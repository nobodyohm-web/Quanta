Simule un scénario réseau P2P et vérifie la convergence :

1. Lis `src-tauri/src/p2p/simulation.rs` pour comprendre le framework de test
2. Lis `src-tauri/src/p2p/integration_tests.rs` pour les tests existants

3. Écris un test d'intégration pour le scénario demandé (ou le prochain dans la liste) :

   **Scénarios standard** :
   a. **Two-node sync** : Node A mine 10 blocs, Node B se connecte → B doit converger
   b. **Late joiner** : Network de 3 nodes mine 100 blocs, Node D rejoint → D rattrape tout
   c. **Fork resolution** : Node A et B minent simultanément → fork résolu déterministiquement
   d. **Peer churn** : Node se déconnecte 30s puis revient → resync complet
   e. **Partition healing** : 2 groupes isolés mergent → chain converge
   f. **Tx propagation** : Node A envoie une tx → arrive à Node C en passant par B
   g. **Block validation** : Node malveillant envoie un bloc invalide → rejeté par tous
   h. **Rate limit** : Node flood → rate limited, pas de crash
   i. **Nonce replay** : Rejouer un message → rejeté
   j. **State recovery** : Kill + restart → state restauré depuis SQLite

4. Le test DOIT :
   - Utiliser des instances réelles de Ledger, GossipRouter, ConsensusEngine
   - Simuler des échanges gossip via les handlers de dispatcher.rs
   - Vérifier la convergence (chaînes identiques, balances identiques)
   - Passer en <5s

5. `cargo test --manifest-path src-tauri/Cargo.toml -- <test_name> --nocapture`

6. Si le test échoue, c'est un BUG RÉEL à corriger, pas le test.
