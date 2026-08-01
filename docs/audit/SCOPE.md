# Quanta — Audit Scope

Prioritized scope for an external review of the Quanta protocol. Line counts were
measured on `main` at v3.15.1 (`TORUS_PROTOCOL_VERSION = 9`)
and **include inline `#[cfg(test)]` tests**, which live in the same files. See
[`THREAT-MODEL.md`](THREAT-MODEL.md) for the assets, properties, and adversary model
that motivate this ordering; the out-of-scope UI is noted at the end.

## Priority overview

| Priority | Area | LOC (incl. inline tests) | Why |
|---|---|---|---|
| P0 | `src-tauri/src/sm/` | 8_507 | Pure IO-free consensus core + DST harness |
| P0 | `src-tauri/src/p2p/ledger/` | 7_545 | Conservation, coverage, stake, slash, reorg, **reward plan (v8/v9)** |
| P0 | `src-tauri/src/security/` | 2_081 | Vault + CryptoEngine + ML-DSA |
| P1 | Network pipeline (8 files) | 8_105 | Gossip ingestion, PoS election, live finality, fork heal, DHT bootstrap |
| P2 | Persistence / RPC / commands | — | State snapshot, identity registry, JSON-RPC, node runtime, Tauri IPC |

## P0 — Consensus core, ledger, cryptography

### `src-tauri/src/sm/` — pure deterministic consensus core (8_507 LOC)
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

### `src-tauri/src/p2p/ledger/` — blockchain state (7_545 LOC incl. `tests.rs`)
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
    validator as-of-parent are rejected — **except** on an OPEN-DOOR-1 open slot
    (one block in 16, a pure function of height).
  - **MINT-EXACT-1 (v8, new):** the block reward equals `emission_for_block(prior mined)`
    and is *recomputed* by every receiver, never merely bounded. Verify that no path
    reintroduces a slack bound, and that `mine_tx` remains unreachable in release builds.
  - **REWARD-SHARE-1 (v9, new):** the proposer/participants split is recomputed by every
    node (`validate_block_reward_plan`) from the same function the producer used
    (`expected_block_rewards`); it is exact to the µQTA and scale-invariant (minting *less*
    is legal, shrinking another payee's share is not). Verify the recent-participants window
    is derived only from `block.miner` over `SHARE_WINDOW_BLOCKS`, and that a pending mint is
    revoked before a new one is struck (a stale mint computed for another height would make
    the next block rejected network-wide).
  - Merkle root correctness; anti-replay via `seen_tx_hashes` + monotonic per-account nonce.

### `src-tauri/src/security/` — cryptography (2_081 LOC)
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

## P1 — Network pipeline (8_105 LOC across 8 files)

| File | LOC | Role |
|---|---|---|
| `p2p/dispatcher.rs` | 2_209 | `dispatch_incoming` — the 10-step ingestion pipeline |
| `p2p/finality_live.rs` | 1_718 | LIVE-1→3B wiring of the gadget's IO; pubkey↔address bridge |
| `p2p/gossip.rs` | 1_051 | Gossip protocol variants + envelope signing |
| `p2p/fork_heal.rs` | 776 | LIVE-4 deep fork reconciliation (`ForkReconciler`) |
| `p2p/willow_node.rs` | 711 | Iroh endpoint + stores + gossip topic (incl. the message-size cap) |
| `p2p/pos_consensus.rs` | 664 | PoS leader election (buried beacon) + open slots |
| `p2p/rendezvous.rs` | 593 | RDV-1 serverless bootstrap — **parses attacker-writable DHT content** |
| `p2p/gossip_tasks.rs` | 383 | Background Hello/Ping broadcast tasks |

- **Entry points:** `dispatch_incoming` (untrusted-bytes boundary), envelope
  sign/verify path (ML-DSA-65 since v4), `FinalityTracker` and the
  `validator_stakes_by_pubkey` bridge, `ForkReconciler` (bounded 1024-block buffer).
- **Invariants to verify:**
  - The 10-step pipeline order is enforced and each step is effective (size cap, dedup LRU,
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
| (c) Network review | P1 pipeline (8 files) | Ingestion hardening, DoS/replay/partition, envelope auth, fork reconciliation |

A combined (a)+(b)+(c) engagement covers all of P0 and P1. P2 can be added as a stretch
scope or folded into (c).
