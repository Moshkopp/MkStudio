//! Lebenszyklus der eigenständigen Laser-Master-Detail-Verwaltung.

use crate::ui::{LaserManagerState, LaserManagerTab};

use super::App;

impl App {
    pub fn open_laser_manager(&mut self, create_new: bool) {
        let profile = if create_new {
            studio_core::LaserProfile::default()
        } else {
            self.laser_backend
                .active_profile()
                .cloned()
                .unwrap_or_default()
        };
        let selected_id = (!profile.id.is_empty()).then(|| profile.id.clone());
        self.laser_manager = Some(LaserManagerState {
            selected_id,
            draft: profile,
            is_new: create_new,
            tab: LaserManagerTab::Grunddaten,
            serial_ports: Vec::new(),
            serial_ports_error: None,
            machine_settings: Vec::new(),
            machine_dirty: Default::default(),
            machine_confirm_write: false,
            machine_read_rx: None,
            machine_write_count: None,
            axis_cal: Default::default(),
            axis_cal_clear_inputs: None,
            axis_cal_pending: None,
            axis_cal_rx: None,
        });
        self.laser_manager_refresh_serial_ports();
    }

    pub fn laser_manager_refresh_serial_ports(&mut self) {
        let Some(state) = self.laser_manager.as_mut() else {
            return;
        };
        match studio_application::available_serial_ports() {
            Ok(ports) => {
                state.serial_ports = ports;
                state.serial_ports_error = None;
            }
            Err(error) => {
                state.serial_ports.clear();
                state.serial_ports_error = Some(error.to_string());
            }
        }
    }

