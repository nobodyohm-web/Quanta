# Quanta — Security Audit Readiness Package

This directory is the self-contained entry point for an external security review of the
Quanta protocol. It exists so that an auditor (or a funding body such as OSTIF) can
understand the system, its claimed guarantees, and the proposed scope in one sitting,
without reverse-engineering the repository.

| | |
|---|---|
| Project | Quanta — sovereign P2P cryptocurrency (no server, no cloud, no issuer) |
| License | Apache-2.0 |
| Repository | <https://github.com/nobodyohm-web/Quanta> |
| Language | Rust (protocol/backend, Tauri 2.0) + Svelte 5 (desktop UI, out of scope) |
| Review baseline | `main` at v3.16.0, `TORUS_PROTOCOL_VERSION = 10` — a dedicated `audit-baseline` tag will be frozen at engagement start |
| Status | ⚠️ Alpha. **One external review has been performed** (2026-08-13, 85 findings, 13 critical) and its reports are published in [`2026-08-13/`](2026-08-13/). It was a paid single-reviewer engagement, not an audit by an established firm, and it is not a substitute for one. |

## Contents

- [`../ARCHITECTURE.md`](../ARCHITECTURE.md) — **read this first.** A guided tour of the
  protocol ending in a table of every claimed invariant, the single place it is
  enforced, and the test that proves it. Written to be read start to finish.
- [`THREAT-MODEL.md`](THREAT-MODEL.md) — what the system protects, against whom, the
  claimed security properties, trust boundaries, and the known accepted risks
  (stated honestly, including what is *not* yet defended).
- [`SCOPE.md`](SCOPE.md) — prioritized audit scope with real line counts, the key
  invariants per area, and suggested engagement shapes.
- [`RFQ.md`](RFQ.md) — request-for-quote template and OSTIF application notes.
- [`2026-08-13/`](2026-08-13/) — **the external review's own reports, published verbatim**:
  six documents by area plus one executable PoC. Nothing was removed or softened, including
  the passages showing that an earlier fix by this project was wrong.
- [`REMEDIATION-2026-08-13.md`](REMEDIATION-2026-08-13.md) — what was fixed, how each fix
  was proven, and what remains open with the reason. Read it next to the reports above,
  not instead of them.
- [`../archive/audits/`](../archive/audits/) — earlier reports kept for history, including
  the internal adversarial audit of 2026-07-25. The 2026-06-10 report predates the
  crypto-only refactor that removed the web/social modules, so it audits a codebase that
  no longer exists; it is **not** a current document.

## Consensus changes since the internal audit (please review these first)

Three consensus rules changed after the 2026-07-25 internal audit. They are the newest,
least-reviewed code in the repository and deserve the sharpest attention:

| Fork | Rule | What it replaced |
|---|---|---|
| v8 | **MINT-EXACT-1** — the block reward is `emission_for_block(prior mined supply)`, a pure function of the chain, recomputed by every receiver | A loose upper bound of `64 × emission_for_tick` against an honest amount of `2 × emission_for_tick / N` — a `32 × N` margin, i.e. 3 200× the legitimate reward on a 100-node network |
| v8 | **OPEN-DOOR-1** — one block in 16 is proposable by any address | `PROPOSER-1` closed the network permanently at the first staker; no faucet, airdrop or premine exists to break the cycle |
| v9 | **REWARD-SHARE-1** — the reward is split between the producer and recent participants, recomputed and imposed by every node | Only the sealer was paid; finality voters, whose ML-DSA broadcasts cost the most bandwidth on the network, earned nothing |

