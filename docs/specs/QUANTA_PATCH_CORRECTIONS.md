# QUANTA — Backlog de correction (revue T0.1 tranches 3 à 6)

> Tâches au format du gabarit (`QUANTA_AGENT_CONSTITUTION.md` §6), ordonnées par priorité.
> Toutes les règles de la Constitution s'appliquent. Les tâches marquées 🛑 peuvent déclencher
> la règle d'arrêt §4 (arbitrage consensus/sécurité) : l'agent s'arrête et remonte la décision,
> il ne devine pas.
>
> Ordre d'exécution : **C1 d'abord** (prouve la propriété centrale), puis C2 (débloque la sync
> avant tranche 7), puis C3 à C6, puis C7 et C8 (corrections de plan, avant le simulateur T0.4).

---

## Priorité 1 — prouver le déterminisme et débloquer la sync

### C1 — Audit de déterminisme transitif + méta-test (pièce maîtresse)

```
## Tâche : C1 — Prouver le déterminisme du cœur de bout en bout

### Phase
Phase 0 — Fondation de fiabilité.

### Contexte
La preuve « sans-IO » actuelle repose sur un grep de `src/sm/` uniquement. Or `Node::handle`
appelle des fonctions de `p2p` (`dispatcher::validate_envelope_at`,
`ledger::apply_remote_tx_checked` → `verify_tx`, `ledger::integrate_remote_block` et leurs
helpers). Le déterminisme doit tenir de façon TRANSITIVE à travers tout cet arbre d'appel, pas
seulement sur la surface des fichiers `sm/`. Le grec sur `sm/` ne voit rien d'une lecture
d'horloge dans `p2p::ledger::verify_tx`.

### Objectif
Établir une preuve de déterminisme qui couvre le chemin complet, et un test de régression qui
la garde.

### Contraintes
- Tous les invariants §3.
- Aucune modification de logique : C1 est diagnostic + test. Tout correctif de clock découvert
  est traité en C2, pas ici.

### Livrables attendus
1. **Audit transitif documenté** : le graphe d'appel réel atteint par `Node::handle` pour
   `Tick`, `MessageReceived` (chaque variante de payload migrée), et `Command`. Pour chaque
   fonction du graphe (y compris `verify_tx`, `integrate_remote_block` et leurs sous-helpers),
   verdict explicite : lit-elle l'horloge système, `OsRng`, ou itère-t-elle un `HashMap` ?
   Lister chaque lecture trouvée avec son fichier:ligne.
2. **Méta-test de déterminisme** : exécuter une séquence d'events FIXE (ticks injectés +
   messages signés) N fois (N ≥ 100) et asserter que les N exécutions produisent des `Effect`
   byte-identiques ET un état de ledger identique (hash du snapshot). Toute source de
   non-déterminisme transitive (ordre HashMap, `OsRng`) fait diverger au moins un run.

### Critères d'acceptation
- L'audit transitif est écrit et exhaustif (chaque fonction du graphe a un verdict).
- Le méta-test passe (N runs identiques). S'il échoue, il a trouvé un non-déterminisme : le
  documenter et le router (HashMap/RNG → corrigé ici en sous-tâche minimale ; horloge → C2).
- `cargo test --lib`, `clippy -D warnings`, fmt propres.

### Note règle d'arrêt
Si l'audit révèle une lecture d'horloge dans un chemin de VALIDATION (pas seulement de création
ou d'admission locale), ne pas la corriger ici : la signaler et la traiter en C2 🛑.
```

### C2 — Fraîcheur de bloc et de tx à temps injecté (avant tranche 7) 🛑

