//! Anordnen-Leiste (zweite Kopfzeile im Design-Reiter): Ausrichten, Verteilen,
//! Gruppieren/Lösen und Nesting.
//!
//! Pilot der `UiAction`-Grenze (ADR 0011): Das Panel zeichnet nur, liest allein
//! die Auswahlanzahl und liefert Absichten zurück, statt `App` zu mutieren.

use egui::Color32;

use super::action::UiAction;

/// Kleiner horizontaler Icon-Knopf (Anordnen-Leiste). `dim` = deaktiviert.
fn bar_icon(ui: &mut egui::Ui, icon: &str, tip: &str, enabled: bool) -> bool {
    let side = 28.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
    let hov = resp.hovered() && enabled;
    let bg = if hov {
        Color32::from_rgb(0x25, 0x2a, 0x33)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 6.0, bg);
    let fg = if enabled {
        Color32::from_rgb(0xd4, 0xd8, 0xdd)
    } else {
        Color32::from_rgb(0x55, 0x5a, 0x62)
    };
    let pad = side * 0.2;
    let ic = egui::Rect::from_min_max(
        rect.min + egui::vec2(pad, pad),
        rect.max - egui::vec2(pad, pad),
    );
    crate::icons::draw(ui.painter(), ic, icon, fg);
    enabled && resp.on_hover_text(tip).clicked()
}

/// Anordnen-Leiste: Ausrichten (7), Verteilen (4), Gruppieren/Lösen, Nesting.
/// `selection` = Anzahl der (gruppierten) Auswahleinheiten; steuert nur die
/// Aktivierung. Gibt die ausgelösten Absichten zurück.
pub(super) fn arrange_bar(ui: &mut egui::Ui, selection: usize) -> Vec<UiAction> {
    use luxifer_core::{Align, Distribute};
    let mut actions = Vec::new();
    let n = selection;
    ui.horizontal(|ui| {
        // Ausrichten (ab 1 Objekt).
        let a1 = n >= 1;
        if bar_icon(ui, "align-left", "Links ausrichten", a1) {
            actions.push(UiAction::Align(Align::Left));
        }
        if bar_icon(ui, "align-hcenter", "Horizontal zentrieren", a1) {
            actions.push(UiAction::Align(Align::HCenter));
        }
        if bar_icon(ui, "align-right", "Rechts ausrichten", a1) {
            actions.push(UiAction::Align(Align::Right));
        }
        ui.add_space(2.0);
        if bar_icon(ui, "align-top", "Oben ausrichten", a1) {
            actions.push(UiAction::Align(Align::Top));
        }
        if bar_icon(ui, "align-vcenter", "Vertikal zentrieren", a1) {
            actions.push(UiAction::Align(Align::VCenter));
        }
        if bar_icon(ui, "align-bottom", "Unten ausrichten", a1) {
            actions.push(UiAction::Align(Align::Bottom));
        }
        if bar_icon(ui, "align-center", "Auf beiden Achsen zentrieren", a1) {
            actions.push(UiAction::Align(Align::Center));
        }
        ui.separator();
        // Verteilen (ab 3 Objekten).
        let a3 = n >= 3;
        if bar_icon(ui, "dist-h", "Horizontal verteilen", a3) {
            actions.push(UiAction::Distribute(Distribute::Horizontal));
        }
        if bar_icon(ui, "space-h", "Horizontale Abstände angleichen", a3) {
            actions.push(UiAction::Distribute(Distribute::SpaceHorizontal));
        }
        if bar_icon(ui, "dist-v", "Vertikal verteilen", a3) {
            actions.push(UiAction::Distribute(Distribute::Vertical));
        }
        if bar_icon(ui, "space-v", "Vertikale Abstände angleichen", a3) {
            actions.push(UiAction::Distribute(Distribute::SpaceVertical));
        }
        ui.separator();
        // Gruppieren.
        if bar_icon(ui, "group", "Gruppieren", n >= 2) {
            actions.push(UiAction::Group);
        }
        if bar_icon(ui, "ungroup", "Gruppierung lösen", n >= 1) {
            actions.push(UiAction::Ungroup);
        }
        ui.separator();
        // Nesting: Packen (≥2) / Bett füllen (≥1), fester Abstand 2 mm.
        if bar_icon(ui, "nest", "Auswahl packen (2 mm)", n >= 2) {
            actions.push(UiAction::Nest(2.0));
        }
        if ui
            .add_enabled(n >= 1, egui::Button::new("Bett füllen"))
            .clicked()
        {
            actions.push(UiAction::NestFill(2.0));
        }
    });
    actions
}
