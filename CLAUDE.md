# Quanta Protocol — CLAUDE.md

> **Marque** : **Quanta** (réseau + app + pièce). Les identifiants techniques/wire hérités
> — TLD `.torus`, `TORUS_PROTOCOL_VERSION`, events Tauri `torus://…` — sont **conservés tels
> quels** pour la compatibilité réseau ; ne pas les renommer sans changement de protocole.
> **Version** : v3.3 (crypto-only) | **Stack** : Rust (Tauri 2.0) + Svelte 5 | **Coin** : QUANTA
> **Licence** : Apache-2.0
> **Status** : ✅ P2P vérifié entre 2 machines physiques (06/05/2026) · ⚠️ alpha, non audité par un tiers
> **Repo** : [github.com/nobodyohm-web/Quanta](https://github.com/nobodyohm-web/Quanta)

---

## Mission — Monnaie souveraine P2P

**Quanta est une monnaie souveraine P2P — une cryptomonnaie sans serveur, sans cloud, sans intermédiaire.**

La mission vise la **perfection réseau** au service d'une seule chose : une monnaie saine,
rare et vérifiable.

### Objectifs
1. **Protocole Torus** : Un protocole P2P natif qui remplace les couches ad-hoc par un protocole unifié, versionné, extensible
2. **Échanges parfaits** : Synchronisation deterministe, 0 perte de données, convergence garantie ≤5s
3. **Réseau robuste** : Reconnexion automatique, multi-peer discovery, NAT traversal fiable
4. **Blockchain production** : Consensus PoS stable, fork resolution déterministe, chain sync rapide
5. **Monnaie vérifiable** : plafond dur 100M gravé dans le code, zéro premine, zéro autorité d'émission

### Trois capacités :
1. **Miner** : gagnez des QUANTA automatiquement en contribuant au réseau (énergie mesurée)
2. **Garder** : votre identité est une clé que vous seul détenez ; on vous joint par un court **`@pseudo`**
3. **Échanger** : transférez des QUANTA entre wallets par `@pseudo`, signés **Ed25519 + ML-DSA-65** (post-quantique), burn 1 %

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
├── lib.rs                 ← Commandes Tauri (≈37 : 30 lib.rs + 7 commands_v3.rs)
├── sm/                    ← ⭐ Cœur sans-IO déterministe (Phase 0, C1) — le consensus pur
│   ├── mod.rs / node.rs   ← Node::handle Event→Effect (horloge + RNG injectés)
│   ├── finality.rs        ← GADGET-1 époque/checkpoint (EPOCH_LENGTH_BLOCKS=32, ADR-009)
│   ├── finality_vote.rs   ← GADGET-2 Vote ML-DSA + certificat ⅔ (MlDsaCertificate)
│   ├── finality_rule.rs   ← GADGET-3 justify/finalize (FinalityState, Casper-FFG)
│   ├── finality_slashing.rs ← GADGET-4 détection de faute (double vote + surround) + preuve + slash
│   ├── fork_choice.rs     ← GADGET-5A LMD-GHOST (ghost_head/anchors) ; 5B = Ledger::reorg_to_fork
│   ├── sim.rs             ← Harnais DST seedé (multi-seed, fautes réseau/byzantines, invariants)
│   └── clock/effect/event/rng ← abstractions injectées
├── p2p/
│   ├── mod.rs             ← PeerInfo, exports
│   ├── pos_consensus.rs   ← ⭐ PoS leader election (élection déterministe, beacon enterré)
│   ├── reputation.rs      ← Mining engine + emission_for_tick (hors chemin de sécurité)
│   ├── ledger.rs          ← Blockchain (seal, validate, fork reorg, O(1) balance cache, genèse PQ)
│   ├── ledger_types.rs    ← Block, Transaction, TxType
│   ├── shapley.rs         ← Distribution Shapley (énergie/travail/validation/uptime)
│   ├── consensus.rs       ← CRDT PN-Counters (convergent merge)
│   ├── gossip.rs          ← ⭐ Protocol gossip (10 variants : + FinalityVote LIVE-1)
│   ├── gossip_tasks.rs    ← Background tasks (Hello broadcast 120s, trigger_hello_now)
│   ├── dispatcher.rs      ← ⭐ Message handler (verify → process → dispatch, étape ⑨ FinalityVote)
│   ├── mining_loop.rs     ← Mine tick 60s + PoS leader seal + cast des votes de finalité (LIVE-1)
│   ├── finality_live.rs   ← ⭐ LIVE-1 : FinalityTracker (câblage IO du gadget) + bridge pubkey↔adresse
│   ├── fork_heal.rs       ← ⭐ LIVE-4 : ForkReconciler (réconciliation de fork profonde — l'appelant réseau de reorg_to_fork)
│   ├── willow_node.rs     ← ⭐ Iroh endpoint + stores + gossip topic
│   ├── state_persistence.rs ← SQLite snapshot every 30s
│   ├── username.rs        ← Registre d'identité @pseudo
│   ├── energy.rs          ← Oracle énergie (33 pays)
│   └── sybil.rs           ← Anti-sybil PoC
├── security/
│   ├── mod.rs             ← CryptoEngine (Ed25519 transport + adresse ML-DSA, ADDR_DOMAIN)
│   ├── pq_vault.rs        ← Identity vault (Argon2id + AES-256-GCM)
│   ├── cipher.rs / crypto_agility.rs ← primitives symétriques + agilité crypto
│   └── hybrid_crypto.rs   ← ⭐ ML-DSA-65 (FIPS 204) — autorité de compte PURE (PQ-MIG-3B) ; Ed25519 = transport
└── storage/               ← libSQL persistence
```

---

## Protocole Torus — Wire Protocol V2

### Couches du protocole
```
┌──────────────────────────────────┐
│    Application Layer             │ ← Wallet, Identité @pseudo, Staking
├──────────────────────────────────┤
│    Protocol Layer                │ ← GossipMessage (9 variants)
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
| `PublishUsername` | Broadcast | Enregistrement d'identité @pseudo | MEDIUM |
| `Ping` | Request | Liveness check | LOW |
| `Pong` | Response | Réponse liveness | LOW |
| `ReportPeer` | Broadcast | Signalement pair malveillant | LOW |

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

## Consensus — Proof-of-Stake, élection vérifiable pondérée par le stake

```
Slot N (= chain height)
│
├─ beacon = BLAKE3(domaine ‖ bloc_enterré_hash ‖ slot)   (enterré = LOOKBACK slots derrière le tip)
├─ seed   = BLAKE3(domaine ‖ beacon ‖ slot ‖ round)
├─ seed % total_weighted_stake → leader déterministe
│
├─ Poids = stake **inscrit sur la chaîne** (`ledger.validator_stakes()` — ADR-002 ;
│   réputation hors chemin de sécurité ; source = état du ledger, identique sur tous les nœuds)
├─ Minimum stake = 1 QUANTA (1M µQTA)
├─ Fallback = 30s timeout → next-in-line
└─ Bootstrap = permissionless si personne n'a staké
```

> **Enjeu on-chain (ONCHAIN-STAKE-1).** Le poids du validateur **n'est plus** lu du
> leaderboard local mais d'un **état d'enjeu dans le ledger**, dérivé des tx `Stake`/`Unstake`
> scellées (ancrage à `block.index`) — donc une **fonction pure de la chaîne**, identique sur
> chaque nœud (live / restauré / synchronisé). C'est la seconde moitié de la fermeture du
> vecteur de fork (la première — la réputation dans le poids — l'a été par STAKE-WEIGHT-1).
> Le solde se scinde en **dépensable / staké / en-déverrouillage** ; staker **déplace** des
> pièces (ne les brûle pas), donc la conservation compte
> `Σ(dépensable + staké + déverrouillage) + brûlé == miné`. Déverrouillage **indexé par
> hauteur** (`unlock = block.index + UNBONDING_PERIOD_BLOCKS = 10_080`, **ratifié ajustable par
> ADR-009** ; contrainte gravée `≥ fenêtre de slashing` garantie par const-assert dans
> `sm/finality_slashing.rs`, ADR-003).

> **Nommage honnête** : l'*élection du proposeur* est *déterministe et publiquement vérifiable*,
> **pas** un VRF cryptographique (aucune clé secrète → leader publiquement prévisible). Le beacon
> enterré bloque l'auto-grinding immédiat ; un vrai VRF (imprévisibilité) + un VDF (anti-grinding)
> sont au roadmap. Le **slashing de l'équivocation** est **vivant** (LIVE-3) : détecté et prouvable
> dans le cœur (`sm/finality_slashing.rs`, GADGET-4 : double vote + surround, preuves ML-DSA) **et
> appliqué sur le ledger réel** — une tx `Slash` (autorisée par la preuve embarquée, re-vérifiée par
> chaque nœud) détruit l'enjeu de l'offenseur STAKE→BURN, conservation neutre. Les identifiants
> internes `vrf` sont des noms legacy gardés pour la compat.

**Fichier** : `pos_consensus.rs` (16 tests). Le **gadget de finalité** (Casper-FFG) qui vient
au-dessus de cette élection vit dans `sm/` — voir la section suivante.

---

## Gadget de finalité (Casper-FFG, GADGET-1→5B) — `sm/`

Au-dessus de l'élection PoS, un **gadget de finalité de type Casper-FFG** rend l'histoire
**irréversible** — quelque chose que Bitcoin n'a pas. Écrit **pur et déterministe** (`sm/`,
sans-IO, C1), **prouvé en simulation DST**, et depuis **LIVE-1** ses votes circulent en vivant.

```
Époque = E blocs (E = 32, ADR-009)
│
├─ GADGET-1 checkpoint(hauteur, hash) à chaque frontière d'époque       (finality.rs)
├─ GADGET-2 Vote ML-DSA-65 (source→target) + certificat ⅔ du stake      (finality_vote.rs)
│            quorum gravé : backing×3 ≥ total×2 (QUORUM_NUM/DEN)
├─ GADGET-3 justify puis finalize (2 liens consécutifs) → FinalityState  (finality_rule.rs)
├─ GADGET-4 accountable safety : détecte double-vote + surround,          (finality_slashing.rs)
│            preuve ML-DSA non-répudiable, slash (brûlé, plein, fenêtre=unbonding)
└─ GADGET-5 fork-choice LMD-GHOST pondéré par le stake, ancré finalité   (fork_choice.rs)
             (5A ghost_head/anchors) ; réconciliation de partition = Ledger::reorg_to_fork (5B)
```

- **Identité de vote = clé publique ML-DSA** ; l'enjeu est address-keyed (`validator_stakes()`).
  Le pont `Ledger::validator_stakes_by_pubkey()` re-clé l'enjeu **purement depuis la chaîne**
  (chaque tx `Stake` révèle sa `pq_public_key`) — le total pesé = vrai poids staké.
- **PQ pur (ADR-005)** : aucune primitive classique sur le chemin de l'irréversibilité ; les
  votes sont signés ML-DSA-65, jamais Ed25519.
- **Déterminisme** : tout verdict est une fonction pure (BTreeMap/BTreeSet ordonnés) — deux
  nœuds aux mêmes votes + même chaîne finalisent **identiquement** (la propriété que C1 garde).

> **Câblage vivant (`DESIGN-LIVE-WIRING.md`) — LIVE-1→4 + 3B FAITS.**
> - **LIVE-1 (votes)** — `GossipMessage::FinalityVote` + bras dispatcher (étape ⑨) + `FinalityTracker`
>   (`p2p/finality_live.rs`) + cast au tick de mining ; pont `validator_stakes_by_pubkey` (enjeu re-clé
>   depuis la chaîne) ; les votes gossippés peuplent `LatestVotes`/`FinalityState` du ledger vivant.
> - **LIVE-2 (plancher de finalité)** — `Ledger::finalized_floor_index` (monotone, **vérifié par hash**,
>   persisté au snapshot) alimenté par les certificats ⅔ ; `integrate_remote_block` **refuse** tout fork qui
>   remplacerait un bloc ≤ plancher (l'histoire finalisée est **irréversible** sur le réseau vivant ;
>   le départage lexicographique libre ne joue qu'**au-dessus** du plancher — Gasper). Garde de sûreté
>   pure : refuser un reorg ne mute aucun solde.
> - **LIVE-3 (slashing vivant)** — équivocation détectée à l'ingest (`detect_fault`) → `FinalityFault`
>   gossipé → tx `Slash` (autorité = preuve embarquée, re-vérifiée par `verify_block_slashes` sur chaque
>   nœud) qui détruit l'enjeu de l'offenseur **STAKE→BURN**, **conservation neutre** (l'enjeu et le
>   brûlé sont deux compartiments de `Σ(dépensable+staké+déverr.)+brûlé==miné`). Un proposeur malveillant
>   ne peut pas punir un innocent (preuve réelle + adresse offenseur + montant = fraction ratifiée).
> - **LIVE-3B (le slash atteint l'unbonding — « unstake-and-run » fermé, audit 837)** — base slashable =
>   `staké + en-déverrouillage` (sémantique Casper : punissable tant que le retrait n'est pas complété).
>   La tx `Slash` **porte sa ventilation de consommation** (`slash_unbonding` : entrées détruites, ordre
>   déterministe `(unlock_height, tx_hash)`), liée au hash **et** au Merkle — chaque nœud la re-vérifie
>   contre son **propre plan** (`expected_slash_consumption`, source unique build+verify) et un reorg
>   restaure **exactement** les entrées consommées (montant + hauteur + tx d'origine). Deux cartes, deux
>   rôles : `validator_stakes_by_pubkey` = poids de **vote** (bondé seul) ; `slashable_stakes_by_pubkey`
>   = poids **punissable** (bondé + unbonding) pour `verify_proof` sur les chemins de slash.
> - **LIVE-4 (réconciliation de fork profonde — l'appelant réseau de GADGET-5B)** — `p2p/fork_heal.rs` :
>   `ForkReconciler`, tampon borné (1024, éviction déterministe du plus haut index) nourri des blocs qui
>   échouent l'intégration linéaire ; assemble la branche concurrente enracinée chez nous, **règle de
>   victoire vivante** = plus-longue-au-dessus-du-plancher + départage lexicographique du tip à hauteur
>   égale (généralisation N-blocs de la règle 1-bloc existante — exactement un côté adopte, convergence
>   symétrique) ; applique via `reorg_to_fork` (validation complète sur clone d'essai, plancher absolu) ;
>   sonde l'ancêtre commun par fenêtres `RequestChain` descendantes (bornées au plancher). Sans nouveau
>   message wire ; guérit aussi les fenêtres ChainSegment hors-ordre (NET-6). Deux partitions qui
>   scellent chacune ≥2 blocs convergent désormais en vivant — le trou de convergence est fermé.
>
> Le cœur `sm/` reste inchangé (aucune règle nouvelle) ; C1 + conservation + sweep multi-seed verts.

**Fichiers** : `sm/finality*.rs` + `fork_choice.rs` (47 tests) · `p2p/finality_live.rs` (25 tests LIVE-1→3B
+ grappe d'audit) · `p2p/fork_heal.rs` (8 tests LIVE-4) · plancher + slash dans `p2p/ledger.rs` (teeth)

---

## P2P — Iroh QUIC + Gossip

### Flux de connexion
```
Node A → connect_peer(B_id) → Hello immédiat (chain_height)
Node B ← reçoit Hello → compare chain_height
  Si B.height < A.height → RequestChain → ChainSegment sync
  Si B.height > A.height → A sync de B
Hello périodique toutes les 120s (+ Ping léger 15s, NET-4) pour liveness
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
- **NET-5** : `TORUS_PROTOCOL_VERSION = 4` (2→3 par PQ-MIG-5 : genèse PQ ; 3→4 par LIVE-3B : règles de consensus des slashes-unbonding — la genèse et l'historique existant restent inchangés) ; peers incompatibles loggués
- **NET-6** : chain sync parallèle (fanout = 4 fenêtres × 50 blocs)
- **NET-7** : ~~DAG sync incrémental~~ — retiré avec les modules web (le DAG de contenu social n'existe plus ; sans rapport avec le futur consensus DAG-BFT)
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
- **Couverture symétrique (COVER-1 réception / COVER-2 production)** : règle de couverture unique
  (`uncovered_tx_indices` + `onchain_spendable_before`, solde **on-chain** avant le bloc, fonction
  pure de la chaîne, jamais le mempool ; séquentielle ; crédits intra-bloc comptés ; synthétiques
  `NETWORK`/`ESCROW`/`BURN` exemptés). **COVER-1 — réception** : `validate_block_against_prev`
  (validateur **partagé** intégration linéaire **et** reorg) **rejette** tout bloc reçu avec une
  dépense/stake non couvert. **COVER-2 — production** : `seal_block_at` **exclut** les tx non
  couvertes (revert cache + éviction) pour produire un bloc **valide par construction** — invariant :
  tout bloc auto-scellé passe `validate_block_against_prev` (un nœud ne corrompt plus sa chaîne). Le
  clamp `.max(0)` est **conservé** (cache pending-inclus via `replay_remote_tx` sans garde + sûreté
  du cast `i128→u64`).

---

## Constantes clés

```rust
// Mining
const MINE_INTERVAL_SECS: u64 = 60;        // 1 tx/min
const SEAL_EVERY_N_TICKS: u32 = 2;          // seal toutes les 2 min

// Consensus PoS
const MIN_VALIDATOR_STAKE: u64 = 1_000_000; // 1 QUANTA min (classe ajustable ratifiée ADR-009 ; valeur = placeholder d'Alexandre)
const LEADER_TIMEOUT_SECS: u64 = 30;        // fallback après 30s
const MAX_FALLBACK_ROUNDS: u32 = 3;         // 3 rounds de fallback
const UNBONDING_PERIOD_BLOCKS: u64 = 10_080;// 🛑 ~2 sem. de blocs ; ≥ fenêtre de slashing (ADR-003)

// Persistence
const SNAPSHOT_INTERVAL: Duration = 30s;    // SQLite save toutes les 30s
const MAX_RECENT_TX: usize = 500;           // ring buffer txs récentes

// Gossip
const HELLO_INTERVAL_SECS: u64 = 120;       // Hello broadcast 120s (+ Ping léger 15s, NET-4)
const MAX_CHAIN_SEGMENT: u64 = 50;          // max blocs par segment sync
const PEER_TTL: Duration = 300s;            // dead peer après 5 min sans Hello
const BASE_MSG_PER_WINDOW: u32 = 30;        // base adaptative NET-13 (plancher MIN=15, plafond MAX=120)
const RATE_WINDOW_SECS: u64 = 60;           // fenêtre rate limit
const MAX_SEEN_MESSAGES: usize = 100_000;   // LRU dedup
const MAX_RAW_ENVELOPE_BYTES: usize = 10MB; // DoS guard
```

---

## Règles Rust absolues

1. `tokio::sync` (JAMAIS `std::sync` avec `.await`)
2. Zéro `unwrap()` — `Result<T,E>` + `?` partout
3. `zeroize()` tous les secrets cryptographiques
4. Autorité de tx = **ML-DSA** (clé liée à l'adresse `from` via `lie`, PQ-MIG-3B) ; Ed25519 = **transport** (chaque enveloppe gossip) + co-facteur tx vestigial
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
# → src-tauri/target/release/bundle/dmg/Quanta_3.3.0_aarch64.dmg

# Installation
# 1. Ouvrir le DMG
# 2. Glisser Quanta dans Applications
# 3. xattr -cr /Applications/Quanta.app (contourne Gatekeeper)
# 4. Clic droit → Ouvrir
```

---

## Tests

```
413 tests lib + 1 intégration, 0 failures
├── sm/ (cœur sans-IO déterministe) — gadget de finalité + harnais DST :
│     finality / finality_vote / finality_rule / finality_slashing / fork_choice
│     (GADGET-1→5B), node (Event→Effect), sim (DST multi-seed, C1 128-runs,
│     sweeps slashing t0_8)
├── finality_live (25 tests) — LIVE-1→3 (bridge, ingest, cast, plancher, slash)
│     + LIVE-3B (slash sur unbonding, ventilation, reorg exact) + grappe d'audit
├── fork_heal (8 tests) — LIVE-4 : heal de partition symétrique, départage,
│     plancher, hors-ordre, branche invalide purgée, tampon borné, sondes
├── pos_consensus (16 tests) — leader election, fairness, fallback
├── security_tests (41 tests) — signatures, replay, nonce, rate limit
├── ledger (72 tests) — balance, fork, merkle, burn, AUDIT-TX/BLK, ONCHAIN-STAKE, COVER, PQ-MIG-5, LIVE-2/3
├── consensus CRDT (5 tests) — merge idempotent
├── integration_tests — paginated chain sync, AUDIT-SYNC compression
├── shapley — distribution énergie/travail/validation/uptime (somme = 1.0)
├── reputation — mining engine + trust score
├── username — registre d'identité @pseudo
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

### HIGH bugs corrigés
- **AUDIT-BLK-1** : la résolution de fork perdait silencieusement les txs
  exclusives à la branche perdante. Le garde `!seen_tx_hashes.contains`
  était toujours false. Désormais on calcule l'ensemble des hashes du
  bloc gagnant et on remet en pending uniquement les txs absentes de
  celui-ci.
- **AUDIT-BLK-2** : la résolution de fork popait le tip AVANT validation —
  un bloc malformé tronquait la chaîne d'un cran à jamais. Validation
  extraite en `validate_block_against_prev` et exécutée pre-mutation.

### MEDIUM bugs corrigés
- **AUDIT-TX-3** : `transfer_with_burn` ne pré-vérifiait que le NET. Si
  `balance == net`, le burn poussait silencieusement le cache en négatif
  (saturé par `balance_of`). Pre-check du gross désormais explicite.
- **AUDIT-SYNC-1** : `handle_chain_segment` ne s'arrêtait pas au premier
  bloc rejeté. Sur un trou dans le segment, tous les blocs suivants
  échouaient inutilement. `break` sur première erreur.

### Tests de régression ajoutés
- ledger : audit_tx1/3/blk1/2/cross_ledger_convergence/out_of_order
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
| 2026-06-20 | **₿ Refonte crypto-only — suppression des modules web/social (sites, domaines, recherche, social, forums, modération, marketplace, DAG) ; Shapley sans terme social (énergie 30 / travail 30 / validation 25 / uptime 15) — 174 tests** |
| 2026-06-23 | **⚖️ Enjeu on-chain (ADR-002 complet) — STAKE-WEIGHT-1 (réputation retirée du poids) puis ONCHAIN-STAKE-1 (état d'enjeu dans le ledger : tx Stake/Unstake, déverrouillage indexé par hauteur, `build_validator_set` sourcé de la chaîne) ; vecteur de fork fermé, conservation `Σ(dépensable+staké+déverrouillage)+brûlé==miné` — 289 tests** |
| 2026-06-23 | **🛡️ COVER-1 — validation de couverture au bloc : `validate_block_against_prev` (validateur partagé des deux chemins) rejette toute dépense/stake non couvert par le solde on-chain ; couverture séquentielle + crédits intra-bloc ; clamp `.max(0)` conservé (§4 « ne force pas ») ; dernier trou de validation fermé avant le gadget — 298 tests** |
| 2026-06-23 | **🛡️ COVER-2 — couverture **symétrique** au seal : `seal_block_at` **exclut** les tx non couvertes (même règle que COVER-1, source unique `uncovered_tx_indices`) + revert cache + éviction ⇒ bloc **valide par construction** ; invariant « bloc auto-scellé passe la validation » (auto-corruption locale fermée) ; clamp/admission inchangés — 306 tests** |
| 2026-06-25 | **🔐 PQ-MIG-3B — identité de compte **entièrement ML-DSA, sans astérisque** (ADR-007 b réalisé ; ADR-008 reversé) : `from`/`to` = **adresse ML-DSA** (`BLAKE3(ADDR_DOMAIN ‖ clé)`) **partout** — solde, récompense (`mine_tx`), enjeu/`validator_stakes`, `@pseudo` (`owner_pk` + `owner_key` révélée, signé ML-DSA + `lie`) ; autorité de `verify_tx` = **pur ML-DSA** (co-facteur Ed25519 retiré du chemin), CRYPTO-ID-1 close **par construction** ; **transport Ed25519 différé** (enveloppes/PeerId intacts) ; conservation/couverture/**C1** verts — 335 tests** |
| 2026-06-25 | **⚖️ Gadget de finalité complet GADGET-1→5B (`sm/`) — checkpoints par époque (E=32) · votes ML-DSA + certificat ⅔ gravé · règle justify/finalize (Casper-FFG) · slashing détecté & prouvable (double-vote + surround) · fork-choice LMD-GHOST pondéré stake, ancré finalité + réconciliation de partition (`reorg_to_fork`) ; prouvé en simulation DST, C1 vert** |
| 2026-06-25 | **🔐 PQ-MIG-5 — genèse post-quantique : état initial reconstruit sur adresses ML-DSA + validateurs initiaux, hash de genèse canonique content-bound (frozen), conservation exacte dès le bloc 0 ; `TORUS_PROTOCOL_VERSION` bumpé 2→3 (rupture de protocole) — 335→374 tests** |
| 2026-06-25 | **📜 ADR-005/006/007/009 — agrégation PQ des votes (ADR-005) ; frontière gravé/ajustable ratifiée (ADR-006 par ADR-009) ; comptes ML-DSA réalisés (ADR-007 b) ; §12 figé (E=32, quorum ⅔, unbonding 10 080, slash brûlé/plein/fenêtre=unbonding) ; conception du câblage vivant (`DESIGN-LIVE-WIRING`)** |
| 2026-07-12 | **🔌 LIVE-1 — câblage vivant du gadget : `GossipMessage::FinalityVote` + bras dispatcher (étape ⑨) + `FinalityTracker` (`p2p/finality_live.rs`) + cast au tick de mining ; pont `validator_stakes_by_pubkey` (enjeu re-clé depuis la chaîne) ; les votes gossippés peuplent `LatestVotes`/`FinalityState` du ledger vivant — cœur `sm/` inchangé, IO testée à part, C1 préservé — 379 tests** |
| 2026-07-12 | **🔒 LIVE-2 — plancher de finalité vivant : `Ledger::finalized_floor_index` (monotone, tip-clampé, persisté) alimenté par les certificats ⅔ ; `integrate_remote_block` refuse tout fork ≤ plancher (histoire finalisée irréversible sur le réseau ; départage libre au-dessus, Gasper). Garde de sûreté pure — aucun solde muté — 384 tests** |
| 2026-07-12 | **⚔️ LIVE-3 — slashing vivant : équivocation détectée à l'ingest → `FinalityFault` gossipé → tx `Slash` (autorité = preuve embarquée, re-vérifiée par `verify_block_slashes`) détruisant l'enjeu de l'offenseur **STAKE→BURN**, **conservation neutre par construction** ; un proposeur ne peut punir un innocent. `TxType::Slash` + accounting + verify + producteur→gossip→apply — C1 + conservation + sweep verts — 388 tests. **Le câblage vivant du gadget est complet (LIVE-1→3).**** |
| 2026-07-12 | **🛡️ Grappe « cycle de vie du slash » (audit exhaustif) — 4 corrigées + 3 évaluées : TTL-exemption du `Slash` (788, le slashing devenait inopérant en vif), éviction du slash pending redondant à l'application du bloc (2318), jamais re-mis en file au reorg (2450), garde par-offenseur dans `queue_slash` (911) ; 2396 réfutée, 2359 déjà mitigée (veto plancher), 837 → LIVE-3B — 399 tests + 1 intégration** |
| 2026-07-13 | **🌐 LIVE-4 — réconciliation de fork profonde en vivant : `p2p/fork_heal.rs` (`ForkReconciler`, tampon borné 1024 + éviction déterministe) nourri des blocs qui échouent l'intégration linéaire ; règle de victoire = plus-longue-au-dessus-du-plancher + départage lexicographique (généralisation N-blocs) ; application via `reorg_to_fork` (clone d'essai, plancher absolu) ; sondes d'ancêtre par `RequestChain` descendantes ; guérit aussi les fenêtres hors-ordre (NET-6). **Deux partitions ≥2 blocs convergent désormais** — le trou de convergence (GADGET-5B jamais appelé du réseau) est fermé — 407 tests + 1 intégration** |
| 2026-07-13 | **⚔️ LIVE-3B — le slash atteint l'unbonding (« unstake-and-run » fermé, audit 837) : base slashable = staké + en-déverrouillage (sémantique Casper) ; la tx `Slash` porte sa **ventilation de consommation** (`slash_unbonding`, ordre déterministe, liée hash + Merkle) re-vérifiée par chaque nœud contre son propre plan (`expected_slash_consumption`) ; un reorg restaure **exactement** les entrées consommées ; deux cartes d'enjeu (vote = bondé ; slashable = bondé + unbonding pour `verify_proof`). Slashes bondés byte-identiques à avant (zéro dérive wire, C1 vert) — 413 tests + 1 intégration** |
