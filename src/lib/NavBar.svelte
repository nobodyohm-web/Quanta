<script lang="ts">
  let { activeView, onNavigate }: {
    activeView: string;
    onNavigate: (view: string) => void;
  } = $props();

  const items = [
    { id: "wallet",   label: "Wallet",   icon: "wallet" },
    { id: "network",  label: "Réseau",   icon: "network" },
    { id: "settings", label: "Réglages", icon: "settings" },
  ];
</script>

<nav class="navbar" aria-label="Navigation principale">
  {#each items as item (item.id)}
    <button
      class="nb-item"
      class:active={activeView === item.id}
      onclick={() => onNavigate(item.id)}
      aria-current={activeView === item.id ? "page" : undefined}
    >
      <span class="nb-icon">
        {#if item.icon === "wallet"}
          <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <rect x="3" y="6" width="18" height="14" rx="2"/><path d="M3 10h18"/><circle cx="17" cy="15" r="1.2" fill="currentColor"/>
          </svg>
        {:else if item.icon === "network"}
          <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="5" r="2.5"/><circle cx="5" cy="19" r="2.5"/><circle cx="19" cy="19" r="2.5"/>
            <path d="M12 7.5v4.5M10 14l-3.5 3M14 14l3.5 3"/>
          </svg>
        {:else if item.icon === "settings"}
          <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 01-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/>
          </svg>
        {/if}
      </span>
      <span class="nb-label">{item.label}</span>
    </button>
  {/each}
</nav>

<style>
  .navbar {
    position: fixed; bottom: 0; left: 0; right: 0;
    height: var(--navbar-h);
    display: flex; align-items: center;
    background: var(--color-bg-0);
    border-top: 1px solid var(--color-border);
    z-index: 50;
    padding-bottom: env(safe-area-inset-bottom, 0px);
  }

  .nb-item {
    all: unset;
    flex: 1;
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 4px;
    height: 100%;
    cursor: pointer;
    color: var(--color-text-3);
    transition: color 0.15s ease;
    user-select: none;
  }
  .nb-item:hover { color: var(--color-text-1); }
  .nb-item.active { color: var(--color-accent); }

  .nb-icon {
    display: flex; align-items: center; justify-content: center;
    width: 24px; height: 24px;
  }

  .nb-label {
    font-size: 10px; font-weight: 500;
    letter-spacing: 0.01em;
  }
</style>
