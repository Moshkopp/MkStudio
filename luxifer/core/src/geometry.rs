//! Reine 2D-Geometrie in Millimetern — UI-frei und vollständig testbar.
//!
//! Angelehnt an ThorBurns `core/state.rs`-`Geo` (siehe
//! docs/referenz/01-thorburn-analyse.md), aber neu und aufgeräumt implementiert.

use serde::{Deserialize, Serialize};

/// Ein Punkt in Millimetern.
pub type Pt = (f64, f64);

/// Achsenparallele Bounding-Box in mm: linke obere Ecke + Größe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl BBox {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// Mittelpunkt der Box.
    pub fn center(&self) -> Pt {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// Enthält die Box den Punkt (mit Toleranz in mm)?
    pub fn contains(&self, px: f64, py: f64, tol: f64) -> bool {
        px >= self.x - tol
            && px <= self.x + self.w + tol
            && py >= self.y - tol
            && py <= self.y + self.h + tol
    }

    /// Umschließt alle übergebenen Boxen. `None`, wenn leer.
    pub fn union_all(boxes: impl IntoIterator<Item = BBox>) -> Option<BBox> {
        let mut it = boxes.into_iter();
        let first = it.next()?;
        let mut min_x = first.x;
        let mut min_y = first.y;
        let mut max_x = first.x + first.w;
        let mut max_y = first.y + first.h;
        for b in it {
            min_x = min_x.min(b.x);
            min_y = min_y.min(b.y);
            max_x = max_x.max(b.x + b.w);
            max_y = max_y.max(b.y + b.h);
        }
        Some(BBox::new(min_x, min_y, max_x - min_x, max_y - min_y))
    }
}

/// Spiegelachse für `Geo::mirror`. `Vertical` spiegelt an einer senkrechten
/// Linie x=coord (tauscht links↔rechts), `Horizontal` an y=coord (oben↔unten).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Vertical,
    Horizontal,
}

/// Wie ein Bild fürs Lasern in 1-Bit umgesetzt wird (ADR 0004 §3/§5).
/// `Grayscale`/`Threshold` = Schwellwert-Pfad (Strichgrafik, raster.rs);
/// die übrigen sind Dither-Verfahren für Fotos (dither.rs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ImageMode {
    /// Reine Graustufe (kein Schwellwert).
    #[default]
    Grayscale,
    /// Harte Schwelle: Pixel ≥ `threshold` → hell, sonst dunkel.
    Threshold,
    /// Floyd–Steinberg-Fehlerdiffusion (Standard für Fotos).
    Floyd,
    /// Jarvis–Judice–Ninke (weicher, breiter Kernel).
    Jarvis,
    /// Stucki (wie Jarvis, kantenschärfer).
    Stucki,
    /// Atkinson (heller/luftiger, klassischer Mac-Look).
    Atkinson,
    /// Geordnetes 4×4-Bayer-Raster (regelmäßiges Muster).
    Bayer,
    /// Floyd mit Scan-Hysterese: lange Brennstrecken statt Einzelpixel —
    /// zündet auf Röhrenlasern zuverlässiger.
    LaserRuns,
}

/// Nicht-destruktive Bildverarbeitungs-Parameter (ADR 0004 §3). Wirken erst bei
/// Vorschau/Rastern; das Store-Asset bleibt unverändert.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImageParams {
    pub mode: ImageMode,
    /// Schwellwert 0..255 (nur bei `Threshold`).
    pub threshold: u8,
    /// Helligkeit −100..+100.
    pub brightness: i32,
    /// Kontrast −100..+100.
    pub contrast: i32,
    /// Gamma 0.1..3.0.
    pub gamma: f64,
    /// Invertiert die Canvas-Darstellung.
    pub invert_editor: bool,
    /// Invertiert nur die Laser-/Rastervorschau (nicht das Canvas).
    pub invert_laser: bool,
}

