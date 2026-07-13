//! UI-unabhängiger Charon-Verbindungstest (ADR 0012).

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::AppError;

const PROTOCOL_VERSION: u32 = 1;
const TIMEOUT: Duration = Duration::from_millis(800);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(10);

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
        if crate::sync_inbox::store_remote_revision(revision)? {
            report.received += 1;
        }
    }
    Ok(report)
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
}
