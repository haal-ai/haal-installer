<script lang="ts">
  import { _ } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import {
    componentsStore,
    type MergedCatalog,
    type CompetencyDetail,
  } from "../stores/componentsStore.svelte";
  import { wizardStore } from "../stores/wizardStore.svelte";

  let loadError = $state("");
  // Which competency panel is expanded
  let expandedCompetency = $state<string | null>(null);
  // view: "collections" | "competencies"
  let view = $state<"collections" | "competencies">("collections");

  async function loadManifest() {
    if (!wizardStore.isConnected) return;
    componentsStore.setLoading(true);
    loadError = "";
    try {
      const catalog = await invoke<MergedCatalog>("initialize_catalog", {
        seedUrl: wizardStore.registryUrl || null,
      });
      componentsStore.setMergedCatalog(catalog);
    } catch (e: any) {
      loadError = String(e);
    } finally {
      componentsStore.setLoading(false);
    }
  }

  async function loadCompetency(id: string, manifestUrl: string) {
    if (componentsStore.competencyDetails[id]) return;
    componentsStore.setCompetencyLoading(id, true);
    try {
      const detail = await invoke<CompetencyDetail>("fetch_competency", {
        competencyId: id,
        manifestUrl,
        // Use local repo path when available — avoids network call
        baseUrl: componentsStore.mergedCatalog?.competencySources[id] ?? "",
      });
      componentsStore.setCompetencyDetail(id, detail);
    } catch {
      componentsStore.setCompetencyLoading(id, false);
    }
  }

  function toggleExpand(id: string, manifestUrl: string) {
    if (expandedCompetency === id) {
      expandedCompetency = null;
    } else {
      expandedCompetency = id;
      loadCompetency(id, manifestUrl);
    }
  }

  // When a collection is selected, also expand its competencies view
  function toggleCollection(id: string) {
    componentsStore.toggle(`collection:${id}`);
  }

  function toggleCompetency(id: string) {
    componentsStore.toggle(`competency:${id}`);
  }

  function isCollectionSelected(id: string) {
    return componentsStore.isSelected(`collection:${id}`);
  }

  function isCompetencySelected(id: string) {
    return componentsStore.isSelected(`competency:${id}`);
  }

  // A competency is "covered" if a selected collection already includes it
  function isCompetencyCovered(id: string): boolean {
    for (const col of componentsStore.collections) {
      if (componentsStore.isSelected(`collection:${col.id}`) && col.competencyIds.includes(id)) {
        return true;
      }
    }
    return false;
  }

  loadManifest();

  let hasSelection = $derived(componentsStore.selectedCount > 0);
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
  {:else if loadError}
    <div class="rounded-lg border border-red-300 dark:border-red-700 bg-red-50 dark:bg-red-900/20 p-4 flex flex-col gap-2">
      <p class="text-sm font-medium text-red-700 dark:text-red-400">Failed to load registry</p>
      <p class="text-xs text-red-600 dark:text-red-300 font-mono break-all">{loadError}</p>
      <button onclick={loadManifest} class="self-start px-3 py-1.5 text-xs font-medium rounded-lg bg-red-600 hover:bg-red-700 text-white">
        Retry
      </button>
    </div>
  {:else if componentsStore.collections.length > 0}
    <!-- View toggle -->
    <div class="flex gap-1 p-1 bg-gray-100 dark:bg-gray-800 rounded-lg w-fit">
      <button
        onclick={() => (view = "collections")}
        class="px-3 py-1.5 text-xs font-medium rounded-md transition-colors
          {view === 'collections' ? 'bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 shadow-sm' : 'text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300'}"
      >
        Collections
      </button>
      <button
        onclick={() => (view = "competencies")}
        class="px-3 py-1.5 text-xs font-medium rounded-md transition-colors
          {view === 'competencies' ? 'bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 shadow-sm' : 'text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300'}"
      >
        Competencies
      </button>
    </div>

    {#if view === "collections"}
      <!-- Collections: quick-install bundles -->
      <div class="space-y-2">
        {#each componentsStore.collections as col}
          {@const selected = isCollectionSelected(col.id)}
          <div
            class="border rounded-lg transition-colors cursor-pointer
              {selected ? 'border-blue-300 dark:border-blue-700 bg-blue-50/50 dark:bg-blue-900/10' : 'border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800'}"
          >
            <div class="flex items-start gap-3 p-4" onclick={() => toggleCollection(col.id)} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && toggleCollection(col.id)}>
              <!-- Checkbox -->
              <div class="mt-0.5 w-5 h-5 flex-shrink-0 rounded border-2 flex items-center justify-center
                {selected ? 'border-blue-600 bg-blue-600 text-white' : 'border-gray-300 dark:border-gray-600'}">
                {#if selected}
                  <svg class="w-3 h-3" fill="currentColor" viewBox="0 0 20 20">
                    <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd" />
                  </svg>
                {/if}
              </div>
              <div class="flex-1 min-w-0">
                <p class="font-medium text-sm text-gray-900 dark:text-gray-100">{col.name}</p>
                <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{col.description}</p>
                <div class="flex flex-wrap gap-1 mt-2">
                  {#each col.competencyIds as cid}
                    {@const entry = componentsStore.competencies.find(c => c.id === cid)}
                    {#if entry}
                      <span class="inline-flex items-center px-2 py-0.5 text-xs rounded-full bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300">
                        {entry.name}
                      </span>
                    {/if}
                  {/each}
                </div>
              </div>
            </div>
          </div>
        {/each}
      </div>

    {:else}
      <!-- Competencies: fine-grained selection with skill drill-down -->
      <div class="space-y-2">
        {#each componentsStore.competencies as comp}
          {@const selected = isCompetencySelected(comp.id)}
          {@const covered = isCompetencyCovered(comp.id)}
          {@const expanded = expandedCompetency === comp.id}
          {@const detail = componentsStore.competencyDetails[comp.id]}
          {@const isLoading = componentsStore.loadingCompetencies.has(comp.id)}

          <div class="border rounded-lg transition-colors
            {covered ? 'border-blue-200 dark:border-blue-800 bg-blue-50/30 dark:bg-blue-900/5 opacity-70' :
             selected ? 'border-blue-300 dark:border-blue-700 bg-blue-50/50 dark:bg-blue-900/10' :
             'border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800'}">

            <div class="flex items-center gap-3 p-3">
              <!-- Checkbox (disabled if covered by collection) -->
              <button
                onclick={() => !covered && toggleCompetency(comp.id)}
                disabled={covered}
                class="flex-shrink-0"
                aria-label="Select {comp.name}"
              >
                <div class="w-5 h-5 rounded border-2 flex items-center justify-center
                  {covered ? 'border-blue-300 dark:border-blue-600 bg-blue-100 dark:bg-blue-900/30 cursor-not-allowed' :
                   selected ? 'border-blue-600 bg-blue-600 text-white' :
                   'border-gray-300 dark:border-gray-600 hover:border-blue-400'}">
                  {#if covered || selected}
                    <svg class="w-3 h-3 {covered ? 'text-blue-400' : 'text-white'}" fill="currentColor" viewBox="0 0 20 20">
                      <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd" />
                    </svg>
                  {/if}
                </div>
              </button>

              <!-- Name + description -->
              <div class="flex-1 min-w-0">
                <p class="font-medium text-sm text-gray-900 dark:text-gray-100">{comp.name}</p>
                <p class="text-xs text-gray-500 dark:text-gray-400 truncate">{comp.description}</p>
                {#if covered}
                  <p class="text-xs text-blue-500 dark:text-blue-400 mt-0.5 italic">Included via collection</p>
                {/if}
              </div>

              <!-- Expand toggle -->
              <button
                onclick={() => toggleExpand(comp.id, comp.manifestUrl)}
                class="flex-shrink-0 p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-400 dark:text-gray-500"
                aria-label="Show skills"
              >
                <svg class="w-4 h-4 transition-transform {expanded ? 'rotate-180' : ''}" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                </svg>
              </button>
            </div>

            <!-- Expanded skill list -->
            {#if expanded}
              <div class="px-3 pb-3 pt-1 border-t border-gray-100 dark:border-gray-700">
                {#if isLoading}
                  <p class="text-xs text-gray-400 dark:text-gray-500 py-2">Loading skills...</p>
                {:else if detail}
                  <div class="space-y-2">
                    {#if detail.skills.length > 0}
                      <div>
                        <p class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wide mb-1">Skills ({detail.skills.length})</p>
                        <div class="flex flex-wrap gap-1">
                          {#each detail.skills as skill}
                            <span class="inline-flex items-center px-2 py-0.5 text-xs rounded bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 font-mono">
                              {skill}
                            </span>
                          {/each}
                        </div>
                      </div>
                    {/if}
                    {#if detail.powers.length > 0}
                      <div>
                        <p class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wide mb-1">Powers ({detail.powers.length})</p>
                        <div class="flex flex-wrap gap-1">
                          {#each detail.powers as power}
                            <span class="inline-flex items-center px-2 py-0.5 text-xs rounded bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300 font-mono">
                              {power}
                            </span>
                          {/each}
                        </div>
                      </div>
                    {/if}
                    {#if detail.hooks.length > 0}
                      <div>
                        <p class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wide mb-1">Hooks ({detail.hooks.length})</p>
                        <div class="flex flex-wrap gap-1">
                          {#each detail.hooks as hook}
                            <span class="inline-flex items-center px-2 py-0.5 text-xs rounded bg-amber-100 dark:bg-amber-900/30 text-amber-700 dark:text-amber-300 font-mono">
                              {hook}
                            </span>
                          {/each}
                        </div>
                      </div>
                    {/if}
                    {#if detail.commands.length > 0}
                      <div>
                        <p class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wide mb-1">Commands ({detail.commands.length})</p>
                        <div class="flex flex-wrap gap-1">
                          {#each detail.commands as cmd}
                            <span class="inline-flex items-center px-2 py-0.5 text-xs rounded bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300 font-mono">
                              {cmd}
                            </span>
                          {/each}
                        </div>
                      </div>
                    {/if}
                    {#if (detail.mcpServers ?? []).length > 0}
                      <div>
                        <p class="text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wide mb-1">MCP Servers ({detail.mcpServers.length})</p>
                        <div class="flex flex-wrap gap-1">
                          {#each detail.mcpServers as mcp}
                            <span class="inline-flex items-center px-2 py-0.5 text-xs rounded bg-cyan-100 dark:bg-cyan-900/30 text-cyan-700 dark:text-cyan-300 font-mono">
                              🔌 {mcp}
                            </span>
                          {/each}
                        </div>
                      </div>
                    {/if}
                  </div>
                {:else}
                  <p class="text-xs text-gray-400 dark:text-gray-500 py-2">No detail available</p>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <!-- Selection summary -->
    {#if hasSelection}
      <div class="rounded-lg bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 p-3 text-xs text-blue-700 dark:text-blue-300">
        {componentsStore.selectedCount} item(s) selected &mdash;
        {componentsStore.resolvedCompetencyIds.length} competenc{componentsStore.resolvedCompetencyIds.length === 1 ? 'y' : 'ies'} will be installed
      </div>
    {/if}
  {/if}
</div>
