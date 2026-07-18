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
    pub error: Option<luxifer_application::AppError>,
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
        if self.tool == Tool::Trim {
            // Nach dem egui-Frame ersetzt App diesen sichtbaren Standardcursor
            // durch den nativen Bitmap-Scheren-Cursor.
            return egui::CursorIcon::Default;
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
                // Im Knotenwerkzeug teilt ein Doppelklick genau das getroffene
                // Segment. Der Core liefert auch bei Kurven/Rotation das exakte t.
                if self.tool == Tool::Node && self.is_double_click(w) {
                    let tolerance = 6.0 / self.cam.scale as f64;
                    if let Some(hit) = session.hit_bezier_segment((w[0], w[1]), tolerance) {
                        session.begin_edit();
                        session.split_node_segment(hit.shape, hit.segment, hit.t);
                        session.commit_edit();
                        return out;
                    }
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
                    Tool::Select => self.begin_select(session, w),
                    Tool::Node => self.begin_node_edit(session, w),
                    Tool::Trim => {
                        let tolerance = 6.0 / self.cam.scale as f64;
                        session.begin_edit();
                        session.trim_edit((w[0], w[1]), tolerance);
                        self.drag = Drag::TrimStroke { last_trim: w };
                        self.trim_preview = session
                            .state()
                            .trim_preview((w[0], w[1]), tolerance)
                            .map(|preview| preview.removed);
                    }
                    // Aufzieh-Werkzeuge (Zentrum/Ecke → Maus).
                    Tool::Rect | Tool::Ellipse | Tool::Polygon | Tool::Line | Tool::Measure => {
                        self.drag = Drag::DrawBox { start: w }
                    }
                    // Haltesteg: nahe einem Endpunkt → diesen nachfassen,
                    // sonst neue Steg-Linie beginnen (ersetzt den Entwurf).
                    Tool::Bridge => {
                        let grab = 10.0 / self.cam.scale as f64;
                        let near = |p: [f64; 2]| (p[0] - w[0]).hypot(p[1] - w[1]) <= grab;
                        match self.bridge {
                            Some(d) if near(d.p0) => self.drag = Drag::BridgeEnd { end: 0 },
                            Some(d) if near(d.p1) => self.drag = Drag::BridgeEnd { end: 1 },
                            _ => {
                                self.bridge = Some(super::state::BridgeDraft {
                                    p0: w,
                                    p1: w,
                                    width: self.bridge_width,
                                });
                                self.drag = Drag::BridgeEnd { end: 1 };
                            }
                        }
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
            MouseButton::Right if self.right_select_active && pressed => {
                self.begin_select(session, w);
            }
            MouseButton::Right if self.right_select_active => {
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

    fn selection_can_gpu_transform(session: &EditorSession) -> bool {
        let is_visible_fill = |shape: &luxifer_core::Shape| {
            shape.geo.outline_points().1
                && session.layers.get(shape.layer_id).is_some_and(|layer| {
                    layer.visible
                        && layer.mode.is_filled()
                        && layer.mode != luxifer_core::LayerMode::Image
                })
        };
        let selected_compounds_complete = session.selected.iter().all(|&selected_index| {
            let Some(selected_shape) = session.shapes.get(selected_index) else {
                return false;
            };
            if !is_visible_fill(selected_shape) {
                return true;
            }
            let Some(group_id) = selected_shape.fill_group_id.or(selected_shape.group_id) else {
                return true;
            };
            session.shapes.iter().enumerate().all(|(index, shape)| {
                shape.layer_id != selected_shape.layer_id
                    || shape.fill_group_id.or(shape.group_id) != Some(group_id)
                    || session.selected.contains(&index)
            })
        });
        !session.selected.is_empty()
            && session
                .selected
                .iter()
                .all(|&i| session.shapes.get(i).is_some())
            && selected_compounds_complete
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
                let gpu_live = Self::selection_can_gpu_transform(session);
                if !gpu_live {
                    session.begin_edit();
                }
                let pivot = [b.x + b.w / 2.0, b.y + b.h / 2.0];
                let angle = (w[1] - pivot[1]).atan2(w[0] - pivot[0]);
                self.drag = Drag::Rotate {
                    pivot,
                    start_angle: angle,
                    orig: gpu_live
                        .then(Vec::new)
                        .unwrap_or_else(|| Self::snapshot_selection(session)),
                    start_box: b,
                    delta_deg: 0.0,
                    gpu_live,
                };
                return;
            }
            // Skalier-Handle?
            for (handle, (hx, hy)) in luxifer_core::Handle::positions(&b) {
                if (w[0] - hx).abs() <= pick && (w[1] - hy).abs() <= pick {
                    let gpu_live = Self::selection_can_gpu_transform(session);
                    if !gpu_live {
                        session.begin_edit();
                    }
                    self.drag = Drag::Resize {
                        handle,
                        start_box: b,
                        orig: gpu_live
                            .then(Vec::new)
                            .unwrap_or_else(|| Self::snapshot_selection(session)),
                        target_box: b,
                        gpu_live,
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
            let gpu_live = Self::selection_can_gpu_transform(session);
            if !gpu_live {
                session.begin_edit();
            }
            self.drag = Drag::MoveShapes {
                start: w,
                last: w,
                gpu_live,
            };
        } else {
            self.drag = Drag::Marquee { start: w };
        }
    }

    fn begin_node_edit(&mut self, session: &mut EditorSession, w: [f64; 2]) {
        let pick = 8.0 / self.cam.scale as f64;
        for &shape_idx in session.selected.iter().rev() {
            let Some(shape) = session.shapes.get(shape_idx) else {
                continue;
            };
            let pivot = shape.geo.bbox().center();
            let to_world = |p: (f64, f64)| {
                if shape.rotation.abs() > f64::EPSILON {
                    luxifer_core::geometry::rotate_point(p.0, p.1, pivot.0, pivot.1, shape.rotation)
                } else {
                    p
                }
            };
            let fallback;
            let nodes = if let Some(path) = &shape.bezier {
                &path.nodes
            } else if !matches!(
                shape.geo,
                luxifer_core::Geo::Image { .. } | luxifer_core::Geo::Ellipse { .. }
            ) {
                fallback = shape
                    .geo
                    .outline_points()
                    .0
                    .into_iter()
                    .map(luxifer_core::bezier::BezierNode::corner)
                    .collect::<Vec<_>>();
                &fallback
            } else {
                continue;
            };
            for (node_idx, node) in nodes.iter().enumerate().rev() {
                for (part, point) in [
                    (luxifer_core::bezier::NodePart::HandleIn, node.h_in),
                    (luxifer_core::bezier::NodePart::HandleOut, node.h_out),
                    (luxifer_core::bezier::NodePart::Anchor, Some(node.p)),
                ] {
                    let Some(point) = point else { continue };
                    let point = to_world(point);
                    if (point.0 - w[0]).hypot(point.1 - w[1]) <= pick {
                        session.begin_edit();
                        self.clear_double_click_candidate();
                        self.drag = Drag::EditNode {
                            shape: shape_idx,
                            node: node_idx,
                            part,
                        };
                        return;
                    }
                }
            }
        }

        // Ein Klick auf eine andere Kontur macht sie zum Ziel des Node-Tools,
        // verschiebt sie aber nicht wie das Auswahlwerkzeug.
        session.select_at(w[0], w[1], 4.0 / self.cam.scale as f64, false);
        self.drag = Drag::None;
    }

    /// Cursorbewegung auf Fensterpixel `new`. Aktualisiert laufende Gesten und
    /// setzt am Ende den Cursor.
    pub fn on_cursor_move(&mut self, session: &mut EditorSession, new: [f32; 2]) {
        let dx = new[0] - self.cursor[0];
        let dy = new[1] - self.cursor[1];
        let w = self.cam.screen_to_world(new);
        if self.tool == Tool::Trim && matches!(self.drag, Drag::None) {
            let tolerance = 6.0 / self.cam.scale as f64;
            self.trim_preview = session
                .state()
                .trim_preview((w[0], w[1]), tolerance)
                .map(|preview| preview.removed);
        }
        if let Drag::TrimStroke { last_trim } = &self.drag {
            let last_trim = *last_trim;
            // Rund acht Bildschirmpixel Abstand vermeiden, dass ein nach dem
            // Trim neu entstandener Rest im unmittelbar folgenden Maus-Event
            // versehentlich ebenfalls verschwindet.
            let step = 8.0 / self.cam.scale as f64;
            if (w[0] - last_trim[0]).hypot(w[1] - last_trim[1]) >= step {
                let tolerance = 6.0 / self.cam.scale as f64;
                if session.trim_edit((w[0], w[1]), tolerance) {
                    self.drag = Drag::TrimStroke { last_trim: w };
                }
                self.trim_preview = session
                    .state()
                    .trim_preview((w[0], w[1]), tolerance)
                    .map(|preview| preview.removed);
            }
        }
        // Erst die reinen Kamera-/Move-Fälle (kein Snapshot nötig).
        match &mut self.drag {
            Drag::Pan => {
                self.cam.pan_pixels(dx, dy);
                self.cursor = new;
                return;
            }
            Drag::MoveShapes {
                start,
                last,
                gpu_live,
            } => {
                let (start, last, gpu_live) = (*start, *last, *gpu_live);
                self.drag = Drag::MoveShapes {
                    start,
                    last: w,
                    gpu_live,
                };
                if !gpu_live {
                    session.translate_edit(w[0] - last[0], w[1] - last[1]);
                }
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
            Drag::EditNode { shape, node, part } => {
                let (shape, node, part) = (*shape, *node, *part);
                let local = session.shapes.get(shape).map_or((w[0], w[1]), |shape| {
                    if shape.rotation.abs() > f64::EPSILON {
                        let pivot = shape.geo.bbox().center();
                        luxifer_core::geometry::rotate_point(
                            w[0],
                            w[1],
                            pivot.0,
                            pivot.1,
                            -shape.rotation,
                        )
                    } else {
                        (w[0], w[1])
                    }
                });
                session.drag_node(shape, node, part, local);
                self.cursor = new;
                return;
            }
            Drag::BridgeEnd { end } => {
                if let Some(d) = self.bridge.as_mut() {
                    if *end == 0 {
                        d.p0 = w;
                    } else {
                        d.p1 = w;
                    }
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
                gpu_live,
                ..
            } => {
                let mut target = luxifer_core::resize_to_cursor(start_box, handle, w);
                // Eck-Handles halten standardmäßig das Seitenverhältnis; Shift
                // löst es (frei). Kanten-Handles skalieren nur eine Achse.
                if handle.is_corner() && !self.shift_down {
                    target = luxifer_core::keep_aspect(start_box, handle, target);
                }
                if !gpu_live {
                    Self::restore_snapshot(session, &orig);
                    session.scale_edit(start_box, target);
                }
                self.drag = Drag::Resize {
                    handle,
                    start_box,
                    orig,
                    target_box: target,
                    gpu_live,
                };
            }
            Drag::Rotate {
                pivot,
                start_angle,
                orig,
                start_box,
                gpu_live,
                ..
            } => {
                let a = (w[1] - pivot[1]).atan2(w[0] - pivot[0]);
                let delta_deg = (a - start_angle).to_degrees();
                if !gpu_live {
                    Self::restore_snapshot(session, &orig);
                    session.rotate_edit_around(pivot, delta_deg);
                }
                self.drag = Drag::Rotate {
                    pivot,
                    start_angle,
                    orig,
                    start_box,
                    delta_deg,
                    gpu_live,
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
                    session.select_rect(start, w, self.invert_marquee_direction);
                }
                false
            }
            Drag::DrawBox { start } => self.finish_box(session, start, w),
            Drag::BezierHandle { .. } => false,
            // Der Steg-Entwurf bleibt stehen — bestätigt wird über das
            // Eingabefeld am Linienende (App::commit_bridge).
            Drag::BridgeEnd { .. } => false,
            Drag::TrimStroke { .. } => {
                session.commit_edit();
                false
            }
            Drag::MoveShapes {
                start,
                gpu_live: true,
                ..
            } => {
                session.begin_edit();
                session.translate_edit(w[0] - start[0], w[1] - start[1]);
                session.commit_edit();
                false
            }
            Drag::MoveShapes { .. }
            | Drag::Resize {
                gpu_live: false, ..
            }
            | Drag::Rotate {
                gpu_live: false, ..
            }
            | Drag::EditNode { .. } => {
                session.commit_edit();
                false
            }
            Drag::Resize {
                start_box,
                target_box,
                gpu_live: true,
                ..
            } => {
                session.begin_edit();
                session.scale_edit(start_box, target_box);
                session.commit_edit();
                false
            }
            Drag::Rotate {
                pivot,
                delta_deg,
                gpu_live: true,
                ..
            } => {
                session.begin_edit();
                session.rotate_edit_around(pivot, delta_deg);
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
    fn rechte_maustaste_nutzt_auswahl_ohne_zeichenwerkzeug_zu_aendern() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Rect;
        canvas.right_select_active = true;
        let mut session = EditorSession::default();
        session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [40.0, 40.0]);
        session.clear_selection();

        canvas.cursor = canvas.cam.world_to_screen([10.0, 0.0]);
        canvas.on_mouse(&mut session, MouseButton::Right, true);
        canvas.on_mouse(&mut session, MouseButton::Right, false);

        assert_eq!(session.selected, vec![0]);
        assert_eq!(canvas.tool, Tool::Rect);
    }

    #[test]
    fn trim_stroke_entfernt_mehrere_ketten_in_einem_undo_schritt() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Trim;
        let mut session = EditorSession::default();
        session.add_line([0.0, 0.0], [20.0, 0.0]);
        session.add_line([0.0, 30.0], [20.0, 30.0]);
        let before = session.shapes.clone();

        canvas.cursor = canvas.cam.world_to_screen([10.0, 0.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([10.0, 30.0]));
        canvas.on_mouse(&mut session, MouseButton::Left, false);

        assert!(session.shapes.is_empty());
        assert!(session.undo(), "der gesamte Trim-Zug ist ein Undo-Schritt");
        assert_eq!(session.shapes, before);
    }

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
    fn node_doppelklick_teilt_das_getroffene_segment() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Node;
        let mut session = EditorSession::default();
        session.add_line([0.0, 0.0], [20.0, 0.0]);
        canvas.cursor = canvas.cam.world_to_screen([10.0, 0.0]);

        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_mouse(&mut session, MouseButton::Left, false);
        canvas.on_mouse(&mut session, MouseButton::Left, true);

        let nodes = &session.shapes[0].bezier.as_ref().unwrap().nodes;
        assert_eq!(nodes.len(), 3);
        assert!((nodes[1].p.0 - 10.0).abs() < 0.01);
        assert!(nodes[1].p.1.abs() < 0.01);
        assert!(
            session.undo(),
            "Doppelklick erzeugt genau einen Undo-Schritt"
        );
        assert!(session.shapes[0].bezier.is_none());
    }

    #[test]
    fn verschobener_startknoten_blockiert_keine_weitere_bearbeitung() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Node;
        let mut session = EditorSession::default();
        session.add_point_path(
            PointPath::Polyline,
            vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0)],
            true,
        );

        canvas.cursor = canvas.cam.world_to_screen([0.0, 0.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([2.0, 2.0]));
        canvas.on_mouse(&mut session, MouseButton::Left, false);

        // Ein sofortiger weiterer Druck auf den verschobenen Startknoten muss
        // wieder einen Node-Drag beginnen und darf keinen Split auslösen.
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        assert!(matches!(
            canvas.drag,
            Drag::EditNode {
                shape: 0,
                node: 0,
                part: luxifer_core::bezier::NodePart::Anchor
            }
        ));
        assert_eq!(session.shapes[0].bezier.as_ref().unwrap().nodes.len(), 3);
    }

    #[test]
    fn haltesteg_geste_legt_entwurf_an_und_endpunkte_sind_nachfassbar() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Bridge;
        let mut session = EditorSession::default();

        // Ziehen: neue Steg-Linie von (0,10) nach (30,10); nichts wird erzeugt.
        canvas.cursor = canvas.cam.world_to_screen([0.0, 10.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([30.0, 10.0]));
        canvas.on_mouse(&mut session, MouseButton::Left, false);
        let d = canvas.bridge.expect("Entwurf bleibt stehen");
        assert_eq!(d.p0, [0.0, 10.0]);
        assert_eq!(d.p1, [30.0, 10.0]);
        assert!(
            session.shapes.is_empty(),
            "Commit erst über das Eingabefeld"
        );

        // Endpunkt nachfassen: Press nahe p1 zieht ihn statt neu zu beginnen.
        canvas.cursor = canvas.cam.world_to_screen([30.5, 10.5]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([30.0, 25.0]));
        canvas.on_mouse(&mut session, MouseButton::Left, false);
        let d = canvas.bridge.expect("Entwurf bleibt erhalten");
        assert_eq!(d.p0, [0.0, 10.0], "Start bleibt");
        assert_eq!(d.p1, [30.0, 25.0], "Ende folgt der Maus");

        // Press abseits ersetzt den Entwurf durch eine neue Linie.
        canvas.cursor = canvas.cam.world_to_screen([100.0, 100.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_mouse(&mut session, MouseButton::Left, false);
        let d = canvas.bridge.expect("neuer Entwurf");
        assert_eq!(d.p0, [100.0, 100.0]);
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
    fn marquee_kann_im_inneren_eines_ausgewaehlten_vektors_starten() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Select;
        let mut session = EditorSession::default();
        session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [100.0, 100.0]);
        assert_eq!(session.selected, vec![0]);

        canvas.cursor = canvas.cam.world_to_screen([50.0, 50.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);

        assert!(matches!(canvas.drag, Drag::Marquee { .. }));
        assert!(session.selected.is_empty());
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

    #[test]
    fn move_vorschau_mutiert_core_erst_beim_loslassen() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Select;
        let mut session = EditorSession::default();
        session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [100.0, 50.0]);
        let before = session.shapes[0].bbox();
        let rev = session.render_rev();

        canvas.cursor = canvas.cam.world_to_screen([0.0, 20.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([25.0, 30.0]));

        assert_eq!(session.render_rev(), rev);
        assert_eq!(session.shapes[0].bbox(), before);
        assert_eq!(canvas.live_move_offset(), [25.0, 10.0]);

        canvas.on_mouse(&mut session, MouseButton::Left, false);
        assert_eq!(session.shapes[0].bbox().x, before.x + 25.0);
        assert_eq!(session.shapes[0].bbox().y, before.y + 10.0);
        assert!(session.render_rev() > rev);
    }

    #[test]
    fn move_aller_fill_konturen_darf_gpu_live_nutzen() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Select;
        let mut session = EditorSession::default();
        session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [40.0, 40.0]);
        session.add_box_shape(BoxShape::Rect, [60.0, 0.0], [100.0, 40.0]);
        session.layers[0].mode = luxifer_core::LayerMode::Fill;
        session.select_all();
        let rev = session.render_rev();

        canvas.cursor = canvas.cam.world_to_screen([10.0, 0.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([25.0, 5.0]));

        assert_eq!(session.render_rev(), rev);
        assert_eq!(canvas.live_move_offset(), [15.0, 5.0]);
    }

    #[test]
    fn move_eines_unabhaengigen_fill_compounds_darf_gpu_live_nutzen() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Select;
        let mut session = EditorSession::default();
        session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [40.0, 40.0]);
        session.add_box_shape(BoxShape::Rect, [60.0, 0.0], [100.0, 40.0]);
        session.layers[0].mode = luxifer_core::LayerMode::Fill;
        session.selected = vec![0];
        let rev = session.render_rev();
        assert!(CanvasState::selection_can_gpu_transform(&session));

        canvas.cursor = canvas.cam.world_to_screen([10.0, 0.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        assert_eq!(session.selected, vec![0]);
        assert!(matches!(
            canvas.drag,
            Drag::MoveShapes { gpu_live: true, .. }
        ));
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([25.0, 5.0]));

        assert_eq!(session.render_rev(), rev);
        assert_eq!(canvas.live_move_offset(), [15.0, 5.0]);
    }

    #[test]
    fn teilselektion_eines_fill_compounds_bleibt_im_sicheren_pfad() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Select;
        let mut session = EditorSession::default();
        session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [40.0, 40.0]);
        session.add_box_shape(BoxShape::Rect, [10.0, 10.0], [30.0, 30.0]);
        session.layers[0].mode = luxifer_core::LayerMode::Fill;
        session.shapes[0].fill_group_id = Some(1);
        session.shapes[1].fill_group_id = Some(1);
        session.selected = vec![0];
        let rev = session.render_rev();

        canvas.cursor = canvas.cam.world_to_screen([10.0, 0.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([25.0, 5.0]));

        assert!(session.render_rev() > rev);
        assert_eq!(canvas.live_move_offset(), [0.0, 0.0]);
    }

    #[test]
    fn resize_vorschau_skaliert_gpu_und_committet_einmal() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Select;
        let mut session = EditorSession::default();
        session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [100.0, 50.0]);
        let rev = session.render_rev();

        canvas.cursor = canvas.cam.world_to_screen([100.0, 25.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([150.0, 25.0]));

        assert_eq!(session.render_rev(), rev);
        assert_eq!(session.shapes[0].bbox().w, 100.0);
        assert_eq!(canvas.selection_transform().matrix, [1.5, 0.0, 0.0, 1.0]);
        assert_eq!(
            canvas
                .display_selection_bbox(session.selection_bbox())
                .unwrap()
                .w,
            150.0
        );

        canvas.on_mouse(&mut session, MouseButton::Left, false);
        assert_eq!(session.shapes[0].bbox().w, 150.0);
        assert!(session.render_rev() > rev);
    }

    #[test]
    fn resize_aller_fill_konturen_bleibt_bis_zum_commit_auf_der_gpu() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Select;
        let mut session = EditorSession::default();
        session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [40.0, 40.0]);
        session.add_box_shape(BoxShape::Rect, [60.0, 0.0], [100.0, 40.0]);
        session.layers[0].mode = luxifer_core::LayerMode::Fill;
        session.select_all();
        let rev = session.render_rev();

        canvas.cursor = canvas.cam.world_to_screen([100.0, 20.0]);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([150.0, 20.0]));

        assert_eq!(session.render_rev(), rev);
        assert_eq!(session.selection_bbox().unwrap().w, 100.0);
        assert_eq!(canvas.selection_transform().matrix, [1.5, 0.0, 0.0, 1.0]);

        canvas.on_mouse(&mut session, MouseButton::Left, false);
        assert_eq!(session.selection_bbox().unwrap().w, 150.0);
        assert!(session.render_rev() > rev);
    }

    #[test]
    fn rotate_vorschau_rotiert_gpu_und_committet_einmal() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Select;
        let mut session = EditorSession::default();
        session.add_box_shape(BoxShape::Rect, [0.0, 0.0], [100.0, 50.0]);
        let rev = session.render_rev();
        let bbox = session.selection_bbox().unwrap();
        let handle = crate::canvas::overlay::rotate_handle_pos(&bbox, canvas.cam.scale);

        canvas.cursor = canvas.cam.world_to_screen(handle);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([97.0, 25.0]));

        assert_eq!(session.render_rev(), rev);
        assert_eq!(session.shapes[0].rotation, 0.0);
        let transform = canvas.selection_transform();
        assert!(transform.matrix[0].abs() < 1e-5);
        assert!((transform.matrix[1] + 1.0).abs() < 1e-5);
        assert!((transform.matrix[2] - 1.0).abs() < 1e-5);

        canvas.on_mouse(&mut session, MouseButton::Left, false);
        assert!((session.shapes[0].rotation - 90.0).abs() < 1e-5);
        assert!(session.render_rev() > rev);
    }

    #[test]
    fn bild_rotate_vorschau_bleibt_bis_zum_commit_auf_der_gpu() {
        let mut canvas = CanvasState::new(Camera::new());
        canvas.tool = Tool::Select;
        let mut session = EditorSession::default();
        session.add_image("test-asset".into(), 0.0, 0.0, 100.0, 50.0);
        let rev = session.render_rev();
        let bbox = session.selection_bbox().unwrap();
        let handle = crate::canvas::overlay::rotate_handle_pos(&bbox, canvas.cam.scale);

        canvas.cursor = canvas.cam.world_to_screen(handle);
        canvas.on_mouse(&mut session, MouseButton::Left, true);
        canvas.on_cursor_move(&mut session, canvas.cam.world_to_screen([97.0, 25.0]));

        assert_eq!(session.render_rev(), rev);
        assert_eq!(session.shapes[0].rotation, 0.0);
        let transform = canvas.selection_transform();
        assert!(transform.matrix[0].abs() < 1e-5);
        assert!((transform.matrix[1] + 1.0).abs() < 1e-5);

        canvas.on_mouse(&mut session, MouseButton::Left, false);
        assert!((session.shapes[0].rotation - 90.0).abs() < 1e-5);
        assert!(session.render_rev() > rev);
    }
}
