//! Laser-Bedienpanel in egui, nach dem frischen Svelte-Design (ADR 0007 +
//! Redesign): Ampel-Grid (Start grün / Pause orange / Stopp rot / Ursprung blau /
//! Rahmen · Gummiband), Job-Parameter, Job-Nullpunkt-Anker (3×3), Jog-Kreuz +
//! Slider. Die Kacheln lösen `UiAction::LaserRun` aus; der Root führt sie über
//! den `LaserService` aus (echte Treiber-Aktionen, hardwarelos getestet).

use egui::{Color32, RichText, Sense, Vec2};
use luxifer_core::{JobAction, StartMode};

use crate::tools::LaserUi;
use crate::ui::UiAction;

/// Reine Sicht auf den Laser-Zustand für das Panel (vom Root abgeleitet, damit
/// das Panel weder Backend noch `App` kennt). `slots` ist die feste 2×3-
/// Ampelbelegung; ein `None`-Slot bleibt leer.
pub struct LaserView {
    /// (id, Anzeige-Label) aller Profile.
    pub profiles: Vec<(String, String)>,
    /// Id des aktiven Profils (leer, wenn keins).
    pub active_id: String,
    /// Ampel-Slots aus den echten Treiber-Aktionen.
    pub slots: [Option<JobAction>; 6],
    /// Ob der aktive Treiber Datei-Export unterstützt.
    pub can_export: bool,
    /// Bewusst aufgebauter Verbindungszustand des aktiven Profils.
    pub connected: bool,
}

/// Farb-Ton der Ampel-Kacheln.
enum Tone {
    Go,
    Warn,
    Stop,
    Nav,
    Neutral,
}

fn tone_colors(t: &Tone) -> (Color32, Color32) {
    // (Füllung, Textfarbe)
    match t {
        Tone::Go => (
            Color32::from_rgb(0x2f, 0xa5, 0x6b),
            Color32::from_rgb(0xea, 0xff, 0xf5),
        ),
        Tone::Warn => (
            Color32::from_rgb(0xe0, 0x93, 0x00),
            Color32::from_rgb(0x24, 0x18, 0x00),
        ),
        Tone::Stop => (
            Color32::from_rgb(0xd2, 0x46, 0x3c),
            Color32::from_rgb(0xff, 0xf0, 0xee),
        ),
        Tone::Nav => (
            Color32::from_rgb(0x35, 0x6f, 0xb0),
            Color32::from_rgb(0xee, 0xf4, 0xff),
        ),
        Tone::Neutral => (
            Color32::from_rgb(0x2a, 0x2f, 0x38),
            Color32::from_rgb(0xec, 0xee, 0xf1),
        ),
    }
}

/// Ordnet einer Job-Aktion Label, Ton und Rasterplatz zu (feste 2×3-Reihenfolge
/// wie im Svelte-Design). None-Slots bleiben leer.
fn action_meta(a: JobAction) -> (&'static str, Tone) {
    match a {
        JobAction::SendJob | JobAction::StreamGcode => ("Start", Tone::Go),
        JobAction::Pause => ("Pause", Tone::Warn),
        JobAction::Stop => ("Stopp", Tone::Stop),
        JobAction::GoOrigin => ("Ursprung", Tone::Nav),
        JobAction::Frame => ("Rahmen", Tone::Neutral),
        JobAction::RubberFrame => ("Gummiband", Tone::Neutral),
        JobAction::Home => ("Home", Tone::Neutral),
        JobAction::ExportFile => ("Export", Tone::Neutral),
    }
}

