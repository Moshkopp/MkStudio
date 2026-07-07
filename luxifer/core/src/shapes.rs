//! Formgeneratoren: parametrische Polygone als Punktfolgen (Baustein F).
//!
//! Alle Formen entstehen als geschlossene Punktliste in mm und werden im Modell
//! als `Geo::Polyline { closed: true }` abgelegt — es gibt keinen eigenen
//! Polygon-Geo-Typ. Der Katalog [`PolyShape::ALL`] ist **datengetrieben**: eine
//! neue Form wird hier als Enum-Variante + Zeile in `ALL` ergänzt, das Frontend
//! rendert die Auswahl generisch (keine neuen Buttons).
//!
//! Angelehnt an ThorBurns `core/geometry/shapes.rs`, aber neu implementiert.

use serde::{Deserialize, Serialize};

use crate::geometry::Pt;

/// Die wählbaren parametrischen Formen. Reihenfolge = Reihenfolge in der Galerie.
///
/// Serde-Repräsentation ist der Kleinbuchstaben-`id` (z. B. `"hex"`), damit das
/// Frontend die Form über einen stabilen String wählt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolyShape {
    Tri,
    Quad,
    Penta,
    Hex,
    Octa,
    Star,
    Sun,
    Gear,
    Heart,
}

/// Ein Katalog-Eintrag fürs Frontend: stabile `id` + Anzeigename + Icon-Name.
/// Das Frontend baut daraus die Galerie, ohne die Formen zu kennen.
#[derive(Debug, Clone, Serialize)]
pub struct ShapeInfo {
    /// Stabiler Bezeichner (= serde-`id`), z. B. `"hex"`.
    pub id: String,
    /// Anzeigename (deutsch) für Tooltip/Label.
    pub label: String,
    /// Name des SVG-Icons in `Icon.svelte` (deckungsgleich mit `id`).
    pub icon: String,
}

impl PolyShape {
    /// Alle Formen in Galerie-Reihenfolge.
    pub const ALL: [PolyShape; 9] = [
        PolyShape::Tri,
        PolyShape::Quad,
        PolyShape::Penta,
        PolyShape::Hex,
        PolyShape::Octa,
        PolyShape::Star,
        PolyShape::Sun,
        PolyShape::Gear,
        PolyShape::Heart,
    ];

    /// Stabiler Bezeichner (identisch mit der serde-Serialisierung und dem
    /// Icon-Namen im Frontend).
    pub fn id(self) -> &'static str {
        match self {
            PolyShape::Tri => "tri",
            PolyShape::Quad => "quad",
            PolyShape::Penta => "penta",
            PolyShape::Hex => "hex",
            PolyShape::Octa => "octa",
            PolyShape::Star => "star",
            PolyShape::Sun => "sun",
            PolyShape::Gear => "gear",
            PolyShape::Heart => "heart",
        }
    }

    /// Deutscher Anzeigename.
    pub fn label(self) -> &'static str {
        match self {
            PolyShape::Tri => "Dreieck",
            PolyShape::Quad => "Raute",
            PolyShape::Penta => "Fünfeck",
            PolyShape::Hex => "Sechseck",
            PolyShape::Octa => "Achteck",
            PolyShape::Star => "Stern",
            PolyShape::Sun => "Sonne",
            PolyShape::Gear => "Zahnrad",
            PolyShape::Heart => "Herz",
        }
    }

    /// Wählt eine Form über ihren stabilen Bezeichner (Frontend → Core).
    pub fn from_id(id: &str) -> Option<PolyShape> {
        PolyShape::ALL.into_iter().find(|s| s.id() == id)
    }

    /// Katalog für das Frontend (datengetriebene Galerie).
    pub fn catalog() -> Vec<ShapeInfo> {
        PolyShape::ALL
            .into_iter()
            .map(|s| ShapeInfo {
                id: s.id().to_string(),
                label: s.label().to_string(),
                icon: s.id().to_string(),
            })
            .collect()
    }

    /// Erzeugt die geschlossene Punktfolge der Form (mm).
    ///
    /// `(cx, cy)` = Mittelpunkt, `r` = Außenradius, `rot` = Drehung in Grad
    /// (im Uhrzeigersinn bei y-nach-unten). Die einfachen n-Ecke sitzen mit der
    /// Spitze oben; Stern/Sonne/Zahnrad nutzen einen zweiten (inneren) Ring.
    pub fn points(self, cx: f64, cy: f64, r: f64, rot: f64) -> Vec<Pt> {
        let r = r.max(crate::geometry::MIN_SIZE);
        match self {
            PolyShape::Tri => regular_ngon(3, cx, cy, r, rot),
            PolyShape::Quad => regular_ngon(4, cx, cy, r, rot),
            PolyShape::Penta => regular_ngon(5, cx, cy, r, rot),
            PolyShape::Hex => regular_ngon(6, cx, cy, r, rot),
            PolyShape::Octa => regular_ngon(8, cx, cy, r, rot),
            // Fünfzackiger Stern, innerer Ring ~38 % (klassische Sternoptik).
            PolyShape::Star => star_ring(5, cx, cy, r, r * 0.382, rot),
            // Sonne: viele kurze Zacken (12 Strahlen), innerer Ring 78 %.
            PolyShape::Sun => star_ring(12, cx, cy, r, r * 0.78, rot),
            // Zahnrad: rechteckige Zähne über zwei Ringe (10 Zähne).
            PolyShape::Gear => gear_ring(10, cx, cy, r, r * 0.72, rot),
            PolyShape::Heart => heart_points(cx, cy, r, rot),
        }
    }
}

