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

/// Höchste Laserprofil-Schemaversion, die dieses Studio vollständig versteht
/// (ADR 0020). Version 1 = Profile ohne `saved_origins`, Version 2 = mit
/// benannten Werkstück-Nullpunkten. Studio schreibt immer diese Version.
pub const LASER_PROFILE_SCHEMA_VERSION: u32 = 2;

/// Profile ohne `schema_version`-Feld stammen aus der Zeit vor ADR 0020.
fn default_schema_version() -> u32 {
    1
}

/// Welcher Treiber ein Profil bedient. Bestimmt, welche `MachineDriver`-
/// Implementierung die App erzeugt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DriverKind {
    #[default]
    Ruida,
    Grbl,
    MiniGrbl,
}

/// Lage des Maschinen-Nullpunkts am Arbeitsbett. Die Editor-Geometrie bleibt
/// immer links-oben orientiert; vor dem Treiber wird in dieses Koordinatensystem
/// transformiert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BedOrigin {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl BedOrigin {
    pub fn transform(self, x: f64, y: f64, bed: (f64, f64)) -> (f64, f64) {
        let x = if matches!(self, Self::TopRight | Self::BottomRight) {
            bed.0 - x
        } else {
            x
        };
        let y = if matches!(self, Self::BottomLeft | Self::BottomRight) {
            bed.1 - y
        } else {
            y
        };
        (x, y)
    }

    /// Hält den im Canvas gewählten 3×3-Jobanker beim Spiegeln an derselben
    /// sichtbaren Ecke/Kante der Geometrie.
    pub fn transform_anchor(self, anchor: crate::Anchor) -> crate::Anchor {
        use crate::Anchor::*;
        let horizontal = matches!(self, Self::TopRight | Self::BottomRight);
        let vertical = matches!(self, Self::BottomLeft | Self::BottomRight);
        match (anchor, horizontal, vertical) {
            (NW, false, false) | (NE, true, false) | (SW, false, true) | (SE, true, true) => NW,
            (N, _, false) | (S, _, true) => N,
            (NE, false, false) | (NW, true, false) | (SE, false, true) | (SW, true, true) => NE,
            (W, false, _) | (E, true, _) => W,
            (Center, _, _) => Center,
            (E, false, _) | (W, true, _) => E,
            (SW, false, false) | (SE, true, false) | (NW, false, true) | (NE, true, true) => SW,
            (S, _, false) | (N, _, true) => S,
            (SE, false, false) | (SW, true, false) | (NE, false, true) | (NW, true, true) => SE,
        }
    }
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

/// Ein benannter Werkstück-Nullpunkt (ADR 0020): stabile Identität mit Name
/// und **absoluten Maschinenkoordinaten**. Gehört genau zu einem Laserprofil.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedOrigin {
    pub id: String,
    pub name: String,
    pub x_mm: f64,
    pub y_mm: f64,
}

impl SavedOrigin {
    /// Formale Gültigkeit (ID/Name nicht leer, Koordinaten endlich). Die
    /// Bettgrenzen prüft [`LaserProfile::saved_origin_usable`], weil sie sich
    /// mit dem Profil ändern können.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("Nullpunkt ohne ID.".into());
        }
        if self.name.trim().is_empty() {
            return Err("Nullpunkt ohne Namen.".into());
        }
        if !self.x_mm.is_finite() || !self.y_mm.is_finite() {
            return Err(format!(
                "Nullpunkt „{}“ hat ungültige Koordinaten.",
                self.name
            ));
        }
        Ok(())
    }
}

/// Zusatzachsen-Konfiguration eines Lasers (ADR 0021). Ob Z/U vorhanden sind,
/// steht NICHT im Controller (an Hardware verifiziert) — es ist eine
/// Profil-Einstellung. `invert_*` dreht die fachliche Richtung pro Achse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AxisConfig {
    /// Maschine hat eine Z-Achse (Fokus/Betthöhe).
    #[serde(default)]
    pub has_z_axis: bool,
    /// Maschine hat eine U-Achse (Rotary über den U-Ausgang).
    #[serde(default)]
    pub has_u_axis: bool,
    /// Z-Richtung umkehren (Verkabelung/Blickrichtung).
    #[serde(default)]
    pub invert_z: bool,
    /// U-Richtung umkehren.
    #[serde(default)]
    pub invert_u: bool,
}

