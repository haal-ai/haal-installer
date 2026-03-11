use crate::errors::HaalError;
use crate::traits::ToolAdapter;
use serde::{Deserialize, Serialize};

/// Information about a detected AI coding tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedTool {
    pub name: String,
    pub version: Option<String>,
    pub path: String,
    pub is_installed: bool,
}

/// Detects installed AI coding tools on the system.
pub struct ToolDetector {
    adapters: Vec<Box<dyn ToolAdapter>>,
}

impl ToolDetector {
    /// Creates a new ToolDetector with the given adapters.
    pub fn new(adapters: Vec<Box<dyn ToolAdapter>>) -> Self {
        Self { adapters }
    }

    /// Detects all tools by querying each adapter.
    pub fn detect_tools(&self) -> Result<Vec<DetectedTool>, HaalError> {
        let mut detected = Vec::new();

        for adapter in &self.adapters {
            if let Ok(Some(path)) = adapter.detect_installation() {
                let version = adapter.detect_version().ok().flatten();

                detected.push(DetectedTool {
                    name: adapter.tool_name().to_string(),
                    version,
                    path: path.display().to_string(),
                    is_installed: true,
                });
            }
        }

        Ok(detected)
    }
}
