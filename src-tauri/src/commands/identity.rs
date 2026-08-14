//! Identity & vault commands — creation, unlock (password + Touch ID),
//! recovery phrase, and address forms. The wallet-lifecycle helpers
//! (`create_wallet`/`unlock_wallet`/`restore_wallet`) are `pub(crate)`:
//! shared by these Tauri commands and the headless `quanta-node` daemon
//! (`crate::node_runtime`).

use crate::commands::error::CmdError;
use crate::security;
use crate::security::pq_vault::{stored_salt, PQVault};
use crate::AppState;
use std::sync::Arc;
use zeroize::{Zeroize, Zeroizing};

/// **MOY-5 (AUDIT-2026-08-13) — SECRET-STRING-1 : plus de secret en `String` nue.**
///
/// `get_recovery_phrase` renvoyait `mnemonic.to_string()` : les 24 mots — donc
/// l'autorité complète et définitive sur les fonds — dans une `String` du tas que
/// personne n'effaçait. Le `Mnemonic` est pourtant zeroize-on-drop : c'est sa
/// copie textuelle qui survivait, jusqu'à réutilisation du tas. Autrement dit
/// jusqu'à un core dump, un swap ou une capture mémoire — précisément le modèle
/// de menace pour lequel tout le reste du zeroize a été écrit. Même trou côté
/// `get_recovery_key` (`formatted.join("-")`, la graine Ed25519 entière).
///
/// Ce type se sérialise **exactement comme la chaîne qu'il contient** : l'IPC et
/// le frontend ne voient aucune différence, et le tampon est effacé au drop.
///
/// Ce qu'il ne fait pas : la recopie que `serde_json` produit dans Tauri pour
/// franchir l'IPC reste hors de portée d'ici. La fuite est réduite d'un cran,
/// pas supprimée ; la supprimer voudrait dire ne jamais faire traverser la phrase.
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
}

impl serde::Serialize for SecretString {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

// ─── Identity (PQ Vault) ─────────────────────────────────────────

#[tauri::command]
pub async fn check_identity(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    let db = state.db.lock().await;
    Ok(db.as_ref().ok_or("DB not ready")?.get_active_keypair().await?.is_some())
}

#[tauri::command]
pub async fn create_identity(
    state: tauri::State<'_, Arc<AppState>>, display_name: String, password: String,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    let display_name = display_name.trim().to_string();
    if display_name.is_empty() { return Err(CmdError::DisplayNameRequired.into()); }
    if password.len() < 8 { return Err(CmdError::WeakPassword.into()); }

    create_wallet(state.inner(), &display_name, &password).await
}

/// Create + persist a new wallet identity (Ed25519 + the **independent** ML-DSA
/// primary — the PQ tx-authority key the ledger binds, seeded separately via OsRng
/// so a quantum break of Ed25519 cannot yield it). Headless-callable: shared by the
/// `create_identity` command and the `quanta-node` daemon's persistent wallet.
pub(crate) async fn create_wallet(
    state: &Arc<AppState>,
    display_name: &str,
    password: &str,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    let display_name = display_name.trim().to_string();
    if display_name.is_empty() {
        return Err(CmdError::DisplayNameRequired.into());
    }
    if password.len() < 8 {
        return Err(CmdError::WeakPassword.into());
    }
    // LOCK-ORDER-1 : `db` puis `crypto`, dans cet ordre — c'est celui de
    // `unlock_wallet`. L'ordre inverse qui régnait ici rendait un `create` et un
    // `unlock` concurrents mutuellement bloquants (chacun tenant le verrou que
    // l'autre attend).
    let db = state.db.lock().await;
    let dbref = db.as_ref().ok_or("DB not ready")?;
    // **HAUT-3** — la création ne remplace jamais une identité en place.
    if dbref.get_active_keypair().await?.is_some() || load_fund_anchor(dbref).await?.is_some() {
        return Err(IDENTITY_ALREADY_EXISTS.to_string());
    }
    let mut engine = state.crypto.lock().await;
    let (id, pk_bytes, enc_sk, nonce, kdf_salt) =
        PQVault::create_identity(&mut engine, &display_name, password)?;
    let (pq_pk, pq_enc, pq_nonce, pq_salt) = PQVault::create_pq_identity(&mut engine, password)?;
    dbref.store_keypair(&pk_bytes, &enc_sk, &nonce, &kdf_salt, &display_name).await?;
    dbref
        .save_state(PQ_IDENTITY_KEY, &pq_identity_blob(&pq_pk, &pq_enc, &pq_nonce, &pq_salt))
        .await?;
    // ANCHOR-1 — l'ancre part **après** le coffre : ancrer d'abord et échouer
    // ensuite laisserait une ancre sans coffre, c'est-à-dire un portefeuille
    // définitivement refusé au déverrouillage.
    dbref.save_state(PQ_ANCHOR_KEY, &fund_address_of(&pq_pk)?).await?;
    Ok(id)
}

/// Storage key for the persisted post-quantum (ML-DSA-65) primary identity bundle.
const PQ_IDENTITY_KEY: &str = "pq_identity_v1";

/// **HAUT-2/HAUT-3 (AUDIT-2026-08-13) — ANCHOR-1 : l'adresse de fonds attendue.**
///
/// Le déverrouillage lisait `pq_identity_v1` et, sur ligne absente, **fabriquait
/// une identité ML-DSA neuve** en silence. Un `DELETE FROM state_snapshots WHERE
/// key='pq_identity_v1'` — à la portée de tout processus local, d'une restauration
/// de sauvegarde partielle ou d'une simple corruption — suffisait donc à faire
/// perdre l'autorité sur la totalité du solde on-chain : l'utilisateur voyait une
/// adresse différente et un solde à zéro, sans un seul avertissement. Ce n'est pas
/// un vol (le mot de passe reste exigé), c'est un sabotage/rançon.
///
/// Cette clé fige l'adresse ML-DSA que ce coffre DOIT redonner. Un coffre ancré
/// refuse tout déverrouillage qui n'y aboutit pas, au lieu de « réparer » l'absence
/// de la clé maîtresse. Un coffre non ancré (installation antérieure) s'ancre au
/// premier déverrouillage **réussi** : la migration ne peut pas fermer un coffre.
const PQ_ANCHOR_KEY: &str = "pq_fund_anchor_v1";

/// HAUT-3 — préfixe des blobs de coffre archivés avant écrasement (horodatés).
const PQ_ARCHIVE_PREFIX: &str = "pq_identity_archive_";

/// HAUT-2 — le refus. Il nomme la seule issue réelle : la phrase de 24 mots. Il
/// ne parle pas de mot de passe, parce que le mot de passe n'y peut rien — et
/// qu'un utilisateur qui croit s'être trompé de mot de passe réessaie, brûle le
/// backoff, et finit par accepter un portefeuille vide comme normal.
///
/// **C'est un code `err.*`, et pas une phrase.** `AuthGate.svelte` passe tout
/// message non reconnu par un repli qui affiche « mot de passe incorrect » : une
/// belle phrase française n'atteignait donc jamais l'écran de déverrouillage, et
/// le refus le plus grave du portefeuille s'y déguisait en le plus banal. Le
/// texte vit maintenant dans `i18n.generated.ts`, dans les six langues.
const FUND_KEY_LOST: &str = "err.fundKeyLost";

/// HAUT-3 — refus d'écraser une identité en place. `create_identity` et
/// `restore_from_phrase` écrivaient toutes deux la clé de fonds sur la même clé
/// KV en `INSERT OR REPLACE`, sans ré-authentification et sans passer par
/// `unlock_guard` : un seul `invoke` depuis le webview détruisait définitivement
/// l'autorité de dépense. La création n'a aucune raison légitime d'être appelée
/// sur un coffre existant — `check_identity` existe précisément pour ça.
const IDENTITY_ALREADY_EXISTS: &str = "err.identityAlreadyExists";

/// Serialize the encrypted ML-DSA primary bundle for the `state_snapshots` KV
/// (no `keypairs` schema migration; transparent to the frontend).
///
/// MOY-2 — `salt` rejoint le blob : depuis SALT-RANDOM-1 le sel Argon2id est
/// aléatoire, donc il n'est plus reconstructible depuis la clé publique. Un blob
/// sans `salt` est un blob d'avant le correctif et se relit tel quel.
fn pq_identity_blob(pq_pk: &str, enc_seed: &[u8], nonce: &[u8], salt: &[u8]) -> String {
    serde_json::json!({
        "pq_public_key": pq_pk,
        "encrypted_seed": enc_seed,
        "nonce": nonce,
        "salt": salt,
    })
    .to_string()
}

/// Le blob `pq_identity_v1` déserialisé. Trois appelants le lisaient en recopiant
/// le même bloc de `serde_json` ; une seule lecture, un seul jeu d'erreurs.
struct PqIdentityBlob {
    pq_public_key: String,
    encrypted_seed: Vec<u8>,
    nonce: Vec<u8>,
    /// MOY-2 — vide pour un blob écrit avant SALT-RANDOM-1 (sel hérité).
    salt: Vec<u8>,
}

fn parse_pq_identity_blob(json: &str) -> Result<PqIdentityBlob, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "PQ identity corrompue".to_string())?;
    let pq_public_key = v["pq_public_key"]
        .as_str()
        .ok_or("PQ identity invalide")?
        .to_string();
    let encrypted_seed: Vec<u8> = serde_json::from_value(v["encrypted_seed"].clone())
        .map_err(|_| "PQ identity invalide".to_string())?;
    let nonce: Vec<u8> = serde_json::from_value(v["nonce"].clone())
        .map_err(|_| "PQ identity invalide".to_string())?;
    // Champ absent ⇒ `Null` ⇒ erreur de conversion ⇒ vide : le marqueur « hérité ».
    let salt: Vec<u8> = serde_json::from_value(v["salt"].clone()).unwrap_or_default();
    Ok(PqIdentityBlob { pq_public_key, encrypted_seed, nonce, salt })
}

