<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";

  let peerCount = $state(0);
  let myPeerId = $state("");
  let isOnline = $state(false);
  let protocol = $state("");
  let totalMined = $state(0);
  let totalBurned = $state(0);
  let supply = $state(0);
  let copied = $state(false);
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

  interface CanvasNode {
    x: number; y: number; vx: number; vy: number;
    r: number; isMe: boolean; label: string;
  }

  let nodes: CanvasNode[] = [];
  let pulses: { fx: number; fy: number; tx: number; ty: number; t: number }[] = [];

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
    loadDisplayName();
    const t = setInterval(refresh, 5000);
    return () => clearInterval(t);
  });

  // NET-16: subscribe to chain-sync progress events from the backend.
  $effect(() => {
    let unlisten: UnlistenFn | null = null;
    listen<SyncProgress>("torus://chain-sync-progress", (e) => {
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

  // Canvas animation — uses REAL peer count, no fake data
  $effect(() => {
    if (!networkCanvas) return;
    const W = networkCanvas.width, H = networkCanvas.height;
    const cx = W / 2, cy = H / 2;

    const myNode: CanvasNode = { x: cx, y: cy, vx: 0, vy: 0, r: 18, isMe: true, label: 'MOI' };
    const count = peerCount; // REAL count only
    const peerNodes: CanvasNode[] = Array.from({ length: count }, (_, i) => {
      const angle = (i / Math.max(count, 1)) * Math.PI * 2;
      const dist = 80 + Math.random() * 100;
      return {
        x: cx + Math.cos(angle) * dist,
        y: cy + Math.sin(angle) * dist,
        vx: (Math.random() - 0.5) * 0.3,
        vy: (Math.random() - 0.5) * 0.3,
        r: 8 + Math.random() * 6,
        isMe: false,
        label: `P${i + 1}`,
      };
    });
    nodes = [myNode, ...peerNodes];

    const draw = () => {
      const ctx = networkCanvas.getContext('2d');
      if (!ctx) return;
      ctx.clearRect(0, 0, W, H);

      // Move peers gently
      nodes.forEach(n => {
        if (n.isMe) return;
        n.x += n.vx; n.y += n.vy;
        const dx = n.x - cx, dy = n.y - cy;
        const dist = Math.sqrt(dx * dx + dy * dy);
        if (dist > 180 || dist < 60) { n.vx *= -0.5; n.vy *= -0.5; }
        if (n.x < 10 || n.x > W - 10) n.vx *= -1;
        if (n.y < 10 || n.y > H - 10) n.vy *= -1;
      });

      // Random pulse (only if peers exist)
      if (Math.random() < 0.02 && peerNodes.length > 0) {
        const target = peerNodes[Math.floor(Math.random() * peerNodes.length)];
        pulses.push({ fx: myNode.x, fy: myNode.y, tx: target.x, ty: target.y, t: 0 });
      }

      // Draw connections
      peerNodes.forEach(n => {
        const dx = n.x - myNode.x, dy = n.y - myNode.y;
        const dist = Math.sqrt(dx * dx + dy * dy);
        const alpha = Math.max(0, 1 - dist / 220) * 0.3;
        ctx.beginPath();
        ctx.moveTo(myNode.x, myNode.y);
        ctx.lineTo(n.x, n.y);
        ctx.strokeStyle = `rgba(0,229,204,${alpha})`;
        ctx.lineWidth = 0.5;
        ctx.stroke();
      });

      // Pulses
      pulses = pulses.filter(p => p.t < 1);
      pulses.forEach(p => {
        p.t += 0.015;
        const px = p.fx + (p.tx - p.fx) * p.t;
        const py = p.fy + (p.ty - p.fy) * p.t;
        const a = 1 - p.t;
        ctx.beginPath(); ctx.arc(px, py, 3, 0, Math.PI * 2);
        ctx.fillStyle = `rgba(0,229,204,${a})`; ctx.fill();
      });

      // Peer nodes
      peerNodes.forEach(n => {
        ctx.beginPath(); ctx.arc(n.x, n.y, n.r, 0, Math.PI * 2);
        ctx.fillStyle = 'rgba(255,255,255,0.06)'; ctx.fill();
        ctx.strokeStyle = 'rgba(255,255,255,0.12)'; ctx.lineWidth = 1; ctx.stroke();
        ctx.font = '9px monospace'; ctx.fillStyle = 'rgba(255,255,255,0.4)';
        ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
        ctx.fillText(n.label, n.x, n.y);
      });

      // My node
      const t = Date.now() / 1000;
      const glow = 0.15 + Math.sin(t * 2) * 0.05;
      ctx.beginPath(); ctx.arc(myNode.x, myNode.y, myNode.r + 8, 0, Math.PI * 2);
      ctx.fillStyle = `rgba(0,229,204,${glow})`; ctx.fill();
      ctx.beginPath(); ctx.arc(myNode.x, myNode.y, myNode.r, 0, Math.PI * 2);
      ctx.fillStyle = '#00E5CC'; ctx.fill();
      ctx.font = 'bold 8px monospace'; ctx.fillStyle = '#000';
      ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
      ctx.fillText('MOI', myNode.x, myNode.y);

      // Empty state text
      if (peerNodes.length === 0) {
        ctx.font = '13px Inter, sans-serif';
        ctx.fillStyle = 'rgba(255,255,255,0.2)';
        ctx.textAlign = 'center';
        ctx.fillText('Aucun peer connecté', cx, cy + 50);
        ctx.font = '11px Inter, sans-serif';
        ctx.fillText('Partagez votre Peer ID pour connecter un ami', cx, cy + 70);
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
    if (!connectInput.trim()) { connectErr = "Peer ID requis"; return; }
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
</script>

<div class="page">
  <div class="page-header">
    <div>
      <div class="page-title">Réseau</div>
      <div class="page-sub">Réseau P2P QUANTA — {protocol || 'Initialisation…'}</div>
    </div>
    <div style="display:flex;gap:8px;align-items:center;">
      <div class="status-dot" class:online={isOnline}></div>
      <span style="font-size:13px;color:var(--color-text-2);">{peerCount} peer{peerCount !== 1 ? 's' : ''} connecté{peerCount !== 1 ? 's' : ''}</span>
    </div>
  </div>

  <!-- Network visualization -->
  <div class="network-canvas-wrap" style="height:300px;margin-bottom:12px;">
    <canvas bind:this={networkCanvas} width={860} height={300} style="width:100%;height:300px;"></canvas>
  </div>

  <!-- Network stats -->
  <div class="grid-3" style="margin-bottom:12px;">
    <div class="card">
      <div class="stat-label">Total miné</div>
      <div class="stat-val sm mono">{fmtNum(totalMined)}<span style="font-size:13px;color:var(--color-text-3);margin-left:4px;">QNT</span></div>
    </div>
    <div class="card">
      <div class="stat-label">Total brûlé</div>
      <div class="stat-val sm mono" style="color:var(--color-amber);">{fmtNum(totalBurned)}<span style="font-size:13px;color:var(--color-text-3);margin-left:4px;">QNT</span></div>
    </div>
    <div class="card">
      <div class="stat-label">En circulation</div>
      <div class="stat-val sm mono">{fmtNum(supply)}<span style="font-size:13px;color:var(--color-text-3);margin-left:4px;">QNT</span></div>
    </div>
  </div>

  <!-- NET-16: chain-sync progress banner -->
  {#if showSyncBanner && syncProgress}
    <div class="card sync-banner" style="margin-bottom:12px;">
      <div class="sync-row">
        <span class="sync-label">Synchronisation chaîne</span>
        <span class="sync-counts mono">
          {syncProgress.our_height} / {syncProgress.sender_height} blocs
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
        <span class="name-title">Surnom public</span>
        <span class="name-sub">Affiché aux autres peers (signé). Vide = anonyme.</span>
      </div>
      <div style="display:flex;gap:8px;flex:1;max-width:420px;">
        <input
          class="input"
          maxlength="32"
          placeholder="Ex: alex@quanta"
          bind:value={displayNameDraft}
          onkeydown={(e) => e.key === 'Enter' && saveDisplayName()}
          style="flex:1;"
        />
        <button class="btn btn-sm" onclick={saveDisplayName} disabled={displayNameSaving}>
          {displayNameSaving ? '⏳' : 'Enregistrer'}
        </button>
      </div>
    </div>
    {#if myDisplayName !== null && myDisplayName !== ''}
      <div class="name-current">Actuel : <strong>{myDisplayName}</strong></div>
    {/if}
  </div>

  <!-- NET-9/10: Peer metrics table -->
  {#if peerMetrics.length > 0}
    <div class="card peers-panel" style="margin-bottom:12px;">
      <h3 class="connect-title" style="margin-bottom:12px;">Pairs ({peerMetrics.length})</h3>
      <div class="peers-table">
        <div class="peers-head">
          <span>Nom / Clé</span>
          <span>Pays</span>
          <span>RTT</span>
          <span>Pertes</span>
          <span>Qualité</span>
          <span>Vu</span>
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
    <h3 class="connect-title">Connexion P2P</h3>

    <!-- Step 1: Your ID -->
    <div class="connect-section">
      <div class="connect-step">
        <span class="step-num">1</span>
        <span class="step-text">Envoyez votre Peer ID à votre ami</span>
      </div>
      <div class="id-display">
        <code class="peer-id-code">{myPeerId || '⏳ Endpoint en cours…'}</code>
        {#if myPeerId}
          <button class="btn btn-sm" onclick={copyId}>
            {copied ? '✓ Copié' : 'Copier'}
          </button>
        {/if}
      </div>
    </div>

    <!-- Step 2: Connect -->
    <div class="connect-section">
      <div class="connect-step">
        <span class="step-num">2</span>
        <span class="step-text">Collez le Peer ID de votre ami</span>
      </div>
      <div style="display:flex;gap:10px;">
        <input class="input mono" placeholder="Coller le Peer ID reçu…" bind:value={connectInput}
          onkeydown={(e) => e.key === 'Enter' && connectPeer()} style="flex:1;" />
        <button class="btn btn-primary" onclick={connectPeer} disabled={connecting}>
          {connecting ? '⏳' : 'Connecter'}
        </button>
      </div>
      {#if connectErr}
        <div class="connect-msg err">{connectErr}</div>
      {/if}
      {#if connectSuccess}
        <div class="connect-msg ok">✓ Peer connecté ! Le Hello sera échangé dans quelques secondes.</div>
      {/if}
    </div>
  </div>
</div>

<style>
  .status-dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--color-text-3);
    transition: background 0.3s;
  }
  .status-dot.online {
    background: #00E5CC;
    box-shadow: 0 0 6px rgba(0,229,204,0.5);
    animation: pulse-dot 2s ease-in-out infinite;
  }
  @keyframes pulse-dot {
    0%, 100% { box-shadow: 0 0 4px rgba(0,229,204,0.3); }
    50% { box-shadow: 0 0 12px rgba(0,229,204,0.7); }
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
    background: var(--color-accent, #00E5CC);
    color: #000;
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
    color: #00E5CC;
    background: rgba(0, 229, 204, 0.06);
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
    background: rgba(0, 220, 130, 0.12);
    color: #00DC82;
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
