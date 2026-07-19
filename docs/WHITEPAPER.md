# Quanta — Livre blanc

> **Une monnaie souveraine P2P — sans serveur, sans cloud, sans intermédiaire.**
> Version protocole `TORUS_PROTOCOL_VERSION = 6` · App v3.4 (crypto-only) · Coin **QUANTA** ·
> Licence Apache-2.0
> **Statut : alpha, non audité par un tiers.** P2P vérifié entre deux machines physiques
> (06/05/2026), pas une preuve à l'échelle. Aucune revue de sécurité externe indépendante à ce jour.
> **QUANTA n'a aucun marché ni prix** ; aucune valeur monétaire n'est avancée ni prédite nulle part.

---

## 1. Résumé

Quanta est une cryptomonnaie pair-à-pair conçue autour d'une seule idée : **retirer le
prometteur**. Là où une monnaie fiat repose sur la retenue d'une banque centrale, et où une
plateforme repose sur un serveur qu'on peut saisir ou geler, Quanta ne repose sur personne. Il
n'y a pas de fonction pour imprimer, pas de compte à geler, pas d'entreprise à sommer, pas
d'autorité d'émission — pas même l'auteur du projet. Ce ne sont pas des promesses de bonne
conduite : ce sont des **absences** dans le code, vérifiées par chaque nœud à chaque bloc.

Techniquement, Quanta combine cinq pièces qui se tiennent :

- un transport P2P **Iroh (QUIC)** avec **gossip** pub/sub, durci par un pipeline de sécurité à
  neuf étapes et une quinzaine de mesures réseau (NET-3 → NET-16) ;
- un consensus **Proof-of-Stake** à élection de proposeur **déterministe et publiquement
  vérifiable**, pondérée par un **enjeu inscrit on-chain** — jamais par une vue locale ;
- un **gadget de finalité de type Casper-FFG** qui rend l'histoire mathématiquement irréversible
  après un **certificat de ⅔ du stake**, avec **slashing** de l'équivocation prouvable et vivant ;
- une pile cryptographique **post-quantique** : l'autorité de compte, les votes de finalité et
  les enveloppes réseau sont signés **ML-DSA-65 (FIPS 204)**, l'échange de clés de transport est
  l'hybride **X25519MLKEM768** ;
- une monnaie **rare par construction** : plafond dur de **100 000 000 QUANTA** gravé au code,
  zéro premine, émission décroissante, et destruction de 1 % à chaque transfert.

