//! P2P Page Store — associates HTML content with wallet public keys.
//!
//! Two coexisting publishing modes :
//!
//! 1. **Single-page** (V2 — legacy) — un wallet publie *une* page (max 64 KB)
//!    via [`PublishedPage`]. Conservé pour rétro-compat des callers existants
//!    et des snapshots déjà persistés.
//!
//! 2. **Multi-page site** (V3.3) — un wallet publie un [`SiteManifest`]
//!    contenant plusieurs pages HTML + assets (CSS, images), accessible via
//!    `torus://name.torus/path`. Le manifest est signé une seule fois et
//!    fournit un Merkle-summary content-addressed (BLAKE3) pour la propagation
//!    incrémentale et la vérification d'intégrité côté pair.
//!
//! Limites V3.3 (anti-abus) :
//!   * 100 pages max / site, 64 KB / page
//!   * 50 assets max / site, 256 KB / asset inline
//!   * Pour les gros assets (vidéos, archives), chunkez via [`merkle_dag`] et
//!     mettez le CID racine dans `SiteAsset::dag_cid` (le contenu n'est plus
//!     dans le manifest mais dans le DAG content-addressed).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Limites ────────────────────────────────────────────────────────────────

/// Maximum page content size in bytes (64 KB) — single-page mode.
pub const MAX_PAGE_SIZE: usize = 64_000;
/// Maximum HTML par page d'un manifest multi-page (64 KB).
pub const MAX_MANIFEST_PAGE_SIZE: usize = 64_000;
/// Maximum asset inline (256 KB ; au-delà, utiliser DAG chunking via `dag_cid`).
pub const MAX_INLINE_ASSET_SIZE: usize = 256_000;
/// Maximum nombre de pages dans un manifest.
pub const MAX_PAGES_PER_SITE: usize = 100;
/// Maximum nombre d'assets dans un manifest.
pub const MAX_ASSETS_PER_SITE: usize = 50;
/// Maximum total bytes d'un manifest sérialisé (8 MB).
pub const MAX_MANIFEST_BYTES: usize = 8 * 1024 * 1024;

// ─── Single-page (legacy V2) ────────────────────────────────────────────────

/// A published page associated with a wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedPage {
    pub author_pk: String,
    pub content: String,
    pub title: String,
    pub updated_at: u64,
    pub signature: String,
    pub version: u64,
}

// ─── Multi-page site (V3.3) ─────────────────────────────────────────────────

/// Une page d'un site (HTML inline, ≤ 64 KB).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SitePage {
    /// Chemin URL relatif au domaine, ex: `/`, `/about`, `/blog/post1`.
    /// Doit commencer par `/`. Pas de `..`. ASCII printable + `/`.
    pub path: String,
    pub title: String,
    pub html: String,
}

/// Un asset (image, CSS, JS opt-in, font, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteAsset {
    /// Path relatif `/style.css`, `/img/logo.png`.
    pub path: String,
    /// MIME type ex: `text/css`, `image/png`, `application/javascript`.
    pub mime: String,
    /// Bytes encodés en base64 si l'asset est inline.
    /// Ignoré si `dag_cid` est `Some` (l'asset est alors récupéré via DAG).
    #[serde(default)]
    pub content_b64: String,
    /// Pour les gros assets (> MAX_INLINE_ASSET_SIZE), CID racine dans le
    /// Merkle-DAG. Le pair récupère le contenu en suivant les chunks.
    #[serde(default)]
    pub dag_cid: Option<String>,
    pub size: u64,
}

/// Manifest d'un site multi-page signé par son auteur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteManifest {
    pub author_pk: String,
    /// Chemin par défaut (landing), souvent `/`.
    pub root_path: String,
    pub pages: Vec<SitePage>,
    pub assets: Vec<SiteAsset>,
    pub updated_at: u64,
    pub version: u64,
    /// Signature Ed25519 du `signable_manifest_bytes(self)`.
    pub signature: String,
}

