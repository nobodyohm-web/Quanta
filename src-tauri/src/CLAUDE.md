# Backend Rust — Règles spécifiques V2

> Ce fichier ne se charge que quand Claude travaille dans src-tauri/

## Lock ordering (CRITIQUE)
```
1. crypto (lock) → release
2. reputation (read) → release
3. ledger (write) → release
4. gossip (read/write)
JAMAIS l'inverse — risque de deadlock
```

## Mining loop (mining_loop.rs)
```
toutes les 60s:
  watts = estimate_watts()
  total = node.total_network_watts() + watts
  (qta, kwh) = reputation.uptime_tick(pk, total_mined, total)
  ledger.mine_tx(pk, qta, kwh)
  dag.append(mine_node)
  
toutes les 2 ticks (seal):
  block = ledger.seal_if_pending(pk, kwh)
  if block → broadcast NewBlock
```

## Émission V2
- `NETWORK_EMISSION_PER_HOUR = 100.0`
- `EMISSION_PER_TICK = 100.0 / 60.0` (1.6667 QUANTA/min)
- Solo : 100% de l'émission
- Multi : proportionnel aux watts via `mining_rate_proportional()`

## Gossip Protocol — 22 Message Types
```
Core sync:     Hello, RequestChain, ChainSegment, NewBlock
Transactions:  BroadcastTx
DAG sync:      WantNodes, HaveNodes
Web P2P:       PublishPage, RequestPage, PublishSiteManifest
Liveness:      Ping, Pong
Security:      ReportPeer
Domains:       PublishDomain, PublishSubdomain
Search:        PublishSite
Social:        BroadcastSocialAction
Moderation:    BroadcastReport, BroadcastJurorCommit, BroadcastJurorReveal
Forums:        PublishForumNode
```

## Dispatch pipeline (dispatcher.rs)
```
① Size guard (10 MB)
② JSON → GossipEnvelope
③ Ban check
④ Dedup (seen_messages LRU 100K)
⑤ Timestamp (±90s)
⑥ Rate limit (30/min/peer)
⑦ Nonce (monotone per sender)
⑧ Ed25519 signature verify
⑨ Handler dispatch
```

## State stores (WillowNode)
```
reputation, ledger, consensus, dag, gossip,
energy_oracle, peer_country_reports, peer_info,
nonce_tracker, marketplace, page_store, domains,
search, social, moderation, forums, follow_graph
```
Tous wrappés dans `Arc<RwLock<T>>`, snapshotés toutes les 30s.

## Tests critiques
```bash
# Compilation
cargo check --manifest-path src-tauri/Cargo.toml

# Tous les tests (196+)
cargo test --manifest-path src-tauri/Cargo.toml

# P2P uniquement
cargo test --manifest-path src-tauri/Cargo.toml -- p2p

# Sécurité
cargo test --manifest-path src-tauri/Cargo.toml -- security

# Avec logs
cargo test --manifest-path src-tauri/Cargo.toml -- --nocapture
```
