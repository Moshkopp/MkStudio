//! Serieller GRBL-Transport. Hält den Port dauerhaft offen und reicht nur
//! bereits geparste Protokollzeilen an den Treiber weiter.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use studio_core::{DriverConsoleDirection, DriverConsoleLine, DriverError};

use crate::protocol::{parse_line, GrblLine, GrblStatus};

const IO_TIMEOUT: Duration = Duration::from_millis(100);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

pub struct SerialTransport {
    port: Mutex<Box<dyn serialport::SerialPort>>,
    port_name: String,
    baud: u32,
    console: Mutex<VecDeque<DriverConsoleLine>>,
    last_logged_status: Mutex<Option<String>>,
}

impl SerialTransport {
    pub fn connect(port_name: &str, baud: u32) -> Result<Self, DriverError> {
        let port = serialport::new(port_name, baud)
            .timeout(IO_TIMEOUT)
            .open()
            .map_err(|error| transport_error("Seriellen Port öffnen", error))?;
        let transport = Self {
            port: Mutex::new(port),
            port_name: port_name.to_owned(),
            baud,
            console: Mutex::new(VecDeque::new()),
            last_logged_status: Mutex::new(None),
        };
        transport.handshake()?;
        Ok(transport)
    }

    pub fn console_snapshot(&self) -> Vec<DriverConsoleLine> {
        self.console
            .lock()
            .map(|lines| lines.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn log(&self, direction: DriverConsoleDirection, text: impl Into<String>) {
        if let Ok(mut lines) = self.console.lock() {
            if lines.len() == 500 {
                lines.pop_front();
            }
            lines.push_back(DriverConsoleLine {
                direction,
                text: text.into(),
            });
        }
    }

    fn log_status_if_changed(&self, line: &str) {
        let Ok(mut previous) = self.last_logged_status.lock() else {
            return;
        };
        if previous.as_deref() != Some(line) {
            self.log(DriverConsoleDirection::Received, line);
            *previous = Some(line.to_owned());
        }
    }

    pub fn matches(&self, port_name: &str, baud: u32) -> bool {
        self.port_name == port_name && self.baud == baud
    }

    fn handshake(&self) -> Result<(), DriverError> {
        let mut port = self.lock_port()?;
        // Ein bereits laufender Controller antwortet auf die Leerzeile; ein
        // durch DTR neu gestarteter ESP32 erhält bis zu fünf Sekunden Bootzeit.
        port.write_all(b"\r")
            .map_err(|error| transport_error("Handshake senden", error))?;
        self.log(DriverConsoleDirection::Sent, "<Handshake>");
        port.flush()
            .map_err(|error| transport_error("Handshake senden", error))?;

        let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
        let mut saw_welcome = false;
        while Instant::now() < deadline {
            if let Some(line) = read_line(&mut **port, deadline)? {
                self.log(DriverConsoleDirection::Received, &line);
                if matches!(parse_line(&line), Some(GrblLine::Welcome(_))) {
                    saw_welcome = true;
                    break;
                }
            }
        }

        // `$I` identifiziert auch einen Controller, der beim Öffnen keinen
        // Reset ausführt und daher keine neue Begrüßung sendet.
        port.write_all(b"$I\r")
            .map_err(|error| transport_error("Identitätsabfrage senden", error))?;
        self.log(DriverConsoleDirection::Sent, "$I");
        port.flush()
            .map_err(|error| transport_error("Identitätsabfrage senden", error))?;
        let deadline = Instant::now() + COMMAND_TIMEOUT;
        let mut saw_identity = false;
        while Instant::now() < deadline {
            let Some(line) = read_line(&mut **port, deadline)? else {
                continue;
            };
            self.log(DriverConsoleDirection::Received, &line);
            match parse_line(&line) {
                Some(GrblLine::Info(info)) if info.starts_with("[VER:") => {
                    saw_identity = true;
                }
                Some(GrblLine::Ack) => break,
                // Ein frisch gestarteter Controller kann Diagnosebefehle im
                // Alarmzustand zunächst mit error:9/ALARM ablehnen. Eine zuvor
                // erkannte echte GRBL-Begrüßung bleibt dennoch ein gültiger
                // Handshake; der Zustand wird separat über `?` sichtbar.
                Some(GrblLine::Error(_)) | Some(GrblLine::Alarm(_)) if saw_welcome => break,
                Some(GrblLine::Error(error)) => return Err(protocol_error("$I", &error)),
                Some(GrblLine::Alarm(alarm)) => return Err(protocol_error("ALARM", &alarm)),
                _ => {}
            }
        }
        if saw_welcome || saw_identity {
            Ok(())
        } else {
            Err(DriverError::Transport(
                "Der serielle Port antwortet nicht als GRBL-Controller.".into(),
            ))
        }
    }

    pub fn status(&self) -> Result<GrblStatus, DriverError> {
        let mut port = self.lock_port()?;
        port.write_all(b"?")
            .map_err(|error| transport_error("Statusabfrage senden", error))?;
        port.flush()
            .map_err(|error| transport_error("Statusabfrage senden", error))?;
        let deadline = Instant::now() + COMMAND_TIMEOUT;
        while Instant::now() < deadline {
            let Some(line) = read_line(&mut **port, deadline)? else {
                continue;
            };
            match parse_line(&line) {
                Some(GrblLine::Status(status)) => {
                    self.log_status_if_changed(&line);
                    return Ok(status);
                }
                Some(GrblLine::Alarm(alarm)) => {
                    self.log(DriverConsoleDirection::Received, &line);
                    return Err(protocol_error("ALARM", &alarm));
                }
                _ => self.log(DriverConsoleDirection::Received, &line),
            }
        }
        Err(DriverError::Transport(
            "Zeitüberschreitung bei der GRBL-Statusabfrage.".into(),
        ))
    }

    /// Streamt ein bereits kompiliertes G-Code-Programm konservativ Zeile für
    /// Zeile. Die nächste Zeile folgt erst auf `ok`; dadurch bleibt die erste
    /// Implementierung unabhängig von controllerabhängigen Puffergrößen.
    pub fn send_program(&self, bytes: &[u8]) -> Result<usize, DriverError> {
        let program = std::str::from_utf8(bytes).map_err(|error| {
            DriverError::Transport(format!("G-Code ist kein gültiges UTF-8: {error}"))
        })?;
        let mut port = self.lock_port()?;
        let mut sent = 0;
        for line in program_lines(program) {
            self.log(DriverConsoleDirection::Sent, line);
            if let Err(error) = send_line(&mut **port, line) {
                // Soft-Reset ist der sichere Abbruchpfad von GRBL und beendet
                // auch einen möglicherweise noch aktiven Laserzustand.
                let _ = port.write_all(&[0x18]);
                let _ = port.flush();
                return Err(error);
            }
            self.log(DriverConsoleDirection::Received, "ok");
            sent += 1;
        }
        Ok(sent)
    }

    fn lock_port(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Box<dyn serialport::SerialPort>>, DriverError> {
        self.port
            .lock()
            .map_err(|_| DriverError::Transport("Serieller Port ist nicht verfügbar.".into()))
    }
}

fn program_lines(program: &str) -> impl Iterator<Item = &str> {
    program
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with(';') && !line.starts_with('('))
}

fn send_line(port: &mut dyn serialport::SerialPort, line: &str) -> Result<(), DriverError> {
    port.write_all(line.as_bytes())
        .and_then(|()| port.write_all(b"\r"))
        .and_then(|()| port.flush())
        .map_err(|error| transport_error("G-Code senden", error))?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    while Instant::now() < deadline {
        let Some(response) = read_line(port, deadline)? else {
            continue;
        };
        match parse_line(&response) {
            Some(GrblLine::Ack) => return Ok(()),
            Some(GrblLine::Error(error)) => return Err(protocol_error("error", &error)),
            Some(GrblLine::Alarm(alarm)) => return Err(protocol_error("ALARM", &alarm)),
            _ => {}
        }
    }
    Err(DriverError::Transport(format!(
        "Keine GRBL-Quittung für „{line}“."
    )))
}

fn read_line(
    port: &mut dyn serialport::SerialPort,
    deadline: Instant,
) -> Result<Option<String>, DriverError> {
    let mut bytes = Vec::new();
    let mut byte = [0_u8; 1];
    while Instant::now() < deadline {
        match port.read(&mut byte) {
            Ok(0) => continue,
            Ok(_) if matches!(byte[0], b'\r' | b'\n') => {
                if !bytes.is_empty() {
                    return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
                }
            }
            Ok(_) => bytes.push(byte[0]),
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(error) => return Err(transport_error("Serielle Antwort lesen", error)),
        }
    }
    Ok((!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).into_owned()))
}

fn transport_error(context: &str, error: impl std::fmt::Display) -> DriverError {
    DriverError::Transport(format!("{context} fehlgeschlagen: {error}"))
}

fn protocol_error(context: &str, detail: &str) -> DriverError {
    DriverError::Transport(format!("GRBL {context}: {detail}"))
}

#[cfg(test)]
mod tests {
    use super::program_lines;

    #[test]
    fn streaming_filtert_kommentare_und_leerzeilen() {
        let lines: Vec<_> = program_lines("; Kopf\nG21\n\n(Info)\n M5 \n").collect();
        assert_eq!(lines, ["G21", "M5"]);
    }
}
