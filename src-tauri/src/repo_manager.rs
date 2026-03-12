use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::errors::{FileSystemError, HaalError, NetworkError};
use crate::models::{CollectionEntry, CompetencyEntry, MergedCatalog, SystemEntry};

/// Entry in `repos-manifest.json` — a secondary registry repo.
#[derive(Debug, Clone, Deserialize)]
pub struct RepoSpec {
    /// e.g. "myorg/my-skills"
    pub repo: String,
    /// Branch to clone, defaults to "main"
    #[serde(default = "default_branch")]
    pub branch: String,
    /// Optional GitHub base URL for enterprise instances
    pub base_url: Option<String>,
}

fn default_branch() -> String {
    "main".to_string()
}

#[derive(Debug, Clone, Deserialize)]
struct ReposManifest {
    #[serde(default)]
    repos: Vec<RepoSpec>,
}

/// Minimal subset of a registry's top-level manifest needed for catalog merging.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryManifest {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub collections: Vec<CollectionEntry>,
    #[serde(default)]
    pub competencies: Vec<CompetencyEntry>,
    #[serde(default)]
    pub systems: Vec<SystemEntry>,
}

/// Manages cloning/pulling registry repos and building the merged catalog.
pub struct RepoManager {
    /// Root cache directory — repos are cloned under `<cache_root>/<owner>_<repo>_<branch>/`
    cache_root: PathBuf,
}

impl RepoManager {
    pub fn new(cache_root: PathBuf) -> Self {
        Self { cache_root }
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Clones or pulls a repo and returns its local path.
    /// `repo_spec` format: `"owner/repo"` or `"owner/repo:branch"`.
    pub fn clone_or_pull(&self, repo: &str, branch: &str, base_url: Option<&str>) -> Result<PathBuf, HaalError> {
        let local_path = self.local_path_for(repo, branch);
        let remote_url = Self::build_clone_url(repo, base_url);

        if local_path.join(".git").exists() {
            // Already cloned — try to pull
            self.git_pull(&local_path)?;
        } else {
            // Fresh clone
            std::fs::create_dir_all(&local_path).map_err(|e| HaalError::FileSystem(FileSystemError {
                message: format!("Failed to create cache dir: {e}"),
                path: Some(local_path.display().to_string()),
            }))?;
            self.git_clone(&remote_url, branch, &local_path)?;
        }

        Ok(local_path)
    }

    /// Builds the merged catalog from the seed repo + any additional repos listed
    /// in `repos-manifest.json` inside the seed repo.
    ///
    /// Priority: seed repo wins on conflict (highest priority).
    /// Additional repos are processed lowest-priority first (order in repos-manifest).
    ///
    /// When `include_test_branches` is false (default), repos whose branch name
    /// starts with "test" are silently skipped.
    pub fn build_merged_catalog(&self, seed_local_path: &Path, include_test_branches: bool) -> Result<MergedCatalog, HaalError> {
        // Load additional repos from seed's repos-manifest.json
        let repos_manifest_path = seed_local_path.join("repos-manifest.json");
        let extra_repos: Vec<RepoSpec> = if repos_manifest_path.exists() {
            let content = std::fs::read_to_string(&repos_manifest_path).unwrap_or_default();
            serde_json::from_str::<ReposManifest>(&content)
                .unwrap_or(ReposManifest { repos: vec![] })
                .repos
        } else {
            vec![]
        };

        // Collect (local_path, priority) pairs — lower index = lower priority
        let mut sources: Vec<(PathBuf, u32)> = Vec::new();

        // Clone/pull extra repos first (lower priority)
        for (i, spec) in extra_repos.iter().enumerate() {
            // Skip test branches unless the developer flag is on
            if !include_test_branches && spec.branch.starts_with("test") {
                eprintln!("INFO: Skipping registry {}/{} (test branch)", spec.repo, spec.branch);
                continue;
            }
            match self.clone_or_pull(&spec.repo, &spec.branch, spec.base_url.as_deref()) {
                Ok(path) => sources.push((path, i as u32)),
                Err(e) => {
                    // Non-fatal: log and skip
                    eprintln!("WARN: Failed to clone {}: {e}", spec.repo);
                }
            }
        }

        // Seed repo has highest priority
        sources.push((seed_local_path.to_path_buf(), extra_repos.len() as u32 + 1));

        // Merge: process lowest priority first, seed last (overwrites)
        let mut collections: HashMap<String, CollectionEntry> = HashMap::new();
        let mut competencies: HashMap<String, CompetencyEntry> = HashMap::new();
        let mut competency_sources: HashMap<String, PathBuf> = HashMap::new();
        let mut systems: HashMap<String, SystemEntry> = HashMap::new();

        for (local_path, _priority) in &sources {
            let manifest = self.load_registry_manifest(local_path);
            if let Some(m) = manifest {
                for col in m.collections {
                    collections.insert(col.id.clone(), col);
                }
                for comp in m.competencies {
                    competency_sources.insert(comp.id.clone(), local_path.clone());
                    competencies.insert(comp.id.clone(), comp);
                }
                for sys in m.systems {
                    // First-seen wins for systems (seed has highest priority, processed last)
                    systems.insert(sys.id.clone(), sys);
                }            }
        }

        let mut collections_vec: Vec<CollectionEntry> = collections.into_values().collect();
        let mut competencies_vec: Vec<CompetencyEntry> = competencies.into_values().collect();
        let mut systems_vec: Vec<SystemEntry> = systems.into_values().collect();

        collections_vec.sort_by(|a, b| a.name.cmp(&b.name));
        competencies_vec.sort_by(|a, b| a.name.cmp(&b.name));
        systems_vec.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(MergedCatalog {
            collections: collections_vec,
            competencies: competencies_vec,
            competency_sources,
            systems: systems_vec,
        })
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn local_path_for(&self, repo: &str, branch: &str) -> PathBuf {
        // "owner/repo" → "owner_repo_branch"
        let safe = repo.replace('/', "_");
        self.cache_root.join(format!("{safe}_{branch}"))
    }

    fn build_clone_url(repo: &str, base_url: Option<&str>) -> String {
        let base = base_url.unwrap_or("https://github.com");
        let base = base.trim_end_matches('/');
        format!("{base}/{repo}.git")
    }

    fn git_clone(&self, url: &str, branch: &str, dest: &Path) -> Result<(), HaalError> {
        let output = std::process::Command::new("git")
            .args(["clone", "--depth", "1", "--branch", branch, url, "."])
            .current_dir(dest)
            .output()
            .map_err(|e| HaalError::Network(NetworkError {
                message: format!("git not found or clone failed: {e}"),
                url: Some(url.to_string()),
                status_code: None,
            }))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(HaalError::Network(NetworkError {
                message: format!("git clone failed: {stderr}"),
                url: Some(url.to_string()),
                status_code: None,
            }));
        }
        Ok(())
    }

    fn git_pull(&self, repo_path: &Path) -> Result<(), HaalError> {
        // Non-fatal: if pull fails (offline), we use the cached version
        let _ = std::process::Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(repo_path)
            .output();
        Ok(())
    }

    fn load_registry_manifest(&self, local_path: &Path) -> Option<RegistryManifest> {
        // Try haal_manifest.json first, then collection-manifest.json for legacy
        let candidates = ["haal_manifest.json"];
        for name in &candidates {
            let path = local_path.join(name);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(m) = serde_json::from_str::<RegistryManifest>(&content) {
                        return Some(m);
                    }
                }
            }
        }
        None
    }
}
