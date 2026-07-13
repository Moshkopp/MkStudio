//! Laser-Gerätedienst (ADR 0011, Phase 6): Registry laden/speichern, den aktiven
//! Treiber lazy bauen, Job-Aktionen ausführen, exportieren, jog/home. Koordiniert
//! die Treiber-Lebenszyklen; die UI erzeugt nie selbst einen Treiber.
//!
//! Fehler werden als stabiler [`AppError`] gemeldet. Erfolgsrückmeldungen des
//! Treibers (z. B. „Job gesendet") bleiben nutzerlesbare Strings.

use luxifer_core::{
    Anchor, Connection, DriverKind, JobAction, JobParams, JobPlan, LaserProfile, LaserRegistry,
    Layer, MachineDriver, Shape, StartMode,
};

use crate::AppError;

/// Ob eine Job-Aktion eine offene Geräteverbindung braucht. Kompilieren/
/// Export laufen ohne Gerät.
fn needs_connection(a: JobAction) -> bool {
    !matches!(a, JobAction::ExportFile)
}

/// Verbindungsziel aus dem Profil: IP (Netz) bzw. Gerätepfad (Seriell).
fn connection_target(profile: &LaserProfile) -> String {
    match &profile.connection {
        Connection::Netz { ip, .. } => ip.clone(),
        Connection::Seriell { port, .. } => port.clone(),
    }
}

/// Baut den passenden Treiber aus einem Profil.
fn driver_for(profile: &LaserProfile) -> Box<dyn MachineDriver + Send> {
    match profile.kind {
        DriverKind::Ruida => Box::new(luxifer_driver_ruida::RuidaDriver::from_profile(profile)),
        _ => Box::new(luxifer_driver_grbl::GrblDriver::default()),
    }
}

/// Hält die Laser-Registry und den (lazy gebauten) aktiven Treiber.
pub struct LaserService {
    pub registry: LaserRegistry,
    driver: Option<Box<dyn MachineDriver + Send>>,
    driver_id: Option<String>,
    connected_id: Option<String>,
}

impl LaserService {
    /// Liest bekannte und rohe Maschinenregister eines Ruida-Controllers.
    pub fn read_machine_settings(
        &mut self,
    ) -> Result<Vec<luxifer_driver_ruida::RuidaMachineSetting>, AppError> {
        let profile = self
            .active_profile()
            .ok_or_else(|| AppError::new("no_active_laser", "Kein Laser aktiv."))?
            .clone();
        if profile.kind != DriverKind::Ruida {
            return Err(AppError::new(
                "machine_settings_unsupported",
                "Maschinendaten werden derzeit nur für Ruida unterstützt.",
            ));
        }
        self.driver = None;
        self.driver_id = None;
        let mut driver = luxifer_driver_ruida::RuidaDriver::from_profile(&profile);
        let target = connection_target(&profile);
        driver.connect(&target).map_err(|e| {
            AppError::wrap(
                "laser_connect",
                format!("Keine Verbindung zum Laser ({target})."),
                e.to_string(),
            )
        })?;
        driver.read_machine_settings().map_err(|e| {
            AppError::wrap(
                "machine_settings_read",
                "Maschinendaten lesen fehlgeschlagen.",
                e.to_string(),
            )
        })
    }

    /// Schreibt geprüfte Ruida-Rohwerte, commitet sie und liest anschließend
    /// der Bestätigung halber den gesamten Block erneut.
    pub fn write_machine_settings(
        &mut self,
        changes: &[(u16, i64)],
    ) -> Result<Vec<luxifer_driver_ruida::RuidaMachineSetting>, AppError> {
        let profile = self
            .active_profile()
            .ok_or_else(|| AppError::new("no_active_laser", "Kein Laser aktiv."))?
            .clone();
        if profile.kind != DriverKind::Ruida {
            return Err(AppError::new(
                "machine_settings_unsupported",
                "Maschinendaten werden derzeit nur für Ruida unterstützt.",
            ));
        }
        self.driver = None;
        self.driver_id = None;
        let mut driver = luxifer_driver_ruida::RuidaDriver::from_profile(&profile);
        let target = connection_target(&profile);
        driver.connect(&target).map_err(|e| {
            AppError::wrap(
                "laser_connect",
                format!("Keine Verbindung zum Laser ({target})."),
                e.to_string(),
            )
        })?;
        driver.write_machine_settings(changes).map_err(|e| {
            AppError::wrap(
                "machine_settings_write",
                "Maschinendaten schreiben fehlgeschlagen.",
                e.to_string(),
            )
        })?;
        driver.read_machine_settings().map_err(|e| AppError::wrap("machine_settings_verify", "Maschinendaten wurden geschrieben, konnten aber nicht zur Kontrolle gelesen werden.", e.to_string()))
    }

