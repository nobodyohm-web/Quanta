---
type: adr
id: ADR-005
status: accepted
decision-class: 🛑 hard-stop
decided: 2026-06-23
ratification: paramètres fixés par ADR-009 (2026-06-25)
supersedes-proposal: hybride BLS + ancrage PQ (proposition initiale du 2026-06-22, rejetée)
updated: 2026-07-12
---

# ADR-005 — Agrégation des votes & certificats de finalité (post-quantique pur, par époque)

← [Registre ADR](README.md) · cadre : [DESIGN-CONSENSUS-DAG-BFT](../protocol/CONSENSUS-DAG-BFT.md) (méta-décision §7 — Signatures)
Lié à : [ADR-001 — Fork-choice](ADR-001-fork-choice.md) · [ADR-002 — Validator set & comité BFT](ADR-002-validator-set.md) · [ADR-003 — Slashing (accountable safety)](ADR-003-slashing.md) · [ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)](ADR-004-election-randomness.md)

> [!TIP] DÉCISION (2026-06-23) — **Post-quantique pur, finalisation par époque**
> Le certificat de finalité d'une époque = l'ensemble des votes **ML-DSA** du comité atteignant
> le quorum. **Pas de BLS, pas d'ancrage, un seul système crypto**, le tout derrière une
> **abstraction de certificat** qui garde l'agrégation future (BLS/SNARK) comme remplacement
> local. **Supersède** la proposition hybride initiale (BLS + ancrage PQ), désormais **rejetée**.
> *Sous-paramètres ci-dessous **fixés par ADR-009** (2026-06-25).*
> Décision de conception, **pas une tâche d'implémentation** : le gadget se construira en specs
> ultérieurs, en ML-DSA dès l'étape 1.

## Contexte

Le gadget de finalité fait voter un comité de validateurs sur les blocs ; un quorum finalise.
Le format de ces votes **paraissait** poser une tension irréductible :

- **BLS** agrège N votes en **une** signature de taille constante (certificats compacts), mais
  repose sur des couplages de courbes elliptiques → **pas post-quantique**.
- **ML-DSA (post-quantique)** tient la promesse PQ partout, mais **ne s'agrège pas** : N votes =
  N signatures d'environ 3,3 Ko.

**Cette tension se dissout** sous deux observations :

