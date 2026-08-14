# Quanta: A Post-Quantum Peer-to-Peer Currency with Irreversible Finality

> Status: alpha. A third-party audit was delivered on 13 August 2026: 85
> findings, 13 of them critical. The reports are published in
> `docs/audit/2026-08-13/`, and what was fixed, how it is known and what stays
> open in `docs/audit/REMEDIATION-2026-08-13.md`. QUANTA has no market and no
> price; none is claimed or predicted anywhere in this document. Every constant
> below is read from the code of version 3.16.0
> (`TORUS_PROTOCOL_VERSION = 10`). Those that bind consensus — the supply cap,
> the emission, the reward split, the quorum, the finality and unbonding
> windows — are recomputed and enforced by every node, not merely applied by
> the reference implementation; the transport bounds are local policy,
> identical in that implementation but imposed by no consensus rule.

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
1 QTA equals 10^6 µQTA: balances, emission, the reward split and the burn are
integer arithmetic, and no floating-point number decides an amount, which
eliminates rounding drift between nodes by construction. One float does sit on
the consensus path — the energy reading a block declares, which enters the
header hash as its canonical IEEE-754 bit pattern, so no relay can rewrite the
field without changing the block. It moves no coin.

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
E_block = 2 · E_tick                 one block is sealed every two ticks
M_n     = S_max · (1 − (1 − 1/D)^n)  after n ticks (one tick per minute)
n_half  = D · ln 2 ≈ 3.47·10^7 ticks ≈ 66 years to half the remaining supply
burn(x) = ⌊x / 100⌋                  on every transfer of x µQTA
```

The first line says everything: each minute, the network mints the fraction
1/D of what remains to be minted. Emission is therefore high in the early
days, decays geometrically, and never reaches the cap. A block is sealed every
two ticks and carries both, so `E_block` is what one block is worth. `M_n` is
the closed form of the minted supply and `n_half` gives the human scale, about
sixty-six years to mint half of whatever remains, from any point in time. The
burn is the only sink: one percent of every transfer is destroyed, which makes
the circulating supply slowly deflationary as the currency is used.

There is no premine: the genesis state contains no balance, not even for the
project's author. There is no issuance authority: a block is worth exactly
`E_block`, a pure function of the chain that every node recomputes from the
supply minted before that block, and a block minting more is invalid to all of
them. A block may mint less, or nothing at all — a producer renouncing its
reward is strictly non-inflationary — but none can mint above the schedule.
The ledger finally maintains one conservation invariant, checked at every
block:

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
beacon = BLAKE3(dom_b ‖ hash(B_{h−L}) ‖ slot)      B_{h−L}: block L = 2 slots behind
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
state, so a malicious node cannot crown itself. The rule has one deliberate
opening, paced by height: one block in sixteen is an open slot that any
address may propose, bonded or not. Without it the first staker would close
the network to every newcomer, since there is no faucet and no premine — a new
address would need a coin to stake, stake to propose, and propose to earn its
first coin. What that opening costs is stated in section 9. Stake enters and
leaves through ordinary signed transactions, visible to all; withdrawal
completes U = 10,080 blocks after it is requested, about two weeks at the
nominal rhythm, and this slowness is a security piece the next section
explains.

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
whoever its author. Above the floor the longer chain wins, and at equal height
the tie is settled by the stake-weighted election rank of the two proposers —
the very ranking that designates who may seal the slot, drawn from the buried
beacon, the height, and the bonded set as of the parent. None of those three
inputs sits in either competing block, so regrinding a block to obtain a
better hash buys nothing; winning a tie requires stake, hence something to
lose. The hash settles only what the rank does not separate: two blocks from
one proposer, which is a slashable equivocation, and the open slot, where a
proposer below the minimum stake holds no rank at all. Two partitions that
meet again adopt the same branch without exchanging a word beyond their
blocks.

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

Nodes connect over QUIC and exchange eleven message types by gossip: presence,
chain segment request and delivery, new blocks, transactions, username
registration, ping and pong, peer reporting, finality votes and fault proofs.
The transport key exchange is the hybrid X25519MLKEM768, the combination of a
classical curve and the post-quantum standard ML-KEM-768: a quantum adversary
recording today's traffic will decrypt none of it tomorrow, and if either
mechanism fell, the other would hold alone.

Every envelope is ML-DSA-65 signed, and the order in which it is checked is
itself the security property:

```
①  size ≤ 4 MiB                    ②  JSON decode
③  header field shape, O(1)        ④  ban probe, read-only
⑤  verify ML-DSA-65 over the canonical pre-image   ← authentication gate
⑥  evict an expired ban            ⑦  id = BLAKE3(that same pre-image)
⑧  |Δt| ≤ 90 s                     ⑨  dedup probe (LRU 10^5)
⑩  per-peer byte accounting        ⑪  dedup insert
⑫  rate ≤ 30·max(1, √(peers/4)) per minute, capped at 120, then monotone nonce
⑬  dispatch
```

Authentication precedes every write. What runs before the signature is O(1)
and mutates nothing: a size bound, a decode, a shape check on the
fixed-length header fields, and a read-only probe of the ban table. What
costs work or leaves a trace comes after — the canonical identifier, the
deduplication cache, the per-peer counters, the rate window, the nonce
high-water mark — because until the signature verifies, the sender field is a
string the attacker chose. Deduplicating before authenticating is not a
stylistic preference but a concrete attack: it lets an unauthenticated peer
seat identifiers of its choosing in the deduplication cache and censor a
peer's chain synchronization for free, and it bills honest peers for bytes
they never sent. The size bound and the cache close floods; timestamp
freshness and the strictly increasing nonce close replay; the rate limit
adapts to the size of the network, from thirty messages per peer per minute
up to a ceiling of a hundred and twenty. Chain synchronization moves at most
fifty blocks per request, or three mebibytes, whichever comes first, four
windows in flight, with optional compression. The node's full state is
photographed to disk every thirty seconds: a power cut costs at worst half a
minute of local state, never the chain.

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
reorganization, ever, descends below the finality floor. A second bound,
independent of finality, refuses any reorganization deeper than 128 blocks
whatever its score. The price is named rather than hidden: past that depth a
partition no longer heals on its own, and resynchronizing takes an explicit
operator action.

## 8. Incentive

A block mints E_block, and that number is not chosen by whoever seals it: it
is a pure function of the chain, recomputed by every receiver from the supply
minted before the block. No local measurement enters the money path. Energy,
uptime and work claimed by a peer are self-declared, hence unverifiable by
construction; they were removed from issuance and remain display signals only.
A reward derived from the chain alone is identical on every node, which is
what lets a receiver recompute it instead of merely bounding it.

The split is recomputed the same way, never trusted:

```
producer      = E_block/2  +  whatever integer division leaves over
participant i = (E_block − E_block/2) · b_i / Σ_j b_j        i ≠ producer
b_i           = blocks produced by i within the last W = 32 blocks
```

Half goes to the producer of the block, the rest to the other addresses that
produced a block within the last thirty-two, in proportion to how many each
produced. No µQTA is lost on the way: what integer division leaves over
returns to the producer, so the plan sums exactly to the reward. The weight is
blocks, not addresses: slots are a finite resource, so splitting one identity
into K produces no extra block and earns nothing extra. Sharing equally
between distinct addresses was the earlier rule, and it subsidized identity
duplication — twenty-eight identities captured 45.2% of every reward where a
single one earned 12.5%. Every receiving node recomputes the whole plan and
rejects a block that departs from it, so a producer cannot keep more than its
half, nor lower the others' share without lowering its own in the same
proportion. A block may carry less than the full reward, or none;
on a chain with no other recent participant, the producer takes it all.

Rewards are ordinary coins under ordinary addresses; block production is the
only issuance, and the one-percent transfer burn the only sink. Validators are
not paid to vote: they stake to be elected proposers, and lose the stake if
they equivocate. The application makes this cycle visible: it runs the node in
the background, shows every reward the instant the chain writes it, and lets
you send and receive through a simple @username, resolved on-chain to its
owner's address.

## 9. Limitations

Stated plainly, because trust is built on what a system admits. The proposer
election is predictable one slot ahead; a cryptographic VRF, which would keep
the winner unknown until revealed, and an anti-grinding VDF are future work.
The transport node identity, the QUIC endpoint, is still Ed25519: an upstream
library constraint, outside this code, switching the day upstream ships
post-quantum endpoint identities. One block in sixteen is an open slot any
address may propose, staked or not; a protocol cannot be permissionless,
Sybil-resistant and free at once, so free entry is bought in Sybil resistance.
The price is bounded and paced by height rather than by the number of
claimants: a farm of identities captures at most that sixteenth of the
emission, however many identities it holds, and never more. Declared energy
readings weigh nothing — they left the money path — but the network total the
application displays remains a sum of self-declarations and should be read as
one. The live network is small; the properties in this document are enforced
by every node and exercised in multi-seed deterministic simulation, not yet
proven at scale. The third-party audit of 13 August 2026 returned 85 findings,
13 of them critical, and forced a protocol break from v9 to v10 together with
a genesis replay; its reports are published in `docs/audit/2026-08-13/` and the
remediation, including what stays open, in
`docs/audit/REMEDIATION-2026-08-13.md`. Finally, QUANTA has no market and no
price, and this document values nothing.

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

*Quanta 3.16.0 · protocol `TORUS_PROTOCOL_VERSION = 10` · chain id
`quanta-mainnet-v10` · Apache-2.0 · The reference implementation, its test
suite and its deterministic consensus simulation live in this repository.*
