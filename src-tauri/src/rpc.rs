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

/// A JSON-RPC handler error: `(code, message)` mapped into the response `error`.
type RpcErr = (i64, String);

/// Serve the JSON-RPC endpoint until `shutdown` fires. Never panics; a bind failure
/// is logged and returns (the node keeps running without RPC).
pub async fn serve(state: Arc<AppState>, addr: SocketAddr, shutdown: CancellationToken) {
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            log::error!("◈ [RPC] impossible d'écouter sur {addr}: {e}");
            return;
        }
    };
    log::info!("◈ [RPC] JSON-RPC actif sur http://{addr}");
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                log::info!("◈ [RPC] arrêt gracieux");
                break;
            }
            accepted = listener.accept() => match accepted {
                Ok((stream, _peer)) => {
                    let st = state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_conn(stream, st).await {
                            log::debug!("◈ [RPC] connexion: {e}");
                        }
                    });
                }
                Err(e) => log::warn!("◈ [RPC] accept: {e}"),
            }
        }
    }
}

struct HttpReq {
    method: String,
    body: Vec<u8>,
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Read one HTTP/1.1 request (head + `Content-Length` body). `Ok(None)` when the
/// peer closes without sending anything. Enforces header/body size caps.
async fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<HttpReq>> {
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break pos;
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
    };

    let head = String::from_utf8_lossy(&buf[..header_end]);
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let method = request_line.split_whitespace().next().unwrap_or("").to_string();

    let mut content_length = 0usize;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    if content_length > MAX_BODY {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "body too large"));
    }

    let body_start = header_end + 4;
    let mut body = buf[body_start..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_length);
    Ok(Some(HttpReq { method, body }))
}

async fn write_response(stream: &mut TcpStream, status: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await
}

async fn handle_conn(mut stream: TcpStream, state: Arc<AppState>) -> std::io::Result<()> {
    let req = match read_request(&mut stream).await? {
        Some(r) => r,
        None => return Ok(()),
    };
    if !req.method.eq_ignore_ascii_case("POST") {
        let body = json!({"error": "JSON-RPC endpoint: POST only"}).to_string();
        return write_response(&mut stream, "405 Method Not Allowed", body.as_bytes()).await;
    }
    let parsed: Result<Value, _> = serde_json::from_slice(&req.body);
    let resp = match parsed {
        Err(_) => json!({"jsonrpc": "2.0", "id": Value::Null, "error": {"code": -32700, "message": "parse error"}}),
        Ok(rpc) => {
            let id = rpc.get("id").cloned().unwrap_or(Value::Null);
            let method = rpc.get("method").and_then(|m| m.as_str()).unwrap_or("");
            let params = rpc.get("params").cloned().unwrap_or_else(|| json!({}));
            match dispatch(&state, method, &params).await {
                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                Err((code, message)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
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
            Ok(json!({
                "version": env!("CARGO_PKG_VERSION"),
                "protocol_version": crate::p2p::gossip::TORUS_PROTOCOL_VERSION,
                "network": "quanta",
                "micro_per_quanta": crate::p2p::ledger::MICRO,
                "height": ledger.chain_height(),
                "finalized_height": ledger.finalized_floor_index(),
                "peers": status.peer_count,
                "online": status.is_online,
                "node_id": status.node_id,
                "address": address,
            }))
        }

        "getblockcount" => Ok(json!(state.node.ledger.read().await.chain_height())),

        "getfinalityheight" => Ok(json!(state.node.ledger.read().await.finalized_floor_index())),

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
            let bytes = address::parse(&addr).map_err(|_| (-32602, "invalid address".into()))?;
            let hexs = hex::encode(bytes);
            let ledger = state.node.ledger.read().await;
            Ok(json!({
                "address": address::encode(&bytes),
                "spendable_uqta": ledger.balance_of(&hexs),
                "staked_uqta": ledger.staked_of(&hexs),
            }))
        }

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
            let bytes = address::parse(&addr).map_err(|_| (-32602, "invalid address".into()))?;
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

        _ => Err((-32601, "method not found".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_state() -> Arc<AppState> {
        Arc::new(AppState::new())
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
}
