// p2p/ledger/validation.rs — signature/tx/block validation & coverage (COVER-1/2)
//
// Split out of ledger.rs (organizational only — pure move, no logic change).
// Houses: verify_tx / verify_multisig / verify_chain, remote-block validation,
// the as-of-parent state reconstructions (onchain_spendable_before /
// staked_before / pq_bindings_before), binding_violations, the shared
// validate_block_against_prev + uncovered_tx_indices coverage rule, and
// is_synthetic_sender. All comments/audit refs travel with their code.

use super::*;

impl Ledger {

    // ── Phase 3.1: Signature Verification (toujours hybride) ─────

    /// Verify the signature(s) on a user-signed transaction.
    ///
    /// AUDIT-TX-1: Only the synthetic system addresses are exempt from
    /// signature verification — `NETWORK` (mining tx) and `ESCROW` (state
    /// machine transitions). Any tx whose `from` is a real wallet pubkey
    /// MUST carry a valid signature, even if `to == "BURN"`. Previously a
    /// bug allowed any peer to forge `from=victim, to=BURN` txs over gossip
    /// because `to == "BURN"` short-circuited the check.
    /// PQ-MIG-3B §1 — **stateless** post-quantum gate of a transaction's authority
    /// (ADR-007 b, accounts fully ML-DSA). Returns `Ok(true)` only if, over the
    /// key-binding pre-image:
    /// - `tx.from` IS the ML-DSA **address** of the revealed `pq_public_key`, i.e.
    ///   `lie(from, key)` holds (`from == BLAKE3(ADDR_DOMAIN ‖ pk)`, PQ-MIG-2), and
    /// - a valid **ML-DSA-65** signature from that revealed key (post-quantum
    ///   authority — **mandatory**, no Ed25519 fallback).
    ///
    /// This closes CRYPTO-ID-1 **intrinsically and statelessly**: the account
    /// identifier (`from`) cryptographically commits to the key, so an attacker
    /// cannot attach its own ML-DSA key to a foreign account (a different key ⇒ a
    /// different `from`), and breaking Ed25519 grants no authority (the address no
    /// longer derives from an Ed25519 key). The model-(a) Ed25519 co-factor is
    /// removed from this path; the on-chain binding registry
    /// ([`Self::binding_violations`]) is retained at validation/seal as a redundant
    /// backstop but is now subsumed by this intrinsic check. A tx without an ML-DSA
    /// layer, or whose key does not hash to `from`, is rejected here.
    pub fn verify_tx(tx: &Transaction) -> Result<bool, String> {
        // Synthetic system addresses are exempt — they originate inside the
        // node and are never accepted from gossip.
        if tx.from == "NETWORK" || tx.from == "ESCROW" {
            return Ok(true);
        }
        // LIVE-3: a Slash is NOT signed by the offender — its authority is the
        // embedded fault proof, re-verified against the on-chain stake by
        // `verify_block_slashes` (a stateful check `verify_tx` can't do). It is
        // block-only (never accepted from a `BroadcastTx`, see `handle_broadcast_tx`),
        // so exempting it from the signature gate here opens no gossip-injection hole.
        if tx.tx_type == TxType::Slash {
            return Ok(true);
        }
        // MINT-GUARD-1: `Mining` is a SYSTEM tx type — legitimately issued ONLY by the
        // synthetic `NETWORK` sender (exempted above). A tx reaching this point has a
        // real user `from`, so a user-authorized `Mining` tx (single-key OR multisig)
        // is a forgery: left unchecked it would be swept into the block reward by
        // `coalesce_block_rewards`, minting unbacked QUANTA. Reject it at THE gate
        // (verify_tx is the single portal before admission AND in block validation).
        if tx.tx_type == TxType::Mining {
            return Ok(false);
        }
        // MSIG-1: a multisig tx is flagged by `pq_public_key == MSIG_TAG`. Its
        // authority is a QUORUM of independent ML-DSA signatures, not a single one,
        // so it takes a distinct path *before* the single-key requirements below
        // (it carries no Ed25519 `signature` and no single `pq_signature`).
        if tx.pq_public_key.as_deref() == Some(MSIG_TAG) {
            return Self::verify_multisig(tx);
        }
        // Any other from value must carry a valid signature, regardless of
        // destination (BURN included).
        if tx.signature.is_empty() {
            return Err("Transaction non signée".into());
        }

        // PQ-MIG-3 §3: the ML-DSA authority layer is MANDATORY — a tx without a
        // non-empty `pq_public_key` + `pq_signature` is rejected (the Ed25519-only
        // fallback is gone). Empty here ⇒ an Ed25519-only tx ⇒ rejected.
        let pq_pk_hex = tx.pq_public_key.as_deref().unwrap_or("");
        let pq_sig_hex = tx.pq_signature.as_deref().unwrap_or("");
        if pq_pk_hex.is_empty() || pq_sig_hex.is_empty() {
            return Ok(false);
        }

        // PQ-MIG-3B §1 — INTRINSIC key↔account binding (the closure of CRYPTO-ID-1).
        // `tx.from` IS the ML-DSA address `BLAKE3(ADDR_DOMAIN ‖ pk)` (PQ-MIG-2), so
        // the revealed key MUST hash to it (`lie(from, key)`). A tx revealing any
        // key other than the one whose address is `from` is REJECTED here — an
        // attacker can no longer attach its OWN ML-DSA key to someone else's
        // account, because the account identifier itself cryptographically commits
        // to the key. This supersedes the model-(a) Ed25519 co-factor + on-chain
        // binding registry: the binding is now stateless and unforgeable by
        // construction (a different key ⇒ a different `from`).
        if !CryptoEngine::address_hex_binds_key_hex(&tx.from, pq_pk_hex) {
            return Ok(false);
        }

        // TX-AUTH-NONCE-1 §2 + PQ-MIG-3 §2: the pre-image binds `tx.nonce` AND the
        // revealed ML-DSA key — the signature covers them, so a third party can
        // alter neither the nonce nor the revealed key of a signed tx.
        let payload = Self::tx_signing_preimage(
            &tx.id, &tx.from, &tx.to, tx.amount, &tx.timestamp, &tx.tx_type, tx.nonce, pq_pk_hex,
        );

        // TX-AUTH-NONCE-1 §3: recompute the content hash and REJECT a wire hash
        // that disagrees — no hash malleability (dedup `seen_tx_hashes` is keyed
        // on `tx.hash`; a forged hash must not slip past as "unseen").
        let recomputed = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        if tx.hash != recomputed {
            return Ok(false);
        }

        // PQ-MIG-3B §1 — authority is PURE ML-DSA: a valid ML-DSA-65 signature from
        // the revealed (address-bound) key over the pre-image, via the project's
        // single ML-DSA verifier ([`CryptoEngine::verify_pq`]). The Ed25519
        // co-factor is GONE from the authority path (ADR-007 b, "sans astérisque"):
        // breaking Ed25519 grants no power over an account, since `from` no longer
        // derives from an Ed25519 key. (`tx.signature` may still carry a vestigial
        // Ed25519 signature for wire-compat; it is NOT consulted for authority.)
        let pq_sig = hex::decode(pq_sig_hex).map_err(|_| "PQ signature invalide")?;
        Ok(CryptoEngine::verify_pq(pq_pk_hex, payload.as_bytes(), &pq_sig))
    }

