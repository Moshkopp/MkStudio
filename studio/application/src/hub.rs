//! UI-unabhängiger Hub-Verbindungstest (ADR 0012).

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::catalog_sync::{catalog_key, load_catalog_state, update_catalog_state, CatalogConflict};
use crate::AppError;

const PROTOCOL_VERSION: u32 = 3;
const TIMEOUT: Duration = Duration::from_millis(800);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(10);
const EVENT_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HubHandshake {
    pub server: String,
    pub server_version: String,
    pub protocol_version: u32,
    pub instance_id: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HubWorkplace {
    pub id: String,
    pub name: String,
    pub last_seen_unix: u64,
    pub online: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubConnection {
    pub handshake: HubHandshake,
    pub workplaces: Vec<HubWorkplace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HubBackupKind {
    UiSettings,
    LaserProfiles,
    MaterialProfiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    LaserProfile,
    MaterialProfile,
}

impl CatalogKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::LaserProfile => "laser_profile",
            Self::MaterialProfile => "material_profile",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SharedCatalogRecord {
    pub sequence: u64,
    pub kind: CatalogKind,
    pub id: String,
    pub deleted: bool,
    pub content_hash: String,
    pub payload: Option<String>,
    pub workplace_id: String,
    pub changed_at_unix: u64,
}

#[derive(Debug, Clone, Default)]
pub struct SharedCatalogSync {
    pub records: Vec<SharedCatalogRecord>,
    pub conflicts: Vec<CatalogConflict>,
}

#[derive(Serialize)]
struct CatalogChangeUpload<'a> {
    kind: CatalogKind,
    id: &'a str,
    base_hash: Option<&'a str>,
    deleted: bool,
    content_hash: &'a str,
    payload: Option<&'a str>,
    workplace_id: &'a str,
}

#[derive(Deserialize)]
struct CatalogChangeAck {
    accepted: bool,
    current: Option<SharedCatalogRecord>,
}

#[derive(Deserialize)]
struct CatalogChanges {
    records: Vec<SharedCatalogRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubWorkplaceBackup {
    pub workplace_id: String,
    pub workplace_name: String,
    pub kind: HubBackupKind,
    pub saved_at_unix: u64,
    pub content_hash: String,
    pub payload: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LeaseUsage {
    #[default]
    Idle,
    Running,
    Paused,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HubLease {
    pub controller_id: String,
    pub granted: bool,
    pub token: Option<String>,
    pub holder_name: Option<String>,
    pub holder_usage: Option<LeaseUsage>,
    pub expires_at_unix: Option<u64>,
    pub release_requested: bool,
    pub force_required: bool,
}

#[derive(Serialize)]
struct LeaseAcquire<'a> {
    controller_id: &'a str,
    controller_name: &'a str,
    workplace_id: &'a str,
    workplace_name: &'a str,
    force: bool,
}

#[derive(Serialize)]
struct LeaseHeartbeat<'a> {
    controller_id: &'a str,
    workplace_id: &'a str,
    token: &'a str,
    usage: LeaseUsage,
}

#[derive(Serialize)]
struct LeaseRelease<'a> {
    controller_id: &'a str,
    workplace_id: &'a str,
    token: &'a str,
}

#[derive(Debug, Deserialize)]
struct WorkplaceBackupAck {
    workplace_id: String,
    kind: HubBackupKind,
    content_hash: String,
    stored: bool,
}

#[derive(Serialize)]
struct WorkplaceHeartbeat<'a> {
    workplace_id: &'a str,
    workplace_name: &'a str,
}

#[derive(Serialize)]
struct RevisionUpload<'a> {
    revision_id: &'a str,
    project_id: &'a str,
    project_name: &'a str,
    project_version_id: &'a str,
    parent_revision_id: Option<&'a str>,
    workplace_id: &'a str,
    queued_at: &'a str,
    content_hash: &'a str,
    payload: &'a str,
}

#[derive(Deserialize)]
struct RevisionAck {
    revision_id: String,
    content_hash: String,
    #[allow(dead_code)]
    stored: bool,
}

