//! JSON-RPC 2.0 over HTTP — the integration surface for wallets, block explorers
//! and **exchange onboarding** (deposit monitoring + address validation).
//!
//! # Why a hand-rolled server
//!
//! The endpoint is a single `POST /` that takes a JSON-RPC request and returns a
//! JSON-RPC response — the Bitcoin-Core shape every integrator already knows. That
//! is small enough (~150 lines) to implement directly on `tokio` with **no new HTTP
//! dependency**, keeping the dependency tree tight and the surface fully auditable.
//! Bound to `127.0.0.1` by default: an integrator runs the node co-located (or
//! tunnels), never exposing money RPC to the open internet.
//!
//! # Semantics
//!
//! - All amounts are **integer µQTA** (base units) — never floats, so an integrator
//!   never inherits rounding drift (`1 QUANTA = 1_000_000 µQTA`).
//! - Addresses accept **either** the public `qta1…` (Bech32m) form or the canonical
//!   64-hex form (see [`crate::security::address`]); results echo both.
//! - **Finality is deterministic**: `finalized_height` / a tx's `finalized` flag
//!   answer "is this irreversible yet?" precisely — an exchange credits a deposit
//!   once its block index `≤ finalized_height`, no confirmation-count guessing.
//!
//! This is a **read + monitor** surface (increment 1). Broadcasting pre-signed
//! withdrawals (`sendrawtransaction`) and key-holding wallet RPC land next.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

use crate::security::address;
use crate::AppState;

/// Max bytes we read for the request head (request line + headers).
const MAX_HEADER: usize = 16 * 1024;
/// Max JSON-RPC body we accept.
const MAX_BODY: usize = 2 * 1024 * 1024;

/// A4 (SEND-OPTIN-1) — plafond par défaut d'un `sendtoaddress`, en µQTA
/// (1 000 QUANTA). Réglable par `QUANTA_RPC_MAX_SEND_UQTA`.
const DEFAULT_RPC_MAX_SEND_UQTA: u64 = 1_000 * crate::p2p::ledger::MICRO;

/// A JSON-RPC handler error: `(code, message)` mapped into the response `error`.
type RpcErr = (i64, String);

/// Self-contained web explorer served on `GET /` — no external assets, no CDN.
const EXPLORER_HTML: &str = include_str!("explorer.html");

/// Methods that hold keys or move money. In `--public` (read-only) mode these are
/// refused, so a node can be exposed to the internet as a safe explorer/query node
/// without also exposing its wallet or a tx-broadcast surface.
fn public_denied(method: &str) -> bool {
    matches!(
        method,
        "sendtoaddress" | "sendrawtransaction" | "getwalletinfo" | "getnewaddress"
    )
}

/// Serve the JSON-RPC endpoint until `shutdown` fires. Never panics; a bind failure
/// is logged and returns (the node keeps running without RPC).
pub async fn serve(
    state: Arc<AppState>,
    addr: SocketAddr,
    shutdown: CancellationToken,
    public: bool,
    auth: Arc<RpcAuth>,
) {
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            log::error!("◈ [RPC] impossible d'écouter sur {addr}: {e}");
            return;
        }
    };
    log::info!(
        "◈ [RPC] JSON-RPC + explorer sur http://{addr} ({})",
        if public { "public read-only" } else { "full/local" }
    );
    serve_on(listener, state, shutdown, public, auth).await
}

/// A5 — la boucle d'acceptation, sur une écoute **déjà liée**. Séparée de
/// [`serve`] pour qu'un test puisse prendre un port éphémère et mesurer le
/// comportement réel du serveur sous connexions muettes, plutôt que de se contenter
/// de relire les constantes.
pub async fn serve_on(
    listener: TcpListener,
    state: Arc<AppState>,
    shutdown: CancellationToken,
    public: bool,
    auth: Arc<RpcAuth>,
) {
    // A8 — une écoute sur la boucle locale n'est joignable par un navigateur que
    // par rebinding DNS ; ailleurs, le nœud est censé être atteint par son nom.
    let loopback_only = listener
        .local_addr()
        .map(|a| a.ip().is_loopback())
        .unwrap_or(true);
    let connections = Arc::new(tokio::sync::Semaphore::new(RPC_MAX_CONNECTIONS));
    let dispatch_permits = Arc::new(tokio::sync::Semaphore::new(RPC_MAX_DISPATCH));
    let per_ip: PeerSlots = Arc::new(std::sync::Mutex::new(
        std::collections::HashMap::new(),
    ));
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                log::info!("◈ [RPC] arrêt gracieux");
                break;
            }
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => {
                    // M1: drop rather than park. Without the ceiling, a slowloris
                    // pins one task + one fd per connection until the process hits
                    // its rlimit.
                    let Ok(permit) = connections.clone().try_acquire_owned() else {
                        log::debug!("◈ [RPC] connexion refusée — plafond de connexions atteint");
                        continue;
                    };
                    // A5 — équité par source, seulement là où une source veut dire
                    // quelque chose (voir RPC_MAX_CONN_PER_IP).
                    let ip_slot = if loopback_only {
                        None
                    } else {
                        match PeerSlot::acquire(&per_ip, peer.ip()) {
                            Some(s) => Some(s),
                            None => {
                                log::debug!(
                                    "◈ [RPC] connexion refusée — {} occupe déjà {} places",
                                    peer.ip(),
                                    RPC_MAX_CONN_PER_IP
                                );
                                continue;
                            }
                        }
                    };
                    let st = state.clone();
                    let au = auth.clone();
                    let dp = dispatch_permits.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        let _ip_slot = ip_slot;
                        let deadline = Duration::from_secs(RPC_READ_TIMEOUT_SECS);
                        let conn = handle_conn(stream, st, public, au, dp, loopback_only);
                        match tokio::time::timeout(deadline, conn).await {
                            Ok(Err(e)) => log::debug!("◈ [RPC] connexion: {e}"),
                            Err(_) => log::debug!("◈ [RPC] connexion expirée"),
                            Ok(Ok(())) => {}
                        }
                    });
                }
                Err(e) => {
                    log::warn!("◈ [RPC] accept: {e}");
                    // M1: EMFILE returns immediately — without this pause the loop
                    // spins at 100% CPU, floods the log and starves mining/gossip
                    // on the same runtime.
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    }
}

/// A5 — compteur de connexions vivantes par adresse source. Borné par construction :
/// il ne peut contenir plus d'entrées que [`RPC_MAX_CONNECTIONS`], puisqu'une entrée
/// disparaît dès que sa dernière connexion se termine.
type PeerSlots = Arc<std::sync::Mutex<std::collections::HashMap<std::net::IpAddr, usize>>>;

/// A5 — place occupée par une connexion pour le compte de sa source. La libération
/// passe par `Drop` : c'est la seule façon de garantir qu'elle a lieu aussi sur les
/// chemins d'erreur, d'expiration et d'annulation de tâche.
struct PeerSlot {
    slots: PeerSlots,
    ip: std::net::IpAddr,
}

impl PeerSlot {
    fn acquire(slots: &PeerSlots, ip: std::net::IpAddr) -> Option<Self> {
        let mut map = slots.lock().ok()?;
        let live = map.entry(ip).or_insert(0);
        if *live >= RPC_MAX_CONN_PER_IP {
            // Ne pas laisser une entrée à zéro derrière un refus.
            if *live == 0 {
                map.remove(&ip);
            }
            return None;
        }
        *live += 1;
        Some(Self { slots: slots.clone(), ip })
    }
}

impl Drop for PeerSlot {
    fn drop(&mut self) {
        if let Ok(mut map) = self.slots.lock() {
            if let Some(live) = map.get_mut(&self.ip) {
                *live = live.saturating_sub(1);
                if *live == 0 {
                    map.remove(&self.ip);
                }
            }
        }
    }
}

