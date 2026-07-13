//! Anordnen-Leiste (zweite Kopfzeile im Design-Reiter): Ausrichten, Verteilen,
//! Gruppieren/Lösen sowie numerische Auswahlgröße.
//!
//! Pilot der `UiAction`-Grenze (ADR 0011): Das Panel zeichnet nur, liest allein
//! die Auswahlanzahl und liefert Absichten zurück, statt `App` zu mutieren.

use egui::Color32;

use super::action::UiAction;
use super::state::SelectionSizeState;
use super::ICON_BUTTON_SIDE;

/// Kleiner horizontaler Icon-Knopf (Anordnen-Leiste). `dim` = deaktiviert.
fn bar_icon(ui: &mut egui::Ui, icon: &str, tip: &str, enabled: bool) -> bool {
    let side = ICON_BUTTON_SIDE;
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

/// Mausbedienbarer Lock-Schalter ohne Tastaturfokus. Dadurch wechselt Tab
/// direkt zwischen den beiden Maßeingaben, statt auf dem Symbol zu landen.
fn aspect_lock(ui: &mut egui::Ui, locked: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::CLICK);
    let fill = if locked {
        ui.visuals().selection.bg_fill
    } else if response.hovered() {
        ui.visuals().widgets.hovered.bg_fill
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };
    ui.painter().rect(
        rect,
        6.0,
        fill,
        ui.visuals().widgets.inactive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let icon_rect = rect.shrink(7.0);
    crate::icons::draw(
        ui.painter(),
        icon_rect,
        if locked { "lock" } else { "unlock" },
        ui.visuals().widgets.inactive.fg_stroke.color,
    );
    response
        .on_hover_text(if locked {
            "Seitenverhältnis beibehalten"
        } else {
            "Länge und Breite unabhängig ändern"
        })
        .clicked()
}

fn size_input(ui: &mut egui::Ui, id: egui::Id, value: &mut String) -> egui::Response {
    let mut output = egui::TextEdit::singleline(value)
        .id(id)
        .desired_width(72.0)
        .horizontal_align(egui::Align::RIGHT)
        .show(ui);
    if output.response.gained_focus() {
        use egui::text::{CCursor, CCursorRange};
        output.state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(0),
            CCursor::new(value.chars().count()),
        )));
        output.state.store(ui.ctx(), output.response.id);
    }
    output.response.response
}

/// Anordnen-Leiste: Ausrichten (7), Verteilen (4), Gruppieren/Lösen und Maße.
/// `selection` = Anzahl der (gruppierten) Auswahleinheiten; steuert nur die
/// Aktivierung. Gibt die ausgelösten Absichten zurück.
pub(super) fn arrange_bar(
    ui: &mut egui::Ui,
    selection: usize,
    bbox: Option<luxifer_core::BBox>,
    size: &mut SelectionSizeState,
) -> Vec<UiAction> {
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
        size_editor(ui, bbox, size, &mut actions);
    });
    actions
}

fn size_editor(
    ui: &mut egui::Ui,
    bbox: Option<luxifer_core::BBox>,
    state: &mut SelectionSizeState,
    actions: &mut Vec<UiAction>,
) {
    let width_id = ui.make_persistent_id("selection_width");
    let height_id = ui.make_persistent_id("selection_height");
    let editing = ui.memory(|m| {
        m.focused()
            .is_some_and(|id| id == width_id || id == height_id)
    });

    if !editing && state.source != bbox {
        sync_draft(state, bbox);
    }

    ui.add_enabled_ui(bbox.is_some(), |ui| {
        ui.label("L").on_hover_text("Länge der Auswahl");
        let width = size_input(ui, width_id, &mut state.width);
        state.width_dirty |= width.changed();

        ui.label("B").on_hover_text("Breite der Auswahl");
        let height = size_input(ui, height_id, &mut state.height);
        state.height_dirty |= height.changed();
        ui.label("mm");
        if aspect_lock(ui, state.proportional) {
            state.proportional = !state.proportional;
        }

        if width.lost_focus() && state.width_dirty {
            commit_size(state, bbox, true, actions);
        } else if height.lost_focus() && state.height_dirty {
            commit_size(state, bbox, false, actions);
        }
    });
}

fn commit_size(
    state: &mut SelectionSizeState,
    bbox: Option<luxifer_core::BBox>,
    width_changed: bool,
    actions: &mut Vec<UiAction>,
) {
    let Some(source) = bbox else {
        sync_draft(state, None);
        return;
    };
    let width = parse_mm(&state.width);
    let height = parse_mm(&state.height);
    let target = match (width_changed, state.proportional, width, height) {
        (true, true, Some(w), _) if source.w > 0.0 => Some((w, source.h * w / source.w)),
        (false, true, _, Some(h)) if source.h > 0.0 => Some((source.w * h / source.h, h)),
        (_, false, Some(w), Some(h)) => Some((w, h)),
        _ => None,
    };
    if let Some((width, height)) = target.filter(|(w, h)| *w >= 0.1 && *h >= 0.1) {
        state.width = format_mm(width);
        state.height = format_mm(height);
        state.width_dirty = false;
        state.height_dirty = false;
        actions.push(UiAction::ResizeSelection { width, height });
    } else {
        sync_draft(state, Some(source));
    }
}

fn sync_draft(state: &mut SelectionSizeState, bbox: Option<luxifer_core::BBox>) {
    state.source = bbox;
    state.width = bbox.map(|b| format_mm(b.w)).unwrap_or_default();
    state.height = bbox.map(|b| format_mm(b.h)).unwrap_or_default();
    state.width_dirty = false;
    state.height_dirty = false;
}

fn parse_mm(value: &str) -> Option<f64> {
    value.trim().replace(',', ".").parse().ok()
}

fn format_mm(value: f64) -> String {
    format!("{value:.2}").replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proportionaler_breitenwert_berechnet_hoehe() {
        let bbox = luxifer_core::BBox::new(0.0, 0.0, 100.0, 50.0);
        let mut state = SelectionSizeState::default();
        sync_draft(&mut state, Some(bbox));
        state.width = "200,0".into();
        state.width_dirty = true;
        let mut actions = Vec::new();

        commit_size(&mut state, Some(bbox), true, &mut actions);

        assert_eq!(state.height, "100,00");
        assert_eq!(
            actions,
            vec![UiAction::ResizeSelection {
                width: 200.0,
                height: 100.0,
            }]
        );
    }

    #[test]
    fn entsperrte_masse_bleiben_unabhaengig() {
        let bbox = luxifer_core::BBox::new(0.0, 0.0, 100.0, 50.0);
        let mut state = SelectionSizeState {
            proportional: false,
            ..Default::default()
        };
        sync_draft(&mut state, Some(bbox));
        state.width = "120".into();
        state.height = "35,5".into();
        state.width_dirty = true;
        let mut actions = Vec::new();

        commit_size(&mut state, Some(bbox), true, &mut actions);

        assert_eq!(
            actions,
            vec![UiAction::ResizeSelection {
                width: 120.0,
                height: 35.5,
            }]
        );
    }
}
