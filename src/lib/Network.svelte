<script lang="ts">
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import ChainHistory from "./ChainHistory.svelte";
  import NetworkScene3D from "./NetworkScene3D.svelte";
  import { t } from "./i18n.svelte";
  import { FEEDBACK_OK_MS, FEEDBACK_COPY_MS } from "./quanta";
  import { connectPeer as apiConnectPeer } from "./api";
  import { nodeStatus, chainOverview, finalityStatus, peerMetrics as peerMetricsStore } from "./stores.svelte";

  let chainView = $state<"history" | "2d">("history");

  // ── Statut du nœud · chaîne vive · finalité · métriques par pair : stores
  //    partagés (UN sondage par donnée). Le Wallet et le Minage partagent
  //    nodeStatus ; le Minage partage chaîne + finalité. Plus d'interval local. ──
  $effect(() => nodeStatus.subscribe());
  $effect(() => chainOverview.subscribe());
  $effect(() => finalityStatus.subscribe());
  $effect(() => peerMetricsStore.subscribe());

  const peerCount = $derived(nodeStatus.value?.peer_count ?? 0);
  const myPeerId = $derived(nodeStatus.value?.peer_id ?? "");
  const isOnline = $derived(nodeStatus.value?.is_online ?? false);
  const protocol = $derived(nodeStatus.value?.protocol ?? "");

  // ─── Blockchain en direct ──────────────────────────────────────
  const chainHeight = $derived(chainOverview.value?.height ?? 0);
  const pendingTx = $derived(chainOverview.value?.pending ?? 0);
  const blocks = $derived(chainOverview.value?.blocks ?? []);
  const finalityFloor = $derived(finalityStatus.value?.finalized_floor ?? 0);
  const peerMetrics = $derived(peerMetricsStore.value ?? []);

  // États de chargement — tant que !loaded, le résumé affiche « — » (pas 0) ;
  // loadError s'allume dès qu'un sondage échoue et s'éteint au succès suivant.
  const loaded = $derived(nodeStatus.loaded && chainOverview.loaded && finalityStatus.loaded);
  const loadError = $derived(nodeStatus.error || chainOverview.error || finalityStatus.error);

  let copied = $state(false);

  // Halo « flash » au nouveau bloc : détecté sur la HAUSSE de hauteur, avec sa
  // minuterie propre (l'ancienne expression ne se réévaluait qu'au bloc suivant
  // → halo bloqué ~2 min ; ici il retombe à 1,6 s).
  let flashOn = $state(false);
  let prevHeight = 0;
  let flashTo: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const h = chainHeight;
    if (prevHeight && h > prevHeight) {
      flashOn = true;
      clearTimeout(flashTo);
      flashTo = setTimeout(() => (flashOn = false), 1600);
    }
    prevHeight = h;
  });

  let connectInput = $state("");
  let connectErr = $state("");
  let connectSuccess = $state(false);
  let connecting = $state(false);

  // NET-16: Chain-sync progress event payload + freshness gate
  type SyncProgress = {
    our_height: number;
    sender_height: number;
    integrated: number;
    rejected: number;
    sender: string;
  };
  let syncProgress = $state<SyncProgress | null>(null);
  let syncProgressAt = $state(0); // ms epoch — used to fade banner

  // NET-16: subscribe to chain-sync progress events from the backend.
  $effect(() => {
    let alive = true;
    let unlisten: UnlistenFn | null = null;
    listen<SyncProgress>("quanta://chain-sync-progress", (e) => {
      syncProgress = e.payload;
      syncProgressAt = Date.now();
    }).then((fn) => { if (!alive) fn(); else unlisten = fn; }).catch(() => {});
    // garde `alive` : si on quitte l'écran avant que listen() résolve, on retire
    // quand même le listener (sinon fuite qui s'accumule à chaque aller-retour).
    return () => { alive = false; if (unlisten) unlisten(); };
  });

  // Hide the banner once sync caught up AND the last event is older than 8s.
  let showSyncBanner = $derived.by(() => {
    if (!syncProgress) return false;
    const stale = Date.now() - syncProgressAt > 8000;
    const caught = syncProgress.our_height >= syncProgress.sender_height;
    return !(stale && caught);
  });
  let syncPercent = $derived.by(() => {
    if (!syncProgress || syncProgress.sender_height === 0) return 0;
    const pct = (syncProgress.our_height / syncProgress.sender_height) * 100;
    return Math.max(0, Math.min(100, pct));
  });

  function copyId() {
    navigator.clipboard?.writeText(myPeerId);
    copied = true;
    setTimeout(() => copied = false, FEEDBACK_COPY_MS);
  }

  async function connectPeer() {
    connectErr = "";
    connectSuccess = false;
    if (!connectInput.trim()) { connectErr = t('net.peerIdRequired'); return; }
    connecting = true;
    try {
      await apiConnectPeer(connectInput.trim());
      connectInput = "";
      connectSuccess = true;
      setTimeout(() => connectSuccess = false, FEEDBACK_OK_MS);
      nodeStatus.refresh();
    } catch (e) {
      connectErr = String(e);
    }
    connecting = false;
  }

  // Presentation only — quality score color class (NET-10: >=80 good, 50-79 mid, <50 low).
  function qualityCls(q: number) {
    return q >= 80 ? 'q-good' : q >= 50 ? 'q-mid' : 'q-low';
  }
