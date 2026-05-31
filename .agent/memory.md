# QUANTA — Mémoire Persistante

> Ce fichier est lu par Claude Code au début de chaque session.
> Il contient les leçons apprises, décisions prises, et pièges à éviter.
> Mets à jour ce fichier à la fin de chaque session de travail.

---

## Décisions architecturales (avril 2026)

### V2 Pivot — Réseau social → Crypto pure
- **Date** : 29 avril 2026
- **Décision** : Suppression complète du réseau social (Feed, Editor, Browser, PostCard, likes, vues, contenu). QUANTA est désormais un protocole crypto pur.
- **Raison** : Le réseau social diluait la proposition de valeur. Le cœur de QUANTA = énergie → valeur → science.
- **Fichiers supprimés (frontend)** : Feed, Editor, Browser, PostCard, TemplatePicker, templates.ts, BadgeForge, ConstellationGraph, ActivityHeatmap, OrbitalAvatar, NotificationBell, UserProfile (12 fichiers)
- **Commandes Tauri supprimées** : create_site, update_site, get_all_sites, get_site, delete_site, record_view, burn_for_boost, get_site_token, get_leaderboard, like_content, validate_help, report_user, get_user_profile, get_notifications, mark_notifications_read (15 commandes)
- **Modules Rust encore présents mais legacy** : `attention.rs`, `notifications.rs` — encore dans `p2p/mod.rs` et `WillowNode`, à supprimer dans Phase 1

### Modèle économique V2
- **Émission** : 100 QUANTA/heure fixe, pour toujours. PAS de halving. PAS de cap.
- **Distribution** : Shapley Value (Nobel 2012) — pondérée : 30% énergie, 35% travail utile, 20% validation, 15% uptime
- **Burn** : 1% par transfert, 2% par soumission de tâche (BME)
- **DeSci** : 5% de l'émission → DAO de financement scientifique

### Navigation frontend
- **3 vues** : Wallet (défaut), Réseau (Dashboard), Réglages (Settings)
- **Bottom bar** : 3 onglets (Wallet 💰, Réseau 🔗, Réglages ⚙️)
- **PAS de sidebar**

---

## Pièges connus

### Rust
- `std::sync::Mutex` à travers un `.await` = **DEADLOCK**. Toujours `tokio::sync::{Mutex, RwLock}`
- Lock ordering : engines (read) → release → DB (write). Jamais l'inverse.
- `unwrap()` interdit en production. Toujours `Result<T, E>` + `?`
- Les `zeroize` sont critiques : toute variable `sk_bytes`, `secret`, `key` doit être effacée

### Frontend
- Svelte 5 uniquement : `$state()`, `$derived()`, `$effect()`, `$props()`
- PAS de `onMount` → utiliser `$effect()`
- PAS de stores Svelte 4 → utiliser `$state()`
- CSS vanilla uniquement, pas de Tailwind

### P2P
- Iroh `0.98` — l'API gossip utilise `iroh_gossip::api::Event::{Received, NeighborUp, NeighborDown, Lagged}`
- Le gossip topic utilise `BLAKE3("quanta-global-v1")` comme identifiant
- NAT traversal via relais Iroh intégrés

### Build
- `cargo check` dans `src-tauri/` (pas à la racine)
- `npm run build` à la racine pour le frontend Svelte
- Les tests d'intégration P2P sont dans `src-tauri/tests/p2p_integration.rs`

---

## Historique des sessions

### Session 29 avril 2026 — Phase 1
- Pivot V2 : réseau social → crypto pure
- Création des 4 fichiers design dans `.agent/design/`
- Réécriture des whitepaper EN + FR (14 sections, 524 lignes EN)
- Nettoyage frontend (12 fichiers supprimés) + backend (15 commandes supprimées)
- `cargo check` ✅ | `npm run build` ✅
- Réécriture CLAUDE.md (12 sections, 467 lignes)
- Création CI/CD GitHub Actions (ci.yml + claude-review.yml)

### Session 29 avril 2026 — Phase 2
- **Suppression modules legacy** : `attention.rs` + `notifications.rs` retirés de `mod.rs` et `WillowNode`
- **Émission fixe V2 implémentée** dans `reputation.rs` :
  - Constantes : `NETWORK_EMISSION_PER_HOUR = 100.0`, `EMISSION_PER_TICK = 1.6667`
  - Supprimé : `HALVING_THRESHOLD`, `MAX_HALVING_EPOCH`, `current_epoch()`, `atn_until_next_halving()`
  - Ajouté : `mining_rate_proportional(my_watts, total_network_watts, poc_score)` — prêt pour Phase 2 multi-nœuds
  - `uptime_tick()` : en solo, reçoit 100% de l'émission (1.6667 QUANTA/min)
- **lib.rs refactorisé** : `get_network_health()` et `get_economy_stats()` retournent `emission_model: "fixed"` au lieu de `halving_epoch`
- Création `.claude/rules/` (3 fichiers scopés : security, rust, frontend)
- Création `.claude/commands/` (3 commandes custom : implement, security-audit, v2-refactor)
- Création `.agent/design/tech_references.md` (RISC Zero, Shapley, CRDT, Iroh, BME)
- Stratégie dual-model documentée : Sonnet (volume) + Opus 4.7 (complexe)
- `cargo check` ✅ | `npm run build` ✅