```
## Tâche : C2 — Rendre la validation indépendante de l'horloge murale

### Phase
Phase 0 — Fondation de fiabilité. À FAIRE AVANT tranche 7 (sync de chaîne).

### Contexte
La sync rejoue des blocs HISTORIQUES (RequestChain → ChainSegment). Si la validation de bloc ou
de tx rejette sur un contrôle de fraîcheur relatif à l'horloge murale (« pas trop vieux / pas
dans le futur »), chaque bloc historique échoue et la sync devient impossible. Le premier audit
de déterminisme avait relevé une lecture d'horloge en « tx validation » à `ledger.rs:272` ;
C1 a établi la liste complète des lectures dans les chemins de validation.

### Objectif
Tout contrôle de fraîcheur situé dans un chemin de VALIDATION devient à temps injecté (motif
déjà établi par `GossipRouter::is_fresh_at`), et la validation de blocs historiques au cours
d'une sync ne dépend plus du moment où on valide.

### Contraintes
- Tous les invariants §3.
- Comportement préservé pour les blocs/tx frais : injecter le temps ne change pas le verdict
  quand le timestamp est proche de « now ».
- Réutiliser le motif `_at(…, now)` existant (wrapper de prod lit l'horloge à la frontière).

### Livrables attendus
- Les contrôles de fraîcheur identifiés en C1 prennent le temps en paramètre injecté.
- **Test sync-replay (celui qui manque aujourd'hui)** : sceller un bloc (et/ou forger une tx) à
  un instant t0, puis le valider/intégrer à un instant injecté **t0 + plusieurs heures**, et
  asserter qu'il s'intègre quand même. C'est le cas dangereux que les tests actuels ne couvrent
  pas (ils valident à t ≈ instant de scellement).

### Critères d'acceptation
- Le test sync-replay passe : un bloc/tx « vieux » s'intègre à un « now » injecté très
  postérieur.
- Aucun chemin de validation ne lit plus l'horloge système (re-vérifié via le méta-test C1).
- `cargo test --lib`, `clippy -D warnings`, fmt propres.

### Note règle d'arrêt 🛑
Décision `ledger.rs:272` : si un contrôle de fraîcheur en validation **rejette par conception**
les éléments anciens, l'injecter ne suffit pas (la sync resterait cassée). Faut-il alors retirer
ce contrôle du chemin de validation et ne le garder qu'à l'admission mempool locale ? C'est un
arbitrage protocole. S'arrêter et remonter les options ; ne pas deviner.
```

---

## Priorité 2 — durcir le chemin consensus-critique

### C3 — Observabilité des décisions consensus du cœur

```
## Tâche : C3 — Rendre les décisions consensus du cœur observables

### Phase
Phase 0 — Fondation de fiabilité.

### Contexte
`Node::on_message` fait `let _ = self.ledger.integrate_remote_block(block);` et renvoie un
`Vec::new()` vide. Un bloc rejeté et un bloc valide sans effet sortant sont donc indistinguables
à la frontière du cœur. Pour que le futur simulateur puisse asserter « ce bloc byzantin a été
rejeté », l'issue doit être observable, pas avalée.

### Objectif
L'issue de chaque décision consensus du cœur (bloc intégré / rejeté / doublon ; tx admise /
rejetée) est inspectable par le harness, sans changer le comportement réseau.

### Contraintes
- Tous les invariants §3.
- Additif : aucune règle de protocole modifiée (pas de §4).
- Ne pas exposer de secret ; l'issue est un statut, pas un dump d'état interne sensible.

### Livrables attendus
- Un mécanisme d'observabilité au choix de l'agent (statut retourné, compteurs inspectables,
  ou un `Effect` d'observation), permettant d'asserter l'issue d'un `MessageReceived` consensus.
- Tests : un bloc rejeté est distinguable d'un bloc intégré **à la frontière du cœur** (pas
  seulement en inspectant `chain.len()`).

### Critères d'acceptation
- Test prouvant la distinction intégré vs rejeté via le canal d'observabilité.
- `cargo test --lib`, `clippy -D warnings`, fmt propres.
```

### C4 — Couverture consensus : reorg à travers le cœur + motifs de rejet