/// ANCHOR-1 — l'adresse de fonds attendue, ou `None` si ce coffre n'a jamais été
/// ancré. Une valeur vide compte comme absente (le wrap biométrique utilise déjà
/// cette convention d'effacement).
async fn load_fund_anchor(dbref: &crate::storage::db::Database) -> Result<Option<String>, String> {
    Ok(dbref
        .load_state(PQ_ANCHOR_KEY)
        .await?
        .filter(|s| !s.trim().is_empty()))
}

/// ANCHOR-1 — l'adresse de fonds dérivée d'une clé publique ML-DSA hex. C'est la
/// valeur ancrée : l'adresse est ce que l'utilisateur reconnaît, et elle est une
/// fonction pure de la clé (`BLAKE3(ADDR_DOMAIN ‖ pk)`).
fn fund_address_of(pq_public_key_hex: &str) -> Result<String, String> {
    let pk_bytes = hex::decode(pq_public_key_hex).map_err(|_| "PQ identity invalide".to_string())?;
    Ok(security::CryptoEngine::ml_dsa_address_hex(&pk_bytes))
}

#[tauri::command]
pub async fn unlock_identity(
    state: tauri::State<'_, Arc<AppState>>, password: String,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    state.unlock_guard.check().await?;
    match unlock_wallet(state.inner(), &password).await {
        Ok(id) => {
            state.unlock_guard.on_success().await;
            Ok(id)
        }
        Err(e) => {
            state.unlock_guard.on_failure().await;
            Err(e)
        }
    }
}

/// Unlock the stored wallet identity (Ed25519 + restore/TOFU-establish the ML-DSA
/// primary). Headless-callable: shared by the `unlock_identity` command (which adds
/// the brute-force guard) and the daemon's persistent wallet (self-unlock).
pub(crate) async fn unlock_wallet(
    state: &Arc<AppState>,
    password: &str,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    let db = state.db.lock().await;
    let dbref = db.as_ref().ok_or("DB not ready")?;
    let kp = dbref.get_active_keypair().await?.ok_or("No identity")?;
    let mut engine = state.crypto.lock().await;
    let id = PQVault::unlock_identity(
        &mut engine,
        &kp.public_key,
        &kp.encrypted_secret_key,
        &kp.nonce,
        stored_salt(&kp.kdf_salt),
        password,
        &kp.display_name,
        &kp.created_at,
    )?;
    // ANCHOR-1 — l'adresse de fonds que ce coffre doit redonner. Lue AVANT le
    // blob : c'est elle qui décide si un blob manquant est une identité d'avant
    // PQ-MIG-3 (légitime) ou la disparition de la clé de dépense (sabotage).
    let anchor = load_fund_anchor(dbref).await?;
    // PQ-MIG-3 §3: restore the independent ML-DSA primary. A legacy identity created
    // before PQ-MIG-3 has no bundle yet — TOFU-establish one at first unlock.
    match dbref.load_state(PQ_IDENTITY_KEY).await? {
        Some(json) => {
            let blob = parse_pq_identity_blob(&json).map_err(|e| {
                // Coffre ancré + blob illisible = même perte que le blob absent.
                if anchor.is_some() { FUND_KEY_LOST.to_string() } else { e }
            })?;
            // Refus **avant** les 88 ms d'Argon2id : l'adresse que le blob
            // revendique doit déjà être l'ancre. Sinon c'est une substitution
            // intégrale, et rien ne justifie de dériver une clé pour elle.
            if let Some(expected) = anchor.as_deref() {
                let claimed = fund_address_of(&blob.pq_public_key)
                    .map_err(|_| FUND_KEY_LOST.to_string())?;
                if !claimed.eq_ignore_ascii_case(expected) {
                    engine.lock();
                    log::error!(
                        "◈ [Security] ANCHOR-1 : coffre de fonds substitué (adresse annoncée ≠ ancre) — déverrouillage refusé"
                    );
                    return Err(FUND_KEY_LOST.to_string());
                }
            }
            PQVault::unlock_pq_identity(
                &mut engine,
                &blob.pq_public_key,
                &blob.encrypted_seed,
                &blob.nonce,
                stored_salt(&blob.salt),
                password,
            )?;
            // Seule cette adresse-ci fait autorité : elle vient de la graine
            // effectivement déchiffrée, pas d'un champ du blob.
            let derived = engine
                .pq_address_hex()
                .ok_or_else(|| "Adresse ML-DSA indisponible".to_string())?;
            match anchor.as_deref() {
                Some(expected) if !derived.eq_ignore_ascii_case(expected) => {
                    engine.lock();
                    log::error!(
                        "◈ [Security] ANCHOR-1 : la clé déchiffrée ne redonne pas l'adresse ancrée — déverrouillage refusé"
                    );
                    return Err(FUND_KEY_LOST.to_string());
                }
                Some(_) => {}
                // Migration : coffre antérieur à ANCHOR-1. On l'ancre sur ce
                // déverrouillage **réussi** — donc sur une clé que le mot de
                // passe de l'utilisateur vient d'ouvrir, jamais sur une valeur
                // fournie par l'attaquant.
                None => {
                    dbref.save_state(PQ_ANCHOR_KEY, &derived).await?;
                    log::info!("◈ [Security] ANCHOR-1 : identité de fonds ancrée pour ce coffre");
                }
            }
            // MOY-2 — même occasion : re-chiffrement au format à sel aléatoire.
            migrate_vault_salts(dbref, &engine, &kp, &blob, password).await;
        }
        None => {
            // **HAUT-2** — le coffre a déjà eu une identité de fonds : son blob
            // ne peut pas avoir disparu de lui-même. Fabriquer une clé neuve ici
            // rendrait le sabotage indiscernable d'un portefeuille vide.
            if anchor.is_some() {
                engine.lock();
                log::error!(
                    "◈ [Security] ANCHOR-1 : blob de fonds absent alors que le coffre est ancré — déverrouillage refusé"
                );
                return Err(FUND_KEY_LOST.to_string());
            }
            let (pq_pk, pq_enc, pq_nonce, pq_salt) =
                PQVault::create_pq_identity(&mut engine, password)?;
            dbref
                .save_state(PQ_IDENTITY_KEY, &pq_identity_blob(&pq_pk, &pq_enc, &pq_nonce, &pq_salt))
                .await?;
            // Ancré immédiatement : ce TOFU est le dernier que ce coffre connaîtra.
            dbref.save_state(PQ_ANCHOR_KEY, &fund_address_of(&pq_pk)?).await?;
            // M4 (AUDIT-2026-07-25) — a brand-new identity invalidates quick unlock.
            invalidate_biometric_wrap(dbref).await;
        }
    }
    Ok(id)
}

