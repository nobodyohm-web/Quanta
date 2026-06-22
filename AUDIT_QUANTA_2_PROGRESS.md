# Suivi de remédiation — AUDIT_QUANTA_2.md

> Suivi des actions de l'audit. Mis à jour à chaque itération de la boucle `/loop`.
> Légende : ✅ fait · ⏳ à faire · 🙋 décision utilisateur requise.

## Itération 1 — Honnêteté & purge du poids mort (✅ build vert vérifié)

| # | Sujet | Statut |
|---|-------|--------|
| 1.1 | Prix EUR inventé retiré de `quanta-identity-preview.html` | ✅ |
| 1.3 | `DEV_API.md` supprimé (git rm) | ✅ |
| 1.3 | « web » retiré de `package.json` description + `NOTICE` | ✅ |
| 1.3 | CLAUDE.md NET-7 (DAG social) marqué retiré | ✅ |
| 4.1 | Tailwind retiré (`@theme`→`:root`, plugin vite, deps) — CSS vanilla réel | ✅ |
| 4.2 | `marked` / `dompurify` / `@types/dompurify` retirés (zéro import) | ✅ |
| 4.3 | `lucide-svelte` retiré (zéro import → règle la provenance) | ✅ |
| 4.4 | CLAUDE.md ref dmg `0.1.0`→`3.3.0` (tauri.conf.json déjà 3.3.0) | ✅ |
| 4.6 | `NOTICE` : « web » corrigé + `fips204` ajouté | ✅ |

Vérif : `npm run build` ✅ · `svelte-check` 0 erreur ✅ · deps retirées du lockfile ✅.

## Itération 2 — Honnêteté doc + 1ʳᵉ correction code (✅ `cargo test` 174 vert)

