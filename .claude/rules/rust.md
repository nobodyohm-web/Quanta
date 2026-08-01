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
8. **Gossip** : Messages signés **ML-DSA-65** (PQ-ENVELOPE-1), enveloppés dans
   `GossipEnvelope`, dédupliqués via `seen_messages` (insertion post-signature, H1)
9. **Émission V3 (tokenomics rareté)** : plafond DUR `MAX_SUPPLY_MICRO = 100M QUANTA`,
   émission décroissante `emission_for_tick = (MAX − total_mined) / EMISSION_DIVISOR`
   (~2 QUANTA/tick à la genèse ⇒ **~4 QUANTA/bloc**). **Zéro premine, zéro autorité de mint.**
   **MINT-EXACT-1** : la récompense d'un bloc est `emission_for_block(offre minée avant
   ce bloc)` — une **fonction pure de la chaîne** que le producteur frappe
   (`mint_block_reward`) et que chaque récepteur **recalcule** (`block_minted ≤ expected`).
   Ne JAMAIS remplacer ce recalcul par une borne large : l'ancienne marge de `64 ticks`
   laissait `32 × N` fois le montant honnête à n'importe quel sceleur. Aucun montant
   calculé localement (Shapley, watts auto-déclarés) ne doit toucher la monnaie.
   NE PAS revenir à un modèle « non plafonné / fixe » : la rareté est le cœur du projet.
10. **Partage imposé (REWARD-SHARE-1)** : la récompense se répartit entre le
    producteur et les participants récents (`expected_block_rewards`), et chaque
    nœud **recalcule** ce plan (`validate_block_reward_plan`). Ne jamais rendre la
    répartition optionnelle : un partage seulement appliqué par le logiciel de
    référence n'est pas une règle, c'est une politesse contournable.
11. **Entrée libre (OPEN-DOOR-1)** : un bloc sur `OPEN_SLOT_EVERY_BLOCKS` est un slot
    ouvert à toute adresse, bondée ou non. Ne pas le supprimer sans le remplacer :
    sans lui, `PROPOSER-1` referme le réseau définitivement au premier staker (aucun
    faucet, airdrop ni premine n'existe pour rompre la boucle œuf-poule).
12. **Tests** : `cargo test` doit passer avant tout commit
