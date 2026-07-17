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

    /// Ist der Handle eine Ecke (skaliert beide Achsen)?
    pub fn is_corner(&self) -> bool {
        matches!(self, Handle::Nw | Handle::Ne | Handle::Sw | Handle::Se)
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

/// Ziel-Box beim Ziehen eines Skalier-Handles auf die absolute Cursor-Position
/// `cursor` (mm): die vom Handle bewegte(n) Kante(n) folgen dem Cursor, die
/// gegenüberliegenden bleiben fix. Negative Größen werden normalisiert (Box
/// klappt sauber um). Eine Mindestgröße von 0.1 mm verhindert eine Nullbox.
pub fn resize_to_cursor(start: BBox, handle: Handle, cursor: [f64; 2]) -> BBox {
    let mut left = start.x;
    let mut right = start.x + start.w;
    let mut top = start.y;
    let mut bottom = start.y + start.h;
    if handle.moves_left() {
        left = cursor[0];
    }
    if handle.moves_right() {
        right = cursor[0];
    }
    if handle.moves_top() {
        top = cursor[1];
    }
    if handle.moves_bottom() {
        bottom = cursor[1];
    }
    let x = left.min(right);
    let y = top.min(bottom);
    BBox::new(
        x,
        y,
        (right - left).abs().max(0.1),
        (bottom - top).abs().max(0.1),
    )
}

/// Zwingt die Ziel-Box aufs Start-Seitenverhältnis (proportionales Skalieren an
/// Ecken). Der Faktor ist der betragsgrößere der beiden Achsen — so folgt die
/// Ecke der Maus großzügig; die gegenüberliegende Ecke (fix bei
/// `resize_to_cursor`) bleibt der Anker.
pub fn keep_aspect(start: BBox, handle: Handle, target: BBox) -> BBox {
    if start.w <= 0.0 || start.h <= 0.0 {
        return target;
    }
    let fx = target.w / start.w;
    let fy = target.h / start.h;
    let f = if fx.abs() > fy.abs() { fx } else { fy };
    let w = start.w * f;
    let h = start.h * f;
    // Anker = die gegenüberliegende Ecke (bleibt fix). x/y so, dass der Anker hält.
    let anchor_x = if handle.moves_left() {
        start.x + start.w
    } else {
        start.x
    };
    let anchor_y = if handle.moves_top() {
        start.y + start.h
    } else {
        start.y
    };
    let x = if handle.moves_left() {
        anchor_x - w
    } else {
        anchor_x
    };
    let y = if handle.moves_top() {
        anchor_y - h
    } else {
        anchor_y
    };
    BBox::new(
        x.min(anchor_x),
        y.min(anchor_y),
        w.abs().max(0.1),
        h.abs().max(0.1),
    )
}

impl AppState {
    /// Gemeinsame Bounding-Box der aktuellen Auswahl (mm), falls vorhanden.
    pub fn selection_bbox(&self) -> Option<BBox> {
        BBox::union_all(
            self.selected
                .iter()
                .filter_map(|&i| self.shape_bbox_cached(i)),
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
        // Geänderte Geometrie macht abgeleitete Shape-Bounds ungültig.
        self.invalidate_shape_bounds();
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
        self.rotate_selection_around(pivot, degrees);
    }

    /// Dreht um einen vom Gestenbeginn stabil gehaltenen Pivot. Direkte
    /// Manipulation darf den Mittelpunkt nicht aus der während der Drehung
    /// wechselnden achsparallelen Auswahlbox neu ableiten; besonders
    /// asymmetrische Polygone würden sonst zwischen Frames versetzt werden.
    pub fn rotate_selection_around(&mut self, pivot: (f64, f64), degrees: f64) {
        if degrees == 0.0 {
            return;
        }
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
        // Die Pivot-Abfrage oben hat den Bounds-Cache für den alten Zustand
        // aufgebaut. Nach der Rotation muss Scene die neue Auswahlbox erhalten.
        self.invalidate_shape_bounds();
        self.dirty = true;
    }

    /// Richtungsabhängige Marquee-Auswahl:
    ///
    /// - rechts → links: nur vollständig umschlossene Shapes;
    /// - links → rechts: alle Shapes, deren Bounding-Box den Kasten berührt.
    ///
    /// Unsichtbare/gesperrte Layer werden übersprungen, die Auswahl ersetzt.
    pub fn select_in_rect(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, crossing: bool) {
        let (lo_x, hi_x) = (x1.min(x2), x1.max(x2));
        let (lo_y, hi_y) = (y1.min(y2), y1.max(y2));
        self.selected.clear();
        for (i, s) in self.shapes.iter().enumerate() {
            if let Some(l) = self.layers.get(s.layer_id) {
                if !l.visible || l.locked || (s.fill_only && !l.mode.is_filled()) {
                    continue;
                }
            }
            let b = s.bbox();
            let contained = b.x >= lo_x && b.y >= lo_y && b.x + b.w <= hi_x && b.y + b.h <= hi_y;
            let intersects = b.x <= hi_x && b.x + b.w >= lo_x && b.y <= hi_y && b.y + b.h >= lo_y;
            if contained || (crossing && intersects) {
                self.selected.push(i);
            }
        }
    }
}

/// Liefert den aktiven Marquee-Modus aus Zugrichtung und Benutzerpräferenz.
pub fn marquee_crossing(start_x: f64, end_x: f64, inverted: bool) -> bool {
    (end_x > start_x) ^ inverted
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
    fn resize_to_cursor_se_zieht_rechte_untere_ecke() {
        let start = BBox::new(0.0, 0.0, 100.0, 100.0);
        // Se-Handle auf (150,120) ziehen: Ursprung bleibt, Box wird 150×120.
        let t = resize_to_cursor(start, Handle::Se, [150.0, 120.0]);
        assert!((t.x - 0.0).abs() < 1e-9 && (t.y - 0.0).abs() < 1e-9);
        assert!((t.w - 150.0).abs() < 1e-9 && (t.h - 120.0).abs() < 1e-9);
    }

    #[test]
    fn resize_to_cursor_nw_haelt_gegenueberliegende_ecke_fix() {
        let start = BBox::new(0.0, 0.0, 100.0, 100.0);
        // Nw auf (20,30): rechte-untere Ecke (100,100) bleibt fix.
        let t = resize_to_cursor(start, Handle::Nw, [20.0, 30.0]);
        assert!((t.x - 20.0).abs() < 1e-9 && (t.y - 30.0).abs() < 1e-9);
        assert!((t.x + t.w - 100.0).abs() < 1e-9);
        assert!((t.y + t.h - 100.0).abs() < 1e-9);
    }

    #[test]
    fn resize_to_cursor_e_aendert_nur_breite() {
        let start = BBox::new(10.0, 10.0, 50.0, 50.0);
        let t = resize_to_cursor(start, Handle::E, [200.0, 999.0]);
        assert!((t.y - 10.0).abs() < 1e-9 && (t.h - 50.0).abs() < 1e-9);
        assert!((t.x + t.w - 200.0).abs() < 1e-9);
    }

    #[test]
    fn keep_aspect_haelt_verhaeltnis_und_anker() {
        let start = BBox::new(0.0, 0.0, 100.0, 50.0); // 2:1
                                                      // SE weit nach rechts (Höhe zieht wenig): frei wäre 300×60.
        let free = resize_to_cursor(start, Handle::Se, [300.0, 60.0]);
        let kept = keep_aspect(start, Handle::Se, free);
        // Verhältnis muss 2:1 bleiben.
        assert!(
            (kept.w / kept.h - 2.0).abs() < 1e-6,
            "Verhältnis {}",
            kept.w / kept.h
        );
        // SE: obere-linke Ecke (0,0) bleibt Anker.
        assert!((kept.x - 0.0).abs() < 1e-6 && (kept.y - 0.0).abs() < 1e-6);
    }

    #[test]
    fn keep_aspect_nw_haelt_gegenecke() {
        let start = BBox::new(0.0, 0.0, 100.0, 50.0);
        // NW nach oben-links: Anker ist die untere-rechte Ecke (100,50).
        let free = resize_to_cursor(start, Handle::Nw, [-100.0, -20.0]);
        let kept = keep_aspect(start, Handle::Nw, free);
        assert!((kept.w / kept.h - 2.0).abs() < 1e-6);
        assert!(
            (kept.x + kept.w - 100.0).abs() < 1e-6,
            "rechte Kante {}",
            kept.x + kept.w
        );
        assert!(
            (kept.y + kept.h - 50.0).abs() < 1e-6,
            "untere Kante {}",
            kept.y + kept.h
        );
    }

    #[test]
    fn is_corner_erkennt_ecken() {
        assert!(Handle::Se.is_corner() && Handle::Nw.is_corner());
        assert!(!Handle::N.is_corner() && !Handle::E.is_corner());
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
    fn selection_bbox_folgt_verschiebung() {
        // Regression: translate_selected muss den Bounds-Cache invalidieren,
        // sonst driftet die Auswahl-BBox vom Shape weg.
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 10.0,
            y: 10.0,
            w: 20.0,
            h: 20.0,
        });
        s.selected = vec![0];
        // BBox einmal lesen → füllt den Cache am alten Ort.
        let before = s.selection_bbox().unwrap();
        assert_eq!((before.x, before.y), (10.0, 10.0));
        // Verschieben und erneut lesen — muss mitgewandert sein.
        s.translate_selected(100.0, 50.0);
        let after = s.selection_bbox().unwrap();
        assert_eq!(
            (after.x, after.y, after.w, after.h),
            (110.0, 60.0, 20.0, 20.0)
        );
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
        let after_box = s.selection_bbox().unwrap();
        let after = after_box.center();
        assert!((before.0 - after.0).abs() < 1e-6);
        assert!((before.1 - after.1).abs() < 1e-6);
        // Regression: Die nach rotate_selection zurückgegebene Box muss bereits
        // den gedrehten Zustand spiegeln, nicht erst nach erneuter Auswahl.
        assert!((after_box.w - 10.0).abs() < 1e-6);
        assert!((after_box.h - 40.0).abs() < 1e-6);
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
    fn asymmetrisches_polygon_dreht_mit_stabilem_gestenpivot() {
        let mut s = AppState::new();
        s.add_shape(Geo::Polyline {
            pts: vec![(10.0, 10.0), (80.0, 20.0), (45.0, 90.0), (20.0, 55.0)],
            closed: true,
        });
        s.selected = vec![0];
        let original = s.shapes[0].clone();
        let pivot = s.selection_bbox().unwrap().center();

        for angle in [8.0, 17.0, 31.0, 46.0, 73.0] {
            s.shapes[0] = original.clone();
            s.rotate_selection_around(pivot, angle);
            assert_eq!(s.shapes[0].geo.bbox().center(), pivot);
            assert!((s.shapes[0].rotation - angle).abs() < 1e-9);
        }
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
        s.select_in_rect(100.0, 0.0, 0.0, 100.0, false);
        assert_eq!(s.selected, vec![0]);
    }

    #[test]
    fn marquee_von_links_nach_rechts_waehlt_auch_schneidende_shapes() {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 10.0,
            y: 10.0,
            w: 20.0,
            h: 20.0,
        });
        s.add_shape(Geo::Rect {
            x: 80.0,
            y: 40.0,
            w: 50.0,
            h: 20.0,
        });
        s.add_shape(Geo::Rect {
            x: 150.0,
            y: 40.0,
            w: 20.0,
            h: 20.0,
        });

        s.select_in_rect(0.0, 0.0, 100.0, 100.0, true);
        assert_eq!(s.selected, vec![0, 1]);
    }

    #[test]
    fn marquee_richtung_kann_invertiert_werden() {
        assert!(marquee_crossing(0.0, 10.0, false));
        assert!(!marquee_crossing(10.0, 0.0, false));
        assert!(!marquee_crossing(0.0, 10.0, true));
        assert!(marquee_crossing(10.0, 0.0, true));
    }
}
