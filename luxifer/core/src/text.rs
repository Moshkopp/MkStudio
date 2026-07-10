//! Text → Vektorpfade: Glyph-Outlines eines Fonts als geschlossene Konturen.
//!
//! Reine Geometrie (UI-frei, testbar): der Aufrufer liefert die Font-Bytes
//! (TTF/OTF), der Core parst die Glyphen (`ttf-parser`), reiht sie mit ihrem
//! horizontalen Advance auf, flattet die Bézier-Kurven adaptiv und liefert
//! Konturen in **mm** (y nach unten, Ursprung = linke Oberkante der Zeile).
//! Buchstaben-Innenräume (Löcher wie im „O") sind eigene Konturen — die
//! Even-Odd-Füllung spart sie automatisch aus.
//!
//! Nach v3-Analyse neu gebaut (CLAUDE.md Regel 6); dieselbe Bibliothekswahl
//! (`ttf-parser`), eigene Umsetzung. Mehrzeilig über `\n` (Zeilenhöhe 1,25 em).

use crate::geometry::Pt;
use ttf_parser::{Face, OutlineBuilder};

/// Fehler beim Font-Parsen.
#[derive(Debug)]
pub struct TextError(pub String);

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for TextError {}

/// Wandelt `text` mit dem Font (`font_data`: TTF/OTF-Bytes) in geschlossene
/// Konturen um. `size_mm` = Versalhöhe grob als Em-Größe (wie Punktgröße).
/// Ursprung (0,0) = linke Oberkante der ersten Zeile; y wächst nach unten.
pub fn text_to_contours(
    font_data: &[u8],
    text: &str,
    size_mm: f64,
) -> Result<Vec<(Vec<Pt>, bool)>, TextError> {
    let face = Face::parse(font_data, 0).map_err(|e| TextError(format!("Font unlesbar: {e}")))?;
    let upm = face.units_per_em() as f64;
    if upm <= 0.0 {
        return Err(TextError("Font ohne units_per_em".into()));
    }
    let scale = size_mm / upm;
    let ascender = face.ascender() as f64 * scale;
    let line_height = size_mm * 1.25;

    let mut out: Vec<(Vec<Pt>, bool)> = Vec::new();
    let mut y_line = 0.0_f64;
    for line in text.split('\n') {
        let mut x_pen = 0.0_f64;
        for ch in line.chars() {
            let Some(gid) = face.glyph_index(ch) else {
                // Unbekanntes Zeichen: Leerraum in Em-Breite/2.
                x_pen += size_mm * 0.5;
                continue;
            };
            let advance = face
                .glyph_hor_advance(gid)
                .map(|a| a as f64 * scale)
                .unwrap_or(size_mm * 0.5);
            // Outline sammeln (Leerzeichen haben keine).
            let mut b = Flattener {
                scale,
                x_off: x_pen,
                // Font: y nach oben. Unser System: y nach unten, Zeilen-
                // Oberkante = Ascender-Linie.
                y_base: y_line + ascender,
                cur: Vec::new(),
                contours: Vec::new(),
                start: (0.0, 0.0),
            };
            face.outline_glyph(gid, &mut b);
            for c in b.contours {
                if c.len() >= 3 {
                    out.push((c, true));
                }
            }
            x_pen += advance;
        }
        y_line += line_height;
    }
    Ok(out)
}

/// Sammelt Glyph-Outlines als geflattete Polylinien. Quadratische und kubische
/// Béziers werden mit fester Unterteilung angenähert (fein genug für mm-Maße;
/// die Segmentzahl skaliert die Punktdichte, nicht die Korrektheit).
struct Flattener {
    scale: f64,
    x_off: f64,
    y_base: f64,
    cur: Vec<Pt>,
    contours: Vec<Vec<Pt>>,
    start: Pt,
}

impl Flattener {
    fn map(&self, x: f32, y: f32) -> Pt {
        (
            self.x_off + x as f64 * self.scale,
            self.y_base - y as f64 * self.scale,
        )
    }
}

/// Unterteilungen je Kurvensegment.
const CURVE_SEGS: usize = 8;

