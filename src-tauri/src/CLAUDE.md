# Backend Rust — Règles spécifiques

> Ce fichier ne se charge que quand Claude travaille dans src-tauri/

## Lock ordering (CRITIQUE)
```
1. reputation (read) → release
2. ledger (write)
3. JAMAIS l'inverse
```

## Mining loop (lib.rs ~ligne 540)
```
toutes les 60s:
  watts = estimate_watts()
  peer_watts = node.peer_watts.read()
  total = peer_watts.sum() + watts
  (sova, kwh) = reputation.uptime_tick(pk, total_mined, total)
  ledger.mine_tx(pk, sova, kwh)
  dag.append(mine_node)
  gossip broadcast Hello(watts, country)
```

## Émission V2
- `NETWORK_EMISSION_PER_HOUR = 100.0`
- `EMISSION_PER_TICK = 100.0 / 60.0` (1.6667 SOVA/min)
- Solo : 100% de l'émission
- Multi : proportionnel aux watts via `mining_rate_proportional()`

## Gossip messages
Hello(heads, node_id, watts, country) → BroadcastTx → WantNodes/HaveNodes → Ping/Pong → ReportPeer
