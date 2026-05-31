<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let miningRate = $state(0);
  let balance = $state(0);
  let earned = $state(0);
  let staked = $state(0);
  let trustScore = $state(0);
  let uptime = $state(0);
  let energyKwh = $state(0);
  let peers = $state(0);
  let chainHeight = $state(0);
  let mode = $state("Actif");
  let eurPrice = $state(0);

  // Sparkline tracks balance snapshots every 30s
  let sparkData = $state<number[]>([]);
  let sparkTick = 0;
  let sparkCanvas: HTMLCanvasElement;

  async function refresh() {
    try {
      const r = await invoke<any>("get_my_reputation");
      balance = r?.atn_balance ?? 0;
      earned = r?.atn_earned ?? 0;
      staked = r?.atn_staked ?? 0;
      trustScore = r?.trust_score ?? 0;
      uptime = r?.uptime_minutes ?? 0;
      energyKwh = r?.energy_kwh ?? 0;
    } catch {}
    try {
      const s = await invoke<any>("get_node_status");
      peers = s?.peer_count ?? 0;
      mode = s?.mode ?? "Actif";
    } catch {}
    try {
      const l = await invoke<any>("get_ledger_stats");
      chainHeight = l?.total_blocks ?? 0;
    } catch {}
    try {
      const e = await invoke<any>("get_energy_stats");
      eurPrice = e?.atn_floor_eur ?? 0;
    } catch {}

    // Mining rate = session average (total earned / total uptime)
    if (uptime > 0) {
      miningRate = earned / uptime;
    }

    // Record balance snapshot every 30s (6 ticks × 5s)
    sparkTick++;
    if (sparkTick % 6 === 0) {
      sparkData = [...sparkData.slice(-59), earned];
    }
  }

  $effect(() => {
    refresh();
    const t = setInterval(refresh, 5000);
    return () => clearInterval(t);
  });

  // Draw sparkline
  $effect(() => {
    if (!sparkCanvas || sparkData.length < 2) return;
    const ctx = sparkCanvas.getContext('2d');
    if (!ctx) return;
    const w = sparkCanvas.width, h = sparkCanvas.height;
    const data = sparkData;
    const min = Math.min(...data), max = Math.max(...data);
    const range = max - min || 1;
    const pts = data.map((v, i) => [i / (data.length - 1) * w, h - ((v - min) / range) * (h * 0.8) - h * 0.1]);
    ctx.clearRect(0, 0, w, h);
    // Fill gradient
    const grad = ctx.createLinearGradient(0, 0, 0, h);
    grad.addColorStop(0, 'rgba(0,229,204,0.25)');
    grad.addColorStop(1, 'rgba(0,229,204,0)');
    ctx.beginPath();
    pts.forEach(([x, y], i) => i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y));
    ctx.lineTo(w, h); ctx.lineTo(0, h); ctx.closePath();
    ctx.fillStyle = grad; ctx.fill();
    // Line
    ctx.beginPath();
    pts.forEach(([x, y], i) => i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y));
    ctx.strokeStyle = '#00E5CC'; ctx.lineWidth = 1.5; ctx.stroke();
    // Dot
    const [lx, ly] = pts[pts.length - 1];
    ctx.beginPath(); ctx.arc(lx, ly, 3, 0, Math.PI * 2);
    ctx.fillStyle = '#00E5CC'; ctx.fill();
  });

  function formatUptime(min: number) {
    const h = Math.floor(min / 60);
    const m = min % 60;
    return `${h}h${m}m`;
  }

  const modeColors: Record<string, string> = { 'Actif': 'tag-green', 'Guardian': 'tag-cyan', 'Recherche': 'tag-orange' };
</script>

