use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::{FileSystemError, HaalError, ValidationError};
use crate::models::{ConfigurationProfile, Destination, Language, Theme, UserPreferences};

/// Manages export/import of configuration profiles and persists application settings.
pub struct ConfigurationManager {
    config_path: PathBuf,
}

impl ConfigurationManager {
    /// Creates a new ConfigurationManager reading from the given config path.
    pub fn new(config_path: PathBuf) -> Self {
        Self { config_path }
    }

    /// Exports the current configuration as a JSON value (for embedding in a larger export).
    pub fn export_profile_value(&self) -> Result<serde_json::Value, HaalError> {
        let preferences = self.load_config()?;

        let profile = if self.config_path.exists() {
            let raw = fs::read_to_string(&self.config_path).map_err(|e| {
                HaalError::FileSystem(FileSystemError {
                    message: format!("Failed to read config for export: {e}"),
                    path: Some(self.config_path.display().to_string()),
                })
            })?;
            let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
                HaalError::Validation(ValidationError {
                    message: format!("Invalid JSON in config file: {e}"),
                    field: None,
                })
            })?;

            let repositories: Vec<String> = value.get("repositories")
                .and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
            let selected_components: Vec<String> = value.get("selected_components")
                .and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
            let destinations: Vec<Destination> = value.get("destinations")
                .and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();

