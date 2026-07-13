use luxifer_core::state::AppState;

use super::App;
use crate::ui::PendingProjectAction;

impl App {
    pub fn defer_inbox_revision(&mut self, revision_id: &str) {
        match luxifer_application::defer_inbox_revision(revision_id) {
            Ok(()) => {
                self.refresh_project_inbox();
                self.toasts.success("Revision für später zurückgestellt.");
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn reconsider_inbox_revision(&mut self, revision_id: &str) {
        match luxifer_application::reconsider_inbox_revision(revision_id) {
            Ok(()) => {
                self.refresh_project_inbox();
                self.toasts
                    .success("Revision wieder zur Prüfung vorgemerkt.");
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn apply_inbox_revision(&mut self, revision_id: &str) {
        match luxifer_application::apply_inbox_revision(revision_id) {
            Ok(name) => {
                self.refresh_project_inbox();
                self.project_browser.show_inbox = false;
                self.project_browser.selected = Some(name.clone());
                self.project_browser.cached = None;
                self.toasts.success(format!("Projekt übernommen: {name}"));
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn refresh_project_inbox(&mut self) {
        match luxifer_application::list_inbox() {
            Ok(entries) => self.project_inbox = entries,
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn project_open(&mut self, name: &str) {
        if self.session.is_dirty() {
            self.pending_project = Some(PendingProjectAction::Open(name.to_string()));
        } else {
            self.do_project_open(name);
        }
    }

    /// Beginnt eine leere, ungespeicherte Arbeitsfläche. Ein bestehendes
    /// Projekt wird nur aus der Session gelöst; seine gespeicherten Dateien
    /// bleiben unverändert.
    pub fn project_new_blank(&mut self) {
        if self.session.is_dirty() {
            self.pending_project = Some(PendingProjectAction::Blank);
        } else {
            self.do_project_new_blank();
        }
    }

    fn do_project_new_blank(&mut self) {
        self.project.close();
        self.replace_editor_state(AppState::default());
        self.canvas.poly_pts.clear();
        self.canvas.bezier_nodes.clear();
        self.canvas.drag = crate::tools::Drag::None;
        self.project_browser.selected = None;
        self.project_browser.cached = None;
        self.view = crate::tools::View::Design;
        self.toasts.success("Neue leere Arbeitsfläche.");
    }

    pub fn confirm_pending_project(&mut self) {
        match self.pending_project.take() {
            Some(PendingProjectAction::Blank) => self.do_project_new_blank(),
            Some(PendingProjectAction::New { name, description }) => {
                self.do_project_new(&name, &description);
            }
            Some(PendingProjectAction::Open(name)) => self.do_project_open(&name),
            Some(PendingProjectAction::OpenVersion(id)) => self.do_project_open_version(&id),
            Some(PendingProjectAction::DeleteVersion(id)) => self.do_project_delete_version(&id),
            None => {}
        }
    }

    pub fn request_close(&mut self) -> bool {
        if self.session.is_dirty() {
            self.close_pending = true;
            self.window.request_redraw();
            false
        } else {
            true
        }
    }

    pub fn confirm_close(&mut self) {
        self.close_pending = false;
        self.should_exit = true;
    }

    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    fn replace_editor_state(&mut self, state: AppState) {
        self.session.replace_state(state);
        self.refresh_accent();
        self.image_dirty = true;
        self.renderer.invalidate_scene();
        self.fit_all();
    }

    fn do_project_open(&mut self, name: &str) {
        match self.project.open(name) {
            Ok(state) => {
                self.replace_editor_state(state);
                self.toasts.success(format!("Geöffnet: {name}"));
                self.view = crate::tools::View::Design;
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn project_open_version(&mut self, id: &str) {
        if self.session.is_dirty() {
            self.pending_project = Some(PendingProjectAction::OpenVersion(id.to_string()));
        } else {
            self.do_project_open_version(id);
        }
    }

    fn do_project_open_version(&mut self, id: &str) {
        match self.project.open_version(id) {
            Ok(state) => {
                self.replace_editor_state(state);
                self.toasts.success("Version geladen.");
                self.view = crate::tools::View::Design;
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn project_delete_version(&mut self, id: &str) {
        let deletes_current = self.project.current_version_id() == Some(id);
        if deletes_current && self.session.is_dirty() {
            self.pending_project = Some(PendingProjectAction::DeleteVersion(id.to_string()));
            return;
        }
        self.do_project_delete_version(id);
    }

    fn do_project_delete_version(&mut self, id: &str) {
        match self.project.delete_version(id) {
            Ok(Some(state)) => {
                self.replace_editor_state(state);
                self.toasts
                    .success("Version gelöscht — vorherige Version geladen.");
            }
            Ok(None) => self.toasts.success("Version gelöscht."),
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn project_rename(&mut self, from: &str, to: &str) {
        match self.project.rename(from, to) {
            Ok(()) => {
                let to = to.trim();
                if self.project_browser.selected.as_deref() == Some(from) {
                    self.project_browser.selected = Some(to.to_string());
                }
                self.toasts.success(format!("Umbenannt: {from} → {to}"));
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Legt das Projekt aus dem aktuellen Canvas an. Der Guard greift nur, wenn
    /// ein ANDERES offenes Projekt ungespeicherte Änderungen trägt (siehe
    /// `commit_project_save_dialog`) — ohne offenes Projekt geht nichts
    /// verloren, der aktuelle Stand wird gerade zum neuen Projekt.
    fn do_project_new(&mut self, name: &str, description: &str) -> bool {
        match self
            .project
            .new_project(self.session.state(), name, description)
        {
            Ok(()) => {
                self.session.mark_saved();
                self.queue_project_for_charon();
                self.toasts
                    .success(format!("Neues Projekt: {}", name.trim()));
                true
            }
            Err(error) => {
                self.app_error = Some(error);
                false
            }
        }
    }

    pub fn project_delete(&mut self, name: &str) {
        match self.project.delete(name) {
            Ok(()) => self.toasts.success(format!("Gelöscht: {name}")),
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn project_export(&mut self, name: &str) {
        let Some(target) = rfd::FileDialog::new()
            .add_filter("LuxiFer-Projekt", &["luxi"])
            .set_file_name(format!("{name}.luxi"))
            .save_file()
        else {
            return;
        };
        match self.project.export(name, &target) {
            Ok(()) => self
                .toasts
                .success(format!("Exportiert: {}", target.display())),
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Strg+S: offenes Projekt in-place speichern; ohne offenes Projekt öffnet
    /// sich stattdessen die „Neues Projekt"-Maske (Name + Beschreibung) direkt
    /// im Designer.
    pub fn project_save(&mut self) {
        if !self.project.has_open() {
            self.open_project_save_dialog();
            return;
        }
        match self.project.save(self.session.state()) {
            Ok(version) => {
                self.session.mark_saved();
                self.queue_project_for_charon();
                self.toasts
                    .success(format!("Gespeichert ({})", version.label));
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Shift+Strg+S: neue Version. Ohne offenes Projekt gilt dasselbe wie bei
    /// Strg+S — erst benennen.
    pub fn project_save_version(&mut self) {
        if !self.project.has_open() {
            self.open_project_save_dialog();
            return;
        }
        match self.project.save_version(self.session.state()) {
            Ok(version) => {
                self.session.mark_saved();
                self.queue_project_for_charon();
                self.toasts
                    .success(format!("Neue Version {}", version.label));
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Erst nach erfolgreichem lokalem Speichern aufrufen. Ein Outbox-Fehler
    /// wird sichtbar, ändert den erfolgreichen lokalen Projektstand aber nicht.
    fn queue_project_for_charon(&mut self) {
        if !self.ui_settings.charon_enabled {
            return;
        }
        if let Err(error) = self
            .project
            .queue_current_for_sync(&self.ui_settings.workplace_id)
        {
            self.toasts.error(format!(
                "Projekt lokal gespeichert, Charon-Vormerkung fehlgeschlagen: {}",
                error.message()
            ));
        }
    }

    /// Öffnet die „Neues Projekt"-Maske (Name + Beschreibung) als modalen
    /// Dialog — die aktuelle Ansicht bleibt stehen.
    pub fn open_project_save_dialog(&mut self) {
        self.project_save_dialog = Some(crate::ui::ProjectSaveDialogState {
            focus_name: true,
            ..Default::default()
        });
    }

    /// Legt das Projekt aus dem Maskenentwurf an. Bei Erfolg true (Dialog
    /// schließen); bei Validierungsfehler (leerer Name) bleibt die Maske offen.
    /// Trägt ein anderes offenes Projekt ungespeicherte Änderungen, übernimmt
    /// der Dirty-Guard (Dialog schließt, Bestätigung folgt).
    pub fn commit_project_save_dialog(&mut self) -> bool {
        let Some(st) = self.project_save_dialog.as_ref() else {
            return false;
        };
        let name = st.name.clone();
        let description = st.description.clone();
        if self.session.is_dirty() && self.project.has_open() {
            self.pending_project = Some(PendingProjectAction::New { name, description });
            return true;
        }
        self.do_project_new(&name, &description)
    }
}
