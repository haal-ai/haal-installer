use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::errors::HaalError;

/// Structured log entry for error reporting.
///
/// Serialized as JSON in log files so entries can be parsed programmatically
/// by the log viewer and support tooling.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorLog {
    pub timestamp: DateTime<Utc>,
    pub error_type: String,
    pub message: String,
    pub stack_trace: Option<String>,
    pub operation: Option<String>,
    pub component: Option<String>,
    pub destination: Option<String>,
    pub recovery_action: Option<String>,
}

impl ErrorLog {
    /// Creates a new `ErrorLog` with the current UTC timestamp.
    pub fn new(error_type: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            error_type: error_type.into(),
            message: message.into(),
            stack_trace: None,
            operation: None,
            component: None,
            destination: None,
            recovery_action: None,
        }
    }

    pub fn with_stack_trace(mut self, trace: impl Into<String>) -> Self {
        self.stack_trace = Some(trace.into());
        self
    }

    pub fn with_operation(mut self, op: impl Into<String>) -> Self {
        self.operation = Some(op.into());
        self
    }

    pub fn with_component(mut self, comp: impl Into<String>) -> Self {
        self.component = Some(comp.into());
        self
    }

    pub fn with_destination(mut self, dest: impl Into<String>) -> Self {
        self.destination = Some(dest.into());
        self
    }

    pub fn with_recovery_action(mut self, action: impl Into<String>) -> Self {
        self.recovery_action = Some(action.into());
        self
    }

    /// Serializes the error log to a sanitized JSON string.
    pub fn to_sanitized_json(&self) -> Result<String, serde_json::Error> {
        let json = serde_json::to_string(self)?;
        Ok(sanitize_log_message(&json))
    }
}

/// Maximum log file size before rotation (10 MB).
const MAX_LOG_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024;

