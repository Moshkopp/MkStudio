//! egui-Panels: Werkzeugleiste (links), Layer + Palette (rechts). Bewusst nah an
//! den frischen Svelte-Designs (aktive-Farbe-Markierung, klare Sektionen). Alle
//! Aktionen laufen über den Core — die Panels halten keinen eigenen Wahrheits-
//! Zustand.

use egui::{Color32, RichText};
use luxifer_application::LayerToggle;
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
    // im Design-Reiter. Wie in der Tauri-App liegt das im Kopf.
    if app.view == View::Design {
        egui::TopBottomPanel::top("arrange").show(ctx, |ui| {
            ui.add_space(3.0);
            arrange_bar(ui, app);
            ui.add_space(3.0);
        });
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

            // Farbpalette (+ Form-Wähler beim Polygon-Werkzeug) als Dock am
            // unteren Canvas-Rand (nur Design), zentriert wie in der Tauri-App.
            if !is_laser {
                egui::TopBottomPanel::bottom("palette_dock")
                    .show_separator_line(true)
                    .show(ctx, |ui| {
                        ui.add_space(6.0);
                        if app.tool == Tool::Polygon {
                            ui.vertical_centered(|ui| shape_picker(ui, app));
                            ui.add_space(4.0);
                        }
                        ui.vertical_centered(|ui| palette_panel(ui, app));
                        ui.add_space(6.0);
                    });
            }
        }
    }

    laser_settings_window(ctx, app);
    text_dialog_window(ctx, app);
    layer_dialog_window(ctx, app);
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

/// Layer-Parameter-Dialog (Doppelklick auf eine Ebene). Native hält nur den
/// Entwurf; Speichern läuft über `EditorSession::set_layer_params` mit
/// Validierung, Abbrechen verwirft ihn ohne Mutation. Die Bild-Invariante wird
/// in der UI durch einen festen Modus für Image-Layer gespiegelt und im Core
/// zusätzlich erzwungen.
fn layer_dialog_window(ctx: &egui::Context, app: &mut App) {
    use luxifer_core::LayerMode;
    if app.layer_dialog.is_none() {
        return;
    }
    let mut commit = false;
    let mut close = false;
    egui::Window::new("Ebene bearbeiten")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
            let p = &mut app.layer_dialog.as_mut().unwrap().params;
            let is_image = p.mode == LayerMode::Image;

            egui::Grid::new("layer_cfg")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(220.0));
                    ui.end_row();

                    ui.label("Modus");
                    if is_image {
                        // Bild-Layer: Modus ist fest (kein Asset-loser Vektor).
                        ui.label(RichText::new("Bild — Rastergravur").weak());
                    } else {
                        let mode_label = |m: LayerMode| match m {
                            LayerMode::Cut => "Schneiden",
                            LayerMode::Fill => "Füllen",
                            LayerMode::Raster => "Raster",
                            LayerMode::Image => "Bild",
                        };
                        egui::ComboBox::from_id_salt("layer_mode")
                            .selected_text(mode_label(p.mode))
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                for m in [LayerMode::Cut, LayerMode::Fill, LayerMode::Raster] {
                                    ui.selectable_value(&mut p.mode, m, mode_label(m));
                                }
                            });
                    }
                    ui.end_row();

                    ui.label("Speed (mm/s)");
                    ui.add(
                        egui::DragValue::new(&mut p.speed_mm_s)
                            .range(1.0..=10000.0)
                            .speed(1.0),
                    );
                    ui.end_row();

                    ui.label("Durchläufe");
                    ui.add(egui::DragValue::new(&mut p.passes).range(1..=100));
                    ui.end_row();

                    ui.label("Power max (%)");
                    ui.add(
                        egui::DragValue::new(&mut p.power_pct)
                            .range(0.0..=100.0)
                            .speed(0.5),
                    );
                    ui.end_row();

                    ui.label("Power min (%)");
                    ui.add(
                        egui::DragValue::new(&mut p.min_power_pct)
                            .range(0.0..=100.0)
                            .speed(0.5),
                    );
                    ui.end_row();

                    // Rasterparameter (DPI + Bidirektional) für Image/Raster,
                    // sonst Zeilenabstand für Fill.
                    if is_image || p.mode == LayerMode::Raster {
                        ui.label("DPI");
                        ui.add(
                            egui::DragValue::new(&mut p.dpi)
                                .range(1.0..=2540.0)
                                .speed(1.0),
                        );
                        ui.end_row();
                        ui.label("Bidirektional");
                        ui.checkbox(&mut p.bidirectional, "");
                        ui.end_row();
                    } else if p.mode == LayerMode::Fill {
                        ui.label("Linienabstand (mm)");
                        ui.add(
                            egui::DragValue::new(&mut p.line_step_mm)
                                .range(0.01..=10.0)
                                .speed(0.01),
                        );
                        ui.end_row();
                    }

                    ui.label("Air Assist");
                    ui.checkbox(&mut p.air_assist, "");
                    ui.end_row();
                });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Speichern").clicked() {
                    commit = true;
                }
                if ui.button("Abbrechen").clicked() {
                    close = true;
                }
            });
        });

    if commit && app.commit_layer_dialog() {
        close = true;
    }
    if close {
        app.layer_dialog = None;
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

/// Quadratischer Icon-Button (Werkzeugleiste). `on` = aktiv (Akzent),
/// `dim` = Stub/deaktiviert dezenter. Gibt true bei Klick zurück.
fn icon_button(ui: &mut egui::Ui, side: f32, icon: &str, tip: &str, on: bool, dim: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
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
        7.0,
        bg,
        egui::Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2e, 0x36)),
    );
    let fg = if dim {
        Color32::from_rgb(0x9a, 0xa0, 0xa9)
    } else {
        Color32::from_rgb(0xec, 0xee, 0xf1)
    };
    // Icon-Box zentriert (etwas kleiner als der Button).
    let pad = side * 0.22;
    let ic = egui::Rect::from_min_max(
        rect.min + egui::vec2(pad, pad),
        rect.max - egui::vec2(pad, pad),
    );
    crate::icons::draw(ui.painter(), ic, icon, fg);
    resp.on_hover_text(tip).clicked()
}

