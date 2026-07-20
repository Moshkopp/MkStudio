//! Laser-Bedienpanel in egui, nach dem frischen Svelte-Design (ADR 0007 +
//! Redesign): Ampel-Grid (Start grün / Pause orange / Stopp rot / Ursprung blau /
//! Rahmen · Gummiband), Job-Parameter, Job-Nullpunkt-Anker (3×3), Jog-Kreuz +
//! Slider. Die Kacheln lösen `UiAction::LaserRun` aus; der Root führt sie über
//! den `LaserService` aus (echte Treiber-Aktionen, hardwarelos getestet).

use egui::{Color32, RichText, Sense, Vec2};
use studio_core::{AxisDir, JobAction, MachineAxis, StartReference};

use crate::app::HoldJog;
use crate::tools::LaserUi;
use crate::ui::UiAction;

/// Ein gespeicherter Werkstück-Nullpunkt in der Panel-Sicht (ADR 0020).
pub struct SavedOriginRow {
    pub id: String,
    pub name: String,
    pub x_mm: f64,
    pub y_mm: f64,
    /// Innerhalb der aktuellen Bettgeometrie nutzbar? Ungültige Einträge
    /// bleiben sichtbar, sind aber gesperrt.
    pub usable: bool,
}

/// Reine Sicht auf den Laser-Zustand für das Panel (vom Root abgeleitet, damit
/// das Panel weder Backend noch `App` kennt). `slots` ist die feste 2×3-
/// Ampelbelegung; ein `None`-Slot bleibt leer. Die Positionsinformation zeigt
/// der Canvas über die Fadenkreuze — das Panel bleibt ruhig.
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
    pub lease_pending: bool,
    /// Gespeicherte Nullpunkte des aktiven Lasers (fürs „Starten von"-Dropdown;
    /// verwaltet werden sie in der Laser-Verwaltung).
    pub saved_origins: Vec<SavedOriginRow>,
    /// Verweist die gemerkte Auswahl auf eine gelöschte Nullpunkt-ID?
    pub reference_missing: bool,
    /// „Position speichern" möglich (verbunden + Positionslesen unterstützt)?
    pub can_save_origin: bool,
    /// Aus dem Controller gelesene Achsen-Verfügbarkeit (ADR 0021 §A). Gated die
    /// Z/U-Ecken des Jog-Kreuzes.
    pub has_z_axis: bool,
    pub has_u_axis: bool,
    /// Live-Achsenpositionen (mm) für die Anzeige; `None` = unbekannt/„—".
    pub pos: AxisPositions,
    /// Läuft gerade ein Achsen-Dauerlauf? Steuert das kontinuierliche Repaint,
    /// damit der Watchdog die Karenzzeit auslaufen lassen und stoppen kann.
    pub hold_active: bool,
}