    /// MSIG-1 — verify an M-of-N multisig authority. `Ok(true)` iff ALL hold:
    /// 1. `from` == the address derived from the revealed policy `{keys, threshold}`
    ///    ([`crate::security::multisig_address_hex`]) — the rebind-proof binding: a
    ///    different key set or threshold yields a different `from`, so an attacker
    ///    cannot substitute their own keys onto someone else's multisig account.
    /// 2. The wire `hash` equals the recomputed pre-image hash (no malleability).
    /// 3. At least `threshold` **distinct** registered keys each carry a valid
    ///    ML-DSA-65 signature over that pre-image (duplicate signatures from one key
    ///    cannot inflate the count — verification is per-distinct-key).
    ///
    /// Pure (no ledger state) and deterministic — same inputs ⇒ same verdict on
    /// every node (C1), like the single-key `verify_tx`.
    fn verify_multisig(tx: &Transaction) -> Result<bool, String> {
        let auth: MultisigAuth = match tx.pq_signature.as_deref() {
            Some(json) => match serde_json::from_str(json) {
                Ok(a) => a,
                Err(_) => return Ok(false),
            },
            None => return Ok(false),
        };
        // Canonicalize exactly as the address derivation does (sorted, de-duplicated)
        // so counting is per-DISTINCT-key and independent of order/duplicates.
        // MSIG-SEC-1: canonicalize by decoded key BYTES (not raw hex strings), so a
        // single key spelled in two hex cases cannot fill two quorum slots; also
        // rejects any malformed / wrong-length key. This is the SAME canonicalization
        // the address derivation uses, so counting and binding can never disagree.
        let keys = match crate::security::canonicalize_msig_keys(&auth.pubkeys) {
            Some(k) => k,
            None => return Ok(false),
        };
        if auth.threshold == 0 || auth.threshold as usize > keys.len() {
            return Ok(false);
        }
        // (1) Binding — `from` must be the address of exactly this (canonical) policy.
        match crate::security::multisig_address_hex(&keys, auth.threshold) {
            Some(addr) if addr == tx.from => {}
            _ => return Ok(false),
        }
        // (2) Pre-image + hash integrity. The pq_pk slot signed is the MSIG tag.
        let payload = Self::tx_signing_preimage(
            &tx.id, &tx.from, &tx.to, tx.amount, &tx.timestamp, &tx.tx_type, tx.nonce, MSIG_TAG,
        );
        if tx.hash != hex::encode(blake3::hash(payload.as_bytes()).as_bytes()) {
            return Ok(false);
        }
        // (3) Count distinct registered keys carrying ≥1 valid signature.
        let mut sigs: Vec<Vec<u8>> = Vec::with_capacity(auth.signatures.len());
        for s in &auth.signatures {
            match hex::decode(s) {
                Ok(b) => sigs.push(b),
                Err(_) => return Ok(false),
            }
        }
        let valid_signers = keys
            .iter()
            .filter(|pk| {
                sigs.iter()
                    .any(|sig| CryptoEngine::verify_pq(pk, payload.as_bytes(), sig))
            })
            .count();
        Ok(valid_signers as u32 >= auth.threshold)
    }