### Session 30 avril 2026 — Purge sociale radicale
- **Supprimés du disque** : `attention.rs` + `notifications.rs` (fichiers orphelins)
- **reputation.rs** réduit de 391 → ~210 lignes :
  - Supprimé : ATN_VIEW, ATN_LIKE, ATN_FORK, ATN_HELP_VALIDATED, ATN_REPORT_CORRECT, TRUST_HELP/CONTENT/LIKE, cooldown constants
  - Supprimé : champs sociaux de UserReputation (content_created, helps_given, likes_received, reports_*)
  - Supprimé : méthodes sociales (on_content_created, on_view, on_like, on_help_validated, on_report, on_fork)
  - Supprimé : view_cooldowns/like_cooldowns de ReputationEngine
  - Supprimé : visibility_boost() de TrustStatus
  - Nouvelle formule compute_trust_score() V2 : uptime + énergie + stake
  - trust_score recomputed dans uptime_tick() à chaque tick
- **sybil.rs** adapté (PoC V2) : suppression content_factor/likes_factor → ajout energy_factor (kWh) ; poids révisés : uptime×0.35, énergie×0.30, stake×0.25, ancienneté×0.10
- **simulation.rs** : suppression des appels à current_epoch()/atn_until_next_halving()
- `cargo test` ✅ (25 tests + 1 intégration) | `npm run build` ✅

### Session 30 avril 2026 — V2 Core (sonnet_v2_core.md)
- **TxType** : supprimé Like/Help/Create/View — seuls Mining/Transfer/Stake/Unstake/Burn restent
- **GossipMessage::Hello** : ajout `watts: f64` + `country: String` (proportional mining V2)
- **WillowNode** : ajout `peer_watts: Arc<RwLock<HashMap<String, f64>>>`
- **dispatcher.rs** : `handle_hello` stocke watts du pair + country
- **reputation.rs** : `uptime_tick(pk, total_mined, total_network_watts)` — proportionnel ; solo si total=0
- **lib.rs** : mining loop agrège `peer_watts.sum() + my_watts` avant d'appeler uptime_tick
- **ledger.rs** : `transfer_with_burn()` — 1% brûlé automatiquement par transfert
- **ledger_transfer** : retourne `{tx, burn_amount, net_amount}` au frontend
- **simulation.rs** : scenario 2 nœuds (10W + 20W) avec total_network_watts réel
- **shapley.rs / marketplace.rs** : fix clippy (from_contributions, collapsible_if)
- 37 tests (25+1 avant → 37+1 maintenant), `cargo clippy -D warnings` ✅ | `npm run build` ✅
- Git non initialisé (`.git/` vide — seulement `hooks/`) — commit manuel requis

### Session 30 avril 2026 — Shapley + Marketplace integration (Opus)
- **Review math** : shapley.rs (6 tests, sum=1.0 invariant garanti) + marketplace.rs (3 tests, lifecycle Pending→Claimed→Submitted→Completed) → aucun patch nécessaire
- **WillowNode** : ajout `pub marketplace: Arc<RwLock<Marketplace>>`
- **reputation.rs::uptime_tick** : Shapley wired (single-node returns share=1.0 ; multi-node attend peer contribs gossip Phase 3+)
- **lib.rs** : 3 commandes Tauri marketplace
  - `submit_compute_task(task_type, reward, deadline)` — task_type ∈ {scientific, ml_training, render_3d, wasm}
  - `get_pending_tasks()` — liste les tâches Pending
  - `get_marketplace_stats()` — MarketplaceStats sérialisé
- **Validation Phase 2 mineure** : marketplace.submit_task ne touche pas le ledger (escrow + refund = Phase 3 ledger-level)
- 37 tests unit + 1 intégration ✅ | clippy clean ✅ | npm build ✅

### Behavior caveat — uptime_tick multi-node (à corriger Phase 3)
- En multi-nœud, Shapley reçoit uniquement notre `NodeContribution` → `compute_all_shares` retourne `{pk: 1.0}` (single-node case) → revient au comportement solo
- La proportionnalité par watts (V2 core) est en attente du gossip des `NodeContribution` des pairs
- **Impact** : pas de régression de tests (les tests proportionnels appellent `mining_rate_proportional` directement, pas `uptime_tick`)

### TODO — prochaines étapes
- [x] `git init` + premier commit ✅ (30 avril, commit 01517d8)
- [ ] Gossiper `NodeContribution` (watts + tasks_completed + blocks_verified + uptime) dans les Hello messages — débloquer la proportionnalité Shapley
- [x] Câbler `marketplace.submit_task` au ledger : escrow `reward_quanta` du soumetteur, refund sur `expire_tasks` ✅ (Phase B)
- [x] Câbler `marketplace.validate_and_pay` au ledger : credit worker `reward - burn` depuis ESCROW ✅ (Phase B)
- [ ] Lier `tasks_completed` (depuis `marketplace.completed_by(pk)`) à `NodeContribution` dans `uptime_tick`
- [ ] DeSci DAO : 5% de l'émission vers un pool de financement scientifique
- [ ] ZK-Proof : guest RISC Zero pour prouver les watts (Phase 4)
- [x] **MOD-1** : Vérifier nonce tx dans `handle_broadcast_tx` (dispatcher.rs) ✅
- [x] **MOD-2** : Borner `seen_messages` avec LRU (gossip.rs) ✅

