//! Muster-Füllung („Pattern Fill"): füllt geschlossene Konturen mit einem von
//! vier Mustern — parallele **Linien**, **Kreise**, **Slots** (Langlöcher),
//! **Waben** (Hexagone). Reine mm-Geometrie, UI-frei, testbar.
//!
//! Nach v1/v3-Referenz neu gebaut (pattern_fill.js / pattern_fill.rs
//! analysiert, nicht kopiert). Übernommene Kernideen:
//! - Das Muster wird im **Rasterraum** erzeugt: am Flächen-**Centroid** der
//!   Kontur verankert (Rand ringsum gleichmäßig) und um `angle` gedreht.
//! - Randelemente werden mit einem **Midpoint-Clipper** gegen die Kontur
//!   geschnitten (Segment-Teilstücke, deren Mittelpunkt innen liegt, bleiben)
//!   — bewusst kein Polygon-Boolean, der an tangentialen Ecken versagt.
//! - Löcher: mehrere Ringe werden Even-Odd ausgewertet.
//!
//! Abweichung von v1: dort war die Füllung nicht-destruktives Metadatum am
//! Objekt; hier erzeugt sie (Schnitt 1) echte Polylinien mit Undo. Ein
//! nicht-destruktives Shape-Metadatum kann später darauf aufsetzen.

use crate::geometry::{rotate_point, Pt};
use crate::scanline::{fill_segments, Contour};

/// Die vier Füllmuster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    /// Parallele Linien (Abstand `gap_y`, gedreht um `angle`).
    Lines,
    /// Kreisgitter (Durchmesser `size`, Abstände `gap_x`/`gap_y`).
    Circles,
    /// Langloch-Gitter (Länge `size`·2, Breite `size`, versetzt je Zeile).
    Slots,
    /// Wabengitter (Sechseck-„Radius" `size`).
    Hex,
}

impl Pattern {
    /// Aus dem Frontend-String ("lines" | "circles" | "slots" | "hex").
    pub fn from_key(s: &str) -> Option<Self> {
        match s {
            "lines" => Some(Pattern::Lines),
            "circles" => Some(Pattern::Circles),
            "slots" => Some(Pattern::Slots),
            "hex" => Some(Pattern::Hex),
            _ => None,
        }
    }
}

/// Füll-Parameter (mm/Grad). Bedeutung wie in der Referenz: `gap_x`/`gap_y`
/// sind die Rasterabstände, `size` die Elementgröße, `angle` dreht das Raster.
#[derive(Debug, Clone, Copy)]
pub struct FillParams {
    pub pattern: Pattern,
    pub gap_x: f64,
    pub gap_y: f64,
    pub angle_deg: f64,
    pub size: f64,
}

impl Default for FillParams {
    fn default() -> Self {
        Self {
            pattern: Pattern::Lines,
            gap_x: 4.0,
            gap_y: 4.0,
            angle_deg: 0.0,
            size: 2.0,
        }
    }
}

/// Füllt die Ringe (geschlossene Konturen in mm; Even-Odd — Löcher werden
/// ausgespart) mit dem Muster. Ergebnis: Konturen (offen/geschlossen), die
/// sich wie normale Polylinien zeichnen und lasern lassen.
pub fn fill_pattern(rings: &[Vec<Pt>], p: &FillParams) -> Vec<(Vec<Pt>, bool)> {
    let rings: Vec<&Vec<Pt>> = rings.iter().filter(|r| r.len() >= 3).collect();
    if rings.is_empty() {
        return Vec::new();
    }
    let anchor = centroid(rings[0]);

    match p.pattern {
        Pattern::Lines => lines_fill(&rings, p, anchor),
        Pattern::Circles => {
            let r = (p.size / 2.0).max(0.05);
            element_grid(
                &rings,
                p,
                anchor,
                p.size + p.gap_x,
                p.size + p.gap_y,
                false,
                |cx, cy| circle_poly(cx, cy, r, 24),
            )
        }
        Pattern::Slots => {
            let r = (p.size / 2.0).max(0.05);
            let len = p.size * 2.0;
            element_grid(
                &rings,
                p,
                anchor,
                len + p.gap_x,
                p.size + p.gap_y,
                true, // Zeilen versetzt (halber Schritt) — wie die Referenz
                move |cx, cy| slot_poly(cx, cy, len, r, 12),
            )
        }
        Pattern::Hex => {
            // Wabengitter (wie die Referenz): Pointy-Top-Sechsecke,
            // step_x = √3·(r+gap), step_y = 1,5·(r+gap), ungerade Zeilen um den
            // halben X-Schritt versetzt — die klassische Honeycomb-Packung.
            let r = p.size.max(0.2);
            let gap = ((p.gap_x + p.gap_y) / 2.0).max(0.0);
            let rg = r + gap;
            element_grid(
                &rings,
                p,
                anchor,
                (3.0f64).sqrt() * rg,
                1.5 * rg,
                true,
                move |cx, cy| hex_poly(cx, cy, r),
            )
        }
    }
}

