# Quanta — Whitepaper

> **A sovereign P2P currency — no server, no cloud, no intermediary.**
> Protocol version `TORUS_PROTOCOL_VERSION = 6` · App v3.4 (crypto-only) · Coin **QUANTA** ·
> License Apache-2.0
> **Status: alpha, not third-party audited.** P2P verified between two physical machines
> (05/06/2026), not a proof at scale. No independent external security review to date.
> **QUANTA has no market and no price**; no monetary value is claimed or predicted anywhere.

---

## 1. Summary

Quanta is a peer-to-peer cryptocurrency built around a single idea: **remove the promiser**.
Where a fiat currency rests on the restraint of a central bank, and a platform rests on a
server that can be seized or frozen, Quanta rests on no one. There is no function to print, no
account to freeze, no company to summon, no issuance authority — not even the project's author.
These are not promises of good conduct: they are **absences** in the code, verified by every
node at every block.

Technically, Quanta combines five pieces that hold together:

- a P2P transport over **Iroh (QUIC)** with pub/sub **gossip**, hardened by a nine-step security
  pipeline and about fifteen network measures (NET-3 → NET-16);
- a **Proof-of-Stake** consensus with **deterministic and publicly verifiable** proposer
  election, weighted by an **on-chain enrolled stake** — never a local view;
- a **Casper-FFG-style finality gadget** that makes history mathematically irreversible after a
  **⅔-of-stake certificate**, with provable and live **slashing** of equivocation;
- a **post-quantum** cryptographic stack: account authority, finality votes, and network
  envelopes are signed with **ML-DSA-65 (FIPS 204)**, and the transport key exchange is the
  hybrid **X25519MLKEM768**;
- a currency **scarce by construction**: a hard cap of **100,000,000 QUANTA** carved into the
  code, zero premine, decreasing emission, and a 1% burn on every transfer.

Around these five pillars, the app keeps three simple promises for a holder: **mine** (earn
QUANTA by keeping the network), **hold** (an identity that is a key only you possess, reachable
by a short `@pseudo`), and **exchange** (transfer between wallets, ML-DSA-signed, with the
usual burn).

This document describes the architecture as it exists in the code (`src-tauri/src/`),
scrupulously distinguishing what is **real** from what remains on the **roadmap**. Every
"real" claim points to a function, a constant, or a test present in the repository. In keeping
with the project's doctrine: **QUANTA has no market and no price.** The value this text speaks
of is never a quotation — it is a set of guarantees that no third party can corrupt.

---

## 2. The problem — money without a guardian

The whole history of money is a series of promises kept by someone who had the power to break
them: the king who clips the coin, the central bank that prints, the platform that freezes the
account. Every time, trust rested on the restraint of whoever held power — and every time, that
power eventually got used.

Bitcoin demonstrated that a currency **without an issuer** was possible, but left two gaps.
First, only **probabilistic** finality — "wait for six confirmations," never a guarantee; a
reorg remains theoretically possible at any depth. Second, security backed by an **energy
race**: the structural sin of Proof-of-Work is that the more electricity the network burns, the
more it emits, in an endless escalation. Classic Proof-of-Stake schemes answer the first point
— they bring deterministic finality — but sign with cryptography (Ed25519, ECDSA, BLS) that a
quantum computer will break: the promise of irreversibility carries a quantum asterisk there.

Quanta sets out to close all four at once. The comparison, one line per family:

- **Fiat** — value depends on the central bank's restraint; Quanta removes the printing press
  (no minting outside the emission curve).
- **Banks** — value passes through the intermediary's permission; Quanta removes the freezable
  account and the seizable server (there is none).
- **Bitcoin** — security is computing power, finality a probability; Quanta removes the energy
  waste **and** the uncertainty (deterministic finality).
- **Classic PoS** — signatures are quantum-breakable; Quanta replaces them with ML-DSA
  end-to-end on both value and finality.

The mission therefore aims at a **network perfection** in service of one thing: a sound, scarce
and verifiable currency. Five objectives follow: a unified, versioned P2P protocol (Torus);
deterministic exchanges with guaranteed convergence and no data loss; a robust network
(reconnection, multi-peer discovery, NAT traversal); a production blockchain (stable PoS,
deterministic fork resolution, fast sync); and a verifiable currency (hard cap carved in,
zero premine, zero issuance authority).

---

## 3. Architecture & the Torus protocol

The backend is written in **Rust (Tauri 2.0, edition 2021)**, the frontend in **Svelte 5**. One
architectural principle dominates everything else: the consensus core lives in a
**deterministic IO-free** module (`sm/`), where the clock and the random generator are
**injected** at the boundary rather than read from the middle of the code. This discipline has
a precise purpose: to make consensus **exhaustively simulatable**, independent of the network
layer. A deterministic simulation test harness (DST) replays the protocol over many seeds,
injects network and byzantine faults, and checks invariants — including **C1**, the property
that two nodes starting from the same inputs produce **byte-identical** outputs (a meta-test
over 128 runs). The consensus verdict must never depend on message arrival order, wall clock,
or `HashMap` iteration order — hence the systematic use of ordered structures
(`BTreeMap`/`BTreeSet`) on decision paths.

### 3.1 The five layers

The Torus protocol is organized into five clean layers:

```
Application  ← Wallet, @pseudo identity, staking
Protocol     ← GossipMessage (Hello, RequestChain, ChainSegment, NewBlock,
               BroadcastTx, Ping/Pong, PublishUsername, ReportPeer, FinalityVote)
Security     ← GossipEnvelope (signature + monotonic nonce + timestamp)
Transport    ← Iroh QUIC + iroh-gossip pub/sub
Network      ← NAT traversal + relay + hole punching
```

The gossip protocol messages and their role:

- **`Hello`** — presence: carries `chain_height`, watts, country. Critical priority.
- **`RequestChain`** / **`ChainSegment`** — chain synchronization: request from a given height,
  paginated response (max 50 blocks per segment). Critical.
- **`NewBlock`** — block sealed by the PoS leader, broadcast. Critical.
- **`BroadcastTx`** — signed transaction (mining, transfer, burn). High priority.
- **`PublishUsername`** — `@pseudo` identity registration. Medium priority.
- **`Ping`** / **`Pong`** — liveness check. Low priority.
- **`ReportPeer`** — report of a malicious peer. Low priority.
- **`FinalityVote`** — finality vote of the Casper-FFG gadget (added by the LIVE-1 live wiring).
  A `FinalityFault` variant carries equivocation proofs.

### 3.2 Connection flow

A node joining the network introduces itself with a `Hello` carrying its `chain_height`. Peers
compare heights: the one further behind issues a `RequestChain`, the other replies with
`ChainSegment`. `Hello` is re-emitted every 120s, a light `Ping` every 15s ensures liveness, and
a cleanup removes dead peers after 5 minutes without a sign of life. Synchronization can be
parallel (4 windows of 50 blocks at a time) and segments can be gzip-compressed (with a 50 MB
decompression cap to prevent zip bombs).

### 3.3 Security pipeline (dispatch_incoming)

Every incoming message crosses nine steps before being processed — a failure at any one and the
message is rejected:

```
① Size guard (max 10 MB per envelope)
② JSON deserialize → GossipEnvelope
③ Ban check (per-peer)
④ Dedup (seen_messages, LRU 100,000)
⑤ Timestamp freshness (±90s)
⑥ Adaptive rate limit (base 30 msg/min/peer, sqrt-scaling, bounded [15, 120])
⑦ Anti-replay nonce (monotonic per sender)
⑧ Signature verification (ML-DSA-65 since the v4 hard-fork)
⑨ Dispatch to handler (including the FinalityVote branch)
```

Every envelope is thus signed, timestamped, carries a monotonic nonce, and is deduplicated by a
unique identifier (LRU of 100,000 entries). The rate limit is **adaptive**: the base of 30
messages per minute per peer is scaled by `sqrt(peers/4)`, bounded between 15 and 120, to absorb
topology spikes without opening the door to spam. Banning is social and temporary: three valid
reports ban a peer for one hour, with auto-expiry.

### 3.4 Network hardening (NET-3 → NET-16)

Around this pipeline, about fifteen measures round out robustness: a four-lane outbound priority
queue (Critical/High/Medium/Low), the already-mentioned parallel sync and compression, per-peer
metrics (RTT as an exponential moving average, inbound bytes and messages, a 0-100 quality score
combining latency, loss, and uptime), a two-hop topology view, an **anti-eclipse** heuristic
(alert if more than 80% of peers share the same public-key prefix over 8 hex digits), a mempool
with a 10-minute TTL capped at 1000 transactions, signed peer nicknames, and the
`TORUS_PROTOCOL_VERSION` field (today **6**) that flags and logs incompatible peers. Version 6
was reached through a documented series of breaking changes: the switch to a post-quantum
genesis (2→3), unbonding slashes (3→4), the v4 "clean genesis" hard-fork (4→5), then the native
MSIG-1 multisig (5→6).

---

## 4. Consensus — stake-weighted PoS + Casper-FFG finality

Quanta's consensus reads in two stages. An **election** stage decides who has the right to
propose the next block. A **finality** stage renders a proposed history *irreversible*. The
first holds liveness (the chain moves forward); the second holds safety (history does not
rewrite itself). Both are weighted by the **same** on-chain stake, and speak of the **same**
account identity — the ML-DSA address.

### 4.1 Proposer election

At each slot (equal to chain height), the proposer is elected **deterministically**:

```
beacon = BLAKE3(domain ‖ buried_block_hash ‖ slot)
seed   = BLAKE3(domain ‖ beacon ‖ slot ‖ round)
seed % total_weighted_stake → leader
```

The "buried" block is the one located `LEADER_ENTROPY_LOOKBACK = 2` slots behind the tip — not
the freshly sealed tip. A validator's weight is its **stake enrolled on the chain**
(`ledger.validator_stakes()`), derived from sealed `Stake`/`Unstake` transactions: it is a
**pure function of the chain**, identical on every node, whether live, restored from a
snapshot, or freshly synced. This is precisely what **closes the fork vector**: as long as
weight came from a local view (a reputation ranking specific to each node), two honest nodes
could elect different leaders at the same slot and diverge. By anchoring weight to ledger
state, the validator set becomes a **consensus object**.

Operational parameters: minimum stake `MIN_VALIDATOR_STAKE = 1,000,000 µQTA` (1 QUANTA, an
adjustable placeholder); if the designated leader fails to produce, a fallback moves to the
next in line after `LEADER_TIMEOUT_SECS = 30`s, up to `MAX_FALLBACK_ROUNDS = 3` rounds; as
long as no one has staked, election is permissionless (bootstrap).

**Honest naming.** This election is *deterministic and publicly verifiable* — it is **not** a
cryptographic VRF. Without a secret key, each slot's leader is **computable in advance by
anyone**, hence targetable (an adversary can DoS the leader right before its slot). The buried
beacon prevents immediate self-grinding — the sealer of the tip does not choose the next slot's
randomness — but long-horizon grinding remains theoretically open without a VDF. This is a
conscious trade-off (see ADR-004): liveness does not rest on the leader's secrecy but on the
**committee that finalizes by quorum** and on slashing. A real VRF (unpredictability) and a
VDF (anti-grinding) are on the roadmap, **not delivered**. The internal `vrf` identifiers are
legacy names kept for compatibility.

