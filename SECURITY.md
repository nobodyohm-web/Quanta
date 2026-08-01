# Politique de sécurité — Quanta Protocol

> Statut : **alpha, non audité par un tiers.** Le réseau n'a jamais tourné au-delà
> de deux machines physiques. N'engagez pas de valeur réelle que vous ne pouvez
> pas perdre.

## Signaler une vulnérabilité

Envoyez un rapport **privé** à **nobodyohm@gmail.com** avec :

- une description et l'impact estimé,
- les étapes de reproduction (idéalement un test ou un PoC),
- la version / le commit concerné.

Merci de **ne pas** ouvrir d'issue publique pour une faille exploitable avant
qu'un correctif soit disponible. Réponse visée sous 7 jours.

## Versions supportées

| Version | Protocole | Supportée |
|---------|-----------|-----------|
| 3.13.x  | TORUS v7  | ✅        |
| < 3.13  | v1 → v6   | ❌ — le protocole a rompu sept fois ; les versions antérieures ne parlent plus au réseau |

## Posture cryptographique

| Usage | Primitive |
|-------|-----------|
| Signatures (transactions — l'argent) | **ML-DSA-65 pur** (NIST FIPS 204, crate `fips204`) |
| Signatures (votes de finalité) | **ML-DSA-65** |
| Signatures (enveloppes gossip) | **ML-DSA-65** |
| Échange de clés du transport | **X25519MLKEM768** (hybride PQ, rustls + aws-lc-rs) |
| Identité de nœud (NodeId Iroh) | Ed25519 — **classique**, voir « Limites connues » |
| Chiffrement au repos (vault) | AES-256-GCM |
| Dérivation de clé (mot de passe) | Argon2id (64 MiB, 3 itérations, parallélisme 4) |
| Hachage / content-addressing | BLAKE3 |
| Transport | Iroh (QUIC, TLS 1.3) |

**Frontière post-quantique.** Il n'y a **plus de schéma hybride ni de repli
classique** : l'autorité d'une transaction, d'un vote de finalité ou d'une
enveloppe gossip est une signature ML-DSA-65, et rien d'autre. Une adresse est
`BLAKE3(ADDR_DOMAIN ‖ clé publique ML-DSA)` ; la clé ML-DSA est dérivée
déterministiquement de la graine (XOF BLAKE3), donc aucune matière secrète
supplémentaire n'est persistée. L'échange de clés QUIC/TLS négocie l'hybride
X25519MLKEM768, ce qui protège la confidentialité du transport contre une
attaque *harvest-now-decrypt-later*.

**Ce qui reste classique.** L'identité de nœud d'Iroh (NodeId = Ed25519) : c'est
une dette *upstream*, Iroh attend un consensus d'industrie sur la signature
post-quantique des EndpointIds. Elle n'authentifie ni l'argent, ni la finalité,
ni les messages — seulement le point de terminaison réseau.

## Modèle de menace — ce qui est défendu

- **Forge de transactions** : chaque tx est signée ML-DSA-65 par la clé liée à
  l'adresse de l'expéditeur, et re-vérifiée par chaque nœud. Les adresses
  synthétiques `NETWORK` / `ESCROW` sont exemptes de signature, mais un bloc
  n'accepte **au plus qu'une** tx de minage, obligatoirement la coinbase
  `NETWORK → block.miner` : un `Transfer` depuis `NETWORK` est rejeté (sinon il
  aurait minté sans limite, invisible au plafond).
- **Rejeu** : nonce monotone par expéditeur, identifiant d'enveloppe canonique
  (`id == BLAKE3` de la pré-image signée), déduplication LRU 100 K insérée
  **après** la vérification de signature, fenêtre de fraîcheur ±90 s,
  `seen_tx_hashes` au ledger.
- **Double-dépense** : règle de couverture symétrique — un bloc reçu dont une
  dépense n'est pas couverte par le solde on-chain est rejeté, et un bloc scellé
  localement exclut ces tx, donc il est valide par construction. Conservation
  `Σ(dépensable + staké + en déverrouillage) + brûlé == miné` vérifiée à chaque
  pas de simulation.
- **Réécriture de l'histoire** : plancher de finalité monotone, vérifié par hash
  et persisté ; aucun fork ne peut remplacer un bloc situé sous le plancher.
- **Équivocation d'un validateur** : détectée à l'ingestion (double-vote et
  surround), preuve ML-DSA non-répudiable diffusée, enjeu de l'offenseur détruit
  STAKE→BURN — y compris l'enjeu en cours de déverrouillage, pour qu'un
  « unstake-and-run » ne mette pas les fonds à l'abri. Un proposeur malveillant
  ne peut pas punir un innocent : la preuve embarquée est re-vérifiée par chaque
  nœud.
- **DoS gossip** : 10 Mo max par enveloppe, rate-limit adaptatif borné [15, 120]
  msg/min, 50 blocs max par segment de synchronisation, bannissement après
  3 signalements, cartes par pair bornées en mémoire.
- **Eclipse** : heuristique de collision de préfixe de clé publique (> 80 %).
- **Sybil (économique)** : le poids d'élection est **l'enjeu inscrit sur la
  chaîne**, une fonction pure du ledger — la réputation locale a été retirée du
  chemin de sécurité, précisément parce qu'elle divergeait entre nœuds.
- **RPC monnaie** : les méthodes qui déplacent des fonds exigent un jeton cookie
  et passent une garde `Origin` / `Content-Type` ; sans elle, un `fetch()` depuis
  n'importe quelle page web atteignait `sendtoaddress` en requête CORS simple.

## Limites connues — ce qui n'est PAS garanti

Par honnêteté, et c'est la ligne du projet :

- **Pas d'audit tiers.** Un audit interne adversarial (25/07/2026,
  `docs/audit/AUDIT-INTERNE-2026-07-25.md`) a ouvert 4 critiques, 8 hauts et
  4 moyens, tous corrigés derrière le fork v7. Ce n'est pas un audit externe. Le
  dossier de consultation est prêt dans `docs/audit/`.
