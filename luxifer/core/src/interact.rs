//! Interaktions-Logik, die das Frontend nur auslöst: Skalier-Handles,
//! Marquee-Auswahl. Reine Core-Berechnungen (UI-frei, testbar).
//!
//! Angelehnt an ThorBurns `Handle`/`scale_from_handle`/`scale_in_bbox`
//! (docs/referenz/02-funktions-worksheet.md, Baustein D).

use crate::geometry::BBox;
use crate::state::AppState;

/// Welches der 8 Skalier-Handles wird gezogen. Benennung nach bewegter Kante
/// (N=oben, S=unten, W=links, E=rechts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    N,
    S,
    W,
    E,
    Nw,
    Ne,
    Sw,
    Se,
}

impl Handle {
    pub fn moves_left(&self) -> bool {
        matches!(self, Handle::W | Handle::Nw | Handle::Sw)
    }
    pub fn moves_right(&self) -> bool {
        matches!(self, Handle::E | Handle::Ne | Handle::Se)
    }
    pub fn moves_top(&self) -> bool {
        matches!(self, Handle::N | Handle::Nw | Handle::Ne)
    }
    pub fn moves_bottom(&self) -> bool {
        matches!(self, Handle::S | Handle::Sw | Handle::Se)
    }

    /// Alle acht Handles mit ihrer Position an der Bounding-Box (Screen-frei, mm).
    pub fn positions(b: &BBox) -> [(Handle, (f64, f64)); 8] {
        let (x, y, w, h) = (b.x, b.y, b.w, b.h);
        let cx = x + w / 2.0;
        let cy = y + h / 2.0;
        [
            (Handle::Nw, (x, y)),
            (Handle::N, (cx, y)),
            (Handle::Ne, (x + w, y)),
            (Handle::E, (x + w, cy)),
            (Handle::Se, (x + w, y + h)),
            (Handle::S, (cx, y + h)),
            (Handle::Sw, (x, y + h)),
            (Handle::W, (x, cy)),
        ]
    }
}

/// Neue Ziel-Box beim Ziehen eines Handles um (dx,dy), relativ zur Start-Box.
/// Mindestgröße wird von den Formen selbst erzwungen; hier nur die Kanten-Logik.
pub fn resize_bbox(start: BBox, handle: Handle, dx: f64, dy: f64) -> BBox {
    let mut x = start.x;
    let mut y = start.y;
    let mut w = start.w;
    let mut h = start.h;
    if handle.moves_left() {
        x += dx;
        w -= dx;
    }
    if handle.moves_right() {
        w += dx;
    }
    if handle.moves_top() {
        y += dy;
        h -= dy;
    }
    if handle.moves_bottom() {
        h += dy;
    }
    BBox::new(x, y, w, h)
}

impl AppState {
    /// Gemeinsame Bounding-Box der aktuellen Auswahl (mm), falls vorhanden.
    pub fn selection_bbox(&self) -> Option<BBox> {
        BBox::union_all(
            self.selected
                .iter()
                .filter_map(|&i| self.shapes.get(i))
                .map(|s| s.bbox()),
        )
    }

    /// Skaliert die Auswahl so, dass ihre Gruppen-Box von `start` auf `target`
    /// wechselt. Jede Form wird proportional in der Box neu platziert/skaliert.
    /// (Aufrufer setzt bei Drag-Beginn einen Undo-Punkt.)
    pub fn scale_selection_to(&mut self, start: BBox, target: BBox) {
        let sw = if start.w > 0.0 {
            target.w / start.w
        } else {
            1.0
        };
        let sh = if start.h > 0.0 {
            target.h / start.h
        } else {
            1.0
        };
        for &idx in &self.selected {
            if let Some(s) = self.shapes.get_mut(idx) {
                let b = s.geo.bbox();
                // Relative Lage der Form-Box in der Start-Gruppen-Box.
                let rx = if start.w > 0.0 {
                    (b.x - start.x) / start.w
                } else {
                    0.0
                };
                let ry = if start.h > 0.0 {
                    (b.y - start.y) / start.h
                } else {
                    0.0
                };
                let nx = target.x + rx * target.w;
                let ny = target.y + ry * target.h;
                s.set_bbox(nx, ny, b.w * sw, b.h * sh);
            }
        }
        self.dirty = true;
    }

    /// Dreht die Auswahl als starren Körper um `degrees` (Grad, im Uhrzeigersinn
    /// bei y-nach-unten) um den Mittelpunkt der Gruppen-Box. Jede Form wandert
    /// mit ihrem Geometrie-Zentrum auf dem Kreisbogen um den Pivot und dreht sich
    /// zusätzlich um denselben Winkel um ihr eigenes Zentrum — so bleibt der
    /// Block formtreu (wie ein gemeinsames Blatt gedreht). Bilder werden nicht
    /// verdreht dargestellt, drehen aber wie alle anderen mit (Box + rotation).
    /// (Aufrufer setzt bei Drag-Beginn einen Undo-Punkt.)
    pub fn rotate_selection(&mut self, degrees: f64) {
        if degrees == 0.0 {
            return;
        }
        let Some(pivot) = self.selection_bbox().map(|b| b.center()) else {
            return;
        };
        for &idx in &self.selected {
            if let Some(s) = self.shapes.get_mut(idx) {
                // Geometrie-Zentrum (ohne Eigen-Rotation) um den Pivot drehen und
                // die Form dorthin verschieben; Eigen-Rotation um den Winkel erhöhen.
                let (cx, cy) = s.geo.bbox().center();
                let (nx, ny) = crate::geometry::rotate_point(cx, cy, pivot.0, pivot.1, degrees);
                s.translate(nx - cx, ny - cy);
                s.rotation = (s.rotation + degrees).rem_euclid(360.0);
            }
        }
        self.dirty = true;
    }

