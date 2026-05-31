// storage/db.rs — libSQL Database Layer (V2 Pure Crypto)
// SQLite fork with async support. Sites and search removed in V2.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredKeypair {
    pub id: String,
    pub public_key: Vec<u8>,
    pub encrypted_secret_key: Vec<u8>,
    pub nonce: Vec<u8>,
    pub display_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub tx_type: String,
    pub amount_sats: i64,
    pub description: Option<String>,
    pub timestamp: String,
    pub payment_hash: Option<String>,
}

/// libSQL database wrapper
pub struct Database {
    conn: libsql::Connection,
}

impl Database {
    pub async fn new(db_path: &Path) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let db = libsql::Builder::new_local(db_path)
            .build().await.map_err(|e| e.to_string())?;
        let conn = db.connect().map_err(|e| e.to_string())?;
        let instance = Self { conn };
        instance.migrate().await?;
        Ok(instance)
    }

    async fn migrate(&self) -> Result<(), String> {
        let stmts = [
            "CREATE TABLE IF NOT EXISTS keypairs (id TEXT PRIMARY KEY, public_key BLOB NOT NULL, encrypted_secret_key BLOB NOT NULL, nonce BLOB NOT NULL, display_name TEXT NOT NULL DEFAULT 'Sovereign', created_at TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS transactions (id TEXT PRIMARY KEY, tx_type TEXT NOT NULL, amount_sats INTEGER NOT NULL, description TEXT, timestamp TEXT NOT NULL, payment_hash TEXT)",
            // Phase 1.1: Persistent state snapshots for Ledger, Reputation engines
            "CREATE TABLE IF NOT EXISTS state_snapshots (key TEXT PRIMARY KEY, data TEXT NOT NULL, updated_at TEXT NOT NULL)",
        ];
        for sql in &stmts {
            self.conn.execute(sql, ()).await.map_err(|e| format!("Migration: {}", e))?;
        }
        // Best-effort backfill for older DBs missing display_name (ignored if present)
        let _ = self.conn
            .execute("ALTER TABLE keypairs ADD COLUMN display_name TEXT NOT NULL DEFAULT 'Sovereign'", ())
            .await;
        Ok(())
    }

    // ─── Keypairs ───
    pub async fn store_keypair(
        &self, pk: &[u8], enc_sk: &[u8], nonce: &[u8], display_name: &str,
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO keypairs (id, public_key, encrypted_secret_key, nonce, display_name, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            libsql::params![id.clone(), pk.to_vec(), enc_sk.to_vec(), nonce.to_vec(), display_name, now],
        ).await.map_err(|e| e.to_string())?;
        Ok(id)
    }

    pub async fn get_active_keypair(&self) -> Result<Option<StoredKeypair>, String> {
        let mut rows = self.conn.query(
            "SELECT id, public_key, encrypted_secret_key, nonce, display_name, created_at \
             FROM keypairs ORDER BY created_at DESC LIMIT 1",
            (),
        ).await.map_err(|e| e.to_string())?;
        match rows.next().await.map_err(|e| e.to_string())? {
            Some(row) => Ok(Some(StoredKeypair {
                id: row.get::<String>(0).unwrap_or_default(),
                public_key: row.get::<Vec<u8>>(1).unwrap_or_default(),
                encrypted_secret_key: row.get::<Vec<u8>>(2).unwrap_or_default(),
                nonce: row.get::<Vec<u8>>(3).unwrap_or_default(),
                display_name: row.get::<String>(4).unwrap_or_else(|_| "Sovereign".into()),
                created_at: row.get::<String>(5).unwrap_or_default(),
            })),
            None => Ok(None),
        }
    }

    // ─── Transactions ───
    pub async fn record_tx(&self, tx_type: &str, amount: i64, desc: Option<&str>, hash: Option<&str>) -> Result<Transaction, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO transactions (id, tx_type, amount_sats, description, timestamp, payment_hash) VALUES (?1,?2,?3,?4,?5,?6)",
            libsql::params![id.clone(), tx_type, amount, desc.unwrap_or(""), now.clone(), hash.unwrap_or("")],
        ).await.map_err(|e| e.to_string())?;
        Ok(Transaction { id, tx_type: tx_type.into(), amount_sats: amount, description: desc.map(String::from), timestamp: now, payment_hash: hash.map(String::from) })
    }

    pub async fn get_transactions(&self) -> Result<Vec<Transaction>, String> {
        let mut rows = self.conn.query("SELECT * FROM transactions ORDER BY timestamp DESC", ())
            .await.map_err(|e| e.to_string())?;
        let mut txs = Vec::new();
        while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
            txs.push(Transaction {
                id: row.get::<String>(0).unwrap_or_default(),
                tx_type: row.get::<String>(1).unwrap_or_default(),
                amount_sats: row.get::<i64>(2).unwrap_or(0),
                description: row.get::<String>(3).ok(),
                timestamp: row.get::<String>(4).unwrap_or_default(),
                payment_hash: row.get::<String>(5).ok(),
            });
        }
        Ok(txs)
    }

    // ─── State Persistence (Phase 1.1) ───

    /// Save a JSON snapshot of an engine's state under the given key.
    pub async fn save_state(&self, key: &str, json_data: &str) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT OR REPLACE INTO state_snapshots (key, data, updated_at) VALUES (?1, ?2, ?3)",
            libsql::params![key, json_data, now],
        ).await.map_err(|e| format!("save_state({}): {}", key, e))?;
        Ok(())
    }

    /// Save several state snapshots in a single SQLite transaction.
    /// Reduces fsync overhead from N to 1 and gives all-or-nothing semantics.
    pub async fn save_states(&self, items: &[(&str, &str)]) -> Result<(), String> {
        if items.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute("BEGIN", ()).await
            .map_err(|e| format!("save_states begin: {}", e))?;
        for (key, json_data) in items {
            if let Err(e) = self.conn.execute(
                "INSERT OR REPLACE INTO state_snapshots (key, data, updated_at) VALUES (?1, ?2, ?3)",
                libsql::params![*key, *json_data, now.clone()],
            ).await {
                // Roll back on any error so we don't leave a half-written batch.
                let _ = self.conn.execute("ROLLBACK", ()).await;
                return Err(format!("save_states({}): {}", key, e));
            }
        }
        self.conn.execute("COMMIT", ()).await
            .map_err(|e| format!("save_states commit: {}", e))?;
        Ok(())
    }

    /// Load a JSON snapshot for the given key. Returns None if no snapshot exists.
    pub async fn load_state(&self, key: &str) -> Result<Option<String>, String> {
        let mut rows = self.conn.query(
            "SELECT data FROM state_snapshots WHERE key=?1",
            libsql::params![key],
        ).await.map_err(|e| format!("load_state({}): {}", key, e))?;
        match rows.next().await.map_err(|e| e.to_string())? {
            Some(row) => Ok(Some(row.get::<String>(0).unwrap_or_default())),
            None => Ok(None),
        }
    }
}