// ── Muster: parallele Linien ─────────────────────────────────────────────────

/// Linienfüllung: Ringe um −angle drehen, horizontale Scanline (Abstand
/// `gap_y`), Segmente zurückdrehen. Serpentinen-Reihenfolge.
fn lines_fill(rings: &[&Vec<Pt>], p: &FillParams, anchor: Pt) -> Vec<(Vec<Pt>, bool)> {
    let step = p.gap_y.max(0.05);
    let rotated: Vec<Vec<Pt>> = rings
        .iter()
        .map(|r| {
            r.iter()
                .map(|&(x, y)| rotate_point(x, y, anchor.0, anchor.1, -p.angle_deg))
                .collect()
        })
        .collect();
    let contours: Vec<Contour> = rotated
        .iter()
        .map(|pts| Contour {
            points: pts,
            closed: true,
        })
        .collect();
    let mut out = Vec::new();
    let mut flip = false;
    let mut last_y = f64::NAN;
    for s in fill_segments(&contours, step) {
        if s.y != last_y {
            flip = !flip;
            last_y = s.y;
        }
        let a = rotate_point(s.x0, s.y, anchor.0, anchor.1, p.angle_deg);
        let b = rotate_point(s.x1, s.y, anchor.0, anchor.1, p.angle_deg);
        out.push((if flip { vec![b, a] } else { vec![a, b] }, false));
    }
    out
}

// ── Muster: Element-Gitter (Kreise/Slots/Waben) ──────────────────────────────

/// Legt ein Gitter von Element-Polygonen (Schritt `step_x`/`step_y`, optional
/// zeilenversetzt) über die Ringe — im Rasterraum (um −angle am Anker gedreht)
/// — und clippt jedes Element gegen die Konturen.
fn element_grid<F>(
    rings: &[&Vec<Pt>],
    p: &FillParams,
    anchor: Pt,
    step_x: f64,
    step_y: f64,
    stagger: bool,
    make: F,
) -> Vec<(Vec<Pt>, bool)>
where
    F: Fn(f64, f64) -> Vec<Pt>,
{
    let step_x = step_x.max(0.1);
    let step_y = step_y.max(0.1);

    // Arbeitsraum: Ringe in den Rasterraum drehen. Geclippt wird dort, das
    // Ergebnis am Ende zurückgedreht — so bleibt das Clipping achsenneutral.
    let work: Vec<Vec<Pt>> = rings
        .iter()
        .map(|r| {
            r.iter()
                .map(|&(x, y)| rotate_point(x, y, anchor.0, anchor.1, -p.angle_deg))
                .collect()
        })
        .collect();
    let Some((min_x, min_y, max_x, max_y)) = bbox(&work) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    // Vom Anker aus in beide Richtungen rastern (Rand ringsum gleichmäßig).
    let n_left = ((anchor.0 - min_x) / step_x).ceil() as i64 + 1;
    let n_right = ((max_x - anchor.0) / step_x).ceil() as i64 + 1;
    let n_up = ((anchor.1 - min_y) / step_y).ceil() as i64 + 1;
    let n_down = ((max_y - anchor.1) / step_y).ceil() as i64 + 1;
    for iy in -n_up..=n_down {
        let cy = anchor.1 + iy as f64 * step_y;
        let x_off = if stagger && iy.rem_euclid(2) == 1 {
            step_x / 2.0
        } else {
            0.0
        };
        for ix in -n_left..=n_right {
            let cx = anchor.0 + ix as f64 * step_x + x_off;
            let poly = make(cx, cy);
            for (piece, closed) in clip_element(&poly, &work) {
                // Zurück in den Weltraum drehen.
                let world: Vec<Pt> = piece
                    .iter()
                    .map(|&(x, y)| rotate_point(x, y, anchor.0, anchor.1, p.angle_deg))
                    .collect();
                out.push((world, closed));
            }
        }
    }
    out
}

