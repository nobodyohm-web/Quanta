# SOVA Protocol — Claude Code Instructions

> **Version** : V2 Pure Crypto | **Stack** : Rust (Tauri 2.0) + Svelte 5 | **Emission** : 100 SOVA/h fixe

## CRITICAL — Toujours lire en premier

- `.agent/memory.md` — Décisions, pièges, historique sessions
- `.agent/design/tech_references.md` — Shapley, CRDT, BME, RISC Zero, Iroh

## Mission

SOVA transforme l'énergie électrique en valeur numérique. Chaque ordinateur mine proportionnellement à ses watts mesurés. Les ressources inutilisées servent au calcul scientifique (IA, 3D, protéines). **Aucune fonctionnalité sociale** (feed, likes, contenu, profils).

## Stack (aucune déviation)

| Couche | Tech | Fichiers |
|--------|------|----------|
| Backend | Rust, Tauri 2.0, Edition 2021 | `src-tauri/src/` |
| Frontend | Svelte 5 (runes), SvelteKit | `src/` |
| CSS | Vanilla CSS, tokens | `src/app.css` |
| P2P | Iroh 0.98 (QUIC), iroh-gossip | `src-tauri/src/p2p/` |
| Crypto | Ed25519 + AES-256-GCM + BLAKE3 | `src-tauri/src/security/` |
| DB | libSQL (turso) | `src-tauri/src/storage/` |
| CRDT | `crdts` crate (PNCounter, GCounter) | `p2p/consensus.rs` |

## Règles absolues

1. **Émission fixe** : 100 SOVA/h, constant, pour toujours. JAMAIS de halving.
2. **Distribution** : proportionnelle aux watts → Shapley Value (30% énergie, 35% travail, 20% validation, 15% uptime)
3. **Burn** : 1% transfert, 2% tâche compute
4. **Rust** : `tokio::sync` (JAMAIS `std::sync` avec `.await`), zéro `unwrap()`, `Result<T,E>` + `?`
5. **Crypto** : `zeroize()` tous les secrets, Ed25519 sur chaque tx, erreurs opaques
6. **Frontend** : Svelte 5 runes UNIQUEMENT (`$state`, `$derived`, `$effect`), 3 vues (Wallet, Réseau, Réglages)
7. **Design** : Noir #000, surfaces #0f0f0f→#2a2a2a, accent #00DC82, Inter, grille 8px, AUCUN gradient/glow

## Architecture backend

```
src-tauri/src/
├── lib.rs              ← Commandes Tauri + mining loop
├── p2p/
│   ├── reputation.rs   ← Mining engine (émission fixe, uptime_tick)
│   ├── ledger.rs       ← Blockchain + transactions signées
│   ├── shapley.rs      ← Distribution Shapley Value
│   ├── marketplace.rs  ← Tâches compute distribuées
│   ├── consensus.rs    ← CRDT PN-Counters
│   ├── gossip.rs       ← Protocol gossip (Hello+watts, BroadcastTx)
│   ├── dispatcher.rs   ← Dispatch des messages entrants
│   ├── willow_node.rs  ← Iroh endpoint + topic
│   ├── merkle_dag.rs   ← DAG content-addressed (BLAKE3)
│   ├── energy.rs       ← Oracle énergie 33 pays
│   └── sybil.rs        ← Anti-sybil PoC
├── security/           ← PQ Vault, Ed25519, AES-256-GCM
└── storage/            ← libSQL persistence
```

## Commandes build

```bash
cargo check --manifest-path src-tauri/Cargo.toml    # Rust
cargo test --manifest-path src-tauri/Cargo.toml     # Tests
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run build                                        # Frontend
```

## Workflow

1. Lire ce fichier + `.agent/memory.md`
2. Entrer en Plan Mode pour les tâches >3 fichiers
3. Créer une branche `feat/xxx`
4. Implémenter → `cargo check` après chaque edit (hook automatique)
5. `cargo test` → corriger → itérer
6. Commiter : `feat|fix|refactor: description`
7. Mettre à jour `.agent/memory.md`

## Ressources détaillées (lire à la demande)

| Besoin | Fichier |
|--------|---------|
| Règles Rust | `.claude/rules/rust.md` |
| Règles sécurité crypto | `.claude/rules/security.md` |
| Règles frontend | `.claude/rules/frontend.md` |
| Vision V2 complète | `.agent/design/sova_v2_vision.md` |
| Innovations techniques | `.agent/design/sova_v2_innovations.md` |
| Refs techniques (Shapley, CRDT, BME, zkVM) | `.agent/design/tech_references.md` |
| Historique + leçons | `.agent/memory.md` |

## Agents spécialisés (`.claude/agents/`)

| Agent | Usage |
|-------|-------|
| `crypto-engineer` | Émission, Shapley, BME, CRDT |
| `p2p-engineer` | Iroh, gossip, DAG, consensus |
| `security-auditor` | Audit 25 points + checklist |
| `frontend-engineer` | Svelte 5, design, IPC |
| `test-engineer` | Tests unitaires + simulation |

## Commandes custom

| Commande | Action |
|----------|--------|
| `/implement` | Branch → code → test → commit |
| `/security-audit` | Scan crypto complet |
| `/v2-refactor` | Prochaine étape V2 |

## Dual-Model

- **Sonnet** : volume, refactoring, tests, cleanup
- **Opus 4.7** : architecture, Shapley, zkVM, audit sécu, debug complexe

Workflow : Opus `/plan` → Sonnet `/implement` → Sonnet `/verify` → Opus `/security-audit`
