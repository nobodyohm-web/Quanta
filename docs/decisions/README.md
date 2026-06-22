---
type: adr-index
updated: 2026-06-21
---

# 🗳️ Décisions d'architecture (ADR)

Registre des arbitrages **consensus/sécurité**. Règle d'arrêt §4 de la
[[QUANTA_AGENT_CONSTITUTION]] : Claude **ne devine pas** un arbitrage — il le
**cadre** (options + conséquences) et **remonte**. Toi tu tranches les
**🛑 hard-stop** ; Claude peut poser un **défaut réversible** quand c'est marqué.

← [[00 — Pilotage QUANTA]]

## Cadre (l'umbrella)
Trajectoire de [[DESIGN-CONSENSUS-DAG-BFT]] : **durcir le PoS linéaire → finality
gadget (Option 1) → DAG-BFT (Option 2)**, le **harnais multi-nœuds**
([[QUANTA_T0_DST_HARNESS]]) étant le prérequis (en cours). État des 4 méta-décisions §7 :
- ✅ **Périmètre** (2026-06-21) : **Option 1 — finality gadget d'abord** (garder la chaîne
  linéaire + leader PoS, **ajouter** un vote BFT 2-rounds qui finalise en ~secondes).
  DAG-BFT (Option 2) reste en Phase 2, derrière bump de protocole.
- ✅ **Prérequis** : harnais + chaos **d'abord** (= Phase 0 actuelle, T0.1 fait).
- ⬜ **Signatures** : BLS (non-PQ) pour agréger 2f+1 votes, ou rester 100 % hybride PQ ? *(OUVERTE)*
- ⬜ **Compat** : le gadget ajoute des messages de vote → fenêtre de migration / version protocole ? *(OUVERTE)*

## Les ADR
| ADR | Sujet | Classe | Statut |
|---|---|---|---|
| [[ADR-001 — Fork-choice]] | Sélection de branche (fenêtre non-finalisée → finalité) | défaut réversible | **résolu par le gadget** ; sous-choix intérim |
| [[ADR-002 — Validator set & comité BFT]] | Comité = stake on-chain par epoch | 🛑 | ✅ **ACCEPTÉE** — stake on-chain seul |
| [[ADR-003 — Slashing (accountable safety)]] | Équivocation prouvable + pénalité | 🛑 | OUVERTE (in-scope Phase 1) |
| [[ADR-004 — Aléa d'élection (beacon vs ECVRF+VDF)]] | Imprévisibilité + anti-grinding | 🛑 | OUVERTE (beacon OK pour P1 ?) |

## Convention
Une ADR = une décision. Cycle de vie : **OUVERTE** → (tu tranches) → **ACCEPTÉE**
(date + choix + conséquences gravées) ou **REJETÉE**. On ne réécrit pas une ADR
acceptée ; on en crée une nouvelle qui la **supersède** (lien `supersedes`).
