---
type: task-spec
id: GADGET-4
status: à exécuter (pièce 4 du gadget ; construit sur GADGET-3)
priorité: 🔴 slashing — détecter les deux fautes ; rend la sûreté RESPONSABLE
classe: détection des deux conditions (double vote, surround) à partir des votes signés ML-DSA
origine: [[DESIGN-FINALITY-GADGET]] §7 · [[ADR-003 — Slashing]] (en découle) · construit sur GADGET-2 (votes) + GADGET-3 (finalité)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[QUANTA_GADGET_PIECE3]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# GADGET-4 : slashing — détecter les deux fautes (sûreté responsable)

> Pièce 4. La sûreté responsable repose sur **deux** fautes, et deux seulement. On les **détecte**
> à partir des votes signés ML-DSA (GADGET-2), ce qui fournit une **preuve** non répudiable. C'est
> ADR-003, et il *découle* du théorème, on n'invente pas de règles. Ici on fait la **détection +
> la preuve** ; l'application de la pénalité (réduire l'enjeu) est cadrée mais ses **montants** sont
> 🛑 à toi. Diff logique seule, déterministe, C1 vert. Pas de fork-choice (GADGET-5).

## Les deux conditions (rappel)
1. **Double vote** : un validateur signe **deux votes différents pour la même époque cible**.
2. **Surround** : un validateur signe un vote dont l'intervalle (source, cible) **en entoure** un
   autre qu'il a déjà signé (source antérieure **et** cible postérieure).

## 1. Détection (pure, sur les votes)
- `detecte_faute(vote_a, vote_b) -> Option<Faute>` pour un **même** validateur :
  - **DoubleVote** si `epoque_cible(a) == epoque_cible(b)` et `a != b`.
  - **Surround** si `epoque_source(a) < epoque_source(b)` et `epoque_cible(b) < epoque_cible(a)`
    (ou symétrique).
  - sinon `None`.
- Fonction **pure**, déterministe. Les deux votes portent la **preuve** (signatures ML-DSA
  valides du même validateur).

## 2. La preuve de faute
- Une **PreuveDeFaute** = les deux votes signés contradictoires. Elle est **vérifiable par
  quiconque** : signatures ML-DSA valides + même validateur + l'une des deux conditions remplie.
- `verifie_preuve(preuve) -> bool` : rejette si les signatures ne valident pas, si ce n'est pas le
  même validateur, ou si aucune condition n'est remplie (pas de fausse accusation).

## 3. Application de la pénalité (cadrée, montants à toi)
- Une preuve valide rend le validateur **slashable** : son enjeu (`staked`, identité ML-DSA) est
  **réduit**. **🛑 décisions d'Alexandre (marquées, réglables)** : le **montant** slashé (fraction
  de l'enjeu), et la **fenêtre** pendant laquelle la preuve est recevable (qui doit rester ≤
  `UNBONDING_PERIOD_BLOCKS`, sinon le validateur retire son enjeu avant punition — contrainte déjà
  posée en ONCHAIN-STAKE-1).
- Implémente la **mécanique** de réduction (déterministe, conservation préservée : l'enjeu slashé
  est brûlé ou redistribué — **🛑 lequel, à toi** ; défaut proposé : **brûlé**, le plus simple et
  le plus sain monétairement). Laisse les montants en **constantes marquées**.

## 4. Lien avec la sûreté responsable (le test qui prouve le théorème)
- **Test clé** : si deux points de contrôle en **conflit** sont finalisés (situation injectée via
  un comité byzantin qui double-vote ou entoure), alors une **PreuveDeFaute** existe et couvre
  **≥ ⅓** de l'enjeu. C'est la sûreté responsable rendue exécutable : casser la finalité **laisse
  une preuve** d'au moins un tiers fautif.

## 5. Les dents (obligatoire)
- **double vote détecté + prouvé** ; **surround détecté + prouvé**.
- **pas de fausse accusation** : deux votes **légaux** (même source, ou cibles d'époques
  différentes sans entourage) ⇒ `detecte_faute` = `None`, `verifie_preuve` = faux.
- **preuve forgée rejetée** : une preuve dont une signature ML-DSA est invalide ⇒ rejetée.
- **sûreté responsable** (§4) : conflit finalisé injecté ⇒ preuve couvrant ≥ ⅓.
- **pénalité** : une preuve valide réduit l'enjeu du fautif ; **conservation préservée** (brûlé
  par défaut) ; déterminisme C1.

## Garde-fous
- Réutiliser les votes/certificats de GADGET-2 et la finalité de GADGET-3. Ne pas redéfinir.
- **Pas** de fork-choice (GADGET-5). **§4 STOP** s'il semble requis.
- **Diff logique seule** ; `dispatcher.rs` intact ; pas de nightly-fmt fichier entier.
- **Déterminisme** : détection/preuve/pénalité pures ; `src/sm/` sans-IO ; **C1 vert**.
- **Conservation préservée** : l'enjeu slashé est brûlé (défaut) ou redistribué, jamais perdu du
  bilan.
- **§4 montants à Alexandre** : montant slashé, fenêtre (≤ unbonding), brûlé vs redistribué —
  **constantes marquées**, ne pas figer en dur sans signaler.
- **Pas de masquage** : les dents §5, surtout la sûreté responsable, mordent.
- **Snapshot git** avant.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant les dents §5 (surtout sûreté responsable ≥ ⅓ et pas de
  fausse accusation).
- `clippy --lib -D warnings` propre · **C1 vert** · sweep + couverture + conservation verts ·
  `src/sm/` sans-IO.
- `git diff` logique seule · `dispatcher.rs` intact · invariants de finalité (GADGET-1/3) verts.
- Entrée GADGET-4 au tracker + auto-revue §3 (détection, preuve, pénalité, sûreté responsable, et
  les montants marqués 🛑).

## Séquence
1. **§1** détection des deux conditions (pure).
2. **§2** preuve de faute vérifiable (anti-fausse-accusation).
3. **§3** mécanique de pénalité (conservation, montants marqués).
4. **§4/§5** test de sûreté responsable (≥ ⅓) + dents.

> Après GADGET-4, la sûreté est **responsable** : toute violation de finalité laisse une preuve
> d'au moins un tiers fautif. Restent **GADGET-5** (fork-choice conscient de la finalité, résout la
> partition), **PQ-MIG-5** (genèse PQ), et la réconciliation clé-de-vote ↔ clé-d'enjeu avant
> câblage vivant. Tes montants de slashing (§3) sont à fixer au §12.
