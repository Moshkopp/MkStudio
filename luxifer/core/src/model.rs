//! Shape und Layer — das Kern-Datenmodell.
//!
//! Wichtig (ADR/Referenz): Eine **Shape hat keine eigene Farbe**. Sie zeigt über
//! `layer_id` auf einen Layer; der Layer hält Farbe UND Laserparameter. Farbe =
//! Layer = Parametersatz (siehe docs/referenz/01-thorburn-analyse.md, §1.5).

use serde::{Deserialize, Serialize};

use crate::geometry::{BBox, Geo};

/// serde-Default für bool-Felder, die fehlend als `true` gelten sollen.
fn default_true() -> bool {
    true
}

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

/// Deterministische, **katalogfremde** Farbe für einen Bild-Layer (ADR 0004 §4):
/// jedes Bild bekommt eine eigene Kennfarbe, die garantiert nicht in
/// `SWATCH_COLORS` vorkommt und sich zwischen Bildern unterscheidet.
///
/// Verfahren: Farbton per Golden-Angle-Rotation (137.508°) über `seed`
/// gestreut, feste Sättigung/Helligkeit. Kollidiert die Farbe mit dem Katalog
/// (nach Rundung auf u8), wird der nächste Golden-Angle-Schritt genommen. So
/// bleibt die Vergabe reproduzierbar und kollisionsfrei.
pub fn image_layer_color(seed: u32) -> [u8; 3] {
    for step in 0..64u32 {
        let hue = (((seed + step) as f64) * 137.508) % 360.0;
        let rgb = hsl_to_rgb(hue, 0.55, 0.60);
        if !SWATCH_COLORS.contains(&rgb) {
            return rgb;
        }
    }
    // Praktisch unerreichbar (Katalog hat 14 Farben, wir haben 64 Versuche).
    hsl_to_rgb((seed as f64 * 137.508) % 360.0, 0.55, 0.60)
}

/// HSL (h in Grad 0..360, s/l 0..1) → RGB u8. Kleiner Helfer für [`image_layer_color`].
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    ]
}

/// Bearbeitungsmodus eines Layers — bestimmt, WIE gelasert wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LayerMode {
    #[default]
    Cut,
    Fill,
    Raster,
    /// Bild-Layer (ADR 0004): trägt genau ein importiertes Bild + dessen
    /// Verarbeitungsparameter. Wird beim Job gerastert.
    Image,
}

impl LayerMode {
    /// Fill/Raster/Image füllen Flächen; Cut nur Kontur. Für die Fill-Vorschau.
    pub fn is_filled(&self) -> bool {
        matches!(self, LayerMode::Fill | LayerMode::Raster | LayerMode::Image)
    }
}

/// Ein Layer bündelt Farbe und Laserparameter. Formen mit gleicher Farbe teilen
/// sich einen Layer (automatisch verwaltet, siehe `AppState::activate_color`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub color: [u8; 3],
    /// Objekte dieses Layers im Canvas anzeigen (Show/Hide).
    pub visible: bool,
    /// Layer im Job mitbrennen/gravieren. Unabhängig von `visible`: ein Layer
    /// kann sichtbar, aber vom Brennen ausgenommen sein. Alte Dateien ohne das
    /// Feld gelten als aktiviert.
    #[serde(default = "default_true")]
    pub enabled: bool,
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
    /// Bidirektionales Rastern (Scan hin und zurück) für Image-/Raster-Layer.
    /// Alte Dateien ohne das Feld gelten als bidirektional.
    #[serde(default = "default_true")]
    pub bidirectional: bool,
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
            enabled: true,
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
            bidirectional: true,
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

/// Quelldaten eines Text-Blocks (Text→Pfad): erlaubt späteres Editieren
/// (Doppelklick), ohne die Konturen zurückrechnen zu müssen. Liegt am
/// ERSTEN Shape der Text-Gruppe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextMeta {
    pub text: String,
    pub font_path: String,
    /// Content-adressierte Schriftdatei im Asset-Katalog.
    #[serde(default)]
    pub font_asset: Option<String>,
    pub size_mm: f64,
    /// Layout-Parameter (Defaults = Verhalten vor deren Einführung, damit
    /// alte Projekte unverändert laden).
    #[serde(default)]
    pub align: crate::text::TextAlign,
    #[serde(default = "default_line_spacing")]
    pub line_spacing: f64,
    #[serde(default)]
    pub letter_spacing_mm: f64,
}

fn default_line_spacing() -> f64 {
    crate::text::DEFAULT_LINE_SPACING
}

impl Default for TextMeta {
    fn default() -> Self {
        Self {
            text: String::new(),
            font_path: String::new(),
            font_asset: None,
            size_mm: 20.0,
            align: crate::text::TextAlign::default(),
            line_spacing: crate::text::DEFAULT_LINE_SPACING,
            letter_spacing_mm: 0.0,
        }
    }
}

