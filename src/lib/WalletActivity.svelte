<script lang="ts">
  import EmptyState from "./EmptyState.svelte";
  import { t, type TKey } from "./i18n.svelte";
  import { shortAddr } from "./quanta";
  import { type LedgerTx } from "./api";
  import { walletOverview as walletStore, recentTxs as recentTxsStore } from "./stores.svelte";

  type Filter = "all" | "out" | "in" | "mining" | "stakeOps" | "burn";
  const PAGE_SIZE = 10;

  // ── Données : stores partagés (chauds entre navigations). walletOverview donne
  //    l'adresse on-chain (direction des tx) + le latch `loaded` (skeletons). ──
  $effect(() => walletStore.subscribe());
  $effect(() => recentTxsStore.subscribe());

  const txs = $derived(recentTxsStore.value ?? []);
  const myPk = $derived(walletStore.value?.address ?? "");
  const loading = $derived(!walletStore.loaded);

  let filter = $state<Filter>("all");
  let page = $state(0);
  function setFilter(f: Filter) { filter = f; page = 0; }

  function timeAgo(ts: string): string {
    const diff = Math.floor((Date.now() - new Date(ts).getTime()) / 1000);
    if (!isFinite(diff) || diff < 0) return "";
    if (diff < 60) return t("time.now");
    if (diff < 3600) return Math.floor(diff / 60) + " " + t("time.min");
    if (diff < 86400) return Math.floor(diff / 3600) + " " + t("time.h");
    return Math.floor(diff / 86400) + " " + t("time.d");
  }

  const TX_KNOWN: Record<string, true> = { Transfer: true, Mining: true, Burn: true, Stake: true, Unstake: true, Slash: true };
  function txLabel(type: string): string {
    return TX_KNOWN[type] ? t(("tx." + type) as TKey) : type;
  }

  function isIncoming(tx: LedgerTx): boolean {
    return tx.to === myPk && tx.from !== myPk;
  }
  function isOutgoing(tx: LedgerTx): boolean {
    return tx.from === myPk && tx.to !== myPk;
  }

  /// Burn implicite d'un transfert sortant (le montant affiché est le NET 99 %).
  function impliedBurn(tx: LedgerTx): number | null {
    if (tx.tx_type !== "Transfer" || !isOutgoing(tx)) return null;
    return tx.amount / 99;
  }

  // ── Feed d'activité : mouvements réels + minage agrégé par jour ──
  type FeedRow =
    | { kind: "tx"; tx: LedgerTx }
    | { kind: "mine"; key: string; label: string; sum: number; count: number; ts: string };

  function dayLabel(ts: string): string {
    const d = new Date(ts);
    if (!isFinite(d.getTime())) return t("wallet.mining");
    const today = new Date();
    const yest = new Date(); yest.setDate(today.getDate() - 1);
    const same = (a: Date, b: Date) => a.toDateString() === b.toDateString();
    if (same(d, today)) return t("time.today");
    if (same(d, yest)) return t("time.yesterday");
    return d.toLocaleDateString(undefined, { day: "numeric", month: "long" });
  }

  let miningDaily = $derived.by((): FeedRow[] => {
    const map = new Map<string, { sum: number; count: number; ts: string }>();
    for (const tx of txs) {
      if (tx.tx_type !== "Mining") continue;
      const d = new Date(tx.timestamp);
      const key = isFinite(d.getTime()) ? d.toDateString() : "—";
      const a = map.get(key) ?? { sum: 0, count: 0, ts: tx.timestamp };
      a.sum += tx.amount; a.count += 1;
      if (new Date(tx.timestamp).getTime() > new Date(a.ts).getTime()) a.ts = tx.timestamp;
      map.set(key, a);
    }
    return [...map.entries()]
      .sort((x, y) => new Date(y[1].ts).getTime() - new Date(x[1].ts).getTime())
      .map(([key, a]) => ({ kind: "mine" as const, key, label: dayLabel(a.ts), sum: a.sum, count: a.count, ts: a.ts }));
  });

  const STAKE_TYPES: Record<string, true> = { Stake: true, Unstake: true, Slash: true };
  let feed = $derived.by((): FeedRow[] => {
    if (filter === "mining") return miningDaily;
    const pass = (tx: LedgerTx) => {
      switch (filter) {
        case "all": return tx.tx_type !== "Mining";
        case "out": return tx.tx_type === "Transfer" && isOutgoing(tx);
        case "in": return tx.tx_type === "Transfer" && isIncoming(tx);
        case "stakeOps": return !!STAKE_TYPES[tx.tx_type];
        case "burn": return tx.tx_type === "Burn";
        default: return false;
      }
    };
    return txs.filter(pass).map((tx) => ({ kind: "tx" as const, tx }));
  });

  let totalPages = $derived(Math.max(1, Math.ceil(feed.length / PAGE_SIZE)));
  let safePage = $derived(Math.min(page, totalPages - 1));
  let pageItems = $derived(feed.slice(safePage * PAGE_SIZE, (safePage + 1) * PAGE_SIZE));

  /// Sous-titre d'une ligne de tx suivant son type (langage simple).
  function txSub(tx: LedgerTx): string {
    switch (tx.tx_type) {
      case "Stake": return t("wallet.tx.stakeSub");
      case "Unstake": return t("wallet.tx.unstakeSub");
      case "Slash": return t("wallet.tx.slashSub");
      case "Burn": return t("wallet.tx.burnSub");
      default:
        return isIncoming(tx) ? `${t("wallet.tx.from")} ${shortAddr(tx.from)}` : `→ ${shortAddr(tx.to)}`;
    }
  }
