<script lang="ts">
  // ═══════════════════════════════════════════════════════════════════════════
  //  NetworkScene3D — « le réseau vivant ». WebGL2 pur, zéro dépendance.
  //
  //  Structure PERMANENTE, 100 % données réelles :
  //    · TON nœud (le forge) au centre-avant — un cœur teal ;
  //    · les pairs mesurés (store peerMetrics) en anneau, halo ∝ quality_score,
  //      apparition/disparition douce ;
  //    · la chaîne récente (chainOverview.blocks) en hélice sobre, la frontière
  //      de finalité VISIBLE : bloc ≤ plancher = pierre teal pleine (irréversible),
  //      au-dessus = verre givré translucide ;
  //    · un bandeau discret de stats réelles (hauteur · pairs · scelleur · époque).
  //
  //  MOUVEMENT — équilibre imposé par le propriétaire :
  //    · un fond AMBIANT discret est autorisé (sa demande) : un champ de motes
  //      fines, lentes, teal/gris sur la carte claire — throttlé à ≤30 fps,
  //      PAUSE totale hors-viewport / onglet caché ;
  //    · les PICS spectaculaires sont pilotés par les ÉVÉNEMENTS RÉELS et chacun
  //      est étiqueté :
  //        block-sealed → jaillissement + le bloc naît au forge et rejoint la
  //          file + étiquette « @pseudo a scellé #N » (« toi » si mine) ;
  //        engine/envelope → filet de particules d'un pair vers le centre (gossip) ;
  //        engine/vote → onde teal (vote de finalité) ;
  //        engine/elect leader → halo sur TON nœud ;
  //        mined → pulse de récompense sur TON nœud.
  //    Pendant ~2 s d'un événement la boucle monte à 60 fps puis redescend à 30.
  //
  //  reduced-motion : ambiance FIGÉE (une frame statique), événements = simple
  //  apparition (aucune explosion, aucune boucle continue).
  //
  //  Hygiène anti-gel (leçons durement apprises) : AUCUNE dépendance ; toute
  //  boucle rAF sous try/catch ; IntersectionObserver lit es[es.length-1] ;
  //  perte de contexte → recréation via {#key glGen} ; pools pré-alloués (zéro
  //  allocation par frame) ; cleanup complet au démontage ; pause hors-vue.
  // ═══════════════════════════════════════════════════════════════════════════
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { locale } from "./i18n.svelte";
  import { note, alertDiag } from "./diag";

  let {
    peerCount = 0,
    peers = [],
    blocks = [],
    finalityFloor = 0,
    height = 0,
    epoch = 0,
  } = $props<{
    peerCount?: number;
    peers?: { public_key: string; display_name?: string | null; quality_score?: number | null }[];
    blocks?: { index: number; tx_count?: number }[];
    finalityFloor?: number;
    height?: number;
    epoch?: number;
  }>();

  // ─── i18n local ×6 (pattern ForgeEngine — aucun ajout dans i18n.generated) ──
  const L: Record<string, Record<string, string>> = {
    en: { title: "Live network", height: "height", peers: "peers", epoch: "epoch",
      sealer: "last seal", none: "—", you: "you", sealedBy: "{who} sealed #{n}",
      legFinal: "finalized", legPending: "pending", drag: "drag to orbit",
      solo: "solo · your node only" },
    fr: { title: "Réseau en direct", height: "hauteur", peers: "pairs", epoch: "époque",
      sealer: "dernier scellé", none: "—", you: "toi", sealedBy: "{who} a scellé #{n}",
      legFinal: "finalisé", legPending: "en attente", drag: "glisse pour orbiter",
      solo: "solo · ton nœud seul" },
    es: { title: "Red en vivo", height: "altura", peers: "pares", epoch: "época",
      sealer: "último sellado", none: "—", you: "tú", sealedBy: "{who} selló #{n}",
      legFinal: "finalizado", legPending: "pendiente", drag: "arrastra para orbitar",
      solo: "solo · solo tu nodo" },
    ru: { title: "Сеть в эфире", height: "высота", peers: "пиры", epoch: "эпоха",
      sealer: "посл. блок", none: "—", you: "вы", sealedBy: "{who} запечатал #{n}",
      legFinal: "финализирован", legPending: "в ожидании", drag: "тяните для вращения",
      solo: "соло · только ваш узел" },
    zh: { title: "实时网络", height: "高度", peers: "节点", epoch: "纪元",
      sealer: "最新封存", none: "—", you: "你", sealedBy: "{who} 封存了 #{n}",
      legFinal: "已确定", legPending: "待定", drag: "拖动以环视",
      solo: "单机 · 仅你的节点" },
    ja: { title: "ライブネットワーク", height: "高さ", peers: "ピア", epoch: "エポック",
      sealer: "最新封印", none: "—", you: "あなた", sealedBy: "{who} が #{n} を封印",
      legFinal: "確定", legPending: "保留中", drag: "ドラッグで回転",
      solo: "ソロ · 自分のノードのみ" },
  };
  function tl(k: string): string { const l = locale(); return L[l]?.[k] ?? L.en[k] ?? k; }
  function fill(tpl: string, v: Record<string, string | number>): string {
    let o = tpl; for (const [k, val] of Object.entries(v)) o = o.replace(`{${k}}`, String(val)); return o;
  }

  // ─── Mise en page / rythme ──────────────────────────────────────────────────
  const HEIGHT = 360;
  const MAX_BLOCKS = 22;
  const MAX_PEERS = 12;
  const N_AMB = 140;             // motes du fond ambiant
  const BURST_MAX = 320;         // pool de particules d'événement
  const WAVE_MAX = 12;           // pool d'ondes (anneaux) d'événement
  const CUBE = 0.34;             // arête d'un bloc

  const YAW0 = 0.92, PITCH0 = 0.34, DIST = 6.4;
  const TARGET: [number, number, number] = [0, -0.35, -2.1];

  const TAU_MOVE = 0.16, TAU_FINAL = 0.13, TAU_SCALE = 0.15, TAU_CAM = 0.10, TAU_PEER = 0.2;

  // Forge = tête de chaîne = position de TON nœud. worldPos(0) == FORGE.
  const FORGE: [number, number, number] = [0, 0.5, 0.75];
  // Hélice sobre de la chaîne : la tête au forge, recule + descend + spirale douce.
  function worldPos(p: number): [number, number, number] {
    const a = p * 0.28;
    return [Math.sin(a) * 0.5, 0.5 - 0.1 * p, 0.75 - 0.36 * p];
  }
  // Anneau des pairs (ellipse tassée en profondeur, sous le forge).
  const RING_C: [number, number, number] = [0, -0.5, -0.15];
  const RING_R = 2.15;
  function peerSlotPos(i: number, orbit: number): [number, number, number] {
    const a = orbit + (i / MAX_PEERS) * Math.PI * 2;
    return [RING_C[0] + Math.cos(a) * RING_R, RING_C[1] + Math.sin(a * 2) * 0.08, RING_C[2] + Math.sin(a) * RING_R * 0.55];
  }

  // ─── Couleurs (light-theme) ─────────────────────────────────────────────────
  const TEAL = [0.043, 0.647, 0.627];   // #0BA5A0
  const TEALB = [0.078, 0.784, 0.722];  // #14C8B8 (vif)
  const MOTE = [0.44, 0.56, 0.62];      // gris-teal discret sur blanc

  let canvas = $state<HTMLCanvasElement>();
  let wrap = $state<HTMLDivElement>();
  let glOk = $state(true);
  let glGen = $state(0);

  // ─── Bandeau de stats (données réelles, MAJ par props + événements) ─────────
  let sHeight = $state(0);
  let sEpoch = $state(0);
  let sSealer = $state("");     // @pseudo / court, ou vide
  let sSealerMine = $state(false);

  // ─── Étiquettes DOM (max 3, fade 4 s) ───────────────────────────────────────
  interface Label { id: number; text: string; mine: boolean }
  let labels = $state<Label[]>([]);
  let labelSeq = 0;

  // ─── Pont props → boucle GL (lu hors tracking) ──────────────────────────────
  let latestBlocks: { index: number }[] = [];
  let latestFloor = 0;
  let latestPeers: { quality_score?: number | null }[] = [];
  let layoutDirty = true;
  let peersDirty = true;
  let kick: (() => void) | null = null;

  // API exposée par l'effet GL vers l'effet événements (null tant que non monté).
  interface SceneApi {
    seal(mine: boolean): void;
    mined(): void;
    envelope(): void;
    vote(): void;
    elect(): void;
    boost(): void;
  }
  let api: SceneApi | null = null;

  $effect(() => {
    latestBlocks = (blocks ?? []) as { index: number }[];
    latestFloor = finalityFloor ?? 0;
    latestPeers = (peers ?? []) as { quality_score?: number | null }[];
    // Stats depuis les props (les événements affinent en direct).
    sHeight = Math.max(sHeight, height ?? 0, latestBlocks[0]?.index ?? 0);
    sEpoch = epoch ?? 0;
    layoutDirty = true;
    peersDirty = true;
    kick?.();
  });

  // ═══════════════════════════════════════════════════════════════════════════
  //  Scène WebGL2
  // ═══════════════════════════════════════════════════════════════════════════
  $effect(() => {
    const cv = canvas, wr = wrap;
    if (!cv) return;
    alertDiag("net-gl-mount", `gen=${glGen}`);

    const gl = cv.getContext("webgl2", { alpha: true, antialias: true, premultipliedAlpha: true });
    if (!gl) { glOk = false; note("net-gl", "webgl2 indisponible"); return; }

    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);

    // ── Shaders : blocs (cubes instanciés) ──
    const BLK_VS = `#version 300 es
    precision highp float;
    layout(location=0) in vec3 aPos;
    layout(location=1) in vec3 aNormal;
    layout(location=2) in vec3 iPos;
    layout(location=3) in float iScale;
    layout(location=4) in float iFinal;
    layout(location=5) in float iAlpha;
    uniform mat4 uVP;
    out vec3 vNormal; out float vFinal; out float vAlpha;
    void main() {
      vec3 world = iPos + aPos * iScale;
      gl_Position = uVP * vec4(world, 1.0);
      vNormal = aNormal; vFinal = iFinal; vAlpha = iAlpha;
    }`;
    const BLK_FS = `#version 300 es
    precision mediump float;
    in vec3 vNormal; in float vFinal; in float vAlpha;
    uniform vec3 uLightDir;
    out vec4 frag;
    void main() {
      vec3 N = normalize(vNormal);
      float diff = max(dot(N, normalize(uLightDir)), 0.0);
      float shade = 0.6 + diff * 0.44;
      vec3 frost = vec3(0.82, 0.94, 0.93);
      vec3 stone = vec3(0.031, 0.498, 0.549);
      vec3 col = mix(frost, stone, vFinal) * shade;
      col += vec3(0.05) * max(N.y, 0.0);
      float alpha = mix(0.55, 1.0, vFinal) * vAlpha;
      frag = vec4(col * alpha, alpha);
    }`;

    // ── Shaders : points (motes, nœud, pairs, particules, ondes) ──
    const PT_VS = `#version 300 es
    precision highp float;
    layout(location=0) in vec3 pPos;
    layout(location=1) in float pSize;
    layout(location=2) in vec4 pColor;
    layout(location=3) in float pMode;   // 0 = disque, 1 = anneau
    uniform mat4 uVP; uniform float uPx;
    out vec4 vColor; out float vMode;
    void main() {
      vec4 clip = uVP * vec4(pPos, 1.0);
      gl_Position = clip;
      gl_PointSize = clamp(pSize * uPx / max(clip.w, 0.001), 1.0, 700.0);
      vColor = pColor; vMode = pMode;
    }`;
    const PT_FS = `#version 300 es
    precision mediump float;
    in vec4 vColor; in float vMode; out vec4 frag;
    void main() {
      vec2 d = gl_PointCoord - vec2(0.5);
      float r = length(d);
      if (r > 0.5) discard;
      float a;
      if (vMode < 0.5) {
        a = smoothstep(0.5, 0.0, r);          // disque doux
      } else {
        a = smoothstep(0.10, 0.0, abs(r - 0.40)); // anneau fin
      }
      a *= vColor.a;
      frag = vec4(vColor.rgb * a, a);          // prémultiplié
    }`;

    function compile(type: number, src: string): WebGLShader | null {
      const s = gl!.createShader(type);
      if (!s) return null;
      gl!.shaderSource(s, src); gl!.compileShader(s);
      if (!gl!.getShaderParameter(s, gl!.COMPILE_STATUS)) {
        alertDiag("net-gl-compile", `${type === gl!.VERTEX_SHADER ? "VS" : "FS"}: ${gl!.getShaderInfoLog(s) ?? "?"}`.slice(0, 200));
        return null;
      }
      return s;
    }
    function linkProg(vsSrc: string, fsSrc: string): WebGLProgram | null {
      const vs = compile(gl!.VERTEX_SHADER, vsSrc), fs = compile(gl!.FRAGMENT_SHADER, fsSrc);
      if (!vs || !fs) return null;
      const p = gl!.createProgram();
      if (!p) return null;
      gl!.attachShader(p, vs); gl!.attachShader(p, fs); gl!.linkProgram(p);
      gl!.deleteShader(vs); gl!.deleteShader(fs);
      if (!gl!.getProgramParameter(p, gl!.LINK_STATUS)) {
        alertDiag("net-gl-link", (gl!.getProgramInfoLog(p) ?? "?").slice(0, 200));
        return null;
      }
      return p;
    }
    const blkProg = linkProg(BLK_VS, BLK_FS);
    const ptProg = linkProg(PT_VS, PT_FS);
    if (!blkProg || !ptProg) { glOk = false; return; }

    const uBlkVP = gl.getUniformLocation(blkProg, "uVP");
    const uBlkLight = gl.getUniformLocation(blkProg, "uLightDir");
    const uPtVP = gl.getUniformLocation(ptProg, "uVP");
    const uPtPx = gl.getUniformLocation(ptProg, "uPx");

    // ── Géométrie du cube (36 sommets, normales par face) ──
    const faces: { n: [number, number, number]; v: [number, number, number][] }[] = [
      { n: [0, 0, 1],  v: [[-.5, -.5, .5], [.5, -.5, .5], [.5, .5, .5], [-.5, .5, .5]] },
      { n: [0, 0, -1], v: [[.5, -.5, -.5], [-.5, -.5, -.5], [-.5, .5, -.5], [.5, .5, -.5]] },
      { n: [1, 0, 0],  v: [[.5, -.5, .5], [.5, -.5, -.5], [.5, .5, -.5], [.5, .5, .5]] },
      { n: [-1, 0, 0], v: [[-.5, -.5, -.5], [-.5, -.5, .5], [-.5, .5, .5], [-.5, .5, -.5]] },
      { n: [0, 1, 0],  v: [[-.5, .5, .5], [.5, .5, .5], [.5, .5, -.5], [-.5, .5, -.5]] },
      { n: [0, -1, 0], v: [[-.5, -.5, -.5], [.5, -.5, -.5], [.5, -.5, .5], [-.5, -.5, .5]] },
    ];
    const cube: number[] = [];
    for (const f of faces) for (const i of [0, 1, 2, 0, 2, 3]) {
      cube.push(f.v[i][0], f.v[i][1], f.v[i][2], f.n[0], f.n[1], f.n[2]);
    }
    const cubeVBO = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, cubeVBO);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(cube), gl.STATIC_DRAW);

    const MAX_INST = MAX_BLOCKS + 8;
    const instVBO = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, instVBO);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(MAX_INST * 6), gl.DYNAMIC_DRAW);
    const instData = new Float32Array(MAX_INST * 6);

    // Points : stride 9 floats (pos3 · size1 · color4 · mode1).
    const MAX_PTS = N_AMB + 2 + MAX_PEERS * 2 + WAVE_MAX + BURST_MAX + 8;
    const ptVBO = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, ptVBO);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(MAX_PTS * 9), gl.DYNAMIC_DRAW);
    const ptData = new Float32Array(MAX_PTS * 9);

    // ── VAO blocs ──
    const blkVAO = gl.createVertexArray();
    gl.bindVertexArray(blkVAO);
    gl.bindBuffer(gl.ARRAY_BUFFER, cubeVBO);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 24, 0);
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 3, gl.FLOAT, false, 24, 12);
    gl.bindBuffer(gl.ARRAY_BUFFER, instVBO);
    gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2, 3, gl.FLOAT, false, 24, 0); gl.vertexAttribDivisor(2, 1);
    gl.enableVertexAttribArray(3); gl.vertexAttribPointer(3, 1, gl.FLOAT, false, 24, 12); gl.vertexAttribDivisor(3, 1);
    gl.enableVertexAttribArray(4); gl.vertexAttribPointer(4, 1, gl.FLOAT, false, 24, 16); gl.vertexAttribDivisor(4, 1);
    gl.enableVertexAttribArray(5); gl.vertexAttribPointer(5, 1, gl.FLOAT, false, 24, 20); gl.vertexAttribDivisor(5, 1);

    // ── VAO points ──
    const ptVAO = gl.createVertexArray();
    gl.bindVertexArray(ptVAO);
    gl.bindBuffer(gl.ARRAY_BUFFER, ptVBO);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 36, 0);
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 1, gl.FLOAT, false, 36, 12);
    gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2, 4, gl.FLOAT, false, 36, 16);
    gl.enableVertexAttribArray(3); gl.vertexAttribPointer(3, 1, gl.FLOAT, false, 36, 32);
    gl.bindVertexArray(null);

    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.frontFace(gl.CCW);
    gl.cullFace(gl.BACK);
    gl.clearColor(0, 0, 0, 0);

    // ── Blocs : état d'animation par index ──
    type BAnim = { curP: number; tgtP: number; curFinal: number; tgtFinal: number; curScale: number; alive: boolean };
    const anims = new Map<number, BAnim>();
    let firstLayout = true;

    function applyLayout(list: { index: number }[], floor: number) {
      const seen = new Set<number>();
      list.forEach((b, i) => {
        seen.add(b.index);
        const tgtFinal = b.index <= floor ? 1 : 0;
        let a = anims.get(b.index);
        if (!a) {
          const isHead = i === 0 && !firstLayout && !reduced;
          a = { curP: isHead ? 0 : i, tgtP: i, curFinal: tgtFinal, tgtFinal, curScale: isHead ? 0 : 1, alive: true };
          anims.set(b.index, a);
        } else {
          a.tgtP = i; a.tgtFinal = tgtFinal; a.alive = true;
          if (reduced) { a.curP = i; a.curScale = 1; a.curFinal = tgtFinal; }
        }
      });
      for (const [idx, a] of anims) {
        if (!seen.has(idx)) {
          a.alive = false; a.tgtP = list.length + 1;
          if (reduced) anims.delete(idx);
        }
      }
      firstLayout = false;
    }

    // ── Pairs : slots + alpha/qualité lissés ──
    const peerAlpha = new Float32Array(MAX_PEERS);
    const peerQual = new Float32Array(MAX_PEERS);
    let peerActive = 0;
    function applyPeers(list: { quality_score?: number | null }[]) {
      peerActive = Math.min(list.length, MAX_PEERS);
      for (let i = 0; i < MAX_PEERS; i++) {
        if (i < peerActive) {
          const q = list[i]?.quality_score;
          peerQual[i] = Math.max(0.12, Math.min(1, (typeof q === "number" ? q : 55) / 100));
        }
      }
    }

    // ── Motes ambiantes (pool statique, dérive lente) ──
    const ambX = new Float32Array(N_AMB), ambY = new Float32Array(N_AMB), ambZ = new Float32Array(N_AMB);
    const ambVy = new Float32Array(N_AMB), ambPh = new Float32Array(N_AMB), ambTeal = new Float32Array(N_AMB);
    for (let i = 0; i < N_AMB; i++) {
      ambX[i] = (Math.random() - 0.5) * 5.4;
      ambY[i] = -2 + Math.random() * 3.4;
      ambZ[i] = -6 + Math.random() * 7.2;
      ambVy[i] = 0.03 + Math.random() * 0.05;
      ambPh[i] = Math.random() * Math.PI * 2;
      ambTeal[i] = Math.random() < 0.28 ? 1 : 0; // fraction teintée teal
    }

    // ── Particules d'événement (pool objets, réutilisés → 0 alloc/frame) ──
    interface Particle { on: boolean; x: number; y: number; z: number; vx: number; vy: number; vz: number; age: number; ttl: number; r: number; g: number; b: number; size: number }
    const parts: Particle[] = [];
    for (let i = 0; i < BURST_MAX; i++) parts.push({ on: false, x: 0, y: 0, z: 0, vx: 0, vy: 0, vz: 0, age: 0, ttl: 0, r: 0, g: 0, b: 0, size: 0 });
    let partCur = 0;
    function spawnPart(x: number, y: number, z: number, vx: number, vy: number, vz: number, ttl: number, col: number[], size: number) {
      const p = parts[partCur]; partCur = (partCur + 1) % BURST_MAX;
      p.on = true; p.x = x; p.y = y; p.z = z; p.vx = vx; p.vy = vy; p.vz = vz;
      p.age = 0; p.ttl = ttl; p.r = col[0]; p.g = col[1]; p.b = col[2]; p.size = size;
    }

    // ── Ondes (anneaux) d'événement ──
    interface Wave { on: boolean; x: number; y: number; z: number; age: number; ttl: number; r0: number; r1: number; r: number; g: number; b: number; peak: number }
    const waves: Wave[] = [];
    for (let i = 0; i < WAVE_MAX; i++) waves.push({ on: false, x: 0, y: 0, z: 0, age: 0, ttl: 0, r0: 0, r1: 0, r: 0, g: 0, b: 0, peak: 0 });
    let waveCur = 0;
    function spawnWave(x: number, y: number, z: number, ttl: number, r0: number, r1: number, col: number[], peak: number) {
      const w = waves[waveCur]; waveCur = (waveCur + 1) % WAVE_MAX;
      w.on = true; w.x = x; w.y = y; w.z = z; w.age = 0; w.ttl = ttl; w.r0 = r0; w.r1 = r1;
      w.r = col[0]; w.g = col[1]; w.b = col[2]; w.peak = peak;
    }

    // ── Générateurs d'événements (exposés via `api`) ──
    function evSeal(mine: boolean) {
      if (reduced) return;
      const col = mine ? TEALB : TEAL;
      const n = 46;
      for (let i = 0; i < n; i++) {
        const th = Math.random() * Math.PI * 2, ph = Math.acos(2 * Math.random() - 1), sp = 0.8 + Math.random() * 1.6;
        spawnPart(FORGE[0], FORGE[1], FORGE[2],
          Math.sin(ph) * Math.cos(th) * sp, Math.cos(ph) * sp * 0.8 + 0.4, Math.sin(ph) * Math.sin(th) * sp,
          0.55 + Math.random() * 0.5, col, 0.045 + Math.random() * 0.04);
      }
      spawnWave(FORGE[0], FORGE[1], FORGE[2], 1.1, 0.15, 1.5, mine ? TEALB : TEAL, mine ? 0.9 : 0.6);
    }
    function evMined() {
      if (reduced) return;
      for (let i = 0; i < 22; i++) {
        const th = Math.random() * Math.PI * 2, sp = 0.5 + Math.random() * 0.9;
        spawnPart(FORGE[0], FORGE[1], FORGE[2], Math.cos(th) * sp, 0.7 + Math.random() * 0.8, Math.sin(th) * sp,
          0.7 + Math.random() * 0.4, TEALB, 0.04 + Math.random() * 0.03);
      }
      spawnWave(FORGE[0], FORGE[1], FORGE[2], 1.0, 0.12, 1.15, TEALB, 0.85);
    }
    function evEnvelope() {
      if (reduced || peerActive === 0) return;
      const i = Math.floor(Math.random() * peerActive);
      const s = peerSlotPos(i, orbit);
      const dx = FORGE[0] - s[0], dy = FORGE[1] - s[1], dz = FORGE[2] - s[2];
      const d = Math.hypot(dx, dy, dz) || 1;
      for (let k = 0; k < 9; k++) {
        const t = k / 9;
        spawnPart(s[0] + dx * t * 0.15, s[1] + dy * t * 0.15, s[2] + dz * t * 0.15,
          (dx / d) * (1.7 + Math.random() * 0.6), (dy / d) * (1.7 + Math.random() * 0.6), (dz / d) * (1.7 + Math.random() * 0.6),
          0.45 + Math.random() * 0.2, TEAL, 0.03 + Math.random() * 0.02);
      }
    }
    function evVote() {
      if (reduced) return;
      spawnWave(FORGE[0], FORGE[1] - 0.1, FORGE[2] - 0.3, 1.4, 0.25, 1.9, TEAL, 0.5);
    }
    function evElect() {
      if (reduced) return;
      spawnWave(FORGE[0], FORGE[1], FORGE[2], 1.0, 0.35, 0.95, TEALB, 0.95);
      spawnWave(FORGE[0], FORGE[1], FORGE[2], 1.3, 0.35, 1.3, TEALB, 0.5);
    }
    let boostUntil = 0;
    function evBoost() { boostUntil = performance.now() + 2000; ensure(); }

    // ── Pas physique (lissage dt-indépendant, arrêt garanti côté blocs) ──
    let orbit = 0;
    const nodePulse = { v: 0 };
    function ease(dt: number, tau: number): number { return reduced ? 1 : 1 - Math.exp(-dt / tau); }

    function step(dt: number) {
      if (!reduced) orbit += dt * 0.05;
      // Blocs
      const kP = ease(dt, TAU_MOVE), kF = ease(dt, TAU_FINAL), kS = ease(dt, TAU_SCALE);
      for (const [idx, a] of anims) {
        const tgtScale = a.alive ? 1 : 0;
        a.curP += (a.tgtP - a.curP) * kP;
        a.curFinal += (a.tgtFinal - a.curFinal) * kF;
        a.curScale += (tgtScale - a.curScale) * kS;
        if (Math.abs(a.tgtP - a.curP) < 1e-3 && Math.abs(a.tgtFinal - a.curFinal) < 1e-3 && Math.abs(tgtScale - a.curScale) < 1e-3) {
          a.curP = a.tgtP; a.curFinal = a.tgtFinal; a.curScale = tgtScale;
        }
        if (!a.alive && a.curScale < 0.01) anims.delete(idx);
      }
      // Pairs
      const kPeer = ease(dt, TAU_PEER);
      for (let i = 0; i < MAX_PEERS; i++) {
        const tgt = i < peerActive ? 1 : 0;
        peerAlpha[i] += (tgt - peerAlpha[i]) * kPeer;
        if (Math.abs(tgt - peerAlpha[i]) < 1e-3) peerAlpha[i] = tgt;
      }
      if (reduced) return;
      // Motes : dérive verticale lente + léger balancement, wrap.
      for (let i = 0; i < N_AMB; i++) {
        ambY[i] += ambVy[i] * dt;
        if (ambY[i] > 1.5) { ambY[i] = -2; ambX[i] = (Math.random() - 0.5) * 5.4; ambZ[i] = -6 + Math.random() * 7.2; }
      }
      // Particules
      for (const p of parts) {
        if (!p.on) continue;
        p.age += dt;
        if (p.age >= p.ttl) { p.on = false; continue; }
        p.vy -= dt * 0.9;                 // gravité douce
        p.x += p.vx * dt; p.y += p.vy * dt; p.z += p.vz * dt;
        p.vx *= 0.96; p.vz *= 0.96;
      }
      // Ondes
      for (const w of waves) { if (w.on) { w.age += dt; if (w.age >= w.ttl) w.on = false; } }
      // Respiration du cœur du nœud
      nodePulse.v = 0.5 + 0.5 * Math.sin(performance.now() * 0.002);
    }

    // ── Caméra : cadrage fixe + orbite au glisser ──
    let yaw = YAW0, pitch = PITCH0, tYaw = YAW0, tPitch = PITCH0, velYaw = 0, velPitch = 0;
    let dragging = false, lx = 0, ly = 0;
    const onDown = (e: PointerEvent) => { dragging = true; lx = e.clientX; ly = e.clientY; velYaw = 0; velPitch = 0; };
    const onMove = (e: PointerEvent) => {
      if (!dragging) return;
      const dx = (e.clientX - lx) * 0.008, dy = (e.clientY - ly) * 0.006;
      tYaw += dx; tPitch = Math.max(0.08, Math.min(1.05, tPitch + dy));
      velYaw = dx; velPitch = dy; lx = e.clientX; ly = e.clientY;
      if (reduced) { yaw = tYaw; pitch = tPitch; renderOnce(); } else ensure();
    };
    const onUp = () => { dragging = false; };
    cv.addEventListener("pointerdown", onDown);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    function stepCamera(dt: number) {
      if (!dragging && !reduced) {
        if (Math.abs(velYaw) > 1e-4) { tYaw += velYaw; velYaw *= 0.9; } else velYaw = 0;
        if (Math.abs(velPitch) > 1e-4) { tPitch = Math.max(0.08, Math.min(1.05, tPitch + velPitch)); velPitch *= 0.9; } else velPitch = 0;
      }
      const k = ease(dt, TAU_CAM);
      yaw += (tYaw - yaw) * k; pitch += (tPitch - pitch) * k;
    }

    // ── Matrices (colonne-major) ──
    const proj = new Float32Array(16), view = new Float32Array(16), vp = new Float32Array(16);
    function buildVP(w: number, h: number) {
      const f = 1 / Math.tan(0.46), asp = w / h, near = 0.1, far = 40;
      proj.fill(0);
      proj[0] = f / asp; proj[5] = f; proj[10] = (far + near) / (near - far); proj[11] = -1;
      proj[14] = (2 * far * near) / (near - far);
      const ex = TARGET[0] + Math.cos(yaw) * Math.cos(pitch) * DIST;
      const ey = TARGET[1] + Math.sin(pitch) * DIST;
      const ez = TARGET[2] + Math.sin(yaw) * Math.cos(pitch) * DIST;
      let zx = ex - TARGET[0], zy = ey - TARGET[1], zz = ez - TARGET[2];
      const zl = Math.hypot(zx, zy, zz) || 1; zx /= zl; zy /= zl; zz /= zl;
      let xx = zz, xy = 0, xz = -zx;
      const xl = Math.hypot(xx, xy, xz) || 1; xx /= xl; xy /= xl; xz /= xl;
      const yx = zy * xz - zz * xy, yy = zz * xx - zx * xz, yz = zx * xy - zy * xx;
      view[0] = xx; view[1] = yx; view[2] = zx; view[3] = 0;
      view[4] = xy; view[5] = yy; view[6] = zy; view[7] = 0;
      view[8] = xz; view[9] = yz; view[10] = zz; view[11] = 0;
      view[12] = -(xx * ex + xy * ey + xz * ez);
      view[13] = -(yx * ex + yy * ey + yz * ez);
      view[14] = -(zx * ex + zy * ey + zz * ez);
      view[15] = 1;
      for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) {
        vp[c * 4 + r] = proj[r] * view[c * 4] + proj[4 + r] * view[c * 4 + 1] + proj[8 + r] * view[c * 4 + 2] + proj[12 + r] * view[c * 4 + 3];
      }
      return { ex, ey, ez, f };
    }

    // ── Écriture d'un point dans ptData ──
    function putPt(o: number, x: number, y: number, z: number, size: number, r: number, g: number, b: number, a: number, mode: number): number {
      ptData[o] = x; ptData[o + 1] = y; ptData[o + 2] = z; ptData[o + 3] = size;
      ptData[o + 4] = r; ptData[o + 5] = g; ptData[o + 6] = b; ptData[o + 7] = a; ptData[o + 8] = mode;
      return o + 9;
    }

    // ── Rendu d'une frame ──
    function render() {
      if (!cv) return;
      const w = wr?.clientWidth || 880;
      const fbw = Math.max(1, Math.round(w * dpr)), fbh = Math.round(HEIGHT * dpr);
      if (cv.width !== fbw || cv.height !== fbh) { cv.width = fbw; cv.height = fbh; }
      gl!.viewport(0, 0, cv.width, cv.height);
      const cam = buildVP(cv.width, cv.height);
      gl!.clear(gl!.COLOR_BUFFER_BIT);
      gl!.useProgram(ptProg);
      gl!.uniformMatrix4fv(uPtVP, false, vp);
      gl!.uniform1f(uPtPx, 0.5 * cv.height * cam.f);
      gl!.bindVertexArray(ptVAO);
      gl!.disable(gl!.CULL_FACE);

      // ── Passe A (fond) : motes + pairs (derrière la chaîne) ──
      let o = 0;
      const ambBase = reduced ? 1 : 1;
      for (let i = 0; i < N_AMB; i++) {
        const sway = reduced ? 0 : Math.sin(performance.now() * 0.0003 + ambPh[i]) * 0.12;
        const col = ambTeal[i] ? TEAL : MOTE;
        o = putPt(o, ambX[i] + sway, ambY[i], ambZ[i], 0.02, col[0], col[1], col[2], 0.17 * ambBase, 0);
      }
      for (let i = 0; i < MAX_PEERS; i++) {
        if (peerAlpha[i] < 0.01) continue;
        const s = peerSlotPos(i, orbit), q = peerQual[i];
        // Halo ∝ qualité, puis le point du pair.
        o = putPt(o, s[0], s[1], s[2], 0.16 + q * 0.34, TEAL[0], TEAL[1], TEAL[2], 0.14 * peerAlpha[i], 0);
        o = putPt(o, s[0], s[1], s[2], 0.13, TEAL[0], TEAL[1], TEAL[2], 0.9 * peerAlpha[i], 0);
      }
      const nBg = o / 9;
      if (nBg > 0) {
        gl!.bindBuffer(gl!.ARRAY_BUFFER, ptVBO);
        gl!.bufferSubData(gl!.ARRAY_BUFFER, 0, ptData, 0, o);
      }

      // ── Passe B : la chaîne (hélice, matière finalité), tri peintre ──
      const live: { a: BAnim; wp: [number, number, number]; d: number }[] = [];
      for (const a of anims.values()) {
        if (a.curScale < 0.005) continue;
        const wp = worldPos(a.curP);
        const dx = wp[0] - cam.ex, dy = wp[1] - cam.ey, dz = wp[2] - cam.ez;
        live.push({ a, wp, d: dx * dx + dy * dy + dz * dz });
      }
      live.sort((p, q) => q.d - p.d);
      const nBgDraw = nBg;

      // Foreground points d'abord empaquetés (nœud + ondes + particules), tracés après les blocs.
      let of = o;
      // Cœur du nœud (TON forge) : halo + noyau, respiration douce.
      const pulse = reduced ? 0.7 : (0.7 + nodePulse.v * 0.3);
      of = putPt(of, FORGE[0], FORGE[1], FORGE[2], 0.62 * pulse, TEALB[0], TEALB[1], TEALB[2], 0.22, 0);
      of = putPt(of, FORGE[0], FORGE[1], FORGE[2], 0.3, TEALB[0], TEALB[1], TEALB[2], 0.95, 0);
      // Ondes (anneaux).
      for (const wv of waves) {
        if (!wv.on) continue;
        const t = wv.age / wv.ttl;
        const rad = wv.r0 + (wv.r1 - wv.r0) * t;
        const a = wv.peak * (1 - t);
        of = putPt(of, wv.x, wv.y, wv.z, rad * 2, wv.r, wv.g, wv.b, a, 1);
      }
      // Particules.
      for (const p of parts) {
        if (!p.on) continue;
        const a = 1 - p.age / p.ttl;
        of = putPt(of, p.x, p.y, p.z, p.size, p.r, p.g, p.b, a * 0.95, 0);
      }
      const nFg = (of - nBgDraw * 9) / 9;
      if (nFg > 0) {
        gl!.bindBuffer(gl!.ARRAY_BUFFER, ptVBO);
        gl!.bufferSubData(gl!.ARRAY_BUFFER, nBgDraw * 9 * 4, ptData, nBgDraw * 9, nFg * 9);
      }

      // Draw passe A.
      if (nBgDraw > 0) gl!.drawArrays(gl!.POINTS, 0, nBgDraw);

      // Draw blocs.
      if (live.length > 0) {
        for (let i = 0; i < live.length; i++) {
          const { a, wp } = live[i], q = i * 6;
          instData[q] = wp[0]; instData[q + 1] = wp[1]; instData[q + 2] = wp[2];
          instData[q + 3] = CUBE * a.curScale; instData[q + 4] = a.curFinal; instData[q + 5] = 1;
        }
        gl!.useProgram(blkProg);
        gl!.bindVertexArray(blkVAO);
        gl!.bindBuffer(gl!.ARRAY_BUFFER, instVBO);
        gl!.bufferSubData(gl!.ARRAY_BUFFER, 0, instData, 0, live.length * 6);
        gl!.uniformMatrix4fv(uBlkVP, false, vp);
        gl!.uniform3f(uBlkLight, 0.4, 0.85, 0.55);
        gl!.enable(gl!.CULL_FACE);
        gl!.drawArraysInstanced(gl!.TRIANGLES, 0, 36, live.length);
      }

      // Draw passe B (foreground : nœud, ondes, particules) par-dessus la chaîne.
      if (nFg > 0) {
        gl!.useProgram(ptProg);
        gl!.bindVertexArray(ptVAO);
        gl!.disable(gl!.CULL_FACE);
        gl!.drawArrays(gl!.POINTS, nBgDraw, nFg);
      }
      gl!.bindVertexArray(null);
    }

    // ── Une frame complète : layout en attente → physique → rendu ──
    function tickAndRender(dt: number) {
      if (layoutDirty) { layoutDirty = false; applyLayout(latestBlocks, latestFloor); }
      if (peersDirty) { peersDirty = false; applyPeers(latestPeers); }
      step(dt);
      stepCamera(dt);
      render();
      try { (window as unknown as { __quantaSceneBeat?: number }).__quantaSceneBeat = performance.now(); } catch { /* best-effort */ }
    }
    function renderOnce() { if (cv && visible && inView) tickAndRender(0); }

    // ── Boucle ambiante throttlée (30 fps ; 60 fps pendant un boost) ──
    let raf = 0, running = false, visible = !document.hidden, inView = true, lastRender = 0;
    function frame() {
      raf = 0;
      if (!cv) return;
      if (!(visible && inView)) { running = false; return; }
      const now = performance.now();
      const interval = now < boostUntil ? 16.5 : 33.0;
      if (now - lastRender >= interval - 2) {
        let dt = (now - lastRender) / 1000; if (dt > 0.1) dt = 0.1;
        lastRender = now;
        tickAndRender(dt);
      }
      raf = requestAnimationFrame(frameSafe);
    }
    const frameSafe = () => {
      try { frame(); }
      catch (err) { alertDiag("net-gl-frame", `${(err as Error)?.stack ?? String(err)}`.slice(0, 300)); running = false; raf = 0; }
    };
    function ensure() {
      if (reduced) { renderOnce(); return; }   // reduced-motion : aucune boucle continue
      if (running || !(visible && inView) || raf) return;
      running = true; lastRender = performance.now() - 100;
      raf = requestAnimationFrame(frameSafe);
    }

    // ── Guards de pause ──
    const io = new IntersectionObserver((es) => {
      inView = es[es.length - 1]?.isIntersecting ?? true;
      if (inView) ensure();
    }, { threshold: 0.05 });
    io.observe(cv);
    const onVis = () => { visible = !document.hidden; if (visible) ensure(); };
    document.addEventListener("visibilitychange", onVis);

    const onLost = (e: Event) => { e.preventDefault(); note("net-gl", "contexte perdu — recréation"); glGen += 1; };
    cv.addEventListener("webglcontextlost", onLost);

    // Exposition + démarrage.
    api = { seal: evSeal, mined: evMined, envelope: evEnvelope, vote: evVote, elect: evElect, boost: evBoost };
    kick = () => ensure();
    layoutDirty = true; peersDirty = true;
    ensure();

    return () => {
      api = null; kick = null;
      if (raf) cancelAnimationFrame(raf);
      running = false;
      io.disconnect();
      document.removeEventListener("visibilitychange", onVis);
      cv.removeEventListener("pointerdown", onDown);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      cv.removeEventListener("webglcontextlost", onLost);
      try { (window as unknown as { __quantaSceneBeat?: number }).__quantaSceneBeat = -1; } catch { /* best-effort */ }
      gl.deleteBuffer(cubeVBO); gl.deleteBuffer(instVBO); gl.deleteBuffer(ptVBO);
      gl.deleteVertexArray(blkVAO); gl.deleteVertexArray(ptVAO);
      gl.deleteProgram(blkProg); gl.deleteProgram(ptProg);
    };
  });

  // ═══════════════════════════════════════════════════════════════════════════
  //  Événements réels du nœud → pics étiquetés + stats
  // ═══════════════════════════════════════════════════════════════════════════
  function shortAddr(a?: string): string { return a ? a.replace(/^0x/, "").slice(0, 8) : ""; }
  function pushLabel(text: string, mine: boolean) {
    const id = ++labelSeq;
    labels = [...labels, { id, text, mine }].slice(-3);
    const to = setTimeout(() => { labels = labels.filter((l) => l.id !== id); labelTimers.delete(to); }, 4000);
    labelTimers.add(to);
  }
  const labelTimers = new Set<ReturnType<typeof setTimeout>>();

  $effect(() => {
    const unsubs: UnlistenFn[] = [];
    let alive = true;
    (async () => {
      const u1 = await listen<{ index: number; txs: number; mine: boolean; miner?: string; miner_name?: string | null }>(
        "quanta://block-sealed", (e) => {
          const p = e.payload; if (!p) return;
          sHeight = Math.max(sHeight, p.index);
          const who = p.mine ? tl("you") : (p.miner_name ? "@" + p.miner_name : shortAddr(p.miner) || "?");
          sSealer = who; sSealerMine = !!p.mine;
          pushLabel(fill(tl("sealedBy"), { who, n: p.index }), !!p.mine);
          api?.seal(!!p.mine); api?.boost();
        });
      const u2 = await listen("quanta://mined", () => { api?.mined(); api?.boost(); });
      const u3 = await listen<{ kind?: string; verdict?: string }>("quanta://engine", (e) => {
        const p = e.payload; if (!p) return;
        if (p.kind === "envelope") { api?.envelope(); api?.boost(); }
        else if (p.kind === "vote") { api?.vote(); api?.boost(); }
        else if (p.kind === "elect" && p.verdict === "leader") { api?.elect(); api?.boost(); }
      });
      if (!alive) { u1(); u2(); u3(); return; }
      unsubs.push(u1, u2, u3);
    })();
    return () => {
      alive = false;
      unsubs.forEach((u) => u());
      for (const to of labelTimers) clearTimeout(to);
      labelTimers.clear();
    };
  });

  const sealerText = $derived(sSealer || tl("none"));