/// Erreurs publiques du store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageStoreError {
    PageTooBig { size: usize, max: usize },
    AssetTooBig { size: usize, max: usize },
    TitleTooLong,
    TooManyPages,
    TooManyAssets,
    ManifestTooBig,
    InvalidPath(String),
    InvalidSignature,
    StaleVersion,
    DuplicatePath(String),
    InvalidManifest(String),
}

impl std::fmt::Display for PageStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Bytes canoniques signés du manifest (tout sauf `signature`).
/// On utilise BLAKE3 sur la concat `version|author|root|pages|assets` pour
/// déterminer un Merkle-summary stable indépendant du JSON ordering.
pub fn signable_manifest_bytes(m: &SiteManifest) -> Vec<u8> {
    let mut h = blake3::Hasher::new();
    h.update(b"SITE|");
    h.update(m.author_pk.as_bytes());
    h.update(b"|");
    h.update(m.root_path.as_bytes());
    h.update(b"|");
    h.update(&m.version.to_be_bytes());
    h.update(b"|");
    h.update(&m.updated_at.to_be_bytes());
    // Pages — ordre déclaré (l'auteur est libre d'ordonner).
    h.update(b"|PAGES|");
    for p in &m.pages {
        h.update(p.path.as_bytes());
        h.update(b"\x1f");
        h.update(p.title.as_bytes());
        h.update(b"\x1f");
        h.update(p.html.as_bytes());
        h.update(b"\x1e");
    }
    h.update(b"|ASSETS|");
    for a in &m.assets {
        h.update(a.path.as_bytes());
        h.update(b"\x1f");
        h.update(a.mime.as_bytes());
        h.update(b"\x1f");
        h.update(a.content_b64.as_bytes());
        h.update(b"\x1f");
        if let Some(cid) = &a.dag_cid {
            h.update(cid.as_bytes());
        }
        h.update(b"\x1e");
    }
    h.finalize().as_bytes().to_vec()
}

/// Helper de signature (utilitaire pour tests externes — la prod passe par
/// `CryptoEngine::sign(&signable_manifest_bytes(m))` puis assigne `m.signature`).
#[allow(dead_code)]
pub fn sign_manifest(sk: &SigningKey, m: &mut SiteManifest) {
    let sig = sk.sign(&signable_manifest_bytes(m));
    m.signature = hex::encode(sig.to_bytes());
}

fn verify_manifest_sig(m: &SiteManifest) -> Result<(), PageStoreError> {
    let pk_bytes = hex::decode(&m.author_pk).map_err(|_| PageStoreError::InvalidSignature)?;
    let sig_bytes = hex::decode(&m.signature).map_err(|_| PageStoreError::InvalidSignature)?;
    let pk_arr: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| PageStoreError::InvalidSignature)?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| PageStoreError::InvalidSignature)?;
    let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|_| PageStoreError::InvalidSignature)?;
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(&signable_manifest_bytes(m), &sig)
        .map_err(|_| PageStoreError::InvalidSignature)
}

/// Validation forme d'un path : commence par `/`, pas de `..`, pas de control char,
/// longueur ≤ 256.
fn validate_path(path: &str) -> Result<(), PageStoreError> {
    if path.is_empty() || !path.starts_with('/') || path.len() > 256 {
        return Err(PageStoreError::InvalidPath(path.into()));
    }
    if path.contains("..") || path.contains("//") {
        return Err(PageStoreError::InvalidPath(path.into()));
    }
    if !path.chars().all(|c| c == '/' || c == '-' || c == '_' || c == '.' || c.is_ascii_alphanumeric()) {
        return Err(PageStoreError::InvalidPath(path.into()));
    }
    Ok(())
}

