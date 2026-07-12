# Quanta — Sovereign Peer-to-Peer Money

> **Whitepaper — Quanta v3.3**
> Money you forge, own, and that no one can take from you.
> No server. No bank. No mint authority. No censor.

> **Implementation status.** Quanta is alpha research software. This document
> describes the protocol as designed and largely implemented; for the precise
> "real vs. experimental vs. not-yet" breakdown, see the status table in the
> [README](README.md). Nothing here is a promise of present-day production
> security or monetary value. QUANTA is not listed on any exchange and has no
> market price; the project invents none.

---

## 1. Why Quanta

Money today is controlled by authorities you did not choose:

| Problem | Effect |
|---|---|
| Central issuers print at will | Your savings are diluted silently |
| Accounts live on someone else's server | They can be frozen, reversed, or closed |
| Intermediaries sit in every transfer | A cut is taken, a record is kept |
| Custody is delegated to a platform | "Your" coins are an IOU you don't hold |

**Quanta inverts the contract.** It is a scarce, hard-capped currency you mine by
keeping a node online, hold with your own keys, and send peer-to-peer. There is no
company, no server to subpoena, no admin key, and no authority that can inflate it,
freeze you, or sign in your place.

---

## 2. The QUANTA coin — invariants

| Parameter | Value |
|---|---|
| Hard cap | **100,000,000 QUANTA**, enforced at consensus — never exceedable |
| Emission | **decaying**: each minute mints `(cap − minted) / 50,000,000` µQTA |
| Genesis rate | ≈ **120 QUANTA / hour**, declining smoothly toward the cap |
| Premine / mint authority | **none** — nobody can create QUANTA outside the rule |
| Unit | 1 QUANTA = 1,000,000 µQTA (`u64`, deterministic integer math, no float drift) |
| Burn | **1% destroyed on every transfer** (burn-and-mint) |

**Scarcity is the core.** Emission is *rate*-front-loaded but bounded: each tick releases a
fixed fraction of the **remaining** supply, so the issuance rate falls as the cap
approaches and the total emitted asymptotically nears — but never reaches — 100,000,000.
The bound is checked twice at consensus: a per-block emission ceiling and the global hard
cap, so a malicious sealer can neither overshoot the cap nor mint a year's emission in one
block.

Note two honest nuances. (1) "Front-loaded" describes the *rate* (highest at genesis, only
ever declining) — **not** the absolute amounts, which take centuries to approach the cap:

| Time | Cumulative supply (approx.) |
|---|---|
| Genesis rate | ≈ 120 QUANTA/h ≈ 1.05 M/year, declining |
| Year 1 | ≈ 1.05 M (~1% of cap) |
| Year 10 | ≈ 10 M (~10%) |
| ~66 years | 50 M (half the cap) |
| ~219 years | 90 M (90%) |
| → ∞ | approaches but never reaches 100 M |

(2) The 1% burn makes the **net** supply deflationary **only above a transfer-volume
threshold** — when burn exceeds emission. At low volume, emission dominates and supply
still grows toward the cap. We do not claim unconditional deflation.

**On value.** Mining costs real electricity, but a production cost is not a price. QUANTA
has no exchange and no market value today; any exchange value will exist only if people
freely choose to trade it. The app never displays a fabricated fiat figure.

---

## 3. How you earn QUANTA — mining by contribution

Keeping a Quanta node online *is* mining. Once a minute, the network mints that tick's
emission and distributes it by **measured contribution**, using a fixed weighting. This is
*inspired by* the Shapley value's efficiency (shares sum to 1) and symmetry (identical
nodes get equal shares) axioms, but it is a linear O(n) contribution score — **not** an
exact Shapley computation (which is NP-hard). The weights:

```
energy 30% · work 30% · validation 25% · uptime 15%   (sum = 1.0)
```

