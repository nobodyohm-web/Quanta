//! Proof-of-Stake Leader Election with VRF (Verifiable Random Function).
//!
//! # How it works
//!
//! Each block slot (= chain height) has exactly one elected leader who has the
//! right to propose a block. The leader is selected deterministically based on:
//!
//!   leader_seed = BLAKE3(prev_block_hash || slot_number)
//!   leader_index = seed_u64 % total_weighted_stake
//!
//! The weight of each validator is:
//!   weight = staked_amount + (reputation_score * 10_000)
//!
//! This ensures:
//!   1. **Determinism** — All nodes compute the same leader for the same slot.
//!   2. **Sybil resistance** — Creating many identities without stake gives no advantage.
//!   3. **Fairness** — Higher stake + contribution = higher chance of being elected.
//!   4. **Liveness** — If the elected leader is offline, a fallback mechanism
//!      allows the next-in-line after a timeout.
//!
//! # Minimum Stake
//!
//! A validator must stake at least `MIN_VALIDATOR_STAKE` (1 QUANTA = 1_000_000 µQTA)
//! to be eligible for block proposal. This prevents zero-cost Sybil attacks.
//!
//! # Fork Resolution
//!
//! When two blocks at the same height are received:
//!   - The block from the elected leader wins.
//!   - If neither is from the elected leader, fall back to hash comparison.
//!
//! This replaces the naive "higher hash wins" with stake-weighted authority.

use std::collections::HashMap;

/// Minimum stake required to participate in consensus (1 QUANTA = 1_000_000 µQTA).
pub const MIN_VALIDATOR_STAKE: u64 = 1_000_000;

/// Timeout in seconds after which the next validator in line can propose.
pub const LEADER_TIMEOUT_SECS: u64 = 30;

/// Maximum number of fallback rounds before any validator can propose.
pub const MAX_FALLBACK_ROUNDS: u32 = 3;

/// A validator's identity and weight in the consensus.
#[derive(Debug, Clone)]
pub struct Validator {
    /// Public key (hex)
    pub pk: String,
    /// Total staked amount in µQTA
    pub stake: u64,
    /// Reputation score (0-100)
    pub reputation: u64,
}

impl Validator {
    /// Consensus weight = stake + reputation bonus.
    /// Reputation is scaled to be meaningful relative to stake
    /// (100 rep = 1 QUANTA worth of weight = 1_000_000 µQTA).
    pub fn weight(&self) -> u64 {
        self.stake.saturating_add(self.reputation.saturating_mul(10_000))
    }
}

/// Compute the elected leader for a given slot (block height).
///
/// Returns the public key of the elected leader, or None if no validators
/// are eligible (all stake below minimum).
///
/// The algorithm:
/// 1. Filter validators with stake >= MIN_VALIDATOR_STAKE
/// 2. Sort by public key (deterministic ordering)
/// 3. Compute VRF seed from previous block hash + slot number
/// 4. Map seed to a position in the cumulative weight distribution
pub fn elect_leader(
    prev_block_hash: &str,
    slot: u64,
    validators: &[Validator],
) -> Option<String> {
    // 1. Filter eligible validators and sort deterministically
    let mut eligible: Vec<&Validator> = validators
        .iter()
        .filter(|v| v.stake >= MIN_VALIDATOR_STAKE)
        .collect();

    if eligible.is_empty() {
        return None;
    }

    // Sort by pk for deterministic ordering across all nodes
    eligible.sort_by(|a, b| a.pk.cmp(&b.pk));

    // 2. Compute VRF seed
    let seed = compute_vrf_seed(prev_block_hash, slot, 0);

    // 3. Weighted random selection
    let total_weight: u64 = eligible.iter().map(|v| v.weight()).sum();
    if total_weight == 0 {
        return Some(eligible[0].pk.clone());
    }

    let target = seed % total_weight;
    let mut cumulative = 0u64;

    for v in &eligible {
        cumulative += v.weight();
        if target < cumulative {
            return Some(v.pk.clone());
        }
    }

    // Fallback (should not reach here)
    Some(eligible.last().unwrap().pk.clone())
}

