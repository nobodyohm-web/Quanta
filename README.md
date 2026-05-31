# Quanta — Le Web P2P souverain

> **A sovereign peer-to-peer web and value network. No server, no cloud, no intermediary.**
> Créez un site, minez la monnaie du réseau, échangez de la valeur — entre pairs, sans tiers de confiance.

<p>
  <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-blue.svg">
  <img alt="Version" src="https://img.shields.io/badge/version-3.3.0-informational">
  <img alt="Backend" src="https://img.shields.io/badge/backend-Rust%20%2B%20Tauri%202-orange">
  <img alt="Frontend" src="https://img.shields.io/badge/frontend-Svelte%205-ff3e00">
  <img alt="Status" src="https://img.shields.io/badge/status-alpha-yellow">
</p>

**Quanta** est une application de bureau (macOS · Windows · Linux) qui fait tourner un nœud
d'un réseau pair-à-pair souverain. La pièce native est **QUANTA** (plus petite unité :
**µQTA**, 1 QUANTA = 1 000 000 µQTA). Tout est local et chiffré : votre identité, votre
wallet, vos sites. Le réseau remplace le serveur.

---

## ⚠️ État du projet — à lire avant tout

Quanta est un **logiciel alpha de recherche**. Il est honnête de le dire clairement :

| ✅ Réel et testé | 🧪 Expérimental | ❌ Pas encore |
|---|---|---|
| Identité Ed25519 + vault chiffré (Argon2id + AES-256-GCM) | Marketplace de calcul distribué | Audit de sécurité tiers |
| Ledger µQTA déterministe (burn-and-mint, fork reorg) | Oracle d'énergie (estimation watts) | Notarisation / signature OS officielle |
| Consensus Proof-of-Stake + VRF (BLAKE3) | Web-of-Trust (PageRank personnalisé) | Valeur monétaire réelle (ne pas spéculer) |
| Transport Iroh QUIC + gossip signé (22 messages) | Modération par jury VRF + slashing | Signatures post-quantiques **actives** |
| PageBuilder no-code + publication P2P | NAT traversal multi-pairs à grande échelle | Réseau public ouvert à grande échelle |
| 265 tests automatisés, 0 `unsafe` | | |

