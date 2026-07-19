<script lang="ts">
  // La chaîne réelle en 3D — FACTUELLE, pas un écran de veille.
  //
  // Sujet = les blocs récents de la chaîne (même sondage que la bande 2D),
  // empilés en escalier sobre. La frontière de finalité Casper-FFG est VISIBLE :
  //   · bloc d'index ≤ plancher  = pierre teal pleine, opaque (irréversible)
  //   · bloc d'index > plancher  = verre givré clair, translucide (pas encore final)
  // Une démarcation nette sépare les deux : c'est l'argument produit.
  //
  // Doctrine du mouvement (identique au terminal de l'app) : RIEN ne bouge sans
  // un événement réel. Les seules animations sont provoquées par la donnée :
  //   · arrivée d'un nouveau bloc (la hauteur monte) → le bloc entre, la pile
  //     glisse d'un cran (~0,8 s, ease-out) puis s'immobilise ;
  //   · progression du plancher → transition de matériau givre→pierre (~0,6 s).
  // Au repos, la boucle rAF est ARRÊTÉE : zéro GPU tant que rien ne change.
  //
  // Hygiène anti-gel (leçons des refontes) : WebGL2 pur, zéro dépendance ;
  // AUCUNE boucle permanente (la boucle se relance sur événement via `kick`,
  // et s'auto-arrête quand tout est stabilisé) ; pause hors-viewport / onglet
  // caché ; perte de contexte gérée par recréation ; cleanup complet au démontage.
  import { t } from "./i18n.svelte";
  import { note, alertDiag } from "./diag";

  let { peerCount = 0, blocks = [], finalityFloor = 0 } = $props<{
    peerCount?: number;
    blocks?: { index: number }[];
    finalityFloor?: number;
  }>();

  // ─── Réglages de scène (constantes de mise en page / rythme) ────────────────
  const HEIGHT = 280;              // hauteur de la carte (réduite, sobre)
  const MAX_BLOCKS = 22;           // fenêtre affichée (= limite du sondage chaîne)
  const CUBE = 0.4;                // arête d'un bloc
  const STEP_Z = 0.46;             // recul en profondeur par bloc (léger espace)
  const STEP_Y = 0.12;             // descente par bloc (l'escalier)
  const YAW0 = 0.72, PITCH0 = 0.42, DIST = 6.6; // cadrage fixe élégant, 3/4 élevé
  const TARGET: [number, number, number] = [0, -0.7, -2.4];
  const MAX_PEERS = 10;            // points de pairs périphériques (fixes)
  const TAU_MOVE = 0.16;           // constante de temps du glissement (~0,8 s)
  const TAU_FINAL = 0.13;          // transition de matériau (~0,6 s)
  const TAU_SCALE = 0.15;          // apparition/disparition d'un bloc
  const TAU_CAM = 0.10;            // suivi de caméra au glisser

  // Position monde d'un bloc à la position continue `p` (0 = tête, la plus récente).
  function worldPos(p: number): [number, number, number] {
    return [0, -STEP_Y * p, -STEP_Z * p];
  }

  // Points de pairs : éventail fixe au premier plan bas (slot i toujours au même
  // endroit → connecter un pair allume le slot suivant, sans reflow des autres).
  const peerSlots: [number, number, number][] = [];
  for (let i = 0; i < MAX_PEERS; i++) {
    const ang = (i / (MAX_PEERS - 1) - 0.5) * 1.7;
    peerSlots.push([Math.sin(ang) * 1.9, -0.15, 0.7 - Math.cos(ang) * 0.5]);
  }

  let canvas = $state<HTMLCanvasElement>();
  let wrap = $state<HTMLDivElement>();
  let glOk = $state(true);
  // Génération GL : incrémentée à la perte de contexte → {#key} recrée le canvas
  // et l'effet repart sur un contexte frais (sinon gel sur la dernière image).
  let glGen = $state(0);

  // ─── Pont props → boucle GL (variables simples, lues hors tracking) ──────────
  let latestBlocks: { index: number }[] = [];
  let latestFloor = 0;
  let latestPeerCount = 0;
  let layoutDirty = true;
  // Déduplication : ne réveille la scène que si quelque chose de visible a changé
  // (tête de chaîne, plancher, nombre de pairs) → vrai repos entre les sondages.
  let lastHead = NaN, lastFloor = NaN, lastPC = NaN;
  // Réveil de la boucle, fourni par l'effet GL (null tant que non monté / démonté).
  let kick: (() => void) | null = null;

  $effect(() => {
    const b = (blocks ?? []) as { index: number }[];
    const fl = finalityFloor ?? 0;
    const pc = peerCount ?? 0;
    latestBlocks = b;
    latestFloor = fl;
    latestPeerCount = pc;
    const head = b[0]?.index ?? -1;
    if (head === lastHead && fl === lastFloor && pc === lastPC) return; // rien de neuf
    lastHead = head; lastFloor = fl; lastPC = pc;
    layoutDirty = true;
    kick?.();
  });

  // ─── Scène WebGL2 ───────────────────────────────────────────────────────────
  $effect(() => {
    const cv = canvas, wr = wrap;
    if (!cv) return;
    alertDiag("gl-mount", `gen=${glGen}`);

    const gl = cv.getContext("webgl2", { alpha: true, antialias: true, premultipliedAlpha: true });
    if (!gl) { glOk = false; note("gl", "webgl2 indisponible"); return; }

    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);

    // ── Shaders : blocs (cubes 3D instanciés) ───────────────────────────────
    const BLK_VS = `#version 300 es
    precision highp float;
    layout(location=0) in vec3 aPos;      // sommet du cube unitaire
    layout(location=1) in vec3 aNormal;   // normale de face
    layout(location=2) in vec3 iPos;      // centre monde du bloc
    layout(location=3) in float iScale;   // taille (animée à l'entrée)
    layout(location=4) in float iFinal;   // 0 = givre, 1 = pierre (finalité)
    layout(location=5) in float iAlpha;   // fondu d'entrée
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
      float shade = 0.58 + diff * 0.46;               // lumière douce, jamais de noir
      vec3 frost = vec3(0.82, 0.94, 0.93);            // verre givré clair (en attente)
      vec3 stone = vec3(0.031, 0.498, 0.549);         // #087F8C pierre teal (finalisé)
      vec3 col = mix(frost, stone, vFinal) * shade;
      col += vec3(0.05) * max(N.y, 0.0);              // léger éclat sur le dessus
      float alpha = mix(0.5, 1.0, vFinal) * vAlpha;   // givre translucide, pierre opaque
      frag = vec4(col * alpha, alpha);                // prémultiplié
    }`;

    // ── Shaders : points (ombres de contact + pairs) ────────────────────────
    const PT_VS = `#version 300 es
    precision highp float;
    layout(location=0) in vec3 pPos;
    layout(location=1) in float pSize;    // taille monde
    layout(location=2) in vec4 pColor;    // rgb + alpha
    uniform mat4 uVP; uniform float uPx;  // = 0.5 * hauteurFB * f
    out vec4 vColor;
    void main() {
      vec4 clip = uVP * vec4(pPos, 1.0);
      gl_Position = clip;
      gl_PointSize = clamp(pSize * uPx / max(clip.w, 0.001), 1.0, 400.0);
      vColor = pColor;
    }`;

    const PT_FS = `#version 300 es
    precision mediump float;
    in vec4 vColor; out vec4 frag;
    void main() {
      vec2 d = gl_PointCoord - vec2(0.5);
      float r = length(d);
      if (r > 0.5) discard;
      float soft = smoothstep(0.5, 0.0, r);
      float a = vColor.a * soft;
      frag = vec4(vColor.rgb * a, a);                 // prémultiplié
    }`;

    function compile(type: number, src: string): WebGLShader | null {
      const s = gl!.createShader(type);
      if (!s) return null;
      gl!.shaderSource(s, src); gl!.compileShader(s);
      if (!gl!.getShaderParameter(s, gl!.COMPILE_STATUS)) {
        alertDiag("gl-compile", `${type === gl!.VERTEX_SHADER ? "VS" : "FS"}: ${gl!.getShaderInfoLog(s) ?? "?"}`.slice(0, 200));
        return null;
      }
      return s;
    }
    function link(vsSrc: string, fsSrc: string): WebGLProgram | null {
      const vs = compile(gl!.VERTEX_SHADER, vsSrc), fs = compile(gl!.FRAGMENT_SHADER, fsSrc);
      if (!vs || !fs) return null;
      const p = gl!.createProgram();
      if (!p) return null;
      gl!.attachShader(p, vs); gl!.attachShader(p, fs); gl!.linkProgram(p);
      gl!.deleteShader(vs); gl!.deleteShader(fs);
      if (!gl!.getProgramParameter(p, gl!.LINK_STATUS)) {
        alertDiag("gl-link", (gl!.getProgramInfoLog(p) ?? "?").slice(0, 200));
        return null;
      }
      return p;
    }
    const blkProg = link(BLK_VS, BLK_FS);
    const ptProg = link(PT_VS, PT_FS);
    if (!blkProg || !ptProg) { glOk = false; return; }

    const uBlkVP = gl.getUniformLocation(blkProg, "uVP");
    const uBlkLight = gl.getUniformLocation(blkProg, "uLightDir");
    const uPtVP = gl.getUniformLocation(ptProg, "uVP");
    const uPtPx = gl.getUniformLocation(ptProg, "uPx");

    // ── Géométrie du cube (36 sommets, normales par face, CCW vers l'extérieur) ──
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

    // ── VBO dynamiques : instances de blocs + points ────────────────────────
    const MAX_INST = MAX_BLOCKS + 10;      // marge pour les blocs en disparition
    const instVBO = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, instVBO);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(MAX_INST * 6), gl.DYNAMIC_DRAW);
    const instData = new Float32Array(MAX_INST * 6);

    const MAX_PTS = MAX_INST + MAX_PEERS;
    const ptVBO = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, ptVBO);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(MAX_PTS * 8), gl.DYNAMIC_DRAW);
    const ptData = new Float32Array(MAX_PTS * 8);

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
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 32, 0);
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 1, gl.FLOAT, false, 32, 12);
    gl.enableVertexAttribArray(2); gl.vertexAttribPointer(2, 4, gl.FLOAT, false, 32, 16);
    gl.bindVertexArray(null);

    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.frontFace(gl.CCW);
    gl.cullFace(gl.BACK);
    gl.clearColor(0, 0, 0, 0);

    // ── État d'animation par bloc (indexé par index de bloc) ─────────────────
    type BAnim = { curP: number; tgtP: number; curFinal: number; tgtFinal: number; curScale: number; alive: boolean };
    const anims = new Map<number, BAnim>();
    let firstLayout = true;
    const peerAlpha = new Float32Array(MAX_PEERS);

    function applyLayout(list: { index: number }[], floor: number) {
      const seen = new Set<number>();
      list.forEach((b, i) => {
        seen.add(b.index);
        const tgtFinal = b.index <= floor ? 1 : 0;
        let a = anims.get(b.index);
        if (!a) {
          const isHead = i === 0 && !firstLayout && !reduced;
          a = {
            curP: isHead ? -0.7 : i,
            tgtP: i,
            curFinal: tgtFinal,
            tgtFinal,
            curScale: isHead ? 0 : 1,
            alive: true,
          };
          anims.set(b.index, a);
        } else {
          a.tgtP = i;
          a.tgtFinal = tgtFinal;
          a.alive = true;
          if (reduced) { a.curP = i; a.curScale = 1; a.curFinal = tgtFinal; }
        }
      });
      // Blocs sortis de la fenêtre : disparition douce (puis suppression).
      for (const [idx, a] of anims) {
        if (!seen.has(idx)) {
          a.alive = false;
          a.tgtP = list.length + 1;
          if (reduced) anims.delete(idx);
        }
      }
      firstLayout = false;
    }

    // ── Pas d'animation : lissage exponentiel dt-indépendant, arrêt garanti ──
    function ease(dt: number, tau: number): number { return reduced ? 1 : 1 - Math.exp(-dt / tau); }

    function stepBlocks(dt: number): boolean {
      const kP = ease(dt, TAU_MOVE), kF = ease(dt, TAU_FINAL), kS = ease(dt, TAU_SCALE);
      let moving = false;
      for (const [idx, a] of anims) {
        const tgtScale = a.alive ? 1 : 0;
        a.curP += (a.tgtP - a.curP) * kP;
        a.curFinal += (a.tgtFinal - a.curFinal) * kF;
        a.curScale += (tgtScale - a.curScale) * kS;
        if (Math.abs(a.tgtP - a.curP) > 1e-3 || Math.abs(a.tgtFinal - a.curFinal) > 1e-3 || Math.abs(tgtScale - a.curScale) > 1e-3) {
          moving = true;
        } else {
          a.curP = a.tgtP; a.curFinal = a.tgtFinal; a.curScale = tgtScale;
        }
        if (!a.alive && a.curScale < 0.01) anims.delete(idx);
      }
      return moving;
    }

    function stepPeers(dt: number, count: number): boolean {
      const k = ease(dt, TAU_SCALE);
      let moving = false;
      for (let i = 0; i < MAX_PEERS; i++) {
        const tgt = i < count ? 1 : 0;
        peerAlpha[i] += (tgt - peerAlpha[i]) * k;
        if (Math.abs(tgt - peerAlpha[i]) > 1e-3) moving = true;
        else peerAlpha[i] = tgt;
      }
      return moving;
    }

    // ── Caméra : cadrage fixe, orbite au glisser + inertie courte ────────────
    let yaw = YAW0, pitch = PITCH0, tYaw = YAW0, tPitch = PITCH0;
    let velYaw = 0, velPitch = 0;
    let dragging = false, lx = 0, ly = 0;

    const onDown = (e: PointerEvent) => { dragging = true; lx = e.clientX; ly = e.clientY; velYaw = 0; velPitch = 0; };
    const onMove = (e: PointerEvent) => {
      if (!dragging) return;
      const dx = (e.clientX - lx) * 0.008, dy = (e.clientY - ly) * 0.006;
      tYaw += dx;
      tPitch = Math.max(0.08, Math.min(1.15, tPitch + dy));
      velYaw = dx; velPitch = dy;
      lx = e.clientX; ly = e.clientY;
      ensure();
    };
    const onUp = () => { dragging = false; };
    cv.addEventListener("pointerdown", onDown);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);

    function stepCamera(dt: number): boolean {
      if (!dragging && !reduced) {
        // Inertie ease-out puis arrêt complet.
        if (Math.abs(velYaw) > 1e-4) { tYaw += velYaw; velYaw *= 0.9; } else velYaw = 0;
        if (Math.abs(velPitch) > 1e-4) { tPitch = Math.max(0.08, Math.min(1.15, tPitch + velPitch)); velPitch *= 0.9; } else velPitch = 0;
      } else if (!dragging) { velYaw = 0; velPitch = 0; }
      const k = ease(dt, TAU_CAM);
      yaw += (tYaw - yaw) * k;
      pitch += (tPitch - pitch) * k;
      const moving = Math.abs(tYaw - yaw) > 1e-4 || Math.abs(tPitch - pitch) > 1e-4 ||
        Math.abs(velYaw) > 1e-4 || Math.abs(velPitch) > 1e-4;
      if (!moving) { yaw = tYaw; pitch = tPitch; }
      return moving;
    }

    // ── Matrices (colonne-major) ─────────────────────────────────────────────
    const proj = new Float32Array(16), view = new Float32Array(16), vp = new Float32Array(16);
    function buildVP(w: number, h: number) {
      const f = 1 / Math.tan(0.45), asp = w / h, near = 0.1, far = 30;
      proj.fill(0);
      proj[0] = f / asp; proj[5] = f; proj[10] = (far + near) / (near - far); proj[11] = -1;
      proj[14] = (2 * far * near) / (near - far);
      const ex = TARGET[0] + Math.cos(yaw) * Math.cos(pitch) * DIST;
      const ey = TARGET[1] + Math.sin(pitch) * DIST;
      const ez = TARGET[2] + Math.sin(yaw) * Math.cos(pitch) * DIST;
      let zx = ex - TARGET[0], zy = ey - TARGET[1], zz = ez - TARGET[2];
      const zl = Math.hypot(zx, zy, zz) || 1; zx /= zl; zy /= zl; zz /= zl;
      let xx = zz, xy = 0, xz = -zx;               // up=(0,1,0) × z
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

    // ── Rendu d'une frame (positions déjà calculées côté CPU) ────────────────
    function render() {
      if (!cv) return; // TS : le narrowing du const ne traverse pas la closure
      const w = wr?.clientWidth || 880;
      const fbw = Math.max(1, Math.round(w * dpr)), fbh = Math.round(HEIGHT * dpr);
      if (cv.width !== fbw || cv.height !== fbh) { cv.width = fbw; cv.height = fbh; }
      gl!.viewport(0, 0, cv.width, cv.height);
      const cam = buildVP(cv.width, cv.height);
      gl!.clear(gl!.COLOR_BUFFER_BIT);

      // Blocs : tri peintre arrière→avant (pas de depth buffer ; ~22 éléments).
      const live: { a: BAnim; wp: [number, number, number]; d: number }[] = [];
      for (const a of anims.values()) {
        if (a.curScale < 0.005) continue;
        const wp = worldPos(a.curP);
        const dx = wp[0] - cam.ex, dy = wp[1] - cam.ey, dz = wp[2] - cam.ez;
        live.push({ a, wp, d: dx * dx + dy * dy + dz * dz });
      }
      live.sort((p, q) => q.d - p.d);

      // Points : ombres de contact (sous chaque bloc) + pairs (éventail périphérique).
      let np = 0;
      for (const { a, wp } of live) {
        const o = np * 8;
        ptData[o] = wp[0]; ptData[o + 1] = wp[1] - 0.26; ptData[o + 2] = wp[2];
        ptData[o + 3] = CUBE * 1.5 * a.curScale;
        ptData[o + 4] = 0.11; ptData[o + 5] = 0.11; ptData[o + 6] = 0.12; // ombre encre
        ptData[o + 7] = 0.1 * a.curScale;
        np++;
      }
      for (let i = 0; i < MAX_PEERS; i++) {
        if (peerAlpha[i] < 0.01) continue;
        const o = np * 8, s = peerSlots[i];
        ptData[o] = s[0]; ptData[o + 1] = s[1]; ptData[o + 2] = s[2];
        ptData[o + 3] = 0.13;
        ptData[o + 4] = 0.043; ptData[o + 5] = 0.647; ptData[o + 6] = 0.627; // teal
        ptData[o + 7] = 0.9 * peerAlpha[i];
        np++;
      }
      if (np > 0) {
        gl!.useProgram(ptProg);
        gl!.bindVertexArray(ptVAO);
        gl!.bindBuffer(gl!.ARRAY_BUFFER, ptVBO);
        gl!.bufferSubData(gl!.ARRAY_BUFFER, 0, ptData, 0, np * 8);
        gl!.uniformMatrix4fv(uPtVP, false, vp);
        gl!.uniform1f(uPtPx, 0.5 * cv.height * cam.f);
        gl!.disable(gl!.CULL_FACE);
        gl!.drawArrays(gl!.POINTS, 0, np);
      }

      // Blocs par-dessus (l'escalier, matière finalité).
      if (live.length > 0) {
        for (let i = 0; i < live.length; i++) {
          const { a, wp } = live[i], o = i * 6;
          instData[o] = wp[0]; instData[o + 1] = wp[1]; instData[o + 2] = wp[2];
          instData[o + 3] = CUBE * a.curScale;
          instData[o + 4] = a.curFinal;
          instData[o + 5] = 1;
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
      gl!.bindVertexArray(null);
    }

    // ── Boucle événementielle : tourne pendant les animations, s'arrête au repos ──
    let raf = 0, running = false, visible = !document.hidden, inView = true, lastT = 0;

    function frame() {
      raf = 0;
      if (!cv) return; // TS : le narrowing du const ne traverse pas la closure
      if (!(visible && inView)) { running = false; return; }
      const now = performance.now();
      const dt = Math.min((now - lastT) / 1000, 0.05);
      lastT = now;
      if (layoutDirty) { layoutDirty = false; applyLayout(latestBlocks, latestFloor); }
      const m1 = stepBlocks(dt);
      const m2 = stepPeers(dt, Math.min(latestPeerCount, MAX_PEERS));
      const m3 = stepCamera(dt);
      render();
      if (!reduced && (m1 || m2 || m3 || dragging)) {
        raf = requestAnimationFrame(frameSafe);
      } else {
        running = false; // stabilisé → zéro GPU jusqu'au prochain événement
      }
    }
    // Une exception ne doit jamais figer la scène (running bloqué à true) :
    // on capture, on trace, et le prochain `kick` pourra relancer proprement.
    const frameSafe = () => {
      try { frame(); }
      catch (err) {
        alertDiag("gl-frame-err", `${(err as Error)?.stack ?? String(err)}`.slice(0, 300));
        running = false; raf = 0;
      }
    };
    function ensure() {
      if (running || !(visible && inView) || raf) return;
      running = true;
      lastT = performance.now();
      raf = requestAnimationFrame(frameSafe);
    }

    // ── Guards de pause : hors-viewport / onglet caché ───────────────────────
    const io = new IntersectionObserver((es) => {
      inView = es[es.length - 1]?.isIntersecting ?? true;
      if (inView) ensure();
    }, { threshold: 0.05 });
    io.observe(cv);
    const onVis = () => { visible = !document.hidden; if (visible) ensure(); };
    document.addEventListener("visibilitychange", onVis);

    // Perte de contexte GL : recréation via {#key glGen} (sinon gel définitif).
    const onLost = (e: Event) => { e.preventDefault(); note("gl", "contexte perdu — recréation"); glGen += 1; };
    cv.addEventListener("webglcontextlost", onLost);

    // Réveil depuis le pont props (nouveau bloc / plancher / pairs).
    kick = () => ensure();
    layoutDirty = true;
    ensure();

    return () => {
      kick = null;
      if (raf) cancelAnimationFrame(raf);
      running = false;
      io.disconnect();
      document.removeEventListener("visibilitychange", onVis);
      cv.removeEventListener("pointerdown", onDown);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      cv.removeEventListener("webglcontextlost", onLost);
      gl.deleteBuffer(cubeVBO); gl.deleteBuffer(instVBO); gl.deleteBuffer(ptVBO);
      gl.deleteVertexArray(blkVAO); gl.deleteVertexArray(ptVAO);
      gl.deleteProgram(blkProg); gl.deleteProgram(ptProg);
    };
  });
</script>

<div class="scene-wrap" bind:this={wrap}>
  {#if glOk}
    {#key glGen}
      <canvas bind:this={canvas} class="scene-canvas" aria-label={t('net.chainTitle')}></canvas>
    {/key}
    <div class="scene-legend">
      <span class="lg"><span class="sw sw-final"></span>{t('net3d.legFinal')}</span>
      <span class="lg"><span class="sw sw-frost"></span>{t('net3d.legPending')}</span>
      {#if peerCount > 0}
        <span class="lg"><span class="sw sw-peer"></span>{peerCount} {peerCount === 1 ? t('wallet.peer') : t('wallet.peers')}</span>
      {/if}
    </div>
    <div class="scene-hint">{t('net3d.dragHint')}</div>
  {/if}
</div>

<style>
  .scene-wrap { position: relative; width: 100%; height: 280px; }
  .scene-canvas {
    display: block; width: 100%; height: 280px;
    background: transparent;         /* la carte claire porte le fond */
    cursor: grab; touch-action: none;
    border-radius: var(--radius-sm);
  }
  .scene-canvas:active { cursor: grabbing; }
  .scene-legend {
    position: absolute; left: 14px; bottom: 12px;
    display: flex; gap: 14px; align-items: center; flex-wrap: wrap;
    font-size: 11px; color: var(--color-text-2);
    background: color-mix(in srgb, var(--surface) 82%, transparent);
    padding: 6px 10px; border-radius: 100px;
    border: 1px solid var(--color-border);
    backdrop-filter: blur(6px);
    pointer-events: none;
  }
  .lg { display: inline-flex; align-items: center; gap: 6px; }
  .sw { width: 8px; height: 8px; }
  .sw-final { background: #087F8C; border-radius: 2px; }
  .sw-frost { background: rgba(11, 165, 160, 0.28); border: 1px solid rgba(11, 165, 160, 0.5); border-radius: 2px; }
  .sw-peer { background: var(--color-accent); border-radius: 50%; }
  .scene-hint {
    position: absolute; right: 14px; bottom: 12px;
    font-size: 10.5px; color: var(--color-text-3);
    pointer-events: none;
  }
</style>
