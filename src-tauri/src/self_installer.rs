use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::errors::{FileSystemError, HaalError};
use crate::no_window_flags;

/// Subdirectories that must exist under HAAL_HOME.
const HOME_SUBDIRS: &[&str] = &[
    "bin",
    "config",
    "config/profiles",
    "data",
    "data/repositories",
    "data/manifests",
    "cache",
    "cache/staging",
    "cache/checksums",
    "backups",
    "logs",
];

/// Handles first-launch self-installation to ~/.haal/ and subsequent binary updates.
pub struct SelfInstaller {
    haal_home: PathBuf,
}

impl SelfInstaller {
    /// Creates a new SelfInstaller targeting the given HAAL home directory.
    pub fn new(haal_home: PathBuf) -> Self {
        Self { haal_home }
    }

    /// Returns the default HAAL home directory path (~/.haal).
    pub fn haal_home() -> PathBuf {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".haal")
    }

    /// Returns the path to the installed binary inside HAAL_HOME.
    fn installed_binary_path(&self) -> PathBuf {
        let bin_name = if cfg!(windows) {
            "haal-installer.exe"
        } else {
            "haal-installer"
        };
        self.haal_home.join("bin").join(bin_name)
    }

    /// Checks if the application is running from ~/.haal/bin/.
    pub fn is_installed(&self) -> bool {
        let bin_dir = self.haal_home.join("bin");
        match std::env::current_exe() {
            Ok(exe_path) => {
                // Canonicalize both paths to resolve symlinks and relative segments
                let canonical_exe = exe_path.canonicalize().unwrap_or(exe_path);
                let canonical_bin = bin_dir.canonicalize().unwrap_or(bin_dir);
                canonical_exe.starts_with(&canonical_bin)
            }
            Err(_) => false,
        }
    }

    /// Checks if the ~/.haal/ directory structure exists.
    ///
    /// Returns `true` only when the root and every required subdirectory are present.
    pub fn home_exists(&self) -> bool {
        if !self.haal_home.exists() {
            return false;
        }
        HOME_SUBDIRS.iter().all(|sub| self.haal_home.join(sub).is_dir())
    }

    /// Creates the full ~/.haal/ directory structure.
    ///
    /// Uses `create_dir_all` so it is safe to call even when some directories
    /// already exist (idempotent).
    pub fn create_home_structure(&self) -> Result<(), HaalError> {
        for sub in HOME_SUBDIRS {
            let dir = self.haal_home.join(sub);
            fs::create_dir_all(&dir).map_err(|e| {
                HaalError::FileSystem(FileSystemError {
                    message: format!("Failed to create directory {}: {e}", dir.display()),
                    path: Some(dir.to_string_lossy().into_owned()),
                })
            })?;
        }
        Ok(())
    }

    /// Copies the current running executable to ~/.haal/bin/.
    pub fn install_binary(&self) -> Result<(), HaalError> {
        let current_exe = std::env::current_exe().map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to determine current executable path: {e}"),
                path: None,
            })
        })?;

        let dest = self.installed_binary_path();

        // Ensure the bin directory exists
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                HaalError::FileSystem(FileSystemError {
                    message: format!("Failed to create bin directory: {e}"),
                    path: Some(parent.to_string_lossy().into_owned()),
                })
            })?;
        }

        fs::copy(&current_exe, &dest).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!(
                    "Failed to copy binary from {} to {}: {e}",
                    current_exe.display(),
                    dest.display()
                ),
                path: Some(dest.to_string_lossy().into_owned()),
            })
        })?;

        // Preserve executable permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o755);
            fs::set_permissions(&dest, perms).map_err(|e| {
                HaalError::FileSystem(FileSystemError {
                    message: format!("Failed to set executable permissions: {e}"),
                    path: Some(dest.to_string_lossy().into_owned()),
                })
            })?;
        }

        Ok(())
    }

    /// Compares the running binary with the installed binary to decide if an
    /// update is needed.
    ///
    /// Uses file size as a fast first check, then falls back to byte-level
    /// comparison when sizes match. Returns `Ok(true)` when the installed
    /// binary is absent or differs from the running one.
    pub fn needs_update(&self) -> Result<bool, HaalError> {
        let current_exe = std::env::current_exe().map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to determine current executable path: {e}"),
                path: None,
            })
        })?;

        let installed = self.installed_binary_path();

        if !installed.exists() {
            return Ok(true);
        }

        let current_meta = fs::metadata(&current_exe).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to read metadata for {}: {e}", current_exe.display()),
                path: Some(current_exe.to_string_lossy().into_owned()),
            })
        })?;

        let installed_meta = fs::metadata(&installed).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to read metadata for {}: {e}", installed.display()),
                path: Some(installed.to_string_lossy().into_owned()),
            })
        })?;

        // Quick check: different sizes means definitely different
        if current_meta.len() != installed_meta.len() {
            return Ok(true);
        }

        // Same size — compare contents
        let current_bytes = fs::read(&current_exe).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to read current binary: {e}"),
                path: Some(current_exe.to_string_lossy().into_owned()),
            })
        })?;

        let installed_bytes = fs::read(&installed).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to read installed binary: {e}"),
                path: Some(installed.to_string_lossy().into_owned()),
            })
        })?;

        Ok(current_bytes != installed_bytes)
    }

    /// Replaces the binary in ~/.haal/bin/ with the currently running binary.
    ///
    /// On platforms where the running binary cannot be overwritten directly
    /// (Windows), the old file is renamed before copying.
    pub fn update_binary(&self) -> Result<(), HaalError> {
        let installed = self.installed_binary_path();

        // Remove old binary first (handles Windows lock issues via rename)
        if installed.exists() {
            let backup = installed.with_extension("old");
            fs::rename(&installed, &backup).map_err(|e| {
                HaalError::FileSystem(FileSystemError {
                    message: format!("Failed to rename old binary: {e}"),
                    path: Some(installed.to_string_lossy().into_owned()),
                })
            })?;

            // Best-effort cleanup of the backup; ignore errors
            let _ = fs::remove_file(&backup);
        }

        self.install_binary()
    }

    /// Creates a desktop shortcut pointing to ~/.haal/bin/.
    pub fn create_desktop_shortcut(&self) -> Result<(), HaalError> {
        // Placeholder — platform-specific implementation needed
        Ok(())
    }

    /// Adds ~/.haal/bin/ to the system PATH permanently.
    ///
    /// - macOS/Linux: appends an export line to ~/.zshrc, ~/.bashrc, ~/.profile
    /// - Windows: adds the directory to the user-level PATH registry key via `setx`
    pub fn add_to_path(&self) -> Result<(), HaalError> {
        let bin_dir = self.haal_home.join("bin");
        let bin_str = bin_dir.to_string_lossy().into_owned();

        #[cfg(windows)]
        {
            // Read current user PATH from registry
            let mut cmd = Command::new("powershell");
            cmd.args([
                    "-NoProfile", "-Command",
                    "[System.Environment]::GetEnvironmentVariable('PATH','User')",
                ]);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(no_window_flags());
            }
            let output = cmd.output()
                .map_err(|e| HaalError::FileSystem(FileSystemError {
                    message: format!("Failed to read PATH: {e}"),
                    path: None,
                }))?;

            let current = String::from_utf8_lossy(&output.stdout);
            let current = current.trim();

            // Only add if not already present
            if !current.split(';').any(|p| p.trim().eq_ignore_ascii_case(&bin_str)) {
                let new_path = if current.is_empty() {
                    bin_str.clone()
                } else {
                    format!("{current};{bin_str}")
                };
                let mut cmd = Command::new("powershell");
                cmd.args([
                        "-NoProfile", "-Command",
                        &format!(
                            "[System.Environment]::SetEnvironmentVariable('PATH','{new_path}','User')"
                        ),
                    ]);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(no_window_flags());
                }
                cmd.status()
                    .map_err(|e| HaalError::FileSystem(FileSystemError {
                        message: format!("Failed to set PATH: {e}"),
                        path: None,
                    }))?;
            }
        }

        #[cfg(unix)]
        {
            let export_line = format!("\nexport PATH=\"{bin_str}:$PATH\"\n");
            let marker = format!("# haal-installer managed path: {bin_str}");

            // Shell rc files to update
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let candidates = [
                format!("{home}/.zshrc"),
                format!("{home}/.bashrc"),
                format!("{home}/.profile"),
            ];

            for rc in &candidates {
                let path = std::path::Path::new(rc);
                // Only update files that exist
                if !path.exists() {
                    continue;
                }
                let content = fs::read_to_string(path).unwrap_or_default();
                // Skip if already added
                if content.contains(&marker) {
                    continue;
                }
                let addition = format!("{marker}{export_line}");
                fs::write(path, format!("{content}{addition}")).map_err(|e| {
                    HaalError::FileSystem(FileSystemError {
                        message: format!("Failed to update {rc}: {e}"),
                        path: Some(rc.clone()),
                    })
                })?;
            }
        }

        Ok(())
    }

    /// Relaunches the application from ~/.haal/bin/ and exits the current
    /// process.
    ///
    /// Spawns the installed binary as a detached child, forwarding any
    /// command-line arguments, then terminates the current process.
    pub fn relaunch_from_home(&self) -> Result<(), HaalError> {
        let installed = self.installed_binary_path();

        if !installed.exists() {
            return Err(HaalError::FileSystem(FileSystemError {
                message: format!(
                    "Installed binary not found at {}",
                    installed.display()
                ),
                path: Some(installed.to_string_lossy().into_owned()),
            }));
        }

        // Collect current args (skip argv[0])
        let args: Vec<String> = std::env::args().skip(1).collect();

        Command::new(&installed)
            .args(&args)
            .spawn()
            .map_err(|e| {
                HaalError::FileSystem(FileSystemError {
                    message: format!(
                        "Failed to relaunch from {}: {e}",
                        installed.display()
                    ),
                    path: Some(installed.to_string_lossy().into_owned()),
                })
            })?;

        // Exit the current process so only the relaunched instance runs
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_installer() -> (SelfInstaller, TempDir) {
        let tmp = TempDir::new().unwrap();
        let haal_home = tmp.path().join(".haal");
        let installer = SelfInstaller::new(haal_home);
        (installer, tmp)
    }

    // -- haal_home ---------------------------------------------------------

    #[test]
    fn haal_home_ends_with_dot_haal() {
        let path = SelfInstaller::haal_home();
        assert!(
            path.ends_with(".haal"),
            "Expected path ending in .haal, got: {}",
            path.display()
        );
    }

    // -- home_exists -------------------------------------------------------

    #[test]
    fn home_exists_returns_false_when_missing() {
        let (installer, _tmp) = make_installer();
        assert!(!installer.home_exists());
    }

    #[test]
    fn home_exists_returns_false_when_partial() {
        let (installer, _tmp) = make_installer();
        // Create only the root, not subdirs
        fs::create_dir_all(&installer.haal_home).unwrap();
        assert!(!installer.home_exists());
    }

    #[test]
    fn home_exists_returns_true_after_create() {
        let (installer, _tmp) = make_installer();
        installer.create_home_structure().unwrap();
        assert!(installer.home_exists());
    }

    // -- create_home_structure ---------------------------------------------

    #[test]
    fn create_home_structure_creates_all_subdirs() {
        let (installer, _tmp) = make_installer();
        installer.create_home_structure().unwrap();

        for sub in HOME_SUBDIRS {
            let dir = installer.haal_home.join(sub);
            assert!(dir.is_dir(), "Missing directory: {}", dir.display());
        }
    }

    #[test]
    fn create_home_structure_is_idempotent() {
        let (installer, _tmp) = make_installer();
        installer.create_home_structure().unwrap();
        // Second call should succeed without error
        installer.create_home_structure().unwrap();
        assert!(installer.home_exists());
    }

    // -- is_installed ------------------------------------------------------

    #[test]
    fn is_installed_returns_false_when_not_in_bin() {
        let (installer, _tmp) = make_installer();
        // The test binary is not inside the temp .haal/bin/
        assert!(!installer.is_installed());
    }

    // -- install_binary ----------------------------------------------------

    #[test]
    fn install_binary_copies_exe_to_bin() {
        let (installer, _tmp) = make_installer();
        installer.create_home_structure().unwrap();
        installer.install_binary().unwrap();

        let dest = installer.installed_binary_path();
        assert!(dest.exists(), "Binary not found at {}", dest.display());
        assert!(dest.metadata().unwrap().len() > 0);
    }

    // -- needs_update ------------------------------------------------------

    #[test]
    fn needs_update_returns_true_when_no_installed_binary() {
        let (installer, _tmp) = make_installer();
        installer.create_home_structure().unwrap();
        assert!(installer.needs_update().unwrap());
    }

    #[test]
    fn needs_update_returns_false_after_install() {
        let (installer, _tmp) = make_installer();
        installer.create_home_structure().unwrap();
        installer.install_binary().unwrap();
        assert!(!installer.needs_update().unwrap());
    }

    // -- update_binary -----------------------------------------------------

    #[test]
    fn update_binary_replaces_existing() {
        let (installer, _tmp) = make_installer();
        installer.create_home_structure().unwrap();
        installer.install_binary().unwrap();

        // Tamper with the installed binary to simulate an older version
        let dest = installer.installed_binary_path();
        fs::write(&dest, b"old-binary-content").unwrap();
        assert!(installer.needs_update().unwrap());

        installer.update_binary().unwrap();
        assert!(!installer.needs_update().unwrap());
    }

    // -- relaunch_from_home ------------------------------------------------

    #[test]
    fn relaunch_from_home_errors_when_binary_missing() {
        let (installer, _tmp) = make_installer();
        installer.create_home_structure().unwrap();
        let result = installer.relaunch_from_home();
        assert!(result.is_err());
    }
}
