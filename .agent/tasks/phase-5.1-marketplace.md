# Phase 5.1 — Marketplace ATN & Contenu Premium

## Objectif
Permettre aux créateurs de vendre l'accès à leurs sites premium en ATN.
Paywall 100% local (vérification sur le ledger natif, aucun serveur externe).

## Modèle Économique

```
Créateur publie un site avec prix = 5 ATN
  → Le site apparaît dans le Feed avec badge "Premium 5 ATN"
  → Un visiteur clique "Acheter l'accès"
  → Le Ledger crée une transaction signée (Transfer: visiteur → créateur, 5 ATN)
  → Le système enregistre l'achat localement
  → Le contenu complet est débloqué
```

## Modifications DB

### Nouvelle colonne dans `sites`
```sql
ALTER TABLE sites ADD COLUMN price_atn REAL DEFAULT 0.0;
```

### Nouvelle table `purchases`
```sql
CREATE TABLE IF NOT EXISTS purchases (
    id TEXT PRIMARY KEY,
    site_id TEXT NOT NULL,
    buyer_pk TEXT NOT NULL,
    seller_pk TEXT NOT NULL,
    amount REAL NOT NULL,
    tx_id TEXT NOT NULL,
    purchased_at TEXT NOT NULL,
    UNIQUE(site_id, buyer_pk)
);
```

## Nouvelles Commandes Tauri

```rust
#[tauri::command]
async fn set_site_price(state: ..., site_id: String, price: f64) -> Result<(), String>

#[tauri::command]
async fn purchase_site(state: ..., site_id: String) -> Result<Transaction, String>
// 1. Vérifie le solde ATN du buyer
// 2. Crée une transaction signée
// 3. Enregistre l'achat dans la table purchases
// 4. Retourne la transaction

#[tauri::command]
async fn check_access(state: ..., site_id: String) -> Result<bool, String>
// Vérifie si l'utilisateur actif a acheté ce site (ou en est l'auteur)

#[tauri::command]
async fn get_my_purchases(state: ...) -> Result<Vec<Purchase>, String>
```

## Modifications Frontend

### Feed.svelte
- Afficher le prix ATN sur les sites premium
- Bouton "Acheter l'accès (X ATN)" au lieu de "Voir"
- Badge "Premium" avec icône cadenas

### Browser.svelte
- Si un site a un prix > 0, vérifier l'accès avant d'afficher le contenu
- Afficher une page "paywall" si pas acheté

### Editor.svelte
- Ajouter un champ "Prix (ATN)" dans l'éditeur de site
- Toggle "Gratuit / Premium"

### Nouveau : Marketplace.svelte (optionnel)
- Galerie des sites premium
- Filtres par prix, catégorie, popularité

## Fichiers à Modifier
- `storage/db.rs` — Migration + table purchases + méthodes CRUD
- `p2p/ledger.rs` — Nouveau TxType::Purchase
- `lib.rs` — Nouvelles commandes Tauri
- `src/lib/Feed.svelte` — Prix + bouton achat
- `src/lib/Browser.svelte` — Vérification d'accès
- `src/lib/Editor.svelte` — Champ prix
