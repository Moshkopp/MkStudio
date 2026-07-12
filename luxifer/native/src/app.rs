//! Der Anwendungs-Zustand des nativen Editors: hält den Core-`AppState`, die
//! Kamera, das aktive Werkzeug und den GPU/egui-Kontext. Verbindet Eingaben mit
//! Core-Aufrufen (der Core bleibt die Wahrheit) und rendert Canvas + Panels.

use std::sync::Arc;

use luxifer_application::{AppError, EditorSession, LayerParams, LayerToggle};
use luxifer_core::geometry::Geo;
use luxifer_core::state::AppState;
use winit::event::{ElementState, WindowEvent};
use winit::keyboard::PhysicalKey;
use winit::window::Window;

use crate::camera::Camera;
use crate::canvas::CanvasState;
use crate::gpu::Gpu;
use crate::render::Renderer;
use crate::tools::{Drag, LaserUi};
use crate::ui::{
    self, GeoOpDialogState, GeoOpKind, ImageDialogState, LayerDialogState, PendingProjectAction,
    TextDialogState,
};

mod image;
mod laser;
mod project;
mod text;

pub struct App {
    pub window: Arc<Window>,
    pub session: EditorSession,
    /// Interaktions-/Kamerazustand des Canvas (Werkzeug, Geste, Cursor, Kamera).
    pub canvas: CanvasState,
    /// GPU-Ressourcen und Frame-Ablauf.
    renderer: Renderer,
    pub view: crate::tools::View,
    /// Projekt-/Versions-/Asset-Lebenszyklus (Application-Dienst).
    pub project: luxifer_application::ProjectService,
    /// Kurze Erfolgs-/Statusmeldung zum Projekt (Fehler laufen über `app_error`).
    pub project_msg: String,
    /// Puffer für den „Neues Projekt"-Namen im Projekt-Reiter.
    pub new_project_name: String,
    /// Präsentationszustand des Projektbrowsers (Auswahl, Drafts, Detail-Cache).
    pub project_browser: crate::ui::ProjectBrowserState,
    /// Material-Vorlage der Laser-Vorschau (Präsentationszustand).
    pub preview_material: crate::canvas::scene::PreviewMaterial,
    /// Leerfahrten in der Vorschau zeichnen (Präsentationszustand).
    pub preview_show_travel: bool,
    pub laser: LaserUi,
    pub laser_backend: luxifer_application::LaserService,
    /// Letzte Laser-Rückmeldung (Statuszeile im Panel).
    pub laser_msg: String,
    /// Zentraler, nutzerlesbarer Fehlerkanal der Anwendungsschicht.
    pub app_error: Option<AppError>,
    /// Offener Laser-Einstellungen-Dialog (Profil-Bearbeitung) oder None.
    pub laser_settings: Option<luxifer_core::LaserProfile>,
    /// Aktive Zeichenfarbe für die Palette-Markierung (aus dem Core gespiegelt).
    pub accent: [u8; 3],
    /// egui-Kontext (billiger Arc-Clone; auch für die Fokus-Gate-Abfrage).
    egui_ctx: egui::Context,
    // Panel-Breiten, damit der Canvas den freien Bereich kennt.
    pub left_w: f32,
    pub right_w: f32,
    /// Externe Bild-Änderung (Import) → Textur-Sync im nächsten Frame nötig.
    image_dirty: bool,
    /// Offener Text-Dialog (Eingabe/Font/Größe) oder None.
    pub text_dialog: Option<TextDialogState>,
    /// Offener Layer-Parameter-Dialog (Doppelklick auf Ebene) oder None.
    pub layer_dialog: Option<LayerDialogState>,
    /// Offener Bildparameter-Dialog (Doppelklick auf Bild) oder None.
    pub image_dialog: Option<ImageDialogState>,
    /// Offener Geometrie-Parameterdialog (Boolean/Offset/Fillet) oder None.
    pub geo_op_dialog: Option<GeoOpDialogState>,
    /// Projektaktion, die auf Bestätigung wartet (Dirty-Guard) oder None.
    pub pending_project: Option<PendingProjectAction>,
    /// Ob der Nutzer das Fenster schließen will und der Dirty-Guard dafür einen
    /// Bestätigungsdialog zeigt.
    pub close_pending: bool,
    /// Vom Close-Guard gesetzt: die Eventschleife darf das Programm beenden.
    should_exit: bool,
    /// Verfügbare System-Fonts (einmalig gescannt, lazy).
    pub fonts: Vec<crate::fonts::FontEntry>,
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
        let viewport = [gpu.config.width as f32, gpu.config.height as f32];
        let renderer = Renderer::new(gpu, egui_state);

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
        cam.viewport = viewport;
        cam.fit_bbox([0.0, 0.0, state.bed_w_mm, state.bed_h_mm], 0.85);

