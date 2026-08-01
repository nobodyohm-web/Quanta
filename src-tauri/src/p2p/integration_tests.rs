//! INT — Integration tests for multi-node scenarios.
//!
//! These tests simulate realistic multi-peer interactions that were previously
//! only tested at the unit level (per-module). They exercise the actual message
//! flow between independent Ledger/Gossip/Social instances.
//!
//! Test categories:
//!   - INT-1: Multi-node ledger convergence (3 nodes reach identical state)
//!   - INT-2: Balance cache consistency invariant (cache == full scan)
//!   - INT-3: Gossip envelope round-trip (sign → serialize → deserialize → verify)
//!   - INT-4: Social + Search + Moderation cross-module pipeline

#[cfg(test)]
#[allow(clippy::module_inception)] // file name reflects module content
mod integration_tests {
    use crate::p2p::ledger::{Ledger, MICRO};
    use crate::p2p::gossip::{GossipEnvelope, GossipMessage, GossipRouter};
    use crate::security::CryptoEngine;
    use std::collections::HashMap;

    // ─── INT-1: Multi-node ledger convergence ───────────────────────────

    /// Three independent ledgers mine independently, exchange blocks, and
    /// must converge to the same chain tip + identical balances.
    #[test]
    fn int1_three_nodes_converge_on_same_chain() {
        let pk_a = "a".repeat(64);
        let pk_b = "b".repeat(64);
        let pk_c = "c".repeat(64);

        let mut node_a = Ledger::new();
        let mut node_b = Ledger::new();
        let mut node_c = Ledger::new();

        // ── Phase 1: Each node mines locally ──
        node_a.mine_tx(&pk_a, 2 * MICRO, 0.5);
        node_b.mine_tx(&pk_b, MICRO, 0.3);
        node_c.mine_tx(&pk_c, MICRO, 0.2);

        // ── Phase 2: Node A seals block #1 and broadcasts ──
        let block_a = node_a.seal_block(&pk_a, 0.5);
        assert_eq!(block_a.index, 1);

        // B and C integrate A's block
        assert!(
            node_b.integrate_remote_block(block_a.clone()).unwrap(),
            "B must accept A's block #1"
        );
        assert!(
            node_c.integrate_remote_block(block_a.clone()).unwrap(),
            "C must accept A's block #1"
        );

        // ── Phase 3: Node B mines more and seals block #2 ──
        // B's pending holds TWO of its own mining rewards (phase-1 + phase-3 —
        // no longer falsely dropped since BLK-HASH-1). EMIT-1 (Option A — one
        // reward per block): `seal_block` COALESCES them into a single NETWORK→B
        // reward (Σ = 2 QTA), so block #2 carries exactly ONE mining tx and
        // passes the "≤1 reward/block" rule peers enforce. MINT-EXACT-1 : Σ reste
        // sous la récompense canonique du bloc (~4 QTA), sinon les pairs le
        // rejetteraient — c'est exactement ce que ce test doit refléter.
        node_b.mine_tx(&pk_b, MICRO, 0.1);
        let block_b = node_b.seal_block(&pk_b, 0.1);
        assert_eq!(block_b.index, 2);
        let mining_in_b2 = block_b
            .transactions
            .iter()
            .filter(|t| t.tx_type == crate::p2p::ledger::TxType::Mining)
            .count();
        assert_eq!(
            mining_in_b2, 1,
            "EMIT-1: B's two rewards coalesce into a single per-block reward"
        );

        // A and C integrate B's block
        assert!(node_a.integrate_remote_block(block_b.clone()).unwrap());
        assert!(node_c.integrate_remote_block(block_b.clone()).unwrap());

        // ── Phase 4: All three nodes seal block #3 independently ──
        // This creates a 3-way fork. Deterministic resolution via hash comparison.
        node_a.mine_tx(&pk_a, MICRO, 0.01);
        node_b.mine_tx(&pk_b, MICRO, 0.01);
        node_c.mine_tx(&pk_c, MICRO, 0.01);
        // TEST-TEETH (HARDEN-HYGIENE-1): the three blocks already differ by miner
        // (pk_a/pk_b/pk_c is bound into the block hash), so their hashes are
        // content-distinct without any wall-clock sleep — the old
        // `thread::sleep(2ms)` calls were a vestigial timing dependency.
        let b3_a = node_a.seal_block(&pk_a, 0.01);
        let b3_b = node_b.seal_block(&pk_b, 0.01);
        let b3_c = node_c.seal_block(&pk_c, 0.01);

        // Determine winner by lexicographic hash comparison (same rule as consensus)
        let mut hashes = [
            (&b3_a.hash, "A"),
            (&b3_b.hash, "B"),
            (&b3_c.hash, "C"),
        ];
        hashes.sort_by(|a, b| b.0.cmp(a.0));
        let winner_hash = hashes[0].0.clone();

        // Each node integrates the other two blocks
        let _ = node_a.integrate_remote_block(b3_b.clone());
        let _ = node_a.integrate_remote_block(b3_c.clone());
        let _ = node_b.integrate_remote_block(b3_a.clone());
        let _ = node_b.integrate_remote_block(b3_c.clone());
        let _ = node_c.integrate_remote_block(b3_a.clone());
        let _ = node_c.integrate_remote_block(b3_b.clone());

        // ── Verify convergence ──
        let tip_a = node_a.chain.last().unwrap().hash.clone();
        let tip_b = node_b.chain.last().unwrap().hash.clone();
        let tip_c = node_c.chain.last().unwrap().hash.clone();

        assert_eq!(tip_a, tip_b, "A and B must converge to same tip");
        assert_eq!(tip_b, tip_c, "B and C must converge to same tip");
        assert_eq!(tip_a, winner_hash, "all nodes must pick the lex-highest hash as winner");

        // Chain heights must be equal
        assert_eq!(node_a.chain_height(), node_b.chain_height());
        assert_eq!(node_b.chain_height(), node_c.chain_height());
    }

