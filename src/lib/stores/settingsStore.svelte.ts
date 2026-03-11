import { invoke } from "@tauri-apps/api/core";
import { locale } from "svelte-i18n";

export type Theme = "light" | "dark" | "system";

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

    async persistToBackend() {
      try {
        await invoke("save_config", {
          config: {
            preferences: {
              theme,
              language,
              auto_update: autoUpdate,
              parallel_operations: parallelOperations,
            },
          },
        });
      } catch {
        // Config save may fail if backend isn't ready — that's okay
      }
    },

    async loadFromBackend() {
      try {
        const config = await invoke<{
          preferences?: {
            theme?: Theme;
            language?: string;
            auto_update?: boolean;
            parallel_operations?: boolean;
          };
        }>("get_config");
        if (config?.preferences) {
          const p = config.preferences;
          if (p.theme) {
            theme = p.theme;
            applyTheme(theme);
          }
          if (p.language) {
            language = p.language;
            locale.set(p.language);
          }
          if (p.auto_update !== undefined) autoUpdate = p.auto_update;
          if (p.parallel_operations !== undefined)
            parallelOperations = p.parallel_operations;
        }
      } catch {
        // Backend not available yet — use defaults
      }
    },
  };
}

export const settingsStore = createSettingsStore();
