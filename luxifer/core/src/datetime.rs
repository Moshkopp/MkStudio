//! Zeit- und ID-Utilities ohne Fremd-Crate (kein chrono/rand): stabile IDs
//! (`gen_id`) und ISO-8601-UTC-Zeitstempel (`now_iso8601`). Generisch,
//! auch von assets.rs genutzt — bewusst getrennt von der Projektdatei-Logik.

use std::time::{SystemTime, UNIX_EPOCH};

/// Erzeugt eine stabile, praktisch eindeutige ID ohne Fremd-Crate (ADR 0003).
///
/// Aufbau: `lx-<zeit-hex>-<zufall-hex>`. Die Zeitkomponente (ns seit Epoche)
/// sorgt für grobe Sortierbarkeit, der Zufallsteil verhindert Kollisionen bei
/// schnellen Aufrufen. Reicht für lokale Identität und späteren Charon-Abgleich.
pub fn gen_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Einfacher, ausreichend gestreuter Zufall aus Adresse + Zeit (kein rand-Crate).
    let seed = nanos ^ ((&nanos as *const _ as u128).wrapping_mul(0x9E37_79B9));
    let rand = splitmix64(seed as u64);
    format!("lx-{:x}-{:x}", nanos as u64, rand)
}

/// Kleiner SplitMix64-Streuer für die Zufallskomponente von `gen_id`.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Aktueller Zeitpunkt als ISO-8601-artiger UTC-String (`YYYY-MM-DDTHH:MM:SSZ`).
///
/// Bewusst ohne `chrono`/`time`: rechnet aus den Sekunden seit Epoche selbst
/// (proleptischer gregorianischer Kalender). Genügt für Anzeige und Vergleich.
pub fn now_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_iso8601(secs)
}

/// Formatiert Sekunden-seit-Epoche als `YYYY-MM-DDTHH:MM:SSZ` (UTC).
pub(crate) fn format_iso8601(secs: u64) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Tage seit 1970-01-01 → (Jahr, Monat, Tag). Howard Hinnants Standard-Algorithmus.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}
