//! Charon — optionaler lokaler Koordinationsdienst für LuxiFer.
//! Der erste Schnitt stellt nur Health und Handshake bereit (ADR 0012).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const DEFAULT_BIND: &str = "127.0.0.1:3737";
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
        let bind: SocketAddr = raw.parse().map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Ungültiges CHARON_BIND '{raw}': {error}"),
            )
        })?;
        if !bind.ip().is_loopback() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Charon darf in diesem Meilenstein nur an Loopback binden.",
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
    capabilities: [&'static str; 4],
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

struct ServerState {
    workplaces: BTreeMap<String, WorkplaceEntry>,
    data_dir: PathBuf,
}

impl ServerState {
    fn new(data_dir: PathBuf) -> Self {
        Self {
            workplaces: BTreeMap::new(),
            data_dir,
        }
    }
}

pub fn serve(config: ServerConfig) -> std::io::Result<()> {
    let listener = TcpListener::bind(config.bind)?;
    let mut state = ServerState::new(charon_data_dir());
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(error) = serve_connection(&mut stream, &mut state) {
                    eprintln!("Charon-Verbindungsfehler: {error}");
                }
            }
            Err(error) => eprintln!("Charon-Accept-Fehler: {error}"),
        }
    }
    Ok(())
}

fn serve_connection(stream: &mut TcpStream, state: &mut ServerState) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
    let request = read_http_request(stream)?;
    let first_line = request.lines().next().unwrap_or_default().to_string();
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    let body = request.split_once("\r\n\r\n").map_or("", |(_, body)| body);

    let (status, body) = route(method, path, body, state)?;
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
            capabilities: ["health", "handshake", "workplaces", "project_revisions"],
        })?,
        ("GET", "/api/v1/workplaces") => json_body(&workplace_list(state, now_unix()))?,
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
        ("GET", _) => ("404 Not Found", r#"{"error":"not_found"}"#.into()),
        _ => (
            "405 Method Not Allowed",
            r#"{"error":"method_not_allowed"}"#.into(),
        ),
    })
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
            if stored.workplace_id == workplace_id {
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
    fn handshake_hat_stabile_protokollgrenze() {
        let body = json_body(&Handshake {
            server: "charon",
            server_version: "test",
            protocol_version: PROTOCOL_VERSION,
            instance_id: "local-test".into(),
            capabilities: ["health", "handshake", "workplaces", "project_revisions"],
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
        let _ = std::fs::remove_dir_all(dir);
    }
}
