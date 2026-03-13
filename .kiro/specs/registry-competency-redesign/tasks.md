# Tasks: Registry Competency Redesign

## Repo Reference

Two repos are involved. Every task header and every file path is prefixed with the repo it belongs to.

| Label | Repo | Local path |
|---|---|---|
| `[installer]` | haal-installer | `c:\Users\ppaccaud\coderepos\haal-ai\haal-installer\` (workspace root) |
| `[haal-skills]` | haal-skills | `c:\Users\ppaccaud\coderepos\haal-ai\haal-installer\haal-skills\` (subfolder, own git repo) |

Tasks 1–4, 9 → `[installer]` only  
Tasks 6–8 → `[haal-skills]` only  
Task 5 → both repos (one file each)

---

## Task 1 `[installer]`: Add CompetencyV2 data models

**Requirement refs**: 1, 3, 4

Add the new Rust structs for the v2 competency schema alongside the existing `CompetencyDetail`. The existing struct is kept unchanged for backward compatibility.

### Subtasks

- [x] 1.1 Add `CompetencyShared` struct — optional `Vec<String>` fields: `skills`, `mcpservers`, `agents`, `systems`, `olafdata` (all `#[serde(default)]`)
- [x] 1.2 Add `CompetencyToolBundle` struct — optional fields: `powers`, `rules`, `commands`, `hooks` (all `#[serde(default)]`)
- [x] 1.3 Add `CompetencyV2` struct — fields: `name`, `description`, `schema_version: Option<u32>`, `shared: Option<CompetencyShared>`, `tools: Option<HashMap<String, CompetencyToolBundle>>`
- [x] 1.4 Keep existing `CompetencyDetail` struct unchanged (used by legacy code paths)
- [x] 1.5 Add `#[serde(rename_all = "camelCase")]` to all new structs

**Repo**: `[installer]`  
**Files**: `src-tauri/src/models.rs`

---

## Task 2 `[installer]`: Implement CompetencyLoader

**Requirement refs**: 1, 3, 4

Create a new `competency_loader.rs` module. Produces a normalised `CompetencyV2` regardless of the on-disk schema version.

### Subtasks

- [x] 2.1 Create `src-tauri/src/competency_loader.rs`
- [x] 2.2 Implement `load_competency(path: &Path) -> Result<CompetencyV2, HaalError>`:
  - Read file, return `FileSystemError` on failure (include file path)
  - Parse JSON, return `ValidationError` on failure (include file path)
  - `schema_version == Some(2)` and `shared`/`tools` present → validate and return as-is
  - Otherwise → call `normalise_legacy`
- [x] 2.3 Implement `normalise_legacy(raw: serde_json::Value) -> CompetencyV2`:
  - Top-level `skills` → `shared.skills`
  - Top-level `powers` → `tools["kiro"].powers`
  - `tools["kiro"].rules/commands/hooks` → empty vecs
  - Absent `skills` or `powers` → default to `[]`
- [x] 2.4 Implement validation:
  - `name` and `description` non-empty — `ValidationError` with field + file path if not
  - All strings in all arrays non-empty — `ValidationError` on first empty string
  - `powers` under non-kiro tool key → log warning, ignore (no error)
  - Unknown tool keys → silently ignored via `HashMap`
- [x] 2.5 Register module in `src-tauri/src/lib.rs`
- [x] 2.6 Write unit tests:
  - `load_v2_competency_returns_competency_v2`
  - `load_v1_competency_normalises_skills_and_powers`
  - `load_missing_skills_defaults_to_empty`
  - `load_missing_powers_defaults_to_empty`
  - `load_missing_name_returns_error`
  - `load_missing_description_returns_error`
  - `load_invalid_json_returns_error_with_path`
  - `load_empty_artifact_id_returns_error`
  - `load_powers_under_non_kiro_tool_emits_warning_and_ignores`
  - `load_unknown_tool_key_is_silently_ignored`
  - `load_mixed_v1_v2_collection_normalises_independently`

**Repo**: `[installer]`  
**Files**: `src-tauri/src/competency_loader.rs`, `src-tauri/src/lib.rs`

---

## Task 3 `[installer]`: Add path traversal guard to DestinationResolver

**Requirement refs**: 6

Single validation helper that rejects any registry path containing `..` before it reaches resolution logic.

### Subtasks

