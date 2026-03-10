use std::path::PathBuf;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::checksum_validator::ChecksumValidator;
use crate::errors::{FileSystemError, HaalError};

/// A single backed-up file entry.
#[derive(Debug, Clone)]
pub struct BackupEntry {
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub checksum: String,
}

/// A snapshot of the system state before an operation.
#[derive(Debug, Clone)]
pub struct RestorePoint {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub operation: String,
    pub backed_up_files: Vec<BackupEntry>,
    pub metadata_snapshot: Vec<u8>,
}

/// Creates restore points and handles rollback on operation failure.
pub struct RollbackManager {
    backup_dir: PathBuf,
    restore_points: RwLock<Vec<RestorePoint>>,
}

impl RollbackManager {
    /// Creates a new RollbackManager storing backups in the given directory.
    pub fn new(backup_dir: PathBuf) -> Self {
        Self {
            backup_dir,
            restore_points: RwLock::new(Vec::new()),
        }
    }

    /// Creates a restore point before an operation.
    ///
    /// Backs up each file to `backup_dir/{timestamp}/`, calculates checksums,
    /// and snapshots the installed metadata file.
    pub fn create_restore_point(
        &self,
        operation: &str,
        files: Vec<PathBuf>,
    ) -> Result<RestorePoint, HaalError> {
        let now = Utc::now();
        let timestamp = now.format("%Y%m%d_%H%M%S_%3f").to_string();
        let snapshot_dir = self.backup_dir.join(&timestamp);

        std::fs::create_dir_all(&snapshot_dir).map_err(|e| FileSystemError {
            message: format!("Failed to create backup directory: {e}"),
            path: Some(snapshot_dir.display().to_string()),
        })?;

        let validator = ChecksumValidator::new();
        let mut backed_up_files = Vec::new();

        for file_path in &files {
            if !file_path.exists() {
                continue;
            }

            // Preserve relative structure by using the file name (or a sanitised
            // representation of the full path) inside the snapshot directory.
            let file_name = file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // To avoid collisions when multiple files share the same name, include
            // a hash of the full original path as a prefix.
            let path_hash = {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(file_path.to_string_lossy().as_bytes());
                let result = hasher.finalize();
                format!("{:x}", result).chars().take(8).collect::<String>()
            };

            let backup_name = format!("{path_hash}_{file_name}");
            let backup_path = snapshot_dir.join(&backup_name);

            std::fs::copy(file_path, &backup_path).map_err(|e| FileSystemError {
                message: format!("Failed to backup file: {e}"),
                path: Some(file_path.display().to_string()),
            })?;

            let checksum = validator.calculate_checksum(&backup_path)?;

            backed_up_files.push(BackupEntry {
                original_path: file_path.clone(),
                backup_path,
                checksum,
            });
        }

        // Snapshot installed metadata if it exists.
        let haal_home = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".haal");
        let metadata_path = haal_home.join("data").join("installed_metadata.json");
        let metadata_snapshot = if metadata_path.exists() {
            std::fs::read(&metadata_path).unwrap_or_default()
        } else {
            Vec::new()
        };

        let restore_point = RestorePoint {
            id: Uuid::new_v4(),
            created_at: now,
            operation: operation.to_string(),
            backed_up_files,
            metadata_snapshot,
        };

        // Store the restore point in memory.
        if let Ok(mut points) = self.restore_points.write() {
            points.push(restore_point.clone());
        }

        tracing::info!(
            operation = operation,
            restore_point_id = %restore_point.id,
            files_backed_up = restore_point.backed_up_files.len(),
            "Created restore point"
        );

