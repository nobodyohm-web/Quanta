// User preferences — stored in localStorage, applied app-wide.
// No secrets here. Identity & QUANTA balance live elsewhere (encrypted vault, ledger).

export type Theme = "light" | "dark" | "auto";
export type Locale = "en" | "fr" | "es" | "ru" | "zh" | "ja";

export const LOCALES: Locale[] = ["en", "fr", "es", "ru", "zh", "ja"];

export interface Prefs {
  theme: Theme;
  lockMinutes: number;       // 0 = never
  confirmThreshold: number;  // ATN amount above which transfers prompt confirmation
  locale: Locale;            // UI language
  sound: boolean;            // subtle forge/receive chimes (WebAudio, generated)
  privacy: boolean;          // blur balances until hovered (over-the-shoulder mode)
}

const LEGACY_KEY = "titan.prefs.v1";
const KEY = "quanta.prefs.v1";

const DEFAULT_PREFS: Prefs = {
  theme: "light",
  lockMinutes: 15,
  confirmThreshold: 100,
  locale: "en",
  sound: true,
  privacy: false,
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
      locale: LOCALES.includes(p.locale as Locale) ? (p.locale as Locale) : DEFAULT_PREFS.locale,
      sound: typeof p.sound === "boolean" ? p.sound : DEFAULT_PREFS.sound,
      privacy: typeof p.privacy === "boolean" ? p.privacy : DEFAULT_PREFS.privacy,
    };
  } catch { return { ...DEFAULT_PREFS }; }
}

export function setPrefs(p: Prefs): void {
  try { localStorage.setItem(KEY, JSON.stringify(p)); } catch {}
}

export function applyTheme(theme: Theme): void {
  const root = document.documentElement;
  let resolved: "light" | "dark";
  if (theme === "auto") {
    const prefersDark = window.matchMedia?.("(prefers-color-scheme: dark)").matches;
    resolved = prefersDark ? "dark" : "light";
  } else {
    resolved = theme;
  }
  root.dataset.theme = resolved;
  // Keep the inline root background (set by the anti-flash boot script) in sync
  // with runtime theme switches, so there is never a stale white/dark backdrop.
  root.style.backgroundColor = resolved === "dark" ? "#0f1115" : "#ffffff";
}
