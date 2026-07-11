//! Geteilte Infrastruktur aller Tauri-Command-Module: der App-Zustand
//! (`AppData`), die Frontend-Sicht (`Scene`, Preview-DTOs) und die Helfer, die
//! mehrere Command-Gruppen brauchen (`scene`, `plan_with_assets`, Base64 …).
//! Die Commands selbst liegen in `commands::*` und ziehen von hier.

use std::sync::Mutex;

use luxifer_core::preview::{JobPreview, MoveKind};
use luxifer_core::{
    assets_dir, AppState, DriverKind, JobPlan, LaserProfile, LaserRegistry, Layer, MachineDriver,
    ProjectFile, Shape,
};
use serde::Serialize;
use tauri::State;

/// Baut den passenden Treiber zu einem Laser-Profil (ADR 0006/0007). Der Treiber
/// trägt die Profil-Kalibrierung; er wird bei jedem Profilwechsel neu erzeugt.
pub(crate) fn driver_for(profile: &LaserProfile) -> Box<dyn MachineDriver + Send> {
    match profile.kind {
        DriverKind::Ruida => Box::new(luxifer_driver_ruida::RuidaDriver::from_profile(profile)),
        DriverKind::Grbl | DriverKind::MiniGrbl => {
            Box::new(luxifer_driver_grbl::GrblDriver::default())
        }
    }
}

/// Der aktive Treiber (aus dem aktiven Profil erzeugt) + dessen Profil-ID, um zu
/// erkennen, wann er neu gebaut werden muss.
#[derive(Default)]
pub(crate) struct ActiveDriver {
    pub(crate) id: Option<String>,
    pub(crate) driver: Option<Box<dyn MachineDriver + Send>>,
}

/// Das aktuell geöffnete Projekt (Metadaten, ohne die Geometrie — die lebt im
/// `AppState`). `None`, solange das Projekt noch namenlos ist.
#[derive(Default)]
pub(crate) struct CurrentProject {
    pub(crate) file: Option<ProjectFile>,
}

/// Geteilter Zustand über alle Commands.
pub(crate) struct AppData {
    pub(crate) state: Mutex<AppState>,
    pub(crate) current: Mutex<CurrentProject>,
    /// App-globale Laser-Registry (ADR 0007), beim Start geladen.
    pub(crate) lasers: Mutex<LaserRegistry>,
    /// Der aktive Treiber (lazy erzeugt/neu gebaut beim Profilwechsel).
    pub(crate) active: Mutex<ActiveDriver>,
}

/// Sperrt einen Mutex und HOLT den Guard auch bei Vergiftung zurück. Ein Panic,
/// während der Lock gehalten wurde, vergiftet ihn sonst dauerhaft: jedes weitere
/// `lock().unwrap()` panickt → die ganze App stirbt an EINEM fehlerhaften Command.
/// `into_inner()` gibt den Guard trotz Vergiftung her; der Zustand kann leicht
/// inkonsistent sein, aber die App bleibt bedienbar (Undo/Neuladen möglich).
fn lock_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl AppData {
    /// Der Editor-Zustand (vergiftungssicher gesperrt). Ersetzt
    /// `data.state.lock().unwrap()` in allen Commands.
    pub(crate) fn state(&self) -> std::sync::MutexGuard<'_, AppState> {
        lock_recover(&self.state)
    }

    /// Der Projektkontext (vergiftungssicher gesperrt).
    pub(crate) fn current(&self) -> std::sync::MutexGuard<'_, CurrentProject> {
        lock_recover(&self.current)
    }

    /// Die Laser-Registry (vergiftungssicher gesperrt).
    pub(crate) fn lasers(&self) -> std::sync::MutexGuard<'_, LaserRegistry> {
        lock_recover(&self.lasers)
    }

    /// Stellt sicher, dass der aktive Treiber zum aktuell aktiven Profil passt,
    /// und ruft `f` damit auf. Fehlt ein aktives Profil → Fehlertext.
    pub(crate) fn with_active_driver<T>(
        &self,
        f: impl FnOnce(&mut Box<dyn MachineDriver + Send>) -> Result<T, String>,
    ) -> Result<T, String> {
        let lasers = self.lasers();
        let profile = lasers
            .active()
            .ok_or_else(|| "Kein Laser aktiv — bitte in den Einstellungen anlegen.".to_string())?
            .clone();
        drop(lasers);

        let mut active = lock_recover(&self.active);
        // Treiber neu bauen, wenn Profil gewechselt hat oder noch keiner da ist.
        if active.id.as_deref() != Some(profile.id.as_str()) || active.driver.is_none() {
            active.driver = Some(driver_for(&profile));
            active.id = Some(profile.id.clone());
        }
        let driver = active.driver.as_mut().unwrap();
        f(driver)
    }
}

/// Metadaten des offenen Projekts fürs Frontend (Kopf im Designer/Toast).
#[derive(Serialize, Clone)]
pub(crate) struct ProjectMeta {
    name: String,
    description: String,
    tags: Vec<String>,
    /// ID der aktuellen Version (= was im Canvas ist), für die Markierung im Browser.
    current_version: String,
}

/// Schlanke Sicht auf den Zustand fürs Frontend (ohne Undo-Stacks).
#[derive(Serialize)]
pub(crate) struct Scene {
    layers: Vec<Layer>,
    shapes: Vec<Shape>,
    selected: Vec<usize>,
    /// Kanonische Welt-Bounding-Box der Auswahl (x, y, w, h).
    selection_bbox: Option<[f64; 4]>,
    bed_w_mm: f64,
    bed_h_mm: f64,
    /// Ungespeicherte Änderungen? Steuert den Unsaved-Guard im Frontend.
    dirty: bool,
    /// Offenes Projekt (Name/Beschreibung/Tags) oder `None`, wenn namenlos.
    project: Option<ProjectMeta>,
}

