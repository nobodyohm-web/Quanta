<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Identicon from "./Identicon.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { t } from "./i18n.svelte";
  import { parsePaymentUri, isAddress, shortAddr } from "./quanta";

  let query = $state("");
  let searched = $state<null | { key: string; name: string | null; balance: number }>(null);
  let searchErr = $state("");
  let searching = $state(false);
  let copiedKey = $state(false);
  let liveFeed = $state<any[]>([]);

  async function refreshFeed() {
    try {
      const txs = await invoke<any[]>("get_recent_txs");
      if (txs && txs.length > 0) {
        liveFeed = txs.slice(0, 20).map((tx) => ({
          type: tx.tx_type ?? "Transfer",
          amount: tx.amount ?? 0,
          from: tx.from ? shortAddr(tx.from) : null,
          to: tx.to ? shortAddr(tx.to) : null,
          timestamp: tx.timestamp ?? "",
        }));
      }
    } catch {}
  }

  $effect(() => {
    refreshFeed();
    const iv = setInterval(refreshFeed, 5000);
    return () => clearInterval(iv);
  });

  // La recherche comprend tout ce qu'on peut lui coller : @pseudo, adresse
  // 64-hex, ou lien de paiement quanta: — même grammaire que l'envoi.
  async function handleSearch() {
    const q = query.trim();
    if (!q) return;
    searchErr = "";
    searching = true;
    searched = null;
    try {
      const parsed = parsePaymentUri(q);
      if (!parsed) { searchErr = t("ex.searchError"); return; }
      let addr = parsed.to;
      let name: string | null = null;
      if (!isAddress(addr)) {
        const uname = addr.replace(/^@/, "");
        const resolved = await invoke<string | null>("resolve_username", { username: uname });
        if (!resolved) { searchErr = t("ex.searchError"); return; }
        addr = resolved;
        name = uname;
      } else {
        // Reverse lookup : cette adresse a-t-elle un @pseudo public ?
        try { name = await invoke<string | null>("username_of_pk", { pk: addr }); } catch {}
      }
      const bal = await invoke<number>("get_balance", { pk: addr });
      searched = { key: addr, name, balance: bal ?? 0 };
    } catch {
      searchErr = t("ex.searchError");
    } finally {
      searching = false;
    }
  }

  async function copyKey() {
    if (!searched) return;
    await navigator.clipboard?.writeText(searched.key);
    copiedKey = true;
    setTimeout(() => (copiedKey = false), 1800);
  }

  /// Les timestamps du ledger sont des chaînes RFC3339 — jamais des secondes.
  function formatAge(ts: string) {
    const ms = new Date(ts).getTime();
    if (!isFinite(ms)) return "";
    const diff = Math.floor((Date.now() - ms) / 1000);
    if (diff < 5) return t("ex.ageNow");
    if (diff < 60) return `${t("ex.agePrefix")}${diff}${t("ex.ageSecondsSuffix")}`;
    if (diff < 3600) return `${t("ex.agePrefix")}${Math.floor(diff / 60)}${t("ex.ageMinutesSuffix")}`;
    if (diff < 86400) return `${t("ex.agePrefix")}${Math.floor(diff / 3600)}${t("ex.ageHoursSuffix")}`;
    return `${t("ex.agePrefix")}${Math.floor(diff / 86400)}${t("ex.ageDaysSuffix")}`;
  }

  const FEED: Record<string, { bg: string; color: string; icon: string }> = {
    Mining:   { bg: "var(--cyan-dim)",            color: "var(--color-accent)", icon: "⚡" },
    Transfer: { bg: "rgba(61,111,224,0.10)",      color: "#3D6FE0",             icon: "→" },
    Burn:     { bg: "rgba(232,129,12,0.10)",      color: "var(--color-amber)",  icon: "▽" },
    Stake:    { bg: "rgba(124,58,237,0.10)",      color: "#7c3aed",             icon: "⚿" },
    Unstake:  { bg: "rgba(124,58,237,0.08)",      color: "#9d7be8",             icon: "⌛" },
    Slash:    { bg: "rgba(229,72,77,0.10)",       color: "var(--color-red)",    icon: "⚔" },
  };
</script>

