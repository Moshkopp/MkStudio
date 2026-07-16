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
pub const UI_FORMAT_VERSION: u32 = 2;

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

/// Semantische Oberflächenfarben. Dadurch verwenden Panels keine verstreuten
/// RGB-Literale und die visuelle Hierarchie bleibt über alle Ansichten stabil.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemePalette {
    pub background: [u8; 3],
    pub toolbar: [u8; 3],
    pub panel: [u8; 3],
    pub surface: [u8; 3],
    pub border: [u8; 3],
    pub text: [u8; 3],
    pub muted: [u8; 3],
    pub success: [u8; 3],
    pub danger: [u8; 3],
}

impl Default for ThemePalette {
    fn default() -> Self {
        Self {
            background: [0x11, 0x13, 0x18],
            toolbar: [0x18, 0x1b, 0x22],
            panel: [0x1e, 0x22, 0x2b],
            surface: [0x26, 0x2b, 0x35],
            border: [0x34, 0x3a, 0x46],
            text: [0xf2, 0xf3, 0xf5],
            muted: [0xa7, 0xaf, 0xbc],
            success: [0x4a, 0xde, 0x80],
            danger: [0xf8, 0x71, 0x71],
        }
    }
}

/// Theming: semantische Dark-Workshop-Palette plus einstellbare Aktionsfarben.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    /// Akzent: aktive Werkzeuge, Auswahl, Handles, Hervorhebungen.
    pub accent: ThemeColor,
    /// Button-Grundfläche, getrennt einstellbar für Sichtbarkeit/Kontrast.
    pub button: ThemeColor,
    #[serde(default)]
    pub palette: ThemePalette,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            accent: ThemeColor::new([0xf5, 0x9e, 0x42], 0.75),
            button: ThemeColor::new([0x43, 0x4b, 0x5a], 0.6),
            palette: ThemePalette::default(),
        }
    }
}

impl Theme {
    pub fn clamped(self) -> Self {
        Theme {
            accent: self.accent.clamped(),
            button: self.button.clamped(),
            palette: self.palette,
        }
    }
}

/// Vollständige GUI-Einstellungen eines Arbeitsplatzes (ADR §4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSettings {
    pub version: u32,
    /// Stabile, nicht vom sichtbaren Namen abhängige Arbeitsplatz-ID.
    #[serde(default = "default_workplace_id")]
    pub workplace_id: String,
    /// Sichtbarer Arbeitsplatzname (z. B. „Werkstatt-PC").
    pub workplace: String,
    pub theme: Theme,
    /// Zuletzt geöffnetes Projekt (Ordnername) für den Start-Toast (ADR 0003).
    /// Leer = kein zuletzt-Projekt. `#[serde(default)]` für alte Settings.
    #[serde(default)]
    pub last_project: String,
    /// Rasterweite des Design-Canvas in mm (parametrierbar). Wird auf einen
    /// sinnvollen Bereich geklemmt. `#[serde(default = …)]` für alte Settings.
    #[serde(default = "default_grid_size")]
    pub grid_size_mm: f64,
    /// Splash-Screen beim Start anzeigen (Logo + Version). Default an.
    #[serde(default = "default_true")]
    pub show_splash: bool,
    /// Mindest-Anzeigedauer des Splash in ms (auch bei blitzschnellem Start).
    /// Geklemmt auf einen sinnvollen Bereich.
    #[serde(default = "default_splash_ms")]
    pub splash_ms: u32,
    /// Deckkraft der dunklen Fläche hinter modalen Dialogen.
    #[serde(default = "default_modal_backdrop_alpha")]
    pub modal_backdrop_alpha: u8,
    /// Optionalen Charon-Koordinationsdienst verwenden.
    #[serde(default)]
    pub charon_enabled: bool,
    /// Basisadresse des Charon-Dienstes (ADR 0012).
    #[serde(default = "default_charon_url")]
    pub charon_url: String,
}

/// Default-Mindestdauer des Splash (ms).
fn default_splash_ms() -> u32 {
    1500
}

fn default_modal_backdrop_alpha() -> u8 {
    200
}

fn default_charon_url() -> String {
    "http://127.0.0.1:3737".into()
}

fn default_workplace_id() -> String {
    crate::datetime::gen_id()
}

/// Default-Rasterweite (mm), wenn eine alte Settings-Datei das Feld nicht hat.
/// Die Rasterweite ist der Abstand der FEINEN Gitterlinien; der native Canvas
/// zeichnet Hauptlinien alle 5 Schritte (10 → gewohntes 10/50-Bild).
fn default_grid_size() -> f64 {
    10.0
}

fn default_true() -> bool {
    true
}

/// Grenzen der Rasterweite (mm): fein genug für Details, grob genug fürs Bett.
pub const GRID_SIZE_MIN: f64 = 1.0;
pub const GRID_SIZE_MAX: f64 = 500.0;

/// Grenzen der Splash-Dauer (ms): 0 = quasi sofort, max 10 s.
pub const SPLASH_MS_MIN: u32 = 0;
pub const SPLASH_MS_MAX: u32 = 10_000;

