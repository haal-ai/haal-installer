# Requirements Document

## Introduction

The haal-skills registry currently uses a flat competency model (schemaVersion 1) that only groups `skills` and `powers` arrays. This feature introduces a structured `CompetencyV2` JSON schema with a `shared` section for tool-agnostic artifacts and a `tools` section for per-tool bundles (rules, commands, hooks). The change touches three surfaces: the competency JSON schema, the registry folder layout, and the installer's competency loader and destination resolver. Existing v1 competencies remain valid through automatic normalisation.

## Glossary

- **CompetencyLoader**: The installer component responsible for reading, parsing, and normalising competency JSON files into a `CompetencyV2` internal representation.
- **DestinationResolver**: The installer component (implemented in `destination_resolver.rs`) that maps registry artifact paths to install destinations and produces `InstallAction` items.
- **CompetencyV2**: The canonical internal data model for a competency, regardless of the on-disk schema version.
- **LegacyCompetency**: A competency JSON file with `schemaVersion: 1` or no `schemaVersion` field, containing top-level `skills` and/or `powers` arrays.
- **InstallAction**: A resolved instruction describing a source registry path and a destination path for a single artifact.
- **Shared artifact**: A tool-agnostic artifact (skill, MCP server, agent, system, or OLAF data entry) that is installed for every selected tool.
- **Tool bundle**: The set of tool-specific artifacts (rules, commands, hooks, and for Kiro: powers) defined under `tools.<tool>` in a competency.
- **Registry path**: A slash-separated string referencing an artifact relative to the registry root (e.g. `rules/repo/kiro/developer-guidelines`).
- **ToolId**: One of the recognised tool identifiers: `kiro`, `cursor`, `windsurf`, `claude`, `copilot`.
- **Scope**: Either `global` or `repo`, used as a path segment in rules and commands registry paths.

---

## Requirements

### Requirement 1: CompetencyV2 Schema

**User Story:** As a registry maintainer, I want a structured competency JSON schema that separates tool-agnostic and tool-specific artifacts, so that I can express per-tool variation without duplicating shared content.

#### Acceptance Criteria

1. THE CompetencyLoader SHALL accept competency JSON files that contain a top-level `schemaVersion` field with value `2`, a `shared` object, and a `tools` object.
2. THE CompetencyLoader SHALL treat all arrays within `shared` (`skills`, `mcpservers`, `agents`, `systems`, `olafdata`) and within each tool bundle (`rules`, `commands`, `hooks`) as optional, defaulting to empty arrays when absent.
3. THE CompetencyLoader SHALL treat the `powers` array as valid only under `tools.kiro`; when `powers` appears under any other tool key, THE CompetencyLoader SHALL emit a warning and ignore the field.
4. WHEN a competency JSON file is parsed successfully, THE CompetencyLoader SHALL produce a `CompetencyV2` value whose `name` and `description` fields are non-empty strings.
5. IF a competency JSON file is missing the `name` or `description` field, THEN THE CompetencyLoader SHALL return a descriptive parse error identifying the file path.

---

### Requirement 2: Registry Folder Layout

**User Story:** As a registry maintainer, I want well-defined top-level folders for rules, commands, and hooks, so that tool-specific artifacts have a predictable location in the registry.

#### Acceptance Criteria

1. THE registry SHALL organise tool-specific rule artifacts under the path `rules/<scope>/<tool>/<id>/`, where `<scope>` is `global` or `repo` and `<tool>` is a recognised ToolId.
2. THE registry SHALL organise tool-specific command artifacts under the path `commands/<scope>/<tool>/<id>/`, following the same `<scope>/<tool>` convention.
3. THE registry SHALL organise tool-specific hook artifacts under the path `hooks/<tool>/<id>/`.
4. THE registry SHALL retain the existing `skills/`, `mcpservers/`, `agents/`, `systems/`, and `powers/` top-level folders unchanged.
5. THE registry SHALL retain the `powers/` top-level folder for backward compatibility; the folder is deprecated but not removed.

---

### Requirement 3: CompetencyLoader — Parsing and Validation

**User Story:** As an installer developer, I want the CompetencyLoader to reliably parse and validate competency files, so that malformed or incomplete files produce clear errors rather than silent failures.

#### Acceptance Criteria