    /// Verify the integrity of the entire chain:
    /// - Each block's prev_hash links to the previous block's hash
    /// - All user-signed transactions have valid Ed25519 signatures
    pub fn verify_chain(&self) -> Result<(u64, u64), String> {
        let mut verified_blocks = 0u64;
        let mut verified_txs = 0u64;
        for (i, block) in self.chain.iter().enumerate() {
            // Genesis block (index 0) has no previous block to link
            if i > 0 {
                let prev = &self.chain[i - 1];
                if block.prev_hash != prev.hash {
                    return Err(format!(
                        "Chaîne corrompue au bloc {}: prev_hash mismatch",
                        block.index
                    ));
                }
            }
            // Verify all signed transactions in this block
            for tx in &block.transactions {
                if !Self::verify_tx(tx)? {
                    return Err(format!(
                        "Signature invalide: tx {} dans bloc {}",
                        tx.id, block.index
                    ));
                }
                verified_txs += 1;
            }
            verified_blocks += 1;
        }
        Ok((verified_blocks, verified_txs))
    }

    // ─── D1: Remote Block Validation & Integration ──────────────────────

    /// D1.2: Validate a block received from a remote peer (against current
    /// tip).
    ///
    /// Checks:
    /// 1. Block index is sequential (matches our chain tip + 1)
    /// 2. prev_hash links to our chain tip
    /// 3. All transaction signatures are valid
    /// 4. Block hash is correctly computed
    ///    (index:prev_hash:ts:tx_count:merkle_root)
    ///
    /// Returns Ok(()) if valid, Err(reason) if not.
    pub fn validate_remote_block(&self, block: &Block) -> Result<(), String> {
        let tip = self.chain.last().ok_or("empty chain")?;
        if block.index != tip.index + 1 {
            return Err(format!(
                "block index {} does not follow tip {} (expected {})",
                block.index,
                tip.index,
                tip.index + 1
            ));
        }
        if block.prev_hash != tip.hash {
            return Err(format!(
                "prev_hash mismatch: block says {} but tip is {}",
                short(&block.prev_hash, 16),
                short(&tip.hash, 16)
            ));
        }
        // COVER-1: seed the shared validator's coverage check with the on-chain
        // balances up to the tip (this block extends it). Mempool-free → identical
        // verdict on every node.
        let onchain_before = self.onchain_spendable_before(tip);
        let bindings_before = self.pq_bindings_before(tip);
        // PROPOSER-1: `self` sits at the parent (tip) on the linear path (and on
        // the reorg trial clone), so the live `validator_stakes()` IS the bonded
        // set as of the parent — O(1), no replay needed.
        let bonded_before = self.validator_stakes();
        Self::validate_block_against_prev(block, tip, &onchain_before, &bindings_before, &bonded_before)?;
        self.validate_block_emission(block)
    }

    /// TOKENOMICS v2 — garde-fou de consensus : la somme minée d'un bloc ne
    /// peut JAMAIS pousser l'offre au-delà du plafond dur, même venant d'un
    /// pair malveillant. Sans ça, un attaquant scellerait un bloc se
    /// créditant des millions (les tx `NETWORK` sont exemptes de signature)
    /// → inflation arbitraire. Le plafond devient ainsi infalsifiable à
    /// l'échelle réseau.
    fn validate_block_emission(&self, block: &Block) -> Result<(), String> {
        // Linear (happy-path) emission: the block extends the tip, so the
        // supply *before* it is the whole current chain's minted total.
        Self::validate_block_emission_against(block, self.stats().total_mined)
    }

    /// FORK-CAP-1: emission validation parameterised by `prior_mined` — the
    /// minted supply that exists **before** this block — so the happy path and
    /// the fork-reorg path enforce the SAME hard cap + per-block bound from one
    /// source of truth and can never diverge on emission again. The happy path
    /// passes the whole chain's `total_mined`; the fork-reorg path passes
    /// `total_mined` MINUS the tip it replaces (else the popped tip's reward is
    /// double-counted, over-stating supply and falsely rejecting honest reorgs
    /// near the cap).
    pub(crate) fn validate_block_emission_against(
        block: &Block,
        prior_mined: u64,
    ) -> Result<(), String> {
        let block_minted: u64 = block
            .transactions
            .iter()
            .filter(|t| t.tx_type == TxType::Mining)
            .map(|t| t.amount)
            .sum();
        if block_minted == 0 {
            return Ok(());
        }
        let current = prior_mined;
        // ① Plafond dur (total) — l'offre ne peut JAMAIS dépasser MAX_SUPPLY.
        if current.saturating_add(block_minted) > crate::p2p::reputation::MAX_SUPPLY_MICRO {
            return Err(format!(
                "bloc rejeté : émission {} pousserait l'offre {} au-delà du plafond dur {}",
                block_minted,
                current,
                crate::p2p::reputation::MAX_SUPPLY_MICRO
            ));
        }
        // ② MINT-EXACT-1 — borne PAR BLOC **serrée** : la récompense d'un bloc est
        // une fonction pure de la chaîne (`emission_for_block`), donc chaque nœud la
        // re-dérive à l'identique et un sceleur ne peut pas s'en écarter.
        //
        // Avant, la borne valait `64 × emission_for_tick` là où le montant honnête
        // est `TICKS_PER_BLOCK × emission_for_tick / N` (N nœuds vivants) : une marge
        // exploitable de `32 × N` — à 100 nœuds, un validateur bondé pouvait se
        // minter 3 200 fois sa récompense légitime, à chaque bloc, et tout le réseau
        // l'acceptait (seul le plafond dur des 100 M était vérifié). Le montant
        // venait d'un calcul **local** (part Shapley sur des watts auto-déclarés),
        // donc invérifiable par définition ; le réseau ne pouvait que le borner.
        // Désormais il le **recalcule**, et la marge disparaît.
        //
        // `block_minted == 0` est déjà sorti plus haut : un bloc peut ne porter
        // aucune récompense (le sceleur y renonce, ou l'émission a atteint le
        // plafond) — c'est strictement non-inflationnaire, donc autorisé.
        let expected = crate::p2p::reputation::emission_for_block(current);
        if block_minted > expected {
            return Err(format!(
                "bloc rejeté : émission/bloc {} dépasse la récompense canonique {} \
                 (offre minée avant ce bloc : {}) — la récompense est une fonction \
                 pure de la chaîne, pas un montant choisi par le sceleur",
                block_minted, expected, current
            ));
        }
        Ok(())
    }

