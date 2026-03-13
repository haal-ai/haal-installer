use std::collections::HashMap;
use std::path::Path;

use crate::errors::{FileSystemError, HaalError, ValidationError};
use crate::models::{CompetencyShared, CompetencyToolBundle, CompetencyV2};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load and normalise a competency JSON file from disk.
///
/// - If the file cannot be read, returns `HaalError::FileSystem`.
/// - If the JSON is malformed or required fields are missing, returns `HaalError::Validation`.
/// - Schema version 2 with `shared`/`tools` present → validated and returned as-is.
/// - Everything else → normalised from legacy format.
pub fn load_competency(path: &Path) -> Result<CompetencyV2, HaalError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        HaalError::FileSystem(FileSystemError {
            message: format!("Failed to read '{}': {}", path.display(), e),
            path: Some(path.display().to_string()),
        })
    })?;

    load_competency_from_str(&content, path)
}

/// Load and normalise a competency from a JSON string (useful for testing).
pub fn load_competency_from_str(content: &str, path: &Path) -> Result<CompetencyV2, HaalError> {
    let raw: serde_json::Value =
        serde_json::from_str(content).map_err(|e| {
            HaalError::Validation(ValidationError {
                message: format!(
                    "JSON parse error in '{}': {}",
                    path.display(),
                    e
                ),
                field: None,
            })
        })?;

    let schema_version = raw.get("schemaVersion").and_then(|v| v.as_u64());
    let has_shared = raw.get("shared").is_some();
    let has_tools = raw.get("tools").is_some();

    let competency = if schema_version == Some(2) && has_shared && has_tools {
        // Parse as v2 directly
        let mut c: CompetencyV2 =
            serde_json::from_value(raw.clone()).map_err(|e| {
                HaalError::Validation(ValidationError {
                    message: format!(
                        "Failed to parse v2 competency '{}': {}",
                        path.display(),
                        e
                    ),
                    field: None,
                })
            })?;

        // Warn about powers under non-kiro tool keys and strip them
        if let Some(ref mut tools) = c.tools {
            for (tool_key, bundle) in tools.iter_mut() {
                if tool_key != "kiro" && !bundle.powers.is_empty() {
                    eprintln!(
                        "WARNING: 'powers' field under tools.{} in '{}' is only valid for \
                         tools.kiro — ignoring",
                        tool_key,
                        path.display()
                    );
                    bundle.powers.clear();
                }
            }
        }

        c
    } else {
        normalise_legacy(raw)
    };

    validate_competency(&competency, path)?;
    Ok(competency)
}

// ---------------------------------------------------------------------------
// Legacy normalisation
// ---------------------------------------------------------------------------

