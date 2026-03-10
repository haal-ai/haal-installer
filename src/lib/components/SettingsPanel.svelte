<script lang="ts">
  import { _ } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { settingsStore, type Theme } from "../stores/settingsStore";

  // --- Repository state ---
  interface Repository {
    id: string;
    url: string;
    enabled: boolean;
    priority: number;
    source: "registry" | "custom";
  }

  let repositories = $state<Repository[]>([]);
  let newRepoUrl = $state("");
  let repoError = $state("");

  // --- Destination state ---
  interface Destination {
    tool: string;
    path: string;
    enabled: boolean;
    writable: boolean | null;
  }

  const toolNames = ["Kiro", "Copilot", "Cursor", "Claude Code", "Windsurf"];

  let destinations = $state<Destination[]>(
    toolNames.map((tool) => ({
      tool,
      path: "",
      enabled: true,
      writable: null,
    })),
  );

  // --- Profile state ---
  let profileMessage = $state("");
  let profileError = $state("");

  // Load repositories from backend
  async function loadRepositories() {
    try {
      const config = await invoke<{
        repositories?: Array<{
          id?: string;
          url: string;
          enabled?: boolean;
          priority?: number;
          source?: string;
        }>;
      }>("get_config");
      if (config?.repositories) {
        repositories = config.repositories.map((r, i) => ({
          id: r.id ?? `repo-${i}`,
          url: r.url,
          enabled: r.enabled ?? true,
          priority: r.priority ?? i + 1,
          source: (r.source as "registry" | "custom") ?? "custom",
        }));
      }
    } catch {
      // Backend not ready
    }
  }

  // Load destinations from backend
  async function loadDestinations() {
    try {
      const config = await invoke<{
        destinations?: Array<{
          tool: string;
          path: string;
          enabled: boolean;
          writable: boolean;
        }>;
      }>("get_config");
      if (config?.destinations) {
        destinations = toolNames.map((tool) => {
          const found = config.destinations!.find(
            (d) => d.tool.toLowerCase() === tool.toLowerCase(),
          );
          return found
            ? { ...found, tool, writable: found.writable }
            : { tool, path: "", enabled: true, writable: null };
        });
      }
    } catch {
      // Backend not ready
    }
  }

  loadRepositories();
  loadDestinations();

  // --- Repository actions ---
  async function addRepository() {
    if (!newRepoUrl.trim()) return;
    repoError = "";
    const newRepo: Repository = {
      id: `custom-${Date.now()}`,
      url: newRepoUrl.trim(),
      enabled: true,
      priority:
        repositories.length > 0
          ? Math.max(...repositories.map((r) => r.priority)) + 1
          : 1,
      source: "custom",
    };
    repositories = [...repositories, newRepo];
    newRepoUrl = "";
    await saveRepositories();
  }

  async function removeRepository(id: string) {
    repositories = repositories.filter((r) => r.id !== id);
    await saveRepositories();
  }

  async function toggleRepository(id: string) {
    repositories = repositories.map((r) =>
      r.id === id ? { ...r, enabled: !r.enabled } : r,
    );
    await saveRepositories();
  }

  async function moveRepository(id: string, direction: "up" | "down") {
    const idx = repositories.findIndex((r) => r.id === id);
    if (idx < 0) return;
    const swapIdx = direction === "up" ? idx - 1 : idx + 1;
    if (swapIdx < 0 || swapIdx >= repositories.length) return;
    const updated = [...repositories];
    // Swap priorities
    const tmpPriority = updated[idx].priority;
    updated[idx] = { ...updated[idx], priority: updated[swapIdx].priority };
    updated[swapIdx] = { ...updated[swapIdx], priority: tmpPriority };
    // Swap positions
    [updated[idx], updated[swapIdx]] = [updated[swapIdx], updated[idx]];
    repositories = updated;
    await saveRepositories();
  }

  async function saveRepositories() {
    try {
      await invoke("save_config", {
        config: {
          repositories: repositories.map((r) => ({
            id: r.id,
            url: r.url,
            enabled: r.enabled,
            priority: r.priority,
            source: r.source,
          })),
        },
      });
    } catch {
      // Config save may fail
    }
  }

  // --- Destination actions ---
  async function toggleDestination(tool: string) {
    destinations = destinations.map((d) =>
      d.tool === tool ? { ...d, enabled: !d.enabled } : d,
    );
    await saveDestinations();
  }

  async function updateDestinationPath(tool: string, path: string) {
    destinations = destinations.map((d) =>
      d.tool === tool ? { ...d, path, writable: null } : d,
    );
    await saveDestinations();
  }

  async function validateWritability(tool: string) {
    const dest = destinations.find((d) => d.tool === tool);
    if (!dest || !dest.path) return;
    try {
      const writable = await invoke<boolean>("validate_writability", {
        path: dest.path,
      });
      destinations = destinations.map((d) =>
        d.tool === tool ? { ...d, writable } : d,
      );
    } catch {
      destinations = destinations.map((d) =>
        d.tool === tool ? { ...d, writable: false } : d,
      );
    }
  }

  async function saveDestinations() {
    try {
      await invoke("save_config", {
        config: {
          destinations: destinations.map((d) => ({
            tool: d.tool,
            path: d.path,
            enabled: d.enabled,
            writable: d.writable ?? false,
          })),
        },
      });
    } catch {
      // Config save may fail
    }
  }

  // --- Profile actions ---
  async function exportConfiguration() {
    profileMessage = "";
    profileError = "";
    try {
      await invoke("export_configuration", { path: "" });
      profileMessage = $_("settings.profile.exportSuccess");
    } catch (err) {
      profileError = String(err);
    }
  }

  async function importConfiguration() {
    profileMessage = "";
    profileError = "";
    try {
      await invoke("import_configuration", { path: "" });
      profileMessage = $_("settings.profile.importSuccess");
      // Reload all settings after import
      await settingsStore.loadFromBackend();
      await loadRepositories();
      await loadDestinations();
    } catch (err) {
      profileError = String(err);
    }
  }
