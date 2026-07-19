//! UI freeze diagnostics — best-effort probes the frontend feeds so a hang
//! (blocked JS thread, stalled rAF) becomes a timestamped line. None of these
//! touch node state. The freeze guardian (`crate::guardian`) reads the shared
//! heartbeat/reload statics defined here.

use crate::node_runtime;

/// Secondes epoch (0 en cas d'horloge cassée — jamais de panic).
pub(crate) fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Écrit un rapport de diagnostic sur TOUS les canaux, sans jamais paniquer :
/// log Rust, stderr (best-effort — `eprintln!` panique si stderr est fermé,
/// c'est ce qui a tué les instances de dev du 2026-07-19), et un journal
/// persistant `ui-diag.log` dans le dossier de données (lisible app fermée).
pub(crate) fn ui_diag_write(msg: &str) {
    log::warn!("◈ [UI-DIAG] {}", msg);
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "◈ [UI-DIAG] {}", msg);
    let path = node_runtime::default_data_dir().join("ui-diag.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{}] {}", epoch_secs(), msg);
    }
}

/// Sonde de diagnostic UI : le frontend rapporte ici tout gel (fil JS bloqué,
/// rendu rAF figé) avec l'anneau des opérations qui l'entouraient.
/// Best-effort, aucun effet sur l'état du nœud — le gel devient une donnée datée.
#[tauri::command]
pub async fn ui_diag(msg: String) {
    ui_diag_write(&msg);
}

/// Battement de cœur du webview (diag.ts, toutes les 5 s). Nourrit le gardien :
/// 25 s de silence = webview mort (WebContent tué) → rechargement automatique.
pub(crate) static LAST_UI_BEAT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[tauri::command]
pub async fn ui_beat() {
    LAST_UI_BEAT.store(epoch_secs(), std::sync::atomic::Ordering::Relaxed);
}

/// Vrai UNE fois après un rechargement initié par le gardien — le frontend
/// reprend alors la session sans écran de déverrouillage (le vault Rust est
/// resté chaud ; l'auto-lock volontaire, lui, ne passe jamais par ici).
pub(crate) static GUARDIAN_RELOADED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
pub async fn was_guardian_reload() -> bool {
    GUARDIAN_RELOADED.swap(false, std::sync::atomic::Ordering::Relaxed)
}