    /// COVER-1 §2: the **on-chain** spendable balance of every account just
    /// before `prev`'s child block — a pure function of the chain up to and
    /// including `prev`, **never** the pending mempool (a mempool-derived figure
    /// differs node-to-node and would make the coverage verdict diverge). It
    /// mirrors the ledger's own balance semantics exactly, so it can never drift:
    ///
    /// - generic `from → to` moves, identical to [`Self::cache_apply_tx`] (a
    ///   `Stake`'s `pk → STAKE` lock flows through here — `STAKE` is not synthetic
    ///   — so staked coins are correctly debited from `from`);
    /// - `Unstake` has no spendable effect (it reclassifies already-locked sink
    ///   coins), exactly as `cache_apply_tx` skips it;
    /// - height-triggered unbonding **maturation** (`STAKE → pk`), identical to
    ///   [`Self::mature_unbonding`], so coins whose unlock height is `≤ prev.index`
    ///   are counted as spendable again.
    ///
    /// Both validation paths seed [`Self::validate_block_against_prev`]'s coverage
    /// loop with this map, so neither the linear nor the fork-reorg path can admit
    /// an uncovered spend (the FORK-CAP lesson: one check, on the shared
    /// validator). A `#[cfg(test)]` no-drift guard asserts this equals the live
    /// cache for a pending-free chain.
    pub(crate) fn onchain_spendable_before(&self, prev: &Block) -> HashMap<String, i128> {
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        let mut bal: HashMap<String, i128> = HashMap::new();
        // pk → list of (amount, unlock_height) still locked in the unbonding pool.
        // H4: (amount, unlock_height, origin_tx_hash) — the hash lets a Slash
        // consume exactly the entries its carried breakdown names.
        let mut unbonding: HashMap<String, Vec<(u64, u64, String)>> = HashMap::new();
        for block in self.chain.iter().filter(|b| b.index <= prev.index) {
            for tx in &block.transactions {
                if matches!(tx.tx_type, TxType::Unstake) {
                    // Bonded → unbonding: no spendable move; the coins stay in the
                    // STAKE sink until they mature (recorded for the pass below).
                    unbonding.entry(tx.from.clone()).or_default().push((
                        tx.amount,
                        block.index.saturating_add(UNBONDING_PERIOD_BLOCKS),
                        tx.hash.clone(),
                    ));
                    continue;
                }
                // LIVE-3: a Slash destroys locked stake (STAKE sink → burned); it has
                // NO spendable effect, so it must not debit the offender's spendable
                // balance here.
                //
                // H4 (AUDIT-2026-07-25): it used to `continue` outright, which was
                // wrong twice. (1) `cache_apply_tx` debits the STAKE sink by
                // `tx.amount`; skipping left the replay's sink permanently higher
                // than the live one. (2) LIVE-3B slashes DESTROY unbonding entries,
                // and `apply_block_stake_effects` removes them so they never mature —
                // but the replay kept them and credited the offender at
                // `unlock_height` with coins the chain had already burned. Since this
                // map seeds COVER-1/COVER-2 on every admission path, every node
                // agreed those phantom coins were spendable.
                if matches!(tx.tx_type, TxType::Slash) {
                    *bal.entry(Self::STAKE_SINK.to_string()).or_insert(0) -= tx.amount as i128;
                    if let Some(consumed) = &tx.slash_unbonding {
                        if let Some(list) = unbonding.get_mut(&tx.from) {
                            for c in consumed {
                                if let Some(e) = list.iter_mut().find(|e| e.2 == c.tx_hash) {
                                    e.0 = e.0.saturating_sub(c.amount);
                                }
                            }
                            list.retain(|e| e.0 > 0);
                        }
                        unbonding.retain(|_, v| !v.is_empty());
                    }
                    continue;
                }
                if !synthetic(&tx.to) {
                    *bal.entry(tx.to.clone()).or_insert(0) += tx.amount as i128;
                }
                if !synthetic(&tx.from) {
                    *bal.entry(tx.from.clone()).or_insert(0) -= tx.amount as i128;
                }
            }
            // Height-triggered maturation — mirrors `mature_unbonding(block.index)`.
            let height = block.index;
            let mut matured: Vec<(String, u64)> = Vec::new();
            for (pk, entries) in unbonding.iter_mut() {
                entries.retain(|(amount, unlock, _)| {
                    if *unlock <= height {
                        matured.push((pk.clone(), *amount));
                        false
                    } else {
                        true
                    }
                });
            }
            unbonding.retain(|_, v| !v.is_empty());
            for (pk, amount) in matured {
                *bal.entry(Self::STAKE_SINK.to_string()).or_insert(0) -= amount as i128;
                *bal.entry(pk).or_insert(0) += amount as i128;
            }
        }
        bal
    }

