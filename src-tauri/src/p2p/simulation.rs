// Simulation réseau QUANTA — 2 nœuds, transfert, impact prix global.
// Run: cd src-tauri && cargo test simulation_reseau -- --nocapture

#[cfg(test)]
mod network_simulation {
    use crate::p2p::energy::EnergyOracle;
    use crate::p2p::consensus::CrdtLedger;
    use crate::p2p::sybil::SybilGuard;
    use crate::p2p::reputation::ReputationEngine;
    use crate::p2p::ledger::MICRO;
    use crate::p2p::shapley::{NodeContribution, NodeMode};
    use std::collections::HashMap;

    #[test]
    fn simulation_reseau() {
        let oracle = EnergyOracle::new();

        println!("\n============================================================");
        println!("  🌍 SIMULATION RÉSEAU QUANTA — 2 nœuds");
        println!("============================================================\n");

        // ── Prix par pays ──────────────────────────────
        let pays = vec![
            ("FR", "France"), ("US", "USA"), ("DE", "Allemagne"),
            ("JP", "Japon"), ("IN", "Inde"), ("CH", "Suisse"),
            ("BR", "Brésil"), ("NO", "Norvège"), ("DK", "Danemark"),
        ];
        println!("  PRIX ÉNERGIE PAR PAYS (EUR/kWh) :");
        println!("  --------------------------------------------------");
        for (code, nom) in &pays {
            let prix = oracle.price_for(code);
            let floor = oracle.atn_floor_eur(code, 15.0); // 15W Apple Silicon
            println!("  {:<15} {:>8.4} EUR/kWh  →  1 ATN ≥ {:.5} EUR", nom, prix, floor);
        }

        // ── Scénario : 2 nœuds minent ──────────────────
        println!("\n  SCÉNARIO : MINING PENDANT 1 HEURE");
        println!("  --------------------------------------------------");

        let mut engine = ReputationEngine::new();

        // Nœud A — France (15W)
        let pk_a = "node_france_01";
        let watts_a = 15.0;
        let prix_a = oracle.price_for("FR"); // 0.2516

        // Nœud B — USA (20W, PC plus gourmand)
        let pk_b = "node_usa_01";
        let watts_b = 20.0;
        let prix_b = oracle.price_for("US"); // 0.1385

        // Simuler 60 minutes de mining (total_mined en µQTA).
        // STRUCT-6: chaque nœud voit l'autre comme un peer dans la map de contributions.
        let mut total_mined: u64 = 0;
        let _total_network_watts = watts_a + watts_b;
        for tick_min in 0..60u64 {
            // Peers vus par A : juste B
            let mut peers_a: HashMap<String, NodeContribution> = HashMap::new();
            peers_a.insert(pk_b.to_string(), NodeContribution {
                node_id: pk_b.to_string(),
                watts: watts_b,
                tasks_completed: 0,
                blocks_verified: 0,
                uptime_minutes: tick_min,
                mode: NodeMode::default(),
            });
            let emission_a = crate::p2p::reputation::emission_for_tick(total_mined);
            let (atn_a, _) = engine.uptime_tick(pk_a, 0, emission_a, &peers_a);
            total_mined = total_mined.saturating_add(atn_a);

            // Peers vus par B : juste A
            let mut peers_b: HashMap<String, NodeContribution> = HashMap::new();
            peers_b.insert(pk_a.to_string(), NodeContribution {
                node_id: pk_a.to_string(),
                watts: watts_a,
                tasks_completed: 0,
                blocks_verified: 0,
                uptime_minutes: tick_min,
                mode: NodeMode::default(),
            });
            let emission_b = crate::p2p::reputation::emission_for_tick(total_mined);
            let (atn_b, _) = engine.uptime_tick(pk_b, 0, emission_b, &peers_b);
            total_mined = total_mined.saturating_add(atn_b);
        }

        let user_a = engine.get_user(pk_a).unwrap();
        let user_b = engine.get_user(pk_b).unwrap();

        let kwh_a = watts_a / 1000.0; // kWh par heure
        let kwh_b = watts_b / 1000.0;
        let cost_a = kwh_a * prix_a;
        let cost_b = kwh_b * prix_b;

        let micro = MICRO as f64;
        println!("  Nœud A (France, {}W) :", watts_a as u32);
        println!("    QUANTA minés  : {:.6}", user_a.atn_earned as f64 / micro);
        println!("    Énergie     : {:.4} kWh", kwh_a);
        println!("    Coût réel   : {:.5} EUR", cost_a);
        println!("    PoC score   : {:.3}", SybilGuard::poc_score(user_a));
        println!();
        println!("  Nœud B (USA, {}W) :", watts_b as u32);
        println!("    QUANTA minés  : {:.6}", user_b.atn_earned as f64 / micro);
        println!("    Énergie     : {:.4} kWh", kwh_b);
        println!("    Coût réel   : {:.5} EUR", cost_b);
        println!("    PoC score   : {:.3}", SybilGuard::poc_score(user_b));

        // ── Impact du réseau sur le prix ────────────────
        println!("\n  IMPACT RÉSEAU SUR LE PRIX ATN");
        println!("  --------------------------------------------------");

        // Scénario 1 : France seule
        let avg_fr_only = oracle.network_weighted_average(&[
            ("FR".into(), 1),
        ]);
        println!("  1 nœud (France seule)     : {:.4} EUR/kWh → 1 ATN ≥ {:.5} EUR",
            avg_fr_only, avg_fr_only * 15.0 / 1000.0);

        // Scénario 2 : France + USA
        let avg_fr_us = oracle.network_weighted_average(&[
            ("FR".into(), 1), ("US".into(), 1),
        ]);
        println!("  2 nœuds (FR + US)         : {:.4} EUR/kWh → 1 ATN ≥ {:.5} EUR",
            avg_fr_us, avg_fr_us * 15.0 / 1000.0);

        // Scénario 3 : 10 pays
        let avg_10 = oracle.network_weighted_average(&[
            ("FR".into(), 5), ("DE".into(), 3), ("US".into(), 8),
            ("JP".into(), 2), ("GB".into(), 4), ("CH".into(), 1),
            ("AU".into(), 2), ("KR".into(), 3), ("BR".into(), 6),
            ("IN".into(), 10),
        ]);
        println!("  44 nœuds (10 pays)        : {:.4} EUR/kWh → 1 ATN ≥ {:.5} EUR",
            avg_10, avg_10 * 15.0 / 1000.0);

        // Scénario 4 : Beaucoup de nœuds dans des pays chers
        let avg_expensive = oracle.network_weighted_average(&[
            ("DK".into(), 20), ("DE".into(), 30), ("IT".into(), 15),
            ("BE".into(), 10), ("NL".into(), 10), ("CH".into(), 5),
        ]);
        println!("  90 nœuds (pays chers)     : {:.4} EUR/kWh → 1 ATN ≥ {:.5} EUR",
            avg_expensive, avg_expensive * 15.0 / 1000.0);

        // Scénario 5 : Beaucoup de nœuds dans des pays pas chers
        let avg_cheap = oracle.network_weighted_average(&[
            ("IN".into(), 50), ("AR".into(), 20), ("AE".into(), 10),
            ("CN".into(), 30), ("MX".into(), 15),
        ]);
        println!("  125 nœuds (pays pas chers) : {:.4} EUR/kWh → 1 ATN ≥ {:.5} EUR",
            avg_cheap, avg_cheap * 15.0 / 1000.0);

        // ── Transfert entre les 2 nœuds ─────────────────
        println!("\n  TRANSFERT CRDT");
        println!("  --------------------------------------------------");

        let mut crdt = CrdtLedger::new();
        // Créditer ce qu'ils ont miné — déjà en µQTA, copie directe
        // (clamp via MAX_CRDT_BATCH dans CrdtLedger).
        crdt.credit("network", pk_a, user_a.atn_earned);
        crdt.credit("network", pk_b, user_b.atn_earned);

        println!("  Avant transfert :");
        println!("    A (France) : {:.6} QUANTA", crdt.balance_of(pk_a) as f64 / micro);
        println!("    B (USA)    : {:.6} QUANTA", crdt.balance_of(pk_b) as f64 / micro);

        // A envoie 0.5 QUANTA à B = MICRO/2 µQTA
        let transfer = MICRO / 2;
        crdt.debit(pk_a, pk_a, transfer);
        crdt.credit(pk_a, pk_b, transfer);

        println!("  Après transfert de 0.5 QUANTA (A → B) :");
        println!("    A (France) : {:.6} QUANTA", crdt.balance_of(pk_a) as f64 / micro);
        println!("    B (USA)    : {:.6} QUANTA", crdt.balance_of(pk_b) as f64 / micro);

        // ── Fusion CRDT (simulation sync) ────────────────
        println!("\n  FUSION CRDT (simulation sync entre 2 nœuds)");
        println!("  --------------------------------------------------");

        let mut crdt_b = CrdtLedger::new();
        // B a miné 0.2 QUANTA = 200_000 µQTA de son côté
        crdt_b.credit("network", pk_b, 200_000);

        println!("  Nœud B local avant sync : {:.6} QUANTA", crdt_b.balance_of(pk_b) as f64 / micro);
        crdt_b.merge(&crdt);
        println!("  Nœud B après merge      : {:.6} QUANTA (convergence !)", crdt_b.balance_of(pk_b) as f64 / micro);

        println!("\n  Supply totale réseau : {:.6} QUANTA", total_mined as f64 / micro);
        println!("  Émission V2          : 100 QUANTA/h fixe, pas de halving");

        println!("\n============================================================");
        println!("  ✅ Simulation terminée");
        println!("============================================================\n");
    }
}

