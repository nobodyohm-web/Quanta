# Quanta — The Sovereign Peer-to-Peer Web

> **Whitepaper — Quanta v3.3**
> A protocol where creating, discovering, and moderating earns QUANTA.
> No server. No hidden algorithm. No censor.

> **Implementation status.** Quanta is alpha research software. This document
> describes the protocol as designed and largely implemented; for the precise
> "real vs. experimental vs. not-yet" breakdown, see the status table in the
> [README](README.md). Nothing here should be read as a promise of present-day
> production security or monetary value.

---

## 1. Why Quanta

Today's web is an attention oligopoly:

| Problem | Effect |
|---|---|
| A handful of platforms capture most traffic | Unilateral censorship, arbitrary demonetization |
| Opaque ranking algorithms | Nobody knows *why* content surfaces |
| Centralized hosting | If one cloud falls, half the web falls |
| Opaque moderation | A deleted account = lost work, no recourse |
| Creators pay (hosting) or sell their audience | Little value captured by the creator |

**Quanta inverts the contract:**

- Hosting is mutualized (P2P, BLAKE3 content-addressing).
- Ranking is public (the **QuantaRank** algorithm is open-source; parameters live on the ledger).
- Moderation is performed by a randomly drawn jury (Kleros-style, but pure P2P).
- Every honest interaction (publishing, liking, subscribing, moderating) **earns QUANTA**.

Nobody owns Quanta. There is no company, no server to subpoena, no admin key.

---

## 2. The QUANTA coin — invariants

| Parameter | Value |
|---|---|
| Emission | **100 QUANTA / hour**, fixed, in perpetuity |
| Halving | **None** — nominal inflation tends to zero asymptotically as supply grows |
| Distribution (Shapley) | energy · compute work · validation · uptime · social utility |
| Burn | 1% per transfer · fees on compute tasks · fees on boosts · 10% moderation slashing |
| Unit | 1 QUANTA = 1,000,000 µQTA (`u64`, deterministic integer math) |

**Why 100/h fixed:** a coin with halving favours early adopters over newcomers. A coin
with fixed nominal inflation and variable burn converges toward a *real* emission of zero
as usage grows — without privileging anyone. All balances are `u64` µQTA; there is no
floating point anywhere in the ledger, so there is no drift.

---

## 3. Architecture

Quanta ships as a Tauri 2 desktop application. The Svelte 5 frontend talks to a Rust
(`tokio`) backend over IPC; the backend runs a full network node.

```
        Svelte 5 frontend  ⇄(IPC)⇄  Rust backend (tokio)
                                         │  Iroh QUIC + iroh-gossip (signed pub/sub)
        ┌────────────────────────────────┼────────────────────────────────┐
        ▼                                ▼                                ▼
   Peer A                           Peer B                           Peer C
   Ledger · DAG · Index · Pages     …                                …
        └──────────── eventual convergence: CRDT + deterministic chain ────┘
```

The protocol is layered: **Application** (sites, wallet, social, forums) →
**Protocol** (`GossipMessage`, 22 variants) → **Security** (`GossipEnvelope`) →
**Transport** (Iroh QUIC + gossip) → **Network** (NAT traversal + relay).

---

## 4. Wire protocol & security

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

## 5. Consensus — Proof-of-Stake with VRF

Block production is leader-based and deterministic per slot (= chain height):

```
seed   = BLAKE3(prev_block_hash || slot)
leader = seed % total_weighted_stake          (weight = stake + reputation·10_000)
```

Minimum validator stake is 1 QUANTA. If the elected leader fails to seal within a 30 s
timeout, production falls back to the next-in-line (bounded rounds). When nobody has
staked, bootstrap is permissionless. Forks resolve deterministically: the losing tip is
reverted from the balance cache, transactions exclusive to the losing branch are
re-queued, and the winning branch is applied — no validated block is ever silently lost.

---

## 6. The web layer

- **PageBuilder** — a no-code, block-based editor (headings, text, image, gallery, video,
  columns, cards, hero, FAQ, callout, navbar, footer, embed…), themes and templates,
  zero external dependencies. User content is rendered in a sandboxed iframe; per-site
  JavaScript is opt-in and off by default.
