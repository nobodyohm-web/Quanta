// p2p/ledger.rs — ATN Native Protocol Distributed Ledger
// Self-sovereign token: no external blockchain dependency
// Value backed by real energy consumption + network utility

use crate::security::CryptoEngine;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use chrono::Utc;

/// A signed ATN transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub from: String,        // sender public key (hex) or "NETWORK" for mining
    pub to: String,          // receiver public key (hex)
    pub amount: f64,
    pub tx_type: TxType,
    pub timestamp: String,
    pub signature: String,   // Ed25519 signature of the payload
    pub hash: String,        // BLAKE3 hash of the transaction
    // ── Phase 2A : Post-quantum hybrid fields (optional, backward-compatible) ──
    #[serde(default)]
    pub pq_signature: Option<String>,   // ML-DSA-65 signature hex (3293 B)
    #[serde(default)]
    pub pq_public_key: Option<String>,  // ML-DSA-65 public key hex (1952 B)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TxType {
    Mining,     // energy-backed minting (uptime tick)
    Transfer,   // user-to-user transfer
    Stake,      // lock tokens to boost PoC score
    Unstake,    // unlock staked tokens
    Burn,       // permanent destruction (burn-and-mint, 1% per transfer)
}

/// A block in the ATN chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub index: u64,
    pub timestamp: String,
    pub transactions: Vec<Transaction>,
    pub prev_hash: String,
    pub hash: String,
    pub miner: String,       // node that created this block
    pub energy_kwh: f64,     // energy consumed during this block
}

/// The ATN distributed ledger
pub struct Ledger {
    pub chain: Vec<Block>,
    pending: Vec<Transaction>,
    tx_counter: u64,
    /// V3: Anti-replay — set of all known transaction hashes
    seen_tx_hashes: HashSet<String>,
}

impl Ledger {
    pub fn new() -> Self {
        // Genesis block
        let genesis = Block {
            index: 0,
            timestamp: Utc::now().to_rfc3339(),
            transactions: vec![],
            prev_hash: "0".repeat(64),
            hash: hex::encode(blake3::hash(b"SOVA_ATN_GENESIS_2026").as_bytes()),
            miner: "GENESIS".into(),
            energy_kwh: 0.0,
        };
        Self { chain: vec![genesis], pending: vec![], tx_counter: 0, seen_tx_hashes: HashSet::new() }
    }

    /// Create a mining transaction (energy-backed minting). Network-issued, so no signature.
    pub fn mine_tx(&mut self, miner_pk: &str, amount: f64, kwh: f64) -> Transaction {
        let tx = self.build_unsigned_tx("NETWORK", miner_pk, amount, TxType::Mining);
        self.pending.push(tx.clone());
        if self.pending.len() >= 10 {
            self.seal_block(miner_pk, kwh);
        }
        tx
    }

    /// Create a transfer transaction signed by the active identity.
    /// Validates balance, rejects self-transfers, enforces timestamp window, prevents replay.
    pub fn transfer_tx(
        &mut self, from: &str, to: &str, amount: f64, crypto: &CryptoEngine,
    ) -> Result<Transaction, String> {
        if amount <= 0.0 { return Err("Montant invalide".into()); }
        if from == to { return Err("Transfert vers soi-même".into()); }
        // V4: Double-spend safe — balance_of includes pending outflows
        let balance = self.balance_of(from);
        if balance < amount { return Err(format!("Solde insuffisant: {:.4} ATN", balance)); }
        let tx = self.build_signed_tx(from, to, amount, TxType::Transfer, crypto)?;
        // V3: Anti-replay — reject if we've seen this tx hash before
        if !self.seen_tx_hashes.insert(tx.hash.clone()) {
            return Err("Transaction déjà traitée (replay détecté)".into());
        }
        // V8: Timestamp window — reject txs with timestamps too far from now (±5 min)
        if let Ok(tx_time) = chrono::DateTime::parse_from_rfc3339(&tx.timestamp) {
            let drift = (Utc::now().timestamp() - tx_time.timestamp()).abs();
            if drift > 300 {
                return Err("Timestamp hors fenêtre de validité (±5 min)".into());
            }
        }
        self.pending.push(tx.clone());
        Ok(tx)
    }

