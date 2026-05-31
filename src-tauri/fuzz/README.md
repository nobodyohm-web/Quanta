# Quanta — Fuzzing

Coverage-guided fuzz targets for Quanta's **untrusted-input** boundaries, built on
[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) (libFuzzer).

This is an **isolated crate** (its own `[workspace]`), so it never affects the main
build or the `cargo test` suite, and CI does not need a nightly toolchain.

## Why

The gossip layer parses bytes that arrive directly from other peers. A single panic on
malformed input is a remote denial-of-service. Fuzzing asserts the invariant that the
parser/validator only ever returns `Ok(())` or `Err(_)` — never crashes.

## Setup (once)

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
```

## Run

```bash
cd src-tauri
cargo +nightly fuzz run gossip_envelope
```

Leave it running; libFuzzer mutates inputs and saves any crash to
`fuzz/artifacts/gossip_envelope/`. Reproduce a crash with:

```bash
cargo +nightly fuzz run gossip_envelope fuzz/artifacts/gossip_envelope/crash-<hash>
```

## Targets

| Target | Entry point | Surface |
|---|---|---|
| `gossip_envelope` | `quanta_lib::fuzz_parse_gossip` → `dispatcher::try_process_raw_gossip` | Network envelope decode + size/freshness/signature validation |

### Adding a target

1. Expose the stateless entry point from the library (see the `fuzz_parse_gossip`
   re-export in `src/lib.rs`).
2. Add `fuzz_targets/<name>.rs` and a `[[bin]]` entry in `fuzz/Cargo.toml`.
