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
mod project;
mod state;
mod status;
mod tools;
mod topbar;

pub use action::UiAction;
pub use state::{
    GeoOpDialogState, GeoOpKind, ImageDialogState, LayerDialogState, PendingProjectAction,
    TextDialogState,
};

use egui::Color32;

use crate::app::App;
use crate::laserpanel;
use crate::tools::Tool;

/// RGB-Tripel → egui-Farbe. Geteilter Helfer für die Panels.
pub(super) fn c32(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

pub fn build(ctx: &egui::Context, app: &mut App) {
    use crate::tools::View;
    apply_theme(ctx);

    // Oben: Reiter | Undo/Redo + Datei-Aktionen | Projektname.
    let view = app.view;
    let project_name = app
        .project
        .open_name()
        .unwrap_or("— (ungespeichert)")
        .to_string();
    let topbar_actions = egui::TopBottomPanel::top("topbar")
        .show(ctx, |ui| topbar::topbar(ui, view, &project_name))
        .inner;
    for action in topbar_actions {
        app.dispatch(action);
    }

    if let Some(error) = app.app_error.as_ref() {
        let code = error.code().to_string();
        let message = error.message().to_string();
        let actions = egui::TopBottomPanel::top("application_error")
            .show(ctx, |ui| status::error_banner(ui, &message, &code))
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

    // Statuszeile unten (rein lesend).
    let (fps, tool_label, shapes, msg) = (
        app.fps(),
        app.canvas.tool.label(),
        app.session.shapes.len(),
        app.project_msg.clone(),
    );
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        status::status_bar(ui, fps, tool_label, shapes, &msg);
    });

    match app.view {
        View::Projekt => {
            app.left_w = 0.0;
            app.right_w = 0.0;
            let projects = app.project.list();
            let open_name = app.project.open_name().map(|s| s.to_string());
            let draft = &mut app.new_project_name;
            let actions = egui::CentralPanel::default()
                .show(ctx, |ui| {
                    project::project_browser(ui, draft, &projects, open_name.as_deref())
                })
                .inner;
            for action in actions {
                app.dispatch(action);
            }
        }
        View::Preview => {
            app.left_w = 0.0;
            app.right_w = 0.0;
        }
        View::Design | View::Laser => {
            let cur_tool = app.canvas.tool;
            let is_laser = app.view == View::Laser;
            if is_laser {
                app.left_w = 0.0;
            } else {
                let left = egui::SidePanel::left("tools")
                    .exact_width(96.0)
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
            let layer_rows: Vec<layers::LayerRow> = layer_rows(app);
            let laser_editable = app.canvas.laser_editable_layers.clone().unwrap_or_default();
            let right = egui::SidePanel::right("inspector")
                .default_width(340.0)
                .width_range(300.0..=460.0)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.add_space(6.0);
                    if let Some(view) = &laser_view {
                        let mut actions = laserpanel::show(ui, view, &mut app.laser);
                        actions.extend(layers::laser_edit_layers(ui, &layer_rows, &laser_editable));
                        actions
                    } else {
                        layers::layers_panel(ui, &layer_rows)
                    }
                });
            app.right_w = right.response.rect.width();
            for action in right.inner {
                app.dispatch(action);
            }

            // Farbpalette (+ Form-Wähler beim Polygon-Werkzeug) als Dock am
            // unteren Canvas-Rand (nur Design), zentriert wie in der Tauri-App.
            if !is_laser {
                let show_shapes = app.canvas.tool == Tool::Polygon;
                let active_shape = app.canvas.active_shape;
                let accent = app.accent;
                let actions = egui::TopBottomPanel::bottom("palette_dock")
                    .show_separator_line(true)
                    .show(ctx, |ui| {
                        let mut actions = Vec::new();
                        ui.add_space(6.0);
                        if show_shapes {
                            ui.vertical_centered(|ui| {
                                actions.extend(palette::shape_picker(ui, active_shape))
                            });
                            ui.add_space(4.0);
                        }
                        ui.vertical_centered(|ui| {
                            actions.extend(palette::palette_panel(ui, accent))
                        });
                        ui.add_space(6.0);
                        actions
                    })
                    .inner;
                for action in actions {
                    app.dispatch(action);
                }
            }
        }
    }

    // Laser-Einstellungen: Entwurf (Profil) als &mut; der Root persistiert bei
    // Speichern/Löschen und schließt bei Abbrechen.
    if app.laser_settings.is_some() {
        let profile = app.laser_settings.as_mut().unwrap();
        match dialogs::laser_settings_window(ctx, profile) {
            dialogs::LaserDialogOutcome::None => {}
            dialogs::LaserDialogOutcome::Save => app.save_laser_settings(),
            dialogs::LaserDialogOutcome::Delete => {
                let id = app.laser_settings.as_ref().unwrap().id.clone();
                app.delete_laser_profile(&id);
                app.laser_settings = None;
            }
            dialogs::LaserDialogOutcome::Cancel => app.laser_settings = None,
        }
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
        match dialogs::image_dialog_window(ctx, &mut state.params) {
            dialogs::DialogOutcome::None => {}
            dialogs::DialogOutcome::Commit => {
                if app.commit_image_dialog() {
                    app.image_dialog = None;
                }
            }
            dialogs::DialogOutcome::Cancel => app.image_dialog = None,
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

    // Dirty-Guard: eine Projektaktion (Neu/Öffnen) wartet auf Bestätigung, weil
    // sie ungespeicherte Änderungen verwerfen würde.
    if let Some(pending) = app.pending_project.as_ref() {
        let label = match pending {
            PendingProjectAction::New(_) => "Neues Projekt anlegen",
            PendingProjectAction::Open(_) => "Projekt öffnen",
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
    let msg = app.laser_msg.clone();
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
        msg,
    }
}

/// Dunkles Theme, an den Svelte-Look angelehnt (kühles Blau-Grau).
/// Theme nah am Tauri-Design (app.css): kühles Blau-Grau, Akzent nur am aktiven
/// Element, sanfte Rundungen und ein bisschen mehr Luft. Bewusst ohne echtes
/// Glas/Blur (das kann egui nicht), aber mit denselben Farbwerten.
fn apply_theme(ctx: &egui::Context) {
    use egui::{Rounding, Stroke};
    let bg = Color32::from_rgb(0x10, 0x12, 0x16); // --bg
    let panel = Color32::from_rgb(0x17, 0x1a, 0x20); // --panel
    let panel2 = Color32::from_rgb(0x1c, 0x1f, 0x26); // --panel-2 (Inputs/Kacheln)
    let border = Color32::from_rgb(0x2a, 0x2e, 0x36); // --border
    let text = Color32::from_rgb(0xec, 0xee, 0xf1); // --text
    let muted = Color32::from_rgb(0x9a, 0xa0, 0xa9); // --muted
    let accent = Color32::from_rgb(0x3B, 0x82, 0xF6); // --accent

    let mut v = egui::Visuals::dark();
    v.panel_fill = panel;
    v.window_fill = panel;
    v.extreme_bg_color = bg; // Hintergrund von TextEdit/Canvas-Rand
    v.faint_bg_color = panel2;
    v.override_text_color = Some(text);
    v.window_stroke = Stroke::new(1.0, border);
    v.window_rounding = Rounding::same(12.0);
    v.selection.bg_fill = accent.gamma_multiply(0.9);
    v.selection.stroke = Stroke::new(1.0, accent);
    v.hyperlink_color = accent;

    let r = Rounding::same(8.0);
    // Ruhende Widgets: neutrale Fläche, weiche Kante.
    v.widgets.noninteractive.bg_fill = panel;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, muted);
    v.widgets.noninteractive.rounding = r;
    v.widgets.inactive.bg_fill = panel2;
    v.widgets.inactive.weak_bg_fill = panel2;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
    v.widgets.inactive.rounding = r;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
    // Hover: leicht anheben.
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x25, 0x2a, 0x33);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x25, 0x2a, 0x33);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, text);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent.gamma_multiply(0.5));
    v.widgets.hovered.rounding = r;
    // Aktiv/gedrückt: Akzent trägt.
    v.widgets.active.bg_fill = accent.gamma_multiply(0.85);
    v.widgets.active.weak_bg_fill = accent.gamma_multiply(0.85);
    v.widgets.active.fg_stroke = Stroke::new(1.0, text);
    v.widgets.active.bg_stroke = Stroke::new(1.0, accent);
    v.widgets.active.rounding = r;
    // „open" (ComboBox aufgeklappt etc.)
    v.widgets.open.bg_fill = panel2;
    v.widgets.open.rounding = r;

    ctx.set_visuals(v);

    // Etwas mehr Luft in Abständen (näher am Svelte-Spacing).
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    ctx.set_style(style);
}
