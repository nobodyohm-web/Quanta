<script lang="ts">
  /**
   * Sparkline — Inline mini-chart, single accent stroke.
   */
  let { data = [], width = 80, height = 22 }: {
    data?: number[]; width?: number; height?: number;
  } = $props();

  let path = $derived.by(() => {
    if (data.length < 2) return "";
    const max = Math.max(...data);
    const min = Math.min(...data);
    const range = Math.max(max - min, 0.001);
    return data.map((v, i) => {
      const x = (i / (data.length - 1)) * width;
      const y = height - ((v - min) / range) * (height - 4) - 2;
      return `${i === 0 ? "M" : "L"} ${x.toFixed(2)} ${y.toFixed(2)}`;
    }).join(" ");
  });
</script>

<svg viewBox="0 0 {width} {height}" {width} {height} class="sparkline" aria-hidden="true">
  {#if data.length > 1}
    <path d={path} fill="none" stroke="var(--color-accent)" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" opacity="0.85"/>
  {/if}
</svg>

<style>
  .sparkline {
    display: inline-block;
    vertical-align: middle;
  }
</style>
