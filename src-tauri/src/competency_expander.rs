use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::competency_loader::load_competency;
use crate::errors::HaalError;
use crate::models::{ComponentType, ResolvedComponent};

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Tries to add a component with `source_path = registry_root/subdir/id`.
/// Returns `Some(ResolvedComponent)` if the path exists, `None` (with warning) otherwise.
fn try_make_component(
    competency_name: &str,
    id: &str,
    ctype: ComponentType,
    source_path: PathBuf,
) -> Option<ResolvedComponent> {
    if !source_path.exists() {
        eprintln!(
            "WARNING: competency '{}' references unknown artifact '{}' (expected at '{}') — skipping",
            competency_name,
            id,
            source_path.display()
        );
        return None;
    }
    Some(ResolvedComponent {
        id: id.to_string(),
        component_type: ctype,
        source_path,
    })
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Expands a competency JSON file into a list of `ResolvedComponent` items
/// using `CompetencyLoader::load_competency`.
///
/// - `competency_path`: absolute path to the competency JSON file on disk.
/// - `registry_root`: root of the cloned registry repo (used to build `source_path`).
/// - `selected_tools`: tool IDs the user selected (e.g. `["kiro"]`).
///
/// Rules:
/// - `shared.*` artifacts → always included (tool-agnostic).
/// - `tools[tool].*` → only included when `tool` is in `selected_tools`.
/// - Tools not in `selected_tools` → skipped entirely.
/// - Artifact IDs whose source path does not exist on disk → warning logged, skipped.
/// - Duplicate artifact IDs (same type + id) → deduplicated.
pub fn expand_competency(
    competency_path: &Path,
    registry_root: &Path,
    selected_tools: &[String],
) -> Result<Vec<ResolvedComponent>, HaalError> {
    let competency = load_competency(competency_path)?;
    let competency_name = competency.name.clone();

    let mut components: Vec<ResolvedComponent> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let selected: HashSet<&str> = selected_tools.iter().map(|s| s.as_str()).collect();

    // -----------------------------------------------------------------------
    // Shared artifacts (tool-agnostic — always included)
    // -----------------------------------------------------------------------
    if let Some(shared) = &competency.shared {
        let pairs: &[(&[String], ComponentType, &str)] = &[
            (&shared.skills,     ComponentType::Skill,     "skills"),
            (&shared.mcpservers, ComponentType::McpServer, "mcpservers"),
            (&shared.agents,     ComponentType::Agent,     "agents"),
            (&shared.systems,    ComponentType::System,    "systems"),
            (&shared.olafdata,   ComponentType::OlafData,  "olafdata"),
        ];
        for (ids, ctype, subdir) in pairs {
            for id in *ids {
                let key = format!("{:?}:{}", ctype, id);
                if seen.contains(&key) { continue; }
                let src = registry_root.join(subdir).join(id);
                if let Some(comp) = try_make_component(&competency_name, id, ctype.clone(), src) {
                    seen.insert(key);
                    components.push(comp);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Tool bundles (only for selected tools)
    // -----------------------------------------------------------------------
    if let Some(tools) = &competency.tools {
        for (tool_key, bundle) in tools {
            if !selected.contains(tool_key.as_str()) {
                // Tool not selected — skip bundle entirely (4.9)
                continue;
            }

            // Powers: kiro only (loader already cleared powers on non-kiro tools)
            if tool_key == "kiro" {
                for id in &bundle.powers {
                    let key = format!("{:?}:{}", ComponentType::Power, id);
                    if seen.contains(&key) { continue; }
                    let src = registry_root.join("powers").join(id);
                    if let Some(comp) = try_make_component(&competency_name, id, ComponentType::Power, src) {
                        seen.insert(key);
                        components.push(comp);
                    }
                }
            }

            // Rules/commands/hooks: the ID is the full registry-relative path
            // (e.g. "rules/repo/kiro/developer-guidelines")
            let path_pairs: &[(&[String], ComponentType)] = &[
                (&bundle.rules,    ComponentType::Rule),
                (&bundle.commands, ComponentType::Command),
                (&bundle.hooks,    ComponentType::Hook),
            ];
            for (ids, ctype) in path_pairs {
                for id in *ids {
                    let key = format!("{:?}:{}", ctype, id);
                    if seen.contains(&key) { continue; }
                    // Normalise path separators: the ID uses '/' but we need OS separators
                    let rel: PathBuf = id.split('/').collect();
                    let src = registry_root.join(&rel);
                    if let Some(comp) = try_make_component(&competency_name, id, ctype.clone(), src) {
                        seen.insert(key);
                        components.push(comp);
                    }
                }
            }
        }
    }

    Ok(components)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;
    use std::io::Write;

    // -----------------------------------------------------------------------
    // Property 6: No Tools Selected Yields No Tool-Specific Actions
    // Validates: Requirements 5.4
    // -----------------------------------------------------------------------
    proptest! {
        #[test]
        fn prop_no_tools_selected_zero_tool_actions(
            name in "[a-z][a-z0-9-]{1,20}",
            description in "[a-zA-Z ]{5,50}",
            power in "[a-z][a-z0-9-]{1,20}",
        ) {
            use tempfile::tempdir;
            let registry = tempdir().unwrap();
            // Create the power directory
            std::fs::create_dir_all(registry.path().join("powers").join(&power)).unwrap();

            let json = format!(r#"{{
                "name": "{}",
                "description": "{}",
                "schemaVersion": 2,
                "shared": {{ "skills": [] }},
                "tools": {{
                    "kiro": {{ "powers": ["{}"], "rules": [], "commands": [], "hooks": [] }}
                }}
            }}"#, name, description, power);

            let mut f = tempfile::NamedTempFile::new().unwrap();
            f.write_all(json.as_bytes()).unwrap();

            // No tools selected
            let components = expand_competency(f.path(), registry.path(), &[]).unwrap();
            let tool_specific: Vec<_> = components.iter()
                .filter(|c| c.component_type == crate::models::ComponentType::Power)
                .collect();
            prop_assert!(tool_specific.is_empty(), "no tool-specific actions when no tools selected");
        }
    }

    // -----------------------------------------------------------------------
    // Property 7: Single-Tool Actions Are a Subset of All-Tools Actions
    // Validates: Requirements 8.1, 8.2
    // -----------------------------------------------------------------------
    proptest! {
        #[test]
        fn prop_single_tool_actions_subset_of_all_tools(
            name in "[a-z][a-z0-9-]{1,20}",
            description in "[a-zA-Z ]{5,50}",
            skill in "[a-z][a-z0-9-]{1,20}",
            power in "[a-z][a-z0-9-]{1,20}",
        ) {
            use tempfile::tempdir;
            let registry = tempdir().unwrap();
            std::fs::create_dir_all(registry.path().join("skills").join(&skill)).unwrap();
            std::fs::create_dir_all(registry.path().join("powers").join(&power)).unwrap();

            let json = format!(r#"{{
                "name": "{}",
                "description": "{}",
                "schemaVersion": 2,
                "shared": {{ "skills": ["{}"] }},
                "tools": {{
                    "kiro": {{ "powers": ["{}"], "rules": [], "commands": [], "hooks": [] }},
                    "cursor": {{ "rules": [], "commands": [], "hooks": [] }}
                }}
            }}"#, name, description, skill, power);

            let mut f = tempfile::NamedTempFile::new().unwrap();
            f.write_all(json.as_bytes()).unwrap();

            let kiro_only = expand_competency(f.path(), registry.path(), &["kiro".to_string()]).unwrap();
            let all_tools = expand_competency(f.path(), registry.path(), &["kiro".to_string(), "cursor".to_string()]).unwrap();

            // Build sets of (type, id) for comparison
            let kiro_set: std::collections::HashSet<_> = kiro_only.iter()
                .map(|c| (format!("{:?}", c.component_type), c.id.clone()))
                .collect();
            let all_set: std::collections::HashSet<_> = all_tools.iter()
                .map(|c| (format!("{:?}", c.component_type), c.id.clone()))
                .collect();

            // Every action from kiro-only must appear in all-tools
            for item in &kiro_set {
                prop_assert!(all_set.contains(item),
                    "kiro-only action {:?} missing from all-tools set", item);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Property 9: Unknown Artifact IDs Do Not Abort Resolution
    // Validates: Requirements 5.7, 5.8
    // -----------------------------------------------------------------------
    proptest! {
        #[test]
        fn prop_unknown_artifact_ids_do_not_abort_resolution(
            name in "[a-z][a-z0-9-]{1,20}",
            description in "[a-zA-Z ]{5,50}",
            existing_skill in "[a-z][a-z0-9-]{1,20}",
            missing_skill in "[a-z][a-z0-9-]{1,20}",
        ) {
            // Ensure the two skill IDs are different to avoid ambiguity
            prop_assume!(existing_skill != missing_skill);

            use tempfile::tempdir;
            let registry = tempdir().unwrap();
            // Only create the existing skill directory
            std::fs::create_dir_all(registry.path().join("skills").join(&existing_skill)).unwrap();
            // missing_skill intentionally absent

            let json = format!(r#"{{
                "name": "{}",
                "description": "{}",
                "schemaVersion": 2,
                "shared": {{ "skills": ["{}", "{}"] }},
                "tools": {{}}
            }}"#, name, description, existing_skill, missing_skill);

            let mut f = tempfile::NamedTempFile::new().unwrap();
            f.write_all(json.as_bytes()).unwrap();

            // Resolution must succeed (not abort) even with a missing artifact
            let components = expand_competency(f.path(), registry.path(), &[]).unwrap();

            // The existing skill must be present
            let has_existing = components.iter().any(|c| c.id == existing_skill);
            prop_assert!(has_existing, "existing skill '{}' should be in result", existing_skill);

            // The missing skill must NOT be present
            let has_missing = components.iter().any(|c| c.id == missing_skill);
            prop_assert!(!has_missing, "missing skill '{}' should not be in result", missing_skill);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{tempdir, NamedTempFile};

    fn write_temp_competency(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("tempfile");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    /// Creates a minimal registry directory structure with the given artifact paths.
    fn create_registry(artifacts: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempdir().expect("tempdir");
        for (subdir, id) in artifacts {
            let path = dir.path().join(subdir).join(id);
            std::fs::create_dir_all(&path).expect("create artifact dir");
        }
        dir
    }

    // -----------------------------------------------------------------------
    // 4.11 Integration test: load a v2 competency fixture, resolve for ["kiro"],
    // assert expected InstallAction list
    // -----------------------------------------------------------------------

    #[test]
    fn integration_v2_competency_resolves_for_kiro() {
        // Build a registry with the expected artifact directories
        let registry = create_registry(&[
            ("skills",   "review-code"),
            ("skills",   "fix-code-smells"),
            ("mcpservers", "github-mcp"),
            ("powers",   "code-in-go"),
            ("powers",   "code-in-rust"),
        ]);
        let registry_root = registry.path();

        let json = r#"{
            "name": "developer",
            "description": "Developer competency",
            "schemaVersion": 2,
            "shared": {
                "skills": ["review-code", "fix-code-smells"],
                "mcpservers": ["github-mcp"],
                "agents": [],
                "systems": [],
                "olafdata": []
            },
            "tools": {
                "kiro": {
                    "powers": ["code-in-go", "code-in-rust"],
                    "rules": [],
                    "commands": [],
                    "hooks": []
                },
                "cursor": {
                    "rules": [],
                    "commands": [],
                    "hooks": []
                }
            }
        }"#;

        let f = write_temp_competency(json);
        let components = expand_competency(f.path(), registry_root, &["kiro".to_string()])
            .expect("expand should succeed");

        // Collect (type, id) pairs for assertion
        let result: Vec<(String, String)> = components
            .iter()
            .map(|c| (format!("{:?}", c.component_type), c.id.clone()))
            .collect();

        // Shared artifacts
        assert!(result.contains(&("Skill".to_string(), "review-code".to_string())),
            "missing review-code skill: {:?}", result);
        assert!(result.contains(&("Skill".to_string(), "fix-code-smells".to_string())),
            "missing fix-code-smells skill: {:?}", result);
        assert!(result.contains(&("McpServer".to_string(), "github-mcp".to_string())),
            "missing github-mcp mcpserver: {:?}", result);

        // Kiro-specific powers
        assert!(result.contains(&("Power".to_string(), "code-in-go".to_string())),
            "missing code-in-go power: {:?}", result);
        assert!(result.contains(&("Power".to_string(), "code-in-rust".to_string())),
            "missing code-in-rust power: {:?}", result);

        // Total: 2 skills + 1 mcpserver + 2 powers = 5
        assert_eq!(components.len(), 5, "expected 5 components, got: {:?}", result);
    }

    #[test]
    fn shared_artifacts_included_regardless_of_selected_tools() {
        let registry = create_registry(&[
            ("skills", "base-skill"),
        ]);
        let json = r#"{
            "name": "base",
            "description": "Base competency",
            "schemaVersion": 2,
            "shared": { "skills": ["base-skill"] },
            "tools": {
                "kiro": { "powers": [], "rules": [], "commands": [], "hooks": [] }
            }
        }"#;
        let f = write_temp_competency(json);

        // No tools selected — shared artifacts still resolved
        let components = expand_competency(f.path(), registry.path(), &[])
            .expect("expand should succeed");
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].id, "base-skill");
        assert_eq!(components[0].component_type, ComponentType::Skill);
    }

    #[test]
    fn non_selected_tool_bundle_is_skipped() {
        let registry = create_registry(&[
            ("skills", "shared-skill"),
            ("powers", "kiro-power"),
        ]);
        let json = r#"{
            "name": "multi",
            "description": "Multi-tool competency",
            "schemaVersion": 2,
            "shared": { "skills": ["shared-skill"] },
            "tools": {
                "kiro": { "powers": ["kiro-power"], "rules": [], "commands": [], "hooks": [] },
                "cursor": { "rules": [], "commands": [], "hooks": [] }
            }
        }"#;
        let f = write_temp_competency(json);

        // Only cursor selected — kiro powers should be skipped
        let components = expand_competency(f.path(), registry.path(), &["cursor".to_string()])
            .expect("expand should succeed");

        let has_power = components.iter().any(|c| c.component_type == ComponentType::Power);
        assert!(!has_power, "kiro powers should be skipped when cursor is selected");
        assert_eq!(components.len(), 1, "only shared skill expected");
    }

    #[test]
    fn unknown_artifact_id_is_skipped_with_warning() {
        let registry = create_registry(&[
            ("skills", "existing-skill"),
            // "missing-skill" intentionally absent
        ]);
        let json = r#"{
            "name": "partial",
            "description": "Partial competency",
            "schemaVersion": 2,
            "shared": { "skills": ["existing-skill", "missing-skill"] },
            "tools": {}
        }"#;
        let f = write_temp_competency(json);

        let components = expand_competency(f.path(), registry.path(), &[])
            .expect("expand should succeed despite missing artifact");

        // Only the existing skill should be in the result
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].id, "existing-skill");
    }

    #[test]
    fn duplicate_artifact_ids_are_deduplicated() {
        let registry = create_registry(&[
            ("skills", "review-code"),
        ]);
        let json = r#"{
            "name": "dedup",
            "description": "Dedup test",
            "schemaVersion": 2,
            "shared": { "skills": ["review-code", "review-code"] },
            "tools": {}
        }"#;
        let f = write_temp_competency(json);

        let components = expand_competency(f.path(), registry.path(), &[])
            .expect("expand should succeed");

        assert_eq!(components.len(), 1, "duplicate should be deduplicated");
    }

    #[test]
    fn legacy_v1_competency_is_normalised_and_expanded() {
        let registry = create_registry(&[
            ("skills", "design-patterns"),
            ("powers", "code-in-rust"),
        ]);
        let json = r#"{
            "name": "architect",
            "description": "Architect competency",
            "skills": ["design-patterns"],
            "powers": ["code-in-rust"]
        }"#;
        let f = write_temp_competency(json);

        let components = expand_competency(f.path(), registry.path(), &["kiro".to_string()])
            .expect("expand should succeed");

        let skill = components.iter().find(|c| c.component_type == ComponentType::Skill);
        let power = components.iter().find(|c| c.component_type == ComponentType::Power);
        assert!(skill.is_some(), "skill should be present");
        assert!(power.is_some(), "power should be present");
        assert_eq!(skill.unwrap().id, "design-patterns");
        assert_eq!(power.unwrap().id, "code-in-rust");
    }
}