impl Scene {
    pub(crate) fn build(s: &AppState, cur: &CurrentProject) -> Self {
        Scene {
            layers: s.layers.clone(),
            shapes: s.shapes.clone(),
            selected: s.selected.clone(),
            selection_bbox: s.selection_bbox().map(|b| [b.x, b.y, b.w, b.h]),
            bed_w_mm: s.bed_w_mm,
            bed_h_mm: s.bed_h_mm,
            dirty: s.dirty,
            project: cur.file.as_ref().map(|f| ProjectMeta {
                name: f.name.clone(),
                description: f.description.clone(),
                tags: f.tags.clone(),
                current_version: f.current_version.clone(),
            }),
        }
    }
}

/// Ein Bewegungssegment der Laser-Vorschau fürs Frontend (ADR 0005). Schlanke,
/// serialisierbare Sicht auf `luxifer_core::preview::PreviewMove` — der Core
/// bleibt UI-frei, die Grenze zum Frontend liegt hier.
#[derive(Serialize)]
pub(crate) struct PreviewMoveDto {
    from: [f64; 2],
    to: [f64; 2],
    /// "Cut" | "Fill" | "Raster" | "Travel" — fürs Einfärben im Frontend.
    kind: &'static str,
    layer_id: usize,
    seq: u32,
}

/// Ein Bild-Layer als Textur fürs Frontend (ADR 0008 §2). Die Pixel (1 Byte je
/// Rasterzelle, 255 = gebrannt) als Base64 — kompakt, statt Hunderttausender
/// Segmente. Das Frontend lädt sie als GPU-Textur an ihrer mm-Box.
#[derive(Serialize)]
pub(crate) struct RasterTextureDto {
    /// Base64 der Texel-Bytes (row-major, width*height).
    pixels_b64: String,
    width: u32,
    height: u32,
    /// Tisch-Box (mm): x, y, w, h.
    rect: [f64; 4],
}

/// Die komplette Laser-Vorschau fürs Frontend.
#[derive(Serialize)]
pub(crate) struct PreviewDto {
    moves: Vec<PreviewMoveDto>,
    /// Bild-Layer als Texturen (statt Raster-Moves).
    rasters: Vec<RasterTextureDto>,
    /// (min_x, min_y, max_x, max_y) in mm, oder `None` bei leerem Job.
    bbox: Option<[f64; 4]>,
    total_len_mm: f64,
}

impl PreviewDto {
    pub(crate) fn from_preview(p: &JobPreview) -> Self {
        let kind_str = |k: MoveKind| match k {
            MoveKind::Cut => "Cut",
            MoveKind::Fill => "Fill",
            MoveKind::Raster => "Raster",
            MoveKind::Travel => "Travel",
        };
        PreviewDto {
            moves: p
                .moves
                .iter()
                .map(|m| PreviewMoveDto {
                    from: [m.from.0, m.from.1],
                    to: [m.to.0, m.to.1],
                    kind: kind_str(m.kind),
                    layer_id: m.layer_id,
                    seq: m.seq,
                })
                .collect(),
            rasters: p
                .rasters
                .iter()
                .map(|t| RasterTextureDto {
                    pixels_b64: base64_encode(&t.pixels),
                    width: t.width,
                    height: t.height,
                    rect: [t.x, t.y, t.w, t.h],
                })
                .collect(),
            bbox: p.bbox.map(|(a, b, c, d)| [a, b, c, d]),
            total_len_mm: p.total_len_mm,
        }
    }
}

/// Sperrt beide Zustände und baut die Scene (der übliche Rückgabewert).
pub(crate) fn scene(data: &State<AppData>) -> Scene {
    let s = data.state();
    let cur = data.current();
    Scene::build(&s, &cur)
}

/// Baut die Scene aus einem bereits gelockten AppState + dem Projektkontext.
pub(crate) fn scene_with(s: &AppState, data: &State<AppData>) -> Scene {
    let cur = data.current();
    Scene::build(s, &cur)
}

/// Baut den Job-Plan aus Shapes/Layern und lädt dabei Bild-Assets von der Platte
/// (Image-Layer werden gerastert). Von `job_preview` und den Laser-Commands
/// gemeinsam genutzt.
pub(crate) fn plan_with_assets(shapes: &[Shape], layers: &[Layer]) -> JobPlan {
    use luxifer_core::load_asset_luma;
    let dir = assets_dir();
    JobPlan::from_shapes_with_assets(shapes, layers, |asset| {
        let (pixels, w, h) = load_asset_luma(&dir, &asset.to_string()).ok()?;
        Some((std::borrow::Cow::Owned(pixels), w as usize, h as usize))
    })
}

/// Minimale Base64-Kodierung (Standard-Alphabet, mit Padding), ohne Fremd-Crate.
pub(crate) fn base64_encode(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(A[((n >> 18) & 63) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            A[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            A[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// `data:`-URL eines PNG von der Platte (für Thumbnails). `None` bei Lesefehler.
pub(crate) fn read_png_data_url(path: &std::path::Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(format!("data:image/png;base64,{}", base64_encode(&bytes)))
}

/// Merkt sich das zuletzt geöffnete/gespeicherte Projekt in den GUI-Settings
/// (für den Start-Toast). Fehler werden geschluckt — rein kosmetisch.
pub(crate) fn remember_last_project(name: &str) {
    use luxifer_core::UiSettings;
    let mut settings = UiSettings::load();
    if settings.last_project != name {
        settings.last_project = name.to_string();
        let _ = settings.save();
    }
}
