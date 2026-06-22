<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "./i18n.svelte";

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
  // TOKENOMICS v2 — offre prouvable
  let maxSupply = $state(100_000_000);
  let minedQta = $state(0);
  let burnedQta = $state(0);
  let circulatingQta = $state(0);
  let pctToCap = $state(0);
  function fmtQ(n: number) { return n.toLocaleString('fr-FR', { maximumFractionDigits: 0 }); }

  // Sparkline tracks balance snapshots every 30s
  let sparkData = $state<number[]>([]);
  let sparkTick = 0;
  let sparkCanvas = $state<HTMLCanvasElement | undefined>();

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
      const o = await invoke<any>("get_chain_overview", { limit: 0 });
      maxSupply = o.max_supply_qta ?? 100_000_000;
      minedQta = o.total_mined_qta ?? 0;
      burnedQta = o.total_burned_qta ?? 0;
      circulatingQta = o.total_supply_qta ?? 0;
      pctToCap = o.pct_to_cap ?? 0;
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
    const iv = setInterval(refresh, 5000);
    return () => clearInterval(iv);
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
    grad.addColorStop(0, 'rgba(11,165,160,0.18)');
    grad.addColorStop(1, 'rgba(11,165,160,0)');
    ctx.beginPath();
    pts.forEach(([x, y], i) => i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y));
    ctx.lineTo(w, h); ctx.lineTo(0, h); ctx.closePath();
    ctx.fillStyle = grad; ctx.fill();
    // Line
    ctx.beginPath();
    pts.forEach(([x, y], i) => i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y));
    ctx.strokeStyle = '#0BA5A0'; ctx.lineWidth = 1.5; ctx.stroke();
    // Dot
    const [lx, ly] = pts[pts.length - 1];
    ctx.beginPath(); ctx.arc(lx, ly, 3, 0, Math.PI * 2);
    ctx.fillStyle = '#0BA5A0'; ctx.fill();
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
      <div class="page-title">{t('db.title')}</div>
      <div class="page-sub">{t('db.subtitle')}</div>
    </div>
    <span class="tag {modeColors[mode] ?? 'tag-dim'}">{mode === 'Actif' ? t('db.mode.actif') : mode === 'Guardian' ? t('db.mode.guardian') : mode === 'Recherche' ? t('db.mode.research') : mode}</span>
  </div>

  <!-- Mining rate hero -->
  <div class="card mining-hero" style="margin-bottom:12px;">
    <div style="display:flex;align-items:flex-start;justify-content:space-between;">
      <div>
        <div class="stat-label" style="color:var(--color-text-2);letter-spacing:0.06em;">{t('db.mining_rate')}</div>
        <div style="display:flex;align-items:baseline;gap:10px;margin-top:6px;">
          <span class="mono" style="font-size:52px;font-weight:700;color:var(--color-accent);line-height:1;letter-spacing:-0.02em;">
            {miningRate.toFixed(4)}
          </span>
          <span class="dim" style="font-size:16px;">QUANTA/min</span>
        </div>
        <div class="stat-sub" style="margin-top:8px;">
          ≈ {(miningRate * 60).toFixed(2)} {t('db.per_hour')} · {(miningRate * 1440).toFixed(0)} {t('db.per_day')}
        </div>
      </div>
      <div style="text-align:right;">
        <div class="stat-label">{t('db.trust_score')}</div>
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
        <span style="font-size:11px;color:var(--color-text-3);letter-spacing:0.04em;text-transform:uppercase;">{t('db.mined_over_time')}</span>
        {#if sparkData.length > 1}
          <span style="font-size:11px;color:var(--color-text-3);">
            {Math.min(...sparkData).toFixed(2)} → {Math.max(...sparkData).toFixed(2)} QNT
          </span>
        {/if}
      </div>
      {#if sparkData.length < 2}
        <div style="height:56px;display:flex;align-items:center;justify-content:center;color:var(--color-text-3);font-size:12px;">
          {t('db.first_point')}
        </div>
      {:else}
        <canvas bind:this={sparkCanvas} width={400} height={56} style="width:100%;height:56px;"></canvas>
      {/if}
    </div>
  </div>

  <!-- Stats row -->
  <div class="grid-4" style="margin-bottom:12px;">
    <div class="card">
      <div class="stat-label">{t('db.energy')}</div>
      <div class="stat-val sm mono">{energyKwh.toFixed(1)}<span style="font-size:12px;color:var(--color-text-2);margin-left:4px;">kWh</span></div>
      <div class="stat-sub">{t('db.since_start')}</div>
    </div>
    <div class="card">
      <div class="stat-label">{t('db.uptime')}</div>
      <div class="stat-val sm mono">{formatUptime(uptime)}</div>
      <div class="stat-sub">{t('db.node_active')}</div>
    </div>
    <div class="card">
      <div class="stat-label">{t('db.peers')}</div>
      <div class="stat-val sm mono" style="color:var(--cyan);">{peers}</div>
      <div class="stat-sub">{t('db.connected')}</div>
    </div>
    <div class="card">
      <div class="stat-label">{t('db.height')}</div>
      <div class="stat-val sm mono">{chainHeight.toLocaleString('fr-FR')}</div>
      <div class="stat-sub">{t('db.blocks')}</div>
    </div>
  </div>

  <!-- Contribution + Balance -->
  <div class="grid-2">
    <div class="card">
      <div class="card-title">{t('db.contribution')}</div>
      <div style="display:flex;flex-direction:column;gap:14px;">
        <div style="display:flex;justify-content:space-between;align-items:center;">
          <span class="dim" style="font-size:13px;">{t('db.total_mined')}</span>
          <span class="mono" style="font-size:16px;font-weight:700;">{earned.toFixed(2)} <span style="color:var(--color-text-3);font-weight:400;font-size:12px;">QNT</span></span>
        </div>
        <div style="display:flex;justify-content:space-between;align-items:center;">
          <span class="dim" style="font-size:13px;">{t('db.staked')}</span>
          <span class="mono" style="font-size:16px;font-weight:700;color:#8b5cf6;">{staked.toFixed(2)} <span style="color:var(--color-text-3);font-weight:400;font-size:12px;">QNT</span></span>
        </div>
      </div>
    </div>
    <div class="card">
      <div class="card-title">{t('db.available_balance')}</div>
      <div class="stat-val" style="font-size:34px;margin-bottom:4px;">{balance.toFixed(2)}</div>
      <div style="font-size:13px;color:var(--color-text-2);font-family:var(--font-mono);margin-bottom:16px;">
        {circulatingQta > 0 ? (balance / circulatingQta * 100).toFixed(2) : "0.00"}{t('db.pct_of_circulating')}
      </div>
      <div class="divider" style="margin:12px 0;"></div>
      <div style="display:flex;gap:8px;">
        <button class="btn btn-primary" style="flex:1;justify-content:center;font-size:12px;">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2 8L14 2L8 14L7 9L2 8Z"/></svg>
          {t('db.send')}
        </button>
        <button class="btn btn-ghost" style="flex:1;justify-content:center;font-size:12px;">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 2v10M4 8l4 4 4-4"/><path d="M2 14h12"/></svg>
          {t('db.receive')}
        </button>
      </div>
    </div>
  </div>

  <!-- Monnaie QUANTA — offre prouvable (confiance) -->
  <div class="card" style="margin-top:12px;">
    <div class="card-title">{t('db.currency_title')}</div>
    <div class="supply-grid">
      <div><div class="sup-k">{t('db.hard_cap')}</div><div class="sup-v mono">{fmtQ(maxSupply)}</div></div>
      <div><div class="sup-k">{t('db.issued')}</div><div class="sup-v mono">{fmtQ(minedQta)}</div></div>
      <div><div class="sup-k">{t('db.burned')}</div><div class="sup-v mono" style="color:var(--color-amber);">{fmtQ(burnedQta)}</div></div>
      <div><div class="sup-k">{t('db.circulating')}</div><div class="sup-v mono">{fmtQ(circulatingQta)}</div></div>
    </div>
    <div class="sup-bar"><div class="sup-fill" style="width:{Math.min(100, Math.max(pctToCap, pctToCap > 0 ? 0.5 : 0))}%;"></div></div>
    <div class="sup-cap-line">{pctToCap < 0.01 && pctToCap > 0 ? '<0,01' : pctToCap.toFixed(2)}{t('db.cap_issued')} · {t('db.deflationary')}</div>
    <div class="sup-trust">
      <span>✓ {t('db.no_authority')}</span>
      <span>✓ {t('db.no_premine')}</span>
      <span>✓ {t('db.policy_in_code')}</span>
    </div>
  </div>
</div>

<style>
  .supply-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 14px; margin-bottom: 14px; }
  .sup-k { font-size: 11px; color: var(--color-text-3); text-transform: uppercase; letter-spacing: .04em; margin-bottom: 4px; }
  .sup-v { font-size: 20px; font-weight: 700; color: var(--color-text-0); }
  .sup-bar { height: 8px; background: var(--color-bg-3); border-radius: 4px; overflow: hidden; }
  .sup-fill { height: 100%; background: var(--color-accent); border-radius: 4px; transition: width 1.2s ease; }
  .sup-cap-line { font-size: 12px; color: var(--color-text-2); margin-top: 6px; }
  .sup-trust { display: flex; flex-wrap: wrap; gap: 14px; margin-top: 14px; font-size: 12px; color: var(--color-green); font-weight: 600; }
  @media (max-width: 720px) { .supply-grid { grid-template-columns: repeat(2, 1fr); } }
  .mining-hero {
    background: var(--color-bg-1);
    border: 1px solid var(--color-border);
  }
</style>
