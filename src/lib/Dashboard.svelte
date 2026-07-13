<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import MiningScene from "./three/MiningScene.svelte";
  import { t } from "./i18n.svelte";

  // ── Live node data ──────────────────────────────────────────────
  let miningRate = $state(0);
  let earned = $state(0);
  let trustScore = $state(0);
  let uptime = $state(0);
  let energyKwh = $state(0);
  let peers = $state(0);
  let chainHeight = $state(0);
  let mode = $state("Actif");

  // Offre prouvable
  let maxSupply = $state(100_000_000);
  let minedQta = $state(0);
  let burnedQta = $state(0);
  let circulatingQta = $state(0);
  let pctToCap = $state(0);

  // Émission RÉELLE (get_economy_stats — même fonction que le minage)
  let emissionPerHour = $state(0);

  // Finalité (gadget Casper-FFG vivant)
  interface Finality {
    height: number;
    finalized_floor: number;
    epoch: number;
    epoch_length: number;
    blocks_into_epoch: number;
    validators: number;
    total_staked: number;
    my_stake: number;
    i_am_validator: boolean;
  }
  let fin = $state<Finality | null>(null);

  function fmtQ(n: number) { return n.toLocaleString("fr-FR", { maximumFractionDigits: 0 }); }

  async function refresh() {
    try {
      const r = await invoke<any>("get_my_reputation");
      earned = r?.atn_earned ?? 0;
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
      const o = await invoke<any>("get_chain_overview", { limit: 0 });
      maxSupply = o.max_supply_qta ?? 100_000_000;
      minedQta = o.total_mined_qta ?? 0;
      burnedQta = o.total_burned_qta ?? 0;
      circulatingQta = o.total_supply_qta ?? 0;
      pctToCap = o.pct_to_cap ?? 0;
    } catch {}
    try {
      const e = await invoke<any>("get_economy_stats");
      emissionPerHour = e?.emission_per_hour ?? 0;
    } catch {}
    try {
      const f = await invoke<Finality>("get_finality_status");
      fin = f;
      chainHeight = f.height;
    } catch {}
    if (uptime > 0) miningRate = earned / uptime;
  }

  $effect(() => {
    refresh();
    const iv = setInterval(refresh, 5000);
    return () => clearInterval(iv);
  });

  // ── Courbe d'émission : QUANTA/h en fonction de l'offre émise ────
  // emission_for_tick(m) = (MAX − m) / DIVISOR → droite décroissante vers 0 au
  // plafond. Aucune projection temporelle (elle supposerait un minage continu) :
  // on trace la LOI, pas une promesse.
  let curveCanvas = $state<HTMLCanvasElement | undefined>();
  $effect(() => {
    const cv = curveCanvas;
    if (!cv) return;
    const pct = Math.min(100, Math.max(0, pctToCap));
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const w = cv.clientWidth || 320, h = cv.clientHeight || 96;
    cv.width = w * dpr; cv.height = h * dpr;
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, w, h);
    const padL = 2, padR = 2, padT = 10, padB = 18;
    const x0 = padL, x1 = w - padR, y0 = padT, y1 = h - padB;
    // Aire sous la droite (émission max → 0)
    const grad = ctx.createLinearGradient(0, y0, 0, y1);
    grad.addColorStop(0, "rgba(11,165,160,0.16)");
    grad.addColorStop(1, "rgba(11,165,160,0)");
    ctx.beginPath();
    ctx.moveTo(x0, y0);
    ctx.lineTo(x1, y1);
    ctx.lineTo(x0, y1);
    ctx.closePath();
    ctx.fillStyle = grad;
    ctx.fill();
    ctx.beginPath();
    ctx.moveTo(x0, y0);
    ctx.lineTo(x1, y1);
    ctx.strokeStyle = "#0BA5A0";
    ctx.lineWidth = 1.8;
    ctx.stroke();
    // Point « vous êtes ici »
    const px = x0 + (x1 - x0) * (pct / 100);
    const py = y0 + (y1 - y0) * (pct / 100);
    ctx.beginPath();
    ctx.arc(px, py, 4.5, 0, Math.PI * 2);
    ctx.fillStyle = "#0BA5A0";
    ctx.fill();
    ctx.beginPath();
    ctx.arc(px, py, 8, 0, Math.PI * 2);
    ctx.strokeStyle = "rgba(11,165,160,0.35)";
    ctx.lineWidth = 1.5;
    ctx.stroke();
    // Axes minimalistes
    ctx.fillStyle = "rgba(110,110,115,0.9)";
    ctx.font = "10px Inter, sans-serif";
    ctx.fillText("0 %", x0, h - 5);
    const cap = "100 %";
    ctx.fillText(cap, x1 - ctx.measureText(cap).width, h - 5);
  });

  function formatUptime(min: number) {
    const h = Math.floor(min / 60);
    const m = min % 60;
    return `${h}h${m.toString().padStart(2, "0")}`;
  }

  const modeColors: Record<string, string> = { Actif: "tag-green", Guardian: "tag-cyan", Recherche: "tag-orange" };
  const epochPct = $derived(fin ? (fin.blocks_into_epoch / fin.epoch_length) * 100 : 0);

  // Répartition Shapley — les 4 contributions mesurées (constantes du protocole).
  const SHAPLEY = [
    { key: "energy", pct: 30, cls: "sh-teal" },
    { key: "work", pct: 30, cls: "sh-blue" },
    { key: "validation", pct: 25, cls: "sh-violet" },
    { key: "uptime", pct: 15, cls: "sh-amber" },
  ] as const;
