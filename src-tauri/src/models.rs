use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Represents an installable item (skill, extension, plugin, configuration).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub id: String,
    pub name: String,
    pub description: String,
    pub component_type: ComponentType,
    pub path: String,
    pub compatible_tools: Vec<String>,
    pub dependencies: Vec<String>,
    pub pinned: bool,
    pub deprecated: bool,
    /// Git commit hash tracking the component version in the repository.
    #[serde(default)]
    pub version: Option<String>,
}

/// The type of a component.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComponentType {
    Skill,
    Extension,
    Plugin,
    Configuration,
}

/// A parsed manifest from a repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub components: Vec<Component>,
    pub collections: Vec<Collection>,
    pub competencies: Vec<Competency>,
}

/// A named group of related components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub description: String,
    pub component_ids: Vec<String>,
}

/// A specialized collection with specific capabilities and config files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Competency {
    pub id: String,
    pub name: String,
    pub description: String,
    pub component_ids: Vec<String>,
    pub config_files: Vec<String>,
}

/// A file-system destination where components are installed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Destination {
    pub tool_name: String,
    pub path: PathBuf,
    pub enabled: bool,
}

/// The result of an install/update/delete/reinitialize operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub success: bool,
    pub components_succeeded: Vec<String>,
    pub components_failed: Vec<ComponentFailure>,
    pub rollback_performed: bool,
}

/// Details about a single component that failed during an operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentFailure {
    pub component_id: String,
    pub error: String,
}

/// An exportable/importable set of installation preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigurationProfile {
    pub repositories: Vec<String>,
    pub selected_components: Vec<String>,
    pub destinations: Vec<Destination>,
    pub preferences: UserPreferences,
}

/// User-level preferences persisted across sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub theme: Theme,
    pub language: Language,
    pub auto_update: bool,
    pub parallel_operations: bool,
}

/// UI theme.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

/// Supported UI languages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Language {
    English,
    French,
}
