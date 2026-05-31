Effectue un cycle complet de développement guidé par les tests (TDD) :

1. **Comprends** l'exigence demandée par l'utilisateur

2. **Écris le test d'abord** :
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;
       
       #[test]
       fn test_feature_name() {
           // Arrange
           let mut engine = MyEngine::new();
           
           // Act
           let result = engine.do_something();
           
           // Assert
           assert_eq!(result, expected_value);
       }
   }
   ```

3. **Vérifie que le test échoue** :
   ```bash
   cargo test --manifest-path src-tauri/Cargo.toml -- test_feature_name
   ```
   Si le test passe déjà → la feature existe déjà, pas besoin de coder.

4. **Implémente le minimum** pour faire passer le test

5. **Vérifie que le test passe** :
   ```bash
   cargo test --manifest-path src-tauri/Cargo.toml -- test_feature_name
   ```

6. **Refactor** si nécessaire (le test doit toujours passer)

7. **Vérifie la suite complète** :
   ```bash
   cargo test --manifest-path src-tauri/Cargo.toml
   cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
   ```

8. **Commit** : `git commit -m "test: description" && git commit -m "feat: description"`

Patterns de test QUANTA :
- **Ledger** : balance_of, transfer, burn, fork reorg
- **Gossip** : signature verify, dedup, nonce anti-replay
- **Consensus** : CRDT merge convergence
- **Network** : reconnection, chain sync, peer lifecycle
- **Security** : zeroize, replay attack, rate limit