/// Zeichnet das Panel. `view` ist die vom Root abgeleitete Sicht, `ui_state`
/// der bearbeitbare Präsentationsentwurf (Slider/Anker/Startmodus). Gibt die
/// ausgelösten Absichten zurück; das Panel kennt weder Backend noch `App`.
pub fn show(ui: &mut egui::Ui, view: &LaserView, ui_state: &mut LaserUi) -> Vec<UiAction> {
    let mut actions = Vec::new();

    // Kopf: „LASER" + aktuelles Profil.
    ui.label(RichText::new("LASER").small().weak());
    ui.add_space(4.0);
    if view.profiles.is_empty() {
        if ui.button("+ Laser anlegen").clicked() {
            actions.push(UiAction::OpenLaserManager { create_new: true });
        }
    } else {
        let active_label = view
            .profiles
            .iter()
            .find(|(id, _)| *id == view.active_id)
            .map(|(_, l)| l.clone())
            .unwrap_or_else(|| "—".into());
        // Rechts-nach-links: erst der „Verwalten"-Knopf am Panelrand, dann
        // füllt die Combo exakt den Rest — nichts läuft über die Panelbreite.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Verwalten")
                    .on_hover_text("Laser verwalten")
                    .clicked()
                {
                    actions.push(UiAction::OpenLaserManager { create_new: false });
                }
                egui::ComboBox::from_id_salt("laser_sel")
                    .selected_text(active_label)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for (id, label) in &view.profiles {
                            if ui.selectable_label(*id == view.active_id, label).clicked() {
                                actions.push(UiAction::LaserSelect(id.clone()));
                            }
                        }
                    });
            });
        });
    }
    if !view.profiles.is_empty() {
        ui.horizontal(|ui| {
            let (color, label) = if view.connected {
                (Color32::from_rgb(0x34, 0xd3, 0x99), "● Verbunden")
            } else {
                (ui.visuals().weak_text_color(), "● Getrennt")
            };
            ui.colored_label(color, label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let clicked = if view.connected {
                    ui.button("Trennen").clicked()
                } else {
                    ui.button("Verbinden").clicked()
                };
                if clicked {
                    actions.push(if view.connected {
                        UiAction::LaserDisconnect
                    } else {
                        UiAction::LaserConnect
                    });
                }
            });
        });
    }
    ui.add_space(10.0);

    // Ampel-Grid aus den ECHTEN Aktionen des aktiven Treibers (feste Slots).
    ui.label(RichText::new("JOB").small().weak());
    ui.add_space(4.0);
    let avail = ui.available_width();
    let gap = 6.0;
    let cell_w = (avail - 2.0 * gap) / 3.0;
    let cell_h = cell_w * 0.72;
    egui::Grid::new("ampel")
        .spacing(Vec2::splat(gap))
        .show(ui, |ui| {
            for (i, slot) in view.slots.iter().enumerate() {
                match slot {
                    Some(a) => {
                        let (label, tone) = action_meta(*a);
                        if ampel_cell(ui, label, &tone, cell_w, cell_h, view.connected) {
                            actions.push(UiAction::LaserRun(*a));
                        }
                    }
                    None => {
                        ui.allocate_exact_size(Vec2::new(cell_w, cell_h), Sense::hover());
                    }
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });

    ui.add_space(8.0);
    ui.checkbox(&mut ui_state.selection_only, "Nur Auswahl lasern");
    if view.can_export && ui.button("Als Datei exportieren").clicked() {
        actions.push(UiAction::LaserExport);
    }
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Job-Parameter: Startmodus.
    ui.label(RichText::new("PARAMETER").small().weak());
    ui.add_space(4.0);
    egui::ComboBox::from_id_salt("startmode")
        .selected_text(match ui_state.start_mode {
            StartMode::Absolut => "Absolute Koordinaten",
            StartMode::AktuellePosition => "Aktuelle Position",
            StartMode::Benutzerursprung => "Benutzerursprung",
        })
        .width(ui.available_width() - 8.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut ui_state.start_mode,
                StartMode::Absolut,
                "Absolute Koordinaten",
            );
            ui.selectable_value(
                &mut ui_state.start_mode,
                StartMode::AktuellePosition,
                "Aktuelle Position",
            );
            ui.selectable_value(
                &mut ui_state.start_mode,
                StartMode::Benutzerursprung,
                "Benutzerursprung",
            );
        });
    ui.add_space(8.0);
    ui.label(RichText::new("JOB-NULLPUNKT").small().weak());
    ui.add_space(4.0);
    anchor_grid(ui, &mut ui_state.anchor);

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(8.0);

    // Jog-Kreuz.
    ui.label(RichText::new("KOPF").small().weak());
    ui.add_space(4.0);
    if let Some(jog) = jog_cross(ui, ui_state.jog_step, view.connected) {
        actions.push(jog);
    }
    ui.add_space(8.0);
    slider_row(ui, "Schritt", "mm", &mut ui_state.jog_step, 0.1, 100.0);
    slider_row(ui, "Speed", "mm/s", &mut ui_state.jog_speed, 1.0, 1000.0);

    actions
}

/// Zeichnet eine Ampel-Kachel; gibt `true` bei Klick zurück.
fn ampel_cell(ui: &mut egui::Ui, label: &str, tone: &Tone, w: f32, h: f32, enabled: bool) -> bool {
    let (fill, text) = tone_colors(tone);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), sense);
    let fill = if enabled {
        fill
    } else {
        fill.gamma_multiply(0.45)
    };
    let bg = if resp.hovered() {
        fill.gamma_multiply(1.15)
    } else {
        fill
    };
    ui.painter().rect_filled(rect, 8.0, bg);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0),
        text,
    );
    resp.clicked()
}

