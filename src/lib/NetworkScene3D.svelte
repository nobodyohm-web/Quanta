<script lang="ts">
  // Le réseau en 3D — particules RÉELLES de ce qui se passe en direct.
  //
  // Contrat zéro-fake :
  //   · particule teal sortante  = une enveloppe réellement signée (ML-DSA)
  //   · particule teal entrante  = une enveloppe réellement vérifiée (pipeline ①-⑧)
  //   · cristal qui se condense  = un bloc réellement scellé (event block-sealed)
  //   · fontaine                 = une récompense réellement minée (quanta://mined)
  //   · anneau au sol            = un snapshot disque réel (kind persist)
  //   · sphères en orbite        = les pairs mesurés (get_peer_metrics)
  //   · le courant du tore       = la présence du nœud (ambiance, légendé comme tel)
  //
  // Hygiène anti-gel (leçons des refontes précédentes) :
  //   · WebGL2 pur, zéro dépendance ; mouvement 100 % GPU (position = f(temps) en
  //     vertex shader) — le CPU n'écrit que quelques slots par événement réel
  //   · AUCUNE écriture de $state dans la boucle rAF ; les événements Tauri
  //     poussent dans une file simple, drainée par la frame suivante
  //   · pause hors-viewport + onglet caché ; prefers-reduced-motion = image fixe
  //     rafraîchie aux événements ; DPR plafonné à 2 ; perte de contexte gérée
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { t } from "./i18n.svelte";
  import { note, alertDiag } from "./diag";

  let { peerCount = 0, blocks = [], finalityFloor = 0 } = $props<{
    peerCount?: number;
    // Blocs récents RÉELS (du même sondage que la bande 2D) — nourrissent
    // l'hélice de cristaux : la blockchain qui se forme sous les yeux.
    blocks?: { index: number }[];
    finalityFloor?: number;
  }>();

  // Pont props → boucle GL (variables simples, lues en rAF hors tracking).
  let pendingCrystals: { index: number }[] = [];
  let crystalsDirty = true;
  let headTarget = -1;
  let floorTarget = 0;
  $effect(() => {
    pendingCrystals = blocks ?? [];
    crystalsDirty = true;
    const h = pendingCrystals[0]?.index;
    if (typeof h === "number") headTarget = h;
  });
  $effect(() => { floorTarget = finalityFloor ?? 0; });

  let canvas = $state<HTMLCanvasElement>();
  let wrap = $state<HTMLDivElement>();
  let glOk = $state(true);
  // Génération du contexte GL : incrémentée à chaque perte de contexte →
  // {#key} recrée le canvas et l'effet repart sur un contexte frais (sans ça,
  // une perte de contexte figeait la scène sur sa dernière image, à jamais).
  let glGen = $state(0);
  // Compteurs coarse pour la légende (mis à jour 1×/s hors boucle de rendu).
  let evtCount = $state(0);
  let lastEvtAgo = $state(-1);

  // ─── File d'événements réels (remplie par les listeners, drainée par rAF) ──
  type Spawn = { kind: number; n: number };
  const queue: Spawn[] = [];
  let realEvents = 0;
  let lastEvtAt = 0;

  // Kinds (miroir exact du shader)
  const K_AMBIENT = 0, K_OUT = 1, K_IN = 2, K_SEAL = 3, K_MINE = 4, K_RING = 5, K_PEER = 6, K_BLOCK = 7;

  const MAX = 4096;          // slots de particules événementielles
  const AMBIENT = 900;       // courant du tore (présence du nœud)
  const CRYSTALS = 64;       // hélice des blocs réels (34 affichés max)
  const FLOATS = 7;          // aSeed, aKind, aBirth, aTtl, aDir.xyz

  function pushSpawn(kind: number, n: number) {
    realEvents++;
    lastEvtAt = performance.now();
    if (queue.length < 64) queue.push({ kind, n });
  }

  // ─── Listeners Tauri — chaque burst provient d'un événement réel ──────────
  $effect(() => {
    const unsubs: UnlistenFn[] = [];
    let alive = true;
    (async () => {
      const u1 = await listen<{ kind?: string }>("quanta://engine", (e) => {
        const k = e.payload?.kind;
        if (k === "sign") pushSpawn(K_OUT, 26);
        else if (k === "verify") pushSpawn(K_IN, 26);
        else if (k === "persist") pushSpawn(K_RING, 40);
        else if (k === "elect") pushSpawn(K_RING, 14);
      });
      const u2 = await listen("quanta://mined", () => pushSpawn(K_MINE, 90));
      const u3 = await listen("quanta://block-sealed", () => pushSpawn(K_SEAL, 130));
      const u4 = await listen<{ tx_type?: string }>("quanta://tx-applied", (e) => {
        if (e.payload?.tx_type === "Transfer") pushSpawn(K_OUT, 18);
      });
      if (!alive) { u1(); u2(); u3(); u4(); return; }
      unsubs.push(u1, u2, u3, u4);
    })().catch(() => {});
    return () => { alive = false; unsubs.forEach((u) => u()); };
  });

  // Légende coarse (1 Hz, hors boucle de rendu — jamais de $state à 60 fps).
  $effect(() => {
    const iv = setInterval(() => {
      evtCount = realEvents;
      lastEvtAgo = lastEvtAt ? Math.round((performance.now() - lastEvtAt) / 1000) : -1;
    }, 1000);
    return () => clearInterval(iv);
  });

  // ─── Scène WebGL2 ─────────────────────────────────────────────────────────
  $effect(() => {
    // Capture locale : bind:this est prêt après le mount ; `cv` garde un type
    // définitif dans toutes les fermetures de l'effet.
    const cv = canvas, wr = wrap;
    if (!cv) return;
    // Balise de cycle de vie (directe → ui-diag.log) : mount / première frame.
    alertDiag("gl-mount", `gen=${glGen}`);
    let firstFrame = true;
    const gl = cv.getContext("webgl2", { alpha: true, antialias: true, premultipliedAlpha: true });
    if (!gl) { glOk = false; note("gl", "webgl2 indisponible"); return; }

    const VS = `#version 300 es
    precision highp float;
    layout(location=0) in float aSeed;
    layout(location=1) in float aKind;
    layout(location=2) in float aBirth;
    layout(location=3) in float aTtl;
    layout(location=4) in vec3 aDir;
    uniform float uTime; uniform mat4 uVP; uniform float uDpr;
    uniform float uHead;   // tête de chaîne animée (ressort CPU)
    uniform float uFloor;  // plancher de finalité (index)
    out float vKind; out float vLife; out float vSeed; out float vAux;
    // Tore de la marque : R majeur 1.0, r mineur 0.38, léger tilt.
    vec3 torusPoint(float u, float v) {
      float R = 1.0, r = 0.38;
      vec3 p = vec3((R + r*cos(v))*cos(u), r*sin(v), (R + r*cos(v))*sin(u));
      float tilt = 0.42;
      return vec3(p.x, p.y*cos(tilt) - p.z*sin(tilt), p.y*sin(tilt) + p.z*cos(tilt));
    }
    float hash(float n) { return fract(sin(n)*43758.5453); }
    void main() {
      float age = uTime - aBirth;
      float life = (aTtl <= 0.0) ? 0.5 : clamp(age / aTtl, 0.0, 1.0);
      vec3 pos = vec3(0.0);
      float size = 2.0;
      vAux = 0.0;
      int k = int(aKind + 0.5);
      if (k == 0) { // AMBIENT — courant du tore, dérive lente perpétuelle
        float u = aSeed*6.28318 + uTime*0.05 + hash(aSeed*7.0)*0.3;
        float v = hash(aSeed*3.0)*6.28318 + uTime*0.11;
        pos = torusPoint(u, v);
        size = 1.4 + hash(aSeed*9.0)*1.3;
        life = 0.5;
      } else if (k == 1) { // OUT — enveloppe signée : du cœur vers le large
        float e = 1.0 - pow(1.0 - life, 2.4);
        pos = aDir * (0.12 + e*2.15);
        size = 2.6*(1.0 - life*0.55);
      } else if (k == 2) { // IN — enveloppe vérifiée : du large vers le cœur
        float e = 1.0 - pow(1.0 - life, 2.0);
        pos = aDir * (2.25*(1.0 - e) + 0.10);
        size = 2.6*(1.0 - life*0.35);
      } else if (k == 3) { // SEAL — condensation puis ABSORPTION dans le cristal naissant
        vec3 target = vec3(0.0, 0.62, 0.0);
        if (life < 0.45) {
          float e = 1.0 - pow(1.0 - life/0.45, 2.2);
          pos = mix(aDir*1.9, target + aDir*0.06, e);
          size = 2.2 + e*1.6;
        } else {
          float e2 = (life - 0.45)/0.55;
          // Les particules s'effondrent DANS le bloc qui vient de naître.
          pos = target + aDir*0.06*(1.0 - e2*e2);
          size = 3.8*(1.0 - e2*0.85);
        }
      } else if (k == 4) { // MINE — fontaine de la récompense
        float g = life*life*2.6;
        pos = vec3(aDir.x*life*1.15, 0.15 + abs(aDir.y)*life*2.3 - g*0.55, aDir.z*life*1.15);
        size = 2.8*(1.0 - life*0.6);
      } else if (k == 5) { // RING — onde au sol (snapshot disque / élection)
        float e = 1.0 - pow(1.0 - life, 1.8);
        float ang = aSeed*6.28318;
        pos = vec3(cos(ang)*(0.25 + e*1.9), -0.55, sin(ang)*(0.25 + e*1.9));
        size = 2.0*(1.0 - life*0.8);
      } else if (k == 6) { // PEER — sphère en orbite elliptique (pair réel)
        float ang = aSeed*6.28318 + uTime*(0.05 + hash(aSeed*11.0)*0.03);
        float rad = 1.55 + hash(aSeed*5.0)*0.55;
        pos = vec3(cos(ang)*rad, sin(ang*0.7)*0.30, sin(ang)*rad*0.82);
        size = 7.0;
        life = 0.5;
      } else { // BLOCK — cristal d'un bloc RÉEL : l'hélice de la chaîne
        float t = uHead - aDir.x;          // 0 = le bloc qui vient d'être scellé
        if (t < -0.5 || t > 34.0) {
          pos = vec3(0.0); size = 0.0; life = 0.0;
        } else {
          float tc = max(t, 0.0);
          float ang2 = 0.46*tc + 1.25;
          float rad2 = 1.50 + tc*0.018;
          vec3 hp = vec3(cos(ang2)*rad2, 0.58 - tc*0.150, sin(ang2)*rad2*0.85);
          hp.y += sin(uTime*0.6 + aDir.x*0.7)*0.012;          // respiration
          // Naît au POINT DE FORGE (où les particules du scellement convergent)
          // puis glisse vers sa place dans l'hélice.
          float pop = clamp((uTime - aBirth)/0.9, 0.0, 1.0);
          float e3 = 1.0 - pow(1.0 - pop, 3.0);
          pos = mix(vec3(0.0, 0.62, 0.0), hp, e3);
          size = (24.0 - tc*0.42) * (0.6 + 0.4*e3);
          life = (aDir.x <= uFloor + 0.5) ? 1.0 : 0.0;        // 1 = finalisé
          vAux = 1.0 - clamp(tc/2.5, 0.0, 1.0);              // fraîcheur (éclat)
        }
      }
      vKind = aKind; vLife = life; vSeed = aSeed;
      vec4 clip = uVP * vec4(pos, 1.0);
      gl_Position = clip;
      float dead = (aTtl > 0.0 && (age < 0.0 || age > aTtl)) ? 0.0 : 1.0;
      gl_PointSize = size * uDpr * dead * clamp(4.2/clip.w, 0.5, 3.2);
    }`;

    const FS = `#version 300 es
    precision mediump float;
    in float vKind; in float vLife; in float vSeed; in float vAux;
    // highp EXPLICITE : le VS déclare uTime en highp (précision par défaut de
    // son étage) — une précision différente ici = échec de LINK silencieux.
    uniform highp float uTime;
    out vec4 frag;
    void main() {
      int k = int(vKind + 0.5);
      vec3 teal = vec3(0.043, 0.647, 0.627);   // #0BA5A0
      vec3 deep = vec3(0.031, 0.498, 0.549);   // #087F8C
      vec3 ink  = vec3(0.114, 0.114, 0.122);   // #1d1d1f
      if (k == 7) {
        // Cristal de bloc : losange facetté en rotation lente.
        // Finalisé (vLife=1) = pierre teal massive ; en attente = verre givré.
        float ang = uTime*0.22 + vSeed*0.9;
        mat2 R = mat2(cos(ang), -sin(ang), sin(ang), cos(ang));
        vec2 p = R * (gl_PointCoord - vec2(0.5));
        float d7 = abs(p.x) + abs(p.y);
        if (d7 > 0.47) discard;
        float body = smoothstep(0.47, 0.42, d7);
        float rim  = smoothstep(0.36, 0.45, d7);
        float facet = 0.72 + 0.28*smoothstep(-0.18, 0.22, p.x + p.y*0.6);
        vec3 stone = deep;
        vec3 frost = vec3(0.62, 0.86, 0.84);
        vec3 col7 = mix(frost, stone, vLife);
        col7 = mix(col7, vec3(0.078, 0.784, 0.722), vAux*0.65);   // éclat du neuf
        float a7 = mix(0.42, 0.94, vLife);
        a7 = (a7 + rim*0.25) * body * (0.85 + vAux*0.15);
        frag = vec4(col7*facet*a7, a7);
        return;
      }
      vec2 d = gl_PointCoord - vec2(0.5);
      float r = length(d);
      if (r > 0.5) discard;
      float soft = smoothstep(0.5, 0.12, r);
      vec3 col; float a;
      if (k == 0)      { col = ink;  a = 0.10*soft; }
      else if (k == 3) { col = deep; a = (0.85 - vLife*0.5)*soft; }
      else if (k == 4) { col = vec3(0.078, 0.784, 0.722); a = (0.9 - vLife*0.6)*soft; }
      else if (k == 5) { col = deep; a = (0.5 - vLife*0.45)*soft; }
      else if (k == 6) {
        float ring = smoothstep(0.5, 0.42, r) - smoothstep(0.30, 0.20, r);
        col = teal; a = max(ring*0.9, soft*0.12);
      }
      else             { col = teal; a = (0.8 - vLife*0.5)*soft; }
      frag = vec4(col*a, a); // premultiplié
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
    const vs = compile(gl.VERTEX_SHADER, VS);
    const fs = compile(gl.FRAGMENT_SHADER, FS);
    if (!vs || !fs) { glOk = false; return; }
    const prog = gl.createProgram()!;
    gl.attachShader(prog, vs); gl.attachShader(prog, fs); gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      // Un échec de link était SILENCIEUX (glOk=false et rien d'autre) —
      // maintenant il se voit dans l'anneau et le rapport de gel suivant.
      alertDiag("gl-link", (gl.getProgramInfoLog(prog) ?? "?").slice(0, 200));
      glOk = false;
      return;
    }
    gl.useProgram(prog);
    const uTime = gl.getUniformLocation(prog, "uTime");
    const uVP = gl.getUniformLocation(prog, "uVP");
    const uDpr = gl.getUniformLocation(prog, "uDpr");
    const uHead = gl.getUniformLocation(prog, "uHead");
    const uFloor = gl.getUniformLocation(prog, "uFloor");

    // ── VBO : ambiance + événements + pairs + cristaux de blocs, un seul buffer ──
    const TOTAL = AMBIENT + MAX + 64 + CRYSTALS;
    const data = new Float32Array(TOTAL * FLOATS);
    for (let i = 0; i < AMBIENT; i++) {
      const o = i * FLOATS;
      data[o] = Math.random(); data[o + 1] = K_AMBIENT; data[o + 2] = 0; data[o + 3] = 0;
    }
    const vbo = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, vbo);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW);
    const stride = FLOATS * 4;
    gl.vertexAttribPointer(0, 1, gl.FLOAT, false, stride, 0);
    gl.vertexAttribPointer(1, 1, gl.FLOAT, false, stride, 4);
    gl.vertexAttribPointer(2, 1, gl.FLOAT, false, stride, 8);
    gl.vertexAttribPointer(3, 1, gl.FLOAT, false, stride, 12);
    gl.vertexAttribPointer(4, 3, gl.FLOAT, false, stride, 16);
    for (let l = 0; l < 5; l++) gl.enableVertexAttribArray(l);

    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.clearColor(0, 0, 0, 0);

    // ── Spawn CPU : n'écrit que quelques slots par événement réel ──
    let cursor = 0;
    const tmp = new Float32Array(FLOATS);
    function spawnBurst(kind: number, n: number, now: number) {
      for (let i = 0; i < n; i++) {
        const th = Math.random() * Math.PI * 2;
        const ph = Math.acos(2 * Math.random() - 1);
        tmp[0] = Math.random(); tmp[1] = kind;
        tmp[2] = now; tmp[3] = kind === K_SEAL ? 3.4 : kind === K_MINE ? 2.2 : kind === K_RING ? 1.6 : 1.9;
        tmp[4] = Math.sin(ph) * Math.cos(th);
        tmp[5] = kind === K_RING ? 0 : Math.cos(ph) * 0.7;
        tmp[6] = Math.sin(ph) * Math.sin(th);
        gl!.bufferSubData(gl!.ARRAY_BUFFER, (AMBIENT + cursor) * stride, tmp);
        cursor = (cursor + 1) % MAX;
      }
    }

    // ── Pairs réels : slots dédiés, mis à jour quand le compte change ──
    let lastPeerCount = -1;
    function syncPeers(count: number) {
      if (count === lastPeerCount) return;
      lastPeerCount = count;
      const buf = new Float32Array(64 * FLOATS);
      for (let i = 0; i < 64; i++) {
        const o = i * FLOATS;
        if (i < Math.min(count, 64)) {
          buf[o] = (i + 1) / Math.max(count, 1); buf[o + 1] = K_PEER; buf[o + 2] = 0; buf[o + 3] = 0;
        } else {
          // Slot éteint : ttl minuscule déjà expiré → gl_PointSize = 0.
          buf[o + 1] = K_OUT; buf[o + 2] = -1e9; buf[o + 3] = 0.001;
        }
      }
      gl!.bufferSubData(gl!.ARRAY_BUFFER, (AMBIENT + MAX) * stride, buf);
    }

    // ── Cristaux : les blocs RÉELS de la chaîne (mis à jour au sondage) ──
    const CRYSTAL_BASE = AMBIENT + MAX + 64;
    const crystalBuf = new Float32Array(CRYSTALS * FLOATS);
    let knownBirth = new Map<number, number>();
    let headCur = -1e9; // saute à la première synchro (pas d'animation fantôme)
    function syncCrystals(list: { index: number }[], now: number) {
      crystalBuf.fill(0);
      for (let i = 0; i < CRYSTALS; i++) {
        const o = i * FLOATS;
        const b = list[i];
        if (!b || i >= 34) {
          // Slot éteint (même pattern que les pairs).
          crystalBuf[o + 1] = K_OUT; crystalBuf[o + 2] = -1e9; crystalBuf[o + 3] = 0.001;
          continue;
        }
        let birth = knownBirth.get(b.index);
        if (birth === undefined) {
          // Nouveau pour la scène : le bloc de tête naît au point de forge ;
          // les blocs d'historique (premier chargement) arrivent déjà posés.
          birth = i === 0 && knownBirth.size > 0 ? now : now - 10;
          knownBirth.set(b.index, birth);
        }
        crystalBuf[o] = b.index;          // aSeed → rotation propre
        crystalBuf[o + 1] = K_BLOCK;
        crystalBuf[o + 2] = birth;
        crystalBuf[o + 3] = 0;            // éternel
        crystalBuf[o + 4] = b.index;      // aDir.x → position dans l'hélice
      }
      if (knownBirth.size > 512) knownBirth = new Map(); // borne mémoire
      gl!.bufferSubData(gl!.ARRAY_BUFFER, CRYSTAL_BASE * stride, crystalBuf);
    }

    // ── Caméra : orbite douce, glisser pour tourner — variables simples ──
    let yaw = 0.6, pitch = 0.36, dist = 4.6;
    let tYaw = yaw, tPitch = pitch;
    let dragging = false, lx = 0, ly = 0, userHold = 0;
    const onDown = (e: PointerEvent) => { dragging = true; lx = e.clientX; ly = e.clientY; };
    const onMove = (e: PointerEvent) => {
      if (!dragging) return;
      tYaw += (e.clientX - lx) * 0.008; tPitch = Math.max(0.05, Math.min(1.25, tPitch + (e.clientY - ly) * 0.006));
      lx = e.clientX; ly = e.clientY; userHold = performance.now();
    };
    const onUp = () => { dragging = false; };
    cv.addEventListener("pointerdown", onDown);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);

    const proj = new Float32Array(16), vp = new Float32Array(16);
    function computeVP(w: number, h: number, now: number) {
      if (now - userHold > 4000) tYaw += 0.0007; // orbite lente, hypnotique
      yaw += (tYaw - yaw) * 0.06; pitch += (tPitch - pitch) * 0.06;
      // Respiration de la caméra : très lent travelling avant/arrière.
      dist = 4.6 + Math.sin(now * 0.00022) * 0.24;
      const f = 1 / Math.tan(0.45), asp = w / h, near = 0.1, far = 30;
      proj.fill(0);
      proj[0] = f / asp; proj[5] = f; proj[10] = (far + near) / (near - far); proj[11] = -1;
      proj[14] = (2 * far * near) / (near - far);
      const cx = Math.cos(yaw) * Math.cos(pitch) * dist;
      const cy = Math.sin(pitch) * dist;
      const cz = Math.sin(yaw) * Math.cos(pitch) * dist;
      // lookAt(origine) hand-rolled
      const zx = cx, zy = cy, zz = cz; const zl = Math.hypot(zx, zy, zz);
      const Zx = zx / zl, Zy = zy / zl, Zz = zz / zl;
      const Xx = -Zz, Xy = 0, Xz = Zx; const xl = Math.hypot(Xx, Xy, Xz) || 1;
      const xX = Xx / xl, xY = Xy / xl, xZ = Xz / xl;
      const Yx = Zy * xZ - Zz * xY, Yy = Zz * xX - Zx * xZ, Yz = Zx * xY - Zy * xX;
      const view = [
        xX, Yx, Zx, 0,
        xY, Yy, Zy, 0,
        xZ, Yz, Zz, 0,
        -(xX * cx + xY * cy + xZ * cz), -(Yx * cx + Yy * cy + Yz * cz), -(Zx * cx + Zy * cy + Zz * cz), 1,
      ];
      // vp = proj × view (colonne-major)
      for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) {
        vp[c * 4 + r] = proj[r] * view[c * 4] + proj[4 + r] * view[c * 4 + 1] + proj[8 + r] * view[c * 4 + 2] + proj[12 + r] * view[c * 4 + 3];
      }
    }

    // ── Boucle : pause hors-viewport / onglet caché ; reduced-motion = statique ──
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    let raf = 0, visible = true, inView = true, running = false;
    // La DERNIÈRE entrée du lot fait foi : sous un défilement rapide, le lot
    // arrive [sorti, revenu] — lire es[0] laissait inView bloqué à faux et la
    // scène gelée À L'ÉCRAN (boucle stoppée, dernière image affichée).
    const io = new IntersectionObserver((es) => {
      inView = es[es.length - 1]?.isIntersecting ?? true; ensure();
    }, { threshold: 0.05 });
    io.observe(cv);
    const onVis = () => { visible = !document.hidden; ensure(); };
    document.addEventListener("visibilitychange", onVis);

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    function frame() {
      raf = 0;
      if (!cv) return; // TS : le narrowing du const ne traverse pas la closure
      if (!(visible && inView)) { running = false; return; }
      const w = wr?.clientWidth || 880, h = 400;
      if (cv.width !== Math.round(w * dpr) || cv.height !== Math.round(h * dpr)) {
        cv.width = Math.round(w * dpr); cv.height = Math.round(h * dpr);
      }
      gl!.viewport(0, 0, cv.width, cv.height);
      const t0 = performance.now();
      const now = t0 / 1000;
      // Budget de spawn par frame : jamais plus de 400 particules écrites
      // d'un coup — une rafale d'événements s'étale sur quelques frames au
      // lieu de bloquer la frame du scellement.
      let budget = 400;
      while (queue.length && budget > 0) {
        const s = queue[0];
        const take = Math.min(s.n, budget);
        spawnBurst(s.kind, take, now);
        s.n -= take;
        budget -= take;
        if (s.n <= 0) queue.shift();
      }
      syncPeers(peerCount);
      // Cristaux : synchro sur nouveau sondage + ressort de tête de chaîne —
      // à chaque bloc, TOUTE l'hélice glisse d'un cran, en douceur.
      if (crystalsDirty) { crystalsDirty = false; syncCrystals(pendingCrystals, now); }
      if (headTarget >= 0) {
        if (Math.abs(headTarget - headCur) > 40) headCur = headTarget - 1.5;
        headCur += (headTarget - headCur) * 0.035;
      }
      computeVP(cv.width, cv.height, performance.now());
      gl!.clear(gl!.COLOR_BUFFER_BIT);
      gl!.uniform1f(uTime, now);
      gl!.uniform1f(uDpr, dpr);
      gl!.uniform1f(uHead, headCur);
      gl!.uniform1f(uFloor, floorTarget);
      gl!.uniformMatrix4fv(uVP, false, vp);
      gl!.drawArrays(gl!.POINTS, 0, TOTAL);
      // Battement de scène : la forensique par bloc lit cet horodatage —
      // « la scène était-elle VIVANTE au scellement ? » devient mesurable.
      (window as unknown as { __quantaSceneBeat?: number }).__quantaSceneBeat = performance.now();
      if (firstFrame) { firstFrame = false; alertDiag("gl-frame1", `${Math.round(performance.now() - t0)}ms`); }
      // Coût de frame anormal → dans l'anneau (visible en forensique).
      const cost = performance.now() - t0;
      if (cost > 40) note("gl-frame", `${Math.round(cost)}ms`);
      if (!reduced) raf = requestAnimationFrame(frameSafe);
      else running = false;
    }
    // La boucle ne doit JAMAIS mourir en silence : toute exception dans frame()
    // tuait la chaîne rAF (running restait true → ensure() ne relançait plus)
    // et la scène gelait sur sa dernière image. Capture + trace + relance.
    const frameSafe = () => {
      try {
        frame();
      } catch (err) {
        // Canal direct : une exception de frame évincée de l'anneau nous a
        // déjà caché ce bug — maintenant elle arrive en clair dans ui-diag.log.
        alertDiag("gl-frame-err", `${(err as Error)?.stack ?? String(err)}`.slice(0, 300));
        running = false;
        raf = 0;
      }
    };
    function ensure() {
      if (!running && visible && inView && !raf) { running = true; raf = requestAnimationFrame(frameSafe); }
    }
    // reduced-motion : une frame par événement réel (file drainée), pas de boucle.
    let evIv: ReturnType<typeof setInterval> | undefined;
    if (reduced) evIv = setInterval(() => { if (queue.length) ensure(); }, 500);
    // Auto-résurrection : si la scène est visible mais qu'aucune frame n'est
    // sortie depuis > 2 s (boucle morte, quelle qu'en soit la cause), on
    // le note, on force la relance — et la forensique du bloc suivant le dira.
    const heal = setInterval(() => {
      if (reduced) return;
      // Vérité géométrique directe : si le canvas est réellement à l'écran
      // mais qu'un état stale (inView faux à tort) tient la boucle arrêtée,
      // on répare l'état AVANT de relancer.
      if (!inView) {
        const r = cv.getBoundingClientRect();
        if (r.bottom > 0 && r.top < window.innerHeight && r.width > 0) {
          note("gl-inview-réparé", "état stale corrigé");
          inView = true;
        }
      }
      if (!(visible && inView)) return;
      const b = (window as unknown as { __quantaSceneBeat?: number }).__quantaSceneBeat ?? 0;
      if (b > 0 && performance.now() - b > 2000) {
        note("gl-résurrection", `${Math.round(performance.now() - b)}ms sans frame`);
        if (raf) cancelAnimationFrame(raf);
        raf = 0;
        running = false;
        ensure();
      }
    }, 1000);
    ensure();

    // Perte de contexte GL (redémarrage du process GPU, pression mémoire…) :
    // AVANT, la scène restait figée sur sa dernière image pour toujours.
    // Maintenant : on recrée canvas + contexte via {#key glGen}.
    const onLost = (e: Event) => {
      e.preventDefault();
      note("gl", "contexte perdu — recréation");
      glGen += 1;
    };
    cv.addEventListener("webglcontextlost", onLost);

    return () => {
      if (raf) cancelAnimationFrame(raf);
      if (evIv) clearInterval(evIv);
      clearInterval(heal);
      // Scène démontée : battement marqué absent (la forensique dira « absente »).
      (window as unknown as { __quantaSceneBeat?: number }).__quantaSceneBeat = -1;
      io.disconnect();
      document.removeEventListener("visibilitychange", onVis);
      cv.removeEventListener("pointerdown", onDown);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      cv.removeEventListener("webglcontextlost", onLost);
      gl.deleteBuffer(vbo); gl.deleteProgram(prog);
      gl.deleteShader(vs); gl.deleteShader(fs);
    };
  });
</script>

<div class="scene-wrap" bind:this={wrap}>
  {#if glOk}
    {#key glGen}
      <canvas bind:this={canvas} class="scene-canvas" aria-label={t('net3d.aria')}></canvas>
    {/key}
    <div class="scene-legend">
      <span class="lg"><span class="sw sw-final"></span>{t('net3d.legFinal')}</span>
      <span class="lg"><span class="sw sw-frost"></span>{t('net3d.legPending')}</span>
      <span class="lg"><span class="sw sw-evt"></span>{t('net3d.legendEvent')}</span>
      <span class="lg"><span class="sw sw-peer"></span>{peerCount} {peerCount === 1 ? t('wallet.peer') : t('wallet.peers')}</span>
      <span class="lg lg-count">{evtCount.toLocaleString('fr-FR')} {t('net3d.evtReal')}{lastEvtAgo >= 0 ? ` · ${lastEvtAgo}s` : ''}</span>
    </div>
    {#if peerCount === 0}
      <div class="scene-solo">{t('net.canvasNoPeers')} — {t('net.canvasShareHint')}</div>
    {/if}
    <div class="scene-hint">{t('net3d.dragHint')}</div>
  {/if}
</div>

<style>
  .scene-wrap { position: relative; width: 100%; height: 400px; }
  .scene-canvas {
    display: block; width: 100%; height: 400px;
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
  .lg-count { font-variant-numeric: tabular-nums lining-nums; color: var(--color-text-1); font-weight: 600; }
  .sw { width: 8px; height: 8px; border-radius: 50%; }
  .sw-evt { background: var(--color-accent); }
  .sw-peer { background: var(--color-accent-hover); box-shadow: inset 0 0 0 2px #fff; border: 1px solid var(--color-accent-hover); }
  .sw-final { background: #087F8C; border-radius: 2px; transform: rotate(45deg); }
  .sw-frost { background: rgba(11,165,160,0.28); border: 1px solid rgba(11,165,160,0.5); border-radius: 2px; transform: rotate(45deg); }
  .scene-solo {
    position: absolute; left: 50%; top: 50%; transform: translate(-50%, 92px);
    font-size: 12px; color: var(--color-text-3);
    pointer-events: none; text-align: center; max-width: 70%;
  }
  .scene-hint {
    position: absolute; right: 14px; bottom: 12px;
    font-size: 10.5px; color: var(--color-text-3);
    pointer-events: none;
  }
</style>