#[derive(Debug, Deserialize)]
struct RevisionInventoryEntry {
    project_id: String,
    project_version_id: String,
    content_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HubSyncReport {
    pub uploaded: usize,
    pub pending: usize,
    pub received: usize,
    pub assets_uploaded: usize,
    pub assets_downloaded: usize,
    pub backups_uploaded: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HubRevision {
    pub revision_id: String,
    pub project_id: String,
    pub project_name: String,
    pub project_version_id: String,
    pub parent_revision_id: Option<String>,
    pub workplace_id: String,
    pub queued_at: String,
    pub content_hash: String,
    pub payload: String,
}

#[derive(Serialize)]
struct RevisionReceipt<'a> {
    workplace_id: &'a str,
    revision_id: &'a str,
    project_id: &'a str,
    content_hash: &'a str,
    received_at_unix: u64,
}

#[derive(Deserialize)]
struct ReceiptAck {
    revision_id: String,
    content_hash: String,
    accepted: bool,
}

#[derive(Serialize, Deserialize)]
struct AssetTransfer {
    meta: studio_core::AssetMeta,
    content_hex: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct HubProjectEvent {
    pub cursor: u64,
    pub changed: bool,
}

pub fn connect_hub(
    base_url: &str,
    workplace_id: &str,
    workplace_name: &str,
) -> Result<HubConnection, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let handshake = parse_json_response(&send_request(
        &endpoint,
        "GET",
        "/api/v1/handshake",
        "",
        TIMEOUT,
    )?)?;
    validate_handshake(&handshake)?;
    let body = serde_json::to_string(&WorkplaceHeartbeat {
        workplace_id,
        workplace_name,
    })
    .map_err(|error| {
        AppError::wrap(
            "hub_json",
            "Arbeitsplatzdaten sind ungültig.",
            error.to_string(),
        )
    })?;
    let workplaces = parse_json_response(&send_request(
        &endpoint,
        "POST",
        "/api/v1/workplaces/heartbeat",
        &body,
        TIMEOUT,
    )?)?;
    Ok(HubConnection {
        handshake,
        workplaces,
    })
}

pub fn acquire_lease(
    base_url: &str,
    controller_id: &str,
    controller_name: &str,
    workplace_id: &str,
    workplace_name: &str,
    force: bool,
) -> Result<HubLease, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let body = serde_json::to_string(&LeaseAcquire {
        controller_id,
        controller_name,
        workplace_id,
        workplace_name,
        force,
    })
    .map_err(|error| AppError::new("hub_lease_json", error.to_string()))?;
    parse_json_response(&send_request(
        &endpoint,
        "POST",
        "/api/v1/leases/acquire",
        &body,
        TIMEOUT,
    )?)
}

pub fn heartbeat_lease(
    base_url: &str,
    controller_id: &str,
    workplace_id: &str,
    token: &str,
    usage: LeaseUsage,
) -> Result<HubLease, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let body = serde_json::to_string(&LeaseHeartbeat {
        controller_id,
        workplace_id,
        token,
        usage,
    })
    .map_err(|error| AppError::new("hub_lease_json", error.to_string()))?;
    parse_json_response(&send_request(
        &endpoint,
        "POST",
        "/api/v1/leases/heartbeat",
        &body,
        TIMEOUT,
    )?)
}

pub fn release_lease(
    base_url: &str,
    controller_id: &str,
    workplace_id: &str,
    token: &str,
) -> Result<bool, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let body = serde_json::to_string(&LeaseRelease {
        controller_id,
        workplace_id,
        token,
    })
    .map_err(|error| AppError::new("hub_lease_json", error.to_string()))?;
    let response: serde_json::Value = parse_json_response(&send_request(
        &endpoint,
        "POST",
        "/api/v1/leases/release",
        &body,
        TIMEOUT,
    )?)?;
    Ok(response["released"].as_bool().unwrap_or(false))
}

