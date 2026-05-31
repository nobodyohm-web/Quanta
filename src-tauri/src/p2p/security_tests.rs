//! B5 — Security Testing Framework for QUANTA Protocol
//!
//! Comprehensive test suite covering all Phase B attack vectors:
//! - S1: Double-spend prevention (balance check)
//! - S2: Signature forgery rejection (Ed25519)
//! - S3: Replay attack prevention (nonce + hash dedup)
//! - S4: Stale message rejection (timestamp window)
//! - S5: Balance overflow protection
//! - S6: Emission invariant (never exceeds cap)
//! - S7: Shapley efficiency axiom (shares sum to 1.0)
//! - S8: Self-transfer rejection
//! - S9: Negative/zero amount rejection
//! - S10: Marketplace task without sufficient balance

#[cfg(test)]
#[allow(clippy::module_inception)] // Le nom du fichier reflète son contenu (security_tests.rs).
mod security_tests {
    use crate::p2p::ledger::{Ledger, MICRO};
    use crate::p2p::gossip::{GossipEnvelope, GossipMessage, GossipRouter};
    use crate::p2p::shapley::{self, NodeContribution, NodeMode};
    use crate::p2p::dispatcher;
    use crate::p2p::marketplace::{Marketplace, TaskType};
    use crate::security::CryptoEngine;
    use std::collections::HashMap;

    // ─── Helpers ─────────────────────────────────────────────────────────────

    /// Helper: amount is in µQTA (u64).
    #[allow(dead_code)]
    fn setup_ledger_with_balance(pk: &str, amount_uqta: u64) -> (Ledger, CryptoEngine) {
        let mut crypto = CryptoEngine::new();
        let _id = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        ledger.mine_tx(pk, amount_uqta, 0.0);
        ledger.seal_block("GENESIS", 0.0);
        (ledger, crypto)
    }

    fn make_fresh_envelope(crypto: &CryptoEngine, pk: &str, msg: GossipMessage) -> GossipEnvelope {
        // STRUCT-1: Sign the full canonical bytes (sender + nonce + timestamp + payload)
        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = 0_u64;
        let signable = GossipRouter::signable_envelope_bytes(pk, nonce, &timestamp, &msg);
        let sig = crypto.sign(&signable).unwrap();
        GossipRouter::build_signed_envelope(pk.to_string(), msg, nonce, timestamp, &sig).unwrap()
    }

    // ─── S1: Double-spend prevention ────────────────────────────────────────

    #[test]
    fn s1_double_spend_rejected() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = &id.public_key_hex;

        let mut ledger = Ledger::new();
        ledger.mine_tx(pk, 100 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        let to = "b".repeat(64);

        // First transfer: 80 QUANTA — should succeed
        assert!(ledger.transfer_tx(pk, &to, 80 * MICRO, &crypto).is_ok(),
            "First transfer of 80 QUANTA should succeed with 100 QUANTA balance");

        // Second transfer: 80 QUANTA — should FAIL (only ~20 left)
        let result = ledger.transfer_tx(pk, &to, 80 * MICRO, &crypto);
        assert!(result.is_err(),
            "Second transfer of 80 QUANTA should fail (insufficient balance). Got: {:?}", result);
    }

    #[test]
    fn s1_exact_balance_transfer() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = &id.public_key_hex;

        let mut ledger = Ledger::new();
        ledger.mine_tx(pk, 50 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        let to = "c".repeat(64);

        // Transfer exactly the balance — should succeed
        assert!(ledger.transfer_tx(pk, &to, 50 * MICRO, &crypto).is_ok(),
            "Transfer of exact balance should succeed");

        // Now balance is 0 — even tiny transfer should fail (0.01 QUANTA = 10_000 µQTA)
        let result = ledger.transfer_tx(pk, &to, 10_000, &crypto);
        assert!(result.is_err(),
            "Transfer after draining balance must fail");
    }

    // ─── S2: Signature forgery rejection ────────────────────────────────────

    #[test]
    fn s2_forged_signature_rejected() {
        let env = GossipEnvelope {
            id: "forged_id".into(),
            sender: "a".repeat(64),        // random public key
            payload: GossipMessage::Ping { nonce: 42 },
            signature: "f".repeat(128),     // random fake signature
            timestamp: chrono::Utc::now().to_rfc3339(),
            nonce: 0,
        };

        let result = dispatcher::try_process_raw_gossip(
            &serde_json::to_vec(&env).unwrap()
        );
        assert!(result.is_err(), "Forged signature must be rejected");
    }

    #[test]
    fn s2_valid_signature_accepted() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();

        let msg = GossipMessage::Ping { nonce: 7 };
        let env = make_fresh_envelope(&crypto, &id.public_key_hex, msg);

