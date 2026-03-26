pub mod adapters;
pub mod checksum_validator;
pub mod config_manager;
pub mod conflict_detector;
pub mod content_hasher;
pub mod destination_resolver;
pub mod requirement_checker;pub mod errors;

// ---------------------------------------------------------------------------
// Windows: hide console windows spawned by background Command calls
// ---------------------------------------------------------------------------

/// Returns the `CREATE_NO_WINDOW` creation flags on Windows (0x0800_0000)
/// so that `Command::new(...)` calls don't flash a visible console.
/// On non-Windows platforms this is a no-op returning 0.
#[cfg(windows)]
pub(crate) fn no_window_flags() -> u32 {
    0x0800_0000 // CREATE_NO_WINDOW
}

#[cfg(not(windows))]
pub(crate) fn no_window_flags() -> u32 {
    0
}
pub mod github_auth;
pub mod installer;
pub mod logging;
pub mod manifest_parser;
pub mod models;
pub mod offline_detector;
pub mod operation_engine;
pub mod registry_manager;
pub mod repo_manager;
pub mod rollback_manager;
pub mod self_installer;
pub mod system_installer;
pub mod tool_detector;
pub mod traits;
pub mod version_tracker;

use std::collections::HashMap;
use std::sync::Arc;

use adapters::claude_code::ClaudeCodeAdapter;
use adapters::copilot::CopilotAdapter;
use adapters::cursor::CursorAdapter;
use adapters::kiro::KiroAdapter;
use adapters::windsurf::WindsurfAdapter;
use checksum_validator::ChecksumValidator;
use conflict_detector::{ConflictDetector, ConflictType};
use github_auth::{GitHubAuthenticator, GitHubCredentials};
use config_manager::ConfigurationManager;
use installer::Installer;
use models::{CompetencyDetail, CompetencyEntry, HaalManifest, Component, ConfigurationProfile, Destination, InstallRequest, InstallResult, MergedCatalog, OperationResult, UserPreferences};
use repo_manager::RepoManager;use offline_detector::OfflineDetector;
use operation_engine::OperationEngine;
use registry_manager::{RegistryManager, DEFAULT_REGISTRY_URL};
use rollback_manager::RollbackManager;
use self_installer::SelfInstaller;
use serde::Serialize;
use tool_detector::{DetectedTool, ToolDetector};
use traits::ProgressReporter;
use version_tracker::{UpdateInfo, VersionTracker};

/// JSON-serializable status returned by the `check_self_install` command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfInstallStatus {
    pub is_installed: bool,
    pub home_exists: bool,
    pub needs_update: bool,
}

/// Checks the current self-installation state without modifying anything.
#[tauri::command]
fn check_self_install() -> Result<SelfInstallStatus, String> {
    // In debug builds, skip self-install — the binary is locked by the dev process
    #[cfg(debug_assertions)]
    return Ok(SelfInstallStatus {
        is_installed: true,
        home_exists: true,
        needs_update: false,
    });

    #[cfg(not(debug_assertions))]
    {
        let installer = SelfInstaller::new(SelfInstaller::haal_home());
        let needs_update = installer.needs_update().map_err(|e| e.to_string())?;
        Ok(SelfInstallStatus {
            is_installed: installer.is_installed(),
            home_exists: installer.home_exists(),
            needs_update,
        })
    }
}

/// Performs the full self-installation: creates the HAAL home directory
/// structure and installs or updates the binary in `~/.haal/bin/`.
#[tauri::command]
fn self_install() -> Result<(), String> {
    let installer = SelfInstaller::new(SelfInstaller::haal_home());

    if !installer.home_exists() {
        installer.create_home_structure().map_err(|e| e.to_string())?;
    }

    if !installer.is_installed() {
        installer.install_binary().map_err(|e| e.to_string())?;
    } else if installer.needs_update().map_err(|e| e.to_string())? {
        installer.update_binary().map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Creates a desktop shortcut pointing to ~/.haal/bin/.
#[tauri::command]
fn create_desktop_shortcut() -> Result<(), String> {
    let installer = SelfInstaller::new(SelfInstaller::haal_home());
    installer
        .create_desktop_shortcut()
        .map_err(|e| e.to_string())
}

/// Adds ~/.haal/bin/ to the system PATH.
#[tauri::command]
fn add_to_path() -> Result<(), String> {
    let installer = SelfInstaller::new(SelfInstaller::haal_home());
    installer.add_to_path().map_err(|e| e.to_string())
}

/// Relaunches the application from ~/.haal/bin/ and exits the current process.
#[tauri::command]
fn relaunch_from_home() -> Result<(), String> {
    let installer = SelfInstaller::new(SelfInstaller::haal_home());
    installer
        .relaunch_from_home()
        .map_err(|e| e.to_string())
}

/// Checks whether the GitHub CLI is installed and authenticated.
/// Returns: { installed: bool, authenticated: bool }
#[tauri::command]
fn check_gh_cli() -> serde_json::Value {
    // Check if gh is installed
    let mut cmd = std::process::Command::new("gh");
    cmd.arg("--version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(no_window_flags());
    }
    let installed = cmd.output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !installed {
        return serde_json::json!({ "installed": false, "authenticated": false });
    }

    // Unset GITHUB_TOKEN/GH_TOKEN so we check stored credentials, not the env var
    let mut cmd = std::process::Command::new("gh");
    cmd.args(["auth", "status"])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(no_window_flags());
    }
    let authenticated = cmd.output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !authenticated {
        return serde_json::json!({ "installed": true, "authenticated": false, "username": null });
    }

    // Fetch the username of the stored account
    let mut cmd = std::process::Command::new("gh");
    cmd.args(["api", "user", "--jq", ".login"])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(no_window_flags());
    }
    let username = cmd.output()
        .ok()
        .and_then(|o| if o.status.success() { Some(o.stdout) } else { None })
        .map(|b| String::from_utf8_lossy(&b).trim().to_string())
        .filter(|s| !s.is_empty());

    serde_json::json!({ "installed": true, "authenticated": true, "username": username })
}

