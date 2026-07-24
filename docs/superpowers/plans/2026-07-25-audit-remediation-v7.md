# Correction de l'audit interne 2026-07-25 — hard-fork v7

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fermer les quatre défauts critiques et les huit hauts relevés par l'audit
`docs/audit/AUDIT-INTERNE-2026-07-25.md`, en groupant les changements de règle de consensus dans
une seule rupture de protocole (`TORUS_PROTOCOL_VERSION` 6→7).

**Architecture:** Le lot A touche les règles que producteur et validateur doivent partager — il se
termine par le bump de protocole. Chaque correction du lot A suit le motif déjà établi par
COVER-1/COVER-2 : **une seule fonction pure décide, le validateur rejette et le scelleur exclut**,
jamais deux implémentations. Le lot B est local à un processus (RPC, explorateur, tampons mémoire,
affichage) et peut être livré indépendamment, avant ou après le fork.

**Tech Stack:** Rust 2021 / Tauri 2.0, `tokio`, `libsql`, ML-DSA-65 (`hybrid_crypto`), BLAKE3,
Svelte 5.

## Global Constraints

- `tokio::sync` exclusivement — jamais `std::sync` traversé par un `.await`.
- Zéro `unwrap()` en code non-test — `Result<T, E>` et `?`.
- Tous les montants en `u64` µQTA. Jamais de `f64` sur un solde.
- L'autorité d'une transaction est **ML-DSA** ; Ed25519 est transport uniquement.
- Ordre de prise des verrous strict — ne jamais introduire une nouvelle paire inversée.
- Code, identifiants, docstrings et messages de commit **en anglais** ; textes destinés à
  l'utilisateur **en français**, via i18n, ticker `QTA`.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets` doit rester **totalement
  silencieux** après chaque tâche.
- `cargo test --manifest-path src-tauri/Cargo.toml` doit rester **vert** après chaque tâche.
  Base de départ : 449 tests + 1 intégration.
- Aucune tâche ne modifie le hash de genèse. Le bump de protocole (tâche 8) est le seul point de
  rupture réseau.
