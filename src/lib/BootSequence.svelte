<script lang="ts">
  /**
   * BootSequence — Quiet startup. Brand mark + thin progress bar.
   * No grid, no neon, no signature stream.
   */
  import { onMount } from "svelte";

  let { onComplete }: { onComplete?: () => void } = $props();

  const DURATION = 900;
  let progress = $state(0);

  onMount(() => {
    const start = performance.now();
    let raf = 0;
    const step = (now: number) => {
      const k = Math.min((now - start) / DURATION, 1);
      progress = 1 - Math.pow(1 - k, 3);
      if (k < 1) raf = requestAnimationFrame(step);
      else setTimeout(() => onComplete?.(), 180);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  });
</script>

<div class="boot">
  <div class="boot-inner">
    <div class="boot-mark">
      <span>◈</span>
    </div>
    <div class="boot-name">Torus</div>
    <div class="boot-bar">
      <div class="boot-fill" style="width:{progress * 100}%"></div>
    </div>
  </div>
</div>

<style>
  .boot {
    position: fixed; inset: 0; z-index: 999;
    display: flex; align-items: center; justify-content: center;
    background: var(--color-bg-0);
    animation: fadeIn 0.2s ease-out;
  }
  .boot-inner {
    display: flex; flex-direction: column; align-items: center;
    animation: fadeIn 0.4s ease-out;
  }
  .boot-mark {
    width: 56px; height: 56px;
    border-radius: 14px;
    background: var(--color-accent);
    display: flex; align-items: center; justify-content: center;
    color: white; font-size: 24px;
    margin-bottom: 22px;
  }
  .boot-name {
    font-family: var(--font-display);
    font-size: 17px; font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--color-text-0);
    margin-bottom: 28px;
  }
  .boot-bar {
    width: 120px; height: 2px;
    background: var(--color-bg-3);
    border-radius: 1px;
    overflow: hidden;
  }
  .boot-fill {
    height: 100%; background: var(--color-accent);
    border-radius: 1px;
    transition: width 0.08s linear;
  }
</style>
