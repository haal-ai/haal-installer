<script lang="ts">
  import { _ } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { open as openUrl } from "@tauri-apps/plugin-shell";
  import { settingsStore } from "../stores/settingsStore.svelte";
  import { wizardStore } from "../stores/wizardStore.svelte";
  import { componentsStore, type McpServerDef } from "../stores/componentsStore.svelte";

  let loading = $state(true);
  let installedAtHome = $state<Record<string, Set<string>>>({});
  let installedAtRepo = $state<Set<string>>(new Set());
  let installedPowers = $state<Set<string>>(new Set());
  let expandedCompetencies = $state<Set<string>>(new Set());
  let installPaths = $state<{ toolPaths: { tool: string; skillsPath: string }[]; powersPath: string } | null>(null);
  let repoScanPending = $state(false);
  let repoError = $state("");
  let selectedTools = $state<Set<string>>(new Set());
  let expandedCards = $state<Set<string>>(new Set());
  // Nearby git repos detected from CWD (scanned 2 levels deep)
  let nearbyRepos = $state<string[]>([]);
  let launchDir = $state("");
  // MCP server definitions loaded lazily by id
  let mcpDefs = $state<Record<string, McpServerDef>>({});
  // id → "up-to-date" | "outdated" (absent = not installed / unknown)
  let installStatusMap = $state<Record<string, Record<string, string>>>({});

  interface RuntimeCheck {
    name: string;
    status: "ok" | "version-mismatch" | "missing";
    foundVersion?: string;
    requiredVersion?: string;
    installHint?: string;
  }
  interface McpCheckResult {
    id: string;
    status: "provided" | "missing";
  }
  interface ComponentRequirements {
    componentId: string;
    componentType: string;
    runtimes: RuntimeCheck[];
    mcp: McpCheckResult[];
    hasPip: boolean;
    hasNpm: boolean;
    notes?: string;
    hasIssues: boolean;
  }
  let requirements = $state<ComponentRequirements[]>([]);
  let requirementsOpen = $state(false);

  let manifest = $derived(componentsStore.manifest);
  let allCompetencyIds = $derived(componentsStore.resolvedCompetencyIds);

  let allCompetencies = $derived.by(() => {
    const comps = componentsStore.competencies;
    return allCompetencyIds
      .map(id => comps.find(c => c.id === id))
      .filter(Boolean) as typeof comps;
  });

  let selectedCollections = $derived.by(() => {
    return componentsStore.collections.filter(c => componentsStore.isSelected(`collection:${c.id}`));
  });

  let selectedCompetencies = $derived.by(() => {
    const covered = new Set(selectedCollections.flatMap(c => c.competencyIds));
    return componentsStore.competencies.filter(c =>
      componentsStore.isSelected(`competency:${c.id}`) && !covered.has(c.id)
    );
  });

  let hasSelection = $derived(allCompetencyIds.length > 0);
  let homeTools = $derived(Object.keys(installedAtHome));

  let allSkills = $derived.by(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const id of allCompetencyIds) {
      const d = componentsStore.competencyDetails[id];
      if (d) for (const s of d.skills) if (!seen.has(s)) { seen.add(s); list.push(s); }
    }
    return list;
  });

  let allPowers = $derived.by(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const id of allCompetencyIds) {
      const d = componentsStore.competencyDetails[id];
      if (d) for (const p of d.powers) if (!seen.has(p)) { seen.add(p); list.push(p); }
    }
    return list;
  });

  let allMcpServers = $derived.by(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const id of allCompetencyIds) {
      const d = componentsStore.competencyDetails[id];
      if (d) for (const m of (d.mcpServers ?? [])) if (!seen.has(m)) { seen.add(m); list.push(m); }
    }
    return list;
  });

  let allHooks = $derived.by(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const id of allCompetencyIds) {
      const d = componentsStore.competencyDetails[id];
      if (d) for (const h of (d.hooks ?? [])) if (!seen.has(h)) { seen.add(h); list.push(h); }
    }
    return list;
  });

  let allCommands = $derived.by(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const id of allCompetencyIds) {
      const d = componentsStore.competencyDetails[id];
      if (d) for (const c of (d.commands ?? [])) if (!seen.has(c)) { seen.add(c); list.push(c); }
    }
    return list;
  });

  let allRules = $derived.by(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const id of allCompetencyIds) {
      const d = componentsStore.competencyDetails[id];
      if (d) for (const r of (d.rules ?? [])) if (!seen.has(r)) { seen.add(r); list.push(r); }
    }
    return list;
  });

  let allAgents = $derived.by(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const id of allCompetencyIds) {
      const d = componentsStore.competencyDetails[id];
      if (d) for (const a of (d.agents ?? [])) if (!seen.has(a)) { seen.add(a); list.push(a); }
    }
    return list;
  });

  let allSystems = $derived.by(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const id of allCompetencyIds) {
      const d = componentsStore.competencyDetails[id];
      if (d) for (const s of (d.systems ?? [])) if (!seen.has(s)) { seen.add(s); list.push(s); }
    }
    return list;
  });

  let allPackages = $derived.by(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const id of allCompetencyIds) {
      const d = componentsStore.competencyDetails[id];
      if (d) for (const p of (d.packages ?? [])) if (!seen.has(p)) { seen.add(p); list.push(p); }
    }
    return list;
  });

  /** Extract the tool slug from a registry-relative path.
   *  commands/repo/copilot/file.md → "copilot"
   *  rules/global/kiro/file.md    → "kiro"
   *  hooks/kiro/my-hook            → "kiro"
   *  agents/claude/my-agent         → "claude"
   *  packages/claude/my-pkg         → "claude" */
  function extractToolFromPath(path: string): string | null {
    const segs = path.split("/");
    if (segs.length < 3) return null;
    const kind = segs[0];
    // commands & rules have a scope segment: {kind}/{scope}/{tool}/{name}
    if (kind === "commands" || kind === "rules") return segs[2] ?? null;
    // hooks, agents, packages: {kind}/{tool}/{name}
    return segs[1] ?? null;
  }

  /** Extract a human-readable name from a registry path.
   *  commands/repo/copilot/haal-lint.md → "haal-lint"
   *  rules/repo/windsurf/ts-standard.md → "ts-standard" */
  function extractNameFromPath(path: string): string {
    const last = path.split("/").pop() ?? path;
    return last.replace(/\.md$/, "");
  }

  /** Extract the scope from a registry path (global or repo).
   *  commands/repo/kiro/... → "repo"
   *  rules/global/kiro/...  → "global"
   *  hooks/kiro/...          → null (no scope segment) */
  function extractScopeFromPath(path: string): "global" | "repo" | null {
    const segs = path.split("/");
    const kind = segs[0];
    if (kind === "commands" || kind === "rules") {
      const scope = segs[1];
      if (scope === "global" || scope === "repo") return scope;
    }
    return null;
  }

  /** Filter artifact paths by the current install scope.
   *  Home → only global/* paths
   *  Repo → only repo/* paths
   *  Both → all paths
   *  Paths without scope (hooks/agents/packages) pass through. */
  function filterByScope(items: string[]): string[] {
    const scope = wizardStore.installScope;
    if (scope === "both") return items;
    return items.filter(path => {
      const pathScope = extractScopeFromPath(path);
      if (!pathScope) return true; // no scope in path → always show
      if (scope === "home") return pathScope === "global";
      if (scope === "repo") return pathScope === "repo";
      return true;
    });
  }

  /** Whether the current install scope includes home */
  let scopeIncludesHome = $derived(wizardStore.installScope === "home" || wizardStore.installScope === "both");
  /** Whether the current install scope includes repo */
  let scopeIncludesRepo = $derived(
    (wizardStore.installScope === "repo" || wizardStore.installScope === "both") && wizardStore.repoPaths.length > 0
  );

  /** Check if a display tool name (e.g. "Copilot") matches a path tool slug (e.g. "copilot") */
  function toolMatchesSlug(displayName: string, slug: string): boolean {
    return displayName.toLowerCase().replace(/\s/g, "-").startsWith(slug);
  }

  /** Build a grouped table: rows = unique artifact names, columns = tools that have it.
   *  Returns { names: string[], toolMap: Map<name, Set<toolSlug>> } */
  function groupByNameAndTool(items: string[]): { names: string[]; toolMap: Map<string, Set<string>> } {
    const toolMap = new Map<string, Set<string>>();
    const order: string[] = [];
    for (const path of items) {
      const name = extractNameFromPath(path);
      const tool = extractToolFromPath(path);
      if (!toolMap.has(name)) { toolMap.set(name, new Set()); order.push(name); }
      if (tool) toolMap.get(name)!.add(tool);
    }
    return { names: order, toolMap };
  }

  /** Short destination path for a tool/artifact combo.
   *  Used as column subtitle so users know where files end up. */
  function destHint(type: "command" | "rule" | "hook" | "agent" | "package", toolSlug: string): string {
    const scope = wizardStore.installScope;
    const home = scope === "home" || scope === "both";
    const repo = scope === "repo" || scope === "both";
    const parts: string[] = [];
    if (type === "command") {
      if (home) {
        const m: Record<string, string> = {
          kiro: "~/.kiro/steering/", "claude-code": "~/.claude/commands/",
          cursor: "~/.cursor/commands/", windsurf: "~/.codeium/windsurf/global_workflows/",
        };
        if (m[toolSlug]) parts.push(m[toolSlug]);
      }
      if (repo) {
        const m: Record<string, string> = {
          kiro: "<repo>/.kiro/steering/", "claude-code": "<repo>/.claude/commands/",
          cursor: "<repo>/.cursor/commands/", copilot: "<repo>/.github/prompts/",
          windsurf: "<repo>/.windsurf/workflows/",
        };
        if (m[toolSlug]) parts.push(m[toolSlug]);
      }
    } else if (type === "rule") {
      if (home) {
        const m: Record<string, string> = {
          kiro: "~/.kiro/steering/", cursor: "~/.cursor/rules/",
          copilot: "~/.copilot/", windsurf: "~/.codeium/windsurf/",
          "claude-code": "~/.claude/",
        };
        if (m[toolSlug]) parts.push(m[toolSlug]);
      }
      if (repo) {
        const m: Record<string, string> = {
          kiro: "<repo>/.kiro/steering/", cursor: "<repo>/.cursor/rules/",
          copilot: "<repo>/.github/instructions/", windsurf: "<repo>/.windsurf/rules/",
          "claude-code": "<repo>/.claude/rules/",
        };
        if (m[toolSlug]) parts.push(m[toolSlug]);
      }
    } else if (type === "hook") {
      const m: Record<string, string> = {
        kiro: "<repo>/.kiro/hooks/", copilot: "<repo>/.github/hooks/",
      };
      if (m[toolSlug]) parts.push(m[toolSlug]);
    } else if (type === "agent") {
      if (home) {
        const m: Record<string, string> = {
          github: "~/.copilot/agents/", cursor: "~/.cursor/agents/",
          kiro: "~/.kiro/agents/",
        };
        if (m[toolSlug]) parts.push(m[toolSlug]);
      }
      if (repo) {
        const m: Record<string, string> = {
          github: "<repo>/.github/agents/", claude: "<repo>/.claude/agents/",
          cursor: "<repo>/.cursor/agents/", kiro: "<repo>/.kiro/agents/",
        };
        if (m[toolSlug]) parts.push(m[toolSlug]);
      }
    } else if (type === "package") {
      const m: Record<string, string> = {
        "claude-code": "~/.claude/plugins/",
      };
      if (m[toolSlug]) parts.push(m[toolSlug]);
    }
    return parts.join(" + ");
  }

  /** Tool columns for artifact cards — selected tools that are relevant */
  let artifactToolCols = $derived.by(() => {
    return (installPaths?.toolPaths ?? [])
      .filter(tp => selectedTools.has(tp.tool))
      .map(tp => ({
        key: tp.tool.toLowerCase().replace(/\s/g, "-"),
        label: tp.tool,
      }));
  });

  function toggleCard(cardId: string) {
    const next = new Set(expandedCards);
    if (next.has(cardId)) next.delete(cardId); else next.add(cardId);
    expandedCards = next;
  }

  let hasOlafData = $derived(settingsStore.isArtifactEnabled("olafData"));

  let reinstallAll = $derived(wizardStore.reinstallAll);
  let cleanInstall = $derived(wizardStore.cleanInstall);

  // Which tools support which component types
  // Returns count or null (= not applicable / tool not selected)
  function typeToolCount(type: string, toolKey: string): number | null {
    if (!settingsStore.isArtifactEnabled(type as any)) return null;
    const tl = toolKey.toLowerCase();
    const isKiro = tl.includes("kiro");
    const isClaude = tl.includes("claude");
    const isCursor = tl.includes("cursor");
    const isWindsurf = tl.includes("windsurf");
    const isCopilot = tl.includes("copilot");
    switch (type) {
      case "skills":   return allSkills.length;
      case "powers":   return isKiro ? allPowers.length : null;
      case "hooks":    return isKiro ? allHooks.length : null;
      case "commands": return (isClaude || isKiro || isWindsurf || isCursor || isCopilot) ? allCommands.length : null;
      case "rules":    return (isKiro || isCursor || isWindsurf || isClaude || isCopilot) ? allRules.length : null;
      case "agents":   return null; // repo-only, shown in repo column
      case "mcp":      return (isKiro || isClaude || isCursor || isWindsurf || isCopilot) ? allMcpServers.length : null;
      case "packages": return (isKiro || isClaude) ? allPackages.length : null;
      case "olafData": return null; // repo-only
      default:         return null;
    }
  }

  function typeRepoCount(type: string): number | null {
    if (!settingsStore.isArtifactEnabled(type as any)) return null;
    const hasRepo = (wizardStore.installScope === "repo" || wizardStore.installScope === "both") && wizardStore.repoPaths.length > 0;
    if (!hasRepo) return null;
    switch (type) {
      case "skills":   return allSkills.length;
      case "hooks":    return allHooks.length;
      case "commands": return allCommands.length;
      case "rules":    return allRules.length;
      case "agents":   return allAgents.length;
      case "powers":   return null; // home only
      case "mcp":      return allMcpServers.length;
      case "packages": return null; // home only (global install)
      case "olafData": return hasOlafData ? 1 : null;
      default:         return null;
    }
  }

  // Matrix rows definition
  const matrixRows = [
    { type: "skills",   label: "Skills",    color: "text-green-600 dark:text-green-400" },
    { type: "powers",   label: "Powers",    color: "text-purple-600 dark:text-purple-400" },
    { type: "hooks",    label: "Hooks",     color: "text-amber-600 dark:text-amber-400" },
    { type: "commands", label: "Commands",  color: "text-blue-600 dark:text-blue-400" },
    { type: "rules",    label: "Rules",     color: "text-indigo-600 dark:text-indigo-400" },
    { type: "agents",   label: "Agents",    color: "text-rose-600 dark:text-rose-400" },
    { type: "mcp",      label: "MCP",        color: "text-cyan-600 dark:text-cyan-400" },
    { type: "packages", label: "Packages",   color: "text-violet-600 dark:text-violet-400" },
    { type: "olafData", label: ".olaf/data", color: "text-teal-600 dark:text-teal-400" },
  ] as const;

  type SkillStatus = "new" | "installed" | "reinstall" | "outdated";

  // Map selectedTools (display names) → scan keys used in installedAtHome.
  // Includes ALL selected tools, even those with no installs yet (they'll show as "new").
  let activeHomeScanKeys = $derived.by(() => {
    return (installPaths?.toolPaths ?? [])
      .filter(tp => selectedTools.has(tp.tool))
      .map(tp => tp.tool.toLowerCase().replace(/\s/g, "-"));
  });

  // Status maps keyed by skill — home has per-tool status (selected tools only), repo has a single status
  let skillStatusMap = $derived.by(() => {
    const r = wizardStore.reinstallAll;
    const map: Record<string, { home: Record<string, SkillStatus>; repo: SkillStatus }> = {};
    for (const skill of allSkills) {
      const home: Record<string, SkillStatus> = {};
      for (const tool of activeHomeScanKeys) {
        const has = installedAtHome[tool]?.has(skill) ?? false;
        if (!has) { home[tool] = "new"; continue; }
        if (r) { home[tool] = "reinstall"; continue; }
        const hashStatus = installStatusMap[tool]?.[skill];
        home[tool] = hashStatus === "outdated" ? "outdated" : "installed";
      }
      const repoHas = installedAtRepo.has(skill);
      const repoHash = installStatusMap["repo"]?.[skill];
      map[skill] = {
        home,
        repo: !repoHas ? "new" : r ? "reinstall" : repoHash === "outdated" ? "outdated" : "installed",
      };
    }
    return map;
  });

  let powerStatusMap = $derived.by(() => {
    const r = wizardStore.reinstallAll;
    const map: Record<string, SkillStatus> = {};
    for (const power of allPowers) {
      const has = installedPowers.has(power);
      if (!has) { map[power] = "new"; continue; }
      if (r) { map[power] = "reinstall"; continue; }
      const hashStatus = installStatusMap["powers"]?.[power];
      map[power] = hashStatus === "outdated" ? "outdated" : "installed";
    }
    return map;
  });

  // Counts scoped to the chosen destination AND selected tools only.
  // "existing" = installed on ALL selected home tools (for home/both) or in repo (for repo/both).
  // A skill is "new" if it needs to be installed on at least one selected destination.
  let counts = $derived.by(() => {
    const s = wizardStore.installScope;
    let newSkills = 0, existingSkills = 0, newPowers = 0, existingPowers = 0;
    for (const skill of allSkills) {
      let isFullyInstalled = true;
      if (s === "home" || s === "both") {
        // Must be installed on every selected tool to count as "existing"
        if (activeHomeScanKeys.length === 0) isFullyInstalled = false;
        else isFullyInstalled = activeHomeScanKeys.every(t => installedAtHome[t]?.has(skill));
      }
      if (s === "repo" || s === "both") {
        const repoOk = installedAtRepo.has(skill);
        isFullyInstalled = s === "both" ? (isFullyInstalled && repoOk) : repoOk;
      }
      if (isFullyInstalled) existingSkills++; else newSkills++;
    }
    for (const power of allPowers) {
      if (installedPowers.has(power)) existingPowers++; else newPowers++;
    }
    return { newSkills, existingSkills, newPowers, existingPowers };
  });

  // Columns shown in the skill table — one per selected tool (home) + optional repo column.
  // Uses selectedTools (not activeHomeScanKeys) so tools with zero installs still get a column.
  let skillTableCols = $derived.by(() => {
    const showHome = wizardStore.installScope === "home" || wizardStore.installScope === "both";
    const showRepo = (wizardStore.installScope === "repo" || wizardStore.installScope === "both") && wizardStore.repoPaths.length > 0;
    const homeCols = showHome
      ? (installPaths?.toolPaths ?? [])
          .filter(tp => selectedTools.has(tp.tool))
          .map(tp => ({
            key: tp.tool.toLowerCase().replace(/\s/g, "-"),
            label: tp.tool,
          }))
      : [];
    return [
      ...homeCols,
      ...(showRepo ? [{ key: "repo", label: "Repo" }] : []),
    ];
  });

  let skillsInstallPaths = $derived.by(() => {
    const s = wizardStore.installScope;
    const repoPaths = wizardStore.repoPaths;
    const repo = repoPaths.length > 0 ? `${repoPaths[0]}/.kiro/skills` : null;
    const activeToolPaths = installPaths?.toolPaths.filter(t => selectedTools.has(t.tool)) ?? [];
    if (s === "home") return activeToolPaths.map(t => ({ label: t.tool, path: t.skillsPath }));
    if (s === "repo") return repo ? [{ label: "Repo", path: repo }] : [];
    return [
      ...activeToolPaths.map(t => ({ label: t.tool, path: t.skillsPath })),
      ...(repo ? [{ label: "Repo", path: repo }] : []),
    ];
  });

  // Re-scan repo whenever repoPaths changes (scan first repo for status display)
  $effect(() => {
    const paths = wizardStore.repoPaths;
    if (paths.length === 0) { installedAtRepo = new Set(); return; }
    repoScanPending = true;
    invoke<Record<string, string[]>>("scan_installed_at", { basePath: paths[0] })
      .then(raw => { installedAtRepo = new Set(raw["repo"] ?? []); })
      .catch(() => { installedAtRepo = new Set(); })
      .finally(() => { repoScanPending = false; });
  });

  async function loadPreviewData() {
    loading = true;

    // 1. Fetch all competency details in parallel (skip already cached)
    const toFetch = allCompetencies.filter(c => !componentsStore.competencyDetails[c.id]);
    const sources = componentsStore.mergedCatalog?.competencySources ?? {};
    const results = await Promise.allSettled(toFetch.map(comp =>
      invoke("fetch_competency", {
        competencyId: comp.id,
        manifestUrl: comp.manifestUrl,
        // Pass local repo path when available — Rust will read from disk, no network call
        baseUrl: sources[comp.id] ?? componentsStore.manifest?.baseUrl ?? "",
      }).then(detail => componentsStore.setCompetencyDetail(comp.id, detail as any))
    ));

    // 2. Parallel: scan installed + get paths + pre-fill CWD
    const [rawInstalled, rawPowers, rawPaths, rawCwd] = await Promise.allSettled([
      invoke<Record<string, string[]>>("scan_installed"),
      invoke<string[]>("scan_installed_powers"),
      invoke<{ toolPaths: { tool: string; skillsPath: string }[]; powersPath: string }>("get_install_paths"),
      invoke<string>("get_current_dir"),
    ]);

    if (rawInstalled.status === "fulfilled") {
      const mapped: Record<string, Set<string>> = {};
      for (const [tool, ids] of Object.entries(rawInstalled.value)) mapped[tool] = new Set(ids);
      installedAtHome = mapped;
    }
    if (rawPowers.status === "fulfilled") installedPowers = new Set(rawPowers.value);
    if (rawPaths.status === "fulfilled") {
      installPaths = rawPaths.value;
      // Pre-select only tools that are enabled in settings
      selectedTools = new Set(
        rawPaths.value.toolPaths
          .map(t => t.tool)
          .filter(tool => settingsStore.isToolEnabled(tool as any))
      );
    }
    if (rawCwd.status === "fulfilled" && rawCwd.value) {
      launchDir = rawCwd.value;
      if (wizardStore.repoPaths.length === 0) {
        // Scan CWD and 2 levels deep for git repos
        try {
          const repos = await invoke<string[]>("scan_nearby_git_repos", {
            root: rawCwd.value,
            maxDepth: 2,
          });
          nearbyRepos = repos;
          if (repos.length > 0) {
            // Auto-select all found repos and default to "both"
            wizardStore.setRepoPaths(repos);
            wizardStore.setInstallScope("both");
          }
          // If no repos found, scope stays "home" (the default)
        } catch {
          // Scan failed — leave blank, user must pick manually
        }
      }
    }

    // 3. Parallel: hash status + MCP defs + requirement checks
    const mcpIds = allMcpServers;
    const mcpToLoad = mcpIds.filter(id => !mcpDefs[id]);

    await Promise.allSettled([
      // Hash-based update detection — only for selected skills/powers
      invoke<Record<string, Array<{ id: string; status: string }>>>(
        "scan_installed_with_status", {
          catalogSources: sources,
          componentIds: [...allSkills, ...allPowers],
        }
      ).then(raw => {
        const mapped: Record<string, Record<string, string>> = {};
        for (const [tool, items] of Object.entries(raw)) {
          mapped[tool] = {};
          for (const item of items) mapped[tool][item.id] = item.status;
        }
        installStatusMap = mapped;
      }),

      // MCP server definitions
      ...mcpToLoad.map(mcpId => {
        const compId = allCompetencyIds.find(cid =>
          componentsStore.competencyDetails[cid]?.mcpServers?.includes(mcpId)
        );
        const sourceRepo = compId ? (sources[compId] ?? "") : "";
        if (!sourceRepo) return Promise.resolve();
        return invoke<McpServerDef>("read_mcp_server_def", {
          sourcePath: `${sourceRepo}/mcpservers/${mcpId}`,
        }).then(def => { mcpDefs = { ...mcpDefs, [mcpId]: def }; });
      }),

      // Requirements check
      invoke<ComponentRequirements[]>("check_requirements", {
        components: componentsStore.buildResolvedComponents(),
        mcpBeingInstalled: mcpIds,
      }).then(reqs => { requirements = reqs.filter(r => r.hasIssues); }),
    ]);

    loading = false;
  }

  loadPreviewData();

  async function pickRepoFolder() {
    const selected = await open({ directory: true, multiple: false, title: "Select project folder" });
    if (selected && typeof selected === "string") {
      repoError = "";
      try {
        // If the folder itself is a git repo, add it
        await invoke("validate_git_repo", { path: selected });
        if (!wizardStore.repoPaths.includes(selected)) {
          wizardStore.setRepoPaths([...wizardStore.repoPaths, selected]);
        }
        // Also add to nearbyRepos for display
        if (!nearbyRepos.includes(selected)) {
          nearbyRepos = [...nearbyRepos, selected];
          launchDir = launchDir || selected;
        }
      } catch {
        // Not a git repo — scan inside for child repos
        try {
          const repos = await invoke<string[]>("scan_nearby_git_repos", {
            root: selected,
            maxDepth: 2,
          });
          if (repos.length > 0) {
            nearbyRepos = [...new Set([...nearbyRepos, ...repos])];
            launchDir = selected;
            wizardStore.setRepoPaths([...new Set([...wizardStore.repoPaths, ...repos])]);
          } else {
            repoError = "No git repositories found in this folder (searched 2 levels deep).";
          }
        } catch (e: any) {
          repoError = String(e);
        }
      }
    }
  }

  function skillDocsUrl(skillId: string) {
    return `https://haal-ai.github.io/haal-skills/skills/${skillId}/description/`;
  }

  function handleConfirm() {
    // Build the InstallRequest from current wizard state and store it
    // so ExecuteStep can pass it directly to install_components_v2
    const resolvedComponents = componentsStore.buildResolvedComponents();
    wizardStore.setInstallRequest({
      components: resolvedComponents,
      scope: wizardStore.installScope,
      repoPaths: wizardStore.repoPaths,
      selectedTools: Array.from(selectedTools),
      reinstallAll: wizardStore.reinstallAll,
      cleanInstall: wizardStore.cleanInstall,
    });
    wizardStore.nextStep();
  }
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-xl font-semibold text-gray-900 dark:text-gray-100">
      {$_("wizard.preview.title")}
    </h2>
    <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
      {$_("wizard.preview.description")}
    </p>
  </div>

  {#if loading}
    <div class="text-center py-12 text-gray-500 dark:text-gray-400">
      <p>{$_("wizard.preview.loadingPreview")}</p>
    </div>
  {:else if !hasSelection}
    <div class="text-center py-12 text-gray-500 dark:text-gray-400">
      <p>{$_("wizard.preview.noChanges")}</p>
    </div>
  {:else}

    <!-- DESTINATION -->
    <div class="border border-gray-200 dark:border-gray-700 rounded-lg p-3 space-y-3">
      <div class="flex items-center justify-between">
        <p class="text-sm font-medium text-gray-700 dark:text-gray-300">Install destination</p>
        <p class="text-xs text-gray-400 dark:text-gray-500">Powers always install globally for Kiro</p>
      </div>

      <!-- Scope: inline radio row -->
      <div class="flex gap-4">
        {#each [
          { value: "home", label: "Home (global)" },
          { value: "repo", label: "Repo only" },
          { value: "both", label: "Both" },
        ] as opt}
          <label class="flex items-center gap-1.5 cursor-pointer">
            <input type="radio" name="install-scope" value={opt.value}
              checked={wizardStore.installScope === opt.value}
              onchange={() => wizardStore.setInstallScope(opt.value as any)}
              class="w-3.5 h-3.5 text-blue-600 border-gray-300 dark:border-gray-600"
            />
            <span class="text-sm text-gray-700 dark:text-gray-300">{opt.label}</span>
          </label>
        {/each}
      </div>

      <!-- Repo folder picker (only when repo/both) -->
      {#if wizardStore.installScope !== "home"}
        <!-- Detected nearby repos -->
        {#if nearbyRepos.length > 0}
          <div class="space-y-1.5">
            <p class="text-xs font-medium text-gray-500 dark:text-gray-400">
              Git repositories found near <span class="font-mono">{launchDir.split(/[\\/]/).pop()}/</span>:
            </p>
            <div class="max-h-32 overflow-y-auto space-y-1 rounded border border-gray-200 dark:border-gray-700 p-1.5">
              {#each nearbyRepos as repo}
                {@const isSelected = wizardStore.repoPaths.includes(repo)}
                {@const shortPath = repo.replace(/\\/g, "/").split("/").slice(-2).join("/")}
                <button
                  onclick={() => { repoError = ""; wizardStore.toggleRepoPath(repo); }}
                  class="w-full text-left px-2 py-1.5 text-xs rounded transition-colors flex items-center gap-2
                    {isSelected
                      ? 'bg-blue-50 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 font-medium'
                      : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-gray-800'}"
                >
                  <span class="flex-shrink-0">{isSelected ? '☑' : '☐'}</span>
                  <span class="font-mono truncate" title={repo}>{shortPath}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}

        <!-- Manual folder picker -->
        <div class="flex items-center gap-2">
          <input type="text" readonly value={wizardStore.repoPaths.length > 0 ? `${wizardStore.repoPaths.length} repo(s) selected` : "No folder selected"}
            class="flex-1 text-xs font-mono px-2 py-1.5 rounded border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-900 text-gray-500 dark:text-gray-400 truncate min-w-0"
          />
          <button onclick={pickRepoFolder}
            class="flex-shrink-0 px-3 py-1.5 text-xs rounded border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700">
            Browse…
          </button>
        </div>
        {#if repoScanPending}
          <p class="text-xs text-blue-500">Scanning…</p>
        {:else if repoError}
          <p class="text-xs text-red-600 dark:text-red-400">⛔ {repoError}</p>
        {:else if wizardStore.repoPaths.length === 0 && nearbyRepos.length === 0}
          <p class="text-xs text-amber-600 dark:text-amber-400">⚠ No git repositories found nearby. Browse to select one.</p>
        {:else if wizardStore.repoPaths.length === 0}
          <p class="text-xs text-amber-600 dark:text-amber-400">⚠ Select at least one git repository</p>
        {/if}
      {/if}

      <!-- No repos found hint — show when scope is home and we scanned -->
      {#if wizardStore.installScope === "home" && nearbyRepos.length === 0 && launchDir}
        <p class="text-xs text-gray-400 dark:text-gray-500">
          💡 No git repos found near launch folder. Switch to "Repo only" or "Both" to browse for one.
        </p>
      {/if}

      <!-- Tool checkboxes moved to matrix table header below -->

      <!-- Optional artifact types — opt-in toggles -->
      <div class="pt-2 border-t border-gray-200 dark:border-gray-700">
        <p class="text-xs font-medium text-gray-500 dark:text-gray-400 mb-1.5">Include optional extras:</p>
        <div class="flex flex-wrap gap-x-4 gap-y-1.5">
          {#each [
            { key: "powers",     label: "⚡ Kiro Powers",    hint: "Always-active context for Kiro" },
            { key: "mcpServers", label: "🔌 MCP Servers",    hint: "Tool config for AI coding tools" },
            { key: "olafData",   label: "📚 .olaf/data",     hint: "Knowledge base in each repo" },
          ] as opt}
            {@const enabled = settingsStore.isArtifactEnabled(opt.key as any)}
            <label class="flex items-center gap-1.5 cursor-pointer group" title={opt.hint}>
              <input type="checkbox" checked={enabled}
                onchange={() => settingsStore.toggleArtifact(opt.key as any)}
                class="w-3.5 h-3.5 rounded border-gray-300 dark:border-gray-600 text-blue-600"
              />
              <span class="text-xs text-gray-700 dark:text-gray-300 group-hover:text-blue-600 dark:group-hover:text-blue-400">{opt.label}</span>
            </label>
          {/each}
        </div>
      </div>

      <!-- Resolved paths (compact, muted) -->
      {#if (wizardStore.installScope === "home" || wizardStore.installScope === "both") && installPaths && installPaths.toolPaths.length > 0}
        <div class="pt-2 border-t border-gray-200 dark:border-gray-700 space-y-0.5">
          {#each installPaths.toolPaths.filter(t => selectedTools.has(t.tool)) as tp}
            <p class="text-xs text-gray-400 dark:text-gray-500">
              <span class="font-medium">{tp.tool}:</span>
              <span class="font-mono ml-1">{tp.skillsPath}</span>
            </p>
          {/each}
          {#if allPowers.length > 0 && installPaths}
            <p class="text-xs text-gray-400 dark:text-gray-500">
              <span class="font-medium">Powers:</span>
              <span class="font-mono ml-1">{installPaths.powersPath}</span>
            </p>
          {/if}
        </div>
      {/if}
    </div>

    <!-- SUMMARY MATRIX: rows = component types, columns = tools + repo -->
    {#if true}
      {@const showHome = wizardStore.installScope === "home" || wizardStore.installScope === "both"}
      {@const showRepo = (wizardStore.installScope === "repo" || wizardStore.installScope === "both") && wizardStore.repoPaths.length > 0}
      {@const allToolCols = installPaths?.toolPaths ?? []}
      {@const toolCols = showHome ? allToolCols : []}
      {@const colCount = toolCols.length + (showRepo ? 1 : 0)}
    <div class="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
      <!-- Repo-only tool selector: which tools to install for in the repo -->
      {#if showRepo && !showHome && allToolCols.length > 0}
        <div class="px-3 py-2 bg-gray-50 dark:bg-gray-800/80 border-b border-gray-200 dark:border-gray-700">
          <p class="text-xs font-medium text-gray-500 dark:text-gray-400 mb-1.5">Install for tools (repo-scoped):</p>
          <div class="flex flex-wrap gap-3">
            {#each allToolCols as tp}
              <label class="flex items-center gap-1.5 cursor-pointer">
                <input type="checkbox" checked={selectedTools.has(tp.tool)}
                  onchange={() => {
                    const next = new Set(selectedTools);
                    if (next.has(tp.tool)) next.delete(tp.tool); else next.add(tp.tool);
                    selectedTools = next;
                  }}
                  class="w-3.5 h-3.5 rounded border-gray-300 dark:border-gray-600 text-blue-600"
                />
                <span class="text-xs text-gray-700 dark:text-gray-300">{tp.tool}</span>
              </label>
            {/each}
          </div>
        </div>
      {/if}
      <div class="overflow-x-auto">
        <table class="w-full text-xs">
          <thead>
            <tr class="border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/80">
              <th class="text-left px-3 py-2 font-medium text-gray-500 dark:text-gray-400 w-24">Type</th>
              {#each toolCols as tp}
                <th class="px-3 py-2 text-center font-medium text-gray-700 dark:text-gray-300 min-w-[5rem]">
                  <label class="flex flex-col items-center gap-1 cursor-pointer">
                    <input type="checkbox" checked={selectedTools.has(tp.tool)}
                      onchange={() => {
                        const next = new Set(selectedTools);
                        if (next.has(tp.tool)) next.delete(tp.tool); else next.add(tp.tool);
                        selectedTools = next;
                      }}
                      class="w-3.5 h-3.5 rounded border-gray-300 dark:border-gray-600 text-blue-600"
                    />
                    <span class="text-xs">{tp.tool}</span>
                  </label>
                </th>
              {/each}
              {#if showRepo}
                <th class="px-3 py-2 text-center font-medium text-gray-700 dark:text-gray-300 min-w-[5rem]">
                  <span class="flex flex-col items-center gap-1">
                    <span class="text-xs">📁</span>
                    <span class="text-xs">Repo</span>
                    {#if !showHome}
                      <span class="text-xs text-gray-400 dark:text-gray-500 font-normal">{wizardStore.repoPaths.length} repo(s)</span>
                    {/if}
                  </span>
                </th>
              {/if}
              {#if colCount === 0}
                <th class="px-3 py-2 text-center text-gray-400 dark:text-gray-500 italic">No destination selected</th>
              {/if}
            </tr>
          </thead>
          <tbody>
            {#each matrixRows as row}
              {@const hasAny =
                toolCols.some(tp => typeToolCount(row.type, tp.tool) !== null && (typeToolCount(row.type, tp.tool) ?? 0) > 0) ||
                (showRepo && (typeRepoCount(row.type) ?? 0) > 0)}
              {#if hasAny}
                <tr class="border-b border-gray-100 dark:border-gray-800 last:border-0 hover:bg-gray-50/50 dark:hover:bg-gray-800/30">
                  <td class="px-3 py-2 font-medium {row.color}">{row.label}</td>
                  {#each toolCols as tp}
                    {@const count = typeToolCount(row.type, tp.tool)}
                    <td class="px-3 py-2 text-center">
                      {#if count === null}
                        <span class="text-gray-300 dark:text-gray-600">—</span>
                      {:else if !selectedTools.has(tp.tool)}
                        <span class="text-gray-300 dark:text-gray-600">—</span>
                      {:else if count === 0}
                        <span class="text-gray-300 dark:text-gray-600">0</span>
                      {:else}
                        <span class="inline-block px-1.5 py-0.5 rounded font-mono font-bold {row.color} bg-gray-100 dark:bg-gray-800">{count}</span>
                      {/if}
                    </td>
                  {/each}
                  {#if showRepo}
                    {@const count = typeRepoCount(row.type)}
                    <td class="px-3 py-2 text-center">
                      {#if count === null}
                        <span class="text-gray-300 dark:text-gray-600">—</span>
                      {:else if count === 0}
                        <span class="text-gray-300 dark:text-gray-600">0</span>
                      {:else}
                        <span class="inline-block px-1.5 py-0.5 rounded font-mono font-bold {row.color} bg-gray-100 dark:bg-gray-800">{count}</span>
                        {#if row.type === "mcp" && !showHome}
                          <div class="text-xs text-gray-400 dark:text-gray-500 mt-0.5 font-normal">all tools</div>
                        {/if}
                      {/if}
                    </td>
                  {/if}
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
      </div>
    </div>
    {/if}

    <!-- POWERS detail (home-scoped only, Kiro) -->
    {#if allPowers.length > 0 && scopeIncludesHome}
      <div class="border border-purple-200 dark:border-purple-800 rounded-lg overflow-hidden">
        <button onclick={() => toggleCard('powers')} class="w-full px-3 py-2 bg-purple-50 dark:bg-purple-900/20 flex items-center justify-between cursor-pointer hover:bg-purple-100 dark:hover:bg-purple-900/30 transition-colors">
          <span class="flex items-center gap-2">
            <span class="text-sm">⚡</span>
            <p class="text-sm font-medium text-purple-800 dark:text-purple-200">Kiro Powers</p>
          </span>
          <span class="flex items-center gap-2">
            <span class="text-xs text-purple-500 dark:text-purple-400">{allPowers.length} powers · ~/.kiro/powers/</span>
            <span class="text-xs text-gray-400">{expandedCards.has('powers') ? '▲' : '▼'}</span>
          </span>
        </button>
        {#if expandedCards.has('powers')}
        <div class="p-3 space-y-1">
          {#each allPowers as power}
            {@const status = powerStatusMap[power] ?? "new"}
            <div class="flex items-center justify-between">
              <span class="text-xs font-mono text-gray-700 dark:text-gray-300">{power}</span>
              {#if status === "reinstall"}
                <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-orange-100 dark:bg-orange-900/30 text-orange-600 dark:text-orange-400">↺ reinstall</span>
              {:else if status === "outdated"}
                <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-yellow-100 dark:bg-yellow-900/30 text-yellow-600 dark:text-yellow-400">↑ update</span>
              {:else if status === "installed"}
                <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-blue-100 dark:bg-blue-900/30 text-blue-600 dark:text-blue-400">✓ installed</span>
              {:else}
                <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-purple-100 dark:bg-purple-900/30 text-purple-600 dark:text-purple-400">+ new</span>
              {/if}
            </div>
          {/each}
        </div>
        {/if}
      </div>
    {/if}

    <!-- MCP SERVERS detail -->
    {#if allMcpServers.length > 0}
      {@const mcpTools = (installPaths?.toolPaths ?? []).filter(tp => {
        const tl = tp.tool.toLowerCase();
        return tl.includes("kiro") || tl.includes("claude") || tl.includes("cursor") || tl.includes("windsurf") || tl.includes("copilot");
      })}
      <div class="border border-cyan-200 dark:border-cyan-800 rounded-lg overflow-hidden">
        <button onclick={() => toggleCard('mcp')} class="w-full px-3 py-2 bg-cyan-50 dark:bg-cyan-900/20 flex items-center justify-between cursor-pointer hover:bg-cyan-100 dark:hover:bg-cyan-900/30 transition-colors">
          <span class="flex items-center gap-2">
            <span class="text-sm">🔌</span>
            <p class="text-sm font-medium text-cyan-800 dark:text-cyan-200">MCP Servers</p>
          </span>
          <span class="flex items-center gap-2">
            <span class="text-xs text-cyan-500 dark:text-cyan-400">{allMcpServers.length} servers</span>
            <span class="text-xs text-gray-400">{expandedCards.has('mcp') ? '▲' : '▼'}</span>
          </span>
        </button>
        {#if expandedCards.has('mcp')}
        <div class="p-3 space-y-2">
          {#each allMcpServers as mcpId}
            {@const def = mcpDefs[mcpId]}
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0">
                <div class="flex items-center gap-1.5">
                  <span class="text-xs font-mono text-gray-700 dark:text-gray-300">{mcpId}</span>
                  {#if def}
                    <span class="inline-block px-1.5 py-0.5 text-xs rounded
                      {def.transport === 'http'
                        ? 'bg-blue-100 dark:bg-blue-900/30 text-blue-600 dark:text-blue-400'
                        : 'bg-amber-100 dark:bg-amber-900/30 text-amber-600 dark:text-amber-400'}">
                      {def.transport === 'http' ? '☁ remote' : '⚙ local'}
                    </span>
                  {/if}
                </div>
                {#if def}
                  <p class="text-xs text-gray-400 dark:text-gray-500 mt-0.5">{def.description}</p>
                  {#if def.transport === 'http' && def.serverUrl}
                    <p class="text-xs font-mono text-gray-400 dark:text-gray-500 truncate">{def.serverUrl}</p>
                  {:else if def.transport === 'stdio' && def.command}
                    <p class="text-xs font-mono text-gray-400 dark:text-gray-500">{def.command} {def.args.join(' ')}</p>
                  {/if}
                {/if}
              </div>
              <span class="flex-shrink-0 inline-block px-1.5 py-0.5 text-xs rounded bg-cyan-100 dark:bg-cyan-900/30 text-cyan-600 dark:text-cyan-400">+ new</span>
            </div>
          {/each}

          <!-- Tool selector — always visible so user knows which tools get the MCP config -->
          <div class="pt-2 border-t border-gray-100 dark:border-gray-700">
            <p class="text-xs text-gray-500 dark:text-gray-400 mb-1.5">Inject into:</p>
            <div class="flex flex-wrap gap-3">
              {#each mcpTools as tp}
                <label class="flex items-center gap-1.5 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={selectedTools.has(tp.tool)}
                    onchange={() => {
                      const next = new Set(selectedTools);
                      if (next.has(tp.tool)) next.delete(tp.tool); else next.add(tp.tool);
                      selectedTools = next;
                    }}
                    class="w-3.5 h-3.5 rounded border-gray-300 dark:border-gray-600 text-cyan-600"
                  />
                  <span class="text-xs text-gray-700 dark:text-gray-300">{tp.tool}</span>
                </label>
              {/each}
              {#if mcpTools.length === 0}
                <span class="text-xs text-gray-400 dark:text-gray-500 italic">No compatible tools detected</span>
              {/if}
            </div>
            {#if wizardStore.installScope !== "home"}
              <p class="text-xs text-gray-400 dark:text-gray-500 mt-1.5">
                + workspace configs in {wizardStore.repoPaths.length} repo(s)
              </p>
            {/if}
          </div>
        </div>
        {/if}
      </div>
    {/if}

    <!-- SYSTEMS -->
    {#if allSystems.length > 0}
      <div class="border border-orange-200 dark:border-orange-800 rounded-lg overflow-hidden">
        <button onclick={() => toggleCard('systems')} class="w-full px-3 py-2 bg-orange-50 dark:bg-orange-900/20 flex items-center justify-between cursor-pointer hover:bg-orange-100 dark:hover:bg-orange-900/30 transition-colors">
          <span class="flex items-center gap-2">
            <span class="text-sm">🚀</span>
            <p class="text-sm font-medium text-orange-800 dark:text-orange-200">Agentic Systems</p>
          </span>
          <span class="flex items-center gap-2">
            <span class="text-xs text-orange-500 dark:text-orange-400">{allSystems.length} systems</span>
            <span class="text-xs text-gray-400">{expandedCards.has('systems') ? '▲' : '▼'}</span>
          </span>
        </button>
        {#if expandedCards.has('systems')}
        <div class="p-3 space-y-1">
          {#each allSystems as sysId}
            {@const sysEntry = componentsStore.mergedCatalog?.systems.find(s => s.id === sysId)}
            <div class="flex items-center justify-between">
              <div class="min-w-0">
                <span class="text-xs font-mono text-gray-700 dark:text-gray-300">{sysId}</span>
                {#if sysEntry}
                  <p class="text-xs text-gray-400 dark:text-gray-500 truncate">{sysEntry.description}</p>
                {/if}
              </div>
              <span class="flex-shrink-0 inline-block px-1.5 py-0.5 text-xs rounded bg-orange-100 dark:bg-orange-900/30 text-orange-600 dark:text-orange-400">+ clone</span>
            </div>
          {/each}
        </div>
        {/if}
      </div>
    {/if}

    <!-- COMMANDS detail -->
    {#if allCommands.length > 0}
      {@const scoped = filterByScope(allCommands)}
      {@const grouped = groupByNameAndTool(scoped)}
      {@const visibleNames = grouped.names.filter(n => {
        const tools = grouped.toolMap.get(n)!;
        return artifactToolCols.some(c => tools.has(c.key));
      })}
      {#if visibleNames.length > 0}
      <div class="border border-blue-200 dark:border-blue-800 rounded-lg overflow-hidden">
        <button onclick={() => toggleCard('commands')} class="w-full px-3 py-2 bg-blue-50 dark:bg-blue-900/20 flex items-center justify-between cursor-pointer hover:bg-blue-100 dark:hover:bg-blue-900/30 transition-colors">
          <span class="flex items-center gap-2">
            <span class="text-sm">💬</span>
            <p class="text-sm font-medium text-blue-800 dark:text-blue-200">Commands</p>
          </span>
          <span class="flex items-center gap-2">
            <span class="text-xs text-blue-500 dark:text-blue-400">{visibleNames.length} commands</span>
            <span class="text-xs text-gray-400">{expandedCards.has('commands') ? '▲' : '▼'}</span>
          </span>
        </button>
        {#if expandedCards.has('commands')}
        <div class="p-3">
          {#if artifactToolCols.length > 0}
            <div class="grid gap-1 mb-1" style="grid-template-columns: 1fr {artifactToolCols.map(() => '6rem').join(' ')}">
              <span class="text-xs text-gray-400 dark:text-gray-500">Command</span>
              {#each artifactToolCols as col}
                {@const hint = destHint('command', col.key)}
                <span class="text-center">
                  <span class="text-xs text-gray-400 dark:text-gray-500 block">{col.label}</span>
                  {#if hint}
                    <span class="text-[10px] text-gray-400/60 dark:text-gray-600 font-mono block truncate" title={hint}>{hint}</span>
                  {/if}
                </span>
              {/each}
            </div>
          {/if}
          <div class="space-y-1">
            {#each visibleNames as name}
              {@const tools = grouped.toolMap.get(name)!}
              <div class="grid gap-1 items-center" style="grid-template-columns: 1fr {artifactToolCols.map(() => '6rem').join(' ')}">
                <span class="text-xs font-mono text-gray-700 dark:text-gray-300 truncate">{name}</span>
                {#each artifactToolCols as col}
                  <span class="text-center">
                    {#if tools.has(col.key)}
                      <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-green-100 dark:bg-green-900/30 text-green-600 dark:text-green-400">+ new</span>
                    {:else}
                      <span class="text-gray-300 dark:text-gray-600">—</span>
                    {/if}
                  </span>
                {/each}
              </div>
            {/each}
          </div>
        </div>
        {/if}
      </div>
      {/if}
    {/if}

    <!-- RULES detail -->
    {#if allRules.length > 0}
      {@const scoped = filterByScope(allRules)}
      {@const grouped = groupByNameAndTool(scoped)}
      {@const visibleNames = grouped.names.filter(n => {
        const tools = grouped.toolMap.get(n)!;
        return artifactToolCols.some(c => tools.has(c.key));
      })}
      {#if visibleNames.length > 0}
      <div class="border border-indigo-200 dark:border-indigo-800 rounded-lg overflow-hidden">
        <button onclick={() => toggleCard('rules')} class="w-full px-3 py-2 bg-indigo-50 dark:bg-indigo-900/20 flex items-center justify-between cursor-pointer hover:bg-indigo-100 dark:hover:bg-indigo-900/30 transition-colors">
          <span class="flex items-center gap-2">
            <span class="text-sm">📏</span>
            <p class="text-sm font-medium text-indigo-800 dark:text-indigo-200">Rules</p>
          </span>
          <span class="flex items-center gap-2">
            <span class="text-xs text-indigo-500 dark:text-indigo-400">{visibleNames.length} rules</span>
            <span class="text-xs text-gray-400">{expandedCards.has('rules') ? '▲' : '▼'}</span>
          </span>
        </button>
        {#if expandedCards.has('rules')}
        <div class="p-3">
          {#if artifactToolCols.length > 0}
            <div class="grid gap-1 mb-1" style="grid-template-columns: 1fr {artifactToolCols.map(() => '6rem').join(' ')}">
              <span class="text-xs text-gray-400 dark:text-gray-500">Rule</span>
              {#each artifactToolCols as col}
                {@const hint = destHint('rule', col.key)}
                <span class="text-center">
                  <span class="text-xs text-gray-400 dark:text-gray-500 block">{col.label}</span>
                  {#if hint}
                    <span class="text-[10px] text-gray-400/60 dark:text-gray-600 font-mono block truncate" title={hint}>{hint}</span>
                  {/if}
                </span>
              {/each}
            </div>
          {/if}
          <div class="space-y-1">
            {#each visibleNames as name}
              {@const tools = grouped.toolMap.get(name)!}
              <div class="grid gap-1 items-center" style="grid-template-columns: 1fr {artifactToolCols.map(() => '6rem').join(' ')}">
                <span class="text-xs font-mono text-gray-700 dark:text-gray-300 truncate">{name}</span>
                {#each artifactToolCols as col}
                  <span class="text-center">
                    {#if tools.has(col.key)}
                      <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-green-100 dark:bg-green-900/30 text-green-600 dark:text-green-400">+ new</span>
                    {:else}
                      <span class="text-gray-300 dark:text-gray-600">—</span>
                    {/if}
                  </span>
                {/each}
              </div>
            {/each}
          </div>
        </div>
        {/if}
      </div>
      {/if}
    {/if}

    <!-- HOOKS detail (repo-scoped only) -->
    {#if allHooks.length > 0 && scopeIncludesRepo}
      {@const grouped = groupByNameAndTool(allHooks)}
      {@const visibleNames = grouped.names.filter(n => {
        const tools = grouped.toolMap.get(n)!;
        return artifactToolCols.some(c => tools.has(c.key));
      })}
      {#if visibleNames.length > 0}
      <div class="border border-amber-200 dark:border-amber-800 rounded-lg overflow-hidden">
        <button onclick={() => toggleCard('hooks')} class="w-full px-3 py-2 bg-amber-50 dark:bg-amber-900/20 flex items-center justify-between cursor-pointer hover:bg-amber-100 dark:hover:bg-amber-900/30 transition-colors">
          <span class="flex items-center gap-2">
            <span class="text-sm">🪝</span>
            <p class="text-sm font-medium text-amber-800 dark:text-amber-200">Hooks</p>
          </span>
          <span class="flex items-center gap-2">
            <span class="text-xs text-amber-500 dark:text-amber-400">{visibleNames.length} hooks</span>
            <span class="text-xs text-gray-400">{expandedCards.has('hooks') ? '▲' : '▼'}</span>
          </span>
        </button>
        {#if expandedCards.has('hooks')}
        <div class="p-3">
          {#if artifactToolCols.length > 0}
            <div class="grid gap-1 mb-1" style="grid-template-columns: 1fr {artifactToolCols.map(() => '6rem').join(' ')}">
              <span class="text-xs text-gray-400 dark:text-gray-500">Hook</span>
              {#each artifactToolCols as col}
                {@const hint = destHint('hook', col.key)}
                <span class="text-center">
                  <span class="text-xs text-gray-400 dark:text-gray-500 block">{col.label}</span>
                  {#if hint}
                    <span class="text-[10px] text-gray-400/60 dark:text-gray-600 font-mono block truncate" title={hint}>{hint}</span>
                  {/if}
                </span>
              {/each}
            </div>
          {/if}
          <div class="space-y-1">
            {#each visibleNames as name}
              {@const tools = grouped.toolMap.get(name)!}
              <div class="grid gap-1 items-center" style="grid-template-columns: 1fr {artifactToolCols.map(() => '6rem').join(' ')}">
                <span class="text-xs font-mono text-gray-700 dark:text-gray-300 truncate">{name}</span>
                {#each artifactToolCols as col}
                  <span class="text-center">
                    {#if tools.has(col.key)}
                      <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-green-100 dark:bg-green-900/30 text-green-600 dark:text-green-400">+ new</span>
                    {:else}
                      <span class="text-gray-300 dark:text-gray-600">—</span>
                    {/if}
                  </span>
                {/each}
              </div>
            {/each}
          </div>
        </div>
        {/if}
      </div>
      {/if}
    {/if}

    <!-- AGENTS detail -->
    {#if allAgents.length > 0}
      {@const grouped = groupByNameAndTool(allAgents)}
      {@const visibleNames = grouped.names.filter(n => {
        const tools = grouped.toolMap.get(n)!;
        return artifactToolCols.some(c => tools.has(c.key));
      })}
      {#if visibleNames.length > 0}
      <div class="border border-rose-200 dark:border-rose-800 rounded-lg overflow-hidden">
        <button onclick={() => toggleCard('agents')} class="w-full px-3 py-2 bg-rose-50 dark:bg-rose-900/20 flex items-center justify-between cursor-pointer hover:bg-rose-100 dark:hover:bg-rose-900/30 transition-colors">
          <span class="flex items-center gap-2">
            <span class="text-sm">🤖</span>
            <p class="text-sm font-medium text-rose-800 dark:text-rose-200">Agents</p>
          </span>
          <span class="flex items-center gap-2">
            <span class="text-xs text-rose-500 dark:text-rose-400">{visibleNames.length} agents</span>
            <span class="text-xs text-gray-400">{expandedCards.has('agents') ? '▲' : '▼'}</span>
          </span>
        </button>
        {#if expandedCards.has('agents')}
        <div class="p-3">
          {#if artifactToolCols.length > 0}
            <div class="grid gap-1 mb-1" style="grid-template-columns: 1fr {artifactToolCols.map(() => '6rem').join(' ')}">
              <span class="text-xs text-gray-400 dark:text-gray-500">Agent</span>
              {#each artifactToolCols as col}
                {@const hint = destHint('agent', col.key)}
                <span class="text-center">
                  <span class="text-xs text-gray-400 dark:text-gray-500 block">{col.label}</span>
                  {#if hint}
                    <span class="text-[10px] text-gray-400/60 dark:text-gray-600 font-mono block truncate" title={hint}>{hint}</span>
                  {/if}
                </span>
              {/each}
            </div>
          {/if}
          <div class="space-y-1">
            {#each visibleNames as name}
              {@const tools = grouped.toolMap.get(name)!}
              <div class="grid gap-1 items-center" style="grid-template-columns: 1fr {artifactToolCols.map(() => '6rem').join(' ')}">
                <span class="text-xs font-mono text-gray-700 dark:text-gray-300 truncate">{name}</span>
                {#each artifactToolCols as col}
                  <span class="text-center">
                    {#if tools.has(col.key)}
                      <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-green-100 dark:bg-green-900/30 text-green-600 dark:text-green-400">+ new</span>
                    {:else}
                      <span class="text-gray-300 dark:text-gray-600">—</span>
                    {/if}
                  </span>
                {/each}
              </div>
            {/each}
          </div>
        </div>
        {/if}
      </div>
      {/if}
    {/if}

    <!-- PACKAGES detail (home-scoped only) -->
    {#if allPackages.length > 0 && settingsStore.isArtifactEnabled("packages") && scopeIncludesHome}}
      {@const grouped = groupByNameAndTool(allPackages)}
      {@const visibleNames = grouped.names.filter(n => {
        const tools = grouped.toolMap.get(n)!;
        return artifactToolCols.some(c => tools.has(c.key));
      })}
      {#if visibleNames.length > 0}
      <div class="border border-violet-200 dark:border-violet-800 rounded-lg overflow-hidden">
        <button onclick={() => toggleCard('packages')} class="w-full px-3 py-2 bg-violet-50 dark:bg-violet-900/20 flex items-center justify-between cursor-pointer hover:bg-violet-100 dark:hover:bg-violet-900/30 transition-colors">
          <span class="flex items-center gap-2">
            <span class="text-sm">📦</span>
            <p class="text-sm font-medium text-violet-800 dark:text-violet-200">Packages</p>
          </span>
          <span class="flex items-center gap-2">
            <span class="text-xs text-violet-500 dark:text-violet-400">{visibleNames.length} packages</span>
            <span class="text-xs text-gray-400">{expandedCards.has('packages') ? '▲' : '▼'}</span>
          </span>
        </button>
        {#if expandedCards.has('packages')}
        <div class="p-3">
          {#if artifactToolCols.length > 0}
            <div class="grid gap-1 mb-1" style="grid-template-columns: 1fr {artifactToolCols.map(() => '6rem').join(' ')}">
              <span class="text-xs text-gray-400 dark:text-gray-500">Package</span>
              {#each artifactToolCols as col}
                {@const hint = destHint('package', col.key)}
                <span class="text-center">
                  <span class="text-xs text-gray-400 dark:text-gray-500 block">{col.label}</span>
                  {#if hint}
                    <span class="text-[10px] text-gray-400/60 dark:text-gray-600 font-mono block truncate" title={hint}>{hint}</span>
                  {/if}
                </span>
              {/each}
            </div>
          {/if}
          <div class="space-y-1">
            {#each visibleNames as name}
              {@const tools = grouped.toolMap.get(name)!}
              <div class="grid gap-1 items-center" style="grid-template-columns: 1fr {artifactToolCols.map(() => '6rem').join(' ')}">
                <span class="text-xs font-mono text-gray-700 dark:text-gray-300 truncate">{name}</span>
                {#each artifactToolCols as col}
                  <span class="text-center">
                    {#if tools.has(col.key)}
                      <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-green-100 dark:bg-green-900/30 text-green-600 dark:text-green-400">+ new</span>
                    {:else}
                      <span class="text-gray-300 dark:text-gray-600">—</span>
                    {/if}
                  </span>
                {/each}
              </div>
            {/each}
          </div>
        </div>
        {/if}
      </div>
      {/if}
    {/if}

    <!-- OLAF DATA detail -->
    <!-- OLAF DATA detail (repo-scoped only) -->
    {#if hasOlafData && scopeIncludesRepo}
      <div class="border border-teal-200 dark:border-teal-800 rounded-lg overflow-hidden">
        <button onclick={() => toggleCard('olafData')} class="w-full px-3 py-2 bg-teal-50 dark:bg-teal-900/20 flex items-center justify-between cursor-pointer hover:bg-teal-100 dark:hover:bg-teal-900/30 transition-colors">
          <span class="flex items-center gap-2">
            <span class="text-sm">🗂</span>
            <p class="text-sm font-medium text-teal-800 dark:text-teal-200">.olaf/data</p>
          </span>
          <span class="flex items-center gap-2">
            <span class="text-xs text-teal-500 dark:text-teal-400">Knowledge base</span>
            <span class="text-xs text-gray-400">{expandedCards.has('olafData') ? '▲' : '▼'}</span>
          </span>
        </button>
        {#if expandedCards.has('olafData')}
        <div class="p-3">
          <p class="text-xs text-gray-500 dark:text-gray-400">
            Knowledge base folders (product/, practices/, people/, project/) from the registry will be merged into
            <span class="font-mono">{wizardStore.repoPaths[0] || "your repo"}/.olaf/data/</span>.
            Empty or placeholder-only folders are skipped.
          </p>
        </div>
        {/if}
      </div>
    {/if}

    <!-- 5. SKILLS PER COMPETENCY -->
    <div class="space-y-3">
      {#each allCompetencies as comp}
        {@const detail = componentsStore.competencyDetails[comp.id]}
        {#if detail && detail.skills.length > 0}
          <div class="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
            <button
              onclick={() => {
                const next = new Set(expandedCompetencies);
                if (next.has(comp.id)) next.delete(comp.id); else next.add(comp.id);
                expandedCompetencies = next;
              }}
              class="w-full px-3 py-2 bg-gray-50 dark:bg-gray-800/80 flex items-center justify-between cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
            >
              <p class="text-sm font-medium text-gray-800 dark:text-gray-200">{comp.name}</p>
              <span class="flex items-center gap-2">
                <span class="text-xs text-gray-400 dark:text-gray-500">{detail.skills.length} skills</span>
                <span class="text-xs text-gray-400">{expandedCompetencies.has(comp.id) ? '▲' : '▼'}</span>
              </span>
            </button>
            {#if expandedCompetencies.has(comp.id)}
            <div class="p-3">
              <!-- Column headers: skill name + one col per location -->
              {#if skillTableCols.length > 0}
                <div class="grid gap-1 mb-1" style="grid-template-columns: 1fr {skillTableCols.map(() => '6rem').join(' ')}">
                  <span class="text-xs text-gray-400 dark:text-gray-500">Skill</span>
                  {#each skillTableCols as col}
                    <span class="text-xs text-gray-400 dark:text-gray-500 text-center capitalize">{col.label}</span>
                  {/each}
                </div>
              {/if}
              <div class="space-y-1">
                {#each detail.skills as skill}
                  {@const sm = skillStatusMap[skill]}
                  <div class="grid gap-1 items-center" style="grid-template-columns: 1fr {skillTableCols.map(() => '6rem').join(' ')}">
                    <span class="text-xs font-mono text-gray-700 dark:text-gray-300 truncate flex items-center gap-1">
                      {skill}
                      <button
                        onclick={() => openUrl(skillDocsUrl(skill))}
                        title="View skill documentation"
                        class="flex-shrink-0 w-4 h-4 text-gray-400 hover:text-blue-500 dark:hover:text-blue-400 transition-colors"
                      >
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4">
                          <path fill-rule="evenodd" d="M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0Zm-7-4a1 1 0 1 1-2 0 1 1 0 0 1 2 0ZM9 9a.75.75 0 0 0 0 1.5h.253a.25.25 0 0 1 .244.304l-.459 2.066A1.75 1.75 0 0 0 10.747 15H11a.75.75 0 0 0 0-1.5h-.253a.25.25 0 0 1-.244-.304l.459-2.066A1.75 1.75 0 0 0 9.253 9H9Z" clip-rule="evenodd" />
                        </svg>
                      </button>
                    </span>
                    {#each skillTableCols as col}
                      {@const status = col.key === "repo" ? (sm?.repo ?? "new") : (sm?.home[col.key] ?? "new")}
                      <span class="text-center">
                        {#if status === "reinstall"}
                          <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-orange-100 dark:bg-orange-900/30 text-orange-600 dark:text-orange-400">↺ reinstall</span>
                        {:else if status === "outdated"}
                          <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-yellow-100 dark:bg-yellow-900/30 text-yellow-600 dark:text-yellow-400">↑ update</span>
                        {:else if status === "installed"}
                          <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-blue-100 dark:bg-blue-900/30 text-blue-600 dark:text-blue-400">✓ installed</span>
                        {:else}
                          <span class="inline-block px-1.5 py-0.5 text-xs rounded bg-green-100 dark:bg-green-900/30 text-green-600 dark:text-green-400">+ new</span>
                        {/if}
                      </span>
                    {/each}
                  </div>
                {/each}
              </div>
            </div>
            {/if}
          </div>
        {/if}
      {/each}
    </div>

    {#if homeTools.length === 0 && wizardStore.installScope === "home"}
      <div class="p-3 rounded-lg bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 text-xs text-amber-700 dark:text-amber-400">
        No AI tools detected on this machine. Skills will be installed to default locations.
      </div>
    {/if}

    <!-- REQUIREMENTS PANEL -->
    {#if requirements.length > 0}
      {@const totalIssues = requirements.reduce((n, r) =>
        n + r.runtimes.filter(x => x.status !== "ok").length + r.mcp.filter(x => x.status === "missing").length, 0)}
      <div class="border rounded-lg overflow-hidden
        {requirements.some(r => r.runtimes.some(x => x.status === 'missing'))
          ? 'border-red-300 dark:border-red-700'
          : 'border-yellow-300 dark:border-yellow-700'}">
        <!-- Header — clickable to expand -->
        <button
          onclick={() => requirementsOpen = !requirementsOpen}
          class="w-full flex items-center gap-2 px-3 py-2 text-left
            {requirements.some(r => r.runtimes.some(x => x.status === 'missing'))
              ? 'bg-red-50 dark:bg-red-900/20'
              : 'bg-yellow-50 dark:bg-yellow-900/20'}"
        >
          <span class="text-sm">
            {requirements.some(r => r.runtimes.some(x => x.status === 'missing')) ? '⛔' : '⚠️'}
          </span>
          <span class="text-sm font-medium
            {requirements.some(r => r.runtimes.some(x => x.status === 'missing'))
              ? 'text-red-700 dark:text-red-300'
              : 'text-yellow-700 dark:text-yellow-300'}">
            {totalIssues} requirement{totalIssues !== 1 ? 's' : ''} need attention
          </span>
          <span class="ml-auto text-xs text-gray-400">
            {requirementsOpen ? '▲ hide' : '▼ show'}
          </span>
        </button>

        {#if requirementsOpen}
          <div class="p-3 space-y-4 bg-white dark:bg-gray-900">
            {#each requirements as req}
              <div>
                <p class="text-xs font-medium text-gray-600 dark:text-gray-400 mb-1.5">
                  <span class="font-mono text-gray-800 dark:text-gray-200">{req.componentId}</span>
                  <span class="ml-1 text-gray-400">({req.componentType})</span>
                </p>
                <div class="space-y-1 pl-2">
                  {#each req.runtimes.filter(r => r.status !== 'ok') as rt}
                    <div class="flex items-start gap-2">
                      <span class="flex-shrink-0 mt-0.5">
                        {rt.status === 'missing' ? '🔴' : '🟡'}
                      </span>
                      <div class="min-w-0">
                        <span class="text-xs font-mono font-medium text-gray-800 dark:text-gray-200">{rt.name}</span>
                        {#if rt.requiredVersion}
                          <span class="text-xs text-gray-400 ml-1">≥ {rt.requiredVersion}</span>
                        {/if}
                        {#if rt.foundVersion}
                          <span class="text-xs text-yellow-600 dark:text-yellow-400 ml-1">(found {rt.foundVersion})</span>
                        {:else}
                          <span class="text-xs text-red-500 ml-1">not found</span>
                        {/if}
                        {#if rt.installHint}
                          <p class="text-xs text-gray-400 dark:text-gray-500 mt-0.5">{rt.installHint}</p>
                        {/if}
                      </div>
                    </div>
                  {/each}
                  {#each req.mcp.filter(m => m.status === 'missing') as mc}
                    <div class="flex items-start gap-2">
                      <span class="flex-shrink-0 mt-0.5">🟡</span>
                      <div>
                        <span class="text-xs font-mono text-gray-800 dark:text-gray-200">MCP: {mc.id}</span>
                        <span class="text-xs text-yellow-600 dark:text-yellow-400 ml-1">not in this install</span>
                      </div>
                    </div>
                  {/each}
                  {#if req.hasPip}
                    <div class="flex items-center gap-2">
                      <span>🐍</span>
                      <span class="text-xs text-gray-500 dark:text-gray-400">Has <span class="font-mono">requirements.txt</span> — run <span class="font-mono">pip install -r requirements.txt</span> after install</span>
                    </div>
                  {/if}
                  {#if req.hasNpm}
                    <div class="flex items-center gap-2">
                      <span>📦</span>
                      <span class="text-xs text-gray-500 dark:text-gray-400">Has <span class="font-mono">package.json</span> — run <span class="font-mono">npm install</span> after install</span>
                    </div>
                  {/if}
                  {#if req.notes}
                    <p class="text-xs text-gray-500 dark:text-gray-400 italic mt-1">{req.notes}</p>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <div class="flex items-center justify-between">
      <!-- Install mode toggles -->
      <div class="flex items-center gap-4">
        <div class="flex items-center gap-2">
          <button
            onclick={() => wizardStore.setReinstallAll(!reinstallAll)}
            class="relative inline-flex h-5 w-9 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors {reinstallAll ? 'bg-orange-500' : 'bg-gray-300 dark:bg-gray-600'}"
            role="switch" aria-checked={reinstallAll}
          >
            <span class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow transition {reinstallAll ? 'translate-x-4' : 'translate-x-0'}"></span>
          </button>
          <span class="text-xs text-gray-600 dark:text-gray-400">
            Re-install all — <span class="text-gray-400 dark:text-gray-500">{reinstallAll ? "overwrite existing" : "skip existing"}</span>
          </span>
        </div>
        <div class="flex items-center gap-2">
          <button
            onclick={() => wizardStore.setCleanInstall(!cleanInstall)}
            class="relative inline-flex h-5 w-9 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors {cleanInstall ? 'bg-red-500' : 'bg-gray-300 dark:bg-gray-600'}"
            role="switch" aria-checked={cleanInstall}
          >
            <span class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow transition {cleanInstall ? 'translate-x-4' : 'translate-x-0'}"></span>
          </button>
          <span class="text-xs text-gray-600 dark:text-gray-400">
            Clean install — <span class="text-gray-400 dark:text-gray-500">{cleanInstall ? "remove unselected skills" : "keep existing"}</span>
          </span>
        </div>
      </div>
      <button
        onclick={handleConfirm}
        disabled={
          (wizardStore.installScope !== "home" && wizardStore.repoPaths.length === 0) ||
          (wizardStore.installScope === "home" && selectedTools.size === 0)
        }
        class="px-5 py-2.5 text-sm font-medium rounded-lg bg-green-600 text-white hover:bg-green-700 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
      >
        {$_("wizard.preview.confirm")}
      </button>
    </div>
  {/if}
</div>
