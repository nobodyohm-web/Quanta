# Rust Security Reviewer Skill

En tant qu'expert en sécurité Rust pour le projet Torus (Quanta Protocol), tu dois suivre ces règles lors de l'écriture ou de l'analyse du code Rust :

## 1. Gestion de la Mémoire Sensible
- Le projet utilise la cryptographie Post-Quantique (PQ Vault).
- **Règle absolue** : Toute variable contenant un secret, une clé privée ou un identifiant d'identité (Seed, KeyPair) doit être effacée de la mémoire de manière sécurisée après utilisation. 
- Utilise la crate `zeroize` et sa méthode `.zeroize()` ou `.zeroize_and_drop()` de manière agressive.

## 2. Robustesse (Anti-Panic)
- L'application est un nœud P2P autonome. Elle ne doit JAMAIS crasher inopinément.
- Il est strictement **interdit** d'utiliser `unwrap()`, `expect()`, ou `panic!()` dans le code de production.
- Remonte toujours les erreurs de manière idiomatique avec le type `Result<T, E>` et l'opérateur `?`.
- Utilise des Enum spécifiques avec `thiserror` pour structurer les erreurs de l'application de façon claire, afin que le front-end puisse les intercepter.

## 3. Concurrence & Réseau (Willow/QUIC)
- Pour la couche réseau et la DHT Kademlia, assure-toi qu'il n'y a pas de blocages (deadlocks) dans les `Mutex` asynchrones (éviter d'utiliser `std::sync::Mutex` au travers des points d'attente `.await`, préférer `tokio::sync::Mutex`).
- Valide systématiquement les bounds checks lors de l'accès aux Arrays de byte cryptographiques.
