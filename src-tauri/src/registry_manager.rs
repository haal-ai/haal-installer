use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::errors::{FileSystemError, HaalError, NetworkError, ValidationError};
use crate::models::Component;

/// Default registry URL for the HAAL official registry.
pub const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/haal-ai/haal-skills/main/haal_manifest.json";

/// Entry in the master manifest describing a component repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryEntry {
    pub id: String,
    pub url: String,
    pub description: String,
    pub category: String,
    pub supported_tools: Vec<String>,
    pub priority: u32,
    pub enabled: bool,
}

/// The master manifest fetched from the registry repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterManifest {
    pub version: String,
    pub registry_name: String,
    pub repositories: Vec<RegistryEntry>,
}

/// A per-repository manifest describing its components, collections, and competencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoManifest {
    pub version: String,
    pub repo_id: String,
    pub components: Vec<Component>,
    pub collections: Vec<crate::models::Collection>,
    pub competencies: Vec<crate::models::Competency>,
}

/// Indicates who pinned a component.
#[derive(Debug, Clone)]
pub enum PinSource {
    /// Pinned by enterprise admin in the registry — cannot be unpinned by users.
    MasterManifest,
    /// Pinned by repo maintainer — can be unpinned by enterprise admin.
    RepoManifest,
}

/// Cache file name for the master manifest.
const MASTER_MANIFEST_CACHE_FILE: &str = "master_manifest.json";

/// Fetches and caches the master manifest, discovers component repositories,
/// and aggregates repo manifests.
pub struct RegistryManager {
    registry_url: String,
    cache_dir: PathBuf,
    http_client: reqwest::Client,
}

impl RegistryManager {
    /// Creates a new RegistryManager with the given registry URL and cache directory.
    pub fn new(registry_url: String, cache_dir: PathBuf) -> Self {
        Self {
            registry_url,
            cache_dir,
            http_client: reqwest::Client::new(),
        }
    }

    /// Returns the path to the cached master manifest file.
    fn master_manifest_cache_path(&self) -> PathBuf {
        self.cache_dir.join(MASTER_MANIFEST_CACHE_FILE)
    }

    /// Returns the path to the cached repo manifest file for a given repo id.
    fn repo_manifest_cache_path(&self, repo_id: &str) -> PathBuf {
        self.cache_dir.join(format!("{repo_id}_manifest.json"))
    }

