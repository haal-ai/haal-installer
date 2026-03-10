<script lang="ts">
  import { _ } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";

  type InstallPhase = "options" | "installing" | "success" | "error";

  interface Step {
    label: string;
    done: boolean;
    active: boolean;
  }

  // --- Props ---
  let { needsUpdate = false }: { needsUpdate?: boolean } = $props();

  // --- State ---
  let phase = $state<InstallPhase>("options");
  let addToPath = $state(true);
  let createShortcut = $state(false);
  let errorMessage = $state("");
  let steps = $state<Step[]>([
    { label: "selfInstall.stepCreatingDirs", done: false, active: false },
    { label: "selfInstall.stepCopyingBinary", done: false, active: false },
    { label: "selfInstall.stepConfiguring", done: false, active: false },
  ]);

  // --- Derived ---
  let title = $derived(
    needsUpdate ? $_("selfInstall.updating") : $_("selfInstall.title")
  );
  let isInstalling = $derived(phase === "installing");

  // --- Helpers ---

  function updateStep(index: number, updates: Partial<Step>) {
    steps = steps.map((s, i) =>
      i === index ? { ...s, ...updates } : s
    );
  }

  async function delay(ms: number) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  async function handleInstall() {
    phase = "installing";
    errorMessage = "";

    try {
      // Step 1: Creating directories
      updateStep(0, { active: true });
      await invoke("self_install");
      updateStep(0, { done: true, active: false });

      // Step 2: Binary copy (already done by self_install, but show progress)
      updateStep(1, { active: true });
      // Use the correct label for update vs fresh install
      if (needsUpdate) {
        steps = steps.map((s, i) =>
          i === 1 ? { ...s, label: "selfInstall.stepUpdatingBinary" } : s
        );
      }
      await delay(300);
      updateStep(1, { done: true, active: false });

      // Step 3: Configuring (shortcuts, PATH)
      updateStep(2, { active: true });

      if (createShortcut) {
        try {
          await invoke("create_desktop_shortcut");
        } catch (err) {
          console.warn("Desktop shortcut creation failed:", err);
        }
      }

      if (addToPath) {
        try {
          await invoke("add_to_path");
        } catch (err) {
          console.warn("Add to PATH failed:", err);
        }
      }

      await delay(200);
      updateStep(2, { done: true, active: false });

      // Success
      phase = "success";

      // Relaunch after a brief pause so the user sees the success message
      await delay(1500);
      await invoke("relaunch_from_home");
    } catch (err) {
      phase = "error";
      errorMessage = String(err);
    }
  }

  function handleRetry() {
    // Reset steps
    steps = steps.map((s) => ({ ...s, done: false, active: false }));
    handleInstall();
  }
</script>

<!-- Modal overlay -->
<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
  role="dialog"
  aria-modal="true"
  aria-labelledby="self-install-title"
>
  <div class="w-full max-w-md mx-4 bg-white dark:bg-gray-800 rounded-xl shadow-2xl border border-gray-200 dark:border-gray-700 overflow-hidden">
    <!-- Header -->
    <div class="px-6 pt-6 pb-4">
      <h2
        id="self-install-title"
        class="text-xl font-semibold text-gray-900 dark:text-gray-100"
      >
        {title}
      </h2>
      <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
        {$_("selfInstall.description")}
      </p>
    </div>

    <!-- Progress steps -->
    <div class="px-6 space-y-3">
      {#each steps as step, i}
        <div class="flex items-center gap-3">
          <!-- Step indicator -->
          {#if step.done}
            <svg class="w-5 h-5 text-green-500 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
            </svg>
          {:else if step.active}
            <svg class="w-5 h-5 text-blue-500 animate-spin flex-shrink-0" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
            </svg>
          {:else}
            <div class="w-5 h-5 rounded-full border-2 border-gray-300 dark:border-gray-600 flex-shrink-0"></div>
          {/if}
          <span
            class="text-sm {step.done
              ? 'text-green-700 dark:text-green-400'
              : step.active
                ? 'text-blue-700 dark:text-blue-400 font-medium'
                : 'text-gray-500 dark:text-gray-400'}"
          >
            {$_(step.label)}
          </span>
        </div>
      {/each}
    </div>

    <!-- Options (only shown before install starts) -->
    {#if phase === "options"}
      <div class="px-6 pt-4 space-y-3">
        <label class="flex items-center gap-3 cursor-pointer">
          <input
            type="checkbox"
            bind:checked={addToPath}
            class="w-4 h-4 rounded border-gray-300 dark:border-gray-600 text-blue-600 focus:ring-blue-500"
          />
          <span class="text-sm text-gray-700 dark:text-gray-300">
            {$_("selfInstall.addToPath")}
          </span>
        </label>
        <label class="flex items-center gap-3 cursor-pointer">
          <input
            type="checkbox"
            bind:checked={createShortcut}
            class="w-4 h-4 rounded border-gray-300 dark:border-gray-600 text-blue-600 focus:ring-blue-500"
          />
          <span class="text-sm text-gray-700 dark:text-gray-300">
            {$_("selfInstall.createShortcut")}
          </span>
        </label>
      </div>
    {/if}

    <!-- Success message -->
    {#if phase === "success"}
      <div class="px-6 pt-4">
        <div class="p-3 bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-lg">
          <p class="text-sm text-green-700 dark:text-green-400">
            {$_("selfInstall.successMessage")}
          </p>
        </div>
      </div>
    {/if}

    <!-- Error message -->
    {#if phase === "error"}
      <div class="px-6 pt-4">
        <div class="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg">
          <p class="text-sm text-red-700 dark:text-red-300">
            {$_("selfInstall.errorMessage", { values: { message: errorMessage } })}
          </p>
        </div>
      </div>
    {/if}

    <!-- Actions -->
    <div class="px-6 py-4 mt-2 flex justify-end gap-3">
      {#if phase === "options"}
        <button
          onclick={handleInstall}
          class="px-5 py-2.5 text-sm font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 dark:focus:ring-offset-gray-800"
        >
          {$_("selfInstall.install")}
        </button>
      {:else if phase === "installing"}
        <button
          disabled
          class="px-5 py-2.5 text-sm font-medium rounded-lg bg-blue-600/60 text-white cursor-not-allowed flex items-center gap-2"
        >
          <svg class="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
          </svg>
          {$_("selfInstall.installing")}
        </button>
      {:else if phase === "error"}
        <button
          onclick={handleRetry}
          class="px-5 py-2.5 text-sm font-medium rounded-lg bg-red-600 text-white hover:bg-red-700 transition-colors focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2 dark:focus:ring-offset-gray-800"
        >
          {$_("selfInstall.retry")}
        </button>
      {/if}
    </div>
  </div>
</div>
