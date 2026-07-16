//! Obere Kopfzeile: links Aktionen, mittig die Ansichten, rechts Projektstatus
//! und Einstellungen. Wie die Tauri-App liegen die globalen Aktionen im
//! Header, nicht im Werkzeug-Panel.
//!
//! Über die `UiAction`-Grenze (ADR 0011): liest nur Ansicht und Projektnamen,
//! liefert Absichten zurück.

use egui::RichText;

use super::action::UiAction;
use super::CharonTestStatus;
use crate::tools::View;

/// `view` = aktive Ansicht (Reiter-Markierung); `project_name` = Anzeige rechts.
pub(super) fn topbar(
    ui: &mut egui::Ui,
    view: View,
    project_name: &str,
    inbox_count: usize,
    charon_enabled: bool,
    charon_status: &CharonTestStatus,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    ui.add_space(4.0);
    ui.allocate_ui(egui::vec2(ui.available_width(), 26.0), |ui| {
        ui.columns(3, |columns| {
            // Verlaufs-/Import-Aktionen links, nur im Design-Reiter.
            columns[0].horizontal(|ui| {
                if view == View::Design {
                    let side = 26.0;
                    if super::tools::icon_button(
                        ui,
                        side,
                        "new-file",
                        "Neue leere Arbeitsfläche",
                        false,
                        false,
                    ) {
                        actions.push(UiAction::NewBlankProject);
                    }
                    ui.separator();
                    if super::tools::icon_button(
                        ui,
                        side,
                        "undo",
                        "Rückgängig (Strg+Z)",
                        false,
                        false,
                    ) {
                        actions.push(UiAction::Undo);
                    }
                    if super::tools::icon_button(
                        ui,
                        side,
                        "redo",
                        "Wiederholen (Strg+Y)",
                        false,
                        false,
                    ) {
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
            });

            // Stabile Reihenfolge und als zusammengehörige Navigation zentriert.
            columns[1].horizontal_centered(|ui| {
                for target in [View::Projekt, View::Design, View::Laser, View::Preview] {
                    let label = if target == View::Projekt && inbox_count > 0 {
                        format!("  {}  • {}  ", target.label(), inbox_count)
                    } else {
                        format!("  {}  ", target.label())
                    };
                    if view_tab(ui, &label, view == target).clicked() {
                        actions.push(UiAction::SelectView(target));
                    }
                }
            });

            columns[2].with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⚙").on_hover_text("Einstellungen").clicked() {
                    actions.push(UiAction::OpenSettings);
                }
                if charon_enabled {
                    let (color, label, hint) = match charon_status {
                        CharonTestStatus::Syncing(_) => (
                            egui::Color32::from_rgb(0xfb, 0x92, 0x3c),
                            "Charon · Sync",
                            "Charon synchronisiert gerade",
                        ),
                        CharonTestStatus::Connected(_) => (
                            egui::Color32::from_rgb(0x4a, 0xde, 0x80),
                            "Charon · verbunden",
                            "Charon ist verbunden",
                        ),
                        CharonTestStatus::Failed(_) => (
                            ui.visuals().error_fg_color,
                            "Charon · getrennt",
                            "Charon ist nicht erreichbar",
                        ),
                        CharonTestStatus::Idle => (
                            ui.visuals().weak_text_color(),
                            "Charon · wartet",
                            "Charon wartet auf die erste Verbindung",
                        ),
                    };
                    if ui
                        .add(egui::Button::new(
                            RichText::new(format!("⏺ {label}")).color(color),
                        ))
                        .on_hover_text(format!("{hint} · Charon-Eingang öffnen"))
                        .clicked()
                    {
                        actions.push(UiAction::OpenCharonInbox);
                    }
                }
                if ui
                    .button("Assets")
                    .on_hover_text("Asset-Bibliothek öffnen")
                    .clicked()
                {
                    actions.push(UiAction::OpenAssetLibrary);
                }
                ui.label(RichText::new(project_name).weak());
            });
        });
    });
    ui.add_space(4.0);
    actions
}

/// Ruhiger Navigationstab: keine massive Akzentfläche, sondern klare
/// Typografie und eine Markenlinie am aktiven Reiter.
fn view_tab(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let text = if active {
        RichText::new(label).strong()
    } else {
        RichText::new(label).color(ui.visuals().weak_text_color())
    };
    let response = ui.add_sized([86.0, 28.0], egui::Button::new(text).frame(false));
    if active {
        let rect = response.rect;
        ui.painter().line_segment(
            [
                egui::pos2(rect.left() + 12.0, rect.bottom() - 1.0),
                egui::pos2(rect.right() - 12.0, rect.bottom() - 1.0),
            ],
            egui::Stroke::new(2.5, ui.visuals().selection.stroke.color),
        );
    } else if response.hovered() {
        ui.painter().line_segment(
            [
                egui::pos2(response.rect.left() + 18.0, response.rect.bottom() - 1.0),
                egui::pos2(response.rect.right() - 18.0, response.rect.bottom() - 1.0),
            ],
            egui::Stroke::new(1.0, ui.visuals().widgets.hovered.bg_stroke.color),
        );
    }
    response
}
