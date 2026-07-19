// security/mod.rs — Post-Quantum Security Layer (QUANTA)
// ML-KEM (FIPS 203), ML-DSA (FIPS 204), AES-256-GCM, Argon2id, zeroize

pub mod pq_vault;
// Touch ID quick unlock (macOS Keychain, biometry-gated KEK)
pub mod biometric;
pub mod cipher;
pub mod crypto_agility;
pub mod hybrid_crypto;
// Public human-facing address encoding — `qta1…` Bech32m (BIP-350). Presentation
// layer over the canonical 32-byte address; the hex on-chain form is unchanged.
pub mod address;

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use fips204::ml_dsa_65;
use fips204::traits::Signer as _;
use hybrid_crypto::derive_ml_dsa;
use rand::rngs::OsRng;
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// Ed25519 keypair with automatic memory wipe on drop
#[derive(Clone)]
pub struct KeyPair {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicIdentity {
    pub public_key_hex: String,
    pub public_key_bytes: Vec<u8>,
    pub pq_public_key_hex: Option<String>,
}

/// PQ-MIG-1 — identité **primaire** post-quantique : paire ML-DSA-65 (FIPS 204),
/// racine ratifiée par ADR-007 (b).
///
/// Enracinée sur **sa propre** graine de 32 octets, générée **indépendamment** de
/// la couche Ed25519 : sa sécurité ne descend donc d'aucune clé classique — la
/// faille précise exposée par CRYPTO-ID-1 (casser Ed25519 ne doit **pas** livrer
/// la clé ML-DSA). La graine reproduit déterministiquement la paire via
/// [`derive_ml_dsa`], donc le vault persiste 32 octets et le rechargement redonne
/// la clé publique identique.
///
/// Les deux champs secrets sont effacés au drop : `seed` via `Zeroizing`, `sk`
/// car `fips204::ml_dsa_65::PrivateKey` est zeroize-on-drop. `pk_hex` est public.
struct MlDsaPrimary {
    seed: Zeroizing<[u8; 32]>,
    sk: ml_dsa_65::PrivateKey,
    pk_hex: String,
}

/// Core crypto engine with zeroize-on-drop sensitive state
pub struct CryptoEngine {
    key_pair: Option<KeyPair>,
    /// Clé secrète ML-DSA-65 + clé publique hex, dérivées de la graine Ed25519.
    /// `fips204::PrivateKey` est zeroize-on-drop (feature activée par défaut).
    /// (Couche héritée du chemin hybride tx — coexistence, voir `ml_dsa_primary`.)
    ml_dsa: Option<(ml_dsa_65::PrivateKey, String)>,
    /// PQ-MIG-1 : identité **primaire** post-quantique (ML-DSA-65), racine
    /// indépendante. Additive — n'altère aucun consommateur Ed25519 (coexistence).
    ml_dsa_primary: Option<MlDsaPrimary>,
}

/// PQ-MIG-2 §1 — étiquette de **séparation de domaine** pour la dérivation
/// d'adresse ML-DSA. Préfixée au hash pour qu'une adresse ne puisse **jamais**
/// entrer en collision avec un hash de bloc ou de transaction (même discipline
/// que [`hybrid_crypto`]'s `ML_DSA_DOMAIN` et `pos_consensus`'s `LEADER_VRF_DOMAIN`).
/// NE JAMAIS modifier : changerait l'adresse de **toutes** les identités existantes.
pub const ADDR_DOMAIN: &[u8] = b"QUANTA-ADDR-V1";

/// MSIG-1 — domain tag for **multisig account** address derivation, distinct from
/// [`ADDR_DOMAIN`] so a multisig address can never collide with a single-key one.
pub const MSIG_DOMAIN: &[u8] = b"QUANTA-MSIG-V1";

/// MSIG-1 — derive the 32-byte address of an **M-of-N multisig account** from its
/// policy (the registered ML-DSA public keys + the threshold).
///
/// The keys are **canonicalized** — sorted and de-duplicated — so the address is
/// independent of key order and of accidental duplicates, and the encoding is
/// **injective** (length-prefixed) so no two distinct policies collide. The account
/// address therefore *commits* to `{keys, threshold}`: a spend reveals them and they
/// cannot be swapped without changing the address (rebind-proof), exactly as a
/// single-key address commits to its one key. Pure & deterministic (C1-safe).
pub fn multisig_address_bytes(pubkeys: &[String], threshold: u32) -> [u8; 32] {
    let mut keys: Vec<&str> = pubkeys.iter().map(|s| s.as_str()).collect();
    keys.sort_unstable();
    keys.dedup();
    let mut h = blake3::Hasher::new();
    h.update(MSIG_DOMAIN);
    h.update(&(keys.len() as u32).to_le_bytes());
    for k in &keys {
        h.update(&(k.len() as u32).to_le_bytes());
        h.update(k.as_bytes());
    }
    h.update(&threshold.to_le_bytes());
    *h.finalize().as_bytes()
}

/// MSIG-1 — hex form of [`multisig_address_bytes`] (the on-chain `from`/`to` value).
pub fn multisig_address_hex(pubkeys: &[String], threshold: u32) -> String {
    hex::encode(multisig_address_bytes(pubkeys, threshold))
}

impl CryptoEngine {
    pub fn new() -> Self { Self { key_pair: None, ml_dsa: None, ml_dsa_primary: None } }

