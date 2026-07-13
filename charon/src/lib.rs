//! Charon — optionaler lokaler Koordinationsdienst für LuxiFer.
//! Der erste Schnitt stellt nur Health und Handshake bereit (ADR 0012).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const DEFAULT_BIND: &str = "127.0.0.1:3737";
pub const NETWORK_OPT_IN_ENV: &str = "CHARON_ALLOW_NETWORK";
pub const PROTOCOL_VERSION: u32 = 1;
const ONLINE_TIMEOUT_SECS: u64 = 15;
const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct ServerConfig {
    pub bind: SocketAddr,
}

impl ServerConfig {
    pub fn from_env() -> std::io::Result<Self> {
        let raw = std::env::var("CHARON_BIND").unwrap_or_else(|_| DEFAULT_BIND.into());
        let allow_network = std::env::var(NETWORK_OPT_IN_ENV).ok().is_some_and(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        });
        Self::from_values(&raw, allow_network)
    }

    fn from_values(raw: &str, allow_network: bool) -> std::io::Result<Self> {
        let bind: SocketAddr = raw.parse().map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Ungültiges CHARON_BIND '{raw}': {error}"),
            )
        })?;
        if !bind.ip().is_loopback() && !allow_network {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "CHARON_BIND '{raw}' gibt Charon im Netzwerk frei. \
                     Setze {NETWORK_OPT_IN_ENV}=1 nur in einem vertrauenswürdigen internen Netz."
                ),
            ));
        }
        Ok(Self { bind })
    }
}

#[derive(Serialize)]
struct Health<'a> {
    status: &'a str,
}

#[derive(Serialize)]
struct Handshake {
    server: &'static str,
    server_version: &'static str,
    protocol_version: u32,
    instance_id: String,
    capabilities: [&'static str; 8],
}

#[derive(Serialize)]
struct ProjectEvent {
    cursor: u64,
    changed: bool,
}

#[derive(Serialize, Deserialize)]
struct AssetTransfer {
    meta: luxifer_core::AssetMeta,
    content_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkplaceBackup {
    workplace_id: String,
    workplace_name: String,
    kind: String,
    saved_at_unix: u64,
    content_hash: String,
    payload: String,
}

#[derive(Debug, Serialize)]
struct WorkplaceBackupAck {
    workplace_id: String,
    kind: String,
    content_hash: String,
    stored: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LeaseUsage {
    Idle,
    Running,
    Paused,
    Unknown,
}

#[derive(Debug, Deserialize)]
struct LeaseAcquire {
    controller_id: String,
    controller_name: String,
    workplace_id: String,
    workplace_name: String,
    force: bool,
}

#[derive(Debug, Deserialize)]
struct LeaseHeartbeat {
    controller_id: String,
    workplace_id: String,
    token: String,
    usage: LeaseUsage,
}

#[derive(Debug, Deserialize)]
struct LeaseRelease {
    controller_id: String,
    workplace_id: String,
    token: String,
}

#[derive(Debug, Serialize)]
struct LeaseReply {
    controller_id: String,
    granted: bool,
    token: Option<String>,
    holder_name: Option<String>,
    holder_usage: Option<LeaseUsage>,
    expires_at_unix: Option<u64>,
    release_requested: bool,
    force_required: bool,
}

#[derive(Debug, Clone)]
struct LeaseEntry {
    token: String,
    workplace_id: String,
    workplace_name: String,
    usage: LeaseUsage,
    expires_at_unix: u64,
    release_requested_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkplaceHeartbeat {
    workplace_id: String,
    workplace_name: String,
}

#[derive(Debug, Clone)]
struct WorkplaceEntry {
    name: String,
    last_seen_unix: u64,
}

#[derive(Debug, Serialize)]
struct WorkplacePresence {
    id: String,
    name: String,
    last_seen_unix: u64,
    online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RevisionUpload {
    revision_id: String,
    project_id: String,
    project_name: String,
    project_version_id: String,
    parent_revision_id: Option<String>,
    workplace_id: String,
    queued_at: String,
    content_hash: String,
    payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRevision {
    revision_id: String,
    project_id: String,
    project_name: String,
    project_version_id: String,
    parent_revision_id: Option<String>,
    workplace_id: String,
    queued_at: String,
    content_hash: String,
}

#[derive(Debug, Serialize)]
struct RevisionAck {
    revision_id: String,
    content_hash: String,
    stored: bool,
}

#[derive(Debug, Serialize)]
struct RevisionDownload {
    revision_id: String,
    project_id: String,
    project_name: String,
    project_version_id: String,
    parent_revision_id: Option<String>,
    workplace_id: String,
    queued_at: String,
    content_hash: String,
    payload: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RevisionReceipt {
    workplace_id: String,
    revision_id: String,
    project_id: String,
    content_hash: String,
    received_at_unix: u64,
}

#[derive(Debug, Serialize)]
struct ReceiptAck {
    revision_id: String,
    content_hash: String,
    accepted: bool,
}

struct ServerState {
    workplaces: BTreeMap<String, WorkplaceEntry>,
    data_dir: PathBuf,
    revision_cursor: u64,
    leases: BTreeMap<String, LeaseEntry>,
}

impl ServerState {
    fn new(data_dir: PathBuf) -> Self {
        Self {
            workplaces: BTreeMap::new(),
            data_dir,
            revision_cursor: 0,
            leases: BTreeMap::new(),
        }
    }
}

struct SharedState {
    state: Mutex<ServerState>,
    revision_changed: Condvar,
}

pub fn serve(config: ServerConfig) -> std::io::Result<()> {
    let listener = TcpListener::bind(config.bind)?;
    let state = Arc::new(SharedState {
        state: Mutex::new(ServerState::new(charon_data_dir())),
        revision_changed: Condvar::new(),
    });
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let state = Arc::clone(&state);
                std::thread::spawn(move || {
                    if let Err(error) = serve_connection(&mut stream, &state) {
                        eprintln!("Charon-Verbindungsfehler: {error}");
                    }
                });
            }
            Err(error) => eprintln!("Charon-Accept-Fehler: {error}"),
        }
    }
    Ok(())
}

fn serve_connection(stream: &mut TcpStream, shared: &SharedState) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
    let request = read_http_request(stream)?;
    let first_line = request.lines().next().unwrap_or_default().to_string();
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    let body = request.split_once("\r\n\r\n").map_or("", |(_, body)| body);

