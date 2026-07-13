<script lang="ts">
  // Torus3D — the living signature of the Torus protocol: a slowly revolving
  // torus of particles, drawn in raw WebGL (zero dependency, zero network).
  // Light-theme native: diluted-ink particles on white, jewel-teal "energized"
  // nodes whose share grows with connected peers, and a teal pulse wave that
  // sweeps the ring whenever a block is sealed (bump the `pulse` prop).
  //
  // Battery-friendly by construction: the RAF loop pauses when the tab is
  // hidden or the element leaves the viewport; `prefers-reduced-motion`
  // renders a single static frame.
  import { untrack } from "svelte";
  import { t } from "./i18n.svelte";

  let {
    height = 260,
    peers = 0,
    pulse = 0,
    density = 1,
  } = $props<{
    height?: number;
    peers?: number;
    pulse?: number;
    density?: number;
  }>();

  let canvas = $state<HTMLCanvasElement | undefined>();
  let failed = $state(false);

  const VERT = `
    attribute vec3 aPos;
    attribute vec3 aRand;   // x: energize lottery 0..1, y: phase, z: major angle
    uniform float uRotY;
    uniform float uRotX;
    uniform float uAspect;
    uniform float uTime;
    uniform float uEnergShare;
    uniform float uPulseAngle;
    uniform float uPulseStrength;
    uniform float uPointScale;
    varying float vEnerg;
    varying float vGlow;
    void main() {
      vec3 p = aPos;
      // Gentle breathing along the tube normal, per-particle phase.
      p *= 1.0 + 0.012 * sin(uTime * 0.8 + aRand.y * 6.2831);
      float cy = cos(uRotY), sy = sin(uRotY);
      p = vec3(cy * p.x + sy * p.z, p.y, -sy * p.x + cy * p.z);
      float cx = cos(uRotX), sx = sin(uRotX);
      p = vec3(p.x, cx * p.y - sx * p.z, sx * p.y + cx * p.z);
      float depth = p.z + 3.1;
      float persp = 1.85 / depth;
      gl_Position = vec4(p.x * persp / uAspect, p.y * persp, p.z * 0.05, 1.0);
      vEnerg = step(1.0 - uEnergShare, aRand.x);
      // Block pulse: brightness window around the wavefront's major angle.
      float d = abs(mod(aRand.z - uPulseAngle + 3.14159, 6.2831) - 3.14159);
      vGlow = uPulseStrength * smoothstep(0.9, 0.0, d);
      float base = mix(2.2, 3.4, vEnerg) + vGlow * 2.4;
      gl_PointSize = base * persp * uPointScale;
    }
  `;

  const FRAG = `
    precision mediump float;
    varying float vEnerg;
    varying float vGlow;
    void main() {
      vec2 c = gl_PointCoord - 0.5;
      float r = length(c);
      if (r > 0.5) discard;
      float soft = smoothstep(0.5, 0.18, r);
      // Diluted ink vs jewel teal (#0BA5A0), pulse pushes toward bright teal (#14C8B8).
      vec3 ink = vec3(0.36, 0.35, 0.33);
      vec3 teal = vec3(0.043, 0.647, 0.627);
      vec3 bright = vec3(0.078, 0.784, 0.722);
      vec3 col = mix(ink, teal, vEnerg);
      col = mix(col, bright, clamp(vGlow, 0.0, 1.0));
      float alpha = soft * mix(0.34, 0.9, max(vEnerg, vGlow));
      gl_FragColor = vec4(col, alpha);
    }
  `;

  function compile(gl: WebGLRenderingContext, type: number, src: string): WebGLShader | null {
    const sh = gl.createShader(type);
    if (!sh) return null;
    gl.shaderSource(sh, src);
    gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) return null;
    return sh;
  }

  $effect(() => {
    const cv = canvas;
    if (!cv) return;
    const gl = cv.getContext("webgl", { alpha: true, antialias: true, premultipliedAlpha: false });
    if (!gl) { failed = true; return; }

    const vs = compile(gl, gl.VERTEX_SHADER, VERT);
    const fs = compile(gl, gl.FRAGMENT_SHADER, FRAG);
    const prog = gl.createProgram();
    if (!vs || !fs || !prog) { failed = true; return; }
    gl.attachShader(prog, vs);
    gl.attachShader(prog, fs);
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) { failed = true; return; }
    gl.useProgram(prog);

    // ── Torus point cloud (R = 1, r = 0.42), deterministic-ish scatter ──
    const N = Math.max(600, Math.floor(2600 * density));
    const pos = new Float32Array(N * 3);
    const rnd = new Float32Array(N * 3);
    for (let i = 0; i < N; i++) {
      const theta = Math.random() * Math.PI * 2; // major angle
      const phi = Math.random() * Math.PI * 2;   // tube angle
      const jitter = 0.42 + (Math.random() - 0.5) * 0.10;
      const w = 1 + jitter * Math.cos(phi);
      pos[i * 3] = w * Math.cos(theta);
      pos[i * 3 + 1] = jitter * Math.sin(phi);
      pos[i * 3 + 2] = w * Math.sin(theta);
      rnd[i * 3] = Math.random();
      rnd[i * 3 + 1] = Math.random();
      rnd[i * 3 + 2] = theta;
    }
    const posBuf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, posBuf);
    gl.bufferData(gl.ARRAY_BUFFER, pos, gl.STATIC_DRAW);
    const aPos = gl.getAttribLocation(prog, "aPos");
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 3, gl.FLOAT, false, 0, 0);
    const rndBuf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, rndBuf);
    gl.bufferData(gl.ARRAY_BUFFER, rnd, gl.STATIC_DRAW);
    const aRand = gl.getAttribLocation(prog, "aRand");
    gl.enableVertexAttribArray(aRand);
    gl.vertexAttribPointer(aRand, 3, gl.FLOAT, false, 0, 0);

    const u = {
      rotY: gl.getUniformLocation(prog, "uRotY"),
      rotX: gl.getUniformLocation(prog, "uRotX"),
      aspect: gl.getUniformLocation(prog, "uAspect"),
      time: gl.getUniformLocation(prog, "uTime"),
      energ: gl.getUniformLocation(prog, "uEnergShare"),
      pulseA: gl.getUniformLocation(prog, "uPulseAngle"),
      pulseS: gl.getUniformLocation(prog, "uPulseStrength"),
      scale: gl.getUniformLocation(prog, "uPointScale"),
    };

    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.clearColor(0, 0, 0, 0);

    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    let dpr = Math.min(2, window.devicePixelRatio || 1);
    let raf = 0;
    let running = false;
    let visible = true;
    let inView = true;
    let start = performance.now();
    let pulseAt = -1e9; // time of the last block pulse
    // untrack: reading the props reactively HERE would tear down and rebuild
    // the whole GL world on every peers/pulse change. The $effect.root
    // sub-effect below is the one tracked reader.
    let peersLive = untrack(() => peers);
    let lastPulseProp = untrack(() => pulse);

    function resize() {
      if (!cv) return;
      const w = cv.clientWidth || 1;
      const h = cv.clientHeight || 1;
      const W = Math.round(w * dpr), H = Math.round(h * dpr);
      if (cv.width !== W || cv.height !== H) {
        cv.width = W; cv.height = H;
      }
      gl!.viewport(0, 0, W, H);
    }

    function frame(now: number) {
      raf = 0;
      if (!running) return;
      resize();
      const tsec = (now - start) / 1000;
      const share = Math.min(0.55, 0.07 + Math.max(0, peersLive) * 0.05);
      const since = tsec - pulseAt;
      const strength = since >= 0 && since < 2.4 ? Math.exp(-since * 1.7) : 0;
      gl!.clear(gl!.COLOR_BUFFER_BIT);
      gl!.uniform1f(u.rotY, tsec * 0.22);
      gl!.uniform1f(u.rotX, 0.55 + Math.sin(tsec * 0.11) * 0.06);
      gl!.uniform1f(u.aspect, (cv!.clientWidth || 1) / (cv!.clientHeight || 1));
      gl!.uniform1f(u.time, tsec);
      gl!.uniform1f(u.energ, share);
      gl!.uniform1f(u.pulseA, (since * 2.6) % (Math.PI * 2));
      gl!.uniform1f(u.pulseS, strength);
      gl!.uniform1f(u.scale, dpr);
      gl!.drawArrays(gl!.POINTS, 0, N);
      if (!reduced) raf = requestAnimationFrame(frame);
    }

    function play() {
      const want = visible && inView;
      if (want && !running) { running = true; raf = requestAnimationFrame(frame); }
      else if (!want && running) { running = false; if (raf) cancelAnimationFrame(raf); raf = 0; }
    }

    const onVis = () => { visible = !document.hidden; play(); };
    document.addEventListener("visibilitychange", onVis);
    const io = new IntersectionObserver((es) => {
      inView = es[0]?.isIntersecting ?? true;
      play();
    });
    io.observe(cv);
    const ro = new ResizeObserver(() => { resize(); if (reduced) { running = true; frame(performance.now()); running = false; } });
    ro.observe(cv);

    // Sub-effect: track prop changes (peers count + pulse bumps) without
    // recreating the GL world.
    const sub = $effect.root(() => {
      $effect(() => {
        peersLive = peers;
        if (pulse !== lastPulseProp) {
          lastPulseProp = pulse;
          pulseAt = (performance.now() - start) / 1000;
          if (reduced) { running = true; frame(performance.now()); running = false; }
        }
      });
    });

    running = true;
    if (reduced) { frame(performance.now()); running = false; }
    else raf = requestAnimationFrame(frame);

    return () => {
      running = false;
      if (raf) cancelAnimationFrame(raf);
      document.removeEventListener("visibilitychange", onVis);
      io.disconnect();
      ro.disconnect();
      sub();
      gl.deleteBuffer(posBuf);
      gl.deleteBuffer(rndBuf);
      gl.deleteProgram(prog);
      gl.deleteShader(vs);
      gl.deleteShader(fs);
    };
  });
</script>

{#if !failed}
  <canvas
    bind:this={canvas}
    class="torus3d"
    style="height:{height}px;"
    aria-label={t('t3d.aria')}
  ></canvas>
{:else}
  <div class="torus3d torus-fallback" style="height:{height}px;" aria-hidden="true">
    <div class="tf-ring"></div>
  </div>
{/if}

<style>
  .torus3d {
    display: block;
    width: 100%;
  }
  /* No-WebGL fallback: a quiet CSS ring in the same spirit. */
  .torus-fallback { display: flex; align-items: center; justify-content: center; }
  .tf-ring {
    width: 58%; max-width: 220px; aspect-ratio: 1;
    border-radius: 50%;
    border: 2px solid var(--cyan-mid);
    box-shadow: inset 0 0 0 1px var(--color-border);
    transform: rotateX(58deg);
  }
</style>
