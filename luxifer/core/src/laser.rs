//! Laser-Profile: gespeicherte Maschinen (ADR 0007). App-global, projekt-
//! übergreifend — ein Laser gehört zur Werkstatt, nicht zum Projekt.
//!
//! Der Core hält die Profil-**Liste** und die **aktive** Auswahl I/O-frei und
//! testbar. Laden/Speichern (JSON-Datei) macht das Backend. Die Typen sind
//! geräteneutral: `DriverKind` sagt nur, *welcher* Treiber instanziiert wird;
//! die Byte-/Transport-Details kennen allein die Treiber-Crates.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Dateiname der app-globalen Laser-Registry.
pub const LASER_FILE: &str = "laser-profile.json";

/// Welcher Treiber ein Profil bedient. Bestimmt, welche `MachineDriver`-
/// Implementierung die App erzeugt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DriverKind {
    #[default]
    Ruida,
    Grbl,
    MiniGrbl,
}

/// Verbindungsparameter eines Lasers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "art", rename_all = "lowercase")]
pub enum Connection {
    /// Netzwerk (Ruida: UDP). `port` optional — der Treiber kennt seinen Standard.
    Netz { ip: String, port: Option<u16> },
    /// Serielle Schnittstelle (GRBL/Marlin).
    Seriell { port: String, baud: u32 },
}

impl Default for Connection {
    fn default() -> Self {
        Connection::Netz {
            ip: "192.168.1.100".into(),
            port: None,
        }
    }
}

/// Ein Stützpunkt der Scan-Offset-Kurve (Reversal-Kalibrierung, ADR 0006 §6).
/// Geräteneutral gespeichert; angewandt wird der Offset nur vom Treiber.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScanOffsetPoint {
    pub speed_mm_s: f64,
    pub offset_mm: f64,
}

/// Geschwindigkeitsabhängige Scan-Offset-Kalibrierung (Tabelle).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScanOffsetCal {
    pub enabled: bool,
    pub points: Vec<ScanOffsetPoint>,
}

/// Ein gespeicherter Laser (Werkstatt-Gerät).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaserProfile {
    /// Eindeutige ID (vom Backend vergeben, z. B. UUID/Zeitstempel).
    pub id: String,
    /// Freier Anzeigename, z. B. „Ruida groß (Keller)".
    pub name: String,
    pub kind: DriverKind,
    #[serde(default)]
    pub connection: Connection,
    /// Arbeitsbereich B×H in mm.
    pub bed_mm: (f64, f64),
    /// Reversal-Kalibrierung (nur für Treiber relevant, die sie nutzen).
    #[serde(default)]
    pub scan_offset: ScanOffsetCal,
}

impl Default for LaserProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Neuer Laser".into(),
            kind: DriverKind::Ruida,
            connection: Connection::default(),
            bed_mm: (600.0, 400.0),
            scan_offset: ScanOffsetCal::default(),
        }
    }
}

/// Die app-globale Laser-Registry: Liste + aktive Auswahl. I/O-frei; das Backend
/// persistiert sie als JSON.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LaserRegistry {
    #[serde(default)]
    pub profiles: Vec<LaserProfile>,
    /// ID des aktiven Lasers (im Panel-Dropdown gewählt). `None` → keiner aktiv.
    #[serde(default)]
    pub active_id: Option<String>,
}

impl LaserRegistry {
    /// Das aktive Profil (oder `None`, wenn keins gewählt/vorhanden).
    pub fn active(&self) -> Option<&LaserProfile> {
        let id = self.active_id.as_deref()?;
        self.profiles.iter().find(|p| p.id == id)
    }

    /// Profil hinzufügen. Ist es das erste, wird es automatisch aktiv.
    pub fn add(&mut self, profile: LaserProfile) {
        if self.active_id.is_none() {
            self.active_id = Some(profile.id.clone());
        }
        self.profiles.push(profile);
    }

    /// Profil ersetzen (gleiche ID). `true`, wenn ein Profil ersetzt wurde.
    pub fn update(&mut self, profile: LaserProfile) -> bool {
        if let Some(slot) = self.profiles.iter_mut().find(|p| p.id == profile.id) {
            *slot = profile;
            true
        } else {
            false
        }
    }

    /// Profil löschen. War es das aktive, rückt das erste verbleibende nach.
    pub fn remove(&mut self, id: &str) {
        self.profiles.retain(|p| p.id != id);
        if self.active_id.as_deref() == Some(id) {
            self.active_id = self.profiles.first().map(|p| p.id.clone());
        }
    }

