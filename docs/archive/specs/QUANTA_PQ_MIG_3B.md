---
type: task-spec
id: PQ-MIG-3B
status: à exécuter (revient à ADR-007 (b), reprend où PQ-MIG-3/Option-a s'est arrêté)
priorité: 🔴 re-keying complet de l'identité : from = adresse ML-DSA PARTOUT
classe: bascule identité de compte Ed25519 → adresse ML-DSA (compte, enjeu, validateur, récompense, @pseudo) ; transport Ed25519 différé
origine: ADR-007 (b) ré-affirmé ; PQ-MIG-3 avait fait (a), il faut compléter en (b)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[QUANTA_PQ_MIG_2]] (lie/adresse) · [[QUANTA_PQ_MIG_3]] (préimage, faille fermée) · [[AUDIT_QUANTA_2_PROGRESS]]
---

# PQ-MIG-3B : from = adresse ML-DSA partout (terminer le tout-PQ)

> ADR-007 (b) ré-affirmé : Quanta entièrement PQ, **sans astérisque**. PQ-MIG-3 a fait le chemin
> (a) (from reste Ed25519 + clé liée). On **complète** : `from` devient l'**adresse ML-DSA**
> (PQ-MIG-2), ce qui re-clé **du même geste** compte, enjeu, validateur, récompense, @pseudo. Le
> code de PQ-MIG-3 (préimage, fermeture CRYPTO-ID-1, vault câblé) **reste** ; on pousse l'identité
> jusqu'au bout. Diff logique seule, déterministe.

## Cadre
- **Fait** : `from`/`to` = adresse ML-DSA (`BLAKE3(ADDR_DOMAIN ‖ pk)`) **partout** : clé de solde,
  cible de récompense (`mine_tx`), `staked`/`validator_stakes`, `@pseudo`. Identité unifiée → tout
  bascule ensemble (c'est ce que l'agent avait identifié, c'est voulu).
- **Différé (§4, ne pas toucher)** : **transport** (PeerId / enveloppes gossip) reste Ed25519,
  éphémère, hors chemin de valeur (conception §6).
- **Documentation** : **ADR-008 (qui actait l'astérisque (a)) est reversé/réécrit** pour rétablir
  ADR-007 (b). Note-le au tracker.

## 1. `from`/`to` = adresse ML-DSA
- Partout où l'identifiant de compte est `tx.from`/`tx.to` (solde, récompense, enjeu, validateur,
  pseudo), c'est désormais l'**adresse ML-DSA**. La clé révélée + `lie(from, clé)` (PQ-MIG-2)
  restent la règle d'autorité de `verify_tx` (déjà posée en PQ-MIG-3).
- Récompense : `mine_tx` crédite l'**adresse ML-DSA** du proposeur.
- Enjeu : `staked` / `validator_stakes()` indexés par l'**adresse ML-DSA**.

## 2. Cohérence (les invariants à ne pas casser)
- **Conservation** et **couverture** (COVER-1/2) restent vertes : on change la clé d'indexation,
  pas la valeur. Les tests existants ne régressent pas.
- **Déterminisme** : adresse pure (BLAKE3), signage ML-DSA déterministe en sim ; **C1 vert**.

## 3. Les dents (obligatoire)
- **faille toujours fermée** : la clé révélée ne hashant pas vers `from` ⇒ tx **rejetée** (le test
  de PQ-MIG-3 reste vert).
- **enjeu re-clé** : un Stake d'une adresse ML-DSA crédite l'enjeu **sous cette adresse** ; `staked`
  et `validator_stakes()` sont indexés par adresse ML-DSA (test direct).
- **récompense re-clé** : `mine_tx` crédite l'adresse ML-DSA, le solde est lisible sous cette
  adresse.
- **conservation/couverture** : un cycle complet (mine → transfer → stake) conserve, sur identité
  ML-DSA.

## Garde-fous
- **Transport NON touché** (Ed25519, différé) ; **§4 STOP** si le transport semble devoir bouger.
- Réutiliser `ml_dsa_address*` / `lie()` (PQ-MIG-2) ; ne pas redéfinir l'adresse.
- **Diff logique seule** ; `dispatcher.rs` intact ; pas de nightly-fmt fichier entier.
- **C1 vert**, sweep + couverture + conservation verts, `src/sm/` sans-IO.
- **Pas de masquage** ; les dents du §3 mordent.
- **Snapshot git** avant.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant les dents §3 (faille fermée + enjeu/récompense re-clé +
  conservation).
- `clippy --lib -D warnings` propre · **C1 vert** · sweep + couverture + conservation verts ·
  `src/sm/` sans-IO.
- `git diff` logique seule · `dispatcher.rs` intact · **transport Ed25519 inchangé**.
- **ADR-008 reversé/réécrit** (rétablit ADR-007 (b)) + entrée PQ-MIG-3B au tracker + auto-revue §3.

## Séquence
1. `from`/`to` = adresse ML-DSA dans solde, récompense, enjeu, validateur, pseudo.
2. Vérifier conservation/couverture/C1 verts.
3. Dents : faille fermée, enjeu re-clé, récompense re-clé, cycle conserve.
4. Reverser/réécrire ADR-008.

> Après PQ-MIG-3B, l'identité de compte est **entièrement ML-DSA** (transport excepté, différé), et
> identité d'enjeu = identité de finalité → **GADGET-3 débloqué**. Reste PQ-MIG-5 (genèse PQ) et,
> si voulu un jour, le transport PQ.