</script>

<div class="page">
  <div class="page-header">
    <div>
      <div class="page-title">{t('mine.title')}</div>
      <div class="page-sub">{t('mine.subtitle')}</div>
    </div>
    <span class="tag {modeColors[mode] ?? 'tag-dim'}">
      {mode === 'Actif' ? t('db.mode.actif') : mode === 'Guardian' ? t('db.mode.guardian') : mode === 'Recherche' ? t('db.mode.research') : mode}
    </span>
  </div>

  <!-- ── Hero : le réseau vivant + votre rythme de forge ── -->
  <div class="card mine-hero">
    <div class="mh-torus">
      <MiningScene height={250} {peers} />
      <div class="mh-live">
        <span class="mh-live-dot"></span>
        {t('mine.hero.live')}
      </div>
    </div>
    <div class="mh-stats">
      <div class="mh-main">
        <div class="stat-label">{t('mine.hero.rate')}</div>
        <div class="mh-rate">
          <span class="mono mh-rate-num">{miningRate.toFixed(4)}</span>
          <span class="mh-rate-unit">QUANTA/min</span>
        </div>
        <div class="stat-sub">
          ≈ {(miningRate * 60).toFixed(2)} {t('db.per_hour')} · {(miningRate * 1440).toFixed(0)} {t('db.per_day')}
        </div>
      </div>
      <div class="mh-side">
        <div class="mh-cell">
          <div class="stat-label">{t('mine.hero.forged')}</div>
          <div class="mono mh-cell-v">+{earned.toFixed(2)}</div>
        </div>
        <div class="mh-cell">
          <div class="stat-label">{t('db.trust_score')}</div>
          <div class="mono mh-cell-v" style="color:{trustScore > 80 ? 'var(--color-green)' : 'var(--color-amber)'};">{trustScore}%</div>
        </div>
      </div>
    </div>
    <p class="mh-explain">{t('mine.hero.explain')}</p>
  </div>

  <!-- ── Stats row ── -->
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

  <div class="grid-2" style="margin-bottom:12px;">
    <!-- ── Pourquoi je gagne ? (Shapley, en langage simple) ── -->
    <div class="card">
      <div class="card-title">{t('mine.why.title')}</div>
      <p class="mine-p">{t('mine.why.intro')}</p>
      <div class="sh-list">
        {#each SHAPLEY as s}
          <div class="sh-row">
            <div class="sh-head">
              <span class="sh-name">{t(`mine.why.${s.key}` as any)}</span>
              <span class="sh-pct mono">{s.pct}%</span>
            </div>
            <div class="sh-bar"><div class="sh-fill {s.cls}" style="width:{s.pct / 0.30}%"></div></div>
            <div class="sh-sub">{t(`mine.why.${s.key}.sub` as any)}</div>
          </div>
        {/each}
      </div>
    </div>

    <!-- ── Émission réelle, décroissante vers le plafond ── -->
    <div class="card">
      <div class="card-title">{t('mine.emission.title')}</div>
      <div class="em-now">
        <span class="mono em-val">{emissionPerHour.toFixed(2)}</span>
        <span class="em-unit">QUANTA/h · {t('mine.emission.network')}</span>
      </div>
      <canvas bind:this={curveCanvas} class="em-curve" aria-label={t('mine.emission.curveAria')}></canvas>
      <p class="mine-p" style="margin-top:10px;">{t('mine.emission.explain')}</p>
    </div>
  </div>

  <!-- ── Finalité — l'histoire gravée (Casper-FFG) ── -->
  <div class="card" style="margin-bottom:12px;">
    <div class="card-title">{t('mine.fin.title')}</div>
    {#if fin}
      <div class="fin-grid">
        <div class="fin-left">
          <div class="fin-epoch-head">
            <span>{t('mine.fin.epoch')} <span class="mono">{fin.epoch}</span></span>
            <span class="mono dim">{fin.blocks_into_epoch}/{fin.epoch_length}</span>
          </div>
          <div class="fin-bar"><div class="fin-fill" style="width:{epochPct}%"></div></div>
          <div class="fin-floor">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="3" y="7" width="10" height="7" rx="1.5"/><path d="M5 7V5a3 3 0 016 0v2"/></svg>
            <span>{t('mine.fin.floor')} <b class="mono">{fin.finalized_floor.toLocaleString('fr-FR')}</b></span>
          </div>
          <p class="mine-p">{t('mine.fin.explain')}</p>
        </div>
        <div class="fin-right">
          <div class="fin-stat">
            <div class="stat-label">{t('mine.fin.validators')}</div>
            <div class="mono fin-stat-v">{fin.validators}</div>
          </div>
          <div class="fin-stat">
            <div class="stat-label">{t('mine.fin.staked')}</div>
            <div class="mono fin-stat-v">{fmtQ(fin.total_staked)} <span class="fin-stat-u">QTA</span></div>
          </div>
          {#if fin.i_am_validator}
            <div class="fin-you ok">✓ {t('mine.fin.youAre')}</div>
          {:else}
            <div class="fin-you">{t('mine.fin.become')}</div>
          {/if}
        </div>
      </div>
    {:else}
      <div class="mine-p dim">{t('loading')}</div>
    {/if}
  </div>

  <!-- ── Monnaie QUANTA — offre prouvable (confiance) ── -->
  <div class="card">
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
  .mine-p { font-size: 12.5px; color: var(--color-text-2); line-height: 1.55; }

  /* Hero */
  .mine-hero { margin-bottom: 12px; padding: 0 0 18px; overflow: hidden; }
  .mh-torus { position: relative; }
  .mh-live {
    position: absolute; top: 14px; left: 16px;
    display: inline-flex; align-items: center; gap: 7px;
    font-size: 11px; font-weight: 600; letter-spacing: 0.05em;
    text-transform: uppercase; color: var(--color-text-2);
    background: rgba(255,255,255,0.75); backdrop-filter: blur(4px);
    border: 1px solid var(--color-border);
    padding: 5px 10px; border-radius: 999px;
  }
  .mh-live-dot {
    width: 7px; height: 7px; border-radius: 50%;
    background: var(--color-green);
    animation: pulse-ring 2s ease infinite;
  }
  .mh-stats {
    display: flex; align-items: flex-end; justify-content: space-between;
    gap: 20px; padding: 4px 24px 0; flex-wrap: wrap;
  }
  .mh-rate { display: flex; align-items: baseline; gap: 10px; margin-top: 6px; }
  .mh-rate-num {
    font-size: 46px; font-weight: 700; color: var(--color-accent);
    line-height: 1; letter-spacing: -0.02em;
  }
  .mh-rate-unit { font-size: 15px; color: var(--color-text-2); }
  .mh-side { display: flex; gap: 28px; }
  .mh-cell-v { font-size: 22px; font-weight: 700; margin-top: 4px; }
  .mh-explain { font-size: 12.5px; color: var(--color-text-2); line-height: 1.55; padding: 14px 24px 0; margin: 0; }

  /* Shapley */
  .sh-list { display: flex; flex-direction: column; gap: 13px; margin-top: 14px; }
  .sh-head { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 5px; }
  .sh-name { font-size: 13px; font-weight: 600; color: var(--color-text-0); }
  .sh-pct { font-size: 12px; color: var(--color-text-2); }
  .sh-bar { height: 6px; background: var(--color-bg-3); border-radius: 3px; overflow: hidden; }
  .sh-fill { height: 100%; border-radius: 3px; }
  .sh-teal   { background: #0BA5A0; }
  .sh-blue   { background: #3D6FE0; }
  .sh-violet { background: #7C3AED; }
  .sh-amber  { background: #E8810C; }
  .sh-sub { font-size: 11.5px; color: var(--color-text-2); margin-top: 4px; line-height: 1.45; }

  /* Émission */
  .em-now { display: flex; align-items: baseline; gap: 8px; margin-bottom: 12px; }
  .em-val { font-size: 30px; font-weight: 700; color: var(--color-text-0); }
  .em-unit { font-size: 12px; color: var(--color-text-2); }
  .em-curve { width: 100%; height: 110px; display: block; }

  /* Finalité */
  .fin-grid { display: grid; grid-template-columns: 1.6fr 1fr; gap: 24px; }
  @media (max-width: 720px) { .fin-grid { grid-template-columns: 1fr; } }
  .fin-epoch-head {
    display: flex; justify-content: space-between; align-items: baseline;
    font-size: 13px; font-weight: 600; margin-bottom: 8px;
  }
  .fin-bar { height: 8px; background: var(--color-bg-3); border-radius: 4px; overflow: hidden; }
  .fin-fill { height: 100%; background: linear-gradient(90deg, #0BA5A0, #14C8B8); border-radius: 4px; transition: width 0.8s ease; }
  .fin-floor {
    display: flex; align-items: center; gap: 8px;
    margin: 14px 0 10px; font-size: 13px; color: var(--color-text-1);
  }
  .fin-floor svg { color: var(--color-accent); flex-shrink: 0; }
  .fin-right { display: flex; flex-direction: column; gap: 14px; }
  .fin-stat-v { font-size: 22px; font-weight: 700; margin-top: 4px; }
  .fin-stat-u { font-size: 12px; color: var(--color-text-2); font-weight: 400; }
  .fin-you {
    font-size: 12px; color: var(--color-text-2);
    padding: 8px 12px; background: var(--color-bg-2);
    border-radius: 8px; line-height: 1.5;
  }
  .fin-you.ok { color: var(--color-green); font-weight: 600; background: rgba(22,163,74,0.07); }

  /* Offre (reprise de la carte existante) */
  .supply-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 14px; margin-bottom: 14px; }
  .sup-k { font-size: 11px; color: var(--color-text-3); text-transform: uppercase; letter-spacing: .04em; margin-bottom: 4px; }
  .sup-v { font-size: 20px; font-weight: 700; color: var(--color-text-0); }
  .sup-bar { height: 8px; background: var(--color-bg-3); border-radius: 4px; overflow: hidden; }
  .sup-fill { height: 100%; background: var(--color-accent); border-radius: 4px; transition: width 1.2s ease; }
  .sup-cap-line { font-size: 12px; color: var(--color-text-2); margin-top: 6px; }
  .sup-trust { display: flex; flex-wrap: wrap; gap: 14px; margin-top: 14px; font-size: 12px; color: var(--color-green); font-weight: 600; }
  @media (max-width: 720px) { .supply-grid { grid-template-columns: repeat(2, 1fr); } }
</style>
