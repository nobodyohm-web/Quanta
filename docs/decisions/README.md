---
type: adr-index
updated: 2026-08-14
---

# Décisions d'architecture (ADR)

Registre des arbitrages de **consensus et de sécurité**. Une ADR est écrite quand une
décision engage le protocole au-delà du fichier qu'elle touche : elle nomme l'option
retenue, **l'option écartée**, et ce que la décision coûte.

Deux règles gouvernent ce registre :

- **Une ADR renversée reste au registre**, marquée comme telle, avec ce qui l'a renversée.
  ADR-008 en est l'exemple : elle est marquée REVERSÉE, et c'est plus instructif que si
  elle avait été effacée.
- **Une décision non tranchée est dite ouverte**, pas silencieuse. ADR-004 (aléa
  d'élection) est ouverte depuis juin 2026 parce qu'aucun VRF post-quantique déployable
  n'existe ; le dire est plus utile que de laisser croire au contraire.

Les classes indiquent la réversibilité : un **défaut réversible** peut être changé par une
mise à jour, un **🛑 hard-stop** engage la genèse ou l'identité et ne se change qu'au prix
d'un fork.

## Le cadre
Trajectoire de [DESIGN-CONSENSUS-DAG-BFT](../protocol/CONSENSUS-DAG-BFT.md) : **durcir le PoS linéaire → finality
gadget (Option 1) → DAG-BFT (Option 2)**, le **harnais multi-nœuds**
([QUANTA_T0_DST_HARNESS](../archive/specs/QUANTA_T0_DST_HARNESS.md)) étant le prérequis (en cours). État des 4 méta-décisions §7 :
- ✅ **Périmètre** (2026-06-21) : **Option 1 — finality gadget d'abord** (garder la chaîne
  linéaire + leader PoS, **ajouter** un vote BFT qui finalise en ~secondes).
  DAG-BFT (Option 2) reste en Phase 2, derrière bump de protocole.
  → **Protocole détaillé** : [DESIGN-FINALITY-GADGET](../protocol/FINALITY-GADGET.md) (style **Casper FFG**, votes **ML-DSA**,
  par époque — synthétise ADR-001→005 ; *synthèse des ADR-001 à 005*).
- ✅ **Prérequis** : harnais + chaos **d'abord** (= Phase 0 actuelle, T0.1 fait).
- ✅ **Signatures** (2026-06-23) : **post-quantique pur (ML-DSA), finalisation par époque** — pas de BLS,
  un seul système crypto, derrière une abstraction de certificat. **Tranchée par
  [ADR-005 — Agrégation des votes & certificats de finalité](ADR-005-vote-aggregation.md)** ; **comité/quorum/longueur d'époque
  fixés** par [ADR-009](ADR-009-carved-vs-adjustable.md)
  (E=32, quorum ⅔, pas de « taille de comité » dans le chemin de finalité).
- ✅ **Compat** (2026-06-25) : le gadget ajoute des messages de vote → **bump de protocole** —
  `TORUS_PROTOCOL_VERSION` **2 → 3** posé à la **genèse PQ** (PQ-MIG-5). *(Le gossip de votes lui-même
  reste hors-périmètre jusqu'au câblage vivant — réconciliation clé-de-vote ↔ clé-d'enjeu.)*

