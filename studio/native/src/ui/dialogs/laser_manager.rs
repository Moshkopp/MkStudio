//! Eigenständige Verwaltung der Laserprofile, Kalibrierung und der
//! treiberspezifischen Controllerdaten.

use std::collections::BTreeSet;

use egui::RichText;
use studio_application::{MachineSetting, MachineSettingUnit};
use studio_core::{BedOrigin, Connection, DriverKind, LaserRegistry, ScanOffsetPoint};

use crate::ui::{LaserManagerState, LaserManagerTab};

#[derive(Debug, Clone, PartialEq, Default)]
pub(in crate::ui) enum LaserManagerOutcome {
    #[default]
    None,
    Close,
    Select(String),
    New,
    Save,
    Delete,
    MachineRead,
    MachineWrite,
    /// Schrittweite einer Achse aus Soll/Ist kalibrieren (ADR 0022 §B).
    CalibrateAxis(studio_core::MachineAxis, f64, f64),
}

pub(in crate::ui) fn laser_manager_window(
    root_ui: &mut egui::Ui,
    state: &mut LaserManagerState,
    registry: &LaserRegistry,
) -> LaserManagerOutcome {
    let mut outcome = LaserManagerOutcome::None;
    let mut open = true;
    let window_size = root_ui.max_rect().size();
    let dialog_size = egui::vec2(window_size.x * 0.5, window_size.y * 0.9);

    egui::Window::new("Laser verwalten")
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .fixed_size(dialog_size)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(root_ui, |ui| {
            let body_height = (ui.available_height() - 46.0).max(380.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), body_height),
                egui::Layout::left_to_right(egui::Align::TOP),
                |ui| {
                    profile_list(ui, state, registry, &mut outcome, body_height);
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.set_min_width(520.0);
                        ui.set_height(body_height);
                        detail(ui, state, &mut outcome);
                    });
                },
            );
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Schließen").clicked() {
                    outcome = LaserManagerOutcome::Close;
                }
                if ui
                    .add(egui::Button::new(
                        RichText::new("Profil speichern").strong(),
                    ))
                    .clicked()
                {
                    outcome = LaserManagerOutcome::Save;
                }
                if !state.is_new && ui.button("Profil löschen").clicked() {
                    outcome = LaserManagerOutcome::Delete;
                }
            });
        });
    if !open || root_ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        LaserManagerOutcome::Close
    } else {
        outcome
    }
}

fn profile_list(
    ui: &mut egui::Ui,
    state: &LaserManagerState,
    registry: &LaserRegistry,
    outcome: &mut LaserManagerOutcome,
    height: f32,
) {
    ui.vertical(|ui| {
        ui.set_width(210.0);
        ui.set_height(height);
        ui.heading("Laserprofile");
        if ui
            .add_sized(
                [ui.available_width(), 30.0],
                egui::Button::new("+ Laser hinzufügen"),
            )
            .clicked()
        {
            *outcome = LaserManagerOutcome::New;
        }
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt("laser_profile_list")
            .max_height(ui.available_height())
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for profile in &registry.profiles {
                    let selected = state.selected_id.as_deref() == Some(profile.id.as_str());
                    let label = format!("{}\n{:?}", profile.name, profile.kind);
                    if ui
                        .add_sized(
                            [ui.available_width(), 44.0],
                            egui::Button::selectable(selected, label),
                        )
                        .clicked()
                    {
                        *outcome = LaserManagerOutcome::Select(profile.id.clone());
                    }
                }
            });
    });
}

fn detail(ui: &mut egui::Ui, state: &mut LaserManagerState, outcome: &mut LaserManagerOutcome) {
    ui.heading(if state.is_new {
        "Neues Laserprofil"
    } else {
        state.draft.name.as_str()
    });
    ui.horizontal(|ui| {
        tab(ui, state, LaserManagerTab::Grunddaten, "Grunddaten", true);
        tab(
            ui,
            state,
            LaserManagerTab::ScanOffset,
            "Scan-Offset",
            !state.is_new,
        );
        tab(
            ui,
            state,
            LaserManagerTab::Achskalibrierung,
            "Achskalibrierung",
            !state.is_new,
        );
        tab(
            ui,
            state,
            LaserManagerTab::Controller,
            "Controller",
            !state.is_new && state.draft.kind == DriverKind::Ruida,
        );
        tab(
            ui,
            state,
            LaserManagerTab::Nullpunkte,
            "Nullpunkte",
            !state.is_new,
        );
    });
    ui.separator();
    let content_height = ui.available_height();
    egui::ScrollArea::vertical()
        .id_salt("laser_manager_detail")
        .max_height(content_height)
        .auto_shrink([false, false])
        .show(ui, |ui| match state.tab {
            LaserManagerTab::Grunddaten => basic_data(ui, state),
            LaserManagerTab::ScanOffset => calibration(ui, state),
            LaserManagerTab::Achskalibrierung => axis_calibration(ui, state, outcome),
            LaserManagerTab::Controller => controller(ui, state, outcome),
            LaserManagerTab::Nullpunkte => saved_origins(ui, &mut state.draft),
        });
}

