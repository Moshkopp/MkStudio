//! Ebenenliste (rechtes Inspector-Panel im Design-Reiter). Farbe = Layer;
//! Doppelklick auf den Namen öffnet den Parameter-Dialog.
//!
//! Über die `UiAction`-Grenze (ADR 0011): Das Panel bekommt eine reine Sicht
//! (`LayerRow`) statt `&mut App` und liefert Absichten zurück.

use egui::RichText;
use luxifer_application::LayerToggle;

use super::action::UiAction;
use super::c32;

/// Reine Darstellungssicht einer Ebene für die Liste. Vom Root aus der Session
/// abgeleitet, damit das Panel nicht selbst auf den Zustand zugreift.
pub(super) struct LayerRow {
    pub color: [u8; 3],
    pub name: String,
    pub visible: bool,
    pub enabled: bool,
    pub locked: bool,
    pub air_assist: bool,
    pub mode: luxifer_core::LayerMode,
    /// Anzahl Shapes auf dieser Ebene.
    pub count: usize,
}

/// `rows` sind in Layer-Reihenfolge (Index 0 = unterste). Angezeigt wird von
/// oben (letzte Ebene) nach unten. Gibt die ausgelösten Absichten zurück.
pub(super) fn layers_panel(ui: &mut egui::Ui, rows: &[LayerRow]) -> Vec<UiAction> {
    let mut actions = Vec::new();
    ui.label(RichText::new("EBENEN").small().weak());
    ui.add_space(4.0);
    if rows.is_empty() {
        ui.weak("Keine Ebenen — zeichne etwas.");
        return actions;
    }
    let n = rows.len();
    // Von oben (letzter Layer) nach unten anzeigen.
    for i in (0..n).rev() {
        let row = &rows[i];
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
                    ui.painter().rect_filled(rect, 6.0, c32(row.color));
                    if resp.on_hover_text("Layerfarbe aktivieren").clicked() {
                        actions.push(UiAction::PickColor(row.color));
                    }
                    ui.vertical(|ui| {
                        if ui
                            .add(
                                egui::Label::new(RichText::new(&row.name).strong())
                                    .sense(egui::Sense::click()),
                            )
                            .on_hover_text("Parameter bearbeiten")
                            .clicked()
                        {
                            actions.push(UiAction::OpenLayerDialog(i));
                        }
                        ui.label(
                            RichText::new(format!("{:?}  ·  {} Objekte", row.mode, row.count))
                                .small()
                                .weak(),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("↓").clicked() && i > 0 {
                            actions.push(UiAction::MoveLayer { from: i, to: i - 1 });
                        }
                        if ui.small_button("↑").clicked() && i + 1 < n {
                            actions.push(UiAction::MoveLayer { from: i, to: i + 1 });
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    let toggles = [
                        ("Sichtbar", row.visible, LayerToggle::Visible),
                        ("Job", row.enabled, LayerToggle::Enabled),
                        ("Gesperrt", row.locked, LayerToggle::Locked),
                        ("Luft", row.air_assist, LayerToggle::AirAssist),
                    ];
                    for (label, active, toggle) in toggles {
                        if ui.selectable_label(active, label).clicked() {
                            actions.push(UiAction::ToggleLayer(i, toggle));
                        }
                    }
                });
            });
        ui.add_space(6.0);
    }
    actions
}

pub(super) fn laser_edit_layers(
    ui: &mut egui::Ui,
    rows: &[LayerRow],
    editable: &std::collections::HashSet<usize>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    ui.separator();
    ui.label(RichText::new("POSITION BEARBEITEN").small().weak());
    ui.label(
        RichText::new("Temporäre Freigabe im Laser-Tab")
            .small()
            .weak(),
    );
    for i in (0..rows.len()).rev() {
        let row = &rows[i];
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 4.0, c32(row.color));
            if ui
                .selectable_label(editable.contains(&i), &row.name)
                .clicked()
            {
                actions.push(UiAction::ToggleLaserEditLayer(i));
            }
        });
    }
    actions
}
