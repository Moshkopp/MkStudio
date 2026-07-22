//! Geometrie-Werkzeuge: Boolean (Vereinigen/Schneiden/Abziehen), Offset
//! (parallele Kontur) und Fillet (Eckenverrundung). Reine mm-Geometrie,
//! UI-frei, testbar.
//!
//! Nach v3-Analyse neu aufgesetzt (CLAUDE.md Regel 6). Bewusste Abweichung:
//! v3 rollte Greiner-Hormann selbst (377 Zeilen Schnittpunkt-Topologie, ein
//! bekanntes Kantenfall-Minenfeld) — wir nutzen die erprobte `i_overlay`-
//! Bibliothek. Beim Offset traf schon v3 dieselbe Wahl (`cavalier_contours`).
//! Fillet ist überschaubare Trigonometrie und selbst implementiert.
//!
//! Löcher: Ergebnisse mit Innenkonturen kommen als **separate geschlossene
//! Polylinien** zurück — die Even-Odd-Scanline (scanline.rs) spart sie beim
//! Füllen automatisch aus; es braucht kein Loch-Konzept im Datenmodell.

use crate::geometry::Pt;
use cavalier_contours::polyline::{PlineSource, PlineSourceMut, Polyline};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;

/// Boolesche Operation zweier Polygon-Mengen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    /// Vereinigen (A ∪ B).
    Union,
    /// Schneiden (A ∩ B).
    Intersect,
    /// Abziehen (A − B).
    Difference,
}

/// Führt die boolesche Operation aus. `subject`/`clip` sind geschlossene
/// Konturen in mm (Weltkoordinaten, Rotation bereits angewandt). Ergebnis:
/// geschlossene Konturen — Außenränder UND Löcher als eigene Polylinien.
pub fn boolean(subject: &[Vec<Pt>], clip: &[Vec<Pt>], op: BoolOp) -> Vec<Vec<Pt>> {
    let to_lib = |contours: &[Vec<Pt>]| -> Vec<Vec<[f64; 2]>> {
        contours
            .iter()
            .map(|c| c.iter().map(|&(x, y)| [x, y]).collect())
            .collect()
    };
    let subj = to_lib(subject);
    let clip = to_lib(clip);
    let rule = match op {
        BoolOp::Union => OverlayRule::Union,
        BoolOp::Intersect => OverlayRule::Intersect,
        BoolOp::Difference => OverlayRule::Difference,
    };
    // Ergebnis: Shapes → Konturen (erste = Außenrand, weitere = Löcher).
    // Wir flachen zu einer Konturliste ab (Even-Odd übernimmt die Löcher).
    let shapes = subj.overlay(&clip, rule, FillRule::EvenOdd);
    let mut out = Vec::new();
    for shape in shapes {
        for contour in shape {
            if contour.len() >= 3 {
                out.push(contour.into_iter().map(|p| (p[0], p[1])).collect());
            }
        }
    }
    out
}

/// Parallele Kontur im Abstand `dist` (mm): positiv = nach außen, negativ =
/// nach innen (bei geschlossenen Konturen). Kann mehrere Konturen liefern
/// (Selbstüberschneidungen werden aufgelöst) oder keine (Kontur kollabiert).
/// Geschlossene konvexe Linienkonturen erhalten harte Miter-Ecken. Offene und
/// konkave Konturen laufen über `cavalier_contours`, dessen End-/Selbstschnitt-
/// behandlung dafür robuster ist; Bögen werden dort zu Linien abgeflacht.
pub fn offset(points: &[Pt], closed: bool, dist: f64) -> Vec<Vec<Pt>> {
    if points.len() < 2 || dist == 0.0 {
        return Vec::new();
    }
    if closed && points.len() >= 3 && is_convex_contour(points) {
        return offset_closed_miter(points, dist).into_iter().collect();
    }
    let mut pl: Polyline<f64> = if closed {
        Polyline::new_closed()
    } else {
        Polyline::new()
    };
    for &(x, y) in points {
        pl.add(x, y, 0.0);
    }
    // cavalier_contours: positives Offset = in Flächenrichtung. Damit
    // „positiv = außen" unabhängig vom Umlaufsinn gilt, normieren wir auf
    // gegen den Uhrzeigersinn (positive Fläche).
    if closed && pl.area() < 0.0 {
        pl.invert_direction_mut();
    }
    let mut out = Vec::new();
    for res in pl.parallel_offset(-dist) {
        // Bögen (bulge ≠ 0) zu Liniensegmenten abflachen (0,01 mm Toleranz).
        let flat = res.arcs_to_approx_lines(0.01).unwrap_or(res);
        let pts: Vec<Pt> = (0..flat.vertex_count())
            .map(|i| {
                let v = flat.at(i);
                (v.x, v.y)
            })
            .collect();
        if pts.len() >= 2 {
            out.push(pts);
        }
    }
    out
}

