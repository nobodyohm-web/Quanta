Améliore le réseau P2P Torus en suivant ce workflow strict :

1. Lis CLAUDE.md (architecture V2 + protocole wire)
2. Lis `.claude/rules/p2p-protocol.md` et `.claude/rules/network-quality.md`
3. Analyse l'état actuel du réseau :
   - `src-tauri/src/p2p/gossip.rs` — messages + routeur
   - `src-tauri/src/p2p/dispatcher.rs` — pipeline sécurité
   - `src-tauri/src/p2p/willow_node.rs` — endpoint Iroh
   - `src-tauri/src/p2p/gossip_tasks.rs` — background tasks
   - `src-tauri/src/p2p/ledger.rs` — blockchain

4. Identifie le prochain objectif dans cette liste ordonnée :
   a. Reconnexion automatique avec backoff exponentiel
   b. Peer exchange (peers partagent leurs known_peers via Hello)
   c. Message prioritization (CRITICAL > HIGH > MEDIUM > LOW)
   d. Compression des ChainSegments (zstd)
   e. Heartbeat léger (remplacer Hello 60s par Ping 15s + Hello 120s)
   f. Métriques réseau temps réel (latence, bandwidth, peer health)
   g. Parallel chain sync (batch request de segments)
   h. Protocol versioning (reject incompatible peers)

5. Implémente le prochain objectif non complété :
   - Crée une branche `git checkout -b net/<feature-name>`
   - Code le changement
   - Écris des tests
   - `cargo check` + `cargo test` + `cargo clippy -D warnings`
   - Commit : `git commit -m "net: <description>"`

6. Met à jour CLAUDE.md avec les changements

Types de commit : net, fix, refactor, docs, test