    /// Writes a repo manifest to the cache directory.
    fn cache_repo_manifest(&self, manifest: &RepoManifest) -> Result<(), HaalError> {
        std::fs::create_dir_all(&self.cache_dir).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to create cache directory: {e}"),
                path: Some(self.cache_dir.display().to_string()),
            })
        })?;

        let json = serde_json::to_string_pretty(manifest).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to serialize repo manifest: {e}"),
                path: None,
            })
        })?;

        let path = self.repo_manifest_cache_path(&manifest.repo_id);
        std::fs::write(&path, json).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to write cached repo manifest: {e}"),
                path: Some(path.display().to_string()),
            })
        })?;

        Ok(())
    }

    /// Fetches the Master_Manifest from the Registry_Repo.
    /// On success, caches the result to disk.
    /// On failure, falls back to the cached version.
    /// If no cache exists either, returns an error.
    pub async fn fetch_master_manifest(&self) -> Result<MasterManifest, HaalError> {
        match self.fetch_remote_master_manifest().await {
            Ok(manifest) => {
                // Cache the freshly fetched manifest (best-effort; ignore write errors)
                let _ = self.cache_master_manifest(&manifest);
                Ok(manifest)
            }
            Err(_fetch_err) => {
                // Offline fallback: try the cached version
                match self.get_cached_master_manifest()? {
                    Some(cached) => Ok(cached),
                    None => Err(HaalError::Network(NetworkError {
                        message: "Registry is unreachable and no cached manifest is available"
                            .to_string(),
                        url: Some(self.registry_url.clone()),
                        status_code: None,
                    })),
                }
            }
        }
    }

    /// Performs the actual HTTP GET to fetch the master manifest.
    async fn fetch_remote_master_manifest(&self) -> Result<MasterManifest, HaalError> {
        let response = self
            .http_client
            .get(&self.registry_url)
            .send()
            .await
            .map_err(|e| {
                HaalError::Network(NetworkError {
                    message: format!("Failed to fetch master manifest: {e}"),
                    url: Some(self.registry_url.clone()),
                    status_code: None,
                })
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(HaalError::Network(NetworkError {
                message: format!("Registry returned HTTP {status}"),
                url: Some(self.registry_url.clone()),
                status_code: Some(status.as_u16()),
            }));
        }

        let manifest: MasterManifest =
            response.json().await.map_err(|e| {
                HaalError::Network(NetworkError {
                    message: format!("Failed to parse master manifest JSON: {e}"),
                    url: Some(self.registry_url.clone()),
                    status_code: None,
                })
            })?;

        Ok(manifest)
    }

    /// Writes the master manifest to the cache directory.
    fn cache_master_manifest(&self, manifest: &MasterManifest) -> Result<(), HaalError> {
        std::fs::create_dir_all(&self.cache_dir).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to create cache directory: {e}"),
                path: Some(self.cache_dir.display().to_string()),
            })
        })?;

        let json = serde_json::to_string_pretty(manifest).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to serialize master manifest: {e}"),
                path: None,
            })
        })?;

        std::fs::write(self.master_manifest_cache_path(), json).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to write cached manifest: {e}"),
                path: Some(self.master_manifest_cache_path().display().to_string()),
            })
        })?;

        Ok(())
    }

    /// Returns cached Master_Manifest if available.
    pub fn get_cached_master_manifest(&self) -> Result<Option<MasterManifest>, HaalError> {
        let path = self.master_manifest_cache_path();
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to read cached manifest: {e}"),
                path: Some(path.display().to_string()),
            })
        })?;

        let manifest: MasterManifest = serde_json::from_str(&content).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to parse cached manifest: {e}"),
                path: Some(path.display().to_string()),
            })
        })?;

        Ok(Some(manifest))
    }

    /// Fetches a Repo_Manifest from a specific repository.
    /// If the URL ends in .json, treats it as a direct manifest URL.
    /// Otherwise constructs the manifest URL as `{repo_url}/raw/main/haal_manifest.json`.
    /// On success, caches the result. On failure, falls back to cached version.
    pub async fn fetch_repo_manifest(&self, repo_url: &str, repo_id: &str) -> Result<RepoManifest, HaalError> {
        let trimmed = repo_url.trim_end_matches('/');
        let manifest_url = if trimmed.ends_with(".json") {
            trimmed.to_string()
        } else {
            format!("{trimmed}/raw/main/haal_manifest.json")
        };

        match self.fetch_remote_repo_manifest(&manifest_url).await {
            Ok(manifest) => {
                let _ = self.cache_repo_manifest(&manifest);
                Ok(manifest)
            }
            Err(_fetch_err) => {
                // Offline fallback: try the cached version
                match self.get_cached_repo_manifest(repo_id)? {
                    Some(cached) => Ok(cached),
                    None => Err(HaalError::Network(NetworkError {
                        message: format!(
                            "Repository manifest is unreachable and no cached manifest is available for '{repo_id}'"
                        ),
                        url: Some(manifest_url),
                        status_code: None,
                    })),
                }
            }
        }
    }

    /// Performs the actual HTTP GET to fetch a repo manifest.
    async fn fetch_remote_repo_manifest(&self, manifest_url: &str) -> Result<RepoManifest, HaalError> {
        let response = self
            .http_client
            .get(manifest_url)
            .send()
            .await
            .map_err(|e| {
                HaalError::Network(NetworkError {
                    message: format!("Failed to fetch repo manifest: {e}"),
                    url: Some(manifest_url.to_string()),
                    status_code: None,
                })
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(HaalError::Network(NetworkError {
                message: format!("Repository returned HTTP {status}"),
                url: Some(manifest_url.to_string()),
                status_code: Some(status.as_u16()),
            }));
        }

        let manifest: RepoManifest = response.json().await.map_err(|e| {
            HaalError::Network(NetworkError {
                message: format!("Failed to parse repo manifest JSON: {e}"),
                url: Some(manifest_url.to_string()),
                status_code: None,
            })
        })?;

        Ok(manifest)
    }

    /// Returns cached Repo_Manifest for a specific repo.
    pub fn get_cached_repo_manifest(
        &self,
        repo_id: &str,
    ) -> Result<Option<RepoManifest>, HaalError> {
        let path = self.repo_manifest_cache_path(repo_id);
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to read cached repo manifest: {e}"),
                path: Some(path.display().to_string()),
            })
        })?;

        let manifest: RepoManifest = serde_json::from_str(&content).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to parse cached repo manifest: {e}"),
                path: Some(path.display().to_string()),
            })
        })?;

        Ok(Some(manifest))
    }

    /// Refreshes all cached manifests — fetches master manifest then all enabled repo manifests.
    pub async fn refresh_all_manifests(&self) -> Result<(), HaalError> {
        let master = self.fetch_master_manifest().await?;

        for repo in &master.repositories {
            if repo.enabled {
                // Best-effort: log failures but continue refreshing other repos
                let _ = self.fetch_repo_manifest(&repo.url, &repo.id).await;
            }
        }

        Ok(())
    }

    /// Aggregates components from all enabled repositories, resolving duplicates by priority.
    /// When the same component ID appears in multiple repos, the highest-priority repo wins
    /// UNLESS a lower-priority version is pinned.
    /// If the registry URL points directly to a repo manifest (.json), skips master manifest.
    pub async fn discover_all_components(&self) -> Result<Vec<Component>, HaalError> {
        // If the registry URL is a direct manifest file, fetch it as a single repo manifest
        if self.registry_url.ends_with(".json") {
            let manifest = self.fetch_remote_repo_manifest(&self.registry_url).await
                .or_else(|_| {
                    // fallback to cache using a stable id derived from the url
                    let id = "default";
                    self.get_cached_repo_manifest(id)
                        .and_then(|opt| opt.ok_or_else(|| HaalError::Network(NetworkError {
                            message: "Registry unreachable and no cache available".to_string(),
                            url: Some(self.registry_url.clone()),
                            status_code: None,
                        })))
                })?;
            // cache it
            let _ = self.cache_repo_manifest(&manifest);
            return Ok(manifest.components);
        }

        let master = self.fetch_master_manifest().await?;

        // Collect (priority, repo_id, component) tuples from all enabled repos
        let mut all_entries: Vec<(u32, String, Component)> = Vec::new();

        for repo in &master.repositories {
            if !repo.enabled {
                continue;
            }
            if let Ok(repo_manifest) = self.fetch_repo_manifest(&repo.url, &repo.id).await {
                for component in repo_manifest.components {
                    all_entries.push((repo.priority, repo.id.clone(), component));
                }
            }
        }

        // Resolve duplicates: pinned components always win, otherwise highest priority wins
        let mut resolved: HashMap<String, (u32, String, Component)> = HashMap::new();

        for (priority, repo_id, component) in all_entries {
            let comp_id = component.id.clone();
            match resolved.get(&comp_id) {
                None => {
                    resolved.insert(comp_id, (priority, repo_id, component));
                }
                Some((existing_priority, _existing_repo_id, existing_component)) => {
                    if component.pinned && !existing_component.pinned {
                        // New component is pinned, existing is not — pinned wins
                        resolved.insert(comp_id, (priority, repo_id, component));
                    } else if !component.pinned && existing_component.pinned {
                        // Existing is pinned, new is not — keep existing
                    } else if component.pinned && existing_component.pinned {
                        // Both pinned: check if one is from master manifest
                        // The one from the master manifest's repo wins; if tie, higher priority wins
                        if priority > *existing_priority {
                            resolved.insert(comp_id, (priority, repo_id, component));
                        }
                    } else {
                        // Neither pinned: highest priority wins
                        if priority > *existing_priority {
                            resolved.insert(comp_id, (priority, repo_id, component));
                        }
                    }
                }
            }
        }

        Ok(resolved.into_values().map(|(_, _, comp)| comp).collect())
    }

    /// Resolves which repo serves a component when it exists in multiple repos.
    /// Pinned components always win regardless of priority; otherwise highest priority wins.
    pub fn resolve_component_source(
        &self,
        component_id: &str,
    ) -> Result<RegistryEntry, HaalError> {
        let master = self.get_cached_master_manifest()?.ok_or_else(|| {
            HaalError::Validation(ValidationError {
                message: "No cached master manifest available".to_string(),
                field: None,
            })
        })?;

        let mut best: Option<(u32, RegistryEntry, bool)> = None; // (priority, entry, pinned)

        for repo in &master.repositories {
            if !repo.enabled {
                continue;
            }
            if let Ok(Some(repo_manifest)) = self.get_cached_repo_manifest(&repo.id) {
                if let Some(component) = repo_manifest.components.iter().find(|c| c.id == component_id) {
                    match &best {
                        None => {
                            best = Some((repo.priority, repo.clone(), component.pinned));
                        }
                        Some((existing_priority, _existing_entry, existing_pinned)) => {
                            if component.pinned && !existing_pinned {
                                best = Some((repo.priority, repo.clone(), component.pinned));
                            } else if !component.pinned && *existing_pinned {
                                // Keep existing pinned
                            } else if repo.priority > *existing_priority {
                                best = Some((repo.priority, repo.clone(), component.pinned));
                            }
                        }
                    }
                }
            }
        }

        best.map(|(_, entry, _)| entry).ok_or_else(|| {
            HaalError::Validation(ValidationError {
                message: format!("Component '{component_id}' not found in any enabled repository"),
                field: Some("component_id".to_string()),
            })
        })
    }

    /// Returns all versions of a component across all repos (for user override, respects pin).
    pub fn get_all_versions(
        &self,
        component_id: &str,
    ) -> Result<Vec<(RegistryEntry, Component)>, HaalError> {
        let master = self.get_cached_master_manifest()?.ok_or_else(|| {
            HaalError::Validation(ValidationError {
                message: "No cached master manifest available".to_string(),
                field: None,
            })
        })?;

        let mut versions: Vec<(RegistryEntry, Component)> = Vec::new();

        for repo in &master.repositories {
            if !repo.enabled {
                continue;
            }
            if let Ok(Some(repo_manifest)) = self.get_cached_repo_manifest(&repo.id) {
                if let Some(component) = repo_manifest.components.iter().find(|c| c.id == component_id) {
                    versions.push((repo.clone(), component.clone()));
                }
            }
        }

        // Sort by priority descending so highest-priority version is first
        versions.sort_by(|a, b| b.0.priority.cmp(&a.0.priority));

        Ok(versions)
    }

    /// Checks if a component is pinned and by which source (MasterManifest vs RepoManifest).
    /// Returns `None` if the component is not pinned in any repo.
    /// Returns `PinSource::MasterManifest` if pinned in the master manifest's own repo entry.
    /// Returns `PinSource::RepoManifest` if pinned in a repo manifest.
    pub fn is_pinned(&self, component_id: &str) -> Result<Option<PinSource>, HaalError> {
        let master = match self.get_cached_master_manifest()? {
            Some(m) => m,
            None => return Ok(None),
        };

        // Collect the set of repo IDs that are listed in the master manifest
        let master_repo_ids: std::collections::HashSet<String> =
            master.repositories.iter().map(|r| r.id.clone()).collect();

        for repo in &master.repositories {
            if !repo.enabled {
                continue;
            }
            if let Ok(Some(repo_manifest)) = self.get_cached_repo_manifest(&repo.id) {
                if let Some(component) = repo_manifest.components.iter().find(|c| c.id == component_id) {
                    if component.pinned {
                        // If this repo is listed in the master manifest, it's a MasterManifest pin
                        // (enterprise admin pinned it). Otherwise it's a RepoManifest pin.
                        if master_repo_ids.contains(&repo.id) {
                            return Ok(Some(PinSource::MasterManifest));
                        } else {
                            return Ok(Some(PinSource::RepoManifest));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Reorders repository priorities. Updates the priority in the cached master manifest.
    pub fn set_repository_priority(
        &self,
        repo_id: &str,
        priority: u32,
    ) -> Result<(), HaalError> {
        let mut manifest = self.get_cached_master_manifest()?.ok_or_else(|| {
            HaalError::Validation(ValidationError {
                message: "No cached master manifest available".to_string(),
                field: None,
            })
        })?;

        let repo = manifest
            .repositories
            .iter_mut()
            .find(|r| r.id == repo_id)
            .ok_or_else(|| {
                HaalError::Validation(ValidationError {
                    message: format!("Repository '{repo_id}' not found in manifest"),
                    field: Some("repo_id".to_string()),
                })
            })?;

        repo.priority = priority;

        self.cache_master_manifest(&manifest)
    }

    /// Overrides the default registry URL (for enterprise use).
    pub fn set_registry_url(&mut self, url: String) {
        self.registry_url = url;
    }

    /// Adds a custom repository not in the master manifest.
    /// If the entry's priority is 0, it defaults to 200.
    /// The entry is appended to the cached master manifest (creating one if needed).
    pub fn add_custom_repository(&self, mut entry: RegistryEntry) -> Result<(), HaalError> {
        // Apply default priority of 200 if not explicitly set (priority 0 treated as unset)
        if entry.priority == 0 {
            entry.priority = 200;
        }

        let mut manifest = self
            .get_cached_master_manifest()?
            .unwrap_or_else(|| MasterManifest {
                version: "1.0".to_string(),
                registry_name: "Custom".to_string(),
                repositories: Vec::new(),
            });

        // Replace if a repo with the same id already exists, otherwise append
        if let Some(existing) = manifest.repositories.iter_mut().find(|r| r.id == entry.id) {
            *existing = entry;
        } else {
            manifest.repositories.push(entry);
        }

        self.cache_master_manifest(&manifest)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: creates a RegistryManager pointing at a temp cache dir.
    fn test_manager(registry_url: &str) -> (RegistryManager, TempDir) {
        let tmp = TempDir::new().unwrap();
        let mgr = RegistryManager::new(registry_url.to_string(), tmp.path().to_path_buf());
        (mgr, tmp)
    }

    /// Helper: builds a sample MasterManifest.
    fn sample_manifest() -> MasterManifest {
        MasterManifest {
            version: "1.0".to_string(),
            registry_name: "Test Registry".to_string(),
            repositories: vec![RegistryEntry {
                id: "test-repo".to_string(),
                url: "https://github.com/org/test-repo".to_string(),
                description: "A test repository".to_string(),
                category: "skills".to_string(),
                supported_tools: vec!["kiro".to_string(), "cursor".to_string()],
                priority: 50,
                enabled: true,
            }],
        }
    }

    #[test]
    fn get_cached_master_manifest_returns_none_when_no_cache() {
        let (mgr, _tmp) = test_manager("https://example.com");
        let result = mgr.get_cached_master_manifest().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn cache_and_retrieve_master_manifest_round_trip() {
        let (mgr, _tmp) = test_manager("https://example.com");
        let manifest = sample_manifest();

        mgr.cache_master_manifest(&manifest).unwrap();

        let cached = mgr.get_cached_master_manifest().unwrap().unwrap();
        assert_eq!(cached.version, "1.0");
        assert_eq!(cached.registry_name, "Test Registry");
        assert_eq!(cached.repositories.len(), 1);
        assert_eq!(cached.repositories[0].id, "test-repo");
        assert_eq!(cached.repositories[0].priority, 50);
    }

    #[test]
    fn cached_manifest_uses_camel_case_json() {
        let (mgr, _tmp) = test_manager("https://example.com");
        let manifest = sample_manifest();
        mgr.cache_master_manifest(&manifest).unwrap();

        let raw = std::fs::read_to_string(mgr.master_manifest_cache_path()).unwrap();
        // Verify camelCase keys are present in the JSON
        assert!(raw.contains("registryName"));
        assert!(raw.contains("supportedTools"));
        // Verify snake_case keys are NOT present
        assert!(!raw.contains("registry_name"));
        assert!(!raw.contains("supported_tools"));
    }

    #[test]
    fn get_cached_master_manifest_reads_camel_case_json() {
        let (mgr, _tmp) = test_manager("https://example.com");

        // Write a camelCase JSON file directly (simulating what a real registry returns)
        std::fs::create_dir_all(&mgr.cache_dir).unwrap();
        let json = r#"{
            "version": "2.0",
            "registryName": "External Registry",
            "repositories": [
                {
                    "id": "ext-repo",
                    "url": "https://github.com/ext/repo",
                    "description": "External",
                    "category": "extensions",
                    "supportedTools": ["copilot"],
                    "priority": 100,
                    "enabled": false
                }
            ]
        }"#;
        std::fs::write(mgr.master_manifest_cache_path(), json).unwrap();

        let cached = mgr.get_cached_master_manifest().unwrap().unwrap();
        assert_eq!(cached.version, "2.0");
        assert_eq!(cached.registry_name, "External Registry");
        assert_eq!(cached.repositories[0].id, "ext-repo");
        assert_eq!(cached.repositories[0].supported_tools, vec!["copilot"]);
        assert!(!cached.repositories[0].enabled);
    }

    #[test]
    fn set_registry_url_updates_url() {
        let (mut mgr, _tmp) = test_manager("https://old.example.com");
        assert_eq!(mgr.registry_url, "https://old.example.com");

        mgr.set_registry_url("https://enterprise.example.com/registry".to_string());
        assert_eq!(mgr.registry_url, "https://enterprise.example.com/registry");
    }

    #[test]
    fn add_custom_repository_creates_manifest_when_none_cached() {
        let (mgr, _tmp) = test_manager("https://example.com");

        let entry = RegistryEntry {
            id: "custom-repo".to_string(),
            url: "https://github.com/custom/repo".to_string(),
            description: "Custom repo".to_string(),
            category: "skills".to_string(),
            supported_tools: vec!["kiro".to_string()],
            priority: 0, // should default to 200
            enabled: true,
        };

        mgr.add_custom_repository(entry).unwrap();

        let cached = mgr.get_cached_master_manifest().unwrap().unwrap();
        assert_eq!(cached.repositories.len(), 1);
        assert_eq!(cached.repositories[0].id, "custom-repo");
        assert_eq!(cached.repositories[0].priority, 200);
    }

    #[test]
    fn add_custom_repository_appends_to_existing_manifest() {
        let (mgr, _tmp) = test_manager("https://example.com");

        // Pre-populate cache with a manifest
        mgr.cache_master_manifest(&sample_manifest()).unwrap();

        let entry = RegistryEntry {
            id: "new-repo".to_string(),
            url: "https://github.com/new/repo".to_string(),
            description: "New repo".to_string(),
            category: "extensions".to_string(),
            supported_tools: vec!["cursor".to_string()],
            priority: 0,
            enabled: true,
        };

        mgr.add_custom_repository(entry).unwrap();

        let cached = mgr.get_cached_master_manifest().unwrap().unwrap();
        assert_eq!(cached.repositories.len(), 2);
        assert_eq!(cached.repositories[0].id, "test-repo");
        assert_eq!(cached.repositories[1].id, "new-repo");
        assert_eq!(cached.repositories[1].priority, 200);
    }

    #[test]
    fn add_custom_repository_replaces_existing_by_id() {
        let (mgr, _tmp) = test_manager("https://example.com");

        mgr.cache_master_manifest(&sample_manifest()).unwrap();

        // Add a repo with the same id as the existing one
        let entry = RegistryEntry {
            id: "test-repo".to_string(),
            url: "https://github.com/org/test-repo-v2".to_string(),
            description: "Updated test repo".to_string(),
            category: "skills".to_string(),
            supported_tools: vec!["kiro".to_string()],
            priority: 150,
            enabled: true,
        };

        mgr.add_custom_repository(entry).unwrap();

        let cached = mgr.get_cached_master_manifest().unwrap().unwrap();
        assert_eq!(cached.repositories.len(), 1);
        assert_eq!(cached.repositories[0].url, "https://github.com/org/test-repo-v2");
        assert_eq!(cached.repositories[0].priority, 150);
    }

    #[test]
    fn add_custom_repository_preserves_explicit_priority() {
        let (mgr, _tmp) = test_manager("https://example.com");

        let entry = RegistryEntry {
            id: "high-pri".to_string(),
            url: "https://github.com/high/pri".to_string(),
            description: "High priority".to_string(),
            category: "skills".to_string(),
            supported_tools: vec!["kiro".to_string()],
            priority: 500, // explicitly set, should NOT be overridden to 200
            enabled: true,
        };

        mgr.add_custom_repository(entry).unwrap();

        let cached = mgr.get_cached_master_manifest().unwrap().unwrap();
        assert_eq!(cached.repositories[0].priority, 500);
    }

    #[tokio::test]
    async fn fetch_master_manifest_falls_back_to_cache_on_network_error() {
        // Point at a URL that will definitely fail
        let (mgr, _tmp) = test_manager("https://localhost:1/nonexistent");

        // Pre-populate the cache
        mgr.cache_master_manifest(&sample_manifest()).unwrap();

        // fetch should fall back to cached version
        let result = mgr.fetch_master_manifest().await.unwrap();
        assert_eq!(result.registry_name, "Test Registry");
        assert_eq!(result.repositories.len(), 1);
    }

    #[tokio::test]
    async fn fetch_master_manifest_errors_when_no_cache_and_offline() {
        let (mgr, _tmp) = test_manager("https://localhost:1/nonexistent");

        // No cache exists — should return an error
        let result = mgr.fetch_master_manifest().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("unreachable"),
            "Expected 'unreachable' in error message, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Helpers for repo manifest tests
    // -----------------------------------------------------------------------

    use crate::models::ComponentType;

    fn make_component(id: &str, pinned: bool) -> Component {
        Component {
            id: id.to_string(),
            name: format!("{id} name"),
            description: format!("{id} description"),
            component_type: ComponentType::Skill,
            path: format!("skills/{id}"),
            compatible_tools: vec!["kiro".to_string()],
            dependencies: vec![],
            pinned,
            deprecated: false,
            version: None,
        }
    }

    fn make_repo_manifest(repo_id: &str, components: Vec<Component>) -> RepoManifest {
        RepoManifest {
            version: "1.0".to_string(),
            repo_id: repo_id.to_string(),
            components,
            collections: vec![],
            competencies: vec![],
        }
    }

    fn make_registry_entry(id: &str, priority: u32) -> RegistryEntry {
        RegistryEntry {
            id: id.to_string(),
            url: format!("https://github.com/org/{id}"),
            description: format!("{id} repo"),
            category: "skills".to_string(),
            supported_tools: vec!["kiro".to_string()],
            priority,
            enabled: true,
        }
    }

    /// Sets up a manager with a master manifest containing the given repos,
    /// and caches repo manifests for each.
    fn setup_multi_repo(
        repos: Vec<(RegistryEntry, RepoManifest)>,
    ) -> (RegistryManager, TempDir) {
        let (mgr, tmp) = test_manager("https://example.com");

        let entries: Vec<RegistryEntry> = repos.iter().map(|(e, _)| e.clone()).collect();
        let master = MasterManifest {
            version: "1.0".to_string(),
            registry_name: "Test".to_string(),
            repositories: entries,
        };
        mgr.cache_master_manifest(&master).unwrap();

        for (_, repo_manifest) in &repos {
            mgr.cache_repo_manifest(repo_manifest).unwrap();
        }

        (mgr, tmp)
    }

    // -----------------------------------------------------------------------
    // Repo manifest cache tests
    // -----------------------------------------------------------------------

    #[test]
    fn get_cached_repo_manifest_returns_none_when_no_cache() {
        let (mgr, _tmp) = test_manager("https://example.com");
        let result = mgr.get_cached_repo_manifest("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn cache_and_retrieve_repo_manifest_round_trip() {
        let (mgr, _tmp) = test_manager("https://example.com");
        let manifest = make_repo_manifest("my-repo", vec![make_component("comp-a", false)]);

        mgr.cache_repo_manifest(&manifest).unwrap();

        let cached = mgr.get_cached_repo_manifest("my-repo").unwrap().unwrap();
        assert_eq!(cached.repo_id, "my-repo");
        assert_eq!(cached.components.len(), 1);
        assert_eq!(cached.components[0].id, "comp-a");
    }

    #[tokio::test]
    async fn fetch_repo_manifest_falls_back_to_cache_on_network_error() {
        let (mgr, _tmp) = test_manager("https://example.com");

        // Pre-populate cache
        let manifest = make_repo_manifest("cached-repo", vec![make_component("comp-x", false)]);
        mgr.cache_repo_manifest(&manifest).unwrap();

        // Fetch from unreachable URL should fall back to cache
        let result = mgr
            .fetch_repo_manifest("https://localhost:1/nonexistent", "cached-repo")
            .await
            .unwrap();
        assert_eq!(result.repo_id, "cached-repo");
        assert_eq!(result.components[0].id, "comp-x");
    }

    #[tokio::test]
    async fn fetch_repo_manifest_errors_when_no_cache_and_offline() {
        let (mgr, _tmp) = test_manager("https://example.com");

        let result = mgr
            .fetch_repo_manifest("https://localhost:1/nonexistent", "no-cache-repo")
            .await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Priority resolution tests
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_component_source_picks_highest_priority() {
        let (mgr, _tmp) = setup_multi_repo(vec![
            (
                make_registry_entry("low-repo", 10),
                make_repo_manifest("low-repo", vec![make_component("shared-comp", false)]),
            ),
            (
                make_registry_entry("high-repo", 100),
                make_repo_manifest("high-repo", vec![make_component("shared-comp", false)]),
            ),
        ]);

        let source = mgr.resolve_component_source("shared-comp").unwrap();
        assert_eq!(source.id, "high-repo");
    }

    #[test]
    fn resolve_component_source_pinned_beats_higher_priority() {
        let (mgr, _tmp) = setup_multi_repo(vec![
            (
                make_registry_entry("low-repo", 10),
                make_repo_manifest("low-repo", vec![make_component("pinned-comp", true)]),
            ),
            (
                make_registry_entry("high-repo", 100),
                make_repo_manifest("high-repo", vec![make_component("pinned-comp", false)]),
            ),
        ]);

        let source = mgr.resolve_component_source("pinned-comp").unwrap();
        assert_eq!(source.id, "low-repo", "Pinned component in low-priority repo should win");
    }

    #[test]
    fn resolve_component_source_not_found_returns_error() {
        let (mgr, _tmp) = setup_multi_repo(vec![(
            make_registry_entry("repo-a", 50),
            make_repo_manifest("repo-a", vec![make_component("comp-a", false)]),
        )]);

        let result = mgr.resolve_component_source("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn resolve_component_source_skips_disabled_repos() {
        let (mgr, _tmp) = test_manager("https://example.com");

        let mut disabled_entry = make_registry_entry("disabled-repo", 200);
        disabled_entry.enabled = false;
        let enabled_entry = make_registry_entry("enabled-repo", 10);

        let master = MasterManifest {
            version: "1.0".to_string(),
            registry_name: "Test".to_string(),
            repositories: vec![disabled_entry, enabled_entry],
        };
        mgr.cache_master_manifest(&master).unwrap();
        mgr.cache_repo_manifest(&make_repo_manifest(
            "disabled-repo",
            vec![make_component("comp-x", false)],
        ))
        .unwrap();
        mgr.cache_repo_manifest(&make_repo_manifest(
            "enabled-repo",
            vec![make_component("comp-x", false)],
        ))
        .unwrap();

        let source = mgr.resolve_component_source("comp-x").unwrap();
        assert_eq!(source.id, "enabled-repo");
    }

    // -----------------------------------------------------------------------
    // Pinning tests
    // -----------------------------------------------------------------------

    #[test]
    fn is_pinned_returns_none_when_not_pinned() {
        let (mgr, _tmp) = setup_multi_repo(vec![(
            make_registry_entry("repo-a", 50),
            make_repo_manifest("repo-a", vec![make_component("unpinned", false)]),
        )]);

        let result = mgr.is_pinned("unpinned").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn is_pinned_returns_master_manifest_for_listed_repo() {
        let (mgr, _tmp) = setup_multi_repo(vec![(
            make_registry_entry("official-repo", 50),
            make_repo_manifest("official-repo", vec![make_component("sec-scan", true)]),
        )]);

        let result = mgr.is_pinned("sec-scan").unwrap();
        assert!(matches!(result, Some(PinSource::MasterManifest)));
    }

    #[test]
    fn is_pinned_returns_none_when_no_manifest() {
        let (mgr, _tmp) = test_manager("https://example.com");
        let result = mgr.is_pinned("anything").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn is_pinned_returns_none_for_unknown_component() {
        let (mgr, _tmp) = setup_multi_repo(vec![(
            make_registry_entry("repo-a", 50),
            make_repo_manifest("repo-a", vec![make_component("comp-a", true)]),
        )]);

        let result = mgr.is_pinned("nonexistent").unwrap();
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // set_repository_priority tests
    // -----------------------------------------------------------------------

    #[test]
    fn set_repository_priority_updates_cached_manifest() {
        let (mgr, _tmp) = test_manager("https://example.com");
        mgr.cache_master_manifest(&sample_manifest()).unwrap();

        mgr.set_repository_priority("test-repo", 999).unwrap();

        let cached = mgr.get_cached_master_manifest().unwrap().unwrap();
        assert_eq!(cached.repositories[0].priority, 999);
    }

    #[test]
    fn set_repository_priority_errors_for_unknown_repo() {
        let (mgr, _tmp) = test_manager("https://example.com");
        mgr.cache_master_manifest(&sample_manifest()).unwrap();

        let result = mgr.set_repository_priority("nonexistent", 100);
        assert!(result.is_err());
    }

    #[test]
    fn set_repository_priority_errors_when_no_manifest() {
        let (mgr, _tmp) = test_manager("https://example.com");
        let result = mgr.set_repository_priority("any", 100);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // get_all_versions tests
    // -----------------------------------------------------------------------

    #[test]
    fn get_all_versions_returns_all_repos_sorted_by_priority() {
        let (mgr, _tmp) = setup_multi_repo(vec![
            (
                make_registry_entry("low-repo", 10),
                make_repo_manifest("low-repo", vec![make_component("shared", false)]),
            ),
            (
                make_registry_entry("mid-repo", 50),
                make_repo_manifest("mid-repo", vec![make_component("shared", false)]),
            ),
            (
                make_registry_entry("high-repo", 100),
                make_repo_manifest("high-repo", vec![make_component("shared", true)]),
            ),
        ]);

        let versions = mgr.get_all_versions("shared").unwrap();
        assert_eq!(versions.len(), 3);
        // Should be sorted by priority descending
        assert_eq!(versions[0].0.id, "high-repo");
        assert_eq!(versions[1].0.id, "mid-repo");
        assert_eq!(versions[2].0.id, "low-repo");
    }

    #[test]
    fn get_all_versions_returns_empty_for_unknown_component() {
        let (mgr, _tmp) = setup_multi_repo(vec![(
            make_registry_entry("repo-a", 50),
            make_repo_manifest("repo-a", vec![make_component("comp-a", false)]),
        )]);

        let versions = mgr.get_all_versions("nonexistent").unwrap();
        assert!(versions.is_empty());
    }

    #[test]
    fn get_all_versions_skips_disabled_repos() {
        let (mgr, _tmp) = test_manager("https://example.com");

        let mut disabled = make_registry_entry("disabled-repo", 200);
        disabled.enabled = false;
        let enabled = make_registry_entry("enabled-repo", 10);

        let master = MasterManifest {
            version: "1.0".to_string(),
            registry_name: "Test".to_string(),
            repositories: vec![disabled, enabled],
        };
        mgr.cache_master_manifest(&master).unwrap();
        mgr.cache_repo_manifest(&make_repo_manifest(
            "disabled-repo",
            vec![make_component("comp-x", false)],
        ))
        .unwrap();
        mgr.cache_repo_manifest(&make_repo_manifest(
            "enabled-repo",
            vec![make_component("comp-x", false)],
        ))
        .unwrap();

        let versions = mgr.get_all_versions("comp-x").unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].0.id, "enabled-repo");
    }
}
