//! Touch ID–gated quick unlock (macOS).
//!
//! Security model — the honest kind, not a cosmetic prompt:
//! - A random 32-byte KEK lives in the **macOS Keychain** behind
//!   `SecAccessControl(.BIOMETRY_CURRENT_SET)`: the *operating system* demands
//!   a fingerprint at **read** time, and invalidates the item if the enrolled
//!   fingerprints ever change (a new finger can't open old secrets).
//! - That KEK wraps (AES-256-GCM) the two Argon2id-**derived** vault keys
//!   (Ed25519 blob + ML-DSA seed blob). The password itself is NEVER stored,
//!   anywhere, in any form — password unlock remains the universal fallback.
//! - Disabling deletes the Keychain item; the on-disk wrap becomes garbage.
//!
//! All calls are blocking (the Touch ID sheet blocks the calling thread) —
//! callers must wrap them in `tokio::task::spawn_blocking`.

/// Keychain coordinates of the quick-unlock KEK.
#[cfg(target_os = "macos")]
const SERVICE: &str = "app.quanta.wallet";
#[cfg(target_os = "macos")]
const ACCOUNT: &str = "quick-unlock-kek-v1";
/// Throwaway item used by [`probe_available`] (added then deleted, no prompt).
#[cfg(target_os = "macos")]
const PROBE_SERVICE: &str = "app.quanta.wallet.probe";

/// Whether this machine can do biometry-gated Keychain storage at all.
///
/// Probe strategy (dependency-free): creating a Keychain item under
/// `.BIOMETRY_CURRENT_SET` **fails** when no biometry is enrolled (the ACL is
/// bound to the current enrollment at creation time), so a successful
/// add-then-delete of a throwaway item proves Touch ID is present and
/// enrolled. Neither operation shows any prompt (only *reads* do).
#[cfg(target_os = "macos")]
pub fn probe_available() -> bool {
    use security_framework::passwords::{delete_generic_password, set_generic_password_options};
    use security_framework::passwords_options::{AccessControlOptions, PasswordOptions};
    let mut opts = PasswordOptions::new_generic_password(PROBE_SERVICE, ACCOUNT);
    opts.set_access_control_options(AccessControlOptions::BIOMETRY_CURRENT_SET);
    let ok = set_generic_password_options(b"probe", opts).is_ok();
    let _ = delete_generic_password(PROBE_SERVICE, ACCOUNT);
    ok
}

/// Store the quick-unlock KEK, replacing any previous one. No prompt.
#[cfg(target_os = "macos")]
pub fn store_kek(kek: &[u8; 32]) -> Result<(), String> {
    use security_framework::passwords::{delete_generic_password, set_generic_password_options};
    use security_framework::passwords_options::{AccessControlOptions, PasswordOptions};
    // Replace-not-update: delete first (update of ACL-protected items would
    // itself require authentication).
    let _ = delete_generic_password(SERVICE, ACCOUNT);
    let mut opts = PasswordOptions::new_generic_password(SERVICE, ACCOUNT);
    opts.set_access_control_options(AccessControlOptions::BIOMETRY_CURRENT_SET);
    set_generic_password_options(kek, opts)
        .map_err(|_| "Touch ID indisponible (biométrie non enrôlée ?)".to_string())
}

/// Read the KEK — this is the moment macOS presents the Touch ID sheet.
/// Fails opaquely on cancel/mismatch/fingerprint-set-changed.
#[cfg(target_os = "macos")]
pub fn read_kek() -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
    use security_framework::passwords::get_generic_password;
    get_generic_password(SERVICE, ACCOUNT)
        .map(zeroize::Zeroizing::new)
        .map_err(|_| "Touch ID refusé".to_string())
}

/// Remove the quick-unlock KEK (turning the on-disk wrap into garbage).
#[cfg(target_os = "macos")]
pub fn delete_kek() {
    use security_framework::passwords::delete_generic_password;
    let _ = delete_generic_password(SERVICE, ACCOUNT);
}

// ── Non-macOS stubs — the feature simply reports itself unsupported. ────────

#[cfg(not(target_os = "macos"))]
pub fn probe_available() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn store_kek(_kek: &[u8; 32]) -> Result<(), String> {
    Err("Biométrie non prise en charge sur cette plateforme".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn read_kek() -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
    Err("Biométrie non prise en charge sur cette plateforme".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn delete_kek() {}