    /// PROPOSER-1 (GENESIS-V4) — the **bonded-stake map as of `prev`**, a pure
    /// function of the chain prefix `chain[.. = prev.index]`. Mirrors
    /// [`Self::apply_block_stake_effects`] for the per-account bonded weight
    /// (`staked`) exactly — `Stake` bonds, `Unstake` unbonds, `Slash` destroys the
    /// bonded portion — so `staked_before(tip) == validator_stakes()` (locked by
    /// the `staked_before_matches_live_cache` test). Maturation touches only the
    /// unbonding pool, never bonded weight, so it is irrelevant here.
    ///
    /// Used to verify a block's proposer against the validator set **as of its
    /// parent**, identically on every node and every admission path (linear /
    /// reorg / sync). The linear + trial-clone paths sit at the parent already, so
    /// they use the O(1) live [`Self::validator_stakes`]; this O(chain) replay is
    /// only needed on the rare 1-block fork tie-break, where `self` is one block
    /// ahead of the parent.
    pub(super) fn staked_before(&self, prev: &Block) -> HashMap<String, u64> {
        let mut staked: HashMap<String, u64> = HashMap::new();
        for block in self.chain.iter().filter(|b| b.index <= prev.index) {
            for tx in &block.transactions {
                match tx.tx_type {
                    TxType::Stake => {
                        *staked.entry(tx.from.clone()).or_insert(0) += tx.amount;
                    }
                    TxType::Unstake => {
                        let bonded = staked.entry(tx.from.clone()).or_insert(0);
                        *bonded = bonded.saturating_sub(tx.amount);
                        if *bonded == 0 {
                            staked.remove(&tx.from);
                        }
                    }
                    TxType::Slash => {
                        let consumed_unbonding: u64 =
                            tx.slash_unbonding.iter().flatten().map(|e| e.amount).sum();
                        let bonded_take = tx.amount.saturating_sub(consumed_unbonding);
                        if bonded_take > 0 {
                            let bonded = staked.entry(tx.from.clone()).or_insert(0);
                            *bonded = bonded.saturating_sub(bonded_take);
                        }
                        if staked.get(&tx.from).copied().unwrap_or(0) == 0 {
                            staked.remove(&tx.from);
                        }
                    }
                    _ => {}
                }
            }
        }
        staked
    }

    /// PQ-MIG-3 §1 — the chain-derived ML-DSA **binding registry** as of `prev`:
    /// the first-seen `from → pq_public_key` over every sealed, real-sender, signed
    /// tx up to and including `prev`. A **pure function of the chain** (like
    /// [`Self::onchain_spendable_before`]), so every node computes the identical
    /// map and binding validation is deterministic — no extra persisted state.
    pub(super) fn pq_bindings_before(&self, prev: &Block) -> HashMap<String, String> {
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        let mut bound: HashMap<String, String> = HashMap::new();
        for block in self.chain.iter().filter(|b| b.index <= prev.index) {
            for tx in &block.transactions {
                if synthetic(&tx.from) {
                    continue;
                }
                if let Some(k) = tx.pq_public_key.as_deref() {
                    if !k.is_empty() {
                        bound.entry(tx.from.clone()).or_insert_with(|| k.to_string());
                    }
                }
            }
        }
        bound
    }

    /// PQ-MIG-3 §1 — the **single source of truth** for the ML-DSA key binding,
    /// mirroring `uncovered_tx_indices` (COVER-1/2). Walks `txs` in order over a
    /// running binding map seeded from `bindings_before`; returns the indices of
    /// txs that **violate** the binding — a real-sender signed tx whose
    /// `pq_public_key` differs from the key already bound to its `from`. The FIRST
    /// signed tx from an as-yet-unbound `from` **establishes** the binding (immutable
    /// thereafter) and is not a violation. Synthetic/unsigned txs bind nothing.
    ///
    /// This is the **stateful** half of closing CRYPTO-ID-1: once an account's
    /// independent ML-DSA key is bound, no later tx may swap in a different key — so
    /// a future Ed25519 break can neither re-bind an attacker's key nor produce the
    /// bound (independent) key's signature. Used by BOTH validation (reject) and
    /// seal (exclude), so the two can never disagree (the COVER-2 lesson).
    pub(super) fn binding_violations(
        bindings_before: &HashMap<String, String>,
        txs: &[Transaction],
    ) -> Vec<usize> {
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        let mut bound = bindings_before.clone();
        let mut out = Vec::new();
        for (i, tx) in txs.iter().enumerate() {
            if synthetic(&tx.from) {
                continue;
            }
            // A real sender with no ML-DSA key is already rejected by `verify_tx`;
            // here we only police key *consistency* for keyed txs.
            let key = match tx.pq_public_key.as_deref() {
                Some(k) if !k.is_empty() => k,
                _ => continue,
            };
            match bound.get(&tx.from) {
                Some(existing) if existing != key => out.push(i),
                Some(_) => {}
                None => {
                    bound.insert(tx.from.clone(), key.to_string());
                }
            }
        }
        out
    }

