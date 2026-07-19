//! Obere Kopfzeile: links Aktionen, mittig die Ansichten, rechts kompakte
//! Verbindungs-/Systemzustände und Einstellungen. Wie die Tauri-App liegen die globalen Aktionen im
//! Header, nicht im Werkzeug-Panel.
//!
//! Über die `UiAction`-Grenze (ADR 0011): liest nur Ansicht und Statussichten,
//! liefert Absichten zurück.

use egui::RichText;

use super::action::UiAction;
use super::CharonTestStatus;
use crate::tools::View;

/// `view` = aktive Ansicht (Reiter-Markierung).
pub(super) fn topbar(
    ui: &mut egui::Ui,
    view: View,
    inbox_count: usize,
    charon_enabled: bool,
    charon_status: &CharonTestStatus,
    lasers: &luxifer_core::LaserRegistry,
    laser_connected: bool,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    ui.add_space(4.0);
    ui.allocate_ui(egui::vec2(ui.available_width(), 26.0), |ui| {
        ui.columns(3, |columns| {
            // Asset-Bibliothek global ganz links; danach die Design-Aktionen.
            columns[0].horizontal(|ui| {
                let side = 26.0;
                if super::tools::icon_button(
                    ui,
                    side,
                    "assets",
                    "Asset-Bibliothek öffnen",
                    false,
                    false,
                ) {
                    actions.push(UiAction::OpenAssetLibrary);
                }
                if view == View::Design {
                    ui.separator();
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
                if ui
                    .add(egui::Button::new("⚙").frame(false))
                    .on_hover_text("Einstellungen")
                    .clicked()
                {
                    actions.push(UiAction::OpenSettings);
                }
                if charon_enabled {
                    ui.separator();
                    let (color, hint) = match charon_status {
                        CharonTestStatus::Syncing(_) => (
                            egui::Color32::from_rgb(0xfb, 0xbf, 0x24),
                            "Charon synchronisiert gerade",
                        ),
                        CharonTestStatus::Connected(_) => (
                            egui::Color32::from_rgb(0x4a, 0xde, 0x80),
                            "Charon ist verbunden",
                        ),
                        CharonTestStatus::Failed(_) => {
                            (ui.visuals().error_fg_color, "Charon ist nicht erreichbar")
                        }
                        CharonTestStatus::Idle => {
                            (ui.visuals().error_fg_color, "Charon ist nicht verbunden")
                        }
                    };
                    ui.horizontal(|ui| {
                        status_dot(ui, color, hint);
                        if ui
                            .add(
                                egui::Label::new(RichText::new("Charon").color(color))
                                    .sense(egui::Sense::click()),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(format!("{hint} · Charon-Eingang öffnen"))
                            .clicked()
                        {
                            actions.push(UiAction::OpenCharonInbox);
                        }
                    });
                }
                ui.separator();
                let active_laser = lasers.active();
                let laser_color = if laser_connected {
                    egui::Color32::from_rgb(0x4a, 0xde, 0x80)
                } else {
                    ui.visuals().error_fg_color
                };
                let laser_label = active_laser
                    .map(|profile| profile.name.clone())
                    .unwrap_or_else(|| "Kein Laser".into());
                egui::ComboBox::from_id_salt("header_laser")
                    .selected_text(laser_label)
                    .width(125.0)
                    .show_ui(ui, |ui| {
                        for profile in &lasers.profiles {
                            if ui
                                .selectable_label(
                                    lasers.active_id.as_deref() == Some(profile.id.as_str()),
                                    &profile.name,
                                )
                                .clicked()
                            {
                                actions.push(UiAction::LaserSelect(profile.id.clone()));
                            }
                        }
                    })
                    .response
                    .on_hover_text(if laser_connected {
                        "Laser verbunden · Verbindung im Laser-Tab verwalten"
                    } else {
                        "Laser nicht verbunden · Verbindung im Laser-Tab herstellen"
                    });
                status_dot(
                    ui,
                    laser_color,
                    if laser_connected {
                        "Laser verbunden"
                    } else {
                        "Laser nicht verbunden"
                    },
                );
            });
        });
    });
    ui.add_space(4.0);
    actions
}

fn status_dot(ui: &mut egui::Ui, color: egui::Color32, hint: &str) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(9.0, 18.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
    response.on_hover_text(hint);
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
