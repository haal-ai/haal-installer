use std::path::{Path, PathBuf};

use crate::models::{ComponentType, InstallScope, McpServerDef, ResolvedComponent};

/// A single file-system operation to perform during install.
#[derive(Debug, Clone)]
pub enum InstallOp {
    /// Copy a directory recursively (skills, powers, packages, agents).
    CopyDir { src: PathBuf, dest: PathBuf },
    /// Copy a single file (rules, hooks, commands).
    CopyFile { src: PathBuf, dest: PathBuf },
    /// Copy a file and prepend frontmatter text before the content.
    InjectFrontmatter { src: PathBuf, dest: PathBuf, frontmatter: String },
    /// Append text content to a file (global rules files like CLAUDE.md, AGENTS.md).
    AppendFile { src: PathBuf, dest: PathBuf },
    /// Merge an mcpServers entry into a JSON config file.
    MergeJson { server_def: McpServerDef, dest: PathBuf, json_key: String },
    /// Clone a git repo into ~/.haal/systems/<id>/ (systems).
    CloneRepo { id: String, repo: String, branch: String },
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
            ComponentType::System    => self.resolve_system(comp),
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
        } else if tool_lower.contains("copilot") || tool_lower.contains("vs code") {
            self.home_dir.join(".github").join("copilot").join(id)
        } else {
            // Cursor, Windsurf, Claude Code etc.
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
    //
    // Source layout: rules/<subfolder>/<id>/rule.md
    //   subfolder = common | kiro | cursor | copilot | windsurf | claude
    //
    // Override precedence: tool-specific subfolder wins over common for that tool.
    // For common/, the installer injects the correct frontmatter per tool.
    //
    // common → deploys to ALL tools with injected frontmatter:
    //   kiro     → ~/.kiro/steering/<id>.md          frontmatter: inclusion: always
    //   cursor   → ~/.cursor/rules/<id>.mdc           frontmatter: description/globs/alwaysApply
    //   copilot  → <repo>/.github/instructions/<id>.instructions.md  frontmatter: applyTo: "**"
    //   windsurf → home: append global_rules.md (no frontmatter)
    //              repo: <repo>/.windsurf/rules/<id>.md (no frontmatter)
    //   claude   → home: append ~/.claude/CLAUDE.md (no frontmatter)
    //
    // kiro     → ~/.kiro/steering/<id>.md  (file already has correct frontmatter)
    // cursor   → ~/.cursor/rules/<id>.mdc  (file already has correct frontmatter)
    // copilot  → <repo>/.github/instructions/<id>.instructions.md (already has applyTo)
    // windsurf → home: append global_rules.md | repo: .windsurf/rules/<id>.md
    // claude   → home: append CLAUDE.md | repo: append AGENTS.md
    // -----------------------------------------------------------------------

    fn resolve_rule(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        let id = &comp.id;
        let src = &comp.source_path;
        let subfolder = self.detect_subfolder(src, "rules");

        match subfolder.as_deref() {
            // ── common: deploy to all tools, inject frontmatter ──────────────
            Some("common") | None => {
                // Kiro: inject "inclusion: always" frontmatter
                let kiro_fm = "---\ninclusion: always\n---\n";
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_with_frontmatter(id, src,
                        self.home_dir.join(".kiro").join("steering").join(format!("{id}.md")),
                        kiro_fm));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_with_frontmatter(id, src,
                            repo.join(".kiro").join("steering").join(format!("{id}.md")),
                            kiro_fm));
                    }
                }

                // Cursor: inject alwaysApply frontmatter
                let cursor_fm = "---\ndescription: \"\"\nglobs: \"\"\nalwaysApply: true\n---\n";
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_with_frontmatter(id, src,
                        self.home_dir.join(".cursor").join("rules").join(format!("{id}.mdc")),
                        cursor_fm));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_with_frontmatter(id, src,
                            repo.join(".cursor").join("rules").join(format!("{id}.mdc")),
                            cursor_fm));
                    }
                }

                // Copilot: inject applyTo frontmatter, repo-scoped only
                let copilot_fm = "---\napplyTo: \"**\"\n---\n";
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_with_frontmatter(id, src,
                            repo.join(".github").join("instructions").join(format!("{id}.instructions.md")),
                            copilot_fm));
                    }
                }

                // Windsurf: no frontmatter — append to global, copy to repo
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
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".windsurf").join("rules").join(format!("{id}.md"))));
                    }
                }

                // Claude: append to CLAUDE.md / AGENTS.md (no frontmatter)
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

            // ── kiro: file already has correct frontmatter ───────────────────
            Some("kiro") => {
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id, src,
                        self.home_dir.join(".kiro").join("steering").join(format!("{id}.md"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".kiro").join("steering").join(format!("{id}.md"))));
                    }
                }
            }

            // ── cursor: file already has correct frontmatter ─────────────────
            Some("cursor") => {
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id, src,
                        self.home_dir.join(".cursor").join("rules").join(format!("{id}.mdc"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".cursor").join("rules").join(format!("{id}.mdc"))));
                    }
                }
            }

            // ── copilot: file already has applyTo frontmatter ────────────────
            Some("copilot") => {
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".github").join("instructions").join(format!("{id}.instructions.md"))));
                    }
                }
            }

            // ── windsurf: no frontmatter ─────────────────────────────────────
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
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".windsurf").join("rules").join(format!("{id}.md"))));
                    }
                }
            }

            // ── claude: no frontmatter ───────────────────────────────────────
            Some("claude") | _ => {
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
    //
    // Source layout: commands/<subfolder>/<id>/command.md
    //   subfolder = common | claude | kiro | copilot | cursor | windsurf
    //
    // Override precedence: tool-specific subfolder wins over common for that tool.
    //
    // common → deploys to ALL tools with injected frontmatter where needed:
    //   claude   → home: ~/.claude/commands/<id>.md          (no frontmatter)
    //              repo: <repo>/.claude/commands/<id>.md
    //   kiro     → home: ~/.kiro/steering/<id>.md            (frontmatter: inclusion: manual)
    //              repo: <repo>/.kiro/steering/<id>.md       (Kiro has no native commands — steering with manual inclusion IS the slash command)
    //   copilot  → repo: <repo>/.github/prompts/<id>.prompt.md  (frontmatter: mode: "agent", description: "")
    //   cursor   → home: ~/.cursor/commands/<id>.md          (no frontmatter)
    //              repo: <repo>/.cursor/commands/<id>.md
    //   windsurf → repo: <repo>/.windsurf/workflows/<id>.md  (no frontmatter — Windsurf calls these "workflows")
    //
    // Tool-specific subfolders: file already has correct frontmatter baked in.
    //   claude   → same paths as above
    //   kiro     → same paths as above (steering with inclusion: manual already in file)
    //   copilot  → same paths as above (.prompt.md extension, frontmatter already in file)
    //   cursor   → same paths as above
    //   windsurf → same paths as above
    // -----------------------------------------------------------------------

    fn resolve_command(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        let id = &comp.id;
        let src = &comp.source_path;
        let subfolder = self.detect_subfolder(src, "commands");

        match subfolder.as_deref() {
            // ── common: deploy to all tools ──────────────────────────────────
            Some("common") | None => {
                // Claude: plain markdown, no frontmatter
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id, src,
                        self.home_dir.join(".claude").join("commands").join(format!("{id}.md"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".claude").join("commands").join(format!("{id}.md"))));
                    }
                }

                // Kiro: steering file with inclusion: manual (= slash command in Kiro)
                let kiro_fm = "---\ninclusion: manual\n---\n";
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_with_frontmatter(id, src,
                        self.home_dir.join(".kiro").join("steering").join(format!("{id}.md")),
                        kiro_fm));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_with_frontmatter(id, src,
                            repo.join(".kiro").join("steering").join(format!("{id}.md")),
                            kiro_fm));
                    }
                }

                // Copilot: .prompt.md with mode/description frontmatter, repo-scoped only
                let copilot_fm = "---\nmode: \"agent\"\ndescription: \"\"\n---\n";
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_with_frontmatter(id, src,
                            repo.join(".github").join("prompts").join(format!("{id}.prompt.md")),
                            copilot_fm));
                    }
                }

                // Cursor: plain markdown, no frontmatter
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id, src,
                        self.home_dir.join(".cursor").join("commands").join(format!("{id}.md"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".cursor").join("commands").join(format!("{id}.md"))));
                    }
                }

                // Windsurf: plain markdown, goes to workflows/, repo-scoped only
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".windsurf").join("workflows").join(format!("{id}.md"))));
                    }
                }
            }

            // ── claude: file already correct ─────────────────────────────────
            Some("claude") => {
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id, src,
                        self.home_dir.join(".claude").join("commands").join(format!("{id}.md"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".claude").join("commands").join(format!("{id}.md"))));
                    }
                }
            }

            // ── kiro: steering with inclusion: manual already in file ─────────
            Some("kiro") => {
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id, src,
                        self.home_dir.join(".kiro").join("steering").join(format!("{id}.md"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".kiro").join("steering").join(format!("{id}.md"))));
                    }
                }
            }

            // ── copilot: .prompt.md with frontmatter already in file ──────────
            Some("copilot") => {
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".github").join("prompts").join(format!("{id}.prompt.md"))));
                    }
                }
            }

            // ── cursor: plain markdown ────────────────────────────────────────
            Some("cursor") => {
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id, src,
                        self.home_dir.join(".cursor").join("commands").join(format!("{id}.md"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".cursor").join("commands").join(format!("{id}.md"))));
                    }
                }
            }

            // ── windsurf: plain markdown, workflows/ ──────────────────────────
            Some("windsurf") | _ => {
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".windsurf").join("workflows").join(format!("{id}.md"))));
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
                } else if tool_lower.contains("vscode") || tool_lower.contains("copilot") || tool_lower.contains("vs code") {
                    // VS Code user-level MCP config
                    dirs::config_dir().map(|c| (c.join("Code").join("User").join("mcp.json"), "servers"))
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
    // Systems — clone repo into ~/.haal/systems/<id>/
    // -----------------------------------------------------------------------
    // source_path carries the GitHub repo URL (set by buildResolvedComponents).

    fn resolve_system(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let repo = comp.source_path.to_string_lossy().to_string();
        vec![InstallAction {
            component_id: comp.id.clone(),
            op: InstallOp::CloneRepo {
                id: comp.id.clone(),
                repo,
                branch: "main".to_string(),
            },
        }]
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

    fn copy_with_frontmatter(&self, id: &str, src: &Path, dest: PathBuf, frontmatter: &str) -> InstallAction {
        InstallAction {
            component_id: id.to_string(),
            op: InstallOp::InjectFrontmatter {
                src: src.to_path_buf(),
                dest,
                frontmatter: frontmatter.to_string(),
            },
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