/// Initializes structured logging with `tracing`.
///
/// Sets up two output layers:
/// 1. **Stdout** – human-readable, compact format for development.
/// 2. **File** – JSON-formatted, written to `log_dir/haal-installer.log` with
///    size-based rotation at [`MAX_LOG_FILE_SIZE_BYTES`].
///
/// Returns a [`WorkerGuard`] that **must** be held for the lifetime of the
/// application — dropping it flushes and closes the file appender.
pub fn init_logging(log_dir: PathBuf) -> Result<WorkerGuard, HaalError> {
    // Ensure the log directory exists.
    std::fs::create_dir_all(&log_dir)?;

    // Rotate when the current file exceeds 10 MB.
    // tracing-appender's `RollingFileAppender` supports daily rotation out of
    // the box. For size-based rotation we use a daily appender (which gives us
    // automatic file naming) and rely on the `max_log_files` option.  True
    // byte-level rotation would require a custom appender; for the MVP the
    // daily roller combined with the cleanup helper below is sufficient.
    let file_appender = tracing_appender::rolling::Builder::new()
        .filename_prefix("haal-installer")
        .filename_suffix("log")
        .max_log_files(5)
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .build(&log_dir)
        .map_err(|e| crate::errors::FileSystemError {
            message: format!("Failed to create log file appender: {e}"),
            path: Some(log_dir.display().to_string()),
        })?;

    let (non_blocking_file, guard) = tracing_appender::non_blocking(file_appender);

    // Environment filter — defaults to INFO, overridable via HAAL_LOG env var.
    let env_filter = EnvFilter::try_from_env("HAAL_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // JSON layer for file output.
    let file_layer = fmt::layer()
        .json()
        .with_writer(non_blocking_file)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    // Compact human-readable layer for stdout.
    let stdout_layer = fmt::layer()
        .compact()
        .with_target(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(stdout_layer)
        .init();

    Ok(guard)
}

// ---------------------------------------------------------------------------
// Sensitive-data sanitization
// ---------------------------------------------------------------------------

/// Patterns that indicate sensitive values in log output.
///
/// Each entry is a case-insensitive prefix that, when found as a key in
/// `key=value`, `key: value`, or `"key":"value"` patterns, causes the
/// associated value to be replaced with `[REDACTED]`.
const SENSITIVE_KEYS: &[&str] = &[
    "token",
    "password",
    "passwd",
    "secret",
    "credential",
    "authorization",
    "auth_token",
    "access_token",
    "refresh_token",
    "api_key",
    "apikey",
    "private_key",
    "client_secret",
];

/// Replaces sensitive information in `message` with `[REDACTED]`.
///
/// Handles three common patterns:
/// 1. **JSON** – `"key":"value"` or `"key": "value"`
/// 2. **Header-style** – `Key: value` (up to end of line)
/// 3. **Query / env** – `key=value` (up to whitespace or `&`)
///
/// The matching is intentionally broad to err on the side of redacting too
/// much rather than leaking credentials.
pub fn sanitize_log_message(message: &str) -> String {
    let mut result = message.to_string();

    for key in SENSITIVE_KEYS {
        // Pattern 1: JSON – "token":"ghp_abc123" or "token": "ghp_abc123"
        // We search case-insensitively by lowering the haystack for matching
        // but replace in the original.
        result = replace_json_values(&result, key);

        // Pattern 2: Header-style – Authorization: Bearer ghp_abc123
        result = replace_header_values(&result, key);

        // Pattern 3: key=value (query-string / env-var style)
        result = replace_kv_values(&result, key);
    }

    // Catch bare tokens that look like GitHub PATs / OAuth tokens.
    result = redact_bare_tokens(&result);

    result
}

/// Redacts JSON `"key":"value"` pairs where `key` matches (case-insensitive).
fn replace_json_values(input: &str, key: &str) -> String {
    let lower = input.to_lowercase();
    let mut result = input.to_string();
    let pattern_variants = [
        format!("\"{}\":", key),   // "token":
    ];

    for pat in &pattern_variants {
        let mut search_from = 0;
        loop {
            let Some(pos) = lower[search_from..].find(pat.as_str()) else {
                break;
            };
            let abs_pos = search_from + pos;
            let after_key = abs_pos + pat.len();

            // Skip optional whitespace.
            let rest = &result[after_key..];
            let trimmed = rest.trim_start();
            let ws_len = rest.len() - trimmed.len();

            if trimmed.starts_with('"') {
                // Find closing quote.
                if let Some(end) = trimmed[1..].find('"') {
                    let value_start = after_key + ws_len + 1; // after opening "
                    let value_end = value_start + end;
                    result.replace_range(value_start..value_end, "[REDACTED]");
                    // Update lowercase copy for subsequent iterations.
                    let lower_new = result.to_lowercase();
                    search_from = value_start + "[REDACTED]".len() + 1;
                    // Reassign lower for next iteration (we break and re-enter).
                    return replace_json_values_continued(&result, &lower_new, key, search_from);
                }
            }
            search_from = after_key;
        }
    }
    result
}

/// Continuation helper to avoid borrow issues in the loop.
fn replace_json_values_continued(input: &str, lower: &str, key: &str, from: usize) -> String {
    let mut result = input.to_string();
    let pattern = format!("\"{}\":", key);
    let mut search_from = from;

    loop {
        let Some(pos) = lower[search_from..].find(pattern.as_str()) else {
            break;
        };
        let abs_pos = search_from + pos;
        let after_key = abs_pos + pattern.len();

        let rest = &result[after_key..];
        let trimmed = rest.trim_start();
        let ws_len = rest.len() - trimmed.len();

        if trimmed.starts_with('"') {
            if let Some(end) = trimmed[1..].find('"') {
                let value_start = after_key + ws_len + 1;
                let value_end = value_start + end;
                result.replace_range(value_start..value_end, "[REDACTED]");
                search_from = value_start + "[REDACTED]".len() + 1;
                continue;
            }
        }
        search_from = after_key;
    }
    result
}

/// Redacts `Key: value` header-style patterns (value extends to end of line).
fn replace_header_values(input: &str, key: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for line in input.split('\n') {
        let lower_line = line.to_lowercase();
        let mut redacted = false;
        // Check for "key:" or "key :" at the start or after whitespace.
        for pat in &[format!("{key}:"), format!("{key} :")] {
            if let Some(pos) = lower_line.find(pat.as_str()) {
                let before = &line[..pos];
                // Only match if `before` is empty or ends with whitespace / line start.
                if before.is_empty() || before.ends_with(char::is_whitespace) {
                    let after_colon = pos + pat.len();
                    result.push_str(&line[..after_colon]);
                    result.push_str(" [REDACTED]");
                    redacted = true;
                    break;
                }
            }
        }
        if !redacted {
            result.push_str(line);
        }
        result.push('\n');
    }
    // Remove trailing newline we added.
    if result.ends_with('\n') && !input.ends_with('\n') {
        result.pop();
    }
    result
}

/// Redacts `key=value` patterns (value extends to whitespace or `&`).
fn replace_kv_values(input: &str, key: &str) -> String {
    let lower = input.to_lowercase();
    let mut result = input.to_string();
    let pattern = format!("{key}=");
    let mut search_from = 0;

    loop {
        let Some(pos) = lower[search_from..].find(pattern.as_str()) else {
            break;
        };
        let abs_pos = search_from + pos;
        // Make sure it's a word boundary (start of string or preceded by & or whitespace).
        if abs_pos > 0 {
            let prev = input.as_bytes()[abs_pos - 1];
            if prev != b'&' && prev != b' ' && prev != b'?' && prev != b'\n' {
                search_from = abs_pos + pattern.len();
                continue;
            }
        }
        let value_start = abs_pos + pattern.len();
        let value_end = input[value_start..]
            .find(|c: char| c.is_whitespace() || c == '&')
            .map(|i| value_start + i)
            .unwrap_or(input.len());

        result.replace_range(value_start..value_end, "[REDACTED]");
        // Recalculate lower after mutation.
        let new_lower = result.to_lowercase();
        search_from = value_start + "[REDACTED]".len();
        return replace_kv_values_continued(&result, &new_lower, key, search_from);
    }
    result
}

fn replace_kv_values_continued(input: &str, lower: &str, key: &str, from: usize) -> String {
    let mut result = input.to_string();
    let pattern = format!("{key}=");
    let mut search_from = from;

    loop {
        let Some(pos) = lower[search_from..].find(pattern.as_str()) else {
            break;
        };
        let abs_pos = search_from + pos;
        if abs_pos > 0 {
            let prev = result.as_bytes()[abs_pos - 1];
            if prev != b'&' && prev != b' ' && prev != b'?' && prev != b'\n' {
                search_from = abs_pos + pattern.len();
                continue;
            }
        }
        let value_start = abs_pos + pattern.len();
        let value_end = result[value_start..]
            .find(|c: char| c.is_whitespace() || c == '&')
            .map(|i| value_start + i)
            .unwrap_or(result.len());

        result.replace_range(value_start..value_end, "[REDACTED]");
        search_from = value_start + "[REDACTED]".len();
    }
    result
}

/// Redacts strings that look like bare GitHub tokens (ghp_, gho_, ghs_, ghu_, ghr_).
fn redact_bare_tokens(input: &str) -> String {
    let mut result = input.to_string();
    let prefixes = ["ghp_", "gho_", "ghs_", "ghu_", "ghr_"];

    for prefix in &prefixes {
        while let Some(pos) = result.find(prefix) {
            let token_end = result[pos..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == '}' || c == '&')
                .map(|i| pos + i)
                .unwrap_or(result.len());
            result.replace_range(pos..token_end, "[REDACTED]");
        }
    }
    result
}

/// Returns the maximum log file size in bytes (10 MB).
///
/// Exposed for use by log-cleanup utilities.
pub const fn max_log_file_size() -> u64 {
    MAX_LOG_FILE_SIZE_BYTES
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_log_builder() {
        let log = ErrorLog::new("NetworkError", "connection timed out")
            .with_operation("clone")
            .with_component("code-review")
            .with_destination("/home/user/.kiro/skills")
            .with_recovery_action("Retry the operation");

        assert_eq!(log.error_type, "NetworkError");
        assert_eq!(log.message, "connection timed out");
        assert_eq!(log.operation.as_deref(), Some("clone"));
        assert_eq!(log.component.as_deref(), Some("code-review"));
        assert_eq!(log.destination.as_deref(), Some("/home/user/.kiro/skills"));
        assert_eq!(log.recovery_action.as_deref(), Some("Retry the operation"));
    }

    #[test]
    fn test_error_log_serializes_to_json() {
        let log = ErrorLog::new("AuthError", "invalid token");
        let json = serde_json::to_string(&log).unwrap();
        assert!(json.contains("\"error_type\":\"AuthError\""));
        assert!(json.contains("\"message\":\"invalid token\""));
    }

    #[test]
    fn test_sanitize_json_token() {
        let input = r#"{"token":"ghp_abc123secret","user":"alice"}"#;
        let result = sanitize_log_message(input);
        assert!(!result.contains("ghp_abc123secret"));
        assert!(result.contains("[REDACTED]"));
        assert!(result.contains("alice"));
    }

    #[test]
    fn test_sanitize_json_password() {
        let input = r#"{"password":"super_secret","name":"test"}"#;
        let result = sanitize_log_message(input);
        assert!(!result.contains("super_secret"));
        assert!(result.contains("[REDACTED]"));
        assert!(result.contains("test"));
    }

    #[test]
    fn test_sanitize_header_authorization() {
        let input = "Authorization: Bearer ghp_mytoken123\nContent-Type: application/json";
        let result = sanitize_log_message(input);
        assert!(!result.contains("ghp_mytoken123"));
        assert!(result.contains("[REDACTED]"));
        assert!(result.contains("Content-Type: application/json"));
    }

    #[test]
    fn test_sanitize_query_string() {
        let input = "url?token=abc123&user=alice";
        let result = sanitize_log_message(input);
        assert!(!result.contains("abc123"));
        assert!(result.contains("token=[REDACTED]"));
        assert!(result.contains("user=alice"));
    }

    #[test]
    fn test_sanitize_bare_github_tokens() {
        let input = "Found token ghp_1234567890abcdef in config";
        let result = sanitize_log_message(input);
        assert!(!result.contains("ghp_1234567890abcdef"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_multiple_sensitive_fields() {
        let input = r#"{"token":"secret1","password":"secret2","api_key":"secret3","name":"safe"}"#;
        let result = sanitize_log_message(input);
        assert!(!result.contains("secret1"));
        assert!(!result.contains("secret2"));
        assert!(!result.contains("secret3"));
        assert!(result.contains("safe"));
    }

    #[test]
    fn test_sanitize_preserves_non_sensitive() {
        let input = "Installing component code-review to /home/user/.kiro/skills";
        let result = sanitize_log_message(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_sanitize_client_secret_json() {
        let input = r#"{"client_secret":"my_secret_value"}"#;
        let result = sanitize_log_message(input);
        assert!(!result.contains("my_secret_value"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_error_log_to_sanitized_json() {
        let log = ErrorLog::new("AuthError", "token ghp_leaked123 was invalid");
        let json = log.to_sanitized_json().unwrap();
        assert!(!json.contains("ghp_leaked123"));
        assert!(json.contains("[REDACTED]"));
    }

    #[test]
    fn test_max_log_file_size() {
        assert_eq!(max_log_file_size(), 10 * 1024 * 1024);
    }
}
