//! Der Anwendungs-Zustand des nativen Editors: hält den Core-`AppState`, die
//! Kamera, das aktive Werkzeug und den GPU/egui-Kontext. Verbindet Eingaben mit
//! Core-Aufrufen (der Core bleibt die Wahrheit) und rendert Canvas + Panels.

use std::sync::Arc;

use luxifer_application::{AppError, EditorSession};
use luxifer_core::state::AppState;
use winit::event::{ElementState, WindowEvent};
use winit::window::Window;

use crate::camera::Camera;
use crate::canvas::CanvasState;
use crate::gpu::Gpu;
use crate::render::Renderer;
use crate::tools::{Drag, LaserUi};
use crate::ui::{
    self, GeoOpDialogState, ImageDialogState, LayerDialogState, PendingProjectAction,
    TextDialogState,
};

mod charon;
mod editor;
mod image;
mod laser;
mod laser_manager;
mod project;
mod settings;
mod text;

pub struct App {
    pub window: Arc<Window>,
    trim_cursor: Option<winit::window::CustomCursor>,
    pub session: EditorSession,
    /// Interaktions-/Kamerazustand des Canvas (Werkzeug, Geste, Cursor, Kamera).
    pub canvas: CanvasState,
    /// GPU-Ressourcen und Frame-Ablauf.
    renderer: Renderer,
    pub view: crate::tools::View,
    /// Projekt-/Versions-/Asset-Lebenszyklus (Application-Dienst).
    pub project: luxifer_application::ProjectService,
    /// Gecachte Projektliste; Dateisystemzugriff nur nach Projektänderungen.
    pub project_catalog: Vec<luxifer_core::ProjectInfo>,
    /// Gecachter Asset-Katalog; Metadaten werden nicht pro UI-Frame gelesen.
    pub asset_catalog: Vec<luxifer_core::AssetMeta>,
    /// Einmal geladene, abgeleitete Thumbnail-PNGs; kein Datei-I/O im Framepfad.
    pub asset_thumbnails: std::collections::BTreeMap<String, egui::TextureHandle>,
    thumbnail_runtime: image::ThumbnailRuntime,
    thumbnail_pending: std::collections::BTreeSet<String>,
    thumbnail_failed: std::collections::BTreeSet<String>,
    asset_import_runtime: image::AssetImportRuntime,
    pub asset_import_pending: bool,
    /// Seit dem letzten Projektwechsel importierte Quellen, damit auch
    /// vektorisierte Assets nach dem später vergebenen Projektnamen taggbar sind.
    pub session_asset_context: std::collections::BTreeSet<String>,
    /// Kurze Erfolgs-/Statusmeldungen als Toasts oben rechts (Fehler laufen
    /// über `app_error` und bleiben stehen).
    pub toasts: crate::ui::Toasts,
    /// Offene „Neues Projekt"-Maske (Name + Beschreibung) oder None.
    pub project_save_dialog: Option<crate::ui::ProjectSaveDialogState>,
    /// Persistente GUI-Einstellungen (Arbeitsplatz, Theme, Raster) — ADR 0002.
    pub ui_settings: luxifer_core::UiSettings,
    /// Periodischer Charon-Heartbeat läuft außerhalb des UI-Threads.
    charon_runtime: charon::CharonRuntime,
    pub charon_status: crate::ui::CharonTestStatus,
    pub charon_sync_error: Option<String>,
    /// Bestätigung vor unkoordinierter Verbindung bei ausgefallenem Charon.
    pub laser_uncoordinated_confirm: bool,
    pub laser_lease_force_confirm: Option<luxifer_application::CharonLease>,
    pub laser_lease_pending: bool,
    /// Laufender Start-Splash oder None (abgelaufen/übersprungen/deaktiviert).
    pub splash: Option<crate::ui::Splash>,
    /// Offener Einstellungen-Dialog (Entwurf) oder None.
    pub settings_dialog: Option<crate::ui::SettingsDialogState>,
    /// Eigenständige Laserprofil-/Controllerverwaltung.
    pub laser_manager: Option<crate::ui::LaserManagerState>,
    /// Präsentationszustand des Projektbrowsers (Auswahl, Drafts, Detail-Cache).
    pub project_browser: crate::ui::ProjectBrowserState,
    /// Kurzlebiger Entwurf der numerischen Auswahlgröße im zweiten Header.
    pub selection_size: crate::ui::SelectionSizeState,
    /// Persistente, noch nicht automatisch angewandte Charon-Revisionen.
    pub project_inbox: Vec<luxifer_application::InboxEntry>,
    project_integration: project::ProjectIntegrationRuntime,
    pub project_integration_pending: bool,
    /// Geöffneter read-only Vergleich einer Charon-Revision.
    pub revision_comparison: Option<crate::ui::RevisionComparisonState>,
    /// Material-Vorlage der Laser-Vorschau (Präsentationszustand).
    pub preview_material: crate::canvas::scene::PreviewMaterial,
    /// Leerfahrten in der Vorschau zeichnen (Präsentationszustand).
    pub preview_show_travel: bool,
    pub preview_show_laser_path: bool,
    pub preview_show_scan_offset: bool,
    preview_trace_key: Option<String>,
    preview_trace: Option<luxifer_core::ExecutionTrace>,
    pub laser: LaserUi,
    pub laser_backend: luxifer_application::LaserService,
    /// Zentraler, nutzerlesbarer Fehlerkanal der Anwendungsschicht.
    pub app_error: Option<AppError>,
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
    /// Verfügbare Font-Familien (einmalig gescannt, lazy).
    pub fonts: Vec<crate::fonts::FontFamily>,
    /// Gelesene Font-Dateien (Pfad → Bytes) für Vorschau und Commit.
    pub font_cache: std::collections::HashMap<std::path::PathBuf, std::sync::Arc<Vec<u8>>>,
}

