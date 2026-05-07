Tu es en mode DEEP ENGINEERING. Ton objectif : rendre le réseau P2P Torus PARFAIT.

Lis CLAUDE.md, toutes les rules dans .claude/rules/, et tous les skills dans .claude/skills/ AVANT de commencer.

## Mission

Tu vas parcourir TOUTE la liste d'améliorations ci-dessous, une par une, dans l'ordre. Pour chaque étape :
1. Analyse le code existant en profondeur
2. Identifie TOUS les problèmes, edge cases, et faiblesses
3. Implémente la solution la plus robuste possible
4. Écris des tests unitaires pour chaque changement
5. `cargo check` + `cargo test` + `cargo clippy -- -D warnings`
6. Si un test échoue → c'est un BUG RÉEL, corrige-le
7. Commit avec un message descriptif
8. Passe à l'étape suivante SANS T'ARRÊTER

## Liste des améliorations (exécute TOUT)

### Phase 1 — Robustesse réseau
- [ ] **Message prioritization** : Les messages CRITICAL (Hello, NewBlock, ChainSegment, RequestChain) doivent être traités avant les MEDIUM/LOW. Implémenter une priority queue dans le dispatch.
- [ ] **Heartbeat optimisé** : Remplacer le Hello lourd (60s) par un Ping léger (15s) pour le liveness + Hello complet (120s) pour le sync. Réduire la bande passante de 75%.
- [ ] **Protocol versioning** : Ajouter un `protocol_version: u8` dans Hello. Si un peer a une version incompatible, log un warning et skip les messages inconnus gracieusement.

### Phase 2 — Performance sync
- [ ] **Parallel chain sync** : Au lieu de demander 50 blocs à la fois, demander des segments en parallèle à PLUSIEURS peers si disponibles. Merger les résultats.
- [ ] **Incremental DAG sync** : Optimiser WantNodes/HaveNodes pour envoyer uniquement le delta depuis le dernier sync réussi, pas tout re-scanner.
- [ ] **Compression ChainSegment** : Les ChainSegments de 50 blocs sont verbeux en JSON. Implémenter une compression optionnelle (bincode ou MessagePack) avec fallback JSON.

### Phase 3 — Monitoring & Observabilité
- [ ] **Métriques réseau temps réel** : Ajouter latency_ms, last_ping_rtt, bandwidth_in/out par peer dans PeerInfo. Exposer via get_node_status au frontend.
- [ ] **Connection quality score** : Calculer un score de qualité par peer (0-100) basé sur : latency, message loss rate, uptime ratio. Utiliser pour le peer selection.
- [ ] **Network topology map** : Exposer une commande Tauri `get_network_topology` qui retourne la liste des peers avec leurs propres peers connus (2 niveaux de profondeur).

### Phase 4 — Sécurité avancée
- [ ] **Eclipse attack protection** : Si tous nos peers ont le même prefixe de public key (>80% similarity), c'est suspect. Logger un warning.
- [ ] **Adaptive rate limiting** : Au lieu d'un rate limit fixe (30/min), adapter dynamiquement basé sur le nombre de peers connectés et le traffic normal observé.
- [ ] **Transaction mempool** : Implémenter un vrai mempool avec ordering par nonce, expiration TTL (10 min), et size limit (1000 txs). Remplacer le Vec<Transaction> pending actuel.

### Phase 5 — UX réseau
- [ ] **Peer nicknames** : Permettre aux peers d'avoir un display_name optionnel dans Hello (signé). Afficher dans le frontend au lieu du hash de public key.
- [ ] **Sync progress** : Quand un chain sync est en cours, émettre des events Tauri pour afficher une barre de progression dans le frontend (X/Y blocs synchronisés).

## Règles d'exécution

- NE T'ARRÊTE PAS entre les étapes. Enchaîne tout.
- Si une étape est trop complexe, implémente la version minimale viable puis passe à la suite.
- Pousse ton raisonnement au MAXIMUM : pense aux edge cases, aux race conditions, aux deadlocks potentiels.
- Chaque fichier modifié doit compiler. Chaque test doit passer.
- Utilise les patterns existants du codebase (Arc<RwLock<T>>, CancellationToken, signable_envelope_bytes).
- Backward compatibility OBLIGATOIRE : `#[serde(default)]` sur tous les nouveaux champs.
- Commit après chaque phase complétée (pas après chaque sous-étape).
- À la fin, mets à jour CLAUDE.md avec les nouvelles features.
