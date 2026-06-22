---
type: adr
id: ADR-003
status: open
decision-class: 🛑 hard-stop
updated: 2026-06-21
---

# ADR-003 — Slashing (accountable safety)

← [[README|Registre ADR]] · cadre : [[DESIGN-CONSENSUS-DAG-BFT]] (problème dur #2)

> [!info] CASCADE (2026-06-21) — **in-scope Phase 1** (suite à Option 1 + stake on-chain)
> Le finality gadget (Option 1) **rend le slashing possible** (équivocation =
> deux votes/blocs signés au même round → preuve). Et [[ADR-002 — Validator set & comité BFT]]
> ayant tranché **stake on-chain seul**, on slashe du **stake on-chain** (propre,
> déterministe). Donc l'**option 1** (slashing d'équivocation) est la direction ;
> il **reste à toi** : **brûlé** (rareté) vs **redistribué au délateur** (incitation),
> le **montant** (% du stake), et la **fenêtre** de soumission de preuve.

## Contexte (code réel)
- **Aucun slashing aujourd'hui.** [[CLAUDE]] le dit honnêtement : « slashing de
  l'équivocation (absent aujourd'hui) … au roadmap ».
- L'élection est **publiquement prévisible** (beacon enterré, pas de clé secrète
  → cf. [[ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)]]). Un leader peut
  **équivoquer** (sceller deux blocs au même slot) **sans coût**.
- Sanction existante : seulement `ReportPeer` → ban réseau 1 h (3 reports). C'est
  du **rate-limit social**, pas de l'**accountable safety** (aucune perte de stake,
  manipulable par 3 attaquants coordonnés).

## Ce que le simulateur force
Les scénarios byzantins (équivocation, double-seal, rétention) sont au programme
du harnais ([[QUANTA_T0_DST_HARNESS]] / design §6). Pour **tester** la punition,
il faut une règle de slashing… ou décider explicitement qu'il n'y en a pas (et
documenter le compromis sûreté).

## Options
1. **Slashing d'équivocation (Phase 1, avec le finality gadget)** *(direction du design)* —
   nouveau type de tx **`Slash`** : une **preuve** = deux blocs/votes signés par la
   **même** clé au **même** round. N'importe quel nœud la soumet ; le coupable perd
   du stake (brûlé ou redistribué).
   - + dissuasion réelle, *accountable safety* prouvable, indispensable au BFT.
   - − exige le **stake on-chain** ([[ADR-002 — Validator set & comité BFT]]),
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
- **Dépend de** [[ADR-002 — Validator set & comité BFT]] (on ne slashe que du
  stake on-chain) et du finality gadget (l'équivocation se définit par round/vote).
- La **preuve** d'équivocation doit être **déterministe et rejouable** (compat
  harnais §3) ; un montant brûlé impacte la conservation µQTA → property-test à
  étendre.
- Politique économique → [[TOKENOMICS_V2]] (brûler renforce la rareté ;
  redistribuer récompense le délateur).

## Statut & ce dont j'ai besoin de toi (🛑)
Slashing d'équivocation **oui/non** pour la Phase 1 ? Si oui : **brûlé** (rareté)
ou **redistribué au délateur** (incitation), quel **montant** (% du stake), et
quelle **fenêtre** de soumission de preuve ? Si non : on grave le compromis
« sûreté par finalité, équivocation non punie » dans cette ADR.
