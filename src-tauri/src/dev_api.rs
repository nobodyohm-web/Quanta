//! Phase 3 — Dev HTTP API.
//!
//! Petit serveur HTTP local (`127.0.0.1:7654`) pour publier/lister/chercher
//! des sites Torus depuis VSCode, un terminal ou tout outil externe.
//!
//! - Aucune nouvelle dépendance : parseur HTTP minimal sur `tokio::net::TcpListener`.
//! - Auth obligatoire via header `Authorization: Bearer <token>` (token aléatoire
//!   32 bytes hex, persisté dans `~/.torus/dev-api-token`).
//! - Désactivé par défaut. L'utilisateur l'active depuis la page Settings.
//!
//! Endpoints :
//!   - `POST   /api/publish`  → publie un site (signature, store, broadcast)
//!   - `GET    /api/status`   → état du nœud (pk, balance, sites, search docs)
//!   - `GET    /api/search`   → recherche P2P (?q=&lang=&tag=)
//!   - `DELETE /api/site`     → dépublie le site du wallet courant
//!   - `GET    /api/health`   → ping (200 si activé, 503 sinon)

use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::AppState;

// ─── Constantes ─────────────────────────────────────────────────────────────

pub const DEV_API_ADDR: &str = "127.0.0.1:7654";
pub const DEV_API_TOKEN_FILE: &str = "dev-api-token";
pub const DEV_API_ENABLED_FILE: &str = "dev-api-enabled";

const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024; // 16 MB
const MAX_HEADER_BYTES: usize = 16 * 1024;

// ─── Storage helpers ────────────────────────────────────────────────────────

fn dev_api_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".torus")
}

fn token_path() -> PathBuf {
    dev_api_dir().join(DEV_API_TOKEN_FILE)
}

fn enabled_path() -> PathBuf {
    dev_api_dir().join(DEV_API_ENABLED_FILE)
}

/// Génère un token random 32 bytes hex (BLAKE3 sur l'horloge + un sel système).
fn generate_token() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Mix : nanos + addr de l'allocateur + thread id pour entropie.
    let salt = format!("{now}-{:p}-{:?}", &now, std::thread::current().id());
    let h = blake3::hash(salt.as_bytes());
    hex::encode(h.as_bytes())
}

/// Renvoie le token courant depuis le disque, ou en génère un nouveau.
pub fn ensure_token() -> std::io::Result<String> {
    let dir = dev_api_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    let path = token_path();
    if path.exists() {
        let s = std::fs::read_to_string(&path)?.trim().to_string();
        if s.len() == 64 {
            return Ok(s);
        }
    }
    let t = generate_token();
    std::fs::write(&path, &t)?;
    Ok(t)
}

/// Régénère le token (force).
pub fn rotate_token() -> std::io::Result<String> {
    let dir = dev_api_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    let t = generate_token();
    std::fs::write(token_path(), &t)?;
    Ok(t)
}

/// État activé/désactivé persistant (présence du fichier = activé).
pub fn is_enabled() -> bool {
    enabled_path().exists()
}

pub fn set_enabled(enabled: bool) -> std::io::Result<()> {
    let dir = dev_api_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    let path = enabled_path();
    if enabled {
        if !path.exists() {
            std::fs::write(&path, "1")?;
        }
        // Pré-créer le token pour que l'UI puisse l'afficher tout de suite.
        let _ = ensure_token();
    } else if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

// ─── Server ─────────────────────────────────────────────────────────────────

/// Lance le serveur HTTP. Idempotent : un seul worker tourne à la fois grâce au
/// flag `is_enabled()` consulté par requête (le listener bind une seule fois au
/// boot ; les requêtes reçoivent 503 quand l'API est désactivée).
pub fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        let listener = match TcpListener::bind(DEV_API_ADDR).await {
            Ok(l) => l,
            Err(e) => {
                log::warn!("◈ [DEV-API] bind {DEV_API_ADDR} failed: {e}");
                return;
            }
        };
        log::info!("◈ [DEV-API] listening on {DEV_API_ADDR}");
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    if !addr.ip().is_loopback() {
                        // Sécurité défensive : on ne bind que loopback, mais
                        // double-check au cas où (port forwarding, etc.).
                        let _ = stream.into_std();
                        continue;
                    }
                    let st = state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, st).await {
                            log::debug!("◈ [DEV-API] connection error: {e}");
                        }
                    });
                }
                Err(e) => log::debug!("◈ [DEV-API] accept failed: {e}"),
            }
        }
    });
}

