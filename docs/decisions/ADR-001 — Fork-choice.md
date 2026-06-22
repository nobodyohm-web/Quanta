---
type: adr
id: ADR-001
status: open
decision-class: défaut-réversible (puis 🛑 à la finalité)
updated: 2026-06-21
---

# ADR-001 — Fork-choice

← [[README|Registre ADR]] · cadre : [[DESIGN-CONSENSUS-DAG-BFT]]

> [!success] CASCADE (2026-06-21) — résolu par le finality gadget (Option 1)
> Le périmètre retenu est le **finality gadget** : un bloc **finalisé** par vote
> BFT n'est **jamais** réorganisé → le fork-choice ne s'applique plus qu'à la
> **fenêtre non-finalisée** (proposition → finalité, ~secondes). Donc :
> - **Plus de reorg multi-blocs à inventer** : la finalité tronque la fenêtre.
> - **Sous-choix restant (intérim, dans la fenêtre)** : remplacer le « hash le plus
>   haut » (grindable **sans** stake) par un départage **pondéré stake on-chain**
>   (cohérent avec [[ADR-002 — Validator set & comité BFT]] : stake seul). Petit,
>   Sybil-ancré, suffisant tant que la finalité suit vite.
> - **Harnais** : les tests de convergence n'exigent l'accord que jusqu'à la
>   **profondeur de finalité**, pas sur la fenêtre vive.

## Contexte (code réel, pas la doc)
`Ledger::integrate_remote_block` (`src-tauri/src/p2p/ledger.rs`) implémente
**uniquement** :
- **extension** : bloc à `tip.index+1` avec `prev_hash == tip.hash` → intègre ;
- **doublon** : hash connu → no-op ;
- **fork à hauteur égale** : même `index`, hash différent → **départage par hash
  lexicographique le plus haut** ; si le distant gagne, pop du tip, **re-queue
  des tx perdantes** (AUDIT-BLK-1), validation **avant** mutation (AUDIT-BLK-2) ;
- hors plage → `Err`.

> [!warning] Drift doc ↔ code
> [[DESIGN-CONSENSUS-DAG-BFT]] dit « plus-longue-chaîne ». **Faux** : le code ne
> fait **pas** de reorg multi-blocs. C'est un **départage mono-bloc à hauteur
> égale** — un placeholder PoC. Au-delà d'1 bloc de divergence, rien ne réconcilie.

## Ce que le simulateur (T0.4) force
Les assertions de **convergence n-nœuds** dépendent entièrement de cette règle.
Un départage trop faible → deux nœuds honnêtes peuvent rester divergents sur >1
bloc, et le test de convergence échoue (ou pire, passe en masquant le trou).

## Options
1. **Garder le départage par hash (PoC)** — *défaut réversible.*
   - + zéro travail, déterministe, suffisant pour le harnais mono-machine.
   - − pas de réconciliation au-delà d'1 bloc ; « hash le plus haut » n'a aucune
     sémantique économique (n'importe qui peut grinder un hash plus haut **sans**
     stake — couplage fort avec [[ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)]]).
2. **Heaviest-stake / plus-longue-chaîne pondérée** — réorg vers la branche au
   **poids de stake cumulé** le plus élevé.
   - + sémantique Sybil-résistante (aligné PoS) ; réconcilie sur N blocs.
   - − vraie logique de reorg multi-blocs à écrire + tester (chaos).
3. **Rendre le point moot via le finality gadget** (cible [[DESIGN-CONSENSUS-DAG-BFT]] Option 1) —
   un bloc **finalisé** par vote BFT n'est **jamais** réorganisable → fork-choice
   ne s'applique qu'**avant** finalité (fenêtre courte).
   - + résout le problème à la racine (finalité déterministe).
   - − dépend du comité ([[ADR-002 — Validator set & comité BFT]]) + Phase 1.

## Contraintes croisées
- « Hash le plus haut » est **grindable sans stake** → à corriger ensemble avec
  [[ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)]].
- La cible est de toute façon le **finality gadget** → option 1 vs 2 n'est qu'un
  **intérim**.

## Statut & recommandation
**Défaut réversible proposé** : garder l'option **1** (départage par hash) **pour
le harnais Phase 0**, MAIS écrire les tests de convergence pour qu'ils n'exigent
la convergence **que jusqu'à la profondeur de finalité** prévue. La **vraie**
décision (heaviest-stake intérimaire vs aller droit au finality gadget) est un
**🛑** lié à [[ADR-002 — Validator set & comité BFT]] et au périmètre (§7 du design).

**À trancher par toi** : intérim heaviest-stake (option 2) maintenant, ou sauter
direct au finality gadget (option 3) ?
