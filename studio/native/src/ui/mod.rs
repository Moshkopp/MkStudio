//! egui-Oberfläche: Komposition der Panels/Dialoge und das Theme. Bewusst nah an
//! den frischen Svelte-Designs (aktive-Farbe-Markierung, klare Sektionen). Alle
//! Aktionen laufen über den Core — die Panels halten keinen eigenen Wahrheits-
//! Zustand.
//!
//! Die einzelnen Panels und Dialoge liegen in den Untermodulen. Nur dieser
//! Root kennt `App`: Er liest Werte, führt Draft-Lebenszyklen und dispatcht die
//! von den Panels gelieferten `UiAction`s (ADR 0011). Die Panels/Dialoge selbst
//! (inklusive `laserpanel`) erhalten `&`-Sichten bzw. `&mut`-Entwürfe und geben
//! Absichten zurück — sie greifen nicht mehr auf `App` zu.

mod action;
mod arrange;
mod dialogs;
mod layers;
mod palette;
mod preview;
mod project;
mod ruler;
mod splash;
mod state;
mod status;
mod toast;
mod tools;
mod topbar;

pub use action::UiAction;
pub(crate) use project::preview_from_state;
pub use splash::Splash;
pub use state::{
    BackupRestoreConfirmation, CachedProjectDetail, CropKind, GeoOpDialogState, GeoOpKind,
    HubTestStatus, ImageDialogPage, ImageDialogState, LaserManagerState, LaserManagerTab,
    LayerDialogState, LayerManagerState, MaterialManagerState, PendingProjectAction,
    ProjectBrowserState, ProjectSaveDialogState, RevisionComparisonState, SavedOriginDialogState,
    SelectionSizeState, SettingsDialogState, SettingsSection, TextDialogState,
};
pub use toast::Toasts;

use egui::Color32;

use crate::app::App;
use crate::laserpanel;

/// Einheitliche Kantenlänge aller kompakten Icon-Buttons.
pub(super) const ICON_BUTTON_SIDE: f32 = 34.0;

