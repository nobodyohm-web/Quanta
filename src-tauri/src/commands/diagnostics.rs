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
    // **A9 (AUDIT-2026-08-13) — DIAG-SANITIZE-1.**
    //
    // `msg` arrive du webview et était écrit **brut**, en append, sans borne, sans
    // rotation et sans limite de débit. Deux conséquences : un `\n` dans le
    // message fabriquait des lignes de journal entières (falsification de
    // journal), et un appelant en boucle remplissait le disque. Le journal d'un
    // portefeuille est en plus une fuite passive d'activité financière, d'où le
    // 0600 posé plus bas.
    //
    // On borne, on met les sauts de ligne et les caractères de contrôle en
    // échappement, et on écrit **une seule ligne**, toujours.
    let msg = sanitize_diag(msg);
    log::warn!("◈ [UI-DIAG] {}", msg);
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "◈ [UI-DIAG] {}", msg);
    let path = node_runtime::default_data_dir().join("ui-diag.log");
    // A9 : rotation naïve mais suffisante — au-delà de la taille limite, le
    // journal repart de zéro. Un diagnostic n'a d'intérêt que récent, et un
    // fichier qui grossit sans fin est un vecteur de déni de service local.
    if std::fs::metadata(&path).map(|m| m.len() > MAX_DIAG_LOG_BYTES).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600); // A9 : le journal reflète l'activité du portefeuille.
    }
    if let Ok(mut f) = opts.open(&path) {
        let _ = writeln!(f, "[{}] {}", epoch_secs(), msg);
    }
}

/// A9 — taille maximale de `ui-diag.log` avant remise à zéro (256 Kio).
const MAX_DIAG_LOG_BYTES: u64 = 256 * 1024;

/// A9 — longueur maximale d'un message de diagnostic, en octets.
const MAX_DIAG_MSG_LEN: usize = 2048;

/// A9 — une ligne, et une seule : les caractères de contrôle (dont `\n` et
/// `\r`) sont remplacés, et la longueur est bornée à une frontière de caractère.
fn sanitize_diag(msg: &str) -> String {
    let mut out: String = msg
        .chars()
        .map(|c| if c.is_control() { '·' } else { c })
        .collect();
    if out.len() > MAX_DIAG_MSG_LEN {
        let mut end = MAX_DIAG_MSG_LEN;
        while end > 0 && !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
        out.push_str("…[tronqué]");
    }
    out
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
