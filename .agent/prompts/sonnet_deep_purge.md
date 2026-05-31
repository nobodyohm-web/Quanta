# Premier Prompt pour Claude Code Sonnet — Élagage Maximum

> Copie-colle ce prompt directement dans Claude Code.

---

```
Lis CLAUDE.md et .agent/memory.md avant de commencer.

# Mission : Élagage Radical — Purge du Code Social Résiduel

Le pivot V2 "réseau social → crypto pure" a déjà été fait en partie, mais il reste BEAUCOUP de code mort social dans le backend. Tu dois tout supprimer chirurgicalement en gardant le build fonctionnel.

## Règle absolue : cargo check + npm run build doivent passer à chaque étape.

---

## ÉTAPE 1 — Supprimer les fichiers morts (2 fichiers)

Supprime ces fichiers du disque :
- src-tauri/src/p2p/attention.rs
- src-tauri/src/p2p/notifications.rs

Ils sont DÉJÀ retirés de mod.rs et WillowNode. Les fichiers traînent encore sur le disque.

cargo check (doit passer, aucune ref n'existe plus)

---

## ÉTAPE 2 — Nettoyer reputation.rs (le plus gros travail)

Fichier : src-tauri/src/p2p/reputation.rs (391 lignes → objectif ~150)

### 2a. Supprimer les constantes sociales mortes :
- ATN_VIEW, ATN_LIKE, ATN_LIKE_GIVER_BONUS, ATN_COMMENT_UPVOTED, ATN_SHARE, ATN_FORK, ATN_HELP_VALIDATED, ATN_REPORT_CORRECT
- TRUST_HELP, TRUST_CONTENT, TRUST_LIKE, TRUST_REPORT_VALID_PENALTY, TRUST_REPORT_ABUSIVE_PENALTY
- VIEW_COOLDOWN_SECS, LIKE_COOLDOWN_SECS

### 2b. Supprimer les champs sociaux de UserReputation :
- content_created, helps_given, likes_received
- reports_made, reports_validated, reports_abusive, reports_received, reports_received_valid
Garder UNIQUEMENT : public_key, trust_score, status, atn_earned, atn_balance, atn_staked, uptime_minutes, energy_kwh, energy_atn_mined, joined_at

### 2c. Supprimer les champs sociaux de ReputationEngine :
- view_cooldowns, like_cooldowns (HashMap)
- Leurs initialisations dans new()

### 2d. Supprimer les méthodes sociales mortes (aucun caller dans lib.rs) :
- on_content_created()
- on_view()
- on_like()
- on_help_validated()
- on_report()
- on_fork()
- visibility_boost() sur TrustStatus

### 2e. Adapter compute_trust_score() :
L'ancienne formule utilise content_created, likes_received, helps_given, reports.
Nouvelle formule V2 basée uniquement sur uptime + énergie :
```rust
fn compute_trust_score(user: &UserReputation) -> f64 {
    let uptime_factor = (user.uptime_minutes as f64 / 60.0).min(1000.0); // heures
    let energy_factor = (user.energy_kwh * 100.0).min(500.0);
    let stake_factor = (user.atn_staked * 2.0).min(500.0);
    (uptime_factor + energy_factor + stake_factor).max(0.0)
}
```

### 2f. Adapter get_or_create() :
Retirer les initialisations des champs sociaux supprimés.

cargo check (doit passer)

---

## ÉTAPE 3 — Nettoyer lib.rs

### 3a. Commandes start_sync et stop_sync :
Elles référencent site_id et db.get_site(). Elles sont encore utilisées pour le P2P.
GARDE-LES mais remplace la logique site par une logique de "connexion pair" simplifiée :
- start_sync : ne prend plus site_id, lance juste le endpoint Iroh
- stop_sync : arrête le endpoint

Ou mieux : si start_sync/stop_sync sont déjà gérés par init_endpoint dans le background task, supprime ces commandes Tauri et retire-les du invoke_handler.

### 3b. Commande index_site + peer_query :
Elles utilisent la recherche BM25 qui servait à chercher des sites.
En V2, il n'y a plus de sites à chercher. Supprime ces 2 commandes Tauri et retire-les du invoke_handler.

### 3c. Nettoyer get_my_reputation :
Vérifie que la réponse JSON ne renvoie plus les champs sociaux supprimés.

cargo check (doit passer)

---

## ÉTAPE 4 — Nettoyer willow_node.rs

### 4a. Le struct Subspace et le champ subspaces : 
Le concept de "subspace = site publié" est social. 
Simplifie : garde subspaces en tant que HashMap vide (il servira pour le Marketplace V2), ou supprime-le avec start_sync/stop_sync/get_subspace si tu les as retirés de lib.rs.

### 4b. SubspaceStatus dans mod.rs :
Si Subspace/SubspaceStatus ne sont plus utilisés, supprime-les de mod.rs.

### 4c. ContentBlock dans mod.rs :
Plus utilisé (c'était pour les blocs de contenu des sites). Supprime-le.

cargo check (doit passer)

---

## ÉTAPE 5 — Nettoyer storage/db.rs

### 5a. Supprimer les fonctions site :
- create_site, update_site, set_published, get_all_sites, get_site, delete_site
- Le struct Site aussi

### 5b. Supprimer les fonctions search index :
- upsert_index, clear_site_index, total_indexed_sites, document_frequency, average_doc_length, search_index
- La table search_index dans le schema SQL

### 5c. Garder :
- new() (avec schema simplifié)
- store_keypair, get_active_keypair
- record_tx, get_transactions
- save_state, load_state

cargo check (doit passer)

---

## ÉTAPE 6 — Nettoyer search/

Si index_site et peer_query sont supprimés de lib.rs, le module search n'a plus de raison d'exister.
Supprime search/indexer.rs et simplifie search/mod.rs (ou supprime le module entier si aucune ref).

cargo check (doit passer)

---

## ÉTAPE 7 — Nettoyer sybil.rs

Vérifie que SybilGuard::poc_score() ne référence plus les champs sociaux supprimés de UserReputation.
Adapte la formule pour utiliser uniquement uptime_minutes, energy_kwh, atn_staked.

cargo check (doit passer)

---

## ÉTAPE 8 — Frontend Dashboard.svelte

Le Dashboard référence encore : sites_created, likes_given, likes_received, helps_validated.
Remplace par des stats V2 :
- uptime_hours, energy_kwh, quanta_mined, connected_peers

---

## ÉTAPE 9 — Vérification finale

```bash
cargo check --manifest-path src-tauri/Cargo.toml    # 0 error
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings  # 0 warning
npm run build                                        # success
```

Si tout passe, commit :
```bash
git add -A
git commit -m "refactor: deep purge of social code from reputation, db, search, willow_node"
```

## ÉTAPE 10 — Mettre à jour .agent/memory.md

Ajoute une entrée de session avec :
- Nombre de lignes supprimées
- Fichiers supprimés
- Fonctions supprimées
- État du build

---

RÉSUMÉ : Tu supprimes ~600 lignes de code mort social réparties dans :
- attention.rs (184 lignes → supprimé)
- notifications.rs (91 lignes → supprimé)
- reputation.rs (391→~150 lignes)
- db.rs (supprimer ~100 lignes de fonctions site/search)
- search/indexer.rs (174 lignes → supprimé ou vidé)
- willow_node.rs (supprimer Subspace, start_sync, stop_sync si possible)
- sybil.rs (adapter poc_score)
- lib.rs (retirer start_sync, stop_sync, index_site, peer_query)
- Dashboard.svelte (adapter stats)

Procède étape par étape. cargo check après CHAQUE étape.
```
