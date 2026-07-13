//! Charon — optionaler lokaler Koordinationsdienst für LuxiFer.
//! Der erste Schnitt stellt nur Health und Handshake bereit (ADR 0012).

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

use serde::Serialize;

pub const DEFAULT_BIND: &str = "127.0.0.1:3737";
pub const PROTOCOL_VERSION: u32 = 1;

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
    capabilities: [&'static str; 2],
}

pub fn serve(config: ServerConfig) -> std::io::Result<()> {
    let listener = TcpListener::bind(config.bind)?;
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(error) = serve_connection(&mut stream) {
                    eprintln!("Charon-Verbindungsfehler: {error}");
                }
            }
            Err(error) => eprintln!("Charon-Accept-Fehler: {error}"),
        }
    }
    Ok(())
}

fn serve_connection(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
    let mut request = [0_u8; 4096];
    let count = stream.read(&mut request)?;
    let first_line = String::from_utf8_lossy(&request[..count])
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();

    let (status, body) = match (method, path) {
        ("GET", "/health") => json_body(&Health { status: "ok" })?,
        ("GET", "/api/v1/handshake") => json_body(&Handshake {
            server: "charon",
            server_version: env!("CARGO_PKG_VERSION"),
            protocol_version: PROTOCOL_VERSION,
            instance_id: format!("local-{}", std::process::id()),
            capabilities: ["health", "handshake"],
        })?,
        ("GET", _) => ("404 Not Found", r#"{"error":"not_found"}"#.into()),
        _ => (
            "405 Method Not Allowed",
            r#"{"error":"method_not_allowed"}"#.into(),
        ),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())
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
            capabilities: ["health", "handshake"],
        })
        .unwrap()
        .1;
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["protocol_version"], 1);
        assert_eq!(value["server"], "charon");
    }
}
