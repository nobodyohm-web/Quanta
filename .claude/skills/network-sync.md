---
description: Working on peer connections, chain synchronization, reconnection, peer discovery, or network topology
globs: ["src-tauri/src/p2p/willow_node.rs", "src-tauri/src/p2p/gossip_tasks.rs", "src-tauri/src/p2p/state_persistence.rs"]
---

# Skill: Network & Sync

## Connection Lifecycle
```
User pastes EndpointId
  → connect_peer(peer_id_str)
    → iroh join_peers()
    → register in known_peers
    → trigger_hello_now()
      → broadcast Hello(chain_height, watts, country)
        → peer receives Hello
          → if chain_height differs → RequestChain
            → ChainSegment (max 50 blocks, paginated)
              → integrate_remote_block() for each
```

## Auto-Reconnection (NET-1)
```
NeighborDown event
  → mark_peer_down(endpoint_id)
    → KnownPeer.connected = false

spawn_auto_reconnect (every 5s)
  → peers_needing_reconnect()
    → for each: sleep(backoff) → try_reconnect()
      → backoff: 1s → 2s → 4s → 8s → 16s → 32s → 60s (max)
      → max 10 attempts before giving up

NeighborUp event
  → mark_peer_up(endpoint_id)
    → KnownPeer.reconnect_attempts = 0
```

## Background Tasks (6 total)
| Task | Interval | File |
|------|----------|------|
| Outgoing drain | continuous | gossip_tasks.rs |
| Incoming dispatch | continuous | gossip_tasks.rs |
| Hello broadcast | 60s | gossip_tasks.rs |
| Peer cleanup | 30s | gossip_tasks.rs |
| Auto-reconnect | 5s | gossip_tasks.rs |
| Mining loop | 60s | mining_loop.rs |
| State persistence | 30s | state_persistence.rs |

ALL tasks use `CancellationToken` for graceful shutdown.

## State Stores (crypto-only)
All wrapped in `Arc<RwLock<T>>`. Persisted to SQLite every 30s:
`ledger, reputation, consensus, gossip, usernames`.
In-memory only: `energy_oracle, peer_country_reports, peer_info, nonce_tracker`.

## Iroh QUIC Setup
```rust
Endpoint::builder(presets::N0).bind().await
  → Gossip::builder().spawn(endpoint)
    → Router::builder(endpoint).accept(GOSSIP_ALPN, gossip).spawn()
      → gossip.subscribe(quanta_topic_id(), vec![])
        → (GossipSender, GossipReceiver)
```

## Topic
All nodes share: `quanta_topic_id() = BLAKE3("quanta-network-v1")`