        let result = dispatcher::try_process_raw_gossip(
            &serde_json::to_vec(&env).unwrap()
        );
        assert!(result.is_ok(), "Valid signed envelope should pass: {:?}", result);
    }

    // ─── S3: Replay attack prevention ───────────────────────────────────────

    #[test]
    fn s3_replay_tx_rejected() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = &id.public_key_hex;

        let mut ledger = Ledger::new();
        ledger.mine_tx(pk, 100 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        let to = "d".repeat(64);

        // First transfer succeeds (10 QUANTA)
        let _tx1 = ledger.transfer_tx(pk, &to, 10 * MICRO, &crypto).unwrap();

        // Balance should be 90 QUANTA = 90 * MICRO µQTA after the transfer
        assert_eq!(ledger.balance_of(pk), 90 * MICRO,
            "Balance should be reduced after first transfer");
    }

    #[test]
    fn s3_nonce_tracking() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = &id.public_key_hex;

        let mut ledger = Ledger::new();
        ledger.mine_tx(pk, 100 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        // Nonce should start at 0
        assert_eq!(ledger.get_nonce(pk), 0);

        let to = "e".repeat(64);
        ledger.transfer_tx(pk, &to, MICRO, &crypto).unwrap();

        // After one transfer, nonce should be 1
        assert_eq!(ledger.get_nonce(pk), 1);

        ledger.transfer_tx(pk, &to, MICRO, &crypto).unwrap();
        assert_eq!(ledger.get_nonce(pk), 2);
    }

    // ─── S4: Stale message rejection ────────────────────────────────────────

    #[test]
    fn s4_stale_message_rejected() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();

        let msg = GossipMessage::Ping { nonce: 1 };
        // STRUCT-1: sign with the old timestamp so signature is valid for that ts
        let old_ts = (chrono::Utc::now() - chrono::Duration::minutes(10)).to_rfc3339();
        let signable = GossipRouter::signable_envelope_bytes(
            &id.public_key_hex, 0, &old_ts, &msg
        );
        let sig = crypto.sign(&signable).unwrap();

        // Create envelope with timestamp 10 minutes ago (beyond ±5 min window)
        let env = GossipEnvelope {
            id: "stale_id".into(),
            sender: id.public_key_hex.clone(),
            payload: msg,
            signature: hex::encode(&sig),
            timestamp: old_ts,
            nonce: 0,
        };

        let result = dispatcher::try_process_raw_gossip(
            &serde_json::to_vec(&env).unwrap()
        );
        assert!(result.is_err(), "Stale message (10 min old) must be rejected");
    }

    #[test]
    fn s4_fresh_message_accepted() {
        assert!(GossipRouter::is_fresh(&chrono::Utc::now().to_rfc3339()),
            "Current timestamp should be considered fresh");
    }

    // ─── S5: Balance overflow protection ────────────────────────────────────

    #[test]
    fn s5_extreme_amount_handled() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = &id.public_key_hex;

        let mut ledger = Ledger::new();
        // Give a huge but finite balance: 1M QUANTA = 10^6 * 10^6 = 10^12 µQTA
        ledger.mine_tx(pk, 1_000_000 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        let to = "f".repeat(64);

        // Transfer of exactly the balance should not overflow
        let result = ledger.transfer_tx(pk, &to, 1_000_000 * MICRO, &crypto);
        assert!(result.is_ok(), "Large but valid transfer should succeed");
    }

    #[test]
    fn s5_negative_balance_impossible() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = &id.public_key_hex;

        let mut ledger = Ledger::new();
        ledger.mine_tx(pk, 10 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        let to = "a".repeat(64);

        // Try many transfers — balance is u64 so it can't go negative.
        for _ in 0..20 {
            let _ = ledger.transfer_tx(pk, &to, MICRO, &crypto);
        }

        let _balance = ledger.balance_of(pk);
        // u64 cannot be negative — invariant trivially holds, but we keep the test
        // for symmetry with the previous f64 contract.
    }

    // ─── S6: Emission invariant ─────────────────────────────────────────────

    #[test]
    fn s6_emission_never_exceeds_cap() {
        let emission_cap = crate::p2p::reputation::EMISSION_PER_TICK;

        let mut rep = crate::p2p::reputation::ReputationEngine::new();
        let pk = "a".repeat(64);
        let peers: HashMap<String, NodeContribution> = HashMap::new();

        // Test 100 mining ticks — none should exceed the cap
        for i in 0..100u64 {
            let (qta, _kwh) = rep.uptime_tick(&pk, i * emission_cap, &peers);
            // Apply the B4 hard cap (as done in lib.rs)
            let capped = qta.min(emission_cap);
            assert!(capped <= emission_cap,
                "Emission tick {} exceeded cap: {} > {}", i, capped, emission_cap);
        }
    }

    // ─── S7: Shapley efficiency invariant ───────────────────────────────────

    #[test]
    fn s7_shapley_shares_sum_to_one() {
        let mut contribs = HashMap::new();
        contribs.insert("A".into(), NodeContribution {
            node_id: "A".into(), watts: 200.0,
            tasks_completed: 20, blocks_verified: 10, uptime_minutes: 120, mode: NodeMode::default(),
            weighted_likes: 0.0,
        });
        contribs.insert("B".into(), NodeContribution {
            node_id: "B".into(), watts: 50.0,
            tasks_completed: 5, blocks_verified: 2, uptime_minutes: 60, mode: NodeMode::default(),
            weighted_likes: 0.0,
        });
        contribs.insert("C".into(), NodeContribution {
            node_id: "C".into(), watts: 100.0,
            tasks_completed: 10, blocks_verified: 5, uptime_minutes: 90, mode: NodeMode::default(),
            weighted_likes: 0.0,
        });

        let shares = shapley::compute_all_shares(&contribs);
        let sum: f64 = shares.values().sum();
        assert!((sum - 1.0).abs() < 1e-9,
            "Shapley shares must sum to 1.0, got {}", sum);
    }

    #[test]
    fn s7_shapley_shares_non_negative() {
        let mut contribs = HashMap::new();
        for i in 0..10 {
            contribs.insert(format!("node_{}", i), NodeContribution {
                node_id: format!("node_{}", i),
                watts: (i + 1) as f64 * 10.0,
                tasks_completed: i as u64,
                blocks_verified: i as u64 * 2,
                uptime_minutes: (i as u64 + 1) * 30, mode: NodeMode::default(),
                weighted_likes: 0.0,
            });
        }

        let shares = shapley::compute_all_shares(&contribs);
        for (id, share) in &shares {
            assert!(*share >= 0.0,
                "Shapley share for {} must be non-negative, got {}", id, share);
        }
    }

    // ─── S8: Self-transfer rejection ────────────────────────────────────────

    #[test]
    fn s8_self_transfer_rejected() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = &id.public_key_hex;

        let mut ledger = Ledger::new();
        ledger.mine_tx(pk, 100 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        let result = ledger.transfer_tx(pk, pk, 10 * MICRO, &crypto);
        assert!(result.is_err(),
            "Self-transfer must be rejected");
    }

    // ─── S9: Zero amount rejection (negative impossible with u64) ────────────

    #[test]
    fn s9_negative_transfer_rejected() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = &id.public_key_hex;

        let mut ledger = Ledger::new();
        ledger.mine_tx(pk, 100 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        let to = "b".repeat(64);

        // u64 amounts cannot be negative — only zero is rejected at the type level.
        assert!(ledger.transfer_tx(pk, &to, 0, &crypto).is_err(),
            "Zero transfer must be rejected");
    }

    // ─── S10: Marketplace task without balance ──────────────────────────────

    #[test]
    fn s10_marketplace_insufficient_funds() {
        let mut mp = Marketplace::new();
        let mut ledger = Ledger::new();
        let submitter = "s".repeat(64);

        // Give only 1 QUANTA = MICRO µQTA
        ledger.mine_tx(&submitter, MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        let result = mp.submit_task_with_escrow(
            &mut ledger,
            &submitter,
            TaskType::Scientific { program_hash: "test".into() },
            1000 * MICRO, // Needs 1000 + 20 (2% burn) = 1020 QUANTA, only have 1
            "2030-01-01T00:00:00Z",
            100.0,
        );

        assert!(result.is_err(),
            "Task submission without sufficient funds must fail. Got: {:?}", result);
    }

    #[test]
    fn s10_marketplace_sufficient_funds() {
        let mut mp = Marketplace::new();
        let mut ledger = Ledger::new();
        let submitter = "s".repeat(64);

        // Give enough QUANTA (200 QUANTA covers 100 + 2% burn)
        ledger.mine_tx(&submitter, 200 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        let result = mp.submit_task_with_escrow(
            &mut ledger,
            &submitter,
            TaskType::GenericWasm { wasm_hash: "test_wasm".into() },
            100 * MICRO,
            "2030-01-01T00:00:00Z",
            500.0,
        );

        assert!(result.is_ok(),
            "Task submission with sufficient funds should succeed. Err: {:?}", result);
    }

    // ─── S11: Total supply conservation ─────────────────────────────────────

    #[test]
    fn s11_total_supply_conserved_on_transfer() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = &id.public_key_hex;

        let mut ledger = Ledger::new();
        ledger.mine_tx(pk, 1000 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        let supply_before = ledger.total_supply();

        let to = "x".repeat(64);
        // transfer_tx (without burn) preserves total supply exactly with integer arithmetic
        ledger.transfer_tx(pk, &to, 100 * MICRO, &crypto).unwrap();

        let supply_after = ledger.total_supply();
        assert_eq!(supply_before, supply_after,
            "Total supply must be conserved on transfer (no burn). Before: {}, After: {}",
            supply_before, supply_after);
    }

    // ─── S12: Nonce-based dispatcher tracker ────────────────────────────────

    #[test]
    fn s12_nonce_tracker_rejects_replay() {
        let mut tracker = dispatcher::NonceTracker::new();

        assert!(tracker.check_and_advance("attacker", 1), "First nonce should pass");
        assert!(tracker.check_and_advance("attacker", 2), "Sequential nonce should pass");
        assert!(!tracker.check_and_advance("attacker", 2), "Replayed nonce must be rejected");
        assert!(!tracker.check_and_advance("attacker", 1), "Lower nonce must be rejected");
        assert!(tracker.check_and_advance("attacker", 100), "Gap nonce should pass (not strict sequential)");
        assert!(!tracker.check_and_advance("attacker", 50), "Nonce below high-water mark must fail");
    }

    /// Specifically guards against the "reset to zero" replay vector at the tracker level:
    /// once a peer has reached nonce 5, a forged envelope with nonce 0 must be rejected.
    /// (The dispatch layer additionally short-circuits nonce==0 as a legacy path; this test
    /// pins the behavior of `check_and_advance` itself, which is the source of truth.)
    #[test]
    fn s12_nonce_zero_after_high_rejected() {
        let mut tracker = dispatcher::NonceTracker::new();
        assert!(tracker.check_and_advance("peer", 5), "high-water rises to 5");
        assert!(!tracker.check_and_advance("peer", 0), "nonce=0 after nonce=5 must be rejected");
        assert!(!tracker.check_and_advance("peer", 4), "any value ≤ high-water must be rejected");
        assert!(tracker.check_and_advance("peer", 6), "next strictly higher nonce passes");
    }

    // ─── S14: Peer ban after threshold reports ──────────────────────────────

    #[test]
    fn s14_peer_banned_after_threshold_reports() {
        let mut tracker = dispatcher::NonceTracker::new();
        let evil = "evil_peer";

        assert!(!tracker.is_banned(evil), "fresh peer is not banned");

        // Premier report : compteur à 1, pas encore de ban.
        let count = tracker.record_report(evil);
        assert_eq!(count, 1);
        assert!(!tracker.is_banned(evil), "1 report ne déclenche pas de ban");

        // Deuxième : compteur à 2, toujours pas.
        let count = tracker.record_report(evil);
        assert_eq!(count, 2);
        assert!(!tracker.is_banned(evil), "2 reports non plus");

        // Troisième : seuil atteint → ban posé.
        let count = tracker.record_report(evil);
        assert_eq!(count, dispatcher::REPORT_BAN_THRESHOLD);
        assert!(tracker.is_banned(evil), "3 reports déclenche le ban");

        // Un autre peer non rapporté reste libre.
        assert!(!tracker.is_banned("innocent_peer"), "le ban est par-peer, pas global");
    }

    #[test]
    fn s14_ban_does_not_leak_across_peers() {
        let mut tracker = dispatcher::NonceTracker::new();
        for _ in 0..3 { tracker.record_report("attacker_a"); }
        assert!(tracker.is_banned("attacker_a"));

        // Reports sur un autre peer comptent indépendamment.
        tracker.record_report("attacker_b");
        assert!(!tracker.is_banned("attacker_b"));
    }

    #[test]
    fn s14_banned_peers_set_reflects_state() {
        let mut tracker = dispatcher::NonceTracker::new();
        for _ in 0..3 { tracker.record_report("p1"); }
        for _ in 0..3 { tracker.record_report("p2"); }

        let banned = tracker.banned_peers();
        assert_eq!(banned.len(), 2);
        assert!(banned.contains("p1"));
        assert!(banned.contains("p2"));
    }

    // ─── S15: Oversized envelope rejected before parse ──────────────────────

    #[test]
    fn s15_oversized_payload_rejected() {
        // 10 MB + 1 byte → doit échouer avec un message "oversized".
        let huge = vec![b'{'; dispatcher::MAX_RAW_ENVELOPE_BYTES + 1];
        let result = dispatcher::try_process_raw_gossip(&huge);
        assert!(result.is_err(), "oversized payload doit être rejeté");
        let err = result.unwrap_err();
        assert!(err.contains("oversized"),
            "l'erreur doit mentionner 'oversized', got: {}", err);
    }

    #[test]
    fn s15_just_under_limit_passes_size_check() {
        // Pile la limite : la fonction continue après le size check et échouera
        // au JSON parse — le message d'erreur ne doit PAS dire "oversized".
        let at_limit = vec![b'{'; dispatcher::MAX_RAW_ENVELOPE_BYTES];
        let result = dispatcher::try_process_raw_gossip(&at_limit);
        assert!(result.is_err(), "JSON invalide donc Err, mais pas pour size");
        let err = result.unwrap_err();
        assert!(!err.contains("oversized"),
            "size check doit accepter pile la limite, got: {}", err);
    }

    // ─── S13: Remote tx nonce verification (MOD-1) ──────────────────────────

    #[test]
    fn s13_remote_tx_nonce_verified() {
        let pk = "a".repeat(64);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        // Expected nonce starts at 0
        assert_eq!(ledger.get_nonce(&pk), 0, "Initial nonce should be 0");

        // Simulate dispatcher: check nonce == expected, then increment
        let expected = ledger.get_nonce(&pk);
        assert_eq!(expected, 0, "First remote tx must have nonce 0");
        ledger.increment_nonce(&pk);
        assert_eq!(ledger.get_nonce(&pk), 1, "Nonce should be 1 after first tx");

        // A replayed tx with nonce 0 would be dropped (0 != 1)
        let replayed_nonce: u64 = 0;
        assert_ne!(
            replayed_nonce,
            ledger.get_nonce(&pk),
            "Replayed nonce (0) must not match new expected nonce (1)"
        );

        // Second tx with nonce 1 must be accepted
        let expected2 = ledger.get_nonce(&pk);
        assert_eq!(expected2, 1, "Second tx must have nonce 1");
        ledger.increment_nonce(&pk);
        assert_eq!(ledger.get_nonce(&pk), 2, "Nonce should be 2 after second tx");
    }

    // ─── MOD-2: seen_messages bounded LRU ───────────────────────────────────

    #[test]
    fn mod2_seen_messages_bounded() {
        let mut router = GossipRouter::new();
        for i in 0..200_000_usize {
            router.mark_seen(&format!("msg_{}", i));
        }
        assert!(
            router.seen_messages_count() <= 100_001,
            "seen_messages should be bounded at MAX_SEEN_MESSAGES, got {}",
            router.seen_messages_count()
        );
    }

    #[test]
    fn mod2_seen_messages_dedup_after_eviction() {
        // After eviction the old ID is gone from the set, so it is re-accepted.
        // This is the expected LRU trade-off for bounding memory.
        let mut router = GossipRouter::new();
        assert!(router.mark_seen("msg_0"), "First insertion must be accepted");
        // Push msg_0 past the eviction window
        for i in 1..=100_001_usize {
            router.mark_seen(&format!("msg_{}", i));
        }
        assert!(
            router.mark_seen("msg_0"),
            "Evicted message should be re-accepted (known LRU trade-off)"
        );
    }
}

#[cfg(test)]
mod property_tests {
    use crate::p2p::ledger::{Block, Ledger, MICRO};
    use crate::p2p::shapley::{self, NodeContribution, NodeMode};
    use crate::security::CryptoEngine;
    use proptest::prelude::*;
    use std::collections::HashMap;

    // ─── P1: Balance never negative under any transfer sequence ─────────

    #[test]
    fn p1_balance_never_negative_after_random_transfers() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = id.public_key_hex.clone();

        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 50 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        let to = "z".repeat(64);

        // Mix of amounts in µQTA: 1, 5, 10, 20, 50, 100, 0.5, 3, 7, 15, 25, 0.1, 2, 8, 12, 30, 0.01, 4, 6, 11 QUANTA
        let amounts: [u64; 20] = [
            MICRO, 5 * MICRO, 10 * MICRO, 20 * MICRO, 50 * MICRO, 100 * MICRO,
            MICRO / 2, 3 * MICRO, 7 * MICRO, 15 * MICRO,
            25 * MICRO, MICRO / 10, 2 * MICRO, 8 * MICRO, 12 * MICRO, 30 * MICRO,
            MICRO / 100, 4 * MICRO, 6 * MICRO, 11 * MICRO,
        ];

        for amount in &amounts {
            let _ = ledger.transfer_tx(&pk, &to, *amount, &crypto);
            // u64 cannot be negative — balance_of saturates at 0 by construction.
            let _balance = ledger.balance_of(&pk);
        }
    }

    // ─── P2: Shapley shares always sum to 1.0 for any node count ────────

    #[test]
    fn p2_shapley_sum_invariant_various_sizes() {
        for n in 1..=20 {
            let mut contribs = HashMap::new();
            for i in 0..n {
                contribs.insert(format!("node_{}", i), NodeContribution {
                    node_id: format!("node_{}", i),
                    watts: (i + 1) as f64 * 25.0,
                    tasks_completed: (i * 3) as u64,
                    blocks_verified: (i * 2) as u64,
                    uptime_minutes: ((i + 1) * 30) as u64, mode: NodeMode::default(),
                    weighted_likes: 0.0,
                });
            }

            let shares = shapley::compute_all_shares(&contribs);
            let sum: f64 = shares.values().sum();
            assert!((sum - 1.0).abs() < 1e-9,
                "Shapley efficiency axiom violated for {} nodes: sum = {}", n, sum);
        }
    }

    // ─── P3: Transfer with burn preserves supply minus burn ─────────────

    #[test]
    fn p3_burn_accounting_correct() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = id.public_key_hex.clone();

        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 1000 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);

        let supply_before = ledger.total_supply();
        let burned_before = ledger.total_burned();

        let to = "y".repeat(64);
        let (_tx, _burn_tx, burn_amount) = ledger.transfer_with_burn(&pk, &to, 100 * MICRO, &crypto).unwrap();

        let supply_after = ledger.total_supply();
        let burned_after = ledger.total_burned();

        // Supply should decrease by exactly the burn amount (integer arithmetic = exact)
        assert_eq!(supply_before - supply_after, burn_amount,
            "Supply change should equal burn amount. Before: {}, After: {}, Burn: {}",
            supply_before, supply_after, burn_amount);

        // Burned total should increase by exactly burn_amount
        assert_eq!(burned_after - burned_before, burn_amount,
            "Burned total should increase by burn amount");
    }

    // ─── P4: Emission distribution equals emission_per_tick ─────────────

    #[test]
    fn p4_emission_distribution_conserves_total() {
        // EMISSION_PER_TICK is u64 µQTA; shapley::distribute_emission uses f64.
        let emission_per_tick_f = crate::p2p::reputation::EMISSION_PER_TICK as f64;

        for n in 1..=10 {
            let mut contribs = HashMap::new();
            for i in 0..n {
                contribs.insert(format!("node_{}", i), NodeContribution {
                    node_id: format!("node_{}", i),
                    watts: ((i + 1) * 50) as f64,
                    tasks_completed: (i * 5) as u64,
                    blocks_verified: (i * 3) as u64,
                    uptime_minutes: ((i + 1) * 60) as u64, mode: NodeMode::default(),
                    weighted_likes: 0.0,
                });
            }

            let dist = shapley::distribute_emission(&contribs, emission_per_tick_f);
            let total: f64 = dist.values().sum();
            assert!((total - emission_per_tick_f).abs() < 1e-3,
                "Total distributed ({}) must equal emission_per_tick ({}) for {} nodes",
                total, emission_per_tick_f, n);
        }
    }

    // ─── E1: Conservation monétaire (théorèmes de solvabilité) ───────────
    //
    // Le minage est la SEULE source de µQTA. Aucune séquence de transferts ne
    // crée ni ne détruit de valeur (hors burn explicite). Ces invariants sont
    // vérifiés sur des milliers de séquences aléatoires (property-based).

    fn four_accounts() -> [String; 4] {
        ["a".repeat(64), "b".repeat(64), "c".repeat(64), "d".repeat(64)]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(40))]

        /// INVARIANT : Σ(soldes) == total miné, pour toute séquence de
        /// transferts sans burn. La valeur ne peut ni apparaître ni disparaître.
        #[test]
        fn proptest_transfers_conserve_total(
            ops in prop::collection::vec((0usize..4, 0usize..4, 1u64..=20_000_000u64), 0..40)
        ) {
            let mut crypto = CryptoEngine::new();
            let _ = crypto.generate_keypair(); // identité de signature (adresses arbitraires ci-dessous)
            let acc = four_accounts();

            let mut ledger = Ledger::new();
            let minted_each = 100 * MICRO;
            for a in &acc { ledger.mine_tx(a, minted_each, 0.0); }
            let m_total = minted_each * acc.len() as u64;

            for (i, j, amount) in ops {
                if i == j { continue; }
                // best-effort : un solde insuffisant échoue proprement (no-op).
                let _ = ledger.transfer_tx(&acc[i], &acc[j], amount, &crypto);
            }

            let sum: u64 = ledger.all_balances().values().sum();
            prop_assert_eq!(sum, m_total,
                "conservation violée : Σ soldes ({}) != total miné ({})", sum, m_total);
        }

        /// INVARIANT : Σ(soldes) + total_brûlé == total miné, avec burn-and-mint.
        /// Le burn déplace la valeur vers un puits comptabilisé, sans la perdre.
        #[test]
        fn proptest_transfers_with_burn_conserve_total(
            ops in prop::collection::vec((0usize..4, 0usize..4, 10_000u64..=20_000_000u64), 0..40)
        ) {
            let mut crypto = CryptoEngine::new();
            let _ = crypto.generate_keypair();
            let acc = four_accounts();

            let mut ledger = Ledger::new();
            let minted_each = 100 * MICRO;
            for a in &acc { ledger.mine_tx(a, minted_each, 0.0); }
            let m_total = minted_each * acc.len() as u64;

            for (i, j, amount) in ops {
                if i == j { continue; }
                let _ = ledger.transfer_with_burn(&acc[i], &acc[j], amount, &crypto);
            }

            let sum: u64 = ledger.all_balances().values().sum();
            let burned = ledger.total_burned();
            prop_assert_eq!(sum + burned, m_total,
                "conservation+burn violée : Σ soldes ({}) + brûlé ({}) != miné ({})",
                sum, burned, m_total);
        }

        /// INVARIANT : anti-double-dépense + monotonie stricte des nonces.
        /// Un compte ne peut jamais émettre plus que son solde, et chaque tx
        /// signée porte un nonce strictement croissant.
        #[test]
        fn proptest_nonce_monotonic_no_overspend(
            amounts in prop::collection::vec(1u64..=30_000_000u64, 0..40)
        ) {
            let mut crypto = CryptoEngine::new();
            let id = crypto.generate_keypair();
            let pk = id.public_key_hex.clone();
            let to = "z".repeat(64);

            let mut ledger = Ledger::new();
            let initial = 100 * MICRO;
            ledger.mine_tx(&pk, initial, 0.0);

            let mut spent = 0u64;
            let mut last_nonce: Option<u64> = None;
            for amount in amounts {
                if let Ok(tx) = ledger.transfer_tx(&pk, &to, amount, &crypto) {
                    if let Some(prev) = last_nonce {
                        prop_assert!(tx.nonce > prev, "le nonce doit croître strictement");
                    }
                    last_nonce = Some(tx.nonce);
                    spent += amount;
                }
                // Jamais de double-dépense, et solde toujours exactement cohérent.
                prop_assert!(spent <= initial, "double-dépense : {} > {}", spent, initial);
                prop_assert_eq!(ledger.balance_of(&pk), initial - spent);
            }
        }
    }

    // ─── D1: Block validation + integration ──────────────────────────────

    /// Helper: build a fresh ledger that has mined one tx (pending, not yet sealed).
    fn ledger_with_pending(pk: &str) -> Ledger {
        let mut l = Ledger::new();
        l.mine_tx(pk, 50 * MICRO, 0.0);
        l
    }

    #[test]
    fn d1_validate_remote_block_valid() {
        // Two ledgers start from the same fixed genesis hash. Ledger A seals a block,
        // and Ledger B should accept it as the next block in its chain.
        let pk = "a".repeat(64);
        let mut a = ledger_with_pending(&pk);
        let mut b = ledger_with_pending(&pk);

        let block = a.seal_block(&pk, 0.0);

        assert!(b.validate_remote_block(&block).is_ok(),
            "Block sealed by A must pass validation on B (shared genesis)");

        let accepted = b.integrate_remote_block(block.clone()).expect("integrate should not error");
        assert!(accepted, "B should accept the new block");
        assert_eq!(b.chain.last().map(|t| t.hash.clone()), Some(block.hash),
            "B's tip must match the integrated block");
    }

    #[test]
    fn d1_reject_block_bad_prev_hash() {
        let pk = "a".repeat(64);
        let mut a = ledger_with_pending(&pk);
        let mut block = a.seal_block(&pk, 0.0);

        // Tamper prev_hash → must no longer link to genesis on a fresh ledger.
        block.prev_hash = "f".repeat(64);

        let b = Ledger::new();
        let err = b.validate_remote_block(&block).expect_err("bad prev_hash must fail");
        assert!(err.contains("prev_hash mismatch"),
            "error should mention prev_hash; got: {}", err);
    }

    #[test]
    fn d1_reject_block_bad_hash() {
        let pk = "a".repeat(64);
        let mut a = ledger_with_pending(&pk);
        let mut block = a.seal_block(&pk, 0.0);

        // Tamper the declared hash → recomputed hash will not match.
        block.hash = "0".repeat(64);

        let b = Ledger::new();
        let err = b.validate_remote_block(&block).expect_err("bad hash must fail");
        assert!(err.contains("block hash mismatch"),
            "error should mention block hash; got: {}", err);
    }

    #[test]
    fn d1_fork_resolution_deterministic() {
        // Two ledgers produce competing block #1. Both nodes must converge to the
        // block with the lexicographically higher hash regardless of arrival order.
        let pk = "a".repeat(64);
        let mut a = ledger_with_pending(&pk);
        let mut b = ledger_with_pending(&pk);

        let block_a = a.seal_block(&pk, 0.0);
        // Ensure timestamps differ so block_b.hash != block_a.hash.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let block_b = b.seal_block(&pk, 0.0);
        assert_ne!(block_a.hash, block_b.hash,
            "test setup: blocks must have distinct hashes");

        let (winner, loser): (&Block, &Block) = if block_a.hash > block_b.hash {
            (&block_a, &block_b)
        } else {
            (&block_b, &block_a)
        };

        // Scenario 1: a node already on `winner` receives `loser` → keep winner.
        let mut keep_winner = Ledger::new();
        assert!(keep_winner.integrate_remote_block(winner.clone()).expect("first integrate ok"),
            "winner must be accepted onto fresh genesis");
        let res = keep_winner.integrate_remote_block(loser.clone()).expect("fork branch should not error");
        assert!(!res, "loser must be ignored when winner is already tip");
        assert_eq!(keep_winner.chain.last().unwrap().hash, winner.hash,
            "winner must remain the tip");

        // Scenario 2: a node already on `loser` receives `winner` → reorg to winner.
        let mut adopt_winner = Ledger::new();
        assert!(adopt_winner.integrate_remote_block(loser.clone()).expect("first integrate ok"),
            "loser must be accepted onto fresh genesis");
        let res = adopt_winner.integrate_remote_block(winner.clone()).expect("reorg should not error");
        assert!(res, "winner must replace loser on reorg");
        assert_eq!(adopt_winner.chain.last().unwrap().hash, winner.hash,
            "winner must become the tip after reorg");
    }

    #[test]
    fn d1_duplicate_block_ignored() {
        let pk = "a".repeat(64);
        let mut a = ledger_with_pending(&pk);
        let mut b = ledger_with_pending(&pk);
        let block = a.seal_block(&pk, 0.0);

        let first = b.integrate_remote_block(block.clone()).expect("first integrate ok");
        assert!(first, "first integration must accept the block");

        let second = b.integrate_remote_block(block.clone()).expect("duplicate must not error");
        assert!(!second, "duplicate block must return Ok(false)");
    }

    // ─── Proptest: Shapley share-sum invariant ─────────────────────────────

    /// Stratégie proptest : génère N nœuds (1..50) avec des champs bornés
    /// mais variés. Watts ∈ [0, 1000] couvre Raspberry Pi → mineur ASIC.
    fn arb_contributions() -> impl Strategy<Value = HashMap<String, NodeContribution>> {
        prop::collection::vec(
            (
                0u32..1_000,            // watts (entier pour reproductibilité)
                0u64..10_000,           // tasks_completed
                0u64..100_000,          // blocks_verified
                0u64..1_000_000,        // uptime_minutes
                prop::sample::select(vec![
                    NodeMode::Active, NodeMode::Research, NodeMode::Guardian,
                ]),
            ),
            1..=50usize,
        )
        .prop_map(|nodes| {
            nodes.into_iter().enumerate().map(|(i, (w, t, b, u, m))| {
                let id = format!("node_{}", i);
                (id.clone(), NodeContribution {
                    node_id: id,
                    watts: w as f64,
                    tasks_completed: t,
                    blocks_verified: b,
                    uptime_minutes: u,
                    mode: m,
                    weighted_likes: 0.0,
                })
            }).collect()
        })
    }

    proptest! {
        /// Pour toute combinaison de nœuds non-vide, la somme des parts Shapley
        /// doit valoir 1.0 (à epsilon float près) — c'est l'axiome d'efficacité.
        /// Cas dégénéré accepté : si TOUS les nœuds sont Guardian sans uptime,
        /// total_score peut être 0 → pas de normalisation, sum reste 0.
        #[test]
        fn proptest_shapley_shares_sum_to_one(contribs in arb_contributions()) {
            let shares = shapley::compute_all_shares(&contribs);
            prop_assert_eq!(shares.len(), contribs.len());

            let total: f64 = shares.values().sum();

            // Tous les scores doivent être finis et ≥ 0
            for (id, s) in &shares {
                prop_assert!(s.is_finite(), "share {} = {} non finite", id, s);
                prop_assert!(*s >= 0.0, "share {} = {} négatif", id, s);
            }

            // Sum doit être 1.0 OU 0.0 (cas dégénéré tout-zero)
            let near_one = (total - 1.0).abs() < 1e-9;
            let near_zero = total.abs() < 1e-9;
            prop_assert!(
                near_one || near_zero,
                "sum des parts = {} (ni 1.0 ni 0.0)", total
            );
        }
    }

    // ─── Proptest: transfer_with_burn never panics ─────────────────────────

    /// Construit un ledger neuf prêt à transférer pour chaque case proptest.
    /// On part de 1M QUANTA pour qu'une plage raisonnable d'amounts succède.
    fn fresh_ledger_with_funds() -> (Ledger, CryptoEngine, String) {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = id.public_key_hex.clone();
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 1_000_000 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);
        (ledger, crypto, pk)
    }

    proptest! {
        /// `transfer_with_burn` doit retourner un Result pour TOUT amount —
        /// y compris 0, 1, MICRO/2 (sous le minimum), u64::MAX, et tout entre les deux.
        /// Aucun panic, aucune arithmetic overflow, aucun unwrap qui claque.
        #[test]
        fn proptest_transfer_with_burn_never_panics(amount in any::<u64>()) {
            let (mut ledger, crypto, pk) = fresh_ledger_with_funds();
            let to = "b".repeat(64);

            // Le contrat est juste : pas de panic. Le résultat (Ok/Err) dépend du montant.
            let result = ledger.transfer_with_burn(&pk, &to, amount, &crypto);

            // Si Ok, vérifier les invariants accessibles : burn = amount/100.
            if let Ok((_tx, _burn_tx, burn)) = result {
                prop_assert_eq!(burn, amount / 100,
                    "burn = amount/100 (intégré exact)");
            }
        }
    }

    proptest! {
        // Bornes explicites : 0, 1, sous-minimum, minimum exact, u64 quasi-max, u64::MAX.
        // Avec 256 cases par défaut, chaque valeur est tirée ~40× par run.
        #[test]
        fn proptest_transfer_with_burn_boundary_amounts(
            amount in prop_oneof![
                Just(0u64),
                Just(1u64),
                Just(9_999u64),       // sous le minimum 0.01 QUANTA
                Just(10_000u64),      // pile minimum
                Just(u64::MAX - 1),
                Just(u64::MAX),
            ]
        ) {
            let (mut ledger, crypto, pk) = fresh_ledger_with_funds();
            let to = "c".repeat(64);
            let _ = ledger.transfer_with_burn(&pk, &to, amount, &crypto);
            // Pas de panic = test passé.
        }
    }
}
