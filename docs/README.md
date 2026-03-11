# haal-installer — Documentation

The haal-installer is a cross-platform desktop application (Tauri + Svelte) that installs AI coding skills, rules, commands, MCP servers, and agentic systems from Git-based registries into the user's AI coding tools (Kiro, Cursor, Claude Code, Windsurf, GitHub Copilot).

## Documents

| Document | What it covers |
|---|---|
| [architecture.md](architecture.md) | Technology stack (Tauri, Svelte, Rust), project structure, data flow |
| [artifact-types.md](artifact-types.md) | All installable artifact types, which tools support them, and where they land |
| [registry-structure.md](registry-structure.md) | The layout of a registry repo — manifest, competency files, component folders |
| [multi-registry.md](multi-registry.md) | Seed vs secondary registries, merge rules, enterprise governance model |
| [rules-and-commands.md](rules-and-commands.md) | Tool-specific frontmatter and file placement for rules and commands |

## Quick concepts

**Registry** — a Git repository containing a `haal_manifest.json` and component folders. The installer clones it and reads it to build the catalog.

**Seed registry** — the primary registry the user connects to. It has the highest priority in the merged catalog and can declare secondary registries via `repos-manifest.json`.

**Collection** — a named group of competencies. Users select collections or individual competencies in the Choose step.

**Competency** — a named set of components (skills, rules, commands, MCP servers, etc.) that belong together. Defined in a JSON file in the `competencies/` folder.

**Component** — an individual installable unit: a skill, power, hook, rule, command, agent, MCP server, or agentic system.

**Agentic system** — a standalone Git repository (e.g. a Python CLI tool) that the installer clones into `~/.haal/systems/<id>/`. Referenced from competency files just like any other component type. The registry manifest's `systems` array provides the repo URL and metadata for each system ID.

## Component types

| Type | Description | Install location |
|---|---|---|
| Skill | Reusable AI workflow (markdown prompt + assets) | `~/.kiro/skills/`, `~/.agents/skills/`, etc. |
| Power | Always-active context package (Kiro only) | `~/.kiro/powers/installed/` |
| Hook | Event-triggered agent action (Kiro only) | `<repo>/.kiro/hooks/` |
| Rule | Persistent instruction loaded into AI context | Tool-specific — see [rules-and-commands.md](rules-and-commands.md) |
| Command | Reusable prompt invoked with `/` slash command | Tool-specific — see [rules-and-commands.md](rules-and-commands.md) |
| Agent | Custom AI agent definition | `<repo>/.github/agents/`, `.kiro/agents/`, `.claude/agents/` |
| MCP Server | Model Context Protocol server definition | Merged into tool config files |
| System | Standalone agentic tool (full Git repo) | `~/.haal/systems/<id>/` |
