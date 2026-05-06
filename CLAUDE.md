# Torus Protocol — CLAUDE.md

> **Version** : V2.0 Network Hardening | **Stack** : Rust (Tauri 2.0) + Svelte 5 | **Coin** : QUANTA
> **Status** : ✅ P2P vérifié entre 2 machines physiques (06/05/2026)
> **Mission** : Réseau blockchain P2P parfait avec protocole souverain
> **Repo** : [github.com/nobodyohm-web/Torus](https://github.com/nobodyohm-web/Torus)

---

## Mission V2 — Network Perfection

**Torus est un Web P2P souverain — sans serveur, sans cloud, sans intermédiaire.**

La V1 est fonctionnelle. La V2 vise la **perfection réseau** :

### Objectifs V2
1. **Protocole Torus** : Un protocole P2P natif qui remplace les couches ad-hoc par un protocole unifié, versionné, extensible
2. **Échanges parfaits** : Synchronisation deterministe, 0 perte de données, convergence garantie ≤5s
3. **Réseau robuste** : Reconnexion automatique, multi-peer discovery, NAT traversal fiable
4. **Blockchain production** : Consensus PoS stable, fork resolution déterministe, chain sync rapide
5. **Web P2P** : Sites publiés accessibles par tous les peers, résolution `.torus` fonctionnelle

### Trois capacités :
1. **Créer** : publiez un site HTML/CSS/JS en 1 clic, hébergé par le réseau P2P
2. **Miner** : gagnez des QUANTA automatiquement en contribuant au réseau
3. **Échanger** : transférez des QUANTA entre wallets avec signatures Ed25519

---

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

---

## Architecture

```
src-tauri/src/
├── lib.rs                 ← Commandes Tauri (40+ commandes)
├── commands_v3.rs         ← Commandes V3 (social, domains, forums)
├── p2p/
│   ├── mod.rs             ← PeerInfo, exports
│   ├── pos_consensus.rs   ← ⭐ PoS leader election VRF (BLAKE3)
│   ├── reputation.rs      ← Mining engine + trust score
│   ├── ledger.rs          ← Blockchain (seal, validate, fork reorg, O(1) balance cache)
│   ├── ledger_types.rs    ← Block, Transaction, TxType
│   ├── shapley.rs         ← Distribution Shapley (énergie + utilité)
│   ├── consensus.rs       ← CRDT PN-Counters (convergent merge)
│   ├── gossip.rs          ← ⭐ Protocol gossip (Hello, Tx, Block, Page, Chain sync)
│   ├── gossip_tasks.rs    ← Background tasks (Hello broadcast 60s, trigger_hello_now)
│   ├── dispatcher.rs      ← ⭐ Message handler (verify → process → dispatch)
│   ├── mining_loop.rs     ← Mine tick 60s + PoS leader seal
│   ├── willow_node.rs     ← ⭐ Iroh endpoint + stores + gossip topic
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

---

## Protocole Torus — Wire Protocol V2

### Couches du protocole
```
┌──────────────────────────────────┐
│    Application Layer             │ ← Sites, Wallet, Social, Forums
├──────────────────────────────────┤
│    Protocol Layer                │ ← GossipMessage (22 variants)
├──────────────────────────────────┤
│    Security Layer                │ ← GossipEnvelope (Ed25519 + nonce + timestamp)
├──────────────────────────────────┤
│    Transport Layer               │ ← Iroh QUIC + iroh-gossip pub/sub
├──────────────────────────────────┤
│    Network Layer                 │ ← NAT traversal + relay + hole punching
└──────────────────────────────────┘
```

### Messages gossip — Protocole complet

| Message | Direction | Description | Priorité |
|---------|-----------|-------------|----------|
| `Hello` | Broadcast | Présence + chain_height + watts + pays | CRITICAL |
| `RequestChain` | Request | Demande sync chain depuis hauteur X | CRITICAL |
| `ChainSegment` | Response | Réponse sync (max 50 blocs) | CRITICAL |
| `NewBlock` | Broadcast | Bloc scellé par le leader PoS | CRITICAL |
| `BroadcastTx` | Broadcast | Transaction signée (mining, transfer, burn) | HIGH |
| `PublishPage` | Broadcast | Page web P2P single-page | HIGH |
| `PublishSiteManifest` | Broadcast | Manifest site multi-page | HIGH |
| `RequestPage` | Request | Requête page par author_pk | MEDIUM |
| `WantNodes` | Request | Demande nœuds DAG manquants | MEDIUM |
| `HaveNodes` | Response | Nœuds DAG demandés | MEDIUM |
| `Ping` | Request | Liveness check | LOW |
| `Pong` | Response | Réponse liveness | LOW |
| `ReportPeer` | Broadcast | Signalement pair malveillant | LOW |
| `PublishDomain` | Broadcast | Enregistrement domaine .torus | MEDIUM |
| `PublishSubdomain` | Broadcast | Délégation sous-domaine | MEDIUM |
| `PublishSite` | Broadcast | Site indexé pour recherche | MEDIUM |
| `BroadcastSocialAction` | Broadcast | Vote/Follow/Tip/Boost | MEDIUM |
| `BroadcastReport` | Broadcast | Signalement contenu | LOW |
| `BroadcastJurorCommit` | Broadcast | Vote juré (commit phase) | LOW |
| `BroadcastJurorReveal` | Broadcast | Révélation vote juré | LOW |
| `PublishForumNode` | Broadcast | Forum/Thread/Comment | MEDIUM |

### Pipeline de sécurité (dispatch_incoming)
```
Raw bytes
  │ ① Size check (max 10 MB)
  │ ② JSON deserialize → GossipEnvelope
  │ ③ Ban check (per-peer)
  │ ④ Dedup check (seen_messages LRU 100K)
  │ ⑤ Timestamp freshness (±90s)
  │ ⑥ Rate limit (30 msg/min per peer)
  │ ⑦ Nonce anti-replay (per-sender monotonic)
  │ ⑧ Ed25519 signature verification (STRUCT-1 canonical)
  │ ⑨ Payload dispatch → handler
  ▼
Processed
```

---

## Consensus — Proof-of-Stake avec VRF

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

---

## P2P — Iroh QUIC + Gossip

### Flux de connexion
```
Node A → connect_peer(B_id) → Hello immédiat (chain_height)
Node B ← reçoit Hello → compare chain_height
  Si B.height < A.height → RequestChain → ChainSegment sync
  Si B.height > A.height → A sync de B
Hello périodique toutes les 60s pour liveness
Dead peer cleanup toutes les 30s (TTL = 5 min)
```

### Sécurité gossip
- Chaque envelope = signature Ed25519 + nonce monotone + timestamp (±90s)
- Canonical signing: `signable_envelope_bytes(sender, nonce, timestamp, payload)`
- Anti-replay : dedup par envelope ID (LRU 100K)
- Rate limiting : max 30 msg/min par peer
- Banning : 3 reports → ban 1h (auto-expire)
- DoS guard : max 10 MB par envelope, max 50 blocs par ChainSegment

---

## Blockchain — Ledger

- **µQTA** : 1 QUANTA = 1_000_000 µQTA (integer math, no float drift)
- **Balance cache** : O(1) via HashMap incrémental
- **Anti-replay** : seen_tx_hashes HashSet + nonce monotone par compte
- **Fork reorg** : pop tip → revert cache → re-queue txs → push winner
- **Merkle root** : BLAKE3 tree des tx IDs dans chaque bloc
- **Burn-and-mint** : 1% sur chaque transfert
- **Chain sync** : RequestChain → ChainSegment (paginated 50 blocks)

---

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
const PEER_TTL: Duration = 300s;            // dead peer après 5 min sans Hello
const MAX_MSG_PER_WINDOW: u32 = 30;         // rate limit par peer
const RATE_WINDOW_SECS: u64 = 60;           // fenêtre rate limit
const MAX_SEEN_MESSAGES: usize = 100_000;   // LRU dedup
const MAX_RAW_ENVELOPE_BYTES: usize = 10MB; // DoS guard
```

---

## Règles Rust absolues

1. `tokio::sync` (JAMAIS `std::sync` avec `.await`)
2. Zéro `unwrap()` — `Result<T,E>` + `?` partout
3. `zeroize()` tous les secrets cryptographiques
4. Ed25519 sur chaque tx et action sociale
5. Lock ordering strict pour éviter deadlocks
6. Tous les montants en `u64` µQTA (jamais f64 pour les balances)

---

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

---

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

---

## Historique

| Date | Milestone |
|------|-----------|
| 2026-04-26 | V1 — Infrastructure P2P + Ledger |
| 2026-04-30 | V2 — Sécurité gossip (signatures, nonce, rate limit) |
| 2026-05-01 | V3 — Social web (search, forums, trust_graph) |
| 2026-05-06 | V4 — PoS consensus + chain sync + PageBuilder réel |
| 2026-05-06 | **🎉 Premier test P2P réussi entre 2 machines physiques** |
| 2026-05-06 | v1.0 publiée sur GitHub + Release DMG |
| 2026-05-06 | **🔧 V2 Network Hardening — protocole Torus P2P parfait** |
