//! Laser-Profile & gerätespezifische Aktionen (ADR 0007): Registry-CRUD,
//! Job-Aktionen des aktiven Treibers, Jog/Home/Position/Ping und Export.

use luxifer_core::{
    Anchor, Connection, JobAction, JobParams, LaserProfile, LaserRegistry, MachineDriver, StartMode,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::shared::{plan_with_assets, AppData};

/// Job-Parameter aus dem Frontend („Starten von" + 3×3-Anker-Index).
#[derive(Deserialize, Default)]
pub struct JobParamsDto {
    /// "absolut" | "aktuell" | "ursprung".
    start_mode: String,
    /// 3×3-Index 0..8 (4 = Mitte).
    anchor: usize,
    /// Nur die aktuell ausgewählten Shapes in Job, Export und Rahmen übernehmen.
    selection_only: bool,
}

fn effective_shapes(
    data: &State<AppData>,
    selection_only: bool,
) -> (Vec<luxifer_core::Shape>, Vec<luxifer_core::Layer>) {
    let s = data.state();
    let shapes = if selection_only {
        s.selected
            .iter()
            .filter_map(|&i| s.shapes.get(i).cloned())
            .collect()
    } else {
        s.shapes.clone()
    };
    (shapes, s.layers.clone())
}

/// Position des sichtbaren Job-Startmarkers. Im absoluten Modus gibt es keinen
/// verschobenen Job-Nullpunkt, daher wird kein Marker geliefert.
#[tauri::command]
pub fn laser_job_start(data: State<AppData>, params: JobParamsDto) -> Option<[f64; 2]> {
    if matches!(params.to_params().start_mode, StartMode::Absolut) {
        return None;
    }
    let (shapes, layers) = effective_shapes(&data, params.selection_only);
    let bbox = plan_with_assets(&shapes, &layers).bbox?;
    let (x, y) = params.to_params().anchor.point(bbox);
    Some([x, y])
}

impl JobParamsDto {
    fn to_params(&self) -> JobParams {
        let start_mode = match self.start_mode.as_str() {
            "aktuell" => StartMode::AktuellePosition,
            "ursprung" => StartMode::Benutzerursprung,
            _ => StartMode::Absolut,
        };
        JobParams {
            start_mode,
            anchor: Anchor::from_index(self.anchor),
        }
    }
}

/// Kompilierter Job für den Datei-Download (Frontend bietet ihn als Datei an).
#[derive(Serialize)]
pub struct ExportDto {
    bytes: Vec<u8>,
    filename: String,
}

/// Kopf- und Ursprungsposition (mm) fürs Canvas. `origin` fehlt, wenn der
/// Controller keinen Benutzerursprung meldet.
#[derive(Serialize)]
pub struct PositionDto {
    head: [f64; 2],
    origin: Option<[f64; 2]>,
}

/// Gibt die gesamte Laser-Registry ans Frontend (Dropdown + Settings-Liste).
#[tauri::command]
pub fn laser_list(data: State<AppData>) -> LaserRegistry {
    data.lasers().clone()
}

/// Legt ein Profil an oder ersetzt ein bestehendes (gleiche ID). Ohne ID wird
/// eine neue vergeben. Speichert und gibt die aktualisierte Registry zurück.
#[tauri::command]
pub fn laser_save(
    data: State<AppData>,
    mut profile: LaserProfile,
) -> Result<LaserRegistry, String> {
    let mut lasers = data.lasers();
    if profile.id.is_empty() {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        profile.id = format!("laser-{millis}");
        lasers.add(profile);
    } else if !lasers.update(profile.clone()) {
        lasers.add(profile);
    }
    lasers.save()?;
    Ok(lasers.clone())
}

/// Löscht ein Profil, speichert und gibt die Registry zurück.
#[tauri::command]
pub fn laser_delete(data: State<AppData>, id: String) -> Result<LaserRegistry, String> {
    let mut lasers = data.lasers();
    lasers.remove(&id);
    lasers.save()?;
    Ok(lasers.clone())
}

/// Setzt den aktiven Laser (Panel-Dropdown), speichert und gibt die Registry
/// zurück. Der Treiber wird beim nächsten Aktions-Aufruf passend neu gebaut.
#[tauri::command]
pub fn laser_set_active(data: State<AppData>, id: String) -> Result<LaserRegistry, String> {
    let mut lasers = data.lasers();
    if !lasers.set_active(&id) {
        return Err("Unbekannter Laser.".into());
    }
    lasers.save()?;
    Ok(lasers.clone())
}

/// Die Job-Aktionen, die der aktive Treiber anbietet (als String-Schlüssel).
/// Leer, wenn kein Laser aktiv ist.
#[tauri::command]
pub fn laser_actions(data: State<AppData>) -> Vec<String> {
    data.with_active_driver(|d| Ok(d.actions().iter().map(|a| a.as_key().to_string()).collect()))
        .unwrap_or_default()
}

/// Führt eine gemeldete Job-Aktion aus (Senden/Rahmen/Home/Stop/Export). Der
/// Treiber entscheidet, was intern passiert (ADR 0007). Gibt eine Meldung zurück.
#[tauri::command]
pub fn laser_run_action(
    data: State<AppData>,
    action: String,
    params: JobParamsDto,
) -> Result<String, String> {
    let job_action = action_from_key(&action)?;
    let (shapes, layers) = effective_shapes(&data, params.selection_only);
    let plan = plan_with_assets(&shapes, &layers);
    let needs_geometry = matches!(
        job_action,
        JobAction::SendJob
            | JobAction::StreamGcode
            | JobAction::ExportFile
            | JobAction::Frame
            | JobAction::RubberFrame
    );
    if needs_geometry && params.selection_only && plan.layers.is_empty() {
        return Err("Keine laserbare Auswahl vorhanden.".into());
    }
    let jp = params.to_params();
    // Aktion, die eine Verbindung braucht, verbindet vorher automatisch.
    data.with_active_driver(|d| {
        if needs_connection(job_action) {
            connect_active(d, &data)?;
        }
        d.run_action(job_action, &plan, &layers, &jp)
            .map_err(|e| e.to_string())
    })
}

/// Kompiliert den aktuellen Job für den aktiven Treiber und gibt die Bytes samt
/// Vorschlags-Dateinamen zurück (Frontend bietet sie als Datei-Download an). Für
/// Ruida sind das .rd-Bytes, für GRBL G-Code. Braucht KEINE Verbindung.
#[tauri::command]
pub fn laser_export(data: State<AppData>, params: JobParamsDto) -> Result<ExportDto, String> {
    let (shapes, layers) = effective_shapes(&data, params.selection_only);
    let plan = plan_with_assets(&shapes, &layers);
    if params.selection_only && plan.layers.is_empty() {
        return Err("Keine laserbare Auswahl vorhanden.".into());
    }
    let jp = params.to_params();
    data.with_active_driver(|d| {
        let ext = match d.name() {
            "Ruida" => "rd",
            _ => "gcode",
        };
        let bytes = d.compile_with(&plan, &layers, &jp)?;
        Ok(ExportDto {
            bytes,
            filename: format!("job.{ext}"),
        })
    })
}

/// Kopf per Jog um (dx, dy) mm mit `speed` mm/s fahren. Verbindet automatisch.
#[tauri::command]
pub fn laser_jog(data: State<AppData>, dx: f64, dy: f64, speed: f64) -> Result<(), String> {
    data.with_active_driver(|d| {
        connect_active(d, &data)?;
        d.jog(dx, dy, speed).map_err(|e| e.to_string())
    })
}

/// Referenzfahrt (Home). Verbindet automatisch.
#[tauri::command]
pub fn laser_home(data: State<AppData>, speed: f64) -> Result<(), String> {
    data.with_active_driver(|d| {
        connect_active(d, &data)?;
        d.home(speed).map_err(|e| e.to_string())
    })
}

/// Liest Kopf- und Ursprungsposition (mm) für die Canvas-Anzeige (auf Knopfdruck).
/// Verbindet automatisch.
#[tauri::command]
pub fn laser_position(data: State<AppData>) -> Result<PositionDto, String> {
    data.with_active_driver(|d| {
        connect_active(d, &data)?;
        let st = d.status().map_err(|e| e.to_string())?;
        // Ursprung ist optional — nicht jeder Controller/Zustand liefert ihn.
        let origin = d.read_origin().ok().map(|(x, y)| [x, y]);
        Ok(PositionDto {
            head: [st.pos_x_mm, st.pos_y_mm],
            origin,
        })
    })
}

/// Prüft, ob der aktive Laser erreichbar ist (nur Netz/Ruida-Ping).
#[tauri::command]
pub fn laser_ping(data: State<AppData>) -> bool {
    let lasers = data.lasers();
    match lasers.active().map(|p| p.connection.clone()) {
        Some(Connection::Netz { ip, .. }) => luxifer_driver_ruida::RuidaTransport::ping(&ip),
        _ => false,
    }
}

fn action_from_key(key: &str) -> Result<JobAction, String> {
    Ok(match key {
        "send_job" => JobAction::SendJob,
        "stream_gcode" => JobAction::StreamGcode,
        "export_file" => JobAction::ExportFile,
        "frame" => JobAction::Frame,
        "rubber_frame" => JobAction::RubberFrame,
        "pause" => JobAction::Pause,
        "home" => JobAction::Home,
        "go_origin" => JobAction::GoOrigin,
        "stop" => JobAction::Stop,
        other => return Err(format!("Unbekannte Aktion: {other}")),
    })
}

fn needs_connection(a: JobAction) -> bool {
    matches!(
        a,
        JobAction::SendJob
            | JobAction::StreamGcode
            | JobAction::Frame
            | JobAction::RubberFrame
            | JobAction::Pause
            | JobAction::Home
            | JobAction::GoOrigin
            | JobAction::Stop
    )
}

/// Verbindet den aktiven Treiber mit der Adresse seines Profils.
fn connect_active(
    driver: &mut Box<dyn MachineDriver + Send>,
    data: &State<AppData>,
) -> Result<(), String> {
    let lasers = data.lasers();
    let profile = lasers.active().ok_or("Kein Laser aktiv.")?;
    let target = match &profile.connection {
        Connection::Netz { ip, .. } => ip.clone(),
        Connection::Seriell { port, .. } => port.clone(),
    };
    drop(lasers);
    driver.connect(&target).map_err(|e| e.to_string())
}
