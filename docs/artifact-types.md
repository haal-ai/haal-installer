# Artifact Types and Tool Compatibility

The installer supports nine types of AI artifacts. Each type maps to a different concept in the AI tooling ecosystem, and each tool supports a different subset of them.

## Overview

| Type | What it is | Kiro | Cursor | Copilot | Windsurf | Claude Code |
|---|---|:---:|:---:|:---:|:---:|:---:|
| Skill | Reusable agent capability (prompt + instructions folder) | ✓ | ✓ | ✓ | ✓ | — |
| Power | Kiro-specific extension (MCP server + docs bundle) | ✓ | — | — | — | — |
| Rule | Persistent instruction always loaded into AI context | ✓ | ✓ | ✓ | ✓ | ✓ |
| Command | Reusable prompt invoked via slash command | ✓ | ✓ | ✓ | ✓ | ✓ |
| Hook | Event-driven automation (file save, prompt submit, etc.) | ✓ | — | — | — | — |
| Agent | Standalone agent definition | ✓ | — | ✓ | — | ✓ |
| MCP Server | Model Context Protocol server configuration | ✓ | ✓ | — | ✓ | ✓ |
| System | Full agentic application cloned from its own repo | — | — | — | — | — |
| OLAF Data | Shared knowledge base data (people, projects, practices) | — | — | — | — | — |

Systems and OLAF Data are tool-agnostic — they install to `~/.haal/systems/` and `~/.olaf/data/` respectively, independent of any specific AI tool.

---

## Skills

A skill is a folder containing a `skill.md` file (the agent instructions) and optional supporting files. Skills are the primary unit of reusable agent capability — they teach the AI how to perform a specific task.

Install locations:

| Scope | Kiro | Other tools |
|---|---|---|
| Home | `~/.kiro/skills/<id>/` | `~/.agents/skills/<id>/` |
| Repo | `<repo>/.kiro/skills/<id>/` | `<repo>/.agents/skills/<id>/` |

Copilot and Windsurf read skills from `.agents/skills/` in the repo. Claude Code does not have a native skills concept.

---

## Powers

Powers are Kiro-specific. A power bundles an MCP server with its documentation and optional steering guides into a single installable unit. They appear in Kiro's Powers panel.

Install location: `~/.kiro/powers/installed/<id>/` (home only, always global).

No other tool has an equivalent concept.

---

## Rules

Rules are persistent instructions that an AI tool loads automatically into every conversation context. They encode team standards, coding conventions, security baselines, and similar always-on guidance.

Every tool supports rules but uses a different file format, location, and frontmatter. The installer normalises this — see [rules-and-commands.md](rules-and-commands.md) for the full details.

Summary of install locations:

| Tool | Home | Repo |
|---|---|---|
| Kiro | `~/.kiro/steering/<id>.md` | `<repo>/.kiro/steering/<id>.md` |
| Cursor | `~/.cursor/rules/<id>.mdc` | `<repo>/.cursor/rules/<id>.mdc` |
| Copilot | — | `<repo>/.github/instructions/<id>.instructions.md` |
| Windsurf | appended to `~/.codeium/windsurf/global_rules.md` | `<repo>/.windsurf/rules/<id>.md` |
| Claude Code | appended to `~/.claude/CLAUDE.md` | appended to `<repo>/AGENTS.md` |

Copilot rules are always repo-scoped. Windsurf and Claude Code global rules are appended to a shared file rather than stored individually.

---

## Commands

Commands are reusable prompts invoked manually via a `/` slash command in the AI chat. They encode repeatable workflows — code review, PR description, test generation, etc.

Like rules, every tool has a different convention. The installer handles the mapping automatically.

Summary of install locations:

| Tool | Home | Repo |
|---|---|---|
| Kiro | `~/.kiro/steering/<id>.md` (inclusion: manual) | `<repo>/.kiro/steering/<id>.md` |
| Cursor | `~/.cursor/commands/<id>.md` | `<repo>/.cursor/commands/<id>.md` |
| Copilot | — | `<repo>/.github/prompts/<id>.prompt.md` |
| Windsurf | — | `<repo>/.windsurf/workflows/<id>.md` |
| Claude Code | `~/.claude/commands/<id>.md` | `<repo>/.claude/commands/<id>.md` |

Note: Kiro has no native command system. A steering file with `inclusion: manual` frontmatter appears in the `/` slash command menu. Windsurf calls commands "workflows". Copilot commands are always repo-scoped.

See [rules-and-commands.md](rules-and-commands.md) for the subfolder model and frontmatter injection details.

---

## Hooks

Hooks are event-driven automations. They trigger an agent action or shell command when an IDE event occurs — a file is saved, a prompt is submitted, a tool is about to run, etc.

Currently Kiro-specific. Hooks are stored as `.kiro.hook` JSON files.

Install location (repo-scoped only): `<repo>/.kiro/hooks/<id>.kiro.hook`

Hooks are always repo-scoped — they encode project-specific automation, not global user preferences.

---

## Agents

Agent definitions describe a standalone AI agent with its own persona, instructions, and tool access. The concept exists in Kiro, GitHub Copilot (via `.github/agents/`), and Claude Code (via `.claude/agents/`).

Install locations (repo-scoped only):

| Subfolder in registry | Destination |
|---|---|
| `agents/kiro/` | `<repo>/.kiro/agents/<id>/` |
| `agents/github/` | `<repo>/.github/agents/<id>/` |
| `agents/claude/` | `<repo>/.claude/agents/<id>/` |
| `agents/common/` | all three above |

---

## MCP Servers

MCP (Model Context Protocol) servers extend the AI tool with additional capabilities — database access, API integrations, custom tools. The installer merges the server definition into each tool's MCP config file.

Each MCP server in the registry has an `mcp.json` file defining its transport (HTTP or stdio), command, args, and environment variables.

Install targets:

| Tool | Home config | Repo config |
|---|---|---|
| Kiro | `~/.kiro/settings/mcp.json` | `<repo>/.kiro/settings/mcp.json` |
| Cursor | `~/.cursor/mcp.json` | `<repo>/.cursor/mcp.json` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | — |
| Claude Code | `~/.claude/settings.json` | `<repo>/.claude/settings.json` |
| VS Code | — | `<repo>/.vscode/mcp.json` (key: `servers`) |

The installer merges the server entry into the existing JSON — it does not overwrite the whole file.

---

## Systems

A system is a full agentic application that lives in its own Git repository. Unlike other artifact types, a system is not a configuration file — it is a standalone tool that the user runs independently of their IDE.

Examples: a reporting agent, a code analysis pipeline, a CI automation tool.

The registry declares a system by pointing to its GitHub repo. The installer clones it to `~/.haal/systems/<id>/` and runs any post-install steps defined in the system's `system.json`.

Systems are tool-agnostic — they are not tied to any specific AI IDE.

---

## OLAF Data

OLAF Data is shared knowledge base content — people records, project metadata, practices, decision records. It is used by OLAF-based skills and agents to provide context about the organisation.

Install location: `~/.olaf/data/` (home only, never overwrites existing files).

OLAF Data is tool-agnostic. It is consumed by skills and agents at runtime, not by the AI tool directly.
