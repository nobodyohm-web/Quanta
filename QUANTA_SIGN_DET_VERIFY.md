---
type: task-spec
id: SIGN-DET-VERIFY
status: à exécuter
priorité: vérification sécurité (à clore avant la suite)
classe: gating de la signature déterministe + borne temporelle de tx
suite de: T0.8-HARDEN (clos) · revue senior post-T0.8-HARDEN
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[QUANTA_T0_8_HARDEN]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# SIGN-DET-VERIFY — la signature déterministe ne doit pas pouvoir atteindre la prod

> Tâche de **vérification**, pas de feature. **Audit et rapport d'abord.** Ne touche au
> code que si un trou réel est trouvé. Si tout est déjà sûr, le livrable est une
> **confirmation documentée** plus, au besoin, les **tests qui épinglent** les deux modes.
> Diff logique seule.

## Contexte

T0.8-HARDEN a introduit un chemin de signature **déterministe** pour la sim
(`ml_dsa_sign_deterministic` / `sign_hybrid_det`, commuté par `det_sign`), à côté du
chemin **hedged** (OsRng) de la prod. Le hedged protège contre les attaques par faute ;
le déterministe y est plus exposé. Le risque : que `det_sign=true` (le chemin faible)
puisse atteindre un build de prod, ce qui serait un **downgrade silencieux** de la
résistance post-quantique. La revue a aussi relevé le retrait d'un self-check ±5min ; il
faut confirmer qu'il n'était pas une borne de validation.

## Vérification A — `det_sign` est-il inatteignable depuis la prod ?

1. **Audit du plumbing** : `det_sign` est-il un champ de struct, un paramètre, une const,
   un `cfg`, une feature ? Où est-il mis `true` (sim/test) et `false` (prod) ? Reporte le
   chemin exact.
2. **Reachability** : un chemin de **prod** (build release, hors `#[cfg(test)]`) peut-il
   construire ou appeler avec `det_sign=true` ?
   - **Cas idéal** : le chemin déterministe est `#[cfg(test)]` ⇒ **physiquement absent** du
     binaire release. Confirme et c'est clos.
   - **Cas mou** : `bool` runtime à `false` par défaut, mis `true` seulement dans des
     modules de test. La fonction déterministe **compile** alors dans le release et serait
     atteignable si un code de prod la mettait `true`. **Durcis** : gate la fonction
     déterministe derrière `#[cfg(test)]` (préféré), ou une **feature off par défaut**, ou
     au minimum un `debug_assert!`/garde de construction.
   - **Cas pire** : la prod peut réellement la basculer (option de config exposée) ⇒ **trou
     réel**, retire ce chemin de prod.
3. **Épingler les deux modes (les dents sécurité)** :
   - `prod_signing_is_hedged` : signer **deux fois** le même message par le chemin **prod**
     ⇒ signatures **différentes** (l'aléa hedged est bien actif en prod). Si elles sont
     identiques, la prod est devenue déterministe par accident ⇒ trou réel.
   - `sim_signing_is_deterministic` : signer deux fois par le chemin **sim** ⇒ signatures
     **identiques** (ce que le déterminisme du harnais exige ; C1 en dépend).
   - Les deux ensemble prouvent que le commutateur fait ce qu'il prétend **et** que la prod
     est du bon côté.
4. **Note de couverture** : la sim signe désormais en déterministe, donc le **mixage
   d'entropie hedged** est le seul bout que la prod fait et que la sim n'exerce pas.
   L'acter dans le tracker (gap mineur et localisé, acceptable).

## Vérification B — le self-check ±5min retiré

1. **Confirme** qu'il était purement **côté création** (vérifier mon propre horodatage que
   je viens de poser, donc inutile) et **pas** la seule borne de validation temporelle.
2. **Corollaire à acter** : « validation sans horloge » signifie qu'aucune borne ne
   contraint le `ts` d'une tx au moment de valider, donc un nœud peut **antidater /
   postdater** librement sans rejet. Confirme que c'était **déjà** le cas avant le retrait
   (donc pas de régression), ou que le retrait a **créé** ce gap.
3. **§4 (règle d'arrêt)** : si le retrait a **créé** un gap (il existait une borne, elle
   n'existe plus), **STOP et remonte**. Faut-il une borne de validité temporelle des tx
   (façon median-time-past) est un **choix de consensus**, pas à inventer par l'agent.

## Garde-fous
- **Audit d'abord, rapport ensuite.** Code seulement pour un trou identifié, pas de churn,
  pas de test fabriqué pour « faire un livrable ».
- **Diff logique seule** · pas de nightly-fmt sur fichiers entiers · `dispatcher.rs` intact.
- **Prod inchangée** sauf si A révèle un vrai trou (alors le durcissement **renforce** la
  prod, ne l'affaiblit pas).
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant `prod_signing_is_hedged` et
  `sim_signing_is_deterministic` (ajoutés s'ils manquent).
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert**.
- **`git diff` logique seule** · `dispatcher.rs` intact.
- Entrée **SIGN-DET-VERIFY** au tracker (avec auto-revue §3) reportant explicitement :
  - A : comment `det_sign` est gardé, s'il était déjà inatteignable depuis la prod ou s'il
    a fallu durcir (et comment), et le résultat des deux tests d'épinglage.
  - B : le ±5min était-il création-seule, le gap d'horodatage existait-il déjà, et le cas
    échéant l'escalade §4.

## Séquence
1. **Audit A + B**, reporter les constats avant toute modif.
2. **A** : durcir le gating **seulement** si nécessaire, ajouter les deux tests d'épinglage.
3. **B** : confirmer/escalader.
4. Mettre à jour le tracker.

> **Hors scope.** Le choix d'une borne temporelle de validation (si B révèle un gap) est
> une décision d'Alexandre, pas cette tâche. Commit d'un baseline git propre = manuel,
> Alexandre.