/// Midpoint-Clipper: schneidet ein geschlossenes Element-Polygon gegen die
/// Ringe (Even-Odd). Liegt es komplett innen → unverändert (geschlossen).
/// Sonst bleiben die Kanten-Teilstücke, deren Mittelpunkt innen liegt (offen).
fn clip_element(poly: &[Pt], rings: &[Vec<Pt>]) -> Vec<(Vec<Pt>, bool)> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    // Schneller Komplett-Test: alle Ecken innen und keine Kante schneidet.
    let all_inside = poly.iter().all(|&(x, y)| inside(x, y, rings));
    if all_inside && !any_edge_crosses(poly, rings) {
        return vec![(poly.to_vec(), true)];
    }

    // Randfall: Teilstücke sammeln. Offene Ketten aus behaltenen Stücken.
    let mut chains: Vec<Vec<Pt>> = Vec::new();
    let mut cur: Vec<Pt> = Vec::new();
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        for (sa, sb) in split_segment(a, b, rings) {
            let mid = ((sa.0 + sb.0) / 2.0, (sa.1 + sb.1) / 2.0);
            if inside(mid.0, mid.1, rings) {
                if cur.is_empty() {
                    cur.push(sa);
                } else {
                    let last = *cur.last().unwrap();
                    if (last.0 - sa.0).hypot(last.1 - sa.1) > 1e-9 {
                        chains.push(std::mem::take(&mut cur));
                        cur.push(sa);
                    }
                }
                cur.push(sb);
            } else if !cur.is_empty() {
                chains.push(std::mem::take(&mut cur));
            }
        }
    }
    if !cur.is_empty() {
        chains.push(cur);
    }
    chains
        .into_iter()
        .filter(|c| c.len() >= 2)
        .map(|c| (c, false))
        .collect()
}

/// Teilt die Strecke a–b an allen Schnittpunkten mit den Ring-Kanten.
fn split_segment(a: Pt, b: Pt, rings: &[Vec<Pt>]) -> Vec<(Pt, Pt)> {
    let mut ts = vec![0.0, 1.0];
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            let c = ring[i];
            let d = ring[(i + 1) % n];
            if let Some(t) = seg_intersect_t(a, b, c, d) {
                ts.push(t);
            }
        }
    }
    ts.sort_by(|x, y| x.partial_cmp(y).unwrap());
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    let lerp = |t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    ts.windows(2).map(|w| (lerp(w[0]), lerp(w[1]))).collect()
}

/// Parameter t des Schnitts von a–b mit c–d (beide als Strecken), falls vorhanden.
fn seg_intersect_t(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<f64> {
    let r = (b.0 - a.0, b.1 - a.1);
    let s = (d.0 - c.0, d.1 - c.1);
    let denom = r.0 * s.1 - r.1 * s.0;
    if denom.abs() < 1e-12 {
        return None;
    }
    let qp = (c.0 - a.0, c.1 - a.1);
    let t = (qp.0 * s.1 - qp.1 * s.0) / denom;
    let u = (qp.0 * r.1 - qp.1 * r.0) / denom;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some(t)
    } else {
        None
    }
}

fn any_edge_crosses(poly: &[Pt], rings: &[Vec<Pt>]) -> bool {
    let n = poly.len();
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        for ring in rings {
            let m = ring.len();
            for j in 0..m {
                if seg_intersect_t(a, b, ring[j], ring[(j + 1) % m]).is_some() {
                    return true;
                }
            }
        }
    }
    false
}

/// Even-Odd-Punkttest über alle Ringe (Löcher zählen doppelt = außen).
fn inside(x: f64, y: f64, rings: &[Vec<Pt>]) -> bool {
    let mut odd = false;
    for ring in rings {
        let n = ring.len();
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = ring[i];
            let (xj, yj) = ring[j];
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                odd = !odd;
            }
            j = i;
        }
    }
    odd
}

