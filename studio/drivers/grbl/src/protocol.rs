//! Reiner GRBL-Zeilenparser. Kein I/O, keine GUI und keine Application-Typen.

#[derive(Debug, Clone, PartialEq)]
pub struct GrblStatus {
    pub state: String,
    pub machine_position: Option<[f64; 3]>,
    pub work_position: Option<[f64; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GrblLine {
    Welcome(String),
    Ack,
    Error(String),
    Alarm(String),
    Status(GrblStatus),
    Info(String),
    Other(String),
}

pub fn parse_line(raw: &str) -> Option<GrblLine> {
    let line = raw.trim_matches(['\r', '\n', ' ']);
    if line.is_empty() {
        return None;
    }
    if line.starts_with("Grbl ") {
        return Some(GrblLine::Welcome(line.to_owned()));
    }
    if line == "ok" {
        return Some(GrblLine::Ack);
    }
    if let Some(error) = line.strip_prefix("error:") {
        return Some(GrblLine::Error(error.trim().to_owned()));
    }
    if let Some(alarm) = line.strip_prefix("ALARM:") {
        return Some(GrblLine::Alarm(alarm.trim().to_owned()));
    }
    if line.starts_with('<') && line.ends_with('>') {
        return parse_status(line).map(GrblLine::Status);
    }
    if line.starts_with('[') && line.ends_with(']') {
        return Some(GrblLine::Info(line.to_owned()));
    }
    Some(GrblLine::Other(line.to_owned()))
}

fn parse_status(line: &str) -> Option<GrblStatus> {
    let body = line.strip_prefix('<')?.strip_suffix('>')?;
    let mut fields = body.split('|');
    let state = fields.next()?.to_owned();
    let mut status = GrblStatus {
        state,
        machine_position: None,
        work_position: None,
    };
    for field in fields {
        if let Some(value) = field.strip_prefix("MPos:") {
            status.machine_position = parse_xyz(value);
        } else if let Some(value) = field.strip_prefix("WPos:") {
            status.work_position = parse_xyz(value);
        }
    }
    Some(status)
}

fn parse_xyz(value: &str) -> Option<[f64; 3]> {
    let mut values = value.split(',').map(str::parse::<f64>);
    let xyz = [
        values.next()?.ok()?,
        values.next()?.ok()?,
        values.next()?.ok()?,
    ];
    xyz.iter().all(|value| value.is_finite()).then_some(xyz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erkennt_handshake_quittung_und_fehler() {
        assert!(matches!(
            parse_line("Grbl 1.1f ['$' for help]"),
            Some(GrblLine::Welcome(_))
        ));
        assert_eq!(parse_line("ok\r\n"), Some(GrblLine::Ack));
        assert_eq!(parse_line("error:20"), Some(GrblLine::Error("20".into())));
        assert_eq!(parse_line("ALARM:1"), Some(GrblLine::Alarm("1".into())));
    }

    #[test]
    fn status_parser_behaelt_maschinen_und_arbeitsposition() {
        let Some(GrblLine::Status(status)) =
            parse_line("<Idle|MPos:1.250,-2.000,0.000|WPos:0.250,3.000,0.000|FS:0,0>")
        else {
            panic!("Status erwartet");
        };
        assert_eq!(status.state, "Idle");
        assert_eq!(status.machine_position, Some([1.25, -2.0, 0.0]));
        assert_eq!(status.work_position, Some([0.25, 3.0, 0.0]));
    }
}