    pub fn generate_keypair(&mut self) -> PublicIdentity {
        let sk = SigningKey::generate(&mut OsRng);
        let vk = sk.verifying_key();
        let bytes = vk.to_bytes().to_vec();
        let hex_str = hex::encode(&bytes);
        // Dérive la paire ML-DSA-65 depuis la même graine (zéro secret en plus).
        let mut seed = sk.to_bytes();
        self.key_pair = Some(KeyPair { signing_key: sk, verifying_key: vk });
        self.ml_dsa = derive_ml_dsa(&seed);
        seed.zeroize();
        let pq_public_key_hex = self.ml_dsa.as_ref().map(|(_, h)| h.clone());
        PublicIdentity { public_key_hex: hex_str, public_key_bytes: bytes, pq_public_key_hex }
    }

    pub fn import_keypair(&mut self, sk_bytes: &[u8; 32]) -> Result<PublicIdentity, String> {
        let sk = SigningKey::from_bytes(sk_bytes);
        let vk = sk.verifying_key();
        let bytes = vk.to_bytes().to_vec();
        let hex_str = hex::encode(&bytes);
        self.key_pair = Some(KeyPair { signing_key: sk, verifying_key: vk });
        // Recalcule la paire ML-DSA-65 à partir de la graine restaurée.
        self.ml_dsa = derive_ml_dsa(sk_bytes);
        let pq_public_key_hex = self.ml_dsa.as_ref().map(|(_, h)| h.clone());
        Ok(PublicIdentity { public_key_hex: hex_str, public_key_bytes: bytes, pq_public_key_hex })
    }

    pub fn get_identity(&self) -> Result<PublicIdentity, String> {
        let kp = self.key_pair.as_ref().ok_or("No active keypair")?;
        let bytes = kp.verifying_key.to_bytes().to_vec();
        let pq_public_key_hex = self.ml_dsa.as_ref().map(|(_, h)| h.clone());
        Ok(PublicIdentity { public_key_hex: hex::encode(&bytes), public_key_bytes: bytes, pq_public_key_hex })
    }

    /// ZEROIZE-SWEEP (HARDEN-HYGIENE-1): returns the raw 32-byte Ed25519 secret
    /// in a **self-wiping** `Zeroizing<Vec<u8>>` (so every caller's copy is
    /// erased on drop — type-enforced, no caller can forget), and the transient
    /// `[u8; 32]` from `to_bytes()` is wiped before return.
    pub fn get_secret_bytes(&self) -> Result<Zeroizing<Vec<u8>>, String> {
        let mut arr = self
            .key_pair
            .as_ref()
            .ok_or("No active keypair")?
            .signing_key
            .to_bytes();
        let out = Zeroizing::new(arr.to_vec());
        arr.zeroize();
        Ok(out)
    }

    // ── PQ-MIG-1 : identité primaire ML-DSA-65 (racine post-quantique) ─────────

    /// PQ-MIG-1 §1 — génère une identité **primaire** ML-DSA-65 indépendante.
    /// La graine racine de 32 octets vient d'`OsRng` (production) ; la paire est
    /// ensuite dérivée *déterministiquement* de cette graine ([`derive_ml_dsa`]),
    /// donc la graine seule la reproduit (round-trip vault). Renvoie la clé
    /// publique ML-DSA (hex).
    pub fn generate_pq_identity(&mut self) -> Result<String, String> {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        let res = self.import_pq_identity(&seed);
        seed.zeroize();
        res
    }

    /// PQ-MIG-1 §2 — (r)établit l'identité primaire ML-DSA-65 depuis une graine
    /// racine de 32 octets. C'est le **chemin déterministe** : utilisé pour le
    /// rechargement vault ET la simulation (jamais d'entropie hedgée ⇒ C1 reste
    /// byte-identique). Renvoie la clé publique ML-DSA (hex).
    pub fn import_pq_identity(&mut self, seed: &[u8; 32]) -> Result<String, String> {
        let (sk, pk_hex) = derive_ml_dsa(seed).ok_or("ML-DSA keygen failed")?;
        self.ml_dsa_primary = Some(MlDsaPrimary {
            seed: Zeroizing::new(*seed),
            sk,
            pk_hex: pk_hex.clone(),
        });
        Ok(pk_hex)
    }

    /// PQ-MIG-1 — la clé publique de l'identité **primaire** (post-quantique),
    /// ML-DSA-65 hex. `None` tant qu'aucune identité primaire n'est établie.
    pub fn pq_identity_hex(&self) -> Option<String> {
        self.ml_dsa_primary.as_ref().map(|p| p.pk_hex.clone())
    }

    /// PQ-MIG-1 §2 — la graine racine ML-DSA de 32 octets, dans un `Zeroizing`
    /// **auto-effaçant** (toute copie appelante est effacée au drop), pour que le
    /// vault la chiffre au repos. Miroir de [`Self::get_secret_bytes`] (Ed25519).
    pub fn get_pq_seed_bytes(&self) -> Result<Zeroizing<Vec<u8>>, String> {
        let p = self.ml_dsa_primary.as_ref().ok_or("No ML-DSA primary identity")?;
        Ok(Zeroizing::new(p.seed.to_vec()))
    }

    /// PQ-MIG-1 — signature ML-DSA-65 de production avec l'identité primaire
    /// (hedgée via `OsRng`, résistance aux attaques par faute — jamais
    /// déterministe sur le chemin de production).
    pub fn sign_pq(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let p = self.ml_dsa_primary.as_ref().ok_or("No ML-DSA primary identity")?;
        let sig = p
            .sk
            .try_sign_with_rng(&mut OsRng, data, &[])
            .map_err(|_| "ML-DSA signing failed".to_string())?;
        Ok(sig.to_vec())
    }

    /// PQ-MIG-1 — signature ML-DSA-65 **déterministe** avec l'identité primaire,
    /// pour la simulation / le harnais DST uniquement. `#[cfg(test)]` ⇒
    /// physiquement absente des builds release (la dureté PQ de production ne
    /// peut pas régresser). Byte-reproductible (C1).
    #[cfg(test)]
    pub fn sign_pq_det(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let p = self.ml_dsa_primary.as_ref().ok_or("No ML-DSA primary identity")?;
        crate::security::hybrid_crypto::ml_dsa_sign_deterministic(&p.sk, data)
            .ok_or_else(|| "ML-DSA signing failed".to_string())
    }