impl Default for ImageParams {
    fn default() -> Self {
        Self {
            mode: ImageMode::Grayscale,
            threshold: 128,
            brightness: 0,
            contrast: 0,
            gamma: 1.0,
            invert_editor: false,
            invert_laser: false,
        }
    }
}

/// Die Geometrie-Typen einer Form. Maße in mm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Geo {
    /// Linke obere Ecke + Breite/Höhe.
    Rect { x: f64, y: f64, w: f64, h: f64 },
    /// Mittelpunkt + Halbachsen.
    Ellipse { cx: f64, cy: f64, rx: f64, ry: f64 },
    /// Offene oder geschlossene Punktfolge.
    Polyline { pts: Vec<Pt>, closed: bool },
    /// Importiertes Bild: Verweis auf ein Store-Asset (ID = Content-Hash) plus
    /// achsenparallele Box (linke obere Ecke + Größe) und Verarbeitungsparameter.
    /// Verhält sich geometrisch wie ein `Rect` (Box); die Pixel liegen im Store.
    Image {
        asset: String,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        params: ImageParams,
    },
}

impl Geo {
    /// Achsenparallele Bounding-Box (ohne Rotation).
    pub fn bbox(&self) -> BBox {
        match self {
            Geo::Rect { x, y, w, h } => BBox::new(*x, *y, *w, *h),
            Geo::Image { x, y, w, h, .. } => BBox::new(*x, *y, *w, *h),
            Geo::Ellipse { cx, cy, rx, ry } => BBox::new(cx - rx, cy - ry, rx * 2.0, ry * 2.0),
            Geo::Polyline { pts, .. } => {
                if pts.is_empty() {
                    return BBox::new(0.0, 0.0, 0.0, 0.0);
                }
                let mut min_x = f64::MAX;
                let mut min_y = f64::MAX;
                let mut max_x = f64::MIN;
                let mut max_y = f64::MIN;
                for &(px, py) in pts {
                    min_x = min_x.min(px);
                    min_y = min_y.min(py);
                    max_x = max_x.max(px);
                    max_y = max_y.max(py);
                }
                BBox::new(min_x, min_y, max_x - min_x, max_y - min_y)
            }
        }
    }

    /// Ob die Form eine füllbare Fläche umschließt (geschlossen).
    /// Nur so eine Form wird auf Fill-/Raster-Layern flächig dargestellt.
    pub fn is_fillable(&self) -> bool {
        match self {
            Geo::Rect { .. } | Geo::Ellipse { .. } | Geo::Image { .. } => true,
            Geo::Polyline { closed, .. } => *closed,
        }
    }