| # | Sujet | Statut |
|---|-------|--------|
| 3.1 | « Shapley » → pondération de contribution honnête (inspirée, pas exacte) — WP EN/FR + README | ✅ |
| 3.2 | « déflationniste » → conditionnel (net-déflationniste au-delà d'un seuil de volume) | ✅ |
| 3.3 | Table calendrier d'émission (an 1 ≈1%, ~66 ans = moitié) ; « front-loaded » = *rythme* | ✅ |
| 1.4 | DAG social (retiré) ≠ DAG-BFT (consensus, roadmap) — clarifié WP EN/FR + README | ✅ |
| 2.7 | **Fix code** : Merkle BLAKE3 à séparation de domaine RFC 6962 (feuille `0x00`, nœud `0x01`, plus de duplication du nœud impair) → ferme CVE-2012-2459 | ✅ |
| 2.2 | « VRF » : doc corrigée (vraie formule beacon enterré) + **honnêteté : ce n'est PAS un VRF** (pas de clé secrète → leader prévisible) — WP EN/FR + CLAUDE.md + docstring `pos_consensus.rs` | ✅ (doc) |
| 2.1/2.3/2.4 | **Disclosure honnête** ajoutée (WP §9) : pas de slashing, éclipse faible (préfixe seul), gossip non filtré sybil, leader prévisible | ✅ (disclosure ; fixes code = roadmap) |

## Itération 3 — Intégrité du code (✅ `cargo test` 175 vert · clippy `-D warnings` clean)

| # | Sujet | Statut |
|---|-------|--------|
| 2.7 (crypto) | **Confirmé sûr** : XOF a une séparation de domaine (`ML_DSA_DOMAIN` préfixé avant la graine, `hybrid_crypto.rs:167`) ; nonce GCM = `OsRng` aléatoire 96-bit par chiffrement (`cipher.rs:27`) → pas de réutilisation. Aucun changement requis. | ✅ |
| 3.4 | **Fix code** : bonus de réputation **plafonné au stake** (`weight()` = stake + min(rep·10_000, stake)) → élection ancrée au stake, réputation ne peut au plus que *doubler* le poids. + test `reputation_bonus_capped_at_stake`. | ✅ |
| 2.8 (a) | **Fix code** : `balance_of` **logge la violation d'invariant** si un solde *utilisateur* devient négatif (au lieu de saturer en silence ; NETWORK/ESCROW/BURN exemptés car légitimement négatifs). Arithmétique : cache `i128` + offre bornée ≪ u64 + ops saturantes au consensus → pas de wrap silencieux. | ✅ |
| bonus | Clippy : corrigé un warning pré-existant `manual_range_contains` (`username.rs`) → `ai:check` clean. | ✅ |

## Prochaines itérations — à faire

### Intégrité du code (reste) ⏳
- **2.8 (b)** **Élaguer `seen_tx_hashes`** — ⚠️ rôle DOUBLE : (1) dédup anti-replay, (2) comptabilité fork-reorg (retiré au pop, `ledger.rs:112`). Un fenêtrage naïf casserait la reorg. Approche sûre = n'élaguer que les hashes de blocs *enterrés au-delà de la profondeur max de reorg* (le nonce monotone garde le replay des transferts). **Nécessite design soigné + test de régression + idéalement le testnet chaos (§5).** Reporté volontairement (risque consensus).
- **2.6** Bump `iroh` (DoS hickory RUSTSEC-2026-0119/0120 sur chemin DNS live) ou désactiver la découverte DNS. ⚠️ risque de rupture d'API → turn dédié.
- **4.5** Branding : la doc user-facing dit déjà « Quanta » (titre README, whitepaper, `name` package). Reste surtout le **rename du repo GitHub** (action utilisateur) ; URLs/chemins réels (`audit/Torus-Audit-360.html`) gardés ; `.torus`/`TORUS_PROTOCOL_VERSION`/`torus://` = wire interne intentionnel.

### Sécurité lourde (effort élevé — fixes code) ⏳
- **2.1** Slashing / preuve d'équivocation (nothing-at-stake) — disclosure faite, **fix code à construire**.
- **2.2 (code)** Vrai VRF à clé secrète (ECVRF) + VDF anti-grinding — disclosure faite, **fix code à construire**.
- **2.3** Vraie résistance eclipse : diversité IP/AS, ancres persistantes, buckets.
- **2.4** Résistance Sybil de la couche gossip (puzzle d'admission léger / scoring peers) ; tester le dimensionnement LRU.

### Décisions utilisateur requises 🙋
- **1.2 Updater** : `@tauri-apps/plugin-updater` est câblé (endpoint HTTPS GitHub) → contredit « aucune requête HTTP sortante » (SECURITY.md règle 5, frontend rule 7) et l'argument deny.toml. **Choix** : (a) le retirer (cohérent « souverain, sans serveur », maj manuelle via release signée) ou (b) le garder et reformuler honnêtement les règles + deny.toml. Recommandation : (a).
- **Portée sécurité lourde** : slashing + vrai VRF + diversité eclipse sont de gros chantiers (semaines). Confirmer l'ordre de priorité.

### Vision (exceptionnel) 🔵 ⏳
Fuzzing `cargo-fuzz` du parseur d'enveloppes · testnet chaos multi-nœuds (byzantin) · model-checking (Stateright/TLA+) · récupération BIP39 · light client/SPV · marché de frais anti-spam · confidentialité ZK · audit tiers · builds reproductibles + release notarisée.

---

# Constitution d'ingénierie (`QUANTA_AGENT_CONSTITUTION.md`) — application phase par phase

> Méthodologie : spec d'abord · tests adverses d'abord · **incrément minimal vérifiable** ·
> règle d'arrêt sur tout arbitrage consensus/sécurité non tranché.

## État Phase 0 (fondation de fiabilité) — déjà en place (constaté à l'audit déterminisme)

| Élément Phase 0 | État constaté |
|---|---|
| Harness de simulation déterministe | ⏳ **partiel** : `p2p::simulation::network_simulation::simulation_reseau` existe (réseau virtuel). Manque : horloge virtuelle injectée + RNG seedé + injecteur byzantin paramétrable + rejouabilité par seed. |
| Property-tests de conservation (Σ soldes + brûlé = miné) | ✅ `proptest_transfers_conserve_total`, `proptest_transfers_with_burn_conserve_total`, `proptest_nonce_monotonic_no_overspend` |
| Fuzzing parseur d'enveloppes (`cargo-fuzz`) | ✅ `src-tauri/fuzz/fuzz_targets/gossip_envelope.rs` (réexport `fuzz_parse_gossip` dans `lib.rs`) |
| Audit déterminisme du consensus | ✅ **fait ci-dessous** |
| `clippy -D warnings` | ✅ propre |
| Builds reproductibles | ⏳ à brancher |
| Spec formelle (TLA+/Stateright) | ⏳ à poser |

## Audit de déterminisme — chemin consensus (`pos_consensus.rs`) + ledger

| Source de non-déterminisme | Verdict |
|---|---|
| **Aléa direct** (`OsRng`/`rand::random`) dans le consensus | ✅ **aucun** — élection = BLAKE3 pur sur beacon enterré. |
| **Ordre d'itération `HashMap`** dans l'élection | ✅ **sûr** — `build_validator_set` trie les clés (`all_pks.sort()`), `elect_leader`/`elect_fallback_leader` re-trient l'ensemble éligible (`sort_by(pk)`). Verrouillé désormais par property-test (voit incrément ci-dessous). |
| **Horloge système** dans `pos_consensus.rs` | ✅ **aucune**. |
| **Horloge système** dans `ledger.rs` | ⚠️ présente : estampillage tx/bloc (création) + contrôle de dérive tx (`ledger.rs:272`). À injecter via abstraction d'horloge **avec le harness** (tâche dédiée, plus large). Pas un bug de divergence isolé : c'est l'item « horloge injectée » de Phase 0. |
| Flottants sur montants/poids | ✅ µQTA `u64`/`u128` partout (Shapley utilise `f64` pour des *scores* normalisés, hors chemin soldes). |

## Incrément Phase 0 #1 — Durcir l'arithmétique & le déterminisme de l'élection ✅

**Quoi.** `elect_leader` / `elect_fallback_leader` (`pos_consensus.rs`) :
- Accumulation des poids passée de `u64` à **`u128`** (`total_weight`, `cumulative`, `target`).
- Suppression des deux **`unwrap()`** terminaux → `eligible.last().map(...)` (sans panique).

**Pourquoi.** Constitution §3 :
- *Arithmétique vérifiée* : `eligible.iter().map(weight).sum::<u64>()` paniquait en debug / wrappait en release si la somme des poids dépassait `u64::MAX`. En `u128` l'overflow devient **inatteignable** (poids ≤ `u64::MAX`, nombre de validateurs borné ⇒ somme ≪ `2^128`) — donc plus de wrap silencieux ni de panique.
- *Robustesse Rust* : zéro `unwrap()` en production.

**Propriétés affectées.** **Aucune régression de comportement consensus** : pour tout ensemble réaliste (`total_weight ≤ u64::MAX`, toujours vrai car Σ stakes ≤ offre 100M µQTA et bonus rép. plafonné au stake ⇒ poids ≤ 2× stake), le leader élu est **bit-identique** à l'ancienne arithmétique `u64`. Seul le domaine d'overflow (inatteignable en pratique) change : il n'erre plus, il calcule juste correctement.

**Tests livrés (adverses inclus).**
- `elect_leader_is_permutation_invariant` (**proptest, 512 cas**) — encode l'**invariant de déterminisme #1** : permuter l'ordre d'entrée des validateurs ne change jamais le leader (sinon deux nœuds honnêtes forkent). Garde contre un futur refactor qui retirerait le tri interne.
- `elect_leader_handles_extreme_weights_without_overflow` — cas adverse : deux « baleines » à `u64::MAX/2` dont la somme des poids dépasse `u64::MAX` (l'ancien `sum::<u64>()` paniquait en debug). Vérifie élection + chemin fallback sans panique.

**Portes de vérification.** `cargo test --lib` → **176 passed / 0 failed** · `cargo clippy --lib -D warnings` → propre.

**Auto-revue invariants §3 :** déterminisme ✅ (tri préservé + verrouillé par test ; aucun nouvel usage d'ordre HashMap/horloge/aléa) · arithmétique ✅ (`u128`, overflow inatteignable, zéro wrap/saturation silencieuse sur les poids) · robustesse Rust ✅ (zéro `unwrap`/`expect`/`panic`/`unsafe` ajouté) · mémoire bornée ✅ (aucune structure ajoutée) · sécurité réseau ✅ (chemin gossip intact) · tests livrés ✅ (property + adverse).

> **Dette honnête signalée (pas modifiée en silence) :** le dépôt n'est **pas** `cargo fmt`-propre à HEAD (la `rustfmt.toml` active des options *nightly-only* ; même `cargo +nightly fmt --check` réécrit des lignes non touchées par cet incrément). Une passe `cargo +nightly fmt` à l'échelle du dépôt est une tâche de ménage **séparée** (gros diff cosmétique) — délibérément hors périmètre de cet incrément consensus.

## Reste de Phase 0 — prochains incréments (sûrs, autonomes)
- Compléter le harness : **horloge virtuelle injectée** (remplacer `Utc::now()` du chemin validation `ledger.rs:272` par une horloge passée en paramètre) + **RNG seedé** + **injecteur byzantin** (équivoque/rejeu/rétention) avec **rejouabilité par seed**. ⚠️ touche le ledger → incréments minimaux + property-tests.
- **2.8(b)** Élaguer `seen_tx_hashes` (mémoire bornée §3) — toujours subordonné au harness chaos (rôle double dédup/reorg).
- Poser le **squelette de spec formelle** (Stateright) avec sûreté/vivacité actuelles.

## Phases 1–3 — RÈGLE D'ARRÊT (§4) : décision humaine requise 🛑
La Constitution interdit de deviner sur ces arbitrages (une mauvaise hypothèse peut tuer le réseau) :
- **Phase 1** : vrai VRF à clé secrète (ECVRF) + entropie profonde anti-grinding ; **détection/pénalisation d'équivocation** (modèle de slashing à choisir).
- **Phase 2** : saut de consensus **DAG-BFT** (Narwhal/Bullshark) + agrégation **BLS** + modèle de finalité — *« décision d'architecture remontée et validée par l'humain avant implémentation »*.
- **Phase 3** : preuve de contribution résistante au Sybil sans autorité (l'oracle d'énergie auto-déclaré est spoofable) — modèle de menace à arbitrer.
- **1.2 Updater** (héritée) : `@tauri-apps/plugin-updater` vs « aucune requête HTTP sortante » — choix (a) retirer / (b) reformuler les règles.

---

# Épopée Harness DST (`QUANTA_T0_DST_HARNESS.md`) — Phase 0, tâche T0.1

> Pattern **sans-IO** : cœur fonctionnel déterministe (`Node::handle(état, Event, &mut Rng) -> Vec<Effect>`)
> + coquille impérative (prod = Iroh/tokio/OsRng/libSQL ; sim = réseau virtuel/horloge virtuelle/RNG
> seedé/fautes). Le cœur ne lit jamais l'horloge, n'appelle jamais `OsRng`, ne touche ni réseau ni disque.
> T0.1 = grosse extraction ⇒ livrée en **tranches vérifiables** (Constitution §4/§8 : jamais tout d'un coup).

## T0.1 — tranche 1 : fondation sans-IO (types de frontière + abstractions) ✅

**Quoi.** Nouveau module **`sm/`** (state machine ; nommé `sm` et non `core` pour ne pas masquer le crate `::core` du prélude — la spec autorise « `core/` ou `sm/` »), branché `pub mod sm;` dans `lib.rs`. Contenu :
- `sm/event.rs` — `Event` (`Tick{now_ms}`, `MessageReceived{from,bytes}`, `Command(LocalCommand)`, `TimerFired{id}`), `PeerId`, `TimerId`, `LocalCommand` (Transfer/Stake/RegisterUsername). **Le temps est une ENTRÉE** (`Tick`), jamais une lecture d'horloge.
- `sm/effect.rs` — `Effect` (`Send`, `Broadcast`, `SetTimer`, `CancelTimer`, `Persist`, `Emit`), `Snapshot`, `UiEvent`. **Tout I/O sortant = donnée inspectable** (la coquille sim peut droper/réordonner/retarder pour injecter des fautes).
- `sm/rng.rs` — trait `Rng { next_u64, fill_bytes }` (object-safe `&mut dyn Rng`) + `Blake3Rng` déterministe (BLAKE3 XOF en mode compteur, domaine `QUANTA-sim-rng-v1`). **Aucune crypto inventée** (§8) : BLAKE3 est déjà une dép. PRNG de *déterminisme de simulation*, pas un substitut de CSPRNG en prod.
- `sm/clock.rs` — trait `Clock { now_ms }` + `ManualClock` (le temps n'avance que sur ordre de la coquille).

**Pourquoi.** Prérequis absolu du simulateur (§0 de la spec : « la première tâche n'est pas le simulateur, c'est l'extraction du cœur déterministe »). Cette tranche pose le langage `Event`/`Effect` et les sources d'aléa/temps injectées ; les tranches suivantes migreront ledger/consensus/mempool/@pseudo derrière `Node::handle`.

**Propriétés affectées.** **Aucune** : module purement additif, aucune ligne de `p2p` touchée, comportement réseau/consensus inchangé. Les `p2p::*` restent la source de vérité jusqu'à la migration.

**Tests livrés (11).** RNG : reproductibilité même-seed (1000 tirages), divergence seeds différentes, `fill_bytes` déterministe à cheval sur la frontière de bloc 64 o, cohérence `fill_bytes`/`next_u64` (même flux), flux non constant. Clock : avance contrôlée + saturation sans panique. Event/Effect : ordre total des `PeerId` (clés `BTreeMap`), effets = données comparables.

**Portes de vérification.** `cargo test --lib` → **187 passed / 0 failed** (était 176 ; +11) · `clippy --lib -D warnings` → propre · 5 fichiers `sm/` **nightly-fmt propres** (formatés à l'unité, sans toucher le reste du dépôt) · **grep sans-IO** : zéro occurrence réelle de `SystemTime`/`Instant::now`/`OsRng`/`rand::random`/`tokio::`/Iroh/libSQL/`HashMap`/`HashSet` dans `sm/` (seules restent des mentions en doc-comment décrivant ce que le cœur *évite*).

**Auto-revue invariants §3 :** déterminisme ✅ (temps = `Event::Tick`, aléa = `&mut dyn Rng`, `BTreeMap`-ready via `PeerId: Ord` ; zéro horloge/OsRng/HashMap dans le cœur) · arithmétique ✅ (compteur PRNG = index de flux documenté `wrapping`, horloge `saturating` documentée ; aucune arithmétique de *montant* introduite) · robustesse Rust ✅ (zéro `unwrap`/`expect`/`panic` hors tests, zéro `unsafe`) · mémoire bornée ✅ (buffer RNG fixe 64 o) · sécurité réseau ✅ (frontière `MessageReceived` documentée pour passer le pipeline avant toute confiance ; rien câblé encore) · tests livrés ✅.

## T0.1 — tranche 2 : `Node::handle` + encapsulation du `Ledger` (temps injecté) ✅

**Quoi.**
- Nouveau **`sm/node.rs`** : `Node { ledger: Ledger, now_ms: u64 }` + `handle(&mut self, Event, &mut dyn Rng) -> Vec<Effect>`. `Event::Tick{now_ms}` → avance le temps virtuel (monotone) **et** élague la mempool de façon déterministe via le temps injecté ; les autres events (`MessageReceived`/`Command`/`TimerFired`) sont **inertes documentés** (branchés aux tranches suivantes). Accesseurs `ledger()`/`ledger_mut()` (transitoire) + `now_ms()`.
- **Extraction d'horloge — ledger** (extraction, pas refonte) :
  - `Ledger::prune_mempool_at(now_secs: i64)` = la vraie logique d'éviction à **temps injecté** ; `prune_mempool()` devient un mince wrapper de production qui lit l'horloge **à la frontière**. Le cœur n'appelle que `_at`.
  - **Timestamp de genèse figé** (`GENESIS_TIMESTAMP` const) au lieu de `Utc::now()` : retire une lecture d'horloge du constructeur **et** corrige une non-déterminisme latent (le bloc de genèse différait par nœud selon l'horloge murale, alors que son *hash* est déjà constant). Zéro impact consensus.

**Pourquoi.** Spec §0 : « la première tâche n'est pas le simulateur, c'est l'extraction du cœur déterministe ». Cette tranche pose `handle` et démontre le pattern Event→état→(Effect) de bout en bout sur un chemin réel (le temps) sans toucher la logique ledger. Constitution §4/§8 (extraction par tranches, jamais tout d'un coup).

**Propriétés affectées.** **Aucune régression** : la logique de transfert/burn/mint/éviction est inchangée (seule la *source du temps* d'éviction est désormais paramétrable ; `prune_mempool()` se comporte à l'identique). Genèse : `timestamp` désormais constant (champ métadonnée, hors hash) → strictement plus déterministe. Module `sm/` toujours additif ; `p2p` reste la source de vérité tant que les mutations ne passent pas par `handle`.

**Tests livrés (5).** `sm::node` : avance monotone du temps virtuel (tick périmé ignoré) ; events inertes = no-op ; **conservation à travers le cœur** (Σ soldes + brûlé == miné après mint+transferts+5 ticks d'éviction) ; **déterminisme** (deux cœurs, même séquence d'events ⇒ état observable identique : temps, hauteur, soldes triés). `ledger` : `prune_mempool_at` **piloté par le temps injecté** (même état, deux temps injectés différents ⇒ résultats différents : sous-TTL = 0 évincé, sur-TTL = 1).

**Portes de vérification.** `cargo test --lib` → **192 passed / 0 failed** (était 187 ; +5) · `clippy --lib -D warnings` propre · `sm/node.rs` + `sm/mod.rs` **nightly-fmt propres** · **grep sans-IO** sur `sm/` : zéro lecture réelle d'horloge/`OsRng`/`tokio::`/`.await`/Iroh/libSQL (les lectures d'horloge résiduelles vivent dans les *wrappers de production* `p2p`, hors du cœur).

**Auto-revue invariants §3 :** déterminisme ✅ (temps = `Event::Tick` injecté + monotone ; éviction via `_at` ; genèse figée ; aucun `Utc::now`/`OsRng`/`HashMap`-order dans `sm/`) · arithmétique ✅ (cast `now_ms/1000` borné ; aucune arithmétique de *montant* nouvelle) · robustesse Rust ✅ (zéro `unwrap`/`expect`/`panic`/`unsafe` hors tests) · mémoire bornée ✅ (rien ajouté) · sécurité réseau ✅ (frontière `MessageReceived` documentée, non encore branchée) · tests livrés ✅ (conservation rebranchée sur le cœur + déterminisme + temps injecté).

## T0.1 — tranche 3 : `Event::MessageReceived` → portail de sécurité à temps injecté ✅

**Quoi.**
- **Extraction d'horloge — freshness** (mirror de `prune_mempool_at`) : `GossipRouter::is_fresh_at(timestamp, now_secs)` = la logique de fenêtre ±90s à **temps injecté** (pur, sans lecture d'horloge) ; `is_fresh()` devient le wrapper de production qui lit l'horloge à la frontière.
- **Validateur d'enveloppe pur** : `dispatcher::validate_envelope_at(data, now_secs) -> Result<GossipEnvelope, String>` = les étapes **sans-état** du pipeline de réception (taille → décode JSON → freshness *injectée* → signature Ed25519). `try_process_raw_gossip` (cible fuzz) délègue désormais à `validate_envelope_at(..., now_epoch_secs())` → comportement de prod inchangé. Les étapes **à état** (ban, dédup, rate-limit, nonce) restent dans `dispatch_incoming` (coquille) jusqu'à une tranche ultérieure.
- **Branchement cœur** : `Node::handle(Event::MessageReceived{from, bytes})` → `on_message` exécute le portail `validate_envelope_at` avec `self.now_ms` ; octets bruts jamais de confiance, tout échec = drop silencieux. Le dispatch du payload (mutation ledger/consensus → `Effect`) arrive en tranche suivante.

**Pourquoi.** La freshness lisait `Utc::now()` : deux nœuds à horloges décalées pouvaient diverger sur la validité d'un message entrant — exactement le non-déterminisme que le harness cible. L'injecter rend la validation rejouable. C'est aussi le 1ᵉʳ chemin réseau branché sur `handle` (le simulateur pourra nourrir le cœur d'octets non fiables de façon déterministe).

**Propriétés affectées.** **Aucune régression** : mêmes contrôles, même ordre (freshness avant signature) ; seule la *source du temps* de freshness devient paramétrable. `try_process_raw_gossip`/`dispatch_incoming` se comportent à l'identique (wrapper lit l'horloge). Aucune règle de protocole/sécurité modifiée.

**Tests livrés (3).** `gossip::is_fresh_at_is_injected_time_driven` (frais à ±90s du temps injecté, périmé au-delà, date invalide = jamais frais). `dispatcher::validate_envelope_at_uses_injected_time_for_freshness` (**mêmes octets signés**, temps injecté dans la fenêtre ⇒ `Ok`, hors fenêtre ⇒ `Err("stale message")`). `sm::node::message_received_drops_untrusted_bytes_without_panic` (octets hostiles : vide/garbage/JSON invalide/2 KB ⇒ zéro effet, zéro mutation, zéro panique).

**Portes de vérification.** `cargo test --lib` → **195 passed / 0 failed** (était 192 ; +3) · `clippy --lib -D warnings` propre · `sm/node.rs` nightly-fmt propre · **grep sans-IO** sur `sm/` : zéro lecture réelle d'horloge/IO (le cœur appelle `validate_envelope_at` à temps injecté ; les wrappers d'horloge vivent dans `p2p`).

**Auto-revue invariants §3 :** déterminisme ✅ (freshness à temps injecté ; `on_message` n'utilise que `self.now_ms` ; aucun `Utc::now`/`OsRng` dans `sm/`) · arithmétique ✅ (aucune arithmétique de montant nouvelle ; cast borné) · robustesse Rust ✅ (`validate_envelope_at` ne panique jamais ; zéro `unwrap`/`unsafe` hors tests ; erreurs opaques préservées) · sécurité réseau ✅ (**octets bruts validés avant toute confiance**, ordre du pipeline préservé) · mémoire bornée ✅ (rien ajouté) · tests livrés ✅ (freshness injectée + drop hostile sans panique).

## T0.1 — tranche 4 : dispatch du payload `BroadcastTx` dans le cœur ✅

**Quoi.**
- **Extraction du chemin ledger linéaire** (source unique de vérité, extraction pas refonte) — deux méthodes `Ledger` :
  - `apply_verified_remote_tx(tx) -> bool` = replay idempotent + avance de nonce monotone (la partie (D)+(E) de `handle_broadcast_tx`). Préserve **exactement** le comportement : les txs à émetteur synthétique (`NETWORK`/`ESCROW`) sont un no-op ici (elles entrent par sync de bloc).
  - `apply_remote_tx_checked(tx) -> bool` = signature (`verify_tx`) → barrière de nonce monotone → `apply_verified_remote_tx`. Pur & synchrone (le cœur admet les txs gossip de façon déterministe).
- **Refonte du shell** `handle_broadcast_tx` : le bloc replay+nonce inline est remplacé par `ledger.apply_verified_remote_tx(tx)` (même garde `if !synthetic`, même verrou) → **un seul** chemin partagé cœur/prod. CRDT + locks async restent dans le shell.
- **Branchement cœur** : `Node::on_message` — après le portail `validate_envelope_at`, `GossipMessage::BroadcastTx { tx_json }` → parse → `apply_remote_tx_checked`. **1ʳᵉ mutation réelle du ledger depuis une entrée réseau** à travers `handle`.

**Pourquoi.** Faire admettre au cœur une tx vérifiée (état réel), en partageant la logique avec la prod pour éviter toute divergence sim/prod. La barrière `tx.nonce + 1` est passée en **`saturating_add`** (Constitution §3 : arithmétique de nonce non-wrappante ; identique pour tout nonce réaliste).

**Propriétés affectées.** **Aucune régression** : `handle_broadcast_tx` se comporte à l'identique (replay+avance via la méthode extraite ; CRDT, verrous, ordre verify→gate→CRDT→apply inchangés). Aucune règle de protocole/sécurité modifiée. `apply_remote_tx_checked` conserve AUDIT-TX-1 (signature même pour `to=="BURN"`) et AUDIT-TX-2 (non-régression de nonce).

**Tests livrés (1 cœur + couverture shell inchangée).** `sm::node::broadcast_tx_is_applied_to_ledger_through_core` : une tx de transfert **signée**, emballée dans une enveloppe gossip **signée**, reçue par un cœur neuf à temps injecté ⇒ le destinataire est crédité du net (après burn) ; **rejeu = no-op** (dédup idempotente). Les tests existants du dispatcher exercent le shell refondu via la même méthode.

**Portes de vérification.** `cargo test --lib` → **196 passed / 0 failed** (était 195 ; +1) · `clippy --lib -D warnings` propre · `sm/node.rs` nightly-fmt propre · **grep sans-IO** sur `sm/` : zéro lecture réelle d'horloge/IO.

**Auto-revue invariants §3 :** déterminisme ✅ (admission tx à temps injecté + `verify_tx`/replay purs ; aucun `Utc::now`/`OsRng`/`HashMap`-order dans le cœur) · arithmétique ✅ (barrière nonce `saturating_add` ; `replay`/cache inchangés) · robustesse Rust ✅ (zéro `unwrap`/`unsafe` hors tests ; `apply_*` ne paniquent pas ; erreurs opaques) · sécurité réseau ✅ (signature + barrière nonce avant toute mutation ; octets bruts non fiables) · mémoire bornée ✅ (rien ajouté ; `seen_tx_hashes` toujours à élaguer — suivi 2.8b) · tests livrés ✅.

## T0.1 — tranche 5 : 1ᵉʳ `Effect` sortant du cœur — `Ping → Pong` signé ✅

**Quoi.**
- **Identité de signature dans le cœur** : `Node` gagne `identity: Option<CryptoEngine>` + `out_nonce: u64`. Constructeur `Node::with_identity(crypto)` (la prod passe l'engine réel ; la sim une identité **dérivée de seed** via `import_keypair`, déterministe). `Node::new()`/`from_ledger()` = mode observateur (sans identité). Ed25519 = signature déterministe (harness §3.3) ⇒ trafic sortant reproductible.
- **Helper d'émission** `sign_broadcast(msg) -> Option<Effect>` : timestamp dérivé de `self.now_ms` (`millis_to_rfc3339`, **pur**, pas d'horloge), nonce sortant monotone du cœur, signature Ed25519 → enveloppe signée → `Effect::Broadcast{bytes}`. Nonce consommé **seulement** si le message est entièrement construit (sinon `None`, sans effet).
- **Handler** : `GossipMessage::Ping{nonce}` → `on_ping` → `Effect::Broadcast(Pong{nonce})`. Reproduit le `handle_ping` de prod (qui *broadcast* un Pong). `on_message` restructuré en `match` (BroadcastTx + Ping + `_`).

**Pourquoi.** 1ᵉʳ message **sortant** produit par le cœur : prouve le cycle complet Event→état→`Effect` sortant, et que le cœur peut signer de façon déterministe sans horloge ni `OsRng`. Le simulateur pourra router ces bytes vers d'autres nœuds.

**Propriétés affectées.** **Aucune** côté prod (le shell `handle_ping` est inchangé ; le cœur reste sim-only). Nonce sortant en `saturating_add` (§3). `millis_to_rfc3339` ne panique jamais (hors-plage → chaîne vide → freshness rejette).

**Tests livrés (3).** `ping_yields_a_valid_signed_pong_broadcast` : un Ping ⇒ exactement 1 `Effect::Broadcast` dont les bytes **repassent `validate_envelope_at`** (signature + freshness valides) et portent `Pong{1234}` signé par **nous**. `ping_without_identity_emits_nothing` (observateur ne peut pas signer). `pong_emission_is_deterministic` (bytes **identiques** sur deux exécutions : signature déterministe + temps injecté + nonce déterministe).

**Portes de vérification.** `cargo test --lib` → **199 passed / 0 failed** (était 196 ; +3) · `clippy --lib -D warnings` propre · `sm/node.rs` nightly-fmt propre · **grep sans-IO** sur `sm/` : **zéro** lecture réelle d'horloge/`OsRng`/`tokio`/`.await`/IO (l'identité tient une `CryptoEngine` de `security/`, mais le cœur n'appelle que `sign`/`get_identity`, déterministes).

**Auto-revue invariants §3 :** déterminisme ✅ (signature Ed25519 déterministe, identité seedée, timestamp injecté, nonce monotone du cœur ; zéro horloge/`OsRng` exécuté dans `sm/`) · arithmétique ✅ (`out_nonce` `saturating_add` ; rien d'autre) · robustesse Rust ✅ (`sign_broadcast` tout en `?`/`Option`, zéro `unwrap`/`unsafe` hors tests, jamais de panique) · sécurité réseau ✅ (le cœur **signe** ses sorties ; Pong rejoue le nonce du Ping comme la prod) · mémoire bornée ✅ (un `u64` + une identité) · tests livrés ✅ (validité + déterminisme du Pong).

## T0.1 — tranche 6 : intégration de bloc `NewBlock` dans le cœur (consensus-critique) ✅

**Quoi.** `Node::on_message` gagne l'arm `GossipMessage::NewBlock{block_json}` → parse `Block` → `self.ledger.integrate_remote_block(block)`. **Aucune refonte du shell** : `handle_new_block` appelait déjà `integrate_remote_block` directement — c'est **déjà** la source unique de vérité (validation structurelle + crypto + résolution de fork à l'intérieur). Le cœur appelle simplement la même méthode.

**Pourquoi.** Faire intégrer au cœur un bloc scellé reçu = chemin consensus complet (le plus critique). Vérifié **clock-free / RNG-free** : grep dans `integrate_remote_block` → zéro `Utc::now`/`OsRng`/`SystemTime` ⇒ consensus déterministe, intégrable au cœur sans abstraction. **Pas d'arbitrage consensus déclenché** (extraction pure) ⇒ règle d'arrêt §4 non applicable.

**Propriétés affectées.** **Aucune** : `integrate_remote_block` inchangé (AUDIT-BLK-1/2 préservés : validation pré-mutation, fork reorg sans perte de tx) ; le compteur `blocks_validated` (métrique Shapley) reste dans le shell ; pas de re-broadcast (comme la prod). Cœur toujours sim-only.

**Tests livrés (2).** `new_block_is_integrated_through_core` : un bloc scellé par une identité, emballé dans une enveloppe `NewBlock` signée, reçu par un cœur neuf ⇒ la chaîne s'étend d'un cran, tip == hash du bloc. `new_block_with_broken_link_is_rejected_by_core` : un bloc au `prev_hash` cassé est **rejeté** (chaîne inchangée) ⇒ la validation consensus est bien appliquée à travers le cœur.

**Portes de vérification.** `cargo test --lib` → **201 passed / 0 failed** (était 199 ; +2) · `clippy --lib -D warnings` propre · `sm/node.rs` nightly-fmt propre · **grep sans-IO** sur `sm/` : zéro lecture réelle d'horloge/IO.

**Auto-revue invariants §3 :** déterminisme ✅ (`integrate_remote_block` prouvé clock/RNG-free ; aucun `Utc::now`/`OsRng`/`HashMap`-order ajouté au cœur) · arithmétique ✅ (rien d'ajouté ; bornes d'émission vérifiées dans la méthode existante) · robustesse Rust ✅ (parse via `if let`, `Result` ignoré explicitement ; zéro `unwrap`/`unsafe` hors tests) · sécurité réseau ✅ (enveloppe signée vérifiée **avant** `integrate` ; bloc malformé/cassé rejeté) · mémoire bornée ✅ (rien ajouté) · tests livrés ✅ (intégration + rejet de lien cassé).

## T0.1 — tranches suivantes (sûres, autonomes) ⏳
- **Tranche 7** : sync de chaîne — `RequestChain{from_height}` → `Effect::Send`(`ChainSegment`) ciblé (1ᵉʳ `Effect::Send` du cœur) ; `ChainSegment` → intégration paginée. Puis `PublishUsername`, `Hello`.
- **Tranche 8** : `Event::Command` (transfert/stake/@pseudo local) → tx signée + `Effect::Broadcast`.
- Puis **T0.3** (coquille prod : adaptateur Iroh/tokio/OsRng/libSQL ↔ Event/Effect ; suite existante verte) → **T0.4** (simulateur : horloge virtuelle + scheduler seedé `(time_ms, seq)`) → T0.5–T0.8.
- **Tranche 4** : rebrancher les property-tests de conservation **sur le cœur** (Σ soldes + brûlé == miné, exécutés en continu).
- Puis **T0.3** (coquille prod : adaptateur Iroh/tokio/OsRng/libSQL ↔ Event/Effect, suite existante toujours verte) → **T0.4** (simulateur : horloge virtuelle + scheduler seedé, départage `(time_ms, seq)`) → T0.5–T0.8.
- Crypto sim (§3.3) : Ed25519 déjà déterministe ✅ ; pour ML-DSA-65 utiliser la **variante déterministe** (non hedged) — à acter lors du branchement signature dans le cœur.

---

# Backlog de correction (`QUANTA_PATCH_CORRECTIONS.md`) — revue T0.1 tranches 3–6

> Ordre imposé : **C1** (prouver le déterminisme) → **C2** (sync, 🛑) → C3–C6 → C7–C8.

## C1 — Audit de déterminisme transitif + méta-test (pièce maîtresse) ✅

### Livrable 1 — audit transitif documenté

Graphe d'appel **réel** atteint par `Node::handle`, fonction par fonction. Verdict
sur les 3 sources de non-déterminisme : **horloge système**, **`OsRng`**, **itération
`HashMap`**. `✓` = aucune des trois.

**`Event::Tick` → `on_tick(now_ms)`**

| Fonction (fichier:ligne) | Horloge | OsRng | HashMap-order | Verdict |
|---|---|---|---|---|
| `Node::on_tick` (`sm/node.rs`) | temps **injecté** (`now_ms`) | non | non | ✓ |
| `Ledger::prune_mempool_at` (`ledger.rs:103`) | **injecté** (`now_secs`) | non | itère `self.pending` (Vec ordonné) ; `seen_tx_hashes`/`balance_cache` = accès membre/`entry` (pas d'itération ordonnée) | ✓ |
| `cache_revert_tx` (`ledger.rs:159`) | non | non | `entry().or_insert` (commutatif) | ✓ |

**`Event::MessageReceived` → `on_message(from, bytes)`** — portail puis dispatch

| Fonction (fichier:ligne) | Horloge | OsRng | HashMap-order | Verdict |
|---|---|---|---|---|
| `dispatcher::validate_envelope_at` (`dispatcher.rs:466`) | **injecté** (`now_secs`) | non | non | ✓ |
| `GossipRouter::is_fresh_at` (`gossip.rs:313`) | **injecté** | non | non | ✓ |
| `verify_envelope_signature` (`dispatcher.rs:178`) | non | non | non | ✓ |
| `signable_envelope_bytes` (`gossip.rs:369`) | non | non | serde sur enum `GossipMessage` (pas de map) | ✓ |
| `CryptoEngine::verify` (`security/mod.rs:102`) | non | non | non | ✓ |
| **BroadcastTx** → `apply_remote_tx_checked` (`ledger.rs:421`) | non | non | `get_nonce` = `HashMap::get` (pas d'itération) | ✓ |
| → `verify_tx` (`ledger.rs:465`) | **non** (signature seule) | non | non | ✓ |
| → `apply_verified_remote_tx`/`replay_remote_tx` (`ledger.rs:400/383`) | non | non | `cache_apply_tx` commutatif ; `pending` Vec ; `seen_tx_hashes` membre | ✓ |
| **Ping** → `sign_broadcast` (`sm/node.rs`) | temps **injecté** (`millis_to_rfc3339(now_ms)`) | non | non | ✓ |
| → `CryptoEngine::sign` (`security/mod.rs:79`) | non | **non** — Ed25519 `Signer::sign` est **déterministe** (RFC 8032) | non | ✓ |
| **NewBlock** → `integrate_remote_block` (`ledger.rs:704`) | **non** (aucun contrôle de fraîcheur ; `block.timestamp` seulement *haché*) | non | `block_tx_ids`/`pending_tx_ids`/`remote_tx_hashes` = `HashSet` **membership-only** ; toutes les itérations sont sur des **Vec** (`block.transactions`, `self.pending`, `our_tip.transactions`) | ✓ |
| → `validate_remote_block`/`validate_block_against_prev` (`ledger.rs:588/658`) | non | non | non (merkle sur Vec) | ✓ |
| → `validate_block_emission` (`ledger.rs:614`) | non | non | `stats().total_mined` = somme sur `chain` (Vec) — commutative | ✓ |

**`Event::Command` / `Event::TimerFired`** → inertes (`Vec::new()`), aucun appel. ✓

**Lectures d'horloge `OsRng` trouvées HORS du graphe courant** (documentées par honnêteté) :
- `CryptoEngine::sign_hybrid` (`security/mod.rs:94`, `OsRng`) — **jamais appelée** par le cœur (le cœur signe via `sign`, pas `sign_hybrid`).
- `CryptoEngine::generate_keypair` (`security/mod.rs:43`, `OsRng`) — **jamais appelée** par le cœur (identités **injectées** via `import_keypair`, déterministe).
- `Ledger::transfer_tx` (`ledger.rs:290`, `Utc::now`, fenêtre ±5 min) — **chemin de CRÉATION locale**, atteint uniquement par `Event::Command(Transfer)` (pas encore branché). **Ce n'est PAS un chemin de validation distante.** C'est la ligne que l'audit antérieur avait étiquetée « tx validation à `ledger.rs:272` » — étiquetage **imprécis** : c'est de la création, corrigé ici.
- `Ledger::seal_block` (`ledger.rs:945`, `Utc::now`) — **production de bloc**, atteinte seulement par C7 (scellement dans le cœur), pas encore branché.

**Conclusion C1 (capitale).** Aucun chemin de **VALIDATION** atteignable depuis `Node::handle`
ne lit l'horloge système : ni `integrate_remote_block` (zéro contrôle de fraîcheur sur les
blocs historiques), ni `verify_tx` (signature seule). Les seuls contrôles de fraîcheur sont
(a) `is_fresh_at` au niveau **enveloppe** (déjà à temps injecté) et (b) `transfer_tx` (±5 min)
en **création locale** (hors graphe courant). → **Le rejeu de sync (blocs historiques)
fonctionne déjà** côté validation ; C2 doit le *prouver* par un test, et la décision 🛑
`ledger.rs:272` est largement **caduque pour les blocs** (le seul écart « temps injecté »
restant est le futur chemin `Command` via `transfer_tx`).

### Livrable 2 — méta-test de déterminisme (`sm/node.rs`)

- `determinism_meta_test_128_runs_are_byte_identical` : une **séquence fixe** (Tick ; `NewBlock`
  finançant alice ; `BroadcastTx` alice→bob signée ; `Ping`→`Pong` ; octets hostiles ; 2 Ticks
  d'élagage), **gelée en bytes une seule fois**, rejouée sur un cœur neuf **N = 128 fois**.
  Assertion : les **`Effect` byte-identiques** ET l'**empreinte ledger identique** (BLAKE3 sur
  un projeté canonique : chaîne ordonnée + soldes **triés** + agrégats). Toute non-déterminisme
  transitive (ordre `HashMap`, `OsRng`, horloge) ferait diverger ≥ 1 run. Sanity intégré :
  bob crédité à l'admission, puis **évincé** par `prune_mempool_at` à temps injecté (exerce
  l'élagage + `cache_revert`), Pong unique émis.
- `determinism_comparison_has_teeth` : prouve que la comparaison **discrimine** (l'empreinte
  détecte 1 µQTA d'écart ; deux Ping distincts donnent deux Pong distincts) — la garde n'est
  pas vacuellement vraie.

**CHANGELOG.** Aucune modification de logique (C1 = diagnostic + tests, conforme à la contrainte).
Ajout de 2 tests + 1 helper d'empreinte dans `sm/node.rs`.

**Portes de vérification.** `cargo test --lib` → **203 passed / 0 failed** (était 201 ; +2) ·
`clippy --lib -D warnings` propre · `sm/node.rs` nightly-fmt propre · **grep sans-IO** sur
`sm/` : zéro lecture réelle d'horloge/`OsRng`/IO (seule occurrence : le mot « OsRng » dans un
message d'assertion, pas du code).

**Auto-revue invariants §3 :** déterminisme ✅ (méta-test 128 runs byte-identiques + empreinte ;
audit transitif exhaustif) · arithmétique ✅ (aucune logique touchée) · robustesse Rust ✅
(tests seulement ; `unwrap` tolérés en test) · sécurité réseau ✅ (séquence passe par le portail
signé) · mémoire bornée ✅ (rien ajouté en prod) · tests livrés ✅.

**Note règle d'arrêt §4.** L'audit n'a révélé **aucune** lecture d'horloge dans un chemin de
validation → rien à router vers C2 comme correctif d'horloge en validation. Le point 🛑 de C2
(`ledger.rs:272` « rejette par conception les éléments anciens ») est **réévalué** : il n'existe
pas de tel rejet dans le chemin de validation de bloc/tx distant. Options à acter en C2 ci-dessous.

## C2 — Fraîcheur de validation à temps injecté + test sync-replay ✅ (🛑 réévalué, non déclenché)

**Constat (issu de C1).** Aucun chemin de **VALIDATION** distante ne lit l'horloge :
`integrate_remote_block` n'a **aucun** contrôle de fraîcheur (le `block.timestamp` n'est que
*haché*), et `verify_tx` ne vérifie que la signature. Le seul contrôle de fraîcheur sur le
chemin entrant est `is_fresh_at` au niveau **enveloppe**, **déjà à temps injecté**. → Le
livrable « injecter le temps dans les contrôles de fraîcheur de validation » est **déjà
satisfait** ; il ne reste **rien à modifier** dans le chemin de validation.

**Décision 🛑 `ledger.rs:272` — réévaluée et NON déclenchée.** La règle d'arrêt s'applique
*si* un contrôle de fraîcheur en validation rejette par conception les éléments anciens.
C1 a **prouvé** qu'un tel contrôle n'existe pas dans la validation distante (bloc/tx). La
condition est donc **fausse** → pas d'arbitrage protocole à remonter. (`ledger.rs:290`
actuel = `transfer_tx`, **création locale**, hors chemin de validation ; son injection est
reportée à la tranche 8 quand `Event::Command(Transfer)` sera branché — voir note ci-dessous.)

**Quoi.** Ajout du **test sync-replay manquant** (aucune modification de logique) : prouve et
**verrouille** l'indépendance à l'âge de la validation distante. Deux tests dans `sm/node.rs`,
tous deux à travers le cœur (`Node::handle`) et **indépendants de la date** (le `now` injecté
est dérivé du *seal time* du bloc / *build time* de la tx + N h) :
- `sync_replay_historical_block_integrates_at_far_future_now` : un bloc scellé à `t0` s'intègre
  quand on le valide à `now = t0 + 6 h` **puis** `t0 + 48 h` (deux points éloignés ⇒ pas un
  effet de bord « juste dans la fenêtre »).
- `remote_tx_admitted_regardless_of_tx_age` : une tx signée construite à `t0` est admise à
  `now = t0 + 6 h` (signature seule, pas de fenêtre de fraîcheur à l'admission distante).

**Pourquoi.** C'est le cas dangereux que les tests précédents ne couvraient pas (ils validaient
à `t ≈ instant de scellement`). Le test garde contre une régression future qui ajouterait une
porte de fraîcheur murale au chemin d'admission/intégration et casserait silencieusement la sync.

**Note (dette explicite, tranche 8).** `transfer_tx` (`ledger.rs:290`, fenêtre ±5 min via
`Utc::now`) est de la **création locale**. Quand `Event::Command(Transfer)` sera branché
(tranche 8), ce contrôle devra passer au motif `_at(…, now)` injecté pour que le rejeu local
de commandes reste déterministe. Hors scope C2 (validation/sync), tracé ici.

**Portes de vérification.** `cargo test --lib` → **205 passed / 0 failed** (était 203 ; +2) ·
`clippy --lib -D warnings` propre · `sm/node.rs` nightly-fmt propre.

**Auto-revue invariants §3 :** déterminisme ✅ (validation distante prouvée clock-free ; tests
date-indépendants) · arithmétique ✅ (aucune logique touchée) · robustesse Rust ✅ (tests seuls) ·
sécurité réseau ✅ (enveloppe fraîche signée exigée ; seul l'âge *interne* bloc/tx est ignoré,
par conception — la fraîcheur anti-rejeu vit au niveau enveloppe) · mémoire bornée ✅ · tests
livrés ✅.

## C3 — Observabilité des décisions consensus du cœur ✅

**Quoi.** Nouveau type `ConsensusTelemetry` (compteurs `u64` : `blocks_integrated` /
`blocks_duplicate` / `blocks_rejected` / `txs_admitted` / `txs_dropped`) tenu par `Node`,
exposé en lecture seule via `Node::telemetry()`. `on_message` mappe désormais l'issue de
chaque décision consensus sur exactement un compteur : `NewBlock` → `Ok(true)`/`Ok(false)`/
`Err` ; `BroadcastTx` → admis/non-appliqué. Le `let _ = …` qui avalait le résultat de
`integrate_remote_block` est remplacé par un `match` qui incrémente le compteur idoine.

**Pourquoi.** Avant, un bloc **rejeté** et un bloc **valide sans effet sortant** renvoyaient
tous deux `Vec::new()` → indistinguables à la frontière du cœur. Le simulateur (T0.4+) doit
pouvoir asserter « ce bloc byzantin a été **rejeté** » sans inférer depuis `chain.len()`. La
télémétrie rend l'**issue** observable.

**Choix de conception.** Compteurs inspectables (option explicite de C3) plutôt qu'un `Effect`
d'observation : **additif**, ne change **pas** le flux d'`Effect` (donc pas le comportement
réseau), ne touche **aucune** règle de protocole (§4 non applicable), n'expose **aucun** secret
(statuts neutres). `txs_dropped` regroupe honnêtement {mauvaise signature, nonce périmé,
doublon} — l'admission ne renvoyant qu'un `bool`, c'est documenté tel quel (pas de sur-promesse).

**Propriétés affectées.** **Aucune** côté réseau/consensus : mêmes effets, mêmes mutations
ledger ; la télémétrie est un sous-produit déterministe (le méta-test C1 reste vert : 128 runs
byte-identiques inchangés).

**Tests livrés (2).** `consensus_outcome_distinguishes_integrated_from_rejected_at_core_boundary` :
un bloc intégré et un bloc à lien cassé renvoient **le même** flux d'`Effect` (vide) — mais la
télémétrie les sépare (`blocks_integrated=1/rejected=0` vs `0/1`). C'est la preuve directe de
la revendication C3. `consensus_telemetry_counts_duplicates_and_tx_outcomes` : un même cœur
compte intégration + doublon de bloc + tx admise + tx rejouée (dropped).

**Portes de vérification.** `cargo test --lib` → **207 passed / 0 failed** (était 205 ; +2) ·
`clippy --lib -D warnings` propre · `sm/node.rs` nightly-fmt propre.

**Auto-revue invariants §3 :** déterminisme ✅ (compteurs déterministes ; méta-test C1 intact) ·
arithmétique ✅ (`+= 1` sur des compteurs internes bornés par le nombre de messages traités —
pas d'entrée attaquant illimitée par message) · robustesse Rust ✅ (zéro `unwrap`/`unsafe` hors
tests ; parse via `map`/`unwrap_or(false)`) · sécurité réseau ✅ (additif, comportement réseau
identique ; statuts neutres, pas de secret) · mémoire bornée ✅ (5 `u64`) · tests livrés ✅.

## C4 — Couverture consensus : reorg + motifs de rejet à travers le cœur ✅

**Quoi.** Tests **uniquement** (aucune logique touchée). Le comportement le plus audité — la
résolution de fork — et les principaux motifs de rejet sont désormais exercés via `Node::handle`,
pas seulement sur `integrate_remote_block` en direct. Helpers ajoutés : `sealed_block_with_transfer`
(bloc valide hauteur 1 : mining→alice + transfert signé alice→dest + burn, scellé) et
`integrate_block_via_core` (livre un bloc dans une enveloppe `NewBlock` signée à un cœur neuf).

**Honnêteté sur le mécanisme.** La règle implémentée est un **départage de fork à hauteur égale**
(hash le plus haut gagne), **pas** un reorg multi-blocs « chaîne la plus lourde ». Le test exerce
le **code réel** (et le note explicitement), au lieu de tester un reorg de poids que le code
n'implémente pas — conforme à l'honnêteté radicale du projet.

**Tests livrés (4).**
- `fork_reorg_through_core_preserves_all_txs` : deux blocs concurrents à hauteur 1 (transferts
  vers bob / carol). On intègre le hash **bas** d'abord, puis le hash **haut** (déclenche le
  reorg). Assertions : le tip bascule sur le gagnant (hash haut) ; **les deux** transferts
  survivent (`balance_of(bob)==net` ET `balance_of(carol)==net`) — celui du perdant via le
  **re-queue mempool** (AUDIT-BLK-1), celui du gagnant via la chaîne ; `stats().pending==3` (les
  3 tx exclusives du perdant remises en attente, **non perdues**) ; `blocks_integrated==2`.
- `core_rejects_block_with_inconsistent_merkle_root` : retirer une tx d'un bloc scellé (sans
  re-sceller) ⇒ l'ensemble des tx ne correspond plus au hash engagé ⇒ rejet (Merkle/hash
  recalculés divergent), chaîne inchangée.
- `core_rejects_block_exceeding_emission_cap` : un bloc minant 500 QUANTA ≫ borne/bloc (~128 à
  la genèse) ⇒ rejeté au garde `validate_block_emission`, chaîne inchangée.
- `core_rejects_block_with_invalid_contained_tx_signature` : corrompre la signature d'une tx
  signée du bloc ⇒ `verify_tx` échoue pendant la validation ⇒ rejet du bloc entier.

Chaque motif de rejet : `blocks_rejected==1`, `blocks_integrated==0`, `chain.len()==1`
(genèse seule, **inchangée**) — vérifié via le canal d'observabilité C3.

**Portes de vérification.** `cargo test --lib` → **211 passed / 0 failed** (était 207 ; +4) ·
`clippy --lib -D warnings` propre · `sm/node.rs` nightly-fmt propre.

**Auto-revue invariants §3 :** déterminisme ✅ (départage par hash = déterministe ; ordre de
livraison contrôlé) · arithmétique ✅ (rien touché) · robustesse Rust ✅ (tests seulement) ·
sécurité réseau ✅ (tous les motifs byzantins — Merkle incohérente, sur-émission, signature de tx
invalide — rejetés à travers le cœur ; non-perte de tx au reorg prouvée) · mémoire bornée ✅ ·
tests livrés ✅.

## C5 — Point d'entrée d'admission de tx unique, signature imposée par le type ✅

**Quoi.** Nouveau **token porteur de preuve** `VerifiedTx(Transaction)` dans `ledger.rs`. Seul
constructeur : `VerifiedTx::new(tx) -> Option<Self>` qui appelle `verify_tx` **une fois**.
`apply_verified_remote_tx` prend désormais un `VerifiedTx` (plus une `Transaction` nue) :
la précondition « signature vérifiée » est **imposée par le type**, pas par la mémoire de
l'appelant. `apply_remote_tx_checked` (cœur) et `handle_broadcast_tx` (shell) frappent le token
de façon identique ⇒ **point d'entrée unique signature-gated**.

**Choix de conception (typestate).** Une simple ré-vérification dans le shell aurait soit
**doublé** la vérification (interdit par C5), soit forcé un changement d'ordre observable autour
du dual-ledger CRDT interleavé (⇒ 🛑). Le token rend l'état illégal *non représentable* à la
**compilation** : zéro double-vérification, **zéro changement d'ordre** (le shell frappe le
token là où il appelait `verify_tx`, puis le consomme là où il appelait `apply_verified_remote_tx`).

