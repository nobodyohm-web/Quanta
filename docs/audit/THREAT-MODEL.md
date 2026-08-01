# Quanta — Threat Model

This document states what Quanta protects, against whom, which security properties it
claims (each traced to a concrete mechanism in the code), where the trust boundaries
sit, and — most importantly for an auditor's confidence — what is *not* yet defended.
It assumes the context in [`README.md`](README.md); read that first.

## 1. System overview

Quanta is a sovereign, serverless P2P cryptocurrency: Rust protocol/backend (Tauri 2.0)
plus a Svelte desktop UI (out of scope). Consensus is stake-weighted Proof-of-Stake
leader election with a Casper-FFG-style finality gadget. The distinguishing property is
end-to-end post-quantum cryptography for money, finality, gossip authentication, and the
transport key exchange. See the README for the one-paragraph summary and the source-tree
map in the repository's top-level `CLAUDE.md`.

## 2. Assets

| Asset | Representation | Where it lives |
|---|---|---|
| Funds / balances | `u64` µQTA (1 QUANTA = 1_000_000 µQTA; integer math, no float) | ledger balance cache, on-chain state |
| Stake | spendable / bonded / unbonding compartments, height-indexed unlock | on-chain stake state derived from `Stake`/`Unstake` tx |
| Account authority | ML-DSA-65 (FIPS 204) private key = account seed | encrypted identity vault |
| Identity vault | Argon2id-derived key + AES-256-GCM ciphertext | `security/pq_vault.rs`, local disk / OS Keychain (KEK) |
| Finality / irreversibility | finalized floor index (monotone, hash-verified) | `Ledger::finalized_floor_index` |
| Network availability | peer connectivity, gossip liveness, chain sync | Iroh QUIC + gossip layer |

The highest-value assets are the ML-DSA account seed (spending authority) and the
finalized history (irreversibility). Loss of the seed means loss of funds; a break of
finality would mean reversal of settled transactions.

## 3. Claimed security properties

Each property below is traceable to a real mechanism. Verifying that the mechanism
actually enforces the property is the purpose of the audit.

### 3.1 Supply conservation
Invariant `Σ(spendable + staked + unbonding) + burned == minted` holds at every block.
Staking *moves* coins between compartments (it does not burn them); slashing moves
stake to burned (conservation-neutral); the 1% transfer burn moves spend to burned.
Enforced across `p2p/ledger/` (accounting, stake, slash) and checked by the DST harness.

### 3.2 Account authority is pure ML-DSA-65
Transaction authority is verified purely with ML-DSA-65 (`verify_tx`), with the account
key bound to the `from` address via `BLAKE3(ADDR_DOMAIN ‖ key)`. The vestigial Ed25519
co-factor was removed from the tx authority path (PQ-MIG-3B). Only synthetic addresses
`NETWORK` and `ESCROW` are exempt from signature verification (`BURN` is **not** exempt —
see AUDIT-TX-1 in §7).

### 3.3 Native M-of-N multisig (MSIG-1)
An M-of-N multisig address commits to its policy as
`BLAKE3(MSIG_DOMAIN ‖ sorted keys ‖ threshold)`, tagged `pq_public_key == "msig1"`, with
the authority carried as JSON in the existing `pq_signature` field (no new wire field, so
single-key transactions stay byte-identical). `verify_multisig` checks rebind-proof
binding + hash + at least `threshold` distinct valid signers. This is post-quantum quorum
custody built without a threshold-ML-DSA primitive.

### 3.4 Coverage (COVER-1 / COVER-2)
No uncovered spend or stake can enter the chain. A single coverage rule
(`uncovered_tx_indices` over on-chain spendable balance, a pure function of the chain,
never the mempool; sequential; intra-block credits counted) is applied on both paths:
- **COVER-1 (receive):** `validate_block_against_prev` — the shared validator for linear
  integration, reorg, and sync — rejects any received block with an uncovered spend/stake.
- **COVER-2 (produce):** `seal_block_at` excludes uncovered tx (cache revert + eviction) so
  self-sealed blocks are valid by construction and always pass `validate_block_against_prev`.

