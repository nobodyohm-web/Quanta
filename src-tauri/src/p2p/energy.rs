//! Oracle d'énergie réelle — prix électricité par pays + mesure CPU
//!
//! Sources des prix : Eurostat, EIA, IEA (Q1 2026).
//! Mise à jour trimestrielle recommandée.
//!
//! Principe : le plancher ATN est ancré au coût énergétique réel.
//! 1 ATN = énergie consommée pour le miner × prix local de l'électricité.

use std::collections::HashMap;

// ─── Oracle de prix ─────────────────────────────────────────────────────────

/// Prix de l'électricité par pays (EUR/kWh, source Eurostat/EIA Q1 2026).
pub struct EnergyOracle {
    prices: HashMap<&'static str, f64>,
}

impl EnergyOracle {
    /// Construit l'oracle avec la table de prix embarquée.
    pub fn new() -> Self {
        let mut prices = HashMap::new();
        // Europe
        prices.insert("FR", 0.2516); // France
        prices.insert("DE", 0.3471); // Allemagne
        prices.insert("GB", 0.2780); // Royaume-Uni
        prices.insert("IT", 0.2890); // Italie
        prices.insert("ES", 0.2230); // Espagne
        prices.insert("CH", 0.2710); // Suisse
        prices.insert("BE", 0.3120); // Belgique
        prices.insert("NL", 0.3250); // Pays-Bas
        prices.insert("AT", 0.2640); // Autriche
        prices.insert("SE", 0.1580); // Suède
        prices.insert("NO", 0.1120); // Norvège
        prices.insert("FI", 0.1940); // Finlande
        prices.insert("DK", 0.3680); // Danemark
        prices.insert("PL", 0.1890); // Pologne
        prices.insert("PT", 0.2370); // Portugal
        prices.insert("RO", 0.1620); // Roumanie
        // Amériques
        prices.insert("US", 0.1385); // USA (converti EUR)
        prices.insert("CA", 0.1090); // Canada
        prices.insert("BR", 0.1150); // Brésil
        prices.insert("MX", 0.0920); // Mexique
        prices.insert("AR", 0.0480); // Argentine
        // Asie-Pacifique
        prices.insert("JP", 0.2190); // Japon
        prices.insert("KR", 0.1120); // Corée du Sud
        prices.insert("CN", 0.0890); // Chine
        prices.insert("IN", 0.0720); // Inde
        prices.insert("AU", 0.2340); // Australie
        prices.insert("NZ", 0.2010); // Nouvelle-Zélande
        prices.insert("SG", 0.2100); // Singapour
        prices.insert("HK", 0.1780); // Hong Kong
        prices.insert("TW", 0.0970); // Taïwan
        // Afrique / Moyen-Orient
        prices.insert("ZA", 0.0880); // Afrique du Sud
        prices.insert("AE", 0.0740); // Émirats arabes unis
        prices.insert("IL", 0.1830); // Israël
        prices.insert("TR", 0.0950); // Turquie

        Self { prices }
    }

    /// Prix EUR/kWh pour un code pays ISO 3166-1 alpha-2.
    /// Retourne 0.15 (moyenne UE) si le pays est inconnu.
    pub fn price_for(&self, country: &str) -> f64 {
        *self.prices.get(country).unwrap_or(&0.15)
    }

    /// Calcule le plancher EUR d'1 ATN pour un pays donné.
    /// Formule : (WATTS_IDLE / 1000) × prix_kWh
    pub fn atn_floor_eur(&self, country: &str, watts_idle: f64) -> f64 {
        (watts_idle / 1000.0) * self.price_for(country)
    }

    /// Phase 3 — moyenne réseau pondérée du prix de l'électricité.
    /// `peer_reports` = [(country_code, node_count), …]
    /// Fallback : 0.15 EUR/kWh (moyenne UE) si aucun pair n'a rapporté.
    pub fn network_weighted_average(&self, peer_reports: &[(String, u64)]) -> f64 {
        let total_nodes: u64 = peer_reports.iter().map(|(_, n)| *n).sum();
        if total_nodes == 0 { return 0.15; }
        let weighted: f64 = peer_reports.iter()
            .map(|(cc, n)| self.price_for(cc) * (*n as f64))
            .sum();
        weighted / total_nodes as f64
    }

