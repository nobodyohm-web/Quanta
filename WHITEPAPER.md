# Quanta: A Post-Quantum Peer-to-Peer Currency with Irreversible Finality

> Status: alpha, not yet audited by a third party. QUANTA has no market and no
> price; none is claimed or predicted anywhere in this document. Every constant
> below is carved in the code and verified by every node at every block.

**Abstract.** A purely peer-to-peer currency must survive the two failures its
predecessors accept. Elliptic-curve signatures fail against a quantum
adversary: everything signed today can be forged the day such a machine
exists. Probabilistic settlement fails against patience: a Bitcoin block is
never final, only exponentially unlikely to be reversed. Quanta answers both.
Account authority, finality votes and network envelopes are ML-DSA-65
signatures, the post-quantum standard FIPS 204; the transport negotiates a
hybrid post-quantum key exchange; and history becomes irreversible by
certificate rather than by probability: a checkpoint carrying signatures from
two thirds of the enrolled stake is final, and finalizing a conflicting
history provably destroys at least one third of that stake. Supply is a
closed form: one hundred million coins, zero premine, an emission decaying
geometrically toward the cap.

---

## 1. Introduction

Electronic money rests on promises. A fiat currency rests on the restraint of
its central bank; a platform balance rests on a server that can be seized or
frozen. Peer-to-peer currencies removed the promiser, but they kept two
quieter assumptions: that discrete-logarithm signatures would never be broken,
and that a history buried under enough work would be safe enough.

Both assumptions carry expiry dates. Elliptic-curve signatures are broken by
a sufficiently large quantum computer, and the threat does not wait for the
machine: traffic and public keys recorded today can be exploited
retroactively the day it arrives — the attack known as "harvest now, decrypt
later". As for work-buried history, it remains a probability, not a fact:
reversal stays possible, merely expensive, and the expense is negotiable.

What is needed is a currency whose signatures resist the quantum computer
from the genesis block, and whose settled history is protected by proof.
Rewriting it must not be unlikely: it must cost a provable, automatic
destruction of the attacker's money. Quanta is built around these two
requirements. Everything else, the transport, the ledger, the emission, the
application, exists to serve them.

## 2. Coins, Keys and Signatures

A coin, in Quanta, is an entry in a ledger replicated by every node,
spendable only by an ML-DSA-65 signature verifying under the public key
committed in the sender's address. All amounts are integers in µQTA, where
1 QTA equals 10^6 µQTA; no floating-point number exists on any money path,
which eliminates rounding drift between nodes by construction.

```
address           a = BLAKE3(dom_addr ‖ pk)                 pk : ML-DSA-65 (FIPS 204)
multisig (m-of-n) a = BLAKE3(dom_msig ‖ sort(pk_1…pk_n) ‖ m)
```

The address commits to the key through a domain-separated hash: the key
cannot change without the address changing, and nobody can substitute a key
under an existing address. For human use the address is written in Bech32m
under the `qta1…` prefix, with a checksum: a typing mistake is caught before
a single µQTA leaves. A multisig address commits to an entire policy, the
sorted key set and the threshold, so the policy cannot be rebound once funds
have arrived. A multisig spend is valid if and only if it carries at least m
valid signatures from distinct signers of the committed set. This is fully
post-quantum quorum custody, built without waiting for a threshold signature
scheme to be standardized for lattice cryptography.

The account key is born from a 32-byte seed that the user backs up as a
24-word recovery phrase, the BIP39 standard every wallet holder knows. On the
machine, the seed lives in an encrypted vault: the encryption key is derived
from the password with Argon2id, a memory-hard function that prices hardware
brute force out of reach, and the content is sealed with AES-256-GCM,
authenticated encryption. On macOS a fingerprint may unlock the vault: a
random envelope key lives in the system keychain behind biometry, the
password is never stored and remains the fallback. Every secret is wiped from
memory the moment it has served.

Ed25519 survives only where money is not: the QUIC node identity of the
transport library, an upstream constraint detailed in section 9. Every
signature that moves value, votes finality or authenticates a network
envelope is ML-DSA-65.

## 3. Supply

Emission is a closed formula of the chain state, not a policy a committee
could amend.

```
E_tick  = (S_max − M) / D            S_max = 10^8 QTA,   D = 5·10^7
M_n     = S_max · (1 − (1 − 1/D)^n)  after n ticks (one tick per minute)
n_half  = D · ln 2 ≈ 3.47·10^7 ticks ≈ 66 years to half the remaining supply
burn(x) = ⌊x / 100⌋                  on every transfer of x µQTA
```

