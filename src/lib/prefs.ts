// User preferences — stored in localStorage, applied app-wide.
// No secrets here. Identity & QUANTA balance live elsewhere (encrypted vault, ledger).

export type Theme = "light" | "dark" | "auto";

export interface Prefs {
  theme: Theme;
  lockMinutes: number;       // 0 = never
  confirmThreshold: number;  // ATN amount above which transfers prompt confirmation
}

const LEGACY_KEY = "titan.prefs.v1";
const KEY = "quanta.prefs.v1";

const DEFAULT_PREFS: Prefs = {
  theme: "dark",
  lockMinutes: 15,
  confirmThreshold: 100,
};

export function getPrefs(): Prefs {
  try {
    // Migrate from legacy key if present
    let raw = localStorage.getItem(KEY);
    if (!raw) {
      raw = localStorage.getItem(LEGACY_KEY);
      if (raw) { localStorage.setItem(KEY, raw); localStorage.removeItem(LEGACY_KEY); }
    }
    if (!raw) return { ...DEFAULT_PREFS };
    const p = JSON.parse(raw) as Partial<Prefs>;
    return {
      theme: (["light", "dark", "auto"] as const).includes(p.theme as Theme) ? (p.theme as Theme) : DEFAULT_PREFS.theme,
      lockMinutes: typeof p.lockMinutes === "number" ? p.lockMinutes : DEFAULT_PREFS.lockMinutes,
      confirmThreshold: typeof p.confirmThreshold === "number" ? p.confirmThreshold : DEFAULT_PREFS.confirmThreshold,
    };
  } catch { return { ...DEFAULT_PREFS }; }
}

export function setPrefs(p: Prefs): void {
  try { localStorage.setItem(KEY, JSON.stringify(p)); } catch {}
}

export function applyTheme(theme: Theme): void {
  const root = document.documentElement;
  if (theme === "auto") {
    const prefersDark = window.matchMedia?.("(prefers-color-scheme: dark)").matches;
    root.dataset.theme = prefersDark ? "dark" : "light";
  } else {
    root.dataset.theme = theme;
  }
}