- **Publishing** — a page is content-addressed (BLAKE3) and broadcast to peers; it stays
  available as long as at least one peer replicates it. Pinning rewards the host.
- **Domains** — `*.torus` names use a Harberger tax: the owner *declares* a value, pays a
  monthly rent of 1% of it, and anyone may buy the name at the declared value. This keeps
  names in productive use and prevents squatting. Subdomains can be delegated arbitrarily deep.
- **Search** — a BM25 index ranked by **QuantaRank**, which blends text relevance with
  social signals (quadratic likes, follows, trust). Smart tags are auto-extracted and
  boost matches.

---

## 7. Social & moderation

- **Likes are quadratic.** A like costs at least 0.1 QUANTA; influence = √(QTA spent).
  Spreading 1 QTA across 100 likes beats 100 QTA on a single like — this forces diversity
  and makes vote-buying expensive.
- **Follows, tips, boosts** flow value directly to creators, each as a signed transaction.
- **Moderation is a jury.** Reports accumulate; past a threshold, a VRF draws a jury that
  votes in a commit-reveal scheme. Dishonest jurors and bad actors are **slashed**.
- **Web-of-Trust.** Each user computes a *personalized* PageRank from themselves
  (damping 0.85). External like-farms cannot touch you unless you follow into them.

---

## 8. Cryptography

| Layer | Mechanism |
|---|---|
| Identity / signatures | **Hybrid Ed25519 + ML-DSA-65** (NIST FIPS 204), active on the transaction layer |
| Key derivation | Argon2id (64 MiB, 3 iterations, parallelism 4) |
| Encryption at rest | AES-256-GCM (unique 12-byte nonce per operation) |
| Hashing / content-addressing | BLAKE3 |
| Memory safety | `zeroize` + `ZeroizeOnDrop` on every secret |

**Post-quantum — active (hybrid).** Every **transaction** is signed with a hybrid
**Ed25519 + ML-DSA-65** signature (NIST FIPS 204) via the standalone `fips204` crate
(pure Rust, constant-time, no `unsafe`). The ML-DSA key is **derived from the Ed25519 seed**
(BLAKE3 XOF), so no extra secret is persisted and no vault migration is needed. Verification
is **strict AND** when a PQ layer is present — forging requires breaking *both* schemes —
with an Ed25519 fallback for pre-activation signatures. Gossip envelopes remain Ed25519
(ephemeral transport, ±90 s freshness window, already inside QUIC/TLS); a network-wide
*require-PQ* flag day (`REQUIRE_PQ`) is a future protocol bump.

---

## 9. Threat model & honest limitations

- **Not third-party audited.** The cryptography and networking have had no independent audit.
- **Alpha-scale network.** Convergence was verified between two physical machines, not at scale.
  Large-scale NAT traversal, partition resilience, and eclipse resistance are works in progress.
- **Anti-Sybil is a proof-of-concept.** Quanta resists Sybil attacks via reputation-weighting,
  quadratic costs, personalized trust, rate limiting, and an eclipse warning — but there is no
  hardened proof-of-work/stake admission puzzle yet.
- **No real monetary value.** QUANTA is experimental. Do not store value you cannot lose.

We document these because a protocol that hides its limitations cannot be trusted with the
things it does well.

---

## 10. Roadmap

Fuzzing the envelope parser · hardened multi-node chaos & partition tests · signed +
notarized release pipeline · UI internationalization (FR/EN) · sub-second BFT finality
([DAG-BFT design](docs/DESIGN-CONSENSUS-DAG-BFT.md)) · VDF-hardened leader randomness ·
network-wide *require-PQ* flag day · on-chain governance of ranking/economic parameters.
The prioritized roadmap lives in [`audit/Torus-Audit-360.html`](audit/Torus-Audit-360.html).

---

## 11. Conclusion

Quanta is not a decentralized Twitter, nor a free Google, nor a P2P WordPress. It is a
single coherent protocol where the act of building the web — and curating it honestly —
is the act that mints its money. The engine is real and tested; the network is young and
open. Both are free software (Apache-2.0): the code is open, the parameters live on the
ledger, and future governance will be on-chain.

<p align="center"><strong>◈ Quanta — Energy Is Value ◈</strong></p>
