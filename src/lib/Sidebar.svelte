<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Aurora from "./Aurora.svelte";
  import { t, type TKey } from "./i18n.svelte";

  let { activeView = 'dashboard', onNavigate, nodeMode = 'Actif' } = $props<{
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

  // Le mode du nœud vient du backend ("Actif"/"Guardian"/"Recherche") → traduit.
  const MODE_KEY: Record<string, TKey> = { 'Actif': 'db.mode.actif', 'Guardian': 'db.mode.guardian', 'Recherche': 'db.mode.research' };

  const NAV: { id: string; label: TKey; icon: string }[] = [
    { id: 'wallet',        label: 'nav.wallet',    icon: 'wallet' },
    { id: 'contacts',      label: 'nav.contacts',  icon: 'contacts' },
    { id: 'dashboard',     label: 'nav.dashboard', icon: 'dashboard' },
    { id: 'network',       label: 'nav.network',   icon: 'network' },
    { id: 'explorer',      label: 'nav.explorer',  icon: 'explorer' },
    { id: 'profile',       label: 'nav.profile',   icon: 'profile' },
  ];
</script>

<nav class="sidebar">
  <div class="sidebar-logo">
    <Aurora radius={7}><span class="logo-q">Q</span></Aurora>
    <span class="logo-text">QUANTA</span>
  </div>

  <div class="sidebar-nav">
    {#each NAV as item}
      <button
        class="nav-item"
        class:active={activeView === item.id}
        onclick={() => onNavigate(item.id)}
      >
        {#if item.icon === 'contacts'}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="6" cy="6" r="2.5"/><path d="M1.5 14c0-2.5 2-4 4.5-4s4.5 1.5 4.5 4"/><path d="M11 5.5a2.2 2.2 0 010 4M12.5 14c0-1.8-.7-3-1.8-3.6"/></svg>
        {:else if item.icon === 'dashboard'}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="1" y="1" width="6" height="6" rx="1.5"/><rect x="9" y="1" width="6" height="6" rx="1.5"/><rect x="1" y="9" width="6" height="6" rx="1.5"/><rect x="9" y="9" width="6" height="6" rx="1.5"/></svg>
        {:else if item.icon === 'wallet'}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="1" y="4" width="14" height="10" rx="2"/><path d="M1 7h14"/><circle cx="11.5" cy="11" r="1" fill="currentColor" stroke="none"/></svg>
        {:else if item.icon === 'network'}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="2.5"/><circle cx="2" cy="3" r="1.5"/><circle cx="14" cy="3" r="1.5"/><circle cx="2" cy="13" r="1.5"/><circle cx="14" cy="13" r="1.5"/><path d="M3.5 4L6.5 6.5M9.5 6.5L12.5 4M3.5 12L6.5 9.5M9.5 9.5L12.5 12"/></svg>
        {:else if item.icon === 'profile'}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="6" r="3"/><path d="M2 14c0-3.314 2.686-5 6-5s6 1.686 6 5"/></svg>
        {:else if item.icon === 'explorer'}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/></svg>
        {/if}
        {t(item.label)}
      </button>
    {/each}
  </div>

  <div class="sidebar-footer">
    {#if username}
      <button class="nav-item" style="margin-bottom:8px;font-weight:600;color:var(--color-accent);" onclick={() => onNavigate('profile')} title={t('nav.yourIdentity')}>
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="6" r="3"/><path d="M2 14c0-3.314 2.686-5 6-5s6 1.686 6 5"/></svg>
        @{username}
      </button>
    {/if}
    <div class="node-status">
      <div class="pulse-dot"></div>
      <div style="font-size:11px;">
        <div style="color:var(--color-text-2);font-size:10px;">{t('node.label')}</div>
        <div style="color:var(--color-green);font-weight:600;">{MODE_KEY[nodeMode] ? t(MODE_KEY[nodeMode]) : nodeMode}</div>
      </div>
    </div>
  </div>
</nav>

<style>
  .sidebar-logo :global(.aurora) {
    width: 28px; height: 28px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
  }
  .logo-q { font-size: 15px; font-weight: 800; color: #fff; line-height: 1; }
</style>
