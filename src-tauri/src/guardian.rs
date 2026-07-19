//! Freeze guardian (Rust side — the layers the JS heartbeat cannot see).
//!
//! 1. Timed macOS main thread (`run_on_main_thread`): a beachball (main thread
//!    blocked > 1 s) becomes a timestamped line.
//! 2. Webview heartbeat: `diag.ts` invokes `ui_beat` every 5 s; 25 s of silence
//!    = dead render process (WebContent killed) → reload the webview (WKWebView
//!    spins up a fresh WebContent).
//!
//! Entirely best-effort: the guardian never touches node state. Extracted from
//! `lib.rs::run().setup()` — spawn it once with the app handle.

use crate::commands::diagnostics::{
    epoch_secs, ui_diag_write, GUARDIAN_RELOADED, LAST_UI_BEAT,
};

/// Spawn the background freeze guardian on the Tauri async runtime. Called from
/// `setup()` with `app.handle().clone()`.
pub(crate) fn spawn_freeze_guardian(guard: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        use tauri::Manager;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            // 1. Thread principal — ping chaque seconde, rapport dès 300 ms
            //    (un gel d'1-2 s à chaque bloc passait sous l'ancien 5 s/1 s).
            let t0 = std::time::Instant::now();
            let (tx, rx) = tokio::sync::oneshot::channel::<()>();
            let _ = guard.run_on_main_thread(move || {
                let _ = tx.send(());
            });
            match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
                Ok(Ok(())) => {
                    let ms = t0.elapsed().as_millis() as u64;
                    if ms > 300 {
                        ui_diag_write(&format!("GEL thread-principal Rust {} ms (beachball)", ms));
                    }
                }
                _ => ui_diag_write("GEL thread-principal Rust >5 s (beachball sévère)"),
            }
            // 2. Battement webview
            let last = LAST_UI_BEAT.load(std::sync::atomic::Ordering::Relaxed);
            if last != 0 {
                let silent = epoch_secs().saturating_sub(last);
                if silent > 25 {
                    // Fenêtre repliée = suspension macOS NORMALE, pas un
                    // webview mort — ne jamais recharger dans ce cas
                    // (9 rechargements fantômes constatés le 19/07 au soir).
                    let minimized = guard
                        .get_webview_window("main")
                        .and_then(|w| w.is_minimized().ok())
                        .unwrap_or(false);
                    LAST_UI_BEAT.store(epoch_secs(), std::sync::atomic::Ordering::Relaxed);
                    if minimized {
                        log::debug!("◈ [Gardien] fenêtre repliée — silence normal, pas de rechargement");
                    } else {
                        ui_diag_write(&format!(
                            "webview muet depuis {} s — rechargement automatique",
                            silent
                        ));
                        GUARDIAN_RELOADED.store(true, std::sync::atomic::Ordering::Relaxed);
                        if let Some(w) = guard.get_webview_window("main") {
                            let _ = w.reload();
                        }
                    }
                }
            }
        }
    });
}
