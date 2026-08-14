---
type: adr
id: ADR-004
status: open
decision-class: 🛑 hard-stop
updated: 2026-06-21
---

# ADR-004 — Aléa d'élection : beacon vs ECVRF + VDF

← [Registre ADR](README.md) · cadre : [DESIGN-CONSENSUS-DAG-BFT](../protocol/CONSENSUS-DAG-BFT.md) (problème dur #7)

> [!NOTE] CASCADE (2026-06-21) — beacon tolérable en Phase 1 (suite à Option 1)
> Le finality gadget (Option 1) garde l'**élection de leader PoS** inchangée (il
> **ajoute** un vote BFT, pas une nouvelle élection). Surtout, le **comité finalise
> par quorum** : un leader **prévisible** peut être ciblé/DoS, mais la **liveness
> est portée par le comité + le slashing** ([ADR-003 — Slashing (accountable safety)](ADR-003-slashing.md)),
> pas par le secret du leader. → **Recommandation : garder le beacon enterré pour
> la Phase 1** (simple, rejouable, non-grindable à court terme) ; **ECVRF
> (imprévisibilité) + VDF (anti-grinding) → Phase 2 / DAG-BFT**.
> **Reste à toi** seulement *si* tu veux l'imprévisibilité plus tôt : tolère-t-on
> une primitive **non-PQ** (ECVRF) dans le chemin consensus (même arbitrage que
> BLS, §7) ? Sinon, statu quo beacon acté.

## Contexte (code réel)
- `pos_consensus::leader_beacon(buried_block_hash, slot)` =
  `BLAKE3(domaine ‖ hash_bloc_enterré ‖ slot)`. Le bloc enterré est à
  `LEADER_ENTROPY_LOOKBACK = 2` derrière le tip.
- **Déterministe et publiquement prévisible** : aucune clé secrète. la référence technique
  est explicite — « élection *déterministe publiquement vérifiable*, **pas** un
  VRF cryptographique ». Le leader de chaque slot est **calculable d'avance** par
  tous.
- Le bloc **enterré** bloque le *grinding immédiat* (le sceleur du tip ne choisit
  pas l'aléa du prochain slot). **Pas de VDF.**

## Conséquences de l'état actuel
- **Prévisibilité** → un leader connu à l'avance est **ciblable** (DoS/eclipse
  juste avant son slot) ; collusion plus facile.
- **Grinding résiduel** : sur un horizon > LOOKBACK, un gros stakeholder peut
  explorer des futurs (rétention/choix de blocs) pour biaiser de futurs slots —
  non fermé sans **VDF**.
- Couple avec [ADR-001 — Fork-choice](ADR-001-fork-choice.md) : « hash le plus haut » est grindable.

## Options
1. **Garder le beacon enterré (statu quo)** — *défaut possible pour Phase 0.*
   - + simple, déterministe, rejouable (idéal harnais), non-grindable à court terme.
   - − prévisible (ciblage du leader), grinding long-horizon ouvert.
2. **Vrai VRF (ECVRF, RFC 9381)** — chaque validateur tire un VRF avec **sa clé**
   ; l'éligibilité dépend d'une sortie **imprévisible** jusqu'à révélation.
   - + leader **imprévisible** (anti-ciblage, anti-collusion).
   - − nouvelle primitive crypto + clés ; **cohérence post-quantique** à arbitrer
     (ECVRF classique n'est pas PQ — comme le BLS du design #3) ; casse la pure
     vérifiabilité publique « tout le monde recalcule ».
3. **Beacon + VDF** — un VDF (délai vérifiable) au-dessus du beacon ferme le
   grinding/rétention (design #7) sans clé secrète.
   - + anti-grinding fort, garde le déterminisme/vérifiabilité.
   - − ne corrige **pas** la prévisibilité ; impl/calibrage VDF non triviaux.
4. **ECVRF + VDF** — imprévisibilité **et** anti-grinding (état de l'art).
   - + le plus robuste.
   - − le plus lourd ; PQ à trancher.

## Contraintes croisées
- **Imprévisibilité ↔ propagation** : un leader imprévisible change ce que le
  **réseau virtuel** (T0.5) doit modéliser sur le timing — relié à la décision
  **transport-flood** ([AUDIT_QUANTA_2_PROGRESS](../archive/journals/AUDIT_QUANTA_2_PROGRESS.md), C8).
- **PQ** : ECVRF/BLS non-PQ ⇒ même arbitrage que le design §7 (tolère-t-on du
  non-PQ dans le chemin consensus, ou 100 % hybride ?).
- Sans imprévisibilité, le **slashing** ([ADR-003 — Slashing (accountable safety)](ADR-003-slashing.md))
  et un fork-choice Sybil-ancré ([ADR-001 — Fork-choice](ADR-001-fork-choice.md)) portent seuls la sûreté.

## Statut & ce dont j'ai besoin de toi (🛑)
Pour la Phase 0/1 : on **reste** au beacon enterré (prévisible mais simple et
rejouable), ou on investit dans **VDF** (anti-grinding) et/ou **ECVRF**
(imprévisibilité) ? Et : **tolère-t-on une primitive non-PQ** (ECVRF) dans le
chemin de consensus, ou exige-t-on du 100 % hybride PQ ?
