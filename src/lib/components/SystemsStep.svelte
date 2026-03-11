<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { componentsStore, type SystemEntry } from "../stores/componentsStore.svelte";

  interface Props {
    onNavigate?: (view: string) => void;
  }

  let { onNavigate }: Props = $props();

  // --- Types ---

  interface SystemDef {
    id: string;
    name: string;
    description: string;
    version: string;
    prerequisites: {
      runtimes: string[];
      pip: boolean;
      npm: boolean;
      env: string[];
      notes?: string;
    };
    postInstall?: {
      commands: string[];
      message?: string;
    };
  }

  type SystemStatus = "NotInstalled" | "Installed" | "UpdateAvailable";

  interface InstalledSystemInfo {
    id: string;
    name: string;
    installPath: string;
    status: SystemStatus;
    currentCommit?: string;
  }

  // --- State ---

  let systemsRoot = $state("");
  let installedInfo = $state<Record<string, InstalledSystemInfo>>({});
  let systemDefs = $state<Record<string, SystemDef>>({});
  let loadingDefs = $state<Set<string>>(new Set());
  let actionPending = $state<Record<string, string>>({}); // id → "installing"|"updating"|"deleting"
  let postInstallSteps = $state<Record<string, string[]>>({});
  let errors = $state<Record<string, string>>({});
  let expandedId = $state<string | null>(null);

  let systems = $derived(componentsStore.mergedCatalog?.systems ?? []);
  let isConnected = $derived(componentsStore.mergedCatalog !== null);

  // --- Load on mount ---

  async function loadAll() {
    if (!isConnected) return;

    const [root] = await Promise.allSettled([invoke<string>("get_systems_root")]);
    if (root.status === "fulfilled") systemsRoot = root.value;
    if (systems.length === 0) return;

    // Scan installed status
    const info = await invoke<InstalledSystemInfo[]>("scan_installed_systems", {
      systems: systems.map(s => ({
        id: s.id, name: s.name, description: s.description,
        repo: s.repo, branch: s.branch ?? null, tags: s.tags,
      })),
    }).catch(() => [] as InstalledSystemInfo[]);

    const map: Record<string, InstalledSystemInfo> = {};
    for (const i of info) map[i.id] = i;
    installedInfo = map;

    // Fetch system.json for each in parallel
    await Promise.allSettled(systems.map(s => fetchDef(s)));
  }

  async function fetchDef(entry: SystemEntry) {
    if (systemDefs[entry.id]) return;
    const next = new Set(loadingDefs); next.add(entry.id); loadingDefs = next;
    try {
      const def = await invoke<SystemDef>("fetch_system_def", {
        id: entry.id, repo: entry.repo, branch: entry.branch ?? null,
      });
      systemDefs = { ...systemDefs, [entry.id]: def };
    } catch {
      // system.json not found — use minimal info from manifest entry
    } finally {
      const n = new Set(loadingDefs); n.delete(entry.id); loadingDefs = n;
    }
  }

  async function install(entry: SystemEntry) {
    errors = { ...errors, [entry.id]: "" };
    actionPending = { ...actionPending, [entry.id]: "installing" };
    try {
      const path = await invoke<string>("install_system", {
        id: entry.id, repo: entry.repo, branch: entry.branch ?? null,
      });
      // Get post-install steps
      const def = systemDefs[entry.id];
      if (def) {
        const steps = await invoke<string[]>("get_post_install_steps", {
          def, installPath: path,
        });
        if (steps.length > 0) postInstallSteps = { ...postInstallSteps, [entry.id]: steps };
      }
      // Refresh status
      await refreshStatus(entry);
    } catch (e: any) {
      errors = { ...errors, [entry.id]: String(e) };
    } finally {
      const p = { ...actionPending }; delete p[entry.id]; actionPending = p;
    }
  }

  async function update(entry: SystemEntry) {
    errors = { ...errors, [entry.id]: "" };
    actionPending = { ...actionPending, [entry.id]: "updating" };
    try {
      await invoke("update_system", { id: entry.id });
      await refreshStatus(entry);
    } catch (e: any) {
      errors = { ...errors, [entry.id]: String(e) };
    } finally {
      const p = { ...actionPending }; delete p[entry.id]; actionPending = p;
    }
  }

  async function remove(entry: SystemEntry) {
    errors = { ...errors, [entry.id]: "" };
    actionPending = { ...actionPending, [entry.id]: "deleting" };
    try {
      await invoke("delete_system", { id: entry.id });
      const map = { ...installedInfo };
      if (map[entry.id]) map[entry.id] = { ...map[entry.id], status: "NotInstalled", currentCommit: undefined };
      installedInfo = map;
      const ps = { ...postInstallSteps }; delete ps[entry.id]; postInstallSteps = ps;
    } catch (e: any) {
      errors = { ...errors, [entry.id]: String(e) };
    } finally {
      const p = { ...actionPending }; delete p[entry.id]; actionPending = p;
    }
  }

  async function refreshStatus(entry: SystemEntry) {
    const info = await invoke<InstalledSystemInfo[]>("scan_installed_systems", {
      systems: [{ id: entry.id, name: entry.name, description: entry.description,
        repo: entry.repo, branch: entry.branch ?? null, tags: entry.tags }],
    }).catch(() => [] as InstalledSystemInfo[]);
    if (info[0]) installedInfo = { ...installedInfo, [entry.id]: info[0] };
  }

  loadAll();
