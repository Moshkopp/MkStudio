//! Bild-Rasterung: Graustufen-Asset → An/Aus-Rasterzeilen (Schwellwert).
//!
//! Reine Geometrie/Bildverarbeitung in mm + Pixel (UI-frei, testbar). Der Core
//! wandelt ein platziertes `Geo::Image` in horizontale **Runs** (durchgebrannte
//! Strecken) pro Scanzeile; der Treiber (Ruida/GRBL) fährt sie ab.
//!
//! **Nur Schwellwert (An/Aus).** Für Ausmalbilder/Strichgrafik ist das der
//! richtige Modus: Der Laser brennt die Linie voll oder gar nicht — kein
//! „Leistung nach Helligkeit modulieren" (das ist für Fotos und kommt mit dem
//! Dithering, ADR 0004 §5). Ein Run trägt deshalb bewusst **keine** Intensität.
//!
//! Angelehnt an ThorBurns `raster.rs` (docs/referenz/, CLAUDE.md Regel 6: nur
//! analysiert, nicht kopiert). Übernommene Erkenntnis: Strichgrafik muss
//! **erst auf Originalauflösung geschwellt** und **danach** auf die Job-Zeilen
//! skaliert werden — würde man erst skalieren, glättet der Filter die harten
//! Kanten zu Grau und die Linien fransen treppig aus. Nach dem Skalieren wird
//! das Zwischengrau erneut hart geschwellt.

use crate::assets::apply_params;
use crate::geometry::{ImageMode, ImageParams};

/// Eine Rasterzeile in mm: bei `y` eine Folge durchgebrannter Strecken (`runs`),
/// jeweils von `x0` bis `x1` (aufsteigend, nicht überlappend, links→rechts).
#[derive(Debug, Clone, PartialEq)]
pub struct RasterRow {
    pub y: f64,
    pub runs: Vec<(f64, f64)>,
}

/// Bild-Layer als **Textur** für die Vorschau (ADR 0008 §2): ein Byte je
/// Rasterzelle (0 = nicht gebrannt, 255 = gebrannt), row-major. Aus denselben
/// `RasterRow`s wie der Job — lügt nicht, ist nur die Pixel-Sicht. Das Frontend
/// lädt `pixels` als GPU-Textur und zeichnet sie an (`x`,`y`,`w`,`h`) mm.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterTexture {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Platzierung auf dem Tisch (mm), linke obere Ecke + Größe.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Ein Graustufenbild als Rasterquelle: row-major `u8`-Pixel + Pixelmaße.
#[derive(Debug, Clone, Copy)]
pub struct RasterImage<'a> {
    pub pixels: &'a [u8],
    pub alpha: Option<&'a [u8]>,
    pub px_w: usize,
    pub px_h: usize,
}

/// Platzierung des Bildes auf dem Tisch (mm): linke obere Ecke + Größe +
/// Zeilenabstand (aus `Layer::line_step_mm`).
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub step_mm: f64,
}

