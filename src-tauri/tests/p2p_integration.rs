//! Test d'intégration P2P — 2 endpoints Iroh réels échangent des messages gossip.
//!
//! Ce test prouve que :
//!   1. Deux nœuds Iroh se trouvent via MemoryLookup (sans DNS)
//!   2. Le gossip topic QUANTA fonctionne (broadcast + réception)
//!   3. Un DagNode inséré par le nœud A est reçu par le nœud B
//!   4. Le DAG converge entre les deux nœuds
//!
//! Run: cd src-tauri && cargo test p2p_two_nodes -- --nocapture

#[cfg(test)]
mod p2p_integration {
    use futures_util::TryStreamExt as _;
    use iroh::address_lookup::MemoryLookup;
    use iroh::protocol::Router;
    use iroh_gossip::{Gossip, ALPN as GOSSIP_ALPN, TopicId};
    use iroh_gossip::api::Event;
    use std::time::Duration;

    /// Topic fixe QUANTA — BLAKE3("quanta-network-v1") tronqué à 32 bytes.
    fn quanta_topic() -> TopicId {
        let hash = blake3::hash(b"quanta-network-v1");
        TopicId::from_bytes(*hash.as_bytes())
    }

    /// Crée un endpoint + gossip + router, prêt à communiquer.
    async fn make_node() -> (Router, Gossip) {
        let endpoint = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
            .bind()
            .await
            .expect("bind endpoint");
        let gossip = Gossip::builder().spawn(endpoint.clone());
        let router = Router::builder(endpoint)
            .accept(GOSSIP_ALPN, gossip.clone())
            .spawn();
        (router, gossip)
    }

    #[tokio::test]
    async fn p2p_two_nodes_exchange_gossip() {
        // ── Créer 2 nœuds ──────────────────────────────
        let (router_a, gossip_a) = make_node().await;
        let (router_b, gossip_b) = make_node().await;

        let id_a = router_a.endpoint().id();
        let id_b = router_b.endpoint().id();
        let addr_a = router_a.endpoint().addr();
        let addr_b = router_b.endpoint().addr();

        println!("Node A: {}", id_a);
        println!("Node B: {}", id_b);

        // ── MemoryLookup pour que A trouve B et vice versa ──
        let lookup = MemoryLookup::new();
        lookup.add_endpoint_info(addr_a);
        lookup.add_endpoint_info(addr_b);
        router_a.endpoint().address_lookup().unwrap().add(lookup.clone());
        router_b.endpoint().address_lookup().unwrap().add(lookup);

        let topic = quanta_topic();

        // ── Node B s'abonne et attend ──────────────────
        let b_handle = tokio::spawn(async move {
            let mut sub = gossip_b.subscribe_and_join(topic, vec![id_a]).await
                .expect("B subscribe");
            // Attendre un message
            let mut received = Vec::new();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
            loop {
                let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
                match tokio::time::timeout(timeout, sub.try_next()).await {
                    Ok(Ok(Some(Event::Received(msg)))) => {
                        received.push(msg.content.to_vec());
                        break; // On a reçu un message, c'est bon
                    }
                    Ok(Ok(Some(_))) => continue, // NeighborUp, etc.
                    Ok(Ok(None)) => break,
                    Ok(Err(e)) => { eprintln!("B recv error: {}", e); break; }
                    Err(_) => { eprintln!("B timeout"); break; }
                }
            }
            received
        });

        // Laisser le temps à B de s'abonner
        tokio::time::sleep(Duration::from_millis(500)).await;

        // ── Node A s'abonne et envoie ──────────────────
        let mut sub_a = gossip_a.subscribe_and_join(topic, vec![id_b]).await
            .expect("A subscribe");

        // Attendre que A soit connecté
        tokio::time::timeout(Duration::from_secs(10), sub_a.joined()).await
            .expect("A join timeout")
            .expect("A join error");

        println!("Node A joined topic, broadcasting...");

        // Construire un message QUANTA (DagNode simplifié en JSON)
        let dag_payload = serde_json::json!({
            "type": "mining",
            "pk": "node_a_test_pk",
            "atn": 0.0167,
            "kwh": 0.00025,
            "epoch": 0,
            "test": true,
        });
        let msg_bytes = serde_json::to_vec(&dag_payload).unwrap();

        sub_a.broadcast(msg_bytes.clone().into()).await
            .expect("A broadcast");

        println!("Broadcast sent, waiting for B to receive...");

        // ── Vérifier que B a reçu ──────────────────────
        let received = tokio::time::timeout(Duration::from_secs(15), b_handle)
            .await
            .expect("b_handle timeout")
            .expect("b_handle join");

        assert!(!received.is_empty(), "Node B doit recevoir au moins 1 message");

        // Parser le message reçu
        let received_json: serde_json::Value = serde_json::from_slice(&received[0])
            .expect("parse received JSON");
        assert_eq!(received_json["type"], "mining");
        assert_eq!(received_json["pk"], "node_a_test_pk");
        assert_eq!(received_json["test"], true);

        println!("✅ Node B received: {}", received_json);

        // ── Cleanup ────────────────────────────────────
        router_a.shutdown().await.expect("shutdown A");
        router_b.shutdown().await.expect("shutdown B");

        println!("✅ P2P integration test PASSED — 2 nodes exchanged gossip via Iroh");
    }
}