- **P2P vérifié** entre deux machines physiques (mai 2026), pas (encore) à l'échelle.
- **Cryptographie expérimentale** : ne stockez aucune valeur réelle dessus.
- Les **signatures post-quantiques sont actives** : chaque transaction est signée en
  **hybride Ed25519 + ML-DSA-65 (NIST FIPS 204)** — voir [Sécurité](#-sécurité).
- Un [audit interne 360°](audit/Torus-Audit-360.html) recense l'état réel, les écarts et la
  feuille de route. Ce README ne survend rien : ce qui est marqué expérimental l'est.

---

## Trois capacités

1. **Créer** — Publiez un site HTML/CSS via un **builder no-code à blocs** (ou en HTML brut).
   Le site est *content-addressed* (BLAKE3) et diffusé aux pairs ; il reste accessible tant
   qu'au moins un pair le réplique. Réservez un nom `*.torus` (registre Harberger).
2. **Miner** — Gagnez des QUANTA en contribuant au réseau (uptime + énergie estimée).
   Émission fixe **100 QUANTA/heure** répartie entre les pairs actifs (distribution Shapley).
3. **Échanger** — Transférez des QUANTA entre wallets, signés Ed25519, avec un burn de 1 %.
   Likez (vote quadratique), abonnez-vous, tippez des créateurs, modérez via jury.

---

## Architecture

```
┌───────────────────────────────────────────────────────────┐
│                     Application Tauri 2                    │
│  ┌───────────────────────┐   ┌──────────────────────────┐ │
│  │   Frontend Svelte 5   │ ↔ │   Backend Rust (tokio)   │ │
│  │  Browser · Builder    │IPC│  P2P · Ledger · Consensus │ │
│  │  Search · Forums      │   │  Search · Social · Mod    │ │
│  │  Wallet · Network      │   │  Domains · Mining         │ │
│  └───────────────────────┘   └────────────┬─────────────┘ │
└───────────────────────────────────────────┼───────────────┘
                                             │ Iroh QUIC + iroh-gossip (pub/sub signé)
        ┌────────────────────────────────────┼────────────────────────────────────┐
        ▼                                    ▼                                    ▼
 ┌──────────────┐                    ┌──────────────┐                    ┌──────────────┐
 │   Pair A     │                    │   Pair B     │                    │   Pair C     │
 │ Ledger · DAG │ ◄── convergence ──►│ Ledger · DAG │ ◄── convergence ──►│ Ledger · DAG │
 │ Index · Pages│   (CRDT + chaîne)  │ Index · Pages│   (CRDT + chaîne)  │ Index · Pages│
 └──────────────┘                    └──────────────┘                    └──────────────┘
```

**Couches du protocole** : Application → Protocole (`GossipMessage`, 22 variants) →
Sécurité (`GossipEnvelope` : Ed25519 + nonce monotone + timestamp ±90 s) → Transport
(Iroh QUIC + gossip) → Réseau (NAT traversal + relais).

**Pipeline d'entrée gossip** (chaque message) : taille (≤ 10 Mo) → désérialisation → ban →
dédup (LRU 100 K) → fraîcheur timestamp (±90 s) → rate-limit adaptatif → **nonce anti-replay
(≥ 1, strictement monotone)** → **vérification de signature Ed25519** → handler.

---

## Stack

| Couche | Technologie | Version |
|---|---|---|
| Desktop | Tauri | 2 |
| Backend | Rust (edition 2021) | — |
| Frontend | Svelte 5 (runes) + SvelteKit | 5 |
| Styles | CSS vanilla (tokens), accent `#00DC82` | — |
| Transport P2P | iroh / iroh-gossip / iroh-blobs | 0.98 / 0.98 / 0.100 |
| Consensus | Proof-of-Stake + VRF (BLAKE3) | — |
| Signatures | Ed25519 (ed25519-dalek) | 2.2 |
| Chiffrement | AES-256-GCM | 0.10 |
| KDF | Argon2id (64 Mio, 3 itér., parallélisme 4) | 0.5 |
| Hashing | BLAKE3 | 1.8 |
| Mémoire | zeroize + ZeroizeOnDrop | 1.8 |
| Base de données | libSQL | 0.9 |
| CRDT | crdts (PN-Counter) | 7 |

---

## 🔐 Sécurité

- **Identité** : clé Ed25519 générée localement. Le secret est dérivé par **Argon2id**
  (64 Mio) et scellé en **AES-256-GCM** dans un vault sur disque. Les secrets sont
  **`zeroize`és** après usage (résistance cold-boot / memory dump).
- **Récupération** : à la création, l'utilisateur **doit** voir et confirmer sa clé de
  récupération (re-saisie du dernier bloc) avant d'entrer — pas de compte non sauvegardé.
- **Transport** : chaque message gossip est une `GossipEnvelope` signée Ed25519, horodatée
  (fenêtre ±90 s), avec un **nonce strictement monotone** par expéditeur (anti-replay).
  Rate-limiting adaptatif, dédup LRU 100 K, bannissement (3 signalements → 1 h), garde DoS
  (10 Mo max/enveloppe, 50 blocs max/segment), heuristique anti-éclipse.
- **Erreurs opaques** : un échec de déchiffrement renvoie « Invalid », jamais le type d'erreur
  réel. Aucune clé privée n'apparaît dans les logs, erreurs ou réponses JSON.
- **Post-quantique — actif** : chaque **transaction** est signée en hybride
  **Ed25519 + ML-DSA-65** (NIST FIPS 204) via le crate `fips204` (pur Rust, constant-time,
  sans `unsafe`). La clé ML-DSA est **dérivée de la graine Ed25519** (XOF BLAKE3) → aucun
  secret supplémentaire persisté, aucune migration de vault. La vérification est en **AND
  strict** quand la couche PQ est présente (forger exige de casser *les deux* schémas), avec
  repli Ed25519 pour les signatures antérieures. Les **enveloppes gossip** restent en
  Ed25519 (transport éphémère, fenêtre ±90 s, déjà sous QUIC/TLS) ; le passage « PQ
  obligatoire » réseau (`REQUIRE_PQ`) est un futur changement de version de protocole.

---

## 💎 Économie QUANTA

| Paramètre | Valeur |
|---|---|
| Émission | **100 QUANTA / heure**, fixe, à perpétuité (pas de halving, pas de cap) |
| Unité | 1 QUANTA = 1 000 000 µQTA — arithmétique `u64`, zéro flottant sur les soldes |
| Distribution | Shapley : énergie · travail compute · validation · uptime · utilité sociale |
| Burn | 1 % par transfert (burn-and-mint), + frais sur boosts et slashing modération |
| Anti-replay ledger | nonce monotone par compte + `seen_tx_hashes` |
| Forks | reorg déterministe : revert cache → re-queue des tx exclusives → push gagnant |

L'inflation **nominale** est fixe ; l'inflation **réelle** tend vers zéro quand l'usage
(donc le burn) augmente — sans privilégier les early-adopters.

---

## 🚀 Démarrage

Prérequis : Node 18+, Rust stable, toolchain Tauri 2 (voir la doc Tauri pour les
dépendances système).

```bash
# Dev (hot-reload)
npm install
npm run tauri dev

# Tests backend (256 tests)
cargo test --manifest-path src-tauri/Cargo.toml

# Lint (zéro warning toléré)
cd src-tauri && cargo clippy -- -D warnings

# Build de production (Apple Silicon)
npx tauri build
# → src-tauri/target/release/bundle/dmg/Quanta_3.3.0_aarch64.dmg
```

Sur macOS, le binaire **n'est pas (encore) notarisé** : `xattr -cr /Applications/Quanta.app`
puis clic droit → Ouvrir. La notarisation officielle est sur la feuille de route.

---

## 🗂️ Structure

```
src-tauri/src/
├── lib.rs                 ← Commandes Tauri (IPC frontend ↔ backend)
├── commands_v3.rs         ← Commandes V3 (social, domaines, forums)
├── dev_api.rs             ← API HTTP dev loopback (127.0.0.1:7654, désactivée par défaut)
├── p2p/
│   ├── pos_consensus.rs   ← Élection de leader PoS (VRF BLAKE3)
│   ├── ledger.rs          ← Blockchain (seal, validate, fork reorg, cache O(1))
│   ├── gossip.rs          ← Protocole gossip (22 messages)
│   ├── dispatcher.rs      ← Pipeline de réception (verify → process → dispatch)
│   ├── willow_node.rs     ← Endpoint Iroh + stores + topic gossip
│   ├── search.rs          ← BM25 + QuantaRank (boost signaux sociaux)
│   ├── domains.rs         ← Registre de noms *.torus (taxe Harberger)
│   ├── social.rs          ← Likes (quadratique), follows, tips
│   ├── moderation.rs      ← Signalements + jury VRF + slashing
│   ├── trust_graph.rs     ← Web-of-Trust (PageRank personnalisé)
│   └── …                  ← consensus CRDT, reputation, energy, sybil, marketplace, merkle_dag
└── security/
    ├── mod.rs             ← CryptoEngine (Ed25519)
    ├── pq_vault.rs        ← Vault d'identité (Argon2id + AES-256-GCM)
    └── hybrid_crypto.rs   ← Signatures hybrides Ed25519 + ML-DSA-65 (FIPS 204, actif)

src/                       ← Frontend Svelte 5 (Browser, PageBuilder, Wallet, Network, …)
```

---

## 🗺️ Feuille de route

La feuille de route détaillée (priorisée par impact × effort) vit dans l'**audit interne** :
[`audit/Torus-Audit-360.html`](audit/Torus-Audit-360.html). En résumé :

- **Crédibilité** : doc fidèle au code, licence, marque unifiée *(en cours)*.
- **Sécurité/robustesse** : fuzzing du parseur d'enveloppes, durcissement, tests multi-nœuds.
- **Réseau** : convergence et résilience aux partitions testées en chaos (2+ nœuds).
- **Produit** : i18n (FR/EN), refactor du PageBuilder, parcours d'onboarding.
- **Production** : pipeline de release signé + notarisation macOS.
- **Vision** : finalité BFT sous-seconde (design [DAG-BFT](docs/DESIGN-CONSENSUS-DAG-BFT.md)),
  aléa d'élection durci par VDF, bascule réseau « PQ obligatoire » (`REQUIRE_PQ`).

---

## 📄 Documents

- [`WHITEPAPER.md`](WHITEPAPER.md) — whitepaper (EN).
- [`WHITEPAPER_FR.md`](WHITEPAPER_FR.md) — whitepaper (FR).
- [`DEV_API.md`](DEV_API.md) — API HTTP développeur (publication externe).
- [`CLAUDE.md`](CLAUDE.md) — référence technique interne (architecture, invariants).
- [`audit/Torus-Audit-360.html`](audit/Torus-Audit-360.html) — audit 360° + roadmap.

---

## Licence

Distribué sous licence **Apache-2.0** — voir [`LICENSE`](LICENSE) et [`NOTICE`](NOTICE).

> **Avertissement** : logiciel expérimental fourni « en l'état », sans garantie d'aucune
> sorte (voir la licence). La cryptographie et le réseau n'ont pas fait l'objet d'un audit
> indépendant. N'y stockez aucune valeur que vous ne pouvez pas vous permettre de perdre.

<p align="center"><strong>◈ Quanta — Energy Is Value ◈</strong></p>
