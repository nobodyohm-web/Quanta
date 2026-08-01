<script lang="ts">
  // ═══════════════════════════════════════════════════════════════════
  //  ForgeEngine v5 — un VRAI terminal de nœud.
  //  La SEULE surface sombre de l'app (un moteur EST sombre).
  //  Comportement de terminal authentique : le flux s'écrit EN BAS et
  //  reste collé au bas (sticky-bottom ; si tu remontes lire, un badge
  //  « ↓ N » te ramène) ; un prompt `quanta>` exécute de vraies
  //  commandes contre le nœud (status, peers, balance, supply, block,
  //  epoch, filter…) ; le texte se sélectionne à la souris. Chaque
  //  ligne est un fait du nœud : événements Tauri, timings ML-DSA
  //  mesurés côté Rust, verdicts d'élection PoS. Aucune animation de
  //  remplissage, aucun calcul décoratif.
  //
  //  Piège réparé (v4 → v5) : les lignes s'inséraient EN HAUT d'une
  //  zone scrollable — le scroll anchoring du navigateur stabilisait la
  //  vue et le journal semblait FIGÉ dès qu'on avait scrollé une fois.
  //  Un terminal append en bas ; le problème disparaît par construction.
  // ═══════════════════════════════════════════════════════════════════
  import { tick as domTick, untrack } from "svelte";
  import {
    getNodeStatus, getFinalityStatus, getChainHistory, getChainOverview,
    getWalletOverview, getPeerMetrics,
  } from "./api";
  import { getVersion } from "@tauri-apps/api/app";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { locale } from "./i18n.svelte";
  import { lastStall } from "./diag";
  import { fmtQ, MICRO, TICKER } from "./quanta";

  const reduce =
    typeof window !== "undefined" && !!window.matchMedia &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ── i18n local (6 langues concises) ──────────────────────────────
  const L: Record<string, Record<string, string>> = {
    en: { title: "Consensus engine", live: "live",
      uS: "s", uMin: "min", uH: "h", ago: "{t} ago",
      sHeight: "height", sEpoch: "epoch", sFloor: "final", sVals: "val", sStake: "staked", sPeers: "peers", sFilter: "filter",
      fAll: "all", fBlocks: "blocks", fCrypto: "crypto", fNetwork: "network", fAlerts: "alerts",
      youVal: "you are a validator — you can be elected to seal blocks",
      becomeVal: "stake ≥ 1 QUANTA to become a validator and seal blocks",
      motd: "Proof-of-Stake — leaders elected by on-chain stake; blocks sealed under ML-DSA-65; irreversible past the ⅔-certificate floor. Type help.",
      boot: "node online",
      solo: "0 peers — only your own node's work appears here",
      unseen: "{n} new lines",
      cmdUnknown: "{c}: unknown command — type help",
      helpTitle: "commands:",
      hHelp: "this list", hStatus: "node & consensus state", hPeers: "connected peers (RTT, quality)",
      hBalance: "your on-chain balance", hSupply: "emitted supply vs hard cap", hBlock: "a block (default: latest)",
      hEpoch: "finality epoch progress", hFilter: "filter the stream", hClear: "clear the stream", hVersion: "app version",
      balLine: "spendable {s} · staked {k} · unbonding {u} · earned {e} ({t})",
      supLine: "minted {m} · burned {b} · cap {c} ({t}) · {p}% emitted",
      epochLine: "epoch {e} · block {i}/{l} · finalized floor #{f}",
      blockLine: "block #{n} · {t} tx · {h}", blockNone: "block not found (recent window only)",
      peersNone: "0 peers connected",
      eReward: "reward minted  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "BLOCK SEALED #{n} · {t} tx · {h} ← {p} · {d} µs",
      eSeal: "block #{n} sealed · {t} tx · {h} ← {p}", eVerify: "block #{n} verified — PoS proposer ✓ · coverage ✓ · Merkle ✓", eState: "chain #{n} · epoch {e} · {v} validators", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "finality vote · epoch {n}", eEnv: "{m} from {s} · nonce {n} — ML-DSA ✓ {us} µs", eVoteCast: "OUR finality vote signed — epoch {e} → #{h}",
      eFinal: "block #{n} finalized · irreversible", ePeer: "peer connected · {n} peers",
      eSign: "ML-DSA-65 signed · {m} envelope · {b} B · {us} µs", ePersist: "disk snapshot · {k} states · {b} KB · {ms} ms",
      eElectLead: "slot #{s} — ELECTED leader ({v} validators) — sealing", eElectFall: "slot #{s} — fallback proposer — sealing",
      eElectObs: "slot #{s} — another validator leads ({v}) — observing", eElectBoot: "slot #{s} — permissionless bootstrap (no stake yet)",
      eStall: "UI thread stalled {ms} ms" },
    fr: { title: "Moteur de consensus", live: "en direct",
      uS: "s", uMin: "min", uH: "h", ago: "il y a {t}",
      sHeight: "hauteur", sEpoch: "époque", sFloor: "final", sVals: "val", sStake: "staké", sPeers: "pairs", sFilter: "filtre",
      fAll: "tout", fBlocks: "blocs", fCrypto: "crypto", fNetwork: "réseau", fAlerts: "alertes",
      youVal: "tu es validateur — tu peux être élu pour sceller des blocs",
      becomeVal: "stake ≥ 1 QUANTA pour devenir validateur et sceller des blocs",
      motd: "Proof-of-Stake — leader élu par l'enjeu on-chain ; blocs scellés sous ML-DSA-65 ; irréversible sous le plancher du certificat ⅔. Tape help.",
      boot: "nœud en ligne",
      solo: "0 pair — seul le travail de ton propre nœud apparaît ici",
      unseen: "{n} nouvelles lignes",
      cmdUnknown: "{c} : commande inconnue — tape help",
      helpTitle: "commandes :",
      hHelp: "cette liste", hStatus: "état du nœud & du consensus", hPeers: "pairs connectés (RTT, qualité)",
      hBalance: "ton solde on-chain", hSupply: "offre émise vs plafond dur", hBlock: "un bloc (défaut : dernier)",
      hEpoch: "progression de l'époque de finalité", hFilter: "filtrer le flux", hClear: "vider le flux", hVersion: "version de l'app",
      balLine: "dépensable {s} · staké {k} · déverrouillage {u} · gagné {e} ({t})",
      supLine: "émis {m} · brûlé {b} · plafond {c} ({t}) · {p} % émis",
      epochLine: "époque {e} · bloc {i}/{l} · plancher finalisé #{f}",
      blockLine: "bloc #{n} · {t} tx · {h}", blockNone: "bloc introuvable (fenêtre récente seulement)",
      peersNone: "0 pair connecté",
      eReward: "récompense minée  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "BLOC SCELLÉ #{n} · {t} tx · {h} ← {p} · {d} µs",
      eSeal: "bloc #{n} scellé · {t} tx · {h} ← {p}", eVerify: "bloc #{n} vérifié — proposeur PoS ✓ · couverture ✓ · Merkle ✓", eState: "chaîne #{n} · époque {e} · {v} validateurs", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "vote de finalité · époque {n}", eEnv: "{m} de {s} · nonce {n} — ML-DSA ✓ {us} µs", eVoteCast: "NOTRE vote de finalité signé — époque {e} → #{h}",
      eFinal: "bloc #{n} finalisé · irréversible", ePeer: "pair connecté · {n} pairs",
      eSign: "ML-DSA-65 signée · enveloppe {m} · {b} o · {us} µs", ePersist: "snapshot disque · {k} états · {b} Ko · {ms} ms",
      eElectLead: "slot #{s} — ÉLU leader ({v} validateurs) — scellement", eElectFall: "slot #{s} — proposeur fallback — scellement",
      eElectObs: "slot #{s} — un autre validateur mène ({v}) — on observe", eElectBoot: "slot #{s} — bootstrap permissionless (personne n'a staké)",
      eStall: "fil UI bloqué {ms} ms" },
    es: { title: "Motor de consenso", live: "en vivo",
      uS: "s", uMin: "min", uH: "h", ago: "hace {t}",
      sHeight: "altura", sEpoch: "época", sFloor: "final", sVals: "val", sStake: "stake", sPeers: "pares", sFilter: "filtro",
      fAll: "todo", fBlocks: "bloques", fCrypto: "cripto", fNetwork: "red", fAlerts: "alertas",
      youVal: "eres validador — puedes ser elegido para sellar bloques",
      becomeVal: "haz stake ≥ 1 QUANTA para ser validador y sellar bloques",
      motd: "Proof-of-Stake — líder elegido por el stake on-chain; bloques sellados con ML-DSA-65; irreversible bajo el piso del certificado ⅔. Escribe help.",
      boot: "nodo en línea",
      solo: "0 pares — aquí solo aparece el trabajo de tu propio nodo",
      unseen: "{n} líneas nuevas",
      cmdUnknown: "{c}: comando desconocido — escribe help",
      helpTitle: "comandos:",
      hHelp: "esta lista", hStatus: "estado del nodo y consenso", hPeers: "pares conectados (RTT, calidad)",
      hBalance: "tu saldo on-chain", hSupply: "oferta emitida vs tope duro", hBlock: "un bloque (por defecto: último)",
      hEpoch: "progreso de la época de finalidad", hFilter: "filtrar el flujo", hClear: "vaciar el flujo", hVersion: "versión de la app",
      balLine: "disponible {s} · staked {k} · desbloqueo {u} · ganado {e} ({t})",
      supLine: "emitido {m} · quemado {b} · tope {c} ({t}) · {p}% emitido",
      epochLine: "época {e} · bloque {i}/{l} · piso finalizado #{f}",
      blockLine: "bloque #{n} · {t} tx · {h}", blockNone: "bloque no encontrado (solo ventana reciente)",
      peersNone: "0 pares conectados",
      eReward: "recompensa minada  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "BLOQUE SELLADO #{n} · {t} tx · {h} ← {p} · {d} µs",
      eSeal: "bloque #{n} sellado · {t} tx · {h} ← {p}", eVerify: "bloque #{n} verificado — proponente PoS ✓ · cobertura ✓ · Merkle ✓", eState: "cadena #{n} · época {e} · {v} validadores", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "voto de finalidad · época {n}", eEnv: "{m} de {s} · nonce {n} — ML-DSA ✓ {us} µs", eVoteCast: "NUESTRO voto de finalidad firmado — época {e} → #{h}",
      eFinal: "bloque #{n} finalizado · irreversible", ePeer: "par conectado · {n} pares",
      eSign: "ML-DSA-65 firmada · sobre {m} · {b} B · {us} µs", ePersist: "snapshot a disco · {k} estados · {b} KB · {ms} ms",
      eElectLead: "slot #{s} — líder ELEGIDO ({v} validadores) — sellando", eElectFall: "slot #{s} — proponente fallback — sellando",
      eElectObs: "slot #{s} — lidera otro validador ({v}) — observando", eElectBoot: "slot #{s} — bootstrap permissionless (nadie ha stakeado)",
      eStall: "hilo UI bloqueado {ms} ms" },
    ru: { title: "Движок консенсуса", live: "в эфире",
      uS: "с", uMin: "мин", uH: "ч", ago: "{t} назад",
      sHeight: "высота", sEpoch: "эпоха", sFloor: "финал", sVals: "вал", sStake: "стейк", sPeers: "пиры", sFilter: "фильтр",
      fAll: "все", fBlocks: "блоки", fCrypto: "крипто", fNetwork: "сеть", fAlerts: "оповещения",
      youVal: "вы валидатор — вас могут выбрать запечатывать блоки",
      becomeVal: "застейкайте ≥ 1 QUANTA, чтобы стать валидатором",
      motd: "Proof-of-Stake — лидера выбирает ончейн-стейк; блоки запечатаны ML-DSA-65; необратимо ниже пола сертификата ⅔. Введите help.",
      boot: "узел в сети",
      solo: "0 пиров — здесь видна только работа вашего узла",
      unseen: "{n} новых строк",
      cmdUnknown: "{c}: неизвестная команда — введите help",
      helpTitle: "команды:",
      hHelp: "этот список", hStatus: "состояние узла и консенсуса", hPeers: "подключённые пиры (RTT, качество)",
      hBalance: "ваш ончейн-баланс", hSupply: "эмиссия против жёсткого потолка", hBlock: "блок (по умолчанию: последний)",
      hEpoch: "прогресс эпохи финальности", hFilter: "фильтр потока", hClear: "очистить поток", hVersion: "версия приложения",
      balLine: "доступно {s} · стейк {k} · разблокировка {u} · заработано {e} ({t})",
      supLine: "эмитировано {m} · сожжено {b} · потолок {c} ({t}) · {p}% эмиссии",
      epochLine: "эпоха {e} · блок {i}/{l} · финализированный пол #{f}",
      blockLine: "блок #{n} · {t} tx · {h}", blockNone: "блок не найден (только недавнее окно)",
      peersNone: "0 подключённых пиров",
      eReward: "награда добыта  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "БЛОК ЗАПЕЧАТАН #{n} · {t} tx · {h} ← {p} · {d} мкс",
      eSeal: "блок #{n} запечатан · {t} tx · {h} ← {p}", eVerify: "блок #{n} проверен — PoS-предлагатель ✓ · покрытие ✓ · Merkle ✓", eState: "цепь #{n} · эпоха {e} · {v} валидаторов", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "голос финальности · эпоха {n}", eEnv: "{m} от {s} · nonce {n} — ML-DSA ✓ {us} мкс", eVoteCast: "НАШ голос финальности подписан — эпоха {e} → #{h}",
      eFinal: "блок #{n} финализирован · необратимо", ePeer: "пир подключён · {n} пиров",
      eSign: "ML-DSA-65 подписан · конверт {m} · {b} Б · {us} мкс", ePersist: "снапшот на диск · {k} состояний · {b} КБ · {ms} мс",
      eElectLead: "слот #{s} — ИЗБРАН лидером ({v} валидаторов) — запечатываем", eElectFall: "слот #{s} — резервный предлагатель — запечатываем",
      eElectObs: "слот #{s} — лидирует другой валидатор ({v}) — наблюдаем", eElectBoot: "слот #{s} — permissionless-бутстрап (никто не застейкал)",
      eStall: "поток UI завис на {ms} мс" },
    zh: { title: "共识引擎", live: "实时",
      uS: "秒", uMin: "分钟", uH: "小时", ago: "{t}前",
      sHeight: "高度", sEpoch: "纪元", sFloor: "已确定", sVals: "验证者", sStake: "质押", sPeers: "节点", sFilter: "筛选",
      fAll: "全部", fBlocks: "区块", fCrypto: "加密", fNetwork: "网络", fAlerts: "警报",
      youVal: "你是验证者——可被选为出块者封存区块",
      becomeVal: "质押 ≥ 1 QUANTA 即可成为验证者并封存区块",
      motd: "权益证明——出块者由链上质押选出；区块以 ML-DSA-65 封存；低于 ⅔ 证书底线即不可逆。输入 help。",
      boot: "节点在线",
      solo: "0 个对等节点——这里只显示你自己节点的工作",
      unseen: "{n} 条新行",
      cmdUnknown: "{c}：未知命令——输入 help",
      helpTitle: "命令：",
      hHelp: "本列表", hStatus: "节点与共识状态", hPeers: "已连接节点（RTT、质量）",
      hBalance: "你的链上余额", hSupply: "已发行量与硬顶", hBlock: "某个区块（默认：最新）",
      hEpoch: "最终性纪元进度", hFilter: "筛选流", hClear: "清空流", hVersion: "应用版本",
      balLine: "可用 {s} · 质押 {k} · 解锁中 {u} · 已赚取 {e}（{t}）",
      supLine: "已发行 {m} · 已销毁 {b} · 硬顶 {c}（{t}）· 已发行 {p}%",
      epochLine: "纪元 {e} · 区块 {i}/{l} · 已确定底线 #{f}",
      blockLine: "区块 #{n} · {t} 笔 · {h}", blockNone: "未找到区块（仅近期窗口）",
      peersNone: "0 个已连接节点",
      eReward: "已获挖矿奖励  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "区块已封存 #{n} · {t} 笔 · {h} ← {p} · {d} µs",
      eSeal: "区块 #{n} 已封存 · {t} 笔 · {h} ← {p}", eVerify: "区块 #{n} 已验证 — PoS 出块者 ✓ · 覆盖 ✓ · Merkle ✓", eState: "链 #{n} · 纪元 {e} · {v} 个验证者", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "最终性投票 · 纪元 {n}", eEnv: "{m} 来自 {s} · nonce {n} — ML-DSA ✓ {us} µs", eVoteCast: "我们的最终性投票已签名 — 纪元 {e} → #{h}",
      eFinal: "区块 #{n} 已最终确定 · 不可逆", ePeer: "节点已连接 · {n} 个节点",
      eSign: "ML-DSA-65 已签名 · {m} 信封 · {b} 字节 · {us} µs", ePersist: "磁盘快照 · {k} 个状态 · {b} KB · {ms} ms",
      eElectLead: "时隙 #{s} — 当选出块者（{v} 个验证者）— 封存中", eElectFall: "时隙 #{s} — 后备提议者 — 封存中",
      eElectObs: "时隙 #{s} — 由其他验证者出块（{v}）— 观察中", eElectBoot: "时隙 #{s} — 无许可引导（尚无质押）",
      eStall: "界面线程卡顿 {ms} ms" },
    ja: { title: "コンセンサスエンジン", live: "ライブ",
      uS: "秒", uMin: "分", uH: "時間", ago: "{t}前",
      sHeight: "高さ", sEpoch: "エポック", sFloor: "確定", sVals: "検証者", sStake: "ステーク", sPeers: "ピア", sFilter: "フィルタ",
      fAll: "すべて", fBlocks: "ブロック", fCrypto: "暗号", fNetwork: "ネットワーク", fAlerts: "アラート",
      youVal: "あなたは検証者です — 選ばれてブロックを封印できます",
      becomeVal: "1 QUANTA 以上ステークすると検証者になれます",
      motd: "プルーフ・オブ・ステーク — リーダーはオンチェーンのステークで選出。ブロックは ML-DSA-65 で封印、⅔ 証明書の床より下は不可逆。help と入力。",
      boot: "ノードはオンライン",
      solo: "ピア 0 — ここには自分のノードの仕事だけが表示されます",
      unseen: "新しい行 {n} 件",
      cmdUnknown: "{c}：不明なコマンド — help と入力",
      helpTitle: "コマンド：",
      hHelp: "この一覧", hStatus: "ノードとコンセンサスの状態", hPeers: "接続中のピア（RTT・品質）",
      hBalance: "オンチェーン残高", hSupply: "発行量とハードキャップ", hBlock: "ブロック（既定：最新）",
      hEpoch: "ファイナリティのエポック進行", hFilter: "ストリームを絞り込む", hClear: "ストリームを消去", hVersion: "アプリのバージョン",
      balLine: "利用可能 {s} · ステーク {k} · 解除中 {u} · 獲得 {e}（{t}）",
      supLine: "発行済 {m} · 焼却 {b} · 上限 {c}（{t}）· {p}% 発行済",
      epochLine: "エポック {e} · ブロック {i}/{l} · 確定床 #{f}",
      blockLine: "ブロック #{n} · {t} tx · {h}", blockNone: "ブロックが見つかりません（直近のみ）",
      peersNone: "接続中のピア 0",
      eReward: "報酬を採掘  +{u} µQTA ({a} QTA) · tx {h}", eSealMine: "ブロック封印 #{n} · {t} tx · {h} ← {p} · {d} µs",
      eSeal: "ブロック #{n} 封印 · {t} tx · {h} ← {p}", eVerify: "ブロック #{n} 検証済 — PoS 提案者 ✓ · カバレッジ ✓ · Merkle ✓", eState: "チェーン #{n} · エポック {e} · {v} 検証者", eTx: "tx {k} · +{u} µQTA · nonce {o} · {h}", eVote: "ファイナリティ投票 · エポック {n}", eEnv: "{m} ({s}) · nonce {n} — ML-DSA ✓ {us} µs", eVoteCast: "私たちのファイナリティ投票に署名 — エポック {e} → #{h}",
      eFinal: "ブロック #{n} 確定 · 不可逆", ePeer: "ピア接続 · {n} ピア",
      eSign: "ML-DSA-65 署名 · {m} エンベロープ · {b} B · {us} µs", ePersist: "ディスクスナップショット · {k} 状態 · {b} KB · {ms} ms",
      eElectLead: "スロット #{s} — リーダーに当選（{v} 検証者）— 封印", eElectFall: "スロット #{s} — フォールバック提案者 — 封印",
      eElectObs: "スロット #{s} — 別の検証者がリード（{v}）— 観測中", eElectBoot: "スロット #{s} — パーミッションレス・ブートストラップ（ステークなし）",
      eStall: "UI スレッド {ms} ms 停止" },
  };
  function tl(k: string): string { const l = locale(); return L[l]?.[k] ?? L.en[k] ?? k; }
  function fill(tpl: string, v: Record<string, string | number>): string {
    let o = tpl; for (const [k, val] of Object.entries(v)) o = o.replace(`{${k}}`, String(val)); return o;
  }
  const hshort = (x?: string) => (x ? x.replace(/^0x/, "").slice(0, 12) + "…" : "—");
  const nf = (n: number) => n.toLocaleString("fr-FR");

  // ── État consensus réel (sondé — nourrit la status bar) ──────────
  let height = $state(0);
  let epoch = $state(0);
  let epochLen = $state(32);
  let intoEpoch = $state(0);
  let floor = $state(0);
  let validators = $state(0);
  let totalStaked = $state(0);
  let peers = $state(0);
  let iAmValidator = $state(false);
  let lastBlockHash = $state("");
  let appVersion = $state("");

  // « scellé il y a X » — heure d'arrivée locale du dernier scellement.
  let lastSealAt = $state(0);
  let nowTick = $state(Date.now());
  function fmtAgo(ms: number): string {
    const s = Math.max(0, Math.round(ms / 1000));
    if (s < 90) return `${s} ${tl("uS")}`;
    const m = Math.round(s / 60);
    if (m < 90) return `${m} ${tl("uMin")}`;
    return `${Math.round(m / 60)} ${tl("uH")}`;
  }

  // ── Le flux (append EN BAS, sticky-bottom) ───────────────────────
  interface Line { id: number; kind: string; time: string; text: string; }
  let lines = $state<Line[]>([]);
  let seq = 0;
  let logEl = $state<HTMLElement | undefined>();
  let unseen = $state(0);
  const GLUE_PX = 48; // à moins de 48 px du bas = collé → on suit

  function stamp(): string {
    const d = new Date(); const p = (n: number) => n.toString().padStart(2, "0");
    return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
  }
  function push(kind: string, text: string) {
    // untrack : appelé depuis des $effect — sans lui, le spread `...lines`
    // serait une LECTURE trackée de l'état qu'on écrit → boucle réactive
    // infinie (effect_update_depth_exceeded ; le bug historique des gels).
    const prev = untrack(() => lines);
    lines = [...prev, { id: ++seq, kind, time: stamp(), text }].slice(-400);
    const el = logEl;
    if (!el) return;
    const glued = el.scrollHeight - el.scrollTop - el.clientHeight < GLUE_PX;
    if (glued) {
      void domTick().then(() => { const e = logEl; if (e) e.scrollTop = e.scrollHeight; });
    } else if (kind !== "echo" && kind !== "out") {
      unseen += 1;
    }
  }
  function glueToBottom() {
    const el = logEl;
    if (el) el.scrollTop = el.scrollHeight;
    unseen = 0;
  }
  function onLogScroll() {
    const el = logEl;
    if (el && el.scrollHeight - el.scrollTop - el.clientHeight < GLUE_PX) unseen = 0;
  }

  // ── Filtre (commande `filter`) — echo/sorties/alertes toujours visibles ──
  type FilterKey = "all" | "blocks" | "crypto" | "network" | "alerts";
  let activeFilter = $state<FilterKey>("all");
  function kindCategory(kind: string): FilterKey {
    if (kind === "seal" || kind === "sealMine" || kind === "verify" || kind === "final" || kind === "forensic") return "blocks";
    if (kind === "sign" || kind === "reward") return "crypto";
    if (kind === "stall") return "alerts";
    return "network";
  }
  const filteredLines = $derived(
    activeFilter === "all"
      ? lines
      : lines.filter((l) =>
          l.kind === "echo" || l.kind === "out" || l.kind === "stall" ||
          kindCategory(l.kind) === activeFilter),
  );

  // Préfixe texte par catégorie — la hiérarchie d'un vrai terminal.
  function pfx(kind: string): string {
    if (kind === "seal" || kind === "sealMine") return "[seal]";
    if (kind === "verify") return "[ok]";
    if (kind === "final" || kind === "vote" || kind === "voteCast") return "[fin]";
    if (kind === "sign") return "[sig]";
    if (kind === "reward") return "[mint]";
    if (kind === "tx") return "[tx]";
    if (kind === "elect" || kind === "electLead") return "[pos]";
    if (kind === "stall") return "[!]";
    if (kind === "echo") return ">";
    if (kind === "forensic" || kind === "out" || kind === "boot") return "[·]";
    return "[net]"; // env, peer, persist…
  }

  // ── Prompt : de vraies commandes contre le nœud ──────────────────
  let cmd = $state("");
  let hist: string[] = [];
  let histIdx = -1;
  const COMMANDS = ["help", "status", "peers", "balance", "supply", "block", "epoch", "filter", "clear", "version"];

  async function runCmd(raw: string) {
    const parts = raw.trim().split(/\s+/);
    const name = (parts[0] ?? "").toLowerCase();
    if (!name) return;
    push("echo", raw.trim());
    try {
      switch (name) {
        case "help": {
          push("out", tl("helpTitle"));
          const desc: Record<string, string> = {
            help: tl("hHelp"), status: tl("hStatus"), peers: tl("hPeers"), balance: tl("hBalance"),
            supply: tl("hSupply"), block: tl("hBlock") + " — block [n]", epoch: tl("hEpoch"),
            filter: tl("hFilter") + ` — filter ${tl("fAll")}|${tl("fBlocks")}|${tl("fCrypto")}|${tl("fNetwork")}|${tl("fAlerts")}`,
            clear: tl("hClear"), version: tl("hVersion"),
          };
          for (const c of COMMANDS) push("out", `  ${c.padEnd(9)} ${desc[c]}`);
          break;
        }
        case "status": {
          push("out", fill(tl("eState"), { n: nf(height), e: epoch, v: validators }));
          push("out", fill(tl("epochLine"), { e: epoch, i: intoEpoch, l: epochLen, f: nf(floor) }));
          push("out", iAmValidator ? tl("youVal") : tl("becomeVal"));
          break;
        }
        case "peers": {
          const ps = await getPeerMetrics();
          if (!ps.length) { push("out", tl("peersNone")); break; }
          for (const p of ps.slice(0, 12)) {
            const rtt = p.smoothed_rtt_ms != null ? `${Math.round(p.smoothed_rtt_ms)} ms` : "—";
            const q = p.quality_score != null ? `${Math.round(p.quality_score)}/100` : "—";
            push("out", `  ${(p.display_name ?? p.public_key.slice(0, 12) + "…").padEnd(18)} rtt ${rtt} · ${q}`);
          }
          break;
        }
        case "balance": {
          const w = await getWalletOverview();
          push("out", fill(tl("balLine"), {
            s: fmtQ(w.spendable / MICRO), k: fmtQ(w.staked / MICRO),
            u: fmtQ(w.unbonding / MICRO), e: fmtQ(w.earned / MICRO), t: TICKER,
          }));
          break;
        }
        case "supply": {
          const c = await getChainOverview(1);
          push("out", fill(tl("supLine"), {
            m: fmtQ(c.total_mined_qta), b: fmtQ(c.total_burned_qta), c: nf(c.max_supply_qta),
            t: TICKER, p: c.pct_to_cap.toFixed(4),
          }));
          break;
        }
        case "block": {
          const c = await getChainOverview(50);
          const n = parts[1] != null ? parseInt(parts[1], 10) : NaN;
          const b = Number.isNaN(n) ? c.blocks[c.blocks.length - 1] : c.blocks.find((x) => x.index === n);
          if (!b) { push("out", tl("blockNone")); break; }
          push("out", fill(tl("blockLine"), { n: nf(b.index), t: b.tx_count, h: b.hash.slice(0, 32) + "…" }));
          break;
        }
        case "epoch": {
          const f = await getFinalityStatus();
          push("out", fill(tl("epochLine"), {
            e: f.epoch, i: f.blocks_into_epoch, l: f.epoch_length, f: nf(f.finalized_floor),
          }));
          break;
        }
        case "filter": {
          const arg = (parts[1] ?? "all").toLowerCase();
          const map: Record<string, FilterKey> = { all: "all", blocks: "blocks", crypto: "crypto", network: "network", alerts: "alerts" };
          activeFilter = map[arg] ?? "all";
          push("out", `${tl("sFilter")}: ${tl("f" + activeFilter[0].toUpperCase() + activeFilter.slice(1))}`);
          break;
        }
        case "clear": { lines = []; unseen = 0; break; }
        case "version": { push("out", `quanta v${appVersion || "?"}`); break; }
        default: push("out", fill(tl("cmdUnknown"), { c: name }));
      }
    } catch {
      push("stall", fill(tl("eStall"), { ms: 0 }));
    }
    void domTick().then(glueToBottom);
  }

  function onPromptKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      const c = cmd; cmd = "";
      if (c.trim()) { hist.push(c.trim()); histIdx = hist.length; void runCmd(c); }
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (histIdx > 0) { histIdx -= 1; cmd = hist[histIdx] ?? ""; }
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (histIdx < hist.length - 1) { histIdx += 1; cmd = hist[histIdx] ?? ""; }
      else { histIdx = hist.length; cmd = ""; }
    } else if (e.key === "Escape") { cmd = ""; }
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
        if (seenStats && f.finalized_floor > floor && f.finalized_floor > 0)
          push("final", fill(tl("eFinal"), { n: nf(f.finalized_floor) }));
        height = f.height ?? height; epoch = f.epoch ?? epoch; floor = f.finalized_floor ?? floor;
        epochLen = f.epoch_length ?? epochLen; intoEpoch = f.blocks_into_epoch ?? intoEpoch;
        validators = f.validators ?? validators; totalStaked = f.total_staked ?? totalStaked;
        iAmValidator = !!f.i_am_validator; seenStats = true;
        if (!statePushed) {
          statePushed = true;
          push("boot", fill(tl("eState"), { n: nf(f.height ?? 0), e: f.epoch ?? 0, v: f.validators ?? 0 }));
          if ((f.validators ?? 0) >= 0) push("boot", iAmValidator ? tl("youVal") : tl("becomeVal"));
          if (peers === 0) push("boot", tl("solo"));
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
    push("boot", tl("motd"));
    (async () => {
      const u1 = await listen<{ amount: number; amount_micro?: number; tx_hash?: string }>("quanta://mined", (e) => {
        const p = e.payload; const a = p?.amount ?? 0; if (a <= 0) return;
        const u = p?.amount_micro ?? Math.round(a * 1e6);
        push("reward", fill(tl("eReward"), { u: nf(u), a: a.toFixed(6), h: hshort(p?.tx_hash) }));
      });
      const u2 = await listen<{ index: number; txs: number; mine: boolean; hash?: string; prev?: string; seal_us?: number }>("quanta://block-sealed", (e) => {
        const p = e.payload; if (!p) return;
        if (p.hash) { lastBlockHash = p.hash; height = Math.max(height, p.index); }
        lastSealAt = Date.now();
        const vars = { n: p.index, t: p.txs ?? 0, h: hshort(p.hash), p: hshort(p.prev) };
        if (p.mine) push("sealMine", fill(tl("eSealMine"), { ...vars, d: nf(p.seal_us ?? 0) }));
        else {
          push("seal", fill(tl("eSeal"), vars));
          push("verify", fill(tl("eVerify"), { n: p.index }));
        }
      });
      const u3 = await listen<{ tx_type: string; amount_micro?: number; nonce?: number; hash?: string }>("quanta://tx-applied", (e) => {
        const p = e.payload; if (!p) return;
        push("tx", fill(tl("eTx"), { k: p.tx_type ?? "", u: nf(p.amount_micro ?? 0), o: p.nonce ?? 0, h: hshort(p.hash) }));
      });
      // Télémétrie du nœud (quanta://engine) : enveloppes authentifiées,
      // signatures sortantes, élections PoS, votes de finalité, snapshots.
      const u4 = await listen<any>("quanta://engine", (e) => {
        const p = e.payload; if (!p) return;
        if (p.kind === "envelope") {
          push("env", fill(tl("eEnv"), { m: p.msg ?? "?", s: (p.sender ?? "") + "…", n: p.nonce ?? 0, us: nf(p.us ?? 0) }));
        } else if (p.kind === "vote") {
          push("voteCast", fill(tl("eVoteCast"), { e: p.epoch ?? 0, h: p.hash ?? "" }));
        } else if (p.kind === "sign") {
          push("sign", fill(tl("eSign"), { m: p.msg ?? "?", b: p.bytes ?? 0, us: nf(p.us ?? 0) }));
        } else if (p.kind === "persist") {
          push("persist", fill(tl("ePersist"), { k: p.keys ?? 0, b: Math.max(1, Math.round((p.bytes ?? 0) / 1024)), ms: p.ms ?? 0 }));
        } else if (p.kind === "elect") {
          const key = p.verdict === "leader" ? "eElectLead"
            : p.verdict === "fallback" ? "eElectFall"
            : p.verdict === "bootstrap" ? "eElectBoot" : "eElectObs";
          const kind = p.verdict === "leader" || p.verdict === "fallback" ? "electLead" : "elect";
          push(kind, fill(tl(key), { s: nf(p.slot ?? 0), v: p.validators ?? 0 }));
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
    const tk = setInterval(() => { nowTick = Date.now(); }, 1000);
    // Watchdog du fil UI : un blocage > 900 ms devient une ligne mesurée.
    let lastBeat = performance.now();
    const wd = setInterval(() => {
      const nowB = performance.now();
      const gap = nowB - lastBeat;
      lastBeat = nowB;
      if (gap > 900) push("stall", fill(tl("eStall"), { ms: Math.round(gap) }));
    }, 250);
    // Sonde globale (diag.ts) : gels + forensique par scellement remontent ici.
    let seenStall = lastStall() ?? "";
    if (seenStall) push("stall", seenStall.slice(0, 220));
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
    // Démarrage collé au bas.
    void domTick().then(glueToBottom);
    return () => {
      clearInterval(iv);
      clearInterval(tk);
      clearInterval(wd);
      clearInterval(sd);
    };
  });

  const sealAgo = $derived(lastSealAt > 0 ? fill(tl("ago"), { t: fmtAgo(nowTick - lastSealAt) }) : "");
</script>

<div class="engine" class:reduce role="group" aria-label={tl("title")}>
  <div class="bar">
    <span class="bar-t">quanta · engine{appVersion ? ` · v${appVersion}` : ""} — {tl("title")}</span>
    <span class="bar-live"><span class="dot"></span>{tl("live")}</span>
  </div>

  <!-- ── Le flux — un vrai terminal : append en bas, collé au bas ── -->
  <div class="logwrap">
    <div class="log" role="log" bind:this={logEl} onscroll={onLogScroll}>
      {#each filteredLines as line (line.id)}
        <div class="ln ln-{line.kind}">
          <span class="lt">{line.time}</span>
          <span class="lg">{pfx(line.kind)}</span>
          <span class="lx">{line.text}</span>
        </div>
      {/each}
    </div>
    {#if unseen > 0}
      <button type="button" class="unseen" onclick={glueToBottom}>
        ↓ {fill(tl("unseen"), { n: unseen })}
      </button>
    {/if}
  </div>

  <!-- ── Prompt : de vraies commandes contre ton nœud ── -->
  <div class="promptrow">
    <span class="ps1">quanta&gt;</span>
    <input
      class="prompt"
      type="text"
      bind:value={cmd}
      onkeydown={onPromptKey}
      spellcheck="false"
      autocomplete="off"
      autocapitalize="off"
      aria-label="quanta>"
      placeholder="help"
    />
  </div>

  <!-- ── Status bar dense (l'état réel, une ligne) ── -->
  <div class="status">
    <span class="sb"><em>{tl("sHeight")}</em> {nf(height)}</span>
    {#if lastBlockHash}<span class="sb sb-hash">{lastBlockHash.replace(/^0x/, "").slice(0, 16)}…{#if sealAgo} · {sealAgo}{/if}</span>{/if}
    <span class="sb"><em>{tl("sEpoch")}</em> {epoch} ({intoEpoch}/{epochLen})</span>
    <span class="sb"><em>{tl("sFloor")}</em> #{nf(floor)}</span>
    <span class="sb"><em>{tl("sVals")}</em> {validators}</span>
    <span class="sb"><em>{tl("sStake")}</em> {fmtQ(totalStaked / MICRO)} {TICKER}</span>
    <span class="sb"><em>{tl("sPeers")}</em> {peers}</span>
    {#if activeFilter !== "all"}<span class="sb sb-filter"><em>{tl("sFilter")}</em> {tl("f" + activeFilter[0].toUpperCase() + activeFilter.slice(1))}</span>{/if}
  </div>
</div>

<style>
  .engine {
    --bg: #0a0c10; --line: rgba(255, 255, 255, 0.07);
    --txt: #cdd2db; --dim: #6a7080; --teal: #14c8b8;
    display: flex; flex-direction: column;
    background: var(--bg); border: 1px solid #1b1f28; border-radius: var(--radius-lg);
    overflow: hidden; box-shadow: var(--shadow); color: var(--txt);
    font-family: var(--font-mono);
  }
  .bar { display: flex; align-items: center; gap: 10px; padding: 10px 14px; border-bottom: 1px solid var(--line); background: #07090c; flex-shrink: 0; }
  .bar-t { font-size: 12px; color: var(--dim); letter-spacing: 0.02em; }
  .bar-live { margin-left: auto; display: inline-flex; align-items: center; gap: 7px; font-size: 11px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: var(--teal); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--teal); animation: pulse 2s ease infinite; }
  .reduce .dot { animation: none; }
  @keyframes pulse { 0%, 100% { box-shadow: 0 0 0 0 rgba(20, 200, 184, 0.45); } 50% { box-shadow: 0 0 0 5px rgba(20, 200, 184, 0); } }

  /* ── Le flux ── */
  .logwrap { position: relative; flex: 1; min-height: 0; }
  .log {
    height: max(420px, 56vh); overflow-y: auto; overscroll-behavior: contain;
    padding: 8px 12px 6px; user-select: text; -webkit-user-select: text;
  }
  .log::-webkit-scrollbar { width: 8px; }
  .log::-webkit-scrollbar-thumb { background: rgba(255, 255, 255, 0.14); border-radius: 4px; }
  .ln { display: flex; align-items: baseline; gap: 9px; padding: 1.5px 4px; font-size: 12px; line-height: 1.45; }
  .lt { color: #495060; flex-shrink: 0; font-variant-numeric: tabular-nums; }
  .lg { color: var(--dim); min-width: 44px; flex-shrink: 0; }
  .lx { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  /* Hiérarchie sobre par catégorie — pas de cartes, pas de bordures arrondies. */
  .ln-sealMine { background: rgba(20, 200, 184, 0.08); border-top: 1px solid rgba(20, 200, 184, 0.35); }
  .ln-sealMine .lg, .ln-sealMine .lx { color: #d6f5f1; font-weight: 600; }
  .ln-seal { border-top: 1px solid rgba(20, 200, 184, 0.22); }
  .ln-seal .lg { color: var(--teal); } .ln-seal .lx { color: #c4cbd8; }
  .ln-verify .lg { color: var(--teal); } .ln-verify .lx { color: #9aa3b2; }
  .ln-final .lg, .ln-final .lx { color: #cdeee9; }
  .ln-vote .lg, .ln-voteCast .lg { color: var(--teal); }
  .ln-voteCast .lx { color: #dff6f3; font-weight: 600; }
  .ln-reward .lg { color: var(--teal); } .ln-reward .lx { color: #dff6f3; }
  .ln-sign .lg { color: var(--teal); } .ln-sign .lx { color: #b8c2d4; }
  .ln-electLead .lg { color: var(--teal); } .ln-electLead .lx { color: #cdeee9; }
  .ln-elect .lg, .ln-elect .lx { color: #9aa3b2; }
  .ln-env .lx, .ln-persist .lx { color: #a9b1c0; }
  .ln-boot .lg, .ln-boot .lx { color: var(--dim); }
  .ln-forensic .lg, .ln-forensic .lx { color: #616a7d; font-style: italic; font-size: 11px; }
  .ln-stall .lg, .ln-stall .lx { color: #ffd97a; font-weight: 600; }
  .ln-echo .lg { color: var(--teal); font-weight: 700; }
  .ln-echo .lx { color: #eef2f7; font-weight: 600; }
  .ln-out .lx { color: #b8c2d4; white-space: pre-wrap; }

  .unseen {
    position: absolute; right: 14px; bottom: 10px;
    font-family: inherit; font-size: 11px; font-weight: 600;
    color: #06110f; background: var(--teal); border: none; border-radius: 3px;
    padding: 4px 10px; cursor: pointer;
  }
  .unseen:focus-visible { outline: 2px solid #dff6f3; outline-offset: 2px; }

  /* ── Prompt ── */
  .promptrow {
    display: flex; align-items: center; gap: 8px; padding: 8px 12px;
    border-top: 1px solid var(--line); background: #07090c; flex-shrink: 0;
  }
  .ps1 { color: var(--teal); font-size: 12px; font-weight: 700; }
  .prompt {
    flex: 1; background: transparent; border: none; outline: none;
    color: #eef2f7; font-family: inherit; font-size: 12px; caret-color: var(--teal);
  }
  .prompt::placeholder { color: #3c4250; }

  /* ── Status bar ── */
  .status {
    display: flex; flex-wrap: wrap; gap: 2px 14px; padding: 7px 12px;
    border-top: 1px solid var(--line); background: #06080b; flex-shrink: 0;
    font-size: 11px; font-variant-numeric: tabular-nums lining-nums;
  }
  .sb { color: #98a0b0; }
  .sb em { font-style: normal; color: #5b6272; text-transform: uppercase; letter-spacing: 0.05em; font-size: 10px; margin-right: 4px; }
  .sb-hash { color: var(--teal); }
  .sb-filter { color: #ffd97a; }
</style>