- [x] 3.1 Add `fn validate_no_traversal(path: &Path) -> Result<(), HaalError>` to `DestinationResolver`:
  - Iterate path components; any component equal to `..` → `ValidationError` with offending path
- [x] 3.2 Call `validate_no_traversal(&comp.source_path)` at the top of `resolve_one`; on error log and return empty `Vec<InstallAction>` (do not abort the whole install)
- [x] 3.3 Write unit tests:
  - `path_with_dotdot_is_rejected`
  - `path_without_dotdot_is_accepted`
  - `path_traversal_does_not_abort_other_components`

**Repo**: `[installer]`  
**Files**: `src-tauri/src/destination_resolver.rs`

---

## Task 4 `[installer]`: Wire CompetencyLoader into the install pipeline

**Requirement refs**: 5

When a competency is being installed, call `CompetencyLoader` to expand it into `ResolvedComponent` items. Shared artifacts go to all selected tools; tool bundles go only to the matching selected tool.

### Subtasks

- [x] 4.1 Locate where `CompetencyDetail` is currently consumed in `installer.rs` or `registry_manager.rs` to build `ResolvedComponent` lists
- [x] 4.2 Replace that code path with `CompetencyLoader::load_competency`
- [x] 4.3 `shared.skills` → one `ResolvedComponent { component_type: Skill }` per ID
- [x] 4.4 `shared.mcpservers` → `McpServer` components
- [x] 4.5 `shared.agents` → `Agent` components
- [x] 4.6 `shared.systems` → `System` components
- [x] 4.7 `shared.olafdata` → `OlafData` components
- [x] 4.8 For each tool in `selected_tools` with a matching key in `tools`:
  - `tools[tool].powers` → `Power` components (kiro only)
  - `tools[tool].rules` → `Rule` components
  - `tools[tool].commands` → `Command` components
  - `tools[tool].hooks` → `Hook` components
- [x] 4.9 Tools not in `selected_tools` → skip bundle entirely
- [x] 4.10 Unknown artifact ID (source path absent on disk) → log warning with competency name + ID, skip, continue
- [x] 4.11 Write integration test: load a v2 competency fixture, resolve for `["kiro"]`, assert expected `InstallAction` list

**Repo**: `[installer]`  
**Files**: `src-tauri/src/installer.rs` (or `registry_manager.rs` — confirm in 4.1)

---

## Task 5: Update manifest model and haal_manifest.json

**Requirement refs**: 7

Two files in two different repos — do not mix them up.

### Subtasks

- [x] 5.1 `[installer]` — Add `#[serde(default)] pub competency_schema_version: Option<u32>` to `HaalManifest` in `src-tauri/src/models.rs`
- [x] 5.2 `[haal-skills]` — Add `"competencySchemaVersion": 2` at the top level of `haal_manifest.json`, keep all existing fields unchanged
- [x] 5.3 `[installer]` — Verify existing tests that deserialise `HaalManifest` still pass (`#[serde(default)]` means old JSON without the field remains valid)

**Repos**: `[installer]` (5.1, 5.3) and `[haal-skills]` (5.2)  
**Files**:
- `[installer]` `src-tauri/src/models.rs`
- `[haal-skills]` `haal_manifest.json`

---

## Task 6 `[haal-skills]`: Migrate developer.json to CompetencyV2 format

**Requirement refs**: 1, 2, 4

Reference example of a v2 competency. Most complex file (23 skills, 3 powers) — serves as the template for Task 7.

### Subtasks

- [x] 6.1 Rewrite `competencies/developer.json`:
  - `"schemaVersion": 2`
  - All skills → `shared.skills`
  - `code-in-go`, `code-in-rust`, `code-microservice-in-quarkus` → `tools.kiro.powers`
  - Empty `tools.kiro.rules/commands/hooks` arrays
  - Empty tool bundles for `cursor`, `windsurf`, `claude`, `copilot`
- [x] 6.2 Verify the file parses correctly with `CompetencyLoader::load_competency`

**Repo**: `[haal-skills]`  
**Files**: `competencies/developer.json`

---

## Task 7 `[haal-skills]`: Migrate remaining competencies to CompetencyV2 format

**Requirement refs**: 1, 4

Same pattern as Task 6: `schemaVersion: 2`, skills → `shared.skills`, powers → `tools.kiro.powers`, empty tool bundles added.

### Subtasks

