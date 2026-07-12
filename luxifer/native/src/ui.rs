//! egui-Panels: Werkzeugleiste (links), Layer + Palette (rechts). Bewusst nah an
//! den frischen Svelte-Designs (aktive-Farbe-Markierung, klare Sektionen). Alle
//! Aktionen laufen über den Core — die Panels halten keinen eigenen Wahrheits-
//! Zustand.

use egui::{Color32, RichText};
use luxifer_core::model::SWATCH_COLORS;

use crate::app::App;
use crate::laserpanel;
use crate::tools::Tool;

fn c32(rgb: [u8; 3]) -> Color32 {
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
                    app.state.undo();
                }
                if ui.button("Redo").clicked() {
                    app.state.redo();
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

    // Statuszeile unten.
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{:.0} fps", app.fps)).monospace());
            ui.separator();
            ui.label(format!("Werkzeug: {}", app.tool.label()));
            ui.separator();
            ui.label(format!("{} Objekte", app.state.shapes.len()));
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
            egui::CentralPanel::default().show(ctx, |ui| project_browser(ui, app));
        }
        View::Design | View::Laser => {
            let left = egui::SidePanel::left("tools")
                .exact_width(96.0)
                .resizable(false)
                .show(ctx, |ui| tools_panel(ui, app));
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
                        layers_panel(ui, app);
                    }
                });
            app.right_w = right.response.rect.width();

            // Farbpalette als Dock am unteren Canvas-Rand (nur Design), zentriert
            // wie das Palette-Dock der Tauri-App.
            if !is_laser {
                egui::TopBottomPanel::bottom("palette_dock")
                    .show_separator_line(true)
                    .show(ctx, |ui| {
                        ui.add_space(6.0);
                        ui.vertical_centered(|ui| palette_panel(ui, app));
                        ui.add_space(6.0);
                    });
            }
        }
    }

    laser_settings_window(ctx, app);
    text_dialog_window(ctx, app);
}

/// Projekt-Browser (Reiter „Projekt"): Liste + Neu/Öffnen/Speichern.
fn project_browser(ui: &mut egui::Ui, app: &mut App) {
    ui.add_space(8.0);
    ui.heading("Projekte");
    ui.add_space(8.0);

    // Aktionszeile: Neu + Speichern.
    ui.horizontal(|ui| {
        ui.label("Neu:");
        ui.add(
            egui::TextEdit::singleline(&mut app.new_project_name)
                .hint_text("Projektname")
                .desired_width(200.0),
        );
        if ui.button("Anlegen").clicked() {
            let name = app.new_project_name.clone();
            app.project_new(&name);
            app.new_project_name.clear();
        }
        ui.separator();
        if ui.button("Speichern").clicked() {
            app.project_save();
        }
        if ui.button("Neue Version").clicked() {
            app.project_save_version();
        }
    });
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(10.0);

    // Projektliste.
    let projects = app.project.list();
    if projects.is_empty() {
        ui.weak("Noch keine Projekte gespeichert.");
        return;
    }
    let open_name = app.project.open_name().map(|s| s.to_string());
    egui::ScrollArea::vertical().show(ui, |ui| {
        for p in &projects {
            let is_open = open_name.as_deref() == Some(p.name.as_str());
            ui.horizontal(|ui| {
                let title = if is_open {
                    RichText::new(&p.name).strong()
                } else {
                    RichText::new(&p.name)
                };
                ui.label(title);
                if !p.modified_at.is_empty() {
                    ui.weak(RichText::new(&p.modified_at).small());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Öffnen").clicked() {
                        app.project_open(&p.name);
                    }
                });
            });
            ui.separator();
        }
    });
}

