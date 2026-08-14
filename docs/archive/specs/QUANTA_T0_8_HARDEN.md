---
type: task-spec
id: T0.8-HARDEN
status: à exécuter
priorité: faire de T0.8 une vraie porte (pas un sous-ensemble)
classe: harnais DST — couverture du plan de charge + réconciliation dure
dépend de: T0.8 (sweep+replay) · EMIT-1 (re-queue reorg) · C7 (temps injecté au scellement)
liens: [[QUANTA_T0_8_SWEEP]] · [[QUANTA_AGENT_CONSTITUTION]] · [[docs/decisions/ADR-001 — Fork-choice]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# T0.8-HARDEN — rendre la porte réelle

> T0.8 ne balaie aujourd'hui qu'un **sous-ensemble** : les transferts utilisateur ont
> été retirés (fuite d'horloge dans la création de tx), donc ni le **burn**, ni le
> **re-queue au reorg**, ni la **conservation-sous-burn** ne sont sous le microscope ; et
> la réconciliation a été rendue **monotone**, donc le sens du **reorg** et la **partition
> multi-blocs** ne sont jamais exercés. Deux phases **séquencées** corrigent ça. La Phase 2
> dépend de la Phase 1.

> **Hygiène, à faire avant de préférence (manuel, Alexandre).** Cet incrément touche du
> code adjacent à la prod (création de tx) **et** le harnais non tracké. Aujourd'hui ton
> filet git ne couvre pas `sm/sim.rs` (untracked) ni le purge (non commité). Committer un
> baseline propre **avant** donnerait un vrai filet. L'agent procède quand même avec un
> snapshot, mais le filet manque tant que ce n'est pas fait.

---

## PHASE 1 — temps injecté dans la création de tx, puis réintroduction des transferts

### 1.0 Audit
Où `next_tx` / `transfer_with_burn` lit-il `Utc::now()` ? Qu'a fait **C7** pour injecter
le temps au scellement (même patron à répliquer) ? Quel est le chemin **prod** (commande
Tauri) qui crée des tx ? Reporte avant de toucher.

### 1.1 Sortir l'horloge du cœur
Objectif : la création de tx **dans le cœur** ne lit **plus** l'horloge, elle reçoit un
**`ts_ms` injecté**. La lecture `Utc::now()` remonte au **bord IO** (le handler de commande
horodate avec `now()` et passe la valeur). Cohérent avec la discipline sans-IO déjà
appliquée au scellement (C7) et à la validation (sans horloge, prouvé par C2).
- **Contrainte** : comportement **prod inchangé** (la commande horodate toujours au moment
  réel). C'est une refacto du **point de lecture**, pas du sens.
- **Contrainte** : ne **pas** ajouter de validation du `ts` qui relirait l'horloge. Le `ts`
  est de la donnée signée et liée au hash (BLK-HASH-1) ; la validation reste sans horloge.

### 1.2 Réintroduire les transferts dans le plan de charge
`scenario(seed)` génère à nouveau des transferts utilisateur signés, **horodatés par
l'horloge virtuelle**, expéditeur/montant tirés du `Blake3Rng` seedé. Réinjecte-les dans
`execute_scenario`.

### 1.3 Ce que ça restaure (et qui était absent)
- **Burn** (1% par transfert) ⇒ la conservation exerce enfin `Σ soldes + brûlé == miné`
  **avec du brûlé réel**, pas seulement le minage.
- **Re-queue au reorg** (EMIT-1 / AUDIT-BLK-1) ⇒ les blocs perdants portent enfin des
  transferts utilisateur, donc le re-queue (garder user, exclure minage) passe sous le
  microscope multi-seed.
- Dynamique du mempool sous fautes.

### 1.4 Tests Phase 1
- **determinism_with_transfers** : un seed avec transferts rejoué deux fois ⇒ byte-identique.
  (C'est le test qui avait attrapé la fuite d'horloge ; il doit maintenant **passer**, ts
  injecté.) C1 reste vert.
- **conservation_under_burn** : sur le sweep, `Σ soldes + brûlé == miné` tient **avec des
  burns présents**.
- **coverage_transfers_and_burns** (anti-vacuité) : sur la plage par défaut, des transferts
  sont **réellement inclus en blocs** et des burns **se produisent**. Sinon le burn est
  généré mais jamais exercé.
- **prod_tx_still_timestamps_at_edge** : le chemin prod horodate toujours au réel (pas de
  régression de comportement).

---

## PHASE 2 — archétypes de réconciliation durs (dépend de la Phase 1)

### 2a Reorg hash-bas-d'abord (censé converger, jamais testé)
Aujourd'hui l'équivocation est livrée **hash-haut-d'abord** (monotone), donc le **sens du
reorg** n'est jamais exercé. Ajoute la livraison **hash-bas-d'abord** sur un fork single-block
au même index : les nœuds adoptent le hash bas, puis le hash haut arrive et **force un
retournement**. Le fork-choice actuel **prétend** gérer ce cas.
- **Test dédié déterministe** `single_block_reorg_lowest_hash_first_reconciles` : asserte
  la **convergence** + le **transfert utilisateur du perdant re-queue** (EMIT-1, désormais
  testable grâce à la Phase 1) + **pas de double-mint**.
- **Et** câble les **deux** ordres de livraison dans l'archétype d'équivocation, pour que le
  sweep aléatoire couvre les deux sens.
- **Attendu : vert.** S'il passe au **rouge** (sûreté, tx perdue, ou double-mint), c'est un
  **vrai bug dans un cas supporté** ⇒ **STOP et remonte** le seed (§7 de T0.8). Ne le verdis pas.

### 2b Partition multi-blocs (le gap connu, rendu explicite)
Les **deux** côtés avancent et produisent des chaînes concurrentes **multi-blocs**, puis
heal. Le fork-choice single-block actuel **ne sait pas** réconcilier ça : c'est la dette
**reportée au gadget** (ADR-001). On ne l'enfouit pas sous « convergent par construction »,
on en fait un **test explicite**.
- **D'abord, confirme** : l'archétype `partition+heal` actuel laisse-t-il **les deux** côtés
  avancer, ou un seul rattrape ? S'il n'en laisse qu'un, c'est pourquoi il « converge » ; le
  nouveau test fait avancer **les deux**.
- **Test explicite** `multiblock_partition_currently_diverges_GADGET_DEFERRED` : construit la
  partition deux-côtés-multi-blocs, vérifie **les trois** invariants, et **asserte le
  comportement actuel** avec un commentaire clair : « gap fork-choice connu, ADR-001, à
  **inverser** (asserter la convergence) quand le gadget de finalité atterrit ». Ce test
  devient la **cible d'acceptation du gadget**.
- **Règle d'arrêt §4, cruciale** :
  - une **divergence de sûreté** (deux tips persistants au même index) = le gap **connu** ⇒
    asserter + marquer gadget-deferred. Pas d'escalade, c'est attendu.
  - une violation de **conservation** ou d'**émission** (fonds dupliqués ou perdus entre les
    deux chaînes, double-mint au heal) = un bug **nouveau et pire** que le gap connu, que le
    gadget ne corrigerait pas forcément ⇒ **STOP et remonte**, ne le marque **pas** « connu ».

---

## Garde-fous de process
- **Diff logique seule.** Pas de nightly-fmt sur fichiers entiers ; `dispatcher.rs` intact.
- **Pas de masquage.** Ne fais pas passer 2b en **affaiblissant** le checker ni en ne
  laissant **qu'un** côté avancer. Ne fais pas passer 2a en revenant à la livraison monotone.
- **Prod inchangée** (Phase 1 : la commande horodate toujours au réel).
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant : Phase 1 (determinism_with_transfers,
  conservation_under_burn, coverage_transfers_and_burns, prod_tx_still_timestamps_at_edge),
  Phase 2a (reorg hash-bas vert), Phase 2b (test explicite gadget-deferred présent et
  asserté).
- Le **sweep aléatoire propre reste vert AVEC transferts** (sinon 2a/Phase 1 a trouvé un bug
  ⇒ reporter, pas verdir).
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert**.
- **`git diff` logique seule** · `dispatcher.rs` intact.
- Auto-revue §3 + entrée tracker : ce que la Phase 1 restaure (burn/re-queue/conservation),
  le résultat de 2a (vert, ou bug remonté), et le **constat factuel** de 2b (que diverge-t-il
  exactement, sûreté seule ou pire).

## Séquence
1. **Phase 1 entière** (audit → temps injecté → transferts réintroduits → tests 1.4). Elle
   doit être verte **avant** la Phase 2, car 2a teste le re-queue qui n'existe qu'avec des
   transferts.
2. **2a** reorg hash-bas (attendu vert ; rouge ⇒ STOP/report).
3. **2b** partition multi-blocs (constat factuel + §4 : sûreté = gap connu marqué ;
   conservation/émission cassée = STOP/report).

> **Hors scope / reporté.** Émission `≤`→`==` : toujours à réexaminer à **EMIT-LAW-1** (note
> portée à EMIT-1-VERIFY). Commit d'un baseline git propre = **manuel**, Alexandre.
