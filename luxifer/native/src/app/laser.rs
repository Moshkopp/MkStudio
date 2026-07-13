use super::App;

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
    fn laser_shapes(&self) -> (Vec<luxifer_core::Shape>, Vec<luxifer_core::Layer>) {
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
    }

    pub fn laser_connect(&mut self) {
        if self.ui_settings.charon_enabled
            && self.laser_backend.active_uses_network()
            && !matches!(
                self.charon_status,
                crate::ui::CharonTestStatus::Connected(_)
            )
        {
            self.laser_uncoordinated_confirm = true;
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
            Ok(()) => self.toasts.success("Laser verbunden."),
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn laser_disconnect(&mut self) {
        self.laser_backend.disconnect();
        self.toasts.success("Laser getrennt.");
    }

    pub fn laser_run(&mut self, action: luxifer_core::JobAction) {
        let (shapes, layers) = self.laser_shapes();
        let start_mode = self.laser.start_mode;
        let anchor = self.laser.anchor;
        match self
            .laser_backend
            .run_action(action, &shapes, &layers, start_mode, anchor)
        {
            Ok(message) => self.toasts.success(message),
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn laser_export(&mut self) {
        let extension = match self
            .laser_backend
            .active_profile()
            .map(|profile| profile.kind)
        {
            Some(luxifer_core::DriverKind::Ruida) => "rd",
            _ => "gcode",
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("job.{extension}"))
            .save_file()
        else {
            return;
        };

        let (shapes, layers) = self.laser_shapes();
        let start_mode = self.laser.start_mode;
        let anchor = self.laser.anchor;
        match self
            .laser_backend
            .export_to(&path, &shapes, &layers, start_mode, anchor)
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
    }

    pub fn laser_home(&mut self) {
        if let Err(error) = self.laser_backend.home(self.laser.jog_speed) {
            self.app_error = Some(error);
        }
    }

    // Die Laser-Profil-Verwaltung (öffnen/speichern/löschen) lebt in der
    // Laser-Sektion des Einstellungen-Dialogs — siehe app/settings.rs.
}
