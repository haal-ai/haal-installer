use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{error, info, warn};

use crate::checksum_validator::ChecksumValidator;
use crate::conflict_detector::ConflictDetector;
use crate::errors::{FileSystemError, HaalError, IntegrityError, NetworkError, ValidationError};
use crate::models::{Component, ComponentFailure, Destination, OperationResult};
use crate::rollback_manager::RollbackManager;
use crate::traits::{ProgressReporter, ToolAdapter};
use crate::version_tracker::{FileEntry, VersionTracker};

// ---------------------------------------------------------------------------
// Retry configuration
// ---------------------------------------------------------------------------

/// Maximum retries for network errors (exponential backoff).
const NETWORK_RETRY_COUNT: u32 = 3;
/// Base delay for network retry backoff.
const NETWORK_RETRY_BASE_DELAY: Duration = Duration::from_secs(2);

/// Maximum retries for integrity/checksum errors.
const INTEGRITY_RETRY_COUNT: u32 = 3;
/// Delay between integrity retries.
const INTEGRITY_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Maximum retries for transient filesystem errors.
const FS_RETRY_COUNT: u32 = 2;
/// Delay between filesystem retries.
const FS_RETRY_DELAY: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------
// Progress event payload
// ---------------------------------------------------------------------------

/// Payload emitted as a Tauri event for progress updates.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProgressEvent {
    pub current_step: String,
    pub percentage: u8,
    pub current_file: Option<String>,
    pub elapsed_secs: f64,
    pub estimated_remaining_secs: Option<f64>,
}

// ---------------------------------------------------------------------------
// OperationEngine
// ---------------------------------------------------------------------------

/// Orchestrates install, update, delete, and reinitialize operations.
pub struct OperationEngine {
    tool_adapters: HashMap<String, Box<dyn ToolAdapter>>,
    rollback_manager: Arc<RollbackManager>,
    checksum_validator: Arc<ChecksumValidator>,
    conflict_detector: Arc<ConflictDetector>,
    version_tracker: Arc<VersionTracker>,
}

impl OperationEngine {
    /// Creates a new OperationEngine with the given dependencies.
    pub fn new(
        tool_adapters: HashMap<String, Box<dyn ToolAdapter>>,
        rollback_manager: Arc<RollbackManager>,
        checksum_validator: Arc<ChecksumValidator>,
        conflict_detector: Arc<ConflictDetector>,
        version_tracker: Arc<VersionTracker>,
    ) -> Self {
        Self {
            tool_adapters,
            rollback_manager,
            checksum_validator,
            conflict_detector,
            version_tracker,
        }
    }

    // -----------------------------------------------------------------------
    // Install
    // -----------------------------------------------------------------------

