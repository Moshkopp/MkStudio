//! Projekt-, Versions- und Asset-Lebenszyklus als UI-unabhängiger Dienst
//! (ADR 0011, Phase 3). Kapselt das offene Projekt und koordiniert die
//! Core-Projekt-API (`ProjectFile`, `list_projects`, `rename_project`,
//! `delete_project`). Fehler werden als stabiler [`AppError`] gemeldet, nicht
//! als roher String.
//!
//! Speichern ist bewusst manuell (kein Autosave): `save` schreibt die aktuelle
//! Version in-place, `save_version` legt eine neue an. Der Dirty-Schutz ist eine
//! reine Abfrage (`AppState::dirty`); die Warn-/Abbruch-Entscheidung trifft die
//! aufrufende Oberfläche.

use std::path::{Path, PathBuf};

use luxifer_core::{
    list_projects, project::ProjectFile, project::VersionInfo, projects_dir, rename_project,
    state::AppState, ProjectInfo,
};

use crate::AppError;

/// UI-unabhängige Detailsicht eines Projekts für den Browser: Metadaten und
/// Versionsliste, ohne Geometrie. Kommt für das offene Projekt aus dem
/// Speicher, sonst aus einer nur-lesenden Dateiladung.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectDetail {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub modified_at: String,
    pub versions: Vec<VersionInfo>,
    /// ID der aktuellen Version (die, die im Canvas landet).
    pub current_version: String,
}

impl ProjectDetail {
    fn from_file(pf: &ProjectFile) -> Self {
        Self {
            name: pf.name.clone(),
            description: pf.description.clone(),
            tags: pf.tags.clone(),
            created_at: pf.created_at.clone(),
            modified_at: pf.modified_at.clone(),
            versions: pf.versions.clone(),
            current_version: pf.current_version.clone(),
        }
    }
}

/// Hält das offene Projekt und dessen Ablageort. Ohne offenes Projekt ist der
/// Arbeitsstand „namenlos" (erst Anlegen/Speichern vergibt einen Namen).
#[derive(Default)]
pub struct ProjectService {
    open: Option<ProjectFile>,
}

