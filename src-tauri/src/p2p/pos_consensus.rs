//! Proof-of-Stake leader election — verifiable, stake-weighted, buried-beacon seeded.
//!
//! > **Naming honesty.** This is a *deterministic, publicly verifiable* election,
//! > **not** a cryptographic VRF — there is no per-validator secret key, so the
//! > output is a public function of public data. Consequence: future leaders are
//! > **publicly predictable** (a DoS-targeting surface still to harden). The
//! > internal `vrf` identifiers are legacy names kept for domain-tag/wire
//! > compatibility. A true secret-key VRF (ECVRF, for unpredictability) and a VDF
//! > (for grinding resistance) are roadmap items.
//!
//! # How it works
//!
//! Each block slot (= chain height) has exactly one elected leader who has the
//! right to propose a block. The leader is selected deterministically based on:
//!
//!   beacon       = BLAKE3(domain || buried_block_hash || slot)
//!   leader_seed  = BLAKE3(domain || beacon || slot || round)
//!   leader_index = seed_u64 % total_weighted_stake
//!
//! The election entropy (`beacon`) is sourced from a *buried* block —
//! `LEADER_ENTROPY_LOOKBACK` slots behind the tip — not the freshly-sealed tip.
//! This closes the obvious self-grinding vector: the validator who just sealed
//! the tip cannot tweak its contents to bias (or re-elect) themselves at the
//! next slot. Residual grinding by an older proposer is weaker and shrinks with
//! a larger lookback; full block-withholding resistance needs a VDF (see
//! SECURITY.md and the DAG-BFT consensus design doc).
//!
//! The weight of each validator is:
//!   weight = staked_amount        // ADR-002: on-chain stake ONLY
//!
//! **ADR-002 (accepté) — poids = enjeu on-chain seul.** Reputation (and any
//! other non-stake, locally-measured quantity) is **removed from the security
//! path**: it is an application signal (mining / Shapley reward), never an
//! election or quorum weight. The previous `stake + reputation × 10_000` made
//! the weight depend on each node's **local** reputation view, so two honest
//! nodes could compute different weights → different leaders → **fork**. Anchoring
//! weight to on-chain stake makes it a pure function of consensus state.
//!
//! This ensures:
//!   1. **Determinism** — weight is a pure function of on-chain stake, so all
//!      nodes at the same chain state compute the **same** weights and leader
//!      (no dependence on any node-local measurement).
//!   2. **Sybil resistance** — Creating many identities without stake gives no advantage.
//!   3. **Fairness** — Higher stake = higher chance of being elected.
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
/// **ADR-009 :** la **classe** est tranchée — *ajustable* (anti-sybil, derrière
/// constante, modifiable par **fork**, pas de gouvernance on-chain). La **valeur**
/// reste un **placeholder nominal 🛑 d'Alexandre** (§3) : son niveau sensé dépend
/// de l'**échelle monétaire** (offre, valeur du µQTA) — le noyau gravé, non
/// redéfini ici. À fixer quand l'échelle l'est ; ne bloque pas le câblage du gadget.
pub const MIN_VALIDATOR_STAKE: u64 = 1_000_000;

