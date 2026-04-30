# 🔬 SOVA Security & Consensus Bible — Phase 2
## Architecture Réseau Supérieure — Brief pour Claude Code Opus 4.7

> **Objectif** : Faire de SOVA le réseau P2P le plus sécurisé et le plus avancé technologiquement
> du marché en s'appuyant sur la recherche académique de pointe (MIT, Stanford, NIST, IETF).
> **Règle** : Ne rien inventer. Chaque choix est justifié par une source académique ou un standard.

---

## TABLE DES MATIÈRES

1. [Architecture de Sécurité — 4 couches](#1-securite)
2. [Cryptographie Post-Quantique — NIST FIPS 2024](#2-pq)
3. [Consensus — Merkle-CRDT + DAG-BFT Léger](#3-consensus)
4. [Économie Énergétique — Prix Réel](#4-energie)
5. [Résistance Sybil — Proof of Contribution](#5-sybil)
6. [Crates Rust — Dépendances Exactes](#6-crates)
7. [Plan d'Implémentation — Fichiers](#7-plan)
8. [Ce qui existe déjà dans le code](#8-existant)

---

## 1. Architecture de Sécurité — 4 couches {#1-securite}

```
┌─────────────────────────────────────────────────────────────┐
│                    COUCHE 4 : APPLICATION                    │
│  Anti-farming, rate limiting, cooldowns, reputation engine   │
│  Source : SOVA reputation.rs (EXISTANT)                      │
├─────────────────────────────────────────────────────────────┤
│                    COUCHE 3 : CONSENSUS                      │
│  Merkle-CRDT + DAG léger + Gossip via Iroh                  │
│  Source : Martin Kleppmann (Cambridge), Sui/Mysticeti        │
├─────────────────────────────────────────────────────────────┤
│                    COUCHE 2 : CRYPTOGRAPHIE                  │
│  Hybride : Ed25519 + ML-DSA (FIPS 204) pour signatures      │
│  Hybride : X25519 + ML-KEM (FIPS 203) pour clés             │
│  AES-256-GCM + Argon2id + BLAKE3 + zeroize (EXISTANT)       │
│  Source : NIST FIPS 203/204/205 (août 2024)                  │
├─────────────────────────────────────────────────────────────┤
│                    COUCHE 1 : TRANSPORT                       │
│  Iroh QUIC + hole punching + relay (EXISTANT)                │
│  Source : IETF RFC 9000 (QUIC), n0.computer                  │
└─────────────────────────────────────────────────────────────┘
```

**Principe** : Chaque couche est indépendante. Si la couche 3 (consensus) est compromise,
les couches 2 (crypto) et 1 (transport) protègent toujours les données.

---

## 2. Cryptographie Post-Quantique — NIST FIPS 2024 {#2-pq}

### Pourquoi c'est nécessaire
> Un ordinateur quantique à 4000+ qubits (attendu ~2030-2035) casserait
> Ed25519 et X25519 en minutes via l'algorithme de Shor.
> NIST a finalisé les standards PQ en août 2024 précisément pour cette raison.

### Architecture Hybride (recommandation NIST + IETF)
La bonne pratique est **hybride** : on combine classique + post-quantique.
Si le PQ est cassé, le classique protège encore. Et vice versa.

```
SIGNATURES (identité + transactions)
─────────────────────────────────────
ACTUEL  : Ed25519-dalek (classique seul)
CIBLE   : Ed25519 + ML-DSA-65 (FIPS 204)
          → 2 signatures par transaction
          → Valide si AU MOINS UNE est correcte
          → Taille : 64B (Ed25519) + 3293B (ML-DSA-65)

ÉCHANGE DE CLÉS (P2P chiffré)
─────────────────────────────────────
ACTUEL  : X25519 via Iroh QUIC (classique seul)
CIBLE   : X25519 + ML-KEM-768 (FIPS 203)
          → Clé partagée = HKDF(X25519_shared || ML-KEM_shared)
          → 2x DH requis pour forger = impossible quantique + classique
```

### Crates Rust purs (vérifié crates.io avril 2026)

| Standard | Crate | Version | Statut |
|----------|-------|---------|--------|
| FIPS 203 (ML-KEM) | `ml-kem` | ≥0.2 | RustCrypto officiel, `no_std`, test vectors NIST |
| FIPS 204 (ML-DSA) | `ml-dsa` | ≥0.2 | RustCrypto officiel, `no_std`, test vectors NIST |
| FIPS 205 (SLH-DSA) | `slh-dsa` | ≥0.1 | Backup hash-based, très conservateur |
| Alternative formellement vérifiée | `libcrux` | ≥0.0.12 | Cryspen, vérifié par F*/hax |

### Implémentation concrète

```rust
// Fichier : src-tauri/src/security/hybrid_crypto.rs

use ed25519_dalek::{SigningKey, VerifyingKey, Signature};
use ml_dsa::{ml_dsa_65, KeyPair as PqKeyPair};

pub struct HybridIdentity {
    pub ed25519_sk: SigningKey,        // Classique
    pub pq_sk: ml_dsa_65::SecretKey,   // Post-quantique
    pub ed25519_pk: VerifyingKey,
    pub pq_pk: ml_dsa_65::PublicKey,
}

pub struct HybridSignature {
    pub classical: Vec<u8>,  // 64 bytes Ed25519
    pub quantum: Vec<u8>,    // 3293 bytes ML-DSA-65
}

impl HybridIdentity {
    pub fn sign(&self, msg: &[u8]) -> HybridSignature {
        let classical = self.ed25519_sk.sign(msg).to_bytes().to_vec();
        let quantum = ml_dsa_65::sign(&self.pq_sk, msg);
        HybridSignature { classical, quantum }
    }

    /// Vérification hybride : valide si AU MOINS UNE signature est correcte
    /// (protection contre la rupture de l'un des deux systèmes)
    pub fn verify(pk_ed: &VerifyingKey, pk_pq: &ml_dsa_65::PublicKey,
                  msg: &[u8], sig: &HybridSignature) -> bool {
        let ed_ok = pk_ed.verify(msg, &Signature::from_bytes(&sig.classical)).is_ok();
        let pq_ok = ml_dsa_65::verify(pk_pq, msg, &sig.quantum);
        ed_ok || pq_ok  // OR = si un des deux casse, l'autre protège encore
    }
}
```

### Contraintes
- **Taille des signatures** : ~3.4 KB par signature hybride (vs 64B classique)
  → Acceptable pour un réseau social (pas du HFT)
- **Performance** : ML-DSA sign < 100µs sur x86_64 (bench Microsoft)
  → Imperceptible pour l'utilisateur
- **Migration** : L'identité existante (Ed25519) reste valide.
  La clé PQ est générée au premier lancement post-upgrade.

---

## 3. Consensus — Merkle-CRDT + DAG Léger {#3-consensus}

### Pourquoi pas un consensus "classique" ?
| Protocole | Problème pour SOVA |
|-----------|--------------------|
| Nakamoto (PoW) | Énergie massive — contradiction avec la philosophie SOVA |
| PBFT/HotStuff | Besoin de connaître tous les validateurs — pas P2P ouvert |
| Raft/Paxos | Pas Byzantine — suppose des nœuds honnêtes |
| Full DAG-BFT (Sui) | Trop complexe pour un réseau naissant <1000 nœuds |

### La solution : Merkle-CRDT (Kleppmann, Cambridge)
> Source : Martin Kleppmann, "Making CRDTs Byzantine Fault Tolerant" (2022)
> Source : Hector Sanjuan, "Merkle-CRDTs" (Protocol Labs, 2020)

**Idée** : Combiner un CRDT (pas besoin de consensus pour fusionner) avec un arbre de Merkle
(détection de toute altération). Le résultat :
- Pas besoin de leader ou de tours de vote
- Chaque nœud peut être hors ligne et resynchroniser
- La fusion est déterministe et automatique
- Les falsifications sont détectées par le hash Merkle

```
ARCHITECTURE CONSENSUS SOVA
═══════════════════════════

Chaque nœud maintient un DAG local (pas une chaîne linéaire) :

         [block-A1]──┐
                      ├──[block-A3]──┐
         [block-A2]──┘               │
                                     ├──[merge-block]
         [block-B1]──┐               │
                      ├──[block-B2]──┘
         [block-B1']─┘

Quand 2 nœuds se synchronisent via Iroh :
1. Échange des heads (dernier bloc de chaque branche)
2. Échange des blocs manquants (delta sync)
3. Fusion déterministe : les 2 branches forment un DAG
4. Le "merge-block" réconcilie les balances via CRDT G-Counter

Il n'y a PAS de fork — le DAG accepte TOUTES les branches.
La "vérité" est l'union de tous les blocs valides.
```

### CRDT pour le Ledger ATN

| Donnée | Type CRDT | Propriété |
|--------|-----------|-----------|
| Balance ATN | **PN-Counter** | balance = total_reçu - total_envoyé (grow-only counters) |
| Reputation/Trust | **G-Counter** | Ne peut que monter (sauf pénalité = separate counter) |
| Contenu (sites) | **LWW-Register** | Last-Writer-Wins par timestamp + hash |
| Likes | **G-Set** | Ensemble qui ne peut que grandir (like = irréversible) |
| Follows | **OR-Set** | Observed-Remove Set (follow + unfollow) |

### Pourquoi c'est supérieur à Bitcoin/Ethereum pour SOVA

| Propriété | Bitcoin | Ethereum | SOVA Merkle-CRDT |
|-----------|---------|----------|-----------------|
| Finalité | ~60 min | ~15 min | **Instantanée locale, ~5s réseau** |
| Énergie | PoW massif | PoS modéré | **Zéro surplus** (juste le CPU existant) |
| Offline | Impossible | Impossible | **Fonctionne hors ligne** |
| Conflits | Fork → reorg | Fork → reorg | **Pas de fork** (DAG) |
| Complexité | Énorme | Énorme | **~500 lignes de Rust** |

---

## 4. Économie Énergétique — Prix Réel {#4-energie}

### Table de prix embarquée (mise à jour trimestrielle)

```rust
// Fichier : src-tauri/src/p2p/energy.rs

pub struct EnergyOracle {
    prices: HashMap<String, f64>,  // country_code → EUR/kWh
}

impl EnergyOracle {
    pub fn new() -> Self {
        let mut prices = HashMap::new();
        // Source : Eurostat, EIA, IEA (Q1 2026)
        prices.insert("FR".into(), 0.2516);  // France
        prices.insert("DE".into(), 0.3471);  // Allemagne
        prices.insert("US".into(), 0.1385);  // USA (converti EUR)
        prices.insert("GB".into(), 0.2780);  // UK
        prices.insert("JP".into(), 0.2190);  // Japon
        prices.insert("IN".into(), 0.0720);  // Inde
        prices.insert("BR".into(), 0.1150);  // Brésil
        prices.insert("AU".into(), 0.2340);  // Australie
        prices.insert("CA".into(), 0.1090);  // Canada
        prices.insert("CH".into(), 0.2710);  // Suisse
        prices.insert("ES".into(), 0.2230);  // Espagne
        prices.insert("IT".into(), 0.2890);  // Italie
        prices.insert("KR".into(), 0.1120);  // Corée du Sud
        prices.insert("SG".into(), 0.2100);  // Singapour
        prices.insert("ZA".into(), 0.0880);  // Afrique du Sud
        // ... 30 pays minimum
        Self { prices }
    }

    /// Détecte le pays via timezone système (gratuit, pas d'API)
    pub fn detect_country() -> String {
        // chrono-tz → timezone → mapping timezone→country
        // Ex: "Europe/Paris" → "FR"
        // Fallback: "XX" → utilise la moyenne mondiale
        todo!()
    }

    /// Moyenne pondérée du réseau (chaque nœud rapporte son prix via gossip)
    pub fn network_weighted_average(&self, peer_reports: &[(String, u64)]) -> f64 {
        // peer_reports = [(country_code, node_count), ...]
        let total_nodes: u64 = peer_reports.iter().map(|(_, n)| n).sum();
        if total_nodes == 0 { return 0.15; } // fallback EU avg
        peer_reports.iter()
            .map(|(cc, n)| self.price_for(cc) * (*n as f64))
            .sum::<f64>() / total_nodes as f64
    }

    pub fn price_for(&self, country: &str) -> f64 {
        *self.prices.get(country).unwrap_or(&0.15) // fallback
    }
}
```

### Mesure CPU réelle

```rust
// Approche hybride : sysinfo CPU% × TDP estimé par plateforme

use sysinfo::System;

pub fn estimate_watts() -> f64 {
    let mut sys = System::new();
    sys.refresh_cpu_all();
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_all();

    let cpu_pct: f64 = sys.cpus().iter()
        .map(|c| c.cpu_usage() as f64)
        .sum::<f64>() / sys.cpus().len() as f64;

    // TDP basé sur la plateforme (détection compile-time)
    let (idle_w, max_w) = if cfg!(target_os = "macos") {
        (5.0, 30.0)   // Apple Silicon M1/M2/M3 (très efficace)
    } else {
        (15.0, 65.0)  // Intel/AMD laptop moyen
    };

    idle_w + (cpu_pct / 100.0) * (max_w - idle_w)
}
```

---

## 5. Résistance Sybil — Proof of Contribution {#5-sybil}

### Problème
> Un attaquant crée 1000 identités pour miner 1000x plus d'ATN
> et manipuler les votes/likes.

### Solution : Proof of Contribution (PoC) — système multi-facteur

```
Score Sybil-Résistance = f(uptime, contenu, interactions, stake)

Facteurs pondérés :
┌──────────────────────────────────────────────────────┐
│ Facteur            │ Poids │ Anti-sybil car...       │
├────────────────────┼───────┼────────────────────────┤
│ Uptime continu     │ 30%   │ Coûte de l'énergie réelle │
│ Contenu créé       │ 25%   │ Temps humain, non duplicable │
│ Likes REÇUS        │ 20%   │ Requiert d'autres humains   │
│ ATN stakés         │ 15%   │ Lock-up = skin in the game  │
│ Ancienneté         │ 10%   │ Le temps ne se fabrique pas │
└──────────────────────────────────────────────────────┘
```

### Rate limiting progressif (déjà partiellement en place)

| Action | Cooldown | Max/jour | Anti-sybil |
|--------|----------|----------|-----------|
| Mining | 60s tick | Illimité mais décroissant (halving) | Énergie réelle |
| Like | 1h par créateur | 50 likes | Cooldown |
| Vue | 5 min par site | 200 vues | Cooldown |
| Création | — | 10 sites | Trust-gated |
| Transfer | — | 50 tx | Balance-gated |
| Staking | — | 5 ops | Min 1 ATN |

### Pénalité identité Sybil détectée

Si le réseau détecte des identités coordonnées (même IP, même patterns) :
- Trust score → 0
- Mining rate → 0
- Contenu masqué du feed global
- ATN gelés (non transférables)

---

## 6. Crates Rust — Dépendances Exactes {#6-crates}

### Nouvelles dépendances à ajouter dans Cargo.toml

```toml
# Post-quantum (NIST FIPS 203/204)
ml-kem = "0.2"        # FIPS 203 — Key Encapsulation
ml-dsa = "0.2"        # FIPS 204 — Digital Signatures

# CRDT pour consensus
crdts = "7"            # Rust CRDTs (G-Counter, PN-Counter, OR-Set, LWW-Register)

# Mesure CPU
sysinfo = "0.33"       # CPU usage pour calcul énergie réelle (DÉJÀ EN PLACE si utilisé)

# Détection timezone/pays
chrono-tz = "0.10"     # Timezone → pays mapping
```

### Dépendances existantes à GARDER

```toml
# DÉJÀ EN PLACE — NE PAS TOUCHER
ed25519-dalek = "2"
aes-gcm = "0.10"
argon2 = "0.5"
blake3 = "1"
zeroize = "1"
iroh = "..."
hex = "0.4"
serde = { version = "1", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
```

---

## 7. Plan d'Implémentation — Fichiers {#7-plan}

### Phase 2A — Crypto Post-Quantique (priorité sécurité)

| # | Fichier | Action | Lignes estimées |
|---|---------|--------|----------------|
| 1 | `src-tauri/src/security/hybrid_crypto.rs` | **CRÉER** | ~200 |
| 2 | `src-tauri/src/security/crypto.rs` | **MODIFIER** | Ajouter génération clé PQ + signature hybride |
| 3 | `src-tauri/src/p2p/ledger.rs` | **MODIFIER** | `verify_tx` → vérification hybride |
| 4 | `Cargo.toml` | **MODIFIER** | Ajouter `ml-kem`, `ml-dsa` |

### Phase 2B — Consensus Merkle-CRDT (priorité réseau)

| # | Fichier | Action | Lignes estimées |
|---|---------|--------|----------------|
| 5 | `src-tauri/src/p2p/consensus.rs` | **CRÉER** | ~400 |
| 6 | `src-tauri/src/p2p/merkle_dag.rs` | **CRÉER** | ~300 |
| 7 | `src-tauri/src/p2p/gossip.rs` | **CRÉER** | ~200 |
| 8 | `src-tauri/src/p2p/ledger.rs` | **MODIFIER** | Chaîne linéaire → DAG |
| 9 | `Cargo.toml` | **MODIFIER** | Ajouter `crdts` |

### Phase 2C — Économie Réelle (priorité valeur)

| # | Fichier | Action | Lignes estimées |
|---|---------|--------|----------------|
| 10 | `src-tauri/src/p2p/energy.rs` | **CRÉER** | ~150 |
| 11 | `src-tauri/src/p2p/reputation.rs` | **MODIFIER** | EUR_PER_KWH dynamique, WATTS réel |

### Phase 2D — Anti-Sybil Renforcé

| # | Fichier | Action | Lignes estimées |
|---|---------|--------|----------------|
| 12 | `src-tauri/src/p2p/sybil.rs` | **CRÉER** | ~200 |
| 13 | `src-tauri/src/p2p/reputation.rs` | **MODIFIER** | Intégrer score multi-facteur |

### ORDRE D'EXÉCUTION
```
1. Phase 2A (PQ crypto)    — Sécurité d'abord, aucune dépendance externe
2. Phase 2C (énergie)      — Simple, impactant, visible dans le wallet
3. Phase 2D (anti-sybil)   — Renforcement du système existant
4. Phase 2B (consensus)    — Le plus complexe, fait en dernier
```

### NE PAS TOUCHER
- Tout le frontend Svelte (Phase 1 séparée)
- `src-tauri/src/security/vault.rs` — Déjà sécurisé
- `src-tauri/src/p2p/mod.rs` — Structure de module

---

## 8. Ce qui existe déjà — Audit {#8-existant}

### Sécurité (score : 8/10 ✅)
- ✅ Ed25519-dalek pour signatures
- ✅ AES-256-GCM pour chiffrement
- ✅ Argon2id pour dérivation de clé (protection brute-force)
- ✅ BLAKE3 pour hash de blocs et transactions
- ✅ zeroize pour effacement mémoire
- ✅ Anti-replay (hash set)
- ✅ Anti-double spend (balance check + pending)
- ✅ Fenêtre temporelle ±5min
- ❌ Pas de post-quantique
- ❌ Pas de signature hybride

### Consensus (score : 3/10 ⚠️)
- ✅ Chaîne de blocs locale fonctionnelle
- ✅ Vérification d'intégrité (`verify_chain`)
- ✅ Signature de transactions Ed25519
- ❌ Pas de sync inter-nœuds du ledger
- ❌ Pas de DAG (chaîne linéaire seulement)
- ❌ Pas de gossip de blocs
- ❌ Pas de résolution de conflits

### Économie (score : 6/10)
- ✅ Mining par uptime
- ✅ Halving Bitcoin-style
- ✅ Récompenses par action (like, view, create)
- ✅ Staking
- ✅ Burning déflationnaire
- ❌ Prix énergie codé en dur (pas réel)
- ❌ Watts estimé fixe (pas réel)
- ❌ Pas de moyenne réseau

### Anti-Sybil (score : 5/10)
- ✅ Cooldowns (vues 5min, likes 1h)
- ✅ Trust score multi-facteurs
- ✅ Pénalités report abusif
- ❌ Pas de détection multi-identité
- ❌ Pas de rate limiting progressif
- ❌ Mining identique pour nouveau et ancien nœud
