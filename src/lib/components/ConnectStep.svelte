<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wizardStore } from "../stores/wizardStore.svelte";
  import { authStore } from "../stores/authStore.svelte";

  let ghStatus = $state<{ installed: boolean; authenticated: boolean; username?: string | null } | null>(null);
  let loading = $state(false);
  let error = $state("");
  let initError = $state("");
  let showPat = $state(false);
  let showEnterprise = $state(false);
  let patToken = $state("");
  let enterpriseUrl = $state("");
  let customRepoUrl = $state("");
  let useDefaultRegistry = $state(false);
  let polling = $state(false);
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  // Registry is confirmed when user typed a URL or checked "use default"
  let registryReady = $derived(customRepoUrl.trim().length > 0 || useDefaultRegistry);

  $effect(() => {
    checkGhCli().catch((e) => {
      initError = String(e);
      ghStatus = { installed: false, authenticated: false };
    });
    return () => { if (pollTimer) clearInterval(pollTimer); };
  });

  async function checkGhCli() {
    ghStatus = await invoke<{ installed: boolean; authenticated: boolean }>("check_gh_cli");
  }

  async function connectWithGhCli() {
    if (!registryReady) { error = "Please enter a repository URL or confirm you want the default registry."; return; }
    error = ""; loading = true;
    try {
      const creds = await invoke("authenticate_gh_cli", {
        enterpriseUrl: showEnterprise && enterpriseUrl.trim() ? enterpriseUrl.trim() : null,
      });
      authStore.setCredentials(creds as any);
      if (customRepoUrl.trim()) wizardStore.setRegistryUrl(customRepoUrl.trim());
      wizardStore.setConnected(true);
      wizardStore.nextStep();
    } catch (e: any) { error = String(e); } finally { loading = false; }
  }

  async function launchGhLogin() {
    error = "";
    try {
      await invoke("launch_gh_auth_login");
      polling = true;
      pollTimer = setInterval(async () => {
        await checkGhCli();
        if (ghStatus?.authenticated) {
          clearInterval(pollTimer!); pollTimer = null; polling = false;
          await connectWithGhCli();
        }
      }, 3000);
    } catch (e: any) { error = String(e); }
  }

  async function connectWithPat() {
    if (!patToken.trim()) { error = "Please enter a token."; return; }
    if (!registryReady) { error = "Please enter a repository URL or confirm you want the default registry."; return; }
    error = ""; loading = true;
    try {
      const creds = await invoke("authenticate_github", {
        authType: "pat", token: patToken.trim(),
        enterpriseUrl: showEnterprise && enterpriseUrl.trim() ? enterpriseUrl.trim() : null,
      });
      authStore.setCredentials(creds as any);
      if (customRepoUrl.trim()) wizardStore.setRegistryUrl(customRepoUrl.trim());
      wizardStore.setConnected(true);
      wizardStore.nextStep();
    } catch (e: any) { error = String(e); } finally { loading = false; }
  }
</script>