/// OPEN-DOOR-1 — cadence des **slots ouverts** : un bloc sur `OPEN_SLOT_EVERY_BLOCKS`
/// peut être proposé par **n'importe quelle adresse**, bondée ou non.
///
/// Le trou qu'elle ferme : `PROPOSER-1` n'autorise à proposer que les validateurs
/// bondés — sauf tant que *personne* n'a staké (amorçage permissionless). Dès
/// qu'un seul compte bonde 1 QUANTA, la porte se referme, et un nouvel arrivant
/// est piégé dans une boucle fermée : il lui faut 1 QUANTA pour staker, staker
/// pour proposer, proposer pour gagner 1 QUANTA. Il n'existe ni faucet, ni
/// airdrop, ni premine — donc aucun chemin vers sa première pièce. Le réseau se
/// fermait définitivement au premier staker.
///
/// Le prix assumé, en toutes lettres : le trilemme de vulnérabilité Sybil (Platt,
/// Platt & McBurney, 2024) démontre qu'un protocole ne peut pas être à la fois
/// *sans permission*, *résistant aux Sybils* et *gratuit*. Une entrée gratuite
/// s'achète donc en résistance Sybil. Ici l'achat est **borné et prévisible** :
/// une ferme de fausses identités ne peut capter qu'au plus `1/OPEN_SLOT_EVERY_BLOCKS`
/// de l'émission — jamais davantage, quel que soit le nombre d'identités — parce
/// que l'ouverture est **cadencée par la hauteur**, pas par le nombre de
/// prétendants. Les 15/16 restants exigent un enjeu bondé, donc la sécurité du
/// consensus reste adossée à l'enjeu.
///
/// Fonction pure de la hauteur : aucun état, aucune horloge, O(1) — chaque nœud
/// tranche identiquement (C1).
pub const OPEN_SLOT_EVERY_BLOCKS: u64 = 16;

/// OPEN-DOOR-1 — ce bloc est-il un slot ouvert ? La genèse (index 0) n'en est
/// jamais un : elle n'est proposée par personne.
pub fn is_open_slot(index: u64) -> bool {
    index > 0 && index.is_multiple_of(OPEN_SLOT_EVERY_BLOCKS)
}

/// Timeout in seconds after which the next validator in line can propose.
pub const LEADER_TIMEOUT_SECS: u64 = 30;

/// Maximum number of fallback rounds before any validator can propose.
pub const MAX_FALLBACK_ROUNDS: u32 = 3;

/// Domain-separation tag for the leader-election VRF (consensus v2).
const LEADER_VRF_DOMAIN: &[u8] = b"QUANTA-leader-vrf-v2";

/// Blocks behind the tip from which the election entropy (beacon) is sourced.
/// Sourcing from a *buried* block prevents the immediate proposer from grinding
/// freshly-sealed block contents to bias — or re-elect — themselves. Residual
/// grinding by an older proposer is weaker and shrinks as this grows; full
/// withholding resistance requires a VDF (see SECURITY.md / consensus design doc).
pub const LEADER_ENTROPY_LOOKBACK: u64 = 2;

/// A validator's identity and weight in the consensus.
#[derive(Debug, Clone)]
pub struct Validator {
    /// Public key (hex)
    pub pk: String,
    /// Total staked amount in µQTA — the **only** consensus weight (ADR-002).
    pub stake: u64,
    /// Reputation score (0-100). **Application signal only — ZERO consensus
    /// effect** (ADR-002: la réputation sort du chemin de sécurité). It is
    /// carried here for display/leaderboard purposes; it does **not** enter
    /// [`Validator::weight`], election, or quorum. Reputation is measured
    /// **locally** by each node, so letting it weigh consensus would fork the
    /// chain — which is exactly the hole ADR-002 closes.
    pub reputation: u64,
}

