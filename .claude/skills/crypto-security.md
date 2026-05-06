---
description: Working on cryptographic operations, identity management, key generation, signing, encryption, or vault operations
globs: ["src-tauri/src/security/**/*.rs"]
---

# Skill: Cryptographic Security

## Identity Stack
```
CryptoEngine (mod.rs)
  └── Ed25519 keypair (ed25519-dalek)
  └── sign(data) → Vec<u8>
  └── verify(pk, data, sig) → bool
  └── get_identity() → QuantaIdentity

PQVault (pq_vault.rs)
  └── create_identity(engine, name, password) → (id, pk, enc_sk, nonce)
  └── unlock_identity(engine, pk, enc_sk, nonce, password) → QuantaIdentity
  └── AES-256-GCM encryption of secret key
  └── Argon2id KDF: m=65536, t=3, p=4

HybridIdentity (hybrid_crypto.rs)
  └── sign_hybrid(data) → (classical, quantum, pq_pk)
  └── verify_hybrid(pk, pq_pk, data, hybrid_sig) → bool
  └── ML-DSA-65 ready (stub for now)
```

## CRITICAL Rules
1. **zeroize**: Every `Vec<u8>` containing secret material → `.zeroize()` after use
2. **No unwrap**: `Result<T, E>` + `?` everywhere
3. **Opaque errors**: Never reveal whether decryption failed due to wrong key vs corrupt data
4. **No secrets in logs**: Never log private keys, passwords, or derived keys
5. **Unique nonces**: AES-256-GCM requires 12 random bytes per encryption
6. **BLAKE3 for hashing**: Content addressing, DAG IDs, block hashes

## Signing Patterns
```rust
// Transaction signing (hybrid)
let (classical, quantum, pq_pk) = crypto.sign_hybrid(payload.as_bytes())?;

// Gossip envelope signing (canonical)
let signable = GossipRouter::signable_envelope_bytes(&pk, nonce, &timestamp, &msg);
let sig = crypto.sign(&signable)?;

// Verification
CryptoEngine::verify(&pk_bytes, &data, &sig_bytes)?; // returns bool
```

## Key Storage (SQLite)
```sql
-- Public key stored as hex, secret key encrypted with AES-256-GCM
-- Nonce = 12 bytes hex, used once for the encryption
keypairs(public_key TEXT, encrypted_secret_key TEXT, nonce TEXT, display_name TEXT)
```
