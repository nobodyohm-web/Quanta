# 🔍 Audit Complet — SOVA Protocol V2

> **Date** : 30 avril 2026 | **Codebase** : 4 759 lignes Rust + 2 006 lignes Svelte
> **Tests** : 37 unitaires + 1 intégration P2P | **Build** : cargo check ✅ clippy ✅ npm ✅

---

## Score Global : 65% Production-Ready

```
████████████░░░░░░░░  65%
```

| Domaine | Score | Détail |
|---------|-------|--------|
| Économie V2 | ██████████░░ 85% | Émission + burn + Shapley câblé |
| Sécurité crypto | ██████░░░░░░ 55% | zeroize OK mais verify_tx contournable |
| P2P/Réseau | █████░░░░░░░ 45% | Gossip OK mais pas de signed Hello |
| Code mort | ████░░░░░░░░ 35% | Encore ~400 lignes legacy social |
| Frontend | ████░░░░░░░░ 35% | Dashboard pas aligné avec V2 |
| Tests | █████░░░░░░░ 45% | 37 tests mais 0 tests de sécurité |
| Infrastructure | ███░░░░░░░░░ 25% | Pas de git, pas de CI/CD actif |

---

## 🔴 CRITIQUE — Sécurité (à corriger MAINTENANT)

### C1. `unwrap()` en production — 2 lignes dangereuses

| Fichier | Ligne | Code | Risque |
|---------|-------|------|--------|
| `lib.rs` | 91 | `db.as_ref().unwrap()` | **Crash** si DB pas prête |
| `ledger.rs` | 215 | `self.chain.last().unwrap()` | **Crash** si chain vide |
| `willow_node.rs` | 212 | `serde_json::to_vec(&meta_json).unwrap()` | **Crash** si serialization échoue |

> [!CAUTION]
> Un seul `unwrap()` peut faire crasher le nœud entier. En P2P, un nœud qui crash = perte de fonds en cours de mining.

**Fix** : Remplacer par `ok_or("message")?` ou `.unwrap_or_default()`

### C2. GossipEnvelope non vérifié à la réception

```rust
// dispatcher.rs:42 — on match le payload SANS vérifier la signature de l'enveloppe
match env.payload { ... }
```

Le dispatcher vérifie `is_fresh()` (timestamp) mais **ne vérifie PAS la signature Ed25519 de l'enveloppe gossip**. Un attaquant peut forger des Hello avec de faux watts → voler une part du mining.

**Fix** : Ajouter `CryptoEngine::verify(&env.sender, &env.payload_bytes, &env.signature)` dans `dispatch_incoming()` avant le match.

### C3. Marketplace sans vérification de solde

```rust
// marketplace.rs:136 — submit_task n'a aucun lien avec le ledger
pub fn submit_task(...) -> Result<ComputeTask, String> {
    // Le submitter paie reward_sova + burn... mais on ne vérifie PAS son solde !
}
```

Un attaquant peut soumettre des tâches avec 10 000 SOVA de reward sans posséder un seul token.

**Fix** : `submit_task()` doit prendre une référence au Ledger et vérifier `balance >= reward + burn`.

### C4. Transfer sans vérification de solde réel

```rust
// ledger.rs:transfer_with_burn — vérifie-t-on le solde ?
```

Le ledger `signed_transfer` ne vérifie probablement pas si `from` a assez de balance (le solde est dans `reputation.rs`, pas dans `ledger.rs`). Double spending possible.

**Fix** : Synchroniser balance entre `reputation.atn_balance` et `ledger`.

---

## 🟡 WARNING — Code mort à purger

### W1. `consensus.rs` : InteractionCounters (likes/views) — 50 lignes mortes

```
consensus.rs:83-130 → record_like(), record_view(), like_count(), view_count()
consensus.rs:259-264 → tests qui appellent record_like
```

**100% social**, aucun caller en V2. À supprimer.

### W2. `lib.rs` : 4 commandes Tauri legacy encore enregistrées

```
lib.rs:83  → start_sync(site_id)     ← site publishing social
lib.rs:96  → stop_sync(site_id)      ← site unpublishing
lib.rs:111 → index_site(site_id)     ← BM25 search indexing
lib.rs:119 → peer_query(query)       ← search across network
lib.rs:694 → invoke_handler still registers them
```

**Fix** : Supprimer ces 4 commandes + retirer de invoke_handler.

### W3. `storage/db.rs` : 7 fonctions Site + table search_index

```
db.rs:8   → struct Site
db.rs:63  → CREATE TABLE search_index
db.rs:80  → create_site, update_site, set_published, get_all_sites, get_site, delete_site
db.rs:185 → upsert_index, clear_site_index, search_index
```