/// **MOY-2 — la seconde moitié de SALT-RANDOM-1 : la migration.**
///
/// Un coffre écrit avant le correctif porte un sel dérivé de sa clé publique. Le
/// relire suffit à l'ouvrir, mais le laisser tel quel garderait le précalcul
/// possible pour toujours. Ici le moteur est **déjà déverrouillé** et le mot de
/// passe vient d'être prouvé : on re-chiffre les deux coffres sous un sel neuf.
///
/// Aucune erreur n'est propagée, et c'est délibéré : l'utilisateur est
/// légitimement déverrouillé avec un coffre parfaitement fonctionnel. Faire
/// échouer son déverrouillage parce qu'une amélioration de format n'a pas abouti
/// (disque plein, base verrouillée) serait échanger un risque hypothétique contre
/// une panne certaine. La migration se retentera au prochain déverrouillage.
///
/// Chaque couche est écrite en **une seule** instruction (l'`UPDATE` de la ligne
/// de coffre, le blob JSON entier) : un demi-écrit — nouveau ciphertext, ancien
/// sel — serait un coffre définitivement perdu.
async fn migrate_vault_salts(
    dbref: &crate::storage::db::Database,
    engine: &security::CryptoEngine,
    kp: &crate::storage::db::StoredKeypair,
    blob: &PqIdentityBlob,
    password: &str,
) {
    let ed_legacy = kp.kdf_salt.is_empty();
    let pq_legacy = blob.salt.is_empty();
    if !ed_legacy && !pq_legacy {
        return;
    }
    let mut migrated = false;
    if ed_legacy {
        match PQVault::reencrypt_ed_vault(engine, password) {
            Ok((ct, nonce, salt)) => {
                match dbref.update_keypair_vault(&kp.id, &ct, &nonce, &salt).await {
                    Ok(()) => migrated = true,
                    Err(e) => log::warn!("◈ [Security] SALT-RANDOM-1 (Ed25519) reporté : {e}"),
                }
            }
            Err(e) => log::warn!("◈ [Security] SALT-RANDOM-1 (Ed25519) reporté : {e}"),
        }
    }
    if pq_legacy {
        match PQVault::reencrypt_pq_vault(engine, password) {
            Ok((pk, ct, nonce, salt)) => {
                let json = pq_identity_blob(&pk, &ct, &nonce, &salt);
                match dbref.save_state(PQ_IDENTITY_KEY, &json).await {
                    Ok(()) => migrated = true,
                    Err(e) => log::warn!("◈ [Security] SALT-RANDOM-1 (ML-DSA) reporté : {e}"),
                }
            }
            Err(e) => log::warn!("◈ [Security] SALT-RANDOM-1 (ML-DSA) reporté : {e}"),
        }
    }
    if migrated {
        // Le wrap Touch ID enveloppe les clés **dérivées**, qui dépendent du sel :
        // après re-chiffrement il n'ouvre plus rien. Le laisser en place ferait
        // exactement ce que M4 décrit — « activé » à l'écran, échec à chaque
        // empreinte, et le backoff partagé qui se referme sur le mot de passe.
        if matches!(dbref.load_state(BIOMETRIC_WRAP_KEY).await, Ok(Some(s)) if !s.is_empty()) {
            invalidate_biometric_wrap(dbref).await;
            log::info!(
                "◈ [Security] SALT-RANDOM-1 : coffre re-chiffré, Touch ID à réactiver depuis les réglages"
            );
        } else {
            log::info!("◈ [Security] SALT-RANDOM-1 : coffre re-chiffré sous sel aléatoire");
        }
    }
}

/// M4 — drop the Keychain KEK and the stored wrap.
///
/// Neither `create_wallet` nor `restore_wallet` used to do this, so the KEK and the
/// on-disk wrap of the OLD identity's Argon2id-derived keys survived a new or
/// restored wallet. `biometric_status` then reported `enabled: true`, the user
/// tapped Touch ID, the OS happily returned the stale KEK, AES-GCM failed on the
/// new vault blobs — and every attempt burned the brute-force backoff shared with
/// password unlock. The scenario is exactly the one RECOVER-1 exists for: forgot
/// the password, restore from the phrase.
///
/// Failures are logged, never fatal: an unusable Keychain must not make wallet
/// creation fail, and the wrap row is what `biometric_status` actually reads.
///
/// # Why there is no unit test here
/// The real path calls `delete_kek`, which operates on the **live macOS
/// Keychain** — a test exercising it would delete the developer's own Touch ID
/// KEK on every `cargo test` run. Testing only the DB half would assert nothing
/// the type system does not already give us. Verified by review instead; the
/// behaviour is a two-line reuse of `disable_biometric_unlock`, which is the
/// same code path users already exercise from Settings.
async fn invalidate_biometric_wrap(dbref: &crate::storage::db::Database) {
    if let Err(e) = tokio::task::spawn_blocking(security::biometric::delete_kek).await {
        log::warn!("◈ [Security] Keychain KEK non supprimé : {e}");
    }
    if let Err(e) = dbref.save_state(BIOMETRIC_WRAP_KEY, "").await {
        log::warn!("◈ [Security] wrap biométrique non effacé : {e}");
    }
}

/// **A2 (AUDIT-2026-08-13) — LOCK-1 : verrouiller le portefeuille, pour de bon.**
///
/// Il n'existait aucune contrepartie Rust au verrouillage : l'interface passait
/// `ready = false` et le `CryptoEngine` gardait la clé de dépense jusqu'à la fin
/// du processus. Cette commande efface réellement l'autorité en mémoire
/// ([`security::CryptoEngine::lock`], secrets zeroize-on-drop) ; il faut ensuite
/// un mot de passe (ou Touch ID) pour signer quoi que ce soit.
///
/// Idempotente et toujours `Ok` : verrouiller un portefeuille déjà verrouillé
/// n'est pas une erreur, et une commande de mise en sécurité ne doit jamais
/// donner à l'appelant une raison de ne pas l'appeler.
#[tauri::command]
pub async fn lock_wallet(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    state.crypto.lock().await.lock();
    log::info!("◈ [Security] portefeuille verrouillé — autorité de dépense effacée de la mémoire");
    Ok(())
}

/// A2 — le portefeuille est-il déverrouillé ? Le frontend doit lire ceci plutôt
/// que son propre drapeau : l'état réel est celui du moteur, pas celui de l'UI.
#[tauri::command]
pub async fn is_wallet_unlocked(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    Ok(state.crypto.lock().await.is_unlocked())
}

/// RECOVER-1 — the wallet's **recovery phrase**: a 24-word BIP39 mnemonic of the
/// ML-DSA **fund** seed. Whoever holds this phrase controls the funds — it is the
/// true backup. (The legacy `get_recovery_key` exports the Ed25519 transport seed,
/// which does NOT recover funds; the phrase does.)
///
/// **A2/A1 (AUDIT-2026-08-13) — REAUTH-1 : le mot de passe est désormais exigé.**
///
/// Cette commande ne demandait **rien**. Combinée au fait que les 34 commandes
/// applicatives échappent à l'ACL de Tauri (A1), n'importe quel JavaScript
/// s'exécutant dans le webview appelait `invoke("get_recovery_phrase")` et
/// repartait avec les 24 mots — c'est-à-dire avec les fonds. La « re-saisie du
/// mot de passe » n'existait que dans l'écran qui la demandait poliment.
///
/// La vérification passe par le déchiffrement effectif du coffre ML-DSA
/// (`unlock_wallet`), donc par Argon2id : un mauvais mot de passe échoue, et
/// échoue **lentement**. Le compteur anti-force-brute (`unlock_guard`) couvre cet
/// appel comme il couvre le déverrouillage — sinon cette commande devenait
/// l'oracle de mot de passe que le déverrouillage n'est pas.
#[tauri::command]
pub async fn get_recovery_phrase(
    state: tauri::State<'_, Arc<AppState>>,
    password: String,
) -> Result<SecretString, String> {
    state.unlock_guard.check().await?;
    if unlock_wallet(state.inner(), &password).await.is_err() {
        state.unlock_guard.on_failure().await;
        return Err(CmdError::UnlockRefused.into());
    }
    state.unlock_guard.on_success().await;
    let engine = state.crypto.lock().await;
    // Self-wiping 32-byte ML-DSA root seed (the fund-controlling secret).
    let seed = engine.get_pq_seed_bytes()?;
    let mnemonic = bip39::Mnemonic::from_entropy(&seed[..])
        .map_err(|_| CmdError::RecoveryPhraseUnavailable)?;
    // MOY-5 : la copie textuelle des 24 mots naît directement auto-effaçante.
    Ok(SecretString::new(mnemonic.to_string()))
}