pub fn upload_workplace_backups(
    base_url: &str,
    settings: &studio_core::UiSettings,
    lasers: &studio_core::LaserRegistry,
    materials: &studio_core::MaterialLibrary,
) -> Result<usize, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let settings_payload = settings.to_json().map_err(|error| {
        AppError::new(
            "hub_backup_json",
            format!("Einstellungen sind ungültig: {error}"),
        )
    })?;
    // Aktive Auswahlen sind Arbeitsplatz-Zustand und gehören nicht in eine
    // übertragbare Profil-Sicherung.
    let mut shared_lasers = lasers.clone();
    shared_lasers.active_id = None;
    let mut shared_materials = materials.clone();
    shared_materials.active_by_laser.clear();
    let laser_payload = serde_json::to_string_pretty(&shared_lasers).map_err(|error| {
        AppError::wrap(
            "hub_backup_json",
            "Laserprofile sind ungültig.",
            error.to_string(),
        )
    })?;
    let material_payload = serde_json::to_string_pretty(&shared_materials).map_err(|error| {
        AppError::wrap(
            "hub_backup_json",
            "Materialprofile sind ungültig.",
            error.to_string(),
        )
    })?;
    let mut stored = 0;
    for (kind, payload) in [
        (HubBackupKind::UiSettings, settings_payload),
        (HubBackupKind::LaserProfiles, laser_payload),
        (HubBackupKind::MaterialProfiles, material_payload),
    ] {
        let backup = HubWorkplaceBackup {
            workplace_id: settings.workplace_id.clone(),
            workplace_name: settings.workplace.clone(),
            kind,
            saved_at_unix: unix_now(),
            content_hash: studio_core::assets::content_hash(payload.as_bytes()),
            payload,
        };
        let body = serde_json::to_string(&backup).map_err(|error| {
            AppError::wrap(
                "hub_backup_json",
                "Arbeitsplatzsicherung konnte nicht serialisiert werden.",
                error.to_string(),
            )
        })?;
        let ack: WorkplaceBackupAck = parse_json_response(&send_request(
            &endpoint,
            "POST",
            "/api/v1/workplaces/backups",
            &body,
            UPLOAD_TIMEOUT,
        )?)?;
        if ack.workplace_id != settings.workplace_id
            || ack.kind != kind
            || ack.content_hash != backup.content_hash
        {
            return Err(AppError::new(
                "hub_backup_ack",
                "Hub hat eine unpassende Sicherungsbestätigung geliefert.",
            ));
        }
        stored += usize::from(ack.stored);
    }
    Ok(stored)
}

pub fn list_workplace_backups(base_url: &str) -> Result<Vec<HubWorkplaceBackup>, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let backups: Vec<HubWorkplaceBackup> = parse_json_response(&send_request(
        &endpoint,
        "GET",
        "/api/v1/workplaces/backups",
        "",
        UPLOAD_TIMEOUT,
    )?)?;
    for backup in &backups {
        if studio_core::assets::content_hash(backup.payload.as_bytes()) != backup.content_hash {
            return Err(AppError::new(
                "hub_backup_hash",
                "Eine Arbeitsplatzsicherung hat einen ungültigen Inhaltshash.",
            ));
        }
        match backup.kind {
            HubBackupKind::UiSettings => {
                studio_core::UiSettings::from_json(&backup.payload).map_err(|error| {
                    AppError::new(
                        "hub_backup_payload",
                        format!("Gesicherte Einstellungen sind ungültig: {error}"),
                    )
                })?;
            }
            HubBackupKind::LaserProfiles => {
                serde_json::from_str::<studio_core::LaserRegistry>(&backup.payload).map_err(
                    |error| {
                        AppError::wrap(
                            "hub_backup_payload",
                            "Gesicherte Laserprofile sind ungültig.",
                            error.to_string(),
                        )
                    },
                )?;
            }
            HubBackupKind::MaterialProfiles => {
                let library = serde_json::from_str::<studio_core::MaterialLibrary>(&backup.payload)
                    .map_err(|error| {
                        AppError::wrap(
                            "hub_backup_payload",
                            "Gesicherte Materialprofile sind ungültig.",
                            error.to_string(),
                        )
                    })?;
                for profile in &library.profiles {
                    profile
                        .validate()
                        .map_err(|message| AppError::new("hub_backup_payload", message))?;
                }
            }
        }
    }
    Ok(backups)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

/// Wartet serverseitig auf eine neue Projektrevision und kehrt spätestens nach
/// Hubs Long-Poll-Timeout zurück. Das UI wird dabei nicht blockiert, weil
/// der Aufruf ausschließlich im Hub-Hintergrundthread läuft.
pub fn wait_for_project_event(
    base_url: &str,
    workplace_id: &str,
    after: u64,
) -> Result<HubProjectEvent, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    parse_json_response(&send_request(
        &endpoint,
        "GET",
        &format!("/api/v1/events/projects?workplace_id={workplace_id}&after={after}"),
        "",
        EVENT_TIMEOUT,
    )?)
}