fn tab(
    ui: &mut egui::Ui,
    state: &mut LaserManagerState,
    tab: LaserManagerTab,
    label: &str,
    enabled: bool,
) {
    if ui
        .add_enabled(enabled, egui::Button::selectable(state.tab == tab, label))
        .clicked()
    {
        state.tab = tab;
    }
}

fn basic_data(ui: &mut egui::Ui, state: &mut LaserManagerState) {
    let profile = &mut state.draft;
    egui::Grid::new("laser_basic_data")
        .num_columns(2)
        .spacing([16.0, 12.0])
        .show(ui, |ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut profile.name).desired_width(280.0));
            ui.end_row();

            ui.label("Treiber");
            let old_kind = profile.kind;
            egui::ComboBox::from_id_salt("laser_driver")
                .selected_text(format!("{:?}", profile.kind))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut profile.kind, DriverKind::Ruida, "Ruida");
                    ui.selectable_value(&mut profile.kind, DriverKind::Grbl, "GRBL");
                    ui.selectable_value(&mut profile.kind, DriverKind::MiniGrbl, "Mini-GRBL");
                });
            if profile.kind != old_kind {
                profile.connection = match profile.kind {
                    DriverKind::Ruida => Connection::default(),
                    DriverKind::Grbl | DriverKind::MiniGrbl => Connection::Seriell {
                        port: "/dev/ttyUSB0".into(),
                        baud: 115_200,
                    },
                };
            }
            ui.end_row();

            match &mut profile.connection {
                Connection::Netz { ip, port } => {
                    ui.label("IP-Adresse");
                    ui.add(egui::TextEdit::singleline(ip).desired_width(220.0));
                    ui.end_row();
                    ui.label("Port");
                    let mut value = port.unwrap_or(50200);
                    if ui
                        .add(egui::DragValue::new(&mut value).range(1..=u16::MAX))
                        .changed()
                    {
                        *port = Some(value);
                    }
                    ui.end_row();
                }
                Connection::Seriell { port, baud } => {
                    ui.label("Schnittstelle");
                    ui.add(egui::TextEdit::singleline(port).desired_width(220.0));
                    ui.end_row();
                    ui.label("Baudrate");
                    ui.add(egui::DragValue::new(baud).range(1_200..=2_000_000));
                    ui.end_row();
                }
            }

            ui.label("Arbeitsbereich");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut profile.bed_mm.0)
                        .speed(1.0)
                        .suffix(" mm"),
                );
                ui.label("×");
                ui.add(
                    egui::DragValue::new(&mut profile.bed_mm.1)
                        .speed(1.0)
                        .suffix(" mm"),
                );
            });
            ui.end_row();

            ui.label("Maschinen-Nullpunkt");
            egui::ComboBox::from_id_salt("laser_origin")
                .selected_text(origin_label(profile.origin))
                .show_ui(ui, |ui| {
                    for (origin, label) in [
                        (BedOrigin::TopLeft, "Oben links"),
                        (BedOrigin::TopRight, "Oben rechts"),
                        (BedOrigin::BottomLeft, "Unten links"),
                        (BedOrigin::BottomRight, "Unten rechts"),
                    ] {
                        ui.selectable_value(&mut profile.origin, origin, label);
                    }
                });
            ui.end_row();

            // Zusatzachsen (ADR 0021 §A): ob Z/U vorhanden sind, weiß der
            // Controller nicht — es ist eine Profil-Einstellung. Steuert, ob die
            // Z/U-Bedienelemente im Laserpanel freigegeben werden.
            let axes = &mut profile.axes;
            ui.label("Zusatzachsen");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut axes.has_z_axis, "Z-Achse (Fokus)");
                    ui.add_enabled(
                        axes.has_z_axis,
                        egui::Checkbox::new(&mut axes.invert_z, "Z umkehren"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut axes.has_u_axis, "U-Achse (Rotary)");
                    ui.add_enabled(
                        axes.has_u_axis,
                        egui::Checkbox::new(&mut axes.invert_u, "U umkehren"),
                    );
                });
            });
            ui.end_row();
        });
}

