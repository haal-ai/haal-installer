use std::path::{Path, PathBuf};

use tracing::debug;

use crate::errors::{FileSystemError, HaalError, ValidationError};
use crate::models::{Component, Destination, Manifest};
use crate::traits::ToolAdapter;

/// Adapter for GitHub Copilot installations.
pub struct CopilotAdapter;

impl CopilotAdapter {
    pub fn new() -> Self {
        Self
    }

    /// The short key used for compatibility matching.
    const TOOL_KEY: &'static str = "copilot";
}

impl ToolAdapter for CopilotAdapter {
    fn tool_name(&self) -> &str {
        "GitHub Copilot"
    }

    fn detect_installation(&self) -> Result<Option<PathBuf>, HaalError> {
        let home = dirs::home_dir().ok_or_else(|| HaalError::FileSystem(FileSystemError {
            message: "Could not determine home directory".to_string(),
            path: None,
        }))?;

        // Platform-specific paths
        let candidates: Vec<PathBuf> = if cfg!(target_os = "windows") {
            let mut paths = Vec::new();
            if let Some(config) = dirs::config_dir() {
                paths.push(config.join("GitHub Copilot"));
            }
            paths.push(home.join(".github").join("copilot"));
            paths
        } else {
            vec![
                home.join(".github").join("copilot"),
                home.join(".config").join("github-copilot"),
            ]
        };

        for path in &candidates {
            debug!("Checking Copilot installation at: {}", path.display());
            if path.exists() {
                debug!("Found Copilot installation at: {}", path.display());
                return Ok(Some(path.clone()));
            }
        }

        debug!("No Copilot installation found");
        Ok(None)
    }

    fn default_destinations(&self) -> Vec<Destination> {
        let mut dests = Vec::new();
        if let Some(home) = dirs::home_dir() {
            dests.push(Destination {
                tool_name: self.tool_name().to_string(),
                path: home.join(".github").join("copilot").join("plugins"),
                enabled: true,
            });
        }
        dests
    }

    fn parse_manifest(&self, content: &str) -> Result<Manifest, HaalError> {
        serde_json::from_str(content).map_err(|e| {
            HaalError::Validation(ValidationError {
                message: format!("Failed to parse Copilot manifest: {e}"),
                field: None,
            })
        })
    }

    fn validate_compatibility(&self, component: &Component) -> Result<bool, HaalError> {
        Ok(component
            .compatible_tools
            .iter()
            .any(|t| t.eq_ignore_ascii_case(Self::TOOL_KEY)))
    }

    fn install_component(&self, _component: &Component, dest: &Path) -> Result<(), HaalError> {
        std::fs::create_dir_all(dest)?;
        Ok(())
    }

    fn update_component(&self, _component: &Component, dest: &Path) -> Result<(), HaalError> {
        std::fs::create_dir_all(dest)?;
        Ok(())
    }

    fn delete_component(&self, component: &Component, dest: &Path) -> Result<(), HaalError> {
        let target = dest.join(&component.id);
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        Ok(())
    }

    fn post_install(&self, _components: &[Component]) -> Result<(), HaalError> {
        Ok(())
    }

    fn detect_version(&self) -> Result<Option<String>, HaalError> {
        Ok(None)
    }
}