- **Helpers de test.** Les tâches 3, 5, 6, 12 et 13 s'appuient sur des constructeurs de scénario
  (chaînes avec slash, validateur bondé à une frontière d'époque, blocs concurrents, liens de pool).
  Ne pas les inventer : chaque tâche indique le `grep` qui localise le test voisin existant dont il
  faut reprendre les constructeurs exacts. Un helper inventé qui ne compile pas coûte plus cher que
  cinq minutes de lecture. Ces helpers sont la seule partie du plan écrite depuis les tests
  existants plutôt que depuis le source lu — les traiter comme des squelettes à ajuster, pas comme
  du code à copier.

---

## Structure des fichiers

| Fichier | Responsabilité après ce plan |
|---|---|
| `src-tauri/src/p2p/ledger/validation.rs` | Ajoute deux règles pures : expéditeur synthétique interdit hors coinbase (C2), `Unstake` couvert par l'enjeu bondé (C3). Corrige la relecture de solde pour consommer les slashes (H4). |
| `src-tauri/src/p2p/ledger/stake.rs` | Clampe l'`UnbondEntry` créé ; rend l'annulation exactement inverse (H8). |
| `src-tauri/src/p2p/ledger/reorg.rs` | Câble les deux nouvelles listes d'exclusion dans `seal_block_at` (symétrie COVER-2). |
| `src-tauri/src/p2p/finality_live.rs` | Nouveau champ `cast_memo` = base anti-slashing (C1) ; borne d'époque à l'ingestion et éviction par récence (H2) ; curseur incrémental de `observe_chain` (M2). |
| `src-tauri/src/p2p/gossip.rs` | Identifiant d'enveloppe calculé sur les octets canoniques signés (H3). |
| `src-tauri/src/p2p/dispatcher.rs` | Dedup déplacé après la vérification de signature + identifiant recalculé (H1) ; cartes indexées par condensé court et plafond de rapporteurs (H6). |
| `src-tauri/src/p2p/fork_heal.rs` | Éviction par utilité, plafonds par index et par expéditeur (H5). |
| `src-tauri/src/rpc.rs` | Jeton d'authentification, garde CSRF, délai de lecture, sémaphore de connexions (C4, M1). |
| `src-tauri/src/explorer.html` | Échappement de tous les champs interpolés (H7). |
| `src-tauri/src/security/crypto_agility.rs` | Inventaire cryptographique honnête (M3). |
| `src-tauri/src/commands/identity.rs` | Invalidation du déverrouillage biométrique à la création/restauration (M4). |
| `src-tauri/src/p2p/gossip.rs` (const) | `TORUS_PROTOCOL_VERSION` 6→7. |

---

# LOT A — rupture de protocole (v7)

## Task 1 : C2 — un expéditeur synthétique ne peut plus créer de monnaie

**Files:**
- Modify: `src-tauri/src/p2p/ledger/validation.rs` (nouvelle fonction + appel dans
  `validate_block_against_prev`)
- Modify: `src-tauri/src/p2p/ledger/reorg.rs:73-90` (exclusion au seal)
- Test: `src-tauri/src/p2p/ledger/tests.rs`

**Interfaces:**
- Produces: `Ledger::illegal_synthetic_indices(txs: &[Transaction], miner: &str) -> Vec<usize>` —
  indices des transactions dont l'expéditeur est synthétique sans être l'unique coinbase légitime.
  Consommée par la tâche 2 (même point de câblage) et par `seal_block_at`.

- [ ] **Step 1: Write the failing test**

Dans `src-tauri/src/p2p/ledger/tests.rs`, à la suite de `cover1_uncovered_stake_block_rejected` :

```rust
    /// **C2 — an unsigned `Transfer` from the synthetic `NETWORK` sender mints
    /// coins out of nothing.** `verify_tx` exempts synthetic senders, coverage
    /// skips them, and the emission cap only sums `TxType::Mining` — so before
    /// this rule a non-Mining synthetic credit was invisible to every guard.
    #[test]
    fn c2_synthetic_transfer_block_rejected() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        // No signature needed: `verify_tx` returns Ok(true) for "NETWORK".
        let mut evil = Transaction::new(
            "NETWORK".to_string(),
            alice.clone(),
            100_000_000 * MICRO,
            TxType::Transfer,
        );
        evil.timestamp = "2026-01-01T00:00:00Z".to_string();
        let block = Ledger::forge_block_at(
            tip.index + 1, &tip.hash, "2026-01-01T00:00:01Z", &alice, vec![evil],
        );

        let err = l
            .integrate_remote_block(block)
            .expect_err("a synthetic-sender Transfer must be rejected");
        assert!(err.contains("expéditeur synthétique"), "got: {err}");
        assert_eq!(l.balance_of(&alice), 10 * MICRO, "no coins were minted");
    }

    /// The legitimate coinbase — the single `Mining` tx `NETWORK → block.miner` —
    /// must still pass, otherwise the rule would halt the chain.
    #[test]
    fn c2_legitimate_coinbase_still_accepted() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        assert_eq!(l.chain_height(), 2, "the coinbase path is unaffected");
        assert_eq!(l.balance_of(&alice), 10 * MICRO);
    }
```

Si `Transaction::new` n'a pas cette signature, ouvrir `src-tauri/src/p2p/ledger_types.rs` et
construire la structure littéralement, en laissant `signature` vide et `pq_public_key` à `None`.

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml c2_synthetic_transfer_block_rejected -- --nocapture
```

Attendu : ÉCHEC — `integrate_remote_block` renvoie `Ok`, donc `expect_err` panique.

- [ ] **Step 3: Write minimal implementation**

Dans `validation.rs`, à côté de `uncovered_tx_indices` :

```rust
    /// C2 — a **synthetic sender** (`NETWORK`, `ESCROW`) is exempt from signature
    /// verification and from coverage, so it must be structurally confined to the
    /// one place it is legitimate: the single `Mining` coinbase credited to the
    /// block's own sealer, already bounded by EMIT-1 and the emission cap.
    /// Anything else — a `Transfer`, a `Stake`, an `ESCROW` release — would be
    /// unsigned, uncovered AND invisible to the 100M cap, i.e. free money.
    /// `ESCROW` has no live producer (`escrow_release_to` has no caller outside
    /// tests), so it is never a legal sender in a sealed block.
    pub(super) fn illegal_synthetic_indices(txs: &[Transaction], miner: &str) -> Vec<usize> {
        let mut bad = Vec::new();
        for (i, tx) in txs.iter().enumerate() {
            let legal_coinbase = tx.tx_type == TxType::Mining
                && tx.from == "NETWORK"
                && tx.to == miner;
            if Self::is_synthetic_sender(&tx.from) && !legal_coinbase {
                bad.push(i);
            }
        }
        bad
    }
```

Puis dans `validate_block_against_prev`, juste après le bloc EMIT-1 (après le contrôle
`reward.to != block.miner`) :

```rust
        // C2 — no synthetic sender outside the one legitimate coinbase.
        let illegal = Self::illegal_synthetic_indices(&block.transactions, &block.miner);
        if let Some(&i) = illegal.first() {
            return Err(format!(
                "bloc rejeté : tx {} — expéditeur synthétique {} hors coinbase (C2)",
                i,
                short(&block.transactions[i].from, 12)
            ));
        }
```

Et dans `reorg.rs`, au seal, étendre la chaîne d'exclusions (lignes 73-85) :

```rust
        let uncovered = Self::uncovered_tx_indices(&onchain_before, &candidate);
        let unbound = Self::binding_violations(&bindings_before, &candidate);
        let bad_slashes = self.invalid_slash_indices(&candidate);
        let bad_synthetic = Self::illegal_synthetic_indices(&candidate, miner);
        let txs = if uncovered.is_empty()
            && unbound.is_empty()
            && bad_slashes.is_empty()
            && bad_synthetic.is_empty()
        {
```

et inclure `bad_synthetic` dans la collecte des indices exclus :

```rust
                uncovered.into_iter().chain(unbound).chain(bad_slashes).chain(bad_synthetic).collect();
```

Vérifier le nom exact du paramètre `miner` dans `seal_block_at` et l'utiliser tel quel.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml c2_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml
```

Attendu : les deux nouveaux tests passent, et la suite complète reste verte (451 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/p2p/ledger/validation.rs src-tauri/src/p2p/ledger/reorg.rs src-tauri/src/p2p/ledger/tests.rs
git commit -m "fix(ledger): C2 — confine synthetic senders to the single legitimate coinbase"
```

---

## Task 2 : C3 — un `Unstake` est confronté à l'enjeu réellement bondé

**Files:**
- Modify: `src-tauri/src/p2p/ledger/validation.rs`
- Modify: `src-tauri/src/p2p/ledger/stake.rs:51-61`
- Modify: `src-tauri/src/p2p/ledger/reorg.rs` (seal)
- Test: `src-tauri/src/p2p/ledger/tests.rs`

**Interfaces:**
- Consumes: `bonded_before: &HashMap<String, u64>` — **déjà** passé à
  `validate_block_against_prev` par PROPOSER-1, aucune nouvelle plomberie n'est nécessaire.
- Produces: `Ledger::overdrawn_unstake_indices(bonded_before: &HashMap<String, u64>, txs: &[Transaction]) -> Vec<usize>`

- [ ] **Step 1: Write the failing test**

```rust
    /// **C3 — an `Unstake` for more than the bonded stake fabricates unbonding
    /// stake that matures into spendable coins.** The signature is genuine (the
    /// attacker signs for its own account), so the rejection must come from the
    /// bonded-amount rule, not from crypto.
    #[test]
    fn c3_overdrawn_unstake_block_rejected() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        // Alice bonded nothing, yet unstakes 10_000 QTA.
        let evil = l
            .build_signed_tx_at(&alice, Ledger::STAKE_SINK, 10_000 * MICRO, TxType::Unstake,
                &crypto_a, "2026-01-01T00:00:00Z".into(), false)
            .expect("an over-unstake still produces a valid signature");
        let block = Ledger::forge_block_at(
            tip.index + 1, &tip.hash, "2026-01-01T00:00:01Z", &alice, vec![evil],
        );

        let err = l
            .integrate_remote_block(block)
            .expect_err("an unstake exceeding bonded stake must be rejected");
        assert!(err.contains("enjeu bondé"), "got: {err}");
    }

    /// A legitimate stake→unstake cycle must still pass, sequentially, inside one
    /// block: the `Stake` bonds first, so the `Unstake` that follows is covered.
    #[test]
    fn c3_sequential_stake_then_unstake_accepted() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        let stake = l
            .build_signed_tx_at(&alice, Ledger::STAKE_SINK, 5 * MICRO, TxType::Stake,
                &crypto_a, "2026-01-01T00:00:00Z".into(), false)
            .expect("stake tx");
        let unstake = l
            .build_signed_tx_at(&alice, Ledger::STAKE_SINK, 5 * MICRO, TxType::Unstake,
                &crypto_a, "2026-01-01T00:00:01Z".into(), false)
            .expect("unstake tx");
        let block = Ledger::forge_block_at(
            tip.index + 1, &tip.hash, "2026-01-01T00:00:02Z", &alice, vec![stake, unstake],
        );

        l.integrate_remote_block(block)
            .expect("stake then unstake of the same amount is legal in one block");
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml c3_overdrawn_unstake_block_rejected
```

Attendu : ÉCHEC — le bloc est accepté aujourd'hui.

- [ ] **Step 3: Write minimal implementation**

Dans `validation.rs` :

```rust
    /// C3 — an `Unstake` moves bonded stake into the unbonding pool, where it
    /// matures into spendable coins. `uncovered_tx_indices` exempts it (there is
    /// no spendable debit to cover) and nothing else checked the amount, so a
    /// signed `Unstake` from an account with zero bonded stake fabricated coins
    /// that every node then accepted. The rule mirrors COVER: sequential over a
    /// running bonded map, so a `Stake` earlier in the same block counts.
    pub(super) fn overdrawn_unstake_indices(
        bonded_before: &HashMap<String, u64>,
        txs: &[Transaction],
    ) -> Vec<usize> {
        let mut running: HashMap<String, u64> = bonded_before.clone();
        let mut bad = Vec::new();
        for (i, tx) in txs.iter().enumerate() {
            match tx.tx_type {
                TxType::Stake => {
                    let e = running.entry(tx.from.clone()).or_insert(0);
                    *e = e.saturating_add(tx.amount);
                }
                TxType::Unstake => {
                    let have = running.get(&tx.from).copied().unwrap_or(0);
                    if tx.amount > have {
                        bad.push(i);
                        continue; // excluded/rejected → do NOT move the running bond
                    }
                    let e = running.entry(tx.from.clone()).or_insert(0);
                    *e = e.saturating_sub(tx.amount);
                }
                TxType::Slash => {
                    // The slash destroys bonded stake first (LIVE-3B); the exact
                    // split is re-verified by `verify_block_slashes`. Here we only
                    // need the bonded weight to shrink so a later Unstake cannot
                    // draw on already-burned stake.
                    let consumed_unbonding: u64 =
                        tx.slash_unbonding.iter().flatten().map(|e| e.amount).sum();
                    let bonded_take = tx.amount.saturating_sub(consumed_unbonding);
                    let e = running.entry(tx.from.clone()).or_insert(0);
                    *e = e.saturating_sub(bonded_take);
                }
                _ => {}
            }
        }
        bad
    }
```

Dans `validate_block_against_prev`, immédiatement après le contrôle C2 de la tâche 1 :

```rust
        // C3 — no unstake beyond the bonded stake as of the parent.
        let overdrawn = Self::overdrawn_unstake_indices(bonded_before, &block.transactions);
        if let Some(&i) = overdrawn.first() {
            return Err(format!(
                "bloc rejeté : tx {} — retrait d'enjeu supérieur à l'enjeu bondé de {} (C3)",
                i,
                short(&block.transactions[i].from, 12)
            ));
        }
```

Dans `stake.rs`, clamper l'entrée créée pour que l'état vivant ne puisse pas diverger même si un
chemin futur oubliait la règle :

```rust
                TxType::Unstake => {
                    let bonded = self.staked.entry(tx.from.clone()).or_insert(0);
                    // C3: the unbonding entry may never exceed what was actually
                    // bonded — the validator rejects the over-unstake, this clamp
                    // makes the apply path unable to fabricate stake regardless.
                    let moved = tx.amount.min(*bonded);
                    *bonded = bonded.saturating_sub(moved);
                    if *bonded == 0 {
                        self.staked.remove(&tx.from);
                    }
                    if moved > 0 {
                        self.unbonding.entry(tx.from.clone()).or_default().push(UnbondEntry {
                            amount: moved,
                            unlock_height: block.index.saturating_add(UNBONDING_PERIOD_BLOCKS),
                            tx_hash: tx.hash.clone(),
                        });
                    }
                }
```

Dans `reorg.rs` au seal, ajouter la liste comme en tâche 1 — la carte bondée as-of-parent est
obtenue par la même fonction que PROPOSER-1 utilise (`staked_before` / `validator_stakes()` selon
le chemin ; reprendre l'appel déjà présent dans `seal_block_at`).

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml c3_
cargo test --manifest-path src-tauri/Cargo.toml
```

Attendu : les deux nouveaux tests passent ; **en particulier** le cycle stake/unstake existant
(`ledger/tests.rs` ligne ~1554) doit rester vert — s'il casse, la carte bondée est lue au mauvais
moment.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/p2p/ledger/
git commit -m "fix(ledger): C3 — reject an unstake beyond the bonded stake, clamp the unbonding entry"
```

---

## Task 3 : H4 — la relecture de solde consomme les slashes

**Files:**
- Modify: `src-tauri/src/p2p/ledger/validation.rs:344-393` (`onchain_spendable_before`)
- Test: `src-tauri/src/p2p/ledger/tests.rs`

**Interfaces:**
- Consumes: `Transaction::slash_unbonding: Option<Vec<ConsumedUnbond>>` (champs `amount`,
  `unlock_height`, `tx_hash`), déjà défini par LIVE-3B.

- [ ] **Step 1: Write the failing test**

```rust
    /// **H4 — the coverage replay must consume what a `Slash` destroyed.** The
    /// live path deletes the slashed unbonding entries, the replay kept them, so
    /// at maturation the shared coverage validator credited coins the chain had
    /// already burned. Assert the two views agree.
    #[test]
    fn h4_slash_consumption_is_replayed_by_coverage() {
        // The scenario is the one LIVE-3B already builds: stake → unstake → slash
        // consuming that unbonding entry. Reuse its constructors verbatim.
        let (mut l, offender) = live3b_stake_unstake_then_slash();

        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();
        let replayed = l.onchain_spendable_before(&tip);
        let seen = replayed.get(&offender).copied().unwrap_or(0);

        assert_eq!(
            seen,
            l.balance_of(&offender) as i128,
            "the coverage replay must agree with the live cache for a slashed account"
        );
        assert_eq!(
            seen, 0,
            "burned unbonding stake must never mature back into spendable coins"
        );
    }
```

Localiser le scénario à factoriser :

```bash
grep -n "slash_unbonding" src-tauri/src/p2p/ledger/tests.rs
```

Extraire `live3b_stake_unstake_then_slash() -> (Ledger, String)` du test LIVE-3B existant qui
construit déjà cette séquence, plutôt que d'en réécrire un — et le nommer ainsi, la tâche 4 s'appuie
sur le même scénario. Le déverrouillage est à `block.index + 10_080` : reprendre la manière dont le
test de reorg LIVE-3B avance jusqu'à l'échéance, ne pas sceller dix mille blocs.

Reprendre les constructeurs de slash exacts des tests LIVE-3B existants (`grep -n "slash_unbonding"
src-tauri/src/p2p/ledger/tests.rs`) plutôt que d'en inventer. Si `balance_snapshot()` n'existe pas,
itérer sur les adresses de test explicitement.

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml h4_slash_consumption_is_replayed_by_coverage
```

Attendu : ÉCHEC — l'adresse de l'offenseur est créditée dans la relecture et pas dans le cache.

- [ ] **Step 3: Write minimal implementation**

Remplacer le `continue` inconditionnel sur `Slash` (`validation.rs:366-368`) par la consommation
réelle. Le pool local est `HashMap<String, Vec<(u64, u64)>>` (montant, hauteur de déverrouillage) et
n'indexe pas le `tx_hash` ; l'étendre en `Vec<(u64, u64, String)>` pour pouvoir apparier :

```rust
        // pk → list of (amount, unlock_height, origin_tx_hash) still locked.
        let mut unbonding: HashMap<String, Vec<(u64, u64, String)>> = HashMap::new();
```

à l'insertion :

```rust
                    unbonding.entry(tx.from.clone()).or_default().push((
                        tx.amount,
                        block.index.saturating_add(UNBONDING_PERIOD_BLOCKS),
                        tx.hash.clone(),
                    ));
```

et à la place du `continue` :

```rust
                // LIVE-3B / H4: a Slash destroys locked stake — including entries
                // already in the unbonding pool. The live path removes them
                // (`apply_block_stake_effects`), so the replay must too, or it
                // will credit burned coins back at maturation and the two views of
                // the same chain diverge for good.
                if matches!(tx.tx_type, TxType::Slash) {
                    if let Some(consumed) = &tx.slash_unbonding {
                        if let Some(list) = unbonding.get_mut(&tx.from) {
                            for c in consumed {
                                if let Some(e) = list.iter_mut().find(|e| e.2 == c.tx_hash) {
                                    e.0 = e.0.saturating_sub(c.amount);
                                }
                            }
                            list.retain(|e| e.0 > 0);
                        }
                        unbonding.retain(|_, v| !v.is_empty());
                    }
                    continue;
                }
```

Adapter la boucle de maturation aux triplets (`entries.retain(|(amount, unlock, _)| …)`).

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml h4_
cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/p2p/ledger/validation.rs src-tauri/src/p2p/ledger/tests.rs
git commit -m "fix(ledger): H4 — coverage replay consumes slashed unbonding entries"
```

---

## Task 4 : H8 — l'annulation d'un bloc est l'exact inverse de son application

**Files:**
- Modify: `src-tauri/src/p2p/ledger/stake.rs:141` (`revert_block_stake_effects`)
- Test: `src-tauri/src/p2p/ledger/tests.rs`

- [ ] **Step 1: Write the failing test**

```rust
    /// **H8 — revert must be the exact inverse of apply.** With a `Stake` and an
    /// `Unstake` of the same account in one block, the per-tx operations do not
    /// commute (`saturating_sub` + the zero-collapse), so reverting forward left
    /// fabricated bonded stake behind. Apply then revert must be a no-op.
    #[test]
    fn h8_revert_stake_effects_is_exact_inverse() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        let stake = l
            .build_signed_tx_at(&alice, Ledger::STAKE_SINK, 5 * MICRO, TxType::Stake,
                &crypto_a, "2026-01-01T00:00:00Z".into(), false).expect("stake");
        let unstake = l
            .build_signed_tx_at(&alice, Ledger::STAKE_SINK, 5 * MICRO, TxType::Unstake,
                &crypto_a, "2026-01-01T00:00:01Z".into(), false).expect("unstake");
        let block = Ledger::forge_block_at(
            tip.index + 1, &tip.hash, "2026-01-01T00:00:02Z", &alice, vec![stake, unstake],
        );

        let before = l.validator_stakes();
        l.apply_block_stake_effects(&block);
        l.revert_block_stake_effects(&block);
        assert_eq!(l.validator_stakes(), before, "apply∘revert must be identity");
    }
```

Si `apply_block_stake_effects` / `revert_block_stake_effects` sont `pub(super)`, placer ce test dans
le module de tests du dossier `ledger/` (il l'est déjà — `tests.rs` fait `use super::*`).

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml h8_revert_stake_effects_is_exact_inverse
```

Attendu : ÉCHEC — `staked[alice]` vaut 5 µQTA × 10⁶ au lieu de rien.

- [ ] **Step 3: Write minimal implementation**

Une seule ligne dans `revert_block_stake_effects` :

```rust
        for tx in block.transactions.iter().rev() {
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml h8_
cargo test --manifest-path src-tauri/Cargo.toml
```

Vérifier en particulier que les tests de reorg LIVE-3B (restauration exacte des entrées consommées)
restent verts — ils dépendent de l'ordre.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/p2p/ledger/stake.rs src-tauri/src/p2p/ledger/tests.rs
git commit -m "fix(ledger): H8 — revert block stake effects in reverse order"
```

---

## Task 5 : C1 — base anti-slashing, un validateur honnête ne s'auto-équivoque plus

**Files:**
- Modify: `src-tauri/src/p2p/finality_live.rs` (champ `cast_memo`, accesseurs, `build_vote_to_cast`)
- Modify: `src-tauri/src/p2p/state_persistence.rs` (persistance du mémo)
- Test: `src-tauri/src/p2p/finality_live.rs` (module `tests`)

**Interfaces:**
- Produces: `FinalityTracker::remember_cast(&mut self, vote: &Vote)` et
  `FinalityTracker::cast_for_epoch(&self, epoch: u64) -> Option<&Vote>`.
- Consumes: `mining_loop.rs` appelle `remember_cast` juste après avoir diffusé le vote.

- [ ] **Step 1: Write the failing test**

Dans le module `tests` de `finality_live.rs` :

```rust
    /// **C1 — the cast path must never sign two different votes for one target
    /// epoch.** Reproduces the reorg trigger: the checkpoint at the boundary
    /// height changes hash while the height stays put, which today produces a
    /// second, distinct vote for the same epoch — an equivocation proof against
    /// the node's own key, punished by a full stake burn.
    #[test]
    fn c1_cast_never_equivocates_on_one_target_epoch() {
        let (mut ledger, crypto, mut tracker) = bonded_validator_at_epoch_boundary();

        let first = build_vote_to_cast(&ledger, &tracker, &crypto)
            .expect("a bonded validator at a boundary votes");
        tracker.remember_cast(&first);

        // Same height, different tip hash (equal-height lexicographic tie-break).
        replace_tip_with_competing_block(&mut ledger);

        let second = build_vote_to_cast(&ledger, &tracker, &crypto);
        assert!(
            second.is_none(),
            "a second, distinct vote for epoch {} would be a self-slash",
            first.target.epoch
        );
    }

    /// The memo must not freeze the validator: a strictly later target epoch is
    /// still votable.
    #[test]
    fn c1_memo_still_allows_the_next_epoch() {
        let (mut ledger, crypto, mut tracker) = bonded_validator_at_epoch_boundary();
        let first = build_vote_to_cast(&ledger, &tracker, &crypto).expect("first vote");
        tracker.remember_cast(&first);

        advance_one_full_epoch(&mut ledger);

        let next = build_vote_to_cast(&ledger, &tracker, &crypto)
            .expect("the next epoch boundary must still be votable");
        assert!(next.target.epoch > first.target.epoch);
    }
```

Écrire les trois helpers (`bonded_validator_at_epoch_boundary`, `replace_tip_with_competing_block`,
`advance_one_full_epoch`) dans le même module, en s'appuyant sur `FinalityTracker::with_epoch_len`
avec un `epoch_len` de 2 pour ne pas sceller 32 blocs — c'est exactement la raison d'être de ce
constructeur, documentée sur le champ `epoch_len`.

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml c1_cast_never_equivocates
```

Attendu : ÉCHEC — `second` est `Some(vote)` avec la même `target.epoch`.

- [ ] **Step 3: Write minimal implementation**

Ajouter le champ à la structure :

```rust
    /// C1 — anti-slashing memo: for each target epoch, the exact vote this node
    /// already signed. `build_vote_to_cast` refuses to emit anything that is not
    /// byte-identical to it, so a tip reorg at a boundary — or a late certificate
    /// advancing `justified` — can never turn an honest validator into its own
    /// attacker. This is the standard Ethereum-client slashing-protection DB;
    /// without it the 60 s cast tick re-derives (source, target) from live state
    /// and equivocates against itself.
    cast_memo: BTreeMap<u64, Vote>,
```

L'initialiser à `BTreeMap::new()` dans `with_caps`, puis :

```rust
    /// C1 — record the vote this node just cast, so the next tick cannot sign a
    /// different one for the same target epoch.
    pub fn remember_cast(&mut self, vote: &Vote) {
        self.cast_memo.insert(vote.target.epoch, vote.clone());
    }

    /// C1 — the vote already cast for `epoch`, if any.
    pub fn cast_for_epoch(&self, epoch: u64) -> Option<&Vote> {
        self.cast_memo.get(&epoch)
    }
```

Dans `build_vote_to_cast`, juste après le calcul de `target_epoch` et avant de construire le vote :

```rust
    // C1 — if we already cast for this target epoch, re-emit that exact vote or
    // nothing at all. Re-emitting the identical vote is safe (`detect_fault`
    // short-circuits on an identical link); emitting a *different* one for the
    // same epoch is a double vote and burns our whole stake.
    if let Some(prior) = tracker.cast_for_epoch(target_epoch) {
        let same_target = prior.target == target;
        let same_source = prior.source == source;
        return if same_target && same_source { Some(prior.clone()) } else { None };
    }
```

Attention à l'ordre : `source` et `target` doivent être calculés avant ce bloc. Déplacer le calcul
de `source` au-dessus si nécessaire, ce qui est sans effet sur le reste.

Dans `mining_loop.rs`, après la diffusion du vote (autour de la ligne 234, là où le nœud auto-ingère
son propre vote), ajouter :

```rust
            tracker.remember_cast(&vote);
```

en respectant l'ordre de verrous existant : le mémo vit dans le même `RwLock` que le reste du
tracker, donc aucun nouveau verrou n'est introduit.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml c1_
cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Persist the memo across restarts**

Le mémo doit survivre au redémarrage, sinon un nœud qui redémarre à une frontière d'époque
recommence à zéro et s'équivoque. Dans `state_persistence.rs`, sérialiser `cast_memo` avec l'état de
finalité déjà persisté (chercher la clé du snapshot de finalité, y ajouter le champ), et le
restaurer au démarrage. Ajouter le test :

```rust
    /// C1 — the anti-slashing memo is worthless if a restart forgets it.
    #[test]
    fn c1_memo_survives_a_snapshot_round_trip() {
        let (ledger, crypto, mut tracker) = bonded_validator_at_epoch_boundary();
        let v = build_vote_to_cast(&ledger, &tracker, &crypto).expect("vote");
        tracker.remember_cast(&v);

        let blob = tracker.to_snapshot();
        let restored = FinalityTracker::from_snapshot(&blob).expect("restore");
        assert_eq!(
            restored.cast_for_epoch(v.target.epoch).map(|x| x.signature.clone()),
            Some(v.signature.clone()),
            "the memo must survive a restart"
        );
    }
```

Si `to_snapshot`/`from_snapshot` n'existent pas sous ce nom, reprendre les noms réels utilisés par
`state_persistence.rs` pour l'état de finalité (`grep -n "finality" src-tauri/src/p2p/state_persistence.rs`).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/p2p/finality_live.rs src-tauri/src/p2p/mining_loop.rs src-tauri/src/p2p/state_persistence.rs
git commit -m "fix(finality): C1 — persisted anti-slashing memo, an honest validator cannot self-equivocate"
```

---

## Task 6 : H2 — le pool de certificats ne s'évince plus à l'envers

**Files:**
- Modify: `src-tauri/src/p2p/finality_live.rs:279` (éviction) et le chemin d'ingestion des votes
- Test: `src-tauri/src/p2p/finality_live.rs`

- [ ] **Step 1: Write the failing test**

```rust
    /// **H2 — a 1 µQTA validator must not be able to halt finality.** Epoch is
    /// attacker-chosen (`link_well_formed` only checks boundary + ordering), and
    /// the pool evicted the LOWEST BTreeMap key — i.e. the honest, current-epoch
    /// link — while absurd high-epoch links survived.
    #[test]
    fn h2_absurd_epoch_votes_are_rejected_at_ingest() {
        let (ledger, _crypto, mut tracker) = bonded_validator_at_epoch_boundary();
        let far = signed_vote_at_epoch(u64::MAX / 64);
        let res = tracker.ingest_vote(&ledger, far);
        assert!(res.is_err(), "a vote attesting an epoch the chain never reached is invalid");
    }

    #[test]
    fn h2_pool_eviction_keeps_the_newest_links() {
        let (ledger, _crypto, mut tracker) = tracker_with_pool_cap(2);
        ingest_link_at_epoch(&mut tracker, &ledger, 1);
        ingest_link_at_epoch(&mut tracker, &ledger, 2);
        ingest_link_at_epoch(&mut tracker, &ledger, 3);
        assert!(tracker.has_pending_link_for_epoch(3), "the newest link must survive eviction");
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml h2_
```

- [ ] **Step 3: Write minimal implementation**

Borne d'époque à l'ingestion, avant l'insertion dans le pool :

```rust
        // H2 — a vote can never legitimately attest an epoch the chain has not
        // reached. Without this bound the epoch is attacker-chosen, and since the
        // pool is a BTreeMap keyed by (epoch, height, hash), absurd epochs sort
        // above every honest link and survive eviction while the real one is
        // dropped — a permanent finality halt bought for 1 µQTA.
        const EPOCH_SLACK: u64 = 2;
        let chain_epoch = epoch_of_height(
            ledger.chain_height().saturating_sub(1),
            self.epoch_len,
        );
        if vote.target.epoch > chain_epoch.saturating_add(EPOCH_SLACK) {
            return Err(format!(
                "vote rejeté : époque cible {} au-delà de l'époque courante {} (H2)",
                vote.target.epoch, chain_epoch
            ));
        }
```

Éviction par récence plutôt que par ordre de clé — remplacer `self.pool.keys().next()` par le
maintien d'une file d'insertion :

```rust
    /// H2 — insertion order of the pending links, so eviction drops the *stalest
    /// inserted* link rather than the lowest BTreeMap key (which the attacker
    /// controls through the epoch field).
    pool_order: VecDeque<Link>,
```

et à l'éviction :

```rust
        while self.pool.len() > self.max_pending_links {
            match self.pool_order.pop_front() {
                Some(oldest) => { self.pool.remove(&oldest); }
                None => break,
            }
        }
```

en poussant chaque nouveau lien dans `pool_order` à l'insertion, et en le retirant quand son
certificat est appliqué.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml h2_
cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/p2p/finality_live.rs
git commit -m "fix(finality): H2 — bound the attested epoch and evict pending links by recency"
```

---

## Task 7 : H1 + H3 — identifiant d'enveloppe canonique et dedup après signature

**Files:**
- Modify: `src-tauri/src/p2p/gossip.rs:484` (`build_signed_envelope`)
- Modify: `src-tauri/src/p2p/dispatcher.rs:426-432` et `:463`
- Test: `src-tauri/src/p2p/security_tests.rs`

**Interfaces:**
- Produces: `gossip::envelope_id(sender: &str, nonce: u64, timestamp: i64, payload: &[u8]) -> String`
  — le BLAKE3 des octets canoniques signés, seule définition de l'identifiant.

- [ ] **Step 1: Write the failing test**

```rust
    /// **H3 — two senders emitting the same payload must not collide.** The ID
    /// was BLAKE3(payload) alone, so two nodes requesting the same chain range
    /// shared one dedup slot and the second request was silently dropped —
    /// a node that can never sync.
    #[test]
    fn h3_same_payload_from_two_senders_has_distinct_ids() {
        let a = envelope_id("sender-a", 1, 1_700_000_000, b"{\"RequestChain\":{}}");
        let b = envelope_id("sender-b", 1, 1_700_000_000, b"{\"RequestChain\":{}}");
        assert_ne!(a, b, "the dedup key must bind the sender");
    }

    /// **H1 — the dedup key must not be attacker-chosen.** An envelope whose `id`
    /// does not equal the canonical digest is rejected before it can poison the
    /// LRU.
    #[test]
    fn h1_forged_envelope_id_is_rejected() {
        let mut env = signed_test_envelope();
        env.id = "deadbeef".repeat(8);
        let err = validate_envelope_id(&env).expect_err("a forged id must be rejected");
        assert!(err.contains("identifiant"), "got: {err}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml h1_ h3_
```

- [ ] **Step 3: Write minimal implementation**

Dans `gossip.rs` :

```rust
/// H3 — the dedup identifier is the BLAKE3 of the **canonical signable bytes**,
/// i.e. exactly what the signature covers (sender + nonce + timestamp + payload).
/// Deriving it from the payload alone made two senders — or one sender retrying —
/// share a single LRU slot network-wide and forever, which silently killed chain
/// sync retries and the Ping/Pong heartbeat. Anti-replay is already carried by the
/// per-sender monotonic nonce and the freshness window, so binding the ID to the
/// full pre-image costs nothing and closes both H1 and H3.
pub fn envelope_id(sender: &str, nonce: u64, timestamp: i64, payload: &[u8]) -> String {
    let full = signable_envelope_bytes(sender, nonce, timestamp, payload);
    hex::encode(blake3::hash(&full).as_bytes())
}
```

et remplacer le calcul de `id` dans `build_signed_envelope` par cet appel.

Dans `dispatcher.rs`, ajouter la vérification et **déplacer** `mark_seen` :

```rust
/// H1 — the ID is now a pure function of the signed pre-image, so an envelope
/// carrying any other value is malformed. Checked before the LRU is touched.
fn validate_envelope_id(env: &GossipEnvelope) -> Result<(), String> {
    let expected = crate::p2p::gossip::envelope_id(
        &env.sender, env.nonce, env.timestamp, &env.payload_bytes(),
    );
    if env.id != expected {
        return Err("identifiant d'enveloppe non canonique".to_string());
    }
    Ok(())
}
```

Dans `dispatch_incoming` : garder à l'étape ④ une **sonde en lecture seule** (`if seen.contains(&env.id) { return; }`)
pour le délestage précoce des retransmissions, appeler `validate_envelope_id` avant elle, et
déplacer l'insertion `mark_seen(...)` **après** `verify_envelope_signature` (ligne 463). Mettre à
jour le commentaire des lignes 435-436, qui documente aujourd'hui le comportement inverse.

Adapter `payload_bytes()` au nom réel du champ sérialisé (`grep -n "payload" src-tauri/src/p2p/gossip.rs`).

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml h1_ h3_
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --test p2p_integration
```

Le test d'intégration deux-nœuds est ici le vrai juge : il échange du gossip réel.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/p2p/gossip.rs src-tauri/src/p2p/dispatcher.rs src-tauri/src/p2p/security_tests.rs
git commit -m "fix(gossip): H1+H3 — canonical envelope id, dedup only after signature verification"
```

---

## Task 8 : bump du protocole et mise à jour de la documentation

**Files:**
- Modify: `src-tauri/src/p2p/gossip.rs` (`TORUS_PROTOCOL_VERSION`)
- Modify: `CLAUDE.md`
- Modify: `docs/audit/AUDIT-INTERNE-2026-07-25.md` (statut des constats fermés)

- [ ] **Step 1: Bump the constant**

```rust
/// 6→7 (AUDIT-2026-07-25) : C2 (expéditeur synthétique confiné), C3 (unstake borné
/// par l'enjeu bondé), H2 (époque de vote bornée), H1/H3 (identifiant d'enveloppe
/// canonique) changent les règles d'admission — un nœud v6 et un nœud v7 ne
/// valident pas le même ensemble de blocs et d'enveloppes.
pub const TORUS_PROTOCOL_VERSION: u32 = 7;
```

- [ ] **Step 2: Correct the stale documentation the audit found**

Dans `CLAUDE.md` : corriger l'en-tête de version (il annonce v3.11 alors que `Cargo.toml`,
`package.json` et `tauri.conf.json` disent 3.12.0) ; corriger le tableau des messages gossip, qui
liste 9 variantes quand l'énumération en compte **11** (la prose en annonce 10 et ne mentionne pas
`FinalityFault`) ; corriger l'ordre documenté du pipeline de dispatch, qui ne correspond plus au code
après la tâche 7 ; ajouter une ligne d'historique pour ce fork.

