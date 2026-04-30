# Phase 3 — Perfection (10/10 partout)
## Intégration + Polish — Brief Claude Code Opus

> **Objectif** : Toutes les briques existent. Il faut maintenant les BRANCHER entre elles
> et polir les derniers détails CSS pour atteindre 10/10 partout.
> **Statut actuel** : les modules Phase 2 compilent et sont testés, mais certains ne sont
> pas encore intégrés dans le flux réel de l'application.

---

## DIAGNOSTIC PRÉCIS — Ce qui manque

### A. Backend Rust — 3 intégrations manquantes

#### A1. Mining loop dans `lib.rs` (CRITIQUE)
**Problème** : `lib.rs:608` utilise encore la constante `KWH_PER_TICK: f64 = 15.0 / 1000.0 / 60.0`.
Mais `reputation.rs:137` appelle déjà `estimate_watts()`.
**→ Le kWh passé à `mine_tx()` en ligne 627 est FAUX — c'est la vieille constante.**

**Fix** : Remplacer dans `lib.rs` le mining loop :
```rust
// AVANT (ligne 608)
const KWH_PER_TICK: f64 = 15.0 / 1000.0 / 60.0;

// APRÈS — lire la vraie valeur après le tick
// (uptime_tick calcule déjà les watts réels via estimate_watts)
```
Et en ligne 627, remplacer `KWH_PER_TICK` par la valeur calculée dans `uptime_tick`.
Solution : ajouter un retour de `kwh_per_min` depuis `uptime_tick` (retourner un tuple).

#### A2. Consensus CRDT + DAG non branchés (IMPORTANT)
`consensus.rs`, `merkle_dag.rs`, `gossip.rs` existent mais ne sont appelés nulle part.
**Ce n'est PAS bloquant** — le système marche en solo. Mais pour 10/10, il faut AU MINIMUM :

1. Ajouter `consensus: RwLock<consensus::ConsensusEngine>` dans `SovaNode` (`lib.rs`)
2. Ajouter `dag: RwLock<merkle_dag::MerkleDAG>` dans `SovaNode`
3. Ajouter `gossip: RwLock<gossip::GossipRouter>` dans `SovaNode`
4. Dans le mining loop, après chaque `mine_tx`, insérer le bloc dans le DAG
5. Exposer une commande `get_consensus_stats` pour le frontend

#### A3. Commande `get_energy_stats` — vérifier format retour
Le Wallet appelle `invoke<EnergyStats>("get_energy_stats")` avec :
```ts
interface EnergyStats {
  kwh_consumed: number;
  atn_mined: number;
  uptime_minutes: number;
  atn_floor_eur: number;
}
```
**Vérifier** que la commande Rust retourne exactement ces 4 champs avec ces noms.

---

### B. Frontend CSS — 50 refs `--color-*` dans Editor.svelte

`Editor.svelte` a encore **50 occurrences** de `--color-*` (ancien système).
`Wallet.svelte` a **0** (OK).
`TemplatePicker.svelte` a **0** (OK).

**Fix** : Migrer les 50 refs de `Editor.svelte` :

| Ancien | Nouveau |
|--------|---------|
| `--color-bg-0` | `--sova-bg-0` |
| `--color-bg-1` | `--sova-bg-1` |
| `--color-bg-2` | `--sova-bg-2` |
| `--color-text-0` | `--sova-text-0` |
| `--color-text-1` | `--sova-text-1` |
| `--color-text-2` | `--sova-text-2` |
| `--color-text-3` | `--sova-text-2` (pas de text-3 dans SOVA) |
| `--color-border` | `--sova-border` |
| `--color-border-hover` | `--sova-border-h` |
| `--color-accent` | `--sova-accent` |
| `--color-accent-dim` | `--sova-accent-dim` |
| `--color-green` | `--sova-accent` |
| `--color-red` | `--sova-negative` |
| `--font-mono` | `--sova-font-mono` |
| `--radius-lg` | `--radius` |
| `#e6edf3` | `var(--sova-text-0)` |
| `#484f58` | `var(--sova-text-2)` |

**Également** : vérifier que `app.css` définit les aliases `--color-*` qui pointent vers `--sova-*`
pour les composants globaux (`.btn`, `.input`, `.page`, etc.).

---

### C. Cleanup final

| Fichier | Action |
|---------|--------|
| `Editor.svelte` L669 | `transition: transform 0.1s, box-shadow 0.1s` → supprimer `box-shadow` |
| `templates.ts` | Vérifier qu'aucune ref "TITAN" ne subsiste |
| `NavBar.svelte` | Vérifier qu'il utilise `--sova-*` tokens |
| `Feed.svelte` | Vérifier qu'il utilise `--sova-*` tokens |
| `Browser.svelte` | Vérifier qu'il utilise `--sova-*` tokens |
| `UserProfile.svelte` | Vérifier qu'il utilise `--sova-*` tokens |

---

## ORDRE D'EXÉCUTION

```
1. lib.rs         — Fix mining loop kWh (A1)
2. lib.rs         — Ajouter consensus/dag/gossip dans SovaNode (A2)
3. lib.rs         — Vérifier get_energy_stats format (A3)
4. Editor.svelte  — Migrer 50x --color-* → --sova-* (B)
5. Tous .svelte   — Audit --color-* restants → --sova-* (C)
6. cargo test     — 16/16 passent
7. npm run ai:check — 0/0
```

---

## TESTS DE VALIDATION

### Backend
1. ✅ `cargo test` → 16 passent, 0 échoué
2. ✅ Mining utilise les watts CPU réels (pas la constante 15W)
3. ✅ `get_energy_stats` retourne `kwh_consumed`, `atn_mined`, `uptime_minutes`, `atn_floor_eur`
4. ✅ Le DAG et les CRDTs sont initialisés dans SovaNode
5. ✅ Pas de `dead_code` warning pour consensus/dag/gossip (branchés)

### Frontend
1. ✅ `grep "color-" src/lib/Editor.svelte` → 0 résultat
2. ✅ `grep "color-" src/lib/Wallet.svelte` → 0 résultat
3. ✅ `grep "color-" src/lib/TemplatePicker.svelte` → 0 résultat
4. ✅ `grep "#e6edf3\|#484f58\|#0d1117" src/lib/Editor.svelte` → 0 résultat
5. ✅ `npm run ai:check` → 0 errors, 0 warnings