### 4.2 The full lifecycle of a block

Mining runs in `mining_loop.rs`: every 60s a mining tick (`MINE_INTERVAL_SECS = 60`), a seal
every 2 ticks (`SEAL_EVERY_N_TICKS = 2`), i.e. roughly one block every 2 minutes (~720
blocks/day). Here is the full journey of a block, from its proposal to its irreversibility:

```
① Election      → the slot's leader is determined (buried beacon, weight = on-chain stake)
② Seal          → the leader seals: selects covered tx (COVER-2), computes the
                  BLAKE3 Merkle root, produces a block valid by construction
③ Gossip        → NewBlock broadcast (critical priority queue, ML-DSA-signed)
④ Reception     → validate_block_against_prev: proposer bonded as-of-parent (PROPOSER-1)?
                  emission ≤ cap? spend/stake coverage (COVER-1)? Merkle?
⑤ Integration   → linear extension, duplicate, tie fork at equal height (tie-break), or reorg
⑥ Vote          → at the epoch boundary, validators sign an ML-DSA source→target Vote
⑦ Certificate   → once ⅔ of stake is reached (backing×3 ≥ total×2), the epoch is justified
⑧ Finality      → two consecutive justified links → finalize; the floor rises, irreversible
```

Step ④ closes a long-deferred CRITICAL. Until the v4 hard-fork (PROPOSER-1), the proposer was
only verified at *seal* time, on the producer side. It is now verified **on reception**: the
shared validator `validate_block_against_prev` — the same code on all four paths (linear
extension, fork tie-break, trial reorg clone, sync) — **rejects** any non-genesis block whose
proposer is not a validator bonded as-of-parent (stake ≥ minimum). The rule is untimed: "any
bonded validator," a superset of what sealing produces. This produce/receive symmetry
guarantees a node never accepts a block it could not have sealed itself, and avoids any fork
caused by clock drift (preserving C1).

### 4.3 The finality gadget (Casper-FFG)

Above election lives a **Casper-FFG-style gadget** (`sm/finality*.rs`) that makes history
irreversible — the property Bitcoin lacks. Written pure and deterministic, proven in DST
simulation, it breaks down into five blocks (GADGET-1 through 5):

```
Epoch = 32 blocks (EPOCH_LENGTH_BLOCKS)
├─ GADGET-1  checkpoint (height, hash) at every epoch boundary
├─ GADGET-2  ML-DSA-65 Vote (source→target) + ⅔-of-stake certificate
│            carved-in quorum: backing×3 ≥ total×2  (QUORUM_NUM/DEN = 2/3)
├─ GADGET-3  justify then finalize (two consecutive links) → FinalityState
├─ GADGET-4  accountable safety: detects double-vote + surround, non-repudiable
│            ML-DSA proof, slash (burned, full, window = unbonding)
└─ GADGET-5  stake-weighted LMD-GHOST fork-choice, anchored to finality
             (5A ghost_head/anchors; 5B reconciliation via reorg_to_fork)
```

The vote format deserves a note. Where many PoS schemes aggregate thousands of votes into a
single constant-size BLS signature, Quanta has **chosen not to aggregate**: every vote is a
separate ML-DSA signature. The reason (ADR-005) is twofold. First, a finality vote is
**ephemeral** — it only matters within the window where finality is decided, unlike a
transaction that must remain unforgeable for years; BLS therefore adds no extra security here,
only compactness. Second, **finalization by epoch** neutralizes the size cost: one certificate
per batch of blocks, not per block, amortized over dozens of blocks and prunable. The payoff is
clean: a **finality that is fully post-quantum, with no asterisk**, and **direct fault
attribution** (slashing is simpler on separate signatures than on an aggregate). A certificate
abstraction keeps the door open to future aggregation (BLS or PQ SNARK) should scale ever
demand it, as a local replacement.

Vote identity is the ML-DSA public key; the `validator_stakes_by_pubkey` bridge re-keys stake
**purely from the chain** (every `Stake` tx reveals its `pq_public_key`). Like stake, the vote
and a transaction's `from` share a **single** identity — the ML-DSA address — two nodes with
the same votes and the same chain finalize **identically**, with no correspondence table.

### 4.4 The live wiring (LIVE-1 → LIVE-4)

The gadget is not just a core proven in simulation: its votes **circulate live** thanks to the
four LIVE workstreams. None of these workstreams introduced a new consensus rule — they wired
the pure core onto the real network and ledger, crossing the IO-free boundary without breaking
it (the `sm/` core remains unchanged, C1 preserved, IO tested separately).

- **LIVE-1 — votes circulate.** A validator signs its `Vote` (ML-DSA) and gossips it
  (`GossipMessage::FinalityVote`); the dispatch branch (step ⑨) deserializes it, deduplicates
  it, validates it (`Vote::verify`) and hands it to the `FinalityTracker`
  (`p2p/finality_live.rs`). Casting happens at the mining tick. Received votes populate
  `LatestVotes` and the live ledger's finality state.
- **LIVE-2 — the finality floor.** A `finalized_floor_index` (monotonic, hash-verified,
  persisted at snapshot) is fed by ⅔ certificates. Fork resolution
  (`integrate_remote_block`) now **refuses** any fork that would replace a block at height ≤
  the floor. Finalized history becomes **irreversible on the live network**; free
  lexicographic tie-breaking only applies **above** the floor (Gasper's rule: free above,
  frozen at and below finality). This is a **pure** safety guard — refusing a reorg mutates no
  balance.
