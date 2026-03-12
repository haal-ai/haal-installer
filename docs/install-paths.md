# Install Path Mappings

This document is the authoritative reference for where each artifact type is installed. For every artifact type, paths are listed by tool with both the global (home) destination and the repo destination clearly separated.

Reflects the logic in `src-tauri/src/destination_resolver.rs`.

---

## Skills

Source layout: `skills/<id>/` (directory)

| Tool | Global (home) | Repo |
|---|---|---|
| Kiro | `~/.kiro/skills/<id>/` | `<repo>/.kiro/skills/<id>/` |
| GitHub Copilot | `~/.github/skills/<id>/` | `<repo>/.agents/skills/<id>/` |
| Claude Code | `~/.claude/skills/<id>/` | `<repo>/.claude/skills/<id>/` |
| Cursor | `~/.agents/skills/<id>/` | `<repo>/.agents/skills/<id>/` |
| Windsurf | `~/.agents/skills/<id>/` | `<repo>/.agents/skills/<id>/` |

Notes:
- Global and repo paths are only written for tools that are actually selected during install.
- `.agents/skills/` is the [agentskills.io](https://agentskills.io) standard shared by Windsurf, Cursor, and Copilot.
- Cursor and Windsurf share the same global path — deduplication is applied automatically.

---

## Powers

Source layout: `powers/<id>/` (directory). Kiro-specific only.

| Tool | Global (home) | Repo |
|---|---|---|
| Kiro | `~/.kiro/powers/installed/<id>/` | — |

Powers are always global. No other tool has an equivalent concept.

---

## Rules

Rules are persistent instructions loaded automatically into every AI context. Each tool variant is a plain markdown file placed in the appropriate registry subfolder with the correct frontmatter already baked in — the installer does a plain copy, no injection.

Registry layout: `rules/<scope>/<tool>/filename.md`
- `<scope>` is either `global` or `repo`
- `<tool>` is `kiro`, `cursor`, `copilot`, `windsurf`, or `claude`
- The filename can be anything — it is preserved at the destination (except where the tool mandates a fixed filename)

| Registry path | Tool | Destination |
|---|---|---|
| `rules/global/kiro/` | Kiro | `~/.kiro/steering/<filename>.md` |
| `rules/global/cursor/` | Cursor | `~/.cursor/rules/<filename>.mdc` |
| `rules/global/copilot/` | GitHub Copilot | `~/.copilot/copilot-instructions.md` (fixed filename) |
| `rules/global/windsurf/` | Windsurf | `~/.codeium/windsurf/global_rules.md` (fixed filename) |
| `rules/global/claude/` | Claude Code | `~/.claude/rules/<filename>.md` |
| `rules/repo/kiro/` | Kiro | `<repo>/.kiro/steering/<filename>.md` |
| `rules/repo/cursor/` | Cursor | `<repo>/.cursor/rules/<filename>.mdc` |
| `rules/repo/copilot/` | GitHub Copilot | `<repo>/.github/instructions/<filename>.instructions.md` |
| `rules/repo/windsurf/` | Windsurf | `<repo>/.windsurf/rules/<filename>.md` |
| `rules/repo/claude/` | Claude Code | `<repo>/.claude/rules/<filename>.md` |
| `rules/repo/agents/` | cross-tool | appended to `<repo>/AGENTS.md` |

Notes:
- Claude Code supports multiple rule files via `~/.claude/rules/` (global) and `.claude/rules/` (repo) — since v2.0.64.
- `rules/repo/agents/` is for explicitly cross-tool instructions — plain markdown, no frontmatter. Multiple files are appended in order. Registry authors opt in deliberately; Claude rules are NOT auto-copied there.
- AGENTS.md is location-scoped: a root-level file applies to the whole project. Subdirectory scoping is not possible from a registry install since the target repo structure is unknown.
- Copilot and Windsurf each have a single fixed global filename — only one global rule file per tool.
- Copilot repo rules must end with `.instructions.md` — the installer enforces this automatically.
- Unknown scope or tool subfolders are silently skipped.

---

## Commands

Commands are reusable prompts invoked via `/` slash command. Each tool variant is a plain markdown file placed in the appropriate registry subfolder with the correct frontmatter already baked in. There is no `common/` subfolder. Kiro has no native command system — a steering file with `inclusion: manual` serves as a slash command. Windsurf calls commands "workflows".

Registry layout: `commands/<scope>/<tool>/filename.md`

| Registry path | Tool | Destination |
|---|---|---|
| `commands/global/kiro/` | Kiro | `~/.kiro/steering/<filename>.md` |
| `commands/global/claude/` | Claude Code | `~/.claude/commands/<filename>.md` (invoked as `/user:name`) |
| `commands/global/cursor/` | Cursor | `~/.cursor/commands/<filename>.md` |
| `commands/global/copilot/` | GitHub Copilot | — no portable global path |
| `commands/global/windsurf/` | Windsurf | `~/.codeium/windsurf/global_workflows/<filename>.md` |
| `commands/repo/kiro/` | Kiro | `<repo>/.kiro/steering/<filename>.md` |
| `commands/repo/claude/` | Claude Code | `<repo>/.claude/commands/<filename>.md` (invoked as `/name`) |
| `commands/repo/cursor/` | Cursor | `<repo>/.cursor/commands/<filename>.md` |
| `commands/repo/copilot/` | GitHub Copilot | `<repo>/.github/prompts/<filename>.prompt.md` |
| `commands/repo/windsurf/` | Windsurf | `<repo>/.windsurf/workflows/<filename>.md` |

Notes:
- Copilot prompt files must end with `.prompt.md` — the installer enforces this automatically.
- Copilot has no portable global/home path for commands (VS Code stores them inside the profile directory which varies per machine).
- Windsurf global workflows live in `~/.codeium/windsurf/global_workflows/` — available in every workspace on the machine, not committed to git.
- Kiro commands are steering files with `inclusion: manual` frontmatter — they appear in the `/` slash command menu.
- Unknown scope or tool subfolders are silently skipped.

---

## Hooks

Source layout: `hooks/<subfolder>/<id>/hook.json`. Always repo-scoped — hooks encode project-specific automation.

| Registry subfolder | Tool | Global (home) | Repo |
|---|---|---|---|
| `hooks/kiro/` | Kiro | — | `<repo>/.kiro/hooks/<id>.kiro.hook` |
| `hooks/copilot/` | GitHub Copilot | — | `<repo>/.github/hooks/<id>.json` |

Notes:
- Cursor and Windsurf do not have a native hook system.

---

## Agents

Source layout: `agents/<subfolder>/<id>/agent.md` and/or `agent.json` (Kiro CLI format).

| Registry subfolder | Tool | File | Global (home) | Repo |
|---|---|---|---|---|
| `agents/kiro/` | Kiro (CLI) | `agent.json` | `~/.kiro/agents/<id>.json` | `<repo>/.kiro/agents/<id>.json` |
| `agents/kiro/` | Kiro (IDE) | `agent.md` | `~/.kiro/agents/<id>.md` | `<repo>/.kiro/agents/<id>.md` |
| `agents/github/` | GitHub Copilot | `agent.md` | `~/.copilot/agents/<id>.md` | `<repo>/.github/agents/<id>.md` |
| `agents/claude/` | Claude Code | `agent.md` | — | `<repo>/.claude/agents/<id>.md` |
| `agents/cursor/` | Cursor | `agent.md` | `~/.cursor/agents/<id>.md` | `<repo>/.cursor/agents/<id>.md` |
| `agents/common/` | all of the above | both | all global paths above | all repo paths above |

Notes:
- Kiro installs whichever of `agent.json` / `agent.md` are present in the registry folder.
- Claude Code agents are always repo-scoped.
- GitHub Copilot global path (`~/.copilot/agents/`) is the CLI user-level location.

---

## MCP Servers

Source layout: `mcpservers/<id>/mcp.json`. The server entry is merged into the tool's existing JSON config — the whole file is never overwritten.

| Tool | Global (home) config | Repo config |
|---|---|---|
| Kiro | `~/.kiro/settings/mcp.json` (`mcpServers`) | `<repo>/.kiro/settings/mcp.json` (`mcpServers`) |
| Cursor | `~/.cursor/mcp.json` (`mcpServers`) | `<repo>/.cursor/mcp.json` (`mcpServers`) |
| Claude Code | `~/.claude/settings.json` (`mcpServers`) | `<repo>/.claude/settings.json` (`mcpServers`) |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` (`mcpServers`) | — |
| VS Code / Copilot | `<os-config>/Code/User/mcp.json` (`servers`) | `<repo>/.vscode/mcp.json` (`servers`) |

Notes:
- Windsurf has no repo-level MCP config.
- VS Code uses `servers` as the JSON key instead of `mcpServers`.
- Global installs are only written for the tools that are actually selected.

---

## Packages

Source layout: `packages/<subfolder>/<id>/` (directory). Always global, never repo-scoped.

| Registry subfolder | Tool | Global (home) | Repo |
|---|---|---|---|
| `packages/claude/` | Claude Code | `~/.claude/plugins/<id>/` | — |

Notes:
- Kiro uses Powers (not packages) for bundled multi-file installs.

---

## Systems

Source layout: registry entry points to a GitHub repo URL. Tool-agnostic.

| Destination | Scope |
|---|---|
| `~/.haal/systems/<id>/` | Global only |

The installer clones the repo and runs any post-install steps defined in the system's `system.json`.

---

## OLAF Data

Source layout: `<registry>/.olaf/data/<subfolder>/`. Always repo-scoped, tool-agnostic.

| Source subfolder | Global (home) | Repo |
|---|---|---|
| `.olaf/data/product/` | — | `<repo>/.olaf/data/product/` |
| `.olaf/data/practices/` | — | `<repo>/.olaf/data/practices/` |
| `.olaf/data/peoples/` | — | `<repo>/.olaf/data/peoples/` |
| `.olaf/data/projects/` | — | `<repo>/.olaf/data/projects/` |
| `.olaf/data/kb/` | — | `<repo>/.olaf/data/kb/` |

Notes:
- Empty folders and folders containing only `.gitkeep` are skipped.
- The installer always ensures `.olaf/work/` is added to the repo's `.gitignore`.