    pub fn load() -> Self {
        Self {
            registry: LaserRegistry::load(),
            driver: None,
            driver_id: None,
            connected_id: None,
        }
    }

    /// Dienst mit vorgegebener Registry (ohne Platten-I/O) — für Tests.
    #[cfg(test)]
    fn with_registry(registry: LaserRegistry) -> Self {
        Self {
            registry,
            driver: None,
            driver_id: None,
            connected_id: None,
        }
    }

    pub fn active_profile(&self) -> Option<&LaserProfile> {
        self.registry.active()
    }

    pub fn set_active(&mut self, id: &str) {
        if self.registry.set_active(id) {
            let _ = self.registry.save();
            self.disconnect();
            self.driver = None; // beim nächsten Zugriff neu bauen
            self.driver_id = None;
        }
    }

    /// Legt ein neues Profil an oder aktualisiert ein bestehendes (nach ID).
    pub fn save_profile(&mut self, mut profile: LaserProfile) {
        if profile.id.is_empty() {
            let millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            profile.id = format!("laser-{millis}");
            self.registry.add(profile);
        } else if !self.registry.update(profile.clone()) {
            self.registry.add(profile);
        }
        let _ = self.registry.save();
        self.driver = None;
        self.driver_id = None;
        self.connected_id = None;
    }

    pub fn delete_profile(&mut self, id: &str) {
        self.registry.remove(id);
        let _ = self.registry.save();
        self.driver = None;
        self.driver_id = None;
        self.connected_id = None;
    }

    /// Ersetzt die lokale Registry nach einer ausdrücklich gewählten
    /// Sicherungs-Wiederherstellung und verwirft den dazu nicht mehr passenden
    /// lazy Treiber.
    pub fn restore_registry(&mut self, registry: LaserRegistry) -> Result<(), AppError> {
        registry.save().map_err(|error| {
            AppError::new(
                "laser_registry_write",
                format!("Laserprofile speichern fehlgeschlagen: {error}"),
            )
        })?;
        self.registry = registry;
        self.driver = None;
        self.driver_id = None;
        self.connected_id = None;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.connected_id.is_some()
            && self.connected_id.as_deref() == self.registry.active_id.as_deref()
    }

    pub fn active_uses_network(&self) -> bool {
        self.active_profile()
            .is_some_and(|profile| matches!(profile.connection, Connection::Netz { .. }))
    }

    pub fn connect(&mut self) -> Result<(), AppError> {
        let profile = self
            .active_profile()
            .cloned()
            .ok_or_else(|| AppError::new("no_active_laser", "Kein Laser aktiv."))?;
        self.with_driver(false, |driver| {
            let target = connection_target(&profile);
            driver.connect(&target).map_err(|error| {
                AppError::wrap(
                    "laser_connect",
                    format!("Keine Verbindung zum Laser ({target})."),
                    error.to_string(),
                )
            })
        })?;
        self.connected_id = Some(profile.id);
        Ok(())
    }

    pub fn disconnect(&mut self) {
        if let Some(driver) = self.driver.as_mut() {
            driver.disconnect();
        }
        self.connected_id = None;
    }

    /// Verfügbare Job-Aktionen des aktiven Treibers (fürs Panel-Grid). Ohne
    /// aktiven Treiber leer.
    pub fn actions(&mut self) -> Vec<JobAction> {
        self.with_driver(false, |d| Ok(d.actions()))
            .unwrap_or_default()
    }

