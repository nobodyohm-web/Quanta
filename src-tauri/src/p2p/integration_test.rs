//! Integration test — full node lifecycle.
//!
//! Exercises: identity → mine → page publish → transfer → verify chain → snapshot/restore.

#[cfg(test)]
mod tests {
    use crate::p2p::ledger::{Ledger, MICRO};
    use crate::p2p::page_store::{PageStore, PublishedPage};
    use crate::security::CryptoEngine;

    /// Full lifecycle: identity → mine → page → transfer → verify → snapshot.
    #[test]
    fn full_node_lifecycle() {
        // ── 1. Create identity ──
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let pk = id.public_key_hex.clone();
        assert!(!pk.is_empty(), "identity must be created");

        // ── 2. Mine 50 QUANTA ──
        let mut ledger = Ledger::new();
        for _ in 0..5 {
            ledger.mine_tx(&pk, 10 * MICRO, 0.5);
        }
        assert_eq!(ledger.balance_of(&pk), 50 * MICRO, "should have 50 QNT");

        // ── 3. Publish a page ──
        let mut page_store = PageStore::new();
        let content = "<h1>QUANTA Node Live</h1>";
        let version = 1u64;
        let signable = format!("{}:{}:{}", pk, version, content);
        let sig = crypto.sign(signable.as_bytes()).expect("signing must work");
        let page = PublishedPage {
            author_pk: pk.clone(),
            content: content.into(),
            title: "My Page".into(),
            updated_at: 1000,
            signature: hex::encode(&sig),
            version,
        };
        page_store.publish(page).expect("page publish must work");
        assert_eq!(page_store.page_count(), 1);

        // ── 4. Transfer (with 1% burn) ──
        let recipient = "b".repeat(64);
        let (tx, burn) = ledger.transfer_with_burn(&pk, &recipient, 10 * MICRO, &crypto)
            .expect("transfer must succeed");
        assert_eq!(burn, 100_000, "1% burn = 0.1 QNT");
        assert_eq!(tx.amount, 9_900_000, "net amount after burn");
        assert_eq!(ledger.balance_of(&pk), 40 * MICRO, "sender has 40 QNT left");

        // ── 5. Seal and verify chain ──
        ledger.seal_block(&pk, 2.5);
        let (blocks, txs) = ledger.verify_chain().expect("chain must be valid");
        assert!(blocks >= 2, "at least genesis + 1 sealed block");
        assert!(txs > 5, "5 mining + 1 transfer + 1 burn = 7+ txs");

        // ── 6. Snapshot and restore ──
        let snap = ledger.snapshot();
        let restored = Ledger::restore(snap);
        assert_eq!(restored.balance_of(&pk), ledger.balance_of(&pk));
        assert_eq!(restored.balance_of(&recipient), ledger.balance_of(&recipient));
        assert_eq!(restored.stats().total_blocks, ledger.stats().total_blocks);
        assert_eq!(restored.stats().total_txs, ledger.stats().total_txs);

        // ── 7. Page snapshot and restore ──
        let page_snap = page_store.snapshot();
        let restored_pages = PageStore::restore(page_snap);
        assert_eq!(restored_pages.page_count(), 1);
        let page = restored_pages.get_page(&pk).expect("page must be restored");
        assert_eq!(page.title, "My Page");

        // ── 8. Supply conservation ──
        let supply = ledger.total_supply();
        let mined = ledger.stats().total_mined;
        let burned = ledger.total_burned();
        assert_eq!(supply, mined - burned, "supply = mined - burned");

        println!("\n═══════ INTEGRATION TEST ═══════");
        println!("  Identity: {}...", &pk[..12]);
        println!("  Balance: {} µQTA", ledger.balance_of(&pk));
        println!("  Blocks: {}, Txs: {}", blocks, txs);
        println!("  Supply: {} µQTA (burned: {})", supply, burned);
        println!("  Pages: {}", page_store.page_count());
        println!("  Snapshot/restore: ✅");
        println!("  Chain integrity: ✅");
        println!("═══════════════════════════════\n");
    }

    /// Verify the shutdown token works (unit-level, no Tauri runtime)
    #[tokio::test]
    async fn shutdown_token_cancels_cleanly() {
        use tokio_util::sync::CancellationToken;
        let token = CancellationToken::new();
        let child = token.child_token();

        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = child.cancelled() => "shutdown",
                _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => "timeout",
            }
        });

        // Cancel immediately
        token.cancel();
        let result = handle.await.unwrap();
        assert_eq!(result, "shutdown", "token must cancel the select");
    }
}
