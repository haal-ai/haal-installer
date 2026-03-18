<script lang="ts">
  import { _ } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { wizardStore } from "../stores/wizardStore.svelte";
  import { progressStore } from "../stores/progressStore.svelte";
  import type { ResolvedComponent } from "../stores/componentsStore.svelte";

  // --- Types ---

  type ComponentStatus = "pending" | "in-progress" | "succeeded" | "failed";

  interface ComponentProgress {
    id: string;
    componentType: string;
    status: ComponentStatus;
    error?: string;
  }

  interface ProgressPayload {
    step: string;
    percentage: number;
    current_file: string;
    elapsed_ms: number;
    estimated_remaining_ms: number;
    components_succeeded: string[];
    components_failed: string[];
    current_component?: string;
  }

  interface OperationResult {
    success: boolean;
    componentsFailed: { componentId: string; error: string }[];
    componentsSucceeded: string[];
    cleanedCount?: number;
    cleanedNames?: string[];
  }

  // --- State ---

  let componentStatuses = $state<ComponentProgress[]>([]);
  let error = $state("");
  let completed = $state(false);
  let cleanedCount = $state(0);
  let cleanedNames = $state<string[]>([]);
  let unlistenFn = $state<UnlistenFn | null>(null);

  // --- Derived ---

  let progressPercentage = $derived(progressStore.progress.percentage);
  let currentStep = $derived(progressStore.progress.step);
  let elapsedMs = $derived(progressStore.progress.elapsedMs);
  let estimatedRemainingMs = $derived(progressStore.progress.estimatedRemainingMs);

  // Group component statuses by type
  const TYPE_ORDER = ["skill", "power", "mcpServer", "hook", "command", "rule", "agent"];
  const TYPE_META: Record<string, { label: string; icon: string; color: string; destLabel: (req: NonNullable<typeof wizardStore.installRequest>) => string }> = {
    skill:     { label: "Skills",      icon: "🧠", color: "text-green-600 dark:text-green-400",  destLabel: (r) => destForSkills(r) },
    power:     { label: "Powers",      icon: "⚡", color: "text-purple-600 dark:text-purple-400", destLabel: (_) => "~/.kiro/powers/installed/" },
    mcpServer: { label: "MCP Servers", icon: "🔌", color: "text-cyan-600 dark:text-cyan-400",    destLabel: (r) => `Config files for: ${r.selectedTools.join(", ") || "selected tools"}` },
    hook:      { label: "Hooks",       icon: "🪝", color: "text-amber-600 dark:text-amber-400",  destLabel: (r) => destForSkills(r) },
    command:   { label: "Commands",    icon: "⌨️", color: "text-blue-600 dark:text-blue-400",    destLabel: (r) => destForSkills(r) },
    rule:      { label: "Rules",       icon: "📋", color: "text-indigo-600 dark:text-indigo-400", destLabel: (r) => destForSkills(r) },
    agent:     { label: "Agents",      icon: "🤖", color: "text-rose-600 dark:text-rose-400",    destLabel: (r) => r.repoPaths.length > 0 ? `${r.repoPaths.length} repo(s)/.kiro/` : "~/.kiro/" },
  };

  function destForSkills(r: NonNullable<typeof wizardStore.installRequest>): string {
    const parts: string[] = [];
    if (r.scope === "home" || r.scope === "both") {
      if (r.selectedTools.length > 0) parts.push(`~/.${r.selectedTools[0].toLowerCase()}/skills/`);
      if (r.selectedTools.length > 1) parts.push(`+${r.selectedTools.length - 1} more`);
    }
    if ((r.scope === "repo" || r.scope === "both") && r.repoPaths.length > 0) {
      parts.push(`${r.repoPaths.length} repo(s)/.kiro/skills/`);
    }
    return parts.join("  ·  ") || "~/.kiro/skills/";
  }

  let groupedStatuses = $derived.by(() => {
    const groups: { type: string; items: ComponentProgress[] }[] = [];
    for (const type of TYPE_ORDER) {
      const items = componentStatuses.filter(c => c.componentType === type);
      if (items.length > 0) groups.push({ type, items });
    }
    return groups;
  });

  // Completion summary: count succeeded/failed per type
  let completionSummary = $derived.by(() => {
    if (!completed) return [];
    return groupedStatuses
      .map(g => ({
        type: g.type,
        succeeded: g.items.filter(i => i.status === "succeeded").length,
        failed: g.items.filter(i => i.status === "failed").length,
        total: g.items.length,
      }))
      .filter(g => g.total > 0);
  });

  let totalSucceeded = $derived(componentStatuses.filter(c => c.status === "succeeded").length);
  let totalFailed = $derived(componentStatuses.filter(c => c.status === "failed").length);

  // Track which groups are expanded (collapsed by default)
  let expandedGroups = $state<Set<string>>(new Set());
  function toggleGroup(type: string) {
    const next = new Set(expandedGroups);
    if (next.has(type)) next.delete(type); else next.add(type);
    expandedGroups = next;
  }

  // --- Helpers ---

  function formatTime(ms: number): string {
    if (ms <= 0) return "00:00";
    const totalSeconds = Math.floor(ms / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }

  function statusDot(status: ComponentStatus): string {
    switch (status) {
      case "pending":    return "bg-gray-300 dark:bg-gray-600";
      case "in-progress": return "bg-blue-500 animate-pulse";
      case "succeeded":  return "bg-green-500";
      case "failed":     return "bg-red-500";
    }
  }

  // --- Execution logic ---

  async function startExecution() {
    wizardStore.setExecuting(true);
    progressStore.start();

    const req = wizardStore.installRequest;
    if (!req) {
      error = "No install request found. Please go back and confirm your selection.";
      progressStore.complete();
      completed = true;
      wizardStore.setExecuting(false);
      return;
    }

    componentStatuses = req.components.map((c: ResolvedComponent) => ({
      id: c.id,
      componentType: c.componentType,
      status: "pending" as ComponentStatus,
    }));

    try {
      unlistenFn = await listen<ProgressPayload>("install-progress", (event) => {
        const p = event.payload;
        progressStore.update({
          step: p.step,
          percentage: p.percentage,
          currentFile: p.current_file,
          elapsedMs: p.elapsed_ms,
          estimatedRemainingMs: p.estimated_remaining_ms,
          componentsSucceeded: p.components_succeeded,
          componentsFailed: p.components_failed,
        });
        componentStatuses = componentStatuses.map((cs) => {
          if (p.components_succeeded.includes(cs.id)) return { ...cs, status: "succeeded" as ComponentStatus };
          if (p.components_failed.includes(cs.id))    return { ...cs, status: "failed" as ComponentStatus };
          if (p.current_component === cs.id)          return { ...cs, status: "in-progress" as ComponentStatus };
          return cs;
        });
      });
    } catch (err) {
      console.error("Failed to listen for progress events:", err);
    }

    try {
      const result = await invoke<OperationResult>("install_components_v2", { request: req });
      cleanedCount = result.cleanedCount ?? 0;
      cleanedNames = result.cleanedNames ?? [];
      componentStatuses = componentStatuses.map((cs) => {
        if (result.componentsSucceeded.includes(cs.id)) return { ...cs, status: "succeeded" as ComponentStatus };
        const failure = result.componentsFailed.find((f) => f.componentId === cs.id);
        if (failure) return { ...cs, status: "failed" as ComponentStatus, error: failure.error };
        return cs;
      });
      progressStore.complete();
      completed = true;

      // Persist the install choices for quick-update
      if (result.success || result.componentsSucceeded.length > 0) {
        try {
          await invoke("save_last_install", {
            profile: {
              seedUrl: wizardStore.registryUrl || "",
              competencyIds: wizardStore.selectedComponents.map(c => c.id),
              selectedTools: req.selectedTools,
              scope: req.scope,
              repoPaths: req.repoPaths ?? [],
              installedAt: new Date().toISOString(),
            },
          });
        } catch (e) {
          console.warn("Could not save last install profile:", e);
        }
      }
    } catch (err) {
      error = String(err);
      progressStore.complete();
      completed = true;
    } finally {
      wizardStore.setExecuting(false);
      if (unlistenFn) { unlistenFn(); unlistenFn = null; }
    }
  }

  function handleStartOver() {
    wizardStore.reset();
    progressStore.reset();
  }

  function handleClose() {
    window.dispatchEvent(new CustomEvent("close-window"));
  }

  startExecution();
</script>

<div class="space-y-5">
  <div>
    <h2 class="text-xl font-semibold text-gray-900 dark:text-gray-100">
      {$_("wizard.execute.title")}
    </h2>
    <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
      {$_("wizard.execute.description")}
    </p>
  </div>

  <!-- Progress bar -->
  <div class="space-y-1.5">
    <div class="flex items-center justify-between text-xs text-gray-500 dark:text-gray-400">
      <span>{currentStep || (completed ? "Done" : "Starting…")}</span>
      <span class="font-mono">{progressPercentage}%</span>
    </div>
    <div class="w-full h-2 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
      <div
        class="h-full rounded-full transition-all duration-300 {completed && totalFailed === 0 ? 'bg-green-500' : completed && totalFailed > 0 ? 'bg-amber-500' : 'bg-blue-600 dark:bg-blue-500'}"
        style="width: {progressPercentage}%"
        role="progressbar"
        aria-valuenow={progressPercentage}
        aria-valuemin={0}
        aria-valuemax={100}
      ></div>
    </div>
    <div class="flex gap-4 text-xs text-gray-400 dark:text-gray-500">
      <span>Elapsed: <span class="font-mono">{formatTime(elapsedMs)}</span></span>
      {#if !completed && estimatedRemainingMs > 0}
        <span>Remaining: <span class="font-mono">{formatTime(estimatedRemainingMs)}</span></span>
      {/if}
    </div>
  </div>

  <!-- Component groups -->
  <div class="space-y-3">
    {#each groupedStatuses as group}
      {@const meta = TYPE_META[group.type] ?? { label: group.type, icon: "📦", color: "text-gray-600", destLabel: () => "" }}
      {@const req = wizardStore.installRequest}
      {@const succeeded = group.items.filter(i => i.status === 'succeeded').length}
      {@const failed = group.items.filter(i => i.status === 'failed').length}
      {@const inProgress = group.items.filter(i => i.status === 'in-progress').length}
      {@const isExpanded = expandedGroups.has(group.type)}
      <div class="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
        <!-- Group header (clickable to expand/collapse) -->
        <button
          onclick={() => toggleGroup(group.type)}
          class="w-full px-3 py-2 bg-gray-50 dark:bg-gray-800/80 flex items-center gap-2 cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors text-left"
        >
          <span class="text-[10px] text-gray-400 transition-transform {isExpanded ? 'rotate-90' : ''}">▶</span>
          <span class="text-sm">{meta.icon}</span>
          <span class="text-sm font-medium {meta.color}">{meta.label}</span>
          <span class="text-xs text-gray-500 dark:text-gray-400">
            {#if inProgress > 0}
              <span class="text-blue-500">{succeeded + failed + inProgress}/{group.items.length}</span>
            {:else if succeeded === group.items.length}
              <span class="text-green-500">✓ {succeeded}</span>
            {:else if failed > 0}
              <span class="text-green-500">{succeeded}</span> · <span class="text-red-500">{failed} ✗</span>
            {:else}
              {group.items.length} pending
            {/if}
          </span>
          <span class="text-xs text-gray-400 dark:text-gray-500 ml-auto font-mono truncate max-w-[50%] text-right">
            {req ? meta.destLabel(req) : ""}
          </span>
        </button>
        <!-- Items (collapsible) -->
        {#if isExpanded}
          <div class="divide-y divide-gray-100 dark:divide-gray-800">
            {#each group.items as comp}
              <div class="flex items-center gap-2.5 px-3 py-2">
                <span class="w-2 h-2 rounded-full flex-shrink-0 {statusDot(comp.status)}"></span>
                <span class="text-xs font-mono text-gray-800 dark:text-gray-200 flex-1 truncate">{comp.id}</span>
                {#if comp.status === "in-progress"}
                  <svg class="w-3.5 h-3.5 text-blue-500 animate-spin flex-shrink-0" fill="none" viewBox="0 0 24 24">
                    <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                    <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
                  </svg>
                {:else if comp.status === "succeeded"}
                  <span class="text-xs text-green-600 dark:text-green-400 flex-shrink-0">✓</span>
                {:else if comp.status === "failed"}
                  <span class="text-xs text-red-500 flex-shrink-0">✗</span>
                {/if}
              </div>
              {#if comp.error}
                <div class="px-3 pb-2 ml-4">
                  <p class="text-xs text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded px-2 py-1">{comp.error}</p>
                </div>
              {/if}
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>

  <!-- Error display -->
  {#if error}
    <div class="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg">
      <p class="text-sm text-red-700 dark:text-red-300">{error}</p>
    </div>
  {/if}

  <!-- Rollback notice -->
  {#if progressStore.rollbackPerformed}
    <div class="p-3 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded-lg">
      <p class="text-sm text-amber-700 dark:text-amber-400">{$_("errors.rollbackPerformed")}</p>
    </div>
  {/if}

  <!-- Completion summary -->
  {#if completed}
    <div class="border rounded-lg overflow-hidden {totalFailed === 0 ? 'border-green-200 dark:border-green-800' : 'border-amber-200 dark:border-amber-800'}">
      <div class="px-3 py-2 {totalFailed === 0 ? 'bg-green-50 dark:bg-green-900/20' : 'bg-amber-50 dark:bg-amber-900/20'} flex items-center gap-2">
        <span class="text-base">{totalFailed === 0 ? "🎉" : "⚠️"}</span>
        <span class="text-sm font-medium {totalFailed === 0 ? 'text-green-800 dark:text-green-200' : 'text-amber-800 dark:text-amber-200'}">
          {totalFailed === 0
            ? `${totalSucceeded} component${totalSucceeded !== 1 ? "s" : ""} installed successfully`
            : `${totalSucceeded} succeeded, ${totalFailed} failed`}
          {#if cleanedCount > 0}
            <span class="font-normal text-gray-500 dark:text-gray-400"> · {cleanedCount} old skill{cleanedCount !== 1 ? "s" : ""} removed</span>
          {/if}
        </span>
      </div>
      <div class="px-3 py-2 space-y-1 bg-white dark:bg-gray-900">
        {#each completionSummary as g}
          {@const meta = TYPE_META[g.type] ?? { label: g.type, icon: "📦", color: "text-gray-600", destLabel: () => "" }}
          {@const req = wizardStore.installRequest}
          <div class="flex items-center gap-2 text-xs">
            <span>{meta.icon}</span>
            <span class="{meta.color} font-medium w-20">{meta.label}</span>
            <span class="text-gray-700 dark:text-gray-300">
              {g.succeeded}/{g.total}
              {#if g.failed > 0}<span class="text-red-500 ml-1">({g.failed} failed)</span>{/if}
            </span>
            <span class="text-gray-400 dark:text-gray-500 font-mono truncate ml-auto max-w-[50%] text-right">
              {req ? meta.destLabel(req) : ""}
            </span>
          </div>
        {/each}
      </div>
      {#if totalFailed === 0}
        <div class="px-3 py-2 border-t border-green-100 dark:border-green-900 bg-green-50/50 dark:bg-green-900/10">
          <p class="text-xs text-green-700 dark:text-green-400">
            Restart your AI tools to pick up the new skills and settings.
          </p>
          {#if cleanedCount > 0}
            <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">
              🧹 Removed: {cleanedNames.join(", ")}
            </p>
          {/if}
        </div>
      {/if}
    </div>

    <div class="flex justify-between">
      <button
        onclick={handleStartOver}
        class="px-4 py-2 text-sm font-medium rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
      >
        ← Another install
      </button>
      <button
        onclick={handleClose}
        class="px-5 py-2.5 text-sm font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors"
      >
        Close
      </button>
    </div>
  {/if}
</div>
