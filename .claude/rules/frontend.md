---
description: Règles frontend Svelte pour SOVA
paths: ["src/**/*.svelte", "src/**/*.ts", "src/**/*.css"]
---

# Règles Frontend

1. **Svelte 5 Runes** obligatoire : `$state()`, `$derived()`, `$effect()`, `$props()`
2. **PAS de** `onMount`, `writable`, `derived` de Svelte 4
3. **CSS vanilla** uniquement — PAS de Tailwind, PAS de CSS-in-JS
4. **Navigation** : 3 vues (Wallet, Réseau, Réglages) — bottom bar uniquement
5. **PAS de réseau social** : aucun Feed, Editor, Browser, PostCard, likes, contenu
6. **Palette** : noir `#000000`, surfaces `#0f0f0f`→`#2a2a2a`, accent `#00DC82`
7. **Typographie** : Inter uniquement, hiérarchie par poids (400-700)
8. **Espacement** : Grille 8px (4, 8, 12, 16, 24, 32, 48)
9. **Effets** : AUCUN gradient, AUCUN glassmorphism, AUCUN glow
10. **invoke()** : Toujours typé `invoke<ReturnType>("command", { args })`