// ─── HTTP request/response types ────────────────────────────────────────────

#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        for (k, v) in &self.headers {
            if k.eq_ignore_ascii_case(name) {
                return Some(v.as_str());
            }
        }
        None
    }
}

fn http_response(status: u16, ctype: &str, body: &[u8]) -> Vec<u8> {
    let phrase = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    };
    let mut out = format!(
        "HTTP/1.1 {status} {phrase}\r\n\
         Content-Type: {ctype}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         Access-Control-Allow-Origin: http://127.0.0.1\r\n\
         \r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

fn json_response(status: u16, value: serde_json::Value) -> Vec<u8> {
    let s = value.to_string();
    http_response(status, "application/json", s.as_bytes())
}

fn err_json(status: u16, message: &str) -> Vec<u8> {
    json_response(status, serde_json::json!({ "error": message }))
}

// ─── Parser ─────────────────────────────────────────────────────────────────

async fn read_request(stream: &mut TcpStream) -> std::io::Result<Request> {
    let mut buf: Vec<u8> = Vec::with_capacity(2048);
    let mut tmp = [0u8; 2048];
    let mut header_end: Option<usize> = None;
    while header_end.is_none() {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "eof"));
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() > MAX_HEADER_BYTES + 1024 {
            return Err(std::io::Error::other("header too large"));
        }
        if let Some(p) = find_header_end(&buf) {
            header_end = Some(p);
            break;
        }
    }
    let split = header_end.unwrap();
    let header_bytes = &buf[..split];
    let header_str = std::str::from_utf8(header_bytes)
        .map_err(|_| std::io::Error::other("non-utf8 header"))?;
    let mut lines = header_str.split("\r\n");
    let request_line = lines.next().ok_or_else(|| std::io::Error::other("no request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or_else(|| std::io::Error::other("no method"))?.to_string();
    let raw_path = parts.next().ok_or_else(|| std::io::Error::other("no path"))?.to_string();
    let (path, query) = match raw_path.find('?') {
        Some(i) => (raw_path[..i].to_string(), raw_path[i + 1..].to_string()),
        None => (raw_path, String::new()),
    };
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some(colon) = line.find(':') {
            let name = line[..colon].trim().to_string();
            let value = line[colon + 1..].trim().to_string();
            headers.push((name, value));
        }
    }
    // Body
    let cl = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    if cl > MAX_REQUEST_BYTES {
        return Err(std::io::Error::other("body too large"));
    }
    let body_offset = split + 4;
    let mut body: Vec<u8> = if buf.len() > body_offset {
        buf[body_offset..].to_vec()
    } else {
        Vec::new()
    };
    while body.len() < cl {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
        if body.len() > MAX_REQUEST_BYTES {
            return Err(std::io::Error::other("body too large"));
        }
    }
    body.truncate(cl);
    Ok(Request {
        method,
        path,
        query,
        headers,
        body,
    })
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    if buf.len() < 4 {
        return None;
    }
    for i in 0..buf.len() - 3 {
        if &buf[i..i + 4] == b"\r\n\r\n" {
            return Some(i);
        }
    }
    None
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex_str = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(b) = u8::from_str_radix(hex_str, 16) {
                    out.push(b);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        if let Some(eq) = pair.find('=') {
            out.insert(url_decode(&pair[..eq]), url_decode(&pair[eq + 1..]));
        } else {
            out.insert(url_decode(pair), String::new());
        }
    }
    out
}

// ─── Connection handler ─────────────────────────────────────────────────────

