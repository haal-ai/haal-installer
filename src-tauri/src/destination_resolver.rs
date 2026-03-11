use std::path::{Path, PathBuf};

use crate::models::{ComponentType, InstallScope, McpServerDef, ResolvedComponent};

/// A single file-system operation to perform during install.
#[derive(Debug, Clone)]
pub enum InstallOp {
    /// Copy a directory recursively (skills, powers, packages, agents).
    CopyDir { src: PathBuf, dest: PathBuf },
    /// Copy a single file (rules, hooks, commands).
    CopyFile { src: PathBuf, dest: PathBuf },
    /// Append text content to a file (global rules files like CLAUDE.md, AGENTS.md).
    AppendFile { src: PathBuf, dest: PathBuf },
    /// Merge an mcpServers entry into a JSON config file.
    MergeJson { server_def: McpServerDef, dest: PathBuf, json_key: String },
}

/// One resolved install action.
#[derive(Debug, Clone)]
pub struct InstallAction {
    pub component_id: String,
    pub op: InstallOp,
}

/// Resolves `ResolvedComponent` list → `Vec<InstallAction>` based on scope and selected tools.
pub struct DestinationResolver {
    home_dir: PathBuf,
    repo_path: Option<PathBuf>,
    scope: InstallScope,
    selected_tools: Vec<String>,
}

impl DestinationResolver {
    pub fn new(
        home_dir: PathBuf,
        repo_path: Option<PathBuf>,
        scope: InstallScope,
        selected_tools: Vec<String>,
    ) -> Self {
        Self { home_dir, repo_path, scope, selected_tools }
    }