/// Launches `gh auth login` in a new terminal window so the user can authenticate.
/// Uses `--web` to skip interactive prompts and go straight to browser-based device flow.
/// Unsets GITHUB_TOKEN first so gh stores credentials rather than using the env var.
#[tauri::command]
fn launch_gh_auth_login() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(["/c", "start", "cmd", "/k", "set GITHUB_TOKEN=&& set GH_TOKEN=&& gh auth login --web -h github.com -p https"])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    std::process::Command::new("osascript")
        .args(["-e", "tell app \"Terminal\" to do script \"unset GITHUB_TOKEN; unset GH_TOKEN; gh auth login --web -h github.com -p https\""])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("sh")
        .args(["-c", "x-terminal-emulator -e 'unset GITHUB_TOKEN; unset GH_TOKEN; gh auth login --web -h github.com -p https' || xterm -e 'unset GITHUB_TOKEN; unset GH_TOKEN; gh auth login --web -h github.com -p https'"])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Reads the token stored by `gh auth login` and uses it to authenticate.
/// Runs `gh auth token` and validates the result against the GitHub API.
#[tauri::command]
async fn authenticate_gh_cli(enterprise_url: Option<String>) -> Result<GitHubCredentials, String> {
    // Ignore enterprise_url if it's a github.com URL (not a GHE instance)
    let enterprise_url = enterprise_url.filter(|u| {
        let u = u.trim();
        !u.is_empty() && !u.contains("github.com/")
    });
    // Run `gh auth token` to get the stored token (ignore env var)
    let mut cmd = std::process::Command::new("gh");
    cmd.args(["auth", "token"])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(no_window_flags());
    }
    let output = cmd.output()
        .map_err(|_| "GitHub CLI (gh) not found. Please install it from https://cli.github.com and run `gh auth login` first.".to_string())?;

    if !output.status.success() {
        return Err("gh auth token failed — have you run `gh auth login`?".to_string());
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err("No token found. Please run `gh auth login` first.".to_string());
    }

    let config_dir = SelfInstaller::haal_home().join("config");
    let authenticator = GitHubAuthenticator::new(config_dir);
    let credentials = authenticator
        .authenticate_pat(token, enterprise_url)
        .await
        .map_err(|e| e.to_string())?;
    authenticator
        .store_credentials(&credentials)
        .map_err(|e| e.to_string())?;
    Ok(credentials)
}

/// Authenticates with GitHub using a Personal Access Token.
#[tauri::command]
async fn authenticate_github(
    auth_type: String,
    token: Option<String>,
    enterprise_url: Option<String>,
) -> Result<GitHubCredentials, String> {
    let config_dir = SelfInstaller::haal_home().join("config");
    let authenticator = GitHubAuthenticator::new(config_dir);

    match auth_type.as_str() {
        "pat" => {
            let pat = token.ok_or_else(|| "Token is required for PAT authentication".to_string())?;
            let credentials = authenticator
                .authenticate_pat(pat, enterprise_url)
                .await
                .map_err(|e| e.to_string())?;
            authenticator
                .store_credentials(&credentials)
                .map_err(|e| e.to_string())?;
            Ok(credentials)
        }
        "oauth" => {
            // OAuth device flow: the caller receives a DeviceCodeResponse and
            // must poll separately.  We return placeholder credentials here;
            // real OAuth completion would go through a dedicated polling command.
            Err("OAuth device flow is not yet supported via this command. Use the dedicated OAuth flow.".to_string())
        }
        other => Err(format!("Unknown auth_type: '{}'. Expected 'pat' or 'oauth'.", other)),
    }
}

/// Verifies that the currently stored credentials have access to the given
/// repository URL.
#[tauri::command]
async fn verify_repo_access(repo_url: String) -> Result<bool, String> {
    let config_dir = SelfInstaller::haal_home().join("config");
    let authenticator = GitHubAuthenticator::new(config_dir);
    authenticator
        .verify_access(&repo_url)
        .await
        .map_err(|e| e.to_string())
}

/// Returns the stored GitHub credentials, if any.
#[tauri::command]
fn get_stored_credentials() -> Result<Option<GitHubCredentials>, String> {
    let config_dir = SelfInstaller::haal_home().join("config");
    let authenticator = GitHubAuthenticator::new(config_dir);
    authenticator
        .retrieve_credentials()
        .map_err(|e| e.to_string())
}

/// Checks current network connectivity by pinging the GitHub API.
#[tauri::command]
async fn check_network_status() -> Result<bool, String> {
    let detector = OfflineDetector::new();
    detector
        .check_connectivity()
        .await
        .map_err(|e| e.to_string())
}

/// Fetches the top-level HAAL manifest (collections + competency stubs).
/// Falls back to cached version if the registry is unreachable.
#[tauri::command]
async fn fetch_registry(registry_url: Option<String>) -> Result<HaalManifest, String> {
    let cache_dir = SelfInstaller::haal_home().join("cache").join("manifests");
    let url = registry_url
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_REGISTRY_URL.to_string());
    let manager = RegistryManager::new(url, cache_dir);
    manager.fetch_manifest().await.map_err(|e| e.to_string())
}

