//! Maus-Gesten des Canvas: Auswahl/Move/Resize/Rotate/Marquee, Aufzieh-Formen
//! und der punktbasierte Zug. Methoden auf [`CanvasState`], die zusätzlich die
//! [`EditorSession`] mutieren — die Fach-Wahrheit bleibt im Core.
//!
//! Rückgabe `bool` = „ein Shape wurde erzeugt". Der Root frischt dann die
//! aktive Zeichenfarbe auf; das Setzen von `App.accent` bleibt Root-Sache.

use luxifer_application::{BoxShape, EditorSession, PointPath};

use crate::tools::{Drag, Tool};

use super::state::CanvasState;

/// Ergebnis eines Maus-Events, das der Root weiterverarbeitet.
#[derive(Default)]
pub struct PointerOutcome {
    /// Ein Shape entstand (Aufzieh-Werkzeug losgelassen) → Accent auffrischen.
    pub shape_added: bool,
    /// Doppelklick traf diesen Shape-Index (Auswahlwerkzeug) → Editor öffnen.
    pub double_clicked: Option<usize>,
}

impl CanvasState {
    /// Cursor für die aktuelle Canvas-Position. Nutzt dieselben Fangzonen wie
    /// `begin_select`, damit sichtbares Signal und folgende Aktion nicht
    /// auseinanderlaufen.
    pub fn hover_cursor(&self, session: &EditorSession) -> egui::CursorIcon {
        match self.drag {
            Drag::Pan | Drag::MoveShapes { .. } => return egui::CursorIcon::Grabbing,
            Drag::Resize { handle, .. } => return resize_cursor(handle),
            Drag::Rotate { .. } => return egui::CursorIcon::Crosshair,
            _ => {}
        }
        if self.space_down {
            return egui::CursorIcon::Grab;
        }
        if self.tool != Tool::Select {
            return egui::CursorIcon::Crosshair;
        }

        let world = self.world();
        if let Some(bbox) = self.editable_selection_bbox(session) {
            let pick = super::overlay::handle_hw(self.cam.scale) as f64 * 1.8;
            let rotate = super::overlay::rotate_handle_pos(&bbox, self.cam.scale);
            if (world[0] - rotate[0]).hypot(world[1] - rotate[1]) <= pick {
                return egui::CursorIcon::Crosshair;
            }
            for (handle, (x, y)) in luxifer_core::Handle::positions(&bbox) {
                if (world[0] - x).abs() <= pick && (world[1] - y).abs() <= pick {
                    return resize_cursor(handle);
                }
            }
        }

        let tolerance = 4.0 / self.cam.scale as f64;
        if session.hit_test(world[0], world[1], tolerance).is_some() {
            egui::CursorIcon::Grab
        } else {
            egui::CursorIcon::Default
        }
    }

    fn editable_selection_bbox(&self, session: &EditorSession) -> Option<luxifer_core::BBox> {
        let editable = self.laser_editable_layers.as_ref().is_none_or(|allowed| {
            session.selected.iter().all(|&index| {
                session
                    .shapes
                    .get(index)
                    .is_some_and(|shape| allowed.contains(&shape.layer_id))
            })
        });
        editable.then(|| session.selection_bbox()).flatten()
    }

    /// Maustaste gedrückt/losgelassen. Liefert, ob ein Shape entstand und ob ein
    /// Doppelklick einen Shape traf.
    pub fn on_mouse(
        &mut self,
        session: &mut EditorSession,
        button: winit::event::MouseButton,
        pressed: bool,
    ) -> PointerOutcome {
        use winit::event::MouseButton;
        let mut out = PointerOutcome::default();
        let w = self.world();
        match button {
            MouseButton::Middle => {
                self.drag = if pressed { Drag::Pan } else { Drag::None };
            }
            MouseButton::Left if pressed => {
                if self.space_down {
                    self.drag = Drag::Pan;
                    return out;
                }
                // Doppelklick im Auswahl-Werkzeug auf einen Shape → Editor öffnen.
                if matches!(self.tool, Tool::Select) && self.is_double_click(w) {
                    let tol = 4.0 / self.cam.scale as f64;
                    out.double_clicked = session.hit_test(w[0], w[1], tol);
                    if out.double_clicked.is_some() {
                        return out;
                    }
                }
                if self.near_point_path_start(w) {
                    out.shape_added = self.finish_point_path(session, true);
                    return out;
                }
                match self.tool {
                    Tool::Select | Tool::Node => self.begin_select(session, w),
                    // Aufzieh-Werkzeuge (Zentrum/Ecke → Maus).
                    Tool::Rect | Tool::Ellipse | Tool::Polygon | Tool::Line | Tool::Measure => {
                        self.drag = Drag::DrawBox { start: w }
                    }
                    // Punkt-für-Punkt-Werkzeuge sammeln in poly_pts.
                    Tool::Polyline | Tool::Spline => self.poly_pts.push((w[0], w[1])),
                    // Bézier-Feder: Drücken setzt den Anker, Ziehen formt eine
                    // symmetrische Tangente für Ein- und Ausgang.
                    Tool::Bezier => {
                        let node = self.bezier_nodes.len();
                        self.bezier_nodes
                            .push(luxifer_core::bezier::BezierNode::corner((w[0], w[1])));
                        self.drag = Drag::BezierHandle { node };
                    }
                }
            }
            MouseButton::Left => {
                // Loslassen: Zug abschließen.
                out.shape_added = self.finish_drag(session, w);
            }
            _ => {}
        }
        out
    }

