<script lang="ts">
  // Aurora QUANTA — l'ARTEFACT de marque (gradient signature). À utiliser
  // UNIQUEMENT dans les moments (accueil, identité, recevoir, succès), jamais
  // sur le chrome de l'UI. Couleurs centralisées ICI → tout ajustement futur
  // se fait à un seul endroit. Interpolation oklch + grain = rendu premium.
  import type { Snippet } from "svelte";
  let { radius = 16, grain = true, children } = $props<{
    radius?: number;
    grain?: boolean;
    children?: Snippet;
  }>();
</script>

<div class="aurora" class:grain style="border-radius:{radius}px;">
  {@render children?.()}
</div>

<style>
  .aurora {
    position: relative;
    overflow: hidden;
    color: #fff;
    background:
      radial-gradient(120% 140% at 12% 8%, rgba(20, 200, 184, 0.55), transparent 50%),
      radial-gradient(120% 140% at 92% 18%, rgba(124, 58, 237, 0.55), transparent 55%),
      linear-gradient(125deg, #14C8B8 0%, #0BA5A0 24%, #3D6FE0 64%, #7C3AED 100%);
    background-size: 170% 170%, 170% 170%, 200% 200%;
    animation: aurora-flow 15s ease-in-out infinite;
  }
  @keyframes aurora-flow {
    0%, 100% { background-position: 0% 50%, 100% 0%, 0% 50%; }
    50%      { background-position: 100% 50%, 0% 100%, 100% 50%; }
  }
  @media (prefers-reduced-motion: reduce) { .aurora { animation: none; } }
  .grain::after {
    content: "";
    position: absolute;
    inset: 0;
    opacity: 0.45;
    mix-blend-mode: overlay;
    pointer-events: none;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='2'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
  }
  .aurora > :global(*) { position: relative; z-index: 1; }
</style>
