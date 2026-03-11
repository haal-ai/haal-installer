pub mod adapters;
pub mod checksum_validator;
pub mod config_manager;
pub mod conflict_detector;
pub mod errors;
pub mod github_auth;
pub mod logging;
pub mod manifest_parser;
pub mod models;
pub mod offline_detector;
pub mod operation_engine;
pub mod registry_manager;
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
use models::{Component, ConfigurationProfile, Destination, OperationResult, UserPreferences};
use offline_detector::OfflineDetector;
use operation_engine::OperationEngine;
use registry_manager::{MasterManifest, RegistryManager, RepoManifest, DEFAULT_REGISTRY_URL};
use rollback_manager::RollbackManager;
use self_installer::SelfInstaller;
use serde::Serialize;
use tool_detector::{DetectedTool, ToolDetector};
use traits::ProgressReporter;
use version_tracker::{UpdateInfo, VersionTracker};

/// JSON-serializable status returned by the `check_self_install` command.
#[derive(Debug, Clone, Serialize)]
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

/// Fetches the master manifest from the HAAL registry.
/// Falls back to cached version if the registry is unreachable.
#[tauri::command]
async fn fetch_registry() -> Result<MasterManifest, String> {
    let cache_dir = SelfInstaller::haal_home().join("cache").join("manifests");
    let manager = RegistryManager::new(DEFAULT_REGISTRY_URL.to_string(), cache_dir);
    manager
        .fetch_master_manifest()
        .await
        .map_err(|e| e.to_string())
}

/// Fetches a repo manifest from a specific repository URL.
#[tauri::command]
async fn fetch_repo_manifest(repo_url: String, repo_id: String) -> Result<RepoManifest, String> {
    let cache_dir = SelfInstaller::haal_home().join("cache").join("manifests");
    let manager = RegistryManager::new(DEFAULT_REGISTRY_URL.to_string(), cache_dir);
    manager
        .fetch_repo_manifest(&repo_url, &repo_id)
        .await
        .map_err(|e| e.to_string())
}

/// Discovers all available components from all enabled repositories,
/// resolving duplicates by priority and respecting pinned components.
/// If `registry_url` is provided, uses that instead of the default registry.
#[tauri::command]
async fn discover_components(registry_url: Option<String>) -> Result<Vec<Component>, String> {
    let cache_dir = SelfInstaller::haal_home().join("cache").join("manifests");
    let url = registry_url
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_REGISTRY_URL.to_string());
    let manager = RegistryManager::new(url, cache_dir);
    manager
        .discover_all_components()
        .await
        .map_err(|e| e.to_string())
}

/// Refreshes all cached manifests (master + all enabled repo manifests).
#[tauri::command]
async fn refresh_manifests() -> Result<(), String> {
    let cache_dir = SelfInstaller::haal_home().join("cache").join("manifests");
    let manager = RegistryManager::new(DEFAULT_REGISTRY_URL.to_string(), cache_dir);
    manager
        .refresh_all_manifests()
        .await
        .map_err(|e| e.to_string())
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

    let cache_dir = SelfInstaller::haal_home().join("cache").join("manifests");
    let manager = RegistryManager::new(DEFAULT_REGISTRY_URL.to_string(), cache_dir);
    let repo_components = manager
        .discover_all_components()
        .await
        .map_err(|e| e.to_string())?;

    Ok(tracker.check_updates(&repo_components))
}

/// Builds an `OperationEngine` with all tool adapters and shared services.
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
            fetch_repo_manifest,
            discover_components,
            refresh_manifests,
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
            export_logs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
