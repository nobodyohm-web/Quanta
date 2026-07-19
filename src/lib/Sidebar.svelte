<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import QuantaMark from "./brand/QuantaMark.svelte";
  import { t, type TKey } from "./i18n.svelte";

  let { activeView = 'wallet', onNavigate, nodeMode = 'Actif' } = $props<{
    activeView: string;
    onNavigate: (v: string) => void;
    nodeMode?: string;
  }>();

  // Pseudo unique @handle — affiché comme identité dans la sidebar.
  let username = $state<string | null>(null);
  $effect(() => {
    let alive = true;
    const load = () => invoke<string | null>("get_my_username")
      .then((u) => { if (alive) username = u; })
      .catch(() => {});
    load();
    const iv = setInterval(load, 10000);
    return () => { alive = false; clearInterval(iv); };
  });

  // The nav mark pulses its seam when WE seal a real block (not during sync).
  let sealing = $state(false);
  $effect(() => {
    let un: (() => void) | undefined;
    let to: ReturnType<typeof setTimeout> | undefined;
    listen<{ mine?: boolean }>("quanta://block-sealed", (e) => {
      if (!e?.payload?.mine) return;
      sealing = true;
      clearTimeout(to);
      to = setTimeout(() => (sealing = false), 80);
    }).then((u) => (un = u)).catch(() => {});
    return () => { un?.(); clearTimeout(to); };
  });

  // Le mode du nœud vient du backend ("Actif"/"Guardian"/"Recherche") → traduit.
  const MODE_KEY: Record<string, TKey> = { 'Actif': 'db.mode.actif', 'Guardian': 'db.mode.guardian', 'Recherche': 'db.mode.research' };

  // Navigation en trois groupes respirés : l'argent · le réseau · vous.
  // (Explorateur retiré — jugé sans valeur par le propriétaire.)
  const GROUPS: { id: string; label: TKey; icon: string }[][] = [
    [
      { id: 'wallet',     label: 'nav.wallet',     icon: 'wallet' },
      { id: 'contacts',   label: 'nav.contacts',   icon: 'contacts' },
    ],
    [
      { id: 'dashboard',  label: 'nav.dashboard',  icon: 'dashboard' },
      { id: 'network',    label: 'nav.network',    icon: 'network' },
    ],
    [
      { id: 'profile',    label: 'nav.profile',    icon: 'profile' },
      { id: 'whitepaper', label: 'nav.whitepaper', icon: 'whitepaper' },
      { id: 'settings',   label: 'set.title',      icon: 'settings' },
    ],
  ];
</script>

<nav class="sidebar">
  <div class="sidebar-logo">
    <QuantaMark size={28} tone="ink" {sealing} />
    <span class="logo-text">QUANTA</span>
  </div>

  <div class="sidebar-nav">
    {#each GROUPS as group, gi}
      {#if gi > 0}<div class="nav-gap" aria-hidden="true"></div>{/if}
      {#each group as item}
        <button
          class="nav-item"
          class:active={activeView === item.id}
          onclick={() => onNavigate(item.id)}
        >
          {#if item.icon === 'contacts'}
            <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="6" cy="6" r="2.5"/><path d="M1.5 14c0-2.5 2-4 4.5-4s4.5 1.5 4.5 4"/><path d="M11 5.5a2.2 2.2 0 010 4M12.5 14c0-1.8-.7-3-1.8-3.6"/></svg>
          {:else if item.icon === 'dashboard'}
            <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M9 1L3 9h4l-1 6 7-8H8l1-6z"/></svg>
          {:else if item.icon === 'wallet'}
            <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="1" y="4" width="14" height="10" rx="2"/><path d="M1 7h14"/><circle cx="11.5" cy="11" r="1" fill="currentColor" stroke="none"/></svg>
          {:else if item.icon === 'network'}
            <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="2.5"/><circle cx="2" cy="3" r="1.5"/><circle cx="14" cy="3" r="1.5"/><circle cx="2" cy="13" r="1.5"/><circle cx="14" cy="13" r="1.5"/><path d="M3.5 4L6.5 6.5M9.5 6.5L12.5 4M3.5 12L6.5 9.5M9.5 9.5L12.5 12"/></svg>
          {:else if item.icon === 'profile'}
            <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="6" r="3"/><path d="M2 14c0-3.314 2.686-5 6-5s6 1.686 6 5"/></svg>
          {:else if item.icon === 'whitepaper'}
            <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M4 1.5h5L12.5 5v9.5H4z"/><path d="M9 1.5V5h3.5M6 8h4.5M6 11h4.5"/></svg>
          {:else if item.icon === 'settings'}
            <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><circle cx="8" cy="8" r="2.2"/><path d="M8 1.8v1.6M8 12.6v1.6M1.8 8h1.6M12.6 8h1.6M3.6 3.6l1.1 1.1M11.3 11.3l1.1 1.1M12.4 3.6l-1.1 1.1M4.7 11.3l-1.1 1.1"/></svg>
          {/if}
          {t(item.label)}
        </button>
      {/each}
    {/each}
  </div>

  <div class="sidebar-footer">
    {#if username}
      <button class="nav-item nav-id" onclick={() => onNavigate('profile')} title={t('nav.yourIdentity')}>
        <svg width="17" height="17" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="6" r="3"/><path d="M2 14c0-3.314 2.686-5 6-5s6 1.686 6 5"/></svg>
        @{username}
      </button>
    {/if}
    <div class="node-status">
      <div class="pulse-dot"></div>
      <div class="node-txt">
        <div class="node-k">{t('node.label')}</div>
        <div class="node-v">{MODE_KEY[nodeMode] ? t(MODE_KEY[nodeMode]) : nodeMode}</div>
      </div>
    </div>
  </div>
</nav>

<style>
  .sidebar-logo :global(svg) { flex-shrink: 0; }
  .nav-gap { height: 18px; flex-shrink: 0; }
  .nav-id { margin-bottom: 8px; font-weight: 600; color: var(--color-accent); }
  .node-txt { font-size: 11px; min-width: 0; }
  .node-k { color: var(--color-text-2); font-size: 10px; }
  .node-v { color: var(--color-green); font-weight: 600; }
</style>