    // ─── INT-2: Balance cache consistency ───────────────────────────────

    /// The incremental balance cache must produce identical results to a full
    /// chain scan after all pending transactions are sealed into blocks.
    #[test]
    fn int2_balance_cache_matches_full_scan() {
        let mut crypto = CryptoEngine::new();
        let id_a = crypto.generate_keypair();
        let pk_a = id_a.public_key_hex.clone();
        let pk_b = "b".repeat(64);
        let pk_c = "c".repeat(64);

        let mut ledger = Ledger::new();

        // Mining — EMIT-1 (one reward per block): each miner earns its own
        // reward in its OWN block, sealed by that miner (the reward credits the
        // block's miner). The multiple holders the cache must track then come
        // from those rewards + the transfers below.
        ledger.mine_tx(&pk_a, 1000 * MICRO, 0.5);
        ledger.seal_block(&pk_a, 0.5);
        ledger.mine_tx(&pk_b, 500 * MICRO, 0.3);
        ledger.seal_block(&pk_b, 0.3);
        ledger.mine_tx(&pk_c, 200 * MICRO, 0.1);
        ledger.seal_block(&pk_c, 0.1);

        // Transfers (sealed in a reward-free block).
        let _ = ledger.transfer_tx(&pk_a, &pk_b, 100 * MICRO, &crypto);
        let _ = ledger.transfer_with_burn(&pk_a, &pk_c, 50 * MICRO, &crypto);
        ledger.seal_block(&pk_a, 0.1);

        // More mining + seal
        ledger.mine_tx(&pk_b, 30 * MICRO, 0.01);
        ledger.seal_block(&pk_b, 0.01);

        // Now pending is empty — full chain scan matches cache exactly
        assert_eq!(ledger.pending_count(), 0, "all pending should be sealed");

        // Verify: rebuild from chain and compare to cache
        let full_scan = full_scan_balance(&ledger);
        let cache_all = ledger.all_balances();

        // Compare each account
        for (pk, full_bal) in &full_scan {
            let cached = cache_all.get(pk).copied().unwrap_or(0);
            assert_eq!(
                cached, *full_bal,
                "Cache mismatch for {}: cache={}, scan={}",
                &pk[..12], cached, full_bal
            );
        }

        // TEST-TEETH (HARDEN-HYGIENE-1): the loop above only checks cache ⊇ scan
        // over SCAN's keys — a phantom/stale account in the cache but absent from
        // the chain scan (the exact reorg-revert bug this test guards) would pass
        // silently. Assert the non-zero key SETS are EQUAL both ways, and the
        // totals agree, so an invented cache account breaks the test.
        let scan_keys: std::collections::HashSet<&String> =
            full_scan.iter().filter(|(_, v)| **v > 0).map(|(k, _)| k).collect();
        let cache_keys: std::collections::HashSet<&String> =
            cache_all.iter().filter(|(_, v)| **v > 0).map(|(k, _)| k).collect();
        assert_eq!(
            cache_keys, scan_keys,
            "cache and full-scan must hold the SAME set of non-zero accounts"
        );
        let scan_total: u64 = full_scan.values().sum();
        let cache_total: u64 = cache_all.values().sum();
        assert_eq!(cache_total, scan_total, "cache and scan must agree on the total balance");

        // Also verify individual balance_of calls
        assert_eq!(
            ledger.balance_of(&pk_a),
            *full_scan.get(&pk_a).unwrap_or(&0),
            "balance_of mismatch for pk_a"
        );
        assert_eq!(
            ledger.balance_of(&pk_b),
            *full_scan.get(&pk_b).unwrap_or(&0),
            "balance_of mismatch for pk_b"
        );
        assert_eq!(
            ledger.balance_of(&pk_c),
            *full_scan.get(&pk_c).unwrap_or(&0),
            "balance_of mismatch for pk_c"
        );
    }

