# Quanta — Audit Scope

Prioritized scope for an external review of the Quanta protocol. Line counts were
measured on the review baseline (`97123d3a4ac6cfae2f2fd76456d1bc173027b4fa`, v3.10.0)
and **include inline `#[cfg(test)]` tests**, which live in the same files. See
[`THREAT-MODEL.md`](THREAT-MODEL.md) for the assets, properties, and adversary model
that motivate this ordering; the out-of-scope UI is noted at the end.

## Priority overview

| Priority | Area | LOC (incl. inline tests) | Why |
|---|---|---|---|
| P0 | `src-tauri/src/sm/` | 8_492 | Pure IO-free consensus core + DST harness |
| P0 | `src-tauri/src/p2p/ledger/` | 6_369 | Conservation, coverage, stake, slash, reorg |
| P0 | `src-tauri/src/security/` | 2_142 | Vault + CryptoEngine + ML-DSA (`hybrid_crypto`) |
| P1 | Network pipeline (7 files) | 6_315 | Gossip ingestion, PoS election, live finality, fork heal |
| P2 | Persistence / RPC / commands | — | State snapshot, identity registry, JSON-RPC, node runtime, Tauri IPC |

## P0 — Consensus core, ledger, cryptography

### `src-tauri/src/sm/` — pure deterministic consensus core (8_492 LOC)
The IO-free, deterministic finality gadget (Phase 0, C1) with an injected clock and RNG,
plus the seeded DST simulation harness.

- **Entry points:** `node.rs` (`Node::handle`, Event→Effect), `finality.rs` (GADGET-1
  epoch/checkpoint), `finality_vote.rs` (GADGET-2 ML-DSA vote + ⅔ certificate),
  `finality_rule.rs` (GADGET-3 justify/finalize), `finality_slashing.rs` (GADGET-4
  double-vote + surround + proof), `fork_choice.rs` (GADGET-5A LMD-GHOST), `sim.rs` (DST).
- **Invariants to verify:**
  - Determinism (C1): identical votes + identical chain ⇒ identical finality verdict on
    every node (ordered `BTreeMap`/`BTreeSet`).
  - Quorum threshold carved as `backing×3 ≥ total×2`.
  - Two-consecutive-link justify → finalize rule (Casper-FFG) is sound.
  - Accountable safety: double-vote and surround are detected with non-repudiable proofs;
    the slashing window is `≥` the unbonding window (const-assert).
  - The core contains no classical primitive on the irreversibility path (votes are
    ML-DSA-65 only, ADR-005).

### `src-tauri/src/p2p/ledger/` — blockchain state (6_369 LOC incl. `tests.rs`)
Module-folder: accounting/types/O(1) cache/PQ genesis (`mod`), plus validation (COVER),
stake, slash, reorg, tests. External surface `p2p::ledger::` is unchanged.

- **Entry points:** `validate_block_against_prev` (the shared validator for linear
  integration, reorg, and sync), `seal_block_at` (COVER-2 production), `integrate_remote_block`
  (finality-floor veto), stake/slash accounting, `reorg_to_fork`.
- **Invariants to verify:**
  - Conservation `Σ(spendable + staked + unbonding) + burned == minted` at every block.
  - COVER-1/COVER-2 symmetry: no uncovered spend/stake is accepted or produced; a
    self-sealed block always passes `validate_block_against_prev`.
  - Finalized floor is monotone, hash-verified, and vetoes any reorg ≤ floor (LIVE-2).
  - Slash reaches unbonding stake with a deterministic consumption breakdown
    (`slash_unbonding`), bound to hash + Merkle, re-verifiable per node
    (`expected_slash_consumption`), and a reorg restores exactly the consumed entries (LIVE-3B).
  - PROPOSER-1: non-genesis blocks whose proposer is not a bonded (stake ≥ MIN)
    validator as-of-parent are rejected.
  - Merkle root correctness; anti-replay via `seen_tx_hashes` + monotonic per-account nonce.

