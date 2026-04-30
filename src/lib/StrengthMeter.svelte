<script lang="ts">
  let { password = "" }: { password: string } = $props();

  const score = $derived.by(() => {
    if (!password) return 0;
    let s = 0;
    if (password.length >= 8) s += 1;
    if (password.length >= 12) s += 1;
    if (password.length >= 16) s += 1;
    if (/[a-z]/.test(password) && /[A-Z]/.test(password)) s += 1;
    if (/\d/.test(password)) s += 1;
    if (/[^A-Za-z0-9]/.test(password)) s += 1;
    return Math.min(s, 5);
  });

  const label = $derived(
    score === 0 ? "" :
    score <= 1 ? "Très faible" :
    score === 2 ? "Faible" :
    score === 3 ? "Correct" :
    score === 4 ? "Fort" : "Excellent"
  );

  const colorVar = $derived(
    score <= 1 ? "var(--color-red)" :
    score === 2 ? "var(--color-amber)" :
    score === 3 ? "var(--color-amber)" :
    "var(--color-green)"
  );
</script>

{#if password}
  <div class="meter">
    <div class="bars">
      {#each Array(5) as _, i}
        <div class="bar" class:on={i < score} style="--c:{colorVar}"></div>
      {/each}
    </div>
    {#if label}
      <span class="lab" style="color:{colorVar}">{label}</span>
    {/if}
  </div>
{/if}

<style>
  .meter { display: flex; align-items: center; gap: 8px; margin-top: 6px; }
  .bars { display: flex; gap: 3px; flex: 1; }
  .bar {
    flex: 1; height: 3px; border-radius: 2px;
    background: var(--color-bg-3);
    transition: background 0.2s;
  }
  .bar.on { background: var(--c, var(--color-accent)); }
  .lab { font-size: 10px; font-weight: 700; letter-spacing: 0.04em; }
</style>
