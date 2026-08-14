# Politique de sécurité — Quanta Protocol

> Statut : **alpha, non audité par un tiers.** Le code actuel n'a **jamais** été
> éprouvé entre deux machines physiques derrière deux NAT distincts : le jalon de
> mai 2026 précède deux ruptures de protocole et une régression qui a rendu le nœud
> muet sur tout réseau réel pendant deux mois sans qu'aucun compteur ne le signale.
> N'engagez pas de valeur réelle que vous ne pouvez pas perdre.

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
| 3.16.x  | TORUS v10 | ✅        |
| < 3.16  | v1 → v9   | ❌ — le protocole a rompu dix fois ; les versions antérieures ne parlent plus au réseau |

Le passage **v9 → v10** est la réponse à l'audit externe du 13/08/2026 : la
pré-image signée d'une transaction, la feuille Merkle et l'en-tête de bloc
passent à un encodage **injectif** (séparateur de domaine + champs préfixés en
longueur), et l'unicité d'une transaction devient une **règle de consensus**
(nonce de compte séquentiel vérifié à l'inclusion). Toute signature et tout hash
changent, genèse comprise ; un nœud v9 et un nœud v10 ne peuvent pas converger.

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

Le raisonnement complet derrière ces choix — ce qu'un ordinateur quantique casse
exactement, pourquoi une chaîne publique est un cas particulier, pourquoi l'hybride
sur le transport et le PQ pur sur les signatures — est développé dans
[`docs/POST-QUANTUM.md`](docs/POST-QUANTUM.md).

## Modèle de menace — ce qui est défendu

- **Forge de transactions** : chaque tx est signée ML-DSA-65 par la clé liée à
  l'adresse de l'expéditeur, et re-vérifiée par chaque nœud. Les adresses
  synthétiques `NETWORK` / `ESCROW` sont exemptes de signature ; un expéditeur
  synthétique ne peut apparaître **que** dans une coinbase `Mining`, et
  l'ensemble exact des bénéficiaires et de leurs montants est re-dérivé de la
  chaîne par chaque nœud (`validate_block_reward_plan`). Un `Transfer` depuis
  `NETWORK` est rejeté (sinon il aurait minté sans limite, invisible au plafond).
  *(Corrigé le 13/08/2026 : ce paragraphe annonçait « au plus une tx de minage »,
  règle remplacée par le plan de récompense au fork v9 — la documentation n'avait
  pas suivi le code.)*
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

## Audit externe du 13 août 2026 — état des corrections

Un audit externe sans complaisance a produit **85 constats, dont 13 critiques**.
Le motif dominant n'était pas l'incompétence mais un angle mort méthodologique :
*le projet vérifiait très bien ce qu'il avait décidé de vérifier, et ne vérifiait
pas ce dont il n'avait jamais écrit la règle.* Les défenses présentes étaient
bonnes ; ce sont les défenses **absentes** qui ouvraient le système, et elles
étaient absentes en silence, sans test rouge pour le dire.

| id | ce qu'un attaquant faisait | état |
|---|---|---|
| `R1` | Bannir n'importe quel nœud du réseau sans posséder aucune clé | **corrigé** — `REPORT-NOAUTH-1` : un échec de signature ne dénonce plus personne |
| `CRIT-1` | Une signature autorisait deux transactions différentes | **corrigé** — `CANON-1` : préimage injective (domaine + préfixes de longueur) |
| `C-01` | Rejouer une transaction signée une seule fois, jusqu'à vider la victime | **corrigé** — `NONCE-ONCHAIN-1` : nonce séquentiel vérifié à l'inclusion |
| `R2` | Voler n'importe quel `@pseudo` avec `claimed_at: 0` | **corrigé** — `CLAIM-WINDOW-1` : dates bornées + fenêtre de contestation |
| `A1` | Les commandes IPC échappaient à l'ACL de Tauri | **corrigé** — manifeste ACL applicatif déclaré, permissions accordées une par une |
| `A2` | Le verrouillage du portefeuille était un drapeau Svelte | **corrigé** — `lock_wallet` côté Rust ; `get_recovery_phrase` exige le mot de passe |
| `A3` | Planter le cookie RPC pour obtenir l'autorité de dépense | **corrigé** — `COOKIE-OWN-1` : propriété + permissions vérifiées avant adoption |
| `A4` | `sendtoaddress` dépensait sans déverrouillage ni plafond | **corrigé** — `SEND-OPTIN-1` : fermé par défaut, plafonné |
| `A6` | `quanta.db` (coffre + graine ML-DSA) lisible en 0644 | **corrigé** — 0600 sur la base et ses journaux, 0700 sur le répertoire |
| `H-07` | Un message rendait le nœud sourd à tout gossip, définitivement | **corrigé** — somme d'émission `checked`, le débordement est un rejet |
| `C-02` | Le timestamp d'un bloc n'était validé nulle part | **corrigé** — `BLOCK-TIME-1` : parsable et non décroissant vis-à-vis du parent |
| `R3` `R4` `R13` | OOM distant par messages, décompression et `Hello` non bornés | **corrigé** — bornes dérivées du plus gros message légal |
| `C-04` | Le départage de fork était « le plus grand hash gagne » — gagnable par broyage, sans posséder un µQTA | **corrigé (14/08)** — `FORK-RANK-1` : rang d'élection pondéré par l'enjeu, ancré sur un beacon enterré ([`docs/DESIGN-FORK-RANK.md`](docs/DESIGN-FORK-RANK.md)) |
| `C-03` | Long-range : une branche reconstruite hors ligne remontant à la genèse était admissible tant que la finalité n'avait pas progressé | **corrigé (14/08)** — `REORG-DEPTH-1` (128 blocs, indépendant de la finalité) + `GENESIS-ANCHOR-1` |
| `H-06` | Le même certificat de finalité échouait puis réussissait selon qui s'était désengagé entre-temps | **corrigé (14/08)** — enjeu figé à la frontière d'époque, fonction pure de la chaîne |
| `H-08` | 45 % de chaque récompense captés avec 28 identités, pour 28 QTA d'enjeu | **corrigé (14/08)** — `REWARD-WEIGHT-1` : le pot se pondère par les **blocs produits**, pas par le nombre d'adresses |
| `HAUT-2` `HAUT-3` | Blob de clé de fonds absent ⇒ une clé neuve était fabriquée, solde à zéro | **corrigé (14/08)** — ancre `pq_fund_anchor_v1` + refus, migration ancien→neuf testée |
| `MOY-6` | `tx.signature` : exigé non vide, jamais vérifié, et pourtant dans la feuille Merkle | **corrigé (14/08)** — retiré des deux |
| `M-09` | `energy_kwh` réécrivable par un relais sans invalider le bloc | **corrigé (14/08)** — entre dans la préimage d'en-tête |
| — | **Trouvé en relisant notre propre correctif** : `BLOCK-TIME-1` (`ts >= prev.ts`) était un **cliquet** — un bloc daté de 2099 gelait la chaîne pour 73 ans | **corrigé (14/08)** — `BLOCK-TIME-2` : *median-time-past* sur 11 blocs, toujours sans horloge |
| `MOY-3` | La préimage de revendication de pseudo était injective par accident de format, et ne nommait pas sa chaîne | **corrigé (14/08)** — encodage canonique + `CHAIN_ID` |
| `BAS-1` | `address::parse` retombait sur l'hex : la somme de contrôle disparaissait sur les deux chemins de dépense | **corrigé (14/08)** — `parse` strict, `parse_hex_unchecked` nommé, avertissement à l'écran d'envoi |
| `R14` `R15` | Trois fonctions publiques mortes qui produisaient des enveloppes rejetées en silence ; anti-rejeu et bans oubliés à chaque redémarrage | **corrigé (14/08)** — supprimées ; 8ᵉ clé de snapshot, restauration monotone |
| **restant** | Grinding de la graine d'élection, prédictibilité sans VRF, `M-14` (validation O(hauteur)), séparation `ctx` FIPS-204 | **ouvert** — nommé et détaillé dans `docs/audit/REMEDIATION-2026-08-13.md` §9.6 et §11 |

**Ce qui reste vrai malgré tout** : la liaison intrinsèque adresse↔clé
(`from == BLAKE3(ADDR_DOMAIN ‖ pk)`), l'indépendance réelle de la racine ML-DSA
vis-à-vis d'Ed25519, la récompense de bloc recalculée plutôt que crue, l'absence
de XSS stockée, la défense CSRF du RPC, Argon2id au-dessus d'OWASP.

**Ce que l'audit n'a pas vérifié, et que nous ne prétendons donc pas** : aucun
réseau réel (tout en mémoire, dans un même processus) ; pas de mesure de temps
constant sur `fips204`/`aes-gcm` ; pas de fuzzing conduit ; pas de revue de
`fips204` lui-même. La cible de fuzz existante était d'ailleurs inopérante — 100 %
des entrées mouraient au mur de signature ML-DSA, donc la couverture réelle des
parseurs était **nulle** (`SC-06`, corrigé) ; le fuzzing reste néanmoins à
conduire.

## Durcissement de la chaîne d'approvisionnement

*Section réécrite le 13/08/2026 après un audit externe. Trois affirmations y
étaient fausses, et deux d'entre elles servaient à justifier des décisions de
sécurité — c'est le pire usage possible d'une phrase inexacte.*

- `deny.toml` — `cargo deny check` (licences, advisories RUSTSEC, sources,
  doublons). **La porte était inopérante** : elle renvoyait 0 alors que
  l'arbre portait 4 vulnérabilités et un use-after-free
  (`RUSTSEC-2026-0253`, `lru`), parce que `[advisories]` ne déclarait que
  `yanked` et qu'une liste d'`ignore` — dont six périmés — couvrait le reste.
  Les `ignore` doivent porter une justification datée, et `cargo audit` tourne
  désormais **sans liste d'exclusion** à côté, pour que le chiffre brut reste
  visible.
- `cargo audit` — vulnérabilités connues des dépendances. Le chiffre à jour
  (13/08/2026) est **4 vulnérabilités + 22 avertissements** transitifs, pas
  « 8 ». Parmi elles, `hickory-*` (boucle non bornée sur réponse DNS) est sur un
  chemin réseau réel, via le résolveur d'iroh.
- **L'application embarque bien un client HTTP sortant.** `reqwest` et `hyper`
  sont dans l'arbre via `iroh`, `iroh-relay` **et** `tauri-plugin-updater`, que
  `lib.rs` enregistre. L'affirmation inverse figurait ici et servait de
  justification à quatre `ignore` de `deny.toml` ; les deux sont corrigés.
- Un seul bloc `unsafe` dans tout le backend (interop AppKit pour l'état
  d'occlusion de la fenêtre, `guardian.rs`) ; **zéro** dans la couche
  cryptographique, et `fips204` est lui-même pur Rust sans `unsafe`. *(Vérifié
  par l'audit.)*
- Secrets `zeroize`-és en mémoire ; jamais de clé privée dans les logs ou les
  erreurs. *(Vérifié par l'audit, avec une réserve : le mnémonique BIP39
  lui-même n'est pas encore zeroize-é.)*

## Règles d'or (rappel développeur)

1. Aucune `unwrap()` dans le code de production — `Result` + `?`.
2. `tokio::sync` uniquement à travers un `.await` (jamais `std::sync`).
3. Tous les montants en `u64` µQTA (jamais de `f64` pour les soldes).
4. Erreurs de déchiffrement opaques (« Invalid », jamais le type réel).
5. L'autorité d'un compte est **ML-DSA** ; Ed25519 n'est plus qu'un identifiant
   de point de terminaison réseau.