/// Text-Dialog: Eingabe, Font-Auswahl, Größe → Text als Pfad einfügen.
fn text_dialog_window(ctx: &egui::Context, app: &mut App) {
    if app.text_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut commit = false;
    // Font-Liste (Name) für die ComboBox vorbereiten.
    let font_names: Vec<String> = app.fonts.iter().map(|f| f.name.clone()).collect();
    egui::Window::new("Text einfügen")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
            let st = app.text_dialog.as_mut().unwrap();
            ui.label("Text");
            ui.add(
                egui::TextEdit::multiline(&mut st.text)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(6.0);
            egui::Grid::new("text_cfg")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Font");
                    let current = st
                        .font_idx
                        .and_then(|i| font_names.get(i).cloned())
                        .unwrap_or_else(|| "—".into());
                    egui::ComboBox::from_id_salt("font")
                        .selected_text(current)
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in font_names.iter().enumerate() {
                                if ui.selectable_label(st.font_idx == Some(i), name).clicked() {
                                    st.font_idx = Some(i);
                                }
                            }
                        });
                    ui.end_row();
                    ui.label("Größe (mm)");
                    ui.add(
                        egui::DragValue::new(&mut st.size_mm)
                            .range(1.0..=500.0)
                            .speed(0.5),
                    );
                    ui.end_row();
                });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Einfügen").clicked() {
                    commit = true;
                }
                if ui.button("Abbrechen").clicked() {
                    close = true;
                }
            });
        });

    if commit && app.commit_text() {
        close = true;
    }
    if close {
        app.text_dialog = None;
    }
}

/// Modaler Laser-Einstellungen-Dialog (Profil anlegen/bearbeiten/löschen).
fn laser_settings_window(ctx: &egui::Context, app: &mut App) {
    use luxifer_core::{Connection, DriverKind};
    let Some(mut profile) = app.laser_settings.take() else {
        return;
    };
    let mut action: Option<&str> = None;
    egui::Window::new("Laser-Einstellungen")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            egui::Grid::new("laser_cfg")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut profile.name);
                    ui.end_row();

                    ui.label("Typ");
                    egui::ComboBox::from_id_salt("kind")
                        .selected_text(format!("{:?}", profile.kind))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut profile.kind, DriverKind::Ruida, "Ruida");
                            ui.selectable_value(&mut profile.kind, DriverKind::Grbl, "GRBL");
                            ui.selectable_value(
                                &mut profile.kind,
                                DriverKind::MiniGrbl,
                                "miniGRBL",
                            );
                        });
                    ui.end_row();

                    // Verbindung: je nach Treiber Netz (IP) oder Seriell (Port).
                    match &mut profile.connection {
                        Connection::Netz { ip, .. } => {
                            ui.label("IP-Adresse");
                            ui.text_edit_singleline(ip);
                            ui.end_row();
                        }
                        Connection::Seriell { port, baud } => {
                            ui.label("Port");
                            ui.text_edit_singleline(port);
                            ui.end_row();
                            ui.label("Baud");
                            ui.add(egui::DragValue::new(baud));
                            ui.end_row();
                        }
                    }

                    ui.label("Bett B×H (mm)");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut profile.bed_mm.0).speed(1.0));
                        ui.label("×");
                        ui.add(egui::DragValue::new(&mut profile.bed_mm.1).speed(1.0));
                    });
                    ui.end_row();
                });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Speichern").clicked() {
                    action = Some("save");
                }
                if !profile.id.is_empty() && ui.button("Löschen").clicked() {
                    action = Some("delete");
                }
                if ui.button("Abbrechen").clicked() {
                    action = Some("cancel");
                }
            });
        });

    match action {
        Some("save") => {
            app.laser_settings = Some(profile);
            app.save_laser_settings();
        }
        Some("delete") => {
            app.delete_laser_profile(&profile.id.clone());
        }
        Some("cancel") => {}
        // Keine Aktion + Fenster noch offen → Bearbeitungsstand behalten.
        _ => app.laser_settings = Some(profile),
    }
}

