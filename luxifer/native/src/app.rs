//! Der Anwendungs-Zustand des nativen Editors: hält den Core-`AppState`, die
//! Kamera, das aktive Werkzeug und den GPU/egui-Kontext. Verbindet Eingaben mit
//! Core-Aufrufen (der Core bleibt die Wahrheit) und rendert Canvas + Panels.

use std::sync::Arc;

use egui_wgpu::ScreenDescriptor;
use luxifer_application::{AppError, BoxShape, EditorSession, LayerParams, LayerToggle, PointPath};
use luxifer_core::geometry::Geo;
use luxifer_core::state::AppState;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

use crate::camera::Camera;
use crate::gpu::Gpu;
use crate::scene_geo::{self, Vertex};
use crate::tools::{Drag, LaserUi, Tool};
use crate::ui;

/// Ziel-Box beim Ziehen eines Skalier-Handles: die vom Handle bewegte(n)
/// Kante(n) folgen dem Cursor `w` (mm), die gegenüberliegenden bleiben fix.
/// Negative Größen werden normalisiert (Box klappt sauber um).
/// Zeichnet ein gestricheltes Segment (Welt-mm) als kurze Striche. `scale` =
/// Pixel pro mm, damit die Strichlänge am Bildschirm konstant wirkt.
fn dashed_seg(v: &mut Vec<Vertex>, a: [f32; 2], b: [f32; 2], color: [f32; 4], scale: f32) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-4 {
        return;
    }
    let dir = [dx / len, dy / len];
    let dash = 6.0 / scale; // ~6 px Strich
    let gap = 4.0 / scale; // ~4 px Lücke
    let step = dash + gap;
    let mut t = 0.0;
    while t < len {
        let s = [a[0] + dir[0] * t, a[1] + dir[1] * t];
        let e_t = (t + dash).min(len);
        let e = [a[0] + dir[0] * e_t, a[1] + dir[1] * e_t];
        scene_geo::push_seg(v, s, e, color);
        t += step;
    }
}

/// Ist der Handle eine Ecke (skaliert beide Achsen)?
pub fn is_corner(h: luxifer_core::Handle) -> bool {
    use luxifer_core::Handle::*;
    matches!(h, Nw | Ne | Sw | Se)
}

/// Zwingt die Ziel-Box aufs Start-Seitenverhältnis (proportionales Skalieren an
/// Ecken). Der Faktor ist der betragsgrößere der beiden Achsen — so folgt die
/// Ecke der Maus großzügig; die gegenüberliegende Ecke (fix bei `resize_target`)
/// bleibt der Anker.
pub fn keep_aspect(
    start: luxifer_core::BBox,
    handle: luxifer_core::Handle,
    target: luxifer_core::BBox,
) -> luxifer_core::BBox {
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
    luxifer_core::BBox::new(
        x.min(anchor_x),
        y.min(anchor_y),
        w.abs().max(0.1),
        h.abs().max(0.1),
    )
}

pub fn resize_target(
    start: luxifer_core::BBox,
    handle: luxifer_core::Handle,
    w: [f64; 2],
) -> luxifer_core::BBox {
    let mut left = start.x;
    let mut right = start.x + start.w;
    let mut top = start.y;
    let mut bottom = start.y + start.h;
    if handle.moves_left() {
        left = w[0];
    }
    if handle.moves_right() {
        right = w[0];
    }
    if handle.moves_top() {
        top = w[1];
    }
    if handle.moves_bottom() {
        bottom = w[1];
    }
    let x = left.min(right);
    let y = top.min(bottom);
    luxifer_core::BBox::new(
        x,
        y,
        (right - left).abs().max(0.1),
        (bottom - top).abs().max(0.1),
    )
}

pub struct App {
    pub window: Arc<Window>,
    pub gpu: Gpu,
    pub session: EditorSession,
    pub cam: Camera,
    pub tool: Tool,
    /// Aktive Polygon-Form (Dreieck/Stern/… — beim Polygon-Werkzeug aufgezogen).
    pub active_shape: luxifer_core::PolyShape,
    pub view: crate::tools::View,
    pub project: crate::project::ProjectBackend,
    /// Puffer für den „Neues Projekt"-Namen im Projekt-Reiter.
    pub new_project_name: String,
    pub laser: LaserUi,
    pub laser_backend: crate::laser::LaserBackend,
    /// Letzte Laser-Rückmeldung (Statuszeile im Panel).
    pub laser_msg: String,
    /// Zentraler, nutzerlesbarer Fehlerkanal der Anwendungsschicht.
    pub app_error: Option<AppError>,
    /// Offener Laser-Einstellungen-Dialog (Profil-Bearbeitung) oder None.
    pub laser_settings: Option<luxifer_core::LaserProfile>,
    pub drag: Drag,
    /// Aktive Zeichenfarbe für die Palette-Markierung (aus dem Core gespiegelt).
    pub accent: [u8; 3],
    cursor: [f32; 2],
    space_down: bool,
    ctrl_down: bool,
    shift_down: bool,
    // Polygon-Zug (Welt-Punkte), bis Doppelklick/Enter schließt.
    poly_pts: Vec<(f64, f64)>,
    // egui.
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    // Panel-Breiten, damit der Canvas den freien Bereich kennt.
    pub left_w: f32,
    pub right_w: f32,
    last_frame: std::time::Instant,
    pub fps: f32,
    // Vertex-Cache: die (teure) Scanline-Füllung wird NUR neu gebaut, wenn sich
    // der Zustand ändert — nicht pro Frame. Pan/Zoom lassen die Vertices
    // unberührt (die Projektion macht der Shader), daher bleiben sie gecacht.
    verts: Vec<Vertex>,
    last_fp: u64,
    /// Ob egui im letzten Frame einen sofortigen weiteren Repaint wollte
    /// (laufende Animation/Interaktion) — steuert die Render-Schleife.
    wants_repaint: bool,
    /// Bild-Texturen (asset-id → GPU-Textur) und ob neu geladen werden muss.
    images: crate::image_gpu::ImageStore,
    image_dirty: bool,
    /// Offener Text-Dialog (Eingabe/Font/Größe) oder None.
    pub text_dialog: Option<TextDialogState>,
    /// Offener Layer-Parameter-Dialog (Doppelklick auf Ebene) oder None.
    pub layer_dialog: Option<LayerDialogState>,
    /// Verfügbare System-Fonts (einmalig gescannt, lazy).
    pub fonts: Vec<crate::fonts::FontEntry>,
}

/// Kurzlebiger Entwurf des Layer-Parameter-Dialogs. Native hält nur diesen
/// Entwurf; die Wahrheit liegt im `AppState`. Speichern läuft über die Session,
/// Abbrechen verwirft den Entwurf ohne Mutation.
pub struct LayerDialogState {
    pub index: usize,
    pub params: LayerParams,
}