/// Convert a raw legacy (v1 or no schemaVersion) JSON value into `CompetencyV2`.
///
/// Mapping:
/// - `skills`  → `shared.skills`
/// - `powers`  → `tools["kiro"].powers`
/// - `tools["kiro"].rules/commands/hooks` → empty vecs
/// - Absent `skills` or `powers` → default to `[]`
pub fn normalise_legacy(raw: serde_json::Value) -> CompetencyV2 {
    let name = raw
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let description = raw
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let skills: Vec<String> = raw
        .get("skills")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let powers: Vec<String> = raw
        .get("powers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let shared = CompetencyShared {
        skills,
        mcpservers: vec![],
        agents: vec![],
        systems: vec![],
        olafdata: vec![],
    };

    let kiro_bundle = CompetencyToolBundle {
        powers,
        rules: vec![],
        commands: vec![],
        hooks: vec![],
    };

    let mut tools = HashMap::new();
    tools.insert("kiro".to_string(), kiro_bundle);

    CompetencyV2 {
        name,
        description,
        schema_version: Some(1),
        shared: Some(shared),
        tools: Some(tools),
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate_competency(c: &CompetencyV2, path: &Path) -> Result<(), HaalError> {
    // name must be non-empty
    if c.name.trim().is_empty() {
        return Err(HaalError::Validation(ValidationError {
            message: format!(
                "Missing or empty 'name' field in '{}'",
                path.display()
            ),
            field: Some("name".to_string()),
        }));
    }

    // description must be non-empty
    if c.description.trim().is_empty() {
        return Err(HaalError::Validation(ValidationError {
            message: format!(
                "Missing or empty 'description' field in '{}'",
                path.display()
            ),
            field: Some("description".to_string()),
        }));
    }

    // All artifact ID strings must be non-empty
    if let Some(shared) = &c.shared {
        validate_string_vec(&shared.skills, "shared.skills", path)?;
        validate_string_vec(&shared.mcpservers, "shared.mcpservers", path)?;
        validate_string_vec(&shared.agents, "shared.agents", path)?;
        validate_string_vec(&shared.systems, "shared.systems", path)?;
        validate_string_vec(&shared.olafdata, "shared.olafdata", path)?;
    }

    if let Some(tools) = &c.tools {
        for (tool_key, bundle) in tools {
            validate_string_vec(&bundle.powers, &format!("tools.{}.powers", tool_key), path)?;
            validate_string_vec(&bundle.rules, &format!("tools.{}.rules", tool_key), path)?;
            validate_string_vec(&bundle.commands, &format!("tools.{}.commands", tool_key), path)?;
            validate_string_vec(&bundle.hooks, &format!("tools.{}.hooks", tool_key), path)?;
        }
    }

    Ok(())
}

fn validate_string_vec(vec: &[String], field: &str, path: &Path) -> Result<(), HaalError> {
    for s in vec {
        if s.trim().is_empty() {
            return Err(HaalError::Validation(ValidationError {
                message: format!(
                    "Empty artifact ID found in '{}' field in '{}'",
                    field,
                    path.display()
                ),
                field: Some(field.to_string()),
            }));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    // -----------------------------------------------------------------------
    // Property 1: V2 Round-Trip Serialisation
    // Validates: Requirements 1.1, 3.1
    // -----------------------------------------------------------------------
    proptest! {
        #[test]
        fn prop_v2_round_trip(
            name in "[a-z][a-z0-9-]{1,20}",
            description in "[a-zA-Z ]{5,50}",
            skills in prop::collection::vec("[a-z][a-z0-9-]{1,20}", 0..5),
        ) {
            let competency = CompetencyV2 {
                name: name.clone(),
                description: description.clone(),
                schema_version: Some(2),
                shared: Some(CompetencyShared {
                    skills: skills.clone(),
                    mcpservers: vec![],
                    agents: vec![],
                    systems: vec![],
                    olafdata: vec![],
                }),
                tools: Some(HashMap::new()),
            };
            let json = serde_json::to_string(&competency).unwrap();
            let path = Path::new("test.json");
            let result = load_competency_from_str(&json, path).unwrap();
            prop_assert_eq!(result.name, name);
            prop_assert_eq!(result.shared.unwrap().skills, skills);
        }
    }

    // -----------------------------------------------------------------------
    // Property 2: Legacy Normalisation — Skills Mapping
    // Validates: Requirements 4.2, 4.5
    // -----------------------------------------------------------------------
    proptest! {
        #[test]
        fn prop_legacy_skills_mapping(
            name in "[a-z][a-z0-9-]{1,20}",
            description in "[a-zA-Z ]{5,50}",
            skills in prop::collection::vec("[a-z][a-z0-9-]{1,20}", 0..5),
        ) {
            let raw = serde_json::json!({
                "name": name,
                "description": description,
                "skills": skills,
            });
            let result = normalise_legacy(raw);
            prop_assert_eq!(result.shared.unwrap().skills, skills);
        }
    }

    // -----------------------------------------------------------------------
    // Property 3: Legacy Normalisation — Powers Mapping
    // Validates: Requirements 4.3, 4.6
    // -----------------------------------------------------------------------
    proptest! {
        #[test]
        fn prop_legacy_powers_mapping(
            name in "[a-z][a-z0-9-]{1,20}",
            description in "[a-zA-Z ]{5,50}",
            powers in prop::collection::vec("[a-z][a-z0-9-]{1,20}", 0..5),
        ) {
            let raw = serde_json::json!({
                "name": name,
                "description": description,
                "powers": powers,
            });
            let result = normalise_legacy(raw);
            let tools = result.tools.unwrap();
            let kiro = tools.get("kiro").unwrap();
            prop_assert_eq!(kiro.powers.clone(), powers);
        }
    }

    // -----------------------------------------------------------------------
    // Property 4: Legacy Normalisation — Empty Tool Arrays
    // Validates: Requirements 4.4
    // -----------------------------------------------------------------------
    proptest! {
        #[test]
        fn prop_legacy_empty_tool_arrays(
            name in "[a-z][a-z0-9-]{1,20}",
            description in "[a-zA-Z ]{5,50}",
        ) {
            let raw = serde_json::json!({
                "name": name,
                "description": description,
            });
            let result = normalise_legacy(raw);
            let tools = result.tools.unwrap();
            let kiro = tools.get("kiro").unwrap();
            prop_assert!(kiro.rules.is_empty());
            prop_assert!(kiro.commands.is_empty());
            prop_assert!(kiro.hooks.is_empty());
        }
    }

    // -----------------------------------------------------------------------
    // Property 5: Unknown Tool Keys Are Ignored
    // Validates: Requirements 3.4
    // -----------------------------------------------------------------------
    proptest! {
        #[test]
        fn prop_unknown_tool_keys_ignored(
            name in "[a-z][a-z0-9-]{1,20}",
            description in "[a-zA-Z ]{5,50}",
            skill in "[a-z][a-z0-9-]{1,20}",
        ) {
            let json_with_unknown = format!(r#"{{
                "name": "{}",
                "description": "{}",
                "schemaVersion": 2,
                "shared": {{ "skills": ["{}"] }},
                "tools": {{
                    "kiro": {{ "powers": [], "rules": [], "commands": [], "hooks": [] }},
                    "unknown-tool-xyz": {{ "rules": [], "commands": [], "hooks": [] }}
                }}
            }}"#, name, description, skill);
            let json_without_unknown = format!(r#"{{
                "name": "{}",
                "description": "{}",
                "schemaVersion": 2,
                "shared": {{ "skills": ["{}"] }},
                "tools": {{
                    "kiro": {{ "powers": [], "rules": [], "commands": [], "hooks": [] }}
                }}
            }}"#, name, description, skill);
            let path = Path::new("test.json");
            let r1 = load_competency_from_str(&json_with_unknown, path).unwrap();
            let r2 = load_competency_from_str(&json_without_unknown, path).unwrap();
            prop_assert_eq!(r1.shared.unwrap().skills, r2.shared.unwrap().skills);
        }
    }

    // -----------------------------------------------------------------------
    // Property 10: Malformed JSON Returns Error, Not Panic
    // Validates: Requirements 3.2
    // -----------------------------------------------------------------------
    proptest! {
        #[test]
        fn prop_malformed_json_returns_error_not_panic(
            garbage in "[ \t\n{}\",:\\[\\]a-zA-Z0-9]{1,50}",
        ) {
            // Only test strings that are NOT valid JSON objects with name+description
            // We just want to ensure no panic
            let path = Path::new("test.json");
            let _ = load_competency_from_str(&garbage, path); // must not panic
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("tempfile");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    // -----------------------------------------------------------------------
    // 2.6 unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn load_v2_competency_returns_competency_v2() {
        let json = r#"{
            "name": "developer",
            "description": "Developer competency",
            "schemaVersion": 2,
            "shared": {
                "skills": ["review-code", "fix-code-smells"],
                "mcpservers": [],
                "agents": [],
                "systems": [],
                "olafdata": []
            },
            "tools": {
                "kiro": {
                    "powers": ["code-in-go"],
                    "rules": [],
                    "commands": [],
                    "hooks": []
                }
            }
        }"#;
        let f = write_temp(json);
        let result = load_competency(f.path()).expect("should succeed");
        assert_eq!(result.name, "developer");
        assert_eq!(result.schema_version, Some(2));
        let shared = result.shared.expect("shared present");
        assert_eq!(shared.skills, vec!["review-code", "fix-code-smells"]);
        let tools = result.tools.expect("tools present");
        let kiro = tools.get("kiro").expect("kiro present");
        assert_eq!(kiro.powers, vec!["code-in-go"]);
    }

    #[test]
    fn load_v1_competency_normalises_skills_and_powers() {
        let json = r#"{
            "name": "architect",
            "description": "Architect competency",
            "skills": ["design-patterns", "system-design"],
            "powers": ["code-in-rust"]
        }"#;
        let f = write_temp(json);
        let result = load_competency(f.path()).expect("should succeed");
        assert_eq!(result.name, "architect");
        let shared = result.shared.expect("shared present");
        assert_eq!(shared.skills, vec!["design-patterns", "system-design"]);
        let tools = result.tools.expect("tools present");
        let kiro = tools.get("kiro").expect("kiro present");
        assert_eq!(kiro.powers, vec!["code-in-rust"]);
        assert!(kiro.rules.is_empty());
        assert!(kiro.commands.is_empty());
        assert!(kiro.hooks.is_empty());
    }

    #[test]
    fn load_missing_skills_defaults_to_empty() {
        let json = r#"{
            "name": "minimal",
            "description": "Minimal competency",
            "powers": ["some-power"]
        }"#;
        let f = write_temp(json);
        let result = load_competency(f.path()).expect("should succeed");
        let shared = result.shared.expect("shared present");
        assert!(shared.skills.is_empty());
    }

    #[test]
    fn load_missing_powers_defaults_to_empty() {
        let json = r#"{
            "name": "minimal",
            "description": "Minimal competency",
            "skills": ["some-skill"]
        }"#;
        let f = write_temp(json);
        let result = load_competency(f.path()).expect("should succeed");
        let tools = result.tools.expect("tools present");
        let kiro = tools.get("kiro").expect("kiro present");
        assert!(kiro.powers.is_empty());
    }

    #[test]
    fn load_missing_name_returns_error() {
        let json = r#"{
            "description": "No name here",
            "skills": []
        }"#;
        let f = write_temp(json);
        let err = load_competency(f.path()).expect_err("should fail");
        match err {
            HaalError::Validation(e) => {
                assert!(e.message.contains("name"), "error should mention 'name': {}", e.message);
                assert_eq!(e.field.as_deref(), Some("name"));
            }
            other => panic!("expected ValidationError, got {:?}", other),
        }
    }

    #[test]
    fn load_missing_description_returns_error() {
        let json = r#"{
            "name": "no-description",
            "skills": []
        }"#;
        let f = write_temp(json);
        let err = load_competency(f.path()).expect_err("should fail");
        match err {
            HaalError::Validation(e) => {
                assert!(e.message.contains("description"), "error should mention 'description': {}", e.message);
                assert_eq!(e.field.as_deref(), Some("description"));
            }
            other => panic!("expected ValidationError, got {:?}", other),
        }
    }

    #[test]
    fn load_invalid_json_returns_error_with_path() {
        let f = write_temp("{ this is not valid json }");
        let err = load_competency(f.path()).expect_err("should fail");
        match err {
            HaalError::Validation(e) => {
                let path_str = f.path().display().to_string();
                assert!(
                    e.message.contains(&path_str),
                    "error should contain file path '{}': {}",
                    path_str,
                    e.message
                );
            }
            other => panic!("expected ValidationError, got {:?}", other),
        }
    }

    #[test]
    fn load_empty_artifact_id_returns_error() {
        let json = r#"{
            "name": "bad-competency",
            "description": "Has empty skill ID",
            "schemaVersion": 2,
            "shared": {
                "skills": ["valid-skill", ""]
            },
            "tools": {}
        }"#;
        let f = write_temp(json);
        let err = load_competency(f.path()).expect_err("should fail");
        match err {
            HaalError::Validation(e) => {
                assert!(
                    e.message.contains("Empty artifact ID"),
                    "error should mention empty artifact ID: {}",
                    e.message
                );
            }
            other => panic!("expected ValidationError, got {:?}", other),
        }
    }

    #[test]
    fn load_powers_under_non_kiro_tool_emits_warning_and_ignores() {
        let json = r#"{
            "name": "multi-tool",
            "description": "Has powers under cursor",
            "schemaVersion": 2,
            "shared": { "skills": [] },
            "tools": {
                "kiro": { "powers": ["valid-power"], "rules": [], "commands": [], "hooks": [] },
                "cursor": { "powers": ["should-be-ignored"], "rules": [], "commands": [], "hooks": [] }
            }
        }"#;
        let f = write_temp(json);
        // Should succeed (warning only, no error)
        let result = load_competency(f.path()).expect("should succeed despite cursor powers");
        let tools = result.tools.expect("tools present");
        // cursor powers should be cleared
        let cursor = tools.get("cursor").expect("cursor present");
        assert!(
            cursor.powers.is_empty(),
            "cursor powers should be cleared, got: {:?}",
            cursor.powers
        );
        // kiro powers should be intact
        let kiro = tools.get("kiro").expect("kiro present");
        assert_eq!(kiro.powers, vec!["valid-power"]);
    }

    #[test]
    fn load_unknown_tool_key_is_silently_ignored() {
        let json = r#"{
            "name": "future-tool",
            "description": "Has unknown tool vscode",
            "schemaVersion": 2,
            "shared": { "skills": ["some-skill"] },
            "tools": {
                "kiro": { "powers": [], "rules": [], "commands": [], "hooks": [] },
                "vscode": { "rules": ["some-rule"], "commands": [], "hooks": [] }
            }
        }"#;
        let f = write_temp(json);
        // Should succeed — unknown tool keys are silently ignored via HashMap
        let result = load_competency(f.path()).expect("should succeed with unknown tool key");
        let tools = result.tools.expect("tools present");
        // vscode key is present in the HashMap (silently accepted)
        assert!(tools.contains_key("kiro"));
        // The shared skills are intact
        let shared = result.shared.expect("shared present");
        assert_eq!(shared.skills, vec!["some-skill"]);
    }

    // -----------------------------------------------------------------------
    // End-to-end: load the actual developer.json from haal-skills
    // -----------------------------------------------------------------------

    #[test]
    fn load_developer_json_end_to_end() {
        // Path relative to src-tauri/ (where `cargo test` runs)
        let path = Path::new("../haal-skills/competencies/developer.json");
        if !path.exists() {
            // Skip gracefully if the haal-skills submodule is not present
            eprintln!("SKIP: developer.json not found at {:?}", path);
            return;
        }

        let result = load_competency(path).expect("developer.json should parse successfully");

        // Basic fields
        assert_eq!(result.name, "developer");
        assert!(!result.description.is_empty(), "description should be non-empty");
        assert_eq!(result.schema_version, Some(2));

        // shared.skills — verify a representative subset
        let shared = result.shared.expect("shared section should be present");
        let expected_skills = [
            "review-code",
            "fix-code-smells",
            "augment-code-unit-test",
            "improve-cyclomatic-complexity",
        ];
        for skill in &expected_skills {
            assert!(
                shared.skills.contains(&skill.to_string()),
                "shared.skills should contain '{}', got: {:?}",
                skill,
                shared.skills
            );
        }
        assert!(!shared.skills.is_empty(), "shared.skills should not be empty");

        // tools.kiro.powers
        let tools = result.tools.expect("tools section should be present");
        let kiro = tools.get("kiro").expect("tools.kiro should be present");
        let expected_powers = ["code-in-go", "code-in-rust", "code-microservice-in-quarkus"];
        for power in &expected_powers {
            assert!(
                kiro.powers.contains(&power.to_string()),
                "tools.kiro.powers should contain '{}', got: {:?}",
                power,
                kiro.powers
            );
        }
        assert_eq!(kiro.powers.len(), 3, "tools.kiro.powers should have exactly 3 entries");

        // tools.kiro rules/commands/hooks should be empty arrays
        assert!(kiro.rules.is_empty(), "tools.kiro.rules should be empty");
        assert!(kiro.commands.is_empty(), "tools.kiro.commands should be empty");
        assert!(kiro.hooks.is_empty(), "tools.kiro.hooks should be empty");

        // Other tool bundles should be present (cursor, windsurf, claude, copilot)
        for tool in &["cursor", "windsurf", "claude", "copilot"] {
            assert!(
                tools.contains_key(*tool),
                "tools.{} should be present",
                tool
            );
        }
    }

    #[test]
    fn load_mixed_v1_v2_collection_normalises_independently() {
        let v1_json = r#"{
            "name": "v1-competency",
            "description": "Legacy format",
            "skills": ["skill-a"],
            "powers": ["power-a"]
        }"#;
        let v2_json = r#"{
            "name": "v2-competency",
            "description": "New format",
            "schemaVersion": 2,
            "shared": { "skills": ["skill-b"] },
            "tools": {
                "kiro": { "powers": ["power-b"], "rules": [], "commands": [], "hooks": [] }
            }
        }"#;

        let f1 = write_temp(v1_json);
        let f2 = write_temp(v2_json);

        let r1 = load_competency(f1.path()).expect("v1 should succeed");
        let r2 = load_competency(f2.path()).expect("v2 should succeed");

        // v1 normalised correctly
        assert_eq!(r1.name, "v1-competency");
        let s1 = r1.shared.expect("v1 shared");
        assert_eq!(s1.skills, vec!["skill-a"]);
        let t1 = r1.tools.expect("v1 tools");
        assert_eq!(t1["kiro"].powers, vec!["power-a"]);

        // v2 parsed correctly
        assert_eq!(r2.name, "v2-competency");
        let s2 = r2.shared.expect("v2 shared");
        assert_eq!(s2.skills, vec!["skill-b"]);
        let t2 = r2.tools.expect("v2 tools");
        assert_eq!(t2["kiro"].powers, vec!["power-b"]);
    }
}
