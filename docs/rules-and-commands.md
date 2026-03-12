# Rules and Commands — Tool Compatibility

Rules and commands are the most complex component types to install because every AI coding tool uses a different file format, location, and frontmatter convention. Registry authors must author one file per target tool — the installer does a plain copy with no frontmatter injection.

## The problem

A "rule" is a persistent instruction that an AI tool loads into context automatically. A "command" is a reusable prompt invoked manually with a `/` slash command. Both concepts exist in all major tools — but the implementation details differ significantly:

| Tool | Rules file | Rules frontmatter | Commands file | Commands frontmatter |
|---|---|---|---|---|
| Kiro | `.kiro/steering/<id>.md` | `inclusion: always \| fileMatch \| manual` | `.kiro/steering/<id>.md` | `inclusion: manual` (steering = command) |
| Cursor | `.cursor/rules/<id>.mdc` | `description`, `globs`, `alwaysApply` | `.cursor/commands/<id>.md` | none |
| GitHub Copilot | `.github/instructions/<id>.instructions.md` (repo) / appended to `.copilot/copilot-instructions.md` (home) | `applyTo: "glob"` (repo only) | `.github/prompts/<id>.prompt.md` | `mode: "agent"`, `description` |
| Windsurf | `.windsurf/rules/<id>.md` or appended to `global_rules.md` | none | `.windsurf/workflows/<id>.md` | none |
| Claude Code | `.claude/rules/<filename>.md` (repo) / `~/.claude/rules/<filename>.md` (global) | none | `.claude/commands/<id>.md` | none |

Key observations:
- Kiro has no native command system — a steering file with `inclusion: manual` becomes a slash command
- Copilot rules use `.instructions.md` extension and `applyTo` frontmatter; commands use `.prompt.md` extension
- Cursor rules use `.mdc` extension with YAML frontmatter; commands are plain `.md`
- Windsurf calls commands "workflows" and stores them in `.windsurf/workflows/`
- Claude Code rules are appended to a shared file, not stored as individual files

## The subfolder model

Registry authors place rules and commands in tool-specific subfolders. The subfolder name tells the installer which tool the file targets. There is no `common/` subfolder — each tool variant must be authored explicitly with the correct frontmatter already in the file.

```
rules/
  global/
    kiro/       ← Kiro only, frontmatter already in file
    cursor/     ← Cursor only, frontmatter already in file
    copilot/    ← Copilot only, frontmatter already in file
    windsurf/   ← Windsurf only, no frontmatter needed
    claude/     ← Claude Code only, no frontmatter needed
  repo/
    kiro/       ← Kiro only
    cursor/     ← Cursor only
    copilot/    ← Copilot only (.instructions.md extension mandatory)
    windsurf/   ← Windsurf only
    claude/     ← Claude Code only
    agents/     ← cross-tool AGENTS.md (plain markdown, no frontmatter, appended to repo root AGENTS.md)
```

```
commands/
  global/
    kiro/       ← Kiro only (steering with inclusion: manual)
    claude/     ← Claude Code only (invoked as /user:name)
    cursor/     ← Cursor only
    windsurf/   ← Windsurf only (goes to ~/.codeium/windsurf/global_workflows/)
  repo/
    kiro/       ← Kiro only
    claude/     ← Claude Code only (invoked as /name)
    cursor/     ← Cursor only
    copilot/    ← Copilot only (.prompt.md extension mandatory)
    windsurf/   ← Windsurf only (goes to .windsurf/workflows/)
```

Unknown or missing subfolders are silently skipped.

## Authoring rules

Each tool-specific file must include the correct frontmatter. Examples:

**Kiro** (`rules/kiro/<id>/rule.md`) — `inclusion: always`:
```markdown
---
inclusion: always
---
# My Rule
...
```

**Cursor** (`rules/cursor/<id>/rule.md`) — `.mdc` extension at destination:
```markdown
---
description: ""
globs: ""
alwaysApply: true
---
# My Rule
...
```