- [ ] **Step 3: Full verification**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
```

Attendu : suite verte, clippy silencieux.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/p2p/gossip.rs CLAUDE.md docs/audit/AUDIT-INTERNE-2026-07-25.md
git commit -m "feat(protocol): bump TORUS_PROTOCOL_VERSION 6→7 (audit remediation hard-fork)"
```

---

# LOT B — livrable sans rupture de protocole

## Task 9 : C4 — authentification du RPC monnaie et garde CSRF

**Files:**
- Modify: `src-tauri/src/rpc.rs` (struct `HttpReq`, `read_request`, `handle_conn`)
- Modify: `src-tauri/src/bin/quanta-node.rs` (génération du jeton au démarrage)
- Test: `src-tauri/src/rpc.rs` (module `tests`)

**Interfaces:**
- Produces: `rpc::RpcAuth { token: String }`, écrit dans `<data_dir>/.cookie` au démarrage.
- `HttpReq` gagne `headers: Vec<(String, String)>` et une méthode
  `header(&self, name: &str) -> Option<&str>` (comparaison insensible à la casse).

- [ ] **Step 1: Write the failing test**

```rust
    /// **C4 — a mutating method requires the cookie token.** Without it the
    /// documented `quanta-node --mine` wallet could be drained by any web page the
    /// operator visited, since a bodyless-preflight CORS simple request reaches
    /// the dispatcher.
    #[test]
    fn c4_mutating_method_requires_auth() {
        let auth = RpcAuth { token: "s3cret".into() };
        let req = req_with(vec![], r#"{"method":"sendtoaddress"}"#);
        assert!(reject_unauthenticated(&req, &auth, false).is_some(), "no token → rejected");

        let ok = req_with(vec![("Authorization", "Bearer s3cret")], r#"{"method":"sendtoaddress"}"#);
        assert!(reject_unauthenticated(&ok, &auth, false).is_none(), "valid token → accepted");
    }

    /// A cross-origin POST is refused even with a valid token — the browser would
    /// attach it automatically in a future cookie-based scheme, so Origin is
    /// checked independently.
    #[test]
    fn c4_cross_origin_post_is_refused() {
        let auth = RpcAuth { token: "s3cret".into() };
        let req = req_with(
            vec![("Authorization", "Bearer s3cret"), ("Origin", "https://evil.example")],
            r#"{"method":"sendtoaddress"}"#,
        );
        assert!(reject_unauthenticated(&req, &auth, false).is_some(), "foreign Origin → rejected");
    }

    /// Read-only methods stay open in `--public` mode, which is the whole point of
    /// that flag.
    #[test]
    fn c4_public_readonly_needs_no_token() {
        let auth = RpcAuth { token: "s3cret".into() };
        let req = req_with(vec![], r#"{"method":"getinfo"}"#);
        assert!(reject_unauthenticated(&req, &auth, true).is_none());
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml c4_
```

