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
    CachedProjectDetail, CharonTestStatus, GeoOpDialogState, GeoOpKind, ImageDialogState,
    LaserManagerState, LaserManagerTab, LayerDialogState, PendingProjectAction,
    ProjectBrowserState, ProjectSaveDialogState, RevisionComparisonState, SettingsDialogState,
    SettingsSection, TextDialogState,
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

pub fn build(ctx: &egui::Context, app: &mut App) {
    use crate::tools::View;
    apply_theme(ctx, &app.ui_settings.theme);

    // Oben: Reiter | Undo/Redo + Datei-Aktionen | Projektname.
    let view = app.view;
    let project_name = app
        .project
        .open_name()
        .unwrap_or("— (ungespeichert)")
        .to_string();
    let inbox_count = app
        .project_inbox
        .iter()
        .filter(|entry| entry.status == luxifer_application::InboxStatus::PendingReview)
        .count();
    let topbar_actions = egui::TopBottomPanel::top("topbar")
        .show(ctx, |ui| {
            topbar::topbar(ui, view, &project_name, inbox_count)
        })
        .inner;
    for action in topbar_actions {
        app.dispatch(action);
    }

    if let Some(error) = app.app_error.as_ref() {
        let code = error.code().to_string();
        let message = error.message().to_string();
        let details = error.details().map(|d| d.to_string());
        let actions = egui::TopBottomPanel::top("application_error")
            .show(ctx, |ui| {
                status::error_banner(ui, &message, &code, details.as_deref())
            })
            .inner;
        for action in actions {
            app.dispatch(action);
        }
    }

    // Zweite Kopfzeile: Anordnen (Ausrichten/Verteilen/Gruppieren/Nesting) — nur
    // im Design-Reiter. Wie in der Tauri-App liegt das im Kopf. Pilot der
    // UiAction-Grenze: Das Panel liefert Absichten, der Root führt sie aus.
    if app.view == View::Design {
        let selection = app.selection_count();
        let actions = egui::TopBottomPanel::top("arrange")
            .show(ctx, |ui| {
                ui.add_space(3.0);
                let a = arrange::arrange_bar(ui, selection);
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
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        status::status_bar(ui, fps, tool_label, shapes);
    });

    match app.view {
        View::Projekt => {
            app.left_w = 0.0;
            app.right_w = 0.0;
            let projects = app.project.list();
            let open_name = app.project.open_name().map(|s| s.to_string());
            sync_project_browser(app, &projects, open_name.as_deref());
            let dirty = app.session.is_dirty();
            let actions = egui::CentralPanel::default()
                .show(ctx, |ui| {
                    project::project_browser(
                        ui,
                        &mut app.project_browser,
                        &projects,
                        &app.project_inbox,
                        open_name.as_deref(),
                        dirty,
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
                ctx.request_repaint();
            }
            let material = app.preview_material;
            let show_travel = app.preview_show_travel;
            let right = egui::SidePanel::right("preview_panel")
                .default_width(240.0)
                .width_range(200.0..=320.0)
                .resizable(true)
                .show(ctx, |ui| {
                    preview::preview_panel(ui, material, show_travel, app.preview_legend())
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
                let left = egui::SidePanel::left("laser_layers")
                    .default_width(300.0)
                    .width_range(260.0..=420.0)
                    .resizable(true)
                    .show(ctx, |ui| {
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
                let left = egui::SidePanel::left("tools")
                    // Zwei 34-pt-Buttons + Abstand + Panel-Innenränder
                    // brauchen bei DPI-Rundung etwas Reserve. 88 pt lagen
                    // exakt auf der rechnerischen Untergrenze und schnitten
                    // die rechte Buttonkante optisch an.
                    .exact_width(100.0)
                    .resizable(false)
                    .show(ctx, |ui| tools::tools_panel(ui, cur_tool));
                app.left_w = left.response.rect.width();
                for action in left.inner {
                    app.dispatch(action);
                }
            }

            // Sichten vorab ableiten, damit die Panels keinen App-/Backend-
            // Zugriff brauchen. `laser_view` ruft `actions()` (baut den Treiber
            // lazy), daher &mut vor der Closure.
            let laser_view = if is_laser {
                Some(laser_view(app))
            } else {
                None
            };
            // Der Inspector-Inhalt ist länger als kleine Fenster: vertikal
            // scrollen, ohne die Breite schrumpfen zu lassen (auto_shrink
            // false hält die Zeilen exakt auf Panelbreite).
            let right = egui::SidePanel::right("inspector")
                .default_width(340.0)
                .width_range(300.0..=460.0)
                .resizable(true)
                .show(ctx, |ui| {
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
                let canvas_rect = ctx.available_rect();

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
                        .show(ctx, |ui| palette::shape_picker(ui, active_shape))
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
                    .show(ctx, |ui| palette::palette_panel(ui, accent))
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
        let profile = app.laser_backend.active_profile();
        let origin = profile.map(|p| p.origin).unwrap_or_default();
        let bed = profile
            .map(|p| p.bed_mm)
            .unwrap_or((app.session.bed_w_mm, app.session.bed_h_mm));
        ruler::rulers(
            ctx,
            &app.canvas.cam,
            app.canvas.cursor,
            app.ui_settings.theme.accent.hue,
            origin,
            bed,
        );
    }

    // Ein gemeinsames Backdrop für alle echten Dialoge. Beim Einstellen wird
    // direkt der Draft gelesen, damit der Alpha-Regler live reagiert.
    let has_dialog = app.text_dialog.is_some()
        || app.layer_dialog.is_some()
        || app.image_dialog.is_some()
        || app.geo_op_dialog.is_some()
        || app.settings_dialog.is_some()
        || app.laser_manager.is_some()
        || app.project_save_dialog.is_some()
        || app.revision_comparison.is_some()
        || app.pending_project.is_some()
        || app.close_pending;
    if has_dialog {
        let alpha = app
            .settings_dialog
            .as_ref()
            .map(|state| state.draft.modal_backdrop_alpha)
            .unwrap_or(app.ui_settings.modal_backdrop_alpha);
        dialogs::modal_backdrop(ctx, alpha);
    }

    // Text-Dialog: Entwurf als &mut, Font-Namen als reine Anzeigeliste.
    if app.text_dialog.is_some() {
        let font_names: Vec<String> = app.fonts.iter().map(|f| f.name.clone()).collect();
        let state = app.text_dialog.as_mut().unwrap();
        match dialogs::text_dialog_window(ctx, state, &font_names) {
            dialogs::DialogOutcome::None => {}
            dialogs::DialogOutcome::Commit => {
                if app.commit_text() {
                    app.text_dialog = None;
                }
            }
            dialogs::DialogOutcome::Cancel => app.text_dialog = None,
        }
    }

    // Layer-Dialog: der Entwurf wird als &mut gereicht, der Root behandelt das
    // Ergebnis (Übernahme über die validierende Session bzw. Verwerfen).
    if let Some(state) = app.layer_dialog.as_mut() {
        match dialogs::layer_dialog_window(ctx, &mut state.params) {
            dialogs::DialogOutcome::None => {}
            dialogs::DialogOutcome::Commit => {
                if app.commit_layer_dialog() {
                    app.layer_dialog = None;
                }
            }
            dialogs::DialogOutcome::Cancel => app.layer_dialog = None,
        }
    }

    // Bildparameter-Dialog: Entwurf als &mut; Speichern über die validierende
    // Session, Abbrechen verwirft.
    if let Some(state) = app.image_dialog.as_mut() {
        match dialogs::image_dialog_window(ctx, state) {
            dialogs::ImageDialogOutcome::None => {}
            dialogs::ImageDialogOutcome::Save => {
                if app.commit_image_dialog() {
                    app.image_dialog = None;
                }
            }
            // Trace lässt den Dialog offen: Regler nachziehen und erneut
            // vektorisieren ist der übliche Arbeitsfluss.
            dialogs::ImageDialogOutcome::Trace => app.trace_image_dialog(),
            dialogs::ImageDialogOutcome::Cancel => app.image_dialog = None,
        }
    }

    // Geometrie-Parameterdialog (Boolean/Offset/Fillet): Entwurf als &mut,
    // Ausführung über die Session.
    if let Some(st) = app.geo_op_dialog.as_mut() {
        match dialogs::geo_op_dialog_window(ctx, st) {
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
        match dialogs::settings_dialog_window(ctx, st) {
            dialogs::SettingsOutcome::None => {}
            dialogs::SettingsOutcome::Commit => {
                if app.commit_settings_dialog() {
                    app.settings_dialog = None;
                }
            }
            dialogs::SettingsOutcome::Cancel => app.settings_dialog = None,
            dialogs::SettingsOutcome::CharonTest => app.test_charon_connection(),
        }
    }

    // Laserprofile, Kalibrierung und Controllerzugriff als eigene
    // Master-Detail-Verwaltung aus dem Laser-Tab.
    if app.laser_manager.is_some() {
        let registry = app.laser_backend.registry.clone();
        let outcome = {
            let state = app.laser_manager.as_mut().unwrap();
            dialogs::laser_manager_window(ctx, state, &registry)
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

    if let Some(state) = app.revision_comparison.as_ref() {
        let revision_id = state.comparison.entry.revision_id.clone();
        match dialogs::revision_comparison_window(ctx, state) {
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
        match dialogs::project_save_dialog_window(ctx, st) {
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
            PendingProjectAction::AcceptInbox(_) => "Charon-Version übernehmen",
            PendingProjectAction::New { .. } => "Neues Projekt anlegen",
            PendingProjectAction::Open(_) => "Projekt öffnen",
            PendingProjectAction::OpenVersion(_) => "Version laden",
            PendingProjectAction::DeleteVersion(_) => "Löschen der aktuellen Version",
        };
        match dialogs::guard_dialog(ctx, label) {
            dialogs::DialogOutcome::None => {}
            dialogs::DialogOutcome::Commit => app.confirm_pending_project(),
            dialogs::DialogOutcome::Cancel => app.pending_project = None,
        }
    }

    // Dirty-Guard beim Schließen: Bestätigung, bevor das Programm mit
    // ungespeicherten Änderungen beendet wird.
    if app.close_pending {
        match dialogs::guard_dialog(ctx, "Beenden") {
            dialogs::DialogOutcome::None => {}
            dialogs::DialogOutcome::Commit => app.confirm_close(),
            dialogs::DialogOutcome::Cancel => app.close_pending = false,
        }
    }

    // Toasts zuletzt, damit sie über allen Panels liegen.
    app.toasts.show(ctx);

    // Start-Splash zuoberst (Tooltip-Ebene); nach Ablauf wegwerfen.
    if let Some(splash) = app.splash.as_mut() {
        if !splash.show(ctx, app.ui_settings.splash_ms) {
            app.splash = None;
        }
    }
}

/// Hält den Detail-/Vorschau-Cache des Projektbrowsers aktuell. Cache-Schlüssel
/// ist `name:modified_at` (beim offenen Projekt `name:rev<render_rev>`), so
/// verfallen Details nach Speichern/Umbenennen/Editieren von selbst. Läuft im
/// Root, weil nur er den `ProjectService` kennt; das Panel liest nur den Cache.
fn sync_project_browser(
    app: &mut App,
    projects: &[luxifer_core::ProjectInfo],
    open_name: Option<&str>,
) {
    // Auswahl validieren: gelöschte/umbenannte Projekte abwählen.
    if let Some(sel) = app.project_browser.selected.clone() {
        if !projects.iter().any(|p| p.name == sel) {
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
        let modified = projects
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

/// Leitet die reine Laser-Sicht für `laserpanel::show` ab. Braucht `&mut`, weil
/// `laser_backend.actions()` den Treiber zum aktiven Profil lazy aufbaut.
fn laser_view(app: &mut App) -> laserpanel::LaserView {
    use luxifer_core::JobAction;
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
    laserpanel::LaserView {
        profiles,
        active_id,
        slots,
        can_export,
    }
}

/// Skaliert einen Farbton auf eine Zielhelligkeit (für die Button-Fläche:
/// die Intensität regelt, wie stark der gewählte Ton durchkommt).
fn scale_rgb(hue: [u8; 3], f: f32) -> Color32 {
    let s = |c: u8| (c as f32 * f).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(s(hue[0]), s(hue[1]), s(hue[2]))
}

/// Dunkles Theme, an den Svelte-Look angelehnt (kühles Blau-Grau).
/// Theme nah am Tauri-Design (app.css): kühles Blau-Grau, Akzent nur am aktiven
/// Element, sanfte Rundungen und ein bisschen mehr Luft. Bewusst ohne echtes
/// Glas/Blur (das kann egui nicht), aber mit denselben Farbwerten.
/// Akzent- und Buttonfarbe kommen aus den GUI-Settings (ADR 0002); mit den
/// Default-Settings entspricht das exakt dem bisherigen festen Look.
fn apply_theme(ctx: &egui::Context, theme: &luxifer_core::Theme) {
    use egui::{Rounding, Stroke};
    let bg = Color32::from_rgb(0x10, 0x12, 0x16); // --bg
    let panel = Color32::from_rgb(0x17, 0x1a, 0x20); // --panel
    let panel2 = Color32::from_rgb(0x1c, 0x1f, 0x26); // --panel-2 (Inputs/Kacheln)
    let border = Color32::from_rgb(0x2a, 0x2e, 0x36); // --border
    let text = Color32::from_rgb(0xec, 0xee, 0xf1); // --text
    let muted = Color32::from_rgb(0x9a, 0xa0, 0xa9); // --muted

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
    v.window_fill = panel;
    v.extreme_bg_color = bg; // Hintergrund von TextEdit/Canvas-Rand
    v.faint_bg_color = panel2;
    v.override_text_color = Some(text);
    v.window_stroke = Stroke::new(1.0, border);
    v.window_rounding = Rounding::same(12.0);
    v.selection.bg_fill = accent_sel;
    v.selection.stroke = Stroke::new(1.0, accent);
    v.hyperlink_color = accent;

    let r = Rounding::same(8.0);
    // Ruhende Widgets: neutrale Fläche, weiche Kante.
    v.widgets.noninteractive.bg_fill = panel;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, muted);
    v.widgets.noninteractive.rounding = r;
    v.widgets.inactive.bg_fill = button_fill;
    v.widgets.inactive.weak_bg_fill = button_fill;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
    v.widgets.inactive.rounding = r;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
    // Hover: leicht anheben.
    v.widgets.hovered.bg_fill = button_hover;
    v.widgets.hovered.weak_bg_fill = button_hover;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, text);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent.gamma_multiply(0.5));
    v.widgets.hovered.rounding = r;
    // Aktiv/gedrückt: Akzent trägt.
    v.widgets.active.bg_fill = accent_fill;
    v.widgets.active.weak_bg_fill = accent_fill;
    v.widgets.active.fg_stroke = Stroke::new(1.0, text);
    v.widgets.active.bg_stroke = Stroke::new(1.0, accent);
    v.widgets.active.rounding = r;
    // „open" (ComboBox aufgeklappt etc.)
    v.widgets.open.bg_fill = button_fill;
    v.widgets.open.rounding = r;

    ctx.set_visuals(v);

    // Etwas mehr Luft in Abständen (näher am Svelte-Spacing).
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    ctx.set_style(style);
}
