---
type: task-spec
id: EMIT-1-VERIFY
status: à exécuter
priorité: vérification (avant T0.8)
classe: audit de complétude EMIT-1 (pas une feature)
suite de: EMIT-1 (clos) · revue senior post-EMIT-1
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# EMIT-1-VERIFY — Vérifier deux moitiés non prouvées d'EMIT-1

> Tâche de **vérification**, pas de feature. **Audit et rapport d'abord.** Ne
> touche au code **que si** un trou réel est trouvé. Si tout est déjà bon, le
> livrable est une **confirmation documentée**, jamais du churn fabriqué. Diff
> logique seule.

## 1. Objet

La revue d'EMIT-1 a relevé deux affirmations non prouvées par les tests actuels.
Cet incrément les vérifie, et ne corrige que ce qui manque.

## 2. Vérification A — le re-queue préserve-t-il les transferts utilisateur ?

Le but originel d'**AUDIT-BLK-1** : ne **pas** perdre les transferts utilisateur au
reorg. Le fix EMIT-1 §4.1 dit « exclure le synthétique, **garder** les transferts
utilisateur ». Le test `losing_block_reward_is_not_requeued` ne prouve qu'**une
moitié** (le minage du perdant n'est pas re-queue). Si le filtre a **sur-corrigé**
et vide tout le re-queue, les transferts utilisateur sont **silencieusement perdus**
au reorg, ce qui réintroduit exactement le bug qu'AUDIT-BLK-1 réparait, et aucun
test ne le verrait.

**À faire :**
1. **Audit du filtre** (`retain` du re-queue) : confirme qu'il retient bien les
   `from` = clé publique réelle et ne retire **que** `NETWORK`/`ESCROW`. Reporte le
   prédicat exact.
2. **Audit du test E1** : met-il un **transfert utilisateur signé** dans la branche
   perdante et asserte-t-il qu'il **survit en pending** après le reorg ?
3. **Si la moitié « préservation » n'est pas testée**, ajoute un test (ou étends E1)
   où le bloc **perdant** contient à la fois :
   - une tx de minage synthétique, et
   - un transfert utilisateur signé.
   Après reorg (livrer le hash bas puis le haut), asserter **les deux** :
   - la tx de minage du perdant **n'est PAS** dans `pending` (pas de double-mint), et
   - le transfert utilisateur du perdant **EST** dans `pending` (non perdu),
   - et conservation + émission tiennent (`run_checked` Ok).
   Nom suggéré : `losing_block_user_tx_requeued_but_reward_is_not`.

## 3. Vérification B — forme de l'invariant d'émission

L'égalité est l'outil tranchant : `count(minage en chaîne) == hauteur − 1` attrape
aussi une récompense **manquante** (bloc à zéro récompense), pas seulement une de
trop. L'inégalité `≤ hauteur` est plus molle.

**À faire :**
1. **Audit de `Violation::Emission`** : est-ce `≤` ou `==` aujourd'hui ?
2. **Détermine le fait** (depuis le code de scellement, coalesce de A) : **chaque**
   bloc non-genèse porte-t-il **toujours** exactement une récompense de proposeur,
   même un bloc vide ? (Réponse factuelle, pas un choix.)
   - **Si oui** → passe l'invariant à l'**égalité** `== hauteur − 1` (forme forte) et
     ajoute un test : un bloc à **zéro** tx de minage déclenche
     `Err(Violation::Emission)` (le cas « récompense manquante »).
   - **Si non** (un bloc peut légitimement avoir zéro récompense) → garde `≤`,
     **documente pourquoi** l'égalité ne tient pas, et note que l'invariant ne peut
     donc pas attraper une récompense manquante.
3. **Règle d'arrêt §4** : si « un bloc peut-il légitimement avoir zéro récompense »
   n'est **pas** déterminé par le comportement actuel mais est un **vrai choix de
   conception** ouvert, **STOP et remonte** avec les deux options ; ne tranche pas.

## 4. Garde-fous

- **Audit d'abord, rapport ensuite.** N'écris du code que pour combler un trou
  identifié. Pas de changement cosmétique, pas de test fabriqué pour « faire un
  livrable ».
- **Diff logique seule.** Pas de nightly-fmt sur fichiers entiers ; pas de reflow
  sur du code non touché ; `dispatcher.rs` **intact** côté formatage.
- **Pas de masquage.** Si la vérification A révèle que les transferts utilisateur
  **sont** perdus, c'est un **bug trouvé** : reporte-le et corrige le filtre
  (retenir le synthétique seulement), ne contourne pas le test.
- **Snapshot git** avant de commencer.

## 5. Porte d'acceptation

- `cargo test --lib` **vert** (suite + tout test ajouté).
- `cargo clippy --lib -- -D warnings` **propre**.
- `src/sm/` sans-IO **propre** · **méta-test C1 vert**.
- **`git diff` logique seule** · `dispatcher.rs` intact.
- Dans `AUDIT_QUANTA_2_PROGRESS.md`, une entrée **EMIT-1-VERIFY** qui reporte
  explicitement, pour A et pour B : ce qui était **déjà couvert**, ce qui a été
  **ajouté/corrigé** (le cas échéant), et la **forme retenue** de l'invariant avec
  sa justification.

## 6. Séquence

1. **Audit A + B**, reporter les constats avant toute modif.
2. Combler **seulement** les trous trouvés (test de préservation, forme `==`).
3. Mettre à jour le tracker (constats + actions).

> **Hors scope (manuel, pas l'agent).** Committer le purge crypto-only pour
> rebaser un HEAD propre est une opération git à faire à la main par Alexandre,
> avec revue. Ce n'est **pas** cette tâche, et l'agent **ne commit pas** le purge.