/// Reguläres n-Eck: `n` Ecken gleichmäßig auf dem Ring, erste Ecke oben
/// (−90°), dann um `rot` gedreht.
fn regular_ngon(n: usize, cx: f64, cy: f64, r: f64, rot: f64) -> Vec<Pt> {
    let start = -std::f64::consts::FRAC_PI_2 + rot.to_radians();
    let step = std::f64::consts::TAU / n as f64;
    (0..n)
        .map(|i| {
            let a = start + step * i as f64;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// Sternförmiger Doppelring: `points` Spitzen, abwechselnd Außen-/Innenradius.
/// Erste Spitze oben, dann um `rot` gedreht. Ergibt `2*points` Punkte.
fn star_ring(points: usize, cx: f64, cy: f64, r_out: f64, r_in: f64, rot: f64) -> Vec<Pt> {
    let start = -std::f64::consts::FRAC_PI_2 + rot.to_radians();
    let step = std::f64::consts::PI / points as f64; // halber Ecken-Abstand
    (0..points * 2)
        .map(|i| {
            let a = start + step * i as f64;
            let rr = if i % 2 == 0 { r_out } else { r_in };
            (cx + rr * a.cos(), cy + rr * a.sin())
        })
        .collect()
}

/// Zahnrad: `teeth` rechteckige Zähne. Je Zahn vier Punkte (innen→außen→
/// außen→innen), sodass die Zähne eckig statt spitz sind.
fn gear_ring(teeth: usize, cx: f64, cy: f64, r_out: f64, r_in: f64, rot: f64) -> Vec<Pt> {
    let start = -std::f64::consts::FRAC_PI_2 + rot.to_radians();
    let step = std::f64::consts::TAU / teeth as f64;
    // Zahn füllt die Hälfte der Teilung; die Kanten sitzen bei ±quarter.
    let quarter = step / 4.0;
    let mut pts = Vec::with_capacity(teeth * 4);
    for i in 0..teeth {
        let c = start + step * i as f64;
        // Reihenfolge: aufsteigender Winkel, damit die Kontur nicht kreuzt.
        for &(off, rr) in &[
            (-quarter, r_in),
            (-quarter, r_out),
            (quarter, r_out),
            (quarter, r_in),
        ] {
            let a = c + off;
            pts.push((cx + rr * a.cos(), cy + rr * a.sin()));
        }
    }
    pts
}

/// Parametrisches Herz, normiert auf den Radius `r` und um `rot` gedreht.
/// Basis ist die klassische Herzkurve; y wird nach unten geklappt (Bildschirm-
/// Koordinaten) und die Form in eine Box der Halbbreite `r` skaliert.
fn heart_points(cx: f64, cy: f64, r: f64, rot: f64) -> Vec<Pt> {
    const SEGS: usize = 60;
    // Rohkurve sammeln, dann auf [-1,1] normieren.
    let mut raw: Vec<Pt> = Vec::with_capacity(SEGS);
    let mut max_abs = 0.0_f64;
    for i in 0..SEGS {
        let t = std::f64::consts::TAU * i as f64 / SEGS as f64;
        let x = 16.0 * t.sin().powi(3);
        // Minus: mathematisches y zeigt nach oben, Bildschirm nach unten.
        let y = -(13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos());
        max_abs = max_abs.max(x.abs()).max(y.abs());
        raw.push((x, y));
    }
    let scale = if max_abs > 0.0 { r / max_abs } else { 1.0 };
    let (s, co) = rot.to_radians().sin_cos();
    raw.into_iter()
        .map(|(x, y)| {
            let x = x * scale;
            let y = y * scale;
            // Um den Mittelpunkt drehen.
            (cx + x * co - y * s, cy + x * s + y * co)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn katalog_hat_alle_formen_mit_stabiler_id() {
        let cat = PolyShape::catalog();
        assert_eq!(cat.len(), 9);
        assert_eq!(cat[0].id, "tri");
        assert_eq!(cat[3].id, "hex");
        // id == icon (Frontend nutzt denselben Namen).
        assert!(cat.iter().all(|c| c.id == c.icon));
    }

    #[test]
    fn from_id_findet_und_verwirft() {
        assert_eq!(PolyShape::from_id("star"), Some(PolyShape::Star));
        assert_eq!(PolyShape::from_id("herz"), None);
    }

    #[test]
    fn serde_id_ist_kleinbuchstaben() {
        let j = serde_json::to_string(&PolyShape::Hex).unwrap();
        assert_eq!(j, "\"hex\"");
        let s: PolyShape = serde_json::from_str("\"star\"").unwrap();
        assert_eq!(s, PolyShape::Star);
    }

    #[test]
    fn ngon_hat_richtige_punktzahl_und_liegt_im_radius() {
        let pts = PolyShape::Hex.points(0.0, 0.0, 10.0, 0.0);
        assert_eq!(pts.len(), 6);
        for (x, y) in &pts {
            let d = (x * x + y * y).sqrt();
            assert!((d - 10.0).abs() < 1e-6, "Punkt nicht auf dem Radius: {d}");
        }
    }

    #[test]
    fn dreieck_hat_spitze_oben() {
        let pts = PolyShape::Tri.points(0.0, 0.0, 10.0, 0.0);
        // Erste Ecke oben: x≈0, y≈-r.
        assert!(pts[0].0.abs() < 1e-6);
        assert!((pts[0].1 + 10.0).abs() < 1e-6);
    }

    #[test]
    fn stern_hat_doppelte_punktzahl_und_abwechselnde_radien() {
        let pts = PolyShape::Star.points(0.0, 0.0, 10.0, 0.0);
        assert_eq!(pts.len(), 10);
        let d0 = (pts[0].0.powi(2) + pts[0].1.powi(2)).sqrt();
        let d1 = (pts[1].0.powi(2) + pts[1].1.powi(2)).sqrt();
        assert!(
            d0 > d1,
            "Außenspitze muss weiter außen liegen als Innenpunkt"
        );
    }

    #[test]
    fn zahnrad_hat_vier_punkte_pro_zahn() {
        let pts = PolyShape::Gear.points(0.0, 0.0, 10.0, 0.0);
        assert_eq!(pts.len(), 40); // 10 Zähne × 4 Punkte
    }

    #[test]
    fn herz_liegt_in_der_box_und_ist_nicht_leer() {
        let pts = PolyShape::Heart.points(0.0, 0.0, 10.0, 0.0);
        assert!(pts.len() > 3);
        // Keine Ausdehnung über den Radius hinaus (Normierung).
        for (x, y) in &pts {
            assert!(x.abs() <= 10.0 + 1e-6);
            assert!(y.abs() <= 10.0 + 1e-6);
        }
    }

    #[test]
    fn rotation_dreht_die_erste_ecke() {
        // 90° gedreht: obere Ecke wandert nach rechts (x≈+r, y≈0).
        let pts = PolyShape::Tri.points(0.0, 0.0, 10.0, 90.0);
        assert!((pts[0].0 - 10.0).abs() < 1e-6);
        assert!(pts[0].1.abs() < 1e-6);
    }

    #[test]
    fn mindestradius_wird_erzwungen() {
        let pts = PolyShape::Quad.points(0.0, 0.0, 0.0, 0.0);
        // Kein Punkt exakt im Zentrum (Radius wurde auf MIN_SIZE angehoben).
        assert!(pts.iter().any(|(x, y)| x.abs() > 0.0 || y.abs() > 0.0));
    }
}