</script>

<div class="space-y-5">
  <div>
    <h2 class="text-xl font-semibold text-gray-900 dark:text-gray-100">Agentic Systems</h2>
    <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
      Standalone AI applications. Each is cloned to
      <span class="font-mono text-xs">{systemsRoot || "~/.haal/systems/"}</span>
    </p>
  </div>

  {#if !isConnected}
    <div class="flex flex-col items-center justify-center py-16 gap-4 text-center">
      <svg class="w-12 h-12 text-gray-300 dark:text-gray-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M13 10V3L4 14h7v7l9-11h-7z"/>
      </svg>
      <p class="text-sm text-gray-500 dark:text-gray-400">Connect to a registry first to browse available systems.</p>
      <button
        onclick={() => { wizardStore.setStep("connect"); onNavigate?.("wizard"); }}
        class="px-4 py-2 text-sm font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700"
      >
        Go to Connect
      </button>
    </div>
  {:else if catalogLoading}
    <div class="flex items-center gap-3 py-12 justify-center text-gray-400 dark:text-gray-500 text-sm">
      <svg class="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
      </svg>
      Loading registry…
    </div>
  {:else if catalogError}
    <div class="py-12 text-center text-sm text-red-500 dark:text-red-400">⛔ {catalogError}</div>
  {:else if systems.length === 0}
    <div class="text-center py-12 text-gray-400 dark:text-gray-500 text-sm">
      No systems found in the registry.
    </div>
  {:else}
    <div class="space-y-3">
      {#each systems as entry}
        {@const info = installedInfo[entry.id]}
        {@const def = systemDefs[entry.id]}
        {@const pending = actionPending[entry.id]}
        {@const steps = postInstallSteps[entry.id] ?? []}
        {@const err = errors[entry.id]}
        {@const isExpanded = expandedId === entry.id}

        <div class="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
          <!-- Header row -->
          <div class="px-4 py-3 flex items-start gap-3 bg-white dark:bg-gray-800">
            <!-- Status dot -->
            <span class="mt-1 w-2.5 h-2.5 rounded-full flex-shrink-0
              {info?.status === 'Installed' ? 'bg-green-500' :
               info?.status === 'UpdateAvailable' ? 'bg-yellow-400' :
               'bg-gray-300 dark:bg-gray-600'}">
            </span>

            <!-- Info -->
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 flex-wrap">
                <span class="text-sm font-semibold text-gray-900 dark:text-gray-100">{entry.name}</span>
                {#if info?.status === 'Installed'}
                  <span class="text-xs px-1.5 py-0.5 rounded bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-400">
                    ✓ installed{info.currentCommit ? ` · ${info.currentCommit}` : ""}
                  </span>
                {:else if info?.status === 'UpdateAvailable'}
                  <span class="text-xs px-1.5 py-0.5 rounded bg-yellow-100 dark:bg-yellow-900/30 text-yellow-700 dark:text-yellow-400">
                    ↑ update available
                  </span>
                {/if}
                {#each entry.tags as tag}
                  <span class="text-xs px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-700 text-gray-500 dark:text-gray-400">{tag}</span>
                {/each}
              </div>
              <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{entry.description}</p>
              {#if def && loadingDefs.has(entry.id) === false}
                <!-- Prerequisites summary -->
                <div class="flex flex-wrap gap-2 mt-1.5">
                  {#each def.prerequisites.runtimes as rt}
                    <span class="text-xs font-mono px-1.5 py-0.5 rounded bg-blue-50 dark:bg-blue-900/20 text-blue-600 dark:text-blue-400">{rt}</span>
                  {/each}
                  {#if def.prerequisites.pip}
                    <span class="text-xs font-mono px-1.5 py-0.5 rounded bg-amber-50 dark:bg-amber-900/20 text-amber-600 dark:text-amber-400">pip</span>
                  {/if}
                  {#each def.prerequisites.env as envVar}
                    <span class="text-xs font-mono px-1.5 py-0.5 rounded bg-purple-50 dark:bg-purple-900/20 text-purple-600 dark:text-purple-400">${envVar}</span>
                  {/each}
                </div>
              {:else if loadingDefs.has(entry.id)}
                <span class="text-xs text-gray-400 mt-1">Loading details…</span>
              {/if}
            </div>

            <!-- Actions -->
            <div class="flex items-center gap-2 flex-shrink-0">
              {#if !info || info.status === 'NotInstalled'}
                <button
                  onclick={() => install(entry)}
                  disabled={!!pending}
                  class="px-3 py-1.5 text-xs font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  {pending === "installing" ? "Installing…" : "Install"}
                </button>
              {:else}
                {#if info.status === 'UpdateAvailable'}
                  <button
                    onclick={() => update(entry)}
                    disabled={!!pending}
                    class="px-3 py-1.5 text-xs font-medium rounded-lg bg-yellow-500 text-white hover:bg-yellow-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                  >
                    {pending === "updating" ? "Updating…" : "Update"}
                  </button>
                {/if}
                <button
                  onclick={() => remove(entry)}
                  disabled={!!pending}
                  class="px-3 py-1.5 text-xs font-medium rounded-lg border border-red-300 dark:border-red-700 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  {pending === "deleting" ? "Removing…" : "Remove"}
                </button>
              {/if}
              <!-- Expand toggle -->
              {#if def}
                <button
                  onclick={() => expandedId = isExpanded ? null : entry.id}
                  class="p-1.5 rounded text-gray-400 hover:text-gray-600 dark:hover:text-gray-300"
                  aria-label="Toggle details"
                >
                  <svg class="w-4 h-4 transition-transform {isExpanded ? 'rotate-180' : ''}" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
                  </svg>
                </button>
              {/if}
            </div>
          </div>

          <!-- Expanded details -->
          {#if isExpanded && def}
            <div class="px-4 py-3 border-t border-gray-100 dark:border-gray-700 bg-gray-50 dark:bg-gray-800/50 space-y-3">
              <!-- Install path -->
              {#if info && info.status !== 'NotInstalled'}
                <div>
                  <p class="text-xs text-gray-500 dark:text-gray-400 mb-0.5">Installed at</p>
                  <p class="text-xs font-mono text-gray-700 dark:text-gray-300">{info.installPath}</p>
                </div>
              {/if}

              <!-- Env vars required -->
              {#if def.prerequisites.env.length > 0}
                <div>
                  <p class="text-xs font-medium text-gray-600 dark:text-gray-400 mb-1">Required environment variables</p>
                  <div class="space-y-0.5">
                    {#each def.prerequisites.env as envVar}
                      <p class="text-xs font-mono text-purple-700 dark:text-purple-300">${envVar}</p>
                    {/each}
                  </div>
                </div>
              {/if}

              <!-- Notes -->
              {#if def.prerequisites.notes}
                <p class="text-xs text-amber-700 dark:text-amber-400 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded px-2 py-1.5">
                  ⚠ {def.prerequisites.notes}
                </p>
              {/if}

              <!-- Post-install steps -->
              {#if steps.length > 0}
                <div>
                  <p class="text-xs font-medium text-gray-600 dark:text-gray-400 mb-1">Run after install</p>
                  <div class="space-y-1">
                    {#each steps as step}
                      <p class="text-xs font-mono bg-gray-900 dark:bg-gray-950 text-green-400 rounded px-2 py-1">{step}</p>
                    {/each}
                  </div>
                </div>
              {:else if def.postInstall?.commands?.length}
                <div>
                  <p class="text-xs font-medium text-gray-600 dark:text-gray-400 mb-1">After install, run</p>
                  <div class="space-y-1">
                    {#each def.postInstall.commands as cmd}
                      <p class="text-xs font-mono bg-gray-900 dark:bg-gray-950 text-green-400 rounded px-2 py-1">{cmd}</p>
                    {/each}
                  </div>
                </div>
              {/if}

              <!-- Post-install message -->
              {#if def.postInstall?.message}
                <p class="text-xs text-green-700 dark:text-green-400 bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded px-2 py-1.5">
                  💡 {def.postInstall.message}
                </p>
              {/if}

              <!-- Source link -->
              <a href={entry.repo} target="_blank" rel="noopener"
                class="inline-flex items-center gap-1 text-xs text-blue-600 dark:text-blue-400 hover:underline">
                <svg class="w-3 h-3" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/>
                </svg>
                View on GitHub
              </a>
            </div>
          {/if}

          <!-- Error -->
          {#if err}
            <div class="px-4 py-2 border-t border-red-200 dark:border-red-800 bg-red-50 dark:bg-red-900/20">
              <p class="text-xs text-red-600 dark:text-red-400">⛔ {err}</p>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