```
## Tâche : C4 — Tester le reorg et les rejets à la frontière du cœur

### Phase
Phase 0 — Fondation de fiabilité.

### Contexte
La tranche 6 affirme préserver AUDIT-BLK-1/2 (validation pré-mutation, fork reorg sans perte de
tx), mais ne le TESTE pas à travers `Node::handle`. Deux tests seulement couvrent le chemin le
plus critique (extension linéaire + lien cassé). Le reorg, comportement le plus dangereux et le
plus audité, n'est pas exercé via le cœur.

### Objectif
Le comportement de résolution de fork et les principaux motifs de rejet sont testés à travers le
cœur, pas seulement sur `integrate_remote_block` en direct.

### Contraintes
- Tous les invariants §3.
- Tests uniquement (les panics sont tolérés dans les tests).

### Livrables attendus
- **Test de reorg via `Node::handle`** : nourrir le cœur d'une chaîne concurrente plus lourde
  qui force un reorg ; asserter que le tip bascule, que les tx exclusives à la branche perdante
  sont re-queue (AUDIT-BLK-1), et qu'aucune tx validée n'est perdue.
- **Tests de rejet supplémentaires via le cœur** : racine de Merkle incohérente ; bloc qui
  dépasse le plafond d'émission (par bloc et/ou global) ; (si applicable) signature invalide
  d'une tx contenue dans le bloc.

### Critères d'acceptation
- Le test de reorg passe et vérifie explicitement la non-perte de tx.
- Chaque motif de rejet laisse la chaîne inchangée.
- `cargo test --lib`, `clippy -D warnings`, fmt propres.
```

---

## Priorité 3 — robustesse et sûreté

### C5 — Frontière de vérification de tx symétrique

```
## Tâche : C5 — Un seul point d'entrée d'admission de tx, signature imposée par la méthode

### Phase
Phase 0 — Fondation de fiabilité.

### Contexte
Le cœur admet les tx via `apply_remote_tx_checked` (qui appelle `verify_tx` à l'intérieur). Le
shell `handle_broadcast_tx` passe par `apply_verified_remote_tx` (qui NE vérifie PAS la
signature ; le shell la vérifie en amont). Si un futur edit retire le `verify_tx` amont du
shell, le shell appliquerait des tx non vérifiées alors que le cœur non : la précondition repose
sur la mémoire de l'appelant, pas sur la méthode.

### Objectif
La précondition « signature vérifiée » est imposée par la méthode partagée, pas par chaque
appelant ; cœur et shell convergent sur un point d'entrée unique signature-gated.

### Contraintes
- Tous les invariants §3.
- **Comportement strictement préservé** : même `verify_tx`, même ordre, mêmes gardes
  AUDIT-TX-1/2. Éviter la double-vérification (ne pas vérifier deux fois si les deux chemins
  convergent).
- Si la consolidation force un changement d'ordre observable, s'arrêter et remonter 🛑.

### Livrables attendus
- Shell et cœur passent par le même point d'entrée signature-gated.
- CHANGELOG attestant l'équivalence de comportement.

### Critères d'acceptation
- Toute la suite existante reste verte (aucune régression).
- `clippy -D warnings`, fmt propres.
```

### C6 — Zeroize de l'identité tenue par le cœur

```
## Tâche : C6 — Garantir le zeroize du secret tenu dans Node

### Phase
Phase 0 — Fondation de fiabilité.

### Contexte
Depuis la tranche 5, `Node` tient une `CryptoEngine` (matière secrète : la clé de signature).
La Constitution §3 exige `zeroize`/`ZeroizeOnDrop` sur chaque secret.

### Objectif
Confirmer que le secret de signature détenu par le cœur est effacé en mémoire au drop.

### Contraintes
- Tous les invariants §3.

### Livrables attendus
- Vérification que `CryptoEngine` (et la clé qu'elle contient) implémente bien le zeroize au
  drop ; si ce n'est pas le cas, l'ajouter.

### Critères d'acceptation
- Le secret est `zeroize`-é au drop (par revue de code, et test si faisable).
- Aucune régression ; `clippy -D warnings`, fmt propres.
```

---

## Priorité 4 — corrections de plan (avant le simulateur T0.4)

### C7 — Scellement de bloc à temps injecté dans le cœur 🛑

