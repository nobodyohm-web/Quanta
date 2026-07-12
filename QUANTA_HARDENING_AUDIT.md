---
type: audit-spec
id: HARDEN-AUDIT-1
status: à exécuter via /goal (pas /loop)
mode: ultra-effort, raisonnement maximal
classe: audit de durcissement Phase 0, LECTURE SEULE
nature: trouve tout, ne corrige rien — chaque trouvaille devient ensuite son propre spec chirurgical
liens: [[QUANTA_AGENT_CONSTITUTION]] (§3 invariants, §4 stop) · [[AUDIT_QUANTA_2_PROGRESS]] · [[QUANTA_T0_8_HARDEN]] · [[QUANTA_SIGN_DET_VERIFY]]
---

# HARDEN-AUDIT-1 — audit de durcissement, lecture seule, raisonnement maximal

> **Ceci n'est pas une tâche de modification. C'est un audit.** Le livrable est un
> **rapport de trouvailles classé**, pas un code modifié. **Zéro changement de code de
> prod ou de test.** Pour chaque trouvaille, l'agent propose le correctif chirurgical
> **en esquisse dans le rapport**, sans l'appliquer. C'est l'usage juste de l'ultra-effort :
> scrutiny exhaustive, blast radius nul. Ensuite, chaque trouvaille réelle devient un spec
> chirurgical séparé, revu et gated individuellement.

> **Pourquoi lecture seule.** La base est consensus-critique et **sans filet git** (purge
> et incréments non commités, `sim.rs` non tracké). Un audit qui ne touche rien rend
> l'absence de filet sans conséquence pour cette tâche. Même un correctif « trivial » est
> **différé** : il deviendra un micro-spec, pour garder l'audit pur et reviewable.

## 0. Méthode (haute puissance)

Pour **chaque** surface ci-dessous : énumération systématique, preuve avec `fichier:ligne`,
le **scénario adverse concret** ou l'**invariant** que ça menace, une **sévérité**
(critique / haute / moyenne / basse), et un **correctif chirurgical proposé** en esquisse.
Raisonne en profondeur : ne te contente pas du symptôme, remonte à la cause et à la classe
du problème (s'il y a un, il y en a souvent des frères). On a déjà trouvé plusieurs fuites
d'horloge, un trou de zeroize, un hash aveugle au contenu, des tests vacueux : **suppose
qu'il en reste**.

## 1. Surfaces d'audit

### 1.1 Déterminisme et sans-IO
Traque **toute** source de non-déterminisme dans le cœur de consensus ou atteignable depuis
lui : lecture d'horloge murale (`Utc::now`, `SystemTime`, `Instant`), entropie OS (`OsRng`,
`thread_rng`), itération de `HashMap`/`HashSet` (ordre non déterministe), flottants sur un
chemin de consensus, `Instant`/timeouts dans le noyau. Confirme au passage que la lecture
d'horloge de la fenêtre d'enveloppe ±90s vit bien au **bord réseau** et **jamais** dans le
noyau ni un chemin de validation. Toute fuite ici est **critique** : elle casse l'accord
inter-nœuds.

### 1.2 Arithmétique vérifiée
**Chaque** calcul de valeur (solde, récompense, masse, burn, frais) : débordement,
sous-débordement, troncature, division entière et arrondi, conversions de largeur
(`as u64`, `as usize`). Cherche tout calcul monétaire non vérifié. Anticipe la loi
d'émission `R = k(S_max − S)` à venir et son arithmétique entière `u128`.

### 1.3 Pas de panique non maîtrisée
**Chaque** `unwrap`/`expect`/`panic!`/`unreachable!`, indexation de slice ou tableau,
`[..]`, division, qui pourrait paniquer sur une **entrée adverse** dans un chemin de
consensus ou de réseau. Un nœud qu'on peut faire paniquer à distance est un vecteur de DoS.

