//! Modale Dialoge (egui-Fenster). Native hält jeweils nur den Entwurf; die
//! Mutation läuft über die Session bzw. die temporären Backends.
//!
//! Über die `UiAction`-Grenze (ADR 0011): Ein Dialog bekommt seinen Entwurf als
//! `&mut`-Draft (nicht `&mut App`) und meldet nur, ob der Nutzer übernehmen oder
//! abbrechen will. Den Draft-Lebenszyklus (Übernahme/Verwerfen) führt der Root.

mod guard;
mod laser_settings;
mod layer;
mod text;

pub(super) use guard::guard_dialog;
pub(super) use laser_settings::{laser_settings_window, LaserDialogOutcome};
pub(super) use layer::layer_dialog_window;
pub(super) use text::text_dialog_window;

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