```
## Tâche : C7 — Faire produire des blocs au cœur, de façon déterministe

### Phase
Phase 0 — Fondation de fiabilité. À FAIRE AVANT T0.4 (simulateur).

### Contexte
Le cœur sait VALIDER un bloc (tranche 6) mais pas en PRODUIRE. Le scellement vit dans
`mining_loop` et lit l'horloge pour le timestamp du bloc. Or consensus = production +
validation + résolution de fork, et le harness DST existe pour tester la DYNAMIQUE du consensus
(élection → scellement → propagation → validation → convergence). Sans scellement à temps
injecté dans le cœur, le simulateur ne pourra pas faire produire de blocs aux nœuds.

### Objectif
Le cœur peut sceller un bloc de façon déterministe, le timestamp provenant du temps injecté
(`self.now_ms`), de sorte qu'un leader puisse produire un bloc dans le simulateur de façon
reproductible.

### Contraintes
- Tous les invariants §3, en particulier déterminisme (timestamp = temps injecté, élection déjà
  RNG-free) et arithmétique vérifiée sur les bornes d'émission.
- **Extraction, pas refonte** : préserver la logique de scellement existante. Si l'extraction
  force un changement de règle de consensus (élection, bornes, structure de bloc), s'arrêter et
  remonter 🛑.

### Livrables attendus
- Chemin de scellement dans le cœur produisant un `Effect` (le bloc scellé à diffuser), timestamp
  issu de `now_ms`.
- Tests : un leader élu scelle un bloc valide à temps injecté ; un non-leader ne scelle pas ;
  scellement byte-déterministe (même seed, même séquence ⇒ mêmes octets).

### Critères d'acceptation
- Les tests de scellement passent, déterminisme inclus.
- `cargo test --lib`, `clippy -D warnings`, fmt propres.

### Note règle d'arrêt 🛑
Le scellement touche le cœur du consensus. Toute ambiguïté sur une règle (qui peut sceller,
quand, avec quelles bornes) est un arbitrage à remonter, pas à deviner.
```

### C8 — Décider et documenter le modèle de propagation

```
## Tâche : C8 — Fixer le modèle de propagation (transport-flood vs app-forward)

### Phase
Phase 0 — Fondation de fiabilité. À FAIRE AVANT T0.5 (réseau virtuel).

### Contexte
Le cœur n'émet rien à la réception d'un bloc (« pas de re-broadcast, comme la prod »). Pour que
les blocs se propagent au-delà d'un saut, soit le transport (iroh-gossip) inonde au niveau
pub/sub, soit l'application doit relayer. Le réseau virtuel du simulateur devra modéliser
EXACTEMENT le même choix, sinon les blocs ne circuleront pas en simulation.

### Objectif
Statuer, preuve à l'appui, sur qui propage (transport ou application), et documenter la
contrainte que cela impose au réseau virtuel du simulateur.

### Contraintes
- Diagnostic + documentation ; aucun changement de comportement de propagation sans remontée 🛑.

### Livrables attendus
- Note documentée : iroh-gossip inonde-t-il les messages au transport, ou l'app relaie-t-elle ?
  Réponse appuyée sur le code/la doc d'iroh-gossip.
- Spécification de la contrainte correspondante pour T0.5 (le réseau virtuel doit inonder si le
  transport inonde ; le cœur doit émettre un effet de relais si l'app relaie).

### Critères d'acceptation
- La note tranche sans ambiguïté et cite sa source.
- La contrainte pour T0.5 est écrite.
```

---

## Récapitulatif d'ordre et de risque

| Tâche | Quoi | Risque §4 |
|---|---|---|
| **C1** | Audit transitif + méta-test déterminisme (pièce maîtresse) | faible |
| **C2** | Fraîcheur de validation à temps injecté + test sync-replay | 🛑 décision `ledger.rs:272` |
| C3 | Observabilité des décisions consensus | faible |
| C4 | Reorg + motifs de rejet testés via le cœur | faible |
| C5 | Point d'entrée d'admission de tx unique et signature-gated | 🛑 si l'ordre change |
| C6 | Zeroize du secret tenu par `Node` | faible |
| **C7** | Scellement de bloc à temps injecté (avant le simulateur) | 🛑 consensus |
| **C8** | Modèle de propagation décidé et documenté (avant T0.5) | faible |
