# Quanta — Monnaie souveraine pair-à-pair

> **A sovereign peer-to-peer currency. No server, no bank, no mint authority.**
> Minez la monnaie du réseau, gardez-la avec vos clés, échangez de la valeur — entre pairs, sans tiers de confiance.

<p>
  <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-blue.svg">
  <img alt="Version" src="https://img.shields.io/badge/version-3.3.0-informational">
  <img alt="Backend" src="https://img.shields.io/badge/backend-Rust%20%2B%20Tauri%202-orange">
  <img alt="Frontend" src="https://img.shields.io/badge/frontend-Svelte%205-ff3e00">
  <img alt="Status" src="https://img.shields.io/badge/status-alpha-yellow">
</p>

**Quanta** est une application de bureau (macOS · Windows · Linux) qui fait tourner un nœud
d'un réseau pair-à-pair souverain. La pièce native est **QUANTA** (plus petite unité :
**µQTA**, 1 QUANTA = 1 000 000 µQTA) — **rare** (plafond dur 100M, émission décroissante) et
**post-quantique**. Tout est local et chiffré : votre identité, votre wallet, vos clés. Le
réseau remplace la banque.

---

## ⚠️ État du projet — à lire avant tout

Quanta est un **logiciel alpha de recherche**. Il est honnête de le dire clairement :

