use std::path::Path;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// haal.json sidecar schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HaalSidecar {
    /// Runtime binaries required, with optional version constraint.
    /// e.g. ["python>=3.10", "node>=18", "uvx", "gh", "aws-cli"]
    #[serde(default)]
    pub runtimes: Vec<String>,
    /// If present, a requirements.txt exists in the component folder (Python deps).
    #[serde(default)]
    pub pip: Option<String>,
    /// If present, a package.json exists in the component folder (Node deps).
    #[serde(default)]
    pub npm: Option<String>,
    /// MCP server IDs this component expects to be configured.
    #[serde(default)]
    pub mcp: Vec<String>,
    /// Free-text hint shown to the user.
    #[serde(default)]
    pub notes: Option<String>,
}

// ---------------------------------------------------------------------------
// Check result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCheck {
    pub name: String,
    /// "ok" | "version-mismatch" | "missing"
    pub status: String,
    pub found_version: Option<String>,
    pub required_version: Option<String>,
    pub install_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCheck {
    pub id: String,
    /// "provided" (being installed this session) | "missing"
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentRequirements {
    pub component_id: String,
    pub component_type: String,
    pub runtimes: Vec<RuntimeCheck>,
    pub mcp: Vec<McpCheck>,
    pub has_pip: bool,
    pub has_npm: bool,
    pub notes: Option<String>,
    /// true if any runtime is "missing" or "version-mismatch"
    pub has_issues: bool,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Reads `haal.json` from `source_path`, checks all declared requirements
/// against the local system, and cross-checks MCP against `mcp_being_installed`.
pub fn check_component(
    component_id: &str,
    component_type: &str,
    source_path: &Path,
    mcp_being_installed: &[String],
) -> Option<ComponentRequirements> {
    let sidecar_path = source_path.join("haal.json");
    if !sidecar_path.exists() {
        return None; // no requirements declared
    }

    let content = std::fs::read_to_string(&sidecar_path).ok()?;
    let sidecar: HaalSidecar = serde_json::from_str(&content).ok()?;

    let runtimes: Vec<RuntimeCheck> = sidecar.runtimes.iter()
        .map(|r| check_runtime(r))
        .collect();

    let mcp: Vec<McpCheck> = sidecar.mcp.iter()
        .map(|id| McpCheck {
            id: id.clone(),
            status: if mcp_being_installed.contains(id) { "provided" } else { "missing" }.to_string(),
        })
        .collect();

    let has_issues = runtimes.iter().any(|r| r.status != "ok")
        || mcp.iter().any(|m| m.status == "missing");

    Some(ComponentRequirements {
        component_id: component_id.to_string(),
        component_type: component_type.to_string(),
        runtimes,
        mcp,
        has_pip: sidecar.pip.is_some(),
        has_npm: sidecar.npm.is_some(),
        notes: sidecar.notes,
        has_issues,
    })
}

// ---------------------------------------------------------------------------
// Runtime detection
// ---------------------------------------------------------------------------

fn check_runtime(spec: &str) -> RuntimeCheck {
    // Parse "name>=version" or "name==version" or just "name"
    let (name, op, required_version) = parse_spec(spec);

    let (found, found_version) = detect_runtime(&name);

    if !found {
        return RuntimeCheck {
            name: name.clone(),
            status: "missing".to_string(),
            found_version: None,
            required_version,
            install_hint: install_hint(&name),
        };
    }

    // If no version constraint, just "ok"
    let status = match (&required_version, &found_version) {
        (Some(req), Some(found)) => {
            if version_satisfies(found, &op, req) { "ok" } else { "version-mismatch" }
        }
        _ => "ok",
    };

    RuntimeCheck {
        name,
        status: status.to_string(),
        found_version,
        required_version,
        install_hint: if status != "ok" { install_hint(spec) } else { None },
    }
}

/// Returns (name, operator, version) from a spec like "python>=3.10"
fn parse_spec(spec: &str) -> (String, String, Option<String>) {
    for op in &[">=", "==", ">", "<=", "<"] {
        if let Some(pos) = spec.find(op) {
            return (
                spec[..pos].trim().to_string(),
                op.to_string(),
                Some(spec[pos + op.len()..].trim().to_string()),
            );
        }
    }
    (spec.trim().to_string(), ">=".to_string(), None)
}

/// Returns (found: bool, version: Option<String>)
fn detect_runtime(name: &str) -> (bool, Option<String>) {
    let (cmd, args): (&str, &[&str]) = match name {
        "python" | "python3"  => ("python3", &["--version"]),
        "node"                => ("node",    &["--version"]),
        "npm"                 => ("npm",     &["--version"]),
        "uvx" | "uv"          => ("uvx",     &["--version"]),
        "gh"                  => ("gh",      &["--version"]),
        "aws-cli" | "aws"     => ("aws",     &["--version"]),
        "cargo" | "rust"      => ("cargo",   &["--version"]),
        "docker"              => ("docker",  &["--version"]),
        "git"                 => ("git",     &["--version"]),
        "npx"                 => ("npx",     &["--version"]),
        other                 => (other,     &["--version"]),
    };

    let output = std::process::Command::new(cmd)
        .args(args)
        .output();

    match output {
        Err(_) => (false, None),
        Ok(o) if !o.status.success() && o.stdout.is_empty() && o.stderr.is_empty() => (false, None),
        Ok(o) => {
            let raw = String::from_utf8_lossy(&o.stdout).to_string()
                + &String::from_utf8_lossy(&o.stderr);
            let version = extract_version(&raw);
            (true, version)
        }
    }
}

/// Extracts the first semver-like token (e.g. "3.11.2") from a version string.
fn extract_version(raw: &str) -> Option<String> {
    for token in raw.split_whitespace() {
        let t = token.trim_start_matches('v');
        // Accept tokens like "3.11.2" or "18.0.0"
        if t.split('.').take(2).all(|p| p.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)) {
            return Some(t.to_string());
        }
    }
    None
}

/// Simple semver comparison — compares major.minor only for robustness.
fn version_satisfies(found: &str, op: &str, required: &str) -> bool {
    let f = parse_version(found);
    let r = parse_version(required);
    match (f, r) {
        (Some(fv), Some(rv)) => match op {
            ">="  => fv >= rv,
            ">"   => fv > rv,
            "=="  => fv == rv,
            "<="  => fv <= rv,
            "<"   => fv < rv,
            _     => true,
        },
        _ => true, // can't compare — assume ok
    }
}

fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<u32> = v.split('.')
        .take(3)
        .filter_map(|p| p.parse().ok())
        .collect();
    match parts.as_slice() {
        [a, b, c] => Some((*a, *b, *c)),
        [a, b]    => Some((*a, *b, 0)),
        [a]       => Some((*a, 0, 0)),
        _         => None,
    }
}

fn install_hint(name: &str) -> Option<String> {
    let hint = match name {
        "python" | "python3" => "Install from https://python.org or via your package manager",
        "node"               => "Install from https://nodejs.org or via nvm",
        "npm"                => "Comes with Node.js — install from https://nodejs.org",
        "npx"                => "Comes with Node.js — install from https://nodejs.org",
        "uvx" | "uv"         => "Install uv: https://docs.astral.sh/uv/getting-started/installation/",
        "gh"                 => "Install GitHub CLI: https://cli.github.com",
        "aws-cli" | "aws"    => "Install AWS CLI: https://aws.amazon.com/cli/",
        "cargo" | "rust"     => "Install Rust: https://rustup.rs",
        "docker"             => "Install Docker: https://docs.docker.com/get-docker/",
        "git"                => "Install Git: https://git-scm.com",
        _                    => return None,
    };
    Some(hint.to_string())
}
