<script lang="ts">
  // ═══════════════════════════════════════════════════════════════════
  //  ForgeEngine — « le moteur de consensus, en direct ».
  //  La SEULE surface sombre de l'app (un moteur EST sombre). HONNÊTE :
  //  Quanta est en Proof-of-Stake — pas de course au hash pour une
  //  difficulté. Mais la cryptographie RÉELLE, elle, tourne : on calcule
  //  ici de VRAIS BLAKE3 (@noble/hashes) chaînés sur le VRAI dernier bloc,
  //  à un rythme mesuré réel, et on diffuse les VRAIS évènements du nœud.
  //  Garde-fous perf stricts : boucle time-boxée 4 ms/frame, pause hors
  //  écran / onglet caché, cleanup au démontage → jamais de gel.
  // ═══════════════════════════════════════════════════════════════════
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { blake3 } from "@noble/hashes/blake3.js";
  import { bytesToHex } from "@noble/hashes/utils.js";
  import { locale } from "./i18n.svelte";

  const reduce =
    typeof window !== "undefined" && !!window.matchMedia &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ── i18n local (6 langues concises) ──────────────────────────────
  const L: Record<string, Record<string, string>> = {
    en: { title: "Consensus engine", live: "live", slot: "SLOT", chain: "BLAKE3 · integrity chaining",
      hps: "hashes/s", computed: "computed live on your device", verified: "chained on block",
      pBeacon: "Beacon", pElect: "Leader election", pSeal: "Block seal", pSig: "ML-DSA signature", pFinal: "Finality",
      sHeight: "Height", sEpoch: "Epoch", sFloor: "Finalized", sVals: "Validators", sStake: "Staked", sPeers: "Peers",
      youVal: "You are a validator — you can be elected to seal blocks.",
      becomeVal: "Stake ≥ 1 QUANTA to become a validator and seal blocks.",
      note: "Proof-of-Stake: no hash race, no mining farm. These are real BLAKE3 hashes — the primitive that hashes and links every block — computed live to prove integrity, not to win a power contest.",
      boot: "engine online — real crypto, real events",
      solo: "Solo node — 0 peers. Only YOUR node's work shows here (mine ~60s → seal ~120s). Amounts look alike because emission declines slowly — but every tx is unique: check its hash. Connect a peer to see signed network traffic.",
      eReward: "reward minted  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "BLOCK SEALED #{n} · {t} tx · {h} ← {p}",
      eSeal: "block #{n} sealed · {t} tx · {h} ← {p}", eVerify: "block #{n} verified — PoS proposer ✓ · coverage ✓ · Merkle ✓", eState: "chain #{n} · epoch {e} · {v} validators", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "finality vote · epoch {n}", eEnv: "{m} from {s} · nonce {n} — ML-DSA ✓", eVoteCast: "OUR finality vote signed — epoch {e} → #{h}",
      eFinal: "block #{n} finalized · irreversible", ePeer: "peer connected · {n} peers" },
    fr: { title: "Moteur de consensus", live: "en direct", slot: "SLOT", chain: "BLAKE3 · chaînage d'intégrité",
      hps: "hashs/s", computed: "calculés en direct sur ton appareil", verified: "chaînés sur le bloc",
      pBeacon: "Beacon", pElect: "Élection du leader", pSeal: "Scellement", pSig: "Signature ML-DSA", pFinal: "Finalité",
      sHeight: "Hauteur", sEpoch: "Époque", sFloor: "Finalisé", sVals: "Validateurs", sStake: "Enjeu", sPeers: "Pairs",
      youVal: "Tu es validateur — tu peux être élu pour sceller des blocs.",
      becomeVal: "Stake ≥ 1 QUANTA pour devenir validateur et sceller des blocs.",
      note: "Proof-of-Stake : pas de course au hash, pas de ferme de minage. Ce sont de vrais BLAKE3 — la primitive qui hache et lie chaque bloc — calculés en direct pour prouver l'intégrité, pas pour gagner une course à la puissance.",
      boot: "moteur en ligne — crypto réelle, évènements réels",
      solo: "Nœud solo — 0 pair. Tu ne vois ici que le travail de TON nœud (mine ~60 s → scelle ~120 s). Les montants se ressemblent car l'émission décroît lentement — mais chaque tx est unique : regarde son hash. Connecte un pair pour voir le trafic réseau signé.",
      eReward: "récompense minée  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "BLOC SCELLÉ #{n} · {t} tx · {h} ← {p}",
      eSeal: "bloc #{n} scellé · {t} tx · {h} ← {p}", eVerify: "bloc #{n} vérifié — proposeur PoS ✓ · couverture ✓ · Merkle ✓", eState: "chaîne #{n} · époque {e} · {v} validateurs", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "vote de finalité · époque {n}", eEnv: "{m} de {s} · nonce {n} — ML-DSA ✓", eVoteCast: "NOTRE vote de finalité signé — époque {e} → #{h}",
      eFinal: "bloc #{n} finalisé · irréversible", ePeer: "pair connecté · {n} pairs" },
    es: { title: "Motor de consenso", live: "en vivo", slot: "SLOT", chain: "BLAKE3 · encadenado de integridad",
      hps: "hashes/s", computed: "calculados en vivo en tu dispositivo", verified: "encadenados en el bloque",
      pBeacon: "Beacon", pElect: "Elección de líder", pSeal: "Sellado", pSig: "Firma ML-DSA", pFinal: "Finalidad",
      sHeight: "Altura", sEpoch: "Época", sFloor: "Finalizado", sVals: "Validadores", sStake: "Stake", sPeers: "Pares",
      youVal: "Eres validador — puedes ser elegido para sellar bloques.",
      becomeVal: "Haz stake ≥ 1 QUANTA para ser validador y sellar bloques.",
      note: "Proof-of-Stake: sin carrera de hashes, sin granja. Son BLAKE3 reales — la primitiva que hashea y enlaza cada bloque — calculados en vivo para probar integridad, no para ganar una carrera de potencia.",
      boot: "motor en línea — cripto real, eventos reales",
      solo: "Nodo solo — 0 pares. Aquí solo ves el trabajo de TU nodo (mina ~60 s → sella ~120 s). Los montos se parecen porque la emisión decrece despacio — pero cada tx es única: mira su hash. Conecta un par para ver el tráfico de red firmado.",
      eReward: "recompensa minada  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "BLOQUE SELLADO #{n} · {t} tx · {h} ← {p}",
      eSeal: "bloque #{n} sellado · {t} tx · {h} ← {p}", eVerify: "bloque #{n} verificado — proponente PoS ✓ · cobertura ✓ · Merkle ✓", eState: "cadena #{n} · época {e} · {v} validadores", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "voto de finalidad · época {n}", eEnv: "{m} de {s} · nonce {n} — ML-DSA ✓", eVoteCast: "NUESTRO voto de finalidad firmado — época {e} → #{h}",
      eFinal: "bloque #{n} finalizado · irreversible", ePeer: "par conectado · {n} pares" },
    ru: { title: "Движок консенсуса", live: "в эфире", slot: "СЛОТ", chain: "BLAKE3 · сцепление целостности",
      hps: "хэшей/с", computed: "вычислено вживую на вашем устройстве", verified: "сцеплены с блоком",
      pBeacon: "Маяк", pElect: "Выбор лидера", pSeal: "Запечатывание", pSig: "Подпись ML-DSA", pFinal: "Финальность",
      sHeight: "Высота", sEpoch: "Эпоха", sFloor: "Финализ.", sVals: "Валидаторы", sStake: "Стейк", sPeers: "Пиры",
      youVal: "Вы валидатор — вас могут выбрать запечатывать блоки.",
      becomeVal: "Застейкайте ≥ 1 QUANTA, чтобы стать валидатором.",
      note: "Proof-of-Stake: без гонки хэшей и ферм. Это настоящие BLAKE3 — примитив, что хэширует и связывает каждый блок — вычисляемые вживую для доказательства целостности, а не ради гонки мощности.",
      boot: "движок в сети — реальная крипта, реальные события",
      solo: "Одиночный узел — 0 пиров. Здесь видна только работа ВАШЕГО узла (майнинг ~60 с → запечатывание ~120 с). Суммы похожи, потому что эмиссия убывает медленно — но каждая tx уникальна: смотрите её хэш. Подключите пира, чтобы увидеть подписанный сетевой трафик.",
      eReward: "награда добыта  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "БЛОК ЗАПЕЧАТАН #{n} · {t} tx · {h} ← {p}",
      eSeal: "блок #{n} запечатан · {t} tx · {h} ← {p}", eVerify: "блок #{n} проверен — PoS-предлагатель ✓ · покрытие ✓ · Merkle ✓", eState: "цепь #{n} · эпоха {e} · {v} валидаторов", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "голос финальности · эпоха {n}", eEnv: "{m} от {s} · nonce {n} — ML-DSA ✓", eVoteCast: "НАШ голос финальности подписан — эпоха {e} → #{h}",
      eFinal: "блок #{n} финализирован · необратимо", ePeer: "пир подключён · {n} пиров" },
    zh: { title: "共识引擎", live: "实时", slot: "时隙", chain: "BLAKE3 · 完整性链接",
      hps: "哈希/秒", computed: "在你的设备上实时计算", verified: "链接于区块",
      pBeacon: "信标", pElect: "出块者选举", pSeal: "封存", pSig: "ML-DSA 签名", pFinal: "最终性",
      sHeight: "高度", sEpoch: "纪元", sFloor: "已最终确定", sVals: "验证者", sStake: "质押", sPeers: "节点",
      youVal: "你是验证者——可被选为出块者封存区块。",
      becomeVal: "质押 ≥ 1 QUANTA 即可成为验证者并封存区块。",
      note: "权益证明：没有哈希竞赛，没有矿场。这些是真实的 BLAKE3——哈希并链接每个区块的原语——实时计算以证明完整性，而非争夺算力。",
      boot: "引擎在线——真实密码学，真实事件",
      solo: "单节点 — 0 个对等节点。这里只显示你自己节点的工作（挖矿 ~60 秒 → 封存 ~120 秒）。金额相近是因为发行量缓慢递减——但每笔交易都是唯一的：看它的哈希。连接一个节点即可看到签名的网络流量。",
      eReward: "已获挖矿奖励  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "区块已封存 #{n} · {t} 笔 · {h} ← {p}",
      eSeal: "区块 #{n} 已封存 · {t} 笔 · {h} ← {p}", eVerify: "区块 #{n} 已验证 — PoS 出块者 ✓ · 覆盖 ✓ · Merkle ✓", eState: "链 #{n} · 纪元 {e} · {v} 个验证者", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "最终性投票 · 纪元 {n}", eEnv: "{m} 来自 {s} · nonce {n} — ML-DSA ✓", eVoteCast: "我们的最终性投票已签名 — 纪元 {e} → #{h}",
      eFinal: "区块 #{n} 已最终确定 · 不可逆", ePeer: "节点已连接 · {n} 个节点" },
    ja: { title: "コンセンサスエンジン", live: "ライブ", slot: "スロット", chain: "BLAKE3 · 整合性チェーン",
      hps: "ハッシュ/秒", computed: "あなたの端末でライブ計算", verified: "ブロックに連鎖",
      pBeacon: "ビーコン", pElect: "リーダー選出", pSeal: "封印", pSig: "ML-DSA 署名", pFinal: "ファイナリティ",
      sHeight: "高さ", sEpoch: "エポック", sFloor: "確定", sVals: "検証者", sStake: "ステーク", sPeers: "ピア",
      youVal: "あなたは検証者です — 選ばれてブロックを封印できます。",
      becomeVal: "1 QUANTA 以上ステークすると検証者になれます。",
      note: "プルーフ・オブ・ステーク：ハッシュ競争もマイニングファームもありません。これは本物の BLAKE3 — 各ブロックをハッシュし連結する原語 — を整合性証明のためにライブ計算しています。力の競争のためではありません。",
      boot: "エンジン起動 — 本物の暗号、本物のイベント",
      solo: "ソロノード — ピア 0。ここにはあなたのノードの仕事だけが表示されます（採掘 ~60 秒 → 封印 ~120 秒）。発行量はゆっくり減るため金額は似ていますが、各 tx は一意です：ハッシュをご覧ください。ピアに接続すると署名済みネットワークトラフィックが見られます。",
      eReward: "報酬を採掘  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "ブロック封印 #{n} · {t} tx · {h} ← {p}",
      eSeal: "ブロック #{n} 封印 · {t} tx · {h} ← {p}", eVerify: "ブロック #{n} 検証済 — PoS 提案者 ✓ · カバレッジ ✓ · Merkle ✓", eState: "チェーン #{n} · エポック {e} · {v} 検証者", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "ファイナリティ投票 · エポック {n}", eEnv: "{m} ({s}) · nonce {n} — ML-DSA ✓", eVoteCast: "私たちのファイナリティ投票に署名 — エポック {e} → #{h}",
      eFinal: "ブロック #{n} 確定 · 不可逆", ePeer: "ピア接続 · {n} ピア" },
  };
  function tl(k: string): string { const l = locale(); return L[l]?.[k] ?? L.en[k] ?? k; }
  function fill(tpl: string, v: Record<string, string | number>): string {
    let o = tpl; for (const [k, val] of Object.entries(v)) o = o.replace(`{${k}}`, String(val)); return o;
  }
  // Préfixe court d'un hash hex (matière unique de chaque ligne).
  const hshort = (x?: string) => (x ? x.replace(/^0x/, "").slice(0, 12) + "…" : "—");

  // ── État consensus réel (sondé) ──────────────────────────────────
  let height = $state(0);
  let epoch = $state(0);
  let floor = $state(0);
  let validators = $state(0);
  let totalStaked = $state(0);
  let peers = $state(0);
  let iAmValidator = $state(false);
  let lastBlockHash = $state("");

  // ── Cœur de hachage BLAKE3 (réel, mesuré) ────────────────────────
  let coreHash = $state("");        // dernier digest hex (vrai BLAKE3)
  let hashTrail = $state<string[]>([]); // cascade : les digests précédents
  let hps = $state(0);              // hashs/seconde mesurés
  let totalHashes = $state(0);      // total calculés depuis l'ouverture
  let appVersion = $state("");      // version affichée — fin du doute sur le build
  const input = new Uint8Array(48); // 32 o (dernier hash bloc) + 8 o nonce + 8 o slot
  // Départ de nonce aléatoire (CSPRNG) : deux lancements ne produisent jamais
  // la même cascade — l'unicité du flux se vérifie d'un regard.
  if (typeof crypto !== "undefined") crypto.getRandomValues(input.subarray(32, 40));
  // ++ nonce big-endian, retenue correcte. Piège JS : sur un Uint8Array,
  // `++a[k]` retourne la valeur CALCULÉE (256) et non la valeur STOCKÉE (0),
  // donc `if (++a[k] !== 0) break` ne propage jamais la retenue → le nonce
  // bouclait sur 256 valeurs et l'affichage alternait entre 2 digests
  // (le bug « les mêmes hashs en boucle »). Relire input[k] après l'écriture
  // donne la valeur wrappée réelle.
  function bumpNonce() {
    for (let k = 39; k >= 32; k--) { input[k]++; if (input[k] !== 0) break; }
  }

  function seedInput() {
    // Chaîne le cœur sur le VRAI dernier hash de bloc : chaque hash calculé
    // dépend de l'état réel de ta chaîne (pas d'un nombre inventé).
    const h = lastBlockHash.replace(/^0x/, "");
    for (let i = 0; i < 32; i++) {
      const byte = h.length >= (i + 1) * 2 ? parseInt(h.slice(i * 2, i * 2 + 2), 16) : (i * 37 + 11) & 0xff;
      input[i] = Number.isNaN(byte) ? 0 : byte;
    }
    // slot dans les 8 derniers octets
    let s = height + 1;
    for (let i = 47; i >= 40; i--) { input[i] = s & 0xff; s = Math.floor(s / 256); }
  }

  // ── Flux d'évènements réels ──────────────────────────────────────
  interface Line { id: number; kind: string; time: string; text: string; }
  let lines = $state<Line[]>([]);
  let seq = 0;
  function stamp(): string {
    const d = new Date(); const p = (n: number) => n.toString().padStart(2, "0");
    return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
  }
  function push(kind: string, text: string) {
    // untrack : appelé depuis des $effect — sans lui, le spread `...lines`
    // serait une LECTURE trackée de l'état qu'on écrit → boucle réactive
    // infinie (effect_update_depth_exceeded ; le bug historique des gels).
    const prev = untrack(() => lines);
    lines = [{ id: ++seq, kind, time: stamp(), text }, ...prev].slice(0, 30);
  }

  let seenStats = $state(false);
  let statePushed = false;
  async function poll() {
    try {
      const s = await invoke<any>("get_node_status");
      const p = s?.peer_count ?? 0;
      if (seenStats && p !== peers && p > 0) push("peer", fill(tl("ePeer"), { n: p }));
      peers = p;
    } catch {}
    try {
      const f = await invoke<any>("get_finality_status");
      if (f) {
        if (seenStats && f.epoch > epoch) push("vote", fill(tl("eVote"), { n: f.epoch }));
        if (seenStats && f.finalized_floor > floor && f.finalized_floor > 0)
          push("final", fill(tl("eFinal"), { n: f.finalized_floor.toLocaleString("fr-FR") }));
        height = f.height ?? height; epoch = f.epoch ?? epoch; floor = f.finalized_floor ?? floor;
        validators = f.validators ?? validators; totalStaked = f.total_staked ?? totalStaked;
        iAmValidator = !!f.i_am_validator; seenStats = true;
        // Première sonde réussie → une ligne d'état immédiate (le journal ne
        // reste jamais vide en attendant le prochain tick de minage à 60 s).
        if (!statePushed) {
          statePushed = true;
          push("boot", fill(tl("eState"), { n: (f.height ?? 0).toLocaleString("fr-FR"), e: f.epoch ?? 0, v: f.validators ?? 0 }));
        }
      }
    } catch {}
    try {
      const c = await invoke<any>("get_chain_history");
      const rec = c?.recent;
      if (Array.isArray(rec) && rec.length) {
        const top = rec[rec.length - 1];
        if (top?.hash) { lastBlockHash = top.hash; seedInput(); }
      }
    } catch {}
  }

  // ── Cœur : petits lots de BLAKE3 réels sur un timer léger. PAS de boucle
  //    rAF 60 fps (qui saturerait le thread par la pression GC → gel). On
  //    calcule un lot borné ~11×/s : vivant à l'œil, invisible pour le CPU. ──
  let rootEl = $state<HTMLElement | undefined>();
  let visible = $state(true);
  let running = false;
  let timer: ReturnType<typeof setInterval> | null = null;
  let acc = 0;               // hashs depuis la dernière mesure
  let tMark = 0;             // timestamp de la dernière mesure hps
  const BATCH = 128;         // hashs réels par lot (~1400/s mesurés) — léger, honnête

  function computeBatch() {
    if (!running) return;
    let digest = input;
    for (let n = 0; n < BATCH; n++) {
      bumpNonce();
      digest = blake3(input);
    }
    // Cascade : le digest courant descend dans la traîne — le calcul se VOIT couler.
    const prev = coreHash;
    if (prev) hashTrail = [prev, ...hashTrail].slice(0, 3);
    coreHash = bytesToHex(digest);
    totalHashes += BATCH;
    acc += BATCH;
    const now = performance.now();
    if (now - tMark >= 1000) { hps = Math.round((acc * 1000) / (now - tMark)); acc = 0; tMark = now; }
  }
  function play() {
    // Le cœur tourne TOUJOURS quand la carte est à l'écran — même sous
    // « réduire les animations » (on ralentit la cadence, on ne fige jamais :
    // un moteur « en direct » figé lirait comme un mensonge).
    const want = visible;
    const period = reduce ? 400 : 90;
    if (want && !timer) { running = true; tMark = performance.now(); acc = 0; timer = setInterval(computeBatch, period); }
    else if (!want && timer) { running = false; clearInterval(timer); timer = null; }
  }

  // ── Câblage : évènements + sondage + boucle, tout nettoyé au démontage ──
  $effect(() => {
    const unsubs: UnlistenFn[] = [];
    let alive = true;
    push("boot", tl("boot"));
    (async () => {
      const u1 = await listen<{ amount: number; amount_micro?: number; tx_hash?: string }>("quanta://mined", (e) => {
        const p = e.payload; const a = p?.amount ?? 0; if (a <= 0) return;
        // µQTA EXACTS + hash BLAKE3 réel de la tx de récompense — deux lignes
        // de récompense ne peuvent jamais être identiques.
        const u = p?.amount_micro ?? Math.round(a * 1e6);
        push("reward", fill(tl("eReward"), { u: u.toLocaleString("fr-FR"), a: a.toFixed(6), h: hshort(p?.tx_hash) }));
      });
      const u2 = await listen<{ index: number; txs: number; mine: boolean; hash?: string; prev?: string }>("quanta://block-sealed", (e) => {
        const p = e.payload; if (!p) return;
        // Le VRAI hash du bloc + son parent — l'ENCHAÎNEMENT (prev ← hash) est
        // visible ligne à ligne ; le cœur se re-chaîne dessus immédiatement.
        if (p.hash) { lastBlockHash = p.hash; height = Math.max(height, p.index); seedInput(); }
        const vars = { n: p.index, t: p.txs ?? 0, h: hshort(p.hash), p: hshort(p.prev) };
        if (p.mine) push("sealMine", fill(tl("eSealMine"), vars));
        else {
          push("seal", fill(tl("eSeal"), vars));
          // Honnête : ces vérifications tournent réellement à la réception
          // (validate_block_against_prev — proposeur bondé, couverture, Merkle).
          push("verify", fill(tl("eVerify"), { n: p.index }));
        }
      });
      const u3 = await listen<{ tx_type: string; amount_micro?: number; nonce?: number; hash?: string }>("quanta://tx-applied", (e) => {
        const p = e.payload; if (!p) return;
        push("tx", fill(tl("eTx"), {
          k: p.tx_type ?? "", u: (p.amount_micro ?? 0).toLocaleString("fr-FR"),
          o: p.nonce ?? 0, h: hshort(p.hash),
        }));
      });
      // « Sous le capot » : télémétrie RÉELLE du nœud (quanta://engine) —
      // chaque enveloppe gossip authentifiée (pipeline complet + ML-DSA) et
      // chaque vote de finalité que NOUS signons. Rien de synthétique.
      const u4 = await listen<any>("quanta://engine", (e) => {
        const p = e.payload; if (!p) return;
        if (p.kind === "envelope") {
          push("env", fill(tl("eEnv"), { m: p.msg ?? "?", s: (p.sender ?? "") + "…", n: p.nonce ?? 0 }));
        } else if (p.kind === "vote") {
          push("voteCast", fill(tl("eVoteCast"), { e: p.epoch ?? 0, h: p.hash ?? "" }));
        }
      });
      if (!alive) { u1(); u2(); u3(); u4(); return; }
      unsubs.push(u1, u2, u3, u4);
    })();
    return () => { alive = false; unsubs.forEach((u) => u()); };
  });

  $effect(() => {
    getVersion().then((v) => (appVersion = v)).catch(() => {});
    poll();
    const iv = setInterval(poll, 3000);
    // pause hors écran / onglet caché (perf + honnêteté : rien ne tourne caché)
    const onVis = () => { visible = !document.hidden; play(); };
    document.addEventListener("visibilitychange", onVis);
    let io: IntersectionObserver | undefined;
    if (rootEl && typeof IntersectionObserver !== "undefined") {
      io = new IntersectionObserver((es) => { visible = es[0]?.isIntersecting ?? true; play(); }, { threshold: 0.01 });
      io.observe(rootEl);
    }
    play();
    // Graine visible immédiate — untrack : sans lui, lire coreHash ici rendrait
    // l'effet dépendant d'un état réécrit toutes les 90 ms par computeBatch
    // (→ re-création interval/observers 11×/s).
    untrack(() => {
      if (!coreHash) { bumpNonce(); coreHash = bytesToHex(blake3(input)); }
    });
    return () => {
      clearInterval(iv);
      document.removeEventListener("visibilitychange", onVis);
      io?.disconnect();
      running = false; if (timer) clearInterval(timer); timer = null;
    };
  });

  const slot = $derived(height + 1);
  const hashGroups = $derived((coreHash || "0".repeat(64)).match(/.{1,4}/g) ?? []);