/// Fetches the full detail for a single competency (skills, powers, hooks, commands).
/// Tries the local cloned repo cache first (fast, no network).
/// Falls back to remote fetch only if the local file is missing.
#[tauri::command]
async fn fetch_competency(
    competency_id: String,
    manifest_url: String,
    base_url: String,
) -> Result<CompetencyDetail, String> {
    // Try local repo cache first — base_url may be a local path from MergedCatalog
    // competencySources (absolute path to the cloned repo root).
    let local_path = {
        // competencySources gives us the repo root; manifest_url is e.g. "competencies/developer.json"
        let candidate = std::path::PathBuf::from(&base_url).join(&manifest_url);
        if candidate.exists() { Some(candidate) } else { None }
    };

    if let Some(path) = local_path {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        return parse_competency_json(&content)
            .map_err(|e| format!("Cannot parse {}: {e}", path.display()));
    }

    // Fall back to remote fetch (base_url is an HTTP base URL)
    let cache_dir = SelfInstaller::haal_home().join("cache").join("manifests");
    let manager = RegistryManager::new(DEFAULT_REGISTRY_URL.to_string(), cache_dir);
    let entry = CompetencyEntry {
        id: competency_id,
        name: String::new(),
        description: String::new(),
        manifest_url,
    };
    manager
        .fetch_competency(&entry, &base_url)
        .await
        .map_err(|e| e.to_string())
}

/// Parses a competency JSON string, handling both v1 (flat) and v2 (shared/tools) schemas.
pub(crate) fn parse_competency_json(content: &str) -> Result<CompetencyDetail, String> {
    // Try v2 first: check for schemaVersion field
    if let Ok(v2) = serde_json::from_str::<models::CompetencyV2>(content) {
        if v2.schema_version == Some(2) {
            return Ok(flatten_v2_to_detail(&v2));
        }
    }
    // Fall back to v1 (flat CompetencyDetail)
    serde_json::from_str::<CompetencyDetail>(content).map_err(|e| e.to_string())
}

/// Flattens a v2 competency (shared + tools) into a flat CompetencyDetail
/// that the frontend expects.
pub(crate) fn flatten_v2_to_detail(v2: &models::CompetencyV2) -> CompetencyDetail {
    let shared = v2.shared.as_ref();
    let mut skills: Vec<String> = shared.map(|s| s.skills.clone()).unwrap_or_default();
    let mut mcp_servers: Vec<String> = shared.map(|s| s.mcpservers.clone()).unwrap_or_default();
    let mut agents: Vec<String> = shared.map(|s| s.agents.clone()).unwrap_or_default();
    let mut systems: Vec<String> = shared.map(|s| s.systems.clone()).unwrap_or_default();
    let mut powers: Vec<String> = Vec::new();
    let mut rules: Vec<String> = Vec::new();
    let mut commands: Vec<String> = Vec::new();
    let mut hooks: Vec<String> = Vec::new();

    // Merge all tool bundles — deduplicate across tools
    if let Some(tools) = &v2.tools {
        for bundle in tools.values() {
            for p in &bundle.powers   { if !powers.contains(p)   { powers.push(p.clone()); } }
            for r in &bundle.rules    { if !rules.contains(r)    { rules.push(r.clone()); } }
            for c in &bundle.commands { if !commands.contains(c) { commands.push(c.clone()); } }
            for h in &bundle.hooks    { if !hooks.contains(h)    { hooks.push(h.clone()); } }
        }
    }

    // Deduplicate shared lists too (just in case)
    skills.dedup();
    mcp_servers.dedup();
    agents.dedup();
    systems.dedup();

    CompetencyDetail {
        name: v2.name.clone(),
        description: v2.description.clone(),
        skills,
        powers,
        hooks,
        commands,
        rules,
        agents,
        mcp_servers,
        systems,
        packages: Vec::new(),
    }
}

/// Reads an MCP server definition from a locally cloned registry repo.
/// `source_path` is the absolute path to `mcpservers/<id>/` in the cache.
#[tauri::command]
fn read_mcp_server_def(source_path: String) -> Result<models::McpServerDef, String> {
    let path = std::path::PathBuf::from(&source_path).join("mcp.json");
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&content).map_err(|e| format!("Cannot parse mcp.json: {e}"))
}

/// Detects all supported AI coding tools installed on the system.
#[tauri::command]
fn detect_tools() -> Result<Vec<DetectedTool>, String> {
    let adapters: Vec<Box<dyn crate::traits::ToolAdapter>> = vec![
        Box::new(CopilotAdapter::new()),
        Box::new(CursorAdapter::new()),
        Box::new(ClaudeCodeAdapter::new()),
        Box::new(KiroAdapter::new()),
        Box::new(WindsurfAdapter::new()),
    ];
    let detector = ToolDetector::new(adapters);
    detector.detect_tools().map_err(|e| e.to_string())
}

/// Checks for available updates by comparing installed component versions
/// with the latest versions from all enabled repositories.
#[tauri::command]
async fn check_updates() -> Result<Vec<UpdateInfo>, String> {
    let metadata_path = SelfInstaller::haal_home()
        .join("data")
        .join("installed_metadata.json");
    let tracker = VersionTracker::new(metadata_path);
    tracker.load().map_err(|e| e.to_string())?;

    // No flat component list anymore — return empty until version tracking
    // is wired to the new competency-based model.
    Ok(tracker.check_updates(&[]))
}

/// Returns the resolved install paths for skills and powers on this machine.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallPath {
    pub tool: String,
    pub skills_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPaths {
    pub tool_paths: Vec<ToolInstallPath>,
    pub powers_path: String,
}

#[tauri::command]
fn get_current_dir() -> String {
    std::env::current_dir()
        .unwrap_or_default()
        .display()
        .to_string()
}