fn validate_manifest(m: &SiteManifest) -> Result<(), PageStoreError> {
    if m.pages.len() > MAX_PAGES_PER_SITE {
        return Err(PageStoreError::TooManyPages);
    }
    if m.assets.len() > MAX_ASSETS_PER_SITE {
        return Err(PageStoreError::TooManyAssets);
    }
    let mut paths = std::collections::HashSet::new();
    for p in &m.pages {
        validate_path(&p.path)?;
        if p.title.len() > 200 {
            return Err(PageStoreError::TitleTooLong);
        }
        if p.html.len() > MAX_MANIFEST_PAGE_SIZE {
            return Err(PageStoreError::PageTooBig {
                size: p.html.len(),
                max: MAX_MANIFEST_PAGE_SIZE,
            });
        }
        if !paths.insert(("p", p.path.clone())) {
            return Err(PageStoreError::DuplicatePath(p.path.clone()));
        }
    }
    for a in &m.assets {
        validate_path(&a.path)?;
        if a.size as usize > MAX_INLINE_ASSET_SIZE && a.dag_cid.is_none() {
            return Err(PageStoreError::AssetTooBig {
                size: a.size as usize,
                max: MAX_INLINE_ASSET_SIZE,
            });
        }
        if !paths.insert(("a", a.path.clone())) {
            return Err(PageStoreError::DuplicatePath(a.path.clone()));
        }
    }
    if !m.root_path.is_empty() {
        validate_path(&m.root_path)?;
    }
    // Sérialisation totale ≤ 8 MB
    let total = serde_json::to_vec(m)
        .map_err(|e| PageStoreError::InvalidManifest(e.to_string()))?
        .len();
    if total > MAX_MANIFEST_BYTES {
        return Err(PageStoreError::ManifestTooBig);
    }
    Ok(())
}

// ─── Store ──────────────────────────────────────────────────────────────────

/// In-memory store for all published pages.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageStore {
    /// Legacy single-page (1 par wallet).
    pages: HashMap<String, PublishedPage>,
    /// V3.3 — Site multi-page (1 manifest par wallet, peut coexister avec une
    /// PublishedPage legacy).
    #[serde(default)]
    sites: HashMap<String, SiteManifest>,
}

/// Snapshot for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageStoreSnapshot {
    pub pages: Vec<PublishedPage>,
    #[serde(default)]
    pub sites: Vec<SiteManifest>,
}

