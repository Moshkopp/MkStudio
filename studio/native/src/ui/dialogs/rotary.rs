//! Rotary einrichten und ein-/ausschalten (ADR 0022/0023).
//!
//! Sitzt bewusst nicht in der Laser-Verwaltung: Bauart und Durchmesser gehören
//! zwar zum Gerät, aber das An/Aus wird zwischen zwei Aufträgen umgeschaltet.
//!
//! **Beim Ruida rechnet der Controller.** `rotary_enable` schaltet die
//! Y-Bewegung auf Drehung um; die Skalierung macht die Firmware aus
//! `pulses_per_rot` und `rotary_diameter`. Studio darf deshalb NICHT zusätzlich
//! umrechnen — es setzt nur die Register passend zur eingerichteten Bauart.
//! Der Umfang aus dem Fachmodell dient hier der Kontrolle und dazu, aus der
//! Bauart die richtigen Registerwerte abzuleiten.

use egui::RichText;
use studio_core::{Rotary, RotaryKind};

use crate::ui::RotaryDialogState;

#[derive(Debug, Clone, PartialEq, Default)]
pub(in crate::ui) enum RotaryOutcome {
    #[default]
    None,
    Close,
    /// Geänderte Einstellung ins Profil übernehmen.
    Apply(Rotary),
    /// Rotary-Register frisch aus dem Controller lesen.
    ReadController,
    /// Bauart und An/Aus in die Controller-Register schreiben.
    WriteController(Rotary),
}

pub(in crate::ui) fn rotary_window(
    root_ui: &mut egui::Ui,
    state: &mut RotaryDialogState,
) -> RotaryOutcome {
    let mut outcome = RotaryOutcome::None;
    let mut open = true;

    egui::Window::new("Rotary")
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .fixed_size(egui::vec2(420.0, 0.0))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(root_ui, |ui| {
            ui.checkbox(&mut state.draft.active, "Rotary aktiv");
            ui.weak("Der Rotary läuft am Y-Ausgang: der Controller deutet jede");
            ui.weak("Y-Bewegung als Drehung. Der Y-Motor muss dafür abgeklemmt sein.");
            ui.add_space(10.0);

            bauart(ui, state);
            ui.add_space(10.0);
            abwicklung(ui, state);
            ui.add_space(10.0);
            controller_werte(ui, state, &mut outcome);

            ui.add_space(12.0);
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::new(
                        RichText::new("Im Profil speichern").strong(),
                    ))
                    .on_hover_text(
                        "Speichert nur in Studio. Wirksam an der Maschine wird es \
                         erst über „Ins Gerät schreiben“.",
                    )
                    .clicked()
                {
                    outcome = RotaryOutcome::Apply(state.draft);
                }
                if ui.button("Abbrechen").clicked() {
                    outcome = RotaryOutcome::Close;
                }
            });
        });

    if !open || root_ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        RotaryOutcome::Close
    } else {
        outcome
    }
}

fn bauart(ui: &mut egui::Ui, state: &mut RotaryDialogState) {
    ui.label(RichText::new("Bauart").strong());
    let is_roller = matches!(state.draft.kind, RotaryKind::Roller { .. });
    ui.horizontal(|ui| {
        if ui.selectable_label(is_roller, "Roller (Walzen)").clicked() {
            state.draft.kind = RotaryKind::Roller {
                roller_diameter_mm: state.roller_diameter_mm,
            };
        }
        if ui.selectable_label(!is_roller, "Chuck (Futter)").clicked() {
            state.draft.kind = RotaryKind::Chuck {
                object_diameter_mm: state.object_diameter_mm,
            };
        }
    });

    ui.add_space(6.0);
    egui::Grid::new("rotary_bauart")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            if is_roller {
                ui.label("Walzendurchmesser (mm)");
                if number(ui, "rotary_roller", &mut state.roller_diameter_mm) {
                    state.draft.kind = RotaryKind::Roller {
                        roller_diameter_mm: state.roller_diameter_mm,
                    };
                }
                ui.end_row();
                ui.weak("Objektdurchmesser");
                ui.weak("spielt beim Roller keine Rolle");
                ui.end_row();
            } else {
                ui.label("Objektdurchmesser (mm)");
                if number(ui, "rotary_object", &mut state.object_diameter_mm) {
                    state.draft.kind = RotaryKind::Chuck {
                        object_diameter_mm: state.object_diameter_mm,
                    };
                }
                ui.end_row();
            }
            ui.label("Schritte pro Umdrehung");
            number(ui, "rotary_steps", &mut state.draft.steps_per_rev);
            ui.end_row();
        });
}

