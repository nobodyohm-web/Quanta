<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import Blockchain3D from "./Blockchain3D.svelte";
  import Network3D from "./Network3D.svelte";
  import ChainHistory from "./ChainHistory.svelte";
  import { t } from "./i18n.svelte";

  let chainView = $state<"history" | "2d" | "3d">("history");

  let peerCount = $state(0);
  let myPeerId = $state("");
  let isOnline = $state(false);
  let protocol = $state("");
  let totalMined = $state(0);
  let totalBurned = $state(0);
  let supply = $state(0);
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
  let newBlockFlash = $state(0);     // ms epoch du dernier bloc reçu → animation
  let mintedDisplay = $state(0);     // compteur animé, monte en continu
  const myShare = $derived(supplyQta > 0 ? (myBalance / supplyQta) * 100 : 0);

  async function loadChain() {
    try {
      const o = await invoke<any>("get_chain_overview", { limit: 14 });
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
  }
  let connectInput = $state("");
  let connectErr = $state("");
  let connectSuccess = $state(false);
  let connecting = $state(false);
  let networkCanvas: HTMLCanvasElement;
  let animFrame = $state(0);

  // NET-9/NET-10/NET-15: Per-peer metrics + display name (NET-15)
  type PeerMetric = {
    public_key: string;
    display_name: string | null;
    country: string;
    watts: number;
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
    try {
      const l = await invoke<any>("get_ledger_stats");
      totalMined = l?.total_mined ?? 0;
      totalBurned = l?.total_burned ?? 0;
      supply = totalMined - totalBurned;
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
    let unlisten: UnlistenFn | null = null;
    listen<SyncProgress>("quanta://chain-sync-progress", (e) => {
      syncProgress = e.payload;
      syncProgressAt = Date.now();
    }).then((fn) => { unlisten = fn; }).catch(() => {});
    return () => { if (unlisten) unlisten(); };
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
  // vers tous les peers (propagation du bloc). Lit newBlockFlash dans le rAF
  // (async) → pas de dépendance réactive, l'effet ne se reconstruit que sur
  // changement de peerCount.
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
    let lastFlash = newBlockFlash;

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
    draw();
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
</script>

<div class="page">
  <div class="page-header">
    <div>
      <div class="page-title">{t('network.title')}</div>
      <div class="page-sub">{t('network.sub')} — {protocol || t('loading')}</div>
    </div>
    <div style="display:flex;gap:8px;align-items:center;">
      <div class="status-dot" class:online={isOnline}></div>
      <span style="font-size:13px;color:var(--color-text-2);">{peerCount} {peerCount !== 1 ? t('wallet.peers') : t('wallet.peer')} {t('network.connectedAdj')}</span>
    </div>
  </div>

  <!-- Globe réseau 3D — héros : le réseau P2P souverain à l'échelle mondiale -->
  <div class="globe-hero">
    <div class="globe-copy">
      <div class="globe-eyebrow">{t('globe.eyebrow')}</div>
      <div class="globe-h">{@html t('globe.h')}</div>
      <p class="globe-p">{t('globe.p')}</p>
      <div class="globe-stats">
        <div class="globe-stat"><span class="gs-v mono">{peerCount}</span><span class="gs-k">{t('globe.peers')}</span></div>
        <div class="globe-stat"><span class="gs-v mono">{chainHeight}</span><span class="gs-k">{t('globe.blocks')}</span></div>
      </div>
    </div>
    <div class="globe-canvas">
      <Network3D size={420} caption={false} />
    </div>
  </div>

  <!-- La Forge — QUANTA forgés en direct (rareté + possession) -->
  <div class="forge-hero" class:forge-flash={Date.now() - newBlockFlash < 1200}>
    <div class="forge-main">
      <div class="forge-label">{t('net.forgeLabel')} · {t('net.forgeMaxOf')} {fmtNum(maxSupply)} {t('net.forgeMaximum')}</div>
      <div class="forge-count mono">{fmtForge(mintedDisplay)}</div>
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
        <span class="forge-side-v mono">{fmtForge(myBalance)}<span class="forge-unit"> QNT</span></span>
      </div>
      <div class="forge-side-row">
        <span class="forge-side-k">{t('net.yourShare')}</span>
        <span class="forge-side-v mono" style="color:var(--color-accent);">{myShare > 0 && myShare < 0.01 ? '<0,01' : myShare.toFixed(2)} %</span>
      </div>
      <div class="forge-share-bar"><div class="forge-share-fill" style="width:{Math.min(100, myShare > 0 ? Math.max(myShare, 2) : 0)}%;"></div></div>
      <div class="forge-side-row" style="margin-top:10px;">
        <span class="forge-side-k">{t('net.holders')}</span>
        <span class="forge-side-v mono">{holders}</span>
      </div>
      <div class="forge-side-row">
        <span class="forge-side-k">{t('net.circulating')}</span>
        <span class="forge-side-v mono">{fmtNum(supplyQta)} QNT</span>
      </div>
    </div>
  </div>

  <!-- Blockchain en direct -->
  <div class="chain-wrap">
    <div class="chain-head">
      <span class="chain-title">{t('net.chainTitle')}</span>
      <div style="display:flex;align-items:center;gap:12px;">
        <span class="chain-meta">{pendingTx} {t('net.chainPendingMeta')}</span>
        <div class="chain-toggle">
          <button class:active={chainView === 'history'} onclick={() => (chainView = 'history')}>{t('net.chainViewHistory')}</button>
          <button class:active={chainView === '2d'} onclick={() => (chainView = '2d')}>{t('net.chainViewRecent')}</button>
          <button class:active={chainView === '3d'} onclick={() => (chainView = '3d')}>3D</button>
        </div>
      </div>
    </div>
    {#if chainView === 'history'}
      <ChainHistory />
    {:else if chainView === '3d'}
      <Blockchain3D blocks={blocks} pending={pendingTx} flashAt={newBlockFlash} />
    {:else}
      <div class="chain-strip">
        <div class="chain-pending" title={t('net.chainPendingTip')}>
          <div class="chain-pending-n mono">{pendingTx}</div>
          <div class="chain-pending-l">{t('net.chainForging')}</div>
        </div>
        {#each blocks as b, i (b.index)}
          <div class="chain-link"></div>
          <div class="chain-block" class:chain-block-new={i === 0 && Date.now() - newBlockFlash < 1600}>
            <div class="chain-block-h">#{b.index}</div>
            <div class="chain-block-mint mono">+{(b.minted_qta ?? 0).toFixed(3)}</div>
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

  <!-- NET-16: chain-sync progress banner -->
  {#if showSyncBanner && syncProgress}
    <div class="card sync-banner" style="margin-bottom:12px;">
      <div class="sync-row">
        <span class="sync-label">{t('net.syncLabel')}</span>
        <span class="sync-counts mono">
          {syncProgress.our_height} / {syncProgress.sender_height} {t('net.syncBlocks')}
          {#if syncProgress.integrated > 0}
            <span class="sync-delta">+{syncProgress.integrated}</span>
          {/if}
        </span>
      </div>
      <div class="sync-bar"><div class="sync-bar-fill" style="width:{syncPercent}%;"></div></div>
    </div>
  {/if}

  <!-- NET-15: Display name editor -->
  <div class="card name-panel" style="margin-bottom:12px;">
    <div class="name-row">
      <div class="name-label">
        <span class="name-title">{t('net.nicknameTitle')}</span>
        <span class="name-sub">{t('net.nicknameHint')}</span>
      </div>
      <div style="display:flex;gap:8px;flex:1;max-width:420px;">
        <input
          class="input"
          maxlength="32"
          placeholder={t('net.nicknamePlaceholder')}
          bind:value={displayNameDraft}
          onkeydown={(e) => e.key === 'Enter' && saveDisplayName()}
          style="flex:1;"
        />
        <button class="btn btn-sm" onclick={saveDisplayName} disabled={displayNameSaving}>
          {displayNameSaving ? '⏳' : t('net.nicknameSave')}
        </button>
      </div>
    </div>
    {#if myDisplayName !== null && myDisplayName !== ''}
      <div class="name-current">{t('net.nicknameCurrent')} <strong>{myDisplayName}</strong></div>
    {/if}
  </div>

  <!-- NET-9/10: Peer metrics table -->
  {#if peerMetrics.length > 0}
    <div class="card peers-panel" style="margin-bottom:12px;">
      <h3 class="connect-title" style="margin-bottom:12px;">{t('net.peersHeading')} ({peerMetrics.length})</h3>
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
            <span class="peer-name mono" title={p.public_key}>
              {p.display_name || (p.public_key.slice(0, 16) + '…')}
            </span>
            <span>{p.country || '—'}</span>
            <span class="mono">{p.smoothed_rtt_ms != null ? p.smoothed_rtt_ms + ' ms' : '—'}</span>
            <span class="mono">{(p.loss_ratio * 100).toFixed(0)}%</span>
            <span>
              {#if p.quality_score != null}
                <span class="quality-pill" style="--q:{p.quality_score};">{p.quality_score}</span>
              {:else}
                <span style="color:var(--color-text-3);">—</span>
              {/if}
            </span>
            <span class="mono">{p.last_seen_secs_ago}s</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Connection panel -->
  <div class="card connect-panel" style="margin-bottom:12px;">
    <h3 class="connect-title">{t('net.connectTitle')}</h3>

    <!-- Step 1: Your ID -->
    <div class="connect-section">
      <div class="connect-step">
        <span class="step-num">1</span>
        <span class="step-text">{t('net.step1')}</span>
      </div>
      <div class="id-display">
        <code class="peer-id-code">{myPeerId || t('net.endpointLoading')}</code>
        {#if myPeerId}
          <button class="btn btn-sm" onclick={copyId}>
            {copied ? t('net.copied') : t('net.copy')}
          </button>
        {/if}
      </div>
    </div>

    <!-- Step 2: Connect -->
    <div class="connect-section">
      <div class="connect-step">
        <span class="step-num">2</span>
        <span class="step-text">{t('net.step2')}</span>
      </div>
      <div style="display:flex;gap:10px;">
        <input class="input mono" placeholder={t('net.connectPlaceholder')} bind:value={connectInput}
          onkeydown={(e) => e.key === 'Enter' && connectPeer()} style="flex:1;" />
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
  /* ── Globe réseau (héros) ── */
  .globe-hero {
    display: grid; grid-template-columns: 1fr 460px;
    align-items: center; gap: 12px;
    background: #ffffff; border: 1px solid var(--color-border);
    border-radius: 20px; padding: 8px 8px 8px 32px; margin-bottom: 14px;
    overflow: hidden; box-shadow: var(--shadow);
  }
  .globe-copy { padding: 22px 0; }
  .globe-eyebrow {
    font-size: 11px; font-weight: 700; letter-spacing: 0.1em;
    color: var(--color-accent); margin-bottom: 12px;
  }
  .globe-h {
    font-size: 30px; font-weight: 800; letter-spacing: -0.02em;
    line-height: 1.08; color: var(--color-text-0); margin-bottom: 12px;
  }
  .globe-p {
    font-size: 14px; line-height: 1.6; color: var(--color-text-2);
    max-width: 400px; margin-bottom: 20px;
  }
  .globe-stats { display: flex; gap: 28px; }
  .globe-stat { display: flex; flex-direction: column; gap: 2px; }
  .gs-v { font-size: 26px; font-weight: 800; color: var(--color-text-0); letter-spacing: -0.02em; }
  .gs-k { font-size: 11px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--color-text-3); }
  .globe-canvas {
    display: flex; align-items: center; justify-content: center;
    background: radial-gradient(circle at 50% 45%, var(--color-bg-1), var(--color-bg-2));
    border-radius: 16px; align-self: stretch; min-height: 444px;
  }
  @media (max-width: 860px) {
    .globe-hero { grid-template-columns: 1fr; padding: 24px; }
    .globe-canvas { min-height: 380px; }
  }

  /* ── La Forge — rareté & possession ── */
  .forge-hero {
    display: flex; gap: 24px; flex-wrap: wrap;
    background: #ffffff; border: 1px solid var(--color-border);
    border-radius: 16px; padding: 24px 28px; margin-bottom: 14px;
    box-shadow: var(--shadow); position: relative; overflow: hidden;
    transition: box-shadow .45s ease;
  }
  .forge-hero::before {
    content: ''; position: absolute; left: 0; top: 0; bottom: 0; width: 4px;
    background: var(--color-accent);
  }
  .forge-flash { box-shadow: 0 0 0 3px var(--color-accent-dim), var(--shadow-lg); }
  .forge-main { flex: 1; min-width: 240px; }
  .forge-label { font-size: 12px; font-weight: 600; letter-spacing: .06em; text-transform: uppercase; color: var(--color-text-3); }
  .forge-count {
    font-size: 46px; font-weight: 700; line-height: 1.05; letter-spacing: -.02em;
    color: var(--color-text-0); margin: 8px 0 10px;
    font-variant-numeric: tabular-nums; font-feature-settings: 'tnum';
  }
  .forge-sub { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--color-text-2); }
  .forge-live-dot {
    width: 8px; height: 8px; border-radius: 50%; background: var(--color-green);
    animation: forge-pulse 1.8s ease infinite; flex-shrink: 0;
  }
  @keyframes forge-pulse { 0%,100% { box-shadow: 0 0 0 0 rgba(22,163,74,.35); } 50% { box-shadow: 0 0 0 5px rgba(22,163,74,0); } }
  .forge-side {
    min-width: 220px; display: flex; flex-direction: column; gap: 7px; justify-content: center;
    border-left: 1px solid var(--color-border); padding-left: 24px;
  }
  .forge-side-row { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
  .forge-side-k { font-size: 12px; color: var(--color-text-2); }
  .forge-side-v { font-size: 15px; font-weight: 700; color: var(--color-text-0); }
  .forge-unit { font-size: 11px; color: var(--color-text-3); font-weight: 400; }
  .forge-share-bar { height: 6px; background: var(--color-bg-3); border-radius: 3px; overflow: hidden; margin-top: 2px; }
  .forge-share-fill { height: 100%; background: var(--color-accent); border-radius: 3px; transition: width 1s ease; }

  .cap-wrap { margin-top: 16px; max-width: 420px; }
  .cap-bar { height: 8px; background: var(--color-bg-3); border-radius: 4px; overflow: hidden; }
  .cap-fill { height: 100%; background: linear-gradient(90deg, #0BA5A0, #3D6FE0); border-radius: 4px; transition: width 1.2s ease; }
  .cap-meta { display: flex; justify-content: space-between; gap: 12px; margin-top: 6px; font-size: 11px; color: var(--color-text-2); }
  .cap-meta b { color: var(--color-text-0); }

  /* ── Blockchain en direct ── */
  .chain-wrap {
    background: #fff; border: 1px solid var(--color-border); border-radius: 16px;
    padding: 18px 20px; margin-bottom: 14px; box-shadow: var(--shadow-sm);
  }
  .chain-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 14px; }
  .chain-toggle { display: inline-flex; background: var(--color-bg-2); border: 1px solid var(--color-border); border-radius: 8px; overflow: hidden; }
  .chain-toggle button {
    border: 0; background: transparent; cursor: pointer; font-family: inherit;
    font-size: 12px; font-weight: 600; color: var(--color-text-2); padding: 5px 12px;
  }
  .chain-toggle button.active { background: var(--color-accent); color: #fff; }
  .chain-title { font-size: 14px; font-weight: 700; color: var(--color-text-0); }
  .chain-meta { font-size: 12px; color: var(--color-text-2); }
  .chain-strip { display: flex; align-items: stretch; overflow-x: auto; padding-bottom: 6px; }
  .chain-strip::-webkit-scrollbar { height: 4px; }
  .chain-pending {
    flex-shrink: 0; min-width: 78px; border: 1px dashed var(--color-border-hover);
    border-radius: 12px; padding: 12px 10px; text-align: center;
    display: flex; flex-direction: column; justify-content: center; gap: 2px;
    background: var(--color-bg-2);
  }
  .chain-pending-n { font-size: 22px; font-weight: 700; color: var(--color-accent); }
  .chain-pending-l { font-size: 10px; color: var(--color-text-3); }
  .chain-link { flex-shrink: 0; width: 18px; align-self: center; height: 2px; background: var(--color-border-hover); }
  .chain-block {
    flex-shrink: 0; min-width: 94px; border: 1px solid var(--color-border);
    border-radius: 12px; padding: 12px; background: #fff;
    box-shadow: var(--shadow-sm); display: flex; flex-direction: column; gap: 3px;
  }
  .chain-block-new { animation: chain-in .6s cubic-bezier(.2,.8,.2,1); border-color: var(--color-accent); box-shadow: 0 0 0 3px var(--color-accent-dim); }
  @keyframes chain-in { from { opacity: 0; transform: translateX(-18px) scale(.92); } to { opacity: 1; transform: none; } }
  .chain-block-h { font-size: 13px; font-weight: 700; color: var(--color-text-0); }
  .chain-block-mint { font-size: 13px; font-weight: 700; color: var(--color-green); }
  .chain-block-meta { font-size: 11px; color: var(--color-text-2); }
  .chain-block-hash { font-size: 10px; color: var(--color-text-3); }
  .chain-empty { flex-shrink: 0; padding: 16px; font-size: 13px; color: var(--color-text-3); align-self: center; }

  .status-dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--color-text-3);
    transition: background 0.3s;
  }
  .status-dot.online {
    background: var(--color-green);
    box-shadow: 0 0 0 0 rgba(22,163,74,0.4);
    animation: pulse-dot 2s ease-in-out infinite;
  }
  @keyframes pulse-dot {
    0%, 100% { box-shadow: 0 0 0 0 rgba(22,163,74,0.35); }
    50% { box-shadow: 0 0 0 4px rgba(22,163,74,0); }
  }

  .connect-panel { padding: 24px; }
  .connect-title {
    font-size: 15px; font-weight: 600;
    letter-spacing: -0.02em;
    margin: 0 0 20px 0;
  }
  .connect-section {
    margin-bottom: 20px;
    padding-bottom: 20px;
    border-bottom: 1px solid var(--color-border);
  }
  .connect-section:last-child {
    margin-bottom: 0;
    padding-bottom: 0;
    border-bottom: none;
  }
  .connect-step {
    display: flex; align-items: center; gap: 10px;
    margin-bottom: 12px;
  }
  .step-num {
    width: 22px; height: 22px; min-width: 22px;
    border-radius: 50%;
    background: var(--color-accent);
    color: #fff;
    display: flex; align-items: center; justify-content: center;
    font-size: 11px; font-weight: 700;
  }
  .step-text {
    font-size: 13px; font-weight: 500;
    color: var(--color-text-1);
  }
  .id-display {
    display: flex; align-items: center; gap: 12px;
    padding: 12px 16px;
    background: var(--color-bg-2);
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
  .btn-sm {
    padding: 6px 14px;
    font-size: 11px;
    white-space: nowrap;
  }
  .connect-msg {
    font-size: 12px;
    margin-top: 8px;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
  }
  .connect-msg.err {
    color: var(--color-red);
    background: rgba(255, 68, 68, 0.06);
  }
  .connect-msg.ok {
    color: var(--color-green);
    background: rgba(22, 163, 74, 0.08);
  }

  /* NET-16: chain-sync banner */
  .sync-banner { padding: 16px 20px; }
  .sync-row { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 8px; }
  .sync-label { font-size: 13px; color: var(--color-text-2); font-weight: 500; }
  .sync-counts { font-size: 12px; color: var(--color-text-1); }
  .sync-delta {
    display: inline-block;
    margin-left: 8px;
    padding: 1px 6px;
    border-radius: 3px;
    background: rgba(22, 163, 74, 0.12);
    color: var(--color-green);
    font-size: 11px;
  }
  .sync-bar {
    height: 4px;
    background: var(--color-border);
    border-radius: 2px;
    overflow: hidden;
  }
  .sync-bar-fill {
    height: 100%;
    background: #00DC82;
    transition: width 0.4s ease-out;
  }

  /* NET-15: display name editor */
  .name-panel { padding: 16px 20px; }
  .name-row {
    display: flex;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
  }
  .name-label { display: flex; flex-direction: column; gap: 2px; min-width: 200px; }
  .name-title { font-size: 13px; font-weight: 600; color: var(--color-text-1); }
  .name-sub { font-size: 11px; color: var(--color-text-3); }
  .name-current {
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--color-border);
    font-size: 12px;
    color: var(--color-text-2);
  }

  /* NET-9/10: peer table */
  .peers-panel { padding: 20px; }
  .peers-table {
    display: flex;
    flex-direction: column;
    gap: 1px;
    background: var(--color-border);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    overflow: hidden;
  }
  .peers-head, .peers-row {
    display: grid;
    grid-template-columns: 2fr 0.7fr 0.9fr 0.9fr 0.9fr 0.7fr;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    background: var(--color-bg-1);
    font-size: 12px;
  }
  .peers-head {
    background: var(--color-bg-2);
    color: var(--color-text-3);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .peers-row:hover { background: var(--color-bg-2); }
  .peer-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text-1);
  }
  .quality-pill {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 10px;
    font-size: 11px;
    font-weight: 600;
    /*
     * NET-10: green for >=80, amber for 50-79, red below 50.
     * Pure color cue, no glow (rule 11).
     */
    background: hsl(calc(var(--q) * 1.2), 70%, 18%);
    color: hsl(calc(var(--q) * 1.2), 70%, 70%);
  }
</style>
