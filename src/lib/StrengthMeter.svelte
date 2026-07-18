<script lang="ts">
  import { t } from "./i18n.svelte";

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
    score <= 1 ? t('pw.veryWeak') :
    score === 2 ? t('pw.weak') :
    score === 3 ? t('pw.ok') :
    score === 4 ? t('pw.strong') : t('pw.excellent')
  );

  const colorVar = $derived(
    score <= 1 ? "var(--color-red)" :
    score === 2 ? "var(--color-amber)" :
    score === 3 ? "var(--color-amber)" :
    "var(--cyan)"
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
    flex: 1; height: 4px; border-radius: 8px;
    background: var(--color-bg-3);
    transition: background var(--dur-fast) var(--ease-out);
  }
  .bar.on { background: var(--c, var(--color-accent)); }
  .lab { font-size: 10px; font-weight: 700; letter-spacing: 0.04em; }
</style>
