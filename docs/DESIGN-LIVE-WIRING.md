---
type: design
status: proposé (à valider — Alexandre)
decision-class: intégration IO (aucune règle de consensus nouvelle)
socle: GADGET-3/4/5 (gadget complet) · PQ-MIG-5 (migration PQ complète) · ADR-009
updated: 2026-06-25
---

# Câblage du gadget de finalité en vivant — connecter le cœur prouvé au réseau réel

← [[00 — Pilotage QUANTA]] · cadre : [[DESIGN-FINALITY-GADGET]] (le gadget) · paramètres : [[ADR-009 — Frontière gravé-ajustable (ADR-006 ratifiée) et valeurs du §12]]
Socle : gadget complet (GADGET-3 finalité · GADGET-4 slashing · GADGET-5A/B fork-choice + résolution de partition) · migration PQ complète ([[ADR-007 — Portée du post-quantique (comptes ML-DSA)]] (b), PQ-MIG-3B → PQ-MIG-5)

> [!abstract] Statut — conception d'intégration, pas du code, **aucune règle de consensus nouvelle**
> Le gadget **tourne, prouvé en simulation déterministe** (harnais DST). Il n'est **pas câblé en
> vivant** : les votes ne circulent pas par le gossip, `LatestVotes` est **vide** (peuplé `::new()`
> au heal, GADGET-5B), le slashing opère sur un **instantané** (`apply_slash` sur un
> `&mut HashMap`). Le fil rouge **« réconciliation clé-de-vote ↔ clé-d'enjeu » est *déjà résolu*** par
> **PQ-MIG-3B** : enjeu, vote et `from`/`to` partagent **une seule** identité — l'**adresse ML-DSA**
> `BLAKE3(ADDR_DOMAIN ‖ clé)`. Ce qui reste n'est donc **pas** une réconciliation d'identités, c'est
> de l'**intégration IO** : brancher le cœur pur sur la couche réseau/ledger réelle. C'est le chantier
> le plus sérieux qui reste, et il **franchit la frontière sans-IO** — à faire sans la casser.
> **À valider par Alexandre.**

## 0. Ce qui est déjà là (le cœur prouvé — rien à réinventer)

Tout le consensus est **décidé et prouvé**. Le câblage **consomme** ces pièces, il n'en crée aucune :

| Pièce | Symbole (vérifié) | Fichier | État |
|---|---|---|---|
| Vote de finalité | `struct Vote` + `Vote::verify(stakes, epoch_len) -> bool` | `sm/finality_vote.rs` | signé **ML-DSA**, vérifié — **fait** |
| Certificat ⅔ | `meets_supermajority` (`backing×3 ≥ total×2`) | `sm/finality_vote.rs` | quorum **gravé** (ADR-009) — fait |
| Votes les plus récents | `struct LatestVotes` | `sm/fork_choice.rs` | moteur prêt, **entrées vides** |
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

## 2. Ce qu'il reste à câbler (4 connexions)

1. **Propager les votes.** Un validateur signe son `Vote` (ML-DSA, fait) et le **gossipe**. Concret :
   un **nouveau variant** `GossipMessage::FinalityVote { vote_json }` (à côté de `NewBlock`/`BroadcastTx`,
   `gossip.rs`), un **bras de dispatch** dans `dispatcher.rs` (étape ⑨ du pipeline, après la vérif
   d'enveloppe Ed25519) qui désérialise → **dé-duplique** (LRU `seen_messages` existant) → valide
   (`Vote::verify`) → remet au cœur. **Cœur inchangé ; IO testée à part.**
2. **Alimenter le fork-choice + la finalité.** Les votes reçus peuplent `LatestVotes` (5A) et, une
   fois un certificat ⅔ constitué, `FinalityState` (3) **du ledger vivant**. Le poids GHOST
   « s'active » dès que les votes coulent — `ghost_head` est **inchangé**, il attend de **vraies**
   entrées (aujourd'hui `LatestVotes::new()` vide au heal).
3. **Slashing sur ledger réel.** Une `FaultProof` (GADGET-4) gossipée (même schéma : variant +
   dispatch + dédup), une fois `detect_fault` re-confirmé, déclenche sur le ledger **vivant** un
   mouvement **STAKE → BURN** réel — une **vraie tx/preuve** scellée, pas une mutation de `HashMap`.
   Conservation via le **bilan réel** (`Σ(dépensable+staké+déverr.)+brûlé == miné`, le STAKE sink se
   vide vers BURN), pas l'instantané. **Fenêtre de preuve ≤ unbonding** (contrainte gravée, ADR-009 /
   `SLASH_EVIDENCE_WINDOW_BLOCKS`).
4. **Boucle de proposition.** Le proposeur (`mining_loop.rs` → `pos_seal_if_leader` →
   `seal_if_pending`, qui aujourd'hui scelle sur `chain.last()` — le fork-choice **intérimaire**)
   utilise `ghost_head` (5A) comme **tête**, ancrée finalité, pour bâtir le bloc suivant. La tête
   vivante devient **finalité-consciente** ; le moteur 5B (`reorg_to_fork`) gère déjà le heal.

## 3. Découpage en pièces (chirurgical, après ce design)

- **LIVE-1 — gossip des votes.** Variant `FinalityVote` + propagation + réception + dédup +
  `Vote::verify`, `LatestVotes` **peuplé depuis le réseau**. Cœur inchangé ; IO testée à part.
- **LIVE-2 — proposition finalité-consciente.** Brancher `FinalityState`/`ghost_head` du ledger
  vivant dans la boucle de proposition (remplace le `chain.last()` intérimaire ; le heal 5B est déjà
  là).
- **LIVE-3 — slashing vivant.** `FaultProof` gossipée → tx **STAKE → BURN** sur ledger réel,
  conservation **réelle**, fenêtre ≤ unbonding.

*(Ordre indicatif. Chaque pièce **prouvée** ; IO et cœur **testés séparément** ; **C1 du cœur
préservé** à chaque étape. Un `/goal` chirurgical par pièce, comme les GADGET/PQ-MIG.)*

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

> Après ce chantier, le gadget ne sera plus seulement prouvé en simulation : il **tournera**. C'est le
> pas qui fait passer Quanta de « cœur de consensus correct » à « réseau qui finalise pour de vrai ».
> **Première pièce concrète : LIVE-1** (gossip des votes).
