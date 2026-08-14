# Quanta — Architecture & Engineering Notes

*A guided tour of the protocol, written to be read start to finish.*

This document exists because a repository of 44 067 lines of Rust does not explain itself.
It is written for two readers who want the same thing from opposite directions: an **auditor**
who needs to know what is claimed and where each claim is enforced, and an **engineer**
who wants to know whether the design holds up. Both get the same answer here, because
if a design cannot be explained plainly it usually cannot be defended either.

Every number below is read from the code at `TORUS_PROTOCOL_VERSION = 10` (v3.16.0,
`CHAIN_ID = "quanta-mainnet-v10"`). Nothing is rounded up to sound better. Where something
is unfinished it says so.

Code references name a file and a symbol — `ledger/validation.rs::verify_tx`, or
`dispatcher.rs`, `MAX_RAW_ENVELOPE_BYTES` — and never a line number. Line numbers go stale
at the next commit: in the revision of this document that preceded the external review,
every one of them pointed at the wrong code, and one pointed into a `#[cfg(test)]` helper.

**Contents**

1. [The problem](#1-the-problem)
2. [The shape of the code](#2-the-shape-of-the-code)
3. [Life of a transaction](#3-life-of-a-transaction)
4. [Life of a block](#4-life-of-a-block)
5. [The invariants, and where each is enforced](#5-the-invariants-and-where-each-is-enforced)
6. [Testing what a unit test cannot reach](#6-testing-what-a-unit-test-cannot-reach)
7. [Four bugs that shaped the design](#7-four-bugs-that-shaped-the-design)
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
├── sm/          8 525 lines   the deterministic, IO-free core — decides
├── p2p/        27 208 lines   the live network — talks, then asks the core
├── security/    2 491 lines   ML-DSA-65, vault, symmetric primitives
├── commands/    2 262 lines   Tauri command surface, by domain
├── storage/       335 lines   libSQL persistence
├── views.rs       478 lines   pure view-models shared by the app and the JSON-RPC
└── crate root   2 768 lines   lib.rs, rpc.rs, node_runtime.rs, commands_v3.rs,
                               guardian.rs, main.rs, bin/quanta-node.rs
                 ─────────
                 44 067 lines of Rust

src/            10 055 lines   Svelte 5 desktop UI (out of consensus scope)
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

Everything with a socket or a lock. The ledger (`ledger/`, a six-file module: `mod.rs`,
`validation.rs`, `stake.rs`, `slash.rs`, `reorg.rs`, `tests.rs`), the gossip protocol and its
dispatcher, the PoS election, the mining loop, the live wiring of the finality gadget
(`finality_live.rs`) and deep fork reconciliation (`fork_heal.rs`).

One irregularity worth naming, since an auditor enumerating the command surface will hit it:
`commands/` is not the whole IPC surface. `commands_v3.rs` sits at the crate root and registers
six more commands (the `@username` handling) in the same `generate_handler!` list.

The division of labour is deliberate: **`p2p/` may decide when to ask, never what the answer
is.** When the two must agree — the reward split, the slash accounting — a single function
produces the plan and both the producer and every verifier call it. That pattern
(`expected_block_rewards`, `expected_slash_consumption`) appears repeatedly below, and it is
the main structural defence against the class of bug where "what I build" and "what you check"
drift apart by one commit.

The shipped binary contains **one `unsafe` block** (`guardian.rs`, an AppKit cast to read
window occlusion state). There is a second one in the backend, and it is fair to say so:
`rpc.rs` calls `std::env::set_var` under `#[cfg(test)]`, which edition 2024 makes `unsafe`.
It is compiled out of every release build. Nothing on the money path is unsafe in either.

---

## 3. Life of a transaction

Follow one transfer of 10 QUANTA from Alice to Bob.

**1. Typed.** Alice enters `@bob` or a `qta1…` address. Addresses are Bech32m with a
checksum, so a typo is rejected locally rather than becoming an unrecoverable send.
Internally an address is `BLAKE3(ADDR_DOMAIN ‖ ML-DSA public key)`.

**2. Signed.** The wallet unlocks the vault (Argon2id 64 MiB / 3 iterations / p=4 →
AES-256-GCM) and signs with ML-DSA-65. Authority is *pure* ML-DSA:
`ledger/validation.rs::verify_tx` checks that the signing key hashes to the `from` address.
There is no classical co-factor left on this path. Secrets are `zeroize`d after use.

What that signature covers is itself a canonical, domain-separated, length-prefixed encoding
(`ledger/mod.rs::tx_signing_preimage`). Until the v10 fork it was a `format!` joined by `:`
over fields free to contain `:` — see the fourth story in §7.

**3. Split.** A transfer burns 1 %. That burn is a **second transaction**, and both are
broadcast — a lesson from AUDIT-TX-2, where the burn was applied locally but never gossiped,
so every remote peer disagreed with the sender about the balance by exactly 1 %.

**4. Gossiped.** Each transaction is wrapped in a `GossipEnvelope` signed with ML-DSA-65 and
published to the shared topic over Iroh QUIC, whose TLS 1.3 handshake negotiates hybrid
`X25519MLKEM768`.

**5. Received.** On the far side, `p2p/dispatcher.rs::dispatch_incoming` runs a fixed
pipeline of thirteen steps. The order is the security property:

```
①  raw size ≤ 4 MiB (MAX_RAW_ENVELOPE_BYTES)    ⑧  timestamp freshness ±90 s
②  JSON decode                                  ⑨  dedup PROBE (read) + global stats
③  header-field shape                           ⑩  per-peer accounting
④  peer ban probe (READ lock)                   ⑪  dedup INSERT
⑤  ML-DSA-65 signature  ← authentication gate   ⑫  adaptive rate-limit, then nonce
⑥  expired-ban eviction (the only write lock)   ⑬  dispatch to handler
⑦  canonical id = BLAKE3(signed pre-image)
```

The rule that produces this order is one sentence: **nothing before ⑤ allocates or mutates
anything at a size the sender chose.** It was arrived at twice, closing two different families
of bug.

The first was censorship. Before the v7 fork the dedup insert happened early, before the
signature check. Any unauthenticated stranger could therefore pre-insert chosen envelope ids
into the LRU and make a peer silently drop the real messages that would later carry those
ids — free, untraceable censorship of chain sync. That fix has two halves: the envelope id
must *equal* `BLAKE3(signed pre-image)` so it can no longer be chosen freely, and insertion
moved after the signature check so an unauthenticated party cannot write to the cache at all.

The second was cost, and the external review of 2026-08-13 found it exactly where the first
fix had left it: verification still came last. The canonical id (a full re-serialization plus
BLAKE3), the ban probe under a *write* lock, the global stats and the per-peer counters were
all paid before a single signature was checked. Measured by the reviewer: 8 MB of
unauthenticated input bought 16 ms of the victim's CPU on the single sequential dispatch loop,
and upstream of the rate limiter, which sits after the signature gate. Three findings were the
same defect from three angles — R6 (work before authentication), R11 (`bytes_in` /
`messages_in` written from an attacker-chosen `sender`, so a node's own metrics lied *during*
an attack, which is when an operator reads them), R16 (ban probe taking a write lock). The
order above is the answer, and its price is stated rather than hidden: a stale but correctly
signed envelope now pays one ML-DSA verification (~160 µs) before being dropped for staleness.

The size bound is 4 MiB, not the 10 MB an earlier revision of this document advertised. The
same review reduced it (R3): `iroh-gossip` relays and caches a message for 30 s *before*
authentication, so a 10 MB envelope was a remote OOM against a peer that had proven nothing.
The same constant caps the transport itself in `willow_node.rs` — one limit, one place, which
is what the third story in §7 is about.

**6. Admitted.** The transaction enters the mempool (TTL 10 min, cap 1000). Admission never
seals a block — that separation is load-bearing, see §7.

---

## 4. Life of a block

**1. Who may propose.** The slot is the chain height. A beacon is derived from a *buried*
block — two heights back, `pos_consensus.rs`, `LEADER_ENTROPY_LOOKBACK = 2` — so the current
proposer cannot grind the seed by choosing their own block's contents:

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
predictable, and therefore targetable. The buried beacon only prevents *immediate* grinding:
the proposer at height `h − 2` still influences the beacon at `h`, which the lookback makes
non-trivial rather than impossible. Both are still open, and named as open in
[`docs/audit/REMEDIATION-2026-08-13.md`](audit/REMEDIATION-2026-08-13.md) §9.6. A real VRF
(unpredictability) plus a VDF (grinding resistance) are the clean closure, tracked in ADR-004.

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

That reward is then **shared**: half to the producer, half among *recent participants* — the
addresses that produced a block within the last `SHARE_WINDOW_BLOCKS = 32`, each weighted by
**how many** of those blocks it produced (`ledger/mod.rs::recent_participation`, consumed by
`ledger/mod.rs::expected_block_rewards`). Integer division remainder goes to the producer, so
conservation is exact to the µQTA. No new wire field was needed: participation is already
proven by `block.miner`, which is bound into the block hash.

The weighting is not a detail, and it is not the first design. The pot used to be split
**equally between distinct addresses**, which pays for presence rather than production — and
an address costs nothing. The external review priced it (H-08): with 28 identities an attacker
captured 45.2 % of every reward against 12.5 % with one, tending to 50 % at the limit. A
mechanism meant to reward liveness was subsidising identity duplication. Weighting by blocks
produced cancels that exactly, because slots are a finite resource: splitting one identity into
K does not produce one more block, so a Sybil becomes strictly neutral — same share, more cost,
no gain. Stake weighting was refused for a different reason: it would recreate the capital rent
the project's economic doctrine rejects, and it would diverge between producing and verifying
depending on the admission path.

The split is **imposed, not suggested**. Each node recomputes the plan
(`ledger/validation.rs::validate_block_reward_plan`) and rejects a producer that keeps
everything or pays someone outside the plan. The plan is *scale-invariant*: minting less than
canonical stays legal, but you cannot shrink someone else's share. A share applied only by the
reference client is not a rule, it is a courtesy — and courtesies are not consensus.

Rewarding *recent production* rather than bonded stake also makes this a liveness premium: a
validator that goes offline falls out of the window and stops being paid.

**3. Sealed.** `ledger/reorg.rs::seal_block_at` excludes any transaction not covered by the
sender's on-chain balance before the block (`ledger/validation.rs::uncovered_tx_indices`), and
any transaction that is not its sender's exact next nonce
(`ledger/validation.rs::nonce_faults`), so a produced block is **valid by construction**.
Coverage is sequential and counts intra-block credits; synthetic senders (`NETWORK`, `ESCROW`,
`BURN`) are exempt from coverage.

`NETWORK` is *not* confined to a single coinbase: since REWARD-SHARE-1 a block carries several,
one per payee of the reward plan. What replaced "at most one mining transaction" is stronger,
not weaker. A synthetic sender may only ever appear as a `Mining` transaction
(`ledger/validation.rs::illegal_synthetic_indices`), and the reward plan pins the exact set of
payees with their exact amounts — a coinbase to someone outside the plan, or for the wrong
amount, is rejected, and no address may be paid twice in one block.

**4. Validated on arrival.** `ledger/validation.rs::validate_block_against_prev` is the
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
re-verifies it against its **own** plan (`ledger/slash.rs::expected_slash_consumption`).

Once finalized, history is anchored by a persisted, monotonic, hash-verified
`finalized_floor_index`. `ledger/reorg.rs::integrate_remote_block` refuses any fork that would
replace a block at or below the floor. Above the floor, ordinary fork choice applies; below it,
nothing does.

**6. Partitions heal.** When two partitions each sealed several blocks,
`p2p/fork_heal.rs::ForkReconciler` assembles the competing branch from a bounded buffer, picks
the winner, and applies it through `Ledger::reorg_to_fork` after full validation on a trial
clone. Exactly one side adopts, so convergence is symmetric.

The winner is the longest branch above the finality floor; at equal height the fork choice
decides. `fork_heal.rs::assemble_winning_run` calls `ledger/reorg.rs::prefers_same_height`,
which is `ledger/reorg.rs::fork_choice_prefers`: the better-elected proposer wins, ranked by
`pos_consensus.rs::election_rank_of`. The tip hash breaks a tie only between equal ranks, or on
an open slot where an unranked proposer is legitimate by design. This is the C-04 fix
(FORK-RANK-1) and it replaced a plain "greatest hash wins": since the block timestamp enters
the hash, a proposer could grind a few thousand BLAKE3 and take every tie-break without owning
one extra µQTA of stake. The three inputs of the rank — buried beacon, height, bonded set as of
the parent — appear in neither competing block, so grinding moves nothing. Reorg depth is
bounded to `MAX_REORG_DEPTH = 128` blocks whatever the score, and that bound has a stated price:
past it, a partition no longer heals by itself.

---

## 5. The invariants, and where each is enforced

The table an auditor should start from. Each row is a property the system claims, the single
place it is enforced, and the test that would fail if it stopped holding.

| # | Invariant | Enforced at | Proven by |
|---|---|---|---|
| **CONSERVE** | `Σ(spendable + staked + unbonding) + burned == minted` | accounting is by construction; checked every simulation step | `sm/sim.rs::check_invariants`, `t0_8_conservation_under_burn` |
| **CAP** | mined supply never exceeds 100 000 000 QUANTA | `ledger/validation.rs::validate_block_emission_against` | `emit_1_emission_invariant_has_teeth` |
| **MINT-EXACT-1** | block reward `== emission_for_block(prior mined)`, recomputed by each receiver | `ledger/validation.rs::validate_block_emission_against` (recompute, not bound) | `mint_exact_reward_is_a_pure_function_of_the_chain`, `mint_exact_over_emission_is_rejected` |
| **REWARD-SHARE-1** | the split is recomputed and imposed, exact to the µQTA, scale-invariant, weighted by blocks produced in the window | `ledger/validation.rs::validate_block_reward_plan`, against `ledger/mod.rs::expected_block_rewards` | `reward_share_splits_and_conserves_to_the_microqta`, `reward_share_greedy_proposer_is_rejected`, `reward_share_is_scale_invariant`, `h08_splitting_into_many_identities_earns_exactly_the_same` |
| **COVER-1/2** | no spend or stake exceeds the on-chain balance before the block — symmetric on send and receive | `ledger/validation.rs::uncovered_tx_indices`, called by both seal and validate | `cover1_*` (reception) and `cover2_*` (production), incl. `cover1_both_paths_reject_uncovered` |
| **AUTH** | transaction authority is pure ML-DSA-65 bound to the `from` address | `ledger/validation.rs::verify_tx` | `p2p/security_tests.rs` (44 tests) |
| **CANON-1** | what a signature covers is injective: domain separator, length-prefixed fields, numeric type tag, chain id | `ledger/mod.rs::tx_signing_preimage`, `::tx_content_bytes`, `::block_header_preimage` | `crit1_tx_preimage_is_injective_across_field_boundaries`, `crit1_tx_type_tags_are_distinct_and_stable` |
| **NONCE-ONCHAIN-1** | a signed transaction is includable exactly once on a branch: the sender's nonce is sequential and checked at inclusion | `ledger/validation.rs::nonce_faults`, called by both seal and validate | `c01_a_signed_tx_cannot_be_included_twice`, `c01_the_same_tx_ten_times_in_one_block_is_rejected` |
| **PROPOSER-1** | a non-genesis block is rejected unless its proposer was a bonded validator as of the parent — except on open slots | `ledger/validation.rs::validate_block_against_prev` | `open_door_newcomer_can_seal_on_an_open_slot_only` |
| **OPEN-DOOR-1** | open-slot cadence is a pure function of height | `p2p/pos_consensus.rs::is_open_slot` | `open_door_cadence_is_a_pure_function_of_height` |
| **FORK-RANK-1** | at equal height the better-elected proposer wins; the hash decides only equal ranks and open slots | `ledger/reorg.rs::fork_choice_prefers`, ranked by `p2p/pos_consensus.rs::election_rank_of` | `c04_a_ground_hash_no_longer_beats_a_better_elected_proposer`, `c04_the_fork_rank_is_identical_on_both_sides_of_a_fork`, `c04_the_open_slot_keeps_a_neutral_tiebreak` |
| **SAFETY** | no two nodes hold different blocks at the same height | fork choice + shared validator | `sm/sim.rs::check_invariants` (safety scan) |
| **FINALITY-SAFETY** | no two conflicting finalized checkpoints at the same epoch | `sm/finality_rule.rs` | `gadget_1_finality_safety_invariant_has_teeth` |
| **LIVE-2 floor** | a finalized block is never replaced | `ledger/reorg.rs::integrate_remote_block` | `finality_live` floor tests |
| **SLASH** | slashing is conservation-neutral and reaches unbonding stake; an innocent cannot be punished | `ledger/slash.rs::expected_slash_consumption`, `ledger/slash.rs::verify_block_slashes` | `t0_8_slash_sweep_conserves`, LIVE-3B tests |
| **C1 determinism** | same events + same chain ⇒ byte-identical outcome | `sm/` is IO-free by construction | `t0_8_replay_is_byte_identical`, `t0_8_sweep_is_reproducible` |
| **H1 envelope** | envelope id equals `BLAKE3(signed pre-image)`; dedup writes only after authentication | `p2p/dispatcher.rs::dispatch_incoming`, steps ⑤–⑪ of §3 | `p2p/security_tests.rs` |

Every invariant with "teeth" in its test name has a **planted-violation test**: the test breaks
the property on purpose and asserts the checker catches it. An invariant checker that has never
failed is not known to work.

---

## 6. Testing what a unit test cannot reach

608 library tests plus 1 integration test, `cargo clippy --all-targets -- -D warnings` clean
(exit 0). But the count is not the argument — the argument is what the tests can reach, and
whether any of them has ever been seen to fail.

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

### A fix is not done until its test has been seen to fail

The second policy, adopted after the external review of 2026-08-13: **no security fix counts as
done until the test that covers it has been observed red.** Sabotage the fix on purpose, run the
test, watch it fail for the stated reason, then restore the file byte for byte and run again.
The planted-violation tests of §5 are the same idea, automated for the invariant checkers.

The incident that hardened this rule is M-14. The finding was a cost asymmetry: a remote peer
paid O(1) to make the receiver do O(height) work — four chain-length views built *before* any
rejection, under the write lock. The fix is an O(1) gate (timestamp plausibility, then the size
bound) ahead of any walk. It was first "verified" by a timing measurement, and that measurement
went green **with and without the fix**: on a test chain, one ML-DSA verification dominates a few
hundred trivial iterations, so the stopwatch was reading the signature, not the walk. A green
test proved nothing about the property it was named after.

It was replaced by a counter: `CHAIN_WALKS`, in `ledger/validation.rs`, compiled under
`#[cfg(test)]` and incremented by `note_chain_walk`, which is an empty function in any other
build. `m14_an_implausible_block_triggers_no_chain_walk_at_all` asserts exactly zero walks for a
block rejected on its timestamp — and then asserts the honest path still walks, because
otherwise the "optimisation" would just be a deleted check. Counting is exact and deterministic
where a duration is neither. The write-up is in
[`docs/audit/REMEDIATION-2026-08-13.md`](audit/REMEDIATION-2026-08-13.md) §13.

The honest limit of this policy: it is a manual discipline. Nothing in CI records that a given
test was ever seen red, so it is a practice, not a guarantee — and §7 ends with two fixes from
that same review that were not right as written, one of them shipped without ever being run.

---

## 7. Four bugs that shaped the design

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

The pattern is one thing: **an exemption is a hole with a comment on it.** Today `NETWORK` may
only ever appear as a `Mining` transaction — a block carries several coinbases since
REWARD-SHARE-1, and the reward plan pins every payee and every amount — and the emission amount
is recomputed rather than bounded. The rule this produced is written into the repository's Rust
conventions: no locally computed value — Shapley share, self-reported watts, wall-clock
reading — may ever touch the money path.

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

All three are fixed. The fix was exercised by running two real daemons against the real DHT:
they discovered each other, synced by `ChainSegment`, converged, and sealed a block carrying two
coinbases that summed exactly to the canonical reward. That session left **no artefact in this
repository** — no test, no committed log, no constant carries it — so it should be read as a
report, not as evidence. What is reproducible is narrower: the transport cap now comes from the
protocol constant (`willow_node.rs` builds the gossip layer with
`max_message_size(MAX_RAW_ENVELOPE_BYTES)`), and the two-machine procedure is scripted in
[`docs/ops/two-machines.sh`](ops/two-machines.sh).

The honest reading of the episode is not that the tests were bad — it is that a green suite plus
an old milestone had been read as evidence of something neither one covered. The milestone
predated the change that broke it by two months.

### The audit that found the rules nobody had written

On 2026-08-13 an external reviewer went through the repository: 85 findings, 13 of them critical.
The reports are published verbatim in [`docs/audit/2026-08-13/`](audit/2026-08-13/), including the
passages showing that an earlier fix by this project was wrong. What was done about each finding,
and what was not, is in [`docs/audit/REMEDIATION-2026-08-13.md`](audit/REMEDIATION-2026-08-13.md).

The list is not the interesting part. The pattern the reviewer named is:

> The project verifies very well what it decided to verify, and does not verify what it never
> wrote a rule for. Every defence in place is well built; it is the **absent** defences that
> open the system, and they are absent in silence, with no red test to say so.

Three findings show what that means concretely.

**The signature was correct; what it signed was ambiguous.** The transaction pre-image was a
`format!("{}:{}:…")` over fields — `id`, `to`, `timestamp` — that may legally contain `:`. So it
was not injective: two semantically different transactions could produce the same pre-image,
therefore the same ML-DSA signature and the same `tx.hash`. Proven end to end: two nodes ended up
with the same chain, the same block hashes, and different balances. Nothing here was a coding
mistake: the verification code was right, the rule "a pre-image must be injective" had simply
never been written down. The model for the fix was already in the repository —
`sm/finality_vote.rs::signable_bytes` had used a domain separator and length-prefixed fields
since GADGET-2. It had never been applied to transactions.

**A transaction had no on-chain uniqueness.** Anti-replay existed, but only at mempool
admission; on the block path the hash was inserted without the result ever being read. The same
signed transaction, included ten times in one block, was accepted — the only wall was balance
coverage, that is, draining the victim. The fix (NONCE-ONCHAIN-1) is a sequential per-account
nonce checked at inclusion on all four admission paths, and it is the structural one: without
it, a signature stays reusable forever.

**The application ACL was decorative.** `core:default` gave the appearance of a perimeter, while
the application's own commands never passed through it — the generated `acl-manifests.json`
contained no application key at all. Any JavaScript running in the webview could call
`invoke("get_recovery_phrase")` and receive the 24 words, and the CSP of the day explicitly
allowed outbound connections to `api.github.com`. The fix declares an application ACL manifest
in `build.rs` and grants each command explicitly.

The first two fixes are what forced the protocol break v9 → v10: every signature and every hash
changed, genesis included, and the chain identifier is now bound into every pre-image
(`CHAIN_ID = "quanta-mainnet-v10"`).

**The epilogue is the useful part: two of the fixes written in answer to this review did not
hold as written, and each was caught by its own test.**

A13 was the CSP. Replacing `script-src 'self' 'unsafe-inline'` with hash mode is the right
answer, but SvelteKit hashes only *its* inline scripts; the hand-written anti-flash theme script
in `src/app.html` was covered by no hash, and the intersection of the two policies refused it.
Nothing failed: not the build, not `svelte-check`, not `clippy`. The fix had been shipped without
ever being executed. The only symptom was at runtime — dark theme reverting to light, a white
flash, a violation in the webview console. A security fix that introduced a visible regression
invisible to the entire verification chain. The script moved to `static/theme-boot.js`, loaded by
a synchronous `<script src>`: `'self'` is already in both policies and never goes stale, unlike a
hash maintained by hand. `scripts/check-csp.mjs` now re-reads the **produced bundle** on every CI
run, so the property is checked on the shipped file rather than on the source.

R17 was an allocation defect in the fork buffer: eviction scanned the whole buffer with a
`max_by_key` that cloned each hash twice, about 2 048 `String`s per offered block, for a decision
that needs none. Writing the test that compares the cheap O(log n) search to the full scan it
replaces surfaced a different bug underneath: `entry(idx).or_default()` created the vector
*before* the three refusals that follow, and `FORK_BUFFER_MAX_BLOCKS` bounds blocks, not keys —
so a peer could inflate the index map with empty phantom indices without consuming a single
buffer slot, and those indices then fed the ancestor-probe window. The old full scan skipped
empty vectors and never saw them; the edge search walked straight into them, which is how the
leak surfaced. The search now skips empty edges explicitly, `offer` no longer creates a key it
is about to refuse, and `r17_a_refused_block_leaves_no_phantom_index_behind` holds the line.

Neither of those two was found by review. They were found because the fix was required to come
with a test, and the test was required to be run. That is the whole argument for the policy in
§6 — and, equally, the reason §8 does not describe this repository as audited.

---

## 8. What is deliberately not done

Stated plainly, because a roadmap that hides its gaps is a marketing document.

- **One external review, which is not a firm audit.** A paid, single-reviewer engagement took
  place on 2026-08-13: 85 findings, 13 critical. The reports are published verbatim in
  [`docs/audit/2026-08-13/`](audit/2026-08-13/) and the remediation in
  [`REMEDIATION-2026-08-13.md`](audit/REMEDIATION-2026-08-13.md). One reviewer's pass is not an
  audit by an established firm and does not replace one. The earlier internal adversarial audit
  (2026-07-25: 4 criticals, 8 highs, 4 mediums, all fixed behind the v7 fork) is not an audit at
  all. The file is still open, and the remediation document states the conclusion this document
  repeats: **while these items are open, the network must carry no value.** The consultation
  package (threat model, scope, RFQ) is in [`docs/audit/`](audit/).
- **What that review left open.** Listed in §9.6 and §13 of
  [`REMEDIATION-2026-08-13.md`](audit/REMEDIATION-2026-08-13.md). The *happy* path of block
  validation is still O(chain height): only the rejection path was made O(1), and making the rest
  incremental means a cache whose smallest error is a fork. `REPORT_BAN_THRESHOLD` counts
  identities, and an ML-DSA identity costs about 165 µs, so three puppets still ban anyone.
  `iroh-gossip` relays and caches a message for 30 s before authentication — upstream behaviour,
  not code written here. The fund anchor sits unsigned in the same database as the vault, so
  deleting two rows replays the trust-on-first-use once. And FIPS-204 context separation was
  deliberately *not* applied: setting it on one side of twenty-five call sites invalidates every
  signature on the network, silently.
- **No public testnet.** No public bootstrap peers, no open network. Real scale is untested;
  the largest verified topology is two nodes.
- **No cryptographic VRF, no VDF.** The proposer of a future slot is publicly predictable, and
  the proposer two heights back still influences the election beacon (ADR-004, open).
- **The Iroh NodeId is still Ed25519.** Money, finality, gossip authentication and transport
  key exchange are post-quantum; node identity is not, and that one is upstream — Iroh is
  waiting on an industry consensus for PQ endpoint signatures.
- **Multisig has no multi-party UX and no integration test.** On-chain M-of-N verification is
  live and tested; the signing flow is not built.
- **The protocol has broken ten times.** `TORUS_PROTOCOL_VERSION = 10`, `CHAIN_ID =
  "quanta-mainnet-v10"`. The v10 break changed every signature and every hash, genesis included;
  a v9 node and a v10 node are refused at the `Hello`. Any older binary, snapshot or chain is
  incompatible by design, and compatibility is not yet a promise anyone should rely on.
- **macOS binaries are not notarized**, and the published release predates the current code.
- **QUANTA has no market value.** It is listed nowhere and the app displays no price. A cost
  of production is not a price.

---

## 9. Where to start reading

If you have thirty minutes and want to judge the engineering, read in this order:

1. `src-tauri/src/sm/finality_rule.rs` — the Casper-FFG justify/finalize rule, pure and small.
   If this is wrong, nothing else matters.
2. `src-tauri/src/p2p/ledger/validation.rs` — `verify_tx`, `validate_block_against_prev`,
   `uncovered_tx_indices`, `nonce_faults`, `validate_block_reward_plan`. Every rule a block must
   satisfy is here.
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
