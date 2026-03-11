<script lang="ts">
  import { _ } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { wizardStore, type WizardStep } from "../stores/wizardStore.svelte";
  import ConnectStep from "./ConnectStep.svelte";
  import ChooseStep from "./ChooseStep.svelte";
  import PreviewStep from "./PreviewStep.svelte";
  import ExecuteStep from "./ExecuteStep.svelte";
  import DoneStep from "./DoneStep.svelte";

  interface LastInstallProfile {
    seedUrl: string;
    competencyIds: string[];
    selectedTools: string[];
    scope: string;
    repoPath: string;
    installedAt: string;
  }

  interface InstallResult {
    success: boolean;
    componentsSucceeded: string[];
    componentsFailed: { componentId: string; error: string }[];
  }

  const steps: { id: WizardStep; labelKey: string }[] = [
    { id: "connect", labelKey: "wizard.steps.connect" },
    { id: "choose", labelKey: "wizard.steps.choose" },
    { id: "preview", labelKey: "wizard.steps.preview" },
    { id: "execute", labelKey: "wizard.steps.execute" },
    { id: "done", labelKey: "wizard.steps.done" },
  ];

  let lastProfile = $state<LastInstallProfile | null>(null);
  let quickUpdateRunning = $state(false);
  let quickUpdateDone = $state(false);
  let quickUpdateError = $state("");
  let quickUpdateResult = $state<InstallResult | null>(null);

  $effect(() => {
    invoke<LastInstallProfile | null>("load_last_install")
      .then(p => { lastProfile = p; })
      .catch(() => {});
  });

  async function runQuickUpdate() {
    quickUpdateRunning = true;
    quickUpdateError = "";
    quickUpdateResult = null;
    try {
      const result = await invoke<InstallResult>("quick_update");
      quickUpdateResult = result;
      quickUpdateDone = true;
    } catch (e) {
      quickUpdateError = String(e);
    } finally {
      quickUpdateRunning = false;
    }
  }

  function dismissQuickUpdate() {
    quickUpdateDone = false;
    quickUpdateResult = null;
    quickUpdateError = "";
  }

  function formatDate(iso: string): string {
    try { return new Date(iso).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" }); }
    catch { return iso; }
  }

  function goToStep(step: WizardStep) {
    if (wizardStore.isExecuting) return;
    const targetIdx = steps.findIndex((s) => s.id === step);
    const currentIdx = wizardStore.currentStepIndex;
    if (targetIdx <= currentIdx) wizardStore.setStep(step);
  }
</script>

<div class="max-w-4xl mx-auto">
  <!-- Quick-update banner — shown when a previous install profile exists -->
  {#if lastProfile && !quickUpdateDone && wizardStore.currentStep === "connect"}
    <div class="mb-6 p-4 rounded-xl border border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-900/20 flex items-start gap-4">
      <div class="flex-1 min-w-0">
        <p class="text-sm font-semibold text-blue-800 dark:text-blue-200">Update your last install</p>
        <p class="text-xs text-blue-600 dark:text-blue-400 mt-0.5">
          Last installed {formatDate(lastProfile.installedAt)} ·
          {lastProfile.competencyIds.length} competenc{lastProfile.competencyIds.length === 1 ? "y" : "ies"} ·
          {lastProfile.selectedTools.join(", ")}
        </p>
        {#if quickUpdateError}
          <p class="text-xs text-red-600 dark:text-red-400 mt-1">{quickUpdateError}</p>
        {/if}
      </div>
      <button
        onclick={runQuickUpdate}
        disabled={quickUpdateRunning}
        class="flex-shrink-0 px-4 py-2 text-sm font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center gap-2"
      >
        {#if quickUpdateRunning}
          <svg class="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
          </svg>
          Updating…
        {:else}
          Quick Update
        {/if}
      </button>
    </div>
  {/if}

  <!-- Quick-update result -->
  {#if quickUpdateDone && quickUpdateResult}
    {@const ok = quickUpdateResult.componentsSucceeded.length}
    {@const fail = quickUpdateResult.componentsFailed.length}
    <div class="mb-6 p-4 rounded-xl border {fail === 0 ? 'border-green-200 dark:border-green-800 bg-green-50 dark:bg-green-900/20' : 'border-amber-200 dark:border-amber-800 bg-amber-50 dark:bg-amber-900/20'} flex items-center gap-4">
      <span class="text-lg">{fail === 0 ? "✅" : "⚠️"}</span>
      <p class="flex-1 text-sm font-medium {fail === 0 ? 'text-green-800 dark:text-green-200' : 'text-amber-800 dark:text-amber-200'}">
        {ok} component{ok !== 1 ? "s" : ""} updated{fail > 0 ? `, ${fail} failed` : " successfully"}.
        Restart your AI tools to pick up the changes.
      </p>
      <button onclick={dismissQuickUpdate} class="text-xs text-gray-500 hover:text-gray-700 dark:hover:text-gray-300">Dismiss</button>
    </div>
  {/if}

  <!-- Step progress bar -->
  <nav class="mb-8" aria-label="Wizard progress">
    <ol class="flex items-center w-full">
      {#each steps as step, i}
        {@const isCurrent = step.id === wizardStore.currentStep}
        {@const isCompleted = i < wizardStore.currentStepIndex}
        {@const isClickable = !wizardStore.isExecuting && i <= wizardStore.currentStepIndex}
        <li class="flex items-center {i < steps.length - 1 ? 'flex-1' : ''}">
          <button
            onclick={() => goToStep(step.id)}
            disabled={!isClickable}
            class="flex items-center gap-2 text-sm font-medium transition-colors
              {isCurrent
                ? 'text-blue-600 dark:text-blue-400'
                : isCompleted
                  ? 'text-green-600 dark:text-green-400'
                  : 'text-gray-400 dark:text-gray-500'}
              {isClickable ? 'cursor-pointer hover:text-blue-500' : 'cursor-default'}"
            aria-current={isCurrent ? "step" : undefined}
          >
            <span
              class="flex items-center justify-center w-8 h-8 rounded-full border-2 text-xs font-bold
                {isCurrent
                  ? 'border-blue-600 dark:border-blue-400 bg-blue-50 dark:bg-blue-900/30'
                  : isCompleted
                    ? 'border-green-600 dark:border-green-400 bg-green-50 dark:bg-green-900/30'
                    : 'border-gray-300 dark:border-gray-600'}"
            >
              {#if isCompleted}
                <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                  <path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd" />
                </svg>
              {:else}
                {i + 1}
              {/if}
            </span>
            <span class="hidden sm:inline">{$_(step.labelKey)}</span>
          </button>
          {#if i < steps.length - 1}
            <div
              class="flex-1 h-0.5 mx-3 {isCompleted ? 'bg-green-400 dark:bg-green-600' : 'bg-gray-200 dark:bg-gray-700'}"
            ></div>
          {/if}
        </li>
      {/each}
    </ol>
  </nav>

  <!-- Step content -->
  <div class="min-h-[400px]">
    {#if wizardStore.currentStep === "connect"}
      <ConnectStep />
    {:else if wizardStore.currentStep === "choose"}
      <ChooseStep />
    {:else if wizardStore.currentStep === "preview"}
      <PreviewStep />
    {:else if wizardStore.currentStep === "execute"}
      <ExecuteStep />
    {:else if wizardStore.currentStep === "done"}
      <DoneStep />
    {/if}
  </div>

  <!-- Navigation buttons — hidden on connect step (it has its own actions) -->
  {#if wizardStore.currentStep !== "connect"}
  <div class="flex justify-between mt-6 pt-4 border-t border-gray-200 dark:border-gray-700">
    <button
      onclick={() => wizardStore.prevStep()}
      disabled={!wizardStore.canGoBack}
      class="px-4 py-2 text-sm font-medium rounded-lg border border-gray-300 dark:border-gray-600
        {wizardStore.canGoBack
          ? 'text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800'
          : 'text-gray-400 dark:text-gray-600 cursor-not-allowed'}"
    >
      {$_("common.back")}
    </button>
    {#if !["preview", "execute", "done"].includes(wizardStore.currentStep)}
      <button
        onclick={() => wizardStore.nextStep()}
        disabled={!wizardStore.canGoForward}
        class="px-4 py-2 text-sm font-medium rounded-lg
          {wizardStore.canGoForward
            ? 'bg-blue-600 text-white hover:bg-blue-700'
            : 'bg-gray-300 dark:bg-gray-700 text-gray-500 dark:text-gray-400 cursor-not-allowed'}"
      >
        {$_("common.next")}
      </button>
    {/if}
  </div>
  {/if}
</div>
