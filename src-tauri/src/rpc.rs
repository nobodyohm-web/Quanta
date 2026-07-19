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
pub async fn serve(state: Arc<AppState>, addr: SocketAddr, shutdown: CancellationToken, public: bool) {
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
                        if let Err(e) = handle_conn(stream, st, public).await {
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
    path: String,
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
    let mut req_parts = request_line.split_whitespace();
    let method = req_parts.next().unwrap_or("").to_string();
    let path = req_parts.next().unwrap_or("/").to_string();

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
    Ok(Some(HttpReq { method, path, body }))
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

async fn write_html(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.flush().await
}

async fn handle_conn(mut stream: TcpStream, state: Arc<AppState>, public: bool) -> std::io::Result<()> {
    let req = match read_request(&mut stream).await? {
        Some(r) => r,
        None => return Ok(()),
    };

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
            Ok(json!({
                "version": env!("CARGO_PKG_VERSION"),
                "protocol_version": crate::p2p::gossip::TORUS_PROTOCOL_VERSION,
                "network": "quanta",
                "micro_per_quanta": crate::p2p::ledger::MICRO,
                "height": ledger.chain_height(),
                "finalized_height": ledger.finalized_floor_index(),
                // Provable-supply transparency (a listing requirement): total minted
                // so far vs the hard cap, both verifiable on-chain, no hidden mint.
                "minted_uqta": stats.total_mined,
                "max_supply_uqta": crate::p2p::reputation::MAX_SUPPLY_MICRO,
                "blocks": stats.total_blocks,
                "total_txs": stats.total_txs,
                "holders": stats.holders,
                "peers": status.peer_count,
                "online": status.is_online,
                "node_id": status.node_id,
                "address": address,
            }))
        }

        "getblockcount" => Ok(json!(state.node.ledger.read().await.chain_height())),

        "getfinalityheight" => Ok(json!(state.node.ledger.read().await.finalized_floor_index())),

        // Casper-FFG finality state — the differentiator Bitcoin lacks: an exchange
        // credits a deposit once its block index ≤ finalized_floor (irreversible),
        // no confirmation-count guessing.
        "getfinalityinfo" => {
            use crate::sm::finality::EPOCH_LENGTH_BLOCKS;
            let ledger = state.node.ledger.read().await;
            let height = ledger.chain_height();
            let stakes = ledger.validator_stakes();
            let total_staked: u64 = stakes.values().sum();
            let min = crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE;
            let validators = stakes.values().filter(|&&s| s >= min).count();
            let blocks_into_epoch = height % EPOCH_LENGTH_BLOCKS;
            Ok(json!({
                "height": height,
                "finalized_floor": ledger.finalized_floor_index(),
                "epoch": height / EPOCH_LENGTH_BLOCKS,
                "epoch_length": EPOCH_LENGTH_BLOCKS,
                "blocks_into_epoch": blocks_into_epoch,
                "next_checkpoint": height - blocks_into_epoch + EPOCH_LENGTH_BLOCKS,
                "validators": validators,
                "total_staked_uqta": total_staked,
                "min_validator_stake_uqta": min,
                "quorum_num": 2,
                "quorum_den": 3,
            }))
        }

        // The on-chain bonded validator set — who secures the network (PoS), sourced
        // purely from the chain (Stake/Unstake/Slash txs), stake-descending.
        "getvalidators" => {
            let stakes = state.node.ledger.read().await.validator_stakes();
            let min = crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE;
            let mut vs: Vec<(String, u64)> = stakes.into_iter().filter(|(_, s)| *s >= min).collect();
            vs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let list: Vec<Value> = vs
                .into_iter()
                .map(|(addr_hex, stake)| {
                    let bech = address::parse(&addr_hex)
                        .map(|b| address::encode(&b))
                        .unwrap_or_else(|_| addr_hex.clone());
                    json!({ "address": bech, "address_hex": addr_hex, "stake_uqta": stake })
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
            let threshold = param_u64(params, "threshold")? as u32;
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
            let ledger = state.node.ledger.read().await;
            let txs: Vec<Value> = ledger
                .pending_txs()
                .iter()
                .map(|t| {
                    json!({
                        "hash": t.hash,
                        "from": t.from,
                        "to": t.to,
                        "amount_uqta": t.amount,
                        "type": format!("{:?}", t.tx_type),
                    })
                })
                .collect();
            Ok(json!({ "count": txs.len(), "transactions": txs }))
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
            let ledger = state.node.ledger.read().await;
            Ok(json!({
                "has_wallet": has_wallet,
                "address": address,
                "spendable_uqta": ledger.balance_of(&hexs),
                "staked_uqta": ledger.staked_of(&hexs),
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
        "sendtoaddress" => {
            let to_input = param_str(params, "address")?;
            let amount = param_u64(params, "amount_uqta")?;
            if amount == 0 {
                return Err((-32602, "amount_uqta must be > 0".into()));
            }
            let to = hex::encode(
                address::parse(&to_input).map_err(|_| (-32602, "invalid address".to_string()))?,
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
