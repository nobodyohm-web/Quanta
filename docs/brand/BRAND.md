# Quanta — Charte de marque

> La marque doit survivre aux modes. Deux formes, une couleur, une histoire.

## La marque : « l'anneau et le quantum »

Le symbole Quanta est un **Q géométrique** composé de exactement deux traits :

1. **L'anneau** — le Torus : le réseau, la monnaie qui circule, le cycle sans
   serveur ni centre.
2. **Le quantum** — la queue diagonale détachée qui **traverse** l'anneau par
   sa brèche : le bloc en train d'être scellé dans la chaîne, l'unité
   d'énergie qui entre dans le réseau.

Ensemble ils se lisent « Q » à toutes les tailles, dès 16 px, en une seule
graisse optique (trait 5.5/48, terminaisons rondes).

### Géométrie canonique (espace 48×48)

- Anneau : centre (24, 24), rayon 15, brèche de 60° ouverte au sud-est
  (centrée sur la diagonale 45°). Arc majeur de 300° :
  `M27.882 38.489 A15 15 0 1 1 38.489 27.882`.
- Quantum : segment sur la diagonale 45°, du rayon **9** au rayon **21** —
  il **croise** la ligne de l'anneau (R=15) exactement à son **milieu**
  (34.607, 34.607) : le bloc saisi au seuil du scellement. Jamais accolé,
  jamais entièrement dehors : dehors = loupe, accolé = lettre banale.
  `x1=30.364 y1=30.364 x2=38.849 y2=38.849`.
- Trait : 5.5, `stroke-linecap: round`. Aucun remplissage. Tout dérive de
  deux constantes (cercle R=15, diagonale 45°) → reproductible à n'importe
  quelle taille en multipliant.
- **Ne jamais** : comprimer la brèche de 60°, sortir la queue de la
  diagonale 45°, fermer l'anneau, ajouter un disque/biseau/ellipse d'orbite.

**Source unique** : `src/lib/brand/QuantaMark.svelte` (app) et
`docs/brand/quanta-app-icon.svg` (icône système 1024).

### Variantes autorisées

| Ton | Usage |
|---|---|
| `ink` (quasi-noir) | chrome clair, documents |
| `teal` (#0BA5A0) | accent, états actifs |
| `white` | sur Aurora ou photo sombre |
| `aurora` (gradient) | **moments** de marque uniquement (accueil, chargement, identité) |

### Interdits

- Ne pas incliner, déformer, ombrer ni contourner d'un halo.
- Ne pas fermer la brèche, ne pas recoller la queue à l'anneau.
- Jamais d'or (dérive « BTC »), jamais de bleu générique fintech.
- Le gradient Aurora ne colore **jamais** le chrome (panneaux, listes, barres).

## Colorimétrie iconique

**Une seule couleur possède la marque : le teal joyau.** Sa gamme complète
vit dans `src/app.css` (`--teal-50 … --teal-900`), et chaque cran porte un
**rôle** (source unique, consommé par la marque *et* les scènes 3D) :

| Token | Hex | Rôle |
|---|---|---|
| `--color-mark` = `--teal-500` | `#0BA5A0` | **signature** — marque au repos, logo nav, boutons primaires, liens, focus |
| `--color-mark-monument` = `--teal-700` | `#087F8C` | gravé sur blanc, petites tailles (favicon), bordures actives |
| `--color-sealed-stone` = `--teal-900` | `#0B4A50` | **pierre scellée** — bloc finalisé (≤ plancher) en 3D, gravure/deboss |
| `--color-seal-flash` | `#14C8B8` | éclat au scellement / récompense — jamais une surface au repos |

L'**Aurora** (teal → indigo #3D6FE0 → violet #7C3AED) est *l'artefact* de
marque : un gradient réservé aux moments (accueil, identité, recevoir,
succès, icône d'app). L'indigo et le violet **n'existent pas** comme accents
autonomes dans l'interface — ce sont les fuites du teal dans l'Aurora.

Le fond est **blanc** (`#ffffff`), les surfaces sont des blancs cassés
chauds, le texte est quasi-noir Apple (`#1d1d1f`). Jamais de thème sombre,
jamais d'esthétique néon/IA.

## Icône d'application

**Fond blanc, marque teal** (inversion de la v13 fond-Aurora/marque-blanche —
« la lumière, pas le coffre-fort », anti « crypto-dark »).

`docs/brand/quanta-app-icon.svg` — squircle blanc (rayon 22.4 %), marque en
**Aurora teal-dominant** (le teal tient ~72 %, l'indigo/violet n'embrassent
que la pointe SE = la queue/quantum, là où se produit la finalité), plus un
**joyau doux** (bloom radial `#14C8B8`) au point de scellement (687, 687).
Filet interne 3 px `#0b4a50`@10 % pour définir le bord sur fonds clairs.

`docs/brand/quanta-app-icon-flat.svg` — **variante plate** pour ≤ 64 px
(favicon, badges) : marque `--teal-700 #087F8C` pleine sur blanc (le gradient
devient boueux en petit). Le master Aurora est réservé au ≥ 128 px.
`static/favicon.png` = variante plate 64 px.

Pipeline de régénération :

```bash
qlmanage -t -s 1024 -o /tmp docs/brand/quanta-app-icon.svg
npx tauri icon /tmp/quanta-app-icon.svg.png
```

## Mouvement — le sceau, jamais une boucle

La marque est **statique par défaut**. Elle n'anime **qu'au scellement d'un
bloc** (ou à une récompense) : la couture fleurit en Aurora ~600 ms puis
retombe à plat (`QuantaMark` prop `sealing`, câblée aux events
`quanta://block-sealed` **aux points d'appel**, jamais dans la marque).
**Jamais** de boucle, jamais de spinner, jamais un pouls permanent — c'est le
piège « app de chargement / santé ». Respecte `prefers-reduced-motion`.

## La loi — l'iconique se tient, il ne se redessine pas

Le chemin vers « aussi reconnaissable que Coca-Cola » n'est pas un énième
logo, c'est l'**engagement** : une marque, une couleur, un wordmark, appliqués
**identiques partout, pendant des années**. Coca-Cola n'a jamais changé de
rouge. On ne restyle plus la marque à chaque version.

## Typographie

Inter uniquement. Le wordmark « QUANTA » : Inter 800, approche +0.12 em,
toujours en capitales, toujours à droite ou sous la marque — jamais
verrouillé dans l'anneau.