/// Zustand des Text-Dialogs.
pub struct TextDialogState {
    pub text: String,
    pub size_mm: f64,
    /// Index in `App::fonts`, oder None (kein Font gewählt).
    pub font_idx: Option<usize>,
}

impl Default for TextDialogState {
    fn default() -> Self {
        Self {
            text: "Text".into(),
            size_mm: 20.0,
            font_idx: None,
        }
    }
}

impl App {
    pub fn new(window: Arc<Window>, gpu: Gpu) -> Self {
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let egui_renderer =
            egui_wgpu::Renderer::new(&gpu.device, gpu.config.format, None, 1, false);

        let mut state = AppState::new();
        // Ein paar Start-Shapes, damit sofort etwas zu sehen ist.
        state.add_shape(Geo::Rect {
            x: 40.0,
            y: 40.0,
            w: 120.0,
            h: 80.0,
        });
        state.selected.clear();
        state.add_shape(Geo::Ellipse {
            cx: 260.0,
            cy: 120.0,
            rx: 60.0,
            ry: 40.0,
        });
        state.selected.clear();
        let accent = state.active_color().unwrap_or([0x3B, 0x82, 0xF6]);
        // Ein erstes CLI-Argument wird als zu importierende Datei geladen
        // (praktisch fürs Testen: `luxifer-native datei.svg`).
        let auto_import = std::env::args().nth(1);

        let mut cam = Camera::new();
        cam.viewport = [gpu.config.width as f32, gpu.config.height as f32];
        cam.fit_bbox([0.0, 0.0, state.bed_w_mm, state.bed_h_mm], 0.85);

        let mut app = Self {
            window,
            gpu,
            session: EditorSession::new(state),
            cam,
            tool: Tool::Select,
            active_shape: luxifer_core::PolyShape::Penta,
            // Start-Ansicht per Env (Testhilfe): LUXI_TAB=laser.
            view: if std::env::var("LUXI_TAB").as_deref() == Ok("laser") {
                crate::tools::View::Laser
            } else {
                crate::tools::View::Design
            },
            project: crate::project::ProjectBackend::default(),
            new_project_name: String::new(),
            laser: LaserUi::default(),
            laser_backend: crate::laser::LaserBackend::load(),
            laser_msg: String::new(),
            app_error: None,
            laser_settings: None,
            drag: Drag::None,
            accent,
            cursor: [0.0, 0.0],
            space_down: false,
            ctrl_down: false,
            shift_down: false,
            poly_pts: Vec::new(),
            egui_ctx,
            egui_state,
            egui_renderer,
            left_w: 0.0,
            right_w: 0.0,
            last_frame: std::time::Instant::now(),
            fps: 0.0,
            verts: Vec::new(),
            last_fp: 0,
            wants_repaint: false,
            images: crate::image_gpu::ImageStore::default(),
            image_dirty: false,
            text_dialog: None,
            layer_dialog: None,
            fonts: Vec::new(),
        };
        if let Some(path) = auto_import {
            app.import_path(std::path::Path::new(&path));
            // Beim Auto-Import gleich füllen (Fill-Stresstest sichtbar machen).
            if std::env::var("LUXI_FILL").is_ok() {
                app.toggle_fill();
            }
        }
        app
    }

