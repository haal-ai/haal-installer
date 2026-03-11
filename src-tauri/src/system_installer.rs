use std::path::{Path, PathBuf};
use std::process::Command;

use crate::models::{InstalledSystemInfo, SystemDef, SystemEntry, SystemStatus};

/// Root directory where all systems are installed: ~/.haal/systems/
pub fn systems_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".haal")
        .join("systems")
}

/// Path for a specific system: ~/.haal/systems/<id>/
pub fn system_path(id: &str) -> PathBuf {
    systems_root().join(id)
}

// ---------------------------------------------------------------------------
// Fetch system.json from a remote repo
// ---------------------------------------------------------------------------

/// Fetches `system.json` from the root of the given GitHub repo URL.
/// Converts https://github.com/org/repo → raw URL.
pub async fn fetch_system_def(entry: &SystemEntry) -> Result<SystemDef, String> {
    let branch = entry.branch.as_deref().unwrap_or("main");
    let raw_url = github_to_raw(&entry.repo, branch, "system.json")?;

    let text = reqwest::Client::new()
        .get(&raw_url)
        .header("User-Agent", "haal-installer")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch system.json: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Failed to read system.json: {e}"))?;

    serde_json::from_str::<SystemDef>(&text)
        .map_err(|e| format!("Failed to parse system.json from {raw_url}: {e}"))
}

fn github_to_raw(repo_url: &str, branch: &str, file: &str) -> Result<String, String> {
    // Strip trailing slash and .git
    let base = repo_url.trim_end_matches('/').trim_end_matches(".git");
    // https://github.com/org/repo → https://raw.githubusercontent.com/org/repo/<branch>/<file>
    let raw = base
        .replace("https://github.com/", "https://raw.githubusercontent.com/");
    Ok(format!("{raw}/{branch}/{file}"))
}

// ---------------------------------------------------------------------------
// Install (clone)
// ---------------------------------------------------------------------------

/// Clones the system repo into ~/.haal/systems/<id>/.
/// Sets the original repo as `upstream` remote so updates can be pulled.
pub fn install_system(entry: &SystemEntry) -> Result<PathBuf, String> {
    let dest = system_path(&entry.id);

    if dest.exists() {
        return Err(format!(
            "System '{}' is already installed at {}",
            entry.id,
            dest.display()
        ));
    }

    std::fs::create_dir_all(dest.parent().unwrap())
        .map_err(|e| format!("Failed to create systems directory: {e}"))?;

    let branch = entry.branch.as_deref().unwrap_or("main");

    // git clone --branch <branch> --depth 1 <url> <dest>
    run_git(&[
        "clone",
        "--branch", branch,
        "--depth", "1",
        &entry.repo,
        dest.to_str().unwrap(),
    ], None)?;

    Ok(dest)
}

// ---------------------------------------------------------------------------
// Update (git pull)
// ---------------------------------------------------------------------------

pub fn update_system(id: &str) -> Result<(), String> {
    let dest = system_path(id);
    if !dest.exists() {
        return Err(format!("System '{id}' is not installed"));
    }
    run_git(&["pull", "--ff-only"], Some(&dest))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

pub fn delete_system(id: &str) -> Result<(), String> {
    let dest = system_path(id);
    if !dest.exists() {
        return Err(format!("System '{id}' is not installed"));
    }
    std::fs::remove_dir_all(&dest)
        .map_err(|e| format!("Failed to delete system '{id}': {e}"))
}

// ---------------------------------------------------------------------------
// Scan installed systems
// ---------------------------------------------------------------------------

pub fn scan_installed_systems(entries: &[SystemEntry]) -> Vec<InstalledSystemInfo> {
    entries
        .iter()
        .map(|e| {
            let path = system_path(&e.id);
            if !path.exists() {
                return InstalledSystemInfo {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    install_path: path.display().to_string(),
                    status: SystemStatus::NotInstalled,
                    current_commit: None,
                };
            }

            let commit = get_current_commit(&path);
            let status = check_update_available(&path);

            InstalledSystemInfo {
                id: e.id.clone(),
                name: e.name.clone(),
                install_path: path.display().to_string(),
                status,
                current_commit: commit,
            }
        })
        .collect()
}

fn get_current_commit(path: &Path) -> Option<String> {
    run_git_output(&["rev-parse", "--short", "HEAD"], Some(path)).ok()
}

fn check_update_available(path: &Path) -> SystemStatus {
    // Fetch remote silently, then compare
    let _ = run_git(&["fetch", "--quiet"], Some(path));
    match run_git_output(&["rev-list", "--count", "HEAD..@{u}"], Some(path)) {
        Ok(s) => {
            let behind: u32 = s.trim().parse().unwrap_or(0);
            if behind > 0 {
                SystemStatus::UpdateAvailable
            } else {
                SystemStatus::Installed
            }
        }
        // If git commands fail, assume installed but can't determine update status
        Err(_) => SystemStatus::Installed,
    }
}

// ---------------------------------------------------------------------------
// Post-install helper — returns commands as strings for the UI to display
// ---------------------------------------------------------------------------

pub fn post_install_commands(def: &SystemDef, install_path: &Path) -> Vec<String> {
    let mut cmds = Vec::new();
    let path_str = install_path.display();

    // Prefer explicit install.commands if provided
    if let Some(install) = &def.install {
        for cmd in &install.commands {
            cmds.push(format!("cd {path_str} && {cmd}"));
        }
    } else {
        // Fallback: auto-detect from prerequisites flags
        if def.prerequisites.pip {
            cmds.push(format!("cd {path_str} && pip install -e \".[all]\""));
        }
        if def.prerequisites.npm {
            cmds.push(format!("cd {path_str} && npm install"));
        }
    }

    if let Some(post) = &def.post_install {
        for cmd in &post.commands {
            cmds.push(format!("cd {path_str} && {cmd}"));
        }
    }
    cmds
}

pub fn post_install_message(def: &SystemDef) -> Option<String> {
    def.post_install.as_ref().and_then(|p| p.message.clone())
}

// ---------------------------------------------------------------------------
// Git helpers
// ---------------------------------------------------------------------------

fn run_git(args: &[&str], cwd: Option<&Path>) -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn run_git_output(args: &[&str], cwd: Option<&Path>) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