- [x] 7.1 Migrate `competencies/architect.json`
- [x] 7.2 Migrate `competencies/api-producers.json`
- [x] 7.3 Migrate `competencies/api-consumers.json`
- [x] 7.4 Migrate `competencies/specification.json`
- [x] 7.5 Migrate `competencies/git-assistant.json`
- [x] 7.6 Migrate `competencies/session-manager.json`
- [x] 7.7 Migrate `competencies/prompt-engineer.json`
- [x] 7.8 Migrate `competencies/technical-writer.json`
- [x] 7.9 Migrate `competencies/base-skills.json`
- [x] 7.10 Migrate `competencies/business-analyst.json`
- [x] 7.11 Migrate `competencies/project-manager.json`
- [x] 7.12 Migrate `competencies/researcher.json`
- [x] 7.13 Migrate `competencies/1a-c-cpp-developpers.json`
- [x] 7.14 Migrate `competencies/powers.json` (Kiro-only — all items go to `tools.kiro.powers`, `shared` is empty)
- [x] 7.15 Migrate `competencies/test-competency.json`

**Repo**: `[haal-skills]`  
**Files**: `competencies/*.json`

---

## Task 8 `[haal-skills]`: Create registry folder scaffolding

**Requirement refs**: 2

New top-level folders with `.gitkeep` placeholders so the structure is committed and visible to registry maintainers.

### Subtasks

- [x] 8.1 `rules/global/kiro/.gitkeep`
- [x] 8.2 `rules/global/cursor/.gitkeep`
- [x] 8.3 `rules/global/windsurf/.gitkeep`
- [x] 8.4 `rules/global/claude/.gitkeep`
- [x] 8.5 `rules/global/copilot/.gitkeep`
- [x] 8.6 `rules/repo/kiro/.gitkeep`
- [x] 8.7 `rules/repo/cursor/.gitkeep`
- [x] 8.8 `rules/repo/windsurf/.gitkeep`
- [x] 8.9 `rules/repo/claude/.gitkeep`
- [x] 8.10 `rules/repo/copilot/.gitkeep`
- [x] 8.11 `rules/repo/agents/.gitkeep`
- [x] 8.12 `commands/global/kiro/.gitkeep`
- [x] 8.13 `commands/global/claude/.gitkeep`
- [x] 8.14 `commands/global/cursor/.gitkeep`
- [x] 8.15 `commands/global/windsurf/.gitkeep`
- [x] 8.16 `commands/repo/kiro/.gitkeep`
- [x] 8.17 `commands/repo/claude/.gitkeep`
- [x] 8.18 `commands/repo/cursor/.gitkeep`
- [x] 8.19 `commands/repo/copilot/.gitkeep`
- [x] 8.20 `commands/repo/windsurf/.gitkeep`
- [x] 8.21 `hooks/kiro/.gitkeep`
- [x] 8.22 `hooks/cursor/.gitkeep`
- [x] 8.23 `hooks/windsurf/.gitkeep`
- [x] 8.24 `hooks/copilot/.gitkeep`

**Repo**: `[haal-skills]`  
**Files**: `rules/`, `commands/`, `hooks/`

---

## Task 9 `[installer]`: Add property-based tests

**Requirement refs**: 1, 4, 5, 6, 8 (Correctness Properties 1–10)

`proptest`-based tests complementing the unit tests in Task 2.

### Subtasks

- [x] 9.1 Add `proptest` to `[dev-dependencies]` in `src-tauri/Cargo.toml`
- [x] 9.2 Add a `proptest` module inside `src-tauri/src/competency_loader.rs`
- [x] 9.3 Property 1: v2 round-trip serialisation
- [x] 9.4 Property 2: legacy normalisation — skills mapping
- [x] 9.5 Property 3: legacy normalisation — powers mapping
- [x] 9.6 Property 4: legacy normalisation — empty tool arrays
- [x] 9.7 Property 5: unknown tool keys are ignored
- [x] 9.8 Property 6: no tools selected → zero tool-specific actions
- [x] 9.9 Property 7: single-tool actions ⊆ all-tools actions
- [x] 9.10 Property 8: path traversal rejection
- [x] 9.11 Property 9: unknown artifact IDs do not abort resolution
- [x] 9.12 Property 10: malformed JSON returns error, not panic

**Repo**: `[installer]`  
**Files**: `src-tauri/src/competency_loader.rs`, `src-tauri/Cargo.toml`