<div class="max-w-lg mx-auto flex flex-col gap-5">
  <div>
    <h2 class="text-xl font-semibold text-gray-900 dark:text-gray-100">Connect to GitHub</h2>
    <p class="text-sm text-gray-500 dark:text-gray-400 mt-1">HAAL needs access to GitHub to fetch components.</p>
  </div>

  {#if initError}
    <p class="text-xs text-red-400">Init error: {initError}</p>
  {/if}

  <!-- Registry selection -->
  <div class="flex flex-col gap-2">
    <label for="repo-url" class="text-sm font-medium text-gray-700 dark:text-gray-300">
      Component repository URL
    </label>
    <input
      id="repo-url"
      type="url"
      placeholder="https://github.com/your-org/your-components"
      bind:value={customRepoUrl}
      disabled={useDefaultRegistry}
      class="px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
    />
    <label class="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-400 cursor-pointer select-none">
      <input
        type="checkbox"
        bind:checked={useDefaultRegistry}
        onchange={() => { if (useDefaultRegistry) customRepoUrl = ""; }}
        class="rounded border-gray-300 dark:border-gray-600"
      />
      Use the default HAAL registry
    </label>
  </div>

  <!-- GitHub CLI card -->
  <div class="border border-gray-200 dark:border-gray-700 rounded-xl p-4 flex flex-col gap-3">
    <div class="flex items-center gap-2">
      <span class="text-xs font-semibold bg-green-100 dark:bg-green-900 text-green-700 dark:text-green-300 px-2 py-0.5 rounded-full">Recommended</span>
      <span class="font-medium text-gray-900 dark:text-gray-100">GitHub CLI</span>
    </div>

    {#if ghStatus === null}
      <p class="text-sm text-gray-400">Checking GitHub CLI...</p>
    {:else if !ghStatus.installed}
      <p class="text-sm text-gray-600 dark:text-gray-400">
        GitHub CLI is not installed.
        <a href="https://cli.github.com" target="_blank" class="text-blue-600 dark:text-blue-400 underline">Install it</a>,
        then come back.
      </p>
    {:else if ghStatus.authenticated}
      <p class="text-sm text-green-600 dark:text-green-400">Signed in as <strong>{ghStatus.username ?? "unknown"}</strong> via GitHub CLI</p>
      <div class="flex items-center gap-3 flex-wrap">
        <button
          onclick={connectWithGhCli}
          disabled={loading || !registryReady}
          class="px-4 py-2 text-sm font-medium rounded-lg bg-green-600 hover:bg-green-700 text-white disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {loading ? "Connecting..." : "Continue with this account"}
        </button>
        <button
          onclick={launchGhLogin}
          disabled={loading}
          class="px-4 py-2 text-sm font-medium rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 disabled:opacity-50"
        >
          Sign in with a different account
        </button>
      </div>
    {:else}
      <p class="text-sm text-gray-600 dark:text-gray-400">Sign in with your GitHub account in one click.</p>
      {#if polling}
        <p class="text-sm text-gray-400">Waiting for sign-in to complete...</p>
      {:else}
        <button
          onclick={launchGhLogin}
          disabled={loading}
          class="self-start px-4 py-2 text-sm font-medium rounded-lg bg-green-600 hover:bg-green-700 text-white disabled:opacity-50 disabled:cursor-not-allowed"
        >
          Sign in with GitHub CLI
        </button>
      {/if}
    {/if}
  </div>

  <!-- GitHub Enterprise toggle -->
  <button
    onclick={() => (showEnterprise = !showEnterprise)}
    class="text-sm text-blue-600 dark:text-blue-400 text-left hover:underline"
  >
    {showEnterprise ? "[-]" : "[+]"} GitHub Enterprise
  </button>
  {#if showEnterprise}
    <div class="flex flex-col gap-1 pl-4">
      <label for="enterprise-url" class="text-sm font-medium text-gray-700 dark:text-gray-300">Enterprise URL</label>
      <input
        id="enterprise-url"
        type="url"
        placeholder="https://github.your-company.com"
        bind:value={enterpriseUrl}
        class="px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500"
      />
    </div>
  {/if}

  <!-- PAT fallback toggle -->
  <button
    onclick={() => (showPat = !showPat)}
    class="text-sm text-blue-600 dark:text-blue-400 text-left hover:underline"
  >
    {showPat ? "[-]" : "[+]"} Use a Personal Access Token instead
  </button>
  {#if showPat}
    <div class="border border-gray-200 dark:border-gray-700 rounded-xl p-4 flex flex-col gap-3 bg-gray-50 dark:bg-gray-800/50">
      <div class="flex flex-col gap-1">
        <label for="pat" class="text-sm font-medium text-gray-700 dark:text-gray-300">Personal Access Token</label>
        <input
          id="pat"
          type="password"
          placeholder="ghp_..."
          bind:value={patToken}
          class="px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
        <p class="text-xs text-gray-400">
          Needs <code class="bg-gray-100 dark:bg-gray-700 px-1 rounded">repo</code> scope.
          <a href="https://github.com/settings/tokens/new?scopes=repo" target="_blank" class="text-blue-600 dark:text-blue-400 underline">Create one</a>
        </p>
      </div>
      <button
        onclick={connectWithPat}
        disabled={loading || !patToken.trim() || !registryReady}
        class="self-start px-4 py-2 text-sm font-medium rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {loading ? "Connecting..." : "Connect with Token"}
      </button>
    </div>
  {/if}

  {#if error}
    <p class="text-sm text-red-600 dark:text-red-400">{error}</p>
  {/if}
</div>