# HAAL Presentation — Slide Plan

## Slide 1 — "The AI Artifact Fragmentation Problem"

**Phrase:** Every vendor ships their own registry format — teams drown in tool-specific configs.

**Body bullets:**

- GitHub Copilot, Cursor, Windsurf, Kiro, Claude Code, Amazon Q — each defines its own format and folder structure for rules, skills, agents, MCP servers
- A single team using 3 tools must maintain 3 sets of files in 3 different locations
- No standard way to curate, share, or govern AI artifacts across an organization

**Visual idea:** A chaotic web diagram — 5-6 vendor logos on the left, arrows pointing to different folder icons (`.github/`, `.cursor/`, `.kiro/`, `.claude/`, `.windsurf/`) — each arrow a different color/style, conveying fragmentation. A developer stick figure in the middle with a "?!" expression.

---

## Slide 2 — "One Registry Structure, One Installer, One Target"

**Phrase:** A unified registry format that maps artifacts to every tool's native locations — author once, install everywhere.

**Body bullets:**

- A single registry repo holds skills, rules, commands, agents, hooks, MCP servers — organized by type, scope, and tool
- The installer reads the registry and copies each artifact to the correct tool-specific path automatically
- 9 artifact types × 5 tools = one source of truth

**Visual idea:** A funnel/convergence diagram — the same vendor logos from Slide 1 now converge into a single "HAAL Registry" cylinder in the center, which fans out cleanly into neat folder icons on the right, each labelled with the tool name. Clean arrows, uniform style — order out of chaos.

---

## Slide 3 — "What We Built: Three Repos, One Ecosystem"

**Phrase:** `haal-skills` (the reference registry), `haal-installer` (the desktop app), `haal-registry-template` (fork & go).

**Body bullets:**

- **haal-skills** — 15+ competencies, 40+ skills, rules for 5 tools — a production-grade open-source registry
- **haal-installer** — Tauri 2 + Svelte desktop app: connect → choose → preview → execute → done
- **haal-registry-template** — empty scaffold with `.gitkeep` stubs for every artifact/tool folder — fork it, fill it, publish it

**Visual idea:** Three connected hexagons (or puzzle pieces) — left: "Registry" (haal-skills), center: "Installer" (haal-installer), right: "Template" (haal-registry-template). Arrows show the flow: Template → creates new registries → consumed by Installer ← reads from haal-skills. GitHub logo watermark to anchor "it's all just Git repos."

---

## Slide 4 — "Curate, Layer, Override"

**Phrase:** Enterprise baselines, division specializations, team overrides — multi-layer registries with deterministic merge.

**Body bullets:**

- **Curate** — define competencies (named bundles of skills, rules, agents) per team, org, or enterprise
- **Layer** — a seed registry declares secondary registries in `repos-manifest.json`; the installer fetches and merges them all
- **Override** — last-in-wins ordering: open-source → enterprise → division → team. The team seed always has final authority
- No transitive trust — the seed controls exactly which registries are included

**Visual idea:** A layered cake / stacked blocks diagram — bottom layer "Open Source (haal-skills)", middle "Enterprise (acme-skills)", top "Team (payments-skills)". An arrow labelled "overrides ↑" on the side. A small lock icon on the top layer to convey governance / authority.

---

## Slide 5 — "See It in Action"

**Phrase:** Live demo — from `git clone` to fully configured developer workstation in 90 seconds.

**Body bullets:**

- Authenticate → pick a competency → preview changes → install
- Show artifacts appearing in Copilot, Cursor, and Kiro simultaneously
- Show override behavior: team rule replacing enterprise default

**Visual idea:** A single large screenshot or embedded video placeholder of the installer UI (the wizard stepper: Connect → Choose → Preview → Execute → Done), with a "▶ Play" overlay button. Minimal slide — the demo is the star.