/// Scans the given root directory (and up to `max_depth` levels of subdirectories)
/// for folders that contain a `.git` directory. Returns the list of absolute paths.
/// Skips hidden directories and common non-project folders (node_modules, target, etc.).
#[tauri::command]
fn scan_nearby_git_repos(root: String, max_depth: u32) -> Vec<String> {
    let root_path = std::path::Path::new(&root);
    if !root_path.is_dir() {
        return Vec::new();
    }
    let mut repos = Vec::new();
    scan_git_repos_recursive(root_path, 0, max_depth, &mut repos);
    repos
}

fn scan_git_repos_recursive(dir: &std::path::Path, depth: u32, max_depth: u32, repos: &mut Vec<String>) {
    // Check if this directory itself is a git repo
    if dir.join(".git").exists() {
        if let Some(s) = dir.to_str() {
            repos.push(s.to_string());
        }
        // Don't recurse into a git repo looking for nested repos
        return;
    }
    if depth >= max_depth {
        return;
    }
    // Read children, skip hidden dirs and common non-project folders
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        // Skip known non-project directories
        if matches!(
            name_str.as_ref(),
            "node_modules" | "target" | "dist" | "build" | ".haal" | "__pycache__" | "venv" | ".venv"
        ) {
            continue;
        }
        scan_git_repos_recursive(&path, depth + 1, max_depth, repos);
    }
}

/// Returns Ok(path) if the folder contains a .git directory, Err otherwise.
#[tauri::command]
fn validate_git_repo(path: String) -> Result<String, String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("Folder does not exist: {}", path));
    }
    if p.join(".git").exists() {
        Ok(path)
    } else {
        Err("No .git directory found. Please select a git repository.".to_string())
    }
}

// ---------------------------------------------------------------------------
// Agentic Systems commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn fetch_system_def(
    id: String,
    repo: String,
    branch: Option<String>,
) -> Result<crate::models::SystemDef, String> {
    let entry = crate::models::SystemEntry {
        id,
        name: String::new(),
        description: String::new(),
        repo,
        branch,
        tags: vec![],
    };
    crate::system_installer::fetch_system_def(&entry).await
}

#[tauri::command]
fn install_system(id: String, repo: String, branch: Option<String>) -> Result<String, String> {
    let entry = crate::models::SystemEntry {
        id,
        name: String::new(),
        description: String::new(),
        repo,
        branch,
        tags: vec![],
    };
    let path = crate::system_installer::install_system(&entry)?;
    Ok(path.display().to_string())
}

#[tauri::command]
fn update_system(id: String) -> Result<(), String> {
    crate::system_installer::update_system(&id)
}

#[tauri::command]
fn delete_system(id: String) -> Result<(), String> {
    crate::system_installer::delete_system(&id)
}

#[tauri::command]
fn scan_installed_systems(
    systems: Vec<crate::models::SystemEntry>,
) -> Vec<crate::models::InstalledSystemInfo> {
    crate::system_installer::scan_installed_systems(&systems)
}

#[tauri::command]
fn get_systems_root() -> String {
    crate::system_installer::systems_root().display().to_string()
}

#[tauri::command]
fn get_post_install_steps(
    def: crate::models::SystemDef,
    install_path: String,
) -> Vec<String> {
    crate::system_installer::post_install_commands(
        &def,
        std::path::Path::new(&install_path),
    )
}

#[tauri::command]
fn get_install_paths() -> InstallPaths {
    let home = dirs::home_dir().unwrap_or_default();
    let powers_path = home.join(".kiro").join("powers").join("installed").display().to_string();

    let adapters: Vec<(&str, Box<dyn crate::traits::ToolAdapter>)> = vec![
        ("Kiro",        Box::new(KiroAdapter::new())),
        ("Cursor",      Box::new(CursorAdapter::new())),
        ("Claude Code", Box::new(ClaudeCodeAdapter::new())),
        ("Windsurf",    Box::new(WindsurfAdapter::new())),
        ("Copilot",     Box::new(CopilotAdapter::new())),
    ];

    let tool_paths = adapters.into_iter()
        .filter(|(_, a)| a.detect_installation().ok().flatten().is_some())
        .filter_map(|(name, a)| {
            // Use the first default destination path for display
            a.default_destinations().into_iter().next().map(|d| ToolInstallPath {
                tool: name.to_string(),
                skills_path: d.path.display().to_string(),
            })
        })
        .collect();

    InstallPaths { tool_paths, powers_path }
}

/// Scans installed skills for each detected tool
/// and returns which skill IDs are already present on disk.
/// Returns a map of { tool_name -> [skill_id, ...] }
#[tauri::command]
fn scan_installed() -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    scan_skills_at_base(None)
}

/// Scans installed skills at a specific base path (e.g. a repo root).
/// Looks for `<base>/.kiro/skills/` instead of the tool's default destinations.
#[tauri::command]
fn scan_installed_at(base_path: String) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    scan_skills_at_base(Some(std::path::PathBuf::from(base_path)))
}

fn scan_skills_at_base(base: Option<std::path::PathBuf>) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    use std::collections::HashMap;

    let mut result: HashMap<String, Vec<String>> = HashMap::new();

    if let Some(base_path) = base {
        // Repo-scoped scan: look in <base>/.kiro/skills/
        let skills_dir = base_path.join(".kiro").join("skills");
        let skill_ids = scan_skills_dir(&skills_dir);
        if !skill_ids.is_empty() {
            result.insert("repo".to_string(), skill_ids);
        }
        return Ok(result);
    }

    // Home scan: use each tool adapter's default destinations
    let adapters: Vec<(&str, Box<dyn crate::traits::ToolAdapter>)> = vec![
        ("kiro",        Box::new(KiroAdapter::new())),
        ("cursor",      Box::new(CursorAdapter::new())),
        ("claude-code", Box::new(ClaudeCodeAdapter::new())),
        ("windsurf",    Box::new(WindsurfAdapter::new())),
        ("copilot",     Box::new(CopilotAdapter::new())),
    ];

    for (key, adapter) in &adapters {
        if adapter.detect_installation().ok().flatten().is_none() {
            continue;
        }
        let mut skill_ids: Vec<String> = Vec::new();
        for dest in adapter.default_destinations() {
            skill_ids.extend(scan_skills_dir(&dest.path));
        }
        skill_ids.sort();
        skill_ids.dedup();
        if !skill_ids.is_empty() {
            result.insert(key.to_string(), skill_ids);
        }
    }

    Ok(result)
}

