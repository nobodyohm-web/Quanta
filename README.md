# ◈ Sovereign Web Engine v4 — TITAN

> **Defense-Grade Post-Quantum Web — Willow Protocol, BM25 Search, zeroize Memory Safety**

Moteur souverain de création, synchronisation et navigation de sites P2P sans serveur. Architecture **Actor Model** (Rust/Tauri 2.0) + Svelte 5, transport **Willow/QUIC** via Iroh, vérification **BLAKE3**, identité **Ed25519** (ML-DSA ready), base **libSQL** vectorielle, recherche décentralisée **BM25 + Centroïdes**.

---

## 🏗️ Architecture v4 "Titan"

```
┌─────────────────────────────────────────────────────────────────┐
│                   SOVEREIGN WEB ENGINE v4                       │
│                     "TITAN" — Defense Grade                     │
├─────────────────────┬───────────────────────────────────────────┤
│   Frontend          │   Backend (Rust Actor Model)              │
│   Svelte 5 Runes    │                                           │
│   + Tailwind v4     │   ┌── security/ ───────────────────┐     │
│                     │   │  mod.rs    Ed25519 + BLAKE3      │     │
│  ┌──────────────┐   │   │  cipher.rs AES-256-GCM + Argon2  │     │
│  │  Cmd+K       │   │   │  pq_vault.rs  PQ Vault (Kyber/   │     │
│  │  Palette     │   │   │    Dilithium ready) + S/Kademlia  │     │
│  └──────────────┘   │   │    + zeroize + SecureBuffer       │     │
│                     │   └────────────────────────────────┘     │
│  ┌──────────────┐   │                                           │
│  │  Editor      │◄──┼──►┌── p2p/ ─────────────────────────┐   │
│  │  MD → HTML   │   │   │  mod.rs     Willow 3D Types      │   │
│  └──────────────┘   │   │  willow_node.rs  Iroh Runtime     │   │
│                     │   │    Subspace sync + BLAKE3 verify  │   │
│  ┌──────────────┐   │   └──────────────────────────────────┘   │
│  │  Search      │◄──┼──►┌── search/ ──────────────────────┐   │
│  │  BM25 + DHT  │   │   │  mod.rs     SearchResult types   │   │
│  └──────────────┘   │   │  indexer.rs  BM25 + Centroid TF  │   │
│                     │   │    -IDF + DHT coordinate mapping  │   │
│  ┌──────────────┐   │   └──────────────────────────────────┘   │
│  │  Bento Grid  │◄──┼──►┌── storage/ ─────────────────────┐   │
│  │  Dashboard   │   │   │  db.rs  libSQL (vectors + CRUD)  │   │
│  └──────────────┘   │   └──────────────────────────────────┘   │
│                     │                                           │
│  ┌──────────────┐   │       15 Commandes Tauri IPC              │
│  │  Wallet      │◄──┼──►   via invoke_handler                   │
│  │  Lightning   │   │                                           │
│  └──────────────┘   │                                           │
└─────────────────────┴───────────────────────────────────────────┘
```

---

## 🔬 Changements v2 → v4

