# Torus Protocol — Claude Code Instructions

> **Version** : V3 Social Web | **Stack** : Rust (Tauri 2.0) + Svelte 5 | **Coin** : QUANTA (émission 100/h fixe)

## CRITICAL — Toujours lire en premier

- `.agent/memory.md` — Décisions, pièges, historique sessions
- `.agent/design/tech_references.md` — Shapley, CRDT, BME, Iroh, Kleros, Harberger
- `.agent/design/v3_social_pivot.md` — Pivot V3 (Google P2P) — **LIRE EN PREMIER pour toute tâche V3**

## Mission V3

**Torus est un Web P2P alternatif où les humains se récompensent en publiant, modérant et découvrant.**

Trois capacités fondamentales :
1. **Créer** : n'importe qui publie un site (`torus://monsite`) en quelques clics, sans serveur, sans hébergeur, sans censeur.
2. **Découvrir** : un moteur de recherche P2P par mots-clés (pas par wallet) classe les sites avec **QuantaRank** (likes pondérés × abonnés × réputation × diversité).
3. **Récompenser** : likes, abonnements, tips et boosts payés en **QUANTA**. Le minage continue à 100 QUANTA/h fixe, redistribué via Shapley *modifié* (énergie + utilité sociale).

**L'ancienne contrainte « aucune fonctionnalité sociale » est levée.** V3 ouvre explicitement le social, mais conserve les invariants V2 (émission fixe, signatures Ed25519, lock ordering tokio).

## Stack (aucune déviation)

| Couche | Tech | Fichiers |
|--------|------|----------|
| Backend | Rust, Tauri 2.0, Edition 2021 | `src-tauri/src/` |
| Frontend | Svelte 5 (runes), SvelteKit | `src/` |
| CSS | Vanilla CSS, tokens | `src/app.css` |
| P2P transport | Iroh 0.98 (QUIC), iroh-gossip | `src-tauri/src/p2p/` |
| Crypto | Ed25519 + AES-256-GCM + BLAKE3 | `src-tauri/src/security/` |
| DB | libSQL (turso) | `src-tauri/src/storage/` |
| CRDT | `crdts` crate (PNCounter, GCounter) | `p2p/consensus.rs` |
| VRF (jurés) | `vrf` ou `schnorrkel` | `p2p/moderation.rs` |
| Recherche | Index inversé custom + tokenizer multilingue | `p2p/search.rs` |

## Règles absolues V3

1. **Émission fixe** : 100 QUANTA/h, constant, pour toujours. JAMAIS de halving.
2. **Distribution Shapley v2** : 25% énergie, 25% travail compute, 20% validation, 15% uptime, **15% utilité sociale (likes reçus pondérés)**
3. **Burn** : 1% transfert, 2% tâche compute, 5% boost, 10% slashing modération
4. **Rust** : `tokio::sync` (JAMAIS `std::sync` avec `.await`), zéro `unwrap()`, `Result<T,E>` + `?`
5. **Crypto** : `zeroize()` tous les secrets, Ed25519 sur chaque tx ET chaque action sociale (like, follow, report)
6. **Frontend** : Svelte 5 runes UNIQUEMENT (`$state`, `$derived`, `$effect`)
7. **Design** : Noir #000, surfaces #0f0f0f→#2a2a2a, accent #00DC82, Inter, grille 8px, AUCUN gradient/glow
8. **Pages** : sandbox iframe stricte, JS désactivé par défaut (toggle opt-in par site), CSP draconien, no-network sauf assets DAG
9. **Anti-Sybil obligatoire** : tout vote/like/report est pondéré par la réputation du voteur (âge wallet × uptime × QUANTA staké)
10. **Pas de stockage centralisé** : tout passe par DAG content-addressé BLAKE3, ledger CRDT, gossip Iroh

## Architecture backend V3

```
src-tauri/src/
├── lib.rs                ← Commandes Tauri (mining + publish + search + social + mod)
├── p2p/
│   ├── mod.rs            ← PeerInfo, exports
│   ├── reputation.rs     ← Mining engine + reputation_score (V3)
│   ├── ledger.rs         ← Blockchain + nonce + escrow + slashing
│   ├── shapley.rs        ← Distribution Shapley v2 (avec social_utility)
│   ├── marketplace.rs    ← Tâches compute + services humains
│   ├── consensus.rs      ← CRDT PN-Counters
│   ├── gossip.rs         ← Protocol gossip (Hello + Tx + Block + Page + Domain + Social + Report)
│   ├── dispatcher.rs     ← verify-before-process + nonce + handlers V3
│   ├── willow_node.rs    ← Iroh endpoint + tous les stores V3
│   ├── merkle_dag.rs     ← DAG content-addressed (BLAKE3) — assets pages
│   ├── energy.rs         ← Oracle énergie 33 pays
│   ├── sybil.rs          ← Anti-sybil PoC
│   ├── page_store.rs     ← Sites multi-pages + assets chunked (V3)
│   ├── domains.rs        ← V3 — Registre noms de domaine (Harberger)
│   ├── search.rs         ← V3 — Moteur de recherche (index inversé + QuantaRank)
│   ├── social.rs         ← V3 — Likes (quadratic), abonnements, tips, boost
│   ├── moderation.rs     ← V3 — Signalements + jury VRF + slashing
│   ├── forums.rs         ← V3 — Threads DAG, commentaires
│   └── trust_graph.rs    ← V3 — Web of Trust (PageRank personnalisé)
├── security/             ← PQ Vault, Ed25519, AES-256-GCM
└── storage/              ← libSQL persistence (snapshots de chaque store)
```

