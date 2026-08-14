# Quanta — Request for Quote & Funding Notes

This file provides a request-for-quote (RFQ) template for approaching audit firms
directly, and notes for applying to a funding body such as OSTIF. It carries no invented
figures — dates, scope, line counts and test counts come from this package and from the
repository at the stated baseline. See [`SCOPE.md`](SCOPE.md) for the priorities referenced
below, [`THREAT-MODEL.md`](THREAT-MODEL.md) for the security claims, and
[`2026-08-13/`](2026-08-13/) for the reports of the review that already took place.

**What is being asked for.** Not a first look at the code: a **second, independent** review.
A paid single-reviewer engagement was performed on 2026-08-13 (85 findings, 13 critical);
its reports are published verbatim, and the remediation — including what was left open — is
in [`REMEDIATION-2026-08-13.md`](REMEDIATION-2026-08-13.md). That engagement was not an
audit by an established firm and does not replace one.

## RFQ email template

> **Subject:** Security audit RFQ — Quanta (post-quantum P2P cryptocurrency, Rust, Apache-2.0)
>
> Hello,
>
> We are seeking an independent security review of Quanta, an open-source (Apache-2.0),
> sovereign P2P cryptocurrency written in Rust (Tauri backend; the Svelte desktop UI is out
> of scope). Its distinguishing property is end-to-end post-quantum cryptography: account
> authority and finality votes are ML-DSA-65 (FIPS 204), gossip envelopes are ML-DSA-signed,
> and the QUIC/TLS 1.3 transport negotiates hybrid X25519MLKEM768.
>
> A self-contained audit-readiness package lives in the repository under `docs/audit/`: a
> threat model, a prioritized scope with measured line counts, this RFQ, and the reports of
> the review already performed. We would like your help scoping and estimating the
> engagement.
>
> - **Repository:** https://github.com/nobodyohm-web/Quanta (Apache-2.0)
> - **Review baseline:** `main` at v3.16.0 (`TORUS_PROTOCOL_VERSION = 10`,
>   `CHAIN_ID = "quanta-mainnet-v10"`); we will freeze a dedicated `audit-baseline` tag at
>   engagement start.
> - **Prior review:** a paid **single-reviewer** external review, 2026-08-13 — 85 findings,
>   13 critical, published verbatim in `docs/audit/2026-08-13/` together with an executable
>   PoC. It was not an audit by an established firm. Remediation broke the protocol
>   (v9 → v10) and is documented finding by finding, with the items left open stated
>   explicitly, in `docs/audit/REMEDIATION-2026-08-13.md`. We are asking for the independent
>   review that this one is not.
> - **Package:** `docs/audit/README.md`, `THREAT-MODEL.md`, `SCOPE.md`, `2026-08-13/`,
>   `REMEDIATION-2026-08-13.md`.
> - **Suggested scope:** P0 = consensus core `sm/` (8_525 LOC), ledger `p2p/ledger/`
>   (9_864 LOC), cryptography `security/` (2_491 LOC); P1 = network pipeline, 8 files
>   (10_746 LOC). Line counts include inline `#[cfg(test)]` tests. The whole Rust backend is
>   44_067 lines. Engagement shapes (crypto / consensus / network) are described in
>   `SCOPE.md`.
> - **Where we expect the sharpest attention:** the post-audit code is the youngest in the
>   repository — canonical injective signing pre-images (`CANON-1`), sequential on-chain
>   nonces (`NONCE-ONCHAIN-1`), stake-weighted fork ranking (`FORK-RANK-1`), and the
>   reward-plan weighting. Also the items the first review states it did **not** cover: no
>   real network (everything ran in-process), no constant-time instrumentation, no fuzzing,
>   no review of `fips204` itself.
> - **Verification:** `cargo test` (608 library + 1 integration test, 0 failures at
>   baseline, including a seeded deterministic simulation harness — 64 seeds by default,
>   `QUANTA_SIM_SEEDS` to deepen, plus a fixed 128-seed sweep that proves the fault
>   generator produces partitions, network faults, equivocation and crash/restart);
>   `cargo clippy --all-targets -D warnings` is enforced, and `cargo-deny` (advisories ·
>   licenses · sources · bans) runs in CI as a blocking gate on every push and pull request.
>
> Could you let us know:
> 1. Whether the scope fits your practice and how you would slice it.
> 2. An estimate (effort/duration/cost) for the shape(s) you would recommend.
> 3. Your availability.
>
> We publish audit reports in full, findings included — the 2026-08-13 reports are in the
> repository, unedited, including the passages showing that one of our earlier fixes was
> wrong. A hidden report is worth nothing to us; we treat an audit as a public artifact.
>
> Thank you,
> [maintainer / contact]

## Notes for an OSTIF application

[OSTIF](https://ostif.org) helps open-source projects obtain and fund independent security
audits (it coordinates with firms such as Least Authority and Trail of Bits). Points to
lead with, all supportable from this package and the repository:

- **Open source, permissive license.** Apache-2.0, public repository.
- **Distinctive angle.** Quanta is positioned as an end-to-end post-quantum cryptocurrency:
  money, finality, gossip authentication, and transport key exchange are all post-quantum.
  The one remaining classical primitive (Iroh NodeId) is documented honestly in the threat
  model as an upstream dependency.
- **Audit-ready, and already audited once.** The readiness package (`docs/audit/`) holds a
  threat model, a prioritized scope with measured line counts, this RFQ, the full reports of
  the 2026-08-13 single-reviewer engagement, and a remediation document written finding by
  finding. The review baseline is stated (v3.16.0, `TORUS_PROTOCOL_VERSION = 10`) and a
  dedicated `audit-baseline` tag will be frozen at engagement start.
- **Demonstrated disclosure, not a promise.** The commitment to publish was tested: 85
  findings, 13 critical, published verbatim — including an executable PoC for a critical and
  the reviewer's judgement that an earlier fix of ours was wrong.
- **Testable claims.** 608 library + 1 integration test at baseline (513 before the audit),
  a deterministic simulation harness with a fault-coverage sweep, and an enforced clippy
  gate. Each major fix was verified **red without it**: the fix was temporarily sabotaged,
  the suite re-run, the file restored.
- **Honest maturity statement.** Alpha. One paid single-reviewer external review has taken
  place; **no audit by an established firm has**. Known accepted risks and non-goals are in
  `THREAT-MODEL.md` §6, and the items left open after remediation — election-seed grinding,
  no post-quantum VRF, O(height) happy-path validation, the deliberate absence of a FIPS-204
  `ctx` separation, ban thresholds that count identities rather than cost — are listed in
  `REMEDIATION-2026-08-13.md` and repeated per area in `SCOPE.md`. The project holds to the
  reviewer's recommendation: the network is to carry no value while those remain open.
- **Why funding a second review is worth more than funding a first one.** The starting point
  is public, so the money buys new findings rather than a rediscovery of the same 85, and
  the second reviewer's verdict can be compared against a published prior.

Link the application to this package's [`README.md`](README.md) as the entry point.

No cost or timeline figures are stated here because none have been quoted; they belong in
the firms' and OSTIF's responses.
