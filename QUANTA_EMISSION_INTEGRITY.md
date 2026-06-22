---
type: task-spec
id: EMIT-1
status: à exécuter
priorité: bloquant (avant T0.8)
classe: incrément consensus (re-queue reorg + règle d'émission + invariant harnais)
suite de: BLK-HASH-1 (le harnais a trouvé que la conservation est aveugle au mint illégitime)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[AUDIT_QUANTA_2_PROGRESS]] · [[docs/decisions/ADR-001 — Fork-choice]]
---

# EMIT-1 — Pas de double-mint au reorg + invariant d'émission

> Incrément unique, vérifiable, AVANT T0.8. On ferme la classe entière, pas le
> symptôme : (1) le re-queue ne doit pas ressusciter une récompense de bloc
> perdu, (2) une règle de validation interdit plus d'une récompense par bloc même
> à un nœud malveillant, (3) le harnais gagne l'invariant qui rend la chose
> visible. Diff logique seule. Pas de devinette consensus (§4).

## 1. Le défaut (précis)

Au reorg d'un fork à hauteur égale, les tx « exclusives » du bloc perdant sont
**re-queue** en pending (AUDIT-BLK-1, pour ne pas perdre les transferts). Mais ce
re-queue inclut la tx de minage synthétique (`NETWORK→mineur`) du bloc perdant.
Si elle est scellée dans un bloc ultérieur, le réseau a **minté deux récompenses**
là où la compétition à hauteur 1 n'en devait produire qu'une.

La **conservation ne l'attrape pas** : `total_minted` est pending-aware, donc les
deux récompenses sont comptées des deux côtés de `Σ soldes + brûlé == miné`,
l'équation reste vraie. C'est le même angle mort que le hash aveugle au contenu
créait pour la sûreté, côté conservation cette fois : la conservation voit la
monnaie qui apparaît **sans** mint (fantôme), pas un mint **illégitime**.

> Note : T5 (BLK-HASH-1) passe **identiquement** que la récompense du perdant soit
> re-queue ou non. Il ne tranche donc pas. Cet incrément ajoute le test qui tranche.

## 2. Pourquoi avant T0.8

T0.8 (porte globale Phase 0) balaie N seeds, reorgs inclus, sous `run_checked`.
Sans invariant d'émission, le balayage a **exactement** l'angle mort de T5 :
il validerait la Phase 0 avec un double-mint possible qu'aucun des deux invariants
(sûreté, conservation) ne regarde. Le fix + l'invariant sont **prérequis** de la
porte.

## 3. Règle d'arrêt §4

Exclure une récompense de bloc perdu du re-queue, et interdire deux récompenses
dans un bloc, sont des **correctifs** (le modèle d'émission veut une récompense
par bloc) : procède. **STOP et remonte avec options** seulement si l'audit révèle
que le chemin de production scelle **légitimement** des blocs à zéro ou plusieurs
récompenses (alors « ≤ 1 » serait faux), ou si le barème d'émission lui-même est
un vrai choix ouvert. Le cas par défaut ci-dessous n'est pas un arbitrage.

## 4. Le correctif (3 pièces)

### 4.0 Audit d'abord (comme BLK-HASH-1)
Localise le chemin de reorg exact (`pop tip → revert cache → re-queue → push
winner`) et **où** les tx du perdant sont remises en pending. Vérifie s'il existe
déjà un prédicat « expéditeur synthétique » (`is_synthetic_sender`, ou test
`from == "NETWORK" || from == "ESCROW"`). Reporte le constat **avant** de modifier.