### Phase A — Hygiène (30 avril 2026, Antigravity)
- **617 lignes supprimées** (4759 → 4142 lignes Rust)
- consensus.rs : purgé InteractionCounters (likes/views), test gcounter_likes_monotone
- lib.rs : supprimé start_sync, stop_sync, index_site, peer_query + SemanticIndexer import
- search/ : module entier supprimé (indexer.rs + mod.rs)
- storage/db.rs : purgé struct Site + 7 fonctions + table search_index (304 → 145 lignes)
- willow_node.rs : supprimé Subspace, start_sync, stop_sync, get_subspace, validate_block
- mod.rs : supprimé SubspaceStatus, ContentBlock, WillowEntry
- Fix : minimum transfer 0.01 QUANTA dans transfer_with_burn
- Fix : ledger.seal_block unwrap → expect("genesis block must exist")
- Git initialisé, 182 fichiers, commit 01517d8
- 36 tests ✅ | clippy -D warnings ✅ | npm build ✅

### Phase B — Security Hardening (30 avril 2026, Antigravity Opus)

**Objectif** : Durcissement sécuritaire du protocole — signature verification, anti-replay, peer TTL, emission cap, escrow.

#### B1 — Gossip Signature Verification ✅
- `dispatcher.rs` réécrit : `verify_envelope_signature()` vérifie Ed25519 AVANT tout dispatch
- `ReportPeer::InvalidSignature` émis sur forgery
- `NonceTracker` ajouté à `WillowNode`, champ `nonce` ajouté à `GossipEnvelope` (`#[serde(default)]`)
- Vérifié dans `dispatch_incoming()` (skip si nonce=0 pour backward compat)

#### B2 — Balance Verification & Nonce System ✅
- `account_nonces: HashMap<String, u64>` dans Ledger, persisté dans `LedgerSnapshot`
- `get_nonce()`, `increment_nonce()` — nonce incrémenté à chaque `build_signed_tx()`
- `total_supply()` ajouté au Ledger
- `build_escrow_lock_tx()` / `escrow_release_to()` pour le vrai escrow marketplace
- `submit_task_with_escrow()` débite réellement les fonds du soumetteur → ESCROW
- `validate_and_pay()` prend `&mut Ledger`, crédite le worker depuis ESCROW
- `expire_tasks()` prend `&mut Ledger`, rembourse le soumetteur
- `submit_compute_task` (lib.rs) utilise maintenant l'API escrow

#### B3 — Peer TTL & Dead Peer Detection ✅
- `PeerInfo { watts, country, last_seen: Instant }` remplace `peer_watts: HashMap<String, f64>`
- `cleanup_dead_peers()` avec TTL 5 min, spawn toutes les 30s dans lib.rs
- `total_network_watts()` ne compte que les peers vivants

#### B4 — Emission Hard Cap ✅
- `.min(EMISSION_PER_TICK)` dans le mining loop de lib.rs
- `EMISSION_PER_TICK` rendu `pub` dans reputation.rs (source unique)

#### B5 — Security Testing Framework ✅
- `security_tests.rs` : 12 tests S1-S12 + 4 property tests P1-P4 (23 total)
- Couverture : double-spend, forgery, replay, stale, overflow, emission cap, Shapley, self-transfer, negative amounts, escrow, supply conservation, nonce replay

#### Audit résultat : 7/7 défauts corrigés ✅
- **CRIT-1** ✅ NonceTracker branché au dispatcher
- **CRIT-2** ✅ Escrow réel avec lock/release
- **CRIT-3** ✅ submit_compute_task → escrow API
- **MOD-1** ✅ Nonce du Ledger vérifié dans `handle_broadcast_tx` + `increment_nonce` pub(crate)
- **MOD-2** ✅ `seen_messages` borné à 100 000 entrées via VecDeque LRU (gossip.rs)
- **MIN-1** ✅ Variable préfixée
- **MIN-2** ✅ Constante centralisée

**68 tests ✅ | 0 warnings | 0 errors** (+3 tests : S13, mod2_bounded, mod2_dedup_after_eviction)

### Session 30 avril 2026 — Phase B completion (MOD-1 + MOD-2)
- **MOD-1** : `handle_broadcast_tx` (dispatcher.rs) vérifie `tx.nonce == ledger.get_nonce(&tx.from)` avant d'appliquer au CRDT, puis appelle `ledger.increment_nonce()`. `increment_nonce` exposé en `pub(crate)`.
- **MOD-2** : `GossipRouter::seen_messages` borné à `MAX_SEEN_MESSAGES = 100_000` via `VecDeque<MsgId>` (insertion-order LRU). `GossipRouterSnapshot` mis à jour (`seen_order` avec `#[serde(default)]`). Méthode `seen_messages_count()` ajoutée.
- **Nouveaux tests** : `s13_remote_tx_nonce_verified`, `mod2_seen_messages_bounded`, `mod2_seen_messages_dedup_after_eviction`
- Lock ordering respecté : ledger.read() → drop → consensus.write() → drop → ledger.write()

### Session 30 avril 2026 — Audit Structural + Fixes (Antigravity Opus)