## Modules V3 — Spécifications

### `domains.rs` — Registre des noms

- Noms : `^[a-z0-9-]{2,40}\.torus$`, lowercase, ASCII
- **Claim** : premier inscrivant + paie un loyer initial (1 QUANTA)
- **Harberger Tax** : le propriétaire déclare une `value_qta` ; il paie `value_qta × 0.01 / mois` en loyer ; n'importe qui peut racheter à `value_qta`. Force la fixation honnête du prix → anti-squatting.
- **Sous-domaines** : `shop.alex.torus` délégués via signature du parent
- **Aliases** : un domaine pointe vers une `pk` ; le propriétaire peut transférer le pointeur
- **Période de grâce** : 30 jours après expiration, le nom reste réservé
- **Réservation** : noms < 2 chars + termes interdits (par jury) bloqués

### `search.rs` — Moteur P2P

- **Tokenizer** : Unicode-aware, normalisation NFKD + lowercase + stop-words FR/EN/ES
- **Index inversé local** : `HashMap<token, Vec<PostingEntry>>` où `PostingEntry = { cid, position, weight }`
- **Sharding DHT** : chaque pair stocke une tranche de l'index `hash(token) % N`
- **Réplication** : k=3 pairs par shard pour résilience
- **Ranking QuantaRank** :
  ```
  score = log(1 + likes_pondérés)
        × log(1 + abonnés_créateur)
        × reputation_créateur^0.5
        × freshness_decay(updated_at)
        × diversity_bonus     # pénalise sur-représentation auteur dans page de résultats
  ```
- **Filtres** : `lang`, `since`, `type` (`site`, `forum`, `shop`, `blog`)
- **Anti-spam** : un mot-clé ne peut pas apparaître > 5× dans les meta d'un site (sinon ignoré)

### `social.rs` — Économie d'attention

- **Like / Dislike** = transaction signée `Social { target_cid, weight, kind }`
- **Quadratic voting** : un user peut dépenser N QUANTA sur un like → influence = √N. Plus cher d'amplifier que de diversifier.
- **Abonnement** : `Follow { followee_pk, tier }`. Tier 1 = gratuit (signal). Tier 2 = 1 QUANTA/mois (boost mining du créateur de 5%). Tier 3 = 10 QUANTA/mois (15%).
- **Tip** : transfert direct avec mémo (`tx_type = Tip`)
- **Boost** : payer X QUANTA pour multiplier ranking ×1.5 pendant 24h (cap : 100 QUANTA/page/jour) ; 5% burn
- **Sponsoring** : flux récurrent créateur → créateur, déductible du mining sponsorisé

### `moderation.rs` — Justice décentralisée

- **Report** = `{ target_cid, category, evidence_cid }`, signé, coût 0.1 QUANTA (anti-spam)
- **Catégories** : `Spam`, `Scam`, `IllegalContent`, `Harassment`, `Impersonation`, `Other`
- **Seuil de jury** : 5 reports indépendants (5 wallets distincts, âge > 7j, uptime > 50%)
- **Sélection jury** : 7 jurés via VRF (`schnorrkel`), tirés du pool des wallets ayant staké ≥ 100 QUANTA et reputation > 0.6
- **Vote** : 24h, majorité simple, vote scellé (commit-reveal)
- **Verdicts** :
  - `Innocent` : reporter slashed (-0.1 QUANTA × 5)
  - `Warning` : -10% mining 7j
  - `Hide` : vitrine masquée 30j, -50% mining 30j
  - `Ban` : vitrine masquée définitivement (récupérable par appel à un super-jury 21 jurés, coût 50 QUANTA)
- **Récompense jurés** : 0.5 QUANTA si vote = majorité, 0 sinon (Schelling point)

### `forums.rs` — Discussions

- Forum = nœud racine signé (`name`, `description`, `creator_pk`)
- Thread = enfant du forum, avec `title`, `body_cid`
- Comment = enfant d'un thread ou d'un comment (DAG)
- Like/dislike + report par nœud
- **Soft-fork** : un user peut copier un thread + l'embrancher différemment (clone signé, lien vers original)

### `trust_graph.rs` — Web of Trust

