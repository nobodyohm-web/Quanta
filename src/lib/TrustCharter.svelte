<script lang="ts">
  import { t, type TKey } from "./i18n.svelte";
  // Charte d'intégrité — levier de confiance n°1 (Mayer-Davis-Schoorman : l'intégrité
  // prime au premier contact). Énonce ce qui est GARANTI dans le code et ce que
  // QUANTA ne fera JAMAIS. Tout est vérifiable → la transparence EST le signal.
  const guaranteed: TKey[] = [
    "ch.g1",
    "ch.g2",
    "ch.g3",
    "ch.g4",
    "ch.g5",
    "ch.g6",
    "ch.g7",
  ];
  const never: TKey[] = [
    "ch.n1",
    "ch.n2",
    "ch.n3",
    "ch.n4",
    "ch.n5",
  ];
</script>

<div class="charter">
  <div class="ch-head">
    <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="var(--color-accent)" stroke-width="1.5"><path d="M8 1.5l5.5 2.2V8c0 3.3-2.3 5.6-5.5 6.5C4.8 13.6 2.5 11.3 2.5 8V3.7L8 1.5z"/><path d="M5.8 8l1.6 1.6L10.4 6.6"/></svg>
    <div>
      <div class="ch-title">{t('ch.title')}</div>
      <div class="ch-sub">{t('ch.sub')}</div>
    </div>
  </div>

  <div class="ch-cols">
    <div class="ch-col">
      <div class="ch-k ch-k-ok">✓ {t('ch.okHead')}</div>
      {#each guaranteed as g}
        <div class="ch-item"><span class="ch-mark ok">✓</span><span>{t(g)}</span></div>
      {/each}
    </div>
    <div class="ch-col">
      <div class="ch-k ch-k-no">✕ {t('ch.noHead')}</div>
      {#each never as n}
        <div class="ch-item"><span class="ch-mark no">✕</span><span>{t(n)}</span></div>
      {/each}
    </div>
  </div>
</div>

<style>
  .charter { background: var(--surface); border: 1px solid var(--color-border); border-radius: 16px; padding: 22px 24px; box-shadow: var(--shadow-sm); }
  .ch-head { display: flex; gap: 10px; align-items: flex-start; margin-bottom: 18px; }
  .ch-title { font-size: 15px; font-weight: 700; color: var(--color-text-0); }
  .ch-sub { font-size: 12px; color: var(--color-text-2); margin-top: 2px; max-width: 540px; line-height: 1.5; }
  .ch-cols { display: grid; grid-template-columns: 1fr 1fr; gap: 28px; }
  .ch-k { font-size: 11px; font-weight: 700; letter-spacing: .05em; text-transform: uppercase; margin-bottom: 10px; }
  .ch-k-ok { color: var(--color-green); }
  .ch-k-no { color: var(--color-red); }
  .ch-item { display: flex; gap: 9px; align-items: flex-start; padding: 5px 0; font-size: 13px; color: var(--color-text-1); line-height: 1.45; }
  .ch-mark { flex-shrink: 0; font-weight: 800; width: 16px; text-align: center; }
  .ch-mark.ok { color: var(--color-green); }
  .ch-mark.no { color: var(--color-red); }
  @media (max-width: 720px) { .ch-cols { grid-template-columns: 1fr; gap: 16px; } }
</style>