/// Compute the fallback leader for a given round.
///
/// If the primary leader hasn't proposed within `LEADER_TIMEOUT_SECS`,
/// the next validator in the weighted rotation gets a chance.
pub fn elect_fallback_leader(
    prev_block_hash: &str,
    slot: u64,
    fallback_round: u32,
    validators: &[Validator],
) -> Option<String> {
    let mut eligible: Vec<&Validator> = validators
        .iter()
        .filter(|v| v.stake >= MIN_VALIDATOR_STAKE)
        .collect();

    if eligible.is_empty() {
        return None;
    }

    eligible.sort_by(|a, b| a.pk.cmp(&b.pk));

    let seed = compute_vrf_seed(prev_block_hash, slot, fallback_round);
    let total_weight: u64 = eligible.iter().map(|v| v.weight()).sum();
    if total_weight == 0 {
        return Some(eligible[0].pk.clone());
    }

    let target = seed % total_weight;
    let mut cumulative = 0u64;

    for v in &eligible {
        cumulative += v.weight();
        if target < cumulative {
            return Some(v.pk.clone());
        }
    }

    Some(eligible.last().unwrap().pk.clone())
}

/// Check if a given public key is the elected leader (or valid fallback) for a slot.
///
/// Returns `(is_valid, is_primary)`:
/// - `is_valid`: true if this pk is allowed to propose at this slot
/// - `is_primary`: true if this pk is the primary (not fallback) leader
pub fn is_valid_proposer(
    pk: &str,
    prev_block_hash: &str,
    slot: u64,
    elapsed_secs: u64,
    validators: &[Validator],
) -> (bool, bool) {
    // Check primary leader
    if let Some(primary) = elect_leader(prev_block_hash, slot, validators) {
        if primary == pk {
            return (true, true);
        }
    }

    // Check fallback rounds (each round unlocks after LEADER_TIMEOUT_SECS)
    if elapsed_secs >= LEADER_TIMEOUT_SECS {
        let rounds = ((elapsed_secs - LEADER_TIMEOUT_SECS) / LEADER_TIMEOUT_SECS + 1)
            .min(MAX_FALLBACK_ROUNDS as u64);

        for round in 1..=rounds as u32 {
            if let Some(fallback) = elect_fallback_leader(
                prev_block_hash, slot, round, validators,
            ) {
                if fallback == pk {
                    return (true, false);
                }
            }
        }
    }

    // After all fallback rounds exhausted, any eligible validator can propose
    if elapsed_secs >= LEADER_TIMEOUT_SECS * (MAX_FALLBACK_ROUNDS as u64 + 1) {
        let has_min_stake = validators
            .iter()
            .any(|v| v.pk == pk && v.stake >= MIN_VALIDATOR_STAKE);
        return (has_min_stake, false);
    }

    (false, false)
}

/// Build the validator set from current ledger + reputation state.
///
/// This is the bridge between the app state and the consensus engine.
pub fn build_validator_set(
    stakes: &HashMap<String, u64>,
    reputations: &HashMap<String, u64>,
) -> Vec<Validator> {
    let mut validators = Vec::new();
    // Merge keys from both maps
    let mut all_pks: Vec<&String> = stakes.keys().chain(reputations.keys()).collect();
    all_pks.sort();
    all_pks.dedup();

    for pk in all_pks {
        let stake = stakes.get(pk).copied().unwrap_or(0);
        let reputation = reputations.get(pk).copied().unwrap_or(0);
        // Include if they have any stake (even below minimum — they just won't be elected)
        if stake > 0 || reputation > 0 {
            validators.push(Validator {
                pk: pk.clone(),
                stake,
                reputation,
            });
        }
    }
    validators
}