    /// Bildschirmkonstante Fangzone am ersten Knoten. Mindestens drei Knoten
    /// verhindern, dass der zweite Klick versehentlich sofort schließt.
    fn near_point_path_start(&self, w: [f64; 2]) -> bool {
        let first = match self.tool {
            Tool::Bezier if self.bezier_nodes.len() >= 3 => self.bezier_nodes.first().map(|n| n.p),
            Tool::Polyline | Tool::Spline if self.poly_pts.len() >= 3 => {
                self.poly_pts.first().copied()
            }
            _ => None,
        };
        first.is_some_and(|p| (p.0 - w[0]).hypot(p.1 - w[1]) <= 10.0 / self.cam.scale as f64)
    }

    /// Kopie der aktuell selektierten Shapes (Index + Shape) — als Ausgangspunkt
    /// für Resize/Rotate, damit vom Startzustand statt inkrementell gerechnet wird.
    fn snapshot_selection(session: &EditorSession) -> Vec<(usize, luxifer_core::Shape)> {
        session
            .selected
            .iter()
            .filter_map(|&i| session.shapes.get(i).map(|s| (i, s.clone())))
            .collect()
    }

    /// Stellt die Shapes aus einem Snapshot wieder her (vor jeder Transformation).
    fn restore_snapshot(session: &mut EditorSession, orig: &[(usize, luxifer_core::Shape)]) {
        for (i, s) in orig {
            if let Some(dst) = session.shapes.get_mut(*i) {
                *dst = s.clone();
            }
        }
    }

    fn begin_select(&mut self, session: &mut EditorSession, w: [f64; 2]) {
        let selection_editable =
            |session: &EditorSession, allowed: &Option<std::collections::HashSet<usize>>| {
                allowed.as_ref().is_none_or(|set| {
                    session.selected.iter().all(|&i| {
                        session
                            .shapes
                            .get(i)
                            .is_some_and(|shape| set.contains(&shape.layer_id))
                    })
                })
            };
        // Zuerst: wurde ein Transform-Handle der aktuellen Auswahl getroffen?
        let editable_bbox = selection_editable(session, &self.laser_editable_layers)
            .then(|| session.selection_bbox())
            .flatten();
        if let Some(b) = editable_bbox {
            // etwas großzügiger als sichtbar; Handle-Geometrie aus canvas::overlay.
            let pick = super::overlay::handle_hw(self.cam.scale) as f64 * 1.8;
            // Rotate-Handle?
            let rp = super::overlay::rotate_handle_pos(&b, self.cam.scale);
            if (w[0] - rp[0]).hypot(w[1] - rp[1]) <= pick {
                session.begin_edit();
                let pivot = [b.x + b.w / 2.0, b.y + b.h / 2.0];
                let angle = (w[1] - pivot[1]).atan2(w[0] - pivot[0]);
                self.drag = Drag::Rotate {
                    pivot,
                    start_angle: angle,
                    orig: Self::snapshot_selection(session),
                };
                return;
            }
            // Skalier-Handle?
            for (handle, (hx, hy)) in luxifer_core::Handle::positions(&b) {
                if (w[0] - hx).abs() <= pick && (w[1] - hy).abs() <= pick {
                    session.begin_edit();
                    self.drag = Drag::Resize {
                        handle,
                        start_box: b,
                        orig: Self::snapshot_selection(session),
                    };
                    return;
                }
            }
        }

        let tol = 4.0 / self.cam.scale as f64;
        let hit = session.select_at(w[0], w[1], tol, self.shift_down);
        if self.shift_down {
            self.drag = Drag::None;
        } else if hit.is_some() && selection_editable(session, &self.laser_editable_layers) {
            session.begin_edit();
            self.drag = Drag::MoveShapes { last: w };
        } else {
            self.drag = Drag::Marquee { start: w };
        }
    }

