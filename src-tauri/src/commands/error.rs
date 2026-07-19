//! Stable, machine-readable command errors — the i18n boundary.
//!
//! Tauri commands return `Result<T, String>`, and a raw prose string used to
//! bubble straight to the UI. Because the prose was hard-coded in French, a
//! user running the app in EN/ES/RU/ZH/JA received French. `CmdError` gives
//! every **user-facing** failure a STABLE machine code — `err.<camelCase>`, with
//! an optional `:param` suffix — that the frontend (`src/lib/errors.ts`) maps to
//! the active locale. The `Display` impl (and `From<CmdError> for String`, which
//! lets `?` convert in a `Result<_, String>` command) emit exactly that code, so
//! `return Err(CmdError::WeakPassword.into())` sends the string `err.weakPassword`.
//!
//! Purely technical / unexpected failures (poisoned locks, IO, serde, internal
//! corruption) intentionally stay raw `String`s: they are not part of this
//! contract and fall through the frontend's passthrough branch unchanged.

use std::fmt;

/// A user-facing command error carrying a stable machine code.
///
/// Every variant maps to one `err.<camelCase>` code; `RateLimited` also carries a
/// numeric parameter rendered after a `:` (`err.rateLimited:37`). `Other` is the
/// passthrough for an already-formed message we do not recognize (emitted verbatim).
#[derive(Debug, Clone)]
pub enum CmdError {
    /// Display name empty on identity creation / restore.
    DisplayNameRequired,
    /// Password shorter than the vault minimum (8 characters).
    WeakPassword,
    /// Amount not finite, negative, or zero where a positive value is required.
    InvalidAmount,
    /// Amount outside the accepted transfer / stake range (0 < x ≤ 1_000_000).
    AmountOutOfRange,
    /// Amount so large it would overflow the µQTA integer space.
    AmountTooLarge,
    /// Recipient is neither a valid `qta1…` (Bech32m) nor a 64-hex address.
    InvalidRecipient,
    /// Not enough spendable balance for the transfer / stake.
    InsufficientBalance,
    /// Not enough bonded stake to unbond the requested amount.
    InsufficientStake,
    /// The wallet's ML-DSA value identity is unavailable (locked / not created).
    IdentityMissing,
    /// Recovery phrase fails BIP39 validation.
    InvalidRecoveryPhrase,
    /// Recovery phrase is not exactly 24 words (a 32-byte seed).
    RecoveryPhraseLength,
    /// The recovery phrase could not be produced from the seed.
    RecoveryPhraseUnavailable,
    /// A password re-check failed (e.g. when enabling Touch ID).
    WrongPassword,
    /// Touch ID quick unlock is not enabled for this wallet.
    BiometricNotEnabled,
    /// Biometric unlock was refused, or the wrapped keys failed to open (opaque).
    UnlockRefused,
    /// An action needs the wallet unlocked once with the password first.
    UnlockFirst,
    /// Brute-force backoff still open; the payload is the remaining seconds.
    RateLimited(u64),
    /// Username fails the format rule (3–20, a–z0–9_, must start with a letter).
    InvalidUsername,
    /// Username already reserved by someone else.
    UsernameTaken,
    /// No account resolves for the given `@username`.
    UsernameNotFound,
    /// Connection code does not match the username.
    CodeMismatch,
    /// A public key / connection code could not be derived (malformed key).
    InvalidKey,
    /// Passthrough for an unrecognized inner error — emitted verbatim, the
    /// frontend treats it as raw text (fallback branch).
    Other(String),
}