/// Werkzeug-Knopf mit gemaltem Icon + Label. `on` = aktiv (Akzent-Hintergrund).
/// Gibt true bei Klick zurück.
fn tool_button(ui: &mut egui::Ui, on: bool, tool: Tool) -> bool {
    let size = egui::vec2(ui.available_width(), 38.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let accent = Color32::from_rgb(0x3B, 0x82, 0xF6);
    let bg = if on {
        accent.gamma_multiply(0.85)
    } else if resp.hovered() {
        Color32::from_rgb(0x25, 0x2a, 0x33)
    } else {
        Color32::from_rgb(0x1c, 0x1f, 0x26)
    };
    ui.painter().rect(
        rect,
        8.0,
        bg,
        egui::Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2e, 0x36)),
    );
    let fg = Color32::from_rgb(0xec, 0xee, 0xf1);
    // Icon-Bereich links (quadratisch), Label rechts.
    let ic = egui::Rect::from_min_size(rect.min + egui::vec2(8.0, 7.0), egui::vec2(24.0, 24.0));
    let c = ic.center();
    let p = ui.painter();
    let stroke = egui::Stroke::new(1.6, fg);
    match tool {
        Tool::Select => {
            // Cursor-Pfeil.
            let pts = vec![
                c + egui::vec2(-6.0, -7.0),
                c + egui::vec2(-6.0, 7.0),
                c + egui::vec2(-1.5, 2.5),
                c + egui::vec2(2.0, 8.0),
                c + egui::vec2(4.5, 6.5),
                c + egui::vec2(1.0, 1.0),
                c + egui::vec2(7.0, 0.0),
            ];
            p.add(egui::Shape::convex_polygon(pts, fg, egui::Stroke::NONE));
        }
        Tool::Rect => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(15.0, 12.0)),
                1.5,
                stroke,
            );
        }
        Tool::Ellipse => {
            p.circle_stroke(c, 8.0, stroke);
        }
        Tool::Polygon => {
            // Dreieck/Polygon-Umriss.
            let pts = vec![
                c + egui::vec2(0.0, -8.0),
                c + egui::vec2(8.0, 4.0),
                c + egui::vec2(-8.0, 4.0),
            ];
            p.add(egui::Shape::closed_line(pts, stroke));
        }
    }
    p.text(
        egui::pos2(ic.right() + 8.0, c.y),
        egui::Align2::LEFT_CENTER,
        tool.label(),
        egui::FontId::proportional(13.0),
        fg,
    );
    resp.clicked()
}

fn tools_panel(ui: &mut egui::Ui, app: &mut App) {
    ui.add_space(6.0);
    ui.label(RichText::new("WERKZEUG").small().weak());
    ui.add_space(4.0);
    for t in [Tool::Select, Tool::Rect, Tool::Ellipse, Tool::Polygon] {
        if tool_button(ui, app.tool == t, t) {
            app.tool = t;
        }
    }
    // Undo/Redo + Datei-Aktionen liegen jetzt im Header; „Fill an/aus" macht der
    // Layer-Modus. Das Werkzeug-Panel bleibt bewusst schlank.
}

fn layers_panel(ui: &mut egui::Ui, app: &mut App) {
    ui.label(RichText::new("EBENEN").small().weak());
    ui.add_space(4.0);
    if app.state.layers.is_empty() {
        ui.weak("Keine Ebenen — zeichne etwas.");
        return;
    }
    // Von oben (letzter Layer) nach unten anzeigen.
    let n = app.state.layers.len();
    for i in (0..n).rev() {
        let (color, name, mut visible, count) = {
            let l = &app.state.layers[i];
            let cnt = app.state.shapes.iter().filter(|s| s.layer_id == i).count();
            (l.color, l.name.clone(), l.visible, cnt)
        };
        ui.horizontal(|ui| {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
            ui.painter().rect_filled(rect, 4.0, c32(color));
            if resp.clicked() {
                app.pick_color(color);
            }
            if ui.checkbox(&mut visible, "").changed() {
                app.state.layers[i].visible = visible;
            }
            ui.label(format!("{name}  ·  {count}"));
        });
    }
}

fn palette_panel(ui: &mut egui::Ui, app: &mut App) {
    ui.label(RichText::new("FARBE").small().weak());
    ui.add_space(6.0);
    let active = app.accent;
    ui.horizontal_wrapped(|ui| {
        for &sw in SWATCH_COLORS {
            let is_active = sw == active;
            let size = if is_active { 26.0 } else { 22.0 };
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
            let r = size * 0.5;
            ui.painter().circle_filled(rect.center(), r, c32(sw));
            if is_active {
                // Heller Ring mit dunklem Absatz — wie in der Svelte-Palette.
                ui.painter()
                    .circle_stroke(rect.center(), r + 1.5, (2.0, Color32::from_gray(20)));
                ui.painter()
                    .circle_stroke(rect.center(), r + 3.0, (2.0, Color32::from_gray(235)));
            }
            if resp.hovered() {
                ui.painter()
                    .circle_stroke(rect.center(), r, (1.5, Color32::WHITE));
            }
            if resp.clicked() {
                app.pick_color(sw);
            }
        }
    });
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
