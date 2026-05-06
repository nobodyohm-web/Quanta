---
description: Règles frontend Svelte pour Torus V3 (Social Web)
paths: ["src/**/*.svelte", "src/**/*.ts", "src/**/*.css"]
---

# Règles Frontend (V3 — Social Web)

1. **Svelte 5 Runes** obligatoire : `$state()`, `$derived()`, `$effect()`, `$props()`
2. **PAS de** `onMount`, `writable`, `derived` de Svelte 4
3. **CSS vanilla** uniquement — PAS de Tailwind, PAS de CSS-in-JS
4. **Navigation V3** : Browser, PageBuilder, Search, Subscriptions, Profile, Forums, Wallet, Network, Settings — sidebar gauche
5. **Social ouvert** : Feed, Browser, PageBuilder, likes, abonnements, forums sont la **mission V3** (cf. CLAUDE.md). L'ancienne interdiction est levée.
6. **Sandboxing pages** : Tout contenu utilisateur affiché passe par une iframe avec `sandbox="allow-same-origin"` (sans `allow-scripts` par défaut). JS opt-in par site, jamais activé sans toggle explicite. CSP draconien.
7. **Aucun fetch externe** : L'app ne fait JAMAIS de requête HTTP vers internet. Tous les assets viennent du DAG via IPC.
8. **Palette** : noir `#000000`, surfaces `#0f0f0f`→`#2a2a2a`, accent `#00DC82`
9. **Typographie** : Inter uniquement, hiérarchie par poids (400-700)
10. **Espacement** : Grille 8px (4, 8, 12, 16, 24, 32, 48)
11. **Effets** : AUCUN gradient, AUCUN glassmorphism, AUCUN glow
12. **invoke()** : Toujours typé `invoke<ReturnType>("command", { args })`
13. **Identité utilisateur** : pseudo + identicon BLAKE3 par défaut, jamais de KYC, jamais de tracking
