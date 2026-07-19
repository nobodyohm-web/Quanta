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
    <div class="ex-err" role="alert">{searchErr}</div>
  {/if}

  <!-- Wallet trouvé (données réelles du ledger) -->
  {#if searched}
    <div class="card ex-hit">
      <div class="ex-hit-head">
        <Identicon pubkey={searched.key} size={48} />
        <div class="ex-hit-id">
          {#if searched.name}
            <div class="ex-hit-name">@{searched.name}</div>
          {/if}
          <div class="section-label ex-sl">{t('ex.walletPublic')}</div>
          <button class="copy-btn" onclick={copyKey}>
            {copiedKey ? t('ct.copied') : shortAddr(searched.key)}
          </button>
        </div>
        <button class="ex-close" aria-label={t('help.close_aria')} onclick={() => searched = null}>
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 3l10 10M13 3L3 13"/></svg>
        </button>
      </div>
      <div class="ex-hit-bal">
        <div class="section-label">{t('ex.balance')}</div>
        <div class="ex-bal">{searched.balance.toFixed(6)}<span class="ex-bal-u">QUANTA</span></div>
        <div class="ex-hit-sub">{t('wallet.availableSub')}</div>
      </div>
    </div>
  {/if}

  <!-- Flux en direct (transactions réelles) -->
  <div class="card">
    <div class="ex-feed-head">
      <div class="card-title" style="margin-bottom:0;">{t('ex.liveTransactions')}</div>
      <div class="pulse-dot"></div>
      <span class="ex-live">{t('ex.realNetworkData')}</span>
    </div>
    {#if liveFeed.length === 0}
      <EmptyState minHeight={160}>{t('ex.waitingTransactions')}</EmptyState>
    {:else}
      {#each liveFeed as tx, i}
        {@const fd = FEED[tx.type] ?? FEED.Transfer}
        <div class="feed-item ex-item" style={i === 0 ? 'animation:fadein 0.3s ease;' : ''}>
          <div class="ex-badge">{fd.icon}</div>
          <div class="ex-row">
            {#if tx.type === 'Mining'}
              <div class="ex-desc">
                <span class="ex-title">{t('ex.mining')}</span>
                <span class="ex-meta mono">{tx.to}</span>
              </div>
              <span class="ex-amount">+{tx.amount.toFixed(4)}</span>
            {:else if tx.type === 'Burn'}
              <div class="ex-desc">
                <span class="ex-title">{t('ex.burned')}</span>
                <span class="ex-meta">QUANTA</span>
              </div>
              <span class="ex-amount">{tx.amount.toFixed(4)}</span>
            {:else if tx.type === 'Stake' || tx.type === 'Unstake' || tx.type === 'Slash'}
              <div class="ex-desc">
                <span class="ex-title">{t(('tx.' + tx.type) as any)}</span>
                <span class="ex-meta mono">{tx.from}</span>
              </div>
              <span class="ex-amount">{tx.amount.toFixed(4)}</span>
            {:else}
              <div class="ex-xfer">
                <span class="mono">{tx.from}</span>
                <span class="ex-arrow">→</span>
                <span class="mono">{tx.to}</span>
              </div>
              <span class="ex-amount">{tx.amount.toFixed(4)}</span>
            {/if}
          </div>
          <span class="ex-age">{formatAge(tx.timestamp)}</span>
        </div>
      {/each}
    {/if}
  </div>

</div>

<style>
  /* ── Recherche ── @pseudo · adresse · lien quanta: */
  .ex-search { display: flex; gap: var(--space-2); margin-bottom: var(--space-6); }
  /* Variante « proéminente » du .input global (taille seulement — le reste
     vient du vocabulaire partagé, focus ring inclus). */
  .ex-search .input { flex: 1; font-size: 15px; padding: 13px 16px; }

  /* Erreur = seul emploi autorisé du rouge sémantique (vraie erreur). */
  .ex-err {
    font-size: 13px; color: var(--color-red);
    margin-bottom: var(--space-4); padding: 12px 14px;
    background: rgba(229,72,77,0.06);
    border: 1px solid rgba(229,72,77,0.16);
    border-radius: var(--radius-sm);
  }

  /* ── Résultat = carte-compte blanche (encre + typo, zéro couleur déco) ── */
  .ex-hit { margin-bottom: var(--space-6); }
  .ex-hit-head {
    display: flex; gap: var(--space-4); align-items: center;
    padding-bottom: var(--space-5); margin-bottom: var(--space-5);
    border-bottom: 1px solid var(--color-border);
  }
  .ex-hit-id { flex: 1; min-width: 0; }
  .ex-hit-name {
    font-size: 18px; font-weight: 700; color: var(--color-text-0);
    letter-spacing: -0.01em; margin-bottom: 8px;
  }
  .ex-sl { margin-bottom: 6px; }
  .ex-close {
    background: none; border: none; cursor: pointer; color: var(--color-text-3);
    padding: 6px; border-radius: var(--radius-sm); flex-shrink: 0;
    transition: background var(--dur-fast) var(--ease-out), color var(--dur-fast) var(--ease-out);
  }
  .ex-close:hover { color: var(--color-text-1); background: var(--color-bg-2); }

  /* Le chiffre est le héros : gros, confiant, tabulaire, en encre. */
  .ex-bal {
    font-family: var(--font-display);
    font-size: 40px; font-weight: 700; line-height: 1;
    letter-spacing: -0.02em; color: var(--color-text-0);
    font-variant-numeric: tabular-nums lining-nums;
    font-feature-settings: 'tnum', 'zero';
  }
  .ex-bal-u {
    font-size: 15px; font-weight: 600; color: var(--color-text-2);
    margin-left: 10px; letter-spacing: 0.02em;
  }
  .ex-hit-sub { font-size: 12px; color: var(--color-text-2); margin-top: 8px; }

  /* ── Flux en direct ── */
  .ex-feed-head { display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-4); }
  /* Le seul point vivant : pastille teal (pas de vert déco sur le chrome). */
  .ex-feed-head .pulse-dot { background: var(--color-accent); }
  .ex-live { font-size: 11px; color: var(--color-text-3); }

  .ex-item { gap: var(--space-3); }
  /* Pastille de type neutre — glyphe encre sur gris, aucune teinte de couleur. */
  .ex-badge {
    width: 32px; height: 32px; border-radius: var(--radius-sm);
    display: flex; align-items: center; justify-content: center;
    background: var(--color-bg-2); color: var(--color-text-1);
    font-size: 14px; flex-shrink: 0;
  }
  .ex-row {
    flex: 1; min-width: 0; display: flex; align-items: center;
    justify-content: space-between; gap: var(--space-3);
  }
  .ex-desc { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .ex-title {
    font-size: 13px; font-weight: 600; color: var(--color-text-0); line-height: 1.2;
  }
  .ex-meta {
    font-size: 11px; color: var(--color-text-2); line-height: 1.2;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .ex-xfer {
    display: flex; align-items: center; gap: 6px; min-width: 0;
    font-size: 12px; color: var(--color-text-1);
  }
  .ex-xfer .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ex-arrow { color: var(--color-text-3); flex-shrink: 0; }
  .ex-amount {
    font-size: 14px; font-weight: 600; color: var(--color-text-0);
    font-variant-numeric: tabular-nums lining-nums;
    font-feature-settings: 'tnum', 'zero';
    flex-shrink: 0; white-space: nowrap;
  }
  .ex-age {
    font-size: 11px; color: var(--color-text-3); flex-shrink: 0;
    min-width: 56px; text-align: right;
    font-variant-numeric: tabular-nums lining-nums;
  }
</style>
