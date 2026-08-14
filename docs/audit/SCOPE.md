# Quanta — Audit Scope

Prioritized scope for an **independent** external review of the Quanta protocol. Baseline:
`main` at v3.16.0 (`TORUS_PROTOCOL_VERSION = 10`, `CHAIN_ID = "quanta-mainnet-v10"`); a
dedicated `audit-baseline` tag will be frozen at engagement start. Line counts were
measured at that baseline with `find src-tauri/src -name '*.rs' | xargs wc -l` and
**include inline `#[cfg(test)]` tests**, which live in the same files; the Rust backend
totals 44_067 lines. See [`THREAT-MODEL.md`](THREAT-MODEL.md) for the assets, properties,
and adversary model that motivate this ordering; the out-of-scope UI is noted at the end.

Baseline verification: `cargo test` — **608 library tests + 1 integration test**, 0
failures; `cargo clippy --all-targets -- -D warnings` — exit 0.

References below name a file and a symbol (`file.rs::function`, `file.rs`, `CONSTANT`)
rather than a line number: line numbers go stale at the next commit, symbols do not.

## This is a request for a second review, not a first one

A paid **single-reviewer** external review was performed on **2026-08-13**: **85 findings,
13 critical**. Its six reports and one executable PoC are published verbatim in
[`2026-08-13/`](2026-08-13/) — nothing removed or softened, including the passages showing
that an earlier fix by this project was wrong. What was fixed, how each fix was proven, and
what remains open is in [`REMEDIATION-2026-08-13.md`](REMEDIATION-2026-08-13.md). That
engagement was not an audit by an established firm, and it is not a substitute for one.

Four consequences for the reviewer we are looking for:

- **The starting point is public.** The findings, the exploit reasoning and the claimed
  fixes are all in the repository, so effort does not go into rediscovering the same 85
  items, and every remediation claim can be checked against the finding it says it closes.
- **The code under review is younger than the review that motivated it.** Remediation broke
  the protocol (v9 → v10: `CANON-1` injective signing pre-images, `NONCE-ONCHAIN-1`
  sequential on-chain nonces) and took the suite from 513 to 608 library tests. The fixes
  are the least-reviewed code in the repository; several of them touch consensus.
- **The first review states its own blind spots** ([`2026-08-13/00-SYNTHESE.md`](2026-08-13/00-SYNTHESE.md),
  "Ce qui n'a pas été vérifié"): no real network — everything ran in-process; no constant-time
  instrumentation of `fips204` or `aes-gcm`; no real macOS Keychain; the GUI was never
  launched; no fuzzing; and no review of `fips204` itself. Those areas are open by
  construction, not by omission.
- **Named items remain open** and are listed per area below rather than buried: election-seed
  grinding, no post-quantum VRF, O(height) happy-path block validation, and a deliberate
  absence of FIPS-204 `ctx` domain separation.

## Priority overview

| Priority | Area | LOC (incl. inline tests) | Why |
|---|---|---|---|
| P0 | `src-tauri/src/sm/` | 8_525 | Pure IO-free consensus core + DST harness |
| P0 | `src-tauri/src/p2p/ledger/` | 9_864 | Conservation, coverage, canonical encodings, on-chain nonce, stake, slash, reorg, reward plan |
| P0 | `src-tauri/src/security/` | 2_491 | Vault + `CryptoEngine` + ML-DSA + address codec |
| P1 | Network pipeline (8 files) | 10_746 | Gossip ingestion, PoS election, live finality, fork heal, DHT bootstrap |
| P2 | Persistence / RPC / commands | 6_454 | State snapshot, username registry, JSON-RPC (17 methods), node runtime, 41 Tauri IPC commands |

## P0 — Consensus core, ledger, cryptography

### `src-tauri/src/sm/` — pure deterministic consensus core (8_525 LOC)

The IO-free, deterministic finality gadget (Phase 0, C1) with an injected clock and RNG,
plus the seeded DST simulation harness. Design notes:
[`../protocol/FINALITY-GADGET.md`](../protocol/FINALITY-GADGET.md).

