//! Persistente lokale Inbox für unveränderte Charon-Projektrevisionen.

use std::path::{Path, PathBuf};

use luxifer_core::{assets::content_hash, data_root, datetime};
use serde::{Deserialize, Serialize};

use crate::{charon::CharonRevision, AppError};

const INBOX_DIR: &str = "sync/inbox";
const MANIFEST_FILE: &str = "manifest.json";
const PAYLOAD_FILE: &str = "payload.luxi";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxStatus {
    PendingReview,
    Deferred,
    Applied,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxEntry {
    pub revision_id: String,
    pub project_id: String,
    pub project_name: String,
    pub project_version_id: String,
    pub parent_revision_id: Option<String>,
    pub source_workplace_id: String,
    pub queued_at: String,
    pub received_at: String,
    pub content_hash: String,
    pub payload_file: String,
    pub status: InboxStatus,
}

/// Read-only Sicht auf eine empfangene Revision und den gegebenenfalls lokal
/// vorhandenen Projektstand. Der Vergleich mutiert weder Inbox noch Projekte.
#[derive(Debug, Clone)]
pub struct InboxComparison {
    pub entry: InboxEntry,
    pub local_project_name: Option<String>,
    pub local_modified_at: Option<String>,
    pub remote_modified_at: String,
    pub local_state: Option<luxifer_core::state::AppState>,
    pub remote_state: luxifer_core::state::AppState,
    pub bed_changed: bool,
    pub layers_changed: bool,
    pub shapes_changed: bool,
    pub metadata_changed: bool,
}

impl InboxEntry {
    pub fn payload_path(&self) -> PathBuf {
        inbox_dir().join(&self.revision_id).join(&self.payload_file)
    }
}

pub(crate) fn store_remote_revision(revision: CharonRevision) -> Result<bool, AppError> {
    if !valid_id(&revision.revision_id) || !valid_id(&revision.project_id) {
        return Err(AppError::new(
            "inbox_revision_id",
            "Charon lieferte eine ungültige Revisionskennung.",
        ));
    }
    let actual_hash = content_hash(revision.payload.as_bytes());
    if actual_hash != revision.content_hash {
        return Err(AppError::new(
            "inbox_hash",
            "Die von Charon gelieferte Projektrevision ist beschädigt.",
        ));
    }
    let root = inbox_dir();
    std::fs::create_dir_all(&root).map_err(inbox_write_error)?;
    let final_dir = root.join(&revision.revision_id);
    let manifest_path = final_dir.join(MANIFEST_FILE);
    if manifest_path.exists() {
        let existing = read_entry(&manifest_path)?;
        if existing.content_hash == revision.content_hash {
            return Ok(false);
        }
        return Err(AppError::new(
            "inbox_conflict",
            "Charon lieferte dieselbe Revisions-ID mit anderem Inhalt.",
        ));
    }
    let entry = InboxEntry {
        revision_id: revision.revision_id,
        project_id: revision.project_id,
        project_name: revision.project_name,
        project_version_id: revision.project_version_id,
        parent_revision_id: revision.parent_revision_id,
        source_workplace_id: revision.workplace_id,
        queued_at: revision.queued_at,
        received_at: datetime::now_iso8601(),
        content_hash: revision.content_hash,
        payload_file: PAYLOAD_FILE.into(),
        status: InboxStatus::PendingReview,
    };
    let temp_dir = root.join(format!(".{}.tmp", entry.revision_id));
    std::fs::create_dir(&temp_dir).map_err(inbox_write_error)?;
    let result = (|| {
        std::fs::write(temp_dir.join(PAYLOAD_FILE), revision.payload).map_err(inbox_write_error)?;
        let manifest = serde_json::to_vec_pretty(&entry).map_err(|error| {
            AppError::wrap(
                "inbox_json",
                "Charon-Inbox konnte nicht serialisiert werden.",
                error.to_string(),
            )
        })?;
        std::fs::write(temp_dir.join(MANIFEST_FILE), manifest).map_err(inbox_write_error)?;
        std::fs::rename(&temp_dir, &final_dir).map_err(inbox_write_error)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    result?;
    Ok(true)
}

pub fn list_inbox() -> Result<Vec<InboxEntry>, AppError> {
    let root = inbox_dir();
    let dirs = match std::fs::read_dir(root) {
        Ok(dirs) => dirs,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(inbox_read_error(error)),
    };
    let mut entries = Vec::new();
    for dir in dirs {
        let dir = dir.map_err(inbox_read_error)?;
        if !dir.file_type().map_err(inbox_read_error)?.is_dir()
            || dir.file_name().to_string_lossy().starts_with('.')
        {
            continue;
        }
        entries.push(read_entry(&dir.path().join(MANIFEST_FILE))?);
    }
    entries.sort_by(|a, b| a.revision_id.cmp(&b.revision_id));
    Ok(entries)
}

pub fn defer_inbox_revision(revision_id: &str) -> Result<(), AppError> {
    set_inbox_status(revision_id, InboxStatus::Deferred)
}

pub fn reconsider_inbox_revision(revision_id: &str) -> Result<(), AppError> {
    set_inbox_status(revision_id, InboxStatus::PendingReview)
}

pub fn ignore_inbox_revision(revision_id: &str) -> Result<(), AppError> {
    set_inbox_status(revision_id, InboxStatus::Ignored)
}

/// Übernimmt den empfangenen Stand als neue lokale Projektversion. Die lokale
/// Versionshistorie bleibt erhalten; Projektname und Erstellungszeit bleiben
/// arbeitsplatzlokal. Bild-Assets werden bis zur Asset-Synchronisierung abgelehnt.
pub fn accept_inbox_revision(revision_id: &str) -> Result<String, AppError> {
    let entry = read_entry(&inbox_dir().join(revision_id).join(MANIFEST_FILE))?;
    let mut remote = read_verified_project(&entry)?;
    resolve_project_assets(&mut remote)?;
    let projects_dir = luxifer_core::projects_dir();
    let (local_name, mut local) = luxifer_core::list_projects(&projects_dir)
        .into_iter()
        .map(|info| {
            luxifer_core::ProjectFile::load_by_name(&projects_dir, &info.name)
                .map(|project| (info.name, project))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::wrap(
                "project_read",
                "Lokale Projekte konnten nicht geprüft werden.",
                error,
            )
        })?
        .into_iter()
        .find(|(_, project)| project.id == entry.project_id)
        .ok_or_else(|| {
            AppError::new(
                "inbox_project_missing",
                "Das zugehörige lokale Projekt wurde nicht gefunden.",
            )
        })?;
    let mut incoming_version = remote
        .versions
        .iter()
        .find(|version| version.id == remote.current_version)
        .cloned()
        .ok_or_else(|| {
            AppError::new(
                "inbox_project_version",
                "Die empfangene aktuelle Projektversion fehlt.",
            )
        })?;
    // Die fremde Version bekommt bewusst eine neue lokale Versions-ID: Office
    // und Workshop können dieselbe Ausgangsversion in-place verändert haben.
    // Ein Überschreiben dieser ID würde den lokalen Stand vernichten.
    incoming_version.id = luxifer_core::datetime::gen_id();
    incoming_version.label = next_version_label(&local.versions);
    incoming_version.note = format!("Von Charon: {}", entry.source_workplace_id);
    let accepted_version_id = incoming_version.id.clone();
    local.versions.push(incoming_version);
    local.description = remote.description;
    local.tags = remote.tags;
    local.modified_at = remote.modified_at;
    local.current_version = accepted_version_id;
    local.bed_w_mm = remote.bed_w_mm;
    local.bed_h_mm = remote.bed_h_mm;
    local.layers = remote.layers;
    local.shapes = remote.shapes;
    local.sync_asset_refs();

    let project_dir = projects_dir.join(&local_name);
    let versions_dir = project_dir.join(luxifer_core::project::VERSIONS_DIR);
    std::fs::create_dir_all(&versions_dir).map_err(inbox_write_error)?;
    let bytes = local.to_json().map_err(|error| {
        AppError::wrap(
            "inbox_project_json",
            "Übernommene Projektversion konnte nicht serialisiert werden.",
            error,
        )
    })?;
    let project_temp = project_dir.join(".projekt.inbox.tmp");
    let version_temp = versions_dir.join(".version.inbox.tmp");
    let version_path = versions_dir.join(format!("{}.luxi", local.current_version));
    let result = (|| {
        std::fs::write(&project_temp, &bytes).map_err(inbox_write_error)?;
        std::fs::write(&version_temp, &bytes).map_err(inbox_write_error)?;
        std::fs::rename(&version_temp, version_path).map_err(inbox_write_error)?;
        std::fs::rename(
            &project_temp,
            project_dir.join(luxifer_core::project::PROJECT_FILE),
        )
        .map_err(inbox_write_error)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&project_temp);
        let _ = std::fs::remove_file(&version_temp);
    }
    result?;
    set_inbox_status(revision_id, InboxStatus::Applied)?;
    Ok(local_name)
}

pub fn compare_inbox_revision(revision_id: &str) -> Result<InboxComparison, AppError> {
    let entry = read_entry(&inbox_dir().join(revision_id).join(MANIFEST_FILE))?;
    let remote = read_verified_project(&entry)?;
    let projects_dir = luxifer_core::projects_dir();
    let local = luxifer_core::list_projects(&projects_dir)
        .into_iter()
        .map(|info| {
            luxifer_core::ProjectFile::load_by_name(&projects_dir, &info.name)
                .map(|project| (info.name, project))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::wrap(
                "project_read",
                "Lokale Projekte konnten nicht verglichen werden.",
                error,
            )
        })?
        .into_iter()
        .find(|(_, project)| project.id == entry.project_id);

    let (
        local_project_name,
        local_modified_at,
        local_state,
        bed_changed,
        layers_changed,
        shapes_changed,
        metadata_changed,
    ) = if let Some((name, local)) = local {
        let bed_changed = local.bed_w_mm != remote.bed_w_mm || local.bed_h_mm != remote.bed_h_mm;
        let layers_changed = local.layers != remote.layers;
        let shapes_changed = local.shapes != remote.shapes;
        let metadata_changed = local.name != remote.name
            || local.description != remote.description
            || local.tags != remote.tags;
        (
            Some(name),
            Some(local.modified_at.clone()),
            Some(local.into_state()),
            bed_changed,
            layers_changed,
            shapes_changed,
            metadata_changed,
        )
    } else {
        (None, None, None, true, true, true, true)
    };
    let remote_modified_at = remote.modified_at.clone();
    let remote_state = remote.into_state();
    Ok(InboxComparison {
        entry,
        local_project_name,
        local_modified_at,
        remote_modified_at,
        local_state,
        remote_state,
        bed_changed,
        layers_changed,
        shapes_changed,
        metadata_changed,
    })
}

pub fn apply_inbox_revision(revision_id: &str) -> Result<String, AppError> {
    let entry = read_entry(&inbox_dir().join(revision_id).join(MANIFEST_FILE))?;
    let mut project = read_verified_project(&entry)?;
    resolve_project_assets(&mut project)?;
    project
        .versions
        .retain(|version| version.id == project.current_version);
    if project.versions.is_empty() {
        return Err(AppError::new(
            "inbox_project_version",
            "Die empfangene aktuelle Projektversion fehlt.",
        ));
    }
    let projects_dir = luxifer_core::projects_dir();
    for local in luxifer_core::list_projects(&projects_dir) {
        let existing = luxifer_core::ProjectFile::load_by_name(&projects_dir, &local.name)
            .map_err(|error| {
                AppError::wrap(
                    "project_read",
                    "Lokale Projekte konnten nicht geprüft werden.",
                    error,
                )
            })?;
        if existing.id == project.id {
            return Err(AppError::new(
                "inbox_project_conflict",
                "Dieses Projekt existiert bereits lokal. Die Revision bleibt zur Prüfung in der Inbox.",
            ));
        }
    }

    let local_name = unique_project_name(&projects_dir, &project.name);
    project.name = local_name.clone();
    let temp_dir = projects_dir.join(format!(".inbox-{}.tmp", entry.revision_id));
    let final_dir = projects_dir.join(&local_name);
    std::fs::create_dir_all(temp_dir.join(luxifer_core::project::VERSIONS_DIR))
        .map_err(inbox_write_error)?;
    let result = (|| {
        let bytes = project.to_json().map_err(|error| {
            AppError::wrap(
                "inbox_project_json",
                "Empfangenes Projekt konnte nicht serialisiert werden.",
                error,
            )
        })?;
        std::fs::write(temp_dir.join(luxifer_core::project::PROJECT_FILE), &bytes)
            .map_err(inbox_write_error)?;
        std::fs::write(
            temp_dir
                .join(luxifer_core::project::VERSIONS_DIR)
                .join(format!("{}.luxi", project.current_version)),
            bytes,
        )
        .map_err(inbox_write_error)?;
        std::fs::rename(&temp_dir, &final_dir).map_err(inbox_write_error)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    result?;
    set_inbox_status(revision_id, InboxStatus::Applied)?;
    Ok(local_name)
}

fn read_verified_project(entry: &InboxEntry) -> Result<luxifer_core::ProjectFile, AppError> {
    let payload = std::fs::read_to_string(entry.payload_path()).map_err(inbox_read_error)?;
    if content_hash(payload.as_bytes()) != entry.content_hash {
        return Err(AppError::new(
            "inbox_hash",
            "Die lokale Inbox-Revision ist beschädigt.",
        ));
    }
    let project = luxifer_core::ProjectFile::from_json(&payload).map_err(|error| {
        AppError::wrap(
            "inbox_project_json",
            "Die empfangene Projektrevision ist ungültig.",
            error,
        )
    })?;
    if project.id != entry.project_id || project.current_version != entry.project_version_id {
        return Err(AppError::new(
            "inbox_project_identity",
            "Die empfangene Projektrevision passt nicht zu ihrem Manifest.",
        ));
    }
    Ok(project)
}

fn resolve_project_assets(project: &mut luxifer_core::ProjectFile) -> Result<(), AppError> {
    let store = luxifer_core::assets_dir();
    for id in &project.asset_refs {
        if luxifer_core::asset_path(&store, id).is_none() {
            return Err(AppError::new(
                "inbox_asset_missing",
                format!("Projekt-Asset {id} ist lokal noch nicht verfügbar."),
            ));
        }
    }
    for shape in &mut project.shapes {
        if let Some(text) = shape.text_meta.as_mut() {
            if let Some(id) = text.font_asset.as_ref() {
                if let Some(path) = luxifer_core::asset_path(&store, id) {
                    text.font_path = path.to_string_lossy().into_owned();
                }
            }
        }
    }
    Ok(())
}

fn set_inbox_status(revision_id: &str, status: InboxStatus) -> Result<(), AppError> {
    let dir = inbox_dir().join(revision_id);
    let manifest_path = dir.join(MANIFEST_FILE);
    let mut entry = read_entry(&manifest_path)?;
    entry.status = status;
    let bytes = serde_json::to_vec_pretty(&entry).map_err(|error| {
        AppError::wrap(
            "inbox_json",
            "Charon-Inbox konnte nicht serialisiert werden.",
            error.to_string(),
        )
    })?;
    let temp = dir.join(".manifest.tmp");
    std::fs::write(&temp, bytes).map_err(inbox_write_error)?;
    std::fs::rename(temp, manifest_path).map_err(inbox_write_error)
}

fn unique_project_name(projects_dir: &Path, preferred: &str) -> String {
    let preferred = if preferred.trim().is_empty() {
        "Empfangenes Projekt"
    } else {
        preferred.trim()
    };
    if !projects_dir.join(preferred).exists() {
        return preferred.into();
    }
    for suffix in 1.. {
        let candidate = if suffix == 1 {
            format!("{preferred} (Charon)")
        } else {
            format!("{preferred} (Charon {suffix})")
        };
        if !projects_dir.join(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn next_version_label(versions: &[luxifer_core::project::VersionInfo]) -> String {
    let max = versions
        .iter()
        .filter_map(|version| version.label.strip_prefix('V'))
        .filter_map(|number| number.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("V{}", max + 1)
}

fn read_entry(path: &Path) -> Result<InboxEntry, AppError> {
    let bytes = std::fs::read(path).map_err(inbox_read_error)?;
    serde_json::from_slice(&bytes).map_err(|error| {
        AppError::wrap(
            "inbox_json",
            "Charon-Inbox enthält ungültige Daten.",
            error.to_string(),
        )
    })
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn inbox_dir() -> PathBuf {
    data_root().join(INBOX_DIR)
}

fn inbox_write_error(error: std::io::Error) -> AppError {
    AppError::wrap(
        "inbox_write",
        "Charon-Inbox konnte nicht geschrieben werden.",
        error.to_string(),
    )
}

fn inbox_read_error(error: std::io::Error) -> AppError {
    AppError::wrap(
        "inbox_read",
        "Charon-Inbox konnte nicht gelesen werden.",
        error.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::with_temp_dir;

    fn revision(payload: &str) -> CharonRevision {
        CharonRevision {
            revision_id: "revision-remote-1".into(),
            project_id: "project-remote-1".into(),
            project_name: "Remote".into(),
            project_version_id: "version-1".into(),
            parent_revision_id: None,
            workplace_id: "office-1".into(),
            queued_at: "2026-07-13T12:00:00Z".into(),
            content_hash: content_hash(payload.as_bytes()),
            payload: payload.into(),
        }
    }

    #[test]
    fn fremdrevision_wird_idempotent_und_unveraendert_abgelegt() {
        let _guard = with_temp_dir("sync_inbox");
        let item = revision(r#"{"version":1}"#);
        assert!(store_remote_revision(item.clone()).unwrap());
        assert!(!store_remote_revision(item).unwrap());
        let entries = list_inbox().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, InboxStatus::PendingReview);
        assert_eq!(
            std::fs::read_to_string(entries[0].payload_path()).unwrap(),
            r#"{"version":1}"#
        );
    }

    #[test]
    fn beschaedigte_fremdrevision_wird_abgewiesen() {
        let _guard = with_temp_dir("sync_inbox_hash");
        let mut item = revision("{}");
        item.content_hash = "falscher-hash".into();
        assert!(store_remote_revision(item).is_err());
        assert!(list_inbox().unwrap().is_empty());
    }

    #[test]
    fn neues_projekt_wird_uebernommen_ohne_editorzustand_zu_beruehren() {
        let _guard = with_temp_dir("sync_inbox_apply");
        let project = luxifer_core::ProjectFile::from_state(
            &luxifer_core::AppState::new(),
            "Vom Office",
            Vec::new(),
        );
        let payload = project.to_json().unwrap();
        let item = CharonRevision {
            revision_id: "revision-apply-1".into(),
            project_id: project.id.clone(),
            project_name: project.name.clone(),
            project_version_id: project.current_version.clone(),
            parent_revision_id: None,
            workplace_id: "office-1".into(),
            queued_at: "2026-07-13T12:00:00Z".into(),
            content_hash: content_hash(payload.as_bytes()),
            payload,
        };
        store_remote_revision(item).unwrap();

        let name = apply_inbox_revision("revision-apply-1").unwrap();
        assert_eq!(name, "Vom Office");
        let imported =
            luxifer_core::ProjectFile::load_by_name(&luxifer_core::projects_dir(), "Vom Office")
                .unwrap();
        assert_eq!(imported.id, project.id);
        assert_eq!(list_inbox().unwrap()[0].status, InboxStatus::Applied);
    }

    #[test]
    fn projekt_mit_vorhandenem_asset_wird_uebernommen() {
        let _guard = with_temp_dir("sync_inbox_asset");
        let bytes = b"synchronisiertes-asset";
        let asset_id = luxifer_core::assets::content_hash(bytes);
        let meta = luxifer_core::AssetMeta {
            id: asset_id.clone(),
            ext: "bin".into(),
            kind: luxifer_core::AssetKind::Image,
            original_name: "bild.bin".into(),
            source_format: String::new(),
            width: 1,
            height: 1,
            import_at: String::new(),
            tags: Vec::new(),
        };
        luxifer_core::store_asset(&luxifer_core::assets_dir(), &meta, bytes).unwrap();

        let mut state = luxifer_core::AppState::new();
        state.add_image(asset_id, 0.0, 0.0, 10.0, 10.0);
        let project = luxifer_core::ProjectFile::from_state(&state, "Mit Asset", Vec::new());
        let payload = project.to_json().unwrap();
        store_remote_revision(CharonRevision {
            revision_id: "revision-asset-1".into(),
            project_id: project.id.clone(),
            project_name: project.name.clone(),
            project_version_id: project.current_version.clone(),
            parent_revision_id: None,
            workplace_id: "office-1".into(),
            queued_at: "2026-07-13T12:00:00Z".into(),
            content_hash: content_hash(payload.as_bytes()),
            payload,
        })
        .unwrap();

        assert_eq!(
            apply_inbox_revision("revision-asset-1").unwrap(),
            "Mit Asset"
        );
    }

    #[test]
    fn vergleich_findet_lokales_projekt_ueber_stabile_id_und_mutiert_nichts() {
        let _guard = with_temp_dir("sync_inbox_compare");
        let local = luxifer_core::ProjectFile::from_state(
            &luxifer_core::AppState::new(),
            "Werkstattname",
            Vec::new(),
        );
        local.save_to_dir(&luxifer_core::projects_dir()).unwrap();

        let mut remote_state = luxifer_core::AppState::new();
        remote_state.add_shape(luxifer_core::Geo::Rect {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
        });
        let mut remote = local.clone();
        remote.name = "Officename".into();
        remote.update_from_state(&remote_state);
        let payload = remote.to_json().unwrap();
        store_remote_revision(CharonRevision {
            revision_id: "revision-compare-1".into(),
            project_id: remote.id.clone(),
            project_name: remote.name.clone(),
            project_version_id: remote.current_version.clone(),
            parent_revision_id: None,
            workplace_id: "office-1".into(),
            queued_at: "2026-07-13T12:00:00Z".into(),
            content_hash: content_hash(payload.as_bytes()),
            payload,
        })
        .unwrap();

        let comparison = compare_inbox_revision("revision-compare-1").unwrap();
        assert_eq!(
            comparison.local_project_name.as_deref(),
            Some("Werkstattname")
        );
        assert!(comparison.layers_changed);
        assert!(comparison.shapes_changed);
        assert!(comparison.metadata_changed);
        assert_eq!(comparison.local_state.unwrap().shapes.len(), 0);
        assert_eq!(comparison.remote_state.shapes.len(), 1);
        assert_eq!(list_inbox().unwrap()[0].status, InboxStatus::PendingReview);

        let name = accept_inbox_revision("revision-compare-1").unwrap();
        assert_eq!(name, "Werkstattname");
        let accepted =
            luxifer_core::ProjectFile::load_by_name(&luxifer_core::projects_dir(), &name).unwrap();
        assert_eq!(accepted.versions.len(), 2);
        assert_eq!(accepted.current().unwrap().label, "V2");
        assert_eq!(accepted.shapes.len(), 1);
        assert_eq!(list_inbox().unwrap()[0].status, InboxStatus::Applied);
    }

    #[test]
    fn lokal_behalten_quittiert_nur_die_inbox() {
        let _guard = with_temp_dir("sync_inbox_ignore");
        let project = luxifer_core::ProjectFile::from_state(
            &luxifer_core::AppState::new(),
            "Ignoriert",
            Vec::new(),
        );
        let payload = project.to_json().unwrap();
        store_remote_revision(CharonRevision {
            revision_id: "revision-ignore-1".into(),
            project_id: project.id.clone(),
            project_name: project.name,
            project_version_id: project.current_version,
            parent_revision_id: None,
            workplace_id: "office-1".into(),
            queued_at: "2026-07-13T12:00:00Z".into(),
            content_hash: content_hash(payload.as_bytes()),
            payload,
        })
        .unwrap();

        ignore_inbox_revision("revision-ignore-1").unwrap();

        assert_eq!(list_inbox().unwrap()[0].status, InboxStatus::Ignored);
        assert!(luxifer_core::list_projects(&luxifer_core::projects_dir()).is_empty());
    }
}