</script>

<div class="max-w-3xl mx-auto space-y-8">
  <h2 class="text-2xl font-bold text-gray-900 dark:text-gray-100">
    {$_("settings.title")}
  </h2>

  <!-- ==================== Repositories Section ==================== -->
  <section class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6">
    <h3 class="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4">
      {$_("settings.repositories.title")}
    </h3>

    <!-- Repository list -->
    {#if repositories.length > 0}
      <ul class="space-y-3 mb-4">
        {#each repositories as repo, idx (repo.id)}
          <li class="flex items-center gap-3 p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg">
            <!-- Enable/disable toggle -->
            <button
              onclick={() => toggleRepository(repo.id)}
              class="shrink-0 w-10 h-6 rounded-full transition-colors relative
                {repo.enabled ? 'bg-blue-600' : 'bg-gray-300 dark:bg-gray-600'}"
              aria-label={repo.enabled ? $_("settings.repositories.disable") : $_("settings.repositories.enable")}
            >
              <span
                class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform
                  {repo.enabled ? 'translate-x-4' : 'translate-x-0'}"
              ></span>
            </button>

            <!-- Repo info -->
            <div class="flex-1 min-w-0">
              <p class="text-sm font-medium text-gray-900 dark:text-gray-100 truncate">
                {repo.url}
              </p>
              <p class="text-xs text-gray-500 dark:text-gray-400">
                {$_("settings.repositories.priority")}: {repo.priority} · {repo.source === "registry" ? $_("settings.repositories.sourceRegistry") : $_("settings.repositories.sourceCustom")}
              </p>
            </div>

            <!-- Reorder buttons -->
            <div class="flex flex-col gap-0.5">
              <button
                onclick={() => moveRepository(repo.id, "up")}
                disabled={idx === 0}
                class="p-1 rounded hover:bg-gray-200 dark:hover:bg-gray-600 disabled:opacity-30 disabled:cursor-not-allowed text-gray-500 dark:text-gray-400"
                aria-label={$_("settings.repositories.moveUp")}
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
                </svg>
              </button>
              <button
                onclick={() => moveRepository(repo.id, "down")}
                disabled={idx === repositories.length - 1}
                class="p-1 rounded hover:bg-gray-200 dark:hover:bg-gray-600 disabled:opacity-30 disabled:cursor-not-allowed text-gray-500 dark:text-gray-400"
                aria-label={$_("settings.repositories.moveDown")}
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                </svg>
              </button>
            </div>

            <!-- Remove button -->
            <button
              onclick={() => removeRepository(repo.id)}
              class="shrink-0 p-1.5 rounded-lg hover:bg-red-100 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400"
              aria-label={$_("settings.repositories.remove")}
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    <!-- Add repository -->
    <div class="flex gap-2">
      <input
        type="url"
        bind:value={newRepoUrl}
        placeholder={$_("settings.repositories.urlPlaceholder")}
        class="flex-1 px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
          bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
          focus:ring-2 focus:ring-blue-500 focus:border-blue-500 outline-none"
      />
      <button
        onclick={addRepository}
        disabled={!newRepoUrl.trim()}
        class="px-4 py-2 text-sm font-medium rounded-lg transition-colors
          {newRepoUrl.trim()
            ? 'bg-blue-600 text-white hover:bg-blue-700'
            : 'bg-gray-300 dark:bg-gray-700 text-gray-500 cursor-not-allowed'}"
      >
        {$_("settings.repositories.add")}
      </button>
    </div>
    {#if repoError}
      <p class="mt-2 text-sm text-red-600 dark:text-red-400">{repoError}</p>
    {/if}
  </section>

  <!-- ==================== Destinations Section ==================== -->
  <section class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6">
    <h3 class="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4">
      {$_("settings.destinations.title")}
    </h3>

    <div class="space-y-4">
      {#each destinations as dest (dest.tool)}
        <div class="flex items-center gap-3 p-3 bg-gray-50 dark:bg-gray-700/50 rounded-lg">
          <!-- Enable/disable toggle -->
          <button
            onclick={() => toggleDestination(dest.tool)}
            class="shrink-0 w-10 h-6 rounded-full transition-colors relative
              {dest.enabled ? 'bg-blue-600' : 'bg-gray-300 dark:bg-gray-600'}"
            aria-label="{dest.tool} {dest.enabled ? $_('settings.destinations.enabled') : $_('settings.repositories.disable')}"
          >
            <span
              class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform
                {dest.enabled ? 'translate-x-4' : 'translate-x-0'}"
            ></span>
          </button>

          <!-- Tool name -->
          <span class="shrink-0 w-28 text-sm font-medium text-gray-900 dark:text-gray-100">
            {dest.tool}
          </span>

          <!-- Path input -->
          <input
            type="text"
            value={dest.path}
            oninput={(e) => updateDestinationPath(dest.tool, (e.target as HTMLInputElement).value)}
            placeholder={$_("settings.destinations.pathPlaceholder")}
            class="flex-1 px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
              bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
              focus:ring-2 focus:ring-blue-500 focus:border-blue-500 outline-none"
          />

          <!-- Validate writability -->
          <button
            onclick={() => validateWritability(dest.tool)}
            disabled={!dest.path}
            class="shrink-0 px-3 py-1.5 text-xs font-medium rounded-lg transition-colors
              {dest.path
                ? 'bg-gray-200 dark:bg-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-300 dark:hover:bg-gray-500'
                : 'bg-gray-100 dark:bg-gray-700 text-gray-400 cursor-not-allowed'}"
          >
            {$_("settings.destinations.validate")}
          </button>

          <!-- Writability indicator -->
          <span class="shrink-0 flex items-center gap-1 text-xs">
            {#if dest.writable === true}
              <span class="w-2.5 h-2.5 rounded-full bg-green-500"></span>
              <span class="text-green-600 dark:text-green-400">{$_("settings.destinations.writable")}</span>
            {:else if dest.writable === false}
              <span class="w-2.5 h-2.5 rounded-full bg-red-500"></span>
              <span class="text-red-600 dark:text-red-400">{$_("settings.destinations.notWritable")}</span>
            {:else}
              <span class="w-2.5 h-2.5 rounded-full bg-gray-300 dark:bg-gray-600"></span>
            {/if}
          </span>
        </div>
      {/each}
    </div>
  </section>

  <!-- ==================== Preferences Section ==================== -->
  <section class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6">
    <h3 class="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4">
      {$_("settings.preferences.title")}
    </h3>

    <div class="space-y-4">
      <!-- Theme selector -->
      <div class="flex items-center justify-between">
        <label class="text-sm font-medium text-gray-700 dark:text-gray-300">
          {$_("settings.preferences.theme")}
        </label>
        <div class="flex gap-1 bg-gray-100 dark:bg-gray-700 rounded-lg p-1">
          {#each [
            { value: "light", label: $_("settings.preferences.themeLight") },
            { value: "dark", label: $_("settings.preferences.themeDark") },
            { value: "system", label: $_("settings.preferences.themeSystem") },
          ] as option (option.value)}
            <button
              onclick={() => settingsStore.setTheme(option.value as Theme)}
              class="px-3 py-1 text-sm rounded-md transition-colors
                {settingsStore.theme === option.value
                  ? 'bg-white dark:bg-gray-600 text-gray-900 dark:text-gray-100 shadow-sm'
                  : 'text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200'}"
            >
              {option.label}
            </button>
          {/each}
        </div>
      </div>

      <!-- Language selector -->
      <div class="flex items-center justify-between">
        <label class="text-sm font-medium text-gray-700 dark:text-gray-300">
          {$_("settings.preferences.language")}
        </label>
        <div class="flex gap-1 bg-gray-100 dark:bg-gray-700 rounded-lg p-1">
          {#each [
            { value: "en", label: $_("settings.preferences.langEnglish") },
            { value: "fr", label: $_("settings.preferences.langFrench") },
          ] as option (option.value)}
            <button
              onclick={() => settingsStore.setLanguage(option.value)}
              class="px-3 py-1 text-sm rounded-md transition-colors
                {settingsStore.language === option.value
                  ? 'bg-white dark:bg-gray-600 text-gray-900 dark:text-gray-100 shadow-sm'
                  : 'text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200'}"
            >
              {option.label}
            </button>
          {/each}
        </div>
      </div>

      <!-- Auto-update toggle -->
      <div class="flex items-center justify-between">
        <label class="text-sm font-medium text-gray-700 dark:text-gray-300">
          {$_("settings.preferences.autoUpdate")}
        </label>
        <button
          onclick={() => settingsStore.setAutoUpdate(!settingsStore.autoUpdate)}
          class="w-10 h-6 rounded-full transition-colors relative
            {settingsStore.autoUpdate ? 'bg-blue-600' : 'bg-gray-300 dark:bg-gray-600'}"
          aria-label={$_("settings.preferences.autoUpdate")}
        >
          <span
            class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform
              {settingsStore.autoUpdate ? 'translate-x-4' : 'translate-x-0'}"
          ></span>
        </button>
      </div>

      <!-- Parallel operations toggle -->
      <div class="flex items-center justify-between">
        <label class="text-sm font-medium text-gray-700 dark:text-gray-300">
          {$_("settings.preferences.parallelOps")}
        </label>
        <button
          onclick={() => settingsStore.setParallelOperations(!settingsStore.parallelOperations)}
          class="w-10 h-6 rounded-full transition-colors relative
            {settingsStore.parallelOperations ? 'bg-blue-600' : 'bg-gray-300 dark:bg-gray-600'}"
          aria-label={$_("settings.preferences.parallelOps")}
        >
          <span
            class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform
              {settingsStore.parallelOperations ? 'translate-x-4' : 'translate-x-0'}"
          ></span>
        </button>
      </div>
    </div>
  </section>

  <!-- ==================== Profile Section ==================== -->
  <section class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6">
    <h3 class="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4">
      {$_("settings.profile.title")}
    </h3>

    <div class="flex gap-3">
      <button
        onclick={exportConfiguration}
        class="px-4 py-2 text-sm font-medium rounded-lg bg-blue-600 text-white hover:bg-blue-700 transition-colors"
      >
        {$_("settings.profile.export")}
      </button>
      <button
        onclick={importConfiguration}
        class="px-4 py-2 text-sm font-medium rounded-lg border border-gray-300 dark:border-gray-600
          text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors"
      >
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