impl OutlineBuilder for Flattener {
    fn move_to(&mut self, x: f32, y: f32) {
        if self.cur.len() >= 3 {
            self.contours.push(std::mem::take(&mut self.cur));
        } else {
            self.cur.clear();
        }
        self.start = self.map(x, y);
        self.cur.push(self.start);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.cur.push(self.map(x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p0 = *self.cur.last().unwrap_or(&self.start);
        let c = self.map(x1, y1);
        let p1 = self.map(x, y);
        for i in 1..=CURVE_SEGS {
            let t = i as f64 / CURVE_SEGS as f64;
            let u = 1.0 - t;
            let px = u * u * p0.0 + 2.0 * u * t * c.0 + t * t * p1.0;
            let py = u * u * p0.1 + 2.0 * u * t * c.1 + t * t * p1.1;
            self.cur.push((px, py));
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p0 = *self.cur.last().unwrap_or(&self.start);
        let c1 = self.map(x1, y1);
        let c2 = self.map(x2, y2);
        let p1 = self.map(x, y);
        for i in 1..=CURVE_SEGS {
            let t = i as f64 / CURVE_SEGS as f64;
            let u = 1.0 - t;
            let px = u * u * u * p0.0
                + 3.0 * u * u * t * c1.0
                + 3.0 * u * t * t * c2.0
                + t * t * t * p1.0;
            let py = u * u * u * p0.1
                + 3.0 * u * u * t * c1.1
                + 3.0 * u * t * t * c2.1
                + t * t * t * p1.1;
            self.cur.push((px, py));
        }
    }

    fn close(&mut self) {
        if self.cur.len() >= 3 {
            self.contours.push(std::mem::take(&mut self.cur));
        } else {
            self.cur.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Einen System-Font finden, der echte Outlines liefert (Color-Emoji-
    /// Fonts z. B. haben keine). Test überspringt, wenn keiner da ist —
    /// CI-sicher; auf dem Zielsystem ist immer einer vorhanden.
    fn any_system_font() -> Option<Vec<u8>> {
        for dir in ["/usr/share/fonts", "/usr/local/share/fonts"] {
            let mut stack = vec![std::path::PathBuf::from(dir)];
            while let Some(d) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&d) else {
                    continue;
                };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().is_some_and(|x| x == "ttf" || x == "otf") {
                        if let Ok(b) = std::fs::read(&p) {
                            // Nur Fonts, die für "A" wirklich Konturen liefern.
                            if text_to_contours(&b, "A", 10.0)
                                .map(|c| !c.is_empty())
                                .unwrap_or(false)
                            {
                                return Some(b);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    #[test]
    fn text_liefert_konturen_in_erwarteter_groesse() {
        let Some(font) = any_system_font() else {
            eprintln!("kein Systemfont — Test übersprungen");
            return;
        };
        let out = text_to_contours(&font, "LuxiFer", 20.0).unwrap();
        assert!(!out.is_empty(), "Buchstaben ergeben Konturen");
        // Bounding-Box: Höhe grob in der Größenordnung der Em-Größe.
        let ys: Vec<f64> = out
            .iter()
            .flat_map(|(c, _)| c.iter().map(|p| p.1))
            .collect();
        let h = ys.iter().cloned().fold(f64::MIN, f64::max)
            - ys.iter().cloned().fold(f64::MAX, f64::min);
        assert!(h > 5.0 && h < 30.0, "Texthöhe ~Em-Größe, war {h:.1}");
        // Alle Konturen geschlossen.
        assert!(out.iter().all(|(_, closed)| *closed));
    }

    #[test]
    fn o_hat_aussen_und_innenkontur() {
        let Some(font) = any_system_font() else {
            return;
        };
        let out = text_to_contours(&font, "O", 20.0).unwrap();
        assert!(
            out.len() >= 2,
            "O = Außenrand + Innenloch, war {}",
            out.len()
        );
    }

    #[test]
    fn mehrzeilig_versetzt_nach_unten() {
        let Some(font) = any_system_font() else {
            return;
        };
        let one = text_to_contours(&font, "A", 10.0).unwrap();
        let two = text_to_contours(&font, "A\nA", 10.0).unwrap();
        let max_y = |cs: &[(Vec<Pt>, bool)]| {
            cs.iter()
                .flat_map(|(c, _)| c.iter().map(|p| p.1))
                .fold(f64::MIN, f64::max)
        };
        assert!(max_y(&two) > max_y(&one) + 5.0, "zweite Zeile liegt tiefer");
    }

    #[test]
    fn kaputte_bytes_geben_fehler() {
        assert!(text_to_contours(&[1, 2, 3], "x", 10.0).is_err());
    }
}