/// RGB-Tripel → egui-Farbe. Geteilter Helfer für die Panels.
pub(super) fn c32(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

pub fn build(ui: &mut egui::Ui, app: &mut App) {
    use crate::tools::View;
    // Oben: globale Aktionen | Ansichten | kompakte Systemzustände.
    let view = app.view;
    // Sicherheit: Ein laufender Achsen-Dauerlauf wird sofort gestoppt, sobald
    // der Laser-Tab nicht mehr sichtbar ist (das Panel meldet dann keinen
    // Halte-Wunsch mehr). Ohne das könnte ein Tab-Wechsel die Achse laufen
    // lassen.
    if view != View::Laser && app.laser_hold.is_some() {
        app.laser_hold_cancel();
    }
    let project_name = app
        .project
        .open_name()
        .unwrap_or("— (ungespeichert)")
        .to_string();
    let inbox_count = app
        .project_inbox
        .iter()
        .filter(|entry| entry.status == studio_application::InboxStatus::PendingReview)
        .count();
    let topbar_actions = egui::Panel::top("topbar")
        .frame(
            egui::Frame::new()
                .fill(c32(app.ui_settings.theme.palette.toolbar))
                .inner_margin(egui::Margin::symmetric(8, 5))
                .stroke(egui::Stroke::new(
                    1.0,
                    c32(app.ui_settings.theme.palette.border),
                )),
        )
        .show(ui, |ui| {
            topbar::topbar(
                ui,
                view,
                inbox_count,
                app.ui_settings.hub_enabled,
                &app.hub_status,
                &app.laser_backend.registry,
                app.laser_backend.is_connected(),
            )
        })
        .inner;
    for action in topbar_actions {
        app.dispatch(action);
    }

    // Anwendungsfehler sind Overlays und dürfen das Arbeitslayout nicht
    // verschieben. Technische Details bleiben im Fehlerobjekt/Log; im Toast
    // steht die handlungsorientierte Meldung.
    if let Some(error) = app.app_error.take() {
        log::error!(
            "application error [{}]: {}{}",
            error.code(),
            error.message(),
            error
                .details()
                .map(|details| format!(" ({details})"))
                .unwrap_or_default()
        );
        app.toasts.error(error.message().to_owned());
    }

    // Zweite Kopfzeile: Anordnen (Ausrichten/Verteilen/Gruppieren/Nesting) — nur
    // im Design-Reiter. Wie in der Tauri-App liegt das im Kopf. Pilot der
    // UiAction-Grenze: Das Panel liefert Absichten, der Root führt sie aus.
    if app.view == View::Design {
        let selection = app.selection_count();
        let selection_bbox = app.session.selection_bbox();
        let actions = egui::Panel::top("arrange")
            .show(ui, |ui| {
                ui.add_space(3.0);
                let a =
                    arrange::arrange_bar(ui, selection, selection_bbox, &mut app.selection_size);
                ui.add_space(3.0);
                a
            })
            .inner;
        for action in actions {
            app.dispatch(action);
        }
    }

    // Statuszeile unten (rein lesend). Meldungen laufen über die Toasts.
    let (fps, tool_label, shapes) = (app.fps(), app.canvas.tool.label(), app.session.shapes.len());
    egui::Panel::bottom("status").show(ui, |ui| {
        status::status_bar(ui, fps, tool_label, shapes, &project_name);
    });

    match app.view {
        View::Projekt => {
            app.left_w = 0.0;
            app.right_w = 0.0;
            let open_name = app.project.open_name().map(|s| s.to_string());
            sync_project_browser(app, open_name.as_deref());
            let dirty = app.session.is_dirty();
            let projects = &app.project_catalog;
            let assets = &app.asset_catalog;
            let asset_thumbnails = &app.asset_thumbnails;
            let inbox = &app.project_inbox;
            let integration_pending = app.project_integration_pending;
            let asset_import_pending = app.asset_import_pending;
            let browser = &mut app.project_browser;
            let actions = egui::CentralPanel::default()
                .show(ui, |ui| {
                    project::project_browser(
                        ui,
                        browser,
                        projects,
                        inbox,
                        (assets, asset_thumbnails),
                        (open_name.as_deref(), dirty),
                        (integration_pending, asset_import_pending),
                    )
                })
                .inner;
            for action in actions {
                app.dispatch(action);
            }
        }
        View::Preview => {
            app.left_w = 0.0;
            // Rechts: Material-Vorlage + Legende. Die Legende entsteht beim
            // Preview-Vertex-Aufbau im selben Frame NACH der UI — solange sie
            // fehlt, einmal nachzeichnen lassen.
            if app.preview_legend().is_none() {
                ui.request_repaint();
            }
            let material = app.preview_material;
            let show_travel = app.preview_show_travel;
            let show_laser_path = app.preview_show_laser_path;
            let show_scan_offset = app.preview_show_scan_offset;
            let right = egui::Panel::right("preview_panel")
                .default_size(240.0)
                .size_range(200.0..=320.0)
                .resizable(true)
                .show(ui, |ui| {
                    preview::preview_panel(
                        ui,
                        material,
                        show_travel,
                        show_laser_path,
                        show_scan_offset,
                        app.preview_legend(),
                    )
                });
            app.right_w = right.response.rect.width();
            for action in right.inner {
                app.dispatch(action);
            }
        }
        View::Design | View::Laser => {
            let cur_tool = app.canvas.tool;
            let is_laser = app.view == View::Laser;
            let layer_rows: Vec<layers::LayerRow> = layer_rows(app);
            let laser_editable = app.canvas.laser_editable_layers.clone().unwrap_or_default();
            if is_laser {
                // Links: Ebenenliste + Positionsfreigabe in eigenem Panel —
                // bei vielen Ebenen teilt sie sich sonst gequetscht die rechte
                // Spalte mit dem Laser-Bedienpanel.
                let left = egui::Panel::left("laser_layers")
                    .default_size(300.0)
                    .size_range(260.0..=420.0)
                    .resizable(true)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.add_space(6.0);
                                let mut actions = layers::layers_panel(ui, &layer_rows);
                                actions.extend(layers::laser_edit_layers(
                                    ui,
                                    &layer_rows,
                                    &laser_editable,
                                ));
                                actions
                            })
                            .inner
                    });
                app.left_w = left.response.rect.width();
                for action in left.inner {
                    app.dispatch(action);
                }
            } else {
                let left = egui::Panel::left("tools")
                    // Zwei 34-pt-Buttons + Abstand + Panel-Innenränder
                    // brauchen bei DPI-Rundung etwas Reserve. 88 pt lagen
                    // exakt auf der rechnerischen Untergrenze und schnitten
                    // die rechte Buttonkante optisch an.
                    .default_size(100.0)
                    .min_size(100.0)
                    .max_size(100.0)
                    .resizable(false)
                    .show(ui, |ui| {
                        tools::tools_panel(ui, cur_tool, app.selection_count())
                    });
                app.left_w = left.response.rect.width();
                for action in left.inner {
                    app.dispatch(action);
                }
            }

            // Sichten vorab ableiten, damit die Panels keinen App-/Backend-
            // Zugriff brauchen.
            let laser_view = if is_laser {
                Some(laser_view(app))
            } else {
                None
            };
            // Der Inspector-Inhalt ist länger als kleine Fenster: vertikal
            // scrollen, ohne die Breite schrumpfen zu lassen (auto_shrink
            // false hält die Zeilen exakt auf Panelbreite).
            let right = egui::Panel::right("inspector")
                .default_size(340.0)
                .size_range(300.0..=460.0)
                .resizable(true)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add_space(6.0);
                            if let Some(view) = &laser_view {
                                laserpanel::show(ui, view, &mut app.laser)
                            } else {
                                layers::layers_panel(ui, &layer_rows)
                            }
                        })
                        .inner
                });
            app.right_w = right.response.rect.width();
            for action in right.inner {
                app.dispatch(action);
            }

            // Die Layerfarben schweben direkt über dem Canvas. Sie reservieren
            // keine eigene Panelhöhe und bleiben eine unmittelbare
            // Canvas-Aktion statt eines separaten UI-Bereichs.
            if !is_laser {
                let accent = app.accent;
                let canvas_rect = ui.available_rect_before_wrap();

                // Polygonvarianten schweben oben mittig über der Zeichenfläche
                // und verändern dadurch weder Header- noch Canvas-Geometrie.
                if app.canvas.tool == crate::tools::Tool::Polygon {
                    let active_shape = app.canvas.active_shape;
                    let actions = egui::Area::new(egui::Id::new("canvas_polygon_shapes"))
                        .order(egui::Order::Foreground)
                        .pivot(egui::Align2::CENTER_TOP)
                        .fixed_pos(egui::pos2(
                            canvas_rect.center().x,
                            canvas_rect.top() + ruler::TOP_THICKNESS + 16.0,
                        ))
                        .show(ui, |ui| palette::shape_picker(ui, active_shape))
                        .inner;
                    for action in actions {
                        app.dispatch(action);
                    }
                }

                let actions = egui::Area::new(egui::Id::new("canvas_palette"))
                    .order(egui::Order::Foreground)
                    .pivot(egui::Align2::CENTER_BOTTOM)
                    .fixed_pos(egui::pos2(
                        canvas_rect.center().x,
                        canvas_rect.bottom() - 16.0,
                    ))
                    .show(ui, |ui| palette::palette_panel(ui, accent))
                    .inner;
                for action in actions {
                    app.dispatch(action);
                }
            }
        }
    }

    // Lineale am Canvas-Rand — nach den Panels, damit `available_rect` genau
    // den freien Canvas-Bereich meint. Vorschau/Projekt bleiben linealfrei.
    if matches!(app.view, View::Design | View::Laser) {
        let canvas_rect = ui.available_rect_before_wrap();
        app.canvas.cursor_over_canvas = ui
            .input(|input| input.pointer.hover_pos())
            .is_some_and(|pointer| canvas_rect.contains(pointer));
        if app.canvas.cursor_over_canvas {
            ui.ctx()
                .set_cursor_icon(app.canvas.hover_cursor(&app.session));
        }
        let profile = app.laser_backend.active_profile();
        let origin = profile.map(|p| p.origin).unwrap_or_default();
        let bed = profile
            .map(|p| p.bed_mm)
            .unwrap_or((app.session.bed_w_mm, app.session.bed_h_mm));
        ruler::rulers(
            ui,
            &app.canvas.cam,
            app.canvas.cursor,
            app.ui_settings.theme.accent.hue,
            origin,
            bed,
        );
        // Beschriftungen der Laser-Fadenkreuze (ADR 0020 §B): Text an festen,
        // versetzten Quadranten, mit Hintergrund für Lesbarkeit auf hellem und
        // dunklem Canvas. Die Kreuze selbst zeichnet das GPU-Overlay.
        if app.view == View::Laser {
            laser_marker_labels(ui, app, canvas_rect);
        }
    }

    // Haltesteg-Entwurf: schwebendes Eingabefeld am Linienende — Breite live
    // anpassen (Bandkanten wachsen im Overlay mit), Einfügen bestätigt.
    if app.view == View::Design && app.canvas.tool == crate::tools::Tool::Bridge {
        if let Some(mut draft) = app.canvas.bridge {
            let ppp = ui.ctx().pixels_per_point();
            let end = app.canvas.cam.world_to_screen(draft.p1);
            let pos = egui::pos2(end[0] / ppp + 14.0, end[1] / ppp + 10.0);
            let mut commit = false;
            let mut cancel = false;
            egui::Area::new(egui::Id::new("bridge_input"))
                .order(egui::Order::Foreground)
                .fixed_pos(pos)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Breite");
                            ui.add(
                                egui::DragValue::new(&mut draft.width)
                                    .range(0.1..=100.0)
                                    .speed(0.1)
                                    .fixed_decimals(1)
                                    .suffix(" mm"),
                            );
                            if ui.button("Einfügen").clicked() {
                                commit = true;
                            }
                            if ui.button("✖").on_hover_text("Verwerfen (Esc)").clicked() {
                                cancel = true;
                            }
                        });
                    });
                });
            app.canvas.bridge = Some(draft);
            if commit {
                app.commit_bridge();
            } else if cancel {
                app.cancel_bridge();
            }
        }
    }

    // Ein gemeinsames Backdrop für alle echten Dialoge. Beim Einstellen wird
    // direkt der Draft gelesen, damit der Alpha-Regler live reagiert.
    let has_dialog = app.text_dialog.is_some()
        || app.layer_dialog.is_some()
        || app.image_dialog.is_some()
        || app.geo_op_dialog.is_some()
        || app.settings_dialog.is_some()
        || app.laser_manager.is_some()
        || app.material_manager.is_some()
        || app.layer_manager.is_some()
        || app.project_save_dialog.is_some()
        || app.revision_comparison.is_some()
        || app.pending_project.is_some()
        || app.close_pending;
    let has_dialog = has_dialog
        || app.laser_uncoordinated_confirm
        || app.laser_lease_force_confirm.is_some()
        || app.saved_origin_dialog.is_some();
    if has_dialog {
        let alpha = app
            .settings_dialog
            .as_ref()
            .map(|state| state.draft.modal_backdrop_alpha)
            .unwrap_or(app.ui_settings.modal_backdrop_alpha);
        dialogs::modal_backdrop(ui, alpha);
    }

    // Text-Dialog: Vorschau-Konturen vorm Zeichnen aktualisieren (Cache im
    // Entwurf), dann Entwurf als &mut und die Familien-Liste nur lesend.
    if app.text_dialog.is_some() {
        app.update_text_preview();
        let (state, families) = (app.text_dialog.as_mut().unwrap(), &app.fonts);
        match dialogs::text_dialog_window(ui, state, families) {
            dialogs::DialogOutcome::None => {}
            dialogs::DialogOutcome::Commit => {
                if app.commit_text() {
                    app.text_dialog = None;
                }
            }
            dialogs::DialogOutcome::Cancel => app.text_dialog = None,
        }
        // Font-Import wurde im Dialog angefordert; der Root öffnet den
        // (blockierenden) Datei-Dialog außerhalb des &mut-Borrows.
        if app
            .text_dialog
            .as_mut()
            .map(|state| std::mem::take(&mut state.request_font_import))
            .unwrap_or(false)
        {
            app.import_font_dialog();
        }
    }

    // Layer-Dialog: der Entwurf wird als &mut gereicht, der Root behandelt das
    // Ergebnis (Übernahme über die validierende Session bzw. Verwerfen).
    if let Some(state) = app.layer_dialog.as_mut() {
        match dialogs::layer_dialog_window(ui, &mut state.params) {
            dialogs::LayerDialogOutcome::None => {}
            dialogs::LayerDialogOutcome::Commit => {
                if app.commit_layer_dialog() {
                    app.layer_dialog = None;
                }
            }
            dialogs::LayerDialogOutcome::Cancel => app.layer_dialog = None,
        }
    }

    // Bildparameter-Dialog: Entwurf als &mut; Speichern über die validierende
    // Session, Abbrechen verwirft.
    if app.image_dialog.is_some() {
        app.update_image_dialog_preview();
        let state = app.image_dialog.as_mut().unwrap();
        match dialogs::image_dialog_window(ui, state) {
            dialogs::ImageDialogOutcome::None => {}
            dialogs::ImageDialogOutcome::Save => {
                if app.commit_image_dialog() {
                    app.image_dialog = None;
                }
            }
            // Trace lässt den Dialog offen: Regler nachziehen und erneut
            // vektorisieren ist der übliche Arbeitsfluss.
            dialogs::ImageDialogOutcome::Trace => app.trace_image_dialog(),
            dialogs::ImageDialogOutcome::Crop => {
                if app.crop_image_dialog() {
                    app.image_dialog = None;
                }
            }
            dialogs::ImageDialogOutcome::Cancel => app.image_dialog = None,
        }
    }

    // Geometrie-Parameterdialog (Boolean/Offset/Fillet): Entwurf als &mut,
    // Ausführung über die Session.
    if let Some(st) = app.geo_op_dialog.as_mut() {
        match dialogs::geo_op_dialog_window(ui, st) {
            dialogs::DialogOutcome::None => {}
            dialogs::DialogOutcome::Commit => {
                if app.commit_geo_op() {
                    app.geo_op_dialog = None;
                }
            }
            dialogs::DialogOutcome::Cancel => app.geo_op_dialog = None,
        }
    }

    // Softwareweite Einstellungen. Geräteprofile leben getrennt im Manager.
    if app.settings_dialog.is_some() {
        let st = app.settings_dialog.as_mut().unwrap();
        match dialogs::settings_dialog_window(ui, st) {
            dialogs::SettingsOutcome::None => {}
            dialogs::SettingsOutcome::Commit => {
                if app.commit_settings_dialog() {
                    app.settings_dialog = None;
                }
            }
            dialogs::SettingsOutcome::Cancel => app.settings_dialog = None,
            dialogs::SettingsOutcome::HubTest => app.test_hub_connection(),
            dialogs::SettingsOutcome::HubBackups => app.load_hub_backups(),
            dialogs::SettingsOutcome::PrepareRestore(index) => {
                app.prepare_hub_backup_restore(index)
            }
            dialogs::SettingsOutcome::ConfirmRestore(index) => app.restore_hub_backup(index),
            dialogs::SettingsOutcome::CancelRestore => {
                if let Some(dialog) = app.settings_dialog.as_mut() {
                    dialog.backup_restore_confirm = None;
                }
            }
        }
    }

    if app.laser_uncoordinated_confirm {
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Unkoordiniert verbinden?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ui, |ui| {
                ui.label("Hub ist aktiviert, aber derzeit nicht erreichbar.");
                ui.label("Eine direkte Ethernet-Verbindung kann mit einem anderen Arbeitsplatz kollidieren.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    cancel = ui.button("Abbrechen").clicked();
                    confirm = ui.button("Trotzdem verbinden").clicked();
                });
            });
        if cancel {
            app.laser_uncoordinated_confirm = false;
        } else if confirm {
            app.laser_connect_uncoordinated();
        }
    }

    if let Some(lease) = app.laser_lease_force_confirm.as_ref() {
        let holder = lease
            .holder_name
            .as_deref()
            .unwrap_or("unbekannt")
            .to_string();
        let usage = format!("{:?}", lease.holder_usage.unwrap_or_default());
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Ruida-Lease zwangsweise übernehmen?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ui, |ui| {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "Die letzte bekannte Lease war nicht sicher untätig.",
                );
                ui.label(format!("Letzter Arbeitsplatz: {holder} · Zustand: {usage}"));
                ui.label("Nur fortfahren, wenn du direkt an der Maschine geprüft hast, dass kein Job läuft oder pausiert ist.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    cancel = ui.button("Abbrechen").clicked();
                    confirm = ui.button("Maschine geprüft – übernehmen").clicked();
                });
            });
        if cancel {
            app.laser_lease_force_confirm = None;
        } else if confirm {
            app.force_laser_lease();
        }
    }

    // Nullpunkt-Namensdialog (ADR 0020 §D): Anlegen mit frisch gelesener
    // Position bzw. Umbenennen. Leere Namen lehnt die Application ab; der
    // Dialog bleibt dann zur Korrektur offen.
    if let Some(dialog) = app.saved_origin_dialog.as_mut() {
        let mut commit = false;
        let mut cancel = false;
        egui::Window::new("Nullpunkt speichern")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ui, |ui| {
                let (x, y) = dialog.position;
                ui.label(format!("Gelesene Position: X {x:.2} mm · Y {y:.2} mm"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Name");
                    let response = ui.text_edit_singleline(&mut dialog.name);
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        commit = true;
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    cancel = ui.button("Abbrechen").clicked();
                    commit |= ui.button("Speichern").clicked();
                });
            });
        if cancel || (commit && app.commit_saved_origin_dialog()) {
            app.saved_origin_dialog = None;
        }
    }

    // Laserprofile, Kalibrierung und Controllerzugriff als eigene
    // Master-Detail-Verwaltung aus dem Laser-Tab.
    if app.laser_manager.is_some() {
        let registry = app.laser_backend.registry.clone();
        let outcome = {
            let state = app.laser_manager.as_mut().unwrap();
            dialogs::laser_manager_window(ui, state, &registry)
        };
        match outcome {
            dialogs::LaserManagerOutcome::None => {}
            dialogs::LaserManagerOutcome::Close => app.laser_manager = None,
            dialogs::LaserManagerOutcome::Select(id) => app.laser_manager_select(&id),
            dialogs::LaserManagerOutcome::New => app.laser_manager_new(),
            dialogs::LaserManagerOutcome::Save => app.laser_manager_save(),
            dialogs::LaserManagerOutcome::Delete => app.laser_manager_delete(),
            dialogs::LaserManagerOutcome::MachineRead => app.laser_manager_machine_read(),
            dialogs::LaserManagerOutcome::MachineWrite => app.laser_manager_machine_write(),
        }
    }

    if app.layer_manager.is_some() && app.material_manager.is_none() {
        let laser = app.laser_backend.active_profile().cloned();
        let colors: Vec<_> = app.session.layers.iter().map(|layer| layer.color).collect();
        let outcome = dialogs::layer_manager_window(
            ui,
            app.layer_manager.as_mut().unwrap(),
            laser.as_ref(),
            app.material_service.library(),
            &colors,
        );
        match outcome {
            dialogs::LayerManagerOutcome::None => {}
            dialogs::LayerManagerOutcome::LoadMaterial => app.layer_manager_load_material(),
            dialogs::LayerManagerOutcome::Save => app.layer_manager_save(),
            dialogs::LayerManagerOutcome::Cancel => app.layer_manager = None,
            dialogs::LayerManagerOutcome::NewMaterial => app.open_material_manager(true),
            dialogs::LayerManagerOutcome::EditMaterial => app.open_material_manager(false),
        }
    }

    // Materialeditor liegt über dem Layer-Manager und kehrt nach Speichern
    // dorthin zurück.
    if let Some(state) = app.material_manager.as_mut() {
        match dialogs::material_manager_window(ui, state) {
            dialogs::MaterialManagerOutcome::None => {}
            dialogs::MaterialManagerOutcome::Save => app.material_manager_save(),
            dialogs::MaterialManagerOutcome::Delete => app.material_manager_delete(),
            dialogs::MaterialManagerOutcome::Cancel => app.material_manager = None,
        }
    }

    if let Some(state) = app.revision_comparison.as_ref() {
        let revision_id = state.comparison.entry.revision_id.clone();
        match dialogs::revision_comparison_window(ui, state, &app.asset_thumbnails) {
            dialogs::RevisionComparisonOutcome::None => {}
            dialogs::RevisionComparisonOutcome::Close => app.revision_comparison = None,
            dialogs::RevisionComparisonOutcome::KeepLocal => {
                app.revision_comparison = None;
                app.keep_local_inbox_revision(&revision_id);
            }
            dialogs::RevisionComparisonOutcome::AcceptRemote => {
                app.revision_comparison = None;
                app.accept_inbox_revision(&revision_id);
            }
        }
    }

    // „Neues Projekt"-Maske: Entwurf als &mut; Anlegen über den validierenden
    // ProjectService (leerer Name → Fehler, Maske bleibt offen).
    if let Some(st) = app.project_save_dialog.as_mut() {
        match dialogs::project_save_dialog_window(ui, st) {
            dialogs::DialogOutcome::None => {}
            dialogs::DialogOutcome::Commit => {
                if app.commit_project_save_dialog() {
                    app.project_save_dialog = None;
                }
            }
            dialogs::DialogOutcome::Cancel => app.project_save_dialog = None,
        }
    }

    // Dirty-Guard: eine Projektaktion (Neu/Öffnen) wartet auf Bestätigung, weil
    // sie ungespeicherte Änderungen verwerfen würde.
    if let Some(pending) = app.pending_project.as_ref() {
        let label = match pending {
            PendingProjectAction::Blank => "Neue Arbeitsfläche",
            PendingProjectAction::AcceptInbox(_) => "Hub-Version übernehmen",
            PendingProjectAction::AcceptAllInbox(_) => "Alle Hub-Versionen übernehmen",
            PendingProjectAction::New { .. } => "Neues Projekt anlegen",
            PendingProjectAction::Open(_) => "Projekt öffnen",
            PendingProjectAction::OpenVersion(_) => "Version laden",
            PendingProjectAction::DeleteVersion(_) => "Löschen der aktuellen Version",
        };
        match dialogs::guard_dialog(ui, label) {
            dialogs::DialogOutcome::None => {}
            dialogs::DialogOutcome::Commit => app.confirm_pending_project(),
            dialogs::DialogOutcome::Cancel => app.pending_project = None,
        }
    }

    // Dirty-Guard beim Schließen: Bestätigung, bevor das Programm mit
    // ungespeicherten Änderungen beendet wird.
    if app.close_pending {
        match dialogs::guard_dialog(ui, "Beenden") {
            dialogs::DialogOutcome::None => {}
            dialogs::DialogOutcome::Commit => app.confirm_close(),
            dialogs::DialogOutcome::Cancel => app.close_pending = false,
        }
    }

    // Toasts zuletzt, damit sie über allen Panels liegen.
    app.toasts.show(ui);

    // Start-Splash zuoberst (Tooltip-Ebene); nach Ablauf wegwerfen.
    if let Some(splash) = app.splash.as_mut() {
        if !splash.show(ui, app.ui_settings.splash_ms) {
            app.splash = None;
        }
    }
}