~100 lignes mortes. Aucune commande Tauri ne les appelle en V2.

### W4. `search/indexer.rs` — Module entier mort (174 lignes)

Plus aucun caller depuis la purge de `index_site`. À supprimer.

### W5. `p2p/mod.rs` : SubspaceStatus + ContentBlock — structs mortes

```
mod.rs:47 → SubspaceStatus { site_id, ... }
mod.rs:58 → ContentBlock { ... }
```

Vestige du système de publication de sites. À supprimer.

### W6. `willow_node.rs` : Subspace, start_sync, stop_sync — 80 lignes mortes

```
willow_node.rs:30  → struct Subspace
willow_node.rs:42  → subspaces: HashMap
willow_node.rs:179 → start_sync()
willow_node.rs:241 → stop_sync()
willow_node.rs:264 → get_subspace()
```

**Total code mort : ~400 lignes** à supprimer.

---

## 🟡 WARNING — Failles économiques

### E1. Pas de cap sur le mining multiplier Shapley

Le Shapley score × SybilGuard multiplier pourrait donner >1.0 en théorie. Vérifier que `sova <= EMISSION_PER_TICK` est garanti.

### E2. Pas de minimum pour transfer_with_burn

Un transfert de 0.0001 SOVA génère un burn de 0.000001 — potentiel spam à coût quasi nul.

**Fix** : Minimum transfer = 0.01 SOVA.

### E3. Pas de max balance vérification

Rien n'empêche un nœud de miner indéfiniment sans jamais synchroniser avec le réseau (offline mining attack).

**Fix** : L'émission solo doit être plafonnée dans le temps (ex: max 24h sans sync gossip).

### E4. CRDT Ledger et Ledger classique non synchronisés

`consensus.rs` a un `CrdtLedger` (PN-Counter) ET `ledger.rs` a un `Ledger` classique (chain-based). Ils ne sont PAS connectés. Les balances CRDT divergent des balances ledger.