fn scan_skills_dir(path: &std::path::Path) -> Vec<String> {
    if !path.exists() { return vec![]; }
    let mut ids = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && p.join("skill.md").exists() {
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    ids.push(name.to_string());
                }
            } else if p.is_file() && p.extension().map(|e| e == "md").unwrap_or(false) {
                if let Some(stem) = p.file_stem().and_then(|n| n.to_str()) {
                    ids.push(stem.to_string());
                }
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

/// Scans installed Kiro Powers under ~/.kiro/powers/installed/.
/// Returns list of power IDs found on disk.
#[tauri::command]
fn scan_installed_powers() -> Vec<String> {
    let Some(home) = dirs::home_dir() else { return vec![] };
    let powers_dir = home.join(".kiro").join("powers").join("installed");
    if !powers_dir.exists() {
        return vec![];
    }
    let mut powers: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&powers_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    powers.push(name.to_string());
                }
            }
        }
    }
    powers.sort();
    powers
}
// ---------------------------------------------------------------------------
// Install status with update detection
// ---------------------------------------------------------------------------

/// Status of a single installed item compared to the registry cache.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStatus {
    pub id: String,
    /// "new" | "up-to-date" | "outdated"
    pub status: String,
}

/// Scans installed skills/powers and compares them against the registry cache
/// to detect outdated items.
/// Returns per-item status for skills and powers.
/// Only checks items in `component_ids` — avoids hashing the entire registry.
#[tauri::command]
fn scan_installed_with_status(
    catalog_sources: std::collections::HashMap<String, String>,
    component_ids: Vec<String>,
) -> Result<std::collections::HashMap<String, Vec<ItemStatus>>, String> {
    use content_hasher::hash_path;
    use std::collections::HashMap;

    let home = dirs::home_dir().unwrap_or_default();
    let mut result: HashMap<String, Vec<ItemStatus>> = HashMap::new();

    let ids_set: std::collections::HashSet<&str> = component_ids.iter().map(|s| s.as_str()).collect();

    // Collect all unique registry source paths
    let registry_paths: Vec<std::path::PathBuf> = catalog_sources
        .values()
        .map(|p| std::path::PathBuf::from(p))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    // Build id→registry_hash only for the requested IDs
    let registry_hashes = |subdir: &str| -> HashMap<String, String> {
        let mut map = HashMap::new();
        for reg_path in &registry_paths {
            let dir = reg_path.join(subdir);
            if !dir.exists() { continue; }
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    let id = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                    if id.is_empty() || !ids_set.contains(id.as_str()) { continue; }
                    if let Some(h) = hash_path(&p) {
                        map.insert(id, h);
                    }
                }
            }
        }
        map
    };

    // --- Skills ---
    let skill_reg = registry_hashes("skills");
    let skill_install_dirs: Vec<(&str, std::path::PathBuf)> = vec![
        ("kiro",        home.join(".kiro").join("skills")),
        ("claude-code", home.join(".claude").join("skills")),
        ("cursor",      home.join(".cursor").join("skills")),
        ("windsurf",    home.join(".windsurf").join("extensions")),
        ("copilot",     home.join(".github").join("skills")),
    ];
    for (tool, dir) in &skill_install_dirs {
        if !dir.exists() { continue; }
        let mut statuses: Vec<ItemStatus> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let id = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                // Only check selected skills
                if id.is_empty() || !ids_set.contains(id.as_str()) { continue; }
                let status = match (hash_path(&p), skill_reg.get(&id)) {
                    (Some(installed), Some(registry)) => {
                        if installed == *registry { "up-to-date" } else { "outdated" }
                    }
                    (Some(_), None) => "up-to-date",
                    (None, _) => continue,
                };
                statuses.push(ItemStatus { id, status: status.to_string() });
            }
        }
        if !statuses.is_empty() {
            result.insert(tool.to_string(), statuses);
        }
    }

    // --- Powers ---
    let power_reg = registry_hashes("powers");
    let powers_dir = home.join(".kiro").join("powers").join("installed");
    if powers_dir.exists() {
        let mut statuses: Vec<ItemStatus> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&powers_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let id = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                if id.is_empty() || !ids_set.contains(id.as_str()) { continue; }
                let status = match (hash_path(&p), power_reg.get(&id)) {
                    (Some(installed), Some(registry)) => {
                        if installed == *registry { "up-to-date" } else { "outdated" }
                    }
                    (Some(_), None) => "up-to-date",
                    (None, _) => continue,
                };
                statuses.push(ItemStatus { id, status: status.to_string() });
            }
        }
        if !statuses.is_empty() {
            result.insert("powers".to_string(), statuses);
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Requirement checking
// ---------------------------------------------------------------------------

/// Reads `haal.json` sidecars for a list of resolved components and checks
/// all declared runtime/MCP requirements against the local system.
/// `mcp_being_installed` is the list of MCP server IDs selected in this session
/// so we can mark them as "provided" rather than "missing".
#[tauri::command]
fn check_requirements(
    components: Vec<models::ResolvedComponent>,
    mcp_being_installed: Vec<String>,
) -> Vec<requirement_checker::ComponentRequirements> {
    components.iter().filter_map(|comp| {
        let source = std::path::PathBuf::from(&comp.source_path);
        let type_str = format!("{:?}", comp.component_type).to_lowercase();
        requirement_checker::check_component(
            &comp.id,
            &type_str,
            &source,
            &mcp_being_installed,
        )
    }).collect()
}

/// Initializes the multi-registry catalog:/// 1. Clones/pulls the seed repo
/// 2. Reads repos-manifest.json from the seed to find additional repos
/// 3. Clones/pulls each additional repo (lower priority)
/// 4. Merges all catalogs — seed wins on conflict
/// Returns the merged catalog for the frontend to display.
#[tauri::command]
async fn initialize_catalog(
    seed_url: Option<String>,
    #[allow(unused_variables)]
    include_test_branches: Option<bool>,
) -> Result<MergedCatalog, String> {
    let url = seed_url
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| registry_manager::DEFAULT_REGISTRY_URL.to_string());

    // Derive the seed repo slug from the URL
    // e.g. "https://raw.githubusercontent.com/haal-ai/haal-skills/main/haal_manifest.json"
    //   → repo = "haal-ai/haal-skills", branch = "main"
    let (repo, branch) = parse_github_raw_url(&url)
        .unwrap_or_else(|| ("haal-ai/haal-skills".to_string(), "main".to_string()));

    let cache_root = SelfInstaller::haal_home().join("cache").join("repos");
    let manager = RepoManager::new(cache_root);

    let seed_path = manager.clone_or_pull(&repo, &branch, None)
        .map_err(|e| e.to_string())?;

    let include_test = include_test_branches.unwrap_or(false);
    manager.build_merged_catalog(&seed_path, include_test)
        .map_err(|e| e.to_string())
}

