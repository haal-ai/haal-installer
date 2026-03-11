# Architecture

haal-installer is a cross-platform desktop application built with Tauri and Svelte. This document explains the technology choices and how the pieces fit together.

## Technology stack

### Tauri

[Tauri](https://tauri.app) is a framework for building desktop applications using web technologies for the UI and Rust for the backend. Unlike Electron, Tauri does not bundle a full Chromium instance — it uses the operating system's native webview (WebKit on macOS/Linux, WebView2 on Windows). This results in a much smaller binary (typically under 10 MB) and lower memory usage.

The Rust backend exposes typed commands that the frontend calls via an async IPC bridge. The frontend cannot access the file system or network directly — all privileged operations go through Rust.

haal-installer uses Tauri 2.

### Svelte 5

[Svelte](https://svelte.dev) is a UI framework that compiles components to vanilla JavaScript at build time. There is no virtual DOM and no runtime framework shipped to the browser — the output is plain, efficient DOM manipulation code.

Svelte 5 introduces runes, a new reactivity model based on signals. State is declared with `$state()`, derived values with `$derived()`, and side effects with `$effect()`. This makes reactivity explicit and predictable without the magic of Svelte 4's `$:` syntax.

For a Tauri app, Svelte is a natural fit: the compiled output is small, startup is fast, and there is no framework overhead sitting between the UI and the native webview.

### Why this combination

The core constraint for haal-installer is that it must run on Windows, macOS, and Linux without requiring users to install Node.js, Python, or any runtime. A single self-contained binary is the goal.

Tauri + Rust delivers this: the backend is compiled to a native binary, the frontend is compiled to static HTML/JS/CSS, and Tauri bundles everything into a single installer. The result is a native desktop app that feels fast and installs cleanly.

Electron would work but ships a 150–300 MB bundle. A pure CLI would work but loses discoverability and the wizard UX that guides users through registry connection, competency selection, and tool detection. Tauri hits the middle ground.

### Other frontend dependencies

- TypeScript — type safety across the frontend
- Tailwind CSS 4 — utility-first styling, compiled to minimal CSS
- Vite — fast dev server and build tool
- svelte-i18n — English and French localisation
- Vitest + Testing Library — unit and component tests

## Project structure

```
haal-installer/
  src/                        ← Svelte frontend
    lib/
      components/             ← UI components (wizard steps, panels)
      stores/                 ← Svelte 5 rune-based state stores
      i18n/                   ← en.json, fr.json translations
  src-tauri/                  ← Rust backend
    src/
      lib.rs                  ← Tauri command definitions (the IPC surface)
      models.rs               ← Shared data types (serialized over IPC)
      registry_manager.rs     ← Fetches and caches registry manifests
      repo_manager.rs         ← Clones repos, builds merged catalog
      destination_resolver.rs ← Maps component types → install paths per tool
      installer.rs            ← Executes install operations
      tool_detector.rs        ← Detects installed AI tools
      github_auth.rs          ← GitHub authentication (gh CLI + token)
      system_installer.rs     ← Clones and sets up agentic systems
      manifest_parser.rs      ← Parses competency JSON files
      conflict_detector.rs    ← Detects existing installs before overwriting
      rollback_manager.rs     ← Reverts failed installs
      operation_engine.rs     ← Orchestrates multi-step install operations
      self_installer.rs       ← Installs the app itself to ~/.haal/
    Cargo.toml
  package.json
```

## Key Rust dependencies

| Crate | Purpose |
|---|---|
| `tauri 2` | Desktop app framework, IPC bridge |
| `tokio` | Async runtime for concurrent registry fetches |
| `reqwest` | HTTP client for fetching manifests and competency files |
| `git2` | Git operations (clone, pull) for registry and system repos |
| `serde / serde_json` | Serialization of all types crossing the IPC boundary |
| `sha2` | Checksums for install verification |
| `tracing` | Structured logging |
| `dirs` | Cross-platform home directory resolution |

## Data flow

```
User action (Svelte UI)
  │
  ▼
Tauri IPC command (lib.rs)
  │
  ├── registry_manager.rs   fetch manifest + competency files (HTTP, cached)
  ├── repo_manager.rs       clone seed + secondary repos, build merged catalog
  ├── tool_detector.rs      detect installed AI tools
  │
  ▼
User selects competencies + tools (Svelte UI)
  │
  ▼
install_components_v2 command
  │
  ├── manifest_parser.rs    resolve component IDs → source paths
  ├── destination_resolver.rs  map each component → install operations per tool
  ├── conflict_detector.rs  check for existing files
  ├── installer.rs          execute copy / inject / append / merge operations
  └── rollback_manager.rs   revert on failure
```

## IPC boundary

All types that cross the Tauri IPC boundary are defined in `models.rs` and derive `serde::Serialize` / `serde::Deserialize`. The frontend receives plain JSON and TypeScript types are inferred from the Tauri-generated bindings.

The frontend never touches the file system or network directly. Every privileged operation is a named Tauri command in `lib.rs`.
