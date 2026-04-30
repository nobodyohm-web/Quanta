# Prompt Opus 4.7 — Optimisé Token-Efficient

> Les modules shapley.rs et marketplace.rs sont PRÉ-ÉCRITS et compilent.
> Opus doit UNIQUEMENT : review → corriger → intégrer → tester.
> Utilise "think hard" pour l'analyse mathématique.

---

```
/clear

Lis CLAUDE.md, .agent/memory.md, .agent/design/tech_references.md.

# Mission : Review + Intégration Shapley & Marketplace

2 modules pré-écrits compilent (cargo check OK, cargo test OK) :
- src-tauri/src/p2p/shapley.rs → 190 lignes, 6 tests
- src-tauri/src/p2p/marketplace.rs → 280 lignes, 3 tests

Think hard about the mathematical correctness of these modules.

## PHASE 1 — Review (Plan Mode recommandé)

### shapley.rs
Vérifie que :
1. Shapley approximation linéaire est correcte (poids 30/35/20/15)
2. Normalisation sum=1.0 dans TOUS les cas (0 nœuds, 1 nœud, N nœuds)
3. Fallback quand aucune tâche (uniform distribution)
4. distribute_emission() préserve l'invariant total=EMISSION_PER_TICK
5. Les 6 tests couvrent les edge cases

### marketplace.rs
Vérifie que :
1. Burn 2% appliqué au submit (pas au pay) — correct ?
2. Worker non-assigné rejeté dans submit_result()
3. Lifecycle : Pending→Claimed→Submitted→Completed est complet
4. Manque-t-il un refund si tâche expire ?
5. Faut-il ajouter un timeout sur Claimed→expire si le worker ne livre pas ?

Patch chirurgical seulement si nécessaire. Ne réécris PAS les modules.

## PHASE 2 — Intégration (3 points de câblage)

### 2a. WillowNode : ajouter Marketplace
Ajoute `pub marketplace: Arc<RwLock<Marketplace>>` dans WillowNode (willow_node.rs).
Initialise dans new().

### 2b. reputation.rs : câbler Shapley dans uptime_tick()
Actuellement (ligne ~98) : `let my_share = EMISSION_PER_TICK;` (solo)
Remplace par :
```rust
// V2: Shapley quand il y a des peers
let my_share = if total_network_watts <= 0.0 {
    EMISSION_PER_TICK
} else {
    let my_contrib = shapley::NodeContribution {
        node_id: pk.to_string(),
        watts,
        tasks_completed: 0, // Phase 3: from marketplace
        blocks_verified: 0, // TODO: from DAG
        uptime_minutes: user.uptime_minutes,
    };
    let mut contribs = HashMap::new();
    contribs.insert(pk.to_string(), my_contrib);
    // TODO Phase 3: ajouter les contributions des peers depuis gossip
    let shares = shapley::compute_all_shares(&contribs);
    shares.get(pk).copied().unwrap_or(1.0) * EMISSION_PER_TICK
};
```

### 2c. lib.rs : 3 commandes Tauri marketplace
```rust
#[tauri::command]
async fn submit_compute_task(
    state: tauri::State<'_, Arc<AppState>>,
    task_type: String,
    reward: f64,
    deadline: String,
) -> Result<serde_json::Value, String>

#[tauri::command]
async fn get_pending_tasks(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String>

#[tauri::command]
async fn get_marketplace_stats(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String>
```

Enregistre dans invoke_handler().

## PHASE 3 — Vérification

```bash
cargo check
cargo test
cargo clippy -- -D warnings
npm run build
```

Commit : `feat: integrate shapley distribution + compute marketplace`

Mets à jour .agent/memory.md.
```
