pub mod adapters;
pub mod checksum_validator;
pub mod config_manager;
pub mod conflict_detector;
pub mod content_hasher;
pub mod destination_resolver;
pub mod requirement_checker;pub mod errors;
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
    let installed = std::process::Command::new("gh")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !installed {
        return serde_json::json!({ "installed": false, "authenticated": false });
    }

    // Unset GITHUB_TOKEN/GH_TOKEN so we check stored credentials, not the env var
    let authenticated = std::process::Command::new("gh")
        .args(["auth", "status"])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !authenticated {
        return serde_json::json!({ "installed": true, "authenticated": false, "username": null });
    }

    // Fetch the username of the stored account
    let username = std::process::Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .ok()
        .and_then(|o| if o.status.success() { Some(o.stdout) } else { None })
        .map(|b| String::from_utf8_lossy(&b).trim().to_string())
        .filter(|s| !s.is_empty());

    serde_json::json!({ "installed": true, "authenticated": true, "username": username })
}

/// Launches `gh auth login` in a new terminal window so the user can authenticate.
/// Unsets GITHUB_TOKEN first so gh stores credentials rather than using the env var.
#[tauri::command]
fn launch_gh_auth_login() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(["/c", "start", "cmd", "/k", "gh auth login"])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    std::process::Command::new("osascript")
        .args(["-e", "tell app \"Terminal\" to do script \"unset GITHUB_TOKEN; unset GH_TOKEN; gh auth login\""])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("sh")
        .args(["-c", "x-terminal-emulator -e 'unset GITHUB_TOKEN; unset GH_TOKEN; gh auth login' || xterm -e 'unset GITHUB_TOKEN; unset GH_TOKEN; gh auth login'"])
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
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
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
        return serde_json::from_str::<CompetencyDetail>(&content)
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
async fn initialize_catalog(seed_url: Option<String>) -> Result<MergedCatalog, String> {
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

    manager.build_merged_catalog(&seed_path)
        .map_err(|e| e.to_string())
}

/// Parses a GitHub raw URL to extract (owner/repo, branch).
fn parse_github_raw_url(url: &str) -> Option<(String, String)> {
    // https://raw.githubusercontent.com/<owner>/<repo>/<branch>/...
    let stripped = url.strip_prefix("https://raw.githubusercontent.com/")?;
    let parts: Vec<&str> = stripped.splitn(4, '/').collect();
    if parts.len() >= 3 {
        Some((format!("{}/{}", parts[0], parts[1]), parts[2].to_string()))
    } else {
        None
    }
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
    manager
        .export_profile(&output_path.into())
        .map_err(|e| e.to_string())
}

/// Imports a configuration profile from the given JSON file path.
#[tauri::command]
fn import_configuration(input_path: String) -> Result<ConfigurationProfile, String> {
    let config_path = SelfInstaller::haal_home()
        .join("config")
        .join("configuration.json");
    let manager = ConfigurationManager::new(config_path);
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
            check_requirements
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