/// Parses a GitHub URL to extract (owner/repo, branch).
/// Supports:
///   - `https://raw.githubusercontent.com/<owner>/<repo>/<branch>/...`
///   - `https://github.com/<owner>/<repo>`
///   - `https://github.com/<owner>/<repo>/tree/<branch>`
///   - `<owner>/<repo>` (bare slug)
fn parse_github_raw_url(url: &str) -> Option<(String, String)> {
    let trimmed = url.trim().trim_end_matches('/');

    // raw.githubusercontent.com format
    if let Some(stripped) = trimmed.strip_prefix("https://raw.githubusercontent.com/") {
        let parts: Vec<&str> = stripped.splitn(4, '/').collect();
        if parts.len() >= 3 {
            return Some((format!("{}/{}", parts[0], parts[1]), parts[2].to_string()));
        }
    }

    // Regular github.com URL
    if let Some(stripped) = trimmed.strip_prefix("https://github.com/") {
        let stripped = stripped.trim_end_matches(".git");
        let parts: Vec<&str> = stripped.split('/').collect();
        // owner/repo/tree/branch
        if parts.len() >= 4 && parts[2] == "tree" {
            return Some((format!("{}/{}", parts[0], parts[1]), parts[3].to_string()));
        }
        // owner/repo
        if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return Some((format!("{}/{}", parts[0], parts[1]), "main".to_string()));
        }
    }

    // Bare slug: owner/repo
    let parts: Vec<&str> = trimmed.splitn(3, '/').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty()
        && !parts[0].contains(':')
    {
        return Some((format!("{}/{}", parts[0], parts[1]), "main".to_string()));
    }

    None
}

/// Installs components using the new multi-registry install engine.
/// Accepts a full `InstallRequest` built by the frontend from the merged catalog.
#[tauri::command]
async fn install_components_v2(
    app: tauri::AppHandle,
    request: InstallRequest,
) -> Result<InstallResult, String> {
    let installer = Installer::new(app, request.reinstall_all);
    Ok(installer.install(&request).await)
}


fn build_operation_engine() -> OperationEngine {
    let mut tool_adapters: HashMap<String, Box<dyn crate::traits::ToolAdapter>> = HashMap::new();
    tool_adapters.insert("copilot".to_string(), Box::new(CopilotAdapter::new()));
    tool_adapters.insert("cursor".to_string(), Box::new(CursorAdapter::new()));
    tool_adapters.insert("claude-code".to_string(), Box::new(ClaudeCodeAdapter::new()));
    tool_adapters.insert("kiro".to_string(), Box::new(KiroAdapter::new()));
    tool_adapters.insert("windsurf".to_string(), Box::new(WindsurfAdapter::new()));

    let haal_home = SelfInstaller::haal_home();
    let backup_dir = haal_home.join("backups");
    let metadata_path = haal_home.join("data").join("installed_metadata.json");

    let rollback_manager = Arc::new(RollbackManager::new(backup_dir));
    let checksum_validator = Arc::new(ChecksumValidator::new());
    let version_tracker = Arc::new(VersionTracker::new(metadata_path));
    let conflict_detector = Arc::new(ConflictDetector::new(Arc::clone(&version_tracker)));

    OperationEngine::new(
        tool_adapters,
        rollback_manager,
        checksum_validator,
        conflict_detector,
        version_tracker,
    )
}

/// Installs components to specified destinations.
#[tauri::command]
async fn install_components(
    components: Vec<Component>,
    destinations: Vec<Destination>,
) -> Result<OperationResult, String> {
    let engine = build_operation_engine();
    let progress = ProgressReporter {
        current_step: String::new(),
        percentage: 0,
        current_file: None,
    };
    engine
        .install(components, destinations, progress)
        .await
        .map_err(|e| e.to_string())
}

/// Updates components at all installed locations using full replacement.
#[tauri::command]
async fn update_components(components: Vec<Component>) -> Result<OperationResult, String> {
    let engine = build_operation_engine();
    let progress = ProgressReporter {
        current_step: String::new(),
        percentage: 0,
        current_file: None,
    };
    engine
        .update(components, progress)
        .await
        .map_err(|e| e.to_string())
}