The first line says everything: each minute, the network mints the fraction
1/D of what remains to be minted. Emission is therefore high in the early
days, decays geometrically, and never reaches the cap: the second line is its
closed form, the third gives the human scale, about sixty-six years to mint
half of whatever remains, from any point in time. The fourth line is the only
sink: one percent of every transfer is burned, which makes the circulating
supply slowly deflationary as the currency is used.

There is no premine: the genesis state contains no balance, not even for the
project's author. There is no issuance authority: a block whose emission
exceeds E_tick is invalid to every node, each of which recomputes the bound
itself. The ledger finally maintains one conservation invariant, checked at
every block:

```
Σ_accounts (spendable + staked + unbonding) + burned = minted ≤ S_max
```

Staking moves coins between compartments of one account; slashing moves them
to burned; nothing creates or loses a µQTA. A chain violating this equation
is not a valid Quanta chain, and no node will follow it.

## 4. Consensus

At every slot, that is at every block height, a proposer is elected among
validators, weighted by stake enrolled on the chain itself, never by a local
view or a declared reputation. This is an essential security point: a
validator's weight is a pure function of the chain, hence identical on every
node, whether it has lived since the first block, been restored from a
backup, or freshly synchronized.

```
beacon = BLAKE3(dom_b ‖ hash(B_{h−L}) ‖ slot)      B_{h−L}: block L slots behind the tip
seed   = BLAKE3(dom_s ‖ beacon ‖ slot ‖ round)
P(validator i proposes) = s_i / S                   s_i: bonded stake,  S = Σ s_j
```

The beacon derives from a block buried L slots behind the tip: a proposer
cannot reshape his own block to influence the election that follows it, since
that election's seed was frozen far behind him. The election is deterministic
and publicly verifiable by every node from the chain alone; it is not a VRF,
so the proposer is publicly predictable one slot ahead, a limitation owned in
section 9. If the elected proposer stays silent for thirty seconds, the
election falls back to the next in line, up to three rounds; and while nobody
has staked, sealing is permissionless, so the network can be born without
anyone's permission.

Eligibility is enforced on reception, not merely on production: a node
rejects any block whose proposer was not a bonded validator in the parent
state, so a malicious node cannot crown itself. Stake enters and leaves
through ordinary signed transactions, visible to all; withdrawal completes
U = 10,080 blocks after it is requested, about two weeks at the nominal
rhythm, and this slowness is a security piece the next section explains.

## 5. Finality

Every E = 32 blocks, the epoch boundary is a checkpoint. Validators sign
finality votes, source-to-target pairs, with the same ML-DSA-65 keys that
hold their money: voting stakes the same matter as owning. Votes accumulate
into certificates:

```
cert(C) valid  ⟺  3 · Σ_{v ∈ V(C)} s_v  ≥  2 · S
```

A certified checkpoint is called justified; two consecutive justified links
finalize the elder. Below this finalized floor the chain is stone: every node
refuses any fork that would replace a finalized block, whatever its length,
whoever its author. Above the floor, forks are settled by the simplest rule
that converges: the longest chain wins, ties break lexicographically, and two
partitions that meet again adopt the same branch without exchanging a word
beyond their blocks.

**Theorem (accountable safety).** If two conflicting checkpoints are ever
finalized, then validators together holding at least S/3 signed
contradictory votes, each is identified by its own signatures, and each
loses its entire stake, bonded and unbonding alike.

*Proof sketch.* Two conflicting finalizations require two two-thirds quorums;
two two-thirds quorums of one total intersect in at least one third; and, as
in Casper FFG, every validator in the intersection necessarily produced
either a double vote, two votes for targets of the same height, or a
surrounding vote, one vote enclosing another. In both cases the pair of
ML-DSA signatures is itself the proof, non-repudiable. It is embedded in a
slashing transaction that every node re-verifies independently before
applying, and that burns the offender's stake. The slashing window equals the
unbonding period,

```
W_slash = U = 10,080 blocks
```

so leaving the validator set does not outrun the punishment: a validator
remains punishable until his withdrawal completes. That is why withdrawal is
slow.

Bitcoin makes rewriting history exponentially expensive; Quanta makes it cost
one third of the money, by proof.

## 6. The Network

Nodes connect over QUIC and exchange nine message types by gossip: presence,
chain segment request and delivery, new blocks, transactions, username
registration, liveness and reporting. The transport key exchange is the
hybrid X25519MLKEM768, the combination of a classical curve and the
post-quantum standard ML-KEM-768: a quantum adversary recording today's
traffic will decrypt none of it tomorrow, and if either mechanism fell, the
other would hold alone.