/// Tab „Nullpunkte": Werkstück-Nullpunkte dieses Lasers (ADR 0020) im
/// Profilentwurf umbenennen und löschen; „Speichern" übernimmt sie validiert
/// (IDs bleiben stabil). Angelegt werden neue Nullpunkte im Laserpanel aus
/// der echten Kopfposition.
fn saved_origins(ui: &mut egui::Ui, profile: &mut studio_core::LaserProfile) {
    if profile.saved_origins.is_empty() {
        ui.label(
            egui::RichText::new(
                "Noch keine Nullpunkte gespeichert. Im Laserpanel neben „Starten von“ \
                 die aktuelle Kopfposition speichern.",
            )
            .weak(),
        );
        return;
    }
    let bed = profile.bed_mm;
    let origin_usable = |origin: &studio_core::SavedOrigin| {
        origin.x_mm >= 0.0 && origin.y_mm >= 0.0 && origin.x_mm <= bed.0 && origin.y_mm <= bed.1
    };
    let mut delete: Option<usize> = None;
    egui::Grid::new("laser_saved_origins")
        .num_columns(4)
        .spacing([16.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Name");
            ui.strong("Position");
            ui.strong("");
            ui.strong("");
            ui.end_row();
            for (index, origin) in profile.saved_origins.iter_mut().enumerate() {
                ui.add(egui::TextEdit::singleline(&mut origin.name).desired_width(220.0));
                ui.label(
                    egui::RichText::new(format!("X {:.2}  Y {:.2} mm", origin.x_mm, origin.y_mm))
                        .weak(),
                );
                if origin_usable(origin) {
                    ui.label("");
                } else {
                    ui.colored_label(egui::Color32::from_rgb(0xd2, 0x46, 0x3c), "ungültig")
                        .on_hover_text(
                            "Liegt außerhalb des Arbeitsbereichs — neu speichern oder entfernen.",
                        );
                }
                if ui.button("🗑").on_hover_text("Nullpunkt löschen").clicked() {
                    delete = Some(index);
                }
                ui.end_row();
            }
        });
    if let Some(index) = delete {
        profile.saved_origins.remove(index);
    }
    ui.add_space(8.0);
    ui.weak("Umbenennen wirkt nach „Speichern“; die stabile ID bleibt erhalten.");
}

fn origin_label(origin: BedOrigin) -> &'static str {
    match origin {
        BedOrigin::TopLeft => "Oben links",
        BedOrigin::TopRight => "Oben rechts",
        BedOrigin::BottomLeft => "Unten links",
        BedOrigin::BottomRight => "Unten rechts",
    }
}

fn calibration(ui: &mut egui::Ui, state: &mut LaserManagerState) {
    let calibration = &mut state.draft.scan_offset;
    ui.checkbox(&mut calibration.enabled, "Scan-Offset-Korrektur aktiv");
    ui.weak("Dezimalwerte können mit Komma oder Punkt eingegeben werden.");
    ui.add_space(8.0);
    let mut remove = None;
    egui::Grid::new("scan_offset_points")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Geschwindigkeit (mm/s)");
            ui.strong("Offset (mm)");
            ui.end_row();
            for (index, point) in calibration.points.iter_mut().enumerate() {
                locale_number(ui, ("scan_speed", index), &mut point.speed_mm_s);
                locale_number(ui, ("scan_offset", index), &mut point.offset_mm);
                if ui.small_button("Entfernen").clicked() {
                    remove = Some(index);
                }
                ui.end_row();
            }
        });
    if let Some(index) = remove {
        calibration.points.remove(index);
    }
    if ui.button("+ Messpunkt").clicked() {
        calibration.points.push(ScanOffsetPoint {
            speed_mm_s: 100.0,
            offset_mm: 0.0,
        });
    }
}

