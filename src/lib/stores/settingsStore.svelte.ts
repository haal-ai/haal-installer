import { invoke } from "@tauri-apps/api/core";
import { locale } from "svelte-i18n";

export type Theme = "light" | "dark" | "system";

export const SUPPORTED_TOOLS = ["Kiro", "Copilot", "Cursor", "Claude Code", "Windsurf"] as const;
export type SupportedTool = typeof SUPPORTED_TOOLS[number];

export const ARTIFACT_TYPES = ["skills", "powers", "hooks", "commands", "rules", "agents", "mcpServers", "packages", "systems", "olafData"] as const;
export type ArtifactType = typeof ARTIFACT_TYPES[number];

const ARTIFACT_LABELS: Record<ArtifactType, string> = {
  skills:     "Skills",
  powers:     "Powers",
  hooks:      "Hooks",
  commands:   "Commands",
  rules:      "Rules",
  agents:     "Agents",
  mcpServers: "MCP Servers",
  packages:   "Packages (Claude plugins)",
  systems:    "Agentic Systems",
  olafData:   ".olaf/data (knowledge base)",
};
export { ARTIFACT_LABELS };

function getSystemTheme(): "light" | "dark" {
  if (typeof window === "undefined") return "light";
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

function applyTheme(theme: Theme) {
  const resolved = theme === "system" ? getSystemTheme() : theme;
  if (resolved === "dark") {
    document.documentElement.classList.add("dark");
  } else {
    document.documentElement.classList.remove("dark");
  }
}

function createSettingsStore() {
  let theme = $state<Theme>("system");
  let language = $state<string>("en");
  let autoUpdate = $state(true);
  let parallelOperations = $state(true);
  // Which tools are enabled for install (all on by default)
  let enabledTools = $state<Set<SupportedTool>>(new Set(SUPPORTED_TOOLS));
  // Which artifact types are enabled for install (all on by default)
  let enabledArtifacts = $state<Set<ArtifactType>>(new Set(ARTIFACT_TYPES));
  // Developer: use test branches for registries instead of main
  let useTestBranches = $state(false);

  // Apply initial theme
  applyTheme(theme);

  // Listen for OS theme changes when using "system"
  if (typeof window !== "undefined") {
    window
      .matchMedia("(prefers-color-scheme: dark)")
      .addEventListener("change", () => {
        if (theme === "system") {
          applyTheme("system");
        }
      });
  }

  return {
    get theme() {
      return theme;
    },
    get language() {
      return language;
    },
    get autoUpdate() {
      return autoUpdate;
    },
    get parallelOperations() {
      return parallelOperations;
    },
    get enabledTools() {
      return enabledTools;
    },
    isToolEnabled(tool: SupportedTool) {
      return enabledTools.has(tool);
    },
    get enabledArtifacts() {
      return enabledArtifacts;
    },
    isArtifactEnabled(type: ArtifactType) {
      return enabledArtifacts.has(type);
    },
    toggleArtifact(type: ArtifactType) {
      const next = new Set(enabledArtifacts);
      if (next.has(type)) next.delete(type); else next.add(type);
      enabledArtifacts = next;
      this.persistToBackend();
    },
    get useTestBranches() {
      return useTestBranches;
    },
    setUseTestBranches(value: boolean) {
      useTestBranches = value;
      this.persistToBackend();
    },
    /** Returns the effective branch name for registry fetches. */
    get registryBranch(): string {
      return useTestBranches ? "test" : "main";
    },
    get resolvedTheme(): "light" | "dark" {
      return theme === "system" ? getSystemTheme() : theme;
    },

    setTheme(value: Theme) {
      theme = value;
      applyTheme(value);
      this.persistToBackend();
    },

    setLanguage(value: string) {
      language = value;
      locale.set(value);
      this.persistToBackend();
    },

    setAutoUpdate(value: boolean) {
      autoUpdate = value;
      this.persistToBackend();
    },

    setParallelOperations(value: boolean) {
      parallelOperations = value;
      this.persistToBackend();
    },

    toggleTool(tool: SupportedTool) {
      const next = new Set(enabledTools);
      if (next.has(tool)) next.delete(tool); else next.add(tool);
      enabledTools = next;
      this.persistToBackend();
    },

    async persistToBackend() {
      try {
        // Store the explicit list only when it's a subset; empty = all enabled (default)
        const toolsToStore = enabledTools.size === SUPPORTED_TOOLS.length
          ? [] : Array.from(enabledTools);
        const artifactsToStore = enabledArtifacts.size === ARTIFACT_TYPES.length
          ? [] : Array.from(enabledArtifacts);
        await invoke("save_config", {
          preferences: {
            theme,
            language,
            auto_update: autoUpdate,
            parallel_operations: parallelOperations,
            enabled_tools: toolsToStore,
            enabled_artifacts: artifactsToStore,
            use_test_branches: useTestBranches,
          },
        });
      } catch {
        // Config save may fail if backend isn't ready — that's okay
      }
    },

    async loadFromBackend() {
      try {
        const prefs = await invoke<{
          theme?: Theme;
          language?: string;
          auto_update?: boolean;
          parallel_operations?: boolean;
          enabled_tools?: string[];
          enabled_artifacts?: string[];
          use_test_branches?: boolean;
        }>("get_config");
        if (prefs) {
          const p = prefs;
          if (p.theme) { theme = p.theme; applyTheme(theme); }
          if (p.language) { language = p.language; locale.set(p.language); }
          if (p.auto_update !== undefined) autoUpdate = p.auto_update;
          if (p.parallel_operations !== undefined) parallelOperations = p.parallel_operations;
          if (p.enabled_tools && p.enabled_tools.length > 0) {
            enabledTools = new Set(
              p.enabled_tools.filter((t): t is SupportedTool =>
                (SUPPORTED_TOOLS as readonly string[]).includes(t)
              )
            );
          }
          if (p.enabled_artifacts && p.enabled_artifacts.length > 0) {
            enabledArtifacts = new Set(
              p.enabled_artifacts.filter((a): a is ArtifactType =>
                (ARTIFACT_TYPES as readonly string[]).includes(a)
              )
            );
          }
          if (p.use_test_branches !== undefined) useTestBranches = p.use_test_branches;
        }
      } catch {
        // Backend not available yet — use defaults
      }
    },
  };
}

export const settingsStore = createSettingsStore();
