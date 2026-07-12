//! Dirty-Guard-Bestätigung: warnt vor dem Verwerfen ungespeicherter Änderungen,
//! wenn eine Projektaktion (Neu/Öffnen) den Editorzustand ersetzen würde.

use super::DialogOutcome;

/// Zeigt die Warnung. `action_label` beschreibt die wartende Aktion (z. B.
/// „Projekt öffnen"). `Commit` = verwerfen und fortfahren, `Cancel` = abbrechen.
pub(in crate::ui) fn guard_dialog(ctx: &egui::Context, action_label: &str) -> DialogOutcome {
    let mut outcome = DialogOutcome::None;
    egui::Window::new("Ungespeicherte Änderungen")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.label(format!(
                "Es gibt ungespeicherte Änderungen. {action_label} verwirft sie."
            ));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Verwerfen und fortfahren").clicked() {
                    outcome = DialogOutcome::Commit;
                }
                if ui.button("Abbrechen").clicked() {
                    outcome = DialogOutcome::Cancel;
                }
            });
        });
    outcome
}