struct HttpReq {
    method: String,
    path: String,
    /// C4 (AUDIT-2026-07-25) — request headers, lowercased names. They used to be
    /// parsed and thrown away, which is why no auth, `Origin`, `Host` or
    /// `Content-Type` check was possible at all.
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpReq {
    /// Case-insensitive header lookup (`name` must already be lowercase).
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The JSON-RPC `method` field, or `""` on a malformed body.
    fn rpc_method(&self) -> String {
        serde_json::from_slice::<Value>(&self.body)
            .ok()
            .and_then(|v| v.get("method").and_then(|m| m.as_str()).map(String::from))
            .unwrap_or_default()
    }
}

/// C4 — the money RPC's authority: a random token written to `<data_dir>/.cookie`
/// at startup, in the spirit of Bitcoin Core's cookie file. Anything that can read
/// that file is already inside the trust boundary; everything else — including a
/// web page the operator happens to open in a browser on the same machine — is not.
///
/// Before this, `handle_conn` accepted any `POST /` and dispatched it: no token, no
/// cookie, no `Origin`, no `Host`, not even a `Content-Type`. Binding to
/// `127.0.0.1` stops the open internet but not the browser, which is already
/// local — and with no `Content-Type` requirement a cross-origin `fetch()` is a
/// CORS *simple* request, sent with no preflight. The attacker cannot read the
/// reply, but `sendtoaddress` does not need a readable reply to move money.
pub struct RpcAuth {
    token: String,
}

impl RpcAuth {
    /// Load the cookie from `data_dir`, or mint a fresh one.
    ///
    /// **A3 (AUDIT-2026-08-13) — COOKIE-OWN-1 : le jeton adopté doit être le
    /// nôtre.**
    ///
    /// Cette fonction adoptait tout fichier `.cookie` existant dont le contenu
    /// faisait ≥ 32 caractères, **sans regarder ni le propriétaire ni les
    /// permissions**, et sans réappliquer `0600` sur ce chemin. Un processus local
    /// quelconque n'avait donc qu'à écrire le fichier avant notre premier
    /// démarrage pour **choisir** le jeton — et obtenir l'autorité complète sur
    /// `sendtoaddress`, c'est-à-dire sur les fonds. C'était un cas de « la défense
    /// existe et ne défend rien » : le `0600` de l'écriture était contourné en ne
    /// laissant jamais l'écriture avoir lieu.
    ///
    /// Un cookie préexistant est désormais adopté **seulement si** :
    /// - nous en sommes le propriétaire (`st_uid == getuid()`) ;
    /// - aucun bit d'accès ne le laisse lisible par le groupe ou les autres
    ///   (`mode & 0o077 == 0`) ;
    /// - il n'est pas un lien symbolique (lu via `symlink_metadata`, donc un
    ///   `.cookie -> /ailleurs` est vu pour ce qu'il est) ;
    /// - il porte au moins 32 caractères, comme avant.
    ///
    /// Sinon on **écrase** avec un jeton frais plutôt que de refuser de démarrer :
    /// le démon reste utilisable, et le processus qui avait planté son jeton perd
    /// simplement son autorité. Le remplacement est journalisé bruyamment, car
    /// c'est soit une migration, soit une tentative.
    pub fn load_or_create(data_dir: &std::path::Path) -> std::io::Result<Self> {
        let path = data_dir.join(".cookie");
        match Self::adoptable_token(&path) {
            Ok(Some(t)) => return Ok(Self { token: t }),
            Ok(None) => {}
            Err(reason) => {
                log::warn!(
                    "◈ [RPC] cookie existant REFUSÉ ({reason}) — un nouveau jeton est généré. \
                     Tout client qui utilisait l'ancien doit relire {}",
                    path.display()
                );
            }
        }
        let mut raw = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut raw);
        let token = hex::encode(raw);
        std::fs::create_dir_all(data_dir)?;
        // A14 : le fichier est créé DIRECTEMENT en 0600 plutôt qu'écrit sous
        // l'umask puis corrigé — la fenêtre TOCTOU entre les deux, si courte
        // soit-elle, était une fenêtre de lecture par un autre utilisateur local.
        Self::write_private(&path, &token)?;
        Ok(Self { token })
    }

    /// A3 — le jeton du fichier existant, s'il est adoptable. `Ok(None)` : pas de
    /// fichier. `Err(raison)` : un fichier existe mais ne satisfait pas les
    /// conditions de propriété/permissions/longueur.
    fn adoptable_token(path: &std::path::Path) -> Result<Option<String>, String> {
        // `symlink_metadata` : on veut connaître le fichier À CE CHEMIN, pas la
        // cible d'un lien qu'on nous aurait tendu.
        let meta = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(_) => return Ok(None), // absent : cas normal du premier démarrage
        };
        if meta.file_type().is_symlink() {
            return Err("le chemin est un lien symbolique".into());
        }
        if !meta.is_file() {
            return Err("le chemin n'est pas un fichier ordinaire".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                // Le fichier a été lisible hors du propriétaire : que ce soit une
                // plantation ou un umask laxiste, le jeton doit être considéré
                // comme divulgué. On ne le réutilise pas.
                return Err(format!("permissions {mode:04o} — lisible hors du propriétaire"));
            }
            // Preuve de propriété **sans dépendance nouvelle et sans `unsafe`** :
            // `chmod` n'aboutit que pour le propriétaire du fichier (ou root, qui
            // nous a alors lancés). Un `.cookie` planté par un autre utilisateur
            // local échoue ici avec `EPERM`. Le même appel **réapplique** 0600 sur
            // ce chemin — ce que l'ancienne implémentation ne faisait jamais pour
            // un fichier adopté.
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("le fichier ne nous appartient pas ({e})"))?;
        }
        let contents = std::fs::read_to_string(path).map_err(|e| format!("lecture: {e}"))?;
        let t = contents.trim().to_string();
        if t.len() < 32 {
            return Err(format!("jeton trop court ({} caractères)", t.len()));
        }
        Ok(Some(t))
    }

    /// A14 — écrire un fichier de secret en 0600 **dès sa création**, sans passer
    /// par l'umask. Sur les plateformes non-Unix on retombe sur `fs::write` : il
    /// n'y a pas de bit de permission équivalent à exiger.
    fn write_private(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(path)?;
            f.write_all(contents.as_bytes())?;
            f.sync_all()
        }
        #[cfg(not(unix))]
        {
            std::fs::write(path, contents)
        }
    }

    /// The cookie path, logged at startup so an integrator knows where to read it.
    pub fn cookie_path(data_dir: &std::path::Path) -> std::path::PathBuf {
        data_dir.join(".cookie")
    }

    #[cfg(test)]
    fn with_token(token: &str) -> Self {
        Self { token: token.to_string() }
    }
}

