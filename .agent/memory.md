# QUANTA — Mémoire Persistante

> Lu au début de chaque session. Leçons apprises, décisions, pièges à éviter.
> Mets à jour ce fichier à la fin de chaque session.

---

## Ce qu'est Quanta (état actuel — crypto-only)

**Une cryptomonnaie souveraine pair-à-pair.** Pas de serveur, pas de banque, pas
d'autorité d'émission. App de bureau Tauri 2 (Rust/tokio) + Svelte 5 ; chaque
instance fait tourner un nœud complet.

- **Pièce** : QUANTA · plus petite unité **µQTA** (1 QUANTA = 1 000 000 µQTA).
  Tous les soldes en `u64` µQTA — **jamais de f64** (zéro dérive).
- **Émission** : **décroissante vers un plafond DUR de 100 000 000 QUANTA**.
  `emission_for_tick(total_mined) = (MAX_SUPPLY_MICRO − total_mined) / 50_000_000`
  par tick (60 s) ⇒ ≈120 QUANTA/h à la genèse, baisse vers le plafond.
  **Zéro premine, zéro autorité d'émission.** Borne vérifiée au consensus
  (par bloc + globale). Source unique : `reputation::emission_for_tick`.
- **Distribution** (multi-nœuds) : Shapley pondéré — **30% énergie · 30% travail ·
  25% validation · 15% uptime** (somme = 1.0, plus de facteur social). Solo = 100% du tick.
- **Burn** : 1% détruit à chaque transfert (burn-and-mint, déflationniste).
- **Valeur** : QUANTA n'est coté nulle part. Miner coûte de l'électricité réelle,
  mais **un coût de production n'est pas un prix** — ne JAMAIS afficher de valeur fiat inventée.
- **Consensus** : Proof-of-Stake + VRF (BLAKE3). Stake min 1 QUANTA, timeout leader
  30 s, graine d'élection d'un beacon enterré (non-grindable).
- **Crypto** : signatures **hybrides Ed25519 + ML-DSA-65** (FIPS 204) sur chaque tx
  (clé ML-DSA dérivée de la graine Ed25519) ; vault Argon2id + AES-256-GCM ; BLAKE3 ; `zeroize`.
- **Identité** : une paire de clés + un court **`@pseudo`** (`UsernameRegistry`,
  module `p2p/username.rs`) + code de connexion + clé de récupération. Aucun KYC.
- **Gossip** : `GossipEnvelope` signé, nonce monotone, fenêtre ±90 s, dédup LRU 100K,
  rate-limit adaptatif, ban. **9 variants** : Hello, RequestChain, ChainSegment,
  NewBlock, BroadcastTx, Ping, Pong, ReportPeer, PublishUsername.
- **Frontend** : Wallet (défaut), Contacts, Tableau de bord, Réseau (globe 3D), Explorateur,
  Profil, Réglages. Barre latérale. **i18n 6 langues** (EN par défaut · FR · ES · RU · ZH · JA).
  Thème clair « Arc » (papier crayon + grain + champ quantique sur les moments/états vides).

### Décision : crypto-only (modules web/social RETIRÉS)
Le produit a été recentré sur la cryptomonnaie. **Supprimés du code** (ne PAS réintroduire) :
`page_store`, `domains` (.torus sites), `search`/QuantaRank, `social` (likes/follows/tips),
`moderation` (jury), `forums`, `trust_graph`, `marketplace`, `merkle_dag`, `commerce`, `dev_api`
— + leurs variants gossip, commandes, stores et tests. L'identité `@pseudo` et la crypto-core
(ledger, consensus, mining, sécurité) ont été préservées. `cargo test` : **174 passés, 0 échec**.

> Note héritage : les identifiants wire `.torus` / `TORUS_PROTOCOL_VERSION` / events `torus://…`
> sont **conservés tels quels** pour la compatibilité réseau — ne pas les renommer sans bump de protocole.

---

## Pièges connus

### Rust
- `std::sync::Mutex` à travers un `.await` = **DEADLOCK**. Toujours `tokio::sync::{Mutex, RwLock}`.
- Lock ordering : crypto → reputation → ledger → gossip. Jamais l'inverse.
- `unwrap()`/`expect()` interdits en production. Toujours `Result<T, E>` + `?`.
- `zeroize` critique : toute variable `sk_bytes`, `secret`, `key` doit être effacée.
- Soldes en `u64` µQTA — jamais f64. Pas de `unsafe`.

### Frontend
- Svelte 5 runes uniquement : `$state()`, `$derived()`, `$effect()`, `$props()`. PAS de `onMount`.
- PAS de stores Svelte 4. CSS vanilla (pas de Tailwind). Thème clair, jamais sombre.
- Tout texte UI passe par `t('clé')` (i18n) — les 6 langues doivent rester complètes
  (typage strict de `i18n.generated.ts` le garantit à la compilation).

### P2P
- Iroh `0.98` — l'API gossip : `iroh_gossip::api::Event::{Received, NeighborUp, NeighborDown, Lagged}`.
- Topic gossip = `BLAKE3(...)` (identifiant réseau). NAT traversal via relais Iroh.
- Signer TOUJOURS `signable_envelope_bytes(sender, nonce, timestamp, payload)` — jamais le payload seul.

### Build
- `cargo check` / `cargo test` dans `src-tauri/` (pas à la racine).
- `npm run build` / `npx svelte-check` à la racine pour le frontend.
- Tests d'intégration P2P : `src-tauri/tests/p2p_integration.rs`.

---

## Invariants sécurité durcis (à préserver)
- **Signature gossip** couvre `sender + nonce + timestamp + payload` (canonique), pas juste le payload.
- **Anti-replay** : nonce strictement monotone par compte (ledger) + `seen_tx_hashes` ; dédup gossip LRU 100K.
- **Émission** : `.min(emission_for_tick(total_mined))` au mining + borne par bloc au consensus → cap 100M infranchissable.
- **Transferts** : burn 1% diffusé avec la tx ; `ESCROW`/`NETWORK`/`BURN` exclus des soldes utilisateurs.
- **Fork** : valider le challenger AVANT de muter ; re-queue des tx exclusives à la branche perdante.
- **Watts** clampés [1, 500] dans `handle_hello` (anti-falsification d'émission).
- Erreurs opaques : un échec de déchiffrement renvoie « Invalid », jamais le détail. Aucune clé privée dans les logs/erreurs/JSON.

---

## Pièges à NE PAS refaire
- Ne JAMAIS revenir à une émission **fixe / non plafonnée** : la rareté (cap 100M + décroissance) est le cœur.
- Ne JAMAIS afficher un **prix/valeur fiat inventé** (le coût énergétique est un coût, pas un prix).
- Ne pas réintroduire les features **web/social/marketplace** (sites, recherche, likes, forums, modération, compute).
- Garder l'identité = **`@pseudo`** (pas un domaine payant).