    pub fn resolve(&self, components: &[ResolvedComponent]) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        for comp in components {
            actions.extend(self.resolve_one(comp));
        }
        actions
    }

    fn resolve_one(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        match comp.component_type {
            ComponentType::Skill     => self.resolve_skill(comp),
            ComponentType::Power     => self.resolve_power(comp),
            ComponentType::Rule      => self.resolve_rule(comp),
            ComponentType::Hook      => self.resolve_hook(comp),
            ComponentType::Command   => self.resolve_command(comp),
            ComponentType::Agent     => self.resolve_agent(comp),
            ComponentType::Package   => self.resolve_package(comp),
            ComponentType::OlafData  => self.resolve_olaf_data(comp),
            ComponentType::McpServer => self.resolve_mcp_server(comp),
        }
    }

    // -----------------------------------------------------------------------
    // Skills
    // -----------------------------------------------------------------------
    // Home: ~/.kiro/skills/<id>/  (and ~/.agents/skills/<id>/ for non-Kiro tools)
    // Repo: <repo>/.kiro/skills/<id>/  +  <repo>/.agents/skills/<id>/

    fn resolve_skill(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        let id = &comp.id;
        let src = &comp.source_path;

        if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
            for tool in &self.selected_tools {
                let dest = self.skill_home_path(tool, id);
                actions.push(InstallAction {
                    component_id: id.clone(),
                    op: InstallOp::CopyDir { src: src.clone(), dest },
                });
            }
        }

        if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
            if let Some(repo) = &self.repo_path {
                // .kiro/skills/<id>/ for Kiro
                actions.push(InstallAction {
                    component_id: id.clone(),
                    op: InstallOp::CopyDir {
                        src: src.clone(),
                        dest: repo.join(".kiro").join("skills").join(id),
                    },
                });
                // .agents/skills/<id>/ for other tools
                actions.push(InstallAction {
                    component_id: id.clone(),
                    op: InstallOp::CopyDir {
                        src: src.clone(),
                        dest: repo.join(".agents").join("skills").join(id),
                    },
                });
            }
        }

        actions
    }

    fn skill_home_path(&self, tool: &str, id: &str) -> PathBuf {
        let tool_lower = tool.to_lowercase();
        if tool_lower.contains("kiro") {
            self.home_dir.join(".kiro").join("skills").join(id)
        } else {
            // All other tools use ~/.agents/skills/<id>/
            self.home_dir.join(".agents").join("skills").join(id)
        }
    }

    // -----------------------------------------------------------------------
    // Powers — home only: ~/.kiro/powers/installed/<id>/
    // -----------------------------------------------------------------------

    fn resolve_power(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        vec![InstallAction {
            component_id: comp.id.clone(),
            op: InstallOp::CopyDir {
                src: comp.source_path.clone(),
                dest: self.home_dir.join(".kiro").join("powers").join("installed").join(&comp.id),
            },
        }]
    }

    // -----------------------------------------------------------------------
    // Rules
    // Source layout: rules/<subfolder>/<id>  where subfolder = kiro|cursor|copilot|windsurf|common
    //
    // Destinations per subfolder:
    //   kiro     → home: ~/.kiro/steering/<id>.md  | repo: <repo>/.kiro/steering/<id>.md
    //   cursor   → home: ~/.cursor/rules/<id>.mdc  | repo: <repo>/.cursor/rules/<id>.mdc
    //   copilot  → repo only: <repo>/.github/instructions/<id>.md
    //   windsurf → home: append ~/.codeium/windsurf/global_rules.md | repo: <repo>/.windsurf/rules/<id>.md
    //   common   → home: append ~/.claude/CLAUDE.md + append ~/.kiro/steering/AGENTS.md
    //              repo: append <repo>/AGENTS.md
    // -----------------------------------------------------------------------

    fn resolve_rule(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        let id = &comp.id;
        let src = &comp.source_path;

        // Determine subfolder from source path (rules/<subfolder>/<id>)
        let subfolder = self.detect_subfolder(src, "rules");

        match subfolder.as_deref() {
            Some("kiro") => {
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id, src, self.home_dir.join(".kiro").join("steering").join(format!("{id}.md"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src, repo.join(".kiro").join("steering").join(format!("{id}.md"))));
                    }
                }
            }
            Some("cursor") => {
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id, src, self.home_dir.join(".cursor").join("rules").join(format!("{id}.mdc"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src, repo.join(".cursor").join("rules").join(format!("{id}.mdc"))));
                    }
                }
            }
            Some("copilot") => {
                // Copilot: repo-scoped only
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src, repo.join(".github").join("instructions").join(format!("{id}.md"))));
                    }
                }
            }
            Some("windsurf") => {
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(InstallAction {
                        component_id: id.clone(),
                        op: InstallOp::AppendFile {
                            src: src.clone(),
                            dest: self.home_dir.join(".codeium").join("windsurf").join("global_rules.md"),
                        },
                    });
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src, repo.join(".windsurf").join("rules").join(format!("{id}.md"))));
                    }
                }
            }
            Some("common") | _ => {
                // common → AGENTS.md (cross-tool open standard)
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(InstallAction {
                        component_id: id.clone(),
                        op: InstallOp::AppendFile {
                            src: src.clone(),
                            dest: self.home_dir.join(".claude").join("CLAUDE.md"),
                        },
                    });
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(InstallAction {
                            component_id: id.clone(),
                            op: InstallOp::AppendFile {
                                src: src.clone(),
                                dest: repo.join("AGENTS.md"),
                            },
                        });
                    }
                }
            }
        }

        actions
    }

    // -----------------------------------------------------------------------
    // Hooks — repo only: <repo>/.kiro/hooks/<id>.kiro.hook
    // -----------------------------------------------------------------------

    fn resolve_hook(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
            if let Some(repo) = &self.repo_path {
                actions.push(self.copy_file_action(
                    &comp.id,
                    &comp.source_path,
                    repo.join(".kiro").join("hooks").join(format!("{}.kiro.hook", comp.id)),
                ));
            }
        }
        actions
    }

    // -----------------------------------------------------------------------
    // Commands
    // Source layout: commands/<subfolder>/<id>
    //   claude   → home: ~/.claude/commands/<id>.md  | repo: <repo>/.claude/commands/<id>.md
    //   kiro     → repo: <repo>/.github/prompts/<id>.md
    //   windsurf → repo: <repo>/.windsurf/commands/<id>.md
    //   github   → repo: <repo>/.github/prompts/<id>.md
    //   common   → home: ~/.claude/commands/<id>.md  | repo: <repo>/.github/prompts/<id>.md
    // -----------------------------------------------------------------------

    fn resolve_command(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        let id = &comp.id;
        let src = &comp.source_path;
        let subfolder = self.detect_subfolder(src, "commands");

        match subfolder.as_deref() {
            Some("claude") => {
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id, src, self.home_dir.join(".claude").join("commands").join(format!("{id}.md"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src, repo.join(".claude").join("commands").join(format!("{id}.md"))));
                    }
                }
            }
            Some("kiro") | Some("github") => {
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src, repo.join(".github").join("prompts").join(format!("{id}.md"))));
                    }
                }
            }
            Some("windsurf") => {
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src, repo.join(".windsurf").join("commands").join(format!("{id}.md"))));
                    }
                }
            }
            _ => {
                // common / unknown: claude home + github prompts repo
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id, src, self.home_dir.join(".claude").join("commands").join(format!("{id}.md"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src, repo.join(".github").join("prompts").join(format!("{id}.md"))));
                    }
                }
            }
        }

        actions
    }

    // -----------------------------------------------------------------------
    // Agents
    // Source layout: agents/<subfolder>/<id>
    //   github   → repo: <repo>/.github/agents/<id>/
    //   kiro     → repo: <repo>/.kiro/agents/<id>/
    //   claude   → repo: <repo>/.claude/agents/<id>/
    //   common   → all three above
    // -----------------------------------------------------------------------

    fn resolve_agent(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        let id = &comp.id;
        let src = &comp.source_path;
        let subfolder = self.detect_subfolder(src, "agents");

        let repo_dests: Vec<PathBuf> = match subfolder.as_deref() {
            Some("github") => vec![PathBuf::from(".github").join("agents").join(id)],
            Some("kiro")   => vec![PathBuf::from(".kiro").join("agents").join(id)],
            Some("claude") => vec![PathBuf::from(".claude").join("agents").join(id)],
            _ => vec![
                PathBuf::from(".github").join("agents").join(id),
                PathBuf::from(".kiro").join("agents").join(id),
                PathBuf::from(".claude").join("agents").join(id),
            ],
        };

        if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
            if let Some(repo) = &self.repo_path {
                for rel in repo_dests {
                    actions.push(InstallAction {
                        component_id: id.clone(),
                        op: InstallOp::CopyDir { src: src.clone(), dest: repo.join(rel) },
                    });
                }
            }
        }

        actions
    }

    // -----------------------------------------------------------------------
    // Packages — always global
    // Source layout: packages/<tool>/<id>
    //   claude → ~/.claude/plugins/<id>/
    //   kiro   → ~/.kiro/packages/<id>/
    // -----------------------------------------------------------------------

    fn resolve_package(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let subfolder = self.detect_subfolder(&comp.source_path, "packages");
        let dest = match subfolder.as_deref() {
            Some("claude") => self.home_dir.join(".claude").join("plugins").join(&comp.id),
            Some("kiro")   => self.home_dir.join(".kiro").join("packages").join(&comp.id),
            _ => return vec![],
        };
        vec![InstallAction {
            component_id: comp.id.clone(),
            op: InstallOp::CopyDir { src: comp.source_path.clone(), dest },
        }]
    }

    // -----------------------------------------------------------------------
    // OlafData — merge to ~/.olaf/data/ (no overwrite)
    // -----------------------------------------------------------------------

    fn resolve_olaf_data(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        vec![InstallAction {
            component_id: comp.id.clone(),
            op: InstallOp::CopyDir {
                src: comp.source_path.clone(),
                dest: self.home_dir.join(".olaf").join("data"),
            },
        }]
    }

    // -----------------------------------------------------------------------
    // MCP Servers — merge server entry into each tool's config file
    //
    // Reads mcpservers/<id>/mcp.json from the source path, then injects the
    // server definition into the appropriate config file per tool and scope.
    //
    // User-level targets:
    //   Kiro      → ~/.kiro/settings/mcp.json          key: mcpServers
    //   Cursor    → ~/.cursor/mcp.json                  key: mcpServers
    //   Claude    → ~/.claude/settings.json             key: mcpServers
    //   Windsurf  → ~/.codeium/windsurf/mcp_config.json key: mcpServers
    //
    // Workspace-level targets:
    //   Kiro      → <repo>/.kiro/settings/mcp.json     key: mcpServers
    //   Cursor    → <repo>/.cursor/mcp.json             key: mcpServers
    //   Claude    → <repo>/.claude/settings.json        key: mcpServers
    //   VS Code   → <repo>/.vscode/mcp.json             key: servers
    // -----------------------------------------------------------------------

    fn resolve_mcp_server(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        // Load the mcp.json definition from the source path
        let def_path = comp.source_path.join("mcp.json");
        let def: McpServerDef = match std::fs::read_to_string(&def_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
        {
            Some(d) => d,
            None => return vec![], // can't resolve without the definition
        };

        let mut actions = Vec::new();
        let id = &comp.id;

        // User-level installs
        if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
            for tool in &self.selected_tools {
                let tool_lower = tool.to_lowercase();
                let dest = if tool_lower.contains("kiro") {
                    Some((self.home_dir.join(".kiro").join("settings").join("mcp.json"), "mcpServers"))
                } else if tool_lower.contains("cursor") {
                    Some((self.home_dir.join(".cursor").join("mcp.json"), "mcpServers"))
                } else if tool_lower.contains("claude") {
                    Some((self.home_dir.join(".claude").join("settings.json"), "mcpServers"))
                } else if tool_lower.contains("windsurf") {
                    Some((self.home_dir.join(".codeium").join("windsurf").join("mcp_config.json"), "mcpServers"))
                } else {
                    None
                };
                if let Some((path, key)) = dest {
                    actions.push(InstallAction {
                        component_id: id.clone(),
                        op: InstallOp::MergeJson {
                            server_def: def.clone(),
                            dest: path,
                            json_key: key.to_string(),
                        },
                    });
                }
            }
        }

        // Workspace-level installs
        if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
            if let Some(repo) = &self.repo_path {
                // Kiro workspace
                actions.push(InstallAction {
                    component_id: id.clone(),
                    op: InstallOp::MergeJson {
                        server_def: def.clone(),
                        dest: repo.join(".kiro").join("settings").join("mcp.json"),
                        json_key: "mcpServers".to_string(),
                    },
                });
                // Cursor workspace
                actions.push(InstallAction {
                    component_id: id.clone(),
                    op: InstallOp::MergeJson {
                        server_def: def.clone(),
                        dest: repo.join(".cursor").join("mcp.json"),
                        json_key: "mcpServers".to_string(),
                    },
                });
                // Claude Code workspace
                actions.push(InstallAction {
                    component_id: id.clone(),
                    op: InstallOp::MergeJson {
                        server_def: def.clone(),
                        dest: repo.join(".claude").join("settings.json"),
                        json_key: "mcpServers".to_string(),
                    },
                });
                // VS Code workspace (different key: "servers")
                actions.push(InstallAction {
                    component_id: id.clone(),
                    op: InstallOp::MergeJson {
                        server_def: def.clone(),
                        dest: repo.join(".vscode").join("mcp.json"),
                        json_key: "servers".to_string(),
                    },
                });
            }
        }

        actions
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn copy_file_action(&self, id: &str, src: &Path, dest: PathBuf) -> InstallAction {
        InstallAction {
            component_id: id.to_string(),
            op: InstallOp::CopyFile { src: src.to_path_buf(), dest },
        }
    }

    /// Detects the subfolder name from a source path like `.../rules/kiro/<id>`.
    fn detect_subfolder(&self, src: &Path, parent: &str) -> Option<String> {
        let components: Vec<_> = src.components().collect();
        for (i, c) in components.iter().enumerate() {
            if c.as_os_str() == parent {
                if let Some(next) = components.get(i + 1) {
                    return Some(next.as_os_str().to_string_lossy().to_string());
                }
            }
        }
        None
    }
}