    /// V2 — Burn-and-Mint : 1% brûlé automatiquement à chaque transfert.
    /// Retourne `(tx_principale, montant_brûlé)`.
    pub fn transfer_with_burn(
        &mut self, from: &str, to: &str, amount: f64, crypto: &CryptoEngine,
    ) -> Result<(Transaction, f64), String> {
        if amount < 0.01 { return Err("Minimum transfer: 0.01 SOVA".into()); }
        const BURN_RATE: f64 = 0.01;
        let burn_amount = (amount * BURN_RATE * 1e8).round() / 1e8; // arrondi 8 décimales
        let net_amount  = amount - burn_amount;
        let tx = self.transfer_tx(from, to, net_amount, crypto)?;
        if burn_amount > 0.0 {
            let burn_tx = self.build_unsigned_tx(from, "BURN", burn_amount, TxType::Burn);
            self.pending.push(burn_tx);
        }
        Ok((tx, burn_amount))
    }

    /// Burn ATN — permanent destruction, signed by the owner.
    /// Destination is the special "BURN" sink: balance goes to nowhere.
    pub fn burn_tx(
        &mut self, from: &str, amount: f64, crypto: &CryptoEngine,
    ) -> Result<Transaction, String> {
        if amount <= 0.0 { return Err("Montant invalide".into()); }
        let balance = self.balance_of(from);
        if balance < amount { return Err(format!("Solde insuffisant: {:.4} ATN", balance)); }
        let tx = self.build_signed_tx(from, "BURN", amount, TxType::Burn, crypto)?;
        if !self.seen_tx_hashes.insert(tx.hash.clone()) {
            return Err("Transaction déjà traitée (replay détecté)".into());
        }
        self.pending.push(tx.clone());
        Ok(tx)
    }

    /// Total ATN that has been permanently burned (deflationary supply).
    pub fn total_burned(&self) -> f64 {
        let chain_burn: f64 = self.chain.iter()
            .flat_map(|b| b.transactions.iter())
            .filter(|t| t.tx_type == TxType::Burn)
            .map(|t| t.amount).sum();
        let pending_burn: f64 = self.pending.iter()
            .filter(|t| t.tx_type == TxType::Burn)
            .map(|t| t.amount).sum();
        chain_burn + pending_burn
    }

    // ── Phase 3.1: Signature Verification (toujours hybride) ─────

    /// Verify the signature(s) on a user-signed transaction.
    /// - Network-issued transactions (from == "NETWORK") are exempt.
    /// - Toutes les autres tx passent par `HybridIdentity::verify_hybrid` :
    ///   - Si `pq_signature` + `pq_public_key` non vides → Ed25519 OR ML-DSA-65
    ///   - Sinon → Ed25519 seul (compatibilité ascendante)
    pub fn verify_tx(tx: &Transaction) -> Result<bool, String> {
        if tx.from == "NETWORK" { return Ok(true); }
        if tx.signature.is_empty() { return Err("Transaction non signée".into()); }

        let payload = format!(
            "{}:{}:{}:{}:{}:{:?}",
            tx.id, tx.from, tx.to, tx.amount, tx.timestamp, tx.tx_type
        );

        let classical = hex::decode(&tx.signature).map_err(|_| "Signature invalide")?;
        let quantum = match tx.pq_signature.as_deref() {
            Some(hex_str) if !hex_str.is_empty() =>
                hex::decode(hex_str).map_err(|_| "PQ signature invalide")?,
            _ => Vec::new(),
        };
        let pq_pk_hex = tx.pq_public_key.as_deref().unwrap_or("");

        let hybrid_sig = crate::security::hybrid_crypto::HybridSignature { classical, quantum };
        Ok(crate::security::hybrid_crypto::HybridIdentity::verify_hybrid(
            &tx.from, pq_pk_hex, payload.as_bytes(), &hybrid_sig,
        ))
    }

