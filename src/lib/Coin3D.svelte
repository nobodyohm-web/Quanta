<script lang="ts">
  // Pièce QUANTA — vraie pièce 3D : disque rond métallique avec ÉPAISSEUR
  // réelle (tranche segmentée + cannelures), faces avant/arrière, brillance
  // spéculaire, et rotation continue sur l'axe Y (révèle la tranche). Pur CSS
  // 3D, zéro dépendance. Respecte prefers-reduced-motion.
  let { size = 104, label = "Q", spin = true } = $props<{ size?: number; label?: string; spin?: boolean }>();

  const SEGMENTS = 48;                         // densité de la tranche (cylindre)
  const R = size / 2;                           // rayon de la pièce
  const thickness = Math.max(8, size * 0.12);   // épaisseur de la tranche
  const segW = (2 * Math.PI * R) / SEGMENTS * 1.12; // largeur d'un segment (+recouvrement)
  const segs = Array.from({ length: SEGMENTS }, (_, i) => i * (360 / SEGMENTS));
  const fontPx = Math.round(size * 0.4);
</script>

<div class="stage" style="width:{size}px;height:{size + thickness}px;--s:{size}px;" role="img" aria-label="Pièce QUANTA">
  <div class="float">
    <div class="tilt">
      <div class="coin" class:spin style="--t:{thickness}px;">
        <!-- Tranche : cylindre segmenté (cannelures via :nth-child) -->
        {#each segs as deg}
          <span class="seg"
            style="width:{segW}px;height:{thickness}px;
                   transform:translate(-50%,-50%) rotateZ({deg}deg) translateY({R}px) rotateX(90deg);"></span>
        {/each}

        <!-- Face avant -->
        <div class="face front" style="transform:translateZ({thickness / 2}px);">
          <span class="q" style="font-size:{fontPx}px;">{label}</span>
          <span class="ring"></span>
          <span class="glint"></span>
        </div>
        <!-- Face arrière -->
        <div class="face back" style="transform:rotateY(180deg) translateZ({thickness / 2}px);">
          <span class="q" style="font-size:{fontPx}px;">{label}</span>
          <span class="ring"></span>
        </div>
      </div>
    </div>
  </div>
  <div class="contact" style="width:{size * 0.72}px;"></div>
</div>

<style>
  .stage {
    position: relative;
    display: flex; align-items: center; justify-content: center;
    perspective: 900px;
  }
  .float {
    width: var(--s); height: var(--s);
    animation: bob 5s ease-in-out infinite;
    transform-style: preserve-3d;
  }
  .tilt {
    width: var(--s); height: var(--s);
    transform: rotateX(-16deg);
    transform-style: preserve-3d;
  }
  .coin {
    position: relative;
    width: var(--s); height: var(--s);
    transform-style: preserve-3d;
    will-change: transform;
  }
  .coin.spin { animation: spin 9s linear infinite; }

  /* Faces */
  .face {
    position: absolute; inset: 0;
    border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    overflow: hidden;
    backface-visibility: hidden;
    background:
      radial-gradient(115% 115% at 30% 22%, rgba(255,255,255,0.55), transparent 44%),
      radial-gradient(130% 140% at 80% 82%, rgba(124,58,237,0.50), transparent 56%),
      conic-gradient(from 215deg at 50% 50%, #14C8B8, #0BA5A0 28%, #3D6FE0 58%, #7C3AED 80%, #14C8B8);
    box-shadow:
      inset 0 0 0 2px rgba(255,255,255,0.22),
      inset 0 7px 18px rgba(255,255,255,0.30),
      inset 0 -12px 24px rgba(0,0,0,0.26);
  }
  .q {
    position: relative; z-index: 2;
    font-weight: 800; color: #fff; line-height: 1;
    letter-spacing: -0.02em;
    text-shadow: 0 1px 1px rgba(0,0,0,0.35), 0 -1px 1px rgba(255,255,255,0.45);
    transform: translateZ(1px);
  }
  /* Liseré gravé sur la face */
  .ring {
    position: absolute; inset: 9%;
    border-radius: 50%;
    border: 1.5px solid rgba(255,255,255,0.30);
    box-shadow: inset 0 0 8px rgba(0,0,0,0.18);
    pointer-events: none;
  }
  /* Reflet spéculaire mobile */
  .glint {
    position: absolute; top: -40%; left: -35%;
    width: 55%; height: 200%;
    background: linear-gradient(100deg, transparent, rgba(255,255,255,0.6), transparent);
    transform: rotate(20deg);
    animation: glint 5.5s ease-in-out infinite;
    pointer-events: none; z-index: 3;
  }

  /* Tranche métallique segmentée */
  .seg {
    position: absolute; top: 50%; left: 50%;
    transform-origin: center center;
    background: linear-gradient(to bottom, #18d4c4 0%, #0BA5A0 46%, #066b67 100%);
  }
  .seg:nth-child(even) { filter: brightness(0.80); }      /* cannelures */
  .seg:nth-child(4n)   { filter: brightness(0.66); }

  /* Ombre de contact */
  .contact {
    position: absolute; bottom: 2px; left: 50%;
    height: 13px; border-radius: 50%;
    transform: translateX(-50%);
    background: radial-gradient(closest-side, rgba(8,80,100,0.32), transparent);
    filter: blur(3px);
    animation: contact 5s ease-in-out infinite;
  }

  @keyframes bob {
    0%, 100% { transform: translateY(0); }
    50%      { transform: translateY(-9px); }
  }
  @keyframes spin {
    from { transform: rotateY(0deg); }
    to   { transform: rotateY(360deg); }
  }
  @keyframes glint {
    0%, 30%   { transform: translateX(-40%) rotate(20deg); opacity: 0; }
    48%       { opacity: 1; }
    70%, 100% { transform: translateX(330%) rotate(20deg); opacity: 0; }
  }
  @keyframes contact {
    0%, 100% { opacity: 0.55; transform: translateX(-50%) scale(1); }
    50%      { opacity: 0.32; transform: translateX(-50%) scale(0.82); }
  }
  @media (prefers-reduced-motion: reduce) {
    .float, .coin.spin, .glint, .contact { animation: none; }
    .tilt { transform: rotateX(-16deg) rotateY(-22deg); }
  }
</style>
