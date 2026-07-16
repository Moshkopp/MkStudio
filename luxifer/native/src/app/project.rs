use luxifer_core::state::AppState;

use super::App;
use crate::ui::PendingProjectAction;

pub(super) struct ProjectIntegrationRuntime {
    request_tx: std::sync::mpsc::Sender<Vec<String>>,
    result_rx: std::sync::mpsc::Receiver<ProjectIntegrationResult>,
}

struct ProjectIntegrationResult {
    accepted: usize,
    project_names: Vec<String>,
    error: Option<luxifer_application::AppError>,
}

impl ProjectIntegrationRuntime {
    pub fn new() -> Result<Self, luxifer_application::AppError> {
        let (request_tx, request_rx) = std::sync::mpsc::channel::<Vec<String>>();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("project-integration".into())
            .spawn(move || {
                while let Ok(revision_ids) = request_rx.recv() {
                    let mut result = ProjectIntegrationResult {
                        accepted: 0,
                        project_names: Vec::new(),
                        error: None,
                    };
                    for revision_id in revision_ids {
                        let comparison =
                            match luxifer_application::compare_inbox_revision(&revision_id) {
                                Ok(comparison) => comparison,
                                Err(error) => {
                                    result.error = Some(error);
                                    break;
                                }
                            };
                        let accepted = if comparison.local_project_name.is_some() {
                            luxifer_application::accept_inbox_revision(&revision_id)
                        } else {
                            luxifer_application::apply_inbox_revision(&revision_id)
                        };
                        match accepted {
                            Ok(name) => {
                                result.accepted += 1;
                                result.project_names.push(name);
                            }
                            Err(error) => {
                                result.error = Some(error);
                                break;
                            }
                        }
                    }
                    if result_tx.send(result).is_err() {
                        return;
                    }
                }
            })
            .map_err(|error| {
                luxifer_application::AppError::wrap(
                    "project_worker_start",
                    "Projektabgleich konnte nicht gestartet werden.",
                    error.to_string(),
                )
            })?;
        Ok(Self {
            request_tx,
            result_rx,
        })
    }
}

