<script lang="ts">
  import { _ } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { componentsStore, type ComponentInfo } from "../stores/componentsStore";

  let detectedTools = $state<string[]>([]);
  let toolTargets = $state<Record<string, string[]>>({});
  let filterRepo = $state("all");
  let filterTool = $state("all");
  let expandedId = $state<string | null>(null);

  // Load components and detected tools on mount
  async function loadData() {
    componentsStore.setLoading(true);
    try {
      const [components, tools] = await Promise.all([
        invoke<ComponentInfo[]>("discover_components"),
        invoke<{ name: string }[]>("detect_tools"),
      ]);
      componentsStore.setAvailable(components);
      detectedTools = tools.map((t) => t.name);
    } catch {
      // Backend may not be ready
    } finally {
      componentsStore.setLoading(false);
    }
  }

  loadData();

  // Derived: unique categories, repos
  let categories = $derived(
    Array.from(new Set(componentsStore.available.map((c) => c.componentType)))
  );
  let repos = $derived(
    Array.from(new Set(componentsStore.available.map((c) => c.repository)))
  );

  // Derived: filtered components
  let filtered = $derived.by(() => {
    let list = componentsStore.available;
    const q = componentsStore.searchQuery.toLowerCase();
    if (q) {
      list = list.filter(
        (c) =>
          c.name.toLowerCase().includes(q) ||
          c.description.toLowerCase().includes(q) ||
          c.id.toLowerCase().includes(q)
      );
    }
    if (componentsStore.filterType !== "all") {
      list = list.filter((c) => c.componentType === componentsStore.filterType);
    }
    if (filterRepo !== "all") {
      list = list.filter((c) => c.repository === filterRepo);
    }
    if (filterTool !== "all") {
      list = list.filter((c) => c.compatibleTools.includes(filterTool));
    }
    return list;
  });

  // Grouped by type
  let grouped = $derived.by(() => {
    const groups: Record<string, ComponentInfo[]> = {};
    for (const c of filtered) {
      const key = c.componentType;
      if (!groups[key]) groups[key] = [];
      groups[key].push(c);
    }
    return groups;
  });

  // Build dependency map: component id -> list of component ids that depend on it
  let dependentsMap = $derived.by(() => {
    const map: Record<string, string[]> = {};
    for (const c of componentsStore.available) {
      for (const dep of c.dependencies) {
        if (!map[dep]) map[dep] = [];
        map[dep].push(c.id);
      }
    }
    return map;
  });

  function getTransitiveDeps(id: string, visited = new Set<string>()): string[] {
    const comp = componentsStore.available.find((c) => c.id === id);
    if (!comp) return [];
    const deps: string[] = [];
    for (const dep of comp.dependencies) {
      if (!visited.has(dep)) {
        visited.add(dep);
        deps.push(dep);
        deps.push(...getTransitiveDeps(dep, visited));
      }
    }
    return deps;
  }

  function hasSelectedDependents(id: string): boolean {
    const dependents = dependentsMap[id] ?? [];
    return dependents.some((d) => componentsStore.isSelected(d));
  }

  function handleToggle(comp: ComponentInfo) {
    if (comp.pinned) return;
    const isCurrentlySelected = componentsStore.isSelected(comp.id);

    if (!isCurrentlySelected) {
      // Selecting: auto-select dependencies
      componentsStore.toggleSelection(comp.id);
      const deps = getTransitiveDeps(comp.id);
      for (const dep of deps) {
        if (!componentsStore.isSelected(dep)) {
          componentsStore.toggleSelection(dep);
        }
      }
    } else {
      // Deselecting: prevent if dependents are selected
      if (hasSelectedDependents(comp.id)) return;
      componentsStore.toggleSelection(comp.id);
    }
  }

  function toggleToolTarget(componentId: string, tool: string) {
    const current = toolTargets[componentId] ?? [];
    if (current.includes(tool)) {
      toolTargets = { ...toolTargets, [componentId]: current.filter((t) => t !== tool) };
    } else {
      toolTargets = { ...toolTargets, [componentId]: [...current, tool] };
    }
  }
</script>