    /// PQ-MIG-1 — vérifie une signature ML-DSA-65 contre une clé publique hex.
    /// Fin wrapper sur l'**unique** vérificateur ML-DSA du projet
    /// ([`hybrid_crypto::verify_ml_dsa`]) — aucune duplication de la primitive.
    pub fn verify_pq(pk_hex: &str, data: &[u8], sig: &[u8]) -> bool {
        crate::security::hybrid_crypto::verify_ml_dsa(pk_hex, data, sig)
    }

    // ── PQ-MIG-2 : adresses ML-DSA = BLAKE3(domaine ‖ clé publique) ────────────
    //
    // La clé publique ML-DSA fait ~1952 o — trop grande pour servir d'adresse. On
    // dérive l'**adresse** = hash BLAKE3 **domaine-séparé** ([`ADDR_DOMAIN`]) de la
    // clé publique. Fonction **pure et déterministe** (aucune entropie) ⇒ même clé
    // ⇒ même adresse sur tous les nœuds, donc C1 inchangé. Périmètre PQ-MIG-2 :
    // dérivation + encodage + liaison + exposition. On **ne câble pas** `from`/`to`
    // ni `verify_tx` (= PQ-MIG-3) — ces couches restent intactes.

    /// PQ-MIG-2 §1 — dérive l'**adresse** (32 octets) d'une clé publique ML-DSA
    /// brute : `adresse = BLAKE3(ADDR_DOMAIN ‖ pk_bytes)`. Sortie naturelle de
    /// BLAKE3, **sans troncature** (256 bits de résistance aux collisions).
    /// 🛑 longueur **32 o par défaut, réglable** : raccourcissable plus tard si une
    /// adresse plus courte est voulue. Pure & déterministe — aucune entropie.
    pub fn ml_dsa_address_bytes(pk_bytes: &[u8]) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(ADDR_DOMAIN);
        h.update(pk_bytes);
        *h.finalize().as_bytes()
    }

    /// PQ-MIG-2 §2 — encode une adresse (32 o) en hex (l'encodage textuel déjà
    /// utilisé pour les identités/clés du projet). Round-trip exact avec
    /// [`Self::decode_address`].
    pub fn encode_address(addr: &[u8; 32]) -> String {
        hex::encode(addr)
    }