    /// Détecte le pays depuis la variable d'environnement TZ (offline, sans API).
    /// Exemples : "Europe/Paris" → "FR", "America/New_York" → "US"
    pub fn detect_country() -> &'static str {
        let tz = std::env::var("TZ").unwrap_or_default();
        Self::tz_to_country(&tz)
    }

    fn tz_to_country(tz: &str) -> &'static str {
        // Correspondance timezone → code pays (cas les plus fréquents)
        match tz {
            s if s.contains("Paris") || s.contains("Lyon") || s.contains("France") => "FR",
            s if s.contains("Berlin") || s.contains("Germany") => "DE",
            s if s.contains("London") || s.contains("Dublin") => "GB",
            s if s.contains("Rome") || s.contains("Italy") => "IT",
            s if s.contains("Madrid") || s.contains("Spain") => "ES",
            s if s.contains("Zurich") || s.contains("Bern") => "CH",
            s if s.contains("Amsterdam") => "NL",
            s if s.contains("Brussels") => "BE",
            s if s.contains("Stockholm") => "SE",
            s if s.contains("Oslo") => "NO",
            s if s.contains("Helsinki") => "FI",
            s if s.contains("Copenhagen") => "DK",
            s if s.contains("Warsaw") => "PL",
            s if s.contains("Lisbon") => "PT",
            // USA / Canada
            s if s.starts_with("America/New_York") || s.contains("Eastern") => "US",
            s if s.starts_with("America/Chicago") || s.contains("Central") => "US",
            s if s.starts_with("America/Denver") || s.starts_with("America/Phoenix") => "US",
            s if s.starts_with("America/Los_Angeles") || s.contains("Pacific") => "US",
            s if s.starts_with("America/Toronto") || s.starts_with("America/Vancouver") => "CA",
            // Asie
            s if s.contains("Tokyo") => "JP",
            s if s.contains("Seoul") => "KR",
            s if s.contains("Shanghai") || s.contains("Beijing") => "CN",
            s if s.contains("Kolkata") || s.contains("Mumbai") || s.contains("India") => "IN",
            s if s.contains("Sydney") || s.contains("Melbourne") => "AU",
            s if s.contains("Singapore") => "SG",
            // Fallback: supposer France (développement local probable)
            _ => "FR",
        }
    }


}

