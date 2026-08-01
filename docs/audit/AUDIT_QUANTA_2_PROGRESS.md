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

## FORK-CAP-1 — fermer la brèche d'émission sur le chemin fork-reorg (clos, 2026-06-22)

> 🔴 **CRITIQUE** (HARDEN-AUDIT-1, trouvaille #1, confirmée 2×). La branche de validation
> **fork-reorg sautait `validate_block_emission`** : un adversaire réseau pouvait forger un bloc
> de remplacement (même hauteur, hash gagnant) avec une récompense `NETWORK→lui` arbitraire,
> grinder le hash pour gagner le tie-break, et **minter au-delà des 100M** sur tout nœud adoptant
> le fork — la seule propriété de sûreté monétaire qui ne doit **jamais** céder. Diff **logique
> seule** ; **2 fichiers** (`ledger.rs`, `sm/sim.rs`) ; `dispatcher.rs` **intact**.

### §1 — Fermeture de la brèche
- Refactor : `validate_block_emission(&self, block)` délègue à un nouveau
  `pub(crate) fn validate_block_emission_against(block, prior_mined)` — **une seule source de
  vérité** (plafond dur `ledger.rs:862` + borne par bloc `876-887`), paramétrée par l'offre
  **avant** le bloc. Le chemin happy passe `self.stats().total_mined` (`ledger.rs:836`,
  byte-équivalent à l'ancien `let current = …` ; seule modif du chemin linéaire).
- Branche fork-reorg : appelle désormais `validate_block_emission_against(&block, prior_mined)`
  (`ledger.rs:1120`) **avant** toute mutation d'état (`self.chain.pop()` `:1123`), respectant
  **AUDIT-BLK-2** (valider avant muter). Subtilité corrigée : le bloc **remplace** le tip, donc
  `prior_mined = total_mined.saturating_sub(our_tip_mining)` (`:1113-1119`) — exclut la récompense
  du tip retiré, sinon double-comptage → faux rejet d'un reorg honnête près du plafond.

### §2 — Preuve (tests adverses + extension du sweep)
- **5 tests ledger** pilotant la **branche reorg** (les anciens tests de plafond ne pilotaient que
  le happy `tip+1`) : `forkcap_reorg_rejects_multiple_mining_rewards` (a), `…_emission_past_hard_cap`
  (b), `…_emission_per_block_bound` (b′), `…_reward_to_non_miner` (c), + contrôle positif
  **anti-masquage** `forkcap_reorg_accepts_honest_competing_block` (un fork **honnête** de même
  hauteur reste **accepté** : `prior_mined` exclut le tip → 2 QTA valident contre 0).
- **Dents prouvées** : en désactivant la ligne `:1120`, **exactement** (b)/(b′) échouent (l'over-mint
  est accepté) tandis que (a)/(c) et le contrôle positif passent — la brèche est isolée à l'ajout
  d'émission ; (a)/(c) sont déjà rejetés par `validate_block_against_prev` (parité structurelle).
- **DST sweep (sm/sim.rs)** : nouvel **archétype byzantin** `Move::OverEmitReorg` +
  `forge_over_emit_reorg` (livre un bloc legit puis un sur-émetteur même-hauteur, **timestamp
  grindé** au-dessus du legit pour atteindre le tie-break → la prod le **rejette** sur la branche
  reorg) ; câblé dans `scenario_equivocation` (~1/3). **Backstop montant** `Violation::EmissionAmount`
  dans `check_invariants` (réutilise `validate_block_emission_against`, `sim.rs:474-493`) : si la
  prod régressait, le sur-émetteur intégré **hurlerait**. 2 tests dédiés
  (`forkcap_emission_amount_invariant_has_teeth`, `forkcap_sweep_over_emit_reorg_is_rejected`) +
  **garde anti-vacuité** `has_over_emit_reorg()` dans `t0_8_sweep_exercises_faults` (l'archétype ne
  peut plus disparaître en silence du sweep — gap remonté par la vérification adverse, **comblé**).

### §3 — Cartographie de la classe « second chemin de validation divergent »
Énumération de **chaque** contrôle du chemin linéaire et de sa parité sur la branche fork
(vérifiée indépendamment par un agent adverse relisant le fichier) :

| Contrôle | Linéaire | Fork-reorg | Parité |
|----------|----------|------------|--------|
| Continuité d'index | `:808` (`==tip+1`) | `:1075` (`==tip`, même hauteur) | variante **par conception** |
| Lien au parent | `:816` / `:897` (`tip.hash`) | `:1099`/`:1103` (`chain[tip-1].hash`, AUDIT-BLK-2) | variante **par conception** |
| ≤1 tx de minage | `validate_block_against_prev :916` | **même fn** `:1103` | **parité** |
| Coinbase `from==NETWORK` | `:923` | **même fn** `:1103` | **parité** |
| Coinbase `to==miner` | `:929` | **même fn** `:1103` | **parité** |
| Signature par tx (`verify_tx`) | `:936-943` | **même fn** `:1103` | **parité** |
| Merkle root recalculé | `:944` | **même fn** `:1103` | **parité** |
| Hash de bloc recalculé (BLK-HASH-1) | `:944-963` | **même fn** `:1103` | **parité** |
| Plafond dur 100M | `:824→836→862` | `:1120` (FORK-CAP) | parité (variante `prior_mined`) |
| Borne d'émission/bloc | `:836→876-887` | `:1120` (FORK-CAP) | parité (variante `prior_mined`) |
| Signature Ed25519 d'enveloppe | pipeline `dispatcher` ⑧ | **même** appel unique `integrate_remote_block` | uniforme en amont |

**Résultat : aucune divergence de validation résiduelle** entre les deux chemins — chaque contrôle
présent côté linéaire est appliqué côté fork. La classe est **close pour cette paire**.

**Divergences restantes (reportées, NON corrigées ici) :**
- *Par conception, correctes* : le tie-break (hash lexicographique) est une règle de **fork-choice**,
  pas de validation (et le hash n'est cru qu'**après** recalcul `:956-963`, ingrindable) ; la
  mécanique pop/revert/re-queue (`:1123-1164`, AUDIT-BLK-1 / EMIT-1 §4.1) est de la **gestion d'état**
  de reorg, toute la validation s'exécutant **avant** la mutation.
- *Décisions d'Alexandre (§4, à ne pas trancher)* : tie-break **hash vs stake** → [[docs/decisions/ADR-001 — Fork-choice]] ; **acceptation leaderless** (aucun chemin ne vérifie le leader élu — **uniforme**, pas une divergence) → [[docs/decisions/ADR-002 — Validator set & comité BFT]] ; **timestamp de bloc non borné**.
- **Prochain spec recommandé** : le correctif **durable** de la classe = converger l'orchestration
  des deux chemins en **une seule** fonction de validation appelée par les deux (ils partagent déjà
  `validate_block_against_prev` + `validate_block_emission_against`, mais l'enchaînement reste
  dupliqué). Son propre spec, après cette cartographie.

### Vérification adverse indépendante
Workflow de 3 agents (lecture seule) relisant le code : **fix-correctness = `sound`** (brèche close
avant mutation ; `prior_mined` exact, pas de sous/sur-comptage ; happy-path byte-équivalent ; pas de
masquage) ; **sweep-extension = `sound-with-notes`** (1 gap **basse** : archétype non gardé contre
disparition → **comblé** par `has_over_emit_reorg()`) ; **class-map** confirme la parité totale
ci-dessus. Nit relevé (non bloquant) : (a)/(c) sont rejetés par `validate_block_against_prev`, pas par
l'émission — c'est la **parité structurelle attendue**, l'émission étant prouvée par (b)/(b′)+sweep.

### Auto-revue §3
- **Pas de masquage** : le bloc fautif est **rejeté** par la validation (`Err` via `?` à `:1120`),
  jamais filtré ailleurs ; le contrôle positif asserte un **vrai** rejet (tip legit conservé), pas
  un no-op ; les dents ledger **et** sim assertent des propriétés réelles (b/b′ échouent si la ligne
  est retirée).
- **Diff logique seule** : `ledger.rs` **+203 / −1** (la seule suppression = `let current = …` →
  wrapper+helper), `sm/sim.rs` additif ; pas de nightly-fmt ; **`dispatcher.rs` intact**.
- **Constitution** : chemin de fix non-test = `saturating_add/sub` + division entière + `?`, **zéro**
  `unwrap/expect/panic` hors `#[cfg(test)]` ; plafond 100M `MAX_SUPPLY_MICRO` vérifié au consensus sur
  les **deux** chemins ; aucun float sur le chemin de consensus.

### Porte d'acceptation
`cargo test --lib` **vert** (**260/0**, +7 : 5 ledger `forkcap_reorg_*` + 2 sim
`forkcap_emission_amount_invariant_has_teeth` / `forkcap_sweep_over_emit_reorg_is_rejected`) ·
**sweep par défaut vert** avec l'archétype sur-émetteur en rotation (validation rejette → **zéro
violation persistante**) · `t0_8_sweep_exercises_faults` garde l'archétype · **C1 vert** ·
`cargo clippy --lib` et `--lib --tests` **propres** · `src/sm/` sans-IO **propre** · `git diff`
**logique seule** · **`dispatcher.rs` intact**.

> **Hors scope.** Convergence durable des deux chemins en une fonction unique = **spec dédié**.
> Fork-choice (hash vs stake), acceptation leaderless, borne temporelle = **décisions d'Alexandre**
> (ADR-001/002). Commit d'un baseline git propre = **manuel, Alexandre**.

## HARDEN-HYGIENE-1 — passe de durcissement groupée (la classe sans danger) (clos, 2026-06-22)

> Passe groupée de l'hygiénique **indépendant du consensus** (SLICE-CLASS + zeroize + bornes
> mémoire sûres + dents de tests). Chaque fix = unité distincte + son test. **`dispatcher.rs`
> intact**, diff **logique seule**, 8 fichiers. Tout item touchant consensus/éviction =
> **renvoyé en §4**, jamais bundlé.

### §1 — SLICE-CLASS (slicing char-safe)
- **Un seul helper** `pub(crate) fn short(s, max)` (troncature sur frontière UTF-8 via
  `is_char_boundary`, `ledger.rs`) appliqué aux **11 sites** de slicing d'octets sur des champs
  `String` télécommandés de `ledger.rs` : les 2 **durs** `&block.hash[..12]`/`&tip.hash[..12]` de
  résolution de fork (**critique** C-1.3-fork : un hash court/multioctet paniquait la tâche gossip
  unique → nœud sourd, DoS distant) **et** les `&champ[..len().min(N)]` char-unsafe
  (`tx.id`/`block.hash`/`prev_hash`/`miner`, **haute** H-1.3-ledger).
- **Tests** : `slice_short_is_char_safe` (table, dont le cas qui paniquait : 13 octets, coupe à 12
  au milieu d'un `é`) + `slice_multibyte_block_hash_does_not_panic_in_fork_log` (intégration : un
  bloc de fork au hash multioctet atteint l'ex-ligne `[..12]` et est **rejeté** `Err`, plus de
  panic). **Dents** : la ligne paniquait avant, retourne `Err` après.
- **§4 — pas d'effet consensus** : tous les slices sont **affichage seul** (`log!`/`format!`) ; le
  consensus (tie-break, hash, merkle) utilise le champ **complet**, jamais la troncature.
- **🛑 Renvoyé en §4** : les **2 sites `dispatcher.rs`** (C-1.3-`env.sender` **critique**,
  H-1.3-`peer_id` **haute**) — `dispatcher.rs` **doit rester intact** et le correctif racine du
  sender est **PRESIG-ORDER** (exclu de cette passe). `short` est `pub(crate)` pour réemploi par ce
  spec. (Les slices `peer_id` de `willow_node.rs` sont gardés par `EndpointId::from_str` → ASCII
  validé, pas un site vulnérable ; laissés tels quels.)

### §2 — ZEROIZE-SWEEP (frères du trou Ed25519 déjà comblé)
- `CryptoEngine::get_secret_bytes` renvoie désormais un **`Zeroizing<Vec<u8>>`** (auto-effacé chez
  **tout** appelant, type-enforced) et efface le tableau transitoire `to_bytes()` (`security/mod.rs`).
- `get_recovery_key` (`lib.rs`) : le hex plein-secret est `Zeroizing` (le `secret` l'est déjà via le
  retour) — plus rien ne traîne sur le heap après la commande.
- `pq_vault.rs` : la clé AES dérivée Argon2id (`enc_key`, create **et** unlock) est `Zeroizing` ;
  `unlock_identity` construit `sk_arr` **depuis la slice** (plus de `.clone()` non effacé) et efface
  `sk_arr` **et** `sk_bytes`.
- **Tests** : `get_secret_bytes_is_self_wiping` (l'annotation `Zeroizing<Vec<u8>>` = preuve
  **compile-time** ; échoue à compiler si on régresse en `Vec`) + `vault_create_unlock_roundtrip_after_clone_removal`
  (round-trip create→unlock identique après suppression du clone + mauvais mot de passe rejeté =
  auth non affaiblie). Les effacements locaux (`enc_key`, `sk_arr`) sont couverts **par revue**
  (non observables au runtime).

### §3 — MEM-BOUNDS (bornes **sûres** uniquement)
- **`known_peers`** (table de reconnexion **locale**, jamais gossipée → pas de consensus) bornée :
  helper pur testable `register_known_peer` (cap `MAX_KNOWN_PEERS = 1024`, réclame d'abord une
  entrée **exhausted** terminale, sinon refuse l'overflow sans jamais évincer un pair vivant) +
  **GC** des entrées exhausted dans `cleanup_dead_peers` (`willow_node.rs`). **Test**
  `known_peers_table_stays_bounded` (cap tenu, pair vivant jamais évincé, slot exhausted réclamé).
- **🛑 Renvoyés en §4 (non bundlés)** :
  - **Registre `@pseudo`** (`username.rs`) : `apply()` est **commutatif/ordre-indépendant**
    (convergence gossip) ; un cap par-owner naïf est **ordre-dépendant** (quels N pseudos survivent
    dépend de l'ordre) → **casse la convergence**. L'audit lui-même le qualifie de « choix
    anti-spam économique/consensus ». = **décision d'Alexandre + spec convergence-preserving**.
  - **Mempool** (`pending`) : la politique d'éviction **affecte le consensus** (ce qui entre en
    bloc) — explicitement hors passe.
  - **`peer_country_reports`, `peer_info`** : insertion dans **`dispatcher.rs`** (intact) → bornes
    à faire dans le spec qui touchera le dispatcher.

### §4 — TEST-TEETH (donner des dents aux tests vacueux nommés)
- `s3_replay_tx_rejected` : **rejoue** désormais la tx signée via `replay_remote_tx` et asserte
  rejet (dedup `seen_tx_hashes`) **+ soldes inchangés** (avant : une seule tx, jamais de replay).
- `s5_negative_balance_impossible` / `p1_balance_never_negative` : ajout de l'assertion de
  **conservation** (`Σ soldes + burned == minted`) + résidu exact (avant : `let _balance` jeté).
- `int2_balance_cache_matches_full_scan` : comparaison **symétrique** des ensembles de clés
  non-nulles + total (avant : itérait les clés du scan seul → un compte fantôme du cache passait).
- `proptest_transfer_with_burn_*` : assertion de **post-état** (conservation indépendante de la
  formule du burn ; rejet ⇒ solde inchangé). `boundary_amounts` n'avait **aucune** assertion.
- `test_shapley_score_is_the_weighted_sum_of_fractions` (**nouveau**) : épingle le score
  **pré-normalisation** contre une somme pondérée **calculée à la main** (4 fractions distinctes),
  là où `test_shares_sum_to_one` est une **tautologie** de normalisation.
- **Flake résolu** : `d1_fork_resolution_deterministic` et `int1_three_nodes_converge` dépendaient
  d'un `thread::sleep` (collision même-timestamp→même-hash, ~1/5 d'échec). Rendus **content-distinct**
  (mineurs/récompenses différents → hash distinct **par contenu**) ; sleeps retirés. **6 runs de
  suite complète consécutifs verts** confirment la fin du flake.
- **Déjà clos par FORK-CAP** : `sim-emission-count-only` → le backstop montant `Violation::EmissionAmount`
  couvre désormais la dimension montant de l'invariant d'émission.

### Auto-revue §3
- **Pas de masquage** : chaque panique/DoS est **corrigée** (slice char-safe → `Err`, pas avalée) ;
  chaque test renforcé asserte une propriété **réelle** (conservation, rejet de replay, score
  pré-norm) ; aucun test mou.
- **Diff logique seule, sections séparables** : 8 fichiers, `dispatcher.rs` **intact**,
  `cipher.rs`/`username.rs` **non touchés** (username renvoyé en §4) ; pas de nightly-fmt.
- **Constitution** : zeroize sur tout secret exporté ; **zéro** `unwrap/expect/panic` hors
  `#[cfg(test)]` ajouté ; `known_peers` borné sans effet consensus ; locks `peer_info` et
  `known_peers` pris **séparément** dans `cleanup_dead_peers` (pas de chevauchement → pas de
  deadlock).

### Porte d'acceptation
`cargo test --lib` **vert et stable** (**266/0**, +6 nouveaux : 2 slice, 2 zeroize, 1 mem-bounds,
1 shapley ; + 7 tests **renforcés** ; flake éliminé, **6 stress runs verts**) ·
`cargo clippy --lib` et `--lib --tests` **propres** · build **non-test** compile · `src/sm/`
sans-IO **propre** · **C1 vert** · **sweep par défaut vert** · `git diff` **logique seule** ·
**`dispatcher.rs` intact**.

> **Renvoyés en specs dédiés (§4).** SLICE dispatcher (`env.sender`/`peer_id`) → avec **PRESIG-ORDER**.
> Cap registre `@pseudo` **convergence-preserving** = décision d'Alexandre. Borne **mempool** =
> décision d'éviction consensus. Bornes `peer_country`/`peer_info` = avec le spec dispatcher.
> **TX-AUTH-NONCE**, **CRDT** (borner/retirer), **convergence des deux chemins** = inchangés, à venir.
> Commit d'un baseline git propre = **manuel, Alexandre**.

## PRESIG-ORDER — vérifier la signature AVANT toute mutation d'état par-expéditeur (clos, 2026-06-22)

> 🔴 Ferme **NONCE-MEM** (critique) + **SLICE-SENDER/PEERID** (critique/haute) de HARDEN-AUDIT-1.
> **Première modification de `dispatcher.rs`** (le pipeline de sécurité, jusqu'ici intact) — la
> garde « dispatcher intact » des passes précédentes ne s'applique pas ici, c'est l'objet même du
> spec. Changement **contenu à `dispatcher.rs`** (réemploie `short` de HYGIENE).

### §1 — Réordonnancement (la classe « état muté avant la signature »)
- **Avant** : `dispatch_incoming` écrivait les maps par-expéditeur `rate_counters` (rate-limit) et
  `last_nonces` (nonce) — toutes deux `.entry(env.sender).or_insert(...)` — **avant** la vérif
  Ed25519. `env.sender` étant **non authentifié et usurpable**, un attaquant **sans clé** faisait
  croître ces maps sans borne (OOM distant, **NONCE-MEM**).
- **Après** : la vérif de signature (`verify_envelope_signature`) est déplacée **avant** rate-limit
  et nonce. Nouvel ordre : taille → JSON → ban → dedup → compta NET-9 → fraîcheur → **SIGNATURE** →
  rate-limit → nonce → dispatch. Seul un expéditeur ayant **réellement signé** atteint les maps ;
  un sender usurpé est droppé à la signature **sans rien muter**. Aligne le code sur le principe
  **B1 « Verify-Before-Process »** déjà documenté en tête de fichier.
- **Filtres bon marché en amont préservés** (taille/JSON/ban/dedup/fraîcheur) → un flot bad-sig est
  écrémé **avant** de payer la vérif (seuls les messages uniques+frais la paient).
- **Bonus** (relevé par la revue) : ferme aussi un **empoisonnement de nonce** — sous l'ancien ordre
  un attaquant usurpait `env.sender=victime` avec un nonce haut, poussant le high-water de la
  victime et faisant **rejeter ses vrais messages** ; désormais seul un envelope **signé** avance
  le high-water.

### §2 — Slicing char-safe (les 2 sites dispatcher différés de HYGIENE)
- Les **27** slices d'octets sur champs `String` télécommandés (`env.sender`, `sender`, `peer_id`,
  `sender_pk`, `from`, `peer_id_clone`) → `short()` (helper char-safe de HYGIENE, `pub(crate)`).
  **Zéro** slice d'octet brut restant dans `dispatcher.rs` (grep `[..]` = ∅). Ferme
  **SLICE-SENDER** (critique) + **SLICE-PEERID** (haute) : plus de panic multioctet → nœud sourd.

### §3 — Test comportemental (pilote le vrai pipeline)
- `presig_bad_signature_writes_no_per_sender_state` (`#[tokio::test]`) : construit un `AppState`
  in-memory, livre une enveloppe **bad-sig à sender usurpé** (fraîche, id unique, non bannie → elle
  traverse réellement ban/dedup/fraîcheur), appelle `dispatch_incoming`, et asserte
  `last_nonces` **ET** `rate_counters` **vides**. **Dents prouvées** : en simulant l'ancien ordre
  (écritures avant vérif), le test **échoue** (le sender usurpé crée une entrée).
- `presig_valid_signature_is_admitted_and_tracked` : une enveloppe correctement signée est admise
  et **enregistrée** (high-water nonce = 1, bucket rate présent) → la voie légitime n'est pas cassée.

### Vérification adverse indépendante
Workflow 3 lentilles (lecture seule) : **reorder-correctness = `sound`** (aucun contrôle sauté ;
anti-replay nonce identique, juste déplacé ; `ban→ReportPeer` intact ; filtres bon marché bien en
amont), **dos-tradeoff = `sound`** (dedup LRU 100K + fraîcheur ±90s écrèment le flot → coût CPU
borné par le transport, **net gain** vs croissance mémoire non bornée ; NET-9 = `get_mut` seul, pas
d'insertion), **antireplay-and-slices = `sound`** (anti-replay intact ; **0** slice brut ; test
**non-vacueux**). **Zéro problème de fond.** Seul **nit** (2 lentilles) : commentaires « ±5 min »
périmés vs ±90s réel → **corrigés** (`dispatcher.rs:233`, `:293`).

### §4 — Renvoyé en spec dédié (non bundlé)
- **Cap des maps `last_nonces`/`rate_counters`** : après réordonnancement, le vecteur **gratuit**
  (sender usurpé, sans clé) est **fermé** ; il reste un résidu **Sybil** (N keypairs valides → N
  entrées, borné par le coût de keygen + le transport). Le combler proprement = **éviction
  par-péremption ≥90s** (anti-replay-safe : un message ré-émis après éviction serait de toute façon
  périmé à la fraîcheur), ce qui exige d'ajouter un **last-seen par entrée à `NonceTracker`** →
  touche la sémantique anti-replay = **décision + spec dédié** (couplé à **TX-AUTH-NONCE**).
- **Compromis CPU assumé** : la vérif après dedup/fraîcheur est bornée ; documenté.

### Porte d'acceptation
`cargo test --lib` **vert** (**268/0**, +2 : `presig_bad_signature_writes_no_per_sender_state`,
`presig_valid_signature_is_admitted_and_tracked`) · `cargo clippy --lib` et `--lib --tests`
**propres** · build **non-test** compile · `src/sm/` sans-IO **propre** · **C1 vert** · **sweep par
défaut vert** · `git diff` **contenu à `dispatcher.rs`** (+ réemploi `ledger::short`).

> **Renvoyés.** Cap maps nonce/rate (anti-replay-safe) + **TX-AUTH-NONCE** (le hang ~2⁶⁴ + censure,
> niveau ledger) = spec dédié. **CRDT**, **convergence des deux chemins de validation** = à venir.
> Commit d'un baseline git propre = **manuel, Alexandre**.

## TX-AUTH-NONCE-1 — authentifier et borner le nonce et le hash de tx (clos, 2026-06-22)

> 🔴 CRITIQUE. Le **nonce** et le **hash** d'une tx vivaient **hors du préimage signé** →
> falsifiables sur une tx signée (griefing/censure), et une boucle de gap de nonce pouvait être
> poussée à **~2⁶⁴ sous le lock du ledger** (hang global). Changement **profond** (le préimage
> signé). Pré-genèse → **aucune migration**. Diff logique seule ; **C1 préservé**.

### Étape 0 — cartographie FORK-CAP (rapportée, verbatim)
Relecture de la §3 FORK-CAP : la branche reorg vérifie, **via le `validate_block_against_prev`
partagé** (`:1103`), la **signature par tx** (`verify_tx`, parité), le **merkle** recalculé, le
**hash de bloc** recalculé (BLK-HASH-1), et la structure coinbase — **tous en parité**. Après
FORK-CAP elle applique aussi l'émission (`:1120`). **« Résultat : aucune divergence de validation
résiduelle. »** → la branche reorg ne saute **que l'émission** (close), **jamais la signature ni la
structure** → **PROCÉDER** (pas d'escalade vers la convergence des chemins).

### §1 — Borner le hang (le DoS immédiat, indépendant de l'auth)
- `apply_verified_remote_tx` rejette `nonce > current + MAX_NONCE_GAP` **avant** d'appliquer
  (mutation nulle), et l'avance du high-water passe de la boucle `for _ in current..new_hw` (qui,
  avec `nonce≈u64::MAX`, itérait **~2⁶⁴ sous le lock ledger**) à `raise_nonce_high_water` (**set
  O(1)** monotone). C'est **l'unique** avance partagée → couvre **tous** les appelants, dont
  l'appel **direct** du dispatcher (`dispatcher.rs:858`). **§4 — valeur de borne** :
  `MAX_NONCE_GAP = 1_024` (tolère le réordonnancement gossip) = **choix de politique signalé**
  (l'**existence** de la borne est requise ; la **valeur** est d'Alexandre). Rejet logué (debug).

### §2 + §3 — Préimage signé + recalcul du hash
- **Un seul** helper `tx_signing_preimage(id,from,to,amount,ts,type,nonce)` = ancien préimage
  6-champs **+ `:nonce`**. Utilisé **identiquement** aux **3 sites** : création (`next_tx_at`),
  récompense NETWORK (`coalesce_block_rewards`, nonce 0), vérification (`verify_tx`). La signature
  **couvre** désormais le nonce → un tiers ne peut plus l'altérer.
- **§3** : `verify_tx` **recalcule** le hash depuis le contenu et **rejette** (`Ok(false)`) si
  `tx.hash` du fil ≠ recalculé → plus de malléabilité (le dedup `seen_tx_hashes` est clé sur
  `tx.hash`).
- **BLK-HASH-1** : le merkle de bloc (`tx_content_bytes`) liait **déjà** le nonce + la signature,
  **indépendamment** de `tx.hash` → blocs **inchangés**, deux nœuds à jour acceptent exactement les
  mêmes blocs (pas de split). `tx.hash` ne sert que d'identité/dedup.

### §4 — Cap anti-replay-safe des maps `NonceTracker` (différé de PRESIG-ORDER)
- `NonceTracker` gagne `last_seen` + `note_activity_and_prune` : **péremption** (évince les entrées
  oisives `≥ NONCE_ENTRY_TTL_SECS = 120s`, **> la fenêtre de fraîcheur ±90s** → un rejeu d'une
  entrée évincée est **déjà périmé**, droppé à la fraîcheur **avant** la porte nonce =
  **anti-replay-safe**) **ET** **borne de taille absolue** (`MAX_TRACKED_SENDERS = 100_000`,
  éviction du plus ancien quand pleine). O(1) sous le cap ; le prune O(n) ne tourne qu'au-dessus.
  Les 3 maps restent synchrones (sous-ensembles de `last_seen`).
