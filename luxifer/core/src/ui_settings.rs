//! GUI-Einstellungen (Theming, Arbeitsplatz) — ADR 0002.
//!
//! Das Panel-Layout ist statisch im Frontend verdrahtet. Persistiert werden nur
//! die Arbeitsplatzdaten, Theme-Farben und der zuletzt verwendete Projektname.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::project::data_root;

/// Dateiname der GUI-Settings im Datenverzeichnis.
pub const UI_SETTINGS_FILE: &str = "gui-settings.json";

/// Aktuelle Formatversion der GUI-Settings.
pub const UI_FORMAT_VERSION: u32 = 1;

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
    /// Zuletzt geöffnetes Projekt (Ordnername) für den Start-Toast (ADR 0003).
    /// Leer = kein zuletzt-Projekt. `#[serde(default)]` für alte Settings.
    #[serde(default)]
    pub last_project: String,
}

impl Default for UiSettings {
    fn default() -> Self {
        UiSettings {
            version: UI_FORMAT_VERSION,
            workplace: "Arbeitsplatz".into(),
            theme: Theme::default(),
            last_project: String::new(),
        }
    }
}

impl UiSettings {
    /// Räumt geladene Settings auf: Werte klemmen.
    /// Macht das Laden robust gegen alte/fehlerhafte Dateien.
    pub fn sanitize(&mut self) {
        self.theme = self.theme.clamped();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intensitaet_wird_auf_korridor_geklemmt() {
        assert_eq!(ThemeColor::new([0, 0, 0], 0.0).intensity, INTENSITY_MIN);
        assert_eq!(ThemeColor::new([0, 0, 0], 1.0).intensity, INTENSITY_MAX);
        assert_eq!(ThemeColor::new([0, 0, 0], 0.5).intensity, 0.5);
    }

    #[test]
    fn json_roundtrip_und_sanitize() {
        let s = UiSettings::default();
        let json = s.to_json().unwrap();
        let back = UiSettings::from_json(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn alte_layout_felder_werden_ignoriert() {
        let json = r#"{
            "version": 1,
            "workplace": "Alt",
            "theme": {
                "accent": { "hue": [59, 130, 246], "intensity": 1.2 },
                "button": { "hue": [96, 102, 112], "intensity": 0.1 }
            },
            "layouts": [{ "tab": "Design", "panels": [] }],
            "last_project": "Text"
        }"#;
        let back = UiSettings::from_json(json).unwrap();
        assert_eq!(back.workplace, "Alt");
        assert_eq!(back.theme.accent.intensity, INTENSITY_MAX);
        assert_eq!(back.theme.button.intensity, INTENSITY_MIN);
        assert_eq!(back.last_project, "Text");
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