    let (status, body) = if method == "GET" && path.starts_with("/api/v1/events/projects?") {
        let query = path.split_once('?').map_or("", |(_, query)| query);
        let workplace_id = query_value(query, "workplace_id").unwrap_or_default();
        let after = query_value(query, "after").and_then(|value| value.parse::<u64>().ok());
        if !valid_id(workplace_id) || after.is_none() {
            (
                "400 Bad Request",
                r#"{"error":"invalid_event_cursor"}"#.into(),
            )
        } else {
            let after = after.unwrap_or_default();
            let state = shared
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let (state, _) = shared
                .revision_changed
                .wait_timeout_while(state, std::time::Duration::from_secs(4), |state| {
                    state.revision_cursor <= after
                })
                .unwrap_or_else(|poison| poison.into_inner());
            json_body(&ProjectEvent {
                cursor: state.revision_cursor,
                changed: state.revision_cursor > after,
            })?
        }
    } else {
        let mut state = shared
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let result = route(method, path, body, &mut state)?;
        if method == "POST" && path == "/api/v1/projects/revisions" && result.0.starts_with("200") {
            state.revision_cursor = state.revision_cursor.saturating_add(1);
            shared.revision_changed.notify_all();
        }
        result
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())
}

fn route(
    method: &str,
    path: &str,
    body: &str,
    state: &mut ServerState,
) -> std::io::Result<(&'static str, String)> {
    let (path, query) = path.split_once('?').map_or((path, ""), |parts| parts);
    Ok(match (method, path) {
        ("GET", "/health") => json_body(&Health { status: "ok" })?,
        ("GET", "/api/v1/handshake") => json_body(&Handshake {
            server: "charon",
            server_version: env!("CARGO_PKG_VERSION"),
            protocol_version: PROTOCOL_VERSION,
            instance_id: format!("local-{}", std::process::id()),
            capabilities: [
                "health",
                "handshake",
                "workplaces",
                "project_revisions",
                "project_events",
                "assets",
                "workplace_backups",
                "machine_leases",
            ],
        })?,
        ("GET", "/api/v1/workplaces") => json_body(&workplace_list(state, now_unix()))?,
        ("GET", "/api/v1/assets") => json_body(
            &luxifer_core::list_assets(&state.data_dir.join("assets"))
                .map_err(|error| std::io::Error::other(error.to_string()))?,
        )?,
        ("POST", "/api/v1/assets") => {
            let transfer: AssetTransfer = match serde_json::from_str(body) {
                Ok(transfer) => transfer,
                Err(_) => return Ok(("400 Bad Request", r#"{"error":"invalid_json"}"#.into())),
            };
            let bytes = match hex_decode(&transfer.content_hex) {
                Some(bytes) => bytes,
                None => return Ok(("400 Bad Request", r#"{"error":"invalid_asset"}"#.into())),
            };
            match luxifer_core::store_asset(&state.data_dir.join("assets"), &transfer.meta, &bytes)
            {
                Ok(()) => json_body(
                    &luxifer_core::asset_meta(&state.data_dir.join("assets"), &transfer.meta.id)
                        .map_err(|error| std::io::Error::other(error.to_string()))?,
                )?,
                Err(_) => {
                    return Ok((
                        "422 Unprocessable Entity",
                        r#"{"error":"invalid_asset"}"#.into(),
                    ))
                }
            }
        }
        ("POST", "/api/v1/workplaces/heartbeat") => {
            let heartbeat: WorkplaceHeartbeat = match serde_json::from_str(body) {
                Ok(heartbeat) => heartbeat,
                Err(_) => return Ok(("400 Bad Request", r#"{"error":"invalid_json"}"#.into())),
            };
            if heartbeat.workplace_id.trim().is_empty()
                || heartbeat.workplace_name.trim().is_empty()
            {
                return Ok(("400 Bad Request", r#"{"error":"invalid_workplace"}"#.into()));
            }
            let now = now_unix();
            state.workplaces.insert(
                heartbeat.workplace_id,
                WorkplaceEntry {
                    name: heartbeat.workplace_name.trim().to_owned(),
                    last_seen_unix: now,
                },
            );
            json_body(&workplace_list(state, now))?
        }
        ("GET", "/api/v1/workplaces/backups") => {
            json_body(&list_workplace_backups(&state.data_dir)?)?
        }
        ("POST", "/api/v1/workplaces/backups") => {
            let backup: WorkplaceBackup = match serde_json::from_str(body) {
                Ok(backup) => backup,
                Err(_) => return Ok(("400 Bad Request", r#"{"error":"invalid_json"}"#.into())),
            };
            match store_workplace_backup(&state.data_dir, backup) {
                Ok(ack) => json_body(&ack)?,
                Err(StoreBackupError::Invalid) => {
                    return Ok(("400 Bad Request", r#"{"error":"invalid_backup"}"#.into()));
                }
                Err(StoreBackupError::HashMismatch) => {
                    return Ok((
                        "422 Unprocessable Entity",
                        r#"{"error":"hash_mismatch"}"#.into(),
                    ));
                }
                Err(StoreBackupError::Io(error)) => return Err(error),
            }
        }
        ("POST", "/api/v1/leases/acquire") => {
            let request: LeaseAcquire = match serde_json::from_str(body) {
                Ok(request) => request,
                Err(_) => return Ok(("400 Bad Request", r#"{"error":"invalid_json"}"#.into())),
            };
            match acquire_lease(state, request, now_unix()) {
                Some(reply) => json_body(&reply)?,
                None => return Ok(("400 Bad Request", r#"{"error":"invalid_lease"}"#.into())),
            }
        }
        ("POST", "/api/v1/leases/heartbeat") => {
            let request: LeaseHeartbeat = match serde_json::from_str(body) {
                Ok(request) => request,
                Err(_) => return Ok(("400 Bad Request", r#"{"error":"invalid_json"}"#.into())),
            };
            match heartbeat_lease(state, request, now_unix()) {
                Some(reply) => json_body(&reply)?,
                None => return Ok(("409 Conflict", r#"{"error":"lease_lost"}"#.into())),
            }
        }
        ("POST", "/api/v1/leases/release") => {
            let request: LeaseRelease = match serde_json::from_str(body) {
                Ok(request) => request,
                Err(_) => return Ok(("400 Bad Request", r#"{"error":"invalid_json"}"#.into())),
            };
            let released = release_lease(state, &request);
            json_body(&serde_json::json!({ "released": released }))?
        }
        ("POST", "/api/v1/projects/revisions") => {
            let upload: RevisionUpload = match serde_json::from_str(body) {
                Ok(upload) => upload,
                Err(_) => return Ok(("400 Bad Request", r#"{"error":"invalid_json"}"#.into())),
            };
            match store_revision(&state.data_dir, upload) {
                Ok(ack) => json_body(&ack)?,
                Err(StoreRevisionError::Invalid) => {
                    return Ok(("400 Bad Request", r#"{"error":"invalid_revision"}"#.into()));
                }
                Err(StoreRevisionError::HashMismatch) => {
                    return Ok((
                        "422 Unprocessable Entity",
                        r#"{"error":"hash_mismatch"}"#.into(),
                    ));
                }
                Err(StoreRevisionError::Conflict) => {
                    return Ok(("409 Conflict", r#"{"error":"revision_conflict"}"#.into()));
                }
                Err(StoreRevisionError::Io(error)) => return Err(error),
            }
        }
        ("GET", "/api/v1/projects/revisions") => {
            let workplace_id = query_value(query, "workplace_id").unwrap_or_default();
            if !valid_id(workplace_id) {
                return Ok(("400 Bad Request", r#"{"error":"invalid_workplace"}"#.into()));
            }
            json_body(&list_remote_revisions(&state.data_dir, workplace_id)?)?
        }
        ("POST", "/api/v1/projects/revisions/ack") => {
            let receipt: RevisionReceipt = match serde_json::from_str(body) {
                Ok(receipt) => receipt,
                Err(_) => return Ok(("400 Bad Request", r#"{"error":"invalid_json"}"#.into())),
            };
            match store_receipt(&state.data_dir, receipt) {
                Ok(ack) => json_body(&ack)?,
                Err(StoreReceiptError::Invalid) => {
                    return Ok(("400 Bad Request", r#"{"error":"invalid_receipt"}"#.into()));
                }
                Err(StoreReceiptError::UnknownRevision) => {
                    return Ok(("404 Not Found", r#"{"error":"unknown_revision"}"#.into()));
                }
                Err(StoreReceiptError::HashMismatch) => {
                    return Ok((
                        "409 Conflict",
                        r#"{"error":"receipt_hash_mismatch"}"#.into(),
                    ));
                }
                Err(StoreReceiptError::Io(error)) => return Err(error),
            }
        }
        ("GET", path) if path.starts_with("/api/v1/assets/") => {
            let id = path.trim_start_matches("/api/v1/assets/");
            if !valid_id(id) {
                return Ok(("400 Bad Request", r#"{"error":"invalid_asset"}"#.into()));
            }
            let store = state.data_dir.join("assets");
            let meta = match luxifer_core::asset_meta(&store, &id.to_string()) {
                Ok(meta) => meta,
                Err(_) => return Ok(("404 Not Found", r#"{"error":"not_found"}"#.into())),
            };
            let bytes = luxifer_core::load_asset(&store, &id.to_string())
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            json_body(&AssetTransfer {
                meta,
                content_hex: hex_encode(&bytes),
            })?
        }
        ("GET", _) => ("404 Not Found", r#"{"error":"not_found"}"#.into()),
        _ => (
            "405 Method Not Allowed",
            r#"{"error":"method_not_allowed"}"#.into(),
        ),
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

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then_some(value)
    })
}

fn list_remote_revisions(
    data_dir: &Path,
    workplace_id: &str,
) -> std::io::Result<Vec<RevisionDownload>> {
    let projects_dir = data_dir.join("projects");
    let projects = match std::fs::read_dir(projects_dir) {
        Ok(projects) => projects,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut revisions = Vec::new();
    for project in projects {
        let revision_dirs = match std::fs::read_dir(project?.path().join("revisions")) {
            Ok(dirs) => dirs,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        for revision_dir in revision_dirs {
            let revision_dir = revision_dir?;
            if !revision_dir.file_type()?.is_dir()
                || revision_dir.file_name().to_string_lossy().starts_with('.')
            {
                continue;
            }
            let manifest = std::fs::read(revision_dir.path().join("manifest.json"))?;
            let stored: StoredRevision = serde_json::from_slice(&manifest)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
            if stored.workplace_id == workplace_id
                || receipt_path(data_dir, workplace_id, &stored.revision_id).exists()
            {
                continue;
            }
            let payload = std::fs::read_to_string(revision_dir.path().join("payload.luxi"))?;
            revisions.push(RevisionDownload {
                revision_id: stored.revision_id,
                project_id: stored.project_id,
                project_name: stored.project_name,
                project_version_id: stored.project_version_id,
                parent_revision_id: stored.parent_revision_id,
                workplace_id: stored.workplace_id,
                queued_at: stored.queued_at,
                content_hash: stored.content_hash,
                payload,
            });
        }
    }
    revisions.sort_by(|a, b| a.revision_id.cmp(&b.revision_id));
    Ok(revisions)
}

#[derive(Debug)]
enum StoreReceiptError {
    Invalid,
    UnknownRevision,
    HashMismatch,
    Io(std::io::Error),
}

fn store_receipt(
    data_dir: &Path,
    mut receipt: RevisionReceipt,
) -> Result<ReceiptAck, StoreReceiptError> {
    if !valid_id(&receipt.workplace_id)
        || !valid_id(&receipt.revision_id)
        || !valid_id(&receipt.project_id)
    {
        return Err(StoreReceiptError::Invalid);
    }
    let manifest_path = data_dir
        .join("projects")
        .join(&receipt.project_id)
        .join("revisions")
        .join(&receipt.revision_id)
        .join("manifest.json");
    let bytes = match std::fs::read(manifest_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(StoreReceiptError::UnknownRevision);
        }
        Err(error) => return Err(StoreReceiptError::Io(error)),
    };
    let stored: StoredRevision = serde_json::from_slice(&bytes)
        .map_err(|error| StoreReceiptError::Io(std::io::Error::other(error)))?;
    if stored.content_hash != receipt.content_hash {
        return Err(StoreReceiptError::HashMismatch);
    }
    receipt.received_at_unix = now_unix();
    let path = receipt_path(data_dir, &receipt.workplace_id, &receipt.revision_id);
    let parent = path.parent().ok_or(StoreReceiptError::Invalid)?;
    std::fs::create_dir_all(parent).map_err(StoreReceiptError::Io)?;
    let temp = parent.join(format!(".{}.tmp", receipt.revision_id));
    let bytes = serde_json::to_vec_pretty(&receipt)
        .map_err(|error| StoreReceiptError::Io(std::io::Error::other(error)))?;
    std::fs::write(&temp, bytes).map_err(StoreReceiptError::Io)?;
    std::fs::rename(temp, path).map_err(StoreReceiptError::Io)?;
    Ok(ReceiptAck {
        revision_id: receipt.revision_id,
        content_hash: receipt.content_hash,
        accepted: true,
    })
}

fn receipt_path(data_dir: &Path, workplace_id: &str, revision_id: &str) -> PathBuf {
    data_dir
        .join("receipts")
        .join(workplace_id)
        .join(format!("{revision_id}.json"))
}

fn read_http_request(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut bytes = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 8192];
    let mut expected = None;
    loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "HTTP-Anfrage überschreitet das Größenlimit.",
            ));
        }
        if expected.is_none() {
            if let Some(header_end) = find_header_end(&bytes) {
                let headers = std::str::from_utf8(&bytes[..header_end])
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                expected = Some(header_end + 4 + content_length);
            }
        }
        if expected.is_some_and(|expected| bytes.len() >= expected) {
            break;
        }
    }
    String::from_utf8(bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

#[derive(Debug)]
enum StoreRevisionError {
    Invalid,
    HashMismatch,
    Conflict,
    Io(std::io::Error),
}

fn store_revision(
    data_dir: &Path,
    upload: RevisionUpload,
) -> Result<RevisionAck, StoreRevisionError> {
    if !valid_id(&upload.revision_id)
        || !valid_id(&upload.project_id)
        || upload.workplace_id.trim().is_empty()
        || upload.project_version_id.trim().is_empty()
    {
        return Err(StoreRevisionError::Invalid);
    }
    let actual_hash = luxifer_core::assets::content_hash(upload.payload.as_bytes());
    if actual_hash != upload.content_hash {
        return Err(StoreRevisionError::HashMismatch);
    }
    let revision_dir = data_dir
        .join("projects")
        .join(&upload.project_id)
        .join("revisions")
        .join(&upload.revision_id);
    let manifest_path = revision_dir.join("manifest.json");
    if manifest_path.exists() {
        let bytes = std::fs::read(&manifest_path).map_err(StoreRevisionError::Io)?;
        let stored: StoredRevision = serde_json::from_slice(&bytes)
            .map_err(|error| StoreRevisionError::Io(std::io::Error::other(error)))?;
        if stored.content_hash == upload.content_hash {
            return Ok(RevisionAck {
                revision_id: upload.revision_id,
                content_hash: upload.content_hash,
                stored: false,
            });
        }
        return Err(StoreRevisionError::Conflict);
    }

    let stored = StoredRevision {
        revision_id: upload.revision_id.clone(),
        project_id: upload.project_id.clone(),
        project_name: upload.project_name,
        project_version_id: upload.project_version_id,
        parent_revision_id: upload.parent_revision_id,
        workplace_id: upload.workplace_id,
        queued_at: upload.queued_at,
        content_hash: upload.content_hash.clone(),
    };
    let parent = revision_dir.parent().ok_or(StoreRevisionError::Invalid)?;
    std::fs::create_dir_all(parent).map_err(StoreRevisionError::Io)?;
    let temp_dir = parent.join(format!(".{}.tmp", upload.revision_id));
    std::fs::create_dir(&temp_dir).map_err(StoreRevisionError::Io)?;
    let result = (|| {
        std::fs::write(temp_dir.join("payload.luxi"), upload.payload)
            .map_err(StoreRevisionError::Io)?;
        let manifest = serde_json::to_vec_pretty(&stored)
            .map_err(|error| StoreRevisionError::Io(std::io::Error::other(error)))?;
        std::fs::write(temp_dir.join("manifest.json"), manifest).map_err(StoreRevisionError::Io)?;
        std::fs::rename(&temp_dir, &revision_dir).map_err(StoreRevisionError::Io)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    result?;
    Ok(RevisionAck {
        revision_id: upload.revision_id,
        content_hash: upload.content_hash,
        stored: true,
    })
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

const LEASE_TTL_SECS: u64 = 15;

fn acquire_lease(state: &mut ServerState, request: LeaseAcquire, now: u64) -> Option<LeaseReply> {
    if !valid_id(&request.controller_id)
        || !valid_id(&request.workplace_id)
        || request.controller_name.trim().is_empty()
        || request.workplace_name.trim().is_empty()
    {
        return None;
    }
    if let Some(existing) = state.leases.get_mut(&request.controller_id) {
        if existing.workplace_id == request.workplace_id {
            existing.expires_at_unix = now + LEASE_TTL_SECS;
            return Some(granted_reply(&request.controller_id, existing));
        }
        let expired = existing.expires_at_unix <= now;
        let safely_reclaimable = expired && existing.usage == LeaseUsage::Idle;
        if !(safely_reclaimable || expired && request.force) {
            if existing.usage == LeaseUsage::Idle {
                existing.release_requested_by = Some(request.workplace_name.clone());
            }
            return Some(LeaseReply {
                controller_id: request.controller_id,
                granted: false,
                token: None,
                holder_name: Some(existing.workplace_name.clone()),
                holder_usage: Some(existing.usage),
                expires_at_unix: Some(existing.expires_at_unix),
                release_requested: existing.usage == LeaseUsage::Idle,
                force_required: expired && existing.usage != LeaseUsage::Idle,
            });
        }
    }
    let token = format!("lease-{}-{now}", std::process::id());
    state.leases.insert(
        request.controller_id.clone(),
        LeaseEntry {
            token: token.clone(),
            workplace_id: request.workplace_id,
            workplace_name: request.workplace_name,
            usage: LeaseUsage::Idle,
            expires_at_unix: now + LEASE_TTL_SECS,
            release_requested_by: None,
        },
    );
    Some(granted_reply(
        &request.controller_id,
        state.leases.get(&request.controller_id).unwrap(),
    ))
}

fn granted_reply(controller_id: &str, lease: &LeaseEntry) -> LeaseReply {
    LeaseReply {
        controller_id: controller_id.into(),
        granted: true,
        token: Some(lease.token.clone()),
        holder_name: Some(lease.workplace_name.clone()),
        holder_usage: Some(lease.usage),
        expires_at_unix: Some(lease.expires_at_unix),
        release_requested: lease.release_requested_by.is_some(),
        force_required: false,
    }
}

fn heartbeat_lease(
    state: &mut ServerState,
    request: LeaseHeartbeat,
    now: u64,
) -> Option<LeaseReply> {
    let lease = state.leases.get_mut(&request.controller_id)?;
    if lease.workplace_id != request.workplace_id || lease.token != request.token {
        return None;
    }
    lease.usage = request.usage;
    lease.expires_at_unix = now + LEASE_TTL_SECS;
    Some(granted_reply(&request.controller_id, lease))
}

fn release_lease(state: &mut ServerState, request: &LeaseRelease) -> bool {
    let matches = state
        .leases
        .get(&request.controller_id)
        .is_some_and(|lease| {
            lease.workplace_id == request.workplace_id && lease.token == request.token
        });
    if matches {
        state.leases.remove(&request.controller_id);
    }
    matches
}

#[derive(Debug)]
enum StoreBackupError {
    Invalid,
    HashMismatch,
    Io(std::io::Error),
}

fn store_workplace_backup(
    data_dir: &Path,
    backup: WorkplaceBackup,
) -> Result<WorkplaceBackupAck, StoreBackupError> {
    if !valid_id(&backup.workplace_id)
        || backup.workplace_name.trim().is_empty()
        || !matches!(backup.kind.as_str(), "ui_settings" | "laser_profiles")
    {
        return Err(StoreBackupError::Invalid);
    }
    let actual_hash = luxifer_core::assets::content_hash(backup.payload.as_bytes());
    if actual_hash != backup.content_hash {
        return Err(StoreBackupError::HashMismatch);
    }
    let dir = data_dir.join("workplaces").join(&backup.workplace_id);
    let path = dir.join(format!("{}.json", backup.kind));
    if let Ok(bytes) = std::fs::read(&path) {
        let existing: WorkplaceBackup = serde_json::from_slice(&bytes)
            .map_err(|error| StoreBackupError::Io(std::io::Error::other(error)))?;
        if existing.content_hash == backup.content_hash {
            return Ok(WorkplaceBackupAck {
                workplace_id: backup.workplace_id,
                kind: backup.kind,
                content_hash: backup.content_hash,
                stored: false,
            });
        }
    }
    std::fs::create_dir_all(&dir).map_err(StoreBackupError::Io)?;
    let temp = dir.join(format!(".{}.tmp", backup.kind));
    let bytes = serde_json::to_vec_pretty(&backup)
        .map_err(|error| StoreBackupError::Io(std::io::Error::other(error)))?;
    std::fs::write(&temp, bytes).map_err(StoreBackupError::Io)?;
    std::fs::rename(&temp, &path).map_err(StoreBackupError::Io)?;
    Ok(WorkplaceBackupAck {
        workplace_id: backup.workplace_id,
        kind: backup.kind,
        content_hash: backup.content_hash,
        stored: true,
    })
}

fn list_workplace_backups(data_dir: &Path) -> std::io::Result<Vec<WorkplaceBackup>> {
    let root = data_dir.join("workplaces");
    let mut backups = Vec::new();
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(backups),
        Err(error) => return Err(error),
    };
    for workplace in entries.flatten() {
        if !workplace.path().is_dir() {
            continue;
        }
        for kind in ["ui_settings", "laser_profiles"] {
            let path = workplace.path().join(format!("{kind}.json"));
            if !path.exists() {
                continue;
            }
            let bytes = std::fs::read(path)?;
            let backup: WorkplaceBackup = serde_json::from_slice(&bytes)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
            backups.push(backup);
        }
    }
    backups.sort_by(|left, right| {
        left.workplace_name
            .cmp(&right.workplace_name)
            .then_with(|| left.kind.cmp(&right.kind))
    });
    Ok(backups)
}

fn charon_data_dir() -> PathBuf {
    std::env::var_os("CHARON_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("local-data/charon"))
}

fn workplace_list(state: &ServerState, now: u64) -> Vec<WorkplacePresence> {
    state
        .workplaces
        .iter()
        .map(|(id, entry)| WorkplacePresence {
            id: id.clone(),
            name: entry.name.clone(),
            last_seen_unix: entry.last_seen_unix,
            online: now.saturating_sub(entry.last_seen_unix) <= ONLINE_TIMEOUT_SECS,
        })
        .collect()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn json_body(value: &impl Serialize) -> std::io::Result<(&'static str, String)> {
    serde_json::to_string(value)
        .map(|body| ("200 OK", body))
        .map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standardbindung_ist_loopback() {
        let bind: SocketAddr = DEFAULT_BIND.parse().unwrap();
        assert!(bind.ip().is_loopback());
        assert_eq!(bind.port(), 3737);
    }

    #[test]
    fn netzwerkbindung_braucht_explizite_freigabe() {
        let error = ServerConfig::from_values("0.0.0.0:3737", false).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains(NETWORK_OPT_IN_ENV));

        let config = ServerConfig::from_values("0.0.0.0:3737", true).unwrap();
        assert!(config.bind.ip().is_unspecified());
        assert_eq!(config.bind.port(), 3737);
    }

    #[test]
    fn loopback_braucht_keine_netzwerkfreigabe() {
        let config = ServerConfig::from_values(DEFAULT_BIND, false).unwrap();
        assert!(config.bind.ip().is_loopback());
    }

    #[test]
    fn handshake_hat_stabile_protokollgrenze() {
        let body = json_body(&Handshake {
            server: "charon",
            server_version: "test",
            protocol_version: PROTOCOL_VERSION,
            instance_id: "local-test".into(),
            capabilities: [
                "health",
                "handshake",
                "workplaces",
                "project_revisions",
                "project_events",
                "assets",
                "workplace_backups",
                "machine_leases",
            ],
        })
        .unwrap()
        .1;
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["protocol_version"], 1);
        assert_eq!(value["server"], "charon");
    }

    #[test]
    fn zwei_arbeitsplaetze_werden_unabhaengig_registriert() {
        let mut state = ServerState::new(PathBuf::new());
        route(
            "POST",
            "/api/v1/workplaces/heartbeat",
            r#"{"workplace_id":"office-id","workplace_name":"Office"}"#,
            &mut state,
        )
        .unwrap();
        let (_, body) = route(
            "POST",
            "/api/v1/workplaces/heartbeat",
            r#"{"workplace_id":"workshop-id","workplace_name":"Workshop"}"#,
            &mut state,
        )
        .unwrap();
        let workplaces: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap();
        assert_eq!(workplaces.len(), 2);
        assert!(workplaces.iter().all(|entry| entry["online"] == true));
    }

    #[test]
    fn revision_wird_geprueft_atomar_und_idempotent_gespeichert() {
        let dir = std::env::temp_dir().join(format!(
            "charon_revision_test_{}_{}",
            std::process::id(),
            now_unix()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let payload = r#"{"version":1}"#;
        let upload = RevisionUpload {
            revision_id: "revision-1".into(),
            project_id: "project-1".into(),
            project_name: "Test".into(),
            project_version_id: "version-1".into(),
            parent_revision_id: None,
            workplace_id: "office-1".into(),
            queued_at: "2026-07-13T12:00:00Z".into(),
            content_hash: luxifer_core::assets::content_hash(payload.as_bytes()),
            payload: payload.into(),
        };

        let first = store_revision(&dir, upload.clone()).unwrap();
        let second = store_revision(&dir, upload).unwrap();
        assert!(first.stored);
        assert!(!second.stored);
        let stored_payload = dir.join("projects/project-1/revisions/revision-1/payload.luxi");
        assert_eq!(std::fs::read_to_string(stored_payload).unwrap(), payload);
        let remote = list_remote_revisions(&dir, "workshop-1").unwrap();
        assert_eq!(remote.len(), 1);
        assert_eq!(remote[0].payload, payload);
        assert!(list_remote_revisions(&dir, "office-1").unwrap().is_empty());
        let receipt = RevisionReceipt {
            workplace_id: "workshop-1".into(),
            revision_id: "revision-1".into(),
            project_id: "project-1".into(),
            content_hash: luxifer_core::assets::content_hash(payload.as_bytes()),
            received_at_unix: 0,
        };
        let ack = store_receipt(&dir, receipt).unwrap();
        assert!(ack.accepted);
        assert!(list_remote_revisions(&dir, "workshop-1")
            .unwrap()
            .is_empty());
        assert!(receipt_path(&dir, "workshop-1", "revision-1").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn arbeitsplatz_sicherungen_bleiben_getrennt_und_idempotent() {
        let dir = std::env::temp_dir().join(format!(
            "charon_backup_test_{}_{}",
            std::process::id(),
            now_unix()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let payload = r#"{"version":1,"workplace":"Office"}"#;
        let backup = WorkplaceBackup {
            workplace_id: "office-1".into(),
            workplace_name: "Office".into(),
            kind: "ui_settings".into(),
            saved_at_unix: 42,
            content_hash: luxifer_core::assets::content_hash(payload.as_bytes()),
            payload: payload.into(),
        };

        assert!(store_workplace_backup(&dir, backup.clone()).unwrap().stored);
        assert!(!store_workplace_backup(&dir, backup).unwrap().stored);
        let backups = list_workplace_backups(&dir).unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].kind, "ui_settings");
        assert_eq!(backups[0].saved_at_unix, 42);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lease_uebergabe_schuetzt_jobs_und_gibt_idle_frei() {
        let mut state = ServerState::new(PathBuf::new());
        let acquire = |workplace: &str, name: &str, force| LeaseAcquire {
            controller_id: "controller-1".into(),
            controller_name: "Ruida".into(),
            workplace_id: workplace.into(),
            workplace_name: name.into(),
            force,
        };
        let office = acquire_lease(&mut state, acquire("office", "Office", false), 10).unwrap();
        assert!(office.granted);
        let denied =
            acquire_lease(&mut state, acquire("workshop", "Werkstatt", false), 11).unwrap();
        assert!(!denied.granted);
        assert!(denied.release_requested);

        let token = office.token.unwrap();
        let handover = heartbeat_lease(
            &mut state,
            LeaseHeartbeat {
                controller_id: "controller-1".into(),
                workplace_id: "office".into(),
                token,
                usage: LeaseUsage::Running,
            },
            12,
        )
        .unwrap();
        assert!(handover.release_requested);
        let busy = acquire_lease(&mut state, acquire("workshop", "Werkstatt", false), 30).unwrap();
        assert!(!busy.granted);
        assert!(busy.force_required);
        let forced = acquire_lease(&mut state, acquire("workshop", "Werkstatt", true), 30).unwrap();
        assert!(forced.granted);
    }
}