- **🛑 Garde-fou « `dispatcher.rs` intact » vs §4** : le §4 **exige** le cap sur `NonceTracker`
  (qui vit dans `dispatcher.rs`). **Résolution rapportée** : seules les **internes de
  `NonceTracker`** sont touchées ; le **pipeline `dispatch_incoming` est byte-identique** (vérifié
  par diff — l'ordre PRESIG-ORDER est intact).
- **§4 — politiques signalées (décisions d'Alexandre)** : valeurs exactes `MAX_TRACKED_SENDERS`,
  `NONCE_ENTRY_TTL_SECS` (contrainte : `> 90s`) et **l'ordre d'éviction par taille**. **Résidu
  documenté** : sous un flood Sybil **rapide** (>100K clés fraîches en <120s), la borne de taille
  peut évincer une entrée `< TTL` et rouvrir brièvement sa fenêtre de nonce — **mitigé** par le
  dedup `seen_messages` (id d'enveloppe, LRU 100K) + les enveloppes **signées** (PRESIG). Acté
  comme tradeoff §4, **non tranché en dur en silence**.

### Vérification adverse indépendante
Workflow 3 lentilles (lecture seule) : **preimage-determinism = `sound`**, **hang-and-auth =
`sound-with-notes`**, **cap-antireplay = `sound`**. Confirmé : hang éliminé pour **tout** appelant ;
**un seul** préimage (grep arbre = aucun site manqué hors helper ; blocs distincts corrects) ; C1
byte-identique (préimage = fonction pure, uniforme) ; **aucun** chemin d'admission ne contourne
`verify_tx` ; maps synchrones + bornées ; pipeline PRESIG intact ; tests `s12_*` inchangés. Nits
traités : log de rejet de gap **ajouté** ; test du **chemin borne-taille** (tout-frais) **ajouté**.
**Décision relevée (à Alexandre)** : le changement de préimage modifie le `tx.hash` des tx
**utilisateur** ; au redémarrage, une chaîne persistée contenant des tx user **ancien-format** est
rejetée par `verify_chain` → **repart de la genèse**. **Conforme au spec** (« pré-genèse, aucune
migration »), mais `TORUS_PROTOCOL_VERSION` reste **2** → **bump de version wire** (pour signaler le
changement de format) = **décision d'Alexandre** (non faite ici : choix de protocole).

### Auto-revue §3
- **Pas de masquage** : le hang et la falsifiabilité se ferment **à la racine** (borne O(1) +
  préimage signé + recalcul de hash), jamais par un test mou ; les 6 tests assertent des propriétés
  réelles, dents prouvées (désactiver la borne fait **échouer** l'anti-hang).
- **Diff logique seule** : `ledger.rs` +204 (§1/§2/§3/§5), `dispatcher.rs` +97 (`NonceTracker` §4 +
  test) ; **pipeline `dispatch_incoming` byte-identique** ; `sm/sim.rs` et `ledger_types.rs`
  **inchangés** ; pas de nightly-fmt.
- **Constitution** : `Result/?`, **zéro** `unwrap/expect` hors `#[cfg(test)]` ajouté ; `saturating_*`
  partout ; aucun float consensus ; nonce **désormais** dans le préimage signé + recalculé.

### Porte d'acceptation
`cargo test --lib` **vert** (**274/0**, +6 : `txauth_far_ahead_nonce_rejected_no_hang`,
`txauth_forged_nonce_breaks_signature`, `txauth_malleable_hash_rejected`,
`txauth_valid_signed_tx_verifies_and_hash_binds_nonce`,
`txauth_nonce_tracker_eviction_is_bounded_and_anti_replay_safe`,
`txauth_nonce_tracker_size_bound_holds_when_all_fresh`) · `cargo clippy --lib` et `--lib --tests`
**propres** · `src/sm/` sans-IO **propre** · **C1 vert** · **sweep par défaut vert** · `git diff`
**logique seule** (`ledger.rs` + `NonceTracker`) · **pipeline `dispatch_incoming` intact**.

> **Renvoyés / décisions d'Alexandre.** `MAX_NONCE_GAP`, `MAX_TRACKED_SENDERS`, `NONCE_ENTRY_TTL_SECS`,
> ordre d'éviction par taille + son résidu Sybil-rapide ; **bump `TORUS_PROTOCOL_VERSION`** (format
> de préimage changé, pré-genèse). **CRDT** (borner/retirer), **convergence des deux chemins de
> validation** = derniers items du backlog, à venir. Commit baseline git = **manuel, Alexandre**.

## CRDT-BOUND-1 — retirer ou borner le ledger CRDT fantôme (clos, 2026-06-23)

> 🟠 Dernier item de HARDEN-AUDIT-1. **Audit d'abord** : vivacité = un fait dans le code.
> Verdict : **vestige référencé mais inerte** → biais conservateur + garde-fou → **borner** (pas
> retirer), escalader **garder-vs-retirer** à Alexandre. Diff **contenu à `consensus.rs`** ;
> `dispatcher.rs` **intact**.

### §1 — Constat de vivacité (le fait, avec preuves — vérifié indépendamment)
- **`ConsensusEngine::merge_peer` : ZÉRO appelant** en prod (seuls la déf `consensus.rs` + un
  `.merge()` dans `simulation.rs` qui est `#[cfg(test)]`). → **le CRDT n'est jamais synchronisé
  entre nœuds** ; sa raison d'être (convergence) est **morte**.
- **`CrdtLedger::balance_of` : ZÉRO consommateur** (grep arbre vide). → les **valeurs** de solde
  CRDT ne sont **jamais lues**. L'état autoritatif est **exclusivement** le `balance_cache` du
  ledger linéaire (soldes, conservation, validation, fork — tous le linéaire).
- **Seule lecture prod** : `account_count()` (`lib.rs:393`, stat frontend `get_consensus_stats`).
  Écritures : `dispatcher.rs:847-848` (Transfer distant), `mining_loop.rs:86-87` (minage local),
  `lib.rs:645-646` (transfert local). `snapshot/restore` ne lit qu'un **count** (non load-bearing).
- **Verdict** : c'est le **double-ledger « Phase 3 » vestigial** de l'ère social/marketplace.
  **Référencé** (account_count + persistance + écritures) → **non mort proprement** ; et le retrait
  **toucherait `dispatcher.rs`** (l'écriture `:847`). **Biais conservateur §3 + garde-fou
  `dispatcher.rs` intact → BORNER (§2b), pas retirer.**

### §2b — Corriger les deux bugs (vivant → on ne retire pas)
- **Boucle O(montant) → O(1)** : `for _ in 0..clamped { inc/dec; apply }` (jusqu'à **10M itérations
  par tx sous le lock consensus** = DoS CPU) remplacée par `PNCounter::inc_many/dec_many` (crdts 7,
  `pncounter.rs:125/133`). **Sémantiquement identique** (vérifié contre la source : `inc_many(N)`
  produit le **même** Op final que N `inc()` — `merge()` et `balance_of` inchangés).
- **Croissance bornée** : cap dur `MAX_CRDT_ACCOUNTS = 100_000` via `admit(addr)` — garde les
  **K clés lexicographiquement les plus petites** (évince le max courant si l'entrant est plus
  petit). Politique **ordre-indépendante** (ensemble gardé = fonction pure du SET, pas de l'ordre →
  « pas un compteur dépendant de l'ordre », la contrainte du cap `@pseudo`). `merge()` **la méthode**
  reste commutative/idempotente (la borne est sur le chemin d'écriture, pas dans `merge`).
- **Bonus SLICE-CLASS** : les 2 slices d'octets `&recipient/&sender[..min(12)]` (`consensus.rs:46/65`,
  panic multioctet dans le log de cap, manqués par HYGIENE) → `short()`.

### §4 — Décisions renvoyées à Alexandre (non tranchées)
- **Garder-vs-retirer (le vrai arbitrage)** : le CRDT est **retirable** — merge mort, soldes non
  lus. La vérif adverse a même trouvé que **`get_consensus_stats` (sa seule lecture prod) n'a aucun
  invocateur frontend vivant** (grep `src/` = ∅) → **même `account_count` est UI-mort** → l'argument
  penche vers **retirer** (du code mort = surface d'attaque). **Mais** le retrait touche
  `dispatcher.rs` + recâble la persistance + 6 fichiers = **refonte comptable = décision
  d'Alexandre** (garder-borné, fait ici, vs supprimer le double-ledger). **Non refactoré en silence.**
- **Politique** : valeur `MAX_CRDT_ACCOUNTS`, ordre d'éviction (lex-smallest-K).
- **Caveat merge** : l'éviction **perd** l'état d'un PN-Counter, irrécupérable par `merge` — **sans
  effet aujourd'hui** (merge mort), mais si la **sync CRDT inter-nœuds est ressuscitée**, une borne
  dure sur une map PN-Counter mergée est un **vrai choix de conception** à revisiter.

### Vérification adverse indépendante
2 lentilles (lecture seule) : **liveness = `sound`** (les 4 faits confirmés ; aucun consommateur
manqué ; bornage-pas-retrait correct), **bound-correctness = `sound`** (`inc_many ≡ N·inc` prouvé
contre la source crdts 7.3.2 + le test du crate ; borne mémoire réelle + ordre-indépendante ;
**aucune régression** au ledger autoritatif/conservation/consensus — stores `consensus` et `ledger`
séparés). **Zéro problème de fond** ; nits : `get_consensus_stats` UI-mort (renforce le verdict
vestige → ajouté à l'escalade §4), le slice `debit` était défensif (le `credit`/recipient était le
vrai site joignable).

### Auto-revue §3
- **Pas de masquage** : la boucle et la croissance se ferment **à la racine** (O(1) + cap
  ordre-indépendant) ; le no-op `admit` ne touche **que** le shadow-ledger non autoritatif (soldes
  non lus) — **jamais** le ledger linéaire/la conservation. Dents prouvées (cap désactivé → la map
  monte à 100050, le test échoue). Pas de demi-retrait/squelette neutralisé.
- **Diff logique seule** : **un seul fichier**, `consensus.rs` (+102) ; `dispatcher.rs`, `lib.rs`,
  `willow_node.rs` **inchangés** ; pas de nightly-fmt.
- **Constitution** : `Result`/no-panic (que `unwrap_or`) ; `tokio` non concerné ; aucun float
  consensus ; le CRDT reste non autoritatif.

### Porte d'acceptation
`cargo test --lib` **vert** (**276/0**, +2 : `crdt_large_amount_is_o1_no_loop_dos`,
`crdt_balances_bounded_and_kept_set_is_order_independent`) · `cargo clippy --lib` et `--lib --tests`
**propres** · `src/sm/` sans-IO **propre** · **C1 vert** · **sweep + conservation verts** ·
`git diff` **logique seule** (`consensus.rs` seul) · **`dispatcher.rs` intact**.

> **Dernier verrou de l'audit posé.** Reste de la phase de durcissement : la **convergence des deux
> chemins de validation** (petite — la cartographie FORK-CAP a montré qu'ils partagent déjà le cœur
> `validate_block_against_prev` + `validate_block_emission_against`). Décisions d'Alexandre :
> garder-vs-retirer le CRDT, le bump `TORUS_PROTOCOL_VERSION`, les valeurs de politique, et le
> gadget de finalité (ADR-001). Commit baseline git = **manuel, Alexandre**.

---

## ADR-005 — Agrégation des votes & certificats de finalité (cadré, **non tranché**) (2026-06-23)

> Spec `QUANTA_ADR_005_AGGREGATION.md`. **Pas une tâche de code** : c'est une **décision de
> conception** (ligne 6-9 de la spec) dont l'implémentation est explicitement **différée à des
> specs ultérieures, après ratification**. Tout y est **§4 hard-stop**. Action faite : **landé le
> brouillon comme ADR canonique** au registre `docs/decisions/`, **câblé les deux index**, et
> **remonté les paramètres à Alexandre sans en trancher aucun**. Status gravé : **proposé / open**.

### Ce qui a été fait (documentation seule, zéro code)
- **Record canonique créé** : `docs/decisions/ADR-005 — Agrégation des votes & certificats de
  finalité.md`, en **style maison** (frontmatter `type: adr` / `id: ADR-005` / `status: open` /
  `decision-class: 🛑 hard-stop` / `updated: 2026-06-23` ; fil d'Ariane `← [[README|Registre ADR]]` ;
  callout `[!info] PROPOSÉ`). Contenu **fidèle** au brouillon (contexte BLS-vs-ML-DSA, décision
  hybride proposée, modèle de sécurité « finalité PQ-**ancrée** », séquencement étape 1 PQ-direct →
  étape 2 BLS+ancrage, alternatives, conséquences, questions ouvertes).
- **Index câblés** :
  - `docs/decisions/README.md` : ligne ADR-005 ajoutée à la table « Les ADR » (classe 🛑, statut
    **PROPOSÉE**) ; méta-décision **§7 « Signatures »** de l'umbrella passée de ⬜ *OUVERTE* à 🟡
    *cadrée par ADR-005* ; `updated:` → 2026-06-23.
  - `docs/00 — Pilotage QUANTA.md` : la ligne « Signatures (agrégation votes) » pointe désormais
    sur `[[ADR-005 …]]` au lieu de `README §7`, statut **PROPOSÉE — hybride BLS + ancrage PQ**.
- **Brouillon racine** `QUANTA_ADR_005_AGGREGATION.md` **conservé** (la spec/source qui a piloté la
  tâche, comme tous les `QUANTA_*.md` au root ; le livrable est le record canonique sous
  `docs/decisions/`, à l'identique du couple `QUANTA_HARDENING_AUDIT.md` → `docs/AUDIT_HARDENING_PHASE0.md`).

### §4 — Renvoyé à Alexandre (non tranché ici)
Le statut reste **proposé** ; **aucun** paramètre n'est résolu par Claude (règle §4 : cadrer +
remonter, pas trancher). À ratifier :
- **K** (intervalle d'ancrage PQ) — borne la fenêtre où un futur adversaire quantique pourrait
  forger la finalité *récente* ; compromis sécurité/coût.
- **Taille de comité** + **seuil de quorum** (tolérance byzantine ⅓).
- **Format de certificat** (agrégat BLS + clés agrégées ; structure de l'ancre PQ d'époque).
- **Courbe BLS** (p. ex. BLS12-381) + **schéma PQ d'ancrage** (ML-DSA, cohérent tx).
- **Arbitrage de fond** : tolérer une primitive **non-PQ** (BLS) sur la finalité récente, ou
  exiger **100 % hybride PQ** (= étape 1 PQ-pur indéfiniment). C'est la méta-décision **§7
  Signatures** du `DESIGN-CONSENSUS-DAG-BFT`, même axe que l'ECVRF d'ADR-004.
- **Articulation slashing↔agrégat** (attribuer une faute dans un agrégat BLS) → à la conception du
  gadget, dépend d'ADR-003.

### Auto-revue §3
- **Rien tranché en silence** : le record est gravé **PROPOSÉ/open**, jamais `accepted` ; la
  décision hybride y est une **recommandation cadrée** (options + conséquences + ce-dont-j'ai-
  besoin-de-toi), exactement le rôle Claude du registre ADR — la ratification reste à Alexandre,
  comme ADR-002/004.
- **Honnêteté préservée** : modèle de sécurité formulé « finalité PQ-**ancrée** » (pas « PQ pure »),
  note d'honnêteté whitepaper conservée ; les tx restent signées PQ de bout en bout.
- **Diff documentation seule** : 1 fichier créé (`docs/decisions/ADR-005 …`) + 2 index touchés
  (`README.md`, `00 — Pilotage QUANTA.md`) ; **zéro `.rs` touché**, `dispatcher.rs`/ledger/`src/sm/`
  **intacts** → pas de porte de tests Rust applicable (rien de compilé n'a changé). Suite, C1,
  sweep, conservation **non impactés** (aucun code).

### Porte d'acceptation (ADR = record + escalade, pas code)
ADR-005 **gravé** au registre canonique avec frontmatter maison · **2 index** cohérents (README +
Pilotage) · **status proposé**, tous les 🛑 listés et **renvoyés en §4** · brouillon source conservé ·
**aucune décision résolue par Claude**.

> **Débloque, une fois ratifié**, la **conception protocolaire du gadget de finalité** (machine à
> états des votes, époques, ancrage, certificats) — le gros morceau restant de la trajectoire
> consensus (Option 1, [[docs/decisions/ADR-001 — Fork-choice|ADR-001]]). Côté durcissement, ne
> reste que la **convergence des deux chemins de validation**. Décisions d'Alexandre en attente :
> ratification ADR-005 (K/comité/quorum/courbe), ADR-003 (slashing), ADR-004 (aléa), garder-vs-
> retirer le CRDT, bump `TORUS_PROTOCOL_VERSION`. Commit baseline git = **manuel, Alexandre**.

---

## ADR-005 (révision) — Alexandre tranche : **post-quantique pur, par époque** (supersède l'hybride) (2026-06-23)

> Spec `QUANTA_ADR_005_AGGREGATION_1.md`. Alexandre a **tranché** la méta-décision §7 « Signatures » :
> au lieu de l'hybride BLS + ancrage PQ que l'ADR-005 *proposait*, la finalité sera **post-quantique
> pure (ML-DSA), finalisée par époque**. L'ADR-005 était **OUVERTE/proposée** (jamais acceptée) → flux
> normal `OUVERTE → il tranche → ACCEPTÉE` : **mise à jour en place** (même ID, même fichier), l'hybride
> devient une **alternative rejetée** au sein d'ADR-005. **Toujours pas de code** (décision de conception).

### Le renversement (et pourquoi il tient)
La tension « BLS compact mais non-PQ » vs « ML-DSA PQ mais lourd » **se dissout** sous deux faits :
1. **Vote éphémère ≠ tx éternelle** : l'urgence PQ est sur ce qui doit rester infalsifiable des années
   (les **tx**, déjà PQ) ; un vote de finalité ne vit que dans sa fenêtre de décision → pas de
   « harvest-now-forge-later » dessus → l'argument « trou BLS acceptable » tombe.
2. **Coût = granularité** : « N × 3,3 Ko » n'est vrai que **bloc-par-bloc**. **Par époque** → un cert
   par lot ; ≈ 50 validateurs ⇒ ≈ **165 Ko/époque**, amortis + élagables. Gérable.
→ **PQ pur viable et préférable** à comité modeste, derrière une **abstraction de certificat** (BLS/SNARK
restent un remplacement **local** futur, pas une réécriture).

### Ce qui a été fait (documentation seule, zéro code)
- **`docs/decisions/ADR-005 — …`** réécrit : titre `(post-quantique pur, par époque)` ; frontmatter
  `status: accepted` + `decided: 2026-06-23` + `ratification: à confirmer formellement (Alexandre)` +
  `supersedes-proposal: hybride BLS + ancrage PQ`. Callout `[!success] DÉCISION`. Sections refondues
  (contexte = les 2 observations ; décision PQ pur/époque + abstraction ; modèle « PQ pur **sans
  astérisque** » ; pourquoi-pas-l'hybride ; hybride **rejeté** en alternatives ; évolution SNARK différée).
- **Index** : `README.md` → ligne ADR-005 **ACCEPTÉE** (PQ pur/époque) + méta-décision §7 « Signatures »
  passée ⬜/🟡 → **✅ tranchée** (PQ pur, ratif. formelle en attente). `00 — Pilotage` → ligne Signatures
  **ACCEPTÉE** (PQ pur ML-DSA par époque).
- **Entrée tracker précédente conservée** (log append-only) : elle décrivait fidèlement la *proposition*
  hybride au moment où elle a été landée ; cette entrée-ci en **acte le renversement**.
- **Spec source** `QUANTA_ADR_005_AGGREGATION_1.md` conservée au root.

### §4 — Encore renvoyé à Alexandre (non tranché ici)
La **direction** est tranchée *par Alexandre* (je grave, je ne tranche pas). Restent **🛑** : **taille de
comité** + **seuil de quorum** (⅓) ; **longueur d'époque** ; **niveau ML-DSA** (65 probable) ; **format
du certificat d'époque** + **stratégie d'élagage** ; et la **ratification formelle** elle-même. Surveillé
(pas à résoudre) : le **seuil de comité** au-delà duquel l'agrégation redeviendrait nécessaire.

### Auto-revue §3
- **Rien tranché en silence, rien sur-réclamé** : `accepted` reflète le statut **qu'Alexandre a écrit**
  dans la spec (« accepté »), tandis que `ratification: à confirmer formellement` + le callout gardent
  le **caveat** honnête ; je n'ai inventé aucune valeur de paramètre (tous listés 🛑).
- **Convention ADR respectée** : on **ne réécrit pas une ADR *acceptée*** — celle-ci était **proposée**,
  donc l'update en place est le cycle de vie normal ; l'hybride survit comme **alternative rejetée**
  tracée (pas d'effacement de l'historique : tracker précédent intact + `supersedes-proposal`).
- **Diff documentation seule** : 0 `.rs` ; `dispatcher.rs`/ledger/`src/sm/` intacts ; pas de porte de
  tests Rust applicable (rien de compilé n'a changé) ; C1/sweep/conservation non impactés.

### Porte d'acceptation (ADR = record + escalade, pas code)
ADR-005 **regravé ACCEPTÉE** (PQ pur/époque) avec caveat ratification · **2 index** cohérents (méta-
décision §7 **tranchée**) · hybride **rejeté & tracé** · sous-paramètres 🛑 **renvoyés en §4** · spec
source conservée · **aucune valeur résolue par Claude**.

> **Débloque** la **conception protocolaire du gadget** sans dépendance crypto résiduelle : on construit
> **en ML-DSA dès l'étape 1**, finalisation **par époque** (propriété structurante). Reste, côté
> durcissement, la **convergence des deux chemins de validation**. Décisions d'Alexandre en attente :
> ratification formelle ADR-005 + ses sous-paramètres, ADR-003 (slashing), ADR-004 (aléa), garder-vs-
> retirer le CRDT, bump `TORUS_PROTOCOL_VERSION`. Commit baseline git = **manuel, Alexandre**.

## Gadget de finalité — conception gravée (style Casper FFG, PQ, par époque) (cadré, **non tranché**) (2026-06-23)

La suite naturelle d'ADR-005 : la **conception protocolaire** du gadget de finalité, raisonnée de
bout en bout **avant** tout code. Spec source : `QUANTA_FINALITY_GADGET_DESIGN.md`. Ce n'est **pas**
une tâche de code — c'est l'**orfèvrerie** (le protocole), à valider avant transcription.

**Choix d'architecture (d'Alexandre, je grave) :** ancrage **Casper FFG** (gadget de finalité
d'Ethereum, sûreté responsable démontrée), adapté à Quanta — **points de contrôle aux frontières
d'époque**, votes **ML-DSA** (socle [[ADR-005 — Agrégation des votes & certificats de finalité]]),
règle **justification/finalisation en deux temps** (⅔ de l'enjeu × 2 époques liées), **certificat
par époque** (≈165 Ko / 50 validateurs, élagable) derrière l'abstraction ADR-005, **fork-choice
conscient de la finalité**, et **deux conditions de slashing** (pas de double vote, pas de vote
enveloppant) d'où **découle** [[ADR-003 — Slashing (accountable safety)]].

### Ce qui a été fait (documentation seule)
- **Créé** `docs/DESIGN-FINALITY-GADGET.md` — record canonique des 14 sections en style vault
  (frontmatter `type: design` / `status: proposé (à valider — Alexandre)` ; breadcrumb umbrella +
  socle ADR-001→005 ; callouts `[!abstract]`/`[!success]` théorème de sûreté responsable /
  `[!warning]` honnêteté vivacité / `[!question]` §12 🛑 / `[!info]` cible d'acceptation harnais).
  Wikilinks réparés vers les **noms de fichiers ADR réels**.
- **Index** : `docs/decisions/README.md` (Cadre → pointeur vers le protocole détaillé sous le
  périmètre Option 1) ; `docs/00 — Pilotage QUANTA.md` (Carte → Consensus futur = umbrella +
  gadget) ; `docs/DESIGN-CONSENSUS-DAG-BFT.md` (Phase 1 Option 1 → pointeur conception détaillée +
  note que les 4 méta-décisions §7 sont depuis cadrées en ADR-001→005).
- **Spec source** `QUANTA_FINALITY_GADGET_DESIGN.md` conservée au root.

### Ancrage d'honnêteté radicale — la cible d'acceptation existe **déjà** dans le code
Le « test 2b » que la conception invoque est **réel** :
`t0_8_multiblock_partition_currently_diverges_gadget_deferred` (`src-tauri/src/sm/sim.rs:2932`).
Son docstring se nomme lui-même « **the acceptance target for the finality gadget** » et asserte
**aujourd'hui** la divergence de sûreté (trou ADR-001, marqué *gadget-deferred*, pas d'escalade car
attendu) ; quand le gadget arrive on **inverse** vers `tips[a] == tips[b]`. Le garde **§4 reste** :
une rupture **conservation/émission** au heal = **panic** (bug NEUF), jamais un *gadget-deferred*.
La conception est donc fidèle au harnais, pas une promesse en l'air.

### §4 — Renvoyé à Alexandre (non tranché ici)
La **règle d'arrêt §4** ([[QUANTA_AGENT_CONSTITUTION]]) nomme explicitement « **quel modèle de
finalité, faut-il du slashing et comment** » : je **cadre**, je ne tranche pas. Restent **🛑** :
**`E` longueur d'époque** (latence de finalité) ; **seuil de quorum** (⅔ à confirmer) ; **variante
exacte de fork-choice** conscient de la finalité (simple → LMD-GHOST si besoin) ; **montants +
fenêtre de slashing** des deux conditions ([[ADR-003 — Slashing (accountable safety)]]) ;
**pénalité d'inactivité** éventuelle (*inactivity leak*). Ces choix recoupent les sous-paramètres
ouverts d'ADR-002/003/004/005. **La validation de la conception elle-même** (« à valider ») est
aussi d'Alexandre.

### Auto-revue §3
- **Rien tranché en silence** : `status: proposé (à valider — Alexandre)` (pas `accepted`) ; le
  modèle Casper FFG est la **direction qu'Alexandre a écrite** dans la spec, marquée « à valider » ;
  aucun paramètre §12 inventé (tous listés 🛑).
- **Honnêteté §13 préservée** : §7/§8 explicitement des **esquisses, pas des preuves** ;
  formalisation + audit externe **devant** ; « architecture de départ, pas un théorème clos ».
- **Diff documentation seule** : 0 `.rs` modifié ; `dispatcher.rs`/ledger/`src/sm/` intacts ; pas de
  porte de tests Rust applicable (rien de compilé n'a changé). Citation `sim.rs:2932` **vérifiée**.

### Porte d'acceptation (conception = record + escalade, pas code)
Gadget de finalité **gravé canoniquement** (`DESIGN-FINALITY-GADGET.md`, 14 sections fidèles) ·
**3 index** pointent vers lui · cible d'acceptation harnais **citée et vérifiée** (`sim.rs:2932`) ·
paramètres §12 🛑 **renvoyés en §4** · honnêteté §13 **conservée** · spec source gardée · **aucune
valeur ni validation résolue par Claude**.

> **Débloque** l'implémentation par étapes du §14 (squelette d'époque + invariant de sûreté de
> finalité **d'abord**, chaque pièce falsifiée par le harnais DST avant la suivante) — **une fois
> la conception validée et les 🛑 du §12 tranchés par Alexandre**. Décisions d'Alexandre en
> attente : validation du modèle de finalité + ses paramètres §12, ratification formelle ADR-005,
> ADR-003 (slashing), ADR-004 (aléa). Commit baseline git = **manuel, Alexandre**.

## GADGET-1 — socle époque/point de contrôle + invariant de sûreté de finalité (avec dents) (2026-06-23)

Première **pièce de code** du gadget de finalité (`QUANTA_GADGET_PIECE1.md`, conception §2/§4/§11/§14
de [[DESIGN-FINALITY-GADGET]]). Le **bedrock** : découper la chaîne en époques, nommer les points de
contrôle, suivre l'ensemble finalisé (genèse seule), et poser l'invariant de **sûreté de finalité**
dans le harnais **avec ses dents**. **Rien ne finalise encore** (la règle justify/finalize est
GADGET-3) ; on pose la fondation et le crochet de vérification. Diff logique seule, déterministe.

### Ce qui a été fait
- **`src-tauri/src/sm/finality.rs`** (NEW, pur, sans-IO) :
  - `EPOCH_LENGTH_BLOCKS` — **placeholder marqué 🛑** (E = décision §12 d'Alexandre ; valeur
    provisoire 32 façon Casper, **non tranchée**). Toutes les fonctions prennent `epoch_len` en
    **paramètre** → squelette **paramétrique**, correct pour tout `E ≥ 1` ; fixer la vraie valeur =
    une ligne, **aucune logique à revoir**.
  - `Checkpoint { epoch, height, hash }` ; fonctions **pures** `epoch_of_height`, `is_epoch_boundary`,
    `checkpoint_at_epoch`, `checkpoints` (aucune horloge ni entropie ; `E=0` clampé → pas de
    div-by-zero, panic-free). 7 tests unitaires (division d'époque, frontières, genèse, tip, `E=0`).
  - `FinalizedSet` (clé = époque, `BTreeMap` déterministe) `genesis_only(hash)` = `{genèse}`, `get`,
    `iter`, `insert` (le **crochet** où GADGET-3 se branchera — *pas* de logique de finalisation ici).
- **`sm/node.rs`** : champ `finalized: FinalizedSet` init `{genèse}` dans les 3 constructeurs (hash
  du bloc 0 via `first()`, panic-free) ; accessor `finalized()` ; `record_finalized_for_test`
  (**`#[cfg(test)]`** — injection de test uniquement, hors chemin de finalisation réel).
- **`sm/sim.rs`** : `Violation::FinalitySafety { seed, epoch, node_a, hash_a, node_b, hash_b }`
  (miroir de `Safety`, `epoch` au lieu d'`index`) ; invariant câblé dans `check_invariants` (donc
  dans `run_checked`/sweep par défaut) : **jamais deux points de contrôle finalisés en conflit à la
  même époque**, itération à clés triées (nœuds `BTreeMap` × époques `BTreeMap`) → 1ʳᵉ violation
  reproductible.
- **`sm/mod.rs`** : `pub mod finality;` + re-export.

### Preuve que l'invariant a des dents (anti-vacuité §4 — crucial)
Comme **rien ne finalise** (genèse seule), l'invariant serait **vacueusement vrai** : c'est le piège
« test qui passe quoi qu'il arrive ». Donc **violation plantée** :
`gadget_1_finality_safety_invariant_has_teeth` injecte dans deux nœuds **deux checkpoints finalisés
en conflit à l'époque 1** (hash différents, hors de tout chemin réel) et asserte que
`check_invariants` **mord** en `FinalitySafety{epoch=1, seed}`. Doublé d'un test **anti-faux-positif**
`gadget_1_finality_safety_accepts_matching_checkpoints` (mêmes checkpoints → reste **vert**) qui
prouve que l'invariant fire sur le **conflit**, pas sur la simple présence. Le vérificateur mord
**dès maintenant**, avant que la finalisation existe.

### Portes d'acceptation — toutes vertes
- `cargo test --lib` : **285 / 0** (dont +9 GADGET-1 : 7 unitaires finality + dents + anti-faux-pos).
- `cargo clippy --lib -- -D warnings` **propre** ; **bare `cargo clippy -- -D warnings` propre** aussi.
- **C1 vert** (`determinism_meta_test_128_runs_are_byte_identical`) — `fingerprint` ne lit pas
  `finalized`, init déterministe, rien ne change dans `handle` ⇒ trace inchangée.
- **Sweep par défaut vert** (`t0_8_clean_default_sweep` + 12 `t0_8`) : l'invariant tourne à chaque
  pas ; genèse seule finalisée partout ⇒ accord d'époque 0 ⇒ jamais déclenché en sweep honnête.
- `sm/` **sans-IO** préservé (finality.rs pur ; init `finalized` déterministe) ; `src/sm/` C1 intact.
- **Diff logique seule** : 4 fichiers (finality.rs NEW + node/sim/mod), siblings clock/effect/event/rng
  **intacts**, `dispatcher.rs` **intact**, aucun reformatage.

### §4 — décision ouverte signalée (non tranchée)
**`E` (longueur d'époque) = 🛑 décision d'Alexandre.** Le squelette est **paramétrique** ; la
constante `EPOCH_LENGTH_BLOCKS` est un **placeholder marqué** (32 provisoire, façon Casper), pas une
valeur arrêtée — gravée comme telle dans la docstring + ici. Rien d'autre du §12 n'est figé.

### Auto-revue §3
- **Pas de masquage, pas de test vacueux** : l'invariant est **prouvé mordant** (violation plantée)
  AVANT toute finalisation — exactement l'exigence §4 du spec. L'anti-faux-positif le borne par le
  haut (accord ≠ violation).
- **Honnête sur le périmètre** : **rien ne finalise** (pas de règle justify/finalize — c'est
  GADGET-3) ; `record_finalized_for_test` est `#[cfg(test)]` pour ne pas exposer une finalisation
  sans règle sur la surface prod. `E` non inventé en dur (placeholder marqué, §4).
- **Note clippy honnête (hors périmètre)** : `cargo clippy --all-targets` (code de test seul)
  signale un `result_large_err` **pré-existant** sur `run_checked_steps` — la taille de `Violation`
  est **inchangée** (fixée par le variant `Safety` pré-existant, 2×`String`+2×`PeerId` = 112 o, que
  mon `FinalitySafety` ne fait qu'**égaler** après trim des `height_a/height_b` redondants). Non
  introduit par GADGET-1, hors de la porte du spec (`--lib`) ; **pas de `#[allow]` posé** (ce serait
  masquer un lint pré-existant hors scope). Signalé, pas dissimulé.
- **Déterminisme** : itération à clés triées partout ; `BTreeMap` pour `finalized` ; aucune lecture
  d'horloge/entropie. C1 + sweep le confirment.

> **Suite** (en attente de la validation de la conception + de `E` par Alexandre) : **GADGET-2**
> (votes ML-DSA + certificat d'époque derrière l'abstraction ADR-005), puis **GADGET-3** (la règle
> justify/finalize en deux temps — là où la finalité devient réelle et où l'invariant **cesse d'être
> vacueux**), puis le fork-choice conscient de la finalité et la **bascule du test 2b**
> (`t0_8_multiblock_partition_currently_diverges_gadget_deferred`, intact aujourd'hui). Commit
> baseline git = **manuel, Alexandre**.

## STAKE-WEIGHT-1 — poids du comité = enjeu on-chain seul (implémente ADR-002) (2026-06-23)

Audit-d'abord (`QUANTA_STAKE_WEIGHT.md`), prérequis de soundness pour GADGET-2 (le certificat
d'époque mesure ⅔ de l'**enjeu** — n'a de sens que si le poids EST l'enjeu seul). ADR-002 (accepté)
a **déjà tranché** : poids consensus = **enjeu on-chain seul**, réputation hors du chemin de sécurité.

### §1 — Constat d'audit (le poids était **teinté de réputation**)
- `Validator::weight()` (`pos_consensus.rs`) calculait `stake + min(reputation × 10_000, stake)` — la
  **réputation entrait dans le poids** (jusqu'à **doubler** le poids d'élection). C'est le bonus
  plafonné de l'audit 3.4, désormais **superséé par ADR-002**.
- Consommateurs : `elect_leader` + `elect_fallback_leader` (somme cumulée de `weight()`) → **toute
  l'élection** était réputation-pondérée. `is_valid_proposer` passe par l'élection.
- **La faille de soundness** : la réputation est **mesurée localement** (`mining_loop.rs:212`
  construit le set depuis `leaderboard` local — `trust_score` par nœud). Deux nœuds honnêtes à
  réputations locales différentes ⇒ poids différents ⇒ **leaders différents au même slot ⇒ fork**
  (exactement « le vrai trou » d'ADR-002).
- **Unique consommateur** du champ `Validator.reputation` = `weight()` (aucune autre lecture
  `validator.reputation` dans le code). Shapley n'est **pas** dans le chemin d'élection (distribution
  d'émission seulement) — confirmé.

### §2b — Conversion en enjeu-seul (action)
- **`Validator::weight()` → `self.stake`** (enjeu on-chain SEUL), conforme au 1ᵉʳ bullet d'ADR-002
  (« retirer le bonus réputation `min(rep×10_000, stake)` »). Détache la réputation de **tout** le
  chemin (élection + futur quorum) en **un seul point** (le chokepoint `weight()`).
- **Champ `reputation` conservé** (le spec le permet : « ne la supprime pas forcément ») mais
  re-documenté **signal applicatif à effet consensus NUL**, avec ⚠️ « ne JAMAIS réintroduire un
  terme non-enjeu ici » sur `weight()`. Diff minimal : aucun des 30 sites `Validator{…}` (tous
  `reputation: 0`) ni `reputation.rs` ni le harnais ne sont touchés.
- **Docs honnêteté** alignées sur le code : module-doc `pos_consensus.rs`, **CLAUDE.md** (`Poids =
  stake`), **WHITEPAPER.md** + **WHITEPAPER_FR.md** (formule `stake + reputation·10_000` retirée).

### §3 — Propriété anti-divergence (prouvée, avec dents)
- `weight_is_stake_only_reputation_has_no_effect` : `weight() == stake` quelle que soit la réputation.
- `weight_and_election_are_identical_across_nodes_despite_local_reputation` (**la propriété ADR-002**) :
  deux nœuds, **mêmes stakes** mais **réputations locales divergentes** → (a) poids identiques par
  validateur, (b) **même leader élu sur 2 000 slots**. **Dents/anti-vacuité** : le test calcule en
  ligne l'ancienne formule `stake + min(rep×10_000, stake)` et **asserte qu'elle AURAIT divergé** sur
  ces entrées — donc l'accord prouvé n'est pas dû à des entrées identiques, mais à la suppression du
  terme réputation. Test unitaire déterministe (pas besoin du harnais : le poids/élection se testent
  directement, plus net).

### §4 — Reste d'ADR-002 = travail dérivé plus grand, sous-décisions OUVERTES (non tranché ici)
ADR-002 liste d'autres dérivés **au-delà** de « poids = enjeu » : **snapshot de stake on-chain par
epoch** (remplace `build_validator_set` qui source aujourd'hui le stake depuis le **leaderboard
local**), **tx `Stake`/`Unstake`** + comptabilité du stake au ledger, quorum 2f+1. Ceux-ci ont des
**sous-décisions ouvertes** (longueur d'epoch = §12, format des tx, délai d'unbonding) → **signalés,
pas tranchés** (règle §4). **Honnêteté** : cette tâche retire le **terme réputation** du poids (la
part décidée et indépendante du §12) ; la détermination pleine inter-nœuds exige encore que la
**source du stake** devienne un objet on-chain (l'epoch snapshot) — c'est le prochain dérivé, pas
celui-ci. La propriété §3 prouve précisément ce qui est clos : *à stakes donnés, la réputation ne
fait plus diverger l'élection*.

### Portes d'acceptation — toutes vertes
- `cargo test --lib` : **286 / 0** (16 pos_consensus dont le test de poids déterministe §3 ; le test
  3.4 `reputation_bonus_capped_at_stake` **réécrit** en `weight_is_stake_only_…` ; overflow whale
  **rebasé** sur des stakes qui somment > u64::MAX en poids-stake-seul).
- `cargo clippy --lib -- -D warnings` **propre** + **bare `cargo clippy` propre**.
- **C1 vert** + **sweep par défaut vert** (aucun changement de comportement du harnais : tous ses
  Validators étaient déjà `reputation: 0` ⇒ `weight()=stake` y tenait déjà).
- **Diff logique seule** : **1 fichier code** (`pos_consensus.rs`) + 3 docs d'honnêteté (CLAUDE/WP) ;
  `reputation.rs`/`sm/`/`dispatcher.rs` **intacts** ; aucun reformatage.

### Auto-revue §3
- **Pas de masquage** : c'était une **vraie** faille de soundness (poids réputation-pondéré, mesuré
  localement) — corrigée à la racine (`weight()`), pas neutralisée par un test mou ; le test §3 a des
  **dents** (prouve que l'ancien chemin divergeait).
- **Pas de sur-portée** : je n'ai tranché ni taille de comité ni quorum ni epoch length (§12 /
  GADGET-2 = Alexandre) ; le principe (enjeu seul) était **déjà décidé** par ADR-002, pas rediscuté.
- **Honnête sur le résiduel** : la source du stake reste locale jusqu'au snapshot on-chain par epoch
  (dérivé ADR-002 suivant, sous-décisions ouvertes) — explicitement signalé, pas dissimulé.

> **Débloque GADGET-2** sur une base saine (le poids est l'enjeu). GADGET-2 (votes ML-DSA + certificat
> d'époque) reste **en attente de la validation de la conception + des §12 d'Alexandre** (E, taille de
> comité, quorum). Prochain dérivé ADR-002 (indépendant de ce spec) : **snapshot de stake on-chain par
> epoch** + tx `Stake`/`Unstake` (sous-décisions ouvertes). Commit baseline git = **manuel, Alexandre**.

---

## ONCHAIN-STAKE-1 — sourcer l'enjeu depuis la chaîne (ferme la 2ᵉ moitié du vecteur de fork) (2026-06-23)

Suite directe de STAKE-WEIGHT-1 §4 (`QUANTA_ONCHAIN_STAKE.md`). STAKE-WEIGHT-1 a retiré la
**réputation** du poids ; mais l'enjeu lui-même restait **lu localement** (`build_validator_set`
sourçait le stake depuis le **leaderboard** local). Deux nœuds pouvaient donc **encore** diverger,
non plus par la réputation mais par l'enjeu. Ce spec ferme la **seconde moitié** : l'enjeu devient
un **état du ledger**, identique sur tous les nœuds.

### §1 — État d'enjeu dans le ledger (block-index-anchored)
- Nouveaux états ledger : `staked: HashMap<pk,u64>` (enjeu **bondé** = poids consensus) et
  `unbonding: HashMap<pk, Vec<UnbondEntry{amount, unlock_height, tx_hash}>>`.
- Le solde se scinde en **dépensable** / **staké** / **en-déverrouillage**. Le poids bondé est
  **dérivé de la chaîne**, ancré à `block.index` (commit au scellage via `apply_block_stake_effects`),
  donc une **fonction pure de la chaîne** : un nœud live, restauré (`rebuild_cache`) ou synchronisé
  calcule des maps **byte-identiques**. `UNBONDING_PERIOD_BLOCKS` = **10 080** (🛑, ~2 sem. de blocs,
  contrainte gravée `≥ fenêtre de slashing` ADR-003) ; `MIN_VALIDATOR_STAKE` reste 🛑 (§12).

### §2/§3 — tx Stake / Unstake (pas de nouveau type gossip, `dispatcher.rs` intact)
- `Stake`/`Unstake` sont des `Transaction` signées (`from=pk`, `to="STAKE"`), propagées par le
  `BroadcastTx` **existant** + admises par `apply_remote_tx_checked` existant → **zéro** changement
  de `dispatcher.rs` / wire. `verify_tx` n'exempte pas `STAKE` (≠ `NETWORK`/`ESCROW`) → on ne peut
  pas forger l'enjeu d'autrui.
- **Unstake** : `staké → en-déverrouillage`, `unlock = block.index + UNBONDING_PERIOD_BLOCKS`
  (indexé **par hauteur**, jamais l'horloge). Maturation = retour `STAKE→dépensable` à la hauteur
  d'unlock.

### §4 — Rewire `build_validator_set` (le cœur du fix)
- `mining_loop.rs` source désormais l'enjeu depuis **`ledger.validator_stakes()`** (état on-chain),
  **plus** le leaderboard ; `reputations` = map vide (réputation hors chemin, ADR-002). Le poids
  d'un validateur est une **fonction pure de la chaîne**, identique partout.

### 🛑 Revue adversariale → 1 **CRITIQUE** trouvé et corrigé (HARDEN-STAKE-1)
Une revue multi-agents (6 dimensions × find→verify) a trouvé une **vraie faille de conservation**
dans ma 1ʳᵉ implémentation : un `Stake` **en attente ne verrouillait pas** les fonds (effet appliqué
au scellage seulement). Course honnête : `stake_tx(100)` puis `transfer(50)` des mêmes pièces →
`balance_of` ne baissait pas → au scellage le cache passait **négatif**, le clamp `.max(0)` le
masquait, et `Σ dépensable + enjeu + brûlé` **dépassait `miné`** (µQTA fabriqués). Variante pire :
un `Stake` **forgé** `amount ≫ solde` fabriquait du **poids consensus** depuis rien.
- **Fix (HARDEN-STAKE-1)** : un `Stake` est désormais un mouvement **neutre en solde** `pk → puits
  "STAKE"`, appliqué au **mempool** (`cache_apply_tx`, chemin générique ; `STAKE` n'est PAS synthétique
  → le puits est crédité, rien n'est détruit). Les fonds **se verrouillent à l'admission** → la course
  transfert est **rejetée**, le cache ne passe jamais négatif, conservation exacte à chaque pas. Le
  **poids bondé** (map `staked`, consommée par `validator_stakes`) est committé **séparément au bloc**
  → un Stake en attente verrouille mais ne pèse qu'une fois scellé (donc **pas de fork** depuis le
  mempool : le poids reste chaîne-seul).
- **Conservation (harnais)** = `Σ all_balances(dépensable, hors STAKE) + locked_stake_total
  [= solde du puits STAKE] + brûlé == miné`. `all_balances` exclut `STAKE`.
- **Re-vérif adversariale du fix** (4 contrôles ciblés) : double-dépense **CLOSED**, déterminisme
  **vérifié** (poids chaîne-seul, `rebuild` byte-identique), intégrité du puits en **reorg** saine,
  **aucune** nouvelle rupture de conservation.

### §7/§5 — propriétés prouvées (avec dents)
- `onchain_stake_weight_identical_across_nodes_despite_local_leaderboards` (§7) : deux nœuds **même
  chaîne** → **mêmes poids** + **même leader sur 2 000 slots**, malgré des leaderboards locaux
  divergents. **Dents** : l'ancienne source (leaderboard local) AURAIT divergé sur ces entrées.
- `onchain_stake_conservation_through_stake_unstake_unlock` (§5) : Stake → Unstake → déblocage
  **conserve à chaque pas** ; fonds dépensables **seulement** à la hauteur d'unlock (verrou testé à
  `unlock-1`, libéré à `unlock`).
- `harden_stake_pending_stake_locks_funds_no_double_spend` (**régression du CRITIQUE**) : `stake_tx`
  verrouille → transfert concurrent **rejeté** → cache **jamais négatif**, conservation tenue.
- `onchain_stake_state_survives_snapshot_restore_byte_identical` : live ≡ restauré (puits + maps).
- `onchain_stake_harness_conservation_counts_locked_stake` : le vérificateur du harnais compte le
  puits (dents : l'ancienne formule `dépensable + brûlé` le **rejetait** faussement).

### §4 (Constitution) — signalé, **non tranché**
- **Validation de couverture au bloc** : un `Stake` (ou `Transfer`) **forgé** `amount > solde`
  (contournant le builder) fait passer le cache négatif et — pour un Stake — **fabrique du poids**.
  C'est la **même** lacune pré-existante de **non-validation des soldes au scellage** qui touche
  **aussi les transferts** (`verify_tx` ne vérifie que les signatures). Fabrication **déterministe**
  (identique sur tous les nœuds → sybil/économique, **pas un fork**). Régler ça (couverture au bloc,
  solde chaîne-seul, application leader-only à l'intégration) est un **choix de conception consensus**
  (lié à ADR-003) → **signalé, pas tranché**. Le HONNÊTE est posé : ONCHAIN-STAKE-1 ne **dégrade pas**
  la conservation vs le comportement transfert pré-existant ; la course **honnête** est, elle, **close**.
- Reorg **mono-bloc** (≤1 de profondeur) et `UNBONDING_PERIOD_BLOCKS ≫ 1` ⇒ aucun reorg ne peut
  enjamber une maturation (la maturation n'est volontairement **pas** annulée au pop).

### Portes d'acceptation — toutes vertes
- `cargo test --lib` : **291 / 0** (incl. §5, §7, régression CRITIQUE, live≡restauré, harnais).
- `cargo clippy --lib -- -D warnings` **propre** + **bare clippy 0 warning**.
- **C1 vert** + **sweep par défaut vert** (aucune tx d'enjeu dans le sweep ⇒ `locked_stake_total=0`
  ⇒ conservation inchangée pour les scénarios existants).
- **Diff logique seule** : **3 fichiers code** (`ledger.rs`, `mining_loop.rs`, `sm/sim.rs`) + docs
  (CLAUDE.md, WHITEPAPER×2 déjà à « on-chain stake ») ; `dispatcher.rs`/`reputation.rs`/`pos_consensus.rs`/
  `sm/node.rs` **intacts**.

> **Vecteur de fork entièrement fermé** : réputation hors du poids (STAKE-WEIGHT-1) **et** enjeu sourcé
> de la chaîne (ici). Le poids consensus est une **fonction pure de l'état**, identique partout — le
> socle dont GADGET-2 a besoin pour mesurer ⅔ de l'enjeu. **Reste à trancher (Alexandre)** : validation
> de couverture au bloc (couvre transferts + stakes ; lié ADR-003), valeurs 🛑 `UNBONDING_PERIOD_BLOCKS`
> / `MIN_VALIDATOR_STAKE` (§12), validation conception finalité + §12 (GADGET-2). Commit baseline git =
> **manuel, Alexandre**.

---

## COVER-1 — Validation de couverture au bloc (on ne dépense pas ce qu'on n'a pas)

> Résout l'item **« Validation de couverture au bloc »** que ONCHAIN-STAKE-1 §4 avait **signalé,
> non tranché**. Alexandre a tranché le **principe** (pas un choix §4) : tout bloc rejette une
> dépense ou un stake **non couvert** par le solde **on-chain**. Indépendant d'ADR-003/slashing
> (règle fondamentale, pas une pénalité). **Dernier trou de validation** avant le gadget.

### §1 — Où : la validation PARTAGÉE (leçon FORK-CAP)
- Le contrôle vit dans **`validate_block_against_prev`** — l'unique validateur que **les deux**
  chemins traversent : intégration **linéaire** (`validate_remote_block` → l. 1249) **et**
  **fork-reorg** (l. 1532). Un seul contrôle, sur le chemin partagé ; aucun chemin ne peut le sauter
  (exactement comme FORK-CAP a paramétré `validate_block_emission_against` pour les deux chemins).
- Nouveau pur **`onchain_spendable_before(prev)`** : solde dépensable **avant le bloc**, **fonction
  pure de la chaîne** jusqu'à `prev`, **jamais le mempool local** (sinon verdict divergent). Miroir
  exact de la sémantique du ledger — mouvements génériques (`cache_apply_tx`), le verrou `pk→STAKE`
  d'un Stake, l'absence d'effet dépensable d'un Unstake, et la **maturation** indexée par hauteur
  (`mature_unbonding`). Garde anti-dérive `#[cfg(test)]` : le replay == cache live (chaîne sans pending).
- Reorg : la couverture du **gagnant** est jugée contre la chaîne **SANS le tip** qu'il remplace
  (`prev_for_remote = chain[tip.index-1]`) — le « avant le bloc » correct pour un bloc de remplacement.

### §2/§3 — Le contrôle : couverture SÉQUENTIELLE + crédits intra-bloc
- Rejoue les tx du bloc **dans l'ordre** sur un solde courant initialisé depuis `onchain_before`.
  Chaque tx à expéditeur **réel** (Transfer **ou** Stake) exige `solde_courant ≥ montant` ; sinon
  **bloc INVALIDE**. L'ordre est déterministe ⇒ tous les nœuds tranchent identiquement.
- **Expéditeurs synthétiques exemptés** (`NETWORK`/`ESCROW` mintent ; `BURN` n'est qu'une
  destination) — régis par les règles d'émission, pas par un solde.
- **Crédits intra-bloc comptent (§3)** : une récompense `NETWORK→mineur` créditée plus tôt dans le
  **même** bloc finance une dépense ultérieure (testé : Alice 0 reçoit 5, en dépense 3, **accepté**).
- **Unstake** : aucun débit dépensable (reclasse des coins déjà bondés) ⇒ rien à couvrir ici ; le
  sur-déliement reste borné par le `saturating_sub` du poids bondé à l'application du bloc.

### §4 — Sort du clamp `.max(0)` : **CONSERVÉ** (branche « ne force pas »)
- **Investigué, non retiré** — le retrait a de **vrais effets de bord** (le spec anticipe : « si le
  retrait a d'autres effets, signale-le, ne force pas ») :
  1. Le cache est **pending-inclus**. L'admission d'une tx **distante** (`replay_remote_tx`) **n'a
     aucun garde de couverture** — et lui en ajouter un risquerait de rejeter une arrivée
     **hors-ordre** dont la tx de financement n'a pas encore atterri, cassant la convergence
     **AUDIT-TX-2** (raison pour laquelle §5 garde la couverture mempool **optionnelle**). Une tx
     **pending** non couverte peut donc encore rendre une entrée du cache transitoirement négative.
  2. `i128 as u64` d'un négatif **enroule** vers un solde colossal fabriqué — **pire** que clamper.
- COVER-1 ferme le non-couvert **au bloc** (état scellé jamais négatif) ; le clamp ne garde plus que
  le **transitoire mempool** + la sûreté de cast. Un vrai négatif **on-chain** (un bug) reste
  **bruyant** : il n'atteint jamais le cache (rejeté à la validation), et tout résidu **gonfle** la
  somme de conservation (`Σ ≠ minted`) au lieu d'être caché. Commentaire posé sur `balance_of`.

### §6 — Tests adverses (tous verts)
- `cover1_uncovered_transfer_block_rejected` : transfert `50 > 10` on-chain ⇒ **bloc rejeté**.
- `cover1_uncovered_stake_block_rejected` : Stake `50 > 10` ⇒ **rejeté** (aucun poids forgé).
- `cover1_sequential_coverage_within_block` : `[→Bob 50, →Carol 60]` **rejeté** (2ᵉ jambe non
  couverte) ; `[→Bob 50, →Carol 40]` **accepté** — preuve que c'est la **déplétion séquentielle**,
  pas la présence de deux tx.
- `cover1_both_paths_reject_uncovered` (**les deux chemins**) : **la même** tx non couverte est
  rejetée en **linéaire** (height +1) **ET** en **fork-reorg** (gagnant à hash supérieur, jugé sans le
  tip) ; chaîne **non tronquée** (AUDIT-BLK-2 préservé), conservation intacte.
- `cover1_valid_block_with_intra_block_credit_passes` : bloc **couvert** (récompense finance la
  dépense même-bloc, §3) **accepté** — pas de régression de vivacité.
- `cover1_rejected_uncovered_preserves_conservation` : après rejet, `Σ dépensable+locked+brûlé ==
  minted`, solde **intact**, **aucune** entrée de cache négative — classe « non-couvert casse/masque
  la conservation » **fermée à la validation**.
- `cover1_onchain_replay_matches_live_cache_no_drift` : `onchain_spendable_before` == cache live.

### Portes d'acceptation — toutes vertes
- `cargo test --lib` : **298 / 0** (291 + 7 COVER-1).
- `cargo clippy --lib -- -D warnings` **propre**. `--lib --tests` : seule subsiste la lacune
  **pré-existante** `result_large_err` de `sm/sim.rs:601` (`run_checked_steps`, enum `Violation` —
  GADGET-1, **pas** COVER-1) ; suivant le précédent GADGET-1, **pas de `#[allow]`** (no-masquage).
- **C1 vert** (`determinism_meta_test_128_runs_are_byte_identical`) + **sweep par défaut vert** +
  conservation verte (le sweep honnête ne produit aucune dépense non couverte ⇒ contrôle transparent).
- **Diff logique seule** : **1 fichier code** (`ledger.rs`) + ce tracker + CLAUDE.md ;
  `dispatcher.rs`/`pos_consensus.rs`/`sm/*`/`mining_loop.rs`/`reputation.rs` **intacts** (0 réf COVER-1
  hors `ledger.rs`). `src/sm/` **sans-IO** préservé.

> **Dernier trou de validation fermé** : aucune dépense ni aucun stake non couvert n'est plus
> **finalisé**, sur les **deux** chemins. La couverture est une **fonction pure de la chaîne** ⇒ verdict
> identique partout (pas un nouveau vecteur de fork). **Reste à trancher (Alexandre)** : valeurs 🛑
> `UNBONDING_PERIOD_BLOCKS` / `MIN_VALIDATOR_STAKE` (§12), GADGET-2 (conception finalité + §12 : E,
> comité, quorum). Commit baseline git = **manuel, Alexandre**.

### Auto-revue — vérification adverse (5 lentilles indépendantes, refute-by-default)
- **false-accept (inflation)** — *sound* : aucun sur-paiement réel ne passe (i128 signé, check avant
  mise à jour du running, recalcul chaîne-seul, Stake couvert comme transfert, synthétiques exemptés).
- **déterminisme** — *sound* : verdict = fonction pure de la chaîne ; aucune dépendance à l'ordre
  d'itération HashMap (maturation collectée en Vec, soldes additifs), au mempool, ni à l'horloge. C1 vert.
- **drift** — *sound* : `onchain_spendable_before` == cache live (test no-drift) ; tous types de tx,
  puits STAKE, maturation byte-identiques à `cache_apply_tx`+`mature_unbonding`.
- **false-reject « frontière de maturation »** — **RÉFUTÉ** (lentille trop zélée). La maturation
  `mature_unbonding(block.index)` s'exécute **après** les tx du bloc (`apply_block_stake_effects` est
  appelé après `cache_apply` ; confirmé `seal_block_at` l. 1131-1134 et integrate). Donc la maturation
  de hauteur N **n'est pas** disponible aux tx de N — **ni** en couverture (mature jusqu'à `prev.index`)
  **ni** à l'application. Les deux sont **cohérents**. Pas de divergence : tout nœud *intégrant* calcule
  `onchain_spendable_before` à l'identique et rejette à l'identique (déterministe). Le flux **honnête**
  ne produit jamais le cas : le builder `transfer_with_burn` vérifie `balance_of` à **l'admission**, et
  le crédit de maturation n'entre dans le cache qu'au **scellage** de N → l'utilisateur ne voit ses
  fonds mûris qu'**après** N → sa dépense atterrit en N+1+, où la couverture les inclut correctement.
  Rejeter une dépense de fonds mûrissant dans **le même** bloc est conforme au spec (« solde *avant* le
  bloc » = jusqu'au parent) et à l'intention de tuer le masquage par négatif transitoire.
- **complétude des chemins de finalisation** — **SIGNALÉ (réel, hors-périmètre §1/§5)** : `seal_block_at`
  **ne** passe **pas** par `validate_block_against_prev`. Scénario : un pair malveillant gossipe une tx
  non couverte → `replay_remote_tx` l'admet au mempool **sans garde de couverture** (volontaire, pour les
  arrivées hors-ordre — **§5 laisse cette couche optionnelle**, car la garder risquerait la convergence
  AUDIT-TX-2) → si **ce** nœud scelle, le bloc entre dans **sa** chaîne sans contrôle. **Mais** : (a) tout
  nœud *intégrant* le **rejette** (le contrôle autoritaire d'intégration fait son travail) ⇒ **jamais
  accepté au niveau réseau**, zéro inflation ; (b) c'est **strictement plus sûr** que l'avant-COVER-1, où
  la **même** tx était **acceptée partout** (inflation silencieuse réseau) ; (c) borné (NET-14 TTL mempool)
  et auto-réparé ~50 % au prochain reorg (départage par hash). Le **but** du spec — « aucune dépense non
  couverte **acceptée sur les deux chemins** [d'intégration] » — est **tenu**. La racine (admission
  `replay_remote_tx` + scellage local sans contrôle) est une **sous-question de conception consensus**
  délibérément laissée ouverte (§5 optionnel) — **même item** que le « contrôle de couverture leader-only
  au scellage / admission » déjà signalé. Constitution §4 : **signalé, non tranché** (options : garde au
  scellage en excluant les tx non couvertes ; ou garde d'admission en acceptant le risque hors-ordre ;
  ou statu quo). → **décision Alexandre**, liée à ADR-003 / GADGET.

> **Bilan adverse** : le **livrable du spec** (couverture sur le validateur partagé des deux chemins
> d'intégration) est **sound** sur les 3 lentilles de correction + la frontière de maturation (réfutée).
> La seule réserve réelle (`seal_block_at`/admission) est **hors du périmètre explicite §1/§5**,
> **safety-positive** vs l'existant, et **signalée** pour décision.

---

## COVER-2 — Couverture au seal (construire un bloc valide, pas en rejeter un)

> Résout l'item **`seal_block_at` bypasse la couverture** que la revue adverse de COVER-1 a
> signalé (auto-corruption locale). Alexandre a tranché le **mécanisme** : au seal on n'**rejette**
> pas (on fabrique le bloc), on **exclut** les tx non couvertes ⇒ bloc **valide par construction**.
> Symétrie : **rejet** à la réception (COVER-1), **exclusion** à la production (COVER-2).

### §1 — Une seule source de vérité (réutilisation, pas duplication)
- Extraction de la règle de couverture séquentielle dans **`uncovered_tx_indices(onchain_before,
  txs) -> Vec<usize>`** — l'unique implémentation. Deux consommateurs : `validate_block_against_prev`
  (COVER-1) **rejette** le bloc si non-vide ; `seal_block_at` (COVER-2) **exclut** exactement ces
  index. Rejet et exclusion **ne peuvent plus diverger** (sinon un bloc auto-scellé serait rejeté par
  les pairs — l'auto-corruption). `onchain_spendable_before` (COVER-1) réutilisé tel quel.

### §2 — Exclusion séquentielle au seal
- `seal_block_at` : solde **avant le bloc** = `onchain_spendable_before(prev)` (chaîne-seule,
  déterministe). Sur les candidats (post-coalesce des récompenses), `uncovered_tx_indices` marque les
  tx non couvertes (expéditeur réel, solde courant insuffisant) ; **crédits intra-bloc comptés** ;
  synthétiques + `Unstake` exemptés. Une tx non couverte est une tx d'un expéditeur réel dont la tx
  effet a **déjà** été appliquée au cache à l'admission (`replay_remote_tx`/builder) ⇒ on
  **`cache_revert_tx`** pour garder `cache == chaîne+pending`, puis on **drop**. Le bloc scellé ne
  contient **que** des tx couvertes.

### §3 — L'invariant qui ferme le trou (testé directement)
- **Tout bloc produit par `seal_block_at` passe `validate_block_against_prev`.** Prouvé par
  `cover2_self_sealed_block_always_validates` (mempool couvert / entièrement non couvert → bloc vide
  valide / mixte) et par chaque test COVER-2 (helper `seal_and_validate`). Un nœud ne peut donc plus
  **ni propager ni sceller** une dépense non couverte ⇒ **auto-corruption locale fermée**.

### §4 — Éviction des tx exclues (signalé)
- **Défaut appliqué** : la tx exclue est **drop** du mempool + son effet cache **reverté** ; elle
  **reste dans `seen_tx_hashes`** (donc non ré-admise par `replay_remote_tx`). **Sous-question §4
  signalée** : une tx *honnête hors-ordre* (financement pas encore arrivé) exclue au seal n'est **pas
  perdue au niveau réseau** — quand son financement est scellé ailleurs, ce nœud la ré-apprend via la
  **synchro de blocs** (pas le mempool), et le bloc qui la porte passe alors COVER-1. Compromis :
  ce nœud ne la **rescellera** pas lui-même (elle est dans `seen`). Alternative (la retirer de `seen`
  pour ré-admission ultérieure) = plus de complexité + risque de boucle ; **défaut raisonnable =
  exclure + évincer**, conforme au spec §4. → choix ouvert pour Alexandre s'il veut la politique de
  rétention.

### §5 — Clamp `.max(0)` : inchangé (comme demandé)
- COVER-2 ne touche **ni** le clamp **ni** l'admission (`replay_remote_tx` garde sa concession
  hors-ordre AUDIT-TX-2). Le clamp reste load-bearing pour le cache pending-inclus + la sûreté du
  cast — exactement la position COVER-1 §4.

### §6 — Tests (tous verts)
- `cover2_uncovered_transfer_excluded_at_seal` : tx non couverte **absente** du bloc, bloc **valide**,
  conservation **restaurée** (phantom d'admission annulé par le revert).
- `cover2_uncovered_stake_excluded_at_seal` : idem Stake (aucun poids bondé).
- `cover2_sequential_exclusion_at_seal` : 60 couvert scellé, 60 devenu non couvert exclu.
- `cover2_auto_corruption_scenario_closed` : admet couvert + non couvert via `replay_remote_tx`,
  scelle ⇒ bloc valide (non couvert exclu) **et un pair (node_b) l'intègre `Ok(true)`** + convergence.
- `cover2_self_sealed_block_always_validates` (**INVARIANT §3**) : bloc auto-scellé toujours valide.
- `cover2_covered_txs_sealed_normally_no_regression` : transfert couvert (+ burn) scellé intact.

### Portes d'acceptation — toutes vertes
- `cargo test --lib` : **306 / 0** (298 + 8 COVER-2), incl. l'invariant §3.
- `cargo clippy --lib -- -D warnings` **propre**. **C1 vert** + **sweep / reproductibilité / sweep
  par défaut verts** (l'exclusion ne perturbe pas la convergence honnête : le sweep ne produit aucune
  tx non couverte ⇒ aucune exclusion ⇒ comportement identique).
- **Diff logique seule** : **1 fichier code** (`ledger.rs`) + tracker + CLAUDE.md ; `dispatcher.rs`
  (0 réf COVER-2), `pos_consensus.rs`, `sm/*`, `mining_loop.rs`, `reputation.rs` **intacts** ;
  `src/sm/` **sans-IO** préservé.

> **Couverture symétrique complète** : rejet à la réception (COVER-1, deux chemins) + exclusion à la
> production (COVER-2). Aucune dépense ni aucun stake non couvert n'est plus **ni propagé ni scellé**.
> **Dernier durcissement de validation avant le gadget.** Reste, indélégable (Alexandre) : validation
> de la conception du gadget + décisions §12 (E, taille de comité, quorum) ; valeurs 🛑
> `UNBONDING_PERIOD_BLOCKS` / `MIN_VALIDATOR_STAKE`. Commit baseline git = **manuel, Alexandre**.

### Auto-revue — vérification adverse COVER-2 (4 lentilles)
- **cache-revert / conservation** — *sound* : `cache_revert_tx` est l'inverse arithmétique exact de
  `cache_apply_tx` (cache seul ; minted/burned inchangés) ⇒ conservation restaurée ; tous types,
  puits STAKE, multi-exclusions indépendantes de l'ordre. Vérifié par les tests COVER-2 + conservation.
- **invariant §3** — *sound* : seal et validate partagent `uncovered_tx_indices` depuis le **même**
  `onchain_before` ; la couverture séquentielle est **monotone** sous exclusion (une tx exclue
  n'apporte pas son crédit, donc une tx gardée couverte le reste) ⇒ un bloc auto-scellé passe
  **toujours** la validation. EMIT-1 préservé (les récompenses `NETWORK` synthétiques ne sont jamais
  exclues).
- **déterminisme + source unique** — *sound* : exclusion = `Vec<usize>`→`HashSet` (test d'appartenance)
  puis itération **linéaire** du `Vec` candidat ⇒ aucune fuite d'ordre HashMap dans le bloc. Le
  refactor COVER-1 (boucle inline → `uncovered_tx_indices` + `.first()`) préserve le verdict de rejet
  **exactement**. C1 vert.
- **éviction / vivacité** — **RÉFUTÉ avec test** (lentille HIGH erronée). Allégation : une tx
  **couverte hors-ordre** exclue au seal serait **perdue** si le bloc excluant gagne le fork. **Faux** :
  la lentille **oublie AUDIT-BLK-1**. Le nœud qui **détenait** la tx couverte et l'a scellée la
  **remet en file** au reorg (re-queue des tx du tip perdant absentes du gagnant), puis la **re-scelle**
  une fois son financement on-chain. Causalité : le **créateur** d'une tx couverte possède forcément son
  financement (le builder vérifie `balance_of` à la création) ⇒ il l'ordonne et la scelle correctement ;
  les autres nœuds la ré-apprennent par **synchro de blocs** (l'intégration applique les tx d'un bloc
  même si leur hash est dans `seen`). Prouvé end-to-end par
  `cover2_out_of_order_covered_tx_survives_exclusion_and_reorg` (T2 re-queue → re-scellé → Carol reçoit
  ses 20). **Aucune perte permanente.** La politique d'**éviction** (garder dans `seen` vs ré-admission
  par gossip) reste le **choix §4 signalé** (le défaut « évincer » est sain ; alternative = retrait de
  `seen` pour retry-via-gossip, au prix de churn) → décision Alexandre.

> **Bilan adverse COVER-2** : noyau (correction / conservation / déterminisme / invariant §3) **sound**
> sur 3 lentilles ; la 4ᵉ (perte de vivacité) **réfutée par test** (AUDIT-BLK-1 + causalité du
> financement). Couverture **symétrique** complète et sûre.

---

## ADR-006 — Gouvernance & évolutivité (noyau immuable par construction)
*(2026-06-24 · décision de **vision**, non technique · `/goal QUANTA_ADR_006_GOUVERNANCE.md`)*

> Décision de **vision** (couche **au-dessus** du consensus) : Quanta n'a **pas** de gouvernance
> on-chain ; noyau monétaire **immuable par construction** (sans porte, pas verrou) ; périphérie
> **ajustable** (réglages, pas promesses) ; évolution par **fork volontaire + dev ouvert** ;
> **aucun mécanisme de gouvernance dormant** (leçon du CRDT fantôme). **Rien à construire** : pas
> de code. Livrable = l'ADR ratifiable + la **vérification** des deux claims falsifiables.

### Livrables
- **ADR formel** : `docs/decisions/ADR-006 — Gouvernance & évolutivité.md` (style maison ADR-005 ;
  statut **proposé**, frontière à ratifier).
- **Registre** : ligne ADR-006 ajoutée à `docs/decisions/README.md` (classe **vision**, note
  « couche au-dessus du consensus, n'oriente pas le gadget »).
- **Frontière proposée** gravé/ajustable (table adossée au code) — **point de départ** pour la
  ratification d'Alexandre, **pas** une décision figée (Constitution §4 : je cadre, il tranche).

### Vérification des claims falsifiables (audit lecture seule, 2026-06-24)
- **§1 — noyau sans levier de code** : `MAX_SUPPLY_MICRO` (`reputation.rs:71`) et `EMISSION_DIVISOR`
  (`reputation.rs:79`) sont des `pub const` ⇒ **substitués à la compilation, aucun emplacement
  mémoire d'exécution, aucun setter exprimable**. `emission_for_tick` (`reputation.rs:85`) =
  fonction **pure** bornée. Plafond **appliqué au consensus** sur les **deux** chemins via le
  validateur partagé `validate_block_emission_against` (`ledger.rs:1315`, rejet `:1330`). Seul moyen
  de changer = éditer + recompiler = **forker**. ⇒ « **absence de porte** » confirmée.
- **§4 — aucun mécanisme dormant** : recherche `governance|referendum|ballot|proposal|param_change|
  protocol_upgrade` sur `src-tauri/src/` ⇒ **zéro** (le seul `proposal` est `block_proposal*`, un
  bloc de consensus). Les **2 seules** occurrences de `vote` sont des **commentaires**
  (`consensus.rs:4` « sans leader ni votes » ; `gossip.rs:100` anti-Sybil). **Aucun levier.**
- **Vote consensus ≠ gouvernance** : les votes (futurs) du gadget (`sm/finality.rs`, GADGET-1)
  décident **quel bloc est final**, jamais **les règles** ⇒ mécanisme de **consensus** (ADR-005),
  pas de gouvernance. *Rien ne finalise encore* (justify/finalize = GADGET-3) ⇒ aucune machinerie de
  vote même active.
- **§12 ajustables = `const` (fork-only)** : `UNBONDING_PERIOD_BLOCKS` (`ledger.rs:64`),
  `MIN_VALIDATOR_STAKE` (`pos_consensus.rs:64`), `EPOCH_LENGTH_BLOCKS` (`finality.rs:36`).

### Auto-revue — vérification adverse de l'audit lui-même (3 lentilles)
- **« un setter caché / une voie d'écriture manquée »** — *réfuté* : un `const` Rust n'a pas
  d'emplacement mémoire ⇒ il n'existe **pas** de voie d'écriture à manquer (garantie au niveau du
  langage, pas du grep). Le grep confirme en plus zéro `set_*supply` / `*supply =`. La seule mutation
  de l'**offre** est l'émission, elle-même **bornée** par le `const` (`emission_for_tick` saturé).
- **« la promesse survend un mécanisme à deux niveaux inexistant »** — *signalé et neutralisé dans
  l'ADR* : aujourd'hui noyau **et** périphérie sont tous deux de simples `const` (fork-only) — il n'y
  a **pas** de tier d'exécution ajustable. L'ADR le **dit explicitement** (« nuance honnête, pas de
  survente ») : la frontière est de **promesse/intention**, pas de mécanisme câblé ⇒ conforme à
  « Rien à construire » + §4 (l'abstraction d'ajustement n'est **pas** un interrupteur dormant).
- **« un module dormant oublié (re: CRDT fantôme) »** — *réfuté* : recherche exhaustive des lemmes
  de gouvernance ⇒ rien ; les seuls « votes » du code sont des commentaires ; le seul mécanisme de
  vote conçu (gadget) est de consensus et **inactif**. Le risque du CRDT fantôme (code non utilisé =
  surface dormante) **ne se reproduit pas** ici puisqu'il n'y a aucun code de gouvernance à laisser
  dormir.

### Portes (doc-only)
- **Diff logique seule, 100 % documentation** : `docs/decisions/ADR-006 …md` (nouveau) +
  `docs/decisions/README.md` (1 ligne + note) + ce tracker. **Aucun fichier Rust touché** ⇒ aucun
  impact tests/clippy (build inchangé) ; `dispatcher.rs` / `sm/` / consensus **intacts**.
- **Constitution §4 respectée** : la **frontière exacte** gravé/ajustable est **cadrée, pas
  tranchée** (ratification = Alexandre).

> **Bilan ADR-006** : décision de vision **enregistrée** + les deux affirmations du code
> (**noyau sans levier**, **zéro gouvernance dormante**) **vérifiées**, pas seulement assénées.
> Reste indélégable (Alexandre) : ratifier la **frontière exacte** gravé/ajustable ; les valeurs
> 🛑 §12 (E, comité, quorum, `UNBONDING_PERIOD_BLOCKS`, `MIN_VALIDATOR_STAKE`) ; la conception du
> gadget. Commit baseline git = **manuel, Alexandre**.

---

## GADGET-2 — Votes ML-DSA + certificat d'époque (la matière de la finalité)
*(2026-06-24 · `/goal QUANTA_GADGET_PIECE2_1.md` · pièce 2 du gadget, §3/§5 de DESIGN-FINALITY-GADGET)*

> On bâtit les **votes** (attestations) et le **certificat d'époque** (lien super-majoritaire ⅔),
> derrière l'**abstraction de certificat** d'ADR-005, sur le squelette GADGET-1 (points de contrôle)
> et l'enjeu on-chain ONCHAIN-STAKE-1. **Rien ne finalise** (la règle justifier/finaliser = GADGET-3) :
> ici, la matière + sa **vérification pure**. Diff logique seule, déterministe, **C1 vert**.

### Livrables (1 fichier neuf + 2 retouches logiques)
- **`src-tauri/src/sm/finality_vote.rs`** (neuf) : `Vote`, `signable_bytes` (canon. longueur-préfixé +
  domaine), `verify` (pur), trait **`FinalityCertificate`** (abstraction ADR-005, **définition unique**),
  impl **`MlDsaCertificate`**, maths quorum intègres (`meets_supermajority`, `total_stake` checked) + 11 tests.
- **`src-tauri/src/sm/mod.rs`** (+2 l.) : `pub mod finality_vote;` + re-export `Vote/MlDsaCertificate/FinalityCertificate`.
- **`src-tauri/src/security/hybrid_crypto.rs`** (+5 l.) : `verify_ml_dsa` passé `pub(crate)` — **unique**
  vérificateur ML-DSA réutilisé tel quel (aucune duplication). Signage `ml_dsa_sign_deterministic`
  (SIGN-DET, `#[cfg(test)]`) réutilisé pour les votes en sim ; **aucun signage production ajouté**.

### §1 — le vote (attestation)
- `Vote { source, target, voting_epoch, validator, signature }`. **Vérification** (pure de `(vote, enjeu, E)`) :
  (a) lien bien formé — `source`/`target` sont des points de contrôle (`is_epoch_boundary` + `epoch==height/E`,
  helpers GADGET-1 réutilisés), `voting_epoch==target.epoch`, **`target.height > source.height`** (descente =
  lien avant) ; (b) signataire = **validateur actif** (`enjeu>0`) ; (c) **signature ML-DSA-65 valide**. Le
  **poids** = l'enjeu on-chain.
- **Descente = structurelle** (frontière strictement postérieure), **pas** l'ancêtre-hash complet : ce dernier
  est le travail de GADGET-3 (qui a la chaîne et raisonne la justification). On **n'anticipe pas** la règle (§5).

### §2 — le certificat, derrière l'abstraction ADR-005
- Trait **`FinalityCertificate`** = *lien super-majoritaire vérifiable* `source→target` ; le **schéma
  d'agrégation** (aujourd'hui un `Vec<Vote>` ML-DSA ; demain BLS/SNARK) vit **derrière le trait** ⇒ remplacement
  **local**, GADGET-3 (consommateur) intact. **Une seule** définition du concept.
- `backing_weight` (pur) : `None` si **malformé** — vote invalide, **liens mélangés**, **votant dupliqué** ;
  sinon Σ enjeu des votants **distincts** (checked). `is_valid` = bien formé **et** `backing*3 ≥ total*2`
  (≥⅔, intègre `u128`, **zéro flottant**) **et** `backing>0` (un cert vide/dégénéré contre enjeu nul ne passe pas).

### §4 — les dents (mordent réellement) — 6 obligatoires + 5 au niveau vote
- `gadget2_certificate_below_quorum_is_rejected` : 100/300 ⇒ `backing=Some(100)`, **rejeté** (math, non malformé).
- `gadget2_certificate_with_forged_vote_is_rejected` : 2 votants atteindraient ⅔, **une signature retournée** ⇒
  `backing=None`, **rejeté**.
- `gadget2_certificate_mixing_links_is_rejected` : vote off-link **valide sur SON lien** mais ≠ lien du cert ⇒
  `None`, **rejeté**.
- `gadget2_certificate_double_counting_is_rejected` : 2× le même votant **atteindrait** 200/300 si compté ⇒
  distinction ⇒ `None`, **rejeté** (la dent mord : sans elle, faux quorum).
- `gadget2_valid_two_thirds_certificate_is_accepted` : 200/300 = **exactement ⅔** ⇒ **accepté** (seuil ≥).
- `gadget2_verdict_is_deterministic_across_nodes` : deux « nœuds » construisent le même snapshot d'enjeu + les
  mêmes votes ⇒ **signatures identiques** (SIGN-DET), **backing identique**, **verdict identique**.
- Niveau vote : valide vérifie ; non-validateur rejeté ; lien non-descendant rejeté ; point de contrôle malformé
  (hauteur non-frontière) rejeté ; **champ signé altéré** ⇒ vérif. cassée (liaison réelle).

### Portes d'acceptation — toutes vertes
- `cargo test --lib` : **317 / 0** (306 + 11 GADGET-2), incl. les 6 dents du §4.
- `cargo clippy --lib -- -D warnings` **propre** · **C1 vert** (`determinism_meta_test_128…`) ·
  **`t0_8_clean_default_sweep` + `t0_8_sweep_is_reproducible` verts** · `src/sm/` **sans-IO** (la vérif. ML-DSA
  est du calcul pur, zéro horloge/entropie/IO).
- **Diff logique seule** : **3 fichiers** (1 neuf + 2 retouches) ; `sm/finality.rs` (GADGET-1) **intact** ⇒
  **`FinalitySafety` inchangé** (toujours quasi-vacueux, comme exigé §5) ; `dispatcher.rs` **intact** (0 réf).

### Défauts marqués (réglables, ADR-005/§12 — pas des promesses, cf. ADR-006)
- **Quorum = ⅔** de l'enjeu (`QUORUM_NUM/DEN`, standard BFT).
- **Comité = tous les validateurs actifs** (`validator_stakes()`, clés `enjeu>0`), **sans plafond** (ADR-005).
- **E = `EPOCH_LENGTH_BLOCKS`** (32 provisoire, GADGET-1), paramétrique partout.

### Auto-revue — vérification adverse GADGET-2 (5 lentilles)
- **forge d'un vote pondéré (identité)** — **fermée dans le module + flag §4** : un attaquant ne peut pas
  apparier la clé-compte d'une victime avec **sa** clé ML-DSA pour voler son poids, **parce que l'identité de
  finalité EST la clé publique ML-DSA** (à la fois clé d'enjeu et clé de vérif.) — sans le secret, pas de
  signature valide. La liaison publique compte→ML-DSA est **impossible** (clé PQ dérivée du *secret* Ed25519,
  hors d'atteinte du public), donc « porter les deux clés » serait le trou ; faire de la clé ML-DSA l'identité
  le ferme. **Reste flag §4** (indélégable, Alexandre) : réconcilier avec `validator_stakes()` keyé-compte
  (registre de liaison **ou** validator set re-keyé ML-DSA). **Ne bloque rien** : rien ne finalise, aucun chemin
  live câblé ; GADGET-2 écrit **agnostique** à la provenance de la clé.
- **déterminisme / divergence inter-nœuds** — *sound* : verdict = fonction pure de `(votes, enjeu on-chain)` ;
  sommes **commutatives** (aucun ordre HashMap dans le résultat) ; distinction par **`BTreeSet`** ; seuil ⅔
  **intègre** `u128` (zéro flottant) ; enjeu issu de la chaîne ⇒ **même seuil, même verdict** partout ; SIGN-DET
  reproductible (testé : signatures byte-identiques sur deux nœuds). `sm/` sans-IO + C1 verts.
- **maths du quorum** — *sound* : `backing*3 ≥ total*2` (≥⅔ exact, `u128` ⇒ pas d'overflow ; `backing,total ≤ u64`),
  `total` **checked_add** (refus sur overflow, jamais de wrap vers un faux quorum), garde **`backing>0`**
  neutralisant le cert vide/dégénéré contre enjeu nul.
- **anti-vacuité (les dents mordent)** — *sound* : chaque dent a un **contre-factuel** — le double-comptage
  **atteindrait** 200/300 sans la distinction ; le vote forgé **atteindrait** ⅔ sans la vérif. de signature ;
  le « below-quorum » distingue rejet **mathématique** (Some(100)) de rejet **malformé** (None). Pas de test mou.
- **périmètre / non-anticipation** — *sound* : **rien ne finalise**, `FinalizedSet`/`FinalitySafety` **non touchés** ;
  descente = lien-avant **structurel** (l'ancêtre-hash est GADGET-3) ; aucune règle justifier/finaliser codée.
  Signalé : la descente structurelle est une **limite assumée** que GADGET-3 resserrera avec la chaîne.

> **Bilan GADGET-2** : la **matière** (votes ML-DSA + certificat ⅔ derrière l'abstraction ADR-005) est posée et
> **vérifiée par des dents qui mordent** ; noyau (déterminisme / quorum / anti-vacuité / périmètre) **sound** sur
> 4 lentilles ; la 5ᵉ (forge d'identité) **fermée dans le module**, avec la **réconciliation du keying** (compte
> ↔ clé ML-DSA) explicitement **remontée à Alexandre** (§4, Constitution). Prochaine pièce **GADGET-3** : la
> règle justifier/finaliser qui **consomme** ces certificats — là, `FinalitySafety` cesse d'être vacueux.
> Valeurs §12 (E, taille de comité, quorum) restent réglables. Commit baseline git = **manuel, Alexandre**.

---

## CRYPTO-ID-1 — Audit identité enjeu vs finalité → **§4 STOP** (décision de portée PQ à Alexandre)
*(2026-06-24 · `/goal QUANTA_CRYPTO_IDENTITY.md` · audit lecture seule, prérequis de GADGET-3)*

> GADGET-2 a révélé deux identités **disjointes** : l'enjeu indexé par la clé **Ed25519** de compte,
> les votes de finalité signés en **ML-DSA**. Ce spec **audite** l'état réel, puis réconcilie **si
> trivial** (cas « comptes déjà ML-DSA »), **sinon §4 STOP**. Verdict : comptes **vraiment Ed25519**
> ⇒ aucune réconciliation triviale ⇒ **STOP**, décision (b) à Alexandre. **Zéro code écrit.**

### §1 — Rapport d'audit (faits, preuves à l'appui)
| Question | Réponse | Preuve |
|---|---|---|
| Qu'est-ce qui **identifie un compte** ? | La **clé publique Ed25519** (hex 64 car.) | `security/mod.rs:45-46` `public_key_hex = hex(vk.to_bytes())`, vk = Ed25519 ; `import_keypair` part d'un secret **Ed25519** 32 B (`:56-66`) |
| Qu'est-ce qui **signe une tx** ? | **Hybride** Ed25519 + ML-DSA — mais la **racine** est Ed25519 | `sign_hybrid` (`security/mod.rs:100`) ; `verify_tx` vérifie `verify_hybrid(tx.from(=Ed25519), pq_pk, …)` (`ledger.rs:1031`) ; `tx.signature` = « Ed25519 signature » (`ledger_types.rs:22`) |
| La clé ML-DSA est-elle **liée** au compte ? | **NON** — auto-déclarée par tx, non liée | `tx.pq_public_key: Option<String>` **porté dans chaque tx** (`ledger_types.rs:31`) ; `verify_hybrid` = `ed_ok && pq_ok`, **aucun cross-check** pq↔ed (`hybrid_crypto.rs:129-145`) |
| Quelle clé **indexe l'enjeu** (`validator_stakes()`) ? | La clé **Ed25519** de compte (`tx.from`) | `staked.entry(tx.from.clone())` (`ledger.rs:403`) ; `validator_stakes()` filtre `staked` (`ledger.rs:513`) |
| Quelle clé **signe les votes** de finalité ? | La clé publique **ML-DSA** (GADGET-2) | `sm/finality_vote.rs` `Vote.validator` = ML-DSA pk hex ; vérif. `verify_ml_dsa` |
| **Registre on-chain** compte → ML-DSA ? | **Aucun** | `pq_public_key` n'apparaît que comme champ **par-tx** (grep arbre) ; pas de map persistée |
| Racine du **transport** (gossip) ? | **Ed25519** | `GossipEnvelope` : pubkey + signature **Ed25519** (`gossip.rs:39,42`) |

**Constat-clé (honnête, sans survente).** La signature « hybride » est **réellement** calculée et
vérifiée sur les deux couches, **mais la racine de confiance du compte reste Ed25519** :
(i) `REQUIRE_PQ=false` autorise le repli Ed25519-seul (`hybrid_crypto.rs:58,132-140`) ; (ii) la clé
ML-DSA est **auto-déclarée par tx et non liée** au compte. Conséquence concrète : un adversaire
quantique qui casse Ed25519 forge `tx.signature` de la victime, **attache sa propre** clé ML-DSA en
`pq_public_key`, signe la couche quantum avec — `verify_hybrid` passe (`ed_ok && pq_ok`). **La couche
ML-DSA ne protège donc pas le compte contre la forge quantique** (« récolter aujourd'hui, forger
demain » s'applique aux **comptes**, pas seulement aux tx anciennes). C'est un **vestige de
rétro-compat**, pas un bug d'implémentation — mais « comptes post-quantiques » serait **survendre**.

### §2 — Décision de portée PQ (🛑 vision, Alexandre) — cadrée avec les faits
- **(a) Comptes Ed25519 + finalité PQ seulement** (registre de liaison). Les validateurs
  **enregistrent on-chain** une clé de finalité ML-DSA, **signée par leur compte Ed25519** (liaison
  vérifiable). GADGET-2 vérifie contre la clé enregistrée ; le poids se lit par le compte lié.
  *Portée* : 1 nouveau tx/registre `compte → ml_dsa_pk` (déterministe, dérivé de la chaîne) ;
  comptes/tx **inchangés**. *Coût* : **léger**. *Prix* : les **comptes restent vulnérables au
  quantique** → « entièrement PQ » garde un **astérisque** (contredit ADR-005 esprit / whitepaper).
- **(b) Tout en ML-DSA** *(recommandation du spec, à ratifier)*. L'identité de compte **devient** la
  clé ML-DSA ; enjeu et finalité **coïncident nativement**. *Portée de migration* (l'audit la chiffre) :
  `CryptoEngine` re-raciné ML-DSA (keygen + vault `pq_vault.rs`) ; **adresse** = hash de la pk ML-DSA
  (la pk brute fait 1952 B → adressage par BLAKE3) ; `tx.from/to`, `verify_tx` (ML-DSA, plus de racine
  Ed25519), builder, `validator_stakes` (devient ML-DSA-keyé ⇒ **but atteint**), registre @pseudo,
  **persistance/soldes existants**, et **bump `TORUS_PROTOCOL_VERSION`** (tx/clé plus grandes : pk
  1952 B, sig 3309 B). *Coût* : **lourd, migration de comptes**. *Gain* : promesse PQ **sans
  astérisque**.

### §3 → §4 STOP — pas de réconciliation triviale
- Le re-key trivial n'est autorisé **que** si « comptes déjà ML-DSA » (spec §3). **Faux ici**
  (comptes Ed25519, preuves §1).
- Re-keyer `validator_stakes()` vers ML-DSA depuis la chaîne est **impossible sainement** : la seule
  donnée ML-DSA on-chain (`tx.pq_public_key`) est **non liée et forgeable**, et un compte peut
  attacher des clés ML-DSA **différentes** selon les tx ⇒ **aucun mapping compte→ML-DSA déterministe
  ni sûr** à re-keyer.
- Downgrader la finalité vers Ed25519 pour « coïncider » est **exclu** (ADR-005 : finalité PQ pure,
  « aucune primitive classique sur le chemin de l'irréversibilité »).
- ⇒ **§4 STOP**. **Aucun code de migration écrit** ; **aucune modification** ce goal (audit lecture
  seule). Diff = **ce tracker seul**.

### Auto-revue (§3)
- **exhaustivité de l'audit** — *sound* : les 5 maillons d'identité (compte, tx, enjeu, finalité,
  transport) tracés avec `fichier:ligne` ; le maillon manquant (liaison compte↔ML-DSA) **prouvé
  absent** (pas de cross-check dans `verify_hybrid`, pas de registre on-chain).
- **« et si un re-key trivial existait ? »** — *réfuté* : la seule source ML-DSA on-chain est
  non liée/forgeable et non-unique par compte ⇒ pas de map déterministe sûre ; donc le cas trivial du
  §3 est **réellement** inapplicable (pas un STOP « par confort »).
- **pas de masquage** — *respecté* : je **n'ai pas** bricolé un pont (ex. faire confiance à
  `tx.pq_public_key`), ce qui aurait introduit le **vecteur de forge** ci-dessus. Le constat « racine
  Ed25519 » est **rapporté tel quel**, même s'il nuance la promesse PQ (Constitution : ne pas survendre).
- **déterminisme / périmètre** — *respecté* : aucune clé de consensus introduite ; GADGET-2 inchangé ;
  `FinalitySafety` intact.

> **Bilan CRYPTO-ID-1** : enjeu (Ed25519) et finalité (ML-DSA) sont **disjoints parce que les comptes
> sont Ed25519** — fait établi, preuves §1. **§4 STOP** : la coïncidence des identités passe par une
> **décision de portée PQ** (a) registre de liaison **vs** (b) comptes ML-DSA — **à toi**. Recommandation
> du spec : **(b)** (compte de longue vie ⇒ doit être PQ), au prix d'une **migration de comptes** dont
> la portée est chiffrée ci-dessus. **Une fois (a)/(b) tranché**, cela mérite un **ADR** (ADR-007 ?) +
> son spec de mise en œuvre, et **débloque GADGET-3** (pondération de vrais votes). Commit baseline git
> = **manuel, Alexandre**.

---

## ADR-007 — Portée du post-quantique : comptes ML-DSA (la décision la plus fondatrice qui reste)
*(2026-06-24 · `/goal QUANTA_ADR_007_PQ_SCOPE.md` · décision de **vision fondatrice**, suite directe de CRYPTO-ID-1)*

> CRYPTO-ID-1 a escaladé la décision de portée PQ (registre (a) vs comptes ML-DSA (b)). ADR-007 la
> **grave** : recommandation **forte (b)** (Quanta réellement entièrement post-quantique), l'**engagement**
> au coût étant la part **§4/vision d'Alexandre**. **Aucun code de migration** avant ratification ; la
> migration se **conçoit** ensuite (doc de conception → specs chirurgicaux). **Bloque GADGET-3.**

### Livrables (documentation seule — vision, comme ADR-006)
- **ADR formel** : `docs/decisions/ADR-007 — Portée du post-quantique (comptes ML-DSA).md` (style maison ;
  statut **proposé**, recommandation (b)), avec une section **Vérification** adossée à CRYPTO-ID-1.
- **Registre** : ligne ADR-007 + note dans `docs/decisions/README.md` (classe **vision fondatrice 🛑**,
  « **bloque GADGET-3** » ; lien à ADR-006 — (b) rend l'invariant « signatures PQ » gravé **honnête** au
  niveau du compte).

### Décision cadrée (🛑 Alexandre)
- **(a) registre de finalité** : léger, mais **astérisque permanent** (comptes forgeables au quantique) →
  *rejetée comme recommandation*.
- **(b) comptes tout ML-DSA** *(recommandé)* : enjeu↔finalité **coïncident nativement** (débloque GADGET-3),
  promesse **sans astérisque** ; **lourd** (re-racinage `CryptoEngine`/vault, adresses BLAKE3 d'une pk ML-DSA
  ~1952 o, `verify_tx`/builder/`validator_stakes`/`@pseudo`, migration des soldes, bump `TORUS_PROTOCOL_VERSION`,
  `REQUIRE_PQ=true`, retrait du repli Ed25519).
- L'**engagement** (payer le coût vs expédier (a) plus vite) dépend de l'horizon/ressources/tolérance du
  fondateur — **non tranchable par l'ingénierie** (Constitution §4 : je **cadre**, il **tranche**).

### Auto-revue (§3)
- **prémisse falsifiable vérifiée** — *sound* : les faits qui fondent (b) (compte=Ed25519, `REQUIRE_PQ=false`,
  clé ML-DSA non liée, `verify_hybrid = ed_ok && pq_ok`) sont **prouvés** par CRYPTO-ID-1 avec `fichier:ligne`,
  cités dans l'ADR ⇒ « aucune assertion de sécurité sans vérification » (Constitution §2).
- **§4 respectée** — *sound* : **aucun code de migration écrit** (le spec l'interdit avant ratification) ;
  la décision d'**engagement** est **cadrée, pas tranchée** ; recommandation (b) **enregistrée**, ratification
  = Alexandre.
- **pas de survente** — *respecté* : l'ADR **dit** que « comptes post-quantiques » est aujourd'hui faux
  (état de fausse promesse), au lieu de le masquer — cohérent avec « tu ne survends jamais ».
- **diff documentation seule** — *vérifié* : 1 ADR neuf + 1 ligne/note registre + ce tracker ; **zéro fichier
  Rust touché** ⇒ build/tests inchangés ; `dispatcher.rs` / `sm/` / consensus **intacts**.

> **Bilan ADR-007** : la décision de portée PQ est **gravée comme recommandation (b)** et **remontée** ;
> elle **bloque GADGET-3** jusqu'à ratification. Suite, **une fois (b) ratifié** : un **document de
> conception** de la migration (re-racinage, schéma d'adresses, soldes, version protocole), **puis** des
> specs chirurgicaux. Commit baseline git = **manuel, Alexandre**.

---

## PQ-MIG-1 — re-raciner CryptoEngine + vault sur ML-DSA-65 (1ʳᵉ pièce du chantier PQ)
*(2026-06-24 · `/goal QUANTA_PQ_MIG_1_1.md` · ADR-007 (b) ratifié · diff logique, déterministe, C1 vert)*

> Première pièce, **la plus contenue**, du chantier post-quantique. On établit **ML-DSA-65 comme identité
> primaire** générée / stockée / rechargée par le moteur crypto et le vault — **la racine PQ**. **Aucune
> autre couche touchée** (ni `verify_tx`, ni adresses, ni enjeu, ni transport, ni genèse) : c'est
> **additif**, Ed25519 **coexiste** intact pour les consommateurs non encore migrés (PQ-MIG-3+).

### Décision de conception (la racine ML-DSA est **indépendante**)
La racine ML-DSA est enracinée sur **sa propre** graine de 32 octets, **tirée d'`OsRng`**, **indépendante**
de la graine Ed25519 — **pas** dérivée d'elle. C'est délibéré : dériver ML-DSA d'une clé classique
**réintroduirait la faille exacte que CRYPTO-ID-1 a exposée** (casser Ed25519 livrerait la clé ML-DSA), or
ADR-007 (b) existe précisément pour la **fermer**. La graine reproduit déterministiquement la paire via
`derive_ml_dsa` (XOF BLAKE3 → keygen) ⇒ le vault persiste **32 octets** et le rechargement redonne la clé
publique **identique**.

### Livrables (moteur crypto + vault **seulement**)
- **`security/mod.rs`** — `CryptoEngine` gagne un champ `ml_dsa_primary` (struct `MlDsaPrimary { seed:
  Zeroizing<[u8;32]>, sk: ml_dsa_65::PrivateKey, pk_hex }`, **deux secrets zeroize-on-drop**) et les méthodes :
  `generate_pq_identity` (graine `OsRng` → racine), `import_pq_identity` (**chemin déterministe** : reload
  vault + sim), `pq_identity_hex`, `get_pq_seed_bytes` (`Zeroizing<Vec<u8>>` auto-effaçant, miroir de
  `get_secret_bytes`), `sign_pq` (production **hedgée** `OsRng`), `sign_pq_det` (`#[cfg(test)]`, déterministe,
  **physiquement absente du release**), `verify_pq` (fin wrapper sur l'**unique** `verify_ml_dsa`, zéro
  duplication).
- **`security/pq_vault.rs`** — `create_pq_identity` / `unlock_pq_identity` (**additifs** ; Ed25519
  `create_identity`/`unlock_identity` **intacts**) : chiffrent / déchiffrent la **graine racine 32 o**
  (Argon2id → AES-256-GCM, sel domaine-séparé `PQ-MIG-1`), **zeroize** des copies, **round-trip → même pk**,
  mauvais mot de passe → **échec opaque**.
- **6 tests `pq_mig1_*`** : signage/vérif ML-DSA **de bout en bout via le moteur** ; **round-trip vault →
  même clé publique** (+ la clé rechargée **signe** ⇒ la clé secrète round-trip aussi) ; déterminisme
  (même graine → même clé) ; **sim byte-reproductible vs prod hedgée** ; **indépendance vis-à-vis d'Ed25519**
  (même graine Ed25519 ⇒ identités PQ **différentes** ; primaire ≠ couche héritée dérivée) ; **graine
  exportée auto-effaçante** (type `Zeroizing` — une régression `Vec<u8>` nu **ne compile pas**).

### Porte d'acceptation (toutes vertes)
- `cargo test --lib` : **323 passés / 0 échec** (317 + 6) — round-trip ML-DSA + signage/vérif e2e inclus.
- `cargo clippy --lib -- -D warnings` **propre** · **C1 vert** (`…128_runs_are_byte_identical`) · **sweep par
  défaut vert** (`t0_8_clean_default_sweep` + `t0_8_sweep_is_reproducible` + 10 autres).
- **Diff logique seule**, **confinée à `security/mod.rs` + `security/pq_vault.rs`** (snapshot pré-goal
  `9d27812`) ; **purement additive** (seule « suppression » = `new()` qui initialise le nouveau champ) ;
  `dispatcher.rs` / `sm/` / `ledger.rs` / `gossip.rs` / `lib.rs` **non touchés** ⇒ couches Ed25519 **inchangées**.

### Auto-revue (§3)
- **identité primaire ML-DSA** — *sound* : le moteur **génère, stocke (zeroize), recharge** une paire
  ML-DSA-65 comme racine, prouvé par le round-trip vault (même pk) et le signage/vérif e2e. Racine
  **indépendante** d'Ed25519 (test dédié) ⇒ ne répète pas la faille CRYPTO-ID-1.
- **déterminisme (vigilance n°1)** — *sound* : génération/reload empruntent `derive_ml_dsa` (déterministe) ;
  le signage sim passe par `sign_pq_det` (`#[cfg(test)]`, BLAKE3-RNG), la production par `sign_pq` (`OsRng`,
  **hedgée**) ; **ML-DSA n'est pas câblée au consensus/au fingerprint** (additive), donc **C1 reste
  byte-identique** (vérifié, 128 runs).
- **§3 pas de fuite non déterministe** — *vérifié* : `fingerprint` (`sm/node.rs`) ne hashe que l'état
  **observable du ledger** (hashes de blocs, soldes triés, agrégats scalaires) — **aucune clé** ; la racine
  ML-DSA ne vit pas dans le ledger ⇒ rien à fuiter ; C1 inchangé le **prouve transitivement**.
- **zeroize** — *respecté* : `seed: Zeroizing<[u8;32]>` + `sk` (`fips204::PrivateKey` zeroize-on-drop) effacés
  au drop ; `get_pq_seed_bytes` rend un `Zeroizing` auto-effaçant ; le vault efface les copies déchiffrées.
- **périmètre tenu** — *vérifié* : **2 fichiers** (moteur + vault), aucune autre couche ; **§4** non
  déclenchée (rien d'amont n'a eu besoin de bouger pour compiler).
- **pas de masquage** — *honnête* : les `#[allow(dead_code)]` sur le vault PQ marquent du **scaffolding non
  encore câblé** (production à PQ-MIG-3), **testé** dès maintenant — même convention que `NodePuzzle`/
  `solve_node_puzzle` dans le fichier ; **aucune** régression/échec masqué (clippy `-D warnings` **propre**).

> **Bilan PQ-MIG-1** : la **racine ML-DSA-65 est posée** (indépendante, déterministe, zeroize, round-trip
> prouvé), Ed25519 **coexiste** sans régression, **C1 vert**. Pièce suivante **PQ-MIG-2** : adresses =
> `BLAKE3(pk_ml_dsa)` tronquées. Puis PQ-MIG-3 (`verify_tx` ML-DSA + liaison clé↔adresse, **retrait du repli
> Ed25519**, câblage production du vault PQ), PQ-MIG-4 (re-clé de l'enjeu → **GADGET-3 débloqué**), PQ-MIG-5
> (genèse PQ). Commit baseline git = **manuel, Alexandre**.

---

## PQ-MIG-2 — adresses = `BLAKE3(domaine ‖ clé publique ML-DSA)` (2ᵉ pièce du chantier PQ)
*(2026-06-24 · `/goal QUANTA_PQ_MIG_2.md` · construit sur PQ-MIG-1 · diff logique, déterministe, C1 vert)*

> Deuxième pièce, **contenue**. La clé publique ML-DSA fait ~1952 o — **trop grande pour servir d'adresse**.
> On introduit l'**adresse = hash BLAKE3 domaine-séparé de la clé publique ML-DSA**, son encodage/décodage, et
> la **fonction de liaison** clé↔adresse que PQ-MIG-3 utilisera. On **ne câble pas** `from`/`to` ni `verify_tx`
> (= PQ-MIG-3) : exactement comme PQ-MIG-1 a posé la clé sans la brancher. **Additif**, Ed25519 intact.

### Précision vs la note d'anticipation PQ-MIG-1
La conclusion PQ-MIG-1 anticipait des adresses « **tronquées** ». Le spec PQ-MIG-2 §1 tranche l'inverse et
**à raison** : on garde les **32 octets** de sortie naturelle de BLAKE3, **sans troncature** (256 bits de
résistance aux collisions). La longueur 32 o est **marquée 🛑 réglable** — raccourcissable plus tard si une
adresse plus courte est explicitement voulue, mais par défaut on ne sacrifie pas la marge de collision.

### Livrables (`security/mod.rs` **seulement**)
- **`ADDR_DOMAIN = b"QUANTA-ADDR-V1"`** — étiquette de **séparation de domaine** préfixée au hash (même
  discipline que `ML_DSA_DOMAIN` / `LEADER_VRF_DOMAIN`), pour qu'une adresse ne puisse **jamais** entrer en
  collision avec un hash de bloc/tx. Marquée « ne jamais modifier ».
- **`ml_dsa_address_bytes(pk) -> [u8;32]`** (§1) — `BLAKE3(ADDR_DOMAIN ‖ pk)`, **pure & déterministe**, aucune
  entropie ; + `ml_dsa_address_hex` (raccourci textuel).
- **`encode_address` / `decode_address`** (§2) — hex (encodage déjà utilisé pour les identités) ; **round-trip
  exact** ; décodage à **erreur opaque** sur entrée malformée (hex invalide / mauvaise longueur), **jamais de
  panique**.
- **`address_binds_key(addr, pk) -> bool`** (§3, la fonction `lie`) — vrai **ssi** `addr == BLAKE3(ADDR_DOMAIN
  ‖ pk)` ; + variante `address_hex_binds_key_hex(addr_hex, pk_hex)` (la forme que `verify_tx` manipulera en
  PQ-MIG-3 ; entrée cassée ⇒ `false`, pas de contournement).
- **`pq_address` / `pq_address_hex`** (§4) — le moteur (identité primaire PQ-MIG-1) **expose son adresse**,
  dérivée de sa clé publique ML-DSA. Lecture seule, pure.
- **5 tests `pq_mig2_*`** : adresse **déterministe** (vecteur connu ⇒ adresse connue **épinglée**
  `64eb5334…d0d6e42e`, + reconstruction indépendante `BLAKE3(ADDR_DOMAIN ‖ pk)`) ; **round-trip** encode/décode
  (+ rejet des longueurs 31/33 o et de l'hex invalide) ; **dents de liaison** (bonne clé ⇒ vrai ; **autre** clé
  ⇒ faux ; adresse altérée d'1 bit ⇒ faux ; variantes hex) ; **séparation de domaine** (adresse ≠ `BLAKE3(pk)`
  nu **et** ≠ hash sous un autre domaine) ; **exposition moteur** (`pq_address` cohérente, `None` sans identité).

### Porte d'acceptation (toutes vertes)
- `cargo test --lib` : **328 passés / 0 échec** (323 + 5).
- `cargo clippy --lib -- -D warnings` **propre** · **C1 vert** (`…128_runs_are_byte_identical`) · **sweep par
  défaut vert** (12 `t0_8_*`).
- **Diff logique seule**, **confinée à `security/mod.rs`** : tous les symboles introduits (`ADDR_DOMAIN`,
  `ml_dsa_address*`, `address_binds_key*`, `pq_address*`, `encode/decode_address`, module `pq_mig2_address`)
  n'apparaissent **que** dans ce fichier (grep) ; **purement additive** (aucune ligne pré-existante supprimée/
  reformatée par PQ-MIG-2 — les 4 suppressions du `git diff HEAD` sont **toutes** de PQ-MIG-1) ;
  `dispatcher.rs` / `ledger.rs` (`from`/`to`/`verify_tx`) / `pos_consensus.rs` (enjeu) / `sm/` **non touchés**.

### Auto-revue (§3)
- **dérivation domaine-séparée** — *sound* : `BLAKE3(ADDR_DOMAIN ‖ pk)` ; le test de séparation **mord**
  (adresse ≠ hash nu **et** ≠ autre domaine), donc le tag participe réellement — pas de collision possible avec
  un hash de bloc/tx.
- **longueur 32 o marquée réglable** — *honnête* : sortie naturelle BLAKE3, **sans troncature** (le spec a
  corrigé la note PQ-MIG-1 « tronquées ») ; 🛑 marquée réglable dans le code, décision de raccourcissement
  **non prise** (n'invente pas une longueur courte).
- **fonction de liaison + dents** — *sound, pas de masquage* : `address_binds_key` est vraie **ssi** l'adresse
  est le hash de la clé ; une **autre** clé échoue, une adresse altérée échoue, et les variantes hex à entrée
  cassée renvoient `false` (un encodage invalide ne contourne pas la liaison). C'est la dent que `verify_tx`
  exigera en PQ-MIG-3.
- **déterminisme (vigilance)** — *sound* : dérivation **pure** BLAKE3, **aucune entropie** ⇒ même clé ⇒ même
  adresse partout ; **non câblée** au ledger/fingerprint/sim (additive) ⇒ **C1 reste byte-identique** (vérifié).
- **périmètre tenu** — *vérifié* : **1 fichier** (`security/mod.rs`) ; **§4** non déclenchée (rien d'amont —
  `from`/`to`/`verify_tx`/enjeu/genèse — n'a eu besoin de bouger pour compiler).

> **Bilan PQ-MIG-2** : le **schéma d'adresses ML-DSA est posé** — dérivation domaine-séparée 32 o (réglable),
> encodage/décodage round-trip, **fonction de liaison `lie()` avec dents**, et le moteur **expose son adresse**.
> Toujours **additif** : `from`/`to`/`verify_tx`/enjeu/genèse **inchangés**, **C1 vert**. Pièce suivante
> **PQ-MIG-3** : `verify_tx` en ML-DSA, qui **exige** via `lie()` que la clé révélée hashe vers l'adresse
> `from`, met la clé dans la préimage signée, **retire le repli Ed25519** et **câble le vault PQ en production**
> (le scaffolding `#[allow(dead_code)]` de PQ-MIG-1 y disparaît). Puis PQ-MIG-4 (enjeu re-clé → **GADGET-3
> débloqué**), PQ-MIG-5 (genèse PQ). Commit baseline git = **manuel, Alexandre**.

---

## PQ-MIG-3 — autorité de tx en ML-DSA par **liaison on-chain** (ferme CRYPTO-ID-1) — [[ADR-008]]
*(2026-06-24 · `/goal QUANTA_PQ_MIG_3.md` · pièce charnière · diff logique, déterministe, C1 vert)*

> **La pièce la plus importante du chantier.** Elle bascule la **racine d'autorité des comptes** en
> post-quantique et **ferme la faille CRYPTO-ID-1**. **Mais** elle n'a **pas** suivi le §1 de la spec à
> la lettre — voir la décision de portée ci-dessous.

### ⚖️ §4 STOP → décision de portée (Alexandre) — voir **[[ADR-008]]**
En implémentant, un fait du code **invalide** le découpage PQ-MIG-3 → PQ-MIG-4 de la spec : l'**identifiant
de compte est unifié** — le **même** `tx.from` (clé pub Ed25519) est le solde, la **cible de minage**, la
clé d'**enjeu/validateur** (`staked`/`validator_stakes` indexent sur `from`), le **@pseudo** et l'identité
de **transport**. Faire de `from` une **adresse ML-DSA** (spec §1) **re-clé donc inévitablement l'enjeu et le
minage** — le livrable de **PQ-MIG-4**, interdit par le garde-fou §4 de PQ-MIG-3. Arbitrage non tranché par
la constitution ⇒ **§4 STOP**, escaladé. **Alexandre a tranché : Option B** — garder `from` = Ed25519, **lier**
une clé ML-DSA au compte par **registre on-chain immuable** (la **voie (a)** d'ADR-007, *astérisque permanent*
assumé ; ADR-007(b) « comptes tout ML-DSA » **différé**).

### Conception (Option B — ferme la faille sans re-clé)
- **Clé liée = identité primaire indépendante** (PQ-MIG-1, graine `OsRng` propre), **jamais** la couche
  héritée dérivée de la graine Ed25519 — sinon casser Ed25519 reconstruirait la clé liée (la faille même).
- **Registre `from → clé_ML-DSA`** = **fonction pure de la chaîne** (`pq_bindings_before`, premier-vu
  **immuable**, comme `staked`) — **aucun état persistant ajouté**, aucun changement de `LedgerSnapshot`.
- **Autorité** = signature **ML-DSA** valide de la clé révélée **+** Ed25519 co-facteur (qui autorise la
  **première** liaison, anti-front-running) ; la **clé ML-DSA est dans la préimage** signée (§2) ; **plus
  aucun repli Ed25519 seul**. La moitié **statique** (crypto) est dans `verify_tx` ; la moitié **étatique**
  (clé == clé liée, immuable) est `binding_violations`, **source unique** que la **validation rejette** et
  le **seal exclut** (symétrie COVER-1/COVER-2).

### Livrables
- **`security/mod.rs`** — `sign_tx_authority` / `sign_tx_authority_det` (`#[cfg(test)]`) : signent Ed25519 +
  **primaire** ML-DSA sur la préimage liante.
- **`p2p/ledger.rs`** — préimage + feuille de Merkle lient `pq_public_key` ; `build_signed_tx_at` signe via le
  primaire ; **`verify_tx` réécrit** (ML-DSA + Ed25519 stricts, **fallback retiré**) ; `pq_bindings_before` +
  `binding_violations` (registre pur) ; **validation** (`validate_block_against_prev`, les 2 chemins) **rejette**
  et **seal** (`seal_block_at`) **exclut** les clés non liées.
- **`sm/node.rs` + `sm/sim.rs`** — `seeded_identity` établit le **primaire** depuis une graine **domaine-séparée**
  (reproductible C1, structurellement indépendante de la graine Ed25519).
- **`lib.rs` (production) + `security/pq_vault.rs`** — `create_identity`/`unlock_identity` **câblent** le vault PQ :
  graine racine ML-DSA chiffrée, persistée dans `state_snapshots` (KV générique — **pas** de migration de schéma,
  **transparent au frontend**), restaurée à l'unlock (TOFU si absente pour une identité héritée). Les
  `#[allow(dead_code)]` de `create_pq_identity`/`unlock_pq_identity` **supprimés** (réellement appelés en prod).

### Les dents (§4) — `pqmig3_*` (5 tests)
- **`pqmig3_unbound_key_rejected_closes_crypto_id_1`** — **LE** test : un attaquant détenant la clé Ed25519 de
  la victime (rupture simulée) mais attachant **sa propre** clé ML-DSA valide ⇒ `verify_tx` **passe** (les deux
  signatures sont valides — c'est toute la prémisse) **mais** la **liaison mord** (clé ≠ clé liée) ⇒ rejeté par
  `binding_violations` **et** par `validate_block_against_prev`. CRYPTO-ID-1 **fermé**.
- substitution de clé rejetée · signature ML-DSA invalide rejetée · **Ed25519-seul rejeté** (fallback mort) ·
  chemin nominal accepté + intégré.

### Porte d'acceptation (toutes vertes)
- `cargo test --lib` : **333 passés / 0 échec** (328 + 5 dents) — couverture/conservation/sweep **non régressés**.
- `cargo clippy --lib -- -D warnings` **propre** · **C1 vert** · **sweep par défaut vert** (12 `t0_8_*`).
  *(Note : `clippy --all-targets` signale 1 `result_large_err` **préexistant** sur `(u64, sm::sim::Violation)`
  — harnais DST, `sim.rs:601`, **hors périmètre** PQ-MIG-3 ; la porte est `--lib`, propre.)*
- **Diff logique seule** ; `dispatcher.rs` **intact** ; `#[allow(dead_code)]` du vault PQ **supprimé** (câblé prod).

### Auto-revue (§3)
- **fermeture CRYPTO-ID-1** — *sound, prouvé* : liaison **immuable** d'une clé ML-DSA **indépendante** ; le test
  des dents montre qu'une clé non liée est rejetée **même quand la crypto statique passe** (la vraie attaque).
- **§4 respecté** — *escaladé, non deviné* : l'incohérence compte-unifié vs découpage 3/4 a déclenché un **STOP**
  et une décision d'Alexandre ([[ADR-008]]), pas un choix unilatéral.
- **honnêteté (§2 « ne survends jamais »)** — *assumée* : **astérisque permanent** documenté — ce n'est **pas**
  « comptes entièrement post-quantiques » ; `from` reste Ed25519, qui co-signe et **bootstrappe** la 1ʳᵉ liaison ;
  un compte jamais lié avant la rupture reste vulnérable (limite intrinsèque d'une migration PQ).
- **déterminisme** — *sound* : vérification pure ; signage sim déterministe (SIGN-DET) ; primaire sim domaine-séparé ;
  ML-DSA hors fingerprint ⇒ **C1 byte-identique** (vérifié).
- **périmètre** — *tenu* : enjeu/minage/validateur/genèse **inchangés** ; registre = **fonction pure de la chaîne**,
  zéro état persistant ajouté ; transport Ed25519 **non touché** (§4).
- **pas de masquage** — *honnête* : `verify_tx` durci (pas adouci) ; les dents **mordent** ; aucun test affaibli.

> **Bilan PQ-MIG-3** : la **racine d'autorité des comptes est post-quantique** (liaison ML-DSA indépendante,
> immuable) et **CRYPTO-ID-1 est close** — avec l'**astérisque** d'ADR-008 (voie (a), `from` Ed25519). Pièce
> suivante **PQ-MIG-4** : aligner l'enjeu/validateur sur la **clé de vote de finalité** ML-DSA → **GADGET-3
> débloqué** (n.b. : le re-`from` des tx d'enjeu **n'a pas lieu** ici — il n'a jamais été fait). Puis PQ-MIG-5
> (genèse PQ) ; et, si l'astérisque doit tomber un jour, le **re-adressage complet** ADR-007(b) (PQ-MIG-2 est
> posé pour ça). Commit baseline git = **manuel, Alexandre**.

---

## PQ-MIG-3B — `from`/`to` = adresse ML-DSA **partout** (termine le tout-PQ, lève l'astérisque) — [[ADR-008]] reversé

> **ADR-007 (b) réalisé, sans astérisque.** PQ-MIG-3 avait fait la voie (a) (`from` reste Ed25519 + clé liée,
> astérisque ADR-008). PQ-MIG-3B **complète** : `from`/`to` deviennent l'**adresse ML-DSA** (PQ-MIG-2,
> `BLAKE3(ADDR_DOMAIN ‖ clé)`), ce qui re-clé **du même geste** solde, récompense, enjeu, validateur et
> `@pseudo`. L'**identité unifiée** — que PQ-MIG-3/ADR-008 lisait comme un blocage — est en fait ce qui rend (b)
> une **unique bascule cohérente** : « tout bascule ensemble, c'est voulu ». **Transport Ed25519 différé** (§4,
> non touché). **Diff logique seule, déterministe.**

### Livrables
- **`p2p/ledger.rs`** — **`verify_tx` modèle (b)** : autorité **pur ML-DSA** = `lie(from, clé)` (la clé révélée
  doit hasher vers `from`) **+** signature ML-DSA valide ; le **co-facteur Ed25519 quitte le chemin d'autorité**
  (vestigial, wire-compat, simple présence vérifiée). CRYPTO-ID-1 fermé **intrinsèquement** (sans état :
  une clé ≠ ⇒ un `from` ≠) ; le registre de liaison de PQ-MIG-3 **conservé** comme filet redondant.
- **`p2p/mining_loop.rs`** (production) — **split valeur/transport** : `mine_tx` crédite l'**adresse** ; CRDT
  miroir = adresse ; `pos_seal_if_leader`/`seal_and_broadcast` reçoivent `(addr, pk)` — **élection + seal** sur
  l'**adresse** (le set de validateurs est indexé par adresse via `validator_stakes()`), **enveloppe** signée par
  le `pk` **transport** Ed25519. La réputation/Shapley reste en espace **transport** (`uptime_tick(&pk, …)`,
  hors chemin de sécurité, ADR-002).
- **`sm/node.rs`** (cœur) — `propose_block_at` : miner-reward + clé d'élection = `pq_address_hex()` (valeur) ;
  `sign_envelope` garde le `pk` transport. **`src/sm/` sans-IO** : seules des dérivations **pures** BLAKE3 ajoutées.
- **`lib.rs`** (commandes) — `ledger_transfer` : `from` = adresse (tx + miroir CRDT), enveloppe = `transport_pk` ;
  `get_public_key` expose l'**adresse** (identité wallet : solde / adresse de réception / `@pseudo`).
- **`p2p/username.rs` + `commands_v3.rs`** — **@pseudo re-clé** : `UsernameRecord.owner_pk` = **adresse**,
  champ **`owner_key`** (clé ML-DSA révélée), signature **ML-DSA** ; `verify_sig` = `lie(owner_pk, owner_key)` +
  `verify_pq` ; `connection_code` dérivé de l'adresse (cohérent avec `verify_connection`). `claim_username`
  signe en ML-DSA. **`dispatcher.rs` intact** (serde absorbe le champ).

### Les dents (§3)
- **faille toujours fermée** — `pqmig3_unbound_key_rejected_closes_crypto_id_1` (réécrit modèle b) + nominal
  `pqmig3b_nominal_address_tx_accepted` (`from`=adresse ⇒ `verify_tx` Ok(true), red→green : l'ancien `verify_tx`
  rejetait l'adresse faute de signature Ed25519 valide) ; **@pseudo** : `rejects_unbound_key_closes_pseudo_hijack`
  (revendiquer l'adresse d'autrui avec sa propre clé ⇒ `lie` faux ⇒ rejet) + `rejects_owner_mismatch` (ML-DSA).
- **enjeu re-clé** — `staked`/`validator_stakes()` indexés par adresse (tx `Stake` `from`=adresse) ; tests stake
  ledger + `onchain_stake_harness_conservation_counts_locked_stake` (sim) verts sur identité ML-DSA.
- **récompense re-clé** — `mine_tx` crédite l'adresse ; soldes lus sous l'adresse (sm/sim/ledger).
- **conservation/couverture** — cycle mine → transfer → stake conserve ; COVER-1/2 + `Σ(…)+brûlé==miné` verts.

### Porte d'acceptation (toutes vertes)
- `cargo test --lib` : **335 passés / 0 échec** — couverture/conservation/sweep **non régressés** ; +1 dent
  `@pseudo`. **C1 vert** (`determinism_meta_test_128_runs_are_byte_identical`) — l'adresse est une fonction pure.
- `cargo clippy --lib -- -D warnings` **propre**.
  *(Note : `clippy --tests` signale 1 `result_large_err` **préexistant** sur `(u64, sm::sim::Violation)` — harnais
  DST, **hors périmètre**, `0` occurrence dans le diff PQ-MIG-3B ; la porte est `--lib`, propre.)*
- **Diff logique seule** ; `dispatcher.rs` **intact** ; **transport Ed25519 inchangé** (enveloppes / PeerId / sign).
- **ADR-008 reversé/réécrit** (rétablit ADR-007 (b)) ; README ADR à jour ; entrée tracker + auto-revue §3.

### Auto-revue (§3)
- **faille fermée — *renforcée*** : la liaison passe d'un **état** (registre) à une **fonction sans état**
  (`from == BLAKE3(ADDR_DOMAIN ‖ clé)`). La dent nominale est le vrai red→green (l'ancien code **rejetait**
  l'adresse, le neuf l'**accepte** via `lie`) ⇒ le test prouve la bascule, pas une tautologie.
- **valeur vs transport — *séparation nette, non devinée*** : tout ce qui est **valeur** (solde, récompense,
  enjeu, validateur, `from`/`to`, `@pseudo`, wallet) = **adresse** ; tout ce qui est **transport** (enveloppe
  gossip, PeerId, `signable_envelope_bytes`) = **Ed25519**. La réputation reste en espace transport (cohérent
  avec `peer_info`/`uptime_tick`) — pas de mélange furtif.
- **déterminisme — *sound*** : adresse = BLAKE3 pure ; signage ML-DSA sim déterministe ; **C1 byte-identique**
  re-vérifié. `src/sm/` reste **sans-IO** (aucune horloge/`OsRng`/ordre de `HashMap` introduits).
- **périmètre — *tenu*** : transport **non touché** (§4) ; `dispatcher.rs` intact ; le re-`from` ne change que la
  **clé d'indexation**, jamais la **valeur** (conservation/couverture invariantes).
- **honnêteté (§2) — *l'astérisque tombe*** : ce **n'est plus** « entièrement PQ\* » mais **entièrement PQ** au
  niveau du **compte** ; la seule couche encore Ed25519 (le **transport**) est **explicitement** différée et
  **hors chemin de valeur** — déclaré, pas masqué.
- **pas de masquage — *honnête*** : `verify_tx` durci (autorité = ML-DSA seul) ; les dents **mordent** (binding
  + signature, côté tx **et** côté `@pseudo`) ; le test de typestate corrige une **fausse-dent** (tamponner la
  signature Ed25519 ne mordait plus en modèle (b) — on tamponne désormais la signature **ML-DSA**, l'autorité réelle).

> **Bilan PQ-MIG-3B** : l'**identité de compte est entièrement ML-DSA** (solde, récompense, enjeu, validateur,
> `@pseudo`), **sans astérisque** ; l'autorité est **pur ML-DSA + `lie`**, CRYPTO-ID-1 close **par construction**.
> Identité d'enjeu = identité de finalité ⇒ **GADGET-3 débloqué**. Restent **PQ-MIG-5** (genèse PQ) et, si un
> jour voulu, le **transport** PQ (la dernière couche Ed25519). Commit = **manuel, Alexandre**.

---

## GADGET-3 — la règle justifier/finaliser (la finalité devient RÉELLE)
*(2026-06-25 · `/goal QUANTA_GADGET_PIECE3.md` · pièce 3 du gadget, §4/§11/§14 de DESIGN-FINALITY-GADGET ; débloquée par PQ-MIG-3B)*

> On applique la **règle en deux temps** (Casper FFG) qui **consomme** les certificats de GADGET-2 :
> un point de contrôle devient **justifié** par un lien super-majoritaire depuis un point déjà justifié,
> puis **finalisé** quand son **enfant direct** est justifié par un lien partant de lui (deux époques
> consécutives liées). **`FinalitySafety` cesse d'être vacueux** : l'ensemble finalisé **grandit
> vraiment** au-delà de la genèse. **Ni** fork-choice (GADGET-5) **ni** slashing (GADGET-4) ici.
> Diff logique seule, **règle pure**, **C1 vert**.

### Livrables (1 fichier neuf + 3 retouches logiques)
- **`src-tauri/src/sm/finality_rule.rs`** (neuf) : **§1** `JustifiedSet` (init `{genèse}`, `is_justified`
  = égalité exacte époque+hash) ; **§2** `FinalityState` (justifié + finalisé) + **`apply_certificate`**
  (la règle en deux temps, pure de `(certificat, enjeu, E)`) ; `StepOutcome` (ce que le pas a avancé) +
  8 tests (§4).
- **`src-tauri/src/sm/node.rs`** (retouche) : champ `finalized: FinalizedSet` → **`finality: FinalityState`**
  (justifié **et** finalisé, init genèse) ; `finalized()` inchangé (renvoie `self.finality.finalized()`,
  **harnais intact**) ; nouveaux `justified()` + **`apply_finality_certificate`** (le **vrai** chemin
  d'enregistrement que GADGET-1 réservait) ; `record_finalized_for_test` re-câblé sur `FinalityState`.
- **`src-tauri/src/sm/mod.rs`** (+2 l.) : `pub mod finality_rule;` + re-export `FinalityState/JustifiedSet/StepOutcome`.
- **`src-tauri/src/sm/sim.rs`** (+1 test) : `gadget_3_finality_safety_guards_real_finalized_checkpoints`.

### §1/§2 — la règle (deux temps)
- **Justifier** : recevant un certificat **valide** (⅔, GADGET-2) pour `source → cible`, **si `source`
  est justifié**, alors `cible` devient justifiée (idempotent, ≤1 par époque).
- **Finaliser** : si en plus `cible` est l'**enfant direct** (`cible.epoch == source.epoch + 1`), alors
  `source` devient finalisé. **Append-only** (jamais d'écrasement d'une époque déjà finalisée —
  irréversibilité). Un certificat **sous le quorum** *ou* une `source` **non justifiée** ⇒ **rien**.
- **Pure** : aucune horloge, aucune entropie, aucun ordre de `HashMap` dans le verdict (`BTreeMap`
  époque-ordonné). L'enjeu est **fourni par l'appelant** : sa **provenance** (re-keyer
  `validator_stakes()` vers l'identité de vote) est la **réconciliation §4 de GADGET-2**, *non* tranchée
  ici, aucun chemin gossip live câblé (comme GADGET-2).

### §4 — les dents (mordent ; surtout « deux temps pas un »)
- `gadget3_quorum_link_from_justified_source_justifies_target` : ⅔ depuis source justifiée ⇒ **cible justifiée**.
- `gadget3_unjustified_source_justifies_nothing` : source **non** justifiée ⇒ `StepOutcome::default` (**rien**).
- `gadget3_subquorum_certificate_justifies_nothing` : 1/3 < ⅔ ⇒ **rien justifié**.
- **`gadget3_justified_alone_is_not_finalized_two_step_not_one`** (le cœur) : `c1` justifié seul **n'est
  PAS finalisé** ; il l'est **seulement** au second lien `c1→c2` (son enfant direct). Un seul temps ne finalise pas.
- `gadget3_skip_link_justifies_but_never_finalizes` : lien à trou `g(0)→c2(2)` justifie `c2` mais **ne finalise rien**
  (époques non consécutives) — renforce « deux temps pas un ».
- `gadget3_honest_path_finalizes_expected_checkpoints` : suite `g→c1→c2→c3` finalise **`g, c1, c2`** (`c3`
  justifié mais pas finalisé — pas de lien vers son enfant). L'ensemble finalisé **grandit** (compte = 3).
- `gadget3_honest_rule_finalizes_no_conflicting_checkpoints` : un certificat **conflictuel** à l'époque 1
  (source ≠ `c1` justifié) est **inerte** ⇒ l'époque 1 **reste** finalisée à `c1` (aucun conflit honnête).
- `gadget3_rule_is_deterministic_across_nodes` : mêmes certificats + même enjeu ⇒ **`FinalityState` byte-identique** sur deux nœuds.
- **Harnais** `gadget_3_finality_safety_guards_real_finalized_checkpoints` : deux nœuds appliquent la **même**
  chaîne honnête via `apply_finality_certificate` ⇒ finalisent **`{g, c1, c2}` réels** (compte = 3, > genèse) ⇒
  `check_invariants()` **vert** : `FinalitySafety` garde enfin de **vrais** points. La **violation plantée de
  GADGET-1** (`gadget_1_finality_safety_invariant_has_teeth`) **mord toujours** ⇒ ni vacueux ni tampon de caoutchouc.

### Portes d'acceptation — toutes vertes
- `cargo test --lib` : **344 / 0** (335 + 9 GADGET-3 : 8 `finality_rule` + 1 harnais), incl. les dents §4
  (surtout « deux temps pas un » + `FinalitySafety` réel).
- `cargo clippy --lib -- -D warnings` **propre** · **C1 vert** (`determinism_meta_test_128…`) ·
  **`t0_8_sweep_catches_planted_violation` / `t0_8_conservation_under_burn` / `t0_8_coverage_transfers_and_burns` verts** ·
  `src/sm/` **sans-IO** (règle = calcul pur, zéro horloge/entropie/IO).
- **Diff logique seule** : **4 fichiers** (1 neuf + 3 retouches) ; `dispatcher.rs` **intact** (0 réf) ;
  **test à violation plantée de GADGET-1 toujours vert**.

### Défauts marqués (réglables, hérités GADGET-1/2, ADR-005/§12)
- **E = `EPOCH_LENGTH_BLOCKS`** (32 provisoire), paramétrique. **Quorum = ⅔**, **comité = validateurs actifs** (GADGET-2).

### Auto-revue — vérification adverse GADGET-3 (5 lentilles)
- **deux temps pas un (le cœur)** — *sound* : la finalisation est gardée par `cible.epoch ==
  source.epoch+1` **ET** `source` justifié ; un point justifié seul, ou justifié par un **lien à trou**,
  n'est **jamais** finalisé (deux dents distinctes le prouvent par contre-factuel). Pas de raccourci un-temps.
- **pas de justification frauduleuse** — *sound* : `is_valid` (⅔, GADGET-2) **et** `is_justified(source)`
  sont des **préconditions dures** ; sous-quorum *ou* source inconnue ⇒ `StepOutcome::default`, état inchangé.
- **sûreté / pas de conflit finalisé** — *sound* : finalisation **append-only** (jamais d'écrasement) +
  `is_justified` exige l'**égalité de hash** (un hash différent à l'époque = conflit, pas correspondance) ⇒
  un certificat conflictuel est inerte. Le harnais confirme l'accord inter-nœuds sur des points **réels** ;
  la violation **plantée** de GADGET-1 mord toujours. *(La preuve qu'on **ne peut pas** forger un conflit
  sans recouvrement ⅔ est le **théorème de slashing = GADGET-4**, hors périmètre — §4 STOP respecté.)*
- **déterminisme** — *sound* : règle pure ; `BTreeMap` époque-ordonné (aucun ordre `HashMap` dans le
  verdict) ; signage ML-DSA sim déterministe (SIGN-DET) ; **C1 byte-identique** re-vérifié ; `sm/` sans-IO.
- **périmètre — *tenu*** : **ni** fork-choice **ni** slashing ; on **réutilise** GADGET-1 (`FinalizedSet`,
  points de contrôle) et GADGET-2 (trait `FinalityCertificate`) sans les redéfinir ; l'enjeu reste **fourni**
  (réconciliation d'identité = §4 GADGET-2, non tranchée). `dispatcher.rs` intact.

> **Bilan GADGET-3** : la finalité **vit**. La règle en deux temps **consomme** les certificats de
> GADGET-2 ; l'ensemble finalisé **grandit vraiment** (`{g, c1, c2}` dans le harnais) et `FinalitySafety`
> garde enfin de **vrais** points — la dent plantée de GADGET-1 mord toujours, donc l'invariant n'est ni
> vacueux ni décoratif. **« Deux temps pas un »** est verrouillé par deux dents. Restent **GADGET-4**
> (slashing : les deux conditions, d'où *découle* la sûreté), **GADGET-5** (fork-choice conscient de la
> finalité) et **PQ-MIG-5** (genèse PQ). La réconciliation enjeu↔identité-de-vote (GADGET-2 §4) reste à
> trancher avant le câblage live. Commit = **manuel, Alexandre**.

---

## REPUT-ID-1 — nettoyer le mix transport/adresse dans la réputation (hygiène, hors consensus)
*(2026-06-25 · `/goal QUANTA_REPUTATION_ID_NIT.md` · NIT de la revue adverse PQ-MIG-3B · run séparé, après GADGET-3)*

> Après le re-keying PQ-MIG-3B, le moteur de réputation **mélangeait** encore clé de **transport**
> (Ed25519) et **adresse** (ML-DSA) : le minage remplissait la réputation sous la clé transport
> (`uptime_tick(&pk,…)`) tandis qu'un transfert créditait le destinataire sous son **adresse** (`to`) —
> deux seaux qui ne se réconciliaient jamais. **Décision** (§1) : l'identité de réputation est l'**adresse
> ML-DSA** (l'**acteur économique**), pas la clé de transport éphémère. **Cosmétique** (réputation hors
> chemin de sécurité, ADR-002/STAKE-WEIGHT-1), mais l'incohérence est levée avant qu'elle ne morde.

### §1 — Audit (où transport vs adresse)
- **Moteur** `reputation.rs` : `users: HashMap<String, UserReputation>` **agnostique** à la clé — le mix
  vivait dans les **appelants**.
- **Mix constaté** : `mining_loop` `uptime_tick(&pk=transport)` remplit ; `lib.rs ledger_transfer`
  `transfer(&transport_pk, &to=adresse)` crédite (côtés **dépareillés**) ; commandes réputation
  (`get_my_reputation`/`transfer_atn`/`stake_atn`/`get_network_health`) lisaient par **transport** ;
  `gossip_tasks` (Hello) lisait l'uptime local par **transport**.
- **Hors mix (intrinsèquement transport, documenté)** : `peer_info` + map de contributions **Shapley**
  (les pairs sont des entités **réseau** identifiées par leur pubkey transport) ; l'**enveloppe** gossip.

### §2 — Re-clé sur l'adresse (identité cohérente) — 4 appelants, 1 doc
- **`mining_loop.rs`** : `uptime_tick(&addr, …)` (l'acteur local mine **sous son adresse**). `peer_contribs`
  reste transport-keyé (intrinsèque) ; Shapley somme des **valeurs**, jamais des identités ⇒ aucun mix.
- **`lib.rs ledger_transfer`** : `transfer(&from=adresse, &to=adresse)` — **les deux côtés** en adresse, donc
  un crédit reçu atterrit dans le **même** seau que celui que le destinataire mine. `transport_pk` ne sert
  plus qu'à l'**enveloppe** (inchangé).
- **`lib.rs`** commandes réputation (×4) : identité locale `public_key_hex` → **`pq_address_hex()`**.
- **`gossip_tasks.rs`** (Hello) : uptime local lu par **adresse** ; `pk` transport gardé pour `peer_info`.
- **`reputation.rs`** (doc seule) : contrat d'identité gravé sur `ReputationEngine` + `UserReputation.public_key`
  (= **adresse** ML-DSA ; nom de champ conservé pour la **compat snapshot/frontend**).

### §3 — zéro effet consensus (vérifié)
- La réputation **ne feed pas** l'élection : `mining_loop.rs:237` passe une map `reputations` **vide** à
  `build_validator_set` ; le poids = **enjeu on-chain seul** (`validator_stakes()`, STAKE-WEIGHT-1/ADR-002 ;
  `pos_consensus` : « Do not re-introduce any non-stake term »). **Aucun re-couplage** introduit.
- `pos_consensus.rs` `Validator.reputation` = champ d'**affichage**, sans rapport avec le `ReputationEngine`.

### Portes d'acceptation — toutes vertes
- `cargo test --lib` : **344 / 0** (inchangé — re-key d'appelants, aucun test cassé).
- `cargo clippy --lib -- -D warnings` **propre** · **C1 vert** (`determinism_meta_test_128…`) ·
  `t0_8_clean_default_sweep` / `t0_8_conservation_under_burn` / `t0_8_coverage_transfers_and_burns` **verts** ·
  `src/sm/` **sans-IO** (intouché — la réputation n'est pas dans le cœur déterministe).
- **Audit** : grep `reputation` × `public_key_hex|transport_pk|get_identity` dans
  `lib.rs`/`mining_loop.rs`/`gossip_tasks.rs` ⇒ **vide** (plus aucune clé transport ne feed la réputation).
- **Diff logique seule**, confinée à **`reputation.rs` + 3 appelants directs** ; `dispatcher.rs` **intact** ;
  consensus (poids/quorum/élection) **inchangé**.

### Auto-revue — vérification adverse REPUT-ID-1 (4 lentilles)
- **cohérence d'identité — *résolue*** : une **seule** notion (adresse ML-DSA) ; minage, transfert (2 côtés),
  commandes et Hello convergent ; le seul transport restant (peer-contribs Shapley, enveloppe) est
  **intrinsèque** et **documenté** (« sauf cas où le transport est la bonne clé », §1 du spec).
- **effet consensus — *nul*** : `build_validator_set` reçoit une map réputation **vide** ; rien re-couplé.
  Sweep/conservation/C1 verts ⇒ le cœur déterministe n'a pas bougé.
- **Shapley correct — *sound*** : re-keyer l'acteur local (adresse) dans une map de pairs (transport) ne
  change pas la **valeur** de sa part (Shapley = fonction des contributions, pas des clés) ; clés distinctes ⇒
  `shares.get(addr)` récupère bien la sienne.
- **migration / transitoire — *signalé, bénin*** : sur **mise à niveau** d'un nœud existant, l'entrée
  réputation transport-keyée d'un **snapshot antérieur** est **orpheline** ; le nœud ré-accumule sous son
  adresse (uptime repart de 0 ⇒ multiplicateur anti-sybil **localement** plus bas quelques ticks). **Sans
  effet consensus** (le multiplicateur est un throttle **local** ; l'émission reste bornée par
  `validate_block_emission`) et **sans perte de valeur** (la réputation n'est **pas** la monnaie — le solde
  réel vit dans le **ledger**, déjà adresse-keyé). Les commandes `*_atn` legacy sont désormais cohérentes
  (adresse) bien que vestigiales (retrait = travail futur, hors périmètre).

> **Bilan REPUT-ID-1** : le mix transport/adresse est **levé** — la réputation a **une** identité,
> l'**adresse ML-DSA** (l'acteur économique), cohérente avec le ledger et le minage ; le transport ne
> subsiste que là où il est **intrinsèque** (pairs réseau, enveloppe), **documenté**. **Zéro** effet
> consensus (map réputation vide vers l'élection), C1/sweep/conservation verts, périmètre **strict**
> (réputation + 3 appelants). Pur nettoyage d'hygiène. Commit = **manuel, Alexandre**.

---

## GADGET-4 — slashing : détecter les **deux** fautes (la sûreté devient RESPONSABLE)
*(2026-06-25 · `/goal QUANTA_GADGET_PIECE4.md` · pièce 4 du gadget, §7 de DESIGN-FINALITY-GADGET ; [[ADR-003 — Slashing]] ; bâtie sur GADGET-2 (votes) + GADGET-3 (finalité))*

> GADGET-3 a rendu la finalité **réelle** ; il ne l'a pas rendue **responsable**. Le théorème de
> sûreté responsable dit : si **deux** points de contrôle en **conflit** sont finalisés, alors des
> validateurs détenant **≥ ⅓** de l'enjeu ont enfreint l'une de **deux** règles — et **leurs propres
> votes signés le prouvent**. On le rend **exécutable** : **détection** des deux conditions +
> **preuve** non répudiable (signatures ML-DSA) + **mécanique** de pénalité (enjeu réduit, slashé
> **brûlé** par défaut, conservation préservée). On **n'invente pas** de règles : elles *découlent* du
> théorème. **Pas** de fork-choice (GADGET-5) — la sûreté responsable n'en a pas besoin (§7 STOP).
> Diff logique seule, **tout pur**, **C1 vert**.

### Livrables (1 fichier neuf + 1 retouche d'export + 1 test harnais)
- **`src-tauri/src/sm/finality_slashing.rs`** (neuf) : **§1** `Fault` (DoubleVote/Surround) +
  **`detect_fault`** (pur, structurel) ; **§2** `FaultProof` + **`verify_proof`** (sigs ML-DSA +
  même validateur + condition de faute) ; **§3** `SlashOutcome` + **`apply_slash`** /
  **`slash_for_proof`** (vérifier-puis-slasher, conservant) ; **§4** **`slashable_weight`** (la mesure
  ≥ ⅓) ; constantes **🛑 marquées** ; 12 tests (§5).
- **`src-tauri/src/sm/mod.rs`** (+5 l.) : `pub mod finality_slashing;` + re-export
  `Fault/FaultProof/SlashOutcome/detect_fault/verify_proof/apply_slash/slash_for_proof/slashable_weight`.
- **`src-tauri/src/sm/sim.rs`** (+1 test) : `gadget_4_accountable_safety_finalized_conflict_leaves_a_third_slashable`.

### §1/§2/§3 — détection, preuve, pénalité (tout pur, réutilise GADGET-2/3)
- **§1 détection** : `detect_fault(a,b)` — **DoubleVote** si `cible.epoch(a)==cible.epoch(b)` et liens
  distincts ; **Surround** si un intervalle `(source,cible)` en **entoure** strictement un autre
  (source antérieure **et** cible postérieure, dans un sens ou l'autre) ; sinon `None`. **Structurel**
  (n'examine **pas** l'identité ni les sigs — c'est le rôle de `verify_proof`) ⇒ un détecteur scanne
  les paires candidates à bas coût, puis prouve.
- **§2 preuve** : `FaultProof = (vote_a, vote_b)`. `verify_proof` = **même** validateur **ET** chaque
  vote individuellement valide (lien bien-formé + validateur **actif** + **sig ML-DSA** valide, via
  `Vote::verify` de GADGET-2) **ET** `detect_fault ≠ None`. **Pas de fausse accusation** : sig forgée,
  paire inter-validateurs, ou votes **légaux** ⇒ faux.
- **§3 pénalité** : `apply_slash` réduit l'enjeu de `SLASH_NUM/SLASH_DEN` ; le slashé est **brûlé**
  (`SLASH_BURN`). **Conservation structurelle** : `stake_before == remaining + slashed` et
  `burned == slashed` — rien créé, rien perdu. `slash_for_proof` = **vérifier-puis-slasher** (jamais
  slasher sur une accusation non prouvée). **Pures**, déterministes (`BTreeMap`/`BTreeSet`, entiers
  `checked`, aucun ordre `HashMap` dans le verdict).
- **§4 mesure** : `slashable_weight(votes, enjeu, E)` = somme de l'enjeu des validateurs **distincts**
  dont les votes **prouvent** une faute (chacun compté **une** fois) — la quantité que le théorème
  borne à **≥ ⅓**.

### §4/§5 — les dents (mordent ; surtout la sûreté responsable ≥ ⅓)
- `gadget4_double_vote_is_detected_and_proven` · `gadget4_surround_is_detected_and_proven` :
  détectées **et** prouvées (sig ML-DSA valides du même validateur).
- `gadget4_legal_votes_same_source_are_not_a_fault` · `gadget4_legal_chain_extension_is_not_a_fault` ·
  `gadget4_same_vote_twice_is_not_a_fault` : **pas de fausse accusation** (même source, époques
  différentes sans entourage, ou re-publication) ⇒ `detect_fault = None`, `verify_proof = faux`.
- `gadget4_forged_proof_is_rejected` : une preuve dont une sig ML-DSA est **forgée** (octet flippé) ⇒
  rejetée. `gadget4_cross_validator_pair_is_not_a_proof` : deux **validateurs distincts** ⇒ pas la
  faute d'**un** validateur ⇒ rejetée (le garde d'identité de `verify_proof`).
- `gadget4_slash_burns_and_conserves` · `gadget4_unproven_accusation_slashes_nothing` : une preuve
  valide **réduit** l'enjeu, le slashé est **brûlé**, **conservation** (`remaining+slashed==before`) ;
  une accusation non prouvée laisse l'enjeu **intact**.
- `gadget4_slashable_weight_covers_at_least_one_third` · `gadget4_honest_votes_leave_nothing_slashable`
  (pur) + **harnais** `gadget_4_accountable_safety_finalized_conflict_leaves_a_third_slashable` :
  **comité byzantin** — `{1,2}` finalisent le fork A sur le nœud A, `{2,3}` le fork B sur B (via la
  règle GADGET-3) ⇒ **(i)** `check_invariants()` lève **`FinalitySafety`** à l'époque 1 (finalité
  **rompue, observable**), **(ii)** le validateur **2** (intersection des deux quorums ⅔, ⅔+⅔−1=⅓) a
  **double-voté** ⇒ `slashable_weight = 100 = ⅓` de 300, `slashable·3 ≥ total`. **Casser la finalité
  laisse une preuve d'au moins un tiers fautif.**
- `gadget4_detection_and_penalty_are_deterministic` : pipeline complet (votes→détection→preuve→slash→
  mesure) **byte-identique** sur deux constructions (SIGN-DET + verdicts entiers purs).

### Portes d'acceptation — toutes vertes
- `cargo test --lib` : **357 / 0** (344 + 13 GADGET-4 : 12 `finality_slashing` + 1 harnais), incl. les
  dents §5 (surtout **sûreté responsable ≥ ⅓** et **pas de fausse accusation**).
- `cargo clippy --lib -- -D warnings` **propre** · **C1 vert** (`determinism_meta_test_128…`) ·
  **`t0_8_sweep_catches_planted_violation` / `t0_8_conservation_under_burn` /
  `t0_8_coverage_transfers_and_burns` verts** · `src/sm/` **sans-IO** (détection/preuve/pénalité =
  calcul pur, zéro horloge/entropie/IO ; SIGN-DET reste `#[cfg(test)]`).
- **Diff logique seule** : **3 fichiers** (1 neuf + export + 1 test) ; `dispatcher.rs` **intact**
  (0 réf GADGET-4) ; **invariants de finalité GADGET-1/3 toujours verts** (la violation **plantée** de
  GADGET-1 mord toujours).

### Défauts marqués (🛑 Alexandre, §12 / ADR-003 — constantes réglables)
- **`SLASH_NUM/SLASH_DEN`** = **1/1** (slash **plein** par défaut : l'équivocation qui rompt la
  finalité est la faute la plus grave ⇒ dissuasion maximale, la plus simple ; alternative = slash
  partiel/corrélé). **`SLASH_BURN`** = **true** (brûlé : le plus simple et le plus sain
  monétairement ; conservation tient aussi en redistribution). **`SLASH_EVIDENCE_WINDOW_BLOCKS`** =
  `UNBONDING_PERIOD_BLOCKS` (le **maximum** : une preuve doit arriver **avant** que le fautif ne
  déverrouille et retire son enjeu ; contrainte `≤ unbonding` **gravée** par un `const _: () =
  assert!(…)` ⇒ erreur de **compilation** si retunée au-delà — la faille *unstake-and-run* d'ONCHAIN-STAKE-1 §3).

### Auto-revue — vérification adverse GADGET-4 (5 lentilles)
- **détection (les deux, et deux seulement) — *sound*** : `Fault` est un enum **clos** ; DoubleVote
  (même époque-cible, liens distincts) et Surround (entourage **strict** des deux côtés) sont les
  conditions de §7 ; tout le reste (extension de chaîne, saut, re-publication) ⇒ `None` (trois dents
  par contre-factuel). Inventer une 3ᵉ condition punirait l'honnête — *non fait*.
- **preuve (anti-fausse-accusation) — *sound*** : `verify_proof` exige **même validateur** ∧ **deux
  sigs ML-DSA valides** (`Vote::verify`, GADGET-2) ∧ une faute structurelle ; sig forgée, paire
  inter-validateurs, ou votes légaux ⇒ **faux** (dents dédiées). La preuve est **vérifiable par
  quiconque** (sigs non répudiables), aucune partie de confiance.
- **pénalité (conservation) — *sound*** : `apply_slash` est **fraction-générique** et **checked**
  (`u128`, clamp ≤ enjeu) ; `stake_before == remaining + slashed` et `burned == slashed` tiennent pour
  **toute** fraction marquée ; `slash_for_proof` ne slashe **que** sur preuve vérifiée ; un fautif
  pleinement slashé **quitte** l'ensemble actif. *(Le câblage sur l'**enjeu on-chain réel** — un
  mouvement `STAKE → BURN` qui ferait mordre l'invariant de conservation du harnais via le vrai
  `locked_stake`/`burned` — est la **même** réconciliation identité/ledger différée que GADGET-2 §4,
  laissée à Alexandre ; aucun chemin gossip live, comme GADGET-2/3.)*
- **sûreté responsable ≥ ⅓ — *prouvée*** : le harnais **injecte** une finalisation conflictuelle (deux
  quorums ⅔), `FinalitySafety` **mord** (finalité rompue, observable), et `slashable_weight` récupère
  **exactement** l'intersection (≥ ⅓, ici 100/300) via le double-vote du validateur partagé. Théorème
  rendu exécutable : **casser la finalité laisse une preuve d'au moins un tiers**.
- **déterminisme / périmètre — *tenu*** : tout pur (`BTreeMap`/`BTreeSet`, entiers), aucun ordre
  `HashMap` dans le verdict ; SIGN-DET `#[cfg(test)]` ; **C1 byte-identique** re-vérifié ; `sm/`
  sans-IO. **Réutilise** GADGET-2 (`Vote`/`Vote::verify`) et GADGET-3 (`apply_finality_certificate`)
  sans les redéfinir ; **ni** fork-choice **ni** ledger live touchés ; `dispatcher.rs` intact.

> **Bilan GADGET-4** : la sûreté est **responsable**. Les **deux** fautes — et deux seulement — sont
> détectées sur les votes ML-DSA, la **preuve** est non répudiable et vérifiable par quiconque, la
> **pénalité** réduit l'enjeu en conservant le bilan (slashé **brûlé**). Le harnais prouve le théorème :
> une finalité **rompue** (`FinalitySafety` mord) laisse une preuve couvrant **≥ ⅓** de l'enjeu. Restent
> **GADGET-5** (fork-choice conscient de la finalité, résout la partition), **PQ-MIG-5** (genèse PQ), et
> la réconciliation enjeu↔identité-de-vote + le **câblage live** du slashing sur le ledger (mouvement
> `STAKE→BURN`, nouvelle tx/gossip) — avant lesquels les **montants §3 (🛑)** sont à figer au §12. Commit
> = **manuel, Alexandre**.

---

## GADGET-5A — fork-choice LMD-GHOST conscient de la finalité (le moteur seul)
*(2026-06-25 · `/goal QUANTA_GADGET_PIECE5A.md` · pièce 5A du gadget, §9 de DESIGN-FINALITY-GADGET ; bâtie sur GADGET-2 (votes) + GADGET-3 (justifié/finalisé) + enjeu on-chain)*

> La plus-longue-chaîne est la mauvaise règle pour une chaîne votée par l'enjeu : elle ignore **qui** a
> soutenu une branche. GHOST suit, à chaque embranchement, l'enfant au **plus de poids de votes** —
> rendu **conscient de la finalité** (façon Gasper) : **ancré au dernier point justifié** (GADGET-3) et
> **plancher au dernier finalisé**, il ne peut **jamais** défaire l'histoire irréversible. Cette pièce
> est le **moteur seul** ; la **résolution de partition** (bascule du test `…gadget_deferred`) est
> **GADGET-5B**. Diff logique seule, **tout pur**, **C1 vert**. Pas de slashing vivant ici (§9 STOP).

### Livrables (1 fichier neuf + 1 retouche d'export)
- **`src-tauri/src/sm/fork_choice.rs`** (neuf) : **§1** `LatestVotes` (LMD **ordre-indépendant**,
  réutilise `Vote` de GADGET-2) ; `BlockTree` (substrat parent/enfant, `is_descendant` **borné** =
  anti-boucle) ; **§2** `branch_weights` (poids = Σ enjeu des derniers votes qui **descendent** du
  bloc, privé) ; **§3** **`ghost_head`** (part du justifié, descend l'enfant de plus grand poids,
  départage **plus petit hash**, **plancher de finalité absolu** + repli) + **`anchors`** (extrait
  `(dernier justifié, dernier finalisé)` d'un `FinalityState` de GADGET-3) ; 9 tests (§4).
- **`src-tauri/src/sm/mod.rs`** (+2 l.) : `pub mod fork_choice;` + re-export
  `BlockTree/LatestVotes/ghost_head/anchors`.

### §1/§2/§3 — LMD, poids de branche, règle GHOST ancrée
- **§1 LMD** : `LatestVotes::observe` ne garde que le **dernier** vote par validateur (clé = **adresse
  ML-DSA**, cohérente depuis le re-keying) — remplace ssi **époque cible strictement supérieure**,
  départage d'égalité sur le **plus petit hash cible** ⇒ **ordre-indépendant** (deux nœuds, même
  ensemble de votes dans n'importe quel ordre ⇒ même état).
- **§2 poids** : `branch_weights` attribue l'enjeu de chaque dernier vote à sa **cible et tous ses
  ancêtres jusqu'à l'ancre** ; un vote hors du sous-arbre ancré, ou d'un validateur à enjeu 0, **ne
  pèse rien**. Pur, sommes entières commutatives (`BTreeMap`, aucun ordre `HashMap` dans le verdict).
- **§3 GHOST ancré** : `ghost_head` part de l'**ancre justifiée**, descend à l'enfant de **plus grand
  poids** (départage **plus petit hash**, `BTreeSet` trié) jusqu'à une feuille. **Plancher de finalité
  absolu** : le résultat **descend toujours** du finalisé ; ancre inconnue ou hors-plancher ⇒ **repli
  sur le plancher** (jamais sous la finalité).

### §4 — les dents (mordent ; surtout poids-prime-longueur et plancher)
- **`gadget5a_weight_beats_length`** (le cœur) : branche **courte mais lourde** `R→X` (200) **bat** la
  branche **longue mais légère** `R→Y1→Y2→Y3` (100) — le poids décide à l'**embranchement**, pas la
  longueur. `gadget5a_branch_weight_sums_supporting_stake` : §2 direct (poids = Σ enjeu, propagé à la racine).
- **`gadget5a_latest_vote_replaces_old_one`** : `a` vote X puis re-vote Y (époque ↑) ⇒ l'ancien vote ne
  compte plus (poids(X)=0), la tête **bascule** vers Y. `gadget5a_stale_vote_does_not_replace` : un vote
  d'époque **inférieure** est ignoré.
- **`gadget5a_finality_floor_is_absolute`** : une branche **hors-plancher** `Z→C` **trois fois plus
  votée** est **ignorée** ; la tête reste sur la branche finalisée (`H`, descend de `F`), jamais `C`.
  `gadget5a_anchor_off_floor_falls_back_to_floor` : ancre hors-plancher / inconnue ⇒ **repli sur F**.
- **`gadget5a_anchors_track_last_justified_not_genesis`** : `anchors` d'un état genèse = `(genèse,
  genèse)` ; après un certificat ⅔ `g→c1` (réutilise GADGET-2/3), l'ancre **avance à `c1`** (dernier
  justifié), le plancher **reste** genèse (c1 justifié, pas finalisé — deux-temps) ; GHOST depuis `c1`
  **ignore** un frère conflictuel `c1'` hors-ancre.
- **`gadget5a_equal_weight_tie_breaks_on_smallest_hash`** + **`gadget5a_head_is_deterministic_across_observation_order`** :
  poids égal ⇒ **plus petit hash** ; mêmes votes observés en **3 ordres** (forward/shuffled/reversed),
  incluant un re-vote ⇒ **même tête** (LMD ordre-indépendant — C1 en miniature).

### Portes d'acceptation — toutes vertes
- `cargo test --lib` : **366 / 0** (357 + 9 GADGET-5A), incl. les dents §4 (surtout
  **poids-prime-longueur** et **plancher de finalité**).
- `cargo clippy --lib -- -D warnings` **propre** · **C1 vert** (`determinism_meta_test_128…`) ·
  **`t0_8_sweep_catches_planted_violation` / `t0_8_conservation_under_burn` /
  `t0_8_coverage_transfers_and_burns` verts** · `src/sm/` **sans-IO** (moteur = calcul pur, zéro
  horloge/entropie/IO ; tie-break = ordre total déterministe ; walks **bornés** = anti-boucle).
- **Diff logique seule** : **2 fichiers** (1 neuf + export) ; `dispatcher.rs` **intact** (0 réf
  GADGET-5A) ; **invariants de finalité GADGET-1/3/4 toujours verts**.

### Auto-revue — vérification adverse GADGET-5A (5 lentilles)
- **LMD (latest-message) — *sound*** : `observe` ne retient que l'époque cible **strictement
  supérieure**, départage d'égalité au **plus petit hash** ⇒ **ordre-indépendant** (test 3-ordres) ;
  un re-vote **déplace** tout le poids du validateur ; un vote périmé est inerte. Clé = adresse ML-DSA
  (identité de vote/enjeu cohérente).
- **poids de branche — *sound*** : `branch_weights` = Σ enjeu des derniers votes **descendant** du
  bloc, propagé aux ancêtres jusqu'à l'ancre ; enjeu 0 ou hors sous-arbre ⇒ **0** ; sommes entières
  commutatives (`saturating_add`), `BTreeMap` ⇒ aucun ordre `HashMap` ne fuit.
- **règle GHOST ancrée — *sound, le cœur*** : la décision se prend à l'**embranchement** par le poids
  (dent poids-prime-longueur), la descente choisit le max-poids/plus-petit-hash jusqu'à la feuille ; le
  départ est l'**ancre justifiée** (`anchors`, dent dédiée), **pas** la genèse.
- **plancher de finalité — *prouvé absolu*** : `ghost_head` ne descend **que** depuis l'ancre (qui
  descend du plancher) ⇒ la tête **descend toujours** du finalisé ; ancre incohérente ⇒ **repli sur le
  plancher**. Une branche conflictuelle, même bien plus lourde, n'est **jamais** choisie (dent qui mord).
- **déterminisme / périmètre — *tenu*** : tout pur (`BTreeMap`/`BTreeSet`, entiers), walks **bornés**
  par le nombre de nœuds (panic/boucle-freedom) ; **réutilise** GADGET-2 (`Vote`) et GADGET-3
  (`FinalityState` via `anchors`), enjeu on-chain ML-DSA — **rien redéfini** ; **moteur seul** — ni
  résolution de partition (GADGET-5B), ni slashing vivant, **aucun** câblage `Node`/`dispatcher`
  (intact), **aucun** invariant du harnais touché. §9 STOP respecté.

> **Bilan GADGET-5A** : le **moteur** de fork-choice conscient de la finalité **existe**. GHOST suit le
> **poids de votes** (l'enjeu), pas la longueur — verrouillé par `poids-prime-longueur` ; il est **ancré
> au dernier justifié** et **plancher au finalisé** — verrouillé par `plancher absolu` + `ancrage` ; LMD
> et tête sont **ordre-indépendants** (C1 en miniature). Tout **pur**, **réutilise** GADGET-2/3 + enjeu
> on-chain, `sm/` sans-IO, `dispatcher.rs` intact. Reste **GADGET-5B** : la **résolution de partition**
> qui consomme ce moteur pour faire basculer le test 2b (`…gadget_deferred`) de **diverge** à
> **réconcilie**, avec **conservation globale au heal** (défaire l'émission des branches perdantes non
> finalisées) — l'aboutissement du gadget. Puis **PQ-MIG-5** (genèse PQ) + réconciliation
> clé-de-vote↔clé-d'enjeu pour le câblage vivant. Commit = **manuel, Alexandre**.

---

## GADGET-5B — résolution de partition (l'aboutissement du gadget)
*(2026-06-25 · `/goal QUANTA_GADGET_PIECE5B.md` · pièce 5B, §9 de DESIGN-FINALITY-GADGET ; consomme le moteur GADGET-5A, résout le trou ADR-001)*

> Pièce **finale** du gadget. À la guérison d'une partition, le moteur GHOST (5A) choisit la branche au
> plus de poids descendant du dernier justifié, **plancher de finalité absolu**, et les nœuds
> **convergent**. Le test multi-blocs `…gadget_deferred`, semé il y a des sessions comme **cible**,
> **bascule** de **diverge** à **réconcilie**. Exigence n°1 : au heal la **conservation globale** tient —
> l'émission d'une branche perdante **non finalisée** est **défaite** proprement (sinon on rouvre la
> classe double-mint d'EMIT-1, à l'échelle de la partition). Diff logique seule, **tout déterministe**,
> `dispatcher.rs` intact, pas de slashing vivant (§4 STOP).

### Livrables (2 fichiers logique + 2 tests, 1 retouche de doc)
- **`src-tauri/src/p2p/ledger.rs`** : `#[derive(Clone)]` sur `Ledger` (sert le **validate-before-commit**)
  + **`reorg_to_fork(winners, floor_index)`** (réorg multi-blocs **conservant** : pop la branche
  perdante en **reversant** cache + enjeu, **re-met en file** ses tx **utilisateur** absentes du
  gagnant — AUDIT-BLK-1 —, **largue** ses récompenses synthétiques — EMIT-1 §4.1 —, applique le
  gagnant via le **même** `integrate_remote_block` linéaire ⇒ couverture/émission/sig/binding
  identiques ; **plancher de finalité absolu** : refuse de toucher un bloc ≤ `floor_index` ; **essai
  sur clone**, un seul gagnant invalide **annule tout** — la chaîne vive reste intacte) +
  `pop_above(keep_index)` (privé) ; **2 dents** ledger.
- **`src-tauri/src/sm/node.rs`** : **`reconcile_fork(competing)`** (construit l'**arbre union** chaîne∪segment,
  lance `ghost_head` ancré au justifié / planché au finalisé, reconstruit la chaîne gagnante par un
  **walk de parents borné**, appelle `Ledger::reorg_to_fork`) ; `on_chain_segment` **collecte tout le
  segment** et reconcilie sur tout bloc non-linéaire (au lieu du `break` AUDIT-SYNC-1 — un gap décline
  à la réconciliation, pas par rejet dur). Pur : **aucune** horloge/entropie, **aucun ordre `HashMap`
  dans le verdict** (la décision passe par le moteur `BTree` de 5A).
- **`src-tauri/src/sm/sim.rs`** : test 2b **inversé** (cf. §3) ; **`src-tauri/src/sm/fork_choice.rs`** :
  doc-module mise au présent (5B a atterri).

### §1/§2/§3 — réconciliation, conservation globale, bascule du test 2b
- **§1 réconciliation (via 5A)** : `reconcile_fork` bâtit l'arbre union des deux côtés, `ghost_head`
  donne la **tête commune** (sans votes câblés — différé —, le tie-break **plus-petit-hash** + le
  plancher + l'enjeu décident, et le moteur weighte dès que les votes arriveront). **Deux nœuds, mêmes
  blocs ⇒ même tête** (déterminisme 5A) ⇒ ils **convergent**.
- **§2 conservation globale (le point délicat — EMIT-1 revient)** : `reorg_to_fork` **reverse** l'état
  ET l'**émission** de la branche perdante (pop + `cache_revert_tx` + `revert_block_stake_effects`),
  **re-met en file** ses tx utilisateur absentes du gagnant, **largue** ses récompenses (jamais
  re-mintées). Invariant **`Σ(dépensable+staké+déverrouillage)+brûlé == miné`** tenu **globalement**,
  exactement comme hors partition. Une histoire **finalisée** n'est **jamais** défaite (plancher).
- **§3 bascule du test 2b** : `t0_8_multiblock_partition_currently_diverges_gadget_deferred` →
  **`t0_8_multiblock_partition_reconciles_at_heal`** : il assertait la **divergence** (marquée
  gadget-deferred), il asserte désormais **`tips[a] == tips[b]`** (convergence + déterminisme),
  hauteurs égales, conservation+émission par nœud, `check_invariants() == Ok` (sûreté+conservation
  **globales**), **plus** la conservation chirurgicale (seul le mineur gagnant porte l'émission d'**une**
  branche : 99 QTA, l'autre = 0). **Marquage gadget-deferred retiré.**

### §4 — les dents (mordent ; surtout réconciliation + conservation + plancher)
- **réconciliation** (`t0_8_multiblock_partition_reconciles_at_heal`) : deux forks **multi-blocs**
  divergents (A & B scellent chacun 2 blocs sous partition) ⇒ au heal, via sync bidirectionnel, **une
  seule tête commune** (`tips[a] == tips[b]`), hauteur 3 des deux côtés. Déterminisme C1 : chaque nœud,
  voyant les mêmes blocs, choisit le **même** gagnant.
- **conservation globale au heal (load-bearing)** : la même inversion vérifie `Σ balances + brûlé ==
  miné` par nœud **et** `check_invariants() == Ok` ; surgical — `total_minted == 99 QTA` (une branche),
  le mineur perdant à **0** : si l'émission perdante n'était **pas** défaite, `Σ balances > miné` et
  l'invariant **casserait** (la preuve plantée que le revert mord). Doublé au niveau ledger par
  **`gadget5b_reorg_reverts_loser_emission_and_requeues_user_tx`** (minté retombe à 107, pas 112).
- **transactions re-mises en file** (`gadget5b_reorg_reverts_loser_emission_and_requeues_user_tx`) : la
  tx **utilisateur** (transfert + burn) de la branche perdante, absente du gagnant, est **re-mise en
  file** (AUDIT-BLK-1) ; sa récompense de minage **n'est pas** re-mise (pas de double-mint, EMIT-1 §4.1).
- **finalité préservée** (`gadget5b_reorg_to_fork_refuses_below_finalized_floor`) : un fork qui
  remplacerait un bloc **finalisé** (`floor_index = 2`, fork enraciné à l'index 1) est **refusé**
  (`Ok(false)`, chaîne intacte) ; contrôle positif — au-dessus du plancher (`floor_index = 1`) le
  **même** fork réorganise. Le plancher protège **exactement** ce que la finalité protège.
- **déterminisme** : convergence identique des deux nœuds (C1) ; tout pur, walks **bornés**, verdict via
  le moteur `BTree` de 5A.

### Portes d'acceptation — toutes vertes
- `cargo test --lib` : **368 / 0** (366 + 2 dents ledger ; le test 2b **inversé** réconcilie+conserve
  au lieu de diverger).
- `cargo clippy --lib -- -D warnings` **propre** · **C1 vert** (`determinism_meta_test_128…`) ·
  **`t0_8_sweep_catches_planted_violation` / `t0_8_conservation_under_burn` /
  `t0_8_coverage_transfers_and_burns` verts** · `src/sm/` **sans-IO** (réconciliation pure : zéro
  horloge/entropie ; arbre depuis des `Vec` ordonnés ; verdict via `ghost_head` `BTree` ; walks bornés).
- **Diff logique seule** : `dispatcher.rs` **intact** (0 réf GADGET-5B) ; **marquage gadget-deferred
  retiré** du test 2b ; **invariants de finalité GADGET-1/3/4 toujours verts**.

### Auto-revue — vérification adverse GADGET-5B (5 lentilles)
- **réconciliation (via 5A) — *sound*** : `reconcile_fork` ne redéfinit **rien** — il bâtit l'arbre
  union et délègue la **décision** à `ghost_head` (moteur 5A : ancré justifié, plancher finalisé). Le
  segment **entier** alimente l'arbre (correction subtile : un tip de fork au **hash plus petit** est
  *avalé* par le chemin single-block `Ok(false)` — le reconstruire des seuls `Err` le raterait). Deux
  nœuds, mêmes blocs ⇒ même tête ⇒ convergence (C1).
- **conservation globale — *prouvée, n°1*** : le pop **reverse** cache + enjeu **et** retire l'émission
  perdante de la chaîne (donc de `total_minted`, recalculé chaîne+pending) ; les tx utilisateur
  re-mises sont neutres au bilan, les récompenses larguées **disparaissent** des deux côtés (balances ↓,
  miné ↓). `Σ(dépensable+staké+déverr.)+brûlé == miné` tenu — vérifié par nœud, globalement, et
  chirurgicalement (mineur perdant à 0). La dent plantée mord.
- **finalité préservée — *absolue*** : `reorg_to_fork` **refuse** tout fork enraciné `< floor_index`
  (jamais de pop d'un bloc finalisé) ; le plancher 5A garantit déjà que `ghost_head` ne sort pas du
  finalisé — **ceinture + bretelles**. Par la sûreté responsable (GADGET-4), deux finalisés en conflit
  exigent ⅓ slashable, donc les préfixes finalisés **coïncident**.
- **validate-before-commit — *tenu*** : l'essai sur **clone** rejoue le **même** `integrate_remote_block`
  linéaire (couverture/émission/sig/binding pleines) ; un seul gagnant invalide **annule tout**, la
  chaîne vive intacte (AUDIT-BLK-2 généralisé à N blocs — un fork malformé ne tronque jamais). Le clone
  ne coûte qu'à un **rare** reorg de heal.
- **déterminisme / périmètre — *tenu*** : tout pur (`Vec` ordonnés → arbre `BTree` → tête ; aucun ordre
  `HashMap` dans le verdict), walks **bornés** ; **réutilise** moteur 5A + finalité 3/4 + conservation
  du harnais — **rien redéfini** ; **pas** de slashing vivant, **aucun** câblage `dispatcher` (intact).
  §4 STOP respecté.

> **Bilan GADGET-5B** : le **gadget de finalité est complet**. Finalité **réelle** (GADGET-3),
> **responsable** (GADGET-4), et **résolution de partition** (GADGET-5A moteur + 5B heal). Le trou
> multi-blocs traîné depuis des sessions est **fermé** : deux partitions multi-blocs **convergent** au
> heal sur une seule tête, l'émission perdante est **défaite**, la **conservation globale** tient, et la
> finalité est un **plancher absolu**. Le test 2b semé jadis comme cible **bascule** enfin. Restent, hors
> gadget : **PQ-MIG-5** (genèse PQ) et la réconciliation **clé-de-vote ↔ clé-d'enjeu** pour le câblage
> vivant (slashing + votes réels en production — vote gossip non encore branché ici, `LatestVotes` vide,
> le tie-break/plancher/enjeu décident en attendant). Commit = **manuel, Alexandre**.
>
> **[MAJ 2026-07-12]** La réconciliation clé-de-vote ↔ clé-d'enjeu est **résolue par PQ-MIG-3B** :
> identité de compte unique ML-DSA, la clé de vote **est** la clé d'enjeu (même adresse). Le câblage
> vivant du gossip des votes est fait — **LIVE-1** (`p2p/finality_live.rs`, `FinalityTracker`,
> `Ledger::validator_stakes_by_pubkey`), `LatestVotes` n'est plus vide en vivant. Reste **LIVE-2**
> (proposition finalité-consciente) et **LIVE-3** (slashing vivant STAKE→BURN).

---

## PQ-MIG-5 — genèse post-quantique (clôt le chantier crypto)
*(2026-06-25 · `/goal QUANTA_PQ_MIG_5.md` · dernière pièce de la migration PQ ; construit sur PQ-MIG-1/2/3B)*

> Le bloc de **genèse** est reconstruit sur les identités **ML-DSA** (PQ-MIG-1) et adresses **BLAKE3**
> (PQ-MIG-2) : la machinerie encode l'**état initial** (allocation) et l'**ensemble de validateurs
> initial** comme un **mapping déterministe** `adresse ML-DSA → (solde, enjeu)`, avec un **hash de
> genèse lié au contenu** (le hachage de bloc canonique, domaine-séparé) et un **bump
> `TORUS_PROTOCOL_VERSION` 2 → 3**. Diff logique seule, déterministe, **C1 vert**, conservation
> **exacte dès le bloc 0**.

### Décision produit — premine (le point délicat, tranché)
- §2 décrit une allocation de genèse = **un premine**, qui **contredit** le pilier de mission
  **« zéro premine »** (CLAUDE.md, règles Rust, mémoire). Le spec marque pourtant l'allocation
  **indécise / placeholder / §12-réglable**. → **Question posée à Alexandre**, réponse :
  **genèse par défaut VIDE** (zéro premine préservé) **+ machinerie complète testée**.
- Donc `Ledger::new()` == `genesis_with_allocation(&[])` : **offre 0 au bloc 0**, bloc de genèse sans
  tx, `trust_no_premine_at_genesis` **reste vert inchangé**. La vraie distribution (possiblement nulle)
  est une décision §12 ; câbler une allocation = changer une ligne (`new()` → mapping).

### Livrables
- **§1 — genèse déterministe** (`ledger.rs`) : `genesis_with_allocation(&[(adresse, solde, enjeu)])`
  bâtit un bloc 0 **fonction pure** du mapping. Encodage en tx : **une seule** `Mining` (mint du total
  `Σ(solde+enjeu)` au 1ᵉʳ compte, ≤ borne d'émission/bloc à offre 0) → `Transfer`s **neutres**
  distribuant chaque part → un `Stake` par validateur (enjeu > 0). État dérivé via `rebuild_cache`
  (source unique = la chaîne), hashes de genèse insérés en anti-replay. `new()` = mapping vide.
- **Hash de genèse lié au contenu** : nouveau helper **partagé** `block_hash_hex(index, prev, ts,
  miner, txs)` — **un seul** hachage de bloc canonique pour `seal_block_at`, `validate_block_against_prev`
  **et** la genèse (domaine-séparé par `index = 0`, `prev = 0×64`, `miner = "GENESIS"`). Les trois ne
  peuvent plus diverger. (L'ancien littéral `BLAKE3("QUANTA_GENESIS_2026")` est retiré.)
- **§2 — allocation placeholder 🛑** : `DEV_GENESIS_ALLOCATION` (`#[cfg(test)]`, 3 adresses ML-DSA de
  test = adresses PQ-MIG-2 de graines figées `seeded_identity(5_000_00N)`, 2 validateurs + 1 porteur,
  total 100 QTA) — **nominal, ne promet rien, jamais figé définitif**. N'est **pas** câblé dans `new()`.
- **§3 — conservation au bloc 0** : `miné == Σ(solde + enjeu)` et
  `Σ dépensable + enjeu-verrouillé + brûlé == miné`, exact dès le bloc 0 (l'enjeu **vient** de
  l'allocation, pas créé en plus).
- **§4 — bump version** : `TORUS_PROTOCOL_VERSION` **2 → 3** (`gossip.rs`) — la genèse PQ est une
  rupture de protocole.

### Les dents (§5) — toutes mordent
- **genèse déterministe** (`pqmig5_genesis_hash_is_deterministic_and_frozen`) : deux constructions ⇒
  hash **identique** ; **vecteurs figés** (vide `37bb8957…`, DEV `d13f6221…`) ; le hash **lie le
  contenu** (vide ≠ alloué).
- **conservation à la genèse** (`pqmig5_dev_genesis_conserves_at_block_zero`) : `miné == Σ alloc == 100
  QTA`, `Σ == miné` au bloc 0 ; **dent négative** — planter un `Stake` de genèse **sans `Mining`
  derrière** rend `Σ ≠ miné` (la vérif **mord**).
- **validateurs initiaux** (`pqmig5_dev_genesis_validators_reflect_mapping`) : `validator_stakes()` au
  bloc 0 = `{G0:10, G1:5}` exactement, indexé par **adresse ML-DSA** ; le porteur (enjeu 0) **n'est
  pas** validateur ; soldes dépensables = colonne « solde ».
- **adresses ↔ graines** (`pqmig5_genesis_addresses_bind_their_seeds`) : chaque adresse figée EST
  `BLAKE3(ADDR_DOMAIN ‖ clé ML-DSA)` de sa graine (PQ-MIG-2) — pas du hex magique.
- **enchaînement** (`pqmig5_first_block_on_pq_genesis_validates_and_conserves`) : un 1ᵉʳ bloc (récompense
  + transfert signé dépensant la valeur de genèse de G0) **valide** chez un récepteur frais sur la même
  genèse PQ (`Ok(true)` — couverture/émission/binding PQ-MIG-3B) et **conserve** (miné 100 → 107).
- **déterminisme global** (`pqmig5_pq_genesis_chain_is_deterministic` + `determinism_meta_test_128`) :
  **C1 vert** sur une chaîne partant de la genèse PQ (genèse + bloc #1 byte-identiques).

### Portes (acceptation)
- `cargo test --lib` : **374 / 0** (368 + 6 dents §5), incl. conservation au bloc 0 + genèse déterministe.
- `clippy --lib -D warnings` **propre** · **C1 vert** · sweep + couverture + conservation verts ·
  `src/sm/` **sans-IO** (machinerie de genèse pure, dans `p2p/ledger.rs` ; `sm/` n'a gagné que du test).
- `git diff` **logique seule** · `dispatcher.rs` **intact** (0 réf PQ-MIG-5) · `TORUS_PROTOCOL_VERSION`
  **2 → 3** · invariants de finalité **GADGET-1/3/4 verts** · `trust_no_premine_at_genesis` **vert**.
  *(Note : `clippy --tests` signale un `result_large_err` **préexistant** sur `run_checked_steps`/`Violation`
  du harnais GADGET — hors PQ-MIG-5, non introduit ici ; la porte du spec est `--lib`, propre.)*

### Auto-revue — §3 (5 lentilles)
- **genèse ML-DSA — *tenu*** : `from`/`to`/validateurs = **adresses ML-DSA** PQ-MIG-2 ; le mapping +
  l'enchaînement signé prouvent qu'un compte de genèse se **dépense** sous autorité ML-DSA (binding).
- **allocation placeholder marquée — *tenu*** : `DEV_GENESIS_ALLOCATION` est `#[cfg(test)]`, 🛑 §12,
  **hors** `new()` ; le défaut est **vide** ⇒ zéro premine préservé (décision Alexandre).
- **conservation au bloc 0 — *prouvée, n°1*** : `miné == Σ(solde+enjeu)`, `Σ dépensable+verrouillé+brûlé
  == miné` exact ; la dent plantée (enjeu non couvert) **casse** l'égalité ⇒ aucun déséquilibre masqué.
- **bump version — *fait*** : 2 → 3, noté ici (rupture de protocole de la genèse PQ).
- **périmètre — *tenu*** : genèse + version **seulement** ; **pas** de gossip de votes, **pas** de
  slashing vivant, **pas** de réconciliation clé-de-vote↔clé-d'enjeu ; réutilise identité/adresses/
  autorité PQ-MIG-1/2/3B — **rien redéfini** ; `dispatcher.rs` intact ; diff logique seule.

> **Bilan PQ-MIG-5** : la **migration post-quantique est complète** — racine (PQ-MIG-1), adresses
> (PQ-MIG-2), autorité + enjeu (PQ-MIG-3B) **et** désormais **genèse** entièrement ML-DSA. De la genèse
> au consensus, Quanta est réellement post-quantique. La **machinerie** de genèse d'allocation existe,
> est déterministe (hash lié au contenu, vecteurs figés), conserve dès le bloc 0 et porte des
> validateurs initiaux ; le **défaut reste zéro premine** (décision Alexandre), une vraie distribution
> étant une décision **§12**. Reste, **hors migration**, le **gros morceau** : la réconciliation
> **clé-de-vote ↔ clé-d'enjeu** pour câbler le gadget en vivant (votes par gossip, slashing sur ledger
> réel) ; et les décisions **§12** (allocation réelle, montants) + la frontière gravé/ajustable d'ADR-006.
> Commit = **manuel, Alexandre**.
>
> **[MAJ 2026-07-12]** Réconciliation clé-de-vote ↔ clé-d'enjeu **résolue** — PQ-MIG-3B fait de la
> clé de vote et de la clé d'enjeu la **même** adresse de compte ML-DSA, rien à réconcilier. Frontière
> gravé/ajustable et §12 **ratifiés par ADR-009** (E=32, quorum ⅔, unbonding 10 080, slash brûlé/plein).
> Le câblage vivant du gossip des votes est fait (**LIVE-1**, 379 tests) ; reste **LIVE-2**
> (proposition finalité-consciente) et **LIVE-3** (slashing vivant STAKE→BURN).

---

## LIVE-1 — câblage vivant du gadget de finalité, gossip des votes
*(2026-07-12 · [[DESIGN-LIVE-WIRING]] §2.1-2.2, §3 · première pièce du câblage IO, construit sur GADGET-1→5B + PQ-MIG-1→5)*

> Le gadget de finalité (`sm/finality*`, `sm/fork_choice`) était **prouvé en simulation** mais
> **hors-circuit** en vivant : aucun message gossip ne portait de vote, `LatestVotes` restait vide sur
> un nœud réel. **LIVE-1** branche le premier fil : les votes de finalité circulent par gossip et
> peuplent l'état vivant, **sans toucher** au chemin de valeur (pas encore de proposition
> finalité-consciente ni de slashing vivant — LIVE-2/3).

### Livrables
- **`GossipMessage::FinalityVote { vote_json }`** (lane **Critical**, au même rang que `NewBlock`) —
  le vote signé ML-DSA voyage en JSON dans l'enveloppe gossip existante (Ed25519 transport + nonce +
  timestamp, pipeline ⑨ inchangé).
- **Bras dispatcher** (étape ⑨) : `handle_finality_vote` — désérialise le `Vote`, l'**ingest** dans le
  tracker vivant ; aucune règle de verdict nouvelle, la logique reste dans `sm/`.
- **`p2p/finality_live.rs` — `FinalityTracker`** : combine `LatestVotes` (GADGET-5A), `FinalityState`
  (GADGET-3), l'arbre de blocs et un **pool de votes par lien** — l'état IO qui **appelle** le cœur
  sans-IO, ne le redéfinit pas.
- **`Ledger::validator_stakes_by_pubkey()`** — le pont pubkey → adresse : re-clé l'enjeu on-chain par
  la clé ML-DSA **révélée** dans chaque tx `Stake` (fonction pure de la chaîne, identique sur tout
  nœud — même discipline qu'ONCHAIN-STAKE-1).
- **Cast au tick de mining** — `cast_finality_vote_if_validator` dans `mining_loop.rs` : un validateur
  vote au fil des ticks, signé ML-DSA, diffusé en `FinalityVote`.
- **Dérives additives** — `serde`/`Ord` sur `Vote`/`Checkpoint` : sérialisation et tri pour le
  transport et l'arbre, **aucune logique** ajoutée aux types du cœur.

### Portes (acceptation)
- `cargo test --lib` : **379 / 0**.
- `clippy --all-targets -D warnings` propre.
- **C1** (déterminisme, 128 runs) byte-identique — inchangé.
- Build frontend OK.

### Auto-revue — périmètre sans-IO
- **cœur `sm/` intact** — aucune règle de verdict nouvelle ; tout verdict de finalité reste une
  **fonction pure** du cœur existant (GADGET-1→5B) ; `FinalityTracker` **appelle**, ne redéfinit pas.
- **pont pubkey↔adresse pur** — `validator_stakes_by_pubkey` est une fonction pure de la chaîne
  (source = tx `Stake` scellées), pas un état séparé à faire diverger.
- **IO testée à part** — cinq dents dédiées : re-clé du pont pubkey↔adresse, round-trip wire du
  `FinalityVote`, rejet d'un vote forgé/non-validateur, finalisation déclenchée depuis le gossip (⅔
  atteint via votes reçus), non-validateur ne cast rien.

> **Bilan LIVE-1** : le gadget de finalité **observe** désormais le réseau réel — les votes ML-DSA
> circulent par gossip, peuplent `LatestVotes`/`FinalityState` sur chaque nœud vivant, le pont
> pubkey↔enjeu est une fonction pure de la chaîne. **Additif et sans risque de divergence** : tant que
> **LIVE-2** n'est pas là, les votes ne font qu'observer — un pair qui ignore `FinalityVote` bâtit
> la **même** chaîne (le proposeur suit toujours `chain.last()`, pas `ghost_head`), donc pas de bump de
> protocole nécessaire pour ce pas. Restent, réservés en `/goal` chirurgical (§4, arbitrage
> consensus) : **LIVE-2** (le proposeur bâtit sur `ghost_head` ancré finalité au lieu de
> `chain.last()`) et **LIVE-3** (slashing vivant STAKE→BURN sur le ledger réel) — chacun touche le
> chemin de valeur/conservation. Commit = **manuel, Alexandre**.
>
> **[MAJ 2026-07-12]** **LIVE-2 et LIVE-3 sont désormais faits** (voir les entrées ci-dessous) — le
> câblage vivant du gadget de finalité est **complet**.

---

## LIVE-2 — câblage vivant du gadget de finalité, plancher de finalité vivant
*(2026-07-12 · [[DESIGN-LIVE-WIRING]] §2.2, §3 · construit sur LIVE-1 · commit 09adde7)*

### Livrables
- **`Ledger::finalized_floor_index`** — monotone, tip-clampé, persisté au snapshot avec
  `#[serde(default)]` (rétro-compatible avec les snapshots existants).
- **`set_finalized_floor`** — alimenté par les certificats ⅔ (dispatcher `handle_finality_vote` +
  cast dans `mining_loop.rs`) : dès qu'un checkpoint finalise, le plancher **monte**.
- **Veto absolu dans `integrate_remote_block`** — refuse tout fork concurrent à hauteur ≤ plancher ;
  le départage libre (lexicographique) ne s'applique **qu'au-dessus** du plancher (Gasper :
  fork-choice libre au-dessus de la finalité, gelé à/sous). Garde de **sûreté pure** : rejeter un
  reorg ne mute aucun solde → conservation inchangée.
- **`FinalityTracker::finalized_floor_height`** — expose la hauteur finalisée côté tracker vivant.

### Portes (acceptation)
- `cargo test --lib` : **384 / 0**.
- 5 dents : veto d'un tip finalisé, reorg toujours possible au-dessus du plancher, setter
  monotone + tip-clampé, round-trip snapshot du plancher, plancher du tracker qui monte sur
  finalisation reçue par gossip.

### Bilan
L'histoire finalisée est **irréversible** sur le réseau vivant — c'est la moitié **sûreté** de
LIVE-2 (l'objectif de la conception initiale). Le reorg-timing **actif** piloté par `ghost_head`
(bâtir directement sur la tête GHOST plutôt que sur `chain.last()`) reste un **raffinement
optionnel** — il n'ajoute pas de garantie de sûreté supplémentaire, seulement une meilleure
convergence de timing. Commit = **manuel, Alexandre**.

---

## LIVE-3 — câblage vivant du gadget de finalité, slashing vivant
*(2026-07-12 · [[DESIGN-LIVE-WIRING]] §2.3, §3 · construit sur LIVE-1/LIVE-2 · commit ccc6039)*

> Accountable safety avec dents : un validateur qui équivoque est détecté, prouvé et **puni pour de
> vrai** sur le ledger réel — pas seulement en simulation.

### Livrables
- **`TxType::Slash`** + champ `fault_proof` embarqué sur `Transaction`.
- **Accounting STAKE→BURN conservation-neutre** : `cache_apply`/`revert`, `total_burned` compte le
  Slash, effets d'enjeu apply/revert, exemptions coverage/binding pour ce type de tx synthétique.
- **`verify_block_slashes` / `slash_tx_valid` / `invalid_slash_indices`** — re-vérification de la
  preuve (`verify_proof`), correspondance de l'adresse de l'offenseur, montant == fraction ratifiée
  de l'enjeu bonded courant, destination == BURN ; **partagé** entre le chemin de scellement et de
  réception (symétrie COVER-2, bloc valide par construction).
- **`build_slash_tx` / `queue_slash` / `slash_amount_for`** — construction, mise en file locale,
  calcul du montant selon la politique ADR-009 (slash plein, brûlé, fenêtre = unbonding).
- **`verify_tx` exempte le Slash** (tx block-only, `handle_broadcast_tx` la rejette si elle arrive
  hors bloc — pas de gossip direct de tx Slash).
- **Détection d'équivocation à l'ingest** (`detect_fault` sur le vote entrant) → surfaçage d'une
  `FaultProof` → `GossipMessage::FinalityFault` + `queue_slashes` local ; les récepteurs
  re-vérifient et mettent en file.

### Portes (acceptation)
- `cargo test --lib` : **388 / 0**.
- 4 dents : brûlage + conservation avec convergence côté récepteur, slash forgé/montant erroné
  rejeté, équivocation détectée à l'ingest, offenseur non-staké → no-op.
- C1 + conservation + sweep multi-seed + émission **verts** ; `clippy --all-targets` propre ; build
  frontend OK.

### Bilan
Le câblage vivant du gadget de finalité est **COMPLET** (LIVE-1 → LIVE-3) : les votes, la
finalité et le slashing tournent sur le réseau réel, plus seulement en simulation déterministe. Un
validateur qui équivoque perd son enjeu pour de vrai, et un proposeur malveillant ne peut pas punir
un validateur innocent (`verify_block_slashes` re-vérifié sur chaque nœud). Commit = **manuel,
Alexandre**.

---

## Revue adversariale LIVE-2/LIVE-3 — 4 failles trouvées & corrigées (2026-07-12)

Avant de livrer du code qui **détruit de l'enjeu** (LIVE-3), une **revue adversariale
indépendante** (agent dédié, mission : « casse la conservation / forge un slash / brise le
déterminisme ») a été passée sur les chemins monétaires. Elle a trouvé **2 CRITICAL + 2 HIGH**
réelles, toutes corrigées avec un test de régression encodant l'attaque (commit `39b9cff`) :

- **CRITICAL-1 — multi-slash / slash+unstake cassaient la conservation.** `invalid_slash_indices`
  validait chaque `Slash` **indépendamment** contre le même enjeu pré-bloc : un leader pouvait
  s'auto-équivoquer une fois puis **dupliquer** la tx `Slash` K fois (ou coupler un `Slash` avec
  l'`Unstake` de l'offenseur) → le sink STAKE débité K× pendant que `staked` sature à 0 → rupture
  **permanente** de conservation (sink négatif masqué par `.max(0)`). Corrigé : passe **séquentielle**
  — au plus **un slash par offenseur par bloc**, et un offenseur slashé ne peut pas aussi Stake/Unstake
  dans le même bloc. Tests : `live3_duplicate_slash_*`, `live3_slash_with_concurrent_unstake_*`.
- **CRITICAL-2 — `rebuild_cache` (restore) mal-comptait le `Slash` → divergence C1.** Au restore
  (snapshot toutes les ~30 s / reprise sur crash), un `Slash` débitait le **spendable** de
  l'offenseur au lieu du **sink STAKE** → un nœud redémarré divergeait d'un nœud en vif (et
  conservation cassée si spendable < montant). Corrigé via un helper `replay_cache_effect` partagé
  qui reflète `cache_apply_tx`. Test : `live3_slash_survives_snapshot_restore_identically`.
- **HIGH-3 — inversion d'ordre de locks → deadlock possible.** `cast_finality_vote_if_validator`
  tenait ledger→finality→crypto alors que `transfer` tient crypto→ledger (cycle croisé). Corrigé :
  `crypto` pris **en premier** (ordre documenté crypto→ledger→finality→gossip).
- **HIGH-4 — plancher de finalité agnostique au hash.** `set_finalized_floor` gelait par index seul :
  un nœud sur un bloc `Y@H` pendant que le gadget finalisait `X@H` gelait `Y` et rejetait `X`
  finalisé **pour toujours**. Corrigé : le plancher n'avance que si **notre** bloc à cette hauteur
  correspond au **hash** du checkpoint finalisé (sinon warning + le nœud doit synchroniser la branche
  finalisée). Test : `live2_finalized_floor_is_monotonic_hash_checked_and_bounded`.
- **Durcissement LOW** : le hash de la tx `Slash` **et** la feuille de Merkle lient désormais la
  **preuve complète** (plus un préfixe 32 chars) — un relais ne peut pas échanger la preuve sans
  changer le hash du bloc ; `slash_amount_for` clampe à l'enjeu (défense pour une future fraction > 1).

**Portes après corrections** : 391 tests / 0 échec ; C1 128-runs byte-identique ; sweep multi-seed
conservation + émission verts ; clippy `--all-targets` propre. **Suivi CLOS** : le harnais DST (`sim.rs`) génère désormais des tx `Slash` — `Move::Stake` +
`Move::Slash` + `slash_scenario` + le sweep dédié `t0_8_slash_sweep_conserves` (slashing multi-seed
sous fautes réseau, conservation/émission/sûreté vérifiées **par pas**) et `t0_8_slash_sweep_is_reproducible`
(C1 sous slashing). La couverture fuzz continue du chemin de slash est en place.

---

## Audit exhaustif multi-agents (ultracode) — durcissement post-livraison (2026-07-12)

Après LIVE-1→3 + la revue adversariale (4 failles), un **audit exhaustif** a été lancé
(workflow multi-agents : un chercheur profond par domaine — conservation/émission, sûreté
consensus, slashing, crypto/PQ, P2P/dispatch, déterminisme — chaque trouvaille vérifiée par
3 sceptiques en réfutation). Trouvaille CONFIRMÉE et corrigée :

- **HIGH — croissance mémoire non bornée du `pool` de votes (`FinalityTracker`).** Le hash de
  `target` d'un vote n'est pas vérifié contre un vrai bloc (c'est le rôle de GADGET-3 avec la
  chaîne) : un seul validateur à l'enjeu minimal pouvait gossiper une infinité de votes bien
  formés à `target` distincts — chacun un nouveau lien, jamais élagué (un attaquant sous-⅔ ne
  finalise jamais) → OOM sur chaque nœud. Corrigé (`finality_live.rs::ingest_vote`) : (1) un lien
  dont l'époque cible est ≤ l'époque finalisée n'est jamais mis en pool (la finalité ne monte que) ;
  (2) plafond dur `max_pending_links` (4096 en prod) avec éviction déterministe du plus ancien lien.
  État IO (liveness/mémoire), pas un verdict de consensus → l'éviction ne change jamais l'ensemble
  finalisé (un vrai certificat ⅔ finalise quel que soit le contenu du pool), C1/sûreté intacts.
  Tests : `audit_pool_is_bounded_against_a_vote_flood`, `audit_stale_links_below_finality_are_not_pooled`.

**Portes** : 395 tests / 0 échec ; C1 128-runs byte-identique ; sweep multi-seed conservation +
émission + **slashing** verts ; clippy `--all-targets` propre.

### Grappe « cycle de vie du slash » — 4 confirmées & corrigées + 3 évaluées (2026-07-12)

L'audit exhaustif (agents coupés par la limite de session) a laissé une grappe de trouvailles
sur le **cycle de vie d'une tx `Slash`** dans le mempool/fork. Chacune **revérifiée à la main
contre le code** (les vérificateurs sceptiques n'ont pas tous fini), puis corrigée avec un test
de régression encodant la panne :

- **HIGH 788 — le slash était inopérant en production (éviction TTL).** Une tx `Slash` porte le
  `GENESIS_TIMESTAMP` fixe **par conception** (hash déterministe / C1 — un horodatage mural
  casserait l'accord inter-nœuds), donc toujours « périmé » face à l'heure réelle. `prune_mempool`
  (toutes les ~30 s) l'évinçait comme du trafic mempool ordinaire — **avant** que le seal (~120 s)
  ne puisse l'inclure → le slashing ne se déclenchait **jamais** en vif. Corrigé : le `Slash` est
  **exempté de l'éviction TTL** (Passe 1) ; il est nettoyé au seal (exclusion COVER-2 d'un offenseur
  déjà slashé) et ne peut pas s'accumuler (un slash pending par offenseur, ci-dessous). Test :
  `audit_pending_slash_survives_mempool_ttl_prune`.
- **HIGH 2318 — slash pending redondant non évincé → rupture de conservation permanente.** Le nœud R
  tient un `Slash` **pending** pour l'offenseur O ; un bloc distant slashe O via une **autre** preuve
  valide. Une fois le bloc appliqué (staked[O]→0), le slash pending est redondant — et exempté du TTL
  (788), donc il **traînait indéfiniment** avec son débit sink d'admission non corrigé → `total_burned`
  double-comptait un enjeu qui n'existe plus. Corrigé : `evict_stale_pending_slashes` (même règle
  `invalid_slash_indices` que seal/réception) appelé **à l'application du bloc** (seal + les deux
  chemins d'`integrate`) ⇒ conservation exacte **au temps du bloc**, pas seulement au prochain prune.
  Test : `audit_pending_slash_evicted_when_a_block_slashes_the_same_offender`.
- **HIGH 2450 — un `Slash` poppé était re-mis en file au reorg.** Les boucles de re-queue
  (`integrate_remote_block` swap de fork + `reorg_to_fork`) ne sautaient que les émetteurs
  synthétiques (`NETWORK`/`ESCROW`). Un `Slash` (émetteur = adresse de l'offenseur) était re-mis en
  file → re-débit du sink STAKE / risque de double-slash si le gagnant re-slashe. Corrigé : le `Slash`
  est **réseau-autorisé** (autorité = preuve embarquée, appartient à un bloc, pas au mempool) — même
  saut que les synthétiques. Test : `audit_reorg_does_not_requeue_a_popped_slash`.
- **MEDIUM 911 — deux preuves distinctes pour un offenseur mettaient 2 slashs en file.** Deux preuves
  valides mais **distinctes** (p. ex. `FaultProof(a,b)` vs `(b,a)`, hash différent → non attrapé par
  la dédup) empilaient deux `Slash` pending pour le même O → double débit sink sur un nœud non-sealer.
  Corrigé : garde par-offenseur dans `queue_slash` (refus si un `Slash` pending existe déjà pour ce
  `from`). Test : `audit_two_distinct_proofs_for_one_offender_queue_one_slash`.

**Évaluées** (vérifiées contre le code — pas de correctif nécessaire ou porté au roadmap) :

- **2396 — « incohérence de montant au reorg » : RÉFUTÉE.** `revert_block_stake_effects` (bras `Slash`)
  restaure `staked += tx.amount` — **le montant propre de la tx**, symétrique de l'apply ; `cache_revert_tx`
  restaure le sink. Un slash poppé est donc inversé **exactement**. `gadget5b_reorg_*` et le nouveau
  `audit_reorg_does_not_requeue_a_popped_slash` confirment la conservation à travers le reorg. Aucun bug.
- **2359 — « fork-choice non conscient de la finalité » : déjà mitigée (défense en profondeur).** La
  fonction `ghost_head` (LMD-GHOST) ne consulte pas le plancher, mais l'**enforcement** est au point
  d'intégration : `integrate_remote_block`/`reorg_to_fork` **vetoent** tout reorg ≤ `finalized_floor_index`
  (LIVE-2). Une suggestion de fork-choice sous le plancher est donc rejetée à l'application — l'histoire
  finalisée reste irréversible. Test existant : `live2_integrate_refuses_to_reorg_a_finalized_tip`.
- **837 — « unstake-and-run » : limitation de conception connue (roadmap, LIVE-3B).** Un offenseur qui
  `Unstake` **avant** que son slash ne soit scellé déplace ses coins vers l'unbonding (le slash cible le
  *bonded*, donc `staked=0` → rien à slasher ; dans le même bloc c'est déjà refusé, mais à cheval sur deux
  blocs il échappe). **Important : c'est un trou d'*imputabilité*, PAS de conservation** — le design actuel
  reste sûr (un `Unstake` scellé rend le slash pending invalide → évincé par `evict_stale_pending_slashes`,
  débit sink révoqué, comptabilité exacte sur chaque nœud ; l'offenseur échappe à la *punition*, l'argent
  reste conservé). Mitigation : coins **verrouillés `UNBONDING_PERIOD_BLOCKS` (~2 sem.)** ≫ fenêtre de
  détection.
  **Pourquoi ce n'est pas un patch rapide (analyse de diligence)** : le correctif propre (le slash atteint
  `staked + unbonding`, sémantique Casper) a un **sous-problème de réversibilité** dur — au reorg, un slash
  poppé doit restaurer **exactement** les entrées d'unbonding détruites (`amount` + `unlock_height` +
  `tx_hash`), que la tx `Slash` ne porte pas aujourd'hui. Le fermer proprement = encoder les entrées
  détruites dans la preuve/tx **ou** rendre le slash-sur-unbonding déterministe par rejeu — une **addition
  de conception** (LIVE-3B) à faire dans une passe dédiée, pas en addendum précipité sur du code
  conservation-critique. L'alternative (geler l'`Unstake` d'un offenseur dès qu'une preuve existe) a ses
  propres courses inter-nœuds. Au roadmap avec le vrai VRF/VDF + le slashing d'inactivité.

**Portes après la grappe** : **399 tests + 1 intégration / 0 échec** ; C1 128-runs byte-identique ;
sweep multi-seed conservation + émission + **slashing** verts ; clippy `--all-targets` propre.

---

## LIVE-4 + LIVE-3B — les deux derniers trous réels fermés (2026-07-13)

### LIVE-4 — réconciliation de fork profonde en vivant (trou de convergence)
Constat vérifié dans le code : `reorg_to_fork` (GADGET-5B, la réconciliation de partition) était
DST-prouvé **mais jamais appelé du chemin réseau** — `integrate_remote_block` ne gère que l'extension
linéaire et le fork 1-bloc, donc **deux partitions scellant chacune ≥2 blocs ne convergeaient jamais**
(boucle `"out of range"`), en violation de la règle « fork convergence <60s ». Fermé par
`p2p/fork_heal.rs` (`ForkReconciler`) : tampon borné déterministe nourri des blocs qui échouent
l'intégration linéaire, assemblage de la branche concurrente, règle de victoire vivante
(plus-longue-au-dessus-du-plancher + départage lexicographique — généralisation N-blocs de la règle
1-bloc, convergence symétrique), application via `reorg_to_fork` (clone d'essai, plancher LIVE-2
absolu), sondes d'ancêtre par `RequestChain` descendantes. Bonus : guérit les fenêtres ChainSegment
hors-ordre (NET-6). 8 tests (heal symétrique 2-3, départage à égalité, veto plancher, hors-ordre,
branche invalide purgée sans retry, tampon borné sous flood, sondes clampées, conservation à travers
un heal avec récompenses).

### LIVE-3B — le slash atteint l'unbonding (« unstake-and-run », ex-837, FERMÉ)
Le sous-problème de réversibilité (un slash poppé au reorg doit restaurer les entrées d'unbonding
exactes) est résolu par conception : la tx `Slash` **porte sa ventilation de consommation**
(`slash_unbonding`, ordre déterministe `(unlock_height, tx_hash)`), liée **hash + Merkle**, que chaque
nœud **re-vérifie contre son propre plan** (`expected_slash_consumption`, source unique build+verify —
un proposeur ne peut ni sur-slasher, ni sous-slasher, ni mentir sur ce qui meurt) et que le revert
rejoue **à l'identique** (montant + hauteur de déverrouillage + tx d'origine). Base slashable =
staké + unbonding ; deux cartes d'enjeu (vote = bondé seul ; slashable = bondé + unbonding pour
`verify_proof` — ledger et dispatcher). Les slashes purement bondés restent **byte-identiques**
(zéro dérive wire, C1 sous slashing vert). La fraction de slash (ADR-009) est **inchangée** — 🛑
décision d'Alexandre. 6 tests (unstake-and-run attrapé bout-en-bout + convergence du récepteur,
mixte bondé+unbonding, reorg restaure exactement — hauteur épinglée par maturation, ventilation
forgée/dépouillée rejetée, entrée mûrie → slash périmé évincé proprement, carte de vote vs carte
slashable).

**Portes** : **413 tests + 1 intégration / 0 échec** ; C1 128-runs byte-identique ; sweeps
conservation + émission + slashing verts ; clippy `--all-targets -D warnings` propre.
