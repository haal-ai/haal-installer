<script lang="ts">
  import { _ } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";

  interface LogEntry {
    timestamp: string;
    level: "info" | "warn" | "error";
    message: string;
  }

  let logEntries = $state<LogEntry[]>([]);
  let loading = $state(false);
  let error = $state("");

  let scrollContainer: HTMLDivElement | undefined = $state();

  function parseLogLine(line: string): LogEntry | null {
    const trimmed = line.trim();
    if (!trimmed) return null;

    // Try JSON format first (tracing JSON output)
    try {
      const parsed = JSON.parse(trimmed);
      return {
        timestamp: parsed.timestamp ?? new Date().toISOString(),
        level: normalizeLevel(parsed.level ?? "info"),
        message: parsed.fields?.message ?? parsed.message ?? trimmed,
      };
    } catch {
      // Fall back to plain text parsing
    }

    // Try common log format: 2024-01-01T00:00:00Z INFO message
    const match = trimmed.match(
      /^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}[^\s]*)\s+(INFO|WARN|ERROR|DEBUG|TRACE)\s+(.+)$/i,
    );
    if (match) {
      return {
        timestamp: match[1],
        level: normalizeLevel(match[2]),
        message: match[3],
      };
    }

    // Unparseable line — treat as info
    return {
      timestamp: "",
      level: "info",
      message: trimmed,
    };
  }

  function normalizeLevel(level: string): "info" | "warn" | "error" {
    const l = level.toLowerCase();
    if (l === "error") return "error";
    if (l === "warn" || l === "warning") return "warn";
    return "info";
  }

  async function loadLogs() {
    loading = true;
    error = "";
    try {
      const raw = await invoke<string>("read_logs");
      const lines = raw.split("\n");
      const parsed: LogEntry[] = [];
      for (const line of lines) {
        const entry = parseLogLine(line);
        if (entry) parsed.push(entry);
      }
      logEntries = parsed;
    } catch (err) {
      error = String(err);
    } finally {
      loading = false;
    }
  }

  function clearLogs() {
    logEntries = [];
  }

  async function exportLogs() {
    try {
      // Prompt user for save location via a simple approach:
      // Build the content and invoke the backend export command
      const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
      const filename = `haal-logs-${timestamp}.log`;
      // Use the home directory as a default export location
      await invoke("export_logs", { outputPath: filename });
    } catch (err) {
      error = String(err);
    }
  }

  function levelColor(level: string): string {
    switch (level) {
      case "error":
        return "text-red-500 dark:text-red-400";
      case "warn":
        return "text-amber-500 dark:text-amber-400";
      default:
        return "text-gray-500 dark:text-gray-400";
    }
  }

  function levelBadgeColor(level: string): string {
    switch (level) {
      case "error":
        return "bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300";
      case "warn":
        return "bg-amber-100 dark:bg-amber-900/30 text-amber-700 dark:text-amber-300";
      default:
        return "bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-400";
    }
  }

  // Auto-scroll to bottom when new entries arrive
  $effect(() => {
    if (logEntries.length > 0 && scrollContainer) {
      scrollContainer.scrollTop = scrollContainer.scrollHeight;
    }
  });

  // Load logs on mount
  loadLogs();
</script>

<div class="max-w-4xl mx-auto space-y-4">
  <div class="flex items-center justify-between">
    <h2 class="text-2xl font-bold text-gray-900 dark:text-gray-100">
      {$_("logs.title")}
    </h2>

    <div class="flex gap-2">
      <button
        onclick={loadLogs}
        disabled={loading}
        class="px-3 py-1.5 text-sm font-medium rounded-lg transition-colors
          bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300
          hover:bg-gray-300 dark:hover:bg-gray-600
          disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {$_("logs.refresh")}
      </button>
      <button
        onclick={exportLogs}
        disabled={logEntries.length === 0}
        class="px-3 py-1.5 text-sm font-medium rounded-lg transition-colors
          bg-blue-600 text-white hover:bg-blue-700
          disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {$_("logs.export")}
      </button>
      <button
        onclick={clearLogs}
        disabled={logEntries.length === 0}
        class="px-3 py-1.5 text-sm font-medium rounded-lg transition-colors
          border border-gray-300 dark:border-gray-600
          text-gray-700 dark:text-gray-300
          hover:bg-gray-100 dark:hover:bg-gray-700
          disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {$_("logs.clear")}
      </button>
    </div>
  </div>

  {#if error}
    <p class="text-sm text-red-600 dark:text-red-400">{error}</p>
  {/if}

  {#if loading}
    <p class="text-sm text-gray-500 dark:text-gray-400">{$_("common.loading")}</p>
  {:else if logEntries.length === 0}
    <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-8 text-center">
      <p class="text-gray-500 dark:text-gray-400">{$_("logs.noLogs")}</p>
    </div>
  {:else}
    <div
      bind:this={scrollContainer}
      class="bg-gray-900 dark:bg-gray-950 rounded-lg border border-gray-200 dark:border-gray-700
        overflow-y-auto max-h-[calc(100vh-14rem)] font-mono text-xs leading-5"
    >
      {#each logEntries as entry, i (i)}
        <div class="flex gap-2 px-3 py-0.5 hover:bg-gray-800 dark:hover:bg-gray-900 border-b border-gray-800 dark:border-gray-800/50">
          {#if entry.timestamp}
            <span class="shrink-0 text-gray-500 dark:text-gray-500 select-all">
              {entry.timestamp}
            </span>
          {/if}
          <span class="shrink-0 w-12 text-center">
            <span class="inline-block px-1.5 py-0 rounded text-[10px] font-semibold uppercase {levelBadgeColor(entry.level)}">
              {entry.level}
            </span>
          </span>
          <span class="{levelColor(entry.level)} break-all select-all">
            {entry.message}
          </span>
        </div>
      {/each}
    </div>
  {/if}
</div>
