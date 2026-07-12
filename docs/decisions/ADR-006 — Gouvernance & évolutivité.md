---
type: adr
id: ADR-006
status: ratified
decision-class: vision (non-technique — ratification de frontière par Alexandre)
proposed: 2026-06-24
ratification: ADR-009 (2026-06-25)
ratified-by: ADR-009
ratified: 2026-06-25
updated: 2026-07-12
---

# ADR-006 — Gouvernance & évolutivité (noyau immuable par construction, évolution par fork)

← [[README|Registre ADR]] · cadre : [[DESIGN-CONSENSUS-DAG-BFT]]
Lié à : politique d'émission · [[ADR-005 — Agrégation des votes & certificats de finalité]] (abstraction de certificat) · **couche au-dessus du consensus**

> [!info] DÉCISION DE **VISION** (proposée 2026-06-24) — ✅ **ratifiée par ADR-009 (2026-06-25)**
> Quanta n'a **pas** de gouvernance on-chain. Le **noyau monétaire** (plafond 100M + loi
> d'émission, signatures post-quantiques, conservation, sûreté du consensus) est **immuable par
> construction** — non pas verrouillé, mais **sans porte** : aucun chemin de code ne le change.
> La **périphérie** (frais, certains paramètres réseau, et les constantes 🛑 §12 : longueur
> d'époque, enjeu minimum, taille de comité) est **ajustable** — des **réglages, pas des
> promesses**. L'évolution se fait par **fork volontaire + développement ouvert**, jamais par un
> vote pondéré par l'enjeu qui modifierait le protocole en vivant. **Rien à construire
> maintenant** : aucun moteur de gouvernance, **aucun mécanisme dormant**. Cet ADR **ne bloque ni
> n'oriente** la conception du gadget de finalité (la gouvernance est une couche au-dessus du
> consensus).

## Contexte

Quanta vise la **réserve de valeur** face à Bitcoin. La question structurante : **comment le
protocole change-t-il une fois le réseau vivant ?** Deux philosophies opposées :

- **Monnaie ossifiée** (Bitcoin) : règles dures à changer, **aucune gouvernance on-chain**,
  évolution par **fork à adoption volontaire**. Sa crédibilité monétaire **vient** de
  l'immuabilité.
- **Plateforme gouvernée** (Tezos, Cosmos, Polkadot) : vote on-chain pondéré par l'enjeu, les
  mises à jour s'appliquent **automatiquement** en vivant.

Le PoS rend la gouvernance on-chain techniquement **naturelle** (la machinerie de vote du gadget
existe déjà). Mais pour une **monnaie**, la facilité de changement est un **passif** : elle érode
la confiance, mène à la **ploutocratie** (les baleines décident) et ouvre une **surface d'attaque
de gouvernance**. La bonne réponse à « améliorable sans péril » est la **séparation entre un
noyau immuable et une périphérie gouvernable**.

## Décision

1. **Noyau immuable, par construction.** Plafond 100M + loi d'émission, signatures
   post-quantiques, règles de conservation, sûreté du consensus : **gravés**. L'immuabilité
   n'est **pas une serrure solide**, c'est l'**absence de porte** : aucun chemin de code ne permet
   de les changer, donc **aucune gouvernance ne peut les atteindre** — il n'y a **aucun levier**
   dessus.
2. **Périphérie ajustable, derrière des abstractions propres.** Paramètres opérationnels (frais,
   certains paramètres réseau) **et** les constantes marquées 🛑 §12 (longueur d'époque, enjeu
   minimum, taille de comité). Ce sont des **réglages, pas des promesses**.
3. **Évolution par fork volontaire + développement ouvert** (le **vrai** modèle de Bitcoin),
   **pas** de gouvernance on-chain. Un changement est proposé, codé, et **chaque nœud choisit**
   de l'adopter. **Zéro surface d'attaque de gouvernance** : la gouvernance vit **hors** du code,
   dans le consensus social et le choix de chacun.
4. **Aucun mécanisme de gouvernance dormant dans le code.** La préparation du futur vient de la
   **structure** (abstractions propres — ex. l'abstraction de certificat d'[[ADR-005 — Agrégation des votes & certificats de finalité]]),
   **pas** d'un interrupteur caché. Du code de gouvernance dormant = une **surface d'attaque
   dormante** (leçon du **CRDT fantôme** : un module non utilisé reste un vecteur).

## Le principe directeur

On ne **protège** pas un invariant critique en le rendant difficile à modifier. On le rend **non
modifiable par construction**, en ne lui donnant **aucun chemin de code** de modification. C'est
plus sûr qu'une serrure : une serrure se crochète, une porte absente non.

## Vérification — l'immuabilité-par-construction tient **aujourd'hui** (audit lecture seule)

> *Constitution §2 : « aucune assertion de sécurité sans vérification ». L'ADR affirme deux choses
> falsifiables sur le code (§1 noyau sans levier, §4 aucun mécanisme dormant). Audit du
> 2026-06-24 — confirmé.*

