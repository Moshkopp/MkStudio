//! Even-Odd-Scanline-Füllung: geschlossene Konturen → horizontale Segmente.
//!
//! Reine Geometrie in mm (UI-frei, testbar). Angelehnt an ThorBurns
//! `hardware/job/scanline.rs` (docs/referenz/): Even-Odd (nicht Nonzero, damit
//! Löcher/Buchstaben-Innenräume ausgespart bleiben), nur geschlossene Konturen
//! schließen implizit, halb-offenes Y-Intervall (jede Kante zählt einmal).

use crate::geometry::Pt;

/// Ein horizontales Füll-Segment in mm: bei `y` von `x0` bis `x1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillSegment {
    pub y: f64,
    pub x0: f64,
    pub x1: f64,
}

/// Eine Kontur für die Scanline: Punktfolge + ob geschlossen.
pub struct Contour<'a> {
    pub points: &'a [Pt],
    pub closed: bool,
}

/// Füllt alle Konturen gemeinsam (Even-Odd) mit Zeilenabstand `step_mm`.
/// Überlappende Formen werden korrekt kombiniert. `step_mm` wird auf ein
/// sinnvolles Minimum begrenzt.
pub fn fill_segments(contours: &[Contour], step_mm: f64) -> Vec<FillSegment> {
    fill_compound_segments(&[contours], step_mm)
}

/// Füllt zusammengesetzte Pfade korrekt: Even/Odd wird je Pfad ausgewertet,
/// anschließend werden deren Intervalle vereinigt. Damit stanzen getrennte,
/// überlappende SVG-Pfade einander nicht aus.
pub fn fill_compound_segments(compounds: &[&[Contour]], step_mm: f64) -> Vec<FillSegment> {
    if !step_mm.is_finite() {
        return vec![];
    }
    let step = step_mm.max(0.01);

    // Y-Bereich über alle (geschlossenen, ≥3 Punkte) Konturen.
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    let mut any = false;
    for c in compounds.iter().flat_map(|compound| compound.iter()) {
        // Eine offene Polyline begrenzt keine Fläche. Sie darf weder den
        // Scanbereich erweitern noch Schnittpunkte zum Even-Odd-Paaren
        // beitragen; mehrere offene Pattern-Fill-Randstücke würden sonst
        // scheinbar zufällig miteinander zu gefüllten Keilen verbunden.
        if !c.closed || c.points.len() < 3 {
            continue;
        }
        for &(_, y) in c.points {
            if !y.is_finite() {
                continue;
            }
            min_y = min_y.min(y);
            max_y = max_y.max(y);
            any = true;
        }
    }
    if !any || min_y >= max_y {
        return vec![];
    }

    let mut out: Vec<FillSegment> = Vec::new();
    let mut y = min_y;
    while y <= max_y {
        let mut intervals = Vec::new();
        for compound in compounds {
            // Even/Odd-Schnittpunkte nur innerhalb dieses Pfades paaren.
            let mut xs: Vec<f64> = Vec::new();
            for c in *compound {
                let n = c.points.len();
                if !c.closed || n < 3 {
                    continue;
                }
                for i in 0..n {
                    let (x0, y0) = c.points[i];
                    let (x1, y1) = c.points[(i + 1) % n];
                    if ![x0, y0, x1, y1].into_iter().all(f64::is_finite) {
                        continue;
                    }
                    // Halb-offenes Intervall: Kante zählt, wenn y genau eine
                    // Endpunktseite unterschreitet (verhindert Doppelkreuzung
                    // an Scheitelpunkten).
                    if (y0 <= y) != (y1 <= y) {
                        let t = (y - y0) / (y1 - y0);
                        xs.push(x0 + t * (x1 - x0));
                    }
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.chunks_exact(2) {
                if pair[1] > pair[0] {
                    intervals.push((pair[0], pair[1]));
                }
            }
        }

        // Getrennte gemalte Pfade werden flächig vereinigt.
        intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (lo, hi) in intervals {
            if let Some(last) = out.last_mut().filter(|last| last.y == y && lo <= last.x1) {
                last.x1 = last.x1.max(hi);
            } else {
                out.push(FillSegment { y, x0: lo, x1: hi });
            }
        }
        y += step;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_pts(x: f64, y: f64, w: f64, h: f64) -> Vec<Pt> {
        vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
    }

    #[test]
    fn rechteck_wird_zeilenweise_gefuellt() {
        let pts = rect_pts(0.0, 0.0, 10.0, 10.0);
        let c = Contour {
            points: &pts,
            closed: true,
        };
        let segs = fill_segments(&[c], 1.0);
        assert!(!segs.is_empty());
        // Jedes Segment spannt die volle Breite 0..10.
        for s in &segs {
            assert!((s.x0 - 0.0).abs() < 1e-6);
            assert!((s.x1 - 10.0).abs() < 1e-6);
        }
    }

    #[test]
    fn loch_bleibt_ausgespart_even_odd() {
        // Äußeres 20x20-Quadrat, inneres 10x10-Loch (beide geschlossen).
        let outer = rect_pts(0.0, 0.0, 20.0, 20.0);
        let inner = rect_pts(5.0, 5.0, 10.0, 10.0);
        let cs = [
            Contour {
                points: &outer,
                closed: true,
            },
            Contour {
                points: &inner,
                closed: true,
            },
        ];
        let segs = fill_segments(&cs, 1.0);
        // In einer Zeile mitten durchs Loch (y=10) muss es ZWEI Segmente geben
        // (links und rechts vom Loch), nicht eins durchgehend.
        let row: Vec<_> = segs.iter().filter(|s| (s.y - 10.0).abs() < 1e-9).collect();
        assert_eq!(row.len(), 2, "Loch muss ausgespart sein");
    }

    #[test]
    fn getrennte_gefuellte_pfade_werden_vereinigt_statt_ausgestanzt() {
        let left = rect_pts(0.0, 0.0, 10.0, 10.0);
        let right = rect_pts(5.0, 0.0, 10.0, 10.0);
        let a = [Contour {
            points: &left,
            closed: true,
        }];
        let b = [Contour {
            points: &right,
            closed: true,
        }];
        let segments = fill_compound_segments(&[&a, &b], 1.0);
        let row: Vec<_> = segments
            .iter()
            .filter(|segment| (segment.y - 5.0).abs() < 1e-9)
            .collect();
        assert_eq!(row.len(), 1);
        assert_eq!((row[0].x0, row[0].x1), (0.0, 15.0));
    }

    #[test]
    fn offene_kontur_wird_nicht_gefuellt() {
        // Auch eine offene Kontur mit genug Punkten begrenzt keine Fläche.
        // Das ist insbesondere für geclippte Pattern-Fill-Randstücke wichtig.
        let pts = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let c = Contour {
            points: &pts,
            closed: false,
        };
        assert!(fill_segments(&[c], 1.0).is_empty());
    }

    #[test]
    fn nicht_finite_werte_paniken_nicht() {
        let pts = vec![(0.0, 0.0), (10.0, f64::NAN), (10.0, 10.0), (0.0, 10.0)];
        let c = Contour {
            points: &pts,
            closed: true,
        };
        let result = fill_segments(&[c], 1.0);
        assert!(result.iter().all(|segment| {
            segment.y.is_finite() && segment.x0.is_finite() && segment.x1.is_finite()
        }));
        assert!(fill_segments(&[], f64::NAN).is_empty());
    }
}
