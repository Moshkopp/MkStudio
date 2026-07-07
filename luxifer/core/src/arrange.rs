//! Anordnen: Ausrichten und Verteilen der Auswahl. Reine Core-Logik.
//!
//! Ausrichten braucht ≥ 2, Verteilen ≥ 3 Objekte. Die Funktionen liefern je
//! Auswahl-Objekt ein Verschiebe-Delta (dx, dy) in mm; das Anwenden übernimmt
//! `AppState`.

use crate::geometry::{Axis, BBox};
use crate::state::AppState;

/// Ausricht-Art.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    HCenter,
    Right,
    Top,
    VCenter,
    Bottom,
}

/// Verteil-Art (gleiche Abstände der Startkanten).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distribute {
    Horizontal,
    Vertical,
}

impl AppState {
    pub fn can_align(&self) -> bool {
        self.selected.len() >= 2
    }
    pub fn can_distribute(&self) -> bool {
        self.selected.len() >= 3
    }

    /// Richtet die Auswahl an der gemeinsamen Kante/Mitte aus (ein Undo-Punkt).
    pub fn align_selection(&mut self, kind: Align) {
        if !self.can_align() {
            return;
        }
        let Some(g) = self.selection_bbox() else {
            return;
        };
        self.push_undo();
        let sel = self.selected.clone();
        for idx in sel {
            let Some(s) = self.shapes.get_mut(idx) else {
                continue;
            };
            let b = s.bbox();
            let (dx, dy) = align_delta(kind, &g, &b);
            s.geo.translate(dx, dy);
        }
        self.dirty = true;
    }

    /// Spiegelbar, sobald mindestens eine Form selektiert ist.
    pub fn can_mirror(&self) -> bool {
        !self.selected.is_empty()
    }

    /// Spiegelt die Auswahl an der Mittelachse ihrer gemeinsamen Bounding-Box
    /// (ein Undo-Punkt). `Axis::Vertical` klappt links↔rechts, `Axis::Horizontal`
    /// oben↔unten. Bei mehreren Formen spiegeln auch die Lagen zueinander.
    pub fn mirror_selection(&mut self, axis: Axis) {
        if !self.can_mirror() {
            return;
        }
        let Some(g) = self.selection_bbox() else {
            return;
        };
        let coord = match axis {
            Axis::Vertical => g.x + g.w / 2.0,
            Axis::Horizontal => g.y + g.h / 2.0,
        };
        self.push_undo();
        let sel = self.selected.clone();
        for idx in sel {
            if let Some(s) = self.shapes.get_mut(idx) {
                s.geo.mirror(axis, coord);
            }
        }
        self.dirty = true;
    }

    /// Verteilt die Auswahl mit gleichen Startkanten-Abständen (ein Undo-Punkt).
    pub fn distribute_selection(&mut self, kind: Distribute) {
        if !self.can_distribute() {
            return;
        }
        self.push_undo();

        // (Index, Startkante) nach Startkante sortieren.
        let mut items: Vec<(usize, f64)> = self
            .selected
            .iter()
            .filter_map(|&i| self.shapes.get(i).map(|s| (i, start_edge(kind, &s.bbox()))))
            .collect();
        items.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let n = items.len();
        let first = items[0].1;
        let last = items[n - 1].1;
        let step = (last - first) / (n as f64 - 1.0);

        for (k, &(idx, cur)) in items.iter().enumerate() {
            if k == 0 || k == n - 1 {
                continue; // Ränder bleiben stehen
            }
            let target = first + step * k as f64;
            let delta = target - cur;
            if let Some(s) = self.shapes.get_mut(idx) {
                match kind {
                    Distribute::Horizontal => s.geo.translate(delta, 0.0),
                    Distribute::Vertical => s.geo.translate(0.0, delta),
                }
            }
        }
        self.dirty = true;
    }
}

fn start_edge(kind: Distribute, b: &BBox) -> f64 {
    match kind {
        Distribute::Horizontal => b.x,
        Distribute::Vertical => b.y,
    }
}