/// Eine gezeichnete Form. Gehört über `layer_id` zu einem Layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub layer_id: usize,
    pub geo: Geo,
    /// Hilfskontur, die ausschließlich eine Flächenfüllung begrenzt. Sie wird
    /// im Cut-Modus weder angezeigt noch in den Laserjob übernommen.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fill_only: bool,
    /// Drehung in Grad (um den Bounding-Box-Mittelpunkt).
    #[serde(default)]
    pub rotation: f64,
    /// Gruppen-ID — Shapes mit gleicher ID werden gemeinsam behandelt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<u32>,
    /// Zusammengesetzter Füllpfad. Even/Odd gilt innerhalb derselben ID;
    /// verschiedene Füllpfade werden anschließend flächig vereinigt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_group_id: Option<u32>,
    /// Pro-Shape-Übersteuerung der Layer-Laserparameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_override: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_override: Option<f64>,
    /// Text-Quelldaten (nur am ersten Shape eines Text-Blocks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_meta: Option<TextMeta>,
    /// Bézier-Knoten (nur bei Polyline-Shapes, die aus einem editierbaren
    /// Bézier-Pfad stammen). Die Polyline hält die geflatteten Punkte; hier
    /// liegen die Knoten für den Node-Editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bezier: Option<crate::bezier::BezierPath>,
}

impl Shape {
    /// Neue Form auf einem Layer.
    pub fn new(layer_id: usize, geo: Geo) -> Self {
        Self {
            layer_id,
            geo,
            fill_only: false,
            rotation: 0.0,
            group_id: None,
            fill_group_id: None,
            speed_override: None,
            power_override: None,
            text_meta: None,
            bezier: None,
        }
    }

    pub fn bbox(&self) -> BBox {
        let local = self.geo.bbox();
        if self.rotation.abs() <= f64::EPSILON {
            return local;
        }
        let (cx, cy) = local.center();
        let (pts, _) = self.geo.outline_points();
        BBox::union_all(pts.into_iter().map(|(x, y)| {
            let (rx, ry) = crate::geometry::rotate_point(x, y, cx, cy, self.rotation);
            BBox::new(rx, ry, 0.0, 0.0)
        }))
        .unwrap_or(local)
    }

