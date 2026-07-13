//! Text-Dialog: Eingabe, Font-Auswahl, Größe → Text als Pfad einfügen.

use super::super::state::TextDialogState;
use super::DialogOutcome;

/// Zeichnet das Fenster auf den Entwurf `st`; `font_names` ist die reine
/// Anzeigeliste der Systemfonts (Index korrespondiert mit `st.font_idx`).
/// Meldet über `DialogOutcome`, ob der Nutzer einfügen/abbrechen will.
pub(in crate::ui) fn text_dialog_window(
    ctx: &egui::Context,
    st: &mut TextDialogState,
    font_names: &[String],
) -> DialogOutcome {
    let mut outcome = DialogOutcome::None;
    egui::Window::new("Text einfügen")
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
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
                    outcome = DialogOutcome::Commit;
                }
                if ui.button("Abbrechen").clicked() {
                    outcome = DialogOutcome::Cancel;
                }
            });
        });
    outcome
}