### 3.5 Finality (Casper-FFG)
Epoch = 32 blocks (`EPOCH_LENGTH_BLOCKS`). Checkpoint votes are ML-DSA-65 signed
(source→target); a certificate requires a ⅔-stake quorum with the threshold carved in as
`backing×3 ≥ total×2` (`QUORUM_NUM/DEN`). Two consecutive justified links finalize
(`finality_rule.rs`). The live floor (`finalized_floor_index`, LIVE-2) is monotone,
hash-verified, snapshot-persisted; `integrate_remote_block` refuses any fork that would
replace a block ≤ floor, so finalized history is irreversible on the live network.
Free lexicographic tie-break applies only *above* the floor (Gasper).

### 3.6 Accountable safety (slashing)
Equivocation — double vote and surround voting — is detected in the pure core
(`sm/finality_slashing.rs`, GADGET-4) with non-repudiable ML-DSA proofs. On the live
ledger, a detected fault is gossiped and turned into a `Slash` tx whose authority is the
embedded proof, re-verified by every node (`verify_block_slashes`). The slash destroys the
offender's stake STAKE→BURN (conservation-neutral) and reaches unbonding stake (LIVE-3B):
the slashable base is `staked + unbonding`, closing the "unstake-and-run" gap. The
slashing window equals the unbonding period (`UNBONDING_PERIOD_BLOCKS = 10_080`,
const-asserted `≥` the slashing window). A malicious proposer cannot punish an innocent —
the proof, offender address, and fraction are all re-checked.

### 3.7 Gossip ingestion pipeline
`dispatch_incoming` enforces, in order: ① size check (max 10 MB envelope), ② JSON
deserialize, ③ per-peer ban check, ④ dedup (seen-messages LRU 100K), ⑤ timestamp
freshness (±90s), ⑥ adaptive rate limit (`sqrt(peers/4) × 30 msg/min`, clamped [15, 120]),
⑦ per-sender monotonic nonce (anti-replay), ⑧ envelope signature verification — **ML-DSA-65**
since the v4 hard-fork (the Ed25519/legacy fallback was removed), ⑨ payload dispatch.
ChainSegment is capped at 50 blocks.

### 3.8 Vault and local key protection
The identity vault derives a key with Argon2id and encrypts with AES-256-GCM; secrets are
`zeroize`d. Touch ID uses a random KEK stored in the macOS Keychain behind
`SecAccessControl(.BIOMETRY_CURRENT_SET)` — the OS requires the fingerprint on read and
invalidates the item if the enrolled set changes; the KEK wraps the Argon2id-derived keys.
The password is never stored (it remains the fallback). `UnlockGuard` applies exponential
backoff against brute force, shared across password and biometric paths.

### 3.9 Post-quantum transport confidentiality
The QUIC/TLS 1.3 transport negotiates the hybrid `X25519MLKEM768` key exchange
(ML-KEM-768 ⊕ X25519) via rustls `prefer-post-quantum` with the `aws-lc-rs` provider,
installed as the process default at startup (`lib.rs::run`). This is a defense against
"harvest-now, decrypt-later" on transport confidentiality (PQ-TRANSPORT-1).

## 4. Adversary model

| Adversary | Capabilities assumed | Primary mitigations |
|---|---|---|
| Network attacker | replay, MITM, partition, DoS/flood | envelope nonce + timestamp + ML-DSA auth, dedup LRU, rate limit, 10 MB cap, TLS 1.3 hybrid KEX, fork reconciliation |
| Byzantine validators (< ⅓ stake) | equivocate, withhold, propose invalid blocks | ⅔ quorum finality, accountable slashing, proposer verified on receive, shared block validator |
| Malicious peer | malformed / oversized / forged messages | full ingestion pipeline (§3.7), signature verification, ban-on-report |
| Local attacker (stolen machine) | offline access to disk / vault | Argon2id + AES-256-GCM, zeroize, biometric KEK, UnlockGuard backoff |
| Quantum adversary | harvest-now-decrypt-later; future signature forgery | ML-DSA-65 account/finality/envelope auth, hybrid transport KEX (residual: Iroh NodeId, §6) |