Energy is measured locally (Intel/AMD RAPL, Apple `powermetrics`, or a calibrated sysinfo
fallback) — never self-reported as an unverifiable number. The *work* term currently
mirrors the energy ratio (there is no compute market in the crypto-only build), so in
practice rewards track measured energy, validation, and uptime. Solo, you receive the full
tick; with peers, your share is proportional to your measured contribution, multiplied by
an anti-Sybil factor. There is no special miner class and no hardware arms race: a laptop
left online contributes.

---

## 4. The ledger

- **Blocks** are sealed by the slot leader roughly every 2 minutes; each carries a BLAKE3
  Merkle root of its transaction IDs.
- **Transfers** are authorized by a mandatory **ML-DSA-65 (FIPS 204)** signature from the
  key that derives the sender's address; Ed25519 remains the gossip transport layer
  (envelope signing) plus a vestigial transaction co-signature. The recipient receives 99%,
  1% is burned, and the sender is debited the full amount — all in `u64` µQTA, so balances
  never drift.
- **Balance cache** is O(1) (incremental `HashMap`), updated on apply and reverted on reorg.
- **Anti-replay**: a strictly monotonic nonce per account plus a `seen_tx_hashes` set.
- **Fork resolution is deterministic**: validate the challenger before mutating, pop the
  losing tip, re-queue transactions exclusive to the losing branch, apply the winner — no
  validated block is ever silently lost.
- **Chain sync** is paginated (`RequestChain → ChainSegment`, ≤50 blocks/segment) and
  resumes from any height.

---

## 5. Wire protocol & security

The only unit on the wire is a signed `GossipEnvelope`. Raw bytes are never trusted.

- **Signing** covers `(sender, nonce, timestamp, payload)` canonically — never the payload alone.
- **Nonce** is strictly monotonic per sender (starts at 1), giving per-peer anti-replay.
- **Timestamp** must be fresh (±90 s window); the same timestamp is signed and sent.
- **Message ID** is `BLAKE3(payload)` — deterministic, enabling dedup.

Every inbound message runs a fixed pipeline before any handler sees it:

```
size guard (≤10 MB) → JSON decode → ban check → dedup (LRU 100K)
  → timestamp freshness (±90s) → adaptive rate limit
  → nonce anti-replay (≥1, strictly monotonic) → Ed25519 signature verify → handler
```

Defenses: adaptive per-peer rate limiting (`sqrt`-scaled), banning (3 reports → 1 h),
DoS caps (10 MB/envelope, 50 blocks/segment), and an eclipse heuristic that warns when
too many peers share a public-key prefix.

---

## 6. Consensus — Proof-of-Stake, verifiable stake-weighted election

Block production is leader-based and deterministic per slot (= chain height):

```
beacon = BLAKE3(domain ‖ buried_block_hash ‖ slot)   (buried = several slots behind tip)
seed   = BLAKE3(domain ‖ beacon ‖ slot ‖ round)
leader = seed % total_weighted_stake                  (weight = stake only — on-chain stake, ADR-002)
```

Minimum validator stake is 1 QUANTA. If the elected leader fails to seal within a 30 s
timeout, production falls back to the next-in-line (bounded rounds); when nobody has
staked, bootstrap is permissionless. The entropy comes from a **buried** block (several
slots behind the tip), not the fresh tip — so the validator who just sealed cannot grind
their own re-election.

**Honest naming.** This is a *deterministic, publicly verifiable* election — **not** a
cryptographic VRF: there is no secret-key component, so a future slot's leader is publicly
predictable (a DoS-targeting surface). A true secret-key VRF (for unpredictability) and a
VDF (for full grinding resistance) are roadmap, not shipped.

---

## 7. Cryptography

| Layer | Mechanism |
|---|---|
| Identity / signatures | **ML-DSA-65** (NIST FIPS 204) transaction authority; Ed25519 gossip transport |
| Key derivation | Argon2id (64 MiB, 3 iterations, parallelism 4) |
| Encryption at rest | AES-256-GCM (unique 12-byte nonce per operation) |
| Hashing / content-addressing | BLAKE3 |
| Memory safety | `zeroize` + `ZeroizeOnDrop` on every secret |