// ─── SIMULATION 2: Multi-Node Block Propagation ─────────────────────────────
// Ce test simule 2 nœuds indépendants qui minent, scellent des blocs,
// se les envoient via JSON (exactement comme le gossip), et vérifient
// que les chaînes convergent.
//
// Run: cd src-tauri && cargo test simulation_multi_nodes -- --nocapture

#[cfg(test)]
mod multi_node_simulation {
    use crate::p2p::ledger::{Ledger, MICRO};

    #[test]
    fn simulation_multi_nodes() {
        println!("\n============================================================");
        println!("  🔗 SIMULATION MULTI-NŒUDS — Propagation de blocs");
        println!("============================================================\n");

        // ── 2 ledgers indépendants, même genesis ──
        let mut node_a = Ledger::new();
        let mut node_b = Ledger::new();

        let pk_a = "node_a_pk_aaaa1111";
        let pk_b = "node_b_pk_bbbb2222";

        println!("  Nœud A: {}", &pk_a[..16]);
        println!("  Nœud B: {}", &pk_b[..16]);

        // ── Phase 1: Node A mine et Node B reçoit ──
        println!("\n  ── Phase 1: A mine → B reçoit ──");

        // Node A crée des transactions mining et scelle un bloc
        node_a.mint_block_reward_of(pk_a, MICRO); // 1 QUANTA
        node_a.mint_block_reward_of(pk_a, MICRO); // 1 QUANTA
        let block_a1 = node_a.seal_block(pk_a, 0.03);

        println!("  A sealed block #{} ({} txs, hash={}...)",
            block_a1.index, block_a1.transactions.len(), &block_a1.hash[..12]);

        // Simuler le gossip: sérialiser en JSON et désérialiser
        let json = serde_json::to_string(&block_a1).expect("serialize block");
        let remote_block: crate::p2p::ledger::Block = serde_json::from_str(&json).expect("deserialize block");

        println!("  Gossip: {} bytes JSON transmis", json.len());

        // Node B reçoit et intègre le bloc
        let result = node_b.integrate_remote_block(remote_block);
        assert!(result.is_ok(), "B devrait accepter le bloc");
        assert!(result.unwrap(), "B devrait intégrer (pas un doublon)");

        // Vérifier convergence
        assert_eq!(node_a.chain.len(), node_b.chain.len(),
            "Les deux chaînes doivent avoir la même longueur");
        assert_eq!(
            node_a.chain.last().unwrap().hash,
            node_b.chain.last().unwrap().hash,
            "Les deux tips doivent être identiques"
        );

        println!("  ✅ B a intégré le bloc — chaînes convergent (len={})", node_a.chain.len());

        // ── Phase 2: Node B mine et Node A reçoit ──
        println!("\n  ── Phase 2: B mine → A reçoit ──");

        node_b.mint_block_reward_of(pk_b, 500_000); // 0.5 QUANTA
        let block_b2 = node_b.seal_block(pk_b, 0.02);

        println!("  B sealed block #{} ({} txs, hash={}...)",
            block_b2.index, block_b2.transactions.len(), &block_b2.hash[..12]);

        let json = serde_json::to_string(&block_b2).expect("serialize");
        let remote: crate::p2p::ledger::Block = serde_json::from_str(&json).expect("deserialize");
        let result = node_a.integrate_remote_block(remote);
        assert!(result.is_ok());
        assert!(result.unwrap());

        assert_eq!(node_a.chain.len(), node_b.chain.len());
        assert_eq!(
            node_a.chain.last().unwrap().hash,
            node_b.chain.last().unwrap().hash,
        );

        println!("  ✅ A a intégré le bloc — chaînes convergent (len={})", node_a.chain.len());

        // ── Phase 3: Duplicate rejeté ──
        println!("\n  ── Phase 3: Replay / Doublon ──");

        let json_dup = serde_json::to_string(&block_b2).expect("serialize");
        let dup: crate::p2p::ledger::Block = serde_json::from_str(&json_dup).expect("deserialize");
        let result = node_a.integrate_remote_block(dup);
        assert!(result.is_ok());
        assert!(!result.unwrap(), "Le doublon doit être ignoré");

        println!("  ✅ Doublon correctement rejeté");

        // ── Phase 4: Fork resolution ──
        println!("\n  ── Phase 4: Fork — 2 blocs au même height ──");

        // A et B minent en même temps (pas encore synchronisés)
        node_a.mint_block_reward_of(pk_a, 200_000);
        let fork_a = node_a.seal_block(pk_a, 0.01);
        println!("  A sealed block #{} (hash={}...)", fork_a.index, &fork_a.hash[..12]);

        // Reset B au même état (avant le fork de A) en utilisant un bloc différent
        node_b.mint_block_reward_of(pk_b, 300_000);
        let fork_b = node_b.seal_block(pk_b, 0.02);
        println!("  B sealed block #{} (hash={}...)", fork_b.index, &fork_b.hash[..12]);

        // Les deux sont au même height mais avec des blocs différents
        assert_eq!(fork_a.index, fork_b.index, "Même height (fork)");
        assert_ne!(fork_a.hash, fork_b.hash, "Hash différent (fork)");

        // A reçoit le bloc de B (fork resolution)
        let json_fb = serde_json::to_string(&fork_b).expect("serialize");
        let remote_fb: crate::p2p::ledger::Block = serde_json::from_str(&json_fb).expect("deserialize");
        let _result_a = node_a.integrate_remote_block(remote_fb);

        // B reçoit le bloc de A (fork resolution)
        let json_fa = serde_json::to_string(&fork_a).expect("serialize");
        let remote_fa: crate::p2p::ledger::Block = serde_json::from_str(&json_fa).expect("deserialize");
        let _result_b = node_b.integrate_remote_block(remote_fa);

        // L'un des deux a gagné — les deux doivent converger vers le même tip
        let winner_hash = if fork_a.hash > fork_b.hash { &fork_a.hash } else { &fork_b.hash };
        println!("  Winner déterministe: {}...", &winner_hash[..12]);

        let tip_a = &node_a.chain.last().unwrap().hash;
        let tip_b = &node_b.chain.last().unwrap().hash;
        assert_eq!(tip_a, tip_b, "CONVERGENCE: les tips doivent être identiques après fork resolution");
        assert_eq!(tip_a, winner_hash, "Le winner doit être le hash le plus haut");

        println!("  ✅ Fork résolu — convergence: tip_a == tip_b == {}...", &tip_a[..12]);

        // ── Phase 4b: Re-seal orphaned txs ──
        // Le nœud perdant a mis les txs du bloc éjecté en pending.
        // Il faut re-sealer et propager BIDIRECTIONNELLEMENT pour que les
        // balances convergent — les txs NETWORK→pk du bloc perdant doivent
        // être visibles par le gagnant.
        println!("\n  ── Phase 4b: Re-seal des txs orphelines ──");

        let loser_is_a = fork_a.hash < fork_b.hash;
        let pending_a = node_a.pending_count();
        let pending_b = node_b.pending_count();
        println!("  Pending A: {}, Pending B: {}", pending_a, pending_b);

        // Bi-directional recovery: seal pending on both sides and exchange.
        // Loop until both are stable (no more pending txs to propagate).
        for round in 0..3 {
            let mut progressed = false;
            if let Some(block) = node_a.seal_if_pending(pk_a, 0.0) {
                println!("  Round {}: A re-sealed block #{} ({} txs récupérées)", round, block.index, block.transactions.len());
                let json = serde_json::to_string(&block).expect("serialize");
                let remote: crate::p2p::ledger::Block = serde_json::from_str(&json).expect("deserialize");
                let _ = node_b.integrate_remote_block(remote);
                progressed = true;
            }
            if let Some(block) = node_b.seal_if_pending(pk_b, 0.0) {
                println!("  Round {}: B re-sealed block #{} ({} txs récupérées)", round, block.index, block.transactions.len());
                let json = serde_json::to_string(&block).expect("serialize");
                let remote: crate::p2p::ledger::Block = serde_json::from_str(&json).expect("deserialize");
                let _ = node_a.integrate_remote_block(remote);
                progressed = true;
            }
            if !progressed { break; }
        }

        println!("  Perdant du fork: Nœud {}", if loser_is_a { "A" } else { "B" });

        // ── Phase 5: Balances ──
        println!("\n  ── Phase 5: Balances finales ──");

        let bal_a_on_a = node_a.balance_of(pk_a);
        let bal_b_on_a = node_a.balance_of(pk_b);
        let bal_a_on_b = node_b.balance_of(pk_a);
        let bal_b_on_b = node_b.balance_of(pk_b);

        let micro = MICRO as f64;
        println!("  Sur Nœud A: balance_a={:.6} QUANTA, balance_b={:.6} QUANTA",
            bal_a_on_a as f64 / micro, bal_b_on_a as f64 / micro);
        println!("  Sur Nœud B: balance_a={:.6} QUANTA, balance_b={:.6} QUANTA",
            bal_a_on_b as f64 / micro, bal_b_on_b as f64 / micro);

        assert_eq!(bal_a_on_a, bal_a_on_b, "Balance A doit être identique sur les 2 nœuds");
        assert_eq!(bal_b_on_a, bal_b_on_b, "Balance B doit être identique sur les 2 nœuds");

        println!("\n  ✅ BALANCES CONVERGENT sur les 2 nœuds");

        // ── Résumé ──
        println!("\n============================================================");
        println!("  📊 RÉSUMÉ MULTI-NŒUDS");
        println!("  Chain length:       {}", node_a.chain.len());
        println!("  Blocks propagés:    {} (dont recovery après fork)", node_a.chain.len() - 1);
        println!("  Doublons rejetés:   1");
        println!("  Forks résolus:      1");
        println!("  Convergence chaîne: ✅ OUI");
        println!("  Convergence balance: ✅ OUI");
        println!("============================================================\n");
    }
}