async fn handle_connection(mut stream: TcpStream, state: Arc<AppState>) -> std::io::Result<()> {
    let req = match read_request(&mut stream).await {
        Ok(r) => r,
        Err(e) => {
            let body = err_json(400, &format!("bad request: {e}"));
            stream.write_all(&body).await?;
            return Ok(());
        }
    };

    // Service availability check.
    if !is_enabled() && req.path != "/api/health" {
        let resp = err_json(503, "Dev API disabled. Enable it from Settings.");
        stream.write_all(&resp).await?;
        return Ok(());
    }

    // Health check (no auth).
    if req.path == "/api/health" {
        let resp = if is_enabled() {
            json_response(200, serde_json::json!({ "status": "ok", "enabled": true }))
        } else {
            json_response(503, serde_json::json!({ "status": "disabled", "enabled": false }))
        };
        stream.write_all(&resp).await?;
        return Ok(());
    }

    // Auth.
    let token = ensure_token().unwrap_or_default();
    let auth_ok = req
        .header("authorization")
        .map(|h| h.trim() == format!("Bearer {token}"))
        .unwrap_or(false);
    if !auth_ok {
        let resp = err_json(401, "missing or invalid bearer token");
        stream.write_all(&resp).await?;
        return Ok(());
    }

    // Routing.
    let resp = match (req.method.as_str(), req.path.as_str()) {
        ("POST", "/api/publish") => handle_publish(&state, &req).await,
        ("GET", "/api/status") => handle_status(&state).await,
        ("GET", "/api/search") => handle_search(&state, &req).await,
        ("DELETE", "/api/site") => handle_delete_site(&state).await,
        (_, "/api/publish") | (_, "/api/status") | (_, "/api/search") | (_, "/api/site") => {
            err_json(405, "method not allowed")
        }
        _ => err_json(404, "not found"),
    };
    stream.write_all(&resp).await?;
    Ok(())
}

// ─── Handlers ───────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct PublishBody {
    title: String,
    html: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_lang")]
    lang: String,
    #[serde(default = "default_kind")]
    kind: String,
    #[serde(default)]
    domain: Option<String>,
}

fn default_lang() -> String {
    "fr".into()
}
fn default_kind() -> String {
    "site".into()
}