pub fn sync_assets(base_url: &str) -> Result<HubSyncReport, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let mut remote: Vec<studio_core::AssetMeta> = parse_json_response(&send_request(
        &endpoint,
        "GET",
        "/api/v1/assets",
        "",
        UPLOAD_TIMEOUT,
    )?)?;
    let store = studio_core::assets_dir();
    let local = studio_core::list_assets(&store).map_err(|error| {
        AppError::wrap(
            "asset_list",
            "Asset-Katalog konnte nicht gelesen werden.",
            error.to_string(),
        )
    })?;
    let mut report = HubSyncReport::default();
    for meta in &local {
        if remote
            .iter()
            .any(|item| item.id == meta.id && meta.tags.iter().all(|tag| item.tags.contains(tag)))
        {
            continue;
        }
        let bytes = studio_core::load_asset(&store, &meta.id).map_err(|error| {
            AppError::wrap(
                "asset_read",
                "Asset konnte nicht gelesen werden.",
                error.to_string(),
            )
        })?;
        let body = serde_json::to_string(&AssetTransfer {
            meta: meta.clone(),
            content_hex: hex_encode(&bytes),
        })
        .map_err(|error| {
            AppError::wrap(
                "asset_json",
                "Asset konnte nicht serialisiert werden.",
                error.to_string(),
            )
        })?;
        let _: studio_core::AssetMeta = parse_json_response(&send_request(
            &endpoint,
            "POST",
            "/api/v1/assets",
            &body,
            UPLOAD_TIMEOUT,
        )?)?;
        if let Some(existing) = remote.iter_mut().find(|item| item.id == meta.id) {
            for tag in &meta.tags {
                if !existing.tags.contains(tag) {
                    existing.tags.push(tag.clone());
                }
            }
        } else {
            remote.push(meta.clone());
        }
        report.assets_uploaded += 1;
    }
    for meta in remote {
        if studio_core::asset_hidden(&store, &meta.id) {
            continue;
        }
        if local
            .iter()
            .any(|item| item.id == meta.id && meta.tags.iter().all(|tag| item.tags.contains(tag)))
        {
            continue;
        }
        let transfer: AssetTransfer = parse_json_response(&send_request(
            &endpoint,
            "GET",
            &format!("/api/v1/assets/{}", meta.id),
            "",
            UPLOAD_TIMEOUT,
        )?)?;
        let bytes = hex_decode(&transfer.content_hex).ok_or_else(|| {
            AppError::new("asset_encoding", "Hub lieferte ungültige Asset-Daten.")
        })?;
        studio_core::store_asset(&store, &transfer.meta, &bytes).map_err(|error| {
            AppError::wrap(
                "asset_write",
                "Asset konnte nicht gespeichert werden.",
                error.to_string(),
            )
        })?;
        report.assets_downloaded += 1;
    }
    Ok(report)
}

