<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Identicon from "./Identicon.svelte";

  let { activeView, publicKey, onNavigate }: {
    activeView: string; publicKey: string;
    onNavigate: (view: string) => void;
  } = $props();

  let nodeStatus = $state<any>(null);
  let pkCopied = $state(false);
  let miningAtn = $state(0);

  const nav = [
    { id: "feed",      label: "Fil",          shortcut: "1", svg: "feed" },
    { id: "discover",  label: "Découvrir",    shortcut: "2", svg: "search" },
    { id: "editor",    label: "Créer",        shortcut: "3", svg: "compose" },
    { id: "wallet",    label: "Portefeuille", shortcut: "4", svg: "wallet" },
    { id: "profile",   label: "Profil",       shortcut: "5", svg: "user" },
    { id: "settings",  label: "Réglages",     shortcut: ",", svg: "gear" },
  ];

  $effect(() => {
    const poll = setInterval(async () => {
      try {
        nodeStatus = await invoke("get_node_status");
        const rep = await invoke<any>("get_my_reputation");
        miningAtn = rep?.energy_atn_mined ?? 0;
      } catch {}
    }, 5000);
    return () => clearInterval(poll);
  });

  $effect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const t = e.target as HTMLElement;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
      const map: Record<string, string> = { "1":"feed","2":"discover","3":"editor","4":"wallet","5":"profile","," :"settings" };
      const v = map[e.key];
      if (v) { e.preventDefault(); onNavigate(v); }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  });

  function shortKey(k: string): string {
    return k?.length > 12 ? k.slice(0, 6) + "…" + k.slice(-4) : k || "";
  }

  async function copyPk() {
    if (!publicKey) return;
    await navigator.clipboard.writeText(publicKey);
    pkCopied = true;
    setTimeout(() => pkCopied = false, 1600);
  }
</script>