    /// Full chain scan (the old O(n) method) — used as ground truth for cache validation.
    fn full_scan_balance(ledger: &Ledger) -> HashMap<String, u64> {
        let mut bals: HashMap<String, i128> = HashMap::new();
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        for block in &ledger.chain {
            for tx in &block.transactions {
                if !synthetic(&tx.to) {
                    *bals.entry(tx.to.clone()).or_insert(0) += tx.amount as i128;
                }
                if !synthetic(&tx.from) {
                    *bals.entry(tx.from.clone()).or_insert(0) -= tx.amount as i128;
                }
            }
        }
        // Include pending
        // Note: pending is private, but we can test via balance_of consistency
        bals.into_iter()
            .map(|(k, v)| (k, v.max(0) as u64))
            .collect()
    }

    /// INT-2b: Balance cache survives snapshot/restore round-trip.
    #[test]
    fn int2b_cache_survives_snapshot_restore() {
        let pk = "a".repeat(64);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 500 * MICRO, 0.1);
        ledger.mine_tx(&pk, 300 * MICRO, 0.2);
        // EMIT-1 (one reward per block): the miner seals its own reward, so the
        // two ticks coalesce into a single NETWORK→pk reward (Σ = 800) and the
        // rebuilt cache credits pk exactly — consistent across snapshot/restore.
        ledger.seal_block(&pk, 0.1);

        let bal_before = ledger.balance_of(&pk);

        let snap = ledger.snapshot();
        let restored = Ledger::restore(snap);

