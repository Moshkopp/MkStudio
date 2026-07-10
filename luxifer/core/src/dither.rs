//! Dithering für Foto-Gravur: Graustufen → 1-Bit (ADR 0004 §5).
//!
//! Reine Bildverarbeitung (UI-frei, testbar). Ausmalbilder/Strichgrafik nutzen
//! den Schwellwert (raster.rs); **Fotos** brauchen Dithering, damit Grautöne als
//! Punktdichte erhalten bleiben. Konvention wie im Raster-Pfad: **0 = brennen**
//! (dunkel), **255 = auslassen** (hell).
//!
//! Nach v3-Analyse (docs/referenz/, CLAUDE.md Regel 6: analysiert, nicht
//! kopiert) neu gebaut. Übernommene Erkenntnisse:
//! - **Serpentinen-Scan** (Zeilen abwechselnd vor/zurück) verhindert die
//!   diagonalen Wurm-Artefakte einfacher Fehlerdiffusion.
//! - **Hysterese-Modus**: Floyd-Steinberg mit richtungsabhängiger Schwelle —
//!   war der Laser „an", bleibt er bevorzugt an; war er „aus", zündet er nicht
//!   für Einzelpixel. Ergebnis: zusammenhängende Brennstrecken statt
//!   Einzelpunkte, die eine Röhre bei Scan-Geschwindigkeit nicht sauber zündet.
//! - Dithering muss auf **Ziel-Auflösung** laufen (erst skalieren, dann
//!   dithern) — Skalieren nach dem Dithern zerstört das Punktmuster.

use crate::geometry::ImageMode;

/// Ein Fehlerdiffusions-Kernel: Verteilung des Quantisierungsfehlers auf
/// Nachbarpixel als (dx, dy, Gewicht); `div` normiert die Gewichte.
struct Kernel {
    taps: &'static [(i32, i32, f32)],
    div: f32,
}

/// Floyd–Steinberg — der Klassiker, gute Balance aus Schärfe und Körnung.
static FLOYD: Kernel = Kernel {
    taps: &[(1, 0, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)],
    div: 16.0,
};

/// Jarvis–Judice–Ninke — breiter Kernel, weichere Verläufe, weniger Struktur.
static JARVIS: Kernel = Kernel {
    taps: &[
        (1, 0, 7.0),
        (2, 0, 5.0),
        (-2, 1, 3.0),
        (-1, 1, 5.0),
        (0, 1, 7.0),
        (1, 1, 5.0),
        (2, 1, 3.0),
        (-2, 2, 1.0),
        (-1, 2, 3.0),
        (0, 2, 5.0),
        (1, 2, 3.0),
        (2, 2, 1.0),
    ],
    div: 48.0,
};

/// Stucki — wie Jarvis, etwas schärfer in den Kanten.
static STUCKI: Kernel = Kernel {
    taps: &[
        (1, 0, 8.0),
        (2, 0, 4.0),
        (-2, 1, 2.0),
        (-1, 1, 4.0),
        (0, 1, 8.0),
        (1, 1, 4.0),
        (2, 1, 2.0),
        (-2, 2, 1.0),
        (-1, 2, 2.0),
        (0, 2, 4.0),
        (1, 2, 2.0),
        (2, 2, 1.0),
    ],
    div: 42.0,
};

/// Atkinson — verteilt nur 6/8 des Fehlers: hellere, luftigere Ergebnisse
/// (klassischer Mac-Look), auf Holz oft gefälliger als Floyd.
static ATKINSON: Kernel = Kernel {
    taps: &[
        (1, 0, 1.0),
        (2, 0, 1.0),
        (-1, 1, 1.0),
        (0, 1, 1.0),
        (1, 1, 1.0),
        (0, 2, 1.0),
    ],
    div: 8.0,
};

/// 4×4-Bayer-Matrix für geordnetes Dithering (regelmäßiges Muster, kein
/// Fehlertransport — robust, aber sichtbares Raster).
static BAYER4: [[f32; 4]; 4] = [
    [0.0, 8.0, 2.0, 10.0],
    [12.0, 4.0, 14.0, 6.0],
    [3.0, 11.0, 1.0, 9.0],
    [15.0, 7.0, 13.0, 5.0],
];