/// C4 — why a request must be refused, or `None` when it may proceed.
///
/// Only money/wallet methods are gated: the read-only surface stays open so the
/// embedded explorer and an exchange's deposit monitoring keep working exactly as
/// before. The gate is a per-method **allowlist of what is NOT gated**
/// (`public_denied` is the same list `--public` refuses), so a method added later
/// is protected by default rather than forgotten.
fn auth_rejection(req: &HttpReq, auth: &RpcAuth, public: bool) -> Option<String> {
    let method = req.rpc_method();
    if !public_denied(&method) {
        return None; // read-only surface, unchanged
    }
    if public {
        return None; // `--public` already refuses these downstream, with its own error
    }
    // A browser attaches `Origin` automatically on cross-origin requests; a
    // same-origin or absent one is fine, a foreign one never is.
    if let Some(origin) = req.header("origin") {
        if !origin.trim().is_empty() {
            return Some("origine croisée refusée".into());
        }
    }
    // Requiring the exact content type removes the CORS *simple request* path,
    // which is what made this reachable from any web page at all.
    match req.header("content-type") {
        Some(ct) if ct.trim().starts_with("application/json") => {}
        _ => return Some("Content-Type: application/json requis".into()),
    }
    let presented = req.header("authorization").unwrap_or("").trim();
    let expected = format!("Bearer {}", auth.token);
    // Constant-time compare — cheap to do, expensive to regret.
    let ok = presented.len() == expected.len()
        && presented
            .as_bytes()
            .iter()
            .zip(expected.as_bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0;
    if ok {
        None
    } else {
        Some("authentification requise (voir le fichier .cookie du nœud)".into())
    }
}

/// M1 (AUDIT-2026-07-25) — no request may hold a task and a file descriptor
/// indefinitely. `read_request` awaited with no deadline, so a client that
/// connected and said nothing parked both forever.
const RPC_READ_TIMEOUT_SECS: u64 = 10;

/// **A5 (AUDIT-2026-08-13) — le plafond en vol transformait le slowloris en
/// coupure de service.**
///
/// Un seul plafond de 128 connexions couvrait à la fois la ressource **chère**
/// (traiter une requête : verrous, CPU, lectures de chaîne) et la ressource **bon
/// marché** (une socket dont on attend les octets), et il était détenu pendant les
/// 10 secondes entières du délai global. 128 connexions muettes — ouvertes en
/// 0,02 s, entretenues pour ~13 kbit/s — refusaient donc **tout** client légitime,
/// preuve exécutée à l'appui. Sur un nœud lancé `--public`, dont l'aide annonce
/// qu'il « peut être exposé publiquement sans risque », c'est une coupure totale à
/// la portée de n'importe qui, sans authentification.
///
/// Les deux ressources sont désormais bornées séparément :
/// - [`RPC_MAX_CONNECTIONS`] borne les descripteurs (une socket en lecture ne coûte
///   presque rien) ;
/// - [`RPC_MAX_DISPATCH`] borne le travail réellement concurrent.
///
/// Une connexion muette ne consomme donc plus **aucun** permis de traitement, et
/// elle est coupée à [`RPC_HEAD_TIMEOUT_SECS`] au lieu de 10 s.
const RPC_MAX_CONNECTIONS: usize = 256;

/// A5 — concurrence de **traitement**. Bien plus bas que le plafond de connexions :
/// c'est ici que se paient les verrous du ledger et les balayages de chaîne. Un
/// dépassement fait attendre (la borne globale de 10 s tranche), il ne ferme pas la
/// connexion au nez du client.
const RPC_MAX_DISPATCH: usize = 64;

/// A5 — délai propre à la lecture de l'**en-tête**. C'est la fenêtre exacte du
/// slowloris : un client qui n'a pas fini d'annoncer sa requête en 3 s ne la finira
/// pas. Le corps, lui, reste couvert par [`RPC_READ_TIMEOUT_SECS`] — un envoi
/// légitime de plusieurs centaines de kilo-octets sur un lien lent ne doit pas être
/// coupé par la garde anti-slowloris.
const RPC_HEAD_TIMEOUT_SECS: u64 = 3;

/// A5 — connexions simultanées tolérées depuis une même adresse source, **quand le
/// nœud n'écoute pas sur la boucle locale**.
///
/// Sur `127.0.0.1` la mesure n'a aucun sens (tout le monde a la même adresse) et
/// serait même nuisible : elle laisserait un processus local en refuser un autre.
/// Sur une écoute publique — le cas où l'attaquant est « n'importe qui » — elle
/// borne à 16/256 ce qu'un seul hôte peut immobiliser. Contre un attaquant
/// **distribué**, elle ne fait que relever le coût ; c'est dit tel quel.
const RPC_MAX_CONN_PER_IP: usize = 16;

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// A5 — lecture de l'en-tête seule, pour pouvoir lui donner son propre délai.
/// `Ok(None)` : le pair a fermé sans rien dire.
async fn read_head(stream: &mut TcpStream, buf: &mut Vec<u8>) -> std::io::Result<Option<usize>> {
    let mut tmp = [0u8; 4096];
    loop {
        if let Some(pos) = find_subsequence(buf, b"\r\n\r\n") {
            return Ok(Some(pos));
        }
        if buf.len() > MAX_HEADER {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "header too large"));
        }
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return if buf.is_empty() {
                Ok(None)
            } else {
                Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "eof in headers"))
            };
        }
        buf.extend_from_slice(&tmp[..n]);
    }
}

/// Read one HTTP/1.1 request (head + `Content-Length` body). `Ok(None)` when the
/// peer closes without sending anything. Enforces header/body size caps.
///
/// **A5 (AUDIT-2026-08-13)** — l'en-tête a son propre délai, court. C'est là, et
/// seulement là, que vit le slowloris : une requête dont l'en-tête n'est pas
/// terminé après [`RPC_HEAD_TIMEOUT_SECS`] n'est pas une requête lente, c'en est
/// une qui n'arrivera jamais. Le corps garde le délai global, pour ne pas couper un
/// envoi légitime volumineux sur un lien lent.
async fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<HttpReq>> {
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let head = tokio::time::timeout(
        Duration::from_secs(RPC_HEAD_TIMEOUT_SECS),
        read_head(stream, &mut buf),
    )
    .await
    .map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::TimedOut, "head read deadline exceeded")
    })??;
    let Some(header_end) = head else {
        return Ok(None);
    };

    let head = String::from_utf8_lossy(&buf[..header_end]);
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut req_parts = request_line.split_whitespace();
    let method = req_parts.next().unwrap_or("").to_string();
    let path = req_parts.next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let name = k.trim().to_ascii_lowercase();
            let value = v.trim().to_string();
            if name == "content-length" {
                content_length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }
    if content_length > MAX_BODY {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "body too large"));
    }

    let body_start = header_end + 4;
    let mut body = buf[body_start..].to_vec();
    let mut tmp = [0u8; 4096];
    while body.len() < content_length {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_length);
    Ok(Some(HttpReq { method, path, headers, body }))
}

/// **A16 (AUDIT-2026-08-13) — en-têtes de sécurité sur TOUTE réponse RPC.**
///
/// Les réponses n'émettaient que `Content-Type`, `Content-Length` et
/// `Connection`. L'explorateur non authentifié était donc encadrable en iframe
/// (`frame-ancestors` absent), et le navigateur restait libre de renifler le type
/// d'une réponse JSON pour l'exécuter autrement (`nosniff` absent). Ces trois
/// en-têtes ne coûtent rien et ferment les deux.
const SECURITY_HEADERS: &str = concat!(
    "X-Content-Type-Options: nosniff\r\n",
    "Referrer-Policy: no-referrer\r\n",
    "Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; \
     script-src 'unsafe-inline'; img-src data:; connect-src 'self'; base-uri 'none'; \
     form-action 'none'; frame-ancestors 'none'\r\n",
);

async fn write_response(stream: &mut TcpStream, status: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
        body.len(),
        SECURITY_HEADERS
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await
}

async fn write_html(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
        body.len(),
        SECURITY_HEADERS
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.flush().await
}

/// **A8 (AUDIT-2026-08-13) — aucune validation de `Host` : rebinding DNS.**
///
/// Le nœud répondait à `Host: attacker.example`. Une page web dont le domaine a un
/// TTL DNS d'une seconde bascule vers `127.0.0.1` : après rebinding elle est
/// **même-origine** avec `http://attaquant.tld:8645`, donc le navigateur la laisse
/// **lire** les réponses. Les méthodes d'argent restaient refusées (l'en-tête
/// `Origin` les protège), mais toute la surface de lecture était ouverte :
/// `getinfo` livre l'adresse `qta1…` de l'opérateur et son `node_id` Iroh,
/// `getbalance` son solde, `listtransactions` jusqu'à mille mouvements. C'est une
/// dé-anonymisation complète pour le prix d'un nom de domaine.
///
/// La règle est celle de l'attaque : un navigateur envoie toujours le `Host`
/// **d'origine**, celui que l'attaquant contrôle, jamais l'adresse à laquelle la
/// socket a réellement abouti. Exiger un `Host` de boucle locale suffit donc à
/// fermer le rebinding, sans gêner `curl http://127.0.0.1:8645` ni l'explorateur
/// embarqué (tous deux envoient l'adresse littérale).
///
/// Elle ne s'applique que si le nœud écoute sur la boucle locale : une écoute
/// publique est censée être atteinte par son nom, et l'opérateur l'a demandée.
/// Un `Host` absent est toléré — HTTP/1.1 l'exige, aucun navigateur ne l'omet, et
/// le tolérer garde les clients minimalistes d'intégrateurs fonctionnels.
fn host_rejection(req: &HttpReq, loopback_only: bool) -> Option<String> {
    if !loopback_only {
        return None;
    }
    let host = req.header("host")?;
    if host_is_local(host) {
        None
    } else {
        Some("en-tête Host étranger refusé (protection anti-rebinding DNS)".into())
    }
}