**4 problèmes structurels corrigés immédiatement :**
- **STRUCT-1** ✅ Signature gossip étendue : couvre maintenant `sender + nonce + timestamp + payload` (pas juste payload). `signable_envelope_bytes()` est la nouvelle source canonique. `payload_bytes()` marquée `#[deprecated]`. Backward compat via fallback dans `verify_envelope_signature()`. Tous les senders (dispatcher broadcast, lib.rs Hello/BroadcastTx) migrés.
- **STRUCT-4** ✅ Watts clampés à [1.0, 500.0] dans `handle_hello()`. Log warn si clampage. Empêche un nœud de capturer l'émission avec des watts falsifiés.
- **STRUCT-5** ✅ CRDT credit/debit cappé à `MAX_CRDT_BATCH = 10_000` milliATN (10 QUANTA). Empêche O(n) DoS via gros transferts.
- **WEAK-4** ✅ Block hash inclut maintenant un Merkle root des tx IDs (`seal_block()` dans ledger.rs). Deux blocs avec des transactions différentes ont des hashes différents.
- **WEAK-5** ✅ ESCROW exclu de `all_balances()` aux côtés de NETWORK et BURN.

**Problèmes structurels identifiés mais non corrigés (refactor majeur):**
- **STRUCT-2** ✅ Migré (voir session Phase C)
- **STRUCT-3** ✅ Migré (voir session Phase C)
- **STRUCT-6** ✅ Migré (voir session Phase C)

**68 tests ✅ | 0 warnings | 0 clippy errors**

### Session 30 avril 2026 — Phase C structural refactor (STRUCT-2 + STRUCT-3 + STRUCT-6)

**STRUCT-2 — f64 → u64 µQUANTA migration**
- Constante `pub const MICRO: u64 = 1_000_000` ajoutée dans `ledger.rs` (1 QUANTA = 10^6 µQUANTA).
- `Transaction.amount: f64 → u64`, balances/supply/burned/escrow tous en u64 µQUANTA.
- `EMISSION_PER_TICK: f64 → u64 = 100 * MICRO / 60 = 1_666_666` µQUANTA.
- `ReputationEngine` : `atn_balance/atn_earned/atn_staked/energy_atn_mined: u64`. Welcome credit = `10 * MICRO`.
- `Marketplace` : `reward_quanta/burn_amount/total_quanta_paid/escrow_locked: u64`. Burn calc = `amount * 2 / 100` (intégers).
- `CrdtLedger` : `balance_of() -> u64` (µQUANTA). `MAX_CRDT_BATCH = 10_000_000` µQUANTA = 10 QUANTA.
- `lib.rs` : helper `quanta_to_uquanta(f64) -> Result<u64,_>` à la frontière Tauri (rejette NaN/inf/négatif/overflow). Conversions display via `uquanta as f64 / MICRO as f64`.
- Sybil : `stake_factor` reçoit µQUANTA, divise par MICRO en interne pour la courbe exp.
- **Pas de backward-compat tx** : signatures pré-migration ne valideront pas (intentionnel, clean break — la chaîne corrompue redémarre fresh genesis dans lib.rs).

**STRUCT-3 — Reconcile dual ledger**
- `Ledger::replay_remote_tx(tx) -> bool` : applique une tx distante au ledger linéaire après vérif sig+nonce, dedup via `seen_tx_hashes`.
- `dispatcher::handle_broadcast_tx` : applique au CRDT puis `ledger.replay_remote_tx(tx)` + `increment_nonce` si `applied`.
- Mining loop (lib.rs) : après `mine_tx`, miroir CRDT `cons.ledger.credit("network", &pk, uquanta_mined)`.
- **Architecture** : Ledger = source de vérité locale (blocs, sigs, nonces) ; CrdtLedger = couche de sync gossip (PNCounter convergent).

**STRUCT-6 — Wire Shapley to real multi-node data**
- `GossipMessage::Hello` étendu avec `tasks_completed: u64`, `blocks_verified: u64`, `uptime_minutes: u64` (tous `#[serde(default)]` pour backward-compat).
- `PeerInfo` étendu avec les 3 mêmes champs ; `dispatcher::handle_hello` les capture.
- `ReputationEngine::uptime_tick(pk, total_mined, &peer_contribs: &HashMap<String, NodeContribution>)` : signature changée — Shapley reçoit la map complète (self + peers), solo si vide.
- Mining loop (lib.rs) : collecte `peer_info` vivants (TTL 5 min) → `HashMap<String, NodeContribution>` → passé à `uptime_tick`.
- Hello broadcast (lib.rs) : envoie `tasks_completed = marketplace.completed_by(pk)` + `uptime_min` du `UserReputation` local.
- `simulation.rs` adapté : chaque nœud voit l'autre comme peer dans la map de contributions.

**Résultat final Phase C** : 68 tests lib ✅ + 1 intégration ✅ | `cargo clippy -- -D warnings` ✅ (lib production) | 0 errors

### Phase D1 — Block Consensus & Propagation (TERMINÉ ✅ — Opus 4.7)