impl Validator {
    /// Consensus weight = **on-chain stake only** (ADR-002, accepté 2026-06-21).
    ///
    /// The weight that decides leader election and (for GADGET-2) the ⅔ quorum is
    /// a **pure function of on-chain stake** — identical on every node at the same
    /// chain state. **Reputation is deliberately NOT included**: it is measured
    /// locally (each node's own view), so a reputation-weighted election would let
    /// two honest nodes elect different leaders → **fork**. The previous formula
    /// `stake + min(reputation × 10_000, stake)` reopened exactly that hole; ADR-002
    /// removes it.
    ///
    /// ⚠️ **Do not re-introduce any non-stake term here** (reputation, Shapley,
    /// energy, uptime…). Eligibility already requires `stake >= MIN_VALIDATOR_STAKE`,
    /// so a zero-stake identity has zero weight and cannot be elected.
    pub fn weight(&self) -> u64 {
        self.stake
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

    // Sort by pk for deterministic ordering across all nodes.
    // NOTE: this sort is what makes the election **permutation-invariant** — the
    // result must not depend on the caller's validator ordering, otherwise two
    // honest nodes with the same validator set could elect different leaders and
    // fork the chain (Constitution §3: zero HashMap-iteration-order dependence).
    // Locked by the `elect_leader_is_permutation_invariant` property test.
    eligible.sort_by(|a, b| a.pk.cmp(&b.pk));

    // 2. Compute VRF seed
    let seed = compute_vrf_seed(prev_block_hash, slot, 0);

    // 3. Weighted selection over the cumulative weight distribution.
    // Accumulate in u128: each `weight()` is ≤ u64::MAX and the validator count is
    // bounded, so the sum cannot overflow u128 (max ≈ N·2^64 ≪ 2^128). This makes
    // overflow *unreachable* rather than wrapping (release) or panicking (debug) —
    // Constitution §3 (checked arithmetic, no silent wrap). For any realistic set
    // (total_weight ≤ u64::MAX) the elected leader is bit-identical to the prior
    // u64 math, so consensus behavior is unchanged.
    let total_weight: u128 = eligible.iter().map(|v| v.weight() as u128).sum();
    if total_weight == 0 {
        return Some(eligible[0].pk.clone());
    }

    let target = (seed as u128) % total_weight;
    let mut cumulative: u128 = 0;

    for v in &eligible {
        cumulative += v.weight() as u128;
        if target < cumulative {
            return Some(v.pk.clone());
        }
    }

    // Unreachable: target < total_weight == cumulative once the loop completes.
    // Fall back to the last eligible validator *without panicking* (Constitution
    // §3/§8: zero unwrap in production code).
    eligible.last().map(|v| v.pk.clone())
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

    // Permutation-invariant ordering (see `elect_leader`).
    eligible.sort_by(|a, b| a.pk.cmp(&b.pk));

    let seed = compute_vrf_seed(prev_block_hash, slot, fallback_round);
    // u128 accumulation: overflow-unreachable, behavior-identical for realistic
    // weights (see `elect_leader` for the full rationale).
    let total_weight: u128 = eligible.iter().map(|v| v.weight() as u128).sum();
    if total_weight == 0 {
        return Some(eligible[0].pk.clone());
    }

    let target = (seed as u128) % total_weight;
    let mut cumulative: u128 = 0;

    for v in &eligible {
        cumulative += v.weight() as u128;
        if target < cumulative {
            return Some(v.pk.clone());
        }
    }

    // Unreachable; non-panicking fallback (Constitution §3/§8: zero unwrap).
    eligible.last().map(|v| v.pk.clone())
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

/// Derive the per-slot election **beacon** from a *buried* block hash.
///
/// The beacon is the entropy that decides the slot's leader. Because the caller
/// (`mining_loop::pos_seal_if_leader`) sources `buried_block_hash` from a block
/// `LEADER_ENTROPY_LOOKBACK` slots behind the tip, the validator who just sealed
/// the tip cannot influence it — closing the obvious self-grinding vector. The
/// output is a hex string so it slots directly into the existing election API.
pub fn leader_beacon(buried_block_hash: &str, slot: u64) -> String {
    let mut h = blake3::Hasher::new();
    h.update(LEADER_VRF_DOMAIN);
    h.update(b"|beacon|");
    h.update(buried_block_hash.as_bytes());
    h.update(&slot.to_le_bytes());
    hex::encode(h.finalize().as_bytes())
}

/// **C-04 (AUDIT-2026-08-13) — FORK-RANK-1 : profondeur du classement d'élection.**
///
/// Le fork-choice n'a besoin que de savoir *qui, du proposeur A ou du proposeur
/// B, était le mieux élu pour cette hauteur*. Classer l'ensemble bondé entier
/// coûterait O(N²) et rendrait le départage sensible à la taille du réseau ;
/// au-delà de ce rang, tout le monde est ex æquo et le départage retombe sur le
/// critère suivant. 64 rangs, c'est déjà bien plus que le nombre de validateurs
/// qu'une partition réaliste peut voir proposer simultanément à une hauteur.
pub const ELECTION_RANK_DEPTH: u32 = 64;

/// **C-04 (AUDIT-2026-08-13) — le classement d'élection complet d'une hauteur.**
///
/// L'audit a montré que l'élection PoS existait (`elect_leader`) mais ne servait
/// **qu'au scellement** : à la réception, deux blocs concurrents à la même
/// hauteur étaient départagés par « le plus grand hash », un critère que
/// n'importe qui peut broyer sans posséder un seul µQTA. L'élection était donc
/// une politesse, pas une règle.
///
/// Cette fonction généralise `elect_leader` en un **classement total** : tirages
/// pondérés par l'enjeu, **sans remise**, sur la même graine et le même beacon
/// enterré. Le rang 0 est exactement ce que renvoie [`elect_leader`] — la
/// propriété est verrouillée par `rank0_is_exactly_elect_leader`. Le fork-choice
/// peut alors préférer le bloc du proposeur le mieux élu, ce qui rend le broyage
/// de hash inutile : pour gagner un départage il faut de l'enjeu, donc quelque
/// chose à perdre.
///
/// Sans remise, et non `elect_fallback_leader(round)` : ce dernier retire à
/// chaque round depuis l'ensemble **complet**, donc il peut ré-élire le même
/// validateur et ne définit aucun ordre total. Un fork-choice a besoin d'un
/// ordre total, sinon deux nœuds honnêtes peuvent préférer des blocs différents.
///
/// Coût borné : au plus `ELECTION_RANK_DEPTH` tirages, chacun linéaire en le
/// nombre d'éligibles restants. Renvoie les clés publiques par rang croissant.
pub fn election_ranking(beacon: &str, slot: u64, validators: &[Validator]) -> Vec<String> {
    let mut remaining: Vec<&Validator> = validators
        .iter()
        .filter(|v| v.stake >= MIN_VALIDATOR_STAKE)
        .collect();
    // Ordre permutation-invariant, pour la même raison que dans `elect_leader` :
    // deux nœuds honnêtes ayant le même ensemble doivent produire le même
    // classement, quel que soit l'ordre d'itération de la carte d'origine.
    remaining.sort_by(|a, b| a.pk.cmp(&b.pk));

    let depth = (ELECTION_RANK_DEPTH as usize).min(remaining.len());
    let mut ranking = Vec::with_capacity(depth);

    for round in 0..depth {
        // Accumulation u128 : la somme de N poids u64 ne peut pas déborder.
        let total: u128 = remaining.iter().map(|v| v.weight() as u128).sum();
        let picked = if total == 0 {
            0usize
        } else {
            let seed = compute_vrf_seed(beacon, slot, round as u32);
            let target = (seed as u128) % total;
            let mut cumulative: u128 = 0;
            let mut idx = remaining.len() - 1; // borne sûre, jamais atteinte
            for (i, v) in remaining.iter().enumerate() {
                cumulative += v.weight() as u128;
                if target < cumulative {
                    idx = i;
                    break;
                }
            }
            idx
        };
        // `picked` est toujours un indice valide (`remaining` est non vide tant
        // que `round < depth <= remaining.len()`), donc pas de panique possible.
        if picked >= remaining.len() {
            break;
        }
        ranking.push(remaining.remove(picked).pk.clone());
    }

    ranking
}

/// Rang d'élection d'un proposeur pour une hauteur, ou `None` s'il n'est pas
/// classé (non bondé, sous le minimum, ou au-delà de [`ELECTION_RANK_DEPTH`]).
///
/// C'est l'unique entrée du fork-choice : plus le rang est **petit**, mieux le
/// proposeur était élu pour cette hauteur, donc plus son bloc est légitime.
pub fn election_rank_of(
    beacon: &str,
    slot: u64,
    validators: &[Validator],
    candidate: &str,
) -> Option<u32> {
    election_ranking(beacon, slot, validators)
        .iter()
        .position(|pk| pk == candidate)
        .and_then(|p| u32::try_from(p).ok())
}

/// Deterministic VRF seed from the election beacon + slot + round.
/// Domain-separated BLAKE3; identical on every node for the same inputs.
fn compute_vrf_seed(beacon: &str, slot: u64, round: u32) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(LEADER_VRF_DOMAIN);
    h.update(b"|seed|");
    h.update(beacon.as_bytes());
    h.update(&slot.to_le_bytes());
    h.update(&round.to_le_bytes());
    let bytes = h.finalize();
    let b = bytes.as_bytes();
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── C-04 (AUDIT-2026-08-13) — FORK-RANK-1 ────────────────────────────────

    /// Le rang 0 du classement DOIT être exactement ce qu'élit `elect_leader`.
    /// Sans cette égalité, le fork-choice préférerait un autre bloc que celui que
    /// le producteur honnête a scellé : produire et vérifier divergeraient, et la
    /// chaîne forkerait à chaque hauteur.
    #[test]
    fn c04_rank0_is_exactly_elect_leader() {
        let vs = vec![
            Validator { pk: "alice".into(), stake: 5_000_000, reputation: 0 },
            Validator { pk: "bob".into(), stake: 2_000_000, reputation: 0 },
            Validator { pk: "carol".into(), stake: 9_000_000, reputation: 0 },
        ];
        for slot in 0..200u64 {
            let beacon = leader_beacon("deadbeef", slot);
            let ranking = election_ranking(&beacon, slot, &vs);
            assert_eq!(
                ranking.first().cloned(),
                elect_leader(&beacon, slot, &vs),
                "slot {slot}: le rang 0 doit être le leader élu"
            );
        }
    }

    /// Le classement est un ordre **total** : chaque éligible apparaît une fois
    /// et une seule. Un doublon rendrait deux blocs ex æquo et ferait retomber le
    /// départage sur le hash, ce que le correctif est censé fermer.
    #[test]
    fn c04_election_ranking_is_a_permutation_of_the_eligible_set() {
        let vs = vec![
            Validator { pk: "alice".into(), stake: 5_000_000, reputation: 0 },
            Validator { pk: "bob".into(), stake: 2_000_000, reputation: 0 },
            Validator { pk: "carol".into(), stake: 9_000_000, reputation: 0 },
            Validator { pk: "dave".into(), stake: 999_999, reputation: 0 }, // sous le minimum
        ];
        for slot in 0..100u64 {
            let beacon = leader_beacon("cafe", slot);
            let mut r = election_ranking(&beacon, slot, &vs);
            assert_eq!(r.len(), 3, "seuls les 3 bondés sont classés");
            assert!(!r.contains(&"dave".to_string()), "un sous-minimum n'est jamais classé");
            r.sort();
            assert_eq!(r, vec!["alice", "bob", "carol"], "aucun doublon, aucun oubli");
        }
    }

    /// Deux nœuds honnêtes construisent leur ensemble depuis une `HashMap`, dont
    /// l'ordre d'itération diffère d'un processus à l'autre. Si le classement en
    /// dépendait, ils préféreraient des blocs différents : fork immédiat.
    #[test]
    fn c04_election_ranking_is_permutation_invariant() {
        let vs = vec![
            Validator { pk: "alice".into(), stake: 5_000_000, reputation: 0 },
            Validator { pk: "bob".into(), stake: 2_000_000, reputation: 0 },
            Validator { pk: "carol".into(), stake: 9_000_000, reputation: 0 },
        ];
        let mut shuffled = vs.clone();
        shuffled.reverse();
        shuffled.swap(0, 1);
        for slot in 0..50u64 {
            let beacon = leader_beacon("f00d", slot);
            assert_eq!(
                election_ranking(&beacon, slot, &vs),
                election_ranking(&beacon, slot, &shuffled),
                "slot {slot}: le classement ne doit pas dépendre de l'ordre d'entrée"
            );
        }
    }

    /// **Le cœur du correctif C-04** : le rang est pondéré par l'enjeu. Un
    /// validateur qui pèse 10× plus doit occuper le rang 0 nettement plus souvent.
    /// C'est ce qui rend le broyage de hash inutile : gagner un départage se paie
    /// en enjeu, pas en cycles CPU.
    #[test]
    fn c04_rank_is_weighted_by_stake_not_by_luck() {
        let vs = vec![
            Validator { pk: "whale".into(), stake: 50_000_000, reputation: 0 },
            Validator { pk: "minnow".into(), stake: 5_000_000, reputation: 0 },
        ];
        let mut whale_first = 0;
        for slot in 0..1000u64 {
            let beacon = leader_beacon("beac0n", slot);
            if election_ranking(&beacon, slot, &vs).first().map(String::as_str) == Some("whale") {
                whale_first += 1;
            }
        }
        // Attendu ≈ 909/1000 (10/11). On laisse une marge très large : le test
        // vérifie la pondération, pas la qualité statistique de BLAKE3.
        assert!(
            (820..=980).contains(&whale_first),
            "le gros enjeu doit dominer le rang 0 (obtenu {whale_first}/1000)"
        );
    }

    /// Une adresse sans enjeu n'a **aucun** rang. C'est la propriété qui prive
    /// l'attaquant de C-04 de tout moyen de gagner un départage hors slot ouvert.
    #[test]
    fn c04_an_unbonded_address_is_never_ranked() {
        let vs = vec![Validator { pk: "alice".into(), stake: 5_000_000, reputation: 0 }];
        let beacon = leader_beacon("00", 7);
        assert_eq!(election_rank_of(&beacon, 7, &vs, "attacker"), None);
        assert_eq!(election_rank_of(&beacon, 7, &vs, "alice"), Some(0));
    }

    /// Le classement est borné : au-delà de `ELECTION_RANK_DEPTH`, personne n'est
    /// classé. Sans cette borne, le coût du fork-choice croîtrait en O(N²) avec la
    /// taille de l'ensemble bondé — un vecteur de déni de service pour le prix
    /// d'un enjeu minimal répliqué.
    #[test]
    fn c04_ranking_depth_is_bounded() {
        let vs: Vec<Validator> = (0..200)
            .map(|i| Validator { pk: format!("v{i:03}"), stake: MIN_VALIDATOR_STAKE, reputation: 0 })
            .collect();
        let beacon = leader_beacon("bounded", 1);
        let r = election_ranking(&beacon, 1, &vs);
        assert_eq!(r.len(), ELECTION_RANK_DEPTH as usize);
        let unranked = vs.iter().filter(|v| !r.contains(&v.pk)).count();
        assert_eq!(unranked, 200 - ELECTION_RANK_DEPTH as usize);
    }

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

    #[test]
    fn weight_is_stake_only_reputation_has_no_effect() {
        // ADR-002: consensus weight = on-chain stake ONLY. Reputation — however
        // large — has zero effect on weight (supersedes audit 3.4's capped bonus).
        // A reputation "farmer" with a small stake gets weight == stake, NOT 2×.
        let farmer = Validator { pk: "rep_farmer".into(), stake: 1_000_000, reputation: 10_000 };
        assert_eq!(farmer.weight(), 1_000_000, "weight must equal stake, ignoring reputation");
        // Same stake, wildly different reputation ⇒ identical weight: the property
        // that makes the weight a pure function of on-chain stake.
        let a = Validator { pk: "a".into(), stake: 5_000_000, reputation: 0 };
        let b = Validator { pk: "b".into(), stake: 5_000_000, reputation: 100 };
        assert_eq!(a.weight(), b.weight(), "reputation cannot change weight");
        assert_eq!(a.weight(), 5_000_000);
    }

    // ─── Track C: aléa d'élection non-grindable (beacon d'entropie enterrée) ──

    #[test]
    fn leader_beacon_deterministic_and_bound() {
        assert_eq!(leader_beacon("h", 1), leader_beacon("h", 1), "déterministe");
        assert_ne!(leader_beacon("h", 1), leader_beacon("h", 2), "lié au slot");
        assert_ne!(leader_beacon("h1", 1), leader_beacon("h2", 1), "lié à l'entropie");
    }

    #[test]
    fn election_uses_buried_beacon_not_tip() {
        // L'entropie d'élection provient du beacon (bloc enterré), jamais du tip
        // fraîchement scellé : le proposeur immédiat ne peut pas se ré-élire en
        // bricolant le contenu de son propre bloc.
        let vals = test_validators();
        let buried = "buried_hash_deadbeef";
        let slot = 12;
        let leader = elect_leader(&leader_beacon(buried, slot), slot, &vals);
        assert!(leader.is_some());
        // Même entropie enterrée ⇒ même leader (le tip non impliqué n'y change rien).
        assert_eq!(leader, elect_leader(&leader_beacon(buried, slot), slot, &vals));
    }

    #[test]
    fn buried_entropy_diversifies_leaders() {
        // Des blocs enterrés différents produisent une distribution de leaders :
        // l'entropie influe réellement sur l'élection.
        let vals = test_validators();
        let leaders: std::collections::HashSet<_> = (0..40u64)
            .map(|s| elect_leader(&leader_beacon(&format!("blk{}", s % 7), s), s, &vals).unwrap())
            .collect();
        assert!(leaders.len() > 1, "l'entropie enterrée doit diversifier les leaders");
    }

    // ─── Constitution Phase 0: déterminisme & arithmétique de l'élection ──────

    /// Deterministically permute `validators` from `perm_seed` (no external RNG,
    /// so the property test is itself fully reproducible).
    fn permute(validators: &[Validator], perm_seed: u64) -> Vec<Validator> {
        let mut v = validators.to_vec();
        v.sort_by_key(|val| {
            let mut h = blake3::Hasher::new();
            h.update(&perm_seed.to_le_bytes());
            h.update(val.pk.as_bytes());
            let d = h.finalize();
            let b = d.as_bytes();
            u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
        });
        v
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(512))]

