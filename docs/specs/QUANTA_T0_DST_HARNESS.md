# QUANTA — Phase 0 : Harness de simulation déterministe

> Document de pilotage pour l'agent de code. À lire avec la Constitution d'ingénierie
> (`QUANTA_AGENT_CONSTITUTION.md`), dont toutes les règles s'appliquent ici.
>
> Objet : l'épopée « harness de simulation déterministe » (DST), décomposée en tâches de
> taille PR. La tâche **T0.1** est spécifiée intégralement au format du gabarit. Les tâches
> **T0.2 à T0.8** sont fournies en backlog ordonné, prêtes à être étendues dans le gabarit.

---

## 0. Étoile polaire architecturale (à comprendre avant de coder)

Le déterminisme est **impossible** avec Iroh et tokio multi-thread dans la boucle de
consensus. Raisons, à tenir pour acquises :

- Le runtime tokio multi-thread ordonne les tâches de façon non déterministe. Même en
  current-thread, l'ordre de réveil de plusieurs tâches prêtes n'est pas garanti.
- Iroh fait de l'I/O réseau réel (sockets, timers OS), non contrôlable de façon déterministe.

La solution est le pattern **sans-IO** (cœur fonctionnel déterministe, coquille impérative).

```
          ┌─────────────────────────────────────────────┐
          │   Cœur déterministe (AUCUN I/O, synchrone)   │
          │   fn handle(état, Event, rng) -> Vec<Effect> │
          │   ledger · consensus · mempool · @pseudo     │
          └───────────────┬─────────────────────────────┘
                          │  Events (entrées)  /  Effects (sorties)
         ┌────────────────┴───────────────────┐
         ▼                                     ▼
┌──────────────────────┐            ┌──────────────────────────┐
│  Coquille production  │            │  Coquille simulation      │
│  Iroh + tokio +       │            │  réseau virtuel + horloge │
│  OsRng + libSQL       │            │  virtuelle + RNG seedé    │
│  (I/O réel)           │            │  + injection de fautes    │
└──────────────────────┘            └──────────────────────────┘
```

Le cœur ne lit jamais l'horloge système, n'appelle jamais `OsRng`, ne touche jamais au réseau
ni au disque. Le temps est une entrée (`Event::Tick`), l'aléa est injecté (`&mut dyn Rng`),
l'envoi réseau et la persistance sont des sorties (`Effect`). La coquille de production traduit
l'I/O réel en events et exécute les effects. La coquille de simulation fait pareil, mais tout
est virtuel et piloté par une seed, donc **toute exécution est rejouable à l'identique**.

**Conséquence : la première tâche du harness n'est pas le simulateur, c'est l'extraction du
cœur déterministe.** Le simulateur ne peut pas exister tant que le cœur fait de l'I/O.

---

## 1. Décomposition de l'épopée (backlog ordonné)

On ne passe à la tâche suivante qu'une fois la porte d'acceptation de la précédente franchie.

- **T0.1 — Extraire le cœur déterministe sans-IO.** Refactorer ledger et consensus en une
  machine à états synchrone `handle(état, Event, rng) -> Vec<Effect>`. Aucun async, aucun
  Iroh, aucune horloge système, aucun aléa direct à l'intérieur. *(spécifiée en section 2)*
- **T0.2 — Modèle Event/Effect et abstractions Clock/Rng.** Figer les enums `Event` et
  `Effect`, et les traits `Clock` et `Rng`. *(souvent réalisée en préambule de T0.1)*
  Acceptation : les types compilent, le cœur ne référence plus aucune source de
  non-déterminisme.
- **T0.3 — Coquille de production.** Brancher le cœur sur Iroh plus tokio plus OsRng plus
  libSQL via un adaptateur qui traduit l'I/O réel en `Event` et exécute les `Effect`.
  Acceptation : comportement inchangé, **toute la suite de tests existante passe encore**.
- **T0.4 — Simulateur : horloge virtuelle plus scheduler seedé.** Boucle d'événements
  mono-thread, file de priorité `(temps_virtuel, départage déterministe, node, event)`, RNG
  seedé, ordre total déterministe. Acceptation : deux exécutions avec la même seed produisent
  une trace identique octet pour octet.
