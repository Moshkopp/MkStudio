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
}