/// Synchronisiert die gemeinsamen Laser- und Materialprofile. Lokale
/// Änderungen kommen aus einer persistenten Outbox; Serverkonflikte werden
/// zur bewussten Auflösung gemeldet und niemals still überschrieben.
pub fn sync_shared_catalog(
    base_url: &str,
    workplace_id: &str,
) -> Result<SharedCatalogSync, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let pending = load_catalog_state()?.pending;
    for change in pending {
        let body = serde_json::to_string(&CatalogChangeUpload {
            kind: change.kind,
            id: &change.id,
            base_hash: change.base_hash.as_deref(),
            deleted: change.payload.is_none(),
            content_hash: &change.content_hash,
            payload: change.payload.as_deref(),
            workplace_id,
        })
        .map_err(|error| AppError::new("catalog_json", error.to_string()))?;
        let ack: CatalogChangeAck = parse_json_response(&send_request(
            &endpoint,
            "POST",
            "/api/v1/catalog/changes",
            &body,
            UPLOAD_TIMEOUT,
        )?)?;
        update_catalog_state(|state| {
            let key = catalog_key(change.kind, &change.id);
            if ack.accepted {
                if let Some(current) = &ack.current {
                    state.known_hashes.insert(key, current.content_hash.clone());
                }
            } else if let Some(current) = &ack.current {
                let conflict = CatalogConflict {
                    kind: change.kind,
                    id: change.id.clone(),
                    local_hash: change.content_hash.clone(),
                    remote_hash: current.content_hash.clone(),
                };
                if !state.conflicts.iter().any(|existing| {
                    existing.kind == conflict.kind
                        && existing.id == conflict.id
                        && existing.local_hash == conflict.local_hash
                        && existing.remote_hash == conflict.remote_hash
                }) {
                    state.conflicts.push(conflict);
                }
                state.known_hashes.insert(key, current.content_hash.clone());
            }
            // Eine inzwischen erneut bearbeitete Version derselben ID bleibt
            // in der Outbox und wird beim nächsten Durchlauf übertragen.
            state.pending.retain(|item| {
                !(item.kind == change.kind
                    && item.id == change.id
                    && item.content_hash == change.content_hash)
            });
        })?;
    }

    // Der Server liefert pro ID nur den aktuellen Stand. Ein Vollabgleich ist
    // deshalb klein, idempotent und nach einem Client-Absturz selbstheilend.
    let mut changes: CatalogChanges = parse_json_response(&send_request(
        &endpoint,
        "GET",
        "/api/v1/catalog/changes?after=0",
        "",
        UPLOAD_TIMEOUT,
    )?)?;
    update_catalog_state(|state| {
        for record in &changes.records {
            state.known_hashes.insert(
                catalog_key(record.kind, &record.id),
                record.content_hash.clone(),
            );
        }
    })?;
    let state = load_catalog_state()?;
    // Solange ein Konflikt nicht bewusst aufgelöst wurde, bleibt der lokale
    // Stand unangetastet. Insbesondere der erste Abgleich eines bestehenden
    // Rechners darf nicht unbemerkt vom Serverstand überschrieben werden.
    changes.records.retain(|record| {
        !state
            .conflicts
            .iter()
            .any(|conflict| conflict.kind == record.kind && conflict.id == record.id)
    });
    Ok(SharedCatalogSync {
        records: changes.records,
        conflicts: state.conflicts,
    })
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let hi = (pair[0] as char).to_digit(16)?;
            let lo = (pair[1] as char).to_digit(16)?;
            Some(((hi << 4) | lo) as u8)
        })
        .collect()
}

pub fn upload_pending_revisions(base_url: &str) -> Result<HubSyncReport, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let entries = crate::sync_outbox::list_outbox()?;
    let mut report = HubSyncReport::default();
    for entry in entries
        .into_iter()
        .filter(|entry| entry.status != crate::OutboxStatus::Uploaded)
    {
        report.pending += 1;
        let result = upload_revision(&endpoint, &entry);
        match result {
            Ok(()) => {
                crate::sync_outbox::set_outbox_status(
                    &entry.revision_id,
                    crate::OutboxStatus::Uploaded,
                    None,
                )?;
                report.uploaded += 1;
                report.pending -= 1;
            }
            Err(error) => {
                crate::sync_outbox::set_outbox_status(
                    &entry.revision_id,
                    crate::OutboxStatus::Failed,
                    Some(error.message().to_owned()),
                )?;
                return Err(error);
            }
        }
    }
    Ok(report)
}

pub fn sync_project_revisions(
    base_url: &str,
    workplace_id: &str,
) -> Result<HubSyncReport, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    crate::sync_outbox::seed_saved_projects(workplace_id)?;
    let inventory: Vec<RevisionInventoryEntry> = parse_json_response(&send_request(
        &endpoint,
        "GET",
        "/api/v1/projects/inventory",
        "",
        UPLOAD_TIMEOUT,
    )?)?;
    let entries = crate::sync_outbox::list_outbox()?;
    let mut report = HubSyncReport::default();
    for entry in entries {
        let present = inventory.iter().any(|remote| {
            remote.project_id == entry.project_id
                && remote.project_version_id == entry.project_version_id
                && remote.content_hash == entry.content_hash
        });
        if present {
            if entry.status != crate::OutboxStatus::Uploaded {
                crate::sync_outbox::set_outbox_status(
                    &entry.revision_id,
                    crate::OutboxStatus::Uploaded,
                    None,
                )?;
            }
            continue;
        }
        report.pending += 1;
        match upload_revision(&endpoint, &entry) {
            Ok(()) => {
                crate::sync_outbox::set_outbox_status(
                    &entry.revision_id,
                    crate::OutboxStatus::Uploaded,
                    None,
                )?;
                report.uploaded += 1;
                report.pending -= 1;
            }
            Err(error) => {
                crate::sync_outbox::set_outbox_status(
                    &entry.revision_id,
                    crate::OutboxStatus::Failed,
                    Some(error.message().to_owned()),
                )?;
                return Err(error);
            }
        }
    }
    let path = format!("/api/v1/projects/revisions?workplace_id={workplace_id}");
    let revisions: Vec<HubRevision> =
        parse_json_response(&send_request(&endpoint, "GET", &path, "", UPLOAD_TIMEOUT)?)?;
    for revision in revisions {
        let is_new = crate::sync_inbox::store_remote_revision(revision.clone())?;
        acknowledge_revision(&endpoint, workplace_id, &revision)?;
        if is_new {
            report.received += 1;
        }
    }
    Ok(report)
}