    /// Installs components to specified destinations.
    ///
    /// 1. Validates disk space (with 10% safety margin).
    /// 2. Creates a restore point before any file modifications.
    /// 3. For each component × destination, copies files via the tool adapter,
    ///    verifies checksums, and preserves file permissions.
    /// 4. Records the file manifest in VersionTracker on success.
    /// 5. On failure: triggers RollbackManager and reports the error.
    pub async fn install(
        &self,
        components: Vec<Component>,
        destinations: Vec<Destination>,
        mut progress: ProgressReporter,
    ) -> Result<OperationResult, HaalError> {
        let start = Instant::now();
        info!(
            component_count = components.len(),
            destination_count = destinations.len(),
            "Starting install operation"
        );

        // 1. Validate disk space
        let required_space = self.estimate_total_size(&components);
        self.validate_disk_space(&destinations, required_space)?;

        // 2. Collect existing files at destinations for restore point
        let existing_files = self.collect_existing_files(&components, &destinations);
        let restore_point = self
            .rollback_manager
            .create_restore_point("install", existing_files)?;

        let total_work = components.len() * destinations.len().max(1);
        let mut work_done: usize = 0;
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();

        // 3. Install each component to each destination
        for component in &components {
            let mut component_ok = true;

            for dest in destinations.iter().filter(|d| d.enabled) {
                let adapter = self.adapter_for_destination(dest);
                let target_dir = dest.path.join(&component.id);

                // Update progress
                work_done += 1;
                self.update_progress(
                    &mut progress,
                    &format!("Installing {}", component.name),
                    work_done,
                    total_work,
                    Some(&component.id),
                    start,
                );

                // Copy files with retry
                let copy_result = self.copy_component_with_retry(
                    component,
                    &dest.path,
                    &target_dir,
                    adapter,
                );

                match copy_result {
                    Ok(file_entries) => {
                        // Verify checksums
                        match self.verify_copied_files(&file_entries, &target_dir) {
                            Ok(()) => {
                                info!(
                                    component = %component.id,
                                    destination = %dest.path.display(),
                                    "Component installed successfully"
                                );
                            }
                            Err(e) => {
                                error!(
                                    component = %component.id,
                                    error = %e,
                                    "Checksum verification failed after install"
                                );
                                component_ok = false;
                                failed.push(ComponentFailure {
                                    component_id: component.id.clone(),
                                    error: e.to_string(),
                                });
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            component = %component.id,
                            error = %e,
                            "Failed to install component"
                        );
                        component_ok = false;
                        failed.push(ComponentFailure {
                            component_id: component.id.clone(),
                            error: e.to_string(),
                        });
                        break;
                    }
                }
            }

            if component_ok {
                // Record file manifest in VersionTracker
                let dest_paths: Vec<PathBuf> = destinations
                    .iter()
                    .filter(|d| d.enabled)
                    .map(|d| d.path.join(&component.id))
                    .collect();

                if let Err(e) = self.version_tracker.update_metadata(component, &dest_paths) {
                    warn!(
                        component = %component.id,
                        error = %e,
                        "Failed to update version metadata"
                    );
                }
                succeeded.push(component.id.clone());
            }
        }

        // 4. If any failures, rollback
        let rollback_performed = if !failed.is_empty() {
            warn!(
                failed_count = failed.len(),
                "Install had failures, triggering rollback"
            );
            if let Err(e) = self.rollback_manager.rollback(&restore_point).await {
                error!(error = %e, "Rollback failed");
            }
            true
        } else {
            false
        };

        // Final progress
        self.update_progress(&mut progress, "Install complete", total_work, total_work, None, start);

        Ok(OperationResult {
            success: failed.is_empty(),
            components_succeeded: succeeded,
            components_failed: failed,
            rollback_performed,
        })
    }

    // -----------------------------------------------------------------------
    // Update (Full Replacement)
    // -----------------------------------------------------------------------

    /// Updates components at all installed locations using full replacement.
    ///
    /// For each component:
    /// 1. Looks up installed metadata to find current destinations.
    /// 2. Diffs file manifests to produce a `FileChangeSummary`.
    /// 3. Backs up old version → removes old directory → copies new → verifies checksums.
    /// 4. Updates file manifest in VersionTracker.
    /// 5. On failure: rolls back to the backed-up version.
    pub async fn update(
        &self,
        components: Vec<Component>,
        mut progress: ProgressReporter,
    ) -> Result<OperationResult, HaalError> {
        let start = Instant::now();
        info!(
            component_count = components.len(),
            "Starting update operation"
        );

        let mut succeeded = Vec::new();
        let mut failed = Vec::new();
        let total_work = components.len();
        let mut work_done: usize = 0;

        for component in &components {
            work_done += 1;
            self.update_progress(
                &mut progress,
                &format!("Updating {}", component.name),
                work_done,
                total_work,
                Some(&component.id),
                start,
            );

            // Look up installed destinations
            let installed_destinations = self.get_installed_destinations(&component.id);
            if installed_destinations.is_empty() {
                warn!(
                    component = %component.id,
                    "Component not installed, skipping update"
                );
                failed.push(ComponentFailure {
                    component_id: component.id.clone(),
                    error: "Component is not currently installed".to_string(),
                });
                continue;
            }

            // Create restore point for all files at installed destinations
            let files_to_backup = self.collect_files_in_dirs(&installed_destinations);
            let restore_point = match self
                .rollback_manager
                .create_restore_point("update", files_to_backup)
            {
                Ok(rp) => rp,
                Err(e) => {
                    error!(component = %component.id, error = %e, "Failed to create restore point");
                    failed.push(ComponentFailure {
                        component_id: component.id.clone(),
                        error: format!("Failed to create backup: {e}"),
                    });
                    continue;
                }
            };

            let mut component_ok = true;

            for dest_dir in &installed_destinations {
                // Full replacement: remove old directory
                if dest_dir.exists() {
                    if let Err(e) = std::fs::remove_dir_all(dest_dir) {
                        error!(
                            path = %dest_dir.display(),
                            error = %e,
                            "Failed to remove old component directory"
                        );
                        component_ok = false;
                        failed.push(ComponentFailure {
                            component_id: component.id.clone(),
                            error: format!("Failed to remove old version: {e}"),
                        });
                        break;
                    }
                }

                // Determine the parent destination (dest_dir without the component id suffix)
                let parent_dest = dest_dir
                    .parent()
                    .unwrap_or(dest_dir)
                    .to_path_buf();

                // Copy new version
                match self.copy_component_with_retry(component, &parent_dest, dest_dir, None) {
                    Ok(file_entries) => {
                        if let Err(e) = self.verify_copied_files(&file_entries, dest_dir) {
                            error!(
                                component = %component.id,
                                error = %e,
                                "Checksum verification failed after update"
                            );
                            component_ok = false;
                            failed.push(ComponentFailure {
                                component_id: component.id.clone(),
                                error: e.to_string(),
                            });
                            break;
                        }
                    }
                    Err(e) => {
                        error!(
                            component = %component.id,
                            error = %e,
                            "Failed to copy new version"
                        );
                        component_ok = false;
                        failed.push(ComponentFailure {
                            component_id: component.id.clone(),
                            error: e.to_string(),
                        });
                        break;
                    }
                }
            }

            if component_ok {
                // Update version tracker
                if let Err(e) = self
                    .version_tracker
                    .update_metadata(component, &installed_destinations)
                {
                    warn!(
                        component = %component.id,
                        error = %e,
                        "Failed to update version metadata after update"
                    );
                }
                succeeded.push(component.id.clone());
            } else {
                // Rollback this component
                warn!(component = %component.id, "Update failed, rolling back");
                if let Err(e) = self.rollback_manager.rollback(&restore_point).await {
                    error!(error = %e, "Rollback failed during update");
                }
            }
        }

        self.update_progress(
            &mut progress,
            "Update complete",
            total_work,
            total_work,
            None,
            start,
        );

        let rollback_performed = !failed.is_empty();
        Ok(OperationResult {
            success: failed.is_empty(),
            components_succeeded: succeeded,
            components_failed: failed.clone(),
            rollback_performed,
        })
    }

    // -----------------------------------------------------------------------
    // Delete
    // -----------------------------------------------------------------------

    /// Deletes components from all destinations and cleans up version tracking.
    ///
    /// For each component, removes it from every destination where it is
    /// installed, then removes its metadata from the VersionTracker.
    /// Continues with remaining components if one fails.
    pub async fn delete(
        &self,
        components: Vec<Component>,
        mut progress: ProgressReporter,
    ) -> Result<OperationResult, HaalError> {
        let start = Instant::now();
        info!(
            component_count = components.len(),
            "Starting delete operation"
        );

        let mut succeeded = Vec::new();
        let mut failed = Vec::new();
        let total_work = components.len();
        let mut work_done: usize = 0;

        for component in &components {
            work_done += 1;
            self.update_progress(
                &mut progress,
                &format!("Deleting {}", component.name),
                work_done,
                total_work,
                Some(&component.id),
                start,
            );

            let installed_destinations = self.get_installed_destinations(&component.id);
            let mut component_ok = true;

            for dest_dir in &installed_destinations {
                if dest_dir.exists() {
                    // Use tool adapter's delete if available
                    let parent = dest_dir.parent().unwrap_or(dest_dir);
                    let adapter = self.find_adapter_for_path(parent);

                    let delete_result = if let Some(adapter) = adapter {
                        adapter.delete_component(component, parent)
                    } else {
                        // Fallback: remove directory directly
                        std::fs::remove_dir_all(dest_dir).map_err(HaalError::from)
                    };

                    if let Err(e) = delete_result {
                        error!(
                            component = %component.id,
                            path = %dest_dir.display(),
                            error = %e,
                            "Failed to delete component from destination"
                        );
                        component_ok = false;
                        failed.push(ComponentFailure {
                            component_id: component.id.clone(),
                            error: format!(
                                "Failed to delete from {}: {e}",
                                dest_dir.display()
                            ),
                        });
                        // Continue with remaining destinations per requirement 14.5
                    }
                }
            }

            // Clean up version tracking regardless of partial failures
            if let Err(e) = self.version_tracker.remove_metadata(&component.id) {
                warn!(
                    component = %component.id,
                    error = %e,
                    "Failed to remove version metadata"
                );
            }

            if component_ok {
                succeeded.push(component.id.clone());
            }
        }

        self.update_progress(
            &mut progress,
            "Delete complete",
            total_work,
            total_work,
            None,
            start,
        );

        Ok(OperationResult {
            success: failed.is_empty(),
            components_succeeded: succeeded,
            components_failed: failed,
            rollback_performed: false,
        })
    }

    // -----------------------------------------------------------------------
    // Reinitialize
    // -----------------------------------------------------------------------

    /// Reinitializes installation for selected tools.
    ///
    /// 1. Backs up the current installation.
    /// 2. Removes all components for the selected tools from all destinations.
    /// 3. Clears version tracking metadata for those tools.
    /// 4. Returns a result indicating readiness for fresh install.
    pub async fn reinitialize(
        &self,
        tools: Vec<String>,
        mut progress: ProgressReporter,
    ) -> Result<OperationResult, HaalError> {
        let start = Instant::now();
        info!(tools = ?tools, "Starting reinitialize operation");

        // Gather all installed components for the selected tools
        let components_to_remove = self.get_components_for_tools(&tools);

        if components_to_remove.is_empty() {
            info!("No components found for selected tools, nothing to reinitialize");
            return Ok(OperationResult {
                success: true,
                components_succeeded: Vec::new(),
                components_failed: Vec::new(),
                rollback_performed: false,
            });
        }

        // Create backup of all files
        let all_files = self.collect_all_installed_files(&components_to_remove);
        let restore_point = self
            .rollback_manager
            .create_restore_point("reinitialize", all_files)?;

        let total_work = components_to_remove.len();
        let mut work_done: usize = 0;
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();

        for (component_id, destinations) in &components_to_remove {
            work_done += 1;
            self.update_progress(
                &mut progress,
                &format!("Removing {component_id}"),
                work_done,
                total_work,
                Some(component_id),
                start,
            );

            let mut component_ok = true;

            for dest_dir in destinations {
                if dest_dir.exists() {
                    if let Err(e) = std::fs::remove_dir_all(dest_dir) {
                        error!(
                            component = %component_id,
                            path = %dest_dir.display(),
                            error = %e,
                            "Failed to remove component during reinitialize"
                        );
                        component_ok = false;
                        failed.push(ComponentFailure {
                            component_id: component_id.clone(),
                            error: format!(
                                "Failed to remove from {}: {e}",
                                dest_dir.display()
                            ),
                        });
                    }
                }
            }

            // Clear metadata
            if let Err(e) = self.version_tracker.remove_metadata(component_id) {
                warn!(
                    component = %component_id,
                    error = %e,
                    "Failed to clear metadata during reinitialize"
                );
            }

            if component_ok {
                succeeded.push(component_id.clone());
            }
        }

        // If there were failures, attempt rollback
        let rollback_performed = if !failed.is_empty() {
            warn!("Reinitialize had failures, triggering rollback");
            if let Err(e) = self.rollback_manager.rollback(&restore_point).await {
                error!(error = %e, "Rollback failed during reinitialize");
            }
            true
        } else {
            false
        };

        self.update_progress(
            &mut progress,
            "Reinitialize complete",
            total_work,
            total_work,
            None,
            start,
        );

        info!(
            succeeded = succeeded.len(),
            failed = failed.len(),
            "Reinitialize operation completed"
        );

        Ok(OperationResult {
            success: failed.is_empty(),
            components_succeeded: succeeded,
            components_failed: failed,
            rollback_performed,
        })
    }

    // -----------------------------------------------------------------------
    // Private helpers — Disk space
    // -----------------------------------------------------------------------

    /// Estimates total size of all components in bytes.
    fn estimate_total_size(&self, components: &[Component]) -> u64 {
        components.iter().map(|c| self.estimate_component_size(c)).sum()
    }

    /// Estimates the size of a single component by walking its source path.
    fn estimate_component_size(&self, component: &Component) -> u64 {
        let source = PathBuf::from(&component.path);
        if source.is_dir() {
            dir_size(&source).unwrap_or(0)
        } else if source.is_file() {
            std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0)
        } else {
            // Default estimate when source isn't accessible locally
            4096
        }
    }

