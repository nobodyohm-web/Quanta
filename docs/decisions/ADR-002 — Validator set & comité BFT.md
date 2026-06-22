---
type: adr
id: ADR-002
status: accepted
decision-class: 🛑 hard-stop
decided: 2026-06-21
updated: 2026-06-21
---

# ADR-002 — Validator set & comité BFT

← [[README|Registre ADR]] · cadre : [[DESIGN-CONSENSUS-DAG-BFT]] (problème dur #1)

> [!success] DÉCISION (2026-06-21) — **Stake on-chain seul** (option 1, variante pure)
> La sécurité (éligibilité + poids BFT) est ancrée au **stake gravé dans le
> ledger**, dérivé déterministiquement à une frontière d'**epoch** → tous les
> nœuds calculent le **même** comité (objet de consensus). La **réputation sort
> du chemin de sécurité** : elle reste un signal **applicatif** (mining / Shapley),
> jamais un poids d'élection/finalité.
>
> **Ce que ça impose (travail dérivé) :**
> - `Validator::weight()` → **`= stake` pur** (retirer le bonus réputation `min(rep×10_000, stake)`).
> - `build_validator_set` → source = **snapshot de stake on-chain par epoch**, plus `reputation.get_leaderboard`.
> - Nouvelles tx **`Stake` / `Unstake`** (staking explicite on-chain) + comptabilité du stake dans le ledger.
> - Découpler `reputation.rs` du consensus ; ajuster [[TOKENOMICS_V2]] / `shapley.rs` (réputation = récompense, pas pouvoir).
> - Quorum BFT = **2f+1 pondéré stake** ; `f` défini en **stake**, pas en nombre de nœuds.
>
> **Sous-décisions encore ouvertes** (implémentation, à fixer avant le code) :
> longueur d'**epoch** ; format des tx `Stake`/`Unstake` (verrou, délai d'unbonding) ;
> `MIN_VALIDATOR_STAKE` conservé à 1 QUANTA ?

## Contexte (code réel)
- `pos_consensus::build_validator_set(stakes, reputations)` construit l'ensemble
  depuis `reputation.get_leaderboard(100)` — **vue locale** de chaque nœud.
- Poids = `stake + min(reputation × 10_000, stake)` (bonus réputation **plafonné
  au stake** → élection ancrée au stake). `MIN_VALIDATOR_STAKE = 1 QUANTA`.
- L'élection (`elect_leader`) est déterministe **pour un ensemble donné**, triée
  par `pk` (permutation-invariante, prouvée par property-test).
- **C7** : le cœur `Node::propose_block_at(now_ms, validators)` prend l'ensemble
  en **paramètre injecté** — exprès, pour ne pas trancher ici.

> [!danger] Le vrai trou
> Chaque nœud construit l'ensemble depuis **sa** réputation/`leaderboard` locale.
> Rien ne garantit que deux nœuds calculent le **même** ensemble → ils peuvent
> élire des leaders **différents** au même slot → **fork**. L'élection est
> déterministe *par ensemble*, mais **l'ensemble n'est pas un objet de consensus**.

## Ce que le simulateur force
Le paramètre injecté de `propose_block_at` doit se **résoudre** en une source
réelle. Pour tester la convergence, tous les nœuds doivent dériver un ensemble
**identique** d'une **même** donnée on-chain.

## Options
1. **Snapshot on-chain par epoch** *(recommandé comme base BFT)* — l'ensemble est
   dérivé **déterministiquement** des stakes **gravés dans le ledger** à une
   frontière d'epoch (`height % EPOCH == 0`). Tous les nœuds à la même hauteur →
   même ensemble.
   - + objet de consensus (déterministe, vérifiable, Sybil-ancré au stake on-chain).
   - − exige un **staking on-chain** explicite (tx `Stake`/`Unstake`) ; la
     réputation (off-chain, locale) ne peut **pas** entrer dans le poids BFT sans
     être elle-même on-chain → revoir le rôle de la réputation.
2. **Vue locale courante (statu quo)** — chaque nœud son `leaderboard`.
   - + zéro changement.
   - − **non-consensuel** → forks ; inacceptable pour du BFT.
3. **Hybride** — stake on-chain pour l'éligibilité/poids BFT (consensuel), la
   réputation reste un signal **applicatif** (mining/Shapley), hors poids BFT.

## Contraintes croisées
- Définit **qui peut finaliser** → préalable à [[ADR-003 — Slashing (accountable safety)]]
  (on ne slashe que des membres du comité) et au finality gadget.
- Quorum BFT = **2f+1 pondéré par stake** (design #1) → `f` défini en **stake**,
  pas en nombre de nœuds.
- Sort la **réputation** du chemin de sécurité si on prend l'option 1/3 → impacte
  [[TOKENOMICS_V2]] et `shapley.rs`.

## Statut & ce dont j'ai besoin de toi (🛑)
La réputation (énergie/contribution, **off-chain et locale**) doit-elle peser
dans l'**élection/finalité** (→ il faut la rendre on-chain et consensuelle), ou
on **ancre la sécurité au stake on-chain seul** et la réputation reste un signal
applicatif ? Et : **epoch length** + introduction des tx `Stake`/`Unstake` ?
