<script lang="ts">
  import { _ } from "svelte-i18n";
  import { wizardStore, type WizardStep } from "../stores/wizardStore";
  import ConnectStep from "./ConnectStep.svelte";
  import ChooseStep from "./ChooseStep.svelte";
  import PreviewStep from "./PreviewStep.svelte";
  import ExecuteStep from "./ExecuteStep.svelte";
  import DoneStep from "./DoneStep.svelte";

  const steps: { id: WizardStep; labelKey: string }[] = [
    { id: "connect", labelKey: "wizard.steps.connect" },
    { id: "choose", labelKey: "wizard.steps.choose" },
    { id: "preview", labelKey: "wizard.steps.preview" },
    { id: "execute", labelKey: "wizard.steps.execute" },
    { id: "done", labelKey: "wizard.steps.done" },
  ];

  function goToStep(step: WizardStep) {
    if (wizardStore.isExecuting) return;
    const targetIdx = steps.findIndex((s) => s.id === step);
    const currentIdx = wizardStore.currentStepIndex;
    // Only allow clicking on completed (earlier) steps or current step
    if (targetIdx <= currentIdx) {
      wizardStore.setStep(step);
    }
  }
</script>

<div class="max-w-4xl mx-auto">
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

  <!-- Navigation buttons -->
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
  </div>
</div>