    pub fn laser_manager_select(&mut self, id: &str) {
        let Some(profile) = self
            .laser_backend
            .registry
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return;
        };
        if let Some(st) = self.laser_manager.as_mut() {
            st.selected_id = Some(id.into());
            st.draft = profile;
            st.is_new = false;
            st.tab = LaserManagerTab::Grunddaten;
            st.machine_settings.clear();
            st.machine_dirty.clear();
            st.axis_cal.clear();
            st.axis_cal_pending = None;
            st.axis_cal_rx = None;
            st.machine_read_rx = None;
            st.machine_write_count = None;
            st.machine_confirm_write = false;
        }
    }

    pub fn laser_manager_new(&mut self) {
        if let Some(st) = self.laser_manager.as_mut() {
            st.selected_id = None;
            st.draft = Default::default();
            st.is_new = true;
            st.tab = LaserManagerTab::Grunddaten;
            st.machine_settings.clear();
            st.machine_dirty.clear();
            st.axis_cal.clear();
            st.axis_cal_pending = None;
            st.axis_cal_rx = None;
            st.machine_read_rx = None;
            st.machine_write_count = None;
            st.machine_confirm_write = false;
        }
    }

    pub fn laser_manager_save(&mut self) {
        let Some((profile, was_new)) = self
            .laser_manager
            .as_ref()
            .map(|st| (st.draft.clone(), st.is_new))
        else {
            return;
        };
        let original_id = profile.id.clone();
        let was_connected = self.laser_backend.is_connected();
        // Verbindungs- oder achsenrelevante Änderungen trennen den Treiber.
        self.laser_hold_cancel();
        if let Err(error) = self.laser_backend.save_profile(profile) {
            self.app_error = Some(error);
            return;
        }
        let saved = if was_new {
            self.laser_backend.registry.profiles.last().cloned()
        } else {
            self.laser_backend
                .registry
                .profiles
                .iter()
                .find(|profile| profile.id == original_id)
                .cloned()
        };
        if let (Some(st), Some(profile)) = (self.laser_manager.as_mut(), saved) {
            st.selected_id = Some(profile.id.clone());
            st.draft = profile;
            st.is_new = false;
        }
        self.apply_active_laser_workspace();
        self.hub_runtime.configure(
            &self.ui_settings,
            &self.laser_backend.registry,
            self.material_service.library(),
        );
        // Hat das Speichern die Verbindung beendet (verbindungsrelevante
        // Änderung), muss auch der Hub-Lease weg — sonst hält dieser
        // Arbeitsplatz das Gerät hub-seitig weiter besetzt.
        if was_connected && !self.laser_backend.is_connected() {
            self.hub_runtime.release_lease();
            self.toasts
                .success("Verbindungsdaten geändert — Laser getrennt.");
        }
        self.toasts.success("Laser-Profil gespeichert.");
    }

    pub fn laser_manager_delete(&mut self) {
        let Some(id) = self
            .laser_manager
            .as_ref()
            .and_then(|st| st.selected_id.clone())
        else {
            return;
        };
        let was_connected = self.laser_backend.is_connected();
        self.laser_hold_cancel();
        if let Err(error) = self.laser_backend.delete_profile(&id) {
            self.app_error = Some(error);
            return;
        }
        if was_connected && !self.laser_backend.is_connected() {
            self.hub_runtime.release_lease();
        }
        self.hub_runtime.configure(
            &self.ui_settings,
            &self.laser_backend.registry,
            self.material_service.library(),
        );
        if let Some(profile) = self.laser_backend.registry.profiles.first().cloned() {
            self.laser_manager_select(&profile.id);
        } else {
            self.laser_manager_new();
        }
        self.toasts.success("Laser-Profil gelöscht.");
    }

    /// Stößt das Auslesen der Maschinenparameter auf dem Geräte-Worker an. Der
    /// Ruida antwortet je Register einzeln; synchron stünde die Oberfläche
    /// dabei mehrere Sekunden still. `poll_machine_read` holt das Ergebnis ab.
    pub fn laser_manager_machine_read(&mut self) {
        self.activate_managed_laser();
        match self.laser_backend.read_machine_settings_async() {
            Ok(receiver) => {
                self.hub_runtime
                    .set_lease_usage(studio_application::LeaseUsage::Unknown);
                if let Some(st) = self.laser_manager.as_mut() {
                    st.machine_read_rx = Some(receiver);
                }
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Holt einen laufenden Lesevorgang ab. Pro Frame aufzurufen; `true`,
    /// sobald ein Ergebnis eingetroffen ist.
    pub fn poll_machine_read(&mut self) -> bool {
        let Some(result) = self
            .laser_manager
            .as_ref()
            .and_then(|st| st.machine_read_rx.as_ref())
            .and_then(|rx| rx.try_recv().ok())
        else {
            return false;
        };
        // War es ein Schreibvorgang, meldet der Toast die geschriebenen Register
        // statt der gelesenen Gesamtzahl.
        let written = self.laser_manager.as_mut().and_then(|st| {
            st.machine_read_rx = None;
            st.machine_write_count.take()
        });
        match result {
            Ok(values) => {
                let count = values.len();
                if let Some(st) = self.laser_manager.as_mut() {
                    st.machine_settings = values;
                    st.machine_dirty.clear();
                    st.machine_confirm_write = false;
                }
                self.toasts.success(match written {
                    Some(count) => format!("{count} Ruida-Register geschrieben und bestätigt."),
                    None => format!("{count} Ruida-Register gelesen."),
                });
            }
            Err(error) => self.app_error = Some(error),
        }
        self.hub_runtime
            .set_lease_usage(studio_application::LeaseUsage::Idle);
        true
    }

    /// Stößt das Schreiben der geänderten Register auf dem Geräte-Worker an.
    /// Ergebnis über `poll_machine_read` — geschrieben wird mit Gegenlesen,
    /// geliefert wird also ebenfalls ein frischer Parametersatz.
    pub fn laser_manager_machine_write(&mut self) {
        self.activate_managed_laser();
        let changes: Vec<_> = self
            .laser_manager
            .as_ref()
            .map(|st| st.machine_dirty.iter().map(|(&a, &v)| (a, v)).collect())
            .unwrap_or_default();
        if changes.is_empty() {
            return;
        }
        let count = changes.len();
        match self.laser_backend.write_machine_settings_async(changes) {
            Ok(receiver) => {
                self.hub_runtime
                    .set_lease_usage(studio_application::LeaseUsage::Unknown);
                if let Some(st) = self.laser_manager.as_mut() {
                    st.machine_read_rx = Some(receiver);
                    st.machine_write_count = Some(count);
                }
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Stößt die Achskalibrierung auf dem Geräte-Worker an (ADR 0022 §D). Das
    /// Schreiben dauert am Ruida mehrere Sekunden; es läuft deshalb im
    /// Hintergrund, damit die Oberfläche bedienbar bleibt und der Spinner
    /// überhaupt gezeichnet werden kann. Das Ergebnis holt
    /// `poll_axis_calibration` ab.
    pub fn laser_calibrate_axis(
        &mut self,
        axis: studio_core::MachineAxis,
        target_mm: f64,
        measured_mm: f64,
    ) {
        self.activate_managed_laser();
        match self
            .laser_backend
            .calibrate_axis_steps_async(axis, target_mm, measured_mm)
        {
            Ok(receiver) => {
                self.hub_runtime
                    .set_lease_usage(studio_application::LeaseUsage::Unknown);
                if let Some(st) = self.laser_manager.as_mut() {
                    st.axis_cal_pending = Some(axis);
                    st.axis_cal_rx = Some(receiver);
                }
            }
            Err(error) => {
                self.app_error = Some(error);
                if let Some(st) = self.laser_manager.as_mut() {
                    st.axis_cal_pending = None;
                }
            }
        }
    }

    /// Holt das Ergebnis einer laufenden Achskalibrierung ab. Pro Frame
    /// aufzurufen; `true`, sobald ein Ergebnis eingetroffen ist.
    pub fn poll_axis_calibration(&mut self) -> bool {
        let Some(result) = self
            .laser_manager
            .as_ref()
            .and_then(|st| st.axis_cal_rx.as_ref())
            .and_then(|rx| rx.try_recv().ok())
        else {
            return false;
        };
        let axis = self
            .laser_manager
            .as_ref()
            .and_then(|st| st.axis_cal_pending);
        if let Some(st) = self.laser_manager.as_mut() {
            // In jedem Fall aufräumen — bliebe der Spinner nach einem Fehler
            // stehen, wäre die Kalibrierung dauerhaft blockiert.
            st.axis_cal_rx = None;
            st.axis_cal_pending = None;
        }
        match result {
            Ok(calibration) => {
                if let (Some(st), Some(axis)) = (self.laser_manager.as_mut(), axis) {
                    st.machine_dirty.clear();
                    // Der Worker hat nach dem Schreiben gegengelesen: „Aktuell"
                    // zeigt damit den neuen Wert, ohne dass der UI-Thread ein
                    // zweites Mal blockierend alle Register abfragt.
                    st.machine_settings = calibration.settings;
                    // Die Messung ist verbraucht: nach dem Schreiben gehört sie
                    // zum alten Stand. Stehen bleiben dürfte sie nicht — ein
                    // zweiter Klick würde den bereits korrigierten Wert erneut
                    // mit demselben Verhältnis skalieren.
                    st.axis_cal.remove(&axis);
                    st.axis_cal_clear_inputs = Some(axis);
                }
                self.toasts.success(format!(
                    "Schrittlänge kalibriert: {:.4} µm pro Schritt.",
                    calibration.step_length
                ));
            }
            Err(error) => self.app_error = Some(error),
        }
        self.hub_runtime
            .set_lease_usage(studio_application::LeaseUsage::Idle);
        true
    }

    /// Öffnet den Rotary-Dialog mit den Werten des aktiven Lasers.
    pub fn open_rotary_dialog(&mut self) {
        let rotary = self
            .laser_backend
            .active_profile()
            .and_then(|profile| profile.rotary);
        self.rotary_dialog = Some(crate::ui::RotaryDialogState::from_profile(rotary));
    }

    /// Übernimmt die Rotary-Einstellung ins aktive Profil.
    pub fn apply_rotary(&mut self, rotary: studio_core::Rotary) {
        let Some(mut profile) = self.laser_backend.active_profile().cloned() else {
            return;
        };
        profile.rotary = Some(rotary);
        // Speichern kann trennen (Rotary steckt im Treiber): vorher stoppen.
        self.laser_hold_cancel();
        if let Err(error) = self.laser_backend.save_profile(profile) {
            self.app_error = Some(error);
            return;
        }
        self.rotary_dialog = None;
        self.toasts.success(if rotary.active {
            "Rotary aktiv."
        } else {
            "Rotary ausgeschaltet."
        });
    }

    /// Schreibt Rotary-Modus, Pulse und Durchmesser in den Controller.
    ///
    /// Beim Ruida rechnet die Firmware die Drehung selbst (ADR 0023 §B); Studio
    /// setzt deshalb nur diese drei Register und rechnet die Bewegung NICHT
    /// zusätzlich um.
    pub fn rotary_write_controller(&mut self, rotary: studio_core::Rotary) {
        match self.laser_backend.configure_rotary_async(rotary) {
            Ok(receiver) => {
                self.rotary_read_rx = Some(receiver);
                self.rotary_wrote = Some(());
                if let Some(st) = self.rotary_dialog.as_mut() {
                    st.reading = true;
                }
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Liest die Rotary-Register frisch aus dem Controller (nur zur Anzeige).
    pub fn rotary_read_controller(&mut self) {
        match self.laser_backend.read_machine_settings_async() {
            Ok(receiver) => {
                self.rotary_read_rx = Some(receiver);
                if let Some(st) = self.rotary_dialog.as_mut() {
                    st.reading = true;
                }
            }
            Err(error) => {
                self.app_error = Some(error);
                if let Some(st) = self.rotary_dialog.as_mut() {
                    st.reading = false;
                }
            }
        }
    }

    /// Holt gelesene Rotary-Register ab. Pro Frame aufzurufen.
    pub fn poll_rotary_read(&mut self) -> bool {
        let Some(result) = self
            .rotary_read_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        else {
            return false;
        };
        self.rotary_read_rx = None;
        match result {
            Ok(values) => {
                let find = |key: &str| {
                    values
                        .iter()
                        .find(|setting| setting.key == key)
                        .and_then(|setting| setting.value())
                };
                // rotary_enable teilt sich ein Register mit anderen Schaltern
                // (bit_mask). Ohne Maskierung meldete jedes andere gesetzte Bit
                // fälschlich „Rotary aktiv".
                let enabled = values
                    .iter()
                    .find(|setting| setting.key == "rotary_enable")
                    .and_then(|setting| {
                        let mask = setting.bit_mask.unwrap_or(i64::MAX);
                        setting.raw.map(|raw| raw & mask != 0)
                    });
                if let Some(st) = self.rotary_dialog.as_mut() {
                    st.controller = Some(crate::ui::ControllerRotary {
                        enabled,
                        pulses_per_rot: find("pulses_per_rot"),
                        diameter_mm: find("rotary_diameter"),
                    });
                    st.machine_settings = values;
                    st.reading = false;
                }
                if self.rotary_wrote.take().is_some() {
                    self.toasts
                        .success("Rotary-Register geschrieben und bestätigt.");
                }
            }
            Err(error) => {
                self.app_error = Some(error);
                if let Some(st) = self.rotary_dialog.as_mut() {
                    st.reading = false;
                }
            }
        }
        true
    }

    fn activate_managed_laser(&mut self) {
        if let Some(id) = self
            .laser_manager
            .as_ref()
            .and_then(|state| state.selected_id.clone())
        {
            // set_active kann trennen (anderes Profil). Ein laufender Dauerlauf
            // muss vorher gestoppt sein, solange die Verbindung noch steht —
            // danach ist HoldStop ein No-op. Idempotent, kostet sonst nichts.
            self.laser_hold_cancel();
            self.laser_backend.set_active(&id);
            self.apply_active_laser_workspace();
        }
    }
}
