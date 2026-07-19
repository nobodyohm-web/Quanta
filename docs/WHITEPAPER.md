# Quanta — Livre blanc

> **Une monnaie souveraine P2P — sans serveur, sans cloud, sans intermédiaire.**
> Version protocole `TORUS_PROTOCOL_VERSION = 6` · App v3.4 (crypto-only) · Coin **QUANTA** ·
> Licence Apache-2.0
> **Statut : alpha, non audité par un tiers.** P2P vérifié entre deux machines physiques
> (06/05/2026), pas une preuve à l'échelle. Aucune revue de sécurité externe indépendante à ce jour.

---

## 1. Résumé

Quanta est une cryptomonnaie pair-à-pair conçue autour d'une seule idée : **retirer le
prometteur**. Là où une monnaie fiat repose sur la retenue d'une banque centrale et où une
plateforme repose sur un serveur qu'on peut saisir ou geler, Quanta ne repose sur personne. Il
n'y a pas de fonction pour imprimer, pas de compte à geler, pas d'entreprise à sommer, pas
d'autorité d'émission — pas même l'auteur du projet.

Techniquement, Quanta combine : un transport P2P **Iroh (QUIC)** avec **gossip** pub/sub ; un
consensus **Proof-of-Stake** à élection de proposeur déterministe et publiquement vérifiable,
pondérée par un **enjeu inscrit on-chain** ; un **gadget de finalité de type Casper-FFG** qui
rend l'histoire mathématiquement irréversible après un certificat de ⅔ du stake ; et une pile
cryptographique **post-quantique** — l'autorité de compte, les votes de finalité et les
enveloppes réseau sont signés **ML-DSA-65 (FIPS 204)**, l'échange de clés de transport est
l'hybride **X25519MLKEM768**. L'offre est plafonnée dur à **100 000 000 QUANTA**, sans premine,
avec une émission décroissante et un burn de 1 % à chaque transfert.

Ce document décrit l'architecture telle qu'elle existe dans le code (`src-tauri/src/`), en
distinguant scrupuleusement ce qui est **réel** de ce qui reste **au roadmap**. Conformément à
la doctrine du projet : **QUANTA n'a aucun marché ni prix.** Aucune valeur monétaire n'est
avancée ni prédite nulle part.

---

## 2. Le problème — une monnaie sans gardien

Toute l'histoire de la monnaie est une suite de promesses tenues par quelqu'un qui avait le
pouvoir de les rompre : le roi qui rogne la pièce, la banque centrale qui imprime, la plateforme
qui gèle le compte. Chaque fois, la confiance reposait sur la retenue d'un détenteur du pouvoir.

Bitcoin a montré qu'une monnaie sans émetteur était possible, mais a laissé deux failles : une
finalité seulement **probabiliste** (« attendez six confirmations ») et une sécurité adossée à
une course énergétique. Les Proof-of-Stake classiques apportent une finalité déterministe, mais
signent avec une cryptographie (Ed25519, ECDSA) qu'un ordinateur quantique brisera.

