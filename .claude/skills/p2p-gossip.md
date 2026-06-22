---
description: Modifying or debugging the gossip protocol, message types, envelopes, routing, or broadcast
globs: ["src-tauri/src/p2p/gossip.rs", "src-tauri/src/p2p/dispatcher.rs", "src-tauri/src/p2p/gossip_tasks.rs"]
---

# Skill: P2P Gossip Protocol

## Context Injection

When working on the gossip layer, ALWAYS load this context:

### Wire Format
Every message travels as: `GossipEnvelope { id, sender, payload: GossipMessage, signature, timestamp, nonce }`

### Signing — CRITICAL
```rust
// CORRECT: canonical bytes cover sender + nonce + timestamp + payload
let signable = GossipRouter::signable_envelope_bytes(&pk, nonce, &timestamp, &msg);
let sig = crypto.sign(&signable)?;
let env = GossipRouter::build_signed_envelope(pk, msg, nonce, timestamp, &sig)?;

// WRONG: never sign payload alone
let bytes = serde_json::to_vec(&msg); // ❌ NEVER
```

### Security Pipeline (dispatcher.rs)
```
① Size guard (MAX_RAW_ENVELOPE_BYTES = 10 MB)
② JSON → GossipEnvelope
③ Ban check (NonceTracker::is_banned)
④ Dedup (mark_seen, LRU 100K)
⑤ Timestamp freshness (±90s)
⑥ Rate limit (30 msg/min/peer)
⑦ Nonce monotone (check_and_advance)
⑧ Ed25519 verify (verify_envelope_signature)
⑨ Payload → handler
```

### Message Types (9 total — crypto-only)
| Priority | Messages |
|----------|----------|
| CRITICAL | Hello, RequestChain, ChainSegment, NewBlock |
| HIGH | BroadcastTx |
| MEDIUM | PublishUsername |
| LOW | Ping, Pong, ReportPeer |

### Adding a New Message Type — Template
```rust
// 1. Add variant to GossipMessage (gossip.rs)
MyNewMessage { data_json: String },

// 2. Add handler in dispatcher.rs (inside match env.payload)
GossipMessage::MyNewMessage { data_json } => {
    handle_my_new_message(state, &data_json).await;
}

// 3. Add stats counter if needed (gossip.rs GossipStats)
#[serde(default)]
pub my_new_count: u64,

// 4. Add #[serde(default)] for backward compat on ALL new fields
```

### Files
- `gossip.rs` (470 lines) — Messages, GossipEnvelope, GossipRouter, GossipStats
- `dispatcher.rs` (1104 lines) — dispatch_incoming(), all handlers, NonceTracker
- `gossip_tasks.rs` (220 lines) — spawn_outgoing_drain, spawn_incoming_dispatch, spawn_hello_broadcast, spawn_auto_reconnect
