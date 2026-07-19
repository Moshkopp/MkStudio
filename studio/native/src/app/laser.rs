use super::App;

/// Live gelesener Maschinen-Anzeigestand (ADR 0020): der zuletzt erfolgreich
/// gelesene Wert oder ein sichtbarer Fehler-/Unbekannt-Zustand. Native erfindet
/// keine Position und behält nach einem Fehler keinen veralteten Wert ohne
/// Kennzeichnung.
#[derive(Default)]
pub struct LaserLiveState {
    /// Kopfposition in absoluten Maschinen-mm (zuletzt erfolgreich gelesen).
    pub head: Option<(f64, f64)>,
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
    pub fn poll_laser_status(&mut self) {
        if !self.laser_backend.is_connected() {
            self.laser_live = LaserLiveState::default();
            return;
        }
        let due = self
            .laser_live
            .last_poll
            .map(|at| at.elapsed() >= self.laser_live.interval())
            .unwrap_or(true);
        if !due {
            return;
        }
        self.laser_live.last_poll = Some(std::time::Instant::now());
        let capabilities = self.laser_backend.driver_capabilities();
        if !capabilities.position_read {
            self.laser_live.head = None;
            self.laser_live.head_note = Some("nicht unterstützt".into());
        } else {
            match self.laser_backend.read_status() {
                Ok(status) => {
                    self.laser_live.head = Some((status.pos_x_mm, status.pos_y_mm));
                    self.laser_live.head_note = None;
                    self.laser_live.is_running = status.is_running;
                    self.laser_live.error_backoff = false;
                }
                Err(error) => {
                    self.laser_live.head = None;
                    self.laser_live.head_note = Some(error.message().to_owned());
                    self.laser_live.error_backoff = true;
                }
            }
        }
        // Der Benutzerursprung wird nur bei angewählter Referenz gelesen —
        // nicht ohne Bedarf in jeder Ansicht (ADR 0020 §A).
        if self.laser.start_reference == studio_core::StartReference::Benutzerursprung {
            if !capabilities.user_origin_read {
                self.laser_live.user_origin = None;
                self.laser_live.user_origin_note = Some("nicht unterstützt".into());
            } else {
                match self.laser_backend.read_user_origin() {
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
            }
        } else {
            self.laser_live.user_origin = None;
            self.laser_live.user_origin_note = None;
        }
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
