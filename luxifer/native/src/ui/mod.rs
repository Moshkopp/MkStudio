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
mod tools;

pub use action::UiAction;

use egui::{Color32, RichText};

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

    // Oben: Reiter | Undo/Redo + Datei-Aktionen | Projektname. Wie die Tauri-App
    // liegen die globalen Aktionen im Header, nicht im Werkzeug-Panel.
    egui::TopBottomPanel::top("topbar").show(ctx, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for v in [View::Projekt, View::Design, View::Laser] {
                if ui
                    .selectable_label(app.view == v, format!("  {}  ", v.label()))
                    .clicked()
                {
                    app.view = v;
                }
            }
            // Datei-/Verlaufs-Aktionen nur im Design-Reiter.
            if app.view == View::Design {
                ui.separator();
                if ui.button("Undo").clicked() {
                    app.undo();
                }
                if ui.button("Redo").clicked() {
                    app.redo();
                }
                ui.separator();
                if ui.button("Vektor…").clicked() {
                    app.import_dialog();
                }
                if ui.button("Bild…").clicked() {
                    app.import_image_dialog();
                }
                if ui.button("Text…").clicked() {
                    app.open_text_dialog();
                }
                let aztec = std::path::Path::new("/home/moshy/Schreibtisch/Aztec.svg");
                if aztec.exists() && ui.button("Aztec laden").clicked() {
                    app.import_path(aztec);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let name = app
                    .project
                    .open_name()
                    .unwrap_or("— (ungespeichert)")
                    .to_string();
                ui.label(RichText::new(name).weak());
            });
        });
        ui.add_space(4.0);
    });

    if let Some(error) = app.app_error.as_ref() {
        let code = error.code();
        let message = error.message().to_string();
        egui::TopBottomPanel::top("application_error").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(
                    Color32::from_rgb(0xf8, 0x71, 0x71),
                    format!("{message}  [{code}]"),
                );
                if ui.small_button("Schließen").clicked() {
                    app.app_error = None;
                }
            });
        });
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

    // Statuszeile unten.
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{:.0} fps", app.fps)).monospace());
            ui.separator();
            ui.label(format!("Werkzeug: {}", app.tool.label()));
            ui.separator();
            ui.label(format!("{} Objekte", app.session.shapes.len()));
            if !app.project.msg.is_empty() {
                ui.separator();
                ui.label(RichText::new(&app.project.msg).weak());
            }
        });
    });

    match app.view {
        View::Projekt => {
            app.left_w = 0.0;
            app.right_w = 0.0;
            egui::CentralPanel::default().show(ctx, |ui| project::project_browser(ui, app));
        }
        View::Design | View::Laser => {
            let left = egui::SidePanel::left("tools")
                .exact_width(96.0)
                .resizable(false)
                .show(ctx, |ui| tools::tools_panel(ui, app));
            app.left_w = left.response.rect.width();

            let is_laser = app.view == View::Laser;
            let right = egui::SidePanel::right("inspector")
                .exact_width(260.0)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.add_space(6.0);
                    if is_laser {
                        laserpanel::show(ui, app);
                    } else {
                        layers::layers_panel(ui, app);
                    }
                });
            app.right_w = right.response.rect.width();

            // Farbpalette (+ Form-Wähler beim Polygon-Werkzeug) als Dock am
            // unteren Canvas-Rand (nur Design), zentriert wie in der Tauri-App.
            if !is_laser {
                egui::TopBottomPanel::bottom("palette_dock")
                    .show_separator_line(true)
                    .show(ctx, |ui| {
                        ui.add_space(6.0);
                        if app.tool == Tool::Polygon {
                            ui.vertical_centered(|ui| palette::shape_picker(ui, app));
                            ui.add_space(4.0);
                        }
                        ui.vertical_centered(|ui| palette::palette_panel(ui, app));
                        ui.add_space(6.0);
                    });
            }
        }
    }

    dialogs::laser_settings_window(ctx, app);
    dialogs::text_dialog_window(ctx, app);
    dialogs::layer_dialog_window(ctx, app);
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
