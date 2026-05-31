Effectue un diagnostic complet du réseau P2P Torus :

1. **État du code** :
   - `cargo check --manifest-path src-tauri/Cargo.toml` — compilation
   - `cargo test --manifest-path src-tauri/Cargo.toml` — tous les tests
   - `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` — linting

2. **Analyse du protocole** :
   - Vérifier que TOUS les GossipMessage variants ont un handler dans dispatcher.rs
   - Vérifier que chaque handler est idempotent (dedup check)
   - Vérifier que les signatures sont vérifiées AVANT le processing
   - Vérifier que les timestamps sont validés (±90s)
   - Vérifier que les nonces sont monotones

3. **Analyse de la synchronisation** :
   - Vérifier que Hello contient chain_height
   - Vérifier que RequestChain est paginé (max 50)
   - Vérifier que ChainSegment déclenche un re-request si encore behind
   - Vérifier que les blocs sont validés avant intégration

4. **Analyse de la robustesse** :
   - Vérifier que CancellationToken est utilisé dans tous les spawns
   - Vérifier que les locks ne deadlockent pas (ordering)
   - Vérifier que state_persistence.rs snapshot couvre tous les stores
   - Vérifier que cleanup_dead_peers fonctionne (TTL 5min)

5. **Rapport** : Générer un tableau des issues trouvées avec :
   - Fichier + ligne
   - Sévérité (CRITICAL / HIGH / MEDIUM / LOW)
   - Description du problème
   - Fix recommandé