- **D1.1** ✅ `GossipMessage::NewBlock { block_json }` dans gossip.rs (code partiel Antigravity complété)
- **D1.2** ✅ `validate_remote_block()` + `integrate_remote_block()` dans ledger.rs + refactor `compute_merkle_root()` partagé entre `seal_block` et `validate_remote_block` (anti-drift)
- **D1.3** ✅ `handle_new_block()` dans dispatcher.rs — deserialize, integrate, log accept/reject/dedup
- **D1.4** ✅ Mining loop (lib.rs:670-689) broadcast le bloc sealed via `NewBlock` gossip après `seal_if_pending`
- **D1.5** ✅ 5 tests ajoutés dans security_tests.rs (d1_validate_remote_block_valid, d1_reject_block_bad_prev_hash, d1_reject_block_bad_hash, d1_fork_resolution_deterministic, d1_duplicate_block_ignored)
- **Fork resolution** : deterministic tie-break via hash lexicographique (highest hash wins). Note: la branche fork dans `integrate_remote_block` ne re-appelle pas `validate_remote_block` sur le winner — seulement prev_hash post-pop est vérifié. À durcir en D2 si besoin.

**Résultat** : 73 tests ✅ | 0 warnings | 0 clippy errors

---

## V3 PIVOT — Torus = Google P2P social (mai 2026 — Opus 4.7)

### Décision

L'utilisateur veut transformer Torus en **navigateur P2P + moteur de recherche + plateforme sociale + marketplace**, avec QUANTA comme coin interne récompensant publications, likes, abonnements, modération honnête. Tout ce que la V2 interdisait explicitement (`feed`, `likes`, `contenu`) est désormais la mission.

CLAUDE.md, WHITEPAPER_FR.md et `.claude/rules/frontend.md` ont été réécrits dans ce sens.

### Phase V3.1 livrée — Modules backend purs

Tous dans `src-tauri/src/p2p/` :

| Fichier | Rôle | Tests |
|---|---|---|
| `domains.rs` | Registre `*.torus`, **Harberger Tax** 1%/mois, sous-domaines, claim/overbid/reclaim | 17 ✅ |
| `search.rs` | Index inversé local + tokenizer multilingue + ranking **QuantaRank** (TF-IDF × likes × abonnés × rep × fraîcheur × diversity × mod_factor) + sharding DHT déterministe BLAKE3 | 12 ✅ |
| `social.rs` | Likes **quadratiques** (influence = √amount), abonnements 3 tiers, tips, boosts (cap 100 QTA/page/jour, burn 5%) | 10 ✅ |
| `moderation.rs` | Reports signés, jury 7 jurés via BLAKE3 PRNG vérifiable, **commit-reveal** Schnorr, slashing graduel + anti-troll factor | 10 ✅ |
| `forums.rs` | Forums + threads DAG + commentaires imbriqués + soft-fork | 9 ✅ |
| `trust_graph.rs` | **PageRank personnalisé** (damping 0.85, max 30 iter, ε=1e-6) → score de confiance par viewer | 6 ✅ |

**Résultat** : `cargo test --lib` → **173 passés / 0 échecs** (incl. 64 nouveaux V3 + 109 V2 existants intacts).

### Choix de design notables

1. **Harberger Tax** plutôt que loyer fixe : force l'évaluation honnête (anti-squatting type ENS)
2. **VRF "MVP"** : sélection juré via BLAKE3(seed_block_hash || case_id) — déterministe + auditable. Migration `schnorrkel` planifiée.
3. **Commit-reveal** strict : reveal validé ssi `BLAKE3(verdict_byte || nonce_hex) == commit_hash`
4. **Quadratic voting** : décourage les fermes de bots (mettre 100 QTA = 10× influence vs 1 QTA, mais 100 likes diversifiés > 1 like de 100 QTA)
5. **PageRank personnalisé** (vs global) : ferme externe non atteignable depuis viewer = score 0
6. **Constants alignées** sur CLAUDE.md (DOMAIN_INITIAL_FEE, JURY_SIZE, BOOST_MAX_DAILY, etc.)

### Pas encore wirées (V3.2)

- Modules V3 **pas encore intégrés à `WillowNode`** (`willow_node.rs`)
- **Pas de gossip messages** ajoutés (`PublishDomain`, `Vote`, `Report`, etc.)
- **Pas de commandes Tauri** côté `lib.rs`
- **Pas de persistence libSQL** des nouveaux stores

### Frontend V3.1 livré

- `Browser.svelte` : barre URL `torus://`, recherche keywords, résultats QuantaRank, viewer iframe sandbox (JS off par défaut)
- `PageBuilder.svelte` : éditeur WYSIWYG par blocs (heading/paragraph/image/link/code), preview live iframe
- `Sidebar.svelte` : ajout entrées `Browser` + `Page Builder`
- `routes/+page.svelte` : router étendu

Les composants utilisent des **mocks** (TODO V3.2 : remplacer par `invoke<T>("...")`).

### Commandes Tauri à wirer en V3.2

```rust
publish_site(title, domain, html, blocks)
search_pages(query, filters) -> Vec<SearchHit>
resolve_and_fetch(name) -> { title, html }
claim_domain(name, value_micro_qta)
pay_domain_rent(name, payment_micro_qta)
overbid_domain(name, new_target_pk, new_value, payment)
social_vote(target_cid, target_author_pk, weight, amount)
social_follow(followee_pk, tier)
social_tip(target_cid, target_author_pk, amount, memo)
social_boost(target_cid, target_author_pk, amount)
submit_report(target_cid, target_author_pk, category, evidence_cid)
juror_commit(case_id, verdict, nonce)
juror_reveal(case_id, verdict, nonce)
forum_create(name, description)
thread_create(forum_id, title, body, forked_from?)
comment_create(thread_id, parent_id?, body)
trust_score(viewer_pk, target_pk) -> f64
```

