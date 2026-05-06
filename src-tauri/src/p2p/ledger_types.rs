//! Ledger type definitions — extracted from ledger.rs for clarity.
//!
//! STRUCT-2: All monetary amounts are u64 in µQTA (1 QUANTA = 1_000_000 µQTA).
//! This eliminates f64 rounding drift between nodes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// STRUCT-2: 1 QUANTA = 1_000_000 µQTA (microQTA, deterministic integer arithmetic).
pub const MICRO: u64 = 1_000_000;

/// A signed QUANTA transaction. Amounts are in µQTA (u64).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub from: String, // sender public key (hex) or "NETWORK" for mining
    pub to: String,   // receiver public key (hex)
    /// STRUCT-2: amount in µQTA (u64)
    pub amount: u64,
    pub tx_type: TxType,
    pub timestamp: String,
    pub signature: String, // Ed25519 signature of the payload
    pub hash: String,      // BLAKE3 hash of the transaction
    // ── B2: Per-account nonce (Ethereum-style anti-replay) ──
    #[serde(default)]
    pub nonce: u64, // Sender's account nonce at time of signing
    // ── Phase 2A : Post-quantum hybrid fields (optional, backward-compatible) ──
    #[serde(default)]
    pub pq_signature: Option<String>, // ML-DSA-65 signature hex (3293 B)
    #[serde(default)]
    pub pq_public_key: Option<String>, // ML-DSA-65 public key hex (1952 B)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TxType {
    Mining,   // energy-backed minting (uptime tick)
    Transfer, // user-to-user transfer
    Stake,    // lock tokens to boost PoC score
    Unstake,  // unlock staked tokens
    Burn,     // permanent destruction (burn-and-mint, 1% per transfer)
}

/// A block in the ATN chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub index: u64,
    pub timestamp: String,
    pub transactions: Vec<Transaction>,
    pub prev_hash: String,
    pub hash: String,
    pub miner: String,   // node that created this block
    pub energy_kwh: f64, // energy consumed during this block
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerStats {
    pub total_blocks: u64,
    pub total_txs: u64,
    /// STRUCT-2: total mined supply in µQTA
    pub total_mined: u64,
    pub total_energy: f64,
    pub holders: u64,
    pub pending: u64,
}

/// Serializable snapshot of the full ledger state — used for SQLite persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerSnapshot {
    pub chain: Vec<Block>,
    pub pending: Vec<Transaction>,
    pub tx_counter: u64,
    /// B2: Persisted nonce state per account
    #[serde(default)]
    pub account_nonces: HashMap<String, u64>,
}
