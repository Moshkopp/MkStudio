//! Modale Dialoge (egui-Fenster). Native hält jeweils nur den Entwurf; die
//! Mutation läuft über die Session bzw. die temporären Backends.
//!
//! Über die `UiAction`-Grenze (ADR 0011): Ein Dialog bekommt seinen Entwurf als
//! `&mut`-Draft (nicht `&mut App`) und meldet nur, ob der Nutzer übernehmen oder
//! abbrechen will. Den Draft-Lebenszyklus (Übernahme/Verwerfen) führt der Root.

mod geo_op;
mod guard;
mod image;
mod laser_manager;
mod layer;
mod layer_manager;
mod material;
mod project_save;
mod revision_compare;
mod rotary;
mod settings;
mod text;

pub(super) use geo_op::geo_op_dialog_window;
pub(super) use guard::guard_dialog;
pub(super) use image::{image_dialog_window, ImageDialogOutcome};
pub(super) use laser_manager::{laser_manager_window, LaserManagerOutcome};
pub(super) use layer::{layer_dialog_window, LayerDialogOutcome};
pub(super) use layer_manager::{layer_manager_window, LayerManagerOutcome};
pub(super) use material::{material_manager_window, MaterialManagerOutcome};
pub(super) use project_save::project_save_dialog_window;
pub(super) use revision_compare::{revision_comparison_window, RevisionComparisonOutcome};
pub(super) use rotary::{rotary_window, RotaryOutcome};
pub(super) use settings::{settings_dialog_window, SettingsOutcome};
pub(super) use text::text_dialog_window;

/// Einheitliche modale Abdunklung hinter allen Dialogen. Die Fläche fängt
/// zugleich Interaktionen mit der darunterliegenden Anwendung ab.
pub(super) fn modal_backdrop(root_ui: &mut egui::Ui, alpha: u8) {
    let screen = root_ui.max_rect();
    egui::Area::new(egui::Id::new("modal_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .show(root_ui, |ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, screen.size());
            ui.allocate_rect(rect, egui::Sense::click_and_drag());
            ui.painter()
                .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(alpha));
        });
}

/// Was ein Dialog nach einem Frame will. `None` = weiter offen, keine Aktion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum DialogOutcome {
    /// Fenster bleibt offen, Nutzer bearbeitet weiter.
    #[default]
    None,
    /// Nutzer will den Entwurf übernehmen.
    Commit,
    /// Nutzer hat abgebrochen.
    Cancel,
}
