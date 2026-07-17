//! Vektor-Import: SVG und DXF → Polylinien-Konturen in mm.
//!
//! Reine Umwandlung (UI-frei, testbar): der Aufrufer liefert die Datei-Bytes,
//! der Core liefert Konturen (Punkte in mm + geschlossen-Flag). Platzieren/
//! Layern übernimmt der Aufrufer (`AppState::add_polylines`).
//!
//! Nach v3-Analyse neu gebaut; dieselben Bibliothekswahlen (`usvg` flacht
//! SVG-Transformationen/Formen zu Pfaden ab; `dxf` parst CAD-Entities).
//! SVG-Einheiten: usvg arbeitet in CSS-px → Umrechnung 25,4/96 mm je px.
//! DXF: Einheit wird als mm angenommen (üblich im Laserumfeld), y-Achse wird
//! gespiegelt (DXF: y nach oben, Canvas: y nach unten).

use crate::geometry::Pt;

pub type ImportedContour = (Vec<Pt>, bool);
pub type ImportedCompound = Vec<ImportedContour>;

/// Fehler beim Vektor-Import.
#[derive(Debug)]
pub struct ImportError(pub String);

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ImportError {}

/// mm je CSS-Pixel (96 dpi — SVG-Standardauflösung).
const MM_PER_PX: f64 = 25.4 / 96.0;

/// Importiert eine Vektordatei anhand der Endung (`svg` oder `dxf`).
pub fn import_vector(bytes: &[u8], ext: &str) -> Result<Vec<ImportedContour>, ImportError> {
    Ok(import_vector_compounds(bytes, ext)?
        .into_iter()
        .flatten()
        .collect())
}

/// Importiert Vektorgeometrie unter Erhalt zusammengesetzter Pfade. Innerhalb
/// eines Eintrags gilt die SVG-Füllregel; verschiedene Einträge sind getrennte
/// gemalte Flächen und dürfen einander nicht per globalem XOR ausstanzen.
pub fn import_vector_compounds(
    bytes: &[u8],
    ext: &str,
) -> Result<Vec<ImportedCompound>, ImportError> {
    match ext.to_ascii_lowercase().as_str() {
        "svg" => import_svg_compounds(bytes),
        "dxf" => import_dxf(bytes)
            .map(|contours| contours.into_iter().map(|contour| vec![contour]).collect()),
        other => Err(ImportError(format!("Nicht unterstütztes Format: .{other}"))),
    }
}

/// SVG → Konturen. usvg löst Transformationen, `<use>`, Formen (rect/circle/…)
/// bereits in absolute Pfade auf; hier nur noch flatten + Einheit umrechnen.
pub fn import_svg(bytes: &[u8]) -> Result<Vec<ImportedContour>, ImportError> {
    Ok(import_svg_compounds(bytes)?.into_iter().flatten().collect())
}

pub fn import_svg_compounds(bytes: &[u8]) -> Result<Vec<ImportedCompound>, ImportError> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(bytes, &opt)
        .map_err(|e| ImportError(format!("SVG unlesbar: {e}")))?;
    let mut out = Vec::new();
    collect_group(tree.root(), &mut out);
    if out.is_empty() {
        return Err(ImportError("SVG enthält keine Pfade.".into()));
    }
    Ok(out)
}

fn collect_group(g: &usvg::Group, out: &mut Vec<ImportedCompound>) {
    for node in g.children() {
        match node {
            usvg::Node::Group(sub) => collect_group(sub, out),
            usvg::Node::Path(p) if is_laser_geometry(p) => {
                let mut compound = Vec::new();
                collect_path(p, &mut compound);
                if !compound.is_empty() {
                    out.push(compound);
                }
            }
            usvg::Node::Path(_) => {}
            // Bilder/Text im SVG werden bewusst ignoriert (Bilder importiert
            // man als Bild; SVG-Text sollte als Pfad exportiert sein).
            usvg::Node::Image(_) | usvg::Node::Text(_) => {}
        }
    }
}

