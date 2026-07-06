//! GUI-Einstellungen (Panel-System, Theming, Arbeitsplatz) — ADR 0002.
//!
//! Auflösungsunabhängiges Panel-Layout: Panel-Positionen werden als
//! **Bruchteile** des Fensters gespeichert (0…1), nie als Pixel. Dadurch sitzt
//! dasselbe Layout auf FullHD und WQHD an der gleichen relativen Stelle. Panele
//! werden frei positioniert und skaliert (kein Raster/Snap) — siehe ADR §1.
//!
//! Das Modell ist UI-frei und testbar. Die Tauri-GUI liest/schreibt es über
//! Commands; die lokale JSON ist offline-first und später von Charon pro
//! Arbeitsplatz synchronisierbar (ADR §4). Der Editier-Modus selbst ist
//! flüchtig und **nicht** Teil dieser Struktur.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::project::data_root;

/// Dateiname der GUI-Settings im Datenverzeichnis.
pub const UI_SETTINGS_FILE: &str = "gui-settings.json";

/// Aktuelle Formatversion der GUI-Settings.
pub const UI_FORMAT_VERSION: u32 = 1;

/// Reiter der Oberfläche. Jeder Reiter hat ein eigenes Layout (ADR §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tab {
    Design,
    Laser,
    Monitor,
}

impl Tab {
    /// Alle Reiter in Anzeige-Reihenfolge.
    pub const ALL: [Tab; 3] = [Tab::Design, Tab::Laser, Tab::Monitor];
}

/// Fachliche Panele. Als Enum (nicht Strings), damit Layouts typsicher und
/// gegen Tippfehler geschützt sind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanelKind {
    Werkzeuge,
    Ebenen,
    Farbpalette,
    Anordnen,
    Laser,
    JobStatus,
}

/// Position und Größe eines Panels als **Bruchteile** des Fensters (0…1).
///
/// `x`/`y` ist die linke obere Ecke, `w`/`h` die Ausdehnung — jeweils relativ
/// zur Fensterbreite/-höhe, stufenlos (kein Raster). `z` bestimmt die Stapel-
/// Reihenfolge bei Überlappung — Panele dürfen sich frei überlappen, es gibt
/// keine Kollisionslogik.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PanelRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub z: i32,
}

impl PanelRect {
    /// Klemmt die Bruchteile in den gültigen Bereich, sodass das Panel im
    /// Fenster bleibt (Startpunkt 0…1, Ausdehnung passt nicht über den Rand).
    pub fn clamped(self) -> Self {
        let w = self.w.clamp(0.02, 1.0);
        let h = self.h.clamp(0.02, 1.0);
        let x = self.x.clamp(0.0, 1.0 - w);
        let y = self.y.clamp(0.0, 1.0 - h);
        Self {
            x,
            y,
            w,
            h,
            z: self.z,
        }
    }
}

/// Ein sichtbares Panel in einem Reiter-Layout.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PanelPlacement {
    pub kind: PanelKind,
    pub rect: PanelRect,
}

/// Layout eines einzelnen Reiters: welche Panele sichtbar sind und wo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabLayout {
    pub tab: Tab,
    pub panels: Vec<PanelPlacement>,
}

/// Eine Theme-Farbe: Farbton (RGB) plus geklemmte Intensität (ADR §3).
///
/// `intensity` ist 0…1, wird aber auf einen **lesbaren Korridor** geklemmt,
/// damit man sich nicht in Unlesbarkeit (zu blass) oder Grellheit regeln kann.
/// Die Anwendung des Reglers auf Sättigung/Helligkeit macht das Frontend über
/// CSS-Variablen; hier wird nur der Wert sicher gehalten.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ThemeColor {
    pub hue: [u8; 3],
    pub intensity: f64,
}

/// Untere/obere Grenze des lesbaren Intensitäts-Korridors (ADR §3).
pub const INTENSITY_MIN: f64 = 0.3;
pub const INTENSITY_MAX: f64 = 0.9;

impl ThemeColor {
    pub fn new(hue: [u8; 3], intensity: f64) -> Self {
        Self {
            hue,
            intensity: intensity.clamp(INTENSITY_MIN, INTENSITY_MAX),
        }
    }

