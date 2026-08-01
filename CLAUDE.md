# Quanta Protocol — CLAUDE.md

> **Marque** : **Quanta** (réseau + app + pièce). Les identifiants techniques/wire hérités
> — TLD `.torus`, `TORUS_PROTOCOL_VERSION`, events Tauri `torus://…` — sont **conservés tels
> quels** pour la compatibilité réseau ; ne pas les renommer sans changement de protocole.
> **Version** : v3.13.1 (crypto-only, hard-fork v7 « remédiation d'audit ») | **Stack** : Rust (Tauri 2.0) + Svelte 5 | **Coin** : QUANTA
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
| Transport KEX | **X25519MLKEM768** (hybride PQ, rustls `prefer-post-quantum` + aws-lc-rs) — PQ-TRANSPORT-1 | `src-tauri/Cargo.toml`, `lib.rs::run` |
| DB | libSQL (turso) | `src-tauri/src/storage/` |
| CRDT | `crdts` crate (PNCounter) | `p2p/consensus.rs` |

---

## Architecture

```
src-tauri/src/
├── lib.rs                 ← run()/AppState/invoke_handler seulement (255 l)
├── commands/              ← Commandes Tauri par domaine : identity/wallet/network/chain/diagnostics + error.rs (codes `err.*` traduits côté front) ; commands_v3.rs = @pseudo
├── guardian.rs            ← gardien de gel (extrait de setup)
├── views.rs               ← view-models PURS partagés app Tauri + JSON-RPC (Finality/Supply/Balance/Validators/Mempool — µQTA entiers, byte-compat RPC testée)
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
│   ├── ledger/            ← Blockchain en module-dossier : mod (état/types/cache O(1), genèse PQ) + validation (COVER) / stake / slash / reorg / tests — surface `p2p::ledger::` intacte
│   ├── ledger_types.rs    ← Block, Transaction, TxType
│   ├── shapley.rs         ← Distribution Shapley (énergie/travail/validation/uptime)
│   ├── consensus.rs       ← CRDT PN-Counters (convergent merge)
│   ├── gossip.rs          ← ⭐ Protocol gossip (11 variants : + FinalityVote/FinalityFault)
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
│   └── hybrid_crypto.rs   ← ⭐ ML-DSA-65 (FIPS 204) — dérivation déterministe de la clé PQ depuis la graine + UNIQUE vérificateur PQ du projet (autorité de compte PURE, PQ-MIG-3B) ; Ed25519 = transport seul
└── storage/               ← libSQL persistence
```

---

## Protocole Torus — Wire Protocol V2

### Couches du protocole
```
┌──────────────────────────────────┐
│    Application Layer             │ ← Wallet, Identité @pseudo, Staking
├──────────────────────────────────┤
│    Protocol Layer                │ ← GossipMessage (11 variants)
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
| `FinalityVote` | Broadcast | Vote de finalité ML-DSA (LIVE-1, source→target) | CRITICAL |
| `FinalityFault` | Broadcast | Preuve d'équivocation (LIVE-3, → tx `Slash`) | HIGH |
| `Ping` | Request | Liveness check | LOW |
| `Pong` | Response | Réponse liveness | LOW |
| `ReportPeer` | Broadcast | Signalement pair malveillant | LOW |

### Pipeline de sécurité (dispatch_incoming)
```
Raw bytes
  │ ① Size check (max 10 MB)
  │ ② JSON deserialize → GossipEnvelope
  │ ③ Ban check (per-peer)
  │ ④ Envelope-id canonique — id == BLAKE3(pré-image signée), sinon drop (H1)
  │ ⑤ Sonde dedup EN LECTURE (seen_messages LRU 100K) — n'insère rien
  │ ⑥ Timestamp freshness (±90s)
  │ ⑦ Signature ML-DSA-65 (PQ-ENVELOPE-1, STRUCT-1 canonical)
  │ ⑧ Insertion dedup — APRÈS authentification (H1)
  │ ⑨ Rate limit adaptatif + nonce anti-replay (per-sender monotonic)
  │ ⑩ Payload dispatch → handler
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
- **NET-5** : `TORUS_PROTOCOL_VERSION = 7` (2→3 PQ-MIG-5 : genèse PQ ; 3→4 LIVE-3B : slashes-unbonding ; 4→5 hard-fork v4 : genèse propre + PROPOSER-1 + enveloppes PQ ; 5→6 **MSIG-1** : multisig ML-DSA natif — additif, tx mono-clé byte-identiques, genèse inchangée ; 6→7 **remédiation d'audit** : C2 expéditeur synthétique confiné, C3 unstake borné par l'enjeu bondé, H2 époque de vote bornée, H1/H3 identifiant d'enveloppe canonique + dedup post-signature) ; peers incompatibles loggués
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
477 tests lib + 1 intégration, 0 failures
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
| 2026-07-13 | **💎 App v12 — wallet complet + minage pédagogique + 3D : staking **on-chain** depuis l'UI (`ledger_stake`/`ledger_unstake` signées + broadcast ; `stake_atn` legacy = miroir réputation, plus utilisé par l'UI), `get_wallet_overview` (vérité chaîne : dépensable/bondé/déverrouillage + entrées datées + pending), `get_finality_status` (époque, plancher, set de validateurs) ; écran **Minage** (Shapley expliqué, émission réelle décroissante tracée, plancher de finalité vivant) ; **Recevoir = QR + lien `quanta:`** (format BIP-21, généré offline via `qrcode-generator` — interop d'usage type BlueWallet/Electrum, jamais cross-chain, note honnête in-app) ; `Torus3D.svelte` WebGL pur zéro-dépendance (pulse au scellement, reduced-motion, pause hors-viewport) ; i18n **6 langues** complètes ; svelte-check 0 erreur/0 warning — 413 tests + 1 intégration** |
| 2026-07-13 | **🛡️ Sécurité v3.4 + app vivante — Touch ID **réel** (KEK aléatoire dans le Keychain macOS derrière `SecAccessControl(.BIOMETRY_CURRENT_SET)` : l'OS exige l'empreinte à la lecture et invalide l'item si les empreintes changent ; le KEK enveloppe les clés **dérivées** Argon2id — le mot de passe n'est jamais stocké, reste le repli), `UnlockGuard` anti-brute-force (backoff exponentiel, partagé mot de passe + biométrie), presse-papiers sensible auto-nettoyé (45 s), mode privé (montants floutés) ; **événements temps réel** `quanta://mined`/`block-sealed`/`tx-applied` → toasts + carillons WebAudio générés (réglage Sons) + solde-ticker — 413 tests, clippy propre** |
| 2026-07-13 | **💠 3D iconique (three.js) — `MiningScene` : courant de 3 600 particules sur le Torus (mouvement 100 % GPU), **surge** à chaque récompense, **bloc-cristal qui naît** à chaque scellement puis s'envole vers la chaîne, orbite au glisser ; `ChainScene` (vue Réseau, onglet par défaut) : l'hélice des blocs récents où **la finalité Casper-FFG devient visible** — pierre teal scellée ≤ plancher, verre givré au-dessus, ligne d'eau à la frontière, tooltip par bloc, drop animé du nouveau bloc ; shell partagé light-theme (ACES, pause hors-viewport, reduced-motion) ; `Blockchain3D` canvas retiré — bump **v3.4.0**** |
| 2026-07-13 | **🪙 Identité de marque v13 — le logo « l'anneau et le quantum » : Q géométrique en deux traits (anneau = Torus ; queue diagonale détachée qui **traverse** la brèche = le bloc qui se scelle), net à 16 px, monochrome-parfait — `src/lib/brand/QuantaMark.svelte` (tons ink/teal/white/aurora) + **icône d'app** régénérée (SVG → qlmanage → `tauri icon`, gradient Aurora dominé teal) + charte `docs/brand/BRAND.md` ; **colorimétrie systématisée** (`--teal-50…900`, tokens de mouvement, `:focus-visible`, `::selection`) ; `.section-label` global (étiquettes Wallet orphelines réparées) ; Explorateur élevé (recherche @pseudo/adresse/URI + reverse-username, âge des tx corrigé — parsait RFC3339 comme des secondes → « NaN », types Stake/Unstake/Slash dans le flux)** |
| 2026-07-18 | **🔐 PQ-TRANSPORT-1 — le canal de transport devient post-quantique : bascule du fournisseur rustls `ring` → **`aws-lc-rs`** (iroh `default-features=false` + `tls-aws-lc-rs`, features par défaut `metrics`/`fast-apple-datapath`/`portmapper` conservées) + rustls `prefer-post-quantum` → l'échange de clés QUIC/TLS 1.3 négocie l'hybride **`X25519MLKEM768`** (ML-KEM-768 ⊕ X25519 ; ~+1 Ko/poignée de main, une fois par connexion). Défense **« harvest-now-decrypt-later »** sur la confidentialité du transport. `aws-lc-rs` installé fournisseur **par défaut du process** au démarrage (`lib.rs::run`, lève l'ambiguïté runtime si `ring` coexiste). **Non destructif — ni bump de protocole, ni reset de genèse** (négociation TLS, dégradation gracieuse). Le hybride est vérifié par l'échange gossip réel 2-nœuds (`p2p_two_nodes_exchange_gossip`). ⚠️ **Reste classique** : l'**identité de nœud** (NodeId = Ed25519) — Iroh attend un consensus d'industrie sur la signature PQ des EndpointIds (dette **upstream**, bascule le jour de la livraison). Bilan PQ : argent + finalité + **confidentialité transport** = PQ ; il ne reste que l'auth de nœud, hors de notre code — 416 tests + 1 intégration, clippy propre** |
| 2026-07-18 | **🎬 Hard-fork v4 « genèse propre + consensus complet » (bump `TORUS_PROTOCOL_VERSION` 4→5) — 4 chantiers groupés en une seule rupture. **W1 GENESIS-V4** : genèse de relance neuve (timestamp 2026-07-18 → nouveau hash gelé content-bound, zéro premine) ; garde-fou de snapshot (`state_persistence` jette tout snapshot d'une genèse antérieure → repart sur v4). **W3 PROPOSER-1** (ferme le CRITIQUE différé) : le proposeur PoS, jusque-là vérifié seulement au *seal* (`mining_loop`), l'est désormais **à la réception** — le validateur partagé `validate_block_against_prev` (linéaire + tie-break fork + clone d'essai reorg + sync) **rejette** tout bloc non-genèse dont le proposeur n'est pas un validateur bondé as-of-parent (enjeu ≥ MIN). **Politique A** (choix d'Alexandre) : déterministe **sans horloge** — règle = union non-temporisée {leader ∪ fallbacks ∪ éligibles} = « tout validateur bondé », **sur-ensemble** de ce que le seal produit (symétrie produce/receive, zéro fork par dérive C1) ; bootstrap permissionless tant que personne n'a staké. Set d'enjeu sourcé as-of-parent sur chaque chemin (`validator_stakes()` O(1) au linéaire/clone ; `staked_before` pur au rare tie-break, verrouillé == cache). **W4** : le montant Shapley minté passe en entier u128 virgule fixe (`scale_amount`) — dernière violation f64 de la règle #6 close, conservation intacte. **W2 PQ-ENVELOPE-1** : les enveloppes gossip sont signées **ML-DSA-65** (sender = clé publique ML-DSA, vérif `verify_pq`, fallback Ed25519/legacy supprimé) sur **tous** les chemins (gossip_tasks/mining_loop/dispatcher/commands_v3/lib + cœur `sm/` via split `cfg(test)`→`sign_pq_det` pour la reproductibilité C1) ; identité transport Iroh (`endpoint_id`) **intacte**. **Après v4, le seul primitif classique restant est le NodeId Iroh (dette upstream).** 419 tests + 1 intégration (dont 2-nœuds réel + C1 128-runs), clippy `--all-targets` propre** |
| 2026-07-19 | **🌐 Écosystème de nœud + 💠 MSIG-1 (multisig PQ natif) — Quanta passe d'app de bureau à protocole avec nœud de référence. **Adresses `qta1…`** (Bech32m à checksum, de bout en bout) ; **daemon `quanta-node`** headless (extraction `node_runtime::bootstrap` partagée app+daemon) + **JSON-RPC** (serveur tokio maison, 17 méthodes : getinfo/getblock/getbalance/validateaddress/getfinalityinfo/getvalidators/getmempool/listtransactions[scan de dépôts]/sendrawtransaction/sendtoaddress/getmultisigaddress…, µQTA entiers) ; **wallet persistant** (vault via `QUANTA_WALLET_PASSWORD`, mine/détient/envoie) ; **explorateur web** autonome (GET /) + mode **`--public`** lecture-seule ; supply prouvée dans getinfo. **MSIG-1 (bump 5→6)** : multisig ML-DSA **on-chain** M-of-N — l'adresse commet à la politique `BLAKE3(MSIG_DOMAIN‖clés triées‖seuil)`, tag `pq_public_key=="msig1"`, autorité JSON dans `pq_signature` (zéro nouveau champ wire → mono-clé byte-identique), `verify_multisig` = binding rebind-proof + hash + ≥seuil signataires **distincts** valides ; première **custody quorum post-quantique** (contourne l'absence de threshold-ML-DSA). 437 tests + 1 intégration, clippy propre, chaque brique prouvée en vivant. ⚠️ multi-party wallet UX + test d'intégration multisig + audit externe = suites** |
| 2026-07-20 | **🏦 Refonte « de-slop » v3.10.0 — audit 6 angles (36 findings), l'app cesse d'être « codée par IA » : **V1 cohérence** (ticker unique QTA — QNT banni —, tutoiement FR partout, casse par CSS, emoji bannis, échelle typo `--text-*` + grille 8px, teal source unique, feedbacks standard) ; **V2 architecture d'info** (une source de vérité par écran — économie sur Minage seul, Réseau = santé réseau, Réglages = préférences, Profil = identité+sécurité ; **flux d'envoi UNIQUE** : Proches → panneau Wallet pré-rempli via `intents.svelte.ts`, revue burn+signature obligatoire ; états —/zéro/erreur honnêtes) ; **V3 fondations** (`api.ts` typé + `stores.svelte.ts` pollés par refcount — 1 intervalle par donnée ; Wallet 1112→333 l scindé en 4 panneaux + `AuthGate` ; Rust `commands/{identity,wallet,network,chain,diagnostics,error}` + `guardian.rs`, lib.rs 1560→255 l ; **10 commandes mortes purgées** dont le `find(\|_\| false)` ; `views.rs` partagé Tauri+RPC byte-compat testée ; **erreurs = codes `err.*` ×22 traduits ×6 langues** ; 150 clés i18n mortes purgées ; `ledger/` module-dossier 6 fichiers, surface intacte) ; whitepapers sans chiffres figés (zéro-fake) — **449 tests + 1 intégration**, clippy propre, svelte-check 0/0 |
| 2026-07-20 | **💠 v3.11.0 « posture iconique » + nœud vivant en arrière-plan — (1) **App Nap fix** : `node_runtime::prevent_app_nap()` (NSActivityBackground via objc2-foundation, app + daemon) — macOS étranglait les timers fenêtre occluse (tick 60 s, heartbeats) ⇒ le nœud mine désormais en fond ; sommeil machine intact ; stores front rattrapés à la re-visibilité. (2) **Audit design 20 findings** (théâtre, copie défensive, tics IA) exécuté en 4 vagues : terminal **ForgeEngine v4** (cascade BLAKE3 décorative supprimée — l'ancre = vrai hash du dernier bloc + « scellé il y a X », pipeline de consensus VIVANT allumé par les événements réels, journal scrollable, glyphes 16→7, copie défensive coupée) ; **NetworkScene3D réécrite** (900 particules d'ambiance/auto-orbit/respiration supprimées → la chaîne réelle en cubes, finalité visible pierre teal ≤ plancher / verre givré au-dessus, mouvement UNIQUEMENT sur donnée réelle, rAF stoppé au repos = zéro GPU idle) ; QuantumField supprimé (états vides sobres) ; Minage en voix propriétaire (explainer 3-cartes → 1 phrase, plancher 0,5 % de la barre d'offre retiré, Peers/Height dédupliqués, « au rythme actuel », Total forgé hiérarchisé) ; accueil premium (mark Aurora + halo doux, jargon crypto → Aide, ✓/✗ → SVG) ; aide dé-gamifiée (« Note globale » A+ supprimée de bout en bout — front, JSON, méthode Rust morte) ; Whitepaper chrome i18n ×6 ; ⌘K avec actions réelles (copier adresse/@pseudo) ; statut de nœud Sidebar honnête ; Aurora.svelte fantôme supprimé ; toasts SVG + ticker QTA. (3) **Dossier d'audit externe** `docs/audit/` (README + THREAT-MODEL + SCOPE + RFQ, scan cargo-audit réel : 8 vulns transitives évaluées, 0 dans le code Quanta) — prêt pour OSTIF/devis. 449 tests + 1 intégration, clippy propre, svelte-check 0/0 |
| 2026-07-25 | **🛡️ v3.13.0 — audit interne complet + hard-fork v7 « remédiation » (`TORUS_PROTOCOL_VERSION` 6→7).** Audit adversarial 8 axes (44 agents, chaque constat re-vérifié par des sceptiques indépendants, 3 réfutés) → `docs/audit/AUDIT-INTERNE-2026-07-25.md`. **4 critiques fermés** : **C1** auto-équivocation — aucune base anti-slashing n'existait, `build_vote_to_cast` re-dérivait (source, cible) à chaque tick de 60 s, donc un départage à hauteur égale sur une frontière d'époque faisait signer à un validateur **honnête** deux votes pour la même époque, brûlant 100 % de son enjeu ⇒ mémo persisté (6ᵉ clé de snapshot) ; **C2** émission illimitée — un `Transfer` depuis `NETWORK` échappait à la signature, à la couverture **et** au plafond (qui ne somme que `Mining`) ⇒ expéditeur synthétique confiné à l'unique coinbase ; **C3** `Unstake` jamais confronté à l'enjeu bondé (le seul garde vivait dans le constructeur local) ⇒ règle séquentielle miroir de COVER + clamp à l'application ; **C4** RPC monnaie sans aucune authentification — un `fetch()` depuis n'importe quelle page web atteignait `sendtoaddress` (requête CORS *simple*, sans préflight) ⇒ jeton cookie + garde `Origin`/`Content-Type`, surface lecture volontairement laissée ouverte. **8 hauts** : H1/H3 identifiant d'enveloppe canonique + dedup **après** signature (censure de sync gratuite fermée), H2 une seule mise de 1 µQTA arrêtait la finalité du réseau (éviction du pool inversée + époque non bornée), H4 la relecture de couverture ressuscitait des coins brûlés, H5 le tampon LIVE-4 épinglable par de la pacotille éteignait la guérison de partition, H6 cartes de pairs indexées par des clés ML-DSA de 3,9 Ko (~390 Mo atteignables), H7 injection HTML stockée dans l'explorateur, H8 annulation de bloc non-inverse fabriquant de l'enjeu. **4 moyens** : M1 délais/plafonds RPC, M2 arbre de blocs incrémental, M3 l'inventaire crypto affiché mentait (Ed25519 annoncé comme signature — corrigé partout, y compris la note de signature du portefeuille dans les 6 langues), M4 le wrap Touch ID survivait à une restauration. **473 tests + 1 intégration**, clippy propre, svelte-check 0/0 |
| 2026-07-20 | **🌗 v3.12.0 « la nuit du propriétaire » — cinq directives d'Alexandre exécutées en une : **livre blanc en voix propre** (prose continue détaillée, zéro liste à tirets ×3 fichiers, formules gardées et expliquées — plus une imitation Nakamoto) ; **Proches → Contacts** ×6 ; **Profil enrichi** (adresse qta1 + code de connexion copiables, chip validateur vivant + stake, uptime mesuré, portefeuille en bref — trust_score sciemment omis, zéro-fake) ; **mode sombre RÉEL** (root cause : `data-theme` posé dans le vide, aucune palette n'écoutait — bloc tokens `:root[data-theme=dark]` slate froid/texte hiérarchisé/teal bright encre, anti-flash `app.html`, balayage des couleurs en dur) ; **scène Réseau vivante** (WebGL2 : ton nœud au centre, pairs réels halo∝qualité, hélice de la chaîne finalité pierre/verre, particules événementielles — burst au scellement + étiquette « @pseudo a scellé #N » via `miner_name` ajouté à l'event Rust des deux chemins, filet gossip, onde de vote, ambiance 140 motes throttlée 30fps, pause hors-viewport, pools zéro-alloc). Avant : **ForgeEngine v5 « vrai terminal »** (flux bas sticky + prompt `quanta>` : status/peers/balance/supply/block/epoch/filter — le piège scroll-anchoring de v4 fermé par construction) + **gardien occlusion-aware** (`NSWindow.occlusionState` — fini les reloads fantômes) + **App Nap opt-out** (`prevent_app_nap`, le nœud mine fenêtre cachée) + **dossier d'audit externe** `docs/audit/` (threat model, scope chiffré, RFQ, scan RustSec réel : 8 vulns transitives évaluées). 449 tests + 1 intégration, clippy propre, svelte-check 0/0 |
| 2026-08-01 | **🧹 Nettoyage v3.13.1 — le programme perd ce qui ne sert plus, sans rien perdre de vivant.** Balayage adversarial 6 axes (chaque candidat mort re-vérifié par un sceptique chargé de le *sauver* : 15 constats réfutés — `consensus.rs`/CRDT, `energy`/`sybil`, `simulation.rs`, les accesseurs testés, `escrow_release_to`, les `@keyframes fadeIn`). **Rust** : 4 dépendances directes jamais importées retirées (`iroh-blobs`, `anyhow`, `base64`, `tauri-plugin-opener` + son `init()` et sa capability) ⇒ **−62 crates** dans l'arbre (842→780), autant de temps de compilation et de surface d'approvisionnement en moins ; **le schéma hybride Ed25519+ML-DSA est mort et parti** (`HybridIdentity`/`HybridSignature`/`verify_hybrid`/`REQUIRE_PQ` — plus aucun appelant depuis PQ-MIG-3B et PQ-ENVELOPE-1, et sa doc affirmait encore « Ed25519 seul fait foi », exactement le mensonge d'inventaire que M3 avait corrigé ailleurs) ; 4 commandes Tauri enregistrées mais jamais invoquées (`get_balance`, `get_node_ticket`, `resolve_address`, `username_of_pk`) ; table SQLite `transactions` + `record_tx`/`get_transactions` (vocabulaire `amount_sats`/`payment_hash`, vestige pré-P2P — l'historique vit dans le ledger) ; `GossipStats.nodes_synced` jamais incrémenté, `CLEANUP_INTERVAL` dupliquée, `shapley::distribute_emission` (helper mort, ses 2 tests re-câblés sur `compute_all_shares`). **Les 3 `#![allow(dead_code)]` périmés sont retirés** (`shapley`, `username`, `hybrid_crypto`) : le compilateur redevient l'autorité sur le code mort, et `clippy -D warnings` le prouve. **Front** : 2 composants jamais importés (`LiveCounter`, `Sparkline`), 16 clés i18n × 6 langues, 16 sélecteurs CSS morts, 2 deps npm (`@noble/hashes`, `@tauri-apps/plugin-opener`), le script `ai:map` cassé — en refusant de toucher `err.*`/`tx.*`, atteintes par clé dynamique. **Dépôt** : racine ramenée de 45 à 8 `.md` (35 specs de tâches livrées archivées dans `docs/specs/`, 2 journaux d'audit dans `docs/audit/` — déplacements internes au vault, wikilinks intacts), `_shots/` (19 Mo de captures), `.agent/` (60 fichiers d'un framework d'agent remplacé par `.claude/`, dont les renvois morts vers `.agent/design/` ont été réparés dans `.claude/`), 2 maquettes HTML. **477 tests + 1 intégration** (−5 tests du schéma hybride mort, +6 sur la primitive ML-DSA vivante : roundtrip, altération message/signature, clé étrangère, entrées malformées), clippy `--all-targets -D warnings` propre, svelte-check 0/0, `vite build` vert |
