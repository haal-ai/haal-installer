<script lang="ts">
  import { _ } from "svelte-i18n";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";

  type UpdatePhase = "idle" | "available" | "downloading" | "installing" | "restarting" | "error";

  let phase: UpdatePhase = $state("idle");
  let version = $state("");
  let errorMessage = $state("");
  let dismissed = $state(false);
  let downloadProgress = $state(0);

  async function checkForUpdate() {
    try {
      const update = await check();
      if (update) {
        version = update.version;
        phase = "available";
      }
    } catch (err) {
      // Silently ignore update check failures — not critical
      console.warn("Update check failed:", err);
    }
  }

  async function installUpdate() {
    try {
      const update = await check();
      if (!update) return;

      phase = "downloading";
      let totalBytes = 0;
      let downloadedBytes = 0;

      await update.downloadAndInstall((event) => {
        if (event.event === "Started" && event.data.contentLength) {
          totalBytes = event.data.contentLength;
        } else if (event.event === "Progress") {
          downloadedBytes += event.data.chunkLength;
          if (totalBytes > 0) {
            downloadProgress = Math.round((downloadedBytes / totalBytes) * 100);
          }
        } else if (event.event === "Finished") {
          phase = "installing";
        }
      });

      phase = "restarting";
      await relaunch();
    } catch (err) {
      phase = "error";
      errorMessage = err instanceof Error ? err.message : String(err);
    }
  }

  function dismiss() {
    dismissed = true;
  }

  // Check for updates on mount
  checkForUpdate();
</script>

{#if !dismissed && phase !== "idle"}
  <div
    class="mx-4 mt-2 rounded-lg border px-4 py-3 text-sm shadow-sm
      {phase === 'error'
        ? 'border-red-300 bg-red-50 text-red-800 dark:border-red-700 dark:bg-red-900/30 dark:text-red-300'
        : 'border-blue-300 bg-blue-50 text-blue-800 dark:border-blue-700 dark:bg-blue-900/30 dark:text-blue-300'}"
    role="alert"
  >
    {#if phase === "available"}
      <div class="flex items-center justify-between gap-3">
        <span>{$_("update.available", { values: { version } })}</span>
        <div class="flex items-center gap-2">
          <button
            onclick={installUpdate}
            class="rounded bg-blue-600 px-3 py-1 text-xs font-medium text-white hover:bg-blue-700 dark:bg-blue-500 dark:hover:bg-blue-600"
          >
            {$_("update.updateNow")}
          </button>
          <button
            onclick={dismiss}
            class="rounded px-3 py-1 text-xs font-medium text-blue-600 hover:bg-blue-100 dark:text-blue-400 dark:hover:bg-blue-800/40"
          >
            {$_("update.later")}
          </button>
        </div>
      </div>
    {:else if phase === "downloading"}
      <div class="flex items-center gap-3">
        <svg class="h-4 w-4 animate-spin" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
        </svg>
        <span>{$_("update.downloading")}</span>
        {#if downloadProgress > 0}
          <span class="tabular-nums">{downloadProgress}%</span>
        {/if}
      </div>
    {:else if phase === "installing"}
      <div class="flex items-center gap-3">
        <svg class="h-4 w-4 animate-spin" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
        </svg>
        <span>{$_("update.installing")}</span>
      </div>
    {:else if phase === "restarting"}
      <div class="flex items-center gap-3">
        <svg class="h-4 w-4 animate-spin" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
        </svg>
        <span>{$_("update.restarting")}</span>
      </div>
    {:else if phase === "error"}
      <div class="flex items-center justify-between gap-3">
        <span>{$_("update.error", { values: { message: errorMessage } })}</span>
        <button
          onclick={dismiss}
          class="rounded px-3 py-1 text-xs font-medium text-red-600 hover:bg-red-100 dark:text-red-400 dark:hover:bg-red-800/40"
        >
          {$_("common.close")}
        </button>
      </div>
    {/if}
  </div>
{/if}