</script>

<div class="engine" class:reduce bind:this={rootEl} role="group" aria-label={tl("title")}>
  <div class="bar">
    <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
    <span class="bar-t">quanta · engine{appVersion ? ` · v${appVersion}` : ""}</span>
    <span class="bar-live" class:paused={!visible}><span class="dot"></span>{tl("live")}</span>
  </div>

  <!-- ── Cœur : BLAKE3 réel, en mouvement ── -->
  <div class="core">
    <div class="core-top">
      <div class="slot">{tl("slot")} <b>#{slot.toLocaleString("fr-FR")}</b></div>
      <div class="core-title">{tl("title")}</div>
    </div>
    <div class="hash" aria-live="off">
      <div class="hash-now">
        {#each hashGroups as g, i}<span class="hg" class:hot={i % 3 === 0}>{g}</span>{/each}
      </div>
      <!-- La traîne : les digests précédents s'estompent — preuve visuelle que
           le calcul coule (chaque ligne = un vrai BLAKE3 qui vient d'exister). -->
      {#each hashTrail as h, d}
        <div class="hash-prev" style="opacity:{0.42 - d * 0.13}">{h}</div>
      {/each}
    </div>
    <div class="core-meta">
      <span class="chain">{tl("chain")}</span>
      <span class="rate"><b>{hps.toLocaleString("fr-FR")}</b> {tl("hps")} · <b>{totalHashes.toLocaleString("fr-FR")}</b> {tl("computed")}</span>
    </div>
    {#if lastBlockHash}
      <div class="anchor">{tl("verified")} #{height.toLocaleString("fr-FR")} · {lastBlockHash.replace(/^0x/, "").slice(0, 16)}…</div>
    {/if}
  </div>

  <!-- ── Pipeline de consensus (les vraies étapes) ── -->
  <div class="pipe">
    {#each [tl("pBeacon"), tl("pElect"), tl("pSeal"), tl("pSig"), tl("pFinal")] as step, i}
      <div class="pstep"><span class="pi">{i + 1}</span>{step}</div>
      {#if i < 4}<span class="parrow" aria-hidden="true">→</span>{/if}
    {/each}
  </div>

  <!-- ── Stats consensus vivantes ── -->
  <div class="stats">
    <div class="st"><span class="sk">{tl("sHeight")}</span><span class="sv">{height.toLocaleString("fr-FR")}</span></div>
    <div class="st"><span class="sk">{tl("sEpoch")}</span><span class="sv">{epoch}</span></div>
    <div class="st"><span class="sk">{tl("sFloor")}</span><span class="sv">{floor.toLocaleString("fr-FR")}</span></div>
    <div class="st"><span class="sk">{tl("sVals")}</span><span class="sv">{validators}</span></div>
    <div class="st"><span class="sk">{tl("sStake")}</span><span class="sv">{totalStaked.toLocaleString("fr-FR")}</span></div>
    <div class="st"><span class="sk">{tl("sPeers")}</span><span class="sv">{peers}</span></div>
  </div>

  <div class="valrow" class:ok={iAmValidator}>{iAmValidator ? tl("youVal") : tl("becomeVal")}</div>

  {#if seenStats && peers === 0}
    <!-- Honnêteté : expliquer POURQUOI le journal se répète quand on est seul. -->
    <div class="solo">{tl("solo")}</div>
  {/if}

  <!-- ── Flux d'évènements réels ── -->
  <div class="log" role="log">
    {#each lines as line (line.id)}
      <div class="ln ln-{line.kind}">
        <span class="lt">{line.time}</span>
        <span class="lg" aria-hidden="true">{#if line.kind === "sealMine"}◆{:else if line.kind === "reward"}✦{:else if line.kind === "final"}●{:else if line.kind === "vote" || line.kind === "voteCast"}◇{:else if line.kind === "seal"}▪{:else if line.kind === "verify"}✓{:else if line.kind === "env"}⇄{:else if line.kind === "tx"}→{:else if line.kind === "peer"}↺{:else}›{/if}</span>
        <span class="lx">{line.text}</span>
      </div>
    {/each}
  </div>

  <p class="note">{tl("note")}</p>
</div>

<style>
  .engine {
    --bg: #0a0c10; --panel: #12151c; --line: rgba(255,255,255,0.07);
    --txt: #cdd2db; --dim: #6a7080; --teal: #14c8b8; --tealdim: rgba(20,200,184,0.12);
    background: var(--bg); border: 1px solid #1b1f28; border-radius: var(--radius-lg);
    overflow: hidden; box-shadow: var(--shadow); color: var(--txt);
    font-family: var(--font-mono);
  }
  .bar { display: flex; align-items: center; gap: 10px; padding: 11px 16px; border-bottom: 1px solid var(--line); background: #07090c; }
  .dots { display: inline-flex; gap: 6px; }
  .dots i { width: 9px; height: 9px; border-radius: 50%; background: #262a34; display: block; }
  .bar-t { font-size: 12px; color: var(--dim); letter-spacing: 0.02em; }
  .bar-live { margin-left: auto; display: inline-flex; align-items: center; gap: 7px; font-size: 11px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: var(--teal); font-family: var(--font-display); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--teal); animation: pulse 2s ease infinite; }
  .bar-live.paused .dot { animation: none; opacity: 0.4; }
  @keyframes pulse { 0%,100% { box-shadow: 0 0 0 0 rgba(20,200,184,0.45); } 50% { box-shadow: 0 0 0 5px rgba(20,200,184,0); } }

  /* ── Cœur ── */
  .core { padding: 22px 22px 18px; }
  .core-top { display: flex; align-items: baseline; justify-content: space-between; margin-bottom: 14px; }
  .slot { font-size: 13px; color: var(--dim); letter-spacing: 0.08em; }
  .slot b { color: #eef2f7; font-size: 15px; }
  .core-title { font-family: var(--font-display); font-size: 13px; font-weight: 700; color: var(--teal); letter-spacing: 0.02em; }
  .hash {
    padding: 18px; border-radius: var(--radius-sm);
    background: #06080b; border: 1px solid var(--line); min-height: 132px;
  }
  .hash-now {
    display: flex; flex-wrap: wrap; gap: 6px 10px;
    font-size: 18px; line-height: 1.5; word-break: break-all;
  }
  .hg { color: #98a0b0; }
  .hg.hot { color: var(--teal); }
  .hash-prev {
    margin-top: 7px; font-size: 12.5px; letter-spacing: 0.04em;
    color: #7b849a; word-break: break-all; line-height: 1.4;
  }
  .reduce .hash { filter: none; }
  .core-meta { display: flex; align-items: baseline; justify-content: space-between; flex-wrap: wrap; gap: 6px 16px; margin-top: 12px; }
  .chain { font-size: 11.5px; color: var(--dim); text-transform: uppercase; letter-spacing: 0.06em; }
  .rate { font-size: 12.5px; color: var(--txt); font-family: var(--font-display); }
  .rate b { color: var(--teal); font-variant-numeric: tabular-nums; }
  .anchor { margin-top: 8px; font-size: 11px; color: var(--dim); }

  /* ── Pipeline ── */
  .pipe { display: flex; align-items: center; flex-wrap: wrap; gap: 6px 4px; padding: 4px 22px 16px; }
  .pstep { display: inline-flex; align-items: center; gap: 7px; font-family: var(--font-display); font-size: 12px; color: var(--txt); background: var(--panel); border: 1px solid var(--line); border-radius: 999px; padding: 6px 12px; }
  .pi { width: 16px; height: 16px; border-radius: 50%; background: var(--tealdim); color: var(--teal); font-size: 10px; font-weight: 700; display: inline-flex; align-items: center; justify-content: center; }
  .parrow { color: var(--dim); font-size: 12px; }

  /* ── Stats ── */
  .stats { display: grid; grid-template-columns: repeat(6, 1fr); gap: 1px; background: var(--line); border-top: 1px solid var(--line); border-bottom: 1px solid var(--line); }
  @media (max-width: 640px) { .stats { grid-template-columns: repeat(3, 1fr); } }
  .st { background: var(--bg); padding: 12px 14px; display: flex; flex-direction: column; gap: 4px; }
  .sk { font-size: 10px; color: var(--dim); text-transform: uppercase; letter-spacing: 0.06em; font-family: var(--font-display); }
  .sv { font-size: 16px; font-weight: 700; color: #eef2f7; font-family: var(--font-display); font-variant-numeric: tabular-nums lining-nums; }

  .valrow { padding: 12px 22px; font-family: var(--font-display); font-size: 12px; color: var(--dim); border-bottom: 1px solid var(--line); }
  .valrow.ok { color: #bff3ee; }
  .solo {
    margin: 10px 14px 0; padding: 10px 14px;
    font-family: var(--font-display); font-size: 12px; line-height: 1.55;
    color: #9aa3b2; background: var(--panel);
    border: 1px solid var(--line); border-radius: var(--radius-sm);
  }

  /* ── Log ── */
  .log { padding: 10px 14px; height: 200px; overflow: hidden; -webkit-mask-image: linear-gradient(180deg,#000 80%,transparent 100%); mask-image: linear-gradient(180deg,#000 80%,transparent 100%); }
  .ln { display: flex; align-items: baseline; gap: 10px; padding: 4px 8px; border-radius: 6px; font-size: 12px; line-height: 1.4; animation: lnin 0.24s ease-out; }
  .reduce .ln { animation: none; }
  @keyframes lnin { from { opacity: 0; transform: translateY(-4px); } to { opacity: 1; transform: none; } }
  .lt { color: #495060; flex-shrink: 0; }
  .lg { color: var(--dim); width: 12px; text-align: center; flex-shrink: 0; }
  .lx { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .ln-reward .lg, .ln-vote .lg { color: var(--teal); } .ln-reward .lx { color: #dff6f3; }
  .ln-sealMine { background: rgba(20,200,184,0.09); border: 1px solid rgba(20,200,184,0.22); }
  .ln-sealMine .lg { color: var(--teal); } .ln-sealMine .lx { color: #d6f5f1; font-weight: 600; }
  .ln-final .lg { color: var(--teal); } .ln-final .lx { color: #cdeee9; }
  .ln-verify .lg { color: var(--teal); } .ln-verify .lx { color: #9aa3b2; }
  .ln-env .lg { color: #7b849a; } .ln-env .lx { color: #a9b1c0; }
  .ln-voteCast .lg { color: var(--teal); } .ln-voteCast .lx { color: #dff6f3; font-weight: 600; }
  .ln-boot .lx, .ln-boot .lg { color: var(--dim); }

  .note { padding: 14px 22px 18px; font-size: 11.5px; color: var(--dim); line-height: 1.55; border-top: 1px solid var(--line); font-family: var(--font-display); max-width: 78ch; }

  :global(.engine) ::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.18); }
</style>
