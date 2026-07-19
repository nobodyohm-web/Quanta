<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import ChainHistory from "./ChainHistory.svelte";
  import { t } from "./i18n.svelte";

  let chainView = $state<"history" | "2d">("history");

  let peerCount = $state(0);
  let myPeerId = $state("");
  let isOnline = $state(false);
  let protocol = $state("");
  let copied = $state(false);

  // ─── La Forge — blockchain en direct + rareté ──────────────────
  let chainHeight = $state(0);
  let supplyQta = $state(0);
  let mintedQta = $state(0);
  let burnedQta = $state(0);
  let maxSupply = $state(100_000_000);
  let pctToCap = $state(0);
  let emissionNextTick = $state(0);   // QUANTA émis au prochain tick (minute)
  let emissionPerHour = $derived(emissionNextTick * 60);
  let pendingTx = $state(0);
  let holders = $state(0);
  let myBalance = $state(0);
  let blocks = $state<any[]>([]);
  let newBlockFlash = $state(0);
  let finalityFloor = $state(0);     // ms epoch du dernier bloc reçu → animation
  let mintedDisplay = $state(0);     // compteur animé, monte en continu
  const myShare = $derived(supplyQta > 0 ? (myBalance / supplyQta) * 100 : 0);

  async function loadChain() {
    try {
      const o = await invoke<any>("get_chain_overview", { limit: 22 });
      const prev = chainHeight;
      chainHeight = o.height ?? 0;
      supplyQta = o.total_supply_qta ?? 0;
      mintedQta = o.total_mined_qta ?? 0;
      burnedQta = o.total_burned_qta ?? 0;
      maxSupply = o.max_supply_qta ?? 100_000_000;
      pctToCap = o.pct_to_cap ?? 0;
      emissionNextTick = o.emission_next_tick_qta ?? 0;
      pendingTx = o.pending ?? 0;
      holders = o.holders ?? 0;
      blocks = o.blocks ?? [];
      if (prev && chainHeight > prev) newBlockFlash = Date.now();
    } catch {}
    try {
      const f = await invoke<any>("get_finality_status");
      finalityFloor = f?.finalized_floor ?? 0;
    } catch {}
  }
  let connectInput = $state("");
  let connectErr = $state("");
  let connectSuccess = $state(false);
  let connecting = $state(false);
  let networkCanvas: HTMLCanvasElement;
  // Variable simple (PAS $state) : réécrite 60 fps par la boucle canvas, elle
  // n'a aucun lecteur réactif — la garder en $state planifiait 60 flushs/s inutiles.
  let animFrame = 0;

  // NET-9/NET-10/NET-15: Per-peer metrics + display name (NET-15)
  type PeerMetric = {
    public_key: string;
    display_name: string | null;
    country: string;
    last_rtt_ms: number | null;
    smoothed_rtt_ms: number | null;
    bytes_in: number;
    messages_in: number;
    pings_sent: number;
    pongs_received: number;
    loss_ratio: number;
    uptime_secs: number;
    quality_score: number | null;
    last_seen_secs_ago: number;
  };
  let peerMetrics = $state<PeerMetric[]>([]);
  let myDisplayName = $state<string | null>(null);
  let displayNameDraft = $state("");
  let displayNameSaving = $state(false);

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


  async function refresh() {
    try {
      const s = await invoke<any>("get_node_status");
      peerCount = s?.peer_count ?? 0;
      myPeerId = s?.peer_id ?? "";
      isOnline = s?.is_online ?? false;
      protocol = s?.protocol ?? "";
    } catch {}
    // NET-9/10: pull per-peer metrics every refresh tick
    try {
      peerMetrics = await invoke<PeerMetric[]>("get_peer_metrics");
    } catch {}
    try {
      const r = await invoke<any>("get_my_reputation");
      myBalance = r?.atn_balance ?? 0;
    } catch {}
  }

  async function loadDisplayName() {
    try {
      myDisplayName = await invoke<string | null>("get_display_name");
      displayNameDraft = myDisplayName ?? "";
    } catch {}
  }

  async function saveDisplayName() {
    displayNameSaving = true;
    try {
      const trimmed = displayNameDraft.trim();
      const arg = trimmed.length === 0 ? null : trimmed;
      myDisplayName = await invoke<string | null>("set_display_name", { name: arg });
      displayNameDraft = myDisplayName ?? "";
    } catch (e) {
      console.warn("set_display_name failed", e);
    }
    displayNameSaving = false;
  }

  $effect(() => {
    refresh();
    loadChain();
    loadDisplayName();
    const iv = setInterval(refresh, 5000);
    const tc = setInterval(loadChain, 1500);
    return () => { clearInterval(iv); clearInterval(tc); };
  });

  // Compteur « QUANTA forgés » qui monte en continu : dérive au rythme
  // d'émission RÉEL du moment (emissionPerHour, décroissant, lu de la chaîne)
  // et rattrape la vraie valeur à chaque sondage.
  // Lit mintedQta/emissionPerHour uniquement dans le callback rAF (async) →
  // pas de dépendance réactive, l'effet ne tourne qu'une fois.
  $effect(() => {
    let raf = 0;
    let last = performance.now();
    let v = 0;
    const tick = (now: number) => {
      const dt = Math.min(0.1, (now - last) / 1000); last = now;
      v += (emissionPerHour / 3600) * dt;                 // dérive live
      if (mintedQta > v) v += (mintedQta - v) * Math.min(1, dt * 2.5); // rattrapage doux
      mintedDisplay = v;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

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

  // Canvas animation — graphe réseau vivant (compte de peers RÉEL, zéro fake).
  // Les peers orbitent en ellipse ; les connexions « coulent » (pointillés
  // animés) ; à chaque nouveau bloc, une rafale d'impulsions part de mon nœud
  // vers tous les peers (propagation du bloc).
  // SEULE dépendance voulue : peerCount. Piège Svelte 5 (bug du gel au forge) :
  // le premier draw() était appelé SYNCHRONIQUEMENT dans le corps de l'effet →
  // toutes ses lectures ($state newBlockFlash, locale via t()) devenaient des
  // dépendances trackées, et l'écriture de newBlockFlash au scellement d'un
  // bloc reconstruisait tout l'effet (positions re-tirées, rAF relancé) au
  // moment exact du forge. Désormais : lastFlash init sous untrack, et le
  // premier draw passe par requestAnimationFrame (asynchrone = non tracké).
  $effect(() => {
    if (!networkCanvas) return;
    const W = networkCanvas.width, H = networkCanvas.height;
    const cx = W / 2, cy = H / 2;
    const count = peerCount;

    const peers = Array.from({ length: count }, (_, i) => ({
      angle: (i / Math.max(count, 1)) * Math.PI * 2 + Math.random() * 0.4,
      radius: 92 + (i % 3) * 28,
      speed: 0.0015 + Math.random() * 0.0011,
      r: 7 + Math.random() * 4,
      phase: Math.random() * Math.PI * 2,
    }));
    let localPulses: { fx: number; fy: number; tx: number; ty: number; t: number; big: boolean }[] = [];
    let lastFlash = untrack(() => newBlockFlash);

    const draw = () => {
      const ctx = networkCanvas.getContext('2d');
      if (!ctx) return;
      const now = Date.now();
      ctx.clearRect(0, 0, W, H);

      // Anneaux de guidage (profondeur)
      for (const rr of [72, 118, 162]) {
        ctx.beginPath(); ctx.ellipse(cx, cy, rr, rr * 0.78, 0, 0, Math.PI * 2);
        ctx.strokeStyle = 'rgba(0,0,0,0.035)'; ctx.lineWidth = 1; ctx.stroke();
      }

      // Positions courantes (orbite elliptique = léger effet de tilt 3D)
      peers.forEach(p => { p.angle += p.speed; });
      const pos = peers.map(p => ({
        x: cx + Math.cos(p.angle) * p.radius,
        y: cy + Math.sin(p.angle) * p.radius * 0.78,
        r: p.r, phase: p.phase,
      }));

      // Nouveau bloc → rafale de propagation vers tous les peers
      if (newBlockFlash !== lastFlash) {
        lastFlash = newBlockFlash;
        pos.forEach(n => localPulses.push({ fx: cx, fy: cy, tx: n.x, ty: n.y, t: 0, big: true }));
      }
      // Trafic ambiant
      if (Math.random() < 0.02 && pos.length) {
        const n = pos[Math.floor(Math.random() * pos.length)];
        localPulses.push({ fx: cx, fy: cy, tx: n.x, ty: n.y, t: 0, big: false });
      }

      // Connexions « qui coulent »
      pos.forEach(n => {
        const dist = Math.hypot(n.x - cx, n.y - cy);
        const a = Math.max(0, 1 - dist / 230) * 0.4;
        ctx.beginPath();
        ctx.setLineDash([3, 6]);
        ctx.lineDashOffset = -((now / 55) % 9);
        ctx.moveTo(cx, cy); ctx.lineTo(n.x, n.y);
        ctx.strokeStyle = `rgba(11,165,160,${a})`;
        ctx.lineWidth = 1;
        ctx.stroke();
      });
      ctx.setLineDash([]);

      // Impulsions
      localPulses = localPulses.filter(p => p.t < 1);
      localPulses.forEach(p => {
        p.t += p.big ? 0.03 : 0.02;
        const e = p.t;
        const px = p.fx + (p.tx - p.fx) * e, py = p.fy + (p.ty - p.fy) * e;
        ctx.beginPath(); ctx.arc(px, py, (p.big ? 4 : 2.5) * (1 - e * 0.25), 0, Math.PI * 2);
        ctx.fillStyle = `rgba(11,165,160,${(1 - e) * (p.big ? 0.9 : 0.55)})`; ctx.fill();
      });

      // Peers — anneaux propres, halo doux, respiration
      pos.forEach(n => {
        const breathe = 1 + Math.sin(now / 700 + n.phase) * 0.08;
        const r = n.r * breathe;
        ctx.beginPath(); ctx.arc(n.x, n.y, r + 5, 0, Math.PI * 2);
        ctx.fillStyle = 'rgba(11,165,160,0.06)'; ctx.fill();
        ctx.beginPath(); ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
        ctx.fillStyle = '#ffffff'; ctx.fill();
        ctx.strokeStyle = 'rgba(11,165,160,0.55)'; ctx.lineWidth = 1.6; ctx.stroke();
      });

      // Mon nœud — anneau qui respire + cœur plein
      const mb = 1 + Math.sin(now / 600) * 0.06;
      const mr = 18 * mb;
      ctx.beginPath(); ctx.arc(cx, cy, mr + 10, 0, Math.PI * 2);
      ctx.fillStyle = `rgba(11,165,160,${0.10 + Math.sin(now / 600) * 0.03})`; ctx.fill();
      ctx.beginPath(); ctx.arc(cx, cy, mr, 0, Math.PI * 2);
      ctx.fillStyle = '#0BA5A0'; ctx.fill();
      ctx.font = 'bold 9px Inter, sans-serif'; ctx.fillStyle = '#fff';
      ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
      ctx.fillText(t('net.canvasMe'), cx, cy);

      if (pos.length === 0) {
        ctx.fillStyle = 'rgba(0,0,0,0.4)'; ctx.font = '13px Inter, sans-serif'; ctx.textAlign = 'center';
        ctx.fillText(t('net.canvasNoPeers'), cx, cy + 56);
        ctx.fillStyle = 'rgba(0,0,0,0.3)'; ctx.font = '11px Inter, sans-serif';
        ctx.fillText(t('net.canvasShareHint'), cx, cy + 76);
      }

      animFrame = requestAnimationFrame(draw);
    };
    // JAMAIS de draw() synchrone ici (ses lectures seraient trackées) — le
    // premier rendu passe par rAF, hors du contexte réactif de l'effet.
    animFrame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(animFrame);
  });

  function copyId() {
    navigator.clipboard?.writeText(myPeerId);
    copied = true;
    setTimeout(() => copied = false, 2000);
  }

  async function connectPeer() {
    connectErr = "";
    connectSuccess = false;
    if (!connectInput.trim()) { connectErr = t('net.peerIdRequired'); return; }
    connecting = true;
    try {
      await invoke("connect_peer", { peerId: connectInput.trim() });
      connectInput = "";
      connectSuccess = true;
      setTimeout(() => connectSuccess = false, 4000);
      refresh();
    } catch (e) {
      connectErr = String(e);
    }
    connecting = false;
  }

  function fmtNum(n: number) {
    if (n >= 1e6) return (n / 1e6).toFixed(2) + 'M';
    if (n >= 1e3) return (n / 1e3).toFixed(1) + 'K';
    return n.toFixed(2);
  }

  // Grand compteur live : séparateurs de milliers + 3 décimales qui défilent.
  function fmtForge(n: number) {
    return n.toLocaleString('fr-FR', { minimumFractionDigits: 3, maximumFractionDigits: 3 });
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
  <div class="grid-4 net-summary">
    <div class="card">
      <div class="stat-label">{t('db.peers')}</div>
      <div class="stat-val sm">{peerCount}</div>
    </div>
    <div class="card">
      <div class="stat-label">{t('db.height')}</div>
      <div class="stat-val sm">{chainHeight}</div>
      <div class="stat-sub">{t('db.blocks')}</div>
    </div>
    <div class="card">
      <div class="stat-label">Plancher de finalité</div>
      <div class="stat-val sm">{finalityFloor}</div>
    </div>
    <div class="card">
      <div class="stat-label">Statut</div>
      <div class="stat-val sm">{isOnline ? t('wallet.connected') : t('wallet.offline')}</div>
      <div class="stat-sub mono">{protocol || '—'}</div>
    </div>
  </div>

  <!-- Graphe vivant du réseau — Canvas2D honnête : MES pairs réels en orbite,
       impulsions de propagation à chaque bloc réel (newBlockFlash). -->
  <div class="card net-live">
    <canvas bind:this={networkCanvas} width="880" height="400" class="net-live-canvas"></canvas>
  </div>

  <!-- La Forge — QUANTA forgés en direct (rareté + possession) -->
  <div class="card forge-hero" class:forge-flash={Date.now() - newBlockFlash < 1200}>
    <div class="forge-main">
      <div class="section-label">{t('net.forgeLabel')} · {t('net.forgeMaxOf')} {fmtNum(maxSupply)} {t('net.forgeMaximum')}</div>
      <div class="forge-count">{fmtForge(mintedDisplay)}</div>
      <div class="forge-sub">
        <span class="forge-live-dot"></span>
        {t('net.forgeLive')} · {emissionPerHour < 1 ? emissionPerHour.toFixed(2) : emissionPerHour.toFixed(0)} {t('net.forgePerHour')} · {t('net.forgeBlock')} #{chainHeight}
      </div>
      <!-- Rareté : progression vers le plafond DUR (100M) -->
      <div class="cap-wrap">
        <div class="cap-bar"><div class="cap-fill" style="width:{Math.min(100, Math.max(pctToCap, pctToCap > 0 ? 0.5 : 0))}%;"></div></div>
        <div class="cap-meta">
          <span><b>{pctToCap < 0.01 && pctToCap > 0 ? '<0,01' : pctToCap.toFixed(2)}%</b> {t('net.capEmitted')}</span>
          <span>{@html t('net.capSupply')}</span>
        </div>
      </div>
    </div>
    <div class="forge-side">
      <div class="forge-side-row">
        <span class="forge-side-k">{t('net.youOwn')}</span>
        <span class="forge-side-v">{fmtForge(myBalance)}<span class="forge-unit"> QNT</span></span>
      </div>
      <div class="forge-side-row">
        <span class="forge-side-k">{t('net.yourShare')}</span>
        <span class="forge-side-v" style="color:var(--color-accent);">{myShare > 0 && myShare < 0.01 ? '<0,01' : myShare.toFixed(2)} %</span>
      </div>
      <div class="forge-share-bar"><div class="forge-share-fill" style="width:{Math.min(100, myShare > 0 ? Math.max(myShare, 2) : 0)}%;"></div></div>
      <div class="forge-side-row forge-side-gap">
        <span class="forge-side-k">{t('net.holders')}</span>
        <span class="forge-side-v">{holders}</span>
      </div>
      <div class="forge-side-row">
        <span class="forge-side-k">{t('net.circulating')}</span>
        <span class="forge-side-v">{fmtNum(supplyQta)} QNT</span>
      </div>
    </div>
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
          <div class="chain-block" class:chain-block-new={i === 0 && Date.now() - newBlockFlash < 1600}>
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

  <!-- NET-15: éditeur de pseudonyme -->
  <div class="card name-panel">
    <div class="name-row">
      <div class="name-label">
        <span class="name-title">{t('net.nicknameTitle')}</span>
        <span class="name-sub">{t('net.nicknameHint')}</span>
      </div>
      <div class="name-field">
        <input
          class="input"
          maxlength="32"
          placeholder={t('net.nicknamePlaceholder')}
          bind:value={displayNameDraft}
          onkeydown={(e) => e.key === 'Enter' && saveDisplayName()}
        />
        <button class="btn btn-ghost btn-sm" onclick={saveDisplayName} disabled={displayNameSaving}>
          {displayNameSaving ? '⏳' : t('net.nicknameSave')}
        </button>
      </div>
    </div>
    {#if myDisplayName !== null && myDisplayName !== ''}
      <div class="name-current">{t('net.nicknameCurrent')} <strong>{myDisplayName}</strong></div>
    {/if}
  </div>

  <!-- NET-9/10: table des pairs — hairlines, chiffres tabulaires -->
  {#if peerMetrics.length > 0}
    <div class="card peers-panel">
      <h3 class="card-title peers-panel-title">{t('net.peersHeading')} · {peerMetrics.length}</h3>
      <div class="peers-table">
        <div class="peers-head">
          <span>{t('net.colNameKey')}</span>
          <span>{t('net.colCountry')}</span>
          <span>RTT</span>
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
          {connecting ? '⏳' : t('net.connectBtn')}
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
  .net-status { display: flex; gap: 8px; align-items: center; }
  .net-status-txt { font-size: 13px; color: var(--color-text-2); font-variant-numeric: tabular-nums lining-nums; }
  .status-dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--color-text-3);
    transition: background 0.3s;
  }
  /* Vert = seul point sémantique conservé : nœud réellement en ligne. */
  .status-dot.online {
    background: var(--color-green);
    box-shadow: 0 0 0 0 rgba(22,163,74,0.4);
    animation: pulse-dot 2s ease-in-out infinite;
  }
  @keyframes pulse-dot {
    0%, 100% { box-shadow: 0 0 0 0 rgba(22,163,74,0.35); }
    50% { box-shadow: 0 0 0 4px rgba(22,163,74,0); }
  }

  /* ── Résumé réseau — 4 chiffres réels, aucune imagerie ───── */
  .net-summary { margin-bottom: 16px; }
  .net-live { margin-bottom: 16px; padding: 8px; }
  .net-live-canvas { display: block; width: 100%; height: auto; }

  /* ── La Forge — rareté & possession ──────────────────────── */
  .forge-hero {
    display: flex; gap: 32px; flex-wrap: wrap;
    padding: 28px 32px; margin-bottom: 16px;
    position: relative; overflow: hidden;
    transition: box-shadow .45s ease;
  }
  .forge-flash { box-shadow: 0 0 0 2px var(--color-accent-dim), var(--shadow); }
  .forge-main { flex: 1; min-width: 240px; }
  .forge-count {
    font-family: var(--font-display);
    font-size: 52px; font-weight: 700; line-height: 1.02; letter-spacing: -.02em;
    color: var(--color-text-0); margin: 6px 0 12px;
    font-variant-numeric: tabular-nums lining-nums; font-feature-settings: 'tnum';
  }
  .forge-sub {
    display: flex; align-items: center; gap: 8px;
    font-size: 13px; color: var(--color-text-2);
    font-variant-numeric: tabular-nums lining-nums;
  }
  .forge-live-dot {
    width: 7px; height: 7px; border-radius: 50%; background: var(--color-accent);
    animation: forge-pulse 2s ease infinite; flex-shrink: 0;
  }
  @keyframes forge-pulse { 0%,100% { box-shadow: 0 0 0 0 rgba(11,165,160,.35); } 50% { box-shadow: 0 0 0 5px rgba(11,165,160,0); } }
  .forge-side {
    min-width: 220px; display: flex; flex-direction: column; gap: 10px; justify-content: center;
    border-left: 1px solid var(--color-border); padding-left: 32px;
  }
  .forge-side-row { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
  .forge-side-gap { margin-top: 10px; }
  .forge-side-k { font-size: 12px; color: var(--color-text-2); }
  .forge-side-v {
    font-family: var(--font-display);
    font-size: 16px; font-weight: 700; color: var(--color-text-0);
    font-variant-numeric: tabular-nums lining-nums;
  }
  .forge-unit { font-size: 11px; color: var(--color-text-3); font-weight: 400; }
  .forge-share-bar { height: 5px; background: var(--color-bg-3); border-radius: 3px; overflow: hidden; margin-top: 2px; }
  .forge-share-fill { height: 100%; background: var(--color-accent); border-radius: 3px; transition: width 1s ease; }

  .cap-wrap { margin-top: 20px; max-width: 440px; }
  .cap-bar { height: 6px; background: var(--color-bg-3); border-radius: 4px; overflow: hidden; }
  .cap-fill { height: 100%; background: var(--color-accent); border-radius: 4px; transition: width 1.2s ease; }
  .cap-meta {
    display: flex; justify-content: space-between; gap: 12px; margin-top: 8px;
    font-size: 11px; color: var(--color-text-2);
    font-variant-numeric: tabular-nums lining-nums;
  }
  .cap-meta b { color: var(--color-text-0); font-weight: 700; }

  /* ── Blockchain en direct ────────────────────────────────── */
  .chain-wrap { padding: 20px 24px 24px; margin-bottom: 16px; }
  .chain-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 16px; }
  .chain-head-r { display: flex; align-items: center; gap: 14px; }
  .chain-title { margin-bottom: 0; }
  .chain-meta { font-size: 12px; color: var(--color-text-2); font-variant-numeric: tabular-nums lining-nums; }
  .chain-strip { display: flex; align-items: stretch; overflow-x: auto; padding-bottom: 8px; }
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
    box-shadow: var(--shadow-sm); display: flex; flex-direction: column; gap: 4px;
  }
  .chain-block-new { animation: chain-in .6s cubic-bezier(.2,.8,.2,1); border-color: var(--color-accent); box-shadow: 0 0 0 2px var(--color-accent-dim); }
  @keyframes chain-in { from { opacity: 0; transform: translateX(-18px) scale(.92); } to { opacity: 1; transform: none; } }
  .chain-block-h { font-size: 13px; font-weight: 700; color: var(--color-text-0); font-variant-numeric: tabular-nums lining-nums; }
  .chain-block-mint {
    font-family: var(--font-display);
    font-size: 13px; font-weight: 700; color: var(--color-text-0);
    font-variant-numeric: tabular-nums lining-nums;
  }
  .chain-block-meta { font-size: 11px; color: var(--color-text-2); font-variant-numeric: tabular-nums lining-nums; }
  .chain-block-hash { font-size: 10px; color: var(--color-text-3); }
  .chain-empty { flex-shrink: 0; padding: 16px; font-size: 13px; color: var(--color-text-3); align-self: center; }

  /* ── NET-16 : bandeau de synchronisation ─────────────────── */
  .sync-banner { margin-bottom: 16px; }
  .sync-row { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 10px; }
  .sync-label { font-size: 13px; color: var(--color-text-2); font-weight: 500; }
  .sync-counts { font-size: 12px; color: var(--color-text-1); font-variant-numeric: tabular-nums lining-nums; }
  .sync-delta {
    display: inline-block;
    margin-left: 8px;
    padding: 1px 8px;
    border-radius: 100px;
    background: var(--cyan-dim);
    color: var(--cyan);
    font-size: 11px;
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

  /* ── NET-15 : éditeur de pseudonyme ──────────────────────── */
  .name-panel { margin-bottom: 16px; }
  .name-row {
    display: flex;
    align-items: center;
    gap: 20px;
    flex-wrap: wrap;
  }
  .name-label { display: flex; flex-direction: column; gap: 3px; min-width: 220px; flex: 1; }
  .name-title { font-size: 14px; font-weight: 600; color: var(--color-text-0); }
  .name-sub { font-size: 12px; color: var(--color-text-2); }
  .name-field { display: flex; gap: 8px; flex: 1; max-width: 420px; }
  .name-field .input { flex: 1; min-width: 0; }
  .name-current {
    margin-top: 14px;
    padding-top: 14px;
    border-top: 1px solid var(--color-border);
    font-size: 12px;
    color: var(--color-text-2);
  }
  .name-current strong { color: var(--color-text-0); font-weight: 600; }

  /* ── NET-9/10 : table des pairs — hairlines seules ───────── */
  .peers-panel { margin-bottom: 16px; }
  .peers-panel-title { margin-bottom: 4px; }
  .peers-table { display: flex; flex-direction: column; }
  .peers-head, .peers-row {
    display: grid;
    grid-template-columns: 2fr 0.7fr 0.9fr 0.8fr 0.8fr 0.7fr;
    align-items: center;
    gap: 16px;
    padding: 13px 4px;
    font-size: 13px;
    border-bottom: 1px solid var(--color-border);
  }
  .peers-head {
    color: var(--color-text-3);
    font-size: 11px;
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
  .peer-name { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .peer-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--color-text-3); flex-shrink: 0; }
  .peer-dot.alive { background: var(--cyan); }
  .peer-name-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-1);
    font-size: 12px;
  }
  .peer-country { color: var(--color-text-2); }
  .peer-seen { color: var(--color-text-2); }
  .peer-muted { color: var(--color-text-3); }
  /* NET-10 : qualité — teal (bon) / encre (moyen) / gris (faible), zéro arc-en-ciel. */
  .quality-pill {
    display: inline-block;
    padding: 2px 9px;
    border-radius: 100px;
    font-size: 11px;
    font-weight: 600;
    font-variant-numeric: tabular-nums lining-nums;
  }
  .quality-pill.q-good { background: var(--cyan-dim); color: var(--cyan); }
  .quality-pill.q-mid { background: var(--color-bg-3); color: var(--color-text-1); }
  .quality-pill.q-low { color: var(--color-text-3); }

  /* ── Panneau de connexion ────────────────────────────────── */
  .connect-panel { margin-bottom: 16px; }
  .connect-section {
    margin-bottom: 24px;
    padding-bottom: 24px;
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
    font-size: 11px; font-weight: 700;
    font-variant-numeric: tabular-nums lining-nums;
  }
  .step-text {
    font-size: 13px; font-weight: 500;
    color: var(--color-text-1);
  }
  .id-display {
    display: flex; align-items: center; gap: 12px;
    padding: 14px 16px;
    background: var(--color-bg-1);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border);
  }
  .peer-id-code {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-text-0);
    word-break: break-all;
    flex: 1;
    line-height: 1.6;
    user-select: all;
  }
  .connect-field { display: flex; gap: 10px; }
  .connect-field .input { flex: 1; min-width: 0; }
  .connect-msg {
    font-size: 12px;
    margin-top: 10px;
    padding: 8px 12px;
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
