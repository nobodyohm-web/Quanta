# Design — Saut de consensus vers DAG-BFT (finalité sous-seconde)

> **Statut : DESIGN / proposition. AUCUN code de consensus modifié.**
> Ce document est livré pour décision *avant* d'écrire la moindre ligne du
> nouveau consensus. Il fait suite à la passe « A+E+C » (post-quantique, preuves
> formelles, aléa non-grindable) qui, elle, est implémentée et vérifiée verte.

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
conservant : le ledger µQTA (burn-and-mint), les identités hybrides
Ed25519+ML-DSA, et la convergence déterministe.

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
- **`merkle_dag.rs`** — DAG content-addressed (BLAKE3) : la structure de données
  de Narwhal existe déjà en germe.
- **Gossip signé** (`gossip.rs`, 22 messages) + **Iroh QUIC** : la couche de
  dissémination fiable est là.
- **Set de validateurs PoS** (`pos_consensus.rs`) : poids = stake + réputation,
  déjà calculé — directement réutilisable comme comité BFT pondéré.
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
  sync. Impacte fortement les 265 tests actuels (ledger/seal/fork).
- **Effort estimé** : ~2–3 mois + harnais multi-nœuds + tests de chaos.

### Recommandation : **étager**.
1. **Phase 0 (prérequis)** : harnais de test **multi-nœuds in-process** +
   tests de partition/chaos (les items **D1/D2** explicitement reportés). On ne
   réécrit PAS le consensus sans pouvoir tester la convergence sous adversité.
2. **Phase 1** : **Option 1** (finality gadget) → finalité déterministe rapide,
   slashing, risque contenu. Livre 80 % de la valeur perçue (« rapide + sûr »).
3. **Phase 2** : migrer vers **Option 2** (DAG-BFT) une fois le harnais et le
   gadget éprouvés, derrière un **bump de version de protocole**.

---

## 5. Problèmes durs spécifiques à Quanta (à arbitrer)

1. **Comité & pondération** : réutiliser le poids PoS (stake + réputation) comme
   stake BFT. Quorum = 2f+1 *pondéré*. Définir f en fonction du stake, pas du
   nombre de nœuds.
2. **Accountable safety (slashing)** : l'équivocation (double-proposition au
   même round) doit être prouvable et *slashée*. Nouveau type de tx `Slash`.
3. **Agrégation de signatures** : 2f+1 signatures par bloc devient lourd.
   Piste : agrégation BLS — **mais** attention à la cohérence post-quantique
   (BLS n'est pas PQ). Compromis à étudier : seuils hybrides ou simple liste de
   signatures Ed25519+ML-DSA tant que le comité est petit.
4. **Data availability** : pour le Web P2P (pages), le DAG doit garantir la
   disponibilité avant commit (Narwhal le fait via accusés de réception).
5. **Compat ledger** : l'ordre total du DAG doit alimenter le **même**
   `cache_apply_tx` / burn-and-mint / merkle root. Le ledger ne change pas ; sa
   *source d'ordre*, si.
6. **Bump de protocole** : `TORUS_PROTOCOL_VERSION` passe à 3 ; les pairs v2 et
   v3 ne peuvent pas co-consensus → fenêtre de migration à gérer.
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

## 7. Décision attendue

Pour lancer la Phase 1, j'ai besoin que tu tranches :

1. **Périmètre** : on vise d'abord le *finality gadget* (Option 1), ou
   directement le DAG-BFT complet (Option 2) ?
2. **Prérequis** : je construis d'abord le harnais multi-nœuds + tests de chaos
   (D1/D2), ou tu veux un prototype consensus en parallèle ?
3. **Signatures** : tolère-t-on du BLS (non-PQ) pour l'agrégation au sein du
   comité, ou on reste 100 % hybride PQ quitte à des blocs plus lourds ?
4. **Compat** : bump `TORUS_PROTOCOL_VERSION` → 3 acceptable (fenêtre de
   migration v2/v3) ?

Tant que ces points ne sont pas tranchés, **aucun code de consensus ne sera
écrit** — conformément au séquençage responsable convenu.

---

## 8. Hors-périmètre de ce document
- Aucune modification de `pos_consensus.rs`, `ledger.rs`, `dispatcher.rs`,
  `mining_loop.rs` autre que celles déjà livrées en Track C (beacon d'entropie).
- La confidentialité (zk-STARK) et les light-clients restent un *track* séparé
  (D, vision), non couvert ici.