/// Flächen-Centroid; bei entartetem Polygon der bbox-Mittelpunkt.
fn centroid(ring: &[Pt]) -> Pt {
    let n = ring.len();
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = ring[i];
        let (xj, yj) = ring[j];
        let cross = xj * yi - xi * yj;
        a += cross;
        cx += (xi + xj) * cross;
        cy += (yi + yj) * cross;
        j = i;
    }
    if a.abs() < 1e-9 {
        let bb = bbox(&[ring.to_vec()]).unwrap_or((0.0, 0.0, 0.0, 0.0));
        return ((bb.0 + bb.2) / 2.0, (bb.1 + bb.3) / 2.0);
    }
    (cx / (3.0 * a), cy / (3.0 * a))
}

fn bbox(rings: &[Vec<Pt>]) -> Option<(f64, f64, f64, f64)> {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut any = false;
    for r in rings {
        for &(x, y) in r {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            any = true;
        }
    }
    any.then_some((min_x, min_y, max_x, max_y))
}

fn circle_poly(cx: f64, cy: f64, r: f64, segs: usize) -> Vec<Pt> {
    (0..segs)
        .map(|i| {
            let a = i as f64 / segs as f64 * std::f64::consts::TAU;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// Langloch: Rechteck mit Halbkreis-Kappen (Gesamtlänge `len`, Radius `r`).
fn slot_poly(cx: f64, cy: f64, len: f64, r: f64, cap_segs: usize) -> Vec<Pt> {
    let half = (len / 2.0 - r).max(0.0);
    let mut pts = Vec::new();
    // rechte Kappe (von oben nach unten)
    for i in 0..=cap_segs {
        let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / cap_segs as f64;
        pts.push((cx + half + r * a.cos(), cy + r * a.sin()));
    }
    // linke Kappe (von unten nach oben)
    for i in 0..=cap_segs {
        let a = std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / cap_segs as f64;
        pts.push((cx - half + r * a.cos(), cy + r * a.sin()));
    }
    pts
}

/// Pointy-Top-Sechseck (erste Ecke oben, Start 90°) — passend zur
/// Honeycomb-Packung (Breite √3·r, Zeilenhöhe 1,5·r).
fn hex_poly(cx: f64, cy: f64, r: f64) -> Vec<Pt> {
    (0..6)
        .map(|i| {
            let a = std::f64::consts::FRAC_PI_2 + i as f64 / 6.0 * std::f64::consts::TAU;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

// ── AppState-Anbindung ───────────────────────────────────────────────────────

use crate::geometry::Geo;
use crate::state::AppState;

impl AppState {
    /// Füllt die Auswahl mit dem Muster (ein Undo-Punkt). Alle selektierten
    /// **geschlossenen** Konturen werden gemeinsam als Ringe behandelt —
    /// eine innere Kontur wirkt so automatisch als Loch (Even-Odd).
    ///
    /// Die Füllung landet auf einem **eigenen Layer mit eigener Farbe**
    /// (Farbe = Layer = Parametersatz): so bleibt die Kontur z. B. „Schneiden",
    /// während das Muster unabhängig graviert wird — sonst würde ein
    /// Cut-Layer das Muster mit ausschneiden. Die Original-Konturen bleiben.
    pub fn pattern_fill_selected(&mut self, p: &FillParams) {
        let mut rings: Vec<Vec<Pt>> = Vec::new();
        for &i in &self.selected {
            let Some(s) = self.shapes.get(i) else {
                continue;
            };
            if matches!(s.geo, Geo::Image { .. }) {
                continue;
            }
            let (mut pts, closed) = s.geo.outline_points();
            if !closed || pts.len() < 3 {
                continue;
            }
            if s.rotation != 0.0 {
                let (cx, cy) = s.bbox().center();
                for q in pts.iter_mut() {
                    *q = rotate_point(q.0, q.1, cx, cy, s.rotation);
                }
            }
            rings.push(pts);
        }
        if rings.is_empty() {
            return;
        }
        let filled = fill_pattern(&rings, p);
        if filled.is_empty() {
            return;
        }
        self.push_undo();
        // Eigener Layer in der nächsten freien Katalogfarbe.
        let used: Vec<[u8; 3]> = self.layers.iter().map(|l| l.color).collect();
        let color = crate::model::SWATCH_COLORS
            .iter()
            .find(|c| !used.contains(c))
            .copied()
            .unwrap_or(
                crate::model::SWATCH_COLORS[self.layers.len() % crate::model::SWATCH_COLORS.len()],
            );
        let layer_id = self.layers.len();
        let mut layer = crate::model::Layer::with_color(layer_id, color);
        layer.name = "Muster".into();
        self.layers.push(layer);

        self.selected.clear();
        for (pts, closed) in filled {
            let idx = self.shapes.len();
            self.shapes.push(crate::model::Shape::new(
                layer_id,
                Geo::Polyline { pts, closed },
            ));
            self.selected.push(idx);
        }
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64) -> Vec<Pt> {
        vec![(x, y), (x + s, y), (x + s, y + s), (x, y + s)]
    }

    #[test]
    fn linien_fuellen_das_quadrat() {
        let p = FillParams {
            pattern: Pattern::Lines,
            gap_y: 2.0,
            ..Default::default()
        };
        let out = fill_pattern(&[square(0.0, 0.0, 20.0)], &p);
        assert!(out.len() >= 8, "war {}", out.len());
        assert!(out.iter().all(|(_, closed)| !closed));
    }

    #[test]
    fn linien_mit_winkel_sind_gedreht() {
        let p = FillParams {
            pattern: Pattern::Lines,
            gap_y: 2.0,
            angle_deg: 45.0,
            ..Default::default()
        };
        let out = fill_pattern(&[square(0.0, 0.0, 20.0)], &p);
        for (pts, _) in &out {
            let (a, b) = (pts[0], pts[1]);
            let (dx, dy) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
            assert!((dx - dy).abs() < 1e-6, "45°: |dx|=|dy|");
        }
    }

    #[test]
    fn kreise_innen_bleiben_geschlossen_randkreise_werden_geclippt() {
        let p = FillParams {
            pattern: Pattern::Circles,
            gap_x: 2.0,
            gap_y: 2.0,
            size: 3.0,
            ..Default::default()
        };
        let out = fill_pattern(&[square(0.0, 0.0, 30.0)], &p);
        let closed = out.iter().filter(|(_, c)| *c).count();
        let open = out.iter().filter(|(_, c)| !*c).count();
        assert!(closed > 4, "innere Kreise ganz ({closed})");
        assert!(open > 0, "Randkreise geclippt ({open})");
        // Alle Punkte aller Stücke liegen (fast) in der Kontur.
        for (pts, _) in &out {
            for &(x, y) in pts {
                assert!(
                    (-0.01..=30.01).contains(&x) && (-0.01..=30.01).contains(&y),
                    "Punkt außerhalb: {x},{y}"
                );
            }
        }
    }

    #[test]
    fn loch_wird_ausgespart() {
        // 30er-Quadrat mit 10er-Loch mittig: keine geschlossenen Elemente im Loch.
        let outer = square(0.0, 0.0, 30.0);
        let hole = square(10.0, 10.0, 10.0);
        let p = FillParams {
            pattern: Pattern::Circles,
            gap_x: 1.0,
            gap_y: 1.0,
            size: 2.0,
            ..Default::default()
        };
        let out = fill_pattern(&[outer, hole], &p);
        for (pts, closed) in &out {
            if !closed {
                continue;
            }
            let (cx, cy) = centroid(pts);
            let in_hole = (11.0..19.0).contains(&cx) && (11.0..19.0).contains(&cy);
            assert!(!in_hole, "geschlossenes Element im Loch bei {cx},{cy}");
        }
    }

    #[test]
    fn waben_und_slots_liefern_elemente() {
        for pat in [Pattern::Hex, Pattern::Slots] {
            let p = FillParams {
                pattern: pat,
                gap_x: 1.0,
                gap_y: 1.0,
                size: 2.0,
                ..Default::default()
            };
            let out = fill_pattern(&[square(0.0, 0.0, 30.0)], &p);
            assert!(!out.is_empty(), "{pat:?} leer");
        }
    }

    #[test]
    fn leere_eingabe_ist_leer() {
        assert!(fill_pattern(&[], &FillParams::default()).is_empty());
    }
}
