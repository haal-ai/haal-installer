// Types matching Rust models (camelCase)
import { settingsStore } from "./settingsStore.svelte";

export interface CollectionEntry {
  id: string;
  name: string;
  description: string;
  competencyIds: string[];
}

export interface CompetencyEntry {
  id: string;
  name: string;
  description: string;
  manifestUrl: string;
}

export interface CompetencyDetail {
  name: string;
  description: string;
  skills: string[];
  powers: string[];
  hooks: string[];
  commands: string[];
  rules: string[];
  agents: string[];
  mcpServers: string[];
  systems: string[];
  packages: string[];
}

export interface McpServerDef {
  id: string;
  name: string;
  description: string;
  transport: "http" | "stdio";
  serverUrl?: string;
  command?: string;
  args: string[];
  env: Record<string, string>;
  scope: string[];
}

export interface HaalManifest {
  version: string;
  repoId: string;
  description: string;
  baseUrl: string;
  collections: CollectionEntry[];
  competencies: CompetencyEntry[];
}

/** Merged catalog returned by initialize_catalog */
export interface MergedCatalog {
  collections: CollectionEntry[];
  competencies: CompetencyEntry[];
  /** competency_id → local path of the source repo */
  competencySources: Record<string, string>;
  /** Agentic systems from all registries */
  systems: SystemEntry[];
}

export interface SystemEntry {
  id: string;
  name: string;
  description: string;
  repo: string;
  branch?: string;
  tags: string[];
}

/** A resolved component ready for install */
export interface ResolvedComponent {
  id: string;
  componentType: string;
  sourcePath: string;
}

function createComponentsStore() {
  let manifest = $state<HaalManifest | null>(null);
  let mergedCatalog = $state<MergedCatalog | null>(null);
  // competency id -> detail (loaded lazily)
  let competencyDetails = $state<Record<string, CompetencyDetail>>({});
  let loadingCompetencies = $state<Set<string>>(new Set());
  let selected = $state<Set<string>>(new Set()); // keys: "collection:x" | "competency:x"
  let loading = $state(false);
  let searchQuery = $state("");

  return {
    get manifest() { return manifest; },
    get mergedCatalog() { return mergedCatalog; },
    get competencyDetails() { return competencyDetails; },
    get loadingCompetencies() { return loadingCompetencies; },
    get selected() { return selected; },
    get loading() { return loading; },
    get searchQuery() { return searchQuery; },
    get selectedCount() { return selected.size; },

    // Unified collections/competencies — prefer mergedCatalog, fall back to manifest
    get collections(): CollectionEntry[] {
      return mergedCatalog?.collections ?? manifest?.collections ?? [];
    },
    get competencies(): CompetencyEntry[] {
      return mergedCatalog?.competencies ?? manifest?.competencies ?? [];
    },

    setManifest(m: HaalManifest) {
      manifest = m;
    },

    setMergedCatalog(c: MergedCatalog) {
      mergedCatalog = c;
    },

    setCompetencyDetail(id: string, detail: CompetencyDetail) {
      competencyDetails = { ...competencyDetails, [id]: detail };
      const next = new Set(loadingCompetencies);
      next.delete(id);
      loadingCompetencies = next;
    },

    setCompetencyLoading(id: string, value: boolean) {
      const next = new Set(loadingCompetencies);
      if (value) next.add(id); else next.delete(id);
      loadingCompetencies = next;
    },

    setLoading(value: boolean) { loading = value; },
    setSearchQuery(q: string) { searchQuery = q; },

    toggle(key: string) {
      const next = new Set(selected);
      if (next.has(key)) next.delete(key); else next.add(key);
      selected = next;
    },

    isSelected(key: string) { return selected.has(key); },

    // Returns all competency IDs that are effectively selected
    get resolvedCompetencyIds(): string[] {
      const cols = mergedCatalog?.collections ?? manifest?.collections ?? [];
      const ids = new Set<string>();
      for (const key of selected) {
        if (key.startsWith("collection:")) {
          const colId = key.slice("collection:".length);
          const col = cols.find(c => c.id === colId);
          col?.competencyIds.forEach(id => ids.add(id));
        } else if (key.startsWith("competency:")) {
          ids.add(key.slice("competency:".length));
        }
      }
      return Array.from(ids);
    },

    /** Build ResolvedComponent list from selected competency details + catalog sources */
    buildResolvedComponents(): ResolvedComponent[] {
      const sources = mergedCatalog?.competencySources ?? {};
      const comps: ResolvedComponent[] = [];
      const seen = new Set<string>();

      // Which artifact types are enabled in settings
      const enabled = settingsStore.enabledArtifacts;

      for (const compId of this.resolvedCompetencyIds) {
        const detail = competencyDetails[compId];
        if (!detail) continue;
        const sourceRepo = sources[compId] ?? "";

        const addComp = (id: string, type: string, subdir: string) => {
          const key = `${type}:${id}`;
          if (seen.has(key)) return;
          seen.add(key);
          comps.push({
            id,
            componentType: type,
            sourcePath: sourceRepo ? `${sourceRepo}/${subdir}/${id}` : "",
          });
        };

        if (enabled.has("skills"))     for (const s of detail.skills ?? [])     addComp(s, "skill",     "skills");
        if (enabled.has("powers"))     for (const p of detail.powers ?? [])     addComp(p, "power",     "powers");
        if (enabled.has("rules"))      for (const r of detail.rules ?? [])      addComp(r, "rule",      "rules");
        if (enabled.has("hooks"))      for (const h of detail.hooks ?? [])      addComp(h, "hook",      "hooks");
        if (enabled.has("commands"))   for (const c of detail.commands ?? [])   addComp(c, "command",   "commands");
        if (enabled.has("agents"))     for (const a of detail.agents ?? [])     addComp(a, "agent",     "agents");
        if (enabled.has("mcpServers")) for (const m of detail.mcpServers ?? []) addComp(m, "mcpServer", "mcpservers");
        if (enabled.has("packages"))   for (const p of detail.packages ?? [])   addComp(p, "package",   "packages");

        if (enabled.has("systems")) {
          for (const s of detail.systems ?? []) {
            const sysEntry = mergedCatalog?.systems.find(sys => sys.id === s);
            if (sysEntry && !seen.has(`system:${s}`)) {
              seen.add(`system:${s}`);
              comps.push({ id: s, componentType: "system", sourcePath: sysEntry.repo });
            }
          }
        }

        // OlafData: driven by settings, not competency flag.
        // Emitted once per source repo (deduplicated by sourceRepo).
        if (enabled.has("olafData") && sourceRepo) {
          const key = `olafData:${sourceRepo}`;
          if (!seen.has(key)) {
            seen.add(key);
            comps.push({
              id: compId,
              componentType: "olafData",
              sourcePath: `${sourceRepo}/.olaf`,
            });
          }
        }
      }

      return comps;
    },

    reset() {
      manifest = null;
      mergedCatalog = null;
      competencyDetails = {};
      loadingCompetencies = new Set();
      selected = new Set();
      loading = false;
      searchQuery = "";
    },
  };
}

export const componentsStore = createComponentsStore();