/// Ein gespeicherter Laser (Werkstatt-Gerät).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaserProfile {
    /// Eindeutige ID (vom Backend vergeben, z. B. UUID/Zeitstempel).
    pub id: String,
    /// Freier Anzeigename, z. B. „Ruida groß (Keller)".
    pub name: String,
    /// Payload-Schemaversion (ADR 0020). Fehlend = 1; mit `saved_origins` = 2.
    /// Der Hub lehnt ein Zurückschreiben mit kleinerer Version ab.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub kind: DriverKind,
    #[serde(default)]
    pub connection: Connection,
    /// Arbeitsbereich B×H in mm.
    pub bed_mm: (f64, f64),
    /// Maschinen-Nullpunkt an einer der vier Bettecken.
    #[serde(default)]
    pub origin: BedOrigin,
    /// Reversal-Kalibrierung (nur für Treiber relevant, die sie nutzen).
    #[serde(default)]
    pub scan_offset: ScanOffsetCal,
    /// Zusatzachsen (Z/U vorhanden, Richtungs-Inversion). Profil-Einstellung,
    /// da nicht aus dem Controller lesbar (ADR 0021 §A).
    #[serde(default)]
    pub axes: AxisConfig,
    /// Benannte Werkstück-Nullpunkte (absolute Maschinenkoordinaten).
    #[serde(default)]
    pub saved_origins: Vec<SavedOrigin>,
}

impl Default for LaserProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Neuer Laser".into(),
            schema_version: LASER_PROFILE_SCHEMA_VERSION,
            kind: DriverKind::Ruida,
            connection: Connection::default(),
            bed_mm: (600.0, 400.0),
            origin: BedOrigin::default(),
            scan_offset: ScanOffsetCal::default(),
            axes: AxisConfig::default(),
            saved_origins: Vec::new(),
        }
    }
}

impl LaserProfile {
    /// Nullpunkt per stabiler ID (niemals per Name oder Index).
    pub fn saved_origin(&self, id: &str) -> Option<&SavedOrigin> {
        self.saved_origins.iter().find(|origin| origin.id == id)
    }

    /// Ob ein Eintrag mit der aktuellen Bettgeometrie nutzbar ist. Ungültige
    /// Einträge bleiben sichtbar, dürfen aber weder angefahren noch als
    /// Jobreferenz verwendet werden (ADR 0020 §C).
    pub fn saved_origin_usable(&self, origin: &SavedOrigin) -> bool {
        origin.validate().is_ok()
            && origin.x_mm >= 0.0
            && origin.y_mm >= 0.0
            && origin.x_mm <= self.bed_mm.0
            && origin.y_mm <= self.bed_mm.1
    }