1. WHEN a competency JSON file is syntactically valid and contains all required fields, THE CompetencyLoader SHALL return a `CompetencyV2` value.
2. WHEN a competency JSON file contains a JSON syntax error, THE CompetencyLoader SHALL return an error that includes the file path and a description of the parse failure.
3. WHEN a competency JSON file is missing the `name` or `description` field, THE CompetencyLoader SHALL return an error identifying the missing field and the file path.
4. WHEN a competency JSON file contains unknown tool keys under `tools`, THE CompetencyLoader SHALL silently ignore those keys and continue parsing the recognised tool entries.
5. WHEN a collection references multiple competency files and one file fails to parse, THE CompetencyLoader SHALL skip that competency and continue loading the remaining competencies without aborting.
6. THE CompetencyLoader SHALL validate that all artifact ID strings within `shared` and `tools` arrays are non-empty; IF an empty string is encountered, THEN THE CompetencyLoader SHALL return a validation error.

---

### Requirement 4: Backward Compatibility — Legacy Normalisation

**User Story:** As an installer developer, I want existing v1 competency files to continue working without modification, so that the migration to v2 can happen incrementally.

#### Acceptance Criteria

1. WHEN a competency JSON file has no `schemaVersion` field or has `schemaVersion: 1`, THE CompetencyLoader SHALL normalise it into a `CompetencyV2` value.
2. WHEN normalising a legacy competency, THE CompetencyLoader SHALL map the top-level `skills` array to `shared.skills`.
3. WHEN normalising a legacy competency, THE CompetencyLoader SHALL map the top-level `powers` array to `tools.kiro.powers`.
4. WHEN normalising a legacy competency, THE CompetencyLoader SHALL set `tools.kiro.rules`, `tools.kiro.commands`, and `tools.kiro.hooks` to empty arrays.
5. WHEN normalising a legacy competency that has no `skills` field, THE CompetencyLoader SHALL set `shared.skills` to an empty array.
6. WHEN normalising a legacy competency that has no `powers` field, THE CompetencyLoader SHALL set `tools.kiro.powers` to an empty array.
7. WHEN a collection contains a mix of v1 and v2 competency files, THE CompetencyLoader SHALL normalise each file independently without error.

---

### Requirement 5: DestinationResolver — Artifact Resolution

**User Story:** As an installer developer, I want the DestinationResolver to resolve all artifact types referenced in a competency for the user's selected tools, so that the correct files are installed.

#### Acceptance Criteria

1. WHEN resolving a competency for a set of selected tools, THE DestinationResolver SHALL include install actions for all shared artifacts (`skills`, `mcpservers`, `agents`, `systems`, `olafdata`) regardless of which tools are selected.
2. WHEN resolving a competency for a set of selected tools, THE DestinationResolver SHALL include install actions only for the tool bundles of the selected tools.
3. WHEN a tool is not in the user's selected tool set, THE DestinationResolver SHALL silently skip that tool's bundle.
4. WHEN no tools are selected, THE DestinationResolver SHALL produce zero install actions for tool-specific artifacts; shared artifacts SHALL still be resolved.
5. WHEN resolving a `tools.kiro` bundle, THE DestinationResolver SHALL resolve `powers`, `rules`, `commands`, and `hooks` entries.
6. WHEN resolving a non-Kiro tool bundle, THE DestinationResolver SHALL resolve `rules`, `commands`, and `hooks` entries.
7. WHEN an artifact ID referenced in a competency does not exist in the registry, THE DestinationResolver SHALL log a warning that includes the competency name and the missing artifact ID, skip that artifact, and continue resolving the remaining artifacts.
8. WHEN all artifact IDs in a competency are valid, THE DestinationResolver SHALL produce an `InstallAction` for each artifact.

---

### Requirement 6: Security — Path Traversal Prevention

**User Story:** As a security-conscious developer, I want the installer to reject registry paths containing path traversal sequences, so that a malicious competency file cannot read or write files outside the registry root.

#### Acceptance Criteria

1. WHEN a registry path in a competency's `tools` section contains the sequence `..`, THE DestinationResolver SHALL reject that path and return an error.
2. WHEN a registry path in a competency's `shared` section contains the sequence `..`, THE DestinationResolver SHALL reject that path and return an error.
3. WHEN a registry path does not contain `..`, THE DestinationResolver SHALL accept and resolve the path normally.
4. IF a path traversal sequence is detected, THEN THE DestinationResolver SHALL not install any artifact from that path and SHALL log the rejection with the offending path.