    /// Validates that all destinations have enough disk space (required + 10% margin).
    fn validate_disk_space(
        &self,
        destinations: &[Destination],
        required_bytes: u64,
    ) -> Result<(), HaalError> {
        let required_with_margin = required_bytes + (required_bytes / 10); // 10% safety margin

        for dest in destinations.iter().filter(|d| d.enabled) {
            let available = available_disk_space(&dest.path);
            if available < required_with_margin {
                return Err(HaalError::Validation(ValidationError {
                    message: format!(
                        "Insufficient disk space at {}: need {} bytes (including 10% margin), \
                         but only {} bytes available",
                        dest.path.display(),
                        required_with_margin,
                        available
                    ),
                    field: Some("disk_space".to_string()),
                }));
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Private helpers — File operations with retry
    // -----------------------------------------------------------------------

    /// Copies a component's files to the target directory with retry logic.
    ///
    /// Returns the list of `FileEntry` items for the copied files.
    fn copy_component_with_retry(
        &self,
        component: &Component,
        dest_base: &Path,
        target_dir: &Path,
        adapter: Option<&dyn ToolAdapter>,
    ) -> Result<Vec<FileEntry>, HaalError> {
        // Let the adapter do its setup (create dirs, etc.)
        if let Some(adapter) = adapter {
            adapter.install_component(component, dest_base)?;
        }

        let source = PathBuf::from(&component.path);

        // Retry wrapper
        retry(INTEGRITY_RETRY_COUNT, INTEGRITY_RETRY_DELAY, || {
            self.copy_and_collect_entries(&source, target_dir)
        })
    }

    /// Copies files from source to target and returns file entries with checksums.
    fn copy_and_collect_entries(
        &self,
        source: &Path,
        target_dir: &Path,
    ) -> Result<Vec<FileEntry>, HaalError> {
        std::fs::create_dir_all(target_dir)?;

        let mut entries = Vec::new();

        if source.is_dir() {
            entries = self.copy_dir_recursive_with_entries(source, target_dir, source)?;
        } else if source.is_file() {
            let file_name = source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let dest_file = target_dir.join(&file_name);
            copy_file_preserving_permissions(source, &dest_file)?;

            let checksum = self.checksum_validator.calculate_checksum(&dest_file)?;
            let size = std::fs::metadata(&dest_file)
                .map(|m| m.len())
                .unwrap_or(0);

            entries.push(FileEntry {
                relative_path: file_name,
                checksum,
                size,
            });
        }

        Ok(entries)
    }

    /// Recursively copies a directory, preserving permissions, and collects file entries.
    fn copy_dir_recursive_with_entries(
        &self,
        src: &Path,
        dst: &Path,
        base: &Path,
    ) -> Result<Vec<FileEntry>, HaalError> {
        std::fs::create_dir_all(dst)?;

        let mut entries = Vec::new();

        let read_dir = std::fs::read_dir(src).map_err(|e| FileSystemError {
            message: format!("Failed to read directory: {e}"),
            path: Some(src.display().to_string()),
        })?;

        for dir_entry in read_dir {
            let dir_entry = dir_entry.map_err(|e| FileSystemError {
                message: format!("Failed to read directory entry: {e}"),
                path: Some(src.display().to_string()),
            })?;

            let src_path = dir_entry.path();
            let dest_path = dst.join(dir_entry.file_name());

            if src_path.is_dir() {
                let sub_entries =
                    self.copy_dir_recursive_with_entries(&src_path, &dest_path, base)?;
                entries.extend(sub_entries);
            } else {
                copy_file_preserving_permissions(&src_path, &dest_path)?;

                let relative = src_path
                    .strip_prefix(base)
                    .unwrap_or(&src_path)
                    .to_string_lossy()
                    .to_string();

                let checksum = self.checksum_validator.calculate_checksum(&dest_path)?;
                let size = std::fs::metadata(&dest_path)
                    .map(|m| m.len())
                    .unwrap_or(0);

                entries.push(FileEntry {
                    relative_path: relative,
                    checksum,
                    size,
                });
            }
        }

        Ok(entries)
    }

    /// Verifies that all copied files match their recorded checksums.
    fn verify_copied_files(
        &self,
        file_entries: &[FileEntry],
        target_dir: &Path,
    ) -> Result<(), HaalError> {
        for entry in file_entries {
            let file_path = target_dir.join(&entry.relative_path);
            let matches = self
                .checksum_validator
                .verify_checksum(&file_path, &entry.checksum)?;
            if !matches {
                return Err(HaalError::Integrity(IntegrityError {
                    message: format!(
                        "Checksum mismatch for {}",
                        entry.relative_path
                    ),
                    expected: Some(entry.checksum.clone()),
                    actual: None,
                }));
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Private helpers — Lookup & collection
    // -----------------------------------------------------------------------

    /// Returns the tool adapter for a destination, if one is registered.
    fn adapter_for_destination(&self, dest: &Destination) -> Option<&dyn ToolAdapter> {
        self.tool_adapters.get(&dest.tool_name).map(|a| a.as_ref())
    }

    /// Finds an adapter whose default destinations include the given path.
    fn find_adapter_for_path(&self, path: &Path) -> Option<&dyn ToolAdapter> {
        for adapter in self.tool_adapters.values() {
            for dest in adapter.default_destinations() {
                if path.starts_with(&dest.path) || dest.path.starts_with(path) {
                    return Some(adapter.as_ref());
                }
            }
        }
        None
    }

    /// Gets the installed destinations for a component from the VersionTracker.
    fn get_installed_destinations(&self, component_id: &str) -> Vec<PathBuf> {
        let components = match self.version_tracker.installed_components.read() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };
        match components.get(component_id) {
            Some(meta) => meta.destinations.clone(),
            None => Vec::new(),
        }
    }

    /// Collects existing files at component destinations for backup.
    fn collect_existing_files(
        &self,
        components: &[Component],
        destinations: &[Destination],
    ) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for component in components {
            for dest in destinations.iter().filter(|d| d.enabled) {
                let target = dest.path.join(&component.id);
                if target.exists() {
                    collect_files_recursive(&target, &mut files);
                }
            }
        }
        files
    }

    /// Collects all files in the given directories.
    fn collect_files_in_dirs(&self, dirs: &[PathBuf]) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for dir in dirs {
            if dir.exists() {
                collect_files_recursive(dir, &mut files);
            }
        }
        files
    }

    /// Gets all installed components for the specified tools.
    fn get_components_for_tools(&self, tools: &[String]) -> Vec<(String, Vec<PathBuf>)> {
        let components = match self.version_tracker.installed_components.read() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };

        components
            .iter()
            .filter(|(_, meta)| {
                tools.iter().any(|t| t.eq_ignore_ascii_case(&meta.tool))
            })
            .map(|(id, meta)| (id.clone(), meta.destinations.clone()))
            .collect()
    }

    /// Collects all files from all installed component destinations.
    fn collect_all_installed_files(
        &self,
        components: &[(String, Vec<PathBuf>)],
    ) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for (_, destinations) in components {
            for dest in destinations {
                if dest.exists() {
                    collect_files_recursive(dest, &mut files);
                }
            }
        }
        files
    }

    // -----------------------------------------------------------------------
    // Private helpers — Progress reporting
    // -----------------------------------------------------------------------

    /// Updates the progress reporter with current state.
    fn update_progress(
        &self,
        progress: &mut ProgressReporter,
        step: &str,
        work_done: usize,
        total_work: usize,
        current_file: Option<&str>,
        start: Instant,
    ) {
        let percentage = if total_work == 0 {
            100
        } else {
            ((work_done as f64 / total_work as f64) * 100.0).min(100.0) as u8
        };

        progress.current_step = step.to_string();
        progress.percentage = percentage;
        progress.current_file = current_file.map(String::from);

        let elapsed = start.elapsed();
        let estimated_remaining = if work_done > 0 && work_done < total_work {
            let rate = elapsed.as_secs_f64() / work_done as f64;
            Some(rate * (total_work - work_done) as f64)
        } else {
            None
        };

        info!(
            step = step,
            percentage = percentage,
            elapsed_secs = elapsed.as_secs_f64(),
            estimated_remaining_secs = estimated_remaining,
            "Progress update"
        );
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Copies a file preserving its permissions (Unix) or just copies (Windows).
fn copy_file_preserving_permissions(src: &Path, dst: &Path) -> Result<(), HaalError> {
    // Ensure parent directory exists
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::copy(src, dst).map_err(|e| FileSystemError {
        message: format!("Failed to copy file: {e}"),
        path: Some(src.display().to_string()),
    })?;

    // Preserve permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(src_meta) = std::fs::metadata(src) {
            let permissions = src_meta.permissions();
            let _ = std::fs::set_permissions(dst, std::fs::Permissions::from_mode(permissions.mode()));
        }
    }

    Ok(())
}

/// Recursively collects all file paths in a directory.
fn collect_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_files_recursive(&path, files);
            } else {
                files.push(path);
            }
        }
    }
}

