//! Bild-Vektorisierung (Trace): Bitmap → geschlossene Vektor-Konturen.
//!
//! Reine Bildverarbeitung/Geometrie (UI-frei, testbar). Ein Graustufenbild
//! wird per Schwellwert binarisiert; um jede Vordergrund-Region werden die
//! Randkonturen verfolgt (Außenränder UND Löcher), dann vereinfacht
//! (Douglas-Peucker) und geglättet (Chaikin). Ergebnis in **Pixelkoordinaten**
//! — nach mm skaliert der Aufrufer (er kennt die Bildbox).
//!
//! Nach v3-Analyse neu gebaut (CLAUDE.md Regel 6). Abweichung: statt
//! Moore-Neighbor-Tracing verketten wir die **Randkanten** des Pixelgitters zu
//! Schleifen — das liefert Löcher gratis mit und hat keine Sonderfälle an
//! 1-Pixel-Stegen. Wie v3 bewusst kein Farbcluster-Tracer: Schwellwert/
//! Glättung/Vereinfachung bleiben als Regler steuerbar.

use crate::geometry::Pt;
use std::collections::HashMap;

/// Parameter für den Trace (Pixel-Einheiten, wo nicht anders genannt).
#[derive(Debug, Clone, Copy)]
pub struct TraceParams {
    /// Schwellwert 0..=255: Pixel **unter** der Schwelle sind Vordergrund
    /// (dunkel = Tinte).
    pub threshold: u8,
    /// Vordergrund/Hintergrund tauschen (helle Motive auf dunklem Grund).
    pub invert: bool,
    /// Chaikin-Glättungs-Durchläufe (0 = aus; 1–2 üblich).
    pub smooth: u32,
    /// Douglas-Peucker-Toleranz in Pixeln (0 = aus; ~1–2 üblich).
    pub simplify_px: f64,
    /// Konturen mit kleinerer Fläche (px²) verwerfen (Rauschen/Staub).
    pub min_area_px: f64,
}

impl Default for TraceParams {
    fn default() -> Self {
        Self {
            threshold: 128,
            invert: false,
            smooth: 1,
            simplify_px: 1.2,
            min_area_px: 16.0,
        }
    }
}