---

### Requirement 7: haal_manifest.json Update

**User Story:** As a registry consumer, I want the manifest to declare the competency schema version in use, so that tooling can detect and handle the format correctly.

#### Acceptance Criteria

1. THE `haal_manifest.json` file SHALL contain a top-level `competencySchemaVersion` field with integer value `2`.
2. THE `haal_manifest.json` file SHALL retain all existing fields unchanged.

---

### Requirement 8: Install Action Monotonicity

**User Story:** As an installer developer, I want the set of install actions for a single tool to be a subset of the install actions for all tools, so that selecting more tools never removes artifacts.

#### Acceptance Criteria

1. FOR ANY competency and any single ToolId T, the set of install actions produced when only T is selected SHALL be a subset of the install actions produced when all tools are selected.
2. FOR ANY competency, adding more tools to the selected set SHALL not remove any install action that was already present for the previously selected tools.

---

### Requirement 9: Non-Functional — Performance

**User Story:** As an installer developer, I want competency loading to be fast enough that it does not noticeably delay startup, so that the user experience is not degraded.

#### Acceptance Criteria

1. WHEN loading and normalising all competency files in a collection at startup, THE CompetencyLoader SHALL complete within 500 ms for collections containing up to 100 competency files of up to 5 KB each.
2. THE CompetencyLoader SHALL not require a caching layer for competency files.

---

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system — essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

### Property 1: V2 Round-Trip Serialisation

*For any* valid `CompetencyV2` object, serialising it to JSON and then parsing it back with `CompetencyLoader` SHALL produce an equivalent `CompetencyV2` value.

**Validates: Requirements 1.1, 3.1**

---

### Property 2: Legacy Normalisation — Skills Mapping

*For any* valid legacy (v1) competency object, `normaliseLegacy(c).shared.skills` SHALL equal `c.skills` (or `[]` when `c.skills` is absent).

**Validates: Requirements 4.2, 4.5**

---

### Property 3: Legacy Normalisation — Powers Mapping

*For any* valid legacy (v1) competency object, `normaliseLegacy(c).tools.kiro.powers` SHALL equal `c.powers` (or `[]` when `c.powers` is absent).

**Validates: Requirements 4.3, 4.6**

---

### Property 4: Legacy Normalisation — Empty Tool Arrays

*For any* valid legacy (v1) competency object, `normaliseLegacy(c).tools.kiro.rules`, `.commands`, and `.hooks` SHALL all be empty arrays.

**Validates: Requirements 4.4**

---

### Property 5: Unknown Tool Keys Are Ignored

*For any* competency JSON object that contains additional unknown keys under `tools` alongside valid ToolId keys, parsing it SHALL produce the same `CompetencyV2` value as parsing the same object with the unknown keys removed.

**Validates: Requirements 3.4**

---

### Property 6: No Tools Selected Yields No Tool-Specific Actions

*For any* competency, resolving install actions with an empty selected-tools set SHALL produce zero tool-specific install actions.

**Validates: Requirements 5.4**

---

### Property 7: Single-Tool Actions Are a Subset of All-Tools Actions

*For any* competency and any ToolId T, the set of install actions produced when only T is selected SHALL be a subset of the install actions produced when all tools are selected.

**Validates: Requirements 8.1, 8.2**

---

### Property 8: Path Traversal Rejection

*For any* registry path string that contains the subsequence `..`, the DestinationResolver SHALL reject it; *for any* registry path string that does not contain `..`, the DestinationResolver SHALL not reject it on security grounds.

**Validates: Requirements 6.1, 6.2, 6.3**

---

### Property 9: Unknown Artifact IDs Do Not Abort Resolution

*For any* competency that contains one or more artifact IDs that do not exist in the registry, the DestinationResolver SHALL still produce install actions for all artifact IDs that do exist, and SHALL emit exactly one warning per missing ID.

**Validates: Requirements 5.7, 5.8**

---

### Property 10: Malformed JSON Returns Error, Not Panic

*For any* byte sequence that is not valid JSON, `CompetencyLoader` SHALL return an error value (not panic or crash), and the error SHALL include a non-empty description.

**Validates: Requirements 3.2**