/// Hysterese-Breite des Laser-Modus (±um die Mittel-Schwelle 128). Je größer,
/// desto längere zusammenhängende Brennstrecken (und desto gröber das Bild).
const LASER_HYSTERESE: f32 = 48.0;

/// Ob ein Modus ein Dither-Verfahren ist (Grays → Punktmuster). `Grayscale`
/// und `Threshold` sind es nicht (kein Fehlertransport/Muster).
pub fn is_dither(mode: ImageMode) -> bool {
    !matches!(mode, ImageMode::Grayscale | ImageMode::Threshold)
}

/// Dithert Graustufen (row-major u8) zu 1-Bit (0 = brennen, 255 = auslassen).
/// `Grayscale`/`Threshold` werden hier NICHT behandelt (Aufgabe des
/// Schwellwert-Pfads in raster.rs); sie fallen sicherheitshalber auf eine
/// harte 128er-Schwelle zurück.
pub fn dither(pixels: &[u8], w: usize, h: usize, mode: ImageMode) -> Vec<u8> {
    if pixels.len() != w * h || w == 0 || h == 0 {
        return pixels.to_vec();
    }
    match mode {
        ImageMode::Floyd => diffuse(pixels, w, h, &FLOYD),
        ImageMode::Jarvis => diffuse(pixels, w, h, &JARVIS),
        ImageMode::Stucki => diffuse(pixels, w, h, &STUCKI),
        ImageMode::Atkinson => diffuse(pixels, w, h, &ATKINSON),
        ImageMode::Bayer => bayer(pixels, w, h),
        ImageMode::LaserRuns => laser_runs(pixels, w, h),
        // Kein Dither-Modus: harte Schwelle als sicherer Fallback.
        ImageMode::Grayscale | ImageMode::Threshold => pixels
            .iter()
            .map(|&v| if v < 128 { 0 } else { 255 })
            .collect(),
    }
}

/// Fehlerdiffusion mit Serpentinen-Scan: gerade Zeilen links→rechts, ungerade
/// rechts→links (Kernel-X wird gespiegelt). Verhindert Richtungs-Artefakte.
fn diffuse(pixels: &[u8], w: usize, h: usize, k: &Kernel) -> Vec<u8> {
    let mut buf: Vec<f32> = pixels.iter().map(|&v| v as f32).collect();
    for y in 0..h {
        let ltr = y % 2 == 0;
        let xs: Vec<usize> = if ltr {
            (0..w).collect()
        } else {
            (0..w).rev().collect()
        };
        for x in xs {
            let i = y * w + x;
            let old = buf[i];
            let new = if old >= 128.0 { 255.0 } else { 0.0 };
            buf[i] = new;
            let err = old - new;
            for &(dx, dy, wgt) in k.taps {
                let dx = if ltr { dx } else { -dx };
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && (nx as usize) < w && (ny as usize) < h {
                    buf[ny as usize * w + nx as usize] += err * wgt / k.div;
                }
            }
        }
    }
    buf.iter().map(|&v| v.clamp(0.0, 255.0) as u8).collect()
}

/// Geordnetes 4×4-Bayer-Dithering: Schwelle je Position aus der Matrix.
fn bayer(pixels: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            let t = (BAYER4[y % 4][x % 4] + 0.5) / 16.0 * 255.0;
            out[y * w + x] = if pixels[y * w + x] as f32 >= t {
                255
            } else {
                0
            };
        }
    }
    out
}