Attendu : ÉCHEC de compilation — `RpcAuth` et `reject_unauthenticated` n'existent pas.

- [ ] **Step 3: Write minimal implementation**

Capturer les en-têtes dans `read_request` (elles sont déjà découpées ligne à ligne dans `head`,
il suffit de les collecter au lieu de les jeter), puis :

```rust
/// C4 — the money RPC's authority. A random token written to `<data_dir>/.cookie`
/// at startup (Bitcoin Core's model): anything that can read the file is already
/// inside the trust boundary, and everything else — including a web page the
/// operator happens to open — is not.
pub struct RpcAuth {
    pub token: String,
}

/// Read-only methods, safe without a token. Everything else is mutating or
/// wallet-touching and requires one.
fn is_read_only(method: &str) -> bool {
    matches!(
        method,
        "getinfo" | "getblock" | "getbalance" | "validateaddress" | "getfinalityinfo"
            | "getvalidators" | "getmempool" | "listtransactions" | "gettransaction"
            | "getmultisigaddress"
    )
}

/// Returns `Some(error_message)` when the request must be refused.
fn reject_unauthenticated(req: &HttpReq, auth: &RpcAuth, public: bool) -> Option<String> {
    let method = req.rpc_method();
    if public && is_read_only(&method) {
        return None;
    }
    // CSRF: a browser attaches Origin automatically on cross-origin requests. A
    // same-origin or absent Origin is fine; a foreign one never is.
    if let Some(origin) = req.header("origin") {
        if !origin.is_empty() && !req.is_same_origin(origin) {
            return Some("origine croisée refusée".to_string());
        }
    }
    // Only a genuine JSON client reaches here: requiring the exact Content-Type
    // removes the CORS *simple request* path entirely.
    match req.header("content-type") {
        Some(ct) if ct.trim().starts_with("application/json") => {}
        _ => return Some("Content-Type application/json requis".to_string()),
    }
    let presented = req.header("authorization").unwrap_or("").trim();
    let expected = format!("Bearer {}", auth.token);
    // Constant-time comparison — a timing oracle on a local token is cheap to
    // avoid and expensive to regret.
    let ok = presented.len() == expected.len()
        && presented
            .as_bytes()
            .iter()
            .zip(expected.as_bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0;
    if ok { None } else { Some("authentification requise".to_string()) }
}
```