/// Deletes components from all destinations and cleans up version tracking.
#[tauri::command]
async fn delete_components(components: Vec<Component>) -> Result<OperationResult, String> {
    let engine = build_operation_engine();
    let progress = ProgressReporter {
        current_step: String::new(),
        percentage: 0,
        current_file: None,
    };
    engine
        .delete(components, progress)
        .await
        .map_err(|e| e.to_string())
}

/// Reinitializes installation for selected tools, removing all their components.
#[tauri::command]
async fn reinitialize(tools: Vec<String>) -> Result<OperationResult, String> {
    let engine = build_operation_engine();
    let progress = ProgressReporter {
        current_step: String::new(),
        percentage: 0,
        current_file: None,
    };
    engine
        .reinitialize(tools, progress)
        .await
        .map_err(|e| e.to_string())
}

/// Detects conflicts before an operation by checking file existence,
/// version mismatches, and local modifications.
#[tauri::command]
async fn detect_conflicts(
    components: Vec<Component>,
    destinations: Vec<Destination>,
) -> Result<Vec<ConflictType>, String> {
    let haal_home = SelfInstaller::haal_home();
    let metadata_path = haal_home.join("data").join("installed_metadata.json");
    let version_tracker = Arc::new(VersionTracker::new(metadata_path));
    let detector = ConflictDetector::new(version_tracker);
    detector
        .detect_conflicts(&components, &destinations)
        .map_err(|e| e.to_string())
}

