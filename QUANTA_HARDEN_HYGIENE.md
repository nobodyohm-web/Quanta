---
type: task-spec
id: HARDEN-HYGIENE-1
status: à exécuter
priorité: passe groupée (vitesse supérieure, classe à faible risque)
classe: durcissement mécanique et indépendant, hors consensus
origine: HARDEN-AUDIT-1 — SLICE-CLASS + zeroize + bornes mémoire sûres + dents de tests
exclut: PRESIG-ORDER, TX-AUTH-NONCE, convergence des chemins, CRDT (specs chirurgicaux séparés)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[AUDIT_QUANTA_2_PROGRESS]] · [[QUANTA_FORK_CAP]]
---

# HARDEN-HYGIENE-1 : passe de durcissement groupée (la classe sans danger)

> Vitesse supérieure assumée : on traite en **une passe** tout l'hygiénique indépendant du
> backlog d'audit. Mais chaque correctif reste une **unité distincte et gated**, avec son
> test, son diff identifiable, pour que tu puisses committer fix par fix. **Rien de
> consensus-critique ici** : si un fix se révèle toucher la sémantique de consensus ou une
> décision (politique d'éviction), **STOP et reporte**, ne le bundle pas.

## Règle transverse
Pour **chaque** section ci-dessous : correctif minimal, **test qui prouve la propriété**,
diff **logique seule** et **identifiable** (l'agent garde les sections séparables, idéalement
un groupe de changements par fix). Pas de nightly-fmt fichier entier. `dispatcher.rs` intact.
**Snapshot git** avant de commencer.

## 1. SLICE-CLASS — slicing char-safe (ferme 2 critiques)
**Problème (audit)** : slicing d'octets sur des champs `String` télécommandés à 4 sites ;
un index qui tombe au milieu d'un caractère UTF-8 multi-octets **panique**, donc un adversaire
rend le nœud **sourd au gossip** (DoS distant).

- Écris **un seul helper char-safe** (troncature/découpe sur frontière de caractère, ex.
  via `char_indices`/`chars().take()`, ou un check de frontière `is_char_boundary`), et
  applique-le aux **4 sites**. Une classe, un correctif.
- **Test** : pour chaque site (ou en table), une chaîne UTF-8 dont un caractère multi-octets
  tombe pile à l'index de découpe ⇒ **aucune panique**, sortie tronquée proprement.
- **§4** : si un de ces champs alimente une décision de consensus (hash, validation), le
  changement de troncature pourrait changer une valeur consensuelle ⇒ **signale-le**, ne
  modifie pas la sémantique sans le dire.

## 2. ZEROIZE-SWEEP — frères du trou déjà comblé
**Problème (audit)** : au-delà de la clé Ed25519 déjà corrigée, d'autres types porteurs de
secret peuvent ne pas être effacés à la libération.

- Énumère **tout** type portant un secret (clés Ed25519 et ML-DSA, graines, matériel de
  dérivation, copies intermédiaires) et confirme l'effacement à la destruction.
- Applique `Zeroize`/`ZeroizeOnDrop` (ou l'effacement manuel équivalent) là où il manque.
- **Test** : là où c'est testable, vérifie qu'une zone secrète est bien remise à zéro après
  drop ; sinon, documente la couverture par revue.

## 3. MEM-BOUNDS — bornes mémoire **sûres uniquement**
**Problème (audit)** : collections atteignables depuis le réseau pouvant croître sans borne
(registre `@pseudo`, tables diverses) ⇒ DoS par épuisement.

- Borne les collections **non consensuelles** (table de pairs, ensembles de « vus », registre
  pseudo) avec une politique simple et déterministe (taille max plus éviction/refus claire).
- **§4, important** : **n'embarque pas** ici une borne dont la **politique d'éviction
  affecte le consensus** (typiquement le **mempool** : quoi évincer change ce qui entre en
  bloc). Ça, c'est un **choix** et un spec séparé ⇒ **signale-le**, ne tranche pas.
- **Test** : insère au-delà de la borne ⇒ la collection reste bornée, comportement défini.

## 4. TEST-TEETH — donner des dents aux tests vacueux nommés
**Problème (audit)** : certains tests d'invariant passent indépendamment de la propriété.

- Pour **chaque** test que l'audit a nommé vacueux, ajoute l'assertion manquante qui le fait
  **échouer si la propriété casse** (le test doit mordre).
- **Vérifie la morsure** : prouve qu'avant ton renforcement, une mutation cassant la
  propriété aurait fait passer le test, et qu'après, elle le fait échouer (raisonne-le ou
  démontre-le).

## Garde-fous
- **Diff logique seule**, sections séparables, `dispatcher.rs` intact.
- **Pas de masquage** : une panique/DoS doit être **corrigée**, jamais avalée par un test mou.
- **§4** : tout fix qui touche la sémantique de consensus ou une décision d'éviction ⇒
  **STOP et reporte**, sors-le de la passe.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant les tests des sections 1 à 4.
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert** ·
  **sweep par défaut vert**.
- **`git diff` logique seule**, sections identifiables · `dispatcher.rs` intact.
- Entrée **HARDEN-HYGIENE-1** au tracker + auto-revue §3, listant par section : ce qui a été
  corrigé, le test qui le prouve, et tout item **renvoyé en §4** (notamment une borne mempool
  ou un champ de slicing consensuel).

## Séquence
1. SLICE-CLASS (le plus haut impact, 2 critiques).
2. ZEROIZE-SWEEP.
3. MEM-BOUNDS (sûres seulement, mempool renvoyé en §4).
4. TEST-TEETH.

> Reste hors de cette passe, en specs chirurgicaux dédiés : **PRESIG-ORDER** (réordonner
> vérif-avant-mutation), **TX-AUTH-NONCE** (champs dans le préimage signé), la **convergence
> des deux chemins de validation** (après ta cartographie FORK-CAP), et **CRDT** (borner ou
> retirer, c'est une décision). Je les écris ensuite, un par un.
