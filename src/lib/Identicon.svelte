<script lang="ts">
  /**
   * Identicon.svelte — gemme Aurora unique par clé publique.
   * Chaque wallet a son propre dégradé (teintes "joyau" froides : teal→bleu→
   * violet→magenta), dérivé de la clé : distinctif comme une carte de membre
   * (façon Arc), mais toujours dans la famille de marque QUANTA. Un motif
   * géométrique subtil ajoute une texture unique. Pas d'initiales : c'est le
   * @pseudo à côté qui nomme ; l'identicon, lui, donne une couleur reconnaissable.
   */
  let { pubkey, size = 36 }: { pubkey: string; size?: number } = $props();

  const cells = 5;
  const mid = Math.ceil(cells / 2);

  // Dégradé Aurora unique : 3 teintes analogues, ancrées dans l'arc froid
  // [160°, 320°] (teal → cyan → bleu → indigo → violet → magenta) → toujours
  // harmonieux et "QUANTA", jamais criard.
  function hashToGradient(pk: string): string {
    if (!pk || pk.length < 6) return "linear-gradient(135deg,#14C8B8,#3D6FE0,#7C3AED)";
    const base = 160 + (parseInt(pk.slice(0, 6), 16) % 160);
    const a = base % 360;
    const b = (base + 32) % 360;
    const c = (base + 66) % 360;
    return `linear-gradient(135deg, hsl(${a},70%,56%) 0%, hsl(${b},66%,50%) 55%, hsl(${c},64%,56%) 100%)`;
  }

  function hashToGrid(pk: string): boolean[][] {
    const grid: boolean[][] = [];
    if (!pk || pk.length < 16) {
      for (let r = 0; r < cells; r++) grid.push(Array(cells).fill(false));
      return grid;
    }
    for (let r = 0; r < cells; r++) {
      const row: boolean[] = [];
      for (let c = 0; c < mid; c++) {
        const idx = (r * mid + c) * 2;
        const byte = parseInt(pk.slice(idx % pk.length, (idx % pk.length) + 2), 16);
        row.push(byte > 127);
      }
      const full = [...row];
      for (let c = mid - 2; c >= 0; c--) full.push(row[c]);
      grid.push(full);
    }
    return grid;
  }

  let gradient = $derived(hashToGradient(pubkey));
  let grid = $derived(hashToGrid(pubkey));
  let cellSize = $derived(size / cells);
</script>

<div
  class="identicon"
  style="width:{size}px;height:{size}px;background:{gradient};border-radius:{size > 32 ? 14 : 8}px"
  title={pubkey ? pubkey.slice(0, 12) + '…' : 'Inconnu'}
>
  <svg width={size} height={size} viewBox="0 0 {size} {size}" aria-hidden="true">
    {#each grid as row, r}
      {#each row as on, c}
        {#if on}
          <rect x={c * cellSize} y={r * cellSize} width={cellSize} height={cellSize} fill="#ffffff" opacity="0.15" />
        {/if}
      {/each}
    {/each}
  </svg>
</div>

<style>
  .identicon {
    position: relative; overflow: hidden; flex-shrink: 0;
    display: inline-flex; align-items: center; justify-content: center;
    border: 1px solid rgba(255, 255, 255, 0.2);
    box-shadow: 0 2px 8px rgba(20, 30, 40, 0.12);
    transition: transform 0.15s ease, box-shadow 0.15s ease;
  }
  .identicon:hover {
    transform: scale(1.05);
    box-shadow: 0 4px 16px rgba(11, 165, 160, 0.3);
  }
  .identicon svg { position: absolute; inset: 0; }
</style>
