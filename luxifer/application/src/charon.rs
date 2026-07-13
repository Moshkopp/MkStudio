//! UI-unabhängiger Charon-Verbindungstest (ADR 0012).

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::AppError;

const PROTOCOL_VERSION: u32 = 1;
const TIMEOUT: Duration = Duration::from_millis(800);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(10);
const EVENT_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CharonHandshake {
    pub server: String,
    pub server_version: String,
    pub protocol_version: u32,
    pub instance_id: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CharonWorkplace {
    pub id: String,
    pub name: String,
    pub last_seen_unix: u64,
    pub online: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharonConnection {
    pub handshake: CharonHandshake,
    pub workplaces: Vec<CharonWorkplace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CharonBackupKind {
    UiSettings,
    LaserProfiles,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharonWorkplaceBackup {
    pub workplace_id: String,
    pub workplace_name: String,
    pub kind: CharonBackupKind,
    pub saved_at_unix: u64,
    pub content_hash: String,
    pub payload: String,
}

#[derive(Debug, Deserialize)]
struct WorkplaceBackupAck {
    workplace_id: String,
    kind: CharonBackupKind,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharonSyncReport {
    pub uploaded: usize,
    pub pending: usize,
    pub received: usize,
    pub assets_uploaded: usize,
    pub assets_downloaded: usize,
    pub backups_uploaded: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CharonRevision {
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
    meta: luxifer_core::AssetMeta,
    content_hex: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CharonProjectEvent {
    pub cursor: u64,
    pub changed: bool,
}

pub fn connect_charon(
    base_url: &str,
    workplace_id: &str,
    workplace_name: &str,
) -> Result<CharonConnection, AppError> {
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
            "charon_json",
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
    Ok(CharonConnection {
        handshake,
        workplaces,
    })
}

pub fn upload_workplace_backups(
    base_url: &str,
    settings: &luxifer_core::UiSettings,
    lasers: &luxifer_core::LaserRegistry,
) -> Result<usize, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let settings_payload = settings.to_json().map_err(|error| {
        AppError::new(
            "charon_backup_json",
            format!("Einstellungen sind ungültig: {error}"),
        )
    })?;
    let laser_payload = serde_json::to_string_pretty(lasers).map_err(|error| {
        AppError::wrap(
            "charon_backup_json",
            "Laserprofile sind ungültig.",
            error.to_string(),
        )
    })?;
    let mut stored = 0;
    for (kind, payload) in [
        (CharonBackupKind::UiSettings, settings_payload),
        (CharonBackupKind::LaserProfiles, laser_payload),
    ] {
        let backup = CharonWorkplaceBackup {
            workplace_id: settings.workplace_id.clone(),
            workplace_name: settings.workplace.clone(),
            kind,
            saved_at_unix: unix_now(),
            content_hash: luxifer_core::assets::content_hash(payload.as_bytes()),
            payload,
        };
        let body = serde_json::to_string(&backup).map_err(|error| {
            AppError::wrap(
                "charon_backup_json",
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
                "charon_backup_ack",
                "Charon hat eine unpassende Sicherungsbestätigung geliefert.",
            ));
        }
        stored += usize::from(ack.stored);
    }
    Ok(stored)
}

pub fn list_workplace_backups(base_url: &str) -> Result<Vec<CharonWorkplaceBackup>, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let backups: Vec<CharonWorkplaceBackup> = parse_json_response(&send_request(
        &endpoint,
        "GET",
        "/api/v1/workplaces/backups",
        "",
        UPLOAD_TIMEOUT,
    )?)?;
    for backup in &backups {
        if luxifer_core::assets::content_hash(backup.payload.as_bytes()) != backup.content_hash {
            return Err(AppError::new(
                "charon_backup_hash",
                "Eine Arbeitsplatzsicherung hat einen ungültigen Inhaltshash.",
            ));
        }
        match backup.kind {
            CharonBackupKind::UiSettings => {
                luxifer_core::UiSettings::from_json(&backup.payload).map_err(|error| {
                    AppError::new(
                        "charon_backup_payload",
                        format!("Gesicherte Einstellungen sind ungültig: {error}"),
                    )
                })?;
            }
            CharonBackupKind::LaserProfiles => {
                serde_json::from_str::<luxifer_core::LaserRegistry>(&backup.payload).map_err(
                    |error| {
                        AppError::wrap(
                            "charon_backup_payload",
                            "Gesicherte Laserprofile sind ungültig.",
                            error.to_string(),
                        )
                    },
                )?;
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
/// Charons Long-Poll-Timeout zurück. Das UI wird dabei nicht blockiert, weil
/// der Aufruf ausschließlich im Charon-Hintergrundthread läuft.
pub fn wait_for_project_event(
    base_url: &str,
    workplace_id: &str,
    after: u64,
) -> Result<CharonProjectEvent, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    parse_json_response(&send_request(
        &endpoint,
        "GET",
        &format!("/api/v1/events/projects?workplace_id={workplace_id}&after={after}"),
        "",
        EVENT_TIMEOUT,
    )?)
}

pub fn sync_assets(base_url: &str) -> Result<CharonSyncReport, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let mut remote: Vec<luxifer_core::AssetMeta> = parse_json_response(&send_request(
        &endpoint,
        "GET",
        "/api/v1/assets",
        "",
        UPLOAD_TIMEOUT,
    )?)?;
    let store = luxifer_core::assets_dir();
    let local = luxifer_core::list_assets(&store).map_err(|error| {
        AppError::wrap(
            "asset_list",
            "Asset-Katalog konnte nicht gelesen werden.",
            error.to_string(),
        )
    })?;
    let mut report = CharonSyncReport::default();
    for meta in &local {
        if remote.iter().any(|item| item.id == meta.id) {
            continue;
        }
        let bytes = luxifer_core::load_asset(&store, &meta.id).map_err(|error| {
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
        let _: luxifer_core::AssetMeta = parse_json_response(&send_request(
            &endpoint,
            "POST",
            "/api/v1/assets",
            &body,
            UPLOAD_TIMEOUT,
        )?)?;
        remote.push(meta.clone());
        report.assets_uploaded += 1;
    }
    for meta in remote {
        if local.iter().any(|item| item.id == meta.id) {
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
            AppError::new("asset_encoding", "Charon lieferte ungültige Asset-Daten.")
        })?;
        luxifer_core::store_asset(&store, &transfer.meta, &bytes).map_err(|error| {
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

pub fn upload_pending_revisions(base_url: &str) -> Result<CharonSyncReport, AppError> {
    let endpoint = HttpEndpoint::parse(base_url)?;
    let entries = crate::sync_outbox::list_outbox()?;
    let mut report = CharonSyncReport::default();
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
) -> Result<CharonSyncReport, AppError> {
    let mut report = upload_pending_revisions(base_url)?;
    let endpoint = HttpEndpoint::parse(base_url)?;
    let path = format!("/api/v1/projects/revisions?workplace_id={workplace_id}");
    let revisions: Vec<CharonRevision> =
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
    revision: &CharonRevision,
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
            "charon_json",
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
            "charon_receipt_ack",
            "Charon hat die Empfangsbestätigung nicht eindeutig angenommen.",
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
            "charon_json",
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
            "charon_revision_ack",
            "Charon hat die Projektrevision nicht eindeutig bestätigt.",
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
                "charon_address",
                "Charon-Adresse ist nicht auflösbar.",
                error.to_string(),
            )
        })?
        .next()
        .ok_or_else(|| AppError::new("charon_address", "Charon-Adresse ist nicht auflösbar."))?;
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT).map_err(|error| {
        AppError::wrap(
            "charon_connect",
            "Charon ist unter der eingestellten Adresse nicht erreichbar.",
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
            "charon_write",
            "Charon-Anfrage konnte nicht gesendet werden.",
            error.to_string(),
        )
    })?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).map_err(|error| {
        AppError::wrap(
            "charon_read",
            "Charon-Antwort konnte nicht gelesen werden.",
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
        let authority = raw.strip_prefix("http://").ok_or_else(|| {
            AppError::new("charon_url", "Charon-Adresse muss mit http:// beginnen.")
        })?;
        if authority.is_empty() || authority.contains('/') {
            return Err(AppError::new(
                "charon_url",
                "Charon-Adresse muss aus Host und optionalem Port bestehen.",
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
            "charon_response",
            "Charon hat ungültige Daten geliefert.",
            error.to_string(),
        )
    })?;
    let (headers, body) = text.split_once("\r\n\r\n").ok_or_else(|| {
        AppError::new(
            "charon_response",
            "Charon hat keine gültige HTTP-Antwort geliefert.",
        )
    })?;
    let status = headers.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        return Err(AppError::new(
            "charon_status",
            format!("Charon antwortet mit {status}."),
        ));
    }
    serde_json::from_str(body).map_err(|error| {
        AppError::wrap(
            "charon_json",
            "Charon-Antwort ist ungültig.",
            error.to_string(),
        )
    })
}

fn validate_handshake(handshake: &CharonHandshake) -> Result<(), AppError> {
    if handshake.server != "charon" || handshake.protocol_version != PROTOCOL_VERSION {
        return Err(AppError::new(
            "charon_protocol",
            format!(
                "Charon-Protokoll ist nicht kompatibel (erwartet {PROTOCOL_VERSION}, erhalten {}).",
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
        let response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"server\":\"charon\",\"server_version\":\"0.1.0\",\"protocol_version\":1,\"instance_id\":\"local-test\",\"capabilities\":[\"health\",\"handshake\"]}";
        let handshake: CharonHandshake = parse_json_response(response).unwrap();
        validate_handshake(&handshake).unwrap();
        assert_eq!(handshake.server_version, "0.1.0");
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
        let event: CharonProjectEvent = parse_json_response(response).unwrap();
        assert_eq!(event.cursor, 7);
        assert!(event.changed);
    }

    #[test]
    fn asset_hex_roundtrip_erhaelt_alle_bytes() {
        let bytes = b"\0LuxiFer\xff\x10";
        assert_eq!(hex_decode(&hex_encode(bytes)).unwrap(), bytes);
        assert!(hex_decode("abc").is_none());
        assert!(hex_decode("xx").is_none());
    }
}
