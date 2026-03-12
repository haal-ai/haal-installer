# Tasks: Registry Competency Redesign

## Overview

Implementation plan derived from the design and requirements documents. Tasks are ordered so each builds on the previous — data models first, then the loader, then the resolver integration, then the registry content, then the manifest.

---

## Task 1: Add CompetencyV2 data models to models.rs

**Requirement refs**: 1, 3, 4

Add the new Rust structs and enums for the v2 competency schema alongside the existing `CompetencyDetail`. The existing struct is kept unchanged for backward compatibility.

### Subtasks

- [ ] 1.1 Add `CompetencyShared` struct with optional `Vec<String>` fields: `skills`, `mcpservers`, `agents`, `systems`, `olafdata` (all `#[serde(default)]`)
- [ ] 1.2 Add `CompetencyToolBundle` struct with optional fields: `powers`, `rules`, `commands`, `hooks` (all `#[serde(default)]`)
- [ ] 1.3 Add `CompetencyV2` struct with fields: `name`, `description`, `schema_version: Option<u32>`, `shared: Option<CompetencyShared>`, `tools: Option<HashMap<String, CompetencyToolBundle>>`
- [ ] 1.4 Keep existing `CompetencyDetail` struct unchanged (used by legacy code paths)
- [ ] 1.5 Add `#[serde(rename_all = "camelCase")]` to all new structs to match the JSON format

**Files**: `src-tauri/src/models.rs`

---

## Task 2: Implement CompetencyLoader

**Requirement refs**: 1, 3, 4

Create a new `competency_loader.rs` module with `load_competency` and `normalise_legacy` functions. This is the core of the redesign — it produces a normalised `CompetencyV2` regardless of the on-disk schema version.

### Subtasks

- [ ] 2.1 Create `src-tauri/src/competency_loader.rs`
- [ ] 2.2 Implement `load_competency(path: &Path) -> Result<CompetencyV2, HaalError>`:
  - Read file from disk, return `FileSystemError` on read failure (include file path in message)
  - Parse JSON with `serde_json`, return `ValidationError` on parse failure (include file path)
  - If `schema_version` is `Some(2)` and `shared`/`tools` are present → validate and return as-is
  - Otherwise → call `normalise_legacy`
- [ ] 2.3 Implement `normalise_legacy(raw: CompetencyV2) -> CompetencyV2`:
  - Map top-level `skills` (from `CompetencyDetail`-shaped JSON) → `shared.skills`
  - Map top-level `powers` → `tools["kiro"].powers`
  - Set `tools["kiro"].rules`, `.commands`, `.hooks` to empty vecs
  - Handle absent `skills` and `powers` fields gracefully (default to `[]`)
- [ ] 2.4 Implement validation:
  - `name` and `description` must be non-empty — return `ValidationError` with field name and file path if not
  - All strings in all arrays must be non-empty — return `ValidationError` on first empty string found
  - `powers` under non-kiro tool keys → log warning, remove the field (do not error)
  - Unknown tool keys → silently ignored (serde handles this via `HashMap`)
- [ ] 2.5 Register the module in `src-tauri/src/lib.rs`
- [ ] 2.6 Write unit tests in the same file:
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

**Files**: `src-tauri/src/competency_loader.rs`, `src-tauri/src/lib.rs`

---

## Task 3: Add path traversal guard to DestinationResolver

**Requirement refs**: 6

Add a single validation helper that rejects any registry path containing `..` before it reaches the resolution logic. Called at the top of `resolve_one` for all component types.

### Subtasks

- [ ] 3.1 Add `fn validate_no_traversal(path: &Path) -> Result<(), HaalError>` to `DestinationResolver`:
  - Iterate path components; if any component equals `..` return a `ValidationError` with the offending path
- [ ] 3.2 Call `validate_no_traversal(&comp.source_path)` at the top of `resolve_one`; on error, log the rejection and return an empty `Vec<InstallAction>` (do not abort the whole install)
- [ ] 3.3 Write unit tests:
  - `path_with_dotdot_is_rejected`
  - `path_without_dotdot_is_accepted`
  - `path_traversal_does_not_abort_other_components`

**Files**: `src-tauri/src/destination_resolver.rs`

---

## Task 4: Wire CompetencyLoader into the install pipeline

**Requirement refs**: 5

Update the install pipeline so that when a competency is being installed, the `CompetencyLoader` is called to expand it into `ResolvedComponent` items before they reach `DestinationResolver`. Shared artifacts go to all selected tools; tool bundles go only to the matching selected tool.

### Subtasks

