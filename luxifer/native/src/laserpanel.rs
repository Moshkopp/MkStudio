//! Laser-Bedienpanel in egui, nach dem frischen Svelte-Design (ADR 0007 +
//! Redesign): Ampel-Grid (Start grün / Pause orange / Stopp rot / Ursprung blau /
//! Rahmen · Gummiband), Job-Parameter, Job-Nullpunkt-Anker (3×3), Jog-Kreuz +
//! Slider. Ohne echten Treiber-Anschluss im Umbau — Aktionen loggen vorerst.

use egui::{Color32, RichText, Sense, Vec2};
use luxifer_core::{JobAction, StartMode};

use crate::app::App;

/// Was das Panel in diesem Frame auslösen will (nach dem UI-Block ausgeführt,
/// um Borrow-Konflikte mit `app` zu vermeiden).
enum PanelAction {
    Run(JobAction),
    Export,
    Jog(f64, f64),
    Home,
    Select(String),
    NewProfile,
    EditProfile,
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

/// Zeichnet das Panel und führt am Ende die gewählte Aktion über `app` aus.
pub fn show(ui: &mut egui::Ui, app: &mut App) {
    let mut pending: Option<PanelAction> = None;

    // Kopf: „LASER" + aktuelles Profil.
    ui.label(RichText::new("LASER").small().weak());
    ui.add_space(4.0);
    let profiles: Vec<(String, String)> = app
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
    if profiles.is_empty() {
        if ui.button("+ Laser anlegen").clicked() {
            pending = Some(PanelAction::NewProfile);
        }
    } else {
        let active_label = profiles
            .iter()
            .find(|(id, _)| *id == active_id)
            .map(|(_, l)| l.clone())
            .unwrap_or_else(|| "—".into());
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("laser_sel")
                .selected_text(active_label)
                .width(ui.available_width() - 34.0)
                .show_ui(ui, |ui| {
                    for (id, label) in &profiles {
                        if ui.selectable_label(*id == active_id, label).clicked() {
                            pending = Some(PanelAction::Select(id.clone()));
                        }
                    }
                });
            if ui
                .button("Verwalten")
                .on_hover_text("Laser verwalten")
                .clicked()
            {
                pending = Some(PanelAction::EditProfile);
            }
        });
    }
    ui.add_space(10.0);

    // Ampel-Grid aus den ECHTEN Aktionen des aktiven Treibers, feste Slots.
    ui.label(RichText::new("JOB").small().weak());
    ui.add_space(4.0);
    let actions = app.laser_backend.actions();
    let has = |a: JobAction| {
        actions
            .iter()
            .any(|x| std::mem::discriminant(x) == std::mem::discriminant(&a))
    };
    // Slot-Reihenfolge; erster passender Treiber-Key füllt den Slot.
    let slots: [Option<JobAction>; 6] = [
        [JobAction::SendJob, JobAction::StreamGcode]
            .into_iter()
            .find(|a| has(*a)),
        Some(JobAction::Pause).filter(|a| has(*a)),
        Some(JobAction::Stop).filter(|a| has(*a)),
        Some(JobAction::GoOrigin).filter(|a| has(*a)),
        Some(JobAction::Frame).filter(|a| has(*a)),
        Some(JobAction::RubberFrame).filter(|a| has(*a)),
    ];
    let avail = ui.available_width();
    let gap = 6.0;
    let cell_w = (avail - 2.0 * gap) / 3.0;
    let cell_h = cell_w * 0.72;
    egui::Grid::new("ampel")
        .spacing(Vec2::splat(gap))
        .show(ui, |ui| {
            for (i, slot) in slots.iter().enumerate() {
                match slot {
                    Some(a) => {
                        let (label, tone) = action_meta(*a);
                        if ampel_cell(ui, label, &tone, cell_w, cell_h) {
                            pending = Some(PanelAction::Run(*a));
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
    ui.checkbox(&mut app.laser.selection_only, "Nur Auswahl lasern");
    if has(JobAction::ExportFile) && ui.button("Als Datei exportieren").clicked() {
        pending = Some(PanelAction::Export);
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Job-Parameter: Startmodus.
    ui.label(RichText::new("PARAMETER").small().weak());
    ui.add_space(4.0);
    egui::ComboBox::from_id_salt("startmode")
        .selected_text(match app.laser.start_mode {
            StartMode::Absolut => "Absolute Koordinaten",
            StartMode::AktuellePosition => "Aktuelle Position",
            StartMode::Benutzerursprung => "Benutzerursprung",
        })
        .width(ui.available_width() - 8.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut app.laser.start_mode,
                StartMode::Absolut,
                "Absolute Koordinaten",
            );
            ui.selectable_value(
                &mut app.laser.start_mode,
                StartMode::AktuellePosition,
                "Aktuelle Position",
            );
            ui.selectable_value(
                &mut app.laser.start_mode,
                StartMode::Benutzerursprung,
                "Benutzerursprung",
            );
        });
    ui.add_space(8.0);
    ui.label(RichText::new("JOB-NULLPUNKT").small().weak());
    ui.add_space(4.0);
    anchor_grid(ui, &mut app.laser.anchor);

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(8.0);

    // Jog-Kreuz.
    ui.label(RichText::new("KOPF").small().weak());
    ui.add_space(4.0);
    let step = app.laser.jog_step;
    if let Some(jog) = jog_cross(ui, step) {
        pending = Some(jog);
    }
    ui.add_space(8.0);
    slider_row(ui, "Schritt", "mm", &mut app.laser.jog_step, 0.1, 100.0);
    slider_row(ui, "Speed", "mm/s", &mut app.laser.jog_speed, 1.0, 1000.0);

    // Statuszeile: letzte Treiber-Rückmeldung.
    if !app.laser_msg.is_empty() {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(RichText::new(&app.laser_msg).small().weak());
    }

    // Gewählte Aktion nach dem UI-Block ausführen.
    match pending {
        Some(PanelAction::Run(a)) => app.laser_run(a),
        Some(PanelAction::Export) => app.laser_export(),
        Some(PanelAction::Jog(dx, dy)) => app.laser_jog(dx, dy),
        Some(PanelAction::Home) => app.laser_home(),
        Some(PanelAction::Select(id)) => app.laser_select(&id),
        Some(PanelAction::NewProfile) => app.open_laser_settings(false),
        Some(PanelAction::EditProfile) => app.open_laser_settings(true),
        None => {}
    }
}

/// Zeichnet eine Ampel-Kachel; gibt `true` bei Klick zurück.
fn ampel_cell(ui: &mut egui::Ui, label: &str, tone: &Tone, w: f32, h: f32) -> bool {
    let (fill, text) = tone_colors(tone);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
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

/// Zeichnet das Jog-Kreuz. Gibt die ausgelöste Bewegung als PanelAction zurück
/// (Jog um `step` mm bzw. Home).
fn jog_cross(ui: &mut egui::Ui, step: f64) -> Option<PanelAction> {
    let b = 46.0;
    let gap = 5.0;
    let total = 3.0 * b + 2.0 * gap;
    let mut result: Option<PanelAction> = None;
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
            let resp = ui.allocate_rect(r, Sense::click());
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
                    JogDir::Up => PanelAction::Jog(0.0, -step),
                    JogDir::Down => PanelAction::Jog(0.0, step),
                    JogDir::Left => PanelAction::Jog(-step, 0.0),
                    JogDir::Right => PanelAction::Jog(step, 0.0),
                    JogDir::Home => PanelAction::Home,
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
