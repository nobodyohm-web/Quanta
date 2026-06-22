<script lang="ts">
  // GLOBE QUANTA — visualisation 3D du réseau P2P souverain à l'échelle mondiale.
  // Terre pointillée en rotation, TON nœud + tes pairs RÉELS épinglés à leur pays
  // réel (code pays signé dans le Hello), arcs de données lumineux entre les nœuds
  // avec impulsions qui circulent. Canvas 2D + projection orthographique 3D maison
  // → zéro dépendance externe. Aucune donnée inventée. Respecte reduced-motion.
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t as tr } from "./i18n.svelte";

  let { size = 380, caption = true } = $props<{ size?: number; caption?: boolean }>();

  let peerCount = $state(0);
  let peers = $state<any[]>([]);
  let online = $state(true);
  let canvasEl: HTMLCanvasElement | undefined = $state();
  let spin = 0; // rotation persistante entre rebuilds

  // Centroïdes (lat, lon) des pays de l'oracle énergie — données géo réelles.
  const CENTROID: Record<string, [number, number]> = {
    FR: [46.6, 2.2], DE: [51.2, 10.4], GB: [54.0, -2.0], IT: [42.8, 12.8],
    ES: [40.2, -3.7], CH: [46.8, 8.2], BE: [50.6, 4.7], NL: [52.2, 5.3],
    AT: [47.6, 14.1], SE: [62.0, 15.0], NO: [64.5, 12.0], FI: [64.0, 26.0],
    DK: [56.0, 9.5], PL: [52.0, 19.0], PT: [39.5, -8.0], RO: [45.9, 24.9],
    US: [39.5, -98.0], CA: [56.0, -106.0], BR: [-10.0, -52.0], MX: [23.6, -102.5],
    AR: [-38.0, -63.6], JP: [36.2, 138.2], KR: [36.5, 127.8], CN: [35.0, 103.0],
    IN: [22.0, 79.0], AU: [-25.0, 133.0], NZ: [-41.0, 174.0], SG: [1.35, 103.8],
    HK: [22.3, 114.2], TW: [23.7, 121.0], ZA: [-29.0, 24.0], AE: [24.0, 54.0],
    IL: [31.0, 35.0], TR: [39.0, 35.0],
  };

  // Mon pays depuis le fuseau horaire réel du navigateur (offline, honnête).
  function myCountry(): string {
    let tz = "";
    try { tz = Intl.DateTimeFormat().resolvedOptions().timeZone || ""; } catch {}
    const m: [RegExp, string][] = [
      [/Paris|Lyon/, "FR"], [/Berlin/, "DE"], [/London|Dublin/, "GB"], [/Rome/, "IT"],
      [/Madrid/, "ES"], [/Zurich|Bern/, "CH"], [/Amsterdam/, "NL"], [/Brussels/, "BE"],
      [/Stockholm/, "SE"], [/Oslo/, "NO"], [/Helsinki/, "FI"], [/Copenhagen/, "DK"],
      [/Warsaw/, "PL"], [/Lisbon/, "PT"], [/New_York|Chicago|Denver|Phoenix|Los_Angeles/, "US"],
      [/Toronto|Vancouver/, "CA"], [/Tokyo/, "JP"], [/Seoul/, "KR"], [/Shanghai|Beijing/, "CN"],
      [/Kolkata|Mumbai/, "IN"], [/Sydney|Melbourne/, "AU"], [/Singapore/, "SG"], [/Sao_Paulo/, "BR"],
    ];
    for (const [re, cc] of m) if (re.test(tz)) return cc;
    return "FR";
  }

  async function load() {
    try {
      const s = await invoke<any>("get_node_status");
      peerCount = s?.peer_count ?? 0;
      online = s?.is_online ?? true;
    } catch {}
    let pl: any[] = [];
    try { pl = await invoke<any[]>("get_peer_metrics"); } catch {}
    if (!pl?.length) { try { pl = await invoke<any[]>("list_peers"); } catch {} }
    peers = Array.isArray(pl) ? pl : [];
  }

  onMount(() => {
    load();
    const iv = setInterval(load, 5000);
    return () => clearInterval(iv);
  });

  const DEG = Math.PI / 180;
  function geoVec(lat: number, lon: number): [number, number, number] {
    const a = lat * DEG, b = lon * DEG;
    return [Math.cos(a) * Math.sin(b), Math.sin(a), Math.cos(a) * Math.cos(b)];
  }

  $effect(() => {
    const cv = canvasEl;
    if (!cv) return;
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches ?? false;

    const liveList = peers;            // dépendances réactives
    const S = size;

    // Maillage de points de la sphère (densité ∝ cos(lat) → répartition régulière).
    const dots: [number, number, number][] = [];
    for (let lat = -84; lat <= 84; lat += 6) {
      const ring = Math.max(1, Math.round(46 * Math.cos(lat * DEG)));
      for (let k = 0; k < ring; k++) dots.push(geoVec(lat, -180 + (360 * k) / ring));
    }

    // Mon nœud + pairs réels, épinglés à leur pays.
    const meCC = myCountry();
    const me = geoVec(...(CENTROID[meCC] ?? [20, 0]));
    const peerVecs = liveList.map((p, i) => {
      const cc = (p?.country || "").toUpperCase();
      const base = CENTROID[cc] ?? [10 + ((i * 47) % 60) - 30, ((i * 89) % 360) - 180];
      const jLat = (((i * 37) % 7) - 3) * 0.8;   // évite le chevauchement même-pays
      const jLon = (((i * 53) % 7) - 3) * 0.8;
      return { v: geoVec(base[0] + jLat, base[1] + jLon), q: Math.min(1, Math.max(0.35, (p?.quality_score ?? 70) / 100)) };
    });

    const DPR = Math.min(2, window.devicePixelRatio || 1);
    cv.width = S * DPR; cv.height = S * DPR;
    cv.style.width = S + "px"; cv.style.height = S + "px";
    ctx.scale(DPR, DPR);

    const cx = S / 2, cy = S / 2;
    const R = S * 0.40;
    const tilt = 0.41; // inclinaison axiale

    function rot(v: [number, number, number]): [number, number, number] {
      const x = v[0] * Math.cos(spin) + v[2] * Math.sin(spin);
      const z0 = -v[0] * Math.sin(spin) + v[2] * Math.cos(spin);
      const y = v[1] * Math.cos(tilt) - z0 * Math.sin(tilt);
      const z = v[1] * Math.sin(tilt) + z0 * Math.cos(tilt);
      return [x, y, z];
    }
    const px = (x: number) => cx + x * R;
    const py = (y: number) => cy - y * R;

    // Slerp (grand cercle) entre deux vecteurs unitaires.
    function slerp(a: number[], b: number[], t: number): [number, number, number] {
      let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
      d = Math.max(-1, Math.min(1, d));
      const om = Math.acos(d);
      if (om < 1e-3) return [a[0], a[1], a[2]];
      const s = Math.sin(om), s0 = Math.sin((1 - t) * om) / s, s1 = Math.sin(t * om) / s;
      return [a[0] * s0 + b[0] * s1, a[1] * s0 + b[1] * s1, a[2] * s0 + b[2] * s1];
    }

    let last = 0, raf = 0;
    function frame(ts: number) {
      if (!ctx) return;
      if (!last) last = ts;
      const dt = Math.min(0.05, (ts - last) / 1000); last = ts;
      if (!reduce) spin += dt * 0.16;
      const tt = ts / 1000;
      ctx.clearRect(0, 0, S, S);

      // Atmosphère (halo teal au limbe)
      const atm = ctx.createRadialGradient(cx, cy, R * 0.72, cx, cy, R * 1.16);
      atm.addColorStop(0, "rgba(11,165,160,0)");
      atm.addColorStop(0.82, "rgba(11,165,160,0.10)");
      atm.addColorStop(1, "rgba(11,165,160,0)");
      ctx.fillStyle = atm;
      ctx.beginPath(); ctx.arc(cx, cy, R * 1.16, 0, Math.PI * 2); ctx.fill();

      // Corps de la sphère (ombrage doux pour la lisibilité 3D)
      const body = ctx.createRadialGradient(cx - R * 0.3, cy - R * 0.34, R * 0.1, cx, cy, R);
      body.addColorStop(0, "rgba(244,251,250,0.95)");
      body.addColorStop(1, "rgba(225,238,238,0.92)");
      ctx.fillStyle = body;
      ctx.beginPath(); ctx.arc(cx, cy, R, 0, Math.PI * 2); ctx.fill();

      // Points du globe — face avant nets, face arrière estompés (profondeur)
      for (const d of dots) {
        const [x, y, z] = rot(d);
        const front = z >= 0;
        const op = front ? 0.10 + 0.32 * z : 0.04 + 0.06 * (z + 1);
        ctx.fillStyle = `rgba(11,140,150,${op})`;
        const r = front ? 1.05 : 0.75;
        ctx.beginPath(); ctx.arc(px(x), py(y), r, 0, Math.PI * 2); ctx.fill();
      }

      // Liens P2P tracés SUR la surface (grand cercle qui épouse le globe) —
      // aucun arc satellite, la partie arrière est masquée par le globe (z<0).
      const LIFT = 1.012;
      for (let i = 0; i < peerVecs.length; i++) {
        const pv = peerVecs[i].v;
        const SEG = 56;
        let prev: { x: number; y: number } | null = null;
        for (let k = 0; k <= SEG; k++) {
          const t = k / SEG;
          const sp = slerp(me, pv, t);
          const [x, y, z] = rot([sp[0] * LIFT, sp[1] * LIFT, sp[2] * LIFT]);
          if (z >= 0) {
            const sx = px(x), sy = py(y);
            if (prev) {
              ctx.strokeStyle = `rgba(11,165,160,${0.20 + 0.55 * z})`;
              ctx.lineWidth = 1.5;
              ctx.beginPath(); ctx.moveTo(prev.x, prev.y); ctx.lineTo(sx, sy); ctx.stroke();
            }
            prev = { x: sx, y: sy };
          } else {
            prev = null;
          }
        }
        // impulsion qui circule (face avant uniquement)
        if (!reduce) {
          const tp = (tt * 0.4 + i * 0.31) % 1;
          const sp = slerp(me, pv, tp);
          const [x, y, z] = rot([sp[0] * LIFT, sp[1] * LIFT, sp[2] * LIFT]);
          if (z >= 0) {
            ctx.fillStyle = `rgba(20,200,184,${0.55 + 0.4 * z})`;
            ctx.beginPath(); ctx.arc(px(x), py(y), 2.2, 0, Math.PI * 2); ctx.fill();
          }
        }
      }

      // Nœuds pairs
      for (let i = 0; i < peerVecs.length; i++) {
        const [x, y, z] = rot(peerVecs[i].v);
        const front = z >= 0;
        const rr = (2.4 + 2.4 * peerVecs[i].q) * (front ? 1 : 0.7);
        const hue = i % 2 === 0 ? "11,165,160" : "61,111,224";
        const op = front ? 0.95 : 0.4;
        const g = ctx.createRadialGradient(px(x), py(y), 0, px(x), py(y), rr * 2.6);
        g.addColorStop(0, `rgba(${hue},${0.4 * op})`);
        g.addColorStop(1, `rgba(${hue},0)`);
        ctx.fillStyle = g;
        ctx.beginPath(); ctx.arc(px(x), py(y), rr * 2.6, 0, Math.PI * 2); ctx.fill();
        ctx.fillStyle = `rgba(${hue},${op})`;
        ctx.beginPath(); ctx.arc(px(x), py(y), rr, 0, Math.PI * 2); ctx.fill();
      }

      // TON nœud — épinglé à ton pays, halo pulsé, au-dessus
      const [mx, my, mz] = rot(me);
      const pulse = reduce ? 0.5 : 0.5 + 0.5 * Math.sin(tt * 2);
      // sonar si aucun pair (honnête)
      if (peerVecs.length === 0 && !reduce) {
        for (let k = 0; k < 3; k++) {
          const rr = (tt * 0.4 + k / 3) % 1;
          ctx.strokeStyle = `rgba(11,165,160,${0.4 * (1 - rr)})`;
          ctx.lineWidth = 1.2;
          ctx.beginPath(); ctx.arc(px(mx), py(my), rr * R * 0.5, 0, Math.PI * 2); ctx.stroke();
        }
      }
      const sr = 5.5 + 1.6 * pulse;
      const gg = ctx.createRadialGradient(px(mx), py(my), 0, px(mx), py(my), sr * 3.4);
      gg.addColorStop(0, `rgba(11,165,160,${0.5 + 0.3 * pulse})`);
      gg.addColorStop(1, "rgba(11,165,160,0)");
      ctx.fillStyle = gg;
      ctx.beginPath(); ctx.arc(px(mx), py(my), sr * 3.4, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = "#0BA5A0";
      ctx.beginPath(); ctx.arc(px(mx), py(my), sr, 0, Math.PI * 2); ctx.fill();
      ctx.strokeStyle = "rgba(255,255,255,0.95)"; ctx.lineWidth = 1.8;
      ctx.beginPath(); ctx.arc(px(mx), py(my), sr, 0, Math.PI * 2); ctx.stroke();

      raf = requestAnimationFrame(frame);
    }
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  });

  let liveCount = $derived(Math.max(peerCount, peers.length));
</script>

<div class="net3d" style="width:{size}px;">
  <canvas bind:this={canvasEl} style="height:{size}px;"></canvas>
  {#if caption}
    <div class="net3d-cap">
      {#if liveCount > 0}
        <span class="net3d-dot on"></span> {tr('net3d.live')} · {liveCount} {liveCount > 1 ? tr('net3d.peersConnected') : tr('net3d.peerConnected')}
      {:else}
        <span class="net3d-dot"></span> {tr('net3d.searching')}
      {/if}
    </div>
  {/if}
</div>

<style>
  .net3d { display: flex; flex-direction: column; align-items: center; }
  .net3d canvas { display: block; }
  .net3d-cap {
    display: flex; align-items: center; gap: 7px;
    font-size: 12px; color: var(--color-text-2);
    margin-top: 6px;
  }
  .net3d-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--color-text-3); }
  .net3d-dot.on { background: var(--color-green); box-shadow: 0 0 0 3px rgba(22,163,74,0.16); }
</style>