impl Default for UiSettings {
    fn default() -> Self {
        UiSettings {
            version: UI_FORMAT_VERSION,
            workplace_id: default_workplace_id(),
            workplace: "Arbeitsplatz".into(),
            theme: Theme::default(),
            last_project: String::new(),
            grid_size_mm: default_grid_size(),
            show_splash: true,
            splash_ms: default_splash_ms(),
            modal_backdrop_alpha: default_modal_backdrop_alpha(),
            charon_enabled: false,
            charon_url: default_charon_url(),
        }
    }
}

impl UiSettings {
    /// Räumt geladene Settings auf: Werte klemmen.
    /// Macht das Laden robust gegen alte/fehlerhafte Dateien.
    pub fn sanitize(&mut self) {
        self.theme = self.theme.clamped();
        // Rasterweite in den sinnvollen Bereich klemmen; NaN → Default.
        if !self.grid_size_mm.is_finite() {
            self.grid_size_mm = default_grid_size();
        }
        self.grid_size_mm = self.grid_size_mm.clamp(GRID_SIZE_MIN, GRID_SIZE_MAX);
        self.splash_ms = self.splash_ms.clamp(SPLASH_MS_MIN, SPLASH_MS_MAX);
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut s: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if s.version < UI_FORMAT_VERSION {
            // Nur den früheren unveränderten Standard migrieren; bewusst
            // gewählte Benutzerfarben bleiben erhalten.
            if s.theme.accent.hue == [0x3b, 0x82, 0xf6] && s.theme.button.hue == [0x60, 0x66, 0x70]
            {
                let defaults = Theme::default();
                s.theme.accent = defaults.accent;
                s.theme.button = defaults.button;
            }
            s.version = UI_FORMAT_VERSION;
        }
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
            Ok(json) => {
                let had_workplace_id = serde_json::from_str::<serde_json::Value>(&json)
                    .ok()
                    .and_then(|value| value.get("workplace_id").cloned())
                    .is_some();
                let needs_format_upgrade = serde_json::from_str::<serde_json::Value>(&json)
                    .ok()
                    .and_then(|value| value.get("version").and_then(|v| v.as_u64()))
                    .is_none_or(|version| version < UI_FORMAT_VERSION as u64);
                let settings = Self::from_json(&json).unwrap_or_default();
                if !had_workplace_id || needs_format_upgrade {
                    let _ = settings.save_to(dir);
                }
                settings
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let settings = Self::default();
                let _ = settings.save_to(dir);
                settings
            }
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
        assert_eq!(back.version, UI_FORMAT_VERSION);
        assert_eq!(back.theme, Theme::default());
        assert_eq!(back.last_project, "Text");
        // Fehlende neue Felder fallen auf ihre Defaults zurück.
        assert_eq!(back.grid_size_mm, default_grid_size());
        assert!(back.show_splash);
        assert_eq!(back.splash_ms, default_splash_ms());
        assert_eq!(back.modal_backdrop_alpha, default_modal_backdrop_alpha());
        assert!(!back.charon_enabled);
        assert_eq!(back.charon_url, default_charon_url());
    }

    #[test]
    fn alte_benutzerfarben_bleiben_beim_palette_upgrade_erhalten() {
        let json = r#"{
            "version": 1,
            "workplace": "Alt",
            "theme": {
                "accent": { "hue": [10, 20, 30], "intensity": 0.6 },
                "button": { "hue": [40, 50, 60], "intensity": 0.7 }
            }
        }"#;

        let back = UiSettings::from_json(json).unwrap();

        assert_eq!(back.theme.accent.hue, [10, 20, 30]);
        assert_eq!(back.theme.button.hue, [40, 50, 60]);
        assert_eq!(back.theme.palette, ThemePalette::default());
    }

    #[test]
    fn splash_ms_wird_geklemmt() {
        let mut s = UiSettings {
            splash_ms: 99_999,
            ..UiSettings::default()
        };
        s.sanitize();
        assert_eq!(s.splash_ms, SPLASH_MS_MAX);
    }

    #[test]
    fn grid_size_wird_geklemmt() {
        let mut s = UiSettings {
            grid_size_mm: 0.0,
            ..UiSettings::default()
        };
        s.sanitize();
        assert_eq!(s.grid_size_mm, GRID_SIZE_MIN);

        let mut s = UiSettings {
            grid_size_mm: 9999.0,
            ..UiSettings::default()
        };
        s.sanitize();
        assert_eq!(s.grid_size_mm, GRID_SIZE_MAX);

        let mut s = UiSettings {
            grid_size_mm: f64::NAN,
            ..UiSettings::default()
        };
        s.sanitize();
        assert_eq!(s.grid_size_mm, default_grid_size());
    }

    #[test]
    fn save_und_load_ueber_tempdir() {
        let dir = std::env::temp_dir().join(format!("luxifer_ui_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let s = UiSettings {
            workplace: "Werkstatt-PC".into(),
            ..UiSettings::default()
        };
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
        assert_eq!(loaded.workplace, "Arbeitsplatz");
        assert!(!loaded.workplace_id.is_empty());
        assert_eq!(
            UiSettings::load_from(&dir).workplace_id,
            loaded.workplace_id
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