- **Entry points:** `node.rs::Node::handle` (Event→Effect), `finality.rs` (GADGET-1
  epoch/checkpoint), `finality_vote.rs::signable_bytes` and `Vote::verify` (GADGET-2 ML-DSA
  vote + ⅔ certificate), `finality_rule.rs` (GADGET-3 justify/finalize),
  `finality_slashing.rs` (GADGET-4 double-vote + surround + proof), `fork_choice.rs`
  (GADGET-5A LMD-GHOST), `sim.rs` (DST).
- **Invariants to verify:**
  - Determinism (C1): identical votes + identical chain ⇒ identical finality verdict on
    every node (ordered `BTreeMap`/`BTreeSet`).
  - Quorum enforced as `backing×3 ≥ total×2` in `finality_vote.rs::meets_supermajority`.
    The ⅔ value is an **adjustable default** (ADR-006 / [`../decisions/ADR-009-carved-vs-adjustable.md`](../decisions/ADR-009-carved-vs-adjustable.md)),
    not a graven promise — the code says so; the documentation must not claim otherwise.
  - Two-consecutive-link justify → finalize rule (Casper-FFG) is sound.
  - Accountable safety: double-vote and surround are detected with non-repudiable proofs;
    the slashing window is `≥` the unbonding window (`SLASH_EVIDENCE_WINDOW_BLOCKS`,
    const-assert against `UNBONDING_PERIOD_BLOCKS`).
  - The core contains no classical primitive on the irreversibility path (votes are
    ML-DSA-65 only, ADR-005), and a vote pre-image names its network (`CHAIN_ID`, H-05).
  - The harness proves what it claims: `sim.rs::default_seed_count` runs 64 seeds by
    default (`QUANTA_SIM_SEEDS` deepens it); a fixed 128-seed sweep (`COVERAGE_SEEDS`,
    deliberately not env-tunable) asserts the generator really produces partitions,
    drops/dups/delays, equivocators and crash/restart; and a separate test plants a
    known-bad split-brain scenario and requires the executor to flag it, carrying the seed
    so the run replays byte-for-byte. A sweep that has never been red proves nothing.

### `src-tauri/src/p2p/ledger/` — blockchain state (9_864 LOC incl. `tests.rs`)

Module-folder: accounting / types / O(1) cache / PQ genesis (`mod.rs`, 2_148), validation
(`validation.rs`, 1_359), stake (486), slash (356), reorg (821), tests (`tests.rs`, 4_694).
External surface `p2p::ledger::` is unchanged.

- **Entry points:** `validation.rs::validate_block_against_prev` (the shared validator for
  linear integration, reorg and sync), `validation.rs::verify_tx` (the single authority
  gate, used both at mempool admission and in block validation),
  `reorg.rs::seal_block_at` (COVER-2 production), `reorg.rs::integrate_remote_block`
  (finality-floor veto), `reorg.rs::reorg_to_fork`, `mod.rs::expected_block_rewards` +
  `validation.rs::validate_block_reward_plan`, `slash.rs::expected_slash_consumption`,
  `mod.rs::tx_signing_preimage`.