impl CmdError {
    /// Adapt a raw error string coming out of the (untouchable) ledger core into
    /// a stable code where we recognize its prefix; otherwise pass it through
    /// unchanged. The ledger's user-facing messages are stable French sentences;
    /// mapping them **here**, at the command boundary, keeps `p2p/ledger.rs`
    /// unedited while still handing the frontend a translatable code.
    pub fn from_ledger(msg: String) -> Self {
        if msg.starts_with("Solde insuffisant") {
            CmdError::InsufficientBalance
        } else if msg.starts_with("Enjeu insuffisant") {
            CmdError::InsufficientStake
        } else if msg.starts_with("Montant invalide") {
            CmdError::InvalidAmount
        } else {
            CmdError::Other(msg)
        }
    }
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CmdError::DisplayNameRequired => f.write_str("err.displayNameRequired"),
            CmdError::WeakPassword => f.write_str("err.weakPassword"),
            CmdError::InvalidAmount => f.write_str("err.invalidAmount"),
            CmdError::AmountOutOfRange => f.write_str("err.amountOutOfRange"),
            CmdError::AmountTooLarge => f.write_str("err.amountTooLarge"),
            CmdError::InvalidRecipient => f.write_str("err.invalidRecipient"),
            CmdError::InsufficientBalance => f.write_str("err.insufficientBalance"),
            CmdError::InsufficientStake => f.write_str("err.insufficientStake"),
            CmdError::IdentityMissing => f.write_str("err.identityMissing"),
            CmdError::InvalidRecoveryPhrase => f.write_str("err.invalidRecoveryPhrase"),
            CmdError::RecoveryPhraseLength => f.write_str("err.recoveryPhraseLength"),
            CmdError::RecoveryPhraseUnavailable => f.write_str("err.recoveryPhraseUnavailable"),
            CmdError::WrongPassword => f.write_str("err.wrongPassword"),
            CmdError::BiometricNotEnabled => f.write_str("err.biometricNotEnabled"),
            CmdError::UnlockRefused => f.write_str("err.unlockRefused"),
            CmdError::UnlockFirst => f.write_str("err.unlockFirst"),
            CmdError::RateLimited(secs) => write!(f, "err.rateLimited:{secs}"),
            CmdError::InvalidUsername => f.write_str("err.invalidUsername"),
            CmdError::UsernameTaken => f.write_str("err.usernameTaken"),
            CmdError::UsernameNotFound => f.write_str("err.usernameNotFound"),
            CmdError::CodeMismatch => f.write_str("err.codeMismatch"),
            CmdError::InvalidKey => f.write_str("err.invalidKey"),
            CmdError::Other(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for CmdError {}

/// The whole point: a `CmdError` collapses to its stable code string, so Tauri
/// commands keep their `Result<T, String>` signature and `?` converts for free.
impl From<CmdError> for String {
    fn from(e: CmdError) -> String {
        e.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::CmdError;

    #[test]
    fn codes_are_stable_and_camel_cased() {
        assert_eq!(CmdError::WeakPassword.to_string(), "err.weakPassword");
        assert_eq!(CmdError::InsufficientBalance.to_string(), "err.insufficientBalance");
        assert_eq!(CmdError::DisplayNameRequired.to_string(), "err.displayNameRequired");
        assert_eq!(CmdError::InvalidRecipient.to_string(), "err.invalidRecipient");
        assert_eq!(CmdError::UsernameTaken.to_string(), "err.usernameTaken");
    }

    #[test]
    fn rate_limited_carries_its_seconds_param() {
        assert_eq!(CmdError::RateLimited(37).to_string(), "err.rateLimited:37");
        assert_eq!(CmdError::RateLimited(1).to_string(), "err.rateLimited:1");
    }

    #[test]
    fn into_string_yields_the_code() {
        let s: String = CmdError::IdentityMissing.into();
        assert_eq!(s, "err.identityMissing");
    }

    #[test]
    fn from_ledger_maps_known_prefixes_else_passes_through() {
        assert!(matches!(
            CmdError::from_ledger("Solde insuffisant: 3.500000 QUANTA".into()),
            CmdError::InsufficientBalance
        ));
        assert!(matches!(
            CmdError::from_ledger("Solde insuffisant pour staker: 1.0 QUANTA".into()),
            CmdError::InsufficientBalance
        ));
        assert!(matches!(
            CmdError::from_ledger("Enjeu insuffisant à délier: 2.0 QUANTA bondé".into()),
            CmdError::InsufficientStake
        ));
        assert!(matches!(
            CmdError::from_ledger("Montant invalide".into()),
            CmdError::InvalidAmount
        ));
        // Unrecognized → verbatim passthrough (frontend fallback branch).
        match CmdError::from_ledger("some internal io error".into()) {
            CmdError::Other(s) => assert_eq!(s, "some internal io error"),
            _ => panic!("expected Other passthrough"),
        }
    }
}
