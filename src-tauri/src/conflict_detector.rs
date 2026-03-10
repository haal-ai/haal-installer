use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::checksum_validator::ChecksumValidator;
use crate::errors::{FileSystemError, HaalError};
use crate::models::{Component, Destination};
use crate::version_tracker::VersionTracker;

/// The type of conflict detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictType {
    FileExists {
        path: PathBuf,
        existing_checksum: String,
    },
    VersionMismatch {
        component_id: String,
        installed: String,
        new: String,
    },
    ModifiedLocally {
        component_id: String,
        path: PathBuf,
    },
    DependencyConflict {
        component_id: String,
        required: String,
        available: String,
    },
}

/// How the user chose to resolve a conflict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictResolution {
    Overwrite,
    Skip,
    Backup { timestamp: DateTime<Utc> },
    Merge,
}

/// Identifies file and version conflicts before operations execute.
pub struct ConflictDetector {
    version_tracker: Arc<VersionTracker>,
}

impl ConflictDetector {
    /// Creates a new ConflictDetector backed by the given VersionTracker.
    pub fn new(version_tracker: Arc<VersionTracker>) -> Self {
        Self { version_tracker }
    }

    /// Detects conflicts before operation execution.
    ///
    /// For each component + destination combination:
    /// - Checks if files already exist at the destination path → `FileExists`
    /// - Checks if the component is already installed with a different version → `VersionMismatch`
    /// - Checks if the component has been locally modified → `ModifiedLocally`
    pub fn detect_conflicts(
        &self,
        components: &[Component],
        destinations: &[Destination],
    ) -> Result<Vec<ConflictType>, HaalError> {
        let mut conflicts = Vec::new();
        let checksum_validator = ChecksumValidator::new();

        // Get locally modified component IDs once up front
        let modified_ids = self.version_tracker.detect_modifications()?;

        let installed = self.version_tracker.installed_components.read().map_err(|e| {
            FileSystemError {
                message: format!("Failed to acquire read lock: {e}"),
                path: None,
            }
        })?;

        for component in components {
            // 1. Check FileExists: does a file/dir already exist at destination?
            for dest in destinations {
                let target_path = dest.path.join(&component.id);
                if target_path.exists() {
                    let existing_checksum = if target_path.is_file() {
                        checksum_validator
                            .calculate_checksum(&target_path)
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    conflicts.push(ConflictType::FileExists {
                        path: target_path,
                        existing_checksum,
                    });
                }
            }

            // 2. Check VersionMismatch: is the component installed with a different version?
            if let Some(metadata) = installed.get(&component.id) {
                if let Some(new_version) = &component.version {
                    if !new_version.is_empty() && metadata.version != *new_version {
                        conflicts.push(ConflictType::VersionMismatch {
                            component_id: component.id.clone(),
                            installed: metadata.version.clone(),
                            new: new_version.clone(),
                        });
                    }
                }
            }

            // 3. Check ModifiedLocally: has the component been locally modified?
            if modified_ids.contains(&component.id) {
                if let Some(metadata) = installed.get(&component.id) {
                    let path = metadata
                        .destinations
                        .first()
                        .cloned()
                        .unwrap_or_else(|| PathBuf::from(&component.id));
                    conflicts.push(ConflictType::ModifiedLocally {
                        component_id: component.id.clone(),
                        path,
                    });
                }
            }
        }

        Ok(conflicts)
    }
