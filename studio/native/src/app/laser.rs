use super::App;

/// Eine im Dauerlauf gehaltene Achse samt Richtung (Watchdog-Wunsch). Die UI
/// meldet sie pro Frame; die App gleicht sie gegen den laufenden Zustand ab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoldJog {
    pub axis: studio_core::MachineAxis,
    pub dir: studio_core::AxisDir,
}

/// Live gelesener Maschinen-Anzeigestand (ADR 0020): der zuletzt erfolgreich
/// gelesene Wert oder ein sichtbarer Fehler-/Unbekannt-Zustand. Native erfindet
/// keine Position und behält nach einem Fehler keinen veralteten Wert ohne
/// Kennzeichnung.
#[derive(Default)]
pub struct LaserLiveState {
    /// Kopfposition in absoluten Maschinen-mm (zuletzt erfolgreich gelesen).
    pub head: Option<(f64, f64)>,
    /// Z-/U-Achsenposition (mm), sofern der Treiber sie liefert.
    pub pos_z: Option<f64>,
    pub pos_u: Option<f64>,
    /// Rotary läuft klassisch über Y (`rotary_enable`, echter Controller-Zustand).
    /// Z/U-Verfügbarkeit steht dagegen im Profil, nicht hier (ADR 0021 §A).
    pub rotary_on_y: bool,
    /// Sichtbare Begründung, warum keine (frische) Kopfposition vorliegt.
    pub head_note: Option<String>,
    /// Benutzerursprung (nur bei angewählter Referenz gelesen).
    pub user_origin: Option<(f64, f64)>,
    pub user_origin_note: Option<String>,
    /// Ob der Controller zuletzt einen laufenden Job gemeldet hat.
    pub is_running: bool,
    /// Zeitpunkt des letzten Leseversuchs (Poll-Drossel).
    last_poll: Option<std::time::Instant>,
    /// Nach einem Fehler wird seltener gelesen, damit Timeouts das UI nicht
    /// dauerhaft ausbremsen.
    error_backoff: bool,
    /// Genau eine laufende Geräteabfrage; weitere Frames starten keine zweite.
    pending: Option<std::sync::mpsc::Receiver<studio_application::LaserLiveRead>>,
    /// Aktives Profil beim Start der Abfrage, gegen verspätete Ergebnisse.
    pending_profile: Option<String>,
}

impl LaserLiveState {
    /// Poll-Intervall: bei Fehlern zurückhaltend, sonst zügig.
    fn interval(&self) -> std::time::Duration {
        if self.error_backoff {
            std::time::Duration::from_secs(5)
        } else {
            std::time::Duration::from_millis(1000)
        }
    }
}

/// Weltkoordinaten (Editor-mm) der drei Laser-Fadenkreuze (ADR 0020 §B).
pub struct LaserMarkers {
    /// „Start": gewählter 3×3-Jobanker auf der Job-BBox der aktiven Inhalte.
    pub start: Option<[f64; 2]>,
    /// „Ursprung": Bezugspunkt der gewählten Startreferenz.
    pub origin: Option<[f64; 2]>,
    /// „Kopf": live gelesene Position des Laserkopfs.
    pub head: Option<[f64; 2]>,
}

impl App {
    /// Übernimmt den Arbeitsbereich des aktiven Maschinenprofils in Canvas und
    /// Kamera. Das Profil ist damit die Quelle für Laser-Bett und Job-Grenzen.
    pub(super) fn apply_active_laser_workspace(&mut self) {
        let Some(profile) = self.laser_backend.active_profile() else {
            return;
        };
        let bed = profile.bed_mm;
        if !bed.0.is_finite() || !bed.1.is_finite() || bed.0 <= 0.0 || bed.1 <= 0.0 {
            return;
        }
        self.session.bed_w_mm = bed.0;
        self.session.bed_h_mm = bed.1;
        self.canvas.cam.fit_bbox([0.0, 0.0, bed.0, bed.1], 0.85);
        self.renderer.invalidate_scene();
    }