    /// Stellt sicher, dass der Treiber zum aktiven Profil gebaut ist, und ruft f.
    /// `requires_connection` weist maschinenwirksame Aufrufe ab, solange der
    /// Nutzer nicht ausdrücklich verbunden hat.
    fn with_driver<T>(
        &mut self,
        requires_connection: bool,
        f: impl FnOnce(&mut Box<dyn MachineDriver + Send>) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let profile = self
            .registry
            .active()
            .ok_or_else(|| {
                AppError::new(
                    "no_active_laser",
                    "Kein Laser aktiv — in den Einstellungen anlegen.",
                )
            })?
            .clone();
        if self.driver_id.as_deref() != Some(profile.id.as_str()) || self.driver.is_none() {
            self.driver = Some(driver_for(&profile));
            self.driver_id = Some(profile.id.clone());
        }
        let driver = self.driver.as_mut().unwrap();
        if requires_connection && self.connected_id.as_deref() != Some(profile.id.as_str()) {
            return Err(AppError::new(
                "laser_not_connected",
                "Laser ist nicht verbunden. Bitte zuerst ausdrücklich verbinden.",
            ));
        }
        f(driver)
    }

    /// Baut den JobPlan MIT Asset-Auflösung — Bild-Layer werden gerastert.
    /// Dieselbe Quelle wie die Vorschau (`EditorSession::job_preview`), damit
    /// die Vorschau nie etwas zeigt, das der echte Job nicht tut (und der Job
    /// nichts auslässt, was die Vorschau zeigt).
    fn plan(&self, shapes: &[Shape], layers: &[Layer]) -> JobPlan {
        let plan = JobPlan::from_shapes_with_assets(shapes, layers, crate::assets::resolve_luma);
        self.active_profile().map_or(plan.clone(), |profile| {
            plan.transformed_for_bed(profile.origin, profile.bed_mm)
        })
    }

    fn job_params(&self, start_mode: StartMode, anchor_idx: usize) -> JobParams {
        let anchor = Anchor::from_index(anchor_idx);
        JobParams {
            start_mode,
            anchor: self
                .active_profile()
                .map(|profile| profile.origin.transform_anchor(anchor))
                .unwrap_or(anchor),
        }
    }

    /// Führt eine Job-Aktion aus und gibt die Rückmeldung des Treibers zurück.
    pub fn run_action(
        &mut self,
        action: JobAction,
        shapes: &[Shape],
        layers: &[Layer],
        start_mode: StartMode,
        anchor_idx: usize,
    ) -> Result<String, AppError> {
        let plan = self.plan(shapes, layers);
        let jp = self.job_params(start_mode, anchor_idx);
        self.with_driver(needs_connection(action), |d| {
            d.run_action(action, &plan, layers, &jp).map_err(|e| {
                AppError::wrap(
                    "laser_action",
                    "Laser-Aktion fehlgeschlagen.",
                    e.to_string(),
                )
            })
        })
    }

    /// Kompiliert den Job und schreibt ihn in eine Datei (Ruida .rd / GRBL .gcode).
    pub fn export_to(
        &mut self,
        path: &std::path::Path,
        shapes: &[Shape],
        layers: &[Layer],
        start_mode: StartMode,
        anchor_idx: usize,
    ) -> Result<(), AppError> {
        let plan = self.plan(shapes, layers);
        let jp = self.job_params(start_mode, anchor_idx);
        // Export kompiliert nur — dafür braucht es kein erreichbares Gerät.
        let bytes = self.with_driver(false, |d| {
            d.compile_with(&plan, layers, &jp)
                .map_err(|e| AppError::wrap("laser_export", "Job-Kompilierung fehlgeschlagen.", e))
        })?;
        std::fs::write(path, bytes).map_err(|e| {
            AppError::wrap(
                "laser_export",
                "Datei schreiben fehlgeschlagen.",
                e.to_string(),
            )
        })
    }

    /// Jog: Kopf relativ bewegen; eine explizite Verbindung ist Voraussetzung.
    pub fn jog(&mut self, dx: f64, dy: f64, speed: f64) -> Result<(), AppError> {
        self.with_driver(true, |d| {
            d.jog(dx, dy, speed)
                .map_err(|e| AppError::wrap("laser_jog", "Jog fehlgeschlagen.", e.to_string()))
        })
    }

    pub fn home(&mut self, speed: f64) -> Result<(), AppError> {
        self.with_driver(true, |d| {
            d.home(speed)
                .map_err(|e| AppError::wrap("laser_home", "Home fehlgeschlagen.", e.to_string()))
        })
    }
}

#[cfg(test)]
mod tests;