impl ProjectService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Projektverzeichnis (plattformneutral über den Core bestimmt).
    fn dir() -> PathBuf {
        projects_dir()
    }

    // ---- Abfragen -----------------------------------------------------------

    /// Alle Projekte, sortiert nach zuletzt geändert (neueste zuerst).
    pub fn list(&self) -> Vec<ProjectInfo> {
        list_projects(&Self::dir())
    }

    /// Name des offenen Projekts, falls eines offen ist.
    pub fn open_name(&self) -> Option<&str> {
        self.open.as_ref().map(|p| p.name.as_str())
    }

    /// Ob gerade ein Projekt offen ist.
    pub fn has_open(&self) -> bool {
        self.open.is_some()
    }

    /// Löst die Bindung an das aktuell geöffnete Projekt, ohne dessen Dateien
    /// zu verändern. Der aufrufende Client kann anschließend einen leeren,
    /// ungespeicherten Editorzustand einsetzen.
    pub fn close(&mut self) {
        self.open = None;
    }

    /// Versionsliste des offenen Projekts (leer, wenn keins offen ist).
    pub fn versions(&self) -> &[VersionInfo] {
        self.open
            .as_ref()
            .map(|p| p.versions.as_slice())
            .unwrap_or(&[])
    }

    /// ID der aktuellen Version des offenen Projekts.
    pub fn current_version_id(&self) -> Option<&str> {
        self.open.as_ref().map(|p| p.current_version.as_str())
    }

    /// Friert den zuletzt erfolgreich gespeicherten Stand als unveränderliche
    /// lokale Sync-Revision ein. Ein Fehler hier macht das Projektspeichern
    /// nicht rückgängig; diese Grenze entscheidet der aufrufende Client.
    pub fn queue_current_for_sync(
        &self,
        workplace_id: &str,
    ) -> Result<crate::OutboxEntry, AppError> {
        let project = self.require_open_ref()?;
        let version_id = &project.current_version;
        let snapshot = Self::dir()
            .join(&project.name)
            .join(luxifer_core::project::VERSIONS_DIR)
            .join(format!("{version_id}.luxi"));
        crate::sync_outbox::enqueue_project_snapshot(project, version_id, workplace_id, &snapshot)
    }

    /// Detailsicht eines Projekts für den Browser (Metadaten + Versionen),
    /// ohne das offene Projekt zu wechseln. Für das offene Projekt kommt die
    /// Sicht aus dem Speicher, sonst wird die Projektdatei nur gelesen.
    pub fn detail(&self, name: &str) -> Result<ProjectDetail, AppError> {
        if let Some(pf) = self.open.as_ref().filter(|p| p.name == name) {
            return Ok(ProjectDetail::from_file(pf));
        }
        let pf = ProjectFile::load_by_name(&Self::dir(), name).map_err(|e| {
            AppError::wrap(
                "project_read",
                format!("Projekt {name} konnte nicht gelesen werden."),
                e,
            )
        })?;
        Ok(ProjectDetail::from_file(&pf))
    }

    /// Zustand eines Projekts nur lesen (z. B. für eine Vorschau im Browser).
    /// Wechselt das offene Projekt **nicht** und mutiert nichts.
    pub fn peek_state(&self, name: &str) -> Result<AppState, AppError> {
        let pf = ProjectFile::load_by_name(&Self::dir(), name).map_err(|e| {
            AppError::wrap(
                "project_read",
                format!("Projekt {name} konnte nicht gelesen werden."),
                e,
            )
        })?;
        Ok(pf.into_state())
    }

    // ---- Lebenszyklus -------------------------------------------------------

    /// Neues Projekt aus dem aktuellen Zustand anlegen und sofort speichern.
    /// Der Name darf nicht leer sein; die Beschreibung ist optional.
    pub fn new_project(
        &mut self,
        state: &AppState,
        name: &str,
        description: &str,
    ) -> Result<(), AppError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::new(
                "project_name_empty",
                "Bitte einen Projektnamen angeben.",
            ));
        }
        let mut pf = ProjectFile::from_state(state, name, Vec::new());
        pf.description = description.trim().to_string();
        pf.save_to_dir(&Self::dir()).map_err(|e| {
            AppError::wrap("project_write", "Projekt konnte nicht angelegt werden.", e)
        })?;
        pf.save_current(&Self::dir(), &[]).map_err(|e| {
            AppError::wrap("project_write", "Projekt konnte nicht angelegt werden.", e)
        })?;
        self.open = Some(pf);
        Ok(())
    }

    /// Projekt laden und seinen Zustand zurückgeben (der Aufrufer ersetzt den
    /// Editorzustand damit). Bei Fehler bleibt das bisher offene Projekt erhalten.
    pub fn open(&mut self, name: &str) -> Result<AppState, AppError> {
        let pf = ProjectFile::load_by_name(&Self::dir(), name).map_err(|e| {
            AppError::wrap(
                "project_read",
                format!("Projekt {name} konnte nicht geöffnet werden."),
                e,
            )
        })?;
        let state = pf.clone().into_state();
        self.open = Some(pf);
        Ok(state)
    }

    /// In-place speichern (aktuelle Version). Erfordert ein offenes Projekt.
    pub fn save(&mut self, state: &AppState) -> Result<VersionInfo, AppError> {
        let pf = self.require_open_mut()?;
        pf.update_from_state(state);
        pf.save_current(&Self::dir(), &[])
            .map_err(|e| AppError::wrap("project_write", "Speichern fehlgeschlagen.", e))
    }

    /// Als neue Version speichern.
    pub fn save_version(&mut self, state: &AppState) -> Result<VersionInfo, AppError> {
        let pf = self.require_open_mut()?;
        pf.update_from_state(state);
        pf.add_version(&Self::dir(), String::new(), &[])
            .map_err(|e| AppError::wrap("project_write", "Neue Version fehlgeschlagen.", e))
    }

    /// Eine bestimmte Version laden und ihren Zustand zurückgeben; sie wird zum
    /// kanonischen offenen Zustand.
    pub fn open_version(&mut self, version_id: &str) -> Result<AppState, AppError> {
        let name = self.require_open_ref()?.name.clone();
        let pf = ProjectFile::load_version(&Self::dir(), &name, version_id).map_err(|e| {
            AppError::wrap("version_read", "Version konnte nicht geladen werden.", e)
        })?;
        let state = pf.clone().into_state();
        self.open = Some(pf);
        Ok(state)
    }

    /// Eine Version löschen. Die letzte Version schützt der Core. War es die
    /// **aktuelle** Version, befördert der Core die vorherige und gibt deren
    /// Zustand zurück — der Aufrufer MUSS den Canvas dann darauf setzen, sonst
    /// zeigt der Editor stillschweigend veraltete Geometrie.
    pub fn delete_version(&mut self, version_id: &str) -> Result<Option<AppState>, AppError> {
        let pf = self.require_open_mut()?;
        let promoted = pf.delete_version(&Self::dir(), version_id).map_err(|e| {
            AppError::wrap("version_delete", "Version konnte nicht gelöscht werden.", e)
        })?;
        Ok(promoted.map(|snap| snap.into_state()))
    }

    /// Projekt umbenennen. Benennt das offene Projekt bei Bedarf mit um.
    pub fn rename(&mut self, old_name: &str, new_name: &str) -> Result<(), AppError> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(AppError::new(
                "project_name_empty",
                "Bitte einen neuen Projektnamen angeben.",
            ));
        }
        rename_project(&Self::dir(), old_name, new_name)
            .map_err(|e| AppError::wrap("project_rename", "Umbenennen fehlgeschlagen.", e))?;
        if let Some(pf) = self.open.as_mut() {
            if pf.name == old_name {
                pf.name = new_name.to_string();
            }
        }
        Ok(())
    }

    /// Projekt löschen. Ist es das offene Projekt, wird es geschlossen.
    pub fn delete(&mut self, name: &str) -> Result<(), AppError> {
        luxifer_core::delete_project(&Self::dir(), name)
            .map_err(|e| AppError::wrap("project_delete", "Löschen fehlgeschlagen.", e))?;
        if self.open_name() == Some(name) {
            self.open = None;
        }
        Ok(())
    }

    /// Die Projektdatei nach `ziel` exportieren (Kopie der `projekt.luxi`).
    pub fn export(&self, name: &str, ziel: &Path) -> Result<(), AppError> {
        let src = Self::dir().join(name).join("projekt.luxi");
        std::fs::copy(&src, ziel).map_err(|e| {
            AppError::wrap("project_export", "Export fehlgeschlagen.", e.to_string())
        })?;
        Ok(())
    }

    // ---- Helfer -------------------------------------------------------------

    fn require_open_ref(&self) -> Result<&ProjectFile, AppError> {
        self.open.as_ref().ok_or_else(Self::no_open_project)
    }

    fn require_open_mut(&mut self) -> Result<&mut ProjectFile, AppError> {
        self.open.as_mut().ok_or_else(Self::no_open_project)
    }

    fn no_open_project() -> AppError {
        AppError::new(
            "no_open_project",
            "Kein Projekt offen — erst anlegen oder öffnen.",
        )
    }
}

#[cfg(test)]
mod tests;