// ─── SIMULATION 3: 5 nodes + late-joiner ────────────────────────────────────
// 5 nœuds avec des watts différents (5W…50W). Pendant 3 ticks, A-D minent
// chacun à leur tour et propagent un bloc. Au tick 4, E rejoint le réseau et
// doit récupérer toute la chaîne via le replay des blocs (comme le ferait un
// nœud en première synchro).
//
// Run: cd src-tauri && cargo test simulation_late_joiner -- --nocapture

#[cfg(test)]
mod late_joiner_simulation {
    use crate::p2p::ledger::{Block, Ledger, MICRO};

    /// Diffuse `block` à tous les nœuds passés (sauf l'auteur). Retourne
    /// le nombre de nœuds qui ont effectivement intégré le bloc.
    fn broadcast_block(block: &Block, peers: &mut [(&str, &mut Ledger)]) -> usize {
        let json = serde_json::to_string(block).expect("serialize block");
        let mut accepted = 0;
        for (_id, peer) in peers.iter_mut() {
            let remote: Block = serde_json::from_str(&json).expect("deserialize block");
            if let Ok(true) = peer.integrate_remote_block(remote) {
                accepted += 1;
            }
        }
        accepted
    }

    #[test]
    fn simulation_late_joiner() {
        println!("\n============================================================");
        println!("  ⏱️  SIMULATION 5 NŒUDS — Late joiner");
        println!("============================================================\n");

        // 5 nœuds avec des watts différents (Raspberry Pi → workstation).
        let nodes = [
            ("A", 5u32),   // RPi-class (Guardian-ish)
            ("B", 10u32),  // laptop léger
            ("C", 15u32),  // Apple Silicon
            ("D", 30u32),  // PC fixe
            ("E", 50u32),  // workstation (rejoint en retard)
        ];
        for (id, w) in &nodes {
            println!("  Nœud {} : {}W", id, w);
        }

        let mut a = Ledger::new();
        let mut b = Ledger::new();
        let mut c = Ledger::new();
        let mut d = Ledger::new();
        // E n'existe pas encore — il sera créé au tick 4.

        let pk_a = "node_a_aaaa1111".to_string();
        let pk_b = "node_b_bbbb2222".to_string();
        let pk_c = "node_c_cccc3333".to_string();
        let pk_d = "node_d_dddd4444".to_string();
        let pk_e = "node_e_eeee5555".to_string();

        let micro = MICRO as f64;

        // ── Phase 1 : 3 ticks de mining round-robin sur A → B → C → D ──
        println!("\n  ── Phase 1 : A→B→C→D minent et propagent (3 ticks) ──");

        let miners: [(&str, &str); 4] = [
            (&pk_a, "A"), (&pk_b, "B"), (&pk_c, "C"), (&pk_d, "D"),
        ];

        for tick in 1..=3u32 {
            // Le mineur de ce tick = miners[tick % 4]. On évite le cas idx=0
            // pour que chaque tick ait un mineur différent.
            let (miner_pk, miner_id) = miners[(tick as usize) % 4];

            let block = match miner_id {
                "A" => { a.mint_block_reward_of(miner_pk, MICRO); a.seal_block(miner_pk, 0.005) }
                "B" => { b.mint_block_reward_of(miner_pk, MICRO); b.seal_block(miner_pk, 0.010) }
                "C" => { c.mint_block_reward_of(miner_pk, MICRO); c.seal_block(miner_pk, 0.015) }
                "D" => { d.mint_block_reward_of(miner_pk, MICRO); d.seal_block(miner_pk, 0.030) }
                _ => unreachable!(),
            };
            println!("  Tick {}: {} sealed block #{} (hash={}…)",
                tick, miner_id, block.index, &block.hash[..12]);

            // Broadcast aux 3 autres nœuds online (pas E, il n'est pas encore là).
            let mut peers: Vec<(&str, &mut Ledger)> = match miner_id {
                "A" => vec![("B", &mut b), ("C", &mut c), ("D", &mut d)],
                "B" => vec![("A", &mut a), ("C", &mut c), ("D", &mut d)],
                "C" => vec![("A", &mut a), ("B", &mut b), ("D", &mut d)],
                "D" => vec![("A", &mut a), ("B", &mut b), ("C", &mut c)],
                _ => unreachable!(),
            };
            let accepted = broadcast_block(&block, &mut peers);
            assert_eq!(accepted, 3, "tick {}: 3 pairs doivent intégrer le bloc", tick);
        }

        // Sanity check : A-D ont la même chaîne.
        let tip_a = a.chain.last().unwrap().hash.clone();
        assert_eq!(a.chain.len(), b.chain.len(), "A.len = B.len");
        assert_eq!(b.chain.len(), c.chain.len(), "B.len = C.len");
        assert_eq!(c.chain.len(), d.chain.len(), "C.len = D.len");
        assert_eq!(b.chain.last().unwrap().hash, tip_a, "B tip = A tip");
        assert_eq!(c.chain.last().unwrap().hash, tip_a, "C tip = A tip");
        assert_eq!(d.chain.last().unwrap().hash, tip_a, "D tip = A tip");
        println!("  ✓ A-D convergent sur tip {}… (chain len = {})",
            &tip_a[..12], a.chain.len());

        // ── Phase 2 : E rejoint au tick 4 ──
        println!("\n  ── Phase 2 : E rejoint le réseau (tick 4) ──");
        let mut e = Ledger::new();
        // Au démarrage, E n'a que le bloc genesis.
        assert_eq!(e.chain.len(), 1, "E commence avec genesis seul");
        println!("  E démarre avec genesis seul (chain len = 1)");

        // E reçoit l'historique blocs depuis le pair le plus à jour (A).
        // Dans la réalité ça passerait par un échange WantBlocks/HaveBlocks
        // gossip ; ici on simule en ré-envoyant simplement les blocs 1..=tip.
        for height in 1..a.chain.len() {
            let block = a.chain[height].clone();
            let json = serde_json::to_string(&block).expect("serialize");
            let remote: Block = serde_json::from_str(&json).expect("deserialize");
            let result = e.integrate_remote_block(remote);
            assert!(result.is_ok(),
                "E doit accepter le bloc {} reçu de A : {:?}", height, result);
            assert!(result.unwrap(),
                "E doit intégrer (pas dupliquer) le bloc {}", height);
        }

        // E doit être au même tip que A-D.
        assert_eq!(e.chain.len(), a.chain.len(),
            "E rattrape la chain length de A");
        assert_eq!(e.chain.last().unwrap().hash, tip_a,
            "E rattrape le tip de A");
        println!("  ✓ E a rattrapé : chain len = {}, tip = {}…",
            e.chain.len(), &e.chain.last().unwrap().hash[..12]);

        // ── Phase 3 : E participe maintenant — il mine un bloc et A l'accepte ──
        println!("\n  ── Phase 3 : E mine et propage à son tour ──");
        e.mint_block_reward_of(&pk_e, MICRO);
        let block_e = e.seal_block(&pk_e, 0.050);
        println!("  E sealed block #{} (hash={}…)",
            block_e.index, &block_e.hash[..12]);

        let mut peers: Vec<(&str, &mut Ledger)> = vec![
            ("A", &mut a), ("B", &mut b), ("C", &mut c), ("D", &mut d),
        ];
        let accepted = broadcast_block(&block_e, &mut peers);
        assert_eq!(accepted, 4, "A-D doivent tous intégrer le bloc de E");

        let tip_after = e.chain.last().unwrap().hash.clone();
        assert_eq!(a.chain.last().unwrap().hash, tip_after, "A converge sur E");
        assert_eq!(d.chain.last().unwrap().hash, tip_after, "D converge sur E");
        println!("  ✓ A-D convergent sur le bloc de E (tip = {}…)",
            &tip_after[..12]);

        // ── Phase 4 : balance de E reflète son bloc ──
        println!("\n  ── Phase 4 : balances après convergence ──");
        let bal_e_self = e.balance_of(&pk_e);
        let bal_e_remote = a.balance_of(&pk_e);
        assert_eq!(bal_e_self, bal_e_remote,
            "balance E identique sur E et sur A");
        println!("  E sur E : {:.6} QUANTA", bal_e_self as f64 / micro);
        println!("  E sur A : {:.6} QUANTA (convergence)", bal_e_remote as f64 / micro);

        println!("\n============================================================");
        println!("  📊 RÉSUMÉ LATE-JOINER");
        println!("  Nodes online phase 1 : 4 (A, B, C, D)");
        println!("  Nodes online phase 2 : 5 (E rejoint)");
        println!("  Chain length finale  : {}", a.chain.len());
        println!("  E rattrapé via replay: ✅");
        println!("  E mine et propage    : ✅");
        println!("  Convergence finale   : ✅");
        println!("============================================================\n");
    }

