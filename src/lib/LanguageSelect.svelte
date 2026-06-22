<script lang="ts">
  // Sélecteur de langue — 6 langues, noms natifs. variant "glass" pour
  // poser sur un fond Aurora (texte clair), "solid" sur le chrome clair.
  import { LOCALES, type Locale } from "./prefs";
  import { setLocale, locale } from "./i18n.svelte";

  let { variant = "solid" } = $props<{ variant?: "solid" | "glass" }>();

  const NAMES: Record<Locale, string> = {
    en: "English", fr: "Français", es: "Español", ru: "Русский", zh: "中文", ja: "日本語",
  };
</script>

<div class="lang-select {variant}">
  {#each LOCALES as l}
    <button type="button" class:active={locale() === l} onclick={() => setLocale(l)}>{NAMES[l]}</button>
  {/each}
</div>

<style>
  .lang-select { display: flex; flex-wrap: wrap; gap: 6px; }
  .lang-select button {
    font: inherit; font-size: 12px; font-weight: 600;
    padding: 6px 12px; border-radius: 999px; cursor: pointer;
    transition: all 0.15s ease;
  }
  /* solid (sur fond clair) */
  .solid button {
    background: var(--color-bg-1); border: 1px solid var(--color-border);
    color: var(--color-text-1);
  }
  .solid button:hover:not(.active) { border-color: var(--color-border-hover); }
  .solid button.active {
    background: var(--color-accent); border-color: var(--color-accent); color: #fff;
  }
  /* glass (sur fond Aurora) */
  .glass button {
    background: rgba(255,255,255,0.14); border: 1px solid rgba(255,255,255,0.34);
    color: rgba(255,255,255,0.88); backdrop-filter: blur(6px);
  }
  .glass button:hover:not(.active) { color: #fff; }
  .glass button.active { background: #fff; border-color: #fff; color: var(--color-accent-hover); }
</style>