fn align_delta(kind: Align, g: &BBox, b: &BBox) -> (f64, f64) {
    match kind {
        Align::Left => (g.x - b.x, 0.0),
        Align::Right => (g.x + g.w - (b.x + b.w), 0.0),
        Align::HCenter => (g.x + g.w / 2.0 - (b.x + b.w / 2.0), 0.0),
        Align::Top => (0.0, g.y - b.y),
        Align::Bottom => (0.0, g.y + g.h - (b.y + b.h)),
        Align::VCenter => (0.0, g.y + g.h / 2.0 - (b.y + b.h / 2.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Geo;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Geo {
        Geo::Rect { x, y, w, h }
    }

    #[test]
    fn align_left_richtet_an_gemeinsamer_kante() {
        let mut s = AppState::new();
        s.add_shape(rect(10.0, 0.0, 20.0, 10.0));
        s.add_shape(rect(50.0, 30.0, 20.0, 10.0));
        s.selected = vec![0, 1];
        s.align_selection(Align::Left);
        assert_eq!(s.shapes[0].bbox().x, 10.0);
        assert_eq!(s.shapes[1].bbox().x, 10.0);
    }

    #[test]
    fn align_hcenter_zentriert() {
        let mut s = AppState::new();
        s.add_shape(rect(0.0, 0.0, 20.0, 10.0)); // Mitte 10
        s.add_shape(rect(80.0, 0.0, 20.0, 10.0)); // Mitte 90
        s.selected = vec![0, 1];
        s.align_selection(Align::HCenter);
        // Gruppenmitte 50 → beide Mitten auf 50.
        assert!((s.shapes[0].bbox().center().0 - 50.0).abs() < 1e-9);
        assert!((s.shapes[1].bbox().center().0 - 50.0).abs() < 1e-9);
    }

    #[test]
    fn distribute_horizontal_verteilt_mitte() {
        let mut s = AppState::new();
        s.add_shape(rect(0.0, 0.0, 5.0, 5.0)); // Start 0
        s.add_shape(rect(10.0, 0.0, 5.0, 5.0)); // Start 10 → soll 45
        s.add_shape(rect(90.0, 0.0, 5.0, 5.0)); // Start 90
        s.selected = vec![0, 1, 2];
        s.distribute_selection(Distribute::Horizontal);
        assert!((s.shapes[1].bbox().x - 45.0).abs() < 1e-9);
        assert_eq!(s.shapes[0].bbox().x, 0.0);
        assert_eq!(s.shapes[2].bbox().x, 90.0);
    }

    #[test]
    fn mirror_vertikal_klappt_gruppe_um_ihre_mitte() {
        let mut s = AppState::new();
        s.add_shape(rect(0.0, 0.0, 10.0, 10.0)); // links
        s.add_shape(rect(90.0, 0.0, 10.0, 10.0)); // rechts
        s.selected = vec![0, 1];
        // Gruppen-BBox 0..100, Achse x=50. Formen tauschen die Seite.
        s.mirror_selection(Axis::Vertical);
        assert_eq!(s.shapes[0].bbox().x, 90.0);
        assert_eq!(s.shapes[1].bbox().x, 0.0);
    }

    #[test]
    fn mirror_einzeln_asymmetrisch_spiegelt_form() {
        let mut s = AppState::new();
        s.add_shape(Geo::Polyline {
            pts: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 5.0)],
            closed: true,
        });
        s.selected = vec![0];
        // BBox 0..10 in x → Achse x=5.
        s.mirror_selection(Axis::Vertical);
        if let Geo::Polyline { pts, .. } = &s.shapes[0].geo {
            assert_eq!(pts[0], (10.0, 0.0));
            assert_eq!(pts[1], (0.0, 0.0));
            assert_eq!(pts[2], (0.0, 5.0));
        } else {
            panic!("kein Polyline");
        }
    }

    #[test]
    fn mirror_ohne_auswahl_ist_noop() {
        let mut s = AppState::new();
        s.add_shape(rect(0.0, 0.0, 10.0, 10.0));
        s.selected.clear();
        assert!(!s.can_mirror());
        s.mirror_selection(Axis::Horizontal); // no-op
        assert_eq!(s.shapes[0].bbox().y, 0.0);
    }

    #[test]
    fn align_braucht_mindestens_zwei() {
        let mut s = AppState::new();
        s.add_shape(rect(0.0, 0.0, 5.0, 5.0));
        s.selected = vec![0];
        assert!(!s.can_align());
        s.align_selection(Align::Left); // no-op
        assert_eq!(s.shapes[0].bbox().x, 0.0);
    }
}
