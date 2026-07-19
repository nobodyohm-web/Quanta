//! Tauri command surface, split by domain.
//!
//! Each submodule holds the `#[tauri::command]` entry points for one concern;
//! `lib.rs` keeps only `AppState`, `run()`/`setup`, the `invoke_handler!`
//! registration (paths updated to these modules) and genuinely cross-cutting
//! helpers. Command names and signatures are unchanged — zero wire/frontend
//! impact. The `@pseudo` identity commands still live in `crate::commands_v3`.
//!
//! - `error`       — `CmdError`: stable `err.<code>` machine strings for i18n
//! - `identity`    — vault lifecycle, biometrics, address forms
//! - `wallet`      — balance, transfer, stake/unstake, wallet overview
//! - `network`     — peers, connect, node status, security posture
//! - `chain`       — chain/finality/economy/history reads
//! - `diagnostics` — UI freeze probes (`ui_diag`/`ui_beat`/`was_guardian_reload`)

pub mod error;
pub mod identity;
pub mod wallet;
pub mod network;
pub mod chain;
pub mod diagnostics;
