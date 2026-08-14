// build.rs — génération du contexte Tauri **et du manifeste ACL applicatif**.
//
// **A1 (AUDIT-2026-08-13) — ACL-APP-1 : l'ACL de Tauri ne couvrait aucune des
// commandes de l'application.**
//
// `tauri_build::build()` (l'appel par défaut) ne déclare pas de manifeste
// applicatif. Tauri 2 ne fait alors respecter l'ACL que pour les commandes de
// *plugin* : les 41 commandes de Quanta passaient à côté du contrôle, et
// `gen/schemas/acl-manifests.json` ne contenait, de fait, aucune clé applicative
// — la capacité `core:default` donnait donc l'**illusion** d'un périmètre qui
// n'existait pas.
//
// En déclarant les commandes ici, `tauri-build` génère pour chacune les
// permissions `allow-<commande>` / `deny-<commande>`, et
// `capabilities/default.json` doit désormais les accorder explicitement. Le
// périmètre devient réel : ajouter une commande sans l'accorder la laisse
// **refusée**, ce qui est le bon défaut — c'était exactement l'inverse avant.
//
// Ce que ceci NE fait PAS, et il faut le dire : la capacité vise la fenêtre
// principale, donc du JavaScript exécuté **dans** cette fenêtre reste autorisé à
// appeler ce qu'elle autorise. L'ACL borne la surface, elle ne remplace pas la
// réauthentification sur les commandes sensibles (A2, `get_recovery_phrase`
// exige maintenant le mot de passe) ni une CSP sans `unsafe-inline` (A13).
fn main() {
    let attrs = tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
        "ui_diag",
        "ui_beat",
        "was_guardian_reload",
        "check_identity",
        "create_identity",
        "unlock_identity",
        "lock_wallet",
        "is_wallet_unlocked",
        "get_public_key",
        "get_recovery_key",
        "get_recovery_phrase",
        "restore_from_phrase",
        "get_receive_address",
        "validate_address",
        "biometric_status",
        "enable_biometric_unlock",
        "disable_biometric_unlock",
        "unlock_biometric",
        "get_node_status",
        "get_node_mode",
        "get_peer_metrics",
        "set_display_name",
        "get_display_name",
        "get_security_audit",
        "connect_peer",
        "get_my_reputation",
        "ledger_transfer",
        "ledger_stake",
        "ledger_unstake",
        "get_wallet_overview",
        "get_chain_overview",
        "get_chain_history",
        "get_recent_txs",
        "get_finality_status",
        "get_economy_stats",
        "claim_username",
        "resolve_username",
        "is_username_available",
        "get_my_username",
        "get_my_connection_code",
        "verify_connection"
        ]),
    );
    tauri_build::try_build(attrs).expect("génération du contexte Tauri + manifeste ACL");
}