    /// Cursorbewegung auf Fensterpixel `new`. Aktualisiert laufende Gesten und
    /// setzt am Ende den Cursor.
    pub fn on_cursor_move(&mut self, session: &mut EditorSession, new: [f32; 2]) {
        let dx = new[0] - self.cursor[0];
        let dy = new[1] - self.cursor[1];
        let w = self.cam.screen_to_world(new);
        // Erst die reinen Kamera-/Move-Fälle (kein Snapshot nötig).
        match &mut self.drag {
            Drag::Pan => {
                self.cam.pan_pixels(dx, dy);
                self.cursor = new;
                return;
            }
            Drag::MoveShapes { last } => {
                let last = *last;
                self.drag = Drag::MoveShapes { last: w };
                session.translate_edit(w[0] - last[0], w[1] - last[1]);
                self.cursor = new;
                return;
            }
            Drag::BezierHandle { node } => {
                if let Some(n) = self.bezier_nodes.get_mut(*node) {
                    let dx = w[0] - n.p.0;
                    let dy = w[1] - n.p.1;
                    n.h_out = Some((w[0], w[1]));
                    n.h_in = Some((n.p.0 - dx, n.p.1 - dy));
                }
                self.cursor = new;
                return;
            }
            _ => {}
        }
        // Resize/Rotate: immer vom Snapshot (Ausgangszustand) rechnen, damit sich
        // die Transformation nicht Schritt für Schritt aufschaukelt.
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Resize {
                handle,
                start_box,
                orig,
            } => {
                Self::restore_snapshot(session, &orig);
                let mut target = luxifer_core::resize_to_cursor(start_box, handle, w);
                // Eck-Handles halten standardmäßig das Seitenverhältnis; Shift
                // löst es (frei). Kanten-Handles skalieren nur eine Achse.
                if handle.is_corner() && !self.shift_down {
                    target = luxifer_core::keep_aspect(start_box, handle, target);
                }
                session.scale_edit(start_box, target);
                self.drag = Drag::Resize {
                    handle,
                    start_box,
                    orig,
                };
            }
            Drag::Rotate {
                pivot,
                start_angle,
                orig,
            } => {
                Self::restore_snapshot(session, &orig);
                let a = (w[1] - pivot[1]).atan2(w[0] - pivot[0]);
                let delta_deg = (a - start_angle).to_degrees();
                session.rotate_edit(delta_deg);
                self.drag = Drag::Rotate {
                    pivot,
                    start_angle,
                    orig,
                };
            }
            other => self.drag = other,
        }
        self.cursor = new;
    }

    /// Schließt die laufende Geste beim Loslassen ab. Gibt true zurück, wenn
    /// dabei ein Shape entstand.
    fn finish_drag(&mut self, session: &mut EditorSession, w: [f64; 2]) -> bool {
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Marquee { start } => {
                if (start[0] - w[0]).abs() > 1.0 || (start[1] - w[1]).abs() > 1.0 {
                    session.select_rect(start, w);
                }
                false
            }
            Drag::DrawBox { start } => self.finish_box(session, start, w),
            Drag::BezierHandle { .. } => false,
            Drag::MoveShapes { .. } | Drag::Resize { .. } | Drag::Rotate { .. } => {
                session.commit_edit();
                false
            }
            _ => false,
        }
    }

    /// Schließt ein Aufzieh-Werkzeug ab. Gibt true zurück, wenn ein Shape entstand.
    fn finish_box(&mut self, session: &mut EditorSession, a: [f64; 2], b: [f64; 2]) -> bool {
        // Messen: nichts erzeugen (nur Anzeige während des Ziehens).
        if self.tool == Tool::Measure {
            return false;
        }
        // Polygon: Form vom Zentrum `a` mit Radius = Abstand zur Maus aufziehen.
        if self.tool == Tool::Polygon {
            return session.add_polygon(self.active_shape, a, b).is_some();
        }
        // Linie: 2-Punkt-Polyline (auch bei kleinem Zug erlaubt).
        if self.tool == Tool::Line {
            return session.add_line(a, b).is_some();
        }
        let shape = match self.tool {
            Tool::Ellipse => BoxShape::Ellipse,
            _ => BoxShape::Rect,
        };
        session.add_box_shape(shape, a, b).is_some()
    }

    /// Schließt den punktbasierten Zug ab. `closed` bestimmt, ob Core und
    /// Application auch die Schlusskante erzeugen. Gibt true zurück, wenn ein
    /// Shape entstand.
    pub fn finish_point_path(&mut self, session: &mut EditorSession, closed: bool) -> bool {
        if self.tool == Tool::Bezier {
            self.poly_pts.clear();
            let nodes = std::mem::take(&mut self.bezier_nodes);
            return session.add_bezier_nodes(nodes, closed).is_some();
        }
        let pts = std::mem::take(&mut self.poly_pts);
        let path = match self.tool {
            Tool::Polyline => PointPath::Polyline,
            Tool::Spline => PointPath::Spline,
            Tool::Bezier => unreachable!("Bézier-Knoten wurden bereits behandelt"),
            _ => return false,
        };
        session.add_point_path(path, pts, closed).is_some()
    }
}