    /// Verify the integrity of the entire chain:
    /// - Each block's prev_hash links to the previous block's hash
    /// - All user-signed transactions have valid Ed25519 signatures
    pub fn verify_chain(&self) -> Result<(u64, u64), String> {
        let mut verified_blocks = 0u64;
        let mut verified_txs = 0u64;
        for (i, block) in self.chain.iter().enumerate() {
            // Genesis block (index 0) has no previous block to link
            if i > 0 {
                let prev = &self.chain[i - 1];
                if block.prev_hash != prev.hash {
                    return Err(format!(
                        "Chaîne corrompue au bloc {}: prev_hash mismatch", block.index
                    ));
                }
            }
            // Verify all signed transactions in this block
            for tx in &block.transactions {
                if !Self::verify_tx(tx)? {
                    return Err(format!(
                        "Signature invalide: tx {} dans bloc {}", tx.id, block.index
                    ));
                }
                verified_txs += 1;
            }
            verified_blocks += 1;
        }
        Ok((verified_blocks, verified_txs))
    }

    /// Seal pending transactions into a new block
    pub fn seal_block(&mut self, miner: &str, energy_kwh: f64) -> Block {
        let prev = self.chain.last().expect("genesis block must exist");
        let txs = std::mem::take(&mut self.pending);
        let index = prev.index + 1;
        let ts = Utc::now().to_rfc3339();
        let payload = format!("{}:{}:{}:{}", index, prev.hash, ts, txs.len());
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        let block = Block {
            index, timestamp: ts, transactions: txs,
            prev_hash: prev.hash.clone(), hash,
            miner: miner.into(), energy_kwh,
        };
        self.chain.push(block.clone());
        block
    }

    /// Seal only if there are pending transactions. Returns the block if one was sealed.
    pub fn seal_if_pending(&mut self, miner: &str, energy_kwh: f64) -> Option<Block> {
        if self.pending.is_empty() { return None; }
        Some(self.seal_block(miner, energy_kwh))
    }

    /// Count of pending (unsealed) transactions.
    pub fn pending_count(&self) -> usize { self.pending.len() }

    /// Compute balance from the full chain
    pub fn balance_of(&self, pk: &str) -> f64 {
        let mut bal = 0.0_f64;
        for block in &self.chain {
            for tx in &block.transactions {
                if tx.to == pk { bal += tx.amount; }
                if tx.from == pk && tx.from != "NETWORK" { bal -= tx.amount; }
            }
        }
        // Also check pending
        for tx in &self.pending {
            if tx.to == pk { bal += tx.amount; }
            if tx.from == pk && tx.from != "NETWORK" { bal -= tx.amount; }
        }
        bal
    }

    /// Get all balances. Excludes synthetic addresses (NETWORK = source, BURN = sink).
    pub fn all_balances(&self) -> HashMap<String, f64> {
        let mut bals: HashMap<String, f64> = HashMap::new();
        let credit = |bals: &mut HashMap<String, f64>, tx: &Transaction| {
            if tx.to != "NETWORK" && tx.to != "BURN" { *bals.entry(tx.to.clone()).or_default() += tx.amount; }
            if tx.from != "NETWORK" && tx.from != "BURN" { *bals.entry(tx.from.clone()).or_default() -= tx.amount; }
        };
        for block in &self.chain { for tx in &block.transactions { credit(&mut bals, tx); } }
        for tx in &self.pending { credit(&mut bals, tx); }
        bals
    }

    /// Get recent transactions
    pub fn recent_txs(&self, limit: usize) -> Vec<Transaction> {
        let mut txs: Vec<Transaction> = self.chain.iter()
            .flat_map(|b| b.transactions.iter().cloned())
            .collect();
        txs.extend(self.pending.iter().cloned());
        txs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        txs.truncate(limit);
        txs
    }

    /// Get chain stats
    pub fn stats(&self) -> LedgerStats {
        let total_blocks = self.chain.len() as u64;
        let total_txs: usize = self.chain.iter().map(|b| b.transactions.len()).sum();
        let total_mined: f64 = self.chain.iter()
            .flat_map(|b| b.transactions.iter())
            .filter(|t| t.tx_type == TxType::Mining)
            .map(|t| t.amount).sum();
        let total_energy: f64 = self.chain.iter().map(|b| b.energy_kwh).sum();
        let holders = self.all_balances().len();
        LedgerStats { total_blocks, total_txs: total_txs as u64, total_mined, total_energy, holders: holders as u64, pending: self.pending.len() as u64 }
    }

    fn next_tx(&mut self, from: &str, to: &str, amount: f64, tx_type: TxType)
        -> (String, String, String, String)
    {
        self.tx_counter += 1;
        let id = format!("tx_{}", self.tx_counter);
        let ts = Utc::now().to_rfc3339();
        let payload = format!("{}:{}:{}:{}:{}:{:?}", id, from, to, amount, ts, tx_type);
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        (id, ts, payload, hash)
    }

