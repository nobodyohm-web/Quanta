---
type: task-spec
id: GADGET-1
status: à exécuter (après que tu aies fixé E et relu la conception)
priorité: socle du gadget de finalité (§14, pièce 1)
classe: squelette époque/point de contrôle + invariant de sûreté de finalité (harnais)
origine: [[DESIGN-FINALITY-GADGET]] §2, §4, §11, §14
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[ADR-005 — Agrégation des votes]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# GADGET-1 : le socle, époques, points de contrôle, et l'invariant de sûreté

> Première pièce du gadget, **et la plus sûre** : c'est le **bedrock** (découper la chaîne en
> époques, identifier les points de contrôle, suivre l'ensemble finalisé, poser l'invariant de
> sûreté dans le harnais). C'est la partie qui **ne bouge pas** quels que soient les choix plus
> fins du §12. **Rien ne finalise encore** ici : la règle de justification/finalisation est la
> pièce 3. On pose la fondation et le crochet de vérification. Diff logique seule, déterministe,
> C1 vert.

## Cadre
- **E (longueur d'époque) est ta décision §12** : le squelette se construit **paramétriquement**,
  `E` en **constante nommée marquée** (placeholder), exactement comme `k` pour l'émission. Ne
  l'invente pas en dur sans le signaler. **§4** : la valeur de `E` est à Alexandre.
- La conception est encore **« à valider »** : la pièce 1 est celle qui ne changera pas (les
  époques et points de contrôle sont le socle de tout gadget Casper-style), donc elle est sûre
  à poser ; les pièces suivantes (votes, finalisation, fork-choice) attendent la validation.

## 1. Structure époque / point de contrôle (déterministe)
- Définis l'**époque** comme un bloc de `E` blocs, et le **point de contrôle** d'une frontière
  d'époque comme la paire `(hauteur, hash)` du bloc de frontière.
- Fonction **pure** : pour une chaîne donnée, calculer le point de contrôle de chaque frontière
  d'époque, et l'époque à laquelle appartient un bloc. **Aucune** lecture d'horloge ni d'entropie
  (sans-IO préservé, C1 reste vert).

## 2. Suivi de l'ensemble finalisé (genèse seule, pour l'instant)
- Une structure suivant l'**ensemble des points de contrôle finalisés**, **initialisée à
  `{genèse}`** (la genèse est finalisée par définition, cf. design §4).
- **Rien d'autre ne finalise** à ce stade (la pièce 3 ajoutera la vraie règle) : c'est
  l'**emplacement** où la finalisation future se branchera. Ne code **pas** de logique de
  justification/finalisation ici.

## 3. Invariant de sûreté de finalité (dans le harnais)
- Ajoute à `Sim::check_invariants` / `run_checked` l'invariant : **à travers les nœuds, jamais
  deux points de contrôle finalisés en conflit à la même époque** (les ensembles finalisés des
  nœuds doivent coïncider sur chaque époque commune).
- Câble-le dans le balayage par défaut, vérifié par pas, ordre déterministe (clés triées).

## 4. Prouver que l'invariant a des dents (anti-vacuité, crucial)
Comme **rien ne finalise encore** (pièce 3), cet invariant serait **vacueusement vrai** : seule
la genèse est finalisée, donc il ne peut rien attraper. C'est exactement le piège « test qui
passe quoi qu'il arrive » qu'on traque depuis le début. Donc :
- Ajoute un **test à violation plantée** : injecte directement dans l'état de deux nœuds **deux
  points de contrôle finalisés en conflit à la même époque** (hors de tout chemin de
  finalisation réel, comme le test de dents de T0.8), et asserte que l'invariant **se déclenche**
  avec le bon repère.
- Ce test prouve que le **vérificateur mord** dès maintenant, avant même que la finalisation
  existe. Sans lui, l'invariant n'est qu'un décor.

## Garde-fous
- **Diff logique seule** ; pas de nightly-fmt fichier entier ; `dispatcher.rs` intact.
- **Déterminisme** : le calcul des points de contrôle est une fonction pure, aucune horloge ni
  entropie ; `src/sm/` sans-IO préservé ; **C1 vert**.
- **Pas de masquage** ni de test vacueux : l'invariant **doit** être prouvé mordant (§4).
- **§4** : la valeur de `E` est à Alexandre ; ne pas la figer en dur sans la marquer.
- **Snapshot git** avant de commencer.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant le **test à violation plantée** du §4.
- `cargo clippy --lib -- -D warnings` propre · `src/sm/` sans-IO propre · **C1 vert** ·
  **sweep par défaut vert**.
- **`git diff` logique seule** · `dispatcher.rs` intact.
- Entrée **GADGET-1** au tracker + auto-revue §3, notant : la structure époque/checkpoint, l'init
  de l'ensemble finalisé à `{genèse}`, l'invariant câblé, **la preuve de dents** (violation
  plantée), et `E` marqué comme décision ouverte d'Alexandre.

## Séquence
1. Structure époque/point de contrôle (pure, déterministe).
2. Ensemble finalisé initialisé à `{genèse}`.
3. Invariant de sûreté de finalité dans le harnais.
4. Test à violation plantée (les dents).

> Pièces suivantes, **en attente de ta validation de la conception et de `E`** : pièce 2 (votes
> ML-DSA + certificat d'époque derrière l'abstraction), puis pièce 3 (la règle
> justification/finalisation, là où la sûreté devient réelle et où l'invariant cesse d'être
> vacueux). On les fait une par une, chacune falsifiée par le harnais avant la suivante.
