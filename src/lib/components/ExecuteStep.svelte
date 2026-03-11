<script lang="ts">
  import { _ } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { wizardStore } from "../stores/wizardStore.svelte";
  import { progressStore } from "../stores/progressStore.svelte";
  import { componentsStore } from "../stores/componentsStore.svelte";

  // --- Types ---

  type ComponentStatus = "pending" | "in-progress" | "succeeded" | "failed";

  interface ComponentProgress {
    id: string;
    name: string;
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
    operation: string;
    components_processed: number;
    components_succeeded: string[];
    components_failed: { component_id: string; error: string; destination?: string }[];
    duration_ms: number;
    rollback_performed: boolean;
  }

  // --- State ---

  let componentStatuses = $state<ComponentProgress[]>([]);
  let error = $state("");
  let completed = $state(false);
  let unlistenFn = $state<UnlistenFn | null>(null);

  // --- Derived ---

  let progressPercentage = $derived(progressStore.progress.percentage);
  let currentStep = $derived(progressStore.progress.step);
  let currentFile = $derived(progressStore.progress.currentFile);
  let elapsedMs = $derived(progressStore.progress.elapsedMs);
  let estimatedRemainingMs = $derived(progressStore.progress.estimatedRemainingMs);

  // --- Helpers ---

  function formatTime(ms: number): string {
    if (ms <= 0) return "00:00";
    const totalSeconds = Math.floor(ms / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }

  function statusColor(status: ComponentStatus): string {
    switch (status) {
      case "pending":
        return "bg-gray-400 dark:bg-gray-500";
      case "in-progress":
        return "bg-blue-500 dark:bg-blue-400";
      case "succeeded":
        return "bg-green-500 dark:bg-green-400";
      case "failed":
        return "bg-red-500 dark:bg-red-400";
    }
  }

  function statusLabel(status: ComponentStatus): string {
    switch (status) {
      case "pending":
        return $_("wizard.execute.statusPending");
      case "in-progress":
        return $_("wizard.execute.statusInProgress");
      case "succeeded":
        return $_("wizard.execute.statusSucceeded");
      case "failed":
        return $_("wizard.execute.statusFailed");
    }
  }

  // --- Execution logic ---

  async function startExecution() {
    wizardStore.setExecuting(true);
    progressStore.start();

    // Initialize component statuses from selected components
    const selectedInfos = componentsStore.available.filter((c) =>
      componentsStore.isSelected(c.id)
    );
    componentStatuses = selectedInfos.map((c) => ({
      id: c.id,
      name: c.name,
      status: "pending" as ComponentStatus,
    }));

    // Listen to progress events from Tauri backend
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

        // Update per-component statuses
        componentStatuses = componentStatuses.map((cs) => {
          if (p.components_succeeded.includes(cs.id)) {
            return { ...cs, status: "succeeded" as ComponentStatus };
          }
          if (p.components_failed.includes(cs.id)) {
            return { ...cs, status: "failed" as ComponentStatus };
          }
          if (p.current_component === cs.id) {
            return { ...cs, status: "in-progress" as ComponentStatus };
          }
          return cs;
        });
      });
    } catch (err) {
      // If listen fails, we still proceed — progress just won't update in real-time
      console.error("Failed to listen for progress events:", err);
    }

    // Invoke the install command
    try {
      const components = selectedInfos.map((c) => ({
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

      const destinations = Object.entries(wizardStore.destinations).map(
        ([tool, path]) => ({
          tool_name: tool,
          path,
          enabled: true,
        })
      );

      const result = await invoke<OperationResult>("install_components", {
        components,
        destinations,
      });

      // Update final component statuses from result
      componentStatuses = componentStatuses.map((cs) => {
        if (result.components_succeeded.includes(cs.id)) {
          return { ...cs, status: "succeeded" as ComponentStatus };
        }
        const failure = result.components_failed.find(
          (f) => f.component_id === cs.id
        );
        if (failure) {
          return { ...cs, status: "failed" as ComponentStatus, error: failure.error };
        }
        return cs;
      });

      if (result.rollback_performed) {
        progressStore.setRollbackPerformed(true);
      }

      progressStore.complete();
      completed = true;
    } catch (err) {
      error = String(err);
      progressStore.complete();
      completed = true;
    } finally {
      wizardStore.setExecuting(false);
      if (unlistenFn) {
        unlistenFn();
        unlistenFn = null;
      }
    }
  }

  function handleDone() {
    wizardStore.nextStep();
  }

  // Start execution on mount
  startExecution();
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-xl font-semibold text-gray-900 dark:text-gray-100">
      {$_("wizard.execute.title")}
    </h2>
    <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
      {$_("wizard.execute.description")}
    </p>
  </div>

  <!-- Progress bar -->
  <div class="space-y-2">
    <div class="flex items-center justify-between text-sm">
      <span class="text-gray-700 dark:text-gray-300">{$_("wizard.execute.progress")}</span>
      <span class="font-medium text-gray-900 dark:text-gray-100">{progressPercentage}%</span>
    </div>
    <div class="w-full h-3 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
      <div
        class="h-full bg-blue-600 dark:bg-blue-500 rounded-full transition-all duration-300"
        style="width: {progressPercentage}%"
        role="progressbar"
        aria-valuenow={progressPercentage}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={$_("wizard.execute.progress")}
      ></div>
    </div>
  </div>

  <!-- Current step and file info -->
  <div class="p-4 bg-gray-50 dark:bg-gray-800/50 border border-gray-200 dark:border-gray-700 rounded-lg space-y-2">
    {#if currentStep}
      <div class="flex items-center gap-2 text-sm">
        <span class="text-gray-500 dark:text-gray-400">{$_("wizard.execute.currentStep")}:</span>
        <span class="font-medium text-gray-900 dark:text-gray-100">{currentStep}</span>
      </div>
    {/if}
    {#if currentFile}
      <div class="flex items-center gap-2 text-sm">
        <span class="text-gray-500 dark:text-gray-400">{$_("wizard.execute.currentFile")}:</span>
        <span class="font-mono text-xs text-gray-700 dark:text-gray-300 truncate">{currentFile}</span>
      </div>
    {/if}
    <div class="flex gap-6 text-sm">
      <div class="flex items-center gap-2">
        <span class="text-gray-500 dark:text-gray-400">{$_("wizard.execute.elapsed")}:</span>
        <span class="font-mono text-gray-900 dark:text-gray-100">{formatTime(elapsedMs)}</span>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-gray-500 dark:text-gray-400">{$_("wizard.execute.remaining")}:</span>
        <span class="font-mono text-gray-900 dark:text-gray-100">{formatTime(estimatedRemainingMs)}</span>
      </div>
    </div>
  </div>

  <!-- Component status list -->
  <div>
    <h3 class="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">
      {$_("wizard.execute.components")}
    </h3>
    <div class="space-y-1.5">
      {#each componentStatuses as comp}
        <div class="flex items-center justify-between p-2.5 border border-gray-200 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800">
          <div class="flex items-center gap-2.5 min-w-0">
            <span class="w-2.5 h-2.5 rounded-full flex-shrink-0 {statusColor(comp.status)}"></span>
            <span class="text-sm text-gray-900 dark:text-gray-100 truncate">{comp.name}</span>
          </div>
          <div class="flex items-center gap-2 flex-shrink-0">
            {#if comp.status === "in-progress"}
              <svg class="w-4 h-4 text-blue-500 animate-spin" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
              </svg>
            {/if}
            <span class="text-xs {comp.status === 'succeeded'
              ? 'text-green-600 dark:text-green-400'
              : comp.status === 'failed'
                ? 'text-red-600 dark:text-red-400'
                : comp.status === 'in-progress'
                  ? 'text-blue-600 dark:text-blue-400'
                  : 'text-gray-400 dark:text-gray-500'}">
              {statusLabel(comp.status)}
            </span>
          </div>
        </div>
        {#if comp.error}
          <div class="ml-5 p-2 text-xs text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded">
            {comp.error}
          </div>
        {/if}
      {/each}
    </div>
  </div>

  <!-- Error display -->
  {#if error}
    <div class="p-4 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg">
      <p class="text-sm text-red-700 dark:text-red-300">{$_("errors.operationFailed", { values: { message: error } })}</p>
    </div>
  {/if}

  <!-- Rollback notice -->
  {#if progressStore.rollbackPerformed}
    <div class="p-4 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded-lg">
      <p class="text-sm text-amber-700 dark:text-amber-400">{$_("errors.rollbackPerformed")}</p>
    </div>
  {/if}

  <!-- Done button (only shown when execution completes) -->
  {#if completed}
    <div class="flex justify-end">
      <button
        onclick={handleDone}
        class="px-5 py-2.5 text-sm font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors"
      >
        {$_("wizard.execute.continue")}
      </button>
    </div>
  {/if}
</div>