    /// AUDIT-BLK-2: Stateless block validation against an explicit `prev`.
    /// Used by `integrate_remote_block` during fork reorg, where the relevant
    /// `prev` is the block at `tip.index - 1` rather than the current tip.
    /// Doesn't enforce index continuity (caller checks) but recomputes the
    /// block hash and verifies every transaction signature.
    ///
    /// COVER-1: also enforces **sequential coverage** against `onchain_before`
    /// (the on-chain spendable balances up to `prev`, from
    /// [`Self::onchain_spendable_before`]). Because BOTH integration paths route
    /// through here, no node can finalize a spend or stake of coins that don't
    /// exist — on either path (FORK-CAP lesson).
    pub(super) fn validate_block_against_prev(
        block: &Block,
        prev: &Block,
        onchain_before: &HashMap<String, i128>,
        bindings_before: &HashMap<String, String>,
        bonded_before: &HashMap<String, u64>,
    ) -> Result<(), String> {
        if block.prev_hash != prev.hash {
            return Err(format!(
                "prev_hash mismatch: block says {} but prev is {}",
                short(&block.prev_hash, 16),
                short(&prev.hash, 16)
            ));
        }
        // PROPOSER-1 (GENESIS-V4) — deterministic proposer check. The reported
        // CRITICAL was that the PoS election was checked only on the SEAL side
        // (`mining_loop`), never on receive: a modified node could seal any slot
        // with any address — staked or not — and the network accepted it. Here we
        // reject, on every admission path (this is the SHARED validator: linear
        // integration, 1-block fork tie-break, deep reorg trial clone, and sync),
        // any non-genesis block whose proposer is not a **bonded validator as of
        // the parent** (`bonded_before`, stake ≥ MIN_VALIDATOR_STAKE).
        //
        // Why bonded-membership and not "is the elected leader/fallback": without a
        // trusted clock we cannot time-gate the fallback tiers, so the deterministic
        // rule is the time-ungated UNION of {primary ∪ fallbacks ∪ any-eligible} =
        // "any bonded validator". That is a SUPERSET of every proposer
        // `is_valid_proposer` (the seal side) can ever return, so an honestly-sealed
        // block always passes (seal/receive never disagree), it is clock-free (no
        // drift forks — C1), and it fully closes the hole: an unstaked/arbitrary
        // address is rejected. Out-of-turn proposing by a *bonded* validator is
        // contained by fork-choice (LMD-GHOST + lexicographic tie-break) and, for
        // equivocation, by the finality slashing gadget.
        //
        // Bootstrap: before anyone has staked, `bonded_before` has no eligible
        // entry → sealing is permissionless (mirrors `mining_loop`'s bootstrap
        // branch), so the fresh v4 chain can start from an empty genesis.
        //
        // OPEN-DOOR-1 : un bloc sur `OPEN_SLOT_EVERY_BLOCKS` est un **slot ouvert**
        // — n'importe quelle adresse peut le proposer, bondée ou non. Sans lui, la
        // règle ci-dessus refermait le réseau **définitivement** au premier staker :
        // un nouvel arrivant a besoin d'un enjeu pour proposer, de proposer pour
        // gagner, et il n'existe ni faucet ni airdrop pour rompre la boucle. La
        // fenêtre est cadencée par la hauteur (fonction pure, O(1)), donc une ferme
        // Sybil ne capte au plus qu'`1/OPEN_SLOT_EVERY_BLOCKS` de l'émission quel
        // que soit son nombre d'identités — le prix, borné et assumé, de l'entrée
        // libre (trilemme Sybil : sans-permission + résistant + gratuit est
        // impossible ; on relâche « gratuit » d'exactement un seizième).
        if block.index > 0 && !crate::p2p::pos_consensus::is_open_slot(block.index) {
            let has_eligible = bonded_before
                .values()
                .any(|&s| s >= crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE);
            if has_eligible {
                let proposer_stake =
                    bonded_before.get(&block.miner).copied().unwrap_or(0);
                if proposer_stake < crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE {
                    return Err(format!(
                        "bloc rejeté : proposeur {} non bondé — seul un validateur avec enjeu ≥ {} µQTA peut proposer (PROPOSER-1)",
                        short(&block.miner, 12),
                        crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE
                    ));
                }
            }
        }
        // EMIT-1 §4.2 (Option A — one reward per block): at most ONE `Mining`
        // tx, and if present it must be the coinbase `NETWORK → block.miner`.
        // Stateless, so it guards BOTH the happy path and the fork-reorg path —
        // a malicious node forging two mining txs (whose sum could slip under
        // the per-block emission bound, which only checks the total) or
        // crediting someone other than the sealer is rejected here. Belt-and-
        // suspenders with BLK-HASH-1, which already binds `miner` into the hash.
        let mining: Vec<&Transaction> = block
            .transactions
            .iter()
            .filter(|t| t.tx_type == TxType::Mining)
            .collect();
        if mining.len() > 1 {
            return Err(format!(
                "bloc rejeté : {} récompenses de minage — au plus une par bloc (EMIT-1)",
                mining.len()
            ));
        }
        if let Some(reward) = mining.first() {
            if reward.from != "NETWORK" {
                return Err(format!(
                    "bloc rejeté : récompense de minage émise par {} (NETWORK attendu)",
                    reward.from
                ));
            }
            if reward.to != block.miner {
                return Err(
                    "bloc rejeté : récompense de minage créditée à un autre que le mineur du bloc"
                        .into(),
                );
            }
        }
        // C2 (AUDIT-2026-07-25): a synthetic sender skips both signature AND
        // coverage, so it must be confined to the single legitimate coinbase —
        // otherwise a `Transfer` from "NETWORK" mints without limit, invisible to
        // the emission bound (which only sums `Mining`). Checked BEFORE
        // `verify_tx`, which is precisely the guard that waves synthetics through.
        let illegal_synth = Self::illegal_synthetic_indices(&block.transactions, &block.miner);
        if let Some(&i) = illegal_synth.first() {
            let tx = &block.transactions[i];
            return Err(format!(
                "bloc rejeté : tx {} — expéditeur synthétique {} hors de la coinbase (C2)",
                i,
                short(&tx.from, 12)
            ));
        }
        // C3 (AUDIT-2026-07-25): an Unstake is exempt from spendable coverage, so
        // without this rule nothing anywhere checked it against the bonded stake —
        // and the fabricated unbonding entry matured into real spendable coins.
        let overdrawn = Self::overdrawn_unstake_indices(bonded_before, &block.transactions);
        if let Some(&i) = overdrawn.first() {
            let tx = &block.transactions[i];
            return Err(format!(
                "bloc rejeté : tx {} — retrait de {} µQTA supérieur à l'enjeu bondé de {} (C3)",
                i,
                tx.amount,
                short(&tx.from, 12)
            ));
        }
        for tx in &block.transactions {
            if !Self::verify_tx(tx)? {
                return Err(format!(
                    "invalid tx signature: {}",
                    short(&tx.id, 16)
                ));
            }
        }
        // PQ-MIG-3 §1/§4: enforce the ML-DSA key binding (the stateful half of
        // closing CRYPTO-ID-1). A block carrying a real-sender tx whose ML-DSA key
        // ≠ the key already bound to its account is INVALID and rejected whole —
        // this is the "clé non liée rejetée" guarantee (an attacker cannot attach
        // its own key to someone else's account). Same single rule the seal path
        // uses to EXCLUDE (COVER-2 symmetry), so reject/exclude never disagree.
        let binding_viol = Self::binding_violations(bindings_before, &block.transactions);
        if let Some(&i) = binding_viol.first() {
            let tx = &block.transactions[i];
            return Err(format!(
                "bloc rejeté : clé ML-DSA non liée — {} présente une clé ≠ celle liée à son compte (CRYPTO-ID-1)",
                short(&tx.from, 12)
            ));
        }
        // BLK-HASH-1: same canonical hash as production via the shared
        // `block_hash_hex` (PQ-MIG-5) — includes `miner` and a content-binding
        // Merkle root.
        let expected_hash = Self::block_hash_hex(
            block.index,
            &block.prev_hash,
            &block.timestamp,
            &block.miner,
            &block.transactions,
        );
        if block.hash != expected_hash {
            return Err(format!(
                "block hash mismatch: declared {} but computed {}",
                short(&block.hash, 16),
                short(&expected_hash, 16)
            ));
        }
        // COVER-1 §2–§3: sequential coverage against the on-chain balance. A block
        // received from a peer with ANY uncovered real-sender tx is INVALID and
        // rejected whole — on whichever integration path called us. The coverage
        // rule itself lives in the shared `uncovered_tx_indices` (COVER-2: same
        // single source of truth the seal path uses to EXCLUDE, so reject and
        // exclude can never disagree).
        let uncovered = Self::uncovered_tx_indices(onchain_before, &block.transactions);
        if let Some(&i) = uncovered.first() {
            let tx = &block.transactions[i];
            return Err(format!(
                "bloc rejeté : dépense non couverte — {} en {} {} µQTA sans couverture (COVER-1)",
                short(&tx.from, 12),
                if matches!(tx.tx_type, TxType::Stake) { "enjeu" } else { "transfère" },
                tx.amount
            ));
        }
        Ok(())
    }