impl Default for EnergyOracle {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Mesure CPU réelle ───────────────────────────────────────────────────────

/// Estime la consommation électrique instantanée en watts.
///
/// ## Stratégie multi-plateforme
///
/// 1. **Linux (Intel/AMD)** : tente `/sys/class/powercap/intel-rapl/...` (RAPL).
///    Sur kernel ≥ 5.4 c'est root-only par défaut, donc fallback fréquent.
/// 2. **macOS Apple Silicon (aarch64)** : tente `powermetrics --samplers cpu_power`.
///    Sans sudo ça échoue, on cache le résultat et on ne retente plus.
/// 3. **Tout le reste** (Intel mac, Windows, Linux sans RAPL accessible) :
///    fallback sysinfo (CPU% × TDP plateforme).
///
/// Toutes les méthodes spécifiques sont strictement *best-effort* — un échec
/// silencieux retombe sur sysinfo. Aucune dépendance externe ni privilège requis.
pub fn estimate_watts() -> f64 {
    if let Some(w) = try_platform_specific() {
        return w;
    }
    estimate_via_sysinfo()
}

/// Estimation sysinfo : CPU% × TDP plateforme. Toujours disponible.
fn estimate_via_sysinfo() -> f64 {
    let cpu_pct = read_cpu_usage_pct();

    let (idle_w, max_w): (f64, f64) = if cfg!(target_os = "macos") {
        (5.0, 30.0)   // Apple Silicon — très efficace
    } else if cfg!(target_os = "windows") {
        (20.0, 65.0)  // PC Windows moyen
    } else {
        (15.0, 65.0)  // Linux / autre (laptop x86_64)
    };

    (idle_w + (cpu_pct / 100.0) * (max_w - idle_w)).clamp(idle_w, max_w)
}

/// Lit le pourcentage d'utilisation CPU global via sysinfo.
/// Retourne 30.0 % en cas d'erreur (estimation conservative).
fn read_cpu_usage_pct() -> f64 {
    use sysinfo::System;
    let mut sys = System::new_all();
    sys.refresh_cpu_usage();
    let cpus = sys.cpus();
    if cpus.is_empty() {
        return 30.0;
    }
    cpus.iter().map(|c| c.cpu_usage() as f64).sum::<f64>() / cpus.len() as f64
}

// ─── Backends spécifiques (best-effort) ──────────────────────────────────────

#[cfg(target_os = "linux")]
fn try_platform_specific() -> Option<f64> {
    read_rapl_watts()
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn try_platform_specific() -> Option<f64> {
    read_powermetrics_watts()
}

#[cfg(not(any(target_os = "linux", all(target_os = "macos", target_arch = "aarch64"))))]
fn try_platform_specific() -> Option<f64> {
    None
}

// ── Linux RAPL (Intel/AMD) ────────────────────────────────────────────────

/// Snapshot d'un compteur RAPL pour calculer les watts par delta.
#[cfg(target_os = "linux")]
struct RaplSample {
    energy_uj: u64,
    at: std::time::Instant,
}

#[cfg(target_os = "linux")]
static RAPL_STATE: std::sync::OnceLock<std::sync::Mutex<Option<RaplSample>>> =
    std::sync::OnceLock::new();

#[cfg(target_os = "linux")]
const RAPL_PATH: &str = "/sys/class/powercap/intel-rapl/intel-rapl:0/energy_uj";

/// Lit l'énergie cumulée RAPL et retourne les watts depuis l'appel précédent.
/// Premier appel : pas de delta → renvoie None (sysinfo prend le relais).
#[cfg(target_os = "linux")]
fn read_rapl_watts() -> Option<f64> {
    let raw = std::fs::read_to_string(RAPL_PATH).ok()?;
    let energy_uj: u64 = raw.trim().parse().ok()?;
    let now = std::time::Instant::now();

    let cell = RAPL_STATE.get_or_init(|| std::sync::Mutex::new(None));
    let mut state = cell.lock().ok()?;

    let prev = state.replace(RaplSample { energy_uj, at: now });
    let prev = prev?;

    let dt_secs = now.duration_since(prev.at).as_secs_f64();
    if dt_secs <= 0.0 {
        return None;
    }
    // RAPL counters wrap; saturating_sub gives 0 on wrap which falls through.
    let delta_uj = energy_uj.saturating_sub(prev.energy_uj);
    if delta_uj == 0 {
        return None;
    }
    let watts = (delta_uj as f64 / 1_000_000.0) / dt_secs;
    if watts.is_finite() && watts > 0.0 {
        Some(watts)
    } else {
        None
    }
}

// ── macOS Apple Silicon: powermetrics ─────────────────────────────────────

/// Cache "powermetrics indisponible" — set au premier échec, jamais rejoué.
/// Évite de spawner un process subshell à chaque tick quand sudo manque.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
static POWERMETRICS_DISABLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn read_powermetrics_watts() -> Option<f64> {
    if POWERMETRICS_DISABLED.get().is_some() {
        return None;
    }
    let output = std::process::Command::new("powermetrics")
        .args(["--samplers", "cpu_power", "-n", "1", "-i", "200"])
        .output()
        .ok();
    let output = match output {
        Some(o) => o,
        None => {
            // Binary missing entirely (e.g. stripped /usr/bin) — never retry.
            let _ = POWERMETRICS_DISABLED.set(());
            return None;
        }
    };
    if !output.status.success() {
        // 99% des cas : "powermetrics must be invoked as the superuser".
        let _ = POWERMETRICS_DISABLED.set(());
        return None;
    }
    let stdout = std::str::from_utf8(&output.stdout).ok()?;
    parse_powermetrics_cpu_power(stdout)
}

/// Extrait les watts depuis la sortie texte de `powermetrics --samplers cpu_power`.
/// Cherche les lignes "CPU Power: <n> mW" ou "Package Power: <n> mW" et convertit.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn parse_powermetrics_cpu_power(stdout: &str) -> Option<f64> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        let Some(body) = trimmed.strip_prefix("CPU Power:")
            .or_else(|| trimmed.strip_prefix("Package Power:"))
        else { continue };
        let num: String = body.chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let Ok(mw) = num.parse::<f64>() else { continue };
        if mw.is_finite() && mw > 0.0 {
            return Some(mw / 1000.0);
        }
    }
    None
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_has_expected_countries() {
        let o = EnergyOracle::new();
        assert!(o.price_for("FR") > 0.0);
        assert!(o.price_for("DE") > 0.0);
        assert_eq!(o.price_for("XX"), 0.15, "Fallback doit être 0.15");
    }

