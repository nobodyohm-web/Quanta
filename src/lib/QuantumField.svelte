<script lang="ts">
  // Champ quantique — particules qui dérivent lentement, reliées par de fines
  // lignes quand elles se rapprochent (intrication). Sobre, lent, monochrome.
  // Canvas 2D, léger. Se pose en fond d'un conteneur positionné (inset:0).
  let { density = 1, tint = "11,165,160", maxOpacity = 1 } = $props<{ density?: number; tint?: string; maxOpacity?: number }>();
  let canvasEl: HTMLCanvasElement | undefined = $state();
  let wrap: HTMLDivElement | undefined = $state();

  function pseudoRand(s: number): number {
    const x = Math.sin(s * 99.73) * 43758.5453;
    return x - Math.floor(x);
  }

  $effect(() => {
    const cv = canvasEl, host = wrap;
    if (!cv || !host) return;
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches ?? false;
    const DPR = Math.min(2, window.devicePixelRatio || 1);

    let W = 0, H = 0;
    let parts: { x: number; y: number; vx: number; vy: number; r: number; ph: number }[] = [];

    function build() {
      const rect = host!.getBoundingClientRect();
      W = Math.max(1, rect.width); H = Math.max(1, rect.height);
      cv!.width = W * DPR; cv!.height = H * DPR;
      cv!.style.width = W + "px"; cv!.style.height = H + "px";
      ctx!.setTransform(DPR, 0, 0, DPR, 0, 0);
      const N = Math.max(20, Math.min(108, Math.round((W * H) / 17000 * density)));
      parts = Array.from({ length: N }, (_, i) => ({
        x: pseudoRand(i * 2.1 + 0.3) * W,
        y: pseudoRand(i * 3.7 + 1.1) * H,
        vx: (pseudoRand(i * 5.3 + 2.2) - 0.5) * 0.013,
        vy: (pseudoRand(i * 7.9 + 4.4) - 0.5) * 0.013,
        r: 0.8 + pseudoRand(i * 11.1 + 6.6) * 1.7,
        ph: pseudoRand(i * 13.3 + 8.8) * Math.PI * 2,
      }));
    }

    const ro = new ResizeObserver(build); ro.observe(host); build();

    const LINK = 134;
    let raf = 0, last = 0;
    function frame(ts: number) {
      if (!ctx) return;
      const dt = last ? Math.min(48, ts - last) : 16; last = ts;
      const tt = ts / 1000;
      ctx.clearRect(0, 0, W, H);

      if (!reduce) for (const p of parts) {
        p.x += p.vx * dt; p.y += p.vy * dt;
        if (p.x < -12) p.x = W + 12; else if (p.x > W + 12) p.x = -12;
        if (p.y < -12) p.y = H + 12; else if (p.y > H + 12) p.y = -12;
      }

      // Liens d'intrication (par proximité)
      for (let i = 0; i < parts.length; i++) {
        for (let j = i + 1; j < parts.length; j++) {
          const a = parts[i], b = parts[j];
          const dx = a.x - b.x, dy = a.y - b.y;
          const d = Math.sqrt(dx * dx + dy * dy);
          if (d < LINK) {
            const op = (1 - d / LINK) * 0.11 * maxOpacity;
            ctx.strokeStyle = `rgba(${tint},${op})`;
            ctx.lineWidth = 1;
            ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
          }
        }
      }

      // Particules (pulsation douce)
      for (const p of parts) {
        const pulse = reduce ? 0.5 : 0.5 + 0.5 * Math.sin(tt * 1.1 + p.ph);
        const a = (0.13 + 0.22 * pulse) * maxOpacity;
        const g = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, p.r * 3.2);
        g.addColorStop(0, `rgba(${tint},${a})`);
        g.addColorStop(1, `rgba(${tint},0)`);
        ctx.fillStyle = g;
        ctx.beginPath(); ctx.arc(p.x, p.y, p.r * 3.2, 0, Math.PI * 2); ctx.fill();
        ctx.fillStyle = `rgba(${tint},${a + 0.12 * maxOpacity})`;
        ctx.beginPath(); ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2); ctx.fill();
      }
      raf = requestAnimationFrame(frame);
    }
    raf = requestAnimationFrame(frame);
    return () => { cancelAnimationFrame(raf); ro.disconnect(); };
  });
</script>

<div class="qf" bind:this={wrap}><canvas bind:this={canvasEl}></canvas></div>

<style>
  .qf { position: absolute; inset: 0; overflow: hidden; pointer-events: none; }
  .qf canvas { display: block; }
</style>