    /// PQ-MIG-2 §2 — décode une adresse hex vers ses **32 octets**. Erreur
    /// **opaque** sur entrée malformée (hex invalide ou mauvaise longueur) —
    /// jamais de panique, jamais le type d'erreur réel (règle sécurité §3).
    pub fn decode_address(s: &str) -> Result<[u8; 32], String> {
        let bytes = hex::decode(s).map_err(|_| "Invalid address")?;
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| "Invalid address".to_string())
    }

    /// PQ-MIG-2 §1+§2 — adresse **textuelle** (hex) dérivée d'une clé publique
    /// ML-DSA brute. Raccourci `encode_address(ml_dsa_address_bytes(pk))`.
    pub fn ml_dsa_address_hex(pk_bytes: &[u8]) -> String {
        Self::encode_address(&Self::ml_dsa_address_bytes(pk_bytes))
    }

    /// PQ-MIG-2 §3 — **fonction de liaison** (`lie`) clé↔adresse : vrai **si et
    /// seulement si** `address == BLAKE3(ADDR_DOMAIN ‖ pk_bytes)`. C'est ce que
    /// `verify_tx` **exigera** en PQ-MIG-3 (la clé révélée doit hasher vers
    /// l'adresse `from`). Elle **mord** : une clé différente ⇒ `false`.
    pub fn address_binds_key(address: &[u8; 32], pk_bytes: &[u8]) -> bool {
        Self::ml_dsa_address_bytes(pk_bytes) == *address
    }

    /// PQ-MIG-2 §3 — variante de liaison sur représentations **hex** (adresse hex,
    /// clé publique hex), la forme que `verify_tx` manipule en pratique. Décode
    /// puis délègue à [`Self::address_binds_key`]. Toute entrée malformée ⇒
    /// `false` (jamais de panique ; un encodage cassé ne contourne pas la liaison).
    pub fn address_hex_binds_key_hex(addr_hex: &str, pk_hex: &str) -> bool {
        let address = match Self::decode_address(addr_hex) {
            Ok(a) => a,
            Err(_) => return false,
        };
        let pk_bytes = match hex::decode(pk_hex) {
            Ok(b) => b,
            Err(_) => return false,
        };
        Self::address_binds_key(&address, &pk_bytes)
    }

    /// PQ-MIG-2 §4 — l'**adresse** de l'identité **primaire** post-quantique du
    /// moteur (PQ-MIG-1), dérivée de sa clé publique ML-DSA. Lecture seule, pure.
    /// `None` tant qu'aucune identité primaire n'est établie.
    pub fn pq_address(&self) -> Option<[u8; 32]> {
        let pk_bytes = hex::decode(&self.ml_dsa_primary.as_ref()?.pk_hex).ok()?;
        Some(Self::ml_dsa_address_bytes(&pk_bytes))
    }

    /// PQ-MIG-2 §4 — l'adresse **textuelle** (hex) de l'identité primaire du
    /// moteur. `None` tant qu'aucune identité primaire n'est établie.
    pub fn pq_address_hex(&self) -> Option<String> {
        self.pq_address().map(|a| Self::encode_address(&a))
    }

    /// The **public** `qta1…` (Bech32m) address of the engine's primary PQ identity
    /// — the receive address a user shares. `None` until a primary identity exists.
    pub fn pq_address_bech32(&self) -> Option<String> {
        self.pq_address().map(|a| address::encode(&a))
    }

    // ── PQ-MIG-3 : autorité de transaction signée par le primaire ML-DSA ───────

    /// PQ-MIG-3 §2 — signe la préimage d'autorité d'une transaction sur **les deux
    /// couches** pour le chemin de production : Ed25519 (autorise la **liaison**
    /// on-chain de la clé ML-DSA la première fois, et reste co-facteur) **plus**
    /// l'**identité primaire** ML-DSA (PQ-MIG-1) — la clé que le ledger **lie** au
    /// compte. Renvoie `(sig_ed25519, sig_ml_dsa, clé_publique_primaire_hex)`.
    ///
    /// C'est le **primaire indépendant** qui est lié, **jamais** la couche héritée
    /// `ml_dsa` dérivée de la graine Ed25519 : casser Ed25519 ne reconstruit donc
    /// pas la clé d'autorité (fermeture de CRYPTO-ID-1). ML-DSA **hedgé** (`OsRng`).
    pub fn sign_tx_authority(&self, payload: &[u8]) -> Result<(Vec<u8>, Vec<u8>, String), String> {
        let kp = self.key_pair.as_ref().ok_or("No active keypair")?;
        let p = self.ml_dsa_primary.as_ref().ok_or("No ML-DSA primary identity")?;
        let ed = kp.signing_key.sign(payload).to_bytes().to_vec();
        let pq = p
            .sk
            .try_sign_with_rng(&mut OsRng, payload, &[])
            .map_err(|_| "ML-DSA signing failed".to_string())?;
        Ok((ed, pq.to_vec(), p.pk_hex.clone()))
    }

    /// PQ-MIG-3 §2 — variante **déterministe** (sim / harnais DST) : Ed25519 (déjà
    /// déterministe) + ML-DSA primaire signé via le RNG dérivé du message
    /// ([`hybrid_crypto::ml_dsa_sign_deterministic`]), pour des runs
    /// byte-reproductibles (C1). `#[cfg(test)]` ⇒ **absente du release** (la dureté
    /// PQ de production ne peut pas régresser vers le déterministe).
    #[cfg(test)]
    pub fn sign_tx_authority_det(&self, payload: &[u8]) -> Result<(Vec<u8>, Vec<u8>, String), String> {
        let kp = self.key_pair.as_ref().ok_or("No active keypair")?;
        let p = self.ml_dsa_primary.as_ref().ok_or("No ML-DSA primary identity")?;
        let ed = kp.signing_key.sign(payload).to_bytes().to_vec();
        let pq = crate::security::hybrid_crypto::ml_dsa_sign_deterministic(&p.sk, payload)
            .ok_or("ML-DSA signing failed")?;
        Ok((ed, pq, p.pk_hex.clone()))
    }

    pub fn sign(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let kp = self.key_pair.as_ref().ok_or("No active keypair")?;
        Ok(kp.signing_key.sign(data).to_bytes().to_vec())
    }

    /// Signature hybride : Ed25519 + ML-DSA-65 (FIPS 204).
    /// Renvoie `(classical_sig_bytes, quantum_sig_bytes, pq_pk_hex)`.
    /// Si aucune clé ML-DSA n'est dérivée, la couche quantum est vide et la
    /// vérification retombe en Ed25519 seul (rétro-compat).
    pub fn sign_hybrid(&self, data: &[u8]) -> Result<(Vec<u8>, Vec<u8>, String), String> {
        let kp = self.key_pair.as_ref().ok_or("No active keypair")?;
        let classical = kp.signing_key.sign(data).to_bytes().to_vec();
        match &self.ml_dsa {
            Some((sk, pk_hex)) => {
                let sig = sk
                    .try_sign_with_rng(&mut OsRng, data, &[])
                    .map_err(|_| "ML-DSA signing failed".to_string())?;
                Ok((classical, sig.to_vec(), pk_hex.clone()))
            }
            None => Ok((classical, Vec::new(), String::new())),
        }
    }

    /// **Deterministic** hybrid signature for the simulation / DST harness:
    /// Ed25519 (already deterministic) + ML-DSA-65 signed with a message-derived
    /// RNG instead of `OsRng` ([`hybrid_crypto::ml_dsa_sign_deterministic`]), so
    /// seeded runs are byte-reproducible. Production keeps [`Self::sign_hybrid`]
    /// (hedged); this is never on the production tx path.
    ///
    /// SIGN-DET-VERIFY: gated `#[cfg(test)]` so the weaker deterministic path is
    /// **physically absent from release builds** (it calls the likewise-gated
    /// [`hybrid_crypto::ml_dsa_sign_deterministic`]). A non-test build cannot
    /// downgrade signing to deterministic even if a caller set `det_sign=true`.
    #[cfg(test)]
    pub fn sign_hybrid_det(&self, data: &[u8]) -> Result<(Vec<u8>, Vec<u8>, String), String> {
        let kp = self.key_pair.as_ref().ok_or("No active keypair")?;
        let classical = kp.signing_key.sign(data).to_bytes().to_vec();
        match &self.ml_dsa {
            Some((sk, pk_hex)) => {
                let sig = crate::security::hybrid_crypto::ml_dsa_sign_deterministic(sk, data)
                    .ok_or("ML-DSA signing failed")?;
                Ok((classical, sig, pk_hex.clone()))
            }
            None => Ok((classical, Vec::new(), String::new())),
        }
    }

    pub fn verify(pk: &[u8], data: &[u8], sig: &[u8]) -> Result<bool, String> {
        let pk_arr: [u8; 32] = pk.try_into().map_err(|_| "Invalid pk len")?;
        let sig_arr: [u8; 64] = sig.try_into().map_err(|_| "Invalid sig len")?;
        let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|e| e.to_string())?;
        let signature = ed25519_dalek::Signature::from_bytes(&sig_arr);
        Ok(vk.verify(data, &signature).is_ok())
    }

    /// BLAKE3 hash — 10+ Go/s, parallelizable, streaming-verified
    pub fn blake3_hash(data: &[u8]) -> Vec<u8> {
        blake3::hash(data).as_bytes().to_vec()
    }

    /// BLAKE3 keyed MAC for authenticated data
    pub fn blake3_mac(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
        blake3::keyed_hash(key, data).as_bytes().to_vec()
    }

    /// XOR distance for Kademlia DHT routing
    pub fn xor_distance(a: &[u8], b: &[u8]) -> Vec<u8> {
        a.iter().zip(b.iter()).map(|(x, y)| x ^ y).collect()
    }

    /// Validate BLAKE3 integrity of incoming data
    pub fn verify_blake3(data: &[u8], expected_hash: &str) -> bool {
        hex::encode(blake3::hash(data).as_bytes()) == expected_hash
    }
}