impl PageStore {
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
            sites: HashMap::new(),
        }
    }

    /// Publish or update a wallet's single page (legacy V2).
    pub fn publish(&mut self, page: PublishedPage) -> Result<(), String> {
        if page.content.len() > MAX_PAGE_SIZE {
            return Err(format!(
                "Page trop grande: {} > {MAX_PAGE_SIZE}",
                page.content.len()
            ));
        }
        if page.title.len() > 100 {
            return Err("Titre trop long (max 100)".into());
        }
        // Verify signature (skip for unsigned migration pages)
        if page.signature != "unsigned" && !page.signature.is_empty() {
            verify_page_signature(&page)?;
        }
        if let Some(existing) = self.pages.get(&page.author_pk) {
            if page.version <= existing.version {
                return Err("Version obsolète".into());
            }
        }
        self.pages.insert(page.author_pk.clone(), page);
        Ok(())
    }

    /// V3.3 — Publie / met à jour le manifest multi-page d'un wallet.
    /// Rejette si signature invalide, version stagnante, ou contraintes violées.
    pub fn publish_site(&mut self, manifest: SiteManifest) -> Result<(), PageStoreError> {
        validate_manifest(&manifest)?;
        verify_manifest_sig(&manifest)?;
        if let Some(existing) = self.sites.get(&manifest.author_pk) {
            if manifest.version <= existing.version {
                return Err(PageStoreError::StaleVersion);
            }
        }
        self.sites.insert(manifest.author_pk.clone(), manifest);
        Ok(())
    }

    /// Get a page by author public key (legacy mode).
    pub fn get_page(&self, pk: &str) -> Option<&PublishedPage> {
        self.pages.get(pk)
    }

    /// V3.3 — Récupère le manifest complet d'un site.
    pub fn get_site(&self, pk: &str) -> Option<&SiteManifest> {
        self.sites.get(pk)
    }

    /// V3.3 — Récupère une page précise dans le site d'un auteur.
    /// Si `path` est `/` ou vide, renvoie la page `root_path`.
    pub fn get_site_page(&self, pk: &str, path: &str) -> Option<&SitePage> {
        let m = self.sites.get(pk)?;
        let target = if path.is_empty() || path == "/" {
            if m.root_path.is_empty() { "/" } else { m.root_path.as_str() }
        } else {
            path
        };
        m.pages.iter().find(|p| p.path == target)
    }

    /// V3.3 — Récupère un asset par path.
    pub fn get_site_asset(&self, pk: &str, path: &str) -> Option<&SiteAsset> {
        let m = self.sites.get(pk)?;
        m.assets.iter().find(|a| a.path == path)
    }

    /// List all published pages (legacy).
    pub fn list_pages(&self) -> Vec<&PublishedPage> {
        self.pages.values().collect()
    }

    /// V3.3 — Liste tous les sites multi-page.
    pub fn list_sites(&self) -> Vec<&SiteManifest> {
        self.sites.values().collect()
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn site_count(&self) -> usize {
        self.sites.len()
    }

    pub fn snapshot(&self) -> PageStoreSnapshot {
        PageStoreSnapshot {
            pages: self.pages.values().cloned().collect(),
            sites: self.sites.values().cloned().collect(),
        }
    }

    pub fn restore(snap: PageStoreSnapshot) -> Self {
        let mut pages = HashMap::new();
        for p in snap.pages {
            pages.insert(p.author_pk.clone(), p);
        }
        let mut sites = HashMap::new();
        for s in snap.sites {
            sites.insert(s.author_pk.clone(), s);
        }
        Self { pages, sites }
    }
}