/// RECOVER-1 — restore a wallet from its 24-word recovery phrase, under a NEW
/// password + display name (e.g. a new device, or a forgotten password).
#[tauri::command]
pub async fn restore_from_phrase(
    state: tauri::State<'_, Arc<AppState>>,
    mnemonic: String,
    display_name: String,
    password: String,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    restore_wallet(state.inner(), &mnemonic, &display_name, &password).await
}

/// RECOVER-1 — reconstruct the ML-DSA **fund authority** from a BIP39 phrase's seed
/// (deterministic keygen → the SAME address as the original wallet) and persist it
/// encrypted under `password`. The Ed25519 transport key is freshly generated (it is
/// regenerable and never the account authority). Headless-callable.
pub(crate) async fn restore_wallet(
    state: &Arc<AppState>,
    mnemonic: &str,
    display_name: &str,
    password: &str,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    let display_name = display_name.trim().to_string();
    if display_name.is_empty() {
        return Err(CmdError::DisplayNameRequired.into());
    }
    if password.len() < 8 {
        return Err(CmdError::WeakPassword.into());
    }
    // Decode the phrase → the 32-byte fund seed (validates the checksum).
    let parsed = bip39::Mnemonic::parse_normalized(mnemonic.trim())
        .map_err(|_| CmdError::InvalidRecoveryPhrase)?;
    let mut entropy = parsed.to_entropy();
    if entropy.len() != 32 {
        entropy.zeroize();
        return Err(CmdError::RecoveryPhraseLength.into());
    }
    let mut seed: [u8; 32] = entropy[..32].try_into().map_err(|_| CmdError::InvalidRecoveryPhrase)?;
    entropy.zeroize();

    // LOCK-ORDER-1 : `db` avant `crypto`, comme `unlock_wallet` et `create_wallet`.
    let db = state.db.lock().await;
    let dbref = db.as_ref().ok_or("DB not ready")?;
    // **HAUT-3** — la restauration est le seul écrasement légitime de la clé de
    // fonds : l'utilisateur y prouve la phrase de 24 mots, et exiger en plus le
    // mot de passe en place viderait de son sens le cas « mot de passe oublié ».
    // Elle reste destructive, et elle est déclenchable par un simple `invoke`.
    // On archive donc l'ancien coffre sous une clé horodatée avant de l'écraser :
    // le blob archivé reste chiffré (il n'ouvre rien sans l'ancien mot de passe),
    // mais une restauration hostile cesse d'être irréversible.
    archive_previous_fund_key(dbref).await;
    let mut engine = state.crypto.lock().await;
    // Fresh Ed25519 transport + the QuantaIdentity bundle (transport is regenerable).
    let (id, pk_bytes, enc_sk, nonce, kdf_salt) =
        PQVault::create_identity(&mut engine, &display_name, password)?;
    // Reconstruct the ML-DSA fund authority from the phrase's seed, encrypted anew.
    let (pq_pk, pq_enc, pq_nonce, pq_salt) =
        PQVault::restore_pq_identity(&mut engine, &seed, password)?;
    seed.zeroize();
    dbref.store_keypair(&pk_bytes, &enc_sk, &nonce, &kdf_salt, &display_name).await?;
    dbref
        .save_state(PQ_IDENTITY_KEY, &pq_identity_blob(&pq_pk, &pq_enc, &pq_nonce, &pq_salt))
        .await?;
    // ANCHOR-1 — l'ancre suit la clé restaurée. Écrite en dernier, et écrasée sans
    // état d'âme : la phrase EST l'autorité, elle prime sur l'ancre précédente.
    dbref.save_state(PQ_ANCHOR_KEY, &fund_address_of(&pq_pk)?).await?;
    // M4 (AUDIT-2026-07-25) — the restored vault is encrypted under a NEW password,
    // so the previous identity's Keychain KEK can only ever fail against it while
    // reporting "enabled" and consuming the shared unlock backoff.
    invalidate_biometric_wrap(dbref).await;
    Ok(id)
}

/// HAUT-3 — recopie le coffre de fonds en place sous une clé horodatée avant
/// qu'un `INSERT OR REPLACE` ne l'efface. Best-effort et silencieux à l'échec :
/// c'est un filet de sécurité, il ne doit jamais empêcher une restauration —
/// l'utilisateur qui restaure a peut-être déjà perdu tout le reste.
async fn archive_previous_fund_key(dbref: &crate::storage::db::Database) {
    let existing = match dbref.load_state(PQ_IDENTITY_KEY).await {
        Ok(Some(json)) if !json.is_empty() => json,
        _ => return,
    };
    let key = format!("{PQ_ARCHIVE_PREFIX}{}", chrono::Utc::now().to_rfc3339());
    match dbref.save_state(&key, &existing).await {
        Ok(()) => log::info!("◈ [Security] ancien coffre de fonds archivé sous « {key} »"),
        Err(e) => log::warn!("◈ [Security] archivage du coffre précédent impossible : {e}"),
    }
}

// ─── Touch ID quick unlock (security/biometric.rs) ──────────────
//
// A random KEK sits in the macOS Keychain behind `.BIOMETRY_CURRENT_SET`
// (the OS demands the fingerprint at read time and invalidates the item if
// enrolled prints change). It wraps the two Argon2id-DERIVED vault keys —
// the password is never stored. Password unlock stays the fallback.

/// SQLite settings key of the KEK-wrapped vault keys.
const BIOMETRIC_WRAP_KEY: &str = "biometric_wrap_v1";

/// Whether the machine supports biometry (probed once per run) and whether
/// quick unlock is currently enabled for this wallet.
#[tauri::command]
pub async fn biometric_status(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let supported = match SUPPORTED.get() {
        Some(v) => *v,
        None => {
            let v = tokio::task::spawn_blocking(security::biometric::probe_available)
                .await
                .unwrap_or(false);
            *SUPPORTED.get_or_init(|| v)
        }
    };
    let enabled = {
        let db = state.db.lock().await;
        match db.as_ref() {
            Some(d) => matches!(d.load_state(BIOMETRIC_WRAP_KEY).await, Ok(Some(s)) if !s.is_empty()),
            None => false,
        }
    };
    Ok(serde_json::json!({ "supported": supported, "enabled": enabled }))
}

