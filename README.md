# HAAL Installer

> **Work in progress — as of March 2026. Not ready for production use.**
>
> The installer is functional in development mode but has not been packaged or released yet. APIs, registry formats, and component structures may change without notice.

A cross-platform desktop application for installing, updating, and managing AI coding tool components (skills, prompts, rules, commands, MCP servers, agentic systems) across tools like GitHub Copilot, Cursor, Claude Code, Kiro, and Windsurf.

## Download

Pre-built installers are published on the [GitHub Releases page](https://github.com/haal-ai/haal-installer/releases).

| Platform | File |
|---|---|
| Windows | `.msi` or `.exe` (NSIS) |
| macOS (Apple Silicon) | `.dmg` (aarch64) |
| macOS (Intel) | `.dmg` (x86_64) |
| Linux | `.deb` or `.AppImage` |

Download the file for your platform, run it, and follow the installer prompts. On first launch the app copies itself to `~/.haal/bin/` and adds that directory to your PATH so you can run `haal-installer` from any terminal.

## What it does

Instead of manually cloning repos and copying files, HAAL Installer gives you a guided wizard that handles the full lifecycle:

- **Connect** — authenticate to GitHub (public or enterprise) via OAuth or Personal Access Token
- **Choose** — browse and select collections or competencies from one or more registries
- **Preview** — see exactly what will change before anything runs, per tool and per component type
- **Execute** — runs the operations with live progress
- **Done** — summary of what was installed

It supports multiple registries simultaneously (team, division, enterprise, open-source) with a clear merge and override model. See [docs/multi-registry.md](docs/multi-registry.md).

## Stack

- [Tauri 2](https://tauri.app) (Rust backend) — native desktop shell, ~5MB installer
- [Svelte 5](https://svelte.dev) — frontend UI
- [Tailwind CSS 4](https://tailwindcss.com) — styling

## Supported platforms

- Windows 10+
- macOS (Intel + Apple Silicon)
- Linux (Ubuntu, Fedora, Debian)

## Development

```bash
npm install
npm run tauri dev
```

Requires Rust (via [rustup](https://rustup.rs)) and Node.js.

## Documentation

- [docs/registry-structure.md](docs/registry-structure.md) — registry layout, manifest format, component folders
- [docs/multi-registry.md](docs/multi-registry.md) — seed vs secondary registries, enterprise governance model
- [docs/rules-and-commands.md](docs/rules-and-commands.md) — tool-specific frontmatter and file placement

## License

Copyright 2026 haal-ai — Apache 2.0. See [LICENSE](./LICENSE).