**Comportement strictement préservé.** Même `verify_tx`, même ordre (mint → nonce gate → CRDT →
apply linéaire), mêmes gardes AUDIT-TX-1/2. Alignement mineur : le nonce-gate du shell passe de
`tx.nonce + 1` (arith non vérifiée, §3) à `saturating_add(1)` — **identique** pour tout nonce
atteignable (ne diffère qu'à `u64::MAX`, inatteignable), et **converge** avec le cœur. Pas de
changement d'ordre ⇒ **pas de 🛑**.

**Note formatage (honnêteté + correction).** En appliquant nightly-rustfmt à
`ledger.rs`/`dispatcher.rs` j'ai déclenché un **reflow** large hors C5 (commentaires
`wrap_comments`, `format_strings` du `rustfmt.toml`). **Corrigé pour `ledger.rs`** : revert à
HEAD puis ré-application de la **logique seule** (tranches + C5 + C7) en style d'origine, avec les
**216 tests comme filet** ⇒ diff passé de **517/68** à **369/4** (les 4 suppressions = exactement
les lignes réellement changées ; zéro reflow sur le code inchangé). `dispatcher.rs` **reste
reflowé** : son working-tree porte aussi la **purge crypto-only non commitée** (HEAD contient
encore 11 handlers web/social supprimés dans le working-tree) ; un revert ré-introduirait cette
purge, donc je ne le touche pas sans demande explicite (pas de snapshot pré-reflow récupérable).
Décision maintenue : ne **pas** reformater de fichiers entiers.