- **Échelle non éprouvée.** Deux machines physiques (mai 2026) et un test
  d'intégration deux nœuds. Les partitions, réordres, crashs et nœuds byzantins
  sont couverts en **simulation déterministe seedée**, pas sur un vrai réseau.
- **Aléa d'élection.** L'élection du proposeur est déterministe et publiquement
  vérifiable — ce n'est **pas** un VRF : le leader d'un slot est prévisible. Un
  beacon enterré bloque le grinding immédiat ; un vrai VRF et un VDF sont au
  roadmap (ADR-004, ouverte).
- **Identité de nœud classique** (voir ci-dessus).
- **Pas de confidentialité on-chain.** Transactions et soldes sont publics ;
  aucune couche ZK.
- **Binaires non notarisés / non signés.** La dernière release publiée date de
  mai 2026 et ne correspond plus au code.

## Durcissement de la chaîne d'approvisionnement

- `deny.toml` — `cargo deny check` (licences, advisories RUSTSEC, sources, doublons).
- `cargo audit` — vulnérabilités connues des dépendances ; le dernier scan a
  évalué 8 vulnérabilités **transitives**, aucune dans le code Quanta.
- Un seul bloc `unsafe` dans tout le backend (interop AppKit pour l'état
  d'occlusion de la fenêtre, `guardian.rs`) ; **zéro** dans la couche
  cryptographique, et `fips204` est lui-même pur Rust sans `unsafe`.
- L'application n'embarque aucun client HTTP sortant.
- Secrets `zeroize`-és en mémoire ; jamais de clé privée dans les logs ou les
  erreurs.

## Règles d'or (rappel développeur)

1. Aucune `unwrap()` dans le code de production — `Result` + `?`.
2. `tokio::sync` uniquement à travers un `.await` (jamais `std::sync`).
3. Tous les montants en `u64` µQTA (jamais de `f64` pour les soldes).
4. Erreurs de déchiffrement opaques (« Invalid », jamais le type réel).
5. L'autorité d'un compte est **ML-DSA** ; Ed25519 n'est plus qu'un identifiant
   de point de terminaison réseau.