/// Hält den Detail-/Vorschau-Cache des Projektbrowsers aktuell. Cache-Schlüssel
/// ist `name:modified_at` (beim offenen Projekt `name:rev<render_rev>`), so
/// verfallen Details nach Speichern/Umbenennen/Editieren von selbst. Läuft im
/// Root, weil nur er den `ProjectService` kennt; das Panel liest nur den Cache.
fn sync_project_browser(app: &mut App, open_name: Option<&str>) {
    // Auswahl validieren: gelöschte/umbenannte Projekte abwählen.
    if let Some(sel) = app.project_browser.selected.clone() {
        if !app.project_catalog.iter().any(|p| p.name == sel) {
            app.project_browser.selected = None;
        }
    }
    let Some(sel) = app.project_browser.selected.clone() else {
        app.project_browser.cached = None;
        return;
    };
    let is_open = open_name == Some(sel.as_str());
    let cache_key = if is_open {
        format!("{sel}:rev{}", app.session.state().render_rev())
    } else {
        let modified = app
            .project_catalog
            .iter()
            .find(|p| p.name == sel)
            .map(|p| p.modified_at.as_str())
            .unwrap_or("");
        format!("{sel}:{modified}")
    };
    let cached_ok = app
        .project_browser
        .cached
        .as_ref()
        .is_some_and(|c| c.cache_key == cache_key);
    if cached_ok {
        return;
    }
    // Vorschau des offenen Projekts kommt aus der Session (aktueller als die
    // Datei); für andere Projekte wird der Zustand nur-lesend geladen.
    let preview = if is_open {
        Ok(project::preview_from_state(app.session.state()))
    } else {
        app.project
            .peek_state(&sel)
            .map(|st| project::preview_from_state(&st))
    };
    match (app.project.detail(&sel), preview) {
        (Ok(detail), Ok(preview)) => {
            app.project_browser.cached = Some(CachedProjectDetail {
                cache_key,
                detail,
                preview,
            });
        }
        (Err(e), _) | (_, Err(e)) => {
            app.app_error = Some(e);
            app.project_browser.selected = None;
            app.project_browser.cached = None;
        }
    }
}