    /// Liefert die vollständige oder auf die Auswahl beschränkte Job-Eingabe.
    fn laser_shapes(&self) -> (Vec<studio_core::Shape>, Vec<studio_core::Layer>) {
        let shapes = if self.laser.selection_only {
            self.session
                .selected
                .iter()
                .filter_map(|&index| self.session.shapes.get(index).cloned())
                .collect()
        } else {
            self.session.shapes.clone()
        };
        (shapes, self.session.layers.clone())
    }

    pub fn laser_select(&mut self, id: &str) {
        self.laser_backend.set_active(id);
        self.apply_active_laser_workspace();
        // Anzeigenwerte gehören zum vorherigen Gerät; Referenz pro Laser
        // wiederherstellen (ADR 0020 §E).
        self.laser_live = Default::default();
        self.restore_laser_start_reference();
    }

    pub fn laser_connect(&mut self) {
        if self.ui_settings.hub_enabled
            && self.laser_backend.active_uses_network()
            && !matches!(
                self.hub_status,
                crate::ui::HubTestStatus::Connected(_) | crate::ui::HubTestStatus::Syncing(_)
            )
        {
            self.laser_uncoordinated_confirm = true;
            return;
        }
        if self.ui_settings.hub_enabled && self.laser_backend.active_uses_network() {
            let Some((controller_id, controller_name)) = self.laser_backend.active_lease_identity()
            else {
                return;
            };
            self.laser_lease_pending = true;
            self.hub_runtime
                .acquire_lease(controller_id, controller_name, false);
            self.toasts.success("Ruida-Lease wird angefordert …");
            return;
        }
        self.laser_connect_now();
    }

    pub fn laser_connect_uncoordinated(&mut self) {
        self.laser_uncoordinated_confirm = false;
        self.laser_connect_now();
    }

    fn laser_connect_now(&mut self) {
        match self.laser_backend.connect() {
            Ok(()) => {
                self.toasts.success("Laser verbunden.");
                self.request_laser_status_refresh();
            }
            Err(error) => {
                self.hub_runtime.release_lease();
                self.app_error = Some(error);
            }
        }
    }

    pub fn laser_disconnect(&mut self) {
        self.laser_backend.disconnect();
        self.hub_runtime.release_lease();
        self.hub_runtime
            .set_lease_usage(studio_application::LeaseUsage::Idle);
        self.toasts.success("Laser getrennt.");
    }

