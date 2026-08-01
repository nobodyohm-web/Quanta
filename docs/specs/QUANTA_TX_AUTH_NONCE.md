---
type: task-spec
id: TX-AUTH-NONCE-1
status: à exécuter
priorité: 🔴 CRITIQUE — hang ~2⁶⁴ sous le lock du ledger + griefing/censure de nonce inter-nœuds
classe racine: champs de tx non signés et plages non bornées
origine: HARDEN-AUDIT-1 (TX-AUTH-NONCE) + cap Sybil différé de PRESIG-ORDER (§4)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[QUANTA_BLK_HASH_INTEGRITY]] (BLK-HASH-1) · [[QUANTA_FORK_CAP]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# TX-AUTH-NONCE-1 : authentifier et borner le nonce et le hash de tx

> 🔴 CRITIQUE. Le **nonce** et le **hash** d'une tx vivent **hors du préimage signé**, donc
> ils sont **falsifiables** sur une tx pourtant signée (griefing/censure inter-nœuds). Et une
> boucle de **gap de nonce** sous le **lock du ledger** peut être poussée à **~2⁶⁴** (hang
> global, DoS). Changement **profond** (le préimage signé et le hash de tx), donc **isolé**.
> Pré-genèse : aucune migration. Diff logique seule, déterminisme (C1) préservé.

## 0. Étape zéro — cartographie FORK-CAP (à RAPPORTER, cette fois)
Relis la section FORK-CAP du tracker et **écris explicitement dans ton compte rendu** ce que
la branche reorg saute, mot pour mot.
- Si elle ne saute **que** l'émission → procède.
- Si elle saute aussi **la signature ou la structure** → **STOP et reporte** : un trou de
  signature sur le chemin reorg prime sur ce spec, on bascule sur la **convergence des
  chemins**. Ne code pas.
Ce point était déjà demandé à PRESIG-ORDER et n'a pas été rapporté : ne le saute pas.

## 1. Borner le hang d'abord (le DoS immédiat, indépendant de l'auth)
Avant toute chose : la boucle/allocation indexée par une plage dérivée du nonce **ne doit
jamais** itérer sans borne, **a fortiori** sous le lock du ledger. Rejette (ou traite sans
boucler) toute tx dont le nonce est **trop loin** devant l'attendu, au-delà d'un écart borné.
Ça ferme le hang ~2⁶⁴ **quel que soit** l'état de l'authentification.
- **§4** : l'**existence** de la borne est requise (soundness). Si sa **valeur** exacte est un
  vrai choix de politique, **signale-la**, ne l'invente pas en dur sans le dire.

## 2. Authentifier les champs : nonce et hash dans le préimage signé
Lie le **nonce** et le **hash** dans le **préimage signé** de la tx, de sorte que la signature
les couvre. Toute falsification d'un de ces champs **invalide** alors la signature. Ferme le
griefing/censure : un tiers ne peut plus altérer le nonce d'une tx signée par autrui.

## 3. Recalculer le hash à la réception
Le hash de tx est **recalculé depuis le contenu** à la réception et **jamais** cru depuis le
fil ; un hash reçu qui ne correspond pas au hash calculé ⇒ **rejet**. Plus de malléabilité de
hash.
- **Interaction BLK-HASH-1** : confirme que le hash utilisé dans le Merkle de bloc est bien le
  hash **recalculé**, cohérent partout (signature, vérification, sim déterministe, tests).

## 4. Cap anti-replay-safe des maps de nonce/rate (le différé de PRESIG-ORDER)
Le résidu Sybil de PRESIG-ORDER se ferme ici, sur la même structure (`NonceTracker`).
- Ajoute un **last-seen** par entrée et une **éviction par péremption** (≥90s, alignée sur la
  fenêtre de fraîcheur : un message ré-émis échouerait de toute façon la fraîcheur).
- **Crucial** : la péremption borne par le **temps**, pas par le **débit**. Un flood Sybil
  rapide dans 90s peut encore gonfler la map. Ajoute donc **aussi** une **borne de taille
  absolue** (éviction LRU/au plus ancien quand la borne est atteinte). Péremption **et**
  taille, pas l'une seule.
- L'éviction doit **préserver l'anti-replay** (ne pas rouvrir une fenêtre de rejeu en évinçant
  une entrée encore protectrice). **§4** : si la politique exacte (taille, ordre d'éviction)
  est un vrai choix, signale-la.

## 5. Tests (adverses, obligatoires)
- **anti-hang** : une tx à nonce ~2⁶⁴ (ou très loin devant) ⇒ **rejetée/bornée**, aucun hang,
  le lock du ledger n'est pas retenu en boucle.
- **nonce/hash falsifié** : une tx signée dont on altère le nonce ou le hash ⇒ **signature
  invalide**, rejetée.
- **hash malléable** : hash du fil ≠ hash recalculé ⇒ **rejet**.
- **cap Sybil** : un flood de N keypairs valides ⇒ la map reste **bornée** (péremption **et**
  taille), sans rouvrir de fenêtre de rejeu.
- **chemin heureux + déterminisme** : une tx valide passe ; **C1 vert** (le nouveau préimage
  garde l'accord inter-nœuds à l'octet).

## Garde-fous
- **Diff logique seule** ; pas de nightly-fmt fichier entier ; `dispatcher.rs` intact (ce
  travail est au niveau ledger/tx).
- **Changement de préimage = profond** : reflète-le **partout** où le préimage est construit
  ou vérifié (signature, vérif, sim déterministe, tests). Aucune incohérence.
- **Pas de masquage** : le hang et la falsifiabilité se ferment **à la racine** (borne +
  préimage), jamais par un test mou.
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant les 5 tests adverses du §5.
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert** ·
  **sweep par défaut vert**.
- **`git diff` logique seule** · `dispatcher.rs` intact.
- Entrée **TX-AUTH-NONCE-1** au tracker + auto-revue §3, avec : **le résultat verbatim de
  l'étape 0** (ce que reorg saute), la borne de nonce, le nouveau préimage, le recalcul de
  hash, le cap (péremption + taille), et toute politique renvoyée en §4.

## Séquence
1. **Étape 0** : cartographie, rapporter, procéder ou escalader.
2. **§1** borner le hang (le DoS immédiat).
3. **§2 + §3** préimage signé + recalcul de hash.
4. **§4** cap anti-replay-safe (péremption + taille).
5. **§5** tests adverses.

> Une tx dont le nonce et le hash ne sont ni signés ni bornés est à la fois falsifiable et un
> levier de DoS. On borne d'abord, on authentifie ensuite, on plafonne la mémoire, et la
> dernière classe consensus-critique du backlog se referme, hors convergence des chemins.