impl App {
    pub fn new(
        window: Arc<Window>,
        gpu: Gpu,
        trim_cursor: Option<winit::window::CustomCursor>,
    ) -> Result<Self, AppError> {
        let egui_ctx = egui::Context::default();
        // Moderate, vom Monitor-DPI unabhängige Vergrößerung für lesbare
        // Beschriftungen und ausreichend große Trefferflächen.
        egui_ctx.set_zoom_factor(1.15);
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

        let state = AppState::new();
        let accent = state.active_color().unwrap_or([0x3B, 0x82, 0xF6]);
        // Ein erstes CLI-Argument wird als zu importierende Datei geladen
        // (praktisch fürs Testen: `luxifer-native datei.svg`).
        let auto_import = std::env::args().nth(1);

        let mut cam = Camera::new();
        cam.viewport = viewport;
        cam.fit_bbox([0.0, 0.0, state.bed_w_mm, state.bed_h_mm], 0.85);

        let ui_settings = luxifer_core::UiSettings::load();
        let laser_backend = luxifer_application::LaserService::load();
        let charon_runtime = charon::CharonRuntime::new(&ui_settings, &laser_backend.registry)?;
        let project_inbox = luxifer_application::list_inbox().unwrap_or_default();
        let project_integration = project::ProjectIntegrationRuntime::new()?;
        let project = luxifer_application::ProjectService::new();
        let project_catalog = project.list();
        if let Err(error) = luxifer_application::AssetService::cleanup_orphan_derived() {
            log::error!("Temporäre Crop-Assets bereinigen: {error}");
        }
        image::enrich_asset_tags_from_projects();
        let asset_catalog = luxifer_application::AssetService::list_visible()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let asset_thumbnails = Default::default();
        let thumbnail_runtime = image::ThumbnailRuntime::new()?;
        let asset_import_runtime = image::AssetImportRuntime::new()?;
        let mut app = Self {
            splash: ui_settings.show_splash.then(crate::ui::Splash::new),
            window,
            trim_cursor,
            session: EditorSession::new(state),
            canvas: {
                let mut canvas = CanvasState::new(cam);
                canvas.invert_marquee_direction = ui_settings.invert_marquee_direction;
                canvas
            },
            renderer,
            // Start-Ansicht per Env (Testhilfe): LUXI_TAB=laser|preview.
            view: match std::env::var("LUXI_TAB").as_deref() {
                Ok("laser") => crate::tools::View::Laser,
                Ok("preview") => crate::tools::View::Preview,
                _ => crate::tools::View::Design,
            },
            project,
            project_catalog,
            asset_catalog,
            asset_thumbnails,
            thumbnail_runtime,
            thumbnail_pending: Default::default(),
            thumbnail_failed: Default::default(),
            asset_import_runtime,
            asset_import_pending: false,
            session_asset_context: Default::default(),
            toasts: Default::default(),
            project_save_dialog: None,
            ui_settings,
            charon_runtime,
            charon_status: crate::ui::CharonTestStatus::Idle,
            charon_sync_error: None,
            laser_uncoordinated_confirm: false,
            laser_lease_force_confirm: None,
            laser_lease_pending: false,
            settings_dialog: None,
            laser_manager: None,
            project_browser: Default::default(),
            selection_size: Default::default(),
            project_inbox,
            project_integration,
            project_integration_pending: false,
            revision_comparison: None,
            preview_material: Default::default(),
            preview_show_travel: false,
            preview_show_laser_path: false,
            preview_show_scan_offset: false,
            preview_trace_key: None,
            preview_trace: None,
            laser: LaserUi::default(),
            laser_backend,
            app_error: None,
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
            font_cache: std::collections::HashMap::new(),
        };
        if app.view == crate::tools::View::Laser {
            app.canvas.tool = crate::tools::Tool::Select;
            app.canvas.laser_editable_layers = Some(Default::default());
            app.apply_active_laser_workspace();
        }
        if let Some(path) = auto_import {
            app.import_path(std::path::Path::new(&path));
            // Beim Auto-Import gleich füllen (Fill-Stresstest sichtbar machen).
            if std::env::var("LUXI_FILL").is_ok() {
                app.toggle_fill();
            }
        }
        // Startinhalt (Auto-Import) ist kein bearbeiteter
        // Nutzerstand: als sauber markieren, sonst schlägt der Dirty-Guard schon
        // beim ersten „Neu"/„Öffnen" an, obwohl es nichts zu verwerfen gibt.
        app.session.mark_saved();
        Ok(app)
    }