fn is_convex_contour(points: &[Pt]) -> bool {
    let pts = if points.len() > 3 && points.first() == points.last() {
        &points[..points.len() - 1]
    } else {
        points
    };
    let mut sign = 0.0_f64;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        let c = pts[(i + 2) % pts.len()];
        let cross = (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0);
        if cross.abs() < 1e-9 {
            continue;
        }
        if sign != 0.0 && cross.signum() != sign {
            return false;
        }
        sign = cross.signum();
    }
    sign != 0.0
}

/// Offset einer geschlossenen, geraden Kontur mit scharfen Ecken. Die
/// Schnittpunkte benachbarter verschobener Kanten sind die Miter-Knoten.
fn offset_closed_miter(points: &[Pt], dist: f64) -> Option<Vec<Pt>> {
    let mut pts = points.to_vec();
    if pts.len() > 3 && pts.first() == pts.last() {
        pts.pop();
    }
    let signed_area = pts
        .iter()
        .zip(pts.iter().cycle().skip(1))
        .take(pts.len())
        .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
        .sum::<f64>()
        * 0.5;
    if signed_area.abs() < 1e-9 {
        return None;
    }
    // Bei CCW liegt außen rechts der Kante, bei CW links.
    let side = if signed_area > 0.0 { 1.0 } else { -1.0 };
    let shifted = |a: Pt, b: Pt| -> Option<(Pt, Pt)> {
        let dx = b.0 - a.0;
        let dy = b.1 - a.1;
        let len = dx.hypot(dy);
        if len < 1e-9 {
            return None;
        }
        let n = (side * dy / len * dist, -side * dx / len * dist);
        Some(((a.0 + n.0, a.1 + n.1), (b.0 + n.0, b.1 + n.1)))
    };
    let line_intersection = |a: Pt, b: Pt, c: Pt, d: Pt| -> Option<Pt> {
        let ab = (b.0 - a.0, b.1 - a.1);
        let cd = (d.0 - c.0, d.1 - c.1);
        let det = ab.0 * cd.1 - ab.1 * cd.0;
        if det.abs() < 1e-9 {
            return None;
        }
        let ac = (c.0 - a.0, c.1 - a.1);
        let t = (ac.0 * cd.1 - ac.1 * cd.0) / det;
        Some((a.0 + t * ab.0, a.1 + t * ab.1))
    };

    let n = pts.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = shifted(pts[(i + n - 1) % n], pts[i])?;
        let next = shifted(pts[i], pts[(i + 1) % n])?;
        // Kollineare Nachbarkanten brauchen keinen unendlichen Schnittpunkt.
        out.push(line_intersection(prev.0, prev.1, next.0, next.1).unwrap_or(next.0));
    }
    let out_area = out
        .iter()
        .zip(out.iter().cycle().skip(1))
        .take(out.len())
        .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
        .sum::<f64>()
        * 0.5;
    // Ein zu großer Innenoffset klappt die Kontur um; fachlich ist sie dann
    // kollabiert und darf nicht als scheinbar neue Fläche zurückkehren.
    let edges_keep_direction = (0..n).all(|i| {
        let original = (pts[(i + 1) % n].0 - pts[i].0, pts[(i + 1) % n].1 - pts[i].1);
        let shifted = (out[(i + 1) % n].0 - out[i].0, out[(i + 1) % n].1 - out[i].1);
        original.0 * shifted.0 + original.1 * shifted.1 > 1e-9
    });
    if out_area.abs() < 1e-9 || out_area.signum() != signed_area.signum() || !edges_keep_direction {
        return None;
    }
    Some(out)
}

/// Verrundet die Ecken einer Polylinie mit Radius `r` (mm): jede Ecke wird
/// durch einen Kreisbogen (als Segmentzug) ersetzt. Ecken, deren Schenkel für
/// den Radius zu kurz sind, bleiben spitz. Offene Konturen behalten Anfangs-
/// und Endpunkt.
pub fn fillet(points: &[Pt], closed: bool, r: f64) -> Vec<Pt> {
    fillet_corners(points, closed, r, None)
}

/// Wie [`fillet`], aber optional nur an den Ecken mit den angegebenen
/// **Punkt-Indizes** (Referenz-UX: Ecken einzeln anklicken). `None` = alle.
pub fn fillet_corners(points: &[Pt], closed: bool, r: f64, only: Option<&[usize]>) -> Vec<Pt> {
    let radii: Vec<(usize, f64)> = only
        .map(|indices| indices.iter().map(|&index| (index, r)).collect())
        .unwrap_or_else(|| {
            let corner_count = if closed {
                points.len()
            } else {
                points.len().saturating_sub(2)
            };
            (0..corner_count)
                .map(|k| (if closed { k } else { k + 1 }, r))
                .collect()
        });
    fillet_corner_radii(points, closed, &radii).0
}

