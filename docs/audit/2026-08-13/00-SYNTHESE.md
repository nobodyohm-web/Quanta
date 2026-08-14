# AUDIT DE SÉCURITÉ — QUANTA PROTOCOL v3.15.1

**Cible** `/Users/alex/Desktop/Quanta` · `quanta-protocol` 3.15.1 · protocole TORUS v9 · commit `de24411`
**Date** 13 août 2026 · **Périmètre** sécurité, sans complaisance
**Dépôt** intact — `git status` propre, aucun commit, aucun fichier suivi modifié. Toutes les
preuves ont tourné sur des copies hors dépôt (`/tmp/quanta_poc`, `/tmp/qaudit`, `/tmp/qtarget_*`).

---

## 1. Verdict

Quanta est un projet **sérieusement construit et sérieusement vulnérable**. Ce sont deux
affirmations compatibles, et l'audit doit les tenir ensemble.

Ce qui est bien fait est rare : la liaison intrinsèque adresse↔clé, la récompense de bloc
recalculée par chaque nœud au lieu d'être crue, la couverture des dépenses écrite une seule fois
pour la production et la vérification, l'ordre du pipeline gossip pensé pour ne rien écrire avant
la signature, Argon2id au-dessus d'OWASP, `OsRng` partout, 513 tests verts, `clippy -D warnings`
propre (vérifié), 6 `expect` documentés dans tout le code de production et aucun `unwrap`.

Et pourtant, **85 constats** ont été trouvés, dont **13 critiques**. Le motif dominant n'est pas
l'incompétence, c'est un **angle mort méthodologique** qui revient partout :

> Le projet vérifie très bien ce qu'il a décidé de vérifier, et ne vérifie pas ce dont il n'a
> jamais écrit la règle. Chaque défense existante est bien faite ; ce sont les défenses **absentes**
> qui ouvrent le système, et elles sont absentes en silence, sans test rouge pour le dire.

Trois exemples qui résument tout :
- la préimage signée d'une transaction est un `format!("{}:{}:…")` — la signature est correcte,
  **ce qu'elle signe est ambigu** ;
- le hash d'une transaction n'a **aucune unicité on-chain** — l'anti-rejeu existe, mais seulement
  à l'entrée du mempool, jamais dans un bloc ;
- l'ACL de Tauri donne l'**illusion** d'un périmètre (`core:default`) alors que les 34 commandes
  applicatives ne passent jamais par elle.

**Recommandation.** En l'état, le réseau ne doit porter aucune valeur, et aucun binaire ne doit
être publié. Ce n'est pas une opinion de prudence : trois des critiques sont exploitables par un
pair quelconque, sans clé, sans enjeu, avec quelques messages.

| sévérité | nombre |
|---|---|
| **CRITIQUE** | **13** |
| HAUT | 21 |
| MOYEN | 29 |
| BAS | 20 |
| info | 2 |
| **total** | **85** |

---

## 2. Les treize critiques, rangées par ce qu'un attaquant fait

### 2.1 Il vole de l'argent, ou en fabrique

