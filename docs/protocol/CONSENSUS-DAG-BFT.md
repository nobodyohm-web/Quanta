---
type: design
updated: 2026-07-12
---

# Design — Saut de consensus vers DAG-BFT (finalité sous-seconde)

> **Statut : Phase 0 + Phase 1 livrées (gadget de finalité prouvé en simulation, 2026-06-25 ;
> gossip des votes câblé en vivant, LIVE-1) ; Phase 2 (DAG-BFT, ce document) non lancée.**
> Voir [DESIGN-FINALITY-GADGET](FINALITY-GADGET.md) pour la Phase 1 réalisée et [DESIGN-LIVE-WIRING](LIVE-WIRING.md) pour le
> câblage réseau. Ce document ne couvre que la **Phase 2** (Option 2, DAG-BFT complet), qui reste
> à l'état de conception — *aucune ligne de code DAG-BFT écrite*. Il fait suite à la passe « A+E+C »
> (post-quantique, preuves formelles, aléa non-grindable) qui, elle, est implémentée et vérifiée
> verte.

---

## 1. Pourquoi

### État actuel de Quanta
- **PoS + VRF, mono-leader** (`pos_consensus.rs`) : un seul proposeur élu par slot.
- **Cadence de scellage** : `SEAL_EVERY_N_TICKS = 2` × `MINE_INTERVAL = 60 s` →
  un bloc **toutes les ~2 minutes**.
- **Finalité** : *probabiliste* (autorité du leader PoS + plus-longue-chaîne),
  réorganisable par `fork reorg` déterministe. Pas de finalité BFT prouvée.
- **Débit** : ~10 tx/bloc cible → ~7 200 tx/jour (`network-quality.md`).

### Le plafond
Le mono-leader est à la fois le goulot de **débit** (un seul proposeur à la
fois) et un risque de **liveness** (si le leader est lent/offline → fallback de
30 s). La finalité en *minutes* est l'écart le plus visible avec l'état de l'art.

### La cible
Finalité **sous-seconde**, **proposeurs en parallèle**, débit ×100+, tout en
conservant : le ledger µQTA (burn-and-mint), les identités de compte **pur
ML-DSA** (PQ-MIG-3B ; Ed25519 = transport gossip uniquement), et la convergence
déterministe.

---

## 2. L'état de l'art (2021-2026) — ce qu'on copie

| Protocole | Idée clé | Perf publiée | Réf |
|-----------|----------|--------------|-----|
| **Narwhal / Tusk** (2021) | Séparer *dissémination* (mempool DAG) de *l'ordonnancement* | 130–160k tx/s | EuroSys '22 |
| **Bullshark** (2022) | Ordonnancement DAG partiellement synchrone sur Narwhal | 297k tx/s, ~2 s | CCS '22 |
| **Shoal++** (2024) | Réduit la latence DAG-BFT (~4,5 vs ~10,5 échanges) | −60 % latence | arXiv 2405.x |
| **Mysticeti** (2023→2024) | **DAG non-certifié**, commit en **3 rounds** (borne basse) | **~390 ms** consensus, **~640 ms** finalité, **200k+ tx/s** | arXiv 2310.14821 ; Sui mainnet, juil. 2024 |

**Leçon centrale (Narwhal)** : découpler *« qui a reçu quelles transactions »*
(dissémination fiable, parallélisable, le DAG) de *« dans quel ordre »*
(consensus, léger). C'est ce découplage qui débloque le débit. **Mysticeti**
montre qu'on peut en plus atteindre la **borne basse de latence** (3 rounds) en
abandonnant les certificats explicites.

---

## 3. Pourquoi Quanta est bien placé

Le codebase a déjà les briques de base :
- **`merkle_dag.rs`** — DAG content-addressed (BLAKE3) : **supprimée le
  2026-06-20** (refonte crypto-only, retrait des modules web/social/DAG de
  contenu) — à réécrire pour la Phase 2 ; ce n'est plus une brique existante.
- **Gossip signé** (`gossip.rs`, 10 messages) + **Iroh QUIC** : la couche de
  dissémination fiable est là.
