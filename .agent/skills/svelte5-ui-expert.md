# Svelte 5 UI Expert Skill

Torus est conçu pour être esthétiquement "premium" avec une identité visuelle "Titan" et moderne (effets de verre, fondations robustes).

## 1. Utilisation de Svelte 5 (Runes)
- Interdiction absolue d'utiliser la syntaxe Svelte 4 (`export let variable`, `$store`).
- Utilise exclusivement les **Runes** de Svelte 5 :
  - `let variable = $state();` pour la réactivité locale.
  - `let computed = $derived(variable * 2);` pour les valeurs calculées.
  - `$effect(() => { ... })` pour les effets de bord au lieu des anciens lifecycles.
  - `let { prop1, prop2 } = $props();` pour déclarer les propriétés des composants.

## 2. Design System Premium & Interactions
- La fluidité est vitale. Ajoute des micro-animations (transitions CSS, ou Svelte `transition:fade/slide/fly` avec paramétrage d'easing local) pour chaque changement d'état UI.
- L'interface repose sur des palettes sombres sophistiquées avec un accent principal `#007AFF` (Bleu Électrique).
- Pas de Tailwind CSS (sauf instruction explicite du développeur). Utilise le CSS/Vanilla Scoped de Svelte de façon propre, et déclare des variables globales (`var(--primary)`, `var(--bg-glass)`) dans `src/app.css`.

## 3. Structure des Composants
- Maintenir les composants propres. Sépare bien la logique (`<script lang="ts">`) de la structure UI.
- N'oublie pas l'accessibilité de base (attributs ARIA, `tabindex`, sémantique HTML).
