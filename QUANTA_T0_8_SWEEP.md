---
type: task-spec
id: T0.8
status: à exécuter
priorité: capstone Phase 0 (porte globale)
classe: harnais DST — balayage multi-seed + replay
dépend de: T0.4→T0.7 (horloge virtuelle, NetFaults, crash/restart, byzantins, run_checked) · EMIT-1 (invariant émission)
liens: [[QUANTA_T0_DST_HARNESS]] · [[QUANTA_AGENT_CONSTITUTION]] · [[AUDIT_QUANTA_2_PROGRESS]]
---

# T0.8 — Balayage multi-seed + replay (la porte globale Phase 0)

> On prend toute la machinerie T0.4→T0.7 et on la tourne à travers **N scénarios
> pseudo-aléatoires dérivés du seed**, en vérifiant **les trois invariants** à chaque
> pas. Toute panne est **reproductible à l'octet** via son seed.
>
> **Cadre honnête : c'est un falsificateur, pas une preuve.** Passer N seeds ne prouve
> pas le protocole correct ; ça donne une confiance proportionnelle au nombre de seeds
> **et** à la couverture de fautes réellement exercée. D'où les deux gardes anti-vacuité
> du §5, non négociables.

## 1. Objet

Un runner qui, pour chaque seed d'une plage :
1. dérive un **scénario complet** (topologie, plan de fautes, byzantins, crash/restart,
   charge de tx, timings) **purement depuis le seed**,
2. exécute la sim bornée,
3. vérifie **sûreté + conservation + émission** à une cadence qui attrape les violations
   **transitoires**,
4. sur violation, enregistre `{seed, invariant, premier pas fautif}`.

Plus un **replay** : depuis un seed isolé, reconstruire le **même** scénario et rejouer,
reproduisant la panne à l'octet, avec trace complète pour le debug.

## 2. Le cœur à ne pas rater : une seule fonction de dérivation

`scenario(seed) -> ScenarioPlan` doit être **une fonction pure unique**, appelée **à la
fois** par le balayage et par le replay. Si le sweep et le replay dérivent le scénario
par deux chemins différents, le replay ne reproduit rien et tout T0.8 est inutile.
**Un seul générateur, deux appelants.**

## 3. Ce que le runner fait

- **Sweep** : itère les seeds `0..N` (ordre déterministe), `scenario(seed)`, exécute,
  `check_invariants` à la cadence du §4, collecte les violations. Sortie propre :
  « N seeds, 0 violation ». Sortie fautive : la **liste des seeds fautifs** avec
  invariant + premier pas.
- **Replay** : un seed unique (mécanisme idiomatique au choix, ex. env var
  `QUANTA_SIM_SEED` qui active un `#[test]` mono-seed, ou `pub(crate) fn replay(seed)`),
  **même `scenario(seed)`**, exécution tracée, résultat **byte-identique** au passage du
  sweep sur ce seed.

## 4. Contraintes dures

- **Déterminisme total.** Toute l'aléa du scénario vient d'un **PRNG seedé déterministe**
  (splitmix/PCG seedé par `seed`), **jamais** d'`OsRng`, d'horloge murale, ni d'itération
  de `HashMap`. Sans-IO préservé (cf. les doc-comments de pureté de `sm/`). C1 reste vert.
- **Cadence de vérification.** La **sûreté** se vérifie à un pas assez fin pour attraper
  une divergence **transitoire** (deux nœuds en désaccord pendant une partition qui se
  réconcilie ensuite reste une violation de sûreté). Enregistre le **premier** pas fautif,
  pas seulement l'état final. Conservation/émission sont collantes : cadence plus lâche
  tolérée si la perf l'exige, mais la sûreté par pas est obligatoire.
- **Bornes.** Pas, nœuds, tx, mémoire : tous bornés, le sweep **termine**. Plage `N` par
  défaut **dimensionnée pour le budget temps de la suite** (quelques dizaines de secondes
  comme le C1 actuel), avec un env var (`QUANTA_SIM_SEEDS`) pour pousser en run profond.
- **Sweep reproductible.** Même plage rejouée deux fois ⇒ **même** ensemble de seeds
  fautifs (conséquence du déterminisme par seed, à asserter explicitement).

## 5. Anti-vacuité (les deux dents, non négociables)

