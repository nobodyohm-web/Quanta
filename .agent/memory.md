# SOVA — Mémoire Persistante

> Ce fichier est lu par Claude Code au début de chaque session.
> Il contient les leçons apprises, décisions prises, et pièges à éviter.
> Mets à jour ce fichier à la fin de chaque session de travail.

---

## Décisions architecturales (avril 2026)

### V2 Pivot — Réseau social → Crypto pure
- **Date** : 29 avril 2026
- **Décision** : Suppression complète du réseau social (Feed, Editor, Browser, PostCard, likes, vues, contenu). SOVA est désormais un protocole crypto pur.
- **Raison** : Le réseau social diluait la proposition de valeur. Le cœur de SOVA = énergie → valeur → science.
- **Fichiers supprimés (frontend)** : Feed, Editor, Browser, PostCard, TemplatePicker, templates.ts, BadgeForge, ConstellationGraph, ActivityHeatmap, OrbitalAvatar, NotificationBell, UserProfile (12 fichiers)
- **Commandes Tauri supprimées** : create_site, update_site, get_all_sites, get_site, delete_site, record_view, burn_for_boost, get_site_token, get_leaderboard, like_content, validate_help, report_user, get_user_profile, get_notifications, mark_notifications_read (15 commandes)
- **Modules Rust encore présents mais legacy** : `attention.rs`, `notifications.rs` — encore dans `p2p/mod.rs` et `WillowNode`, à supprimer dans Phase 1

### Modèle économique V2
- **Émission** : 100 SOVA/heure fixe, pour toujours. PAS de halving. PAS de cap.
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
- Le gossip topic utilise `BLAKE3("sova-global-v1")` comme identifiant
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
  - `uptime_tick()` : en solo, reçoit 100% de l'émission (1.6667 SOVA/min)
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

### TODO Phase 3 — prochaines étapes
- [ ] `git init` + premier commit (le repo n'est pas initialisé)
- [ ] Gossiper `NodeContribution` (watts + tasks_completed + blocks_verified + uptime) dans les Hello messages — débloquer la proportionnalité Shapley
- [ ] Câbler `marketplace.submit_task` au ledger : escrow `reward_sova` du soumetteur, refund sur `expire_tasks`
- [ ] Câbler `marketplace.validate_and_pay` au ledger : transfer worker `reward - burn`, mine_tx pour le burn 2%
- [ ] Lier `tasks_completed` (depuis `marketplace.completed_by(pk)`) à `NodeContribution` dans `uptime_tick`
- [ ] DeSci DAO : 5% de l'émission vers un pool de financement scientifique
- [ ] ZK-Proof : guest RISC Zero pour prouver les watts (Phase 4)