impl Default for CryptoEngine { fn default() -> Self { Self::new() } }

/// Secure buffer that zeroizes on drop
#[allow(dead_code)]
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecureBuffer {
    pub data: Vec<u8>,
}

#[allow(dead_code)]
impl SecureBuffer {
    pub fn new(data: Vec<u8>) -> Self { Self { data } }
}

#[cfg(test)]
mod zeroize_guards {
    //! C6: compile-time guarantees that the secrets held by [`CryptoEngine`]
    //! (and therefore by `sm::Node`) are wiped from memory on drop.
    //! `CryptoEngine` owns its secrets in `key_pair.signing_key` (Ed25519)
    //! and `ml_dsa.0` (ML-DSA-65); both types below implement
    //! `ZeroizeOnDrop`, so the engine's drop glue erases them. These
    //! assertions fail to COMPILE if a dependency's zeroize support is ever
    //! dropped (e.g. removing the ed25519-dalek `zeroize` feature), so the
    //! §3 "zeroize every secret" invariant cannot silently regress.
    use zeroize::ZeroizeOnDrop;

    fn assert_zeroize_on_drop<T: ZeroizeOnDrop>() {}

    #[test]
    fn held_signing_secrets_zeroize_on_drop() {
        // Ed25519 secret scalar — requires the ed25519-dalek `zeroize` feature.
        assert_zeroize_on_drop::<ed25519_dalek::SigningKey>();
        // ML-DSA-65 (FIPS 204) private key — zeroizes by default.
        assert_zeroize_on_drop::<fips204::ml_dsa_65::PrivateKey>();
    }
}

#[cfg(test)]
mod zeroize_sweep_guards {
    //! ZEROIZE-SWEEP (HARDEN-HYGIENE-1): pin the secret-export hygiene the sweep
    //! added — `get_secret_bytes` returns a **self-wiping** `Zeroizing` (the type
    //! annotation below fails to COMPILE if it regresses to a bare `Vec`), and
    //! the vault create→unlock round-trip still reconstructs the keypair after
    //! removing the un-wiped `sk_bytes.clone()` (correctness guard for the wipe
    //! edits — a broken wipe must never silently corrupt unlock).
    use super::pq_vault::PQVault;
    use super::CryptoEngine;
    use zeroize::Zeroizing;

    #[test]
    fn get_secret_bytes_is_self_wiping() {
        let mut e = CryptoEngine::new();
        e.generate_keypair();
        // The annotation is the teeth: a regression to `Vec<u8>` won't compile.
        let s: Zeroizing<Vec<u8>> = e.get_secret_bytes().expect("secret");
        assert_eq!(s.len(), 32, "Ed25519 secret is 32 bytes");
    }

    #[test]
    fn vault_create_unlock_roundtrip_after_clone_removal() {
        let pw = "correct horse battery staple";
        let mut e = CryptoEngine::new();
        let (identity, pk_bytes, ct, nonce) =
            PQVault::create_identity(&mut e, "alice", pw).expect("create");

        // unlock now builds sk_arr from the slice (no un-wiped clone) and wipes
        // both copies — it must still reconstruct the SAME keypair.
        let mut e2 = CryptoEngine::new();
        let unlocked = PQVault::unlock_identity(
            &mut e2,
            &pk_bytes,
            &ct,
            &nonce,
            pw,
            "alice",
            &identity.created_at,
        )
        .expect("unlock");
        assert_eq!(
            unlocked.public_key_hex, identity.public_key_hex,
            "unlock must reconstruct the same keypair after the zeroize-sweep edits"
        );

        // Wrong password must still fail opaquely — auth not weakened.
        let mut e3 = CryptoEngine::new();
        assert!(
            PQVault::unlock_identity(&mut e3, &pk_bytes, &ct, &nonce, "wrong", "alice", &identity.created_at)
                .is_err(),
            "wrong password must fail"
        );
    }
}

#[cfg(test)]
mod sign_mode_guards {
    //! SIGN-DET-VERIFY A.3: pin the two signing modes so the `det_sign` switch
    //! provably does what it claims AND production sits on the strong side. The
    //! deterministic path (`sign_hybrid_det`) is `#[cfg(test)]` — physically
    //! absent from release — so these tests are the live proof that the
    //! production path stays hedged while the harness path stays reproducible.
    use super::CryptoEngine;

    const MSG: &[u8] = b"SIGN-DET-VERIFY canonical message";

