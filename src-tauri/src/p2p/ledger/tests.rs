// p2p/ledger/tests.rs — inline test module, moved verbatim from ledger.rs
// (organizational only — pure move; 4-space indent preserved to avoid rewriting
//  any line of the audited test suite).

    use super::*;

    /// PQ-MIG-3 test wallet: an engine carrying an **independent ML-DSA primary**
    /// (the tx-authority key the ledger binds). Returned without an Ed25519
    /// keypair yet — callers that need one still call `generate_keypair()`, which
    /// sets the Ed25519 layer **without disturbing** the primary. So a wallet that
    /// signs a tx satisfies `verify_tx`'s mandatory ML-DSA layer + the binding.
    /// Uses `CryptoEngine::default()` (≡ `new()`) so the engine starts empty.
    fn pq_wallet() -> CryptoEngine {
        let mut c = CryptoEngine::default();
        c.generate_pq_identity().expect("ml-dsa primary");
        c
    }

    /// PQ-MIG-3B test helper — a wallet's **account identity** is its ML-DSA
    /// address (`from`/`to`/miner/balance key under ADR-007 b). Installs the
    /// Ed25519 signing layer (so `sign_tx_authority` co-signs the pre-image) and
    /// returns `pq_address_hex()`. Replaces the model-(a) `…public_key_hex`.
    fn gen_addr(c: &mut CryptoEngine) -> String {
        c.generate_keypair();
        c.pq_address_hex().expect("ml-dsa address")
    }

    // ─── NET-14: Mempool eviction tests ─────────────────────────────────

    fn old_timestamp(secs_ago: i64) -> String {
        let t = chrono::Utc::now() - chrono::Duration::seconds(secs_ago);
        t.to_rfc3339()
    }

    #[test]
    fn prune_mempool_evicts_expired_txs() {
        // Insert two mining txs (mine_tx pushes to pending), then backdate
        // one to before TTL. prune_mempool must evict the stale one only.
        let mut ledger = Ledger::new();
        let pk_a = "a".repeat(64);
        let pk_b = "b".repeat(64);
        let _fresh = ledger.mine_tx(&pk_a, 1, 0.0);
        let stale = ledger.mine_tx(&pk_b, 1, 0.0);

        // Backdate the second pending entry to 11 minutes ago (> TTL).
        let stale_id = stale.id.clone();
        if let Some(p) = ledger.pending.iter_mut().find(|t| t.id == stale_id) {
            p.timestamp = old_timestamp(660);
        }
        let before = ledger.pending_count();
        let evicted = ledger.prune_mempool();
        assert_eq!(evicted, 1, "exactly the stale tx must be pruned");
        assert_eq!(ledger.pending_count(), before - 1);
    }

    #[test]
    fn prune_mempool_at_is_driven_by_injected_time() {
        // Phase 0 (T0.1): eviction is a pure function of the INJECTED clock, so
        // the deterministic core prunes reproducibly. Same pending state pruned
        // at two different injected times ⇒ different result ⇒ time is an input.
        let mut ledger = Ledger::new();
        let pk = "a".repeat(64);
        let tx = ledger.mine_tx(&pk, 1, 0.0);

        // Pin the pending tx to a fixed absolute instant.
        let fixed = "2026-03-01T12:00:00+00:00";
        let t0 = chrono::DateTime::parse_from_rfc3339(fixed)
            .unwrap()
            .timestamp();
        let id = tx.id.clone();
        if let Some(p) = ledger.pending.iter_mut().find(|t| t.id == id) {
            p.timestamp = fixed.to_string();
        }

        // Within the TTL window → nothing evicted.
        assert_eq!(
            ledger.prune_mempool_at(t0 + Ledger::MEMPOOL_TTL_SECS - 1),
            0
        );
        // Just past the TTL → the same tx is now evicted.
        assert_eq!(
            ledger.prune_mempool_at(t0 + Ledger::MEMPOOL_TTL_SECS + 1),
            1
        );
    }

    #[test]
    fn prune_mempool_caps_at_max() {
        let mut ledger = Ledger::new();
        // Push synthetic NETWORK -> X transactions through the mining path so
        // we don't have to sign anything. These all get FRESH timestamps so
        // pass the TTL filter — the cap is the only thing that should evict.
        let target_pk = "f".repeat(64);
        // Insert MEMPOOL_MAX + 5 entries.
        for _ in 0..(Ledger::MEMPOOL_MAX as u32 + 5) {
            ledger.mine_tx(&target_pk, 1, 0.0);
        }
        let _ = ledger.prune_mempool();
        assert!(
            ledger.pending_count() <= Ledger::MEMPOOL_MAX,
            "pending must be capped at MEMPOOL_MAX, got {}",
            ledger.pending_count()
        );
    }

    #[test]
    fn prune_mempool_no_op_on_fresh_pool() {
        let mut ledger = Ledger::new();
        let target_pk = "0".repeat(64);
        for _ in 0..5 {
            ledger.mine_tx(&target_pk, 1, 0.0);
        }
        let before = ledger.pending_count();
        let evicted = ledger.prune_mempool();
        assert_eq!(evicted, 0, "no eviction expected on a fresh pool under cap");
        assert_eq!(ledger.pending_count(), before);
    }

    #[test]
    fn signed_tx_uses_hybrid_path() {
        // PQ-MIG-3: une tx signée porte des champs PQ **non vides** (la clé +
        // signature ML-DSA du primaire), et `verify_tx` exige strictement les deux
        // couches (Ed25519 + ML-DSA) — il n'y a plus de repli Ed25519 seul.
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        // Donne un solde au signataire via une mining tx réseau (5 QUANTA = 5_000_000
        // µQTA).
        ledger.mine_tx(&id, 5 * MICRO, 0.0);

        // Adresse destinataire valide (64 chars hex)
        let to = "b".repeat(64);
        let tx = ledger
            .transfer_tx(&id, &to, MICRO, &crypto)
            .expect("transfer should succeed");

        assert!(
            tx.pq_signature.is_some(),
            "tx signée doit porter le champ pq_signature"
        );
        assert!(
            tx.pq_public_key.is_some(),
            "tx signée doit porter le champ pq_public_key"
        );
        assert!(
            !tx.pq_signature.as_deref().unwrap_or("").is_empty(),
            "la couche ML-DSA est non vide (autorité PQ obligatoire)"
        );
        assert!(
            Ledger::verify_tx(&tx).unwrap(),
            "verify_tx doit valider la tx (Ed25519 + ML-DSA stricts)"
        );
    }

    #[test]
    fn network_tx_skips_signature() {
        // Une mining tx réseau (from=NETWORK) n'a pas à porter de signature.
        let to = "c".repeat(64);
        let mut ledger = Ledger::new();
        // 0.5 QUANTA = 500_000 µQTA
        let tx = ledger.mine_tx(&to, MICRO / 2, 0.0);
        assert_eq!(tx.from, "NETWORK");
        assert!(
            Ledger::verify_tx(&tx).unwrap(),
            "tx NETWORK toujours valide"
        );
    }

    // ─── PQ-MIG-5 : genèse post-quantique (les dents §5) ────────────────────

    /// **PQ-MIG-5 §5 — genèse déterministe.** Deux constructions de la même
    /// allocation ⇒ hash **identique** (C1), et le hash est **figé** sur un vecteur
    /// connu (comme les vecteurs d'adresses épinglés de PQ-MIG-2). La genèse par
    /// défaut (`new()`, allocation vide, zéro premine) et la genèse d'allocation DEV
    /// ont chacune un hash fixe ; le hash **lie le contenu** — changer l'allocation
    /// change le hash (vide ≠ alloué).
    #[test]
    fn pqmig5_genesis_hash_is_deterministic_and_frozen() {
        assert_eq!(
            Ledger::new().chain[0].hash,
            Ledger::new().chain[0].hash,
            "genèse vide : deux constructions ⇒ même hash (C1)"
        );
        assert_eq!(
            Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION).chain[0].hash,
            Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION).chain[0].hash,
            "genèse allouée : deux constructions ⇒ même hash (C1)"
        );
        // Vecteurs figés — GENESIS-V4 (timestamp 2026-07-18), recalculés via le
        // hachage de bloc existant. Le changement délibéré de la genèse met à jour
        // ces vecteurs gelés (c'est leur rôle : verrouiller la genèse contre toute
        // dérive silencieuse ; une refonte volontaire les réinitialise).
        assert_eq!(
            Ledger::new().chain[0].hash,
            "ee58235deda396dbb7c84ac2e86829c990fa5562e89b4edcb80caf19ac2a1dff",
            "hash de genèse vide figé (v4)"
        );
        assert_eq!(
            Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION).chain[0].hash,
            "875cb2b2a8912b76db46af04eae064b06e5a2ea5183a9ed030b4bee14a925160",
            "hash de genèse DEV figé (v4)"
        );
        assert_ne!(
            Ledger::new().chain[0].hash,
            Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION).chain[0].hash,
            "le hash lie le contenu : vide ≠ alloué"
        );
    }

    /// **PROPOSER-1 (GENESIS-V4) — the CRITICAL is closed on receive.** Once a
    /// validator is bonded, a block whose proposer is NOT a bonded validator is
    /// rejected on the receive path (`integrate_remote_block`), while a block
    /// proposed by a bonded validator is accepted. This is the fix for
    /// "any address could seal any slot and the network accepted it".
    #[test]
    fn proposer1_rejects_unbonded_proposer_once_staked() {
        use crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE;
        // DEV genesis already bonds G0 (10 QTA) and G1 (5 QTA) — both ≥ MIN.
        let base = Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION);
        assert!(
            base.validator_stakes().values().any(|&s| s >= MIN_VALIDATOR_STAKE),
            "précondition : au moins un validateur bondé"
        );

        // (a) a block proposed by a NON-validator address is REJECTED.
        let attacker = "z".repeat(64);
        let mut evil_src = base.clone();
        let evil = evil_src.seal_block_at(&attacker, 0.0, "2026-07-19T00:00:00Z".into());
        assert_eq!(evil.miner, attacker, "le bloc malveillant se scelle sous l'attaquant");
        let mut a = base.clone();
        assert!(
            a.integrate_remote_block(evil).is_err(),
            "PROPOSER-1 : un proposeur non bondé est rejeté à la réception"
        );
        assert_eq!(a.chain_height(), 1, "chaîne non étendue par le proposeur non bondé");

        // (b) a block proposed by a bonded genesis validator is ACCEPTED.
        let mut good_src = base.clone();
        let good = good_src.seal_block_at(Ledger::GENESIS_ADDR_0, 0.0, "2026-07-19T00:00:00Z".into());
        let mut b = base.clone();
        assert!(
            b.integrate_remote_block(good).is_ok(),
            "un proposeur validateur bondé est accepté"
        );
        assert_eq!(b.chain_height(), 2, "la chaîne est étendue par le validateur");
    }

    /// **PROPOSER-1 bootstrap — permissionless before anyone stakes.** On the fresh
    /// empty v4 genesis (zero validators) any proposer may seal, so the chain can
    /// start; the check only bites once `has_eligible` becomes true.
    #[test]
    fn proposer1_bootstrap_allows_any_proposer() {
        let base = Ledger::new(); // empty v4 genesis, zero bonded validators
        assert!(base.validator_stakes().is_empty(), "aucun validateur au démarrage");
        let anyone = "b".repeat(64);
        let mut src = base.clone();
        let blk = src.seal_block_at(&anyone, 0.0, "2026-07-19T00:00:00Z".into());
        let mut a = base.clone();
        assert!(
            a.integrate_remote_block(blk).is_ok(),
            "bootstrap : proposeur permissionless accepté tant que personne n'a staké"
        );
        assert_eq!(a.chain_height(), 2);
    }

    /// **PROPOSER-1 determinism lock.** `staked_before(tip)` (the pure chain replay
    /// used on the rare fork tie-break) MUST equal the live `validator_stakes()`
    /// (used on the O(1) linear/clone paths), so every admission path computes the
    /// identical bonded set as of the parent — no node ever disagrees on a
    /// proposer's eligibility.
    #[test]
    fn staked_before_matches_live_cache() {
        let l = Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION);
        let tip = l.chain.last().unwrap().clone();
        assert_eq!(
            l.staked_before(&tip),
            l.validator_stakes(),
            "staked_before(tip) == validator_stakes() (les deux sources de PROPOSER-1 concordent)"
        );
    }

    /// **PQ-MIG-5 §5 — conservation à la genèse (EMIT-1 / classe conservation).**
    /// Au bloc 0 : `miné == Σ(solde + enjeu)` (l'enjeu vient de l'allocation, pas
    /// créé en plus) et `Σ dépensable + enjeu-verrouillé + brûlé == miné`. Puis la
    /// **dent négative** : planter un enjeu de genèse **non couvert** par une
    /// émission (un `Stake` sans `Mining` derrière) **casse** l'égalité de
    /// conservation — la vérification mord, un déséquilibre de genèse ne passe pas.
    #[test]
    fn pqmig5_dev_genesis_conserves_at_block_zero() {
        let l = Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION);
        let expected_alloc: u64 = Ledger::DEV_GENESIS_ALLOCATION
            .iter()
            .map(|(_, bal, stake)| bal + stake)
            .sum();
        assert_eq!(l.total_minted(), expected_alloc, "miné == Σ(solde + enjeu)");
        assert_eq!(l.total_minted(), 100 * MICRO, "allocation DEV = 100 QTA");
        assert_eq!(
            conservation_lhs(&l),
            l.total_minted(),
            "Σ dépensable + enjeu-verrouillé + brûlé == miné, dès le bloc 0"
        );
        assert_eq!(l.total_burned(), 0, "rien de brûlé à la genèse");
        assert_eq!(l.locked_stake_total(), 15 * MICRO, "enjeu de genèse = 10 + 5");

        // Dent négative — planter un enjeu non couvert (Stake sans Mining derrière).
        let mut bad = Ledger::genesis_with_allocation(&[]); // miné 0
        let planted = Ledger::genesis_tx(
            "planted_unbacked_stake",
            Ledger::GENESIS_ADDR_0,
            Ledger::STAKE_SINK,
            10 * MICRO,
            TxType::Stake,
        );
        bad.chain[0].transactions.push(planted);
        bad.rebuild_cache();
        assert_eq!(bad.total_minted(), 0, "aucune émission ne couvre l'enjeu planté");
        assert_eq!(bad.locked_stake_total(), 10 * MICRO, "10 verrouillés sans backing");
        assert_ne!(
            conservation_lhs(&bad),
            bad.total_minted(),
            "enjeu de genèse non couvert ⇒ conservation cassée (la vérif mord)"
        );
    }

    /// **PQ-MIG-5 §5 — validateurs initiaux.** `validator_stakes()` au bloc 0
    /// reflète **exactement** le mapping de genèse, indexé par **adresse ML-DSA** :
    /// les deux comptes à enjeu > 0 (G0, G1), pas le porteur simple (G2, enjeu 0).
    #[test]
    fn pqmig5_dev_genesis_validators_reflect_mapping() {
        let l = Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION);
        let stakes = l.validator_stakes();
        assert_eq!(stakes.len(), 2, "deux validateurs initiaux (enjeu > 0)");
        assert_eq!(stakes.get(Ledger::GENESIS_ADDR_0).copied(), Some(10 * MICRO));
        assert_eq!(stakes.get(Ledger::GENESIS_ADDR_1).copied(), Some(5 * MICRO));
        assert_eq!(
            stakes.get(Ledger::GENESIS_ADDR_2).copied(),
            None,
            "le porteur sans enjeu n'est pas un validateur"
        );
        // Mêmes valeurs via l'accès par compte (chaîne ⇒ état pur).
        assert_eq!(l.staked_of(Ledger::GENESIS_ADDR_0), 10 * MICRO);
        assert_eq!(l.staked_of(Ledger::GENESIS_ADDR_1), 5 * MICRO);
        assert_eq!(l.staked_of(Ledger::GENESIS_ADDR_2), 0);
        // Soldes dépensables = colonne « solde » du mapping (enjeu retiré).
        assert_eq!(l.balance_of(Ledger::GENESIS_ADDR_0), 50 * MICRO);
        assert_eq!(l.balance_of(Ledger::GENESIS_ADDR_1), 25 * MICRO);
        assert_eq!(l.balance_of(Ledger::GENESIS_ADDR_2), 10 * MICRO);
    }

    // ─── TOKENOMICS v2 — invariants de CONFIANCE (offre prouvable) ──────────

    #[test]
    fn trust_no_premine_at_genesis() {
        let ledger = Ledger::new();
        assert_eq!(
            ledger.total_supply(),
            0,
            "aucune offre à la genèse (zéro premine)"
        );
        assert_eq!(ledger.stats().total_mined, 0, "rien de miné à la genèse");
        assert!(
            ledger.balance_cache.is_empty(),
            "aucun compte crédité à la genèse"
        );
        assert_eq!(ledger.chain[0].transactions.len(), 0, "bloc de genèse vide");
    }

    #[test]
    fn trust_burn_reduces_total_supply() {
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id, 100 * MICRO, 0.0);
        ledger.seal_block(&id, 0.0); // pending → chaîne (compté dans l'offre)
        let before = ledger.total_supply();
        let to = "e".repeat(64);
        let (_tx, _burn, burn_uqta) = ledger
            .transfer_with_burn(&id, &to, 50 * MICRO, &crypto)
            .unwrap();
        ledger.seal_block(&id, 0.0);
        assert!(burn_uqta > 0);
        // Déflation réelle : l'offre diminue EXACTEMENT du montant brûlé.
        assert_eq!(
            ledger.total_supply(),
            before - burn_uqta,
            "le burn doit détruire de l'offre (net-destructeur)"
        );
    }

    #[test]
    fn trust_supply_equals_sum_of_balances() {
        // Invariant fort : offre en circulation == somme des soldes de comptes.
        let mut crypto = pq_wallet();
        let a = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&a, 100 * MICRO, 0.0);
        let b = "f".repeat(64);
        ledger
            .transfer_with_burn(&a, &b, 30 * MICRO, &crypto)
            .unwrap();
        ledger.seal_block(&a, 0.0); // chaîne et cache cohérents
        let sum: i128 = ledger.balance_cache.values().sum();
        assert_eq!(
            sum as u64,
            ledger.total_supply(),
            "Σ soldes doit égaler l'offre (mined − burned)"
        );
    }

    #[test]
    fn trust_only_network_mints_and_transfers_conserve() {
        // Seul chemin de création monétaire : mine_tx depuis "NETWORK".
        let mut ledger = Ledger::new();
        let to = "a".repeat(64);
        let tx = ledger.mine_tx(&to, MICRO, 0.0);
        assert_eq!(
            tx.from, "NETWORK",
            "création monétaire uniquement depuis NETWORK"
        );
        assert_eq!(tx.tx_type, TxType::Mining);
        // Un transfert (sans burn) ne crée AUCUNE offre.
        let mut crypto = pq_wallet();
        let s = gen_addr(&mut crypto);
        ledger.mine_tx(&s, 10 * MICRO, 0.0);
        let supply = ledger.total_supply();
        let r = "b".repeat(64);
        ledger
            .transfer_tx(&s, &r, 2 * MICRO, &crypto)
            .unwrap();
        assert_eq!(
            ledger.total_supply(),
            supply,
            "un transfert ne crée pas d'offre"
        );
    }

    #[test]
    fn trust_remote_block_cannot_exceed_hard_cap() {
        // Un pair MALVEILLANT forge un bloc qui se crédite plus que le plafond.
        // Le consensus DOIT le rejeter (plafond infalsifiable réseau).
        let mut ledger = Ledger::new();
        let tip = ledger.chain.last().unwrap().clone();
        let attacker = "a".repeat(64);
        let huge = crate::p2p::reputation::MAX_SUPPLY_MICRO + 5 * MICRO;
        let evil_tx = ledger.build_unsigned_tx("NETWORK", &attacker, huge, TxType::Mining);

        let txs = vec![evil_tx];
        let index = tip.index + 1;
        let ts = Utc::now().to_rfc3339();
        let tx_root = Ledger::compute_merkle_root(&txs);
        // BLK-HASH-1: pre-image now includes the miner.
        let payload = format!(
            "{}:{}:{}:{}:{}:{}",
            index,
            tip.hash,
            ts,
            attacker,
            txs.len(),
            tx_root
        );
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        let evil_block = Block {
            index,
            timestamp: ts,
            transactions: txs,
            prev_hash: tip.hash.clone(),
            hash,
            miner: attacker,
            energy_kwh: 0.0,
        };

        let res = ledger.integrate_remote_block(evil_block);
        assert!(
            res.is_err(),
            "le bloc dépassant le plafond doit être rejeté"
        );
        assert_eq!(
            ledger.stats().total_mined,
            0,
            "offre inchangée (bloc rejeté)"
        );
        assert_eq!(
            ledger.chain_height(),
            1,
            "chaîne non étendue par le bloc malveillant"
        );
    }

    #[test]
    fn trust_remote_block_emission_per_block_bounded() {
        // Un sceleur forge un bloc qui se crédite 1 000 000 QUANTA — TRÈS sous le
        // plafond total (100M), mais des milliers de fois l'émission/tick légitime.
        // La borne PAR BLOC doit le rejeter : impossible de rafler l'émission d'un
        // coup.
        let mut ledger = Ledger::new();
        let tip = ledger.chain.last().unwrap().clone();
        let attacker = "b".repeat(64);
        let greedy = 1_000_000 * MICRO; // 1M QUANTA, ≫ emission_for_tick(0) × 64
        let evil_tx = ledger.build_unsigned_tx("NETWORK", &attacker, greedy, TxType::Mining);

        let txs = vec![evil_tx];
        let index = tip.index + 1;
        let ts = Utc::now().to_rfc3339();
        let tx_root = Ledger::compute_merkle_root(&txs);
        // BLK-HASH-1: pre-image now includes the miner.
        let payload = format!(
            "{}:{}:{}:{}:{}:{}",
            index,
            tip.hash,
            ts,
            attacker,
            txs.len(),
            tx_root
        );
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        let evil_block = Block {
            index,
            timestamp: ts,
            transactions: txs,
            prev_hash: tip.hash.clone(),
            hash,
            miner: attacker,
            energy_kwh: 0.0,
        };

        let res = ledger.integrate_remote_block(evil_block);
        assert!(
            res.is_err(),
            "un bloc minant 1M QUANTA d'un coup doit être rejeté (borne par bloc)"
        );
        assert_eq!(
            ledger.stats().total_mined,
            0,
            "offre inchangée (bloc rejeté)"
        );
        assert_eq!(ledger.chain_height(), 1, "chaîne non étendue");
    }

    // ─── FORK-CAP-1: le chemin fork-reorg applique la MÊME validation ───────
    // d'émission que le chemin linéaire. L'audit HARDEN-AUDIT-1 a confirmé 2×
    // que la branche de reorg sautait `validate_block_emission`, laissant un
    // adversaire réseau minter au-delà des 100M via un bloc de remplacement.
    // Ces tests pilotent la BRANCHE REORG (même hauteur, hash gagnant), là où
    // les anciens tests de plafond ne pilotaient que le chemin happy (tip+1).

    /// Construit une chaîne genesis + 1 bloc scellé (tip à la hauteur 1), pour
    /// qu'un reorg soit possible (`tip.index >= 1`). Renvoie le ledger, le tip
    /// honnête, et le hash de genesis auquel le fork doit se rattacher.
    fn forkcap_two_block_chain() -> (Ledger, Block, String) {
        let mut ledger = Ledger::new();
        let genesis_hash = ledger.chain.last().unwrap().hash.clone();
        let honest_miner = "f".repeat(64);
        ledger.mine_tx(&honest_miner, 2 * MICRO, 0.0); // émission légitime
        let tip = ledger.seal_block(&honest_miner, 0.0);
        assert_eq!(tip.index, 1, "setup: le tip doit être à la hauteur 1 pour un reorg");
        (ledger, tip, genesis_hash)
    }

    /// Forge un bloc de fork avec un hash CORRECT (même pré-image que la prod)
    /// qui bat lexicographiquement `must_beat`, pour gagner le tie-break et
    /// atteindre la validation. Le timestamp (libre, seulement haché) sert de
    /// nonce de grind — BLAKE3 est ~uniforme, donc quelques essais suffisent.
    fn forge_winning_fork(
        index: u64,
        prev_hash: &str,
        miner: &str,
        txs: Vec<Transaction>,
        must_beat: &str,
    ) -> Block {
        for nonce in 0..1_000_000u64 {
            let ts = format!("forkcap-grind-{nonce}");
            let block = Ledger::forge_block_at(index, prev_hash, &ts, miner, txs.clone());
            if block.hash.as_str() > must_beat {
                return block;
            }
        }
        panic!("impossible de grinder un hash de fork au-dessus du tip honnête");
    }

    /// Après un reorg REJETÉ : la chaîne n'a pas bougé, le tip honnête reste, et
    /// la masse en circulation ne dépasse JAMAIS le plafond dur.
    fn forkcap_assert_rejected(ledger: &Ledger, honest_tip: &Block) {
        assert_eq!(ledger.chain.len(), 2, "chaîne non réorganisée (reorg rejeté)");
        assert_eq!(
            ledger.chain.last().unwrap().hash,
            honest_tip.hash,
            "le tip honnête doit être conservé"
        );
        assert_eq!(
            ledger.stats().total_mined,
            2 * MICRO,
            "offre minée inchangée par le bloc rejeté"
        );
        assert!(
            ledger.total_supply() <= crate::p2p::reputation::MAX_SUPPLY_MICRO,
            "la masse ne dépasse JAMAIS 100M"
        );
    }

    #[test]
    fn forkcap_reorg_rejects_multiple_mining_rewards() {
        // (a) Plus d'une récompense de minage dans le bloc de reorg.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let attacker = "a".repeat(64);
        let r1 = ledger.build_unsigned_tx("NETWORK", &attacker, MICRO, TxType::Mining);
        let r2 = ledger.build_unsigned_tx("NETWORK", &attacker, MICRO, TxType::Mining);
        let evil = forge_winning_fork(tip.index, &genesis_hash, &attacker, vec![r1, r2], &tip.hash);
        assert!(
            evil.hash.as_str() > tip.hash.as_str(),
            "le fork doit gagner le tie-break pour atteindre la validation"
        );

        let res = ledger.integrate_remote_block(evil);
        assert!(res.is_err(), "(a) reorg à ≥2 récompenses de minage doit être REJETÉ");
        forkcap_assert_rejected(&ledger, &tip);
    }

    #[test]
    fn forkcap_reorg_rejects_emission_past_hard_cap() {
        // (b) Récompense sur-dimensionnée poussant la masse au-delà du plafond.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let attacker = "a".repeat(64);
        let huge = crate::p2p::reputation::MAX_SUPPLY_MICRO + 5 * MICRO;
        let reward = ledger.build_unsigned_tx("NETWORK", &attacker, huge, TxType::Mining);
        let evil = forge_winning_fork(tip.index, &genesis_hash, &attacker, vec![reward], &tip.hash);

        let res = ledger.integrate_remote_block(evil);
        assert!(
            res.is_err(),
            "(b) reorg dépassant le plafond dur 100M doit être REJETÉ"
        );
        forkcap_assert_rejected(&ledger, &tip);
    }

    #[test]
    fn forkcap_reorg_rejects_emission_per_block_bound() {
        // (b′) 1M QUANTA d'un coup — très sous 100M, mais des milliers de fois
        // l'émission/tick légitime : la borne PAR BLOC doit aussi le rejeter sur
        // le chemin de reorg.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let attacker = "a".repeat(64);
        let greedy = 1_000_000 * MICRO;
        let reward = ledger.build_unsigned_tx("NETWORK", &attacker, greedy, TxType::Mining);
        let evil = forge_winning_fork(tip.index, &genesis_hash, &attacker, vec![reward], &tip.hash);

        let res = ledger.integrate_remote_block(evil);
        assert!(
            res.is_err(),
            "(b′) reorg minant 1M QUANTA d'un coup doit être REJETÉ (borne par bloc)"
        );
        forkcap_assert_rejected(&ledger, &tip);
    }

    #[test]
    fn forkcap_reorg_rejects_reward_to_non_miner() {
        // (c) Récompense créditée à un autre que le mineur du bloc.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let miner = "a".repeat(64);
        let someone_else = "c".repeat(64);
        let reward = ledger.build_unsigned_tx("NETWORK", &someone_else, MICRO, TxType::Mining);
        let evil = forge_winning_fork(tip.index, &genesis_hash, &miner, vec![reward], &tip.hash);

        let res = ledger.integrate_remote_block(evil);
        assert!(
            res.is_err(),
            "(c) reorg créditant un non-mineur doit être REJETÉ"
        );
        forkcap_assert_rejected(&ledger, &tip);
    }

    #[test]
    fn forkcap_reorg_accepts_honest_competing_block() {
        // Contrôle positif (anti-masquage) : un fork HONNÊTE de même hauteur,
        // émission légitime + hash gagnant, DOIT être accepté. Fermer le plafond
        // ne doit pas rejeter les reorgs honnêtes ni « filtrer » par excès. Le
        // `prior_mined` exclut le tip remplacé, donc 2 QUANTA valident contre une
        // offre de 0, exactement comme un bloc à la hauteur 1.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let rival_miner = "e".repeat(64);
        let reward = ledger.build_unsigned_tx("NETWORK", &rival_miner, 2 * MICRO, TxType::Mining);
        let honest_fork =
            forge_winning_fork(tip.index, &genesis_hash, &rival_miner, vec![reward], &tip.hash);

        let res = ledger.integrate_remote_block(honest_fork.clone());
        assert_eq!(
            res,
            Ok(true),
            "un fork honnête de même hauteur (hash gagnant) doit être accepté"
        );
        assert_eq!(ledger.chain.len(), 2, "la chaîne reste à la même hauteur après reorg");
        assert_eq!(
            ledger.chain.last().unwrap().hash,
            honest_fork.hash,
            "le fork honnête devient le tip"
        );
        assert_eq!(
            ledger.stats().total_mined,
            2 * MICRO,
            "offre minée cohérente après un reorg honnête (le nouveau bloc remplace l'ancien)"
        );
        assert!(
            ledger.total_supply() <= crate::p2p::reputation::MAX_SUPPLY_MICRO,
            "masse ≤ 100M"
        );
    }

    // ─── GADGET-5B: multi-block fork reconciliation (reorg_to_fork) ─────────
    // The ledger mechanics the GHOST engine drives at partition heal: a
    // competing fork is adopted with full conservation (loser emission reverted,
    // user txs re-queued, synthetic rewards dropped) and the finalized floor is
    // absolute (a fork that would undo finalized history is refused).

    /// **GADGET-5B §2 tooth — conservation revert is load-bearing.** Reorging to
    /// a competing fork REVERTS the abandoned branch's emission (so it is never
    /// double-minted) and RE-QUEUES its user txs (never lost, AUDIT-BLK-1), while
    /// its synthetic reward is dropped — and `Σ(spendable+staked+unbonding)+burned
    /// == minted` still holds globally afterwards.
    #[test]
    fn gadget5b_reorg_reverts_loser_emission_and_requeues_user_tx() {
        let mut ledger = Ledger::new();
        let mut wallet = pq_wallet();
        let pk = gen_addr(&mut wallet);
        let y = "d".repeat(64);

        // G, b1 (mining → pk 100 QTA, funds the transfer).
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        let b1 = ledger.seal_block(&pk, 0.0);
        assert_eq!(b1.index, 1);

        // A signed transfer pk→y (10 QTA) + its 1% burn, deterministic.
        let ts = "2026-06-25T00:00:00+00:00".to_string();
        let (xfer, burn_opt, _) = ledger
            .transfer_with_burn_at(&pk, &y, 10 * MICRO, &wallet, ts.clone(), true)
            .expect("signed transfer");
        let burn = burn_opt.expect("burn leg present");

        // LOSER block (index 2): the user transfer + burn + a 3 QTA mining reward
        // (MINT-EXACT-1 : ≤ la récompense canonique du bloc, sinon rejeté).
        let loser_reward = ledger.build_unsigned_tx("NETWORK", &pk, 3 * MICRO, TxType::Mining);
        let loser = Ledger::forge_block_at(
            2,
            &b1.hash,
            &ts,
            &pk,
            vec![xfer.clone(), burn.clone(), loser_reward],
        );
        assert_eq!(ledger.integrate_remote_block(loser.clone()), Ok(true), "loser sealed at index 2");
        assert_eq!(ledger.pending_count(), 0, "transfer + burn now sealed in the loser block");
        assert_eq!(ledger.total_minted(), 103 * MICRO, "b1(100) + loser mining(3)");

        // WINNER fork (index 2, rooted at b1): mining only (2 QTA) — replaces the loser.
        let winner_reward = ledger.build_unsigned_tx("NETWORK", &pk, 2 * MICRO, TxType::Mining);
        let winner = Ledger::forge_block_at(2, &b1.hash, &ts, &pk, vec![winner_reward]);

        assert_eq!(
            ledger.reorg_to_fork(std::slice::from_ref(&winner), 0),
            Ok(true),
            "the competing fork is adopted (floor 0, valid)"
        );

        // (a) the winner is the new tip.
        assert_eq!(ledger.chain.last().unwrap().hash, winner.hash, "winner is the tip");
        assert_eq!(ledger.chain.len(), 3, "same height after the reorg");

        // (b) the loser's USER txs are re-queued; its mining reward is NOT.
        let pending = ledger.pending_txs();
        assert!(
            pending.iter().any(|t| t.hash == xfer.hash && t.tx_type == TxType::Transfer),
            "the loser's transfer is re-queued (AUDIT-BLK-1)"
        );
        assert!(
            pending.iter().any(|t| t.hash == burn.hash && t.tx_type == TxType::Burn),
            "the loser's burn (a user tx) is re-queued"
        );
        assert!(
            pending.iter().all(|t| t.tx_type != TxType::Mining),
            "the loser's mining reward is NOT re-queued — no double-mint (EMIT-1 §4.1)"
        );

        // (c) the loser's emission was UNDONE: minted counts the winner's 2 QTA,
        //     not the dropped loser's 3 (b1 100 + winner 2 = 102).
        assert_eq!(
            ledger.total_minted(),
            102 * MICRO,
            "loser emission reverted, winner counted — the revert is load-bearing"
        );

        // (d) global conservation holds after reconciliation.
        let spendable: u64 = ledger.all_balances().values().sum();
        assert_eq!(
            spendable + ledger.locked_stake_total() + ledger.total_burned(),
            ledger.total_minted(),
            "Σ(spendable+staked+unbonding)+burned == minted after reconciliation"
        );
    }

    /// **GADGET-5B §4 tooth — finality preserved (the floor is absolute).** A fork
    /// that would replace a **finalized** block can never win the reconciliation:
    /// `reorg_to_fork` refuses to disturb any block at or below `floor_index`. The
    /// positive control proves the floor blocks exactly — and only — what finality
    /// protects (a fork above the floor still reorganizes).
    #[test]
    fn gadget5b_reorg_to_fork_refuses_below_finalized_floor() {
        let mut ledger = Ledger::new();
        let miner = "f".repeat(64);
        ledger.mine_tx(&miner, 2 * MICRO, 0.0);
        let b1 = ledger.seal_block(&miner, 0.0);
        ledger.mine_tx(&miner, 2 * MICRO, 0.0);
        let b2 = ledger.seal_block(&miner, 0.0);
        assert_eq!(ledger.chain.len(), 3, "setup: G, b1, b2");

        // A valid competing block at index 2, rooted at b1 — it would REPLACE b2.
        let rival = "e".repeat(64);
        // REWARD-SHARE-1 : le bloc rival respecte le plan de partage — ce test
        // éprouve le PLANCHER de finalité, pas la répartition.
        let rewards = ledger.planned_reward_txs(2, &rival, 2 * MICRO);
        let winner = Ledger::forge_block_at(2, &b1.hash, "fork-ts", &rival, rewards);

        // Floor at index 2 (b2 finalized): the fork roots BELOW the floor (at b1=1),
        // so adopting it would undo finalized b2 → REFUSED, chain untouched.
        assert_eq!(
            ledger.reorg_to_fork(std::slice::from_ref(&winner), 2),
            Ok(false),
            "a fork that would replace a FINALIZED block is refused (5A floor, absolute)"
        );
        assert_eq!(ledger.chain.last().unwrap().hash, b2.hash, "finalized tip preserved");
        assert_eq!(ledger.chain.len(), 3, "chain unchanged by the refused reorg");

        // Positive control: floor at index 1 (only b1 finalized) ⇒ b2 is fair game,
        // so the SAME fork is adopted. The floor protects exactly what is finalized.
        assert_eq!(
            ledger.reorg_to_fork(std::slice::from_ref(&winner), 1),
            Ok(true),
            "a fork above the finalized floor reorganizes normally"
        );
        assert_eq!(ledger.chain.last().unwrap().hash, winner.hash, "the fork is now the tip");
    }

    // ─── LIVE-2: finality floor on the LIVE single-block-fork path ──────────

    #[test]
    fn live2_integrate_refuses_to_reorg_a_finalized_tip() {
        // The finality-safety property on the live path: once the tip is finalized
        // (floor at its index), a higher-hash competitor at the SAME height — which
        // would normally WIN the lexicographic tie-break — is REFUSED. Finalized
        // history is irreversible. This closes on `integrate_remote_block` the same
        // hole `reorg_to_fork` already guards on the multi-block path.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let rival = "e".repeat(64);
        let reward = ledger.build_unsigned_tx("NETWORK", &rival, 2 * MICRO, TxType::Mining);
        // A competitor engineered to BEAT the tip's hash (would win without finality).
        let winner =
            forge_winning_fork(tip.index, &genesis_hash, &rival, vec![reward], &tip.hash);
        assert!(winner.hash > tip.hash, "setup: the competitor wins the raw tie-break");

        // Finalize the tip (floor = its index + our matching hash). The higher-hash
        // fork must be refused.
        let floor = ledger.set_finalized_floor(tip.index, &tip.hash);
        assert_eq!(floor, tip.index, "floor advanced to the finalized tip");
        assert_eq!(
            ledger.integrate_remote_block(winner.clone()),
            Ok(false),
            "a fork replacing a FINALIZED tip is refused regardless of hash",
        );
        assert_eq!(ledger.chain.last().unwrap().hash, tip.hash, "finalized tip preserved");
        assert_eq!(ledger.chain.len(), 2, "chain unchanged by the refused reorg");
    }

    #[test]
    fn live2_integrate_still_reorgs_above_the_floor() {
        // Positive control (anti-masking): finality freezes ONLY what it finalized.
        // With the floor left at genesis (0), the SAME higher-hash competitor at
        // height 1 wins the tie-break and reorganizes normally — fork-choice stays
        // free above the floor (Gasper).
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let rival = "e".repeat(64);
        let reward = ledger.build_unsigned_tx("NETWORK", &rival, 2 * MICRO, TxType::Mining);
        let winner =
            forge_winning_fork(tip.index, &genesis_hash, &rival, vec![reward], &tip.hash);
        assert_eq!(ledger.finalized_floor_index(), 0, "only genesis finalized");
        assert_eq!(
            ledger.integrate_remote_block(winner.clone()),
            Ok(true),
            "above the floor the free lexicographic tie-break still applies",
        );
        assert_eq!(ledger.chain.last().unwrap().hash, winner.hash, "the heavier fork won");
    }

    #[test]
    fn live2_finalized_floor_is_monotonic_hash_checked_and_bounded() {
        // The setter never lowers the floor (finality is irreversible), never freezes
        // a height the node doesn't hold, and (HIGH-4) only freezes when OUR block at
        // that height matches the finalized hash.
        let (mut ledger, tip, _g) = forkcap_two_block_chain(); // tip at index 1
        // HIGH-4: a WRONG hash at the finalized height must NOT advance the floor —
        // the node is on a finality-overruled fork and must be able to sync `X` later.
        assert_eq!(
            ledger.set_finalized_floor(1, "not-our-block-hash"),
            0,
            "a mismatched finalized hash does NOT freeze our (different) block",
        );
        // Matching hash → advance.
        assert_eq!(ledger.set_finalized_floor(1, &tip.hash), 1, "advances to a held+matching height");
        assert_eq!(ledger.set_finalized_floor(0, "GENESIS"), 1, "never moves backward");
        assert_eq!(
            ledger.set_finalized_floor(999, "whatever"),
            tip.index,
            "a not-yet-held height can't freeze (we don't have that block)",
        );
    }

    #[test]
    fn live2_floor_survives_snapshot_restore() {
        // The floor is persisted (it is NOT chain-derivable — it depends on votes),
        // so a restart keeps finalized history irreversible before votes re-flow.
        let (mut ledger, tip, _g) = forkcap_two_block_chain();
        ledger.set_finalized_floor(tip.index, &tip.hash);
        let restored = Ledger::restore(ledger.snapshot());
        assert_eq!(
            restored.finalized_floor_index(),
            tip.index,
            "the finality floor round-trips through snapshot/restore",
        );
    }

    // ─── SLICE-CLASS (HARDEN-HYGIENE-1): char-safe log truncation ───────────

    #[test]
    fn slice_short_is_char_safe() {
        assert_eq!(short("hello", 3), "hel"); // plain ASCII prefix
        assert_eq!(short("hello", 10), "hello"); // max >= len → whole string
        assert_eq!(short("", 5), "");
        // A multi-byte char straddling the cut index is DROPPED, not split
        // (byte-indexing there panics). "héllo": é occupies bytes 1..3.
        assert_eq!(short("héllo", 2), "h");
        assert_eq!(short("héllo", 1), "h");
        assert_eq!(short("é", 1), ""); // cut inside the only char → empty, no panic
        assert_eq!(short("ÿÿ", 99), "ÿÿ");
        // The exact shape that panicked the hard slice: 13 bytes, cut at 12 lands
        // mid-'é'. Must not panic.
        let s = format!("g{}é", "f".repeat(10));
        assert_eq!(short(&s, 12), &format!("g{}", "f".repeat(10)));
    }

    #[test]
    fn slice_multibyte_block_hash_does_not_panic_in_fork_log() {
        // A malicious peer sends a fork-height block whose `hash` is a short,
        // multi-byte string lexicographically ABOVE the tip — reaching the
        // fork-resolution log that used a HARD `&block.hash[..12]`, which
        // panicked the single gossip-dispatch task (remote DoS). With `short` it
        // logs safely and the block is REJECTED (hash mismatch), never panics.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        // 13 bytes: 'g' + ten 'f' + 'é'; byte 12 lands inside 'é'. 'g' (0x67) >
        // any hex digit, so block.hash > tip.hash → enters the reorg `>` arm.
        let evil_hash = format!("g{}é", "f".repeat(10));
        assert!(
            evil_hash.as_str() > tip.hash.as_str(),
            "must win the tie-break to reach the log slice"
        );
        let evil = Block {
            index: tip.index,
            timestamp: "t".into(),
            transactions: vec![],
            prev_hash: genesis_hash,
            hash: evil_hash,
            miner: "z".repeat(64),
            energy_kwh: 0.0,
        };
        let res = ledger.integrate_remote_block(evil); // pre-fix: PANIC at [..12]
        assert!(res.is_err(), "bogus-hash fork block must be rejected, not panic");
        assert_eq!(ledger.chain.len(), 2, "chain unchanged");
    }

    // ─── TX-AUTH-NONCE-1: nonce/hash authentication + hang bound ────────────

    /// Build a tx signed over the canonical pre-image with an ATTACKER-CHOSEN
    /// nonce (hash recomputed to match), the way a hostile peer crafts one.
    /// Mirrors `build_signed_tx_at`'s encoding so a legit nonce verifies and a
    /// tampered field is caught.
    fn sign_tx_with_nonce(
        crypto: &CryptoEngine,
        from: &str,
        to: &str,
        amount: u64,
        nonce: u64,
    ) -> Transaction {
        let id = format!("tx_evil_{nonce}");
        let ts = "2026-01-01T00:00:00+00:00".to_string();
        let tx_type = TxType::Transfer;
        // PQ-MIG-3: bind the primary ML-DSA key into the pre-image, then sign both
        // layers with it — mirrors `build_signed_tx_at`.
        let pq_pk = crypto.pq_identity_hex().expect("ml-dsa primary");
        let payload =
            Ledger::tx_signing_preimage(&id, from, to, amount, &ts, &tx_type, nonce, &pq_pk);
        let (classical, quantum, pq_pk) = crypto.sign_tx_authority(payload.as_bytes()).unwrap();
        Transaction {
            id,
            from: from.into(),
            to: to.into(),
            amount,
            tx_type,
            timestamp: ts,
            signature: hex::encode(&classical),
            hash: hex::encode(blake3::hash(payload.as_bytes()).as_bytes()),
            nonce,
            pq_signature: Some(hex::encode(&quantum)),
            pq_public_key: Some(pq_pk),
            fault_proof: None,
            slash_unbonding: None,
        }
    }

    #[test]
    fn txauth_far_ahead_nonce_rejected_no_hang() {
        // §5 anti-hang: a validly-SIGNED tx with a nonce near u64::MAX (far beyond
        // MAX_NONCE_GAP) must be REJECTED in O(1) — never the old ~2^64 increment
        // loop under the ledger lock. (The test completing at all = no hang.)
        let mut crypto = pq_wallet();
        let pk = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block(&pk, 0.0);
        let to = "d".repeat(64);

        let evil = sign_tx_with_nonce(&crypto, &pk, &to, 5 * MICRO, u64::MAX);
        assert!(
            Ledger::verify_tx(&evil).unwrap(),
            "the crafted tx is correctly signed — the GAP BOUND rejects it, not the signature"
        );
        let before_hw = ledger.get_nonce(&pk);
        let applied = ledger.apply_remote_tx_checked(evil);
        assert!(!applied, "a nonce far ahead of the high-water must be rejected (gap bound)");
        assert_eq!(
            ledger.get_nonce(&pk),
            before_hw,
            "the rejected tx must not advance/poison the nonce high-water"
        );
    }

    #[test]
    fn txauth_forged_nonce_breaks_signature() {
        // §5: altering a signed tx's nonce (with the hash recomputed to bypass §3)
        // must invalidate the SIGNATURE — the nonce is now in the signed pre-image.
        let mut crypto = pq_wallet();
        let pk = gen_addr(&mut crypto);
        let to = "d".repeat(64);
        let tx = sign_tx_with_nonce(&crypto, &pk, &to, 10 * MICRO, 7);
        assert!(Ledger::verify_tx(&tx).unwrap(), "the untouched signed tx verifies");

        let mut forged = tx.clone();
        forged.nonce = 8;
        let payload = Ledger::tx_signing_preimage(
            &forged.id, &forged.from, &forged.to, forged.amount, &forged.timestamp,
            &forged.tx_type, forged.nonce, forged.pq_public_key.as_deref().unwrap_or(""),
        );
        forged.hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        assert!(
            !Ledger::verify_tx(&forged).unwrap(),
            "altering the signed nonce invalidates the signature (§2)"
        );
    }

    #[test]
    fn txauth_malleable_hash_rejected() {
        // §5: a wire hash that disagrees with the recomputed content hash is
        // rejected (§3 — no hash malleability past the dedup).
        let mut crypto = pq_wallet();
        let pk = gen_addr(&mut crypto);
        let to = "d".repeat(64);
        let tx = sign_tx_with_nonce(&crypto, &pk, &to, 10 * MICRO, 3);
        let mut forged = tx.clone();
        forged.hash = "0".repeat(64);
        assert!(
            !Ledger::verify_tx(&forged).unwrap(),
            "a wire hash != the recomputed content hash must be rejected (§3)"
        );
    }

    #[test]
    fn txauth_valid_signed_tx_verifies_and_hash_binds_nonce() {
        // §5 happy path: the real production path (transfer_tx) still verifies, and
        // tx.hash now commits to the nonce. C1 byte-identical determinism is
        // covered by determinism_meta_test_128_runs_are_byte_identical (green).
        let mut crypto = pq_wallet();
        let pk = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block(&pk, 0.0);
        let to = "d".repeat(64);
        let tx = ledger.transfer_tx(&pk, &to, 10 * MICRO, &crypto).unwrap();
        assert!(Ledger::verify_tx(&tx).unwrap(), "a freshly-signed tx verifies (nonce in the pre-image)");
        let payload = Ledger::tx_signing_preimage(
            &tx.id, &tx.from, &tx.to, tx.amount, &tx.timestamp, &tx.tx_type, tx.nonce,
            tx.pq_public_key.as_deref().unwrap_or(""),
        );
        assert_eq!(
            tx.hash,
            hex::encode(blake3::hash(payload.as_bytes()).as_bytes()),
            "tx.hash commits to the nonce"
        );
    }

    #[test]
    fn transfer_with_burn_deducts_one_percent() {
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        // Give sender 100 QUANTA
        ledger.mine_tx(&id, 100 * MICRO, 0.0);

        let to = "d".repeat(64);
        let (tx, burn_tx, burn_uqta) = ledger
            .transfer_with_burn(&id, &to, 10 * MICRO, &crypto)
            .expect("transfer should succeed");
        // AUDIT-TX-1: burn leg must be present and signed for any burn > 0
        assert!(
            burn_tx.is_some(),
            "burn tx must be returned for non-zero burn"
        );
        let bt = burn_tx.as_ref().unwrap();
        assert_eq!(bt.to, "BURN");
        assert!(!bt.signature.is_empty(), "burn tx must be signed");
        assert!(
            Ledger::verify_tx(bt).unwrap(),
            "burn tx signature must verify"
        );

        // 1% of 10 QUANTA = 0.1 QUANTA = 100_000 µQTA
        assert_eq!(burn_uqta, 100_000, "burn should be exactly 1%");
        // tx.amount stores the NET amount (after burn deduction)
        assert_eq!(tx.amount, 9_900_000, "tx amount = net after 1% burn");

        // Sender: 100 - 10 = 90 QUANTA
        assert_eq!(ledger.balance_of(&id), 90 * MICRO);
        // Receiver: 10 - 0.1 = 9.9 QUANTA = 9_900_000 µQTA
        assert_eq!(ledger.balance_of(&to), 9_900_000);
    }

    #[test]
    fn double_spend_rejected() {
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id, 5 * MICRO, 0.0);

        let to = "e".repeat(64);
        // First transfer: 5 QTA → should succeed
        assert!(ledger
            .transfer_with_burn(&id, &to, 5 * MICRO, &crypto)
            .is_ok());
        // Second transfer: 5 QTA → should fail (balance is 0 now)
        assert!(ledger
            .transfer_with_burn(&id, &to, 5 * MICRO, &crypto)
            .is_err());
    }

    #[test]
    fn balance_never_negative() {
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id, MICRO, 0.0);

        let to = "f".repeat(64);
        // Try to send more than balance
        assert!(ledger
            .transfer_with_burn(&id, &to, 2 * MICRO, &crypto)
            .is_err());
        // Balance unchanged
        assert_eq!(ledger.balance_of(&id), MICRO);
    }

    #[test]
    fn snapshot_restore_preserves_state() {
        let mut ledger = Ledger::new();
        let pk = "a".repeat(64);
        ledger.mine_tx(&pk, 50 * MICRO, 1.5);
        ledger.mine_tx(&pk, 30 * MICRO, 0.8);

        let snap = ledger.snapshot();
        let restored = Ledger::restore(snap);

        assert_eq!(restored.balance_of(&pk), ledger.balance_of(&pk));
        assert_eq!(restored.stats().total_txs, ledger.stats().total_txs);
        assert_eq!(restored.stats().total_mined, ledger.stats().total_mined);
    }

    #[test]
    fn chain_verification_catches_tamper() {
        let mut ledger = Ledger::new();
        let pk = "g".repeat(64);
        // Mine enough to seal a block
        for _ in 0..12 {
            ledger.mine_tx(&pk, MICRO, 0.1);
        }
        // Verify passes on clean chain
        assert!(ledger.verify_chain().is_ok());
    }

    #[test]
    fn total_supply_tracks_balances() {
        let mut ledger = Ledger::new();
        let pk1 = "h".repeat(64);
        let pk2 = "i".repeat(64);
        ledger.mine_tx(&pk1, 10 * MICRO, 0.5);
        ledger.mine_tx(&pk2, 20 * MICRO, 1.0);

        // Balances should equal the sum of mining amounts
        assert_eq!(
            ledger.balance_of(&pk1) + ledger.balance_of(&pk2),
            30 * MICRO,
            "sum of balances = total mined (no burns yet)"
        );
        // Each user got their exact amount
        assert_eq!(ledger.balance_of(&pk1), 10 * MICRO);
        assert_eq!(ledger.balance_of(&pk2), 20 * MICRO);
    }

    // ─── AUDIT-TX regression tests ─────────────────────────────────────

    /// AUDIT-TX-1: a tx with `to == "BURN"` but empty signature must be
    /// rejected by verify_tx. Previously `verify_tx` short-circuited on
    /// `to == "BURN"`, allowing any peer to forge a victim's burn over gossip.
    #[test]
    fn audit_tx1_unsigned_burn_target_is_rejected() {
        let pk = "a".repeat(64);
        // Manually craft an unsigned burn tx — exactly what an attacker
        // would have submitted under the previous bug.
        let forged = Transaction {
            id: "fake_burn".into(),
            from: pk.clone(),
            to: "BURN".into(),
            amount: 1_000_000,
            tx_type: TxType::Burn,
            timestamp: chrono::Utc::now().to_rfc3339(),
            signature: String::new(), // <-- the smoking gun: no sig
            hash: "deadbeef".into(),
            nonce: 0,
            pq_signature: None,
            pq_public_key: None,
            fault_proof: None,
            slash_unbonding: None,
        };
        assert!(
            Ledger::verify_tx(&forged).is_err(),
            "verify_tx must reject unsigned burn-target tx (AUDIT-TX-1)"
        );
    }

    /// AUDIT-TX-1 (positive): burns from `transfer_with_burn` are signed and
    /// verifiable.
    #[test]
    fn audit_tx1_burn_leg_is_signed_and_verifies() {
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id, 100 * MICRO, 0.0);

        let to = "d".repeat(64);
        let (transfer, burn_opt, burn_uqta) = ledger
            .transfer_with_burn(&id, &to, 10 * MICRO, &crypto)
            .expect("transfer must succeed");

        assert_eq!(burn_uqta, 100_000);
        let burn = burn_opt.expect("burn leg must be present");
        assert_eq!(burn.to, "BURN");
        assert!(!burn.signature.is_empty(), "burn must carry a signature");
        assert!(
            Ledger::verify_tx(&burn).expect("verify must not error"),
            "burn signature must verify"
        );
        // Both legs are different txs (different hashes).
        assert_ne!(transfer.hash, burn.hash);
    }

    /// AUDIT-TX-3 (regression): transfer_with_burn rejects amounts where
    /// `balance < gross` (the sender can cover net but not the 1% burn).
    /// Previously the net-only check let the burn debit silently push the
    /// cache below zero (saturated to 0 by `balance_of`, hiding the bug).
    #[test]
    fn audit_tx3_gross_balance_check_blocks_overdraw() {
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        // Sender holds 99_000 µQTA (= 0.099 QTA, just under the 0.1 QTA min).
        // This is the boundary case: balance == net (99_000 = 100_000 - 1_000)
        // but balance < gross (99_000 < 100_000). Old code would accept the
        // transfer (net check passes) then apply the unsigned burn,
        // overdrawing the sender by 1_000 µQTA.
        ledger.mine_tx(&id, 99_000, 0.0);

        let to = "e".repeat(64);
        let result = ledger.transfer_with_burn(&id, &to, 100_000, &crypto);
        assert!(
            result.is_err(),
            "transfer where balance < gross must be rejected (AUDIT-TX-3)"
        );
        // Balance untouched.
        assert_eq!(ledger.balance_of(&id), 99_000);
    }

    /// AUDIT-TX cross-ledger convergence: two independent ledgers receiving
    /// the same transfer + burn pair must end up with IDENTICAL balances.
    /// This exercises the same flow as A→B over gossip.
    #[test]
    fn audit_tx_cross_ledger_convergence() {
        let mut crypto = pq_wallet();
        let id_a = gen_addr(&mut crypto);
        let pk_a = id_a.clone();
        let pk_b = "b".repeat(64);

        // Local ledger A: gives A 100 QTA, sends 10 to B with 1% burn.
        let mut node_a = Ledger::new();
        node_a.mine_tx(&pk_a, 100 * MICRO, 0.0);
        let (transfer, burn_opt, _) = node_a
            .transfer_with_burn(&pk_a, &pk_b, 10 * MICRO, &crypto)
            .expect("transfer must succeed");
        let burn = burn_opt.expect("burn must exist");

        // Local ledger B: also has the original mining (would reach via chain sync).
        let mut node_b = Ledger::new();
        node_b.mine_tx(&pk_a, 100 * MICRO, 0.0);

        // B receives BOTH legs over gossip (order-independent).
        assert!(node_b.replay_remote_tx(burn.clone()), "B must apply burn");
        assert!(
            node_b.replay_remote_tx(transfer.clone()),
            "B must apply transfer"
        );

        // Both ledgers MUST show identical balances.
        assert_eq!(
            node_a.balance_of(&pk_a),
            node_b.balance_of(&pk_a),
            "sender balance must converge"
        );
        assert_eq!(
            node_a.balance_of(&pk_b),
            node_b.balance_of(&pk_b),
            "receiver balance must converge"
        );
        // Concrete values: A had 100 QTA, sent 10 (gross), 9.9 to B, 0.1 burned.
        assert_eq!(node_a.balance_of(&pk_a), 90 * MICRO);
        assert_eq!(node_a.balance_of(&pk_b), 9_900_000);
    }

    // ─── AUDIT-BLK regression tests ────────────────────────────────────

    /// AUDIT-BLK-1 ∩ EMIT-1 §4.1: a fork reorg must NOT silently drop the
    /// loser's exclusive **user** transactions — they re-enter `pending` so the
    /// next leader can include them (AUDIT-BLK-1). But the loser's **synthetic
    /// mining reward** must be EXCLUDED from the re-queue (EMIT-1 §4.1):
    /// re-queuing it would let it be sealed again — a double-mint.
    #[test]
    fn audit_blk1_fork_reorg_preserves_exclusive_txs() {
        // A real signer so the loser can carry an exclusive USER transfer.
        let mut crypto = pq_wallet();
        let signer = gen_addr(&mut crypto);
        let pk_x = "e".repeat(64);

        // Loser fork: the signer is funded and makes a transfer the winner never
        // sees — a loser-EXCLUSIVE user tx (+ its 1% burn). One reward, to the
        // block's miner.
        let mut loser = Ledger::new();
        loser.mine_tx(&signer, 100 * MICRO, 0.0);
        let _ = loser
            .transfer_with_burn(&signer, &pk_x, 10 * MICRO, &crypto)
            .expect("loser transfer builds");
        let loser_tip = loser.seal_block(&signer, 0.0);

        // Winning fork (higher hash so it triggers the reorg): a different reward
        // amount each try until its hash beats the loser's — sealed by its own
        // miner (EMIT-1 §4.2: the reward credits the block's miner).
        let winner_tip = {
            // MINT-EXACT-1 : l'émission ne peut plus servir de « nonce » pour
            // chercher un hash supérieur — elle est bornée par la récompense
            // canonique du bloc. On fait donc varier l'horodatage, qui entre
            // dans la pré-image du hash sans toucher à la monnaie.
            let mut n = 0u32;
            loop {
                let mut w = Ledger::new();
                w.mine_tx(&signer, 3 * MICRO, 0.0);
                let ts = format!("2026-06-25T00:00:00.{n:06}+00:00");
                let tip = w.seal_block_at(&signer, 0.0, ts);
                if tip.hash > loser_tip.hash {
                    break tip;
                }
                n += 1;
            }
        };

        // The loser node holds its own tip, then receives the higher-hash winner.
        let mut node = Ledger::new();
        node.chain.push(loser_tip.clone());
        let res = node.integrate_remote_block(winner_tip.clone());
        assert!(res.is_ok(), "fork integration must succeed: {:?}", res);
        assert_eq!(node.chain.last().unwrap().hash, winner_tip.hash);

        // Every loser-only USER tx is re-queued (AUDIT-BLK-1); the loser's
        // synthetic mining reward is NOT (EMIT-1 §4.1).
        let winner_hashes: std::collections::HashSet<&String> =
            winner_tip.transactions.iter().map(|t| &t.hash).collect();
        let mut saw_user = false;
        let mut saw_mining = false;
        for tx in loser_tip
            .transactions
            .iter()
            .filter(|t| !winner_hashes.contains(&t.hash))
        {
            let requeued = node.pending.iter().any(|p| p.hash == tx.hash);
            if tx.tx_type == TxType::Mining {
                saw_mining = true;
                assert!(
                    !requeued,
                    "the loser's mining reward must NOT be re-queued (EMIT-1 §4.1)"
                );
            } else {
                saw_user = true;
                assert!(
                    requeued,
                    "loser-only user tx {} must be re-queued (AUDIT-BLK-1)",
                    tx.id
                );
            }
        }
        assert!(saw_user, "the loser carried an exclusive user tx to re-queue");
        assert!(
            saw_mining,
            "the loser carried an exclusive mining reward to exclude"
        );
    }

    /// AUDIT-BLK-2: if the remote fork block is malformed, the local chain
    /// must NOT be truncated. Previously we popped our tip BEFORE
    /// validating, so a corrupt block left a permanent gap.
    #[test]
    fn audit_blk2_failed_fork_validation_preserves_chain() {
        let pk_a = "a".repeat(64);

        let mut node = Ledger::new();
        node.mine_tx(&pk_a, 100 * MICRO, 0.0);
        let our_tip = node.seal_block(&pk_a, 0.0);
        let height_before = node.chain.len();

        // Construct a fork block at the same height with a HIGHER hash
        // (so the tie-break would prefer it) but a CORRUPT hash field
        // (mismatched payload → validate_block_against_prev rejects).
        let bogus = Block {
            index: our_tip.index,
            timestamp: our_tip.timestamp.clone(),
            transactions: vec![],
            prev_hash: our_tip.prev_hash.clone(),
            // Force a hash that's > our_tip.hash by setting the high byte to 'f'.
            hash: "f".repeat(64),
            miner: "attacker".into(),
            energy_kwh: 0.0,
        };

        let res = node.integrate_remote_block(bogus);
        assert!(res.is_err(), "corrupt fork block must be rejected");
        // Chain unchanged: our tip is still there.
        assert_eq!(node.chain.len(), height_before);
        assert_eq!(node.chain.last().unwrap().hash, our_tip.hash);
    }

    /// AUDIT-TX out-of-order delivery: B receives the burn (tx_type=Burn)
    /// BEFORE the transfer. With the relaxed nonce policy, BOTH must be
    /// applied — the previous strict equality dropped the second arrival.
    #[test]
    fn audit_tx_out_of_order_replay_both_apply() {
        let mut crypto = pq_wallet();
        let id_a = gen_addr(&mut crypto);
        let pk_a = id_a.clone();
        let pk_b = "b".repeat(64);

        let mut node_a = Ledger::new();
        node_a.mine_tx(&pk_a, 100 * MICRO, 0.0);
        let (transfer, burn_opt, _) = node_a
            .transfer_with_burn(&pk_a, &pk_b, 10 * MICRO, &crypto)
            .expect("transfer must succeed");
        let burn = burn_opt.unwrap();

        // B applies burn FIRST, then transfer (reverse arrival order).
        let mut node_b = Ledger::new();
        node_b.mine_tx(&pk_a, 100 * MICRO, 0.0);
        assert!(node_b.replay_remote_tx(burn));
        assert!(node_b.replay_remote_tx(transfer));

        // Both legs landed; balances match the in-order case exactly.
        assert_eq!(node_b.balance_of(&pk_a), 90 * MICRO);
        assert_eq!(node_b.balance_of(&pk_b), 9_900_000);
    }

    // ─── BLK-HASH-1: block hash commits content + miner ──────────────────

    /// **T1 — collision closed.** Two blocks with the SAME
    /// `(index, prev_hash, timestamp, tx_count)` but a DIFFERENT mining
    /// recipient (different content) must now get DIFFERENT hashes. This is
    /// the exact tuple that collided before the fix.
    #[test]
    fn blk_hash_1_content_collision_is_closed() {
        let ts = "2026-03-01T00:00:00+00:00".to_string();
        let seal = |miner: &str| {
            let mut o = Ledger::new();
            o.mine_tx(miner, 50 * MICRO, 0.0);
            o.seal_block_at(miner, 0.0, ts.clone())
        };
        let bx = seal(&"a".repeat(64));
        let by = seal(&"b".repeat(64));
        assert_eq!(bx.index, by.index);
        assert_eq!(bx.timestamp, by.timestamp);
        assert_eq!(bx.transactions.len(), by.transactions.len());
        assert_eq!(bx.prev_hash, by.prev_hash, "same shared genesis prev");
        assert_ne!(
            bx.hash, by.hash,
            "different content ⇒ different block hash (collision closed)"
        );
    }

    /// **T3 — tampering a contained tx changes the block hash.** Tampering a
    /// MINING tx's amount (no signature to break — `NETWORK` is sig-exempt)
    /// must now invalidate the block via the content-binding Merkle ⇒
    /// rejected.
    #[test]
    fn blk_hash_1_tampering_contained_tx_is_rejected() {
        let miner = "a".repeat(64);
        let mut origin = Ledger::new();
        origin.mine_tx(&miner, 50 * MICRO, 0.0);
        let mut block = origin.seal_block(&miner, 0.0);
        if let Some(tx) = block
            .transactions
            .iter_mut()
            .find(|t| t.tx_type == TxType::Mining)
        {
            tx.amount += 1; // content change, hash left stale
        }
        let mut node = Ledger::new();
        assert!(
            node.integrate_remote_block(block).is_err(),
            "content tamper ⇒ block hash mismatch ⇒ rejected"
        );
        assert_eq!(node.chain_height(), 1, "chain unchanged");
    }

    // ─── ONCHAIN-STAKE-1: on-chain stake state ──────────────────────────────

    /// The conservation LHS the harness checks (§5): spendable + locked-stake +
    /// burned. It must equal `total_minted` at **every** step of a stake lifecycle
    /// — staking moves coins into the `STAKE` sink, it never destroys them.
    fn conservation_lhs(l: &Ledger) -> u64 {
        let spendable: u64 = l.all_balances().values().sum();
        spendable + l.locked_stake_total() + l.total_burned()
    }

    /// **ONCHAIN-STAKE-1 §5 — conservation through Stake → Unstake → unlock.**
    /// Each phase moves coins between the spendable, staked, and unbonding pools;
    /// conservation (`Σ = minted`) holds at every step, and the funds become
    /// spendable again only once the unlock height is reached. HARDEN-STAKE-1: a
    /// Stake **locks** spendable funds the instant it is admitted (mempool), while
    /// its consensus **weight** is committed only on seal.
    #[test]
    fn onchain_stake_conservation_through_stake_unstake_unlock() {
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let pk = id.clone();

        let mut l = Ledger::new();
        // Fund 100 QUANTA on-chain.
        l.mine_tx(&pk, 100 * MICRO, 0.0);
        l.seal_block(&pk, 0.0);
        let minted = l.total_minted();
        assert_eq!(l.balance_of(&pk), 100 * MICRO, "funded spendable");
        assert_eq!(conservation_lhs(&l), minted, "conservation after funding");

        // ── Stake 40: spendable LOCKS immediately (100 → 60) at mempool, but the
        //    consensus bonded weight is only committed on seal.
        l.stake_tx(&pk, 40 * MICRO, &crypto).expect("stake builds");
        assert_eq!(l.balance_of(&pk), 60 * MICRO, "stake LOCKS spendable at mempool");
        assert_eq!(l.staked_of(&pk), 0, "but bonded weight not committed until sealed");
        assert_eq!(l.locked_stake_total(), 40 * MICRO, "40 locked in the STAKE sink");
        assert_eq!(conservation_lhs(&l), minted, "conservation while pending stake");
        l.seal_block(&pk, 0.0);
        assert_eq!(l.balance_of(&pk), 60 * MICRO, "spendable stays locked");
        assert_eq!(l.staked_of(&pk), 40 * MICRO, "40 now bonded (consensus weight)");
        assert_eq!(conservation_lhs(&l), minted, "conservation after stake");

        // ── Unstake 25: bonded 40 → 15, unbonding 0 → 25 (still locked).
        l.unstake_tx(&pk, 25 * MICRO, &crypto).expect("unstake builds");
        l.seal_block(&pk, 0.0);
        let unbond_block = l.chain_height() - 1; // index that sealed the unstake
        let unlock = unbond_block + UNBONDING_PERIOD_BLOCKS;
        assert_eq!(l.staked_of(&pk), 15 * MICRO, "15 still bonded");
        assert_eq!(l.unbonding_of(&pk), 25 * MICRO, "25 unbonding");
        assert_eq!(
            l.balance_of(&pk),
            60 * MICRO,
            "unbonding funds are NOT spendable yet"
        );
        assert_eq!(conservation_lhs(&l), minted, "conservation after unstake");

        // ── Just before the unlock height: still locked (height-indexed, §3).
        l.mature_unbonding(unlock - 1);
        assert_eq!(
            l.unbonding_of(&pk),
            25 * MICRO,
            "must not unlock before the unbonding period elapses"
        );
        assert_eq!(l.balance_of(&pk), 60 * MICRO, "still locked one block early");
        assert_eq!(conservation_lhs(&l), minted, "conservation while locked");

        // ── At the unlock height (production reaches this via the per-seal
        //    `apply_block_stake_effects(block.index)`): funds return to spendable.
        l.mature_unbonding(unlock);
        assert_eq!(l.unbonding_of(&pk), 0, "unbonding fully matured");
        assert_eq!(l.staked_of(&pk), 15 * MICRO, "still-bonded stake untouched");
        assert_eq!(
            l.balance_of(&pk),
            85 * MICRO,
            "60 spendable + 25 matured = 85"
        );
        assert_eq!(conservation_lhs(&l), minted, "conservation after unlock");
    }

    /// **ONCHAIN-STAKE-1 §7 — anti-divergence (the seal), with teeth.** Two nodes
    /// holding the **same chain** derive the **same** validator weights and elect
    /// the **same** leader at every slot — even though their *local* leaderboards
    /// differ. Non-vacuity: the OLD source (a node-local leaderboard) would have
    /// elected different leaders on those very inputs, so the agreement comes from
    /// the change of source (chain, not leaderboard), not from identical inputs.
    #[test]
    fn onchain_stake_weight_identical_across_nodes_despite_local_leaderboards() {
        use crate::p2p::pos_consensus::{build_validator_set, elect_leader};

        // Build a chain where three accounts bond different amounts.
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);
        let mut crypto_c = pq_wallet();
        let carol = gen_addr(&mut crypto_c);

        let mut node1 = Ledger::new();
        for (pk, c, fund, stake) in [
            (&alice, &crypto_a, 10 * MICRO, 5 * MICRO),
            (&bob, &crypto_b, 10 * MICRO, 3 * MICRO),
            (&carol, &crypto_c, 10 * MICRO, 4 * MICRO),
        ] {
            node1.mine_tx(pk, fund, 0.0);
            node1.seal_block(pk, 0.0);
            node1.stake_tx(pk, stake, c).expect("stake builds");
            node1.seal_block(pk, 0.0);
        }

        // node2 holds the SAME chain (restored from node1's snapshot) — its stake
        // state is rebuilt deterministically from the chain.
        let node2 = Ledger::restore(node1.snapshot());

        // On-chain stake snapshots coincide exactly.
        assert_eq!(
            node1.validator_stakes(),
            node2.validator_stakes(),
            "same chain ⇒ identical on-chain stake snapshot"
        );

        // Validator sets built from the chain (reputation OFF the path: empty map).
        let no_rep = std::collections::HashMap::new();
        let vals1 = build_validator_set(&node1.validator_stakes(), &no_rep);
        let vals2 = build_validator_set(&node2.validator_stakes(), &no_rep);

        // The two nodes elect the SAME leader at every slot.
        for slot in 0..2_000u64 {
            assert_eq!(
                elect_leader("beacon", slot, &vals1),
                elect_leader("beacon", slot, &vals2),
                "chain-sourced weight must elect the same leader on both nodes (slot {slot})"
            );
        }

        // TEETH — the OLD source (node-local leaderboards) would have diverged.
        // Simulate two nodes whose *local* views disagree (the exact hazard
        // ONCHAIN-STAKE-1 closes): node A sees alice richest, node B sees bob
        // richest. Build validator sets from those (the pre-fix path) and show
        // they elect different leaders on at least one slot.
        let local_a = std::collections::HashMap::from([
            (alice.clone(), 9 * MICRO),
            (bob.clone(), MICRO),
            (carol.clone(), MICRO),
        ]);
        let local_b = std::collections::HashMap::from([
            (alice.clone(), MICRO),
            (bob.clone(), 9 * MICRO),
            (carol.clone(), MICRO),
        ]);
        let old_a = build_validator_set(&local_a, &no_rep);
        let old_b = build_validator_set(&local_b, &no_rep);
        let old_would_diverge = (0..2_000u64)
            .any(|slot| elect_leader("beacon", slot, &old_a) != elect_leader("beacon", slot, &old_b));
        assert!(
            old_would_diverge,
            "non-vacuity: locally-sourced stake WOULD have forked the leader — \
             the new agreement is due to the on-chain source, not identical inputs"
        );
    }

    /// **ONCHAIN-STAKE-1 — live ≡ restored stake state.** A node that sealed a
    /// full Stake → Unstake → maturation history reaches **byte-identical** stake
    /// state (bonded map, unbonding, spendable cache, validator snapshot) after a
    /// snapshot → restore. This is the determinism property a synced/restarted node
    /// needs: the stake state is reconstructed purely from the chain, with the same
    /// block-index-anchored unlock heights and matured-fund credits — the live seal
    /// path and the `rebuild_cache` replay path agree.
    #[test]
    fn onchain_stake_state_survives_snapshot_restore_byte_identical() {
        let mut crypto = pq_wallet();
        let pk = gen_addr(&mut crypto);

        let mut live = Ledger::new();
        live.mine_tx(&pk, 100 * MICRO, 0.0);
        live.seal_block(&pk, 0.0);
        live.stake_tx(&pk, 60 * MICRO, &crypto).expect("stake");
        live.seal_block(&pk, 0.0);
        live.unstake_tx(&pk, 20 * MICRO, &crypto).expect("unstake");
        live.seal_block(&pk, 0.0);

        // A comparable fingerprint of the full stake-relevant state.
        let snap_of = |l: &Ledger| {
            let mut bals: Vec<(String, i128)> = l.balance_cache.iter().map(|(k, v)| (k.clone(), *v)).collect();
            bals.sort();
            let mut stk: Vec<(String, u64)> = l.staked.iter().map(|(k, v)| (k.clone(), *v)).collect();
            stk.sort();
            (
                bals,
                stk,
                l.staked_total(),
                l.unbonding_total(),
                l.unbonding_of(&pk),
                l.balance_of(&pk),
            )
        };

        let restored = Ledger::restore(live.snapshot());
        assert_eq!(
            snap_of(&live),
            snap_of(&restored),
            "restored node must rebuild byte-identical stake state from the chain"
        );

        // And maturing both to the same height keeps them identical (the credit
        // fold replays deterministically too).
        let target = restored.chain_height() - 1 + UNBONDING_PERIOD_BLOCKS;
        let mut live2 = live;
        let mut restored2 = restored;
        live2.mature_unbonding(target);
        restored2.mature_unbonding(target);
        assert_eq!(
            snap_of(&live2),
            snap_of(&restored2),
            "maturation folds identically on both"
        );
        assert_eq!(live2.balance_of(&pk), 60 * MICRO, "40 bonded + 20 matured returns 60 spendable");
    }

    /// **HARDEN-STAKE-1 (regression) — a pending Stake LOCKS funds; no double-spend,
    /// no conservation break.** The adversarial review found that, before the fix,
    /// a pending Stake did not reduce `balance_of`, so a concurrent transfer could
    /// spend the same coins → on seal the balance cache went negative, the `.max(0)`
    /// clamp hid it, and `Σ spendable + locked_stake + burned` exceeded `minted`
    /// (µQTA fabricated). Now the Stake debits the spendable balance into the
    /// `STAKE` sink at admission, so the racing transfer is rejected and
    /// conservation holds — at every step, including across a seal.
    #[test]
    fn harden_stake_pending_stake_locks_funds_no_double_spend() {
        let mut crypto = pq_wallet();
        let pk = gen_addr(&mut crypto);
        let bob = "b".repeat(64);

        let mut l = Ledger::new();
        l.mine_tx(&pk, 100 * MICRO, 0.0);
        l.seal_block(&pk, 0.0);
        let minted = l.total_minted();

        // Stake the FULL balance — funds lock immediately (mempool).
        l.stake_tx(&pk, 100 * MICRO, &crypto).expect("stake builds");
        assert_eq!(l.balance_of(&pk), 0, "the staked funds are locked NOW");

        // The racing transfer of the same coins must be REJECTED (the exact
        // double-spend the review reproduced as a conservation break).
        let racing = l.transfer_with_burn(&pk, &bob, 50 * MICRO, &crypto);
        assert!(
            racing.is_err(),
            "a transfer of already-staked funds must be rejected (no double-spend)"
        );

        // Seal the Stake; the balance cache never went negative, conservation holds.
        l.seal_block(&pk, 0.0);
        let spendable: u64 = l.all_balances().values().sum();
        assert_eq!(spendable, 0, "all funds locked in stake, none spendable");
        assert_eq!(l.locked_stake_total(), 100 * MICRO, "100 locked in the STAKE sink");
        assert_eq!(l.staked_of(&pk), 100 * MICRO, "100 bonded");
        assert_eq!(
            spendable + l.locked_stake_total() + l.total_burned(),
            minted,
            "conservation holds — no µQTA fabricated by the (now rejected) double-spend"
        );
        // The raw balance cache is non-negative everywhere (the clamp can hide
        // nothing): the sum of raw entries equals minted − burned exactly.
        let raw_sum: i128 = l.balance_cache.values().sum();
        assert_eq!(
            raw_sum,
            (minted - l.total_burned()) as i128,
            "raw cache conserves with no hidden negative deficit"
        );
        assert!(
            l.balance_cache.values().all(|v| *v >= 0),
            "no balance_cache entry is negative (the bug's signature is absent)"
        );
    }

    // ── COVER-1: block-level coverage validation (no spending what you don't have) ──

    /// Forge a block whose hash exceeds `threshold`, so `integrate_remote_block`
    /// takes the fork-reorg branch (remote wins on the lexicographically higher
    /// hash). Deterministic: only the block timestamp (part of the hash pre-image)
    /// is varied, over a fixed sequence, and the first hash that clears the bar is
    /// returned. The txs — hence the Merkle root — are untouched.
    #[cfg(test)]
    fn forge_block_hash_above(
        index: u64,
        prev_hash: &str,
        miner: &str,
        txs: &[Transaction],
        threshold: &str,
    ) -> Block {
        for i in 0..100_000u32 {
            let ts = format!("2026-02-01T00:00:00.{i:06}Z");
            let b = Ledger::forge_block_at(index, prev_hash, &ts, miner, txs.to_vec());
            if b.hash.as_str() > threshold {
                return b;
            }
        }
        panic!("could not forge a block with hash above the threshold");
    }

    /// **H8 (AUDIT-2026-07-25) — reverting a block must be the exact inverse of
    /// applying it.**
    ///
    /// `revert_block_stake_effects` iterated the block's transactions FORWARD, like
    /// the apply path. The per-tx operations do not commute (`saturating_sub` plus
    /// the collapse-to-zero that removes the key), so with a `Stake` and an
    /// `Unstake` from the same account in one block, revert-forward did not undo
    /// apply-forward: it left fabricated bonded stake behind — i.e. consensus
    /// weight — after any reorg over that block. Both reorg paths call it.
    #[test]
    fn h8_revert_block_stake_effects_is_the_exact_inverse() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        let stake = l
            .build_signed_tx_at(&alice, Ledger::STAKE_SINK, 5 * MICRO, TxType::Stake,
                &crypto_a, "2026-07-25T00:00:00+00:00".into(), false)
            .expect("stake tx");
        let unstake = l
            .build_signed_tx_at(&alice, Ledger::STAKE_SINK, 5 * MICRO, TxType::Unstake,
                &crypto_a, "2026-07-25T00:00:01+00:00".into(), false)
            .expect("unstake tx");
        let block = Ledger::forge_block_at(
            tip.index + 1,
            &tip.hash,
            "2026-07-25T00:00:02+00:00",
            &alice,
            vec![stake, unstake],
        );

        let bonded_before = l.validator_stakes();
        l.apply_block_stake_effects(&block);
        l.revert_block_stake_effects(&block);
        assert_eq!(
            l.validator_stakes(),
            bonded_before,
            "apply then revert must be the identity — otherwise a reorg mints consensus weight"
        );
        assert_eq!(l.staked_of(&alice), 0, "no bonded stake was fabricated");
    }

    /// **C3 (AUDIT-2026-07-25) — an `Unstake` beyond the bonded stake fabricates
    /// coins.**
    ///
    /// `uncovered_tx_indices` exempts `Unstake` ("no spendable debit to cover" —
    /// true), and nothing else ever checked `tx.amount <= bonded[tx.from]`. The
    /// only bonded-amount guard lives in `unstake_tx_at`, the *local builder*,
    /// which a modified node simply does not run. The signature is genuine (the
    /// attacker signs for its own account), so the rejection must come from the
    /// bonded rule, not from crypto.
    ///
    /// Left open, `apply_block_stake_effects` pushed an unbonding entry for the
    /// full amount, which matured ~2 weeks of blocks later into spendable coins
    /// that were never minted — while `locked_stake_total()`'s `.max(0)` clamp
    /// quietly hid the negative STAKE sink.
    #[test]
    fn c3_overdrawn_unstake_block_is_rejected() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();
        assert_eq!(l.staked_of(&alice), 0, "alice bonded nothing");

        let evil = l
            .build_signed_tx_at(
                &alice,
                Ledger::STAKE_SINK,
                10_000 * MICRO,
                TxType::Unstake,
                &crypto_a,
                "2026-07-25T00:00:00+00:00".into(),
                false,
            )
            .expect("an over-unstake still produces a valid signature");
        let block = Ledger::forge_block_at(
            tip.index + 1,
            &tip.hash,
            "2026-07-25T00:00:01+00:00",
            &alice,
            vec![evil],
        );

        let err = l
            .integrate_remote_block(block)
            .expect_err("an unstake exceeding bonded stake must be rejected");
        assert!(err.contains("enjeu bondé"), "rejected on the bonded rule, got: {err}");
        assert_eq!(l.chain_height(), 2, "the chain did not grow");
        assert_eq!(l.balance_of(&alice), 10 * MICRO, "no phantom coins");
    }

    /// **C3 — a legitimate `Stake` then `Unstake` in the SAME block stays valid.**
    /// The rule walks the block sequentially over a running bonded map, exactly
    /// like COVER does for spendable balance, so an earlier `Stake` counts.
    #[test]
    fn c3_sequential_stake_then_unstake_in_one_block_is_accepted() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        let stake = l
            .build_signed_tx_at(&alice, Ledger::STAKE_SINK, 5 * MICRO, TxType::Stake,
                &crypto_a, "2026-07-25T00:00:00+00:00".into(), false)
            .expect("stake tx");
        let unstake = l
            .build_signed_tx_at(&alice, Ledger::STAKE_SINK, 5 * MICRO, TxType::Unstake,
                &crypto_a, "2026-07-25T00:00:01+00:00".into(), false)
            .expect("unstake tx");
        let block = Ledger::forge_block_at(
            tip.index + 1,
            &tip.hash,
            "2026-07-25T00:00:02+00:00",
            &alice,
            vec![stake, unstake],
        );

        l.integrate_remote_block(block)
            .expect("staking then unstaking the same amount in one block is legal");
    }

    /// **C2 (AUDIT-2026-07-25) — a synthetic sender must not be able to mint.**
    ///
    /// Three guards protect money creation and a `Transfer` from `"NETWORK"` passed
    /// all three: `verify_tx` returns `Ok(true)` unconditionally for synthetic
    /// senders, `uncovered_tx_indices` skips the coverage debit for them, and
    /// `validate_block_emission_against` only sums `TxType::Mining` — then returns
    /// `Ok(())` early when that total is zero. EMIT-1 filters on `Mining` too, so
    /// it never sees the tx either. The block needs no signature at all.
    ///
    /// Note every adversarial synthetic test in this file forges the tx as
    /// `Mining`; none tried `Transfer`, which is exactly why this survived.
    #[test]
    fn c2_synthetic_transfer_block_is_rejected() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();
        let minted_before = l.stats().total_mined;

        // 100M QUANTA out of thin air, unsigned, from the synthetic sender.
        let evil = l.build_unsigned_tx("NETWORK", &alice, 100_000_000 * MICRO, TxType::Transfer);
        let block = Ledger::forge_block_at(
            tip.index + 1,
            &tip.hash,
            "2026-07-25T00:00:01+00:00",
            &alice,
            vec![evil],
        );

        let err = l
            .integrate_remote_block(block)
            .expect_err("a synthetic-sender Transfer must be rejected");
        assert!(err.contains("synthétique"), "rejected as synthetic, got: {err}");
        assert_eq!(l.balance_of(&alice), 10 * MICRO, "no coins were minted");
        assert_eq!(l.stats().total_mined, minted_before, "supply is untouched");
        assert_eq!(l.chain_height(), 2, "the chain did not grow");
    }

    /// **C2 — an `ESCROW` sender is never legal in a sealed block.**
    /// `escrow_release_to` has no caller outside tests, so the sound rule is the
    /// strict one. `sim.rs` already proves an unbacked ESCROW release breaks
    /// conservation — but only the simulation's invariant checker caught it, never
    /// a consensus rule.
    #[test]
    fn c2_synthetic_escrow_sender_is_rejected() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        let evil = l.build_unsigned_tx("ESCROW", &alice, 42 * MICRO, TxType::Transfer);
        let block = Ledger::forge_block_at(
            tip.index + 1,
            &tip.hash,
            "2026-07-25T00:00:02+00:00",
            &alice,
            vec![evil],
        );
        assert!(
            l.integrate_remote_block(block).is_err(),
            "ESCROW may never be a sender in a sealed block"
        );
    }

    /// **C2 — the legitimate coinbase must still pass.** A rule that rejects the
    /// one synthetic tx the protocol needs would halt the chain, so this guards
    /// the guard.
    #[test]
    fn c2_legitimate_coinbase_still_seals_and_integrates() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        assert_eq!(l.chain_height(), 2, "the coinbase path is unaffected");
        assert_eq!(l.balance_of(&alice), 10 * MICRO);
    }

    /// **COVER-1 §6.1 — an uncovered transfer makes the block INVALID.** Alice
    /// holds 10 on-chain; a block carrying a (validly-signed) transfer of 50 from
    /// her is rejected at validation — the spend of coins that don't exist never
    /// touches the ledger. The signature is genuine (so the rejection is coverage,
    /// not a forged sig): `build_signed_tx` does not itself check coverage — that
    /// is exactly the hole COVER-1 closes at the block.
    #[test]
    fn cover1_uncovered_transfer_block_rejected() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        let evil = l
            .build_signed_tx_at(&alice, &bob, 50 * MICRO, TxType::Transfer, &crypto_a,
                "2026-01-01T00:00:00Z".into(), false)
            .expect("an over-spend tx still produces a valid signature");
        let block = Ledger::forge_block_at(tip.index + 1, &tip.hash,
            "2026-01-01T00:00:01Z", &alice, vec![evil]);

        let res = l.integrate_remote_block(block);
        let err = res.expect_err("a transfer exceeding the on-chain balance must be rejected");
        assert!(err.contains("non couverte"), "rejected for coverage, got: {err}");
        // The rejection touched nothing.
        assert_eq!(l.chain_height(), 2, "chain unchanged by a rejected block");
        assert_eq!(l.balance_of(&alice), 10 * MICRO, "balance untouched");
    }

    /// **COVER-1 §6.2 — an uncovered Stake makes the block INVALID.** Identical to
    /// the transfer case but with a `Stake`: staking is a spend (it moves coins
    /// into the locked sink), so it is covered the same way — you cannot bond
    /// coins you don't have (which would forge consensus weight).
    #[test]
    fn cover1_uncovered_stake_block_rejected() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        let evil = l
            .build_signed_tx_at(&alice, Ledger::STAKE_SINK, 50 * MICRO, TxType::Stake, &crypto_a,
                "2026-01-01T00:00:00Z".into(), false)
            .expect("an over-stake tx still signs");
        let block = Ledger::forge_block_at(tip.index + 1, &tip.hash,
            "2026-01-01T00:00:01Z", &alice, vec![evil]);

        let err = l.integrate_remote_block(block)
            .expect_err("a Stake exceeding the on-chain balance must be rejected");
        assert!(err.contains("non couverte"), "rejected for coverage, got: {err}");
        assert_eq!(l.staked_of(&alice), 0, "no consensus weight forged from absent coins");
    }

    /// **COVER-1 §6.3 — coverage is SEQUENTIAL within a block.** Alice holds 100.
    /// A block `[→Bob 50, →Carol 60]` is rejected: the first leg passes (50 left),
    /// the second is then uncovered (50 < 60). The *same* first leg with a second
    /// leg of 40 (total 90 ≤ 100) is accepted — proving it is the running-balance
    /// depletion that rejects, not merely the presence of two txs. The order is
    /// the block's tx order, deterministic on every node.
    #[test]
    fn cover1_sequential_coverage_within_block() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);
        let mut crypto_c = pq_wallet();
        let carol = gen_addr(&mut crypto_c);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        // Over-budget sequence: 50 then 60 — second leg becomes uncovered.
        let a0 = l.build_signed_tx_at(&alice, &bob, 50 * MICRO, TxType::Transfer, &crypto_a,
            "2026-01-01T00:00:00Z".into(), false).unwrap();
        let a1 = l.build_signed_tx_at(&alice, &carol, 60 * MICRO, TxType::Transfer, &crypto_a,
            "2026-01-01T00:00:01Z".into(), false).unwrap();
        let bad = Ledger::forge_block_at(tip.index + 1, &tip.hash,
            "2026-01-01T00:00:02Z", &alice, vec![a0, a1]);
        let err = l.integrate_remote_block(bad)
            .expect_err("the second leg is uncovered once the first depletes the balance");
        assert!(err.contains("non couverte"), "got: {err}");

        // In-budget sequence on the SAME start state (the rejection mutated nothing):
        // 50 then 40 — total 90 ≤ 100, both covered sequentially.
        let b0 = l.build_signed_tx_at(&alice, &bob, 50 * MICRO, TxType::Transfer, &crypto_a,
            "2026-01-01T00:00:03Z".into(), false).unwrap();
        let b1 = l.build_signed_tx_at(&alice, &carol, 40 * MICRO, TxType::Transfer, &crypto_a,
            "2026-01-01T00:00:04Z".into(), false).unwrap();
        let good = Ledger::forge_block_at(tip.index + 1, &tip.hash,
            "2026-01-01T00:00:05Z", &alice, vec![b0, b1]);
        assert_eq!(l.integrate_remote_block(good), Ok(true),
            "a sequence within the budget is accepted (no liveness regression)");
        assert_eq!(l.balance_of(&alice), 10 * MICRO, "100 − 50 − 40 = 10");
        assert_eq!(l.balance_of(&bob), 50 * MICRO);
        assert_eq!(l.balance_of(&carol), 40 * MICRO);
    }

    /// **COVER-1 §6.4 — BOTH paths reject the uncovered, identically.** The same
    /// uncovered transfer (Alice → Bob 50) is rejected on the **linear** path (a
    /// block extending the tip) AND on the **fork-reorg** path (a higher-hash block
    /// replacing the tip) — proving the coverage check lives on the *shared*
    /// validator, not just one path (the FORK-CAP lesson). The reorg is checked
    /// against the chain WITHOUT the tip it would replace, and the rejected reorg
    /// truncates nothing (AUDIT-BLK-2 preserved).
    #[test]
    fn cover1_both_paths_reject_uncovered() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        // Chain: genesis(0), block1(1: +10 alice), block2 = tip T(2: +10 alice).
        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let block1 = l.block_at(1).unwrap().clone();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(2).unwrap().clone();
        let minted = l.total_minted();

        // One uncovered tx (50 > alice's balance on EITHER prefix: 10 vs 20).
        let evil = l.build_signed_tx_at(&alice, &bob, 50 * MICRO, TxType::Transfer, &crypto_a,
            "2026-01-01T00:00:00Z".into(), false).unwrap();

        // LINEAR path — block at height 3 extending T (alice has 20 ≤ chain through T).
        let linear = Ledger::forge_block_at(tip.index + 1, &tip.hash,
            "2026-01-01T00:00:01Z", &alice, vec![evil.clone()]);
        let lin_err = l.integrate_remote_block(linear)
            .expect_err("linear integration rejects the uncovered spend");
        assert!(lin_err.contains("non couverte"), "linear: {lin_err}");

        // FORK-REORG path — a higher-hash block at height 2 replacing T, checked
        // against the chain WITHOUT T (alice has 10 there).
        let reorg = forge_block_hash_above(tip.index, &block1.hash, &alice, &[evil], &tip.hash);
        assert!(reorg.hash.as_str() > tip.hash.as_str(), "must trigger the reorg branch");
        let reorg_err = l.integrate_remote_block(reorg)
            .expect_err("fork-reorg rejects the uncovered winner exactly like the linear path");
        assert!(reorg_err.contains("non couverte"), "reorg: {reorg_err}");

        // Neither rejection mutated the chain — T is still the tip, nothing truncated.
        assert_eq!(l.chain_height(), 3, "rejected blocks leave the chain intact");
        assert_eq!(l.block_at(2).unwrap().hash, tip.hash, "tip unchanged (no truncation)");
        assert_eq!(conservation_lhs(&l), minted, "conservation intact after both rejections");
    }

    /// **COVER-1 §6.5 — a fully-covered block PASSES (no liveness regression), and
    /// §3 intra-block credits count.** Alice holds 0 on-chain, but a block credits
    /// her a 5-QUANTA mining reward and then spends 3 of it to Bob, in that order.
    /// The reward (synthetic `NETWORK` sender, exempt) funds the same-block spend
    /// sequentially, so the block is covered and integrates.
    #[test]
    fn cover1_valid_block_with_intra_block_credit_passes() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        let genesis = l.block_at(0).unwrap().clone();

        // A NETWORK→alice mining reward, minted on a throwaway ledger.
        let mining_tx = {
            let mut tmp = Ledger::new();
            tmp.mine_tx(&alice, 3 * MICRO, 0.0)
        };
        // Alice spends 2 of the reward she receives IN THE SAME BLOCK (§3).
        let spend = l.build_signed_tx_at(&alice, &bob, 2 * MICRO, TxType::Transfer, &crypto_a,
            "2026-03-01T00:00:00Z".into(), false).unwrap();
        // miner == alice (EMIT-1: the single reward is credited to block.miner).
        let block = Ledger::forge_block_at(1, &genesis.hash, "2026-03-01T00:00:01Z",
            &alice, vec![mining_tx, spend]);

        assert_eq!(l.integrate_remote_block(block), Ok(true),
            "reward-funds-same-block-spend is covered sequentially (§3)");
        assert_eq!(l.balance_of(&alice), MICRO, "3 reward − 2 spent = 1");
        assert_eq!(l.balance_of(&bob), 2 * MICRO, "bob received 2");
        assert_eq!(conservation_lhs(&l), l.total_minted(), "conservation holds for the accepted block");
    }

    /// **COVER-1 §6.6 — rejecting the uncovered keeps conservation exact.** With
    /// the spend rejected at validation it never reaches the cache, so the class
    /// "uncovered spend breaks/masks conservation" is closed AT validation: `Σ
    /// spendable + locked + burned == minted` holds, and no balance entry is
    /// negative. (Determinism — C1 — is the separate 128-run meta-test; this change
    /// only reads the chain and a `Vec`, both order-deterministic.)
    #[test]
    fn cover1_rejected_uncovered_preserves_conservation() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let minted = l.total_minted();
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        let evil = l.build_signed_tx_at(&alice, &bob, 500 * MICRO, TxType::Transfer, &crypto_a,
            "2026-01-01T00:00:00Z".into(), false).unwrap();
        let block = Ledger::forge_block_at(tip.index + 1, &tip.hash,
            "2026-01-01T00:00:01Z", &alice, vec![evil]);
        assert!(l.integrate_remote_block(block).is_err(), "uncovered block rejected");

        assert_eq!(conservation_lhs(&l), minted, "conservation exact after the rejection");
        assert_eq!(l.balance_of(&alice), 100 * MICRO, "balance untouched by the rejected spend");
        assert!(l.balance_cache.values().all(|v| *v >= 0), "no negative cache entry — nothing leaked in");
    }

    /// **COVER-1 — no-drift guard for `onchain_spendable_before`.** The pure
    /// chain-replay the coverage check relies on must equal the ledger's own
    /// (chain-only) balance cache, or the verdict could diverge from reality. After
    /// a mixed history (mine, transfer+burn, stake, unstake) sealed with an empty
    /// mempool, the replay of the chain up to the tip matches `balance_cache`
    /// entry-for-entry — generic moves, the STAKE-sink lock, and Unstake's
    /// no-spendable-effect all mirrored.
    #[test]
    fn cover1_onchain_replay_matches_live_cache_no_drift() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        l.transfer_with_burn(&alice, &bob, 20 * MICRO, &crypto_a).expect("transfer");
        l.seal_block(&alice, 0.0);
        l.stake_tx(&alice, 30 * MICRO, &crypto_a).expect("stake");
        l.seal_block(&alice, 0.0);
        l.unstake_tx(&alice, 10 * MICRO, &crypto_a).expect("unstake");
        l.seal_block(&alice, 0.0);
        assert_eq!(l.pending_count(), 0, "all txs sealed — cache is chain-only");

        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();
        let replay = l.onchain_spendable_before(&tip);
        let norm = |m: &HashMap<String, i128>| {
            let mut v: Vec<(String, i128)> =
                m.iter().filter(|(_, x)| **x != 0).map(|(k, x)| (k.clone(), *x)).collect();
            v.sort();
            v
        };
        assert_eq!(norm(&replay), norm(&l.balance_cache),
            "onchain replay up to the tip must equal the live chain-only cache (no drift)");
    }

    // ── COVER-2: coverage at SEAL (exclude uncovered, build a valid block) ──

    /// Seal a block AND assert the §3 invariant: a self-produced block always
    /// passes `validate_block_against_prev` (the integration check). Captures the
    /// pre-seal tip + on-chain balances, seals, then validates the result against
    /// them — exactly what a peer integrating this block would compute.
    #[cfg(test)]
    fn seal_and_validate(l: &mut Ledger, miner: &str, ts: &str) -> Block {
        let prev = l.block_at(l.chain_height() - 1).unwrap().clone();
        let onchain = l.onchain_spendable_before(&prev);
        let bindings = l.pq_bindings_before(&prev);
        // PROPOSER-1: same bonded set a real peer would use (l sits at the parent).
        let bonded = l.validator_stakes();
        let block = l.seal_block_at(miner, 0.0, ts.to_string());
        assert!(
            Ledger::validate_block_against_prev(&block, &prev, &onchain, &bindings, &bonded).is_ok(),
            "COVER-2 §3: a self-sealed block must always pass validation (block #{})",
            block.index
        );
        block
    }

    /// **COVER-2 §6.1 — an uncovered transfer is EXCLUDED at seal.** A node admits
    /// an uncovered transfer via `replay_remote_tx` (the no-coverage-gate mempool
    /// path COVER-1 §5 leaves open), then seals: the tx is ABSENT from the block,
    /// the block passes integration validation (§3), and conservation is restored
    /// (the cache revert undoes the transient phantom the uncovered pending tx
    /// created at admission).
    #[test]
    fn cover2_uncovered_transfer_excluded_at_seal() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let minted = l.total_minted();

        let evil = l.build_signed_tx_at(&alice, &bob, 200 * MICRO, TxType::Transfer, &crypto_a,
            "2026-04-01T00:00:00Z".into(), false).unwrap();
        let evil_hash = evil.hash.clone();
        assert!(l.replay_remote_tx(evil), "admitted to pending (no coverage gate at admission)");

        let block = seal_and_validate(&mut l, &alice, "2026-04-01T00:00:01Z");
        assert!(!block.transactions.iter().any(|t| t.hash == evil_hash),
            "the uncovered tx is EXCLUDED from the sealed block");
        assert_eq!(conservation_lhs(&l), minted, "conservation restored — phantom undone by the revert");
        assert_eq!(l.balance_of(&alice), 100 * MICRO, "alice's coins are intact (the spend never happened)");
        assert_eq!(l.balance_of(&bob), 0, "bob received nothing");
    }

    /// **COVER-2 §6.2 — an uncovered Stake is EXCLUDED at seal.** Same as the
    /// transfer case: no consensus weight is bonded from coins that don't exist.
    #[test]
    fn cover2_uncovered_stake_excluded_at_seal() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let minted = l.total_minted();

        let evil = l.build_signed_tx_at(&alice, Ledger::STAKE_SINK, 200 * MICRO, TxType::Stake, &crypto_a,
            "2026-04-01T00:00:00Z".into(), false).unwrap();
        let evil_hash = evil.hash.clone();
        assert!(l.replay_remote_tx(evil));

        let block = seal_and_validate(&mut l, &alice, "2026-04-01T00:00:01Z");
        assert!(!block.transactions.iter().any(|t| t.hash == evil_hash), "uncovered stake excluded");
        assert_eq!(l.staked_of(&alice), 0, "no consensus weight bonded from absent coins");
        assert_eq!(conservation_lhs(&l), minted, "conservation restored");
        assert_eq!(l.balance_of(&alice), 100 * MICRO, "coins intact");
    }

    /// **COVER-2 §6.3 — sequential exclusion at seal.** Two admitted txs: the first
    /// (60) is covered, the second (60) becomes uncovered once the first depletes
    /// the balance (40 < 60). The first is sealed, the second excluded — the
    /// running-balance depletion, not the count, drives the exclusion.
    #[test]
    fn cover2_sequential_exclusion_at_seal() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);
        let mut crypto_c = pq_wallet();
        let carol = gen_addr(&mut crypto_c);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let minted = l.total_minted();

        let t1 = l.build_signed_tx_at(&alice, &bob, 60 * MICRO, TxType::Transfer, &crypto_a,
            "2026-04-01T00:00:00Z".into(), false).unwrap();
        let t1_hash = t1.hash.clone();
        let t2 = l.build_signed_tx_at(&alice, &carol, 60 * MICRO, TxType::Transfer, &crypto_a,
            "2026-04-01T00:00:01Z".into(), false).unwrap();
        let t2_hash = t2.hash.clone();
        assert!(l.replay_remote_tx(t1));
        assert!(l.replay_remote_tx(t2));

        let block = seal_and_validate(&mut l, &alice, "2026-04-01T00:00:02Z");
        assert!(block.transactions.iter().any(|t| t.hash == t1_hash), "the covered leg is sealed");
        assert!(!block.transactions.iter().any(|t| t.hash == t2_hash), "the now-uncovered leg is excluded");
        assert_eq!(l.balance_of(&alice), 40 * MICRO, "100 − 60 (only the covered leg) = 40");
        assert_eq!(l.balance_of(&bob), 60 * MICRO);
        assert_eq!(l.balance_of(&carol), 0, "carol got nothing — her funding leg was excluded");
        assert_eq!(conservation_lhs(&l), minted, "conservation holds");
    }

    /// **COVER-2 §6.4 — the auto-corruption scenario is CLOSED.** node_a admits a
    /// covered tx AND a malicious uncovered tx via gossip replay, then seals.
    /// COVER-2 excludes the uncovered one, so node_a's block is valid — and the
    /// decisive proof is that a PEER (node_b) integrates it cleanly (`Ok(true)`)
    /// and the two nodes converge. A node can no longer corrupt its own chain.
    #[test]
    fn cover2_auto_corruption_scenario_closed() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);
        let mut crypto_c = pq_wallet();
        let carol = gen_addr(&mut crypto_c);

        let mut node_a = Ledger::new();
        node_a.mine_tx(&alice, 100 * MICRO, 0.0);
        node_a.seal_block(&alice, 0.0);
        let mut node_b = Ledger::restore(node_a.snapshot());

        let covered = node_a.build_signed_tx_at(&alice, &bob, 30 * MICRO, TxType::Transfer, &crypto_a,
            "2026-04-01T00:00:00Z".into(), false).unwrap();
        let cov_hash = covered.hash.clone();
        let evil = node_a.build_signed_tx_at(&alice, &carol, 200 * MICRO, TxType::Transfer, &crypto_a,
            "2026-04-01T00:00:01Z".into(), false).unwrap();
        let evil_hash = evil.hash.clone();
        assert!(node_a.replay_remote_tx(covered));
        assert!(node_a.replay_remote_tx(evil));

        let block = node_a.seal_block_at(&alice, 0.0, "2026-04-01T00:00:02Z".into());
        assert!(block.transactions.iter().any(|t| t.hash == cov_hash), "covered tx sealed");
        assert!(!block.transactions.iter().any(|t| t.hash == evil_hash),
            "uncovered tx excluded — chain not corrupted");

        // Decisive: a peer ACCEPTS node_a's self-sealed block (no divergence).
        assert_eq!(node_b.integrate_remote_block(block), Ok(true),
            "a peer accepts the self-sealed block — auto-corruption closed");
        assert_eq!(node_b.balance_of(&alice), 70 * MICRO, "covered transfer applied on the peer");
        assert_eq!(node_a.balance_of(&alice), node_b.balance_of(&alice), "nodes converge (alice)");
        assert_eq!(node_a.balance_of(&bob), node_b.balance_of(&bob), "nodes converge (bob)");
    }

    /// **COVER-2 §3 (closing invariant) — every block `seal_block_at` produces
    /// passes `validate_block_against_prev`.** Exercised across a covered mempool,
    /// a fully-uncovered mempool (→ empty but valid block), and a mix. The
    /// self-produced block is integration-valid in all cases — the property that
    /// proves a node can no longer corrupt its own chain.
    #[test]
    fn cover2_self_sealed_block_always_validates() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);

        // (a) covered mempool (just the mining reward) — seal_and_validate asserts §3.
        seal_and_validate(&mut l, &alice, "2026-05-01T00:00:01Z");
        assert_eq!(l.balance_of(&alice), 100 * MICRO);

        // (b) fully-uncovered mempool → all excluded → empty but valid block.
        let e1 = l.build_signed_tx_at(&alice, &bob, 500 * MICRO, TxType::Transfer, &crypto_a,
            "2026-05-01T01:00:00Z".into(), false).unwrap();
        l.replay_remote_tx(e1);
        let empty = seal_and_validate(&mut l, &alice, "2026-05-01T01:00:01Z");
        assert!(empty.transactions.is_empty(), "all-uncovered mempool seals an empty (valid) block");
        assert_eq!(l.balance_of(&alice), 100 * MICRO, "uncovered spend never happened");

        // (c) mixed: one covered, one uncovered.
        let c = l.build_signed_tx_at(&alice, &bob, 20 * MICRO, TxType::Transfer, &crypto_a,
            "2026-05-01T02:00:00Z".into(), false).unwrap();
        l.replay_remote_tx(c);
        let u = l.build_signed_tx_at(&alice, &bob, 900 * MICRO, TxType::Transfer, &crypto_a,
            "2026-05-01T02:00:01Z".into(), false).unwrap();
        l.replay_remote_tx(u);
        let block = seal_and_validate(&mut l, &alice, "2026-05-01T02:00:02Z");
        assert_eq!(
            block.transactions.iter().filter(|t| t.tx_type == TxType::Transfer).count(),
            1,
            "exactly the covered transfer is sealed"
        );
        assert_eq!(l.balance_of(&bob), 20 * MICRO);
    }

    /// **COVER-2 §6 — no regression: covered txs seal normally.** An honestly-built
    /// covered transfer (+ its burn leg) is sealed intact — liveness preserved, the
    /// exclusion logic touches only genuinely-uncovered txs.
    #[test]
    fn cover2_covered_txs_sealed_normally_no_regression() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);

        let (tx, burn, _) = l.transfer_with_burn(&alice, &bob, 50 * MICRO, &crypto_a)
            .expect("covered transfer builds");
        let tx_hash = tx.hash.clone();
        let burn_hash = burn.as_ref().map(|b| b.hash.clone());

        let block = seal_and_validate(&mut l, &alice, "2026-06-01T00:00:00Z");
        assert!(block.transactions.iter().any(|t| t.hash == tx_hash),
            "covered transfer IS sealed (liveness preserved)");
        if let Some(bh) = burn_hash {
            assert!(block.transactions.iter().any(|t| t.hash == bh), "the burn leg is sealed too");
        }
        assert_eq!(l.balance_of(&alice), 50 * MICRO, "alice: 100 − 50 = 50");
        assert_eq!(l.balance_of(&bob), 49_500_000, "bob: 50 − 1% burn = 49.5");
    }

    /// **COVER-2 — exclusion is per-tx and dependency-aware.** A tx funded ONLY by
    /// an excluded tx is ITSELF excluded (the excluded tx's credit never enters the
    /// running balance), while an INDEPENDENTLY-covered tx that comes *after* an
    /// excluded one is still kept. Proves seal doesn't "drop everything after the
    /// first uncovered" (a liveness bug) nor keep a tx funded by phantom coins (a
    /// safety bug). Cache + conservation are restored regardless.
    #[test]
    fn cover2_dependent_exclusion_keeps_independent_covered() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_z = pq_wallet();
        let zoe = gen_addr(&mut crypto_z); // unfunded sender
        let dave = "d".repeat(64);
        let erin = "e".repeat(64);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let minted = l.total_minted();

        // tx_A: zoe→dave 50 — UNCOVERED (zoe has 0).
        let tx_a = l.build_signed_tx_at(&zoe, &dave, 50 * MICRO, TxType::Transfer, &crypto_z,
            "2026-07-01T00:00:00Z".into(), false).unwrap();
        let a_hash = tx_a.hash.clone();
        // tx_B: dave→erin 40 — would be covered ONLY by tx_A's phantom credit.
        // (dave is signed by a throwaway key; replay_remote_tx doesn't verify sigs.)
        let mut crypto_d = pq_wallet();
        let _ = crypto_d.generate_keypair();
        let tx_b = l.build_signed_tx_at(&dave, &erin, 40 * MICRO, TxType::Transfer, &crypto_d,
            "2026-07-01T00:00:01Z".into(), false).unwrap();
        let b_hash = tx_b.hash.clone();
        // tx_C: alice→erin 30 — INDEPENDENTLY covered (alice has 100), AFTER the excluded ones.
        let tx_c = l.build_signed_tx_at(&alice, &erin, 30 * MICRO, TxType::Transfer, &crypto_a,
            "2026-07-01T00:00:02Z".into(), false).unwrap();
        let c_hash = tx_c.hash.clone();
        assert!(l.replay_remote_tx(tx_a));
        assert!(l.replay_remote_tx(tx_b));
        assert!(l.replay_remote_tx(tx_c));

        let block = seal_and_validate(&mut l, &alice, "2026-07-01T00:00:03Z");
        assert!(!block.transactions.iter().any(|t| t.hash == a_hash), "tx_A (uncovered) excluded");
        assert!(!block.transactions.iter().any(|t| t.hash == b_hash),
            "tx_B excluded — its only funding (tx_A) was excluded, so it is NOT covered by phantom coins");
        assert!(block.transactions.iter().any(|t| t.hash == c_hash),
            "tx_C kept — independently covered despite following excluded txs (no over-pruning)");
        assert_eq!(l.balance_of(&alice), 70 * MICRO, "100 − 30 (only tx_C) = 70");
        assert_eq!(l.balance_of(&erin), 30 * MICRO, "erin got only tx_C's 30");
        assert_eq!(l.balance_of(&dave), 0, "dave: phantom credit reverted");
        assert_eq!(conservation_lhs(&l), minted, "conservation restored after dependency-aware exclusion");
    }

    /// **COVER-2 — an out-of-order COVERED tx survives exclusion-elsewhere + reorg
    /// (no permanent loss).** Rebuts the eviction-liveness concern: even if a peer
    /// excludes a covered-but-out-of-order tx at seal AND its (T1-only) block wins
    /// the fork, the node that HELD the tx covered re-queues it on reorg
    /// (AUDIT-BLK-1) and re-seals it once its funding is on-chain. The tx is never
    /// lost: it always rides into a block via the holder, then propagates.
    #[test]
    fn cover2_out_of_order_covered_tx_survives_exclusion_and_reorg() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);
        let mut crypto_c = pq_wallet();
        let carol = gen_addr(&mut crypto_c);

        // node_a holds T1 (Alice→Bob 30) then T2 (Bob→Carol 20) IN ORDER — both
        // covered (T1 funds T2) — and seals a block with both.
        let mut node_a = Ledger::new();
        node_a.mine_tx(&alice, 100 * MICRO, 0.0);
        node_a.seal_block(&alice, 0.0);
        let tip = node_a.block_at(node_a.chain_height() - 1).unwrap().clone();

        let t1 = node_a.transfer_tx_at(&alice, &bob, 30 * MICRO, &crypto_a,
            "2026-08-01T00:00:00Z".into(), false).unwrap();
        let t2 = node_a.transfer_tx_at(&bob, &carol, 20 * MICRO, &crypto_b,
            "2026-08-01T00:00:01Z".into(), false).unwrap();
        let t2_hash = t2.hash.clone();
        let block_a = node_a.seal_block_at(&alice, 0.0, "2026-08-01T00:00:02Z".into());
        assert!(block_a.transactions.iter().any(|t| t.hash == t2_hash), "node_a sealed the covered T2");

        // A competing block at the same height carries ONLY T1 (the shape a peer
        // that excluded the out-of-order T2 would seal). Forge it with a higher
        // hash so it wins the deterministic fork tie-break.
        let block_b = forge_block_hash_above(block_a.index, &tip.hash, &alice, &[t1], &block_a.hash);
        assert!(block_b.hash.as_str() > block_a.hash.as_str(), "block_b wins the fork");

        // node_a reorgs to the winner → AUDIT-BLK-1 re-queues T2 (absent from the
        // winning block), so it is NOT lost despite being excluded elsewhere.
        assert_eq!(node_a.integrate_remote_block(block_b), Ok(true), "reorg to higher-hash block");
        assert!(node_a.pending_txs().iter().any(|t| t.hash == t2_hash),
            "the covered tx is RE-QUEUED on reorg (AUDIT-BLK-1) — not permanently lost");

        // node_a re-seals: T1 is now on-chain, so T2 is covered again and sealed.
        let block_c = node_a.seal_block_at(&alice, 0.0, "2026-08-01T00:00:03Z".into());
        assert!(block_c.transactions.iter().any(|t| t.hash == t2_hash),
            "T2 rides into a block once its funding is on-chain — survives end to end");
        assert_eq!(node_a.balance_of(&carol), 20 * MICRO, "carol ultimately receives her 20");
    }

    // ───────────────────── PQ-MIG-3 §4 : les dents (CRYPTO-ID-1) ─────────────
    //
    // L'autorité d'une transaction = signature ML-DSA valide depuis la clé
    // RÉVÉLÉE **et** cette clé est celle LIÉE au compte (immuable). Ces tests
    // prouvent la fermeture de CRYPTO-ID-1 : attacher sa propre clé ML-DSA à un
    // compte étranger ne passe plus.

    /// A wallet with a CHOSEN Ed25519 seed and a CHOSEN ML-DSA primary seed, so a
    /// test can model "same Ed25519 account, different ML-DSA key" deterministically.
    fn wallet_seeds(ed: u8, pq: u8) -> CryptoEngine {
        let mut c = CryptoEngine::new();
        c.import_keypair(&[ed; 32]).expect("ed seed");
        c.import_pq_identity(&[pq; 32]).expect("pq seed");
        c
    }

    /// §4 chemin nominal : une tx correctement signée (clé révélée = clé liée,
    /// signature ML-DSA valide) est ACCEPTÉE par `verify_tx` et ne viole aucune
    /// liaison.
    #[test]
    fn pqmig3_nominal_signed_tx_accepted() {
        let victim = wallet_seeds(0x11, 0x22);
        let pk = victim.pq_address_hex().expect("ml-dsa address");
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block(&pk, 0.0);
        let tx = ledger.transfer_tx(&pk, &"d".repeat(64), 10 * MICRO, &victim).unwrap();
        assert!(Ledger::verify_tx(&tx).unwrap(), "ML-DSA-signed tx verifies");
        let tip = ledger.chain.last().unwrap().clone();
        let bindings = ledger.pq_bindings_before(&tip);
        assert!(
            Ledger::binding_violations(&bindings, std::slice::from_ref(&tx)).is_empty(),
            "first signed tx establishes the binding — not a violation"
        );
        ledger.seal_block(&pk, 0.0);
        assert!(ledger.balance_of(&"d".repeat(64)) > 0, "nominal tx integrated end to end");
    }

    /// §4 plus aucun repli Ed25519 : une tx ne présentant qu'une signature Ed25519
    /// (couche ML-DSA absente) est REJETÉE.
    #[test]
    fn pqmig3_ed25519_only_tx_rejected() {
        let victim = wallet_seeds(0x11, 0x22);
        let pk = victim.pq_address_hex().expect("ml-dsa address");
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block(&pk, 0.0);
        let mut tx = ledger.transfer_tx(&pk, &"d".repeat(64), 10 * MICRO, &victim).unwrap();
        tx.pq_signature = None;
        tx.pq_public_key = None;
        assert!(
            !Ledger::verify_tx(&tx).unwrap(),
            "Ed25519-only tx rejected — the fallback is gone"
        );
    }

    /// §4 signature ML-DSA invalide rejetée.
    #[test]
    fn pqmig3_invalid_ml_dsa_signature_rejected() {
        let victim = wallet_seeds(0x11, 0x22);
        let pk = victim.pq_address_hex().expect("ml-dsa address");
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block(&pk, 0.0);
        let tx = ledger.transfer_tx(&pk, &"d".repeat(64), 10 * MICRO, &victim).unwrap();
        let mut bad = tx.clone();
        let mut sig = hex::decode(bad.pq_signature.as_ref().unwrap()).unwrap();
        sig[0] ^= 0xFF;
        bad.pq_signature = Some(hex::encode(sig));
        assert!(!Ledger::verify_tx(&bad).unwrap(), "corrupted ML-DSA signature rejected");
    }

    /// §4 substitution de clé rejetée : remplacer la clé révélée (sans re-signer)
    /// change la préimage ⇒ les deux signatures échouent ⇒ rejet.
    #[test]
    fn pqmig3_key_substitution_rejected() {
        let victim = wallet_seeds(0x11, 0x22);
        let pk = victim.pq_address_hex().expect("ml-dsa address");
        let other = wallet_seeds(0x33, 0x44);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block(&pk, 0.0);
        let tx = ledger.transfer_tx(&pk, &"d".repeat(64), 10 * MICRO, &victim).unwrap();
        let mut swapped = tx.clone();
        swapped.pq_public_key = Some(other.pq_identity_hex().unwrap());
        assert!(
            !Ledger::verify_tx(&swapped).unwrap(),
            "swapping the revealed key (pre-image changes) invalidates the signatures"
        );
    }

    /// §4 / PQ-MIG-3B — **LE** test : clé NON LIÉE rejetée. L'attaque CRYPTO-ID-1 :
    /// un adversaire vise le compte `from` de la victime mais ne possède que **sa
    /// propre** clé ML-DSA K_a. Sous le modèle (b), `from` EST l'adresse `addr(K_v)`,
    /// donc la clé révélée doit hasher vers `from` : `verify_tx` **rejette
    /// directement** (la liaison est intrinsèque, `lie(from, K_a)` faux). Casser
    /// Ed25519 ne donne aucun pouvoir — l'adresse ne dérive plus d'une clé Ed25519.
    #[test]
    fn pqmig3_unbound_key_rejected_closes_crypto_id_1() {
        // Victim's ACCOUNT is its ML-DSA address addr(K_v). It funds + seals a tx.
        let victim = wallet_seeds(0x11, 0x22);
        let pk = victim.pq_address_hex().expect("ml-dsa address"); // = addr(K_v)
        let kv = victim.pq_identity_hex().unwrap();
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block(&pk, 0.0);
        let legit = ledger.transfer_tx(&pk, &"d".repeat(64), 10 * MICRO, &victim).unwrap();
        assert!(Ledger::verify_tx(&legit).unwrap());
        ledger.seal_block(&pk, 0.0);
        let tip = ledger.chain.last().unwrap().clone();
        let bindings = ledger.pq_bindings_before(&tip);

        // Attacker: even with the victim's Ed25519 seed (a modelled quantum break),
        // it can only ML-DSA-sign with its OWN independent primary K_a ≠ K_v.
        let attacker = wallet_seeds(0x11, 0x99);
        let ka = attacker.pq_identity_hex().unwrap();
        assert_ne!(ka, kv, "attacker's ML-DSA key differs from the victim's");

        // Evil tx: claims `from = addr(K_v)` but reveals/signs with K_a.
        let evil = sign_tx_with_nonce(&attacker, &pk, &"e".repeat(64), 5 * MICRO, ledger.get_nonce(&pk));
        // INTRINSIC closure: the revealed key does NOT hash to `from` ⇒ `verify_tx`
        // REJECTS it outright (no Ed25519 check, no registry needed) — this IS the
        // closure of CRYPTO-ID-1 under model (b).
        assert!(
            !Ledger::verify_tx(&evil).unwrap(),
            "unbound key (≠ the one whose address is `from`) MUST be rejected by verify_tx"
        );
        assert!(
            !CryptoEngine::address_hex_binds_key_hex(&evil.from, evil.pq_public_key.as_deref().unwrap()),
            "lie(from, K_a) is false — the account commits to K_v, not K_a"
        );
        // Defense-in-depth: the retained on-chain binding registry ALSO flags it.
        assert_eq!(
            Ledger::binding_violations(&bindings, std::slice::from_ref(&evil)),
            vec![0],
            "binding registry (backstop) also rejects the unbound key"
        );
        // …and the full consensus path rejects a block carrying it.
        let tx_root = Ledger::compute_merkle_root(std::slice::from_ref(&evil));
        let idx = tip.index + 1;
        let ts = "2026-09-09T00:00:00Z";
        let payload = format!("{}:{}:{}:{}:{}:{}", idx, tip.hash, ts, pk, 1, tx_root);
        let evil_block = Block {
            index: idx,
            timestamp: ts.into(),
            transactions: vec![evil.clone()],
            prev_hash: tip.hash.clone(),
            hash: hex::encode(blake3::hash(payload.as_bytes()).as_bytes()),
            miner: pk.clone(),
            energy_kwh: 0.0,
        };
        let onchain = ledger.onchain_spendable_before(&tip);
        let bonded = ledger.validator_stakes();
        assert!(
            Ledger::validate_block_against_prev(&evil_block, &tip, &onchain, &bindings, &bonded).is_err(),
            "a block carrying the unbound-key tx is rejected by validation"
        );
    }

    // ── PQ-MIG-3B : from = adresse ML-DSA, autorité PURE ML-DSA + liaison ────────
    //
    // ADR-007 (b) : `tx.from` est désormais l'**adresse ML-DSA** (PQ-MIG-2,
    // `BLAKE3(ADDR_DOMAIN ‖ pk)`). L'autorité d'une tx = signature ML-DSA valide
    // de la clé révélée **et** `lie(from, clé)` — la clé révélée hashe vers `from`.
    // Le co-facteur Ed25519 disparaît du chemin d'autorité (l'adresse n'est plus
    // une clé Ed25519). La faille CRYPTO-ID-1 se ferme **intrinsèquement** : le
    // compte (`from`) commet cryptographiquement la clé, donc on ne peut pas
    // attacher une autre clé à un compte étranger.

    /// PQ-MIG-3B — chemin nominal : une tx dont `from` est l'**adresse ML-DSA** du
    /// signataire, révélant la clé qui hashe vers cette adresse, avec une signature
    /// ML-DSA valide, est ACCEPTÉE. (Échoue avant 3B : l'ancien `verify_tx` exigeait
    /// une signature Ed25519 valide *de `from`*, impossible quand `from` est une
    /// adresse et non une clé Ed25519.)
    #[test]
    fn pqmig3b_nominal_address_tx_accepted() {
        let mut wallet = pq_wallet();
        wallet.generate_keypair(); // couche Ed25519 (transport) ; primaire posé par pq_wallet
        let addr = wallet.pq_address_hex().expect("ml-dsa address");
        let mut ledger = Ledger::new();
        ledger.mine_tx(&addr, 100 * MICRO, 0.0);
        ledger.seal_block(&addr, 0.0);
        let tx = ledger.transfer_tx(&addr, &"d".repeat(64), 10 * MICRO, &wallet).unwrap();
        assert!(
            Ledger::verify_tx(&tx).unwrap(),
            "address-from tx with a valid ML-DSA sig and lie(from,key) holding must verify"
        );
        assert!(
            CryptoEngine::address_hex_binds_key_hex(&tx.from, tx.pq_public_key.as_deref().unwrap()),
            "the revealed key intrinsically hashes to `from` (the address IS the binding)"
        );
    }

    // ── MSIG-1 — native post-quantum M-of-N multisig ─────────────────────────

    fn msig_signer() -> CryptoEngine {
        let mut c = CryptoEngine::new();
        c.generate_pq_identity().unwrap();
        c
    }

    #[test]
    fn msig1_address_is_deterministic_order_free_and_policy_bound() {
        let s: Vec<CryptoEngine> = (0..3).map(|_| msig_signer()).collect();
        let keys: Vec<String> = s.iter().map(|c| c.pq_identity_hex().unwrap()).collect();

        let a = crate::security::multisig_address_hex(&keys, 2).unwrap();
        assert_eq!(a, crate::security::multisig_address_hex(&keys, 2).unwrap(), "deterministic");
        let mut reordered = keys.clone();
        reordered.reverse();
        assert_eq!(a, crate::security::multisig_address_hex(&reordered, 2).unwrap(), "order-independent");
        assert_ne!(a, crate::security::multisig_address_hex(&keys, 3).unwrap(), "threshold-bound");
        // A different key set ⇒ a different address.
        let other = msig_signer().pq_identity_hex().unwrap();
        let swapped = vec![keys[0].clone(), keys[1].clone(), other];
        assert_ne!(a, crate::security::multisig_address_hex(&swapped, 2).unwrap(), "key-set-bound");
        // A multisig address never collides with a single-key address (domain sep).
        assert_ne!(a, CryptoEngine::ml_dsa_address_hex(keys[0].as_bytes()));
    }

    #[test]
    fn msig1_quorum_accept_and_reject() {
        let s: Vec<CryptoEngine> = (0..3).map(|_| msig_signer()).collect();
        let keys: Vec<String> = s.iter().map(|c| c.pq_identity_hex().unwrap()).collect();
        let addr = crate::security::multisig_address_hex(&keys, 2).unwrap();

        // 2-of-3 with two DISTINCT signers → accepted. Any valid pair works.
        for pair in [[0usize, 1], [0, 2], [1, 2]] {
            let mut l = Ledger::new();
            let tx = l
                .build_multisig_tx("recipient", 100, TxType::Transfer, &keys, 2, &[&s[pair[0]], &s[pair[1]]])
                .unwrap();
            assert_eq!(tx.from, addr, "from is the policy address");
            assert!(VerifiedTx::new(tx).is_some(), "2-of-3 pair {pair:?} accepted");
        }

        // Below threshold (one signer) → rejected.
        let mut l = Ledger::new();
        let low = l.build_multisig_tx("r", 100, TxType::Transfer, &keys, 2, &[&s[0]]).unwrap();
        assert!(VerifiedTx::new(low).is_none(), "1 signature < 2 threshold rejected");

        // Duplicate signer cannot inflate the quorum (counted per DISTINCT key).
        let mut l = Ledger::new();
        let dup = l.build_multisig_tx("r", 100, TxType::Transfer, &keys, 2, &[&s[0], &s[0]]).unwrap();
        assert!(VerifiedTx::new(dup).is_none(), "same key twice is one signer");

        // An outsider's signature does not count toward the quorum.
        let outsider = msig_signer();
        let mut l = Ledger::new();
        let out = l
            .build_multisig_tx("r", 100, TxType::Transfer, &keys, 2, &[&s[0], &outsider])
            .unwrap();
        assert!(VerifiedTx::new(out).is_none(), "non-registered signer does not count");
    }

    #[test]
    fn msig1_rejects_rebind_and_tamper() {
        let s: Vec<CryptoEngine> = (0..3).map(|_| msig_signer()).collect();
        let keys: Vec<String> = s.iter().map(|c| c.pq_identity_hex().unwrap()).collect();

        // Rebind: replace a registered key in the auth → `from` no longer matches.
        let mut l = Ledger::new();
        let mut tx = l
            .build_multisig_tx("r", 100, TxType::Transfer, &keys, 2, &[&s[0], &s[1]])
            .unwrap();
        let mut auth: MultisigAuth = serde_json::from_str(tx.pq_signature.as_ref().unwrap()).unwrap();
        auth.pubkeys[0] = msig_signer().pq_identity_hex().unwrap();
        tx.pq_signature = Some(serde_json::to_string(&auth).unwrap());
        assert!(VerifiedTx::new(tx).is_none(), "rebinding keys breaks the address binding");

        // Tamper the amount → recomputed hash disagrees → rejected.
        let mut l = Ledger::new();
        let mut tx = l
            .build_multisig_tx("r", 100, TxType::Transfer, &keys, 2, &[&s[0], &s[1]])
            .unwrap();
        tx.amount = 999_999;
        assert!(VerifiedTx::new(tx).is_none(), "amount tamper breaks the pre-image hash");
    }

    // ── Regression tests for the adversarial-review findings ─────────────────

    /// MSIG-SEC-1: a single key spelled in two hex CASES must not fill two quorum
    /// slots (hex decoding is case-insensitive). Cryptographic canonicalization
    /// collapses the aliases to one key, so a "2-of-2" of one key is impossible.
    #[test]
    fn msig1_case_aliased_keys_collapse() {
        let k = msig_signer();
        let k_lo = k.pq_identity_hex().unwrap();
        let k_hi = k_lo.to_uppercase();
        assert_ne!(k_lo, k_hi, "hex spellings differ as strings");
        let canon = crate::security::canonicalize_msig_keys(&[k_lo.clone(), k_hi.clone()]).unwrap();
        assert_eq!(canon.len(), 1, "case aliases are the SAME key");
        // The "2-of-2" address of the aliases equals the 1-key address (unsatisfiable
        // at threshold 2) — it can never be a genuine 2-of-2.
        assert_eq!(
            crate::security::multisig_address_hex(&[k_lo.clone(), k_hi.clone()], 2),
            crate::security::multisig_address_hex(std::slice::from_ref(&k_lo), 2)
        );
        // Building a 2-of-2 from a single aliased key is refused (threshold 2 > 1 key).
        let mut l = Ledger::new();
        assert!(l
            .build_multisig_tx("r", 1, TxType::Transfer, &[k_lo.clone(), k_hi.clone()], 2, &[&k, &k])
            .is_err());
        // A malformed (non-ML-DSA-65) key invalidates the whole policy.
        assert!(crate::security::canonicalize_msig_keys(&["deadbeef".to_string()]).is_none());
    }

    /// MINT-GUARD-1: a `Mining` tx authorized by a real user account (not the
    /// synthetic `NETWORK`) is a mint forgery and must be rejected at the gate.
    #[test]
    fn mint_guard_rejects_user_authorized_mining_tx() {
        let mut c = CryptoEngine::new();
        let _ = c.generate_keypair();
        c.generate_pq_identity().unwrap();
        let from = c.pq_address_hex().unwrap();
        let mut l = Ledger::new();
        let forged = l.build_signed_tx(&from, &from, 1_000_000, TxType::Mining, &c).unwrap();
        assert_eq!(forged.from, from);
        assert!(VerifiedTx::new(forged).is_none(), "user-signed Mining rejected (MINT-GUARD-1)");
        // A normal Transfer from the same account still verifies (single-key unaffected).
        let ok = l.build_signed_tx(&from, "recipient", 1, TxType::Transfer, &c).unwrap();
        assert!(VerifiedTx::new(ok).is_some());
    }

    /// MINT-GUARD-2: `coalesce_block_rewards` coalesces ONLY genuine `NETWORK`
    /// rewards; a forged non-`NETWORK` `Mining` tx is dropped, never minted.
    #[test]
    fn mint_guard_coalesce_drops_forged_mining() {
        fn mining(from: &str, amount: u64, id: &str) -> Transaction {
            Transaction {
                id: id.into(),
                from: from.into(),
                to: "x".into(),
                amount,
                tx_type: TxType::Mining,
                timestamp: "t".into(),
                signature: String::new(),
                hash: String::new(),
                nonce: 0,
                pq_signature: None,
                pq_public_key: None,
                fault_proof: None,
                slash_unbonding: None,
            }
        }
        let txs = vec![
            mining("NETWORK", 10, "a"),
            mining("NETWORK", 20, "b"),
            mining("attacker", 1000, "c"),
        ];
        // MINT-EXACT-1 : la canonique est ignorée quand le mempool porte déjà des
        // récompenses (chemin du robinet de test) — ce test vise MINT-GUARD-2, la
        // règle « une `Mining` d'un expéditeur non-NETWORK est une forgerie ».
        let out = Ledger::coalesce_block_rewards(txs, 1, "ts");
        let rewards: Vec<&Transaction> = out.iter().filter(|t| t.tx_type == TxType::Mining).collect();
        assert_eq!(rewards.len(), 1, "exactly one coalesced reward");
        assert_eq!(rewards[0].from, "NETWORK");
        assert_eq!(rewards[0].amount, 30, "forged Mining amount is DROPPED, never minted");
    }

    // ─── MINT-EXACT-1 : la récompense est une fonction PURE de la chaîne ─────
    //
    // Le trou fermé ici : le montant de la récompense était calculé **localement**
    // par le sceleur (part Shapley dérivée de watts auto-déclarés par les pairs,
    // donc invérifiables), et le réseau se contentait de le BORNER à
    // `64 × emission_for_tick`. Le montant honnête valant `2 × emission_for_tick / N`
    // (N nœuds vivants), la marge exploitable était de `32 × N` : à 100 nœuds, tout
    // validateur bondé pouvait se frapper 3 200 fois sa récompense légitime, à
    // chaque bloc, et chaque nœud l'acceptait — seul le plafond dur des 100 M
    // était vérifié. Désormais le récepteur **recalcule** au lieu de borner.

    /// La récompense canonique est une fonction pure de la chaîne : elle ne
    /// dépend **ni** du nombre de pairs, **ni** de l'énergie déclarée, **ni** de
    /// quoi que ce soit de local. Deux nœuds sur la même chaîne obtiennent le
    /// même nombre — c'est ce qui permet de recalculer plutôt que de borner.
    #[test]
    fn mint_exact_reward_is_a_pure_function_of_the_chain() {
        let a = Ledger::new();
        let b = Ledger::new();
        assert_eq!(
            a.canonical_block_reward(),
            b.canonical_block_reward(),
            "deux chaînes identiques ⇒ même récompense canonique"
        );
        assert_eq!(
            a.canonical_block_reward(),
            crate::p2p::reputation::emission_for_block(a.stats().total_mined),
            "la récompense EST emission_for_block(offre minée) — rien d'autre"
        );
        // Et elle décroît strictement avec l'offre déjà minée (rareté).
        let mut c = Ledger::new();
        let before = c.canonical_block_reward();
        c.mine_tx(&"a".repeat(64), 3 * MICRO, 0.0);
        c.seal_block(&"a".repeat(64), 0.0);
        assert!(
            c.canonical_block_reward() < before,
            "l'émission décroît à mesure que l'offre est minée"
        );
    }

    /// **La vulnérabilité elle-même.** Un bloc frappant nettement plus que la
    /// récompense canonique — ce que l'ancienne marge de `64 ticks` autorisait —
    /// est REJETÉ. C'est le test qui aurait attrapé le trou.
    #[test]
    fn mint_exact_over_emission_is_rejected() {
        let mut node = Ledger::new();
        let miner = "e".repeat(64);
        let canonical = node.canonical_block_reward();
        let genesis = node.block_at(0).unwrap().hash.clone();

        // L'ancienne borne tolérait 64 ticks == 32 × la récompense d'un bloc.
        let greedy = canonical * 32;
        let block = Ledger::forge_block_at(
            1,
            &genesis,
            "2026-08-01T00:00:00+00:00",
            &miner,
            vec![node.build_unsigned_tx("NETWORK", &miner, greedy, TxType::Mining)],
        );
        let verdict = node.integrate_remote_block(block);
        assert!(
            verdict.is_err(),
            "un bloc frappant 32× la canonique doit être rejeté, obtenu {verdict:?}"
        );
        assert_eq!(node.chain_height(), 1, "la chaîne n'a pas bougé");
        assert_eq!(node.total_minted(), 0, "aucune monnaie créée");

        // Et la frontière est exacte : canonique + 1 µQTA passe déjà de l'autre côté.
        let over_by_one = Ledger::forge_block_at(
            1,
            &genesis,
            "2026-08-01T00:00:00+00:00",
            &miner,
            vec![node.build_unsigned_tx("NETWORK", &miner, canonical + 1, TxType::Mining)],
        );
        assert!(
            node.integrate_remote_block(over_by_one).is_err(),
            "la borne est EXACTE — un seul µQTA de trop suffit à invalider"
        );
    }

    /// Symétrie produire/vérifier (règle COVER-2) : un bloc produit par le chemin
    /// de production — `mint_block_reward` puis `seal_block` — est **valide par
    /// construction** pour un pair frais sur la même genèse.
    #[test]
    fn mint_exact_produced_block_is_valid_by_construction() {
        let mut producer = Ledger::new();
        let miner = "f".repeat(64);
        let expected = producer.canonical_block_reward();

        let minted = producer.mint_block_reward(&miner);
        assert_eq!(minted.len(), 1, "chaîne neuve : aucun participant récent, le producteur seul");
        assert_eq!(minted[0].amount, expected, "le producteur frappe EXACTEMENT la canonique");
        let block = producer.seal_block(&miner, 0.0);

        let mut receiver = Ledger::new();
        assert_eq!(
            receiver.integrate_remote_block(block),
            Ok(true),
            "le bloc de production passe la validation d'un pair frais"
        );
        assert_eq!(receiver.balance_of(&miner), expected, "le sceleur est crédité de la canonique");
        assert_eq!(receiver.total_minted(), expected, "l'offre a grandi d'exactement un bloc");
    }

    /// Frapper deux fois ne rapporte rien : la coalescence somme, et la somme
    /// dépasse la canonique — le bloc devient invalide pour tout le réseau.
    #[test]
    fn mint_exact_double_mint_is_rejected() {
        let mut producer = Ledger::new();
        let miner = "0".repeat(64);

        // (a) La production s'auto-nettoie : frapper deux fois n'accumule pas —
        //     la frappe périmée est révoquée (elle vaudrait pour une autre hauteur).
        producer.mint_block_reward(&miner);
        producer.mint_block_reward(&miner);
        let block = producer.seal_block(&miner, 0.0);
        let minted: u64 = block
            .transactions
            .iter()
            .filter(|t| t.tx_type == TxType::Mining)
            .map(|t| t.amount)
            .sum();
        let mut fresh = Ledger::new();
        assert_eq!(minted, fresh.canonical_block_reward(), "une seule récompense canonique");

        // (b) Et un bloc FORGÉ qui double la frappe est rejeté par la validation.
        let genesis = fresh.block_at(0).unwrap().hash.clone();
        let doubled = fresh.canonical_block_reward() * 2;
        let forged = Ledger::forge_block_at(
            1,
            &genesis,
            "2026-08-01T00:00:00+00:00",
            &miner,
            vec![fresh.build_unsigned_tx("NETWORK", &miner, doubled, TxType::Mining)],
        );
        assert!(
            fresh.integrate_remote_block(forged).is_err(),
            "le double-mint dépasse la canonique ⇒ rejeté"
        );
    }

    /// **Le fork privé silencieux est fermé.** `mine_tx` scellait tout seul dès
    /// 10 tx en attente — sans contrôle de proposeur PoS et **sans jamais
    /// diffuser** le bloc. Un nœud non éligible accumulant une tx par minute se
    /// fabriquait donc une chaîne privée toutes les 10 minutes et affichait à son
    /// porteur un solde que le réseau ne reconnaissait pas. Plus aucun scellement
    /// ne peut naître d'une simple admission au mempool.
    #[test]
    fn mint_exact_admission_never_seals_a_block() {
        let mut l = Ledger::new();
        let pk = "1".repeat(64);
        let height_before = l.chain_height();
        for _ in 0..25 {
            l.mine_tx(&pk, MICRO, 0.0);
        }
        assert_eq!(
            l.chain_height(),
            height_before,
            "aucun bloc ne doit naître d'une admission au mempool (fork privé fermé)"
        );
        assert_eq!(l.pending_count(), 25, "les tx restent en attente, rien n'est scellé");
    }

    // ─── OPEN-DOOR-1 : la porte ne se referme plus au premier staker ─────────

    /// **Le blocage de lancement, reproduit puis fermé.** Dès qu'un seul compte
    /// bonde l'enjeu minimum, `PROPOSER-1` refuse tout proposeur non bondé — et
    /// comme il n'existe ni faucet, ni airdrop, ni premine, un nouvel arrivant
    /// n'a **aucun** chemin vers sa première pièce : il lui faut un enjeu pour
    /// proposer et proposer pour gagner. Ce test montre les deux faces :
    /// un slot ordinaire lui reste fermé, un **slot ouvert** le laisse entrer.
    #[test]
    fn open_door_newcomer_can_seal_on_an_open_slot_only() {
        use crate::p2p::pos_consensus::{is_open_slot, MIN_VALIDATOR_STAKE};

        // Un validateur bondé existe ⇒ le mode bootstrap permissionless est clos.
        let mut wallet = pq_wallet();
        let whale = gen_addr(&mut wallet);
        let mut l = Ledger::new();
        l.mine_tx(&whale, 3 * MICRO, 0.0);
        l.seal_block(&whale, 0.0);
        l.stake_tx(&whale, MIN_VALIDATOR_STAKE, &wallet).expect("stake builds");
        l.seal_block(&whale, 0.0);
        assert!(
            l.validator_stakes().values().any(|&s| s >= MIN_VALIDATOR_STAKE),
            "la porte est bien refermée : quelqu'un a staké"
        );

        // Le nouvel arrivant : zéro pièce, zéro enjeu.
        let newcomer = "9".repeat(64);
        assert_eq!(l.balance_of(&newcomer), 0, "il n'a rien");
        assert_eq!(l.staked_of(&newcomer), 0, "et rien de bondé");

        // Amener la chaîne juste avant un slot ouvert, scellée par le validateur.
        // `chain_height()` == nombre de blocs == index du PROCHAIN bloc.
        while !is_open_slot(l.chain_height()) {
            l.mint_block_reward(&whale);
            l.seal_block(&whale, 0.0);
        }
        let open_index = l.chain_height();
        assert!(is_open_slot(open_index), "la prochaine hauteur est un slot ouvert");

        // (a) Sur un slot ORDINAIRE, il reste refusé — la règle n'a pas été
        //     affaiblie, elle a été *cadencée*.
        let ordinary_index = open_index + 1;
        assert!(!is_open_slot(ordinary_index));

        // (b) Sur le slot OUVERT, son bloc est accepté par le réseau.
        let tip = l.chain.last().unwrap().hash.clone();
        let reward = l.canonical_block_reward();
        let rewards = l.planned_reward_txs(open_index, &newcomer, reward);
        let newcomer_cut = rewards
            .iter()
            .find(|t| t.to == newcomer)
            .map(|t| t.amount)
            .expect("le nouvel arrivant est payé");
        let open_block = Ledger::forge_block_at(
            open_index,
            &tip,
            "2026-08-01T00:00:00+00:00",
            &newcomer,
            rewards,
        );
        assert_eq!(
            l.integrate_remote_block(open_block),
            Ok(true),
            "un nouvel arrivant sans enjeu DOIT pouvoir sceller un slot ouvert"
        );
        assert_eq!(
            l.balance_of(&newcomer),
            newcomer_cut,
            "il tient sa première pièce — la boucle œuf-poule est rompue"
        );
        assert!(
            newcomer_cut >= MIN_VALIDATOR_STAKE,
            "et un seul slot ouvert suffit à financer son enjeu minimum, partage compris"
        );

        // (c) Le slot suivant lui est de nouveau fermé : la fenêtre est bornée.
        let tip2 = l.chain.last().unwrap().hash.clone();
        let next = Ledger::forge_block_at(
            ordinary_index,
            &tip2,
            "2026-08-01T00:00:02+00:00",
            &newcomer,
            vec![],
        );
        assert!(
            l.integrate_remote_block(next).is_err(),
            "hors slot ouvert, PROPOSER-1 s'applique intégralement"
        );
    }

    /// La cadence est une fonction PURE de la hauteur — aucun état, aucune
    /// horloge, donc tous les nœuds tranchent identiquement (C1) — et elle borne
    /// la capture Sybil à exactement une fenêtre sur `OPEN_SLOT_EVERY_BLOCKS`.
    #[test]
    fn open_door_cadence_is_a_pure_function_of_height() {
        use crate::p2p::pos_consensus::{is_open_slot, OPEN_SLOT_EVERY_BLOCKS};
        assert!(!is_open_slot(0), "la genèse n'est proposée par personne");
        let window = OPEN_SLOT_EVERY_BLOCKS;
        let open = (1..=window * 4).filter(|&i| is_open_slot(i)).count() as u64;
        assert_eq!(open, 4, "exactement une fenêtre ouverte par cycle de {window} blocs");
        for i in 1..=window * 4 {
            assert_eq!(is_open_slot(i), i.is_multiple_of(window), "cadence déterministe à la hauteur {i}");
        }
    }

    // ─── REWARD-SHARE-1 : la récompense se partage, et le partage est vérifié ──
    //
    // Avant, tout le bloc allait au seul sceleur : on n'était payé qu'en gagnant
    // un slot, donc la quasi-totalité des participants ne touchait jamais rien.
    // Désormais la moitié va au producteur et l'autre aux **participants récents**
    // constatés par la chaîne — et la répartition est recalculée par chaque nœud,
    // exactement comme le montant total.

    /// Le partage est une fonction pure de la chaîne, et il rémunère la
    /// **liveness** : on y entre en produisant un bloc, on en sort en cessant.
    #[test]
    fn reward_share_participants_come_from_the_chain_itself() {
        use crate::p2p::reputation::SHARE_WINDOW_BLOCKS;
        let mut l = Ledger::new();
        let a = "a".repeat(64);
        let b = "b".repeat(64);

        assert!(
            l.recent_participants(l.chain_height()).is_empty(),
            "chaîne neuve : aucun participant, le producteur prendra tout"
        );
        l.mint_block_reward(&a);
        l.seal_block(&a, 0.0);
        l.mint_block_reward(&b);
        l.seal_block(&b, 0.0);

        let now = l.chain_height();
        let seen = l.recent_participants(now);
        assert!(seen.contains(&a) && seen.contains(&b), "les deux producteurs sont vus");

        // Hors fenêtre, un producteur inactif sort du partage — c'est ce qui fait
        // du partage une prime de présence et non une rente.
        let far = now + SHARE_WINDOW_BLOCKS + 1;
        assert!(
            l.recent_participants(far).is_empty(),
            "passé {SHARE_WINDOW_BLOCKS} blocs sans rien produire, on sort du partage"
        );
    }

    /// Le producteur ne rafle plus tout : sa part est celle du barème, le reste
    /// va aux participants, et la somme vaut exactement la récompense canonique
    /// (aucun µQTA ne se perd — la conservation reste vraie au µQTA près).
    #[test]
    fn reward_share_splits_and_conserves_to_the_microqta() {
        use crate::p2p::reputation::{PROPOSER_SHARE_DEN, PROPOSER_SHARE_NUM};
        let mut l = Ledger::new();
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        l.mint_block_reward(&a);
        l.seal_block(&a, 0.0); // A devient participant récent

        let reward = l.canonical_block_reward();
        let plan = l.expected_block_rewards(l.chain_height(), &b, reward);
        let total: u64 = plan.iter().map(|(_, v)| *v).sum();
        assert_eq!(total, reward, "le plan distribue EXACTEMENT la récompense, sans perte");
        assert_eq!(plan.len(), 2, "le producteur B + le participant A");

        let got_b = plan.iter().find(|(k, _)| *k == b).unwrap().1;
        let got_a = plan.iter().find(|(k, _)| *k == a).unwrap().1;
        assert!(got_a > 0, "le participant récent est réellement payé");
        // Barème : part du producteur, plus le reste de division entière (le
        // « dust ») pour que la somme retombe au µQTA près sur la canonique.
        let cut = reward * PROPOSER_SHARE_NUM / PROPOSER_SHARE_DEN;
        let pot = reward - cut;
        assert_eq!(got_a, pot, "l'unique participant reçoit tout le pot partagé");
        assert_eq!(got_b, cut, "et le producteur exactement sa part de barème");
        assert!(got_b >= got_a, "le producteur garde au moins sa part");
    }

    /// **Le partage est imposé, pas suggéré.** Un producteur qui garde tout pour
    /// lui — le comportement qu'un nœud modifié adopterait naturellement — voit
    /// son bloc rejeté par le réseau. Sans ce test, le partage ne serait qu'une
    /// politesse du logiciel de référence.
    #[test]
    fn reward_share_greedy_proposer_is_rejected() {
        let mut l = Ledger::new();
        let a = "a".repeat(64);
        let greedy = "b".repeat(64);
        l.mint_block_reward(&a);
        l.seal_block(&a, 0.0);

        let tip = l.chain.last().unwrap().hash.clone();
        let index = l.chain_height();
        let reward = l.canonical_block_reward();
        let all_for_me = Ledger::forge_block_at(
            index,
            &tip,
            "2026-08-01T00:00:00+00:00",
            &greedy,
            vec![l.build_unsigned_tx("NETWORK", &greedy, reward, TxType::Mining)],
        );
        assert!(
            l.integrate_remote_block(all_for_me).is_err(),
            "un producteur qui capte toute l'émission doit être rejeté"
        );

        // Et payer un tiers qui n'a rien produit ne passe pas davantage.
        let stranger = "c".repeat(64);
        let bribe = Ledger::forge_block_at(
            index,
            &tip,
            "2026-08-01T00:00:01+00:00",
            &greedy,
            vec![l.build_unsigned_tx("NETWORK", &stranger, reward, TxType::Mining)],
        );
        assert!(
            l.integrate_remote_block(bribe).is_err(),
            "un bénéficiaire hors plan est rejeté"
        );

        // Le bloc CONFORME, lui, passe — le contrôle vise la triche, pas la vie.
        let honest = Ledger::forge_block_at(
            index,
            &tip,
            "2026-08-01T00:00:02+00:00",
            &greedy,
            l.planned_reward_txs(index, &greedy, reward),
        );
        assert_eq!(
            l.integrate_remote_block(honest),
            Ok(true),
            "le bloc qui respecte le plan est accepté"
        );
        assert!(l.balance_of(&a) > 0, "le participant récent a bien été payé par un tiers");
    }

    /// Émettre MOINS reste permis (non-inflationnaire), mais la **forme** du
    /// partage tient : un producteur ne peut pas rogner la part des autres sans
    /// rogner la sienne dans la même proportion.
    #[test]
    fn reward_share_is_scale_invariant() {
        let mut l = Ledger::new();
        let a = "a".repeat(64);
        let p = "b".repeat(64);
        l.mint_block_reward(&a);
        l.seal_block(&a, 0.0);

        let index = l.chain_height();
        let tip = l.chain.last().unwrap().hash.clone();
        let half = l.canonical_block_reward() / 2;
        let modest = Ledger::forge_block_at(
            index,
            &tip,
            "2026-08-01T00:00:00+00:00",
            &p,
            l.planned_reward_txs(index, &p, half),
        );
        assert_eq!(
            l.integrate_remote_block(modest),
            Ok(true),
            "émettre moins que la canonique est permis, tant que la forme tient"
        );
        assert!(l.balance_of(&a) > 0, "le participant garde sa fraction du total réduit");
    }

    /// Une frappe restée en attente est **périmée** : elle valait pour une autre
    /// hauteur, donc pour un autre ensemble de participants. Ça arrive en vrai
    /// dès qu'un bloc distant s'intercale entre notre frappe et notre scellement
    /// — et sans la révocation, le bloc suivant serait rejeté par tout le réseau.
    #[test]
    fn reward_share_stale_mint_is_revoked_before_the_next_one() {
        let mut l = Ledger::new();
        let me = "a".repeat(64);
        l.mint_block_reward(&me); // frappe #1, jamais scellée
        l.mint_block_reward(&me); // un bloc distant est passé entre-temps
        let sealed = l.seal_block(&me, 0.0);
        let minted: u64 = sealed
            .transactions
            .iter()
            .filter(|t| t.tx_type == TxType::Mining)
            .map(|t| t.amount)
            .sum();
        let mut fresh = Ledger::new();
        assert_eq!(minted, fresh.canonical_block_reward(), "une seule récompense survit");
        assert_eq!(
            fresh.integrate_remote_block(sealed),
            Ok(true),
            "et le bloc reste valide pour un pair frais"
        );
    }