/// Weiße, reine Füllflächen sind in Grafik-SVGs typischerweise der
/// exportierte Hintergrund und keine zu lasernde Geometrie. Ein vorhandener
/// Stroke bleibt dagegen erhalten: eine weiß gefüllte, dunkel umrandete Form
/// ist weiterhin ein absichtlicher Vektorpfad.
fn is_laser_geometry(path: &usvg::Path) -> bool {
    if path.stroke().is_some() {
        return true;
    }
    match path.fill().map(usvg::Fill::paint) {
        Some(usvg::Paint::Color(color)) => color.red < 250 || color.green < 250 || color.blue < 250,
        Some(_) => true,
        None => false,
    }
}

/// Unterteilungen je Bézier-Segment.
const CURVE_SEGS: usize = 12;

fn collect_path(p: &usvg::Path, out: &mut Vec<ImportedContour>) {
    let t = p.abs_transform();
    let map = |x: f32, y: f32| -> Pt {
        let mut pt = tiny_skia_path_point(x, y);
        t.map_point(&mut pt);
        (pt.x as f64 * MM_PER_PX, pt.y as f64 * MM_PER_PX)
    };

    let mut cur: Vec<Pt> = Vec::new();
    // SVG schließt offene Teilpfade für die Füllauswertung implizit. Viele
    // Exporter (auch das reale Anker-Asset) sparen das letzte `Z` deshalb aus.
    // Reine Stroke-Pfade bleiben dagegen offen.
    let implicit_fill_close = p.fill().is_some();
    let flush = |cur: &mut Vec<Pt>, closed: bool, out: &mut Vec<(Vec<Pt>, bool)>| {
        if cur.len() >= 2 {
            out.push((std::mem::take(cur), closed));
        } else {
            cur.clear();
        }
    };

    for seg in p.data().segments() {
        use usvg::tiny_skia_path::PathSegment::*;
        match seg {
            MoveTo(pt) => {
                flush(&mut cur, implicit_fill_close, out);
                cur.push(map(pt.x, pt.y));
            }
            LineTo(pt) => cur.push(map(pt.x, pt.y)),
            QuadTo(c, pt) => {
                let p0 = *cur.last().unwrap_or(&map(c.x, c.y));
                let c1 = map(c.x, c.y);
                let p1 = map(pt.x, pt.y);
                for i in 1..=CURVE_SEGS {
                    let t = i as f64 / CURVE_SEGS as f64;
                    let u = 1.0 - t;
                    cur.push((
                        u * u * p0.0 + 2.0 * u * t * c1.0 + t * t * p1.0,
                        u * u * p0.1 + 2.0 * u * t * c1.1 + t * t * p1.1,
                    ));
                }
            }
            CubicTo(c1, c2, pt) => {
                let p0 = *cur.last().unwrap_or(&map(c1.x, c1.y));
                let k1 = map(c1.x, c1.y);
                let k2 = map(c2.x, c2.y);
                let p1 = map(pt.x, pt.y);
                for i in 1..=CURVE_SEGS {
                    let t = i as f64 / CURVE_SEGS as f64;
                    let u = 1.0 - t;
                    cur.push((
                        u * u * u * p0.0
                            + 3.0 * u * u * t * k1.0
                            + 3.0 * u * t * t * k2.0
                            + t * t * t * p1.0,
                        u * u * u * p0.1
                            + 3.0 * u * u * t * k1.1
                            + 3.0 * u * t * t * k2.1
                            + t * t * t * p1.1,
                    ));
                }
            }
            Close => flush(&mut cur, true, out),
        }
    }
    flush(&mut cur, implicit_fill_close, out);
}

fn tiny_skia_path_point(x: f32, y: f32) -> usvg::tiny_skia_path::Point {
    usvg::tiny_skia_path::Point::from_xy(x, y)
}

