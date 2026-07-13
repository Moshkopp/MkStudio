//! GUI-Einstellungen-Workflow (ADR 0002): Dialog-Entwurf öffnen/übernehmen,
//! Ausschließlich softwareweite LuxiFer-Einstellungen (ADR 0002).

use crate::ui::{CharonTestStatus, SettingsDialogState, SettingsSection};
use luxifer_application::AppError;

use super::App;

impl App {
    /// Öffnet den Einstellungen-Dialog (Zahnrad) mit den aktuellen Werten.
    pub fn open_settings_dialog(&mut self) {
        self.settings_dialog = Some(SettingsDialogState {
            draft: self.ui_settings.clone(),
            section: SettingsSection::Oberflaeche,
            charon_status: self.charon_status.clone(),
            charon_sync_error: self.charon_sync_error.clone(),
        });
    }

    pub fn test_charon_connection(&mut self) {
        let Some(state) = self.settings_dialog.as_mut() else {
            return;
        };
        state.charon_status = match luxifer_application::connect_charon(
            &state.draft.charon_url,
            &state.draft.workplace_id,
            &state.draft.workplace,
        ) {
            Ok(connection) => CharonTestStatus::Connected(connection),
            Err(error) => CharonTestStatus::Failed(error.message().to_string()),
        };
    }

    /// Übernimmt den GUI-Entwurf: klemmen, speichern, anwenden. Bei Erfolg true
    /// (Dialog schließen); bei Schreibfehler bleibt der Dialog offen und der
    /// Fehler erscheint im zentralen Kanal.
    pub fn commit_settings_dialog(&mut self) -> bool {
        let Some(st) = self.settings_dialog.as_ref() else {
            return false;
        };
        let mut draft = st.draft.clone();
        draft.sanitize();
        if let Err(error) = draft.save() {
            self.app_error = Some(AppError::new(
                "settings_write",
                format!("Einstellungen speichern fehlgeschlagen: {error}"),
            ));
            return false;
        }
        // Rasterweite steckt im gecachten Basis-Vertexpuffer → neu aufbauen.
        if draft.grid_size_mm != self.ui_settings.grid_size_mm {
            self.renderer.invalidate_scene();
        }
        self.ui_settings = draft;
        self.charon_runtime.configure(&self.ui_settings);
        self.toasts.success("Einstellungen gespeichert.");
        true
    }

    /// Übernimmt das jüngste Ergebnis des Hintergrund-Heartbeats in den
    /// sichtbaren Anwendungszustand. Netzwerkzugriff findet nie hier statt.
    pub fn poll_charon(&mut self) -> bool {
        let Some(result) = self.charon_runtime.try_result() else {
            return false;
        };
        self.charon_status = match result {
            super::charon::CharonWorkerResult::Connected(connection, sync) => {
                match sync {
                    Ok(report) => {
                        self.charon_sync_error = None;
                        if report.uploaded > 0 {
                            self.toasts.success(format!(
                                "{} Projektrevision(en) an Charon übertragen.",
                                report.uploaded
                            ));
                        }
                        if report.received > 0 {
                            self.toasts.success(format!(
                                "{} neue Projektrevision(en) von Charon empfangen.",
                                report.received
                            ));
                        }
                    }
                    Err(message) => self.charon_sync_error = Some(message),
                }
                CharonTestStatus::Connected(connection)
            }
            super::charon::CharonWorkerResult::Failed(message) => {
                self.charon_sync_error = None;
                CharonTestStatus::Failed(message)
            }
            super::charon::CharonWorkerResult::Disabled => {
                self.charon_sync_error = None;
                CharonTestStatus::Idle
            }
        };
        if let Some(dialog) = self.settings_dialog.as_mut() {
            dialog.charon_status = self.charon_status.clone();
            dialog.charon_sync_error = self.charon_sync_error.clone();
        }
        true
    }
}
