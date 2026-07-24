//! `quanta-node` — the headless Quanta daemon.
//!
//! Runs the full node core (Iroh P2P sync, ledger, Casper-FFG finality) with **no
//! GUI**, and serves a JSON-RPC endpoint for wallets, block explorers and exchange
//! integrations (deposit monitoring, address validation, chain queries).
//!
//! Two identity modes:
//! - **Ephemeral** (default): an in-memory ML-DSA identity — joins gossip and serves
//!   the RPC, but holds no funds (a safe watch / relay / integration node).
//! - **Persistent wallet** (`QUANTA_WALLET_PASSWORD` set): unlocks an existing wallet
//!   or creates one in the data dir (Ed25519 + ML-DSA primary), so the node can mine
//!   to a stable address, hold funds, and sign its own sends (`sendtoaddress`).
//!
//! ```text
//! quanta-node [--data-dir <path>] [--rpc-addr <ip:port>] [--mine]
//! ```
//! `RUST_LOG` controls logging; `QUANTA_WALLET_PASSWORD` selects the wallet.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use quanta_lib::{node_runtime, rpc, AppState};

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cfg = Config::from_args();

    // PQ transport (X25519MLKEM768) — same provider install as the desktop app.
    node_runtime::install_crypto_provider();

    // Same App Nap opt-out as the app: a daemon exists to run unattended.
    node_runtime::prevent_app_nap();

    let state = Arc::new(AppState::new());

    // `QUANTA_WALLET_PASSWORD` opens a PERSISTENT wallet (unlock existing / create
    // new) so the node can hold funds, mine to a stable address and sign its own
    // sends. Absent → an ephemeral in-memory identity: joins gossip, holds no funds.
    let wallet_password = std::env::var("QUANTA_WALLET_PASSWORD").ok().filter(|s| !s.is_empty());

    // DB first (identity persistence lives there), then the wallet, then the network
    // (the mining loop, if enabled, needs the identity established before it starts).
    node_runtime::open_db(&state, cfg.data_dir.clone()).await;
    if let Err(e) = node_runtime::establish_wallet(&state, wallet_password.as_deref()).await {
        log::error!("◈ [quanta-node] initialisation du wallet impossible: {e}");
        std::process::exit(1);
    }
    node_runtime::start_network(&state, cfg.mine).await;

    let address = state.crypto.lock().await.pq_address_bech32().unwrap_or_default();
    log::info!(
        "◈ [quanta-node] démarré — data-dir={:?} rpc=http://{} mine={} wallet={} addr={}",
        cfg.data_dir,
        cfg.rpc_addr,
        cfg.mine,
        if wallet_password.is_some() { "persistent" } else { "ephemeral" },
        address
    );

    // C4 (AUDIT-2026-07-25) — mint or load the RPC cookie before serving. Money and
    // wallet methods are refused without it, so a browser page on this machine can
    // no longer reach `sendtoaddress` on a node running with a loaded wallet.
    let auth = match rpc::RpcAuth::load_or_create(&cfg.data_dir) {
        Ok(a) => std::sync::Arc::new(a),
        Err(e) => {
            log::error!("◈ [quanta-node] impossible d'écrire le cookie RPC : {e}");
            return;
        }
    };
    log::info!(
        "◈ [quanta-node] cookie RPC : {}",
        rpc::RpcAuth::cookie_path(&cfg.data_dir).display()
    );

    let shutdown = state.node.shutdown.clone();
    tokio::select! {
        _ = rpc::serve(state.clone(), cfg.rpc_addr, shutdown.clone(), cfg.public, auth) => {}
        _ = tokio::signal::ctrl_c() => {
            log::info!("◈ [quanta-node] SIGINT reçu — arrêt gracieux");
        }
    }
    // Signal every background task to stop (idempotent).
    shutdown.cancel();
    log::info!("◈ [quanta-node] arrêté");
}

/// Parsed command-line configuration.
struct Config {
    data_dir: PathBuf,
    rpc_addr: SocketAddr,
    mine: bool,
    public: bool,
}

impl Config {
    fn from_args() -> Self {
        // Default RPC bind: localhost only. Money RPC is never exposed to the open
        // internet by default; an integrator co-locates the node or tunnels to it.
        let mut data_dir = node_runtime::default_data_dir();
        let mut rpc_addr = SocketAddr::from(([127, 0, 0, 1], 8645));
        let mut mine = false;
        let mut public = false;

        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--data-dir" => {
                    if let Some(v) = args.next() {
                        data_dir = PathBuf::from(v);
                    }
                }
                "--rpc-addr" => {
                    if let Some(v) = args.next() {
                        match v.parse() {
                            Ok(addr) => rpc_addr = addr,
                            Err(e) => {
                                eprintln!("--rpc-addr invalide '{v}': {e}");
                                std::process::exit(2);
                            }
                        }
                    }
                }
                "--mine" => mine = true,
                "--public" => public = true,
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                other => {
                    eprintln!("argument inconnu: {other}\n");
                    print_help();
                    std::process::exit(2);
                }
            }
        }
        Config { data_dir, rpc_addr, mine, public }
    }
}

fn print_help() {
    println!("quanta-node — headless Quanta daemon + JSON-RPC\n");
    println!("USAGE:");
    println!("  quanta-node [--data-dir <path>] [--rpc-addr <ip:port>] [--mine]\n");
    println!("OPTIONS:");
    println!("  --data-dir <path>   Data directory (default: <OS data dir>/quanta-protocol)");
    println!("  --rpc-addr <addr>   JSON-RPC bind address (default: 127.0.0.1:8645)");
    println!("  --mine              Enable block production (default: off — watch/relay node)");
    println!("  --public            Read-only mode: disable wallet/broadcast RPC methods so");
    println!("                      the node + web explorer can be safely exposed publicly");
    println!("  -h, --help          Show this help\n");
    println!("ENV:");
    println!("  RUST_LOG                 Log level (info, debug, …)");
    println!("  QUANTA_WALLET_PASSWORD   If set, opens a persistent wallet (unlock/create);");
    println!("                           otherwise the node runs with an ephemeral identity");
}