**Copilot** (`rules/copilot/<id>.instructions.md`) — `applyTo` frontmatter, `.instructions.md` extension mandatory at destination:
```markdown
---
applyTo: "**"
---
# My Rule
...
```
Global install: appended to `~/.copilot/copilot-instructions.md` (single shared file, no frontmatter needed there).

**Windsurf** (`rules/windsurf/<id>/rule.md`) — no frontmatter; appended to `global_rules.md` (home) or copied to `.windsurf/rules/<id>.md` (repo).

**Claude Code** (`rules/claude/<id>/rule.md`) — no frontmatter; appended to `~/.claude/CLAUDE.md` (home) or `AGENTS.md` (repo).

## AGENTS.md — cross-tool rules

`AGENTS.md` is a cross-tool convention read by Claude Code, Windsurf, Copilot CLI, and others. It is location-scoped: a file at the repo root applies to the whole project; a file in a subdirectory applies only to that directory.

The installer can only place `AGENTS.md` at the repo root (it has no knowledge of the target repo structure). Registry authors who want cross-tool coverage place their content under `rules/repo/agents/` — plain markdown, no frontmatter. Multiple files are appended in order.

Claude-specific rules in `rules/repo/claude/` are NOT automatically copied to `AGENTS.md`. If you want something in `AGENTS.md`, author it explicitly in `rules/repo/agents/`.

## Authoring commands

**Kiro** (`commands/kiro/<id>/command.md`) — steering file with `inclusion: manual`:
```markdown
---
inclusion: manual
---
# My Command
...
```

**Copilot** (`commands/copilot/<id>/command.md`) — `.prompt.md` extension at destination:
```markdown
---
mode: "agent"
description: ""
---
# My Command
...
```

**Cursor** (`commands/cursor/<id>/command.md`) — plain `.md`, no frontmatter.

**Windsurf** (`commands/windsurf/<id>/command.md`) — plain `.md`, goes to `.windsurf/workflows/<id>.md`.

**Claude Code** (`commands/claude/<id>/command.md`) — plain `.md`, goes to `.claude/commands/<id>.md`.

## Kiro commands are steering files

Kiro does not have a native custom command system. Steering files with `inclusion: manual` appear in the `/` slash command menu in Kiro chat. When the user types `/my-command`, Kiro injects the steering file's content into the conversation context.

A command in `commands/kiro/` is stored in `.kiro/steering/` at install time. The file must have `inclusion: manual` frontmatter written by the registry author.

## Scope: home vs repo

| Type | Home install | Repo install |
|---|---|---|
| Rules (Kiro) | `~/.kiro/steering/` | `<repo>/.kiro/steering/` |
| Rules (Cursor) | `~/.cursor/rules/` | `<repo>/.cursor/rules/` |
| Rules (Copilot) | appended to `~/.copilot/copilot-instructions.md` | `<repo>/.github/instructions/` (`.instructions.md` extension mandatory) |
| Rules (Windsurf) | append to `global_rules.md` | `<repo>/.windsurf/rules/` |
| Rules (Claude) | `~/.claude/rules/` | `<repo>/.claude/rules/` |
| Rules (AGENTS.md) | — | appended to `<repo>/AGENTS.md` |
| Commands (Kiro) | `~/.kiro/steering/` | `<repo>/.kiro/steering/` |
| Commands (Copilot) | — | `<repo>/.github/prompts/` |
| Commands (Cursor) | `~/.cursor/commands/` | `<repo>/.cursor/commands/` |
| Commands (Windsurf) | `~/.codeium/windsurf/global_workflows/` | `<repo>/.windsurf/workflows/` |
| Commands (Claude) | `~/.claude/commands/` | `<repo>/.claude/commands/` |

Copilot commands are always repo-scoped — there is no portable global path for Copilot commands. Copilot global rules are written to `~/.copilot/copilot-instructions.md`.