- [ ] 4.1 Locate where `CompetencyDetail` is currently consumed in `installer.rs` (or `registry_manager.rs`) to build `ResolvedComponent` lists
- [ ] 4.2 Replace that code path with a call to `CompetencyLoader::load_competency`
- [ ] 4.3 For `shared.skills` → emit one `ResolvedComponent { component_type: Skill, ... }` per skill ID
- [ ] 4.4 For `shared.mcpservers` → emit `McpServer` components
- [ ] 4.5 For `shared.agents` → emit `Agent` components
- [ ] 4.6 For `shared.systems` → emit `System` components
- [ ] 4.7 For `shared.olafdata` → emit `OlafData` components
- [ ] 4.8 For each tool in `selected_tools` that has a matching key in `tools`:
  - Emit `Power` components for `tools[tool].powers` (kiro only)
  - Emit `Rule` components for `tools[tool].rules`
  - Emit `Command` components for `tools[tool].commands`
  - Emit `Hook` components for `tools[tool].hooks`
- [ ] 4.9 Tools not in `selected_tools` → skip their bundle entirely
- [ ] 4.10 Unknown artifact IDs (source path does not exist on disk) → log warning with competency name + artifact ID, skip, continue
- [ ] 4.11 Write integration test: load a v2 competency fixture, resolve for `["kiro"]`, assert expected `InstallAction` list

**Files**: `src-tauri/src/installer.rs` (or `registry_manager.rs` — confirm in 4.1)

---

## Task 5: Update HaalManifest model and haal_manifest.json

**Requirement refs**: 7

Add `competency_schema_version` to the `HaalManifest` struct and bump the value in `haal-skills/haal_manifest.json`.

### Subtasks

- [ ] 5.1 Add `#[serde(default)] pub competency_schema_version: Option<u32>` to `HaalManifest` in `models.rs`
- [ ] 5.2 Update `haal-skills/haal_manifest.json`: add `"competencySchemaVersion": 2` at the top level, keep all existing fields unchanged
- [ ] 5.3 Verify existing tests that deserialise `HaalManifest` still pass (the field is `#[serde(default)]` so old JSON without it remains valid)

**Files**: `src-tauri/src/models.rs`, `haal-skills/haal_manifest.json`

---

## Task 6: Migrate developer.json to CompetencyV2 format

**Requirement refs**: 1, 2, 4

Migrate `developer.json` as the reference example of a v2 competency. This is the most complex competency (23 skills, 3 powers) and serves as the template for all others.

### Subtasks

- [ ] 6.1 Rewrite `haal-skills/competencies/developer.json` to v2 format:
  - `"schemaVersion": 2`
  - Move all skills to `shared.skills`
  - Move `code-in-go`, `code-in-rust`, `code-microservice-in-quarkus` to `tools.kiro.powers`
  - Add empty `tools.kiro.rules`, `tools.kiro.commands`, `tools.kiro.hooks` arrays (ready for future content)
  - Add empty tool bundles for `cursor`, `windsurf`, `claude`, `copilot` (rules/commands/hooks all `[]`)
- [ ] 6.2 Verify the file parses correctly with `CompetencyLoader::load_competency` (manual test or unit test)

**Files**: `haal-skills/competencies/developer.json`

---

## Task 7: Migrate remaining competencies to CompetencyV2 format

**Requirement refs**: 1, 4

Migrate all remaining v1 competency files to v2 format following the same pattern as `developer.json`. Each file gets `schemaVersion: 2`, skills move to `shared.skills`, powers (if any) move to `tools.kiro.powers`, and empty tool bundles are added.

### Subtasks

- [ ] 7.1 Migrate `architect.json`
- [ ] 7.2 Migrate `api-producers.json`
- [ ] 7.3 Migrate `api-consumers.json`
- [ ] 7.4 Migrate `specification.json`
- [ ] 7.5 Migrate `git-assistant.json`
- [ ] 7.6 Migrate `session-manager.json`
- [ ] 7.7 Migrate `prompt-engineer.json`
- [ ] 7.8 Migrate `technical-writer.json`
- [ ] 7.9 Migrate `base-skills.json`
- [ ] 7.10 Migrate `business-analyst.json`
- [ ] 7.11 Migrate `project-manager.json`
- [ ] 7.12 Migrate `researcher.json`
- [ ] 7.13 Migrate `1a-c-cpp-developpers.json`
- [ ] 7.14 Migrate `powers.json` (Kiro-only competency — all items go to `tools.kiro.powers`)
- [ ] 7.15 Migrate `test-competency.json`

