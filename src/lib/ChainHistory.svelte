<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "./i18n.svelte";

  // Histoire complète de la chaîne, depuis la genèse. Les anciens blocs sont
  // regroupés en « gros blocs » (agrégats) pour tout voir d'un coup ; les blocs
  // récents restent individuels (petits).
  let height = $state(0);
  let bucketSize = $state(10);
  let buckets = $state<any[]>([]);
  let recent = $state<any[]>([]);
  let scroller = $state<HTMLDivElement | undefined>();
  let hover = $state<string>("");

  // Le plus gros agrégat sert d'échelle visuelle.
  const maxMint = $derived(
    Math.max(0.0001, ...buckets.map(b => b.minted_qta ?? 0), ...recent.map(r => (r.minted_qta ?? 0) * bucketSize))
  );

  async function load() {
    try {
      const h = await invoke<any>("get_chain_history");
      const wasEnd = scroller ? (scroller.scrollLeft + scroller.clientWidth >= scroller.scrollWidth - 40) : true;
      height = h.height ?? 0;
      bucketSize = h.bucket_size ?? 10;
      buckets = h.buckets ?? [];
      recent = h.recent ?? [];
      // Reste collé à droite (les blocs récents) si on y était.
      if (wasEnd) queueMicrotask(() => { if (scroller) scroller.scrollLeft = scroller.scrollWidth; });
    } catch {}
  }

  $effect(() => {
    load();
    const iv = setInterval(load, 2500);
    return () => clearInterval(iv);
  });

  function bucketHeight(mint: number) {
    return 30 + Math.round((Math.min(mint, maxMint) / maxMint) * 46); // 30..76 px
  }
  function smallHeight(mint: number) {
    return 26 + Math.round((Math.min(mint * bucketSize, maxMint) / maxMint) * 28); // 26..54
  }
</script>

<div class="hist">
  <div class="hist-legend">
    <span><span class="lg-sw lg-agg"></span> {t('cha.aggLegendPre')}{bucketSize}{t('cha.aggLegendPost')}</span>
    <span><span class="lg-sw lg-one"></span> {t('cha.recentOne')}</span>
    <span class="hist-total">{height.toLocaleString('fr-FR')} {t('cha.sinceGenesis')}</span>
  </div>

  <div class="hist-scroll" bind:this={scroller}>
    <div class="hist-cap">{t('cha.genesis')}</div>

    {#each buckets as b (b.from)}
      <div class="hist-col"
        onmouseenter={() => hover = `${t('cha.blocks')} #${b.from}–#${b.to} · ${b.count} ${t('cha.blocks')} · +${(b.minted_qta ?? 0).toFixed(2)} QUANTA · ${b.tx_count} tx`}
        onmouseleave={() => hover = ''}
        role="img" aria-label={t('cha.aggAria')}>
        <div class="agg" style="height:{bucketHeight(b.minted_qta ?? 0)}px;">
          <span class="agg-n">{b.count}</span>
        </div>
        <div class="hist-x">#{b.from}</div>
      </div>
    {/each}

    {#if buckets.length && recent.length}
      <div class="hist-div"></div>
    {/if}

    {#each recent as r (r.index)}
      <div class="hist-col"
        onmouseenter={() => hover = `${t('cha.block')} #${r.index} · +${(r.minted_qta ?? 0).toFixed(3)} QUANTA · ${r.tx_count} tx · ${(r.hash || '').slice(0,8)}`}
        onmouseleave={() => hover = ''}
        role="img" aria-label={t('cha.recentAria')}>
        <div class="one" style="height:{smallHeight(r.minted_qta ?? 0)}px;"></div>
        <div class="hist-x">#{r.index}</div>
      </div>
    {/each}

    {#if height === 0}
      <div class="hist-empty">{t('cha.firstBlock')}</div>
    {:else}
      <div class="hist-cap now">{t('cha.now')}</div>
    {/if}
  </div>

  <div class="hist-hover">{hover || t('cha.hoverHint')}</div>
</div>

<style>
  .hist { display: flex; flex-direction: column; gap: 10px; }
  .hist-legend { display: flex; align-items: center; gap: 18px; flex-wrap: wrap; font-size: 12px; color: var(--color-text-2); }
  .lg-sw { display: inline-block; width: 12px; height: 12px; border-radius: 3px; vertical-align: -2px; margin-right: 5px; }
  .lg-agg { background: var(--color-accent); }
  .lg-one { background: var(--color-bg-4); border: 1px solid var(--color-border-hover); }
  .hist-total { margin-left: auto; font-weight: 600; color: var(--color-text-1); }

  .hist-scroll {
    display: flex; align-items: flex-end; gap: 5px;
    overflow-x: auto; padding: 10px 4px 6px; min-height: 110px;
    background:
      linear-gradient(90deg, #f5f6f8 0%, #ffffff 18%, #ffffff 100%);
    border: 1px solid var(--color-border); border-radius: 12px;
  }
  .hist-scroll::-webkit-scrollbar { height: 6px; }
  .hist-scroll::-webkit-scrollbar-thumb { background: rgba(0,0,0,0.18); border-radius: 3px; }

  .hist-cap {
    flex-shrink: 0; align-self: stretch; display: flex; align-items: center;
    writing-mode: vertical-rl; transform: rotate(180deg);
    font-size: 10px; letter-spacing: .08em; text-transform: uppercase;
    color: var(--color-text-3); padding: 0 2px;
  }
  .hist-cap.now { color: var(--color-accent); font-weight: 700; }

  .hist-col { flex-shrink: 0; display: flex; flex-direction: column; align-items: center; gap: 4px; cursor: default; }
  .agg {
    width: 30px; border-radius: 5px; background: var(--color-accent);
    display: flex; align-items: flex-start; justify-content: center;
    box-shadow: 0 2px 6px rgba(11,165,160,0.25);
  }
  .agg-n { color: #fff; font-size: 10px; font-weight: 700; padding-top: 3px; }
  .one {
    width: 16px; border-radius: 4px;
    background: var(--color-bg-4); border: 1px solid var(--color-border-hover);
  }
  .hist-x { font-size: 9px; color: var(--color-text-3); font-family: var(--font-mono); }

  .hist-div { flex-shrink: 0; width: 1px; align-self: stretch; background: var(--color-border-hover); margin: 6px 4px; }
  .hist-empty { padding: 30px; color: var(--color-text-3); font-size: 13px; }
  .hist-hover { font-size: 12px; color: var(--color-text-2); min-height: 16px; }
</style>
