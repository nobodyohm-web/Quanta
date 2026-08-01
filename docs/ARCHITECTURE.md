# Quanta — Architecture & Engineering Notes

*A guided tour of the protocol, written to be read start to finish.*

This document exists because a repository of 36 000 lines of Rust does not explain itself.
It is written for two readers who want the same thing from opposite directions: an **auditor**
who needs to know what is claimed and where each claim is enforced, and an **engineer**
who wants to know whether the design holds up. Both get the same answer here, because
if a design cannot be explained plainly it usually cannot be defended either.

Every number below is read from the code at `TORUS_PROTOCOL_VERSION = 9` (v3.15.1).
Nothing is rounded up to sound better. Where something is unfinished it says so.

**Contents**

1. [The problem](#1-the-problem)
2. [The shape of the code](#2-the-shape-of-the-code)
3. [Life of a transaction](#3-life-of-a-transaction)
4. [Life of a block](#4-life-of-a-block)
5. [The invariants, and where each is enforced](#5-the-invariants-and-where-each-is-enforced)
6. [Testing what a unit test cannot reach](#6-testing-what-a-unit-test-cannot-reach)
7. [Three bugs that shaped the design](#7-three-bugs-that-shaped-the-design)
8. [What is deliberately not done](#8-what-is-deliberately-not-done)
9. [Where to start reading](#9-where-to-start-reading)

---

## 1. The problem

Quanta is a peer-to-peer currency with no server, no company and no issuing authority.
That sentence sets every constraint that follows.

**No server** means there is nobody to ask what the truth is. Two nodes that disagree
about history must resolve it from the chain alone, with no tiebreaker to call. Any
value derived from *local* state — a wallet's own view, a peer's self-reported hardware,
a wall clock — is by definition not shared, and therefore cannot be allowed to decide
anything the network must agree on. This single rule kills more designs than any other,
and §7 shows what happens when it is violated.

**No issuing authority** means the money supply is a function, not a decision. The cap
is 100 000 000 QUANTA, enforced at consensus and not merely in the UI. There is no premine
and no mint key. If the emission rule can be argued with, it is not a rule.

**Post-quantum by default** is the third constraint, and it is not decoration. A currency
is a long-lived commitment: coins signed today must still be attributable in twenty years.
Account authority and finality votes are ML-DSA-65 (FIPS 204) with no classical fallback —
a fallback is just the weaker algorithm with extra steps. This has a real cost, paid in
bytes: an ML-DSA-65 public key is 1 952 B and a signature 3 309 B, so every gossip envelope
carries ~10 KB of authentication before any payload. §7 tells the story of the day that
cost went unpaid.

---

## 2. The shape of the code

The layout follows one rule: **the code that decides is separated from the code that talks.**

```
src-tauri/src/
├── sm/          8 507 lines   the deterministic, IO-free core — decides
├── p2p/        21 600 lines   the live network — talks, then asks the core
├── security/    2 081 lines   ML-DSA-65, vault, symmetric primitives
├── commands/    1 349 lines   Tauri command surface, by domain
├── views.rs                   pure view-models shared by the app and the JSON-RPC
└── storage/       132 lines   libSQL persistence

src/             9 551 lines   Svelte 5 desktop UI (out of consensus scope)
```

### `sm/` — the part that must never surprise you

`sm/` is a state machine with no IO at all. It cannot open a socket, read a clock or draw
a random number; the clock and the RNG are **injected** (`sm/clock.rs`, `sm/rng.rs`). Its
whole contract is `Node::handle(Event) -> Vec<Effect>`: events go in, effects come out, and
nothing else happens.

This is the sans-IO pattern, and the reason for it is testability of the only kind that
matters here. Consensus bugs are not the kind you find by reading. They live in the
interleaving of network delays, restarts and malice — conditions you cannot reach with a
unit test and cannot reproduce from a bug report. Making the core a pure function of its
inputs means a failing schedule can be **replayed byte for byte** from a seed. §6 covers
the harness built on top of that property.

Living here: the finality gadget (`finality.rs`, `finality_vote.rs`, `finality_rule.rs`,
`finality_slashing.rs`), fork choice (`fork_choice.rs`), and the simulator (`sim.rs`).

### `p2p/` — the part that faces a hostile world

Everything with a socket or a lock. The ledger (`ledger/`, a six-file module: state,
validation, stake, slash, reorg, tests), the gossip protocol and its dispatcher, the PoS
election, the mining loop, the live wiring of the finality gadget (`finality_live.rs`) and
deep fork reconciliation (`fork_heal.rs`).

The division of labour is deliberate: **`p2p/` may decide when to ask, never what the answer
is.** When the two must agree — the reward split, the slash accounting — a single function
produces the plan and both the producer and every verifier call it. That pattern
(`expected_block_rewards`, `expected_slash_consumption`) appears repeatedly below, and it is
the main structural defence against the class of bug where "what I build" and "what you check"
drift apart by one commit.

The whole backend contains **one `unsafe` block** (`guardian.rs:34`, an AppKit cast to read
window occlusion state). Nothing on the money path is unsafe.

---

## 3. Life of a transaction

Follow one transfer of 10 QUANTA from Alice to Bob.

**1. Typed.** Alice enters `@bob` or a `qta1…` address. Addresses are Bech32m with a
checksum, so a typo is rejected locally rather than becoming an unrecoverable send.
Internally an address is `BLAKE3(ADDR_DOMAIN ‖ ML-DSA public key)`.

**2. Signed.** The wallet unlocks the vault (Argon2id 64 MiB / 3 iterations / p=4 →
AES-256-GCM) and signs with ML-DSA-65. Authority is *pure* ML-DSA: `verify_tx`
(`ledger/validation.rs:41`) checks that the signing key hashes to the `from` address. There is
no classical co-factor left on this path. Secrets are `zeroize`d after use.

**3. Split.** A transfer burns 1 %. That burn is a **second transaction**, and both are
broadcast — a lesson from AUDIT-TX-2, where the burn was applied locally but never gossiped,
so every remote peer disagreed with the sender about the balance by exactly 1 %.

**4. Gossiped.** Each transaction is wrapped in a `GossipEnvelope` signed with ML-DSA-65 and
published to the shared topic over Iroh QUIC, whose TLS 1.3 handshake negotiates hybrid
`X25519MLKEM768`.

**5. Received.** On the far side, `dispatch_incoming` (`p2p/dispatcher.rs:422`) runs a fixed
pipeline. The order is the security property:

```
① size ≤ 10 MB          ⑥ timestamp freshness ±90 s
② deserialize           ⑦ ML-DSA-65 signature check
③ peer ban check        ⑧ dedup INSERT  ← after authentication
④ canonical envelope id ⑨ adaptive rate-limit + monotonic nonce
⑤ dedup PROBE (read)    ⑩ dispatch to handler
```

Steps ④/⑤/⑧ look like plumbing and are not. Before v7 the dedup insert happened at ④, before
the signature check at ⑦. Any unauthenticated stranger could therefore pre-insert chosen
envelope ids into the LRU and make a peer silently drop the real messages that would later
carry those ids — free, untraceable censorship of chain sync. The fix has two halves: the
envelope id must *equal* `BLAKE3(signed pre-image)` so it can no longer be chosen freely, and
insertion moved after the signature check so an unauthenticated party cannot write to the
cache at all.

**6. Admitted.** The transaction enters the mempool (TTL 10 min, cap 1000). Admission never
seals a block — that separation is load-bearing, see §7.

---

## 4. Life of a block

**1. Who may propose.** The slot is the chain height. A beacon is derived from a *buried*
block (several slots behind the tip) so the current proposer cannot grind the seed by
choosing their own block's contents:

```
beacon = BLAKE3(domain ‖ buried_block_hash ‖ slot)
seed   = BLAKE3(domain ‖ beacon ‖ slot ‖ round)
seed % total_weighted_stake  →  leader
```

Weight is the **on-chain** stake — derived from sealed `Stake`/`Unstake` transactions, so it
is a pure function of the chain and identical on a live, restored or freshly synced node.
Reputation and energy readings are explicitly *not* in this path.

**Honest naming:** this is a deterministic, publicly verifiable election. It is **not** a
cryptographic VRF. No secret key enters the draw, so the leader of a future slot is publicly
predictable. The buried beacon only prevents immediate grinding. A real VRF (unpredictability)
plus a VDF (grinding resistance) are open work, tracked in ADR-004.

**One block in 16 is an open slot** (`OPEN_SLOT_EVERY_BLOCKS = 16`), proposable by any address,
bonded or not. Without it the network closes permanently the moment the first person stakes:
with no faucet, airdrop or premine, a newcomer needs stake to propose and needs to propose to
earn. The price of the open door is named and bounded: the Sybil attack vulnerability trilemma
proves that permissionlessness, Sybil resistance and freeness cannot all hold at once,[^1] so
exactly one sixteenth of emission relaxes "free", and it is scheduled *by height* — a farm of a
million fake identities captures no more than a single honest newcomer would.

[^1]: Platt, M., Platt, D. & McBurney, P., "Sybil attack vulnerability trilemma",
*International Journal of Parallel, Emergent and Distributed Systems*, 2024.
[doi:10.1080/17445760.2024.2352740](https://doi.org/10.1080/17445760.2024.2352740)

**2. The reward is computed, not chosen.** The block reward is
`emission_for_block(supply mined before this block)` — a pure function of the chain. The
producer mints it; **every receiver recomputes it**. This replaced a loose upper bound that
allowed `32 × N` times the honest amount (§7).

That reward is then **shared**: half to the producer, half split equally among *recent
participants* — the distinct addresses that produced a block within the last
`SHARE_WINDOW_BLOCKS = 32`. Integer division remainder goes to the producer, so conservation
is exact to the µQTA. No new wire field was needed: participation is already proven by
`block.miner`, which is bound into the block hash.

The split is **imposed, not suggested**. Each node recomputes the plan
(`validate_block_reward_plan`, `ledger/validation.rs:350`) and rejects a producer that keeps
everything or pays someone outside the plan. The plan is *scale-invariant*: minting less than
canonical stays legal, but you cannot shrink someone else's share. A share applied only by the
reference client is not a rule, it is a courtesy — and courtesies are not consensus.

Rewarding *recent production* rather than bonded stake makes this a liveness premium: a
validator that goes offline falls out of the window and stops being paid. Weighting by stake
would have recreated a capital rent, which the project's economic doctrine rejects.

**3. Sealed.** `seal_block_at` excludes any transaction not covered by the sender's on-chain
balance before the block (`uncovered_tx_indices`, `ledger/validation.rs:899`), so a produced
block is **valid by construction**. Coverage is sequential and counts intra-block credits;
synthetic senders (`NETWORK`, `ESCROW`, `BURN`) are exempt, and `NETWORK` is confined to the
single coinbase.

**4. Validated on arrival.** `validate_block_against_prev` (`ledger/validation.rs:602`) is the
**shared** validator for every path: linear integration, fork tie-break, reorg trial clone and
chain sync. One function, four callers — the alternative, a second validator for the reorg
path, is how a chain ends up accepting on one path what it rejects on another.

**5. Finalized.** Above the election sits a Casper-FFG finality gadget. Epochs of 32 blocks,
ML-DSA-signed votes (source → target), a certificate at **⅔ of stake** (`backing × 3 ≥ total × 2`),
then justify-then-finalize. Equivocation — double vote or surround vote — is detected, proven
with a non-repudiable ML-DSA proof, and slashed: the offender's stake is destroyed STAKE→BURN,
which is conservation-neutral because stake and burn are two compartments of the same identity.

Slashing reaches stake that is *unbonding*, not just bonded. Otherwise "unstake and run" is a
free equivocation: misbehave, withdraw, wait out the window. The `Slash` transaction carries
its own consumption breakdown, bound into the hash and the Merkle root, and every node
re-verifies it against its **own** plan (`expected_slash_consumption`, `ledger/slash.rs:39`).

Once finalized, history is anchored by a persisted, monotonic, hash-verified
`finalized_floor_index`. `integrate_remote_block` refuses any fork that would replace a block
at or below the floor. Above the floor, ordinary fork choice applies; below it, nothing does.

**6. Partitions heal.** When two partitions each sealed several blocks, `fork_heal.rs` assembles
the competing branch from a bounded buffer, picks the winner (longest above the floor, then
lexicographic tie-break on the tip hash), and applies it through `reorg_to_fork` after full
validation on a trial clone. Exactly one side adopts, so convergence is symmetric.

---

## 5. The invariants, and where each is enforced

The table an auditor should start from. Each row is a property the system claims, the single
place it is enforced, and the test that would fail if it stopped holding.

| # | Invariant | Enforced at | Proven by |
|---|---|---|---|
| **CONSERVE** | `Σ(spendable + staked + unbonding) + burned == minted` | accounting is by construction; checked every simulation step | `sm/sim.rs::check_invariants`, `t0_8_conservation_under_burn` |
| **CAP** | mined supply never exceeds 100 000 000 QUANTA | `ledger/validation.rs` emission check | `emit_1_emission_invariant_has_teeth` |
| **MINT-EXACT-1** | block reward `== emission_for_block(prior mined)`, recomputed by each receiver | `ledger/validation.rs` (recompute, not bound) | `mint_exact_reward_is_a_pure_function_of_the_chain`, `mint_exact_over_emission_is_rejected` |
| **REWARD-SHARE-1** | the split is recomputed and imposed, exact to the µQTA, scale-invariant | `validate_block_reward_plan` (`validation.rs:350`) | `reward_share_splits_and_conserves_to_the_microqta`, `reward_share_greedy_proposer_is_rejected`, `reward_share_is_scale_invariant` |
| **COVER-1/2** | no spend or stake exceeds the on-chain balance before the block — symmetric on send and receive | `uncovered_tx_indices` (`validation.rs:899`), called by both seal and validate | `cover1_*` (reception) and `cover2_*` (production), incl. `cover1_both_paths_reject_uncovered` |
| **AUTH** | transaction authority is pure ML-DSA-65 bound to the `from` address | `verify_tx` (`validation.rs:41`) | `p2p/security_tests.rs` (44 tests) |
| **PROPOSER-1** | a non-genesis block is rejected unless its proposer was a bonded validator as of the parent — except on open slots | `validate_block_against_prev` (`validation.rs:602`) | `open_door_newcomer_can_seal_on_an_open_slot_only` |
| **OPEN-DOOR-1** | open-slot cadence is a pure function of height | `pos_consensus.rs:98` | `open_door_cadence_is_a_pure_function_of_height` |
| **SAFETY** | no two nodes hold different blocks at the same height | fork choice + shared validator | `sm/sim.rs::check_invariants` (safety scan) |
| **FINALITY-SAFETY** | no two conflicting finalized checkpoints at the same epoch | `sm/finality_rule.rs` | `gadget_1_finality_safety_invariant_has_teeth` |
| **LIVE-2 floor** | a finalized block is never replaced | `integrate_remote_block` (`reorg.rs:317`) | `finality_live` floor tests |
| **SLASH** | slashing is conservation-neutral and reaches unbonding stake; an innocent cannot be punished | `expected_slash_consumption` (`slash.rs:39`), `verify_block_slashes` | `t0_8_slash_sweep_conserves`, LIVE-3B tests |
| **C1 determinism** | same events + same chain ⇒ byte-identical outcome | `sm/` is IO-free by construction | `t0_8_replay_is_byte_identical`, `t0_8_sweep_is_reproducible` |
| **H1 envelope** | envelope id equals `BLAKE3(signed pre-image)`; dedup writes only after authentication | `dispatcher.rs:422` steps ④–⑧ | `p2p/security_tests.rs` |

Every invariant with "teeth" in its test name has a **planted-violation test**: the test breaks
the property on purpose and asserts the checker catches it. An invariant checker that has never
failed is not known to work.

---

## 6. Testing what a unit test cannot reach

513 library tests plus 1 integration test, `clippy --all-targets -D warnings` clean.
But the count is not the argument — the argument is what the tests can reach.

**The deterministic simulation harness** (`sm/sim.rs`) runs a whole network inside one process
with an injected clock and a seeded RNG. It can partition the network, reorder and drop
messages, crash and restart nodes, and run Byzantine participants that equivocate on purpose.
After every step it checks the invariants of §5. A failing seed reproduces exactly, forever.

Two properties make it more than a fuzzer. `t0_8_replay_is_byte_identical` asserts that
replaying a seed produces identical bytes — meaning a bug found in CI can be handed to a
developer as a number. And `t0_8_sweep_catches_planted_violation` deliberately breaks a rule to
prove the harness notices; a sweep that has never gone red proves only that it runs.

The default sweep runs 64 seeds (`default_seed_count()`, raise it with `QUANTA_SIM_SEEDS`);
the fault-coverage sweep runs 128.

**What the harness cannot see** is the honest and important part. It runs in one process, over
simulated transport. It cannot see anything that only exists between operating-system processes
on a real network — and in August 2026 that gap hid a two-month-old regression that had made
the node completely mute. See the third story below. The lesson is now policy: before claiming
P2P works, run two real daemons against the real DHT and verify by RPC, not by log lines.

---

## 7. Three bugs that shaped the design

Design rationale is easier to trust when you can see what it was paying for.

### The exemption that printed money

`verify_tx` once exempted transactions with `to == "BURN"` from signature verification — the
reasoning being that burning is destructive, so who would forge one? Anyone, as it turned out:
`from = victim, to = BURN` drains a victim through gossip without ever holding their key.

The same shape recurred twice more. A `Transfer` from the synthetic `NETWORK` sender skipped
signature, coverage *and* the supply cap — the cap only summed `Mining` transactions — giving
unlimited issuance to anyone who asked in the right dialect. And the emission bound accepted
`64 × emission_for_tick` per block when the honest amount was `2 × emission_for_tick / N`: a
factor of `32 × N`, so on a 100-node network any bonded validator could mint **3 200×** its
legitimate reward, every block, accepted by every peer, with only the 100M cap ever noticed.

The pattern is one thing: **an exemption is a hole with a comment on it.** Today `NETWORK` is
confined to the single coinbase, and the emission amount is recomputed rather than bounded. The
rule this produced is written into the repository's Rust conventions: no locally computed value
— Shapley share, self-reported watts, wall-clock reading — may ever touch the money path.

### The reward that sealed a private chain

Fixing the emission amount fixed five other things at once, which is usually a sign the original
design was wrong rather than merely buggy.

Minting used to happen on the 60-second mining tick, via a function that sealed a block by
itself once ten transactions had accumulated — **without ever broadcasting it**. A node that was
not an elected proposer therefore built a private chain, credited itself, and displayed a balance
the network had never heard of. Meanwhile the `Mining` transactions it did broadcast were
rejected by every peer as invalid, so the traffic was pure noise. And because the local Shapley
shares summed to 1, the *realized* emission collapsed as `1/N`: the bigger the network, the less
money it created.

The fix separates the two responsibilities that had been fused: **admission never seals**, and
minting is an explicit producer step invoked from the sealing path. `mine_tx` is now
`#[cfg(test)]` — in a release build nothing can put a `Mining` transaction into the mempool at
all. That is the strongest kind of fix available: not a check that must be remembered, but a
construction where the mistake cannot be expressed.

### The node that was mute for two months

Every test was green. A two-node integration test passed on every run. The repository's own
milestone said P2P had been verified between two physical machines.

Then two real daemons were started against the public mainline DHT. They found each other,
completed the QUIC handshake, reported `NeighborUp`, and exchanged **nothing at all**. Not a
`Hello`, not a block, not a finality vote.

`iroh-gossip` caps a message at 4 096 bytes. Since the v4 hard fork made envelopes ML-DSA-signed,
a `Hello` carries a 3 309 B signature and a 1 952 B public key, hex-encoded inside JSON — about
15 KB, or roughly four times the cap. The send failed silently and was still counted in
`stats.messages_sent`, so no metric contradicted it.

Two further bugs surfaced in the same session. The node inserted **itself** into its own peer
map, whose size *is* the peer count — so an entirely isolated node reported "1 peer", and the
rendezvous back-off logic, which retries fast only while zero peers are known, never fired.
And `NeighborUp` triggered no immediate `Hello`, leaving two freshly connected nodes mutually
invisible for up to 120 seconds.

All three are fixed and verified live: mutual discovery in about 9 seconds, chain sync by
`ChainSegment`, convergence, and a block carrying two coinbases summing exactly to the canonical
reward. The honest reading of the episode is not that the tests were bad — it is that a green
suite plus an old milestone had been read as evidence of something neither one covered. The
milestone predated the change that broke it by two months.

---

## 8. What is deliberately not done

Stated plainly, because a roadmap that hides its gaps is a marketing document.

- **No third-party audit.** An internal adversarial audit (2026-07-25) opened 4 criticals,
  8 highs and 4 mediums, all fixed behind the v7 fork — but an internal audit is not an audit.
  The consultation package (threat model, scope, RFQ) is ready in [`docs/audit/`](audit/).
- **No public testnet.** No public bootstrap peers, no open network. Real scale is untested;
  the largest verified topology is two nodes.
- **No cryptographic VRF, no VDF.** The proposer of a future slot is publicly predictable
  (ADR-004, open).
- **The Iroh NodeId is still Ed25519.** Money, finality, gossip authentication and transport
  key exchange are post-quantum; node identity is not, and that one is upstream — Iroh is
  waiting on an industry consensus for PQ endpoint signatures.
- **Multisig has no multi-party UX and no integration test.** On-chain M-of-N verification is
  live and tested; the signing flow is not built.
- **The protocol has broken nine times.** `TORUS_PROTOCOL_VERSION = 9`; any older binary,
  snapshot or chain is incompatible by design. Compatibility is not yet a promise anyone
  should rely on.
- **macOS binaries are not notarized**, and the published release predates the current code.
- **QUANTA has no market value.** It is listed nowhere and the app displays no price. A cost
  of production is not a price.

---

## 9. Where to start reading

If you have thirty minutes and want to judge the engineering, read in this order:

1. `src-tauri/src/sm/finality_rule.rs` — the Casper-FFG justify/finalize rule, pure and small.
   If this is wrong, nothing else matters.
2. `src-tauri/src/p2p/ledger/validation.rs` — `verify_tx`, `validate_block_against_prev`,
   `uncovered_tx_indices`, `validate_block_reward_plan`. Every rule a block must satisfy is here.
3. `src-tauri/src/p2p/dispatcher.rs` — the receive pipeline, where ordering is the security
   property.
4. `src-tauri/src/sm/sim.rs::check_invariants` — what the system believes about itself.

If you are scoping an audit, start with [`docs/audit/SCOPE.md`](audit/SCOPE.md) and
[`docs/audit/THREAT-MODEL.md`](audit/THREAT-MODEL.md), then the invariant table in §5.

If you want to run it, [`docs/ops/QUICKSTART.md`](ops/QUICKSTART.md) covers the desktop app,
the headless node, the JSON-RPC surface, and a scripted two-machine test.

Design decisions with their rejected alternatives are recorded as ADRs in
[`docs/decisions/`](decisions/). The economic reasoning — including the mechanisms that were
considered and refused — is in [`docs/economy/DOCTRINE.md`](economy/DOCTRINE.md).