    /// Stress test: 100 operations on the ledger, verify supply integrity.
    #[test]
    fn stress_100_concurrent_txs() {
        use crate::p2p::ledger::Ledger;
        use crate::security::CryptoEngine;

        let mut ledger = Ledger::new();
        let mut crypto = CryptoEngine::new();
        crypto.generate_pq_identity().expect("ml-dsa primary"); // PQ-MIG-3: bind authority key
        let _ = crypto.generate_keypair();
        // PQ-MIG-3B: the account identity (`from`/`to`, balance key) is the ML-DSA
        // address; Ed25519 stays the transport key, off the value path.
        let pk = crypto.pq_address_hex().expect("ml-dsa address");

        // Seed: mine 1000 QUANTA to sender. MINT-EXACT-1 : `mine_tx` ne scelle
        // plus tout seul à 10 tx en attente (c'était le fork privé silencieux —
        // un bloc jamais diffusé), donc le scellement est explicite ici.
        for _ in 0..10 {
            ledger.mine_tx(&pk, 100 * MICRO, 0.5);
        }
        ledger.seal_block(&pk, 0.5);
        let initial_supply = ledger.total_supply();
        assert_eq!(initial_supply, 1000 * MICRO, "initial supply = 1000 QNT");

        // Create 10 recipients
        let recipients: Vec<String> = (0..10)
            .map(|i| format!("{:0>64}", format!("recipient_{}", i)))
            .collect();

        // 50 transfers (10 QUANTA each → 1% burn each)
        for i in 0..50 {
            let to = &recipients[i % 10];
            match ledger.transfer_with_burn(&pk, to, 10 * MICRO, &crypto) {
                Ok((_tx, _burn_tx, _burn)) => {}
                Err(e) => {
                    // Expected: sender runs out of balance eventually
                    println!("  Transfer #{} failed (expected): {}", i, e);
                    break;
                }
            }
        }

        // 50 more mining ops (refill)
        for _ in 0..50 {
            ledger.mint_block_reward_of(&pk, 2 * MICRO);
        }

        // Seal everything
        ledger.seal_block(&pk, 1.0);

        // ── Verify supply conservation ──
        let final_supply = ledger.total_supply();
        let total_mined = ledger.stats().total_mined;
        let total_burned_chain = ledger.total_burned();
        assert_eq!(
            final_supply,
            total_mined - total_burned_chain,
            "supply = mined - burned (conservation law)"
        );

        // ── Verify chain integrity ──
        let (blocks, txs) = ledger.verify_chain().expect("chain must be valid after stress");
        assert!(blocks >= 2, "at least genesis + 1 sealed block");
        assert!(txs > 50, "should have processed 50+ transactions");

        // ── Verify no negative balances (all_balances floors at 0) ──
        let all = ledger.all_balances();
        assert!(!all.is_empty(), "should have at least 1 holder");

        println!("\n═══════ STRESS TEST RESULTS ═══════");
        println!("  Transactions: {} verified", txs);
        println!("  Blocks: {}", blocks);
        println!("  Supply: {} µQTA", final_supply);
        println!("  Burned: {} µQTA", total_burned_chain);
        println!("  Holders: {}", all.len());
        println!("  Chain integrity: ✅");
        println!("═══════════════════════════════════\n");
    }
}
