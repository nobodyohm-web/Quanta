<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let peerCount = $state(0);
  let myPeerId = $state("");
  let totalMined = $state(0);
  let totalBurned = $state(0);
  let supply = $state(0);
  let copied = $state(false);
  let connectInput = $state("");
  let connectErr = $state("");
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
    const t = setInterval(refresh, 8000);
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
        ctx.fillText('Connectez un peer pour voir le réseau', cx, cy + 70);
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
    if (!connectInput.trim()) { connectErr = "Peer ID requis"; return; }
    connecting = true;
    try {
      await invoke("connect_peer", { peerId: connectInput.trim() });
      connectInput = "";
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
      <div class="page-sub">Visualisation live du réseau P2P QUANTA</div>
    </div>
    <div style="display:flex;gap:8px;align-items:center;">
      <div class="pulse-dot"></div>
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

  <!-- Peer ID + connect -->
  <div class="card" style="margin-bottom:12px;">
    <div style="display:flex;align-items:center;gap:16px;margin-bottom:16px;">
      <div style="flex:1;">
        <div class="stat-label" style="margin-bottom:6px;">Mon Peer ID</div>
        <button class="copy-btn" onclick={copyId}>
          {#if copied}✓ Copié !{:else}{myPeerId ? myPeerId.slice(0, 24) + '…' : '—'}{/if}
        </button>
      </div>
    </div>
    <div style="display:flex;gap:10px;">
      <input class="input mono" placeholder="Coller le Peer ID d'un autre nœud…" bind:value={connectInput}
        onkeydown={(e) => e.key === 'Enter' && connectPeer()} style="flex:1;" />
      <button class="btn btn-primary" onclick={connectPeer} disabled={connecting}>
        {connecting ? '…' : 'Connecter'}
      </button>
    </div>
    {#if connectErr}
      <div style="font-size:12px;color:var(--color-red);margin-top:8px;">{connectErr}</div>
    {/if}
  </div>
</div>