    pub fn window_event(&mut self, event: &WindowEvent) -> bool {
        // egui zuerst — verschluckt es das Event (Panel getroffen), geht es nicht
        // an den Canvas.
        let resp = self.egui_state.on_window_event(&self.window, event);
        // Modifier immer mitschreiben — auch wenn egui das Event konsumiert,
        // sonst geht der Shift-/Ctrl-Status beim Zeichnen/Resizen verloren.
        if let WindowEvent::ModifiersChanged(m) = event {
            self.ctrl_down = m.state().control_key();
            self.shift_down = m.state().shift_key();
        }

        if resp.consumed {
            // Trotzdem Cursor mitschreiben, damit Canvas-Koordinaten stimmen.
            if let WindowEvent::CursorMoved { position, .. } = event {
                self.cursor = [position.x as f32, position.y as f32];
            }
            return resp.repaint;
        }

        match event {
            WindowEvent::Resized(sz) => {
                self.gpu.resize(sz.width, sz.height);
                self.cam.viewport = [sz.width as f32, sz.height as f32];
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    // Strg+S / Shift+Strg+S: speichern bzw. neue Version.
                    if pressed && self.ctrl_down && code == KeyCode::KeyS {
                        if self.shift_down {
                            self.project_save_version();
                        } else {
                            self.project_save();
                        }
                        return true;
                    }
                    match code {
                        KeyCode::Space => self.space_down = pressed,
                        KeyCode::Delete | KeyCode::Backspace if pressed => {
                            if !self.session.selected.is_empty() {
                                self.delete_selected();
                            }
                        }
                        KeyCode::Escape if pressed => {
                            self.poly_pts.clear();
                            if self.session.edit_active() {
                                self.session.cancel_edit();
                                self.drag = Drag::None;
                            } else {
                                self.session.clear_selection();
                            }
                        }
                        KeyCode::Enter if pressed => self.finish_polygon(),
                        KeyCode::KeyV if pressed => self.tool = Tool::Select,
                        KeyCode::KeyR if pressed => self.tool = Tool::Rect,
                        KeyCode::KeyE if pressed => self.tool = Tool::Ellipse,
                        KeyCode::KeyP if pressed => self.tool = Tool::Polygon,
                        KeyCode::KeyZ if pressed => {
                            self.undo();
                        }
                        KeyCode::KeyY if pressed => {
                            self.redo();
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let new = [position.x as f32, position.y as f32];
                self.on_cursor_move(new);
                self.cursor = new;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.on_mouse(*button, *state == ElementState::Pressed);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let s = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.cam.zoom_at(1.12_f32.powf(s), self.cursor);
            }
            _ => {}
        }
        true
    }

    fn world(&self) -> [f64; 2] {
        self.cam.screen_to_world(self.cursor)
    }

    pub fn delete_selected(&mut self) {
        if let Err(error) = self.session.delete_selected() {
            self.app_error = Some(error);
        }
    }

    pub fn undo(&mut self) {
        self.session.undo();
    }

    pub fn redo(&mut self) {
        self.session.redo();
    }

    fn on_mouse(&mut self, button: MouseButton, pressed: bool) {
        let w = self.world();
        match button {
            MouseButton::Middle => {
                self.drag = if pressed { Drag::Pan } else { Drag::None };
            }
            MouseButton::Left if pressed => {
                if self.space_down {
                    self.drag = Drag::Pan;
                    return;
                }
                match self.tool {
                    Tool::Select | Tool::Node => self.begin_select(w),
                    // Aufzieh-Werkzeuge (Zentrum/Ecke → Maus).
                    Tool::Rect | Tool::Ellipse | Tool::Polygon | Tool::Line | Tool::Measure => {
                        self.drag = Drag::DrawBox { start: w }
                    }
                    // Punkt-für-Punkt-Werkzeuge sammeln in poly_pts.
                    Tool::Polyline | Tool::Spline | Tool::Bezier => {
                        self.poly_pts.push((w[0], w[1]))
                    }
                }
            }
            MouseButton::Left => {
                // Loslassen: Zug abschließen.
                self.finish_drag(w);
            }
            _ => {}
        }
    }

    /// Kopie der aktuell selektierten Shapes (Index + Shape) — als Ausgangspunkt
    /// für Resize/Rotate, damit vom Startzustand statt inkrementell gerechnet wird.
    fn snapshot_selection(&self) -> Vec<(usize, luxifer_core::Shape)> {
        self.session
            .selected
            .iter()
            .filter_map(|&i| self.session.shapes.get(i).map(|s| (i, s.clone())))
            .collect()
    }

    /// Stellt die Shapes aus einem Snapshot wieder her (vor jeder Transformation).
    fn restore_snapshot(&mut self, orig: &[(usize, luxifer_core::Shape)]) {
        for (i, s) in orig {
            if let Some(dst) = self.session.shapes.get_mut(*i) {
                *dst = s.clone();
            }
        }
    }

    fn begin_select(&mut self, w: [f64; 2]) {
        // Zuerst: wurde ein Transform-Handle der aktuellen Auswahl getroffen?
        if let Some(b) = self.session.selection_bbox() {
            let pick = self.handle_hw() as f64 * 1.8; // etwas großzügiger als sichtbar
                                                      // Rotate-Handle?
            let rp = self.rotate_handle_pos(&b);
            if (w[0] - rp[0]).hypot(w[1] - rp[1]) <= pick {
                self.session.begin_edit();
                let pivot = [b.x + b.w / 2.0, b.y + b.h / 2.0];
                let angle = (w[1] - pivot[1]).atan2(w[0] - pivot[0]);
                self.drag = Drag::Rotate {
                    pivot,
                    start_angle: angle,
                    orig: self.snapshot_selection(),
                };
                return;
            }
            // Skalier-Handle?
            for (handle, (hx, hy)) in luxifer_core::Handle::positions(&b) {
                if (w[0] - hx).abs() <= pick && (w[1] - hy).abs() <= pick {
                    self.session.begin_edit();
                    self.drag = Drag::Resize {
                        handle,
                        start_box: b,
                        orig: self.snapshot_selection(),
                    };
                    return;
                }
            }
        }

        let tol = 4.0 / self.cam.scale as f64;
        let hit = self.session.select_at(w[0], w[1], tol, self.shift_down);
        if self.shift_down {
            self.drag = Drag::None;
        } else if hit.is_some() {
            self.session.begin_edit();
            self.drag = Drag::MoveShapes { last: w };
        } else {
            self.drag = Drag::Marquee { start: w };
        }
    }

    fn on_cursor_move(&mut self, new: [f32; 2]) {
        let dx = new[0] - self.cursor[0];
        let dy = new[1] - self.cursor[1];
        let w = self.cam.screen_to_world(new);
        // Erst die reinen Kamera-/Move-Fälle (kein Snapshot nötig).
        match &mut self.drag {
            Drag::Pan => {
                self.cam.pan_pixels(dx, dy);
                return;
            }
            Drag::MoveShapes { last } => {
                let last = *last;
                self.drag = Drag::MoveShapes { last: w };
                self.session.translate_edit(w[0] - last[0], w[1] - last[1]);
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
                self.restore_snapshot(&orig);
                let mut target = resize_target(start_box, handle, w);
                // Eck-Handles halten standardmäßig das Seitenverhältnis; Shift
                // löst es (frei). Kanten-Handles skalieren nur eine Achse.
                if is_corner(handle) && !self.shift_down {
                    target = keep_aspect(start_box, handle, target);
                }
                self.session.scale_edit(start_box, target);
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
                self.restore_snapshot(&orig);
                let a = (w[1] - pivot[1]).atan2(w[0] - pivot[0]);
                let delta_deg = (a - start_angle).to_degrees();
                self.session.rotate_edit(delta_deg);
                self.drag = Drag::Rotate {
                    pivot,
                    start_angle,
                    orig,
                };
            }
            other => self.drag = other,
        }
    }

    fn finish_drag(&mut self, w: [f64; 2]) {
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Marquee { start } => {
                if (start[0] - w[0]).abs() > 1.0 || (start[1] - w[1]).abs() > 1.0 {
                    self.session.select_rect(start, w);
                }
            }
            Drag::DrawBox { start } => self.finish_box(start, w),
            Drag::MoveShapes { .. } | Drag::Resize { .. } | Drag::Rotate { .. } => {
                self.session.commit_edit();
            }
            _ => {}
        }
    }

    fn finish_box(&mut self, a: [f64; 2], b: [f64; 2]) {
        // Messen: nichts erzeugen (nur Anzeige während des Ziehens).
        if self.tool == Tool::Measure {
            return;
        }
        // Polygon: Form vom Zentrum `a` mit Radius = Abstand zur Maus aufziehen
        // (wie Tauri: ondrawpolygon(shape, cx, cy, r, rot)).
        if self.tool == Tool::Polygon {
            if self.session.add_polygon(self.active_shape, a, b).is_some() {
                self.refresh_accent();
            }
            return;
        }
        // Linie: 2-Punkt-Polyline (auch bei kleinem Zug erlaubt).
        if self.tool == Tool::Line {
            if self.session.add_line(a, b).is_some() {
                self.refresh_accent();
            }
            return;
        }
        let shape = match self.tool {
            Tool::Ellipse => BoxShape::Ellipse,
            _ => BoxShape::Rect,
        };
        if self.session.add_box_shape(shape, a, b).is_some() {
            self.refresh_accent();
        }
    }

    /// Schließt den punktbasierten Zug ab (Enter/Doppelklick). Je nach Werkzeug:
    /// Polygon (geschlossen), Polylinie (offen), Spline (glatt), Bézier (Feder).
    fn finish_polygon(&mut self) {
        let pts = std::mem::take(&mut self.poly_pts);
        let path = match self.tool {
            Tool::Polyline => PointPath::Polyline,
            Tool::Spline => PointPath::Spline,
            Tool::Bezier => PointPath::Bezier,
            _ => return,
        };
        if self.session.add_point_path(path, pts).is_some() {
            self.refresh_accent();
        }
    }

    pub fn pick_color(&mut self, c: [u8; 3]) {
        self.session.activate_color(c);
        self.refresh_accent();
    }

    // ---- Sofort-Aktionen auf der Auswahl (Werkzeugleiste + Arrange) ----------

    pub fn mirror_h(&mut self) {
        let result = self.session.mirror(luxifer_core::Axis::Vertical);
        self.report(result);
    }
    pub fn mirror_v(&mut self) {
        let result = self.session.mirror(luxifer_core::Axis::Horizontal);
        self.report(result);
    }
    pub fn insert_coasters(&mut self, round: bool) {
        self.session.insert_coasters(round);
        self.fit_all();
    }
    pub fn align(&mut self, kind: luxifer_core::Align) {
        let result = self.session.align(kind);
        self.report(result);
    }
    pub fn distribute(&mut self, kind: luxifer_core::Distribute) {
        let result = self.session.distribute(kind);
        self.report(result);
    }
    pub fn group(&mut self) {
        let result = self.session.group();
        self.report(result);
    }
    pub fn ungroup(&mut self) {
        let result = self.session.ungroup();
        self.report(result);
    }
    pub fn nest(&mut self, gap: f64) {
        let result = self.session.nest(gap);
        self.report(result);
    }
    pub fn nest_fill(&mut self, gap: f64) {
        let result = self.session.nest_fill(gap);
        self.report(result);
    }
    pub fn boolean(&mut self, op: luxifer_core::BoolOp) {
        let result = self.session.boolean(op);
        self.report(result);
    }
    pub fn offset(&mut self, dist: f64) {
        let result = self.session.offset(dist);
        self.report(result);
    }
    pub fn fillet(&mut self, radius: f64) {
        let result = self.session.fillet(radius);
        self.report(result);
    }
    pub fn selection_count(&self) -> usize {
        self.session.selected.len()
    }

    pub fn toggle_layer(&mut self, index: usize, toggle: LayerToggle) {
        let result = self.session.toggle_layer(index, toggle);
        self.report(result);
    }

    pub fn move_layer(&mut self, from: usize, to: usize) {
        let result = self.session.move_layer(from, to);
        self.report(result);
    }

    /// Öffnet den Layer-Parameter-Dialog mit den aktuellen Werten als Entwurf.
    pub fn open_layer_dialog(&mut self, index: usize) {
        if let Some(layer) = self.session.layers.get(index) {
            self.layer_dialog = Some(LayerDialogState {
                index,
                params: LayerParams::from_layer(layer),
            });
        }
    }

    /// Übernimmt den Dialogentwurf über die Session. Bei Erfolg true (Dialog
    /// schließen); bei Validierungsfehler bleibt der Dialog offen und der Fehler
    /// erscheint im zentralen Kanal.
    pub fn commit_layer_dialog(&mut self) -> bool {
        let Some(st) = self.layer_dialog.as_ref() else {
            return false;
        };
        let index = st.index;
        let params = st.params.clone();
        match self.session.set_layer_params(index, params) {
            Ok(()) => true,
            Err(error) => {
                self.app_error = Some(error);
                false
            }
        }
    }

    /// Sofort-Aktion aus der Werkzeugleiste. Boolean/Fillet/Offset laufen mit
    /// sinnvollen Defaults (Parameter-Feinjustage folgt als Dialog); Bridge/
    /// Muster brauchen Interaktion/mehr Parameter und melden das vorerst.
    pub fn begin_action(&mut self, a: crate::tools::ToolAction) {
        use crate::tools::ToolAction as A;
        match a {
            A::Boolean => self.boolean(luxifer_core::BoolOp::Union),
            A::Fillet => self.fillet(2.0),
            A::Offset => self.offset(2.0),
            A::PatternFill => {
                self.app_error = Some(AppError::new(
                    "not_migrated",
                    "Muster-Füllung ist noch nicht migriert.",
                ))
            }
            A::Bridge => {
                self.app_error = Some(AppError::new(
                    "not_migrated",
                    "Haltestege sind noch nicht migriert.",
                ))
            }
        }
    }

    fn report(&mut self, result: Result<(), AppError>) {
        if let Err(error) = result {
            self.app_error = Some(error);
        }
    }

    // ---- Projekt (README-3c) -------------------------------------------------

    /// Projekt öffnen: ersetzt den Canvas-Zustand durch den geladenen.
    pub fn project_open(&mut self, name: &str) {
        if let Some(state) = self.project.open(name) {
            self.session.replace_state(state);
            self.refresh_accent();
            self.image_dirty = true;
            self.fit_all();
            self.view = crate::tools::View::Design;
        }
    }

    /// Neues Projekt aus dem aktuellen Canvas anlegen und in-place speichern.
    pub fn project_new(&mut self, name: &str) {
        if name.trim().is_empty() {
            self.project.msg = "Bitte einen Namen angeben.".into();
            return;
        }
        self.project.new_from_state(&self.session, name.trim());
        self.project.save(&self.session);
        self.view = crate::tools::View::Design;
    }

    /// In-place speichern (Strg+S).
    pub fn project_save(&mut self) {
        self.project.save(&self.session);
    }

    /// Als neue Version speichern (Shift+Strg+S).
    pub fn project_save_version(&mut self) {
        self.project.save_version(&self.session);
    }

    /// Öffnet den Text-Dialog und scannt bei Bedarf die System-Fonts.
    pub fn open_text_dialog(&mut self) {
        if self.fonts.is_empty() {
            self.fonts = crate::fonts::list_fonts();
        }
        let mut st = TextDialogState::default();
        // Ersten Font vorwählen.
        if !self.fonts.is_empty() {
            st.font_idx = Some(0);
        }
        self.text_dialog = Some(st);
    }

    /// Setzt den Text als Pfad-Shapes (Text→Kontur über den Core) und platziert
    /// ihn. Gibt bei Erfolg true zurück (Dialog schließen).
    pub fn commit_text(&mut self) -> bool {
        let Some(st) = self.text_dialog.as_ref() else {
            return false;
        };
        let Some(fi) = st.font_idx else {
            self.laser_msg = "Kein Font gewählt".into();
            return false;
        };
        let Some(font) = self.fonts.get(fi) else {
            return false;
        };
        let text = st.text.clone();
        let size = st.size_mm;
        let font_path = font.path.clone();
        let font_data = match std::fs::read(&font_path) {
            Ok(d) => d,
            Err(e) => {
                self.laser_msg = format!("Font lesen: {e}");
                return false;
            }
        };
        match luxifer_core::text::text_to_contours(&font_data, &text, size) {
            Ok(contours) if !contours.is_empty() => {
                let meta = luxifer_core::TextMeta {
                    text,
                    font_path: font_path.to_string_lossy().to_string(),
                    size_mm: size,
                };
                let idxs = self.session.add_text_block(contours, meta);
                self.session.selected = idxs;
                self.refresh_accent();
                self.fit_all();
                true
            }
            Ok(_) => {
                self.laser_msg = "Text ergab keine Konturen".into();
                false
            }
            Err(e) => {
                self.laser_msg = format!("Text-Fehler: {e}");
                false
            }
        }
    }

    /// Öffnet einen nativen Datei-Dialog und importiert SVG/DXF über den Core.
    /// Danach Kamera auf die neue Geometrie einpassen.
    pub fn import_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Vektor", &["svg", "dxf"])
            .pick_file()
        {
            self.import_path(&path);
        }
    }

