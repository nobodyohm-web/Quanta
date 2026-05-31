<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Identicon from "./Identicon.svelte";

  let query = $state("");
  let searched = $state<any>(null);
  let searchedPage = $state<any>(null);
  let searchErr = $state("");
  let searching = $state(false);
  let liveFeed = $state<any[]>([]);
  let publishedPages = $state<any[]>([]);

  async function refreshFeed() {
    try {
      const txs = await invoke<any[]>("get_recent_txs");
      if (txs && txs.length > 0) {
        liveFeed = txs.slice(0, 20).map(tx => ({
          type: tx.tx_type === 'Mining' ? 'mining' : tx.tx_type === 'Burn' ? 'burn' : 'transfer',
          amount: tx.amount ?? 0,
          from: tx.from ? shortKey(tx.from) : null,
          to: tx.to ? shortKey(tx.to) : null,
          timestamp: tx.timestamp ?? 0,
        }));
      }
    } catch {}
  }

  $effect(() => {
    refreshFeed();
    loadPages();
    const t = setInterval(refreshFeed, 5000);
    const t2 = setInterval(loadPages, 15000);
    return () => { clearInterval(t); clearInterval(t2); };
  });

  function shortKey(k: string) {
    if (!k || k.length < 12) return k ?? '';
    return k.slice(0, 6) + '…' + k.slice(-6);
  }

  async function handleSearch() {
    const q = query.trim();
    if (!q) return;
    searchErr = "";
    searching = true;
    searched = null;
    try {
      // Use real backend API to get balance
      const bal = await invoke<number>("get_balance", { pk: q });
      searched = {
        key: q,
        keyShort: shortKey(q),
        balance: bal ?? 0,
      };
    } catch (e) {
      searchErr = "Wallet non trouvé ou clé invalide";
    }
    // Also try to fetch page
    searchedPage = null;
    try {
      const p = await invoke<any>("get_page", { pk: q });
      if (p) searchedPage = p;
    } catch {}
    searching = false;
  }

  async function loadPages() {
    try {
      const p = await invoke<any[]>("list_pages");
      if (p) publishedPages = p;
    } catch {}
  }

  function formatAge(ts: number) {
    if (!ts) return '';
    const diff = Math.floor((Date.now() / 1000) - ts);
    if (diff < 0) return 'à l\'instant';
    if (diff < 60) return `il y a ${diff}s`;
    if (diff < 3600) return `il y a ${Math.floor(diff / 60)} min`;
    if (diff < 86400) return `il y a ${Math.floor(diff / 3600)}h`;
    return `il y a ${Math.floor(diff / 86400)}j`;
  }

  const feedColors: Record<string, { bg: string; color: string; icon: string }> = {
    mining: { bg: 'rgba(0,229,204,0.1)', color: '#00E5CC', icon: '⛏' },
    transfer: { bg: 'rgba(139,92,246,0.1)', color: '#8b5cf6', icon: '→' },
    burn: { bg: 'rgba(249,115,22,0.1)', color: '#f97316', icon: '🔥' },
  };
</script>