</script>

<div class="w-section">
  <div class="section-label">{t('wallet.activity')}</div>

  <div class="card">
  <div class="filter-tabs w-filters" role="tablist" aria-label={t('wallet.activity')}>
    <button class="filter-tab" class:active={filter === "all"} onclick={() => setFilter("all")} role="tab" aria-selected={filter === "all"}>{t('wallet.f.all')}</button>
    <button class="filter-tab" class:active={filter === "out"} onclick={() => setFilter("out")} role="tab" aria-selected={filter === "out"}>{t('wallet.f.out')}</button>
    <button class="filter-tab" class:active={filter === "in"} onclick={() => setFilter("in")} role="tab" aria-selected={filter === "in"}>{t('wallet.f.in')}</button>
    <button class="filter-tab" class:active={filter === "mining"} onclick={() => setFilter("mining")} role="tab" aria-selected={filter === "mining"}>{t('wallet.f.mining')}</button>
    <button class="filter-tab" class:active={filter === "stakeOps"} onclick={() => setFilter("stakeOps")} role="tab" aria-selected={filter === "stakeOps"}>{t('wallet.f.stakeOps')}</button>
    <button class="filter-tab" class:active={filter === "burn"} onclick={() => setFilter("burn")} role="tab" aria-selected={filter === "burn"}>{t('wallet.f.burn')}</button>
  </div>

  <div class="w-tx-list">
    {#if loading}
      <div class="skeleton sk-row"></div>
      <div class="skeleton sk-row"></div>
      <div class="skeleton sk-row"></div>
    {:else if feed.length === 0}
      <EmptyState>
        {#if filter === "all"}{t('wallet.empty.all')}
        {:else if filter === "mining"}{t('wallet.empty.mining')}
        {:else}{t('wallet.empty.other')}{/if}
      </EmptyState>
    {:else}
      {#each pageItems as row (row.kind === "tx" ? row.tx.id : row.key)}
        {#if row.kind === "mine"}
          <div class="w-tx-row">
            <div class="tx-icon w-ic-mine" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M13 3L5 13h5l-1 8 8-10h-5l1-8z"/></svg>
            </div>
            <div class="w-tx-left">
              <span class="w-tx-label">{t('wallet.mining')} · {row.label}</span>
              <span class="w-tx-sub">{row.count} {row.count > 1 ? t('wallet.rewards') : t('wallet.reward')} · {t('wallet.auto')}</span>
            </div>
            <div class="w-tx-right">
              <span class="w-tx-amt mono tx-in">+{row.sum.toFixed(2)}</span>
              <span class="w-tx-time">{timeAgo(row.ts)}</span>
            </div>
          </div>
        {:else}
          {@const tx = row.tx}
          {@const inc = isIncoming(tx)}
          {@const burn = impliedBurn(tx)}
          {@const isSlash = tx.tx_type === "Slash"}
          {@const isUnstake = tx.tx_type === "Unstake"}
          <div class="w-tx-row">
            <div class="tx-icon"
              class:w-ic-slash={isSlash}
              class:w-ic-stake={!isSlash && (tx.tx_type === "Stake" || isUnstake)}
              class:w-ic-burn={tx.tx_type === "Burn"}
              class:w-ic-in={tx.tx_type === "Transfer" && inc}
              class:w-ic-out={!isSlash && !isUnstake && tx.tx_type !== "Stake" && tx.tx_type !== "Burn" && !(tx.tx_type === "Transfer" && inc)}
              aria-hidden="true">
              {#if isSlash}
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4L3 19h18L12 4z"/><path d="M12 11v3M12 16.5h.01"/></svg>
              {:else if tx.tx_type === "Stake"}
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 018 0v3"/></svg>
              {:else if isUnstake}
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 017.6-1.7"/></svg>
              {:else if tx.tx_type === "Burn"}
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4c2.5 3-4 5-4 9a4 4 0 008 0c0-4-6.5-6-4-9z"/></svg>
              {:else if inc}
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M17 7L7 17M7 9v8h8"/></svg>
              {:else}
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M7 17L17 7M9 7h8v8"/></svg>
              {/if}
            </div>
            <div class="w-tx-left">
              <span class="w-tx-label" class:tx-slash={isSlash}>{txLabel(tx.tx_type)}</span>
              <span class="w-tx-sub mono">{txSub(tx)}</span>
            </div>
            <div class="w-tx-right">
              <span class="w-tx-amt mono"
                class:tx-in={inc && !isSlash}
                class:tx-out={!inc && !isSlash && !isUnstake}
                class:tx-slash={isSlash}
                class:tx-neutral={isUnstake}>
                {isSlash ? "−" : isUnstake ? "" : inc ? "+" : "−"}{tx.amount.toFixed(2)}
              </span>
              {#if burn !== null}
                <span class="w-tx-burn mono">−{burn.toFixed(2)} {t('wallet.burned')}</span>
              {:else}
                <span class="w-tx-time">{timeAgo(tx.timestamp)}</span>
              {/if}
            </div>
          </div>
        {/if}
      {/each}

      {#if totalPages > 1}
        <div class="w-pager">
          <button class="btn btn-ghost btn-sm"
            onclick={() => page = Math.max(0, safePage - 1)}
            disabled={safePage === 0}
            aria-label={t('wallet.prevAria')}>
            {t('wallet.prev')}
          </button>
          <span class="w-pager-info mono">{safePage + 1} / {totalPages}</span>
          <button class="btn btn-ghost btn-sm"
            onclick={() => page = Math.min(totalPages - 1, safePage + 1)}
            disabled={safePage >= totalPages - 1}
            aria-label={t('wallet.nextAria')}>
            {t('wallet.next')}
          </button>
        </div>
      {/if}
    {/if}
  </div>
  </div>
</div>

<style>
  /* Skeletons — base (shimmer discret sur gris chaud) + taille de ligne */
  .skeleton {
    background: linear-gradient(90deg, var(--color-bg-2) 25%, var(--color-bg-3) 50%, var(--color-bg-2) 75%);
    background-size: 200% 100%;
    animation: sk-shimmer 1.4s ease infinite;
  }
  @keyframes sk-shimmer { from { background-position: 200% 0; } to { background-position: -200% 0; } }
  @media (prefers-reduced-motion: reduce) { .skeleton { animation: none; } }
  .sk-row  { width: 100%; height: 44px; border-radius: var(--radius-sm); margin-bottom: 6px; }

  /* Sections — le canevas respire entre les cartes */
  .w-section { margin: 20px 0 var(--space-3); }

  /* Transactions — lignes aérées sur carte blanche, hairlines internes */
  .w-tx-row {
    display: flex; align-items: center; gap: var(--space-3);
    padding: 13px 0; border-bottom: 1px solid var(--color-border);
  }
  .w-tx-row:last-child { border-bottom: none; }
  .w-tx-left  { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .w-tx-right { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; margin-left: auto; }
  .w-tx-label { font-size: var(--text-base); font-weight: 500; color: var(--color-text-0); }
  .w-tx-sub   { font-size: var(--text-sm); color: var(--color-text-2); }
  .w-tx-time  { font-size: var(--text-sm); color: var(--color-text-2); }
  .w-tx-amt   { font-size: var(--text-base); font-weight: 600; }
  .tx-in      { color: var(--cyan); }
  .tx-out     { color: var(--color-text-0); }
  .tx-neutral { color: var(--color-text-2); }
  .tx-slash   { color: var(--color-text-0); }

  /* Icônes de ligne — teal entrant, encre sortant, rouge sobre pour Slash */
  .w-ic-in    { background: var(--cyan-dim); color: var(--cyan); }
  .w-ic-out   { background: var(--color-bg-3); color: var(--color-text-1); }
  .w-ic-mine  { background: var(--cyan-dim); color: var(--cyan); }
  .w-ic-stake { background: var(--cyan-dim); color: var(--teal-700); }
  .w-ic-burn  { background: var(--color-bg-3); color: var(--color-text-1); }
  .w-ic-slash { background: var(--color-text-0); color: #fff; }

  /* Filtres — vocabulaire global .filter-tabs/.filter-tab ; seul le wrap est local */
  .w-filters { flex-wrap: wrap; margin-bottom: var(--space-3); }

  .w-tx-burn { font-size: var(--text-xs); color: var(--color-text-2); font-weight: 400; }

  /* Pagination — boutons .btn-ghost globaux ; seul l'agencement est local */
  .w-pager {
    display: flex; align-items: center; justify-content: center;
    gap: var(--space-4);
    padding: var(--space-4) 0 0;
    border-top: 1px solid var(--color-border);
    margin-top: var(--space-2);
  }
  .w-pager-info { font-size: var(--text-base); color: var(--color-text-1); min-width: 48px; text-align: center; }
</style>
