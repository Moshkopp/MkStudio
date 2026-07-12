//! egui-Oberfläche: Komposition der Panels/Dialoge und das Theme. Bewusst nah an
//! den frischen Svelte-Designs (aktive-Farbe-Markierung, klare Sektionen). Alle
//! Aktionen laufen über den Core — die Panels halten keinen eigenen Wahrheits-
//! Zustand.
//!
//! Die einzelnen Panels und Dialoge liegen in den Untermodulen. Dieser
//! Root komponiert nur den Frame und hält das Theme. (Mechanischer Split; die
//! Panels bekommen vorerst weiterhin `&mut App` — die `UiAction`-Grenze folgt
//! als eigener Schritt.)

mod action;
mod arrange;
mod dialogs;
mod layers;
mod palette;
mod project;
mod status;
mod tools;
mod topbar;

pub use action::UiAction;

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
        app.fps,
        app.tool.label(),
        app.session.shapes.len(),
        app.project.msg.clone(),
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
        View::Design | View::Laser => {
            let cur_tool = app.tool;
            let left = egui::SidePanel::left("tools")
                .exact_width(96.0)
                .resizable(false)
                .show(ctx, |ui| tools::tools_panel(ui, cur_tool));
            app.left_w = left.response.rect.width();
            for action in left.inner {
                app.dispatch(action);
            }

            let is_laser = app.view == View::Laser;
            // Ebenen-Sicht vorab aus der Session ableiten (nur Lesezugriff),
            // damit das Panel keinen App-Zugriff braucht.
            let layer_rows: Vec<layers::LayerRow> = if is_laser {
                Vec::new()
            } else {
                layer_rows(app)
            };
            let right = egui::SidePanel::right("inspector")
                .exact_width(260.0)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.add_space(6.0);
                    if is_laser {
                        laserpanel::show(ui, app);
                        Vec::new()
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
                let show_shapes = app.tool == Tool::Polygon;
                let active_shape = app.active_shape;
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

    dialogs::laser_settings_window(ctx, app);
    dialogs::text_dialog_window(ctx, app);

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
            count: s.shapes.iter().filter(|sh| sh.layer_id == i).count(),
        })
        .collect()
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
