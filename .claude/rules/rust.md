---
description: Règles Rust pour le backend Tauri QUANTA
paths: ["src-tauri/src/**/*.rs"]
---

# Règles Rust

1. **tokio::sync** uniquement : `Mutex`, `RwLock` de tokio — JAMAIS `std::sync::Mutex` à travers un `.await`
2. **Lock ordering** : engines (read) → release → DB (write) — jamais l'inverse
3. **Error handling** : `Result<T, String>` pour les commandes Tauri, `?` operator
4. **No dead code** : `cargo clippy -D warnings` doit passer
5. **Serde** : Tout type partagé avec le frontend doit être `Serialize + Deserialize`
6. **CRDT** : Les compteurs utilisent `crdts` crate — `PNCounter` pour balances, `GCounter` pour métriques
7. **DAG** : Les nœuds sont content-addressed via BLAKE3(parents + payload + author)
8. **Gossip** : Messages signés Ed25519, enveloppés dans `GossipEnvelope`, dedupliqués via `seen_messages`
9. **Émission V3 (tokenomics rareté)** : plafond DUR `MAX_SUPPLY_MICRO = 100M QUANTA`,
   émission **décroissante front-loaded** `emission_for_tick = (MAX − total_mined) / EMISSION_DIVISOR`
   (rythme réaliste, ~4 QUANTA/bloc à la genèse). **Zéro premine, zéro autorité de mint.**
   Le plafond ET la borne par bloc sont **vérifiés au consensus** (`validate_block_emission`)
   pour qu'un pair malveillant ne puisse ni dépasser 100M ni rafler l'émission d'un coup.
   NE PAS revenir à un modèle « non plafonné / fixe » : la rareté est le cœur du projet.
10. **Tests** : `cargo test` doit passer avant tout commit