/// Exports the current configuration to a profile JSON file at the given path.
#[tauri::command]
fn export_configuration(output_path: String) -> Result<(), String> {
    let config_path = SelfInstaller::haal_home()
        .join("config")
        .join("configuration.json");
    let manager = ConfigurationManager::new(config_path);
    let mut profile = manager.export_profile_value().map_err(|e| e.to_string())?;

    // Merge last_install.json into the export so the full picture is captured
    if let Ok(Some(last)) = load_last_install() {
        if let Ok(v) = serde_json::to_value(&last) {
            profile.as_object_mut().unwrap().insert("last_install".to_string(), v);
        }
    }

    let json = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
    let path: std::path::PathBuf = output_path.into();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Imports a configuration profile from the given JSON file path.
#[tauri::command]
fn import_configuration(input_path: String) -> Result<ConfigurationProfile, String> {
    let config_path = SelfInstaller::haal_home()
        .join("config")
        .join("configuration.json");
    let manager = ConfigurationManager::new(config_path);

    // Read the file once, restore last_install if present, then import preferences
    let raw = std::fs::read_to_string(&input_path).map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    // Restore last_install profile if embedded
    if let Some(last_val) = value.get("last_install") {
        if let Ok(last) = serde_json::from_value::<LastInstallProfile>(last_val.clone()) {
            let _ = save_last_install(last);
        }
    }

    manager
        .import_profile(&input_path.into())
        .map_err(|e| e.to_string())
}

/// Returns the current user preferences from the configuration file.
#[tauri::command]
fn get_config() -> Result<UserPreferences, String> {
    let config_path = SelfInstaller::haal_home()
        .join("config")
        .join("configuration.json");
    let manager = ConfigurationManager::new(config_path);
    manager.load_config().map_err(|e| e.to_string())
}

/// Saves user preferences to the configuration file.
#[tauri::command]
fn save_config(preferences: UserPreferences) -> Result<(), String> {
    let config_path = SelfInstaller::haal_home()
        .join("config")
        .join("configuration.json");
    let manager = ConfigurationManager::new(config_path);
    manager
        .save_config(&preferences)
        .map_err(|e| e.to_string())
}

/// Reads log files from `~/.haal/logs/` and returns their content as a string.
#[tauri::command]
fn read_logs() -> Result<String, String> {
    let log_dir = SelfInstaller::haal_home().join("logs");
    if !log_dir.exists() {
        return Ok(String::new());
    }

    let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(&log_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| {
            p.extension()
                .map(|ext| ext == "log")
                .unwrap_or(false)
        })
        .collect();

    // Sort by filename so newest daily log comes last
    entries.sort();

    let mut combined = String::new();
    for path in entries {
        if let Ok(content) = std::fs::read_to_string(&path) {
            combined.push_str(&content);
        }
    }

    Ok(combined)
}

/// Exports logs to a user-specified path.
#[tauri::command]
fn export_logs(output_path: String) -> Result<(), String> {
    let content = read_logs()?;
    std::fs::write(&output_path, content).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Last-install profile — persists the user's choices so they can quick-update
// ---------------------------------------------------------------------------

/// The minimal set of choices needed to replay an install without the wizard.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastInstallProfile {
    /// Seed registry URL used during the install.
    pub seed_url: String,
    /// Competency IDs the user selected (already expanded from collections).
    pub competency_ids: Vec<String>,
    /// Tool names the user selected (e.g. ["Kiro", "Cursor"]).
    pub selected_tools: Vec<String>,
    /// Install scope: "home" | "repo" | "both".
    pub scope: String,
    /// Repo paths for multi-repo install.
    #[serde(default)]
    pub repo_paths: Vec<String>,
    /// DEPRECATED: single repo path — kept for reading old profiles.
    #[serde(default)]
    pub repo_path: String,
    /// ISO-8601 timestamp of the last install.
    pub installed_at: String,
}

fn last_install_path() -> std::path::PathBuf {
    SelfInstaller::haal_home()
        .join("config")
        .join("last_install.json")
}

/// Persists the user's install choices for future quick-updates.
#[tauri::command]
fn save_last_install(profile: LastInstallProfile) -> Result<(), String> {
    let path = last_install_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Loads the last install profile, or returns null if none exists.
#[tauri::command]
fn load_last_install() -> Result<Option<LastInstallProfile>, String> {
    let path = last_install_path();
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let profile: LastInstallProfile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(Some(profile))
}

/// Runs a quick-update using the last saved install profile.
/// Re-fetches the registry, resolves the same competencies, and re-installs.
#[tauri::command]
async fn quick_update(app: tauri::AppHandle) -> Result<InstallResult, String> {
    // Load saved profile
    let profile = load_last_install()?
        .ok_or_else(|| "No previous install found".to_string())?;

    // Re-initialize catalog from the same seed
    let seed_url = if profile.seed_url.is_empty() {
        DEFAULT_REGISTRY_URL.to_string()
    } else {
        profile.seed_url.clone()
    };
    let catalog = initialize_catalog(Some(seed_url), None).await?;

    // Resolve competency details and build component list
    let cache_root = SelfInstaller::haal_home().join("cache").join("repos");
    let _repo_mgr = RepoManager::new(cache_root);
    let reg_mgr = RegistryManager::new(
        profile.seed_url.clone(),
        SelfInstaller::haal_home().join("cache").join("manifests"),
    );

    let mut components: Vec<models::ResolvedComponent> = Vec::new();
    let seen = &mut std::collections::HashSet::new();

    for comp_id in &profile.competency_ids {
        // Find the competency entry in the merged catalog
        let entry = catalog.competencies.iter().find(|c| c.id == *comp_id);
        let source_path = catalog.competency_sources.get(comp_id).cloned()
            .unwrap_or_default();

        if let Some(entry) = entry {
            // Try local repo cache first (fast, no network) — same logic as the
            // Tauri command `fetch_competency`.
            let detail = {
                let local = source_path.join(&entry.manifest_url);
                if local.exists() {
                    std::fs::read_to_string(&local).ok()
                        .and_then(|c| parse_competency_json(&c).ok())
                } else {
                    None
                }
            };

            let detail = match detail {
                Some(d) => d,
                None => {
                    // Fall back to registry manager (remote → cache → local dev)
                    let base = source_path.to_string_lossy().to_string();
                    match reg_mgr.fetch_competency(entry, &base).await {
                        Ok(d) => d,
                        Err(e) => { eprintln!("WARN: Could not fetch competency {comp_id}: {e}"); continue; }
                    }
                }
            };

            {
                let add = |id: &str, ctype: &str, subdir: &str, comps: &mut Vec<models::ResolvedComponent>, seen: &mut std::collections::HashSet<String>| {
                    let key = format!("{ctype}:{id}");
                    if seen.insert(key) {
                        comps.push(models::ResolvedComponent {
                            id: id.to_string(),
                            component_type: match ctype {
                                "skill"     => models::ComponentType::Skill,
                                "power"     => models::ComponentType::Power,
                                "rule"      => models::ComponentType::Rule,
                                "hook"      => models::ComponentType::Hook,
                                "command"   => models::ComponentType::Command,
                                "agent"     => models::ComponentType::Agent,
                                "mcpServer" => models::ComponentType::McpServer,
                                _           => models::ComponentType::OlafData,
                            },
                            source_path: source_path.join(subdir).join(id),
                        });
                    }
                };
                for s in &detail.skills     { add(s, "skill",     "skills",     &mut components, seen); }
                for p in &detail.powers     { add(p, "power",     "powers",     &mut components, seen); }
                for r in &detail.rules      { add(r, "rule",      "rules",      &mut components, seen); }
                for h in &detail.hooks      { add(h, "hook",      "hooks",      &mut components, seen); }
                for c in &detail.commands   { add(c, "command",   "commands",   &mut components, seen); }
                for a in &detail.agents     { add(a, "agent",     "agents",     &mut components, seen); }
                for m in &detail.mcp_servers { add(m, "mcpServer", "mcpservers", &mut components, seen); }
            }
        }
    }

    let scope = match profile.scope.as_str() {
        "repo"  => models::InstallScope::Repo,
        "both"  => models::InstallScope::Both,
        _       => models::InstallScope::Home,
    };

    let request = InstallRequest {
        components,
        scope,
        repo_paths: {
            let mut paths = profile.repo_paths.clone();
            // Backward compat: old profiles only have repo_path
            if paths.is_empty() && !profile.repo_path.is_empty() {
                paths.push(profile.repo_path.clone());
            }
            paths.into_iter().map(|p| p.into()).collect()
        },
        selected_tools: profile.selected_tools,
        reinstall_all: true,
        clean_install: false,
    };

    let installer = Installer::new(app, true);
    Ok(installer.install(&request).await)
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to HAAL Installer.", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            check_self_install,
            self_install,
            create_desktop_shortcut,
            add_to_path,
            relaunch_from_home,
            authenticate_github,
            authenticate_gh_cli,
            check_gh_cli,
            launch_gh_auth_login,
            verify_repo_access,
            get_stored_credentials,
            check_network_status,
            fetch_registry,
            fetch_competency,
            scan_installed,
            scan_installed_at,
            scan_installed_powers,
            get_install_paths,
            get_current_dir,
            scan_nearby_git_repos,
            validate_git_repo,
            detect_tools,
            check_updates,
            install_components,
            update_components,
            delete_components,
            reinitialize,
            detect_conflicts,
            export_configuration,
            import_configuration,
            get_config,
            save_config,
            read_logs,
            export_logs,
            initialize_catalog,
            install_components_v2,
            read_mcp_server_def,
            scan_installed_with_status,
            check_requirements,
            fetch_system_def,
            install_system,
            update_system,
            delete_system,
            scan_installed_systems,
            get_systems_root,
            get_post_install_steps,
            save_last_install,
            load_last_install,
            quick_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
