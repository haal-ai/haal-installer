# Multi-Registry Model

The installer supports multiple registries simultaneously. This allows organisations to layer their own skills on top of a shared foundation — a team registry can extend a division registry, which extends an enterprise registry, which extends a public open-source registry.

## Where the registry list lives

The list of secondary registries is **not** stored in the installer. It lives in `repos-manifest.json` inside the seed registry repo itself. This is a deliberate design choice: the registry owner controls the chain, not the end user.

The user only provides one URL at the Connect step — the seed registry. Everything else is resolved automatically from there.

The installer's own config stores only the seed registry URL (and the GitHub credentials). It does not maintain a list of repos.

## Seed registry vs secondary registries

When the user connects and authenticates, they point the installer at one registry: the **seed registry**. This is the entry point — the installer fetches its manifest first and reads its `repos-manifest.json` to discover secondary registries.

```
User connects to → seed registry (URL entered at Connect step)
                       │
                       ├── repos-manifest.json
                       │     ├── secondary registry A
                       │     └── secondary registry B
                       │
                   Installer fetches all, merges catalog
```

Secondary registries are fetched in parallel. Their catalogs are merged with the seed's catalog before the user sees anything.

## No chaining — the seed is the authority

The installer reads `repos-manifest.json` **only from the seed registry**. If a secondary registry also contains a `repos-manifest.json`, it is ignored.

This is intentional. The seed registry owner decides what is included in their domain. They do not delegate that decision to registries they reference — a secondary registry cannot pull in additional registries without the seed owner's explicit knowledge and consent.

Concretely: if the seed lists registry A, and registry A lists registry B, registry B is **not** fetched. Only what the seed explicitly lists is included.

This design reflects a trust boundary: the seed owner trusts the content of the registries they list, but they do not extend that trust transitively. A secondary registry could list anything — including malicious or untested registries. The seed owner is responsible for their users' experience and cannot outsource that responsibility to a third party.

## Practical example: team → division → enterprise → open source

Consider a developer at Acme Corp, working in the payments team. The payments team maintains the seed registry. Its `repos-manifest.json` explicitly lists every registry the team wants to include:

```json
{
  "repos": [
    { "repo": "haal-ai/haal-skills",  "branch": "main" },
    { "repo": "acme/acme-skills",     "branch": "main" },
    { "repo": "acme/fintech-skills",  "branch": "main" }
  ]
}
```

The installer fetches all three in addition to the seed. Priority follows the array order: `haal-ai/haal-skills` has the lowest priority, `acme/fintech-skills` the highest among secondaries, and the seed always wins over all of them.

The payments team is in full control of what their developers get. They chose to include the open-source registry, the enterprise registry, and the division registry. If the enterprise later adds a new registry, the payments team must explicitly add it to their `repos-manifest.json` — it does not appear automatically.

## Merge rules

When the same competency or collection ID appears in multiple registries, position in the `repos` array determines who wins. The array is processed first-to-last — each entry overwrites the previous on conflict. The seed is always processed last and always wins.

```
repos[0]  ← lowest priority (processed first)
repos[1]  ← overwrites repos[0] on conflict
repos[2]  ← overwrites repos[1] on conflict
seed      ← always highest priority (processed last)
```

This means:
- Put the most generic registries (open-source, enterprise baseline) first
- Put the most specific registries (division, team) last — they override the generic ones
- The seed always has the final word regardless of position

## Enterprise governance

In this model, the seed registry is authoritative for its users. An enterprise can enforce standards by publishing an enterprise registry and requiring all team seeds to list it in their `repos-manifest.json`. Because the seed always wins, teams can still override enterprise defaults for their own context.

Individual teams contribute by adding their own competencies and collections. They cannot be forced to include content they did not explicitly list — the non-chaining design guarantees this.

## Private registries

Secondary registries can be private GitHub repositories. The installer uses the same authentication token the user provided at the Connect step. For enterprise GitHub instances, the `base_url` field in `repos-manifest.json` points to the enterprise GitHub URL:

```json
{
  "repos": [
    {
      "repo": "acme/acme-skills",
      "branch": "main",
      "base_url": "https://github.acme.com",
      "priority": 20
    }
  ]
}
```

If a secondary registry is unreachable (network issue, expired token, private repo without access), the installer logs a warning and continues with the registries it could reach. It never fails hard on a secondary registry — the seed is always sufficient to proceed.

## Caching

All fetched registry manifests and competency files are cached locally under `~/.haal/cache/`. The installer refreshes on each run when online. When offline, it uses the cached version silently.

Cache key naming: `<owner>_<repo>_<branch>/` — for example `acme_payments-skills_main/`.