Autour de ces cinq piliers, l'app tient trois promesses simples pour un porteur : **miner**
(gagner des QUANTA en gardant le réseau), **garder** (une identité qui est une clé qu'on seul
détient, joignable par un court `@pseudo`) et **échanger** (transférer entre wallets, signé
ML-DSA, avec le burn d'usage).

Ce document décrit l'architecture telle qu'elle existe dans le code (`src-tauri/src/`), en
distinguant scrupuleusement ce qui est **réel** de ce qui reste **au roadmap**. Toute affirmation
« réelle » renvoie à une fonction, une constante ou un test présent dans le dépôt. Conformément à
la doctrine du projet : **QUANTA n'a aucun marché ni prix.** La valeur dont parle ce texte n'est
jamais une cotation — c'est un ensemble de garanties qu'aucun tiers ne peut corrompre.

---

## 2. Le problème — une monnaie sans gardien

Toute l'histoire de la monnaie est une suite de promesses tenues par quelqu'un qui avait le
pouvoir de les rompre : le roi qui rogne la pièce, la banque centrale qui imprime, la plateforme
qui gèle le compte. Chaque fois, la confiance reposait sur la retenue d'un détenteur du pouvoir —
et chaque fois, ce pouvoir a fini par servir.

Bitcoin a démontré qu'une monnaie **sans émetteur** était possible, mais a laissé deux failles.
D'abord une finalité seulement **probabiliste** — « attendez six confirmations », jamais une
garantie ; un réorg reste théoriquement possible à toute profondeur. Ensuite une sécurité
adossée à une **course énergétique** : le péché structurel du Proof-of-Work est que plus le
réseau brûle d'électricité, plus il en émet, dans une escalade sans fin. Les Proof-of-Stake
classiques répondent au premier point — ils apportent une finalité déterministe — mais signent
avec une cryptographie (Ed25519, ECDSA, BLS) qu'un ordinateur quantique brisera : la promesse
d'irréversibilité y porte un astérisque quantique.

Quanta se donne pour objet de fermer les quatre à la fois. La comparaison, en une ligne par
famille :

- **Fiat** — la valeur tient à la retenue de la banque centrale ; Quanta retire la planche à
  billets (aucun mint hors de la courbe d'émission).
- **Banques** — la valeur passe par la permission de l'intermédiaire ; Quanta retire le compte
  gelable et le serveur saisissable (il n'y en a aucun).
- **Bitcoin** — la sécurité est une puissance de calcul, la finalité une probabilité ; Quanta
  retire le gaspillage énergétique **et** l'incertitude (finalité déterministe).
- **PoS classiques** — les signatures sont cassables au quantique ; Quanta les remplace par
  ML-DSA de bout en bout sur la valeur et la finalité.

La mission vise donc une **perfection réseau** au service d'une seule chose : une monnaie saine,
rare et vérifiable. Cinq objectifs en découlent : un protocole P2P unifié et versionné (Torus) ;
des échanges déterministes à convergence garantie sans perte de données ; un réseau robuste
(reconnexion, discovery multi-peer, NAT traversal) ; une blockchain de production (PoS stable,
résolution de fork déterministe, sync rapide) ; et une monnaie vérifiable (plafond dur gravé,
zéro premine, zéro autorité d'émission).

---

## 3. Architecture & protocole Torus

Le backend est écrit en **Rust (Tauri 2.0, édition 2021)**, le frontend en **Svelte 5**. Un
principe d'architecture domine tout le reste : le cœur du consensus vit dans un module **sans-IO
déterministe** (`sm/`), où l'horloge et le générateur d'aléa sont **injectés** au bord plutôt que
lus au milieu du code. Cette discipline a un but précis : rendre le consensus **simulable
exhaustivement**, indépendamment de la couche réseau. Un harnais de test déterministe (DST)
rejoue le protocole sur de nombreuses graines, injecte des fautes réseau et byzantines, et
vérifie des invariants — dont **C1**, la propriété que deux nœuds partant des mêmes entrées
produisent des sorties **byte-identiques** (méta-test sur 128 exécutions). Le verdict de
consensus ne doit jamais dépendre de l'ordre d'arrivée des messages, de l'horloge murale ou de
l'ordre d'itération d'une `HashMap` — d'où l'usage systématique de structures ordonnées
(`BTreeMap`/`BTreeSet`) sur les chemins de décision.

### 3.1 Les cinq couches

Le protocole Torus s'organise en cinq couches nettes :

```
Application  ← Wallet, identité @pseudo, staking
Protocole    ← GossipMessage (Hello, RequestChain, ChainSegment, NewBlock,
               BroadcastTx, Ping/Pong, PublishUsername, ReportPeer, FinalityVote)
Sécurité     ← GossipEnvelope (signature + nonce monotone + timestamp)
Transport    ← Iroh QUIC + iroh-gossip pub/sub
Réseau       ← NAT traversal + relay + hole punching
```

Les messages du protocole gossip et leur rôle :

- **`Hello`** — présence : porte `chain_height`, watts, pays. Priorité critique.
- **`RequestChain`** / **`ChainSegment`** — synchronisation de chaîne : demande depuis une
  hauteur, réponse paginée (max 50 blocs par segment). Critique.
- **`NewBlock`** — bloc scellé par le leader PoS, diffusé. Critique.
- **`BroadcastTx`** — transaction signée (minage, transfert, burn). Haute priorité.
- **`PublishUsername`** — enregistrement d'identité `@pseudo`. Priorité moyenne.
- **`Ping`** / **`Pong`** — vérification de liveness. Priorité basse.
- **`ReportPeer`** — signalement d'un pair malveillant. Priorité basse.
- **`FinalityVote`** — vote de finalité du gadget Casper-FFG (ajouté par le câblage vivant
  LIVE-1). Une variante `FinalityFault` porte les preuves d'équivocation.

### 3.2 Flux de connexion

Un nœud qui rejoint le réseau se présente par un `Hello` portant sa `chain_height`. Les pairs
comparent les hauteurs : le plus en retard émet un `RequestChain`, l'autre répond par des
`ChainSegment`. Le `Hello` est ré-émis toutes les 120 s, un `Ping` léger toutes les 15 s assure
la liveness, et un nettoyage supprime les pairs morts après 5 minutes sans signe de vie. La
synchronisation peut être parallèle (4 fenêtres de 50 blocs à la fois) et les segments peuvent
être compressés en gzip (avec un plafond de décompression de 50 Mo pour éviter les bombes zip).

### 3.3 Pipeline de sécurité (dispatch_incoming)

Chaque message entrant traverse neuf étapes avant d'être traité — un raté à n'importe laquelle
et le message est rejeté :

```
① Size guard (max 10 MB par enveloppe)
② JSON deserialize → GossipEnvelope
③ Ban check (per-peer)
④ Dedup (seen_messages, LRU 100 000)
⑤ Timestamp freshness (±90 s)
⑥ Rate limit adaptatif (base 30 msg/min/peer, sqrt-scaling, borné [15, 120])
⑦ Nonce anti-replay (monotone par expéditeur)
⑧ Vérification de signature (ML-DSA-65 depuis le hard-fork v4)
⑨ Dispatch vers le handler (dont le bras FinalityVote)
```

Chaque enveloppe est ainsi signée, horodatée, munie d'un nonce monotone et dé-dupliquée par un
identifiant unique (LRU de 100 000 entrées). Le rate limit est **adaptatif** : la base de 30
messages par minute et par pair est mise à l'échelle en `sqrt(peers/4)`, bornée entre 15 et 120,
pour absorber les pics de topologie sans ouvrir la porte au spam. Le bannissement est social et
temporaire : trois signalements valides bannissent un pair pour une heure, avec auto-expiration.

### 3.4 Durcissement réseau (NET-3 → NET-16)

Autour de ce pipeline, une quinzaine de mesures complètent la robustesse : une file de priorité
sortante à quatre voies (Critical/High/Medium/Low), la sync parallèle et la compression déjà
citées, des métriques par pair (RTT en moyenne mobile exponentielle, octets et messages entrants,
score de qualité 0-100 combinant latence, perte et uptime), une vue de topologie à deux sauts,
une heuristique **anti-eclipse** (alerte si plus de 80 % des pairs partagent le même préfixe de
clé publique sur 8 chiffres hexadécimaux), un mempool à TTL de 10 minutes plafonné à 1000
transactions, des surnoms de pairs signés, et le champ `TORUS_PROTOCOL_VERSION` (aujourd'hui
**6**) qui signale et journalise les pairs incompatibles. La version 6 a été atteinte par une
série de ruptures documentées : passage à la genèse post-quantique (2→3), slashes sur unbonding
(3→4), hard-fork v4 « genèse propre » (4→5), puis le multisig natif MSIG-1 (5→6).

---

## 4. Consensus — PoS pondéré par l'enjeu + finalité Casper-FFG

Le consensus de Quanta se lit en deux étages. Un étage **d'élection** décide qui a le droit de
proposer le prochain bloc. Un étage **de finalité** rend une histoire proposée *irréversible*.
Le premier tient la vivacité (la chaîne avance) ; le second tient la sûreté (l'histoire ne se
réécrit pas). Les deux sont pondérés par le **même** enjeu on-chain, et parlent de la **même**
identité de compte — l'adresse ML-DSA.

### 4.1 Élection du proposeur

À chaque slot (égal à la hauteur de chaîne), le proposeur est élu de façon **déterministe** :

```
beacon = BLAKE3(domaine ‖ hash_du_bloc_enterré ‖ slot)
seed   = BLAKE3(domaine ‖ beacon ‖ slot ‖ round)
seed % total_weighted_stake → leader
```

Le bloc « enterré » est celui situé `LEADER_ENTROPY_LOOKBACK = 2` slots derrière le tip — pas le
tip fraîchement scellé. Le poids d'un validateur est son **enjeu inscrit sur la chaîne**
(`ledger.validator_stakes()`), dérivé des transactions `Stake`/`Unstake` scellées : c'est une
**fonction pure de la chaîne**, identique sur chaque nœud, qu'il soit vivant, restauré depuis un
snapshot ou fraîchement synchronisé. C'est précisément ce qui **ferme le vecteur de fork** : tant
que le poids venait d'une vue locale (un classement de réputation propre à chaque nœud), deux
nœuds honnêtes pouvaient élire des leaders différents au même slot et diverger. En ancrant le
poids à l'état du ledger, l'ensemble des validateurs devient un **objet de consensus**.

Les paramètres opérationnels : stake minimum `MIN_VALIDATOR_STAKE = 1 000 000 µQTA` (1 QUANTA, un
placeholder ajustable) ; si le leader désigné ne produit pas, un fallback bascule au suivant après
`LEADER_TIMEOUT_SECS = 30` s, jusqu'à `MAX_FALLBACK_ROUNDS = 3` rounds ; tant que personne n'a
staké, l'élection est permissionless (bootstrap).

**Nommage honnête.** Cette élection est *déterministe et publiquement vérifiable* — ce **n'est
pas** un VRF cryptographique. Sans clé secrète, le leader de chaque slot est **calculable
d'avance par tous**, donc ciblable (un adversaire peut DoS le leader juste avant son slot). Le
beacon enterré empêche l'auto-grinding immédiat — le sceleur du tip ne choisit pas l'aléa du
prochain slot — mais un grinding à long horizon reste théoriquement ouvert sans VDF. C'est un
compromis assumé (voir ADR-004) : la liveness ne repose pas sur le secret du leader mais sur le
**comité qui finalise par quorum** et sur le slashing. Un vrai VRF (imprévisibilité) et un VDF
(anti-grinding) sont au roadmap, **non livrés**. Les identifiants internes `vrf` sont des noms
hérités gardés pour la compatibilité.

### 4.2 Le cycle de vie complet d'un bloc

Le minage tourne dans `mining_loop.rs` : toutes les 60 s un tick de minage
(`MINE_INTERVAL_SECS = 60`), un seal toutes les 2 ticks (`SEAL_EVERY_N_TICKS = 2`), soit environ
un bloc toutes les 2 minutes (~720 blocs/jour). Voici le trajet complet d'un bloc, de sa
proposition à son irréversibilité :

```
① Élection      → le leader du slot est déterminé (beacon enterré, poids = stake on-chain)
② Seal          → le leader scelle : sélectionne les tx couvertes (COVER-2), calcule le
                  Merkle root BLAKE3, produit un bloc valide par construction
③ Gossip        → NewBlock diffusé (file de priorité critique, signé ML-DSA)
④ Réception     → validate_block_against_prev : proposeur bondé as-of-parent (PROPOSER-1) ?
                  émission ≤ plafond ? couverture des dépenses/stakes (COVER-1) ? Merkle ?
⑤ Intégration   → extension linéaire, doublon, fork à hauteur égale (départage), ou reorg
⑥ Vote          → à la frontière d'époque, les validateurs signent un Vote ML-DSA source→target
⑦ Certificat    → une fois ⅔ du stake atteints (backing×3 ≥ total×2), l'époque est justifiée
⑧ Finalité      → deux liens consécutifs justifiés → finalize ; le plancher monte, irréversible
```

L'étape ④ est la fermeture d'un CRITIQUE longtemps différé. Jusqu'au hard-fork v4 (PROPOSER-1), le
proposeur n'était vérifié qu'au *seal*, côté producteur. Il l'est désormais **à la réception** : le
validateur partagé `validate_block_against_prev` — le même code sur les quatre chemins (extension
linéaire, départage de fork, clone d'essai de reorg, synchronisation) — **rejette** tout bloc
non-genèse dont le proposeur n'est pas un validateur bondé as-of-parent (enjeu ≥ minimum). La règle
est non-temporisée : « tout validateur bondé », un sur-ensemble de ce que le seal produit. Cette
symétrie produce/receive garantit qu'un nœud n'accepte jamais un bloc qu'il n'aurait pas lui-même
pu sceller, et évite tout fork par dérive d'horloge (préservant C1).

### 4.3 Le gadget de finalité (Casper-FFG)

Au-dessus de l'élection vit un **gadget de type Casper-FFG** (`sm/finality*.rs`) qui rend
l'histoire irréversible — la propriété que Bitcoin n'a pas. Écrit pur et déterministe, prouvé en
simulation DST, il se décompose en cinq blocs (GADGET-1 à 5) :

```
Époque = 32 blocs (EPOCH_LENGTH_BLOCKS)
├─ GADGET-1  checkpoint (hauteur, hash) à chaque frontière d'époque
├─ GADGET-2  Vote ML-DSA-65 (source→target) + certificat de ⅔ du stake
│            quorum gravé : backing×3 ≥ total×2  (QUORUM_NUM/DEN = 2/3)
├─ GADGET-3  justify puis finalize (deux liens consécutifs) → FinalityState
├─ GADGET-4  accountable safety : détecte double-vote + surround, preuve
│            ML-DSA non-répudiable, slash (brûlé, plein, fenêtre = unbonding)
└─ GADGET-5  fork-choice LMD-GHOST pondéré par le stake, ancré à la finalité
             (5A ghost_head/anchors ; 5B réconciliation via reorg_to_fork)
```

Le format de vote mérite une note. Là où beaucoup de PoS agrègent des milliers de votes en une
seule signature BLS de taille constante, Quanta a **choisi de ne pas agréger** : chaque vote est
une signature ML-DSA séparée. La raison (ADR-005) est double. D'abord un vote de finalité est
**éphémère** — il ne compte que dans la fenêtre où la finalité se décide, contrairement à une
transaction qui doit rester infalsifiable des années ; le BLS n'apporte donc pas de sécurité
supplémentaire ici, seulement de la compacité. Ensuite la **finalisation par époque** neutralise
le coût de taille : un certificat par lot de blocs, pas par bloc, amorti sur des dizaines de
blocs et élagable. Le bénéfice est net : une **finalité entièrement post-quantique, sans
astérisque**, et une **attribution de faute directe** (le slashing est plus simple sur des
signatures séparées que sur un agrégat). Une abstraction de certificat garde la porte ouverte à
une agrégation future (BLS ou SNARK PQ) si l'échelle l'impose un jour, en remplacement local.

L'identité de vote est la clé publique ML-DSA ; le pont `validator_stakes_by_pubkey` re-clé
l'enjeu **purement depuis la chaîne** (chaque tx `Stake` révèle sa `pq_public_key`). Comme
l'enjeu, le vote et le `from` d'une transaction partagent une **seule** identité — l'adresse
ML-DSA — deux nœuds aux mêmes votes et à la même chaîne finalisent **identiquement**, sans table
de correspondance.

### 4.4 Le câblage vivant (LIVE-1 → LIVE-4)

Le gadget n'est pas qu'un cœur prouvé en simulation : ses votes **circulent en vivant** depuis
les quatre chantiers LIVE. Aucun de ces chantiers n'a introduit de règle de consensus nouvelle —
ils ont branché le cœur pur sur le réseau et le ledger réels, en franchissant la frontière
sans-IO sans la casser (le cœur `sm/` reste inchangé, C1 préservé, l'IO testée séparément).

- **LIVE-1 — les votes circulent.** Un validateur signe son `Vote` (ML-DSA) et le gossipe
  (`GossipMessage::FinalityVote`) ; le bras de dispatch (étape ⑨) le désérialise, le dé-duplique,
  le valide (`Vote::verify`) et le remet au `FinalityTracker` (`p2p/finality_live.rs`). Le cast se
  fait au tick de mining. Les votes reçus peuplent `LatestVotes` et l'état de finalité du ledger
  vivant.
- **LIVE-2 — le plancher de finalité.** Un `finalized_floor_index` (monotone, vérifié par hash,
  persisté au snapshot) est alimenté par les certificats ⅔. La résolution de fork
  (`integrate_remote_block`) **refuse** désormais tout fork qui remplacerait un bloc à hauteur ≤
  plancher. L'histoire finalisée devient **irréversible sur le réseau vivant** ; le départage
  lexicographique libre ne joue qu'**au-dessus** du plancher (règle de Gasper : libre au-dessus,
  gelé à et sous la finalité). C'est une garde de sûreté **pure** — refuser un reorg ne mute aucun
  solde.
- **LIVE-3 / 3B — le slashing vivant.** L'équivocation détectée à l'ingest d'un vote
  (`detect_fault` : double-vote ou surround) est gossipée comme `FinalityFault`, puis scellée
  comme une transaction `Slash` dans le prochain bloc. L'autorité de cette tx est la **preuve
  embarquée elle-même**, re-vérifiée par chaque nœud (`verify_block_slashes`) : un proposeur
  malveillant ne peut pas punir un innocent, car il faudrait une vraie preuve, la bonne adresse
  d'offenseur et le montant ratifié. Le slash détruit l'enjeu de l'offenseur **STAKE → BURN**,
  **conservation neutre par construction**. La variante 3B ferme le « unstake-and-run » : la base
  slashable inclut le stake **en cours de déverrouillage** (sémantique Casper — punissable tant
  que le retrait n'est pas complété), et la tx `Slash` porte sa ventilation de consommation
  (entrées détruites, ordre déterministe), liée au hash et au Merkle, re-vérifiée par chaque nœud
  contre son propre plan et restaurée exactement au reorg.
- **LIVE-4 — la réconciliation de fork profonde.** Le `ForkReconciler` (`p2p/fork_heal.rs`)
  bufferise les blocs qui échouent l'intégration linéaire (tampon borné à 1024, éviction
  déterministe), assemble la branche concurrente enracinée chez nous, applique la **règle de
  victoire vivante** — plus-longue-au-dessus-du-plancher, avec départage lexicographique du tip à
  hauteur égale — via `reorg_to_fork` sur un clone d'essai validé intégralement. C'est la
  généralisation N-blocs de la règle 1-bloc historique : exactement un côté adopte, les deux
  convergent. Deux partitions qui scellent chacune ≥ 2 blocs convergent désormais en vivant — le
  dernier trou de convergence est fermé.

---

## 5. Cryptographie post-quantique

Quanta sépare deux rôles cryptographiques — **l'autorité** (qui peut dépenser, staker, voter,
revendiquer une identité) et le **transport** (qui parle à qui sur le réseau) — et migre chacun
vers le post-quantique là où c'est nécessaire. Les primitives, par rôle :

- **Autorité de compte** — **ML-DSA-65 (FIPS 204)**, pure. C'est la signature qui protège
  l'argent des années durant.
- **Votes de finalité** — **ML-DSA-65**, jamais de primitive classique sur le chemin de
  l'irréversibilité.
- **Enveloppes gossip** — **ML-DSA-65** depuis le hard-fork v4 (PQ-ENVELOPE-1) ; le sender est la
  clé publique ML-DSA, le fallback Ed25519/legacy a été supprimé.
- **Échange de clés de transport** — hybride **X25519MLKEM768** (ML-KEM-768 ⊕ X25519), négocié
  par QUIC/TLS 1.3.
- **Chiffrement symétrique** — **AES-256-GCM** (vault, données au repos).
- **Dérivation de clé** — **Argon2id** (mot de passe → clé du vault).
- **Hachage** — **BLAKE3** (adresses, Merkle root, beacon d'élection, arbres de finalité).

### 5.1 Autorité de compte — ML-DSA pur, sans astérisque

Depuis PQ-MIG-3B, `from` et `to` sont des **adresses ML-DSA** (`BLAKE3(ADDR_DOMAIN ‖ clé)`)
**partout** : solde, récompense de minage, enjeu, `@pseudo`. Ce n'est pas un détail
d'indexation : c'est ce qui rend la promesse « entièrement post-quantique » **honnête au niveau
du compte**. L'audit interne CRYPTO-ID-1 avait montré que la version antérieure ne la tenait
pas — les comptes étaient enracinés Ed25519, et la clé ML-DSA, auto-déclarée par transaction et
jamais liée au compte, ne protégeait rien : un adversaire quantique cassant Ed25519 pouvait
forger la signature de la victime, y attacher sa **propre** clé ML-DSA et passer. La correction
est **intrinsèque** : `from` **est** `BLAKE3(ADDR_DOMAIN ‖ clé)`, donc révéler une autre clé
donne un autre hash, différent de `from`, et la vérification (`lie(from, clé)`) échoue. Aucune
signature classique forgée ne change ce fait. La vérification de transaction (`verify_tx`) est
purement ML-DSA ; le co-facteur Ed25519 a quitté le chemin d'autorité.

### 5.2 Transport — hybride post-quantique

Le fournisseur rustls est passé de `ring` à **`aws-lc-rs`** avec l'option `prefer-post-quantum`
(PQ-TRANSPORT-1) : l'échange de clés QUIC/TLS 1.3 négocie l'hybride **X25519MLKEM768**, pour un
surcoût d'environ 1 Ko par poignée de main, une seule fois par connexion. C'est une défense
**« harvest-now-decrypt-later »** sur la confidentialité du transport : un adversaire qui
enregistre le trafic aujourd'hui ne pourra pas le déchiffrer après l'avènement d'un ordinateur
quantique. La bascule est non destructive — ni bump de protocole, ni reset de genèse — parce
qu'elle se joue dans la négociation TLS avec dégradation gracieuse.

### 5.3 Multisig post-quantique natif (MSIG-1)

Quanta offre une custody quorum **M-of-N** entièrement ML-DSA (MSIG-1, bump de protocole 5→6).
L'adresse commet à sa politique par `BLAKE3(MSIG_DOMAIN ‖ clés triées ‖ seuil)`, se reconnaît à
un tag `pq_public_key == "msig1"`, et porte son autorité en JSON dans le champ `pq_signature` — ce
qui n'ajoute **aucun** nouveau champ wire (une transaction mono-clé reste byte-identique à avant).
La vérification `verify_multisig` exige au moins *seuil* signataires **distincts** valides, et est
rebind-proof (l'adresse ne peut pas être re-liée à une autre politique). C'est la **première
custody quorum post-quantique** du projet, qui contourne l'absence de standard de signature
à seuil pour ML-DSA.

### 5.4 La seule dette honnête — le NodeId Iroh

L'**identifiant de nœud** réseau — le `NodeId` d'Iroh — reste **Ed25519**. C'est une dette **en
amont** : Iroh attend un consensus d'industrie sur la signature post-quantique des EndpointIds,
et c'est hors de notre code. Concrètement : un adversaire quantique pourrait usurper l'*identité
réseau* d'un nœud en temps réel, mais **ne peut ni forger une transaction ou un vote de finalité**
(protégés par ML-DSA) **ni déchiffrer le trafic passé** (protégé par X25519MLKEM768). La bascule
est prévue le jour où Iroh livrera un EndpointId post-quantique. Bilan : argent, finalité et
confidentialité du transport sont post-quantiques ; il ne reste que l'auth de nœud, hors de notre
portée. Tous les secrets cryptographiques sont par ailleurs effacés en mémoire (`zeroize()`).

---

## 6. Tokenomics

### 6.1 Rareté prouvée, pas promise

Le plafond est **dur : 100 000 000 QUANTA** (`MAX_SUPPLY_MICRO = 100_000_000 × MICRO`). Il n'est
pas écrit sur un site : il est **vérifié au consensus** (`validate_block_emission`), qui fait
rejeter tout bloc qui porterait l'offre au-delà — sur le chemin linéaire comme sur les reorgs, via
un validateur partagé. `1 QUANTA = 1 000 000 µQTA` ; toute la comptabilité est en `u64`/`u128`,
jamais un `float` ne touche un solde (règle Rust #6). Le plafond, comme la loi d'émission, est une
constante Rust — substituée à la compilation, sans emplacement mémoire d'exécution : il n'existe
**aucun setter exprimable**. La porte n'est pas verrouillée, elle est **absente**. Le seul moyen
de changer ces valeurs serait d'éditer la source et de recompiler, c'est-à-dire de forker.

### 6.2 Émission décroissante, front-loaded

```
emission_for_tick(total_miné) = (MAX_SUPPLY_MICRO − total_miné) / EMISSION_DIVISOR
                                 EMISSION_DIVISOR = 50 000 000
```

C'est une **fonction pure** de ce qui est déjà miné : chaque tick libère `1/50 000 000` de
l'offre *restante*. La décroissance est géométrique et **front-loaded** — à la genèse,
`emission_for_tick(0)` vaut exactement 2 000 000 µQTA, soit **2 QUANTA par tick** (~120/h) ; puis
une longue traîne approche le plafond **en asymptote sans jamais l'atteindre** (la division
entière ne solde jamais le dernier µQTA : `emission_for_tick(MAX_SUPPLY)` vaut 0, testé). Ce
profil récompense les premiers gardiens sans jamais produire d'avalanche de pièces. Le choix d'un
plafond « nombre-phare » (100 M) est le « moment 21 M » de QUANTA — une rareté terminale prouvable
comme celle de Bitcoin, mais par une courbe lisse plutôt que par des falaises de halving qui
déstabilisent l'économie des validateurs.

**Zéro premine** : la genèse alloue une table vide (testé : `Ledger::new() ==
genesis_with_allocation(&[])`). La **seule** origine de pièces dans tout le code est
`mine_tx(from = "NETWORK", TxType::Mining)`, plafonnée par `emission_for_tick`. Aucune clé, aucun
rôle fondateur ne peut fabriquer un µQTA hors de la courbe — pas même l'auteur du projet.

### 6.3 Déflation d'usage

**1 % de chaque transfert est détruit** (`transfer_with_burn`, `amount / 100`, arithmétique
entière). L'usage lui-même resserre l'offre. Comme la subvention d'émission tend vers zéro tandis
que le burn persiste avec l'usage, QUANTA devient **net-déflationniste dès que le volume dépasse
la subvention (décroissante)** — la rareté est alors liée à l'utilité réelle. Détruire (plutôt
que reverser au validateur) est aussi une propriété de sécurité : cela rend le mécanisme
incitatif-compatible et résistant à la collusion.

### 6.4 Conservation — une loi, pas un espoir

```
Σ(dépensable + staké + en-déverrouillage) + brûlé  ==  miné
```

Cette égalité est gravée et testée sur tous les chemins (Stake/Unstake/Slash/reorg). Une pièce
ne se crée ni ne se perd ; elle se **déplace entre compartiments**. Une pièce naît par le minage
(vers *dépensable*), circule d'`@pseudo` en `@pseudo` en brûlant 1 % à chaque saut, se verrouille
par le staking (*dépensable* → *staké* → *en-déverrouillage* → *dépensable*), et meurt par le
burn d'usage ou par le slashing (*staké* → *brûlé*). Staker **déplace** des pièces sans les
brûler ; le déverrouillage est indexé par hauteur (`unlock = block.index +
UNBONDING_PERIOD_BLOCKS`, soit **10 080 blocs**, environ 2 semaines) et cette durée est
contrainte, par une assertion `const` au compile-time, à rester **au moins égale à la fenêtre de
slashing** — sinon un tricheur pourrait échapper à la punition en attendant le déverrouillage.

### 6.5 Couverture symétrique (COVER-1 / COVER-2)

La **couverture** garantit qu'aucune transaction ne dépense ou ne stake plus que le solde
on-chain ne le permet, selon une règle unique et purement fonction de la chaîne (jamais du
mempool), séquentielle, comptant les crédits intra-bloc, exemptant les adresses synthétiques
`NETWORK`/`ESCROW`/`BURN`. Elle est **symétrique** :

- **COVER-1 (réception)** — `validate_block_against_prev` **rejette** tout bloc reçu contenant une
  dépense ou un stake non couvert.
- **COVER-2 (production)** — `seal_block_at` **exclut** ces transactions à la production (revert du
  cache + éviction) pour émettre un bloc **valide par construction**.

L'invariant qui en résulte — tout bloc auto-scellé passe la validation de réception — ferme la
possibilité qu'un nœud corrompe sa propre chaîne.

### 6.6 Enjeu on-chain — staking, unstaking, unbonding

Le poids d'un validateur n'est plus lu d'un classement local mais d'un **état d'enjeu dans le
ledger** (ONCHAIN-STAKE-1), dérivé des transactions `Stake`/`Unstake` scellées, ancré à
`block.index`. Le solde se scinde en trois compartiments — **dépensable**, **staké** et
**en-déverrouillage** — et la conservation compte les trois. Un `Unstake` déplace des pièces du
compartiment staké vers un déverrouillage indexé par hauteur (10 080 blocs plus tard), pendant
lequel les pièces restent slashables (LIVE-3B). C'est la seconde moitié de la fermeture du vecteur
de fork évoqué en §4.1 : le poids de vote et le poids punissable sont deux vues pures de la même
chaîne.

### 6.7 Répartition entre nœuds — Shapley

Quand plusieurs nœuds contribuent, l'émission d'un tick est répartie par une valeur de Shapley
sur quatre dimensions (`p2p/shapley.rs`, somme des poids = 1.0) : **énergie 0.30 / travail 0.30 /
validation 0.25 / uptime 0.15**. En minage solo — le seul chemin testé en réel — 100 % du tick va
au mineur sans passer par Shapley.

> **Réserve honnête (héritée de la doctrine).** La clé de répartition paie encore partiellement au
> prorata des watts (`W_ENERGY = 0.30`), aggravé par le fait que la dimension « travail » est morte
> (`tasks_completed` câblé à 0 depuis le retrait des modules web). L'énergie **domine encore** le
> partage réel entre nœuds. C'est un vestige, pas une intention : la doctrine grave la direction de
> rétrograder l'énergie à un simple signal anti-sybil (le coût prouve la *présence*, il n'achète pas
> la valeur), et de la remplacer par un « Dividende du Commun » égalitaire — chantiers **non
> livrés**, hors du chemin de sécurité du consensus. À noter : cette couche de distribution
> n'entre **jamais** dans la *quantité* émise — `emission_for_tick` ne dépend que de `total_miné`.

---

## 7. Identité

L'identité d'un porteur est une **clé qu'il seul détient** ; on le joint par un court `@pseudo`
humain (`p2p/username.rs`), enregistré via `PublishUsername`, signé ML-DSA avec `lie` (la clé
publique révélée est liée à l'adresse, ce qui ferme le détournement de pseudo). Pas de compte,
pas de KYC, pas de dossier — un nœud souverain joignable par un nom lisible.

Les adresses sont au format **Bech32m à checksum `qta1…`**, de bout en bout depuis l'écosystème de
nœud, dérivées de la clé ML-DSA. Le daemon de référence `quanta-node` (headless) expose un
ensemble **JSON-RPC de 17 méthodes** (getinfo, getbalance, getblock, validateaddress,
getfinalityinfo, getvalidators, getmempool, listtransactions avec scan de dépôts,
sendrawtransaction, sendtoaddress, getmultisigaddress…), en µQTA entiers, avec l'offre prouvée
dans `getinfo`. Un explorateur web autonome et un mode `--public` en lecture seule complètent
l'ensemble. Ce sont précisément les briques qu'une intégration d'exchange attend d'un coin :
génération d'adresse, consultation de solde, construction et diffusion de transactions, suivi des
dépôts, et une réponse **nette** à « combien de confirmations avant de créditer ? » — le plancher
de finalité, pas une probabilité.

La récupération repose sur une **phrase BIP39 de 24 mots** : le secret de fonds est la **graine
ML-DSA** (et non Ed25519). Le vault d'identité chiffre les clés dérivées Argon2id sous
AES-256-GCM (`security/pq_vault.rs`). Sur macOS, un KEK aléatoire est stocké dans le Keychain
derrière `SecAccessControl(.BIOMETRY_CURRENT_SET)` — Touch ID réel : l'OS exige l'empreinte à la
lecture et invalide l'item si les empreintes changent ; le mot de passe n'est jamais stocké et
reste le repli. Un `UnlockGuard` anti-brute-force applique un backoff exponentiel, partagé entre
le mot de passe et la biométrie.

L'identité de marque (`docs/brand/BRAND.md`) est « l'anneau et le quantum » : un Q géométrique en
deux traits — l'anneau (le Torus, le réseau) et la queue diagonale qui traverse la brèche (le
bloc en train d'être scellé). Une seule couleur possède la marque, le teal joyau ; le thème de
l'app est volontairement blanc, jamais sombre, dans une esthétique épurée de qualité bancaire.

---

## 8. Doctrine économique — le Bien Commun Souverain

La constitution économique (`docs/economy/DOCTRINE.md`) tient en un paradoxe que le code fait
tenir : **la souveraineté individuelle absolue** (ta clé, c'est toi ; ta part est intouchable)
**et** **le bien commun** (la monnaie n'appartient à personne, se garde par tous, paie ses
gardiens) — dans le même objet. Une monnaie qui appartient à tous *parce qu'elle n'appartient à
personne*, et qui rend chacun maître absolu de sa part.

### 8.1 D'où vient la valeur, sans marché ni prix

**Jamais du coût de production** — une pièce qui aurait coûté mille kilowattheures mais qu'on
pourrait réinflater, réverser ou geler vaudrait zéro. Attribuer la valeur au coût est une erreur
que l'on traîne depuis Bitcoin. La valeur de QUANTA est l'**ensemble des garanties qu'aucun tiers
ne peut corrompre**, chacune re-vérifiée par chaque nœud à chaque bloc : une **rareté** que
personne ne peut diluer, une **propriété** absolue (nulle autorité de saisie), une **finalité**
irréversible que même une majorité ne peut réécrire, une **permanence** qui traverse le saut
post-quantique. Ce qu'on détient n'est pas la promesse d'une institution, mais **l'absence
d'institution capable de la briser** — l'exact inverse d'un billet fiat, dont la valeur dépend
entièrement de la retenue de celui qui tient la planche à billets.

### 8.2 Le doute énergétique, tranché

Une intuition fondatrice a lancé cette doctrine : récompenser au prorata des watts ressemble au
« brûler de l'électricité pour gagner » de Bitcoin. Le code y répond au niveau structurel :
`emission_for_tick` ne dépend **que** de `total_miné` — les watts n'entrent nulle part dans la
*quantité émise*. Que le réseau brûle 3 W ou 3 MW, un tick libère exactement le même nombre de
pièces. Le péché du PoW (« plus tu brûles, plus le réseau émet ») est déjà **absent** ; la
sécurité vient du stake et de la finalité, pas du hashrate. Ne subsiste que le résidu de la clé de
répartition entre nœuds (§6.7), identifié comme à corriger — un vestige, pas une intention.

### 8.3 L'âme choisie — permanence par défaut, circulation par volonté

Le carrefour monétaire — *or dur* (les pièces sont éternelles) contre *monnaie vivante* (les
pièces doivent circuler) — n'a pas été tranché en faveur d'un camp, mais **rendu choisissable par
le porteur**, ce qui est l'expression la plus pure du Bien Commun Souverain. Par défaut, tes
pièces sont à toi indéfiniment, sans érosion. Sur option, tu peux en confier au « Courant » qui
les fait couler vers les gardiens actifs — ton or privé dort tranquille, seul ce que tu confies
coule. Personne ne t'impose une philosophie monétaire : tu choisis celle de tes propres pièces.

### 8.4 Les mécanismes audacieux — et leur statut honnête

Cinq mécanismes ont été inventés puis passés à l'épreuve du code. Ils vivent tous dans la **couche
distribution**, aujourd'hui **hors du chemin de sécurité du consensus** (le module Shapley porte
encore un `#![allow(dead_code)]`, et le seul chemin testé en réel — le minage solo — verse 100 %
du tick sans même passer par Shapley). **Aucun n'est en production ; les graver est de
l'ingénierie à faire, pas un fait acquis.** Leurs paramètres relèvent d'une ratification du
fondateur. Résumé fidèle, avec statut :

- **Le Dividende du Commun** (*la vedette réalisable*) — chaque tick d'émission se scinde en deux
  poches : *le commun* (une fraction φ répartie en **parts égales** entre tous les gardiens vivants
  prouvés — un Raspberry Pi et un data-center touchent la même part de base) et *le mérite* (le
  reste, par contribution mesurée). La création monétaire porte ainsi une clause d'égalité,
  **anti-baleine par construction** (le plancher ne scale pas avec la puissance). *Statut :* 🔵 cap,
  petit chantier, ne casse aucun invariant (on redistribue *qui reçoit* le tick, on ne crée pas un
  µQTA). Réserve : le compte des présents est une vue **locale** — donc une équité *sociale*,
  bornée par le plafond consensus, pas une garantie de consensus tant qu'un `TxType::Presence`
  ancré on-chain n'existe pas.
- **Le Legs Scellé** (*la permanence rendue tangible*) — une « volonté comme code » signée ML-DSA
  désignant des bénéficiaires et un déclencheur indexé par hauteur : un **verrou-temps** (débloqué
  au bloc X) ou un **dead-man's switch** (si la clé ne signe plus depuis N blocs, avec fenêtre de
  veto). Le self-custody souverain cesse d'avoir pour bug fatal « si je meurs, tout est perdu ».
  *Statut :* 🔵 cap ; le verrou-temps est propre et réalisable, la dormance (déduire la mort de
  l'inactivité) est un chantier lourd à ne jamais confondre avec une preuve cryptographique.
- **Le Courant** (*la circulation, en opt-in strict*) — une fraction minuscule des soldes **oisifs
  d'un pool communautaire** est prélevée (jamais brûlée) au fil des époques et redistribuée aux
  nœuds actifs ; l'or gardé hors du pool n'est jamais touché. C'est du Gesell/Wörgl porté par la
  cryptographie post-quantique. *Statut :* 🔵 cap, chantier lourd (hard-fork) ; il manque un suivi
  d'activité par compte. L'opt-in strict est ce qui le sauve : il ne trahit jamais la permanence
  par défaut.
- **La Paie du Sceau** (*l'étoile polaire*) — le renversement le plus pur : l'émission d'une époque
  est accumulée en escrow et n'est mintée **que lorsqu'un certificat de ⅔ finalise** cette époque,
  répartie au prorata du stake qui a réellement signé. Pas de certificat, pas de mint. La monnaie
  ne naît plus du temps qui passe mais du bien commun produit — l'irréversibilité elle-même — et
  cela *renforce* les invariants (non-finalisé = non-minté). *Statut :* 🌟 étoile polaire, pas
  prochain pas ; chantier XL (le mint doit passer de « chaque nœud se verse son tick » à « seul un
  certificat autorise le versement »).

Un cinquième mécanisme envisagé — « La Rosée », un nivellement anti-baleine par rendements
décroissants — a été écarté : le Dividende du Commun le subsume, moins cher et moins exposé au
sybil.

---

## 9. Décisions d'architecture (ADR)

Les arbitrages de consensus et de sécurité sont tracés dans un registre d'ADR
(`docs/decisions/`). Le principe : une décision n'est jamais devinée — elle est cadrée (options +
conséquences) puis tranchée. Résumé digeste, une entrée par décision.

**ADR-001 — Fork-choice.** *Problème :* le code ne savait départager qu'un fork mono-bloc à
hauteur égale, par « hash le plus haut » — grindable sans stake, et incapable de réconcilier
au-delà d'un bloc de divergence. *Choix :* résoudre le point à la racine par le finality gadget
(un bloc finalisé n'est jamais réorganisé) et, dans la fenêtre non-finalisée, remplacer le
départage par hash par un LMD-GHOST pondéré par le stake on-chain (`ghost_head`), plus un reorg
multi-blocs (`reorg_to_fork`) borné par le plancher. *Pourquoi :* une sémantique Sybil-résistante
alignée sur le PoS, et une convergence garantie jusqu'à la profondeur de finalité.

**ADR-002 — Validator set & comité BFT.** *Problème :* chaque nœud construisait l'ensemble des
validateurs depuis sa réputation **locale** ; rien ne garantissait deux nœuds calculant le même
ensemble, donc des leaders différents au même slot, donc un fork. *Choix :* ancrer éligibilité et
poids au **stake on-chain seul**, dérivé déterministiquement à la frontière d'époque ; la
réputation sort du chemin de sécurité (elle reste un signal applicatif pour le minage/Shapley).
*Pourquoi :* l'ensemble des validateurs devient un objet de consensus, vérifiable et
Sybil-ancré ; c'est le préalable au slashing (on ne slashe que du stake on-chain).

**ADR-003 — Slashing (accountable safety).** *Problème :* un leader pouvait équivoquer (sceller
deux blocs au même slot) **sans coût** ; la seule sanction était un ban réseau social,
manipulable. *Choix :* un `TxType::Slash` dont l'autorité est une **preuve** d'équivocation
(double-vote ou surround) re-vérifiée par chaque nœud, avec une politique gravée par ADR-009 :
enjeu **brûlé** (pas redistribué), montant **plein**, fenêtre de preuve **égale à l'unbonding**.
*Pourquoi :* une dissuasion réelle et une *accountable safety* prouvable, indispensables au BFT ;
brûler renforce la rareté et simplifie la comptabilité.

**ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF).** *Problème :* le beacon enterré rend le leader
**publiquement prévisible** (ciblable) et laisse un grinding long-horizon ouvert. *Choix :*
**garder le beacon enterré** pour cette phase, en actant le compromis. *Pourquoi :* la liveness
est portée par le comité (quorum) et le slashing, pas par le secret du leader ; un vrai VRF
(imprévisibilité) et un VDF (anti-grinding) introduiraient soit une primitive non-PQ, soit une
complexité de calibrage — ils restent au roadmap, non livrés.

**ADR-005 — Agrégation des votes & certificats de finalité.** *Problème :* le format des votes
paraissait imposer un choix entre BLS (compact mais non post-quantique) et ML-DSA (post-quantique
mais non agrégeable, ~3,3 Ko par vote). *Choix :* **ML-DSA pur, finalisation par époque**, derrière
une abstraction de certificat. *Pourquoi :* un vote est éphémère (le BLS n'y ajoute aucune sécurité
de long terme), et la finalisation par époque amortit la taille (un certificat par lot de blocs,
élagable) ; on gagne une finalité PQ sans astérisque et une attribution de faute directe.

**ADR-006 — Gouvernance & évolutivité.** *Problème :* comment le protocole change-t-il une fois le
réseau vivant, sans ouvrir une surface d'attaque de gouvernance ni glisser vers la ploutocratie ?
*Choix :* **pas de gouvernance on-chain** ; un noyau monétaire (plafond, émission, signatures PQ,
conservation, sûreté) **immuable par construction** — non pas verrouillé, mais sans porte : aucun
chemin de code ne le change — et une périphérie ajustable par **fork volontaire**. *Pourquoi :* on
ne protège pas un invariant en le rendant difficile à modifier, mais en ne lui donnant aucun
chemin de modification ; une porte absente ne se crochète pas. Aucun mécanisme de gouvernance
dormant n'est laissé dans le code.

**ADR-007 — Portée du post-quantique (comptes ML-DSA).** *Problème :* l'audit CRYPTO-ID-1 a prouvé
que « entièrement post-quantique » n'était pas tenu — les comptes étaient enracinés Ed25519, la
clé ML-DSA non liée. *Choix :* re-raciner les comptes **entièrement en ML-DSA** (option b), au
prix du plus gros chantier du projet, plutôt qu'un registre léger laissant un astérisque
permanent (option a). *Pourquoi :* pour une monnaie dont la raison d'être est le post-quantique,
l'astérisque n'est pas un compromis mais un renoncement, sur exactement le différenciateur du
projet ; une signature de transaction est de longue vie (« récolter aujourd'hui, forger demain »).

**ADR-008 — Autorité de tx via liaison ML-DSA on-chain.** *Problème :* l'identifiant de compte
est **unifié** (le même `from` sert de clé de solde, de cible de minage, d'enjeu, de liaison
`@pseudo`), donc le migrer semblait re-clé fatalement l'enjeu et le minage du même geste. *Choix :*
d'abord un registre de liaison en gardant `from` Ed25519 (option a), puis **reversé** — `from`/`to`
deviennent l'adresse ML-DSA partout. *Pourquoi :* l'unification, lue d'abord comme un blocage, est
en réalité la justification de (b) : « tout bascule ensemble, c'est voulu » ; la liaison
clé↔compte cesse d'être un état (registre) pour devenir une fonction sans état, fermant
CRYPTO-ID-1 intrinsèquement.

**ADR-009 — Frontière gravé/ajustable et valeurs du §12.** *Problème :* ADR-006 posait le principe
mais laissait ouvertes la frontière exacte et les valeurs (longueur d'époque, quorum, enjeu
minimum…). *Choix :* ratifier les constantes existantes **sans en changer aucune** — gravé pour le
monétaire (100 M, émission, burn 1 %, µQTA, zéro premine) et les invariants de sûreté (quorum ⅔,
slash brûlé, fenêtre ≤ unbonding) ; ajustable par fork pour les réglages opérationnels (E = 32,
unbonding 10 080, fraction de slash, `MIN_VALIDATOR_STAKE`). *Pourquoi :* nommer la frontière rend
ADR-006 opérationnel ; figer aussi les réglages opérationnels coûterait un fork pour le moindre
ajustement de cadence, sans bénéfice de sûreté. L'échelle monétaire (et donc la valeur sensée de
`MIN_VALIDATOR_STAKE`) reste explicitement une décision du fondateur.

---

## 10. Limites, statut, roadmap honnête

Ce livre blanc ne vaut que s'il distingue le réel du cap.

**Réel aujourd'hui, vérifiable en lecture et en test** — le consensus PoS + finalité Casper-FFG
vivant (LIVE-1→4), la cryptographie post-quantique de l'argent, de la finalité et du transport, le
plafond dur et l'émission décroissante vérifiés au consensus, la conservation et la couverture
symétrique, le multisig PQ, l'écosystème de nœud (daemon + 17 méthodes RPC + explorateur). Le tout
couvert par **441 tests + 1 intégration** (dont un échange gossip réel à 2 nœuds et l'invariant de
déterminisme C1 sur 128 runs), clippy propre.

**Ce qui reste — sans maquillage :**

- **Élection = déterministe, pas VRF.** Le leader est publiquement prévisible ; un vrai VRF
  (imprévisibilité) + un VDF (anti-grinding) sont au roadmap, non livrés.
- **NodeId Iroh = Ed25519.** Le seul primitif classique restant, dette en amont, hors de notre
  code ; bascule le jour où Iroh livre un EndpointId post-quantique.
- **Résidu énergétique.** `W_ENERGY = 0.30` pèse encore réellement dans le partage entre nœuds ;
  la correction (rétrograder l'énergie en signal anti-sybil, livrer le Dividende du Commun) est la
  direction, pas l'état.
- **Couche distribution hors consensus.** Les mécanismes de la doctrine (Dividende, Legs, Courant,
  Paie du Sceau) sont des caps non livrés ; les graver est de l'ingénierie à faire.
- **Non audité.** Aucune de ces garanties n'a reçu de revue de sécurité externe indépendante. La
  vérification « 2 machines physiques » est réelle mais n'est pas une preuve à l'échelle. Un audit
  de sécurité par un tiers reconnu est un prérequis quasi obligatoire à toute cotation, non
  contournable par du code seul.
- **Listing readiness — partiel.** L'économie (offre prouvable, zéro premine, finalité
  déterministe, profil « utility ») est un atout ; le volet technique (daemon + RPC + explorateur)
  est amorcé ; mais l'audit, le juridique et la liquidité exigent des tiers et de l'adoption, hors
  de portée d'un document ou d'une seule personne qui code. Ce n'est pas un avis juridique.
- **Aucun prix.** QUANTA n'a pas de marché ; rien ici ne prétend ni ne prédit qu'un QUANTA vaudra
  quoi que ce soit en échange.

---

## 11. Références internes (code)

Toutes vérifiables dans `src-tauri/src/` :

- Émission & rareté : `p2p/reputation.rs` (`MAX_SUPPLY_MICRO`, `EMISSION_DIVISOR = 50_000_000`,
  `emission_for_tick`), `p2p/ledger.rs` (`validate_block_emission`, `mine_tx`,
  `transfer_with_burn`, couverture, plancher de finalité, slash), `p2p/ledger_types.rs`
  (`MICRO = 1_000_000`).
- Consensus PoS : `p2p/pos_consensus.rs` (élection, `MIN_VALIDATOR_STAKE`, `LEADER_TIMEOUT_SECS`,
  `MAX_FALLBACK_ROUNDS`, `LEADER_ENTROPY_LOOKBACK = 2`).
- Gadget de finalité (Casper-FFG) : `sm/finality.rs` (`EPOCH_LENGTH_BLOCKS = 32`),
  `finality_vote.rs` (quorum ⅔, `meets_supermajority`), `finality_rule.rs`,
  `finality_slashing.rs`, `fork_choice.rs` (LMD-GHOST) ; harnais DST `sm/sim.rs`.
- Câblage vivant : `p2p/finality_live.rs` (LIVE-1→3B), `p2p/fork_heal.rs` (LIVE-4).
- Cryptographie : `security/mod.rs`, `security/hybrid_crypto.rs` (ML-DSA-65),
  `security/pq_vault.rs` (Argon2id + AES-256-GCM), `security/cipher.rs`.
- Réseau : `p2p/gossip.rs` (`TORUS_PROTOCOL_VERSION = 6`), `p2p/dispatcher.rs`,
  `p2p/willow_node.rs` (Iroh QUIC), `p2p/gossip_tasks.rs`, `p2p/mining_loop.rs`
  (`MINE_INTERVAL_SECS = 60`, `SEAL_EVERY_N_TICKS = 2`).
- Répartition : `p2p/shapley.rs` (poids énergie 0.30 / travail 0.30 / validation 0.25 /
  uptime 0.15).
- Identité & nœud : `p2p/username.rs`, `security/mod.rs` (adresses `qta1…`), le daemon
  `quanta-node` et son JSON-RPC.
- Décisions : `docs/decisions/ADR-001…009`. Doctrine & marque : `docs/economy/DOCTRINE.md`,
  `docs/brand/BRAND.md`.

Les concepts cités — Casper-FFG, LMD-GHOST/Gasper, FIPS 204 (ML-DSA), ML-KEM, Bech32m, BIP39,
Argon2id, BLAKE3 — sont des standards ou constructions publics réels ; ce document n'invente
aucune référence académique ni URL.

---

*Document vivant. Les affirmations « réel » sont vérifiables dans `src-tauri/src/` à la date de
rédaction (`TORUS_PROTOCOL_VERSION = 6`, 441 tests + 1 intégration). Le roadmap décrit des
intentions, pas des livraisons. Statut du projet : alpha, non audité par un tiers. QUANTA n'a
aucun marché ni prix.*
