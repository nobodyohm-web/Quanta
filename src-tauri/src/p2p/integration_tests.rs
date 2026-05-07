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
        node_a.mine_tx(&pk_a, 100 * MICRO, 0.5);
        node_b.mine_tx(&pk_b, 80 * MICRO, 0.3);
        node_c.mine_tx(&pk_c, 60 * MICRO, 0.2);

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
        node_b.mine_tx(&pk_b, 50 * MICRO, 0.1);
        let block_b = node_b.seal_block(&pk_b, 0.1);
        assert_eq!(block_b.index, 2);

        // A and C integrate B's block
        assert!(node_a.integrate_remote_block(block_b.clone()).unwrap());
        assert!(node_c.integrate_remote_block(block_b.clone()).unwrap());

        // ── Phase 4: All three nodes seal block #3 independently ──
        // This creates a 3-way fork. Deterministic resolution via hash comparison.
        node_a.mine_tx(&pk_a, 10 * MICRO, 0.01);
        node_b.mine_tx(&pk_b, 10 * MICRO, 0.01);
        node_c.mine_tx(&pk_c, 10 * MICRO, 0.01);
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b3_a = node_a.seal_block(&pk_a, 0.01);
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b3_b = node_b.seal_block(&pk_b, 0.01);
        std::thread::sleep(std::time::Duration::from_millis(2));
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

        // Mining
        ledger.mine_tx(&pk_a, 1000 * MICRO, 0.5);
        ledger.mine_tx(&pk_b, 500 * MICRO, 0.3);
        ledger.mine_tx(&pk_c, 200 * MICRO, 0.1);

        // Transfers
        let _ = ledger.transfer_tx(&pk_a, &pk_b, 100 * MICRO, &crypto);
        let _ = ledger.transfer_with_burn(&pk_a, &pk_c, 50 * MICRO, &crypto);

        // Seal all pending so full_scan can see everything
        ledger.seal_block("miner", 0.1);

        // More mining + seal
        ledger.mine_tx(&pk_b, 30 * MICRO, 0.01);
        ledger.seal_block("miner", 0.01);

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
        ledger.seal_block("miner", 0.1);

        let bal_before = ledger.balance_of(&pk);

        let snap = ledger.snapshot();
        let restored = Ledger::restore(snap);

        assert_eq!(
            restored.balance_of(&pk),
            bal_before,
            "Cache must be rebuilt identically after restore"
        );
    }

    // ─── INT-3: Gossip envelope full round-trip ─────────────────────────

    /// Sign → serialize → deserialize → verify: the entire gossip pipeline
    /// must work end-to-end without any field corruption.
    #[test]
    fn int3_gossip_envelope_full_round_trip() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = &id.public_key_hex;

        let msg = GossipMessage::Ping { nonce: 42 };
        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = 1u64;

        // Step 1: Build signable bytes
        let signable = GossipRouter::signable_envelope_bytes(pk, nonce, &timestamp, &msg);

        // Step 2: Sign
        let sig = crypto.sign(&signable).expect("signing must succeed");

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

    // ─── INT-4: Social + Search cross-module ────────────────────────────

    /// A document indexed in SearchIndex, liked via SocialState, must have
    /// its social signals correctly reflected in search ranking.
    #[test]
    fn int4_social_signals_flow_to_search() {
        use crate::p2p::search::{SearchIndex, IndexedDoc, DocKind, SocialSignals, SearchFilters, tokenize, term_freq};
        use crate::p2p::social::{SocialState, SocialAction, SignedAction, FollowTier, LIKE_BASE_COST_MICRO_QTA};

        let author_sk = ed25519_dalek::SigningKey::from_bytes(&[1; 32]);
        let voter_sk = ed25519_dalek::SigningKey::from_bytes(&[2; 32]);
        let author_pk = hex::encode(author_sk.verifying_key().as_bytes());
        let _voter_pk = hex::encode(voter_sk.verifying_key().as_bytes());

        // 1. Index a document
        let mut search = SearchIndex::new();
        let text = "Cuisine vegan biologique";
        let toks = tokenize(text);
        search.upsert(IndexedDoc {
            cid: "cid_cuisine".into(),
            title: "Cuisine Vegan".into(),
            snippet: text.into(),
            author_pk: author_pk.clone(),
            kind: DocKind::Site,
            lang: "fr".into(),
            updated_at: 1000,
            term_freq: term_freq(&toks),
            torus_domain: Some("chef.torus".into()),
        });

        // 2. Like the document via SocialState
        let mut social = SocialState::new();
        let mut action = SignedAction {
            action: SocialAction::Vote {
                target_cid: "cid_cuisine".into(),
                target_author_pk: author_pk.clone(),
                amount_micro_qta: 10 * LIKE_BASE_COST_MICRO_QTA,
                weight: 1,
            },
            author_pk: String::new(),
            timestamp: 1001,
            nonce: 1,
            signature: String::new(),
        };
        crate::p2p::social::sign_action(&voter_sk, &mut action);
        social.apply(&action, 1001).unwrap();

        // 3. Follow the author
        let mut follow_action = SignedAction {
            action: SocialAction::Follow {
                followee_pk: author_pk.clone(),
                tier: FollowTier::Supporter,
                active: true,
            },
            author_pk: String::new(),
            timestamp: 1002,
            nonce: 2,
            signature: String::new(),
        };
        crate::p2p::social::sign_action(&voter_sk, &mut follow_action);
        social.apply(&follow_action, 1002).unwrap();

        // 4. Search with real social signals
        let hits = search.search(
            "cuisine vegan",
            &SearchFilters::default(),
            2000,
            10,
            |cid| {
                let (likes, followers) = social.signals_for(cid, &author_pk);
                SocialSignals {
                    weighted_likes: likes,
                    follower_count: followers,
                    creator_reputation: 0.9,
                    moderation_malus: 0.0,
                }
            },
        );

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].cid, "cid_cuisine");
        assert!(hits[0].score > 0.0, "Score must be positive with real social signals");

        // Verify social state is consistent
        let page = social.page_stats("cid_cuisine").unwrap();
        assert_eq!(page.like_count, 1);
        assert!(page.weighted_likes > 0.0);

        let creator = social.creator_stats(&author_pk).unwrap();
        assert_eq!(creator.follower_count, 1);
        assert!(creator.weighted_likes_received > 0.0);
    }
}