/// Deterministic VRF seed from block hash + slot + round.
/// Uses BLAKE3 for speed and collision resistance.
fn compute_vrf_seed(prev_hash: &str, slot: u64, round: u32) -> u64 {
    let input = format!("{}:{}:{}", prev_hash, slot, round);
    let hash = blake3::hash(input.as_bytes());
    let bytes = hash.as_bytes();
    u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_validators() -> Vec<Validator> {
        vec![
            Validator { pk: "alice".into(), stake: 5_000_000, reputation: 90 },
            Validator { pk: "bob".into(), stake: 2_000_000, reputation: 50 },
            Validator { pk: "charlie".into(), stake: 500_000, reputation: 80 }, // below MIN_STAKE
        ]
    }

    #[test]
    fn elect_leader_deterministic() {
        let vals = test_validators();
        let l1 = elect_leader("abc123", 1, &vals);
        let l2 = elect_leader("abc123", 1, &vals);
        assert_eq!(l1, l2, "Same inputs must produce same leader");
    }

    #[test]
    fn elect_leader_changes_with_slot() {
        let vals = test_validators();
        let leaders: Vec<_> = (0..20)
            .map(|s| elect_leader("abc123", s, &vals).unwrap())
            .collect();
        // With different slots, we should eventually get different leaders
        let unique: std::collections::HashSet<_> = leaders.iter().collect();
        assert!(unique.len() > 1, "Different slots should elect different leaders over 20 rounds");
    }

    #[test]
    fn below_min_stake_not_elected() {
        let vals = test_validators();
        // Charlie has only 500k, minimum is 1M
        for slot in 0..100 {
            let leader = elect_leader("test", slot, &vals).unwrap();
            assert_ne!(leader, "charlie", "Below-min-stake validator should never be elected");
        }
    }

    #[test]
    fn primary_is_valid_proposer() {
        let vals = test_validators();
        let leader = elect_leader("prev_hash", 5, &vals).unwrap();
        let (valid, primary) = is_valid_proposer(&leader, "prev_hash", 5, 0, &vals);
        assert!(valid, "Elected leader must be valid");
        assert!(primary, "Elected leader must be primary");
    }

    #[test]
    fn non_leader_rejected_initially() {
        let vals = test_validators();
        let leader = elect_leader("prev_hash", 5, &vals).unwrap();
        let other = if leader == "alice" { "bob" } else { "alice" };
        let (valid, _) = is_valid_proposer(other, "prev_hash", 5, 0, &vals);
        assert!(!valid, "Non-leader should be rejected before timeout");
    }

    #[test]
    fn fallback_after_timeout() {
        let vals = test_validators();
        let leader = elect_leader("prev_hash", 5, &vals).unwrap();
        let other = if leader == "alice" { "bob" } else { "alice" };
        // After enough time, fallback should allow the other validator
        let (valid, primary) = is_valid_proposer(
            other, "prev_hash", 5,
            LEADER_TIMEOUT_SECS * (MAX_FALLBACK_ROUNDS as u64 + 2),
            &vals,
        );
        assert!(valid, "After all fallback rounds, eligible validators can propose");
        assert!(!primary);
    }

    #[test]
    fn no_eligible_validators() {
        let vals = vec![
            Validator { pk: "low".into(), stake: 100, reputation: 0 },
        ];
        assert!(elect_leader("hash", 1, &vals).is_none());
    }

    #[test]
    fn higher_stake_elected_more_often() {
        let vals = test_validators();
        let mut counts: HashMap<String, u32> = HashMap::new();
        for slot in 0..1000 {
            let leader = elect_leader("test_fairness", slot, &vals).unwrap();
            *counts.entry(leader).or_insert(0) += 1;
        }
        let alice_count = counts.get("alice").copied().unwrap_or(0);
        let bob_count = counts.get("bob").copied().unwrap_or(0);
        // Alice has 5M stake vs Bob's 2M — she should win more often
        assert!(
            alice_count > bob_count,
            "Higher stake should be elected more often: alice={} bob={}",
            alice_count, bob_count
        );
    }

    #[test]
    fn build_validator_set_merges_maps() {
        let mut stakes = HashMap::new();
        stakes.insert("alice".into(), 5_000_000u64);
        stakes.insert("bob".into(), 1_000_000u64);

        let mut reps = HashMap::new();
        reps.insert("alice".into(), 95u64);
        reps.insert("charlie".into(), 80u64);

        let vals = build_validator_set(&stakes, &reps);
        assert_eq!(vals.len(), 3); // alice, bob, charlie
        let alice = vals.iter().find(|v| v.pk == "alice").unwrap();
        assert_eq!(alice.stake, 5_000_000);
        assert_eq!(alice.reputation, 95);
    }
}