/// Achs-Schrittweiten-Kalibrierung (ADR 0022 §D): pro Achse Soll und Ist
/// eingeben, „Kalibrieren" liest den aktuellen Wert, rechnet im Core und
/// schreibt zurück. Nur Achsen, die es an dieser Maschine gibt.
fn axis_calibration(
    ui: &mut egui::Ui,
    state: &mut LaserManagerState,
    outcome: &mut LaserManagerOutcome,
) {
    use studio_core::MachineAxis;

    ui.label("Strecke fahren oder schneiden, nachmessen und Soll/Ist eintragen.");
    ui.weak(
        "Beispiel: Soll 10 mm, tatsächlich 25 mm — die Schrittlänge steigt dann auf das 2,5-Fache.",
    );
    ui.add_space(8.0);

    // Ohne gelesene Werte gibt es weder einen Ist-Stand noch eine Vorschau —
    // dann wird nur zum Auslesen aufgefordert, statt blind schreiben zu lassen.
    let reading = state.machine_read_rx.is_some();
    if state.machine_settings.is_empty() || reading {
        ui.horizontal(|ui| {
            if reading {
                ui.spinner();
                ui.weak("liest Register…");
            } else {
                if ui.button("Maschine auslesen").clicked() {
                    *outcome = LaserManagerOutcome::MachineRead;
                }
                ui.weak("nötig, um die aktuelle Schrittlänge zu zeigen");
            }
        });
        ui.add_space(8.0);
    }

    let axes = [
        (MachineAxis::X, "X", "x_step_length", true),
        (MachineAxis::Y, "Y", "y_step_length", true),
        (
            MachineAxis::Z,
            "Z",
            "z_step_length",
            state.draft.axes.has_z_axis,
        ),
        (
            MachineAxis::U,
            "U",
            "u_step_length",
            state.draft.axes.has_u_axis,
        ),
    ];
    let current_of = |key: &str| -> Option<f64> {
        state
            .machine_settings
            .iter()
            .find(|setting| setting.key == key)
            .and_then(|setting| setting.value())
    };

    // Welche Achse ihre Eingaben verliert, entscheidet sich hier; geleert wird
    // erst im Feld selbst, weil die Puffer-ID am dortigen Ui hängt.
    let clear_for = state.axis_cal_clear_inputs.take();

    let mut requested = None;
    egui::Grid::new("axis_calibration")
        .num_columns(6)
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Achse");
            ui.strong("Aktuell (µm)");
            ui.strong("Soll (mm)");
            ui.strong("Ist (mm)");
            ui.strong("Neu (µm)");
            ui.end_row();
            for (axis, label, key, available) in axes {
                // Eine Achse, die es nicht gibt, wird gar nicht erst angeboten.
                if !available {
                    continue;
                }
                let current = current_of(key);
                let input = state.axis_cal.entry(axis).or_default();
                ui.label(label);
                match current {
                    Some(value) => ui.label(format!("{value:.4}")),
                    None => ui.weak("—"),
                };
                let clear = clear_for == Some(axis);
                locale_number_reset(ui, ("axis_cal_target", label), &mut input.target_mm, clear);
                locale_number_reset(
                    ui,
                    ("axis_cal_measured", label),
                    &mut input.measured_mm,
                    clear,
                );
                let (target, measured) = (input.target_mm, input.measured_mm);

                // Vorschau vor dem Schreiben: der Nutzer sieht den neuen Wert,
                // bevor er die Maschine verändert.
                let preview = current.and_then(|value| {
                    studio_core::calibrated_step_length(value, target, measured).ok()
                });
                match preview {
                    Some(value) => ui.strong(format!("{value:.4}")),
                    None => ui.weak("—"),
                };
                // Während des Schreibens steht hier ein Spinner: der Vorgang
                // dauert am Ruida mehrere Sekunden und blockiert den
                // UI-Thread, ohne Anzeige wirkt die Anwendung eingefroren.
                if state.axis_cal_pending == Some(axis) {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.weak("schreibt…");
                    });
                } else {
                    // Geschrieben wird nur, was auch berechnet werden konnte,
                    // und nie, solange eine andere Achse noch läuft.
                    let ready = preview.is_some() && state.axis_cal_pending.is_none();
                    if ui
                        .add_enabled(ready, egui::Button::new("Schreiben"))
                        .clicked()
                    {
                        requested = Some((axis, target, measured));
                    }
                }
                ui.end_row();
            }
        });
    if let Some((axis, target, measured)) = requested {
        *outcome = LaserManagerOutcome::CalibrateAxis(axis, target, measured);
    }
    // Solange ein Worker arbeitet, weiter zeichnen — sonst stünde der Spinner
    // still und das Ergebnis käme erst beim nächsten Mausereignis an.
    if state.axis_cal_pending.is_some() || state.machine_read_rx.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn locale_number(ui: &mut egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug, value: &mut f64) {
    locale_number_reset(ui, salt, value, false)
}

/// Wie `locale_number`, kann den zwischengelagerten Text aber verwerfen.
/// Nötig, weil das Feld seinen Text in `ui.data_mut` hält: wird nur der Wert
/// im State genullt, holt der Puffer die alte Eingabe im nächsten Frame zurück.
/// Das Zurücksetzen muss dort passieren, wo auch die ID gebildet wird — eine
/// außerhalb nachgebaute `make_persistent_id` trifft einen anderen Eintrag.
fn locale_number_reset(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    value: &mut f64,
    reset: bool,
) {
    let id = ui.make_persistent_id(salt);
    if reset {
        ui.data_mut(|data| data.remove::<String>(id));
    }
    let mut text = ui
        .data_mut(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| format!("{value:.3}"));
    let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(130.0));
    if response.changed() {
        if let Ok(parsed) = text.trim().replace(',', ".").parse::<f64>() {
            *value = parsed;
        }
    }
    if response.lost_focus() {
        text = format!("{value:.3}");
    }
    ui.data_mut(|data| data.insert_temp(id, text));
}

