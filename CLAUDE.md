# Torus Protocol — CLAUDE.md

> **Version** : V1.0 Production | **Stack** : Rust (Tauri 2.0) + Svelte 5 | **Coin** : QUANTA
> **Status** : ✅ P2P vérifié entre 2 machines physiques (06/05/2026)
> **Repo** : [github.com/nobodyohm-web/Torus](https://github.com/nobodyohm-web/Torus)

## Mission

**Torus est un Web P2P souverain — sans serveur, sans cloud, sans intermédiaire.**

Trois capacités :
1. **Créer** : publiez un site HTML/CSS/JS en 1 clic, hébergé par le réseau P2P
2. **Miner** : gagnez des QUANTA automatiquement en contribuant au réseau
3. **Échanger** : transférez des QUANTA entre wallets avec signatures Ed25519

## Stack

| Couche | Tech | Fichiers |
|--------|------|----------|
| Backend | Rust, Tauri 2.0, Edition 2021 | `src-tauri/src/` |
| Frontend | Svelte 5 (runes), SvelteKit | `src/` |
| CSS | Vanilla CSS, tokens | `src/app.css` |
| P2P transport | Iroh (QUIC), iroh-gossip | `src-tauri/src/p2p/` |
| Consensus | Proof-of-Stake + VRF (BLAKE3) | `p2p/pos_consensus.rs` |
| Crypto | Ed25519 + AES-256-GCM + BLAKE3 + Argon2id | `src-tauri/src/security/` |
| DB | libSQL (turso) | `src-tauri/src/storage/` |
| CRDT | `crdts` crate (PNCounter) | `p2p/consensus.rs` |

## Architecture

```
src-tauri/src/
├── lib.rs                 ← Commandes Tauri (40+ commandes)
├── p2p/
│   ├── mod.rs             ← PeerInfo, exports
│   ├── pos_consensus.rs   ← ⭐ PoS leader election VRF (BLAKE3)
│   ├── reputation.rs      ← Mining engine + trust score
│   ├── ledger.rs          ← Blockchain (seal, validate, fork reorg, O(1) balance cache)
│   ├── ledger_types.rs    ← Block, Transaction, TxType
│   ├── shapley.rs         ← Distribution Shapley (énergie + utilité)
│   ├── consensus.rs       ← CRDT PN-Counters (convergent merge)
│   ├── gossip.rs          ← Protocol gossip (Hello, Tx, Block, Page, Chain sync)
│   ├── gossip_tasks.rs    ← Background tasks (Hello broadcast 60s, trigger_hello_now)
│   ├── dispatcher.rs      ← Message handler (verify → process → dispatch)
│   ├── mining_loop.rs     ← Mine tick 60s + PoS leader seal
│   ├── willow_node.rs     ← Iroh endpoint + stores
│   ├── state_persistence.rs ← SQLite snapshot every 30s (12 stores)
│   ├── page_store.rs      ← Sites P2P (publish + gossip broadcast)
│   ├── domains.rs         ← Registre noms .torus (Harberger tax)
│   ├── search.rs          ← BM25 ranking + QuantaRank
│   ├── social.rs          ← Likes (quadratic), follows, tips
│   ├── moderation.rs      ← Reports + jury VRF + slashing
│   ├── forums.rs          ← Threads DAG + commentaires
│   ├── trust_graph.rs     ← Web of Trust (PageRank personnalisé)
│   ├── energy.rs          ← Oracle énergie (33 pays)
│   ├── sybil.rs           ← Anti-sybil PoC
│   ├── marketplace.rs     ← Tâches compute distribuées
│   └── merkle_dag.rs      ← DAG content-addressed (BLAKE3)
├── security/
│   ├── mod.rs             ← CryptoEngine (Ed25519)
│   ├── pq_vault.rs        ← Identity vault (Argon2id + AES-256-GCM)
│   └── hybrid_crypto.rs   ← Hybrid signatures (Ed25519 + post-quantum ready)
└── storage/               ← libSQL persistence
```

## Consensus — Proof-of-Stake avec VRF

Le consensus est **réel** (pas un simple tie-break par hash) :

```
Slot N (= chain height)
│
├─ VRF seed = BLAKE3(prev_block_hash || slot)
├─ seed % total_weighted_stake → leader déterministe
│
├─ Poids = stake + (reputation × 10_000)
├─ Minimum stake = 1 QUANTA (1M µQTA)
├─ Fallback = 30s timeout → next-in-line
└─ Bootstrap = permissionless si personne n'a staké
```

**Fichier** : `pos_consensus.rs` (9 tests)

### Règles consensus :
- Seul le leader élu scelle les blocs
- Chaque nœud mine (accumule les txs en pending)
- Fork resolution : bloc du leader élu gagne > hash tie-break
- Fallback après timeout : prochain validateur dans la rotation
- Bootstrap mode : si aucun validateur éligible, tous peuvent proposer

## P2P — Iroh QUIC + Gossip

### Flux de connexion :
```
Node A → connect_peer(B_id) → Hello immédiat (chain_height)
Node B ← reçoit Hello → compare chain_height
  Si B.height < A.height → RequestChain → ChainSegment sync
  Si B.height > A.height → A sync de B
Hello périodique toutes les 60s pour liveness
```

### Messages gossip (`GossipMessage`) :
- `Hello { heads, chain_height }` — annonce de présence + hauteur chaîne
- `Transaction { tx_json }` — tx signée (mining, transfer, burn)
- `NewBlock { block_json }` — bloc scellé par le leader PoS
- `RequestChain { from_index }` — demande de sync
- `ChainSegment { blocks_json }` — réponse sync (max 50 blocs)
- `PublishPage { page_json }` — publication web P2P

### Sécurité gossip :
- Chaque envelope = signature Ed25519 + nonce monotone + timestamp (±5 min)
- Anti-replay : dedup par envelope ID
- Rate limiting : max 60 msg/s par peer
- Banning : 3 violations → ban peer

## Blockchain — Ledger

- **µQTA** : 1 QUANTA = 1_000_000 µQTA (integer math, no float drift)
- **Balance cache** : O(1) via HashMap incrémental
- **Anti-replay** : seen_tx_hashes HashSet + nonce monotone par compte
- **Fork reorg** : pop tip → revert cache → re-queue txs → push winner
- **Merkle root** : BLAKE3 tree des tx IDs dans chaque bloc
- **Burn-and-mint** : 1% sur chaque transfert

## Frontend — Svelte 5

| Composant | Rôle |
|-----------|------|
| `Welcome.svelte` | Onboarding (créer identité + mot de passe) |
| `Dashboard.svelte` | Mining rate, balance, peers, chain height, sparkline |
| `Wallet.svelte` | Envoyer/Recevoir/Staker + historique tx paginé |
| `PageBuilder.svelte` | ⭐ Éditeur HTML/CSS/JS + aperçu live + publication P2P |
| `Network.svelte` | Peer ID, connexion, canvas réseau |
| `Browser.svelte` | Navigation pages P2P |
| `Explorer.svelte` | Explorateur de blocs |
| `Forums.svelte` | Threads de discussion |
| `Profile.svelte` | Identité + clé de récupération |
| `Settings.svelte` | Préférences |
| `Sidebar.svelte` | Navigation principale |

## Tests

```
196 tests, 0 failures
├── pos_consensus (9 tests) — leader election, fairness, fallback
├── security_tests (80+ tests) — signatures, replay, nonce, rate limit
├── ledger (20+ tests) — balance, fork, merkle, burn
├── consensus CRDT (3 tests) — merge idempotent
├── integration (10+ tests) — full pipeline
└── autres modules
```

## Build & Run

```bash
# Dev
npm install
npm run tauri dev

# Tests
cargo test --manifest-path src-tauri/Cargo.toml

# Production build (Apple Silicon)
npx tauri build
# → src-tauri/target/release/bundle/dmg/Quanta_0.1.0_aarch64.dmg

# Installation
# 1. Ouvrir le DMG
# 2. Glisser Quanta dans Applications
# 3. xattr -cr /Applications/Quanta.app (contourne Gatekeeper)
# 4. Clic droit → Ouvrir
```

## Constantes clés

```rust
// Mining
const MINE_INTERVAL_SECS: u64 = 60;        // 1 tx/min
const SEAL_EVERY_N_TICKS: u32 = 2;          // seal toutes les 2 min

// Consensus PoS
const MIN_VALIDATOR_STAKE: u64 = 1_000_000; // 1 QUANTA minimum
const LEADER_TIMEOUT_SECS: u64 = 30;        // fallback après 30s
const MAX_FALLBACK_ROUNDS: u32 = 3;         // 3 rounds de fallback

// Persistence
const SNAPSHOT_INTERVAL: Duration = 30s;    // SQLite save toutes les 30s
const MAX_RECENT_TX: usize = 500;           // ring buffer txs récentes

// Gossip
const HELLO_INTERVAL: u64 = 60;             // Hello broadcast toutes les 60s
const MAX_CHAIN_SEGMENT: u64 = 50;          // max blocs par segment sync
```

## Règles Rust absolues

1. `tokio::sync` (JAMAIS `std::sync` avec `.await`)
2. Zéro `unwrap()` — `Result<T,E>` + `?` partout
3. `zeroize()` tous les secrets cryptographiques
4. Ed25519 sur chaque tx et action sociale
5. Lock ordering strict pour éviter deadlocks
6. Tous les montants en `u64` µQTA (jamais f64 pour les balances)

## Historique

| Date | Milestone |
|------|-----------|
| 2026-04-26 | V1 — Infrastructure P2P + Ledger |
| 2026-04-30 | V2 — Sécurité gossip (signatures, nonce, rate limit) |
| 2026-05-01 | V3 — Social web (search, forums, trust_graph) |
| 2026-05-06 | V4 — PoS consensus + chain sync + PageBuilder réel |
| 2026-05-06 | **🎉 Premier test P2P réussi entre 2 machines physiques** |
| 2026-05-06 | v1.0 publiée sur GitHub + Release DMG |