    /// Prüft die gesamte Nullpunktliste: formale Gültigkeit jedes Eintrags und
    /// keine doppelten IDs/Namen. Bettgrenzen sind hier bewusst KEIN Fehler —
    /// nach einer Bettverkleinerung bleibt der Eintrag zur Korrektur sichtbar.
    pub fn validate_saved_origins(&self) -> Result<(), String> {
        for origin in &self.saved_origins {
            origin.validate()?;
        }
        for (index, origin) in self.saved_origins.iter().enumerate() {
            for other in &self.saved_origins[index + 1..] {
                if other.id == origin.id {
                    return Err(format!("Doppelte Nullpunkt-ID „{}“.", origin.id));
                }
                if other.name.trim() == origin.name.trim() {
                    return Err(format!("Doppelter Nullpunkt-Name „{}“.", origin.name));
                }
            }
        }
        Ok(())
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

    /// Lädt aus einem Verzeichnis (für Tests). Ungültige Nullpunktdaten
    /// (doppelte IDs, nicht endliche Koordinaten) gelten als beschädigte
    /// Profildatei und werden abgelehnt — nicht still umgedeutet (ADR 0020).
    pub fn load_from(dir: &Path) -> Self {
        let registry: Self = match std::fs::read_to_string(dir.join(LASER_FILE)) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        };
        if registry
            .profiles
            .iter()
            .any(|profile| profile.validate_saved_origins().is_err())
        {
            return Self::default();
        }
        registry
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
    fn altes_profil_ohne_nullpunkt_bleibt_oben_links() {
        let json = r#"{
            "id":"alt","name":"Alt","kind":"Ruida",
            "connection":{"art":"netz","ip":"127.0.0.1","port":null},
            "bed_mm":[300.0,200.0]
        }"#;
        let profile: LaserProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.origin, BedOrigin::TopLeft);
    }

    #[test]
    fn altes_profil_gilt_als_schema_version_1_mit_leerer_nullpunktliste() {
        let json = r#"{
            "id":"alt","name":"Alt","kind":"Ruida",
            "connection":{"art":"netz","ip":"127.0.0.1","port":null},
            "bed_mm":[300.0,200.0]
        }"#;
        let profile: LaserProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.schema_version, 1);
        assert!(profile.saved_origins.is_empty());
        // Neue Profile schreiben immer die höchste verstandene Version.
        assert_eq!(
            LaserProfile::default().schema_version,
            LASER_PROFILE_SCHEMA_VERSION
        );
    }

    fn origin(id: &str, name: &str, x: f64, y: f64) -> SavedOrigin {
        SavedOrigin {
            id: id.into(),
            name: name.into(),
            x_mm: x,
            y_mm: y,
        }
    }

    #[test]
    fn nullpunkt_validierung_lehnt_leeres_und_doppeltes_ab() {
        assert!(origin("a", "Posi", 10.0, 20.0).validate().is_ok());
        assert!(origin("a", "  ", 10.0, 20.0).validate().is_err());
        assert!(origin("", "Posi", 10.0, 20.0).validate().is_err());
        assert!(origin("a", "Posi", f64::NAN, 20.0).validate().is_err());

        let mut profile = LaserProfile {
            saved_origins: vec![origin("a", "Posi", 1.0, 2.0)],
            ..Default::default()
        };
        assert!(profile.validate_saved_origins().is_ok());
        profile.saved_origins.push(origin("a", "Anders", 3.0, 4.0));
        assert!(profile.validate_saved_origins().is_err(), "doppelte ID");
        profile.saved_origins[1] = origin("b", "Posi", 3.0, 4.0);
        assert!(profile.validate_saved_origins().is_err(), "doppelter Name");
    }

    #[test]
    fn nullpunkt_ausserhalb_des_betts_bleibt_sichtbar_aber_unbrauchbar() {
        let mut profile = LaserProfile {
            bed_mm: (300.0, 200.0),
            saved_origins: vec![origin("a", "Posi", 250.0, 150.0)],
            ..Default::default()
        };
        assert!(profile.saved_origin_usable(&profile.saved_origins[0]));
        // Bett schrumpft: Eintrag bleibt in der Liste, ist aber ungültig.
        profile.bed_mm = (200.0, 100.0);
        assert!(profile.validate_saved_origins().is_ok());
        assert!(!profile.saved_origin_usable(&profile.saved_origins[0]));
        assert!(profile.saved_origin("a").is_some());
        assert!(profile.saved_origin("fehlt").is_none());
    }

    #[test]
    fn registry_mit_beschaedigten_nullpunkten_wird_abgelehnt() {
        let dir = std::env::temp_dir().join(format!(
            "studio-laser-invalid-{}-{}",
            std::process::id(),
            line!()
        ));
        let mut registry = LaserRegistry::default();
        let mut profile = profil("a");
        profile.saved_origins = vec![
            origin("o", "Posi", 1.0, 1.0),
            origin("o", "Posi 2", 2.0, 2.0),
        ];
        registry.add(profile);
        // Direkt schreiben (ohne Validierung), um eine beschädigte Datei zu
        // simulieren.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(LASER_FILE),
            serde_json::to_string_pretty(&registry).unwrap(),
        )
        .unwrap();
        assert_eq!(LaserRegistry::load_from(&dir), LaserRegistry::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn nullpunkt_spiegelt_punkt_und_jobanker() {
        assert_eq!(
            BedOrigin::BottomRight.transform(25.0, 40.0, (300.0, 200.0)),
            (275.0, 160.0)
        );
        assert_eq!(
            BedOrigin::BottomRight.transform_anchor(crate::Anchor::NW),
            crate::Anchor::SE
        );
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
        let dir = std::env::temp_dir().join(format!("studio-laser-test-{}", std::process::id()));
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
