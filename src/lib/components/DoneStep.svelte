<script lang="ts">
  import { _ } from "svelte-i18n";
  import { wizardStore } from "../stores/wizardStore.svelte";
  import { progressStore } from "../stores/progressStore.svelte";

  // --- Derived state ---

  let succeeded = $derived(progressStore.progress.componentsSucceeded);
  let failed = $derived(progressStore.progress.componentsFailed);
  let totalCount = $derived(succeeded.length + failed.length);
  let allSucceeded = $derived(failed.length === 0 && succeeded.length > 0);
  let rollbackPerformed = $derived(progressStore.rollbackPerformed);

  // --- Actions ---

  function handleRetryFailed() {
    // Set selected components to only the failed ones, go back to execute step
    const failedComponents = wizardStore.selectedComponents.filter((c) =>
      failed.includes(c.id)
    );
    wizardStore.setSelectedComponents(failedComponents);
    wizardStore.setStep("execute");
  }

  function handleViewLogs() {
    // Dispatch a custom event to navigate to logs view at the App level
    window.dispatchEvent(new CustomEvent("navigate", { detail: "logs" }));
  }

  function handleFinish() {
    wizardStore.reset();
    progressStore.reset();
  }
</script>

<div class="space-y-6">
  <!-- Header with success/failure icon -->
  <div class="text-center">
    {#if allSucceeded}
      <div class="mx-auto w-16 h-16 flex items-center justify-center rounded-full bg-green-100 dark:bg-green-900/30 mb-4">
        <svg class="w-8 h-8 text-green-600 dark:text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
        </svg>
      </div>
    {:else}
      <div class="mx-auto w-16 h-16 flex items-center justify-center rounded-full bg-amber-100 dark:bg-amber-900/30 mb-4">
        <svg class="w-8 h-8 text-amber-600 dark:text-amber-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
      </div>
    {/if}
    <h2 class="text-xl font-semibold text-gray-900 dark:text-gray-100">
      {$_("wizard.done.title")}
    </h2>
    <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
      {$_("wizard.done.description")}
    </p>
  </div>

  <!-- Summary counts -->
  <div class="flex justify-center gap-6">
    <div class="flex items-center gap-2 px-4 py-2 rounded-lg bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800">
      <svg class="w-5 h-5 text-green-600 dark:text-green-400" fill="currentColor" viewBox="0 0 20 20">
        <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd" />
      </svg>
      <span class="text-sm font-medium text-green-700 dark:text-green-300">
        {$_("wizard.done.succeeded", { values: { count: succeeded.length } })}
      </span>
    </div>
    {#if failed.length > 0}
      <div class="flex items-center gap-2 px-4 py-2 rounded-lg bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800">
        <svg class="w-5 h-5 text-red-600 dark:text-red-400" fill="currentColor" viewBox="0 0 20 20">
          <path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd" />
        </svg>
        <span class="text-sm font-medium text-red-700 dark:text-red-300">
          {$_("wizard.done.failed", { values: { count: failed.length } })}
        </span>
      </div>
    {/if}
  </div>

  <!-- Rollback notification -->
  {#if rollbackPerformed}
    <div class="p-4 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded-lg flex items-start gap-3">
      <svg class="w-5 h-5 text-amber-600 dark:text-amber-400 flex-shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
        <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
      <p class="text-sm text-amber-700 dark:text-amber-400">
        {$_("wizard.done.rollbackNotice")}
      </p>
    </div>
  {/if}

  <!-- Succeeded components list -->
  {#if succeeded.length > 0}
    <div>
      <h3 class="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">
        {$_("wizard.done.succeededTitle")}
      </h3>
      <div class="space-y-1.5">
        {#each succeeded as componentId}
          {@const comp = wizardStore.selectedComponents.find((c) => c.id === componentId)}
          <div class="flex items-center gap-2.5 p-2.5 border border-green-200 dark:border-green-800 rounded-lg bg-green-50 dark:bg-green-900/10">
            <svg class="w-4 h-4 text-green-600 dark:text-green-400 flex-shrink-0" fill="currentColor" viewBox="0 0 20 20">
              <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd" />
            </svg>
            <span class="text-sm text-gray-900 dark:text-gray-100">{comp?.name ?? componentId}</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Failed components list -->
  {#if failed.length > 0}
    <div>
      <h3 class="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-2">
        {$_("wizard.done.failedTitle")}
      </h3>
      <div class="space-y-1.5">
        {#each failed as componentId}
          {@const comp = wizardStore.selectedComponents.find((c) => c.id === componentId)}
          <div class="p-2.5 border border-red-200 dark:border-red-800 rounded-lg bg-red-50 dark:bg-red-900/10">
            <div class="flex items-center gap-2.5">
              <svg class="w-4 h-4 text-red-600 dark:text-red-400 flex-shrink-0" fill="currentColor" viewBox="0 0 20 20">
                <path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd" />
              </svg>
              <span class="text-sm text-gray-900 dark:text-gray-100">{comp?.name ?? componentId}</span>
            </div>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Verification summary -->
  <div class="p-4 bg-gray-50 dark:bg-gray-800/50 border border-gray-200 dark:border-gray-700 rounded-lg">
    <h3 class="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-3">
      {$_("wizard.done.verificationTitle")}
    </h3>
    <div class="space-y-2">
      <div class="flex items-center justify-between text-sm">
        <span class="text-gray-600 dark:text-gray-400">{$_("wizard.done.fileExistence")}</span>
        <span class="font-medium {allSucceeded ? 'text-green-600 dark:text-green-400' : 'text-amber-600 dark:text-amber-400'}">
          {allSucceeded ? $_("wizard.done.verificationPassed") : $_("wizard.done.verificationPartial", { values: { succeeded: succeeded.length, total: totalCount } })}
        </span>
      </div>
      <div class="flex items-center justify-between text-sm">
        <span class="text-gray-600 dark:text-gray-400">{$_("wizard.done.checksumStatus")}</span>
        <span class="font-medium {allSucceeded ? 'text-green-600 dark:text-green-400' : 'text-amber-600 dark:text-amber-400'}">
          {allSucceeded ? $_("wizard.done.verificationPassed") : $_("wizard.done.verificationPartial", { values: { succeeded: succeeded.length, total: totalCount } })}
        </span>
      </div>
    </div>
  </div>

  <!-- Action buttons -->
  <div class="flex items-center justify-end gap-3 pt-2">
    {#if failed.length > 0}
      <button
        onclick={handleRetryFailed}
        class="px-4 py-2 text-sm font-medium rounded-lg border border-red-300 dark:border-red-700 text-red-700 dark:text-red-300 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors"
      >
        {$_("wizard.done.retry")}
      </button>
    {/if}
    <button
      onclick={handleViewLogs}
      class="px-4 py-2 text-sm font-medium rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
    >
      {$_("wizard.done.viewLogs")}
    </button>
    <button
      onclick={handleFinish}
      class="px-5 py-2.5 text-sm font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors"
    >
      {$_("wizard.done.finish")}
    </button>
  </div>
</div>