<div class="page">
  <div class="page-header">
    <div>
      <div class="page-title">Explorateur</div>
      <div class="page-sub">Transparent par design — tout est vérifiable</div>
    </div>
  </div>

  <!-- Search -->
  <div style="display:flex;gap:10px;margin-bottom:20px;">
    <input
      class="input mono"
      placeholder="Coller une clé publique pour voir un wallet…"
      bind:value={query}
      onkeydown={(e) => e.key === 'Enter' && handleSearch()}
      style="flex:1;"
    />
    <button class="btn btn-primary" onclick={handleSearch} disabled={searching}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/></svg>
      {searching ? '…' : 'Rechercher'}
    </button>
  </div>

  {#if searchErr}
    <div style="font-size:13px;color:var(--color-red);margin-bottom:16px;padding:12px;background:rgba(244,63,94,0.06);border-radius:8px;">{searchErr}</div>
  {/if}

  <!-- Searched wallet (REAL data from get_balance) -->
  {#if searched}
    <div class="card" style="margin-bottom:20px;border:1px solid rgba(0,229,204,0.2);">
      <div style="display:flex;gap:16px;align-items:center;margin-bottom:16px;">
        <Identicon pubkey={searched.key} size={52} />
        <div style="flex:1;">
          <div style="font-size:12px;color:var(--color-text-2);margin-bottom:6px;">Wallet public</div>
          <button class="copy-btn" onclick={() => navigator.clipboard?.writeText(searched.key)}>
            {searched.keyShort}
          </button>
        </div>
        <button style="background:none;border:none;cursor:pointer;color:var(--color-text-3);" onclick={() => searched = null}>
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 3l10 10M13 3L3 13"/></svg>
        </button>
      </div>
      <div>
        <div class="stat-label">Solde</div>
        <div class="stat-val mono">{searched.balance.toFixed(6)}<span style="font-size:14px;color:var(--color-text-2);margin-left:8px;">QUANTA</span></div>
      </div>
      {#if searchedPage}
        <div class="divider"></div>
        <div>
          <div style="display:flex;align-items:center;gap:8px;margin-bottom:10px;">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="var(--cyan)" stroke-width="1.5"><rect x="2" y="2" width="12" height="12" rx="2"/><path d="M2 5h12"/></svg>
            <span style="font-size:14px;font-weight:600;color:var(--cyan);">{searchedPage.title}</span>
          </div>
          <div style="background:var(--color-bg-2);border-radius:8px;padding:16px;font-size:13px;line-height:1.7;color:var(--color-text-1);">
            {@html searchedPage.content}
          </div>
          <div style="font-size:11px;color:var(--color-text-3);margin-top:8px;">
            v{searchedPage.version} · {formatAge(searchedPage.updated_at)}
          </div>
        </div>
      {/if}
    </div>
  {/if}

  <!-- Live feed (REAL transactions from backend) -->
  <div class="card">
    <div style="display:flex;align-items:center;gap:10px;margin-bottom:16px;">
      <div class="card-title" style="margin-bottom:0;">Transactions en direct</div>
      <div class="pulse-dot" style="margin-left:4px;"></div>
      <span style="font-size:11px;color:var(--color-text-3);">données réelles du réseau</span>
    </div>
    {#if liveFeed.length === 0}
      <div style="padding:24px;text-align:center;color:var(--color-text-3);font-size:13px;">
        En attente de transactions…
      </div>
    {:else}
      {#each liveFeed as tx, i}
        {@const fd = feedColors[tx.type] ?? feedColors.transfer}
        <div class="feed-item" style={i === 0 ? 'animation:fadein 0.3s ease;' : ''}>
          <div class="feed-type-badge" style="background:{fd.bg};color:{fd.color};font-size:14px;">
            {fd.icon}
          </div>
          <div style="flex:1;min-width:0;">
            <div style="font-size:13px;font-weight:500;display:flex;gap:6px;align-items:center;flex-wrap:wrap;">
              {#if tx.type === 'mining'}
                <span style="color:var(--color-text-2);">Mining</span>
                <span class="mono" style="color:var(--cyan);font-weight:700;">+{tx.amount.toFixed(4)}</span>
                <span class="dim">→</span>
                <span class="mono" style="font-size:11px;color:var(--color-text-2);">{tx.to}</span>
              {:else if tx.type === 'transfer'}
                <span class="mono" style="font-size:11px;color:var(--color-text-2);">{tx.from}</span>
                <span class="dim">→</span>
                <span class="mono" style="font-size:11px;color:var(--color-text-2);">{tx.to}</span>
                <span class="mono" style="color:#8b5cf6;font-weight:700;">{tx.amount.toFixed(4)}</span>
              {:else if tx.type === 'burn'}
                <span style="color:var(--color-text-2);">Brûlé</span>
                <span class="mono" style="color:var(--color-amber);font-weight:700;">{tx.amount.toFixed(4)}</span>
                <span class="dim">QNT</span>
              {/if}
            </div>
          </div>
          <div style="font-size:11px;color:var(--color-text-3);flex-shrink:0;">{formatAge(tx.timestamp)}</div>
        </div>
      {/each}
    {/if}
  </div>

  <!-- Published pages directory -->
  {#if publishedPages.length > 0}
    <div class="card" style="margin-top:12px;">
      <div class="card-title">Pages publiées sur le réseau</div>
      {#each publishedPages as pg}
        <div class="peer-row" style="cursor:pointer;" onclick={() => { query = pg.author_pk; handleSearch(); }}>
          <Identicon pubkey={pg.author_pk} size={28} />
          <div style="flex:1;min-width:0;">
            <div style="font-size:13px;font-weight:600;">{pg.title}</div>
            <div class="mono" style="font-size:10px;color:var(--color-text-3);">{shortKey(pg.author_pk)}</div>
          </div>
          <div style="font-size:11px;color:var(--color-text-3);">{formatAge(pg.updated_at)}</div>
          <div style="font-size:11px;color:var(--color-text-3);">{pg.size} B</div>
        </div>
      {/each}
    </div>
  {/if}
</div>