        assert_eq!(
            restored.balance_of(&pk),
            bal_before,
            "Cache must be rebuilt identically after restore"
        );
    }

    // ─── AUDIT-SYNC: paginated chain reconstruction ─────────────────────

    /// AUDIT-SYNC: a fresh ledger must be able to rebuild a 100-block chain
    /// from a peer by integrating blocks in batches of 50 (the wire-format
    /// pagination limit). Mirrors the real ChainSegment flow without the
    /// gossip transport layer.
    #[test]
    fn audit_sync_reconstructs_100_blocks_paginated() {
        let pk = "a".repeat(64);

        // Source ledger: build a 100-block chain locally.
        let mut source = Ledger::new();
        for _ in 0..100 {
            // Mining tx ensures pending is non-empty so seal_block works.
            source.mine_tx(&pk, MICRO, 0.01);
            source.seal_block(&pk, 0.01);
        }
        // Source chain has 101 blocks total (genesis + 100).
        assert_eq!(source.chain.len(), 101);
        let final_hash = source.chain.last().unwrap().hash.clone();

        // Receiver: starts with only genesis. Sync in two pages of 50.
        let mut receiver = Ledger::new();
        assert_eq!(receiver.chain.len(), 1);

        // Page 1: blocks 1..=50
        for i in 1..=50 {
            let blk = source.chain[i].clone();
            assert!(
                receiver.integrate_remote_block(blk).is_ok(),
                "page-1 block {} must integrate",
                i
            );
        }
        assert_eq!(receiver.chain.len(), 51);

        // Page 2: blocks 51..=100
        for i in 51..=100 {
            let blk = source.chain[i].clone();
            assert!(
                receiver.integrate_remote_block(blk).is_ok(),
                "page-2 block {} must integrate",
                i
            );
        }
        assert_eq!(receiver.chain.len(), 101);
        assert_eq!(
            receiver.chain.last().unwrap().hash,
            final_hash,
            "receiver must converge on source's tip hash"
        );
    }

    /// AUDIT-SYNC: integrating a contiguous segment must reject the whole
    /// rest of the segment as soon as one block fails (gap detection).
    #[test]
    fn audit_sync_segment_breaks_at_first_invalid() {
        let pk = "a".repeat(64);
        let mut source = Ledger::new();
        for _ in 0..5 {
            source.mine_tx(&pk, MICRO, 0.0);
            source.seal_block(&pk, 0.0);
        }
        let mut receiver = Ledger::new();
        // Skip block 1 to introduce a gap in the segment.
        let segment: Vec<_> = (2..=5).map(|i| source.chain[i].clone()).collect();

        let mut accepted = 0;
        for blk in segment {
            match receiver.integrate_remote_block(blk) {
                Ok(true) => accepted += 1,
                _ => break, // mirrors dispatcher behaviour
            }
        }
        // None of the 4 blocks should integrate — block 2 needs block 1's
        // hash, which receiver doesn't have.
        assert_eq!(accepted, 0, "no block in a gapped segment may integrate");
    }

    /// AUDIT-SYNC: gzip compression round-trip preserves the exact block JSON.
    #[test]
    fn audit_sync_compression_round_trip() {
        use crate::p2p::gossip::{compress_blocks, decompress_blocks};
        let pk = "a".repeat(64);
        let mut source = Ledger::new();
        for _ in 0..10 {
            source.mine_tx(&pk, MICRO, 0.0);
            source.seal_block(&pk, 0.0);
        }
        let blocks_json: Vec<String> = source
            .chain
            .iter()
            .skip(1) // skip genesis
            .map(|b| serde_json::to_string(b).unwrap())
            .collect();
        let compressed = compress_blocks(&blocks_json).expect("must compress");
        let decompressed = decompress_blocks(&compressed).expect("must decompress");
        assert_eq!(decompressed, blocks_json);
    }

    // ─── INT-3: Gossip envelope full round-trip ─────────────────────────

    /// Sign → serialize → deserialize → verify: the entire gossip pipeline
    /// must work end-to-end without any field corruption.
    #[test]
    fn int3_gossip_envelope_full_round_trip() {
        let mut crypto = CryptoEngine::new();
        crypto.generate_pq_identity().expect("ml-dsa primary");
        // PQ-ENVELOPE-1: envelope sender + signature = ML-DSA-65 primary key.
        let pk = crypto.pq_identity_hex().expect("ml-dsa primary");
        let pk = &pk;

        let msg = GossipMessage::Ping { nonce: 42 };
        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = 1u64;

        // Step 1: Build signable bytes
        let signable = GossipRouter::signable_envelope_bytes(pk, nonce, &timestamp, &msg);

        // Step 2: Sign (ML-DSA-65)
        let sig = crypto.sign_pq(&signable).expect("signing must succeed");

        // Step 3: Build envelope
        let env = GossipRouter::build_signed_envelope(
            pk.to_string(),
            msg,
            nonce,
            timestamp,
            &sig,
        )
        .expect("envelope build must succeed");

        // Step 4: Serialize to JSON (simulates network transport)
        let json = serde_json::to_vec(&env).expect("serialization must succeed");

        // Step 5: Deserialize from JSON (simulates network reception)
        let received: GossipEnvelope =
            serde_json::from_slice(&json).expect("deserialization must succeed");

        // Step 6: Verify signature on received envelope
        let result = crate::p2p::dispatcher::try_process_raw_gossip(&json);
        assert!(
            result.is_ok(),
            "Full round-trip envelope must pass verification: {:?}",
            result
        );

        // Verify fields survived serialization
        assert_eq!(received.sender, env.sender);
        assert_eq!(received.nonce, env.nonce);
        assert_eq!(received.id, env.id);
    }
}
