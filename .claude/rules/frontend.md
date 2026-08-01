---
description: Règles frontend Svelte (Quanta — cryptomonnaie, crypto-only)
paths: ["src/**/*.svelte", "src/**/*.ts", "src/**/*.css"]
---

# Règles Frontend (Quanta — cryptomonnaie)

1. **Svelte 5 Runes** obligatoire : `$state()`, `$derived()`, `$effect()`, `$props()`
2. **PAS de** `onMount`, `writable`, `derived` de Svelte 4
3. **CSS vanilla** uniquement — PAS de Tailwind, PAS de CSS-in-JS
4. **Navigation** : sidebar gauche — Wallet, Proches, Minage (vue `dashboard`), Réseau, Explorateur, Profil (+ Réglages). PAS de sites/forums/recherche.
5. **i18n obligatoire** : tout texte UI passe par `t('clé')` ; 6 langues (EN défaut · FR · ES · RU · ZH · JA), dictionnaires complets (`i18n.svelte.ts` + `i18n.generated.ts`).
6. **Crypto-only** : pas de PageBuilder, Browser, Forums, Subscriptions, likes — ces modules ont été supprimés. Ne pas les réintroduire.
7. **Aucun fetch externe** : l'app ne fait JAMAIS de requête HTTP vers internet ; toutes les données via Tauri IPC.
8. **Palette — THÈME CLAIR + identité « Aurora » (validé par le propriétaire,
   non négociable)** : fond **blanc** `#ffffff`, surfaces gris très clairs
   `#fbfbfd`→`#e3e3e6`, texte quasi-noir `#1d1d1f`→`#a1a1a6`, accent **teal joyau
   `#0BA5A0`** (deep `#087F8C`, bright `#14C8B8`). Style **épuré, Apple / Google**.
   ⛔ JAMAIS d'esthétique « IA / futuriste ». ⛔ PAS d'accent bleu générique
   (`#0071e3`) ni d'or (dérivatif « BTC »). Les tokens sont dans `src/app.css`.
   **Thème sombre** (depuis v3.12.0) : le clair reste le **défaut** et l'identité
   du produit ; le sombre est une **préférence utilisateur explicite**, servie par
   le bloc `:root[data-theme="dark"]` (slate froid, texte hiérarchisé, teal bright).
   Toute couleur écrite en dur qui ne répond pas au thème est un bug — c'est ainsi
   que le basculement était resté sans effet jusqu'à v3.12.0.
   *(Corrigé le 2026-07-25 : cette règle interdisait encore le mode sombre que
   l'application livre depuis v3.12.0 — une contradiction relevée par l'audit.)*
9. **Typographie** : Inter uniquement, hiérarchie par poids (400-700) ; chiffres
   **tabulaires** (`tabular-nums lining-nums`, zéro barré) sur tous les montants.
10. **Espacement** : Grille 8px (4, 8, 12, 16, 24, 32, 48)
11. **Effets** : le **gradient « Aurora »** (teal→indigo→violet, composant
    `Aurora.svelte`) est l'**artefact de marque** — autorisé UNIQUEMENT dans les
    MOMENTS (logo/pièce, identicon, accueil, recevoir, succès, empty-states),
    **JAMAIS sur le chrome** (panneaux, listes, barres = surfaces solides ; les
    boutons primaires = teal plein). AUCUN glassmorphism sur le chrome, AUCUN glow
    néon. Ombres douces autorisées (style Apple : `--shadow-sm/--shadow/--shadow-lg`).
12. **invoke()** : Toujours typé `invoke<ReturnType>("command", { args })`
13. **Identité utilisateur** : pseudo + identicon BLAKE3 par défaut, jamais de KYC, jamais de tracking
