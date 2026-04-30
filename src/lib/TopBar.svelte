<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let { onHelp } = $props<{ onHelp: () => void }>();

  let balance = $state(0);
  let online = $state(false);
  let peers = $state(0);

  async function refresh() {
    try {
      const r = await invoke<{ atn_balance: number }>("get_my_reputation");
      balance = r?.atn_balance ?? 0;
    } catch { /* ignore */ }
    try {
      const s = await invoke<{ is_online: boolean; peer_count: number }>("get_node_status");
      online = !!s?.is_online;
      peers = s?.peer_count ?? 0;
    } catch { /* ignore */ }
  }

  $effect(() => {
    refresh();
    const t = setInterval(refresh, 10_000);
    return () => clearInterval(t);
  });
</script>

<header class="topbar">
  <div class="tb-left">
    <span class="tb-logo">SOVA</span>
  </div>
  <div class="tb-right">
    <span class="tb-balance mono">{balance.toFixed(2)} <span class="tb-unit">ATN</span></span>
    <span class="tb-status">
      <span class="dot" class:dot-on={online} class:dot-off={!online}></span>
      {#if online}<span class="tb-peers">{peers}</span>{/if}
    </span>
    <button class="tb-help" onclick={onHelp} title="Aide (⌘/)">?</button>
  </div>
</header>

<style>
  .topbar {
    height: var(--topbar-h);
    display: flex; align-items: center; justify-content: space-between;
    padding: 0 20px;
    background: var(--color-bg-0);
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .tb-left { display: flex; align-items: center; }
  .tb-logo {
    font-size: 16px; font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--color-text-0);
  }

  .tb-right {
    display: flex; align-items: center; gap: 16px;
  }
  .tb-balance {
    font-size: 13px; font-weight: 600;
    color: var(--color-text-0);
  }
  .tb-unit { color: var(--color-text-2); font-weight: 400; }

  .tb-status {
    display: flex; align-items: center; gap: 4px;
    font-size: 11px; color: var(--color-text-2);
  }
  .tb-peers { font-family: var(--font-mono); font-size: 11px; }

  .tb-help {
    width: 24px; height: 24px;
    display: flex; align-items: center; justify-content: center;
    background: none; border: 1px solid var(--color-border);
    border-radius: 50%;
    color: var(--color-text-2);
    font-size: 12px; font-weight: 600;
    cursor: pointer;
    transition: border-color 0.15s ease, color 0.15s ease;
  }
  .tb-help:hover { border-color: var(--color-border-hover); color: var(--color-text-0); }
</style>