1. **Un vote de finalité est éphémère ; une transaction est éternelle.** Le post-quantique est
   urgent là où une signature doit rester infalsifiable **des années** — les **transactions**,
   déjà signées PQ (Ed25519 + ML-DSA-65). Un vote de finalité ne compte que dans la
   fenêtre où la finalité se décide : pas de « récolter aujourd'hui, forger demain » sur une
   décision jetable. *(Cela n'amoindrit pas l'intérêt du PQ sur les votes — ça retire seulement
   l'argument « le trou BLS serait acceptable ».)*
2. **Le poids des certificats dépend de la granularité de finalisation.** Le coût « N × 3,3 Ko »
   n'est vrai qu'en finalisant **bloc par bloc**. En finalisant **par époque**, on produit **un
   certificat par lot de blocs**, pas par bloc : ≈ 50 validateurs ⇒ ≈ **165 Ko par époque**,
   amortis sur des dizaines de blocs et **élagables**. Gérable.

Conjuguées, ces deux observations rendent le **PQ pur viable et préférable** pour une chaîne au
comité modeste, **sans rien céder de la promesse**.

## Décision

**Agrégation post-quantique pure (ML-DSA), finalisation par époque.** Le certificat de finalité
d'une époque est l'ensemble des votes ML-DSA du comité atteignant le quorum. **Pas de BLS, pas
d'ancrage, un seul système cryptographique.**

Le tout derrière une **abstraction de certificat** propre, qui **isole le schéma d'agrégation**
du reste du gadget : une agrégation future (BLS, SNARK PQ) serait un **remplacement local**, pas
une réécriture.

## Modèle de sécurité

- **Finalité entièrement post-quantique**, certificats compris. **Aucune primitive classique**
  sur le chemin de l'irréversibilité.
- **Aucune fenêtre** de vulnérabilité quantique à expliquer (contrairement à l'hybride
  initialement envisagé, dont la finalité récente était classique).
- **Promesse whitepaper** : « **finalité post-quantique pure**, **sans astérisque** ». Les
  transactions **et** la finalité sont PQ de bout en bout.

## Pourquoi ce choix (et pas l'hybride)

- **Distinction produit** : « la seule L1 entièrement post-quantique, **finalité comprise** » est
  l'accroche la plus nette. L'hybride **retirait** cet argument en imposant un astérisque BLS. Le
  BLS est une **optimisation d'ingénierie invisible au public**, pas un argument de valeur.
- **Simplicité** : un seul système crypto, pas d'ancrage ; l'**attribution de faute** (slashing,
  [ADR-003 — Slashing (accountable safety)](ADR-003-slashing.md)) est **plus simple sur des signatures séparées** que
  sur un agrégat. Le build **le moins risqué** tant qu'il n'y a pas d'audit externe.
- **Le seul vrai inconvénient du PQ pur** — la **taille** des signatures (le coût *calcul* étant
  négligeable) — est **neutralisé par la finalisation par époque** tant que le comité reste
  modeste.

## Alternatives considérées

- **Hybride BLS + ancrage PQ** : *rejeté* (était la proposition initiale). À la fois **moins
  distinctif** (astérisque BLS sur la promesse) **et plus complexe** (deux systèmes crypto,
  ancrage, slashing plus délicat). Malin en apparence, moins bon pour ce projet.
- **BLS pur** : *rejeté*. Troue le post-quantique sur l'irréversibilité → contredit la
  proposition de valeur.
- **Agrégat PQ compressé par SNARK** : *différé*. Idéal à très grande échelle (PQ partout **et**
  certificats compacts), mais exige un SNARK lui-même PQ-sûr (STARK / hash-based) et un coût de
  preuve élevé. Évolution possible, **pas un point de départ**.

## Évolution future (différée, non bloquante)

Le BLS (ou un SNARK PQ) ne devient pertinent qu'à **très grand comité** (centaines à milliers de
validateurs), ce qu'une chaîne jeune n'a pas. L'**abstraction de certificat** garde ce chemin
ouvert : si l'échelle l'impose un jour, l'agrégation se **substitue localement**, sans toucher au
reste du gadget. Tant que le comité est modeste, **c'est inutile**.

## Conséquences

- **Débloque** la conception protocolaire du gadget — **sans dépendance crypto résiduelle à
  trancher** : on construit en **ML-DSA dès l'étape 1**.
- **Promesse whitepaper renforcée et simplifiée** (PQ pur, sans nuance).
- **Slashing** ([ADR-003 — Slashing (accountable safety)](ADR-003-slashing.md)) : attribution de faute **directe**
  sur des votes séparés, plus simple que sous agrégat.
- Impose la **finalisation par époque** comme **propriété structurante** du gadget (pas un détail
  d'implémentation).

## Statut & ce dont j'ai besoin de toi (🛑)

ADR **accepté** (post-quantique pur, par époque) ; sous-paramètres **fixés par ADR-009**
(2026-06-25) :

- **Comité / quorum** : **fixés par ADR-009** — quorum **⅔ gravé** (`QUORUM_NUM = 2` /
  `QUORUM_DEN = 3`, `src-tauri/src/sm/finality_vote.rs:56-57`, appliqué par
  `meets_supermajority` `:236`) ; **pas de comité échantillonné**.
- **Longueur d'époque** : **fixée par ADR-009** — `EPOCH_LENGTH_BLOCKS = 32`
  (`src-tauri/src/sm/finality.rs:35`).
- **Schéma PQ** : ML-DSA, cohérent avec la signature de tx (niveau **65**).
- **Format du certificat d'époque** et **stratégie d'élagage** des certificats anciens :
  inchangé, non couvert par ADR-009.

## Questions ouvertes

- Taille de comité, quorum, longueur d'époque, format et élagage des certificats (ci-dessus).
- Interaction avec l'**aléa d'élection** ([ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)](ADR-004-election-randomness.md))
  pour la **rotation du comité**.
- **Seuil concret de comité** au-delà duquel l'agrégation redeviendrait nécessaire (à
  **surveiller**, pas à résoudre maintenant).

> Prochaine étape, **une fois ratifié** : la **conception protocolaire du gadget** (machine à
> états des votes, époques, certificats, quorum, articulation slashing & beacon), à réfléchir à
> la main **avant** tout découpage en specs. C'est là que commence la vraie montagne.
