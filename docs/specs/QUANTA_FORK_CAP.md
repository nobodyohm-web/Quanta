---
type: task-spec
id: FORK-CAP-1
status: à exécuter
priorité: 🔴 CRITIQUE — brèche du plafond 100M atteignable par un adversaire réseau
classe racine: deuxième chemin de validation divergent (fork-reorg saute validate_block_emission)
origine: HARDEN-AUDIT-1, trouvaille #1 (confirmée 2×)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[QUANTA_EMISSION_INTEGRITY]] (EMIT-1) · [[QUANTA_EMIT1_VERIFY]] · [[docs/decisions/ADR-001 — Fork-choice]]
---

# FORK-CAP-1 : fermer la brèche d'émission sur le chemin fork-reorg

> 🔴 CRITIQUE. L'audit a confirmé deux fois que la branche de validation fork-reorg **saute**
> `validate_block_emission`, donc un adversaire réseau peut faire **minter au-delà des 100M**
> via un bloc de reorg. C'est la seule propriété de sûreté monétaire qui ne doit **jamais**
> céder. On ferme le critique d'abord, on cartographie la classe ensuite. Diff logique seule.

## Contexte (pourquoi c'est la priorité absolue)

Le plafond de 100M est l'invariant racine de toute la thèse monétaire. EMIT-1 a posé la règle
de validation (≤1 récompense, `from==NETWORK`, `to==miner`, montant borné) **et** l'invariant
du harnais. Mais il existe un **second chemin de validation**, celui du fork-reorg, qui
**n'applique pas** cette règle. Distinction décisive : l'invariant du harnais **détecte** la
sur-émission en simulation, mais le nœud de **production accepte** le bloc fautif sur ce
chemin. Résultat réel : conservation cassée sur les vrais nœuds, plafond franchi.

## 1. Fermer la brèche (le critique, XS)

La branche fork-reorg DOIT appliquer **exactement** la même validation d'émission que le
chemin linéaire : `validate_block_emission` (≤1 tx de minage, `from==NETWORK`, `to==miner`,
montant dans la borne du plafond). Localise où le chemin linéaire l'appelle, localise la
branche fork-reorg qui l'omet, applique le même contrôle au même endroit logique.

## 2. Le prouver (test adverse, obligatoire)

Test qui construit un bloc de reorg **fautif d'un adversaire** et asserte qu'il est **REJETÉ
par la validation** sur la branche reorg, pas seulement signalé par l'invariant après coup.
Couvre les trois formes d'abus :
- (a) plus d'une tx de minage dans le bloc de reorg,
- (b) une récompense **sur-dimensionnée** poussant la masse au-delà du plafond,
- (c) `to != miner`.

Dans les trois cas, le bloc est **rejeté** et la masse en circulation ne dépasse **jamais**
100M.

Et, parce que le harnais ne l'a **pas** attrapé (l'archétype byzantin a été rendu monotone et
convergent, donc il ne produit jamais d'over-mint), **étends l'archétype byzantin du sweep**
pour qu'il **puisse produire** un bloc de reorg sur-émetteur. Ainsi l'invariant d'émission
**et** le nouveau rejet de validation sont exercés en continu sur ce chemin. Tant que le sweep
ne sait pas générer l'attaque, il reste aveugle à la classe.

## 3. Cartographier la classe (audit dans le spec, sans tout corriger)

« Deuxième chemin de validation divergent » est une **classe racine** identifiée par l'audit.
FORK-CAP n'en ferme qu'**un** symptôme, l'émission. Énumère donc **chaque** contrôle que la
validation linéaire applique, et confirme un par un si la branche fork-reorg l'applique aussi
(signature, structure, ordre, antériorité du parent, hash, etc.). **Reporte** toute autre
divergence comme trouvaille à spec-er séparément. Ne corrige ici **que** l'émission ; le reste
est cartographié, pas rustiné.

- **§4, arrêt** : si une divergence relève d'un **choix de conception** (le reorg validerait
  délibérément autrement, territoire fork-choice [[ADR-001]]), **signale-la**, ne tranche pas.
- Le correctif **durable** de la classe (converger les deux chemins en **une seule** fonction
  de validation appelée par les deux) sera son propre spec, après cette cartographie.

## Garde-fous

- **Diff logique seule**, pas de nightly-fmt sur fichier entier, `dispatcher.rs` intact.
- **Pas de masquage** : le bloc fautif doit être **rejeté** par la validation, jamais filtré
  ailleurs ni toléré par un test mou.
- **Snapshot git** avant de commencer (le baseline est en place, le filet existe).

## Porte d'acceptation

- `cargo test --lib` **vert**, incluant le test adverse de rejet (les trois formes a/b/c) et
  l'extension de l'archétype byzantin.
- Le **sweep par défaut reste vert** avec l'archétype sur-émetteur désormais disponible : la
  validation rejette le bloc, donc **zéro violation persistante** au balayage.
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert**.
- **`git diff` logique seule** · `dispatcher.rs` intact.
- Entrée **FORK-CAP** au tracker + auto-revue §3, **avec la cartographie de la classe** :
  quels contrôles la branche reorg applique, lesquels elle omet, et lesquels relèvent du §4.

## Séquence

1. **§1** fermer la brèche (le critique).
2. **§2** test adverse des trois formes + extension du sweep.
3. **§3** cartographier la classe, reporter les autres divergences.

> Le correctif est petit, l'enjeu est maximal : c'est le plafond. On le ferme d'abord, on
> mesure l'ampleur de la classe ensuite, et on converge les deux chemins dans un spec dédié.
