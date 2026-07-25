---
description: Règles du protocole Torus P2P — wire format, sync, network layer
paths: ["src-tauri/src/p2p/**/*.rs"]
---

# Règles Protocole P2P Torus

## Wire Protocol

1. **GossipEnvelope** est la seule unité de transport réseau. Jamais de bytes bruts.
2. **Signing** : TOUJOURS utiliser `signable_envelope_bytes(sender, nonce, timestamp, payload)` — JAMAIS signer le payload seul
3. **Nonce monotone** : Le nonce de l'envelope DOIT être strictement croissant par sender. Utiliser `next_outgoing_nonce()`
4. **Timestamp** : Fenêtre ±90s. Utiliser le MÊME timestamp pour signing et envelope
5. **Message ID** : `BLAKE3(signable_envelope_bytes(sender, nonce, timestamp, payload))`
   — le digest de la **pré-image signée**, jamais du payload seul (H1/H3, audit
   2026-07-25 : un id dérivé du payload faisait collisionner deux expéditeurs sur
   une seule case de dedup, et un id choisi par l'attaquant censurait le sync).
   Déterministe, pas de UUID ; recalculé et **vérifié** à la réception.

## Sync Protocol

6. **Chain sync** : Hello → compare chain_height → RequestChain(from_height) → ChainSegment(max 50) → loop
7. **DAG sync** : Hello(heads) → WantNodes(missing) → HaveNodes(nodes) — tri par profondeur parents
8. **Pagination** : ChainSegment est paginé. Si sender_height > our_height après réception, re-demander
9. **Idempotence** : Tous les handlers DOIVENT être idempotents (dedup via seen_messages + seen_tx_hashes)

## Network Robustness

10. **Reconnexion** : Si un peer est perdu (NeighborDown), tenter reconnexion après backoff exponentiel
11. **Multi-peer** : TOUJOURS supporter N peers, pas seulement 1. Le design DOIT scaler à 100+ peers
12. **DoS protection** : 10 MB max envelope, 30 msg/min rate limit (adaptatif),
    50 blocs max par ChainSegment
13. **Graceful shutdown** : Utiliser `CancellationToken` pour TOUS les background tasks
14. **Fallback offline** : Si Iroh endpoint échoue, le node reste fonctionnel en local mode

## Invariants

15. **Eventual consistency** : CRDT + blockchain DOIVENT converger. Tester avec 2+ nodes en chaos
16. **No data loss** : Un bloc validé ne peut JAMAIS disparaître sauf fork reorg déterministe
17. **Broadcast order** : Hello AVANT tout autre message après connexion
18. **Topic unique** : Tous les nodes utilisent `quanta_topic_id()` — BLAKE3("quanta-network-v1")