Every envelope is ML-DSA-65 signed and crosses nine gates before touching any
state:

```
① size ≤ 10 MB          ② decode                ③ sender not banned
④ dedup (LRU 10^5)      ⑤ |Δt| ≤ 90 s           ⑥ rate ≤ √(peers/4)·30/min
⑦ nonce monotone        ⑧ verify ML-DSA         ⑨ dispatch
```

The size bound and deduplication close floods; timestamp freshness and the
strictly increasing nonce close replay; the rate limit adapts to the size of
the network; and nothing is processed without a valid signature. Chain
synchronization moves at most fifty blocks per request, four windows in
flight, with optional compression. The node's full state is photographed to
disk every thirty seconds: a power cut costs at worst half a minute of local
state, never the chain.

The node also exists without an interface: a headless daemon exposes the same
chain through a seventeen-method JSON-RPC API, enough to query a block, a
balance, finality, the proven supply, to submit a signed transaction or scan
deposits, which is what an explorer, a service or an exchange expects. The
desktop application and the daemon share one core, booted by one code path.

## 7. Validation

A block admits no uncovered spend. Processing its transactions sequentially
against the on-chain balances before the block, intra-block credits counted,
mempool never consulted, every debit must be covered at its turn. One single
function enforces this rule everywhere: it validates blocks received from the
network, it filters the blocks the node produces, so that a self-sealed block
passes by construction the validation others will apply to it, and it
re-checks every block of a candidate fork on a trial copy before any
reorganization. A node can neither accept an overdraft, nor seal one, nor
corrupt its own chain through a careless reorganization; and no
reorganization, ever, descends below the finality floor.

## 8. Incentive

Each tick mints E_tick and shares it according to measured contribution, the
energy committed, the work done, the validation rendered and the uptime, by
Shapley values, the division rule that grants each participant his average
marginal contribution:

```
share_i = φ_i / Σ_j φ_j
```

A solo node earns the full tick. Rewards are ordinary coins under ordinary
addresses; mining is the only issuance, and the one-percent transfer burn the
only sink. Validators are not paid to vote: they stake to be elected
proposers, and lose the stake if they equivocate. The application makes this
cycle visible: it mines in the background, shows every reward the instant the
chain writes it, and lets you send and receive through a simple @username,
resolved on-chain to its owner's address.

## 9. Limitations

Stated plainly, because trust is built on what a system admits. The proposer
election is predictable one slot ahead; a cryptographic VRF, which would keep
the winner unknown until revealed, and an anti-grinding VDF are future work.
The transport node identity, the QUIC endpoint, is still Ed25519: an upstream
library constraint, outside this code, switching the day upstream ships
post-quantum endpoint identities. Declared energy readings weight a share of
emission; they sit outside the consensus security path, a validator's weight
being on-chain stake and nothing else, but they remain an economic gaming
surface under study. The live network is small; the properties in this
document are enforced by every node and exercised in multi-seed deterministic
simulation, not yet proven at scale. No third-party audit has taken place
yet; the complete readiness package, threat model, measured scope and a
commitment to publish the full report, lives in `docs/audit/`. Finally,
QUANTA has no market and no price, and this document values nothing.

## 10. Calculations

The proposer lottery gives each slot to an attacker holding stake fraction q
with probability q. Sealing an entire epoch alone requires winning
thirty-two consecutive slots:

```
q = 0.10      P = 10^−32
q = 0.30      P ≈ 2·10^−17
q = 0.45      P ≈ 8·10^−12
```

But the lottery is not the wall. Finalizing a conflicting history is not a
matter of luck at any q: it requires certified signatures from two thirds of
the stake, hence, by the theorem of section 5, the provable destruction of at
least one third. Below the floor, reversal is not improbable: it is priced,
and the price is automatic.

## 11. Conclusion

We have proposed a currency without a promiser: no issuer, no server, no
account to freeze, and not one signature a quantum computer retires. Coins
are ML-DSA-65 signature chains under hash-committed addresses; supply is a
closed formula converging to a hard cap; proposers are elected by on-chain
stake; and history hardens into certificates whose violation destroys a third
of the money that signed them. The rules are few, and every node checks all
of them.

---

*Protocol `TORUS_PROTOCOL_VERSION = 6` · Apache-2.0 · The reference
implementation, its test suite and its deterministic consensus simulation
live in this repository.*