Câbler dans `handle_conn`, juste avant `dispatch`, en répondant `401` avec le message. Générer le
jeton au démarrage du daemon (32 octets aléatoires en hexadécimal, écrits avec des permissions
`0o600` sur Unix) et l'injecter dans `serve`. L'explorateur embarqué (`explorer.html:105`) envoie
déjà `Content-Type: application/json` ; lui faire lire le jeton n'est pas nécessaire tant qu'il
n'appelle que des méthodes en lecture seule — vérifier la liste qu'il utilise et, si l'une est
mutante, la retirer de l'explorateur.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml c4_
cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Documentation**

Mettre à jour `docs/ops/QUICKSTART.md` : la ligne `QUANTA_WALLET_PASSWORD=… quanta-node --mine` doit
désormais expliquer où trouver le jeton et comment l'envoyer (`-H "Authorization: Bearer $(cat …/.cookie)"`).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/rpc.rs src-tauri/src/bin/quanta-node.rs docs/ops/QUICKSTART.md
git commit -m "fix(rpc): C4 — cookie-token auth, Origin and Content-Type guards on the money RPC"
```

---

## Task 10 : M1 — délai de lecture, plafond de connexions, pas de boucle chaude sur EMFILE

**Files:**
- Modify: `src-tauri/src/rpc.rs:60-90` (`serve`) et `handle_conn`

- [ ] **Step 1: Write the failing test**

```rust
    /// **M1 — a silent client must not park a task and an fd forever.**
    #[tokio::test]
    async fn m1_idle_connection_is_dropped_after_the_timeout() {
        let addr = spawn_test_rpc().await;
        let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        // Send nothing at all, then wait past the read deadline.
        tokio::time::sleep(std::time::Duration::from_secs(RPC_READ_TIMEOUT_SECS + 1)).await;
        let mut buf = [0u8; 1];
        let n = { let mut s = stream; s.read(&mut buf).await.unwrap_or(0) };
        assert_eq!(n, 0, "the server must have closed the idle connection");
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml m1_idle_connection
```

- [ ] **Step 3: Write minimal implementation**

```rust
/// M1 — no request may hold a task and a file descriptor indefinitely.
const RPC_READ_TIMEOUT_SECS: u64 = 10;
/// M1 — in-flight connection ceiling. Excess connections are dropped rather than
/// parked, so a slowloris cannot exhaust the process's descriptors and take the
/// mining and gossip tasks down with it.
const RPC_MAX_INFLIGHT: usize = 128;
```

Dans `serve` :

```rust
    let permits = Arc::new(tokio::sync::Semaphore::new(RPC_MAX_INFLIGHT));
```

puis, dans la branche `Ok((stream, _peer))` :

```rust
                Ok((stream, _peer)) => {
                    let Ok(permit) = permits.clone().try_acquire_owned() else {
                        log::debug!("◈ [RPC] connexion refusée — plafond en vol atteint");
                        continue;
                    };
                    let st = state.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        let deadline = std::time::Duration::from_secs(RPC_READ_TIMEOUT_SECS);
                        match tokio::time::timeout(deadline, handle_conn(stream, st, public)).await {
                            Ok(Err(e)) => log::debug!("◈ [RPC] connexion: {e}"),
                            Err(_) => log::debug!("◈ [RPC] connexion expirée"),
                            Ok(Ok(())) => {}
                        }
                    });
                }
