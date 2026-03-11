use std::path::Path;

use tauri::{AppHandle, Emitter};

use crate::destination_resolver::{InstallAction, InstallOp};
use crate::errors::{FileSystemError, HaalError};
use crate::models::{ComponentFailure, InstallRequest, InstallResult, McpServerDef, McpTransport};
use crate::destination_resolver::DestinationResolver;

/// Progress event payload emitted to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgressEvent {
    pub step: String,
    pub percentage: u32,
    pub current_component: Option<String>,
    pub components_succeeded: Vec<String>,
    pub components_failed: Vec<String>,
}

/// Executes a list of `InstallAction`s, emitting progress events via Tauri.
pub struct Installer {
    app: AppHandle,
    reinstall_all: bool,
}

impl Installer {
    pub fn new(app: AppHandle, reinstall_all: bool) -> Self {
        Self { app, reinstall_all }
    }

    pub async fn install(&self, request: &InstallRequest) -> InstallResult {
        let home_dir = dirs::home_dir().unwrap_or_default();
        let resolver = DestinationResolver::new(
            home_dir,
            request.repo_path.clone(),
            request.scope.clone(),
            request.selected_tools.clone(),
        );

        let actions = resolver.resolve(&request.components);
        let total = actions.len().max(1);

        let mut succeeded: Vec<String> = Vec::new();
        let mut failed: Vec<ComponentFailure> = Vec::new();
        // Track which component IDs we've already counted
        let mut seen_failed: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut seen_succeeded: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (i, action) in actions.iter().enumerate() {
            let pct = ((i as f32 / total as f32) * 100.0) as u32;
            self.emit_progress(InstallProgressEvent {
                step: format!("Installing {}", action.component_id),
                percentage: pct,
                current_component: Some(action.component_id.clone()),
                components_succeeded: succeeded.clone(),
                components_failed: failed.iter().map(|f| f.component_id.clone()).collect(),
            });

            match self.execute_action(action) {
                Ok(()) => {
                    if !seen_failed.contains(&action.component_id) {
                        seen_succeeded.insert(action.component_id.clone());
                    }
                }
                Err(e) => {
                    if !seen_succeeded.contains(&action.component_id) && !seen_failed.contains(&action.component_id) {
                        seen_failed.insert(action.component_id.clone());
                        failed.push(ComponentFailure {
                            component_id: action.component_id.clone(),
                            error: e.to_string(),
                        });
                    }
                }
            }
        }

        succeeded = seen_succeeded.into_iter().collect();
        succeeded.sort();

        self.emit_progress(InstallProgressEvent {
            step: "Done".to_string(),
            percentage: 100,
            current_component: None,
            components_succeeded: succeeded.clone(),
            components_failed: failed.iter().map(|f| f.component_id.clone()).collect(),
        });

        InstallResult {
            success: failed.is_empty(),
            components_succeeded: succeeded,
            components_failed: failed,
        }
    }

    fn execute_action(&self, action: &InstallAction) -> Result<(), HaalError> {
        match &action.op {
            InstallOp::CopyDir { src, dest } => self.copy_dir(src, dest),
            InstallOp::CopyFile { src, dest } => self.copy_file(src, dest),
            InstallOp::AppendFile { src, dest } => self.append_file(src, dest),
            InstallOp::MergeJson { server_def, dest, json_key } => self.merge_json(server_def, dest, json_key),
        }
    }

    fn copy_dir(&self, src: &Path, dest: &Path) -> Result<(), HaalError> {
        if dest.exists() && !self.reinstall_all {
            return Ok(()); // skip existing
        }
        if dest.exists() {
            std::fs::remove_dir_all(dest).map_err(|e| fs_err(e, dest))?;
        }
        std::fs::create_dir_all(dest).map_err(|e| fs_err(e, dest))?;
        copy_dir_recursive(src, dest)
    }

    fn copy_file(&self, src: &Path, dest: &Path) -> Result<(), HaalError> {
        if dest.exists() && !self.reinstall_all {
            return Ok(());
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| fs_err(e, parent))?;
        }
        std::fs::copy(src, dest).map_err(|e| fs_err(e, dest))?;
        Ok(())
    }

    fn append_file(&self, src: &Path, dest: &Path) -> Result<(), HaalError> {
        let content = std::fs::read_to_string(src).map_err(|e| fs_err(e, src))?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| fs_err(e, parent))?;
        }
        // Check if content already present (idempotent)
        if dest.exists() {
            let existing = std::fs::read_to_string(dest).unwrap_or_default();
            if existing.contains(content.trim()) {
                return Ok(());
            }
        }
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dest)
            .map_err(|e| fs_err(e, dest))?;
        writeln!(file, "\n{content}").map_err(|e| fs_err(e, dest))?;
        Ok(())
    }

    /// Merges an MCP server entry into a JSON config file.
    /// Reads the existing file (or starts with `{}`), injects/updates the server
    /// under `json_key.<server_id>`, then writes back.
    fn merge_json(&self, def: &McpServerDef, dest: &Path, json_key: &str) -> Result<(), HaalError> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| fs_err(e, parent))?;
        }

        // Read existing or start fresh
        let mut root: serde_json::Value = if dest.exists() {
            let content = std::fs::read_to_string(dest).map_err(|e| fs_err(e, dest))?;
            serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        // Build the server entry in the format each tool expects
        let entry = self.build_mcp_entry(def);

        // Ensure the key exists as an object
        if !root[json_key].is_object() {
            root[json_key] = serde_json::json!({});
        }

        // Skip if already present and reinstall_all is false
        if root[json_key][&def.id].is_object() && !self.reinstall_all {
            return Ok(());
        }

        root[json_key][&def.id] = entry;

        let json = serde_json::to_string_pretty(&root).map_err(|e| {
            HaalError::FileSystem(FileSystemError {
                message: format!("Failed to serialize MCP config: {e}"),
                path: Some(dest.display().to_string()),
            })
        })?;
        std::fs::write(dest, json).map_err(|e| fs_err(e, dest))?;
        Ok(())
    }

    fn build_mcp_entry(&self, def: &McpServerDef) -> serde_json::Value {
        match def.transport {
            McpTransport::Http => {
                let url = def.server_url.as_deref().unwrap_or("");
                serde_json::json!({ "url": url })
            }
            McpTransport::Stdio => {
                let mut entry = serde_json::json!({
                    "command": def.command.as_deref().unwrap_or(""),
                    "args": def.args,
                });
                if !def.env.is_empty() {
                    entry["env"] = serde_json::to_value(&def.env).unwrap_or_default();
                }
                entry
            }
        }
    }

    fn emit_progress(&self, event: InstallProgressEvent) {
        let _ = self.app.emit("install-progress", event);
    }
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), HaalError> {
    for entry in std::fs::read_dir(src).map_err(|e| fs_err(e, src))? {
        let entry = entry.map_err(|e| fs_err(e, src))?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if src_path.is_dir() {
            std::fs::create_dir_all(&dest_path).map_err(|e| fs_err(e, &dest_path))?;
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path).map_err(|e| fs_err(e, &dest_path))?;
        }
    }
    Ok(())
}

fn fs_err(e: impl std::fmt::Display, path: &Path) -> HaalError {
    HaalError::FileSystem(FileSystemError {
        message: e.to_string(),
        path: Some(path.display().to_string()),
    })
}