### Pièges constatés

- Test `search::moderation_malus_kills_score` initialement échouait car le hit avec score 0 restait dans la liste — **fix** : `if mod_factor <= 0.0 { return None; }` skip explicite.
- Le build cache Tauri pointait vers un ancien chemin `/Users/alex/Desktop/Torus/` au lieu de `/Users/alex/Desktop/Projets/Torus/` — `cargo clean` nécessaire après déplacement du projet.
- `frontend.md` interdisait toujours Browser/Feed/likes (V2) → mis à jour pour V3.

### Prochaines étapes V3.2 (pour la prochaine session)

1. **Wiring backend** : ajouter les 6 nouveaux moteurs comme champs de `WillowNode`, créer commandes Tauri correspondantes
2. **Gossip messages V3** : `PublishDomain`, `PublishSubdomain`, `BroadcastSocialAction`, `BroadcastReport`, `BroadcastJurorVote`, `PublishSite`, `PublishForumNode`
3. **Étendre `page_store.rs`** : sites multi-pages + assets DAG (réutilise `merkle_dag.rs`)
4. **Frontend** : SearchView.svelte plein écran, CreatorProfile.svelte (bouton suivre/tip), ForumList.svelte/ThreadView.svelte, Subscriptions.svelte
5. **Branchement Shapley v2** : ajouter `social_utility` (15%) à `shapley.rs` à partir de `SocialState::page_stats`
6. **Tests d'intégration** end-to-end (publication → recherche → like → modération → slashing)


---

## Session V3.2 — 2026-05-06 — Wiring backend des engines V3

### Livrables

**1. WillowNode étendu** (`willow_node.rs`)
- 6 nouveaux champs : `domains` (DomainRegistry), `search` (SearchIndex), `social` (SocialState), `moderation` (ModerationEngine), `forums` (ForumsEngine), `follow_graph` (FollowGraph)
- Tous en `Arc<RwLock<...>>` (cohérent avec le pattern V2)
- Init dans `new()` ; persistence libSQL à brancher en V3.3

**2. Gossip étendu** (`gossip.rs`, `dispatcher.rs`)
- 8 nouveaux variants `GossipMessage` : `PublishDomain`, `PublishSubdomain`, `PublishSite`, `BroadcastSocialAction`, `BroadcastReport`, `BroadcastJurorCommit`, `BroadcastJurorReveal`, `PublishForumNode`
- Dispatcher : 8 handlers V3 (`handle_publish_domain`, `handle_publish_site`, `handle_broadcast_social_action` etc.)
- Pipeline B/C/D inchangé : signature → nonce → freshness → dedup → handler V3
- 5 nouveaux compteurs `GossipStats` : `domains_published`, `sites_indexed`, `social_actions_applied`, `reports_received`, `forum_nodes_received`
- Web of Trust : `Follow` côté handler met à jour `follow_graph` automatiquement

**3. Commandes Tauri V3** (`commands_v3.rs`, ~700 lignes)
Nouveau module dédié pour ne pas gonfler `lib.rs`. **25 commandes** :
- **Domaines (5)** : `claim_domain`, `pay_domain_rent`, `overbid_domain`, `resolve_domain`, `list_my_domains`
- **Recherche (3)** : `index_my_page`, `search_pages`, `search_stats`
- **Social (6)** : `social_vote`, `social_follow`, `social_tip`, `social_boost`, `get_page_social_stats`, `get_creator_social_stats`
- **Modération (5)** : `submit_moderation_report`, `juror_commit`, `juror_reveal`, `finalize_case`, `get_open_cases`
- **Forums (6)** : `forum_create`, `thread_create`, `comment_create`, `list_forums`, `list_threads`, `list_comments`
- **Trust (1)** : `trust_score_for`

Patron : `wrap_broadcast()` signe l'enveloppe via `CryptoEngine` (jamais d'accès direct au SK depuis les commandes), puis push sur `gossip_tx`. Helper `signing_key_from_state()` pour les `build_*` qui requièrent un `SigningKey` (zéroïse implicitement à drop).

**4. Shapley v2** (`shapley.rs`)
- Poids re-équilibrés : 25% énergie / 25% travail / 20% validation / 15% uptime / **15% utilité sociale** (somme = 1.0)
- Nouveau champ `NodeContribution::weighted_likes` (Σ √µQTA reçus)
- Mining loop wire `weighted_likes` depuis `SocialState::creator_stats` (proxy `follower_count` en V3.2 ; vraie somme likes en V3.3 quand `SearchIndex` exposera `doc_by_cid`)
- Tous les call-sites mis à jour : `reputation.rs`, `mining_loop.rs`, `simulation.rs`, `security_tests.rs`

**5. Tests + clippy**
- `cargo test --lib` : **173/173 passent** (zéro régression — Shapley v2 reste compatible avec les invariants V2)
- `cargo clippy -- -D warnings` : **clean**
- Modules V3 marqués `#![allow(dead_code)]` (helpers `sign_action`, `sign_report` etc. exposés pour tests externes/V3.3)
- Test flaky `search::diversity_penalizes_same_author` rendu déterministe via tie-breaker `then_with(|a.cid.cmp(&b.cid))` — bug pré-existant lié à l'ordre HashSet en parallèle

