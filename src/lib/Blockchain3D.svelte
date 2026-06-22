<script lang="ts">
  import { t } from "./i18n.svelte";
  // Visualisation 3D cinématique de la blockchain — CSS 3D natif, zéro dépendance.
  // Les blocs s'enchaînent en profondeur, threadés sur un « spine » lumineux
  // (la chaîne). Le plus récent porte la face Aurora. Auto-rotation + flottement,
  // orbite à la souris. Respecte prefers-reduced-motion.
  let { blocks = [], pending = 0, flashAt = 0 } = $props<{
    blocks: any[];
    pending?: number;
    flashAt?: number;
  }>();

  let rotX = $state(-20);
  let rotY = $state(-30);
  let dragging = false;
  let lastX = 0, lastY = 0;

  const SPACING = 88;        // écart en profondeur entre deux blocs (px)
  const MAX = 12;            // blocs affichés en 3D
  const shown = $derived(blocks.slice(0, MAX)); // 0 = plus récent (au premier plan)

  // Spine (la chaîne) : un faisceau qui traverse tous les blocs en profondeur.
  const spineLen = $derived((shown.length + 1) * SPACING);
  const spineMidZ = $derived((SPACING - (shown.length - 1) * SPACING) / 2);

  // Auto-rotation douce (lue dans le rAF async → effet monté une seule fois).
  $effect(() => {
    let raf = 0;
    const tick = () => {
      if (!dragging) rotY += 0.1;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  function onDown(e: PointerEvent) {
    dragging = true; lastX = e.clientX; lastY = e.clientY;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }
  function onMove(e: PointerEvent) {
    if (!dragging) return;
    rotY += (e.clientX - lastX) * 0.4;
    rotX = Math.max(-82, Math.min(82, rotX - (e.clientY - lastY) * 0.4));
    lastX = e.clientX; lastY = e.clientY;
  }
  function onUp() { dragging = false; }

  function depthOpacity(i: number) {
    return Math.max(0.16, 1 - i * 0.072);
  }
  function isFresh() {
    return Date.now() - flashAt < 1800;
  }
</script>

<div
  class="scene"
  role="img"
  aria-label={t('b3d.aria')}
  onpointerdown={onDown}
  onpointermove={onMove}
  onpointerup={onUp}
  onpointerleave={onUp}
>
  <div class="bob">
    <div class="world" style="transform: translateZ(-280px) rotateX({rotX}deg) rotateY({rotY}deg);">
      <!-- La chaîne : faisceau lumineux threadant les blocs -->
      {#if shown.length > 0}
        <div class="spine" style="width:{spineLen}px; transform: translate(-50%,-50%) translateZ({spineMidZ}px) rotateY(90deg);"></div>
      {/if}

      <!-- Bloc en cours de forge (au premier plan, translucide) -->
      <div class="blk pending" style="transform: translate(-50%,-50%) translateZ({SPACING}px);">
        <div class="face front">
          <div class="b-h">{t('b3d.forging')}</div>
          <div class="b-mint mono">{pending} tx</div>
          <div class="b-hash mono">· · · · · ·</div>
        </div>
        <div class="face top"></div>
        <div class="face right"></div>
        <div class="face left"></div>
        <div class="face bottom"></div>
      </div>

      {#each shown as b, i (b.index)}
        <div
          class="blk"
          class:fresh={i === 0 && isFresh()}
          style="transform: translate(-50%,-50%) translateZ({-i * SPACING}px); opacity: {depthOpacity(i)};"
        >
          <div class="face front" class:newest={i === 0}>
            {#if i === 0 && isFresh()}<div class="pulse"></div>{/if}
            <div class="b-h">#{b.index}</div>
            <div class="b-mint mono" class:on-aurora={i === 0}>+{(b.minted_qta ?? 0).toFixed(3)}</div>
            <div class="b-meta" class:on-aurora={i === 0}>{b.tx_count} tx</div>
            <div class="b-hash mono" class:on-aurora={i === 0}>{(b.hash || '········').slice(0, 8)}</div>
          </div>
          <div class="face top" class:newest={i === 0}></div>
          <div class="face right" class:newest={i === 0}></div>
          <div class="face left" class:newest={i === 0}></div>
          <div class="face bottom"></div>
        </div>
      {/each}

      {#if shown.length === 0}
        <div class="blk" style="transform: translate(-50%,-50%);">
          <div class="face front"><div class="b-h">{t('b3d.genesis')}</div></div>
          <div class="face top"></div><div class="face right"></div><div class="face left"></div>
        </div>
      {/if}
    </div>
  </div>

  <div class="hint">{t('b3d.hint')}</div>
</div>

<style>
  .scene {
    position: relative;
    height: 380px;
    width: 100%;
    perspective: 1150px;
    overflow: hidden;
    border-radius: 16px;
    background:
      radial-gradient(90% 70% at 50% 8%, rgba(11,165,160,0.06) 0%, transparent 60%),
      radial-gradient(120% 95% at 50% 0%, #ffffff 0%, #f5f6f8 68%, #eceef1 100%);
    border: 1px solid var(--color-border);
    box-shadow: var(--shadow-sm);
    cursor: grab;
    touch-action: none;
    user-select: none;
  }
  .scene:active { cursor: grabbing; }

  .bob { position: absolute; left: 50%; top: 54%; transform-style: preserve-3d; animation: bob 6s ease-in-out infinite; }
  .world { position: absolute; left: 0; top: 0; transform-style: preserve-3d; will-change: transform; }

  /* La chaîne — faisceau teal→indigo qui traverse les blocs. */
  .spine {
    position: absolute; left: 0; top: 0;
    height: 5px; border-radius: 3px;
    background: linear-gradient(90deg, transparent 0%, rgba(11,165,160,0.0) 4%, rgba(11,165,160,0.55) 30%, rgba(61,111,224,0.6) 70%, rgba(11,165,160,0.0) 96%, transparent 100%);
    box-shadow: 0 0 16px rgba(11,165,160,0.4);
  }

  /* Un bloc = boîte 3D 120 × 74 × 44. */
  .blk { position: absolute; left: 0; top: 0; width: 120px; height: 74px; transform-style: preserve-3d; }
  .face { position: absolute; box-sizing: border-box; border-radius: 8px; }
  .front {
    width: 120px; height: 74px;
    transform: translateZ(22px);
    background: linear-gradient(160deg, #ffffff 0%, #f7f8fa 100%);
    border: 1px solid var(--color-border-hover);
    padding: 9px 11px;
    display: flex; flex-direction: column; gap: 2px;
    box-shadow: 0 8px 22px rgba(20,30,40,0.12);
  }
  .front.newest {
    color: #fff;
    background:
      radial-gradient(120% 140% at 18% 12%, rgba(20,200,184,0.6), transparent 52%),
      radial-gradient(130% 150% at 88% 22%, rgba(124,58,237,0.55), transparent 56%),
      linear-gradient(125deg, #14C8B8 0%, #0BA5A0 26%, #3D6FE0 66%, #7C3AED 100%);
    background-size: 170% 170%;
    border-color: rgba(255,255,255,0.5);
    box-shadow: 0 14px 36px rgba(11,165,160,0.34), inset 0 1px 4px rgba(255,255,255,0.3);
    animation: aurora-shift 11s ease-in-out infinite;
  }
  .top {
    width: 120px; height: 44px;
    transform: rotateX(90deg) translateZ(22px);
    background: linear-gradient(180deg, #fbfcfd, #eceef1);
    border: 1px solid var(--color-border);
  }
  .top.newest { background: linear-gradient(180deg, #2ec7bd, #149f9a); border-color: rgba(255,255,255,0.35); }
  .right {
    width: 44px; height: 74px;
    transform: rotateY(90deg) translateZ(98px);
    background: linear-gradient(180deg, #e9eaee, #dcdee3);
    border: 1px solid var(--color-border);
  }
  .right.newest { background: linear-gradient(180deg, #2f5fd6, #2848b8); }
  .left {
    width: 44px; height: 74px;
    transform: rotateY(-90deg) translateZ(22px);
    background: linear-gradient(180deg, #f1f2f5, #e6e7ec);
    border: 1px solid var(--color-border);
  }
  .left.newest { background: linear-gradient(180deg, #18b3ab, #0e8f9a); }
  .bottom {
    width: 120px; height: 44px;
    transform: rotateX(90deg) translateZ(-52px);
    background: #e0e2e7;
  }

  .pending .front {
    background: rgba(255,255,255,0.5);
    border: 1px dashed var(--color-border-hover);
    box-shadow: none;
  }
  .pending .top { background: rgba(245,245,247,0.45); }
  .pending .right, .pending .left { background: rgba(230,230,234,0.45); }
  .pending .bottom { background: rgba(224,226,231,0.4); }

  .b-h { font-size: 13px; font-weight: 700; color: var(--color-text-0); }
  .front.newest .b-h { color: #fff; }
  .b-mint { font-size: 12px; font-weight: 700; color: var(--color-green); }
  .b-meta { font-size: 10px; color: var(--color-text-2); }
  .b-hash { font-size: 10px; color: var(--color-text-3); margin-top: auto; }
  .on-aurora { color: rgba(255,255,255,0.92) !important; }

  /* Anneau de pulsation au minage d'un nouveau bloc. */
  .pulse {
    position: absolute; inset: -3px; border-radius: 10px;
    border: 2px solid rgba(20,200,184,0.9);
    animation: pulse-ring 1.8s ease-out;
    pointer-events: none;
  }

  .fresh { animation: blk-in 0.8s cubic-bezier(.18,.9,.28,1.1); }
  @keyframes blk-in {
    from { opacity: 0; transform: translate(-50%,-50%) translateZ(200px); }
    to   { opacity: 1; }
  }
  @keyframes pulse-ring {
    0%   { opacity: 0.9; transform: scale(1); }
    100% { opacity: 0; transform: scale(1.5); }
  }
  @keyframes bob {
    0%, 100% { transform: translate(-50%,-50%) translateY(0); }
    50%      { transform: translate(-50%,-50%) translateY(-12px); }
  }
  @keyframes aurora-shift {
    0%, 100% { background-position: 0% 50%; }
    50%      { background-position: 100% 50%; }
  }

  .hint {
    position: absolute; left: 0; right: 0; bottom: 8px;
    text-align: center; font-size: 11px; color: var(--color-text-3);
    pointer-events: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .bob, .front.newest, .fresh, .pulse { animation: none; }
  }
</style>
