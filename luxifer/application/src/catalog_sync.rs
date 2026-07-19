//! Dauerhafte Outbox und Sync-Metadaten für gemeinsame Laser-/Materialprofile.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

use serde::{Deserialize, Serialize};

use crate::{AppError, CatalogKind};

const STATE_FILE: &str = "sync/shared-catalog.json";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static STATE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PendingCatalogChange {
    pub kind: CatalogKind,
    pub id: String,
    pub base_hash: Option<String>,
    pub content_hash: String,
    pub payload: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogConflict {
    pub kind: CatalogKind,
    pub id: String,
    pub local_hash: String,
    pub remote_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct CatalogSyncState {
    #[serde(default)]
    pub known_hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub pending: Vec<PendingCatalogChange>,
    #[serde(default)]
    pub conflicts: Vec<CatalogConflict>,
}

pub(crate) fn enqueue_catalog_profile<T: Serialize>(
    kind: CatalogKind,
    id: &str,
    profile: Option<&T>,
) -> Result<(), AppError> {
    let payload = profile
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| AppError::new("catalog_json", error.to_string()))?;
    let content_hash =
        luxifer_core::assets::content_hash(payload.as_deref().unwrap_or_default().as_bytes());
    update_catalog_state(|state| {
        let key = catalog_key(kind, id);
        if let Some(pending) = state
            .pending
            .iter_mut()
            .find(|change| change.kind == kind && change.id == id)
        {
            pending.content_hash = content_hash;
            pending.payload = payload;
        } else {
            state.pending.push(PendingCatalogChange {
                kind,
                id: id.to_owned(),
                base_hash: state.known_hashes.get(&key).cloned(),
                content_hash,
                payload,
            });
        }
    })
}

pub fn seed_shared_catalog(
    lasers: &[luxifer_core::LaserProfile],
    materials: &[luxifer_core::MaterialProfile],
) -> Result<(), AppError> {
    for profile in lasers {
        seed_profile(CatalogKind::LaserProfile, &profile.id, profile)?;
    }
    for profile in materials {
        seed_profile(CatalogKind::MaterialProfile, &profile.id, profile)?;
    }
    Ok(())
}

fn seed_profile<T: Serialize>(kind: CatalogKind, id: &str, profile: &T) -> Result<(), AppError> {
    let state = load_catalog_state()?;
    let key = catalog_key(kind, id);
    if state.known_hashes.contains_key(&key)
        || state
            .pending
            .iter()
            .any(|change| change.kind == kind && change.id == id)
    {
        return Ok(());
    }
    enqueue_catalog_profile(kind, id, Some(profile))
}

pub(crate) fn load_catalog_state() -> Result<CatalogSyncState, AppError> {
    let _guard = STATE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    load_catalog_state_unlocked()
}

fn load_catalog_state_unlocked() -> Result<CatalogSyncState, AppError> {
    let path = state_path();
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| AppError::new("catalog_state_json", error.to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(CatalogSyncState::default())
        }
        Err(error) => Err(AppError::wrap(
            "catalog_state_read",
            "Profil-Synchronisierung konnte nicht gelesen werden.",
            error.to_string(),
        )),
    }
}

fn save_catalog_state_unlocked(state: &CatalogSyncState) -> Result<(), AppError> {
    let path = state_path();
    let parent = path.parent().expect("Katalogzustand hat Elternpfad");
    std::fs::create_dir_all(parent)
        .map_err(|error| AppError::new("catalog_state_write", error.to_string()))?;
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|error| AppError::new("catalog_state_json", error.to_string()))?;
    let temp = path.with_extension(format!(
        "json.{}.{}.tmp",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&temp, bytes)
        .map_err(|error| AppError::new("catalog_state_write", error.to_string()))?;
    std::fs::rename(temp, path)
        .map_err(|error| AppError::new("catalog_state_write", error.to_string()))
}

pub(crate) fn update_catalog_state(
    update: impl FnOnce(&mut CatalogSyncState),
) -> Result<(), AppError> {
    let _guard = STATE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let mut state = load_catalog_state_unlocked()?;
    update(&mut state);
    save_catalog_state_unlocked(&state)
}

pub(crate) fn catalog_key(kind: CatalogKind, id: &str) -> String {
    format!("{}:{id}", kind.as_str())
}

fn state_path() -> PathBuf {
    luxifer_core::data_root().join(STATE_FILE)
}
