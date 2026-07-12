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
            // Start-Ansicht per Env (Testhilfe): LUXI_TAB=laser.
            view: if std::env::var("LUXI_TAB").as_deref() == Ok("laser") {
                crate::tools::View::Laser
            } else {
                crate::tools::View::Design
            },
            project: luxifer_application::ProjectService::new(),
            project_msg: String::new(),
            new_project_name: String::new(),
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

    /// Öffnet den Bildparameter-Dialog mit den aktuellen Werten des Bild-Shapes.
    pub fn open_image_dialog(&mut self, index: usize) {
        use luxifer_core::Geo;
        if let Some(Geo::Image { params, .. }) =
            self.session.state().shapes.get(index).map(|s| &s.geo)
        {
            self.image_dialog = Some(ImageDialogState {
                index,
                params: *params,
            });
        }
    }

    /// Übernimmt den Bildparameter-Entwurf über die Session (validiert, ein
    /// Undo-Schritt). Erfolg → Dialog schließen; Fehler → offen + Fehlerkanal.
    pub fn commit_image_dialog(&mut self) -> bool {
        let Some(st) = self.image_dialog.as_ref() else {
            return false;
        };
        let (index, params) = (st.index, st.params);
        match self.session.set_image_params(index, params) {
            Ok(()) => {
                self.image_dirty = true;
                true
            }
            Err(error) => {
                self.app_error = Some(error);
                false
            }
        }
    }

    /// Sofort-Aktion aus der Werkzeugleiste. Boolean/Fillet/Offset öffnen einen
    /// Parameterdialog (Variante bzw. Distanz/Radius); Bridge/Muster brauchen
    /// Interaktion/mehr Parameter und melden das vorerst.
    pub fn begin_action(&mut self, a: crate::tools::ToolAction) {
        use crate::tools::ToolAction as A;
        match a {
            A::Boolean => self.geo_op_dialog = Some(GeoOpDialogState::new(GeoOpKind::Boolean)),
            A::Fillet => self.geo_op_dialog = Some(GeoOpDialogState::new(GeoOpKind::Fillet)),
            A::Offset => self.geo_op_dialog = Some(GeoOpDialogState::new(GeoOpKind::Offset)),
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

    // ---- Projekt (README-3c) -------------------------------------------------

    /// Projekt öffnen. Dirty-Guard: liegen ungespeicherte Änderungen vor, wird
    /// erst bestätigt (Dialog), statt den Zustand kommentarlos zu ersetzen.
    pub fn project_open(&mut self, name: &str) {
        if self.session.is_dirty() {
            self.pending_project = Some(PendingProjectAction::Open(name.to_string()));
        } else {
            self.do_project_open(name);
        }
    }

    /// Neues Projekt anlegen. Dirty-Guard wie bei `project_open`.
    pub fn project_new(&mut self, name: &str) {
        if self.session.is_dirty() {
            self.pending_project = Some(PendingProjectAction::New(name.to_string()));
        } else {
            self.do_project_new(name);
        }
    }

    /// Führt die durch den Dirty-Guard bestätigte Aktion aus (verwirft die
    /// ungespeicherten Änderungen). Ohne wartende Aktion ein No-op.
    pub fn confirm_pending_project(&mut self) {
        match self.pending_project.take() {
            Some(PendingProjectAction::New(name)) => self.do_project_new(&name),
            Some(PendingProjectAction::Open(name)) => self.do_project_open(&name),
            None => {}
        }
    }

    /// Reaktion auf einen Schließen-Wunsch. Gibt true zurück, wenn sofort beendet
    /// werden darf; sonst öffnet der Dirty-Guard den Bestätigungsdialog.
    pub fn request_close(&mut self) -> bool {
        if self.session.is_dirty() {
            self.close_pending = true;
            self.window.request_redraw();
            false
        } else {
            true
        }
    }

    /// Der Nutzer hat im Close-Guard „Verwerfen" bestätigt.
    pub fn confirm_close(&mut self) {
        self.close_pending = false;
        self.should_exit = true;
    }

    /// Ob die Eventschleife das Programm beenden soll.
    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    /// Projekt öffnen: ersetzt den Editorzustand durch den geladenen.
    fn do_project_open(&mut self, name: &str) {
        match self.project.open(name) {
            Ok(state) => {
                self.session.replace_state(state);
                self.refresh_accent();
                self.image_dirty = true;
                // Der neue State führt seinen eigenen Revisionszähler; erzwinge
                // den Vertex-Neuaufbau, statt auf einen zufälligen
                // Zählervergleich zu vertrauen.
                self.renderer.invalidate_scene();
                self.fit_all();
                self.project_msg = format!("Geöffnet: {name}");
                self.view = crate::tools::View::Design;
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Neues Projekt aus dem aktuellen Canvas anlegen und speichern.
    fn do_project_new(&mut self, name: &str) {
        match self.project.new_project(self.session.state(), name) {
            Ok(()) => {
                self.session.mark_saved();
                self.project_msg = format!("Neues Projekt: {}", name.trim());
                self.view = crate::tools::View::Design;
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Projekt löschen (schließt es, wenn es das offene war).
    pub fn project_delete(&mut self, name: &str) {
        match self.project.delete(name) {
            Ok(()) => self.project_msg = format!("Gelöscht: {name}"),
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Projekt über einen nativen Zieldialog exportieren.
    pub fn project_export(&mut self, name: &str) {
        let Some(ziel) = rfd::FileDialog::new()
            .add_filter("LuxiFer-Projekt", &["luxi"])
            .set_file_name(format!("{name}.luxi"))
            .save_file()
        else {
            return; // Abbruch im Dialog: nichts tun.
        };
        match self.project.export(name, &ziel) {
            Ok(()) => self.project_msg = format!("Exportiert: {}", ziel.display()),
            Err(error) => self.app_error = Some(error),
        }
    }

    /// In-place speichern (Strg+S).
    pub fn project_save(&mut self) {
        match self.project.save(self.session.state()) {
            Ok(v) => {
                self.session.mark_saved();
                self.project_msg = format!("Gespeichert ({})", v.label);
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Als neue Version speichern (Shift+Strg+S).
    pub fn project_save_version(&mut self) {
        match self.project.save_version(self.session.state()) {
            Ok(v) => {
                self.session.mark_saved();
                self.project_msg = format!("Neue Version {}", v.label);
            }
            Err(error) => self.app_error = Some(error),
        }
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

    /// Öffnet den Text-Dialog zum Editieren eines bestehenden Textblocks
    /// (Doppelklick). Füllt Text/Größe/Font aus der am Shape gespeicherten Meta.
    pub fn open_text_editor(&mut self, index: usize) {
        if self.fonts.is_empty() {
            self.fonts = crate::fonts::list_fonts();
        }
        let Some(meta) = self
            .session
            .state()
            .shapes
            .get(index)
            .and_then(|s| s.text_meta.clone())
        else {
            return;
        };
        // Font in der Liste anhand des Pfads wiederfinden (sonst erster Font).
        let font_idx = self
            .fonts
            .iter()
            .position(|f| f.path.to_string_lossy() == meta.font_path)
            .or(if self.fonts.is_empty() { None } else { Some(0) });
        self.text_dialog = Some(TextDialogState {
            text: meta.text,
            size_mm: meta.size_mm,
            font_idx,
            edit_index: Some(index),
        });
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
        let edit_index = st.edit_index;
        match luxifer_core::text::text_to_contours(&font_data, &text, size) {
            Ok(contours) if !contours.is_empty() => {
                let meta = luxifer_core::TextMeta {
                    text,
                    font_path: font_path.to_string_lossy().to_string(),
                    size_mm: size,
                };
                if let Some(index) = edit_index {
                    // Bestehenden Textblock atomar ersetzen.
                    self.session.state_mut_for_migration().push_undo();
                    self.session
                        .state_mut_for_migration()
                        .replace_text_block(index, contours, meta);
                } else {
                    let idxs = self.session.add_text_block(contours, meta);
                    self.session.selected = idxs;
                }
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
            Err(error) => self.app_error = Some(error),
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
                Err(error) => self.app_error = Some(error),
            }
        }
    }

    pub fn laser_jog(&mut self, dx: f64, dy: f64) {
        let speed = self.laser.jog_speed;
        if let Err(error) = self.laser_backend.jog(dx, dy, speed) {
            self.app_error = Some(error);
        }
    }

    pub fn laser_home(&mut self) {
        let speed = self.laser.jog_speed;
        if let Err(error) = self.laser_backend.home(speed) {
            self.app_error = Some(error);
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

    /// Ob nach dem letzten Frame sofort weiter gezeichnet werden soll
    /// (egui-Animation/Interaktion läuft). Steuert die Render-Schleife.
    pub fn egui_wants_repaint(&self) -> bool {
        self.renderer.wants_repaint()
    }

    /// Laufende Bildrate (für die Statuszeile).
    pub fn fps(&self) -> f32 {
        self.renderer.fps()
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
            },
            image_dirty,
            preview: self.view == crate::tools::View::Preview,
            selection_only: self.laser.selection_only,
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
