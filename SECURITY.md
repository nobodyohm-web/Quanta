# Politique de sécurité — Quanta Protocol

> Statut : **alpha, non audité par un tiers.** P2P vérifié entre deux machines
> physiques. N'engagez pas de valeur réelle que vous ne pouvez pas perdre.

## Signaler une vulnérabilité

Envoyez un rapport **privé** à **nobodyohm@gmail.com** avec :

- une description et l'impact estimé,
- les étapes de reproduction (idéalement un test ou un PoC),
- la version / le commit concerné.

Merci de **ne pas** ouvrir d'issue publique pour une faille exploitable avant
qu'un correctif soit disponible. Réponse visée sous 7 jours.

## Versions supportées

| Version | Supportée |
|---------|-----------|
| 3.3.x   | ✅        |
| < 3.3   | ❌        |

## Posture cryptographique

| Usage | Primitive |
|-------|-----------|
| Signatures (transactions) | **Hybride Ed25519 + ML-DSA-65** (NIST FIPS 204) |
| Signatures (enveloppes gossip) | Ed25519 (transport éphémère, déjà sous QUIC/TLS) |
| Chiffrement au repos (vault) | AES-256-GCM |
| Dérivation de clé (mot de passe) | Argon2id (64 MiB, 3 itérations, parallélisme 4) |
| Hachage / content-addressing | BLAKE3 |
| Transport | Iroh (QUIC, TLS 1.3) |

**Frontière post-quantique.** La couche **valeur** (transactions du ledger) est
signée en hybride Ed25519+ML-DSA-65 : forger une signature exige de casser *les
deux* schémas. La clé ML-DSA est dérivée déterministiquement de la graine
Ed25519 (XOF BLAKE3) — aucune matière secrète supplémentaire persistée. Les
enveloppes gossip restent en Ed25519 : ce sont des messages éphémères (fenêtre
de fraîcheur ±90 s) déjà protégés par QUIC/TLS ; un adversaire quantique futur
ne peut pas rejouer une enveloppe vieille de 90 secondes.

**Flag-day futur.** `REQUIRE_PQ` (cf. `security/hybrid_crypto.rs`) est à `false` :
le réseau accepte encore les signatures Ed25519-seules (rétro-compatibilité). Le
passage à une protection PQ *totale* (rejet de toute signature sans couche
ML-DSA) sera un changement de version de protocole, à arbitrer à maturité.

## Modèle de menace — ce qui est défendu

- **Forge de transactions / d'actions sociales** : chaque tx et action est
  signée et vérifiée (`verify_tx`, `verify_hybrid`). Les adresses synthétiques
  (`NETWORK`, `ESCROW`) sont les seules exemptes, et ne sont jamais acceptées
  depuis le gossip.
- **Rejeu (replay)** : nonce monotone par expéditeur sur les enveloppes,
  déduplication LRU (100k), `seen_tx_hashes`, fenêtre temporelle ±90 s.
- **Double-dépense** : `balance_of` inclut les sorties *pending* ; conservation
  vérifiée par tests property-based (Σ soldes + brûlé == miné, sur des milliers
  de séquences aléatoires).
- **DoS gossip** : taille max 10 MB/enveloppe, rate-limit adaptatif, 50 blocs
  max par segment de sync, bannissement après 3 signalements.
- **Eclipse** : heuristique de collision de préfixe de clé publique (>80 %).
- **Sybil (économique)** : stake minimum + réputation pour l'élection du leader
  PoS ; le poids n'est pas gratuit.

## Limites connues — ce qui n'est PAS encore garanti

Par honnêteté (et c'est la ligne du projet) :

- **Pas d'audit tiers.** Le code n'a pas été revu par un cabinet externe.
- **Finalité.** Le consensus PoS+VRF n'offre pas encore de finalité BFT rapide
  prouvée ; un saut vers un consensus DAG-BFT est à l'étude (voir
  `docs/DESIGN-CONSENSUS-DAG-BFT.md`).
- **Aléa du leader.** L'entropie d'élection est durcie (entropie d'epoch passée
  + accumulateur, voir `pos_consensus.rs`) mais n'intègre pas encore de VDF
  complet contre la rétention de bloc par le proposeur.
- **PQ partielle (par conception).** Voir « Frontière post-quantique » ci-dessus.
- **Pas de confidentialité.** Les transactions et les soldes sont publics (pas
  de couche ZK pour l'instant).

## Durcissement de la chaîne d'approvisionnement

- `deny.toml` — `cargo deny check` (licences, advisories RUSTSEC, sources,
  doublons). À exécuter en CI.
- `cargo audit` — vulnérabilités connues des dépendances.
- **Zéro `unsafe`** dans la couche cryptographique ; `fips204` est lui-même
  pur Rust sans `unsafe`.
- Secrets `zeroize`-és en mémoire ; jamais de clé privée dans les logs/erreurs.

## Règles d'or (rappel développeur)

1. Aucune `unwrap()` dans le code de production — `Result` + `?`.
2. `tokio::sync` uniquement à travers un `.await` (jamais `std::sync`).
3. Tous les montants en `u64` µQTA (jamais de `f64` pour les soldes).
4. Erreurs de déchiffrement opaques (« Invalid », jamais le type réel).
5. L'application ne fait **aucune** requête HTTP sortante.