- **LIVE-3 / 3B — live slashing.** Equivocation detected at vote ingest (`detect_fault`:
  double-vote or surround) is gossiped as `FinalityFault`, then sealed as a `Slash`
  transaction in the next block. This tx's authority is the **embedded proof itself**,
  re-verified by every node (`verify_block_slashes`): a malicious proposer cannot punish an
  innocent, since it would need a real proof, the right offender address, and the ratified
  amount. The slash destroys the offender's stake **STAKE → BURN**, **conservation-neutral by
  construction**. Variant 3B closes the "unstake-and-run" loophole: the slashable base
  includes stake **currently unbonding** (Casper semantics — punishable as long as withdrawal
  is not complete), and the `Slash` tx carries its consumption breakdown (entries destroyed,
  deterministic order), tied to the hash and Merkle root, re-verified by every node against
  its own plan and restored exactly on reorg.
- **LIVE-4 — deep fork reconciliation.** The `ForkReconciler` (`p2p/fork_heal.rs`) buffers
  blocks that fail linear integration (bounded buffer of 1024, deterministic eviction),
  assembles the competing branch rooted at us, applies the **live victory rule** —
  longest-above-the-floor, with lexicographic tip tie-break at equal height — via
  `reorg_to_fork` on a fully-validated trial clone. This is the N-block generalization of the
  historical single-block rule: exactly one side adopts, both converge. Two partitions each
  sealing ≥2 blocks now converge live — the last convergence gap is closed.

---

## 5. Post-quantum cryptography

Quanta separates two cryptographic roles — **authority** (who can spend, stake, vote, claim an
identity) and **transport** (who talks to whom on the network) — and migrates each to
post-quantum where necessary. The primitives, by role:

- **Account authority** — **ML-DSA-65 (FIPS 204)**, pure. This is the signature that protects
  money for years.
- **Finality votes** — **ML-DSA-65**, never a classical primitive on the irreversibility path.
- **Gossip envelopes** — **ML-DSA-65** since the v4 hard-fork (PQ-ENVELOPE-1); the sender is
  the ML-DSA public key, the Ed25519/legacy fallback has been removed.
- **Transport key exchange** — hybrid **X25519MLKEM768** (ML-KEM-768 ⊕ X25519), negotiated by
  QUIC/TLS 1.3.
- **Symmetric encryption** — **AES-256-GCM** (vault, data at rest).
- **Key derivation** — **Argon2id** (password → vault key).
- **Hashing** — **BLAKE3** (addresses, Merkle root, election beacon, finality trees).

### 5.1 Account authority — pure ML-DSA, no asterisk

Since PQ-MIG-3B, `from` and `to` are **ML-DSA addresses** (`BLAKE3(ADDR_DOMAIN ‖ key)`)
**everywhere**: balance, mining reward, stake, `@pseudo`. This is not an indexing detail: it is
what makes the "fully post-quantum" promise **honest at the account level**. The internal
CRYPTO-ID-1 audit had shown the earlier version did not keep it — accounts were rooted in
Ed25519, and the ML-DSA key, self-declared per transaction and never bound to the account,
protected nothing: a quantum adversary breaking Ed25519 could forge the victim's signature,
attach its **own** ML-DSA key to it, and pass. The fix is **intrinsic**: `from` **is**
`BLAKE3(ADDR_DOMAIN ‖ key)`, so revealing a different key yields a different hash, unequal to
`from`, and verification (`lie(from, key)`) fails. No forged classical signature changes that
fact. Transaction verification (`verify_tx`) is purely ML-DSA; the Ed25519 co-factor has left
the authority path.

### 5.2 Transport — post-quantum hybrid

The rustls provider moved from `ring` to **`aws-lc-rs`** with the `prefer-post-quantum` option
(PQ-TRANSPORT-1): the QUIC/TLS 1.3 key exchange negotiates the hybrid **X25519MLKEM768**, for
an overhead of about 1 KB per handshake, once per connection. This is a
**"harvest-now-decrypt-later"** defense on transport confidentiality: an adversary recording
traffic today will not be able to decrypt it once a quantum computer arrives. The switch is
non-destructive — neither a protocol bump nor a genesis reset — because it plays out in TLS
negotiation with graceful degradation.

### 5.3 Native post-quantum multisig (MSIG-1)

Quanta offers **M-of-N** quorum custody entirely in ML-DSA (MSIG-1, protocol bump 5→6). The
address commits to its policy via `BLAKE3(MSIG_DOMAIN ‖ sorted keys ‖ threshold)`, is
recognized by a `pq_public_key == "msig1"` tag, and carries its authority as JSON in the
`pq_signature` field — which adds **no** new wire field (a single-key transaction stays
byte-identical to before). Verification (`verify_multisig`) requires at least *threshold*
distinct valid signers, and is rebind-proof (the address cannot be re-bound to another
policy). This is the project's **first post-quantum quorum custody**, working around the
absence of a threshold signature standard for ML-DSA.

### 5.4 The one honest debt — the Iroh NodeId

The network **node identifier** — Iroh's `NodeId` — remains **Ed25519**. This is an
**upstream** debt: Iroh is waiting on an industry consensus on post-quantum signing for
EndpointIds, which is outside our code. Concretely: a quantum adversary could impersonate a
node's *network identity* in real time, but **can neither forge a transaction or finality
vote** (protected by ML-DSA) **nor decrypt past traffic** (protected by X25519MLKEM768). The
switch is planned for the day Iroh ships a post-quantum EndpointId. Bottom line: money,
finality, and transport confidentiality are post-quantum; only node authentication remains,
outside our reach. All cryptographic secrets are otherwise wiped from memory (`zeroize()`).

---

## 6. Tokenomics

### 6.1 Scarcity proven, not promised