/// Gemeinsamer Kern: schwellt das Bild auf die Zielrasterung und gibt das
/// 1-Bit-`bw`-Feld (0/255, row-major) samt Zielmaßen `(tw, th)` zurück. Leere/
/// entartete Eingaben ⇒ `None`. Basis für `raster_rows` (→ Job-Runs) UND
/// `raster_texture` (→ Vorschau-Pixel) — eine Wahrheit für beide.
///
/// Ablauf für scharfe Kanten (siehe Modul-Doc): Tonwert-LUT + Schwelle auf der
/// **Originalauflösung** (1-Bit, maximal scharf), dann auf die Zielzeilenzahl
/// skalieren, dann erneut schwellen.
fn rasterize(
    img: RasterImage,
    place: Placement,
    params: &ImageParams,
    invert: bool,
) -> Option<(Vec<u8>, usize, usize)> {
    let RasterImage {
        pixels,
        alpha,
        px_w,
        px_h,
    } = img;
    let Placement { w, h, step_mm, .. } = place;
    if px_w == 0 || px_h == 0 || pixels.len() != px_w * px_h || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let step = step_mm.max(0.01);
    // Zielraster: eine Job-Zeile je `step` mm, mindestens eine Zeile/Spalte.
    let th = ((h / step).round() as usize).max(1);
    let tw = ((w / step).round() as usize).max(1);

    let (mut result, tw, th) = if crate::dither::is_dither(params.mode) {
        // FOTO-Pfad (Dithering): erst die Tonwert-LUT als Graustufe, dann auf
        // die Zielauflösung skalieren, dann dithern — das Dithern MUSS auf der
        // Job-Auflösung laufen, sonst zerstört das Skalieren das Punktmuster.
        let gray = apply_params(pixels, params, invert);
        let scaled = scale_gray(&gray, px_w, px_h, tw, th);
        (crate::dither::dither(&scaled, tw, th, params.mode), tw, th)
    } else {
        // STRICHGRAFIK-Pfad (Schwellwert): auf Originalauflösung schwellen
        // (scharf), dann skalieren, dann erneut schwellen (Zwischengrau vom
        // Skalieren wieder hart machen). Auch ein als Grayscale markierter
        // Layer wird hart geschwellt.
        let tp = ImageParams {
            mode: ImageMode::Threshold,
            ..*params
        };
        let bw_src = apply_params(pixels, &tp, invert);
        let scaled = scale_gray(&bw_src, px_w, px_h, tw, th);
        (threshold_128(&scaled), tw, th)
    };
    if let Some(alpha) = alpha.filter(|alpha| alpha.len() == px_w * px_h) {
        let scaled_alpha = scale_gray(alpha, px_w, px_h, tw, th);
        for (pixel, alpha) in result.iter_mut().zip(scaled_alpha) {
            if alpha < 128 {
                *pixel = 255;
            }
        }
    }
    Some((result, tw, th))
}