/// A8 — l'autorité désigne-t-elle bien la machine locale ? Le port est ignoré ; ce
/// qui compte est le nom.
fn host_is_local(host: &str) -> bool {
    let host = host.trim();
    // Forme IPv6 littérale : `[::1]:8645`.
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    if name.eq_ignore_ascii_case("localhost") {
        return true;
    }
    name.parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

async fn handle_conn(
    mut stream: TcpStream,
    state: Arc<AppState>,
    public: bool,
    auth: Arc<RpcAuth>,
    dispatch_permits: Arc<tokio::sync::Semaphore>,
    loopback_only: bool,
) -> std::io::Result<()> {
    let req = match read_request(&mut stream).await? {
        Some(r) => r,
        None => return Ok(()),
    };

    // A8 — avant tout traitement, y compris l'explorateur : une page rebindée ne
    // doit rien pouvoir lire du nœud.
    if let Some(reason) = host_rejection(&req, loopback_only) {
        let body = json!({
            "jsonrpc": "2.0", "id": Value::Null,
            "error": {"code": -32002, "message": reason}
        })
        .to_string();
        return write_response(&mut stream, "403 Forbidden", body.as_bytes()).await;
    }

    // GET / → the self-contained web explorer (it POSTs JSON-RPC to this same origin).
    if req.method.eq_ignore_ascii_case("GET") {
        let path = req.path.split('?').next().unwrap_or("/");
        if matches!(path, "/" | "/index.html" | "/explorer") {
            return write_html(&mut stream, "200 OK", EXPLORER_HTML).await;
        }
        return write_response(&mut stream, "404 Not Found", b"{\"error\":\"not found\"}").await;
    }

    if !req.method.eq_ignore_ascii_case("POST") {
        let body = json!({"error": "GET / for the explorer, POST / for JSON-RPC"}).to_string();
        return write_response(&mut stream, "405 Method Not Allowed", body.as_bytes()).await;
    }

    // C4 — money/wallet methods require the cookie token, a same-origin request and
    // a real JSON content type. Checked before the body is dispatched, so a refused
    // request never reaches a handler that holds keys.
    if let Some(reason) = auth_rejection(&req, &auth, public) {
        let body = json!({
            "jsonrpc": "2.0", "id": Value::Null,
            "error": {"code": -32001, "message": reason}
        })
        .to_string();
        return write_response(&mut stream, "401 Unauthorized", body.as_bytes()).await;
    }

    let parsed: Result<Value, _> = serde_json::from_slice(&req.body);
    let resp = match parsed {
        Err(_) => json!({"jsonrpc": "2.0", "id": Value::Null, "error": {"code": -32700, "message": "parse error"}}),
        Ok(rpc) => {
            let id = rpc.get("id").cloned().unwrap_or(Value::Null);
            let method = rpc.get("method").and_then(|m| m.as_str()).unwrap_or("");
            let params = rpc.get("params").cloned().unwrap_or_else(|| json!({}));
            if public && public_denied(method) {
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method disabled in public read-only mode"}})
            } else {
                // A5 — le permis de traitement n'est pris qu'ICI, autour du seul
                // travail coûteux. Une connexion qui lit ses octets n'en consomme
                // aucun, ce qui est exactement ce qui empêchait un client légitime
                // d'être servi pendant qu'une poignée de sockets muettes tenaient
                // tous les permis.
                let _permit = dispatch_permits.acquire().await;
                match dispatch(&state, method, &params).await {
                    Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                    Err((code, message)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
                }
            }
        }
    };
    write_response(&mut stream, "200 OK", resp.to_string().as_bytes()).await
}

fn param_str(params: &Value, key: &str) -> Result<String, RpcErr> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| (-32602, format!("missing string param '{key}'")))
}

fn param_u64(params: &Value, key: &str) -> Result<u64, RpcErr> {
    params
        .get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| (-32602, format!("missing integer param '{key}'")))
}

/// The list of supported methods (also returned by `listmethods`).
const METHODS: &[&str] = &[
    "getinfo",
    "getblockcount",
    "getblock",
    "getbalance",
    "validateaddress",
    "getfinalityheight",
    "gettransaction",
    "listtransactions",
    "getfinalityinfo",
    "getvalidators",
    "getmempool",
    "getmultisigaddress",
    "sendrawtransaction",
    "getwalletinfo",
    "getnewaddress",
    "sendtoaddress",
    "listmethods",
];