### 1.4 Zeroize et hygiène des clés
**Chaque** type porteur de secret (clé de signature Ed25519 et ML-DSA, graines, matériel
de dérivation) : est-il effacé à la libération ? On a déjà comblé un trou (la clé Ed25519
non remise à zéro) : cherche ses **frères** sur tous les types secrets, et les copies
intermédiaires non effacées.

### 1.5 Mémoire bornée
**Chaque** collection atteignable depuis une entrée réseau (mempool, table de pairs,
tampons de gossip, ensembles de « vus », files de re-queue) : sa croissance est-elle
**bornée** ? Une collection illimitée alimentée par le réseau est un DoS par épuisement
mémoire. Le TTL de mempool NET-14 compte ici : couvre-t-il toutes les voies d'insertion ?

### 1.6 Soundness de la validation
Cherche les endroits où la validation est **aveugle au contenu** ou où **deux nœuds
peuvent diverger** sur la validité (on a trouvé le hash aveugle au contenu et la divergence
de réputation, tous deux corrigés). Toute donnée entrant dans une décision de consensus mais
mesurée **localement** plutôt que dérivée de la chaîne est suspecte. Inclut : un horodatage
de tx n'étant borné par rien à la validation (confirmé par SIGN-DET), à **signaler comme
décision ouverte**, pas à corriger.

### 1.7 Vacuité des tests
Audite la suite : pour **chaque** test d'invariant, **échouerait-il réellement** si la
propriété qu'il prétend tester cassait ? On a répété ce piège (la conservation aveugle au
mint illégitime, la forme `≤` trop molle, E1 ne couvrant qu'une moitié). Liste les tests
qui passent **indépendamment** de la propriété, et propose comment leur donner des dents.

### 1.8 Items ouverts (vérifier qu'aucun n'a régressé en silence)
Confirme l'état, sans rien trancher : réconciliation de partition multi-blocs (gap connu,
gadget-deferred), fork-choice intérimaire single-block, BLS contre PQ pour l'agrégation des
votes (ouvert), paramètres de slashing (ouverts), loi d'émission (non implémentée), borne
temporelle des tx (ouverte). Ce sont des **décisions d'Alexandre** : les **lister**, jamais
les résoudre.

## 2. Règles dures

- **Lecture seule.** Zéro modification de code, prod ou test. Le livrable est le rapport.
- **§4, arrêt absolu.** Ne prends **aucune** décision de design ou de consensus (BLS contre
  PQ, slashing, émission, règles d'horodatage, stratégie de fork-choice). Tu les **signales**
  comme décisions, tu ne les **résous pas**, même « évident ».
- **Ne devine rien.** Si un comportement est ambigu, reporte l'ambiguïté ; ne tranche pas
  par hypothèse.
- **Pas d'emballement.** L'ultra-effort sert la **profondeur de l'audit**, pas l'ampleur des
  changements (il n'y en a aucun).

## 3. Livrable

Un **rapport unique classé** (par ex. `docs/AUDIT_HARDENING_PHASE0.md`), avec, pour chaque
trouvaille :
- `id` court, `surface` (1.1 à 1.8), `sévérité`,
- preuve `fichier:ligne`,
- l'**invariant ou scénario adverse** menacé,
- le **correctif chirurgical proposé** (esquisse de code, **non appliqué**),
- un drapeau **« décision d'Alexandre »** si c'en est une.

Plus une **synthèse en tête** : ce qui est solide, ce qui est faible, et le **top N à
corriger en premier**, ordonné par sévérité puis par effort. C'est cette liste qui devient
le backlog des prochains specs chirurgicaux.

## 4. Hors scope

- **Appliquer les correctifs** : chacun sera son propre spec, après ton tri.
- **Les décisions de design** : les tiennes (BLS/PQ, slashing, émission, horodatage).
- **Le baseline git** : opération manuelle d'Alexandre. Reste la **priorité numéro un**,
  indépendante de cet audit.

> Cet audit est la version juste de « protège tout au maximum » : il **trouve** tout ce qui
> menace la protection, sans rien risquer. La protection réelle vient ensuite, un correctif
> chirurgical et reviewable à la fois.