    /// Klemmt die Intensität in den lesbaren Korridor.
    pub fn clamped(self) -> Self {
        Self {
            hue: self.hue,
            intensity: self.intensity.clamp(INTENSITY_MIN, INTENSITY_MAX),
        }
    }
}

/// Theming: Glassmorphism-Grundstil (fest) plus zwei einstellbare Farben.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    /// Akzent: aktive Werkzeuge, Auswahl, Handles, Hervorhebungen.
    pub accent: ThemeColor,
    /// Button-Grundfläche, getrennt einstellbar für Sichtbarkeit/Kontrast.
    pub button: ThemeColor,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            accent: ThemeColor::new([0x3B, 0x82, 0xF6], 0.7), // Blau, kräftig
            button: ThemeColor::new([0x60, 0x66, 0x70], 0.5), // neutrales Grau
        }
    }
}

impl Theme {
    pub fn clamped(self) -> Self {
        Theme {
            accent: self.accent.clamped(),
            button: self.button.clamped(),
        }
    }
}

/// Vollständige GUI-Einstellungen eines Arbeitsplatzes (ADR §4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSettings {
    pub version: u32,
    /// Arbeitsplatzname (z. B. „Werkstatt-PC"). Später von Charon als Schlüssel
    /// zum Synchronisieren genutzt.
    pub workplace: String,
    pub theme: Theme,
    /// Ein Layout je Reiter.
    pub layouts: Vec<TabLayout>,
}

impl Default for UiSettings {
    fn default() -> Self {
        UiSettings {
            version: UI_FORMAT_VERSION,
            workplace: "Arbeitsplatz".into(),
            theme: Theme::default(),
            layouts: Tab::ALL.iter().map(|&t| default_layout(t)).collect(),
        }
    }
}

impl UiSettings {
    /// Layout eines Reiters (oder `None`, wenn nicht vorhanden).
    pub fn layout(&self, tab: Tab) -> Option<&TabLayout> {
        self.layouts.iter().find(|l| l.tab == tab)
    }

    /// Setzt den aktuellen Reiter auf sein eingebautes Standard-Layout zurück
    /// (ADR §2). Andere Reiter bleiben unberührt.
    pub fn reset_tab(&mut self, tab: Tab) {
        let def = default_layout(tab);
        if let Some(slot) = self.layouts.iter_mut().find(|l| l.tab == tab) {
            *slot = def;
        } else {
            self.layouts.push(def);
        }
    }

    /// Räumt geladene Settings auf: fehlende Reiter ergänzen, Werte klemmen.
    /// Macht das Laden robust gegen alte/fehlerhafte Dateien.
    pub fn sanitize(&mut self) {
        self.theme = self.theme.clamped();
        for tab in Tab::ALL {
            if !self.layouts.iter().any(|l| l.tab == tab) {
                self.layouts.push(default_layout(tab));
            }
        }
        for layout in &mut self.layouts {
            for p in &mut layout.panels {
                p.rect = p.rect.clamped();
            }
        }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut s: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        s.sanitize();
        Ok(s)
    }

    /// Speichert nach `<data_root>/gui-settings.json`.
    pub fn save(&self) -> Result<PathBuf, String> {
        self.save_to(&data_root())
    }

    /// Speichert in ein beliebiges Verzeichnis (für Tests).
    pub fn save_to(&self, dir: &Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let path = dir.join(UI_SETTINGS_FILE);
        std::fs::write(&path, self.to_json()?).map_err(|e| e.to_string())?;
        Ok(path)
    }

    /// Lädt aus dem Datenverzeichnis; fehlt die Datei, gilt der Default.
    pub fn load() -> Self {
        Self::load_from(&data_root())
    }

