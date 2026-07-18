<script lang="ts">
  import { t, type TKey } from "./i18n.svelte";

  let { isOpen, onClose, onCommand }: {
    isOpen: boolean;
    onClose: () => void;
    onCommand: (id: string) => void;
  } = $props();

  let query = $state("");
  let activeIndex = $state(0);

  const commands: { id: string; labelKey: TKey; shortcut: string; groupKey: TKey }[] = [
    { id: "wallet", labelKey: "cmd.wallet", shortcut: "W", groupKey: "cmd.group.goTo" },
    { id: "dashboard", labelKey: "cmd.dashboard", shortcut: "D", groupKey: "cmd.group.goTo" },
    { id: "network", labelKey: "cmd.network", shortcut: "N", groupKey: "cmd.group.goTo" },
    { id: "explorer", labelKey: "cmd.explorer", shortcut: "E", groupKey: "cmd.group.goTo" },
    { id: "profile", labelKey: "cmd.profile", shortcut: "P", groupKey: "cmd.group.goTo" },
    { id: "settings", labelKey: "cmd.settings", shortcut: ",", groupKey: "cmd.group.goTo" },
  ];

  let filtered = $derived(
    query
      ? commands.filter(c => t(c.labelKey).toLowerCase().includes(query.toLowerCase()))
      : commands
  );

  $effect(() => {
    if (!isOpen) { query = ""; activeIndex = 0; }
  });

  $effect(() => {
    if (!isOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") { onClose(); return; }
      if (e.key === "ArrowDown") { e.preventDefault(); activeIndex = Math.min(activeIndex + 1, filtered.length - 1); }
      if (e.key === "ArrowUp") { e.preventDefault(); activeIndex = Math.max(activeIndex - 1, 0); }
      if (e.key === "Enter" && filtered[activeIndex]) {
        onCommand(filtered[activeIndex].id);
        onClose();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  });
</script>

{#if isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="cmd-overlay" onclick={onClose}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="cmd-box" onclick={(e) => e.stopPropagation()}>
      <input
        class="cmd-input"
        type="text"
        placeholder={t('cmd.placeholder')}
        bind:value={query}
        id="cmd-palette-input"
      />
      <div class="cmd-list">
        {#each filtered as cmd, i (cmd.id)}
          <button
            class="cmd-item"
            class:active={i === activeIndex}
            onclick={() => { onCommand(cmd.id); onClose(); }}
            id="cmd-{cmd.id}"
          >
            <span class="cmd-icon">◈</span>
            <span class="cmd-label">{t(cmd.labelKey)}</span>
            <span class="cmd-shortcut mono">⌘{cmd.shortcut}</span>
          </button>
        {/each}
        {#if filtered.length === 0}
          <div class="cmd-empty">{t('cmd.empty')} "{query}"</div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .cmd-overlay {
    position: fixed; inset: 0; z-index: 100;
    display: flex; align-items: flex-start; justify-content: center;
    padding-top: 20vh;
    background: rgba(0, 0, 0, 0.32);
    backdrop-filter: blur(4px);
    animation: fadeIn 0.1s ease-out;
  }
  .cmd-box {
    width: 520px; background: var(--surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    overflow: hidden;
    box-shadow: var(--shadow-lg);
  }
  .cmd-input {
    width: 100%; padding: 14px 18px;
    background: transparent; border: none;
    border-bottom: 1px solid var(--color-border);
    color: var(--color-text-0); font-family: inherit;
    font-size: 15px; outline: none;
  }
  .cmd-input::placeholder { color: var(--color-text-2); }
  .cmd-list { max-height: 300px; overflow-y: auto; padding: 4px 0; }
  .cmd-item {
    display: flex; align-items: center; gap: 10px;
    width: 100%; padding: 10px 18px;
    border: none; background: transparent;
    color: var(--color-text-1); cursor: pointer;
    font-family: inherit; font-size: 13px; text-align: left;
    transition: all 0.08s;
  }
  .cmd-item:hover, .cmd-item.active {
    background: var(--color-accent-dim);
    color: var(--color-text-0);
  }
  .cmd-item.active { border-left: 2px solid var(--color-accent); padding-left: 16px; }
  .cmd-icon { font-size: 12px; color: var(--color-accent); opacity: 0.6; }
  .cmd-label { flex: 1; }
  .cmd-shortcut { font-size: 11px; color: var(--color-text-3); }
  .cmd-empty { padding: 16px 18px; text-align: center; color: var(--color-text-3); font-size: 12px; }
</style>
