<script lang="ts">
  // QuantaMark — « l'anneau et le quantum ». The timeless mark:
  //
  //   ● a RING — the Torus, the network, the money that circulates;
  //   ● a detached diagonal TAIL — the quantum: the block being sealed
  //     into the ring, crossing it. Together they read as a geometric Q.
  //
  // Two shapes, one story. Crisp at 16 px, perfect in monochrome, no
  // gradient required (the Aurora variant is for brand MOMENTS only).
  //
  // Variants:
  //   tone="ink"    — near-black on light chrome (default)
  //   tone="teal"   — the jewel accent
  //   tone="white"  — on Aurora / dark imagery
  //   tone="aurora" — gradient stroke, reserved for hero moments
  let {
    size = 28,
    tone = "ink",
    title = "Quanta",
  } = $props<{
    size?: number;
    tone?: "ink" | "teal" | "white" | "aurora";
    title?: string;
  }>();

  const uid = `qm${Math.random().toString(36).slice(2, 8)}`;
  const stroke = $derived(
    tone === "aurora" ? `url(#${uid})`
    : tone === "white" ? "#ffffff"
    : tone === "teal" ? "var(--color-accent, #0BA5A0)"
    : "var(--color-text-0, #1d1d1f)",
  );
</script>

<!--
  Geometry (viewBox 48): ring r=15 centred 24,24 with a 60° gap opening
  south-east; the tail CROSSES the ring line on the 45° diagonal (radius 9.5 → 21.5)
  through the gap — the classic Q gesture, unmistakable at any size. Stroke 5.5, round caps — one optical weight.
-->
<svg
  width={size}
  height={size}
  viewBox="0 0 48 48"
  fill="none"
  role="img"
  aria-label={title}
>
  {#if tone === "aurora"}
    <defs>
      <linearGradient id={uid} x1="6" y1="6" x2="42" y2="42" gradientUnits="userSpaceOnUse">
        <stop offset="0" stop-color="#14C8B8" />
        <stop offset="0.45" stop-color="#0BA5A0" />
        <stop offset="1" stop-color="#7C3AED" />
      </linearGradient>
    </defs>
  {/if}
  <path
    d="M27.88 38.49 A15 15 0 1 1 38.49 27.88"
    {stroke}
    stroke-width="5.5"
    stroke-linecap="round"
  />
  <line
    x1="30.72" y1="30.72" x2="39.20" y2="39.20"
    {stroke}
    stroke-width="5.5"
    stroke-linecap="round"
  />
</svg>
