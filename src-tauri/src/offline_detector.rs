use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::errors::{HaalError, NetworkError};

/// URL used for connectivity checks — a lightweight HEAD request to the GitHub API.
const CONNECTIVITY_CHECK_URL: &str = "https://api.github.com";

/// Default timeout for the connectivity probe (in seconds).
const CONNECTIVITY_TIMEOUT_SECS: u64 = 5;

/// Callback type invoked when the connectivity status changes.
/// Receives `true` when the network becomes available, `false` when lost.
pub type OnStatusChange = Arc<dyn Fn(bool) + Send + Sync>;

/// Monitors network connectivity by periodically pinging the GitHub API.
///
/// The detector caches the current status in an `AtomicBool` so that
/// `is_online()` is lock-free and can be called from any thread.
/// When the status changes, an optional callback is invoked — the Tauri
/// command layer wires this to emit a `network-status-changed` event.
pub struct OfflineDetector {
    http_client: reqwest::Client,
    is_online_flag: Arc<AtomicBool>,
    on_status_change: Option<OnStatusChange>,
}

impl OfflineDetector {
    /// Creates a new `OfflineDetector` that assumes online until the first check.
    pub fn new() -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(CONNECTIVITY_TIMEOUT_SECS))
            .build()
            .unwrap_or_default();

        Self {
            http_client,
            is_online_flag: Arc::new(AtomicBool::new(true)),
            on_status_change: None,
        }
    }

    /// Creates an `OfflineDetector` with a custom `reqwest::Client` (useful for testing).
    pub fn with_client(http_client: reqwest::Client) -> Self {
        Self {
            http_client,
            is_online_flag: Arc::new(AtomicBool::new(true)),
            on_status_change: None,
        }
    }

    /// Registers a callback that fires whenever the connectivity status changes.
    pub fn set_on_status_change(&mut self, callback: OnStatusChange) {
        self.on_status_change = Some(callback);
    }

    /// Performs a single connectivity check by sending a HEAD request to the
    /// GitHub API endpoint. Updates the cached status and fires the callback
    /// if the status changed.
    ///
    /// Returns `true` if the network is reachable, `false` otherwise.
    pub async fn check_connectivity(&self) -> Result<bool, HaalError> {
        let reachable = self
            .http_client
            .head(CONNECTIVITY_CHECK_URL)
            .send()
            .await
            .map(|resp| resp.status().is_success() || resp.status().is_client_error())
            .unwrap_or(false);

        let previous = self.is_online_flag.swap(reachable, Ordering::SeqCst);

        if previous != reachable {
            if let Some(ref cb) = self.on_status_change {
                cb(reachable);
            }
        }

        Ok(reachable)
    }

    /// Starts periodic connectivity monitoring.
    ///
    /// Runs an infinite loop that calls `check_connectivity()` every
    /// `interval_secs` seconds. This should be spawned on a background
    /// tokio task — it will run until the task is cancelled/aborted.
    pub async fn monitor_connectivity(&self, interval_secs: u64) -> Result<(), HaalError> {
        if interval_secs == 0 {
            return Err(HaalError::Network(NetworkError {
                message: "Monitoring interval must be greater than 0".to_string(),
                url: None,
                status_code: None,
            }));
        }

        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

        loop {
            interval.tick().await;
            // Errors from individual checks are non-fatal for the monitor loop.
            let _ = self.check_connectivity().await;
        }
    }

    /// Returns the current cached connectivity status (lock-free).
    pub fn is_online(&self) -> bool {
        self.is_online_flag.load(Ordering::SeqCst)
    }

    /// Returns a clone of the internal `AtomicBool` flag so other components
    /// can read the status without going through the detector.
    pub fn online_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.is_online_flag)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    // -- Unit tests for cached status logic ----------------------------------

    #[test]
    fn new_detector_defaults_to_online() {
        let detector = OfflineDetector::new();
        assert!(detector.is_online());
    }

    #[test]
    fn online_flag_shares_state() {
        let detector = OfflineDetector::new();
        let flag = detector.online_flag();

        // Mutate via the flag, observe via is_online.
        flag.store(false, Ordering::SeqCst);
        assert!(!detector.is_online());

        flag.store(true, Ordering::SeqCst);
        assert!(detector.is_online());
    }

    #[test]
    fn with_client_defaults_to_online() {
        let client = reqwest::Client::new();
        let detector = OfflineDetector::with_client(client);
        assert!(detector.is_online());
    }

    // -- Callback invocation tests -------------------------------------------

    #[tokio::test]
    async fn callback_fires_on_status_change() {
        // Build a client that will always fail (unreachable address).
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(50))
            .build()
            .unwrap();

        let mut detector = OfflineDetector::with_client(client);

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);
        let received_status = Arc::new(AtomicBool::new(true));
        let received_status_clone = Arc::clone(&received_status);

        detector.set_on_status_change(Arc::new(move |status| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            received_status_clone.store(status, Ordering::SeqCst);
        }));

        // First check — detector starts online, check will fail → status changes to offline.
        let result = detector.check_connectivity().await.unwrap();
        assert!(!result);
        assert!(!detector.is_online());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        assert!(!received_status.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn callback_does_not_fire_when_status_unchanged() {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(50))
            .build()
            .unwrap();

        let mut detector = OfflineDetector::with_client(client);

        // Pre-set to offline so the failing check doesn't change status.
        detector.is_online_flag.store(false, Ordering::SeqCst);

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        detector.set_on_status_change(Arc::new(move |_| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        let _ = detector.check_connectivity().await;

        // Status was already false, check returns false → no change → no callback.
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    // -- monitor_connectivity validation -------------------------------------

    #[tokio::test]
    async fn monitor_rejects_zero_interval() {
        let detector = OfflineDetector::new();
        let result = detector.monitor_connectivity(0).await;
        assert!(result.is_err());
    }

    // -- check_connectivity with unreachable endpoint ------------------------

    #[tokio::test]
    async fn check_connectivity_returns_false_on_timeout() {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(1))
            .build()
            .unwrap();

        let detector = OfflineDetector::with_client(client);
        let online = detector.check_connectivity().await.unwrap();
        assert!(!online);
        assert!(!detector.is_online());
    }
}