**Post-quantum — active (pure ML-DSA authority).** Account identity is an ML-DSA address —
`BLAKE3(ADDR_DOMAIN ‖ ML-DSA public key)` — and every **transaction**'s authority is a
mandatory **ML-DSA-65** signature (NIST FIPS 204) from the key that derives the sender's
address, via the standalone `fips204` crate (pure Rust, constant-time, no `unsafe`). The
ML-DSA key is **derived from the Ed25519 seed** (BLAKE3 XOF), so no extra secret is
persisted and no vault migration is needed. There is **no Ed25519 fallback** for
transaction authority — verification is pure ML-DSA, not a hybrid AND of two schemes.
Gossip envelopes remain Ed25519 (ephemeral transport, ±90 s freshness window, already
inside QUIC/TLS), and a vestigial Ed25519 co-signature still rides on transactions for
transport-layer continuity.

---

## 8. Identity & self-custody

Your identity is a keypair — nothing more, nothing rented. You are reachable by a short
**`@pseudo` handle** (and a one-time connection code), not by a domain you must keep paying
for. The private key never leaves the device: it lives in an encrypted vault (Argon2id +
AES-256-GCM) and is the only thing that can move your funds. A **recovery key** shown once
at setup is the sole way to restore the account on another device.

No KYC, no tracking, no account to close. You hold the keys; nobody can sign for you, and
nobody can freeze, reverse, or confiscate what you hold.

---

## 9. Threat model & honest limitations

- **Not third-party audited.** The cryptography and networking have had no independent audit.
- **Alpha-scale network.** Convergence was verified between two physical machines, not at scale.
  Large-scale NAT traversal, partition resilience, and eclipse resistance are works in progress.
- **Anti-Sybil is a proof-of-concept.** Quanta resists Sybil attacks via reputation-weighting,
  staking weight, and rate limiting — but there is no hardened proof-of-work/stake admission
  puzzle yet, and the gossip layer itself is not Sybil-gated. The eclipse heuristic only flags
  *lazy* attackers (peers sharing a pubkey prefix); real eclipse resistance needs IP/AS
  diversity and persistent anchor peers, which are roadmap.
- **Finality gadget is simulation-verified, not yet fully live.** A Casper-FFG-style
  finality gadget (validator votes, ⅔-stake justify/finalize, and an equivocation slashing
  rule) is implemented and proven in deterministic simulation. Finality-vote gossip is now
  wired live; finality-aware block proposal and live slashing execution are still being
  wired. Leader election also remains publicly predictable (no secret-key VRF yet). Until
  the gadget is fully live, treat deeper confirmations as *strong*, not *final*.
- **No real monetary value.** QUANTA is experimental and unpriced. Do not store value you
  cannot lose.

We document these because a protocol that hides its limitations cannot be trusted with the
things it does well.

---

## 10. Roadmap

External security audit · hardened multi-node chaos & partition tests · signed +
notarized release pipeline · sub-second BFT finality
([DAG-BFT design](docs/DESIGN-CONSENSUS-DAG-BFT.md) — a *consensus* DAG, unrelated to the
social-content DAG removed in the crypto-only refactor) · VDF-hardened leader randomness ·
network-wide *require-PQ* flag day · hardened anti-Sybil admission · on-chain governance of
economic parameters. UI is internationalized (EN · FR · ES · RU · ZH · JA).

---

## 11. Conclusion

Quanta is sovereign money: scarce by rule, mined by contribution, held by you alone, and
moved peer-to-peer with no authority in the middle. The hard cap and the decaying emission
are enforced in code and verified at consensus; the keys are yours; the network has no
owner. The engine is real and tested; the network is young and open. Everything is free
software (Apache-2.0) — the code is open, the rules live in the protocol, and future
governance will be on-chain.

<p align="center"><strong>◈ Quanta — Scarcity You Forge ◈</strong></p>
