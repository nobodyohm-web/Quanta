# Phase 1 — Wallet + Editor Irréprochables
## Brief Claude Code — Focus Solo Mode

> **Objectif** : Rendre le Wallet et l'Editor parfaits en utilisation solo (1 seul utilisateur,
> pas de réseau). C'est la fondation sur laquelle tout le reste se construit.
> **Règle** : Respecter la Design Bible `.agent/design/sova_design_bible.md` — chaque pixel compte.

---

## AUDIT — État actuel (28 avril 2026)

### Wallet.svelte (226 lignes) — Note : 6/10

**Ce qui marche :**
- ✅ Balance affichée avec `get_my_reputation`
- ✅ Liste de transactions avec `get_recent_txs`
- ✅ Envoi d'ATN avec `ledger_transfer`
- ✅ Direction in/out détectée
- ✅ Feedback d'erreur/succès

**Ce qui est cassé ou manquant :**
- ❌ Bouton "Recevoir" ne fait rien (juste `refresh()`)
- ❌ Bouton "Staker" fait `stake_atn(1)` codé en dur — pas d'UI pour choisir le montant
- ❌ Pas de stats réseau/énergie visibles (commandes `get_energy_stats`, `get_economy_stats` existent mais pas utilisées)
- ❌ Pas de copie de clé publique pour recevoir
- ❌ Pas de variation % (le mockup montre "+12.3% this month")
- ❌ Pas d'info "plancher ATN en EUR" (la commande `atn_floor_eur` existe dans le Rust)
- ❌ Pas de vue "détails de transaction" (hash, signature, bloc)
- ❌ Pas de filtre par type de transaction
- ❌ Le `catch(e: any)` est un anti-pattern Svelte 5
- ❌ Pas de skeleton loading (l'écran est vide pendant le fetch)

### Editor.svelte (710 lignes) — Note : 7/10

**Ce qui marche :**
- ✅ Éditeur bloc par bloc (Notion-style)
- ✅ Slash commands (/)
- ✅ Support code avec sélecteur de langage
- ✅ Auto-save drafts (localStorage)
- ✅ Templates variés (landing, shop, blog, code, etc.)
- ✅ Sérialisation Markdown + HTML
- ✅ Sauvegarde backend (`create_site`, `update_site`)
- ✅ Publication (`start_sync`, `index_site`)

**Ce qui est cassé ou manquant :**
- ❌ Références "TITAN" et "BTC" au lieu de "SOVA" et "ATN" dans les templates
- ❌ `linear-gradient` dans `.comp-hero` (ligne 650) — interdit par la bible
- ❌ `box-shadow` glow sur `.hero-cta:hover` (ligne 661) — interdit
- ❌ `translateY(-2px)` sur le TemplatePicker hover (ligne 44) — interdit
- ❌ `transform: translateY(-1px)` sur bouton CTA (ligne 661) — interdit
- ❌ Couleur `#007AFF` implicite (l'ancien accent bleu "Aurora")
- ❌ Pas de preview du contenu avant publication
- ❌ Pas de compteur de mots/caractères
- ❌ Pas d'indication du mining ATN gagné ("Publication → +1 ATN")
- ❌ Le `catch(e: any)` anti-pattern
- ❌ L'éditeur dit "Site Builder" — devrait être "Éditeur" ou "Créer"

### TemplatePicker.svelte (49 lignes) — Note : 5/10

- ❌ `translateY(-2px)` sur hover — interdit
- ❌ Les templates disent "TITAN", "BTC", "Lightning" — tout doit être "SOVA", "ATN"
- ❌ Pas de description claire des types de contenu (Article vs Site vs Programme)
- ❌ Emoji comme icônes — la bible dit pas d'emoji dans les boutons principaux

---

## PLAN DE TRAVAIL — Priorité par impact

### 1. Wallet.svelte — RÉÉCRIRE (objectif : ~300 lignes)

#### Architecture

```
┌─────────────────────────────────────────────┐
│                 WALLET                       │
│                                             │
│          ┌──────────────────┐               │
│          │    142.50 ATN     │  ← hero       │
│          │   +12.3% total   │               │
│          │ Plancher: 0.003€ │               │
│          └──────────────────┘               │
│                                             │
│  ┌─────────┐ ┌──────────┐ ┌─────────────┐  │
│  │ Envoyer │ │ Recevoir │ │   Staker    │  │
│  └─────────┘ └──────────┘ └─────────────┘  │
│                                             │
│  ┌──── Panel Envoyer (conditionnel) ─────┐  │
│  │ Adresse:  [________________]          │  │
│  │ Montant:  [____] ATN                  │  │
│  │        [  Confirmer l'envoi  ]        │  │
│  └───────────────────────────────────────┘  │
│                                             │
│  ┌──── Panel Recevoir (conditionnel) ────┐  │
│  │ Votre clé publique :                  │  │
│  │ [abc1...f29d]  📋 Copié !             │  │
│  └───────────────────────────────────────┘  │
│                                             │
│  ┌──── Panel Staker (conditionnel) ──────┐  │
│  │ Montant:  [____] ATN                  │  │
│  │ Stakés : 5.0 ATN                      │  │
│  │        [  Staker  ]                   │  │
│  └───────────────────────────────────────┘  │
│                                             │
│  ÉNERGIE                                    │
│  ────────────────────────────────            │
│  Consommation   0.045 kWh                   │
│  ATN minés      12.5 ATN                    │
│  Uptime         340 min                     │
│  Plancher 1 ATN 0.003 EUR                   │
│                                             │
│  ACTIVITÉ                                   │
│  ────────────────────────────────            │
│  Mining         +0.02 ATN          à l'inst │
│  Création       +1.00 ATN          2 min    │
│  Like reçu      +1.00 ATN          5 min    │
│  Transfert      -5.00 ATN          1 h      │
│                                             │
└─────────────────────────────────────────────┘
```

#### Commandes Rust à utiliser

| Commande | Usage |
|----------|-------|
| `get_my_reputation` | Balance, earned, staked |
| `get_balance` | Balance rapide |
| `get_recent_txs` | Liste transactions |
| `get_ledger_stats` | Stats réseau (blocks, total mined, energy) |
| `get_energy_stats` | kWh, ATN minés, uptime |
| `get_public_key` | Clé publique pour "Recevoir" |
| `ledger_transfer` | Envoi ATN |
| `stake_atn` | Staking |
| `verify_ledger` | Vérification intégrité chaîne |

#### Fonctionnalités à implémenter

1. **Hero** : solde 48px + "ATN" 14px + variation % en vert/rouge
2. **Plancher EUR** : afficher `ReputationEngine::atn_floor_eur()` × balance = valeur en EUR
3. **Bouton Recevoir** : afficher sa clé publique avec bouton copier (clipboard API)
4. **Bouton Staker** : ouvrir un panel avec input montant + affichage des ATN déjà stakés
5. **Section ÉNERGIE** : afficher les stats énergie (kWh, minés, uptime, plancher EUR/ATN)
6. **Section ACTIVITÉ** : transactions avec icône par type, filtre possible
7. **Skeleton loading** : pendant le fetch, afficher des blocs pulsants gris
8. **Auto-refresh** : `setInterval(refresh, 30_000)` pour voir le mining en direct
9. **Commande `get_economy_stats`** : si elle existe, l'utiliser pour epoch/halving info

### 2. Editor.svelte — REFACTORER (objectif : ~600 lignes, plus propre)

#### Changements requis

1. **Renommer** : "Site Builder" → "Éditeur" dans la barre
2. **Templates** : remplacer tous les "TITAN" par "SOVA", "BTC" par "ATN"
3. **CSS** : supprimer TOUS les gradients, glow, translateY dans les styles
4. **Colors** : remplacer toute couleur hardcodée (#007AFF, #0d1117) par des vars SOVA
5. **Hero component** : supprimer le `linear-gradient` — fond `#0f0f0f` + border subtile
6. **CTA button** : supprimer `box-shadow` et `translateY` — juste `opacity: 0.9` au hover
7. **Compteur** : ajouter un compteur de mots en bas de la barre (discret, 12px grey)
8. **ATN reward** : afficher "+1 ATN" à côté du bouton "Publish" pour montrer le gain
9. **Preview** : ajouter un toggle "Éditer | Prévisualiser" qui rend le markdown en lecture seule
10. **Focus mode** : quand l'éditeur est actif, la barre d'outils se masque (opacity → apparition au hover)

### 3. TemplatePicker.svelte — REFACTORER

1. Renommer les templates en contexte SOVA :
   - "Landing Page" → "Article"
   - "Boutique" → "Produit à vendre"
   - "Blog" → "Blog / Recherche"
   - "Programme" → "Code source"
   - "Page Vide" → "Page libre"
2. Remplacer "TITAN" → "SOVA" dans tous les textes
3. Remplacer "BTC" → "ATN" dans les prix
4. Supprimer `translateY(-2px)` du hover
5. Titre : "Créer un site" → "Créer du contenu"

---

## CSS TOKENS à utiliser (rappel)

```css
/* Backgrounds */
var(--sova-bg-0): #000000;
var(--sova-bg-1): #0f0f0f;
var(--sova-bg-2): #1a1a1a;

/* Text */
var(--sova-text-0): #ffffff;
var(--sova-text-1): #a0a0a0;
var(--sova-text-2): #666666;

/* Accent */
var(--sova-accent): #00DC82;
var(--sova-negative): #FF4444;

/* Border */
var(--sova-border): rgba(255,255,255,0.06);

/* Radius */
var(--radius-sm): 8px;
var(--radius): 12px;
```

---

## INTERDIT — Rappel express

1. ❌ `linear-gradient()` pour décoration
2. ❌ `backdrop-filter: blur()`
3. ❌ `box-shadow` coloré (glow)
4. ❌ `transform: translateY()` sur hover
5. ❌ Animations > 0.3s
6. ❌ Emoji dans les boutons principaux
7. ❌ Texte < 11px
8. ❌ Fond teinté (#0a0a0f) — noir pur
9. ❌ Références "TITAN", "BTC", "Lightning"

---

## TESTS — Scénarios solo à valider

### Wallet
1. ✅ L'app démarre → le mining commence → le solde augmente toutes les 60s
2. ✅ Cliquer "Recevoir" → la clé publique s'affiche → clic copie dans le presse-papier
3. ✅ Cliquer "Envoyer" → remplir adresse + montant → confirmer → feedback OK
4. ✅ Cliquer "Staker" → choisir montant → staker → le solde staké s'affiche
5. ✅ La section ÉNERGIE affiche des chiffres qui augmentent avec le temps
6. ✅ La liste de transactions montre Mining, Creation, etc. avec bonnes couleurs

### Editor
1. ✅ Choisir "Article" → l'éditeur s'ouvre avec un template vide propre
2. ✅ Taper `/` → le menu slash apparaît → sélectionner un type de bloc
3. ✅ Ajouter du texte + heading + code → le markdown est bien formé
4. ✅ Cliquer "Save" → le site est créé dans le backend
5. ✅ Cliquer "Publish" → le site est indexé → "+1 ATN" affiché
6. ✅ Fermer l'éditeur → le brouillon est sauvé → rouvrir → le brouillon revient
7. ✅ Compteur de mots visible et correct

---

## ORDRE D'EXÉCUTION

```
1. app.css          → Vérifier que tous les tokens SOVA existent
2. Wallet.svelte    → RÉÉCRIRE selon les specs ci-dessus
3. Editor.svelte    → REFACTORER (CSS + renommages + preview)
4. TemplatePicker   → REFACTORER (renommages)
5. templates.ts     → Remplacer TITAN→SOVA, BTC→ATN
6. npm run ai:check → 0 errors, 0 warnings
```