/// Zeichnet die Textbeschriftungen „Start"/„Ursprung"/„Kopf" an die
/// zugehörigen Fadenkreuze (feste Quadranten: Start links oberhalb, Ursprung
/// rechts oberhalb, Kopf rechts unterhalb — ADR 0020 §B). Deckungsgleiche
/// Marker bleiben so unterscheidbar.
fn laser_marker_labels(ui: &mut egui::Ui, app: &App, canvas_rect: egui::Rect) {
    let markers = app.laser_canvas_markers();
    let ppp = ui.ctx().pixels_per_point();
    let painter = ui
        .ctx()
        .layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("laser_marker_labels"),
        ))
        .with_clip_rect(canvas_rect);
    // `anchor` = welche Ecke des Textkastens am (versetzten) Kreuz hängt.
    let draw = |world: Option<[f64; 2]>,
                label: &str,
                anchor: egui::Align2,
                offset: egui::Vec2,
                color: Color32| {
        let Some(world) = world else {
            return;
        };
        let screen = app.canvas.cam.world_to_screen(world);
        let pos = egui::pos2(screen[0] / ppp, screen[1] / ppp);
        if !canvas_rect.contains(pos) {
            return;
        }
        let font = egui::FontId::proportional(12.0);
        let galley = painter.layout_no_wrap(label.to_owned(), font, color);
        let rect = anchor.anchor_size(pos + offset, galley.size());
        painter.rect_filled(rect.expand(3.0), 4.0, Color32::from_black_alpha(160));
        painter.galley(rect.min, galley, color);
    };
    // Farbtöne identisch zu den GPU-Kreuzen (overlay.rs).
    draw(
        markers.start,
        "Start",
        egui::Align2::RIGHT_BOTTOM, // Text links oberhalb des Kreuzes
        egui::vec2(-10.0, -8.0),
        Color32::from_rgb(0x40, 0xc7, 0x73),
    );
    draw(
        markers.origin,
        "Ursprung",
        egui::Align2::LEFT_BOTTOM, // rechts oberhalb
        egui::vec2(10.0, -8.0),
        Color32::from_rgb(0x40, 0x8c, 0xfa),
    );
    draw(
        markers.head,
        "Kopf",
        egui::Align2::LEFT_TOP, // rechts unterhalb
        egui::vec2(10.0, 8.0),
        Color32::from_rgb(0xff, 0x9e, 0x26),
    );
}