        Ok(restore_point)
    }

    /// Restores the system to a previous restore point.
    ///
    /// Copies each backed-up file back to its original location and restores
    /// the metadata snapshot.
    pub async fn rollback(&self, restore_point: &RestorePoint) -> Result<(), HaalError> {
        tracing::info!(
            restore_point_id = %restore_point.id,
            operation = %restore_point.operation,
            "Starting rollback"
        );

        for entry in &restore_point.backed_up_files {
            // Ensure the parent directory exists before restoring.
            if let Some(parent) = entry.original_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| FileSystemError {
                    message: format!("Failed to create parent directory during rollback: {e}"),
                    path: Some(parent.display().to_string()),
                })?;
            }

            std::fs::copy(&entry.backup_path, &entry.original_path).map_err(|e| {
                FileSystemError {
                    message: format!("Failed to restore file during rollback: {e}"),
                    path: Some(entry.original_path.display().to_string()),
                }
            })?;
        }

        // Restore metadata snapshot if non-empty.
        if !restore_point.metadata_snapshot.is_empty() {
            let haal_home = dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".haal");
            let metadata_path = haal_home.join("data").join("installed_metadata.json");

            if let Some(parent) = metadata_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| FileSystemError {
                    message: format!("Failed to create metadata directory during rollback: {e}"),
                    path: Some(parent.display().to_string()),
                })?;
            }

            std::fs::write(&metadata_path, &restore_point.metadata_snapshot).map_err(|e| {
                FileSystemError {
                    message: format!("Failed to restore metadata during rollback: {e}"),
                    path: Some(metadata_path.display().to_string()),
                }
            })?;
        }

        tracing::info!(
            restore_point_id = %restore_point.id,
            files_restored = restore_point.backed_up_files.len(),
            "Rollback completed"
        );

        Ok(())
    }

    /// Verifies rollback success by comparing checksums.
    ///
    /// For each backed-up file, verifies the file at the original path matches
    /// the stored checksum. Returns `true` if all files match.
    pub fn verify_rollback(&self, restore_point: &RestorePoint) -> Result<bool, HaalError> {
        let validator = ChecksumValidator::new();

        for entry in &restore_point.backed_up_files {
            if !entry.original_path.exists() {
                tracing::warn!(
                    path = %entry.original_path.display(),
                    "File missing after rollback"
                );
                return Ok(false);
            }

            let matches = validator.verify_checksum(&entry.original_path, &entry.checksum)?;
            if !matches {
                tracing::warn!(
                    path = %entry.original_path.display(),
                    expected = %entry.checksum,
                    "Checksum mismatch after rollback"
                );
                return Ok(false);
            }
        }

        tracing::info!(
            restore_point_id = %restore_point.id,
            "Rollback verification passed"
        );

        Ok(true)
    }

    /// Cleans up old restore points, keeping the N most recent.
    ///
    /// Lists timestamped directories in `backup_dir`, sorts by name (which
    /// encodes creation time), and removes the oldest ones.
    pub fn cleanup_old_restore_points(&self, keep_count: usize) -> Result<(), HaalError> {
        if !self.backup_dir.exists() {
            return Ok(());
        }

        let mut entries: Vec<PathBuf> = std::fs::read_dir(&self.backup_dir)
            .map_err(|e| FileSystemError {
                message: format!("Failed to read backup directory: {e}"),
                path: Some(self.backup_dir.display().to_string()),
            })?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.path())
            .collect();

        // Sort by directory name ascending (oldest first since names are timestamps).
        entries.sort();

        if entries.len() <= keep_count {
            return Ok(());
        }

        let to_remove = entries.len() - keep_count;
        for dir in entries.iter().take(to_remove) {
            std::fs::remove_dir_all(dir).map_err(|e| FileSystemError {
                message: format!("Failed to remove old restore point: {e}"),
                path: Some(dir.display().to_string()),
            })?;

            tracing::info!(path = %dir.display(), "Removed old restore point");
        }

        // Also clean up the in-memory list.
        if let Ok(mut points) = self.restore_points.write() {
            if points.len() > keep_count {
                let drain_count = points.len() - keep_count;
                points.drain(..drain_count);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Helper: create a file with given content inside a directory.
    fn create_file(dir: &std::path::Path, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content).unwrap();
        path
    }

    #[test]
    fn create_restore_point_backs_up_files() {
        let backup_dir = TempDir::new().unwrap();
        let source_dir = TempDir::new().unwrap();

        let file1 = create_file(source_dir.path(), "a.txt", b"hello");
        let file2 = create_file(source_dir.path(), "b.txt", b"world");

        let mgr = RollbackManager::new(backup_dir.path().to_path_buf());
        let rp = mgr
            .create_restore_point("test-install", vec![file1.clone(), file2.clone()])
            .unwrap();

        assert_eq!(rp.operation, "test-install");
        assert_eq!(rp.backed_up_files.len(), 2);

        // Verify backup files exist and have correct checksums.
        let validator = ChecksumValidator::new();
        for entry in &rp.backed_up_files {
            assert!(entry.backup_path.exists());
            let checksum = validator.calculate_checksum(&entry.backup_path).unwrap();
            assert_eq!(checksum, entry.checksum);
        }
    }

    #[test]
    fn create_restore_point_skips_nonexistent_files() {
        let backup_dir = TempDir::new().unwrap();
        let mgr = RollbackManager::new(backup_dir.path().to_path_buf());

        let rp = mgr
            .create_restore_point(
                "test",
                vec![PathBuf::from("/nonexistent/file.txt")],
            )
            .unwrap();

        assert!(rp.backed_up_files.is_empty());
    }

    #[tokio::test]
    async fn rollback_restores_files() {
        let backup_dir = TempDir::new().unwrap();
        let source_dir = TempDir::new().unwrap();

        let file1 = create_file(source_dir.path(), "a.txt", b"original");

        let mgr = RollbackManager::new(backup_dir.path().to_path_buf());
        let rp = mgr
            .create_restore_point("test", vec![file1.clone()])
            .unwrap();

        // Modify the original file.
        std::fs::write(&file1, b"modified").unwrap();
        assert_eq!(std::fs::read(&file1).unwrap(), b"modified");

        // Rollback should restore original content.
        mgr.rollback(&rp).await.unwrap();
        assert_eq!(std::fs::read(&file1).unwrap(), b"original");
    }

    #[tokio::test]
    async fn verify_rollback_returns_true_when_checksums_match() {
        let backup_dir = TempDir::new().unwrap();
        let source_dir = TempDir::new().unwrap();

        let file1 = create_file(source_dir.path(), "a.txt", b"data");

        let mgr = RollbackManager::new(backup_dir.path().to_path_buf());
        let rp = mgr
            .create_restore_point("test", vec![file1.clone()])
            .unwrap();

        // Modify then rollback.
        std::fs::write(&file1, b"changed").unwrap();
        mgr.rollback(&rp).await.unwrap();

        assert!(mgr.verify_rollback(&rp).unwrap());
    }

    #[test]
    fn verify_rollback_returns_false_when_file_missing() {
        let backup_dir = TempDir::new().unwrap();
        let source_dir = TempDir::new().unwrap();

        let file1 = create_file(source_dir.path(), "a.txt", b"data");

        let mgr = RollbackManager::new(backup_dir.path().to_path_buf());
        let rp = mgr
            .create_restore_point("test", vec![file1.clone()])
            .unwrap();

        // Delete the original file.
        std::fs::remove_file(&file1).unwrap();

        assert!(!mgr.verify_rollback(&rp).unwrap());
    }

    #[test]
    fn verify_rollback_returns_false_when_checksum_mismatch() {
        let backup_dir = TempDir::new().unwrap();
        let source_dir = TempDir::new().unwrap();

        let file1 = create_file(source_dir.path(), "a.txt", b"data");

        let mgr = RollbackManager::new(backup_dir.path().to_path_buf());
        let rp = mgr
            .create_restore_point("test", vec![file1.clone()])
            .unwrap();

        // Modify the file without rolling back.
        std::fs::write(&file1, b"tampered").unwrap();

        assert!(!mgr.verify_rollback(&rp).unwrap());
    }

    #[test]
    fn cleanup_old_restore_points_keeps_n_most_recent() {
        let backup_dir = TempDir::new().unwrap();

        // Create 5 timestamped directories.
        for i in 0..5 {
            let name = format!("20240101_00000{i}_000", i = i);
            std::fs::create_dir(backup_dir.path().join(&name)).unwrap();
        }

        let mgr = RollbackManager::new(backup_dir.path().to_path_buf());
        mgr.cleanup_old_restore_points(2).unwrap();

        let remaining: Vec<_> = std::fs::read_dir(backup_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn cleanup_old_restore_points_noop_when_fewer_than_keep() {
        let backup_dir = TempDir::new().unwrap();

        std::fs::create_dir(backup_dir.path().join("20240101_000000_000")).unwrap();

        let mgr = RollbackManager::new(backup_dir.path().to_path_buf());
        mgr.cleanup_old_restore_points(5).unwrap();

        let remaining: Vec<_> = std::fs::read_dir(backup_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn cleanup_old_restore_points_noop_when_dir_missing() {
        let mgr = RollbackManager::new(PathBuf::from("/nonexistent/backup/dir"));
        assert!(mgr.cleanup_old_restore_points(3).is_ok());
    }
}