| id | ancre | ce qu'il fait |
|---|---|---|
| **CRIT-1** | `p2p/ledger/mod.rs:1481` | **La préimage de transaction n'est pas injective.** `format!("{}:{}:{}:{}:{}:{:?}:{}:{}")` sans préfixe de longueur, avec `id`, `to` et `timestamp` libres de contenir `:`. Deux transactions **sémantiquement différentes** partagent la même préimage, donc la même signature ML-DSA **et le même `tx.hash`**. Prouvé : deux nœuds finissent avec **la même chaîne, les mêmes hashs de bloc, et des soldes différents** — Bob crédité de 100 QUANTA sur l'un, 0 sur l'autre. Divergence de consensus silencieuse. |
| **C-01** | `p2p/ledger/validation.rs:602` | **Rejeu de transaction on-chain.** `validate_block_against_prev` ne vérifie ni le nonce ni l'unicité. `seen_tx_hashes` n'est consulté qu'à l'admission mempool ; sur le chemin bloc, `reorg.rs:336` **insère** le hash sans lire le retour. Prouvé : la même transaction signée une seule fois est incluse 10 fois — Alice à 0, Bob à 100 QTA. Le seul mur est la couverture de solde, c'est-à-dire l'épuisement de la victime. |
| **R2** | `p2p/username.rs:173` | **Vol de n'importe quel `@pseudo`.** `challenger_wins` applique « le plus ancien gagne » sur un champ `claimed_at: u64` **choisi par le revendiquant** et couvert par sa propre signature. `claimed_at: 0` gagne contre tout le monde. Coût : une clé ML-DSA (165 µs) et un message. Les paiements adressés à `@alex` partent chez le voleur. |
| **A1** | `src-tauri/src/lib.rs:205` | **Les 34 commandes IPC échappent à l'ACL Tauri.** Tauri 2 ne contrôle l'ACL que pour les commandes de plugin ou si l'app déclare un manifeste ; `gen/schemas/acl-manifests.json` ne contient **aucune clé applicative**. Tout JS exécuté dans le webview appelle `invoke("get_recovery_phrase")` et obtient les 24 mots. La CSP autorise explicitement l'exfiltration vers `api.github.com`. |
| **A2** | `commands/identity.rs:172` | **Le verrouillage du portefeuille est cosmétique.** `ready = false` côté Svelte ; côté Rust il n'existe aucune fonction de verrouillage, le `CryptoEngine` garde la clé de dépense pendant toute la vie du processus, et `get_recovery_phrase` ne demande **aucun mot de passe** — la réauthentification est une convention d'interface. |
| **A3** | `rpc.rs:167` | **Plantation du cookie RPC.** `load_or_create` adopte tout fichier `.cookie` existant dont le contenu fait ≥ 32 caractères, **sans vérifier ni les permissions ni le propriétaire**, et ne réapplique pas `0600` sur ce chemin. Un processus local qui écrit le fichier avant le premier démarrage choisit le jeton et obtient l'autorité complète sur les méthodes qui déplacent des fonds. |

### 2.2 Il fait taire ou tomber un nœud

| id | ancre | ce qu'il fait |
|---|---|---|
| **R1** | `p2p/dispatcher.rs:554` | **Diffamation à distance — bannissement de n'importe quel nœud, sans posséder aucune clé.** À l'échec de la vérification de signature, le nœud **diffuse** `ReportPeer{peer_id: env.sender}` — c'est-à-dire qu'il dénonce le `sender` d'une enveloppe dont la signature vient précisément d'être jugée fausse, donc une donnée non authentifiée. `REPORT_BAN_THRESHOLD = 3` rapporteurs *indépendants* : l'attaquant envoie la même forgerie à trois nœuds honnêtes, qui dénoncent chacun de leur côté. **Prouvé de bout en bout** (`~/Desktop/QUANTA_POC_BAN.rs`, 3 tests verts) : 3 enveloppes forgées ⇒ victime bannie 3 600 s, son trafic parfaitement valide n'est même plus compté. Amplification en prime : chaque forgerie coûte au nœud une **signature ML-DSA-65 complète** plus un envoi à tous ses pairs, avant le limiteur de débit. |
| **R3** | `p2p/willow_node.rs:597` | `max_message_size` porté à 10 Mo (défaut iroh : 4 Ko), et plumtree **relaie et met en cache 30 s avant authentification**, avec un cache non borné en taille — OOM distant. |
| **H-07** | `p2p/ledger/validation.rs:290` | `sum()` u64 non protégé sur l'émission d'un bloc. En **debug**, `attempt to add with overflow` **panique** dans `validate_remote_block` ; la boucle `spawn_incoming_dispatch` n'est jamais relancée ⇒ **un seul message rend le nœud sourd à tout gossip, définitivement**. En release la somme s'enroule, et seule `validate_block_reward_plan` — une règle *économique* ajoutée en v9 — empêche le mint infini. |

