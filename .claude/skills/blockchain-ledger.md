---
description: Working on the blockchain ledger, transactions, blocks, chain sync, fork resolution, or token economics
globs: ["src-tauri/src/p2p/ledger.rs", "src-tauri/src/p2p/ledger_types.rs", "src-tauri/src/p2p/pos_consensus.rs"]
---

# Skill: Blockchain & Ledger

## Core Invariants

### Monetary System
- **1 QUANTA = 1_000_000 µQTA** (`pub const MICRO: u64 = 1_000_000`)
- ALL amounts in `u64` µQTA — NEVER f64 for balances
- Emission: 100 QUANTA/hour, fixed forever
- Burn: 1% on transfers, 2% on compute tasks

### Balance Cache (PERF-1)
```rust
// O(1) balance lookup via incremental HashMap
balance_cache: HashMap<String, i128>  // signed to handle temporary negatives during reorg
// Every tx applies: cache_apply_tx() / cache_revert_tx()
// Full rebuild: rebuild_cache() (on restore from snapshot)
```

### Transaction Lifecycle
```
build_signed_tx(from, to, amount, type, crypto)
  → assigns nonce (get_nonce + increment_nonce)
  → hybrid sign (Ed25519 + PQ stub)
  → returns Transaction
  
cache_apply_tx(&tx)  → updates balance_cache
pending.push(tx)     → queued for next block

seal_block(miner, kwh)  → Block { index, timestamp, txs, prev_hash, hash, miner }
  → hash = BLAKE3(index:prev_hash:ts:tx_count:merkle_root)
  → merkle_root = BLAKE3 tree of tx IDs
```

### Chain Sync (Remote Blocks)
```
validate_remote_block(&block)
  → check index continuity (tip.index + 1)
  → check prev_hash linkage
  → verify ALL tx signatures
  → recompute block hash

integrate_remote_block(block)
  → Ok(true)  = new block integrated
  → Ok(false) = duplicate (already known)
  → Err(reason) = invalid
  → Fork: same height, different hash → higher hash wins
```

### Anti-Replay
- `seen_tx_hashes: HashSet<String>` — dedup all known tx hashes
- `account_nonces: HashMap<String, u64>` — per-account monotonic nonce
- Timestamp window ±5min on transfers

### Fork Resolution
```rust
if block.hash > tip.hash {
    // Remote wins: pop our tip, revert cache, re-queue txs, push remote
} else {
    // Ours wins: keep tip, discard remote
}
```

### Snapshot/Restore
```rust
LedgerSnapshot { chain, pending, tx_counter, account_nonces }
// rebuild_cache() called on restore
// seen_tx_hashes rebuilt from chain + pending
```
