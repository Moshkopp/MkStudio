//! Fehler-Banner (oben) und Statuszeile (unten). Beide rein darstellend; der
//! Banner meldet nur „schließen" als Absicht zurück.

use egui::{Color32, RichText};

use super::action::UiAction;

/// Rotes Banner mit `message` und stabilem Fehlercode. Gibt `DismissError`
/// zurück, wenn der Nutzer schließt.
pub(super) fn error_banner(
    ui: &mut egui::Ui,
    message: &str,
    code: &str,
    details: Option<&str>,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    ui.horizontal(|ui| {
        ui.colored_label(
            Color32::from_rgb(0xf8, 0x71, 0x71),
            format!("{message}  [{code}]"),
        );
        if ui.small_button("Schließen").clicked() {
            actions.push(UiAction::DismissError);
        }
    });
    // Technische Ursache mit anzeigen — ohne sie ist z. B. ein
    // Verbindungsfehler („Timeout") nicht von einem Protokollfehler
    // unterscheidbar und der Nutzer rät im Dunkeln.
    if let Some(details) = details {
        ui.label(RichText::new(details).small().weak());
    }
    actions
}

/// Statuszeile: FPS, aktives Werkzeug, Objektzahl und rechts die Version
/// (git-abgeleitet, wächst mit jedem Commit — siehe build.rs). Rein lesend —
/// Erfolgs-/Statusmeldungen laufen über die Toasts oben rechts.
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