/// Leitet die reine Ebenen-Sicht für `layers_panel` aus der Session ab.
fn layer_rows(app: &App) -> Vec<layers::LayerRow> {
    let s = app.session.state();
    s.layers
        .iter()
        .enumerate()
        .map(|(i, l)| layers::LayerRow {
            color: l.color,
            name: l.name.clone(),
            visible: l.visible,
            enabled: l.enabled,
            locked: l.locked,
            air_assist: l.air_assist,
            mode: l.mode,
            count: s.shapes.iter().filter(|sh| sh.layer_id == i).count(),
        })
        .collect()
}

/// Leitet die reine Laser-Sicht für `laserpanel::show` ab.
fn laser_view(app: &mut App) -> laserpanel::LaserView {
    use studio_core::JobAction;
    let profiles = app
        .laser_backend
        .registry
        .profiles
        .iter()
        .map(|p| (p.id.clone(), format!("{} · {:?}", p.name, p.kind)))
        .collect();
    let active_id = app
        .laser_backend
        .active_profile()
        .map(|p| p.id.clone())
        .unwrap_or_default();
    let actions = app.laser_backend.actions();
    let has = |a: JobAction| {
        actions
            .iter()
            .any(|x| std::mem::discriminant(x) == std::mem::discriminant(&a))
    };
    // Feste Slot-Reihenfolge; erster passender Treiber-Key füllt den Slot.
    let slots = [
        [JobAction::SendJob, JobAction::StreamGcode]
            .into_iter()
            .find(|a| has(*a)),
        Some(JobAction::Pause).filter(|a| has(*a)),
        Some(JobAction::Stop).filter(|a| has(*a)),
        Some(JobAction::GoOrigin).filter(|a| has(*a)),
        Some(JobAction::Frame).filter(|a| has(*a)),
        Some(JobAction::RubberFrame).filter(|a| has(*a)),
    ];
    let can_export = has(JobAction::ExportFile);
    let connected = app.laser_backend.is_connected();
    let capabilities = app.laser_backend.driver_capabilities();

    // Nullpunktliste + Gültigkeit aus dem aktiven Profil (ADR 0020).
    let saved_origins = app
        .laser_backend
        .active_profile()
        .map(|profile| {
            profile
                .saved_origins
                .iter()
                .map(|origin| laserpanel::SavedOriginRow {
                    id: origin.id.clone(),
                    name: origin.name.clone(),
                    x_mm: origin.x_mm,
                    y_mm: origin.y_mm,
                    usable: profile.saved_origin_usable(origin),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // Verweist die gemerkte Auswahl auf eine gelöschte Nullpunkt-ID? Dann
    // sichtbar warnen — kein stiller Fallback (ADR 0020 §E). Die Position
    // selbst zeigt der Canvas über die Fadenkreuze.
    let reference_missing = app
        .laser
        .start_reference
        .saved_origin_id()
        .is_some_and(|id| !saved_origins.iter().any(|row| row.id == id));
    let can_save_origin = connected && capabilities.position_read;
    // Z/U-Verfügbarkeit ist eine Profil-Einstellung (nicht aus dem Controller,
    // ADR 0021 §A). Aus dem aktiven Profil, unabhängig von der Verbindung.
    let axes = app
        .laser_backend
        .active_profile()
        .map(|profile| profile.axes)
        .unwrap_or_default();
    let live = &app.laser_live;
    let pos = laserpanel::AxisPositions {
        x: live.head.map(|(x, _)| x),
        y: live.head.map(|(_, y)| y),
        z: live.pos_z,
        u: live.pos_u,
    };
    laserpanel::LaserView {
        profiles,
        active_id,
        slots,
        can_export,
        connected,
        lease_pending: app.laser_lease_pending,
        saved_origins,
        reference_missing,
        can_save_origin,
        has_z_axis: axes.has_z_axis,
        has_u_axis: axes.has_u_axis,
        pos,
        hold_active: app.laser_hold.is_some(),
    }
}

/// Skaliert einen Farbton auf eine Zielhelligkeit (für die Button-Fläche:
/// die Intensität regelt, wie stark der gewählte Ton durchkommt).
fn scale_rgb(hue: [u8; 3], f: f32) -> Color32 {
    let s = |c: u8| (c as f32 * f).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(s(hue[0]), s(hue[1]), s(hue[2]))
}

/// Dark-Workshop-Theme: warme Graphitflächen mit klarer Helligkeitsstaffelung.
/// Akzent- und Buttonfarbe kommen aus den GUI-Settings (ADR 0002); mit den
/// Default-Settings entspricht das exakt dem bisherigen festen Look.
///
/// Liefert einen kompletten Context-Style: Fenster, Menüs, Popups und Tooltips
/// baut egui aus dem Context-Style auf — ein pro-`Ui` gesetztes Theme erreicht
/// sie nicht.
pub(crate) fn theme_style(theme: &studio_core::Theme) -> egui::Style {
    use egui::{CornerRadius, Stroke};
    let bg = c32(theme.palette.background);
    let toolbar = c32(theme.palette.toolbar);
    let panel = c32(theme.palette.panel);
    let panel2 = c32(theme.palette.surface);
    let border = c32(theme.palette.border);
    let text = c32(theme.palette.text);
    let muted = c32(theme.palette.muted);

    // Akzent: voller Farbton für Kanten/Text, Intensität steuert die Füllungen
    // (Default 0.7 → 0.85/0.9, die bisherigen festen Werte).
    let accent = c32(theme.accent.hue);
    let ai = theme.accent.intensity as f32;
    let accent_sel = accent.gamma_multiply((ai + 0.2).min(1.0));
    let accent_fill = accent.gamma_multiply((ai + 0.15).min(1.0));
    // Button-Fläche: Farbton auf Panel-Helligkeit skaliert (Default ≈ panel-2).
    let bi = theme.button.intensity as f32;
    let button_fill = scale_rgb(theme.button.hue, bi * 0.6);
    let button_hover = scale_rgb(theme.button.hue, bi * 0.78);

    let mut v = egui::Visuals::dark();
    v.panel_fill = panel;
    v.window_fill = panel2;
    v.extreme_bg_color = bg; // Hintergrund von TextEdit/Canvas-Rand
    v.faint_bg_color = panel2;
    v.override_text_color = Some(text);
    v.window_stroke = Stroke::new(1.0, border);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(8);
    v.selection.bg_fill = accent_sel;
    v.selection.stroke = Stroke::new(1.0, accent);
    v.hyperlink_color = accent;
    v.error_fg_color = c32(theme.palette.danger);
    v.warn_fg_color = accent;

    let r = CornerRadius::same(8);
    // Ruhende Widgets: neutrale Fläche, weiche Kante.
    v.widgets.noninteractive.bg_fill = toolbar;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, muted);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.bg_fill = button_fill;
    v.widgets.inactive.weak_bg_fill = button_fill;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
    v.widgets.inactive.corner_radius = r;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
    // Hover: leicht anheben.
    v.widgets.hovered.bg_fill = button_hover;
    v.widgets.hovered.weak_bg_fill = button_hover;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, text);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent.gamma_multiply(0.5));
    v.widgets.hovered.corner_radius = r;
    v.widgets.hovered.expansion = 1.0;
    // Aktiv/gedrückt: Akzent trägt.
    v.widgets.active.bg_fill = accent_fill;
    v.widgets.active.weak_bg_fill = accent_fill;
    v.widgets.active.fg_stroke = Stroke::new(1.0, text);
    v.widgets.active.bg_stroke = Stroke::new(1.0, accent);
    v.widgets.active.corner_radius = r;
    v.widgets.active.expansion = 1.0;
    // „open" (ComboBox aufgeklappt etc.)
    v.widgets.open.bg_fill = button_fill;
    v.widgets.open.corner_radius = r;

    // Seit 0.30 geänderte Defaults, auf deren alte Werte das Layout
    // abgestimmt ist: Clip-Rand (sonst wirken Ränder abgeschnitten) und
    // runde Slider-Griffe statt der neuen rechteckigen.
    v.clip_rect_margin = 3.0;
    v.handle_shape = egui::style::HandleShape::Circle;

    let mut style = egui::Style {
        visuals: v,
        ..egui::Style::default()
    };

    // Etwas mehr Luft in Abständen (näher am Svelte-Spacing).
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(12);
    style.spacing.menu_margin = egui::Margin::same(8);
    // Zeilenhöhe = tatsächliche Button-Höhe (Button-Text ≈ 14 px + 2×6 px
    // Padding). `horizontal()` nimmt `interact_size.y` als Zeilenhöhe an;
    // beim egui-Default 18 laufen unsere Buttons über und Combo/Label/Button
    // sitzen um 4 px versetzt.
    style.spacing.interact_size = egui::vec2(40.0, 26.0);

    // Textmaße wie vor dem Upgrade (0.35 vergrößerte Body/Button auf 13.0,
    // Monospace auf 13.0) — die Panel-Breiten sind auf die alten Maße gebaut.
    use egui::{FontFamily, FontId, TextStyle};
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(12.5, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(12.5, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.0, FontFamily::Monospace),
    );
    style
}