    #[test]
    fn tz_detection_known_zones() {
        assert_eq!(EnergyOracle::tz_to_country("Europe/Paris"), "FR");
        assert_eq!(EnergyOracle::tz_to_country("America/New_York"), "US");
        assert_eq!(EnergyOracle::tz_to_country("Asia/Tokyo"), "JP");
    }

    #[test]
    fn watts_estimate_is_plausible() {
        let w = estimate_watts();
        assert!((1.0..=200.0).contains(&w), "Watts hors plage plausible: {}", w);
    }

    #[test]
    fn network_average_weights_by_node_count() {
        let oracle = EnergyOracle::new();
        // 1 nœud FR (0.2516) + 9 nœuds IN (0.0720) → moyenne pondérée ≈ 0.0900
        let reports = vec![("FR".to_string(), 1u64), ("IN".to_string(), 9u64)];
        let avg = oracle.network_weighted_average(&reports);
        let expected = (0.2516 + 9.0 * 0.0720) / 10.0;
        assert!((avg - expected).abs() < 1e-6, "avg pondérée = {} (attendu {})", avg, expected);
    }

    #[test]
    fn network_average_empty_falls_back() {
        let oracle = EnergyOracle::new();
        let avg = oracle.network_weighted_average(&[]);
        assert!((avg - 0.15).abs() < 1e-9, "fallback EU = 0.15");
    }

    /// Le contrat de `estimate_watts` est : ne jamais paniquer, ne jamais
    /// retourner NaN/Inf — peu importe le succès du backend spécifique.
    #[test]
    fn estimate_watts_always_finite_and_positive() {
        for _ in 0..3 {
            let w = estimate_watts();
            assert!(w.is_finite(), "watts must be finite, got {}", w);
            assert!(w > 0.0, "watts must be positive, got {}", w);
            assert!(w < 1000.0, "watts must be plausible, got {}", w);
        }
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn powermetrics_parser_extracts_cpu_power_mw() {
        let sample = "\
*** Sampled system activity ...
**** Processor usage ****
CPU Power: 1234 mW
GPU Power: 200 mW
Package Power: 1500 mW
";
        let watts = parse_powermetrics_cpu_power(sample).expect("should parse");
        // Premier hit = CPU Power = 1234 mW = 1.234 W
        assert!((watts - 1.234).abs() < 1e-6, "got {}", watts);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn powermetrics_parser_skips_unparseable_lines() {
        let sample = "CPU Power: -- mW\nCPU Power: 500 mW\n";
        let watts = parse_powermetrics_cpu_power(sample).expect("second line should parse");
        assert!((watts - 0.5).abs() < 1e-6);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn powermetrics_parser_returns_none_on_no_match() {
        assert!(parse_powermetrics_cpu_power("nothing useful here").is_none());
        assert!(parse_powermetrics_cpu_power("").is_none());
    }
}