The consensus safety assumption is the standard BFT bound: **less than ⅓ of the total
staked weight is Byzantine.** Above that bound, finality safety is not claimed.

## 5. Trust boundaries

- **Gossip ingestion** — `dispatch_incoming` (`p2p/dispatcher.rs`): the boundary between
  untrusted network bytes and internal state. Every inbound message crosses the 9-step
  pipeline (§3.7) before touching consensus or ledger state.
- **Block validation** — `validate_block_against_prev`: the single shared validator used
  by linear integration, reorg, and chain sync. All coverage (COVER-1), proposer
  eligibility (PROPOSER-1), and slash-proof checks funnel through it.
- **JSON-RPC surface** — 17 methods exposed by `quanta-node` (getinfo, getblock,
  sendrawtransaction, sendtoaddress, …). Untrusted callers on this surface must not be
  able to move funds without valid signatures or to crash the node.
- **Tauri IPC** — the `commands/` domain handlers between the Svelte UI and the Rust core.
  In scope insofar as they mutate wallet/ledger state; the UI itself is out of scope.

## 6. Known accepted risks and non-goals

Stated plainly, because credibility depends on it.

- **Iroh NodeId stays classical (Ed25519).** Node identity is not post-quantum; Iroh awaits
  an industry consensus on PQ signing of endpoint IDs. This is an upstream dependency, not
  Quanta code, and will switch when upstream ships.
- **Leader election is deterministic and publicly predictable.** It is a verifiable
  stake-weighted election, **not** a cryptographic VRF and **not** VDF-protected. A buried
  beacon (block hash `LOOKBACK` slots behind the tip) blocks immediate self-grinding, but
  the proposer is publicly foreseeable. Real unpredictability (VRF) + anti-grinding (VDF)
  are on the roadmap. The internal `vrf` identifiers are legacy names.
- **The live network is tiny (alpha).** Security properties are proven in a deterministic
  simulation and verified between two physical machines; they have not been exercised at
  scale or under adversarial conditions in the wild.
- **Eclipse detection is a simple heuristic** (warning when >80% of peers share the same
  8-hex pubkey prefix). It is not a robust anti-eclipse defense.
- **Anti-sybil is a proof of concept** (`p2p/sybil.rs`).
- **Watts are self-declared** in the energy term of the Shapley mining distribution. This
  is an economic gaming surface, but it is **outside the consensus security path** — validator
  weight comes purely from on-chain stake (`validator_stakes()`), never from reputation or
  declared energy.
- **No prior third-party audit.** This package exists to obtain the first one.

## 7. Prior internal reviews

These were internal, not third-party, and are documented in `CLAUDE.md`.

- **P2P audit, 2026-05-07** — corrected: **AUDIT-TX-1** (`verify_tx` exempted `to == "BURN"`
  from signature check, allowing a forged `from=victim, to=BURN` drain; now only `NETWORK`
  and `ESCROW` are exempt); **AUDIT-TX-2** (the 1% burn tx was produced but never broadcast,
  causing a 1% balance divergence network-wide); **AUDIT-TX-3** (gross vs net pre-check on
  transfer+burn); **AUDIT-BLK-1** (fork resolution silently dropped losing-branch tx);
  **AUDIT-BLK-2** (fork resolution popped the tip before validation — validation extracted to
  `validate_block_against_prev` and run pre-mutation); **AUDIT-SYNC-1** (segment ingest did not
  stop at the first rejected block).
- **Slash lifecycle cluster, 2026-07-12** — exhaustive review of the live slashing path:
  TTL-exemption of `Slash` tx (slashing was becoming inoperative in-flight), eviction of
  redundant pending slashes on block apply, non-requeue on reorg, per-offender guard in
  `queue_slash`.
- **PROPOSER-1 (v4 hard-fork)** — the PoS proposer, previously verified only at seal, is now
  verified **on receive**: `validate_block_against_prev` rejects any non-genesis block whose
  proposer is not a bonded validator (stake ≥ MIN) as-of-parent. Deterministic, clock-free
  (Policy A: the non-temporal union {leader ∪ fallbacks ∪ eligible} = "any bonded validator",
  a superset of what seal produces), closing a deferred critical.
