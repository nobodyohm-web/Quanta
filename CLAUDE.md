# Quanta Protocol — CLAUDE.md

> **Marque** : **Quanta** (réseau + app + pièce). Les identifiants techniques/wire hérités
> — TLD `.torus`, `TORUS_PROTOCOL_VERSION`, events Tauri `torus://…` — sont **conservés tels
> quels** pour la compatibilité réseau ; ne pas les renommer sans changement de protocole.
> **Version** : v3.3 (Site Engine) | **Stack** : Rust (Tauri 2.0) + Svelte 5 | **Coin** : QUANTA
> **Licence** : Apache-2.0
> **Status** : ✅ P2P vérifié entre 2 machines physiques (06/05/2026) · ⚠️ alpha, non audité par un tiers
> **Repo** : [github.com/nobodyohm-web/Torus](https://github.com/nobodyohm-web/Torus)

---

## Mission V2 — Network Perfection

**Quanta est un Web P2P souverain — sans serveur, sans cloud, sans intermédiaire.**

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
| Crypto | **Ed25519 + ML-DSA-65 (FIPS 204)** + AES-256-GCM + BLAKE3 + Argon2id | `src-tauri/src/security/` |
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
│   └── hybrid_crypto.rs   ← ⭐ Signatures hybrides Ed25519 + ML-DSA-65 (FIPS 204, actif)
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
- Rate limiting **adaptatif** (NET-13) : `sqrt(peers/4) × 30 msg/min` clamped to [15, 120]
- Banning : 3 reports → ban 1h (auto-expire)
- DoS guard : max 10 MB par envelope, max 50 blocs par ChainSegment
- Eclipse warning (NET-12) : >80% des peers partageant le même préfixe pubkey 8-hex → warning

### Network V2 hardening (NET-3 → NET-16)
- **NET-3** : priority queue sortante 4-lanes (Critical/High/Medium/Low)
- **NET-4** : Hello 120s + Ping 15s léger pour la liveness
- **NET-5** : `TORUS_PROTOCOL_VERSION = 2` ; peers incompatibles loggués
- **NET-6** : chain sync parallèle (fanout = 4 fenêtres × 50 blocs)
- **NET-7** : DAG sync incrémental — skip WantNodes si heads inchangées
- **NET-8** : ChainSegment gzip optionnel (50 MB inflate cap)
- **NET-9** : RTT (EWMA), bytes_in/messages_in par peer
- **NET-10** : score qualité 0-100 (latency 50pts + loss 30pts + uptime 20pts)
- **NET-11** : `get_network_topology` 2-hop view
- **NET-12** : eclipse heuristic (80% prefix collision)
- **NET-13** : rate limit adaptatif sqrt-scaling
- **NET-14** : mempool TTL 10 min + cap 1000 tx
- **NET-15** : peer nicknames signés (`display_name` 32 chars)
- **NET-16** : events Tauri `torus://chain-sync-progress` pour la barre de sync

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
243 tests, 0 failures
├── pos_consensus (9 tests) — leader election, fairness, fallback
├── security_tests (80+ tests) — signatures, replay, nonce, rate limit
├── ledger (20+ tests) — balance, fork, merkle, burn, AUDIT-TX/BLK
├── consensus CRDT (3 tests) — merge idempotent
├── integration_tests — paginated chain sync, AUDIT-SYNC compression
├── page_store (16 tests) — single-page + multi-page, AUDIT-PAGE
├── domains — claim, overbid, harberger, AUDIT-DOM round-trip
├── social — quadratic likes, follow tiers, boost cap, AUDIT-SOC
├── forums — forum/thread/comment hierarchy, AUDIT-FORUM round-trip
├── moderation (12 tests) — report/commit/reveal, AUDIT-MOD restart
├── search — TF-IDF + social signals, AUDIT-SEARCH author binding
├── trust_graph (8 tests) — personalised PageRank, AUDIT-TRUST
└── autres modules
```

---

## Audit P2P V3.3 — corrections appliquées (2026-05-07)

### CRITICAL bugs corrigés
- **AUDIT-TX-1** : `verify_tx` exemptait les txs avec `to == "BURN"` de la
  vérification de signature. N'importe quel pair pouvait forger
  `from=victim, to=BURN` et drainer une victime via gossip. Maintenant
  seules les adresses synthétiques `NETWORK` et `ESCROW` sont exemptes.
- **AUDIT-TX-2** : `transfer_with_burn` produisait un 2ᵉ tx (le burn 1%) que
  `ledger_transfer` ne diffusait jamais — chaque pair distant ratait 1% du
  débit, divergence du solde sur tout le réseau. La fonction renvoie
  désormais `Option<burn_tx>` (signé) et `ledger_transfer` diffuse les deux
  branches. Le contrôle de nonce strict du dispatcher est aussi relâché en
  monotonic-non-regression pour ne pas perdre les arrivées hors-ordre.
- **AUDIT-PAGE-1** : `page_store::publish()` acceptait `signature == "unsigned"`
  ou vide. Atteignable via gossip — n'importe quel pair pouvait imposer
  du contenu pour le wallet d'une victime. Corrigé : exigence d'une
  signature Ed25519 valide ; helper `publish_local_unsigned` relégué à
  `#[cfg(test)]`.
- **AUDIT-SOC-1** : toutes les commandes V3 (`social_vote`, `social_tip`,
  `social_boost`, `claim_domain`, `pay_domain_rent`, `overbid_domain`,
  `submit_moderation_report`) mutaient le ledger sans diffuser le tx
  sous-jacent — divergence systématique des soldes. Helper
  `broadcast_ledger_txs()` capture les txs fraîchement créés et envoie
  chacune comme `BroadcastTx`.
- **AUDIT-SEARCH-1** : `handle_publish_site` ne liait pas `IndexedDoc.author_pk`
  au `sender` de l'enveloppe. Un pair pouvait polluer l'index avec des docs
  spoofés. Maintenant on rejette tout doc dont l'`author_pk` ne correspond
  pas à l'expéditeur signé.

### HIGH bugs corrigés
- **AUDIT-BLK-1** : la résolution de fork perdait silencieusement les txs
  exclusives à la branche perdante. Le garde `!seen_tx_hashes.contains`
  était toujours false. Désormais on calcule l'ensemble des hashes du
  bloc gagnant et on remet en pending uniquement les txs absentes de
  celui-ci.
- **AUDIT-BLK-2** : la résolution de fork popait le tip AVANT validation —
  un bloc malformé tronquait la chaîne d'un cran à jamais. Validation
  extraite en `validate_block_against_prev` et exécutée pre-mutation.
- **AUDIT-MOD-1** : `pending_reports` (queue de reports sous le seuil de 5)
  n'était pas snapshotée. Un redémarrage effaçait tous les reports en
  cours d'accumulation. `ModerationSnapshot` les persiste désormais avec
  `#[serde(default)]`.

### MEDIUM bugs corrigés
- **AUDIT-TX-3** : `transfer_with_burn` ne pré-vérifiait que le NET. Si
  `balance == net`, le burn poussait silencieusement le cache en négatif
  (saturé par `balance_of`). Pre-check du gross désormais explicite.
- **AUDIT-SYNC-1** : `handle_chain_segment` ne s'arrêtait pas au premier
  bloc rejeté. Sur un trou dans le segment, tous les blocs suivants
  échouaient inutilement. `break` sur première erreur.

### Tests de régression ajoutés
- ledger : audit_tx1/3/blk1/2/cross_ledger_convergence/out_of_order
- page_store : audit_page1_*
- domains : audit_dom_*
- social : audit_soc_*
- forums : audit_forum_*
- moderation : audit_mod1_pending_reports_persist
- search : audit_search_*
- trust_graph : audit_trust_*
- integration_tests : audit_sync_*

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
| 2026-05-07 | **🛡️ Audit P2P complet — 5 critiques + 3 high + 2 medium corrigés (243 tests)** |
| 2026-05-07 | **🧱 Site Engine v3.3 — smart tags, no-code builder, dev HTTP API (256 tests)** |
| 2026-05-31 | **🔐 Post-quantique hybride ACTIF (ML-DSA-65/FIPS 204, dérivé de la graine Ed25519) + invariants formels (proptest) + aléa d'élection non-grindable (beacon enterré) — 265 tests** |

---

## Site Engine v3.3 — Smart Tags · No-Code Builder · Dev API

### Phase 1 — Smart Tags + auto-indexation (`p2p/search.rs`, `lib.rs`)
- Champ `tags: Vec<String>` (`#[serde(default)]`) sur `IndexedDoc`,
  `PublishedPage`, `SiteManifest`. Rétro-compat des snapshots existants.
- `auto_extract_tags(html, title)` : strip HTML → tokenize → top-5 tokens
  (bonus ×3 sur les mots du titre). `sanitize_tag/sanitize_tags`
  normalisent (lowercase, alphanum + tirets, max 30 chars, max 10 tags,
  dédup).
- `SearchIndex::search` : multiplicateur `TAG_BOOST=2.0` quand un tag du
  doc matche un token de la query, et nouveau filtre exact
  `SearchFilters.tag`.
- **Bug critique corrigé** : `publish_page` (lib.rs) auto-indexe désormais
  le site dans le `SearchIndex` local et broadcast `PublishSite` vers les
  pairs. Avant ce fix, les sites publiés via PageBuilder n'étaient
  **jamais** trouvables via la recherche réseau.
- `index_my_page` et `publish_site` (commands_v3) acceptent un paramètre
  `tags: Option<Vec<String>>` ; auto-extraction si absent.
- `search_pages` accepte un filtre `tag: Option<String>`.
- 8 nouveaux tests unitaires (`auto_extract`, `sanitize`, `tag_boost`,
  `tag_filter`, `strip_html`, sérialisation legacy).

### Phase 2 — No-Code Block Builder (`src/lib/PageBuilder.svelte`)
- Réécriture complète en builder visuel à blocs (style Notion/Wix).
  Svelte 5 runes (`$state`/`$derived`/`$effect`), vanilla CSS,
  **zéro dépendance externe**, drag-drop HTML5 natif.
- 20 types de blocs : heading (H1-H3), paragraph, quote, list, code,
  image (file picker → base64), gallery, video, columns (2/3),
  spacer (S/M/L), divider, hero, cards (2-6), feature, faq, callout,
  navbar, footer, button, embed (HTML brut sanitisé).
- 5 thèmes (Minimal, Dark Pro, Ocean, Warm, Glass) — variables CSS
  injectées dans l'iframe sandboxée.
- 5 templates pré-remplis (Landing, Blog, Portfolio, Boutique, Page perso),
  contenu placeholder en français.
- Top bar : titre, domaine `.torus`, pill-tags (Entrée/×, max 10),
  suggestions auto-générées via TF du contenu, sélecteur de thème,
  popup Méta (langue/cat/desc), toggle Code, Publier.
- Édition inline (contenteditable), toolbar de bloc au hover, handle
  drag `⋮⋮`, bouton `+` entre chaque bloc avec modal catégorisé.
- Bouton Publier appelle `invoke("publish_page", { title, content, tags })`
  avec tags manuels ou suggérés (intégration Phase 1).

### Phase 3 — Dev HTTP API (`src-tauri/src/dev_api.rs`)
- Serveur HTTP local `127.0.0.1:7654` (loopback only, double-check),
  parser HTTP minimal sur `tokio::net::TcpListener` — **0 nouvelle dep**.
- Auth Bearer obligatoire ; token 32 bytes hex persisté dans
  `~/.torus/dev-api-token` (régénérable depuis Settings).
- Désactivé par défaut. Toggle dans **Réglages → API Développeur**
  (fichier sentinel `~/.torus/dev-api-enabled`). Quand off, toutes les
  routes (sauf `/api/health`) renvoient `503`.
- Endpoints :
  - `GET /api/health` (sans auth) — ping disponibilité.
  - `GET /api/status` — `{ pk, balance_qta, sites_count, search_docs }`.
  - `POST /api/publish` — signe + store + broadcast + auto-index.
    Champs : `title`, `html`, `tags?`, `lang?`, `kind?`, `domain?`.
  - `GET /api/search?q=&lang=&tag=&limit=` — recherche P2P.
  - `DELETE /api/site` — dépublie le site courant.
- Doc complète + workflow VSCode dans [`DEV_API.md`](DEV_API.md).
- 5 nouveaux tests (token roundtrip, parse_query, url_decode,
  http_response format, find_header_end).
