---
description: Règles Rust pour le backend Tauri SOVA
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
9. **Émission V2** : 100 SOVA/h fixe — PAS de halving, PAS de cap, PAS de `MAX_ATN`
10. **Tests** : `cargo test` doit passer avant tout commit
