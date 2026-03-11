<script lang="ts">
  import { _ } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { wizardStore } from "../stores/wizardStore.svelte";
  import { componentsStore, type ComponentInfo } from "../stores/componentsStore.svelte";

  // --- Types matching Rust backend ---

  interface ConflictFileExists {
    FileExists: { path: string; existing_checksum: string };
  }
  interface ConflictVersionMismatch {
    VersionMismatch: { component_id: string; installed: string; new: string };
  }
  interface ConflictModifiedLocally {
    ModifiedLocally: { component_id: string; path: string };
  }
  interface ConflictDependency {
    DependencyConflict: { component_id: string; required: string; available: string };
  }

  type ConflictType =
    | ConflictFileExists
    | ConflictVersionMismatch
    | ConflictModifiedLocally
    | ConflictDependency;

  type Resolution = "overwrite" | "skip" | "backup";

  interface FileChangeSummary {
    added: string[];
    modified: string[];
    deleted: string[];
    renamed: [string, string][];
  }

  interface DiskSpaceInfo {
    required: number;
    available: number;
  }

  interface MembershipInfo {
    componentId: string;
    groups: string[];
  }

  // --- State ---

  let conflicts = $state<ConflictType[]>([]);
  let resolutions = $state<Record<number, Resolution>>({});
  let loading = $state(true);
  let conflictError = $state("");

  // Simulated file change summaries per component (would come from version_tracker diff in real flow)
  let fileChanges = $state<Record<string, FileChangeSummary>>({});

  // Disk space info
  let diskSpace = $state<DiskSpaceInfo>({ required: 0, available: 0 });

  // Membership warnings for deletions
  let membershipWarnings = $state<MembershipInfo[]>([]);

  // --- Derived ---

  let selectedComponentInfos = $derived(
    componentsStore.available.filter((c) => componentsStore.isSelected(c.id))
  );

  let totalCounts = $derived.by(() => {
    let added = 0;
    let modified = 0;
    let deleted = 0;
    let renamed = 0;
    for (const summary of Object.values(fileChanges)) {
      added += summary.added.length;
      modified += summary.modified.length;
      deleted += summary.deleted.length;
      renamed += summary.renamed.length;
    }
    return { added, modified, deleted, renamed };
  });

  let diskSpaceSufficient = $derived(diskSpace.available >= diskSpace.required);

  let hasConflicts = $derived(conflicts.length > 0);

  let allResolved = $derived(
    conflicts.length === 0 ||
    conflicts.every((_, i) => resolutions[i] !== undefined)
  );

  let canConfirm = $derived(
    selectedComponentInfos.length > 0 && diskSpaceSufficient && allResolved
  );

  // --- Helpers ---

  function formatBytes(bytes: number): string {
    if (bytes === 0) return "0 B";
    const units = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
  }

  function getConflictLabel(conflict: ConflictType): string {
    if ("FileExists" in conflict) return $_("wizard.preview.conflictFileExists");
    if ("VersionMismatch" in conflict) return $_("wizard.preview.conflictVersionMismatch");
    if ("ModifiedLocally" in conflict) return $_("wizard.preview.conflictModifiedLocally");
    if ("DependencyConflict" in conflict) return $_("wizard.preview.conflictDependency");
    return "";
  }

  function getConflictDetail(conflict: ConflictType): string {
    if ("FileExists" in conflict) return conflict.FileExists.path;
    if ("VersionMismatch" in conflict) {
      const c = conflict.VersionMismatch;
      return `${c.component_id}: ${c.installed.substring(0, 7)} → ${c.new.substring(0, 7)}`;
    }
    if ("ModifiedLocally" in conflict) return `${conflict.ModifiedLocally.component_id}: ${conflict.ModifiedLocally.path}`;
    if ("DependencyConflict" in conflict) {
      const c = conflict.DependencyConflict;
      return `${c.component_id}: ${$_("wizard.preview.conflictRequired")} ${c.required}, ${$_("wizard.preview.conflictAvailableVer")} ${c.available}`;
    }
    return "";
  }

  function setResolution(index: number, resolution: Resolution) {
    resolutions = { ...resolutions, [index]: resolution };
  }

  function handleConfirm() {
    wizardStore.nextStep();
  }

  // --- Load preview data ---

  async function loadPreviewData() {
    loading = true;
    conflictError = "";

    try {
      // Build component/destination arrays for the Tauri command
      const components = selectedComponentInfos.map((c) => ({
        id: c.id,
        name: c.name,
        description: c.description,
        component_type: c.componentType,
        path: "",
        compatible_tools: c.compatibleTools,
        dependencies: c.dependencies,
        pinned: c.pinned,
        deprecated: c.deprecated,
        version: c.version || null,
      }));

      const destinations = Object.entries(wizardStore.destinations).map(([tool, path]) => ({
        tool_name: tool,
        path,
        enabled: true,
      }));

      const detected = await invoke<ConflictType[]>("detect_conflicts", {
        components,
        destinations,
      });
      conflicts = detected;
    } catch (err) {
      conflictError = String(err);
    }

    // Compute disk space from component file sizes
    const totalRequired = selectedComponentInfos.reduce((sum, c) => sum + (c.fileSize || 0), 0);
    const withMargin = Math.ceil(totalRequired * 1.1); // 10% safety margin
    diskSpace = { required: withMargin, available: 10 * 1024 * 1024 * 1024 }; // 10GB placeholder

    // Build simulated file change summaries per component
    const changes: Record<string, FileChangeSummary> = {};
    for (const comp of selectedComponentInfos) {
      changes[comp.id] = { added: [], modified: [], deleted: [], renamed: [] };
    }
    fileChanges = changes;

    // Check membership warnings (collections/competencies containing selected components)
    const warnings: MembershipInfo[] = [];
    // This would query the backend for collection/competency membership
    // For now, we leave it as an empty array — the UI handles it when populated
    membershipWarnings = warnings;

    loading = false;
  }

  loadPreviewData();
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
  {:else if selectedComponentInfos.length === 0}
    <div class="text-center py-12 text-gray-500 dark:text-gray-400">
      <p>{$_("wizard.preview.noChanges")}</p>
    </div>
  {:else}
    <!-- Total file change counts -->
    <div class="p-4 bg-gray-50 dark:bg-gray-800/50 border border-gray-200 dark:border-gray-700 rounded-lg">
      <h3 class="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">
        {$_("wizard.preview.totalChanges")}
      </h3>
      <div class="flex flex-wrap gap-4 text-sm">
        <span class="flex items-center gap-1.5">
          <span class="w-2.5 h-2.5 rounded-full bg-green-500"></span>
          <span class="text-gray-600 dark:text-gray-400">{$_("wizard.preview.filesAdded")}:</span>
          <span class="font-medium text-gray-900 dark:text-gray-100">{totalCounts.added}</span>
        </span>
        <span class="flex items-center gap-1.5">
          <span class="w-2.5 h-2.5 rounded-full bg-blue-500"></span>
          <span class="text-gray-600 dark:text-gray-400">{$_("wizard.preview.filesModified")}:</span>
          <span class="font-medium text-gray-900 dark:text-gray-100">{totalCounts.modified}</span>
        </span>
        <span class="flex items-center gap-1.5">
          <span class="w-2.5 h-2.5 rounded-full bg-red-500"></span>
          <span class="text-gray-600 dark:text-gray-400">{$_("wizard.preview.filesDeleted")}:</span>
          <span class="font-medium text-gray-900 dark:text-gray-100">{totalCounts.deleted}</span>
        </span>
        <span class="flex items-center gap-1.5">
          <span class="w-2.5 h-2.5 rounded-full bg-yellow-500"></span>
          <span class="text-gray-600 dark:text-gray-400">{$_("wizard.preview.filesRenamed")}:</span>
          <span class="font-medium text-gray-900 dark:text-gray-100">{totalCounts.renamed}</span>
        </span>
      </div>
    </div>

    <!-- Component list with per-component file changes -->
    <div>
      <h3 class="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">
        {$_("wizard.preview.componentSummary")}
      </h3>
      <div class="space-y-2">
        {#each selectedComponentInfos as comp}
          {@const changes = fileChanges[comp.id]}
          {@const membership = membershipWarnings.find((w) => w.componentId === comp.id)}
          <div class="border border-gray-200 dark:border-gray-700 rounded-lg p-3 bg-white dark:bg-gray-800">
            <div class="flex items-center justify-between">
              <div class="min-w-0">
                <span class="font-medium text-sm text-gray-900 dark:text-gray-100">{comp.name}</span>
                <span class="ml-2 text-xs text-gray-400 dark:text-gray-500">{comp.version?.substring(0, 7) ?? ""}</span>
              </div>
              <span class="text-xs px-2 py-0.5 rounded bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-400">
                {comp.componentType}
              </span>
            </div>

            <!-- Target tools -->
            {#if comp.compatibleTools.length > 0}
              <div class="mt-1 text-xs text-gray-500 dark:text-gray-400">
                {$_("wizard.preview.targetTool")}: {comp.compatibleTools.join(", ")}
              </div>
            {/if}

            <!-- File change summary for this component -->
            {#if changes}
              <div class="mt-2 flex flex-wrap gap-3 text-xs">
                {#if changes.added.length > 0}
                  <span class="text-green-600 dark:text-green-400">+{changes.added.length} {$_("wizard.preview.filesAdded").toLowerCase()}</span>
                {/if}
                {#if changes.modified.length > 0}
                  <span class="text-blue-600 dark:text-blue-400">~{changes.modified.length} {$_("wizard.preview.filesModified").toLowerCase()}</span>
                {/if}
                {#if changes.deleted.length > 0}
                  <span class="text-red-600 dark:text-red-400">-{changes.deleted.length} {$_("wizard.preview.filesDeleted").toLowerCase()}</span>
                {/if}
                {#if changes.renamed.length > 0}
                  <span class="text-yellow-600 dark:text-yellow-400">↔{changes.renamed.length} {$_("wizard.preview.filesRenamed").toLowerCase()}</span>
                {/if}
              </div>
            {/if}

            <!-- Membership warning -->
            {#if membership}
              <div class="mt-2 flex items-start gap-1.5 p-2 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded text-xs text-amber-700 dark:text-amber-400">
                <svg class="w-4 h-4 flex-shrink-0 mt-0.5" fill="currentColor" viewBox="0 0 20 20">
                  <path fill-rule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clip-rule="evenodd" />
                </svg>
                <span>{$_("wizard.preview.membershipWarning", { values: { names: membership.groups.join(", ") } })}</span>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </div>

    <!-- Disk space -->
    <div class="p-4 border rounded-lg {diskSpaceSufficient
        ? 'border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800'
        : 'border-red-300 dark:border-red-700 bg-red-50 dark:bg-red-900/20'}">
      <h3 class="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">
        {$_("wizard.preview.diskSpace")}
      </h3>
      <div class="flex flex-wrap gap-6 text-sm">
        <div>
          <span class="text-gray-500 dark:text-gray-400">{$_("wizard.preview.diskSpaceRequired")}:</span>
          <span class="ml-1 font-medium text-gray-900 dark:text-gray-100">{formatBytes(diskSpace.required)}</span>
        </div>
        <div>
          <span class="text-gray-500 dark:text-gray-400">{$_("wizard.preview.diskSpaceAvailable")}:</span>
          <span class="ml-1 font-medium text-gray-900 dark:text-gray-100">{formatBytes(diskSpace.available)}</span>
        </div>
      </div>
      <p class="mt-2 text-xs {diskSpaceSufficient
          ? 'text-green-600 dark:text-green-400'
          : 'text-red-600 dark:text-red-400'}">
        {diskSpaceSufficient ? $_("wizard.preview.diskSpaceSufficient") : $_("wizard.preview.diskSpaceInsufficient")}
      </p>
    </div>

    <!-- Conflicts -->
    {#if conflictError}
      <div class="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg text-sm text-red-700 dark:text-red-300">
        {$_("wizard.preview.errorLoadingConflicts")}: {conflictError}
      </div>
    {/if}

    {#if hasConflicts}
      <div class="p-4 border border-amber-200 dark:border-amber-800 bg-amber-50 dark:bg-amber-900/10 rounded-lg">
        <h3 class="text-sm font-semibold text-amber-700 dark:text-amber-400 mb-3">
          {$_("wizard.preview.conflicts")} ({conflicts.length})
        </h3>
        <div class="space-y-3">
          {#each conflicts as conflict, i}
            <div class="p-3 bg-white dark:bg-gray-800 border border-amber-100 dark:border-amber-900 rounded">
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                  <span class="text-xs font-medium text-amber-600 dark:text-amber-400">
                    {getConflictLabel(conflict)}
                  </span>
                  <p class="text-xs text-gray-600 dark:text-gray-400 mt-0.5 break-all">
                    {getConflictDetail(conflict)}
                  </p>
                </div>
                <!-- Resolution buttons -->
                <div class="flex-shrink-0">
                  <span class="text-xs text-gray-500 dark:text-gray-400 block mb-1">{$_("wizard.preview.resolutionLabel")}</span>
                  <div class="flex gap-1">
                    <button
                      onclick={() => setResolution(i, "overwrite")}
                      class="px-2 py-1 text-xs rounded border transition-colors
                        {resolutions[i] === 'overwrite'
                          ? 'bg-blue-100 dark:bg-blue-900/40 border-blue-300 dark:border-blue-700 text-blue-700 dark:text-blue-300'
                          : 'border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-400 hover:border-blue-300'}"
                    >
                      {$_("wizard.preview.resolutionOverwrite")}
                    </button>
                    <button
                      onclick={() => setResolution(i, "skip")}
                      class="px-2 py-1 text-xs rounded border transition-colors
                        {resolutions[i] === 'skip'
                          ? 'bg-blue-100 dark:bg-blue-900/40 border-blue-300 dark:border-blue-700 text-blue-700 dark:text-blue-300'
                          : 'border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-400 hover:border-blue-300'}"
                    >
                      {$_("wizard.preview.resolutionSkip")}
                    </button>
                    <button
                      onclick={() => setResolution(i, "backup")}
                      class="px-2 py-1 text-xs rounded border transition-colors
                        {resolutions[i] === 'backup'
                          ? 'bg-blue-100 dark:bg-blue-900/40 border-blue-300 dark:border-blue-700 text-blue-700 dark:text-blue-300'
                          : 'border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-400 hover:border-blue-300'}"
                    >
                      {$_("wizard.preview.resolutionBackup")}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Confirm button -->
    <div class="flex justify-end">
      <button
        onclick={handleConfirm}
        disabled={!canConfirm}
        class="px-5 py-2.5 text-sm font-medium rounded-lg transition-colors
          {canConfirm
            ? 'bg-green-600 text-white hover:bg-green-700'
            : 'bg-gray-300 dark:bg-gray-700 text-gray-500 dark:text-gray-400 cursor-not-allowed'}"
      >
        {$_("wizard.preview.confirm")}
      </button>
    </div>
  {/if}
</div>
