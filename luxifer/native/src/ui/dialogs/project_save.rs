//! „Neues Projekt"-Maske: Name + Beschreibung, geöffnet direkt in der
//! aktuellen Ansicht (Strg+S ohne offenes Projekt bzw. „Neues Projekt…" im
//! Projekt-Reiter). Native hält nur den Entwurf; das Anlegen validiert der
//! `ProjectService` (leerer Name → Fehler, Maske bleibt offen).

use crate::ui::state::ProjectSaveDialogState;

use super::DialogOutcome;

pub(in crate::ui) fn project_save_dialog_window(
    ctx: &egui::Context,
    st: &mut ProjectSaveDialogState,
) -> DialogOutcome {
    let mut outcome = DialogOutcome::None;
    egui::Window::new("Projekt speichern")
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(360.0);

            ui.label("Name");
            let name_edit = ui.add(
                egui::TextEdit::singleline(&mut st.name)
                    .hint_text("Projektname")
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut st.focus_name) {
                name_edit.request_focus();
            }
            // Enter im Namensfeld speichert direkt.
            if name_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                outcome = DialogOutcome::Commit;
            }

            ui.add_space(6.0);
            ui.label("Beschreibung (optional)");
            ui.add(
                egui::TextEdit::multiline(&mut st.description)
                    .hint_text("Worum geht es in diesem Projekt?")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );

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