/// Route a JSON-RPC method to its handler. Params are **named** (a JSON object).
async fn dispatch(state: &Arc<AppState>, method: &str, params: &Value) -> Result<Value, RpcErr> {
    match method {
        "listmethods" => Ok(json!(METHODS)),

        "getinfo" => {
            // Lock ordering: crypto first (brief), then node/ledger.
            let address = state.crypto.lock().await.pq_address_bech32().unwrap_or_default();
            let status = state.node.get_status().await;
            let ledger = state.node.ledger.read().await;
            let stats = ledger.stats();
            // Provable-supply transparency (a listing requirement): total minted so
            // far vs the hard cap, from the shared supply math (see `crate::views`).
            let supply = crate::views::supply_view(&ledger, &stats);
            // RDV-1 — how many distinct addresses actually sealed a block recently.
            // Chain-proved, unlike `peers` (which is this node's local view only);
            // the window ships alongside it because the count is meaningless
            // without knowing what it was sampled over.
            let miners = crate::views::miners_view(&ledger);
            Ok(json!({
                "version": env!("CARGO_PKG_VERSION"),
                "protocol_version": crate::p2p::gossip::TORUS_PROTOCOL_VERSION,
                "network": "quanta",
                "micro_per_quanta": crate::p2p::ledger::MICRO,
                "height": ledger.chain_height(),
                "finalized_height": ledger.finalized_floor_index(),
                "minted_uqta": supply.minted_uqta,
                "max_supply_uqta": supply.max_supply_uqta,
                "blocks": stats.total_blocks,
                "total_txs": stats.total_txs,
                "holders": stats.holders,
                "peers": status.peer_count,
                "active_miners": miners.active_miners,
                "miner_window_blocks": miners.window_blocks,
                "online": status.is_online,
                // The Iroh EndpointId — stable across restarts (it is derived
                // from the persisted `node_key`), and the same string a peer
                // dials. Empty until the endpoint binds. Until v3.15.1 this
                // reported a per-process random value instead, so anything
                // keying on it saw a different "node" after every restart.
                "node_id": status.peer_id,
                "address": address,
            }))
        }

        "getblockcount" => Ok(json!(state.node.ledger.read().await.chain_height())),

        "getfinalityheight" => Ok(json!(state.node.ledger.read().await.finalized_floor_index())),

        // Casper-FFG finality state — the differentiator Bitcoin lacks: an exchange
        // credits a deposit once its block index ≤ finalized_floor (irreversible),
        // no confirmation-count guessing.
        "getfinalityinfo" => {
            // Shared finality math (see `crate::views`). This surface's frozen JSON
            // *is* the view's shape (all µQTA integers), so it serializes directly —
            // byte-identical to the former hand-rolled map.
            let ledger = state.node.ledger.read().await;
            serde_json::to_value(crate::views::finality_view(&ledger))
                .map_err(|_| (-32603, "serialize error".into()))
        }

        // The on-chain bonded validator set — who secures the network (PoS), sourced
        // purely from the chain (Stake/Unstake/Slash txs), stake-descending.
        "getvalidators" => {
            // Shared canonical bonded set (see `crate::views`): ≥MIN, stake-desc then
            // addr-asc. This surface adds the bech32 presentation form per entry.
            let vv = crate::views::validators_view(&*state.node.ledger.read().await);
            let list: Vec<Value> = vv
                .validators
                .into_iter()
                .map(|e| {
                    // BAS-1 : adresse LUE DE LA CHAÎNE, origine machine — la
                    // somme de contrôle n'a rien à valider ici.
                    let bech = address::parse_hex_unchecked(&e.address_hex)
                        .map(|b| address::encode(&b))
                        .unwrap_or_else(|_| e.address_hex.clone());
                    json!({ "address": bech, "address_hex": e.address_hex, "stake_uqta": e.stake_uqta })
                })
                .collect();
            Ok(json!({ "count": list.len(), "validators": list }))
        }

        // MSIG-1: compute the address of an M-of-N multisig policy. Pure derivation
        // from public keys — no secrets, no state — so it's a safe public method and
        // lets a wallet/integrator show the shared account address before funding it.
        "getmultisigaddress" => {
            let pubkeys: Vec<String> = params
                .get("pubkeys")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .ok_or((-32602, "missing 'pubkeys' array".to_string()))?;
            // AUDIT-2026-07-25: `as u32` truncates silently — a threshold of
            // 2^32+1 became 1, returning a VALID address for a policy the caller
            // never asked for (a 1-of-N where they wanted more). Reject instead.
            let threshold_raw = param_u64(params, "threshold")?;
            let threshold = u32::try_from(threshold_raw)
                .map_err(|_| (-32602, "threshold out of range".to_string()))?;
            let canon = crate::security::canonicalize_msig_keys(&pubkeys)
                .ok_or((-32602, "invalid pubkeys (each must be a valid ML-DSA-65 public key)".to_string()))?;
            if threshold == 0 || threshold as usize > canon.len() {
                return Err((-32602, "invalid policy (need 1 ≤ threshold ≤ distinct keys)".into()));
            }
            let bytes = crate::security::multisig_address_bytes(&canon, threshold)
                .ok_or((-32603, "address derivation failed".to_string()))?;
            Ok(json!({
                "address": address::encode(&bytes),
                "address_hex": hex::encode(bytes),
                "threshold": threshold,
                "keys": canon.len(),
            }))
        }

        // Pending (mempool) transactions not yet sealed into a block.
        "getmempool" => {
            // Shared mempool projection (see `crate::views`): each entry's `type`
            // field is the tx-type Debug form, as before.
            let mv = crate::views::mempool_view(&*state.node.ledger.read().await);
            Ok(json!({ "count": mv.transactions.len(), "transactions": mv.transactions }))
        }

        "getblock" => {
            let height = param_u64(params, "height")?;
            let ledger = state.node.ledger.read().await;
            match ledger.block_at(height) {
                Some(b) => serde_json::to_value(b).map_err(|_| (-32603, "serialize error".into())),
                None => Err((-32004, "block not found".into())),
            }
        }

        "getbalance" => {
            let addr = param_str(params, "address")?;
            // BAS-1 : lecture seule, l'hexadécimal d'opérateur reste accepté.
            let bytes = address::parse_hex_unchecked(&addr)
                .map_err(|_| (-32602, "invalid address".into()))?;
            let hexs = hex::encode(bytes);
            // Shared per-account money split (see `crate::views`), plus the echoed address.
            let b = crate::views::balance_view(&*state.node.ledger.read().await, &hexs);
            Ok(json!({
                "address": address::encode(&bytes),
                "spendable_uqta": b.spendable_uqta,
                "staked_uqta": b.staked_uqta,
            }))
        }

        // **BAS-1** — c'est LA fonction qu'un échange appelle avant de créditer.
        // Elle doit être **stricte** : répondre `isvalid: true` sur une chaîne
        // sans somme de contrôle, c'est certifier ce qu'on n'a pas vérifié.
        "validateaddress" => {
            let addr = param_str(params, "address")?;
            match address::parse(&addr) {
                Ok(b) => Ok(json!({
                    "isvalid": true,
                    "address": hex::encode(b),
                    "bech32": address::encode(&b),
                })),
                Err(_) => Ok(json!({ "isvalid": false })),
            }
        }

        "gettransaction" => {
            let hash = param_str(params, "hash")?;
            let ledger = state.node.ledger.read().await;
            let finalized = ledger.finalized_floor_index();
            // Search recent history, then the mempool.
            for tx in ledger.recent_txs(2000) {
                if tx.hash == hash {
                    let mut v = serde_json::to_value(&tx).map_err(|_| (-32603, "serialize error".into()))?;
                    if let Some(o) = v.as_object_mut() {
                        o.insert("in_mempool".into(), json!(false));
                    }
                    return Ok(v);
                }
            }
            for tx in ledger.pending_txs() {
                if tx.hash == hash {
                    let mut v = serde_json::to_value(tx).map_err(|_| (-32603, "serialize error".into()))?;
                    if let Some(o) = v.as_object_mut() {
                        o.insert("in_mempool".into(), json!(true));
                        o.insert("finalized".into(), json!(false));
                    }
                    return Ok(v);
                }
            }
            let _ = finalized;
            Err((-32004, "transaction not found in recent history".into()))
        }

        // Deposit-detection primitive: scan blocks for txs touching `address`.
        "listtransactions" => {
            let addr = param_str(params, "address")?;
            // BAS-1 : lecture seule, l'hexadécimal d'opérateur reste accepté.
            let bytes = address::parse_hex_unchecked(&addr)
                .map_err(|_| (-32602, "invalid address".into()))?;
            let hexs = hex::encode(bytes);
            let ledger = state.node.ledger.read().await;
            let height = ledger.chain_height();
            let finalized = ledger.finalized_floor_index();
            let from_height = params
                .get("from_height")
                .and_then(|v| v.as_u64())
                .unwrap_or_else(|| height.saturating_sub(1000));
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(100)
                .min(1000) as usize;
            let mut out: Vec<Value> = Vec::new();
            'scan: for i in from_height..=height {
                if let Some(b) = ledger.block_at(i) {
                    for tx in &b.transactions {
                        let is_in = tx.to == hexs;
                        let is_out = tx.from == hexs;
                        if is_in || is_out {
                            out.push(json!({
                                "hash": tx.hash,
                                "from": tx.from,
                                "to": tx.to,
                                "amount_uqta": tx.amount,
                                "type": format!("{:?}", tx.tx_type),
                                "height": b.index,
                                "finalized": b.index <= finalized,
                                "direction": if is_in { "in" } else { "out" },
                            }));
                            if out.len() >= limit {
                                break 'scan;
                            }
                        }
                    }
                }
            }
            Ok(json!({
                "address": address::encode(&bytes),
                "from_height": from_height,
                "height": height,
                "finalized_height": finalized,
                "transactions": out,
            }))
        }

        // Broadcast a PRE-SIGNED transaction (the withdrawal side of an exchange
        // integration: the integrator builds + signs with their own custody, the
        // node relays). Authority is the sender's ML-DSA signature — re-verified
        // here by the exact same gate the network uses for a gossiped tx, so this
        // RPC can never inject an unsigned or forged tx.
        "sendrawtransaction" => {
            use crate::p2p::ledger::VerifiedTx;
            use crate::p2p::ledger_types::TxType;

            let tx_val = params.get("tx").ok_or((-32602, "missing 'tx' param".to_string()))?;
            let tx: crate::p2p::ledger::Transaction = if let Some(s) = tx_val.as_str() {
                serde_json::from_str(s).map_err(|_| (-32602, "invalid tx json".to_string()))?
            } else {
                serde_json::from_value(tx_val.clone()).map_err(|_| (-32602, "invalid tx object".to_string()))?
            };

            // A Slash is block-only (its authority is an in-block fault proof, and
            // `verify_tx` exempts it from the signature gate) — never admit one here.
            if matches!(tx.tx_type, TxType::Slash) {
                return Err((-32602, "slash tx not accepted via RPC (block-only)".into()));
            }
            // Synthetic senders (`NETWORK`, `ESCROW`) are block-internal only.
            if tx.from == "NETWORK" || tx.from == "ESCROW" {
                return Err((-32602, "synthetic sender not allowed".into()));
            }

            // Keep a copy for gossip; the token is consumed by the local apply.
            let tx_gossip = tx.clone();

            // THE signature gate — same `VerifiedTx::new` the dispatcher uses.
            let vtx = VerifiedTx::new(tx).ok_or((-32003, "invalid transaction signature".to_string()))?;
            let (from, to, amount, tx_type) = {
                let t = vtx.tx();
                (t.from.clone(), t.to.clone(), t.amount, t.tx_type.clone())
            };

            // Local admission — mirror `handle_broadcast_tx`: reconcile the CRDT
            // mirror, then apply through the single signature-gated ledger entry
            // point (idempotent, dedup by tx hash). Locks released before crypto.
            if tx_type == TxType::Transfer {
                let mut cons = state.node.consensus.write().await;
                cons.ledger.debit(&from, &from, amount);
                cons.ledger.credit(&from, &to, amount);
            }
            let applied = {
                let mut ledger = state.node.ledger.write().await;
                ledger.apply_verified_remote_tx(vtx)
            };

            // Relay to peers, wrapped in an envelope signed by THIS node's identity
            // (transport auth); peers re-verify the tx's own signature on receipt.
            {
                let crypto = state.crypto.lock().await;
                if let Some(sender_pk) = crypto.pq_identity_hex() {
                    crate::broadcast_signed_tx(state, &crypto, &sender_pk, &tx_gossip).await;
                }
            }

            Ok(json!({
                "accepted": true,
                "applied": applied,
                "txid": tx_gossip.hash,
                "from": from,
                "to": to,
                "amount_uqta": amount,
            }))
        }

        // ── Wallet RPC (only meaningful when the node runs a persistent wallet,
        //    i.e. was started with QUANTA_WALLET_PASSWORD) ─────────────────────

        "getwalletinfo" => {
            let (has_wallet, address, hexs) = {
                let c = state.crypto.lock().await;
                (
                    c.pq_identity_hex().is_some(),
                    c.pq_address_bech32().unwrap_or_default(),
                    c.pq_address_hex().unwrap_or_default(),
                )
            };
            // Shared per-account money split (see `crate::views`), plus wallet flags.
            let b = crate::views::balance_view(&*state.node.ledger.read().await, &hexs);
            Ok(json!({
                "has_wallet": has_wallet,
                "address": address,
                "spendable_uqta": b.spendable_uqta,
                "staked_uqta": b.staked_uqta,
            }))
        }

        "getnewaddress" => {
            let c = state.crypto.lock().await;
            match c.pq_address_bech32() {
                Some(a) => Ok(json!({ "address": a })),
                None => Err((-32603, "node has no wallet identity".into())),
            }
        }

        // Build + sign a transfer from the node's OWN wallet and broadcast it.
        // Requires a persistent wallet (Ed25519 + ML-DSA); an ephemeral watch node
        // returns an error rather than silently doing nothing.
        //
        // **A4 (AUDIT-2026-08-13) — SEND-OPTIN-1 : cette méthode est désormais
        // fermée par défaut.**
        //
        // Elle signait avec la clé du nœud et diffusait sans déverrouillage, sans
        // plafond, sans confirmation et sans alerte : le cookie RPC était l'unique
        // et entière autorité de dépense du démon. Quiconque lit ce fichier —
        // c'est-à-dire tout processus local avant le correctif A3, ou tout
        // opérateur d'une sauvegarde du répertoire de données — vidait le
        // portefeuille en un appel.
        //
        // Deux garde-fous, tous deux explicites et journalisés :
        // - `QUANTA_RPC_ALLOW_SEND=1` doit être posé. Sans lui, la méthode refuse
        //   en disant comment l'ouvrir. Le reste de l'intégration d'échange n'est
        //   pas touché : `sendrawtransaction` continue de fonctionner, et il est
        //   plus sûr par construction (l'intégrateur signe avec sa propre garde,
        //   le nœud ne fait que relayer).
        // - `QUANTA_RPC_MAX_SEND_UQTA` plafonne le montant d'un appel (défaut :
        //   1 000 QUANTA). Un plafond n'arrête pas un attaquant patient, mais il
        //   transforme « une requête vide le portefeuille » en « il faut N
        //   requêtes », ce qui laisse le temps aux journaux d'exister.
        "sendtoaddress" => {
            if std::env::var("QUANTA_RPC_ALLOW_SEND").ok().as_deref() != Some("1") {
                return Err((
                    -32004,
                    "sendtoaddress est désactivé : la dépense par RPC doit être ouverte                      explicitement avec QUANTA_RPC_ALLOW_SEND=1. Pour une intégration                      d'échange, préférez sendrawtransaction (vous signez, le nœud relaie)."
                        .into(),
                ));
            }
            let to_input = param_str(params, "address")?;
            let amount = param_u64(params, "amount_uqta")?;
            if amount == 0 {
                return Err((-32602, "amount_uqta must be > 0".into()));
            }
            let cap = std::env::var("QUANTA_RPC_MAX_SEND_UQTA")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(DEFAULT_RPC_MAX_SEND_UQTA);
            if amount > cap {
                return Err((
                    -32004,
                    format!(
                        "montant {amount} µQTA au-dessus du plafond RPC {cap} µQTA                          (QUANTA_RPC_MAX_SEND_UQTA)"
                    ),
                ));
            }
            log::warn!(
                "◈ [RPC] sendtoaddress : {amount} µQTA vers {} — dépense autorisée par \
                 QUANTA_RPC_ALLOW_SEND",
                &to_input[..to_input.len().min(16)]
            );
            // BAS-1 : `sendtoaddress` accepte encore l'hexadécimal — l'appelant
            // est un opérateur qui a activé QUANTA_RPC_ALLOW_SEND et colle
            // souvent une adresse lue sur la chaîne. La forme publique `qta1…`
            // reste checksummée ; `validateaddress` est là pour trancher avant.
            let to = hex::encode(
                address::parse_hex_unchecked(&to_input)
                    .map_err(|_| (-32602, "invalid address".to_string()))?,
            );
            // Lock ordering: crypto → ledger → gossip (held throughout, like the
            // desktop wallet's send). Both legs (transfer + 1% burn) are broadcast.
            let crypto = state.crypto.lock().await;
            let from = crypto
                .pq_address_hex()
                .ok_or((-32603, "node has no wallet identity".to_string()))?;
            let sender_pk = crypto
                .pq_identity_hex()
                .ok_or((-32603, "node has no wallet identity".to_string()))?;
            let (tx, burn_tx, burn_uqta) = {
                let mut ledger = state.node.ledger.write().await;
                ledger
                    .transfer_with_burn(&from, &to, amount, &crypto)
                    .map_err(|e| (-32000, e))?
            };
            for leg in std::iter::once(&tx).chain(burn_tx.as_ref()) {
                crate::broadcast_signed_tx(state, &crypto, &sender_pk, leg).await;
            }
            Ok(json!({
                "txid": tx.hash,
                "from": from,
                "to": to,
                "amount_uqta": amount,
                "burn_uqta": burn_uqta,
            }))
        }

        _ => Err((-32601, "method not found".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_state() -> Arc<AppState> {
        Arc::new(AppState::new())
    }

    fn req_with(headers: &[(&str, &str)], body: &str) -> HttpReq {
        HttpReq {
            method: "POST".into(),
            path: "/".into(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.to_string()))
                .collect(),
            body: body.as_bytes().to_vec(),
        }
    }

    /// C4 (AUDIT-2026-07-25) — the exact attack from the report: a web page the
    /// operator visits issues a CORS *simple* request (no preflight, default
    /// `text/plain` content type) at the local node running with a loaded wallet.
    /// Before the fix this reached `sendtoaddress` and moved the money; the
    /// attacker never needs to read the reply.
    #[test]
    fn c4_browser_simple_request_cannot_reach_sendtoaddress() {
        let auth = RpcAuth::with_token("s3cret");
        let drive_by = req_with(
            &[("Content-Type", "text/plain;charset=UTF-8"), ("Origin", "https://evil.example")],
            r#"{"jsonrpc":"2.0","id":1,"method":"sendtoaddress","params":{}}"#,
        );
        assert!(
            auth_rejection(&drive_by, &auth, false).is_some(),
            "a cross-origin simple request must never reach a wallet method"
        );
    }

    /// C4 — a legitimate integrator holding the cookie is let through.
    #[test]
    fn c4_money_method_accepted_with_the_cookie_token() {
        let auth = RpcAuth::with_token("s3cret");
        let ok = req_with(
            &[("Content-Type", "application/json"), ("Authorization", "Bearer s3cret")],
            r#"{"method":"sendtoaddress"}"#,
        );
        assert!(auth_rejection(&ok, &auth, false).is_none());

        let wrong = req_with(
            &[("Content-Type", "application/json"), ("Authorization", "Bearer nope")],
            r#"{"method":"sendtoaddress"}"#,
        );
        assert!(auth_rejection(&wrong, &auth, false).is_some(), "a wrong token is refused");

        let none = req_with(
            &[("Content-Type", "application/json")],
            r#"{"method":"sendtoaddress"}"#,
        );
        assert!(auth_rejection(&none, &auth, false).is_some(), "no token is refused");
    }

    /// C4 — the read-only surface is deliberately NOT gated: the embedded explorer
    /// and an exchange's deposit monitoring must keep working untouched. Gating
    /// them would have been the easy over-correction.
    #[test]
    fn c4_read_only_surface_stays_open() {
        let auth = RpcAuth::with_token("s3cret");
        for m in ["getinfo", "getblock", "getbalance", "listtransactions", "validateaddress"] {
            let req = req_with(&[], &format!(r#"{{"method":"{m}"}}"#));
            assert!(
                auth_rejection(&req, &auth, false).is_none(),
                "{m} must stay reachable without a token"
            );
        }
    }


    /// **A5 (AUDIT-2026-08-13) — une connexion muette immobilisait un permis
    /// pendant 10 secondes.**
    ///
    /// Le plafond en vol couvrait indistinctement « une socket dont on attend les
    /// octets » et « une requête en cours de traitement », et il était détenu
    /// pendant tout le délai global : 128 connexions muettes ouvertes en 0,02 s
    /// coupaient le service à tout client légitime (preuve exécutée dans le
    /// rapport). Le test mesure la propriété qui ferme cela : un client qui se tait
    /// est libéré au délai d'**en-tête**, strictement plus court que le délai
    /// global.
    #[tokio::test]
    async fn a5_a_silent_connection_is_released_at_the_head_deadline() {
        use tokio::io::AsyncReadExt as _;
        // La propriété tient à la compilation : le délai d'en-tête doit être
        // strictement plus court que le délai global, sinon la mesure ci-dessous
        // ne distingue plus rien.
        const _: () = assert!(RPC_HEAD_TIMEOUT_SECS < RPC_READ_TIMEOUT_SECS);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let shutdown = CancellationToken::new();
        let state = test_state().await;
        let auth = Arc::new(RpcAuth::with_token("s3cret"));
        let server = tokio::spawn(serve_on(listener, state, shutdown.clone(), false, auth));

        // Un slowloris : on ouvre, on annonce une requête, et on ne la finit jamais.
        let mut sock = TcpStream::connect(addr).await.expect("connect");
        sock.write_all(b"POST / HTTP/1.1\r\nHost: 127.0.0.1\r\n")
            .await
            .expect("partial head");

        let started = std::time::Instant::now();
        let mut buf = [0u8; 64];
        // Le serveur doit fermer de son propre chef bien avant le délai global.
        let closed = tokio::time::timeout(
            Duration::from_secs(RPC_READ_TIMEOUT_SECS - 2),
            sock.read(&mut buf),
        )
        .await;
        shutdown.cancel();
        server.abort();

        assert!(
            closed.is_ok(),
            "A5 : une connexion muette doit être coupée au délai d'en-tête, pas au délai global"
        );
        assert!(
            started.elapsed() < Duration::from_secs(RPC_READ_TIMEOUT_SECS),
            "libérée en {:?}, ce qui doit rester sous le délai global",
            started.elapsed()
        );
    }

    /// A5 — sur une écoute publique, un seul hôte ne peut pas prendre toutes les
    /// places. La borne est par adresse source ; les autres sources ne la voient
    /// pas, et une place se rend à la fermeture de la connexion.
    #[test]
    fn a5_one_source_cannot_take_every_connection_slot() {
        let slots: PeerSlots = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let flooder: std::net::IpAddr = "203.0.113.7".parse().expect("ip");
        let honest: std::net::IpAddr = "198.51.100.9".parse().expect("ip");

        let held: Vec<PeerSlot> = (0..RPC_MAX_CONN_PER_IP)
            .filter_map(|_| PeerSlot::acquire(&slots, flooder))
            .collect();
        assert_eq!(held.len(), RPC_MAX_CONN_PER_IP, "la borne est atteignable");
        assert!(
            PeerSlot::acquire(&slots, flooder).is_none(),
            "A5 : une source ne dépasse pas sa part"
        );
        assert!(
            PeerSlot::acquire(&slots, honest).is_some(),
            "A5 : un autre hôte reste servi pendant l'inondation"
        );

        drop(held);
        assert!(
            PeerSlot::acquire(&slots, flooder).is_some(),
            "les places se rendent à la fermeture"
        );
    }

    /// **A8 (AUDIT-2026-08-13) — rebinding DNS : toute page web lisait l'adresse,
    /// le solde et l'historique du nœud.**
    ///
    /// Après rebinding, la page est même-origine avec `http://attaquant.tld:8645`
    /// et lit donc les réponses — mais le navigateur envoie toujours le `Host`
    /// d'origine, jamais l'adresse réellement jointe. C'est exactement ce que ce
    /// contrôle regarde.
    #[test]
    fn a8_a_rebound_host_is_refused_on_a_loopback_bind() {
        let rebound = req_with(
            &[("Host", "attacker.example:8645")],
            r#"{"method":"getinfo"}"#,
        );
        assert!(
            host_rejection(&rebound, true).is_some(),
            "A8 : un Host étranger doit être refusé sur une écoute locale"
        );
        // Les clients légitimes ne sont pas touchés.
        for host in ["127.0.0.1:8645", "localhost:8645", "[::1]:8645", "127.0.0.1"] {
            let ok = req_with(&[("Host", host)], r#"{"method":"getinfo"}"#);
            assert!(
                host_rejection(&ok, true).is_none(),
                "{host} doit rester accepté"
            );
        }
        // Un client minimaliste sans en-tête Host reste accepté (aucun navigateur
        // n'omet le Host, donc le tolérer n'ouvre pas l'attaque).
        let bare = req_with(&[], r#"{"method":"getinfo"}"#);
        assert!(host_rejection(&bare, true).is_none());
        // Écoute publique : le nœud est censé être joint par son nom.
        assert!(host_rejection(&rebound, false).is_none());
    }

    /// H7 (AUDIT-2026-07-25) — every chain-controlled string interpolated into
    /// `innerHTML` must be escaped. A transaction's `to` is attacker-chosen (the
    /// protocol imposes no shape on it) and `short()` returns any string of ≤18
    /// characters **verbatim**, so `${short(x)}` was a stored HTML injection: a
    /// transfer to `<base href=//a.co>` re-pointed every relative fetch of anyone
    /// browsing that block, including the explorer's own RPC calls. The escaped
    /// form is `${esc(short(x))}`, which does not contain the pattern below.
    #[test]
    fn h7_explorer_never_interpolates_short_unescaped() {
        assert!(
            !EXPLORER_HTML.contains("${short("),
            "unescaped short() interpolation in explorer.html — wrap it in esc()"
        );
    }

    #[tokio::test]
    async fn validateaddress_accepts_qta1_and_rejects_garbage() {
        let state = test_state().await;
        let addr = crate::security::address::encode(&[7u8; 32]);
        let ok = dispatch(&state, "validateaddress", &json!({ "address": addr })).await.unwrap();
        assert_eq!(ok["isvalid"], json!(true));
        assert!(ok["bech32"].as_str().unwrap().starts_with("qta1"));

        let bad = dispatch(&state, "validateaddress", &json!({ "address": "not-an-address" })).await.unwrap();
        assert_eq!(bad["isvalid"], json!(false));
    }

    #[tokio::test]
    async fn getblockcount_and_listmethods_and_unknown() {
        let state = test_state().await;
        // Fresh ledger has at least the genesis block → a numeric height.
        let count = dispatch(&state, "getblockcount", &json!({})).await.unwrap();
        assert!(count.is_number());

        let methods = dispatch(&state, "listmethods", &json!({})).await.unwrap();
        assert!(methods.as_array().unwrap().iter().any(|m| m == "listtransactions"));

        let err = dispatch(&state, "does_not_exist", &json!({})).await;
        assert_eq!(err.unwrap_err().0, -32601);
    }

    #[tokio::test]
    async fn getbalance_of_unknown_address_is_zero() {
        let state = test_state().await;
        let addr = crate::security::address::encode(&[9u8; 32]);
        let bal = dispatch(&state, "getbalance", &json!({ "address": addr })).await.unwrap();
        assert_eq!(bal["spendable_uqta"], json!(0));
        assert_eq!(bal["staked_uqta"], json!(0));
    }

    #[tokio::test]
    async fn sendrawtransaction_accepts_signed_and_rejects_bad() {
        use crate::p2p::ledger::{Ledger, MICRO};
        use crate::security::CryptoEngine;

        // Build a valid, signed Transfer tx with a throwaway funded ledger.
        // Building a tx signs authority on both layers (Ed25519 co-factor + the
        // ML-DSA primary that the account is actually bound to), so seed both.
        let mut crypto = CryptoEngine::new();
        let _ = crypto.generate_keypair();
        crypto.generate_pq_identity().unwrap();
        let from = crypto.pq_address_hex().unwrap();
        let to = CryptoEngine::ml_dsa_address_hex(b"rpc-recipient-key");
        let mut tmp = Ledger::new();
        tmp.mine_tx(&from, 100 * MICRO, 0.0);
        let (tx, _burn, _net) = tmp.transfer_with_burn(&from, &to, 10 * MICRO, &crypto).unwrap();
        let tx_json = serde_json::to_string(&tx).unwrap();

        let state = test_state().await;

        // Valid signature → accepted (independent of this node's local balance).
        let ok = dispatch(&state, "sendrawtransaction", &json!({ "tx": tx_json })).await.unwrap();
        assert_eq!(ok["accepted"], json!(true));
        assert_eq!(ok["txid"], json!(tx.hash));

        // Tampered amount → signature no longer matches → rejected.
        let mut bad: Value = serde_json::from_str(&tx_json).unwrap();
        bad["amount"] = json!(tx.amount + 1);
        let err = dispatch(&state, "sendrawtransaction", &json!({ "tx": bad.to_string() })).await;
        assert_eq!(err.unwrap_err().0, -32003);

        // A synthetic-sender tx (Mining, from = NETWORK) is refused outright.
        let mint = tmp.mine_tx(&to, MICRO, 0.0);
        let mint_json = serde_json::to_string(&mint).unwrap();
        let err2 = dispatch(&state, "sendrawtransaction", &json!({ "tx": mint_json })).await;
        assert_eq!(err2.unwrap_err().0, -32602);

        // Missing param → error.
        let err3 = dispatch(&state, "sendrawtransaction", &json!({})).await;
        assert_eq!(err3.unwrap_err().0, -32602);
    }

    #[tokio::test]
    async fn wallet_rpc_info_getnewaddress_and_send() {
        use crate::p2p::ledger::MICRO;
        use crate::security::CryptoEngine;

        let state = test_state().await;

        // A4 (SEND-OPTIN-1) : la dépense par RPC est fermée par défaut. Le test
        // l'ouvre explicitement — et vérifie d'abord qu'elle est bien fermée.
        let shut = dispatch(
            &state,
            "sendtoaddress",
            &json!({ "address": CryptoEngine::ml_dsa_address_hex(b"x"), "amount_uqta": 1000 }),
        )
        .await;
        assert_eq!(
            shut.unwrap_err().0,
            -32004,
            "A4 : sendtoaddress refuse tant que QUANTA_RPC_ALLOW_SEND n'est pas posé"
        );
        // SAFETY: `set_var` est `unsafe` depuis l'édition 2024 (course avec les
        // lectures d'environnement d'autres threads). Ce test est mono-thread sur
        // cette variable, qu'aucun autre test ne lit.
        unsafe { std::env::set_var("QUANTA_RPC_ALLOW_SEND", "1") };

        // No wallet identity yet → info reports it, and the key-holding methods refuse.
        let info = dispatch(&state, "getwalletinfo", &json!({})).await.unwrap();
        assert_eq!(info["has_wallet"], json!(false));
        assert_eq!(dispatch(&state, "getnewaddress", &json!({})).await.unwrap_err().0, -32603);
        let no_id = dispatch(
            &state,
            "sendtoaddress",
            &json!({ "address": CryptoEngine::ml_dsa_address_hex(b"x"), "amount_uqta": 1000 }),
        )
        .await;
        assert_eq!(no_id.unwrap_err().0, -32603);

        // Give the node a full wallet identity (Ed25519 + ML-DSA) and fund it.
        let from_hex = {
            let mut c = state.crypto.lock().await;
            let _ = c.generate_keypair();
            c.generate_pq_identity().unwrap();
            c.pq_address_hex().unwrap()
        };
        {
            state.node.ledger.write().await.mine_tx(&from_hex, 100 * MICRO, 0.0);
        }

        let info2 = dispatch(&state, "getwalletinfo", &json!({})).await.unwrap();
        assert_eq!(info2["has_wallet"], json!(true));
        assert_eq!(info2["spendable_uqta"], json!(100 * MICRO));

        let newaddr = dispatch(&state, "getnewaddress", &json!({})).await.unwrap();
        assert!(newaddr["address"].as_str().unwrap().starts_with("qta1"));

        // Send from the node's wallet → a signed tx is produced and echoed.
        let to = CryptoEngine::ml_dsa_address_hex(b"wallet-rpc-recipient");
        let sent = dispatch(&state, "sendtoaddress", &json!({ "address": to, "amount_uqta": 10 * MICRO }))
            .await
            .unwrap();
        assert!(!sent["txid"].as_str().unwrap().is_empty());
        assert_eq!(sent["amount_uqta"], json!(10 * MICRO));
    }

    #[tokio::test]
    async fn public_mode_gating_and_getinfo_supply() {
        // Public read-only mode gates exactly the key-holding / money methods.
        assert!(public_denied("sendtoaddress"));
        assert!(public_denied("sendrawtransaction"));
        assert!(public_denied("getwalletinfo"));
        assert!(public_denied("getnewaddress"));
        assert!(!public_denied("getinfo"));
        assert!(!public_denied("getblock"));
        assert!(!public_denied("listtransactions"));

        // getinfo exposes provable-supply fields (transparency for explorers/exchanges).
        let state = test_state().await;
        let info = dispatch(&state, "getinfo", &json!({})).await.unwrap();
        assert!(info["minted_uqta"].is_number());
        assert_eq!(info["max_supply_uqta"], json!(crate::p2p::reputation::MAX_SUPPLY_MICRO));
        assert!(info["blocks"].is_number());
    }

    /// `getinfo.node_id` is the node's network identity — the Iroh EndpointId,
    /// derived from the persisted `node_key`, and the exact string a peer
    /// dials. It is NOT a per-process value.
    ///
    /// It used to be `BLAKE3(Uuid::new_v4())`, minted in `WillowNode::new()`:
    /// a fresh 64-hex string on every boot that no peer could ever observe.
    /// Anything keying on it — an operator dashboard, a deposit monitor, a
    /// peer inventory — silently saw a different node after each restart.
    /// This test pins the contract: with no endpoint bound there is no
    /// identity, and the field must say so rather than invent one.
    #[tokio::test]
    async fn getinfo_node_id_is_the_network_identity_not_a_per_boot_value() {
        let state = test_state().await;

        let first = dispatch(&state, "getinfo", &json!({})).await.unwrap();
        let second = dispatch(&state, "getinfo", &json!({})).await.unwrap();
        assert_eq!(first["node_id"], second["node_id"], "identity is not re-drawn per call");

        let node_id = first["node_id"].as_str().expect("node_id is a string");
        assert!(
            node_id.is_empty(),
            "no endpoint is bound in a test state, so there is no identity to \
             report — got {node_id:?}, which is the shape of the old bug: a \
             plausible 64-hex value indistinguishable from a real EndpointId"
        );

        // It must track the one identity field, whatever the endpoint state is.
        let status = state.node.get_status().await;
        assert_eq!(first["node_id"], json!(status.peer_id));
    }

    #[tokio::test]
    async fn finality_validators_and_mempool_methods() {
        let state = test_state().await;

        let fin = dispatch(&state, "getfinalityinfo", &json!({})).await.unwrap();
        assert!(fin["epoch"].is_number());
        assert_eq!(fin["quorum_num"], json!(2));
        assert_eq!(fin["quorum_den"], json!(3));
        assert_eq!(
            fin["min_validator_stake_uqta"],
            json!(crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE)
        );

        let vals = dispatch(&state, "getvalidators", &json!({})).await.unwrap();
        assert!(vals["validators"].is_array());
        assert_eq!(vals["count"], json!(0)); // fresh chain: nobody has staked

        let mp = dispatch(&state, "getmempool", &json!({})).await.unwrap();
        assert!(mp["transactions"].is_array());
        assert_eq!(mp["count"], json!(0));
    }

    #[tokio::test]
    async fn getmultisigaddress_matches_onchain_derivation() {
        use crate::security::CryptoEngine;
        let state = test_state().await;
        // Real ML-DSA-65 public keys (the derivation now validates key well-formedness).
        let keys: Vec<String> = (0..3)
            .map(|_| {
                let mut c = CryptoEngine::new();
                c.generate_pq_identity().unwrap();
                c.pq_identity_hex().unwrap()
            })
            .collect();
        let r = dispatch(&state, "getmultisigaddress", &json!({ "pubkeys": keys, "threshold": 2 }))
            .await
            .unwrap();
        assert!(r["address"].as_str().unwrap().starts_with("qta1"));
        assert_eq!(r["keys"], json!(3));
        assert_eq!(r["threshold"], json!(2));
        // Must equal the exact derivation the consensus authority check uses.
        assert_eq!(r["address_hex"], json!(crate::security::multisig_address_hex(&keys, 2).unwrap()));

        // Invalid policy (threshold > distinct keys) → error.
        let bad = dispatch(&state, "getmultisigaddress", &json!({ "pubkeys": keys, "threshold": 9 })).await;
        assert_eq!(bad.unwrap_err().0, -32602);
        // Malformed key → rejected (MSIG-SEC-1: keys must be valid ML-DSA-65 pubkeys).
        let badkey = dispatch(&state, "getmultisigaddress", &json!({ "pubkeys": ["zz"], "threshold": 1 })).await;
        assert_eq!(badkey.unwrap_err().0, -32602);
    }
}