/// Tracet alle Randkonturen des Vordergrunds. Ergebnis: geschlossene Polygone
/// in Pixelkoordinaten (Gitterecken), Löcher als eigene Konturen.
pub fn trace(pixels: &[u8], w: usize, h: usize, p: &TraceParams) -> Vec<Vec<Pt>> {
    if pixels.len() != w * h || w == 0 || h == 0 {
        return Vec::new();
    }
    let fg = |x: i64, y: i64| -> bool {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            return false; // außerhalb = Hintergrund (Rand wird Kontur)
        }
        let dark = pixels[y as usize * w + x as usize] < p.threshold;
        dark != p.invert
    };

    // 1. Randkanten sammeln: für jedes Vordergrund-Pixel je Kante zum
    //    Hintergrund eine gerichtete Einheitskante (Vordergrund links der
    //    Laufrichtung ⇒ Außenränder CCW, Löcher CW — konsistent verkettbar).
    //    An Diagonal-Berührungen starten ZWEI Kanten an derselben Ecke,
    //    deshalb Vec statt Einzelwert.
    let mut edges: HashMap<(i64, i64), Vec<(i64, i64)>> = HashMap::new();
    let add = |from: (i64, i64), to: (i64, i64), edges: &mut HashMap<_, Vec<(i64, i64)>>| {
        edges.entry(from).or_default().push(to);
    };
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            if !fg(x, y) {
                continue;
            }
            if !fg(x, y - 1) {
                add((x, y), (x + 1, y), &mut edges); // obere Kante →
            }
            if !fg(x, y + 1) {
                add((x + 1, y + 1), (x, y + 1), &mut edges); // untere Kante ←
            }
            if !fg(x - 1, y) {
                add((x, y + 1), (x, y), &mut edges); // linke Kante ↑
            }
            if !fg(x + 1, y) {
                add((x + 1, y), (x + 1, y + 1), &mut edges); // rechte Kante ↓
            }
        }
    }

    // 2. Kanten zu geschlossenen Schleifen verketten. Bei mehreren Abgängen
    //    (Diagonal-Berührung) den **linksdrehendsten** wählen (max. Kreuz-
    //    produkt zur Einlaufrichtung) — so bleibt jede Schleife an ihrer
    //    Region statt in die Nachbar-Region zu springen.
    let take_next = |edges: &mut HashMap<(i64, i64), Vec<(i64, i64)>>,
                     cur: (i64, i64),
                     dir_in: (i64, i64)|
     -> Option<(i64, i64)> {
        let cands = edges.get_mut(&cur)?;
        if cands.is_empty() {
            return None;
        }
        let mut best_i = 0;
        if cands.len() > 1 {
            let mut best_cross = i64::MIN;
            for (i, &(nx, ny)) in cands.iter().enumerate() {
                let d_out = (nx - cur.0, ny - cur.1);
                let cross = dir_in.0 * d_out.1 - dir_in.1 * d_out.0;
                if cross > best_cross {
                    best_cross = cross;
                    best_i = i;
                }
            }
        }
        let next = cands.swap_remove(best_i);
        if cands.is_empty() {
            edges.remove(&cur);
        }
        Some(next)
    };

    let mut contours: Vec<Vec<Pt>> = Vec::new();
    while let Some(&start) = edges.keys().next() {
        // Erste Kante der Schleife direkt entnehmen.
        let first = {
            let v = edges.get_mut(&start).unwrap();
            let f = v.swap_remove(0);
            if v.is_empty() {
                edges.remove(&start);
            }
            f
        };
        let mut loop_pts: Vec<Pt> = vec![(start.0 as f64, start.1 as f64)];
        let mut dir_in = (first.0 - start.0, first.1 - start.1);
        let mut cur = first;
        while cur != start {
            loop_pts.push((cur.0 as f64, cur.1 as f64));
            let Some(next) = take_next(&mut edges, cur, dir_in) else {
                break; // abgerissen (sollte nicht passieren) — Schleife verwerfen
            };
            dir_in = (next.0 - cur.0, next.1 - cur.1);
            cur = next;
        }
        if cur == start && loop_pts.len() >= 4 {
            contours.push(loop_pts);
        }
    }

    // 3. Kollineare Treppenpunkte zusammenfassen, filtern, vereinfachen, glätten.
    let mut out = Vec::new();
    for c in contours {
        let mut pts = merge_collinear(&c);
        if area(&pts).abs() < p.min_area_px {
            continue;
        }
        if p.simplify_px > 0.0 {
            pts = simplify_closed(&pts, p.simplify_px);
        }
        for _ in 0..p.smooth {
            pts = chaikin_closed(&pts);
        }
        if pts.len() >= 3 {
            out.push(pts);
        }
    }
    out
}

/// Signierte Fläche (Shoelace).
fn area(pts: &[Pt]) -> f64 {
    let n = pts.len();
    let mut a = 0.0;
    for i in 0..n {
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[(i + 1) % n];
        a += x1 * y2 - x2 * y1;
    }
    a / 2.0
}

/// Entfernt Zwischenpunkte auf geraden Strecken (die Treppenkanten erzeugen
/// viele kollineare Punkte — vorm DP billig zusammenfassen).
fn merge_collinear(pts: &[Pt]) -> Vec<Pt> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let a = pts[(i + n - 1) % n];
        let b = pts[i];
        let c = pts[(i + 1) % n];
        let cross = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        if cross.abs() > 1e-9 {
            out.push(b);
        }
    }
    if out.len() < 3 {
        pts.to_vec()
    } else {
        out
    }
}

/// Douglas-Peucker für geschlossene Konturen: am weitesten entfernten
/// Punktepaar aufteilen, beide Hälften offen vereinfachen.
fn simplify_closed(pts: &[Pt], eps: f64) -> Vec<Pt> {
    let n = pts.len();
    if n < 4 {
        return pts.to_vec();
    }
    // Punkt mit größtem Abstand zu pts[0] als zweiter Anker.
    let mut far = 1;
    let mut best = 0.0;
    for (i, &q) in pts.iter().enumerate().skip(1) {
        let d = (q.0 - pts[0].0).hypot(q.1 - pts[0].1);
        if d > best {
            best = d;
            far = i;
        }
    }
    let mut half1: Vec<Pt> = pts[0..=far].to_vec();
    let mut half2: Vec<Pt> = pts[far..].to_vec();
    half2.push(pts[0]);
    half1 = dp(&half1, eps);
    half2 = dp(&half2, eps);
    // zusammensetzen ohne doppelte Nahtpunkte
    let mut out = half1;
    out.pop();
    out.extend_from_slice(&half2[..half2.len() - 1]);
    out
}