Quanta cherche la **perfection réseau** au service d'une seule chose : une monnaie saine, rare
et vérifiable. Cela impose cinq objectifs : un protocole P2P unifié et versionné (Torus) ; des
échanges déterministes à convergence garantie ; un réseau robuste (reconnexion, discovery,
NAT traversal) ; une blockchain de production (PoS stable, résolution de fork déterministe) ;
et une monnaie vérifiable (plafond dur gravé, zéro premine, zéro autorité d'émission).

---

## 3. Architecture & protocole Torus

Le backend est écrit en **Rust (Tauri 2.0, édition 2021)**, le frontend en **Svelte 5**. Le
cœur du consensus vit dans un module **sans-IO déterministe** (`sm/`) — horloge et RNG injectés
— ce qui permet de le simuler exhaustivement (harnais DST multi-seed, invariant de déterminisme
C1) indépendamment de la couche réseau.

Le protocole Torus s'organise en cinq couches :

```
Application  ← Wallet, identité @pseudo, staking
Protocole    ← GossipMessage (Hello, RequestChain, ChainSegment, NewBlock,
               BroadcastTx, Ping/Pong, PublishUsername, ReportPeer, FinalityVote)
Sécurité     ← GossipEnvelope (signature + nonce monotone + timestamp)
Transport    ← Iroh QUIC + iroh-gossip pub/sub
Réseau       ← NAT traversal + relay + hole punching
```

Un nœud qui rejoint le réseau se présente par un `Hello` (portant sa `chain_height`) ; les pairs
comparent les hauteurs et synchronisent via `RequestChain` → `ChainSegment` (paginé, max 50
blocs par segment). Le `Hello` est ré-émis toutes les 120 s, avec un `Ping` léger toutes les
15 s pour la liveness, et un nettoyage des pairs morts (TTL 5 min).

### Pipeline de sécurité (dispatch_incoming)

Chaque message entrant traverse neuf étapes avant d'être traité :

```
① Size guard (max 10 MB par enveloppe)
② JSON deserialize → GossipEnvelope
③ Ban check (per-peer)
④ Dedup (seen_messages, LRU 100 000)
⑤ Timestamp freshness (±90 s)
⑥ Rate limit adaptatif (base 30 msg/min/peer, sqrt-scaling, borné [15, 120])
⑦ Nonce anti-replay (monotone par expéditeur)
⑧ Vérification de signature (ML-DSA-65 depuis le hard-fork v4)
⑨ Dispatch vers le handler
```

Le durcissement réseau (NET-3 → NET-16) ajoute une file de priorité sortante à 4 voies, la sync
parallèle (4 fenêtres × 50 blocs), la compression gzip optionnelle des segments, des métriques
par pair (RTT EWMA, score qualité 0-100), une heuristique anti-eclipse (alerte si >80 % des
pairs partagent un préfixe de clé), un mempool à TTL 10 min plafonné à 1000 tx, et le bump de
`TORUS_PROTOCOL_VERSION` (aujourd'hui **6**) qui signale les pairs incompatibles.

---

## 4. Consensus — PoS pondéré par l'enjeu + finalité Casper-FFG

### 4.1 Élection du proposeur

À chaque slot (= hauteur de chaîne), le proposeur est élu de façon **déterministe** :

```
beacon = BLAKE3(domaine ‖ hash_du_bloc_enterré ‖ slot)   (enterré = LOOKBACK slots derrière le tip)
seed   = BLAKE3(domaine ‖ beacon ‖ slot ‖ round)
seed % total_weighted_stake → leader
```

Le poids d'un validateur est son **enjeu inscrit sur la chaîne** (`ledger.validator_stakes()`),
dérivé des transactions `Stake`/`Unstake` scellées — une **fonction pure de la chaîne**,
identique sur chaque nœud (live, restauré ou synchronisé). C'est ce qui ferme le vecteur de
fork : le poids ne vient plus d'une vue locale (réputation), mais de l'état du ledger. Le stake
minimum est `MIN_VALIDATOR_STAKE = 1 000 000 µQTA` (1 QUANTA, placeholder ajustable). Si le
leader ne produit pas, un fallback bascule au suivant après 30 s (max 3 rounds). Tant que
personne n'a staké, l'élection est permissionless (bootstrap).

**Nommage honnête.** Cette élection est *déterministe et publiquement vérifiable* — ce **n'est
pas** un VRF cryptographique : sans clé secrète, le leader est publiquement prévisible. Le beacon
enterré (un bloc plusieurs slots derrière le tip) empêche l'auto-grinding immédiat. Un vrai VRF
(imprévisibilité) et un VDF (anti-grinding) sont au roadmap, non livrés. Les identifiants
internes `vrf` sont des noms hérités gardés pour la compatibilité.

Depuis le hard-fork v4 (PROPOSER-1), le proposeur est vérifié **à la réception** et non plus
seulement au seal : le validateur partagé `validate_block_against_prev` rejette tout bloc
non-genèse dont le proposeur n'est pas un validateur bondé as-of-parent. La règle est
non-temporisée (« tout validateur bondé »), sur-ensemble de ce que le seal produit — donc
symétrie produce/receive, zéro fork par dérive d'horloge.

### 4.2 Le gadget de finalité

Au-dessus de l'élection vit un **gadget de type Casper-FFG** (`sm/finality*.rs`) qui rend
l'histoire irréversible — la propriété que Bitcoin n'a pas. Il est écrit pur et déterministe,
prouvé en simulation DST, et vivant sur le réseau depuis les chantiers LIVE-1→4 :

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

L'identité de vote est la clé publique ML-DSA ; le pont `validator_stakes_by_pubkey` re-clé
l'enjeu purement depuis la chaîne (chaque tx `Stake` révèle sa `pq_public_key`). Deux nœuds aux
mêmes votes et à la même chaîne finalisent **identiquement**.

Le câblage vivant est complet :

- **LIVE-1** — les votes circulent (`GossipMessage::FinalityVote`, étape ⑨ du dispatcher,
  `FinalityTracker`, cast au tick de mining) et peuplent l'état de finalité du ledger.
- **LIVE-2** — un **plancher de finalité** (`finalized_floor_index`, monotone, vérifié par
  hash, persisté) alimenté par les certificats ⅔ : `integrate_remote_block` **refuse** tout
  fork qui remplacerait un bloc ≤ plancher. L'histoire finalisée est irréversible sur le réseau
  vivant ; le départage lexicographique libre ne joue qu'**au-dessus** du plancher (Gasper).
- **LIVE-3 / 3B** — l'équivocation détectée à l'ingest devient une tx `Slash` (autorité = preuve
  embarquée, re-vérifiée par chaque nœud) qui détruit l'enjeu de l'offenseur **STAKE→BURN**,
  conservation neutre. La base slashable inclut le stake en cours de déverrouillage (ferme le
  « unstake-and-run »), avec une ventilation de consommation déterministe restaurée exactement
  au reorg.
- **LIVE-4** — `ForkReconciler` (`p2p/fork_heal.rs`) réconcilie les forks profonds : règle de
  victoire = plus-longue-au-dessus-du-plancher + départage lexicographique, appliquée via
  `reorg_to_fork` sur clone d'essai. Deux partitions qui scellent chacune ≥2 blocs convergent
  désormais en vivant.

---

## 5. Cryptographie post-quantique

Quanta sépare deux rôles cryptographiques et migre chacun vers le post-quantique.

**Autorité de compte = ML-DSA-65 (FIPS 204), pure.** Depuis PQ-MIG-3B, `from`/`to` sont des
**adresses ML-DSA** (`BLAKE3(ADDR_DOMAIN ‖ clé)`) partout — solde, récompense de minage,
enjeu, `@pseudo`. La vérification de transaction (`verify_tx`) est purement ML-DSA ; le
co-facteur Ed25519 a été retiré du chemin d'autorité. Les votes de finalité et, depuis le
hard-fork v4 (PQ-ENVELOPE-1), **toutes les enveloppes gossip** sont signés ML-DSA-65.

**Transport = hybride post-quantique.** Le fournisseur rustls est passé à `aws-lc-rs` avec
`prefer-post-quantum` : l'échange de clés QUIC/TLS 1.3 négocie l'hybride **X25519MLKEM768**
(ML-KEM-768 ⊕ X25519), une défense « harvest-now-decrypt-later » sur la confidentialité du
transport. Non destructif (négociation TLS, dégradation gracieuse — ni bump de protocole, ni
reset de genèse).

**Primitives symétriques.** AES-256-GCM (chiffrement), Argon2id (dérivation de clé du vault),
BLAKE3 (hachage, Merkle, beacon). Tous les secrets sont `zeroize()`. Ed25519 subsiste comme
**transport** historique (co-facteur tx vestigial).

**Multisig PQ natif (MSIG-1).** Custody quorum M-of-N entièrement ML-DSA : l'adresse commet à
sa politique `BLAKE3(MSIG_DOMAIN ‖ clés triées ‖ seuil)`, l'autorité est portée en JSON dans le
champ `pq_signature` (zéro nouveau champ wire, tx mono-clé byte-identiques). `verify_multisig`
exige ≥ seuil signataires distincts valides et est rebind-proof. C'est la première custody
quorum post-quantique du projet (contourne l'absence de threshold-ML-DSA standard).

**Ce qui n'est PAS encore post-quantique — la seule dette honnête.** L'**identifiant de nœud**
réseau (le `NodeId` d'Iroh) reste **Ed25519**. C'est une dette **en amont** (Iroh attend un
consensus d'industrie sur la signature PQ des EndpointIds), hors de notre code. Concrètement :
un adversaire quantique pourrait usurper l'*identité réseau* d'un nœud en temps réel, mais **ne
peut ni forger une transaction ou un vote de finalité** (ML-DSA) **ni déchiffrer le trafic
passé** (X25519MLKEM768). Bascule prévue le jour où Iroh livrera un EndpointId post-quantique.

Bilan : argent + finalité + confidentialité du transport = PQ ; il ne reste que l'auth de nœud.

---

## 6. Tokenomics

### 6.1 Rareté prouvée, pas promise

Le plafond est **dur : 100 000 000 QUANTA** (`MAX_SUPPLY_MICRO = 100_000_000 × MICRO`). Il n'est
pas écrit sur un site : il est **vérifié au consensus** (`validate_block_emission`), qui fait
rejeter tout bloc qui porterait l'offre au-delà — sur le chemin linéaire comme sur les reorgs.
`1 QUANTA = 1 000 000 µQTA` ; toute la comptabilité est en `u64`/`u128`, jamais un `float` ne
touche un solde (règle Rust #6).

### 6.2 Émission décroissante, front-loaded

```
emission_for_tick(total_miné) = (MAX_SUPPLY_MICRO − total_miné) / EMISSION_DIVISOR
                                 EMISSION_DIVISOR = 50 000 000
```

Une **fonction pure** de ce qui est déjà miné : chaque tick libère `1/50 000 000` de l'offre
*restante*. Décroissance géométrique — ~2 QUANTA par tick à la genèse (~120/h), puis une longue
traîne qui approche le plafond en asymptote sans jamais l'atteindre (la division entière ne
solde jamais le dernier µQTA). **Zéro premine** (la genèse alloue une table vide, testé) ; la
seule origine de pièces dans tout le code est `mine_tx(from = "NETWORK", TxType::Mining)`,
plafonnée par `emission_for_tick`. Aucune clé, aucun rôle fondateur ne peut fabriquer un µQTA
hors de la courbe.

Le minage tourne dans `mining_loop.rs` : toutes les 60 s un tick de minage, un seal toutes les
2 ticks (~2 min/bloc, soit ~720 blocs/jour).

### 6.3 Déflation d'usage

**1 % de chaque transfert est détruit** (`transfer_with_burn`, `amount / 100`, arithmétique
entière). L'usage lui-même resserre l'offre.

### 6.4 Conservation — une loi, pas un espoir

```
Σ(dépensable + staké + en-déverrouillage) + brûlé  ==  miné
```

Gravée et testée sur tous les chemins (Stake/Unstake/Slash/reorg). Une pièce ne se crée ni ne se
perd ; elle se déplace entre compartiments. Staker **déplace** des pièces (ne les brûle pas) ;
le déverrouillage est indexé par hauteur (`unlock = block.index + UNBONDING_PERIOD_BLOCKS`,
soit **10 080 blocs**, ~2 semaines — contrainte gravée : ≥ fenêtre de slashing, garantie par
const-assert). Un slash détruit l'enjeu STAKE→BURN, conservation neutre.

La **couverture** est symétrique : `validate_block_against_prev` rejette à la réception tout
bloc contenant une dépense ou un stake non couvert par le solde on-chain (COVER-1), et
`seal_block_at` exclut ces tx à la production pour émettre un bloc valide par construction
(COVER-2) — un nœud ne peut plus corrompre sa propre chaîne.

### 6.5 Répartition entre nœuds — Shapley

Quand plusieurs nœuds contribuent, l'émission d'un tick est répartie par une valeur de Shapley
sur quatre dimensions (`p2p/shapley.rs`, somme des poids = 1.0) : **énergie 0.30 / travail 0.30
/ validation 0.25 / uptime 0.15**. En minage solo — le seul chemin testé en réel — 100 % du
tick va au mineur sans passer par Shapley.

**Réserve honnête (héritée de la doctrine).** La clé de répartition paie encore partiellement au
prorata des watts (`W_ENERGY = 0.30`), aggravé par le fait que la dimension « travail » est
morte (`tasks_completed` câblé à 0 depuis le retrait des modules web). L'énergie **domine encore**
le partage réel entre nœuds. C'est un vestige, pas une intention : la doctrine grave la direction
de rétrograder l'énergie à un simple signal anti-sybil (le coût prouve la *présence*, il n'achète
pas la valeur), et de la remplacer par un « Dividende du Commun » égalitaire — chantiers **non
livrés**, hors du chemin de sécurité du consensus.

---

## 7. Identité

L'identité d'un porteur est une **clé qu'il seul détient** ; on le joint par un court `@pseudo`
humain (`p2p/username.rs`), enregistré via `PublishUsername`, signé ML-DSA + `lie` (la clé
publique révélée est liée à l'adresse). Pas de compte, pas de KYC.

Les adresses sont au format **Bech32m à checksum `qta1…`** (de bout en bout depuis l'écosystème
de nœud), dérivées de la clé ML-DSA. Le daemon de référence `quanta-node` expose une méthode
`validateaddress` et un ensemble JSON-RPC (getinfo, getbalance, getblock, sendtoaddress,
getmultisigaddress…), briques attendues par une intégration d'exchange.

La récupération repose sur une **phrase BIP39 de 24 mots** : le secret de fonds est la **graine
ML-DSA** (et non Ed25519). Le vault d'identité chiffre les clés dérivées Argon2id sous AES-256-GCM
(`security/pq_vault.rs`). Sur macOS, un KEK aléatoire est stocké dans le Keychain derrière
`SecAccessControl(.BIOMETRY_CURRENT_SET)` — Touch ID réel : l'OS exige l'empreinte à la lecture
et invalide l'item si les empreintes changent ; le mot de passe n'est jamais stocké et reste le
repli. Un `UnlockGuard` anti-brute-force applique un backoff exponentiel.

L'identité de marque (`docs/brand/BRAND.md`) est « l'anneau et le quantum » : un Q géométrique
en deux traits — l'anneau (le Torus, le réseau) et la queue diagonale qui traverse la brèche
(le bloc en train d'être scellé). Une seule couleur possède la marque, le teal joyau (#0BA5A0) ;
le thème de l'app est volontairement blanc, jamais sombre.

---

## 8. Doctrine économique — le Bien Commun Souverain

La constitution économique (`docs/economy/DOCTRINE.md`) tient en un paradoxe que le code fait
tenir : **la souveraineté individuelle absolue** (ta clé, c'est toi ; ta part est intouchable)
**et** **le bien commun** (la monnaie n'appartient à personne, se garde par tous, paie ses
gardiens) — dans le même objet.

D'où vient la valeur, sans marché ni prix ? **Jamais du coût de production** — une pièce qui
aurait coûté mille kilowattheures mais qu'on pourrait réinflater, réverser ou geler vaudrait
zéro. La valeur de QUANTA est l'**ensemble des garanties qu'aucun tiers ne peut corrompre**,
chacune re-vérifiée par chaque nœud à chaque bloc : une rareté que personne ne peut diluer, une
propriété absolue (nulle autorité de saisie), une finalité irréversible que même une majorité ne
peut réécrire, une permanence qui traverse le saut post-quantique. Ce qu'on détient n'est pas la
promesse d'une institution, mais **l'absence d'institution capable de la briser**.

Le doute énergétique est tranché au niveau structurel : `emission_for_tick` ne dépend **que** de
`total_miné` — les watts n'entrent nulle part dans la *quantité émise*. Le péché du PoW (« plus
tu brûles, plus le réseau émet ») est déjà absent ; la sécurité vient du stake et de la finalité,
pas du hashrate. Ne subsiste que le résidu de la clé de répartition (§6.5), identifié comme à
corriger.

La doctrine grave enfin une **âme choisie** — permanence par défaut, circulation par volonté :
tes pièces sont à toi indéfiniment, mais tu peux (sur option) en confier au « Courant » qui les
fait couler vers les gardiens actifs. Plusieurs mécanismes audacieux sont gravés comme **caps**
(non livrés, hors chemin de sécurité) : le **Dividende du Commun** (une fraction de chaque tick
répartie en parts égales entre tous les gardiens vivants — anti-baleine par construction), le
**Legs Scellé** (volonté signée ML-DSA : verrou-temps ou dead-man's switch), le **Courant**
(vélocité opt-in à la Gesell/Wörgl), et l'étoile polaire, la **Paie du Sceau** (l'émission d'une
époque mintée seulement quand un certificat ⅔ la finalise — « payé quand l'histoire qu'on a
signée devient irréversible »). Ces mécanismes sont des intentions gravées, pas des livraisons ;
leurs paramètres relèvent d'une ratification du fondateur.

---

## 9. Limites, statut, roadmap honnête

Ce livre blanc ne vaut que s'il distingue le réel du cap.

**Réel aujourd'hui, vérifiable en lecture et en test** — le consensus PoS + finalité Casper-FFG
vivant (LIVE-1→4), la cryptographie post-quantique de l'argent/finalité/transport, le plafond
dur et l'émission décroissante vérifiés au consensus, la conservation et la couverture
symétrique, le multisig PQ, l'écosystème de nœud (daemon + RPC). Le tout couvert par 437 tests +
1 intégration (dont un échange gossip réel à 2 nœuds et l'invariant de déterminisme C1 sur
128 runs), clippy propre.

**Ce qui reste — sans maquillage :**

- **Élection = déterministe, pas VRF.** Le leader est publiquement prévisible ; un vrai VRF
  (imprévisibilité) + un VDF (anti-grinding) sont au roadmap.
- **NodeId Iroh = Ed25519.** Le seul primitif classique restant, dette en amont, hors de notre
  code ; bascule le jour où Iroh livre un EndpointId post-quantique.
- **Résidu énergétique.** `W_ENERGY = 0.30` pèse encore réellement dans le partage entre nœuds ;
  la correction (rétrograder l'énergie en signal anti-sybil, livrer le Dividende du Commun) est
  la direction, pas l'état.
- **Couche distribution hors consensus.** Les mécanismes de la doctrine (Dividende, Legs,
  Courant, Paie du Sceau) sont des caps non livrés ; les graver est de l'ingénierie à faire.
- **Non audité.** Aucune de ces garanties n'a reçu de revue de sécurité externe indépendante.
  La vérification « 2 machines physiques » est réelle mais n'est pas une preuve à l'échelle.
- **Aucun prix.** QUANTA n'a pas de marché ; rien ici ne prétend ni ne prédit qu'un QUANTA
  vaudra quoi que ce soit en échange.

---

## 10. Références internes (code)

Toutes vérifiables dans `src-tauri/src/` :

- Émission & rareté : `p2p/reputation.rs` (`MAX_SUPPLY_MICRO`, `EMISSION_DIVISOR`,
  `emission_for_tick`), `p2p/ledger.rs` (`validate_block_emission`, `mine_tx`,
  `transfer_with_burn`, couverture, plancher de finalité, slash).
- Consensus PoS : `p2p/pos_consensus.rs` (élection, `MIN_VALIDATOR_STAKE`, fallback).
- Gadget de finalité (Casper-FFG) : `sm/finality.rs`, `finality_vote.rs` (quorum ⅔),
  `finality_rule.rs`, `finality_slashing.rs`, `fork_choice.rs` (LMD-GHOST) ; harnais DST `sm/sim.rs`.
- Câblage vivant : `p2p/finality_live.rs` (LIVE-1→3B), `p2p/fork_heal.rs` (LIVE-4).
- Cryptographie : `security/mod.rs`, `security/hybrid_crypto.rs` (ML-DSA-65),
  `security/pq_vault.rs` (Argon2id + AES-256-GCM), `security/cipher.rs`.
- Réseau : `p2p/gossip.rs` (`TORUS_PROTOCOL_VERSION = 6`), `p2p/dispatcher.rs`,
  `p2p/willow_node.rs` (Iroh QUIC), `p2p/gossip_tasks.rs`, `p2p/mining_loop.rs`.
- Répartition : `p2p/shapley.rs` (poids énergie/travail/validation/uptime).
- Identité : `p2p/username.rs`, `security/mod.rs` (adresses `qta1…`), RPC `rpc.rs`.
- Doctrine & marque : `docs/economy/DOCTRINE.md`, `docs/brand/BRAND.md`.

Les concepts cités — Casper-FFG, LMD-GHOST/Gasper, FIPS 204 (ML-DSA), ML-KEM, Bech32m, BIP39 —
sont des standards ou constructions publics réels ; ce document n'invente aucune référence
académique ni URL.

---

*Document vivant. Les affirmations « réel » sont vérifiables dans `src-tauri/src/` à la date de
rédaction (`TORUS_PROTOCOL_VERSION = 6`). Le roadmap décrit des intentions, pas des livraisons.
Statut du projet : alpha, non audité par un tiers. QUANTA n'a aucun marché ni prix.*
