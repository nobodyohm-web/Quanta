# Backend Rust — Règles spécifiques V2

> Ce fichier ne se charge que quand Claude travaille dans src-tauri/

## Lock ordering (CRITIQUE)
```
1. crypto (lock) → release
2. reputation (read) → release
3. ledger (write) → release
4. gossip (read/write)
JAMAIS l'inverse — risque de deadlock
```

## Mining loop (mining_loop.rs)
```
toutes les 60s:
  watts = estimate_watts()
  total = node.total_network_watts() + watts
  (qta, kwh) = reputation.uptime_tick(pk, blocks_verified, emission_for_tick, peer_contribs)
  ledger.mine_tx(pk, qta, kwh)

toutes les 2 ticks (seal):
  block = ledger.seal_if_pending(pk, kwh)
  if block → broadcast NewBlock
```

## Émission (rareté — plafond dur + décroissance)
- `MAX_SUPPLY_MICRO = 100_000_000 * MICRO` (plafond DUR, vérifié au consensus)
- `EMISSION_DIVISOR = 50_000_000`
- `emission_for_tick(total_mined) = (MAX_SUPPLY_MICRO − total_mined) / EMISSION_DIVISOR`
  → ≈2 QUANTA/tick à la genèse (~120 QUANTA/h), décroît vers le plafond
- Solo : 100% du tick ; Multi : part proportionnelle à la contribution mesurée
  (Shapley) distribuée par `uptime_tick` / `shapley::distribute_emission`
- Zéro premine, zéro autorité d'émission ; le minage en direct (`mining_loop`)
  appelle `emission_for_tick(total_mined)`

## Gossip Protocol — Message Types (crypto-only)
```
Core sync:     Hello, RequestChain, ChainSegment, NewBlock
Transactions:  BroadcastTx
Liveness:      Ping, Pong
Security:      ReportPeer
Identity:      PublishUsername
```

## Dispatch pipeline (dispatcher.rs)
```
① Size guard (10 MB)
② JSON → GossipEnvelope
③ Ban check
④ Envelope-id canonique (H1) — id == BLAKE3(pré-image signée), sinon drop
⑤ Sonde dedup EN LECTURE (seen_messages LRU 100K) — n'insère rien
⑥ Timestamp (±90s)
⑦ Signature ML-DSA-65 (PQ-ENVELOPE-1)
⑧ Insertion dedup — APRÈS authentification (H1)
⑨ Rate limit adaptatif + nonce monotone par expéditeur
⑩ Handler dispatch
```
> **H1 (audit 2026-07-25)** : l'insertion dedup était en ④, la signature en ⑧.
> Un pair non authentifié pouvait donc empoisonner le LRU avec des identifiants
> choisis et censurer la synchronisation de chaîne gratuitement.

## State stores (WillowNode)
```
reputation, ledger, consensus, gossip,
energy_oracle, peer_country_reports, peer_info,
nonce_tracker, usernames
```
Tous wrappés dans `Arc<RwLock<T>>`. Stores persistés (30s) :
ledger, reputation, consensus, gossip, usernames.

## Tests critiques
```bash
# Compilation
cargo check --manifest-path src-tauri/Cargo.toml

# Tous les tests (196+)
cargo test --manifest-path src-tauri/Cargo.toml

# P2P uniquement
cargo test --manifest-path src-tauri/Cargo.toml -- p2p

# Sécurité
cargo test --manifest-path src-tauri/Cargo.toml -- security

# Avec logs
cargo test --manifest-path src-tauri/Cargo.toml -- --nocapture
```
