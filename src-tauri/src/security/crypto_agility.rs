// security/crypto_agility.rs — Crypto-Agility Layer (Audit Recommendation)
// Abstraction for algorithm swapping without rewriting core logic.
// Enables pivot from Ed25519 → ML-DSA-65 or Falcon without refactor.

use serde::{Deserialize, Serialize};

/// Cryptographic Bill of Materials (CBOM) — tracks all algorithms in use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoBOM {
    pub signing: AlgorithmEntry,
    pub key_exchange: AlgorithmEntry,
    /// M3 (AUDIT-2026-07-25) — the node's *transport* identity, kept as its own
    /// entry because it is the only classical primitive left and hiding it inside
    /// `signing` is exactly the misstatement this audit found. Ed25519 here is the
    /// Iroh NodeId, an upstream dependency: the account authority and the gossip
    /// envelopes are both ML-DSA-65 since PQ-MIG-3B / PQ-ENVELOPE-1.
    pub transport_auth: AlgorithmEntry,
    pub hashing: AlgorithmEntry,
    pub symmetric: AlgorithmEntry,
    pub kdf: AlgorithmEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgorithmEntry {
    pub name: String,
    pub standard: String,
    pub key_size_bits: u32,
    pub quantum_safe: bool,
    pub status: AlgorithmStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlgorithmStatus {
    Active,
    Deprecated,
    PendingMigration,
}

impl CryptoBOM {
    /// The CBOM as it actually is — rendered to the user in the Help screen, so
    /// it is held to the project's zero-fake rule like any other displayed value.
    ///
    /// M3 (AUDIT-2026-07-25): this used to report `signing: Ed25519 / RFC 8032 /
    /// quantum_safe: false` and `key_exchange: X25519 / PendingMigration`. Both
    /// were false — PQ-MIG-3B made ML-DSA-65 the sole tx authority (the Ed25519
    /// co-factor was removed from `verify_tx`), PQ-ENVELOPE-1 made every gossip
    /// envelope ML-DSA-signed with the classical fallback deleted, and
    /// PQ-TRANSPORT-1 made the QUIC/TLS handshake negotiate the X25519MLKEM768
    /// hybrid. The app was telling its own users that its signatures were the
    /// exact primitive the whole migration removed.
    pub fn current() -> Self {
        Self {
            signing: AlgorithmEntry {
                name: "ML-DSA-65".into(),
                standard: "FIPS 204".into(),
                // Public key size: 1952 bytes.
                key_size_bits: 15_616,
                quantum_safe: true,
                status: AlgorithmStatus::Active,
            },
            key_exchange: AlgorithmEntry {
                name: "X25519MLKEM768".into(),
                standard: "FIPS 203 hybrid (ML-KEM-768 ⊕ X25519)".into(),
                // Combined encapsulation key: 1184 + 32 bytes.
                key_size_bits: 9_728,
                quantum_safe: true,
                status: AlgorithmStatus::Active,
            },
            transport_auth: AlgorithmEntry {
                name: "Ed25519 (Iroh NodeId)".into(),
                standard: "RFC 8032".into(),
                key_size_bits: 256,
                quantum_safe: false,
                status: AlgorithmStatus::PendingMigration,
            },
            hashing: AlgorithmEntry {
                name: "BLAKE3".into(),
                standard: "BLAKE3 Spec 1.0".into(),
                key_size_bits: 256,
                quantum_safe: true,
                status: AlgorithmStatus::Active,
            },
            symmetric: AlgorithmEntry {
                name: "AES-256-GCM".into(),
                standard: "NIST SP 800-38D".into(),
                key_size_bits: 256,
                quantum_safe: true,
                status: AlgorithmStatus::Active,
            },
            kdf: AlgorithmEntry {
                name: "Argon2id".into(),
                standard: "RFC 9106".into(),
                key_size_bits: 256,
                quantum_safe: true,
                status: AlgorithmStatus::Active,
            },
        }
    }

    /// Count algorithms needing PQ migration. After PQ-TRANSPORT-1 this is 1 —
    /// the Iroh NodeId — and that one is upstream-blocked, not deferred by us.
    pub fn pq_migration_count(&self) -> usize {
        [
            &self.signing,
            &self.key_exchange,
            &self.transport_auth,
            &self.hashing,
            &self.symmetric,
            &self.kdf,
        ]
        .iter()
        .filter(|a| !a.quantum_safe)
        .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M3 (AUDIT-2026-07-25) — the in-app security disclosure is rendered to the
    /// user, so it falls under the zero-fake rule: every entry must match what the
    /// code actually runs.
    #[test]
    fn m3_cbom_reports_the_real_primitives() {
        let bom = CryptoBOM::current();
        assert_eq!(bom.signing.name, "ML-DSA-65", "PQ-MIG-3B: ML-DSA is the tx authority");
        assert!(bom.signing.quantum_safe);
        assert_eq!(bom.key_exchange.name, "X25519MLKEM768", "PQ-TRANSPORT-1");
        assert!(bom.key_exchange.quantum_safe);
        // The one honest remaining debt, kept visible on purpose.
        assert!(!bom.transport_auth.quantum_safe);
        assert_eq!(bom.pq_migration_count(), 1, "only the Iroh NodeId is still classical");
    }
}