- **Set de validateurs PoS** (`pos_consensus.rs`) : poids = **enjeu inscrit sur
  la chaîne** (`Ledger::validator_stakes()`, ADR-002 — la réputation est hors
  chemin de sécurité depuis STAKE-WEIGHT-1), déjà calculé — directement
  réutilisable comme comité BFT pondéré.
- **Beacon d'aléa non-grindable** (Track C) : utilisable pour la sélection
  d'ancre (anchor) dans le commit DAG.
- **Ledger déterministe µQTA** : l'ordre total produit par le DAG-BFT alimente
  le même `apply`/`seal` qu'aujourd'hui.

---

## 4. Deux options (avec recommandation)

### Option 1 — *Finality gadget* par-dessus la chaîne linéaire (incrémental)
On **garde** la chaîne linéaire + leader PoS, et on **ajoute** une phase de
vote BFT (style Tendermint/Jolteon, 2 rounds) qui **finalise** un bloc en
quelques secondes au lieu d'attendre la profondeur probabiliste.

- **Gain** : finalité *déterministe* en ~secondes ; slashing d'équivocation
  possible. Débit inchangé (toujours mono-leader).
- **Coût/risque** : MOYEN. Ajoute un comité de votes + agrégation de signatures,
  mais ne touche pas la structure de la chaîne.
- **Effort estimé** : ~2–3 semaines + harnais multi-nœuds.

### Option 2 — DAG-BFT complet (style Mysticeti) (cible)
On **remplace** la chaîne linéaire par un **DAG round-based** : chaque
validateur propose un bloc par round (proposeurs parallèles) ; une règle de
commit déterministe (sélection d'ancre via le beacon) produit l'ordre total.

- **Gain** : finalité ~sous-seconde, débit ×100+, plus de goulot mono-leader,
  liveness bien meilleure.
- **Coût/risque** : ÉLEVÉ. Réécrit le cœur du consensus + le format de bloc + la
  sync. Impacte fortement la suite de tests actuelle (379 tests, ledger/seal/fork).
- **Effort estimé** : ~2–3 mois + harnais multi-nœuds + tests de chaos.

### Recommandation : **étager**.
1. **Phase 0 (prérequis)** : harnais de test **multi-nœuds in-process** +
   tests de partition/chaos (les items **D1/D2** explicitement reportés). On ne
   réécrit PAS le consensus sans pouvoir tester la convergence sous adversité.
2. **Phase 1** : **Option 1** (finality gadget) → finalité déterministe rapide,
   slashing, risque contenu. Livre 80 % de la valeur perçue (« rapide + sûr »).
   → **Conception détaillée** : [DESIGN-FINALITY-GADGET](FINALITY-GADGET.md) (style Casper FFG, votes ML-DSA
   post-quantiques, finalisation par époque ; **implémentée et prouvée en simulation DST,
   2026-06-25**). Les 4 méta-décisions §7 ci-dessous sont tranchées en ADR-001→005 + ADR-009
   (`docs/decisions/`) — voir §7 pour le registre des décisions prises.
3. **Phase 2** : migrer vers **Option 2** (DAG-BFT) une fois le harnais et le
   gadget éprouvés, derrière un **bump de version de protocole**.

---

## 5. Problèmes durs spécifiques à Quanta (à arbitrer)

1. **Comité & pondération** : réutiliser le poids PoS (**enjeu inscrit sur la chaîne**,
   `Ledger::validator_stakes()`, ADR-002 — la réputation est hors chemin de sécurité depuis
   STAKE-WEIGHT-1) comme stake BFT. Quorum = 2f+1 *pondéré*. Définir f en fonction du stake, pas
   du nombre de nœuds.
2. **Accountable safety (slashing)** : l'équivocation (double-proposition au
   même round) doit être prouvable et *slashée*. Nouveau type de tx `Slash`.