**Noyau monétaire = `const`, donc sans porte au sens du langage.**

- `MAX_SUPPLY_MICRO: u64 = 100_000_000 * MICRO` — `pub const` (`p2p/reputation.rs:71`).
- `EMISSION_DIVISOR: u64 = 50_000_000` — `pub const` (`p2p/reputation.rs:79`).
- `emission_for_tick(total_mined) = (MAX_SUPPLY − total_mined) / EMISSION_DIVISOR` — **fonction
  pure**, strictement bornée par le plafond (`p2p/reputation.rs:85`).
- Un `const` Rust est **substitué à la compilation** : il n'a **aucun emplacement mémoire**
  d'exécution. Il n'existe donc **aucun setter exprimable** — la « porte » n'est pas verrouillée,
  elle est **absente**. Le seul moyen de changer ces valeurs est d'**éditer la source et
  recompiler** — c'est-à-dire **forker** (décision §3).
- **Aucun setter trouvé** : toutes les autres occurrences de `MAX_SUPPLY_MICRO` sont des
  **lectures**, des comparaisons `<=`, ou l'**enforcement** au consensus.
- *Note (2026-07-12) : les ancrages de ligne ci-dessus (`reputation.rs:71/79/85`, `ledger.rs:1315/1330`)
  ont dérivé après GADGET-1→5B et PQ-MIG-3B (renumérotation de fichier) — les `const`/fonctions
  citées existent toujours, valeurs inchangées ; ancrages non rafraîchis.*

**Le plafond est appliqué au consensus, sur les deux chemins.**

- `validate_block_emission_against(block, prior_mined)` (`p2p/ledger.rs:1315`) — validateur
  **partagé** FORK-CAP-1 ; rejette tout bloc poussant l'offre au-delà du plafond
  (`p2p/ledger.rs:1330`) **et** une borne par bloc. Happy path **et** fork-reorg passent par la
  **même** source de vérité ⇒ un pair malveillant ne peut **ni dépasser 100M ni rafler
  l'émission d'un coup**.

**Aucun mécanisme de gouvernance, dormant ou actif (la leçon du CRDT fantôme honorée).**

- Recherche `governance` / `gouvernance` / `referendum` / `ballot` / `proposal` / `param_change` /
  `protocol_upgrade` sur tout `src-tauri/src/` ⇒ **zéro** code de gouvernance. (Le seul `proposal`
  est `block_proposal_is_byte_deterministic` — un **bloc** de consensus, pas une proposition de
  gouvernance.)
- Les **deux seules** occurrences de `vote` sont des **commentaires** : `p2p/consensus.rs:4`
  (« convergence déterministe entre nœuds **sans leader ni votes** ») et `p2p/gossip.rs:100`
  (description du `ReportPeer` anti-Sybil). **Aucun levier.**

**Vote de consensus ≠ vote de gouvernance (distinction préservée).** Le gadget de finalité
(`sm/finality.rs`, GADGET-1) fait — à terme — voter un comité sur **quel bloc est final**, jamais
sur **quelles sont les règles**. C'est exactement la frontière de l'ADR : le vote de finalité est
un mécanisme de **consensus** (légitime, ADR-005), pas de **gouvernance** (rejeté ici). *Rien ne
finalise encore* (la règle justify/finalize est GADGET-3) — donc **aucune** machinerie de vote
n'est même active aujourd'hui. *(État au 2026-06-24 ; GADGET-3→5B livrés depuis, gossip des votes
câblé en vivant par LIVE-1 — la règle finalise désormais dans `sm/`.)*

**Nuance honnête (pas de survente).** Aujourd'hui, le noyau **et** la périphérie sont tous deux de
simples `const` : tous deux ne changent **que par fork (recompilation)** — il n'existe **pas
encore** de mécanisme d'exécution à **deux niveaux**. C'est **conforme** à l'ADR (§2 « abstractions
propres » est une *structure future*, pas un état actuel ; « Rien à construire maintenant ») et
c'est précisément ce que §4 exige : **l'abstraction d'ajustement n'est pas un interrupteur dormant**.
La frontière gravé / ajustable ci-dessous est donc aujourd'hui une frontière de **promesse et
d'intention** (ce que Quanta s'engage à ne jamais toucher vs ce qu'il se réserve de **calibrer par
fork**), **pas** un clivage de mécanisme déjà câblé.

## Frontière — gravé vs ajustable (✅ ratifiée par ADR-009, 2026-06-25)

> *Point de départ pour la ratification — l'esquisse §1/§2 de l'ADR rendue précise et adossée au
> code. Constitution §4 : je **cadre**, Alexandre **tranche** la frontière exacte. Frontière
> ratifiée par ADR-009 (2026-06-25).*

