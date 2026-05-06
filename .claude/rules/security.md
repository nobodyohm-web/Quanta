---
description: Règles de sécurité cryptographique pour le protocole QUANTA
paths: ["src-tauri/src/security/**", "src-tauri/src/p2p/ledger.rs"]
---

# Règles Sécurité Crypto

1. **zeroize** : Toute variable `secret`, `sk_bytes`, `key`, `password` → `.zeroize()` après usage
2. **Pas de `unwrap()`** : `Result<T, E>` + `?` partout
3. **Erreurs opaques** : En cas d'échec de déchiffrement, retourner "Invalid" — jamais le type d'erreur réel
4. **Ed25519** : Chaque transaction doit être signée et vérifiée via `verify_tx()`
5. **Argon2id** : KDF avec 64 MiB mémoire, 3 itérations, 4 parallélisme
6. **AES-256-GCM** : Nonce unique par opération (12 bytes random)
7. **BLAKE3** : Pour tout hashing (content-addressing, DAG IDs, signatures)
8. **Pas de secrets en clair** : Jamais de clé privée dans les logs, les erreurs, ou les réponses JSON