/// DXF → Konturen. Unterstützt LINE, LWPOLYLINE, POLYLINE, CIRCLE, ARC.
/// y wird gespiegelt (DXF y↑ → Canvas y↓) und alles auf min=(0,0) verschoben.
pub fn import_dxf(bytes: &[u8]) -> Result<Vec<(Vec<Pt>, bool)>, ImportError> {
    let mut cursor = std::io::Cursor::new(bytes);
    let drawing =
        dxf::Drawing::load(&mut cursor).map_err(|e| ImportError(format!("DXF unlesbar: {e}")))?;

    let mut out: Vec<(Vec<Pt>, bool)> = Vec::new();
    for e in drawing.entities() {
        use dxf::entities::EntityType;
        match &e.specific {
            EntityType::Line(l) => {
                out.push((vec![(l.p1.x, -l.p1.y), (l.p2.x, -l.p2.y)], false));
            }
            EntityType::LwPolyline(p) => {
                let pts: Vec<Pt> = p.vertices.iter().map(|v| (v.x, -v.y)).collect();
                if pts.len() >= 2 {
                    out.push((pts, p.is_closed()));
                }
            }
            EntityType::Polyline(p) => {
                let pts: Vec<Pt> = p
                    .vertices()
                    .map(|v| (v.location.x, -v.location.y))
                    .collect();
                if pts.len() >= 2 {
                    out.push((pts, p.is_closed()));
                }
            }
            EntityType::Circle(c) => {
                let segs = 64;
                let pts: Vec<Pt> = (0..segs)
                    .map(|i| {
                        let a = i as f64 / segs as f64 * std::f64::consts::TAU;
                        (
                            c.center.x + c.radius * a.cos(),
                            -(c.center.y + c.radius * a.sin()),
                        )
                    })
                    .collect();
                out.push((pts, true));
            }
            EntityType::Arc(a) => {
                // Winkel in Grad, CCW im DXF-System.
                let (mut a0, mut a1) = (a.start_angle.to_radians(), a.end_angle.to_radians());
                if a1 <= a0 {
                    a1 += std::f64::consts::TAU;
                }
                let sweep = a1 - a0;
                let segs = ((sweep / std::f64::consts::TAU * 64.0).ceil() as usize).max(2);
                let pts: Vec<Pt> = (0..=segs)
                    .map(|i| {
                        let ang = a0 + sweep * i as f64 / segs as f64;
                        (
                            a.center.x + a.radius * ang.cos(),
                            -(a.center.y + a.radius * ang.sin()),
                        )
                    })
                    .collect();
                let _ = &mut a0;
                out.push((pts, false));
            }
            // SPLINE u. a.: vorerst nicht unterstützt (bewusst; selten in
            // Laser-DXFs, und lieber sichtbar fehlen als falsch nähern).
            _ => {}
        }
    }
    if out.is_empty() {
        return Err(ImportError(
            "DXF enthält keine unterstützten Entities (LINE/POLYLINE/CIRCLE/ARC).".into(),
        ));
    }

    // Auf (0,0) normalisieren, damit das Motiv nicht außerhalb des Betts liegt.
    let (mut min_x, mut min_y) = (f64::MAX, f64::MAX);
    for (pts, _) in &out {
        for &(x, y) in pts {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
        }
    }
    for (pts, _) in &mut out {
        for p in pts.iter_mut() {
            *p = (p.0 - min_x, p.1 - min_y);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_rect_wird_geschlossene_kontur() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <rect x="10" y="10" width="50" height="30"/></svg>"#;
        let out = import_svg(svg).unwrap();
        assert_eq!(out.len(), 1);
        let (pts, closed) = &out[0];
        assert!(*closed);
        // 50 px Breite = 50 * 25,4/96 ≈ 13,23 mm.
        let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
        let w = xs.iter().cloned().fold(f64::MIN, f64::max)
            - xs.iter().cloned().fold(f64::MAX, f64::min);
        assert!((w - 50.0 * MM_PER_PX).abs() < 0.01, "Breite war {w}");
    }

    #[test]
    fn svg_mit_transform_wird_aufgeloest() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <g transform="translate(20,0)"><rect x="0" y="0" width="10" height="10"/></g></svg>"#;
        let out = import_svg(svg).unwrap();
        let min_x = out[0].0.iter().map(|p| p.0).fold(f64::MAX, f64::min);
        assert!((min_x - 20.0 * MM_PER_PX).abs() < 0.01, "translate wirkt");
    }

    #[test]
    fn svg_kreis_und_pfad_werden_gefunden() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <circle cx="50" cy="50" r="20"/>
            <path d="M 10 10 L 30 10 L 30 30 Z"/></svg>"#;
        let out = import_svg(svg).unwrap();
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|(_, closed)| *closed));
    }

    #[test]
    fn svg_teilkonturen_bleiben_beim_ursprungspfad() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <path fill="black" fill-rule="evenodd"
                  d="M 0 0 H 40 V 40 H 0 Z M 10 10 H 30 V 30 H 10 Z"/>
            <path fill="black" d="M 20 20 H 60 V 60 H 20 Z"/>
        </svg>"#;
        let compounds = import_svg_compounds(svg).unwrap();
        assert_eq!(compounds.len(), 2, "zwei gemalte SVG-Pfade");
        assert_eq!(compounds[0].len(), 2, "Außenkontur plus Loch");
        assert_eq!(compounds[1].len(), 1);
    }

    #[test]
    fn svg_fill_schliesst_teilpfad_auch_ohne_z_implizit() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <path fill="black" d="M 0 0 H 40 V 40 H 0"/>
            <path fill="none" stroke="black" d="M 50 0 H 90 V 40 H 50"/>
        </svg>"#;
        let compounds = import_svg_compounds(svg).unwrap();
        assert!(compounds[0][0].1, "Füllpfad wird implizit geschlossen");
        assert!(!compounds[1][0].1, "reiner Stroke-Pfad bleibt offen");
    }

    #[test]
    fn svg_weisser_hintergrund_wird_nicht_zur_laserflaeche() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <rect x="-10" y="-10" width="120" height="120" fill="white"/>
            <path fill="black" fill-rule="evenodd"
                  d="M 10 10 L 90 10 L 90 90 L 10 90 Z
                     M 30 30 L 30 70 L 70 70 L 70 30 Z"/>
        </svg>"#;
        let out = import_svg(svg).unwrap();
        assert_eq!(
            out.len(),
            2,
            "nur die beiden Teilkonturen des schwarzen Pfads"
        );
        assert!(out.iter().all(|(_, closed)| *closed));
    }

    #[test]
    fn svg_weiss_gefuellte_form_mit_stroke_bleibt_erhalten() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <circle cx="50" cy="50" r="20" fill="white" stroke="black"/>
        </svg>"#;
        let out = import_svg(svg).unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].1);
    }

    #[test]
    fn kaputtes_svg_gibt_fehler() {
        assert!(import_svg(b"kein svg").is_err());
    }

    #[test]
    fn dxf_line_und_kreis() {
        // Minimales DXF über die Bibliothek selbst erzeugen (round-trip).
        use dxf::entities::{Circle, Entity, EntityType, Line};
        use dxf::{Drawing, Point};
        let mut d = Drawing::new();
        d.add_entity(Entity::new(EntityType::Line(Line::new(
            Point::new(0.0, 0.0, 0.0),
            Point::new(10.0, 0.0, 0.0),
        ))));
        d.add_entity(Entity::new(EntityType::Circle(Circle::new(
            Point::new(5.0, 5.0, 0.0),
            3.0,
        ))));
        let mut buf = Vec::new();
        d.save(&mut buf).unwrap();

        let out = import_dxf(&buf).unwrap();
        assert_eq!(out.len(), 2);
        let closed_count = out.iter().filter(|(_, c)| *c).count();
        assert_eq!(closed_count, 1, "Kreis geschlossen, Linie offen");
    }

    #[test]
    fn unbekannte_endung_gibt_fehler() {
        assert!(import_vector(b"x", "pdf").is_err());
    }
}