impl App {
    fn refresh_project_catalog(&mut self) {
        self.project_catalog = self.project.list();
    }

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
        // Derselbe Inbox-Button deckt beide Fälle ab: Ein unbekanntes Projekt
        // wird lokal angelegt, eine weitere Revision desselben stabilen
        // Projekts wird als neue lokale Version übernommen. Der zweite Pfad
        // besitzt außerdem den Dirty-Guard für ein gerade geöffnetes Projekt.
        match luxifer_application::compare_inbox_revision(revision_id) {
            Ok(comparison) if comparison.local_project_name.is_some() => {
                self.accept_inbox_revision(revision_id);
                return;
            }
            Ok(_) => {}
            Err(error) => {
                self.app_error = Some(error);
                return;
            }
        }
        self.start_project_integration(vec![revision_id.to_owned()]);
    }

    pub fn apply_all_inbox_revisions(&mut self) {
        let revision_ids: Vec<_> = self
            .project_inbox
            .iter()
            .filter(|entry| {
                matches!(
                    entry.status,
                    luxifer_application::InboxStatus::PendingReview
                        | luxifer_application::InboxStatus::Deferred
                )
            })
            .map(|entry| entry.revision_id.clone())
            .collect();
        if revision_ids.is_empty() {
            return;
        }
        let open_name = self.project.open_name();
        let touches_open_project = revision_ids.iter().any(|revision_id| {
            luxifer_application::compare_inbox_revision(revision_id)
                .ok()
                .and_then(|comparison| comparison.local_project_name)
                .is_some_and(|name| Some(name.as_str()) == open_name)
        });
        if touches_open_project && self.session.is_dirty() {
            self.pending_project = Some(PendingProjectAction::AcceptAllInbox(revision_ids));
        } else {
            self.start_project_integration(revision_ids);
        }
    }

    fn start_project_integration(&mut self, revision_ids: Vec<String>) {
        if self.project_integration_pending {
            return;
        }
        if self
            .project_integration
            .request_tx
            .send(revision_ids)
            .is_ok()
        {
            self.project_integration_pending = true;
            self.toasts
                .success("Charon-Projekte werden im Hintergrund übernommen …");
        }
    }

    pub fn poll_project_integration(&mut self) -> bool {
        let Ok(result) = self.project_integration.result_rx.try_recv() else {
            return false;
        };
        self.project_integration_pending = false;
        self.refresh_project_catalog();
        self.refresh_project_inbox();
        self.project_browser.cached = None;
        if let Some(name) = result.project_names.last() {
            self.project_browser.selected = Some(name.clone());
        }
        if let Some(open_name) = self.project.open_name().map(str::to_owned) {
            let updated_open_project = result.project_names.contains(&open_name);
            if updated_open_project {
                match self.project.open(&open_name) {
                    Ok(state) => self.replace_editor_state(state),
                    Err(error) => {
                        self.app_error = Some(error);
                        return true;
                    }
                }
            }
        }
        if result.accepted > 0 {
            self.toasts.success(format!(
                "{} Charon-Revision(en) übernommen.",
                result.accepted
            ));
        }
        if let Some(error) = result.error {
            self.app_error = Some(error);
        }
        true
    }

    pub fn show_inbox_comparison(&mut self, revision_id: &str) {
        match luxifer_application::compare_inbox_revision(revision_id) {
            Ok(comparison) => {
                let local_preview = comparison
                    .local_state
                    .as_ref()
                    .map(crate::ui::preview_from_state);
                let remote_preview = crate::ui::preview_from_state(&comparison.remote_state);
                self.revision_comparison = Some(crate::ui::RevisionComparisonState {
                    comparison,
                    local_preview,
                    remote_preview,
                });
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn keep_local_inbox_revision(&mut self, revision_id: &str) {
        match luxifer_application::ignore_inbox_revision(revision_id) {
            Ok(()) => {
                self.refresh_project_inbox();
                self.toasts
                    .success("Lokale Projektversion bleibt erhalten.");
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn accept_inbox_revision(&mut self, revision_id: &str) {
        let target_is_open = match luxifer_application::compare_inbox_revision(revision_id) {
            Ok(comparison) => comparison
                .local_project_name
                .is_some_and(|name| self.project.open_name() == Some(name.as_str())),
            Err(error) => {
                self.app_error = Some(error);
                return;
            }
        };
        if target_is_open && self.session.is_dirty() {
            self.pending_project = Some(PendingProjectAction::AcceptInbox(revision_id.to_string()));
        } else {
            self.do_accept_inbox_revision(revision_id);
        }
    }

    fn do_accept_inbox_revision(&mut self, revision_id: &str) {
        self.start_project_integration(vec![revision_id.to_owned()]);
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
            Some(PendingProjectAction::AcceptInbox(revision_id)) => {
                self.do_accept_inbox_revision(&revision_id);
            }
            Some(PendingProjectAction::AcceptAllInbox(revision_ids)) => {
                self.start_project_integration(revision_ids);
            }
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
        self.session_asset_context.clear();
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
                self.refresh_project_catalog();
                self.replace_editor_state(state);
                self.toasts
                    .success("Version gelöscht — vorherige Version geladen.");
            }
            Ok(None) => {
                self.refresh_project_catalog();
                self.toasts.success("Version gelöscht.");
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn project_rename(&mut self, from: &str, to: &str) {
        match self.project.rename(from, to) {
            Ok(()) => {
                self.refresh_project_catalog();
                let to = to.trim();
                if self.project_browser.selected.as_deref() == Some(from) {
                    self.project_browser.selected = Some(to.to_string());
                }
                self.toasts
                    .success(format!("Umbenannt: „{from}“ heißt jetzt „{to}“"));
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
                self.refresh_project_catalog();
                self.tag_current_project_assets();
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
            Ok(()) => {
                self.refresh_project_catalog();
                self.toasts.success(format!("Gelöscht: {name}"));
            }
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
                self.refresh_project_catalog();
                self.tag_current_project_assets();
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
                self.refresh_project_catalog();
                self.tag_current_project_assets();
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