- **T0.5 — Réseau virtuel plus injection de fautes réseau.** Bus de messages avec drop,
  réordre, duplication, délai, et partition de graphe, tout piloté par le RNG seedé.
  Acceptation : chaque type de faute est activable et reproductible par seed.
- **T0.6 — Injecteur de fautes nœud plus nœuds byzantins.** Crash/redémarrage, dérive
  d'horloge, et variantes byzantines paramétrables (équivoque, rejeu, rétention de bloc).
  Acceptation : chaque variante est scénarisable et reproductible.
- **T0.7 — Vérificateurs d'invariants.** Sûreté et conservation vérifiées à chaque pas,
  vivacité vérifiée à quiescence. Sur violation : afficher la seed et la trace. *(liste en
  section 3.2)* Acceptation : un bug injecté volontairement est détecté.
- **T0.8 — Scénarios plus runner multi-seed.** Test qui balaie N seeds sur plusieurs configs
  de scénario, plus un mode replay `--seed <S>`. Acceptation : porte globale de Phase 0
  (section 4).

---

## 2. Tâche T0.1 (format gabarit, intégrale)

```
## Tâche : T0.1 — Extraire le cœur déterministe sans-IO (ledger + consensus)

### Phase
Phase 0 — Fondation de fiabilité.

### Contexte
Le ledger (`p2p/ledger.rs`, `ledger_types.rs`), le consensus PoS (`p2p/pos_consensus.rs`),
la mempool, le dispatcher (`p2p/dispatcher.rs`) et le registre @pseudo (`p2p/username.rs`)
sont aujourd'hui mêlés à de l'async (tokio), au réseau (Iroh via `willow_node.rs`,
`gossip.rs`), à l'horloge système et à de l'aléa direct. Cette tâche les isole en un cœur
synchrone et déterministe, prérequis absolu de toute simulation.

### Objectif
Produire un module `core/` (ou `sm/`) exposant une machine à états déterministe et synchrone :
`Node::handle(&mut self, event: Event, rng: &mut dyn Rng) -> Vec<Effect>`. Ce cœur contient
toute la logique de ledger, mempool, consensus et registre d'identité. Il n'effectue **aucun**
I/O et ne lit **aucune** source de non-déterminisme en interne.

### Contraintes
- Tous les invariants de la Constitution (section 3), en particulier :
  - Aucune lecture d'horloge système dans le cœur : le temps arrive via `Event::Tick { now_ms }`.
  - Aucun `OsRng`/`rand::random` dans le cœur : l'aléa passe par `&mut dyn Rng` injecté.
  - Aucun appel réseau, disque, ni async dans le cœur : ce sont des `Effect`.
  - `BTreeMap`/`BTreeSet` (ou `IndexMap` à ordre explicite) partout où l'ordre d'itération
    influence le résultat. Bannir toute dépendance à l'ordre de `HashMap`.
  - Arithmétique `checked_*` sur montants, poids, nonces.
- Ne change AUCUNE règle de protocole ni propriété de sécurité. C'est une **extraction**, pas
  une refonte de logique. Si une règle doit changer pour être extractible, **arrête-toi et
  remonte la décision** (règle d'arrêt).
- La signature reste réelle (Ed25519 plus ML-DSA-65) ; voir note 3.3 sur les variantes
  déterministes.

### Livrables attendus
- Module `core/` avec `Node`, `Event`, `Effect`, et la fonction `handle`.
- Traits `Clock` et `Rng` (T0.2 absorbée ici si pas déjà faite).
- La logique ledger/consensus/mempool/@pseudo déplacée dans le cœur, inchangée fonctionnellement.
- Property-tests de conservation rebranchés sur le cœur (Σ soldes plus brûlé == miné).
- Tests unitaires existants adaptés pour piloter le cœur via des `Event` et asserter les `Effect`.
- CHANGELOG : ce qui a bougé, ce qui n'a pas changé, confirmation qu'aucune propriété de
  sécurité n'est modifiée.

### Critères d'acceptation
- `cargo test` passe, zéro échec.
- `cargo clippy -- -D warnings` propre, `cargo fmt --check` propre.
- Le cœur ne contient AUCune occurrence de : `SystemTime`, `Instant::now`, `OsRng`,
  `rand::random`, `tokio::`, ni d'appel Iroh/libSQL (vérifiable par grep dans `core/`).
- Aucune dépendance à l'ordre d'itération de `HashMap` dans le cœur.
- Aucune arithmétique de montant non `checked_*` dans le cœur.
- `handle` est une fonction pure de (état, event, rng) : appelée deux fois sur le même état,
  le même event et un RNG dans le même état, elle produit la même sortie et la même mutation.

### Si tu rencontres un arbitrage de sécurité/consensus non tranché
Arrête-toi, remonte les options et leurs compromis. Ne devine pas.
```

