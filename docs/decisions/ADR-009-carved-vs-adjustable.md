---
type: adr
id: ADR-009
status: accepted
decision-class: frontière (ratification d'ADR-006) + valeurs §12 — défauts réglables
ratifies: ADR-006
proposed: 2026-06-25
accepted: 2026-06-25
updated: 2026-06-25
---

# ADR-009 — Frontière gravé/ajustable (ADR-006 ratifiée) et valeurs du §12

← [Registre ADR](README.md) · ratifie : [ADR-006 — Gouvernance & évolutivité](ADR-006-governance.md)
Lié à : [ADR-002 — Validator set & comité BFT](ADR-002-validator-set.md) · [ADR-003 — Slashing (accountable safety)](ADR-003-slashing.md) ·
[ADR-005 — Agrégation des votes & certificats de finalité](ADR-005-vote-aggregation.md) · politique d'émission · PQ-MIG-5 (genèse PQ)

> [!TIP] DÉCISION **ACCEPTÉE** (défauts tranchés, réglables — 2026-06-25)
> [ADR-006 — Gouvernance & évolutivité](ADR-006-governance.md) posait le **principe** (noyau monétaire immuable **par
> construction**, périphérie ajustable derrière abstractions) mais laissait la **frontière exacte**
> et les **valeurs** ouvertes. Cet ADR les **tranche**, avec des défauts ancrés dans la littérature
> et l'**état réel du code**. Les valeurs **monétaires** existantes ne sont **pas redéfinies** ici
> (ce sont des choix du mainteneur) ; elles sont **ratifiées comme gravées**. Le reste est fixé à des
> défauts **réglables** (réglages, pas promesses ; évolution par **fork volontaire**, jamais par
> gouvernance on-chain). **Rien à construire** : aucun code, aucune valeur changée — un **document
> de ratification**.

## Contexte

ADR-006 (proposée 2026-06-24) demandait au mainteneur trois ratifications restées ouvertes : la
**frontière exacte** gravé/ajustable, la **liste précise** des invariants gravés (le burn 1 %, l'unité
µQTA méritent-ils ce statut ?), et les **valeurs §12** (longueur d'époque, quorum, enjeu minimum).
ADR-009 répond aux trois. La règle d'arrêt du projet s'applique : une décision de cette classe est **cadrée**
(table + ancrage + conséquences) puis **tranchée** par le mainteneur — ici les défauts sont posés **réglables**, et les
valeurs purement **économiques** (échelle monétaire) restent explicitement au mainteneur (§3).

## 1. La frontière gravé/ajustable (ratification d'ADR-006)

**GRAVÉ (immuable par construction, aucun setter) — deux familles :**

- **Monétaire** — *définit la monnaie, ne doit jamais bouger* :
  - `MAX_SUPPLY_MICRO` = `100_000_000 * MICRO` (plafond dur 100 M) — `p2p/reputation.rs`.
  - Calendrier d'émission : `EMISSION_DIVISOR = 50_000_000` + courbe `emission_for_tick =
    (MAX − total_mined) / EMISSION_DIVISOR` — `p2p/reputation.rs`.
  - Taux de **burn** = **1 %** par transfert (`amount / 100`, entier) — `p2p/ledger.rs`.
  - Unité **µQTA** = `MICRO = 1_000_000` — `p2p/ledger_types.rs`.
  - **Zéro premine** — pilier de mission, **confirmé en PQ-MIG-5** : `Ledger::new()` ==
    `genesis_with_allocation(&[])`, **allocation de genèse vide par défaut** (`p2p/ledger.rs`).

- **Invariants de sûreté** — *abaisser l'un casse la sécurité* :
  - **Quorum ⅔** : `backing × 3 ≥ total × 2` (`QUORUM_NUM=2 / QUORUM_DEN=3`,
    `meets_supermajority`) — `sm/finality_vote.rs`.
  - Le **slashing brûle** (ne redistribue pas) : `SLASH_BURN = true` — `sm/finality_slashing.rs`.
  - Contrainte **fenêtre de preuve ≤ unbonding** : `SLASH_EVIDENCE_WINDOW_BLOCKS ≤
    UNBONDING_PERIOD_BLOCKS`, garantie par une **assertion `const`** (erreur de compilation si
    violée) — `sm/finality_slashing.rs`.
  - **Sûreté responsable** : recouvrement ⅓ par principe du pigeonnier (deux finalisés en conflit ⇒
    ⅓ de l'enjeu slashable) — GADGET-4.

**AJUSTABLE (réglage opérationnel, derrière abstractions, valeurs §12) :**

Durées et tailles qui **règlent** performance/économie **sans toucher la monnaie ni la sûreté** :
longueur d'époque, durée d'unbonding, enjeu minimum de validateur, fraction de slash (défaut plein).
Modifiables par **fork volontaire + développement ouvert** (modèle Bitcoin, ADR-006) — **pas** de
gouvernance on-chain, **pas** de code dormant.

## 2. Les valeurs du §12 (tranchées, réglables)

| Constante | Valeur | Ancrage (code) | Classe |
|---|---|---|---|
| `EPOCH_LENGTH_BLOCKS` (E) | **32** | Gasper/Ethereum (époque = 32 slots) ; `sm/finality.rs` (GADGET-1, paramétrique `∀ E ≥ 1`) | ajustable |
| Quorum de finalité | **⅔** (entier, `backing×3 ≥ total×2`) | seuil BFT ; `sm/finality_vote.rs` (ADR-005/GADGET-2) | **gravé** |
| `UNBONDING_PERIOD_BLOCKS` | **10 080** (≈ 2 sem.) | `p2p/ledger.rs` (ONCHAIN-STAKE-1) ; ≥ fenêtre de slashing | ajustable (durée) |
| `SLASH_EVIDENCE_WINDOW_BLOCKS` | **= UNBONDING** | `sm/finality_slashing.rs` ; `const`-assert ≤ unbonding (GADGET-4) | **contrainte gravée** |
| `SLASH_NUM / SLASH_DEN` | **1 / 1** (plein) | dissuasion maximale ; `sm/finality_slashing.rs` (GADGET-4) | ajustable (fraction) |
| `SLASH_BURN` | **true** (brûlé) | sain monétairement ; `sm/finality_slashing.rs` (GADGET-4) | **gravé** (brûle vs redistribue) |
| `MIN_VALIDATOR_STAKE` | **1 QTA = 1_000_000 µQTA — placeholder nominal 🛑** | anti-sybil ; `p2p/pos_consensus.rs` ; **échelle économique : décision du mainteneur** (§3) | ajustable |
| Allocation de genèse | **vide** (zéro premine) | pilier mission ; PQ-MIG-5 (`p2p/ledger.rs`) | **gravé** (principe) ; valeur réelle : décision du mainteneur |

> **Finalité = ⅔ de l'enjeu total actif**, pas un comité échantillonné : il n'y a **pas** de
> paramètre « taille de comité » dans le **chemin de finalité** (`backing_weight` somme l'enjeu
> on-chain des votants distincts valides ; le seuil porte sur l'enjeu **total** actif). Si l'élection
> de leader par beacon (ADR-004) échantillonne, c'est un paramètre **séparé**, hors §12 finalité.

## 3. Ce qui reste honnêtement au mainteneur (valeurs économiques, pas structure)

- **`MIN_VALIDATOR_STAKE`** : posé comme **placeholder nominal** (1 QTA) parce que sa valeur sensée
  dépend de l'**échelle monétaire** (offre totale, valeur du µQTA) — le **noyau gravé**, qu'on ne
  redéfinit pas. À fixer quand l'échelle est fixée. La **frontière** (constante gravée *sans setter*,
  ajustable par fork) est, elle, **tranchée**.
- **Distribution / émission réelles** : ratifiées **gravées à leurs valeurs actuelles en code**.
  Changer ces nombres reste une décision **de politique monétaire** (mainteneur) ; mais le **statut**
  (constantes gravées sans setter) est tranché.

## Conséquences

- Les constantes déjà en code (E, unbonding, slash, burn, quorum, plafond, divisor, µQTA) sont
  **ratifiées, pas changées** — ADR-009 est un acte de ratification, **zéro modification de valeur**.
- **`MIN_VALIDATOR_STAKE` existe** (1 QTA, `pos_consensus.rs`) — il n'est ni absent ni à 0, donc le
  petit spec « placeholder » évoqué en §Conséquences du brouillon **n'est pas nécessaire** : le
  placeholder marqué est déjà là.
- [ADR-006 — Gouvernance & évolutivité](ADR-006-governance.md) devient **opérationnel** : la frontière est **nommée**, pas
  seulement intentionnelle. ADR-006 passe de 🟡 *proposée* à ✅ *ratifiée (par ADR-009)*.
- **Pas de gouvernance on-chain, pas de code dormant** (ADR-006 tenu).
- Aucun blocage pour le câblage du gadget : tous les paramètres du chemin de finalité ont un défaut
  ratifié.

## Ouvert (réglages, pas blocages)

- L'**échelle monétaire** (offre, valeur du µQTA) et donc **`MIN_VALIDATOR_STAKE`** : décision
  économique du mainteneur, quand il le souhaite. **Aucune ne bloque** le câblage du gadget.
- Matérialiser un jour la périphérie « ajustable » derrière une **vraie abstraction de paramètres**
  versionnée (fork-only) vs rester sur des `const` simples : à surveiller, **pas à résoudre
  maintenant** (tout changement étant de toute façon un fork).

## Alternatives écartées

- **Gouvernance on-chain pondérée par l'enjeu** (Tezos/Cosmos/Polkadot) : *rejetée* pour une monnaie
  (ploutocratie, surface d'attaque) — déjà tranché en ADR-006, confirmé ici.
- **Tout graver, y compris E / unbonding / slash** : *non retenu* — fige des réglages opérationnels
  qui n'affectent ni la monnaie ni la sûreté, au prix d'un fork pour le moindre ajustement de cadence.
- **Tout laisser ouvert (§12 indéfini)** : *non retenu* — laissait le gadget sans défauts câblables ;
  ADR-009 pose des défauts ancrés et réglables.