/// Verrundet einzelne Ecken mit jeweils eigenem Radius. Der zweite Rückgabewert
/// zählt die tatsächlich erzeugten Rundungen; ungeeignete Ecken bleiben spitz.
pub fn fillet_corner_radii(
    points: &[Pt],
    closed: bool,
    radii: &[(usize, f64)],
) -> (Vec<Pt>, usize) {
    let n = points.len();
    if n < 3 || radii.is_empty() {
        return (points.to_vec(), 0);
    }
    /// Segmente je Viertelkreis — fein genug für Laser-Konturen.
    const ARC_SEGS: usize = 8;

    let mut out: Vec<Pt> = Vec::new();
    let mut accepted = 0;
    let corner_count = if closed { n } else { n - 2 };
    if !closed {
        out.push(points[0]);
    }
    for k in 0..corner_count {
        // Ecke p mit Nachbarn a (davor) und b (danach).
        let (ia, ip, ib) = if closed {
            ((k + n - 1) % n, k, (k + 1) % n)
        } else {
            (k, k + 1, k + 2)
        };
        let (a, p, b) = (points[ia], points[ip], points[ib]);
        let radius = radii
            .iter()
            .find_map(|&(index, radius)| (index == ip && radius > 0.0).then_some(radius));
        match radius.and_then(|radius| corner_arc(a, p, b, radius, ARC_SEGS)) {
            Some(arc) => {
                accepted += 1;
                out.extend(arc);
            }
            None => out.push(p), // nicht gewählt / zu kurze Schenkel: Ecke bleibt
        }
    }
    if !closed {
        out.push(points[n - 1]);
    }
    (out, accepted)
}

/// Bogenpunkte für die Ecke `p` (Schenkel zu `a` und `b`) mit Radius `r`.
/// `None`, wenn die Schenkel zu kurz sind oder die Ecke (nahezu) gerade ist.
fn corner_arc(a: Pt, p: Pt, b: Pt, r: f64, segs: usize) -> Option<Vec<Pt>> {
    let (v1, l1) = unit(p, a)?;
    let (v2, l2) = unit(p, b)?;
    // Halber Eckwinkel über das Skalarprodukt.
    let cos_full = (v1.0 * v2.0 + v1.1 * v2.1).clamp(-1.0, 1.0);
    let full = cos_full.acos();
    if full < 1e-3 || (std::f64::consts::PI - full) < 1e-3 {
        return None; // spitz zusammengefaltet oder gerade — nichts zu runden
    }
    let half = full / 2.0;
    // Abstand der Tangentenpunkte von der Ecke.
    let t = r / (half.tan());
    if t > l1 * 0.5 || t > l2 * 0.5 {
        return None; // Schenkel zu kurz — Ecke bleibt spitz
    }
    let t1 = (p.0 + v1.0 * t, p.1 + v1.1 * t);
    let t2 = (p.0 + v2.0 * t, p.1 + v2.1 * t);
    // Bogenmittelpunkt: von der Ecke entlang der Winkelhalbierenden.
    let bis = ((v1.0 + v2.0) / 2.0, (v1.1 + v2.1) / 2.0);
    let bl = (bis.0 * bis.0 + bis.1 * bis.1).sqrt();
    if bl < 1e-12 {
        return None;
    }
    let d = r / half.sin();
    let c = (p.0 + bis.0 / bl * d, p.1 + bis.1 / bl * d);
    // Winkel von c zu den Tangentenpunkten; kurzen Bogen interpolieren.
    let a1 = (t1.1 - c.1).atan2(t1.0 - c.0);
    let a2 = (t2.1 - c.1).atan2(t2.0 - c.0);
    let mut sweep = a2 - a1;
    while sweep > std::f64::consts::PI {
        sweep -= std::f64::consts::TAU;
    }
    while sweep < -std::f64::consts::PI {
        sweep += std::f64::consts::TAU;
    }
    let steps = ((segs as f64) * (sweep.abs() / (std::f64::consts::PI / 2.0))).ceil() as usize;
    let steps = steps.max(2);
    let mut arc = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let ang = a1 + sweep * (i as f64 / steps as f64);
        arc.push((c.0 + r * ang.cos(), c.1 + r * ang.sin()));
    }
    Some(arc)
}

/// Einheitsvektor von `from` nach `to` + Länge; `None` bei (nahezu) Null.
pub(super) fn unit(from: Pt, to: Pt) -> Option<((f64, f64), f64)> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let l = (dx * dx + dy * dy).sqrt();
    if l < 1e-9 {
        return None;
    }
    Some(((dx / l, dy / l), l))
}

/// Haltesteg nach v3-Modell für EINE Kontur — Wrapper um [`bridge_contours`].
pub fn bridge_line(
    points: &[Pt],
    closed: bool,
    p0: Pt,
    p1: Pt,
    width: f64,
) -> Option<Vec<(Vec<Pt>, bool)>> {
    bridge_contours(&[(points.to_vec(), closed)], p0, p1, width)
}

/// Ob die Strecke `p0`–`p1` die Kontur kreuzt.
pub(super) fn line_crosses(points: &[Pt], closed: bool, p0: Pt, p1: Pt) -> bool {
    let n = points.len();
    if n < 2 {
        return false;
    }
    let edges = if closed { n } else { n - 1 };
    (0..edges).any(|i| seg_seg_t(points[i], points[(i + 1) % n], p0, p1).is_some())
}

