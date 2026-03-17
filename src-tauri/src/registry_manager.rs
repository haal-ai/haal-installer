use std::path::PathBuf;

use crate::errors::{FileSystemError, HaalError, NetworkError};
use crate::models::{CompetencyDetail, CompetencyEntry, HaalManifest};
use crate::parse_competency_json;

/// Default registry URL — points directly to the HAAL manifest JSON.
pub const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/haal-ai/haal-skills/main/haal_manifest.json";

/// Fetches and caches the top-level manifest and competency details.
pub struct RegistryManager {
    registry_url: String,
    cache_dir: PathBuf,
    http_client: reqwest::Client,
}

impl RegistryManager {
    pub fn new(registry_url: String, cache_dir: PathBuf) -> Self {
        Self {
            registry_url,
            cache_dir,
            http_client: reqwest::Client::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Top-level manifest
    // -----------------------------------------------------------------------

    /// Fetches the top-level `HaalManifest`. Falls back to cache on failure.
    /// In debug builds, also tries a local `haal_manifest.json` next to the binary.
    pub async fn fetch_manifest(&self) -> Result<HaalManifest, HaalError> {
        match self.fetch_remote_manifest().await {
            Ok(m) => {
                let _ = self.write_cache("haal_manifest.json", &m);
                Ok(m)
            }
            Err(_) => {
                // Try disk cache first
                if let Ok(Some(cached)) = self.read_cache::<HaalManifest>("haal_manifest.json") {
                    return Ok(cached);
                }
                // Dev fallback: look for haal_manifest.json relative to the workspace
                #[cfg(debug_assertions)]
                if let Some(local) = self.try_load_local_manifest() {
                    return Ok(local);
                }
                Err(HaalError::Network(NetworkError {
                    message: "Registry unreachable and no cached manifest available".to_string(),
                    url: Some(self.registry_url.clone()),
                    status_code: None,
                }))
            }
        }
    }

    /// In debug builds, walks up from the binary location looking for
    /// `haal-skills/haal_manifest.json` (the local copy in the workspace).
    #[cfg(debug_assertions)]
    fn try_load_local_manifest(&self) -> Option<HaalManifest> {
        // Try common dev paths relative to the current exe
        let exe = std::env::current_exe().ok()?;
        let mut dir = exe.parent()?;
        for _ in 0..8 {
            let candidate = dir.join("haal-skills").join("haal_manifest.json");
            if candidate.exists() {
                let content = std::fs::read_to_string(&candidate).ok()?;
                return serde_json::from_str(&content).ok();
            }
            dir = dir.parent()?;
        }
        None
    }

    async fn fetch_remote_manifest(&self) -> Result<HaalManifest, HaalError> {
        let resp = self
            .http_client
            .get(&self.registry_url)
            .send()
            .await
            .map_err(|e| HaalError::Network(NetworkError {
                message: format!("Failed to fetch manifest: {e}"),
                url: Some(self.registry_url.clone()),
                status_code: None,
            }))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(HaalError::Network(NetworkError {
                message: format!("Registry returned HTTP {status}"),
                url: Some(self.registry_url.clone()),
                status_code: Some(status.as_u16()),
            }));
        }

        resp.json::<HaalManifest>().await.map_err(|e| {
            HaalError::Network(NetworkError {
                message: format!("Failed to parse manifest JSON: {e}"),
                url: Some(self.registry_url.clone()),
                status_code: None,
            })
        })
    }

    // -----------------------------------------------------------------------
    // Competency detail (lazy fetch)
    // -----------------------------------------------------------------------

    /// Fetches the full detail for a competency given its entry from the manifest.
    /// Resolves relative `manifest_url` against `base_url`.
    pub async fn fetch_competency(
        &self,
        entry: &CompetencyEntry,
        base_url: &str,
    ) -> Result<CompetencyDetail, HaalError> {
        let url = resolve_url(base_url, &entry.manifest_url);
        let cache_key = format!("competency_{}.json", entry.id);

        match self.fetch_remote_competency(&url).await {
            Ok(detail) => {
                let _ = self.write_cache(&cache_key, &detail);
                Ok(detail)
            }
            Err(_) => {
                if let Ok(Some(cached)) = self.read_cache::<CompetencyDetail>(&cache_key) {
                    return Ok(cached);
                }
                // Dev fallback: load from local haal-skills/competencies/
                #[cfg(debug_assertions)]
                if let Some(local) = self.try_load_local_competency(&entry.manifest_url) {
                    return Ok(local);
                }
                Err(HaalError::Network(NetworkError {
                    message: format!(
                        "Competency '{}' unreachable and no cache available",
                        entry.id
                    ),
                    url: Some(url),
                    status_code: None,
                }))
            }
        }
    }

    #[cfg(debug_assertions)]
    fn try_load_local_competency(&self, manifest_url: &str) -> Option<CompetencyDetail> {
        // manifest_url is like "competencies/developer.json"
        let exe = std::env::current_exe().ok()?;
        let mut dir = exe.parent()?;
        for _ in 0..8 {
            let candidate = dir.join("haal-skills").join(manifest_url.replace('/', std::path::MAIN_SEPARATOR_STR));
            if candidate.exists() {
                let content = std::fs::read_to_string(&candidate).ok()?;
                return parse_competency_json(&content).ok();
            }
            dir = dir.parent()?;
        }
        None
    }

    async fn fetch_remote_competency(&self, url: &str) -> Result<CompetencyDetail, HaalError> {
        let resp = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| HaalError::Network(NetworkError {
                message: format!("Failed to fetch competency: {e}"),
                url: Some(url.to_string()),
                status_code: None,
            }))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(HaalError::Network(NetworkError {
                message: format!("Competency URL returned HTTP {status}"),
                url: Some(url.to_string()),
                status_code: Some(status.as_u16()),
            }));
        }