## Les ADR
| ADR | Sujet | Classe | Statut |
|---|---|---|---|
| [ADR-001 — Fork-choice](ADR-001-fork-choice.md) | Sélection de branche (fenêtre non-finalisée → finalité) | défaut réversible | **résolu par le gadget** ; sous-choix intérim |
| [ADR-002 — Validator set & comité BFT](ADR-002-validator-set.md) | Comité = stake on-chain par epoch | 🛑 | ✅ **ACCEPTÉE** — stake on-chain seul |
| [ADR-003 — Slashing (accountable safety)](ADR-003-slashing.md) | Équivocation prouvable + pénalité | 🛑 | OUVERTE (in-scope Phase 1) |
| [ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)](ADR-004-election-randomness.md) | Imprévisibilité + anti-grinding | 🛑 | OUVERTE (beacon OK pour P1 ?) |
| [ADR-005 — Agrégation des votes & certificats de finalité](ADR-005-vote-aggregation.md) | Format des votes du gadget (PQ pur ML-DSA, par époque) | 🛑 | ✅ **ACCEPTÉE** — PQ pur par époque ; **comité/quorum/époque fixés par [ADR-009](ADR-009-carved-vs-adjustable.md)** |
| [ADR-006 — Gouvernance & évolutivité](ADR-006-governance.md) | Rapport au changement : noyau immuable par construction, évolution par fork (pas de gouvernance on-chain) | vision (non-technique) | ✅ **RATIFIÉE** (par [ADR-009](ADR-009-carved-vs-adjustable.md), 2026-06-25) — frontière gravé/ajustable **nommée** ; immuabilité-par-construction vérifiée (audit 2026-06-24) |
| [ADR-007 — Portée du post-quantique (comptes ML-DSA)](ADR-007-post-quantum-scope.md) | Quanta réellement entièrement PQ : comptes en ML-DSA (b) vs registre de finalité (a) | vision fondatrice 🛑 | 🟢 **RÉALISÉE (b)** — comptes **entièrement ML-DSA** (PQ-MIG-3B, 2026-06-25) ; **sans astérisque** ; **GADGET-3 débloqué** |
| [ADR-008 — Autorité de tx via liaison ML-DSA on-chain (PQ-MIG-3)](ADR-008-tx-authority-binding.md) | PQ-MIG-3 : autorité de tx PQ — d'abord (a) « liaison on-chain, `from` Ed25519 », puis **reversé** en (b) | portée d'implémentation | 🔴 **REVERSÉ** (2026-06-25, PQ-MIG-3B) — l'astérisque (a) **levé** : `from`/`to` = **adresse ML-DSA** partout, autorité **pur ML-DSA + `lie`** ; rétablit ADR-007 (b) |
| [ADR-009 — Frontière gravé-ajustable (ADR-006 ratifiée) et valeurs du §12](ADR-009-carved-vs-adjustable.md) | Ratifie ADR-006 : frontière gravé/ajustable + valeurs §12 (E, unbonding, slash, quorum, enjeu min) | ratification + défauts réglables | ✅ **ACCEPTÉE** (2026-06-25) — défauts ancrés au code, **ratifiés sans rien changer** ; échelle monétaire et `MIN_VALIDATOR_STAKE` restent des choix ouverts |

> **Note** — ADR-006 et ADR-007 sont des décisions de **vision** (ADR-006 au-dessus du consensus ;
> ADR-007 sous le consensus, à la **racine cryptographique**). ADR-006 n'oriente pas le gadget ;
> ADR-007 **bloquait** GADGET-3 (qui a besoin de l'identité autoritaire tranchée) — **débloqué**
> depuis que (b) est **réalisé** (PQ-MIG-3B, 2026-06-25) : l'identité d'enjeu = l'identité de
> finalité = l'**adresse ML-DSA**. ADR-007 est née de l'audit **CRYPTO-ID-1**, qui a prouvé que
> « entièrement post-quantique » n'était **pas tenu** (comptes enracinés Ed25519, clé ML-DSA non
> liée) ; (b) **réalisé** ferme la faille **intrinsèquement** (`from == BLAKE3(ADDR_DOMAIN ‖ clé)`)
> et rend l'invariant « signatures PQ » du noyau gravé d'ADR-006 **honnête** au niveau du compte.

## Convention
Une ADR = une décision. Cycle de vie : **OUVERTE** → (tu tranches) → **ACCEPTÉE**
(date + choix + conséquences gravées) ou **REJETÉE**. On ne réécrit pas une ADR
acceptée ; on en crée une nouvelle qui la **supersède** (lien `supersedes`).
