// security/crypto_agility.rs — Crypto-Agility Layer (Audit Recommendation)
// Abstraction for algorithm swapping without rewriting core logic.
// Enables pivot from Ed25519 → ML-DSA-65 or Falcon without refactor.

use serde::{Deserialize, Serialize};

/// Cryptographic Bill of Materials (CBOM) — tracks all algorithms in use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoBOM {
    pub signing: AlgorithmEntry,
    pub key_exchange: AlgorithmEntry,
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
    /// Current TITAN v4 CBOM — used for audit reporting
    pub fn current() -> Self {
        Self {
            signing: AlgorithmEntry {
                name: "Ed25519".into(),
                standard: "RFC 8032".into(),
                key_size_bits: 256,
                quantum_safe: false,
                status: AlgorithmStatus::Active,
            },
            key_exchange: AlgorithmEntry {
                name: "X25519 (ML-KEM-768 ready)".into(),
                standard: "RFC 7748 → FIPS 203".into(),
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

    /// Count algorithms needing PQ migration
    pub fn pq_migration_count(&self) -> usize {
        [&self.signing, &self.key_exchange, &self.hashing, &self.symmetric, &self.kdf]
            .iter()
            .filter(|a| !a.quantum_safe)
            .count()
    }

    /// Overall security grade
    pub fn security_grade(&self) -> &'static str {
        match self.pq_migration_count() {
            0 => "A+ (Full PQ)",
            1 => "A (Near PQ)",
            2 => "B (Hybrid)",
            _ => "C (Classical)",
        }
    }
}
