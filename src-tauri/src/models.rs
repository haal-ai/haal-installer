use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Registry manifest types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaalManifest {
    pub version: String,
    pub repo_id: String,
    pub description: String,
    pub base_url: String,
    pub collections: Vec<CollectionEntry>,
    pub competencies: Vec<CompetencyEntry>,
    #[serde(default)]
    pub systems: Vec<SystemEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub competency_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompetencyEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub manifest_url: String,
}

/// Full competency detail — all component IDs grouped by type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompetencyDetail {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub powers: Vec<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    #[serde(default)]
    pub systems: Vec<String>,
}

// ---------------------------------------------------------------------------
// MCP server definition (stored in registry as mcpservers/<id>/mcp.json)
// ---------------------------------------------------------------------------

/// Transport type for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum McpTransport {
    /// Remote/cloud — just a URL, nothing to install locally.
    Http,
    /// Local stdio process — needs a runtime (npx, uvx, binary).
    Stdio,
}

/// Full MCP server definition loaded from the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub transport: McpTransport,
    /// For Http transport: the remote URL.
    #[serde(default)]
    pub server_url: Option<String>,
    /// For Stdio transport: the command to run (e.g. "uvx", "npx").
    #[serde(default)]
    pub command: Option<String>,
    /// Args passed to the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables to inject.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// Which scopes are meaningful: "user" | "workspace"
    #[serde(default)]
    pub scope: Vec<String>,
}

/// Legacy manifest kept for adapter compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub components: Vec<Component>,
    pub collections: Vec<CollectionEntry>,
    pub competencies: Vec<CompetencyEntry>,
}

// ---------------------------------------------------------------------------
// Multi-registry catalog
// ---------------------------------------------------------------------------

/// A single cloned registry repo with its resolved local path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSource {
    /// e.g. "haal-ai/haal-skills:main"
    pub repo_spec: String,
    /// Local path to the cloned repo.
    pub local_path: PathBuf,
    /// Priority — higher wins on conflict (seed = highest).
    pub priority: u32,
}

/// The merged catalog built from all cloned repos.
/// Higher-priority repos win on duplicate competency/collection IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedCatalog {
    pub collections: Vec<CollectionEntry>,
    pub competencies: Vec<CompetencyEntry>,
    /// Maps competency_id → source repo local path (for install-time resolution).
    pub competency_sources: std::collections::HashMap<String, PathBuf>,
    /// All systems from all registries (deduplicated by id, first-seen wins).
    #[serde(default)]
    pub systems: Vec<SystemEntry>,
}

// ---------------------------------------------------------------------------
// Component types
// ---------------------------------------------------------------------------

/// Tool-agnostic component type. The installer adapter maps each type
/// to the correct destination path(s) per tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ComponentType {
    Skill,
    Power,
    Rule,
    Hook,
    Command,
    Agent,
    OlafData,
    Package,
    McpServer,
    System,
}

/// A resolved, installable component with its source path in the local cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedComponent {
    pub id: String,
    pub component_type: ComponentType,
    /// Absolute path to the component folder/file in the cloned repo cache.
    pub source_path: PathBuf,
}

// ---------------------------------------------------------------------------
// Agentic Systems
// ---------------------------------------------------------------------------

/// A system entry in the manifest — points to a standalone GitHub repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    /// GitHub repo URL, e.g. "https://github.com/haal-ai/haal-gitpulse"
    pub repo: String,
    /// Optional branch (defaults to "main")
    #[serde(default)]
    pub branch: Option<String>,
    /// Tags for display/filtering
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Full system definition loaded from `system.json` at the repo root.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub prerequisites: SystemPrerequisites,
    /// Custom install commands (e.g. pip install -e ".[all]")
    #[serde(default)]
    pub install: Option<SystemInstall>,
    #[serde(default)]
    pub post_install: Option<PostInstall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInstall {
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SystemPrerequisites {
    /// Runtime requirements, e.g. ["python>=3.10", "uvx", "aws"]
    #[serde(default)]
    pub runtimes: Vec<String>,
    /// pip install needed (runs `pip install -e ".[all]"` or custom install commands)
    #[serde(default)]
    pub pip: bool,
    /// npm install needed
    #[serde(default)]
    pub npm: bool,
    /// Required environment variables (must be set before use)
    #[serde(default)]
    pub env: Vec<String>,
    /// Optional environment variables with descriptions
    #[serde(default)]
    pub env_optional: Vec<EnvVar>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostInstall {
    /// Shell commands to run after clone (shown to user, not auto-executed)
    #[serde(default)]
    pub commands: Vec<String>,
    /// Human-readable message shown on the done screen
    #[serde(default)]
    pub message: Option<String>,
}

/// Status of an installed system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SystemStatus {
    /// Not installed
    NotInstalled,
    /// Installed, up to date
    Installed,
    /// Installed, updates available (remote has newer commits)
    UpdateAvailable,
}

/// Full info about an installed system returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSystemInfo {
    pub id: String,
    pub name: String,
    pub install_path: String,
    pub status: SystemStatus,
    pub current_commit: Option<String>,
}

/// Legacy component kept for adapter compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub id: String,
    pub name: String,
    pub description: String,
    pub component_type: ComponentType,
    pub path: String,
    pub compatible_tools: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub version: Option<String>,
}

// ---------------------------------------------------------------------------
// Install request / result
// ---------------------------------------------------------------------------

/// Scope of installation chosen by the user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum InstallScope {
    /// Install to home/global tool directories only.
    Home,
    /// Install to the specified repo directory only.
    Repo,
    /// Install to both home and repo.
    Both,
}

/// Full install request passed from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    /// Resolved components to install.
    pub components: Vec<ResolvedComponent>,
    pub scope: InstallScope,
    /// Absolute path to the target repo (required for Repo/Both scope).
    pub repo_path: Option<PathBuf>,
    /// Which tools to install to (e.g. ["Kiro", "Cursor"]).
    pub selected_tools: Vec<String>,
    /// If true, overwrite existing; if false, skip existing.
    pub reinstall_all: bool,
}

/// Result of the install operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub success: bool,
    pub components_succeeded: Vec<String>,
    pub components_failed: Vec<ComponentFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentFailure {
    pub component_id: String,
    pub error: String,
}

// ---------------------------------------------------------------------------
// Legacy operation types (kept for existing commands)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Destination {
    pub tool_name: String,
    pub path: PathBuf,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub success: bool,
    pub components_succeeded: Vec<String>,
    pub components_failed: Vec<ComponentFailure>,
    pub rollback_performed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigurationProfile {
    pub repositories: Vec<String>,
    pub selected_components: Vec<String>,
    pub destinations: Vec<Destination>,
    pub preferences: UserPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub theme: Theme,
    pub language: Language,
    pub auto_update: bool,
    pub parallel_operations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Theme { Dark, Light }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Language { English, French }