fn resize_cursor(handle: luxifer_core::Handle) -> egui::CursorIcon {
    use luxifer_core::Handle;
    match handle {
        Handle::N | Handle::S => egui::CursorIcon::ResizeVertical,
        Handle::E | Handle::W => egui::CursorIcon::ResizeHorizontal,
        Handle::Nw | Handle::Se => egui::CursorIcon::ResizeNwSe,
        Handle::Ne | Handle::Sw => egui::CursorIcon::ResizeNeSw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;
    use winit::event::MouseButton;

    #[test]
    fn bezier_drag_erzeugt_symmetrische_tangenten_und_fertigen_pfad() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Bezier;
        let mut session = EditorSession::default();

        canvas.cursor = canvas.cam.world_to_screen([10.0, 10.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([15.0, 12.0]));
        canvas.on_mouse(&mut session, MouseButton::Left, false);

        canvas.cursor = canvas.cam.world_to_screen([30.0, 20.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_mouse(&mut session, MouseButton::Left, false);

        assert_eq!(canvas.bezier_nodes[0].h_out, Some((15.0, 12.0)));
        assert_eq!(canvas.bezier_nodes[0].h_in, Some((5.0, 8.0)));
        assert!(canvas.finish_point_path(&mut session, true));
        assert!(canvas.bezier_nodes.is_empty());
        assert_eq!(session.shapes.len(), 1);
        assert_eq!(
            session.shapes[0].bezier.as_ref().unwrap().nodes[0].h_out,
            Some((15.0, 12.0))
        );
        assert!(session.shapes[0].bezier.as_ref().unwrap().closed);
    }

    #[test]
    fn klick_nahe_start_schliesst_punktpfad_ohne_zusaetzlichen_knoten() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Polyline;
        canvas.poly_pts = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0)];
        let mut session = EditorSession::default();
        canvas.cursor = canvas.cam.world_to_screen([1.0, 1.0]);

        let out = canvas.on_mouse(&mut session, MouseButton::Left, true);

        assert!(out.shape_added);
        assert!(canvas.poly_pts.is_empty());
        let luxifer_core::Geo::Polyline { pts, closed } = &session.shapes[0].geo else {
            panic!("erwartete Polyline");
        };
        assert!(*closed);
        assert_eq!(pts.len(), 3);
    }

    #[test]
    fn laser_policy_erlaubt_transformation_nur_nach_temporaerer_freigabe() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Select;
        canvas.laser_editable_layers = Some(Default::default());
        let mut session = EditorSession::default();
        session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [100.0, 50.0]);
        let before = session.shapes[0].bbox();

        canvas.cursor = canvas.cam.world_to_screen([0.0, 0.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        assert!(!matches!(
            canvas.drag,
            Drag::MoveShapes { .. } | Drag::Resize { .. } | Drag::Rotate { .. }
        ));
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([-10.0, -5.0]));
        canvas.on_mouse(&mut session, MouseButton::Left, false);
        assert_eq!(session.shapes[0].bbox(), before);

        canvas.laser_editable_layers.as_mut().unwrap().insert(0);
        assert!(canvas.laser_editable_layers.as_ref().unwrap().contains(&0));
    }
}