**Tests livrés (1) + suite complète.** `verified_tx_typestate_gates_admission_on_signature` :
signature valide ⇒ `Some(token)` ; signature corrompue ⇒ `None`. La garantie « pas d'apply sans
token » est imposée à la **compilation** (rien à asserter au runtime — c'est le but).

**Portes de vérification.** `cargo test --lib` → **212 passed / 0 failed** (était 211 ; +1, aucune
régression) · `clippy --lib -D warnings` propre (token en `Option` pour éviter `result_large_err`) ·
`sm/node.rs` + `ledger.rs` + `dispatcher.rs` nightly-fmt propres.

**Auto-revue invariants §3 :** déterminisme ✅ (cœur inchangé ; méta-test C1 vert) · arithmétique ✅
(shell `+1` → `saturating_add`, **durcit** §3) · robustesse Rust ✅ (token `Option`, zéro `unwrap`
hors tests) · sécurité réseau ✅ (signature imposée par le type sur le chemin autoritatif ; shell
et cœur convergent) · mémoire bornée ✅ (token = wrapper zéro-coût) · tests livrés ✅.

## C6 — Zeroize du secret de signature tenu par le cœur ✅

**Faille trouvée (réelle).** `Node` tient une `CryptoEngine` qui possède le secret Ed25519 dans
`key_pair.signing_key`. Or `ed25519-dalek` était configuré `features = ["rand_core"]` **sans**
`zeroize` ⇒ `SigningKey` **n'effaçait pas** son scalaire secret au drop. `CryptoEngine` n'a pas
non plus d'`impl Drop`/`ZeroizeOnDrop` (le `derive` de `security/mod.rs:135` porte sur
`SecureBuffer`, pas `CryptoEngine`). La clé ML-DSA-65 (`fips204::PrivateKey`), elle, dérive déjà
`Zeroize, ZeroizeOnDrop` (vérifié dans la source du crate). → seul l'Ed25519 fuyait.

**Correctif.** `Cargo.toml` : `ed25519-dalek` passe à `features = ["rand_core", "zeroize"]`.
`SigningKey` implémente alors `ZeroizeOnDrop` ⇒ le *drop glue* de `CryptoEngine` (qui drope
`key_pair` → `KeyPair` → `signing_key`, et `ml_dsa.0`) **efface les deux secrets** au drop. Pas
d'`impl Drop` manuel nécessaire (idiomatique, zéro risque).

**Garde anti-régression (test compile-time).** Module `security::zeroize_guards` :
`assert_zeroize_on_drop::<ed25519_dalek::SigningKey>()` et `…::<fips204::ml_dsa_65::PrivateKey>()`.
Ces assertions **ne compilent que si** les deux types impl `ZeroizeOnDrop` — retirer la feature
`zeroize` casserait le build. C'est le test le plus fort faisable (observer la mémoire libérée
serait de l'UB).

**Dette notée (hors C6).** `CryptoEngine::get_secret_bytes()` renvoie une `Vec<u8>` copie du
secret, **non** zeroize-ée — responsabilité de l'appelant. Le cœur ne l'appelle pas (`sign_broadcast`
utilise `sign`/`get_identity`). À durcir si un appel futur l'expose.

**Portes de vérification.** `cargo test --lib` → **213 passed / 0 failed** (était 212 ; +1) ·
`clippy --lib -D warnings` propre · édition minimale (Cargo.toml + un module de test ; pas de
reformatage de `security/mod.rs`).