| Aspect | v2 | v4 Titan |
|--------|----|----|
| **Module structure** | crypto/, network/, storage/ | **security/, p2p/, search/, storage/** |
| **Identity** | Ed25519 only | **PQ Vault** (Ed25519 + ML-DSA-65 ready) |
| **Memory safety** | None | **zeroize** on drop + SecureBuffer |
| **Sybil defense** | None | **S/Kademlia crypto puzzles** |
| **P2P model** | SwarmActor (feeds) | **WillowNode** (subspaces, 3D entries) |
| **Data structure** | ContentBlocks only | **WillowEntry** (subspace/path/timestamp) |
| **Search** | Simple TF-IDF | **BM25** (k1=1.2, b=0.75) + centroïdes |
| **Ranking** | freq * sqrt(matches) | **BM25 score** + multi-keyword boost |
| **Accents** | Monochrome (#e2e2e2) | **Electric blue** (#007AFF) |
| **CommandPalette** | Inline in +page | **Separate component** |
| **Version** | 0.2.0 | **0.4.0** |

---

## 📦 Stack Technique

| Couche | Technologie | Version |
|--------|------------|---------|
| **Desktop** | Tauri | 2.10 |
| **Backend** | Rust | 1.95 |
| **Frontend** | Svelte 5 (Runes) | 5.x |
| **Styles** | Tailwind CSS | v4 |
| **P2P** | iroh + iroh-blobs + iroh-gossip | 0.98/0.100 |
| **Hashing** | BLAKE3 | 1.8 |
| **Memory** | zeroize | 1.8 |
| **Database** | libSQL | 0.9 |
| **Identité** | Ed25519 (ML-DSA ready) | 2.2 |
| **Chiffrement** | AES-256-GCM | 0.10 |
| **KDF** | Argon2id | 0.5 |

---

## 🗂️ Structure du Projet

```
Torus/
├── src/                              # Frontend
│   ├── app.html                      # Google Fonts (Inter, JetBrains Mono)
│   ├── app.css                       # "Obsidian Industrial" (#007AFF accent)
│   ├── routes/+page.svelte           # Router + Cmd+K + Identity wizard
│   └── lib/
│       ├── CommandPalette.svelte     # ⌘K — navigation centrale
│       ├── Sidebar.svelte            # Nav + node status
│       ├── Editor.svelte             # Notion-like block editor (/, Enter, ⌫)
│       ├── Browser.svelte            # Google-like search (BM25)
│       ├── Wallet.svelte             # Lightning (LDK-ready)
│       └── Dashboard.svelte          # Bento stats + site list
│
├── src-tauri/                        # Backend (Rust)
│   ├── Cargo.toml                    # v0.4.0 Titan
│   ├── src/
│   │   ├── main.rs                   # Entry point
│   │   ├── lib.rs                    # 15 Tauri commands + Actor state
│   │   ├── security/
│   │   │   ├── mod.rs                # CryptoEngine + BLAKE3 + SecureBuffer
│   │   │   ├── cipher.rs             # AES-256-GCM + encrypt_and_wipe
│   │   │   └── pq_vault.rs           # PQ identity + S/Kademlia puzzles
│   │   ├── p2p/
│   │   │   ├── mod.rs                # Willow 3D types + NodeStatus
│   │   │   └── willow_node.rs        # Iroh subspace sync actor
│   │   ├── search/
│   │   │   ├── mod.rs                # SearchResult types
│   │   │   └── indexer.rs            # BM25 + centroid + DHT coords
│   │   └── storage/
│   │       ├── mod.rs                # Module exports
│   │       └── db.rs                 # libSQL (4 tables + vectors)
│   └── capabilities/default.json
│
├── .vscode/settings.json             # rust-analyzer → Rust 1.95
├── vite.config.js
└── package.json
```

---

## 🔧 Modules Rust — Détails

### `security/` — Barrière Post-Quantique

| Fichier | Rôle |
|---------|------|
| `mod.rs` | `CryptoEngine` + BLAKE3 (hash, keyed MAC, verify) + SecureBuffer (zeroize on drop) |
| `cipher.rs` | AES-256-GCM + Argon2id + **`encrypt_and_wipe()`** (zeroize le plaintext) |
| `pq_vault.rs` | `TitanIdentity` (Ed25519 + ML-DSA-65 ready) + **S/Kademlia crypto puzzles** |

**zeroize** : Toute donnée sensible (clés privées, mots de passe dérivés) est effacée de la RAM via `zeroize::Zeroize` + `ZeroizeOnDrop`. Empêche les attaques par cold boot / memory dump.

**S/Kademlia Puzzles** : Pour rejoindre le réseau, un nœud doit résoudre un puzzle BLAKE3 (difficulté paramétrable). Coût : ~500ms CPU. Empêche les attaques Sybil (création massive de faux nœuds).

### `p2p/` — Willow Protocol (Iroh)

| Fichier | Rôle |
|---------|------|
| `mod.rs` | Types : `WillowEntry` (3D: subspace/path/timestamp), `NodeStatus`, `ContentBlock` |
| `willow_node.rs` | `WillowNode` : sync de subspaces, validation BLAKE3 des blocs entrants |

**Espace de Noms Tridimensionnel** :
```
WillowEntry {
    subspace: "pk_hex",          // → propriétaire
    path: "/content/html",       // → chemin hiérarchique
    timestamp: "2026-04-23...",  // → version temporelle
    payload_hash: "blake3(...)", // → intégrité vérifiable
}
```

### `search/` — Google Géométrique (BM25 + Centroïdes)

| Fichier | Rôle |
|---------|------|
| `mod.rs` | `SearchResult` avec `bm25_score` + `centroid_distance` |
| `indexer.rs` | BM25 (k1=1.2, b=0.75) + centroïde TF-IDF + coordonnées DHT |

**BM25 vs TF-IDF simple** :
- BM25 normalise par la longueur du document (paramètre `b`)
- Saturation des fréquences (paramètre `k1`) : un mot apparaissant 100× n'est pas 100× plus important
- Standard industriel de recherche (Elasticsearch, Lucene)

### `storage/` — libSQL Vectorielle

| Fichier | Rôle |
|---------|------|
| `db.rs` | CRUD complet, `F32_BLOB` vectors, `vector_top_k` ready |

---

## 🎨 Design : "Obsidian Industrial"

| Propriété | Valeur |
|-----------|--------|
| Fond | `#050505` |
| Bordures | `#1a1a1a` |
| **Accent** | **`#007AFF`** (bleu électrique) |
| Accent dim | `rgba(0,122,255,0.08)` |
| Accent glow | `rgba(0,122,255,0.15)` |
| Font UI | Inter |
| Font Code | JetBrains Mono |

---

## ⌨️ Raccourcis

| Raccourci | Action |
|-----------|--------|
| `⌘K` | Command Palette |
| `↑↓` | Naviguer |
| `Enter` | Exécuter / Nouveau bloc |
| `Escape` | Fermer |
| `/` | Menu de blocs (dans l'éditeur) |
| `Backspace` | Supprimer bloc vide |

---

## 📝 Éditeur — Style Notion

Éditeur à blocs avec `contenteditable`. Pas de Markdown brut visible.

| Bloc | Commande `/` | Rendu |
|------|-------------|-------|
| Texte | `/text` | Paragraphe normal |
| Heading 1 | `/h1` | Grand titre (28px, 800) |
| Heading 2 | `/h2` | Titre moyen (22px, 700) |
| Heading 3 | `/h3` | Petit titre (18px, 600) |
| Liste | `/list` | Puce • |
| Citation | `/quote` | Bordure bleue + italique |
| Code | `/code` | Bloc monospace (JetBrains Mono) |
| Séparateur | `/divider` | Ligne horizontale |

**Interactions** :
- `Enter` → nouveau bloc
- `Backspace` sur bloc vide → supprimer le bloc
- `/` → menu contextuel de types
- Drag handle `⋮⋮` visible au hover
- Sérialise en Markdown + HTML pour le backend

---

## 🔍 Recherche — Style Google

**Page d'accueil** : Logo ◈ centré + barre de recherche ronde + boutons.

**Page de résultats** :
- URL verte (`swe://pk_hex…/`)
- Titre bleu cliquable (#007AFF)
- Snippet gris avec contexte
- Tags de mots-clés matchés
- Score BM25 affiché
- Temps de recherche (ms)

---

## 🔐 Matrice de Sécurité

| Couche | Mécanisme | Quantique | Mémoire |
|--------|-----------|-----------|---------|
| Identité | Ed25519 + **ML-DSA ready** | ✓ | **zeroize** |
| Hashing | BLAKE3 (10 Go/s) | ✓ | — |
| Chiffrement | AES-256-GCM | ✓ | **encrypt_and_wipe** |
| KDF | Argon2id (GPU/ASIC résistant) | ✓ | — |
| Réseau | S/Kademlia puzzles | — | — |
| Transport | QUIC (Iroh) | ✓ | — |
| Données | Willow 3D entries | ✓ | — |
| ZKP (prévu) | Plonky3 | ✓ | — |

---

## 🚀 Démarrage

```bash
cd ~/Desktop/Torus
npm install
npm run tauri dev
```

---

## 🗺️ Roadmap v4+

| Priorité | Fonctionnalité |
|----------|---------------|
| 🔴 P0 | ML-KEM-768 (FIPS 203) + ML-DSA-65 (FIPS 204) hybrid identity |
| 🔴 P0 | Iroh Willow endpoint complet (sync réelle) |
| 🔴 P0 | LDK-Node mainnet + zk-APC |
| 🟡 P1 | Plonky3 ZK-STARKs (transactions anonymes) |
| 🟡 P1 | DLC (Discreet Log Contracts) |
| 🟡 P1 | VecDHT (centroïdes vectoriels sur DHT) |
| 🟡 P1 | ZHTP (Zero-Knowledge Transport Protocol) |
| 🟢 P2 | Erasure Coding (0G-storage) |
| 🟢 P2 | CRDT sync (Automerge) |
| 🟢 P2 | Mobile (Tauri 2.0 iOS/Android) |

---

<p align="center"><strong>◈ ENGINE TITAN v4 — Defense-Grade Sovereign Web ◈</strong></p>