---

## 3. Notes de conception transverses (s'appliquent à toute l'épopée)

### 3.1 Squelettes cibles (l'agent implémente les corps et les tests)

```rust
// ── core/event.rs : tout I/O entrant devient un Event ──────────────────────
pub enum Event {
    /// L'horloge est une ENTRÉE. Aucune lecture d'horloge système dans le cœur.
    Tick { now_ms: u64 },
    /// Un message réseau brut reçu d'un pair (le cœur fait passer le pipeline de sécurité).
    MessageReceived { from: PeerId, bytes: Vec<u8> },
    /// Une commande locale issue de l'UI (transfert, stake, enregistrement @pseudo, …).
    Command(LocalCommand),
    /// Le déclenchement d'un timer précédemment armé via Effect::SetTimer.
    TimerFired { id: TimerId },
}

// ── core/effect.rs : tout I/O sortant devient un Effect ────────────────────
pub enum Effect {
    Send { to: PeerId, bytes: Vec<u8> },
    Broadcast { bytes: Vec<u8> },
    SetTimer { id: TimerId, fire_at_ms: u64 },
    CancelTimer { id: TimerId },
    Persist { snapshot: Snapshot },
    Emit(UiEvent), // events Tauri (torus://…), traduits par la coquille
}

// ── core/rng.rs : l'aléa est injecté, jamais OsRng dans le cœur ────────────
pub trait Rng {
    fn next_u64(&mut self) -> u64;
    fn fill_bytes(&mut self, dst: &mut [u8]);
}

// ── core/node.rs : la machine à états déterministe, synchrone ──────────────
pub struct Node { /* ledger, mempool, vue consensus, registre @pseudo, … */ }

impl Node {
    /// Fonction DÉTERMINISTE. Aucun I/O, aucune horloge système, aucun OsRng.
    pub fn handle(&mut self, event: Event, rng: &mut dyn Rng) -> Vec<Effect> { /* … */ }
}
```

```rust
// ── sim/sim.rs : la coquille de simulation (code de test) ──────────────────
struct Sim {
    clock_ms: u64,
    rng: SeededRng,
    queue: BinaryHeap<Scheduled>, // ordonné par (time_ms, seq) : départage déterministe total
    seq: u64,                     // compteur monotone, tie-breaker
    nodes: BTreeMap<PeerId, Node>,
    net: VirtualNetwork,          // drop / réordre / dup / délai / partition, piloté par self.rng
    faults: FaultSchedule,        // crash, dérive horloge, byzantins
}

impl Sim {
    fn run(&mut self, seed: u64, max_steps: u64) -> Result<(), Violation> {
        // Initialiser le RNG, les clés de test (dérivées de seed → reproductible), le réseau.
        while let Some(s) = self.queue.pop() {
            if self.step >= max_steps { break; }
            self.clock_ms = s.time_ms;
            let effects = self.nodes.get_mut(&s.node)
                .expect("node existe (code de test)")
                .handle(s.event, &mut self.rng);
            self.apply_effects(s.node, effects); // → VirtualNetwork applique les fautes, planifie timers
            self.maybe_inject_faults();          // crash/partition/byzantin, piloté par self.rng
            self.check_invariants(seed)?;        // sûreté + conservation à CHAQUE pas
        }
        self.check_liveness_at_quiescence(seed) // vivacité une fois le réseau réparé
    }
}
```

