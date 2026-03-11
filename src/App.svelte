<script lang="ts">
  import { _ } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import Header from "./lib/components/Header.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import WizardFlow from "./lib/components/WizardFlow.svelte";
  import SettingsPanel from "./lib/components/SettingsPanel.svelte";
  import SystemsStep from "./lib/components/SystemsStep.svelte";
  import LogViewer from "./lib/components/LogViewer.svelte";
  import SelfInstallDialog from "./lib/components/SelfInstallDialog.svelte";
  import UpdateNotification from "./lib/components/UpdateNotification.svelte";
  import { settingsStore } from "./lib/stores/settingsStore.svelte";

  interface SelfInstallStatus {
    isInstalled: boolean;
    homeExists: boolean;
    needsUpdate: boolean;
  }

  let activeView = $state("wizard");
  let selfInstallNeeded = $state(false);
  let selfInstallNeedsUpdate = $state(false);
  let selfInstallChecked = $state(false);

  function handleNavigate(view: string) {
    activeView = view;
  }

  // Check self-installation status on mount
  async function checkSelfInstall() {
    try {
      const status = await invoke<SelfInstallStatus>("check_self_install");

      if (!status.isInstalled) {
        selfInstallNeeded = true;
        selfInstallNeedsUpdate = status.needsUpdate;
      }
    } catch (err) {
      console.error("Self-install check failed:", err);
    } finally {
      selfInstallChecked = true;
    }
  }

  // Attempt to load persisted settings from backend on mount
  settingsStore.loadFromBackend();
  checkSelfInstall();
</script>

{#if selfInstallNeeded}
  <SelfInstallDialog needsUpdate={selfInstallNeedsUpdate} />
{/if}

<div class="h-screen flex flex-col bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100">
  <Header />
  {#if selfInstallChecked && !selfInstallNeeded}
    <UpdateNotification />
  {/if}

  <div class="flex flex-1 overflow-hidden">
    <Sidebar {activeView} onNavigate={handleNavigate} />

    <main class="flex-1 overflow-y-auto p-6">
      {#if !selfInstallChecked}
        <div class="flex items-center justify-center h-full">
          <div class="flex items-center gap-3 text-gray-500 dark:text-gray-400">
            <svg class="w-5 h-5 animate-spin" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
            </svg>
            <span class="text-sm">{$_("common.loading")}</span>
          </div>
        </div>
      {:else if activeView === "wizard"}
        <WizardFlow />
      {:else if activeView === "systems"}
        <SystemsStep />
      {:else if activeView === "settings"}
        <SettingsPanel />
      {:else if activeView === "logs"}
        <LogViewer />
      {/if}
    </main>
  </div>
</div>