        let mut app = Self {
            window,
            session: EditorSession::new(state),
            canvas: CanvasState::new(cam),
            renderer,
            // Start-Ansicht per Env (Testhilfe): LUXI_TAB=laser|preview.
            view: match std::env::var("LUXI_TAB").as_deref() {
                Ok("laser") => crate::tools::View::Laser,
                Ok("preview") => crate::tools::View::Preview,
                _ => crate::tools::View::Design,
            },
            project: luxifer_application::ProjectService::new(),
            project_msg: String::new(),
            new_project_name: String::new(),
            project_browser: Default::default(),
            preview_material: Default::default(),
            preview_show_travel: false,
            laser: LaserUi::default(),
            laser_backend: luxifer_application::LaserService::load(),
            laser_msg: String::new(),
            app_error: None,
            laser_settings: None,
            accent,
            egui_ctx,
            left_w: 0.0,
            right_w: 0.0,
            image_dirty: false,
            text_dialog: None,
            layer_dialog: None,
            image_dialog: None,
            geo_op_dialog: None,
            pending_project: None,
            close_pending: false,
            should_exit: false,
            fonts: Vec::new(),
        };
        if app.view == crate::tools::View::Laser {
            app.canvas.tool = crate::tools::Tool::Select;
            app.canvas.laser_editable_layers = Some(Default::default());
        }
        if let Some(path) = auto_import {
            app.import_path(std::path::Path::new(&path));
            // Beim Auto-Import gleich füllen (Fill-Stresstest sichtbar machen).
            if std::env::var("LUXI_FILL").is_ok() {
                app.toggle_fill();
            }
        }
        // Startinhalt (Demo-Shapes / Auto-Import) ist kein bearbeiteter
        // Nutzerstand: als sauber markieren, sonst schlägt der Dirty-Guard schon
        // beim ersten „Neu"/„Öffnen" an, obwohl es nichts zu verwerfen gibt.
        app.session.mark_saved();
        app
    }

    pub fn window_event(&mut self, event: &WindowEvent) -> bool {
        // egui zuerst — verschluckt es das Event (Panel getroffen), geht es nicht
        // an den Canvas.
        let resp = self.renderer.on_window_event(&self.window, event);
        // Modifier immer mitschreiben — auch wenn egui das Event konsumiert,
        // sonst geht der Shift-/Ctrl-Status beim Zeichnen/Resizen verloren.
        if let WindowEvent::ModifiersChanged(m) = event {
            self.canvas.ctrl_down = m.state().control_key();
            self.canvas.shift_down = m.state().shift_key();
        }

        if resp.consumed {
            // Trotzdem Cursor mitschreiben, damit Canvas-Koordinaten stimmen.
            if let WindowEvent::CursorMoved { position, .. } = event {
                self.canvas.cursor = [position.x as f32, position.y as f32];
            }
            return resp.repaint;
        }

        match event {
            WindowEvent::Resized(sz) => {
                self.renderer.resize(sz.width, sz.height);
                self.canvas.cam.viewport = [sz.width as f32, sz.height as f32];
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if self.view == crate::tools::View::Preview {
                    return true;
                }
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    if let Some(key) = crate::canvas::input::map_keycode(code) {
                        let mods = crate::tools::Mods {
                            ctrl: self.canvas.ctrl_down,
                            shift: self.canvas.shift_down,
                        };
                        let blocked = self.input_blocked();
                        if let Some(shortcut) =
                            crate::tools::resolve_shortcut(key, mods, pressed, blocked)
                        {
                            if self.view == crate::tools::View::Laser
                                && !matches!(shortcut, crate::tools::Shortcut::PanModifier(_))
                            {
                                return true;
                            }
                            self.apply_shortcut(shortcut);
                        }
                    }
                }
            }
            // Reine Canvas-Zeiger-Events übersetzt canvas::input. Erzeugt die
            // Geste ein Shape, frischt der Root die Zeichenfarbe auf; ein
            // Doppelklick auf ein Objekt öffnet den passenden Editor.
            _ => {
                if self.view == crate::tools::View::Preview {
                    self.canvas.handle_preview_pointer_event(event);
                    return true;
                }
                let out = self.canvas.handle_pointer_event(&mut self.session, event);
                if out.shape_added {
                    self.refresh_accent();
                }
                if let Some(index) = out.double_clicked {
                    self.edit_shape(index);
                }
            }
        }
        true
    }

    /// Öffnet den passenden Editor für einen doppelt angeklickten Shape:
    /// Bildparameter bei einem Bild-Objekt, Text-Editor bei einem Textblock.
    fn edit_shape(&mut self, index: usize) {
        use luxifer_core::Geo;
        let shapes = &self.session.state().shapes;
        let Some(hit) = shapes.get(index) else {
            return;
        };
        if matches!(hit.geo, Geo::Image { .. }) {
            self.open_image_dialog(index);
            return;
        }
        // Textblock: die Meta liegt am ersten Shape der Gruppe. Anker suchen.
        let anchor = if hit.text_meta.is_some() {
            Some(index)
        } else {
            hit.group_id.and_then(|g| {
                shapes
                    .iter()
                    .position(|s| s.group_id == Some(g) && s.text_meta.is_some())
            })
        };
        if let Some(a) = anchor {
            self.open_text_editor(a);
        }
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

    /// Tastatur-Eingabe für den Canvas ist blockiert, wenn ein egui-Textfeld
    /// den Fokus hat ODER ein modaler Dialog offen ist. `wants_keyboard_input`
    /// allein greift nur bei fokussiertem Feld; ein bloß geöffneter Dialog ohne
    /// aktives Feld ließe sonst Delete/Werkzeugwechsel/Undo durch und würde die
    /// Szene hinter dem Dialog verändern.
    fn input_blocked(&self) -> bool {
        self.egui_ctx.wants_keyboard_input() || self.modal_open()
    }

    /// Ob gerade ein modaler Dialog offen ist. Solange das gilt, ignoriert der
    /// UI-Root die von den (nicht modalen egui-)Panels gelieferten Aktionen —
    /// sonst könnte man Undo/Werkzeugwechsel/Text… auslösen, während ein Dialog
    /// seinen Entwurf bearbeitet, und die Szene würde sich darunter ändern.
    pub fn modal_open(&self) -> bool {
        self.layer_dialog.is_some()
            || self.image_dialog.is_some()
            || self.geo_op_dialog.is_some()
            || self.text_dialog.is_some()
            || self.laser_settings.is_some()
            || self.pending_project.is_some()
            || self.close_pending
    }

    /// Führt eine typisierte Tastatur-Aktion aus. Die Zuordnung Taste→Aktion
    /// (inklusive Fokusregeln) liegt in `tools::resolve_shortcut`; hier steht
    /// nur die Ausführung über die Session/den UI-Zustand.
    fn apply_shortcut(&mut self, shortcut: crate::tools::Shortcut) {
        use crate::tools::Shortcut as S;
        match shortcut {
            S::Save => self.project_save(),
            S::SaveVersion => self.project_save_version(),
            S::Delete => {
                if !self.session.selected.is_empty() {
                    self.delete_selected();
                }
            }
            S::Cancel => {
                self.canvas.poly_pts.clear();
                self.canvas.bezier_nodes.clear();
                if self.session.edit_active() {
                    self.session.cancel_edit();
                    self.canvas.drag = Drag::None;
                } else {
                    self.session.clear_selection();
                }
            }
            S::FinishPolygon => {
                if self.canvas.finish_point_path(&mut self.session, true) {
                    self.refresh_accent();
                }
            }
            S::Undo => self.undo(),
            S::Redo => self.redo(),
            S::SelectTool(tool) => self.canvas.tool = tool,
            S::PanModifier(down) => self.canvas.space_down = down,
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
    pub fn selection_count(&self) -> usize {
        self.session.selected.len()
    }

    /// Führt eine typisierte UI-Absicht aus (ADR 0011: „UI erzeugt Absicht, App
    /// koordiniert"). Die eigentliche Fachlogik liegt weiterhin in den
    /// bestehenden Methoden bzw. der `EditorSession`.
    pub fn dispatch(&mut self, action: crate::ui::UiAction) {
        use crate::ui::UiAction as A;
        // Bei offenem modalem Dialog ignoriert der Root die (nicht modalen)
        // Panel-Aktionen — nur die Fehleranzeige lässt sich noch schließen.
        if self.modal_open() && action != A::DismissError {
            return;
        }
        match action {
            A::Align(kind) => self.align(kind),
            A::Distribute(kind) => self.distribute(kind),
            A::Group => self.group(),
            A::Ungroup => self.ungroup(),
            A::Nest(gap) => self.nest(gap),
            A::NestFill(gap) => self.nest_fill(gap),
            A::PickColor(color) => self.pick_color(color),
            A::SelectShape(shape) => self.canvas.active_shape = shape,
            A::SelectTool(tool) => self.canvas.tool = tool,
            A::ToolAction(a) => self.begin_action(a),
            A::OpenTextDialog => self.open_text_dialog(),
            A::MirrorH => self.mirror_h(),
            A::MirrorV => self.mirror_v(),
            A::InsertCoasters(round) => self.insert_coasters(round),
            A::ToggleLayer(index, toggle) => self.toggle_layer(index, toggle),
            A::OpenLayerDialog(index) => self.open_layer_dialog(index),
            A::MoveLayer { from, to } => self.move_layer(from, to),
            A::NewProject => {
                // Draft-Lebenszyklus liegt am Root: Namen auslesen, anlegen, leeren.
                let name = self.new_project_name.clone();
                self.project_new(&name);
                self.new_project_name.clear();
            }
            A::SaveProject => self.project_save(),
            A::SaveProjectVersion => self.project_save_version(),
            A::OpenProject(name) => self.project_open(&name),
            A::DeleteProject(name) => self.project_delete(&name),
            A::ExportProject(name) => self.project_export(&name),
            A::RenameProject { from, to } => self.project_rename(&from, &to),
            A::OpenProjectVersion(id) => self.project_open_version(&id),
            A::DeleteProjectVersion(id) => self.project_delete_version(&id),
            A::SelectView(view) => {
                self.view = view;
                if view == crate::tools::View::Laser {
                    self.canvas.tool = crate::tools::Tool::Select;
                    self.canvas.laser_editable_layers = Some(Default::default());
                } else {
                    self.canvas.laser_editable_layers = None;
                }
                self.renderer.invalidate_scene();
            }
            A::ToggleLaserEditLayer(index) => {
                if let Some(editable) = self.canvas.laser_editable_layers.as_mut() {
                    if !editable.insert(index) {
                        editable.remove(&index);
                    }
                }
            }
            A::SelectPreviewMaterial(material) => {
                self.preview_material = material;
                // Der Preview-Cache hängt an der Render-Revision — Materialwechsel
                // muss den Vertex-/Textur-Aufbau erzwingen.
                self.renderer.invalidate_scene();
            }
            A::SetPreviewTravel(show) => {
                self.preview_show_travel = show;
                self.renderer.invalidate_scene();
            }
            A::Undo => self.undo(),
            A::Redo => self.redo(),
            A::ImportVector => self.import_dialog(),
            A::ImportImage => self.import_image_dialog(),
            A::ImportPath(path) => self.import_path(&path),
            A::DismissError => self.app_error = None,
            A::LaserSelect(id) => self.laser_select(&id),
            A::LaserRun(action) => self.laser_run(action),
            A::LaserExport => self.laser_export(),
            A::LaserJog(dx, dy) => self.laser_jog(dx, dy),
            A::LaserHome => self.laser_home(),
            A::OpenLaserSettings { edit_active } => self.open_laser_settings(edit_active),
        }
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

    /// Sofort-Aktion aus der Werkzeugleiste. Boolean/Fillet/Offset/Muster
    /// öffnen einen Parameterdialog; Bridge braucht eine eigene Geste und
    /// meldet das vorerst.
    pub fn begin_action(&mut self, a: crate::tools::ToolAction) {
        use crate::tools::ToolAction as A;
        match a {
            A::Boolean => self.geo_op_dialog = Some(GeoOpDialogState::new(GeoOpKind::Boolean)),
            A::Fillet => self.geo_op_dialog = Some(GeoOpDialogState::new(GeoOpKind::Fillet)),
            A::Offset => self.geo_op_dialog = Some(GeoOpDialogState::new(GeoOpKind::Offset)),
            A::PatternFill => {
                self.geo_op_dialog = Some(GeoOpDialogState::new(GeoOpKind::PatternFill))
            }
            A::Bridge => {
                self.app_error = Some(AppError::new(
                    "not_migrated",
                    "Haltestege sind noch nicht migriert.",
                ))
            }
        }
    }

    /// Führt die im Geometrie-Dialog parametrierte Operation über die Session
    /// aus. Erfolg → Dialog schließen; Auswahl-/Voraussetzungsfehler → offen +
    /// Fehlerkanal.
    pub fn commit_geo_op(&mut self) -> bool {
        let Some(st) = self.geo_op_dialog.as_ref() else {
            return false;
        };
        let result = match st.kind {
            GeoOpKind::Boolean => self.session.boolean(st.bool_op),
            GeoOpKind::Offset => self.session.offset(st.distance),
            GeoOpKind::Fillet => self.session.fillet(st.radius),
            GeoOpKind::PatternFill => self.session.pattern_fill(&st.fill),
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                self.app_error = Some(error);
                false
            }
        }
    }

    fn report(&mut self, result: Result<(), AppError>) {
        if let Err(error) = result {
            self.app_error = Some(error);
        }
    }

    /// Kamera auf die BBox aller Shapes einpassen (Fallback: Tisch).
    fn fit_all(&mut self) {
        let b =
            luxifer_core::geometry::BBox::union_all(self.session.shapes.iter().map(|s| s.bbox()));
        if let Some(b) = b {
            self.canvas.cam.fit_bbox([b.x, b.y, b.w, b.h], 0.85);
        } else {
            self.canvas.cam.fit_bbox(
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

    /// Ob nach dem letzten Frame sofort weiter gezeichnet werden soll
    /// (egui-Animation/Interaktion läuft). Steuert die Render-Schleife.
    pub fn egui_wants_repaint(&self) -> bool {
        self.renderer.wants_repaint()
    }

    /// Laufende Bildrate (für die Statuszeile).
    pub fn fps(&self) -> f32 {
        self.renderer.fps()
    }

    /// Legende des letzten Preview-Aufbaus (für den Preview-Reiter).
    pub fn preview_legend(&self) -> Option<&crate::canvas::scene::PreviewLegend> {
        self.renderer.preview_legend()
    }

    pub fn render(&mut self) {
        // egui-Frame bauen (Panels): die Closure braucht `&mut App`, daher hier
        // im Root. Der egui-Kontext ist ein billiger Arc-Clone.
        let raw = self.renderer.take_egui_input(&self.window);
        let mut full = self.egui_ctx.clone().run(raw, |ctx| ui::build(ctx, self));
        let shapes = std::mem::take(&mut full.shapes);
        let tris = self.egui_ctx.tessellate(shapes, full.pixels_per_point);

        // Szenenzustand (nur lesend) an den Renderer übergeben; er baut/lädt die
        // Caches und zeichnet Canvas + Overlay + egui.
        let image_dirty = std::mem::take(&mut self.image_dirty);
        let scene = crate::render::FrameScene {
            session: &self.session,
            cam: &self.canvas.cam,
            overlay: crate::canvas::overlay::OverlayInput {
                session: &self.session,
                accent: self.accent,
                drag: &self.canvas.drag,
                tool: self.canvas.tool,
                active_shape: self.canvas.active_shape,
                poly_pts: &self.canvas.poly_pts,
                bezier_nodes: &self.canvas.bezier_nodes,
                world_cursor: self.canvas.world(),
                cam_scale: self.canvas.cam.scale,
                // Startmarker nur im Laser-Tab: Dort wird der Job platziert.
                job_start: if self.view == crate::tools::View::Laser {
                    self.session
                        .job_start_marker(
                            self.laser.selection_only,
                            self.laser.start_mode,
                            luxifer_core::Anchor::from_index(self.laser.anchor),
                        )
                        .map(|(x, y)| [x, y])
                } else {
                    None
                },
            },
            image_dirty,
            preview: self.view == crate::tools::View::Preview,
            selection_only: self.laser.selection_only,
            preview_material: self.preview_material,
            preview_show_travel: self.preview_show_travel,
        };
        self.renderer.draw_frame(&self.window, scene, full, tris);
    }
}

#[cfg(test)]
mod tests {
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
        let target = luxifer_core::resize_to_cursor(start, Handle::Se, [150.0, 100.0]);
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
