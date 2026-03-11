# Rules and Commands — Tool Compatibility

Rules and commands are the most complex component types to install because every AI coding tool uses a different file format, location, and frontmatter convention. The installer handles this automatically, but registry authors need to understand the model to structure their content correctly.

## The problem

A "rule" is a persistent instruction that an AI tool loads into context automatically. A "command" is a reusable prompt invoked manually with a `/` slash command. Both concepts exist in all major tools — but the implementation details differ significantly:

| Tool | Rules file | Rules frontmatter | Commands file | Commands frontmatter |
|---|---|---|---|---|
| Kiro | `.kiro/steering/<id>.md` | `inclusion: always \| fileMatch \| manual` | `.kiro/steering/<id>.md` | `inclusion: manual` (steering = command) |
| Cursor | `.cursor/rules/<id>.mdc` | `description`, `globs`, `alwaysApply` | `.cursor/commands/<id>.md` | none |
| GitHub Copilot | `.github/instructions/<id>.instructions.md` | `applyTo: "glob"` | `.github/prompts/<id>.prompt.md` | `mode: "agent"`, `description` |
| Windsurf | `.windsurf/rules/<id>.md` or appended to `global_rules.md` | none | `.windsurf/workflows/<id>.md` | none |
| Claude Code | appended to `CLAUDE.md` or `AGENTS.md` | none | `.claude/commands/<id>.md` | none |

Key observations:
- Kiro has no native command system — a steering file with `inclusion: manual` becomes a slash command
- Copilot rules use `.instructions.md` extension and `applyTo` frontmatter; commands use `.prompt.md` extension
- Cursor rules use `.mdc` extension with YAML frontmatter; commands are plain `.md`
- Windsurf calls commands "workflows" and stores them in `.windsurf/workflows/`
- Claude Code rules are appended to a shared file, not stored as individual files

## The subfolder model

Registry authors place rules and commands in typed subfolders. The subfolder name tells the installer which tool the file targets:

```
rules/
  common/     ← deployed to ALL tools, installer injects frontmatter
  kiro/       ← Kiro only, frontmatter already in file
  cursor/     ← Cursor only, frontmatter already in file
  copilot/    ← Copilot only, frontmatter already in file
  windsurf/   ← Windsurf only, no frontmatter needed
  claude/     ← Claude Code only, no frontmatter needed

commands/
  common/     ← deployed to ALL tools, installer injects frontmatter
  kiro/       ← Kiro only (steering with inclusion: manual)
  copilot/    ← Copilot only (.prompt.md with mode/description)
  cursor/     ← Cursor only
  windsurf/   ← Windsurf only (goes to .windsurf/workflows/)
  claude/     ← Claude Code only
```

## Override precedence

If the same component ID exists in both `common/` and a tool-specific subfolder, **the tool-specific version wins for that tool**. All other tools fall back to `common/`.

Example: a rule `security-baseline` exists in both `rules/common/` and `rules/cursor/`.
- Cursor gets `rules/cursor/security-baseline/rule.md` — the author controls the exact frontmatter (e.g. a specific glob pattern)
- All other tools get `rules/common/security-baseline/rule.md` — the installer injects the appropriate frontmatter

This lets authors write one canonical version for most tools and only specialise where needed.

## What the installer injects for `common/` rules

When deploying a rule from `rules/common/`, the installer prepends the correct frontmatter before writing the file:

**Kiro** — `inclusion: always` (always loaded into context):
```markdown
---
inclusion: always
---
# My Rule
...
```

**Cursor** — `alwaysApply: true` with empty description and globs:
```markdown
---
description: ""
globs: ""
alwaysApply: true
---
# My Rule
...
```

**Copilot** — `applyTo: "**"` (applies to all files), `.instructions.md` extension:
```markdown
---
applyTo: "**"
---
# My Rule
...
```

**Windsurf** — no frontmatter; appended to `~/.codeium/windsurf/global_rules.md` (home) or copied to `.windsurf/rules/<id>.md` (repo).

**Claude Code** — no frontmatter; appended to `~/.claude/CLAUDE.md` (home) or `AGENTS.md` (repo).

## What the installer injects for `common/` commands

**Kiro** — steering file with `inclusion: manual` (appears as `/id` in Kiro chat):
```markdown
---
inclusion: manual
---
# My Command
...
```

**Copilot** — `.prompt.md` extension with `mode: "agent"` frontmatter:
```markdown
---
mode: "agent"
description: ""
---
# My Command
...
```

**Cursor** — plain `.md`, no frontmatter, goes to `.cursor/commands/<id>.md`.

**Windsurf** — plain `.md`, goes to `.windsurf/workflows/<id>.md`.

**Claude Code** — plain `.md`, goes to `.claude/commands/<id>.md`.

## Writing tool-specific overrides

When a rule or command needs tool-specific behaviour — a Cursor glob pattern, a Copilot `applyTo` scoped to a specific path, a Kiro `fileMatch` — place it in the tool subfolder with the frontmatter already written:

```
rules/
  common/
    security-baseline/
      rule.md          ← plain content, no frontmatter
  cursor/
    security-baseline/
      rule.md          ← with globs: "src/**/*.ts" — Cursor only
  copilot/
    security-baseline/
      rule.md          ← with applyTo: "src/**" — Copilot only
```

The content of the rule (the actual instructions) is typically the same across all versions. Only the frontmatter differs. Authors can keep the content in sync manually, or use the `common/` version as the canonical source and only override the frontmatter in tool-specific files.

## Kiro commands are steering files

This is the most counterintuitive part of the model. Kiro does not have a native custom command system. Instead, steering files with `inclusion: manual` appear in the `/` slash command menu in Kiro chat. When the user types `/my-command`, Kiro injects the steering file's content into the conversation context.

This means:
- A command in `commands/kiro/` is stored in `.kiro/steering/` at install time
- The file must have `inclusion: manual` frontmatter
- The installer injects this frontmatter automatically for `commands/common/`
- For `commands/kiro/`, the author writes the frontmatter directly in the file

## Scope: home vs repo

Some component types are only meaningful in a repo context:

| Type | Home install | Repo install |
|---|---|---|
| Rules (Kiro) | `~/.kiro/steering/` | `<repo>/.kiro/steering/` |
| Rules (Cursor) | `~/.cursor/rules/` | `<repo>/.cursor/rules/` |
| Rules (Copilot) | — | `<repo>/.github/instructions/` |
| Rules (Windsurf) | append to `global_rules.md` | `<repo>/.windsurf/rules/` |
| Rules (Claude) | append to `~/.claude/CLAUDE.md` | append to `<repo>/AGENTS.md` |
| Commands (Kiro) | `~/.kiro/steering/` | `<repo>/.kiro/steering/` |
| Commands (Copilot) | — | `<repo>/.github/prompts/` |
| Commands (Cursor) | `~/.cursor/commands/` | `<repo>/.cursor/commands/` |
| Commands (Windsurf) | — | `<repo>/.windsurf/workflows/` |
| Commands (Claude) | `~/.claude/commands/` | `<repo>/.claude/commands/` |

Copilot rules and commands are always repo-scoped — Copilot reads them from the repository, not from a global user directory.