### `src-tauri/src/security/` — cryptography (2_142 LOC)
- **Entry points:** `mod.rs` (`CryptoEngine`: Ed25519 transport + ML-DSA address,
  `ADDR_DOMAIN`), `hybrid_crypto.rs` (ML-DSA-65 / FIPS 204 account authority — the pure PQ
  path), `pq_vault.rs` (Argon2id + AES-256-GCM identity vault), `cipher.rs` /
  `crypto_agility.rs` (symmetric primitives + crypto agility).
- **Invariants to verify:**
  - Correct and safe use of `fips204` for ML-DSA-65 (key generation, sign, verify; domain
    separation via `ADDR_DOMAIN` / `MSIG_DOMAIN`; the account address binding `lie`).
  - Multisig `verify_multisig`: rebind-proof binding, policy hash, ≥ threshold **distinct**
    valid signers.
  - Vault: Argon2id parameters, AES-256-GCM nonce handling, `zeroize` of all secrets, no
    plaintext password persisted; biometric KEK handling.
  - No `unwrap()` on the crypto path; secrets are zeroized (Rust rules #2, #3).

## P1 — Network pipeline (6_315 LOC across 7 files)

| File | LOC | Role |
|---|---|---|
| `p2p/dispatcher.rs` | 1_998 | `dispatch_incoming` — the 9-step ingestion pipeline |
| `p2p/finality_live.rs` | 1_362 | LIVE-1→3B wiring of the gadget's IO; pubkey↔address bridge |
| `p2p/gossip.rs` | 729 | Gossip protocol variants + envelope signing |
| `p2p/fork_heal.rs` | 672 | LIVE-4 deep fork reconciliation (`ForkReconciler`) |
| `p2p/pos_consensus.rs` | 633 | PoS leader election (buried beacon) |
| `p2p/willow_node.rs` | 565 | Iroh endpoint + stores + gossip topic |
| `p2p/gossip_tasks.rs` | 356 | Background Hello/Ping broadcast tasks |

- **Entry points:** `dispatch_incoming` (untrusted-bytes boundary), envelope
  sign/verify path (ML-DSA-65 since v4), `FinalityTracker` and the
  `validator_stakes_by_pubkey` bridge, `ForkReconciler` (bounded 1024-block buffer).
- **Invariants to verify:**
  - The 9-step pipeline order is enforced and each step is effective (size cap, dedup LRU,
    ±90s freshness, adaptive rate limit clamp [15,120], monotonic nonce, ML-DSA envelope
    auth). No path skips signature verification.
  - Live finality votes gossiped by the tracker map correctly to on-chain stake weight
    (bridge re-keys stake purely from the chain).
  - Fork reconciliation: bounded buffer with deterministic eviction; "longest-above-floor +
    lexicographic tie-break" is symmetric (exactly one side adopts); reorg goes through a
    trial clone under the absolute floor; ancestor probing is bounded to the floor.

## P2 — Supporting surfaces

`p2p/state_persistence.rs` (SQLite snapshot every 30s + genesis guard), `p2p/username.rs`
(@pseudo identity registry), the JSON-RPC server (17 methods, `quanta-node`),
`node_runtime` (shared app+daemon bootstrap), and `commands/` (Tauri IPC handlers +
`views.rs` shared view-models). Lower priority but part of the reachable attack surface —
notably the JSON-RPC and IPC boundaries (see the threat model's trust boundaries).

## Out of scope

The Svelte UI, the three.js / WebGL 3D scenes, i18n resources, and documentation. These
carry no consensus, ledger, or key-handling logic.

## Suggested engagement shapes

Offered as ways to slice the work; no pricing is implied.

| Shape | Covers | Focus |
|---|---|---|
| (a) Crypto-focused review | `src-tauri/src/security/` + `fips204` usage | ML-DSA-65 correctness, multisig, vault, KEK/biometric, domain separation |
| (b) Consensus review | `src-tauri/src/sm/` + `src-tauri/src/p2p/ledger/` | Finality soundness, conservation, coverage, slashing, reorg, determinism |
| (c) Network review | P1 pipeline (7 files) | Ingestion hardening, DoS/replay/partition, envelope auth, fork reconciliation |

A combined (a)+(b)+(c) engagement covers all of P0 and P1. P2 can be added as a stretch
scope or folded into (c).
