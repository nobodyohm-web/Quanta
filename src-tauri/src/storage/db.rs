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
    /// **MOY-2 (AUDIT-2026-08-13)** — sel Argon2id du coffre, 16 octets d'`OsRng`.
    /// **Vide** pour une ligne écrite avant SALT-RANDOM-1 : le sel valait alors
    /// `BLAKE3(hex(public_key))[..16]`, donc il se recalculait depuis une donnée
    /// publique. Le vide est le seul discriminant de format, et il n'est plus
    /// jamais produit en écriture — c'est un chemin de **lecture** qui se referme
    /// au premier déverrouillage réussi (`update_keypair_vault`).
    #[serde(default)]
    pub kdf_salt: Vec<u8>,
    pub display_name: String,
    pub created_at: String,
}

/// libSQL database wrapper
pub struct Database {
    conn: libsql::Connection,
}

impl Database {
    pub async fn new(db_path: &Path) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            // A6 : le répertoire d'abord — un fichier en 0600 dans un répertoire
            // en 0755 reste illisible, mais le répertoire expose la liste de ce
            // qu'on stocke. 0700 est le bon défaut pour un dossier de secrets.
            Self::restrict_to_owner(parent)?;
        }
        let db = libsql::Builder::new_local(db_path)
            .build().await.map_err(|e| e.to_string())?;
        let conn = db.connect().map_err(|e| e.to_string())?;
        // **A6 (AUDIT-2026-08-13)** — `quanta.db` était créé en **0644** dans un
        // répertoire 0755, sans le moindre `set_permissions`. Or ce fichier porte
        // le coffre AES-256-GCM, la graine ML-DSA `pq_identity_v1` et
        // `biometric_wrap_v1` : n'importe quel processus local pouvait le copier
        // et attaquer la phrase hors ligne, à son rythme, sans limite d'essais —
        // ce qui vide tout le bénéfice des 88 ms d'Argon2id, calibrés contre un
        // attaquant qui doit passer par l'application.
        //
        // Appliqué APRÈS l'ouverture : libSQL crée le fichier, on le restreint
        // aussitôt. Les journaux `-wal` et `-shm` sont couverts eux aussi — ils
        // contiennent les mêmes octets en transit.
        Self::restrict_to_owner(db_path)?;
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut side = db_path.as_os_str().to_os_string();
            side.push(suffix);
            let side = std::path::PathBuf::from(side);
            if side.exists() {
                Self::restrict_to_owner(&side)?;
            }
        }
        let instance = Self { conn };
        instance.migrate().await?;
        Ok(instance)
    }

    /// A6 — 0600 pour un fichier, 0700 pour un répertoire. No-op hors Unix (il
    /// n'y a pas de bit de permission équivalent à poser ; le contrôle d'accès y
    /// passe par les ACL du profil utilisateur).
    fn restrict_to_owner(path: &Path) -> Result<(), String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
            let want = if meta.is_dir() { 0o700 } else { 0o600 };
            if meta.permissions().mode() & 0o777 != want {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(want))
                    .map_err(|e| format!("permissions {}: {e}", path.display()))?;
            }
        }
        #[cfg(not(unix))]
        {
            let _ = path;
        }
        Ok(())
    }

    async fn migrate(&self) -> Result<(), String> {
        let stmts = [
            "CREATE TABLE IF NOT EXISTS keypairs (id TEXT PRIMARY KEY, public_key BLOB NOT NULL, encrypted_secret_key BLOB NOT NULL, nonce BLOB NOT NULL, display_name TEXT NOT NULL DEFAULT 'Sovereign', created_at TEXT NOT NULL, kdf_salt BLOB)",
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
        // MOY-2 — même figure pour le sel Argon2id : la colonne est ajoutée aux
        // bases existantes, sans DEFAULT et donc NULL pour les lignes déjà là.
        // NULL se relit en `Vec` vide, ce qui EST le marqueur « coffre hérité » —
        // aucune ligne n'est réécrite ici, donc aucun coffre ne peut se fermer
        // sur une migration de schéma.
        let _ = self.conn
            .execute("ALTER TABLE keypairs ADD COLUMN kdf_salt BLOB", ())
            .await;
        Ok(())
    }

    // ─── Keypairs ───
    /// MOY-2 — `kdf_salt` : le sel Argon2id qui a chiffré `enc_sk`. Il est écrit
    /// dans la MÊME instruction que le ciphertext : les deux ne peuvent pas
    /// diverger, et un coffre neuf ne peut pas naître sans son sel.
    pub async fn store_keypair(
        &self, pk: &[u8], enc_sk: &[u8], nonce: &[u8], kdf_salt: &[u8], display_name: &str,
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO keypairs (id, public_key, encrypted_secret_key, nonce, kdf_salt, display_name, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            libsql::params![id.clone(), pk.to_vec(), enc_sk.to_vec(), nonce.to_vec(), kdf_salt.to_vec(), display_name, now],
        ).await.map_err(|e| e.to_string())?;
        Ok(id)
    }

    /// MOY-2 — réécrit le coffre d'une identité **existante** après re-chiffrement
    /// sous un sel aléatoire (migration du format hérité).
    ///
    /// `UPDATE` et non `INSERT` : `get_active_keypair` prend la ligne la plus
    /// récente, donc insérer aurait créé une seconde identité active et laissé
    /// l'ancienne derrière elle. Les trois champs partent dans une seule
    /// instruction — un coffre à moitié migré (nouveau ciphertext, ancien sel)
    /// serait définitivement inouvrable.
    pub async fn update_keypair_vault(
        &self, id: &str, enc_sk: &[u8], nonce: &[u8], kdf_salt: &[u8],
    ) -> Result<(), String> {
        self.conn.execute(
            "UPDATE keypairs SET encrypted_secret_key=?2, nonce=?3, kdf_salt=?4 WHERE id=?1",
            libsql::params![id, enc_sk.to_vec(), nonce.to_vec(), kdf_salt.to_vec()],
        ).await.map_err(|e| format!("update_keypair_vault: {}", e))?;
        Ok(())
    }

    pub async fn get_active_keypair(&self) -> Result<Option<StoredKeypair>, String> {
        let mut rows = self.conn.query(
            "SELECT id, public_key, encrypted_secret_key, nonce, display_name, created_at, kdf_salt \
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
                // NULL (colonne ajoutée après coup) ⇒ vide ⇒ coffre hérité.
                kdf_salt: row.get::<Vec<u8>>(6).unwrap_or_default(),
            })),
            None => Ok(None),
        }
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

    /// **HAUT-3 (AUDIT-2026-08-13)** — les clés de `state_snapshots` commençant par
    /// `prefix`, triées.
    ///
    /// Le coffre de fonds écrasé par une restauration est recopié sous une clé
    /// horodatée ; sans énumération, cette archive serait inatteignable — donc
    /// inutile. `LIKE` reçoit un motif échappé : `prefix` vient du code, mais un
    /// `%` qui s'y glisserait un jour transformerait une recherche en balayage
    /// complet de la table.
    pub async fn list_state_keys(&self, prefix: &str) -> Result<Vec<String>, String> {
        let escaped = prefix
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let mut rows = self.conn.query(
            "SELECT key FROM state_snapshots WHERE key LIKE ?1 ESCAPE '\\' ORDER BY key",
            libsql::params![format!("{escaped}%")],
        ).await.map_err(|e| format!("list_state_keys({}): {}", prefix, e))?;
        let mut keys = Vec::new();
        while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
            if let Ok(k) = row.get::<String>(0) {
                keys.push(k);
            }
        }
        Ok(keys)
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

