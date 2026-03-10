use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::checksum_validator::ChecksumValidator;
use crate::errors::{FileSystemError, HaalError, ValidationError};
use crate::models::Component;

/// Metadata for a single installed file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub relative_path: String,
    pub checksum: String,
    pub size: u64,
}

/// Summary of file-level changes between two versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeSummary {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
    pub renamed: Vec<(String, String)>,
}

/// Metadata for an installed component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMetadata {
    pub component_id: String,
    pub version: String,
    pub installed_at: DateTime<Utc>,
    pub destinations: Vec<PathBuf>,
    pub tool: String,
    pub checksum: String,
    pub modified: bool,
    pub file_manifest: Vec<FileEntry>,
}

/// Information about an available update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub component_id: String,
    pub installed_version: String,
    pub available_version: String,
}

/// Tracks installed component versions and detects modifications.
pub struct VersionTracker {
    metadata_path: PathBuf,
    pub(crate) installed_components: RwLock<HashMap<String, InstalledMetadata>>,
}

impl VersionTracker {
    /// Creates a new VersionTracker reading from the given metadata path.
    pub fn new(metadata_path: PathBuf) -> Self {
        Self {
            metadata_path,
            installed_components: RwLock::new(HashMap::new()),
        }
    }

    /// Loads installed metadata from disk.
    ///
    /// Reads the JSON file at `metadata_path`. If the file does not exist the
    /// internal map is left empty (fresh install). Any other I/O or parse error
    /// is propagated.
    pub fn load(&self) -> Result<(), HaalError> {
        if !self.metadata_path.exists() {
            // Fresh install — nothing to load.
            return Ok(());
        }

        let data = std::fs::read_to_string(&self.metadata_path).map_err(|e| FileSystemError {
            message: format!("Failed to read metadata file: {e}"),
            path: Some(self.metadata_path.display().to_string()),
        })?;

        let map: HashMap<String, InstalledMetadata> =
            serde_json::from_str(&data).map_err(|e| ValidationError {
                message: format!("Failed to parse metadata JSON: {e}"),
                field: None,
            })?;

        let mut components = self.installed_components.write().map_err(|e| {
            FileSystemError {
                message: format!("Failed to acquire write lock: {e}"),
                path: None,
            }
        })?;
        *components = map;
        Ok(())
    }

    /// Compares installed versions with repository versions.
    ///
    /// For each installed component that also appears in `repo_components` and
    /// whose repo version differs from the installed version, an `UpdateInfo`
    /// is produced. Components without a version in the repo are skipped.
    pub fn check_updates(&self, repo_components: &[Component]) -> Vec<UpdateInfo> {
        let components = match self.installed_components.read() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };

        let mut updates = Vec::new();

        for repo_comp in repo_components {
            let repo_version = match &repo_comp.version {
                Some(v) if !v.is_empty() => v,
                _ => continue,
            };

            if let Some(installed) = components.get(&repo_comp.id) {
                if installed.version != *repo_version {
                    updates.push(UpdateInfo {
                        component_id: repo_comp.id.clone(),
                        installed_version: installed.version.clone(),
                        available_version: repo_version.clone(),
                    });
                }
            }
        }