```

et, dans la branche d'erreur d'acceptation, casser la boucle chaude :

```rust
                Err(e) => {
                    log::warn!("◈ [RPC] accept: {e}");
                    // EMFILE returns immediately; without this pause the loop
                    // spins at 100% CPU and starves mining and gossip.
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml m1_
cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/rpc.rs
git commit -m "fix(rpc): M1 — read timeout, in-flight cap, backoff on accept errors"
```

---

## Task 11 : H7 — l'explorateur échappe tout ce qu'il affiche

**Files:**
- Modify: `src-tauri/src/explorer.html:177` et toute autre interpolation non échappée

- [ ] **Step 1: Find every unescaped interpolation**

```bash
grep -n 'innerHTML' src-tauri/src/explorer.html
grep -n '${short(' src-tauri/src/explorer.html
```

Chaque `${…}` inséré dans une chaîne assignée à `innerHTML` doit passer par `esc()`.

- [ ] **Step 2: Write the failing test**

L'explorateur n'a pas de suite de tests ; le test est manuel et doit être fait avant la correction
pour prouver la vulnérabilité, en suivant le scénario du rapport : une transaction dont le `to` vaut
`<base href=//a.co>` (dix-huit caractères, donc `short()` la rend verbatim). Vérifier dans un
navigateur que l'élément `<base>` est bien injecté dans le DOM du bloc affiché.

- [ ] **Step 3: Write minimal implementation**

```javascript
      `<div class="row"><div class="rk">${esc(t.tx_type)}</div><div class="rv mono">${esc(short(t.from))} → ${esc(short(t.to))} · ${fmtQ(t.amount)} QTA</div></div>`).join("")
```

Appliquer le même traitement à toutes les autres occurrences relevées à l'étape 1. `esc()` doit
envelopper `short()`, jamais l'inverse — tronquer après échappement couperait une entité HTML en
deux.

- [ ] **Step 4: Verify**

Rejouer le scénario manuel : le champ doit désormais s'afficher comme du texte littéral. Puis :

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

(`explorer.html` est inclus par `include_str!`, donc une erreur de syntaxe casse la compilation.)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/explorer.html
git commit -m "fix(explorer): H7 — escape every interpolated chain-controlled field"
```

---

## Task 12 : H5 — le tampon de réconciliation ne s'épingle plus

**Files:**
- Modify: `src-tauri/src/p2p/fork_heal.rs:148-160` (`offer`) et `purge_dead`
- Test: `src-tauri/src/p2p/fork_heal.rs`

- [ ] **Step 1: Write the failing test**

```rust
    /// **H5 — junk blocks must not be able to switch LIVE-4 off.** A buffer full
    /// of low-index blocks that can never win used to be stable, and from then on
    /// every genuine competing-branch block was refused.
    #[test]
    fn h5_junk_cannot_pin_the_buffer_against_a_real_branch() {
        let mut r = ForkReconciler::with_capacity(8);
        for i in 0..8 {
            r.offer(junk_block_at(1, &format!("junk-{i}")));
        }
        let real = competing_block_at(500, "real-branch");
        r.offer(real.clone());
        assert!(r.contains(&real.hash), "a real competing block must always find a slot");
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml h5_
```

- [ ] **Step 3: Write minimal implementation**

Remplacer la règle « évincer l'index le plus haut, refuser tout index supérieur » par une éviction
par utilité :

```rust
    /// H5 — evict by usefulness, not by index. A block far below our tip can never
    /// root a winning run, so it is the first to go; ties break on insertion age.
    /// The previous rule (drop the highest index, refuse any newcomer at or above
    /// it) made a buffer of cheap low-index junk *stable*, which switched off the
    /// only live caller of `reorg_to_fork`.
    fn evict_one(&mut self, tip_height: u64) {
        let victim = self
            .buffer
            .iter()
            .min_by_key(|(hash, b)| {
                let distance = tip_height.abs_diff(b.index);
                (std::cmp::Reverse(distance), self.insertion_seq.get(*hash).copied().unwrap_or(0))
            })
            .map(|(hash, _)| hash.clone());
        if let Some(h) = victim {
            self.buffer.remove(&h);
            self.insertion_seq.remove(&h);
        }
    }
```

Ajouter en outre un plafond par index et par expéditeur, pour qu'un seul pair ne puisse pas occuper
tout le tampon, et exiger une validation minimale (hash recalculé, racine de Merkle, proposeur bondé
as-of-parent) avant d'accorder une place — la fonction de validation existe déjà,
`validate_block_against_prev` ; ici on n'appelle que ses contrôles sans état.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml h5_
cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/p2p/fork_heal.rs
git commit -m "fix(fork-heal): H5 — evict by usefulness, cap per index and per sender"
```

---

## Task 13 : H6 — les cartes de pairs cessent d'être une bombe mémoire

**Files:**
- Modify: `src-tauri/src/p2p/dispatcher.rs:262-313` (`NonceTracker`)
- Test: `src-tauri/src/p2p/security_tests.rs`

- [ ] **Step 1: Write the failing test**

```rust
    /// **H6 — one target with many reporters must stay bounded.** `prune` tested
    /// the number of *targets*, never the number of reporters, and each reporter
    /// key is a 3904-byte ML-DSA hex string since PQ-ENVELOPE-1.
    #[test]
    fn h6_reporter_set_is_capped_per_target() {
        let mut t = NonceTracker::new();
        let victim = "victim".to_string();
        for i in 0..10_000 {
            t.record_report(&victim, &format!("reporter-{i}"));
        }
        assert!(
            t.reporter_count(&victim) <= MAX_REPORTERS_PER_TARGET,
            "the reporter set must be capped"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml h6_
```

- [ ] **Step 3: Write minimal implementation**

```rust
/// H6 — a ban needs 3 distinct reporters, so keeping more than a handful buys
/// nothing and lets an attacker with cheap fresh ML-DSA keypairs grow the map
/// without ever tripping the per-sender rate limit.
const MAX_REPORTERS_PER_TARGET: usize = 8;

/// H6 — peer maps are keyed by a short digest instead of the full 3904-char
/// ML-DSA hex, which is what `MAX_TRACKED_SENDERS = 100_000` was sized for back
/// when keys were 64-char Ed25519.
fn peer_key(public_key_hex: &str) -> String {
    hex::encode(&blake3::hash(public_key_hex.as_bytes()).as_bytes()[..16])
}
```

Appliquer `peer_key` à toutes les insertions dans `last_nonces`, `report_counts` et les cartes
sœurs, et refuser l'insertion d'un rapporteur supplémentaire une fois
`MAX_REPORTERS_PER_TARGET` atteint.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml h6_
cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/p2p/dispatcher.rs src-tauri/src/p2p/security_tests.rs
git commit -m "fix(dispatcher): H6 — digest-keyed peer maps and a per-target reporter cap"
```

---

## Task 14 : M2 — l'arbre de blocs se construit incrémentalement

**Files:**
- Modify: `src-tauri/src/p2p/finality_live.rs:183` (`observe_chain`)

- [ ] **Step 1: Write the failing test**

```rust
    /// **M2 — observing the chain twice must not redo the whole walk.** The tree
    /// is append-only for a linear chain, so a cursor is exact.
    #[test]
    fn m2_observe_chain_is_incremental() {
        let (mut ledger, _c, mut tracker) = bonded_validator_at_epoch_boundary();
        tracker.observe_chain(&ledger);
        let after_first = tracker.observed_height();
        tracker.observe_chain(&ledger);
        assert_eq!(tracker.observed_height(), after_first, "no re-walk on an unchanged chain");
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml m2_observe_chain_is_incremental
```

Attendu : ÉCHEC de compilation — `observed_height()` n'existe pas.

- [ ] **Step 3: Write minimal implementation**

Ajouter le curseur :

```rust
    /// M2 — highest chain index already folded into `tree`. Re-walking the whole
    /// chain on every incoming vote cost O(height) BTreeMap operations and string
    /// allocations *inside the finality write lock*, which the mining tick also
    /// needs. The tree is append-only on a linear chain, so the cursor is exact.
    observed_height: u64,
```

et borner la boucle à `self.observed_height.max(1)..height`, en mettant à jour le curseur en fin
d'appel. Sur un reorg, `integrate_remote_block` doit remettre le curseur à la hauteur de l'ancêtre
commun — chercher les points où le ledger notifie un reorg et y ajouter
`tracker.reset_observed_to(common_ancestor_index)`.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml m2_
cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/p2p/finality_live.rs
git commit -m "perf(finality): M2 — incremental block-tree observation under the write lock"
```

---

## Task 15 : M3 — l'écran de sécurité dit la vérité

**Files:**
- Modify: `src-tauri/src/security/crypto_agility.rs:38` (`CryptoBOM::current`)

- [ ] **Step 1: Write the failing test**

```rust
    /// **M3 — the in-app security disclosure must match reality.** It advertised
    /// Ed25519 as the signing algorithm and X25519 as "PendingMigration", both
    /// false since PQ-MIG-3B, PQ-ENVELOPE-1 and PQ-TRANSPORT-1. It is rendered to
    /// the user in the Help modal, so this is a zero-fake violation, not dead code.
    #[test]
    fn m3_cbom_reports_the_real_primitives() {
        let bom = CryptoBOM::current();
        assert_eq!(bom.signing.name, "ML-DSA-65");
        assert!(bom.signing.quantum_safe, "the account authority is post-quantum");
        assert_eq!(bom.key_exchange.name, "X25519MLKEM768");
        assert!(bom.key_exchange.quantum_safe, "the transport KEX is hybrid PQ");
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml m3_cbom_reports_the_real_primitives
```

- [ ] **Step 3: Write minimal implementation**

Réécrire `CryptoBOM::current()` avec l'inventaire réel : signature `ML-DSA-65` / `FIPS 204` /
`quantum_safe: true` / actif ; échange de clés `X25519MLKEM768` / hybride / `quantum_safe: true` /
actif ; **plus une entrée distincte** pour l'authentification de transport Ed25519 (identité de nœud
Iroh), honnêtement marquée `quantum_safe: false` et en attente amont — c'est la seule dette
classique restante et la cacher serait le symétrique du mensonge qu'on corrige.

Vérifier ensuite le rendu côté front : `HelpModal.svelte` affiche `audit.signing?.name` et
`audit.signing?.standard`, et doit maintenant afficher la troisième entrée sans casser la mise en
page.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml m3_
npx svelte-check --threshold warning
```

Attendu : 0 erreur, 0 avertissement.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/security/crypto_agility.rs src/lib/HelpModal.svelte
git commit -m "fix(security): M3 — the in-app crypto inventory reports the real primitives"
```

---

## Task 16 : M4 — le déverrouillage biométrique ne survit plus à une nouvelle identité

**Files:**
- Modify: `src-tauri/src/commands/identity.rs` (`create_wallet`, `restore_wallet`)
- Test: `src-tauri/src/commands/identity.rs`

- [ ] **Step 1: Write the failing test**

```rust
    /// **M4 — a restored identity must not inherit the previous Touch ID wrap.**
    /// The stale KEK unwrapped the old vault keys, AES-GCM then failed on the new
    /// blobs, and every attempt burned the brute-force backoff shared with the
    /// password path — in exactly the scenario RECOVER-1 exists for.
    #[tokio::test]
    async fn m4_restore_clears_the_biometric_wrap() {
        let db = test_db().await;
        db.save_state(BIOMETRIC_WRAP_KEY, "stale-wrap").await.expect("seed");
        restore_wallet_inner(&db, TEST_PHRASE, "new-password").await.expect("restore");
        let wrap = db.load_state(BIOMETRIC_WRAP_KEY).await.unwrap_or_default();
        assert!(wrap.is_empty(), "the stale wrap must be cleared by a restore");
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --manifest-path src-tauri/Cargo.toml m4_restore_clears_the_biometric_wrap
```

- [ ] **Step 3: Write minimal implementation**

À la fin de `create_wallet` et de `restore_wallet`, après la persistance de la nouvelle identité :

```rust
    // M4 — a new identity invalidates quick unlock: the Keychain KEK wraps the
    // OLD vault's Argon2id-derived keys, so keeping it means Touch ID succeeds,
    // AES-GCM fails, and each attempt burns the shared brute-force backoff.
    let _ = tokio::task::spawn_blocking(security::biometric::delete_kek).await;
    dbref.save_state(BIOMETRIC_WRAP_KEY, "").await?;
```

Vérifier le nom exact de la fonction de suppression du KEK dans `security/biometric.rs` et
réutiliser le corps de `disable_biometric_unlock` s'il fait déjà exactement cela.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml m4_
cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/identity.rs
git commit -m "fix(identity): M4 — clear the biometric wrap on wallet create and restore"
```

---

## Vérification finale

- [ ] **Full suite green**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
npx svelte-check --threshold warning
```

Attendu : au moins 449 tests + les nouveaux, zéro échec ; clippy silencieux ; svelte-check 0/0.

- [ ] **Two physical nodes still exchange gossip**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test p2p_integration
```

C'est le seul test qui prouve que les tâches 7 et 8 n'ont pas cassé le fil.

- [ ] **Update the audit report status**

Marquer dans `docs/audit/AUDIT-INTERNE-2026-07-25.md` chaque constat fermé, avec le hash du commit
qui le ferme. Ce qui n'a pas été traité — les onze constats moyens non vérifiés, les onze de
l'annexe, et les angles morts du §8 — reste ouvert et doit le rester explicitement.

---

## Hors périmètre, à traiter séparément

Ces points sortent du plan mais ne doivent pas se perdre :

- **Le canal de mise à jour pointe vers `nobodyohm-web/Torus`** (`tauri.conf.json:46`,
  `package.json`). Ne fonctionne que par la redirection GitHub des dépôts renommés. La signature
  minisign protège l'exécution ; l'impact est un déni de mise à jour. Correction triviale, à faire.
- **CSP** : `script-src 'unsafe-inline'` à retirer, et les origines Google Fonts à supprimer
  puisque plus aucun fichier ne les appelle.
- **`claude-review.yml` accorde `contents: write` sur `issue_comment`** sur un dépôt public — à
  auditer avant tout le reste de la chaîne CI.
- **La phrase BIP39 n'est jamais zeroizée** (la caisse `bip39` est compilée sans la fonctionnalité
  `zeroize`). C'est le secret qui contrôle les fonds.
- **`rustls-webpki 0.102.8`** porte quatre avis RUSTSEC sur le chemin TLS du transport : mettre à
  jour la dépendance et réévaluer les huit avis, dont l'évaluation documentée en v3.11 porte sur un
  jeu périmé.
- **`postcss`** : `npm audit fix` suffit, sans rupture.