    /// COVER-1/COVER-2 — the **single source of truth** for sequential coverage.
    /// Walks `txs` in declared order over a running balance seeded from
    /// `onchain_before` (the on-chain spendable state up to the parent, from
    /// [`Self::onchain_spendable_before`]); returns the indices of the txs that are
    /// **uncovered** — a REAL-sender `Transfer`/`Stake` whose running balance is
    /// insufficient at that point. An uncovered tx's effect is **not** applied to
    /// the running balance (it will be rejected/excluded, so it must not fund later
    /// txs), while every covered tx, every synthetic-sender tx (`NETWORK`/`ESCROW`
    /// mint; `BURN` destination-only), and every `Unstake` (no spendable debit)
    /// applies its move — so **intra-block credits fund later txs** (§3).
    ///
    /// Two consumers, one rule: validation ([`Self::validate_block_against_prev`])
    /// REJECTS the whole block iff this is non-empty (COVER-1); seal
    /// ([`Self::seal_block_at`]) EXCLUDES exactly these txs to build a
    /// valid-by-construction block (COVER-2). One rule ⇒ a self-sealed block always
    /// passes validation.
    /// **C3 (AUDIT-2026-07-25) — an `Unstake` may not exceed the bonded stake.**
    ///
    /// `uncovered_tx_indices` exempts `Unstake` — correctly, since it moves bonded
    /// stake rather than spendable balance — and *nothing else* checked the amount.
    /// The only bonded-amount guard lived in `unstake_tx_at`, i.e. in the local
    /// builder, which a modified node does not run. A validly-signed `Unstake` from
    /// an account with zero bonded stake therefore propagated, got sealed by an
    /// honest leader (COVER-2 did not exclude it either), and
    /// `apply_block_stake_effects` created an unbonding entry for the full amount.
    /// `UNBONDING_PERIOD_BLOCKS` later it matured into spendable coins that were
    /// never minted, while `locked_stake_total()`'s `.max(0)` clamp hid the
    /// negative STAKE sink and `onchain_spendable_before` replayed the same
    /// permissive logic — so COVER-1 on every node agreed the phantom coins were
    /// real.
    ///
    /// Sequential over a running bonded map seeded from `bonded_before` (the
    /// as-of-parent stake PROPOSER-1 already computes and passes in), so a `Stake`
    /// earlier in the same block counts, exactly like COVER credits intra-block
    /// income. `Slash` shrinks the running bond so a later `Unstake` cannot draw on
    /// stake the same block just burned.
    pub(super) fn overdrawn_unstake_indices(
        bonded_before: &HashMap<String, u64>,
        txs: &[Transaction],
    ) -> Vec<usize> {
        let mut running: HashMap<String, u64> = bonded_before.clone();
        let mut bad = Vec::new();
        for (i, tx) in txs.iter().enumerate() {
            match tx.tx_type {
                TxType::Stake => {
                    let e = running.entry(tx.from.clone()).or_insert(0);
                    *e = e.saturating_add(tx.amount);
                }
                TxType::Unstake => {
                    let have = running.get(&tx.from).copied().unwrap_or(0);
                    if tx.amount > have {
                        bad.push(i);
                        continue; // rejected/excluded → do NOT move the running bond
                    }
                    let e = running.entry(tx.from.clone()).or_insert(0);
                    *e = e.saturating_sub(tx.amount);
                }
                TxType::Slash => {
                    // LIVE-3B: the slash consumes bonded stake first, then unbonding
                    // entries. Only the bonded portion affects what a later Unstake
                    // in this block may draw on; the exact split is re-verified by
                    // `verify_block_slashes`.
                    let consumed_unbonding: u64 =
                        tx.slash_unbonding.iter().flatten().map(|e| e.amount).sum();
                    let bonded_take = tx.amount.saturating_sub(consumed_unbonding);
                    let e = running.entry(tx.from.clone()).or_insert(0);
                    *e = e.saturating_sub(bonded_take);
                }
                _ => {}
            }
        }
        bad
    }