The cap is **hard: 100,000,000 QUANTA** (`MAX_SUPPLY_MICRO = 100_000_000 × MICRO`). It is not
written on a website: it is **verified at consensus** (`validate_block_emission`), which
rejects any block that would push supply beyond it — on the linear path as on reorgs, via a
shared validator. `1 QUANTA = 1,000,000 µQTA`; all accounting is in `u64`/`u128`, a `float`
never touches a balance (Rust rule #6). The cap, like the emission law, is a Rust constant —
substituted at compile time, with no runtime memory location: there is **no expressible
setter**. The door is not locked, it is **absent**. The only way to change these values would
be to edit the source and recompile — that is, to fork.

### 6.2 Decreasing, front-loaded emission

```
emission_for_tick(total_mined) = (MAX_SUPPLY_MICRO − total_mined) / EMISSION_DIVISOR
                                  EMISSION_DIVISOR = 50,000,000
```

This is a **pure function** of what has already been mined: every tick releases `1/50,000,000`
of the *remaining* supply. The decay is geometric and **front-loaded** — at genesis,
`emission_for_tick(0)` equals exactly 2,000,000 µQTA, i.e. **2 QUANTA per tick** (~120/h); then
a long tail approaches the cap **asymptotically, never reaching it** (integer division never
settles the last µQTA: `emission_for_tick(MAX_SUPPLY)` equals 0, tested). This profile rewards
early guardians without ever producing an avalanche of coins. The choice of a "flagship number"
cap (100M) is QUANTA's "21M moment" — a terminal scarcity provable like Bitcoin's, but via a
smooth curve rather than the halving cliffs that destabilize validator economics.

**Zero premine**: genesis allocates an empty table (tested: `Ledger::new() ==
genesis_with_allocation(&[])`). The **only** origin of coins anywhere in the code is
`mine_tx(from = "NETWORK", TxType::Mining)`, capped by `emission_for_tick`. No key, no founder
role can mint a µQTA outside the curve — not even the project's author.

### 6.3 Usage deflation

**1% of every transfer is destroyed** (`transfer_with_burn`, `amount / 100`, integer
arithmetic). Usage itself tightens supply. As the emission subsidy trends toward zero while the
burn persists with usage, QUANTA becomes **net-deflationary once volume exceeds the
(decreasing) subsidy** — scarcity then tracks real utility. Destroying (rather than reverting
to the validator) is also a security property: it makes the mechanism incentive-compatible and
resistant to collusion.

### 6.4 Conservation — a law, not a hope

```
Σ(spendable + staked + unbonding) + burned  ==  mined
```

This equality is carved in and tested on every path (Stake/Unstake/Slash/reorg). A coin is
neither created nor lost; it **moves between compartments**. A coin is born by mining (into
*spendable*), circulates from `@pseudo` to `@pseudo` burning 1% at every hop, locks up via
staking (*spendable* → *staked* → *unbonding* → *spendable*), and dies through usage burn or
slashing (*staked* → *burned*). Staking **moves** coins without burning them; unbonding is
height-indexed (`unlock = block.index + UNBONDING_PERIOD_BLOCKS`, i.e. **10,080 blocks**, about
2 weeks) and this duration is constrained, by a compile-time `const` assertion, to remain **at
least equal to the slashing window** — otherwise a cheater could dodge punishment by waiting
out unbonding.

### 6.5 Symmetric coverage (COVER-1 / COVER-2)

**Coverage** guarantees that no transaction spends or stakes more than the on-chain balance
allows, under a single rule that is a pure function of the chain (never of the mempool),
sequential, counting intra-block credits, exempting the synthetic addresses
`NETWORK`/`ESCROW`/`BURN`. It is **symmetric**:

- **COVER-1 (reception)** — `validate_block_against_prev` **rejects** any received block
  containing an uncovered spend or stake.
- **COVER-2 (production)** — `seal_block_at` **excludes** these transactions at production
  time (cache revert + eviction) to emit a block **valid by construction**.

The resulting invariant — every self-sealed block passes reception validation — closes off the
possibility of a node corrupting its own chain.

### 6.6 On-chain stake — staking, unstaking, unbonding

A validator's weight is no longer read from a local ranking but from an **on-chain stake
state** (ONCHAIN-STAKE-1), derived from sealed `Stake`/`Unstake` transactions, anchored to
`block.index`. The balance splits into three compartments — **spendable**, **staked**, and
**unbonding** — and conservation counts all three. An `Unstake` moves coins from the staked
compartment to a height-indexed unbonding period (10,080 blocks later), during which the coins
remain slashable (LIVE-3B). This is the second half of closing the fork vector mentioned in
§4.1: vote weight and slashable weight are two pure views of the same chain.

### 6.7 Distribution among nodes — Shapley

When several nodes contribute, a tick's emission is split by a Shapley value across four
dimensions (`p2p/shapley.rs`, weights sum to 1.0): **energy 0.30 / work 0.30 / validation
0.25 / uptime 0.15**. In solo mining — the only path tested live — 100% of the tick goes to the
miner without passing through Shapley.

> **Honest caveat (inherited from doctrine).** The distribution key still pays partly pro-rata
> to watts (`W_ENERGY = 0.30`), worsened by the fact that the "work" dimension is dead
> (`tasks_completed` wired to 0 since the removal of the web modules). Energy **still
> dominates** actual node-to-node sharing. This is a vestige, not an intention: doctrine carves
> in the direction of demoting energy to a mere anti-sybil signal (cost proves *presence*, it
> does not buy value), and replacing it with an egalitarian "Dividend of the Commons" —
> workstreams **not delivered**, outside the consensus security path. Note: this distribution
> layer never enters the *amount* emitted — `emission_for_tick` depends only on `total_mined`.

---

## 7. Identity

A holder's identity is a **key only they possess**; they are reached by a short, human
`@pseudo` (`p2p/username.rs`), registered via `PublishUsername`, ML-DSA-signed with `lie` (the
revealed public key is bound to the address, which closes off pseudo hijacking). No account, no
KYC, no file — a sovereign node reachable by a readable name.

Addresses use the **checksummed Bech32m `qta1…`** format, end-to-end since the node ecosystem,
derived from the ML-DSA key. The reference daemon `quanta-node` (headless) exposes a **17-method
JSON-RPC** set (getinfo, getbalance, getblock, validateaddress, getfinalityinfo, getvalidators,
getmempool, listtransactions with deposit scanning, sendrawtransaction, sendtoaddress,
getmultisigaddress…), in integer µQTA, with supply proven in `getinfo`. A standalone web
explorer and a read-only `--public` mode round out the set. These are precisely the building
blocks an exchange integration expects from a coin: address generation, balance lookup,
transaction construction and broadcast, deposit tracking, and a **clean** answer to "how many
confirmations before crediting?" — the finality floor, not a probability.

Recovery relies on a **24-word BIP39 phrase**: the funds secret is the **ML-DSA seed** (not
Ed25519). The identity vault encrypts Argon2id-derived keys under AES-256-GCM
(`security/pq_vault.rs`). On macOS, a random KEK is stored in the Keychain behind
`SecAccessControl(.BIOMETRY_CURRENT_SET)` — real Touch ID: the OS requires the fingerprint on
read and invalidates the item if fingerprints change; the password is never stored and remains
the fallback. An `UnlockGuard` anti-brute-force applies exponential backoff, shared between
password and biometrics.

The brand identity (`docs/brand/BRAND.md`) is "the ring and the quantum": a geometric Q in two
strokes — the ring (the Torus, the network) and the diagonal tail crossing the gap (the block
being sealed). A single color owns the brand, jewel teal; the app's theme is deliberately
white, never dark, in a clean, bank-grade aesthetic.

---

## 8. Economic doctrine — the Sovereign Commons

The economic constitution (`docs/economy/DOCTRINE.md`) holds a paradox that the code makes
hold: **absolute individual sovereignty** (your key is you; your share is untouchable) **and**
**the common good** (the currency belongs to no one, is kept by all, pays its guardians) — in
the same object. A currency that belongs to everyone *because it belongs to no one*, and that
makes each person the absolute master of their share.

### 8.1 Where value comes from, without market or price

**Never from production cost** — a coin that cost a thousand kilowatt-hours but could be
reinflated, reversed, or frozen would be worth zero. Attributing value to cost is a mistake
inherited from Bitcoin. QUANTA's value is the **set of guarantees no third party can
corrupt**, each re-verified by every node at every block: a **scarcity** no one can dilute, an
absolute **property** (no seizure authority), an irreversible **finality** that even a majority
cannot rewrite, a **permanence** that survives the post-quantum leap. What one holds is not an
institution's promise, but **the absence of an institution able to break it** — the exact
opposite of a fiat note, whose value depends entirely on the restraint of whoever holds the
printing press.

### 8.2 The energy doubt, settled

A founding intuition launched this doctrine: rewarding pro-rata to watts resembles Bitcoin's
"burn electricity to win." The code answers at the structural level: `emission_for_tick`
depends **only** on `total_mined` — watts enter nowhere in the *emitted amount*. Whether the
network burns 3 W or 3 MW, a tick releases exactly the same number of coins. PoW's sin ("the
more you burn, the more the network emits") is already **absent**; security comes from stake
and finality, not hashrate. What remains is the residue of the inter-node distribution key
(§6.7), flagged for correction — a vestige, not an intention.

### 8.3 The chosen soul — permanence by default, circulation by choice

The monetary crossroads — *hard gold* (coins are eternal) versus *living money* (coins must
circulate) — was not settled in favor of one camp, but **made choosable by the holder**, which
is the purest expression of the Sovereign Commons. By default, your coins are yours
indefinitely, without erosion. Optionally, you can entrust some to the "Current" that flows
them toward active guardians — your private gold sleeps undisturbed, only what you entrust
flows. No one imposes a monetary philosophy on you: you choose your own coins' philosophy.

### 8.4 The bold mechanisms — and their honest status

Five mechanisms were invented and then put to the test of code. They all live in the
**distribution layer**, today **outside the consensus security path** (the Shapley module
still carries a `#![allow(dead_code)]`, and the only path tested live — solo mining — pays
100% of the tick without even passing through Shapley). **None is in production; carving them
in is engineering yet to do, not an accomplished fact.** Their parameters are subject to
ratification by the founder. Faithful summary, with status:

- **The Dividend of the Commons** (*the achievable star*) — every emission tick splits into two
  pockets: *the commons* (a fraction φ split into **equal shares** among all proven-live
  guardians — a Raspberry Pi and a data center get the same base share) and *the merit* (the
  rest, by measured contribution). Money creation thus carries an equality clause,
  **anti-whale by construction** (the floor does not scale with power). *Status:* 🔵 cap, small
  workstream, breaks no invariant (we redistribute *who receives* the tick, we don't create a
  µQTA). Caveat: the presence count is a **local** view — hence a *social* fairness, bounded by
  the consensus cap, not a consensus guarantee until an on-chain-anchored `TxType::Presence`
  exists.
- **The Sealed Legacy** (*permanence made tangible*) — a "will as code," ML-DSA-signed,
  naming beneficiaries and a height-indexed trigger: a **time-lock** (unlocked at block X) or
  a **dead-man's switch** (if the key stops signing for N blocks, with a veto window).
  Sovereign self-custody stops having "if I die, everything is lost" as a fatal bug. *Status:*
  🔵 cap; the time-lock is clean and achievable, dormancy (inferring death from inactivity) is
  a heavy workstream never to be confused with a cryptographic proof.
- **The Current** (*circulation, strictly opt-in*) — a tiny fraction of **idle balances in a
  community pool** is levied (never burned) over epochs and redistributed to active nodes; gold
  kept outside the pool is never touched. This is Gesell/Wörgl carried by post-quantum
  cryptography. *Status:* 🔵 cap, heavy workstream (hard-fork); it lacks per-account activity
  tracking. Strict opt-in is what saves it: it never betrays default permanence.
- **The Seal's Pay** (*the polar star*) — the purest reversal: an epoch's emission is
  accumulated in escrow and minted **only when a ⅔ certificate finalizes** that epoch,
  distributed pro-rata to the stake that actually signed. No certificate, no mint. Money no
  longer arises from the passage of time but from the common good produced — irreversibility
  itself — and this *reinforces* the invariants (not-finalized = not-minted). *Status:* 🌟
  polar star, not the next step; an XL workstream (minting must move from "each node pays
  itself its tick" to "only a certificate authorizes payment").

A fifth mechanism considered — "The Dew," an anti-whale leveling via diminishing returns — was
dropped: the Dividend of the Commons subsumes it, more cheaply and less exposed to sybil.

---

## 9. Architecture decisions (ADR)

Consensus and security trade-offs are tracked in an ADR registry (`docs/decisions/`). The
principle: a decision is never guessed — it is framed (options + consequences) then settled.
Digest summary, one entry per decision.

**ADR-001 — Fork-choice.** *Problem:* the code could only tie-break a single-block fork at
equal height, by "highest hash" — grindable without stake, and unable to reconcile beyond a
one-block divergence. *Choice:* solve the point at its root via the finality gadget (a
finalized block is never reorganized) and, within the non-finalized window, replace hash
tie-breaking with a stake-weighted LMD-GHOST (`ghost_head`), plus a multi-block reorg
(`reorg_to_fork`) bounded by the floor. *Why:* a Sybil-resistant semantics aligned with PoS,
and guaranteed convergence up to finality depth.

**ADR-002 — Validator set & BFT committee.** *Problem:* each node built the validator set from
its **local** reputation; nothing guaranteed two nodes computing the same set, hence different
leaders at the same slot, hence a fork. *Choice:* anchor eligibility and weight to **on-chain
stake alone**, derived deterministically at the epoch boundary; reputation leaves the security
path (it remains an application-level signal for mining/Shapley). *Why:* the validator set
becomes a verifiable, Sybil-anchored consensus object; it is the prerequisite for slashing (only
on-chain stake gets slashed).

**ADR-003 — Slashing (accountable safety).** *Problem:* a leader could equivocate (seal two
blocks at the same slot) **at no cost**; the only sanction was a social network ban,
manipulable. *Choice:* a `TxType::Slash` whose authority is a **proof** of equivocation
(double-vote or surround) re-verified by every node, with a policy carved in by ADR-009: stake
**burned** (not redistributed), **full** amount, proof window **equal to unbonding**. *Why:* a
real deterrent and provable *accountable safety*, essential for BFT; burning strengthens
scarcity and simplifies accounting.

**ADR-004 — Election randomness (beacon vs ECVRF+VDF).** *Problem:* the buried beacon makes
the leader **publicly predictable** (targetable) and leaves long-horizon grinding open.
*Choice:* **keep the buried beacon** for this phase, accepting the trade-off. *Why:* liveness
is carried by the committee (quorum) and slashing, not leader secrecy; a real VRF
(unpredictability) and a VDF (anti-grinding) would introduce either a non-PQ primitive or
calibration complexity — they remain on the roadmap, not delivered.

**ADR-005 — Vote aggregation & finality certificates.** *Problem:* the vote format seemed to
force a choice between BLS (compact but not post-quantum) and ML-DSA (post-quantum but not
aggregatable, ~3.3 KB per vote). *Choice:* **pure ML-DSA, epoch-based finalization**, behind a
certificate abstraction. *Why:* a vote is ephemeral (BLS adds no long-term security here), and
epoch-based finalization amortizes size (one certificate per batch of blocks, prunable); this
yields a PQ finality with no asterisk and direct fault attribution.

**ADR-006 — Governance & evolvability.** *Problem:* how does the protocol change once the
network is live, without opening a governance attack surface or sliding into plutocracy?
*Choice:* **no on-chain governance**; a monetary core (cap, emission, PQ signatures,
conservation, safety) **immutable by construction** — not locked, but doorless: no code path
changes it — and an adjustable periphery via **voluntary fork**. *Why:* an invariant is
protected not by making it hard to modify, but by giving it no modification path; an absent
door cannot be picked. No dormant governance mechanism is left in the code.

**ADR-007 — Post-quantum scope (ML-DSA accounts).** *Problem:* the CRYPTO-ID-1 audit proved
that "fully post-quantum" was not kept — accounts were rooted in Ed25519, the ML-DSA key
unbound. *Choice:* re-root accounts **entirely in ML-DSA** (option b), at the cost of the
project's biggest workstream, rather than a lightweight registry leaving a permanent asterisk
(option a). *Why:* for a currency whose reason for being is post-quantum, the asterisk is not
a compromise but a renunciation, on exactly the project's differentiator; a transaction
signature has long life ("harvest today, forge tomorrow").

**ADR-008 — Tx authority via on-chain ML-DSA binding.** *Problem:* the account identifier is
**unified** (the same `from` serves as the balance key, mining target, stake, and `@pseudo`
binding), so migrating it seemed to fatally re-key stake and mining in the same move. *Choice:*
first a binding registry keeping `from` as Ed25519 (option a), then **reversed** — `from`/`to`
become the ML-DSA address everywhere. *Why:* unification, first read as a blocker, is actually
the justification for (b): "everything moves together, on purpose"; key↔account binding stops
being a state (registry) and becomes a stateless function, closing CRYPTO-ID-1 intrinsically.

**ADR-009 — Carved-in/adjustable boundary and §12 values.** *Problem:* ADR-006 set the
principle but left the exact boundary and values (epoch length, quorum, minimum stake…) open.
*Choice:* ratify the existing constants **without changing any of them** — carved in for the
monetary parts (100M, emission, 1% burn, µQTA, zero premine) and safety invariants (⅔ quorum,
slash burned, window ≤ unbonding); adjustable by fork for operational settings (E = 32,
unbonding 10,080, slash fraction, `MIN_VALIDATOR_STAKE`). *Why:* naming the boundary makes
ADR-006 operational; freezing operational settings too would cost a fork for the slightest
cadence adjustment, with no safety benefit. The monetary scale (and thus the sensible value of
`MIN_VALIDATOR_STAKE`) explicitly remains a founder decision.

---

## 10. Limits, status, honest roadmap

This whitepaper is only worth something if it separates the real from the aspirational.

**Real today, verifiable by reading and testing** — the live PoS consensus + Casper-FFG
finality (LIVE-1→4), post-quantum cryptography for money, finality, and transport, the hard
cap and decreasing emission verified at consensus, conservation and symmetric coverage, PQ
multisig, the node ecosystem (daemon + 17 RPC methods + explorer). All covered by an
**exhaustive test suite** (unit, integration, multi-seed deterministic simulation — including a
real 2-node gossip exchange and the C1 determinism invariant over 128 runs), clean clippy; the
exact count and their execution live in the public repo.

**What remains — without dressing it up:**

- **Election = deterministic, not VRF.** The leader is publicly predictable; a real VRF
  (unpredictability) + a VDF (anti-grinding) are on the roadmap, not delivered.
- **Iroh NodeId = Ed25519.** The only remaining classical primitive, an upstream debt outside
  our code; switches the day Iroh ships a post-quantum EndpointId.
- **Energy residue.** `W_ENERGY = 0.30` still weighs materially in inter-node sharing; the fix
  (demoting energy to an anti-sybil signal, delivering the Dividend of the Commons) is the
  direction, not the state.
- **Distribution layer outside consensus.** Doctrine's mechanisms (Dividend, Legacy, Current,
  Seal's Pay) are undelivered caps; carving them in is engineering yet to do.
- **Not audited.** None of these guarantees has received an independent external security
  review. The "2 physical machines" verification is real but is not a proof at scale. A
  security audit by a recognized third party is a near-mandatory prerequisite for any listing,
  not something code alone can bypass.
- **Listing readiness — partial.** The economics (provable supply, zero premine,
  deterministic finality, "utility" profile) is an asset; the technical side (daemon + RPC +
  explorer) is underway; but audit, legal, and liquidity require third parties and adoption,
  beyond the reach of a document or a single person coding. This is not legal advice.
- **No price.** QUANTA has no market; nothing here claims or predicts that a QUANTA will be
  worth anything in exchange.

---

## 11. Internal references (code)

All verifiable in `src-tauri/src/`:

- Emission & scarcity: `p2p/reputation.rs` (`MAX_SUPPLY_MICRO`, `EMISSION_DIVISOR =
  50_000_000`, `emission_for_tick`), `p2p/ledger.rs` (`validate_block_emission`, `mine_tx`,
  `transfer_with_burn`, coverage, finality floor, slash), `p2p/ledger_types.rs`
  (`MICRO = 1_000_000`).
- PoS consensus: `p2p/pos_consensus.rs` (election, `MIN_VALIDATOR_STAKE`,
  `LEADER_TIMEOUT_SECS`, `MAX_FALLBACK_ROUNDS`, `LEADER_ENTROPY_LOOKBACK = 2`).
- Finality gadget (Casper-FFG): `sm/finality.rs` (`EPOCH_LENGTH_BLOCKS = 32`),
  `finality_vote.rs` (⅔ quorum, `meets_supermajority`), `finality_rule.rs`,
  `finality_slashing.rs`, `fork_choice.rs` (LMD-GHOST); DST harness `sm/sim.rs`.
- Live wiring: `p2p/finality_live.rs` (LIVE-1→3B), `p2p/fork_heal.rs` (LIVE-4).
- Cryptography: `security/mod.rs`, `security/hybrid_crypto.rs` (ML-DSA-65),
  `security/pq_vault.rs` (Argon2id + AES-256-GCM), `security/cipher.rs`.
- Network: `p2p/gossip.rs` (`TORUS_PROTOCOL_VERSION = 6`), `p2p/dispatcher.rs`,
  `p2p/willow_node.rs` (Iroh QUIC), `p2p/gossip_tasks.rs`, `p2p/mining_loop.rs`
  (`MINE_INTERVAL_SECS = 60`, `SEAL_EVERY_N_TICKS = 2`).
- Distribution: `p2p/shapley.rs` (weights energy 0.30 / work 0.30 / validation 0.25 /
  uptime 0.15).
- Identity & node: `p2p/username.rs`, `security/mod.rs` (`qta1…` addresses), the
  `quanta-node` daemon and its JSON-RPC.
- Decisions: `docs/decisions/ADR-001…009`. Doctrine & brand: `docs/economy/DOCTRINE.md`,
  `docs/brand/BRAND.md`.

The concepts cited — Casper-FFG, LMD-GHOST/Gasper, FIPS 204 (ML-DSA), ML-KEM, Bech32m, BIP39,
Argon2id, BLAKE3 — are real public standards or constructions; this document invents no
academic reference or URL.

---

*Living document. "Real" claims are verifiable in `src-tauri/src/` as of the date of writing
(`TORUS_PROTOCOL_VERSION = 6`, covered by an exhaustive test suite whose exact count lives in
the public repo). The roadmap describes intentions,
not deliveries. Project status: alpha, not third-party audited. QUANTA has no market and no
price.*
