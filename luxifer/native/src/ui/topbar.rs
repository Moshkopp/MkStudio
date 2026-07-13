//! Obere Kopfzeile: Reiter | Undo/Redo + Datei-Aktionen | Projektname. Wie die
//! Tauri-App liegen die globalen Aktionen im Header, nicht im Werkzeug-Panel.
//!
//! Über die `UiAction`-Grenze (ADR 0011): liest nur Ansicht und Projektnamen,
//! liefert Absichten zurück.

use egui::RichText;

use super::action::UiAction;
use crate::tools::View;

/// `view` = aktive Ansicht (Reiter-Markierung); `project_name` = Anzeige rechts.
pub(super) fn topbar(ui: &mut egui::Ui, view: View, project_name: &str) -> Vec<UiAction> {
    let mut actions = Vec::new();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for v in [View::Projekt, View::Design, View::Preview, View::Laser] {
            if ui
                .selectable_label(view == v, format!("  {}  ", v.label()))
                .clicked()
            {
                actions.push(UiAction::SelectView(v));
            }
        }
        // Verlaufs-/Import-Aktionen nur im Design-Reiter, als Icon-Buttons.
        if view == View::Design {
            let side = 26.0;
            ui.separator();
            if super::tools::icon_button(ui, side, "undo", "Rückgängig (Strg+Z)", false, false) {
                actions.push(UiAction::Undo);
            }
            if super::tools::icon_button(ui, side, "redo", "Wiederholen (Strg+Y)", false, false) {
                actions.push(UiAction::Redo);
            }
            ui.separator();
            if super::tools::icon_button(
                ui,
                side,
                "import",
                "Importieren (SVG, DXF, Bild)",
                false,
                false,
            ) {
                actions.push(UiAction::Import);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("⚙").on_hover_text("Einstellungen").clicked() {
                actions.push(UiAction::OpenSettings);
            }
            ui.label(RichText::new(project_name).weak());
        });
    });
    ui.add_space(4.0);
    actions
}