/// Klassischer (offener) Douglas-Peucker.
fn dp(pts: &[Pt], eps: f64) -> Vec<Pt> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let mut best = 0.0;
    let mut idx = 0;
    for (i, &q) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = perp_dist(q, a, b);
        if d > best {
            best = d;
            idx = i;
        }
    }
    if best <= eps {
        return vec![a, b];
    }
    let mut left = dp(&pts[..=idx], eps);
    let right = dp(&pts[idx..], eps);
    left.pop();
    left.extend(right);
    left
}

/// Senkrechter Abstand von `q` zur Strecke a–b.
fn perp_dist(q: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-18 {
        return (q.0 - a.0).hypot(q.1 - a.1);
    }
    ((q.0 - a.0) * dy - (q.1 - a.1) * dx).abs() / len2.sqrt()
}

/// Ein Chaikin-Durchlauf (Ecken schneiden) für geschlossene Konturen.
fn chaikin_closed(pts: &[Pt]) -> Vec<Pt> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        out.push((a.0 * 0.75 + b.0 * 0.25, a.1 * 0.75 + b.1 * 0.25));
        out.push((a.0 * 0.25 + b.0 * 0.75, a.1 * 0.25 + b.1 * 0.75));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bild mit schwarzem Rechteck (Pixel [x0..x1) × [y0..y1) dunkel).
    fn img_rect(w: usize, h: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> Vec<u8> {
        let mut px = vec![255u8; w * h];
        for y in y0..y1 {
            for x in x0..x1 {
                px[y * w + x] = 0;
            }
        }
        px
    }

    fn raw_params() -> TraceParams {
        TraceParams {
            smooth: 0,
            simplify_px: 0.0,
            min_area_px: 0.0,
            ..Default::default()
        }
    }

    #[test]
    fn rechteck_ergibt_eine_kontur_mit_vier_ecken() {
        let px = img_rect(20, 20, 5, 5, 15, 12);
        let out = trace(&px, 20, 20, &raw_params());
        assert_eq!(out.len(), 1);
        // Nach merge_collinear bleiben genau die 4 Ecken.
        assert_eq!(out[0].len(), 4, "war {:?}", out[0]);
        assert!((area(&out[0]).abs() - 70.0) < 1e-9, "10×7 Pixel Fläche");
    }

    #[test]
    fn loch_wird_eigene_kontur() {
        // Schwarzer Rahmen: außen 16×16, innen 8×8 weiß.
        let mut px = img_rect(20, 20, 2, 2, 18, 18);
        for y in 6..14 {
            for x in 6..14 {
                px[y * 20 + x] = 255;
            }
        }
        let out = trace(&px, 20, 20, &raw_params());
        assert_eq!(out.len(), 2, "Außenrand + Loch");
        // Löcher laufen andersherum: Vorzeichen der Flächen unterscheiden sich.
        let s0 = area(&out[0]).signum();
        let s1 = area(&out[1]).signum();
        assert_ne!(s0, s1, "Loch hat entgegengesetzten Umlaufsinn");
    }

    #[test]
    fn min_area_filtert_staub() {
        // Großes Rechteck + 1 einzelnes Pixel.
        let mut px = img_rect(30, 30, 5, 5, 20, 20);
        px[25 * 30 + 25] = 0;
        let mut p = raw_params();
        p.min_area_px = 4.0;
        let out = trace(&px, 30, 30, &p);
        assert_eq!(out.len(), 1, "Staubpixel weggefiltert");
    }

    #[test]
    fn invert_tracet_helles_motiv() {
        // Weißes Rechteck auf schwarzem Grund + invert.
        let mut px = vec![0u8; 20 * 20];
        for y in 5..15 {
            for x in 5..15 {
                px[y * 20 + x] = 255;
            }
        }
        let mut p = raw_params();
        p.invert = true;
        let out = trace(&px, 20, 20, &p);
        assert_eq!(out.len(), 1);
        assert!((area(&out[0]).abs() - 100.0) < 1e-9);
    }

    #[test]
    fn simplify_und_smooth_liefern_gueltige_konturen() {
        let px = img_rect(40, 40, 5, 5, 35, 35);
        let p = TraceParams::default();
        let out = trace(&px, 40, 40, &p);
        assert_eq!(out.len(), 1);
        assert!(out[0].len() >= 3);
    }

    #[test]
    fn leeres_bild_ergibt_nichts() {
        let px = vec![255u8; 100];
        assert!(trace(&px, 10, 10, &raw_params()).is_empty());
    }
}
