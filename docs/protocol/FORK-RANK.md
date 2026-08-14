# FORK-RANK-1 — le départage de fork cesse d'être gratuit

**Statut** : implémenté (2026-08-14) · **Ferme** C-04 et une partie de C-03 de l'audit externe du
13/08/2026 · **Fichiers** `p2p/pos_consensus.rs`, `p2p/ledger/reorg.rs`, `p2p/fork_heal.rs`

---

## 1. Ce qui n'allait pas

Deux blocs valides à la même hauteur, il faut en choisir un. La règle était :

```rust
if block.hash > tip.hash { /* le distant gagne */ }
```

Trois propriétés de cette règle, prises ensemble, sont fatales :

1. **Elle est gratuite.** Le `timestamp` d'un bloc entre dans son hash et n'était borné nulle part
   (c'était le constat C-02, corrigé depuis). Un proposeur essayait donc des horodatages jusqu'à
   obtenir un hash supérieur — quelques milliers de BLAKE3, soit un coût indiscernable de zéro.
2. **Elle ne consulte pas l'enjeu.** Gagner un départage ne demandait rien à perdre.
3. **L'élection pondérée par l'enjeu existait déjà** (`pos_consensus::elect_leader`, ancrée sur un
   beacon *enterré* de deux hauteurs, permutation-invariante, arithmétique u128 exacte) — mais elle
   ne servait **qu'au scellement**. À la réception, elle n'existait pas.

C'est le motif dominant de tout l'audit : *le projet vérifiait très bien ce qu'il avait décidé de
vérifier.* Ici, il avait décidé de vérifier le scellement.

Conséquence pratique : combiné à OPEN-DOOR-1 (un bloc sur seize proposable par n'importe qui), un
attaquant sans enjeu réécrivait à volonté un bloc sur seize, et un validateur bondé quelconque
réécrivait n'importe quel bloc en broyant un peu plus longtemps que son voisin.

## 2. La règle qui remplace

Le départage devient un ordre lexicographique sur :

```
(rang d'élection du proposeur, hash du bloc)
```

Le **rang d'élection** est la position du proposeur dans un classement total de l'ensemble bondé
*as-of-parent*, obtenu par tirages pondérés par l'enjeu **sans remise** sur la graine de
`elect_leader`. Rang 0 = leader élu de la hauteur. Plus petit = mieux élu = préféré.

```
rank(bloc) = election_rank_of(
    beacon  = leader_beacon(chain[h - LEADER_ENTROPY_LOOKBACK].hash, h),
    slot    = h,
    valset  = { v : stake_as_of_parent(v) >= MIN_VALIDATOR_STAKE },
    who     = bloc.miner
)
```

Le hash ne tranche plus que ce que le rang laisse ex æquo.

### Pourquoi ça ferme le trou

Le rang d'un bloc ne dépend que de trois choses : le hash d'un bloc **enterré**, la hauteur, et
l'ensemble bondé chez le parent. **Aucune des trois n'est dans le bloc concurrent.** Broyer le
contenu, l'ordre des transactions ou l'horodatage ne déplace donc pas le rang d'un cran. Pour gagner
un départage, il faut être mieux élu ; pour être mieux élu, il faut de l'enjeu ; l'enjeu est bondé,
donc slashable. Le coût de l'attaque passe de « quelques milliers de hashs » à « du capital exposé ».

### Le cas ex æquo est exactement le cas punissable

Deux blocs de même rang à la même hauteur = **même proposeur**, donc auto-équivocation. C'est
précisément ce que le gadget de finalité slashe (GADGET-4). Le broyage n'achète alors qu'une faute
prouvable : le seul résidu de l'ancienne règle est un chemin vers sa propre punition.

## 3. Ce qu'on a refusé de faire, et pourquoi

### Pas de VRF

La vraie primitive pour ce problème est une VRF (fonction pseudo-aléatoire vérifiable) : chaque
validateur prouve son ticket sans que personne ne puisse le prédire. Deux raisons de ne pas y aller :

- **ML-DSA n'est pas une VRF.** Elle n'a pas d'unicité : plusieurs signatures valides existent pour
  un même message et une même clé, donc `H(signature)` se broie. Il n'existe pas aujourd'hui de VRF
  post-quantique déployable, et le projet est post-quantique par contrainte, pas par goût.
- Le tirage à graine publique donne déjà la propriété qui manquait — *l'enjeu décide* — au prix de la
  **prédictibilité** (tout le monde sait qui est élu). C'est un vrai coût, nommé au §5.

### Pas d'horloge

Le projet a fait le choix explicite (« Politique A ») d'un consensus **sans horloge de confiance** :
sans NTP, deux nœuds désynchronisés divergeraient sur la validité d'un même bloc, ce qui est un fork.
Le rang est donc un ordre statique, pas un tour de parole temporisé. Conséquence heureuse : si le
rang 0 est hors ligne, le rang 3 produit et gagne face au rang 7 — la liveness ne dépend d'aucun
délai d'attente.

### Le slot ouvert garde un départage neutre

Sur les slots OPEN-DOOR-1 (un sur seize), un proposeur non bondé n'a pas de rang et le hash décide.
C'est délibéré : y faire gagner systématiquement le mieux bondé rendrait la porte décorative et
refermerait le réseau au premier staker, ce que OPEN-DOOR-1 avait ouvert exprès. Un seizième des
blocs reste donc départageable par broyage — la même borne, déjà concédée au trilemme de
vulnérabilité Sybil, et verrouillée par un test.

## 4. Long-range : REORG-DEPTH-1 et GENESIS-ANCHOR-1

Le rang décide *à hauteur égale*. Il ne dit rien d'une branche **plus longue** reconstruite hors
ligne par un ancien validateur — qui n'a plus rien à perdre, son enjeu étant retiré. Deux bornes
complètent le dispositif :

- **`MAX_REORG_DEPTH = 128`** — aucune réorganisation de plus de 128 blocs, quel que soit son score
  et **indépendamment de la finalité**. Le plancher de finalité (LIVE-2) ne protège que ce que les
  votes ont atteint ; tant que personne ne vote il vaut 0. Le prix est nommé : au-delà, une partition
  ne guérit plus seule et demande une resynchronisation explicite. C'est le bon comportement — un
  reorg de deux heures est soit une attaque, soit un incident qui mérite un humain.
- **`GENESIS-ANCHOR-1`** — `verify_chain` vérifiait « bien chaînée », jamais « chaînée à *notre*
  genèse ». La seule chose qu'un attaquant ne peut pas refabriquer est le bloc 0, encore faut-il le
  comparer.

## 5. Ce qui reste ouvert

- **Grinding de la graine.** Le proposeur de `h - LEADER_ENTROPY_LOOKBACK` influence le beacon de
  `h` en choisissant le contenu de son bloc. Le lookback rend l'attaque non triviale (il faut
  contrôler des blocs consécutifs), il ne la supprime pas. La fermeture propre demande un VDF ou un
  RANDAO à révélation différée.
- **Prédictibilité.** L'élection étant publique, le leader de `h+1` est connu à `h` : il est ciblable
  (déni de service). Le fallback par rang limite le dommage (le rang 1 produit), il ne cache pas
  l'identité. C'est le prix explicite de l'absence de VRF post-quantique.
- **Nothing-at-stake au-delà de la borne.** REORG-DEPTH-1 borne le dommage, il ne rend pas le
  double-vote coûteux hors du périmètre du gadget de finalité.

**Tant que ces trois lignes existent, ce réseau ne doit porter aucune valeur.**

## 6. Tests qui verrouillent la règle

| test | propriété |
|---|---|
| `c04_rank0_is_exactly_elect_leader` | produire et vérifier ne peuvent pas diverger |
| `c04_election_ranking_is_a_permutation_of_the_eligible_set` | ordre total, sans doublon ni oubli |
| `c04_election_ranking_is_permutation_invariant` | pas de dépendance à l'ordre d'itération d'une `HashMap` (= pas de fork) |
| `c04_rank_is_weighted_by_stake_not_by_luck` | le rang se paie en enjeu |
| `c04_an_unbonded_address_is_never_ranked` | l'attaquant sans enjeu n'a aucun levier |
| `c04_ranking_depth_is_bounded` | le fork-choice reste O(1) en taille de réseau |
| `c04_a_ground_hash_no_longer_beats_a_better_elected_proposer` | **le PoC de l'audit, retourné** |
| `c04_the_fork_rank_is_identical_on_both_sides_of_a_fork` | convergence quel que soit l'ordre de réception |
| `c04_the_open_slot_keeps_a_neutral_tiebreak` | la borne concédée ne s'élargit pas |
| `c03_a_reorg_deeper_than_the_cap_is_refused_even_with_no_finality` | long-range fermé sans dépendre de la finalité |
| `c03_a_reorg_within_the_cap_still_heals` | la borne ne gèle pas le réseau |
| `c03_a_chain_rooted_on_a_foreign_genesis_is_refused` | l'ancre de genèse est vérifiée |