/// Live-Positionen aller vier Achsen (mm), soweit gelesen.
#[derive(Default, Clone, Copy)]
pub struct AxisPositions {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
    pub u: Option<f64>,
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
                (Color32::from_rgb(0x34, 0xd3, 0x99), "⏺ Verbunden")
            } else {
                (ui.visuals().weak_text_color(), "⏺ Getrennt")
            };
            ui.colored_label(color, label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let clicked = if view.connected {
                    ui.button("Trennen").clicked()
                } else {
                    ui.add_enabled(
                        !view.lease_pending,
                        egui::Button::new(if view.lease_pending {
                            "Lease …"
                        } else {
                            "Verbinden"
                        }),
                    )
                    .clicked()
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

    // Job-Parameter: Startreferenz („Starten von", ADR 0020). Die bloße
    // Auswahl bewegt die Maschine nie; angefahren wird der Bezugspunkt nur
    // über die „Ursprung"-Kachel. Daneben: aktuelle Position als Nullpunkt
    // speichern (Icon). Verwaltet werden Nullpunkte in der Laser-Verwaltung.
    ui.label(RichText::new("PARAMETER").small().weak());
    ui.add_space(4.0);
    if view.connected {
        // Positions-Fadenkreuze im Canvas frisch halten (Lesen läuft
        // gedrosselt im Root-Poll; das Panel liest nie selbst).
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1000));
    }
    let reference_label = |reference: &StartReference| -> String {
        match reference {
            StartReference::Absolut => "Absolute Koordinaten".into(),
            StartReference::AktuellePosition => "Aktuelle Position".into(),
            StartReference::Benutzerursprung => "Benutzerursprung".into(),
            StartReference::GespeicherterNullpunkt { id } => view
                .saved_origins
                .iter()
                .find(|row| &row.id == id)
                .map(|row| row.name.clone())
                .unwrap_or_else(|| "⚠ Nullpunkt fehlt".into()),
        }
    };
    let selected_text = if view.reference_missing {
        "⚠ Nullpunkt fehlt — neu wählen".into()
    } else {
        reference_label(&ui_state.start_reference)
    };
    ui.horizontal(|ui| {
        // Rechts-nach-links wie beim Laser-Selector: erst das Speichern-Icon,
        // die Combo füllt exakt den Rest der Panelbreite.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(view.can_save_origin, egui::Button::new("+"))
                .on_hover_text("Aktuelle Kopfposition als Nullpunkt speichern")
                .clicked()
            {
                actions.push(UiAction::LaserSaveOriginHere);
            }
            egui::ComboBox::from_id_salt("startmode")
                .selected_text(selected_text)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for reference in [
                        StartReference::Absolut,
                        StartReference::AktuellePosition,
                        StartReference::Benutzerursprung,
                    ] {
                        if ui
                            .selectable_label(
                                ui_state.start_reference == reference,
                                reference_label(&reference),
                            )
                            .clicked()
                        {
                            actions.push(UiAction::LaserSelectStartReference(reference.clone()));
                        }
                    }
                    for row in &view.saved_origins {
                        let reference =
                            StartReference::GespeicherterNullpunkt { id: row.id.clone() };
                        let label = if row.usable {
                            row.name.clone()
                        } else {
                            format!("{} (ungültig)", row.name)
                        };
                        if ui
                            .selectable_label(ui_state.start_reference == reference, label)
                            .on_hover_text(format!("X {:.2} mm · Y {:.2} mm", row.x_mm, row.y_mm))
                            .clicked()
                        {
                            actions.push(UiAction::LaserSelectStartReference(reference));
                        }
                    }
                });
        });
    });
    if view.reference_missing {
        ui.colored_label(
            Color32::from_rgb(0xe0, 0x93, 0x00),
            "Die gemerkte Startreferenz existiert nicht mehr. Bitte neu wählen.",
        );
    }
    ui.add_space(8.0);
    ui.label(RichText::new("JOB-NULLPUNKT").small().weak());
    ui.add_space(4.0);
    anchor_grid(ui, &mut ui_state.anchor);

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(8.0);

    // Jog-Kreuz: Mitte X/Y + Home, Ecken oben U (dreh −/+), Ecken unten Z (−/+).
    // Das eine Panel für alle fünf Bewegungen. Der Modus (Schritt/Dauer) rechts
    // neben der Überschrift bestimmt, ob ein Klick einen festen Schritt fährt
    // oder Gedrückthalten einen Dauerlauf auslöst.
    ui.horizontal(|ui| {
        ui.label(RichText::new("KOPF").small().weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.selectable_value(&mut ui_state.continuous_jog, false, "Schritt");
            ui.selectable_value(&mut ui_state.continuous_jog, true, "Dauer");
        });
    });
    ui.add_space(4.0);

    let mut hold: Option<HoldJog> = None;
    jog_cross(
        ui,
        view,
        ui_state.jog_step,
        ui_state.continuous_jog,
        &mut actions,
        &mut hold,
    );
    if ui_state.continuous_jog {
        // Kontinuierlich neu zeichnen, solange etwas zu tun ist: eine Zelle
        // gehalten wird ODER noch ein Dauerlauf aktiv ist (Watchdog-Karenz).
        if hold.is_some() || view.hold_active {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        actions.push(UiAction::LaserHoldFrame(hold));
    }

    ui.add_space(8.0);
    slider_row(ui, "Schritt", "mm", &mut ui_state.jog_step, 0.1, 100.0);
    slider_row(ui, "Speed", "mm/s", &mut ui_state.jog_speed, 1.0, 1000.0);
    // Eigener Z-Speed, hart auf Z_JOG_SPEED_MAX begrenzt (Gewindestange).
    slider_row(
        ui,
        "Z-Speed",
        "mm/s",
        &mut ui_state.z_jog_speed,
        1.0,
        crate::tools::Z_JOG_SPEED_MAX,
    );

    // Live-Positionsanzeige aller vier Achsen (rein informativ).
    ui.add_space(8.0);
    ui.separator();
    ui.label(RichText::new("POSITION").small().weak());
    ui.add_space(2.0);
    egui::Grid::new("axis_position")
        .num_columns(2)
        .spacing([10.0, 3.0])
        .show(ui, |ui| {
            for (label, value) in [
                ("X", view.pos.x),
                ("Y", view.pos.y),
                ("Z", view.pos.z),
                ("U", view.pos.u),
            ] {
                ui.label(RichText::new(label).weak());
                let text = match value {
                    Some(mm) => format!("{mm:.3} mm"),
                    None => "—".to_owned(),
                };
                ui.label(RichText::new(text).monospace());
                ui.end_row();
            }
        });

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

/// Was eine Zelle des Jog-Kreuzes auslöst.
#[derive(Clone, Copy)]
enum JogCell {
    /// X/Y-Kopfbewegung (dx, dy als Vielfache von `jog_step`).
    Head(f64, f64),
    Home,
    /// Zusatzachse Z/U in eine Richtung (Schritt: Tippen; Dauer: Halten).
    Axis(MachineAxis, AxisDir),
}

/// Symbol einer Jog-Zelle.
#[derive(Clone, Copy)]
enum JogGlyph {
    Up,
    Down,
    Left,
    Right,
    Home,
    Label(&'static str),
}

/// Zeichnet das Jog-Kreuz: Mitte X/Y + Home, Ecken oben U (dreh), unten Z. Im
/// Schritt-Modus tippt ein Klick einen festen Schritt (`actions`); im
/// Dauer-Modus setzt eine gedrückt gehaltene Zelle `hold`. Z/U-Ecken sind nur
/// aktiv, wenn die Achse laut Controller vorhanden ist (`has_z/has_u`).
fn jog_cross(
    ui: &mut egui::Ui,
    view: &LaserView,
    step: f64,
    continuous: bool,
    actions: &mut Vec<UiAction>,
    hold: &mut Option<HoldJog>,
) {
    let b = 46.0;
    let gap = 5.0;
    let total = 3.0 * b + 2.0 * gap;
    ui.horizontal(|ui| {
        let pad = (ui.available_width() - total) * 0.5;
        if pad > 0.0 {
            ui.add_space(pad);
        }
        let (rect, _) = ui.allocate_exact_size(Vec2::new(total, total), Sense::hover());
        let cell_rect = |col: usize, row: usize| -> egui::Rect {
            let x = rect.left() + col as f32 * (b + gap);
            let y = rect.top() + row as f32 * (b + gap);
            egui::Rect::from_min_size(egui::pos2(x, y), Vec2::splat(b))
        };

        // Ecken oben = U (dreh −/+), Ecken unten = Z (−/+), Mitte = X/Y + Home.
        let cells: [(usize, usize, JogGlyph, JogCell); 9] = [
            (
                0,
                0,
                JogGlyph::Label("U−"),
                JogCell::Axis(MachineAxis::U, AxisDir::Backward),
            ),
            (1, 0, JogGlyph::Up, JogCell::Head(0.0, -1.0)),
            (
                2,
                0,
                JogGlyph::Label("U+"),
                JogCell::Axis(MachineAxis::U, AxisDir::Forward),
            ),
            (0, 1, JogGlyph::Left, JogCell::Head(-1.0, 0.0)),
            (1, 1, JogGlyph::Home, JogCell::Home),
            (2, 1, JogGlyph::Right, JogCell::Head(1.0, 0.0)),
            (
                0,
                2,
                JogGlyph::Label("Z−"),
                JogCell::Axis(MachineAxis::Z, AxisDir::Backward),
            ),
            (1, 2, JogGlyph::Down, JogCell::Head(0.0, 1.0)),
            (
                2,
                2,
                JogGlyph::Label("Z+"),
                JogCell::Axis(MachineAxis::Z, AxisDir::Forward),
            ),
        ];

        for (col, row, glyph, action) in cells {
            // Zelle nur aktiv, wenn verbunden UND (X/Y/Home immer, Z/U nur bei
            // vorhandener Achse).
            let axis_ok = match action {
                JogCell::Axis(MachineAxis::Z, _) => view.has_z_axis,
                JogCell::Axis(MachineAxis::U, _) => view.has_u_axis,
                _ => true,
            };
            let enabled = view.connected && axis_ok;

            let r = cell_rect(col, row);
            let sense = if !enabled {
                Sense::hover()
            } else if continuous {
                Sense::drag()
            } else {
                Sense::click()
            };
            let resp = ui.allocate_rect(r, sense);
            let held = continuous && enabled && resp.is_pointer_button_down_on();
            let bg = if held {
                ui.visuals().selection.stroke.color.gamma_multiply(0.6)
            } else if !enabled {
                Color32::from_rgb(0x1c, 0x1f, 0x25)
            } else if resp.hovered() {
                Color32::from_rgb(0x30, 0x36, 0x40)
            } else {
                Color32::from_rgb(0x24, 0x28, 0x30)
            };
            ui.painter().rect_filled(r, 8.0, bg);
            let fg = if enabled {
                Color32::from_gray(0xec)
            } else {
                Color32::from_gray(0x55)
            };
            draw_glyph(ui, r, glyph, fg);

            if !enabled {
                continue;
            }
            if continuous {
                if held {
                    match action {
                        JogCell::Axis(axis, dir) => *hold = Some(HoldJog { axis, dir }),
                        JogCell::Head(dx, dy) => *hold = head_hold(dx, dy),
                        JogCell::Home => {}
                    }
                }
            } else if resp.clicked() {
                match action {
                    JogCell::Head(dx, dy) => {
                        // X/Y-Schritt geht über das bestehende `LaserJog(dx,dy)`
                        // (Ebene, absolut+delta), skaliert mit dem Schritt.
                        actions.push(UiAction::LaserJog(dx * step, dy * step))
                    }
                    JogCell::Home => actions.push(UiAction::LaserHome),
                    JogCell::Axis(axis, dir) => actions.push(UiAction::LaserJogAxis(axis, dir)),
                }
            }
        }
    });
}

/// Übersetzt eine X/Y-Kreuzrichtung in einen Dauerlauf-Halte-Wunsch. Die
/// Vorzeichen folgen dem Kreuz: links/rechts = X∓, hoch/runter = Y (Y-Achse
/// zählt am Ruida von oben, daher hoch = Forward).
fn head_hold(dx: f64, dy: f64) -> Option<HoldJog> {
    if dx < 0.0 {
        Some(HoldJog {
            axis: MachineAxis::X,
            dir: AxisDir::Backward,
        })
    } else if dx > 0.0 {
        Some(HoldJog {
            axis: MachineAxis::X,
            dir: AxisDir::Forward,
        })
    } else if dy < 0.0 {
        Some(HoldJog {
            axis: MachineAxis::Y,
            dir: AxisDir::Forward,
        })
    } else if dy > 0.0 {
        Some(HoldJog {
            axis: MachineAxis::Y,
            dir: AxisDir::Backward,
        })
    } else {
        None
    }
}

/// Zeichnet das Symbol einer Jog-Zelle (Pfeil-Dreieck, Home-Punkt oder Label).
fn draw_glyph(ui: &egui::Ui, r: egui::Rect, glyph: JogGlyph, fg: Color32) {
    let c = r.center();
    let s = 9.0;
    match glyph {
        JogGlyph::Up => tri(
            ui,
            [
                c + Vec2::new(0.0, -s),
                c + Vec2::new(-s, s * 0.6),
                c + Vec2::new(s, s * 0.6),
            ],
            fg,
        ),
        JogGlyph::Down => tri(
            ui,
            [
                c + Vec2::new(0.0, s),
                c + Vec2::new(-s, -s * 0.6),
                c + Vec2::new(s, -s * 0.6),
            ],
            fg,
        ),
        JogGlyph::Left => tri(
            ui,
            [
                c + Vec2::new(-s, 0.0),
                c + Vec2::new(s * 0.6, -s),
                c + Vec2::new(s * 0.6, s),
            ],
            fg,
        ),
        JogGlyph::Right => tri(
            ui,
            [
                c + Vec2::new(s, 0.0),
                c + Vec2::new(-s * 0.6, -s),
                c + Vec2::new(-s * 0.6, s),
            ],
            fg,
        ),
        JogGlyph::Home => {
            ui.painter().circle_filled(c, 4.0, fg);
        }
        JogGlyph::Label(text) => {
            ui.painter().text(
                c,
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(15.0),
                fg,
            );
        }
    }
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
    // Slider über die volle Panelbreite (egui-Default sind starre 100 px).
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = ui.available_width();
        ui.add(egui::Slider::new(value, min..=max).show_value(false));
    });
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view_with_origin() -> LaserView {
        LaserView {
            profiles: vec![("laser-1".into(), "Test · Ruida".into())],
            active_id: "laser-1".into(),
            slots: [None; 6],
            can_export: false,
            connected: false,
            lease_pending: false,
            saved_origins: vec![SavedOriginRow {
                id: "origin-1".into(),
                name: "Untersetzer".into(),
                x_mm: 326.03,
                y_mm: 320.06,
                usable: true,
            }],
            reference_missing: false,
            can_save_origin: false,
            has_z_axis: false,
            has_u_axis: false,
            pos: AxisPositions::default(),
            hold_active: false,
        }
    }

    /// Alle Textinhalte eines Frames (rekursiv über verschachtelte Shapes).
    fn frame_texts(shapes: &[egui::epaint::ClippedShape]) -> String {
        fn collect(shape: &egui::epaint::Shape, out: &mut String) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    out.push_str(&text.galley.job.text);
                    out.push('\n');
                }
                egui::epaint::Shape::Vec(children) => {
                    for child in children {
                        collect(child, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = String::new();
        for clipped in shapes {
            collect(&clipped.shape, &mut out);
        }
        out
    }

    /// y-Zentrum eines Textes im Frame (Panik, wenn er fehlt).
    fn text_y_center(shapes: &[egui::epaint::ClippedShape], needle: &str) -> f32 {
        fn find(shape: &egui::epaint::Shape, needle: &str) -> Option<f32> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.job.text == needle => {
                    Some(egui::Rect::from_min_size(t.pos, t.galley.size()).center().y)
                }
                egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| find(s, needle)),
                _ => None,
            }
        }
        shapes
            .iter()
            .find_map(|c| find(&c.shape, needle))
            .unwrap_or_else(|| panic!("Text {needle:?} nicht im Frame"))
    }

    /// Regressionstest zum egui-0.35-Befund „Zeileninhalte um 4 px versetzt":
    /// `horizontal()` nimmt `interact_size.y` als Zeilenhöhe an; ist die
    /// kleiner als die Button-Höhe, sitzen Button-, Combo- und Label-Texte
    /// einer Zeile nicht mehr auf einer Linie. Das Theme muss die Zeilenhöhe
    /// deshalb an die tatsächliche Button-Höhe koppeln.
    #[test]
    fn zeileninhalte_sitzen_auf_einer_linie() {
        let ctx = egui::Context::default();
        let style = crate::ui::theme_style(&studio_core::Theme::default());
        ctx.all_styles_mut(|s| *s = style.clone());
        let mut ui_state = crate::tools::LaserUi::default();
        let view = view_with_origin();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 1300.0),
            )),
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show(ui, &view, &mut ui_state);
            });
        });
        for (left, right) in [
            ("Test · Ruida", "Verwalten"),
            ("⏺ Getrennt", "Verbinden"),
            ("Absolute Koordinaten", "+"),
            // „Speed" ist eindeutig (das „Schritt"-Label kommt jetzt auch im
            // Modus-Umschalter vor); prüft dieselbe Slider-Zeilen-Ausrichtung.
            ("Speed", "100.0"),
        ] {
            let dy = (text_y_center(&out.shapes, left) - text_y_center(&out.shapes, right)).abs();
            assert!(
                dy < 0.5,
                "{left:?} und {right:?} sind vertikal um {dy:.1} px versetzt"
            );
        }
    }

    /// Headless-Absicherung des Nutzerbefunds „gespeicherter Nullpunkt fehlt
    /// im Starten-von-Dropdown": Panel rendern, Combo anklicken und prüfen,
    /// dass der Nullpunkt im geöffneten Popup als Eintrag auftaucht.
    #[test]
    fn starten_von_dropdown_listet_gespeicherte_nullpunkte() {
        let ctx = egui::Context::default();
        let mut ui_state = crate::tools::LaserUi::default();
        let view = view_with_origin();
        let mut run = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(420.0, 900.0),
                )),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(ui, &view, &mut ui_state);
                });
            })
        };
        // Frame 1: Layout aufbauen, Position der Combo aus dem Text der
        // Auswahl ("Absolute Koordinaten" = Default-Referenz) bestimmen.
        let first = run(Vec::new());
        let texts = frame_texts(&first.shapes);
        assert!(
            texts.contains("Absolute Koordinaten"),
            "Combo-Beschriftung fehlt im Panel:\n{texts}"
        );
        let combo_pos = first
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::epaint::Shape::Text(text)
                    if text.galley.job.text == "Absolute Koordinaten" =>
                {
                    Some(text.pos + egui::vec2(4.0, 4.0))
                }
                _ => None,
            })
            .expect("Combo-Text nicht gefunden");
        // Frame 2+3: Klick auf die Combo (down/up) öffnet das Popup.
        let click = vec![
            egui::Event::PointerMoved(combo_pos),
            egui::Event::PointerButton {
                pos: combo_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos: combo_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ];
        run(click);
        let open = run(Vec::new());
        let texts = frame_texts(&open.shapes);
        assert!(
            texts.contains("Benutzerursprung"),
            "Popup hat sich nicht geöffnet:\n{texts}"
        );
        assert!(
            texts.contains("Untersetzer"),
            "Gespeicherter Nullpunkt fehlt im Dropdown:\n{texts}"
        );
    }

    /// Wie in der echten App: Zoom 1.15, Panel im ScrollArea-Seitenpanel, und
    /// der Nullpunkt wird erst NACH einem ersten Öffnen des Dropdowns
    /// gespeichert. Auch dann muss er beim erneuten Öffnen erscheinen
    /// (kein veralteter Popup-/ScrollArea-Cache).
    #[test]
    fn dropdown_zeigt_nachtraeglich_gespeicherten_nullpunkt() {
        let ctx = egui::Context::default();
        ctx.set_zoom_factor(1.15);
        let mut ui_state = crate::tools::LaserUi::default();
        let mut view = view_with_origin();
        view.saved_origins.clear();
        let run = |view: &LaserView, ui_state: &mut crate::tools::LaserUi, events| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 1000.0),
                )),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::Panel::right("inspector")
                        .default_size(340.0)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| show(ui, view, ui_state));
                        });
                });
            })
        };
        let combo_pos = |shapes: &[egui::epaint::ClippedShape]| {
            shapes.iter().find_map(|clipped| match &clipped.shape {
                egui::epaint::Shape::Text(text)
                    if text.galley.job.text == "Absolute Koordinaten" =>
                {
                    Some(text.pos + egui::vec2(4.0, 4.0))
                }
                _ => None,
            })
        };
        let click_at = |pos: egui::Pos2| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ]
        };
        // Ohne Nullpunkte öffnen (füllt Popup-/ScrollArea-Caches) …
        let first = run(&view, &mut ui_state, Vec::new());
        let pos = combo_pos(&first.shapes).expect("Combo nicht gefunden");
        run(&view, &mut ui_state, click_at(pos));
        run(&view, &mut ui_state, Vec::new());
        // … wieder schließen (Escape) und Nullpunkt „speichern".
        run(
            &view,
            &mut ui_state,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        view.saved_origins = view_with_origin().saved_origins;
        let refreshed = run(&view, &mut ui_state, Vec::new());
        // Erneut öffnen (Position frisch messen — der Zoomfaktor greift erst
        // ab dem zweiten Frame): der neue Eintrag muss gelistet sein.
        let pos = combo_pos(&refreshed.shapes).unwrap_or(pos);
        run(&view, &mut ui_state, click_at(pos));
        let open = run(&view, &mut ui_state, Vec::new());
        let texts = frame_texts(&open.shapes);
        assert!(
            texts.contains("Benutzerursprung"),
            "Popup hat sich nicht geöffnet:\n{texts}"
        );
        assert!(
            texts.contains("Untersetzer"),
            "Nachträglich gespeicherter Nullpunkt fehlt im Dropdown:\n{texts}"
        );
    }
}
