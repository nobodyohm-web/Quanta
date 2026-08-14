---
type: task-spec
id: GADGET-5B
status: à exécuter (pièce 5B ; l'aboutissement du gadget)
priorité: 🔴 résolution de partition — bascule le test 2b de diverge à réconcilie
classe: réconciliation de partition via le moteur GHOST (5A), avec conservation globale au heal
origine: [[DESIGN-FINALITY-GADGET]] §9 · résout le trou ADR-001 · construit sur GADGET-5A
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[QUANTA_GADGET_PIECE5A]] · EMIT-1 (la classe à ne pas rouvrir) · [[AUDIT_QUANTA_2_PROGRESS]]
---

# GADGET-5B : résolution de partition (l'aboutissement)

> Pièce finale du gadget. Quand deux partitions guérissent, le moteur GHOST (5A) choisit la branche
> au plus de poids, la **finalité interdit** de défaire l'histoire scellée, et les nœuds
> **convergent**. On fait basculer le test `t0_8_multiblock_partition_currently_diverges_gadget_
> deferred` de **diverge** à **réconcilie**. **Exigence critique** : au heal, la **conservation
> globale** tient, l'émission d'une branche perdante non finalisée est **défaite** proprement
> (sinon on rouvre la classe double-mint d'EMIT-1). Diff logique seule, déterministe.

## 1. Réconciliation au heal (via le moteur 5A)
- À la guérison d'une partition, chaque nœud applique le fork-choice GHOST (5A) sur l'ensemble des
  votes et blocs des deux côtés : la **tête commune** est la branche au plus de poids descendant du
  dernier point justifié, **plancher de finalité absolu**.
- Les deux nœuds, voyant les mêmes votes/blocs, **convergent** sur la même tête (déterminisme 5A).

## 2. Conservation globale au heal (le point délicat — EMIT-1 revient)
- Quand une branche perdante **non finalisée** est abandonnée, **défaire** ses effets : son
  **émission** (récompenses de bloc), et l'état (soldes, enjeu) qu'elle avait appliqué, sont
  **reversés** ; ses transactions non incluses dans la branche gagnante sont **re-mises en file**
  (comme AUDIT-BLK-1), pas perdues.
- **Invariant** : après réconciliation, `Σ(spendable + staked + unbonding) + brûlé == miné`
  **globalement**, exactement comme hors partition. **Aucune** pièce créée par une double émission
  de branche, aucune perdue par un abandon. C'est la classe EMIT-1, à l'échelle de la partition.
- Une histoire **finalisée** n'est **jamais** défaite (plancher 5A) : par la sûreté responsable
  (GADGET-4), deux branches finalisées en conflit sont impossibles sans ⅓ slashable, donc les
  parties finalisées **coïncident**.

## 3. Bascule du test 2b
- Inverse `t0_8_multiblock_partition_currently_diverges_gadget_deferred` : il assertait la
  **divergence** (marquée gadget-deferred) ; il asserte désormais la **convergence** des deux têtes
  après heal, **plus** la **conservation globale** (§2). Retire le marquage gadget-deferred.

## 4. Les dents (obligatoire)
- **réconciliation** : deux partitions multi-blocs divergentes, au heal, **convergent** sur une
  seule tête (le test 2b inversé).
- **conservation globale au heal** : après réconciliation, le bilan global conserve ; planter une
  émission de branche perdante non défaite **casse** le test (preuve que le revert mord).
- **finalité préservée** : une branche qui contredirait un point **finalisé** ne peut **jamais**
  gagner la réconciliation (plancher).
- **transactions re-mises en file** : une tx de la branche perdante, absente de la gagnante, n'est
  **pas perdue** (re-queue, comme AUDIT-BLK-1).
- **déterminisme** : les deux nœuds convergent identiquement (C1).

## Garde-fous
- Réutiliser le moteur GHOST (5A), la finalité (GADGET-3/4), la conservation (harnais). Ne pas
  redéfinir.
- **Diff logique seule** ; `dispatcher.rs` intact ; pas de nightly-fmt fichier entier.
- **Déterminisme** : réconciliation pure ; `src/sm/` sans-IO ; **C1 vert**.
- **Conservation globale** : c'est l'exigence n°1 ; un revert incomplet est un **bug**, pas à
  masquer.
- **Pas de masquage** : le test 2b inversé doit **réellement** réconcilier + conserver, pas passer
  vacueusement.
- **§4** : pas de slashing vivant ni de câblage dispatcher ici ; **STOP** si requis.
- **Snapshot git** avant.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant le **test 2b inversé** (réconcilie + conserve) et les dents
  §4.
- `clippy --lib -D warnings` propre · **C1 vert** · sweep + couverture + conservation verts ·
  `src/sm/` sans-IO.
- `git diff` logique seule · `dispatcher.rs` intact · **le marquage gadget-deferred retiré** du
  test 2b · invariants de finalité (GADGET-1/3/4) verts.
- Entrée GADGET-5B au tracker + auto-revue §3 (réconciliation, conservation globale au heal,
  bascule du test 2b, les dents).

## Séquence
1. **§1** réconciliation au heal via le moteur GHOST (5A).
2. **§2** conservation globale : défaire l'émission/état des branches perdantes, re-queue des tx.
3. **§3** inverser le test 2b (diverge → réconcilie + conserve).
4. **§4** les dents.

> Après GADGET-5B, le **gadget de finalité est complet** : finalité réelle (3), responsable (4),
> et résolution de partition (5). Le trou multi-blocs que tu traînais depuis des sessions est
> **fermé**, et le test 2b semé il y a longtemps comme cible **bascule enfin**. Restent, hors
> gadget : **PQ-MIG-5** (genèse PQ) et la réconciliation clé-de-vote ↔ clé-d'enjeu pour le câblage
> vivant (slashing et votes réels en production).
