<script lang="ts">
  import { _ } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { authStore } from "../stores/authStore";

  let authMethod = $state<"pat" | "oauth">("pat");
  let patToken = $state("");
  let enterpriseUrl = $state("");
  let hasStoredCredentials = $state(false);

  // Check for stored credentials on mount
  async function checkStoredCredentials() {
    try {
      const creds = await invoke<{ auth_type: string; token: string; enterprise_url?: string } | null>("get_stored_credentials");
      if (creds) {
        hasStoredCredentials = true;
        authStore.setCredentials({
          authType: creds.auth_type === "OAuth" ? "oauth" : "pat",
          token: creds.token,
          enterpriseUrl: creds.enterprise_url,
        });
      }
    } catch {
      // No stored credentials — that's fine
    }
  }

  checkStoredCredentials();

  async function authenticate() {
    authStore.setStatus("authenticating");
    try {
      const result = await invoke<{ success: boolean; message?: string }>("authenticate_github", {
        authType: authMethod === "pat" ? "PersonalAccessToken" : "OAuth",
        token: authMethod === "pat" ? patToken : null,
        enterpriseUrl: enterpriseUrl || null,
      });
      if (result.success) {
        authStore.setCredentials({
          authType: authMethod,
          token: patToken,
          enterpriseUrl: enterpriseUrl || undefined,
        });
      } else {
        authStore.setError(result.message ?? "Authentication failed");
      }
    } catch (err) {
      authStore.setError(String(err));
    }
  }
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-xl font-semibold text-gray-900 dark:text-gray-100">
      {$_("wizard.connect.title")}
    </h2>
    <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
      {$_("wizard.connect.description")}
    </p>
  </div>

  <!-- Stored credentials indicator -->
  {#if hasStoredCredentials && authStore.isAuthenticated}
    <div class="flex items-center gap-2 p-3 bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-lg">
      <svg class="w-5 h-5 text-green-600 dark:text-green-400" fill="currentColor" viewBox="0 0 20 20">
        <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd" />
      </svg>
      <span class="text-sm text-green-700 dark:text-green-300">{$_("wizard.connect.storedCredentials")}</span>
    </div>
  {/if}

  <!-- Auth status -->
  {#if authStore.isAuthenticated && !hasStoredCredentials}
    <div class="flex items-center gap-2 p-3 bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-lg">
      <svg class="w-5 h-5 text-green-600 dark:text-green-400" fill="currentColor" viewBox="0 0 20 20">
        <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd" />
      </svg>
      <span class="text-sm text-green-700 dark:text-green-300">{$_("wizard.connect.authenticated")}</span>
    </div>
  {/if}

  <!-- Error display -->
  {#if authStore.error}
    <div class="flex items-center gap-2 p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg">
      <svg class="w-5 h-5 text-red-600 dark:text-red-400" fill="currentColor" viewBox="0 0 20 20">
        <path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z" clip-rule="evenodd" />
      </svg>
      <span class="text-sm text-red-700 dark:text-red-300">{authStore.error}</span>
    </div>
  {/if}

  <!-- Auth method tabs -->
  <div class="flex gap-2 border-b border-gray-200 dark:border-gray-700">
    <button
      onclick={() => (authMethod = "pat")}
      class="px-4 py-2 text-sm font-medium border-b-2 transition-colors
        {authMethod === 'pat'
          ? 'border-blue-600 text-blue-600 dark:border-blue-400 dark:text-blue-400'
          : 'border-transparent text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300'}"
    >
      {$_("wizard.connect.authMethodPat")}
    </button>
    <button
      onclick={() => (authMethod = "oauth")}
      class="px-4 py-2 text-sm font-medium border-b-2 transition-colors
        {authMethod === 'oauth'
          ? 'border-blue-600 text-blue-600 dark:border-blue-400 dark:text-blue-400'
          : 'border-transparent text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300'}"
    >
      {$_("wizard.connect.authMethodOauth")}
    </button>
  </div>

  <!-- PAT form -->
  {#if authMethod === "pat"}
    <div class="space-y-4">
      <div>
        <label for="pat-input" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          {$_("wizard.connect.patLabel")}
        </label>
        <input
          id="pat-input"
          type="password"
          bind:value={patToken}
          placeholder={$_("wizard.connect.patPlaceholder")}
          class="w-full px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
            bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
            focus:ring-2 focus:ring-blue-500 focus:border-blue-500 outline-none"
        />
      </div>
      <div>
        <label for="enterprise-url" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          {$_("wizard.connect.enterpriseUrl")}
        </label>
        <input
          id="enterprise-url"
          type="url"
          bind:value={enterpriseUrl}
          placeholder={$_("wizard.connect.enterprisePlaceholder")}
          class="w-full px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
            bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
            focus:ring-2 focus:ring-blue-500 focus:border-blue-500 outline-none"
        />
      </div>
      <button
        onclick={authenticate}
        disabled={authStore.status === "authenticating" || !patToken}
        class="px-4 py-2 text-sm font-medium rounded-lg
          {authStore.status === 'authenticating' || !patToken
            ? 'bg-gray-300 dark:bg-gray-700 text-gray-500 cursor-not-allowed'
            : 'bg-blue-600 text-white hover:bg-blue-700'}"
      >
        {authStore.status === "authenticating" ? $_("wizard.connect.authenticating") : $_("wizard.connect.authenticate")}
      </button>
    </div>
  {:else}
    <!-- OAuth -->
    <div class="space-y-4">
      <div>
        <label for="enterprise-url-oauth" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          {$_("wizard.connect.enterpriseUrl")}
        </label>
        <input
          id="enterprise-url-oauth"
          type="url"
          bind:value={enterpriseUrl}
          placeholder={$_("wizard.connect.enterprisePlaceholder")}
          class="w-full px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg
            bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100
            focus:ring-2 focus:ring-blue-500 focus:border-blue-500 outline-none"
        />
      </div>
      <button
        onclick={authenticate}
        disabled={authStore.status === "authenticating"}
        class="flex items-center gap-2 px-4 py-2 text-sm font-medium rounded-lg
          {authStore.status === 'authenticating'
            ? 'bg-gray-300 dark:bg-gray-700 text-gray-500 cursor-not-allowed'
            : 'bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 hover:bg-gray-800 dark:hover:bg-gray-200'}"
      >
        <svg class="w-5 h-5" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z" />
        </svg>
        {authStore.status === "authenticating" ? $_("wizard.connect.authenticating") : $_("wizard.connect.oauthButton")}
      </button>
    </div>
  {/if}
</div>
