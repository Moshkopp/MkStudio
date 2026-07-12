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
        for v in [View::Projekt, View::Design, View::Laser] {
            if ui
                .selectable_label(view == v, format!("  {}  ", v.label()))
                .clicked()
            {
                actions.push(UiAction::SelectView(v));
            }
        }
        // Datei-/Verlaufs-Aktionen nur im Design-Reiter.
        if view == View::Design {
            ui.separator();
            if ui.button("Undo").clicked() {
                actions.push(UiAction::Undo);
            }
            if ui.button("Redo").clicked() {
                actions.push(UiAction::Redo);
            }
            ui.separator();
            if ui.button("Vektor…").clicked() {
                actions.push(UiAction::ImportVector);
            }
            if ui.button("Bild…").clicked() {
                actions.push(UiAction::ImportImage);
            }
            if ui.button("Text…").clicked() {
                actions.push(UiAction::OpenTextDialog);
            }
            let aztec = std::path::Path::new("/home/moshy/Schreibtisch/Aztec.svg");
            if aztec.exists() && ui.button("Aztec laden").clicked() {
                actions.push(UiAction::ImportPath(aztec.to_path_buf()));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(project_name).weak());
        });
    });
    ui.add_space(4.0);
    actions
}
