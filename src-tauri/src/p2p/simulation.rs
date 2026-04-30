/// Simulation réseau SOVA — 2 nœuds, transfert, impact prix global
///
/// Run: cd src-tauri && cargo test simulation_reseau -- --nocapture

#[cfg(test)]
mod network_simulation {
    use crate::p2p::energy::EnergyOracle;
    use crate::p2p::consensus::CrdtLedger;
    use crate::p2p::sybil::SybilGuard;
    use crate::p2p::reputation::ReputationEngine;

    #[test]
    fn simulation_reseau() {
        let oracle = EnergyOracle::new();

        println!("\n============================================================");
        println!("  🌍 SIMULATION RÉSEAU SOVA — 2 nœuds");
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

        // Simuler 60 minutes de mining
        let mut total_mined = 0.0;
        // Simuler réseau 2 nœuds : total_network_watts = watts_a + watts_b
        let total_network_watts = watts_a + watts_b;
        for _ in 0..60 {
            let (atn_a, _) = engine.uptime_tick(pk_a, total_mined, total_network_watts);
            total_mined += atn_a;
            let (atn_b, _) = engine.uptime_tick(pk_b, total_mined, total_network_watts);
            total_mined += atn_b;
        }

        let user_a = engine.get_user(pk_a).unwrap();
        let user_b = engine.get_user(pk_b).unwrap();

        let kwh_a = watts_a / 1000.0; // kWh par heure
        let kwh_b = watts_b / 1000.0;
        let cost_a = kwh_a * prix_a;
        let cost_b = kwh_b * prix_b;

        println!("  Nœud A (France, {}W) :", watts_a as u32);
        println!("    ATN minés   : {:.4}", user_a.atn_earned);
        println!("    Énergie     : {:.4} kWh", kwh_a);
        println!("    Coût réel   : {:.5} EUR", cost_a);
        println!("    PoC score   : {:.3}", SybilGuard::poc_score(user_a));
        println!();
        println!("  Nœud B (USA, {}W) :", watts_b as u32);
        println!("    ATN minés   : {:.4}", user_b.atn_earned);
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
        // Créditer ce qu'ils ont miné
        crdt.credit("network", pk_a, (user_a.atn_earned * 1000.0) as u64);
        crdt.credit("network", pk_b, (user_b.atn_earned * 1000.0) as u64);

        println!("  Avant transfert :");
        println!("    A (France) : {:.4} ATN", crdt.balance_of(pk_a));
        println!("    B (USA)    : {:.4} ATN", crdt.balance_of(pk_b));

        // A envoie 0.5 ATN à B
        let transfer = 500; // 500 milliATN = 0.5 ATN
        crdt.debit(pk_a, pk_a, transfer);
        crdt.credit(pk_a, pk_b, transfer);

        println!("  Après transfert de 0.5 ATN (A → B) :");
        println!("    A (France) : {:.4} ATN", crdt.balance_of(pk_a));
        println!("    B (USA)    : {:.4} ATN", crdt.balance_of(pk_b));

        // ── Fusion CRDT (simulation sync) ────────────────
        println!("\n  FUSION CRDT (simulation sync entre 2 nœuds)");
        println!("  --------------------------------------------------");

        let mut crdt_b = CrdtLedger::new();
        crdt_b.credit("network", pk_b, 200); // B a miné 0.2 ATN de son côté

        println!("  Nœud B local avant sync : {:.4} ATN", crdt_b.balance_of(pk_b));
        crdt_b.merge(&crdt);
        println!("  Nœud B après merge      : {:.4} ATN (convergence !)", crdt_b.balance_of(pk_b));

        println!("\n  Supply totale réseau : {:.4} SOVA", total_mined);
        println!("  Émission V2          : 100 SOVA/h fixe, pas de halving");

        println!("\n============================================================");
        println!("  ✅ Simulation terminée");
        println!("============================================================\n");
    }
}
