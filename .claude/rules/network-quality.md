---
description: Règles de qualité réseau — latence, throughput, fiabilité
paths: ["src-tauri/src/p2p/**/*.rs", "src-tauri/src/lib.rs"]
---

# Règles Qualité Réseau

## Latence

1. **Hello broadcast** : <100ms de latence entre connect_peer() et réception du Hello
2. **Chain sync** : <5s pour synchroniser 100 blocs entre 2 peers
3. **Transaction propagation** : <2s entre broadcast et réception par tous les peers
4. **Block propagation** : <3s entre seal et intégration par les peers

## Throughput

5. **Messages** : Le réseau DOIT supporter 30 msg/min/peer × 100 peers = 3000 msg/min
6. **Blocs** : Seal toutes les 2 min → 720 blocs/jour
7. **Transactions** : 10 tx/bloc minimum → 7200 tx/jour

## Fiabilité

8. **Zero message loss** : Si un message est broadcasté, TOUS les peers connectés le reçoivent
9. **Dedup** : LRU 100K entries — suffisant pour ~24h de traffic réseau normal
10. **Recovery** : Si le node crash, state_persistence restaure le dernier snapshot (30s max de perte)
11. **Fork convergence** : Après un fork, les 2 nodes DOIVENT converger au même tip en <60s

## Monitoring

12. **GossipStats** : Toujours maintenir bytes_sent, bytes_received, messages_sent, messages_received
13. **Peer liveness** : peer_info.elapsed() accessible pour le frontend via get_node_status
14. **Chain health** : chain_height, pending_count, balance_cache.len() exposés au frontend