</script>

<div class="scene-wrap" bind:this={wrap}>
  {#if glOk}
    {#key glGen}
      <canvas bind:this={canvas} class="scene-canvas" aria-label={tl('title')}></canvas>
    {/key}

    <!-- Étiquettes d'événement (max 3, fade 4 s) — « qui a scellé quel bloc » -->
    <div class="scene-labels" aria-live="polite">
      {#each labels as lb (lb.id)}
        <div class="scene-label" class:mine={lb.mine}>
          <span class="scene-label-dot"></span>{lb.text}
        </div>
      {/each}
    </div>

    <!-- Bandeau de stats réelles -->
    <div class="scene-stats">
      <span class="st"><em>{tl('height')}</em> {sHeight}</span>
      <span class="st"><em>{tl('peers')}</em> {peerCount}</span>
      <span class="st"><em>{tl('epoch')}</em> {sEpoch}</span>
      <span class="st st-sealer" class:mine={sSealerMine}><em>{tl('sealer')}</em> {sealerText}</span>
      {#if peerCount === 0}<span class="st st-solo">{tl('solo')}</span>{/if}
    </div>

    <!-- Légende finalité + hint -->
    <div class="scene-legend">
      <span class="lg"><span class="sw sw-final"></span>{tl('legFinal')}</span>
      <span class="lg"><span class="sw sw-frost"></span>{tl('legPending')}</span>
    </div>
    <div class="scene-hint">{tl('drag')}</div>
  {/if}
</div>

<style>
  .scene-wrap { position: relative; width: 100%; height: 360px; }
  .scene-canvas {
    display: block; width: 100%; height: 360px;
    background: transparent;
    cursor: grab; touch-action: none;
    border-radius: var(--radius-sm);
  }
  .scene-canvas:active { cursor: grabbing; }

  /* ── Étiquettes d'événement ── */
  .scene-labels {
    position: absolute; left: 14px; top: 12px;
    display: flex; flex-direction: column; gap: 6px;
    pointer-events: none; max-width: 72%;
  }
  .scene-label {
    display: inline-flex; align-items: center; gap: 7px;
    font-size: 12px; font-weight: 500; color: var(--color-text-1);
    background: color-mix(in srgb, var(--surface) 88%, transparent);
    border: 1px solid var(--color-border);
    padding: 5px 11px; border-radius: 100px;
    box-shadow: var(--shadow-sm);
    animation: label-in 0.32s cubic-bezier(.2,.8,.2,1);
    font-variant-numeric: tabular-nums lining-nums;
  }
  .scene-label.mine { border-color: var(--color-accent); color: var(--color-text-0); }
  .scene-label-dot {
    width: 7px; height: 7px; border-radius: 50%;
    background: var(--color-accent); flex-shrink: 0;
  }
  .scene-label.mine .scene-label-dot { box-shadow: 0 0 0 3px var(--color-accent-dim); }
  @keyframes label-in { from { opacity: 0; transform: translateY(-6px); } to { opacity: 1; transform: none; } }

  /* ── Bandeau de stats réelles ── */
  .scene-stats {
    position: absolute; left: 14px; bottom: 40px;
    display: flex; flex-wrap: wrap; gap: 4px 14px;
    font-size: 11.5px; color: var(--color-text-2);
    font-variant-numeric: tabular-nums lining-nums;
    background: color-mix(in srgb, var(--surface) 80%, transparent);
    padding: 6px 11px; border-radius: 10px;
    border: 1px solid var(--color-border);
    pointer-events: none; max-width: calc(100% - 28px);
  }
  .st em { font-style: normal; color: var(--color-text-3); text-transform: uppercase; letter-spacing: 0.05em; font-size: 9.5px; margin-right: 5px; }
  .st-sealer { color: var(--color-text-1); }
  .st-sealer.mine { color: var(--color-accent); }
  .st-solo { color: var(--color-text-3); }

  /* ── Légende finalité ── */
  .scene-legend {
    position: absolute; left: 14px; bottom: 12px;
    display: flex; gap: 14px; align-items: center; flex-wrap: wrap;
    font-size: 11px; color: var(--color-text-2);
    pointer-events: none;
  }
  .lg { display: inline-flex; align-items: center; gap: 6px; }
  .sw { width: 8px; height: 8px; }
  .sw-final { background: #087F8C; border-radius: 2px; }
  .sw-frost { background: rgba(11, 165, 160, 0.28); border: 1px solid rgba(11, 165, 160, 0.5); border-radius: 2px; }
  .scene-hint {
    position: absolute; right: 14px; bottom: 12px;
    font-size: 10.5px; color: var(--color-text-3);
    pointer-events: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .scene-label { animation: none; }
  }
</style>