    #[test]
    fn prod_signing_is_hedged() {
        // Production signing (`sign_hybrid`, ML-DSA over `OsRng`) MUST be
        // non-deterministic: signing the SAME message twice yields DIFFERENT
        // ML-DSA signatures. If they ever matched, prod would have silently
        // become deterministic — a post-quantum hardness downgrade (the exact
        // failure SIGN-DET-VERIFY exists to catch).
        let mut engine = CryptoEngine::new();
        engine.generate_keypair();
        let (c1, q1, _) = engine.sign_hybrid(MSG).expect("sign_hybrid");
        let (c2, q2, _) = engine.sign_hybrid(MSG).expect("sign_hybrid");
        // Anti-vacuity: the ML-DSA layer must actually be present, else "differ"
        // would compare two empty vecs.
        assert!(!q1.is_empty(), "ML-DSA layer must be present for the check to bite");
        assert_ne!(q1, q2, "production ML-DSA signing must be hedged (non-deterministic)");
        // Ed25519 is deterministic by construction — documents that the hedging
        // lives purely in the ML-DSA layer, not Ed25519.
        assert_eq!(c1, c2, "Ed25519 layer is deterministic by construction");
    }

    #[test]
    fn sim_signing_is_deterministic() {
        // Harness signing (`sign_hybrid_det`, ML-DSA over a BLAKE3-derived RNG)
        // MUST be byte-reproducible across BOTH layers: the DST determinism
        // proof (C1) and seed-replay depend on it. Signing the SAME message
        // twice yields BYTE-IDENTICAL signatures.
        let mut engine = CryptoEngine::new();
        engine.generate_keypair();
        let (c1, q1, p1) = engine.sign_hybrid_det(MSG).expect("sign_hybrid_det");
        let (c2, q2, p2) = engine.sign_hybrid_det(MSG).expect("sign_hybrid_det");
        // Anti-vacuity: a present ML-DSA layer is what makes determinism
        // non-trivial (two empty vecs would compare equal for free).
        assert!(!q1.is_empty(), "ML-DSA layer must be present for the check to bite");
        assert_eq!(
            (c1, q1, p1),
            (c2, q2, p2),
            "simulation hybrid signing must be byte-identical (C1 depends on it)"
        );
    }
}

#[cfg(test)]
mod pq_mig1_primary_identity {
    //! PQ-MIG-1 : l'identité **primaire** post-quantique (ML-DSA-65) — racine
    //! ratifiée par ADR-007 (b). Ces tests prouvent la porte d'acceptation :
    //! round-trip vault (même clé publique), signage/vérification ML-DSA de bout
    //! en bout via le moteur, déterminisme du chemin sim vs hedging de
    //! production, indépendance vis-à-vis d'Ed25519 (la faille CRYPTO-ID-1), et
    //! auto-effacement de la graine racine.
    use super::pq_vault::PQVault;
    use super::CryptoEngine;
    use zeroize::Zeroizing;

    const MSG: &[u8] = b"PQ-MIG-1 end-to-end ML-DSA vector";

    #[test]
    fn engine_sign_verify_roundtrip_end_to_end() {
        // Porte d'acceptation : signage/vérification ML-DSA de bout en bout via
        // le moteur, sur l'identité primaire.
        let mut e = CryptoEngine::new();
        let pk_hex = e.generate_pq_identity().expect("generate pq identity");
        assert!(!pk_hex.is_empty(), "ML-DSA public key present");
        assert_eq!(e.pq_identity_hex().as_deref(), Some(pk_hex.as_str()));

        let sig = e.sign_pq(MSG).expect("sign_pq");
        assert!(
            CryptoEngine::verify_pq(&pk_hex, MSG, &sig),
            "valid ML-DSA signature must verify"
        );
        // Message altéré ⇒ rejet.
        assert!(
            !CryptoEngine::verify_pq(&pk_hex, b"tampered", &sig),
            "tampered message must fail"
        );
        // Signature corrompue ⇒ rejet.
        let mut bad = sig.clone();
        bad[0] ^= 0xFF;
        assert!(
            !CryptoEngine::verify_pq(&pk_hex, MSG, &bad),
            "corrupted signature must fail"
        );
    }

    #[test]
    fn vault_store_load_roundtrip_same_public_key() {
        // Porte d'acceptation : round-trip stockage→chargement de l'identité
        // ML-DSA redonne EXACTEMENT la même clé publique.
        let pw = "correct horse battery staple";
        let mut e = CryptoEngine::new();
        let (pk_hex, ct, nonce) = PQVault::create_pq_identity(&mut e, pw).expect("create pq");

        let mut e2 = CryptoEngine::new();
        let reloaded =
            PQVault::unlock_pq_identity(&mut e2, &pk_hex, &ct, &nonce, pw).expect("unlock pq");
        assert_eq!(
            reloaded, pk_hex,
            "reload must reconstruct the identical ML-DSA public key"
        );
        assert_eq!(e2.pq_identity_hex().as_deref(), Some(pk_hex.as_str()));

        // La clé rechargée signe et vérifie ⇒ la clé SECRÈTE round-trip aussi
        // (pas seulement la publique).
        let sig = e2.sign_pq(MSG).expect("sign with reloaded key");
        assert!(
            CryptoEngine::verify_pq(&pk_hex, MSG, &sig),
            "reloaded secret key must produce valid signatures"
        );

        // Mauvais mot de passe ⇒ échec opaque (auth non affaiblie).
        let mut e3 = CryptoEngine::new();
        assert!(
            PQVault::unlock_pq_identity(&mut e3, &pk_hex, &ct, &nonce, "wrong").is_err(),
            "wrong password must fail"
        );
    }