- **Invariants to verify:**
  - Conservation `Σ(spendable + staked + unbonding) + burned == minted` at every block.
  - COVER-1/COVER-2 symmetry: no uncovered spend/stake is accepted or produced
    (`validation.rs::uncovered_tx_indices`); a self-sealed block always passes
    `validate_block_against_prev`.
  - **CANON-1 (v10, new):** the transaction signing pre-image, the Merkle leaf and the
    block header are injective — domain separator, length-prefixed fields, a stable numeric
    tag for the transaction type. The previous `format!` encoding let two semantically
    different transactions share one signature and one hash (CRIT-1). Verify injectivity at
    field boundaries and that every pre-image is bound to `CHAIN_ID` (H-05).
  - **NONCE-ONCHAIN-1 (v10, new):** the per-account nonce is sequential and verified **at
    inclusion**, on all four admission paths (`validation.rs::nonce_faults`,
    `mod.rs::apply_block_nonce_effects`, `mod.rs::rebuild_chain_nonces`). Before it, a
    signature was replayable until the payer was drained (C-01). Verify no path commits
    nonce effects before validation, and that a reorg rebuilds the high-water mark from the
    chain rather than trusting a cached value.
  - **MINT-EXACT-1:** the block reward equals `emission_for_block(prior mined)` and is
    *recomputed* by every receiver, never merely bounded. Verify no path reintroduces a
    slack bound, and that `mine_tx` remains `#[cfg(test)]`, unreachable in release builds.
  - **REWARD-SHARE-1 + REWARD-WEIGHT-1 (H-08):** half to the proposer, the rest split among
    recent participants **weighted by the number of blocks each produced** within
    `SHARE_WINDOW_BLOCKS = 32` (`mod.rs::recent_participation`) — the earlier equal split
    among *distinct addresses* subsidised identity duplication (measured: 45.2 % of every
    reward captured with 28 identities). The plan is recomputed and imposed by every node
    (`validate_block_reward_plan`), exact to the µQTA, scale-invariant (minting *less* is
    legal, shrinking another payee's share is not), the integer remainder goes to the
    proposer. Verify the window is derived only from `block.miner` over the chain, and that
    a pending mint is revoked before a new one is struck.
  - **BLOCK-TIME-1 / BLOCK-TIME-2 (C-02):** `validation.rs::validate_block_timestamp` —
    RFC3339-parsable, and non-decreasing against the **median of the last
    `MEDIAN_TIME_SPAN = 11` blocks** (`validation.rs::median_time_before`), not against the parent
    alone. Comparing to the parent was a ratchet: one block dated 2099 was parsable and
    greater than its parent, so it was accepted and no honest block could ever be sealed
    again. A drift bound is **deliberately absent** — without a trusted clock two desynced
    nodes would disagree on the same block, i.e. fork, and a sub-second RFC3339 timestamp
    leaves grinding room anyway; fork grinding is closed at fork choice, not here. The
    reasoning is written in the function's own comment.
  - **BLOCK-SIZE-1 (M-10):** `MAX_TXS_PER_BLOCK = 256`, checked *before* any signature
    verification.
  - **H-07:** the per-block emission sum is checked arithmetic — an overflow is a block
    rejection, identical in debug and release. Previously it panicked in debug, and the
    dispatch loop was never restarted: one message deafened a node permanently.
  - Finalized floor is monotone, hash-verified, and vetoes any reorg ≤ floor (LIVE-2);
    reorg depth is bounded by `MAX_REORG_DEPTH = 128` independently of finality.
  - **Fork choice (C-04 / FORK-RANK-1):** `reorg.rs::fork_choice_prefers` ranks candidates
    by stake-weighted election rank (`pos_consensus.rs::election_rank_of`); the tip hash is
    only a fallback when ranks tie or the slot is an unranked open slot. Verify the rule is
    symmetric (exactly one side adopts) and that production and reception apply the same
    ranking. See [`../protocol/FORK-RANK.md`](../protocol/FORK-RANK.md).
  - Slash reaches unbonding stake with a deterministic consumption breakdown, bound to hash
    + Merkle, re-verifiable per node (`slash.rs::expected_slash_consumption`), and a reorg
    restores exactly the consumed entries (LIVE-3B).
  - PROPOSER-1: non-genesis blocks whose proposer is not a bonded (stake ≥
    `MIN_VALIDATOR_STAKE`) validator as-of-parent are rejected — **except** on an
    OPEN-DOOR-1 open slot (`pos_consensus.rs::is_open_slot`, one block in 16, a pure
    function of height).
  - Merkle root correctness; anti-replay via `seen_tx_hashes` at admission **and** the
    on-chain nonce at inclusion.
  - **Open (M-14):** an O(1) gate (timestamp + size bound) now precedes any chain walk, so
    a rejected block no longer costs O(height). The **happy path is still O(height)**;
    closing it needs incremental views, whose slightest error is a fork. The
    `CHAIN_WALKS` counter (test-only) makes the property exact rather than timing-based.

### `src-tauri/src/security/` — cryptography (2_491 LOC)

`mod.rs` (973), `pq_vault.rs` (602), `address.rs` (393), `hybrid_crypto.rs` (203),
`crypto_agility.rs` (135), `cipher.rs` (93), `biometric.rs` (92).

- **Entry points:** `mod.rs` (`CryptoEngine`, `ADDR_DOMAIN` / `MSIG_DOMAIN` address
  derivation, `CryptoEngine::lock`), `hybrid_crypto.rs::derive_ml_dsa` /
  `ml_dsa_sign_deterministic` / `verify_ml_dsa` (the ML-DSA-65 primitives — the account
  *authority rule* lives in `p2p/ledger/validation.rs::verify_tx`, not here),
  `pq_vault.rs` (Argon2id + AES-256-GCM identity vault), `cipher.rs::encrypt` /
  `derive_key`, `address.rs::parse` / `parse_hex_unchecked`, `crypto_agility.rs`.
- **Invariants to verify:**
  - Correct and safe use of `fips204` 0.4.6 for ML-DSA-65 (key generation from a 32-byte
    seed, deterministic signing, verification; domain separation via `ADDR_DOMAIN` /
    `MSIG_DOMAIN`; the intrinsic address↔key binding checked in `verify_tx`).
  - Multisig `p2p/ledger/validation.rs::verify_multisig`: rebind-proof binding, policy
    hash, ≥ threshold **distinct** valid signers, and **MSIG-BOUND-1** — `MAX_MSIG_KEYS =
    16` keys and signatures, bounded *before* any ML-DSA verification (unbounded K×S was
    ~135 s of CPU per message).
  - Vault: Argon2id parameters (64 MiB, t=3, p=4), a random persisted salt with a tested
    legacy migration, AES-256-GCM nonce handling, `zeroize` of all secrets, no plaintext
    password persisted; biometric KEK handling; `LOCK-1` — locking is a real Rust operation
    and `get_recovery_phrase` demands the password.
  - `address.rs::parse` is a strict Bech32m decoder; the checksum-free path is explicitly
    named `parse_hex_unchecked` (BAS-1). Verify the RPC `validateaddress` path is the
    strict one.
  - No `unwrap()` on the crypto path; secrets are zeroized.
  - **Open (MOY-3):** the FIPS-204 `ctx` parameter is empty on all four domains
    (`hybrid_crypto.rs::verify_ml_dsa` passes `&[]`). Setting it on one side only would
    silently invalidate every signature on the network; the four pre-images already carry
    explicit domain separators. This is a recorded decision, not an oversight — a reviewer
    is invited to disagree with it in writing.
  - **Open (TOFU):** the fund-address anchor (`pq_fund_anchor_v1`, in `commands/identity.rs`)
    is unsigned and lives in the same store as the vault; deleting both rows replays the
    trust-on-first-use once. An unforgeable anchor needs a separate store (Keychain/TPM).

## P1 — Network pipeline (10_746 LOC across 8 files)

| File | LOC | Role |
|---|---|---|
| `p2p/dispatcher.rs` | 3_621 | `dispatch_incoming` — the 13-step ingestion pipeline, bans, rate limit, nonce tracker |
| `p2p/finality_live.rs` | 1_723 | LIVE-1→3B wiring of the gadget's IO; pubkey↔address bridge |
| `p2p/gossip.rs` | 1_359 | Gossip message variants (11) + envelope signing, dedup LRU, freshness |
| `p2p/fork_heal.rs` | 1_012 | LIVE-4 deep fork reconciliation (`ForkReconciler`) |
| `p2p/rendezvous.rs` | 939 | RDV-1 serverless bootstrap — **parses attacker-writable DHT content** |
| `p2p/pos_consensus.rs` | 876 | PoS leader election (buried beacon), open slots, election ranking |
| `p2p/willow_node.rs` | 826 | Iroh endpoint + stores + gossip topic (incl. the message-size cap) |
| `p2p/gossip_tasks.rs` | 390 | Background Hello/Ping broadcast tasks |

Adjacent evidence, same area (outside the eight-file count): `p2p/security_tests.rs`
holds 44 network-security tests.

- **Entry points:** `dispatcher.rs::dispatch_incoming` (the untrusted-bytes boundary),
  `dispatcher.rs::envelope_shape_is_plausible`, `dispatcher.rs::NonceTracker::ban_state`,
  `dispatcher.rs::adaptive_limit_for`, the envelope sign/verify path (ML-DSA-65 since v4),
  `finality_live.rs::FinalityTracker` with the `ledger/stake.rs::validator_stakes_by_pubkey`
  bridge, `fork_heal.rs::ForkReconciler`, `pos_consensus.rs::leader_beacon`.
- **Invariants to verify:**
  - **Pipeline order is the security property** (R6). The documented order in
    `dispatcher.rs::dispatch_incoming` is 13 steps, and the ML-DSA-65 signature is step
    **5** — after the raw size bound (`MAX_RAW_ENVELOPE_BYTES` = 4 MiB) and the structural
    shape check, and **before** the canonical id, freshness, dedup, per-peer accounting,
    rate limit and dispatch. Nothing between step 1 and step 5 allocates or mutates at a
    size chosen by the sender. Verify the code still matches that list, and that no path
    skips signature verification.
  - **REPORT-NOAUTH-1 (R1):** on signature failure the node drops and counts, and denounces
    nobody. The previous behaviour broadcast `ReportPeer` naming the *unauthenticated*
    sender, which banned any node on the network with three forged envelopes (the
    executable PoC is published as [`2026-08-13/poc-ban-C1.rs`](2026-08-13/poc-ban-C1.rs)).
  - Size: `MAX_RAW_ENVELOPE_BYTES = 4 * 1024 * 1024` (4 MiB), which also sets
    `willow_node.rs` `max_message_size`. It was 10 MB, which allowed a remote OOM through
    plumtree relay-and-cache before authentication (R3).
  - Rate limit: `adaptive_limit_for` computes `30 × max(1, √(peers/4))` clamped to 120 per
    60 s window. The `MIN_MSG_PER_WINDOW = 15` constant is below the reachable floor — the
    effective floor is 30. Dedup LRU `MAX_SEEN_MESSAGES = 100_000`; freshness ±90 s
    (`gossip.rs::is_fresh_at`); per-peer maps bounded (`MAX_TRACKED_SENDERS`,
    `MAX_TRACKED_REPORTS`).
  - **R15:** the `NonceTracker` (anti-replay + live bans) is persisted as the 8th snapshot
    key and restored **monotonically** (nonces never go backwards, expired bans are not
    resurrected). Rate counters stay volatile on purpose — they are windowed on wall time.
  - Live finality votes gossiped by the tracker map correctly to on-chain stake weight (the
    bridge re-keys stake purely from the chain).
  - Fork reconciliation: bounded buffer with deterministic eviction
    (`fork_heal.rs::worst_entry`, O(log n), no allocation); adoption is symmetric because
    both sides call the same `fork_choice_prefers`; reorg goes through a trial clone under
    the absolute floor; ancestor probing is bounded to the floor.
  - Bootstrap: `rendezvous.rs` parses DHT records any peer can write. R9 removed a mineable
    ordering (composition sorted by `EndpointId` bytes let 8 prefix-ground identities
    eclipse a new node); verify no ordering or selection remains attacker-shapeable.
  - Eclipse detection in `willow_node.rs` is an 80 %-of-one-prefix heuristic
    (`ECLIPSE_MIN_PEERS`, `ECLIPSE_PREFIX_LEN`, `ECLIPSE_THRESHOLD`), not a defense; it is
    listed as an accepted risk, not as a control.
  - **Open (election):** the beacon is sourced from a block buried
    `LEADER_ENTROPY_LOOKBACK = 2` slots behind the tip, so the sealer of the tip cannot
    grind itself — but the proposer at `h − 2` still influences the beacon at `h`, and
    without a VRF the leader of `h+1` is public at `h`, therefore targetable. There is no
    deployable post-quantum VRF; the fallback ranking limits the damage. See
    [`../decisions/ADR-004-election-randomness.md`](../decisions/ADR-004-election-randomness.md).
  - **Open (bans):** `REPORT_BAN_THRESHOLD = 3` counts *identities*, and an ML-DSA identity
    costs ~165 µs to make. Three puppets still ban anyone. Binding a report to an
    unforgeable cost (bonded stake) is an unmade design decision.

Live wiring of the gadget to this pipeline is described in
[`../protocol/LIVE-WIRING.md`](../protocol/LIVE-WIRING.md).

## P2 — Supporting surfaces (6_454 LOC)

`p2p/state_persistence.rs` (314 — SQLite snapshot every 30 s, 8 keys, genesis guard),
`p2p/username.rs` (1_191 — `@pseudo` registry; CLAIM-WINDOW-1 bounds `claimed_at` between
`CLAIM_EPOCH_FLOOR` and now + `CLAIM_MAX_FUTURE_SKEW_SECS`, and closes contestation after
`CLAIM_CONTEST_WINDOW_SECS`; the claim pre-image is canonical and chain-id bound),
`rpc.rs` (1_856 — the JSON-RPC server, 17 methods, cookie authority with COOKIE-OWN-1
ownership/permission checks, `Origin`/`Content-Type` CSRF guard, `rpc.rs::listtx_window`
bounded by `MAX_LISTTX_SCAN_BLOCKS`, `sendtoaddress` closed by default behind
`QUANTA_RPC_ALLOW_SEND`), `node_runtime.rs` (158 — shared app+daemon bootstrap, crypto
provider install), and the Tauri IPC surface: `commands/` (2_262), `commands_v3.rs` (195),
`views.rs` (478) — **41 commands** registered in `lib.rs`, now covered by an applicative ACL
manifest declared in `build.rs` (A1: before the fix, the ACL controlled nothing applicative
and any script in the webview could read the recovery phrase).

Lower priority but part of the reachable attack surface — notably the JSON-RPC and IPC
boundaries (see the threat model's trust boundaries).

## Out of scope

The Svelte UI, the three.js / WebGL 3D scenes, i18n resources, and documentation. These
carry no consensus, ledger, or key-handling logic. The CSP and the webview boundary are an
exception: they are in scope through the IPC surface above, because A13 showed a CSP fix
can regress silently through the whole verification chain.

## Suggested engagement shapes

Offered as ways to slice the work; no pricing is implied.

| Shape | Covers | Focus |
|---|---|---|
| (a) Crypto-focused review | `src-tauri/src/security/` + `fips204` usage + the CANON-1 pre-images | ML-DSA-65 correctness, injective encodings, multisig bounds, vault, KEK/biometric, domain separation (incl. the open `ctx` decision) |
| (b) Consensus review | `src-tauri/src/sm/` + `src-tauri/src/p2p/ledger/` | Finality soundness, conservation, coverage, on-chain nonce, reward plan, slashing, reorg, fork rank, determinism |
| (c) Network review | P1 pipeline (8 files) | Ingestion order and hardening, DoS/replay/partition, envelope auth, ban economics, bootstrap, fork reconciliation |

A combined (a)+(b)+(c) engagement covers all of P0 and P1. P2 can be added as a stretch
scope or folded into (c).

The open items above map onto these shapes: the `ctx` decision and the TOFU anchor into
(a); election grinding, VRF absence and O(height) validation into (b) and (c); ban economics
and bootstrap shapeability into (c). We would rather have them contested than confirmed.