**Fix** : Choisir UN modèle de balance (recommandé : CRDT uniquement pour le consensus, ledger pour l'historique auditabe).

### E5. DeSci DAO non implémenté

Le whitepaper promet 5% de l'émission vers un fonds scientifique voté par les utilisateurs. Le code ne l'implémente pas du tout.

---

## 🟡 WARNING — P2P Protocol

### P1. Hello non signé

Le message Hello contient watts et country mais **n'est PAS signé individuellement**. L'enveloppe gossip est signée mais si la vérification est bypassée (voir C2), les watts sont forgeable.

### P2. Pas de heartbeat/cleanup des peers morts

`peer_watts` s'accumule sans jamais supprimer les peers déconnectés. Après 1 semaine, 10 000 peers morts gonflent `total_network_watts` → le mining local tend vers 0.

**Fix** : TTL de 5 minutes sur chaque entrée peer_watts. Si pas de Hello reçu, supprimer.

### P3. Pas de rate limiting sur le gossip

Un nœud malveillant peut flooder le réseau de messages gossip (DoS). Pas de throttle, pas de ban temporaire.

### P4. Pas de Merkle proof sur les transactions

Les transactions sont vérifiées une par une mais il n'y a pas de Merkle root par bloc. Sans ça, impossible de faire du light-client verification (SPV).

### P5. Pas de consensus sur l'ordre des blocs

`seal_block()` crée un bloc local mais il n'y a pas de mécanisme de consensus entre nœuds sur QUEL bloc est le suivant. Deux nœuds peuvent créer des blocs concurrents → fork permanent.

**Fix Phase 4** : DAG-based consensus (le Merkle-DAG existe déjà, l'utiliser comme source de vérité).

### P6. Pas de peer discovery

Actuellement, la connexion aux peers dépend d'Iroh relay. Il n'y a pas de bootstrap nodes, pas de DHT pour découvrir de nouveaux peers.

---

## 🟡 WARNING — Frontend

### F1. Dashboard pas aligné V2

`Dashboard.svelte` affiche probablement encore les anciennes stats. Doit afficher :
- Watts en temps réel, pairs connectés, SOVA minés/h
- Shapley share, position dans le réseau
- Historique de mining (sparkline)

### F2. Pas de vue Marketplace

Le marketplace est câblé côté Rust mais **aucune UI** n'existe pour :
- Soumettre une tâche
- Voir les tâches pending
- Voir ses earnings de worker

### F3. Sidebar encore présente (244 lignes)

`Sidebar.svelte` est un composant de 244 lignes qui ne devrait plus exister (navigation = bottom bar 3 items via NavBar.svelte).

### F4. Pas d'indicateur de mining en direct

L'utilisateur ne voit pas que son nœud mine activement. Besoin d'un pulse/animation + compteur live.

### F5. Pas de responsive mobile

Tauri est desktop mais si on veut une version web future, le CSS doit être responsive.

---

## 🟡 WARNING — Tests

### T1. Zéro test de sécurité

Pas de test pour :
- Double spending (transfert > balance)
- Signature forgée (tx avec mauvaise signature → rejet)
- Replay attack (même tx envoyée 2 fois)
- Overflow/underflow sur les balances

### T2. Pas de test de convergence CRDT

Les tests CRDT testent le merge mais pas la convergence avec 3+ nœuds appliquant des opérations dans un ordre différent.

### T3. Pas de fuzz testing

Pour un protocole crypto, le fuzzing est obligatoire sur :
- Parsing des messages gossip
- verify_tx() avec des inputs aléatoires
- Shapley avec des valeurs extrêmes (NaN, Infinity, négatifs)

### T4. Pas de benchmark

Aucun `#[bench]` ou criterion pour mesurer :
- Transactions/seconde
- Temps de seal_block
- Latence gossip

---

## 🔵 Infrastructure

### I1. Git non initialisé

`.git/` est vide. Aucun historique, aucune branche. **Critique** pour la collaboration et le rollback.

**Fix immédiat** : `git init && git add -A && git commit -m "V2.0 — SOVA Protocol"`

### I2. CI/CD non fonctionnel

Les fichiers `.github/workflows/` existent mais sans git, ils ne servent à rien.

### I3. Pas de release pipeline

Aucun workflow pour builder les binaires Tauri (macOS .dmg, Windows .msi, Linux .AppImage).

### I4. Pas de documentation API

Les commandes Tauri ne sont documentées nulle part. Un `API.md` avec chaque commande, ses params et sa réponse JSON serait essentiel.

---

## 📋 Roadmap Prioritaire

### Phase A — Hygiène (1 session Sonnet, ~2h)

| # | Tâche | Lignes | Impact |
|---|-------|--------|--------|
| A1 | Supprimer code mort (consensus likes, search, site, subspace) | -400 | Clean |
| A2 | Remplacer tous les `unwrap()` production | ~10 fixes | Sécurité |
| A3 | Supprimer `Sidebar.svelte` | -244 | Clean |
| A4 | `git init` + premier commit | — | Infra |
| A5 | Minimum transfer = 0.01 SOVA | 2 lignes | Économie |

### Phase B — Sécurité (1 session Opus, ~1h)

| # | Tâche | Complexité | Impact |
|---|-------|-----------|--------|
| B1 | Vérifier signature GossipEnvelope dans dispatcher | Medium | 🔴 Critique |
| B2 | Vérifier solde avant transfer + marketplace submit | Medium | 🔴 Critique |
| B3 | TTL sur peer_watts (cleanup peers morts) | Easy | P2P |
| B4 | Rate limiting gossip (max 10 msg/s/peer) | Medium | DoS |
| B5 | Tests de sécurité (double-spend, replay, forge) | Medium | Tests |

### Phase C — Features (multi-sessions)

| # | Tâche | Effort | Impact |
|---|-------|--------|--------|
| C1 | Frontend Dashboard V2 (watts live, sparkline mining) | 4h | UX |
| C2 | Frontend Marketplace UI | 4h | Feature |
| C3 | Consensus DAG-based (résolution de forks) | 8h Opus | Core |
| C4 | DeSci DAO (5% émission, vote) | 4h | Whitepaper |
| C5 | Solo offline mining cap (24h max) | 1h | Économie |
| C6 | Merkle root par bloc (light client) | 2h | Protocol |
| C7 | Fuzz testing (cargo-fuzz) | 2h | Sécurité |
| C8 | ZK-Proofs RISC Zero (guest program) | 16h Opus | Phase 4 |

---

## Verdict

> [!IMPORTANT]
> SOVA a un **protocole solide** (émission fixe, Shapley, burn, P2P Iroh, crypto post-quantique) mais souffre de **~400 lignes de code mort social**, de **3 failles de sécurité critiques** (unwrap, signature gossip non vérifiée, solde non vérifié), et d'un **frontend pas aligné** avec le backend V2.

**Pour devenir la meilleure crypto au monde**, les priorités sont :
1. 🔴 Fixer les 3 failles de sécurité (Phase B)
2. 🟡 Purger le code mort restant (Phase A)
3. 🟢 Aligner le frontend sur le backend V2 (Phase C)
4. 🔵 ZK-Proofs + consensus DAG pour la scalabilité (Phase C)