    #[test]
    fn import_is_deterministic_same_seed_same_key() {
        // §1/§2 : le chemin de (ré)établissement est déterministe — même graine ⇒
        // même clé publique (fonde le round-trip vault et le chemin sim).
        let seed = [7u8; 32];
        let mut a = CryptoEngine::new();
        let mut b = CryptoEngine::new();
        let ka = a.import_pq_identity(&seed).expect("import a");
        let kb = b.import_pq_identity(&seed).expect("import b");
        assert_eq!(ka, kb, "same seed must yield the same ML-DSA public key");
    }

    #[test]
    fn sim_signing_is_byte_reproducible_but_prod_is_hedged() {
        // Vigilance n°1 : le chemin sim (`sign_pq_det`) est byte-reproductible
        // (C1 en dépendrait s'il était câblé), tandis que la production
        // (`sign_pq`, OsRng) est hedgée — la dureté PQ ne régresse jamais.
        let mut e = CryptoEngine::new();
        e.import_pq_identity(&[3u8; 32]).expect("import");

        let d1 = e.sign_pq_det(MSG).expect("det 1");
        let d2 = e.sign_pq_det(MSG).expect("det 2");
        assert_eq!(d1, d2, "deterministic sim signing must be byte-identical (C1)");

        let h1 = e.sign_pq(MSG).expect("hedged 1");
        let h2 = e.sign_pq(MSG).expect("hedged 2");
        assert_ne!(h1, h2, "production ML-DSA signing must be hedged (non-deterministic)");

        // Les deux chemins produisent des signatures valides pour la MÊME clé.
        let pk = e.pq_identity_hex().expect("pk");
        assert!(CryptoEngine::verify_pq(&pk, MSG, &d1), "det signature verifies");
        assert!(CryptoEngine::verify_pq(&pk, MSG, &h1), "hedged signature verifies");
    }

    #[test]
    fn pq_root_is_independent_of_ed25519() {
        // CRYPTO-ID-1 / ADR-007 (b) : la racine ML-DSA ne descend PAS d'Ed25519.
        // Deux moteurs partageant la MÊME graine Ed25519 produisent des identités
        // primaires ML-DSA DIFFÉRENTES (graine PQ indépendante, tirée d'OsRng).
        let ed_seed = [9u8; 32];
        let mut a = CryptoEngine::new();
        let mut b = CryptoEngine::new();
        let _ = a.import_keypair(&ed_seed).expect("ed a");
        let _ = b.import_keypair(&ed_seed).expect("ed b");
        let pa = a.generate_pq_identity().expect("pq a");
        let pb = b.generate_pq_identity().expect("pq b");
        assert_ne!(
            pa, pb,
            "independent PQ roots: same Ed25519 seed must NOT fix the ML-DSA primary key"
        );
        // Et la primaire n'est pas la couche héritée dérivée de la graine Ed25519.
        let legacy = a.get_identity().expect("identity").pq_public_key_hex;
        assert_ne!(
            Some(pa),
            legacy,
            "the PQ primary is independent of the legacy seed-derived ML-DSA layer"
        );
    }

    #[test]
    fn pq_seed_export_is_self_wiping() {
        // §2 zeroize : la graine exportée pour le vault est un `Zeroizing`
        // auto-effaçant (une régression vers `Vec<u8>` nu ne compilerait pas).
        let mut e = CryptoEngine::new();
        e.generate_pq_identity().expect("gen");
        let s: Zeroizing<Vec<u8>> = e.get_pq_seed_bytes().expect("seed");
        assert_eq!(s.len(), 32, "ML-DSA root seed is 32 bytes");
    }
}

#[cfg(test)]
mod pq_mig2_address {
    //! PQ-MIG-2 : adresses ML-DSA = `BLAKE3(ADDR_DOMAIN ‖ clé_publique)`. Ces tests
    //! prouvent la porte d'acceptation : dérivation **déterministe** (clé connue ⇒
    //! adresse connue), round-trip encode/décode, **dents de liaison** (la bonne
    //! clé passe `lie()`, une autre échoue), et **séparation de domaine** (les
    //! mêmes octets bruts dans un autre contexte ne donnent pas la même adresse).
    use super::{CryptoEngine, ADDR_DOMAIN};

    /// Échantillon fixe de « clé publique » (octets arbitraires mais stables) —
    /// isole la construction BLAKE3 de la dérivation, indépendamment de la
    /// longueur réelle (~1952 o) d'une vraie clé ML-DSA.
    const SAMPLE_PK: &[u8] = b"QUANTA PQ-MIG-2 fixed ML-DSA public key sample";

    /// Vecteur connu : `BLAKE3(ADDR_DOMAIN ‖ SAMPLE_PK)` en hex. Épinglé ⇒ toute
    /// dérive de la construction (domaine retiré, ordre changé, troncature) casse.
    const KNOWN_ADDR_HEX: &str =
        "64eb5334fa1c56ff2e71abf53ca35fba1e38556c260709b284c2e803d0d6e42e";

    #[test]
    fn address_is_deterministic_known_key_known_address() {
        // Porte : clé connue ⇒ adresse connue, identique sur deux dérivations.
        let a1 = CryptoEngine::ml_dsa_address_bytes(SAMPLE_PK);
        let a2 = CryptoEngine::ml_dsa_address_bytes(SAMPLE_PK);
        assert_eq!(a1, a2, "même clé ⇒ même adresse (pure, déterministe)");
        assert_eq!(a1.len(), 32, "adresse = 32 octets (sortie naturelle BLAKE3)");

        // Reconstruction indépendante de la formule spécifiée : prouve que la
        // fonction hashe bien `ADDR_DOMAIN ‖ pk` (pas seulement « un » hash).
        let mut h = blake3::Hasher::new();
        h.update(ADDR_DOMAIN);
        h.update(SAMPLE_PK);
        let reconstructed: [u8; 32] = *h.finalize().as_bytes();
        assert_eq!(a1, reconstructed, "adresse == BLAKE3(ADDR_DOMAIN ‖ pk)");

        // Vecteur épinglé : un humain peut vérifier que l'adresse est stable
        // dans le temps (et qu'aucune régression silencieuse ne l'a déplacée).
        assert_eq!(
            CryptoEngine::ml_dsa_address_hex(SAMPLE_PK),
            KNOWN_ADDR_HEX,
            "adresse connue figée pour le vecteur d'échantillon"
        );
    }

