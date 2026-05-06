<script lang="ts">
  let { activeView = 'dashboard', onNavigate, nodeMode = 'Actif' } = $props<{
    activeView: string;
    onNavigate: (v: string) => void;
    nodeMode?: string;
  }>();

  const NAV = [
    { id: 'browser',       label: 'Browser',       icon: 'browser' },
    { id: 'builder',       label: 'Page Builder',  icon: 'builder' },
    { id: 'forums',        label: 'Forums',        icon: 'forums' },
    { id: 'subscriptions', label: 'Abonnements',   icon: 'subs' },
    { id: 'dashboard',     label: 'Dashboard',     icon: 'dashboard' },
    { id: 'wallet',        label: 'Wallet',        icon: 'wallet' },
    { id: 'network',       label: 'Réseau',        icon: 'network' },
    { id: 'profile',       label: 'Profil',        icon: 'profile' },
    { id: 'explorer',      label: 'Explorateur',   icon: 'explorer' },
  ];
</script>

<nav class="sidebar">
  <div class="sidebar-logo">
    <div class="logo-mark">
      <svg viewBox="0 0 16 16" fill="none">
        <circle cx="8" cy="8.5" r="2" fill="#fff"/>
        <path d="M8 5v7M5 8.5h6" stroke="#000" stroke-width="1.5"/>
      </svg>
    </div>
    <span class="logo-text">QUANTA</span>
  </div>

  <div class="sidebar-nav">
    {#each NAV as item}
      <button
        class="nav-item"
        class:active={activeView === item.id}
        onclick={() => onNavigate(item.id)}
      >
        {#if item.icon === 'browser'}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="6.5"/><path d="M1.5 8h13M8 1.5c2 2 2 11 0 13M8 1.5c-2 2-2 11 0 13"/></svg>
        {:else if item.icon === 'builder'}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="2" y="2" width="12" height="12" rx="1.5"/><path d="M2 6h12M6 6v8"/></svg>
        {:else if item.icon === 'forums'}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2 4h10a1 1 0 011 1v5a1 1 0 01-1 1H6l-3 3v-3H2a1 1 0 01-1-1V5a1 1 0 011-1z"/></svg>
        {:else if item.icon === 'subs'}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2 4h12M2 8h12M2 12h8"/></svg>
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
        {item.label}
      </button>
    {/each}
  </div>

  <div class="sidebar-footer">
    <div class="node-status">
      <div class="pulse-dot"></div>
      <div style="font-size:11px;">
        <div style="color:var(--color-text-2);font-size:10px;">Nœud</div>
        <div style="color:var(--color-green);font-weight:600;">{nodeMode}</div>
      </div>
    </div>
  </div>
</nav>