### 2.3 Il réécrit l'histoire

| id | ancre | ce qu'il fait |
|---|---|---|
| **C-02** | `p2p/ledger/validation.rs:233` | **Le `timestamp` d'un bloc n'est validé nulle part** : ni bornes de dérive, ni monotonie vis-à-vis du parent, ni cadence minimale, ni même parsabilité RFC3339. La seule fraîcheur du système (±90 s) porte sur l'enveloppe gossip, pas sur le contenu du bloc. Devient un compteur libre pour le grinding. |
| **C-03** | `p2p/fork_heal.rs:340` | **Long-range / nothing-at-stake à 1 QTA.** Le fork-choice vivant est « plus longue chaîne + départage lexicographique », sans coût, sans pondération par l'enjeu. |
| **C-04** | `p2p/ledger/validation.rs:650` | **L'élection pondérée par l'enjeu n'est jamais appliquée à la réception** : la règle est une simple appartenance à l'ensemble bondé. N'importe quel compte bondé au minimum peut proposer n'importe quel slot. |

### 2.4 Il compromet la chaîne de publication

| id | ancre | ce qu'il fait |
|---|---|---|
| **SC-01** | `.github/workflows/release.yml:77` | La **clé privée de signature de l'updater** est passée à `tauri-action@v0` — un **tag mutable, non épinglé par SHA**. Quiconque contrôle ce tag exfiltre la clé qui signe les mises à jour de tous les utilisateurs. |
| **SC-02** | `deny.toml:19-59` | La porte `cargo deny check` **renvoie 0** alors que 4 vulnérabilités et un use-after-free sont dans l'arbre. La porte existe, elle est inopérante. |

---

## 3. Répartition par domaine

| domaine | CRIT | HAUT | MOY | BAS | rapport détaillé |
|---|---|---|---|---|---|
| Cryptographie & clés | 1 | 3 | 6 | 3 | `QUANTA_AUDIT_CRYPTO.md` (617 l.) |
| Consensus, ledger, économie | 4 | 4 | 6 | 3 | `QUANTA_AUDIT_CONSENSUS.md` (857 l.) |
| Réseau P2P | 3 | 5 | 5 | 4 | `QUANTA_AUDIT_RESEAU.md` (889 l.) |
| Application, RPC, IPC, front | 3 | 4 | 6 | 8 | `QUANTA_AUDIT_APPLI.md` (857 l.) |
| Chaîne d'appro., build, CI | 2 | 5 | 6 | 2 | `QUANTA_AUDIT_SUPPLYCHAIN.md` (1073 l.) |

---

## 4. Ce qui tient — et il faut le savoir

Ce n'est pas une section de politesse. Chacun de ces points a été attaqué et a résisté.

1. **La liaison intrinsèque adresse↔clé** (`validation.rs:95`) est le meilleur choix de conception
   du projet : `from == BLAKE3(ADDR_DOMAIN ‖ pk)` est exigé *avant* toute vérification de
   signature. Sans registre, sans état, sans fenêtre de course. Un attaquant ne peut pas attacher
   sa clé à un compte étranger.
2. **La racine ML-DSA est réellement indépendante d'Ed25519.** Prouvé : deux moteurs partageant la
   même graine Ed25519 produisent des adresses de fonds **différentes**. Casser Ed25519 ne donne
   aucune autorité sur les fonds. La dette de l'identité de transport est correctement déclarée.
3. **La récompense de bloc est recalculée, jamais crue** — et sa **répartition** aussi
   (`validate_block_reward_plan`). Le plafond de 100 M est vérifié au consensus sur les quatre
   chemins d'admission, avec `saturating_add`, et un reorg profond ne rejoue pas l'émission parce
   que `prior_mined` est re-dérivé de la chaîne courante.
4. **Le correctif H1/H3 sur l'identifiant d'enveloppe tient** : la LRU de déduplication n'est plus
   empoisonnable par un non authentifié, et l'insertion est bien après la signature.