            ConfigurationProfile { repositories, selected_components, destinations, preferences }
        } else {
            ConfigurationProfile {
                repositories: Vec::new(),
                selected_components: Vec::new(),
                destinations: Vec::new(),
                preferences,
            }
        };

        serde_json::to_value(&profile).map_err(|e| {
            HaalError::Validation(ValidationError {
                message: format!("Failed to serialize profile: {e}"),
                field: None,
            })
        })
    }

    /// Exports the current configuration to a profile JSON file.
    ///
    /// Reads the current configuration from disk, wraps it in a
    /// `ConfigurationProfile`, and writes it as pretty-printed JSON to
    /// `output_path`.
    pub fn export_profile(&self, output_path: &PathBuf) -> Result<(), HaalError> {
        let preferences = self.load_config()?;

        // Build a profile from the current on-disk config.
        // If a full config file exists we try to pull repos/destinations from it;
        // otherwise we fall back to sensible defaults.
        let profile = if self.config_path.exists() {
            let raw = fs::read_to_string(&self.config_path).map_err(|e| {
                HaalError::FileSystem(FileSystemError {
                    message: format!("Failed to read config for export: {e}"),
                    path: Some(self.config_path.display().to_string()),
                })
            })?;
            let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
                HaalError::Validation(ValidationError {
                    message: format!("Invalid JSON in config file: {e}"),
                    field: None,
                })
            })?;

            let repositories: Vec<String> = value
                .get("repositories")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let selected_components: Vec<String> = value
                .get("selected_components")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let destinations: Vec<Destination> = value
                .get("destinations")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            ConfigurationProfile {
                repositories,
                selected_components,
                destinations,
                preferences,
            }
        } else {
            ConfigurationProfile {
                repositories: Vec::new(),
                selected_components: Vec::new(),
                destinations: Vec::new(),
                preferences,
            }
        };

        let json = serde_json::to_string_pretty(&profile).map_err(|e| {
            HaalError::Validation(ValidationError {
                message: format!("Failed to serialize profile: {e}"),
                field: None,
            })
        })?;

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                HaalError::FileSystem(FileSystemError {
                    message: format!("Failed to create output directory: {e}"),
                    path: Some(parent.display().to_string()),
                })
            })?;
        }

        fs::write(output_path, json).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to write profile: {e}"),
                path: Some(output_path.display().to_string()),
            })
        })?;

        Ok(())
    }

    /// Imports and applies a configuration profile from a JSON file.
    ///
    /// Reads the file at `input_path`, validates it can be deserialized into a
    /// `ConfigurationProfile`, and returns the parsed profile.
    pub fn import_profile(
        &self,
        input_path: &PathBuf,
    ) -> Result<ConfigurationProfile, HaalError> {
        if !input_path.exists() {
            return Err(HaalError::FileSystem(FileSystemError {
                message: "Profile file does not exist".to_string(),
                path: Some(input_path.display().to_string()),
            }));
        }

        let raw = fs::read_to_string(input_path).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to read profile: {e}"),
                path: Some(input_path.display().to_string()),
            })
        })?;

        let profile: ConfigurationProfile =
            serde_json::from_str(&raw).map_err(|e| {
                HaalError::Validation(ValidationError {
                    message: format!("Invalid configuration profile: {e}"),
                    field: None,
                })
            })?;

        // Apply the imported profile by saving it as the current config.
        self.save_full_config(&profile)?;

        Ok(profile)
    }

    /// Loads the application configuration (user preferences) from disk.
    ///
    /// Returns default preferences when the config file does not exist.
    pub fn load_config(&self) -> Result<UserPreferences, HaalError> {
        if !self.config_path.exists() {
            return Ok(Self::default_preferences());
        }

        let raw = fs::read_to_string(&self.config_path).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to read config: {e}"),
                path: Some(self.config_path.display().to_string()),
            })
        })?;

        let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
            HaalError::Validation(ValidationError {
                message: format!("Invalid JSON in config file: {e}"),
                field: None,
            })
        })?;

        // The preferences live under the "preferences" key in the full config.
        if let Some(prefs_value) = value.get("preferences") {
            let prefs: UserPreferences =
                serde_json::from_value(prefs_value.clone()).map_err(|e| {
                    HaalError::Validation(ValidationError {
                        message: format!("Invalid preferences in config: {e}"),
                        field: Some("preferences".to_string()),
                    })
                })?;
            Ok(prefs)
        } else {
            // Legacy or minimal config — try parsing the whole file as prefs.
            Ok(serde_json::from_value(value).unwrap_or_else(|_| Self::default_preferences()))
        }
    }

    /// Saves the application configuration (user preferences) to disk.
    ///
    /// Preserves other top-level keys (repositories, destinations, etc.) if
    /// the config file already exists.
    pub fn save_config(&self, preferences: &UserPreferences) -> Result<(), HaalError> {
        let mut value = if self.config_path.exists() {
            let raw = fs::read_to_string(&self.config_path).unwrap_or_default();
            serde_json::from_str::<serde_json::Value>(&raw)
                .unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        let prefs_value = serde_json::to_value(preferences).map_err(|e| {
            HaalError::Validation(ValidationError {
                message: format!("Failed to serialize preferences: {e}"),
                field: None,
            })
        })?;

        value
            .as_object_mut()
            .expect("config root must be an object")
            .insert("preferences".to_string(), prefs_value);

        self.write_config(&value)
    }

    // -----------------------------------------------------------------------
    // Destination management
    // -----------------------------------------------------------------------

    /// Enables or disables a destination by tool name.
    pub fn enable_destination(
        &self,
        tool_name: &str,
        enabled: bool,
    ) -> Result<(), HaalError> {
        let mut value = self.read_config_value()?;
        let destinations = self.get_destinations_mut(&mut value);

        let mut found = false;
        if let Some(arr) = destinations.as_array_mut() {
            for dest in arr.iter_mut() {
                if dest.get("tool_name").and_then(|v| v.as_str()) == Some(tool_name) {
                    dest.as_object_mut()
                        .unwrap()
                        .insert("enabled".to_string(), serde_json::json!(enabled));
                    found = true;
                }
            }
        }

        if !found {
            return Err(HaalError::Validation(ValidationError {
                message: format!("Destination for tool '{}' not found", tool_name),
                field: Some("tool_name".to_string()),
            }));
        }

        self.write_config(&value)
    }

    /// Sets a custom installation path for a destination identified by tool name.
    pub fn set_custom_path(
        &self,
        tool_name: &str,
        path: &Path,
    ) -> Result<(), HaalError> {
        let mut value = self.read_config_value()?;
        let destinations = self.get_destinations_mut(&mut value);

        let mut found = false;
        if let Some(arr) = destinations.as_array_mut() {
            for dest in arr.iter_mut() {
                if dest.get("tool_name").and_then(|v| v.as_str()) == Some(tool_name) {
                    dest.as_object_mut().unwrap().insert(
                        "path".to_string(),
                        serde_json::json!(path.to_string_lossy()),
                    );
                    found = true;
                }
            }
        }

        if !found {
            return Err(HaalError::Validation(ValidationError {
                message: format!("Destination for tool '{}' not found", tool_name),
                field: Some("tool_name".to_string()),
            }));
        }

        self.write_config(&value)
    }

    /// Validates whether files can be created at the given path.
    ///
    /// Returns `true` when the path is an existing writable directory, or when
    /// the path does not yet exist but its nearest existing ancestor is
    /// writable.
    pub fn validate_writability(path: &Path) -> bool {
        // If the path exists, try creating a temp file inside it.
        if path.is_dir() {
            let probe = path.join(".haal_write_probe");
            match fs::write(&probe, b"probe") {
                Ok(_) => {
                    let _ = fs::remove_file(&probe);
                    true
                }
                Err(_) => false,
            }
        } else {
            // Walk up to the nearest existing ancestor and check writability.
            let mut ancestor = path.to_path_buf();
            loop {
                if ancestor.is_dir() {
                    return Self::validate_writability(&ancestor);
                }
                if !ancestor.pop() {
                    return false;
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn default_preferences() -> UserPreferences {
        UserPreferences {
            theme: Theme::Dark,
            language: Language::English,
            auto_update: true,
            parallel_operations: true,
            enabled_tools: vec![],
            enabled_artifacts: vec![],
            use_test_branches: false,
        }
    }

    /// Saves a full `ConfigurationProfile` as the on-disk config.
    fn save_full_config(&self, profile: &ConfigurationProfile) -> Result<(), HaalError> {
        let value = serde_json::json!({
            "repositories": profile.repositories,
            "selected_components": profile.selected_components,
            "destinations": profile.destinations,
            "preferences": profile.preferences,
        });
        self.write_config(&value)
    }

    /// Reads the config file as a generic JSON value, returning an empty
    /// object when the file does not exist.
    fn read_config_value(&self) -> Result<serde_json::Value, HaalError> {
        if !self.config_path.exists() {
            return Ok(serde_json::json!({}));
        }
        let raw = fs::read_to_string(&self.config_path).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to read config: {e}"),
                path: Some(self.config_path.display().to_string()),
            })
        })?;
        serde_json::from_str(&raw).map_err(|e| {
            HaalError::Validation(ValidationError {
                message: format!("Invalid JSON in config: {e}"),
                field: None,
            })
        })
    }

    /// Returns a mutable reference to the `destinations` array inside the
    /// config value, creating it if absent.
    fn get_destinations_mut<'a>(
        &self,
        value: &'a mut serde_json::Value,
    ) -> &'a mut serde_json::Value {
        let obj = value.as_object_mut().expect("config root must be an object");
        if !obj.contains_key("destinations") {
            obj.insert("destinations".to_string(), serde_json::json!([]));
        }
        obj.get_mut("destinations").unwrap()
    }

    /// Writes a JSON value to the config file, creating parent directories as
    /// needed.
    fn write_config(&self, value: &serde_json::Value) -> Result<(), HaalError> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                HaalError::FileSystem(FileSystemError {
                    message: format!("Failed to create config directory: {e}"),
                    path: Some(parent.display().to_string()),
                })
            })?;
        }

        let json = serde_json::to_string_pretty(value).map_err(|e| {
            HaalError::Validation(ValidationError {
                message: format!("Failed to serialize config: {e}"),
                field: None,
            })
        })?;

        fs::write(&self.config_path, json).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to write config: {e}"),
                path: Some(self.config_path.display().to_string()),
            })
        })
    }
}
