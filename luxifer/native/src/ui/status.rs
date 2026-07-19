//! Rein lesende Statuszeile am unteren Fensterrand.

use egui::RichText;

/// Statuszeile: FPS, aktives Werkzeug, Objektzahl und rechts die Version
/// (git-abgeleitet, wächst mit jedem Commit — siehe build.rs). Rein lesend —
/// Erfolgs-/Statusmeldungen laufen über die Toasts oben mittig.
pub(super) fn status_bar(ui: &mut egui::Ui, fps: f32, tool: &str, shapes: usize) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{fps:.0} fps")).monospace());
        ui.separator();
        ui.label(format!("Werkzeug: {tool}"));
        ui.separator();
        ui.label(format!("{shapes} Objekte"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(concat!("LuxiFer ", env!("LUXIFER_VERSION")))
                    .small()
                    .weak(),
            );
        });
    });
}
