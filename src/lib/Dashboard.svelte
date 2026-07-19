<script lang="ts">
  import ForgeEngine from "./ForgeEngine.svelte";
  import { t, locale } from "./i18n.svelte";
  import { TICKER, TEAL, formatUptime } from "./quanta";
  import { myReputation, nodeStatus, chainOverview, finalityStatus, economyStats } from "./stores.svelte";

  // Alpha variants of TEAL derived from the single brand constant — no
  // duplicated rgba() literals for the canvas gradient/halo.
  const TEAL_VEIL = TEAL + "29"; // ≈ rgba(11,165,160,0.16)
  const TEAL_TRANSPARENT = TEAL + "00";
  const TEAL_GLOW = TEAL + "59"; // ≈ rgba(11,165,160,0.35)

  // ── Données vivantes du nœud (ZÉRO énergie, ZÉRO 3D) — stores partagés,
  //    UN sondage par donnée (cf. stores.svelte.ts). Le Réseau partage nodeStatus
  //    + chaîne/finalité ; on n'ouvre plus d'interval local ici.
  $effect(() => myReputation.subscribe());
  $effect(() => nodeStatus.subscribe());
  $effect(() => chainOverview.subscribe());
  $effect(() => finalityStatus.subscribe());
  $effect(() => economyStats.subscribe());

  const earned = $derived(myReputation.value?.atn_earned ?? 0);
  const uptime = $derived(myReputation.value?.uptime_minutes ?? 0);
  const peers = $derived(nodeStatus.value?.peer_count ?? 0);
  const mode = $derived(nodeStatus.value?.mode ?? "Actif");

  // Offre prouvable (get_chain_overview)
  const maxSupply = $derived(chainOverview.value?.max_supply_qta ?? 100_000_000);
  const minedQta = $derived(chainOverview.value?.total_mined_qta ?? 0);
  const burnedQta = $derived(chainOverview.value?.total_burned_qta ?? 0);
  const circulatingQta = $derived(chainOverview.value?.total_supply_qta ?? 0);
  const pctToCap = $derived(chainOverview.value?.pct_to_cap ?? 0);

  // Émission RÉELLE (get_economy_stats — même fonction que le minage)
  const emissionPerHour = $derived(economyStats.value?.emission_per_hour ?? 0);

  // Finalité (gadget Casper-FFG vivant)
  const fin = $derived(finalityStatus.value);
  const chainHeight = $derived(fin?.height ?? 0);

  // ── État de chargement honnête : « — » tant que rien n'est confirmé,
  //    ligne d'erreur discrète si un sondage échoue (agrégat des stores lus).
  const loaded = $derived(
    myReputation.loaded && nodeStatus.loaded && chainOverview.loaded &&
    finalityStatus.loaded && economyStats.loaded,
  );
  const loadError = $derived(
    myReputation.error || nodeStatus.error || chainOverview.error ||
    finalityStatus.error || economyStats.error,
  );

  let miningRate = $derived(uptime > 0 ? earned / uptime : 0);

  // Formateur d'offre : entiers seuls (grands nombres) — distinct de `fmtQ`
  // de quanta.ts (2..6 décimales, pour les montants du portefeuille).
  function fmtQ(n: number) { return n.toLocaleString("fr-FR", { maximumFractionDigits: 0 }); }

  // ── Courbe d'émission : QUANTA/h en fonction de l'offre émise ────
  // emission_for_tick(m) = (MAX − m) / DIVISOR → droite décroissante vers 0 au
  // plafond. On trace la LOI (pas une projection temporelle).
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
    const grad = ctx.createLinearGradient(0, y0, 0, y1);
    grad.addColorStop(0, TEAL_VEIL);
    grad.addColorStop(1, TEAL_TRANSPARENT);
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
    ctx.strokeStyle = TEAL;
    ctx.lineWidth = 1.8;
    ctx.stroke();
    const px = x0 + (x1 - x0) * (pct / 100);
    const py = y0 + (y1 - y0) * (pct / 100);
    ctx.beginPath();
    ctx.arc(px, py, 4.5, 0, Math.PI * 2);
    ctx.fillStyle = TEAL;
    ctx.fill();
    ctx.beginPath();
    ctx.arc(px, py, 8, 0, Math.PI * 2);
    ctx.strokeStyle = TEAL_GLOW;
    ctx.lineWidth = 1.5;
    ctx.stroke();
    ctx.fillStyle = "rgba(110,110,115,0.9)";
    ctx.font = "10px 'Inter Variable', Inter, sans-serif";
    ctx.fillText("0 %", x0, h - 5);
    const cap = "100 %";
    ctx.fillText(cap, x1 - ctx.measureText(cap).width, h - 5);
  });

  const modeColors: Record<string, string> = { Actif: "tag-cyan", Guardian: "tag-cyan", Recherche: "tag-dim" };
  const epochPct = $derived(fin ? (fin.blocks_into_epoch / fin.epoch_length) * 100 : 0);

  // ── Explicatif honnête « comprendre ton minage » (dict local 6 langues,
  //    même patron que ForgeEngine) — répond exactement à : c'est quoi,
  //    qu'est-ce que ça prouve, est-ce gratuit. ─────────────────────────────
  const EX: Record<string, Record<string, string>> = {
    en: {
      understand: "Understanding your mining",
      howK: "No hash race",
      howV: "Quanta is Proof-of-Stake — not Bitcoin's brute-force race. No electricity is wasted, no mining farm needed. Any Mac or PC that stays online earns newly-minted QUANTA, on a schedule that shrinks toward a hard cap of 100M.",
      workK: "What your node does",
      workV: "It verifies ML-DSA signatures, validates and relays transactions, seals a block when it is elected leader, and votes on finality. Watch it happen live below — every line is a real event, nothing simulated.",
      proofK: "What it proves",
      proofV: "Every block is signed and chained. Once two-thirds of the stake vote, a block is finalized — irreversible forever (Casper-FFG). Your balance is a pure function of the chain that every node re-verifies. Nobody can invent it or erase it.",
    },
    fr: {
      understand: "Comprendre ton minage",
      howK: "Pas de course au hash",
      howV: "Quanta est en Proof-of-Stake — pas la course en force brute du Bitcoin. Aucune électricité gaspillée, aucune ferme de minage. N'importe quel Mac ou PC qui reste en ligne gagne des QUANTA fraîchement émis, selon une cadence qui décroît vers un plafond dur de 100M.",
      workK: "Ce que fait ton nœud",
      workV: "Il vérifie les signatures ML-DSA, valide et relaie les transactions, scelle un bloc quand il est élu leader, et vote la finalité. Regarde-le en direct ci-dessous — chaque ligne est un évènement réel, rien de simulé.",
      proofK: "Ce que ça prouve",
      proofV: "Chaque bloc est signé et chaîné. Dès que deux tiers de l'enjeu votent, un bloc est finalisé — irréversible à jamais (Casper-FFG). Ton solde est une fonction pure de la chaîne, revérifiée par chaque nœud. Personne ne peut l'inventer ni l'effacer.",
    },
    es: {
      understand: "Entender tu minería",
      howK: "Sin carrera de hashes",
      howV: "Quanta usa Proof-of-Stake — no la carrera de fuerza bruta de Bitcoin. No se gasta electricidad, no hace falta una granja. Cualquier Mac o PC que siga en línea gana QUANTA recién emitidos, con una cadencia que decrece hacia un tope duro de 100M.",
      workK: "Qué hace tu nodo",
      workV: "Verifica firmas ML-DSA, valida y retransmite transacciones, sella un bloque cuando es elegido líder, y vota la finalidad. Míralo en vivo abajo — cada línea es un evento real, nada simulado.",
      proofK: "Qué demuestra",
      proofV: "Cada bloque va firmado y encadenado. Cuando dos tercios del stake votan, un bloque se finaliza — irreversible para siempre (Casper-FFG). Tu saldo es una función pura de la cadena que cada nodo re-verifica. Nadie puede inventarlo ni borrarlo.",
    },
    ru: {
      understand: "Как работает твой майнинг",
      howK: "Без гонки хэшей",
      howV: "Quanta работает на Proof-of-Stake — без биткойновской гонки грубой силы. Электричество не тратится, ферма не нужна. Любой Mac или PC, оставаясь в сети, получает свежеэмитированные QUANTA по графику, убывающему к жёсткому потолку 100M.",
      workK: "Что делает твой узел",
      workV: "Он проверяет подписи ML-DSA, валидирует и ретранслирует транзакции, запечатывает блок, когда избран лидером, и голосует за финальность. Смотри вживую ниже — каждая строка реальна, ничего не симулировано.",
      proofK: "Что это доказывает",
      proofV: "Каждый блок подписан и связан. Как только две трети стейка проголосуют, блок финализирован — необратимо навсегда (Casper-FFG). Твой баланс — чистая функция цепи, перепроверяемая каждым узлом. Никто не может его выдумать или стереть.",
    },
    zh: {
      understand: "理解你的挖矿",
      howK: "没有哈希竞赛",
      howV: "Quanta 采用权益证明（PoS）——不是比特币的暴力竞赛。不浪费电，不需要矿场。任何保持在线的 Mac 或 PC 都能获得新铸造的 QUANTA，其发放速率逐步递减，趋向 1 亿的硬顶。",
      workK: "你的节点在做什么",
      workV: "它验证 ML-DSA 签名、校验并转发交易、在被选为出块者时封存区块，并对最终性投票。在下方实时观看——每一行都是真实事件，绝无模拟。",
      proofK: "它证明了什么",
      proofV: "每个区块都经签名并链接。一旦三分之二的权益投票，区块即被最终确定——永久不可逆（Casper-FFG）。你的余额是链的纯函数，由每个节点重新验证。没人能凭空捏造或抹除它。",
    },
    ja: {
      understand: "あなたのマイニングを理解する",
      howK: "ハッシュ競争なし",
      howV: "Quanta はプルーフ・オブ・ステーク——ビットコインの力任せの競争ではありません。電力の浪費も、マイニングファームも不要。オンラインを保つ Mac や PC は、1 億の上限へ向けて逓減するペースで新規発行の QUANTA を得ます。",
      workK: "ノードがすること",
      workV: "ML-DSA 署名を検証し、取引を検証・中継し、リーダーに選ばれたらブロックを封印し、ファイナリティに投票します。下でライブで見られます——各行は実イベントで、シミュレーションはありません。",
      proofK: "それが証明すること",
      proofV: "各ブロックは署名され連鎖します。ステークの三分の二が投票すると、ブロックは確定——永久に不可逆です（Casper-FFG）。残高はチェーンの純粋な関数で、各ノードが再検証します。誰も捏造も消去もできません。",
    },
  };
  function tx(key: string): string {
    const loc = locale();
    return EX[loc]?.[key] ?? EX.en[key] ?? key;
  }
