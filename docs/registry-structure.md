# Registry Structure

A registry is a Git repository that the installer clones locally and reads to discover what can be installed. It declares collections, competencies, and the components that belong to them.

## The manifest file

Every registry must have a `haal_manifest.json` at its root. It is an **index** — it declares what exists in the registry but does not contain the full component details.

```json
{
  "version": "1.0",
  "repoId": "my-org-skills",
  "description": "My organisation's AI skills registry",
  "baseUrl": "https://raw.githubusercontent.com/my-org/my-skills/main",
  "collections": [
    {
      "id": "backend-team",
      "name": "Backend Team",
      "description": "Skills for the backend engineering team",
      "competencyIds": ["java-developer", "api-producer"]
    }
  ],
  "competencies": [
    {
      "id": "java-developer",
      "name": "Java Developer",
      "description": "Core Java development skills",
      "manifestUrl": "competencies/java-developer.json"
    }
  ]
}
```

| Field | Required | Description |
|---|---|---|
| `version` | yes | Manifest format version, currently `"1.0"` |
| `repoId` | yes | Unique identifier for this registry |
| `description` | yes | Human-readable description |
| `baseUrl` | yes | Raw content base URL — used to fetch competency JSON files |
| `collections` | yes | Named groups of competencies (each just lists competency IDs) |
| `competencies` | yes | Index of competencies — id, name, description, and a `manifestUrl` pointing to the detail file |

The manifest does **not** contain the actual component lists (skills, rules, commands, etc.). Those live in separate competency files referenced by `manifestUrl`. The manifest does **not** declare systems directly either — systems are components listed inside competency files.

## Competency files

Each competency entry in the manifest has a `manifestUrl` pointing to a JSON file in the `competencies/` folder. That file is where the actual component lists live:

```json
{
  "name": "Java Developer",
  "description": "Core Java development skills and tooling",
  "skills": ["code-review", "generate-tech-spec-from-code"],
  "powers": ["code-in-java"],
  "hooks": ["lint-on-save"],
  "commands": ["review-pr", "generate-tests"],
  "rules": ["java-standards", "security-baseline"],
  "agents": ["code-reviewer"],
  "mcpServers": ["sonar-mcp"],
  "systems": ["my-agent"]
}
```

All fields are optional and default to empty arrays. The installer fetches this file on demand (when the user selects a competency in the Choose step) and resolves each ID to a physical path inside the cloned registry.

Systems are listed here just like any other component type. The system ID must match an entry in the registry manifest's `systems` metadata array (see below), which provides the repo URL needed to clone it.

## Systems metadata

Although systems are referenced from competency files by ID, the registry also needs to declare the full system metadata (repo URL, branch, tags) so the installer knows where to clone from. This is declared in the top-level `haal_manifest.json` under a `systems` array:

```json
{
  "systems": [
    {
      "id": "my-agent",
      "name": "My Agent",
      "description": "An agentic system for automated reporting",
      "repo": "https://github.com/my-org/my-agent",
      "branch": "main",
      "tags": ["reporting", "automation"]
    }
  ]
}
```

The competency file says *which* systems to install; the manifest's `systems` array says *where* to get them. The installer looks up the system ID in the merged catalog to find the repo URL at install time.

## Component folder layout

Components live in typed top-level folders. Each component is a subfolder named by its ID:

```
my-skills/
  skills/
    code-review/
      skill.md
      description.md
  powers/
    code-in-java/
      POWER.md
  hooks/
    lint-on-save/
      hook.json
  commands/
    common/
      review-pr/
        command.md
    kiro/
      review-pr/          ← Kiro-specific override
        command.md
  rules/
    common/
      security-baseline/
        rule.md
    cursor/
      security-baseline/  ← Cursor-specific override
        rule.md
  agents/
    code-reviewer/
      agent.md
  mcpservers/
    sonar-mcp/
      mcp.json
```

Rules and commands use a subfolder layer to handle tool-specific formats — see [rules-and-commands.md](rules-and-commands.md) for details.

## Legacy: `collection-manifest.json`

Older versions of the shell install scripts (`install-haal-skills.sh` / `.ps1`) read a flat `collection-manifest.json` file at the registry root to resolve collection names to competency lists. That file is no longer used.

Collections are now declared inline in `haal_manifest.json` under the `collections` array (see above). The shell scripts have been updated to read from `haal_manifest.json` directly. If you have an old `collection-manifest.json` in your registry, it can be safely removed — the Tauri installer never reads it.

## The `repos-manifest.json` file

The list of secondary registries is maintained in a `repos-manifest.json` file **inside the seed registry repo** — not in the installer itself. This means the registry owner controls which other registries get pulled in, not the end user.

```json
{
  "repos": [
    {
      "repo": "haal-ai/haal-skills",
      "branch": "main"
    },
    {
      "repo": "my-org/division-skills",
      "branch": "main"
    },
    {
      "repo": "enterprise/enterprise-skills",
      "branch": "main",
      "base_url": "https://github.myenterprise.com"
    }
  ]
}
```

| Field | Required | Description |
|---|---|---|
| `repo` | yes | `owner/repo` slug on GitHub (or the enterprise GitHub instance) |
| `branch` | yes | Branch to fetch from |
| `base_url` | no | GitHub base URL — omit for github.com, set for enterprise instances |

Priority is determined by position in the array: the **first entry has the lowest priority**, the **last entry has the highest** among secondaries. The seed registry always wins over all of them regardless of position.

The installer reads this file **only from the seed registry**. If a secondary registry also contains a `repos-manifest.json`, it is ignored. Secondary registries cannot pull in additional registries — the seed owner decides what is included in their domain and does not delegate that trust to third parties.

See [multi-registry.md](multi-registry.md) for the full merge and priority model.
