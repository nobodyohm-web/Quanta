<script lang="ts">
  // ═══════════════════════════════════════════════════════════════════
  //  ForgeEngine — le moteur de consensus du nœud, en direct.
  //  La SEULE surface sombre de l'app (un moteur EST sombre).
  //  Tout ce qui s'affiche ici est un fait du nœud : événements Tauri
  //  (récompenses, scellements, enveloppes, élections PoS, votes de
  //  finalité), timings ML-DSA mesurés côté Rust (µs), et l'ancre de
  //  chaîne = le hash du dernier bloc réel. Aucune animation de
  //  remplissage, aucun calcul décoratif.
  // ═══════════════════════════════════════════════════════════════════
  import { untrack } from "svelte";
  import { getNodeStatus, getFinalityStatus, getChainHistory } from "./api";
  import { getVersion } from "@tauri-apps/api/app";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { locale } from "./i18n.svelte";
  import { lastStall } from "./diag";

  const reduce =
    typeof window !== "undefined" && !!window.matchMedia &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ── i18n local (6 langues concises) ──────────────────────────────
  const L: Record<string, Record<string, string>> = {
    en: { title: "Consensus engine", live: "live", slot: "SLOT",
      anchorLine: "block #{n} · sealed {t} ago", anchorBare: "block #{n}",
      uS: "s", uMin: "min", uH: "h",
      pBeacon: "Beacon", pElect: "Leader election", pSeal: "Block seal", pSig: "ML-DSA signature", pFinal: "Finality",
      sHeight: "Height", sEpoch: "Epoch", sFloor: "Finalized", sVals: "Validators", sStake: "Staked", sPeers: "Peers",
      fAll: "All", fBlocks: "Blocks", fCrypto: "Crypto", fNetwork: "Network", fAlerts: "Alerts",
      youVal: "You are a validator — you can be elected to seal blocks.",
      becomeVal: "Stake ≥ 1 QUANTA to become a validator and seal blocks.",
      note: "Proof-of-Stake — leaders are elected by on-chain stake; every block is sealed under an ML-DSA-65 signature and becomes irreversible once an epoch gathers a ⅔-stake certificate.",
      boot: "node online",
      solo: "0 peers — only your own node's work appears here.",
      eReward: "reward minted  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "BLOCK SEALED #{n} · {t} tx · {h} ← {p} · {d} µs",
      eSeal: "block #{n} sealed · {t} tx · {h} ← {p}", eVerify: "block #{n} verified — PoS proposer ✓ · coverage ✓ · Merkle ✓", eState: "chain #{n} · epoch {e} · {v} validators", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "finality vote · epoch {n}", eEnv: "{m} from {s} · nonce {n} — ML-DSA ✓ {us} µs", eVoteCast: "OUR finality vote signed — epoch {e} → #{h}",
      eFinal: "block #{n} finalized · irreversible", ePeer: "peer connected · {n} peers",
      eSign: "ML-DSA-65 signed · {m} envelope · {b} B · {us} µs", ePersist: "disk snapshot · {k} states · {b} KB · {ms} ms",
      eElectLead: "slot #{s} — ELECTED leader ({v} validators) — sealing", eElectFall: "slot #{s} — fallback proposer — sealing",
      eElectObs: "slot #{s} — another validator leads ({v}) — observing", eElectBoot: "slot #{s} — permissionless bootstrap (no stake yet)",
      eStall: "⚠ UI thread stalled {ms} ms" },
    fr: { title: "Moteur de consensus", live: "en direct", slot: "SLOT",
      anchorLine: "bloc #{n} · scellé il y a {t}", anchorBare: "bloc #{n}",
      uS: "s", uMin: "min", uH: "h",
      pBeacon: "Beacon", pElect: "Élection du leader", pSeal: "Scellement", pSig: "Signature ML-DSA", pFinal: "Finalité",
      sHeight: "Hauteur", sEpoch: "Époque", sFloor: "Finalisé", sVals: "Validateurs", sStake: "Enjeu", sPeers: "Pairs",
      fAll: "Tout", fBlocks: "Blocs", fCrypto: "Crypto", fNetwork: "Réseau", fAlerts: "Alertes",
      youVal: "Tu es validateur — tu peux être élu pour sceller des blocs.",
      becomeVal: "Stake ≥ 1 QUANTA pour devenir validateur et sceller des blocs.",
      note: "Proof-of-Stake — le leader est élu par l'enjeu on-chain ; chaque bloc est scellé sous signature ML-DSA-65 et devient irréversible dès qu'une époque réunit un certificat aux ⅔ de l'enjeu.",
      boot: "nœud en ligne",
      solo: "0 pair — seul le travail de ton propre nœud apparaît ici.",
      eReward: "récompense minée  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "BLOC SCELLÉ #{n} · {t} tx · {h} ← {p} · {d} µs",
      eSeal: "bloc #{n} scellé · {t} tx · {h} ← {p}", eVerify: "bloc #{n} vérifié — proposeur PoS ✓ · couverture ✓ · Merkle ✓", eState: "chaîne #{n} · époque {e} · {v} validateurs", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "vote de finalité · époque {n}", eEnv: "{m} de {s} · nonce {n} — ML-DSA ✓ {us} µs", eVoteCast: "NOTRE vote de finalité signé — époque {e} → #{h}",
      eFinal: "bloc #{n} finalisé · irréversible", ePeer: "pair connecté · {n} pairs",
      eSign: "ML-DSA-65 signée · enveloppe {m} · {b} o · {us} µs", ePersist: "snapshot disque · {k} états · {b} Ko · {ms} ms",
      eElectLead: "slot #{s} — ÉLU leader ({v} validateurs) — scellement", eElectFall: "slot #{s} — proposeur fallback — scellement",
      eElectObs: "slot #{s} — un autre validateur mène ({v}) — on observe", eElectBoot: "slot #{s} — bootstrap permissionless (personne n'a staké)",
      eStall: "⚠ fil UI bloqué {ms} ms" },
    es: { title: "Motor de consenso", live: "en vivo", slot: "SLOT",
      anchorLine: "bloque #{n} · sellado hace {t}", anchorBare: "bloque #{n}",
      uS: "s", uMin: "min", uH: "h",
      pBeacon: "Beacon", pElect: "Elección de líder", pSeal: "Sellado", pSig: "Firma ML-DSA", pFinal: "Finalidad",
      sHeight: "Altura", sEpoch: "Época", sFloor: "Finalizado", sVals: "Validadores", sStake: "Stake", sPeers: "Pares",
      fAll: "Todo", fBlocks: "Bloques", fCrypto: "Cripto", fNetwork: "Red", fAlerts: "Alertas",
      youVal: "Eres validador — puedes ser elegido para sellar bloques.",
      becomeVal: "Haz stake ≥ 1 QUANTA para ser validador y sellar bloques.",
      note: "Proof-of-Stake — el líder se elige por el stake on-chain; cada bloque se sella con una firma ML-DSA-65 y se vuelve irreversible cuando una época reúne un certificado de ⅔ del stake.",
      boot: "nodo en línea",
      solo: "0 pares — aquí solo aparece el trabajo de tu propio nodo.",
      eReward: "recompensa minada  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "BLOQUE SELLADO #{n} · {t} tx · {h} ← {p} · {d} µs",
      eSeal: "bloque #{n} sellado · {t} tx · {h} ← {p}", eVerify: "bloque #{n} verificado — proponente PoS ✓ · cobertura ✓ · Merkle ✓", eState: "cadena #{n} · época {e} · {v} validadores", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "voto de finalidad · época {n}", eEnv: "{m} de {s} · nonce {n} — ML-DSA ✓ {us} µs", eVoteCast: "NUESTRO voto de finalidad firmado — época {e} → #{h}",
      eFinal: "bloque #{n} finalizado · irreversible", ePeer: "par conectado · {n} pares",
      eSign: "ML-DSA-65 firmada · sobre {m} · {b} B · {us} µs", ePersist: "snapshot a disco · {k} estados · {b} KB · {ms} ms",
      eElectLead: "slot #{s} — líder ELEGIDO ({v} validadores) — sellando", eElectFall: "slot #{s} — proponente fallback — sellando",
      eElectObs: "slot #{s} — lidera otro validador ({v}) — observando", eElectBoot: "slot #{s} — bootstrap permissionless (nadie ha stakeado)",
      eStall: "⚠ hilo UI bloqueado {ms} ms" },
    ru: { title: "Движок консенсуса", live: "в эфире", slot: "СЛОТ",
      anchorLine: "блок #{n} · запечатан {t} назад", anchorBare: "блок #{n}",
      uS: "с", uMin: "мин", uH: "ч",
      pBeacon: "Маяк", pElect: "Выбор лидера", pSeal: "Запечатывание", pSig: "Подпись ML-DSA", pFinal: "Финальность",
      sHeight: "Высота", sEpoch: "Эпоха", sFloor: "Финализ.", sVals: "Валидаторы", sStake: "Стейк", sPeers: "Пиры",
      fAll: "Все", fBlocks: "Блоки", fCrypto: "Крипто", fNetwork: "Сеть", fAlerts: "Оповещения",
      youVal: "Вы валидатор — вас могут выбрать запечатывать блоки.",
      becomeVal: "Застейкайте ≥ 1 QUANTA, чтобы стать валидатором.",
      note: "Proof-of-Stake — лидера выбирает ончейн-стейк; каждый блок запечатывается подписью ML-DSA-65 и становится необратимым, когда эпоха собирает сертификат ⅔ стейка.",
      boot: "узел в сети",
      solo: "0 пиров — здесь видна только работа вашего узла.",
      eReward: "награда добыта  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "БЛОК ЗАПЕЧАТАН #{n} · {t} tx · {h} ← {p} · {d} мкс",
      eSeal: "блок #{n} запечатан · {t} tx · {h} ← {p}", eVerify: "блок #{n} проверен — PoS-предлагатель ✓ · покрытие ✓ · Merkle ✓", eState: "цепь #{n} · эпоха {e} · {v} валидаторов", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "голос финальности · эпоха {n}", eEnv: "{m} от {s} · nonce {n} — ML-DSA ✓ {us} мкс", eVoteCast: "НАШ голос финальности подписан — эпоха {e} → #{h}",
      eFinal: "блок #{n} финализирован · необратимо", ePeer: "пир подключён · {n} пиров",
      eSign: "ML-DSA-65 подписан · конверт {m} · {b} Б · {us} мкс", ePersist: "снапшот на диск · {k} состояний · {b} КБ · {ms} мс",
      eElectLead: "слот #{s} — ИЗБРАН лидером ({v} валидаторов) — запечатываем", eElectFall: "слот #{s} — резервный предлагатель — запечатываем",
      eElectObs: "слот #{s} — лидирует другой валидатор ({v}) — наблюдаем", eElectBoot: "слот #{s} — permissionless-бутстрап (никто не застейкал)",
      eStall: "⚠ поток UI завис на {ms} мс" },
    zh: { title: "共识引擎", live: "实时", slot: "时隙",
      anchorLine: "区块 #{n} · 封存于 {t} 前", anchorBare: "区块 #{n}",
      uS: "秒", uMin: "分钟", uH: "小时",
      pBeacon: "信标", pElect: "出块者选举", pSeal: "封存", pSig: "ML-DSA 签名", pFinal: "最终性",
      sHeight: "高度", sEpoch: "纪元", sFloor: "已最终确定", sVals: "验证者", sStake: "质押", sPeers: "节点",
      fAll: "全部", fBlocks: "区块", fCrypto: "加密", fNetwork: "网络", fAlerts: "警报",
      youVal: "你是验证者——可被选为出块者封存区块。",
      becomeVal: "质押 ≥ 1 QUANTA 即可成为验证者并封存区块。",
      note: "权益证明——出块者由链上质押选出；每个区块以 ML-DSA-65 签名封存，当一个纪元集齐 ⅔ 质押的证书后即不可逆转。",
      boot: "节点在线",
      solo: "0 个对等节点——这里只显示你自己节点的工作。",
      eReward: "已获挖矿奖励  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "区块已封存 #{n} · {t} 笔 · {h} ← {p} · {d} µs",
      eSeal: "区块 #{n} 已封存 · {t} 笔 · {h} ← {p}", eVerify: "区块 #{n} 已验证 — PoS 出块者 ✓ · 覆盖 ✓ · Merkle ✓", eState: "链 #{n} · 纪元 {e} · {v} 个验证者", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "最终性投票 · 纪元 {n}", eEnv: "{m} 来自 {s} · nonce {n} — ML-DSA ✓ {us} µs", eVoteCast: "我们的最终性投票已签名 — 纪元 {e} → #{h}",
      eFinal: "区块 #{n} 已最终确定 · 不可逆", ePeer: "节点已连接 · {n} 个节点",
      eSign: "ML-DSA-65 已签名 · {m} 信封 · {b} 字节 · {us} µs", ePersist: "磁盘快照 · {k} 个状态 · {b} KB · {ms} ms",
      eElectLead: "时隙 #{s} — 当选出块者（{v} 个验证者）— 封存中", eElectFall: "时隙 #{s} — 后备提议者 — 封存中",
      eElectObs: "时隙 #{s} — 由其他验证者出块（{v}）— 观察中", eElectBoot: "时隙 #{s} — 无许可引导（尚无质押）",
      eStall: "⚠ 界面线程卡顿 {ms} ms" },
    ja: { title: "コンセンサスエンジン", live: "ライブ", slot: "スロット",
      anchorLine: "ブロック #{n} · {t} 前に封印", anchorBare: "ブロック #{n}",
      uS: "秒", uMin: "分", uH: "時間",
      pBeacon: "ビーコン", pElect: "リーダー選出", pSeal: "封印", pSig: "ML-DSA 署名", pFinal: "ファイナリティ",
      sHeight: "高さ", sEpoch: "エポック", sFloor: "確定", sVals: "検証者", sStake: "ステーク", sPeers: "ピア",
      fAll: "すべて", fBlocks: "ブロック", fCrypto: "暗号", fNetwork: "ネットワーク", fAlerts: "アラート",
      youVal: "あなたは検証者です — 選ばれてブロックを封印できます。",
      becomeVal: "1 QUANTA 以上ステークすると検証者になれます。",
      note: "プルーフ・オブ・ステーク — リーダーはオンチェーンのステークで選出されます。各ブロックは ML-DSA-65 署名で封印され、エポックがステークの 2/3 証明書を集めると不可逆になります。",
      boot: "ノードはオンライン",
      solo: "ピア 0 — ここには自分のノードの仕事だけが表示されます。",
      eReward: "報酬を採掘  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "ブロック封印 #{n} · {t} tx · {h} ← {p} · {d} µs",
      eSeal: "ブロック #{n} 封印 · {t} tx · {h} ← {p}", eVerify: "ブロック #{n} 検証済 — PoS 提案者 ✓ · カバレッジ ✓ · Merkle ✓", eState: "チェーン #{n} · エポック {e} · {v} 検証者", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "ファイナリティ投票 · エポック {n}", eEnv: "{m} ({s}) · nonce {n} — ML-DSA ✓ {us} µs", eVoteCast: "私たちのファイナリティ投票に署名 — エポック {e} → #{h}",
      eFinal: "ブロック #{n} 確定 · 不可逆", ePeer: "ピア接続 · {n} ピア",
      eSign: "ML-DSA-65 署名 · {m} エンベロープ · {b} B · {us} µs", ePersist: "ディスクスナップショット · {k} 状態 · {b} KB · {ms} ms",
      eElectLead: "スロット #{s} — リーダーに当選（{v} 検証者）— 封印", eElectFall: "スロット #{s} — フォールバック提案者 — 封印",
      eElectObs: "スロット #{s} — 別の検証者がリード（{v}）— 観測中", eElectBoot: "スロット #{s} — パーミッションレス・ブートストラップ（ステークなし）",
      eStall: "⚠ UI スレッド {ms} ms 停止" },
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
  let appVersion = $state("");
  let signUs = $state(0);   // dernière signature ML-DSA-65 (µs, mesurée côté Rust)
  let verifyUs = $state(0); // dernier pipeline d'enveloppe vérifié (µs, mesuré côté Rust)

  // Heure d'arrivée locale du dernier scellement observé → « scellé il y a X ».
  let lastSealAt = $state(0);
  let nowTick = $state(Date.now());
  function fmtAgo(ms: number): string {
    const s = Math.max(0, Math.round(ms / 1000));
    if (s < 90) return `${s} ${tl("uS")}`;
    const m = Math.round(s / 60);
    if (m < 90) return `${m} ${tl("uMin")}`;
    return `${Math.round(m / 60)} ${tl("uH")}`;
  }

  // ── Pipeline vivant : l'étape courante s'allume sur l'événement réel ─
  //    0 Beacon (repos) · 1 Élection · 2 Scellement · 3 Signature · 4 Finalité
  let pipeStep = $state(0);
  let pipeTimer: ReturnType<typeof setTimeout> | null = null;
  function lightStep(i: number) {
    pipeStep = i;
    if (pipeTimer) clearTimeout(pipeTimer);
    // Retombe au repos (beacon) après 8 s sans nouvel événement de consensus.
    pipeTimer = setTimeout(() => { pipeStep = 0; pipeTimer = null; }, 8000);
  }

  // ── Flux d'évènements réels ──────────────────────────────────────
  interface Line { id: number; kind: string; time: string; ts: number; text: string; }
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
    // Buffer 200 lignes (le tableau garde TOUT ; les filtres n'affectent que
    // l'affichage — voir filteredLines plus bas).
    lines = [{ id: ++seq, kind, time: stamp(), ts: Date.now(), text }, ...prev].slice(0, 200);
  }

  // ── Filtres d'affichage (le buffer `lines` reste complet) ────────
  type FilterKey = "all" | "blocks" | "crypto" | "network" | "alerts";
  let activeFilter = $state<FilterKey>("all");
  const FILTERS: FilterKey[] = ["all", "blocks", "crypto", "network", "alerts"];
  const FILTER_LABEL: Record<FilterKey, string> = { all: "fAll", blocks: "fBlocks", crypto: "fCrypto", network: "fNetwork", alerts: "fAlerts" };
  function kindCategory(kind: string): FilterKey {
    // blocs : le squelette de la chaîne (scellement, vérification, finalité,
    // + la forensique par-scellement qui vient de quanta.lastSeal)
    if (kind === "seal" || kind === "sealMine" || kind === "verify" || kind === "final" || kind === "forensic") return "blocks";
    // crypto : signatures ML-DSA + récompense minée
    if (kind === "sign" || kind === "reward") return "crypto";
    // alertes : anomalies mesurées (fil UI bloqué)
    if (kind === "stall") return "alerts";
    // réseau : tout le reste — gossip, élection, votes, tx appliquées, persistance, boot
    return "network";
  }
  const filteredLines = $derived(
    activeFilter === "all" ? lines : lines.filter((l) => kindCategory(l.kind) === activeFilter),
  );

  // ── Copie d'une ligne au clic (presse-papiers + flash bref) ──────
  let flashId = $state<number | null>(null);
  let flashTimer: ReturnType<typeof setTimeout> | null = null;
  function copyLine(line: Line) {
    if (typeof navigator !== "undefined" && navigator.clipboard) {
      navigator.clipboard.writeText(`${line.time} ${line.text}`).catch(() => {});
    }
    if (flashTimer) clearTimeout(flashTimer);
    flashId = line.id;
    flashTimer = setTimeout(() => { flashId = null; flashTimer = null; }, 420);
  }

  let seenStats = $state(false);
  let statePushed = false;
  async function poll() {
    try {
      const s = await getNodeStatus();
      const p = s?.peer_count ?? 0;
      if (seenStats && p !== peers && p > 0) push("peer", fill(tl("ePeer"), { n: p }));
      peers = p;
    } catch {}
    try {
      const f = await getFinalityStatus();
      if (f) {
        if (seenStats && f.epoch > epoch) push("vote", fill(tl("eVote"), { n: f.epoch }));
        if (seenStats && f.finalized_floor > floor && f.finalized_floor > 0) {
          push("final", fill(tl("eFinal"), { n: f.finalized_floor.toLocaleString("fr-FR") }));
          lightStep(4);
        }
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
      const c = await getChainHistory();
      const rec = c?.recent;
      if (Array.isArray(rec) && rec.length) {
        const top = rec[rec.length - 1];
        if (top?.hash) lastBlockHash = top.hash;
      }
    } catch {}
  }

  // ── Câblage : évènements + sondage, tout nettoyé au démontage ──
  $effect(() => {
    const unsubs: UnlistenFn[] = [];
    let alive = true;
    push("boot", tl("boot"));
    (async () => {
      const u1 = await listen<{ amount: number; amount_micro?: number; tx_hash?: string }>("quanta://mined", (e) => {
        const p = e.payload; const a = p?.amount ?? 0; if (a <= 0) return;
        // µQTA exacts + hash BLAKE3 de la tx de récompense.
        const u = p?.amount_micro ?? Math.round(a * 1e6);
        push("reward", fill(tl("eReward"), { u: u.toLocaleString("fr-FR"), a: a.toFixed(6), h: hshort(p?.tx_hash) }));
      });
      const u2 = await listen<{ index: number; txs: number; mine: boolean; hash?: string; prev?: string; seal_us?: number }>("quanta://block-sealed", (e) => {
        const p = e.payload; if (!p) return;
        // Le hash du bloc + son parent — l'enchaînement (prev ← hash) est
        // visible ligne à ligne ; l'ancre du cœur bascule dessus.
        if (p.hash) { lastBlockHash = p.hash; height = Math.max(height, p.index); }
        lastSealAt = Date.now();
        lightStep(2);
        const vars = { n: p.index, t: p.txs ?? 0, h: hshort(p.hash), p: hshort(p.prev) };
        if (p.mine) push("sealMine", fill(tl("eSealMine"), { ...vars, d: (p.seal_us ?? 0).toLocaleString("fr-FR") }));
        else {
          push("seal", fill(tl("eSeal"), vars));
          // Ces vérifications tournent à la réception (validate_block_against_prev
          // — proposeur bondé, couverture, Merkle).
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
      // Télémétrie du nœud (quanta://engine) : enveloppes gossip authentifiées
      // (pipeline complet + ML-DSA), signatures sortantes, élections PoS,
      // votes de finalité, snapshots disque.
      const u4 = await listen<any>("quanta://engine", (e) => {
        const p = e.payload; if (!p) return;
        if (p.kind === "envelope") {
          verifyUs = p.us ?? 0;
          push("env", fill(tl("eEnv"), { m: p.msg ?? "?", s: (p.sender ?? "") + "…", n: p.nonce ?? 0, us: (p.us ?? 0).toLocaleString("fr-FR") }));
        } else if (p.kind === "vote") {
          lightStep(4);
          push("voteCast", fill(tl("eVoteCast"), { e: p.epoch ?? 0, h: p.hash ?? "" }));
        } else if (p.kind === "sign") {
          // Durée réelle de la signature ML-DSA-65 de l'enveloppe sortante.
          signUs = p.us ?? 0;
          lightStep(3);
          push("sign", fill(tl("eSign"), { m: p.msg ?? "?", b: p.bytes ?? 0, us: (p.us ?? 0).toLocaleString("fr-FR") }));
        } else if (p.kind === "persist") {
          // Battement 30 s : l'écriture disque du snapshot d'état.
          push("persist", fill(tl("ePersist"), { k: p.keys ?? 0, b: Math.max(1, Math.round((p.bytes ?? 0) / 1024)), ms: p.ms ?? 0 }));
        } else if (p.kind === "elect") {
          // Verdict de l'élection PoS de ce slot.
          lightStep(1);
          const key = p.verdict === "leader" ? "eElectLead"
            : p.verdict === "fallback" ? "eElectFall"
            : p.verdict === "bootstrap" ? "eElectBoot" : "eElectObs";
          const kind = p.verdict === "leader" || p.verdict === "fallback" ? "electLead" : "elect";
          push(kind, fill(tl(key), { s: (p.slot ?? 0).toLocaleString("fr-FR"), v: p.validators ?? 0 }));
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
    // Ticker 1 s du « scellé il y a X » (léger : une écriture d'entier).
    const tk = setInterval(() => { nowTick = Date.now(); }, 1000);
    // Watchdog du fil UI : si le thread principal bloque > 900 ms, le terminal
    // l'écrit lui-même, mesuré — un gel devient une donnée datée, pas un mystère.
    let lastBeat = performance.now();
    const wd = setInterval(() => {
      const nowB = performance.now();
      const gap = nowB - lastBeat;
      lastBeat = nowB;
      if (gap > 900) push("stall", fill(tl("eStall"), { ms: Math.round(gap) }));
    }, 250);
    // Rapports de la sonde globale (diag.ts) : un gel survenu sur n'importe
    // quelle page — avec l'anneau des opérations qui l'entouraient — remonte
    // ici, dans le terminal, copiable tel quel.
    let seenStall = lastStall() ?? "";
    if (seenStall) push("stall", seenStall.slice(0, 220));
    // La forensique par scellement (diag.ts) s'affiche aussi ici : chaque bloc
    // laisse sa ligne de vérité mesurée.
    let seenSeal = "";
    try { seenSeal = localStorage.getItem("quanta.lastSeal") ?? ""; } catch { /* best-effort */ }
    const sd = setInterval(() => {
      const s = lastStall();
      if (s && s !== seenStall) { seenStall = s; push("stall", s.slice(0, 220)); }
      try {
        const f = localStorage.getItem("quanta.lastSeal");
        if (f && f !== seenSeal) { seenSeal = f; push("forensic", f.slice(25, 245)); }
      } catch { /* best-effort */ }
    }, 5000);
    return () => {
      clearInterval(iv);
      clearInterval(tk);
      clearInterval(wd);
      clearInterval(sd);
      if (pipeTimer) { clearTimeout(pipeTimer); pipeTimer = null; }
      if (flashTimer) { clearTimeout(flashTimer); flashTimer = null; }
    };
  });

  const slot = $derived(height + 1);
  // L'ancre du cœur : le hash du dernier bloc réel, groupé par 4.
  const hashGroups = $derived(
    (lastBlockHash.replace(/^0x/, "") || "·".repeat(64)).match(/.{1,4}/g) ?? [],
  );
  const sealAgo = $derived(lastSealAt > 0 ? fmtAgo(nowTick - lastSealAt) : "");
</script>

<div class="engine" class:reduce role="group" aria-label={tl("title")}>
  <div class="bar">
    <span class="bar-t">quanta · engine{appVersion ? ` · v${appVersion}` : ""}</span>
    <span class="bar-live"><span class="dot"></span>{tl("live")}</span>
  </div>

  <!-- ── Cœur : l'ancre de chaîne — le hash du dernier bloc scellé ── -->
  <div class="core">
    <div class="core-top">
      <div class="slot">{tl("slot")} <b>#{slot.toLocaleString("fr-FR")}</b></div>
      <div class="core-title">{tl("title")}</div>
    </div>
    {#key lastBlockHash}
      <div class="hash" class:fresh={lastBlockHash !== ""} aria-live="off">
        <div class="hash-now">
          {#each hashGroups as g, i}<span class="hg" class:hot={i % 3 === 0}>{g}</span>{/each}
        </div>
      </div>
    {/key}
    <div class="core-meta">
      <span class="anchor">
        {#if lastBlockHash}
          {sealAgo
            ? fill(tl("anchorLine"), { n: height.toLocaleString("fr-FR"), t: sealAgo })
            : fill(tl("anchorBare"), { n: height.toLocaleString("fr-FR") })}
        {/if}
      </span>
      {#if signUs || verifyUs}
        <!-- Timings mesurés côté Rust (Instant autour de l'opération). -->
        <span class="crypt">
          {#if signUs}<span>ML-DSA-65 sign <b>{signUs.toLocaleString("fr-FR")}</b> µs</span>{/if}
          {#if verifyUs}<span>verify <b>{verifyUs.toLocaleString("fr-FR")}</b> µs</span>{/if}
        </span>
      {/if}
    </div>
  </div>

  <!-- ── Pipeline de consensus vivant : l'étape courante s'allume ── -->
  <div class="pipe">
    {#each [tl("pBeacon"), tl("pElect"), tl("pSeal"), tl("pSig"), tl("pFinal")] as step, i}
      <div class="pstep" class:on={pipeStep === i}><span class="pi">{i + 1}</span>{step}</div>
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
    <div class="solo">{tl("solo")}</div>
  {/if}

  <!-- ── Filtres — l'affichage seul change, le buffer garde tout ── -->
  <div class="chips" role="group" aria-label={tl("fAll")}>
    {#each FILTERS as f}
      <button type="button" class="chip" class:active={activeFilter === f} onclick={() => (activeFilter = f)}>{tl(FILTER_LABEL[f])}</button>
    {/each}
  </div>

  <!-- ── Flux d'évènements réels (scrollable — l'historique t'appartient) ── -->
  <div class="log" role="log">
    {#each filteredLines as line (line.id)}
      <div
        class="ln ln-{line.kind}"
        class:flash={flashId === line.id}
        role="button"
        tabindex="0"
        onclick={() => copyLine(line)}
        onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); copyLine(line); } }}
      >
        <span class="lt">{line.time}</span>
        <span class="lg" aria-hidden="true">{#if line.kind === "sealMine" || line.kind === "seal"}◆{:else if line.kind === "final" || line.kind === "vote" || line.kind === "voteCast"}●{:else if line.kind === "sign" || line.kind === "reward"}⬡{:else if line.kind === "verify"}✓{:else if line.kind === "stall"}⚠{:else if line.kind === "electLead"}▲{:else}›{/if}</span>
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
  .bar-t { font-size: 12px; color: var(--dim); letter-spacing: 0.02em; }
  .bar-live { margin-left: auto; display: inline-flex; align-items: center; gap: 7px; font-size: 11px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: var(--teal); font-family: var(--font-display); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--teal); animation: pulse 2s ease infinite; }
  .reduce .dot { animation: none; }
  @keyframes pulse { 0%,100% { box-shadow: 0 0 0 0 rgba(20,200,184,0.45); } 50% { box-shadow: 0 0 0 5px rgba(20,200,184,0); } }

  /* ── Cœur ── */
  .core { padding: 22px 22px 18px; }
  .core-top { display: flex; align-items: baseline; justify-content: space-between; margin-bottom: 14px; }
  .slot { font-size: 13px; color: var(--dim); letter-spacing: 0.08em; }
  .slot b { color: #eef2f7; font-size: 15px; }
  .core-title { font-family: var(--font-display); font-size: 13px; font-weight: 700; color: var(--teal); letter-spacing: 0.02em; }
  .hash {
    padding: 18px; border-radius: var(--radius-sm);
    background: #06080b; border: 1px solid var(--line);
  }
  /* Bascule brève à chaque nouveau bloc réel (~2 min) — pas d'animation continue. */
  .hash.fresh { animation: hashin 0.5s ease-out; }
  .reduce .hash.fresh { animation: none; }
  @keyframes hashin { from { border-color: rgba(20,200,184,0.55); background: rgba(20,200,184,0.05); } to { border-color: var(--line); background: #06080b; } }
  .hash-now {
    display: flex; flex-wrap: wrap; gap: 6px 10px;
    font-size: 18px; line-height: 1.5; word-break: break-all;
  }
  .hg { color: #98a0b0; }
  .hg.hot { color: var(--teal); }
  .core-meta { display: flex; align-items: baseline; justify-content: space-between; flex-wrap: wrap; gap: 6px 16px; margin-top: 12px; }
  .anchor { font-size: 12px; color: var(--dim); }
  .crypt { display: inline-flex; gap: 18px; flex-wrap: wrap; font-size: 11.5px; color: #8791a3; }
  .crypt b { color: var(--teal); font-variant-numeric: tabular-nums; }

  /* ── Pipeline vivant ── */
  .pipe { display: flex; align-items: center; flex-wrap: wrap; gap: 6px 4px; padding: 4px 22px 16px; }
  .pstep { display: inline-flex; align-items: center; gap: 7px; font-family: var(--font-display); font-size: 12px; color: var(--txt); background: var(--panel); border: 1px solid var(--line); border-radius: 999px; padding: 6px 12px; transition: border-color 0.2s ease, background 0.2s ease, color 0.2s ease; }
  .pstep.on { border-color: rgba(20,200,184,0.5); background: var(--tealdim); color: #dff6f3; }
  .pstep.on .pi { background: var(--teal); color: #06110f; }
  .pi { width: 16px; height: 16px; border-radius: 50%; background: var(--tealdim); color: var(--teal); font-size: 10px; font-weight: 700; display: inline-flex; align-items: center; justify-content: center; transition: background 0.2s ease, color 0.2s ease; }
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
    margin: 10px 14px 0; padding: 8px 14px;
    font-family: var(--font-display); font-size: 12px; line-height: 1.55;
    color: #9aa3b2; background: var(--panel);
    border: 1px solid var(--line); border-radius: var(--radius-sm);
  }

  /* ── Filtres (affichage seul — le buffer garde tout) ── */
  .chips { display: flex; flex-wrap: wrap; gap: 6px; padding: 10px 14px 0; }
  .chip {
    font-family: var(--font-display); font-size: 11px; font-weight: 600; letter-spacing: 0.02em;
    color: var(--dim); background: var(--panel); border: 1px solid var(--line); border-radius: 999px;
    padding: 5px 12px; cursor: pointer; transition: color 0.15s ease, border-color 0.15s ease, background 0.15s ease;
  }
  .chip:hover { color: var(--txt); }
  .chip.active { color: #06110f; background: var(--teal); border-color: var(--teal); }
  .chip:focus-visible { outline: 2px solid var(--teal); outline-offset: 2px; }

  /* ── Log — scrollable : les 200 lignes du buffer sont consultables ── */
  .log { padding: 10px 14px; height: max(340px, 46vh); overflow-y: auto; overscroll-behavior: contain; }
  .log::-webkit-scrollbar { width: 8px; }
  .log::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.14); border-radius: 4px; }
  .ln { display: flex; align-items: baseline; gap: 10px; padding: 4px 8px; border-radius: 6px; font-size: 12px; line-height: 1.4; animation: lnin 0.24s ease-out; cursor: pointer; }
  .ln:hover { background: rgba(255,255,255,0.035); }
  .ln:focus-visible { outline: 2px solid var(--teal); outline-offset: -1px; }
  .reduce .ln { animation: none; }
  @keyframes lnin { from { opacity: 0; transform: translateY(-4px); } to { opacity: 1; transform: none; } }
  /* Flash bref de confirmation de copie (presse-papiers). */
  .ln.flash { animation: lnflash 0.42s ease; }
  .reduce .ln.flash { animation: lnflash-reduce 0.6s ease; }
  @keyframes lnflash { 0% { background: rgba(20,200,184,0.4); } 100% { background: transparent; } }
  @keyframes lnflash-reduce { 0% { background: rgba(20,200,184,0.28); } 100% { background: transparent; } }
  .lt { color: #495060; opacity: 0.62; flex-shrink: 0; font-variant-numeric: tabular-nums; }
  .lg { color: var(--dim); width: 12px; text-align: center; flex-shrink: 0; }
  .lx { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .ln-reward .lg, .ln-vote .lg { color: var(--teal); } .ln-reward .lx { color: #dff6f3; }
  /* Hiérarchie : les lignes de scellement scandent la chaîne bloc par bloc. */
  .ln-sealMine, .ln-seal { border-top: 2px solid rgba(20,200,184,0.4); margin-top: 7px; padding-top: 9px; }
  .ln-sealMine { background: rgba(20,200,184,0.09); border-left: 1px solid rgba(20,200,184,0.22); border-right: 1px solid rgba(20,200,184,0.22); border-bottom: 1px solid rgba(20,200,184,0.22); }
  .ln-sealMine .lg { color: var(--teal); } .ln-sealMine .lx { color: #d6f5f1; font-weight: 600; }
  .ln-seal .lg { color: var(--teal); } .ln-seal .lx { color: #c4cbd8; font-weight: 500; }
  .ln-final .lg { color: var(--teal); } .ln-final .lx { color: #cdeee9; }
  .ln-verify .lg { color: var(--teal); } .ln-verify .lx { color: #9aa3b2; }
  .ln-env .lg { color: #7b849a; } .ln-env .lx { color: #a9b1c0; }
  .ln-voteCast .lg { color: var(--teal); } .ln-voteCast .lx { color: #dff6f3; font-weight: 600; }
  .ln-boot .lx, .ln-boot .lg { color: var(--dim); }
  .ln-sign .lg { color: var(--teal); } .ln-sign .lx { color: #b8c2d4; }
  .ln-persist .lg, .ln-persist .lx { color: #8791a3; }
  .ln-electLead { background: rgba(20,200,184,0.06); border: 1px solid rgba(20,200,184,0.14); }
  .ln-electLead .lg { color: var(--teal); } .ln-electLead .lx { color: #cdeee9; }
  .ln-elect .lg { color: #7b849a; } .ln-elect .lx { color: #9aa3b2; }
  /* Forensique par-scellement (quanta.lastSeal) : discret, en retrait, italique. */
  .ln-forensic { opacity: 0.72; }
  .ln-forensic .lg, .ln-forensic .lx { color: #616a7d; font-style: italic; font-size: 11px; }
  .ln-stall .lg { color: #f0b429; } .ln-stall .lx { color: #ffd97a; font-weight: 600; }

  .note { padding: 14px 22px 18px; font-size: 11.5px; color: var(--dim); line-height: 1.55; border-top: 1px solid var(--line); font-family: var(--font-display); max-width: 78ch; }
</style>