    /// Wählt alle Shapes, deren Bounding-Box vollständig im Rechteck liegt
    /// (Marquee). Überspringt unsichtbare/gesperrte Layer. Ersetzt die Auswahl.
    pub fn select_in_rect(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let (lo_x, hi_x) = (x1.min(x2), x1.max(x2));
        let (lo_y, hi_y) = (y1.min(y2), y1.max(y2));
        self.selected.clear();
        for (i, s) in self.shapes.iter().enumerate() {
            if let Some(l) = self.layers.get(s.layer_id) {
                if !l.visible || l.locked {
                    continue;
                }
            }
            let b = s.bbox();
            if b.x >= lo_x && b.y >= lo_y && b.x + b.w <= hi_x && b.y + b.h <= hi_y {
                self.selected.push(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Geo;

    #[test]
    fn resize_bbox_bewegt_richtige_kanten() {
        let start = BBox::new(0.0, 0.0, 100.0, 100.0);
        // SE zieht rechts+unten größer.
        let se = resize_bbox(start, Handle::Se, 20.0, 10.0);
        assert_eq!((se.x, se.y, se.w, se.h), (0.0, 0.0, 120.0, 110.0));
        // NW zieht links+oben; Ecke wandert, Größe schrumpft.
        let nw = resize_bbox(start, Handle::Nw, 10.0, 10.0);
        assert_eq!((nw.x, nw.y, nw.w, nw.h), (10.0, 10.0, 90.0, 90.0));
    }

    #[test]
    fn handle_positions_liefert_acht() {
        let b = BBox::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(Handle::positions(&b).len(), 8);
    }

    #[test]
    fn selection_bbox_umschliesst_auswahl() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 10.0,
            y: 10.0,
            w: 20.0,
            h: 20.0,
        });
        s.add_shape(Geo::Rect {
            x: 50.0,
            y: 5.0,
            w: 10.0,
            h: 40.0,
        });
        s.selected = vec![0, 1];
        let b = s.selection_bbox().unwrap();
        assert_eq!((b.x, b.y, b.w, b.h), (10.0, 5.0, 50.0, 40.0));
    }

    #[test]
    fn scale_selection_verdoppelt_breite() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
        });
        s.selected = vec![0];
        let start = s.selection_bbox().unwrap();
        let target = BBox::new(0.0, 0.0, 200.0, 50.0);
        s.scale_selection_to(start, target);
        let b = s.shapes[0].bbox();
        assert!((b.w - 200.0).abs() < 1e-9);
        assert!((b.h - 50.0).abs() < 1e-9);
    }

    #[test]
    fn rotate_selection_dreht_um_gruppenzentrum() {
        let mut s = AppState::new();
        // Zwei Rechtecke nebeneinander; Gruppen-Zentrum liegt mittig dazwischen.
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        s.add_shape(Geo::Rect {
            x: 30.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        s.selected = vec![0, 1];
        let before = s.selection_bbox().unwrap().center();
        s.rotate_selection(90.0);
        // Zentrum der Gruppe bleibt bei 90°-Drehung erhalten.
        let after = s.selection_bbox().unwrap().center();
        assert!((before.0 - after.0).abs() < 1e-6);
        assert!((before.1 - after.1).abs() < 1e-6);
        // Jede Form trägt jetzt die 90°-Eigenrotation.
        assert!((s.shapes[0].rotation - 90.0).abs() < 1e-6);
        assert!((s.shapes[1].rotation - 90.0).abs() < 1e-6);
    }

    #[test]
    fn rotate_selection_einzelform_nur_eigenrotation() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 20.0,
            h: 10.0,
        });
        s.selected = vec![0];
        let before = s.selection_bbox().unwrap().center();
        s.rotate_selection(45.0);
        let after = s.selection_bbox().unwrap().center();
        // Einzelform dreht um ihr eigenes Zentrum → Zentrum unverändert.
        assert!((before.0 - after.0).abs() < 1e-6);
        assert!((before.1 - after.1).abs() < 1e-6);
        assert!((s.shapes[0].rotation - 45.0).abs() < 1e-6);
    }

    #[test]
    fn marquee_waehlt_nur_vollstaendig_umschlossene() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 10.0,
            y: 10.0,
            w: 20.0,
            h: 20.0,
        }); // ganz drin
        s.add_shape(Geo::Rect {
            x: 200.0,
            y: 200.0,
            w: 20.0,
            h: 20.0,
        }); // draußen
        s.select_in_rect(0.0, 0.0, 100.0, 100.0);
        assert_eq!(s.selected, vec![0]);
    }
}