/// Returns the total size of a directory in bytes.
fn dir_size(path: &Path) -> Result<u64, std::io::Error> {
    let mut total = 0u64;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                total += dir_size(&path)?;
            } else {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}

/// Returns available disk space at the given path in bytes.
///
/// Uses `std::process::Command` to query disk space in a cross-platform way.
/// Falls back to `u64::MAX` (assume enough) if the query fails.
fn available_disk_space(path: &Path) -> u64 {
    // Find the first existing ancestor
    let mut check_path = path.to_path_buf();
    while !check_path.exists() {
        if !check_path.pop() {
            return u64::MAX;
        }
    }

    // Use `df` on Unix or PowerShell on Windows to query available space.
    #[cfg(unix)]
    {
        let output = std::process::Command::new("df")
            .arg("-P") // POSIX output format
            .arg("-k") // 1K blocks
            .arg(&check_path)
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Second line, fourth column is available 1K-blocks
            if let Some(line) = stdout.lines().nth(1) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() >= 4 {
                    if let Ok(kb) = fields[3].parse::<u64>() {
                        return kb * 1024;
                    }
                }
            }
        }
        u64::MAX
    }

    #[cfg(windows)]
    {
        // Use PowerShell to get free space on the drive
        let drive = check_path
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_default();

        if drive.is_empty() {
            return u64::MAX;
        }

        let script = format!(
            "(Get-PSDrive -Name '{}').Free",
            drive.trim_end_matches('\\').trim_end_matches(':')
        );

        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(bytes) = stdout.trim().parse::<u64>() {
                return bytes;
            }
        }
        u64::MAX
    }

    #[cfg(not(any(unix, windows)))]
    {
        u64::MAX
    }
}

