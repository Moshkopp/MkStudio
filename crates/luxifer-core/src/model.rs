//! Shape und Layer — das Kern-Datenmodell.
//!
//! Wichtig (ADR/Referenz): Eine **Shape hat keine eigene Farbe**. Sie zeigt über
//! `layer_id` auf einen Layer; der Layer hält Farbe UND Laserparameter. Farbe =
//! Layer = Parametersatz (siehe docs/referenz/01-thorburn-analyse.md, §1.5).

use serde::{Deserialize, Serialize};

use crate::geometry::{BBox, Geo};

/// Die 14 Standard-Layerfarben (RGB). Neue Layer erhalten reihum eine Farbe.
pub const SWATCH_COLORS: &[[u8; 3]] = &[
    [0xEF, 0x44, 0x44], // rot
    [0x3B, 0x82, 0xF6], // blau
    [0x10, 0xB9, 0x81], // grün
    [0xEA, 0xB3, 0x08], // gelb
    [0xD9, 0x46, 0xEF], // pink
    [0xA8, 0x55, 0xF7], // violett
    [0x84, 0xCC, 0x16], // limette
    [0x06, 0xB6, 0xD4], // cyan
    [0xF9, 0x73, 0x16], // orange
    [0x8B, 0x5C, 0xF6], // indigo
    [0xEC, 0x48, 0x99], // magenta
    [0x00, 0xFF, 0xFF], // aqua
    [0xF5, 0x9E, 0x0B], // amber
    [0x6B, 0x72, 0x80], // grau
];

/// Bearbeitungsmodus eines Layers — bestimmt, WIE gelasert wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LayerMode {
    #[default]
    Cut,
    Fill,
    Raster,
}

impl LayerMode {
    /// Fill/Raster füllen Flächen; Cut nur Kontur. Für die Fill-Vorschau.
    pub fn is_filled(&self) -> bool {
        matches!(self, LayerMode::Fill | LayerMode::Raster)
    }
}

/// Ein Layer bündelt Farbe und Laserparameter. Formen mit gleicher Farbe teilen
/// sich einen Layer (automatisch verwaltet, siehe `AppState::activate_color`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub color: [u8; 3],
    pub visible: bool,
    pub active: bool,
    pub locked: bool,
    pub mode: LayerMode,
    pub speed_mm_s: f64,
    pub power_pct: f64,
    pub min_power_pct: f64,
    pub air_assist: bool,
    /// Zeilenabstand für Fill-Layer in mm.
    pub line_step_mm: f64,
    /// Wiederholungen.
    pub passes: u32,
    /// Auflösung für Raster-Layer.
    pub dpi: f64,
}

impl Layer {
    /// Neuer Layer mit reihum vergebener Palettenfarbe und Standard-Parametern.
    pub fn new(index: usize) -> Self {
        Self::with_color(index, SWATCH_COLORS[index % SWATCH_COLORS.len()])
    }

    /// Neuer Layer mit fester Farbe (für die Farbpalette).
    pub fn with_color(index: usize, color: [u8; 3]) -> Self {
        Self {
            name: format!("Ebene {}", index + 1),
            color,
            visible: true,
            active: true,
            locked: false,
            mode: LayerMode::Cut,
            speed_mm_s: 100.0,
            power_pct: 20.0,
            min_power_pct: 10.0,
            air_assist: false,
            line_step_mm: 0.1,
            passes: 1,
            dpi: 254.0,
        }
    }

    /// Farbe als Hex-String "#RRGGBB" (fürs Frontend praktisch).
    pub fn color_hex(&self) -> String {
        format!(
            "#{:02X}{:02X}{:02X}",
            self.color[0], self.color[1], self.color[2]
        )
    }
}

/// Eine gezeichnete Form. Gehört über `layer_id` zu einem Layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub layer_id: usize,
    pub geo: Geo,
    /// Drehung in Grad (um den Bounding-Box-Mittelpunkt).
    #[serde(default)]
    pub rotation: f64,
    /// Gruppen-ID — Shapes mit gleicher ID werden gemeinsam behandelt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<u32>,
    /// Pro-Shape-Übersteuerung der Layer-Laserparameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_override: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_override: Option<f64>,
}

impl Shape {
    /// Neue Form auf einem Layer.
    pub fn new(layer_id: usize, geo: Geo) -> Self {
        Self {
            layer_id,
            geo,
            rotation: 0.0,
            group_id: None,
            speed_override: None,
            power_override: None,
        }
    }

    pub fn bbox(&self) -> BBox {
        self.geo.bbox()
    }

    /// Trifft ein Punkt die Form? Berücksichtigt die Rotation (Punkt wird in den
    /// ungedrehten Objektraum zurückgedreht).
    pub fn hit_test(&self, px: f64, py: f64, tol: f64) -> bool {
        if self.rotation == 0.0 {
            return self.geo.hit_test(px, py, tol);
        }
        let (cx, cy) = self.bbox().center();
        let (rx, ry) = crate::geometry::rotate_point(px, py, cx, cy, -self.rotation);
        self.geo.hit_test(rx, ry, tol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_vergibt_farben_reihum() {
        assert_eq!(Layer::new(0).color, SWATCH_COLORS[0]);
        assert_eq!(Layer::new(1).color, SWATCH_COLORS[1]);
        assert_eq!(Layer::new(SWATCH_COLORS.len()).color, SWATCH_COLORS[0]);
    }

    #[test]
    fn color_hex_formatiert() {
        assert_eq!(
            Layer::with_color(0, [0xEF, 0x44, 0x44]).color_hex(),
            "#EF4444"
        );
    }

    #[test]
    fn nur_fill_und_raster_gefuellt() {
        assert!(!LayerMode::Cut.is_filled());
        assert!(LayerMode::Fill.is_filled());
        assert!(LayerMode::Raster.is_filled());
    }

    #[test]
    fn shape_hit_test_beruecksichtigt_rotation() {
        // Längliches Rechteck 100×20, Mitte (50,10).
        let mut s = Shape::new(
            0,
            Geo::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 20.0,
            },
        );
        assert!(!s.hit_test(50.0, 45.0, 0.0)); // ungedreht außerhalb
        s.rotation = 90.0;
        assert!(s.hit_test(50.0, 45.0, 0.0)); // gedreht: ragt vertikal
    }
}
