#![no_main]

use libfuzzer_sys::fuzz_target;

// The gossip-envelope parser/validator is the front door for *untrusted* bytes
// arriving from the network. Whatever a malicious peer sends — truncated JSON,
// giant payloads, malformed signatures, bad UTF-8 — this code path must only
// ever return Ok(()) or Err(_). It must never panic, overflow, or hang.
//
// Run with:
//   rustup toolchain install nightly      # once
//   cargo install cargo-fuzz              # once
//   cd src-tauri && cargo +nightly fuzz run gossip_envelope
fuzz_target!(|data: &[u8]| {
    let _ = quanta_lib::fuzz_parse_gossip(data);
});
