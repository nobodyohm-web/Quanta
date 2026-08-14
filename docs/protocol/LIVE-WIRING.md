---
type: design
status: LIVE-1/LIVE-2/LIVE-3 implémentés — câblage vivant du gadget COMPLET
decision-class: intégration IO (aucune règle de consensus nouvelle)
socle: GADGET-3/4/5 (gadget complet) · PQ-MIG-5 (migration PQ complète) · ADR-009
updated: 2026-07-12
---

# Câblage du gadget de finalité en vivant — connecter le cœur prouvé au réseau réel

← `00 — Pilotage QUANTA` · cadre : [DESIGN-FINALITY-GADGET](FINALITY-GADGET.md) (le gadget) · paramètres : [ADR-009 — Frontière gravé-ajustable (ADR-006 ratifiée) et valeurs du §12](../decisions/ADR-009-carved-vs-adjustable.md)
Socle : gadget complet (GADGET-3 finalité · GADGET-4 slashing · GADGET-5A/B fork-choice + résolution de partition) · migration PQ complète ([ADR-007 — Portée du post-quantique (comptes ML-DSA)](../decisions/ADR-007-post-quantum-scope.md) (b), PQ-MIG-3B → PQ-MIG-5)

> [!NOTE] Statut — LIVE-1, LIVE-2, LIVE-3 tous livrés ; **aucune règle de consensus nouvelle**
> Le gadget **tourne, prouvé en simulation déterministe** (harnais DST), et **tourne désormais aussi
> en vivant** : **LIVE-1 est implémenté** — les votes circulent par le gossip
> (`GossipMessage::FinalityVote`, bras de dispatch, `FinalityTracker` dans
> `src-tauri/src/p2p/finality_live.rs`, émission depuis `mining_loop.rs`, pont
> `Ledger::validator_stakes_by_pubkey`). `LatestVotes` se peuple **depuis le réseau réel**.
> **LIVE-2 est implémenté** — le **plancher de finalité** (`Ledger::finalized_floor_index`, monotone,
> tip-clampé, persisté au snapshot) est branché dans `integrate_remote_block` : tout fork à hauteur
> ≤ plancher est **refusé**, le départage libre (lexicographique) ne s'applique **qu'au-dessus** du
> plancher (Gasper : libre au-dessus, gelé à/sous la finalité) — une **garde de sûreté pure** (aucun
> solde muté). Note honnête : le design initial envisageait LIVE-2 comme « le proposeur bâtit sur
> `ghost_head` au lieu de `chain.last()` » ; la version livrée atteint le **même objectif de
> sûreté** (la finalité protège la chaîne vivante) par ce veto de plancher dans la résolution de
> fork — le raffinement de *timing* actif piloté par `ghost_head` reste une option future, pas
> nécessaire à la garantie d'irréversibilité. **LIVE-3 est implémenté** — le slashing tourne sur le
> ledger réel : `TxType::Slash` + `fault_proof` embarqué, `detect_fault` à l'ingest d'un vote →
> `GossipMessage::FinalityFault` → tx `Slash` scellée dans le prochain bloc, mouvement **STAKE →
> BURN** conservation-neutre par construction, et `verify_block_slashes` re-vérifie chaque slash sur
> chaque nœud (preuve réelle, adresse de l'offenseur, montant ratifié) — un proposeur malveillant ne
> peut pas punir un validateur innocent. Le fil rouge **« réconciliation clé-de-vote ↔ clé-d'enjeu »
> était *déjà résolu*** par **PQ-MIG-3B** : enjeu, vote et `from`/`to` partagent **une seule**
> identité — l'**adresse ML-DSA** `BLAKE3(ADDR_DOMAIN ‖ clé)`. Le câblage n'était donc **pas** une
> réconciliation d'identités, c'était de l'**intégration IO** : brancher le cœur pur sur la couche
> réseau/ledger réelle, en franchissant la frontière sans-IO sans la casser — fait.

## 0. Ce qui est déjà là (le cœur prouvé — rien à réinventer)

Tout le consensus est **décidé et prouvé**. Le câblage **consomme** ces pièces, il n'en crée aucune :

| Pièce | Symbole (vérifié) | Fichier | État |
|---|---|---|---|
| Vote de finalité | `struct Vote` + `Vote::verify(stakes, epoch_len) -> bool` | `sm/finality_vote.rs` | signé **ML-DSA**, vérifié — **fait** |
| Certificat ⅔ | `meets_supermajority` (`backing×3 ≥ total×2`) | `sm/finality_vote.rs` | quorum **gravé** (ADR-009) — fait |
| Votes les plus récents | `struct LatestVotes` | `sm/fork_choice.rs` | moteur prêt, **peuplé depuis le réseau réel (LIVE-1)** |
| Fork-choice GHOST | `ghost_head(tree, latest, stakes, justifié, finalisé)` · `anchors(&FinalityState)` | `sm/fork_choice.rs` | **GADGET-5A**, pur — fait |
| État de finalité | `struct FinalityState` · `apply_certificate` · `justified()` · `finalized()` | `sm/finality_rule.rs` | **GADGET-3** — fait |
| Détection de faute | `enum Fault` · `detect_fault(a, b) -> Option<Fault>` · `struct FaultProof` | `sm/finality_slashing.rs` | **GADGET-4** — fait |
| Application du slash | `apply_slash(&mut stakes, offender) -> SlashOutcome` | `sm/finality_slashing.rs` | opère sur **instantané** — à porter au ledger réel |

> **Une seule identité (PQ-MIG-3B).** Le votant d'un `Vote`, le `from` d'une tx, et la clé d'enjeu
> dans `validator_stakes()` sont la **même adresse ML-DSA**. Donc `Vote::verify` (qui pèse le votant
> par `stakes[votant]`) et le slashing (qui débite `staked[offender]`) parlent du **même** compte,
> sans table de correspondance. Le « gros morceau » annoncé à PQ-MIG-5 n'est plus une réconciliation
> d'identités — il est **dissous**.

## 1. Le principe (et le risque)

- Le cœur `src/sm/` reste **pur et déterministe** — sans-IO, le sceau du projet (Constitution §3 ;
  C1 = `determinism_meta_test_128_runs_are_byte_identical`). La couche IO (`dispatcher.rs`,
  `gossip.rs`, `p2p/ledger.rs`, `mining_loop.rs`) **appelle** le cœur : elle apporte le réseau et le
  temps réel, le cœur garde le **verdict déterministe**.
- **Risque n°1** — ne pas laisser le non-déterminisme (ordre réseau, horloge, `OsRng`, ordre
  d'itération `HashMap`) **fuir** dans le verdict de consensus. C1 doit rester vrai **pour le cœur** ;
  l'IO est testée **séparément**. La discipline qui a déjà tenu (votes/blocs ordonnés → structures
  `BTree` → verdict ; temps **injecté** au bord, jamais relu pour valider) reste la règle.

## 2. Ce qui a été câblé (3 connexions sur 3 — LIVE-1, LIVE-2, LIVE-3 faites)

1. ~~**Propager les votes.**~~ **Fait (LIVE-1).** Un validateur signe son `Vote` (ML-DSA) et le
   **gossipe** : `GossipMessage::FinalityVote { vote_json }` (`gossip.rs`), un **bras de dispatch**
   dans `dispatcher.rs` (étape ⑨ du pipeline, après la vérif d'enveloppe Ed25519) qui désérialise →
   **dé-duplique** (LRU `seen_messages` existant) → valide (`Vote::verify`) → remet au
   `FinalityTracker` (`p2p/finality_live.rs`). Émission depuis `mining_loop.rs`. **Cœur inchangé ;
   IO testée à part.**
2. ~~**Alimenter le fork-choice + la finalité.**~~ **Fait.** Les votes reçus peuplent `LatestVotes`
   (5A, LIVE-1) et, une fois un certificat ⅔ constitué, `Ledger::set_finalized_floor` (LIVE-2)
   **du ledger vivant**.
3. ~~**Slashing sur ledger réel (LIVE-3).**~~ **Fait.** Une `FaultProof` (GADGET-4) gossipée
   (`GossipMessage::FinalityFault`, même schéma : variant + dispatch + dédup), une fois
   `detect_fault` re-confirmé à l'ingest du vote, déclenche sur le ledger **vivant** un mouvement
   **STAKE → BURN** réel — une **vraie tx** (`TxType::Slash` + `fault_proof` embarqué) scellée dans
   le prochain bloc, pas une mutation de `HashMap`. Conservation via le **bilan réel**
   (`Σ(dépensable+staké+déverr.)+brûlé == miné`, le STAKE sink se vide vers BURN), pas l'instantané.
   `verify_block_slashes` re-vérifie chaque slash (preuve, adresse offenseur, montant ratifié) sur
   chaque nœud avant application — pas de punition d'un innocent.
4. ~~**Boucle de proposition (LIVE-2).**~~ **Fait, via le plancher de finalité.** Le proposeur
   (`mining_loop.rs` → `pos_seal_if_leader` → `seal_if_pending`) continue de sceller au sommet
   courant, mais la **résolution de fork** (`integrate_remote_block`) refuse désormais tout bloc
   concurrent à hauteur ≤ `finalized_floor_index` — l'histoire finalisée est **irréversible** sur le
   réseau vivant. Le départage reste **libre au-dessus** du plancher (Gasper). Le raffinement actif
   « bâtir directement sur `ghost_head` » (timing de reorg, pas de garantie de sûreté supplémentaire)
   reste une option future ; le moteur 5B (`reorg_to_fork`) gère déjà le heal au-dessus du plancher.

## 3. Découpage en pièces (chirurgical, après ce design)

- **LIVE-1 — gossip des votes. ✅ Fait.** Variant `FinalityVote` + propagation + réception + dédup +
  `Vote::verify`, `LatestVotes` **peuplé depuis le réseau** via `FinalityTracker`
  (`src-tauri/src/p2p/finality_live.rs`), émission dans `mining_loop.rs`, pont
  `Ledger::validator_stakes_by_pubkey`. Cœur inchangé ; IO testée à part.
- **LIVE-2 — plancher de finalité vivant. ✅ Fait.** `Ledger::finalized_floor_index` (monotone,
  tip-clampé, persisté au snapshot), alimenté par les certificats ⅔ (dispatcher +
  mining-loop) via `set_finalized_floor` ; veto absolu dans `integrate_remote_block` (refuse un fork
  ≤ plancher, départage libre au-dessus — Gasper). Livré comme **garde de sûreté** dans le chemin de
  résolution de fork (l'objectif d'irréversibilité de la conception initiale), et non comme un
  changement de la cible de scellement du proposeur — cette dernière reste un raffinement optionnel.
- **LIVE-3 — slashing vivant. ✅ Fait.** `TxType::Slash` + `fault_proof`, `FaultProof` gossipée
  (`GossipMessage::FinalityFault`) → tx **STAKE → BURN** sur ledger réel, conservation-neutre par
  construction, `verify_block_slashes` partagé seal↔réception (COVER-2) empêche de punir un
  innocent.

*(Chaque pièce **prouvée** ; IO et cœur **testés séparément** ; **C1 du cœur préservé** à chaque
étape. Un `/goal` chirurgical par pièce, comme les GADGET/PQ-MIG.)*

## 4. La frontière sans-IO — comment ne pas la casser

- **Sens unique.** L'IO **construit** l'objet typé (un `Vote` désérialisé+vérifié, une `FaultProof`)
  et **appelle** une fonction de cœur **pure** qui rend le verdict (tête, certificat, faute). Le cœur
  ne lit **jamais** l'horloge / `OsRng` / l'ordre réseau ; le temps réel entre comme **donnée
  injectée** au bord (le motif `*_at(ts)` existant).
- **Déterminisme du verdict.** Le verdict traverse des structures **ordonnées** (`BTreeMap`/`BTreeSet`
  dans `fork_choice`/`finality_*`) ; aucun ordre d'itération `HashMap` ne le touche. Deux nœuds aux
  **mêmes** votes+blocs ⇒ **même** verdict (la propriété 5A/5B).
- **Tests séparés.** Le cœur garde **C1** (méta-test 128 runs) ; l'IO (dé-dup, ordre des messages,
  pairs malveillants au transport) est testée par le **harnais réseau** (`NetFaults`, T0.5/6) — pas
  mélangée au verdict.

## 5. Limites honnêtes

- C'est de l'**ingénierie réseau sérieuse**, pas une formalité : entre « prouvé en simulation » et
  « tournant sur un vrai réseau », il y a l'**ordre des messages**, les **pairs malveillants au
  transport**, et la **non-régression du déterminisme** du cœur.
- Le **transport reste Ed25519** (différé, conception PQ §6 : l'enveloppe gossip + `PeerId`) — **hors
  chemin de valeur** (la valeur est **pur ML-DSA** depuis PQ-MIG-3B), à passer en PQ plus tard si
  voulu.
- Ceci **n'invente aucune règle de consensus** : tout le consensus est déjà **décidé et prouvé**.
  C'est purement de la **connexion** du cœur prouvé au monde réel.

> Le gadget n'est plus seulement prouvé en simulation : il **tourne**. C'est le pas qui fait passer
> Quanta de « cœur de consensus correct » à « réseau qui finalise pour de vrai ». **Le câblage vivant
> est complet : LIVE-1 (gossip des votes), LIVE-2 (plancher de finalité vivant dans le fork-choice)
> et LIVE-3 (slashing vivant, accountable safety) sont tous les trois livrés.** Le gadget de finalité
> tourne sur le réseau réel, pas seulement en simulation déterministe.
