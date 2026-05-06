# Prompt Sonnet — Phase A : Hygiène & Sécurité de Base

> Purge code mort + fix unwrap + git init. Audit : .agent/design/audit_v2.md

---

```
Lis CLAUDE.md et .agent/memory.md. Référence : .agent/design/audit_v2.md

# Mission : Phase A — Nettoyage final + fixes sécurité de base

cargo check après CHAQUE étape.

---

## ÉTAPE 1 — Purger consensus.rs : likes/views (50 lignes)

Fichier : src-tauri/src/p2p/consensus.rs

Supprimer :
- struct InteractionCounters (lignes ~86-91)
- impl InteractionCounters : new(), record_like(), record_view(), like_count(), view_count(), merge() (lignes ~93-131)
- Le champ `counters` dans ConsensusEngine s'il existe
- Les tests qui appellent record_like (lignes ~259-264)

Garder : CrdtLedger, le reste du consensus.

cargo check

---

## ÉTAPE 2 — Supprimer 4 commandes Tauri legacy dans lib.rs

Supprimer ces fonctions ET leurs entrées dans invoke_handler() :
- start_sync (lignes ~83-92)
- stop_sync (lignes ~96-100)
- index_site (lignes ~111-116)
- peer_query (lignes ~119-...)

Supprimer aussi :
- `use search::indexer::SemanticIndexer;` (ligne ~11)

cargo check

---

## ÉTAPE 3 — Supprimer search/indexer.rs (174 lignes)

Supprimer le fichier src-tauri/src/search/indexer.rs.
Vérifier si search/mod.rs référence indexer. Si oui, nettoyer.
Si le module search/ est vide, supprimer tout le dossier.
Retirer `mod search;` de main.rs ou lib.rs si nécessaire.

cargo check

---

## ÉTAPE 4 — Supprimer code mort dans storage/db.rs

Supprimer :
- struct Site + ses champs
- create_site(), update_site(), set_published(), get_all_sites(), get_site(), delete_site()
- upsert_index(), clear_site_index(), total_indexed_sites(), document_frequency(), average_doc_length(), search_index()
- La table search_index dans le schema SQL (CREATE TABLE search_index...)
- Les index idx_search_keyword, idx_search_site

Garder : new(), store_keypair, get_active_keypair, record_tx, get_transactions, save_state, load_state.

cargo check

---

## ÉTAPE 5 — Supprimer code mort dans willow_node.rs

Supprimer :
- struct Subspace (ligne ~30)
- champ subspaces dans WillowNode + son init dans new()
- start_sync() method
- stop_sync() method
- get_subspace() method

Supprimer dans mod.rs :
- struct SubspaceStatus
- struct ContentBlock

cargo check

---

## ÉTAPE 6 — Supprimer Sidebar.svelte

Si Sidebar.svelte n'est importé dans aucun composant/route actif, supprimer le fichier.
Vérifier : grep -r "Sidebar" src/

cargo check + npm run build

---

## ÉTAPE 7 — Fix tous les unwrap() en production

Fichiers à corriger (HORS tests) :
- lib.rs:91 → `db.as_ref().unwrap()` → `db.as_ref().ok_or("DB not ready")?`
- ledger.rs:215 → `self.chain.last().unwrap()` → `self.chain.last().ok_or("Empty chain")?`
- willow_node.rs:212 → `serde_json::to_vec(...).unwrap()` → `.map_err(|e| e.to_string())?`

Ne touche PAS aux unwrap dans les tests (#[cfg(test)]).

cargo check

---

## ÉTAPE 8 — Minimum transfer = 0.01 QUANTA

Dans ledger.rs, transfer_with_burn() :
```rust
if amount < 0.01 {
    return Err("Minimum transfer: 0.01 QUANTA".into());
}
```

cargo check

---

## ÉTAPE 9 — Git init

```bash
cd /Users/alex/Desktop/Torus
git init
git add -A
git commit -m "V2.0 — QUANTA Protocol: proportional mining, Shapley, burn-and-mint, marketplace"
```

---

## ÉTAPE 10 — Vérification finale

```bash
cargo check
cargo test
cargo clippy -- -D warnings
npm run build
```

Mets à jour .agent/memory.md avec :
- Nombre de lignes supprimées
- Fichiers supprimés
- unwrap fixés
- Git initialisé

---

RÉSUMÉ : ~500 lignes supprimées, 3 unwrap fixés, minimum transfer ajouté, git init.
```