Le départage des events à temps virtuel égal **doit** être total et déterministe : ordonner par
`(time_ms, seq)` où `seq` est un compteur monotone d'insertion. Sans ça, le simulateur n'est
pas reproductible.

### 3.2 Liste des invariants à vérifier (T0.7)

Sûreté, vérifiée à chaque pas :
- **Conservation** : Σ soldes de tous les comptes plus total brûlé == total miné. C'est ton
  property-test existant, désormais exécuté en continu dans la simulation.
- **Pas de double-dépense** : un hash de tx n'est jamais appliqué deux fois ; nonce strictement
  monotone par compte.
- **Cohérence des nœuds honnêtes** : deux nœuds honnêtes ne finalisent jamais deux blocs
  différents à la même hauteur, et ne divergent jamais sur leur préfixe finalisé.
- **Borne d'émission** : plafond par bloc respecté et plafond global jamais dépassé.

Vivacité, vérifiée à quiescence (après réparation des partitions et fin des fautes) :
- **Progression** : la hauteur de chaîne avance sur tous les nœuds honnêtes.
- **Convergence** : tous les nœuds honnêtes convergent vers la même chaîne finalisée.

Sur violation : afficher `seed`, le numéro de pas, et un dump compact de la trace (le journal
des events), pour rejouer via `cargo test … -- --seed <S>` ou une variable d'environnement.

### 3.3 Crypto en simulation (fidélité plus reproductibilité)
- Utiliser les **variantes déterministes** de signature : Ed25519 (RFC 8032) est déjà
  déterministe (pas de RNG au signing), bonne nouvelle. Pour ML-DSA-65 (FIPS 204), utiliser la
  **variante déterministe** plutôt que la variante hedged, sinon les signatures ne sont pas
  reproductibles. Documenter ce choix.
- Dériver les **clés de test de la seed de simulation**, pour que tout le run soit
  reproductible depuis la seed unique.
- Garder la **crypto réelle par défaut** (fidélité). Prévoir un **mode rapide** optionnel qui
  court-circuite la vérification de signature, réservé aux runs à grande échelle où le coût CPU
  domine ; le mode rapide ne doit jamais être celui qui valide une propriété de sécurité.

### 3.4 Modélisation du disque (recovery)
La persistance libSQL est un I/O, donc un `Effect::Persist`. En simulation, modéliser le
stockage par un store en mémoire déterministe, et injecter des fautes « crash avant persist » et
« crash après persist » pour tester la reprise sur snapshot. C'est ainsi qu'on attrape les bugs
de recovery sans toucher au vrai disque.

---

## 4. Porte d'acceptation globale de la Phase 0 (T0.8)

La Phase 0 est terminée quand **toutes** ces conditions sont démontrées :

- **Le harness a des dents** : on réintroduit volontairement un bug connu (par ex. l'exemption
  de signature `to == "BURN"` de AUDIT-TX-1, ou une soustraction non `checked_*`), et le
  vérificateur de conservation ou de sûreté le **détecte**, en affichant une seed qui
  **reproduit** l'échec à l'identique. Puis on retire le bug et le test redevient vert.
- **Reproductibilité** : deux exécutions sur la même seed produisent une trace identique octet
  pour octet.
- **Scénario heureux multi-seed** : N seeds (par ex. 10 000) passent sur le scénario de
  convergence nominal, avec conservation et sûreté vérifiées à chaque pas.
- **Au moins un scénario adverse** passe ou échoue de façon attendue et documentée (partition
  réseau puis réparation, avec vérification de vivacité à quiescence).
- `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo audit` propres.

Une fois cette porte franchie, tu as le banc d'essai qui rendra sûres les Phases 1 à 4. Un L1
déterministiquement simulé est déjà, en soi, un niveau de fiabilité que presque aucune chaîne
n'atteint.
