<script lang="ts">
  // ChainScene — the blockchain you can look at. Recent blocks recede along a
  // gentle helix: the newest stands front-right, history shrinks into the
  // distance. Blocks at or below the FINALITY FLOOR are sealed teal stone
  // (irreversible — carved); blocks above it are frosted glass (still
  // replaceable by fork-choice). The teal waterline between the two makes
  // Casper-FFG *visible*. New blocks drop in with a soft bounce; hover any
  // block for its details. Drag to orbit.
  import * as THREE from "three";
  import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
  import { RoundedBoxGeometry } from "three/examples/jsm/geometries/RoundedBoxGeometry.js";
  import { untrack } from "svelte";
  import { t } from "../i18n.svelte";
  import { createShell, softShadow, PALETTE } from "./scene";

  interface BlockInfo {
    index: number;
    tx_count: number;
    minted_qta: number;
    hash: string;
  }

  let {
    blocks = [],
    floor = 0,
    flashAt = 0,
    height = 300,
  } = $props<{
    blocks?: BlockInfo[];
    floor?: number;
    flashAt?: number;
    height?: number;
  }>();

  let canvas = $state<HTMLCanvasElement | undefined>();
  let failed = $state(false);
  let tip = $state<{ x: number; y: number; index: number; txs: number; minted: number; sealed: boolean } | null>(null);

  const MAXB = 22;

  function slot(i: number): { x: number; y: number; z: number; s: number } {
    return {
      x: -i * 0.98,
      y: Math.sin(i * 0.42) * 0.14,
      z: -i * 0.34,
      s: 1 / (1 + i * 0.055),
    };
  }

  $effect(() => {
    const cv = canvas;
    if (!cv) return;
    const shell = createShell(cv, { fov: 38, z: 5.4, y: 1.15 });
    if (!shell) { failed = true; return; }
    // Narrowed alias — TS narrowing does not flow into hoisted `function`s.
    const sh = shell;
    const { scene, camera } = shell;
    camera.position.x = 1.1;
    camera.lookAt(-1.2, 0, 0);

    scene.add(new THREE.HemisphereLight(0xffffff, 0xe9e4da, 1.2));
    const sun = new THREE.DirectionalLight(0xffffff, 1.0);
    sun.position.set(3, 5, 4);
    scene.add(sun);

    const controls = new OrbitControls(camera, cv);
    controls.enableZoom = false;
    controls.enablePan = false;
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;
    controls.target.set(-1.2, 0, 0);
    controls.minPolarAngle = Math.PI * 0.24;
    controls.maxPolarAngle = Math.PI * 0.58;

    const boxGeo = new RoundedBoxGeometry(0.66, 0.5, 0.5, 4, 0.09);
    const sealedMat = new THREE.MeshStandardMaterial({
      color: PALETTE.sealedStone, roughness: 0.42, metalness: 0.14, // teal-900 carved stone
    });
    const frostedMat = new THREE.MeshPhysicalMaterial({
      color: 0xffffff, roughness: 0.55, metalness: 0,
      transparent: true, opacity: 0.55, clearcoat: 0.4,
    });
    const newestEdge = new THREE.LineSegments(
      new THREE.EdgesGeometry(boxGeo, 30),
      new THREE.LineBasicMaterial({ color: PALETTE.tealBright, transparent: true, opacity: 0.9 }),
    );

    // Waterline — the finality boundary plane (repositioned per data).
    const waterline = new THREE.Mesh(
      new THREE.PlaneGeometry(0.02, 1.6),
      new THREE.MeshBasicMaterial({
        color: PALETTE.teal, transparent: true, opacity: 0.0, side: THREE.DoubleSide,
      }),
    );
    // A slim vertical "curtain" between sealed & unsealed history.
    waterline.geometry = new THREE.PlaneGeometry(1.6, 1.5);
    waterline.rotation.y = Math.PI / 2;
    scene.add(waterline);

    const shadow = softShadow();
    shadow.position.set(-2.2, -0.62, 0);
    shadow.scale.set(7.5, 1.1, 1);
    scene.add(shadow);

    // ── Block meshes, rebuilt on data change (≤22 meshes — trivial) ──
    const group = new THREE.Group();
    scene.add(group);
    interface Anim { mesh: THREE.Mesh; targetX: number; targetY: number; drop?: number }
    let anims: Anim[] = [];
    let lastNewest = -1;

    function rebuild(list: BlockInfo[], floorIdx: number) {
      // dispose old
      for (const a of anims) {
        group.remove(a.mesh);
      }
      anims = [];
      newestEdge.removeFromParent();
      const trimmed = list.slice(0, MAXB);
      let boundaryX: number | null = null;
      trimmed.forEach((b, i) => {
        const sealed = b.index <= floorIdx;
        const mesh = new THREE.Mesh(boxGeo, sealed ? sealedMat : frostedMat);
        const p = slot(i);
        mesh.position.set(p.x, p.y, p.z);
        mesh.scale.setScalar(p.s);
        mesh.userData = { index: b.index, txs: b.tx_count, minted: b.minted_qta, sealed };
        group.add(mesh);
        const isNew = i === 0 && b.index !== lastNewest && lastNewest !== -1;
        anims.push({ mesh, targetX: p.x, targetY: p.y, drop: isNew ? performance.now() : undefined });
        if (i === 0) {
          newestEdge.position.copy(mesh.position);
          newestEdge.scale.copy(mesh.scale).multiplyScalar(1.001);
          group.add(newestEdge);
        }
        // boundary sits between the last sealed and the first frosted block
        if (!sealed && i + 1 < trimmed.length && trimmed[i + 1].index <= floorIdx) {
          boundaryX = (slot(i).x + slot(i + 1).x) / 2;
        }
      });
      lastNewest = trimmed[0]?.index ?? -1;
      const wlMat = waterline.material as THREE.MeshBasicMaterial;
      if (boundaryX !== null) {
        waterline.position.set(boundaryX, 0.15, -Math.abs(boundaryX) * 0.35);
        wlMat.opacity = 0.13;
      } else if (trimmed.length > 0 && trimmed[trimmed.length - 1].index > floorIdx) {
        wlMat.opacity = 0; // floor below the visible window
      } else if (trimmed.length > 0) {
        wlMat.opacity = 0; // everything visible is sealed
      }
      if (sh.reduced) sh.renderOnce();
    }

    // Reactive data feed (props tracked in a sub-effect; GL world persists).
    let flashSeen = untrack(() => flashAt);
    const sub = $effect.root(() => {
      $effect(() => {
        rebuild(blocks, floor);
        if (flashAt !== flashSeen) flashSeen = flashAt;
      });
    });

    // ── Hover tooltip (raycast) ──
    const ray = new THREE.Raycaster();
    const ptr = new THREE.Vector2();
    function onMove(e: PointerEvent) {
      const r = cv!.getBoundingClientRect();
      ptr.x = ((e.clientX - r.left) / r.width) * 2 - 1;
      ptr.y = -((e.clientY - r.top) / r.height) * 2 + 1;
      ray.setFromCamera(ptr, camera);
      const hits = ray.intersectObjects(group.children, false);
      const hit = hits.find((h) => (h.object as THREE.Mesh).userData?.index !== undefined);
      if (hit) {
        const u = (hit.object as THREE.Mesh).userData as { index: number; txs: number; minted: number; sealed: boolean };
        tip = { x: e.clientX - r.left, y: e.clientY - r.top, index: u.index, txs: u.txs, minted: u.minted, sealed: u.sealed };
        if (sh.reduced) sh.renderOnce();
      } else if (tip) {
        tip = null;
      }
    }
    function onLeave() { tip = null; }
    cv.addEventListener("pointermove", onMove);
    cv.addEventListener("pointerleave", onLeave);

    shell.start((dt) => {
      controls.update();
      const now = performance.now();
      for (const a of anims) {
        if (a.drop !== undefined) {
          const age = (now - a.drop) / 1000;
          if (age < 0.7) {
            const p = age / 0.7;
            // drop from above with a single soft bounce
            const e = p < 0.75 ? 1 - Math.pow(1 - p / 0.75, 2) : 1 + Math.sin((p - 0.75) * 12.5) * 0.06 * (1 - p);
            a.mesh.position.y = a.targetY + (1 - e) * 1.4;
          } else {
            a.mesh.position.y = a.targetY;
            a.drop = undefined;
          }
        }
      }
      void dt;
    });

    return () => {
      cv.removeEventListener("pointermove", onMove);
      cv.removeEventListener("pointerleave", onLeave);
      sub();
      controls.dispose();
      boxGeo.dispose();
      shell.dispose();
    };
  });