    pub fn window_event(&mut self, event: &WindowEvent) -> bool {
        // Splash aktiv: Klick/Taste überspringt ihn nur — nichts sickert zur
        // App oder zu egui durch. Alles andere (Resize, Cursor) läuft normal.
        if self.splash.is_some() {
            let skip = match event {
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                } => true,
                WindowEvent::KeyboardInput { event, .. } => event.state == ElementState::Pressed,
                _ => false,
            };
            if skip {
                self.splash = None;
                return true;
            }
        }
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
                // Logische Taste (Systemlayout), nicht die physische Position —
                // sonst sind Z/Y auf QWERTZ vertauscht (Strg+Z wäre Redo).
                if let Some(key) = crate::canvas::input::map_key(&event.logical_key) {
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
                if let Some(error) = out.error {
                    self.app_error = Some(error);
                }
            }
        }
        true
    }

    /// Tastatur-Eingabe für den Canvas ist blockiert, wenn ein egui-Textfeld
    /// den Fokus hat ODER ein modaler Dialog offen ist. `wants_keyboard_input`
    /// allein greift nur bei fokussiertem Feld; ein bloß geöffneter Dialog ohne
    /// aktives Feld ließe sonst Delete/Werkzeugwechsel/Undo durch und würde die
    /// Szene hinter dem Dialog verändern.
    fn input_blocked(&self) -> bool {
        self.egui_ctx.egui_wants_keyboard_input() || self.modal_open()
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
            || self.project_save_dialog.is_some()
            || self.settings_dialog.is_some()
            || self.laser_manager.is_some()
            || self.revision_comparison.is_some()
            || self.pending_project.is_some()
            || self.close_pending
            || self.laser_uncoordinated_confirm
            || self.laser_lease_force_confirm.is_some()
            || self.splash.is_some()
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
                self.cancel_bridge();
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
            S::SelectAll => self.session.select_all(),
            S::FitView => self.fit_view(),
            S::SelectTool(tool) => self.select_tool(tool),
            S::PanModifier(down) => self.canvas.space_down = down,
        }
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
            A::ResizeSelection { width, height } => self.resize_selection(width, height),
            A::Nest(gap) => self.nest(gap),
            A::NestFill(gap) => self.nest_fill(gap),
            A::PickColor(color) => self.pick_color(color),
            A::SelectShape(shape) => self.canvas.active_shape = shape,
            A::SelectTool(tool) => self.select_tool(tool),
            A::ToolAction(a) => self.begin_action(a),
            A::OpenTextDialog => self.open_text_dialog(),
            A::MirrorH => self.mirror_h(),
            A::MirrorV => self.mirror_v(),
            A::InsertCoasters(round) => self.insert_coasters(round),
            A::ToggleLayer(index, toggle) => self.toggle_layer(index, toggle),
            A::OpenLayerDialog(index) => self.open_layer_dialog(index),
            A::MoveLayer { from, to } => self.move_layer(from, to),
            A::OpenProjectSaveDialog => self.open_project_save_dialog(),
            A::NewBlankProject => self.project_new_blank(),
            A::OpenSettings => self.open_settings_dialog(),
            A::SaveProjectVersion => self.project_save_version(),
            A::OpenProject(name) => self.project_open(&name),
            A::DeleteProject(name) => self.project_delete(&name),
            A::ExportProject(name) => self.project_export(&name),
            A::RenameProject { from, to } => self.project_rename(&from, &to),
            A::OpenProjectVersion(id) => self.project_open_version(&id),
            A::DeleteProjectVersion(id) => self.project_delete_version(&id),
            A::DeferInboxRevision(id) => self.defer_inbox_revision(&id),
            A::ReconsiderInboxRevision(id) => self.reconsider_inbox_revision(&id),
            A::ApplyInboxRevision(id) => self.apply_inbox_revision(&id),
            A::ApplyAllInboxRevisions => self.apply_all_inbox_revisions(),
            A::ShowInboxComparison(id) => self.show_inbox_comparison(&id),
            A::SelectView(view) => {
                self.view = view;
                if view == crate::tools::View::Laser {
                    self.canvas.tool = crate::tools::Tool::Select;
                    self.canvas.laser_editable_layers = Some(Default::default());
                    self.apply_active_laser_workspace();
                } else {
                    self.canvas.laser_editable_layers = None;
                }
                self.renderer.invalidate_scene();
            }
            A::OpenAssetLibrary => {
                self.view = crate::tools::View::Projekt;
                self.project_browser.show_assets = true;
                self.project_browser.show_inbox = false;
                self.canvas.laser_editable_layers = None;
                self.renderer.invalidate_scene();
            }
            A::OpenCharonInbox => {
                self.view = crate::tools::View::Projekt;
                self.project_browser.show_inbox = true;
                self.project_browser.show_assets = false;
                self.canvas.laser_editable_layers = None;
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
            A::SetPreviewLaserPath(show) => {
                self.preview_show_laser_path = show;
                self.renderer.invalidate_scene();
            }
            A::SetPreviewScanOffset(show) => {
                self.preview_show_scan_offset = show;
                self.renderer.invalidate_scene();
            }
            A::Undo => self.undo(),
            A::Redo => self.redo(),
            A::Import => self.import_dialog(),
            A::ImportCatalogAsset(id) => self.import_catalog_asset(&id),
            A::DeleteCatalogAsset(id) => self.delete_catalog_asset(&id),
            A::RequestAssetThumbnail(id) => self.request_asset_thumbnail(&id),
            A::DismissError => self.app_error = None,
            A::LaserSelect(id) => self.laser_select(&id),
            A::LaserConnect => self.laser_connect(),
            A::LaserDisconnect => self.laser_disconnect(),
            A::LaserRun(action) => self.laser_run(action),
            A::LaserExport => self.laser_export(),
            A::LaserJog(dx, dy) => self.laser_jog(dx, dy),
            A::LaserHome => self.laser_home(),
            A::OpenLaserManager { create_new } => self.open_laser_manager(create_new),
        }
    }

    /// Werkzeugwechsel; ein schwebender Haltesteg-Entwurf verfällt dabei.
    fn select_tool(&mut self, tool: crate::tools::Tool) {
        if tool != crate::tools::Tool::Bridge {
            self.cancel_bridge();
        }
        self.canvas.tool = tool;
    }

    /// F-Shortcut: Kamera auf die Auswahl einpassen, sonst auf alle Objekte.
    fn fit_view(&mut self) {
        if let Some(b) = self.session.selection_bbox() {
            self.canvas.cam.fit_bbox([b.x, b.y, b.w, b.h], 0.85);
        } else {
            self.fit_all();
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

    pub fn egui_next_repaint(&self) -> Option<std::time::Instant> {
        self.renderer.next_repaint()
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
        let mut full = self.egui_ctx.clone().run_ui(raw, |ui| ui::build(ui, self));
        let shapes = std::mem::take(&mut full.shapes);
        let tris = self.egui_ctx.tessellate(shapes, full.pixels_per_point);

        // Szenenzustand (nur lesend) an den Renderer übergeben; er baut/lädt die
        // Caches und zeichnet Canvas + Overlay + egui.
        let image_dirty = std::mem::take(&mut self.image_dirty);
        if self.view == crate::tools::View::Preview {
            let trace_key = format!(
                "{}:{}:{:?}:{}:{:?}",
                self.session.render_rev(),
                self.laser.selection_only,
                self.laser.start_mode,
                self.laser.anchor,
                self.laser_backend.active_profile()
            );
            if self.preview_trace_key.as_deref() != Some(trace_key.as_str()) {
                let shapes: Vec<_> = if self.laser.selection_only {
                    self.session
                        .state()
                        .selected
                        .iter()
                        .filter_map(|&i| self.session.state().shapes.get(i).cloned())
                        .collect()
                } else {
                    self.session.state().shapes.clone()
                };
                self.preview_trace = self
                    .laser_backend
                    .execution_trace(
                        &shapes,
                        &self.session.state().layers,
                        self.laser.start_mode,
                        self.laser.anchor,
                    )
                    .ok();
                self.preview_trace_key = Some(trace_key);
            }
        }
        let scene = crate::render::FrameScene {
            session: &self.session,
            bed_origin: self
                .laser_backend
                .active_profile()
                .map(|profile| profile.origin)
                .unwrap_or_default(),
            cam: &self.canvas.cam,
            overlay: crate::canvas::overlay::OverlayInput {
                session: &self.session,
                accent: self.accent,
                drag: &self.canvas.drag,
                tool: self.canvas.tool,
                active_shape: self.canvas.active_shape,
                poly_pts: &self.canvas.poly_pts,
                bezier_nodes: &self.canvas.bezier_nodes,
                bridge: self.canvas.bridge,
                trim_preview: self.canvas.trim_preview.as_deref(),
                world_cursor: self.canvas.world(),
                cam_scale: self.canvas.cam.scale,
                invert_marquee_direction: self.canvas.invert_marquee_direction,
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
            preview_show_laser_path: self.preview_show_laser_path,
            preview_show_scan_offset: self.preview_show_scan_offset,
            preview_trace: self.preview_trace.as_ref(),
            grid_mm: self.ui_settings.grid_size_mm as f32,
        };
        self.renderer.draw_frame(&self.window, scene, full, tris);
        if self.canvas.cursor_over_canvas
            && self.canvas.tool == crate::tools::Tool::Trim
            && !self.canvas.space_down
            && !matches!(self.canvas.drag, Drag::Pan)
        {
            if let Some(cursor) = &self.trim_cursor {
                self.window.set_cursor(cursor.clone());
            }
        }
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
        let families = crate::fonts::list_font_families();
        let Some(face) = families.first().and_then(|fam| fam.faces.first()) else {
            eprintln!("Kein System-Font — Test übersprungen");
            return;
        };
        let data = std::fs::read(&face.path).expect("font lesen");
        let contours = text_to_contours(&data, "Hi", 20.0).expect("text_to_contours");
        assert!(!contours.is_empty(), "Text sollte Konturen ergeben");
        let mut s = AppState::new();
        let idxs = s.add_text_block(
            contours,
            TextMeta {
                text: "Hi".into(),
                font_path: face.path.to_string_lossy().to_string(),
                font_asset: None,
                size_mm: 20.0,
                ..Default::default()
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