/// Rastert ein Graustufenbild in die mm-Fläche der `Placement` zu **Job-Runs**:
/// Zeilen von An/Aus-Strecken in mm (der Treiber fährt sie). Leere Zeilen
/// entfallen. `invert` wählt das Laser-Invert (schwarz↔weiß).
pub fn raster_rows(
    img: RasterImage,
    place: Placement,
    params: &ImageParams,
    invert: bool,
) -> Vec<RasterRow> {
    let Placement { x, y, w, h, .. } = place;
    let Some((bw, tw, th)) = rasterize(img, place, params, invert) else {
        return Vec::new();
    };

    // 2. Pro Zeile die schwarzen (= zu brennenden) Runs sammeln und auf mm mappen.
    let mm_per_col = w / tw as f64;
    let mm_per_row = h / th as f64;
    let mut rows = Vec::new();
    for row in 0..th {
        let base = row * tw;
        let mut runs: Vec<(f64, f64)> = Vec::new();
        let mut run_start: Option<usize> = None;
        for col in 0..tw {
            let black = bw[base + col] < 128;
            match (black, run_start) {
                (true, None) => run_start = Some(col),
                (false, Some(s)) => {
                    runs.push((x + s as f64 * mm_per_col, x + col as f64 * mm_per_col));
                    run_start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = run_start {
            runs.push((x + s as f64 * mm_per_col, x + tw as f64 * mm_per_col));
        }
        if !runs.is_empty() {
            rows.push(RasterRow {
                y: y + row as f64 * mm_per_row,
                runs,
            });
        }
    }
    rows
}

/// Wie `raster_rows`, liefert aber die **Textur** für die Vorschau (ADR 0008 §2):
/// ein Byte je Rasterzelle (255 = gebrannt, 0 = nicht), row-major, plus Maße und
/// Tisch-Platzierung. Aus demselben geschwellten Feld wie die Job-Runs — exakt
/// dasselbe Ergebnis, nur als Pixel. `None` bei leeren/entarteten Eingaben.
pub fn raster_texture(
    img: RasterImage,
    place: Placement,
    params: &ImageParams,
    invert: bool,
) -> Option<RasterTexture> {
    let Placement { x, y, w, h, .. } = place;
    let (bw, tw, th) = rasterize(img, place, params, invert)?;
    // bw: 255 = weiß/nicht gebrannt, <128 = gebrannt. Textur: 255 = gebrannt.
    let pixels: Vec<u8> = bw.iter().map(|&v| if v < 128 { 255 } else { 0 }).collect();
    Some(RasterTexture {
        pixels,
        width: tw as u32,
        height: th as u32,
        x,
        y,
        w,
        h,
    })
}

/// Harte Schwelle bei 128 auf einen Graustufen-Puffer (nach dem Skalieren, um
/// vom Filter erzeugtes Zwischengrau wieder rein 1-Bit zu machen).
fn threshold_128(pixels: &[u8]) -> Vec<u8> {
    pixels
        .iter()
        .map(|&v| if v < 128 { 0 } else { 255 })
        .collect()
}

/// Skaliert einen Graustufen-Puffer (`sw`×`sh`) auf (`dw`×`dh`) mit Lanczos3 —
/// derselbe hochwertige Filter, der feine Linien beim Herunterrechnen scharf
/// hält. Gleiche Größe ⇒ unverändert.
fn scale_gray(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    if sw == dw && sh == dh {
        return src.to_vec();
    }
    let Some(img) = image::GrayImage::from_raw(sw as u32, sh as u32, src.to_vec()) else {
        return src.to_vec();
    };
    image::imageops::resize(
        &img,
        dw as u32,
        dh as u32,
        image::imageops::FilterType::Lanczos3,
    )
    .into_raw()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn threshold_params() -> ImageParams {
        ImageParams {
            mode: ImageMode::Threshold,
            threshold: 128,
            ..Default::default()
        }
    }

    /// Knapper Test-Aufruf: Bild aus `pixels`/Maßen, Box (x,y,w,h) + step.
    #[allow(clippy::too_many_arguments)]
    fn run(
        pixels: &[u8],
        px_w: usize,
        px_h: usize,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        step: f64,
        invert: bool,
    ) -> Vec<RasterRow> {
        raster_rows(
            RasterImage {
                pixels,
                alpha: None,
                px_w,
                px_h,
            },
            Placement {
                x,
                y,
                w,
                h,
                step_mm: step,
            },
            &threshold_params(),
            invert,
        )
    }

    #[test]
    fn dither_modus_erhaelt_mittelgrau_als_flaechendichte() {
        // 50%-Grau, 16×16 px auf 16×16 mm (step 1). Threshold kippt alles auf
        // eine Seite (ganz oder gar nicht); ein Dither-Modus muss ~die halbe
        // Fläche brennen (Grau als Punktdichte, ADR 0004 §5).
        let px = vec![128u8; 16 * 16];
        let img = RasterImage {
            pixels: &px,
            alpha: None,
            px_w: 16,
            px_h: 16,
        };
        let place = Placement {
            x: 0.0,
            y: 0.0,
            w: 16.0,
            h: 16.0,
            step_mm: 1.0,
        };
        let dp = ImageParams {
            mode: ImageMode::Floyd,
            ..Default::default()
        };
        let burned: f64 = raster_rows(img, place, &dp, false)
            .iter()
            .flat_map(|r| r.runs.iter())
            .map(|&(a, b)| b - a)
            .sum();
        let total = 16.0 * 16.0;
        assert!(
            burned > total * 0.3 && burned < total * 0.7,
            "Dither soll ~50% brennen, war {:.0}%",
            burned / total * 100.0
        );

        // Threshold zum Vergleich: 128 ≥ 128 ⇒ weiß ⇒ nichts zu brennen.
        let tp = threshold_params();
        let t_rows = raster_rows(img, place, &tp, false);
        assert!(t_rows.is_empty(), "Threshold kippt Mittelgrau komplett");
    }

    #[test]
    fn volle_schwarze_flaeche_gibt_zeilen_ueber_ganze_breite() {
        // 4×4 komplett schwarz, 4×4 mm, step 1 mm ⇒ 4 Zeilen à ein Run 0..4.
        let rows = run(&[0u8; 16], 4, 4, 0.0, 0.0, 4.0, 4.0, 1.0, false);
        assert_eq!(rows.len(), 4);
        for r in &rows {
            assert_eq!(r.runs.len(), 1);
            assert!((r.runs[0].0 - 0.0).abs() < 1e-6);
            assert!((r.runs[0].1 - 4.0).abs() < 1e-6);
        }
    }

    #[test]
    fn weisse_flaeche_gibt_keine_zeilen() {
        let rows = run(&[255u8; 16], 4, 4, 0.0, 0.0, 4.0, 4.0, 1.0, false);
        assert!(rows.is_empty(), "weiß = nichts zu brennen");
    }

    #[test]
    fn luecke_in_der_zeile_gibt_zwei_runs() {
        // Eine Zeile: schwarz weiß weiß schwarz → zwei getrennte Runs.
        let rows = run(&[0u8, 255, 255, 0], 4, 1, 0.0, 0.0, 4.0, 1.0, 1.0, false);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].runs.len(), 2, "Lücke trennt die Runs");
        assert!((rows[0].runs[0].0 - 0.0).abs() < 1e-6);
        assert!((rows[0].runs[1].1 - 4.0).abs() < 1e-6);
    }

    #[test]
    fn invert_dreht_schwarz_und_weiss() {
        // Rein weiß + invert ⇒ alles wird schwarz ⇒ volle Zeilen.
        let rows = run(&[255u8; 4], 4, 1, 0.0, 0.0, 4.0, 1.0, 1.0, true);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].runs.len(), 1);
    }

    #[test]
    fn transparente_pixel_brennen_auch_invertiert_nicht() {
        let pixels = [255, 255];
        let alpha = [0, 255];
        let rows = raster_rows(
            RasterImage {
                pixels: &pixels,
                alpha: Some(&alpha),
                px_w: 2,
                px_h: 1,
            },
            Placement {
                x: 0.0,
                y: 0.0,
                w: 2.0,
                h: 1.0,
                step_mm: 1.0,
            },
            &threshold_params(),
            true,
        );
        assert_eq!(rows[0].runs, vec![(1.0, 2.0)]);
    }

    #[test]
    fn offset_wird_in_mm_addiert() {
        let rows = run(&[0u8; 4], 4, 1, 10.0, 20.0, 4.0, 1.0, 1.0, false);
        assert!((rows[0].y - 20.0).abs() < 1e-6, "y = Box-Offset");
        assert!((rows[0].runs[0].0 - 10.0).abs() < 1e-6, "x0 = Box-Offset");
        assert!(
            (rows[0].runs[0].1 - 14.0).abs() < 1e-6,
            "x1 = Offset + Breite"
        );
    }

    #[test]
    fn kante_bleibt_scharf_beim_skalieren() {
        // Scharfe vertikale Kante 8×2: links schwarz, rechts weiß. Auf ungerade
        // Zielbreite skalieren erzwingt Interpolation → darf KEIN Zwischengrau
        // als Run erzeugen (nur ein sauberer schwarzer Block links).
        let (w0, h0) = (8usize, 2usize);
        let mut src = vec![255u8; w0 * h0];
        for r in 0..h0 {
            for c in 0..w0 {
                src[r * w0 + c] = if c < 4 { 0 } else { 255 };
            }
        }
        // 5 mm breit bei step 1 ⇒ tw=5, erzwingt Skalierung 8→5.
        let rows = run(&src, w0, h0, 0.0, 0.0, 5.0, 2.0, 1.0, false);
        assert!(!rows.is_empty());
        for r in &rows {
            // Genau ein zusammenhängender Run (die linke Hälfte), keine Grautreppe.
            assert_eq!(r.runs.len(), 1, "scharfe Kante = ein Run, war {:?}", r.runs);
            assert!((r.runs[0].0 - 0.0).abs() < 1e-6);
        }
    }

    #[test]
    fn leere_eingaben_sind_robust() {
        assert!(run(&[], 0, 0, 0.0, 0.0, 4.0, 4.0, 1.0, false).is_empty());
        // Pixelanzahl passt nicht zur Größe.
        assert!(run(&[0, 0], 4, 4, 0.0, 0.0, 4.0, 4.0, 1.0, false).is_empty());
        // Nullgröße.
        assert!(run(&[0], 1, 1, 0.0, 0.0, 0.0, 0.0, 1.0, false).is_empty());
    }
}