/// Verify the Ed25519 signature of a published page.
pub fn verify_page_signature(page: &PublishedPage) -> Result<(), String> {
    let signable = format!("{}:{}:{}", page.author_pk, page.version, page.content);
    let pk_bytes = hex::decode(&page.author_pk).map_err(|e| format!("Invalid pk hex: {}", e))?;
    let sig_bytes = hex::decode(&page.signature).map_err(|e| format!("Invalid sig hex: {}", e))?;

    let pk_arr: [u8; 32] = pk_bytes.try_into()
        .map_err(|_| "Public key must be 32 bytes".to_string())?;
    let sig_arr: [u8; 64] = sig_bytes.try_into()
        .map_err(|_| "Signature must be 64 bytes".to_string())?;
    let vk = VerifyingKey::from_bytes(&pk_arr)
        .map_err(|e| format!("Invalid public key: {}", e))?;
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(signable.as_bytes(), &sig)
        .map_err(|e| format!("Signature invalide: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sk(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn pk_of(s: &SigningKey) -> String {
        hex::encode(s.verifying_key().as_bytes())
    }

    #[test]
    fn publish_and_retrieve() {
        let mut store = PageStore::new();
        let page = PublishedPage {
            author_pk: "a".repeat(64),
            content: "<h1>Hello</h1>".into(),
            title: "Ma page".into(),
            updated_at: 1000,
            signature: "unsigned".into(),
            version: 1,
        };
        assert!(store.publish(page).is_ok());
        assert_eq!(store.get_page(&"a".repeat(64)).unwrap().title, "Ma page");
    }

    #[test]
    fn rejects_oversized_page() {
        let mut store = PageStore::new();
        let big = "x".repeat(MAX_PAGE_SIZE + 1);
        let page = PublishedPage {
            author_pk: "b".repeat(64),
            content: big,
            title: "T".into(),
            updated_at: 1,
            signature: "unsigned".into(),
            version: 1,
        };
        assert!(store.publish(page).is_err());
    }

    #[test]
    fn rejects_old_version() {
        let mut store = PageStore::new();
        let mk = |v| PublishedPage {
            author_pk: "c".repeat(64),
            content: "c".into(),
            title: "T".into(),
            updated_at: v,
            signature: "unsigned".into(),
            version: v,
        };
        store.publish(mk(2)).unwrap();
        assert!(store.publish(mk(1)).is_err());
        assert!(store.publish(mk(3)).is_ok());
    }

    #[test]
    fn snapshot_restore() {
        let mut store = PageStore::new();
        let page = PublishedPage {
            author_pk: "d".repeat(64),
            content: "x".into(),
            title: "T".into(),
            updated_at: 1,
            signature: "unsigned".into(),
            version: 1,
        };
        store.publish(page).unwrap();
        let snap = store.snapshot();
        let restored = PageStore::restore(snap);
        assert_eq!(restored.page_count(), 1);
    }

    #[test]
    fn page_with_valid_ed25519_signature() {
        let sk_ = sk(42);
        let pk_hex = pk_of(&sk_);
        let content = "<h1>Signed</h1>";
        let version = 1u64;
        let signable = format!("{}:{}:{}", pk_hex, version, content);
        let sig = sk_.sign(signable.as_bytes());
        let sig_hex = hex::encode(sig.to_bytes());

        let page = PublishedPage {
            author_pk: pk_hex,
            content: content.into(),
            title: "Signed page".into(),
            updated_at: 100,
            signature: sig_hex,
            version,
        };
        let mut store = PageStore::new();
        assert!(store.publish(page).is_ok());
    }

    #[test]
    fn page_with_forged_signature_rejected() {
        let page = PublishedPage {
            author_pk: "a".repeat(64),
            content: "forged".into(),
            title: "Bad".into(),
            updated_at: 1,
            signature: "b".repeat(128), // fake 64-byte sig
            version: 1,
        };
        let mut store = PageStore::new();
        assert!(store.publish(page).is_err());
    }

    // ── V3.3 — Multi-page site tests ────────────────────────────────────────

    fn fresh_manifest(seed: u8) -> (SigningKey, SiteManifest) {
        let s = sk(seed);
        let pk = pk_of(&s);
        let mut m = SiteManifest {
            author_pk: pk,
            root_path: "/".into(),
            pages: vec![
                SitePage {
                    path: "/".into(),
                    title: "Accueil".into(),
                    html: "<h1>Bienvenue</h1>".into(),
                },
                SitePage {
                    path: "/about".into(),
                    title: "À propos".into(),
                    html: "<p>Site Torus</p>".into(),
                },
            ],
            assets: vec![SiteAsset {
                path: "/style.css".into(),
                mime: "text/css".into(),
                content_b64: "Ym9keSB7IGNvbG9yOiByZWQ7IH0=".into(),
                dag_cid: None,
                size: 22,
            }],
            updated_at: 1_000,
            version: 1,
            signature: String::new(),
        };
        sign_manifest(&s, &mut m);
        (s, m)
    }

    #[test]
    fn site_publish_and_retrieve_pages() {
        let (_, m) = fresh_manifest(1);
        let pk = m.author_pk.clone();
        let mut store = PageStore::new();
        store.publish_site(m).unwrap();
        assert_eq!(store.site_count(), 1);
        assert_eq!(store.get_site_page(&pk, "/").unwrap().title, "Accueil");
        assert_eq!(store.get_site_page(&pk, "/about").unwrap().title, "À propos");
        assert!(store.get_site_page(&pk, "/missing").is_none());
        assert_eq!(store.get_site_asset(&pk, "/style.css").unwrap().mime, "text/css");
    }

    #[test]
    fn site_rejects_forged_signature() {
        let (_, mut m) = fresh_manifest(2);
        m.pages[0].html = "<h1>tampered</h1>".into(); // tamper après sig
        let mut store = PageStore::new();
        assert_eq!(store.publish_site(m), Err(PageStoreError::InvalidSignature));
    }

    #[test]
    fn site_rejects_oversized_page() {
        let s = sk(3);
        let big = "x".repeat(MAX_MANIFEST_PAGE_SIZE + 1);
        let mut m = SiteManifest {
            author_pk: pk_of(&s),
            root_path: "/".into(),
            pages: vec![SitePage {
                path: "/".into(),
                title: "T".into(),
                html: big,
            }],
            assets: vec![],
            updated_at: 1,
            version: 1,
            signature: String::new(),
        };
        sign_manifest(&s, &mut m);
        let mut store = PageStore::new();
        assert!(matches!(
            store.publish_site(m),
            Err(PageStoreError::PageTooBig { .. })
        ));
    }

    #[test]
    fn site_rejects_invalid_path() {
        let s = sk(4);
        let mut m = SiteManifest {
            author_pk: pk_of(&s),
            root_path: "/".into(),
            pages: vec![SitePage {
                path: "/../etc/passwd".into(),
                title: "T".into(),
                html: "x".into(),
            }],
            assets: vec![],
            updated_at: 1,
            version: 1,
            signature: String::new(),
        };
        sign_manifest(&s, &mut m);
        let mut store = PageStore::new();
        assert!(matches!(
            store.publish_site(m),
            Err(PageStoreError::InvalidPath(_))
        ));
    }

    #[test]
    fn site_rejects_duplicate_path() {
        let s = sk(5);
        let mut m = SiteManifest {
            author_pk: pk_of(&s),
            root_path: "/".into(),
            pages: vec![
                SitePage { path: "/a".into(), title: "1".into(), html: "x".into() },
                SitePage { path: "/a".into(), title: "2".into(), html: "y".into() },
            ],
            assets: vec![],
            updated_at: 1,
            version: 1,
            signature: String::new(),
        };
        sign_manifest(&s, &mut m);
        let mut store = PageStore::new();
        assert!(matches!(
            store.publish_site(m),
            Err(PageStoreError::DuplicatePath(_))
        ));
    }

    #[test]
    fn site_version_must_increase() {
        let (s, m1) = fresh_manifest(6);
        let mut store = PageStore::new();
        store.publish_site(m1).unwrap();
        let mut m2 = SiteManifest {
            author_pk: pk_of(&s),
            root_path: "/".into(),
            pages: vec![],
            assets: vec![],
            updated_at: 2,
            version: 1, // même version
            signature: String::new(),
        };
        sign_manifest(&s, &mut m2);
        assert_eq!(store.publish_site(m2), Err(PageStoreError::StaleVersion));
    }

    #[test]
    fn site_snapshot_round_trip() {
        let (_, m) = fresh_manifest(7);
        let pk = m.author_pk.clone();
        let mut store = PageStore::new();
        store.publish_site(m).unwrap();
        let snap = store.snapshot();
        let restored = PageStore::restore(snap);
        assert_eq!(restored.site_count(), 1);
        assert!(restored.get_site_page(&pk, "/").is_some());
    }

    #[test]
    fn root_path_resolution_with_alternate_root() {
        let s = sk(8);
        let mut m = SiteManifest {
            author_pk: pk_of(&s),
            root_path: "/home".into(),
            pages: vec![SitePage {
                path: "/home".into(),
                title: "Home".into(),
                html: "x".into(),
            }],
            assets: vec![],
            updated_at: 1,
            version: 1,
            signature: String::new(),
        };
        sign_manifest(&s, &mut m);
        let pk = m.author_pk.clone();
        let mut store = PageStore::new();
        store.publish_site(m).unwrap();
        assert!(store.get_site_page(&pk, "/").is_some()); // résout vers /home
        assert!(store.get_site_page(&pk, "/home").is_some());
    }
}
