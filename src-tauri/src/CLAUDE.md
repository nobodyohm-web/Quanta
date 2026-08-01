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
  reputation.uptime_tick(...)      ← compteurs d'AFFICHAGE seulement (MINT-EXACT-1)
                                      le tick ne crée PLUS de monnaie

toutes les 2 ticks (si proposeur élu OU slot ouvert):
  ledger.mint_block_reward(pk)     ← coinbase CANONIQUE = emission_for_block(chaîne)
  block = ledger.seal_block(pk, 0.0)  ← le proposeur produit toujours son bloc
  broadcast NewBlock
```
> **MINT-EXACT-1.** Le tick appelait `mine_tx` avec un montant calculé *localement*
> (part Shapley sur des watts **auto-déclarés** par les pairs). Trois conséquences,
> toutes fermées : (1) `mine_tx` scellait tout seul à 10 tx en attente, **sans
> diffuser** → chaîne privée + solde fantôme ; (2) la tx `Mining` diffusée était
> rejetée par chaque pair (MINT-GUARD-1) → trafic mort ; (3) les parts Shapley
> sommant à 1, l'émission réalisée s'effondrait en `1/N`. `mine_tx` est désormais
> `#[cfg(test)]` — en release **rien** ne peut poser de `Mining` au mempool.

## Émission (rareté — plafond dur + décroissance)
- `MAX_SUPPLY_MICRO = 100_000_000 * MICRO` (plafond DUR, vérifié au consensus)
- `EMISSION_DIVISOR = 50_000_000`
- `emission_for_tick(total_mined) = (MAX_SUPPLY_MICRO − total_mined) / EMISSION_DIVISOR`
  → ≈2 QUANTA/tick à la genèse (~120 QUANTA/h), décroît vers le plafond
- **MINT-EXACT-1** : `emission_for_block(total_mined) = emission_for_tick × TICKS_PER_BLOCK`
  → la récompense d'un bloc est une **fonction pure de la chaîne**, identique pour
  tous, indépendante du nombre de nœuds et de l'énergie déclarée. Le récepteur la
  **recalcule** (`block_minted ≤ emission_for_block(prior)`) au lieu de la borner
  mollement — l'ancienne marge `64 ticks` valait `32 × N` fois le montant honnête.
  Shapley/énergie restent des signaux d'**affichage**, hors du chemin monétaire.
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
