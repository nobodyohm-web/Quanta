# Prompt Claude Code — V2 Core Implementation

> Copie ce prompt dans Claude Code (Sonnet). Utilise les agents avec @.claude/agents/

---

```
Lis CLAUDE.md, .agent/memory.md, et .agent/design/tech_references.md.

# Mission : Implémenter les 4 mécanismes V2 du protocole SOVA

Le code est propre (purge terminée). Il reste 4 features V2 critiques à câbler.
Procède étape par étape. cargo check APRÈS CHAQUE ÉTAPE.

---

## 1. Nettoyer TxType (5 min)

Fichier : src-tauri/src/p2p/ledger.rs ligne ~28

Le enum TxType contient encore des variants sociaux morts.
Supprime : Like, Help, Create, View
Garde : Mining, Transfer, Stake, Unstake, Burn

Fais un grep dans tout src-tauri/src/ pour vérifier qu'aucun code ne référence Like, Help, Create, ou View avant de supprimer.

cargo check

---

## 2. Watts dans GossipMessage::Hello (10 min)

Fichier : src-tauri/src/p2p/gossip.rs ligne ~38

Ajoute 2 champs au variant Hello :
```rust
Hello {
    heads:   Vec<String>,
    node_id: String,
    version: u8,
    watts:   f64,      // watts CPU mesurés
    country: String,   // code pays ISO
},
```

Modifie build_hello() pour accepter watts: f64 et country: String.

Puis mets à jour TOUS les appels à build_hello() :
- Dans lib.rs (mining loop, cherche build_hello)
- Dans dispatcher.rs (si présent)

Utilise crate::p2p::energy::estimate_watts() et EnergyOracle::detect_country() pour les valeurs.

Quand on REÇOIT un Hello dans dispatcher.rs, ajoute le match sur les nouveaux champs :
```rust
GossipMessage::Hello { heads, node_id, watts, country, .. } => {
    // watts et country sont maintenant disponibles
}
```

cargo check

---

## 3. Tracking des watts des pairs (10 min)

Fichier : src-tauri/src/p2p/willow_node.rs

Ajoute un champ dans WillowNode :
```rust
/// V2: watts mesurés par chaque pair (peer_id → watts)
pub peer_watts: Arc<RwLock<HashMap<String, f64>>>,
```

Initialise dans new() : peer_watts: Arc::new(RwLock::new(HashMap::new()))

Dans dispatcher.rs, quand on reçoit un Hello, stocke les watts :
```rust
state.node.peer_watts.write().await.insert(node_id.clone(), watts);
```

cargo check

---

## 4. Mining proportionnel (15 min)

Fichier : src-tauri/src/p2p/reputation.rs

Modifie uptime_tick() pour accepter total_network_watts :
```rust
pub fn uptime_tick(&mut self, pk: &str, _total_mined: f64, total_network_watts: f64) -> (f64, f64) {
    let watts = estimate_watts();
    let kwh_per_min = watts / 1000.0 / 60.0;

    let user = self.get_or_create(pk);
    user.uptime_minutes += 1;
    user.energy_kwh += kwh_per_min;

    // V2 : émission proportionnelle
    let my_share = if total_network_watts <= 0.0 {
        EMISSION_PER_TICK  // solo
    } else {
        (watts / total_network_watts).min(1.0) * EMISSION_PER_TICK
    };

    let poc = SybilGuard::poc_score(user);
    let sova = my_share * SybilGuard::mining_multiplier(poc);

    user.atn_balance += sova;
    user.atn_earned += sova;
    user.energy_atn_mined += sova;

    let score = compute_trust_score(user);
    user.trust_score = score;
    user.status = TrustStatus::from_score(score);

    (sova, kwh_per_min)
}
```

Dans lib.rs, dans le mining loop, AVANT l'appel à uptime_tick :
```rust
let my_watts = crate::p2p::energy::estimate_watts();
let peer_watts = ms.node.peer_watts.read().await;
let total_network_watts = peer_watts.values().sum::<f64>() + my_watts;
drop(peer_watts);
```
Puis passe total_network_watts à uptime_tick.

Mets aussi à jour simulation.rs si uptime_tick y est appelé.

cargo check + cargo test

---

## 5. Burn-and-Mint sur transferts (15 min)

Fichier : src-tauri/src/p2p/ledger.rs

Ajoute la constante :
```rust
const BURN_RATE_TRANSFER: f64 = 0.01; // 1% brûlé par transfert
```

Crée une méthode transfer_with_burn :
```rust
pub fn transfer_with_burn(
    &mut self, from: &str, to: &str, amount: f64, crypto: &CryptoEngine
) -> Result<(Transaction, f64), String> {
    let burn_amount = amount * BURN_RATE_TRANSFER;
    let net_amount = amount - burn_amount;
    
    let tx = self.signed_transfer(from, to, net_amount, crypto)?;
    
    if burn_amount > 0.0 {
        let burn_tx = self.build_unsigned_tx(from, "BURN_ADDRESS", burn_amount, TxType::Burn);
        self.pending.push(burn_tx);
    }
    
    Ok((tx, burn_amount))
}
```

Dans lib.rs, commande ledger_transfer : utilise transfer_with_burn au lieu de signed_transfer.
Ajoute burn_amount dans la réponse JSON.

cargo check + cargo test

---

## 6. Tests unitaires (10 min)

Ajoute au moins 3 tests dans reputation.rs :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emission_solo_full() {
        let mut rep = ReputationEngine::new();
        let (sova, _kwh) = rep.uptime_tick("test_pk", 0.0, 0.0);
        // Solo : devrait recevoir ~EMISSION_PER_TICK (ajusté par PoC)
        assert!(sova > 0.0);
        assert!(sova <= EMISSION_PER_TICK * 1.1);
    }

    #[test]
    fn test_emission_proportional() {
        // 2 nœuds : 100W et 50W
        let rate_a = ReputationEngine::mining_rate_proportional(100.0, 150.0, 1.0);
        let rate_b = ReputationEngine::mining_rate_proportional(50.0, 150.0, 1.0);
        assert!((rate_a / rate_b - 2.0).abs() < 0.1);
    }

    #[test]
    fn test_trust_score_increases_with_uptime() {
        let mut rep = ReputationEngine::new();
        rep.uptime_tick("pk1", 0.0, 0.0);
        let score1 = rep.get_user("pk1").unwrap().trust_score;
        for _ in 0..60 { rep.uptime_tick("pk1", 0.0, 0.0); } // 1 heure
        let score2 = rep.get_user("pk1").unwrap().trust_score;
        assert!(score2 > score1);
    }
}
```

cargo test (tous les tests doivent passer)

---

## 7. Finalisation

```bash
cargo check     # 0 error
cargo test      # tous verts
cargo clippy -- -D warnings  # 0 warning
npm run build   # success
```

Commit :
```bash
git add -A
git commit -m "feat: V2 core — proportional mining, gossip watts, burn-and-mint, tests"
```

Mets à jour .agent/memory.md avec les changements.
```