    /// Build an unsigned (network-issued) tx. Used for Mining and rewards.
    fn build_unsigned_tx(&mut self, from: &str, to: &str, amount: f64, tx_type: TxType) -> Transaction {
        let (id, ts, _payload, hash) = self.next_tx(from, to, amount, tx_type.clone());
        Transaction {
            id, from: from.into(), to: to.into(), amount, tx_type,
            timestamp: ts, signature: String::new(), hash,
            pq_signature: None, pq_public_key: None,
        }
    }

    /// Build a tx signed by the active identity (signature hybride Ed25519 + PQ stub).
    /// La couche PQ est vide jusqu'à activation de `ml-dsa >= 0.2` ; le verify
    /// hybride retombe alors automatiquement sur Ed25519 seul.
    fn build_signed_tx(
        &mut self, from: &str, to: &str, amount: f64, tx_type: TxType, crypto: &CryptoEngine,
    ) -> Result<Transaction, String> {
        let (id, ts, payload, hash) = self.next_tx(from, to, amount, tx_type.clone());
        let (classical, quantum, pq_pk) = crypto.sign_hybrid(payload.as_bytes())?;
        Ok(Transaction {
            id, from: from.into(), to: to.into(), amount, tx_type,
            timestamp: ts,
            signature: hex::encode(&classical),
            hash,
            pq_signature: Some(hex::encode(&quantum)),
            pq_public_key: Some(pq_pk),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerStats {
    pub total_blocks: u64,
    pub total_txs: u64,
    pub total_mined: f64,
    pub total_energy: f64,
    pub holders: u64,
    pub pending: u64,
}

impl Default for Ledger { fn default() -> Self { Self::new() } }

/// Serializable snapshot of the full ledger state — used for SQLite persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerSnapshot {
    pub chain: Vec<Block>,
    pub pending: Vec<Transaction>,
    pub tx_counter: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_tx_uses_hybrid_path() {
        // Une tx signée doit avoir les champs PQ "présents-mais-vides", ce qui
        // exerce HybridIdentity::verify_hybrid avec fallback Ed25519.
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        // Donne un solde au signataire via une mining tx réseau.
        ledger.mine_tx(&id.public_key_hex, 5.0, 0.0);

        // Adresse destinataire valide (64 chars hex)
        let to = "b".repeat(64);
        let tx = ledger.transfer_tx(&id.public_key_hex, &to, 1.0, &crypto)
            .expect("transfer should succeed");

        assert!(tx.pq_signature.is_some(), "tx signée doit porter le champ pq_signature");
        assert!(tx.pq_public_key.is_some(), "tx signée doit porter le champ pq_public_key");
        assert!(Ledger::verify_tx(&tx).unwrap(), "verify_tx hybride doit valider Ed25519");
    }

    #[test]
    fn network_tx_skips_signature() {
        // Une mining tx réseau (from=NETWORK) n'a pas à porter de signature.
        let to = "c".repeat(64);
        let mut ledger = Ledger::new();
        let tx = ledger.mine_tx(&to, 0.5, 0.0);
        assert_eq!(tx.from, "NETWORK");
        assert!(Ledger::verify_tx(&tx).unwrap(), "tx NETWORK toujours valide");
    }
}

impl Ledger {
    /// Create a serializable snapshot of the current state.
    pub fn snapshot(&self) -> LedgerSnapshot {
        LedgerSnapshot {
            chain: self.chain.clone(),
            pending: self.pending.clone(),
            tx_counter: self.tx_counter,
        }
    }

    /// Restore ledger state from a previously persisted snapshot.
    pub fn restore(snap: LedgerSnapshot) -> Self {
        // V3: Rebuild the anti-replay set from the existing chain
        let mut seen = HashSet::new();
        for block in &snap.chain {
            for tx in &block.transactions {
                seen.insert(tx.hash.clone());
            }
        }
        for tx in &snap.pending {
            seen.insert(tx.hash.clone());
        }
        Self {
            chain: snap.chain,
            pending: snap.pending,
            tx_counter: snap.tx_counter,
            seen_tx_hashes: seen,
        }
    }
}
