---
type: task-spec
id: GADGET-5A
status: à exécuter (pièce 5A du gadget ; moteur de fork-choice)
priorité: 🔴 fork-choice conscient de la finalité, façon LMD-GHOST (moteur seul)
classe: dernier-vote-par-validateur + poids de branche par enjeu + tête au plus de poids, ancrée au dernier justifié
origine: [[DESIGN-FINALITY-GADGET]] §9 · construit sur GADGET-2 (votes) + GADGET-3 (justifié/finalisé)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[QUANTA_GADGET_PIECE4]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# GADGET-5A : fork-choice LMD-GHOST conscient de la finalité (le moteur)

> Pièce 5A. Le fork-choice ne suit **pas** la plus longue chaîne mais, à chaque embranchement, la
> branche au **plus de poids de votes** (GHOST), ancré au **dernier point justifié** (GADGET-3) et
> **ne revenant jamais** sur un point **finalisé**. Cette pièce fait le **moteur seul** ; la
> résolution de partition (bascule du test 2b) est **GADGET-5B**. Diff logique seule, déterministe,
> C1 vert. Pas de slashing vivant ici.

## 1. Dernier vote par validateur (LMD)
- Maintiens, par validateur, son **dernier** vote (latest-message). Pur, déterministe, indexé par
  l'identité de vote (adresse ML-DSA, cohérente depuis le re-keying).
- Un vote plus récent (époque cible supérieure) **remplace** le précédent pour ce validateur.

## 2. Poids de branche par enjeu (GHOST)
- Le **poids** d'un bloc/branche = somme de l'**enjeu** des validateurs dont le dernier vote
  **soutient** cette branche (descend de ce bloc). Enjeu = enjeu on-chain (identité ML-DSA).
- Pur et déterministe (sommes commutatives, pas d'ordre de HashMap qui fuit).

## 3. La règle GHOST, ancrée à la finalité
- Pars du **dernier point de contrôle justifié** (jamais en deçà du **finalisé**).
- À chaque embranchement, descends vers l'enfant de **plus grand poids** (§2). Départage
  déterministe en cas d'égalité (ex. plus petit hash, comme le fork-choice intérimaire).
- La tête est le bout de ce chemin. **Ne remonte jamais** au-dessus d'un point finalisé (plancher
  absolu).

## 4. Les dents (obligatoire)
- **le poids prime la longueur** : une branche **plus longue** mais de **moindre poids de votes**
  **perd** contre une branche plus courte mieux votée. (Le cœur de GHOST vs longest-chain.)
- **dernier vote remplace** : un validateur qui revote bascule son poids ; l'ancien vote ne compte
  plus.
- **plancher de finalité** : le fork-choice ne choisit **jamais** une branche qui contredirait un
  point **finalisé** (GADGET-3).
- **ancrage au justifié** : le départ est bien le dernier justifié, pas la genèse.
- **déterminisme** : mêmes votes + même enjeu ⇒ **même tête** sur deux nœuds (C1).

## Garde-fous
- Réutiliser votes (GADGET-2), justifié/finalisé (GADGET-3), enjeu on-chain (identité ML-DSA). Ne
  pas redéfinir.
- **Périmètre : le moteur de fork-choice seul.** Pas la résolution de partition (GADGET-5B), pas le
  slashing vivant. **§4 STOP** si l'un semble requis.
- **Diff logique seule** ; `dispatcher.rs` intact ; pas de nightly-fmt fichier entier.
- **Déterminisme** : tout pur ; `src/sm/` sans-IO ; **C1 vert**.
- **Pas de masquage** : les dents §4, surtout « poids prime longueur » et « plancher de finalité »,
  mordent.
- **Snapshot git** avant.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant les dents §4 (surtout poids-prime-longueur et plancher de
  finalité).
- `clippy --lib -D warnings` propre · **C1 vert** · sweep + couverture + conservation verts ·
  `src/sm/` sans-IO.
- `git diff` logique seule · `dispatcher.rs` intact · invariants de finalité (GADGET-1/3) verts.
- Entrée GADGET-5A au tracker + auto-revue §3 (LMD, poids de branche, règle GHOST ancrée, les
  dents, le périmètre tenu).

## Séquence
1. **§1** dernier vote par validateur (LMD).
2. **§2** poids de branche par enjeu.
3. **§3** règle GHOST ancrée au justifié, plancher de finalité.
4. **§4** les dents.

> Pièce suivante, **GADGET-5B** : la **résolution de partition**, qui s'appuie sur ce moteur pour
> faire basculer le test 2b (`...gadget_deferred`) de **diverge** à **réconcilie**, avec la
> **conservation globale au heal** (défaire l'émission des branches perdantes non finalisées).
> C'est l'aboutissement du gadget. Restent ensuite PQ-MIG-5 (genèse PQ) et la réconciliation
> clé-de-vote ↔ clé-d'enjeu pour le câblage vivant.