    /// Lädt aus einem Verzeichnis; fehlt/kaputt → Default (nie ein Fehler nach
    /// außen, die GUI soll immer starten).
    pub fn load_from(dir: &Path) -> Self {
        let path = dir.join(UI_SETTINGS_FILE);
        match std::fs::read_to_string(&path) {
            Ok(json) => Self::from_json(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }
}

/// Eingebautes Standard-Layout je Reiter (ADR §2). Bewusst schlicht und
/// sinnvoll: Design zeigt die Editier-Panele, Laser die Steuerung, Monitor
/// (später) den Job-Status.
pub fn default_layout(tab: Tab) -> TabLayout {
    // Kleiner Helfer für lesbare Bruchteil-Rechtecke.
    let r = |x: f64, y: f64, w: f64, h: f64, z: i32| PanelRect { x, y, w, h, z };
    let p = |kind: PanelKind, rect: PanelRect| PanelPlacement { kind, rect };

    let panels = match tab {
        Tab::Design => vec![
            // Werkzeuge links oben, schmal.
            p(PanelKind::Werkzeuge, r(0.0, 0.0, 0.12, 0.5, 0)),
            // Ebenen rechts oben.
            p(PanelKind::Ebenen, r(0.80, 0.0, 0.20, 0.6, 0)),
            // Farbpalette rechts unten.
            p(PanelKind::Farbpalette, r(0.80, 0.62, 0.20, 0.18, 0)),
            // Anordnen-Toolbar oben mittig, flach.
            p(PanelKind::Anordnen, r(0.14, 0.0, 0.40, 0.08, 0)),
        ],
        Tab::Laser => vec![
            p(PanelKind::Ebenen, r(0.0, 0.0, 0.20, 0.6, 0)),
            // Laser-Control unten rechts (wie bisher).
            p(PanelKind::Laser, r(0.78, 0.30, 0.22, 0.68, 0)),
        ],
        Tab::Monitor => vec![
            // Vorerst nur ein Platzhalter-Panel; Inhalt folgt später.
            p(PanelKind::JobStatus, r(0.70, 0.0, 0.30, 0.5, 0)),
        ],
    };
    TabLayout { tab, panels }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hat_ein_layout_je_reiter() {
        let s = UiSettings::default();
        assert_eq!(s.layouts.len(), 3);
        for tab in Tab::ALL {
            assert!(s.layout(tab).is_some(), "{tab:?} fehlt");
        }
    }

    #[test]
    fn intensitaet_wird_auf_korridor_geklemmt() {
        assert_eq!(ThemeColor::new([0, 0, 0], 0.0).intensity, INTENSITY_MIN);
        assert_eq!(ThemeColor::new([0, 0, 0], 1.0).intensity, INTENSITY_MAX);
        assert_eq!(ThemeColor::new([0, 0, 0], 0.5).intensity, 0.5);
    }

    #[test]
    fn panelrect_bleibt_im_fenster() {
        // Startpunkt außerhalb, Ausdehnung zu groß → wird zurechtgeklemmt.
        let c = PanelRect {
            x: 1.5,
            y: -0.2,
            w: 2.0,
            h: 0.3,
            z: 0,
        }
        .clamped();
        assert!(c.x >= 0.0 && c.x + c.w <= 1.0 + f64::EPSILON);
        assert!(c.y >= 0.0 && c.y + c.h <= 1.0 + f64::EPSILON);
    }

    #[test]
    fn reset_tab_setzt_nur_den_reiter_zurueck() {
        let mut s = UiSettings::default();
        // Design-Layout verändern.
        if let Some(l) = s.layouts.iter_mut().find(|l| l.tab == Tab::Design) {
            l.panels.clear();
        }
        s.reset_tab(Tab::Design);
        assert!(!s.layout(Tab::Design).unwrap().panels.is_empty());
    }

    #[test]
    fn json_roundtrip_und_sanitize() {
        let s = UiSettings::default();
        let json = s.to_json().unwrap();
        let back = UiSettings::from_json(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn fehlender_reiter_wird_beim_laden_ergaenzt() {
        let mut s = UiSettings::default();
        s.layouts.retain(|l| l.tab != Tab::Monitor);
        let json = serde_json::to_string(&s).unwrap();
        let back = UiSettings::from_json(&json).unwrap();
        assert!(back.layout(Tab::Monitor).is_some(), "Monitor ergänzt");
    }

    #[test]
    fn save_und_load_ueber_tempdir() {
        let dir = std::env::temp_dir().join(format!("luxifer_ui_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let mut s = UiSettings::default();
        s.workplace = "Werkstatt-PC".into();
        let path = s.save_to(&dir).unwrap();
        assert!(path.exists());

        let loaded = UiSettings::load_from(&dir);
        assert_eq!(loaded.workplace, "Werkstatt-PC");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fehlende_datei_gibt_default() {
        let dir = std::env::temp_dir().join(format!("luxifer_ui_none_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let loaded = UiSettings::load_from(&dir);
        assert_eq!(loaded, UiSettings::default());
    }
}
