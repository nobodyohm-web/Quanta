<script lang="ts">
  // ── Mode pro : le terminal de la forge ──────────────────────────
  // La SEULE surface sombre de l'app (un terminal EST sombre). Il écoute
  // les VRAIS évènements Tauri du nœud (quanta://mined / block-sealed /
  // tx-applied) et sonde get_node_status / get_finality_status pour des
  // compteurs vivants. HONNÊTE : Quanta est en Proof-of-Stake — aucun hash
  // miné, aucune course. On montre le vrai travail : vérification des
  // signatures ML-DSA, validation, relais, scellement, vote de finalité.
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { t, locale } from "./i18n.svelte";

  const reduce =
    typeof window !== "undefined" &&
    !!window.matchMedia &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ── i18n local (6 langues, réactif via locale()) — le terminal est un
  //    module « mode pro » autonome ; ses clés vivent ici, pas dans le
  //    dictionnaire partagé. Repli sur l'anglais si une clé manque. ──
  const L: Record<string, Record<string, string>> = {
    en: {
      "live": "live",
      "title": "The forge, live",
      "sub": "Your node's real work, in real time — no mined hashes, nothing simulated.",
      "c.sig": "ML-DSA signatures verified",
      "c.blocks": "Blocks validated",
      "c.tx": "Transactions applied",
      "s.peers": "Peers",
      "s.height": "Height",
      "s.epoch": "Epoch",
      "s.floor": "Finalized floor",
      "e.boot": "listening to the node — real events only",
      "e.reward": "reward mined  +{a} QUANTA",
      "e.sealedMine": "BLOCK SEALED #{n} · {t} tx — you were the leader",
      "e.block": "block #{n} validated · {t} tx",
      "e.tx": "transaction applied · {k}",
      "e.vote": "finality vote — epoch {n}",
      "e.final": "block #{n} finalized · irreversible",
      "e.peer": "peer connected · {n} peers",
      "note": "Quanta runs on Proof-of-Stake: no proof-of-work, no hash race. The real work is verifying signatures, validating, relaying, sealing and voting finality.",
    },
    fr: {
      "live": "en direct",
      "title": "La forge, en direct",
      "sub": "Le vrai travail de ton nœud, en temps réel — aucun hash miné, rien de simulé.",
      "c.sig": "Signatures ML-DSA vérifiées",
      "c.blocks": "Blocs validés",
      "c.tx": "Transactions appliquées",
      "s.peers": "Pairs",
      "s.height": "Hauteur",
      "s.epoch": "Époque",
      "s.floor": "Plancher finalisé",
      "e.boot": "écoute du nœud — évènements réels uniquement",
      "e.reward": "récompense minée  +{a} QUANTA",
      "e.sealedMine": "BLOC SCELLÉ #{n} · {t} tx — tu étais le leader",
      "e.block": "bloc #{n} validé · {t} tx",
      "e.tx": "transaction appliquée · {k}",
      "e.vote": "vote de finalité — époque {n}",
      "e.final": "bloc #{n} finalisé · irréversible",
      "e.peer": "pair connecté · {n} pairs",
      "note": "Quanta fonctionne en Proof-of-Stake : pas de proof-of-work, pas de course au hash. Le vrai travail, c'est vérifier les signatures, valider, relayer, sceller et voter la finalité.",
    },
    es: {
      "live": "en vivo",
      "title": "La forja, en vivo",
      "sub": "El trabajo real de tu nodo, en tiempo real — sin hashes minados, nada simulado.",
      "c.sig": "Firmas ML-DSA verificadas",
      "c.blocks": "Bloques validados",
      "c.tx": "Transacciones aplicadas",
      "s.peers": "Pares",
      "s.height": "Altura",
      "s.epoch": "Época",
      "s.floor": "Suelo finalizado",
      "e.boot": "escuchando al nodo — solo eventos reales",
      "e.reward": "recompensa minada  +{a} QUANTA",
      "e.sealedMine": "BLOQUE SELLADO #{n} · {t} tx — fuiste el líder",
      "e.block": "bloque #{n} validado · {t} tx",
      "e.tx": "transacción aplicada · {k}",
      "e.vote": "voto de finalidad — época {n}",
      "e.final": "bloque #{n} finalizado · irreversible",
      "e.peer": "par conectado · {n} pares",
      "note": "Quanta funciona con Proof-of-Stake: sin proof-of-work, sin carrera de hashes. El trabajo real es verificar firmas, validar, retransmitir, sellar y votar la finalidad.",
    },
    ru: {
      "live": "в эфире",
      "title": "Кузница, в прямом эфире",
      "sub": "Реальная работа узла в реальном времени — никаких намайненных хэшей, ничего симулированного.",
      "c.sig": "Подписи ML-DSA проверены",
      "c.blocks": "Блоков проверено",
      "c.tx": "Транзакций применено",
      "s.peers": "Пиры",
      "s.height": "Высота",
      "s.epoch": "Эпоха",
      "s.floor": "Финализированный пол",
      "e.boot": "слушаем узел — только реальные события",
      "e.reward": "награда добыта  +{a} QUANTA",
      "e.sealedMine": "БЛОК ЗАПЕЧАТАН #{n} · {t} tx — вы были лидером",
      "e.block": "блок #{n} проверен · {t} tx",
      "e.tx": "транзакция применена · {k}",
      "e.vote": "голос финальности — эпоха {n}",
      "e.final": "блок #{n} финализирован · необратимо",
      "e.peer": "пир подключён · {n} пиров",
      "note": "Quanta работает на Proof-of-Stake: без proof-of-work и гонки хэшей. Реальная работа — проверять подписи, валидировать, ретранслировать, запечатывать и голосовать за финальность.",
    },
    zh: {
      "live": "实时",
      "title": "锻造炉·实时",
      "sub": "你的节点的真实工作，实时呈现——不挖哈希，绝无模拟。",
      "c.sig": "已验证 ML-DSA 签名",
      "c.blocks": "已验证区块",
      "c.tx": "已应用交易",
      "s.peers": "节点",
      "s.height": "高度",
      "s.epoch": "纪元",
      "s.floor": "最终性地板",
      "e.boot": "正在监听节点——仅真实事件",
      "e.reward": "已获挖矿奖励  +{a} QUANTA",
      "e.sealedMine": "区块已封存 #{n} · {t} 笔 — 你是出块者",
      "e.block": "区块 #{n} 已验证 · {t} 笔",
      "e.tx": "交易已应用 · {k}",
      "e.vote": "最终性投票——纪元 {n}",
      "e.final": "区块 #{n} 已最终确定 · 不可逆",
      "e.peer": "节点已连接 · {n} 个节点",
      "note": "Quanta 采用权益证明（PoS）：没有工作量证明，没有哈希竞赛。真正的工作是验证签名、校验、转发、封存并对最终性投票。",
    },
    ja: {
      "live": "ライブ",
      "title": "鍛冶場・ライブ",
      "sub": "ノードの本当の仕事をリアルタイムで——ハッシュ採掘なし、シミュレーションなし。",
      "c.sig": "検証済み ML-DSA 署名",
      "c.blocks": "検証済みブロック",
      "c.tx": "適用済み取引",
      "s.peers": "ピア",
      "s.height": "高さ",
      "s.epoch": "エポック",
      "s.floor": "確定フロア",
      "e.boot": "ノードを監視中——実イベントのみ",
      "e.reward": "報酬を採掘  +{a} QUANTA",
      "e.sealedMine": "ブロック封印 #{n} · {t} tx — あなたがリーダー",
      "e.block": "ブロック #{n} 検証 · {t} tx",
      "e.tx": "取引を適用 · {k}",
      "e.vote": "ファイナリティ投票——エポック {n}",
      "e.final": "ブロック #{n} 確定 · 不可逆",
      "e.peer": "ピア接続 · {n} ピア",
      "note": "Quanta はプルーフ・オブ・ステークで動作：PoW もハッシュ競争もありません。本当の仕事は署名検証・検証・中継・封印・ファイナリティ投票です。",
    },
  };
  function tl(key: string): string {
    const loc = locale();
    return L[loc]?.[key] ?? L.en[key] ?? key;
  }

  // ── Flux de lignes ──
  interface Line { id: number; kind: string; time: string; text: string; }
  let lines = $state<Line[]>([]);
  let seq = 0;

  // ── Compteurs vivants (dérivés d'évènements RÉELS) ──
  let cSig = $state(0);     // signatures ML-DSA vérifiées (une par tx d'un bloc scellé + proposeur)
  let cBlocks = $state(0);  // blocs validés (un par block-sealed)
  let cTx = $state(0);      // transactions appliquées (un par tx-applied)

  // ── Stats vivantes (sondage) ──
  let peers = $state(0);
  let height = $state(0);
  let epoch = $state(0);
  let floor = $state(0);

  // suivis pour ne journaliser que les VRAIES transitions
  let seenStats = false;
  let rootEl = $state<HTMLElement | undefined>();
  let visible = $state(true);

  function stamp(): string {
    const d = new Date();
    const p = (n: number) => n.toString().padStart(2, "0");
    return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
  }

  function push(kind: string, text: string) {
    lines = [{ id: ++seq, kind, time: stamp(), text }, ...lines].slice(0, 40);
  }

  function fill(tpl: string, vars: Record<string, string | number>): string {
    let out = tpl;
    for (const [k, v] of Object.entries(vars)) out = out.replace(`{${k}}`, String(v));
    return out;
  }

  async function poll() {
    try {
      const s = await invoke<any>("get_node_status");
      const p = s?.peer_count ?? 0;
      if (seenStats && p !== peers && p > 0) push("peer", fill(tl("e.peer"), { n: p }));
      peers = p;
    } catch {}
    try {
      const f = await invoke<any>("get_finality_status");
      if (f) {
        if (seenStats && f.epoch > epoch) push("vote", fill(tl("e.vote"), { n: f.epoch }));
        if (seenStats && f.finalized_floor > floor && f.finalized_floor > 0)
          push("seal", fill(tl("e.final"), { n: f.finalized_floor.toLocaleString("fr-FR") }));
        height = f.height ?? height;
        epoch = f.epoch ?? epoch;
        floor = f.finalized_floor ?? floor;
        seenStats = true;
      }
    } catch {}
  }

  // ── Écoute des VRAIS évènements du nœud ──
  $effect(() => {
    const unsubs: UnlistenFn[] = [];
    let alive = true;
    (async () => {
      push("boot", tl("e.boot"));
      const u1 = await listen<{ amount: number; kwh: number }>("quanta://mined", (e) => {
        const a = e.payload?.amount ?? 0;
        if (a <= 0) return;
        push("reward", fill(tl("e.reward"), { a: a.toFixed(4) }));
      });
      const u2 = await listen<{ index: number; txs: number; mine: boolean }>(
        "quanta://block-sealed",
        (e) => {
          const p = e.payload;
          if (!p) return;
          cBlocks += 1;
          cSig += Math.max(0, p.txs ?? 0) + 1; // txs vérifiées + signature du proposeur
          if (p.mine) push("seal", fill(tl("e.sealedMine"), { n: p.index, t: p.txs ?? 0 }));
          else push("block", fill(tl("e.block"), { n: p.index, t: p.txs ?? 0 }));
        },
      );
      const u3 = await listen<{ from: string; to: string; amount: number; tx_type: string }>(
        "quanta://tx-applied",
        (e) => {
          const p = e.payload;
          if (!p) return;
          cTx += 1;
          const kind = p.tx_type ? t(("tx." + p.tx_type) as any) : "";
          push("tx", fill(tl("e.tx"), { k: kind }));
        },
      );
      if (!alive) { u1(); u2(); u3(); return; }
      unsubs.push(u1, u2, u3);
    })();
    return () => {
      alive = false;
      unsubs.forEach((u) => u());
    };
  });

  // ── Sondage des compteurs + pause hors-écran ──
  $effect(() => {
    let iv: ReturnType<typeof setInterval> | null = null;
    const start = () => { if (!iv) iv = setInterval(poll, 4000); };
    const stop = () => { if (iv) { clearInterval(iv); iv = null; } };

    poll();
    start();

    let io: IntersectionObserver | undefined;
    const el = rootEl;
    if (el && typeof IntersectionObserver !== "undefined") {
      io = new IntersectionObserver(
        (entries) => {
          const vis = entries[0]?.isIntersecting ?? true;
          visible = vis;
          if (vis) { poll(); start(); } else { stop(); }
        },
        { threshold: 0.01 },
      );
      io.observe(el);
    }
    return () => { stop(); io?.disconnect(); };
  });
