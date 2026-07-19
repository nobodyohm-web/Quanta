// Shared three.js shell for Quanta scenes — one place for the boring-but-
// critical plumbing: renderer setup tuned for the LIGHT theme (transparent
// canvas over white, ACES tone mapping), DPR clamp, resize tracking, battery
// discipline (pause when the tab is hidden or the canvas leaves the
// viewport), reduced-motion single-frame mode, and leak-free disposal.
import * as THREE from "three";

export interface Shell {
  renderer: THREE.WebGLRenderer;
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  /** Register the per-frame callback and start the loop (or render one
   *  static frame under prefers-reduced-motion). */
  start(frame: (dt: number, t: number) => void): void;
  /** True while the loop is allowed to run (visible + in viewport). */
  readonly reduced: boolean;
  /** Ask for a single render now (used by reduced-motion event updates). */
  renderOnce(): void;
  dispose(): void;
}

export function createShell(
  canvas: HTMLCanvasElement,
  opts: { fov?: number; z?: number; y?: number } = {},
): Shell | null {
  let renderer: THREE.WebGLRenderer;
  try {
    renderer = new THREE.WebGLRenderer({
      canvas,
      alpha: true,
      antialias: true,
      powerPreference: "low-power",
    });
  } catch {
    return null;
  }
  renderer.setClearColor(0x000000, 0);
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.outputColorSpace = THREE.SRGBColorSpace;

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(opts.fov ?? 42, 1, 0.1, 60);
  camera.position.set(0, opts.y ?? 0.9, opts.z ?? 4.2);
  camera.lookAt(0, 0, 0);

  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  let raf = 0;
  let running = false;
  let visible = !document.hidden;
  let inView = true;
  let frameCb: ((dt: number, t: number) => void) | null = null;
  let last = performance.now();
  const t0 = last;

  function resize() {
    const w = canvas.clientWidth || 1;
    const h = canvas.clientHeight || 1;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    renderer.setPixelRatio(dpr);
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  }

  function tick(now: number) {
    raf = 0;
    if (!running) return;
    const dt = Math.min(0.05, (now - last) / 1000);
    last = now;
    frameCb?.(dt, (now - t0) / 1000);
    renderer.render(scene, camera);
    raf = requestAnimationFrame(tick);
  }

  function play() {
    const want = visible && inView && !reduced && frameCb !== null;
    if (want && !running) {
      running = true;
      last = performance.now();
      raf = requestAnimationFrame(tick);
    } else if (!want && running) {
      running = false;
      if (raf) cancelAnimationFrame(raf);
      raf = 0;
    }
  }

  const onVis = () => { visible = !document.hidden; play(); };
  document.addEventListener("visibilitychange", onVis);
  const io = new IntersectionObserver((es) => {
    inView = es[0]?.isIntersecting ?? true;
    play();
  });
  io.observe(canvas);
  const ro = new ResizeObserver(() => {
    resize();
    if (reduced) renderOnce();
  });
  ro.observe(canvas);
  resize();

  function renderOnce() {
    frameCb?.(0.016, (performance.now() - t0) / 1000);
    renderer.render(scene, camera);
  }

  return {
    renderer,
    scene,
    camera,
    reduced,
    renderOnce,
    start(fn) {
      frameCb = fn;
      if (reduced) renderOnce();
      else play();
    },
    dispose() {
      running = false;
      if (raf) cancelAnimationFrame(raf);
      document.removeEventListener("visibilitychange", onVis);
      io.disconnect();
      ro.disconnect();
      // Disposal must NEVER be able to break navigation (a throw here would leave
      // the view stuck) — hence the guard.
      try {
        scene.traverse((obj) => {
          const mesh = obj as THREE.Mesh;
          if (mesh.geometry) mesh.geometry.dispose();
          const mat = mesh.material as THREE.Material | THREE.Material[] | undefined;
          if (Array.isArray(mat)) mat.forEach((m) => m.dispose());
          else mat?.dispose();
        });
        renderer.dispose();
        // ⭐ THE nav-freeze fix: renderer.dispose() frees GPU objects but does NOT
        // release the WebGL context. Without this, every visit to a 3D screen
        // (Minage/Réseau…) leaked a context; after the browser's ~16-context cap
        // the whole app froze ("l'app bloque"). Force the context loss on unmount.
        renderer.forceContextLoss();
      } catch { /* never let teardown trap the UI */ }
    },
  };
}

/** Soft circular drop-shadow sprite (fake, cheap, perfect on white). */
export function softShadow(radiusPx = 128): THREE.Sprite {
  const cv = document.createElement("canvas");
  cv.width = cv.height = radiusPx;
  const ctx = cv.getContext("2d");
  if (ctx) {
    const g = ctx.createRadialGradient(
      radiusPx / 2, radiusPx / 2, 0,
      radiusPx / 2, radiusPx / 2, radiusPx / 2,
    );
    g.addColorStop(0, "rgba(48,40,30,0.32)");
    g.addColorStop(0.65, "rgba(48,40,30,0.10)");
    g.addColorStop(1, "rgba(48,40,30,0)");
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, radiusPx, radiusPx);
  }
  const tex = new THREE.CanvasTexture(cv);
  const sprite = new THREE.Sprite(
    new THREE.SpriteMaterial({ map: tex, transparent: true, depthWrite: false }),
  );
  return sprite;
}

/** Quanta palette as three colors (kept in one place). */
export const PALETTE = {
  ink: new THREE.Color("#57544e"),
  teal: new THREE.Color("#0BA5A0"),
  tealBright: new THREE.Color("#14C8B8"),
  sealedStone: new THREE.Color("#0B4A50"), // teal-900 — finalized block (carved, permanent)
  indigo: new THREE.Color("#3D6FE0"),
  violet: new THREE.Color("#7C3AED"),
};