fn controller(ui: &mut egui::Ui, state: &mut LaserManagerState, outcome: &mut LaserManagerOutcome) {
    // Lesen und Schreiben laufen auf dem Geräte-Worker und dauern am Ruida
    // mehrere Sekunden; solange zeigt die Leiste einen Spinner statt Knöpfen,
    // die ins Leere liefen.
    let busy = state.machine_read_rx.is_some();
    ui.horizontal(|ui| {
        if busy {
            ui.spinner();
            ui.weak(match state.machine_write_count {
                Some(_) => "schreibt Register…",
                None => "liest Register…",
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        if ui.button("Maschine auslesen").clicked() {
            *outcome = LaserManagerOutcome::MachineRead;
        }
        let dirty = state.machine_dirty.len();
        if ui
            .add_enabled(
                dirty > 0,
                egui::Button::new(format!("Änderungen schreiben ({dirty})")),
            )
            .clicked()
        {
            state.machine_confirm_write = true;
        }
    });
    ui.weak("Controllerwerte werden live gelesen und nicht im Laserprofil gespeichert.");

    if state.machine_confirm_write {
        ui.add_space(8.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Maschinenregister wirklich schreiben? Falsche Werte können die Mechanik gefährden.",
            );
            ui.horizontal(|ui| {
                if ui.button("Abbrechen").clicked() {
                    state.machine_confirm_write = false;
                }
                if ui.button("Schreiben und erneut lesen").clicked() {
                    *outcome = LaserManagerOutcome::MachineWrite;
                }
            });
        });
    }

    if state.machine_settings.is_empty() {
        ui.add_space(16.0);
        ui.label("Noch keine Maschinendaten gelesen.");
        return;
    }
    let groups: BTreeSet<_> = state
        .machine_settings
        .iter()
        .map(|setting| setting.group.clone())
        .collect();
    for group in groups {
        egui::CollapsingHeader::new(group.clone())
            .default_open(group != "Raw")
            .show(ui, |ui| {
                for setting in state
                    .machine_settings
                    .iter()
                    .filter(|setting| setting.group == group)
                {
                    machine_setting_row(ui, setting, &mut state.machine_dirty);
                }
            });
    }
}

fn machine_setting_row(
    ui: &mut egui::Ui,
    setting: &MachineSetting,
    dirty: &mut std::collections::BTreeMap<u16, i64>,
) {
    let original = setting.raw.unwrap_or_default();
    let mut raw = dirty.get(&setting.address).copied().unwrap_or(original);
    ui.horizontal(|ui| {
        ui.set_min_width(480.0);
        ui.label(format!("0x{:04X}", setting.address));
        ui.label(&setting.label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let changed = if !setting.options.is_empty() {
                let mask = setting.bit_mask.unwrap_or(i64::MAX);
                let mut selected = raw & mask;
                let changed = egui::ComboBox::from_id_salt(("machine", setting.address))
                    .selected_text(
                        setting
                            .options
                            .iter()
                            .find(|(value, _)| *value == selected)
                            .map(|(_, label)| label.as_str())
                            .unwrap_or("Unbekannt"),
                    )
                    .show_ui(ui, |ui| {
                        for (value, label) in &setting.options {
                            ui.selectable_value(&mut selected, *value, label);
                        }
                    })
                    .response
                    .changed();
                if changed {
                    raw = (raw & !mask) | selected;
                }
                changed
            } else if setting.unit == MachineSettingUnit::Raw {
                ui.add_enabled(setting.writable, egui::DragValue::new(&mut raw))
                    .changed()
            } else {
                let factor = setting.unit.factor();
                let mut display = raw as f64 / factor;
                let changed = ui
                    .add_enabled(
                        setting.writable,
                        egui::DragValue::new(&mut display)
                            .speed(0.1)
                            .suffix(format!(" {}", setting.unit.label())),
                    )
                    .changed();
                if changed {
                    raw = (display * factor).round() as i64;
                }
                changed
            };
            if changed {
                if raw == original {
                    dirty.remove(&setting.address);
                } else {
                    dirty.insert(setting.address, raw);
                }
            }
            if !setting.writable {
                ui.weak("geschützt");
            }
        });
    });
}
