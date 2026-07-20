# Quanta: A Post-Quantum Peer-to-Peer Currency with Irreversible Finality

> Status: alpha — not yet audited by a third party. QUANTA has no market and no
> price; none is claimed or predicted anywhere in this document. Every constant
> below is carved in the code and verified by every node at every block.

**Abstract.** A purely peer-to-peer currency must survive the two failures its
predecessors accept. Elliptic-curve signatures fail against a quantum
adversary: everything signed today can be forged the day such a machine
exists. And probabilistic settlement fails against patience: a Bitcoin block
is never final, only exponentially unlikely to be reversed. We propose a
currency in which account authority, finality votes and network envelopes are
ML-DSA-65 signatures (FIPS 204), the transport negotiates a hybrid
post-quantum key exchange, and history becomes irreversible by certificate
rather than by probability: a checkpoint carrying signatures from two thirds
of the enrolled stake is final, and finalizing a conflicting history provably
destroys at least one third of that stake. Supply is closed-form: 100,000,000
coins, zero premine, emission decaying geometrically toward the cap.

---

## 1. Introduction

Electronic money rests on promises. A fiat currency rests on the restraint of
its central bank; a platform balance rests on a server that can be seized or
frozen. Peer-to-peer currencies removed the promiser, but kept two quieter
assumptions: that discrete-logarithm signatures will never be broken, and that
a history buried under enough work is safe enough.

Both assumptions have expiry dates. Signatures based on elliptic curves are
broken by a sufficiently large quantum computer; traffic and public keys
recorded today are forged retroactively ("harvest now, decrypt later"). And
work-buried history is a probability, not a fact: reversal is always possible,
merely expensive.

What is needed is a currency whose signatures are quantum-resistant from the
genesis block, and whose settled history is protected by proof: rewriting it
must not be unlikely — it must cost a provable, automatically executed
destruction of the attacker's money. Quanta is built around those two
requirements. Everything else — the transport, the ledger, the emission —
exists to serve them.

## 2. Coins and Signatures

We define a coin as an entry in a replicated ledger, spendable only by an
ML-DSA-65 signature verifying under the public key committed in the sender's
address. All amounts are integers in µQTA (1 QTA = 10^6 µQTA); no floating
point exists on any money path.

```
address           a = BLAKE3(dom_addr ‖ pk)                 pk : ML-DSA-65 (FIPS 204)
multisig (m-of-n) a = BLAKE3(dom_msig ‖ sort(pk_1…pk_n) ‖ m)
```

The address commits to the key; the key cannot change without the address
changing. A multisig address commits to an entire policy — the sorted key set
and the threshold — so the policy cannot be rebound after funds arrive. A
multisig spend is valid iff it carries at least m distinct valid signatures
under keys of the committed set.

Ed25519 remains only where money is not: the QUIC node identity of the
transport library (§9). Every signature that moves value, votes finality, or
authenticates a network envelope is ML-DSA-65.

## 3. Supply

Emission is a closed formula of the chain state, not a policy.

```
E_tick  = (S_max − M) / D            S_max = 10^8 QTA,   D = 5·10^7
M_n     = S_max · (1 − (1 − 1/D)^n)  after n ticks (one tick per minute)
n_half  = D · ln 2 ≈ 3.47·10^7 ticks ≈ 66 years to half the remaining supply
burn(x) = ⌊x / 100⌋                  on every transfer of x µQTA
```

Each minute the network mints E_tick — large early, geometrically decaying,
never reaching the cap. There is no premine: the genesis state contains no
balance. There is no issuance authority: a block whose emission exceeds
E_tick is invalid to every node.

The ledger maintains one conservation invariant, checked at every block:

```
Σ_accounts (spendable + staked + unbonding) + burned = minted ≤ S_max
```

Staking moves coins between compartments; slashing moves them to burned;
nothing creates or loses a µQTA. A chain that violates the equation is not a
valid Quanta chain.

## 4. Consensus

Each slot (one per block height), a proposer is elected among validators,
weighted by stake enrolled on the chain itself — never by any local view.

```
beacon = BLAKE3(dom_b ‖ hash(B_{h−L}) ‖ slot)      B_{h−L}: block L slots behind the tip
seed   = BLAKE3(dom_s ‖ beacon ‖ slot ‖ round)
P(validator i proposes) = s_i / S                   s_i: bonded stake,  S = Σ s_j
```

The election is deterministic and publicly verifiable by every node from the
chain alone. It is not a VRF: the proposer is publicly predictable one slot
ahead (§9). The buried beacon denies a proposer any immediate grinding on his
own block. If the elected proposer is silent for 30 s, the election falls
back to the next in line, up to three rounds; while nobody has staked, sealing
is permissionless so the network can be born.

Eligibility is enforced on reception, not merely on production: a node
rejects any block whose proposer was not a bonded validator in the parent
state. Stake enters and leaves through ordinary signed transactions;
withdrawal completes U = 10,080 blocks after it is requested (§5).

