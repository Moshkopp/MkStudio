//! Layer-Parameter-Dialog (Doppelklick auf eine Ebene). Native hält nur den
//! Entwurf; Speichern läuft über `EditorSession::set_layer_params` mit
//! Validierung, Abbrechen verwirft ihn ohne Mutation. Die Bild-Invariante wird
//! in der UI durch einen festen Modus für Image-Layer gespiegelt und im Core
//! zusätzlich erzwungen.

use egui::RichText;
use luxifer_application::LayerParams;

use super::DialogOutcome;

/// Zeichnet das Fenster auf `p` (den kurzlebigen Entwurf) und meldet, ob der
/// Nutzer übernehmen/abbrechen will. Keine Mutation außerhalb des Entwurfs.
pub(in crate::ui) fn layer_dialog_window(
    ctx: &egui::Context,
    p: &mut LayerParams,
) -> DialogOutcome {
    use luxifer_core::LayerMode;
    let mut outcome = DialogOutcome::None;
    egui::Window::new("Ebene bearbeiten")
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
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
                    outcome = DialogOutcome::Commit;
                }
                if ui.button("Abbrechen").clicked() {
                    outcome = DialogOutcome::Cancel;
                }
            });
        });
    outcome
}