</script>

<div class="term" class:reduce bind:this={rootEl} role="group" aria-label={tl("title")}>
  <div class="term-bar">
    <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
    <span class="term-title mono">quanta · node</span>
    <span class="term-live" class:paused={!visible}>
      <span class="live-dot"></span>{tl("live")}
    </span>
  </div>

  <div class="term-head">
    <div class="th-title">{tl("title")}</div>
    <div class="th-sub">{tl("sub")}</div>
  </div>

  <div class="counters">
    <div class="ct">
      <div class="ct-v mono">{cSig.toLocaleString("fr-FR")}</div>
      <div class="ct-k">{tl("c.sig")}</div>
    </div>
    <div class="ct">
      <div class="ct-v mono">{cBlocks.toLocaleString("fr-FR")}</div>
      <div class="ct-k">{tl("c.blocks")}</div>
    </div>
    <div class="ct">
      <div class="ct-v mono">{cTx.toLocaleString("fr-FR")}</div>
      <div class="ct-k">{tl("c.tx")}</div>
    </div>
  </div>

  <div class="stats-row">
    <div class="sr"><span class="sr-k">{tl("s.peers")}</span><span class="sr-v mono">{peers}</span></div>
    <div class="sr"><span class="sr-k">{tl("s.height")}</span><span class="sr-v mono">{height.toLocaleString("fr-FR")}</span></div>
    <div class="sr"><span class="sr-k">{tl("s.epoch")}</span><span class="sr-v mono">{epoch}</span></div>
    <div class="sr"><span class="sr-k">{tl("s.floor")}</span><span class="sr-v mono">{floor.toLocaleString("fr-FR")}</span></div>
  </div>

  <div class="console" role="log" aria-label={tl("title")}>
    <div class="log">
      {#each lines as line (line.id)}
        <div class="line ln-{line.kind}">
          <span class="ln-time mono">{line.time}</span>
          <span class="ln-glyph" aria-hidden="true">
            {#if line.kind === "seal"}◆{:else if line.kind === "reward"}✦{:else if line.kind === "vote"}◇{:else if line.kind === "block"}▢{:else if line.kind === "tx"}→{:else if line.kind === "peer"}↺{:else}›{/if}
          </span>
          <span class="ln-text mono">{line.text}</span>
        </div>
      {/each}
    </div>
  </div>

  <p class="term-note mono">{tl("note")}</p>
</div>

<style>
  .term {
    --tbg: #0c0d11;
    --tpanel: #14161c;
    --tline: rgba(255, 255, 255, 0.07);
    --ttext: #c8ccd4;
    --tdim: #6b7180;
    --tteal: #14c8b8;
    background: var(--tbg);
    border: 1px solid #1c1f27;
    border-radius: var(--radius-lg);
    padding: 0;
    overflow: hidden;
    box-shadow: var(--shadow);
    color: var(--ttext);
  }

  /* barre de fenêtre terminal */
  .term-bar {
    display: flex; align-items: center; gap: 10px;
    padding: 11px 16px;
    border-bottom: 1px solid var(--tline);
    background: #0a0b0e;
  }
  .dots { display: inline-flex; gap: 6px; }
  .dots i { width: 9px; height: 9px; border-radius: 50%; background: #2a2e38; display: block; }
  .term-title { font-size: 12px; color: var(--tdim); letter-spacing: 0.02em; }
  .term-live {
    margin-left: auto;
    display: inline-flex; align-items: center; gap: 7px;
    font-size: 11px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase;
    color: var(--tteal);
  }
  .live-dot {
    width: 7px; height: 7px; border-radius: 50%; background: var(--tteal);
    box-shadow: 0 0 0 0 rgba(20, 200, 184, 0.5);
    animation: term-pulse 2s ease infinite;
  }
  .term-live.paused .live-dot { animation: none; opacity: 0.4; }
  @keyframes term-pulse {
    0%, 100% { box-shadow: 0 0 0 0 rgba(20, 200, 184, 0.45); }
    50% { box-shadow: 0 0 0 5px rgba(20, 200, 184, 0); }
  }

  .term-head { padding: 18px 20px 4px; }
  .th-title { font-family: var(--font-display); font-size: 17px; font-weight: 700; color: #eef1f5; letter-spacing: -0.01em; }
  .th-sub { font-size: 12.5px; color: var(--tdim); margin-top: 4px; line-height: 1.5; max-width: 62ch; }

  /* compteurs */
  .counters { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; padding: 16px 20px 4px; }
  @media (max-width: 560px) { .counters { grid-template-columns: 1fr; } }
  .ct { background: var(--tpanel); border: 1px solid var(--tline); border-radius: var(--radius-sm); padding: 13px 15px; }
  .ct-v { font-family: var(--font-display); font-size: 24px; font-weight: 700; color: #f2f4f7; font-variant-numeric: tabular-nums lining-nums; line-height: 1; }
  .ct-k { font-size: 11.5px; color: var(--tdim); margin-top: 6px; }

  /* stats vivantes */
  .stats-row { display: flex; flex-wrap: wrap; gap: 8px 24px; padding: 14px 20px 0; }
  .sr { display: inline-flex; align-items: baseline; gap: 8px; }
  .sr-k { font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--tdim); }
  .sr-v { font-size: 14px; font-weight: 700; color: var(--ttext); font-variant-numeric: tabular-nums lining-nums; }

  /* console */
  .console {
    margin: 14px 16px 0;
    border: 1px solid var(--tline);
    border-radius: var(--radius-sm);
    background: #090a0d;
    padding: 8px;
    height: 260px;
    overflow: hidden;
    position: relative;
    -webkit-mask-image: linear-gradient(180deg, #000 78%, transparent 100%);
    mask-image: linear-gradient(180deg, #000 78%, transparent 100%);
  }
  .log { display: flex; flex-direction: column; gap: 2px; }
  .line {
    display: flex; align-items: baseline; gap: 10px;
    padding: 5px 9px; border-radius: 7px;
    font-size: 12.5px; color: var(--ttext); line-height: 1.4;
    animation: line-in 0.26s var(--ease-out, ease-out);
  }
  .reduce .line { animation: none; }
  @keyframes line-in { from { opacity: 0; transform: translateY(-4px); } to { opacity: 1; transform: none; } }
  .ln-time { color: #4b505c; font-size: 11.5px; flex-shrink: 0; }
  .ln-glyph { color: var(--tdim); width: 12px; flex-shrink: 0; text-align: center; }
  .ln-text { flex: 1; min-width: 0; overflow-wrap: anywhere; }

  .ln-reward .ln-glyph { color: var(--tteal); }
  .ln-reward .ln-text { color: #dff6f3; }
  .ln-vote .ln-glyph { color: var(--tteal); }
  .ln-seal {
    background: rgba(20, 200, 184, 0.08);
    border: 1px solid rgba(20, 200, 184, 0.22);
  }
  .ln-seal .ln-glyph { color: var(--tteal); }
  .ln-seal .ln-text { color: #d6f5f1; font-weight: 600; }
  .ln-boot .ln-text, .ln-boot .ln-glyph { color: var(--tdim); }

  .term-note {
    padding: 14px 20px 18px;
    font-size: 11.5px; color: var(--tdim); line-height: 1.55;
    border-top: 1px solid var(--tline);
    margin-top: 14px;
  }

  :global(.term) ::-webkit-scrollbar-thumb { background: rgba(255, 255, 255, 0.18); }
</style>
