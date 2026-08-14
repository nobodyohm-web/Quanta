---
type: adr
id: ADR-003
status: accepted
decision-class: 🛑 hard-stop
decided: 2026-06-25
updated: 2026-07-12
---

# ADR-003 — Slashing (accountable safety)

← [Registre ADR](README.md) · cadre : [DESIGN-CONSENSUS-DAG-BFT](../protocol/CONSENSUS-DAG-BFT.md) (problème dur #2)

> [!NOTE] CASCADE (2026-06-21) — **in-scope Phase 1** (suite à Option 1 + stake on-chain)
> Le finality gadget (Option 1) **rend le slashing possible** (équivocation =
> deux votes/blocs signés au même round → preuve). Et [ADR-002 — Validator set & comité BFT](ADR-002-validator-set.md)
> ayant tranché **stake on-chain seul**, on slashe du **stake on-chain** (propre,
> déterministe). Donc l'**option 1** (slashing d'équivocation) est la direction ;
> il **reste à toi** : **brûlé** (rareté) vs **redistribué au délateur** (incitation),
> le **montant** (% du stake), et la **fenêtre** de soumission de preuve.

> [!TIP] RÉSOLU (2026-06-25) — Option 1 retenue et **implémentée** (GADGET-4)
> Politique fixée par **ADR-009** : **brûlé** (`SLASH_BURN = true`), montant **plein**
> (`SLASH_NUM/DEN = 1/1`), **fenêtre = période d'unbonding**
> (`SLASH_EVIDENCE_WINDOW_BLOCKS = UNBONDING_PERIOD_BLOCKS`, const-assert gravée).
> Détection de faute, preuve et pénalité **implémentées** dans
> `src-tauri/src/sm/finality_slashing.rs` (`detect_fault`, `FaultProof`, `apply_slash`) —
> double vote **et** surround vote. Reste le **câblage vivant** sur le ledger réel
> (LIVE-3, STAKE→BURN).

## Contexte (code réel)
- **Aucun slashing aujourd'hui.** la référence technique du dépôt le dit honnêtement : « slashing de
  l'équivocation (absent aujourd'hui) … au roadmap ».
- L'élection est **publiquement prévisible** (beacon enterré, pas de clé secrète
  → cf. [ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)](ADR-004-election-randomness.md)). Un leader peut
  **équivoquer** (sceller deux blocs au même slot) **sans coût**.
- Sanction existante : seulement `ReportPeer` → ban réseau 1 h (3 reports). C'est
  du **rate-limit social**, pas de l'**accountable safety** (aucune perte de stake,
  manipulable par 3 attaquants coordonnés).

## Ce que le simulateur force
Les scénarios byzantins (équivocation, double-seal, rétention) sont au programme
du harnais ([QUANTA_T0_DST_HARNESS](../archive/specs/QUANTA_T0_DST_HARNESS.md) / design §6). Pour **tester** la punition,
il faut une règle de slashing… ou décider explicitement qu'il n'y en a pas (et
documenter le compromis sûreté).

## Options
1. **Slashing d'équivocation (Phase 1, avec le finality gadget)** *(direction du design)* —
   nouveau type de tx **`Slash`** : une **preuve** = deux blocs/votes signés par la
   **même** clé au **même** round. N'importe quel nœud la soumet ; le coupable perd
   du stake (brûlé ou redistribué).
   - + dissuasion réelle, *accountable safety* prouvable, indispensable au BFT.
   - − exige le **stake on-chain** ([ADR-002 — Validator set & comité BFT](ADR-002-validator-set.md)),
     une preuve canonique, et une politique (montant brûlé vs redistribué ; fenêtre
     de soumission ; protection contre fausses preuves).
2. **Pas de slashing — sûreté par finalité seule** — on s'appuie sur le quorum BFT
   (un équivoquant ne peut pas faire finaliser deux blocs contradictoires) sans
   **punir** économiquement.
   - + plus simple, pas de tx `Slash`.
   - − pas de dissuasion ; l'équivocation reste « gratuite » (spam, tentatives de
     fork pré-finalité, grinding) ; liveness dégradée sous adversité.
3. **Statu quo (ReportPeer)** — explicitement insuffisant ; à acter comme dette.

## Contraintes croisées
- **Dépend de** [ADR-002 — Validator set & comité BFT](ADR-002-validator-set.md) (on ne slashe que du
  stake on-chain) et du finality gadget (l'équivocation se définit par round/vote).
- La **preuve** d'équivocation doit être **déterministe et rejouable** (compat
  harnais §3) ; un montant brûlé impacte la conservation µQTA → property-test à
  étendre.
- Politique économique → [TOKENOMICS_V2](../archive/design-notes/TOKENOMICS_V2.md) (brûler renforce la rareté ;
  redistribuer récompense le délateur).

## Statut & ce dont j'ai besoin de toi (🛑)

✅ **Résolu (2026-06-25).** Option 1 (slashing d'équivocation) retenue **et implémentée**
(GADGET-4, `sm/finality_slashing.rs`). Politique fixée par **ADR-009** :
**brûlé** (`SLASH_BURN = true`), montant **plein** (`SLASH_NUM/DEN = 1/1`), **fenêtre =
unbonding** (`SLASH_EVIDENCE_WINDOW_BLOCKS = UNBONDING_PERIOD_BLOCKS`, const-assert). Il ne
reste que le **câblage vivant** (LIVE-3 : appliquer `apply_slash` — STAKE→BURN — sur le ledger
réel, pas seulement dans la machine à états `sm/`).

*(Questions d'origine, désormais tranchées : Slashing d'équivocation **oui/non** pour la
Phase 1 ? → oui. Brûlé ou redistribué ? → brûlé. Montant ? → plein. Fenêtre de soumission de
preuve ? → fenêtre d'unbonding.)*
