use std::path::{Path, PathBuf};

use crate::errors::HaalError;
use crate::models::{Component, Destination, Manifest, OperationResult};

/// Issue found during operation validation.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub message: String,
    pub severity: ValidationSeverity,
}

/// Severity level for a validation issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationSeverity {
    Warning,
    Error,
}

/// Callback for reporting operation progress to the frontend.
pub struct ProgressReporter {
    /// Current step description.
    pub current_step: String,
    /// Progress percentage (0–100).
    pub percentage: u8,
    /// File currently being processed.
    pub current_file: Option<String>,
}

/// Adapter interface for a specific AI coding tool.
///
/// Each supported tool (Copilot, Cursor, Claude Code, Kiro, Windsurf)
/// provides a concrete implementation.
pub trait ToolAdapter: Send + Sync {
    /// Returns the human-readable tool name.
    fn tool_name(&self) -> &str;

    /// Detects whether the tool is installed; returns its path if found.
    fn detect_installation(&self) -> Result<Option<PathBuf>, HaalError>;

    /// Returns the default destination paths for this tool.
    fn default_destinations(&self) -> Vec<Destination>;

    /// Parses a tool-specific manifest from raw content.
    fn parse_manifest(&self, content: &str) -> Result<Manifest, HaalError>;

    /// Checks whether a component is compatible with this tool.
    fn validate_compatibility(&self, component: &Component) -> Result<bool, HaalError>;

    /// Installs a component to the given destination path.
    fn install_component(&self, component: &Component, dest: &Path) -> Result<(), HaalError>;

    /// Updates a component at the given destination path.
    fn update_component(&self, component: &Component, dest: &Path) -> Result<(), HaalError>;

    /// Deletes a component from the given destination path.
    fn delete_component(&self, component: &Component, dest: &Path) -> Result<(), HaalError>;

    /// Runs any tool-specific post-installation steps.
    fn post_install(&self, components: &[Component]) -> Result<(), HaalError>;

    /// Returns the tool's version string if detectable.
    fn detect_version(&self) -> Result<Option<String>, HaalError>;
}

/// An executable operation (install, update, delete, reinitialize).
pub trait Operation: Send + Sync {
    /// Returns the operation name for logging.
    fn name(&self) -> &str;

    /// Calculates the disk space required by this operation in bytes.
    fn calculate_disk_space(&self) -> Result<u64, HaalError>;

    /// Executes the operation, reporting progress through the callback.
    ///
    /// Implementations should use `tokio::spawn` or similar for async work.
    fn execute(
        &self,
        progress: ProgressReporter,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<OperationResult, HaalError>> + Send + '_>,
    >;

    /// Validates that the operation can proceed; returns any issues found.
    fn validate(&self) -> Result<Vec<ValidationIssue>, HaalError>;
}
