---
type: task-spec
id: PQ-MIG-5
status: à exécuter (dernière pièce du chantier PQ ; clôt la migration crypto)
priorité: 🟠 genèse post-quantique — bloc de genèse déterministe sur identités ML-DSA
classe: genèse PQ (état initial + validateurs initiaux comme mapping déterministe) + bump version ; pas de câblage vivant
origine: [[QUANTA_PQ_MIGRATION_DESIGN]] §4.8, §8 · construit sur PQ-MIG-1/2/3B
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[QUANTA_PQ_MIG_3B]] · EMIT-1 / conservation · [[AUDIT_QUANTA_2_PROGRESS]]
---

# PQ-MIG-5 : genèse post-quantique (clôt le chantier crypto)

> Dernière pièce de la migration. Le bloc de **genèse** est reconstruit sur les identités **ML-DSA**
> (PQ-MIG-1) et adresses **BLAKE3** (PQ-MIG-2) : il fixe l'**état initial** (allocation) et
> l'**ensemble de validateurs initial** comme un **mapping déterministe** adresse→(solde, enjeu),
> plus les règles de consensus et un **bump `TORUS_PROTOCOL_VERSION`**. Pré-genèse, donc on
> **remplace** la genèse, rien à migrer en vol. Pas de câblage gossip/dispatcher. Diff logique
> seule, déterministe, **C1 vert**, conservation verte dès le bloc 0.

## 1. Genèse déterministe sur identités ML-DSA
- Le bloc de genèse encode : règles de consensus (déjà en place), **allocation initiale** =
  mapping **adresse ML-DSA → solde**, **ensemble de validateurs initial** = mapping **adresse
  ML-DSA → enjeu** (cohérent avec `validator_stakes()` re-clé en PQ-MIG-3B).
- **Déterminisme strict** : la genèse est une **constante** (mêmes adresses, mêmes montants), son
  hash est **figé** ; deux nœuds construisent la **même** genèse byte-à-byte (C1).
- Hash de genèse via le hachage de bloc existant (domaine-séparé), adresses via PQ-MIG-2.

## 2. Allocation initiale = placeholder dev marqué (🛑 §12)
- La **distribution des tokens** est **indécise** (pré-genèse). Donc l'allocation initiale est un
  **placeholder de développement clairement marqué 🛑**, pas une promesse : une ou quelques
  adresses ML-DSA de test avec des montants nominaux, suffisant pour que le harnais tourne.
- **Tie au §12** : montants, nombre de validateurs initiaux, enjeu initial par validateur, tout
  est **marqué réglable**. Ne fige **aucune** valeur en dur comme définitive.

## 3. Conservation dès le bloc 0 (EMIT-1 / classe conservation)
- **Invariant** : `miné` initial == **somme de l'allocation de genèse** (l'enjeu de genèse est de
  la valeur allouée, pas créée en plus). Dès le bloc 0,
  `Σ(spendable + staked + unbonding) + brûlé == miné` tient.
- La genèse ne doit **pas** ouvrir une création de valeur silencieuse : ce qui est en enjeu de
  genèse provient de l'allocation, conservation **exacte**.

## 4. Bump de version protocolaire
- Incrémente `TORUS_PROTOCOL_VERSION` (la genèse PQ est une rupture de protocole). Note l'ancienne
  → nouvelle au tracker.

## 5. Les dents (obligatoire)
- **genèse déterministe** : deux constructions de genèse ⇒ **hash identique** (vecteur figé, comme
  PQ-MIG-2).
- **conservation à la genèse** : `miné == Σ allocation` au bloc 0 ; planter un enjeu de genèse non
  couvert par l'allocation **casse** le test (le revert mord).
- **validateurs initiaux** : `validator_stakes()` au bloc 0 reflète le mapping de genèse, indexé
  par **adresse ML-DSA**.
- **enchaînement** : un premier bloc bâti sur la genèse PQ valide (couverture/émission/binding
  PQ-MIG-3B) et conserve.
- **déterminisme global** : C1 vert sur une chaîne partant de la genèse PQ.

## Garde-fous
- Réutiliser identité ML-DSA (PQ-MIG-1), adresses (PQ-MIG-2), autorité/enjeu re-clé (PQ-MIG-3B).
  Ne pas redéfinir.
- **Périmètre : la genèse + version seulement.** Pas de gossip de votes, pas de slashing vivant,
  pas de réconciliation clé-de-vote↔clé-d'enjeu (chantier suivant). **§4 STOP** si requis.
- **Diff logique seule** ; `dispatcher.rs` intact ; pas de nightly-fmt fichier entier.
- **Déterminisme** : genèse constante, hash figé ; `src/sm/` sans-IO ; **C1 vert**.
- **Conservation** : exacte dès le bloc 0 ; un déséquilibre de genèse est un **bug**, pas à masquer.
- **§12 marqué** : allocation/validateurs initiaux = placeholders réglables, jamais figés
  définitifs.
- **Snapshot git** avant.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant les dents §5 (genèse déterministe + conservation au bloc 0).
- `clippy --lib -D warnings` propre · **C1 vert** · sweep + couverture + conservation verts ·
  `src/sm/` sans-IO.
- `git diff` logique seule · `dispatcher.rs` intact · `TORUS_PROTOCOL_VERSION` bumpé · invariants de
  finalité (GADGET-1/3/4) verts.
- Entrée PQ-MIG-5 au tracker + auto-revue §3 (genèse ML-DSA, allocation placeholder marquée,
  conservation au bloc 0, bump version, périmètre tenu).

## Séquence
1. **§1** genèse déterministe = mapping adresse ML-DSA → (solde, enjeu) + règles + hash figé.
2. **§2** allocation = placeholder dev marqué 🛑 (§12).
3. **§3** conservation exacte dès le bloc 0.
4. **§4** bump `TORUS_PROTOCOL_VERSION`.
5. **§5** les dents.

> Après PQ-MIG-5, la **migration post-quantique est complète** : racine, adresses, autorité, enjeu
> et **genèse** entièrement ML-DSA. Quanta est, de la genèse au consensus, réellement
> post-quantique. Reste, hors migration, le **gros morceau** : la réconciliation clé-de-vote ↔
> clé-d'enjeu pour câbler le gadget en vivant (votes par gossip, slashing sur ledger réel). Et tes
> décisions §12 (allocation réelle, montants) + la frontière gravé/ajustable d'ADR-006.