    /// Verschiebt alle Darstellungen der Form gemeinsam. `geo` ist die
    /// render-/laserbare Kontur, `bezier` die editierbare Pfad-Wahrheit.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.geo.translate(dx, dy);
        if let Some(bp) = self.bezier.as_mut() {
            for n in &mut bp.nodes {
                n.p = (n.p.0 + dx, n.p.1 + dy);
                if let Some(h) = n.h_in.as_mut() {
                    *h = (h.0 + dx, h.1 + dy);
                }
                if let Some(h) = n.h_out.as_mut() {
                    *h = (h.0 + dx, h.1 + dy);
                }
            }
        }
    }

    /// Skaliert die Form auf eine Zielbox und hält editierbare Bézier-Knoten
    /// samt Tangenten mit der abgeflachten Kontur synchron.
    pub fn set_bbox(&mut self, nx: f64, ny: f64, nw: f64, nh: f64) {
        let old = self.geo.bbox();
        self.geo.set_bbox(nx, ny, nw, nh);
        let target = self.geo.bbox();
        let sx = if old.w.abs() > f64::EPSILON {
            target.w / old.w
        } else {
            1.0
        };
        let sy = if old.h.abs() > f64::EPSILON {
            target.h / old.h
        } else {
            1.0
        };
        if let Some(meta) = self.text_meta.as_mut() {
            if (sx - sy).abs() <= 1e-6 {
                meta.size_mm *= sx.abs();
            } else {
                // Nichtproportional verzerrte Glyphen sind aus Font+Größe nicht
                // mehr reproduzierbar und werden deshalb normale Konturen.
                self.text_meta = None;
            }
        }
        if let Some(bp) = self.bezier.as_mut() {
            let map = |p: (f64, f64)| {
                let rx = if old.w.abs() > f64::EPSILON {
                    (p.0 - old.x) / old.w
                } else {
                    0.0
                };
                let ry = if old.h.abs() > f64::EPSILON {
                    (p.1 - old.y) / old.h
                } else {
                    0.0
                };
                (target.x + rx * target.w, target.y + ry * target.h)
            };
            for n in &mut bp.nodes {
                n.p = map(n.p);
                n.h_in = n.h_in.map(map);
                n.h_out = n.h_out.map(map);
            }
        }
    }

    /// Spiegelt Kontur und editierbare Bézier-Wahrheit an derselben Achse.
    pub fn mirror(&mut self, axis: crate::geometry::Axis, coord: f64) {
        self.geo.mirror(axis, coord);
        // Gespiegelter Text kann aus den ursprünglichen Textparametern nicht
        // wiederhergestellt werden; ab hier ist er bewusst ein Konturpfad.
        self.text_meta = None;
        if let Some(bp) = self.bezier.as_mut() {
            let flip = |p: (f64, f64)| match axis {
                crate::geometry::Axis::Vertical => (2.0 * coord - p.0, p.1),
                crate::geometry::Axis::Horizontal => (p.0, 2.0 * coord - p.1),
            };
            for n in &mut bp.nodes {
                n.p = flip(n.p);
                n.h_in = n.h_in.map(flip);
                n.h_out = n.h_out.map(flip);
            }
        }
    }

    /// Trifft ein Punkt die Form? Berücksichtigt die Rotation (Punkt wird in den
    /// ungedrehten Objektraum zurückgedreht).
    pub fn hit_test(&self, px: f64, py: f64, tol: f64) -> bool {
        if self.rotation == 0.0 {
            return self.geo.hit_test(px, py, tol);
        }
        let (cx, cy) = self.geo.bbox().center();
        let (rx, ry) = crate::geometry::rotate_point(px, py, cx, cy, -self.rotation);
        self.geo.hit_test(rx, ry, tol)
    }

    /// Editor-Auswahl trifft Vektoren nur an ihrer sichtbaren Kontur. So kann
    /// innerhalb einer großen Form eine Marquee-Auswahl begonnen werden.
    /// Bilder bleiben als flächige Objekte über ihre gesamte Box anklickbar.
    pub fn selection_hit_test(&self, px: f64, py: f64, tol: f64) -> bool {
        if matches!(self.geo, Geo::Image { .. }) {
            return self.hit_test(px, py, tol);
        }
        let (px, py) = if self.rotation == 0.0 {
            (px, py)
        } else {
            let (cx, cy) = self.geo.bbox().center();
            crate::geometry::rotate_point(px, py, cx, cy, -self.rotation)
        };
        let (points, closed) = self.geo.outline_points();
        if points
            .windows(2)
            .any(|edge| crate::geometry::point_segment_distance(px, py, edge[0], edge[1]) <= tol)
        {
            return true;
        }
        closed
            && points.len() >= 2
            && crate::geometry::point_segment_distance(px, py, *points.last().unwrap(), points[0])
                <= tol
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
        assert!(LayerMode::Image.is_filled());
    }

    #[test]
    fn image_layer_farbe_katalogfremd_und_verschieden() {
        // Keine der vergebenen Farben liegt im Katalog; benachbarte seeds
        // liefern unterschiedliche Farben.
        let mut seen = std::collections::HashSet::new();
        for seed in 0..20u32 {
            let c = image_layer_color(seed);
            assert!(
                !SWATCH_COLORS.contains(&c),
                "seed {seed} kollidiert mit Katalog"
            );
            seen.insert(c);
        }
        assert!(seen.len() >= 18, "Farben streuen ausreichend");
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

    #[test]
    fn selection_hit_test_trifft_vektor_nur_an_der_kontur() {
        let shape = Shape::new(
            0,
            Geo::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
        );
        assert!(shape.selection_hit_test(0.5, 50.0, 1.0));
        assert!(!shape.selection_hit_test(50.0, 50.0, 1.0));
    }

    #[test]
    fn shape_bbox_beruecksichtigt_rotation() {
        let mut s = Shape::new(
            0,
            Geo::Rect {
                x: 0.0,
                y: 0.0,
                w: 20.0,
                h: 10.0,
            },
        );
        s.rotation = 90.0;
        let b = s.bbox();
        assert!((b.x - 5.0).abs() < 1e-9);
        assert!((b.y + 5.0).abs() < 1e-9);
        assert!((b.w - 10.0).abs() < 1e-9);
        assert!((b.h - 20.0).abs() < 1e-9);
    }

    #[test]
    fn proportionale_textskalierung_aktualisiert_schriftgroesse() {
        let mut s = Shape::new(
            0,
            Geo::Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 20.0,
            },
        );
        s.text_meta = Some(TextMeta {
            text: "A".into(),
            font_path: "font.ttf".into(),
            font_asset: None,
            size_mm: 10.0,
            ..Default::default()
        });
        s.set_bbox(0.0, 0.0, 20.0, 40.0);
        assert_eq!(s.text_meta.unwrap().size_mm, 20.0);
    }

    #[test]
    fn verzerrter_oder_gespiegelter_text_wird_kontur() {
        let meta = || {
            Some(TextMeta {
                text: "A".into(),
                font_path: "font.ttf".into(),
                font_asset: None,
                size_mm: 10.0,
                ..Default::default()
            })
        };
        let mut stretched = Shape::new(
            0,
            Geo::Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 20.0,
            },
        );
        stretched.text_meta = meta();
        stretched.set_bbox(0.0, 0.0, 30.0, 40.0);
        assert!(stretched.text_meta.is_none());

        let mut mirrored = Shape::new(
            0,
            Geo::Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 20.0,
            },
        );
        mirrored.text_meta = meta();
        mirrored.mirror(crate::geometry::Axis::Vertical, 5.0);
        assert!(mirrored.text_meta.is_none());
    }
}
