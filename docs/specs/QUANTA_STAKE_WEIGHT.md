---
type: task-spec
id: STAKE-WEIGHT-1
status: à exécuter (prérequis du gadget, décidé, indépendant des paramètres §12)
priorité: 🟠 soundness — poids du consensus = enjeu on-chain seul (ADR-002), prérequis de GADGET-2
classe: implémenter ADR-002 dans le code (retirer la réputation du chemin de poids)
origine: [[ADR-002 — Validator set]] (accepté) + HARDEN-AUDIT-1 (« réputation locale comme poids »)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[DESIGN-FINALITY-GADGET]] §10 · [[AUDIT_QUANTA_2_PROGRESS]]
---

# STAKE-WEIGHT-1 : poids du comité = enjeu on-chain seul (implémenter ADR-002)

> **Audit d'abord, pas une feature.** ADR-002 a **déjà décidé** : le poids dans le consensus
> dérive de l'**enjeu inscrit sur la chaîne**, et la réputation (et toute mesure non-enjeu) est
> retirée du chemin de sécurité, réduite à un signal applicatif. Cette tâche **confirme** que le
> code le respecte, et le **corrige** sinon. C'est un **prérequis de GADGET-2** (le certificat
> d'époque mesure ⅔ de l'**enjeu**, ce qui n'a de sens que si le poids est l'enjeu seul) et la
> fermeture d'une **faille de soundness** (un poids mesuré localement fait diverger les nœuds).
> Indépendant des paramètres §12 (E, taille de comité, quorum). Diff logique seule, déterministe.

## 1. Audit (le fait, à rapporter)
Trace **comment le poids d'un validateur est calculé aujourd'hui** sur les chemins de consensus :
élection du leader/proposeur, et tout endroit qui pèsera dans le quorum. Le poids est-il déjà
l'**enjeu on-chain seul**, ou intègre-t-il encore de la **réputation**, du **Shapley**, ou toute
autre quantité **non-enjeu** ? Reporte avec preuves (où le poids est calculé, ce qui l'alimente,
`reputation.rs` / `shapley.rs` / autre).

## 2a. Si déjà enjeu-seul → confirmer et clore
Si le poids du consensus est déjà strictement l'enjeu on-chain, **confirme-le**, documente la
fermeture de l'écart ADR-002-vs-code, **aucun churn**. (Possible : ADR-002 peut déjà être
implémenté.)

## 2b. Si encore teinté de non-enjeu → convertir en enjeu-seul
- Le poids du consensus (élection, quorum) devient l'**enjeu on-chain seul**, calculé
  **déterministement depuis l'état de la chaîne**, donc identique sur tous les nœuds.
- La **réputation** (et le Shapley, s'il pesait dans le consensus) devient un **signal
  applicatif** sans **aucun** effet sur le poids, l'élection ou le quorum. Ne la supprime pas
  forcément du projet (elle peut servir ailleurs) ; **détache-la du chemin de sécurité**.
- **§4** : si une mesure non-enjeu s'avère **structurellement entremêlée** au consensus d'une
  façon qui rouvre un vrai choix de conception, **signale-le**, ne tranche pas ; mais le principe
  (enjeu seul) est **déjà décidé** par ADR-002, pas à rediscuter.

## 3. La propriété à vérifier : poids déterministe et identique entre nœuds
Le poids doit être une **fonction pure de l'état on-chain**, pas une quantité mesurée localement.
- **Test** : deux nœuds, même état de chaîne ⇒ **mêmes poids** pour le même comité (aucune
  divergence). C'est la propriété anti-fork au cœur d'ADR-002. Si testable dans le harnais
  (`sm/sim.rs`), ajoute-la là ; sinon un test unitaire déterministe.
- Si la conversion 2b a lieu, **prouve** que l'ancien chemin (réputation-pondéré) aurait fait
  diverger deux nœuds aux réputations locales différentes, et que le nouveau ne le fait plus
  (dents : raisonne-le ou démontre-le).

## Garde-fous
- **Diff logique seule** ; pas de nightly-fmt fichier entier ; `dispatcher.rs` intact.
- **Déterminisme** : le poids est pur, fonction de l'état ; `src/sm/` sans-IO préservé ; **C1
  vert**.
- **Pas de masquage** : si le code pondérait par réputation, c'est une **vraie** faille de
  soundness à corriger, pas à neutraliser par un test mou.
- **§4** : ne décide **ni** la taille de comité **ni** le quorum (ce sont les §12 d'Alexandre,
  GADGET-2). Cette tâche ne fait qu'assurer **poids = enjeu**.
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant le test de poids déterministe/identique du §3.
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert** ·
  **sweep par défaut vert**.
- **`git diff` logique seule** · `dispatcher.rs` intact.
- Entrée **STAKE-WEIGHT-1** au tracker + auto-revue §3, avec : **le constat d'audit** (le poids
  était-il déjà enjeu-seul, ou teinté de quoi), l'action (confirmation ou conversion), et la
  preuve de la propriété anti-divergence.

## Séquence
1. **§1** auditer le calcul du poids, rapporter.
2. **§2a** confirmer si déjà enjeu-seul, **sinon §2b** convertir.
3. **§3** test de poids déterministe et identique entre nœuds.

> Une fois ce prérequis posé, GADGET-2 (votes ML-DSA + certificat d'époque) pourra mesurer ⅔ de
> l'enjeu sur une base saine. GADGET-2 reste, lui, **en attente de ta validation de la conception
> et de tes décisions §12** (E, taille de comité, quorum). Ce spec-ci ne dépend d'**aucune** de
> ces décisions, c'est pour ça qu'il peut partir maintenant.