| Invariant / constante | Tier | Mécanisme aujourd'hui | Localisation |
|---|---|---|---|
| Plafond 100M `MAX_SUPPLY_MICRO` | **GRAVÉ** (promesse) | `const` + enforcement consensus | `reputation.rs:71`, `ledger.rs:1330` |
| Loi d'émission (`EMISSION_DIVISOR`, `emission_for_tick`) | **GRAVÉ** (promesse) | `const` + fonction pure | `reputation.rs:79`, `:85` |
| Conservation (`Σ dépensable+staké+déverr.+brûlé == miné`) | **GRAVÉ** (promesse) | invariant testé (proptest + sim) | `ledger.rs`, `sm/sim.rs` |
| Signatures post-quantiques (Ed25519 + ML-DSA-65) | **GRAVÉ** (promesse) | schéma figé | `security/hybrid_crypto.rs` |
| Sûreté du consensus (couverture, FORK-CAP, finalité) | **GRAVÉ** (promesse) | validateurs partagés | `ledger.rs`, `sm/finality.rs` |
| Longueur d'époque `EPOCH_LENGTH_BLOCKS` | **AJUSTABLE** (calibrage) | `const` (fork) — 🛑 §12 | `finality.rs:36` |
| Enjeu minimum `MIN_VALIDATOR_STAKE` | **AJUSTABLE** (calibrage) | `const` (fork) — 🛑 §12 | `pos_consensus.rs:64` |
| Taille de comité / quorum | **AJUSTABLE** (calibrage) | à fixer — 🛑 §12 / ADR-005 | (gadget) |
| Période de débond `UNBONDING_PERIOD_BLOCKS` | **AJUSTABLE** (calibrage) | `const` (fork) — ≥ fenêtre slashing (ADR-003) | `ledger.rs:64` |
| Frais / certains paramètres réseau | **AJUSTABLE** (réglage) | `const` (fork) | divers |

> **Ligne de partage proposée** : est **gravé** ce qui est une **promesse de valeur** (rareté,
> conservation, post-quantique, sûreté) ; est **ajustable** ce qui est un **paramètre de calibrage
> opérationnel** dont on attend qu'il soit réglé (par fork) avant ou après lancement. Alexandre
> ratifie la frontière **exacte**.

## Conséquences

- La monnaie est **digne de confiance** sur ce qui compte (noyau monétaire immuable, **prouvé sans
  levier de code**).
- Le projet reste **vivant et améliorable** (dev ouvert + fork volontaire) sur le reste.
- **Rien à construire maintenant** : pas de moteur de gouvernance ; en alpha le fondateur change le
  code lui-même.
- Les constantes 🛑 §12 sont explicitement des **paramètres ajustables, pas des promesses**.
- **N'affecte pas le gadget** : la gouvernance est une couche au-dessus du consensus ; la
  conception du gadget se poursuit indépendamment.

## Deux sens de « améliorable par la communauté » (ne pas confondre)

- **Développement ouvert** (la communauté propose, code, contribue ; adoption par fork volontaire) :
  **sain, sans danger, déjà en place** sur GitHub. **Conservé.**
- **Gouvernance on-chain** (vote pondéré par l'enjeu modifiant le protocole automatiquement, en
  vivant) : le sens **risqué**, **rejeté** pour une monnaie.

## Alternatives considérées

- **Gouvernance on-chain pondérée par l'enjeu** (Tezos/Cosmos/Polkadot) : *rejetée* pour une
  monnaie (passif, ploutocratie, surface d'attaque). Convient aux **plateformes**, pas à la monnaie
  saine.
- **Ossification totale sans apport communautaire** : *non retenue* ; le développement ouvert et les
  propositions communautaires sont **gardés**, seule la gouvernance **automatique** est écartée.

## Statut & ce dont j'ai besoin de toi (🛑)

✅ **ADR ratifié** (par ADR-009, 2026-06-25) — décision de **vision**, non technique.

- La **frontière exacte** gravé vs ajustable est **ratifiée** (la table ci-dessus, précisée par
  ADR-009).
- La **liste précise** des invariants « gravés » — confirmer qu'aucun n'est oublié (ex. : le
  format de tx, le burn 1 %, l'unité µQTA méritent-ils le statut gravé ?).

## Questions ouvertes

- Faut-il un jour matérialiser la périphérie « ajustable » derrière une **vraie abstraction de
  paramètres** (versionnée, fork-only), ou rester sur des `const` simples tant que tout changement
  est de toute façon un fork ? *(Pas urgent ; surveiller, ne pas résoudre maintenant.)*
- Le **burn 1 %**, le **format de transaction**, l'**unité µQTA** : gravés ou ajustables ?

> Couche au-dessus du consensus : cet ADR **ne bloque ni n'oriente** la conception du gadget. La
> validation de la conception du gadget se poursuit indépendamment — c'est, avec les décisions §12
> (E, taille de comité, quorum), ce qui **ne se délègue pas**.
