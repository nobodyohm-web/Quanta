---
type: task-spec
id: PQ-MIG-3
status: à exécuter (pièce charnière du chantier PQ, §8 de la conception)
priorité: 🔴🔴 autorité de transaction en ML-DSA — ferme la faille de CRYPTO-ID-1
classe: verify_tx ML-DSA + liaison clé↔adresse dans la préimage + retrait du repli Ed25519 + câblage vault PQ
origine: [[QUANTA_PQ_MIGRATION_DESIGN]] §4.3, §4.4, §8 · construit sur PQ-MIG-1 et PQ-MIG-2
liens: [[QUANTA_AGENT_CONSTITUTION]] · CRYPTO-ID-1 (la faille à fermer) · [[QUANTA_PQ_MIG_2]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# PQ-MIG-3 : autorité de transaction en ML-DSA (fermer la faille CRYPTO-ID-1)

> **La pièce la plus importante du chantier.** C'est elle qui fait basculer la racine de confiance
> en post-quantique et **ferme la faille** que l'audit a trouvée : aujourd'hui un attaquant casse
> Ed25519, attache **sa propre** clé ML-DSA non liée à un compte qui n'est pas le sien, et passe.
> Après PQ-MIG-3, l'autorisation d'une transaction exige une **signature ML-DSA** dont la clé
> **hashe vers l'adresse `from`**. Diff logique seule, déterministe, C1 vert. La rigueur des dents
> est non négociable ici.

## Ce que cette pièce fait (couches liées, d'où sa densité)
Quatre changements **indissociables** (les séparer laisserait un état incohérent) :
1. `verify_tx` vérifie une **signature ML-DSA** (plus Ed25519).
2. Elle **exige la liaison** : la clé révélée doit satisfaire `lie(from, clé)` (PQ-MIG-2), c'est-à-
   dire `from == BLAKE3(ADDR_DOMAIN ‖ clé)`. **C'est ce qui ferme la faille.**
3. La **clé publique entre dans la préimage signée**, pour lier cryptographiquement signature,
   clé et adresse (on ne peut plus échanger la clé après coup).
4. Le **repli Ed25519 est retiré** du chemin d'autorité, `REQUIRE_PQ` saute (ou devient
   inconditionnel), et le **vault PQ est câblé en production** (l'échafaudage `#[allow(dead_code)]`
   de PQ-MIG-1 **disparaît**).

## 1. `verify_tx` en ML-DSA + liaison (le cœur)
- L'autorité d'une transaction = **signature ML-DSA valide** sur la préimage, **et**
  `lie(tx.from, clé_révélée)` vrai. **Les deux** sont requis. Si l'un échoue, la tx est
  **rejetée**.
- La clé révélée voyage dans la transaction (le champ `pq_public_key` existant devient la clé
  **liante**, plus une déclaration libre). `tx.from` est désormais une **adresse** (PQ-MIG-2),
  pas une clé brute.

## 2. La clé dans la préimage signée
- Inclus la clé publique (ou son adresse dérivée) dans les **octets signés**. Ainsi un attaquant
  ne peut pas garder une signature valide en substituant une autre clé : changer la clé change la
  préimage, donc invalide la signature. **Recompute/vérifie** la cohérence à la réception.

## 3. Retrait du repli Ed25519 + câblage vault
- Retire le chemin Ed25519 de l'**autorité de transaction** (`verify_hybrid` → vérification
  ML-DSA pure). Retire `REQUIRE_PQ=false` / le repli.
- Câble `create_pq_identity` / `unlock_pq_identity` (PQ-MIG-1) en **production** ; l'attribut
  `#[allow(dead_code)]` doit **disparaître**.
- **§4** : Ed25519 peut encore exister pour le **transport** (gossip) à ce stade, c'est hors
  autorité de transaction et différé (conception §6). Ne touche au transport que si nécessaire,
  et alors **signale**.

## 4. Les dents (la fermeture de la faille, obligatoire)
Le test central, celui qui prouve que CRYPTO-ID-1 est fermé :
- **clé non liée rejetée** : une transaction signée par une clé ML-DSA valide **mais dont
  l'adresse dérivée ≠ `tx.from`** est **REJETÉE**. C'est l'attaque exacte de l'audit (attacher
  sa propre clé à un compte étranger).
Plus :
- **substitution de clé rejetée** : prendre une tx valide, remplacer la clé (et donc invalider la
  préimage) ⇒ **rejetée**.
- **signature ML-DSA invalide rejetée**.
- **chemin nominal accepté** : une tx dont la clé hashe vers `from` et dont la signature ML-DSA
  est valide est **acceptée**.
- **plus aucun repli Ed25519** : une tx présentant seulement une signature Ed25519 est
  **rejetée** (le repli est mort).

## Garde-fous
- **Indivisibilité** : les 4 changements ensemble ; ne pas livrer un état où `verify_tx` est
  ML-DSA mais le repli Ed25519 subsiste, ou inversement.
- **Diff logique seule** ; pas de nightly-fmt fichier entier ; `dispatcher.rs` intact.
- **Déterminisme** : vérification pure ; signage ML-DSA déterministe en sim (SIGN-DET) ; `src/sm/`
  sans-IO ; **C1 vert**.
- **Couverture toujours verte** : `verify_tx` change, mais la validation de couverture (COVER-1/2)
  et la conservation doivent rester vertes (les tests existants ne régressent pas).
- **Pas de masquage** : la faille se ferme **réellement** ; le test « clé non liée rejetée » doit
  vraiment mordre, pas être contourné.
- **§4** : transport Ed25519 différé (ne pas y toucher sauf nécessité signalée) ; ne pas
  re-clé l'enjeu (PQ-MIG-4) ni refaire la genèse (PQ-MIG-5).
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant les 5 tests du §4, **surtout** « clé non liée rejetée ».
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert** ·
  **sweep + couverture + conservation verts**.
- **`git diff` logique seule** · `dispatcher.rs` intact · l'échafaudage `#[allow(dead_code)]` du
  vault PQ (PQ-MIG-1) **supprimé** (câblé en prod).
- Entrée **PQ-MIG-3** au tracker + auto-revue §3, notant : `verify_tx` ML-DSA, la liaison
  clé↔adresse, la clé dans la préimage, le repli Ed25519 retiré, le vault câblé, et **la preuve
  que la faille CRYPTO-ID-1 est fermée** (le test des dents).

## Séquence
1. **§2** clé publique dans la préimage signée (avant de changer la vérification).
2. **§1** `verify_tx` en ML-DSA + exigence de liaison `lie(from, clé)`.
3. **§3** retrait du repli Ed25519, câblage du vault PQ en production.
4. **§4** les dents, dont « clé non liée rejetée ».

> Après PQ-MIG-3, la **racine de confiance est post-quantique** et la faille de CRYPTO-ID-1 est
> close : ta promesse devient vraie sur l'autorité des comptes. Pièce suivante, **PQ-MIG-4** :
> re-clé l'enjeu (`staked`/`validator_stakes`) sur la nouvelle identité ML-DSA, ce qui fait
> coïncider identité d'enjeu et de finalité et **débloque GADGET-3**. Puis PQ-MIG-5 (genèse PQ).