</script>

<div class="page">
  <div class="page-header">
    <div>
      <div class="page-title">{t('mine.title')}</div>
      <div class="page-sub">{t('mine.subtitle')}</div>
      {#if loadError}
        <div class="load-err">{t('common.errLoad')}</div>
      {/if}
    </div>
    <span class="tag {modeColors[mode] ?? 'tag-dim'}">
      {mode === 'Actif' ? t('db.mode.actif') : mode === 'Guardian' ? t('db.mode.guardian') : mode === 'Recherche' ? t('db.mode.research') : mode}
    </span>
  </div>

  <!-- ── Le moteur de consensus, en direct — LA pièce maîtresse ──── -->
  <div class="forge">
    <ForgeEngine />
  </div>

  <!-- ── Comprendre ton minage (honnête, compact) ─────────────────── -->
  <div class="card understand">
    <div class="card-title">{tx('understand')}</div>
    <div class="ex-grid">
      <div class="ex">
        <div class="ex-glyph" aria-hidden="true">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2 4 14h7l-1 8 9-12h-7l1-8z"/></svg>
        </div>
        <div class="ex-k">{tx('howK')}</div>
        <div class="ex-v">{tx('howV')}</div>
      </div>
      <div class="ex">
        <div class="ex-glyph" aria-hidden="true">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l7 4v5c0 4-3 7-7 8-4-1-7-4-7-8V7l7-4z"/><path d="M9.5 12l1.8 1.8L15 10"/></svg>
        </div>
        <div class="ex-k">{tx('workK')}</div>
        <div class="ex-v">{tx('workV')}</div>
      </div>
      <div class="ex">
        <div class="ex-glyph" aria-hidden="true">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="10" width="16" height="11" rx="2"/><path d="M8 10V7a4 4 0 018 0v3"/></svg>
        </div>
        <div class="ex-k">{tx('proofK')}</div>
        <div class="ex-v">{tx('proofV')}</div>
      </div>
    </div>
  </div>

  <!-- ── Chiffres du nœud (zéro énergie) ───────────────────────────── -->
  <div class="grid-4 stats-row">
    <div class="card stat-card">
      <div class="stat-label">{t('mine.hero.forged')}</div>
      <div class="stat-val sm">{loaded ? `+${earned.toFixed(2)}` : '—'}</div>
      <div class="stat-sub">≈ {(miningRate * 1440).toFixed(2)} {t('db.per_day')}</div>
    </div>
    <div class="card stat-card">
      <div class="stat-label">{t('db.uptime')}</div>
      <div class="stat-val sm">{loaded ? formatUptime(uptime) : '—'}</div>
      <div class="stat-sub">{t('db.node_active')}</div>
    </div>
    <div class="card stat-card">
      <div class="stat-label">{t('db.peers')}</div>
      <div class="stat-val sm">{loaded ? peers : '—'}</div>
      <div class="stat-sub">{t('db.connected')}</div>
    </div>
    <div class="card stat-card">
      <div class="stat-label">{t('db.height')}</div>
      <div class="stat-val sm">{loaded ? chainHeight.toLocaleString('fr-FR') : '—'}</div>
      <div class="stat-sub">{t('db.blocks')}</div>
    </div>
  </div>

  <!-- ── Finalité — l'histoire gravée (Casper-FFG), ce qui prouve ──── -->
  <div class="card fin-card">
    <div class="card-title">{t('mine.fin.title')}</div>
    {#if fin}
      <div class="fin-grid">
        <div class="fin-left">
          <div class="fin-epoch-head">
            <span>{t('mine.fin.epoch')} <b>{fin.epoch}</b></span>
            <span class="dim">{fin.blocks_into_epoch}/{fin.epoch_length}</span>
          </div>
          <div class="fin-bar"><div class="fin-fill" style="width:{epochPct}%"></div></div>
          <div class="fin-floor">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="3" y="7" width="10" height="7" rx="1.5"/><path d="M5 7V5a3 3 0 016 0v2"/></svg>
            <span>{t('mine.fin.floor')} <b>{fin.finalized_floor.toLocaleString('fr-FR')}</b></span>
          </div>
          <p class="mine-p">{t('mine.fin.explain')}</p>
        </div>
        <div class="fin-right">
          <div class="fin-stat">
            <div class="stat-label">{t('mine.fin.validators')}</div>
            <div class="fin-stat-v">{fin.validators}</div>
          </div>
          <div class="fin-stat">
            <div class="stat-label">{t('mine.fin.staked')}</div>
            <div class="fin-stat-v">{fmtQ(fin.total_staked)} <span class="fin-stat-u">{TICKER}</span></div>
          </div>
          {#if fin.i_am_validator}
            <div class="fin-you ok">{t('mine.fin.youAre')}</div>
          {:else}
            <div class="fin-you">{t('mine.fin.become')}</div>
          {/if}
        </div>
      </div>
    {:else}
      <div class="mine-p dim">{t('loading')}</div>
    {/if}
  </div>

  <div class="grid-2 dual-row">
    <!-- ── Émission réelle, décroissante vers le plafond ── -->
    <div class="card">
      <div class="card-title">{t('mine.emission.title')}</div>
      <div class="em-now">
        <span class="em-val">{emissionPerHour.toFixed(2)}</span>
        <span class="em-unit">QUANTA/h · {t('mine.emission.network')}</span>
      </div>
      <canvas bind:this={curveCanvas} class="em-curve" aria-label={t('mine.emission.curveAria')}></canvas>
      <p class="mine-p em-explain">{t('mine.emission.explain')}</p>
    </div>

    <!-- ── Monnaie QUANTA — offre prouvable (confiance) ── -->
    <div class="card">
      <div class="card-title">{t('db.currency_title')}</div>
      <div class="supply-grid">
        <div><div class="sup-k">{t('db.hard_cap')}</div><div class="sup-v">{loaded ? fmtQ(maxSupply) : '—'}</div></div>
        <div><div class="sup-k">{t('db.issued')}</div><div class="sup-v">{loaded ? fmtQ(minedQta) : '—'}</div></div>
        <div><div class="sup-k">{t('db.burned')}</div><div class="sup-v">{loaded ? fmtQ(burnedQta) : '—'}</div></div>
        <div><div class="sup-k">{t('db.circulating')}</div><div class="sup-v">{loaded ? fmtQ(circulatingQta) : '—'}</div></div>
      </div>
      <div class="sup-bar"><div class="sup-fill" style="width:{Math.min(100, Math.max(pctToCap, pctToCap > 0 ? 0.5 : 0))}%;"></div></div>
      <div class="sup-cap-line">{pctToCap < 0.01 && pctToCap > 0 ? '<0,01' : pctToCap.toFixed(2)}{t('db.cap_issued')} · {t('db.deflationary')}</div>
      <div class="sup-trust">
        <span>{t('db.no_authority')}</span>
        <span>{t('db.no_premine')}</span>
        <span>{t('db.policy_in_code')}</span>
      </div>
    </div>
  </div>
</div>

<style>
  .mine-p { font-size: var(--text-sm); color: var(--color-text-2); line-height: 1.55; }

  .load-err { color: var(--color-text-2); font-size: var(--text-sm); margin-top: 4px; }

  /* ── Comprendre ── */
  .understand { margin-bottom: 12px; }
  .ex-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 22px; margin-top: 16px; }
  @media (max-width: 820px) { .ex-grid { grid-template-columns: 1fr; gap: 18px; } }
  .ex-glyph {
    width: 36px; height: 36px; border-radius: 10px;
    display: flex; align-items: center; justify-content: center;
    background: var(--cyan-dim); color: var(--color-accent);
    margin-bottom: 12px;
  }
  .ex-k { font-size: var(--text-base); font-weight: 700; color: var(--color-text-0); margin-bottom: 6px; letter-spacing: -0.01em; }
  .ex-v { font-size: var(--text-sm); color: var(--color-text-2); line-height: 1.55; }

  /* ── Forge (terminal) ── */
  .forge { margin-bottom: 12px; }

  /* ── Stats row (zéro énergie) ── */
  .stats-row { margin-bottom: 12px; }
  .stat-card { padding: 18px 20px; }

  .dual-row { margin-bottom: 12px; }

  /* ── Émission ── */
  .em-now { display: flex; align-items: baseline; gap: 8px; margin-bottom: 14px; }
  .em-val { font-size: 32px; font-weight: 700; color: var(--color-text-0); letter-spacing: -0.02em; font-variant-numeric: tabular-nums lining-nums; }
  .em-unit { font-size: var(--text-sm); color: var(--color-text-2); }
  .em-curve { width: 100%; height: 110px; display: block; }
  .em-explain { margin-top: 12px; }

  /* ── Finalité ── */
  .fin-card { margin-bottom: 12px; }
  .fin-grid { display: grid; grid-template-columns: 1.6fr 1fr; gap: 28px; }
  @media (max-width: 720px) { .fin-grid { grid-template-columns: 1fr; } }
  .fin-epoch-head {
    display: flex; justify-content: space-between; align-items: baseline;
    font-size: var(--text-base); font-weight: 600; color: var(--color-text-0); margin-bottom: 8px;
  }
  .fin-bar { height: 8px; background: var(--color-bg-3); border-radius: 4px; overflow: hidden; }
  .fin-fill { height: 100%; background: var(--color-accent); border-radius: 4px; transition: width 0.8s var(--ease-out); }
  .fin-floor {
    display: flex; align-items: center; gap: 8px;
    margin: 16px 0 10px; font-size: var(--text-base); color: var(--color-text-1);
  }
  .fin-floor svg { color: var(--color-accent); flex-shrink: 0; }
  .fin-right { display: flex; flex-direction: column; gap: 16px; }
  .fin-stat-v { font-size: 22px; font-weight: 700; color: var(--color-text-0); margin-top: 4px; font-variant-numeric: tabular-nums lining-nums; }
  .fin-stat-u { font-size: var(--text-sm); color: var(--color-text-2); font-weight: 400; }
  .fin-you {
    font-size: var(--text-sm); color: var(--color-text-2);
    padding: 10px 12px; background: var(--color-bg-2);
    border-radius: var(--radius-sm); line-height: 1.5;
  }
  .fin-you.ok {
    color: var(--color-accent-hover); font-weight: 600;
    background: var(--cyan-dim); border: 1px solid var(--cyan-mid);
  }

  /* ── Offre prouvable ── */
  .supply-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 16px; margin-bottom: 16px; }
  .sup-k { font-size: var(--text-xs); color: var(--color-text-3); text-transform: uppercase; letter-spacing: .05em; margin-bottom: 5px; }
  .sup-v { font-size: 19px; font-weight: 700; color: var(--color-text-0); font-variant-numeric: tabular-nums lining-nums; }
  .sup-bar { height: 8px; background: var(--color-bg-3); border-radius: 4px; overflow: hidden; }
  .sup-fill { height: 100%; background: var(--color-accent); border-radius: 4px; transition: width 1.2s var(--ease-out); }
  .sup-cap-line { font-size: var(--text-sm); color: var(--color-text-2); margin-top: 8px; }
  .sup-trust {
    display: flex; flex-direction: column; gap: 8px; margin-top: 16px;
    font-size: var(--text-sm); color: var(--color-text-1); font-weight: 500;
  }
  .sup-trust span { display: inline-flex; align-items: center; gap: 7px; }
  .sup-trust span::before {
    content: ""; width: 14px; height: 14px; flex-shrink: 0;
    border-radius: 50%;
    background:
      url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='10' viewBox='0 0 12 12'%3E%3Cpath d='M2.5 6.2 4.8 8.5 9.5 3.5' fill='none' stroke='%230BA5A0' stroke-width='1.8' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E") center / 10px no-repeat,
      var(--cyan-dim);
  }
</style>
