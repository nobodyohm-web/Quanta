# Quanta — Request for Quote & Funding Notes

This file provides a request-for-quote (RFQ) template for approaching audit firms
directly, and notes for applying to a funding body such as OSTIF. It carries no invented
figures — dates, scope, and line counts come from this package. See
[`SCOPE.md`](SCOPE.md) for the priorities referenced below and
[`THREAT-MODEL.md`](THREAT-MODEL.md) for the security claims.

## RFQ email template

> **Subject:** Security audit RFQ — Quanta (post-quantum P2P cryptocurrency, Rust, Apache-2.0)
>
> Hello,
>
> We are seeking a security review of Quanta, an open-source (Apache-2.0), sovereign P2P
> cryptocurrency written in Rust (Tauri backend; the Svelte desktop UI is out of scope).
> Its distinguishing property is end-to-end post-quantum cryptography: account authority
> and finality votes are ML-DSA-65 (FIPS 204), gossip envelopes are ML-DSA-signed, and the
> QUIC/TLS 1.3 transport negotiates hybrid X25519MLKEM768.
>
> A self-contained audit-readiness package lives in the repository under `docs/audit/`:
> a threat model, a prioritized scope with real line counts, and this RFQ. We would like
> your help scoping and estimating the engagement.
>
> - **Repository:** https://github.com/nobodyohm-web/Quanta (Apache-2.0)
> - **Review baseline:** `main` at v3.15.0 (`TORUS_PROTOCOL_VERSION = 9`); we will
>   freeze a dedicated `audit-baseline` tag at engagement start.
> - **Package:** `docs/audit/README.md`, `THREAT-MODEL.md`, `SCOPE.md`.
> - **Suggested scope:** P0 = consensus core `sm/` (8_507 LOC), ledger `p2p/ledger/`
>   (7_545 LOC), cryptography `security/` (2_081 LOC); P1 = network pipeline (8_105 LOC).
>   Line counts include inline `#[cfg(test)]` tests. Engagement shapes (crypto / consensus /
>   network) are described in `SCOPE.md`.
> - **Verification:** `cargo test` (508 library + 1 integration test, 0 failures at baseline,
>   including a multi-seed deterministic simulation harness and a 128-run determinism check);
>   `cargo clippy --all-targets -D warnings` is enforced.
>
> Could you let us know:
> 1. Whether the scope fits your practice and how you would slice it.
> 2. An estimate (effort/duration/cost) for the shape(s) you would recommend.
> 3. Your availability.
>
> We commit to publishing the full report — findings included — in the repository. A hidden
> report is worth nothing to us; we treat the audit as a public artifact.
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
- **Audit-ready.** A complete readiness package (`docs/audit/`): threat model, prioritized
  scope with measured line counts, and this RFQ. A frozen baseline commit is identified.
- **Testable claims.** 508 library + 1 integration test at baseline, a deterministic
  simulation harness, a 128-run determinism check, and an enforced clippy gate.
- **Public-disclosure commitment.** The full report will be published in the repository.
- **Honest maturity statement.** Alpha; no prior third-party audit; known accepted risks
  and non-goals are enumerated in `THREAT-MODEL.md` §6.

Link the application to this package's `README.md` as the entry point.

No cost or timeline figures are stated here because none have been quoted; they belong in
the firms' and OSTIF's responses.