/// Enable Touch ID quick unlock. Requires the password (deliberate re-auth:
/// enabling a new unlock factor is a security-sensitive act), which is used
/// ONLY to re-derive + verify the two vault keys before wrapping them.
#[tauri::command]
pub async fn enable_biometric_unlock(
    state: tauri::State<'_, Arc<AppState>>, password: String,
) -> Result<(), String> {
    let db = state.db.lock().await;
    let dbref = db.as_ref().ok_or("DB not ready")?;
    let kp = dbref.get_active_keypair().await?.ok_or("No identity")?;
    let pq_json = dbref
        .load_state(PQ_IDENTITY_KEY)
        .await?
        .ok_or(CmdError::UnlockFirst)?;
    let blob = parse_pq_identity_blob(&pq_json)?;

    // Derive both vault keys and PROVE the password is right (decrypt both
    // blobs; plaintexts wiped immediately). Opaque error on mismatch.
    // MOY-2 : les sels persistés — sans eux la dérivation ne redonnerait pas les
    // clés qui ouvrent réellement les coffres, et le wrap Touch ID naîtrait mort.
    let ed_key = PQVault::derive_ed_vault_key(
        &password,
        &kp.public_key,
        stored_salt(&kp.kdf_salt),
    )?;
    let pq_key = PQVault::derive_pq_vault_key(
        &password,
        &blob.pq_public_key,
        stored_salt(&blob.salt),
    )?;
    let mut probe = security::cipher::decrypt(&kp.encrypted_secret_key, &ed_key, &kp.nonce)
        .map_err(|_| CmdError::WrongPassword)?;
    probe.zeroize();
    let mut probe = security::cipher::decrypt(&blob.encrypted_seed, &pq_key, &blob.nonce)
        .map_err(|_| CmdError::WrongPassword)?;
    probe.zeroize();

    // Random KEK → Keychain (biometry-gated); KEK-wrapped derived keys → disk.
    let mut kek = [0u8; 32];
    {
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut kek);
    }
    let mut plain = Vec::with_capacity(64);
    plain.extend_from_slice(&ed_key[..]);
    plain.extend_from_slice(&pq_key[..]);
    let wrapped = security::cipher::encrypt_and_wipe(&mut plain, &kek)?;
    let kek_owned = kek;
    let store = tokio::task::spawn_blocking(move || {
        let r = security::biometric::store_kek(&kek_owned);
        let mut k = kek_owned;
        k.zeroize();
        r
    })
    .await
    .map_err(|_| "Tâche Keychain interrompue".to_string())?;
    kek.zeroize();
    store?;
    dbref
        .save_state(
            BIOMETRIC_WRAP_KEY,
            &serde_json::json!({ "wrapped": wrapped.ciphertext, "nonce": wrapped.nonce }).to_string(),
        )
        .await?;
    log::info!("◈ [Security] Touch ID quick unlock ENABLED (Keychain biometry-gated KEK)");
    Ok(())
}

/// Disable Touch ID quick unlock: the Keychain KEK is deleted (the on-disk
/// wrap becomes undecryptable garbage, also cleared).
#[tauri::command]
pub async fn disable_biometric_unlock(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    tokio::task::spawn_blocking(security::biometric::delete_kek)
        .await
        .map_err(|_| "Tâche Keychain interrompue".to_string())?;
    let db = state.db.lock().await;
    if let Some(d) = db.as_ref() {
        d.save_state(BIOMETRIC_WRAP_KEY, "").await?;
    }
    log::info!("◈ [Security] Touch ID quick unlock disabled");
    Ok(())
}

/// Unlock the wallet with Touch ID: macOS shows the biometric sheet while we
/// read the KEK; the unwrapped derived keys decrypt both vault blobs. Ends in
/// exactly the same engine state as a password unlock.
#[tauri::command]
pub async fn unlock_biometric(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    state.unlock_guard.check().await?;
    let db = state.db.lock().await;
    let dbref = db.as_ref().ok_or("DB not ready")?;
    let kp = dbref.get_active_keypair().await?.ok_or("No identity")?;
    let wrap_json = dbref
        .load_state(BIOMETRIC_WRAP_KEY)
        .await?
        .filter(|s| !s.is_empty())
        .ok_or(CmdError::BiometricNotEnabled)?;
    let w: serde_json::Value =
        serde_json::from_str(&wrap_json).map_err(|_| "Wrap biométrique corrompu".to_string())?;
    let wrapped: Vec<u8> = serde_json::from_value(w["wrapped"].clone())
        .map_err(|_| "Wrap biométrique invalide".to_string())?;
    let wnonce: Vec<u8> = serde_json::from_value(w["nonce"].clone())
        .map_err(|_| "Wrap biométrique invalide".to_string())?;
    // ANCHOR-1 — la même règle que sur le chemin mot de passe : un coffre ancré
    // dont le blob de fonds a disparu ne se « répare » pas, il se refuse.
    let anchor = load_fund_anchor(dbref).await?;
    let pq_json = match dbref.load_state(PQ_IDENTITY_KEY).await? {
        Some(json) => json,
        None if anchor.is_some() => return Err(FUND_KEY_LOST.to_string()),
        None => return Err("Identité PQ absente".to_string()),
    };
    let blob = parse_pq_identity_blob(&pq_json)
        .map_err(|e| if anchor.is_some() { FUND_KEY_LOST.to_string() } else { e })?;

    // The Touch ID moment — blocking while the system sheet is up.
    let kek = tokio::task::spawn_blocking(security::biometric::read_kek)
        .await
        .map_err(|_| "Tâche Keychain interrompue".to_string())?;
    let kek = match kek {
        Ok(k) => k,
        Err(e) => {
            state.unlock_guard.on_failure().await;
            return Err(e);
        }
    };
    let kek_arr: [u8; 32] = kek[..]
        .try_into()
        .map_err(|_| "KEK invalide".to_string())?;
    let mut keys = match security::cipher::decrypt(&wrapped, &kek_arr, &wnonce) {
        Ok(k) => k,
        Err(_) => {
            state.unlock_guard.on_failure().await;
            return Err(CmdError::UnlockRefused.into());
        }
    };
    if keys.len() != 64 {
        keys.zeroize();
        state.unlock_guard.on_failure().await;
        return Err(CmdError::UnlockRefused.into());
    }
    let mut ed_key = [0u8; 32];
    let mut pq_key = [0u8; 32];
    ed_key.copy_from_slice(&keys[..32]);
    pq_key.copy_from_slice(&keys[32..]);
    keys.zeroize();

    let mut engine = state.crypto.lock().await;
    let unlocked = PQVault::unlock_identity_with_key(
        &mut engine,
        &kp.public_key,
        &kp.encrypted_secret_key,
        &kp.nonce,
        &ed_key,
        &kp.display_name,
        &kp.created_at,
    )
    .and_then(|id| {
        PQVault::unlock_pq_identity_with_key(
            &mut engine,
            &blob.pq_public_key,
            &blob.encrypted_seed,
            &blob.nonce,
            &pq_key,
        )
        .map(|_| id)
    });
    ed_key.zeroize();
    pq_key.zeroize();
    match unlocked {
        Ok(id) => {
            // ANCHOR-1 — Touch ID prouve la présence de l'utilisateur, pas
            // l'intégrité du coffre : la clé ouverte doit redonner l'adresse
            // ancrée, exactement comme sur le chemin mot de passe.
            if let Some(expected) = anchor.as_deref() {
                let derived = engine.pq_address_hex().unwrap_or_default();
                if !derived.eq_ignore_ascii_case(expected) {
                    engine.lock();
                    state.unlock_guard.on_failure().await;
                    log::error!(
                        "◈ [Security] ANCHOR-1 : déverrouillage Touch ID refusé (adresse de fonds ≠ ancre)"
                    );
                    return Err(FUND_KEY_LOST.to_string());
                }
            }
            state.unlock_guard.on_success().await;
            log::info!("◈ [Security] Wallet unlocked via Touch ID");
            Ok(id)
        }
        Err(_) => {
            state.unlock_guard.on_failure().await;
            // Opaque: don't reveal which layer failed.
            Err(CmdError::UnlockRefused.into())
        }
    }
}

// ─── Public key / address forms ─────────────────────────────────

#[tauri::command]
pub async fn get_public_key(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    // PQ-MIG-3B: the wallet identity exposed to the UI is the ML-DSA **address**
    // (the value identity: balance key, receive address, `from`/`to`, @pseudo
    // target). The Ed25519 transport key is internal plumbing and never the
    // user's account. (Command name kept for wire/UI compat, per CLAUDE.md.)
    state
        .crypto
        .lock()
        .await
        .pq_address_hex()
        .ok_or_else(|| "Identité ML-DSA absente".to_string())
}