#[cfg(test)]
mod audit_20260813_schema {
    //! **MOY-2 (AUDIT-2026-08-13)** — la migration de schéma elle-même : une base
    //! écrite avant SALT-RANDOM-1 n'a pas de colonne `kdf_salt`. Elle doit
    //! continuer à s'ouvrir et à rendre son coffre, sel vide (= sel hérité), puis
    //! accepter le re-chiffrement en place. Un coffre qui deviendrait illisible
    //! ici serait une perte de fonds pure et simple.
    use super::Database;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("quanta-schema-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// Écrit une base au **schéma d'avant le correctif** (celui de v3.15.1), sans
    /// passer par `Database` — sinon le test ne testerait que le schéma courant.
    async fn write_pre_fix_db(path: &std::path::Path) {
        let db = libsql::Builder::new_local(path).build().await.expect("build");
        let conn = db.connect().expect("connect");
        conn.execute(
            "CREATE TABLE keypairs (id TEXT PRIMARY KEY, public_key BLOB NOT NULL, \
             encrypted_secret_key BLOB NOT NULL, nonce BLOB NOT NULL, \
             display_name TEXT NOT NULL DEFAULT 'Sovereign', created_at TEXT NOT NULL)",
            (),
        )
        .await
        .expect("create legacy table");
        conn.execute(
            "INSERT INTO keypairs (id, public_key, encrypted_secret_key, nonce, display_name, created_at) \
             VALUES ('legacy-id', ?1, ?2, ?3, 'alice', '2026-01-01T00:00:00+00:00')",
            libsql::params![vec![1u8; 32], vec![2u8; 48], vec![3u8; 12]],
        )
        .await
        .expect("insert legacy row");
    }

    #[tokio::test]
    async fn moy2_a_pre_salt_schema_opens_and_reads_as_a_legacy_vault() {
        let dir = scratch("legacy-schema");
        let path = dir.join("quanta.db");
        write_pre_fix_db(&path).await;

        // Ouverture par le code courant : la colonne est ajoutée, rien n'est réécrit.
        let db = Database::new(&path).await.expect("une base héritée DOIT s'ouvrir");
        let kp = db.get_active_keypair().await.expect("query").expect("row");
        assert_eq!(kp.id, "legacy-id");
        assert_eq!(kp.public_key, vec![1u8; 32]);
        assert_eq!(kp.encrypted_secret_key, vec![2u8; 48]);
        assert_eq!(kp.nonce, vec![3u8; 12]);
        assert_eq!(kp.display_name, "alice");
        assert!(
            kp.kdf_salt.is_empty(),
            "colonne absente ⇒ NULL ⇒ sel vide, le marqueur « coffre hérité »"
        );

        // Puis le re-chiffrement en place (migration SALT-RANDOM-1).
        db.update_keypair_vault("legacy-id", &[9u8; 48], &[8u8; 12], &[7u8; 16])
            .await
            .expect("update");
        let migrated = db.get_active_keypair().await.expect("query").expect("row");
        assert_eq!(migrated.id, "legacy-id", "même ligne : pas de seconde identité active");
        assert_eq!(migrated.kdf_salt, vec![7u8; 16]);
        assert_eq!(migrated.encrypted_secret_key, vec![9u8; 48]);
        assert_eq!(migrated.public_key, vec![1u8; 32], "la clé publique ne bouge pas");

        // Et la base rouvre proprement une fois migrée (idempotence du `migrate`).
        drop(db);
        let reopened = Database::new(&path).await.expect("réouverture");
        let after = reopened.get_active_keypair().await.expect("query").expect("row");
        assert_eq!(after.kdf_salt, vec![7u8; 16]);
    }

    /// HAUT-3 — l'énumération des archives ne doit pas se laisser piéger par les
    /// jokers de `LIKE` : `_` et `%` sont littéraux, sinon `pq_identity_archive_`
    /// ramasserait des clés qui ne sont pas des archives.
    #[tokio::test]
    async fn haut3_archive_listing_matches_the_prefix_literally() {
        let dir = scratch("archive-listing");
        let db = Database::new(&dir.join("quanta.db")).await.expect("db");
        db.save_state("pq_identity_archive_2026", "a").await.expect("save");
        db.save_state("pq_identity_archiveX2026", "b").await.expect("save");
        db.save_state("pq_identity_v1", "c").await.expect("save");

        let keys = db.list_state_keys("pq_identity_archive_").await.expect("list");
        assert_eq!(keys, vec!["pq_identity_archive_2026".to_string()]);
        assert!(db.list_state_keys("aucun_prefixe_").await.expect("list").is_empty());
    }
}