    /// **C2 (AUDIT-2026-07-25) — confine synthetic senders to the one place they
    /// are legitimate.**
    ///
    /// A synthetic sender (`NETWORK`, `ESCROW`) is exempt from signature
    /// verification ([`Self::verify_tx`]) *and* from coverage
    /// ([`Self::uncovered_tx_indices`]) — it is supposed to originate inside the
    /// node. But the emission guards only ever looked at `TxType::Mining`:
    /// `validate_block_emission_against` sums Mining amounts and returns early when
    /// that total is zero, and the EMIT-1 coinbase rule filters on Mining as well.
    /// So a `Transfer` (or `Stake`, or `Burn`) whose `from` is `"NETWORK"` was
    /// unsigned, uncovered **and** invisible to the 100M hard cap: free money that
    /// every honest node accepted, and that `stats().total_mined` never counted, so
    /// the supply views kept reporting the honest figure.
    ///
    /// The rule: `NETWORK` may appear as a sender only as the single `Mining`
    /// coinbase credited to the block's own sealer — the tx EMIT-1 and the emission
    /// bound already police. `ESCROW` may never be a sender in a sealed block
    /// (`escrow_release_to` has no caller outside tests).
    ///
    /// Returned as indices, not a bool, so the same function drives *both* sides:
    /// the validator rejects (C2-receive) and the sealer excludes (C2-produce),
    /// which is the COVER-1/COVER-2 symmetry that keeps produce and receive from
    /// ever disagreeing.
    pub(super) fn illegal_synthetic_indices(txs: &[Transaction], miner: &str) -> Vec<usize> {
        let mut bad = Vec::new();
        for (i, tx) in txs.iter().enumerate() {
            if !Self::is_synthetic_sender(&tx.from) {
                continue;
            }
            let legal_coinbase =
                tx.tx_type == TxType::Mining && tx.from == "NETWORK" && tx.to == miner;
            if !legal_coinbase {
                bad.push(i);
            }
        }
        bad
    }

    pub(super) fn uncovered_tx_indices(
        onchain_before: &HashMap<String, i128>,
        txs: &[Transaction],
    ) -> Vec<usize> {
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        let mut running: HashMap<String, i128> = onchain_before.clone();
        let mut uncovered = Vec::new();
        for (i, tx) in txs.iter().enumerate() {
            if matches!(tx.tx_type, TxType::Unstake) {
                continue; // no spendable debit to cover
            }
            // LIVE-3: a Slash spends LOCKED stake (STAKE sink), not the offender's
            // spendable balance — its legitimacy is checked by the embedded fault
            // proof + bonded-stake amount in `validate_block_against_prev`, not by
            // spendable coverage. Exempt it here (like Unstake) so it neither
            // requires spendable cover nor perturbs the running balance.
            if matches!(tx.tx_type, TxType::Slash) {
                continue;
            }
            if !synthetic(&tx.from) {
                let have = running.get(&tx.from).copied().unwrap_or(0);
                if have < tx.amount as i128 {
                    uncovered.push(i);
                    continue; // excluded/rejected → do NOT move the running balance
                }
            }
            // Mirror `cache_apply_tx` so intra-block credits fund later txs.
            if !synthetic(&tx.to) {
                *running.entry(tx.to.clone()).or_insert(0) += tx.amount as i128;
            }
            if !synthetic(&tx.from) {
                *running.entry(tx.from.clone()).or_insert(0) -= tx.amount as i128;
            }
        }
        uncovered
    }

    /// EMIT-1: a synthetic **sender** — `NETWORK` (mining reward) or `ESCROW`
    /// (state-machine release). Such txs belong to a sealed block, never the
    /// mempool, so they are excluded from the fork-reorg re-queue (§4.1).
    /// `BURN` is a synthetic *destination*, never a sender, so it is not here.
    pub(super) fn is_synthetic_sender(from: &str) -> bool {
        matches!(from, "NETWORK" | "ESCROW")
    }
}