async fn handle_publish(state: &Arc<AppState>, req: &Request) -> Vec<u8> {
    let body: PublishBody = match serde_json::from_slice(&req.body) {
        Ok(b) => b,
        Err(e) => return err_json(400, &format!("invalid JSON: {e}")),
    };

    if body.title.is_empty() || body.html.is_empty() {
        return err_json(400, "title and html are required");
    }

    let pk = match state.crypto.lock().await.get_identity() {
        Ok(id) => id.public_key_hex,
        Err(e) => return err_json(500, &format!("identity not unlocked: {e}")),
    };
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let version = {
        let store = state.node.page_store.read().await;
        store.get_page(&pk).map(|p| p.version + 1).unwrap_or(1)
    };

    let signable_content = format!("{}:{}:{}", pk, version, body.html);
    let sig_bytes = match state.crypto.lock().await.sign(signable_content.as_bytes()) {
        Ok(s) => s,
        Err(e) => return err_json(500, &format!("sign failed: {e}")),
    };
    let signature = hex::encode(&sig_bytes);

    // Tags : input → sanitize ; sinon auto-extract.
    let final_tags: Vec<String> = if body.tags.is_empty() {
        crate::p2p::search::auto_extract_tags(&body.html, &body.title)
    } else {
        crate::p2p::search::sanitize_tags(&body.tags)
    };

    let page = crate::p2p::page_store::PublishedPage {
        author_pk: pk.clone(),
        content: body.html.clone(),
        title: body.title.clone(),
        updated_at: timestamp,
        signature,
        version,
        tags: final_tags.clone(),
    };

    {
        let mut store = state.node.page_store.write().await;
        if let Err(e) = store.publish(page.clone()) {
            return err_json(400, &format!("publish failed: {e}"));
        }
    }

    // Broadcast PublishPage.
    let page_json = match serde_json::to_string(&page) {
        Ok(s) => s,
        Err(e) => return err_json(500, &format!("serialise page: {e}")),
    };
    let msg = crate::p2p::gossip::GossipMessage::PublishPage { page_json };
    let ts = chrono::Utc::now().to_rfc3339();
    let nonce = state.node.gossip.read().await.next_outgoing_nonce();
    let signable = crate::p2p::gossip::GossipRouter::signable_envelope_bytes(&pk, nonce, &ts, &msg);
    let env_sig = state.crypto.lock().await.sign(&signable).unwrap_or_default();
    if let Ok(env) = crate::p2p::gossip::GossipRouter::build_signed_envelope(
        pk.clone(), msg, nonce, ts, &env_sig,
    ) {
        state.node.gossip.write().await.mark_seen(&env.id);
        let _ = state.node.gossip_tx.send(env);
        state.node.gossip.write().await.stats.pages_published += 1;
    }

    // Auto-index dans le SearchIndex local + broadcast PublishSite.
    let plain_text = crate::p2p::search::strip_html(&body.html);
    let cid = hex::encode(blake3::hash(body.html.as_bytes()).as_bytes());
    let snippet: String = plain_text.chars().take(200).collect();
    let tokens = crate::p2p::search::tokenize(&format!("{} {}", body.title, plain_text));
    let tf = crate::p2p::search::term_freq(&tokens);
    let kind_enum = match body.kind.as_str() {
        "blog" => crate::p2p::search::DocKind::Blog,
        "forum" => crate::p2p::search::DocKind::Forum,
        "comment" => crate::p2p::search::DocKind::Comment,
        "shop" => crate::p2p::search::DocKind::Shop,
        _ => crate::p2p::search::DocKind::Site,
    };
    let doc = crate::p2p::search::IndexedDoc {
        cid: cid.clone(),
        title: body.title.clone(),
        snippet,
        author_pk: pk.clone(),
        kind: kind_enum,
        lang: body.lang.clone(),
        updated_at: timestamp,
        term_freq: tf,
        torus_domain: body.domain.clone(),
        tags: final_tags.clone(),
    };
    state.node.search.write().await.upsert(doc.clone());
    if let Ok(doc_json) = serde_json::to_string(&doc) {
        let msg2 = crate::p2p::gossip::GossipMessage::PublishSite { doc_json };
        let ts2 = chrono::Utc::now().to_rfc3339();
        let nonce2 = state.node.gossip.read().await.next_outgoing_nonce();
        let signable2 = crate::p2p::gossip::GossipRouter::signable_envelope_bytes(
            &pk, nonce2, &ts2, &msg2,
        );
        let env_sig2 = state.crypto.lock().await.sign(&signable2).unwrap_or_default();
        if let Ok(env2) = crate::p2p::gossip::GossipRouter::build_signed_envelope(
            pk.clone(),
            msg2,
            nonce2,
            ts2,
            &env_sig2,
        ) {
            state.node.gossip.write().await.mark_seen(&env2.id);
            let _ = state.node.gossip_tx.send(env2);
        }
    }

    json_response(
        200,
        serde_json::json!({
            "cid": cid,
            "author_pk": pk,
            "version": version,
            "tags": final_tags,
        }),
    )
}

async fn handle_status(state: &Arc<AppState>) -> Vec<u8> {
    let pk = state
        .crypto
        .lock()
        .await
        .get_identity()
        .map(|id| id.public_key_hex)
        .unwrap_or_default();
    let balance_uqta = if pk.is_empty() {
        0
    } else {
        state.node.ledger.read().await.balance_of(&pk)
    };
    let sites_count = if pk.is_empty() {
        0
    } else {
        let store = state.node.page_store.read().await;
        let mut n = 0;
        if store.get_page(&pk).is_some() {
            n += 1;
        }
        if store.get_site(&pk).is_some() {
            n += 1;
        }
        n
    };
    let search_docs = state.node.search.read().await.doc_count();
    json_response(
        200,
        serde_json::json!({
            "pk": pk,
            "balance_qta": (balance_uqta as f64) / 1_000_000.0,
            "sites_count": sites_count,
            "search_docs": search_docs,
            "endpoint": DEV_API_ADDR,
        }),
    )
}

