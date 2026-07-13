<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Identicon from "./Identicon.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { t } from "./i18n.svelte";

  let query = $state("");
  let searched = $state<any>(null);
  let searchErr = $state("");
  let searching = $state(false);
  let liveFeed = $state<any[]>([]);

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
    const iv = setInterval(refreshFeed, 5000);
    return () => clearInterval(iv);
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
      searchErr = t('ex.searchError');
    }
    searching = false;
  }

  function formatAge(ts: number) {
    if (!ts) return '';
    const diff = Math.floor((Date.now() / 1000) - ts);
    if (diff < 0) return t('ex.ageNow');
    if (diff < 60) return `${t('ex.agePrefix')}${diff}${t('ex.ageSecondsSuffix')}`;
    if (diff < 3600) return `${t('ex.agePrefix')}${Math.floor(diff / 60)}${t('ex.ageMinutesSuffix')}`;
    if (diff < 86400) return `${t('ex.agePrefix')}${Math.floor(diff / 3600)}${t('ex.ageHoursSuffix')}`;
    return `${t('ex.agePrefix')}${Math.floor(diff / 86400)}${t('ex.ageDaysSuffix')}`;
  }

  const feedColors: Record<string, { bg: string; color: string; icon: string }> = {
    mining: { bg: 'rgba(11,165,160,0.1)', color: '#0BA5A0', icon: '⛏' },
    transfer: { bg: 'rgba(139,92,246,0.1)', color: '#8b5cf6', icon: '→' },
    burn: { bg: 'rgba(249,115,22,0.1)', color: '#f97316', icon: '🔥' },
  };
</script>

<div class="page">
  <div class="page-header">
    <div>
      <div class="page-title">{t('ex.title')}</div>
      <div class="page-sub">{t('ex.subtitle')}</div>
    </div>
  </div>

  <!-- Search -->
  <div style="display:flex;gap:10px;margin-bottom:20px;">
    <input
      class="input mono"
      placeholder={t('ex.searchPlaceholder')}
      bind:value={query}
      onkeydown={(e) => e.key === 'Enter' && handleSearch()}
      style="flex:1;"
    />
    <button class="btn btn-primary" onclick={handleSearch} disabled={searching}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/></svg>
      {searching ? '…' : t('ex.searchButton')}
    </button>
  </div>

  {#if searchErr}
    <div style="font-size:13px;color:var(--color-red);margin-bottom:16px;padding:12px;background:rgba(244,63,94,0.06);border-radius:8px;">{searchErr}</div>
  {/if}

  <!-- Searched wallet (REAL data from get_balance) -->
  {#if searched}
    <div class="card" style="margin-bottom:20px;border:1px solid rgba(11,165,160,0.2);">
      <div style="display:flex;gap:16px;align-items:center;margin-bottom:16px;">
        <Identicon pubkey={searched.key} size={52} />
        <div style="flex:1;">
          <div style="font-size:12px;color:var(--color-text-2);margin-bottom:6px;">{t('ex.walletPublic')}</div>
          <button class="copy-btn" onclick={() => navigator.clipboard?.writeText(searched.key)}>
            {searched.keyShort}
          </button>
        </div>
        <button style="background:none;border:none;cursor:pointer;color:var(--color-text-3);" aria-label={t('help.close_aria')} onclick={() => searched = null}>
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 3l10 10M13 3L3 13"/></svg>
        </button>
      </div>
      <div>
        <div class="stat-label">{t('ex.balance')}</div>
        <div class="stat-val mono">{searched.balance.toFixed(6)}<span style="font-size:14px;color:var(--color-text-2);margin-left:8px;">QUANTA</span></div>
      </div>
    </div>
  {/if}

  <!-- Live feed (REAL transactions from backend) -->
  <div class="card">
    <div style="display:flex;align-items:center;gap:10px;margin-bottom:16px;">
      <div class="card-title" style="margin-bottom:0;">{t('ex.liveTransactions')}</div>
      <div class="pulse-dot" style="margin-left:4px;"></div>
      <span style="font-size:11px;color:var(--color-text-3);">{t('ex.realNetworkData')}</span>
    </div>
    {#if liveFeed.length === 0}
      <EmptyState minHeight={160}>{t('ex.waitingTransactions')}</EmptyState>
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
                <span style="color:var(--color-text-2);">{t('ex.mining')}</span>
                <span class="mono" style="color:var(--cyan);font-weight:700;">+{tx.amount.toFixed(4)}</span>
                <span class="dim">→</span>
                <span class="mono" style="font-size:11px;color:var(--color-text-2);">{tx.to}</span>
              {:else if tx.type === 'transfer'}
                <span class="mono" style="font-size:11px;color:var(--color-text-2);">{tx.from}</span>
                <span class="dim">→</span>
                <span class="mono" style="font-size:11px;color:var(--color-text-2);">{tx.to}</span>
                <span class="mono" style="color:#8b5cf6;font-weight:700;">{tx.amount.toFixed(4)}</span>
              {:else if tx.type === 'burn'}
                <span style="color:var(--color-text-2);">{t('ex.burned')}</span>
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

</div>
