# Contributing to Quanta

Quanta is alpha research software for a currency with no issuer. That combination sets an
unusual bar: a bug here does not degrade a feature, it prints money or loses it. What follows
is the working agreement, not bureaucracy.

New to the codebase? Read [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) first — it ends with a
table of every invariant and where it is enforced, which is the fastest way to know whether a
change you are considering is safe.

## The gates

Every pull request must pass all three CI jobs. They are the same commands you can run locally:

```bash
cargo test   --manifest-path src-tauri/Cargo.toml                       # 608 tests + 1 integration
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo deny   --manifest-path src-tauri/Cargo.toml check                 # advisories · licenses · sources · bans
npm run check && npm run build                                          # svelte-check 0/0
npm audit --audit-level=high && npm run check:csp                       # supply chain · CSP du bundle
```

`clippy -D warnings` is not style policing: with no `#![allow(dead_code)]` anywhere in the tree,
the compiler is the authority on what code is still reachable. Silencing it hides the evidence.

## Rules that are not negotiable

These come from bugs that actually happened. Each one is a scar.

1. **No locally computed value may touch the money path.** Not a Shapley share, not self-reported
   watts, not a wall clock. If two honest nodes can disagree about a number, that number cannot
   decide anything about consensus. Money amounts are pure functions of the chain, recomputed by
   every receiver.
2. **No exemptions in validation.** Every "this case is obviously safe, skip the check" in this
   codebase's history became a vulnerability — including one that let anyone drain a stranger's
   balance by addressing the transfer to `BURN`.
3. **Build and verify from one function.** When a producer computes something a verifier must
   agree with, both call the same function (`expected_block_rewards`, `expected_slash_consumption`,
   `uncovered_tx_indices`). Two implementations of one rule will drift, and the drift is a fork.
4. **`tokio::sync` only** — never `std::sync` across an `.await`. Respect the lock ordering
   documented in `src-tauri/src/CLAUDE.md`.
5. **No `unwrap()`** on any path that can fail. `Result<T, E>` and `?`. The tree holds
   exactly one `unsafe` block in production code (`guardian.rs`, AppKit interop); adding a second
   requires a written justification in the PR.
6. **All amounts are `u64` µQTA.** Never `f64` for a balance.
7. **`zeroize()` every secret** after use.
8. **A consensus rule change is a hard fork.** Bump `TORUS_PROTOCOL_VERSION`, and say in the
   commit body what breaks and why it was worth breaking.

## Testing

An invariant needs a **teeth test**: a test that breaks the property on purpose and asserts the
checker catches it. An invariant checker that has never failed is not known to work — several in
`sm/sim.rs` are named `*_has_teeth` for exactly this reason.

The same rule applies to any security fix, and it is stronger than it sounds: **a fix is not
done until its test has been seen to fail.** Sabotage the fix, watch the test go red, restore the
file byte for byte, watch it go green. During the August 2026 remediation this caught two tests
that passed for the wrong reason — including a timing test that was green with *and* without the
fix it claimed to prove, because a single ML-DSA verification dominated everything it measured.

Consensus changes should also be exercised through the deterministic simulation harness
(`sm/sim.rs`), which can partition the network, reorder messages, crash nodes and run Byzantine
participants, and replays byte-identically from a seed.

**In-process tests are not evidence that P2P works.** They cannot see anything that only exists
between operating-system processes on a real network. Before claiming a networking change works,
run two real daemons — see [`docs/ops/QUICKSTART.md` §6](docs/ops/QUICKSTART.md) — and verify by
RPC, not by log lines. A "connected to peer" log proves a QUIC dial; only a dispatched `Hello`
proves the protocol.

## Commits and pull requests

- Commit messages in **English**, Conventional Commits (`feat(consensus)!:`, `fix(p2p):`,
  `docs:`). The `!` marks a protocol break.
- Explain **why**, not what. The diff already says what.
- Never commit onto `main`; branch, then open a PR.
- Never commit secrets, keys, vaults or `node_key` files.

## Security

Do not open a public issue for a vulnerability. See [`SECURITY.md`](SECURITY.md) for the
disclosure process.

## License

Contributions are accepted under the [Apache-2.0](LICENSE) license of the project.
