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
    /// Clone a git repo into ~/.haal/systems/<id>/ (systems).
    CloneRepo { id: String, repo: String, branch: String },
    /// Ensure an entry exists in a .gitignore file (idempotent append).
    MergeGitignore { entry: String, dest: PathBuf },
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
        // Resolve the actual on-disk path — the frontend may omit the subfolder
        // (e.g. `rules/test-rule-kiro` instead of `rules/kiro/test-rule-kiro/`)
        let resolved = ResolvedComponent {
            source_path: self.resolve_component_path(&comp.source_path),
            ..comp.clone()
        };
        let comp = &resolved;
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
    // Home paths (per tool):
    //   Kiro        → ~/.kiro/skills/<id>/
    //   Copilot     → ~/.github/skills/<id>/
    //   Claude Code → ~/.claude/skills/<id>/
    //   Cursor / Windsurf / others → ~/.agents/skills/<id>/  (agentskills.io standard)
    //
    // Repo paths (written for all selected tools, deduplicated):
    //   .kiro/skills/<id>/    (Kiro IDE + CLI)
    //   .claude/skills/<id>/  (Claude Code)
    //   .agents/skills/<id>/  (Windsurf, Cursor, and agentskills.io-compatible tools)
    // -----------------------------------------------------------------------

    fn resolve_skill(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        let id = &comp.id;
        let src = &comp.source_path;

        if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
            // Deduplicate: multiple tools may map to the same home path
            let mut seen_dests = std::collections::HashSet::new();
            for tool in &self.selected_tools {
                let dest = self.skill_home_path(tool, id);
                if seen_dests.insert(dest.clone()) {
                    actions.push(InstallAction {
                        component_id: id.clone(),
                        op: InstallOp::CopyDir { src: src.clone(), dest },
                    });
                }
            }
        }

        if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
            if let Some(repo) = &self.repo_path {
                let has_kiro    = self.selected_tools.iter().any(|t| t.to_lowercase().contains("kiro"));
                let has_claude  = self.selected_tools.iter().any(|t| t.to_lowercase().contains("claude"));
                let has_agents  = self.selected_tools.iter().any(|t| {
                    let tl = t.to_lowercase();
                    tl.contains("cursor") || tl.contains("windsurf") || tl.contains("copilot")
                });

                if has_kiro {
                    actions.push(InstallAction {
                        component_id: id.clone(),
                        op: InstallOp::CopyDir { src: src.clone(), dest: repo.join(".kiro").join("skills").join(id) },
                    });
                }
                if has_claude {
                    actions.push(InstallAction {
                        component_id: id.clone(),
                        op: InstallOp::CopyDir { src: src.clone(), dest: repo.join(".claude").join("skills").join(id) },
                    });
                }
                if has_agents {
                    actions.push(InstallAction {
                        component_id: id.clone(),
                        op: InstallOp::CopyDir { src: src.clone(), dest: repo.join(".agents").join("skills").join(id) },
                    });
                }
            }
        }

        actions
    }

    fn skill_home_path(&self, tool: &str, id: &str) -> PathBuf {
        let tool_lower = tool.to_lowercase();
        if tool_lower.contains("kiro") {
            self.home_dir.join(".kiro").join("skills").join(id)
        } else if tool_lower.contains("copilot") {
            self.home_dir.join(".github").join("skills").join(id)
        } else if tool_lower.contains("claude") {
            self.home_dir.join(".claude").join("skills").join(id)
        } else {
            // Cursor, Windsurf, and agentskills.io-compatible tools
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
    //   subfolder = kiro | cursor | copilot | windsurf | claude
    //
    // No common/ subfolder — each tool variant is authored in the registry
    // with the correct frontmatter already baked in. Unknown subfolders are skipped.
    //
    // kiro     → home: ~/.kiro/steering/<id>.md
    //            repo: <repo>/.kiro/steering/<id>.md
    // cursor   → home: ~/.cursor/rules/<id>.mdc
    //            repo: <repo>/.cursor/rules/<id>.mdc
    // copilot  → home: ~/.copilot/instructions/<id>.instructions.md  (VS Code user-level)
    //            repo: <repo>/.github/instructions/<id>.instructions.md
    // windsurf → home: append ~/.codeium/windsurf/global_rules.md
    //            repo: <repo>/.windsurf/rules/<id>.md
    // claude   → home: append ~/.claude/CLAUDE.md
    //            repo: append <repo>/AGENTS.md
    // -----------------------------------------------------------------------

    fn resolve_rule(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
            let mut actions = Vec::new();
            let id = &comp.id;
            // source_path is the rule file itself (any .md filename)
            let src = &comp.source_path;

            // Registry layout: rules/<scope>/<tool>/file.md
            //   scope = global | repo
            //   tool  = kiro | cursor | copilot | windsurf | claude
            let (scope_folder, tool_folder) = self.detect_two_subfolders(src, "rules");
            let is_global = scope_folder.as_deref() == Some("global");
            let is_repo   = scope_folder.as_deref() == Some("repo");

            // Only act if the install scope matches the registry scope
            let want_global = self.scope == InstallScope::Home || self.scope == InstallScope::Both;
            let want_repo   = self.scope == InstallScope::Repo || self.scope == InstallScope::Both;

            match tool_folder.as_deref() {
                Some("kiro") => {
                    if is_global && want_global {
                        actions.push(self.copy_file_action(id, src,
                            self.home_dir.join(".kiro").join("steering").join(format!("{id}.md"))));
                    }
                    if is_repo && want_repo {
                        if let Some(repo) = &self.repo_path {
                            actions.push(self.copy_file_action(id, src,
                                repo.join(".kiro").join("steering").join(format!("{id}.md"))));
                        }
                    }
                }
                Some("cursor") => {
                    if is_global && want_global {
                        actions.push(self.copy_file_action(id, src,
                            self.home_dir.join(".cursor").join("rules").join(format!("{id}.mdc"))));
                    }
                    if is_repo && want_repo {
                        if let Some(repo) = &self.repo_path {
                            actions.push(self.copy_file_action(id, src,
                                repo.join(".cursor").join("rules").join(format!("{id}.mdc"))));
                        }
                    }
                }
                // Copilot: global → single ~/.copilot/copilot-instructions.md
                //          repo   → <repo>/.github/instructions/<filename>.instructions.md (extension mandatory)
                Some("copilot") => {
                    if is_global && want_global {
                        actions.push(self.copy_file_action(id, src,
                            self.home_dir.join(".copilot").join("copilot-instructions.md")));
                    }
                    if is_repo && want_repo {
                        if let Some(repo) = &self.repo_path {
                            // Preserve original filename but ensure .instructions.md extension
                            let filename = src.file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_else(|| format!("{id}.instructions.md"));
                            let filename = if filename.ends_with(".instructions.md") {
                                filename
                            } else {
                                format!("{}.instructions.md", filename.trim_end_matches(".md"))
                            };
                            actions.push(self.copy_file_action(id, src,
                                repo.join(".github").join("instructions").join(filename)));
                        }
                    }
                }
                // Windsurf: global → single ~/.codeium/windsurf/global_rules.md
                //           repo   → <repo>/.windsurf/rules/<filename>.md
                Some("windsurf") => {
                    if is_global && want_global {
                        actions.push(self.copy_file_action(id, src,
                            self.home_dir.join(".codeium").join("windsurf").join("global_rules.md")));
                    }
                    if is_repo && want_repo {
                        if let Some(repo) = &self.repo_path {
                            let filename = src.file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_else(|| format!("{id}.md"));
                            actions.push(self.copy_file_action(id, src,
                                repo.join(".windsurf").join("rules").join(filename)));
                        }
                    }
                }
                // Claude: global → ~/.claude/rules/<filename>.md  (multiple files supported)
                //         repo   → <repo>/.claude/rules/<filename>.md
                Some("claude") => {
                    if is_global && want_global {
                        let filename = src.file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| format!("{id}.md"));
                        actions.push(self.copy_file_action(id, src,
                            self.home_dir.join(".claude").join("rules").join(filename)));
                    }
                    if is_repo && want_repo {
                        if let Some(repo) = &self.repo_path {
                            let filename = src.file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_else(|| format!("{id}.md"));
                            actions.push(self.copy_file_action(id, src,
                                repo.join(".claude").join("rules").join(filename)));
                        }
                    }
                }
                // agents: cross-tool AGENTS.md — repo-scoped only
                //         repo → appended to <repo>/AGENTS.md
                // Registry authors explicitly opt in by placing files under rules/repo/agents/.
                // Plain markdown, no frontmatter. Multiple files are appended in order.
                Some("agents") => {
                    if is_repo && want_repo {
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
                _ => {}
            }

            actions
        }


    // -----------------------------------------------------------------------
    // Hooks
    //
    // Source layout: hooks/<subfolder>/<id>/hook.json
    //   kiro    → repo: <repo>/.kiro/hooks/<id>.kiro.hook
    //   copilot → repo: <repo>/.github/hooks/<id>.json
    //   (no subfolder) → treated as kiro (backward compat)
    // -----------------------------------------------------------------------

    fn resolve_hook(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        if self.scope != InstallScope::Repo && self.scope != InstallScope::Both {
            return actions;
        }
        let Some(repo) = &self.repo_path else { return actions; };

        let subfolder = self.detect_subfolder(&comp.source_path, "hooks");
        match subfolder.as_deref() {
            Some("copilot") => {
                // Copilot hooks: .github/hooks/<id>.json
                actions.push(self.copy_file_action(
                    &comp.id,
                    &comp.source_path.join("hook.json"),
                    repo.join(".github").join("hooks").join(format!("{}.json", comp.id)),
                ));
            }
            // kiro or no subfolder → .kiro/hooks/<id>.kiro.hook
            _ => {
                actions.push(self.copy_file_action(
                    &comp.id,
                    &comp.source_path.join("hook.json"),
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
    //   subfolder = claude | kiro | copilot | cursor | windsurf
    //
    // No common/ subfolder — each tool variant is authored in the registry
    // with the correct frontmatter already baked in. Unknown subfolders are skipped.
    //
    //   claude   → home: ~/.claude/commands/<filename>.md     (invoked as /user:name)
    //              repo: <repo>/.claude/commands/<filename>.md
    //   kiro     → home: ~/.kiro/steering/<filename>.md       (inclusion: manual = slash command)
    //              repo: <repo>/.kiro/steering/<filename>.md
    //   copilot  → repo: <repo>/.github/prompts/<filename>.prompt.md  (repo-scoped only)
    //   cursor   → home: ~/.cursor/commands/<filename>.md
    //              repo: <repo>/.cursor/commands/<filename>.md
    //   windsurf → home: ~/.codeium/windsurf/global_workflows/<filename>.md
    //              repo: <repo>/.windsurf/workflows/<filename>.md
    // -----------------------------------------------------------------------

    fn resolve_command(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        let id = &comp.id;
        // source_path is the command file itself (any .md filename)
        let src = &comp.source_path;

        // Registry layout: commands/<scope>/<tool>/file.md
        //   scope = global | repo
        //   tool  = kiro | claude | cursor | copilot | windsurf
        //
        // global scope:
        //   kiro     → ~/.kiro/steering/<filename>.md                      (inclusion: manual = slash command)
        //   claude   → ~/.claude/commands/<filename>.md                    (invoked as /user:name)
        //   cursor   → ~/.cursor/commands/<filename>.md
        //   windsurf → ~/.codeium/windsurf/global_workflows/<filename>.md
        //   copilot  → no portable global path (VS Code profile path varies per machine)
        //
        // repo scope:
        //   kiro     → <repo>/.kiro/steering/<filename>.md
        //   claude   → <repo>/.claude/commands/<filename>.md
        //   cursor   → <repo>/.cursor/commands/<filename>.md
        //   copilot  → <repo>/.github/prompts/<filename>.prompt.md  (extension mandatory)
        //   windsurf → <repo>/.windsurf/workflows/<filename>.md

        let (scope_folder, tool_folder) = self.detect_two_subfolders(src, "commands");
        let is_global = scope_folder.as_deref() == Some("global");
        let is_repo   = scope_folder.as_deref() == Some("repo");
        let want_global = self.scope == InstallScope::Home || self.scope == InstallScope::Both;
        let want_repo   = self.scope == InstallScope::Repo || self.scope == InstallScope::Both;

        let filename = src.file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("{id}.md"));

        match tool_folder.as_deref() {
            Some("kiro") => {
                if is_global && want_global {
                    actions.push(self.copy_file_action(id, src,
                        self.home_dir.join(".kiro").join("steering").join(&filename)));
                }
                if is_repo && want_repo {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".kiro").join("steering").join(&filename)));
                    }
                }
            }
            Some("claude") => {
                if is_global && want_global {
                    actions.push(self.copy_file_action(id, src,
                        self.home_dir.join(".claude").join("commands").join(&filename)));
                }
                if is_repo && want_repo {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".claude").join("commands").join(&filename)));
                    }
                }
            }
            Some("cursor") => {
                if is_global && want_global {
                    actions.push(self.copy_file_action(id, src,
                        self.home_dir.join(".cursor").join("commands").join(&filename)));
                }
                if is_repo && want_repo {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".cursor").join("commands").join(&filename)));
                    }
                }
            }
            // Copilot: repo-only, must end with .prompt.md
            Some("copilot") => {
                if is_repo && want_repo {
                    if let Some(repo) = &self.repo_path {
                        let prompt_filename = if filename.ends_with(".prompt.md") {
                            filename.clone()
                        } else {
                            format!("{}.prompt.md", filename.trim_end_matches(".md"))
                        };
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".github").join("prompts").join(prompt_filename)));
                    }
                }
            }
            // Windsurf: global → ~/.codeium/windsurf/global_workflows/<filename>.md
            //           repo   → <repo>/.windsurf/workflows/<filename>.md
            Some("windsurf") => {
                if is_global && want_global {
                    actions.push(self.copy_file_action(id, src,
                        self.home_dir.join(".codeium").join("windsurf").join("global_workflows").join(&filename)));
                }
                if is_repo && want_repo {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id, src,
                            repo.join(".windsurf").join("workflows").join(&filename)));
                    }
                }
            }
            _ => {}
        }

        actions
    }

    // -----------------------------------------------------------------------
    // Agents
    //
    // Source layout: agents/<subfolder>/<id>/
    //   github → <id>/agent.md  → repo: <repo>/.github/agents/<id>.md
    //   claude → <id>/agent.md  → repo: <repo>/.claude/agents/<id>.md
    //   kiro   → <id>/agent.json → home: ~/.kiro/agents/<id>.json
    //                              repo: <repo>/.kiro/agents/<id>.json
    //   common → deploys to all three above
    //
    // GitHub Copilot agents: markdown with YAML frontmatter, repo-scoped only.
    // Claude Code agents: markdown with YAML frontmatter, repo-scoped only.
    // Kiro agents: JSON config file, home or repo.
    // -----------------------------------------------------------------------

    fn resolve_agent(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        let id = &comp.id;
        let src = &comp.source_path;
        let subfolder = self.detect_subfolder(src, "agents");

        match subfolder.as_deref() {
            Some("github") => {
                // GitHub Copilot: same markdown format for VS Code IDE, CLI, and GitHub.com
                // Home scope → ~/.copilot/agents/<id>.md  (Copilot CLI user-level)
                // Repo scope → <repo>/.github/agents/<id>.md
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id,
                        &src.join("agent.md"),
                        self.home_dir.join(".copilot").join("agents").join(format!("{id}.md"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id,
                            &src.join("agent.md"),
                            repo.join(".github").join("agents").join(format!("{id}.md"))));
                    }
                }
            }
            Some("claude") => {
                // Claude Code: markdown file, repo-scoped only
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id,
                            &src.join("agent.md"),
                            repo.join(".claude").join("agents").join(format!("{id}.md"))));
                    }
                }
            }
            Some("cursor") => {
                // Cursor: same markdown format for IDE and CLI
                // Home scope → ~/.cursor/agents/<id>.md  (user-level)
                // Repo scope → <repo>/.cursor/agents/<id>.md
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    actions.push(self.copy_file_action(id,
                        &src.join("agent.md"),
                        self.home_dir.join(".cursor").join("agents").join(format!("{id}.md"))));
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        actions.push(self.copy_file_action(id,
                            &src.join("agent.md"),
                            repo.join(".cursor").join("agents").join(format!("{id}.md"))));
                    }
                }
            }
            Some("kiro") => {
                // Kiro CLI: agent.json  → .kiro/agents/<id>.json
                // Kiro IDE: agent.md   → .kiro/agents/<id>.md
                // Install whichever files exist in the registry folder.
                let json_src = src.join("agent.json");
                let md_src   = src.join("agent.md");
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    if json_src.exists() {
                        actions.push(self.copy_file_action(id, &json_src,
                            self.home_dir.join(".kiro").join("agents").join(format!("{id}.json"))));
                    }
                    if md_src.exists() {
                        actions.push(self.copy_file_action(id, &md_src,
                            self.home_dir.join(".kiro").join("agents").join(format!("{id}.md"))));
                    }
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        if json_src.exists() {
                            actions.push(self.copy_file_action(id, &json_src,
                                repo.join(".kiro").join("agents").join(format!("{id}.json"))));
                        }
                        if md_src.exists() {
                            actions.push(self.copy_file_action(id, &md_src,
                                repo.join(".kiro").join("agents").join(format!("{id}.md"))));
                        }
                    }
                }
            }
            _ => {
                // common / no subfolder → deploy to all tools
                if self.scope == InstallScope::Home || self.scope == InstallScope::Both {
                    if src.join("agent.md").exists() {
                        // GitHub Copilot CLI user-level
                        actions.push(self.copy_file_action(id,
                            &src.join("agent.md"),
                            self.home_dir.join(".copilot").join("agents").join(format!("{id}.md"))));
                        // Cursor user-level
                        actions.push(self.copy_file_action(id,
                            &src.join("agent.md"),
                            self.home_dir.join(".cursor").join("agents").join(format!("{id}.md"))));
                        // Kiro IDE
                        actions.push(self.copy_file_action(id,
                            &src.join("agent.md"),
                            self.home_dir.join(".kiro").join("agents").join(format!("{id}.md"))));
                    }
                    if src.join("agent.json").exists() {
                        // Kiro CLI
                        actions.push(self.copy_file_action(id,
                            &src.join("agent.json"),
                            self.home_dir.join(".kiro").join("agents").join(format!("{id}.json"))));
                    }
                }
                if self.scope == InstallScope::Repo || self.scope == InstallScope::Both {
                    if let Some(repo) = &self.repo_path {
                        if src.join("agent.md").exists() {
                            // GitHub Copilot repo-level
                            actions.push(self.copy_file_action(id,
                                &src.join("agent.md"),
                                repo.join(".github").join("agents").join(format!("{id}.md"))));
                            // Claude Code
                            actions.push(self.copy_file_action(id,
                                &src.join("agent.md"),
                                repo.join(".claude").join("agents").join(format!("{id}.md"))));
                            // Cursor
                            actions.push(self.copy_file_action(id,
                                &src.join("agent.md"),
                                repo.join(".cursor").join("agents").join(format!("{id}.md"))));
                            // Kiro IDE
                            actions.push(self.copy_file_action(id,
                                &src.join("agent.md"),
                                repo.join(".kiro").join("agents").join(format!("{id}.md"))));
                        }
                        if src.join("agent.json").exists() {
                            // Kiro CLI
                            actions.push(self.copy_file_action(id,
                                &src.join("agent.json"),
                                repo.join(".kiro").join("agents").join(format!("{id}.json"))));
                        }
                    }
                }
            }
        }

        actions
    }

    // -----------------------------------------------------------------------
    // Packages — always global, always home-scoped
    //
    // Source layout: packages/<tool>/<id>/
    //   claude → ~/.claude/plugins/<id>/   (Claude plugin bundle)
    //
    // Note: Kiro uses Powers (not packages) for bundled multi-file installs.
    // -----------------------------------------------------------------------

    fn resolve_package(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let subfolder = self.detect_subfolder(&comp.source_path, "packages");
        let dest = match subfolder.as_deref() {
            Some("claude") => self.home_dir.join(".claude").join("plugins").join(&comp.id),
            _ => return vec![],
        };
        vec![InstallAction {
            component_id: comp.id.clone(),
            op: InstallOp::CopyDir { src: comp.source_path.clone(), dest },
        }]
    }

    // -----------------------------------------------------------------------
    // OlafData — repo-scoped only
    //
    // source_path = <registry>/.olaf  (the whole .olaf folder)
    //
    // Scans .olaf/data/{product,practices,peoples,projects,kb}/:
    //   - Skip if folder is absent, empty, or contains only .gitkeep
    //   - Otherwise: CopyDir → <repo>/.olaf/data/<subfolder>/
    //
    // Always emits MergeGitignore for ".olaf/work/" in the target repo.
    // -----------------------------------------------------------------------

    fn resolve_olaf_data(&self, comp: &ResolvedComponent) -> Vec<InstallAction> {
        let mut actions = Vec::new();
        let repo = match &self.repo_path {
            Some(r) => r,
            None => return actions, // repo-scoped only
        };

        let olaf_root = &comp.source_path; // <registry>/.olaf
        let data_root = olaf_root.join("data");

        for subfolder in &["product", "practices", "peoples", "projects", "kb"] {
            let src = data_root.join(subfolder);
            if !src.is_dir() {
                continue;
            }
            // Skip if only .gitkeep (or empty)
            if self.is_empty_or_gitkeep_only(&src) {
                continue;
            }
            actions.push(InstallAction {
                component_id: comp.id.clone(),
                op: InstallOp::CopyDir {
                    src: src.clone(),
                    dest: repo.join(".olaf").join("data").join(subfolder),
                },
            });
        }

        // Always ensure .olaf/work/ is gitignored in the target repo
        actions.push(InstallAction {
            component_id: comp.id.clone(),
            op: InstallOp::MergeGitignore {
                entry: ".olaf/work/".to_string(),
                dest: repo.join(".gitignore"),
            },
        });

        actions
    }

    /// Returns true if a directory is empty or contains only `.gitkeep` files.
    fn is_empty_or_gitkeep_only(&self, dir: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else { return true; };
        let files: Vec<_> = entries.flatten().collect();
        if files.is_empty() {
            return true;
        }
        files.iter().all(|e| e.file_name() == ".gitkeep")
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

    /// Detects two subfolder levels after `parent`.
    /// e.g. `rules/global/kiro/file.md` with parent="rules" → ("global", "kiro")
    fn detect_two_subfolders(&self, src: &Path, parent: &str) -> (Option<String>, Option<String>) {
        let components: Vec<_> = src.components().collect();
        for (i, c) in components.iter().enumerate() {
            if c.as_os_str() == parent {
                let first  = components.get(i + 1).map(|c| c.as_os_str().to_string_lossy().to_string());
                let second = components.get(i + 2).map(|c| c.as_os_str().to_string_lossy().to_string());
                return (first, second);
            }
        }
        (None, None)
    }

    /// Resolves the actual on-disk path for a component that may live under a subfolder.
    ///
    /// The frontend builds `source_path` as `<registry>/<type_dir>/<id>` (no subfolder).
    /// But the registry may store it as `<registry>/<type_dir>/<subfolder>/<id>/`.
    /// This helper returns the real path, scanning subfolders if the direct path doesn't exist.
    fn resolve_component_path(&self, src: &Path) -> PathBuf {
        if src.exists() {
            return src.to_path_buf();
        }
        // Try scanning one level of subfolders in the parent directory
        if let Some(parent) = src.parent() {
            if let Some(id) = src.file_name() {
                if let Ok(entries) = std::fs::read_dir(parent) {
                    for entry in entries.flatten() {
                        let candidate = entry.path().join(id);
                        if candidate.exists() {
                            return candidate;
                        }
                    }
                }
            }
        }
        src.to_path_buf() // fall back to original even if not found
    }
}