        updates
    }

    /// Detects local modifications by comparing checksums.
    ///
    /// For each installed component, every file in its `file_manifest` is
    /// checked against the stored checksum. If any file has been changed (or
    /// is missing), the component is flagged as modified.
    pub fn detect_modifications(&self) -> Result<Vec<String>, HaalError> {
        let components = self.installed_components.read().map_err(|e| {
            FileSystemError {
                message: format!("Failed to acquire read lock: {e}"),
                path: None,
            }
        })?;

        let validator = ChecksumValidator::new();
        let mut modified_ids = Vec::new();

        for (id, metadata) in components.iter() {
            let mut is_modified = false;

            for dest in &metadata.destinations {
                for entry in &metadata.file_manifest {
                    let file_path = dest.join(&entry.relative_path);

                    if !file_path.exists() {
                        is_modified = true;
                        break;
                    }

                    match validator.verify_checksum(&file_path, &entry.checksum) {
                        Ok(true) => {} // matches
                        Ok(false) => {
                            is_modified = true;
                            break;
                        }
                        Err(_) => {
                            // Can't read the file — treat as modified.
                            is_modified = true;
                            break;
                        }
                    }
                }

                if is_modified {
                    break;
                }
            }

            if is_modified {
                modified_ids.push(id.clone());
            }
        }

        Ok(modified_ids)
    }

    /// Compares file manifests to detect added/modified/deleted files.
    ///
    /// Looks up the installed component's `file_manifest` and compares it with
    /// `new_files`. Files are matched by `relative_path`.
    ///
    /// - `added`: present in `new_files` but not in the old manifest
    /// - `deleted`: present in the old manifest but not in `new_files`
    /// - `modified`: present in both but with different checksums
    /// - `renamed`: empty (rename detection is not implemented yet)
    pub fn diff_file_manifests(
        &self,
        component_id: &str,
        new_files: &[FileEntry],
    ) -> Result<FileChangeSummary, HaalError> {
        let components = self.installed_components.read().map_err(|e| {
            FileSystemError {
                message: format!("Failed to acquire read lock: {e}"),
                path: None,
            }
        })?;

        let old_files = match components.get(component_id) {
            Some(meta) => &meta.file_manifest,
            None => {
                // No existing manifest — everything is new.
                return Ok(FileChangeSummary {
                    added: new_files.iter().map(|f| f.relative_path.clone()).collect(),
                    modified: Vec::new(),
                    deleted: Vec::new(),
                    renamed: Vec::new(),
                });
            }
        };

        let old_map: HashMap<&str, &FileEntry> = old_files
            .iter()
            .map(|f| (f.relative_path.as_str(), f))
            .collect();

        let new_map: HashMap<&str, &FileEntry> = new_files
            .iter()
            .map(|f| (f.relative_path.as_str(), f))
            .collect();

        let old_keys: HashSet<&str> = old_map.keys().copied().collect();
        let new_keys: HashSet<&str> = new_map.keys().copied().collect();

        let added: Vec<String> = new_keys
            .difference(&old_keys)
            .map(|k| k.to_string())
            .collect();

        let deleted: Vec<String> = old_keys
            .difference(&new_keys)
            .map(|k| k.to_string())
            .collect();

        let modified: Vec<String> = old_keys
            .intersection(&new_keys)
            .filter(|k| old_map[**k].checksum != new_map[**k].checksum)
            .map(|k| k.to_string())
            .collect();

        Ok(FileChangeSummary {
            added,
            modified,
            deleted,
            renamed: Vec::new(),
        })
    }

    /// Detects orphaned components (installed but no longer in any enabled repo).
    ///
    /// Returns metadata for every installed component whose `component_id` does
    /// not appear in `available_components`.
    pub fn detect_orphaned(&self, available_components: &[Component]) -> Vec<InstalledMetadata> {
        let components = match self.installed_components.read() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };

        let available_ids: HashSet<&str> = available_components
            .iter()
            .map(|c| c.id.as_str())
            .collect();

        components
            .values()
            .filter(|meta| !available_ids.contains(meta.component_id.as_str()))
            .cloned()
            .collect()
    }

    /// Updates metadata after a successful operation.
    ///
    /// Creates or replaces the `InstalledMetadata` entry for the given
    /// component and persists the full map to disk.
    pub fn update_metadata(
        &self,
        component: &Component,
        destinations: &[PathBuf],
    ) -> Result<(), HaalError> {
        let metadata = InstalledMetadata {
            component_id: component.id.clone(),
            version: component.version.clone().unwrap_or_default(),
            installed_at: Utc::now(),
            destinations: destinations.to_vec(),
            tool: component
                .compatible_tools
                .first()
                .cloned()
                .unwrap_or_default(),
            checksum: String::new(),
            modified: false,
            file_manifest: Vec::new(),
        };

        {
            let mut components = self.installed_components.write().map_err(|e| {
                FileSystemError {
                    message: format!("Failed to acquire write lock: {e}"),
                    path: None,
                }
            })?;
            components.insert(component.id.clone(), metadata);
        }

        self.save()
    }

    /// Removes metadata for deleted components and persists the change.
    pub fn remove_metadata(&self, component_id: &str) -> Result<(), HaalError> {
        {
            let mut components = self.installed_components.write().map_err(|e| {
                FileSystemError {
                    message: format!("Failed to acquire write lock: {e}"),
                    path: None,
                }
            })?;
            components.remove(component_id);
        }

        self.save()
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Serializes the installed-components map to JSON and writes it to
    /// `metadata_path`, creating parent directories if needed.
    fn save(&self) -> Result<(), HaalError> {
        let components = self.installed_components.read().map_err(|e| {
            FileSystemError {
                message: format!("Failed to acquire read lock: {e}"),
                path: None,
            }
        })?;

        let json = serde_json::to_string_pretty(&*components).map_err(|e| {
            ValidationError {
                message: format!("Failed to serialize metadata: {e}"),
                field: None,
            }
        })?;

        if let Some(parent) = self.metadata_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| FileSystemError {
                    message: format!("Failed to create metadata directory: {e}"),
                    path: Some(parent.display().to_string()),
                })?;
            }
        }

        std::fs::write(&self.metadata_path, json).map_err(|e| FileSystemError {
            message: format!("Failed to write metadata file: {e}"),
            path: Some(self.metadata_path.display().to_string()),
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ComponentType;
    use tempfile::TempDir;

    /// Helper to create a Component with the given id and optional version.
    fn make_component(id: &str, version: Option<&str>) -> Component {
        Component {
            id: id.to_string(),
            name: id.to_string(),
            description: String::new(),
            component_type: ComponentType::Skill,
            path: format!("skills/{id}"),
            compatible_tools: vec!["kiro".to_string()],
            dependencies: Vec::new(),
            pinned: false,
            deprecated: false,
            version: version.map(String::from),
        }
    }

    #[test]
    fn load_missing_file_is_ok() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.json");
        let tracker = VersionTracker::new(path);
        assert!(tracker.load().is_ok());
    }

    #[test]
    fn load_valid_json() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("meta.json");

        let mut map = HashMap::new();
        map.insert(
            "comp-a".to_string(),
            InstalledMetadata {
                component_id: "comp-a".to_string(),
                version: "abc123".to_string(),
                installed_at: Utc::now(),
                destinations: vec![PathBuf::from("/tmp/dest")],
                tool: "kiro".to_string(),
                checksum: String::new(),
                modified: false,
                file_manifest: Vec::new(),
            },
        );
        std::fs::write(&path, serde_json::to_string(&map).unwrap()).unwrap();

        let tracker = VersionTracker::new(path);
        tracker.load().unwrap();

        let comps = tracker.installed_components.read().unwrap();
        assert!(comps.contains_key("comp-a"));
        assert_eq!(comps["comp-a"].version, "abc123");
    }

    #[test]
    fn load_invalid_json_returns_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not valid json!!!").unwrap();

        let tracker = VersionTracker::new(path);
        assert!(tracker.load().is_err());
    }

    #[test]
    fn check_updates_detects_version_change() {
        let dir = TempDir::new().unwrap();
        let tracker = VersionTracker::new(dir.path().join("m.json"));

        // Seed installed component
        {
            let mut comps = tracker.installed_components.write().unwrap();
            comps.insert(
                "comp-a".to_string(),
                InstalledMetadata {
                    component_id: "comp-a".to_string(),
                    version: "old-hash".to_string(),
                    installed_at: Utc::now(),
                    destinations: Vec::new(),
                    tool: "kiro".to_string(),
                    checksum: String::new(),
                    modified: false,
                    file_manifest: Vec::new(),
                },
            );
        }

        let repo = vec![make_component("comp-a", Some("new-hash"))];
        let updates = tracker.check_updates(&repo);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].installed_version, "old-hash");
        assert_eq!(updates[0].available_version, "new-hash");
    }

    #[test]
    fn check_updates_skips_same_version() {
        let dir = TempDir::new().unwrap();
        let tracker = VersionTracker::new(dir.path().join("m.json"));

        {
            let mut comps = tracker.installed_components.write().unwrap();
            comps.insert(
                "comp-a".to_string(),
                InstalledMetadata {
                    component_id: "comp-a".to_string(),
                    version: "same-hash".to_string(),
                    installed_at: Utc::now(),
                    destinations: Vec::new(),
                    tool: "kiro".to_string(),
                    checksum: String::new(),
                    modified: false,
                    file_manifest: Vec::new(),
                },
            );
        }

        let repo = vec![make_component("comp-a", Some("same-hash"))];
        let updates = tracker.check_updates(&repo);
        assert!(updates.is_empty());
    }

    #[test]
    fn check_updates_skips_repo_without_version() {
        let dir = TempDir::new().unwrap();
        let tracker = VersionTracker::new(dir.path().join("m.json"));

        {
            let mut comps = tracker.installed_components.write().unwrap();
            comps.insert(
                "comp-a".to_string(),
                InstalledMetadata {
                    component_id: "comp-a".to_string(),
                    version: "hash".to_string(),
                    installed_at: Utc::now(),
                    destinations: Vec::new(),
                    tool: "kiro".to_string(),
                    checksum: String::new(),
                    modified: false,
                    file_manifest: Vec::new(),
                },
            );
        }

        let repo = vec![make_component("comp-a", None)];
        assert!(tracker.check_updates(&repo).is_empty());
    }

    #[test]
    fn diff_file_manifests_all_categories() {
        let dir = TempDir::new().unwrap();
        let tracker = VersionTracker::new(dir.path().join("m.json"));

        {
            let mut comps = tracker.installed_components.write().unwrap();
            comps.insert(
                "comp-a".to_string(),
                InstalledMetadata {
                    component_id: "comp-a".to_string(),
                    version: "v1".to_string(),
                    installed_at: Utc::now(),
                    destinations: Vec::new(),
                    tool: "kiro".to_string(),
                    checksum: String::new(),
                    modified: false,
                    file_manifest: vec![
                        FileEntry { relative_path: "a.txt".into(), checksum: "aaa".into(), size: 10 },
                        FileEntry { relative_path: "b.txt".into(), checksum: "bbb".into(), size: 20 },
                        FileEntry { relative_path: "c.txt".into(), checksum: "ccc".into(), size: 30 },
                    ],
                },
            );
        }

        let new_files = vec![
            FileEntry { relative_path: "a.txt".into(), checksum: "aaa".into(), size: 10 }, // unchanged
            FileEntry { relative_path: "b.txt".into(), checksum: "bbb_new".into(), size: 25 }, // modified
            FileEntry { relative_path: "d.txt".into(), checksum: "ddd".into(), size: 40 }, // added
            // c.txt is deleted
        ];

        let summary = tracker.diff_file_manifests("comp-a", &new_files).unwrap();
        assert!(summary.added.contains(&"d.txt".to_string()));
        assert!(summary.modified.contains(&"b.txt".to_string()));
        assert!(summary.deleted.contains(&"c.txt".to_string()));
        assert!(summary.renamed.is_empty());
        // a.txt should not appear in any change list
        assert!(!summary.added.contains(&"a.txt".to_string()));
        assert!(!summary.modified.contains(&"a.txt".to_string()));
        assert!(!summary.deleted.contains(&"a.txt".to_string()));
    }

    #[test]
    fn diff_file_manifests_unknown_component_treats_all_as_added() {
        let dir = TempDir::new().unwrap();
        let tracker = VersionTracker::new(dir.path().join("m.json"));

        let new_files = vec![
            FileEntry { relative_path: "x.txt".into(), checksum: "xxx".into(), size: 5 },
        ];

        let summary = tracker.diff_file_manifests("unknown", &new_files).unwrap();
        assert_eq!(summary.added, vec!["x.txt".to_string()]);
        assert!(summary.modified.is_empty());
        assert!(summary.deleted.is_empty());
    }

    #[test]
    fn detect_orphaned_finds_missing_components() {
        let dir = TempDir::new().unwrap();
        let tracker = VersionTracker::new(dir.path().join("m.json"));

        {
            let mut comps = tracker.installed_components.write().unwrap();
            comps.insert("comp-a".into(), InstalledMetadata {
                component_id: "comp-a".into(),
                version: "v1".into(),
                installed_at: Utc::now(),
                destinations: Vec::new(),
                tool: "kiro".into(),
                checksum: String::new(),
                modified: false,
                file_manifest: Vec::new(),
            });
            comps.insert("comp-b".into(), InstalledMetadata {
                component_id: "comp-b".into(),
                version: "v1".into(),
                installed_at: Utc::now(),
                destinations: Vec::new(),
                tool: "kiro".into(),
                checksum: String::new(),
                modified: false,
                file_manifest: Vec::new(),
            });
        }

        // Only comp-a is available
        let available = vec![make_component("comp-a", Some("v1"))];
        let orphaned = tracker.detect_orphaned(&available);
        assert_eq!(orphaned.len(), 1);
        assert_eq!(orphaned[0].component_id, "comp-b");
    }

    #[test]
    fn update_and_remove_metadata_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("meta.json");
        let tracker = VersionTracker::new(path.clone());

        let comp = make_component("comp-a", Some("hash1"));
        let dests = vec![PathBuf::from("/tmp/dest1")];

        tracker.update_metadata(&comp, &dests).unwrap();

        // Verify persisted
        assert!(path.exists());
        let tracker2 = VersionTracker::new(path.clone());
        tracker2.load().unwrap();
        {
            let comps = tracker2.installed_components.read().unwrap();
            assert!(comps.contains_key("comp-a"));
            assert_eq!(comps["comp-a"].version, "hash1");
            assert!(!comps["comp-a"].modified);
        }

        // Remove and verify
        tracker2.remove_metadata("comp-a").unwrap();
        let tracker3 = VersionTracker::new(path);
        tracker3.load().unwrap();
        let comps = tracker3.installed_components.read().unwrap();
        assert!(!comps.contains_key("comp-a"));
    }

    #[test]
    fn detect_modifications_flags_changed_file() {
        let dir = TempDir::new().unwrap();
        let dest = dir.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap();

        // Write a file with known content
        let file_path = dest.join("hello.txt");
        std::fs::write(&file_path, b"original content").unwrap();

        let validator = ChecksumValidator::new();
        let original_checksum = validator.calculate_checksum(&file_path).unwrap();

        let tracker = VersionTracker::new(dir.path().join("m.json"));
        {
            let mut comps = tracker.installed_components.write().unwrap();
            comps.insert("comp-a".into(), InstalledMetadata {
                component_id: "comp-a".into(),
                version: "v1".into(),
                installed_at: Utc::now(),
                destinations: vec![dest.clone()],
                tool: "kiro".into(),
                checksum: String::new(),
                modified: false,
                file_manifest: vec![FileEntry {
                    relative_path: "hello.txt".into(),
                    checksum: original_checksum,
                    size: 16,
                }],
            });
        }

        // Before modification — should be clean
        let modified = tracker.detect_modifications().unwrap();
        assert!(modified.is_empty());

        // Modify the file
        std::fs::write(&file_path, b"changed content").unwrap();

        let modified = tracker.detect_modifications().unwrap();
        assert_eq!(modified, vec!["comp-a".to_string()]);
    }

    #[test]
    fn detect_modifications_flags_missing_file() {
        let dir = TempDir::new().unwrap();
        let dest = dir.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap();

        let tracker = VersionTracker::new(dir.path().join("m.json"));
        {
            let mut comps = tracker.installed_components.write().unwrap();
            comps.insert("comp-a".into(), InstalledMetadata {
                component_id: "comp-a".into(),
                version: "v1".into(),
                installed_at: Utc::now(),
                destinations: vec![dest],
                tool: "kiro".into(),
                checksum: String::new(),
                modified: false,
                file_manifest: vec![FileEntry {
                    relative_path: "gone.txt".into(),
                    checksum: "abc".into(),
                    size: 5,
                }],
            });
        }

        let modified = tracker.detect_modifications().unwrap();
        assert_eq!(modified, vec!["comp-a".to_string()]);
    }
}
