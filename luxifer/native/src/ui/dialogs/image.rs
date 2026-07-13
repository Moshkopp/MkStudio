//! Bildparameter-Dialog (Doppelklick auf ein Bild-Objekt). Bearbeitet die
//! nicht-destruktiven Verarbeitungsparameter (ADR 0004) und bietet das
//! Vektorisieren (Trace) an; Native hält nur den Entwurf, Speichern läuft
//! über `EditorSession::set_image_params`, Trace über
//! `EditorSession::trace_image`.

use luxifer_core::ImageMode;

use super::super::state::ImageDialogState;

/// Ergebnis des Bild-Dialogs. `Trace` vektorisiert das Bild mit den
/// Trace-Reglern des Entwurfs (der Dialog bleibt dabei offen).
#[derive(PartialEq, Eq)]
pub(in crate::ui) enum ImageDialogOutcome {
    None,
    Save,
    Cancel,
    Trace,
}

pub(in crate::ui) fn image_dialog_window(
    ctx: &egui::Context,
    st: &mut ImageDialogState,
) -> ImageDialogOutcome {
    let mut outcome = ImageDialogOutcome::None;
    let p = &mut st.params;
    egui::Window::new("Bild bearbeiten")
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
            egui::Grid::new("image_cfg")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Modus");
                    let mode_label = |m: ImageMode| match m {
                        ImageMode::Grayscale => "Graustufe",
                        ImageMode::Threshold => "Schwelle",
                        ImageMode::Floyd => "Floyd–Steinberg",
                        ImageMode::Jarvis => "Jarvis",
                        ImageMode::Stucki => "Stucki",
                        ImageMode::Atkinson => "Atkinson",
                        ImageMode::Bayer => "Bayer 4×4",
                        ImageMode::LaserRuns => "Laser-Runs",
                    };
                    egui::ComboBox::from_id_salt("image_mode")
                        .selected_text(mode_label(p.mode))
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for m in [
                                ImageMode::Grayscale,
                                ImageMode::Threshold,
                                ImageMode::Floyd,
                                ImageMode::Jarvis,
                                ImageMode::Stucki,
                                ImageMode::Atkinson,
                                ImageMode::Bayer,
                                ImageMode::LaserRuns,
                            ] {
                                ui.selectable_value(&mut p.mode, m, mode_label(m));
                            }
                        });
                    ui.end_row();

                    if p.mode == ImageMode::Threshold {
                        ui.label("Schwelle");
                        ui.add(egui::Slider::new(&mut p.threshold, 0..=255));
                        ui.end_row();
                    }

                    ui.label("Helligkeit");
                    ui.add(egui::Slider::new(&mut p.brightness, -100..=100));
                    ui.end_row();

                    ui.label("Kontrast");
                    ui.add(egui::Slider::new(&mut p.contrast, -100..=100));
                    ui.end_row();

                    ui.label("Gamma");
                    ui.add(egui::Slider::new(&mut p.gamma, 0.1..=3.0));
                    ui.end_row();

                    ui.label("Invertieren (Canvas)");
                    ui.checkbox(&mut p.invert_editor, "");
                    ui.end_row();

                    ui.label("Invertieren (Laser)");
                    ui.checkbox(&mut p.invert_laser, "");
                    ui.end_row();
                });

            ui.add_space(10.0);
            ui.separator();
            // Vektorisieren: Tonwert-LUT (oben) wirkt vor der Trace-Schwelle.
            ui.strong("Vektorisieren (Trace)");
            egui::Grid::new("image_trace")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Schwelle");
                    ui.add(egui::Slider::new(&mut st.trace_threshold, 0..=255));
                    ui.end_row();
                    ui.label("Invertieren");
                    ui.checkbox(&mut st.trace_invert, "");
                    ui.end_row();
                });
            if ui.button("Konturen erzeugen").clicked() {
                outcome = ImageDialogOutcome::Trace;
            }
            ui.weak("Erzeugt geschlossene Konturen auf dem aktiven Zeichen-Layer.");

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Speichern").clicked() {
                    outcome = ImageDialogOutcome::Save;
                }
                if ui.button("Abbrechen").clicked() {
                    outcome = ImageDialogOutcome::Cancel;
                }
            });
        });
    outcome
}
