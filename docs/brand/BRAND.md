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
  (centrée sur la diagonale 45°).
- Quantum : segment sur la diagonale 45°, du rayon 9.5 au rayon 21.5 —
  il **croise** la ligne de l'anneau à travers la brèche (jamais accolé,
  jamais entièrement dehors : dehors = loupe, accolé = lettre banale).
- Trait : 5.5, `stroke-linecap: round`. Aucun remplissage.

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
vit dans `src/app.css` (`--teal-50 … --teal-900`, accent = `--teal-500
#0BA5A0`, profond = `--teal-700 #087F8C`, éclat = `#14C8B8`).

L'**Aurora** (teal → indigo #3D6FE0 → violet #7C3AED) est *l'artefact* de
marque : un gradient réservé aux moments (accueil, identité, recevoir,
succès, icône d'app). L'indigo et le violet **n'existent pas** comme accents
autonomes dans l'interface — ce sont les fuites du teal dans l'Aurora.

Le fond est **blanc** (`#ffffff`), les surfaces sont des blancs cassés
chauds, le texte est quasi-noir Apple (`#1d1d1f`). Jamais de thème sombre,
jamais d'esthétique néon/IA.

## Icône d'application

`docs/brand/quanta-app-icon.svg` — carré arrondi (rayon 22.4 %) rempli du
gradient Aurora **dominé par le teal** (le violet n'embrasse que le coin
bas-droit), marque blanche centrée à ~62 % de la largeur. Pipeline de
régénération :

```bash
qlmanage -t -s 1024 -o /tmp docs/brand/quanta-app-icon.svg
npx tauri icon /tmp/quanta-app-icon.svg.png
```

## Typographie

Inter uniquement. Le wordmark « QUANTA » : Inter 800, approche +0.12 em,
toujours en capitales, toujours à droite ou sous la marque — jamais
verrouillé dans l'anneau.
