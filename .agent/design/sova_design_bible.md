# 🏛️ SOVA Design Bible — v7 "Perfection"
## Le document de référence ABSOLU pour Claude Code

> Ce document est le résultat d'une recherche approfondie sur les systèmes de design de Apple (HIG 2026), Google (Material Design 3), Trade Republic, Linear, Stripe, Vercel, Ledger Live, Phantom, et les principes de Dieter Rams. Chaque décision est justifiée par une source.

---

## Table des matières
1. [Philosophie de Design](#1-philosophie)
2. [Références Visuelles (Mockups)](#2-mockups)
3. [Système de Couleurs](#3-couleurs)
4. [Typographie](#4-typographie)
5. [Spacing & Layout](#5-spacing)
6. [Composants — Spécifications Pixel-Perfect](#6-composants)
7. [Motion & Animations](#7-motion)
8. [Icônes](#8-icones)
9. [Fichiers à Modifier — Instructions Détaillées](#9-fichiers)
10. [Anti-Patterns — Liste Exhaustive](#10-antipatterns)
11. [Checklist Qualité](#11-checklist)

---

## 1. Philosophie de Design {#1-philosophie}

### Les 5 Commandements SOVA (inspirés de Dieter Rams + Apple HIG)

#### I. "Less, but better" — Rams #10
> "Good design is as little design as possible. Back to purity, back to simplicity."

Chaque pixel doit justifier son existence. Si un élément ne communique pas une information actionnable, il n'existe pas. Pas de décorations. Pas d'éléments "pour faire joli". Le design est un outil, pas une œuvre d'art.

#### II. "Clarity" — Apple HIG Principle #1
> "Interfaces must be legible, precise, and unambiguous."

L'utilisateur doit comprendre chaque écran en 3 secondes. Les labels sont en langage humain (pas technique). Les nombres sont grands et lisibles. La hiérarchie est immédiate : le regard va d'abord au chiffre le plus important.

#### III. "Deference" — Apple HIG Principle #2
> "The UI should support and highlight content, not distract from it."

Le design ne doit PAS avoir de "personnalité". Il doit être invisible, comme iOS. On ne pense pas au design, on utilise l'outil. Toute l'attention va au **contenu** : le solde, les posts, les transactions.

#### IV. "Calm Design" — Institutional Crypto UX
> "Generous white space (even in dark mode) to reduce visual clutter and cognitive load."

L'utilisateur manipule de l'argent. Le design doit inspirer **calme et confiance**, comme un coffre-fort suisse. Pas d'urgence visuelle, pas de FOMO, pas de couleurs flashy. Le vert ne célèbre pas — il informe.

#### V. "Honest Design" — Rams #6
> "UI should not manipulate the user."

Pas de dark patterns. Pas d'animations qui distraient de l'action. Pas de chiffres mis en valeur artificiellement. Le design est transparent : ce que vous voyez est ce que vous possédez.

---

## 2. Références Visuelles (Mockups) {#2-mockups}

> **IMPORTANT pour Claude Code** : Ces mockups sont la cible EXACTE. Chaque composant doit reproduire ces designs pixel par pixel.

### Welcome Screen (Onboarding Apple-style)
![Welcome — fond noir pur, headline 3 lignes, 1 bouton vert](/Users/alex/.gemini/antigravity/brain/d63ba1cd-ebf5-42d6-a672-fb8d1c6b106b/artifacts/sova_welcome_screen_1777363408843.png)

**Spécifications** :
- Fond : `#000000` pur
- "SOVA" : 14px, uppercase, letter-spacing 0.15em, `#666666`
- Headline : 32-36px, weight 700, blanc `#ffffff`, letter-spacing -0.03em, line-height 1.2
- Sous-titre : 15px, `#a0a0a0`, line-height 1.6
- Bouton : fond `#00DC82`, texte `#000000`, weight 600, padding 14px 48px, border-radius 8px
- Aucun autre élément sur l'écran

### Feed Screen (Timeline social)
![Feed — posts en colonne, séparateurs ultra-fins, bottom bar](/Users/alex/.gemini/antigravity/brain/d63ba1cd-ebf5-42d6-a672-fb8d1c6b106b/artifacts/sova_feed_screen_1777363397301.png)

**Spécifications** :
- TopBar : "SOVA" bold blanc à gauche, "730.49 ATN ●" à droite, hauteur 48px
- Posts : séparés par des lignes `rgba(255,255,255,0.06)`, PAS de cards avec bordures
- Avatar : cercle 36px, fond `#1a1a1a`, initiales 12px weight 700 blanc
- Username : 13px, monospace, blanc
- Timestamp : 13px, `#666666`
- Titre du post : 16px, weight 600, blanc, letter-spacing -0.01em
- Actions : ♡ + count, 💬 + count, ↗ share — tout en `#666666`, 13px
- Reward ATN : aligné à droite, 12px, `#666666`
- Bottom bar : 5 items, fond `#0f0f0f`, bouton Create = cercle vert

### Wallet Screen (Trade Republic style)
![Wallet — gros solde central, 3 boutons, liste transactions](/Users/alex/.gemini/antigravity/brain/d63ba1cd-ebf5-42d6-a672-fb8d1c6b106b/artifacts/sova_wallet_screen_1777363383640.png)

**Spécifications** :
- Solde : 48px, weight 700, monospace, blanc, centré, letter-spacing -0.03em
- "ATN" : 14px, `#666666`, uppercase, spacing 0.06em
- Variation : 13px, `#00DC82` si positif, `#FF4444` si négatif
- 3 boutons : fond `#141414`, border `rgba(255,255,255,0.06)`, radius 12px, icône 18px + label 12px
- "ACTIVITY" : section label 11px, uppercase, `#666666`, spacing 0.06em
- Chaque transaction : ligne avec label+time à gauche, montant à droite
- Montant entrant : `#00DC82` — Montant sortant : `#a0a0a0`
- Séparateurs : `rgba(255,255,255,0.06)`

### Profile Screen (Mon Compte)
![Profile — avatar, stats grid, sections info](/Users/alex/.gemini/antigravity/brain/d63ba1cd-ebf5-42d6-a672-fb8d1c6b106b/artifacts/sova_profile_screen_1777363440517.png)

**Spécifications** :
- Titre : "Mon compte", 28px, weight 700, letter-spacing -0.03em
- Avatar : cercle 48px, fond `#2a2a2a`, initiales 16px weight 700
- Clé publique : monospace, 13px, tronquée, cliquable (copie)
- Stats : 3 cards en grille, fond `#0f0f0f`, border subtile, valeur 24px bold, label 10px uppercase
- Sections : "SÉCURITÉ" et "ACTIVITÉ" — label 11px uppercase `#666666`
- Lignes info : label à gauche `#a0a0a0`, valeur à droite `#ffffff`, padding 14px 0

---

## 3. Système de Couleurs {#3-couleurs}

### Pourquoi ces couleurs (avec sources)

| Token | Valeur | Justification |
|-------|--------|---------------|
| `--sova-bg-0` | `#000000` | Apple utilise le noir pur sur OLED pour un contraste maximal et des bords "infinis" |
| `--sova-bg-1` | `#0f0f0f` | Stripe/Linear : surface de card à peine plus claire, crée de la profondeur sans couleur |
| `--sova-bg-2` | `#1a1a1a` | Trade Republic : inputs et surfaces élevées — assez clair pour être distingué, assez sombre pour ne pas distraire |
| `--sova-bg-3` | `#2a2a2a` | Material Design 3 : bordures et séparateurs — "surface fill opacities to represent depth" |
| `--sova-text-0` | `#ffffff` | Apple HIG : blanc pur pour le texte primaire — contraste ratio 21:1 sur noir |
| `--sova-text-1` | `#a0a0a0` | Ratio 7.4:1 — dépasse WCAG AAA (7:1) pour le texte secondaire |
| `--sova-text-2` | `#666666` | Ratio 4.7:1 — dépasse WCAG AA (4.5:1) pour labels tertiaires |
| `--sova-accent` | `#00DC82` | Trade Republic, Robinhood, Wise — LE vert finance. Croissance, gains, positif. |
| `--sova-negative` | `#FF4444` | Pertes, erreurs — rouge standard finance (pas rose, pas corail) |
| `--sova-warning` | `#FFB800` | Ambre standard — avertissements |
| `--sova-border` | `rgba(255,255,255,0.06)` | Linear app : "subtle borders to create hierarchy" — presque invisible |

### Règle absolue
**UNE SEULE couleur d'accent : le vert `#00DC82`.** Pas de deuxième couleur. Le vert sert UNIQUEMENT pour :
- Gains financiers positifs
- Bouton CTA principal
- Status "en ligne" / "vérifié"
- Le bouton "Create" dans la nav

Le rouge sert UNIQUEMENT pour les pertes et les erreurs. L'ambre UNIQUEMENT pour les warnings. Tout le reste est monochrome (noir/gris/blanc).

---

## 4. Typographie {#4-typographie}

### Source : Apple HIG + Vercel/Geist

**UNE SEULE font : Inter** (le SF Pro du web, optiquement optimisé, variable).
JetBrains Mono UNIQUEMENT pour les valeurs numériques financières et les clés publiques.

### Échelle typographique (EXACTE)

| Rôle | Taille | Poids | Letter-spacing | Usage |
|------|--------|-------|----------------|-------|
| Hero number | 48px | 700 | -0.03em | Solde wallet central |
| Page title | 28px | 700 | -0.03em | Titres de page |
| Card value | 24px | 700 | -0.02em | Stats dans les cards |
| Post title | 16px | 600 | -0.01em | Titre de post dans le feed |
| Body | 14px | 400 | 0 | Texte courant |
| Small | 13px | 500 | 0 | Sous-titres, timestamps |
| Caption | 12px | 500 | 0 | Labels d'action, temps relatif |
| Section label | 11px | 600 | 0.06em | UPPERCASE — titres de section |
| Overline | 10px | 600 | 0.06em | UPPERCASE — labels de stats |

### Règles typographiques (Apple HIG)
- **Minimum 11px** pour tout texte — rien de plus petit (accessibilité)
- **Hiérarchie par poids**, pas par changement de font (Apple: "use weight and color to distinguish")
- **Negative tracking sur les titres** (-0.03em) — Apple: "tighter letter-spacing for display sizes"
- **Positive tracking sur les labels** (+0.06em uppercase) — Apple: "looser spacing for small caps"
- **font-feature-settings: 'cv11', 'ss01'** — Inter stylistic sets pour chiffres propres

---

## 5. Spacing & Layout {#5-spacing}

### Source : Material Design 3 (4dp grid) + Linear (8px)

**Grille de base : 4px.** Tous les spacings sont des multiples de 4.

| Token | Valeur | Usage |
|-------|--------|-------|
| `--space-1` | 4px | Micro-gaps (entre icône et texte) |
| `--space-2` | 8px | Petit gap (entre éléments liés) |
| `--space-3` | 12px | Gap moyen |
| `--space-4` | 16px | Gap standard (padding de card) |
| `--space-5` | 20px | Padding horizontal de page |
| `--space-6` | 24px | Séparation entre sections |
| `--space-8` | 32px | Grand espacement |
| `--space-10` | 40px | Padding vertical de page |
| `--space-12` | 48px | Séparation majeure |

### Touch targets (Apple HIG)
**44×44 points minimum** pour tout élément interactif. Les boutons, les liens, les items de liste — rien en dessous.

### Border radius (Apple squircle)
| Token | Valeur | Usage |
|-------|--------|-------|
| `--radius-sm` | 8px | Boutons, badges, inputs |
| `--radius` | 12px | Cards, containers |
| `--radius-lg` | 16px | Modales, grandes cards |
| `--radius-xl` | 24px | Hero sections (rarement) |

---

## 6. Composants — Spécifications Pixel-Perfect {#6-composants}

### TopBar
- **Hauteur** : 48px
- **Fond** : `#000000` (même que le fond de page — invisible)
- **Bord bas** : `rgba(255,255,255,0.06)`, 1px
- **Gauche** : "SOVA" — 16px, weight 700, spacing 0.08em, blanc
- **Droite** : Solde ATN (monospace 13px, weight 600) + dot status (6px) + bouton aide (cercle 24px, border 1px)
- **PAS de glassmorphism, PAS de blur**

### NavBar (Bottom bar)
- **Hauteur** : 56px + safe-area
- **Fond** : `#0f0f0f`
- **Bord haut** : `rgba(255,255,255,0.06)`, 1px
- **5 items** : flex equal, centré vertical
- **Icône** : 20px, poids 300
- **Label** : 10px, weight 500
- **Couleur inactive** : `#666666`
- **Couleur active** : `#ffffff`
- **Create** : cercle 28px, fond `#00DC82`, texte `#000000`, "+" en 18px weight 600
- **Transition** : color 0.15s ease

### PostCard (dans le Feed)
- **PAS de card bordurée** — juste des séparateurs horizontaux entre posts
- **Séparateur** : `rgba(255,255,255,0.06)`, 1px
- **Padding** : 20px 0 (vertical uniquement)
- **Avatar** : cercle 36px, fond `#1a1a1a`, initiales 12px weight 700
- **Actions** : ♡ count · 💬 count · ↗ — en `#666666`, hover → `#ffffff`
- **Hover sur le post** : background → `#0f0f0f`, avec padding horizontal -20px/+20px

### Wallet Hero
- **Solde** : centré, 48px monospace, weight 700
- **"ATN"** : 14px, `#666666`, uppercase, spacing 0.06em, margin-top 4px
- **Variation** : 13px, weight 500, margin-top 12px, vert si positif, rouge si négatif
- **Actions** : 3 boutons flex, fond `#141414`, border subtile, radius 12px, 12px 24px padding

### Transaction Row
- **Padding** : 14px 0
- **Séparateur** : border-bottom `rgba(255,255,255,0.06)`, 1px
- **Label** : 14px, weight 500, blanc
- **Time** : 12px, `#666666`
- **Montant** : 14px monospace, weight 600
- **Entrant** : `#00DC82` avec "+"
- **Sortant** : `#a0a0a0` avec "-"

### Stat Card (Profile)
- **Fond** : `#0f0f0f`
- **Border** : 1px `rgba(255,255,255,0.06)`
- **Radius** : 12px
- **Padding** : 20px 16px
- **Valeur** : 24px, weight 700, centré
- **Label** : 10px, weight 600, uppercase, spacing 0.06em, `#666666`

### Info Row (Profile sections)
- **Padding** : 14px 0
- **Séparateur** : border-bottom comme partout
- **Label** : 14px, `#a0a0a0`
- **Valeur** : 14px, weight 500, `#ffffff`, flex end
- **Avec dot status** : cercle 6px inline, `#00DC82` ou `#3a3a3a`

---

## 7. Motion & Animations {#7-motion}

### Source : Material Design 3 "Motion with Purpose" + Apple HIG "Reduce Motion"

**Règle** : Les animations ne sont PAS décoratives. Elles **communiquent** un changement d'état.

| Type | Durée | Easing | Usage |
|------|-------|--------|-------|
| Hover | 0.15s | ease | Borders, couleurs, backgrounds |
| Page entrance | 0.15s | ease-out | `opacity: 0 → 1` (fadeIn) |
| Éléments de liste | 0.12s | ease | Background au hover |
| Modales | 0.15s | ease-out | Apparition overlay |

### Ce qui est INTERDIT (source : Rams #5 "unobtrusive")
- Animations de plus de 0.3s
- `translateY` ou `scale` sur les transitions de page
- Keyframes nommés `aurora-shift`, `heart-pop`, `count-up`, `pulse-dot`
- `backdrop-filter: blur()`
- Toute animation en boucle infinie (sauf skeleton shimmer sur le loading)

---

## 8. Icônes {#8-icones}

### Style : SF Symbols / Phosphor Icons (outline)
- **Stroke** : 1.5px
- **Taille** : 20×20px dans la nav, 18×18 dans les boutons
- **Couleur** : hérite du texte
- **PAS d'emoji** dans la navigation ou les boutons

Pour la bottom bar, utiliser des caractères unicode simples ou des SVG inline :
- Home : `⌂` ou SVG house
- Explore : `○` ou SVG compass
- Create : `+` (dans cercle vert)
- Wallet : `◇` ou SVG wallet
- Profile : `●` ou SVG person

---

## 9. Fichiers à Modifier — Instructions Détaillées {#9-fichiers}

### P0 — Fondations (faire en premier)

| # | Fichier | Action | Détails |
|---|---------|--------|---------|
| 1 | `src/app.html` | Modifier | Titre "SOVA", supprimer font Outfit du Google Fonts link |
| 2 | `src/app.css` | **RÉÉCRIRE** | Implémenter TOUT le design system SOVA ci-dessus |
| 3 | `src/lib/NavBar.svelte` | **CRÉER** | Bottom bar iOS 56px, 5 items, create en cercle vert |
| 4 | `src/lib/TopBar.svelte` | **RÉÉCRIRE** | Ultra-fin 48px, SOVA + solde + dot + aide |

### P1 — Écrans principaux

| # | Fichier | Action | Détails |
|---|---------|--------|---------|
| 5 | `src/lib/Welcome.svelte` | **RÉÉCRIRE** | Voir mockup : fond noir, 3 lignes, 1 bouton vert |
| 6 | `src/lib/Feed.svelte` | **RÉÉCRIRE** | Colonne unique, pas de cards bordurées, juste des séparateurs |
| 7 | `src/lib/Wallet.svelte` | **RÉÉCRIRE** | Voir mockup : solde 48px, 3 boutons, liste transactions |
| 8 | `src/lib/Dashboard.svelte` | **RÉÉCRIRE** | Voir mockup : avatar, stats grid, sections info |

### P2 — Layout & configuration

| # | Fichier | Action | Détails |
|---|---------|--------|---------|
| 9 | `src/routes/+page.svelte` | Modifier | Layout vertical : TopBar → main → NavBar. Plus de Sidebar. |
| 10 | `src-tauri/tauri.conf.json` | Modifier | `"title": "SOVA"` |
| 11 | `src/lib/Browser.svelte` | Modifier | Adapter les couleurs au design system SOVA |
| 12 | `src/lib/Editor.svelte` | Modifier | Adapter les couleurs au design system SOVA |
| 13 | `src/lib/Settings.svelte` | Modifier | Adapter les couleurs au design system SOVA |

### NE PAS TOUCHER
- `src-tauri/src/**` — Tout le backend Rust reste intact
- `src/lib/prefs.ts` — Logique de préférences inchangée
- `src/lib/templates.ts` — Templates inchangés
- `src/lib/StrengthMeter.svelte` — Déjà sobre
- `src/lib/HelpModal.svelte` — OK si couleurs adaptées
- `src/lib/CommandPalette.svelte` — OK si couleurs adaptées

---

## 10. Anti-Patterns — JAMAIS (avec explication) {#10-antipatterns}

| # | Interdit | Pourquoi (source) |
|---|----------|-------------------|
| 1 | `background: linear-gradient(...)` pour décoration | Rams #5: "design should be unobtrusive" |
| 2 | `backdrop-filter: blur(...)` glassmorphism | Apple 2026 "Liquid Glass" est réservé au système, pas aux apps |
| 3 | `box-shadow: 0 0 Npx rgba(color...)` glow | Stripe: "avoid fancy design gimmicks; focus on the data" |
| 4 | Font Outfit ou toute deuxième font | Apple: "SF Pro only" — une seule famille pour la cohérence |
| 5 | Plus d'un accent | Trade Republic: palette limité = confiance |
| 6 | Sidebar de navigation | Linear mobile + iOS: bottom bar est le standard 2026 |
| 7 | SVG pattern identicons | Institutional crypto: "calm design" — cercle + initiales |
| 8 | Animations > 0.3s | Material Design 3: "motion as utility, not decoration" |
| 9 | `animation: aurora-shift` ou similaire | Rams #10: "as little design as possible" |
| 10 | Fond `#0a0a0f` ou teinté bleu/violet | Apple OLED: noir pur `#000000` uniquement |
| 11 | Texte < 11px | Apple HIG: minimum 11pt pour l'accessibilité |
| 12 | Boutons sans 44×44 touch target | Apple HIG: "minimize error rates" |
| 13 | `font-family: var(--font-display)` | Supprimé. Inter only. |
| 14 | Classes `.glass`, `.aurora-bg`, `.card-aurora` | Mortes. Supprimées du CSS. |

---

## 11. Checklist Qualité — Avant chaque commit {#11-checklist}

### Design
- [ ] Fond de page = `#000000` pur
- [ ] Aucun gradient visible
- [ ] Aucun blur/glassmorphism
- [ ] Aucun glow coloré
- [ ] Un seul accent vert `#00DC82`
- [ ] Tous les textes ≥ 11px
- [ ] Section labels en UPPERCASE + 0.06em spacing
- [ ] Spacing en multiples de 4px
- [ ] Hierarchy par poids de font, pas par changement de famille
- [ ] Bottom bar présente (pas de sidebar)

### Code
- [ ] `npm run ai:check` → 0 errors, 0 warnings
- [ ] Svelte 5 Runes (`$state`, `$derived`, `$effect`, `$props`)
- [ ] Pas de `{@html}` sans DOMPurify
- [ ] Pas de `any` sur les retours `invoke()`
- [ ] Pas d'import de Sidebar.svelte

### Accessibilité (Apple HIG)
- [ ] Tous les boutons ont un touch target ≥ 44×44
- [ ] Tous les inputs ont un label
- [ ] Contraste ratio ≥ 4.5:1 pour tout texte
- [ ] `role` et `aria-label` sur les éléments interactifs
- [ ] Support de `prefers-reduced-motion`

---

## 12. 🌐 VISION INTERNET V3 — CE QUE SOVA EST VRAIMENT {#12-vision}

> ⚠️ **SOVA n'est PAS juste un wallet avec un feed.** C'est un **Internet V3 complet** :
> réseau social + moteur de recherche + marketplace + outil de publication.
> Pense : **Substack + DuckDuckGo + Gumroad + Twitter** — le tout décentralisé, P2P, zero cloud.

### Ce que chaque utilisateur peut faire :
1. **Publier** : articles, recherches, programmes, sites web, produits à vendre
2. **Chercher** : moteur de recherche P2P avec BM25 ranking, filtres par type de contenu
3. **Interagir** : liker, commenter, tiper en ATN, suivre des créateurs
4. **Monétiser** : chaque vue = ATN, chaque like = ATN, vente directe de produits
5. **Profil créateur** : vitrine Substack-style avec articles, produits, stats

### Les 5 piliers fonctionnels

| Pilier | Inspiré de | Composant Svelte |
|--------|-----------|-----------------|
| **Publication** | Substack + Medium | `Editor.svelte` (déjà existe) |
| **Découverte** | Kagi + DuckDuckGo | `Browser.svelte` (moteur de recherche) |
| **Social** | Twitter + Farcaster | `Feed.svelte` + `PostCard.svelte` |
| **Finance** | Trade Republic | `Wallet.svelte` |
| **Profil créateur** | Substack homepage | `UserProfile.svelte` |

---

## 13. Écrans Internet V3 — Nouveaux Mockups {#13-mockups-v3}

### Search / Explorer (moteur de recherche P2P)
![Search — barre de recherche, résultats avec types de contenu, filtres, créateurs](/Users/alex/Desktop/Torus/.agent/design/sova_search_screen_1777364028647.png)

**Spécifications** :
- **Barre de recherche** : centrée en haut, fond `#1a1a1a`, border subtile, radius 8px, placeholder "Rechercher sur le réseau..."
- **Filtres** : colonne gauche ou chips horizontaux — "Tout", "Articles", "Produits", "Code", "Sites"
- Filtre actif : border `#00DC82`
- **Résultats** : liste clean, chaque résultat =
  - Titre : 16px, weight 600, blanc
  - URL/auteur : 13px, `#00DC82`
  - Extrait : 14px, `#a0a0a0`, max 2 lignes
  - Stats à droite : vues + valeur ATN, 12px, `#666666`
- **Créateurs populaires** : sidebar ou section "TOP CREATORS" avec avatars-cercle + nom
- **Commandes Rust disponibles** : `peer_query`, `get_all_sites`, `index_site`

### Article Reader (Substack-style)
![Article — lecteur Substack, auteur, actions, commentaires, tip ATN](/Users/alex/Desktop/Torus/.agent/design/sova_article_screen_1777364042304.png)

**Spécifications** :
- **Header** : back arrow + "SOVA" centré
- **Auteur** : avatar-cercle 40px + nom 14px bold + date 12px grey + bouton "Follow" (outline vert)
- **Titre** : 28px, weight 700, letter-spacing -0.03em, blanc
- **Corps** : 16px, `#a0a0a0`, line-height 1.8, **max-width 680px**, centré — COMME Substack
- **Barre d'actions** : ♡ count · 💬 comments · ↗ share · "Tip ATN" (bouton outline vert)
- **Commentaires** : liste avec avatar + nom + temps + texte, séparateurs subtils
- **Footer** : "+2.5 ATN earned" en vert discret
- **Commandes Rust disponibles** : `get_site`, `like_content`, `record_view`

### Creator Profile (vitrine Substack)
![Creator — profil public avec tabs Articles/Products/About, grille de contenu](/Users/alex/Desktop/Torus/.agent/design/sova_creator_profile_1777364053469.png)

**Spécifications** :
- **Avatar** : cercle 72px, fond `#2a2a2a`, initiales
- **Nom** : 20px, weight 700, blanc
- **Bio** : 14px, `#a0a0a0`
- **Stats** : "12 articles · 340 likes · 45.2 ATN earned" en 13px `#666666`
- **Actions** : "Follow" (fond vert, texte noir) + "Tip" (outline vert)
- **Tabs** : "Articles | Products | About" — tab actif = soulignement vert `#00DC82`
- **Grille** : 2 colonnes de cards avec titre + extrait + date + views/likes
- **Commandes Rust disponibles** : `get_user_profile`, `get_all_sites`, `like_content`

---

## 14. Architecture des Interactions Sociales {#14-social}

### Types de contenu publiable
| Type | Icône | Description | ATN Mining |
|------|-------|-------------|-----------|
| Article | 📄 | Texte long, markdown, images | Création: +1 ATN, vue: +0.1, like: +1 |
| Recherche | 🔬 | Papier/analyse technique | Création: +2 ATN (bonus qualité) |
| Programme | 💻 | Code source, outil, script | Création: +1 ATN, download: +0.5 |
| Produit | 🏷️ | À vendre contre ATN | Commission: 0% (P2P direct) |
| Site web | 🌐 | Page HTML hébergée en P2P | Création: +1 ATN, vue: +0.1 |

### Interactions sociales
| Action | Qui | Résultat | ATN |
|--------|-----|----------|-----|
| Like (♡) | Lecteur | Signal de qualité | Auteur: +1 ATN, Lecteur: -0 |
| Comment (💬) | Lecteur | Discussion sous le contenu | Auteur: notifié |
| Tip | Lecteur | Transfert volontaire d'ATN | Direct P2P |
| Follow | Lecteur | Le contenu apparaît dans son feed | — |
| Share (↗) | Lecteur | Copie le lien P2P | — |
| View | Automatique | Compteur de vues | Auteur: +0.1 ATN |
| Star ⭐ | Communauté | Contenu mis en avant | Auteur: badge "Featured" |

### Système de profil et réputation
- **Trust Score** : calculé par le backend Rust (qualité du contenu, likes reçus, ancienneté)
- **Badges** : affichés sur le profil (créateur, contributeur, early adopter)
- **Leaderboard** : classement des créateurs par ATN gagnés ou contenu le plus vu
- **Commandes Rust** : `get_trust_leaderboard`, `get_my_reputation`, `get_user_profile`

---

## 15. Backend Rust — Commandes Tauri Disponibles {#15-backend}

> Claude Code DOIT utiliser ces commandes existantes. **NE PAS inventer de nouvelles commandes** sans vérifier.

### Identité
| Commande | Arguments | Retour |
|----------|-----------|--------|
| `check_identity` | — | `boolean` |
| `create_identity` | `displayName, password` | `{ public_key_hex }` |
| `unlock_identity` | `password` | `{ public_key_hex }` |
| `get_public_key` | — | `string` |
| `get_recovery_key` | — | `string` |

### Contenu
| Commande | Arguments | Retour |
|----------|-----------|--------|
| `create_site` | `title, content, template` | `SiteToken` |
| `update_site` | `id, title, content` | `SiteToken` |
| `delete_site` | `id` | `()` |
| `get_site` | `id` | `SiteToken` |
| `get_all_sites` | — | `Vec<SiteToken>` |
| `index_site` | `id` | `()` |

### Social
| Commande | Arguments | Retour |
|----------|-----------|--------|
| `like_content` | `siteId` | `()` |
| `record_view` | `siteId` | `()` |
| `get_leaderboard` | — | `{ tokens, total_network_views }` |
| `get_user_profile` | `publicKey` | `UserProfile` |
| `get_trust_leaderboard` | — | `Vec<TrustEntry>` |
| `report_user` | `publicKey, reason` | `()` |

### Réseau P2P
| Commande | Arguments | Retour |
|----------|-----------|--------|
| `peer_query` | `query` | `Vec<SearchResult>` |
| `get_node_status` | — | `{ is_online, peer_count }` |
| `get_node_ticket` | — | `string` |
| `start_sync` | — | `()` |
| `stop_sync` | — | `()` |

### Finance (ATN)
| Commande | Arguments | Retour |
|----------|-----------|--------|
| `get_my_reputation` | — | `Reputation` |
| `get_balance` | — | `f64` |
| `ledger_transfer` | `to, amount` | `()` |
| `transfer_atn` | `to, amount` | `()` |
| `stake_atn` | `amount` | `()` |
| `get_ledger_stats` | — | `LedgerStats` |
| `get_recent_txs` | — | `Vec<LedgerTx>` |
| `verify_ledger` | — | `VerifyResult` |
| `burn_for_boost` | `amount` | `()` |

### Notifications
| Commande | Arguments | Retour |
|----------|-----------|--------|
| `get_notifications` | — | `Vec<Notification>` |
| `mark_notifications_read` | — | `()` |

---

## 16. Fichiers à Modifier — Plan COMPLET Internet V3 {#16-fichiers-v3}

### P0 — Fondations design (DÉJÀ FAIT si la bible est respectée)
| # | Fichier | Action |
|---|---------|--------|
| 1 | `src/app.html` | Titre "SOVA", Inter only |
| 2 | `src/app.css` | Design system SOVA complet |
| 3 | `src/lib/NavBar.svelte` | Bottom bar iOS-style |
| 4 | `src/lib/TopBar.svelte` | Barre fine SOVA |

### P1 — Core Social (le plus important)
| # | Fichier | Action | Mockup |
|---|---------|--------|--------|
| 5 | `src/lib/Feed.svelte` | **RÉÉCRIRE** | Voir mockup Feed — timeline sociale avec interactions |
| 6 | `src/lib/PostCard.svelte` | **RÉÉCRIRE** | Card de post avec ♡ 💬 ↗ Tip, avatar, auteur |
| 7 | `src/lib/UserProfile.svelte` | **RÉÉCRIRE** | Voir mockup Creator — vitrine Substack avec tabs |
| 8 | `src/lib/Browser.svelte` | **RÉÉCRIRE** | Voir mockup Search — moteur de recherche P2P |

### P2 — Publication & Lecture
| # | Fichier | Action | Mockup |
|---|---------|--------|--------|
| 9 | `src/lib/Editor.svelte` | **REFACTORER** | Éditeur de contenu sobre (article/produit/site) |
| 10 | Nouveau : `src/lib/ArticleReader.svelte` | **CRÉER** | Voir mockup Article — lecteur Substack-style |

### P3 — Finance & Profil
| # | Fichier | Action | Mockup |
|---|---------|--------|--------|
| 11 | `src/lib/Wallet.svelte` | **RÉÉCRIRE** | Voir mockup Wallet — Trade Republic |
| 12 | `src/lib/Dashboard.svelte` | **RÉÉCRIRE** | Voir mockup Profile — Mon Compte |

### P4 — Layout & configuration
| # | Fichier | Action |
|---|---------|--------|
| 13 | `src/routes/+page.svelte` | Layout vertical, routing vers ArticleReader |
| 14 | `src/lib/Settings.svelte` | Adapter couleurs SOVA |
| 15 | `src/lib/CommandPalette.svelte` | Adapter couleurs SOVA |
| 16 | `src/lib/NotificationBell.svelte` | Adapter + intégrer dans TopBar |

### Composants à SUPPRIMER ou DÉPRÉCIER
- `Sidebar.svelte` — remplacé par NavBar
- `BootSequence.svelte` — remplacé par loading screen simple
- `ConstellationGraph.svelte` — trop flashy, supprimé
- `OrbitalAvatar.svelte` — remplacé par cercle + initiales
- `LiveCounter.svelte` — animation interdite
- `BadgeForge.svelte` — trop complexe, simplifier
- `ActivityHeatmap.svelte` — pas dans la vision V3
- `Sparkline.svelte` — remplacer par texte + couleur vert/rouge

### NE PAS TOUCHER
- `src-tauri/src/**` — Tout le backend Rust
- `src/lib/prefs.ts` — Logique de préférences
- `src/lib/templates.ts` — Templates de site

---

## 11. Checklist Qualité — Avant chaque commit {#11-checklist}

### Design
- [ ] Fond de page = `#000000` pur
- [ ] Aucun gradient visible
- [ ] Aucun blur/glassmorphism
- [ ] Aucun glow coloré
- [ ] Un seul accent vert `#00DC82`
- [ ] Tous les textes ≥ 11px
- [ ] Section labels en UPPERCASE + 0.06em spacing
- [ ] Spacing en multiples de 4px
- [ ] Hierarchy par poids de font, pas par changement de famille
- [ ] Bottom bar présente (pas de sidebar)

### Fonctionnel (Internet V3)
- [ ] Le Feed montre de vrais posts avec actions (♡ 💬 ↗ Tip)
- [ ] Le Search/Browser permet de chercher et filtrer par type de contenu
- [ ] Le UserProfile affiche la vitrine créateur avec tabs Articles/Products
- [ ] L'Editor permet de publier au moins un article markdown
- [ ] Le Wallet affiche les transactions liées au contenu social
- [ ] Les profils ont Follow + Tip

### Code
- [ ] `npm run ai:check` → 0 errors, 0 warnings
- [ ] Svelte 5 Runes (`$state`, `$derived`, `$effect`, `$props`)
- [ ] Pas de `{@html}` sans DOMPurify
- [ ] Pas de `any` sur les retours `invoke()`
- [ ] Pas d'import de Sidebar.svelte

### Accessibilité (Apple HIG)
- [ ] Tous les boutons ont un touch target ≥ 44×44
- [ ] Tous les inputs ont un label
- [ ] Contraste ratio ≥ 4.5:1 pour tout texte
- [ ] `role` et `aria-label` sur les éléments interactifs
- [ ] Support de `prefers-reduced-motion`