fn acknowledge_revision(
    endpoint: &HttpEndpoint,
    workplace_id: &str,
    revision: &HubRevision,
) -> Result<(), AppError> {
    let body = serde_json::to_string(&RevisionReceipt {
        workplace_id,
        revision_id: &revision.revision_id,
        project_id: &revision.project_id,
        content_hash: &revision.content_hash,
        received_at_unix: 0,
    })
    .map_err(|error| {
        AppError::wrap(
            "hub_json",
            "Empfangsbestätigung konnte nicht serialisiert werden.",
            error.to_string(),
        )
    })?;
    let ack: ReceiptAck = parse_json_response(&send_request(
        endpoint,
        "POST",
        "/api/v1/projects/revisions/ack",
        &body,
        TIMEOUT,
    )?)?;
    if !ack.accepted
        || ack.revision_id != revision.revision_id
        || ack.content_hash != revision.content_hash
    {
        return Err(AppError::new(
            "hub_receipt_ack",
            "Hub hat die Empfangsbestätigung nicht eindeutig angenommen.",
        ));
    }
    Ok(())
}

fn upload_revision(endpoint: &HttpEndpoint, entry: &crate::OutboxEntry) -> Result<(), AppError> {
    let payload = std::fs::read_to_string(entry.payload_path()).map_err(|error| {
        AppError::wrap(
            "outbox_payload_read",
            "Vorgemerkte Projektrevision konnte nicht gelesen werden.",
            error.to_string(),
        )
    })?;
    let body = serde_json::to_string(&RevisionUpload {
        revision_id: &entry.revision_id,
        project_id: &entry.project_id,
        project_name: &entry.project_name,
        project_version_id: &entry.project_version_id,
        parent_revision_id: entry.parent_revision_id.as_deref(),
        workplace_id: &entry.workplace_id,
        queued_at: &entry.queued_at,
        content_hash: &entry.content_hash,
        payload: &payload,
    })
    .map_err(|error| {
        AppError::wrap(
            "hub_json",
            "Projektrevision konnte nicht serialisiert werden.",
            error.to_string(),
        )
    })?;
    let ack: RevisionAck = parse_json_response(&send_request(
        endpoint,
        "POST",
        "/api/v1/projects/revisions",
        &body,
        UPLOAD_TIMEOUT,
    )?)?;
    if ack.revision_id != entry.revision_id || ack.content_hash != entry.content_hash {
        return Err(AppError::new(
            "hub_revision_ack",
            "Hub hat die Projektrevision nicht eindeutig bestätigt.",
        ));
    }
    Ok(())
}

fn send_request(
    endpoint: &HttpEndpoint,
    method: &str,
    path: &str,
    body: &str,
    timeout: Duration,
) -> Result<Vec<u8>, AppError> {
    let address = endpoint
        .authority
        .to_socket_addrs()
        .map_err(|error| {
            AppError::wrap(
                "hub_address",
                "Hub-Adresse ist nicht auflösbar.",
                error.to_string(),
            )
        })?
        .next()
        .ok_or_else(|| AppError::new("hub_address", "Hub-Adresse ist nicht auflösbar."))?;
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT).map_err(|error| {
        AppError::wrap(
            "hub_connect",
            "Hub ist unter der eingestellten Adresse nicht erreichbar.",
            error.to_string(),
        )
    })?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        endpoint.authority,
        body.len()
    );
    stream.write_all(request.as_bytes()).map_err(|error| {
        AppError::wrap(
            "hub_write",
            "Hub-Anfrage konnte nicht gesendet werden.",
            error.to_string(),
        )
    })?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).map_err(|error| {
        AppError::wrap(
            "hub_read",
            "Hub-Antwort konnte nicht gelesen werden.",
            error.to_string(),
        )
    })?;
    Ok(response)
}

