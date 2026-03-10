# Vision — Desktop Installer & Lifecycle Manager

## Summary

A cross-platform desktop application that replaces existing bash/powershell installation scripts with a friendly, GUI-based tool. The app manages the full lifecycle of installed components: install, update, delete, and reinitialize. It targets both technical and non-technical users.

### Target Platforms

- **Windows** 10 and above
- **macOS** (Intel and Apple Silicon)
- **Linux** (major distributions: Ubuntu, Fedora, Debian)

## Problem

- Current installation relies on shell scripts (`git clone`, file copy, file delete)
- Non-technical users struggle with CLI-based workflows
- No visibility into what's installed, what version, or what changed
- No easy way to update, roll back, or clean up

## Chosen Stack

| Layer | Technology | Why |
|---|---|---|
| Desktop shell | Tauri 2.x (Rust) | Small installers (~5-10MB), native filesystem access, built-in auto-updater, OS keychain support |
| Frontend | Svelte | Minimal syntax, compiles to small bundles, pairs naturally with Tauri, easy to generate with GenAI |
| Code generation | GenAI-assisted | Team does not know React or Svelte — code will be generated and iterated with AI |

## User Experience

### Wizard / Stepper Flow

The app guides users through a step-by-step flow with a visible progress indicator:

1. **Connect** — Authenticate to GitHub (public or enterprise)
2. **Choose** — Select components to install/update/delete from lists
3. **Preview** — See exactly what will happen before confirming (dry-run)
4. **Execute** — Run the operations with live progress
5. **Done** — Summary of what was done, with option to view logs

### Selection UI

- **Single-select lists** — pick one option (e.g., GitHub instance, destination folder)
- **Multi-select lists** — pick multiple items (e.g., which components/repos to install)
- Clear labels and descriptions for every option
- Search/filter for long lists

### App Modes

| Mode | Description |
|---|---|
| **Install** | Fresh setup: authenticate, choose options, clone repos, copy files |
| **Update** | Pull latest changes, update files in place |
| **Delete** | Remove selected or all installed components |
| **Reinitialize** | Wipe everything and start fresh from scratch |

## Functional Requirements

### GitHub Authentication

- Login to **github.com** (public)
- Login to **GitHub Enterprise** (custom URL)
- OAuth or Personal Access Token (PAT)
- Credentials stored securely in the **OS keychain** (via Tauri's secure storage) — never in plain text

### Core Operations

- `git clone` / `git pull` repositories
- Copy files to target locations
- Delete files
- Progress indicators for all operations

### Reliability

- **Offline detection** — check connectivity before operations, show clear message on failure
- **Retry on failure** — automatic retry with manual retry button for network issues
- **Conflict detection** — warn if destination folder has existing files, offer overwrite/skip/merge
- **Disk space check** — verify available space before clone/copy
- **Rollback** — if an operation fails mid-way, revert to previous state instead of leaving things broken
- **Checksum verification** — verify integrity of cloned/downloaded content

### Lifecycle Management

- **Version tracking** — remember what version of each component is installed
- **Change detection** — Update mode knows what changed since last install
- **Auto-update for the app itself** — Tauri built-in updater keeps the installer app current
- **Configuration persistence** — save user choices (GitHub instance, repos, destination) so they don't re-enter them each time

### Configuration Sharing

- **Export config** — save current setup as a shareable file
- **Import config** — load a shared config so teams install the same setup consistently
- Config format: JSON or YAML

### Theming & Appearance

- **Dark / light theme** — respects OS preference, with manual toggle
- **Simple theming** — CSS variables-based, easy to reskin after MVP without touching backend logic
- Appearance is fully decoupled from the Tauri backend — colors, fonts, layout, icons can all be changed at any time
- Branding (app icon, window title, installer graphics) configurable via `tauri.conf.json`

### Multilingual Support (i18n)

- **MVP languages:** French and English
- Auto-detect language from OS locale, with manual language picker in the UI
- Translation files stored as JSON per language (`locales/en.json`, `locales/fr.json`)
- All UI strings use translation keys — no hardcoded text
- Additional languages can be added later by dropping in new JSON files

### User Trust & Polish

- **Dry-run / preview** — show what will happen before any destructive action
- **Collapsible log panel** — technical users see details, non-technical users ignore it
- **Clear error messages** — human-readable, actionable ("Check your token has repo access" not "Error: 401")

## Distribution

- Platform installers: `.msi` (Windows 10+), `.dmg` (macOS Intel + Apple Silicon), `.deb` / `.AppImage` (Linux)
- `tauri-action` GitHub Action builds all platform installers on push/tag
- Hosted on **GitHub Releases** and/or a static website
- Built-in auto-updater for subsequent app updates

## Alternatives Considered

| Option | Verdict |
|---|---|
| React + Tauri | Viable but React is heavier and unknown to the team |
| Electron | Large bundle (~150MB), overkill |
| Go CLI binary | No GUI, not suitable for non-technical users |
| Package existing scripts (Inno Setup) | Fragile, platform-specific, hard to maintain |
| Tauri + plain HTML/JS | Possible but Svelte adds minimal overhead with better DX |

## Next Steps

- Implementation will happen in a **separate repository**
- This document serves as the product vision and feature scope
- Detailed technical design and task breakdown will follow in the implementation repo