<div class="page">
  <div class="page-header">
    <div>
      <div class="page-title">Dashboard</div>
      <div class="page-sub">Votre nœud contribue au réseau QUANTA</div>
    </div>
    <span class="tag {modeColors[mode] ?? 'tag-dim'}">{mode}</span>
  </div>

  <!-- Mining rate hero -->
  <div class="card mining-hero" style="margin-bottom:12px;">
    <div style="display:flex;align-items:flex-start;justify-content:space-between;">
      <div>
        <div class="stat-label" style="color:rgba(0,229,204,0.6);letter-spacing:0.06em;">Taux de mining</div>
        <div style="display:flex;align-items:baseline;gap:10px;margin-top:6px;">
          <span class="mono" style="font-size:52px;font-weight:700;color:#00E5CC;line-height:1;letter-spacing:-0.02em;">
            {miningRate.toFixed(4)}
          </span>
          <span class="dim" style="font-size:16px;">QUANTA/min</span>
        </div>
        <div class="stat-sub" style="margin-top:8px;">
          ≈ {(miningRate * 60).toFixed(2)} / heure · {(miningRate * 1440).toFixed(0)} / jour
        </div>
      </div>
      <div style="text-align:right;">
        <div class="stat-label">Score de confiance</div>
        <div class="stat-val" style="font-size:36px;color:{trustScore > 80 ? '#22c55e' : '#f97316'};">
          {trustScore}<span style="font-size:18px;font-weight:400;color:var(--color-text-2);">%</span>
        </div>
        <div style="width:120px;margin-left:auto;margin-top:8px;">
          <div class="trust-bar-bg">
            <div class="trust-bar-fill" style="width:{trustScore}%;"></div>
          </div>
        </div>
      </div>
    </div>
    <div style="margin-top:20px;">
      <div style="display:flex;justify-content:space-between;margin-bottom:4px;">
        <span style="font-size:11px;color:rgba(0,229,204,0.5);letter-spacing:0.04em;text-transform:uppercase;">Total miné dans le temps</span>
        {#if sparkData.length > 1}
          <span style="font-size:11px;color:var(--color-text-3);">
            {Math.min(...sparkData).toFixed(2)} → {Math.max(...sparkData).toFixed(2)} QNT
          </span>
        {/if}
      </div>
      {#if sparkData.length < 2}
        <div style="height:56px;display:flex;align-items:center;justify-content:center;color:var(--color-text-3);font-size:12px;">
          Premier point dans une minute…
        </div>
      {:else}
        <canvas bind:this={sparkCanvas} width={400} height={56} style="width:100%;height:56px;"></canvas>
      {/if}
    </div>
  </div>

  <!-- Stats row -->
  <div class="grid-4" style="margin-bottom:12px;">
    <div class="card">
      <div class="stat-label">Énergie</div>
      <div class="stat-val sm mono">{energyKwh.toFixed(1)}<span style="font-size:12px;color:var(--color-text-2);margin-left:4px;">kWh</span></div>
      <div class="stat-sub">depuis le début</div>
    </div>
    <div class="card">
      <div class="stat-label">Uptime</div>
      <div class="stat-val sm mono">{formatUptime(uptime)}</div>
      <div class="stat-sub">nœud actif</div>
    </div>
    <div class="card">
      <div class="stat-label">Peers</div>
      <div class="stat-val sm mono" style="color:var(--cyan);">{peers}</div>
      <div class="stat-sub">connectés</div>
    </div>
    <div class="card">
      <div class="stat-label">Hauteur</div>
      <div class="stat-val sm mono">{chainHeight.toLocaleString('fr-FR')}</div>
      <div class="stat-sub">blocs</div>
    </div>
  </div>

  <!-- Contribution + Balance -->
  <div class="grid-2">
    <div class="card">
      <div class="card-title">Contribution</div>
      <div style="display:flex;flex-direction:column;gap:14px;">
        <div style="display:flex;justify-content:space-between;align-items:center;">
          <span class="dim" style="font-size:13px;">Total miné</span>
          <span class="mono" style="font-size:16px;font-weight:700;">{earned.toFixed(2)} <span style="color:var(--color-text-3);font-weight:400;font-size:12px;">QNT</span></span>
        </div>
        <div style="display:flex;justify-content:space-between;align-items:center;">
          <span class="dim" style="font-size:13px;">Staké</span>
          <span class="mono" style="font-size:16px;font-weight:700;color:#8b5cf6;">{staked.toFixed(2)} <span style="color:var(--color-text-3);font-weight:400;font-size:12px;">QNT</span></span>
        </div>
      </div>
    </div>
    <div class="card">
      <div class="card-title">Solde disponible</div>
      <div class="stat-val" style="font-size:34px;margin-bottom:4px;">{balance.toFixed(2)}</div>
      <div style="font-size:13px;color:var(--color-text-2);font-family:var(--font-mono);margin-bottom:16px;">
        ≈ {(balance * eurPrice).toFixed(2)} EUR
      </div>
      <div class="divider" style="margin:12px 0;"></div>
      <div style="display:flex;gap:8px;">
        <button class="btn btn-primary" style="flex:1;justify-content:center;font-size:12px;">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2 8L14 2L8 14L7 9L2 8Z"/></svg>
          Envoyer
        </button>
        <button class="btn btn-ghost" style="flex:1;justify-content:center;font-size:12px;">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 2v10M4 8l4 4 4-4"/><path d="M2 14h12"/></svg>
          Recevoir
        </button>
      </div>
    </div>
  </div>
</div>

<style>
  .mining-hero {
    background: linear-gradient(135deg, #0a0f0e 0%, #050a0a 100%);
    border: 1px solid rgba(0,229,204,0.15);
  }
</style>