<div class="page">
  <div class="page-header">
    <div>
      <div class="page-title">{t('ex.title')}</div>
      <div class="page-sub">{t('ex.subtitle')}</div>
    </div>
  </div>

  <!-- Recherche — @pseudo · adresse · lien quanta: -->
  <div class="ex-search">
    <input
      class="input mono"
      placeholder={t('ex.searchPlaceholder')}
      bind:value={query}
      onkeydown={(e) => e.key === 'Enter' && handleSearch()}
    />
    <button class="btn btn-primary" onclick={handleSearch} disabled={searching}>
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/></svg>
      {searching ? '…' : t('ex.searchButton')}
    </button>
  </div>

  {#if searchErr}
    <div class="ex-err">{searchErr}</div>
  {/if}

  <!-- Wallet trouvé (données réelles du ledger) -->
  {#if searched}
    <div class="card ex-hit">
      <div class="ex-hit-head">
        <Identicon pubkey={searched.key} size={52} />
        <div style="flex:1;min-width:0;">
          {#if searched.name}
            <div class="ex-hit-name">@{searched.name}</div>
          {/if}
          <div class="ex-hit-k">{t('ex.walletPublic')}</div>
          <button class="copy-btn" onclick={copyKey}>
            {copiedKey ? t('ct.copied') : shortAddr(searched.key)}
          </button>
        </div>
        <button class="ex-close" aria-label={t('help.close_aria')} onclick={() => searched = null}>
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 3l10 10M13 3L3 13"/></svg>
        </button>
      </div>
      <div>
        <div class="stat-label">{t('ex.balance')}</div>
        <div class="stat-val mono">{searched.balance.toFixed(6)}<span class="ex-unit">QUANTA</span></div>
        <div class="stat-sub">{t('wallet.availableSub')}</div>
      </div>
    </div>
  {/if}

  <!-- Flux en direct (transactions réelles) -->
  <div class="card">
    <div class="ex-feed-head">
      <div class="card-title" style="margin-bottom:0;">{t('ex.liveTransactions')}</div>
      <div class="pulse-dot" style="margin-left:4px;"></div>
      <span class="ex-feed-live">{t('ex.realNetworkData')}</span>
    </div>
    {#if liveFeed.length === 0}
      <EmptyState minHeight={160}>{t('ex.waitingTransactions')}</EmptyState>
    {:else}
      {#each liveFeed as tx, i}
        {@const fd = FEED[tx.type] ?? FEED.Transfer}
        <div class="feed-item" style={i === 0 ? 'animation:fadein 0.3s ease;' : ''}>
          <div class="feed-type-badge" style="background:{fd.bg};color:{fd.color};font-size:14px;">
            {fd.icon}
          </div>
          <div style="flex:1;min-width:0;">
            <div class="ex-feed-line">
              {#if tx.type === 'Mining'}
                <span style="color:var(--color-text-2);">{t('ex.mining')}</span>
                <span class="mono" style="color:var(--cyan);font-weight:700;">+{tx.amount.toFixed(4)}</span>
                <span class="dim">→</span>
                <span class="mono ex-feed-addr">{tx.to}</span>
              {:else if tx.type === 'Burn'}
                <span style="color:var(--color-text-2);">{t('ex.burned')}</span>
                <span class="mono" style="color:var(--color-amber);font-weight:700;">{tx.amount.toFixed(4)}</span>
                <span class="dim">QNT</span>
              {:else if tx.type === 'Stake' || tx.type === 'Unstake' || tx.type === 'Slash'}
                <span class="tag {tx.type === 'Stake' ? 'ex-tag-stake' : tx.type === 'Slash' ? 'ex-tag-slash' : 'ex-tag-unstake'}">{t(('tx.' + tx.type) as any)}</span>
                <span class="mono" style="color:{fd.color};font-weight:700;">{tx.amount.toFixed(4)}</span>
                <span class="dim">·</span>
                <span class="mono ex-feed-addr">{tx.from}</span>
              {:else}
                <span class="mono ex-feed-addr">{tx.from}</span>
                <span class="dim">→</span>
                <span class="mono ex-feed-addr">{tx.to}</span>
                <span class="mono" style="color:#3D6FE0;font-weight:700;">{tx.amount.toFixed(4)}</span>
              {/if}
            </div>
          </div>
          <div class="ex-feed-age">{formatAge(tx.timestamp)}</div>
        </div>
      {/each}
    {/if}
  </div>

</div>

<style>
  .ex-search { display: flex; gap: 10px; margin-bottom: 20px; }
  /* Variante « proéminente » du .input global (taille seulement — le reste
     vient du vocabulaire partagé, focus ring inclus). */
  .ex-search .input { flex: 1; font-size: 15px; padding: 13px 16px; }
  .ex-err {
    font-size: 13px; color: var(--color-red);
    margin-bottom: 16px; padding: 12px;
    background: rgba(229,72,77,0.06); border-radius: 8px;
  }
  /* Résultat = carte blanche élevée (ombre du système, pas de bordure teal). */
  .ex-hit { margin-bottom: 20px; box-shadow: var(--shadow); }
  .ex-hit-head { display: flex; gap: 16px; align-items: center; margin-bottom: 16px; }
  .ex-hit-name { font-size: 17px; font-weight: 700; color: var(--color-accent); margin-bottom: 2px; }
  .ex-hit-k { font-size: 12px; color: var(--color-text-2); margin-bottom: 6px; }
  .ex-close {
    background: none; border: none; cursor: pointer; color: var(--color-text-3);
    padding: 6px; border-radius: 8px;
    transition: background var(--dur-fast) var(--ease-out), color var(--dur-fast) var(--ease-out);
  }
  .ex-close:hover { color: var(--color-text-1); background: var(--color-bg-2); }
  .ex-unit { font-size: 14px; color: var(--color-text-2); margin-left: 8px; }
  /* Tags distincts Stake / Unstake / Slash (base .tag globale + teinte locale ;
     mêmes familles que les pastilles du flux). */
  .ex-tag-stake   { background: rgba(124,58,237,0.10); color: #7c3aed; }
  .ex-tag-unstake { background: rgba(124,58,237,0.07); color: #9d7be8; }
  .ex-tag-slash   { background: rgba(229,72,77,0.10);  color: var(--color-red); }
  .ex-feed-head { display: flex; align-items: center; gap: 10px; margin-bottom: 16px; }
  .ex-feed-live { font-size: 11px; color: var(--color-text-3); }
  .ex-feed-line { font-size: 13px; font-weight: 500; display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .ex-feed-addr { font-size: 11px; color: var(--color-text-2); }
  .ex-feed-age { font-size: 11px; color: var(--color-text-3); flex-shrink: 0; }
</style>