3. **Agrégation de signatures** : 2f+1 signatures par bloc devient lourd.
   Piste : agrégation BLS — **mais** attention à la cohérence post-quantique
   (BLS n'est pas PQ). Compromis à étudier : seuils hybrides ou simple liste de
   signatures Ed25519+ML-DSA tant que le comité est petit.
4. **Data availability** : pour les transactions (le module Web P2P/pages a été retiré le
   2026-06-20, refonte crypto-only — plus de périmètre), le DAG doit garantir la disponibilité
   avant commit (Narwhal le fait via accusés de réception).
5. **Compat ledger** : l'ordre total du DAG doit alimenter le **même**
   `cache_apply_tx` / burn-and-mint / merkle root. Le ledger ne change pas ; sa
   *source d'ordre*, si.
6. **Bump de protocole** : **réalisé** — `TORUS_PROTOCOL_VERSION = 4` est en vigueur
   (`p2p/gossip.rs:32`) ; les pairs d'anciennes versions ne peuvent pas co-consensus, la fenêtre de migration
   a déjà été gérée pour ce bump. Un futur bump vers Phase 2 (DAG-BFT) en nécessitera un nouveau.
7. **Aléa d'ancre** : la sélection d'ancre/leader de round réutilise le **beacon
   non-grindable** (Track C) ; un **VDF** par-dessus fermerait la rétention de
   bloc (le dernier item d'inviolabilité de l'aléa).

---

## 6. Stratégie de test (non négociable)

- **Prérequis D1/D2** : harnais multi-nœuds in-process (`tokio` tasks +
  transport mémoire) ; scénarios : convergence n-nœuds, partition réseau,
  nœuds byzantins (équivocation, rétention), churn de validateurs.
- **Invariants à prouver** (extension du Track E) : *agreement* (deux nœuds
  corrects ne committent jamais d'ordres contradictoires), *validity*,
  *termination* sous synchronie partielle, conservation µQTA préservée.
- **Property-based** : ordre total déterministe pour un même DAG sur tous les
  nœuds.

---

## 7. Décisions prises (registre) — Phase 1 lancée et livrée

Ces quatre points, posés pour lancer la Phase 1, sont **tranchés** :

1. **Périmètre** : **Option 1** (finality gadget) d'abord — livrée, `src-tauri/src/sm/`
   (GADGET-1→5B). Voir [DESIGN-FINALITY-GADGET](FINALITY-GADGET.md).
2. **Prérequis** : le harnais de test **d'abord** (Phase 0, DST) — livré (commit dd3ad99,
   « baseline Phase 0 »), utilisé pour prouver le gadget avant tout câblage réseau.
3. **Signatures** : **100 % ML-DSA, sans BLS** — tranché en
   [ADR-005 — Agrégation des votes & certificats de finalité](../decisions/ADR-005-vote-aggregation.md) ; les certificats de finalité sont
   un ensemble de votes ML-DSA (~165 Ko / 50 validateurs), pas d'agrégation BLS.
4. **Compat** : le protocole est déjà à **`TORUS_PROTOCOL_VERSION = 4`** (`gossip.rs`, 3→4 par LIVE-3B) — la
   fenêtre de migration v2/v3 est du passé.

Référence : [ADR-001 — Fork-choice](../decisions/ADR-001-fork-choice.md) · [ADR-002 — Validator set & comité BFT](../decisions/ADR-002-validator-set.md) ·
[ADR-003 — Slashing (accountable safety)](../decisions/ADR-003-slashing.md) · [ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)](../decisions/ADR-004-election-randomness.md) ·
[ADR-005 — Agrégation des votes & certificats de finalité](../decisions/ADR-005-vote-aggregation.md) ·
[ADR-009 — Frontière gravé-ajustable (ADR-006 ratifiée) et valeurs du §12](../decisions/ADR-009-carved-vs-adjustable.md) ; commits dd3ad99
(baseline Phase 0) et 4d4fd63 (gadget + migration PQ complète).

La **Phase 2** (ce document, DAG-BFT complet) reste, elle, **non lancée** : aucune de ces
décisions ne couvre le saut vers un DAG round-based — ce sera un nouveau cycle de décision le
moment venu.

---

## 8. Hors-périmètre de ce document
- Aucune modification de `pos_consensus.rs`, `ledger.rs`, `dispatcher.rs`,
  `mining_loop.rs` autre que celles déjà livrées en Track C (beacon d'entropie).
- La confidentialité (zk-STARK) et les light-clients restent un *track* séparé
  (D, vision), non couvert ici.