/// Laser-Modus: Floyd-Steinberg mit Schwellen-Hysterese entlang der Scanzeile.
/// War das vorige Pixel „brennen", sinkt die Hürde weiterzubrennen; war es
/// „aus", steigt die Hürde zu zünden. Der Quantisierungsfehler wird normal
/// weiterverteilt, die Grauwert-Summe bleibt also erhalten — nur die
/// **Verteilung** wird lauffreundlich (lange Runs statt Einzelpunkte).
fn laser_runs(pixels: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut buf: Vec<f32> = pixels.iter().map(|&v| v as f32).collect();
    for y in 0..h {
        let ltr = y % 2 == 0;
        let xs: Vec<usize> = if ltr {
            (0..w).collect()
        } else {
            (0..w).rev().collect()
        };
        let mut burning = false;
        for x in xs {
            let i = y * w + x;
            let old = buf[i];
            // brennen = dunkel (unter der Schwelle); Hysterese verschiebt sie.
            let t = if burning {
                128.0 + LASER_HYSTERESE
            } else {
                128.0 - LASER_HYSTERESE
            };
            let new = if old >= t { 255.0 } else { 0.0 };
            buf[i] = new;
            burning = new == 0.0;
            let err = old - new;
            for &(dx, dy, wgt) in FLOYD.taps {
                let dx = if ltr { dx } else { -dx };
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && (nx as usize) < w && (ny as usize) < h {
                    buf[ny as usize * w + nx as usize] += err * wgt / FLOYD.div;
                }
            }
        }
    }
    buf.iter().map(|&v| v.clamp(0.0, 255.0) as u8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Horizontaler Grauverlauf 0..255.
    fn gradient(w: usize, h: usize) -> Vec<u8> {
        (0..w * h).map(|i| ((i % w) * 255 / w) as u8).collect()
    }

    fn mean(v: &[u8]) -> f64 {
        v.iter().map(|&x| x as f64).sum::<f64>() / v.len() as f64
    }

    #[test]
    fn ausgabe_ist_rein_1bit() {
        let px = gradient(32, 16);
        for mode in [
            ImageMode::Floyd,
            ImageMode::Jarvis,
            ImageMode::Stucki,
            ImageMode::Atkinson,
            ImageMode::Bayer,
            ImageMode::LaserRuns,
        ] {
            let out = dither(&px, 32, 16, mode);
            assert!(
                out.iter().all(|&v| v == 0 || v == 255),
                "{mode:?} liefert Graupixel"
            );
        }
    }

    #[test]
    fn fehlerdiffusion_erhaelt_den_mittelwert() {
        // Fehlertransport ⇒ mittlerer Grauwert bleibt ungefähr erhalten
        // (das ist der Sinn von Dithering: Grau als Punktdichte).
        let px = gradient(64, 16);
        let m_in = mean(&px);
        for mode in [ImageMode::Floyd, ImageMode::Atkinson, ImageMode::LaserRuns] {
            let out = dither(&px, 64, 16, mode);
            let m_out = mean(&out);
            assert!(
                (m_in - m_out).abs() < 24.0,
                "{mode:?}: Mittelwert driftet ({m_in:.0} → {m_out:.0})"
            );
        }
    }

    #[test]
    fn laser_runs_erzeugt_laengere_strecken_als_floyd() {
        // 50%-Grau: Floyd macht Schachbrett-artige Einzelpixel; der
        // Hysterese-Modus soll deutlich längere zusammenhängende Runs bauen.
        let (w, h) = (48usize, 4usize);
        let px = vec![128u8; w * h];
        let max_run = |data: &[u8]| {
            let (mut max, mut cur) = (0usize, 0usize);
            for &v in data {
                if v == 0 {
                    cur += 1;
                    max = max.max(cur);
                } else {
                    cur = 0;
                }
            }
            max
        };
        let laser = max_run(&dither(&px, w, h, ImageMode::LaserRuns));
        let floyd = max_run(&dither(&px, w, h, ImageMode::Floyd));
        assert!(
            laser > floyd,
            "Laser-Runs ({laser}) nicht länger als Floyd ({floyd})"
        );
    }

    #[test]
    fn schwarz_und_weiss_bleiben_erhalten() {
        // Reines Schwarz/Weiß darf kein Dither-Rauschen bekommen.
        let black = vec![0u8; 64];
        let white = vec![255u8; 64];
        for mode in [ImageMode::Floyd, ImageMode::Bayer, ImageMode::LaserRuns] {
            assert!(dither(&black, 8, 8, mode).iter().all(|&v| v == 0));
            assert!(dither(&white, 8, 8, mode).iter().all(|&v| v == 255));
        }
    }

    #[test]
    fn leere_oder_falsche_masse_sind_robust() {
        assert!(dither(&[], 0, 0, ImageMode::Floyd).is_empty());
        // Länge passt nicht zu w*h → unverändert zurück.
        let px = vec![10u8, 20];
        assert_eq!(dither(&px, 4, 4, ImageMode::Floyd), px);
    }
}