A fourth change is not a consensus rule but is the most consequential defect found to date:
between fork v4 (2026-07-18) and 2026-08-01 the node was **mute on any real network**.
`iroh-gossip` caps a message at 4 096 B while an ML-DSA-signed envelope is ~15 KB; the send
failed silently and was still counted in `stats.messages_sent`. Every test was green
throughout. This is documented in
[`../ARCHITECTURE.md` §7](../ARCHITECTURE.md#7-four-bugs-that-shaped-the-design) and is the
reason the project now treats in-process tests as insufficient evidence for any P2P claim.

## What Quanta is, in three sentences

Quanta is a desktop-native cryptocurrency with a hard cap of 100M coins, zero premine
and no issuing authority. Consensus is Proof-of-Stake (deterministic stake-weighted
leader election) with a Casper-FFG-style finality gadget: ML-DSA-signed votes, ⅔-stake
certificates, accountable slashing (double vote + surround, reaching unbonding stake).
Its distinguishing property is **end-to-end post-quantum cryptography**: account
authority and finality votes are ML-DSA-65 (FIPS 204), gossip envelopes are
ML-DSA-signed, and the QUIC/TLS 1.3 transport negotiates hybrid X25519MLKEM768 —
the only remaining classical primitive is the Iroh endpoint identity (upstream
limitation, documented in the threat model).

## Building and verifying the claims

```bash
# Full test suite — 608 library tests + 1 integration test, 0 failures at baseline.
# Includes a deterministic simulation harness (multi-seed DST) and a 128-run
# determinism check (C1) of the consensus core.
cargo test --manifest-path src-tauri/Cargo.toml

# Lint gate enforced on every commit
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Key dependencies (from `src-tauri/Cargo.toml`): `fips204` 0.4.6 (ML-DSA-65 — pure
Rust, constant-time, no `unsafe`), `ed25519-dalek` 2.2, `blake3` 1.8, `aes-gcm` 0.10,
`argon2` 0.5, `zeroize` 1.8, `iroh` 0.98 (QUIC, `default-features = false` +
aws-lc-rs TLS provider), `iroh-gossip` 0.98, `rustls` 0.23 (`prefer-post-quantum`),
`libsql` 0.9, `crdts` 7.

Supply-chain checking is enforced continuously, not only at baseline: every pull request
runs `cargo-deny check` (advisories · licenses · sources · bans) as a required CI job, with
the ignore list documented in [`deny.toml`](../../deny.toml).

## Baseline tooling results (2026-07-20, superseded)

> These figures describe the July baseline and are kept because the remediation is
> judged against them. As of 2026-08-14 the supply-chain gate is green: `cargo deny
> check` passes on advisories, bans, licenses and sources, and `npm audit
> --audit-level=high` is clean. See §SC of the remediation report.

`cargo audit` (RustSec advisory DB) on the baseline: **8 vulnerabilities, 24 warnings —
all in transitive dependencies**, none in Quanta code. The four affected crates are
`rustls-webpki` 0.102.8 (×4: CRL parsing panic, URI/wildcard name-constraint acceptance,
CRL distribution-point matching), `quick-xml` 0.38.4 (×2, severity 7.5: memory-exhaustion
and quadratic-time DoS), and `hickory-proto`/`hickory-net` 0.26.0-beta.4 (×2: DNS
message-encoding CPU exhaustion, NSEC3 validation loop). All have published fixes, but
each is pinned by a parent constraint (`iroh` 0.98 pins hickory; rustls 0.23 in-tree wants
webpki ^0.102; the Tauri chain wants quick-xml ^0.38) — resolving them requires parent
version bumps (tracked as a dependency-refresh work item), not a lockfile update.
Assessment: none sits on a Quanta-controlled network path with attacker-supplied input
(no CRLs or name-constrained certificates are used; quick-xml parses local bundle data;
the hickory items require a hostile DNS resolver during relay discovery — a bounded DoS,
not a compromise). The warnings are unmaintained/unsound advisories dominated by the
Linux GTK3 binding family.

## Disclosure policy

The full audit report — findings included — is published in this repository, in
[`2026-08-13/`](2026-08-13/). A hidden report is worth nothing; the project treats an
audit as a public artifact, including the parts that are unflattering.

## Contact

Open an issue on the repository, or contact the maintainer through the repository
profile.