    /// Aktiven Laser setzen, sofern die ID existiert.
    pub fn set_active(&mut self, id: &str) -> bool {
        if self.profiles.iter().any(|p| p.id == id) {
            self.active_id = Some(id.to_string());
            true
        } else {
            false
        }
    }

    // --- Persistenz (app-global, eigene Datei; ADR 0007) --------------------

    /// Lädt die Registry aus dem Datenverzeichnis; fehlt/kaputt → leer (die GUI
    /// startet immer).
    pub fn load() -> Self {
        Self::load_from(&crate::project::data_root())
    }

    /// Lädt aus einem Verzeichnis (für Tests).
    pub fn load_from(dir: &Path) -> Self {
        match std::fs::read_to_string(dir.join(LASER_FILE)) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Speichert nach `<data_root>/laser-profile.json`.
    pub fn save(&self) -> Result<PathBuf, String> {
        self.save_to(&crate::project::data_root())
    }

    /// Speichert in ein beliebiges Verzeichnis (für Tests).
    pub fn save_to(&self, dir: &Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let path = dir.join(LASER_FILE);
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(path)
    }
}

/// Eine Job-Aktion, die ein Treiber im Laserpanel anbietet (ADR 0007). Das Panel
/// rendert nur, was der aktive Treiber via `actions()` meldet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobAction {
    /// Ruida: Job kompilieren + per UDP senden/starten.
    SendJob,
    /// GRBL: G-Code streamen.
    StreamGcode,
    /// Job-Bytes in eine Datei exportieren (.rd bzw. .gcode).
    ExportFile,
    /// Bounding-Box abfahren (Platzierung prüfen).
    Frame,
    /// Konvexe Außenkontur des Jobs abfahren (Gummiband).
    RubberFrame,
    /// Laufenden Job pausieren oder fortsetzen.
    Pause,
    /// Referenzfahrt (Maschinen-Null 0/0).
    Home,
    /// Zum Benutzerursprung fahren.
    GoOrigin,
    /// Sofort-Stopp.
    Stop,
}

impl JobAction {
    /// Stabiler String-Schlüssel fürs Frontend.
    pub fn as_key(self) -> &'static str {
        match self {
            JobAction::SendJob => "send_job",
            JobAction::StreamGcode => "stream_gcode",
            JobAction::ExportFile => "export_file",
            JobAction::Frame => "frame",
            JobAction::RubberFrame => "rubber_frame",
            JobAction::Pause => "pause",
            JobAction::Home => "home",
            JobAction::GoOrigin => "go_origin",
            JobAction::Stop => "stop",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profil(id: &str) -> LaserProfile {
        LaserProfile {
            id: id.into(),
            ..Default::default()
        }
    }

    #[test]
    fn erstes_profil_wird_aktiv() {
        let mut r = LaserRegistry::default();
        r.add(profil("a"));
        assert_eq!(r.active_id.as_deref(), Some("a"));
        r.add(profil("b"));
        assert_eq!(
            r.active_id.as_deref(),
            Some("a"),
            "zweites ändert aktiv nicht"
        );
    }

    #[test]
    fn loeschen_des_aktiven_rueckt_nach() {
        let mut r = LaserRegistry::default();
        r.add(profil("a"));
        r.add(profil("b"));
        r.remove("a");
        assert_eq!(r.active_id.as_deref(), Some("b"));
    }

    #[test]
    fn loeschen_des_letzten_leert_aktiv() {
        let mut r = LaserRegistry::default();
        r.add(profil("a"));
        r.remove("a");
        assert_eq!(r.active_id, None);
        assert!(r.active().is_none());
    }

    #[test]
    fn set_active_nur_bei_existenz() {
        let mut r = LaserRegistry::default();
        r.add(profil("a"));
        assert!(!r.set_active("x"));
        assert!(r.set_active("a"));
        assert_eq!(r.active().unwrap().id, "a");
    }

    #[test]
    fn update_ersetzt_gleiche_id() {
        let mut r = LaserRegistry::default();
        r.add(profil("a"));
        let mut p = profil("a");
        p.name = "Umbenannt".into();
        assert!(r.update(p));
        assert_eq!(r.active().unwrap().name, "Umbenannt");
    }

    #[test]
    fn save_und_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("luxifer-laser-test-{}", std::process::id()));
        let mut r = LaserRegistry::default();
        let mut p = profil("a");
        p.scan_offset = ScanOffsetCal {
            enabled: true,
            points: vec![ScanOffsetPoint {
                speed_mm_s: 100.0,
                offset_mm: 0.1,
            }],
        };
        r.add(p);
        r.save_to(&dir).unwrap();
        let loaded = LaserRegistry::load_from(&dir);
        assert_eq!(loaded, r);
        std::fs::remove_dir_all(&dir).ok();
    }
}