- Chaque `Follow` est une arête signée
- Calcul local : PageRank personnalisé partant de soi (damping 0.85, 20 itérations max)
- Donne un score de confiance par wallet, utilisé pour pondérer likes/reports

### Anti-troll graduel (dans `reputation.rs`)

```
reports_validés_30j  →  malus mining
   1                 →  warning
   3                 →  -10%
   5                 →  -25%
   8                 →  -50%
  12                 →  -100% (vitrine masquée)
```

Récupération : +1% par like positif validé (cap restauration : reset après 30j sans report).

## Phase B/C/D (V2) — toujours valides

Pipeline sécurité gossip : voir `gossip.rs:dispatcher`. Les nouveaux messages V3 (`Domain*`, `Search*`, `Social*`, `Report*`, `Forum*`) **DOIVENT** passer par le même pipeline (sig + nonce + freshness + dedup).

## Constantes V3

```rust
// Domains
pub const DOMAIN_INITIAL_FEE_MICRO_QTA:  u64 = 1_000_000;            // 1 QUANTA
pub const DOMAIN_HARBERGER_RATE_BPS:     u32 = 100;                  // 1% / mois sur value
pub const DOMAIN_GRACE_PERIOD_SECS:      u64 = 30 * 86_400;          // 30 jours

// Social
pub const LIKE_BASE_COST_MICRO_QTA:      u64 = 100_000;              // 0.1 QTA
pub const FOLLOW_TIER2_MICRO_QTA:        u64 = 1_000_000;            // 1 QTA / mois
pub const FOLLOW_TIER3_MICRO_QTA:        u64 = 10_000_000;           // 10 QTA / mois
pub const BOOST_BURN_BPS:                u32 = 500;                  // 5% burn
pub const BOOST_MAX_DAILY_MICRO_QTA:     u64 = 100_000_000;          // 100 QTA/page/jour

// Moderation
pub const REPORT_COST_MICRO_QTA:         u64 = 100_000;              // 0.1 QTA
pub const JURY_SIZE:                     usize = 7;
pub const JURY_STAKE_MIN_MICRO_QTA:      u64 = 100_000_000;          // 100 QTA
pub const JURY_REWARD_MICRO_QTA:         u64 = 500_000;              // 0.5 QTA
pub const SLASH_HIDE_BPS:                u32 = 1000;                 // 10% slash propriétaire
pub const SLASH_DURATION_SECS:           u64 = 30 * 86_400;          // 30 jours

// Search
pub const SEARCH_TOKEN_MAX_REPETITION:   u32 = 5;
pub const SEARCH_SHARD_REPLICATION:      usize = 3;
```

## Commandes build

```bash
cargo check  --manifest-path src-tauri/Cargo.toml
cargo test   --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run build                                       # Frontend SvelteKit
npm run tauri dev                                   # App desktop
```

## Workflow V3

1. Lire `CLAUDE.md` + `.agent/memory.md` + `.agent/design/v3_social_pivot.md`
2. Plan Mode si tâche > 3 fichiers
3. Branche `feat/v3-<module>`
4. Implémenter → `cargo check` après chaque edit
5. **Tests obligatoires** pour chaque module V3 (anti-Sybil, signatures, replay)
6. `cargo test` + `cargo clippy -D warnings` doivent passer
7. Commit : `feat(v3): description`
8. Mettre à jour `.agent/memory.md`

## Ressources

| Besoin | Fichier |
|--------|---------|
| Règles Rust | `.claude/rules/rust.md` |
| Règles sécurité crypto | `.claude/rules/security.md` |
| Règles frontend | `.claude/rules/frontend.md` |
| Vision V3 complète | `.agent/design/v3_social_pivot.md` |
| Whitepaper V3 (FR) | `WHITEPAPER_FR.md` |
| Whitepaper V3 (EN) | `WHITEPAPER.md` |
| Refs techniques (Shapley, CRDT, Harberger, VRF, PageRank) | `.agent/design/tech_references.md` |
| Historique + leçons | `.agent/memory.md` |

## Agents spécialisés (`.claude/agents/`)

| Agent | Usage V3 |
|-------|----------|
| `crypto-engineer` | Émission, Shapley v2, quadratic voting, slashing |
| `p2p-engineer` | Iroh, gossip V3, DHT search, DAG sites |
| `security-auditor` | Anti-Sybil, modération, jury VRF, audit crypto |
| `frontend-engineer` | Svelte 5, PageBuilder, Browser P2P, IPC |
| `test-engineer` | Tests unitaires + simulation réseau + fuzz inputs |

## Dual-Model

- **Sonnet** : volume, refactoring, tests, cleanup
- **Opus 4.7** : architecture, Shapley v2, modération VRF, audit sécu, debug complexe

Workflow : Opus `/plan` → Sonnet `/implement` → Sonnet `/verify` → Opus `/security-audit`