<div class="space-y-4">
  <div>
    <h2 class="text-xl font-semibold text-gray-900 dark:text-gray-100">
      {$_("wizard.choose.title")}
    </h2>
    <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
      {$_("wizard.choose.description")}
    </p>
  </div>

  {#if componentsStore.loading}
    <div class="text-center py-12 text-gray-500 dark:text-gray-400">
      <p>{$_("wizard.choose.loading")}</p>
    </div>
  {:else}
    <!-- Search and filters -->
    <div class="flex flex-wrap gap-3 items-center">
      <input
        type="text"
        value={componentsStore.searchQuery}
        oninput={(e) => componentsStore.setSearchQuery((e.target as HTMLInputElement).value)}
        placeholder={$_("wizard.choose.searchPlaceholder")}
        class="flex-1 min-w-[200px] px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
          bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
          focus:ring-2 focus:ring-blue-500 focus:border-blue-500 outline-none"
      />
      <!-- Category filter -->
      <select
        value={componentsStore.filterType}
        onchange={(e) => componentsStore.setFilterType((e.target as HTMLSelectElement).value)}
        class="px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
          bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100"
      >
        <option value="all">{$_("wizard.choose.filterAll")}</option>
        {#each categories as cat}
          <option value={cat}>{cat}</option>
        {/each}
      </select>
      <!-- Repo filter -->
      {#if repos.length > 1}
        <select
          value={filterRepo}
          onchange={(e) => (filterRepo = (e.target as HTMLSelectElement).value)}
          class="px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
            bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100"
        >
          <option value="all">{$_("wizard.choose.filterRepo")}</option>
          {#each repos as repo}
            <option value={repo}>{repo}</option>
          {/each}
        </select>
      {/if}
      <!-- Tool filter -->
      {#if detectedTools.length > 0}
        <select
          value={filterTool}
          onchange={(e) => (filterTool = (e.target as HTMLSelectElement).value)}
          class="px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
            bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100"
        >
          <option value="all">{$_("wizard.choose.filterTool")}</option>
          {#each detectedTools as tool}
            <option value={tool}>{tool}</option>
          {/each}
        </select>
      {/if}
    </div>

    <!-- Bulk actions -->
    <div class="flex items-center justify-between">
      <span class="text-sm text-gray-600 dark:text-gray-400">
        {$_("wizard.choose.selectedCount", { values: { count: componentsStore.selectedCount } })}
      </span>
      <div class="flex gap-2">
        <button
          onclick={() => componentsStore.selectAll()}
          class="px-3 py-1 text-xs font-medium rounded border border-gray-300 dark:border-gray-600
            text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800"
        >
          {$_("wizard.choose.selectAll")}
        </button>
        <button
          onclick={() => componentsStore.deselectAll()}
          class="px-3 py-1 text-xs font-medium rounded border border-gray-300 dark:border-gray-600
            text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800"
        >
          {$_("wizard.choose.deselectAll")}
        </button>
      </div>
    </div>

    <!-- Component list grouped by type -->
    {#if filtered.length === 0}
      <div class="text-center py-8 text-gray-500 dark:text-gray-400">
        <p>{$_("wizard.choose.noComponents")}</p>
      </div>
    {:else}
      <div class="space-y-6">
        {#each Object.entries(grouped) as [type, components]}
          <div>
            <h3 class="text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wide mb-2">
              {type}
            </h3>
            <div class="space-y-2">
              {#each components as comp}
                {@const isSelected = componentsStore.isSelected(comp.id)}
                {@const isDepLocked = hasSelectedDependents(comp.id)}
                {@const isExpanded = expandedId === comp.id}
                <div
                  class="border rounded-lg transition-colors
                    {isSelected
                      ? 'border-blue-300 dark:border-blue-700 bg-blue-50/50 dark:bg-blue-900/10'
                      : 'border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800'}"
                >
                  <div class="flex items-center gap-3 p-3">
                    <!-- Checkbox -->
                    <button
                      onclick={() => handleToggle(comp)}
                      disabled={comp.pinned || (isSelected && isDepLocked)}
                      class="flex-shrink-0"
                      aria-label="Toggle {comp.name}"
                    >
                      <div
                        class="w-5 h-5 rounded border-2 flex items-center justify-center
                          {comp.pinned
                            ? 'border-gray-300 dark:border-gray-600 bg-gray-100 dark:bg-gray-700 cursor-not-allowed'
                            : isSelected
                              ? 'border-blue-600 bg-blue-600 text-white'
                              : 'border-gray-300 dark:border-gray-600 hover:border-blue-400'}"
                      >
                        {#if comp.pinned}
                          <!-- Lock icon -->
                          <svg class="w-3 h-3 text-gray-400" fill="currentColor" viewBox="0 0 20 20">
                            <path fill-rule="evenodd" d="M5 9V7a5 5 0 0110 0v2a2 2 0 012 2v5a2 2 0 01-2 2H5a2 2 0 01-2-2v-5a2 2 0 012-2zm8-2v2H7V7a3 3 0 016 0z" clip-rule="evenodd" />
                          </svg>
                        {:else if isSelected}
                          <svg class="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                            <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd" />
                          </svg>
                        {/if}
                      </div>
                    </button>

                    <!-- Component info -->
                    <button
                      onclick={() => (expandedId = isExpanded ? null : comp.id)}
                      class="flex-1 text-left min-w-0"
                    >
                      <div class="flex items-center gap-2">
                        <span class="font-medium text-sm text-gray-900 dark:text-gray-100 truncate">
                          {comp.name}
                        </span>
                        {#if comp.pinned}
                          <span class="inline-flex items-center px-1.5 py-0.5 text-xs font-medium bg-yellow-100 dark:bg-yellow-900/30 text-yellow-700 dark:text-yellow-400 rounded">
                            {$_("wizard.choose.pinned")}
                          </span>
                        {/if}
                        {#if comp.deprecated}
                          <span class="inline-flex items-center px-1.5 py-0.5 text-xs font-medium bg-orange-100 dark:bg-orange-900/30 text-orange-700 dark:text-orange-400 rounded">
                            {$_("wizard.choose.deprecated")}
                          </span>
                        {/if}
                      </div>
                      <p class="text-xs text-gray-500 dark:text-gray-400 truncate mt-0.5">
                        {comp.description}
                      </p>
                    </button>

                    <!-- Version badge -->
                    <span class="text-xs text-gray-400 dark:text-gray-500 flex-shrink-0">
                      {comp.version.substring(0, 7)}
                    </span>
                  </div>

                  <!-- Expanded details -->
                  {#if isExpanded}
                    <div class="px-3 pb-3 pt-1 border-t border-gray-100 dark:border-gray-700 space-y-2 text-xs">
                      <div class="grid grid-cols-2 gap-2">
                        <div>
                          <span class="text-gray-500 dark:text-gray-400">{$_("wizard.choose.version")}:</span>
                          <span class="ml-1 text-gray-700 dark:text-gray-300">{comp.version.substring(0, 12)}</span>
                        </div>
                        <div>
                          <span class="text-gray-500 dark:text-gray-400">{$_("wizard.choose.repository")}:</span>
                          <span class="ml-1 text-gray-700 dark:text-gray-300">{comp.repository}</span>
                        </div>
                      </div>
                      {#if comp.compatibleTools.length > 0}
                        <div>
                          <span class="text-gray-500 dark:text-gray-400">{$_("wizard.choose.compatibleTools")}:</span>
                          <span class="ml-1 text-gray-700 dark:text-gray-300">{comp.compatibleTools.join(", ")}</span>
                        </div>
                      {/if}
                      {#if comp.dependencies.length > 0}
                        <div>
                          <span class="text-gray-500 dark:text-gray-400">{$_("wizard.choose.dependencies")}:</span>
                          <span class="ml-1 text-gray-700 dark:text-gray-300">{comp.dependencies.join(", ")}</span>
                        </div>
                      {/if}
                      <!-- Multi-tool target selection -->
                      {#if isSelected && detectedTools.length > 0}
                        <div>
                          <span class="text-gray-500 dark:text-gray-400">{$_("wizard.choose.targetTools")}:</span>
                          <div class="flex flex-wrap gap-1.5 mt-1">
                            {#each detectedTools.filter((t) => comp.compatibleTools.includes(t)) as tool}
                              {@const isTargeted = (toolTargets[comp.id] ?? []).includes(tool)}
                              <button
                                onclick={() => toggleToolTarget(comp.id, tool)}
                                class="px-2 py-0.5 rounded text-xs border transition-colors
                                  {isTargeted
                                    ? 'bg-blue-100 dark:bg-blue-900/40 border-blue-300 dark:border-blue-700 text-blue-700 dark:text-blue-300'
                                    : 'border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-400 hover:border-blue-300'}"
                              >
                                {tool}
                              </button>
                            {/each}
                          </div>
                        </div>
                      {/if}
                      <!-- Dependency lock warning -->
                      {#if isSelected && isDepLocked}
                        <p class="text-amber-600 dark:text-amber-400 italic">
                          {$_("wizard.choose.depRequired", { values: { name: (dependentsMap[comp.id] ?? []).filter((d) => componentsStore.isSelected(d)).join(", ") } })}
                        </p>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>
