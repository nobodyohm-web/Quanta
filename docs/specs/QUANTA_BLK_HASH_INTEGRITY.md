---
type: task-spec
id: BLK-HASH-1
status: à exécuter
priorité: bloquant (avant T0.8)
classe: incrément consensus (change le hash de bloc)
trouvé par: harnais T0.7 (test de sûreté), 2026-06-21
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[AUDIT_QUANTA_2_PROGRESS]] · [[docs/decisions/ADR-001 — Fork-choice]]
---

# BLK-HASH-1 — Le hash de bloc doit committer le contenu

> Tâche unique, vérifiable, à exécuter AVANT T0.8. Le harnais a trouvé le défaut
> tout seul ; cet incrément le corrige proprement et s'en sert pour le prouver.
> Pas de devinette consensus (règle d'arrêt §4). Diff logique seule.

## 1. Le défaut (précis)

`Block.hash = H(index ‖ prev_hash ‖ timestamp ‖ tx_count ‖ merkle_root)` où
`merkle_root` est calculé sur **`tx.id = "tx_{compteur}"`**, un identifiant
positionnel **local**, pas sur le contenu. De plus **`block.miner` n'est PAS**
dans la pré-image.

Conséquences :
- Le hash ne lie réellement que `(index, prev_hash, timestamp, tx_count)`.
- Le **contenu** des tx (`from`, `to`, `amount`, `nonce`, `tx_type`, signature)
  n'est **pas** committé par le hash.
- Les compteurs étant locaux, deux blocs **différents** produits par deux nœuds
  au même `(index, timestamp, tx_count)` **collisionnent** (même hash).
- Le commentaire « WEAK-4 : le hash commet le contenu via Merkle » est **faux** :
  le Merkle hache des ids-compteur, pas le contenu.

Vecteur à fermer explicitement : une tx de **minage** (`NETWORK→mineur`) n'est ni
signée ni liée par le hash. On peut changer `NETWORK→alice` en `NETWORK→attaquant`
(et `block.miner` en conséquence) sans altérer le hash ni casser de signature.
C'est un **vol de récompense de bloc** potentiel. À tester (T2).

## 2. Pourquoi avant T0.8 (non négociable)

Ton vérificateur de sûreté `Sim::check_invariants` compare **`block.hash`** à
chaque index. Si le hash ne lie pas le contenu, deux nœuds tenant des blocs au
**contenu différent mais au hash identique** passent le test : **faux négatif**.
Donc l'invariant de sûreté ne vaut que ce que vaut le content-binding du hash.

T0.8 (porte d'acceptation globale Phase 0) s'appuie sur `run_checked`, donc sur ce
checker. La passer maintenant = valider la Phase 0 sur un vérificateur amputé.
**Le fix est un prérequis de la porte, pas un incrément qui peut attendre après.**

## 3. Règle d'arrêt §4

Lier le contenu dans le hash est un **correctif de défaut**, pas un arbitrage :
procède. **STOP et remonte avec options** seulement si l'audit (§4.3) révèle que
le fix changerait une **règle de validation consensus** qui est un vrai choix de
conception (p. ex. sémantique de `nonce`, fenêtre d'anti-rejeu, format d'identité
de tx exposé à l'utilisateur). Changer la pré-image du hash n'est pas ça ;
redéfinir l'identité de tx au niveau protocole en est.

## 4. Le correctif (minimal, blast radius maîtrisé)

Approche retenue : **lier le contenu sans toucher l'identité locale `tx.id`**.
Ne réécris pas `tx.id` partout (énorme blast radius). Change uniquement ce qui
doit committer le contenu.

### 4.1 Feuilles de Merkle = contenu canonique, pas l'id
Remplacer la feuille `tx.id` par `H(bytes_canoniques(tx) ‖ signature)`.
- Réutilise la fonction **déjà existante** qui produit les octets signés d'une tx
  (la pré-image de signature) comme commitment de contenu ; concatène la
  signature pour la lier aussi (Ed25519 et la variante ML-DSA **déterministe**
  rendent ça reproductible).
- **Sérialisation canonique stricte** : ordre de champs fixe, domaine-séparé,
  **aucune** itération de map (même discipline déterminisme que le cœur, §3).
- Si aucune fonction de pré-image de signature réutilisable n'existe, écris un
  `tx_content_bytes(&tx)` canonique dédié (champs : `from, to, amount, nonce,
  tx_type, timestamp`, ordre fixe), puis `leaf = H(content_bytes ‖ signature)`.

### 4.2 `miner` dans la pré-image du bloc
Ajouter `block.miner` à la pré-image du hash de bloc :
`H(index ‖ prev_hash ‖ timestamp ‖ miner ‖ tx_count ‖ merkle_root)`.

### 4.3 Audit obligatoire des usages de `tx.id` (avant d'écrire le fix)
Le même défaut peut se cacher ailleurs. Liste **chaque** usage de `tx.id` et
classe-le **consensus-pertinent** vs **bookkeeping local pur**. En particulier :
- **Dedup** (`seen_tx_hashes`) : keyé sur le **compteur** `tx.id` ou sur un **hash
  de contenu** ? Si c'est le compteur ⇒ **même classe de bug** (deux tx
  différentes collisionnent comme « déjà vues ») ⇒ **fix obligatoire** : keyer sur
  le hash de contenu. **Pas optionnel.**
- **Nonce / anti-rejeu** : vérifier qu'aucune décision de rejet ne repose sur
  l'unicité du compteur entre nœuds.
- Tout `==`/lookup supposant que `tx.id` identifie une tx **entre** nœuds.
Reporte les résultats de l'audit **avant** la modif (le blast radius doit être
connu, pas découvert).

### 4.4 Fixtures
Ce changement modifie **tous** les hash de bloc (genèse incluse). Tout hash de
bloc en dur dans les tests/fixtures casse. **Mets-les à jour**, ne les
contourne pas par du cas-spécial. Si la genèse a un hash en dur, recalcule-le.

## 5. Tests obligatoires

- **T1 — collision fermée.** Deux blocs au contenu différent (récompense de
  minage différente, ou transfert différent) mais `(index, prev_hash, timestamp,
  tx_count)` identiques produisent maintenant des **hash différents**. C'est le
  cas exact qui collisionnait.
- **T2 — vol de récompense rejeté.** Prendre un bloc valide, changer le
  destinataire de la tx de minage **et** `block.miner` vers un attaquant, le
  livrer au cœur : il doit être **rejeté** (mismatch de hash maintenant que
  `miner` + contenu sont liés). `blocks_rejected==1`, chaîne inchangée.
- **T3 — tamper de tx signée.** Modifier un champ d'une tx contenue change le
  hash de bloc ⇒ rejet (re-confirme C4 sous le nouveau Merkle).
- **T4 — retirer le pansement timestamp.** Dans
  `sim_partition_fork_breaks_safety_and_is_detected`, **supprimer** le décalage
  de timestamp artificiel. Les deux côtés doivent forker parce que leur
  **contenu** diffère (miners différents ⇒ blocs différents), et la violation de
  sûreté doit être détectée **pour la bonne raison**. Si sans le pansement ça ne
  forke plus, le fix est **incomplet** (`miner` mal lié) ⇒ investigue, ne remets
  pas le pansement.
- **T5 — conservation à travers le reorg.** Un `run_checked` à travers un fork à
  **hauteur égale avec re-queue**, assertant `Σ soldes + brûlé == miné`. C'est le
  double-mint suspecté depuis deux revues (la tx de minage du perdant remise en
  pending). Si ça viole ⇒ **vrai bug trouvé, reporte-le**, ne masque pas le test.
- **Déterminisme.** Le méta-test C1 (128 runs byte-identiques) doit **rester
  vert** : la sérialisation canonique des feuilles ne doit introduire aucune
  itération de map ni lecture d'horloge/`OsRng`.

## 6. Garde-fous de process (les leçons des incréments précédents)

- **Diff logique seule.** **PAS** de nightly-fmt sur des fichiers entiers. Formate
  uniquement les lignes que tu touches ; si une dérive nightly apparaît sur du
  code non touché, **ne la reflow pas** (leçon `ledger.rs`). Vérifie par
  `git diff` que le seul changement sémantique est le hash/merkle/dedup + fixtures.
- **Ne touche pas `dispatcher.rs`** côté formatage (il porte encore la purge
  crypto-only non commitée ; un revert le casserait).
- **Pas de masquage.** Toute surprise (T5 qui viole, audit qui élargit le scope)
  se **reporte**, ne se contourne pas par un pansement de test.
- **Snapshot = git.** Avant de commencer : note l'état `git`. Si tu dois revenir
  en arrière, `git checkout HEAD -- <fichier>` puis ré-applique la logique seule.

## 7. Porte d'acceptation

- `cargo test --lib` **vert** (suite complète + T1–T5).
- `cargo clippy --lib -- -D warnings` **propre**.
- `src/sm/` sans-IO **propre** (grep horloge/`OsRng`/IO).
- **Méta-test C1 vert** (déterminisme préservé).
- **`git diff` confirmé logique seule** (hash/merkle/dedup + fixtures + tests ;
  zéro reflow sur du code non touché).
- Auto-revue invariants §3 dans `AUDIT_QUANTA_2_PROGRESS.md` :
  déterminisme / arithmétique / robustesse / **sécurité (contenu + miner liés au
  hash ; dedup content-addressed)** / mémoire / tests.

## 8. Séquence

1. **Audit §4.3** (usages de `tx.id`), reporter le blast radius.
2. **Fix §4.1 + §4.2** (+ dedup si keyé compteur).
3. **Fixtures §4.4** (hash genèse/bloc en dur).
4. **T1–T3** (binding + vol de récompense).
5. **T4** (dé-pansement partition).
6. **T5** (conservation sur reorg).
7. **Puis seulement T0.8** (incrément séparé, porte globale qui veut enfin dire
   quelque chose).

> Le harnais a fait son travail. Cet incrément répare ce qu'il a trouvé, puis lui
> redonne des dents fiables. Ne lance pas T0.8 avant que cette porte soit verte.
