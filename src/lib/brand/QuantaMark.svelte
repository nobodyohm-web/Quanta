<script lang="ts">
  // QuantaMark — « l'anneau et le quantum ». The timeless mark:
  //
  //   ● a RING — the Torus, the network, the money that circulates;
  //   ● a detached diagonal TAIL — the quantum: the block sealing INTO the
  //     ring, crossing its seam. Together they read as a geometric Q.
  //
  // Geometry (viewBox 48): ring r=15 centred 24,24 with a 60° seam opening
  // south-east; the tail lies on the 45° diagonal from radius 9 → 21, so it
  // crosses the (absent) ring line at r=15 = (34.607, 34.607) — the tail's
  // EXACT midpoint: the block caught precisely at the threshold = the seal.
  // Stroke 5.5, round caps — one optical weight, no fill.
  //
  //   tone="ink"    — near-black on light chrome (default)
  //   tone="teal"   — the jewel accent / nav logo
  //   tone="white"  — on Aurora / dark imagery
  //   tone="aurora" — gradient stroke, reserved for hero MOMENTS only
  //
  // `sealing`: when it flips true, the seam blooms Aurora once (~600 ms) then
  // settles — wire it to real quanta://block-sealed events at CALL SITES,
  // never as a loop. Honours prefers-reduced-motion.
  let {
    size = 28,
    tone = "ink",
    title = "Quanta",
    sealing = false,
  } = $props<{
    size?: number;
    tone?: "ink" | "teal" | "white" | "aurora";
    title?: string;
    sealing?: boolean;
  }>();

  const uid = `qm${Math.random().toString(36).slice(2, 8)}`;
  const stroke = $derived(
    tone === "aurora" ? `url(#${uid}g)`
    : tone === "white" ? "#ffffff"
    : tone === "teal" ? "var(--color-accent, #0BA5A0)"
    : "var(--color-text-0, #1d1d1f)",
  );

  // One-shot seal bloom: re-keyed each time `sealing` flips true (unless the
  // viewer prefers reduced motion). Cheap — no persistent media listener.
  let seal = $state(0);
  $effect(() => {
    if (!sealing) return;
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches;
    if (!reduce) seal += 1;
  });
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 48 48"
  fill="none"
  role="img"
  aria-label={title}
>
  <defs>
    {#if tone === "aurora"}
      <linearGradient id={`${uid}g`} x1="8" y1="8" x2="42" y2="42" gradientUnits="userSpaceOnUse">
        <stop offset="0" stop-color="#14C8B8" />
        <stop offset="0.42" stop-color="#0BA5A0" />
        <stop offset="0.72" stop-color="#087F8C" />
        <stop offset="0.9" stop-color="#3D6FE0" />
        <stop offset="1" stop-color="#7C3AED" />
      </linearGradient>
    {/if}
    <radialGradient id={`${uid}b`} cx="0.5" cy="0.5" r="0.5">
      <stop offset="0" stop-color="#14C8B8" stop-opacity="0.85" />
      <stop offset="1" stop-color="#14C8B8" stop-opacity="0" />
    </radialGradient>
  </defs>

  <path
    d="M27.882 38.489 A15 15 0 1 1 38.489 27.882"
    {stroke}
    stroke-width="5.5"
    stroke-linecap="round"
  />
  <line
    x1="30.364" y1="30.364" x2="38.849" y2="38.849"
    {stroke}
    stroke-width="5.5"
    stroke-linecap="round"
  />

  {#key seal}
    {#if seal > 0}
      <circle class="seal-bloom" cx="34.607" cy="34.607" r="11" fill={`url(#${uid}b)`} />
    {/if}
  {/key}
</svg>

<style>
  /* The seal bloom scales from its own centre; one shot, never a loop. */
  .seal-bloom {
    transform-box: fill-box;
    transform-origin: center;
    animation: sealPulse 0.62s cubic-bezier(0.22, 0.61, 0.36, 1);
    pointer-events: none;
  }
  @keyframes sealPulse {
    0%   { opacity: 0; transform: scale(0.25); }
    28%  { opacity: 1; }
    100% { opacity: 0; transform: scale(1.75); }
  }
  @media (prefers-reduced-motion: reduce) {
    .seal-bloom { animation: none; opacity: 0; }
  }
</style>
