<script lang="ts">
  let { isOpen, onClose, onCommand }: {
    isOpen: boolean;
    onClose: () => void;
    onCommand: (id: string) => void;
  } = $props();

  let query = $state("");
  let activeIndex = $state(0);

  const commands = [
    { id: "feed", label: "Aller au fil", shortcut: "F", group: "Navigate" },
    { id: "discover", label: "Rechercher sur le réseau", shortcut: "S", group: "Navigate" },
    { id: "editor", label: "Créer / éditer un site", shortcut: "E", group: "Navigate" },
    { id: "wallet", label: "Ouvrir le portefeuille", shortcut: "W", group: "Navigate" },
    { id: "profile", label: "Mon profil", shortcut: "P", group: "Navigate" },
    { id: "settings", label: "Réglages", shortcut: ",", group: "Navigate" },
  ];

  let filtered = $derived(
    query
      ? commands.filter(c => c.label.toLowerCase().includes(query.toLowerCase()))
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
        placeholder="Type a command…"
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
            <span class="cmd-label">{cmd.label}</span>
            <span class="cmd-shortcut mono">⌘{cmd.shortcut}</span>
          </button>
        {/each}
        {#if filtered.length === 0}
          <div class="cmd-empty">No commands match "{query}"</div>
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
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(4px);
    animation: fadeIn 0.1s ease-out;
  }
  .cmd-box {
    width: 520px; background: var(--color-bg-1);
    border: 1px solid var(--color-border-hover);
    border-radius: var(--radius-lg);
    overflow: hidden;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.6), 0 0 0 1px rgba(0, 122, 255, 0.08);
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
