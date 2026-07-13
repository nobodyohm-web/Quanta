<script lang="ts">
  // MiningScene — the reactor. What mining IS, made visible:
  // a torus of energy where thousands of particles (the network's work)
  // circulate in continuous currents. When a reward lands (quanta://mined)
  // the flow SURGES and brightens; when a block is sealed (quanta://block-
  // sealed) a crystal block crystallizes at the core, spins once, and flies
  // off toward the chain. Drag to orbit — it keeps living on its own.
  //
  // Light-theme native (ink + jewel teal over white), GPU-side motion (all
  // particle animation in the vertex shader), battery-disciplined via the
  // shared shell (pauses off-viewport, static under reduced motion).
  import * as THREE from "three";
  import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
  import { RoundedBoxGeometry } from "three/examples/jsm/geometries/RoundedBoxGeometry.js";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { untrack } from "svelte";
  import { t } from "../i18n.svelte";
  import { createShell, softShadow, PALETTE } from "./scene";

  let { height = 260, peers = 0 } = $props<{ height?: number; peers?: number }>();

  let canvas = $state<HTMLCanvasElement | undefined>();
  let failed = $state(false);

  const VERT = `
    attribute float aTheta;   // major angle at t=0
    attribute float aPhi;     // tube angle at t=0
    attribute float aSpeed;   // individual toroidal speed
    attribute float aDrift;   // individual poloidal drift
    attribute float aJitter;  // radial jitter inside the tube
    attribute float aSeed;    // 0..1 lottery (energized share) + phase
    uniform float uTime;
    uniform float uSurge;       // 0..1 reward surge
    uniform float uEnergShare;  // fraction of energized (teal) particles
    uniform float uPixelRatio;
    varying float vEnerg;
    varying float vBoost;
    void main() {
      float speed = aSpeed * (1.0 + uSurge * 2.2);
      float theta = aTheta + uTime * speed;
      float phi = aPhi + uTime * aDrift + sin(uTime * 0.7 + aSeed * 6.283) * 0.15;
      float R = 1.15;
      float r = 0.40 * aJitter;
      float w = R + r * cos(phi);
      vec3 p = vec3(w * cos(theta), r * sin(phi), w * sin(theta));
      // breathing
      p *= 1.0 + 0.01 * sin(uTime * 0.9 + aSeed * 6.283);
      vec4 mv = modelViewMatrix * vec4(p, 1.0);
      gl_Position = projectionMatrix * mv;
      vEnerg = step(1.0 - uEnergShare, aSeed);
      vBoost = uSurge;
      float size = mix(2.1, 3.6, vEnerg) * (1.0 + uSurge * 0.9);
      gl_PointSize = size * uPixelRatio * (3.4 / -mv.z);
    }
  `;
  const FRAG = `
    precision mediump float;
    varying float vEnerg;
    varying float vBoost;
    void main() {
      vec2 c = gl_PointCoord - 0.5;
      float d = length(c);
      if (d > 0.5) discard;
      float soft = smoothstep(0.5, 0.15, d);
      vec3 ink = vec3(0.34, 0.33, 0.31);
      vec3 teal = vec3(0.043, 0.647, 0.627);
      vec3 bright = vec3(0.078, 0.784, 0.722);
      vec3 col = mix(ink, teal, vEnerg);
      col = mix(col, bright, vBoost * (0.35 + 0.65 * vEnerg));
      float alpha = soft * mix(0.30, 0.85, max(vEnerg, vBoost * 0.6));
      gl_FragColor = vec4(col, alpha);
    }
  `;

  interface Crystal {
    mesh: THREE.Mesh;
    born: number;
  }

  $effect(() => {
    const cv = canvas;
    if (!cv) return;
    const shell = createShell(cv, { fov: 40, z: 4.35, y: 1.05 });
    if (!shell) { failed = true; return; }
    // Narrowed alias — TS narrowing does not flow into hoisted `function`s.
    const sh = shell;
    const { scene, camera, renderer } = shell;

    // ── Lights (soft, Apple-like on white) ──
    scene.add(new THREE.HemisphereLight(0xffffff, 0xe9e4da, 1.15));
    const sun = new THREE.DirectionalLight(0xffffff, 1.1);
    sun.position.set(2.5, 4, 3);
    scene.add(sun);

    // ── Torus guides — two whisper-thin ink rings ──
    const guideMat = new THREE.MeshBasicMaterial({
      color: PALETTE.ink, transparent: true, opacity: 0.16,
    });
    const g1 = new THREE.Mesh(new THREE.TorusGeometry(1.15, 0.005, 6, 200), guideMat);
    g1.rotation.x = Math.PI / 2;
    scene.add(g1);
    const g2 = new THREE.Mesh(
      new THREE.TorusGeometry(1.15, 0.0035, 6, 200),
      new THREE.MeshBasicMaterial({ color: PALETTE.teal, transparent: true, opacity: 0.22 }),
    );
    g2.rotation.x = Math.PI / 2;
    g2.scale.setScalar(1.06);
    scene.add(g2);

    // ── Particle current (all motion on the GPU) ──
    const N = 3600;
    const theta = new Float32Array(N);
    const phi = new Float32Array(N);
    const speed = new Float32Array(N);
    const drift = new Float32Array(N);
    const jitter = new Float32Array(N);
    const seed = new Float32Array(N);
    for (let i = 0; i < N; i++) {
      theta[i] = Math.random() * Math.PI * 2;
      phi[i] = Math.random() * Math.PI * 2;
      speed[i] = 0.12 + Math.random() * 0.3;
      drift[i] = (Math.random() - 0.5) * 0.5;
      jitter[i] = 0.55 + Math.random() * 0.75;
      seed[i] = Math.random();
    }
    const geo = new THREE.BufferGeometry();
    // three needs a `position` attribute even if the shader recomputes it.
    geo.setAttribute("position", new THREE.BufferAttribute(new Float32Array(N * 3), 3));
    geo.setAttribute("aTheta", new THREE.BufferAttribute(theta, 1));
    geo.setAttribute("aPhi", new THREE.BufferAttribute(phi, 1));
    geo.setAttribute("aSpeed", new THREE.BufferAttribute(speed, 1));
    geo.setAttribute("aDrift", new THREE.BufferAttribute(drift, 1));
    geo.setAttribute("aJitter", new THREE.BufferAttribute(jitter, 1));
    geo.setAttribute("aSeed", new THREE.BufferAttribute(seed, 1));
    geo.boundingSphere = new THREE.Sphere(new THREE.Vector3(), 2.2);
    const uniforms = {
      uTime: { value: 0 },
      uSurge: { value: 0 },
      uEnergShare: { value: 0.08 },
      uPixelRatio: { value: Math.min(2, window.devicePixelRatio || 1) },
    };
    const points = new THREE.Points(
      geo,
      new THREE.ShaderMaterial({
        vertexShader: VERT,
        fragmentShader: FRAG,
        uniforms,
        transparent: true,
        depthWrite: false,
      }),
    );
    scene.add(points);

    // ── Soft ground shadow ──
    const shadow = softShadow();
    shadow.position.set(0, -1.02, 0);
    shadow.scale.set(3.1, 0.9, 1);
    scene.add(shadow);

    // ── Orbit (drag to explore; auto-rotates when idle) ──
    const controls = new OrbitControls(camera, cv);
    controls.enableZoom = false;
    controls.enablePan = false;
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;
    controls.autoRotate = !shell.reduced;
    controls.autoRotateSpeed = 0.7;
    controls.minPolarAngle = Math.PI * 0.22;
    controls.maxPolarAngle = Math.PI * 0.62;
    let idleAt = 0;
    controls.addEventListener("start", () => { controls.autoRotate = false; });
    controls.addEventListener("end", () => { idleAt = performance.now(); });

    // ── The block crystal (born at seal, flies off to the chain) ──
    const crystalGeo = new RoundedBoxGeometry(0.34, 0.34, 0.34, 4, 0.07);
    const crystals: Crystal[] = [];
    function birthCrystal() {
      const mat = new THREE.MeshPhysicalMaterial({
        color: PALETTE.teal,
        roughness: 0.22,
        metalness: 0.1,
        clearcoat: 0.7,
        clearcoatRoughness: 0.3,
        transparent: true,
        opacity: 0.95,
      });
      const mesh = new THREE.Mesh(crystalGeo, mat);
      mesh.position.set(0, 0.05, 0);
      mesh.scale.setScalar(0.001);
      scene.add(mesh);
      crystals.push({ mesh, born: performance.now() });
      if (sh.reduced) sh.renderOnce();
    }

    // ── Live events feed the scene directly ──
    let surgeTarget = 0;
    const unsubs: UnlistenFn[] = [];
    let alive = true;
    (async () => {
      const u1 = await listen("quanta://mined", () => {
        surgeTarget = 1;
        if (sh.reduced) sh.renderOnce();
      });
      const u2 = await listen("quanta://block-sealed", () => birthCrystal());
      if (!alive) { u1(); u2(); return; }
      unsubs.push(u1, u2);
    })();

    // Peers → energized share (reactive prop read via tracked sub-effect).
    let peersLive = untrack(() => peers);
    const sub = $effect.root(() => {
      $effect(() => { peersLive = peers; });
    });

    shell.start((dt, tsec) => {
      uniforms.uTime.value = tsec;
      // surge: fast attack, gentle decay
      const s = uniforms.uSurge.value;
      uniforms.uSurge.value = surgeTarget > s
        ? Math.min(1, s + dt * 6)
        : Math.max(0, s - dt * 0.55);
      if (uniforms.uSurge.value >= 0.999) surgeTarget = 0;
      uniforms.uEnergShare.value = Math.min(0.55, 0.08 + Math.max(0, peersLive) * 0.05);
      // resume auto-rotation 6s after the user lets go
      if (!controls.autoRotate && idleAt && performance.now() - idleAt > 6000 && !shell.reduced) {
        controls.autoRotate = true;
        idleAt = 0;
      }
      controls.update();
      // crystals: pop in (0–0.5s), spin+hover (0.5–1.4s), fly right + fade (1.4–2.4s)
      const now = performance.now();
      for (let i = crystals.length - 1; i >= 0; i--) {
        const c = crystals[i];
        const age = (now - c.born) / 1000;
        const m = c.mesh;
        if (age < 0.5) {
          const p = age / 0.5;
          const e = 1 - Math.pow(1 - p, 3);
          m.scale.setScalar(0.001 + e * 1);
          m.rotation.y = p * 1.2;
        } else if (age < 1.4) {
          m.rotation.y += dt * 2.2;
          m.position.y = 0.05 + Math.sin((age - 0.5) * 3.5) * 0.05;
        } else if (age < 2.4) {
          const p = (age - 1.4) / 1.0;
          const e = p * p;
          m.position.x = e * 4.2;
          m.position.y = 0.05 + p * 0.5;
          m.rotation.y += dt * 3.5;
          (m.material as THREE.MeshPhysicalMaterial).opacity = 0.95 * (1 - p);
        } else {
          scene.remove(m);
          (m.material as THREE.Material).dispose();
          crystals.splice(i, 1);
        }
      }
    });

    return () => {
      alive = false;
      unsubs.forEach((u) => u());
      sub();
      controls.dispose();
      crystalGeo.dispose();
      shell.dispose();
      void renderer; // shell owns it
    };
  });
</script>

{#if !failed}
  <div class="mining-scene" style="height:{height}px;">
    <canvas bind:this={canvas} aria-label={t('scene.mining.aria')}></canvas>
  </div>
{:else}
  <div class="mining-scene scene-fallback" style="height:{height}px;" aria-hidden="true">
    <div class="sf-ring"></div>
  </div>
{/if}

<style>
  .mining-scene {
    position: relative;
    width: 100%;
    /* Soft teal mist behind the reactor — depth without darkness. */
    background:
      radial-gradient(75% 90% at 50% 42%, rgba(11, 165, 160, 0.07), transparent 70%),
      radial-gradient(50% 60% at 68% 30%, rgba(61, 111, 224, 0.05), transparent 70%);
  }
  .mining-scene canvas {
    display: block;
    width: 100%;
    height: 100%;
    cursor: grab;
  }
  .mining-scene canvas:active { cursor: grabbing; }
  .scene-fallback { display: flex; align-items: center; justify-content: center; }
  .sf-ring {
    width: 46%; max-width: 200px; aspect-ratio: 1;
    border-radius: 50%;
    border: 2px solid var(--cyan-mid);
    transform: rotateX(58deg);
  }
</style>