    /// Bild importieren (Asset-Store) und als Image-Shape platzieren.
    pub fn import_image_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Bild", &["png", "jpg", "jpeg", "bmp", "gif", "webp"])
            .pick_file()
        else {
            return;
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                log::error!("Bild lesen: {e}");
                return;
            }
        };
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("bild")
            .to_string();
        match luxifer_core::import_image(&luxifer_core::assets_dir(), &bytes, &name) {
            Ok(meta) => {
                // Pixel → mm bei 254 DPI (10 px/mm), wie der Core-Default.
                let w_mm = meta.width as f64 / 10.0;
                let h_mm = meta.height as f64 / 10.0;
                let idx = self
                    .session
                    .add_image(meta.id.clone(), 20.0, 20.0, w_mm, h_mm);
                self.session.selected = vec![idx];
                self.image_dirty = true;
                self.fit_all();
                log::info!(
                    "Bild importiert: {} ({}×{})",
                    meta.id,
                    meta.width,
                    meta.height
                );
            }
            Err(e) => log::error!("Bild-Import fehlgeschlagen: {e}"),
        }
    }

    /// Importiert eine Datei direkt (auch für den „Aztec laden"-Schnellknopf).
    pub fn import_path(&mut self, path: &std::path::Path) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                log::error!("Datei lesen: {e}");
                return;
            }
        };
        match luxifer_core::import::import_vector(&bytes, &ext) {
            Ok(contours) => {
                let t = std::time::Instant::now();
                self.session.add_polylines(contours);
                self.refresh_accent();
                self.fit_all();
                log::info!(
                    "Import {}: {} Shapes in {:?}",
                    path.display(),
                    self.session.shapes.len(),
                    t.elapsed()
                );
            }
            Err(e) => log::error!("Import fehlgeschlagen: {e}"),
        }
    }

    /// Kamera auf die BBox aller Shapes einpassen (Fallback: Tisch).
    fn fit_all(&mut self) {
        let b =
            luxifer_core::geometry::BBox::union_all(self.session.shapes.iter().map(|s| s.bbox()));
        if let Some(b) = b {
            self.cam.fit_bbox([b.x, b.y, b.w, b.h], 0.85);
        } else {
            self.cam.fit_bbox(
                [0.0, 0.0, self.session.bed_w_mm, self.session.bed_h_mm],
                0.85,
            );
        }
    }

    /// Schaltet den Modus aller Layer zwischen Cut (nur Kontur) und Fill (Fläche).
    /// Für den Fill-Stresstest an importierter Geometrie.
    pub fn toggle_fill(&mut self) {
        use luxifer_core::model::LayerMode;
        let any_cut = self.session.layers.iter().any(|l| l.mode == LayerMode::Cut);
        let target = if any_cut {
            LayerMode::Fill
        } else {
            LayerMode::Cut
        };
        for l in &mut self.session.layers {
            if l.mode == LayerMode::Cut || l.mode == LayerMode::Fill {
                l.mode = target;
            }
        }
    }

    fn refresh_accent(&mut self) {
        if let Some(c) = self.session.active_color() {
            self.accent = c;
        }
    }

    // ---- Laser-Aktionen (verdrahtet das Panel mit dem echten Treiber) --------

    /// Die (ggf. nur selektierten) Shapes + Layer für einen Job.
    fn laser_shapes(&self) -> (Vec<luxifer_core::Shape>, Vec<luxifer_core::Layer>) {
        let shapes = if self.laser.selection_only {
            self.session
                .selected
                .iter()
                .filter_map(|&i| self.session.shapes.get(i).cloned())
                .collect()
        } else {
            self.session.shapes.clone()
        };
        (shapes, self.session.layers.clone())
    }

    pub fn laser_select(&mut self, id: &str) {
        self.laser_backend.set_active(id);
        self.laser_msg.clear();
    }

    /// Führt eine Job-Aktion des aktiven Treibers aus.
    pub fn laser_run(&mut self, action: luxifer_core::JobAction) {
        let (shapes, layers) = self.laser_shapes();
        let sm = self.laser.start_mode;
        let anchor = self.laser.anchor;
        match self
            .laser_backend
            .run_action(action, &shapes, &layers, sm, anchor)
        {
            Ok(msg) => self.laser_msg = msg,
            Err(e) => self.laser_msg = format!("Fehler: {e}"),
        }
    }

    /// Kompiliert den Job und speichert ihn über einen Datei-Dialog.
    pub fn laser_export(&mut self) {
        let ext = match self.laser_backend.active_profile().map(|p| p.kind) {
            Some(luxifer_core::DriverKind::Ruida) => "rd",
            _ => "gcode",
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("job.{ext}"))
            .save_file()
        {
            let (shapes, layers) = self.laser_shapes();
            let sm = self.laser.start_mode;
            let anchor = self.laser.anchor;
            match self
                .laser_backend
                .export_to(&path, &shapes, &layers, sm, anchor)
            {
                Ok(()) => self.laser_msg = format!("Exportiert: {}", path.display()),
                Err(e) => self.laser_msg = format!("Export-Fehler: {e}"),
            }
        }
    }

    pub fn laser_jog(&mut self, dx: f64, dy: f64) {
        let speed = self.laser.jog_speed;
        if let Err(e) = self.laser_backend.jog(dx, dy, speed) {
            self.laser_msg = format!("Jog-Fehler: {e}");
        }
    }

    pub fn laser_home(&mut self) {
        let speed = self.laser.jog_speed;
        if let Err(e) = self.laser_backend.home(speed) {
            self.laser_msg = format!("Home-Fehler: {e}");
        }
    }

    /// Öffnet den Einstellungen-Dialog: bestehendes Profil bearbeiten oder ein
    /// neues (Default) anlegen.
    pub fn open_laser_settings(&mut self, edit_active: bool) {
        self.laser_settings = Some(if edit_active {
            self.laser_backend
                .active_profile()
                .cloned()
                .unwrap_or_default()
        } else {
            luxifer_core::LaserProfile::default()
        });
    }

    pub fn save_laser_settings(&mut self) {
        if let Some(profile) = self.laser_settings.take() {
            let new = profile.id.is_empty();
            self.laser_backend.save_profile(profile);
            // Neu angelegtes Profil gleich aktivieren, wenn noch keins aktiv war.
            if new && self.laser_backend.active_profile().is_none() {
                if let Some(p) = self.laser_backend.registry.profiles.last() {
                    let id = p.id.clone();
                    self.laser_backend.set_active(&id);
                }
            }
        }
    }

    pub fn delete_laser_profile(&mut self, id: &str) {
        self.laser_backend.delete_profile(id);
        self.laser_settings = None;
    }

    /// Billiger Fingerprint dessen, was die Vertices beeinflusst. Ändert er
    /// sich, werden die Vertices neu gebaut — sonst bleibt der Cache. So kann
    /// kein mutierender Pfad das Invalidieren „vergessen". Kamera ist bewusst
    /// NICHT enthalten (Pan/Zoom projiziert der Shader, keine Neuberechnung).
    fn scene_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.session.shapes.len().hash(&mut h);
        self.session.selected.hash(&mut h);
        self.poly_pts.len().hash(&mut h);
        // Layer-Modus/Farbe/Sichtbarkeit beeinflussen Fill + Farbe.
        for l in &self.session.layers {
            l.color.hash(&mut h);
            l.visible.hash(&mut h);
            (l.mode as u8).hash(&mut h);
        }
        // Geometrie-Änderungen (Move/Draw/Import): BBox-Summe als grober Proxy,
        // plus Rotation. Günstig und für Cache-Invalidierung ausreichend.
        for s in &self.session.shapes {
            let b = s.geo.bbox();
            (b.x.to_bits(), b.y.to_bits(), b.w.to_bits(), b.h.to_bits()).hash(&mut h);
            s.rotation.to_bits().hash(&mut h);
            s.layer_id.hash(&mut h);
        }
        // Laufender Polygon-Zug (Live-Vorschau).
        for p in &self.poly_pts {
            (p.0.to_bits(), p.1.to_bits()).hash(&mut h);
        }
        h.finish()
    }

    /// Halbe Handle-Kantenlänge in Welt-mm, damit sie am Bildschirm konstant
    /// ~7px groß wirken (unabhängig vom Zoom).
    fn handle_hw(&self) -> f32 {
        7.0 / self.cam.scale
    }

    /// Rotate-Handle-Position (mm): mittig über der Auswahl-BBox, mit Abstand.
    fn rotate_handle_pos(&self, b: &luxifer_core::BBox) -> [f64; 2] {
        let off = 22.0 / self.cam.scale as f64;
        [b.x + b.w / 2.0, b.y - off]
    }

    /// Baut die Overlay-Vertices (Live-Zeichenvorschau, Transform-Handles).
    /// Jeden Frame neu (kamera-abhängig), aber winzig.
    fn build_overlay(&self) -> Vec<Vertex> {
        let mut v = Vec::new();

        let preview = [0.6, 0.8, 1.0, 0.9];
        // Live-Vorschau beim Aufziehen eines Rechtecks/einer Ellipse/Linie.
        if let Drag::DrawBox { start } = self.drag {
            let cur = self.world();
            match self.tool {
                Tool::Ellipse => {
                    let x = start[0].min(cur[0]) as f32;
                    let y = start[1].min(cur[1]) as f32;
                    let w = (start[0] - cur[0]).abs() as f32;
                    let h = (start[1] - cur[1]).abs() as f32;
                    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
                    let (rx, ry) = (w / 2.0, h / 2.0);
                    let n = 48;
                    let mut prev = [cx + rx, cy];
                    for i in 1..=n {
                        let a = i as f32 / n as f32 * std::f32::consts::TAU;
                        let p = [cx + rx * a.cos(), cy + ry * a.sin()];
                        scene_geo::push_seg(&mut v, prev, p, preview);
                        prev = p;
                    }
                }
                Tool::Line | Tool::Measure => {
                    scene_geo::push_seg(
                        &mut v,
                        [start[0] as f32, start[1] as f32],
                        [cur[0] as f32, cur[1] as f32],
                        preview,
                    );
                }
                Tool::Polygon => {
                    // Form vom Zentrum aufziehen (Vorschau der gewählten PolyShape).
                    let r = (start[0] - cur[0]).hypot(start[1] - cur[1]);
                    if r > 0.5 {
                        let pts = self.active_shape.points(start[0], start[1], r, 0.0);
                        if pts.len() >= 2 {
                            for wnd in pts.windows(2) {
                                scene_geo::push_seg(
                                    &mut v,
                                    [wnd[0].0 as f32, wnd[0].1 as f32],
                                    [wnd[1].0 as f32, wnd[1].1 as f32],
                                    preview,
                                );
                            }
                            // Schlusskante.
                            let (f, l) = (pts[0], pts[pts.len() - 1]);
                            scene_geo::push_seg(
                                &mut v,
                                [l.0 as f32, l.1 as f32],
                                [f.0 as f32, f.1 as f32],
                                preview,
                            );
                        }
                    }
                }
                _ => {
                    let x = start[0].min(cur[0]) as f32;
                    let y = start[1].min(cur[1]) as f32;
                    let w = (start[0] - cur[0]).abs() as f32;
                    let h = (start[1] - cur[1]).abs() as f32;
                    v.extend(scene_geo::rect_outline(x, y, w, h, preview));
                }
            }
        }

        // Live-Vorschau des Punkt-Zugs (Polyline/Spline/Bézier/Polygon): gesetzte
        // Segmente + gestricheltes Gummiband zur Maus + Punkt-Marker, wie Tauri.
        if !self.poly_pts.is_empty()
            && matches!(self.tool, Tool::Polyline | Tool::Spline | Tool::Bezier)
        {
            let col = [0.9, 0.9, 0.95, 0.9];
            // Gesetzte Segmente.
            for wnd in self.poly_pts.windows(2) {
                scene_geo::push_seg(
                    &mut v,
                    [wnd[0].0 as f32, wnd[0].1 as f32],
                    [wnd[1].0 as f32, wnd[1].1 as f32],
                    col,
                );
            }
            // Gummiband vom letzten Punkt zur Maus (gestrichelt).
            let cur = self.world();
            let last = *self.poly_pts.last().unwrap();
            dashed_seg(
                &mut v,
                [last.0 as f32, last.1 as f32],
                [cur[0] as f32, cur[1] as f32],
                [1.0, 1.0, 1.0, 0.4],
                self.cam.scale,
            );
            // Punkt-Marker (kleine Quadrate); Startpunkt hervorgehoben.
            let hw = 3.0 / self.cam.scale;
            for (i, p) in self.poly_pts.iter().enumerate() {
                let c = if i == 0 {
                    [0.25, 0.72, 0.5, 1.0] // Start grün (Schließen-Signal)
                } else {
                    [0.3, 0.51, 0.97, 1.0]
                };
                v.extend(scene_geo::handle_marker(p.0 as f32, p.1 as f32, hw, c));
            }
        }

        // Handles nur im Auswahl-Werkzeug und bei vorhandener Auswahl.
        if self.tool != Tool::Select {
            return v;
        }
        let Some(b) = self.session.selection_bbox() else {
            return v;
        };
        let hw = self.handle_hw();
        for (_, (hx, hy)) in luxifer_core::Handle::positions(&b) {
            v.extend(scene_geo::handle_marker(
                hx as f32,
                hy as f32,
                hw,
                scene_geo::HANDLE_COLOR,
            ));
        }
        // Rotate-Handle: Linie von oben-Mitte nach oben + Kreis-Marker.
        let rp = self.rotate_handle_pos(&b);
        let top = [b.x as f32 + b.w as f32 / 2.0, b.y as f32];
        scene_geo::push_seg(
            &mut v,
            top,
            [rp[0] as f32, rp[1] as f32],
            scene_geo::SEL_BOX_COLOR,
        );
        v.extend(scene_geo::handle_marker(
            rp[0] as f32,
            rp[1] as f32,
            hw * 1.1,
            scene_geo::HANDLE_COLOR,
        ));
        v
    }

    /// Baut die Zeichendaten (Tisch-Gitter, Shapes, Auswahl-BBox, Polygon-Zug).
    fn build_vertices(&self) -> Vec<Vertex> {
        let mut v = scene_geo::bed_grid(self.session.bed_w_mm as f32, self.session.bed_h_mm as f32);
        // Füllung zuerst (liegt unter den Konturen), dann die Umrisse.
        v.extend(scene_geo::fill_lines(&self.session));
        v.extend(scene_geo::shape_lines(&self.session, self.accent));
        if let Some(b) = self.session.selection_bbox() {
            v.extend(scene_geo::rect_outline(
                b.x as f32,
                b.y as f32,
                b.w as f32,
                b.h as f32,
                scene_geo::SEL_BOX_COLOR,
            ));
        }
        // Der laufende Punkt-Zug (Polyline/Spline/Bézier/Polygon) wird im OVERLAY
        // gezeichnet (jeden Frame, damit das Gummiband der Maus folgt).
        v
    }

    /// Ob nach dem letzten Frame sofort weiter gezeichnet werden soll
    /// (egui-Animation/Interaktion läuft). Steuert die Render-Schleife.
    pub fn egui_wants_repaint(&self) -> bool {
        self.wants_repaint
    }

    pub fn render(&mut self) {
        // FPS.
        let dt = self.last_frame.elapsed().as_secs_f32();
        self.last_frame = std::time::Instant::now();
        if dt > 0.0 {
            self.fps = 0.9 * self.fps + 0.1 * (1.0 / dt);
        }

        // egui-Frame bauen (Panels). Liefert Breiten zurück für den Canvas-Bereich.
        let raw = self.egui_state.take_egui_input(&self.window);
        let full = self.egui_ctx.clone().run(raw, |ctx| ui::build(ctx, self));
        self.egui_state
            .handle_platform_output(&self.window, full.platform_output);
        // Will egui gleich wieder zeichnen (laufende Animation/Interaktion)?
        // Delay == 0 → ja. So bleibt die Schleife nur bei Bedarf aktiv.
        self.wants_repaint = full
            .viewport_output
            .values()
            .any(|v| v.repaint_delay.is_zero());
        let tris = self.egui_ctx.tessellate(full.shapes, full.pixels_per_point);

        // Canvas-Vertices nur neu bauen+hochladen, wenn sich die Szene änderte
        // (nicht bei reinem Pan/Zoom — das macht der Shader). Das war der
        // 3-fps-Killer: die Scanline-Füllung lief zuvor pro Frame.
        let fp = self.scene_fingerprint();
        if fp != self.last_fp {
            self.last_fp = fp;
            self.verts = self.build_vertices();
            let verts = std::mem::take(&mut self.verts);
            self.gpu.upload_verts(&verts);
            self.verts = verts;
        }
        self.gpu.upload_camera(&self.cam);
        // Bild-Texturen laden (nur wenn neue Bilder dazukamen).
        if self.image_dirty || fp != self.last_fp {
            self.images.sync(
                &self.gpu.device,
                &self.gpu.queue,
                self.gpu.config.format,
                &self.session,
            );
            self.image_dirty = false;
        }
        let count = self.verts.len() as u32;
        // Overlay (Handles) jeden Frame neu — klein, kamera-abhängig.
        let overlay = self.build_overlay();
        self.gpu.upload_overlay(&overlay);

        let frame = match self.gpu.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                self.gpu
                    .surface
                    .configure(&self.gpu.device, &self.gpu.config);
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());

        // egui-Texturen/Buffer aktualisieren.
        let screen = ScreenDescriptor {
            size_in_pixels: [self.gpu.config.width, self.gpu.config.height],
            pixels_per_point: full.pixels_per_point,
        };
        for (id, delta) in &full.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.gpu.device, &self.gpu.queue, *id, delta);
        }
        self.egui_renderer.update_buffers(
            &self.gpu.device,
            &self.gpu.queue,
            &mut enc,
            &tris,
            &screen,
        );

        // Scratch-Buffer für die Bild-Quads (muss den Render-Pass überleben).
        let mut img_scratch: Option<wgpu::Buffer> = None;
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.06,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // Bilder zuunterst, dann Linien/Fill, dann Handles.
            self.images.draw(
                &mut rp,
                &self.gpu.device,
                &self.gpu.queue,
                &self.cam,
                &self.session,
                &mut img_scratch,
            );
            self.gpu.draw_canvas(&mut rp, count);
            self.gpu.draw_overlay(&mut rp);
            // egui obendrauf (eigener Lebenszeit-Scope via forget_lifetime).
            let mut rp = rp.forget_lifetime();
            self.egui_renderer.render(&mut rp, &tris, &screen);
        }
        self.gpu.queue.submit(Some(enc.finish()));
        frame.present();

        for id in &full.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resize_target;
    use luxifer_core::{BBox, Handle};

    #[test]
    fn resize_se_zieht_rechte_untere_ecke() {
        let start = BBox::new(0.0, 0.0, 100.0, 100.0);
        // Se-Handle auf (150,120) ziehen: Ursprung bleibt, Box wird 150×120.
        let t = resize_target(start, Handle::Se, [150.0, 120.0]);
        assert!((t.x - 0.0).abs() < 1e-9 && (t.y - 0.0).abs() < 1e-9);
        assert!((t.w - 150.0).abs() < 1e-9 && (t.h - 120.0).abs() < 1e-9);
    }

    #[test]
    fn resize_nw_haelt_gegenueberliegende_ecke_fix() {
        let start = BBox::new(0.0, 0.0, 100.0, 100.0);
        // Nw auf (20,30): rechte-untere Ecke (100,100) bleibt fix.
        let t = resize_target(start, Handle::Nw, [20.0, 30.0]);
        assert!((t.x - 20.0).abs() < 1e-9 && (t.y - 30.0).abs() < 1e-9);
        assert!((t.x + t.w - 100.0).abs() < 1e-9);
        assert!((t.y + t.h - 100.0).abs() < 1e-9);
    }

    #[test]
    fn resize_e_aendert_nur_breite() {
        let start = BBox::new(10.0, 10.0, 50.0, 50.0);
        let t = resize_target(start, Handle::E, [200.0, 999.0]);
        assert!((t.y - 10.0).abs() < 1e-9 && (t.h - 50.0).abs() < 1e-9);
        assert!((t.x + t.w - 200.0).abs() < 1e-9);
    }

    /// Bild-Import-Kette: import_image (Store) → add_image → Geo::Image im State.
    /// Verifiziert die native Verdrahtung (Rendern selbst braucht die GPU).
    #[test]
    fn bild_import_legt_image_shape_an() {
        use luxifer_core::{import_image, AppState, Geo};
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../luxifer/native/tests/fixtures/test2x2.png"
        ));
        // Fixture optional: wenn nicht vorhanden, Test überspringen (CI-tolerant).
        let Ok(bytes) = png else {
            eprintln!("Fixture fehlt — Test übersprungen");
            return;
        };
        let dir = std::env::temp_dir().join("luxifer_img_test");
        let meta = import_image(&dir, &bytes, "test.png").expect("import_image");
        assert!(meta.width >= 1 && meta.height >= 1);

        let mut s = AppState::new();
        let idx = s.add_image(
            meta.id.clone(),
            0.0,
            0.0,
            meta.width as f64,
            meta.height as f64,
        );
        match &s.shapes[idx].geo {
            Geo::Image { asset, .. } => assert_eq!(asset, &meta.id),
            _ => panic!("erwartet Geo::Image"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Text→Pfad-Kette mit einem echten System-Font. CI-tolerant.
    #[test]
    fn text_wird_zu_pfad_shapes() {
        use luxifer_core::{text::text_to_contours, AppState, TextMeta};
        let fonts = crate::fonts::list_fonts();
        let Some(font) = fonts.first() else {
            eprintln!("Kein System-Font — Test übersprungen");
            return;
        };
        let data = std::fs::read(&font.path).expect("font lesen");
        let contours = text_to_contours(&data, "Hi", 20.0).expect("text_to_contours");
        assert!(!contours.is_empty(), "Text sollte Konturen ergeben");
        let mut s = AppState::new();
        let idxs = s.add_text_block(
            contours,
            TextMeta {
                text: "Hi".into(),
                font_path: font.path.to_string_lossy().to_string(),
                size_mm: 20.0,
            },
        );
        assert!(!idxs.is_empty(), "Text-Block sollte Shapes anlegen");
    }

    /// Projekt-Round-Trip: from_state → save_to_dir → load_by_name → into_state.
    /// Deckt die Kette ab, die ProjectBackend nutzt.
    #[test]
    fn projekt_speichern_und_laden() {
        use luxifer_core::{project::ProjectFile, AppState, Geo};
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 5.0,
            y: 5.0,
            w: 30.0,
            h: 20.0,
        });
        let n_shapes = s.shapes.len();

        let dir = std::env::temp_dir().join("luxifer_proj_test");
        let _ = std::fs::remove_dir_all(&dir);
        let mut pf = ProjectFile::from_state(&s, "TestProj", Vec::new());
        pf.save_to_dir(&dir).expect("save_to_dir");
        pf.save_current(&dir, &[]).expect("save_current");

        let loaded = ProjectFile::load_by_name(&dir, "TestProj").expect("load_by_name");
        assert_eq!(loaded.name, "TestProj");
        let restored = loaded.into_state();
        assert_eq!(restored.shapes.len(), n_shapes);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keep_aspect_haelt_verhaeltnis_und_anker() {
        use super::{keep_aspect, resize_target};
        use luxifer_core::Handle;
        let start = BBox::new(0.0, 0.0, 100.0, 50.0); // 2:1
                                                      // SE weit nach rechts ziehen (Höhe zieht wenig): frei wäre 300×60.
        let free = resize_target(start, Handle::Se, [300.0, 60.0]);
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
        use super::{keep_aspect, resize_target};
        use luxifer_core::Handle;
        let start = BBox::new(0.0, 0.0, 100.0, 50.0);
        // NW nach oben-links ziehen: Anker ist die untere-rechte Ecke (100,50).
        let free = resize_target(start, Handle::Nw, [-100.0, -20.0]);
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

    /// Reproduziert den Resize-Aufschaukel-Bug und beweist den Snapshot-Fix:
    /// Ohne Snapshot verdoppelt sich die Größe bei jedem scale_selection_to von
    /// derselben start_box. MIT Restore auf den Snapshot bleibt sie stabil.
    #[test]
    fn resize_schaukelt_nicht_auf() {
        use luxifer_core::{AppState, BBox, Geo, Handle};
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        });
        s.selected = vec![0];
        let start = BBox::new(0.0, 0.0, 100.0, 100.0);

        // Snapshot der Ausgangsform.
        let orig: Vec<(usize, _)> = s
            .selected
            .iter()
            .map(|&i| (i, s.shapes[i].clone()))
            .collect();

        // Cursor bleibt konstant bei (150,100) — SE-Handle. 5 „Frames".
        let target = resize_target(start, Handle::Se, [150.0, 100.0]);
        for _ in 0..5 {
            // Vor jedem Schritt Snapshot wiederherstellen (wie in on_cursor_move).
            for (i, sh) in &orig {
                s.shapes[*i] = sh.clone();
            }
            s.scale_selection_to(start, target);
        }
        // Ergebnis muss 150×100 sein — NICHT aufgeschaukelt.
        let b = s.shapes[0].geo.bbox();
        assert!((b.w - 150.0).abs() < 1e-6, "Breite {} statt 150", b.w);
        assert!((b.h - 100.0).abs() < 1e-6, "Höhe {} statt 100", b.h);
    }
}
