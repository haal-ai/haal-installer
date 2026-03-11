<script lang="ts">
  import { _ } from "svelte-i18n";
  import { settingsStore } from "../stores/settingsStore.svelte";

  let networkOnline = $state(true);
  let langDropdownOpen = $state(false);

  function toggleTheme() {
    const next = settingsStore.resolvedTheme === "dark" ? "light" : "dark";
    settingsStore.setTheme(next);
  }

  function selectLanguage(lang: string) {
    settingsStore.setLanguage(lang);
    langDropdownOpen = false;
  }

  // Check network status periodically
  if (typeof window !== "undefined") {
    networkOnline = navigator.onLine;
    window.addEventListener("online", () => (networkOnline = true));
    window.addEventListener("offline", () => (networkOnline = false));
  }
</script>

<header
  class="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800"
>
  <div class="flex items-center gap-3">
    <h1 class="text-lg font-semibold text-gray-900 dark:text-gray-100">
      {$_("app.title")}
    </h1>
  </div>

  <div class="flex items-center gap-3">
    <!-- Network status -->
    <div class="flex items-center gap-1.5 text-sm">
      <span
        class="inline-block w-2 h-2 rounded-full {networkOnline
          ? 'bg-green-500'
          : 'bg-red-500'}"
      ></span>
      <span class="text-gray-600 dark:text-gray-400">
        {networkOnline
          ? $_("header.networkOnline")
          : $_("header.networkOffline")}
      </span>
    </div>

    <!-- Theme toggle -->
    <button
      onclick={toggleTheme}
      class="p-2 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-600 dark:text-gray-400"
      aria-label={$_("header.themeToggle")}
    >
      {#if settingsStore.resolvedTheme === "dark"}
        <svg
          class="w-5 h-5"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z"
          />
        </svg>
      {:else}
        <svg
          class="w-5 h-5"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z"
          />
        </svg>
      {/if}
    </button>

    <!-- Language selector -->
    <div class="relative">
      <button
        onclick={() => (langDropdownOpen = !langDropdownOpen)}
        class="px-3 py-1.5 text-sm rounded-lg border border-gray-300 dark:border-gray-600 hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300"
      >
        {settingsStore.language === "fr" ? "FR" : "EN"}
      </button>
      {#if langDropdownOpen}
        <div
          class="absolute right-0 mt-1 w-32 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg z-10"
        >
          <button
            onclick={() => selectLanguage("en")}
            class="block w-full text-left px-3 py-2 text-sm hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300"
          >
            English
          </button>
          <button
            onclick={() => selectLanguage("fr")}
            class="block w-full text-left px-3 py-2 text-sm hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300"
          >
            Français
          </button>
        </div>
      {/if}
    </div>
  </div>
</header>