## 5. Finality

Every E = 32 blocks, the epoch boundary is a checkpoint. Validators sign
finality votes — (source → target) checkpoint pairs — with the same ML-DSA-65
keys that hold their money. Votes accumulate into certificates:

```
cert(C) valid  ⟺  3 · Σ_{v ∈ V(C)} s_v  ≥  2 · S
```

A certified checkpoint is justified; two consecutive justified links finalize
the elder. Below the finalized floor, the chain is stone: every node refuses
any fork that would replace a finalized block, whatever its length.

**Theorem (accountable safety).** If two conflicting checkpoints are ever
finalized, then validators together holding at least S/3 signed contradictory
votes, each is identified by its own signatures, and each loses its entire
stake — bonded and unbonding alike.

*Sketch.* Two conflicting finalizations require two ⅔-quorums; two ⅔-quorums
intersect in at least S/3, and (as in Casper FFG) every validator in the
intersection produced either a double vote or a surrounding vote. The pair of
ML-DSA signatures is itself the proof: it is embedded in a slashing
transaction, re-verified independently by every node, and burns the
offender's stake. The slashing window equals the unbonding period,

```
W_slash = U = 10,080 blocks
```

so leaving the validator set does not outrun the punishment.

Bitcoin makes rewriting history exponentially expensive; Quanta makes it cost
one third of the money, by proof.

## 6. Network

Nodes connect over QUIC and exchange nine message types by gossip. The
transport key exchange is the hybrid X25519MLKEM768 — a quantum adversary
recording today's traffic decrypts nothing later. Every envelope is signed
ML-DSA-65 and crosses nine gates before touching any state:

```
① size ≤ 10 MB          ② decode                ③ sender not banned
④ dedup (LRU 10^5)      ⑤ |Δt| ≤ 90 s           ⑥ rate ≤ √(peers/4)·30/min
⑦ nonce monotone        ⑧ verify ML-DSA         ⑨ dispatch
```

Chain synchronization moves at most 50 blocks per request, four windows in
flight. Deep partitions heal by the same rule every node applies alone:
longest chain above the finalized floor wins, ties break lexicographically —
below the floor, nothing moves.

## 7. Validation

A block admits no uncovered spend. Processing its transactions sequentially
against the on-chain balances before the block — intra-block credits counted,
mempool never consulted — every debit must be covered at its turn. One
function enforces this everywhere: it validates received blocks, filters
produced ones, and re-checks every block of a candidate fork on a trial copy
before any reorganization. A node can neither accept an overdraft nor seal
one; it cannot corrupt its own chain.

## 8. Incentive

Each tick mints E_tick and divides it by measured contribution — energy,
work, validation, uptime — using Shapley values:

```
share_i = φ_i / Σ_j φ_j
```

A solo node earns the full tick. Rewards are ordinary coins under ordinary
addresses; mining is the only issuance, and the 1% transfer burn the only
sink. Validators are not paid to vote; they stake to be elected, and lose the
stake if they equivocate.

## 9. Limitations

Stated plainly, because trust is built on what a system admits.

- The proposer election is predictable one slot ahead; a cryptographic VRF
  and an anti-grinding VDF are future work.
- The transport node identity (QUIC endpoint) is still Ed25519 — an upstream
  library constraint, outside this codebase; it switches the day upstream
  ships post-quantum endpoint identities.
- Declared energy readings weight a share of emission (§8); they are outside
  the consensus security path — validator weight is on-chain stake only —
  but they are an economic gaming surface under study.
- The live network is small; the properties above are enforced by every node
  and exercised in deterministic simulation, not yet proven at scale.
- No third-party audit has been performed yet. The audit-readiness package
  lives in `docs/audit/`.
- QUANTA has no market and no price. This document values nothing.

## 10. Calculations

The proposer lottery gives an attacker with stake fraction q each slot with
probability q. Sealing an entire epoch alone requires winning 32 consecutive
slots:

```
q = 0.10      P = 10^−32
q = 0.30      P ≈ 2·10^−17
q = 0.45      P ≈ 8·10^−12
```

But the lottery is not the wall. Finalizing a conflicting history is not a
matter of luck at any q: it requires certified signatures from two thirds of
the stake, hence (§5) the provable destruction of at least one third. Below
the floor, reversal is not improbable — it is priced, and the price is
automatic.

## 11. Conclusion

We have proposed a currency without a promiser: no issuer, no server, no
account to freeze, and no signature a quantum computer retires. Coins are
ML-DSA-65 signature chains under hash-committed addresses; supply is a closed
formula converging to a hard cap; proposers are elected by on-chain stake;
and history hardens into certificates whose violation destroys a third of the
money that signed it. The rules are few, and every node checks all of them.

---

*Protocol `TORUS_PROTOCOL_VERSION = 6` · Apache-2.0 · The reference
implementation, its test suite and its deterministic consensus simulation
live in this repository.*