### 4.1 Re-queue : exclure le synthétique (le bug immédiat)
Distinction **critique**, ne pas confondre les deux opérations :
- **Revert de cache** au reorg : porte sur **TOUT** le bloc perdant, minage
  compris (sinon le perdant reste crédité ⇒ conservation cassée dans l'autre sens).
  **Inchangé.**
- **Re-queue vers `pending`** : **uniquement les transferts utilisateur**
  (`from` = vraie clé publique). **Jamais** les tx synthétiques (`NETWORK` minage,
  `ESCROW` release) : elles appartiennent au bloc, pas au mempool.
Concrètement : filtrer la liste re-queue, `retain(|tx| !is_synthetic_sender(tx))`.

### 4.2 Règle de validation : une récompense par bloc (la vraie défense)
Le 4.1 corrige le chemin **honnête**. Un nœud **malveillant** peut forger un bloc
à deux tx de minage dont la somme passe la borne actuelle (qui ne vérifie que le
total). Ajouter à `validate_remote_block` / `validate_block_*` :
- **au plus une** tx de type `Mining` par bloc ;
- si présente : `from == "NETWORK"`, `to == block.miner`, et `amount ≤` la borne
  d'émission par bloc déjà en vigueur (garde le sens `≤`, ne sur-contraint pas).
Sinon ⇒ **rejet** du bloc. (Le `to == block.miner` est belt-and-suspenders avec
BLK-HASH-1 qui a déjà mis `miner` dans le hash.)

### 4.3 Invariant harnais : émission (le tube qui rend ça visible)
Ajouter à `Sim::check_invariants` une 3ᵉ variante `Violation::Emission` :
- structurel et indépendant du barème : **`count(tx Mining dans la chaîne) ≤
  chain_height`** (au plus une récompense par bloc non-genèse).
- (optionnel, plus fort) valeur : `stats().total_mined (chaîne seule) ≤ barème
  cumulé(hauteur)`.
Porte la `seed` comme les autres `Violation`.

## 5. Tests obligatoires

- **E1 — la question enfin tranchée.** Après un reorg à hauteur égale (livrer le
  hash bas puis le haut), asserter que la tx de minage du **perdant n'est PAS dans
  `pending`** (`stats().pending` ne contient aucune tx `Mining` synthétique). C'est
  le test que T5 ne pouvait pas être.
- **E2 — deux récompenses rejetées.** Un bloc forgé avec **deux** tx `Mining`
  (somme sous la borne) est **rejeté** par le cœur. `blocks_rejected==1`.
- **E3 — récompense mal créditée rejetée.** Un bloc dont la tx `Mining` crédite
  un autre que `block.miner` est **rejeté**.
- **E4 — dents de l'invariant.** Injecter un état chaîne avec une tx `Mining` de
  trop (hauteur 1, deux mining en chaîne) ⇒ `run_checked` renvoie
  `Err(Violation::Emission{seed})`.
- **E5 — run sain.** Un run honnête garde sûreté + conservation + **émission**
  vrais à chaque pas, et la chaîne avance (vivacité).
- **int1 / modèle d'émission.** La règle 4.2 invalide le bloc à deux mining txs
  que `int1` scellait après BLK-HASH-1. **Restructure `int1`** pour sceller les
  deux récompenses de B dans **deux blocs séparés** (ou une seule), sans relâcher
  la règle. Confirme au passage que le **chemin de production** (`pos_seal_if_leader`)
  ne peut pas accumuler plusieurs récompenses non scellées dans un bloc ; note le
  constat.
- **Déterminisme.** Méta-test C1 (128 runs byte-identiques) **vert**.

## 6. Garde-fous de process (les mêmes, ils ont marché)

- **Diff logique seule.** Pas de nightly-fmt sur fichiers entiers ; formate
  seulement les lignes touchées ; ne reflow pas une dérive sur du code non touché.
  `git diff` doit montrer uniquement : filtre re-queue + règle de validation +
  invariant émission + tests + `int1` restructuré.
- **Ne touche pas `dispatcher.rs`** côté formatage.
- **Pas de masquage.** Si E1/E4 révèlent autre chose, ou si `int1` cache encore un
  effet, **reporte**, ne contourne pas par un pansement de test.
- **Snapshot = git** avant de commencer ; reviens par `git checkout HEAD -- <f>`
  si besoin, puis ré-applique la logique seule.

## 7. Porte d'acceptation

- `cargo test --lib` **vert** (suite + E1–E5 + `int1` restructuré).
- `cargo clippy --lib -- -D warnings` **propre**.
- `src/sm/` sans-IO **propre**.
- **Méta-test C1 vert**.
- **`git diff` logique seule** confirmé (zéro reflow ; `dispatcher.rs` intact).
- Auto-revue §3 dans `AUDIT_QUANTA_2_PROGRESS.md` : déterminisme / arithmétique /
  robustesse / **sécurité (re-queue sans synthétique ; ≤ 1 récompense/bloc ;
  recipient == miner ; invariant émission)** / mémoire / tests.

## 8. Séquence

1. **Audit 4.0** (chemin re-queue + prédicat synthétique), reporter.
2. **Fix 4.1** (filtre re-queue), **4.2** (règle de validation), **4.3** (invariant).
3. **E1–E5** + **restructurer `int1`**.
4. **Puis T0.8**, avec l'invariant d'émission **inclus** dans le balayage
   multi-seed. La porte globale regarde enfin les trois : sûreté, conservation,
   émission.

> Rappel de cadre : ce n'est pas une catastrophe, c'est le harnais qui fait son
> métier. On ferme la classe, on rend la chose visible pour toujours, et la porte
> T0.8 devient une vraie porte.