#[tauri::command]
pub async fn get_recovery_key(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<SecretString, String> {
    let engine = state.crypto.lock().await;
    // ZEROIZE-SWEEP: `secret` is a self-wiping `Zeroizing<Vec<u8>>`; wrap the
    // full-secret hex `String` in `Zeroizing` too so neither intermediate
    // lingers on the heap after this command returns. Only the formatted
    // recovery key the user explicitly asked to see leaves the function.
    let secret = engine.get_secret_bytes()?;
    // Format: 8 groups of 8 hex chars (64 chars total = 32 bytes Ed25519 secret)
    let hex = zeroize::Zeroizing::new(hex::encode(&secret));
    let hs: &str = &hex;
    let formatted: Vec<&str> = (0..8).map(|i| &hs[i * 8..(i + 1) * 8]).collect();
    // MOY-5 : `join` reconstruisait le secret complet dans une `String` nue.
    Ok(SecretString::new(formatted.join("-")))
}

/// The user's **public receive address** in canonical `qta1…` (Bech32m) form — the
/// checksummed address to share, put in a QR, or hand to an exchange. Unlike the raw
/// hex (`get_public_key`), a single mistyped character fails the checksum instead of
/// silently pointing at another account. The hex form stays the on-chain identity.
/// See [`crate::security::address`].
#[tauri::command]
pub async fn get_receive_address(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    state
        .crypto
        .lock()
        .await
        .pq_address_bech32()
        .ok_or_else(|| "Identité ML-DSA absente".to_string())
}

/// Exchange-grade address validation: `true` iff `address` is a well-formed Quanta
/// `qta1…` address (Bech32m checksum + length). This is the `validateaddress`
/// primitive a wallet send-form and an exchange integration both need.
#[tauri::command]
pub fn validate_address(address: String) -> bool {
    crate::security::address::is_valid(&address)
}

// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod audit_20260813_vault {
    //! Non-régressions de l'audit du 13/08/2026 sur le coffre : **HAUT-2/HAUT-3**
    //! (ANCHOR-1, l'ancre d'identité de fonds), **MOY-2** (SALT-RANDOM-1, le sel
    //! Argon2id aléatoire et sa migration), **MOY-4** (liaison coffre↔identité) et
    //! **MOY-5** (le secret ne sort plus en `String` nue).
    //!
    //! Ces tests montent une vraie base libSQL sur disque et appellent les mêmes
    //! fonctions que les commandes Tauri et le démon `quanta-node` : l'audit
    //! notait HAUT-2 « prouvé au niveau DB, non prouvé de bout en bout » — il
    //! l'est ici, `AppState` compris.

    use super::*;
    use crate::security::{cipher, CryptoEngine};
    use crate::storage::db::Database;
    use std::path::{Path, PathBuf};

    const PW: &str = "correct horse battery staple";

    /// Répertoire de travail unique par test **et** par processus : un reliquat
    /// d'une exécution tuée ne peut pas faire passer un test par accident.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("quanta-vault-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    async fn open_state(dir: &Path) -> Arc<AppState> {
        let state = Arc::new(AppState::new());
        let db = Database::new(&dir.join("quanta.db")).await.expect("open db");
        *state.db.lock().await = Some(db);
        state
    }

    /// Ferme la connexion (l'`AppState` est abandonné) pour que la base puisse
    /// être manipulée à côté, comme le ferait un processus tiers.
    async fn close_db(state: &Arc<AppState>) {
        *state.db.lock().await = None;
    }

    async fn read_state(state: &Arc<AppState>, key: &str) -> Option<String> {
        let db = state.db.lock().await;
        db.as_ref().expect("db").load_state(key).await.expect("load_state")
    }

    async fn write_state(state: &Arc<AppState>, key: &str, value: &str) {
        let db = state.db.lock().await;
        db.as_ref().expect("db").save_state(key, value).await.expect("save_state");
    }

    async fn fund_address(state: &Arc<AppState>) -> Option<String> {
        state.crypto.lock().await.pq_address_hex()
    }

    /// L'attaque, littéralement : la ligne du coffre de fonds est supprimée par
    /// un processus tiers ayant l'écriture sur `quanta.db` (autre utilisateur
    /// local, malware sans privilège, restauration d'un snapshot partiel).
    async fn delete_row(dir: &Path, key: &str) {
        let db = libsql::Builder::new_local(dir.join("quanta.db"))
            .build()
            .await
            .expect("raw open");
        let conn = db.connect().expect("raw connect");
        conn.execute("DELETE FROM state_snapshots WHERE key=?1", libsql::params![key])
            .await
            .expect("raw delete");
    }

    /// **HAUT-2** — le constat exact : supprimer `pq_identity_v1` faisait
    /// **fabriquer une clé de fonds neuve** au déverrouillage suivant. La victime
    /// voyait une autre adresse et un solde à zéro, sans un mot d'explication, et
    /// perdait l'autorité sur la totalité de son solde on-chain.
    ///
    /// Sans le correctif ce test échoue sur la première assertion : le
    /// déverrouillage renvoyait `Ok` (et l'adresse de fonds avait changé).
    #[tokio::test]
    async fn haut2_a_deleted_fund_blob_refuses_the_unlock_instead_of_minting_a_new_key() {
        let dir = scratch("haut2-deleted");
        let state = open_state(&dir).await;
        create_wallet(&state, "alice", PW).await.expect("create");
        let original = fund_address(&state).await.expect("adresse de fonds");
        close_db(&state).await;

        delete_row(&dir, PQ_IDENTITY_KEY).await;

        // Redémarrage de l'application sur la base sabotée.
        let state2 = open_state(&dir).await;
        let err = unlock_wallet(&state2, PW)
            .await
            .expect_err("un coffre ancré sans clé de fonds NE DOIT PAS s'ouvrir");
        assert!(
            err == "err.fundKeyLost",
            "l'erreur doit dire quoi faire (restaurer depuis la phrase), pas seulement échouer : {err}"
        );
        assert!(
            !state2.crypto.lock().await.is_unlocked(),
            "aucune autorité de dépense ne doit rester chargée après un refus"
        );
        assert!(
            read_state(&state2, PQ_IDENTITY_KEY).await.is_none(),
            "le refus ne doit surtout pas écrire une identité de fonds de remplacement"
        );
        // Et l'ancre survit : c'est elle qui rend le sabotage détectable au
        // prochain démarrage, et qui documente l'adresse à récupérer.
        assert_eq!(
            read_state(&state2, PQ_ANCHOR_KEY).await.as_deref(),
            Some(original.as_str()),
            "l'ancre nomme l'adresse que la phrase de 24 mots doit redonner"
        );
    }

    /// **HAUT-2, migration** — un coffre créé avant ANCHOR-1 n'a pas d'ancre. Il
    /// doit s'ouvrir normalement et s'ancrer sur ce déverrouillage **réussi** (donc
    /// sur une clé que le mot de passe vient d'ouvrir), puis être protégé comme
    /// les autres. Un coffre existant qui échouerait ici serait un échec total.
    #[tokio::test]
    async fn haut2_a_vault_from_before_the_anchor_anchors_itself_at_first_unlock() {
        let dir = scratch("haut2-migrate");
        let state = open_state(&dir).await;
        create_wallet(&state, "alice", PW).await.expect("create");
        let original = fund_address(&state).await.expect("adresse");
        close_db(&state).await;

        // Retour à l'état « avant le correctif » : le coffre existe, l'ancre non.
        delete_row(&dir, PQ_ANCHOR_KEY).await;

        let state2 = open_state(&dir).await;
        assert!(read_state(&state2, PQ_ANCHOR_KEY).await.is_none(), "point de départ : pas d'ancre");
        unlock_wallet(&state2, PW).await.expect("un coffre sans ancre doit s'ouvrir");
        assert_eq!(fund_address(&state2).await.as_deref(), Some(original.as_str()));
        assert_eq!(
            read_state(&state2, PQ_ANCHOR_KEY).await.as_deref(),
            Some(original.as_str()),
            "le premier déverrouillage réussi ancre le coffre"
        );

        // Deuxième déverrouillage : l'ancre est maintenant vérifiée, pas subie.
        let state3 = open_state(&dir).await;
        unlock_wallet(&state3, PW).await.expect("le coffre ancré s'ouvre toujours");
        assert_eq!(fund_address(&state3).await.as_deref(), Some(original.as_str()));

        // Et il est désormais protégé : la même suppression est refusée.
        close_db(&state3).await;
        delete_row(&dir, PQ_IDENTITY_KEY).await;
        let state4 = open_state(&dir).await;
        assert!(
            unlock_wallet(&state4, PW).await.is_err(),
            "une fois ancré, le coffre ne fabrique plus de clé de remplacement"
        );
    }

    /// **HAUT-2 + MOY-4** — substitution intégrale : l'attaquant remplace le blob
    /// de fonds par le sien, chiffré sous le **même** mot de passe (il l'a observé,
    /// ou c'est un mot de passe imposé). Sans ancre ni liaison, l'application
    /// ouvrait le coffre de l'attaquant et affichait son adresse de réception.
    #[tokio::test]
    async fn haut2_a_substituted_fund_blob_is_refused() {
        let dir = scratch("haut2-subst");
        let state = open_state(&dir).await;
        create_wallet(&state, "alice", PW).await.expect("create");
        let original = fund_address(&state).await.expect("adresse");

        // Coffre de fonds d'un attaquant, valide en tout point sauf l'identité.
        let mut rogue_engine = CryptoEngine::new();
        let (rogue_pk, rogue_ct, rogue_nonce, rogue_salt) =
            PQVault::create_pq_identity(&mut rogue_engine, PW).expect("rogue vault");
        write_state(
            &state,
            PQ_IDENTITY_KEY,
            &pq_identity_blob(&rogue_pk, &rogue_ct, &rogue_nonce, &rogue_salt),
        )
        .await;
        close_db(&state).await;

        let state2 = open_state(&dir).await;
        let err = unlock_wallet(&state2, PW).await.expect_err("coffre substitué");
        assert_eq!(err, "err.fundKeyLost", "code stable attendu : {err}");
        assert!(
            !state2.crypto.lock().await.is_unlocked(),
            "l'identité substituée ne doit pas rester chargée dans le moteur"
        );
        assert_ne!(
            fund_address(&state2).await.as_deref(),
            Some(CryptoEngine::ml_dsa_address_hex(
                &hex::decode(&rogue_pk).expect("hex")
            ))
            .as_deref(),
            "l'adresse de l'attaquant ne doit jamais devenir l'adresse affichée"
        );
        assert_eq!(
            read_state(&state2, PQ_ANCHOR_KEY).await.as_deref(),
            Some(original.as_str()),
            "l'ancre reste celle de la victime"
        );
    }

    /// **MOY-4** — variante plus fine : le blob **ment sur sa propre clé
    /// publique**. Il annonce l'adresse ancrée (donc il passe le contrôle amont)
    /// mais chiffre la graine d'une autre identité. Seule la clé effectivement
    /// déchiffrée fait autorité.
    #[tokio::test]
    async fn moy4_a_blob_lying_about_its_own_public_key_is_refused() {
        let dir = scratch("moy4-lying");
        let state = open_state(&dir).await;
        create_wallet(&state, "alice", PW).await.expect("create");
        let honest = read_state(&state, PQ_IDENTITY_KEY).await.expect("blob");
        let honest = parse_pq_identity_blob(&honest).expect("parse");

        // Graine étrangère, chiffrée sous le sel/mot de passe de la victime, mais
        // présentée sous la clé publique de la victime.
        let mut rogue = CryptoEngine::new();
        rogue.import_pq_identity(&[0x42u8; 32]).expect("rogue seed");
        let mut seed = rogue.get_pq_seed_bytes().expect("seed").to_vec();
        let key = PQVault::derive_pq_vault_key(PW, &honest.pq_public_key, Some(&honest.salt))
            .expect("derive");
        let enc = cipher::encrypt_and_wipe(&mut seed, &key).expect("encrypt");
        write_state(
            &state,
            PQ_IDENTITY_KEY,
            &pq_identity_blob(&honest.pq_public_key, &enc.ciphertext, &enc.nonce, &honest.salt),
        )
        .await;
        close_db(&state).await;

        let state2 = open_state(&dir).await;
        assert!(
            unlock_wallet(&state2, PW).await.is_err(),
            "la clé déchiffrée doit être comparée à l'identité annoncée, pas seulement déchiffrée"
        );
        assert!(!state2.crypto.lock().await.is_unlocked());
    }

    /// **HAUT-3** — `create_identity` écrasait la clé de fonds sur la même clé KV,
    /// sans ré-authentification et sans passer par le garde anti-force-brute : un
    /// seul `invoke` depuis le webview détruisait le portefeuille.
    #[tokio::test]
    async fn haut3_creating_a_second_identity_cannot_destroy_the_first() {
        let dir = scratch("haut3-create");
        let state = open_state(&dir).await;
        create_wallet(&state, "alice", PW).await.expect("create");
        let original = fund_address(&state).await.expect("adresse");
        let blob = read_state(&state, PQ_IDENTITY_KEY).await.expect("blob");

        let err = create_wallet(&state, "attaquant", "autre-mot-de-passe")
            .await
            .expect_err("la création doit refuser d'écraser une identité en place");
        // Le message actionnable vit dans i18n (6 langues) ; ce qui traverse l'IPC
        // est le CODE stable, seul contrat testable côté Rust.
        assert_eq!(err, "err.identityAlreadyExists", "code stable attendu : {err}");
        assert_eq!(
            read_state(&state, PQ_IDENTITY_KEY).await.as_deref(),
            Some(blob.as_str()),
            "le coffre de fonds doit être intact octet pour octet"
        );

        close_db(&state).await;
        let state2 = open_state(&dir).await;
        unlock_wallet(&state2, PW).await.expect("le portefeuille d'origine s'ouvre toujours");
        assert_eq!(fund_address(&state2).await.as_deref(), Some(original.as_str()));
    }

    /// **HAUT-3** — la restauration, elle, reste légitime (elle prouve la phrase),
    /// mais elle est destructive : l'ancien coffre doit être archivé, pas effacé.
    #[tokio::test]
    async fn haut3_a_restore_archives_the_previous_fund_vault() {
        let dir = scratch("haut3-restore");
        let state = open_state(&dir).await;
        create_wallet(&state, "alice", PW).await.expect("create");
        let previous_blob = read_state(&state, PQ_IDENTITY_KEY).await.expect("blob");

        // Phrase d'un autre portefeuille (entropie fixe ⇒ adresse calculable).
        let phrase = bip39::Mnemonic::from_entropy(&[0x11u8; 32]).expect("mnemonic").to_string();
        let mut expected_engine = CryptoEngine::new();
        expected_engine.import_pq_identity(&[0x11u8; 32]).expect("import");
        let expected = expected_engine.pq_address_hex().expect("adresse attendue");

        restore_wallet(&state, &phrase, "alice-restaurée", "un-autre-mot-de-passe")
            .await
            .expect("restore");
        assert_eq!(fund_address(&state).await.as_deref(), Some(expected.as_str()));
        assert_eq!(
            read_state(&state, PQ_ANCHOR_KEY).await.as_deref(),
            Some(expected.as_str()),
            "l'ancre suit la clé restaurée — la phrase fait autorité"
        );

        // L'ancien coffre est toujours là, sous une clé horodatée énumérable.
        let archived = {
            let db = state.db.lock().await;
            let dbref = db.as_ref().expect("db");
            let keys = dbref.list_state_keys(PQ_ARCHIVE_PREFIX).await.expect("list");
            assert_eq!(keys.len(), 1, "exactement une archive après une restauration");
            dbref.load_state(&keys[0]).await.expect("load")
        };
        assert_eq!(
            archived.as_deref(),
            Some(previous_blob.as_str()),
            "le coffre écrasé doit rester récupérable (chiffré) sous sa clé d'archive"
        );
    }

    /// **MOY-2** — le sel Argon2id n'est plus une fonction de la clé publique :
    /// il est tiré d'`OsRng` et persisté. L'assertion mord littéralement sur
    /// l'ancienne formule — avant le correctif, `kdf_salt` VALAIT ces 16 octets,
    /// calculables par quiconque connaît l'adresse de la victime, donc avant même
    /// le vol du fichier de coffre.
    #[tokio::test]
    async fn moy2_the_kdf_salt_is_random_and_persisted_not_derived_from_public_data() {
        let dir = scratch("moy2-random");
        let state = open_state(&dir).await;
        create_wallet(&state, "alice", PW).await.expect("create");

        let kp = {
            let db = state.db.lock().await;
            db.as_ref().expect("db").get_active_keypair().await.expect("kp").expect("some")
        };
        assert_eq!(kp.kdf_salt.len(), 16, "sel de 16 octets persisté avec le coffre");
        let predictable = CryptoEngine::blake3_hash(hex::encode(&kp.public_key).as_bytes());
        assert_ne!(
            kp.kdf_salt.as_slice(),
            &predictable[..16],
            "le sel Ed25519 ne doit plus être BLAKE3(clé publique)[..16]"
        );

        let blob = parse_pq_identity_blob(&read_state(&state, PQ_IDENTITY_KEY).await.expect("blob"))
            .expect("parse");
        assert_eq!(blob.salt.len(), 16, "sel de 16 octets dans le blob de fonds");
        let predictable_pq = CryptoEngine::blake3_hash(
            format!("QUANTA-PQ-MIG-1-vault-v1:{}", blob.pq_public_key).as_bytes(),
        );
        assert_ne!(
            blob.salt.as_slice(),
            &predictable_pq[..16],
            "le sel ML-DSA ne doit plus dériver de la clé publique révélée par chaque transaction"
        );

        // Deux portefeuilles, même mot de passe : sels différents (pas de table
        // précalculée réutilisable d'une installation à l'autre).
        let dir2 = scratch("moy2-random-2");
        let other = open_state(&dir2).await;
        create_wallet(&other, "bob", PW).await.expect("create");
        let kp2 = {
            let db = other.db.lock().await;
            db.as_ref().expect("db").get_active_keypair().await.expect("kp").expect("some")
        };
        assert_ne!(kp.kdf_salt, kp2.kdf_salt, "chaque coffre a son propre sel");
    }

    /// **MOY-2, migration ancien → neuf.** Le test qui compte : un coffre écrit
    /// par la version d'avant le correctif (sel dérivé, aucune colonne `kdf_salt`,
    /// aucun champ `salt`, aucune ancre) DOIT continuer à s'ouvrir, être
    /// re-chiffré sous un sel aléatoire, et **se rouvrir après**. Un utilisateur
    /// qui perdrait son coffre dans cette migration serait un échec total.
    #[tokio::test]
    async fn moy2_a_pre_fix_vault_opens_then_is_reencrypted_and_opens_again() {
        let dir = scratch("moy2-legacy");
        let state = open_state(&dir).await;

        // ── Fabrication du format hérité, exactement comme v3.15.1 l'écrivait ──
        let (public_key, ed_ct, ed_nonce, pq_pk, pq_ct, pq_nonce, expected_addr) = {
            let mut engine = CryptoEngine::new();
            let pk = engine.generate_keypair();
            let pq_pk = engine.generate_pq_identity().expect("pq");
            let addr = engine.pq_address_hex().expect("adresse");
            let mut sk = engine.get_secret_bytes().expect("sk").to_vec();
            // `None` == dérivation héritée : BLAKE3(hex(pk))[..16].
            let ed_key = PQVault::derive_ed_vault_key(PW, &pk.public_key_bytes, None).expect("ed key");
            let ed_enc = cipher::encrypt_and_wipe(&mut sk, &ed_key).expect("ed enc");
            let mut seed = engine.get_pq_seed_bytes().expect("seed").to_vec();
            let pq_key = PQVault::derive_pq_vault_key(PW, &pq_pk, None).expect("pq key");
            let pq_enc = cipher::encrypt_and_wipe(&mut seed, &pq_key).expect("pq enc");
            (
                pk.public_key_bytes,
                ed_enc.ciphertext,
                ed_enc.nonce,
                pq_pk,
                pq_enc.ciphertext,
                pq_enc.nonce,
                addr,
            )
        };
        {
            let db = state.db.lock().await;
            let dbref = db.as_ref().expect("db");
            // Sel vide == colonne absente/NULL sur une base d'avant le correctif.
            dbref
                .store_keypair(&public_key, &ed_ct, &ed_nonce, &[], "alice")
                .await
                .expect("store");
            // Blob sans champ `salt` — la forme exacte de l'ancien schéma.
            let legacy_blob = serde_json::json!({
                "pq_public_key": pq_pk,
                "encrypted_seed": pq_ct,
                "nonce": pq_nonce,
            })
            .to_string();
            dbref.save_state(PQ_IDENTITY_KEY, &legacy_blob).await.expect("save");
        }

        // ── 1. Le coffre hérité s'ouvre ────────────────────────────────────────
        let id = unlock_wallet(&state, PW).await.expect("un coffre hérité DOIT s'ouvrir");
        assert_eq!(hex::encode(&public_key), id.public_key_hex, "même identité Ed25519");
        assert_eq!(
            fund_address(&state).await.as_deref(),
            Some(expected_addr.as_str()),
            "même adresse de fonds : la migration ne déplace pas l'argent"
        );

        // ── 2. Il a été ré-écrit au format neuf ────────────────────────────────
        let migrated_kp = {
            let db = state.db.lock().await;
            db.as_ref().expect("db").get_active_keypair().await.expect("kp").expect("some")
        };
        assert_eq!(migrated_kp.kdf_salt.len(), 16, "coffre Ed25519 re-chiffré sous sel aléatoire");
        let migrated_blob =
            parse_pq_identity_blob(&read_state(&state, PQ_IDENTITY_KEY).await.expect("blob"))
                .expect("parse");
        assert_eq!(migrated_blob.salt.len(), 16, "coffre de fonds re-chiffré sous sel aléatoire");
        assert_eq!(
            read_state(&state, PQ_ANCHOR_KEY).await.as_deref(),
            Some(expected_addr.as_str()),
            "et ancré au passage"
        );
        assert_ne!(
            migrated_blob.encrypted_seed, pq_ct,
            "le ciphertext doit avoir changé, sinon rien n'a été re-chiffré"
        );

        // ── 3. Et il se rouvre — la garantie qui compte vraiment ───────────────
        close_db(&state).await;
        let state2 = open_state(&dir).await;
        unlock_wallet(&state2, PW).await.expect("le coffre migré DOIT se rouvrir");
        assert_eq!(fund_address(&state2).await.as_deref(), Some(expected_addr.as_str()));

        // Et une seconde migration ne se déclenche pas (idempotence) : mêmes
        // octets qu'après la première.
        let after = {
            let db = state2.db.lock().await;
            db.as_ref().expect("db").get_active_keypair().await.expect("kp").expect("some")
        };
        assert_eq!(after.kdf_salt, migrated_kp.kdf_salt, "pas de re-chiffrement en boucle");
    }

    /// **MOY-2** — un sel persisté tronqué (base modifiable par un tiers) est
    /// refusé : sans ce contrôle, un sel vide ramènerait la dérivation à une
    /// constante partagée par toutes les installations.
    #[test]
    fn moy2_a_truncated_stored_salt_is_refused() {
        assert!(PQVault::derive_ed_vault_key(PW, &[7u8; 32], Some(&[1u8; 8])).is_err());
        assert!(PQVault::derive_pq_vault_key(PW, "ab", Some(&[1u8; 15])).is_err());
        assert!(PQVault::derive_ed_vault_key(PW, &[7u8; 32], Some(&[1u8; 16])).is_ok());
        // Vide ⇒ `None` ⇒ chemin hérité, pas un sel de 0 octet.
        assert!(stored_salt(&[]).is_none());
        assert!(stored_salt(&[1u8; 16]).is_some());
    }

    /// **MOY-5** — la phrase de récupération traverse l'IPC sous la même forme
    /// qu'avant (une chaîne JSON : aucun changement de contrat), mais son tampon
    /// est auto-effaçant. Le test épingle le contrat de sérialisation, sans quoi
    /// « rendre le type sûr » casserait silencieusement le frontend.
    #[test]
    fn moy5_the_secret_string_stays_a_plain_json_string() {
        let s = SecretString::new("abandon abandon ability".to_string());
        assert_eq!(
            serde_json::to_string(&s).expect("serialize"),
            "\"abandon abandon ability\"",
            "le type auto-effaçant doit rester transparent sur le fil"
        );
        // Le tampon est bien un `Zeroizing` (une régression vers `String` nue ne
        // compilerait pas).
        let _: &Zeroizing<String> = &s.0;
    }
}