    #[test]
    fn encode_decode_roundtrip() {
        // §2 : encoder puis décoder redonne les mêmes 32 octets.
        let addr = CryptoEngine::ml_dsa_address_bytes(SAMPLE_PK);
        let enc = CryptoEngine::encode_address(&addr);
        let dec = CryptoEngine::decode_address(&enc).expect("decode valid hex");
        assert_eq!(addr, dec, "round-trip encode→decode préserve l'adresse");
        assert_eq!(enc, CryptoEngine::ml_dsa_address_hex(SAMPLE_PK));

        // Entrées malformées ⇒ erreur opaque, jamais de panique.
        assert!(CryptoEngine::decode_address("zz").is_err(), "hex invalide rejeté");
        assert!(
            CryptoEngine::decode_address(&"ab".repeat(31)).is_err(),
            "mauvaise longueur (31 o) rejetée"
        );
        assert!(
            CryptoEngine::decode_address(&"ab".repeat(33)).is_err(),
            "mauvaise longueur (33 o) rejetée"
        );
    }

    #[test]
    fn binding_has_teeth() {
        // §3 : la bonne clé passe `lie()`, une AUTRE clé échoue (pas de masquage).
        let addr = CryptoEngine::ml_dsa_address_bytes(SAMPLE_PK);
        assert!(
            CryptoEngine::address_binds_key(&addr, SAMPLE_PK),
            "la clé qui dérive l'adresse doit lier"
        );

        let other_pk: &[u8] = b"QUANTA PQ-MIG-2 a DIFFERENT ML-DSA public key!!";
        assert!(
            !CryptoEngine::address_binds_key(&addr, other_pk),
            "une autre clé NE doit PAS lier (la liaison mord)"
        );

        // Une adresse altérée d'un seul bit ne lie plus la bonne clé.
        let mut tampered = addr;
        tampered[0] ^= 0x01;
        assert!(
            !CryptoEngine::address_binds_key(&tampered, SAMPLE_PK),
            "adresse altérée ⇒ la liaison échoue"
        );

        // Variante hex (forme que verify_tx utilisera en PQ-MIG-3).
        let addr_hex = CryptoEngine::encode_address(&addr);
        let pk_hex = hex::encode(SAMPLE_PK);
        let other_hex = hex::encode(other_pk);
        assert!(
            CryptoEngine::address_hex_binds_key_hex(&addr_hex, &pk_hex),
            "liaison hex : bonne clé ⇒ vrai"
        );
        assert!(
            !CryptoEngine::address_hex_binds_key_hex(&addr_hex, &other_hex),
            "liaison hex : autre clé ⇒ faux"
        );
        // Entrées hex cassées ⇒ faux (jamais de panique, pas de contournement).
        assert!(
            !CryptoEngine::address_hex_binds_key_hex("zz", &pk_hex),
            "adresse hex invalide ⇒ faux"
        );
        assert!(
            !CryptoEngine::address_hex_binds_key_hex(&addr_hex, "zz"),
            "clé hex invalide ⇒ faux"
        );
    }

    #[test]
    fn domain_separation_holds() {
        // §1 obligatoire : les mêmes octets bruts dans un AUTRE contexte ne
        // donnent pas la même adresse — le tag de domaine participe réellement.
        let addr = CryptoEngine::ml_dsa_address_bytes(SAMPLE_PK);

        // (a) hash nu, sans domaine.
        let bare: [u8; 32] = *blake3::hash(SAMPLE_PK).as_bytes();
        assert_ne!(addr, bare, "adresse ≠ BLAKE3(pk) sans domaine");

        // (b) même octets, domaine différent ⇒ adresse différente.
        let mut h = blake3::Hasher::new();
        h.update(b"QUANTA-OTHER-CONTEXT-V1");
        h.update(SAMPLE_PK);
        let other_ctx: [u8; 32] = *h.finalize().as_bytes();
        assert_ne!(addr, other_ctx, "adresse ≠ hash sous un autre domaine");
    }

    #[test]
    fn engine_exposes_its_address() {
        // §4 : le moteur expose l'adresse de son identité primaire ML-DSA,
        // cohérente avec la dérivation pure depuis la clé publique exposée.
        let mut e = CryptoEngine::new();
        assert!(e.pq_address().is_none(), "aucune adresse sans identité primaire");

        let pk_hex = e.generate_pq_identity().expect("generate pq identity");
        let pk_bytes = hex::decode(&pk_hex).expect("pk hex");
        let expected = CryptoEngine::ml_dsa_address_bytes(&pk_bytes);

        assert_eq!(
            e.pq_address(),
            Some(expected),
            "l'adresse exposée par le moteur dérive de sa clé publique primaire"
        );
        assert_eq!(
            e.pq_address_hex().as_deref(),
            Some(CryptoEngine::encode_address(&expected).as_str()),
            "l'adresse hex exposée correspond à l'adresse brute"
        );

        // La liaison reconnaît l'adresse du moteur contre sa propre clé publique.
        let engine_addr = e.pq_address().expect("engine address present");
        assert!(
            CryptoEngine::address_binds_key(&engine_addr, &pk_bytes),
            "l'adresse du moteur lie sa clé publique primaire"
        );
    }
}
