//! `quanta-node` — the headless Quanta daemon.
//!
//! Runs the full node core (Iroh P2P sync, ledger, Casper-FFG finality) with **no
//! GUI**, and serves a JSON-RPC endpoint for wallets, block explorers and exchange
//! integrations (deposit monitoring, address validation, chain queries).
//!
//! It boots with an **ephemeral in-memory ML-DSA identity**: enough to sign gossip
//! envelopes and participate in the network, but it is **not** a user wallet — no
//! vault, no persisted key, and (mining off by default) it never credits itself. In
//! other words: a safe watch / relay / integration node.
//!
//! ```text
//! quanta-node [--data-dir <path>] [--rpc-addr <ip:port>] [--mine]
//! ```
//! `RUST_LOG` controls logging (e.g. `RUST_LOG=info`).

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

    let state = Arc::new(AppState::new());

    // Ephemeral in-memory ML-DSA identity: lets the node sign gossip envelopes and
    // join the network. NOT a wallet — no vault, no persistence — so it can never
    // spend anyone's funds. (Only ML-DSA is needed on the gossip path per PQ-ENVELOPE-1.)
    {
        let mut crypto = state.crypto.lock().await;
        if let Err(e) = crypto.generate_pq_identity() {
            log::error!("◈ [quanta-node] identité éphémère impossible: {e}");
        }
    }

    node_runtime::bootstrap(&state, cfg.data_dir.clone(), cfg.mine).await;

    log::info!(
        "◈ [quanta-node] démarré — data-dir={:?} rpc=http://{} mine={}",
        cfg.data_dir,
        cfg.rpc_addr,
        cfg.mine
    );

    let shutdown = state.node.shutdown.clone();
    tokio::select! {
        _ = rpc::serve(state.clone(), cfg.rpc_addr, shutdown.clone()) => {}
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
}

impl Config {
    fn from_args() -> Self {
        // Default RPC bind: localhost only. Money RPC is never exposed to the open
        // internet by default; an integrator co-locates the node or tunnels to it.
        let mut data_dir = node_runtime::default_data_dir();
        let mut rpc_addr = SocketAddr::from(([127, 0, 0, 1], 8645));
        let mut mine = false;

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
        Config { data_dir, rpc_addr, mine }
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
    println!("  -h, --help          Show this help\n");
    println!("ENV:");
    println!("  RUST_LOG            Log level (info, debug, …)");
}