    pub fn laser_run(&mut self, action: studio_core::JobAction) {
        // Die „Ursprung"-Kachel fährt den Bezugspunkt der gewählten
        // Startreferenz an — nicht mehr fest den controllerseitigen
        // Benutzerursprung.
        if action == studio_core::JobAction::GoOrigin {
            self.laser_goto_reference();
            return;
        }
        let (shapes, layers) = self.laser_shapes();
        let reference = self.laser.start_reference.clone();
        let anchor = self.laser.anchor;
        self.request_laser_status_refresh();
        match self
            .laser_backend
            .run_action(action, &shapes, &layers, &reference, anchor)
        {
            Ok(message) => {
                let usage = match action {
                    studio_core::JobAction::SendJob | studio_core::JobAction::StreamGcode => {
                        Some(studio_application::LeaseUsage::Running)
                    }
                    studio_core::JobAction::Pause => Some(studio_application::LeaseUsage::Paused),
                    studio_core::JobAction::Stop => Some(studio_application::LeaseUsage::Idle),
                    _ => None,
                };
                if let Some(usage) = usage {
                    self.hub_runtime.set_lease_usage(usage);
                }
                self.toasts.success(message)
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn force_laser_lease(&mut self) {
        self.laser_lease_force_confirm = None;
        let Some((controller_id, controller_name)) = self.laser_backend.active_lease_identity()
        else {
            return;
        };
        self.laser_lease_pending = true;
        self.hub_runtime
            .acquire_lease(controller_id, controller_name, true);
    }

    pub fn poll_laser_lease(&mut self) -> bool {
        let Some(result) = self.hub_runtime.try_lease_result() else {
            return false;
        };
        self.laser_lease_pending = false;
        match result {
            super::hub::LeaseWorkerResult::Acquired => self.laser_connect_now(),
            super::hub::LeaseWorkerResult::Denied(reply) => {
                if reply.force_required {
                    self.laser_lease_force_confirm = Some(reply);
                } else {
                    let holder = reply
                        .holder_name
                        .as_deref()
                        .unwrap_or("anderer Arbeitsplatz");
                    self.toasts.error(format!(
                        "Ruida ist durch {holder} belegt. Eine Übergabe wurde angefordert."
                    ));
                }
            }
            super::hub::LeaseWorkerResult::Released => {}
            super::hub::LeaseWorkerResult::ReleaseRequested => {
                self.laser_backend.disconnect();
                self.toasts
                    .success("Ruida-Verbindung an anderen Arbeitsplatz übergeben.");
            }
            super::hub::LeaseWorkerResult::Lost(message) => {
                self.laser_backend.disconnect();
                self.app_error = Some(studio_application::AppError::new(
                    "hub_lease_lost",
                    format!("Ruida-Lease verloren: {message}"),
                ));
            }
        }
        true
    }

    pub fn laser_export(&mut self) {
        let extension = match self
            .laser_backend
            .active_profile()
            .map(|profile| profile.kind)
        {
            Some(studio_core::DriverKind::Ruida) => "rd",
            _ => "gcode",
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("job.{extension}"))
            .save_file()
        else {
            return;
        };

        let (shapes, layers) = self.laser_shapes();
        let reference = self.laser.start_reference.clone();
        let anchor = self.laser.anchor;
        match self
            .laser_backend
            .export_to(&path, &shapes, &layers, &reference, anchor)
        {
            Ok(()) => self
                .toasts
                .success(format!("Exportiert: {}", path.display())),
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn laser_jog(&mut self, dx: f64, dy: f64) {
        if let Err(error) = self.laser_backend.jog(dx, dy, self.laser.jog_speed) {
            self.app_error = Some(error);
        }
        self.request_laser_status_refresh();
    }

    /// Jog-Speed für eine Achse: Z nutzt den eigenen, hart gedeckelten Wert
    /// (Gewindestange), alle anderen den gemeinsamen Jog-Speed.
    fn jog_speed_for(&self, axis: studio_core::MachineAxis) -> f64 {
        match axis {
            studio_core::MachineAxis::Z => {
                self.laser.z_jog_speed.min(crate::tools::Z_JOG_SPEED_MAX)
            }
            _ => self.laser.jog_speed,
        }
    }

    /// Schritt-Jog einer Zusatzachse (Tippen). Kein Status-Refresh (die Anzeige
    /// zieht über den regulären Poll nach).
    pub fn laser_jog_axis_step(
        &mut self,
        axis: studio_core::MachineAxis,
        dir: studio_core::AxisDir,
    ) {
        let speed = self.jog_speed_for(axis);
        let motion = studio_core::JogMotion::Step(self.laser.jog_step);
        if let Err(error) = self.laser_backend.jog_axis(axis, dir, motion, speed) {
            self.app_error = Some(error);
        }
    }

    /// Watchdog für den Achsen-Dauerlauf (ADR 0021). `wanted` ist der pro Frame
    /// gemeldete Halte-Wunsch der UI (gehaltene Achse+Richtung oder None). Ein
    /// `Some` startet/wechselt den Dauerlauf; ein `None` stoppt erst, wenn die
    /// Karenzzeit seit dem letzten `Some` verstrichen ist — so überbrücken
    /// einzelne Frames ohne Pointer-Info den Lauf, während echtes Loslassen
    /// (dauerhaft `None`) sicher stoppt.
    pub fn laser_hold_frame(&mut self, wanted: Option<HoldJog>) {
        const GRACE: std::time::Duration = std::time::Duration::from_millis(150);
        use studio_core::JogMotion::{HoldStart, HoldStop};

        match wanted {
            Some(next) => {
                self.laser_hold_seen = Some(std::time::Instant::now());
                if self.laser_hold == Some(next) {
                    return; // läuft bereits (gleiche Achse+Richtung)
                }
                if let Some(active) = self.laser_hold.take() {
                    let _ = self.laser_backend.jog_axis(
                        active.axis,
                        active.dir,
                        HoldStop,
                        self.jog_speed_for(active.axis),
                    );
                }
                let speed = self.jog_speed_for(next.axis);
                match self
                    .laser_backend
                    .jog_axis(next.axis, next.dir, HoldStart, speed)
                {
                    Ok(()) => self.laser_hold = Some(next),
                    Err(error) => self.app_error = Some(error),
                }
            }
            None => {
                let Some(active) = self.laser_hold else {
                    return;
                };
                if self.laser_hold_seen.is_some_and(|t| t.elapsed() < GRACE) {
                    return; // Karenz: Frame-Aussetzer überbrücken
                }
                let _ = self.laser_backend.jog_axis(
                    active.axis,
                    active.dir,
                    HoldStop,
                    self.jog_speed_for(active.axis),
                );
                self.laser_hold = None;
                self.laser_hold_seen = None;
            }
        }
    }

    /// Sofortiger Abbruch eines laufenden Achsen-Dauerlaufs (ohne Karenz) —
    /// z. B. beim Verlassen des Laser-Tabs. Idempotent.
    pub fn laser_hold_cancel(&mut self) {
        if let Some(active) = self.laser_hold.take() {
            let _ = self.laser_backend.jog_axis(
                active.axis,
                active.dir,
                studio_core::JogMotion::HoldStop,
                self.jog_speed_for(active.axis),
            );
        }
        self.laser_hold_seen = None;
    }

    pub fn laser_home(&mut self) {
        if let Err(error) = self.laser_backend.home(self.laser.jog_speed) {
            self.app_error = Some(error);
        }
        self.request_laser_status_refresh();
    }

    // --- Positionsanzeige, Startreferenz und Werkstück-Nullpunkte (ADR 0020) --

    /// Erzwingt beim nächsten Frame eine frische Statuslesung (nach Verbinden,
    /// Jog, Home, Anfahren und Job-Aktionen).
    pub(super) fn request_laser_status_refresh(&mut self) {
        self.laser_live.last_poll = None;
        self.laser_live.error_backoff = false;
    }

    /// Drosselt gepolltes Lesen von Kopfposition (und bei Bedarf
    /// Benutzerursprung) während einer aktiven Verbindung im Laser-Tab.
    /// Fehler ersetzen den Anzeigewert sichtbar, statt einen alten Stand
    /// unmarkiert stehen zu lassen.
    pub fn poll_laser_status(&mut self) -> bool {
        if !self.laser_backend.is_connected() {
            self.laser_live = LaserLiveState::default();
            return false;
        }

        if let Some(receiver) = self.laser_live.pending.as_ref() {
            match receiver.try_recv() {
                Ok(read) => {
                    self.laser_live.pending = None;
                    let requested_profile = self.laser_live.pending_profile.take();
                    let active_profile = self
                        .laser_backend
                        .active_profile()
                        .map(|profile| profile.id.as_str());
                    if requested_profile.as_deref() != active_profile {
                        return false;
                    }
                    match read.status {
                        Ok(status) => {
                            self.laser_live.head = Some((status.pos_x_mm, status.pos_y_mm));
                            self.laser_live.pos_z = status.pos_z_mm;
                            self.laser_live.pos_u = status.pos_u_mm;
                            self.laser_live.rotary_on_y = status.rotary_on_y;
                            self.laser_live.head_note = None;
                            self.laser_live.is_running = status.is_running;
                            self.laser_live.error_backoff = false;
                        }
                        Err(error) => {
                            self.laser_live.head = None;
                            self.laser_live.pos_z = None;
                            self.laser_live.pos_u = None;
                            self.laser_live.rotary_on_y = false;
                            self.laser_live.head_note = Some(error.message().to_owned());
                            self.laser_live.error_backoff = true;
                        }
                    }
                    if let Some(origin) = read.user_origin {
                        match origin {
                            Ok(origin) => {
                                self.laser_live.user_origin = Some(origin);
                                self.laser_live.user_origin_note = None;
                            }
                            Err(error) => {
                                self.laser_live.user_origin = None;
                                self.laser_live.user_origin_note = Some(error.message().to_owned());
                                self.laser_live.error_backoff = true;
                            }
                        }
                    } else {
                        self.laser_live.user_origin = None;
                        self.laser_live.user_origin_note = None;
                    }
                    return true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return false,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.laser_live.pending = None;
                    self.laser_live.pending_profile = None;
                    self.laser_live.error_backoff = true;
                }
            }
        }
        let due = self
            .laser_live
            .last_poll
            .map(|at| at.elapsed() >= self.laser_live.interval())
            .unwrap_or(true);
        if !due {
            return false;
        }
        self.laser_live.last_poll = Some(std::time::Instant::now());
        let include_origin =
            self.laser.start_reference == studio_core::StartReference::Benutzerursprung;
        match self.laser_backend.read_live_async(include_origin) {
            Ok(receiver) => {
                self.laser_live.pending = Some(receiver);
                self.laser_live.pending_profile = self
                    .laser_backend
                    .active_profile()
                    .map(|profile| profile.id.clone());
            }
            Err(error) => {
                self.laser_live.head = None;
                self.laser_live.head_note = Some(error.message().to_owned());
                self.laser_live.error_backoff = true;
                return true;
            }
        }
        false
    }

    /// Referenzkoordinate der aktuellen „Starten von"-Auswahl in absoluten
    /// Maschinen-mm — oder `None`, solange ein live zu lesender Wert fehlt.
    pub fn laser_reference_position(&self) -> Option<(f64, f64)> {
        use studio_core::StartReference as R;
        match &self.laser.start_reference {
            R::Absolut => Some((0.0, 0.0)),
            R::AktuellePosition => self.laser_live.head,
            R::Benutzerursprung => self.laser_live.user_origin,
            R::GespeicherterNullpunkt { id } => {
                let profile = self.laser_backend.active_profile()?;
                let origin = profile.saved_origin(id)?;
                Some((origin.x_mm, origin.y_mm))
            }
        }
    }

    /// Die drei Canvas-Fadenkreuze in Editor-Weltkoordinaten. Live-Maschinen-
    /// koordinaten werden über die `BedOrigin`-Abbildung des Profils in die
    /// links-oben orientierte Editor-Sicht gespiegelt (die Transformation ist
    /// selbstinvers) — dieselbe Abbildung wie für die Jobgeometrie.
    pub fn laser_canvas_markers(&self) -> LaserMarkers {
        let Some(profile) = self.laser_backend.active_profile() else {
            return LaserMarkers {
                start: None,
                origin: None,
                head: None,
            };
        };
        let bed = profile.bed_mm;
        let to_editor = |machine: (f64, f64)| -> [f64; 2] {
            let (x, y) = profile.origin.transform(machine.0, machine.1, bed);
            [x, y]
        };
        let reference = self.laser_reference_position().map(to_editor);
        let head = self.laser_live.head.map(to_editor);
        // „Start": immer der gewählte 3×3-Jobanker auf der Job-BBox — er
        // zeigt, wo AUF DEN OBJEKTEN der Job beginnt. Den Bezugspunkt der
        // Startreferenz zeigt das „Ursprung"-Fadenkreuz; beide auf die
        // Referenz zu legen machte den Startmarker informationslos
        // (ADR 0020 §B, revidiert).
        let start = self
            .session
            .job_anchor_marker(
                self.laser.selection_only,
                studio_core::Anchor::from_index(self.laser.anchor),
            )
            .map(|(x, y)| [x, y]);
        LaserMarkers {
            start,
            origin: reference,
            head,
        }
    }

    /// Setzt die Startreferenz, merkt sie pro Laserprofil in den lokalen
    /// GUI-Settings und liest bei Bedarf den Benutzerursprung frisch. Die
    /// bloße Auswahl löst KEINE Maschinenbewegung aus (ADR 0020 §E).
    pub fn laser_set_start_reference(&mut self, reference: studio_core::StartReference) {
        self.laser.start_reference = reference.clone();
        if let Some(profile) = self.laser_backend.active_profile() {
            self.ui_settings
                .laser_start_reference
                .insert(profile.id.clone(), reference);
            if let Err(error) = self.ui_settings.save() {
                log::error!("GUI-Settings speichern: {error}");
            }
        }
        self.request_laser_status_refresh();
    }

    /// Stellt die zuletzt verwendete Startreferenz des aktiven Lasers wieder
    /// her. Eine gemerkte, aber gelöschte Nullpunkt-ID bleibt sichtbar
    /// bestehen (Panel zeigt den fehlenden Bezug an) — kein stiller Fallback.
    pub(super) fn restore_laser_start_reference(&mut self) {
        let Some(profile) = self.laser_backend.active_profile() else {
            self.laser.start_reference = studio_core::StartReference::Absolut;
            return;
        };
        self.laser.start_reference = self
            .ui_settings
            .laser_start_reference
            .get(&profile.id)
            .cloned()
            .unwrap_or_default();
        self.request_laser_status_refresh();
    }

    /// „Ursprung"-Kachel: fährt den Bezugspunkt der gewählten Startreferenz
    /// laserfrei an. Nur diese ausdrückliche Aktion bewegt die Maschine.
    pub fn laser_goto_reference(&mut self) {
        let reference = self.laser.start_reference.clone();
        match self
            .laser_backend
            .goto_reference(&reference, self.laser.jog_speed)
        {
            Ok(message) => self.toasts.success(message),
            Err(error) => self.app_error = Some(error),
        }
        self.request_laser_status_refresh();
    }

    /// „Aktuelle Position als Nullpunkt speichern": liest die Kopfposition
    /// frisch vom Controller; erst nach erfolgreichem Lesen öffnet sich der
    /// Namensdialog (ADR 0020 §D). Gespeichert wird genau diese Position.
    /// Umbenennen und Löschen laufen in der Laser-Verwaltung.
    pub fn laser_save_origin_here(&mut self) {
        match self.laser_backend.read_plausible_position() {
            Ok(status) => {
                self.saved_origin_dialog = Some(crate::ui::SavedOriginDialogState {
                    name: String::new(),
                    position: (status.pos_x_mm, status.pos_y_mm),
                });
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Bestätigt den Nullpunkt-Namensdialog. `true` = Dialog schließen.
    pub fn commit_saved_origin_dialog(&mut self) -> bool {
        let Some(dialog) = self.saved_origin_dialog.clone() else {
            return true;
        };
        let (x, y) = dialog.position;
        match self.laser_backend.add_saved_origin(&dialog.name, x, y) {
            Ok(origin) => {
                // Hub-Worker mit der frischen Registry versorgen, damit
                // Arbeitsplatz-Backups die neue Liste enthalten.
                self.hub_runtime.configure(
                    &self.ui_settings,
                    &self.laser_backend.registry,
                    self.material_service.library(),
                );
                self.toasts
                    .success(format!("Nullpunkt „{}“ gespeichert.", origin.name));
                true
            }
            Err(error) => {
                // Dialog offen lassen: Name korrigieren statt Position verlieren.
                self.toasts.error(error.message().to_owned());
                false
            }
        }
    }

    // Die Laser-Profil-Verwaltung (öffnen/speichern/löschen) lebt in der
    // Laser-Sektion des Einstellungen-Dialogs — siehe app/settings.rs.
}