<aside class="sidebar">
  <!-- Brand -->
  <div class="sb-brand">
    <div class="sb-mark"><span>◈</span></div>
    <span class="sb-name">Torus</span>
  </div>

  <!-- Identity -->
  <button class="sb-user" onclick={() => onNavigate("profile")}>
    <Identicon pubkey={publicKey} size={28} />
    <div class="sb-user-info">
      <span class="sb-user-name mono">{shortKey(publicKey)}</span>
      <span class="sb-user-status">
        <span class="dot" class:dot-on={nodeStatus?.is_online} class:dot-off={!nodeStatus?.is_online}></span>
        {nodeStatus?.is_online ? `${nodeStatus?.peer_count ?? 0} pairs` : "Hors ligne"}
      </span>
    </div>
  </button>

  <!-- Navigation -->
  <nav class="sb-nav">
    {#each nav as item}
      <button class="sb-item" class:active={activeView === item.id}
        onclick={() => onNavigate(item.id)}
        id="nav-{item.id}">
        <span class="sb-icon">
          {#if item.svg === "feed"}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M4 6h16M4 12h16M4 18h10"/></svg>
          {:else if item.svg === "search"}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/></svg>
          {:else if item.svg === "compose"}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M16 4l4 4-12 12H4v-4z"/><path d="M14 6l4 4"/></svg>
          {:else if item.svg === "wallet"}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="6" width="18" height="14" rx="2"/><path d="M3 10h18"/><circle cx="17" cy="15" r="1.2" fill="currentColor"/></svg>
          {:else if item.svg === "user"}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/></svg>
          {:else if item.svg === "gear"}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3 1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8 1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/></svg>
          {/if}
        </span>
        <span class="sb-label">{item.label}</span>
        <span class="sb-shortcut mono">{item.shortcut}</span>
      </button>
    {/each}
  </nav>

  <!-- Footer -->
  <div class="sb-footer">
    <div class="sb-mining">
      <span class="sb-mining-label">Miné</span>
      <span class="sb-mining-val mono">{miningAtn.toFixed(3)} ATN</span>
    </div>
    <button class="sb-id" onclick={copyPk}>
      <span class="mono">{shortKey(publicKey)}</span>
      <span class="sb-id-action">{pkCopied ? "Copié" : "Copier"}</span>
    </button>
  </div>
</aside>

<style>
  .sidebar {
    width: var(--sidebar-w); height: 100vh;
    display: flex; flex-direction: column;
    background: var(--color-bg-1);
    border-right: 1px solid var(--color-border);
    flex-shrink: 0;
    user-select: none;
  }

  /* Brand */
  .sb-brand {
    display: flex; align-items: center; gap: 10px;
    padding: 18px 18px 10px;
  }
  .sb-mark {
    width: 26px; height: 26px;
    border-radius: 7px;
    background: var(--color-accent);
    display: flex; align-items: center; justify-content: center;
    color: white; font-size: 13px;
    flex-shrink: 0;
  }
  .sb-name {
    font-family: var(--font-display);
    font-size: 15px; font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--color-text-0);
  }

  /* Identity */
  .sb-user {
    all: unset;
    display: flex; align-items: center; gap: 10px;
    padding: 8px 10px;
    margin: 6px 10px 4px;
    border-radius: var(--radius);
    cursor: pointer;
    transition: background 0.12s;
  }
  .sb-user:hover { background: var(--color-bg-2); }
  .sb-user-info { display: flex; flex-direction: column; min-width: 0; gap: 1px; }
  .sb-user-name {
    font-size: 12px; font-weight: 500;
    color: var(--color-text-0);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .sb-user-status {
    display: flex; align-items: center; gap: 5px;
    font-size: 11px; color: var(--color-text-2);
  }

  /* Nav */
  .sb-nav {
    flex: 1;
    padding: 6px 8px;
    display: flex; flex-direction: column; gap: 1px;
  }
  .sb-item {
    all: unset;
    display: flex; align-items: center; gap: 10px;
    padding: 7px 10px;
    border-radius: var(--radius);
    color: var(--color-text-1);
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
    font-size: 13px; font-weight: 500;
  }
  .sb-icon {
    width: 18px; height: 18px;
    display: flex; align-items: center; justify-content: center;
    color: var(--color-text-2);
    flex-shrink: 0;
  }
  .sb-icon :global(svg) { width: 17px; height: 17px; }
  .sb-label { flex: 1; }
  .sb-shortcut {
    font-size: 11px;
    color: var(--color-text-3);
    padding: 1px 6px;
    border-radius: 4px;
    background: transparent;
  }

  .sb-item:hover {
    background: var(--color-bg-2);
    color: var(--color-text-0);
  }
  .sb-item:hover .sb-icon { color: var(--color-text-0); }
  .sb-item:hover .sb-shortcut { background: var(--color-bg-3); color: var(--color-text-2); }

  .sb-item.active {
    background: var(--color-bg-2);
    color: var(--color-text-0);
  }
  .sb-item.active .sb-icon { color: var(--color-accent); }

  /* Footer */
  .sb-footer {
    padding: 10px 12px 14px;
    border-top: 1px solid var(--color-border);
    display: flex; flex-direction: column; gap: 4px;
  }
  .sb-mining {
    display: flex; justify-content: space-between; align-items: baseline;
    padding: 6px 10px;
    font-size: 11px;
  }
  .sb-mining-label { color: var(--color-text-2); }
  .sb-mining-val { color: var(--color-text-0); font-weight: 500; }

  .sb-id {
    all: unset; cursor: pointer;
    display: flex; align-items: center; justify-content: space-between;
    padding: 6px 10px;
    border-radius: var(--radius);
    transition: background 0.12s;
  }
  .sb-id:hover { background: var(--color-bg-2); }
  .sb-id .mono { font-size: 11px; color: var(--color-text-2); }
  .sb-id-action {
    font-size: 11px;
    color: var(--color-text-3);
    transition: color 0.12s;
  }
  .sb-id:hover .sb-id-action { color: var(--color-accent); }
</style>
