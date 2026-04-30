<script lang="ts">
  /**
   * LiveCounter — Animated numeric value that tweens from old to new.
   * Used for ATN balances, view counts, trust scores — anywhere a number
   * arrives async and should feel "alive" rather than snapping into place.
   */
  let { value = 0, decimals = 0, duration = 600, prefix = "", suffix = "" }: {
    value?: number; decimals?: number; duration?: number;
    prefix?: string; suffix?: string;
  } = $props();

  let display = $state(0);
  let from = 0;
  let to = 0;
  let raf = 0;

  $effect(() => {
    const target = Number(value) || 0;
    if (Math.abs(target - display) < Math.pow(10, -decimals - 1)) {
      display = target;
      return;
    }
    from = display;
    to = target;
    const start = performance.now();
    cancelAnimationFrame(raf);
    const step = (t: number) => {
      const k = Math.min((t - start) / duration, 1);
      // easeOutCubic
      const e = 1 - Math.pow(1 - k, 3);
      display = from + (to - from) * e;
      if (k < 1) raf = requestAnimationFrame(step);
      else display = to;
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  });

  let formatted = $derived(prefix + display.toFixed(decimals) + suffix);
</script>

<span class="live-counter">{formatted}</span>

<style>
  .live-counter {
    font-variant-numeric: tabular-nums;
    font-feature-settings: 'tnum';
  }
</style>