5. **PRESIG-ORDER tient** : aucune carte par expéditeur n'est écrite depuis un `sender` usurpé.
   L'espace de nonce d'un pair honnête n'est pas épuisable par un tiers (vérifié par test).
6. **Aucune XSS stockée**, ni dans `explorer.html` (37 interpolations classées une par une), ni
   dans le frontend Svelte (les 17 `{@html}` portent sur des constantes i18n, un QR généré
   localement, et le whitepaper échappé).
7. **La défense CSRF sur les méthodes d'argent tient** : exigence de `Content-Type:
   application/json` (qui tue la requête CORS simple), refus de toute origine croisée, comparaison
   du jeton **à temps constant**, allowlist par défaut.
8. **Argon2id 64 MiB / t=3 / p=4**, au-dessus d'OWASP 2024, 88 ms mesurés par essai. Nonce AES-GCM
   tiré d'`OsRng` à chaque chiffrement (64 chiffrements ⇒ 64 nonces distincts, prouvé).
9. **Le multisig** refuse M=0, M>N, le double comptage d'un signataire et le rejeu inter-messages.
   La logique de quorum est juste ; c'est sa préimage qui est vulnérable.
10. **Qualité d'ingénierie mesurée** : 36 256 lignes de Rust (27 % de commentaire), 513 tests
    unitaires + 1 test d'intégration, **tous verts** ; `cargo clippy --all-targets -D warnings`
    **RC=0** (vérifié) ; **zéro `unwrap`** en production et 6 `expect` tous justifiés par un
    invariant local ; un seul bloc `unsafe`.

---

## 5. Les affirmations de `SECURITY.md` confrontées au code

Le projet se présente comme honnête, et il l'est largement — les limites annoncées sont réelles.
Quatre affirmations sont pourtant fausses ou trompeuses, et deux d'entre elles servent à
justifier des décisions de sécurité.

| affirmation | réalité |
|---|---|
| « L'application n'embarque **aucun client HTTP sortant**. » | **Faux.** `reqwest 0.13.2` + `hyper 1.9` sont dans l'arbre via `iroh`, `iroh-relay` **et** `tauri-plugin-updater`, que `lib.rs:187` enregistre. Cette phrase sert de justification à quatre `ignore` de `deny.toml`. |
| « `cargo deny check` (licences, advisories RUSTSEC, sources, doublons). » | La commande **renvoie 0** avec 4 vulnérabilités et un use-after-free dans l'arbre. |
| « le dernier scan a évalué 8 vulnérabilités transitives » | `cargo audit` en donne **4 + 22 avertissements** aujourd'hui, dont `hickory-*` (boucle non bornée sur réponse DNS, chemin réseau réel via le résolveur d'iroh). |
| « un bloc n'accepte **au plus qu'une** tx de minage » | Plus vrai depuis v9 : un bloc porte plusieurs coinbases. La règle a bien été remplacée par le plan de récompense — mais le document, lui, n'a pas suivi. |

À l'inverse, ces affirmations ont été **vérifiées et tiennent** : « aucune `unwrap()` en
production », « tous les montants en u64 µQTA », « secrets zeroize-és » (sauf le mnémonique,
MOY), « un seul bloc `unsafe` », « 513 tests + 1 ».

---

## 6. Ordre de remédiation

L'ordre n'est pas la sévérité : c'est le rapport entre ce que ça ferme et ce que ça coûte.

**Étape 1 — trois corrections courtes qui ferment cinq critiques.**
1. `R1` : ne **jamais** diffuser de `ReportPeer` sur un `env.sender` non authentifié. La règle est
   d'une ligne : à l'échec de signature, on jette, on compte, et on ne dénonce personne. On ne
   peut pas dénoncer avec une donnée qu'on vient de déclarer non fiable.
2. `CRIT-1` + `MOY-1` : rendre la préimage **injective** — préfixe de longueur sur chaque champ et
   séparateur de domaine, exactement comme `sm/finality_vote.rs:87` le fait déjà correctement.
   Le bon modèle est **dans le dépôt** ; il suffit de l'appliquer aux transactions et aux feuilles
   Merkle. Rupture de protocole assumée (v10).
3. `R2` : contraindre `claimed_at` — borne supérieure « pas dans le futur », ancrage sur une
   hauteur de bloc, ou premier-vu-gagne à la place de plus-ancien-gagne.

**Étape 2 — l'unicité on-chain (`C-01`), la seule correction structurante.**
Un état de nonce par compte vérifié **à l'inclusion**, dans la même boucle séquentielle que
`uncovered_tx_indices`. C'est la correction la plus lourde et la plus indispensable : sans elle,
une signature reste réutilisable indéfiniment.

**Étape 3 — l'application locale.**
`A1` (déclarer un manifeste ACL applicatif), `A2` (un vrai verrouillage côté Rust + mot de passe
exigé par `get_recovery_phrase`), `A3` (refuser un `.cookie` dont les permissions ou le
propriétaire ne conviennent pas), `A6` (`quanta.db` en 0600), CSP sans `unsafe-inline`.

**Étape 4 — les bornes manquantes.**
Taille de bloc et nombre de transactions, `K×S` du multisig, cardinalité de `Hello`, éléments
après décompression, lanes mpsc bornées, `[profile.release] overflow-checks = true` (sinon la
suite de tests valide une arithmétique que le binaire n'exécute pas — `SC-07`).

**Étape 5 — la publication.**
Épingler `tauri-action` par SHA et sortir la clé de signature du workflow (`SC-01`), rendre
`cargo deny` bloquant (`SC-02`), `--locked` en CI, `npm audit`, et une cible de fuzz qui commence
**après** le mur d'authentification (`SC-06` : aujourd'hui 100 % des entrées meurent avant, la
couverture réelle du parseur est nulle).

**Étape 6 — le consensus.** `C-02`, `C-03`, `C-04`, le VRF (ADR-004), la pondération par l'enjeu à
l'époque du vote (`H-06`). C'est un chantier de conception, pas un correctif.

---

## 7. Ce qui n'a pas été vérifié

- Aucun **réseau réel** : tout a été fait sur des nœuds en mémoire dans un même processus. Les
  attaques de propagation, de partition et de NAT restent théoriques ici — et le projet dit
  lui-même n'avoir jamais tourné au-delà de deux nœuds.
- **Temps constant** des primitives `fips204` et `aes-gcm` (non instrumenté), **Keychain macOS**
  réel, **application graphique** non lancée (`A1` est établi par lecture du code de Tauri et des
  manifestes générés, pas par un `invoke` exécuté).
- **Pas de fuzzing** conduit pendant cet audit ; la cible existante est inopérante (`SC-06`).
- **Pas de revue de la cryptographie de `fips204` elle-même** : on suppose l'implémentation
  correcte, ce qu'aucun audit de ce périmètre ne peut établir.

---

## 8. Fichiers produits

| fichier | contenu |
|---|---|
| `QUANTA_AUDIT.md` | cette synthèse |
| `QUANTA_AUDIT_CRYPTO.md` | cryptographie et gestion des clés — 13 constats, 14 tests |
| `QUANTA_AUDIT_CONSENSUS.md` | consensus, ledger, économie — 17 constats, tests d'exploitation |
| `QUANTA_AUDIT_RESEAU.md` | réseau P2P — 17 constats |
| `QUANTA_AUDIT_APPLI.md` | RPC, IPC, frontend, stockage — 22 constats |
| `QUANTA_AUDIT_SUPPLYCHAIN.md` | dépendances, build, CI, secrets, qualité de la preuve — 16 constats |
| `QUANTA_POC_BAN.rs` | preuve exécutable de `R1` — 3 tests, à coller dans `dispatcher.rs` |

Chaque constat des rapports détaillés porte une ancre `fichier:ligne` vérifiée, un chemin
d'exploitation, et la mention explicite **[PROUVÉ]** quand un test a été écrit et exécuté.