        /// CORE DETERMINISM INVARIANT (Constitution §3): the elected leader must
        /// be a pure function of the validator *set*, never its iteration order.
        /// Two honest nodes that disagree on leader fork the chain — so we assert
        /// that arbitrarily permuting the input never changes the result, across
        /// many validator sets, slots, and beacons.
        #[test]
        fn elect_leader_is_permutation_invariant(
            stakes in proptest::collection::vec(MIN_VALIDATOR_STAKE..50_000_000u64, 1..8usize),
            reps in proptest::collection::vec(0..100u64, 1..8usize),
            slot in 0u64..100_000,
            perm_seed in proptest::prelude::any::<u64>(),
        ) {
            let validators: Vec<Validator> = stakes.iter().enumerate().map(|(i, &s)| Validator {
                pk: format!("v{:02}", i),
                stake: s,
                reputation: *reps.get(i).unwrap_or(&0),
            }).collect();

            let beacon = leader_beacon("perm-prop", slot);
            let baseline = elect_leader(&beacon, slot, &validators);
            proptest::prop_assert!(baseline.is_some(), "all stakes >= MIN ⇒ a leader exists");

            let shuffled = permute(&validators, perm_seed);
            let after = elect_leader(&beacon, slot, &shuffled);
            proptest::prop_assert_eq!(baseline, after, "leader must be permutation-invariant");
        }
    }

    #[test]
    fn elect_leader_handles_extreme_weights_without_overflow() {
        // Adversarial arithmetic case: two whales whose weights together exceed
        // u64::MAX. The previous `eligible.iter().map(weight).sum::<u64>()` would
        // panic in debug / wrap in release; the u128 accumulation must elect one
        // of them without panicking (Constitution §3: no silent wrap/overflow).
        // Weight is now stake-only (ADR-002), so the stakes themselves must sum
        // past u64::MAX: (u64::MAX/2 + 1_000_000) × 2 = u64::MAX + 2_000_000.
        let huge = u64::MAX / 2 + 1_000_000;
        let vals = vec![
            Validator { pk: "whale_a".into(), stake: huge, reputation: 0 },
            Validator { pk: "whale_b".into(), stake: huge, reputation: 0 },
        ];
        let leader = elect_leader("extreme", 7, &vals);
        assert!(
            matches!(leader.as_deref(), Some("whale_a") | Some("whale_b")),
            "extreme-weight election must elect an eligible whale, got {leader:?}"
        );
        // Fallback path must survive the same extreme weights.
        let fb = elect_fallback_leader("extreme", 7, 1, &vals);
        assert!(matches!(fb.as_deref(), Some("whale_a") | Some("whale_b")));
    }

    /// **STAKE-WEIGHT-1 / ADR-002 — the anti-divergence property (with teeth).**
    ///
    /// The fork hole ADR-002 closes: reputation is measured **locally**, so if it
    /// weighed the election, two honest nodes with the same on-chain stakes but
    /// *different local reputation views* could elect **different** leaders and
    /// fork. Here two nodes hold the SAME validator stakes but DIVERGENT local
    /// reputations; we assert (a) per-validator weight is identical, and (b) the
    /// elected leader agrees across both views, for every slot in a long sweep.
    ///
    /// **Teeth (non-vacuity):** the reputations genuinely differ, and the OLD
    /// formula `stake + min(rep×10_000, stake)` *would* have produced different
    /// weights — we assert that inline, so the test proves the new election agrees
    /// **despite** an input that previously caused divergence, not because the
    /// inputs happen to match.
    #[test]
    fn weight_and_election_are_identical_across_nodes_despite_local_reputation() {
        // Same on-chain stakes on both nodes; only the LOCAL reputation differs.
        let stakes = [("alice", 5_000_000u64), ("bob", 3_000_000), ("carol", 4_000_000)];
        let node_a_reps = [90u64, 10, 50]; // node A's local view
        let node_b_reps = [10u64, 99, 0]; //  node B's local view (divergent)

        let view = |reps: &[u64; 3]| -> Vec<Validator> {
            stakes
                .iter()
                .zip(reps.iter())
                .map(|((pk, stake), rep)| Validator {
                    pk: (*pk).into(),
                    stake: *stake,
                    reputation: *rep,
                })
                .collect()
        };
        let a = view(&node_a_reps);
        let b = view(&node_b_reps);

        // The reputations really differ, and the OLD rep-weighted formula WOULD
        // have diverged — so the agreement below is meaningful, not vacuous.
        let old_weight = |v: &Validator| v.stake.saturating_add(v.reputation.saturating_mul(10_000).min(v.stake));
        let mut old_would_diverge = false;
        for (va, vb) in a.iter().zip(b.iter()) {
            assert_eq!(va.weight(), vb.weight(), "stake-only weight must match across views");
            if old_weight(va) != old_weight(vb) {
                old_would_diverge = true;
            }
        }
        assert!(
            old_would_diverge,
            "test must exercise a reputation delta the OLD formula reacted to (else it proves nothing)"
        );

        // The real property: same leader on both nodes, every slot.
        for slot in 0..2_000u64 {
            let beacon = leader_beacon("anti-fork", slot);
            assert_eq!(
                elect_leader(&beacon, slot, &a),
                elect_leader(&beacon, slot, &b),
                "nodes with the same stakes but different local reputation must elect the SAME leader (slot {slot})"
            );
        }
    }
}