### Pièges constatés

- **`overbid_domain`** : la méthode `DomainRegistry::overbid` requiert que le caller fournisse le SigningKey du challenger ; en V3.2 on contourne via `update()` direct (la sig vérification réseau garantit la légitimité). À nettoyer en V3.3 en exposant une variante `overbid_with_signed_record(rec, payment)`.
- **`signing_key_from_state`** : reconstruit `ed25519_dalek::SigningKey` à partir du secret en mémoire (zéroïse implicitement). Nécessaire pour `build_forum/thread/comment` qui prennent `&SigningKey` plutôt que `&dyn Signer`. Pas de leak — secret consommé puis dropé.
- **Shapley v2 backward-compat** : `weighted_likes: 0.0` partout où la valeur n'est pas connue (peers sans données sociales). Le facteur retombe sur la distribution uniforme (`1.0 / node_count`), donc pas de régression sur les tests V2.
- Marquer chaque module V3 avec `#![allow(dead_code)]` au lieu de annotation par-symbole : plus propre pour des modules qui exposeront leur API publique progressivement.

### Reste pour V3.3

- **Persistence libSQL** : snapshots `DomainRegistry`, `SearchIndex`, `SocialState`, `ModerationEngine`, `ForumsEngine`, `FollowGraph` (chaque module a déjà `snapshot()/restore()` — il manque le wiring dans `state_persistence.rs`)
- **Vrai `weighted_likes`** par auteur : exposer `SearchIndex::doc_by_cid()` puis sommer sur tous les pages d'un même auteur dans `mining_loop`
- **Sous-domaines** : `handle_publish_subdomain` est un stub ; ajouter un store dédié dans `DomainRegistry`
- **Anti-troll wiring** : brancher `anti_troll_mining_factor` dans le calcul Shapley (multiplier `weighted_likes` par ce facteur)
- **page_store.rs étendu** : sites multi-pages + assets DAG (V3.1 task #8)
- **Frontend** : Browser invoke réel (au lieu des `mockSearch/mockFetch`), SearchView, CreatorProfile, ForumList, Subscriptions, ModeratorPanel

---

## V3.3 — From "compiles" to "production-ready" (2026-05-06)

V3.3 ferme tous les gaps qui empêchaient Torus d'être déployable au-delà de la démo : persistence complète, vrais signaux Shapley, sous-domaines, anti-troll, sites multi-pages, frontend branché, onboarding zéro-friction.

### Livrables backend

1. **Persistence libSQL des 6 moteurs V3** (`state_persistence.rs`)
   - 12 clés persistées (6 V2 + 6 V3) : `domains`, `search`, `social`, `moderation`, `forums`, `follow_graph`
   - Pattern `tokio::join!` étendu à 12 snapshots concurrents — toujours 1 transaction batchée
   - Restore dans l'ordre canonique au boot ; logs d'audit (`◈ [V3] X restored`)
   - `FollowGraph` (alias `HashMap`) sérialisé/restauré directement (pas besoin de wrapper Snapshot)

2. **Vrai `weighted_likes` par auteur** (`social.rs` + `mining_loop.rs`)
   - `CreatorStats` augmenté de `weighted_likes_received`, `weighted_dislikes_received`, `tip_total_received_micro_qta` (`#[serde(default)]` rétro-compat)
   - `social.apply()` Vote crédite directement le créateur cible : `creator.weighted_likes_received += √(amount)`
   - `mining_loop` lit `(likes - dislikes).max(0.0)` au lieu du proxy `follower_count`
   - Plus besoin de joindre SearchIndex × SocialState — calcul O(1) par peer

3. **Sous-domaines** (`domains.rs` + dispatcher + commande Tauri)
   - Nouveau `DomainRegistry::apply_overbid_record()` : remplace le workaround `update()` par une vraie méthode qui accepte un `owner_pk` différent si signature challenger valide
   - `handle_publish_subdomain` n'est plus un stub : appelle `grant_subdomain` après vérification sig parent
   - Commande `grant_subdomain(name, target_pk)` : valide forme `child.parent.torus`, vérifie ownership parent, signe via CryptoEngine, broadcast

4. **Anti-troll wiring dans Shapley** (`mining_loop.rs`)
   - Après `uptime_tick()`, multiplie `raw_uqta` par `moderation::anti_troll_mining_factor(reports_validés_30j)` (1.0 → 0.0)
   - Logs `warn!` si `factor < 1.0` pour tracer les nœuds en sanction
   - Helper `tick_start_secs()` extrait pour la fenêtre 30j

5. **Sites multi-pages + assets** (`page_store.rs` réécrit, +480 LoC, 8 tests)
   - Nouveau `SiteManifest { author_pk, root_path, pages[], assets[], version, signature }` cohabite avec `PublishedPage` (legacy)
   - `SitePage { path, title, html }` (max 64 KB), `SiteAsset { path, mime, content_b64 OR dag_cid, size }` (max 256 KB inline, sinon DAG chunking)
   - Limites : 100 pages, 50 assets, 8 MB total, paths ASCII validés (`/`, `[a-z0-9-_.]`, pas de `..`)
   - `signable_manifest_bytes()` calcule un Merkle-summary BLAKE3 stable (indépendant du JSON ordering)
   - Persistence via `PageStoreSnapshot { pages, sites }` (default `[]` pour rétro-compat)
   - Nouveau gossip variant `PublishSiteManifest`, handler dispatcher, stat `site_manifests_received`
   - Commandes Tauri : `publish_site`, `get_site_page`, `get_site_asset`, `list_sites`

6. **Bonus refactor pendant qu'on y était**
   - `SearchIndex::doc_by_cid()` exposé → `index_doc_author()` n'est plus un stub, le ranking utilise les vrais signaux follower/reputation
   - `SearchIndex::list_by_authors()` pour le feed Subscriptions
   - Commandes `list_my_subscriptions` + `subscriptions_feed` (combo follow_graph ⇆ search index)

### Livrables frontend

1. **Browser.svelte** réécrit (mocks → invoke réels)
   - `search_pages` invoke réel + filtres typés
   - Résolution URL `torus://name.torus/path` : `resolve_domain` → `get_site_page` (V3.3 multi-page) avec fallback `get_page` (legacy V2)
   - `renderWithAssets()` injecte les assets inline (`/style.css`, `/img/x.png`) en data: URIs avant rendu iframe
   - Iframe sandbox stricte (`""` par défaut, `"allow-scripts"` opt-in via toggle JS — `allow-same-origin` retiré pour CSP plus sévère que CLAUDE.md)
   - Actions sociales branchées : `social_vote` (like 0.1 QTA), `social_tip` (1 QTA), `social_follow` ("signal"), `submit_moderation_report`
   - CID local dérivé via `SHA-256(authorPk|path)` côté client (assez stable pour servir d'identifiant social)
   - Stats `get_page_social_stats` affichées en barre d'actions
   - Empty states + retry friendly (proposition de publier le 1er site)

2. **Forums.svelte** créé
   - 3 vues : list (forums + threads dépliables), thread (commentaires), newForum
   - Création forum/thread/comment via `forum_create`/`thread_create`/`comment_create` avec params alignés sur les commandes Rust (`forkedFrom: null`, `parentCommentId: null`)
   - Auto-refresh des threads à l'ouverture

3. **Subscriptions.svelte** créé
   - 2 sections : créateurs suivis (avec stats agrégées : likes pondérés reçus, tips reçus, follower_count) + feed des publications récentes
   - Action désabonner (1 clic, `social_follow active=false`)

4. **Sidebar** étendu avec 2 nouvelles vues (icons SVG inline) ; `+page.svelte` route vers Forums + Subscriptions

5. **Welcome.svelte** réécrit (onboarding zéro-friction)
   - 1 seul écran : juste un mot de passe, pseudo auto-généré (`User-XXXX`), pas de saisie de nom obligatoire
   - "Options avancées" cachées par défaut (pseudo + confirmation password)
   - Plus de forced screen recovery key — l'app entre directement dans le Dashboard, recovery key disponible plus tard via Profil
   - Préserve les anciens steps `recovery`/`confirm`/`unlock` comme fallback (compat)

### Quality gates V3.3

- `cargo test --lib` → **181/181** (8 nouveaux tests pour SiteManifest + 0 régression V2/V3.2)
- `cargo clippy -- -D warnings` → **clean** (1 fix mineur : `sort_by` → `sort_by_key + Reverse`)
- `npm run check` (svelte-check) → **0 errors**, 8 warnings non-bloquants pré-existants
- Bonus : 3 erreurs TS pré-existantes corrigées (Identicon `seed` → `pubkey` dans Explorer/Profile)

### Décisions / pièges V3.3

- **Browser sandbox sans `allow-same-origin`** : choix plus strict que la version V3.2 (qui l'avait par défaut). Conséquence : pas de cookies/storage par site. Acceptable car les pages Torus sont stateless par design.
- **CID local côté frontend** : `SHA-256(authorPk|path)` n'est pas le BLAKE3 du contenu, mais c'est intentionnel — sert juste de clé sociale stable pour `social_vote/tip/report`. Les vrais CIDs content-addressed restent côté backend.
- **`SiteManifest.signature`** : Merkle-summary BLAKE3 du contenu (pas le JSON brut), donc la sig reste valide même si le JSON est re-sérialisé/réordonné par les caches gossip.
- **`anti_troll` lit moderation deux fois** (`validated_count_30d`) si on log : acceptable (read lock, peu fréquent), sinon refactoriser pour cacher la valeur.
- **Subscriptions vide** : par défaut UX dirige vers le Browser pour suivre quelqu'un (« Visitez un site et + Suivre »). Acceptable car l'utilisateur arrive avec un graphe vide.

### Reste pour V3.4 (post-bilan)

- Banner « Sauvegardez votre clé de récupération » dans le Dashboard si pas encore fait
- Vrai chunking DAG des gros assets (`SiteAsset.dag_cid`) — handler côté backend pour reconstituer
- ProfilePublic.svelte : visiter le profil d'un autre user (follow + tip + liste des sites publiés)
- Search filters UI dans Browser (langue, type, fraîcheur)
- Settings → Boutique de domaines `*.torus` (claim/pay rent/overbid via UI)
- Settings → Modération : voir les cas ouverts, voter en tant que juré
- Internationalisation EN/FR (i18n)