fn anchor_grid(ui: &mut egui::Ui, anchor: &mut usize) {
    let size = 40.0;
    let gap = 5.0;
    egui::Grid::new("anchor")
        .spacing(Vec2::splat(gap))
        .show(ui, |ui| {
            for i in 0..9 {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
                let on = *anchor == i;
                let bg = if on {
                    Color32::from_rgb(0x1e, 0x3a, 0x5f)
                } else {
                    Color32::from_black_alpha(64)
                };
                ui.painter().rect_filled(rect, 8.0, bg);
                let dot = if on {
                    Color32::from_rgb(0x3B, 0x82, 0xF6)
                } else {
                    Color32::from_gray(0x9a)
                };
                let r = if on { 5.0 } else { 3.5 };
                ui.painter().circle_filled(rect.center(), r, dot);
                if resp.clicked() {
                    *anchor = i;
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
}

/// Zeichnet das Jog-Kreuz. Gibt die ausgelöste Bewegung als `UiAction` zurück
/// (Jog um `step` mm bzw. Home).
fn jog_cross(ui: &mut egui::Ui, step: f64, enabled: bool) -> Option<UiAction> {
    let b = 46.0;
    let gap = 5.0;
    let total = 3.0 * b + 2.0 * gap;
    let mut result: Option<UiAction> = None;
    ui.horizontal(|ui| {
        // Zentrieren.
        let pad = (ui.available_width() - total) * 0.5;
        if pad > 0.0 {
            ui.add_space(pad);
        }
        let (rect, _) = ui.allocate_exact_size(Vec2::new(total, total), Sense::hover());
        let cell = |col: usize, row: usize| -> egui::Rect {
            let x = rect.left() + col as f32 * (b + gap);
            let y = rect.top() + row as f32 * (b + gap);
            egui::Rect::from_min_size(egui::pos2(x, y), Vec2::splat(b))
        };
        // Richtung als selbstgezeichnetes Dreieck/Symbol — schriftunabhängig
        // (egui-Default-Font hat die Unicode-Pfeile nicht).
        let mut btn = |ui: &mut egui::Ui, r: egui::Rect, dir: JogDir| {
            let sense = if enabled {
                Sense::click()
            } else {
                Sense::hover()
            };
            let resp = ui.allocate_rect(r, sense);
            let bg = if resp.hovered() {
                Color32::from_rgb(0x30, 0x36, 0x40)
            } else {
                Color32::from_rgb(0x24, 0x28, 0x30)
            };
            ui.painter().rect_filled(r, 8.0, bg);
            let c = r.center();
            let fg = Color32::from_gray(0xec);
            let s = 9.0;
            match dir {
                JogDir::Up => tri(
                    ui,
                    [
                        c + Vec2::new(0.0, -s),
                        c + Vec2::new(-s, s * 0.6),
                        c + Vec2::new(s, s * 0.6),
                    ],
                    fg,
                ),
                JogDir::Down => tri(
                    ui,
                    [
                        c + Vec2::new(0.0, s),
                        c + Vec2::new(-s, -s * 0.6),
                        c + Vec2::new(s, -s * 0.6),
                    ],
                    fg,
                ),
                JogDir::Left => tri(
                    ui,
                    [
                        c + Vec2::new(-s, 0.0),
                        c + Vec2::new(s * 0.6, -s),
                        c + Vec2::new(s * 0.6, s),
                    ],
                    fg,
                ),
                JogDir::Right => tri(
                    ui,
                    [
                        c + Vec2::new(s, 0.0),
                        c + Vec2::new(-s * 0.6, -s),
                        c + Vec2::new(-s * 0.6, s),
                    ],
                    fg,
                ),
                JogDir::Home => {
                    // Kleines Haus.
                    ui.painter().circle_filled(c, 4.0, fg);
                }
            }
            if resp.clicked() {
                result = Some(match dir {
                    JogDir::Up => UiAction::LaserJog(0.0, -step),
                    JogDir::Down => UiAction::LaserJog(0.0, step),
                    JogDir::Left => UiAction::LaserJog(-step, 0.0),
                    JogDir::Right => UiAction::LaserJog(step, 0.0),
                    JogDir::Home => UiAction::LaserHome,
                });
            }
        };
        btn(ui, cell(1, 0), JogDir::Up);
        btn(ui, cell(0, 1), JogDir::Left);
        btn(ui, cell(1, 1), JogDir::Home);
        btn(ui, cell(2, 1), JogDir::Right);
        btn(ui, cell(1, 2), JogDir::Down);
    });
    result
}

#[derive(Debug, Clone, Copy)]
enum JogDir {
    Up,
    Down,
    Left,
    Right,
    Home,
}

/// Ausgefülltes Dreieck aus drei Punkten (für die Jog-Pfeile).
fn tri(ui: &egui::Ui, pts: [egui::Pos2; 3], color: Color32) {
    ui.painter().add(egui::Shape::convex_polygon(
        pts.to_vec(),
        color,
        egui::Stroke::NONE,
    ));
}

fn slider_row(ui: &mut egui::Ui, label: &str, unit: &str, value: &mut f64, min: f64, max: f64) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(unit).small().weak());
            ui.add(egui::DragValue::new(value).range(min..=max).speed(0.5));
        });
    });
    ui.add(egui::Slider::new(value, min..=max).show_value(false));
    ui.add_space(4.0);
}