        resp.text().await.map_err(|e| {
            HaalError::Network(NetworkError {
                message: format!("Failed to read competency response: {e}"),
                url: Some(url.to_string()),
                status_code: None,
            })
        }).and_then(|text| {
            parse_competency_json(&text).map_err(|e| {
                HaalError::Network(NetworkError {
                    message: format!("Failed to parse competency JSON: {e}"),
                    url: Some(url.to_string()),
                    status_code: None,
                })
            })
        })
    }

    // -----------------------------------------------------------------------
    // Cache helpers
    // -----------------------------------------------------------------------

    fn write_cache<T: serde::Serialize>(&self, filename: &str, value: &T) -> Result<(), HaalError> {
        std::fs::create_dir_all(&self.cache_dir).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to create cache dir: {e}"),
                path: Some(self.cache_dir.display().to_string()),
            })
        })?;
        let json = serde_json::to_string_pretty(value).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to serialize: {e}"),
                path: None,
            })
        })?;
        let path = self.cache_dir.join(filename);
        std::fs::write(&path, json).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to write cache: {e}"),
                path: Some(path.display().to_string()),
            })
        })
    }

    fn read_cache<T: serde::de::DeserializeOwned>(
        &self,
        filename: &str,
    ) -> Result<Option<T>, HaalError> {
        let path = self.cache_dir.join(filename);
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to read cache: {e}"),
                path: Some(path.display().to_string()),
            })
        })?;
        serde_json::from_str(&content)
            .map(Some)
            .map_err(|e| HaalError::FileSystem(FileSystemError {
                message: format!("Failed to parse cache: {e}"),
                path: Some(path.display().to_string()),
            }))
    }
}

/// Resolves a potentially relative URL against a base URL.
/// If `url` is already absolute (starts with http), returns it as-is.
fn resolve_url(base: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        let base = base.trim_end_matches('/');
        let url = url.trim_start_matches('/');
        format!("{base}/{url}")
    }
}