**Auto-revue invariants §3 :** déterminisme ✅ (Ed25519 toujours déterministe ; feature `zeroize`
n'affecte que le drop) · arithmétique ✅ (rien) · robustesse Rust ✅ (zéro `unwrap`/`unsafe`) ·
sécurité ✅ (**secret de signature effacé au drop** — invariant §3 désormais tenu et gardé) ·
mémoire bornée ✅ · tests livrés ✅ (garde compile-time).

## C7 — Scellement de bloc à temps injecté dans le cœur ✅ (🛑 évalué, non déclenché)

**Quoi.**
- `ledger.rs` : extraction `seal_block_at(miner, kwh, timestamp)` (+ `seal_if_pending_at`).
  `seal_block`/`seal_if_pending` lisent l'horloge à la frontière et **délèguent**. Logique
  **identique** : même racine de Merkle, même pré-image de hash, même structure de bloc — seule
  la **source du timestamp** change.
- `sm/node.rs` : `Node::propose_block_at(now_ms, validators) -> Option<Effect>` — le cœur
  **PRODUIT** son 1ᵉʳ bloc (jusqu'ici il ne faisait que valider). Extraction fidèle de
  `mining_loop::pos_seal_if_leader` : **même** beacon (bloc enterré `LEADER_ENTROPY_LOOKBACK`),
  **même** slot (= hauteur), **même** `is_valid_proposer`, **même** bootstrap permissionless
  (`MIN_VALIDATOR_STAKE`). Lit **zéro horloge** : `timestamp` du bloc ET `elapsed` du fallback
  dérivent de `self.now_ms`. Produit un `Effect::Broadcast(NewBlock)` signé.

**🛑 évalué, NON déclenché.** Le stop-rule C7 vise un **changement de règle** de consensus
(qui peut sceller / quand / bornes). Or **aucune règle n'est modifiée** : élection, timeout de
fallback, `MIN_VALIDATOR_STAKE`, bornes d'émission, structure de bloc — tout est **réutilisé
verbatim**. Seuls les **inputs** sont injectés (sans-IO) : le temps (déjà le motif établi) et le
**jeu de validateurs en paramètre**. Ce dernier est un *input du harness*, pas une règle : la
prod le construit depuis le reputation-engine, le simulateur depuis son scénario. → comme pour
C2, la condition d'arrêt est **fausse**, je procède (extraction pure documentée).

**Point de conception différé (transparent, hors règle consensus).** *Où* le cœur obtiendra à
terme les stakes (paramètre vs état `Node` absorbé vs snapshot mis à jour par événement) reste à
décider à T0.3/T0.4. Le choix « paramètre » est le plus **réversible** et n'enferme rien — c'est
le contrat sans-IO (le shell/simulateur fournit les inputs). Aucune règle de consensus n'en dépend.

**Tests livrés (3).** `elected_leader_seals_a_valid_block_at_injected_time` (le leader élu scelle
un `NewBlock` signé valide, `block.timestamp == now_ms`, chaîne +1) ;
`non_leader_does_not_seal` (un non-leader, `elapsed` faible ⇒ hors fenêtres fallback, ne scelle
**pas** ; chaîne inchangée) ; `block_proposal_is_byte_deterministic` (même seed + pending gelé +
temps injecté ⇒ octets **identiques**).

**Portes de vérification.** `cargo test --lib` → **216 passed / 0 failed** (était 213 ; +3) ·
`clippy --lib -D warnings` propre · `sm/node.rs` + `ledger.rs` nightly-fmt propres.

**Auto-revue invariants §3 :** déterminisme ✅ (timestamp + `elapsed` injectés ; élection RNG-free ;
scellement byte-déterministe prouvé) · arithmétique ✅ (`elapsed` en `saturating_sub` ; bornes
d'émission inchangées, vérifiées à la validation) · robustesse Rust ✅ (tout en `?`/`Option`, zéro
`unwrap`/`unsafe` hors tests) · sécurité réseau ✅ (bloc **signé** ; mêmes gardes d'éligibilité que
la prod) · mémoire bornée ✅ · tests livrés ✅.

## C8 — Modèle de propagation décidé et documenté (avant T0.5) ✅

**Décision (sans ambiguïté) : TRANSPORT-FLOOD.** C'est le **transport** (iroh-gossip) qui inonde
les messages à tout le topic ; l'application **ne relaie pas**. Le cœur (`sm::Node`) et la prod
n'émettent **rien** à la réception d'un bloc/tx — c'est correct.

**Preuves (citées).**
1. **iroh-gossip 0.98.0 = Plumtree** : `…/iroh-gossip-0.98.0/src/proto/plumtree.rs` (doc de module)
   « *Implementation of the Plumtree **epidemic broadcast tree** protocol* », d'après le papier
   Leitão/Pereira/Rodrigues (SRDS'07). Plumtree dissémine un message à **tous** les membres du
   topic via un arbre couvrant (eager push) + réparation lazy (IHAVE/GRAFT), en **multi-saut géré
   par la couche gossip**.
2. **API** : `…/src/api.rs:179` `GossipSender::broadcast` = « *Broadcasts a message to **all
   endpoints*** » ; vs `:185` `broadcast_neighbors` = « *…to our **direct neighbors*** ». 
3. **Usage dans le code** : `gossip_tasks.rs:41` appelle `sender.broadcast(bytes.into())` — la
   variante **swarm complet**, pas `broadcast_neighbors`.
4. **Design confirmé** : `dispatcher.rs:875-877` — « *Re-broadcast is intentionally skipped: the
   gossip layer already floods envelopes via iroh-gossip; the dedup hash … ensures convergence* ».

→ Un nœud qui reçoit un `NewBlock` l'intègre et n'émet rien : Plumtree l'a **déjà** transmis à
ses voisins. Le dedup (`seen_messages`/`seen_tx_hashes`) absorbe les livraisons multiples.
**Aucun changement de comportement** (donc pas de 🛑) — je documente l'existant prouvé.

**Contrainte pour T0.5 (réseau virtuel) — écrite.**
- `Effect::Broadcast { bytes }` du cœur a la sémantique **« disséminer à tout le topic »**, PAS
  « envoyer aux voisins directs ». Le **réseau virtuel** est responsable de la livraison
  **multi-saut** : sur un `Effect::Broadcast`, il délivre un `Event::MessageReceived` à **tous**
  les nœuds abonnés du topic (modulo topologie/pertes/partitions simulées pour injecter des
  fautes). Modèle fidèle minimal : « broadcast = livraison à tous les membres, sous fautes ».
  (Optionnel : modéliser l'arbre Plumtree exact ; non requis pour la fidélité fonctionnelle.)
- Le **cœur ne doit PAS** émettre d'effet de relais à la réception (ni bloc ni tx) — c'est le
  réseau virtuel qui inonde, exactement comme iroh-gossip en prod. Le cœur reste tel quel.
- Invariant à respecter en sim : **zéro perte** si pas de faute (règle réseau #8) ⇒ le flood
  virtuel doit atteindre tous les nœuds connectés ; la convergence repose dessus + dedup.

**Critères d'acceptation.** Note tranchée + sourcée ✅ ; contrainte T0.5 écrite ✅. (Tâche
diagnostic/doc : aucun code, aucun test — la suite reste à **216 / 0**.)

---

## Bilan backlog `QUANTA_PATCH_CORRECTIONS.md` — C1→C8 ✅

| Tâche | État | 🛑 | Livrable clé |
|---|---|---|---|
| C1 | ✅ | — | Audit déterminisme **transitif** + méta-test 128 runs byte-identiques (+ teeth) |
| C2 | ✅ | évalué, non déclenché | Test sync-replay (bloc/tx vieux intégrés à `now` injecté lointain) |
| C3 | ✅ | — | `ConsensusTelemetry` : issue consensus observable à la frontière du cœur |
| C4 | ✅ | — | Reorg + 3 motifs de rejet testés **à travers le cœur** |
| C5 | ✅ | non déclenché | `VerifiedTx` typestate : 1 point d'entrée signature-gated (cœur+shell) |
| C6 | ✅ | — | Feature `zeroize` ed25519 + garde compile-time (secret effacé au drop) |
| C7 | ✅ | évalué, non déclenché | `propose_block_at` : production de bloc déterministe, temps injecté |
| C8 | ✅ | — | Décision **transport-flood** prouvée + contrainte T0.5 écrite |

**Total** : `cargo test --lib` **216 passed / 0 failed** (était 201 à l'ouverture du backlog ; +15) ·
`clippy --lib -D warnings` propre · `sm/*` + `ledger.rs` + `dispatcher.rs` nightly-fmt propres.
Deux 🛑 (C2, C7) **évalués avec preuve** et non déclenchés (aucune règle de consensus modifiée) ;
aucun n'a nécessité de deviner. Reste hors backlog : tranches 7-8 du harness (sync de chaîne,
`Event::Command`) puis T0.3 (coquille prod) → T0.4 (simulateur) → T0.5 (réseau virtuel, contrainte
C8 à appliquer).

---

# Harness DST — T0.4 (simulateur déterministe) — tranche 1 ✅

**Décisions consensus actées (2026-06-21)** : périmètre **Option 1 — finality gadget
d'abord** ; comité **stake on-chain seul** (réputation hors sécurité). ADR gravées dans
`docs/decisions/`. Le harnais multi-nœuds est le **prérequis** avant tout code de gadget.

**Quoi.** Nouveau module **test-only** `src-tauri/src/sm/sim.rs` (`#[cfg(test)] mod sim`) :
la **coquille de simulation** du spec §3.1. Boucle d'événements mono-thread sur **horloge
virtuelle** + **RNG seedé** (`Blake3Rng`), file de priorité ordonnée par **`(time_ms, seq)`**
(départage total déterministe ; `seq` = compteur d'insertion monotone). **Réseau virtuel**
single-hop sans perte appliquant le modèle **transport-flood** acté en **C8** : un
`Effect::Broadcast` est livré comme `Event::MessageReceived` à **tous les autres** nœuds,
`Effect::Send` à un seul ; `SetTimer` → `TimerFired` planifié. La **production de bloc** n'est
pas encore event-driven (besoin de timers consensus + vue validateur in-core) → un seul *kick*
orchestré `Sim::propose` ; tout le reste (propagation, validation, intégration, convergence)
passe **purement par la boucle** `handle`.

**Pourquoi.** C'est l'infrastructure qui permettra de tester la **dynamique** du consensus
(convergence n-nœuds, partitions, byzantins) **avant** d'écrire le finality gadget — séquençage
exigé par [[DESIGN-CONSENSUS-DAG-BFT]] §6 et la Constitution.

**Tests livrés (3).**
- `sim_three_nodes_converge_on_proposed_block` : le leader (seul validateur staké) **scelle** un
  bloc → flood → **B et C convergent** sur le même tip (hauteur 2 partout). **Convergence
  multi-nœuds réelle**, pilotée par la boucle déterministe.
- `sim_run_is_byte_deterministic` : même seed ⇒ **trace d'effets byte-identique** + tips
  identiques sur deux exécutions (acceptation T0.4).
- `sim_scheduler_total_order_is_deterministic_under_many_events` : 150 events au même instant
  virtuel popés dans un ordre `(time_ms, seq)` reproductible (la propriété fondatrice).

**Portes de vérification.** `cargo test --lib` → **219 passed / 0 failed** (était 216 ; +3) ·
`clippy --lib -D warnings` propre · `sm/sim.rs` + `sm/mod.rs` nightly-fmt propres · sim en
`#[cfg(test)]` → **zéro poids dans le binaire de production**.

**Auto-revue invariants §3 :** déterminisme ✅ (cœur inchangé ; ordre total `(time_ms, seq)` ;
flood déterministe en ordre de clés triées ; run byte-déterministe prouvé) · arithmétique ✅
(`seq += 1`, `now_ms + NET_DELAY` bornés par le scénario ; `max_steps` garde-fou anti-boucle) ·
robustesse Rust ✅ (test-only ; `expect` toléré en test) · sécurité réseau ✅ (flood = C8 ;
enveloppes signées validées par le portail à la réception) · mémoire bornée ✅ (`max_steps`) ·
tests livrés ✅.

## Reste T0.4 → T0.8 (prochaines tranches)
- **T0.4 tr.2** : câbler la **proposition dans la boucle** (timer consensus → `TimerFired` →
  proposer), retirer le *kick* orchestré ; + la **sync de chaîne** (tranche 7 : `RequestChain` →
  `Effect::Send(ChainSegment)`) pour que les nœuds en retard rattrapent.
- **T0.5** : réseau virtuel avec **fautes** (drop / réordre / dup / délai variable / partition),
  pilotées par le RNG seedé (contrainte C8 : le réseau inonde, le cœur ne relaie pas).
- **T0.6** : fautes nœud + **byzantins** (équivocation, rejeu, rétention) — la cible pour tester
  le futur slashing ([[ADR-003 — Slashing (accountable safety)]]).
- **T0.7** : vérificateurs d'invariants à chaque pas (sûreté + conservation ; vivacité à
  quiescence) ; **T0.8** : runner multi-seed + replay `--seed`.
- **T0.3** (coquille prod) : à faire en additif (adaptateur Iroh/tokio/OsRng/libSQL ↔ Event/Effect)
  sans casser l'app — moins urgent que la dynamique consensus.

---

# Harness DST — T0.4 (simulateur) — tranche 2 : proposition event-driven + sync ✅

**A. Proposition event-driven (le *kick* orchestré disparaît).** Le cœur arme un **timer
consensus** : `on_tick` (nœud avec identité ET validator set) émet `Effect::SetTimer{SEAL_TIMER_ID,
now+SEAL_INTERVAL}` ; à la réception du `Event::TimerFired{SEAL_TIMER_ID}`, `on_seal_timer`
**produit un bloc** via `propose_block_at` (réutilise l'élection + le scellement) puis **re-arme**.
Le `Node` tient un `validators: Vec<Validator>` (snapshot in-core ; **transitoire** — par
[[docs/decisions/ADR-002 — Validator set & comité BFT|ADR-002]] il deviendra un snapshot de stake
on-chain par epoch) posé par `set_validators`. Cadence `SEAL_INTERVAL_MS = 120 000` (= prod
`SEAL_EVERY_N_TICKS × MINE_INTERVAL`). Aucune règle de consensus modifiée (extraction/câblage).

> **Bug trouvé + corrigé (le harnais a des dents).** Premier jet : re-arm à `now_ms + INTERVAL`.
> Or entre deux `Tick` le `now_ms` du cœur est **gelé** → chaque re-arm reprogrammait le **même**
> instant → **boucle infinie** (run tué, exit 144). Fix : champ `next_seal_at_ms` qui avance
> **monotone** de `SEAL_INTERVAL_MS` à chaque tir, indépendamment de `now_ms`.

**B. Sync de chaîne (`RequestChain` → `Effect::Send(ChainSegment)`).** `on_message` gagne :
`RequestChain{from_height, max_blocks}` → `on_request_chain` sérialise `[from_height, …)` (cap 50,
forme inline) et renvoie un **`ChainSegment` signé ciblé** (le **1ᵉʳ `Effect::Send` du cœur**) ;
`ChainSegment{blocks_json}` → `on_chain_segment` intègre bloc par bloc via `integrate_remote_block`,
**stop au 1ᵉʳ échec** (AUDIT-SYNC-1), issues comptées en télémétrie C3. Helper de signature
factorisé : `sign_envelope` → `sign_broadcast` (Broadcast) / `sign_send` (Send ciblé). Décompression
gzip NET-8 laissée au shell (le cœur émet/lit l'inline ; suffisant pour le harnais).

**Simulateur.** `Sim::run_until(horizon_ms)` (fenêtre bornée, laisse un timer périodique tirer sans
churn) + `Sim::add_node` (joiner tardif). `Effect::Send` déjà routé vers un seul nœud.

**Tests livrés (2).**
- `sim_leader_proposes_via_consensus_timer` : le leader produit un bloc **par son timer** (aucun
  `sim.propose`), flood, **convergence 3-nœuds**.
- `sim_late_joiner_syncs_via_request_chain` : un nœud **D** qui rejoint **après** le flood (à la
  genèse) demande la chaîne → A répond un `ChainSegment` ciblé → D **rattrape** (tip + hauteur == A ;
  `blocks_integrated == 1`).

**Portes.** `cargo test --lib` → **221 passed / 0 failed** (était 219 ; +2) · `clippy -D warnings`
propre · `sm/node.rs` + `sm/sim.rs` nightly-fmt propres.

**Auto-revue §3 :** déterminisme ✅ (timer monotone ; ordre `(time_ms, seq)` ; sync = extraction
pure, mêmes octets pour même seed) · arithmétique ✅ (`saturating_add` sur `next_seal_at_ms` et les
horizons ; cap 50 blocs) · robustesse Rust ✅ (tout en `?`/`Option`, zéro `unwrap`/`unsafe` hors
tests ; **boucle infinie corrigée**, `run`/`run_until` bornés) · sécurité réseau ✅ (RequestChain
validé par le portail signé avant réponse ; ChainSegment intégré via la validation consensus
partagée ; AUDIT-SYNC-1) · mémoire bornée ✅ (segment ≤ 50) · tests livrés ✅.

## Reste T0.4 → T0.8
- **T0.5** : fautes réseau (drop/réordre/dup/délai variable/**partition**) pilotées par le RNG seedé
  — la sync de tr.2 est la voie de rattrapage post-partition.
- **T0.6** : byzantins (équivocation → testera le futur [[docs/decisions/ADR-003 — Slashing (accountable safety)|slashing]]).
- **T0.7/T0.8** : vérificateurs d'invariants à chaque pas + runner multi-seed.
- Plus tard : `Hello` (détection de retard auto qui déclenche `RequestChain`), `Event::Command`
  (transfert/stake local), puis **T0.3** (coquille prod, additif).

---

# Harness DST — T0.5 : fautes réseau + partition ✅

**Quoi.** Le réseau virtuel du simulateur passe de *lossless* à **fautif**. Nouveau profil
`NetFaults` (dans `sm/sim.rs`), **tout piloté par le RNG seedé** → reproductible par seed :
- **drop** (`drop_ppm`), **duplication** (`dup_ppm`) — probabilités en **parts-par-million
  entières** (pas de float → déterminisme cross-platform) ;
- **délai variable** (`[min,max] ms`) qui produit aussi le **réordre** (un message à petit délai
  double un message antérieur à grand délai) ;
- **partition de graphe** (`Option<(côté A, côté B)>`) : deux côtés ne peuvent plus se joindre.

`Sim::deliver` applique le modèle dans l'ordre **partition → drop → délai → duplication**, chaque
décision tirée de `self.rng` dans un ordre fixe (flood en clés triées, ordre des effets) → run
déterministe. Helpers : `set_faults`, `partition`/`heal`. Défaut = lossless (`NET_DELAY_MS`), donc
tr.1/2 inchangés. La duplication est absorbée par l'**idempotence du cœur** (dedup tx/bloc).

**Tests livrés (3).**
- `sim_partition_isolates_then_sync_recovers` : une partition **isole A de B** (le bloc de A
  n'atteint pas B) ; après **`heal`**, B **rattrape** via la sync de tr.2 (`RequestChain` →
  `ChainSegment`). Le « split-brain puis réconciliation » canonique.
- `sim_network_faults_are_reproducible_by_seed` : drop 40 % + dup 15 % + délai 1..30 ms actifs →
  **deux runs même seed = sortie identique** (tips + hauteurs). C'est l'acceptation T0.5.
- `sim_total_drop_prevents_delivery` : 100 % drop ⇒ B ne reçoit **rien** ; lossless ⇒ B reçoit le
  bloc. Prouve que le drop est **activable** et déterministe.

**Portes.** `cargo test --lib` → **224 passed / 0 failed** (était 221 ; +3) · `clippy -D warnings`
propre · `sm/sim.rs` nightly-fmt propre.

**Auto-revue §3 :** déterminisme ✅ (fautes pilotées par RNG seedé, ppm entières, ordre de tirage
fixe ; reproductibilité prouvée) · arithmétique ✅ (`saturating_add` sur délais ; modulo borné) ·
robustesse Rust ✅ (test-only ; `run` borné par `max_steps`) · sécurité réseau ✅ (le modèle
N'AFFAIBLIT PAS la validation : drop/dup/délai n'altèrent pas le contenu ; les enveloppes restent
signées + validées à la réception ; partition = pure perte) · mémoire bornée ✅ · tests livrés ✅.

## Reste T0.6 → T0.8
- **T0.6** : fautes **nœud** (crash/restart, dérive d'horloge) + **byzantins** paramétrables
  (équivocation, rejeu, rétention) — la cible pour exercer le futur
  [[docs/decisions/ADR-003 — Slashing (accountable safety)|slashing]].
- **T0.7** : vérificateurs d'invariants à **chaque pas** (sûreté + conservation µQTA) et vivacité
  à quiescence — sur violation, afficher seed + trace ; **détecter un bug injecté**.
- **T0.8** : runner **multi-seed** + mode replay `--seed <S>` — porte globale Phase 0.

---

# Harness DST — T0.6 : fautes nœud + byzantins (détection) ✅

> Périmètre : **détection/observation**. La *punition* (slashing) attend
> [[docs/decisions/ADR-003 — Slashing (accountable safety)|ADR-003]] (encore ouverte).

**Quoi.** Le simulateur gagne :
- **Faute nœud — crash/restart** : `Sim::crash(id)` met un nœud hors-ligne (tout event qui lui
  est destiné est **droppé** dans `run`/`run_until`, son état est conservé) ; `restart(id)` le
  rallume (il est en retard → rattrape par sync).
- **Byzantin — rétention** : `NetFaults.withheld: BTreeSet<(from,to)>` = coupe **dirigée** (un
  leader livre à certains pairs et **retient** vers d'autres), distincte de la `partition`
  symétrique. `Sim::withhold(from, to)`.
- **Byzantin — équivocation** : helper `equivocating_blocks` (deux blocs **différents** à la même
  hauteur, signés par la **même** clé) + **primitive de détection** `detect_equivocation` (deux
  enveloppes valides, **même** émetteur, `NewBlock` à la même hauteur, **hashs différents**) —
  c'est l'**évidence** qu'un futur slashing (ADR-003) consommera, sans punition câblée.

**Tests livrés (3).**
- `sim_equivocation_is_detected_and_honest_nodes_stay_safe` : un proposeur byzantin diffuse **deux
  blocs conflictuels** ; les nœuds honnêtes **convergent quand même** (départage déterministe →
  même tip) **ET** la paire est détectée comme équivocation (et un même bloc deux fois ne l'est
  pas).
- `sim_crashed_node_misses_blocks_then_recovers_on_restart` : un nœud **crashé** rate le bloc ;
  après **restart**, il rattrape par `RequestChain → ChainSegment`.
- `sim_byzantine_retention_then_victim_syncs_from_honest_peer` : le leader **retient** son bloc
  vers R (le livre à S) ; **R rattrape en se synchronisant sur l'honnête S** (pas sur le byzantin).

**Portes.** `cargo test --lib` → **227 passed / 0 failed** (était 224 ; +3) · `clippy -D warnings`
propre · `sm/sim.rs` nightly-fmt propre.

**Auto-revue §3 :** déterminisme ✅ (crash/withhold = règles dures ; équivocation injectée par le
sim ; runs reproductibles) · arithmétique ✅ (rien de neuf côté montants) · robustesse Rust ✅
(test-only ; `run` borné) · sécurité ✅ (le **cœur reste honnête** — l'équivocation est une
attaque *injectée*, jamais une capacité du cœur ; la détection vérifie des **signatures valides** ;
la sûreté tient : les honnêtes ne se divisent pas) · mémoire bornée ✅ · tests livrés ✅.

## Reste T0.7 → T0.8
- **T0.7** : **vérificateurs d'invariants** exécutés à **chaque pas** (sûreté : pas d'ordres
  contradictoires entre nœuds corrects ; conservation µQTA) + vivacité à quiescence. Acceptation
  forte : **un bug injecté volontairement est détecté** (le harnais « a des dents »).
- **T0.8** : **runner multi-seed** (balaye N seeds × configs) + replay `--seed <S>` → **porte
  globale Phase 0**.
- Dette consensus : le slashing de l'équivocation (la *réaction*) attend
  [[docs/decisions/ADR-003 — Slashing (accountable safety)|ADR-003]] ; la **détection** est prête.

---

# Harness DST — T0.7 : vérificateurs d'invariants (avec dents) ✅

**Quoi.** Le simulateur vérifie les invariants consensus à **chaque pas** :
- **Sûreté / agreement** : deux nœuds ne tiennent jamais un **bloc différent au même index**
  (genèse partagée par construction).
- **Conservation µQTA** : `Σ soldes + brûlé == miné` pour chaque nœud. Pour que ça tienne à
  **chaque** pas (pas seulement post-seal), ajout de `Ledger::total_minted()` (chain **+ pending**,
  symétrique de `total_burned()`) — sinon une tx de minage *pending* ferait diverger soldes et
  `total_mined` (chain-only).

`Sim::check_invariants() -> Result<(), Violation>` (ordre clés trié → 1ʳᵉ violation reproductible) ;
`Violation` porte la **seed** (rejeu) + le détail (index/hashs ou soldes/brûlé/miné). `run_checked`
exécute la boucle et **vérifie après chaque pas**, s'arrête à la 1ʳᵉ violation.

**Tests livrés (3).**
- `sim_invariants_hold_through_a_healthy_run` : run sain → sûreté + conservation tiennent partout
  (`run_checked == Ok`) **et** la chaîne avance (vivacité : hauteur 2 partout).
- `sim_partition_fork_breaks_safety_and_is_detected` (**les dents**) : une partition laisse deux
  côtés sceller un bloc **différent** à la hauteur 1 → `run_checked` renvoie `Err(Safety{index:1})`
  **avec la seed**. C'est aussi le **miroir honnête de la faiblesse du PoC** (pas de finalité BFT)
  que le finality gadget ([[docs/decisions/ADR-001 — Fork-choice|ADR-001]] / Option 1) corrigera.
- `sim_conservation_violation_is_detected` : une **release ESCROW sans lock** (monnaie fantôme) →
  `Err(Conservation{minted:0})`.

> [!warning] Trouvaille du harnais (réelle, à corriger plus tard)
> En écrivant le test de sûreté, le harnais a révélé une **faiblesse de hachage de bloc** :
> `tx.id = "tx_{compteur}"` (par-ledger, pas content-addressed) et le **hash de bloc ne commet
> pas le `miner`**. Donc deux blocs au **contenu différent** (mineurs/destinataires différents)
> mais mêmes index/prev/timestamp/nb-tx ont le **même hash** — le commentaire « WEAK-4 : le hash
> commet le CONTENU via Merkle » est **incomplet** (le Merkle hache des **ids compteur**, pas le
> contenu). Contournement dans le test (timestamps distincts → hashs distincts). **À corriger** :
> Merkle sur le contenu (ou ids content-addressed) + inclure `miner` dans le pré-image. Tracé ici ;
> c'est exactement le genre de bug que T0.7 existe pour faire remonter.

**Portes.** `cargo test --lib` → **230 passed / 0 failed** (était 227 ; +3) · `clippy -D warnings`
propre · `sm/sim.rs` + `ledger.rs` nightly-fmt propres.

**Auto-revue §3 :** déterminisme ✅ (checks en ordre trié ; violation reproductible, seed portée) ·
arithmétique ✅ (`total_minted` somme bornée ; `saturating_add` dans la conservation) · robustesse
Rust ✅ (`total_minted` pur ; checks read-only ; tests bornés) · sécurité ✅ (la sûreté est
**vérifiée**, pas supposée ; bug injecté **détecté**) · mémoire bornée ✅ · tests livrés ✅.

## Reste T0.8 (porte globale Phase 0)
- **Runner multi-seed** : balaye N seeds × configs (sain / fautes réseau / partition / byzantin /
  crash), chaque run sous `run_checked` ; agrège ; **replay `--seed <S>`**.
- Acceptation = **porte globale Phase 0** (§4 du spec). Ensuite : `Hello` (détection de retard
  auto), `Event::Command`, puis **T0.3** (coquille prod, additif).
- Dette consensus traçée : faiblesse de hachage de bloc (ci-dessus) ; slashing (réaction) → ADR-003.

---

# BLK-HASH-1 — Le hash de bloc doit committer le contenu (spec `QUANTA_BLK_HASH_INTEGRITY.md`)

## §4.3 — Audit des usages de `tx.id` (blast radius, AVANT modif)

`tx.id = "tx_{compteur}"` = identifiant **positionnel local** (par-ledger), pas content-addressed.
Classement de **chaque** usage :

**CONSENSUS-PERTINENT → à corriger :**
- **Merkle leaf** (`compute_merkle_root`, ~l.949) : feuille = `hash(tx.id)` → le hash de bloc ne
  commet **pas** le contenu. ⇒ §4.1 : feuille = `H(contenu ‖ signature)`.
- **`integrate_remote_block` happy-path** (~l.833-842) : `block_tx_ids`/`pending_tx_ids` keyés sur
  `tx.id`. Une tx de bloc **distante** et une pending **locale** au **même compteur** matchent à
  tort ⇒ pending droppée / effet cache sauté ⇒ **divergence de solde** entre nœuds. ⇒ keyer sur
  `tx.hash`.

**DEDUP (audité) :**
- `seen_tx_hashes.insert(tx.hash)` (l.327/385/416/429/828/918) : keyé sur **`tx.hash`**
  (`blake3(id:from:to:amount:ts:type)`, **porté** par la tx, pas recalculé) → content-bearing,
  **pas** de fausse collision. **OK, pas de fix.**
- `seen_tx_hashes.remove(&tx.id)` (l.165, prune) : **INCOHÉRENT** (insert par hash, remove par id)
  → le remove ne matche jamais → fuite + tx évincée non ré-admissible. ⇒ corriger en `remove(&tx.hash)`.

**BOOKKEEPING LOCAL (pas de fix) :**
- `verify_tx` pré-image de signature (l.553) inclut `tx.id` : la signature commet l'id, qui
  **voyage** avec la tx (pas un problème d'identité inter-nœuds). Le **changer = §4 stop** (schéma
  de signature) → **on ne touche pas**. La feuille Merkle utilisera un `tx_content_bytes` dédié
  **sans** `tx.id`.
- Logs/erreurs (l.779/1756), `find(|t| t.id==…)` sur le pending **d'un seul** ledger (tests
  l.1162-1187) : local, cohérent.

**Nonce / anti-rejeu** : gate sur `tx.nonce` (nonce de compte), aucune dépendance à `tx.id`. OK.

**Conclusion** : fix = (1) feuille Merkle content+sig, (2) `miner` dans la pré-image du bloc,
(3) `integrate_remote_block` matching `tx.id`→`tx.hash`, (4) `prune` remove par `tx.hash`,
(5) fixtures (2 constructions de bloc de test). **Pas de règle de consensus modifiée** (identité
locale `tx.id` et schéma de signature inchangés) ⇒ pas de §4 stop. Suite ci-dessous.

## Correctif + tests — BLK-HASH-1 ✅

**Implémentation (diff logique seule, `ledger.rs` + `node.rs` + `integration_tests.rs`) :**
- **§4.1 Merkle content-addressed** : `compute_merkle_root` — feuilles
  `H(0x00 ‖ tx_content_bytes ‖ signature)`, nœuds internes `H(0x01 ‖ l ‖ r)`, **nœud impair
  promu** (pas dupliqué). `tx_content_bytes(tx)` = `from|to|amount|nonce|type|ts` (ordre fixe,
  zéro itération de map, zéro horloge). *Bonus : la restauration C5 avait régressé ce Merkle vers
  la version simple `hash(tx.id)` (sans séparation de domaine, nœud impair dupliqué = trou
  CVE-2012-2459) — désormais corrigé robustement.*
- **§4.2 `miner` dans la pré-image** du bloc : `seal_block_at` et `validate_block_against_prev`
  partagent `{index}:{prev}:{ts}:{miner}:{count}:{root}`.
- **Cross-node** : `integrate_remote_block` matche bloc↔pending sur `tx.hash` (content-bearing,
  porté) au lieu de `tx.id` (compteur local) → plus de fausse collision inter-nœuds.
- **Dedup cohérent** : `prune` retire de `seen_tx_hashes` par `tx.hash` (l'insert l'était déjà ;
  le remove par `tx.id` ne matchait jamais → fuite).
- **Fixtures §4.4** : les 2 blocs « evil » des `trust_remote_block_*` recalculent leur hash avec
  `miner`. Genèse = hash constant `BLAKE3("QUANTA_GENESIS_2026")` (hors pré-image) → inchangée.

> [!warning] Surprise reportée (pas masquée) — §6
> `int1_three_nodes_converge_on_same_chain` **dépendait du bug** : la fausse collision droppait à
> tort la tx de minage pending de B quand B intégrait le bloc de A (même `tx.id = "tx_1"`). Une
> fois corrigé, le bloc #2 de B porte légitimement **deux** tx de minage (130 QUANTA) > borne
> d'émission/bloc → **rejet correct**. Fix : réduire le 2ᵉ minage de B (20 QUANTA) pour rester sous
> la borne (le test vérifie la **convergence**, pas l'émission — couverte par les `trust_*`). **La
> borne d'émission a fait son travail ; aucun masquage.**

**Tests obligatoires §5 (tous verts) :**
- **T1** `blk_hash_1_content_collision_is_closed` : même `(index, prev, ts, tx_count)`, mineur
  différent ⇒ **hashs différents** (le tuple qui collisionnait).
- **T2** `blk_hash_1_reward_theft_is_rejected_by_core` : rediriger la récompense + `miner` vers un
  attaquant sans recalculer le hash ⇒ **rejeté** (mismatch), attaquant à 0.
- **T3** `blk_hash_1_tampering_contained_tx_is_rejected` : altérer le montant d'une tx de minage
  (sans signature à casser) ⇒ Merkle/hash mismatch ⇒ rejeté.
- **T4** `sim_partition_fork_breaks_safety_and_is_detected` : **pansement timestamp retiré** — le
  fork tient par **contenu** (mineurs différents) ⇒ la sûreté est violée et détectée **pour la
  bonne raison** ⇒ `miner` correctement lié.
- **T5** `blk_hash_1_conservation_holds_through_reorg` : `run_checked` à travers un fork à hauteur
  égale (hash bas livré d'abord ⇒ reorg + re-queue) ⇒ `Σ soldes + brûlé == miné` à chaque pas ⇒
  **pas de double-mint** sur la récompense du perdant re-queue.
- **C1 méta-test (128 runs byte-identiques)** : **vert** — sérialisation canonique sans map/horloge.

**Porte §7.** `cargo test --lib` → **234 passed / 0 failed** (était 230 ; +4, −0) · `clippy --lib
-D warnings` propre · `src/sm/` sans-IO propre · méta-test C1 vert · diff **logique seule** (edits
ciblés, zéro reflow ; `dispatcher.rs` non touché) · `node.rs`/`sim.rs`/`ledger.rs` nightly-fmt propres.

**Auto-revue invariants §3 :** déterminisme ✅ (`tx_content_bytes` ordre fixe, zéro map/horloge/`OsRng` ;
méta-test C1 vert) · arithmétique ✅ (aucune arithmétique de montant nouvelle ; cap d'émission
inchangé) · robustesse Rust ✅ (zéro `unwrap`/`unsafe` hors tests) · **sécurité ✅ (contenu + `miner`
liés au hash ; vol de récompense rejeté (T2) ; Merkle domaine-séparé anti-CVE-2012-2459 ; matching
inter-nœuds et dedup content-addressed)** · mémoire bornée ✅ · tests livrés ✅ (T1–T5 + C1).

**→ T0.8 débloqué** : la porte globale de Phase 0 vérifie maintenant la sûreté sur un hash qui
**lie vraiment le contenu** (plus de faux négatif). Dette restante : slashing (réaction) → ADR-003.

---

## EMIT-1 — Pas de double-mint au reorg + invariant d'émission (2026-06-22)

> Spec : `QUANTA_EMISSION_INTEGRITY.md`. Suite directe de BLK-HASH-1 : le harnais a trouvé que la
> **conservation est aveugle au mint illégitime** (un double-mint apparaît des deux côtés de
> `Σ soldes + brûlé == miné`, l'équation reste vraie). On ferme la **classe entière** avant T0.8.

### §4.0 Audit d'abord (reporté avant tout patch)
- **Prédicat synthétique** : pas de fonction centrale ; closures inline `matches!(a, "NETWORK"|"BURN"|"ESCROW")`
  (`ledger.rs`) + tests directs `from=="NETWORK"||from=="ESCROW"`. Les **expéditeurs** synthétiques
  sont `NETWORK` (minage) et `ESCROW` (release) ; `BURN` est une **destination**, jamais un `from`.
- **Le défaut nommé est réel** : branche fork-gagnant de `integrate_remote_block` — re-queue de
  **toute** tx du perdant absente du gagnant, **y compris** la tx de minage synthétique → peut être
  re-scellée → double-mint. (Le *revert* de cache du perdant porte sur tout le bloc — inchangé.)
- **Trou de validation** : `validate_block_emission` ne vérifie que la **somme** minée ; ni le
  nombre, ni le destinataire. Et il ne tourne que sur le **chemin heureux** (`validate_remote_block`),
  pas sur le **fork-reorg** (qui n'appelle que `validate_block_against_prev`).

> [!warning] §3 STOP déclenché — escaladé avec options, pas deviné
> **Le chemin de production (`mining_loop`) scelle légitimement PLUSIEURS récompenses par bloc** :
> `mine_tx` ajoute 1 tx de minage par tick (`mining_loop.rs:82`) mais le seal n'a lieu que toutes
> les `SEAL_EVERY_N_TICKS = 2` ticks (`:98`) — un leader scelle donc ≥2 de ses propres récompenses
> (toutes `NETWORK→self`, car les minages distants sont no-op à l'admission). `int1` scellait déjà
> un bloc à 2 minages. Donc « ≤1 récompense/bloc » (§4.2) et « count(Mining) ≤ hauteur » (§4.3)
> **casseraient la prod** (rejet de tout bloc honnête). C'est exactement le cas §3 (« le barème est
> un vrai choix ouvert »). **Décision utilisateur : Option A — une récompense par bloc, alignée
> sur la prod.**

### Le correctif (Option A décidée)
- **§4.1 re-queue filtré** : `integrate_remote_block` re-queue **uniquement** les transferts
  utilisateur ; `is_synthetic_sender(from) = NETWORK|ESCROW` exclut la récompense de minage du
  perdant (son effet de cache reste reverté). C'est le défaut immédiat fermé.
- **§4.2 règle de validation** (dans `validate_block_against_prev`, donc **les deux chemins**,
  heureux ET fork) : **au plus une** tx `Mining` ; si présente, `from == "NETWORK"` et
  `to == block.miner`. Sinon → rejet. Ceinture-et-bretelles avec BLK-HASH-1 (`miner` déjà dans le hash).
- **Option A — coalescing au seal** (`coalesce_block_rewards` dans `seal_block_at`, **tous les
  chemins de seal**, prod + cœur) : les récompenses de minage en attente sont **fusionnées en UNE**
  `NETWORK→miner` (montant = Σ). Un bloc à ≤1 minage est rendu **byte-identique** au seal pré-EMIT-1.
  Fusion **déterministe** : id dérivé de `index`, timestamp = `ts` **injecté** (zéro horloge) →
  cœur sans-IO préservé. La prod (minage→self, seal→self) reste cohérente cache↔bloc. **`mining_loop`
  non touché** (le coalescing vit dans le ledger).
- **§4.3 invariant harnais** : 3ᵉ variante `Violation::Emission{seed, node, index, mining_count}`,
  vérifiée **avant** la conservation (l'invariant qui rend le mint illégitime visible — la
  conservation ne le voit pas). Forme structurelle : **aucun bloc ne porte >1 tx `Mining`**.

### Tests obligatoires §5 (tous verts)
- **E1** `emit_1_losing_block_reward_is_not_requeued` : après reorg à hauteur égale, la récompense
  du perdant **n'est PAS dans pending** (`total_minted == stats().total_mined` ⇒ zéro mint pending).
  *Le test que T5 ne pouvait pas être.*
- **E2** `emit_1_two_mining_rewards_in_one_block_is_rejected_by_core` : bloc forgé (hash **correct**
  via `forge_block_at`) à 2 minages ⇒ **rejeté par le cœur** (`blocks_rejected==1`), par la règle de comptage.
- **E3** `emit_1_mining_reward_to_non_miner_is_rejected_by_core` : 1 minage créditant un autre que
  `block.miner` ⇒ **rejeté** (règle destinataire), attaquant à 0.
- **E4** `emit_1_emission_invariant_has_teeth` : chaîne injectée avec un bloc à 2 minages ⇒
  `check_invariants` renvoie `Violation::Emission{seed}`.
- **E5** `emit_1_healthy_run_keeps_emission_safety_conservation` : run sain ⇒ sûreté + conservation
  + **émission** vrais à chaque pas (`run_checked`), vivacité (hauteur 2), et **exactement une
  récompense par bloc non-genèse** sur chaque nœud.
- **C1 méta-test (128 runs byte-identiques)** : **vert** (funding_block à 1 minage ⇒ coalescing
  inactif ⇒ byte-identique).

> [!warning] Surprises reportées (pas masquées) — §6
> Le coalescing a fait échouer **4 tests legacy** qui scellaient des blocs où le **destinataire du
> minage ≠ l'argument `miner`** (`seal_block("miner")` en minant vers `pk_a`), ou attendaient
> plusieurs récompenses/bloc — exactement le **modèle pré-Option-A**. Ce n'est **pas** un bug de
> prod (la prod a toujours `miner == destinataire == self`) : ces fixtures encodaient l'ancien
> modèle. Migrées vers une-récompense-par-bloc :
> - `full_node_lifecycle` : `txs > 5` → `txs == 3` (5 minages coalescés en 1 + transfert + burn).
> - `int2b_cache_survives_snapshot_restore` : `seal_block("miner")` → `seal_block(&pk)` (le mineur
>   scelle sa propre récompense ⇒ cache↔bloc cohérents au restore).
> - `int2_balance_cache_matches_full_scan` : chaque mineur scelle son propre bloc ; les multiples
>   détenteurs viennent des récompenses + transferts.
> - `audit_blk1_fork_reorg_preserves_exclusive_txs` : **renforcé** — porte un transfert utilisateur
>   exclusif ⇒ teste **AUDIT-BLK-1** (tx user re-queue) **ET EMIT-1 §4.1** (minage **exclu** du re-queue).
> - `fork_reorg_through_core_preserves_all_txs` (C4) : `pending 3 → 2` (transfert + burn re-queue ;
>   minage synthétique exclu).

### Porte §7
`cargo test --lib` → **239 passed / 0 failed** (était 234 ; +5 E1–E5, et 4 fixtures legacy migrées) ·
`cargo clippy --lib -- -D warnings` **propre** · `src/sm/` **sans-IO** propre (coalescing sur `ts`
injecté ; mentions OsRng/horloge = doc seulement) · **méta-test C1 vert** · diff **logique seule**
(edits ciblés, zéro reflow ; `dispatcher.rs` **non touché**).

**Auto-revue invariants §3 :** déterminisme ✅ (coalescing déterministe : id depuis `index`, `ts`
injecté, zéro map/horloge ; C1 vert) · arithmétique ✅ (Σ minage en `saturating_add` ; cap d'émission
inchangé ; µQTA u64) · robustesse Rust ✅ (zéro `unwrap`/`unsafe` hors tests ; `forge_block_at` =
`#[cfg(test)]`) · **sécurité ✅ (re-queue sans synthétique §4.1 ; ≤1 récompense/bloc sur les deux
chemins de validation §4.2 ; `to == block.miner` ; invariant émission §4.3 visible avant la
conservation)** · mémoire bornée ✅ (coalescing réduit la taille des blocs) · tests livrés ✅
(E1–E5 + int1 + 4 fixtures migrées).

**→ T0.8 vraiment débloqué** : la porte globale Phase 0 balaie désormais **trois** invariants —
sûreté, conservation, **émission** — donc un double-mint au reorg ne peut plus passer le balayage
multi-seed inaperçu. Dette restante inchangée : slashing (réaction) → ADR-003.

## EMIT-1-VERIFY — audit de complétude (deux moitiés non prouvées) (2026-06-22)

> Tâche de **vérification**, pas de feature. Audit d'abord, code seulement pour un trou réel.
> **Constat : aucun trou fonctionnel. Zéro code de prod/test ajouté.** Deux corrections de
> **documentation** (commentaire de l'invariant) pour graver la forme `≤` au point de vérité.

### Vérification A — le re-queue préserve-t-il les transferts utilisateur ?
**Crainte de la revue** : si le filtre §4.1 avait *sur-corrigé* (vidé tout le re-queue), les
transferts utilisateur seraient silencieusement perdus au reorg — la régression exacte qu'AUDIT-BLK-1
réparait — et aucun test ne le verrait.

- **Prédicat exact (audit)** : `is_synthetic_sender(from) = matches!(from, "NETWORK" | "ESCROW")`
  (`ledger.rs:918`). La boucle de re-queue (`ledger.rs:1071-1088`) `continue` sur (a) tx déjà dans le
  bloc gagnant, (b) expéditeur synthétique ; **tout autre `from` (clé publique réelle) est conservé**
  → `cache_apply_tx` + `pending.push`. Le filtre est **chirurgical**, pas sur-correcteur. La crainte
  ne se matérialise pas dans le chemin de prod.
- **Couverture déjà présente** : `audit_blk1_fork_reorg_preserves_exclusive_txs` (`ledger.rs:1918`)
  prouve **les deux moitiés en une fixture** : la branche perdante porte un transfert utilisateur
  **signé** (signer réel via `CryptoEngine`) *et* sa récompense de minage synthétique ; après reorg,
  le test asserte que le transfert user **EST** re-queue (`requeued` ⇒ AUDIT-BLK-1) **et** que le
  minage **NE l'est PAS** (`!requeued` ⇒ EMIT-1 §4.1). Les drapeaux `saw_user`/`saw_mining`
  garantissent que les deux branches s'exécutent. Si le filtre sur-corrigeait, l'assertion
  `requeued` du transfert user **échouerait** — la régression A serait donc bien attrapée.
- **Verdict A** : déjà couvert. La note de revue (« E1 ne prouve qu'une moitié ») visait le test *sim*
  `losing_block_reward_is_not_requeued` et a manqué le test *ledger* `audit_blk1…` qui couvre les
  deux. **Aucun test ajouté** (pas de churn fabriqué, §4).

### Vérification B — forme de l'invariant d'émission (`≤` vs `==`)
**Question factuelle (pas un choix)** : chaque bloc non-genèse porte-t-il *toujours* exactement une
récompense, même un bloc vide ? **Réponse : NON, déterminée par le code actuel.**

- `coalesce_block_rewards` (`ledger.rs:697`) : si `count(Mining) ≤ 1` ⇒ renvoie les tx **inchangées**.
  Il **n'injecte pas** de récompense quand il n'y en a pas ⇒ un bloc à 0 minage reste à 0.
- `seal_if_pending` (`ledger.rs:736`) scelle **tout** pending non vide ⇒ un pending de transferts
  utilisateur **sans** minage produit un bloc à **zéro** récompense.
- `validate_block_against_prev` (`ledger.rs:863-882`) rejette `mining.len() > 1` ; les contrôles
  `from=="NETWORK"`/`to==miner` ne s'appliquent **que si une récompense est présente**
  (`if let Some(reward) = …`). **Un bloc à zéro minage passe la validation.**
- Preuve vivante : `int2_balance_cache_matches_full_scan` scelle un **bloc reward-free**
  (`integration_tests.rs:147-150`, transferts uniquement) et la suite est **verte**.
- **Forme retenue : `≤` (inchangée).** `== hauteur−1` serait **faux** contre le comportement testé,
  et sur-contraindrait : il rejetterait des blocs user-only légitimes (régression de vivacité).
  L'invariant attrape donc la **sur-émission** (double-mint, la menace EMIT-1) mais **pas** une
  récompense *manquante*. C'est **sans danger** : la sous-émission ne fait qu'**approfondir la
  rareté**, jamais dépasser le plafond dur 100M — la récompense est une *incitation*, pas une
  propriété de sûreté.
- **§4 (règle d'arrêt)** : pas d'escalade. « Un bloc peut-il légitimement avoir zéro récompense ? »
  est **tranché par le code** (coalesce/seal/validate + `int2` vert), pas un choix de conception
  ouvert. Le cas par défaut n'est pas un arbitrage.
- **Action** : documentation seule — commentaire de `Sim::check_invariants` corrigé (« one reward per
  non-genesis block » impliquait `==` ; remplacé par « at most one… zero is legitimate… forme `≤`
  intentionnelle »), au doc de méthode **et** au commentaire inline du test `> 1`. **Aucun test
  ajouté** (la branche `==` du spec, seule à demander un test « récompense manquante », n'est pas
  retenue).

### Porte §5
`cargo test --lib` **vert** (239/0, suite inchangée — zéro test ajouté) · `cargo clippy --lib -D
warnings` **propre** · `src/sm/` sans-IO **propre** · **C1 vert** · diff **logique seule** (deux
commentaires de `sim.rs` ; `dispatcher.rs` **intact**). Livrable = **confirmation documentée** : A déjà
couvert (`audit_blk1…`), B `≤` retenu (fait, pas choix), justifié ci-dessus.

---

## T0.8 — Balayage multi-seed + replay (la porte globale Phase 0) (2026-06-22)

**Tâche** (`QUANTA_T0_8_SWEEP.md`) : tourner toute la machinerie T0.4→T0.7 à travers `N` scénarios
pseudo-aléatoires **dérivés du seed**, vérifiant **les trois invariants** (sûreté + conservation +
émission) à cadence par-pas, toute panne reproductible à l'octet via son seed. **Falsificateur, pas
preuve** — d'où les deux dents anti-vacuité du §5 (couverture réelle + violation plantée).

### Livré (tout dans `src-tauri/src/sm/sim.rs`, le shell de simulation `cfg(test)`)
- **`scenario(seed) -> ScenarioPlan`** : le **générateur pur unique** (§2). Un `Blake3Rng::from_seed`
  dérive l'archétype + tous les boutons ; **jamais** `OsRng`, horloge murale, ni ordre de `HashMap`.
  Appelé par **le sweep ET le replay** — un seul générateur, deux appelants.
- **`execute_scenario(plan)`** : l'exécuteur unique. Interprète la timeline de `Move`s sur un `Sim`
  neuf ; vérifie les invariants **après chaque mutation orchestrée** ET **par pas d'événement** dans
  chaque `Run` (cadence §4 qui attrape la divergence *transitoire*). Borné par `plan.max_steps`.
- **`replay(seed)`** + **`#[test] t0_8_replay_from_env_seed`** (`QUANTA_SIM_SEED=<n>`) : rejoue un
  seed isolé via **le même `scenario(seed)`**, sortie tracée pour le debug.
- **`run_checked_steps`** : variante de `run_checked` retournant le **premier pas fautif** ;
  `run_checked` délègue désormais (appelants existants inchangés).
- **4 archétypes**, chacun une famille de fautes **récupérable** (convergente par construction →
  sweep propre *significatif*) : `faulty_link` (drop/dup/délai garantis > 0 → heal + sync),
  `partition_heal`, `crash_restart`, `equivocation` (deux blocs en conflit livrés **hash-haut
  d'abord** ⇒ réconciliation monotone, zéro fork transitoire).

### Décision de cadrage (honnête, **pas** du masquage — §7)
L'espace aléatoire modélise les fautes que le protocole **est censé** survivre (leader unique de
scellement → pas de fork honnête ; byzantins que le tie-break absorbe). Le cas **connu
irrécupérable** — deux validateurs honnêtes scellant de part et d'autre d'une partition (le
split-brain ADR-001, déjà documenté) — est **délibérément hors de l'espace aléatoire** : il a ses
**propres dents** (`planted_fork_scenario` + `sim_partition_fork_breaks_safety_and_is_detected`).
Exclure une limitation **connue et testée à part** est du *cadrage*, pas du masquage : une **nouvelle**
violation dans l'espace modélisé fait **échouer bruyamment** le sweep avec seed + invariant + pas.
- **`charge de tx`** : la charge modélisée = récompenses de minage (`Mining`, coalescées par bloc) +
  intégration de blocs. Les **transferts user ont été retirés** : `transfer_with_burn` → `build_signed_tx`
  → `next_tx` **lit `Utc::now()`** (horloge murale), ce qui casse la reproductibilité octet (block hash
  non-déterministe — détecté par `sweep_is_reproducible`). La conservation reste exercée par minage +
  intégration ; un transfert signé déterministe (ts injecté) est un suivi propre, **non requis** par la
  couverture §5.1.

### Auto-revue §3
- **Déterminisme** : `scenario` pur(seed) ; outcomes **byte-identiques** sur deux passages de la même
  plage (`t0_8_sweep_is_reproducible`, assert sur l'outcome complet — trace incluse — donc non vacuant
  même sans violation) ; replay byte-identique sur un seed **riche** (`t0_8_replay_is_byte_identical`,
  étend C1 du happy-path au plan complet). Récompense de minage déterministe (`det_mining_tx`, ts
  injecté + hash unique par `seq`) ; blocs d'équivocation déterministes (`forge_equivocation` via
  `forge_block_at`, pas `seal_block`/wall-clock).
- **Arithmétique** : µQTA `u64` partout ; probabilités de faute en **ppm entiers** (pas de float) ;
  `sd_range` sans biais grossier sur petits intervalles ; plafond 100M jamais touché (le sweep observe,
  ne change pas l'émission).
- **Robustesse (bornes, terminaison)** : nœuds 2..=3, pas par `Run` bornés, **`max_steps` global**
  ⇒ l'exécuteur (donc le sweep) **termine** quoi qu'il arrive ; `N` par défaut **64** (≈2,3 s pour les
  6 tests), `QUANTA_SIM_SEEDS` pour le run profond (**256 seeds → 0 violation en ~9 s**).
- **Sécurité (les trois invariants balayés + les deux dents)** : sûreté **par pas** (transitoire
  attrapée), conservation + émission à chaque pas ; **dents prouvées** — `planted_fork_scenario`
  surface une `Safety` portant **le bon seed** + index 1 + premier pas fautif, **et** le replay la
  reproduit à l'octet (`t0_8_sweep_catches_planted_violation`, un seul test prouve dents **et**
  fidélité) ; **couverture prouvée** — `t0_8_sweep_exercises_faults` (plage fixe 0..128, indépendante de
  l'env) atteste que le générateur produit **réellement** partitions, drop/dup/délais, byzantins,
  crash/restart.
- **Mémoire** : aucune fuite de secret (identités seedées de test, jamais loguées) ; pas de clé privée
  en clair ; `Sim`/trace bornés.
- **Tests** : **+6** (245/0 au total) — `clean_default_sweep`, `sweep_is_reproducible`,
  `replay_is_byte_identical`, `sweep_exercises_faults`, `sweep_catches_planted_violation`,
  `replay_from_env_seed`.

### Entrée tracker T0.8
- **`N` par défaut** = 64 (budget : 6 tests ≈ 2,3 s ; sweep seul ≈ 9 s à 256). Override
  `QUANTA_SIM_SEEDS`.
- **Replay** : `replay(seed)` + `QUANTA_SIM_SEED=<n>` (test mono-seed tracé), via le **même** générateur.
- **Couverture exercée** : partitions · drop/dup/délais (reorder) · équivocateurs byzantins ·
  crash/restart — chacune attestée existentiellement sur 0..128.
- **Violation plantée** : split-brain deux-validateurs (partition non guérie) → `Safety` @ index 1,
  seed porté, replay fidèle.

### Porte §9
`cargo test --lib` **vert** (**245/0**, +6 T0.8) · `cargo clippy --lib -D warnings` **propre** ·
`src/sm/` sans-IO **propre** (ajouts confinés au shell `cfg(test)` ; cœur `node.rs` **inchangé** ;
`std::env`/`chrono` du shell sanctionnés par le spec §3/§4) · **C1 vert** · sweep par défaut **dans le
budget** · `git diff` **logique seule** (seul `sim.rs` touché ; `dispatcher.rs` **intact**, 0 marqueur
T0.8) · **aucun masquage, aucun test fabriqué** (les 6 tests prouvent une propriété distincte ; les
transferts retirés pour cause de non-déterminisme réel, documenté).

> **Hors scope / à porter** (inchangé vs spec §10) : mode **soak** (millions de seeds nocturnes) ;
> bascule émission `≤`→`==` à **EMIT-LAW-1** ; **commit du purge crypto-only** = opération git manuelle
> d'Alexandre, **pas l'agent**. Transfert signé déterministe (ts injecté) = enrichissement conservation
> optionnel.

---

## T0.8-HARDEN — rendre la porte réelle (deux phases séquencées) (2026-06-22)

**Tâche** (`QUANTA_T0_8_HARDEN.md`) : T0.8 ne balayait qu'un **sous-ensemble** — transferts user
retirés (fuite d'horloge), réconciliation rendue **monotone** (sens du reorg jamais exercé). Deux phases
séquencées corrigent ça.

### Audit §1.0 (avant de toucher)
- **Lecture d'horloge dans la création de tx** : `next_tx` (`Utc::now().to_rfc3339()`) — le cœur de
  chaque tx ; **plus** un self-check de dérive `±5 min` dans `transfer_tx` (`Utc::now().timestamp()`).
- **Patron C7 à répliquer** : `seal_block` lit l'horloge au bord et délègue à `seal_block_at(ts)` ; le
  cœur déterministe appelle la variante `_at`.
- **Chemin prod tx** : le **seul** appelant non-test de `transfer_with_burn` est `lib.rs:633` (la commande
  Tauri) ; `transfer_tx` n'est appelé qu'en interne (par `transfer_with_burn`). ⇒ garder des wrappers
  lisant l'horloge laisse **tous** les sites d'appel prod inchangés.
- **Obstacle non anticipé par le spec** : la signature **ML-DSA** signe avec `OsRng` (hedged) →
  **non-déterministe** ; or la racine de Merkle lie `tx.signature` ⇒ tout bloc portant un transfert était
  non-reproductible (ce n'était **pas** que l'horloge). Choix `hedged` = propriété de sécurité (anti-faute)
  ⇒ **pas touché en prod** (§4) ; à la place, **aléa injecté** (constitution §3).

### PHASE 1 — temps injecté + signature déterministe, transferts réintroduits
- **`ledger.rs`** : ajout des cœurs sans-horloge `next_tx_at` / `build_signed_tx_at` /
  `transfer_tx_at` / `transfer_with_burn_at` ; les méthodes existantes deviennent des **wrappers de bord**
  qui horodatent au mur (`Utc::now()`) + signent en hedged et délèguent. **Self-check de dérive `±5 min`
  retiré** : il relisait l'horloge (casse le temps injecté) et ne pouvait jamais se déclencher (un nœud ne
  crée pas une tx périmée à lui-même) — la validation reste **sans horloge** (§1.1). Prod **inchangé** :
  `lib.rs:633` appelle toujours le wrapper.
- **Signature déterministe injectée** : `hybrid_crypto::ml_dsa_sign_deterministic` (rnd ML-DSA dérivé en
  BLAKE3 du message, **pas** `OsRng`) + `CryptoEngine::sign_hybrid_det`. Le commutateur `det_sign` est
  câblé dans `build_signed_tx_at` (`false`=prod hedged, `true`=sim déterministe). **Prod conserve `OsRng`**
  (sécurité intacte) ; la sim signe en déterministe (les clés de test n'ont aucun secret à protéger).
- **`sim.rs`** : `Move::Transfer { …, at_ms }` réinjecté (transfert leader→pair, horodaté virtuel,
  ML-DSA déterministe, embarqué dans le bloc suivant). `run_plan` extrait (le cœur de `execute_scenario`
  expose le `Sim` final) ; `chain_tx_tally` compte transferts/burns/minage scellés.
- **Tests Phase 1 (4)** : `determinism_with_transfers` (le test qui avait attrapé la fuite — **vert** à
  présent), `conservation_under_burn` (Σ soldes + brûlé == miné **avec burns présents**),
  `coverage_transfers_and_burns` (transferts **réellement** en blocs + burns **réels**, anti-vacuité),
  `prod_tx_still_timestamps_at_edge` (le chemin prod horodate au réel **et** garde la couche ML-DSA).

### PHASE 2 — archétypes de réconciliation durs
- **2a — reorg hash-bas-d'abord (le sens jamais exercé).** Test dédié
  `single_block_reorg_lowest_hash_first_reconciles` : le perdant (hash bas) porte un **transfert user**
  signé + burn + minage ; après livraison bas-puis-haut, le nœud **bascule** sur le gagnant, **re-queue le
  transfert + le burn** (user, AUDIT-BLK-1), **exclut le minage** (EMIT-1 §4.1, pas de double-mint), les
  trois invariants tenant à chaque pas — un pair converge ensuite par sync. **VERT.** Reorg isolé à un nœud
  pour qu'aucune divergence transitoire inter-nœud ne déclenche (correctement) le check de sûreté par pas.
  **Les deux sens câblés dans l'archétype d'équivocation** (`low_first` tiré du seed) : haut-d'abord
  (monotone, tous les nœuds) **ou** bas-puis-haut isolé au nœud 0 + sync des autres. Le sweep couvre
  désormais les deux sens, et **reste vert** (256 seeds, 0 violation).
- **2b — partition multi-blocs (le gap connu, explicite).** Confirmé : `partition_heal` ne fait avancer
  qu'**un** côté (le leader scelle, l'autre re-sync) — d'où sa convergence. Le nouveau test
  `multiblock_partition_currently_diverges_GADGET_DEFERRED` fait avancer **les deux** côtés de **2 blocs**,
  puis heal + tente la réconciliation. **Constat factuel observé** : le fork-choice single-block **ne
  réconcilie pas** (un bloc height-1 arrivant quand le tip est height-2 échoue le lien prev-hash) ⇒
  **divergence de sûreté persistante** (tips distincts au même index) — le gap ADR-001. **§4 appliqué** :
  conservation **et** émission tiennent **par nœud** (vérifiées directement, car un échec de sûreté
  court-circuite `check_invariants` avant elles) ⇒ ce n'est **que** la sûreté, le gap connu → **asserté +
  marqué gadget-deferred** (cible d'acceptation du gadget ; **inverser** vers `tips[a]==tips[b]` quand il
  atterrira). Si la conservation/émission s'était cassée (fonds dupliqués/perdus, double-mint au heal) =
  **bug nouveau et pire** → le test **panique** (signal STOP/report), jamais « connu ».

### Auto-revue §3
- **Déterminisme** : `scenario` pur(seed) inchangé ; transfert signé **byte-reproductible** (ts injecté
  **+ ML-DSA déterministe** — les deux fuites fermées) ; `determinism_with_transfers` vert ; C1 vert ;
  sweep reproductible (outcome complet) et **propre à 256 seeds avec transferts + les deux sens de reorg**.
- **Arithmétique** : µQTA `u64` ; burn `amount/100` entier ; conservation `Σ + brûlé == miné` exercée
  **avec brûlé réel** ; plafond 100M intact.
- **Robustesse (bornes, terminaison)** : `max_steps` global ; sweep termine ; 12 tests T0.8 ≈ 6 s,
  sweep profond 256 ≈ 12 s.
- **Sécurité (trois invariants + dents)** : sûreté par pas ; reorg flip vérifié (re-queue user, exclusion
  minage, pas de double-mint) ; le gap multi-blocs **explicite** avec garde §4 anti-masquage. **Prod
  signature inchangée** (`OsRng` hedged) — la sécurité anti-faute n'est **pas** affaiblie ; déterminisme
  confiné à la sim. Pas de clé privée loguée.
- **Mémoire** : accesseur `pending_txs()` read-only ; pas de fuite.
- **Tests** : **+6** (251/0) — Phase 1 (4) + 2a (1) + 2b (1).

### Porte d'acceptation §9
`cargo test --lib` **vert** (**251/0**) incluant Phase 1 (4), 2a (reorg bas vert), 2b (gadget-deferred
asserté) · sweep propre **avec transferts** (0 violation, 256 seeds) · `cargo clippy --lib -D warnings`
**propre** (et `--lib --tests` **propre**) · `src/sm/` sans-IO **propre** (cœur `node.rs` : seule la
doc-C7 reformulée ; aléa/horloge confinés au shell + au bord prod) · **C1 vert** · `git diff` **logique
seule** — prod touché : `ledger.rs` (cœurs `_at` + `pending_txs`), `security/mod.rs` + `hybrid_crypto.rs`
(signature déterministe sim) ; `lib.rs`/`commands_v3.rs` **inchangés** (commande de transfert prod
identique) ; `dispatcher.rs` **intact** (0 marqueur).

> **Hors scope / reporté** : mode **soak** ; émission `≤`→`==` à **EMIT-LAW-1** ; **inversion de 2b**
> (asserter la convergence) = cible d'acceptation du **gadget de finalité** (ADR-001) ; commit du purge
> crypto-only = manuel, Alexandre.

---

## SIGN-DET-VERIFY — la signature déterministe ne doit pas pouvoir atteindre la prod (clos)

> Tâche de **vérification** (revue senior post-T0.8-HARDEN), pas de feature. Audit + rapport
> d'abord ; code **uniquement** pour le trou identifié. Diff logique seule.

### Vérification A — `det_sign` atteignable depuis la prod ?

**A.1 — plumbing (constat).** `det_sign` est un **paramètre `bool` runtime**, pas un champ/const/feature.
Il est tissé dans les cœurs *clock-free* du ledger : `transfer_with_burn_at` → `transfer_tx_at` →
`build_signed_tx_at`, dont l'**unique** point de branchement est
`if det_sign { sign_hybrid_det } else { sign_hybrid }`. Mis à `false` par les **trois** wrappers de
bord prod (`transfer_tx`/`transfer_with_burn`/`build_signed_tx`, ledger.rs:316/382/1337) ; mis à `true`
**seulement** dans `sm/sim.rs` (`#[cfg(test)]`, un seul site : sim.rs:2542).

**A.2 — reachability : c'était le « cas mou ».** Avant ce travail, ni `sign_hybrid_det`, ni
`ml_dsa_sign_deterministic`, ni la branche `det_sign=true` n'étaient `#[cfg(test)]` → **tous compilaient
dans le binaire release**, simplement jamais appelés avec `true` en prod. C'est exactement le cas mou du
spec : la fonction faible *compile* dans le release et serait atteignable si un code de prod la mettait
`true`. **Trou (mou) confirmé → durcissement requis.**

**Durcissement appliqué (remède *préféré* du spec : `#[cfg(test)]`).** Le chemin déterministe est
désormais **physiquement absent du release** :
- `hybrid_crypto::ml_dsa_sign_deterministic` → `#[cfg(test)]` (la primitive ; +5 lignes).
- `CryptoEngine::sign_hybrid_det` → `#[cfg(test)]` (le wrapper hybride ; +doc).
- `ledger::build_signed_tx_at` : branche scindée par `cfg`. En `#[cfg(test)]` : l'`if det_sign`
  d'origine. En `#[cfg(not(test))]` : signature **toujours hedged** (`sign_hybrid`) +
  `debug_assert!(!det_sign, …)` (garde ceinture-bretelles ; +14 lignes).
- Vérifié **aux deux configs** : `cargo build --lib` (cfg test=false) compile — la primitive
  déterministe est absente *et* non référencée ; `cargo test --lib` (cfg test=true) compile — la sim
  garde son chemin déterministe. **Prod inchangée** (toujours `OsRng` hedged ; `lib.rs`/`commands_v3.rs`
  intacts). Le pire cas est neutralisé par construction : même un appelant de prod passant `det_sign=true`
  signerait *hedged* (la branche déterministe n'existe pas dans le binaire).

**A.3 — les deux dents d'épinglage (ajoutées, `security/mod.rs::sign_mode_guards`).**
- `prod_signing_is_hedged` : deux `sign_hybrid` du même message ⇒ ML-DSA **différents** (hedged actif ;
  Ed25519 identique → le hedging est purement la couche ML-DSA). **VERT.**
- `sim_signing_is_deterministic` : deux `sign_hybrid_det` ⇒ tuple **byte-identique** (ce dont C1 dépend).
  **VERT.** Anti-vacuité : les deux assertent d'abord que la couche ML-DSA est **non vide** (sinon on
  comparerait deux vecteurs vides).
- Ensemble : le commutateur fait ce qu'il prétend **et** la prod est du bon côté (fort).

**A.4 — note de couverture.** Avant, le mixage d'entropie hedged n'était exercé par **aucun** test (la sim
signe déterministe). `prod_signing_is_hedged` **ferme ce gap** : le primitive hedged est désormais
épinglé et asserté non-déterministe. Le seul bout restant non couvert est le hedged *sur le chemin réel
de production de tx/bloc* (la sim y signe déterministe) — gap **mineur et localisé**, acceptable.

### Vérification B — le self-check ±5min retiré

**B.1 — création-seule (confirmé via git).** Le code retiré (`V8: Timestamp window`) vivait **dans**
`transfer_tx` (fonction de **création**), juste après la construction de la tx avec
`timestamp = Utc::now()` : il comparait l'horodatage que le nœud **venait lui-même de poser** à sa propre
horloge ⇒ drift ≈ 0, jamais > 300 s. **Inutile et inatteignable** sur une tx créée localement ; ce
**n'était pas** une borne de validation des tx **distantes**.

**B.2 — le gap de validation pré-existait.** Les chemins d'**admission/validation** — `verify_tx`,
`apply_verified_remote_tx`, `apply_remote_tx_checked`, `validate_block_against_prev`,
`validate_block_emission` — ne lisent **aucune horloge** et ne **bornent aucun** `tx.timestamp` /
`block.timestamp` (le timestamp n'y apparaît que comme **donnée signée** dans le préimage de
signature/hash). Donc un nœud pouvait **déjà** antidater/postdater librement le champ `timestamp` d'une
tx sans rejet, **avant** le retrait. (Les bornes temporelles existantes sont à d'autres couches et
**non touchées** : fenêtre **±90 s de l'enveloppe gossip** = anti-replay transport sur le timestamp
d'enveloppe, pas de la tx ; **TTL mempool** NET-14 = éviction locale injectée, pas une borne de consensus.)

**B.3 — §4 : pas de régression, pas d'escalade.** Le retrait n'a **pas créé** de gap — il a supprimé un
self-check de création mort, jamais une borne de validation. Le gap (aucune borne temporelle sur le `ts`
d'une tx à la validation) **pré-existait et est inchangé**. L'ajout d'une borne de validité (façon
median-time-past) est un **choix de consensus** = **décision d'Alexandre, hors scope** (correctement
non inventé par l'agent).

### Auto-revue §3
- **Pas de masquage** : le durcissement *renforce* la prod (chemin faible retiré du binaire) sans
  affaiblir le hedged ; les deux dents asserttent des propriétés **opposées** (≠ vs =), aucune n'est
  vacante. Aucun test fabriqué « pour livrer ».
- **Diff logique seule** : 3 fichiers prod (`hybrid_crypto.rs` +5, `security/mod.rs` +58 dont les 2
  tests, `ledger.rs` +14) ; pas de nightly-fmt ; `dispatcher.rs` **intact** (0 marqueur).
- **Sécurité** : `det_sign=true` désormais **impossible** hors `#[cfg(test)]` ; aucune clé privée
  loguée ; signature prod = `OsRng` hedged (résistance aux fautes intacte).

### Porte d'acceptation
`cargo test --lib` **vert** (**253/0**, +2 : `prod_signing_is_hedged`, `sim_signing_is_deterministic`) ·
`cargo clippy --lib -D warnings` **propre** (et `--lib --tests` **propre**) · `src/sm/` sans-IO **propre**
(non touché) · **C1 vert** · build **non-test** (cfg test=false) compile — chemin déterministe absent du
release · `git diff` **logique seule** · `dispatcher.rs` **intact**.

> **Hors scope.** Borne de validité temporelle des tx (si B révèle un gap → c'en est un, pré-existant) =
> décision de consensus d'Alexandre. Commit d'un baseline git propre = manuel, Alexandre.