**Files**: `haal-skills/competencies/*.json`

---

## Task 8: Create registry folder scaffolding for rules, commands, hooks

**Requirement refs**: 2

Create the new top-level folders in `haal-skills/` with `.gitkeep` placeholders so the structure is committed and visible to registry maintainers.

### Subtasks

- [ ] 8.1 Create `haal-skills/rules/global/kiro/.gitkeep`
- [ ] 8.2 Create `haal-skills/rules/global/cursor/.gitkeep`
- [ ] 8.3 Create `haal-skills/rules/global/windsurf/.gitkeep`
- [ ] 8.4 Create `haal-skills/rules/global/claude/.gitkeep`
- [ ] 8.5 Create `haal-skills/rules/global/copilot/.gitkeep`
- [ ] 8.6 Create `haal-skills/rules/repo/kiro/.gitkeep`
- [ ] 8.7 Create `haal-skills/rules/repo/cursor/.gitkeep`
- [ ] 8.8 Create `haal-skills/rules/repo/windsurf/.gitkeep`
- [ ] 8.9 Create `haal-skills/rules/repo/claude/.gitkeep`
- [ ] 8.10 Create `haal-skills/rules/repo/copilot/.gitkeep`
- [ ] 8.11 Create `haal-skills/rules/repo/agents/.gitkeep`
- [ ] 8.12 Create `haal-skills/commands/global/kiro/.gitkeep`
- [ ] 8.13 Create `haal-skills/commands/global/claude/.gitkeep`
- [ ] 8.14 Create `haal-skills/commands/global/cursor/.gitkeep`
- [ ] 8.15 Create `haal-skills/commands/global/windsurf/.gitkeep`
- [ ] 8.16 Create `haal-skills/commands/repo/kiro/.gitkeep`
- [ ] 8.17 Create `haal-skills/commands/repo/claude/.gitkeep`
- [ ] 8.18 Create `haal-skills/commands/repo/cursor/.gitkeep`
- [ ] 8.19 Create `haal-skills/commands/repo/copilot/.gitkeep`
- [ ] 8.20 Create `haal-skills/commands/repo/windsurf/.gitkeep`
- [ ] 8.21 Create `haal-skills/hooks/kiro/.gitkeep`
- [ ] 8.22 Create `haal-skills/hooks/cursor/.gitkeep`
- [ ] 8.23 Create `haal-skills/hooks/windsurf/.gitkeep`
- [ ] 8.24 Create `haal-skills/hooks/copilot/.gitkeep`

**Files**: `haal-skills/rules/`, `haal-skills/commands/`, `haal-skills/hooks/`

---

## Task 9: Add property-based tests

**Requirement refs**: 1, 4, 5, 6, 8 (Correctness Properties 1–10)

Add `proptest`-based tests to verify the correctness properties from the requirements document. These complement the unit tests in Task 2 with exhaustive random input coverage.

### Subtasks

- [ ] 9.1 Add `proptest` to `[dev-dependencies]` in `src-tauri/Cargo.toml`
- [ ] 9.2 Create `src-tauri/src/competency_loader_props.rs` (or add a `proptest` module inside `competency_loader.rs`)
- [ ] 9.3 Implement Property 1: v2 round-trip serialisation
- [ ] 9.4 Implement Property 2: legacy normalisation — skills mapping
- [ ] 9.5 Implement Property 3: legacy normalisation — powers mapping
- [ ] 9.6 Implement Property 4: legacy normalisation — empty tool arrays
- [ ] 9.7 Implement Property 5: unknown tool keys are ignored
- [ ] 9.8 Implement Property 6: no tools selected → zero tool-specific actions
- [ ] 9.9 Implement Property 7: single-tool actions ⊆ all-tools actions
- [ ] 9.10 Implement Property 8: path traversal rejection
- [ ] 9.11 Implement Property 9: unknown artifact IDs do not abort resolution
- [ ] 9.12 Implement Property 10: malformed JSON returns error, not panic

**Files**: `src-tauri/src/competency_loader.rs` (or new props file), `src-tauri/Cargo.toml`

---

## Completion Checklist

- [ ] All unit tests pass: `cargo test`
- [ ] All property tests pass: `cargo test` (proptest runs inline)
- [ ] `developer.json` loads and resolves correctly end-to-end
- [ ] All 15 competency files are in v2 format
- [ ] `haal_manifest.json` has `competencySchemaVersion: 2`
- [ ] Registry folder scaffolding committed to `haal-skills/`
- [ ] No `..` path traversal possible through any competency JSON
