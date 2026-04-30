Effectue un audit complet de sécurité du code modifié :

1. Scanne tous les fichiers `.rs` modifiés récemment (git diff)
2. Vérifie chaque point :
   - [ ] Aucun `unwrap()` en production
   - [ ] Tous les secrets sont `zeroize()` après usage
   - [ ] Erreurs opaques pour le déchiffrement
   - [ ] Signatures Ed25519 vérifiées sur chaque transaction
   - [ ] Pas de `std::sync::Mutex` à travers un `.await`
   - [ ] Lock ordering respecté (engines → DB)
   - [ ] Pas de clé privée dans les logs/erreurs
   - [ ] Nonces uniques pour AES-256-GCM
3. Exécute `cargo clippy -D warnings`
4. Rapporte les vulnérabilités trouvées avec fichier:ligne