</script>

<div class="page">
  <!-- En-tête ─────────────────────────────────────────────── -->
  <div class="page-header">
    <div>
      <div class="page-title">{t('network.title')}</div>
      <div class="page-sub">{t('network.sub')} — {protocol || t('loading')}</div>
    </div>
    <div class="net-status">
      <span class="status-dot" class:online={isOnline}></span>
      <span class="net-status-txt">{peerCount} {peerCount !== 1 ? t('wallet.peers') : t('wallet.peer')} {t('network.connectedAdj')}</span>
    </div>
  </div>

  <!-- Résumé réseau — chiffres réels du nœud, zéro imagerie -->
  <div class="net-summary-wrap">
    <div class="grid-4 net-summary">
      <div class="card">
        <div class="stat-label">{t('db.peers')}</div>
        <div class="stat-val sm">{loaded ? peerCount : '—'}</div>
      </div>
      <div class="card">
        <div class="stat-label">{t('db.height')}</div>
        <div class="stat-val sm">{loaded ? chainHeight : '—'}</div>
        <div class="stat-sub">{t('db.blocks')}</div>
      </div>
      <div class="card">
        <div class="stat-label">{t("net.finalityFloor")}</div>
        <div class="stat-val sm">{loaded ? finalityFloor : '—'}</div>
      </div>
      <div class="card">
        <div class="stat-label">{t("net.statusLabel")}</div>
        <div class="stat-val sm">{loaded ? (isOnline ? t('wallet.connected') : t('wallet.offline')) : '—'}</div>
        <div class="stat-sub mono">{protocol || '—'}</div>
      </div>
    </div>
    {#if loadError}
      <div class="net-load-err">{t('common.errLoad')}</div>
    {/if}
  </div>

  <!-- Le réseau en 3D — chaque particule est un événement réel du nœud
       (signature, vérification, scellement, minage, snapshot) ; les sphères
       en orbite sont les pairs mesurés. WebGL2 pur, zéro dépendance. -->
  <div class="card net-live">
    <NetworkScene3D {peerCount} {blocks} {finalityFloor} />
  </div>

  <!-- Blockchain en direct -->
  <div class="card chain-wrap">
    <div class="chain-head">
      <span class="card-title chain-title">{t('net.chainTitle')}</span>
      <div class="chain-head-r">
        <span class="chain-meta">{pendingTx} {t('net.chainPendingMeta')}</span>
        <div class="filter-tabs">
          <button class="filter-tab" class:active={chainView === 'history'} onclick={() => (chainView = 'history')}>{t('net.chainViewHistory')}</button>
          <button class="filter-tab" class:active={chainView === '2d'} onclick={() => (chainView = '2d')}>{t('net.chainViewRecent')}</button>
        </div>
      </div>
    </div>
    {#if chainView === 'history'}
      <ChainHistory />
    {:else}
      <div class="chain-strip">
        <div class="chain-pending" title={t('net.chainPendingTip')}>
          <div class="chain-pending-n">{pendingTx}</div>
          <div class="chain-pending-l">{t('net.chainForging')}</div>
        </div>
        {#each blocks as b, i (b.index)}
          <div class="chain-link"></div>
          <div class="chain-block" class:chain-block-new={i === 0 && flashOn}>
            <div class="chain-block-h">#{b.index}</div>
            <div class="chain-block-mint">+{(b.minted_qta ?? 0).toFixed(3)}</div>
            <div class="chain-block-meta">{b.tx_count} {t('net.txAbbr')}</div>
            <div class="chain-block-hash mono">{(b.hash || '········').slice(0, 8)}</div>
          </div>
        {/each}
        {#if blocks.length === 0}
          <div class="chain-link"></div>
          <div class="chain-empty">{t('net.chainEmpty')}</div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- NET-16: bandeau de progression de synchronisation -->
  {#if showSyncBanner && syncProgress}
    <div class="card sync-banner">
      <div class="sync-row">
        <span class="sync-label">{t('net.syncLabel')}</span>
        <span class="sync-counts">
          {syncProgress.our_height} / {syncProgress.sender_height} {t('net.syncBlocks')}
          {#if syncProgress.integrated > 0}
            <span class="sync-delta">+{syncProgress.integrated}</span>
          {/if}
        </span>
      </div>
      <div class="sync-bar"><div class="sync-bar-fill" style="width:{syncPercent}%;"></div></div>
    </div>
  {/if}

  <!-- NET-9/10: table des pairs — hairlines, chiffres tabulaires -->
  {#if peerMetrics.length > 0}
    <div class="card peers-panel">
      <h3 class="card-title peers-panel-title">{t('net.peersHeading')} · {peerMetrics.length}</h3>
      <div class="peers-table">
        <div class="peers-head">
          <span>{t('net.colNameKey')}</span>
          <span>{t('net.colCountry')}</span>
          <span>{t('net.colRtt')}</span>
          <span>{t('net.colLoss')}</span>
          <span>{t('net.colQuality')}</span>
          <span>{t('net.colSeen')}</span>
        </div>
        {#each peerMetrics as p (p.public_key)}
          <div class="peers-row">
            <span class="peer-name" title={p.public_key}>
              <span class="peer-dot" class:alive={p.last_seen_secs_ago < 300}></span>
              <span class="mono peer-name-text">{p.display_name || (p.public_key.slice(0, 16) + '…')}</span>
            </span>
            <span class="peer-country">{p.country || '—'}</span>
            <span class="tnum">{p.smoothed_rtt_ms != null ? p.smoothed_rtt_ms + ' ms' : '—'}</span>
            <span class="tnum">{(p.loss_ratio * 100).toFixed(0)}%</span>
            <span>
              {#if p.quality_score != null}
                <span class="quality-pill {qualityCls(p.quality_score)}">{p.quality_score}</span>
              {:else}
                <span class="peer-muted">—</span>
              {/if}
            </span>
            <span class="tnum peer-seen">{p.last_seen_secs_ago}s</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Panneau de connexion -->
  <div class="card connect-panel">
    <h3 class="card-title">{t('net.connectTitle')}</h3>

    <!-- Étape 1 : votre identité -->
    <div class="connect-section">
      <div class="connect-step">
        <span class="step-num">1</span>
        <span class="step-text">{t('net.step1')}</span>
      </div>
      <div class="id-display">
        <code class="peer-id-code">{myPeerId || t('net.endpointLoading')}</code>
        {#if myPeerId}
          <button class="btn btn-ghost btn-sm" onclick={copyId}>
            {copied ? t('net.copied') : t('net.copy')}
          </button>
        {/if}
      </div>
    </div>

    <!-- Étape 2 : connexion -->
    <div class="connect-section">
      <div class="connect-step">
        <span class="step-num">2</span>
        <span class="step-text">{t('net.step2')}</span>
      </div>
      <div class="connect-field">
        <input class="input mono" placeholder={t('net.connectPlaceholder')} bind:value={connectInput}
          onkeydown={(e) => e.key === 'Enter' && connectPeer()} />
        <button class="btn btn-primary" onclick={connectPeer} disabled={connecting}>
          {connecting ? '…' : t('net.connectBtn')}
        </button>
      </div>
      {#if connectErr}
        <div class="connect-msg err">{connectErr}</div>
      {/if}
      {#if connectSuccess}
        <div class="connect-msg ok">{t('net.connectSuccess')}</div>
      {/if}
    </div>
  </div>
</div>

<style>
  /* Utilitaire : chiffres tabulaires alignés (police d'affichage Inter). */
  .tnum { font-variant-numeric: tabular-nums lining-nums; font-feature-settings: 'tnum', 'lnum'; }

  /* ── En-tête : statut réseau ─────────────────────────────── */
  .net-status { display: flex; gap: var(--space-2); align-items: center; }
  .net-status-txt { font-size: var(--text-base); color: var(--color-text-2); font-variant-numeric: tabular-nums lining-nums; }
  .status-dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--color-text-3);
    transition: background 0.3s;
  }
  /* Vert = seul point sémantique conservé : nœud réellement en ligne. */
  .status-dot.online {
    background: var(--color-green);
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--color-green) 40%, transparent);
    animation: pulse-dot 2s ease-in-out infinite;
  }
  @keyframes pulse-dot {
    0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--color-green) 35%, transparent); }
    50% { box-shadow: 0 0 0 4px transparent; }
  }

  /* ── Résumé réseau — 4 chiffres réels, aucune imagerie ───── */
  .net-summary-wrap { margin-bottom: var(--space-4); }
  .net-load-err {
    font-size: var(--text-sm); color: var(--color-text-2);
    margin: var(--space-2) 0 0 var(--space-1);
  }
  .net-live { margin-bottom: var(--space-4); padding: var(--space-2); }

  /* ── Blockchain en direct ────────────────────────────────── */
  .chain-wrap { padding: var(--space-5) var(--space-6) var(--space-6); margin-bottom: var(--space-4); }
  .chain-head { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); margin-bottom: var(--space-4); }
  .chain-head-r { display: flex; align-items: center; gap: 14px; }
  .chain-title { margin-bottom: 0; }
  .chain-meta { font-size: var(--text-sm); color: var(--color-text-2); font-variant-numeric: tabular-nums lining-nums; }
  .chain-strip { display: flex; align-items: stretch; overflow-x: auto; padding-bottom: var(--space-2); }
  .chain-strip::-webkit-scrollbar { height: 4px; }
  .chain-pending {
    flex-shrink: 0; min-width: 82px; border: 1px dashed var(--color-border-hover);
    border-radius: 12px; padding: 14px 10px; text-align: center;
    display: flex; flex-direction: column; justify-content: center; gap: 3px;
    background: var(--color-bg-1);
  }
  .chain-pending-n {
    font-family: var(--font-display);
    font-size: 22px; font-weight: 700; color: var(--color-accent);
    font-variant-numeric: tabular-nums lining-nums;
  }
  .chain-pending-l { font-size: 10px; letter-spacing: 0.04em; text-transform: uppercase; color: var(--color-text-3); }
  .chain-link { flex-shrink: 0; width: 20px; align-self: center; height: 1px; background: var(--color-border-hover); }
  .chain-block {
    flex-shrink: 0; min-width: 98px; border: 1px solid var(--color-border);
    border-radius: 12px; padding: 14px 12px; background: var(--surface);
    box-shadow: var(--shadow-sm); display: flex; flex-direction: column; gap: var(--space-1);
  }
  .chain-block-new { animation: chain-in .6s cubic-bezier(.2,.8,.2,1); border-color: var(--color-accent); box-shadow: 0 0 0 2px var(--color-accent-dim); }
  @keyframes chain-in { from { opacity: 0; transform: translateX(-18px) scale(.92); } to { opacity: 1; transform: none; } }
  .chain-block-h { font-size: var(--text-base); font-weight: 700; color: var(--color-text-0); font-variant-numeric: tabular-nums lining-nums; }
  .chain-block-mint {
    font-family: var(--font-display);
    font-size: var(--text-base); font-weight: 700; color: var(--color-text-0);
    font-variant-numeric: tabular-nums lining-nums;
  }
  .chain-block-meta { font-size: var(--text-xs); color: var(--color-text-2); font-variant-numeric: tabular-nums lining-nums; }
  .chain-block-hash { font-size: 10px; color: var(--color-text-3); }
  .chain-empty { flex-shrink: 0; padding: var(--space-4); font-size: var(--text-base); color: var(--color-text-3); align-self: center; }

  /* ── NET-16 : bandeau de synchronisation ─────────────────── */
  .sync-banner { margin-bottom: var(--space-4); }
  .sync-row { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 10px; }
  .sync-label { font-size: var(--text-base); color: var(--color-text-2); font-weight: 500; }
  .sync-counts { font-size: var(--text-sm); color: var(--color-text-1); font-variant-numeric: tabular-nums lining-nums; }
  .sync-delta {
    display: inline-block;
    margin-left: var(--space-2);
    padding: 1px 8px;
    border-radius: 100px;
    background: var(--cyan-dim);
    color: var(--cyan);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums lining-nums;
  }
  .sync-bar {
    height: 4px;
    background: var(--color-bg-3);
    border-radius: 2px;
    overflow: hidden;
  }
  .sync-bar-fill {
    height: 100%;
    background: var(--cyan);
    transition: width 0.4s ease-out;
  }

  /* ── NET-9/10 : table des pairs — hairlines seules ───────── */
  .peers-panel { margin-bottom: var(--space-4); }
  .peers-panel-title { margin-bottom: var(--space-1); }
  .peers-table { display: flex; flex-direction: column; }
  .peers-head, .peers-row {
    display: grid;
    grid-template-columns: 2fr 0.7fr 0.9fr 0.8fr 0.8fr 0.7fr;
    align-items: center;
    gap: var(--space-4);
    padding: 13px 4px;
    font-size: var(--text-base);
    border-bottom: 1px solid var(--color-border);
  }
  .peers-head {
    color: var(--color-text-3);
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    padding: 0 4px 10px;
  }
  /* Colonnes numériques alignées à droite (RTT / perte / qualité / vu). */
  .peers-head span:nth-child(n+3),
  .peers-row > span:nth-child(n+3) { text-align: right; justify-self: end; }
  .peers-row:last-child { border-bottom: none; }
  .peers-row:hover { background: var(--color-bg-1); }
  .peer-name { display: flex; align-items: center; gap: var(--space-2); min-width: 0; }
  .peer-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--color-text-3); flex-shrink: 0; }
  .peer-dot.alive { background: var(--cyan); }
  .peer-name-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-1);
    font-size: var(--text-sm);
  }
  .peer-country { color: var(--color-text-2); }
  .peer-seen { color: var(--color-text-2); }
  .peer-muted { color: var(--color-text-3); }
  /* NET-10 : qualité — teal (bon) / encre (moyen) / gris (faible), zéro arc-en-ciel. */
  .quality-pill {
    display: inline-block;
    padding: 2px 9px;
    border-radius: 100px;
    font-size: var(--text-xs);
    font-weight: 600;
    font-variant-numeric: tabular-nums lining-nums;
  }
  .quality-pill.q-good { background: var(--cyan-dim); color: var(--cyan); }
  .quality-pill.q-mid { background: var(--color-bg-3); color: var(--color-text-1); }
  .quality-pill.q-low { color: var(--color-text-3); }

  /* ── Panneau de connexion ────────────────────────────────── */
  .connect-panel { margin-bottom: var(--space-4); }
  .connect-section {
    margin-bottom: var(--space-6);
    padding-bottom: var(--space-6);
    border-bottom: 1px solid var(--color-border);
  }
  .connect-section:last-child {
    margin-bottom: 0;
    padding-bottom: 0;
    border-bottom: none;
  }
  .connect-step {
    display: flex; align-items: center; gap: 10px;
    margin-bottom: 14px;
  }
  .step-num {
    width: 22px; height: 22px; min-width: 22px;
    border-radius: 50%;
    border: 1px solid var(--color-border-hover);
    color: var(--color-text-2);
    display: flex; align-items: center; justify-content: center;
    font-size: var(--text-xs); font-weight: 700;
    font-variant-numeric: tabular-nums lining-nums;
  }
  .step-text {
    font-size: var(--text-base); font-weight: 500;
    color: var(--color-text-1);
  }
  .id-display {
    display: flex; align-items: center; gap: var(--space-3);
    padding: 14px 16px;
    background: var(--color-bg-1);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border);
  }
  .peer-id-code {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--color-text-0);
    word-break: break-all;
    flex: 1;
    line-height: 1.6;
    user-select: all;
  }
  .connect-field { display: flex; gap: 10px; }
  .connect-field .input { flex: 1; min-width: 0; }
  .connect-msg {
    font-size: var(--text-sm);
    margin-top: 10px;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
  }
  /* Rouge = seul emploi sémantique conservé : erreur réelle de connexion. */
  .connect-msg.err {
    color: var(--color-red);
    background: rgba(229, 72, 77, 0.06);
  }
  .connect-msg.ok {
    color: var(--cyan);
    background: var(--cyan-dim);
  }
</style>