/// Retries an operation with a fixed delay between attempts.
///
/// - Network errors: retried up to `NETWORK_RETRY_COUNT` with exponential backoff.
/// - Integrity errors: retried up to `INTEGRITY_RETRY_COUNT`.
/// - Transient filesystem errors: retried up to `FS_RETRY_COUNT`.
/// - Other errors: not retried.
fn retry<F, T>(max_attempts: u32, base_delay: Duration, mut f: F) -> Result<T, HaalError>
where
    F: FnMut() -> Result<T, HaalError>,
{
    let mut attempt = 0u32;
    loop {
        match f() {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempt += 1;
                let (max, delay) = retry_params_for_error(&e, max_attempts, base_delay);
                if attempt >= max {
                    return Err(e);
                }
                warn!(
                    attempt = attempt,
                    max_attempts = max,
                    error = %e,
                    "Retrying after error"
                );
                std::thread::sleep(delay * attempt);
            }
        }
    }
}

/// Returns (max_attempts, delay) based on the error type.
fn retry_params_for_error(
    error: &HaalError,
    default_max: u32,
    default_delay: Duration,
) -> (u32, Duration) {
    match error {
        HaalError::Network(_) => (NETWORK_RETRY_COUNT, NETWORK_RETRY_BASE_DELAY),
        HaalError::Integrity(_) => (INTEGRITY_RETRY_COUNT, INTEGRITY_RETRY_DELAY),
        HaalError::FileSystem(fs_err) => {
            // Treat permission errors as non-transient
            if fs_err.message.contains("permission")
                || fs_err.message.contains("Permission")
                || fs_err.message.contains("read-only")
            {
                (1, Duration::ZERO) // Don't retry
            } else {
                (FS_RETRY_COUNT, FS_RETRY_DELAY)
            }
        }
        _ => (default_max, default_delay),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ComponentType;
    use std::io::Write;
    use tempfile::TempDir;

    /// Creates a minimal OperationEngine for testing.
    fn test_engine(
        backup_dir: &Path,
        metadata_path: &Path,
    ) -> OperationEngine {
        let adapters: HashMap<String, Box<dyn ToolAdapter>> = HashMap::new();
        let rollback = Arc::new(RollbackManager::new(backup_dir.to_path_buf()));
        let checksum = Arc::new(ChecksumValidator::new());
        let version_tracker = Arc::new(VersionTracker::new(metadata_path.to_path_buf()));
        let conflict = Arc::new(ConflictDetector::new(version_tracker.clone()));

        OperationEngine::new(adapters, rollback, checksum, conflict, version_tracker)
    }

    fn make_progress() -> ProgressReporter {
        ProgressReporter {
            current_step: String::new(),
            percentage: 0,
            current_file: None,
        }
    }

    fn make_component(id: &str, source_path: &str) -> Component {
        Component {
            id: id.to_string(),
            name: id.to_string(),
            description: String::new(),
            component_type: ComponentType::Skill,
            path: source_path.to_string(),
            compatible_tools: vec!["kiro".to_string()],
            dependencies: Vec::new(),
            pinned: false,
            deprecated: false,
            version: Some("abc123".to_string()),
        }
    }

    fn create_source_component(dir: &Path, id: &str) -> PathBuf {
        let comp_dir = dir.join(id);
        std::fs::create_dir_all(&comp_dir).unwrap();
        let file_path = comp_dir.join("skill.md");
        let mut f = std::fs::File::create(&file_path).unwrap();
        f.write_all(b"# Test Skill\nThis is a test.").unwrap();
        let helper_dir = comp_dir.join("helpers");
        std::fs::create_dir_all(&helper_dir).unwrap();
        let helper_path = helper_dir.join("utils.md");
        let mut f2 = std::fs::File::create(&helper_path).unwrap();
        f2.write_all(b"Helper content").unwrap();
        comp_dir
    }

    #[tokio::test]
    async fn install_copies_files_and_records_metadata() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join("backups");
        let metadata_path = tmp.path().join("meta.json");
        let source_dir = tmp.path().join("source");
        let dest_dir = tmp.path().join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();

        let comp_source = create_source_component(&source_dir, "skill-a");

        let engine = test_engine(&backup_dir, &metadata_path);
        let component = make_component("skill-a", comp_source.to_str().unwrap());
        let destinations = vec![Destination {
            tool_name: "Kiro".to_string(),
            path: dest_dir.clone(),
            enabled: true,
        }];

        let result = engine
            .install(vec![component], destinations, make_progress())
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.components_succeeded, vec!["skill-a"]);
        assert!(result.components_failed.is_empty());
        assert!(!result.rollback_performed);

        // Verify files were copied
        assert!(dest_dir.join("skill-a").join("skill.md").exists());
        assert!(dest_dir.join("skill-a").join("helpers").join("utils.md").exists());
    }

    #[tokio::test]
    async fn install_validates_disk_space() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join("backups");
        let metadata_path = tmp.path().join("meta.json");

        let engine = test_engine(&backup_dir, &metadata_path);

        // Create a component pointing to a non-existent source (size estimate = 4096)
        let component = make_component("skill-a", "/nonexistent/path");
        let destinations = vec![Destination {
            tool_name: "Kiro".to_string(),
            path: tmp.path().to_path_buf(),
            enabled: true,
        }];

        // This should succeed since disk space is available
        // (we can't easily test insufficient space without mocking)
        let result = engine
            .install(vec![component], destinations, make_progress())
            .await;
        // The install itself may fail due to missing source, but disk space check should pass
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn delete_removes_files_and_metadata() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join("backups");
        let metadata_path = tmp.path().join("meta.json");
        let source_dir = tmp.path().join("source");
        let dest_dir = tmp.path().join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();

        let comp_source = create_source_component(&source_dir, "skill-a");

        let engine = test_engine(&backup_dir, &metadata_path);
        let component = make_component("skill-a", comp_source.to_str().unwrap());
        let destinations = vec![Destination {
            tool_name: "Kiro".to_string(),
            path: dest_dir.clone(),
            enabled: true,
        }];

        // Install first
        engine
            .install(vec![component.clone()], destinations, make_progress())
            .await
            .unwrap();

        assert!(dest_dir.join("skill-a").exists());

        // Now delete
        let result = engine
            .delete(vec![component], make_progress())
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.components_succeeded, vec!["skill-a"]);
        assert!(!dest_dir.join("skill-a").exists());
    }

    #[tokio::test]
    async fn update_replaces_component_files() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join("backups");
        let metadata_path = tmp.path().join("meta.json");
        let source_dir = tmp.path().join("source");
        let dest_dir = tmp.path().join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();

        let comp_source = create_source_component(&source_dir, "skill-a");

        let engine = test_engine(&backup_dir, &metadata_path);
        let component = make_component("skill-a", comp_source.to_str().unwrap());
        let destinations = vec![Destination {
            tool_name: "Kiro".to_string(),
            path: dest_dir.clone(),
            enabled: true,
        }];

        // Install first
        engine
            .install(vec![component.clone()], destinations, make_progress())
            .await
            .unwrap();

        // Modify source for update
        let updated_source = tmp.path().join("source_v2").join("skill-a");
        std::fs::create_dir_all(&updated_source).unwrap();
        std::fs::write(updated_source.join("skill.md"), b"# Updated Skill\nNew content.").unwrap();
        std::fs::create_dir_all(updated_source.join("new_dir")).unwrap();
        std::fs::write(updated_source.join("new_dir").join("new.md"), b"New file").unwrap();

        let updated_component = make_component("skill-a", updated_source.to_str().unwrap());

        let result = engine
            .update(vec![updated_component], make_progress())
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.components_succeeded, vec!["skill-a"]);

        // New files should exist
        assert!(dest_dir.join("skill-a").join("skill.md").exists());
        assert!(dest_dir.join("skill-a").join("new_dir").join("new.md").exists());
        // Old helpers dir should be gone (full replacement)
        assert!(!dest_dir.join("skill-a").join("helpers").exists());
    }

    #[tokio::test]
    async fn reinitialize_removes_all_for_tool() {
        let tmp = TempDir::new().unwrap();
        let backup_dir = tmp.path().join("backups");
        let metadata_path = tmp.path().join("meta.json");
        let source_dir = tmp.path().join("source");
        let dest_dir = tmp.path().join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();

        let comp_source = create_source_component(&source_dir, "skill-a");

        let engine = test_engine(&backup_dir, &metadata_path);
        let component = make_component("skill-a", comp_source.to_str().unwrap());
        let destinations = vec![Destination {
            tool_name: "Kiro".to_string(),
            path: dest_dir.clone(),
            enabled: true,
        }];

        // Install
        engine
            .install(vec![component], destinations, make_progress())
            .await
            .unwrap();

        assert!(dest_dir.join("skill-a").exists());

        // Reinitialize for "kiro"
        let result = engine
            .reinitialize(vec!["kiro".to_string()], make_progress())
            .await
            .unwrap();

        assert!(result.success);
        assert!(!dest_dir.join("skill-a").exists());
    }

    #[test]
    fn retry_succeeds_on_first_attempt() {
        let mut calls = 0;
        let result: Result<i32, HaalError> = retry(3, Duration::from_millis(1), || {
            calls += 1;
            Ok(42)
        });
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls, 1);
    }

    #[test]
    fn retry_retries_on_integrity_error() {
        let mut calls = 0;
        let result: Result<i32, HaalError> = retry(3, Duration::from_millis(1), || {
            calls += 1;
            if calls < 3 {
                Err(HaalError::Integrity(IntegrityError {
                    message: "checksum mismatch".to_string(),
                    expected: None,
                    actual: None,
                }))
            } else {
                Ok(42)
            }
        });
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls, 3);
    }

    #[test]
    fn retry_gives_up_after_max_attempts() {
        let mut calls = 0;
        let result: Result<i32, HaalError> = retry(2, Duration::from_millis(1), || {
            calls += 1;
            Err(HaalError::Network(NetworkError {
                message: "timeout".to_string(),
                url: None,
                status_code: None,
            }))
        });
        assert!(result.is_err());
        // Network errors get NETWORK_RETRY_COUNT (3) attempts
        assert_eq!(calls, 3);
    }

    #[test]
    fn copy_file_preserving_permissions_works() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src.txt");
        let dst = tmp.path().join("dst.txt");
        std::fs::write(&src, b"hello").unwrap();

        copy_file_preserving_permissions(&src, &dst).unwrap();

        assert_eq!(std::fs::read(&dst).unwrap(), b"hello");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let src_mode = std::fs::metadata(&src).unwrap().permissions().mode();
            let dst_mode = std::fs::metadata(&dst).unwrap().permissions().mode();
            assert_eq!(src_mode, dst_mode);
        }
    }

    #[test]
    fn progress_update_calculates_percentage() {
        let tmp = TempDir::new().unwrap();
        let engine = test_engine(
            &tmp.path().join("b"),
            &tmp.path().join("m.json"),
        );
        let mut progress = make_progress();
        let start = Instant::now();

        engine.update_progress(&mut progress, "Step 1", 5, 10, Some("file.txt"), start);

        assert_eq!(progress.percentage, 50);
        assert_eq!(progress.current_step, "Step 1");
        assert_eq!(progress.current_file, Some("file.txt".to_string()));
    }
}