</script>

{#if !failed}
  <div class="chain-scene" style="height:{height}px;">
    <canvas bind:this={canvas} aria-label={t('scene.chain.aria')}></canvas>
    <div class="cs-legend">
      <span class="cs-key"><span class="cs-dot cs-sealed"></span>{t('scene.chain.sealed')}</span>
      <span class="cs-key"><span class="cs-dot cs-frosted"></span>{t('scene.chain.open')}</span>
      <span class="cs-floor mono">
        <svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="3" y="7" width="10" height="7" rx="1.5"/><path d="M5 7V5a3 3 0 016 0v2"/></svg>
        ≤ #{floor.toLocaleString()}
      </span>
    </div>
    {#if tip}
      <div class="cs-tip mono" style="left:{Math.min(tip.x + 14, 9999)}px;top:{tip.y + 10}px;">
        <b>#{tip.index.toLocaleString()}</b> · {tip.txs} tx · {tip.minted.toFixed(2)} QTA
        {#if tip.sealed}<span class="cs-tip-sealed">{t('scene.chain.sealedTag')}</span>{/if}
      </div>
    {/if}
  </div>
{:else}
  <div class="chain-scene scene-fallback" style="height:{height}px;" aria-hidden="true"></div>
{/if}

<style>
  .chain-scene {
    position: relative;
    width: 100%;
    overflow: hidden;
    background:
      radial-gradient(70% 85% at 42% 40%, rgba(11, 165, 160, 0.06), transparent 70%),
      radial-gradient(45% 60% at 75% 25%, rgba(124, 58, 237, 0.04), transparent 70%);
  }
  .chain-scene canvas { display: block; width: 100%; height: 100%; cursor: grab; }
  .chain-scene canvas:active { cursor: grabbing; }

  .cs-legend {
    position: absolute; left: 14px; bottom: 12px;
    display: flex; align-items: center; gap: 14px;
    font-size: 11px; color: var(--color-text-2);
    background: rgba(255, 255, 255, 0.75);
    backdrop-filter: blur(4px);
    border: 1px solid var(--color-border);
    border-radius: 999px;
    padding: 5px 12px;
    pointer-events: none;
  }
  .cs-key { display: inline-flex; align-items: center; gap: 6px; }
  .cs-dot { width: 9px; height: 9px; border-radius: 3px; display: inline-block; }
  .cs-sealed { background: var(--color-accent); }
  .cs-frosted { background: #fff; border: 1px solid var(--color-border-hover); }
  .cs-floor { display: inline-flex; align-items: center; gap: 5px; color: var(--color-accent); font-weight: 600; }

  .cs-tip {
    position: absolute; z-index: 5;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.94);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    box-shadow: var(--shadow);
    font-size: 12px; color: var(--color-text-0);
    pointer-events: none;
    white-space: nowrap;
  }
  .cs-tip-sealed {
    margin-left: 7px; font-size: 10px; font-weight: 700;
    color: var(--color-accent); text-transform: uppercase; letter-spacing: 0.05em;
  }
</style>
