<script lang="ts">
  /**
   * Identicon.svelte — Deterministic geometric avatar from public key hash.
   * Each user gets a unique, visually memorable pattern derived from their key.
   */
  let { pubkey, size = 36 }: { pubkey: string; size?: number } = $props();

  const cells = 5;
  const mid = Math.ceil(cells / 2);

  // Derive a stable color and pattern from the public key
  function hashToColors(pk: string): { bg: string; fg: string } {
    if (!pk || pk.length < 6) return { bg: '#7c3aed', fg: '#f0eef5' };
    const h = parseInt(pk.slice(0, 6), 16);
    const hue = h % 360;
    return {
      bg: `hsl(${hue}, 65%, 18%)`,
      fg: `hsl(${hue}, 80%, 65%)`,
    };
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
      // Mirror for symmetry
      const full = [...row];
      for (let c = mid - 2; c >= 0; c--) full.push(row[c]);
      grid.push(full);
    }
    return grid;
  }

  let colors = $derived(hashToColors(pubkey));
  let grid = $derived(hashToGrid(pubkey));
  let cellSize = $derived(size / cells);
  let initials = $derived(pubkey?.slice(0, 2).toUpperCase() || '??');
</script>

<div
  class="identicon"
  style="width:{size}px;height:{size}px;background:{colors.bg};border-radius:{size > 32 ? 12 : 8}px"
  title={pubkey ? pubkey.slice(0, 12) + '…' : 'Unknown'}
>
  <svg width={size} height={size} viewBox="0 0 {size} {size}">
    {#each grid as row, r}
      {#each row as on, c}
        {#if on}
          <rect
            x={c * cellSize} y={r * cellSize}
            width={cellSize} height={cellSize}
            fill={colors.fg} opacity="0.85"
          />
        {/if}
      {/each}
    {/each}
  </svg>
  <span class="identicon-fallback" style="font-size:{Math.max(size * 0.28, 9)}px;color:{colors.fg}">
    {initials}
  </span>
</div>

<style>
  .identicon {
    position: relative; overflow: hidden; flex-shrink: 0;
    display: inline-flex; align-items: center; justify-content: center;
    border: 1px solid rgba(255,255,255,0.06);
    transition: transform 0.15s ease, box-shadow 0.15s ease;
  }
  .identicon:hover {
    transform: scale(1.05);
    box-shadow: 0 0 12px rgba(124, 58, 237, 0.3);
  }
  .identicon svg { position: absolute; inset: 0; }
  .identicon-fallback {
    position: relative; z-index: 1;
    font-weight: 800; letter-spacing: 0.04em;
    font-family: var(--font-mono);
    text-shadow: 0 1px 3px rgba(0,0,0,0.4);
  }
</style>