async fn handle_search(state: &Arc<AppState>, req: &Request) -> Vec<u8> {
    let q = parse_query(&req.query);
    let query = q.get("q").cloned().unwrap_or_default();
    if query.trim().is_empty() {
        return err_json(400, "missing query parameter `q`");
    }
    let lang = q.get("lang").cloned();
    let tag = q
        .get("tag")
        .and_then(|t| crate::p2p::search::sanitize_tag(t));
    let limit = q
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(20)
        .min(100);
    let filters = crate::p2p::search::SearchFilters {
        lang,
        since_ts: None,
        kind: None,
        creator_pk: None,
        min_likes: None,
        tag,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let index = state.node.search.read().await;
    let hits = index.search(&query, &filters, now, limit, |_| {
        crate::p2p::search::SocialSignals {
            creator_reputation: 0.5,
            ..Default::default()
        }
    });
    json_response(200, serde_json::to_value(hits).unwrap_or_default())
}

async fn handle_delete_site(state: &Arc<AppState>) -> Vec<u8> {
    let pk = match state.crypto.lock().await.get_identity() {
        Ok(id) => id.public_key_hex,
        Err(e) => return err_json(500, &format!("identity not unlocked: {e}")),
    };
    // Le PageStore actuel n'expose pas de remove ; on remplace par une page
    // vide signée pour invalider le contenu. Le SearchIndex retire le doc.
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let version = state
        .node
        .page_store
        .read()
        .await
        .get_page(&pk)
        .map(|p| p.version + 1)
        .unwrap_or(1);
    let empty_html = "<!DOCTYPE html><html><body></body></html>";
    let signable = format!("{}:{}:{}", pk, version, empty_html);
    let sig = match state.crypto.lock().await.sign(signable.as_bytes()) {
        Ok(s) => hex::encode(&s),
        Err(e) => return err_json(500, &format!("sign failed: {e}")),
    };
    let page = crate::p2p::page_store::PublishedPage {
        author_pk: pk.clone(),
        content: empty_html.into(),
        title: String::new(),
        updated_at: timestamp,
        signature: sig,
        version,
        tags: Vec::new(),
    };
    if let Err(e) = state.node.page_store.write().await.publish(page) {
        return err_json(400, &format!("delete failed: {e}"));
    }
    // Drop le doc depuis le search index (best-effort : on cherche par auteur).
    let mut idx = state.node.search.write().await;
    let cids: Vec<String> = idx
        .snapshot()
        .docs
        .into_iter()
        .filter(|d| d.author_pk == pk)
        .map(|d| d.cid)
        .collect();
    for cid in cids {
        idx.remove(&cid);
    }
    json_response(200, serde_json::json!({ "deleted": true, "version": version }))
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_roundtrip() {
        let t1 = generate_token();
        assert_eq!(t1.len(), 64);
        assert!(t1.chars().all(|c| c.is_ascii_hexdigit()));
        let t2 = generate_token();
        assert_ne!(t1, t2, "tokens must be unique");
    }

    #[test]
    fn parse_query_basics() {
        let q = parse_query("q=hello%20world&lang=fr&tag=chaussures");
        assert_eq!(q.get("q").map(|s| s.as_str()), Some("hello world"));
        assert_eq!(q.get("lang").map(|s| s.as_str()), Some("fr"));
        assert_eq!(q.get("tag").map(|s| s.as_str()), Some("chaussures"));
    }

    #[test]
    fn url_decode_pct_and_plus() {
        assert_eq!(url_decode("a+b%20c"), "a b c");
        assert_eq!(url_decode("100%25"), "100%");
    }

    #[test]
    fn http_response_format() {
        let r = http_response(200, "application/json", b"{}");
        let s = std::str::from_utf8(&r).unwrap();
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(s.contains("Content-Length: 2\r\n"));
        assert!(s.ends_with("\r\n\r\n{}"));
    }

    #[test]
    fn find_header_end_works() {
        let bytes = b"GET / HTTP/1.1\r\nHost: x\r\n\r\nbody";
        let p = find_header_end(bytes).unwrap();
        assert_eq!(&bytes[p..p + 4], b"\r\n\r\n");
    }
}