/// Was aus der Einstellung folgt — der Umfang ist die Zahl, an der man einen
/// Zahlendreher sofort erkennt.
fn abwicklung(ui: &mut egui::Ui, state: &mut RotaryDialogState) {
    let circumference = state.draft.circumference_mm();
    ui.label(RichText::new("Abwicklung").strong());
    if circumference.is_finite() && circumference > 0.0 {
        ui.label(format!("Umfang: {circumference:.2} mm pro Umdrehung"));
        match state.draft.steps_per_mm() {
            Some(per_mm) => ui.weak(format!("{per_mm:.3} Schritte pro mm")),
            None => ui.weak("Schritte pro mm: —"),
        };
    } else {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "Durchmesser muss größer als 0 sein.",
        );
        return;
    }

    // Kalibrierhilfe: korrigiert werden bewusst NUR die Pulse. Der
    // Walzendurchmesser ist gemessen — daran zu rechnen macht die Einrichtung
    // unklar, weil man dann zwei Größen gleichzeitig verstellt.
    ui.add_space(8.0);
    ui.label("Testgravur nachmessen und korrigieren:");
    ui.horizontal(|ui| {
        ui.label("Soll");
        number(ui, "rotary_cal_target", &mut state.cal_target_mm);
        ui.label("Ist");
        number(ui, "rotary_cal_measured", &mut state.cal_measured_mm);
        ui.label("mm");
    });

    let corrected = studio_core::calibrated_pulses_per_rev(
        state.draft.steps_per_rev,
        state.cal_target_mm,
        state.cal_measured_mm,
    )
    .ok();
    ui.horizontal(|ui| {
        match corrected {
            Some(value) => ui.label(format!("→ {value:.0} Pulse pro Umdrehung")),
            None => ui.weak("→ Soll und Ist eintragen"),
        };
        if ui
            .add_enabled(corrected.is_some(), egui::Button::new("Übernehmen"))
            .clicked()
        {
            if let Some(value) = corrected {
                state.draft.steps_per_rev = value;
                // Auch das Zahlenfeld oben neu aufbauen lassen, sonst zeigte es
                // weiter den alten Wert aus seinem Textpuffer.
                let id = ui.make_persistent_id("rotary_steps");
                ui.data_mut(|data| data.remove::<String>(id));
                state.cal_target_mm = 0.0;
                state.cal_measured_mm = 0.0;
                for salt in ["rotary_cal_target", "rotary_cal_measured"] {
                    let id = ui.make_persistent_id(salt);
                    ui.data_mut(|data| data.remove::<String>(id));
                }
            }
        }
    });
    ui.weak("Danach ins Gerät schreiben.");
}

fn controller_werte(ui: &mut egui::Ui, state: &mut RotaryDialogState, outcome: &mut RotaryOutcome) {
    ui.label(RichText::new("Controller").strong());
    ui.weak("Der Ruida rechnet die Drehung selbst — diese Werte steuern sie.");
    ui.add_space(4.0);

    if state.reading {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.weak("liest Register…");
        });
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
        return;
    }

    // Soll gegen Ist: so sieht man auf einen Blick, ob der Controller zur
    // eingerichteten Bauart passt.
    let soll_diameter = state.draft.kind.driving_diameter_mm();
    let soll_pulses = state.draft.steps_per_rev;
    egui::Grid::new("rotary_controller")
        .num_columns(3)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label("");
            ui.strong("im Gerät");
            ui.strong("soll");
            ui.end_row();

            ui.label("Rotary aktiv");
            // Weicht der Gerätezustand vom Profil ab, farblich markieren: am
            // Controller lässt sich Rotary auch direkt umschalten.
            match state.controller.and_then(|values| values.enabled) {
                Some(active) => {
                    let text = if active { "ja" } else { "nein" };
                    if active != state.draft.active {
                        ui.colored_label(ui.visuals().warn_fg_color, text)
                    } else {
                        ui.label(text)
                    }
                }
                None => ui.weak("—"),
            };
            ui.label(if state.draft.active { "ja" } else { "nein" });
            ui.end_row();

            ui.label("Pulse pro Umdrehung");
            wert_mit_abweichung(
                ui,
                state.controller.and_then(|values| values.pulses_per_rot),
                soll_pulses,
                0,
            );
            ui.label(format!("{soll_pulses:.0}"));
            ui.end_row();

            ui.label("Durchmesser (mm)");
            wert_mit_abweichung(
                ui,
                state.controller.and_then(|values| values.diameter_mm),
                soll_diameter,
                3,
            );
            ui.label(format!("{soll_diameter:.3}"));
            ui.end_row();
        });

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("Aus Gerät lesen").clicked() {
            *outcome = RotaryOutcome::ReadController;
        }
        if ui
            .button("Ins Gerät schreiben")
            .on_hover_text("Setzt Rotary-Modus, Pulse und Durchmesser im Controller")
            .clicked()
        {
            *outcome = RotaryOutcome::WriteController(state.draft);
        }
    });
}

/// Gelesener Wert; weicht er vom Soll ab, wird er hervorgehoben — eine stille
/// Abweichung wäre genau der Fall, der schief gravierte Teile erzeugt.
fn wert_mit_abweichung(ui: &mut egui::Ui, actual: Option<f64>, expected: f64, decimals: usize) {
    match actual {
        Some(value) => {
            let text = format!("{value:.decimals$}");
            if (value - expected).abs() > 1e-3 {
                ui.colored_label(ui.visuals().warn_fg_color, text);
            } else {
                ui.label(text);
            }
        }
        None => {
            ui.weak("—");
        }
    }
}

/// Zahlenfeld mit Komma oder Punkt; `true`, wenn der Wert sich geändert hat.
fn number(ui: &mut egui::Ui, salt: &str, value: &mut f64) -> bool {
    let id = ui.make_persistent_id(salt);
    let mut text = ui
        .data_mut(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| format!("{value:.3}"));
    let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(120.0));
    let mut changed = false;
    if response.changed() {
        if let Ok(parsed) = text.trim().replace(',', ".").parse::<f64>() {
            *value = parsed;
            changed = true;
        }
    }
    if response.lost_focus() {
        text = format!("{value:.3}");
    }
    ui.data_mut(|data| data.insert_temp(id, text));
    changed
}