    /// Trifft ein Punkt (mm) die Form? `tol` = Toleranz in mm.
    /// Rect/Ellipse: Fläche. Ellipse exakt über die Ellipsengleichung.
    /// Polyline: Abstand zu einem Segment ≤ Toleranz (geschlossen inkl. Schlusskante).
    pub fn hit_test(&self, px: f64, py: f64, tol: f64) -> bool {
        match self {
            Geo::Rect { .. } | Geo::Image { .. } => self.bbox().contains(px, py, tol),
            Geo::Ellipse { cx, cy, rx, ry } => {
                let rx = rx + tol;
                let ry = ry + tol;
                if rx <= 0.0 || ry <= 0.0 {
                    return false;
                }
                let dx = px - cx;
                let dy = py - cy;
                (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry) <= 1.0
            }
            Geo::Polyline { pts, closed } => {
                if pts.len() < 2 {
                    return false;
                }
                for w in pts.windows(2) {
                    if point_segment_distance(px, py, w[0], w[1]) <= tol {
                        return true;
                    }
                }
                if *closed {
                    let a = pts[pts.len() - 1];
                    let b = pts[0];
                    if point_segment_distance(px, py, a, b) <= tol {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Verschiebt die Form um (dx, dy) in mm.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        match self {
            Geo::Rect { x, y, .. } | Geo::Image { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
            Geo::Ellipse { cx, cy, .. } => {
                *cx += dx;
                *cy += dy;
            }
            Geo::Polyline { pts, .. } => {
                for p in pts.iter_mut() {
                    p.0 += dx;
                    p.1 += dy;
                }
            }
        }
    }

    /// Spiegelt die Form an einer Achse durch `coord`.
    /// `Axis::Vertical` spiegelt x-Koordinaten an x=coord, `Axis::Horizontal`
    /// y-Koordinaten an y=coord. Rect/Ellipse behalten ihre Größe (nur die Lage
    /// klappt um); die Polyline spiegelt jeden Punkt einzeln.
    pub fn mirror(&mut self, axis: Axis, coord: f64) {
        // Spiegelt einen Skalarwert v an der Achsenposition c: v' = 2c - v.
        let flip = |v: f64| 2.0 * coord - v;
        match (axis, self) {
            (Axis::Vertical, Geo::Rect { x, w, .. } | Geo::Image { x, w, .. }) => {
                // Rechte Kante wird zur neuen linken: x' = flip(x + w).
                *x = flip(*x + *w);
            }
            (Axis::Horizontal, Geo::Rect { y, h, .. } | Geo::Image { y, h, .. }) => {
                *y = flip(*y + *h);
            }
            (Axis::Vertical, Geo::Ellipse { cx, .. }) => *cx = flip(*cx),
            (Axis::Horizontal, Geo::Ellipse { cy, .. }) => *cy = flip(*cy),
            (Axis::Vertical, Geo::Polyline { pts, .. }) => {
                for p in pts.iter_mut() {
                    p.0 = flip(p.0);
                }
            }
            (Axis::Horizontal, Geo::Polyline { pts, .. }) => {
                for p in pts.iter_mut() {
                    p.1 = flip(p.1);
                }
            }
        }
    }

    /// Setzt die Bounding-Box neu (Skalieren). Breite/Höhe ≥ `MIN_SIZE`.
    /// Rect/Ellipse rechnen direkt, Polyline skaliert proportional.
    pub fn set_bbox(&mut self, nx: f64, ny: f64, nw: f64, nh: f64) {
        let nw = nw.max(MIN_SIZE);
        let nh = nh.max(MIN_SIZE);
        match self {
            Geo::Rect { x, y, w, h } | Geo::Image { x, y, w, h, .. } => {
                *x = nx;
                *y = ny;
                *w = nw;
                *h = nh;
            }
            Geo::Ellipse { cx, cy, rx, ry } => {
                *cx = nx + nw / 2.0;
                *cy = ny + nh / 2.0;
                *rx = nw / 2.0;
                *ry = nh / 2.0;
            }
            Geo::Polyline { pts, .. } => {
                let b = self_bbox_of_pts(pts);
                let sx = if b.w > 0.0 { nw / b.w } else { 1.0 };
                let sy = if b.h > 0.0 { nh / b.h } else { 1.0 };
                for p in pts.iter_mut() {
                    p.0 = nx + (p.0 - b.x) * sx;
                    p.1 = ny + (p.1 - b.y) * sy;
                }
            }
        }
    }
}

/// Mindestgröße einer Form in mm (verhindert entartete 0-Boxen beim Skalieren).
pub const MIN_SIZE: f64 = 0.1;

// BBox einer Punktliste (Hilfe für Polyline::set_bbox, ohne Geo zu bauen).
fn self_bbox_of_pts(pts: &[Pt]) -> BBox {
    if pts.is_empty() {
        return BBox::new(0.0, 0.0, 0.0, 0.0);
    }
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for &(px, py) in pts {
        min_x = min_x.min(px);
        min_y = min_y.min(py);
        max_x = max_x.max(px);
        max_y = max_y.max(py);
    }
    BBox::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

/// Kürzester Abstand des Punktes (px,py) zum Segment a→b.
pub fn point_segment_distance(px: f64, py: f64, a: Pt, b: Pt) -> f64 {
    let (ax, ay) = a;
    let (bx, by) = b;
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let mut t = ((px - ax) * dx + (py - ay) * dy) / len2;
    t = t.clamp(0.0, 1.0);
    let projx = ax + t * dx;
    let projy = ay + t * dy;
    ((px - projx).powi(2) + (py - projy).powi(2)).sqrt()
}

/// Dreht den Punkt (x,y) um das Zentrum (cx,cy) um `degrees` (im Uhrzeigersinn
/// bei y-nach-unten-Achse). Für Rotations-Hit-Test/Rendering.
pub fn rotate_point(x: f64, y: f64, cx: f64, cy: f64, degrees: f64) -> Pt {
    if degrees == 0.0 {
        return (x, y);
    }
    let rad = degrees.to_radians();
    let (s, c) = rad.sin_cos();
    let dx = x - cx;
    let dy = y - cy;
    (cx + dx * c - dy * s, cy + dx * s + dy * c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_bbox_und_hit() {
        let r = Geo::Rect {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
        };
        let b = r.bbox();
        assert_eq!((b.x, b.y, b.w, b.h), (10.0, 20.0, 30.0, 40.0));
        assert!(r.hit_test(15.0, 25.0, 0.0));
        assert!(!r.hit_test(5.0, 25.0, 0.0));
        assert!(r.hit_test(9.0, 25.0, 2.0)); // mit Toleranz
    }

    #[test]
    fn ellipse_bbox_und_hit() {
        let e = Geo::Ellipse {
            cx: 50.0,
            cy: 50.0,
            rx: 20.0,
            ry: 10.0,
        };
        let b = e.bbox();
        assert_eq!((b.x, b.y, b.w, b.h), (30.0, 40.0, 40.0, 20.0));
        assert!(e.hit_test(50.0, 50.0, 0.0)); // Mitte
        assert!(e.hit_test(69.0, 50.0, 0.0)); // innen am Rand
        assert!(!e.hit_test(50.0, 61.0, 0.0)); // außerhalb (y-Radius 10)
    }

    #[test]
    fn polyline_bbox_umfasst_alle_punkte() {
        let p = Geo::Polyline {
            pts: vec![(10.0, 20.0), (50.0, 5.0), (30.0, 40.0)],
            closed: false,
        };
        let b = p.bbox();
        assert_eq!((b.x, b.y, b.w, b.h), (10.0, 5.0, 40.0, 35.0));
    }

    #[test]
    fn offene_polyline_hit_nur_nahe_der_linie() {
        let p = Geo::Polyline {
            pts: vec![(0.0, 0.0), (100.0, 0.0)],
            closed: false,
        };
        assert!(p.hit_test(50.0, 0.5, 1.0)); // nah an der Linie
        assert!(!p.hit_test(50.0, 20.0, 1.0)); // weit weg
    }

    #[test]
    fn is_fillable() {
        assert!(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0
        }
        .is_fillable());
        assert!(Geo::Ellipse {
            cx: 0.0,
            cy: 0.0,
            rx: 1.0,
            ry: 1.0
        }
        .is_fillable());
        assert!(!Geo::Polyline {
            pts: vec![(0.0, 0.0), (1.0, 1.0)],
            closed: false
        }
        .is_fillable());
        assert!(Geo::Polyline {
            pts: vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)],
            closed: true
        }
        .is_fillable());
    }

    #[test]
    fn translate_verschiebt_alle_typen() {
        let mut r = Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        r.translate(5.0, -3.0);
        assert_eq!(r.bbox().x, 5.0);
        assert_eq!(r.bbox().y, -3.0);

        let mut p = Geo::Polyline {
            pts: vec![(0.0, 0.0), (10.0, 10.0)],
            closed: false,
        };
        p.translate(1.0, 2.0);
        if let Geo::Polyline { pts, .. } = &p {
            assert_eq!(pts[0], (1.0, 2.0));
            assert_eq!(pts[1], (11.0, 12.0));
        }
    }

    #[test]
    fn set_bbox_skaliert_und_erzwingt_mindestgroesse() {
        let mut r = Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        r.set_bbox(0.0, 0.0, 0.0, 0.0);
        let b = r.bbox();
        assert_eq!(b.w, MIN_SIZE);
        assert_eq!(b.h, MIN_SIZE);

        let mut p = Geo::Polyline {
            pts: vec![(0.0, 0.0), (10.0, 10.0)],
            closed: false,
        };
        p.set_bbox(5.0, 5.0, 20.0, 20.0);
        if let Geo::Polyline { pts, .. } = &p {
            assert_eq!(pts[0], (5.0, 5.0));
            assert_eq!(pts[1], (25.0, 25.0));
        }
    }

    #[test]
    fn mirror_rect_vertikal_behaelt_groesse() {
        let mut r = Geo::Rect {
            x: 10.0,
            y: 5.0,
            w: 20.0,
            h: 8.0,
        };
        // Achse x=50: rechte Kante (30) → 70, also neue linke Kante 70.
        r.mirror(Axis::Vertical, 50.0);
        let b = r.bbox();
        assert_eq!((b.x, b.y, b.w, b.h), (70.0, 5.0, 20.0, 8.0));
    }

    #[test]
    fn mirror_rect_horizontal() {
        let mut r = Geo::Rect {
            x: 10.0,
            y: 5.0,
            w: 20.0,
            h: 8.0,
        };
        // Achse y=0: untere Kante (13) → -13, neue obere Kante -13.
        r.mirror(Axis::Horizontal, 0.0);
        let b = r.bbox();
        assert_eq!((b.x, b.y, b.w, b.h), (10.0, -13.0, 20.0, 8.0));
    }

    #[test]
    fn mirror_ellipse_spiegelt_mittelpunkt() {
        let mut e = Geo::Ellipse {
            cx: 20.0,
            cy: 30.0,
            rx: 5.0,
            ry: 3.0,
        };
        e.mirror(Axis::Vertical, 0.0);
        if let Geo::Ellipse { cx, cy, rx, ry } = e {
            assert_eq!((cx, cy, rx, ry), (-20.0, 30.0, 5.0, 3.0));
        } else {
            panic!("kein Ellipse");
        }
    }

    #[test]
    fn mirror_polyline_spiegelt_punkte() {
        let mut p = Geo::Polyline {
            pts: vec![(0.0, 0.0), (10.0, 4.0)],
            closed: false,
        };
        p.mirror(Axis::Vertical, 5.0);
        if let Geo::Polyline { pts, .. } = &p {
            assert_eq!(pts[0], (10.0, 0.0));
            assert_eq!(pts[1], (0.0, 4.0));
        }
    }

    #[test]
    fn mirror_zweimal_ist_identitaet() {
        let mut r = Geo::Rect {
            x: 3.0,
            y: 7.0,
            w: 11.0,
            h: 13.0,
        };
        let orig = r.clone();
        r.mirror(Axis::Vertical, 42.0);
        r.mirror(Axis::Vertical, 42.0);
        assert_eq!(r, orig);
    }

    #[test]
    fn union_all_umschliesst() {
        let a = BBox::new(10.0, 10.0, 20.0, 20.0);
        let b = BBox::new(50.0, 5.0, 10.0, 40.0);
        let u = BBox::union_all([a, b]).unwrap();
        assert_eq!((u.x, u.y, u.w, u.h), (10.0, 5.0, 50.0, 40.0));
    }

    #[test]
    fn rotate_point_90_grad() {
        let (x, y) = rotate_point(10.0, 0.0, 0.0, 0.0, 90.0);
        assert!((x - 0.0).abs() < 1e-9);
        assert!((y - 10.0).abs() < 1e-9);
    }
}