- **5.1 Couverture réelle.** Un sweep où chaque seed produit un happy-path 3-nœuds sans
  faute « passe » en testant **le vide**. Ajoute un test de **couverture** prouvant que,
  sur la plage par défaut, le générateur produit **réellement** : des partitions, des
  drops/dups/délais, des nœuds byzantins/équivocateurs, des crash/restart. Si la
  couverture est creuse, le sweep ne vaut rien.
- **5.2 Dents du runner.** Plante une violation **connue** dans un scénario (ex. un nœud
  qui équivoque et fait diverger deux tips au même index, ou un double-mint forcé) et
  asserte que le sweep la **signale** avec le **bon seed** — **et** que le **replay** de
  ce seed la **reproduit**. Un seul test prouve les dents **et** la fidélité du replay. Un
  runner qui ne sait pas attraper une violation plantée est un runner qui valide tout par
  construction.

## 6. Tests (T0.8.x)

- **clean_default_sweep** : `N` par défaut, **0 violation**, dans le budget temps.
- **sweep_is_reproducible** : même plage deux fois ⇒ ensemble de seeds fautifs identique.
- **replay_is_byte_identical** : un seed au scénario **riche** (avec fautes/byzantins)
  rejoué deux fois ⇒ identique (étend C1 du happy-path au plan complet).
- **sweep_exercises_faults** : la couverture du §5.1.
- **sweep_catches_planted_violation** : les dents + replay du §5.2.

## 7. Règle critique — une violation trouvée est un LIVRABLE, pas un échec à verdir

Tout l'intérêt de T0.8 est de **trouver des bugs**. Si le sweep révèle une vraie violation
de sûreté / conservation / émission :
- **STOP et remonte** : le seed, l'invariant, le premier pas fautif, la trace de replay.
- **Ne masque jamais** : ne désactive pas le checker, ne tune pas le scénario pour éviter
  le seed fautif, ne baisse pas la cadence pour que la transitoire passe inaperçue.
- Une panne reproduite **est** le résultat attendu. La verdir serait détruire la seule
  valeur de l'outil.

## 8. Garde-fous de process

- **Diff logique seule.** Pas de nightly-fmt sur fichiers entiers, pas de reflow sur du
  code non touché. `dispatcher.rs` **intact** côté formatage.
- **Pas de masquage** (cf. §7), ni de test fabriqué pour gonfler le compte.
- **Snapshot git** avant de commencer.

## 9. Porte d'acceptation

- `cargo test --lib` **vert**, incluant **5.1 couverture** et **5.2 dents+replay**.
- `cargo clippy --lib -- -D warnings` **propre**.
- `src/sm/` sans-IO **propre** · **C1 vert** · sweep par défaut **dans le budget temps**.
- **`git diff` logique seule** · `dispatcher.rs` intact.
- Auto-revue §3 dans `AUDIT_QUANTA_2_PROGRESS.md` : déterminisme (scénario pur(seed),
  sweep reproductible) / arithmétique / **robustesse (bornes, terminaison)** / **sécurité
  (les trois invariants balayés, dents prouvées, couverture prouvée)** / mémoire / tests.
- Entrée **T0.8** au tracker : `N` par défaut retenu + budget temps, mécanisme de replay,
  ce que la couverture exerce, le test de violation plantée.

## 10. Séquence

1. **`scenario(seed)`** d'abord (le générateur pur unique).
2. **Sweep** + **replay** par-dessus, partageant ce générateur.
3. **Dents (5.2)** et **couverture (5.1)** : tant qu'elles ne passent pas, le sweep n'a
   pas de valeur démontrée.
4. **clean_default_sweep** en dernier : il n'a de sens qu'une fois 5.1/5.2 verts.

> **Hors scope / à porter.**
> - **Mode soak** (millions de seeds en nocturne) = suivi optionnel, pas cet incrément.
> - **Émission `≤`→`==`** : le sweep balaie aujourd'hui la forme `≤` (correcte pré-loi). À
>   **réexaminer à EMIT-LAW-1** quand chaque bloc de proposeur portera `R` (la note qu'on
>   a portée à la revue EMIT-1-VERIFY).
> - **Commit du purge crypto-only** pour rebaser un HEAD propre = opération git **manuelle**
>   d'Alexandre, pas l'agent.
