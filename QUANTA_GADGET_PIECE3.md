---
type: task-spec
id: GADGET-3
status: à exécuter (pièce 3 du gadget ; débloquée par le re-keying PQ-MIG-3B)
priorité: 🔴 la règle justifier/finaliser — la finalité devient RÉELLE
classe: règle en deux temps (Casper) consommant les certificats de GADGET-2 ; FinalitySafety cesse d'être vacueux
origine: [[DESIGN-FINALITY-GADGET]] §4, §11, §14 · construit sur GADGET-1 (squelette) + GADGET-2 (certificats)
liens: [[QUANTA_AGENT_CONSTITUTION]] · [[QUANTA_GADGET_PIECE2]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# GADGET-3 : la règle justifier/finaliser (la finalité devient réelle)

> Pièce 3. On applique la **règle en deux temps** validée : un point de contrôle devient
> **justifié** par un lien super-majoritaire (certificat GADGET-2) depuis un point déjà justifié,
> puis **finalisé** quand son **enfant direct** est justifié par un lien partant de lui. Consomme
> les certificats de GADGET-2. **La finalité cesse d'être vacueuse** : l'ensemble finalisé de
> GADGET-1 (jusqu'ici `{genèse}`) **grandit vraiment**. Pas de fork-choice (GADGET-5) ni de
> slashing (GADGET-4). Diff logique seule, déterministe, C1 vert.

## 1. État de justification
- Ensemble des points de contrôle **justifiés**, init `{genèse}` (genèse justifiée par
  définition). À côté de l'ensemble **finalisé** de GADGET-1.

## 2. La règle (deux temps)
- **Justifier** : recevant un certificat valide (GADGET-2) pour un lien `source → cible`, si
  `source` est **justifié**, alors `cible` devient **justifié**.
- **Finaliser** : si `c` est justifié **et** qu'il existe un certificat pour le lien `c → enfant
  direct` (époque c+1), alors `c` devient **finalisé**. (Deux époques consécutives liées.)
- Fonction **pure** sur (certificats, ensembles justifié/finalisé). Aucune horloge/entropie.

## 3. FinalitySafety devient réel
- L'ensemble finalisé grandit désormais via la règle. L'invariant **FinalitySafety** (GADGET-1)
  protège enfin de vrais points finalisés, et le **test à violation plantée** de GADGET-1 doit
  **rester vert** (il mord toujours).

## 4. Les dents (obligatoire)
- **justification** : un certificat ⅔ depuis une source justifiée ⇒ cible justifiée.
- **pas de justification frauduleuse** : source **non** justifiée, ou certificat sous le quorum ⇒
  **rien** n'est justifié.
- **deux temps, pas un** : un point justifié **seul** (sans lien vers son enfant) n'est **PAS**
  finalisé ; il l'est seulement quand l'enfant est justifié par un lien partant de lui. (Le cœur
  validé : un seul temps ne finalise pas.)
- **chemin honnête finalise** : une suite de certificats bien formés finalise les points de
  contrôle attendus.
- **FinalitySafety** : la violation plantée de GADGET-1 reste détectée ; aucun chemin honnête ne
  finalise deux points en conflit.
- **déterminisme** : mêmes certificats ⇒ mêmes ensembles justifié/finalisé sur deux nœuds.

## Garde-fous
- Réutiliser GADGET-1 (`sm/finality.rs`, points de contrôle, FinalizedSet) et GADGET-2
  (certificat / abstraction). Ne pas redéfinir.
- **Pas** de fork-choice (GADGET-5) ni de slashing (GADGET-4) ici. **§4 STOP** si l'un semble
  requis.
- **Diff logique seule** ; `dispatcher.rs` intact ; pas de nightly-fmt fichier entier.
- **Déterminisme** : règle pure ; `src/sm/` sans-IO ; **C1 vert**.
- **Pas de masquage** : les dents §4 mordent, surtout « deux temps pas un ».
- **Snapshot git** avant.

## Porte d'acceptation
- `cargo test --lib` **vert**, incluant les dents §4 (surtout « deux temps pas un » et
  FinalitySafety réel).
- `clippy --lib -D warnings` propre · **C1 vert** · sweep + couverture + conservation verts ·
  `src/sm/` sans-IO.
- `git diff` logique seule · `dispatcher.rs` intact · le test à violation plantée de GADGET-1
  **toujours vert**.
- Entrée GADGET-3 au tracker + auto-revue §3 (la règle, l'ensemble finalisé qui grandit, les
  dents, le périmètre tenu).

## Séquence
1. **§1** état de justification (init `{genèse}`).
2. **§2** la règle en deux temps consommant les certificats GADGET-2.
3. **§4** les dents, dont « deux temps pas un » et FinalitySafety réel.

> Après GADGET-3, la finalité **vit** : des points de contrôle se finalisent et FinalitySafety
> protège du réel. Restent **GADGET-4** (slashing : les deux conditions, qui *découlent* du
> théorème), **GADGET-5** (fork-choice conscient de la finalité, qui résout la partition, le « à
> corriger plus tard » assumé), et PQ-MIG-5 (genèse PQ).