---

## Task 10 `[installer]`: Align docs/artifact-types.md and docs/rules-and-commands.md with the new design

**Requirement refs**: 1, 2

These two files were partially updated in the last commit but still contain stale content from before the redesign:

`docs/artifact-types.md`:
- The Rules summary table still shows the old Claude destination (`appended to ~/.claude/CLAUDE.md` / `appended to AGENTS.md`) — should now be `~/.claude/rules/<filename>.md` and `<repo>/.claude/rules/<filename>.md`
- The Commands summary table shows Windsurf as repo-only — should now include the global path `~/.codeium/windsurf/global_workflows/`

`docs/rules-and-commands.md`:
- The subfolder tree examples show a single-level layout (`rules/kiro/`, `rules/cursor/`) — should now show the two-level `rules/<scope>/<tool>/` layout matching the new registry structure
- The commands tree similarly needs updating to `commands/<scope>/<tool>/`

### Subtasks

- [x] 10.1 `docs/artifact-types.md` — fix the Rules summary table: Claude row → `~/.claude/rules/<filename>.md` (home) and `<repo>/.claude/rules/<filename>.md` (repo)
- [x] 10.2 `docs/artifact-types.md` — fix the Commands summary table: Windsurf row → add `~/.codeium/windsurf/global_workflows/` as the home path
- [x] 10.3 `docs/rules-and-commands.md` — update the subfolder tree examples to show `rules/global/<tool>/` and `rules/repo/<tool>/` layout
- [x] 10.4 `docs/rules-and-commands.md` — update the commands subfolder tree to show `commands/global/<tool>/` and `commands/repo/<tool>/` layout
- [x] 10.5 Cross-check both files against `docs/install-paths.md` (the authoritative reference) and fix any remaining discrepancies

**Repo**: `[installer]`  
**Files**: `docs/artifact-types.md`, `docs/rules-and-commands.md`

---

## Task 11 `[haal-skills]`: Assess and update the shell install scripts

**Requirement refs**: 1, 2, 4

The two shell install scripts (`install-haal-skills.ps1` and `install-haal-skills.sh`) were written for the v1 competency schema. They read `skills` and `powers` from competency JSON files and install them. With the v2 schema, skills move to `shared.skills` and powers move to `tools.kiro.powers`. The scripts must be updated to read from the new locations.

Note: these scripts are a lightweight CLI alternative to the Tauri installer — they do not handle rules, commands, hooks, agents, or MCP servers. That scope does not change. The only impact is the JSON path they read from competency files.

### Subtasks

- [x] 11.1 Audit `get_skills_from_competency` (sh) / `Get-SkillsFromCompetencies` (ps1): currently reads top-level `skills` array — update to read `shared.skills` when `schemaVersion == 2`, fall back to top-level `skills` for v1
- [x] 11.2 Audit the powers install call: currently reads top-level `powers` array (delegated to `install-powers.ps1` / `install-powers.sh`) — update the powers scripts to read `tools.kiro.powers` when `schemaVersion == 2`, fall back to top-level `powers` for v1
- [x] 11.3 Verify the `haal_manifest.json` parsing is unaffected (the scripts only read `collections[].competencyIds` from the manifest — the new `competencySchemaVersion` field is additive and ignored by the scripts)
- [x] 11.4 Test manually: run the updated ps1 script against the migrated `developer.json` (v2) and a legacy v1 competency file, confirm skills are resolved correctly in both cases

**Repo**: `[haal-skills]`  
**Files**: `.olaf/tools/install-haal-skills.ps1`, `.olaf/tools/install-haal-skills.sh`, `.olaf/tools/install-powers.ps1`, `.olaf/tools/install-powers.sh`

---

## Completion Checklist

### `[installer]`
- [x] `cargo test` — all unit and property tests pass
- [x] `developer.json` loads and resolves correctly end-to-end via `CompetencyLoader`
- [x] No `..` path traversal possible through any competency JSON

### `[installer]`
- [x] `docs/artifact-types.md` and `docs/rules-and-commands.md` match `docs/install-paths.md`

### `[haal-skills]`
- [x] All 15 competency files are in v2 format and parse without error
- [x] `haal_manifest.json` has `"competencySchemaVersion": 2`
- [x] `rules/`, `commands/`, `hooks/` folder scaffolding committed
- [x] Install scripts handle both v1 and v2 competency JSON correctly
