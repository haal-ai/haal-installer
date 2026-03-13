use std::path::{Path, PathBuf};

use crate::errors::{HaalError, ValidationError};
use crate::models::{Component, Manifest};

/// Parses JSON manifest files into `Manifest` structs with validation
/// and component path resolution.
pub struct ManifestParser;

impl ManifestParser {
    /// Parses a JSON string into a `Manifest`, validating required fields.
    pub fn parse(json: &str) -> Result<Manifest, HaalError> {
        let manifest: Manifest =
            serde_json::from_str(json).map_err(|e| ValidationError {
                message: format!("Invalid manifest JSON: {e}"),
                field: None,
            })?;

        Self::validate(&manifest)?;
        Ok(manifest)
    }

    /// Reads a manifest file from disk and parses it.
    pub fn parse_file(path: &Path) -> Result<Manifest, HaalError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            crate::errors::FileSystemError {
                message: format!("Failed to read manifest file '{}': {e}", path.display()),
                path: Some(path.display().to_string()),
            }
        })?;
        Self::parse(&content)
    }

    /// Parses a JSON string and resolves all component paths relative to
    /// `repo_root`.
    pub fn parse_with_root(json: &str, repo_root: &Path) -> Result<Manifest, HaalError> {
        let mut manifest = Self::parse(json)?;
        Self::resolve_paths(&mut manifest.components, repo_root);
        manifest
            .collections
            .iter()
            .for_each(|_| { /* collections reference component IDs, no paths to resolve */ });
        Ok(manifest)
    }

    /// Validates that a manifest has the required structure.
    fn validate(manifest: &Manifest) -> Result<(), HaalError> {
        if manifest.version.trim().is_empty() {
            return Err(ValidationError {
                message: "Manifest 'version' field must not be empty".into(),
                field: Some("version".into()),
            }
            .into());
        }

        if manifest.components.is_empty()
            && manifest.collections.is_empty()
            && manifest.competencies.is_empty()
        {
            return Err(ValidationError {
                message: "Manifest must contain at least one component, collection, or competency"
                    .into(),
                field: None,
            }
            .into());
        }

        // Validate each component has required fields
        for (i, comp) in manifest.components.iter().enumerate() {
            Self::validate_component(comp, i)?;
        }

        Ok(())
    }

    fn validate_component(comp: &Component, index: usize) -> Result<(), HaalError> {
        if comp.id.trim().is_empty() {
            return Err(ValidationError {
                message: format!("Component at index {index} has an empty 'id'"),
                field: Some(format!("components[{index}].id")),
            }
            .into());
        }
        if comp.name.trim().is_empty() {
            return Err(ValidationError {
                message: format!("Component '{}' has an empty 'name'", comp.id),
                field: Some(format!("components[{index}].name")),
            }
            .into());
        }
        Ok(())
    }

    /// Resolves each component's `path` relative to `repo_root`, replacing
    /// the relative path with the full absolute path.
    fn resolve_paths(components: &mut [Component], repo_root: &Path) {
        for comp in components.iter_mut() {
            let resolved: PathBuf = repo_root.join(&comp.path);
            comp.path = resolved.to_string_lossy().into_owned();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest_json() -> String {
        serde_json::json!({
            "version": "1.0",
            "components": [
                {
                    "id": "code-review",
                    "name": "Code Review Skill",
                    "description": "AI-assisted code review",
                    "componentType": "skill",
                    "path": "skills/code-review",
                    "compatibleTools": ["kiro", "cursor"],
                    "dependencies": [],
                    "pinned": false,
                    "deprecated": false
                }
            ],
            "collections": [
                {
                    "id": "starter-pack",
                    "name": "Starter Pack",
                    "description": "Essential skills",
                    "competencyIds": ["code-review"]
                }
            ],
            "competencies": []
        })
        .to_string()
    }

    #[test]
    fn parse_valid_manifest() {
        let manifest = ManifestParser::parse(&valid_manifest_json()).unwrap();
        assert_eq!(manifest.version, "1.0");
        assert_eq!(manifest.components.len(), 1);
        assert_eq!(manifest.components[0].id, "code-review");
        assert_eq!(manifest.collections.len(), 1);
    }

    #[test]
    fn parse_rejects_invalid_json() {
        let result = ManifestParser::parse("not json at all");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid manifest JSON"));
    }

    #[test]
    fn parse_rejects_empty_version() {
        let json = serde_json::json!({
            "version": "  ",
            "components": [{
                "id": "a", "name": "A", "description": "",
                "componentType": "skill", "path": "a",
                "compatibleTools": [], "dependencies": [],
                "pinned": false, "deprecated": false
            }],
            "collections": [],
            "competencies": []
        })
        .to_string();

        let err = ManifestParser::parse(&json).unwrap_err();
        assert!(err.to_string().contains("version"));
    }

    #[test]
    fn parse_rejects_empty_manifest() {
        let json = serde_json::json!({
            "version": "1.0",
            "components": [],
            "collections": [],
            "competencies": []
        })
        .to_string();

        let err = ManifestParser::parse(&json).unwrap_err();
        assert!(err.to_string().contains("at least one"));
    }

    #[test]
    fn parse_rejects_component_with_empty_id() {
        let json = serde_json::json!({
            "version": "1.0",
            "components": [{
                "id": "", "name": "A", "description": "",
                "componentType": "skill", "path": "a",
                "compatibleTools": [], "dependencies": [],
                "pinned": false, "deprecated": false
            }],
            "collections": [],
            "competencies": []
        })
        .to_string();

        let err = ManifestParser::parse(&json).unwrap_err();
        assert!(err.to_string().contains("empty 'id'"));
    }

    #[test]
    fn parse_rejects_component_with_empty_name() {
        let json = serde_json::json!({
            "version": "1.0",
            "components": [{
                "id": "x", "name": "  ", "description": "",
                "componentType": "skill", "path": "a",
                "compatibleTools": [], "dependencies": [],
                "pinned": false, "deprecated": false
            }],
            "collections": [],
            "competencies": []
        })
        .to_string();

        let err = ManifestParser::parse(&json).unwrap_err();
        assert!(err.to_string().contains("empty 'name'"));
    }

    #[test]
    fn parse_rejects_missing_required_fields() {
        // Missing "components" key entirely
        let json = r#"{"version": "1.0"}"#;
        let result = ManifestParser::parse(json);
        assert!(result.is_err());
    }

    #[test]
    fn parse_with_root_resolves_component_paths() {
        let json = valid_manifest_json();
        let repo_root = Path::new("/home/user/repos/haal-skills");
        let manifest = ManifestParser::parse_with_root(&json, repo_root).unwrap();

        let expected = repo_root.join("skills/code-review");
        assert_eq!(
            manifest.components[0].path,
            expected.to_string_lossy().as_ref()
        );
    }

    #[test]
    fn parse_file_returns_error_for_missing_file() {
        let result = ManifestParser::parse_file(Path::new("/nonexistent/manifest.json"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Failed to read manifest file"));
    }

    #[test]
    fn parse_file_reads_and_parses_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("manifest.json");
        std::fs::write(&file_path, valid_manifest_json()).unwrap();

        let manifest = ManifestParser::parse_file(&file_path).unwrap();
        assert_eq!(manifest.version, "1.0");
        assert_eq!(manifest.components.len(), 1);
    }

    #[test]
    fn parse_accepts_manifest_with_only_collections() {
        let json = serde_json::json!({
            "version": "1.0",
            "components": [],
            "collections": [{
                "id": "c1", "name": "C1", "description": "d",
                "competencyIds": []
            }],
            "competencies": []
        })
        .to_string();

        let manifest = ManifestParser::parse(&json).unwrap();
        assert_eq!(manifest.collections.len(), 1);
    }

    #[test]
    fn parse_accepts_manifest_with_only_competencies() {
        let json = serde_json::json!({
            "version": "1.0",
            "components": [],
            "collections": [],
            "competencies": [{
                "id": "comp1", "name": "Comp1", "description": "d",
                "manifestUrl": "https://example.com/comp1.json"
            }]
        })
        .to_string();

        let manifest = ManifestParser::parse(&json).unwrap();
        assert_eq!(manifest.competencies.len(), 1);
    }
}