| ✅ Réel et testé | 🧪 Expérimental | ❌ Pas encore |
|---|---|---|
| Identité = adresse ML-DSA + vault chiffré (Argon2id + AES-256-GCM) | Oracle d'énergie (estimation watts) | Audit de sécurité tiers |
| Ledger µQTA déterministe (burn-and-mint, fork reorg, **plafond 100M**) | Anti-sybil (PoC : réputation + poids de stake) | Notarisation / signature OS officielle |
| Consensus Proof-of-Stake + VRF (BLAKE3) | NAT traversal multi-pairs à grande échelle | Réseau public ouvert à grande échelle |
| Transport Iroh QUIC + gossip signé (9 messages) | | Valeur / prix de marché (QUANTA n'est pas coté) |
| **Autorité de transaction ML-DSA-65 pure** (FIPS 204), transport Ed25519 | | |
| **Gadget de finalité** (votes, ⅔-stake, slashing) vérifié en simulation, gossip câblé en vivant | | |
| 379 tests automatisés, 0 `unsafe` | | |

- **P2P vérifié** entre deux machines physiques (mai 2026), pas (encore) à l'échelle.
- **Cryptographie expérimentale** : ne stockez aucune valeur réelle dessus.
- L'**autorité de transaction est du ML-DSA-65 pur** (NIST FIPS 204) : chaque transaction est
  signée par la clé liée à l'adresse ML-DSA de l'expéditeur ; Ed25519 reste le transport gossip
  — voir [Sécurité](#-sécurité).
- Un [audit interne 360°](audit/Torus-Audit-360.html) recense l'état réel, les écarts et la
  feuille de route. Ce README ne survend rien : ce qui est marqué expérimental l'est.

---

## Trois capacités

1. **Miner** — Gardez un nœud en ligne et gagnez des QUANTA selon votre **contribution réelle**
   (énergie mesurée + uptime + validation, pondération inspirée de Shapley). Émission **décroissante** vers
   un **plafond dur de 100M** (~120 QUANTA/h à la genèse), **zéro premine, zéro autorité d'émission**.
2. **Garder** — Votre identité est une paire de clés ; on vous joint par un court **`@pseudo`**.
   La clé privée ne quitte jamais l'appareil (vault Argon2id + AES-256-GCM) ; une clé de
   récupération restaure le compte. Aucun KYC, aucun tiers.
3. **Échanger** — Transférez des QUANTA entre wallets, autorisés par une signature **ML-DSA-65**
   (post-quantique) liée à l'adresse ML-DSA de l'expéditeur, avec un **burn de 1 %** à chaque
   transfert. Stakez pour sécuriser le réseau (PoS + VRF).

---

## Architecture

```
┌───────────────────────────────────────────────────────────┐
│                     Application Tauri 2                    │
│  ┌───────────────────────┐   ┌──────────────────────────┐ │
│  │   Frontend Svelte 5   │ ↔ │   Backend Rust (tokio)   │ │
│  │  Wallet · Network      │IPC│  P2P · Ledger · Consensus │ │
│  │  Profile · Settings   │   │  Mining · Staking         │ │
│  └───────────────────────┘   └────────────┬─────────────┘ │
└───────────────────────────────────────────┼───────────────┘
                                             │ Iroh QUIC + iroh-gossip (pub/sub signé)
        ┌────────────────────────────────────┼────────────────────────────────────┐
        ▼                                    ▼                                    ▼
 ┌──────────────┐                    ┌──────────────┐                    ┌──────────────┐
 │   Pair A     │                    │   Pair B     │                    │   Pair C     │
 │    Ledger    │ ◄── convergence ──►│    Ledger    │ ◄── convergence ──►│    Ledger    │
 │  (chaîne)    │   (CRDT + chaîne)  │  (chaîne)    │   (CRDT + chaîne)  │  (chaîne)    │
 └──────────────┘                    └──────────────┘                    └──────────────┘
```

**Couches du protocole** : Application → Protocole (`GossipMessage`, 9 variants) →
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
| Signatures | Ed25519 transport (ed25519-dalek) | 2.2 |
| Signatures | ML-DSA-65 autorité de transaction (fips204) | — |
| Chiffrement | AES-256-GCM | 0.10 |
| KDF | Argon2id (64 Mio, 3 itér., parallélisme 4) | 0.5 |
| Hashing | BLAKE3 | 1.8 |
| Mémoire | zeroize + ZeroizeOnDrop | 1.8 |
| Base de données | libSQL | 0.9 |
| CRDT | crdts (PN-Counter) | 7 |

---

## 🔐 Sécurité

- **Identité** : une adresse **ML-DSA** — `BLAKE3(ADDR_DOMAIN ‖ clé publique ML-DSA)` — c'est
  le `from`/`to` on-chain. La paire Ed25519 générée localement reste dans le vault comme
  identité de transport (PeerId) et comme **graine** dont la clé ML-DSA est dérivée
  (BLAKE3 XOF). Le secret est dérivé par **Argon2id** (64 Mio) et scellé en **AES-256-GCM**
  dans un vault sur disque. Les secrets sont **`zeroize`és** après usage (résistance
  cold-boot / memory dump).
- **Récupération** : à la création, l'utilisateur **doit** voir et confirmer sa clé de
  récupération (re-saisie du dernier bloc) avant d'entrer — pas de compte non sauvegardé.
- **Transport** : chaque message gossip est une `GossipEnvelope` signée Ed25519, horodatée
  (fenêtre ±90 s), avec un **nonce strictement monotone** par expéditeur (anti-replay).
  Rate-limiting adaptatif, dédup LRU 100 K, bannissement (3 signalements → 1 h), garde DoS
  (10 Mo max/enveloppe, 50 blocs max/segment), heuristique anti-éclipse.
- **Erreurs opaques** : un échec de déchiffrement renvoie « Invalid », jamais le type d'erreur
  réel. Aucune clé privée n'apparaît dans les logs, erreurs ou réponses JSON.
- **Post-quantique — actif (autorité ML-DSA pure)** : l'autorité de chaque **transaction**
  est une signature **ML-DSA-65** (NIST FIPS 204) obligatoire, de la clé liée à l'adresse de
  l'expéditeur, via le crate `fips204` (pur Rust, constant-time, sans `unsafe`). La clé
  ML-DSA est **dérivée de la graine Ed25519** (XOF BLAKE3) → aucun secret supplémentaire
  persisté, aucune migration de vault. Il n'y a **plus de repli Ed25519** pour l'autorité de
  transaction — la vérification est du ML-DSA pur, pas un AND hybride entre deux schémas.
  Les **enveloppes gossip** restent en Ed25519 (transport éphémère, fenêtre ±90 s, déjà sous
  QUIC/TLS) ; le passage « PQ obligatoire » réseau (`REQUIRE_PQ`) est un futur changement de
  version de protocole.

---

## 💎 Économie QUANTA

| Paramètre | Valeur |
|---|---|
| Plafond | **100 000 000 QUANTA**, dur, vérifié au consensus — jamais dépassable |
| Émission | **décroissante** : `(plafond − miné) / 50 000 000` µQTA par minute (~120 QUANTA/h à la genèse, baisse vers le plafond) |
| Premine / autorité d'émission | **aucun** |
| Unité | 1 QUANTA = 1 000 000 µQTA — arithmétique `u64`, zéro flottant sur les soldes |
| Distribution | pondération de contribution (inspirée de Shapley, **pas** un calcul exact) : énergie 30 · travail 30 · validation 25 · uptime 15 |
| Burn | **1 % détruit par transfert** (burn-and-mint ; *net*-déflationniste seulement au-delà d'un seuil de volume) |
| Anti-replay ledger | nonce monotone par compte + `seen_tx_hashes` |
| Forks | reorg déterministe : revert cache → re-queue des tx exclusives → push gagnant |

Le **rythme** d'émission est front-loaded mais borné : chaque tick libère une fraction fixe de
l'offre **restante**, donc le rythme baisse à l'approche du plafond et le total tend vers — sans
jamais atteindre — 100M. Mais les **montants absolus** prennent des siècles (≈1 % à l'an 1,
moitié du plafond vers ~66 ans) — voir le calendrier dans le [whitepaper](WHITEPAPER_FR.md#3).
Le terme *travail* suit l'énergie (pas de marché de calcul en crypto-only). La rareté est
**gravée dans le code et vérifiée au consensus** (borne par bloc + plafond global). Miner coûte
de l'électricité réelle, mais **un coût de production n'est pas un prix** : QUANTA n'est coté
nulle part et l'app n'affiche aucune valeur fiat inventée.

---

## 🚀 Démarrage

Prérequis : Node 18+, Rust stable, toolchain Tauri 2 (voir la doc Tauri pour les
dépendances système).

```bash
# Dev (hot-reload)
npm install
npm run tauri dev

# Tests backend (379 tests)
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
├── p2p/
│   ├── pos_consensus.rs   ← Élection de leader PoS (VRF BLAKE3)
│   ├── ledger.rs          ← Blockchain (seal, validate, fork reorg, cache O(1))
│   ├── gossip.rs          ← Protocole gossip (9 messages)
│   ├── dispatcher.rs      ← Pipeline de réception (verify → process → dispatch)
│   ├── willow_node.rs     ← Endpoint Iroh + stores + topic gossip
│   ├── reputation.rs      ← Moteur de minage + trust score
│   ├── shapley.rs         ← Pondération de contribution, inspirée de Shapley (énergie/validation/uptime)
│   ├── username.rs        ← Registre d'identité @pseudo
│   └── …                  ← consensus CRDT, mining_loop, energy, sybil, state_persistence
└── security/
    ├── mod.rs             ← CryptoEngine (Ed25519)
    ├── pq_vault.rs        ← Vault d'identité (Argon2id + AES-256-GCM)
    └── hybrid_crypto.rs   ← Autorité ML-DSA-65 (FIPS 204) + transport Ed25519 (actif)

src/                       ← Frontend Svelte 5 (Wallet, Network, Profile, Settings, …)
```

---

## 🗺️ Feuille de route

La feuille de route détaillée (priorisée par impact × effort) vit dans l'**audit interne** :
[`audit/Torus-Audit-360.html`](audit/Torus-Audit-360.html). En résumé :

- **Crédibilité** : doc fidèle au code, licence, marque unifiée *(en cours)*.
- **Sécurité/robustesse** : fuzzing du parseur d'enveloppes, durcissement, tests multi-nœuds.
- **Réseau** : convergence et résilience aux partitions testées en chaos (2+ nœuds).
- **Produit** : i18n (FR/EN), parcours d'onboarding, ergonomie du wallet.
- **Production** : pipeline de release signé + notarisation macOS.
- **Finalité** : gadget de finalité (votes, justify/finalize à ⅔ du stake, règle de slashing
  de l'équivocation) implémenté et vérifié en simulation déterministe ; gossip des votes
  câblé en vivant (LIVE-1) ; proposition de bloc finalité-consciente et slashing en vivant
  encore en cours de câblage.
- **Vision** : finalité BFT sous-seconde (design [DAG-BFT](docs/DESIGN-CONSENSUS-DAG-BFT.md) —
  un DAG de *consensus*, sans rapport avec le DAG de contenu social retiré),
  aléa d'élection durci par VDF, bascule réseau « PQ obligatoire » (`REQUIRE_PQ`).

---

## 📄 Documents

- [`WHITEPAPER.md`](WHITEPAPER.md) — whitepaper (EN).
- [`WHITEPAPER_FR.md`](WHITEPAPER_FR.md) — whitepaper (FR).
- [`CLAUDE.md`](CLAUDE.md) — référence technique interne (architecture, invariants).
- [`audit/Torus-Audit-360.html`](audit/Torus-Audit-360.html) — audit 360° + roadmap.

---

## Licence

Distribué sous licence **Apache-2.0** — voir [`LICENSE`](LICENSE) et [`NOTICE`](NOTICE).

> **Avertissement** : logiciel expérimental fourni « en l'état », sans garantie d'aucune
> sorte (voir la licence). La cryptographie et le réseau n'ont pas fait l'objet d'un audit
> indépendant. N'y stockez aucune valeur que vous ne pouvez pas vous permettre de perdre.

<p align="center"><strong>◈ Quanta — Energy Is Value ◈</strong></p>
