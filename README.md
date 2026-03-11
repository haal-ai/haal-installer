# HAAL Installer

> ⚠️ Work in progress — not ready for production use.

A cross-platform desktop application for installing, updating, and managing AI coding tool components (skills, prompts, configurations) across tools like GitHub Copilot, Cursor, Claude Code, Kiro, and Windsurf.

## What it does

Instead of manually cloning repos and copying files, HAAL Installer gives you a guided wizard that handles the full lifecycle:

- **Connect** — authenticate to GitHub (public or enterprise) via OAuth or Personal Access Token
- **Choose** — browse and select which components to install, update, or remove
- **Preview** — see exactly what will change before anything runs (dry-run)
- **Execute** — runs the operations with live progress and automatic rollback on failure
- **Done** — summary of what was installed, with a log viewer for details

It also handles updates, deletions, and full reinitialization, with conflict detection, checksum verification, and version tracking built in.

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

## License

Apache 2.0 — see [LICENSE](./LICENSE).