/// Werkzeuge in einem 2-Spalten-Grid; gibt das geklickte Werkzeug zurück.
fn tool_grid(ui: &mut egui::Ui, side: f32, gap: f32, cur: Tool, tools: &[Tool]) -> Option<Tool> {
    let mut clicked = None;
    egui::Grid::new(("tg", tools.first().map(|t| t.label()).unwrap_or("")))
        .spacing([gap, gap])
        .show(ui, |ui| {
            for (i, &t) in tools.iter().enumerate() {
                if icon_button(ui, side, t.icon(), t.label(), cur == t, false) {
                    clicked = Some(t);
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    clicked
}

/// 2-spaltige Werkzeugleiste, 5 Gruppen wie die Tauri-ToolsPanel — nur Icons.
fn tools_panel(ui: &mut egui::Ui, app: &mut App) {
    use crate::tools::ToolAction as A;
    ui.add_space(4.0);
    let full = ui.available_width();
    let gap = 4.0;
    let side = ((full - gap) / 2.0).clamp(24.0, 42.0);

    let cur = app.tool;
    // Gruppe 1: Auswahl (breit über beide Spalten).
    if icon_button(
        ui,
        full.min(side * 2.0 + gap),
        "select",
        "Auswahl / Verschieben",
        cur == Tool::Select,
        false,
    ) {
        app.tool = Tool::Select;
    }
    divider(ui);
    // Gruppe 2: Zeichnen & Formen.
    if let Some(t) = tool_grid(
        ui,
        side,
        gap,
        cur,
        &[
            Tool::Rect,
            Tool::Ellipse,
            Tool::Polygon,
            Tool::Line,
            Tool::Polyline,
            Tool::Spline,
            Tool::Bezier,
        ],
    ) {
        app.tool = t;
    }
    // Text (Sofort-Aktion) + Node (Werkzeug) in derselben Gruppe.
    egui::Grid::new("tg_textnode")
        .spacing([gap, gap])
        .show(ui, |ui| {
            if icon_button(ui, side, "text", "Text einfügen (Text→Pfad)", false, false) {
                app.open_text_dialog();
            }
            if icon_button(
                ui,
                side,
                "node",
                "Knoten bearbeiten",
                app.tool == Tool::Node,
                false,
            ) {
                app.tool = Tool::Node;
            }
            ui.end_row();
        });
    divider(ui);
    // Gruppe 3: Operationen. `trim` bleibt Stub (wie Tauri).
    egui::Grid::new("tg_ops")
        .spacing([gap, gap])
        .show(ui, |ui| {
            icon_button(ui, side, "trim", "Trimmen (Vorschau)", false, true);
            if icon_button(ui, side, "bridge", "Haltesteg (Klick+Ziehen)", false, false) {
                app.begin_action(A::Bridge);
            }
            ui.end_row();
            if icon_button(ui, side, "boolean", "Boolean (Auswahl)", false, false) {
                app.begin_action(A::Boolean);
            }
            if icon_button(
                ui,
                side,
                "fillet",
                "Ecken verrunden (Auswahl)",
                false,
                false,
            ) {
                app.begin_action(A::Fillet);
            }
            ui.end_row();
            if icon_button(
                ui,
                side,
                "pattern-fill",
                "Muster füllen (Auswahl)",
                false,
                false,
            ) {
                app.begin_action(A::PatternFill);
            }
            if icon_button(
                ui,
                side,
                "offset",
                "Offset / parallele Kontur (Auswahl)",
                false,
                false,
            ) {
                app.begin_action(A::Offset);
            }
            ui.end_row();
            if icon_button(
                ui,
                side,
                "measure",
                "Messen (Klick+Ziehen)",
                app.tool == Tool::Measure,
                false,
            ) {
                app.tool = Tool::Measure;
            }
            ui.end_row();
        });
    divider(ui);
    // Gruppe 4: Spiegeln.
    egui::Grid::new("tg_mirror")
        .spacing([gap, gap])
        .show(ui, |ui| {
            if icon_button(ui, side, "mirror-h", "Horizontal spiegeln", false, false) {
                app.mirror_h();
            }
            if icon_button(ui, side, "mirror-v", "Vertikal spiegeln", false, false) {
                app.mirror_v();
            }
            ui.end_row();
        });
    divider(ui);
    // Gruppe 5: Untersetzer.
    egui::Grid::new("tg_coaster")
        .spacing([gap, gap])
        .show(ui, |ui| {
            if icon_button(
                ui,
                side,
                "coaster-rect",
                "4×2 eckige Untersetzer",
                false,
                false,
            ) {
                app.insert_coasters(false);
            }
            if icon_button(
                ui,
                side,
                "coaster-circle",
                "4×2 runde Untersetzer",
                false,
                false,
            ) {
                app.insert_coasters(true);
            }
            ui.end_row();
        });
}

/// Dünner horizontaler Trenner zwischen Werkzeuggruppen.
fn divider(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let w = ui.available_width() * 0.8;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    let y = rect.center().y;
    let x0 = rect.center().x - w / 2.0;
    ui.painter().line_segment(
        [egui::pos2(x0, y), egui::pos2(x0 + w, y)],
        egui::Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2e, 0x36)),
    );
    ui.add_space(4.0);
}

fn layers_panel(ui: &mut egui::Ui, app: &mut App) {
    ui.label(RichText::new("EBENEN").small().weak());
    ui.add_space(4.0);
    if app.session.layers.is_empty() {
        ui.weak("Keine Ebenen — zeichne etwas.");
        return;
    }
    // Von oben (letzter Layer) nach unten anzeigen.
    let n = app.session.layers.len();
    for i in (0..n).rev() {
        let (color, name, visible, enabled, locked, air_assist, count) = {
            let l = &app.session.layers[i];
            let cnt = app
                .session
                .shapes
                .iter()
                .filter(|s| s.layer_id == i)
                .count();
            (
                l.color,
                l.name.clone(),
                l.visible,
                l.enabled,
                l.locked,
                l.air_assist,
                cnt,
            )
        };
        ui.horizontal(|ui| {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
            ui.painter().rect_filled(rect, 4.0, c32(color));
            if resp.clicked() {
                app.pick_color(color);
            }
            if ui
                .selectable_label(visible, "S")
                .on_hover_text("Im Canvas sichtbar")
                .clicked()
            {
                app.toggle_layer(i, LayerToggle::Visible);
            }
            if ui
                .selectable_label(enabled, "J")
                .on_hover_text("Im Laserjob aktiviert")
                .clicked()
            {
                app.toggle_layer(i, LayerToggle::Enabled);
            }
            if ui
                .selectable_label(locked, "L")
                .on_hover_text("Bearbeitung sperren")
                .clicked()
            {
                app.toggle_layer(i, LayerToggle::Locked);
            }
            if ui
                .selectable_label(air_assist, "A")
                .on_hover_text("Luftunterstützung")
                .clicked()
            {
                app.toggle_layer(i, LayerToggle::AirAssist);
            }
            if ui
                .add(egui::Label::new(format!("{name}  ·  {count}")).sense(egui::Sense::click()))
                .on_hover_text("Doppelklick: Parameter bearbeiten")
                .double_clicked()
            {
                app.open_layer_dialog(i);
            }
            if ui
                .small_button("↑")
                .on_hover_text("Ebene nach oben")
                .clicked()
                && i + 1 < n
            {
                app.move_layer(i, i + 1);
            }
            if ui
                .small_button("↓")
                .on_hover_text("Ebene nach unten")
                .clicked()
                && i > 0
            {
                app.move_layer(i, i - 1);
            }
        });
    }
}

/// Form-Wähler für das Polygon-Werkzeug (Dreieck/Stern/… wie Tauri-ShapesPanel).
fn shape_picker(ui: &mut egui::Ui, app: &mut App) {
    use luxifer_core::PolyShape as P;
    let shapes = [
        (P::Tri, "tri"),
        (P::Quad, "quad"),
        (P::Penta, "penta"),
        (P::Hex, "hex"),
        (P::Octa, "octa"),
        (P::Star, "star"),
        (P::Sun, "sun"),
        (P::Gear, "gear"),
        (P::Heart, "heart"),
    ];
    ui.horizontal(|ui| {
        for (shape, icon) in shapes {
            let on = app.active_shape == shape;
            if icon_button(ui, 30.0, icon, "", on, false) {
                app.active_shape = shape;
            }
        }
    });
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
/// Kleiner horizontaler Icon-Knopf (Anordnen-Leiste). `dim` = deaktiviert.
fn bar_icon(ui: &mut egui::Ui, icon: &str, tip: &str, enabled: bool) -> bool {
    let side = 28.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
    let hov = resp.hovered() && enabled;
    let bg = if hov {
        Color32::from_rgb(0x25, 0x2a, 0x33)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 6.0, bg);
    let fg = if enabled {
        Color32::from_rgb(0xd4, 0xd8, 0xdd)
    } else {
        Color32::from_rgb(0x55, 0x5a, 0x62)
    };
    let pad = side * 0.2;
    let ic = egui::Rect::from_min_max(
        rect.min + egui::vec2(pad, pad),
        rect.max - egui::vec2(pad, pad),
    );
    crate::icons::draw(ui.painter(), ic, icon, fg);
    enabled && resp.on_hover_text(tip).clicked()
}

/// Anordnen-Leiste: Ausrichten (7), Verteilen (4), Gruppieren/Lösen, Nesting.
fn arrange_bar(ui: &mut egui::Ui, app: &mut App) {
    use luxifer_core::{Align, Distribute};
    let n = app.selection_count();
    ui.horizontal(|ui| {
        // Ausrichten (ab 1 Objekt).
        let a1 = n >= 1;
        if bar_icon(ui, "align-left", "Links ausrichten", a1) {
            app.align(Align::Left);
        }
        if bar_icon(ui, "align-hcenter", "Horizontal zentrieren", a1) {
            app.align(Align::HCenter);
        }
        if bar_icon(ui, "align-right", "Rechts ausrichten", a1) {
            app.align(Align::Right);
        }
        ui.add_space(2.0);
        if bar_icon(ui, "align-top", "Oben ausrichten", a1) {
            app.align(Align::Top);
        }
        if bar_icon(ui, "align-vcenter", "Vertikal zentrieren", a1) {
            app.align(Align::VCenter);
        }
        if bar_icon(ui, "align-bottom", "Unten ausrichten", a1) {
            app.align(Align::Bottom);
        }
        if bar_icon(ui, "align-center", "Auf beiden Achsen zentrieren", a1) {
            app.align(Align::Center);
        }
        ui.separator();
        // Verteilen (ab 3 Objekten).
        let a3 = n >= 3;
        if bar_icon(ui, "dist-h", "Horizontal verteilen", a3) {
            app.distribute(Distribute::Horizontal);
        }
        if bar_icon(ui, "space-h", "Horizontale Abstände angleichen", a3) {
            app.distribute(Distribute::SpaceHorizontal);
        }
        if bar_icon(ui, "dist-v", "Vertikal verteilen", a3) {
            app.distribute(Distribute::Vertical);
        }
        if bar_icon(ui, "space-v", "Vertikale Abstände angleichen", a3) {
            app.distribute(Distribute::SpaceVertical);
        }
        ui.separator();
        // Gruppieren.
        if bar_icon(ui, "group", "Gruppieren", n >= 2) {
            app.group();
        }
        if bar_icon(ui, "ungroup", "Gruppierung lösen", n >= 1) {
            app.ungroup();
        }
        ui.separator();
        // Nesting: Packen (≥2) / Bett füllen (≥1), fester Abstand 2 mm.
        if bar_icon(ui, "nest", "Auswahl packen (2 mm)", n >= 2) {
            app.nest(2.0);
        }
        if ui
            .add_enabled(n >= 1, egui::Button::new("Bett füllen"))
            .clicked()
        {
            app.nest_fill(2.0);
        }
    });
}

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
