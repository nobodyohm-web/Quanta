<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

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
  }

  $effect(() => {
    refresh();
    const t = setInterval(refresh, 5000);
    return () => clearInterval(t);
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
</style>