/// Haltesteg nach v3-Modell über einen **Even-Odd-Verbund** von Konturen
/// (z. B. „O" = Außenrand + Loch): der Nutzer zieht eine **Steg-Linie**
/// `p0`→`p1` der Breite `width` (mm) über die Konturen. Wo die Linie kreuzt,
/// werden die Konturen aufgeschnitten und die Teilstücke **innerhalb des
/// Steg-Bandes** (Abstand ≤ width/2 zur Linie) entfernt — dort bleibt beim
/// Schneiden Material stehen.
///
/// Die Grenzpunkte werden je Steg-Seite entlang der Linie sortiert und
/// **paarweise** verbunden: Nach Even-Odd liegt zwischen Punkt 2k und 2k+1
/// immer Material — die Querstücke schließen also genau die Materialschnitte
/// (beim Rechteck quer durchs Innere, beim „O" Außenrand↔Loch über den Ring)
/// und laufen nie quer durch Löcher. Der Steg ist der uncut Materialstreifen
/// zwischen den beiden Querstück-Wänden (ThorBurn-Verhalten, vom Nutzer an
/// Rechteck UND Schrift bestätigt). Zusammengehörige Konturen müssen dafür
/// GEMEINSAM in einem Aufruf landen; einzeln geschnitten würden die
/// Querstücke Löcher überspannen.
///
/// Offene Polylinien bekommen nur die Lücke (ein Grenzpunkt je Seite —
/// keine Paare, keine Querstücke).
///
/// Ergebnis: die neu verketteten Teilkonturen, oder `None`, wenn die Linie
/// keine der Konturen kreuzt.
pub fn bridge_contours(
    contours: &[(Vec<Pt>, bool)],
    p0: Pt,
    p1: Pt,
    width: f64,
) -> Option<Vec<(Vec<Pt>, bool)>> {
    if width <= 0.0 {
        return None;
    }
    let r = width / 2.0;

    // 1. Schnittpunkte der Steg-Linie mit ALLEN Konturen — zugleich die
    //    Zentren der Band-Kreise für die Unterteilung (v3-Logik).
    let mut crossings: Vec<Pt> = Vec::new();
    for (points, closed) in contours {
        let n = points.len();
        if n < 2 {
            continue;
        }
        let edges = if *closed { n } else { n - 1 };
        for i in 0..edges {
            if let Some(pt) = seg_seg_point(points[i], points[(i + 1) % n], p0, p1) {
                crossings.push(pt);
            }
        }
    }
    if crossings.is_empty() {
        return None;
    }

    // Segment „drin" = Mittelpunkt im Band eines Schnittpunkts.
    let inside = |seg: &(Pt, Pt)| -> bool {
        let mid = ((seg.0 .0 + seg.1 .0) / 2.0, (seg.0 .1 + seg.1 .1) / 2.0);
        crossings
            .iter()
            .any(|c| (mid.0 - c.0).hypot(mid.1 - c.1) <= r + 1e-5)
    };
    let add = |path: &mut Vec<Pt>, pt: Pt| {
        if path
            .last()
            .is_none_or(|l: &Pt| (l.0 - pt.0).hypot(l.1 - pt.1) > 1e-6)
        {
            path.push(pt);
        }
    };

    let mut paths: Vec<Vec<Pt>> = Vec::new();
    let mut boundary: Vec<Pt> = Vec::new();
    for (points, closed) in contours {
        let n = points.len();
        if n < 2 {
            continue;
        }
        let edges = if *closed { n } else { n - 1 };

        // 2. Kontur in Mikro-Segmente zerlegen, die entweder ganz „drin"
        //    (Band) oder ganz „draußen" sind.
        let mut sub: Vec<(Pt, Pt)> = Vec::new();
        for i in 0..edges {
            let a = points[i];
            let b = points[(i + 1) % n];
            // Teilung: Schnitt mit der Linie + Ein/Austritt der Bänder.
            let mut ts: Vec<f64> = vec![0.0, 1.0];
            if let Some(t) = seg_seg_t(a, b, p0, p1) {
                ts.push(t);
            }
            for &c in &crossings {
                ts.extend(seg_circle_ts(a, b, c, r));
            }
            ts.retain(|&t| (0.0..=1.0).contains(&t));
            ts.sort_by(|x, y| x.partial_cmp(y).unwrap());
            ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            let lerp = |t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            for w in ts.windows(2) {
                sub.push((lerp(w[0]), lerp(w[1])));
            }
        }
        let total = sub.len();
        if total == 0 {
            continue;
        }

        // 3. Zusammenhängende „draußen"-Ketten sammeln; bei geschlossenen
        //    Konturen an einer drin→draußen-Kante beginnen, damit die Kette
        //    nicht mitten im Band startet.
        let mut start = 0;
        if *closed {
            for i in 0..total {
                if !inside(&sub[i]) && inside(&sub[(i + total - 1) % total]) {
                    start = i;
                    break;
                }
            }
        }
        let mut cur: Vec<Pt> = Vec::new();
        for step in 0..total {
            let idx = if *closed {
                (start + step) % total
            } else {
                step
            };
            let seg = &sub[idx];
            if inside(seg) {
                if cur.len() >= 2 {
                    paths.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
            } else {
                add(&mut cur, seg.0);
                add(&mut cur, seg.1);
            }
        }
        if cur.len() >= 2 {
            paths.push(cur);
        }

        // 4. Grenzpunkte (drin↔draußen-Wechsel) in den gemeinsamen Pool.
        for i in 0..total {
            let prev = &sub[(i + total - 1) % total];
            if (*closed || i > 0) && inside(&sub[i]) != inside(prev) {
                boundary.push(sub[i].0);
            }
        }
    }

    // 5. Die Grenzpunkte ALLER Konturen je Steg-Seite entlang der Linie
    // sortieren und paarweise verbinden (Even-Odd, siehe Doku oben). Diese
    // Querstücke sind der in der ersten Portierung fehlende Teil der
    // ThorBurn-Logik.
    let (dir, _) = unit(p0, p1)?;
    let normal = (-dir.1, dir.0);
    let mut left: Vec<(f64, Pt)> = Vec::new();
    let mut right: Vec<(f64, Pt)> = Vec::new();
    for pt in boundary {
        let rel = (pt.0 - p0.0, pt.1 - p0.1);
        let along = rel.0 * dir.0 + rel.1 * dir.1;
        let side = rel.0 * normal.0 + rel.1 * normal.1;
        if side < 0.0 {
            left.push((along, pt));
        } else {
            right.push((along, pt));
        }
    }
    for side in [&mut left, &mut right] {
        side.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for pair in side.chunks_exact(2) {
            paths.push(vec![pair[0].1, pair[1].1]);
        }
    }

    let chained = chain_paths(paths);
    let result = chained
        .into_iter()
        .filter(|p| p.len() >= 2)
        .map(|mut p| {
            let is_closed =
                p.len() >= 3 && (p[0].0 - p[p.len() - 1].0).hypot(p[0].1 - p[p.len() - 1].1) < 1e-5;
            if is_closed {
                p.pop();
            }
            (p, is_closed)
        })
        .collect::<Vec<_>>();
    (!result.is_empty()).then_some(result)
}

fn chain_paths(mut paths: Vec<Vec<Pt>>) -> Vec<Vec<Pt>> {
    let close = |a: Pt, b: Pt| (a.0 - b.0).hypot(a.1 - b.1) < 1e-5;
    loop {
        let mut merged = false;
        'outer: for i in 0..paths.len() {
            if paths[i].len() < 2 || close(paths[i][0], *paths[i].last().unwrap()) {
                continue;
            }
            for j in (i + 1)..paths.len() {
                if paths[j].is_empty() {
                    continue;
                }
                let (ps, pe) = (paths[i][0], *paths[i].last().unwrap());
                let (qs, qe) = (paths[j][0], *paths[j].last().unwrap());
                let next = if close(pe, qs) {
                    let mut p = paths[i].clone();
                    p.extend_from_slice(&paths[j][1..]);
                    Some(p)
                } else if close(pe, qe) {
                    let mut q = paths[j].clone();
                    q.reverse();
                    let mut p = paths[i].clone();
                    p.extend_from_slice(&q[1..]);
                    Some(p)
                } else if close(ps, qs) {
                    let mut p = paths[i].clone();
                    p.reverse();
                    p.extend_from_slice(&paths[j][1..]);
                    Some(p)
                } else if close(ps, qe) {
                    let mut p = paths[j].clone();
                    p.extend_from_slice(&paths[i][1..]);
                    Some(p)
                } else {
                    None
                };
                if let Some(next) = next {
                    paths[i] = next;
                    paths.remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            break;
        }
    }
    paths
}

/// Schnittpunkt zweier Strecken (falls vorhanden).
fn seg_seg_point(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<Pt> {
    let t = seg_seg_t(a, b, c, d)?;
    Some((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t))
}

/// Parameter t auf a–b des Schnitts mit c–d (beide Strecken), falls vorhanden.
fn seg_seg_t(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<f64> {
    let den = (b.0 - a.0) * (d.1 - c.1) - (b.1 - a.1) * (d.0 - c.0);
    if den.abs() < 1e-12 {
        return None;
    }
    let t = ((c.0 - a.0) * (d.1 - c.1) - (c.1 - a.1) * (d.0 - c.0)) / den;
    let u = ((c.0 - a.0) * (b.1 - a.1) - (c.1 - a.1) * (b.0 - a.0)) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// Parameter t auf a–b, an denen die Strecke den Kreis (Mittelpunkt `c`,
/// Radius `r`) schneidet.
fn seg_circle_ts(a: Pt, b: Pt, c: Pt, r: f64) -> Vec<f64> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return Vec::new();
    }
    let (vx, vy) = (a.0 - c.0, a.1 - c.1);
    let bq = 2.0 * (vx * dx + vy * dy);
    let cq = vx * vx + vy * vy - r * r;
    let disc = bq * bq - 4.0 * len2 * cq;
    if disc < 0.0 {
        return Vec::new();
    }
    let sq = disc.sqrt();
    [(-bq - sq) / (2.0 * len2), (-bq + sq) / (2.0 * len2)]
        .into_iter()
        .filter(|&t| (0.0..=1.0).contains(&t))
        .collect()
}

mod apply;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Geo;
    use crate::state::AppState;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Vec<Pt> {
        vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
    }

    /// Fläche einer geschlossenen Kontur (Shoelace, Betrag).
    fn area(pts: &[Pt]) -> f64 {
        let n = pts.len();
        let mut a = 0.0;
        for i in 0..n {
            let (x1, y1) = pts[i];
            let (x2, y2) = pts[(i + 1) % n];
            a += x1 * y2 - x2 * y1;
        }
        (a / 2.0).abs()
    }

    #[test]
    fn union_zweier_ueberlappender_rechtecke() {
        // 10×10 und um (5,0) versetzt: Vereinigung = 10*10 + 10*10 − 5*10 = 150.
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 0.0, 10.0, 10.0);
        let out = boolean(&[a], &[b], BoolOp::Union);
        assert_eq!(out.len(), 1, "eine zusammenhängende Kontur");
        assert!((area(&out[0]) - 150.0).abs() < 1e-6);
    }

    #[test]
    fn intersect_liefert_ueberlappung() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 0.0, 10.0, 10.0);
        let out = boolean(&[a], &[b], BoolOp::Intersect);
        assert_eq!(out.len(), 1);
        assert!((area(&out[0]) - 50.0).abs() < 1e-6, "Schnitt = 5×10");
    }

    #[test]
    fn difference_zieht_ab() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 0.0, 10.0, 10.0);
        let out = boolean(&[a], &[b], BoolOp::Difference);
        assert_eq!(out.len(), 1);
        assert!((area(&out[0]) - 50.0).abs() < 1e-6, "Rest = 5×10");
    }

    #[test]
    fn difference_mit_loch_liefert_zwei_konturen() {
        // Kleines Rechteck mittig aus großem ausstanzen → Außenrand + Loch.
        let a = rect(0.0, 0.0, 20.0, 20.0);
        let b = rect(5.0, 5.0, 10.0, 10.0);
        let out = boolean(&[a], &[b], BoolOp::Difference);
        assert_eq!(out.len(), 2, "Außenrand + Lochkontur");
        let sum: f64 = out.iter().map(|c| area(c)).sum();
        // Flächen beider Konturen: 400 (außen) + 100 (Loch) = 500.
        assert!((sum - 500.0).abs() < 1e-6);
    }

    #[test]
    fn getrennte_rechtecke_union_bleiben_zwei() {
        let a = rect(0.0, 0.0, 5.0, 5.0);
        let b = rect(20.0, 0.0, 5.0, 5.0);
        let out = boolean(&[a], &[b], BoolOp::Union);
        assert_eq!(out.len(), 2, "disjunkt bleibt zweiteilig");
    }

    #[test]
    fn offset_nach_aussen_vergroessert() {
        let sq = rect(0.0, 0.0, 10.0, 10.0);
        let out = offset(&sq, true, 2.0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 4, "harte Rechteck-Ecken bleiben hart");
        assert!(
            (area(&out[0]) - 196.0).abs() < 1e-6,
            "war {}",
            area(&out[0])
        );
        let xs: Vec<f64> = out[0].iter().map(|p| p.0).collect();
        let ys: Vec<f64> = out[0].iter().map(|p| p.1).collect();
        assert_eq!(xs.iter().copied().fold(f64::INFINITY, f64::min), -2.0);
        assert_eq!(xs.iter().copied().fold(f64::NEG_INFINITY, f64::max), 12.0);
        assert_eq!(ys.iter().copied().fold(f64::INFINITY, f64::min), -2.0);
        assert_eq!(ys.iter().copied().fold(f64::NEG_INFINITY, f64::max), 12.0);
    }

    #[test]
    fn offset_nach_innen_verkleinert() {
        let sq = rect(0.0, 0.0, 10.0, 10.0);
        let out = offset(&sq, true, -2.0);
        assert_eq!(out.len(), 1);
        assert!(
            (area(&out[0]) - 36.0).abs() < 0.5,
            "innen 6×6, war {}",
            area(&out[0])
        );
    }

    #[test]
    fn offset_kollabiert_bei_zu_grossem_innenabstand() {
        let sq = rect(0.0, 0.0, 10.0, 10.0);
        let out = offset(&sq, true, -6.0);
        assert!(out.is_empty(), "6mm nach innen bei 10mm-Quadrat = weg");
    }

    #[test]
    fn offset_unabhaengig_vom_umlaufsinn() {
        // Gleiche Kontur, andersherum aufgezählt → gleiches Außen-Offset.
        let cw: Vec<Pt> = rect(0.0, 0.0, 10.0, 10.0).into_iter().rev().collect();
        let out = offset(&cw, true, 2.0);
        assert_eq!(out.len(), 1);
        assert!(area(&out[0]) > 180.0, "positiv muss auch bei CW außen sein");
    }

    #[test]
    fn fillet_rundet_quadratecken() {
        let sq = rect(0.0, 0.0, 10.0, 10.0);
        let out = fillet(&sq, true, 2.0);
        // Mehr Punkte als vorher (Bögen) …
        assert!(out.len() > 4);
        // … und die Fläche schrumpft um die Eckenabschnitte:
        // 100 − 4·(4 − π·4/4) ≈ 100 − 3,43 ≈ 96,57.
        let a = area(&out);
        assert!((a - 96.566).abs() < 0.2, "Fläche nach Fillet war {a}");
        // Kein Punkt liegt mehr auf der spitzen Ecke (0,0). Der nächste
        // Bogenpunkt hat Abstand r·(√2−1)·√2 ≈ 0,828 zur alten Ecke.
        assert!(out.iter().all(|&(x, y)| (x - 0.0).hypot(y - 0.0) > 0.8));
    }

    #[test]
    fn fillet_ecken_behalten_individuelle_radien() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        let (out, accepted) = fillet_corner_radii(&sq, true, &[(0, 2.0), (1, 4.0)]);
        assert_eq!(accepted, 2);
        let near = |x: f64, y: f64| {
            out.iter()
                .any(|point| (point.0 - x).abs() < 1e-6 && (point.1 - y).abs() < 1e-6)
        };
        assert!(near(2.0, 0.0), "erste Ecke verwendet 2 mm");
        assert!(near(16.0, 0.0), "zweite Ecke verwendet 4 mm");
        assert!(near(20.0, 4.0), "zweite Tangente verwendet 4 mm");
    }

    #[test]
    fn bridge_line_verbindet_stegrander_zu_geschlossenen_haelften() {
        // 20mm-Quadrat, Steg-Linie waagerecht mitten durch (y=10), Breite 4.
        // ThorBurn-Verhalten (Nutzer-verifiziert): die Querstücke schließen
        // beide Hälften; der Steg ist der Materialstreifen dazwischen.
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        let pieces = bridge_line(&sq, true, (-5.0, 10.0), (25.0, 10.0), 4.0).unwrap();
        assert_eq!(pieces.len(), 2, "obere und untere Hälfte");
        assert!(
            pieces.iter().all(|(_, closed)| *closed),
            "beide Hälften sind über die Querstücke geschlossen"
        );
        // Kein Punkt liegt im Steg-Band (|y-10| < 2 an den linken/rechten Kanten
        // bei x=0 und x=20 — dort schneidet die Linie).
        for (pts, _) in &pieces {
            for &(x, y) in pts {
                let on_side = x <= 0.01 || x >= 19.99;
                if on_side {
                    assert!((y - 10.0).abs() >= 2.0 - 1e-6, "Lücke im Band bei y={y}");
                }
            }
        }
    }

    #[test]
    fn bridge_contours_ring_verbindet_material_statt_loch() {
        // Ring aus Außenrand (0..30) + Innenloch (10..20), Steg quer durch
        // die Mitte. Erwartung: obere + untere Ringhälfte, und die Querstücke
        // schließen NUR die Materialschnitte (Ringbreite 10 mm) — kein Steg
        // überspannt das Loch (der Fehler der Einzelkontur-Fassung).
        let outer = rect(0.0, 0.0, 30.0, 30.0);
        let inner = rect(10.0, 10.0, 10.0, 10.0);
        let pieces = bridge_contours(
            &[(outer, true), (inner, true)],
            (-5.0, 15.0),
            (35.0, 15.0),
            4.0,
        )
        .unwrap();
        assert_eq!(pieces.len(), 2, "oberer und unterer Ring-Teil");
        assert!(pieces.iter().all(|(_, closed)| *closed));
        // Querstücke liegen auf den Bandkanten y=13/y=17: keines länger als
        // die Ringbreite.
        for (pts, _) in &pieces {
            let n = pts.len();
            for i in 0..n {
                let a = pts[i];
                let b = pts[(i + 1) % n];
                let on_band = (a.1 - b.1).abs() < 1e-6
                    && ((a.1 - 13.0).abs() < 1e-6 || (a.1 - 17.0).abs() < 1e-6);
                if on_band {
                    assert!(
                        (a.0 - b.0).abs() <= 10.0 + 1e-6,
                        "Quersteg {a:?}→{b:?} überspannt das Loch"
                    );
                }
            }
        }
    }

    #[test]
    fn bridge_stroke_ring_wird_als_verbund_geschnitten() {
        // Zwei Shapes (Außen + Loch) auf demselben Layer ohne Gruppe → der
        // Steg muss sie als Verbund behandeln: zwei Teilstücke, ein Undo.
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 30.0,
            h: 30.0,
        });
        s.add_shape(Geo::Rect {
            x: 10.0,
            y: 10.0,
            w: 10.0,
            h: 10.0,
        });
        assert!(s.bridge_stroke((-5.0, 15.0), (35.0, 15.0), 4.0));
        assert_eq!(s.shapes.len(), 2, "Ring → obere + untere Hälfte");
        s.undo();
        assert_eq!(s.shapes.len(), 2, "Undo stellt Außenrand + Loch her");
        assert!(matches!(s.shapes[0].geo, Geo::Rect { .. }));
    }

    #[test]
    fn bridge_line_ohne_kreuzung_ist_none() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        // Linie komplett außerhalb.
        assert!(bridge_line(&sq, true, (30.0, 30.0), (40.0, 40.0), 4.0).is_none());
    }

    #[test]
    fn bridge_line_diagonal_bleibt_zusammenhaengend() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        let pieces = bridge_line(&sq, true, (-5.0, -5.0), (25.0, 25.0), 2.0).unwrap();
        assert_eq!(pieces.len(), 2);
        assert!(pieces.iter().all(|(_, closed)| *closed));
    }

    #[test]
    fn bridge_line_offene_linie_bekommt_luecke() {
        // Offene 2-Punkt-Linie quer zum Steg → zwei Teilstücke mit Lücke.
        let line = vec![(0.0, 0.0), (20.0, 0.0)];
        let pieces = bridge_line(&line, false, (10.0, -5.0), (10.0, 5.0), 4.0).unwrap();
        assert_eq!(pieces.len(), 2, "Linie wird aufgetrennt");
        assert!(pieces.iter().all(|(_, closed)| !*closed));
        // Die Lücke ist band-breit: kein Punkt im Bereich |x-10| < 2.
        for (pts, _) in &pieces {
            for &(x, _) in pts {
                assert!((x - 10.0).abs() >= 2.0 - 1e-6, "Lücke fehlt bei x={x}");
            }
        }
    }

    #[test]
    fn bridge_line_lehnt_nullbreite_ab() {
        let sq = rect(0.0, 0.0, 20.0, 20.0);
        assert!(bridge_line(&sq, true, (-5.0, 10.0), (25.0, 10.0), 0.0).is_none());
    }

    #[test]
    fn bridge_stroke_klick_sucht_naechste_kante() {
        let mut state = AppState::new();
        state.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 20.0,
            h: 20.0,
        });
        assert!(state.bridge_stroke((0.2, 10.0), (0.2, 10.0), 2.0));
        assert!(state.dirty);
    }

    #[test]
    fn fillet_zu_grosser_radius_laesst_ecken_spitz() {
        let sq = rect(0.0, 0.0, 4.0, 4.0);
        let out = fillet(&sq, true, 10.0);
        assert_eq!(out.len(), 4, "Radius passt nicht → unverändert");
    }

    #[test]
    fn fillet_offene_kontur_behaelt_enden() {
        let l = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        let out = fillet(&l, false, 2.0);
        assert_eq!(out[0], (0.0, 0.0));
        assert_eq!(*out.last().unwrap(), (10.0, 10.0));
        assert!(out.len() > 3, "die eine Ecke wurde verrundet");
    }

    // ── AppState-Verdrahtung ────────────────────────────────────────────────

    fn state_two_overlapping() -> AppState {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        let c = s.layers[0].color;
        s.selected.clear();
        s.activate_color(c);
        s.add_shape(Geo::Rect {
            x: 5.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        s.selected = vec![0, 1];
        s
    }

    #[test]
    fn boolean_selected_ersetzt_shapes_und_undo_stellt_wieder_her() {
        let mut s = state_two_overlapping();
        assert!(s.can_boolean());
        s.boolean_selected(BoolOp::Union);
        assert_eq!(s.shapes.len(), 1, "zwei Rechtecke → eine Kontur");
        assert!(matches!(
            s.shapes[0].geo,
            Geo::Polyline { closed: true, .. }
        ));
        assert_eq!(s.selected, vec![0], "Ergebnis ist selektiert");
        s.undo();
        assert_eq!(s.shapes.len(), 2, "Undo stellt die Eingaben wieder her");
    }

    #[test]
    fn boolean_ohne_ueberlappung_intersect_aendert_nichts() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 5.0,
            h: 5.0,
        });
        let c = s.layers[0].color;
        s.selected.clear();
        s.activate_color(c);
        s.add_shape(Geo::Rect {
            x: 50.0,
            y: 0.0,
            w: 5.0,
            h: 5.0,
        });
        s.selected = vec![0, 1];
        s.boolean_selected(BoolOp::Intersect);
        assert_eq!(s.shapes.len(), 2, "leerer Schnitt zerstört nichts");
    }

    #[test]
    fn offset_selected_fuegt_kontur_hinzu_und_behaelt_original() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        s.selected = vec![0];
        s.offset_selected(2.0);
        assert_eq!(s.shapes.len(), 2, "Original + Offset-Kontur");
        assert!(matches!(s.shapes[1].geo, Geo::Polyline { .. }));
    }

    #[test]
    fn fillet_selected_ersetzt_rect_durch_polyline() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        s.selected = vec![0];
        s.fillet_selected(2.0);
        assert_eq!(s.shapes.len(), 1);
        let Geo::Polyline { ref pts, closed } = s.shapes[0].geo else {
            panic!("Polyline erwartet");
        };
        assert!(closed);
        assert!(pts.len() > 4, "Bögen eingefügt");
    }
}
