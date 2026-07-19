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
| Review baseline | commit `97123d3a4ac6cfae2f2fd76456d1bc173027b4fa` (v3.10.0) — a dedicated `audit-baseline` tag will be frozen at engagement start |
| Status | ⚠️ Alpha. **No third-party audit has been performed yet.** That is precisely what this package is for. |

## Contents

- [`THREAT-MODEL.md`](THREAT-MODEL.md) — what the system protects, against whom, the
  claimed security properties, trust boundaries, and the known accepted risks
  (stated honestly, including what is *not* yet defended).
- [`SCOPE.md`](SCOPE.md) — prioritized audit scope with real line counts, the key
  invariants per area, and suggested engagement shapes.
- [`RFQ.md`](RFQ.md) — request-for-quote template and OSTIF application notes.

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
# Full test suite — 449 library tests + 1 integration test, 0 failures at baseline.
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

## Baseline tooling results (2026-07-20)

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

The full audit report — findings included — will be published in this repository.
A hidden report is worth nothing; the project treats the audit as a public artifact.

## Contact

Open an issue on the repository, or contact the maintainer through the repository
profile.
