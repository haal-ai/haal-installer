<script lang="ts">
  import { _ } from "svelte-i18n";
  import { settingsStore, type Theme, SUPPORTED_TOOLS, type SupportedTool } from "../stores/settingsStore.svelte";

  const TOOL_DESCRIPTIONS: Record<SupportedTool, string> = {
    "Kiro":        "Skills, Powers, Hooks, MCP",
    "Copilot":     "Skills, Rules",
    "Cursor":      "Skills, Rules, MCP",
    "Claude Code": "Skills, Commands, Rules, MCP",
    "Windsurf":    "Skills, Commands, Rules, MCP (global)",
  };

  let profileMessage = $state("");
  let profileError = $state("");

  async function exportConfiguration() {
    profileMessage = ""; profileError = "";
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("export_configuration", { path: "" });
      profileMessage = $_("settings.profile.exportSuccess");
    } catch (err) { profileError = String(err); }
  }

  async function importConfiguration() {
    profileMessage = ""; profileError = "";
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("import_configuration", { path: "" });
      profileMessage = $_("settings.profile.importSuccess");
      await settingsStore.loadFromBackend();
    } catch (err) { profileError = String(err); }
  }
</script>

<div class="max-w-2xl mx-auto space-y-6">
  <h2 class="text-xl font-semibold text-gray-900 dark:text-gray-100">
    {$_("settings.title")}
  </h2>

  <!-- ==================== Supported Tools ==================== -->
  <section class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-5">
    <div class="mb-4">
      <h3 class="text-base font-semibold text-gray-900 dark:text-gray-100">Supported tools</h3>
      <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
        Choose which AI tools HAAL installs components for. Install paths are resolved automatically.
      </p>
    </div>

    <div class="space-y-2">
      {#each SUPPORTED_TOOLS as tool}
        {@const enabled = settingsStore.isToolEnabled(tool)}
        <div class="flex items-center gap-3 p-3 rounded-lg bg-gray-50 dark:bg-gray-700/50">
          <button
            onclick={() => settingsStore.toggleTool(tool)}
            class="shrink-0 w-9 h-5 rounded-full transition-colors relative {enabled ? 'bg-blue-600' : 'bg-gray-300 dark:bg-gray-600'}"
            role="switch"
            aria-checked={enabled}
            aria-label="{tool}"
          >
            <span class="absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform {enabled ? 'translate-x-4' : 'translate-x-0'}"></span>
          </button>
          <div class="flex-1 min-w-0">
            <span class="text-sm font-medium text-gray-900 dark:text-gray-100">{tool}</span>
            <span class="text-xs text-gray-400 dark:text-gray-500 ml-2">{TOOL_DESCRIPTIONS[tool]}</span>
          </div>
        </div>
      {/each}
    </div>
  </section>

  <!-- ==================== Preferences ==================== -->
  <section class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-5">
    <h3 class="text-base font-semibold text-gray-900 dark:text-gray-100 mb-4">
      {$_("settings.preferences.title")}
    </h3>

    <div class="space-y-4">
      <!-- Theme -->
      <div class="flex items-center justify-between">
        <label class="text-sm font-medium text-gray-700 dark:text-gray-300">
          {$_("settings.preferences.theme")}
        </label>
        <div class="flex gap-1 bg-gray-100 dark:bg-gray-700 rounded-lg p-1">
          {#each [
            { value: "light", label: $_("settings.preferences.themeLight") },
            { value: "dark",  label: $_("settings.preferences.themeDark") },
            { value: "system",label: $_("settings.preferences.themeSystem") },
          ] as opt (opt.value)}
            <button
              onclick={() => settingsStore.setTheme(opt.value as Theme)}
              class="px-3 py-1 text-sm rounded-md transition-colors
                {settingsStore.theme === opt.value
                  ? 'bg-white dark:bg-gray-600 text-gray-900 dark:text-gray-100 shadow-sm'
                  : 'text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200'}"
            >
              {opt.label}
            </button>
          {/each}
        </div>
      </div>

      <!-- Language -->
      <div class="flex items-center justify-between">
        <label class="text-sm font-medium text-gray-700 dark:text-gray-300">
          {$_("settings.preferences.language")}
        </label>
        <div class="flex gap-1 bg-gray-100 dark:bg-gray-700 rounded-lg p-1">
          {#each [
            { value: "en", label: $_("settings.preferences.langEnglish") },
            { value: "fr", label: $_("settings.preferences.langFrench") },
          ] as opt (opt.value)}
            <button
              onclick={() => settingsStore.setLanguage(opt.value)}
              class="px-3 py-1 text-sm rounded-md transition-colors
                {settingsStore.language === opt.value
                  ? 'bg-white dark:bg-gray-600 text-gray-900 dark:text-gray-100 shadow-sm'
                  : 'text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200'}"
            >
              {opt.label}
            </button>
          {/each}
        </div>
      </div>

      <!-- Auto-update -->
      <div class="flex items-center justify-between">
        <label class="text-sm font-medium text-gray-700 dark:text-gray-300">
          {$_("settings.preferences.autoUpdate")}
        </label>
        <button
          onclick={() => settingsStore.setAutoUpdate(!settingsStore.autoUpdate)}
          class="w-9 h-5 rounded-full transition-colors relative {settingsStore.autoUpdate ? 'bg-blue-600' : 'bg-gray-300 dark:bg-gray-600'}"
          role="switch" aria-checked={settingsStore.autoUpdate}
        >
          <span class="absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform {settingsStore.autoUpdate ? 'translate-x-4' : 'translate-x-0'}"></span>
        </button>
      </div>

      <!-- Parallel operations -->
      <div class="flex items-center justify-between">
        <label class="text-sm font-medium text-gray-700 dark:text-gray-300">
          {$_("settings.preferences.parallelOps")}
        </label>
        <button
          onclick={() => settingsStore.setParallelOperations(!settingsStore.parallelOperations)}
          class="w-9 h-5 rounded-full transition-colors relative {settingsStore.parallelOperations ? 'bg-blue-600' : 'bg-gray-300 dark:bg-gray-600'}"
          role="switch" aria-checked={settingsStore.parallelOperations}
        >
          <span class="absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform {settingsStore.parallelOperations ? 'translate-x-4' : 'translate-x-0'}"></span>
        </button>
      </div>
    </div>
  </section>

  <!-- ==================== Profile ==================== -->
  <section class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-5">
    <h3 class="text-base font-semibold text-gray-900 dark:text-gray-100 mb-4">
      {$_("settings.profile.title")}
    </h3>
    <div class="flex gap-3">
      <button onclick={exportConfiguration}
        class="px-4 py-2 text-sm font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors">
        {$_("settings.profile.export")}
      </button>
      <button onclick={importConfiguration}
        class="px-4 py-2 text-sm font-medium rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors">
        {$_("settings.profile.import")}
      </button>
    </div>
    {#if profileMessage}
      <p class="mt-3 text-sm text-green-600 dark:text-green-400">{profileMessage}</p>
    {/if}
    {#if profileError}
      <p class="mt-3 text-sm text-red-600 dark:text-red-400">{profileError}</p>
    {/if}
  </section>
</div>