struct HttpEndpoint {
    authority: String,
}

impl HttpEndpoint {
    fn parse(raw: &str) -> Result<Self, AppError> {
        let raw = raw.trim().trim_end_matches('/');
        let authority = raw
            .strip_prefix("http://")
            .ok_or_else(|| AppError::new("hub_url", "Hub-Adresse muss mit http:// beginnen."))?;
        if authority.is_empty() || authority.contains('/') {
            return Err(AppError::new(
                "hub_url",
                "Hub-Adresse muss aus Host und optionalem Port bestehen.",
            ));
        }
        let authority = if authority.contains(':') {
            authority.into()
        } else {
            format!("{authority}:80")
        };
        Ok(Self { authority })
    }
}

fn parse_json_response<T: for<'de> Deserialize<'de>>(response: &[u8]) -> Result<T, AppError> {
    let text = std::str::from_utf8(response).map_err(|error| {
        AppError::wrap(
            "hub_response",
            "Hub hat ungültige Daten geliefert.",
            error.to_string(),
        )
    })?;
    let (headers, body) = text.split_once("\r\n\r\n").ok_or_else(|| {
        AppError::new(
            "hub_response",
            "Hub hat keine gültige HTTP-Antwort geliefert.",
        )
    })?;
    let status = headers.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        return Err(AppError::new(
            "hub_status",
            format!("Hub antwortet mit {status}."),
        ));
    }
    serde_json::from_str(body)
        .map_err(|error| AppError::wrap("hub_json", "Hub-Antwort ist ungültig.", error.to_string()))
}

fn validate_handshake(handshake: &HubHandshake) -> Result<(), AppError> {
    if handshake.server != studio_core::branding::HUB_PROTOCOL_ID {
        return Err(AppError::new(
            "hub_protocol",
            format!(
                "Hub-Identität ist nicht kompatibel (erwartet {}, erhalten {}).",
                studio_core::branding::HUB_PROTOCOL_ID,
                handshake.server
            ),
        ));
    }
    if handshake.protocol_version != PROTOCOL_VERSION {
        return Err(AppError::new(
            "hub_protocol",
            format!(
                "Hub-Protokoll ist nicht kompatibel (erwartet {PROTOCOL_VERSION}, erhalten {}).",
                handshake.protocol_version
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_akzeptiert_gueltigen_handshake() {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{{\"server\":\"{}\",\"server_version\":\"1.0.0\",\"protocol_version\":3,\"instance_id\":\"local-test\",\"capabilities\":[\"health\",\"handshake\"]}}",
            studio_core::branding::HUB_PROTOCOL_ID
        );
        let handshake: HubHandshake = parse_json_response(response.as_bytes()).unwrap();
        validate_handshake(&handshake).unwrap();
        assert_eq!(handshake.server_version, "1.0.0");
        assert!(handshake.capabilities.contains(&"handshake".into()));
    }

    #[test]
    fn url_verlangt_http_und_reines_ziel() {
        assert!(HttpEndpoint::parse("https://localhost:3737").is_err());
        assert!(HttpEndpoint::parse("http://localhost:3737/pfad").is_err());
        assert_eq!(
            HttpEndpoint::parse("http://localhost:3737/")
                .unwrap()
                .authority,
            "localhost:3737"
        );
    }

    #[test]
    fn projekt_event_traegt_monotonen_cursor() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"cursor\":7,\"changed\":true}";
        let event: HubProjectEvent = parse_json_response(response).unwrap();
        assert_eq!(event.cursor, 7);
        assert!(event.changed);
    }

    #[test]
    fn asset_hex_roundtrip_erhaelt_alle_bytes() {
        let bytes = b"\0Studio\xff\x10";
        assert_eq!(hex_decode(&hex_encode(bytes)).unwrap(), bytes);
        assert!(hex_decode("abc").is_none());
        assert!(hex_decode("xx").is_none());
    }
}
