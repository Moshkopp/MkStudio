//! LuxiFer Tauri-Backend. Hält den `AppState` des Cores und stellt Commands
//! bereit. Das Frontend zeichnet nur — die gesamte Fachlogik bleibt im Core.

use std::sync::Mutex;

use luxifer_core::preview::{JobPreview, MoveKind};
use luxifer_core::{
    asset_meta, assets_dir, delete_project, import_image, list_projects, projects_dir,
    rename_project, rendered_png, Anchor, AppState, Connection, DriverKind, Geo, ImageParams,
    JobAction, JobParams, JobPlan, Layer, LaserProfile, LaserRegistry, MachineDriver, PolyShape,
    ProjectFile, ProjectInfo, Shape, ShapeInfo, StartMode, Tab, UiSettings, VersionInfo,
};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

/// Baut den passenden Treiber zu einem Laser-Profil (ADR 0006/0007). Der Treiber
/// trägt die Profil-Kalibrierung; er wird bei jedem Profilwechsel neu erzeugt.
fn driver_for(profile: &LaserProfile) -> Box<dyn MachineDriver + Send> {
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
struct ActiveDriver {
    id: Option<String>,
    driver: Option<Box<dyn MachineDriver + Send>>,
}

/// Das aktuell geöffnete Projekt (Metadaten, ohne die Geometrie — die lebt im
/// `AppState`). `None`, solange das Projekt noch namenlos ist.
#[derive(Default)]
struct CurrentProject {
    file: Option<ProjectFile>,
}

/// Geteilter Zustand über alle Commands.
struct AppData {
    state: Mutex<AppState>,
    current: Mutex<CurrentProject>,
    /// App-globale Laser-Registry (ADR 0007), beim Start geladen.
    lasers: Mutex<LaserRegistry>,
    /// Der aktive Treiber (lazy erzeugt/neu gebaut beim Profilwechsel).
    active: Mutex<ActiveDriver>,
}

impl AppData {
    /// Stellt sicher, dass der aktive Treiber zum aktuell aktiven Profil passt,
    /// und ruft `f` damit auf. Fehlt ein aktives Profil → Fehlertext.
    fn with_active_driver<T>(
        &self,
        f: impl FnOnce(&mut Box<dyn MachineDriver + Send>) -> Result<T, String>,
    ) -> Result<T, String> {
        let lasers = self.lasers.lock().unwrap();
        let profile = lasers
            .active()
            .ok_or_else(|| "Kein Laser aktiv — bitte in den Einstellungen anlegen.".to_string())?
            .clone();
        drop(lasers);

        let mut active = self.active.lock().unwrap();
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
struct ProjectMeta {
    name: String,
    description: String,
    tags: Vec<String>,
    /// ID der aktuellen Version (= was im Canvas ist), für die Markierung im Browser.
    current_version: String,
}

/// Schlanke Sicht auf den Zustand fürs Frontend (ohne Undo-Stacks).
#[derive(Serialize)]
struct Scene {
    layers: Vec<Layer>,
    shapes: Vec<Shape>,
    selected: Vec<usize>,
    bed_w_mm: f64,
    bed_h_mm: f64,
    /// Ungespeicherte Änderungen? Steuert den Unsaved-Guard im Frontend.
    dirty: bool,
    /// Offenes Projekt (Name/Beschreibung/Tags) oder `None`, wenn namenlos.
    project: Option<ProjectMeta>,
}

impl Scene {
    fn build(s: &AppState, cur: &CurrentProject) -> Self {
        Scene {
            layers: s.layers.clone(),
            shapes: s.shapes.clone(),
            selected: s.selected.clone(),
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
struct PreviewMoveDto {
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
struct RasterTextureDto {
    /// Base64 der Texel-Bytes (row-major, width*height).
    pixels_b64: String,
    width: u32,
    height: u32,
    /// Tisch-Box (mm): x, y, w, h.
    rect: [f64; 4],
}

/// Die komplette Laser-Vorschau fürs Frontend.
#[derive(Serialize)]
struct PreviewDto {
    moves: Vec<PreviewMoveDto>,
    /// Bild-Layer als Texturen (statt Raster-Moves).
    rasters: Vec<RasterTextureDto>,
    /// (min_x, min_y, max_x, max_y) in mm, oder `None` bei leerem Job.
    bbox: Option<[f64; 4]>,
    total_len_mm: f64,
}

impl PreviewDto {
    fn from_preview(p: &JobPreview) -> Self {
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

// Sperrt beide Zustände und baut die Scene (der übliche Rückgabewert).
fn scene(data: &State<AppData>) -> Scene {
    let s = data.state.lock().unwrap();
    let cur = data.current.lock().unwrap();
    Scene::build(&s, &cur)
}

// Baut die Scene aus einem bereits gelockten AppState + dem Projektkontext.
// Ersetzt das frühere `Scene::from_state(&s)` in den einzelnen Commands.
fn scene_with(s: &AppState, data: &State<AppData>) -> Scene {
    let cur = data.current.lock().unwrap();
    Scene::build(s, &cur)
}

#[tauri::command]
fn get_scene(data: State<AppData>) -> Scene {
    scene(&data)
}

#[tauri::command]
fn swatch_colors() -> Vec<[u8; 3]> {
    luxifer_core::SWATCH_COLORS.to_vec()
}

#[tauri::command]
fn add_rect(data: State<AppData>, x: f64, y: f64, w: f64, h: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.add_shape(Geo::Rect { x, y, w, h });
    scene_with(&s, &data)
}

#[tauri::command]
fn add_ellipse(data: State<AppData>, cx: f64, cy: f64, rx: f64, ry: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.add_shape(Geo::Ellipse { cx, cy, rx, ry });
    scene_with(&s, &data)
}

/// Fügt eine offene 2-Punkt-Linie als Polyline hinzu.
#[tauri::command]
fn add_line(data: State<AppData>, x1: f64, y1: f64, x2: f64, y2: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.add_shape(Geo::Polyline {
        pts: vec![(x1, y1), (x2, y2)],
        closed: false,
    });
    scene_with(&s, &data)
}

/// Fügt eine Polylinie aus den gelieferten Punkten hinzu. `closed` schließt die
/// Kontur (letzter → erster Punkt). Wird ignoriert, wenn < 2 Punkte kommen.
#[tauri::command]
fn add_polyline(data: State<AppData>, pts: Vec<(f64, f64)>, closed: bool) -> Scene {
    let mut s = data.state.lock().unwrap();
    if pts.len() >= 2 {
        s.add_shape(Geo::Polyline { pts, closed });
    }
    scene_with(&s, &data)
}

/// Importiert ein Bild (ADR 0004): legt die (Graustufen-)Kopie im Asset-Store ab
/// und fügt ein Bild-Objekt auf einem eigenen Image-Layer ein. `bytes` sind die
/// rohen Bytes der vom Nutzer gewählten Datei (das Frontend liest sie über einen
/// `<input type=file>` — kein Tauri-Dialog nötig), `name` der Anzeigename.
///
/// Die Zielgröße in mm ergibt sich aus den Pixelmaßen bei 96 DPI, begrenzt auf
/// 80 % der Bettgröße (ein 4K-Bild soll nicht riesig platziert werden), und wird
/// mittig aufs Bett gesetzt. Seitenverhältnis bleibt erhalten.
#[tauri::command]
fn import_image_file(data: State<AppData>, bytes: Vec<u8>, name: String) -> Result<Scene, String> {
    let meta = import_image(&assets_dir(), &bytes, &name).map_err(|e| e.to_string())?;

    let mut s = data.state.lock().unwrap();
    // px → mm bei 96 DPI.
    const PX_TO_MM: f64 = 25.4 / 96.0;
    let mut w = meta.width as f64 * PX_TO_MM;
    let mut h = meta.height as f64 * PX_TO_MM;
    // Auf 80 % der Bettgröße begrenzen, Seitenverhältnis wahren.
    let max_w = s.bed_w_mm * 0.8;
    let max_h = s.bed_h_mm * 0.8;
    if w > max_w || h > max_h {
        let scale = (max_w / w).min(max_h / h);
        w *= scale;
        h *= scale;
    }
    // Mittig aufs Bett.
    let x = (s.bed_w_mm - w) / 2.0;
    let y = (s.bed_h_mm - h) / 2.0;
    s.add_image(meta.id, x, y, w, h);
    Ok(scene_with(&s, &data))
}

/// Rendert ein Asset mit den gegebenen Parametern und gibt es als PNG-Data-URL
/// zurück (Canvas-Darstellung bzw. Editor-Vorschau). `invert` = Editor- oder
/// Laser-Invert (der Aufrufer wählt); für die Canvas-Anzeige `invert_editor`.
#[tauri::command]
fn image_render(asset: String, params: ImageParams, invert: bool) -> Option<String> {
    let png = rendered_png(&assets_dir(), &asset, &params, invert).ok()?;
    Some(format!("data:image/png;base64,{}", base64_encode(&png)))
}

/// Setzt die Bild-Parameter eines Bild-Shapes (Editor). `index` ist der
/// Shape-Index; nicht-Bild-Shapes werden ignoriert.
#[tauri::command]
fn set_image_params(data: State<AppData>, index: usize, params: ImageParams) -> Scene {
    let mut s = data.state.lock().unwrap();
    if let Some(shape) = s.shapes.get_mut(index) {
        if let Geo::Image { params: p, .. } = &mut shape.geo {
            *p = params;
        }
    }
    scene_with(&s, &data)
}

/// Katalog der parametrischen Formen für die Galerie im Werkzeug-Panel.
/// Datengetrieben: eine neue Form im Core erscheint hier automatisch.
#[tauri::command]
fn shape_catalog() -> Vec<ShapeInfo> {
    PolyShape::catalog()
}

/// Fügt eine parametrische Form als geschlossene Polylinie hinzu.
/// `shape` = stabiler Bezeichner aus dem Katalog (z. B. "hex"); unbekannte
/// Bezeichner werden ignoriert (Zustand bleibt unverändert).
#[tauri::command]
fn add_polygon(data: State<AppData>, shape: String, cx: f64, cy: f64, r: f64, rot: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    if let Some(kind) = PolyShape::from_id(&shape) {
        let pts = kind.points(cx, cy, r, rot);
        if pts.len() >= 3 {
            s.add_shape(Geo::Polyline { pts, closed: true });
        }
    }
    scene_with(&s, &data)
}

#[tauri::command]
fn activate_color(data: State<AppData>, color: [u8; 3]) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.activate_color(color);
    scene_with(&s, &data)
}

#[tauri::command]
fn select_at(data: State<AppData>, x: f64, y: f64, tol: f64, additive: bool) -> Scene {
    let mut s = data.state.lock().unwrap();
    match s.hit_test(x, y, tol) {
        Some(idx) => {
            if additive {
                // Toggle: enthalten → raus, sonst rein.
                if let Some(pos) = s.selected.iter().position(|&i| i == idx) {
                    s.selected.remove(pos);
                } else {
                    s.selected.push(idx);
                }
            } else if !s.selected.contains(&idx) {
                s.selected = vec![idx];
            }
        }
        None => {
            if !additive {
                s.selected.clear();
            }
        }
    }
    // Gruppen sind eine Einheit: Auswahl auf ganze Gruppen erweitern.
    s.expand_selection_to_groups();
    scene_with(&s, &data)
}

/// Marquee-Auswahl: alle Shapes, deren BBox vollständig im Rechteck liegt.
#[tauri::command]
fn select_rect(data: State<AppData>, x1: f64, y1: f64, x2: f64, y2: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.select_in_rect(x1, y1, x2, y2);
    s.expand_selection_to_groups();
    scene_with(&s, &data)
}

/// Gruppiert die Auswahl (Shapes verhalten sich danach als Einheit).
#[tauri::command]
fn group_op(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.group_selected();
    scene_with(&s, &data)
}

/// Löst die Gruppierung der Auswahl.
#[tauri::command]
fn ungroup_op(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.ungroup_selected();
    scene_with(&s, &data)
}

/// Verschiebt die Auswahl um ein Gesamt-Delta (ein Undo-Punkt pro Geste).
#[tauri::command]
fn move_selected(data: State<AppData>, dx: f64, dy: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    if dx != 0.0 || dy != 0.0 {
        s.push_undo();
        s.translate_selected(dx, dy);
    }
    scene_with(&s, &data)
}

/// Skaliert die Auswahl von der Start-Gruppenbox auf die Zielbox (ein Undo-Punkt).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn scale_selected(
    data: State<AppData>,
    sx: f64,
    sy: f64,
    sw: f64,
    sh: f64,
    tx: f64,
    ty: f64,
    tw: f64,
    th: f64,
) -> Scene {
    use luxifer_core::BBox;
    let mut s = data.state.lock().unwrap();
    s.push_undo();
    s.scale_selection_to(BBox::new(sx, sy, sw, sh), BBox::new(tx, ty, tw, th));
    scene_with(&s, &data)
}

#[tauri::command]
fn align(data: State<AppData>, kind: String) -> Scene {
    use luxifer_core::Align;
    let mut s = data.state.lock().unwrap();
    let k = match kind.as_str() {
        "left" => Align::Left,
        "hcenter" => Align::HCenter,
        "right" => Align::Right,
        "top" => Align::Top,
        "vcenter" => Align::VCenter,
        "bottom" => Align::Bottom,
        _ => return scene_with(&s, &data),
    };
    s.align_selection(k);
    scene_with(&s, &data)
}

#[tauri::command]
fn distribute(data: State<AppData>, kind: String) -> Scene {
    use luxifer_core::Distribute;
    let mut s = data.state.lock().unwrap();
    let k = match kind.as_str() {
        "h" => Distribute::Horizontal,
        "v" => Distribute::Vertical,
        _ => return scene_with(&s, &data),
    };
    s.distribute_selection(k);
    scene_with(&s, &data)
}

/// Importiert eine Vektordatei (SVG/DXF): Konturen als Polylinien auf dem
/// aktiven Layer (ein Undo-Punkt). Die Endung des Dateinamens entscheidet.
#[tauri::command]
fn import_vector_file(data: State<AppData>, bytes: Vec<u8>, name: String) -> Result<Scene, String> {
    let ext = name.rsplit('.').next().unwrap_or("");
    let contours =
        luxifer_core::import::import_vector(&bytes, ext).map_err(|e| e.to_string())?;
    let mut s = data.state.lock().unwrap();
    s.add_polylines(contours);
    Ok(scene_with(&s, &data))
}

/// Füllt die Auswahl mit einem Muster (Pattern-Fill, wie v1: Linien, Kreise,
/// Slots, Waben). Alle selektierten geschlossenen Konturen wirken gemeinsam
/// als Ringe (innere = Löcher). Ein Undo-Punkt; die Konturen bleiben.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn pattern_fill_op(
    data: State<AppData>,
    pattern: String,
    gap_x: f64,
    gap_y: f64,
    angle: f64,
    size: f64,
) -> Result<Scene, String> {
    use luxifer_core::pattern_fill::{FillParams, Pattern};
    let Some(pat) = Pattern::from_key(&pattern) else {
        return Err(format!("Unbekanntes Muster: {pattern}"));
    };
    let mut s = data.state.lock().unwrap();
    s.pattern_fill_selected(&FillParams {
        pattern: pat,
        gap_x,
        gap_y,
        angle_deg: angle,
        size,
    });
    Ok(scene_with(&s, &data))
}

/// Fügt eine Spline hinzu: Catmull-Rom-Kurve durch die geklickten Punkte
/// (Zeichenfluss wie die Polylinie; die Glättung passiert im Core).
#[tauri::command]
fn add_spline(data: State<AppData>, pts: Vec<(f64, f64)>, closed: bool) -> Scene {
    use luxifer_core::geometry::catmull_rom;
    let mut s = data.state.lock().unwrap();
    let smooth = catmull_rom(&pts, closed, 12);
    s.add_shape(Geo::Polyline {
        pts: smooth,
        closed,
    });
    scene_with(&s, &data)
}

/// Eigener Fonts-Ordner der App (<data_root>/Fonts) — wie v3s Fonts-Ablage.
fn fonts_dir() -> std::path::PathBuf {
    luxifer_core::data_root().join("Fonts")
}

/// Installiert einen Font (TTF/OTF-Bytes) in den App-Fonts-Ordner.
#[tauri::command]
fn upload_font(bytes: Vec<u8>, name: String) -> Result<String, String> {
    // Vorab prüfen, dass der Font lesbar ist (sonst Datenmüll im Ordner).
    luxifer_core::text::text_to_contours(&bytes, "Ag", 10.0).map_err(|e| e.to_string())?;
    let dir = fonts_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe = name.replace(['/', '\\'], "_");
    let path = dir.join(&safe);
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Ein installierter Font (fürs Text-Werkzeug).
#[derive(Serialize)]
struct FontInfo {
    name: String,
    path: String,
}

/// Listet die System-Fonts (TTF/OTF) aus den üblichen Verzeichnissen.
#[tauri::command]
fn list_fonts() -> Vec<FontInfo> {
    let home = std::env::var("HOME").unwrap_or_default();
    // Eigene Fonts der App zuerst (erscheinen oben in der Liste).
    let dirs = [
        fonts_dir().to_string_lossy().to_string(),
        "/usr/share/fonts".to_string(),
        "/usr/local/share/fonts".to_string(),
        format!("{home}/.fonts"),
        format!("{home}/.local/share/fonts"),
    ];
    let mut out: Vec<FontInfo> = Vec::new();
    for dir in dirs {
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "ttf" || x == "otf") {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        out.push(FontInfo {
                            name: stem.to_string(),
                            path: p.to_string_lossy().to_string(),
                        });
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// Fügt Text als Vektorpfade ein (Text→Pfad, ein Undo-Punkt). Der Text landet
/// bei 10 % der Bettmaße; verschieben/skalieren wie jede andere Form.
#[tauri::command]
fn add_text(
    data: State<AppData>,
    text: String,
    font_path: String,
    size_mm: f64,
) -> Result<Scene, String> {
    if text.trim().is_empty() {
        return Err("Kein Text eingegeben.".into());
    }
    let bytes = std::fs::read(&font_path).map_err(|e| format!("Font nicht lesbar: {e}"))?;
    let contours = luxifer_core::text::text_to_contours(&bytes, &text, size_mm.clamp(1.0, 500.0))
        .map_err(|e| e.to_string())?;
    if contours.is_empty() {
        return Err("Der Font liefert für diesen Text keine Konturen.".into());
    }
    let mut s = data.state.lock().unwrap();
    let (ox, oy) = (s.bed_w_mm * 0.1, s.bed_h_mm * 0.1);
    let placed: Vec<(Vec<(f64, f64)>, bool)> = contours
        .into_iter()
        .map(|(c, closed)| (c.into_iter().map(|(x, y)| (x + ox, y + oy)).collect(), closed))
        .collect();
    // Als Text-Block: eine Gruppe + Quelldaten fürs spätere Editieren.
    s.add_text_block(
        placed,
        luxifer_core::TextMeta {
            text,
            font_path,
            size_mm,
        },
    );
    Ok(scene_with(&s, &data))
}

/// Vorschau-Konturen für den Text-Dialog (mm, Ursprung oben links). Reine
/// Anzeige — die Wahrheit erzeugt add_text/update_text im Core.
#[tauri::command]
fn text_preview(
    text: String,
    font_path: String,
    size_mm: f64,
) -> Result<Vec<(Vec<(f64, f64)>, bool)>, String> {
    let bytes = std::fs::read(&font_path).map_err(|e| e.to_string())?;
    luxifer_core::text::text_to_contours(&bytes, &text, size_mm.clamp(1.0, 500.0))
        .map_err(|e| e.to_string())
}

/// Ersetzt einen bestehenden Text-Block (Doppelklick-Edit): neuer Text/Font/
/// Größe, gleiche Position, gleicher Layer.
#[tauri::command]
fn update_text(
    data: State<AppData>,
    shape_index: usize,
    text: String,
    font_path: String,
    size_mm: f64,
) -> Result<Scene, String> {
    if text.trim().is_empty() {
        return Err("Kein Text eingegeben.".into());
    }
    let bytes = std::fs::read(&font_path).map_err(|e| format!("Font nicht lesbar: {e}"))?;
    let contours = luxifer_core::text::text_to_contours(&bytes, &text, size_mm.clamp(1.0, 500.0))
        .map_err(|e| e.to_string())?;
    if contours.is_empty() {
        return Err("Der Font liefert für diesen Text keine Konturen.".into());
    }
    let mut s = data.state.lock().unwrap();
    s.replace_text_block(
        shape_index,
        contours,
        luxifer_core::TextMeta {
            text,
            font_path,
            size_mm,
        },
    );
    Ok(scene_with(&s, &data))
}

/// Vektorisiert ein Bild-Shape (Trace): Konturen des Motivs als geschlossene
/// Polylinien in mm, auf dem aktiven Zeichen-Layer (ein Undo-Punkt). Die
/// Tonwert-LUT des Bildes (Helligkeit/Kontrast/Gamma) wirkt vor der Schwelle.
#[tauri::command]
fn trace_image(
    data: State<AppData>,
    shape_index: usize,
    threshold: u8,
    invert: bool,
) -> Result<Scene, String> {
    use luxifer_core::geometry::{Geo, ImageMode, ImageParams};
    use luxifer_core::trace::{trace, TraceParams};

    let mut s = data.state.lock().unwrap();
    let (asset, bx, by, bw, bh, params) = match s.shapes.get(shape_index).map(|sh| &sh.geo) {
        Some(Geo::Image {
            asset,
            x,
            y,
            w,
            h,
            params,
        }) => (asset.clone(), *x, *y, *w, *h, *params),
        _ => return Err("Kein Bild ausgewählt.".into()),
    };
    let (px, w, h) =
        luxifer_core::load_asset_luma(&assets_dir(), &asset).map_err(|e| e.to_string())?;
    // Tonwerte anwenden (nur LUT), dann tracen.
    let lut_params = ImageParams {
        mode: ImageMode::Grayscale,
        ..params
    };
    let gray = luxifer_core::apply_params(&px, &lut_params, false);
    let contours = trace(
        &gray,
        w as usize,
        h as usize,
        &TraceParams {
            threshold,
            invert,
            ..Default::default()
        },
    );
    if contours.is_empty() {
        return Err("Keine Konturen gefunden — Schwelle anpassen?".into());
    }
    // Pixel → mm über die Bildbox.
    let (sx, sy) = (bw / w as f64, bh / h as f64);
    let mm: Vec<(Vec<(f64, f64)>, bool)> = contours
        .into_iter()
        .map(|c| {
            (
                c.into_iter()
                    .map(|(x, y)| (bx + x * sx, by + y * sy))
                    .collect(),
                true,
            )
        })
        .collect();
    s.add_polylines(mm);
    Ok(scene_with(&s, &data))
}

/// Boolesche Operation auf der Auswahl: "union" | "intersect" | "diff".
/// Subjekt = zuerst selektierte Shape; die Eingaben werden ersetzt.
#[tauri::command]
fn boolean_op(data: State<AppData>, op: String) -> Scene {
    use luxifer_core::BoolOp;
    let mut s = data.state.lock().unwrap();
    let o = match op.as_str() {
        "union" => BoolOp::Union,
        "intersect" => BoolOp::Intersect,
        "diff" => BoolOp::Difference,
        _ => return scene_with(&s, &data),
    };
    s.boolean_selected(o);
    scene_with(&s, &data)
}

/// Parallele Kontur (mm) zu jeder selektierten Shape hinzufügen.
/// Positiv = außen, negativ = innen; das Original bleibt.
#[tauri::command]
fn offset_op(data: State<AppData>, dist: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.offset_selected(dist);
    scene_with(&s, &data)
}

/// Ecken der selektierten Shapes mit Radius (mm) verrunden.
#[tauri::command]
fn fillet_op(data: State<AppData>, radius: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.fillet_selected(radius);
    scene_with(&s, &data)
}

/// Packt die Auswahl platzsparend aufs Bett (Nesting, `gap` mm Abstand).
#[tauri::command]
fn nest_op(data: State<AppData>, gap: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.nest_selected(gap);
    scene_with(&s, &data)
}

/// Spiegelt die Auswahl an der Mittelachse ihrer gemeinsamen BBox.
/// `axis`: "h" = horizontal spiegeln (links↔rechts, vertikale Achse),
/// "v" = vertikal spiegeln (oben↔unten, horizontale Achse).
#[tauri::command]
fn mirror(data: State<AppData>, axis: String) -> Scene {
    use luxifer_core::Axis;
    let mut s = data.state.lock().unwrap();
    let a = match axis.as_str() {
        "h" => Axis::Vertical,
        "v" => Axis::Horizontal,
        _ => return scene_with(&s, &data),
    };
    s.mirror_selection(a);
    scene_with(&s, &data)
}

#[tauri::command]
fn clear_selection(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.selected.clear();
    scene_with(&s, &data)
}

#[tauri::command]
fn delete_selected(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.delete_selected();
    scene_with(&s, &data)
}

/// Vom Frontend gelieferte Layer-Parameter (Doppelklick-Dialog).
#[derive(serde::Deserialize)]
struct LayerParams {
    name: String,
    mode: String,
    speed_mm_s: f64,
    power_pct: f64,
    min_power_pct: f64,
    passes: u32,
    air_assist: bool,
    line_step_mm: f64,
    dpi: f64,
    #[serde(default = "default_bidirectional")]
    bidirectional: bool,
}

fn default_bidirectional() -> bool {
    true
}

/// Setzt die Parameter eines Layers (ein Undo-Punkt).
#[tauri::command]
fn set_layer_params(data: State<AppData>, index: usize, p: LayerParams) -> Scene {
    use luxifer_core::LayerMode;
    let mut s = data.state.lock().unwrap();
    if index < s.layers.len() {
        s.push_undo();
        let l = &mut s.layers[index];
        l.name = p.name;
        l.mode = match p.mode.as_str() {
            "Fill" => LayerMode::Fill,
            "Raster" => LayerMode::Raster,
            "Image" => LayerMode::Image,
            _ => LayerMode::Cut,
        };
        l.speed_mm_s = p.speed_mm_s;
        l.power_pct = p.power_pct;
        l.min_power_pct = p.min_power_pct;
        l.passes = p.passes;
        l.air_assist = p.air_assist;
        l.line_step_mm = p.line_step_mm;
        l.dpi = p.dpi;
        l.bidirectional = p.bidirectional;
    }
    scene_with(&s, &data)
}

/// Schalter eines Layers umschalten (Anzeige, Brennen, Luft, Sperre).
#[tauri::command]
fn toggle_layer(data: State<AppData>, index: usize, field: String) -> Scene {
    let mut s = data.state.lock().unwrap();
    if let Some(l) = s.layers.get_mut(index) {
        match field.as_str() {
            "visible" => l.visible = !l.visible,          // Objekte anzeigen
            "enabled" => l.enabled = !l.enabled,          // im Job mitbrennen
            "air_assist" => l.air_assist = !l.air_assist, // Luftunterstützung
            "locked" => l.locked = !l.locked,             // Editiersperre
            _ => {}
        }
    }
    scene_with(&s, &data)
}

/// Verschiebt einen Layer in der Brenn-Reihenfolge (ADR 0005 §0). `from`/`to`
/// sind Layer-Indizes; der Core remappt dabei alle `shape.layer_id`. Ein
/// Undo-Punkt entsteht nur bei tatsächlicher Bewegung.
#[tauri::command]
fn move_layer(data: State<AppData>, from: usize, to: usize) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.move_layer(from, to);
    scene_with(&s, &data)
}

/// Baut den `JobPlan` aus Shapes + Layern **mit Asset-Auflösung**: Bild-Assets
/// werden aus dem Store gelesen und zu Graustufen-Pixeln dekodiert, damit
/// Image-Layer gerastert werden (raster.rs). Der Core selbst fasst die Platte
/// nicht an — diese Closure liefert ihm die Pixel. Dekodierfehler/fehlende
/// Assets ⇒ Layer wird still übersprungen (der Job bleibt baubar).
fn plan_with_assets(shapes: &[Shape], layers: &[Layer]) -> JobPlan {
    use luxifer_core::load_asset_luma;
    let dir = assets_dir();
    JobPlan::from_shapes_with_assets(shapes, layers, |asset| {
        let (pixels, w, h) = load_asset_luma(&dir, &asset.to_string()).ok()?;
        Some((std::borrow::Cow::Owned(pixels), w as usize, h as usize))
    })
}

/// Leitet aus dem aktuellen Zustand die Laser-Vorschau ab (ADR 0005): die zu
/// fahrenden Segmente in Ausführungsreihenfolge inkl. Verfahrwege. Reine
/// Ableitung des `JobPlan` — kein Undo, keine Mutation.
#[tauri::command]
fn job_preview(data: State<AppData>) -> PreviewDto {
    let s = data.state.lock().unwrap();
    let plan = plan_with_assets(&s.shapes, &s.layers);
    let preview = JobPreview::from_plan(&plan);
    PreviewDto::from_preview(&preview)
}

// --- Laser-Profile & gerätespezifische Aktionen (ADR 0007) ------------------

/// Job-Parameter aus dem Frontend („Starten von" + 3×3-Anker-Index).
#[derive(Deserialize, Default)]
struct JobParamsDto {
    /// "absolut" | "aktuell" | "ursprung".
    start_mode: String,
    /// 3×3-Index 0..8 (4 = Mitte).
    anchor: usize,
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
struct ExportDto {
    bytes: Vec<u8>,
    filename: String,
}

/// Kopf- und Ursprungsposition (mm) fürs Canvas. `origin` fehlt, wenn der
/// Controller keinen Benutzerursprung meldet.
#[derive(Serialize)]
struct PositionDto {
    head: [f64; 2],
    origin: Option<[f64; 2]>,
}

/// Gibt die gesamte Laser-Registry ans Frontend (Dropdown + Settings-Liste).
#[tauri::command]
fn laser_list(data: State<AppData>) -> LaserRegistry {
    data.lasers.lock().unwrap().clone()
}

/// Legt ein Profil an oder ersetzt ein bestehendes (gleiche ID). Ohne ID wird
/// eine neue vergeben. Speichert und gibt die aktualisierte Registry zurück.
#[tauri::command]
fn laser_save(data: State<AppData>, mut profile: LaserProfile) -> Result<LaserRegistry, String> {
    let mut lasers = data.lasers.lock().unwrap();
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
fn laser_delete(data: State<AppData>, id: String) -> Result<LaserRegistry, String> {
    let mut lasers = data.lasers.lock().unwrap();
    lasers.remove(&id);
    lasers.save()?;
    Ok(lasers.clone())
}

/// Setzt den aktiven Laser (Panel-Dropdown), speichert und gibt die Registry
/// zurück. Der Treiber wird beim nächsten Aktions-Aufruf passend neu gebaut.
#[tauri::command]
fn laser_set_active(data: State<AppData>, id: String) -> Result<LaserRegistry, String> {
    let mut lasers = data.lasers.lock().unwrap();
    if !lasers.set_active(&id) {
        return Err("Unbekannter Laser.".into());
    }
    lasers.save()?;
    Ok(lasers.clone())
}

/// Die Job-Aktionen, die der aktive Treiber anbietet (als String-Schlüssel).
/// Leer, wenn kein Laser aktiv ist.
#[tauri::command]
fn laser_actions(data: State<AppData>) -> Vec<String> {
    data.with_active_driver(|d| Ok(d.actions().iter().map(|a| a.as_key().to_string()).collect()))
        .unwrap_or_default()
}

/// Führt eine gemeldete Job-Aktion aus (Senden/Rahmen/Home/Stop/Export). Der
/// Treiber entscheidet, was intern passiert (ADR 0007). Gibt eine Meldung zurück.
#[tauri::command]
fn laser_run_action(
    data: State<AppData>,
    action: String,
    params: JobParamsDto,
) -> Result<String, String> {
    let job_action = action_from_key(&action)?;
    let (plan, layers) = {
        let s = data.state.lock().unwrap();
        (plan_with_assets(&s.shapes, &s.layers), s.layers.clone())
    };
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
fn laser_export(data: State<AppData>, params: JobParamsDto) -> Result<ExportDto, String> {
    let (plan, layers) = {
        let s = data.state.lock().unwrap();
        (plan_with_assets(&s.shapes, &s.layers), s.layers.clone())
    };
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
fn laser_jog(data: State<AppData>, dx: f64, dy: f64, speed: f64) -> Result<(), String> {
    data.with_active_driver(|d| {
        connect_active(d, &data)?;
        d.jog(dx, dy, speed).map_err(|e| e.to_string())
    })
}

/// Referenzfahrt (Home). Verbindet automatisch.
#[tauri::command]
fn laser_home(data: State<AppData>, speed: f64) -> Result<(), String> {
    data.with_active_driver(|d| {
        connect_active(d, &data)?;
        d.home(speed).map_err(|e| e.to_string())
    })
}

/// Liest Kopf- und Ursprungsposition (mm) für die Canvas-Anzeige (auf Knopfdruck).
/// Verbindet automatisch.
#[tauri::command]
fn laser_position(data: State<AppData>) -> Result<PositionDto, String> {
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
fn laser_ping(data: State<AppData>) -> bool {
    let lasers = data.lasers.lock().unwrap();
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
    let lasers = data.lasers.lock().unwrap();
    let profile = lasers.active().ok_or("Kein Laser aktiv.")?;
    let target = match &profile.connection {
        Connection::Netz { ip, .. } => ip.clone(),
        Connection::Seriell { port, .. } => port.clone(),
    };
    drop(lasers);
    driver.connect(&target).map_err(|e| e.to_string())
}

/// Lädt die GUI-Settings (Panel-Layouts, Theming, Arbeitsplatz) — ADR 0002.
/// Fehlt die Datei, kommt der Default zurück; die GUI startet immer.
#[tauri::command]
fn get_ui_settings() -> UiSettings {
    UiSettings::load()
}

/// Speichert die vom Frontend gelieferten GUI-Settings lokal als JSON.
/// Werte werden vor dem Schreiben geklemmt/aufgeräumt (sanitize).
#[tauri::command]
fn save_ui_settings(mut settings: UiSettings) -> Result<UiSettings, String> {
    settings.sanitize();
    settings.save()?;
    Ok(settings)
}

/// Setzt einen Reiter auf sein Standard-Layout zurück (ADR §2), speichert und
/// gibt die aktualisierten Settings zurück. Andere Reiter bleiben unberührt.
#[tauri::command]
fn reset_ui_tab(tab: Tab) -> Result<UiSettings, String> {
    let mut settings = UiSettings::load();
    settings.reset_tab(tab);
    settings.save()?;
    Ok(settings)
}

// ---- Projektverwaltung (ADR 0003) -----------------------------------------

/// Volle Detailansicht eines Projekts (rechte Seite im Browser).
#[derive(Serialize)]
struct ProjectDetail {
    name: String,
    description: String,
    tags: Vec<String>,
    created_at: String,
    modified_at: String,
    versions: Vec<VersionInfo>,
    current_version: String,
    asset_refs: Vec<String>,
}

/// Neues, leeres Projekt: Zeichenfläche leeren, Projektkontext zurücksetzen.
#[tauri::command]
fn new_project(data: State<AppData>) -> Scene {
    {
        let mut s = data.state.lock().unwrap();
        *s = AppState::new();
    }
    {
        let mut cur = data.current.lock().unwrap();
        cur.file = None;
    }
    scene(&data)
}

/// Speichert das Projekt. Ist noch keins offen (namenlos), wird mit den
/// gelieferten Metadaten ein neues angelegt; sonst wird der Arbeitsstand des
/// offenen Projekts überschrieben. `thumb_png` sind fertige PNG-Bytes (Frontend).
#[tauri::command]
fn save_project(
    data: State<AppData>,
    name: String,
    description: String,
    tags: Vec<String>,
    thumb_png: Vec<u8>,
) -> Result<Scene, String> {
    let dir = projects_dir();
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Projektname darf nicht leer sein.".into());
    }
    let mut s = data.state.lock().unwrap();
    let mut cur = data.current.lock().unwrap();

    // Bestehendes Projekt aktualisieren oder neues anlegen. save_current schreibt
    // Projektdatei + Snapshot + Thumbnail der aktuellen Version (kein separates,
    // driftendes thumbnail.png mehr — das Thumbnail hängt an der Version).
    let mut pf = match cur.file.take() {
        Some(mut existing) => {
            existing.name = name.clone();
            existing.description = description;
            existing.tags = tags;
            existing.update_from_state(&s);
            existing
        }
        None => {
            let mut pf = ProjectFile::from_state(&s, &name, tags);
            pf.description = description;
            pf
        }
    };
    pf.save_current(&dir, &thumb_png)?;
    s.mark_saved();
    cur.file = Some(pf);
    remember_last_project(&name);
    Ok(Scene::build(&s, &cur))
}

/// Hält den aktuellen Stand als **neue Version** fest (Shift+Strg+S). Verlangt
/// ein bereits gespeichertes (benanntes) Projekt.
#[tauri::command]
fn save_version(data: State<AppData>, note: String, thumb_png: Vec<u8>) -> Result<Scene, String> {
    let dir = projects_dir();
    let mut s = data.state.lock().unwrap();
    let mut cur = data.current.lock().unwrap();
    let Some(pf) = cur.file.as_mut() else {
        return Err("Bitte zuerst das Projekt speichern (Strg+S).".into());
    };
    // Arbeitsstand ins ProjectFile übernehmen, dann als neue Version festhalten.
    // add_version schreibt Snapshot + Thumbnail der neuen Version und macht sie
    // zur aktuellen; ein separates thumbnail.png gibt es nicht mehr.
    pf.update_from_state(&s);
    pf.add_version(&dir, note, &thumb_png)?;
    s.mark_saved();
    Ok(Scene::build(&s, &cur))
}

/// Öffnet ein Projekt (lädt den aktuellen Arbeitsstand in den AppState).
#[tauri::command]
fn open_project(data: State<AppData>, name: String) -> Result<Scene, String> {
    let dir = projects_dir();
    let pf = ProjectFile::load_by_name(&dir, &name)?;
    {
        let mut s = data.state.lock().unwrap();
        *s = pf.clone().into_state();
    }
    {
        let mut cur = data.current.lock().unwrap();
        cur.file = Some(pf);
    }
    remember_last_project(&name);
    Ok(scene(&data))
}

/// Lädt eine Version in den Canvas und macht sie zur **aktuellen Version**
/// (ADR 0003, 2026-07-08): Die Geometrie des Snapshots wird der Canvas, und
/// `current_version` zeigt darauf. `projekt.luxi` wird **nicht** umgeschrieben —
/// die Version bleibt unverändert Historie. Wird eine *ältere* Version geladen
/// und danach mit Strg+S bearbeitet, verzweigt der Core beim Speichern
/// automatisch in eine neue Version (kein Überschreiben der alten).
#[tauri::command]
fn open_version(data: State<AppData>, name: String, version_id: String) -> Result<Scene, String> {
    let dir = projects_dir();
    let snap = ProjectFile::load_version(&dir, &name, &version_id)?;
    // Projektdatei laden, nur den Zeiger auf die aktuelle Version umsetzen.
    let mut current = ProjectFile::load_by_name(&dir, &name)?;
    if !current.versions.iter().any(|v| v.id == version_id) {
        return Err("Version nicht in der Historie gefunden.".into());
    }
    current.current_version = version_id.clone();
    // Geometrie in projekt.luxi an die geladene Version angleichen (damit ein
    // späterer Reload denselben Stand sieht), aber ohne modified_at zu berühren
    // und ohne die Version selbst zu ändern.
    current.bed_w_mm = snap.bed_w_mm;
    current.bed_h_mm = snap.bed_h_mm;
    current.layers = snap.layers.clone();
    current.shapes = snap.shapes.clone();
    current.save_to_dir(&dir)?;
    {
        let mut s = data.state.lock().unwrap();
        *s = snap.into_state();
    }
    {
        let mut cur = data.current.lock().unwrap();
        cur.file = Some(current);
    }
    remember_last_project(&name);
    Ok(scene(&data))
}

/// Löscht eine einzelne Version (ADR 0003). Die letzte verbliebene Version ist
/// geschützt (Core lehnt ab). War es die aktuelle Version, wird die vorherige
/// zur aktuellen und ihr Stand in den Canvas geladen.
#[tauri::command]
fn delete_version(data: State<AppData>, name: String, version_id: String) -> Result<Scene, String> {
    let dir = projects_dir();
    let mut cur = data.current.lock().unwrap();
    // Auf dem offenen Projekt arbeiten, falls es dasselbe ist; sonst frisch laden.
    let mut pf = match cur.file.take() {
        Some(f) if f.name == name => f,
        other => {
            // anderes/kein offenes Projekt: zurücklegen und frisch laden.
            cur.file = other;
            ProjectFile::load_by_name(&dir, &name)?
        }
    };
    let promoted = pf.delete_version(&dir, &version_id)?;
    // Wurde die aktuelle Version gelöscht, den beförderten Stand in den Canvas.
    if let Some(snap) = promoted {
        let mut s = data.state.lock().unwrap();
        *s = snap.into_state();
    }
    cur.file = Some(pf);
    drop(cur);
    Ok(scene(&data))
}

/// Liste aller Projekte (linke Seite im Browser).
#[tauri::command]
fn project_list() -> Vec<ProjectInfo> {
    list_projects(&projects_dir())
}

/// Volle Details eines Projekts (rechte Seite im Browser).
#[tauri::command]
fn project_detail(name: String) -> Result<ProjectDetail, String> {
    let pf = ProjectFile::load_by_name(&projects_dir(), &name)?;
    Ok(ProjectDetail {
        name: pf.name,
        description: pf.description,
        tags: pf.tags,
        created_at: pf.created_at,
        modified_at: pf.modified_at,
        current_version: pf.current_version,
        versions: pf.versions,
        asset_refs: pf.asset_refs,
    })
}

/// Ein Asset eines Projekts für die Anzeige im Browser (ADR 0004).
#[derive(Serialize)]
struct ProjectAsset {
    id: String,
    original_name: String,
    width: u32,
    height: u32,
    /// Kleine Vorschau als PNG-Data-URL (rohes Graustufen-Asset).
    thumb: Option<String>,
}

/// Liefert die Assets eines Projekts (aus `asset_refs`) mit Metadaten und einer
/// Vorschau-Data-URL — für den Assets-Bereich im Projekt-Browser.
#[tauri::command]
fn project_assets(name: String) -> Result<Vec<ProjectAsset>, String> {
    let dir = assets_dir();
    let pf = ProjectFile::load_by_name(&projects_dir(), &name)?;
    let mut out = Vec::new();
    for id in &pf.asset_refs {
        let meta = match asset_meta(&dir, id) {
            Ok(m) => m,
            Err(_) => continue, // fehlendes Asset überspringen, nicht scheitern
        };
        // Rohe Graustufen-Vorschau (neutrale Parameter).
        let thumb = rendered_png(&dir, id, &ImageParams::default(), false)
            .ok()
            .map(|png| format!("data:image/png;base64,{}", base64_encode(&png)));
        out.push(ProjectAsset {
            id: meta.id,
            original_name: meta.original_name,
            width: meta.width,
            height: meta.height,
            thumb,
        });
    }
    Ok(out)
}

/// Liefert das Thumbnail des Projekts (= Thumbnail der **aktuellen Version**) als
/// Data-URL, oder `None`, wenn keins existiert. Kein separates `thumbnail.png`
/// mehr — die große Vorschau zeigt immer die aktuelle Version (ADR 0003).
#[tauri::command]
fn project_thumb(name: String) -> Option<String> {
    let dir = projects_dir();
    let pf = ProjectFile::load_by_name(&dir, &name).ok()?;
    let p = luxifer_core::version_thumb_path(&dir, &name, &pf.current_version)?;
    read_png_data_url(&p)
}

/// Liefert das Thumbnail einer bestimmten Version als Data-URL (oder `None`).
#[tauri::command]
fn version_thumb(name: String, version_id: String) -> Option<String> {
    let p = luxifer_core::version_thumb_path(&projects_dir(), &name, &version_id)?;
    read_png_data_url(&p)
}

/// Liest eine PNG-Datei und kodiert sie als `data:image/png;base64,…`-URL.
fn read_png_data_url(path: &std::path::Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(format!("data:image/png;base64,{}", base64_encode(&bytes)))
}

/// Minimale Base64-Kodierung (Standard-Alphabet, mit Padding), ohne Fremd-Crate.
fn base64_encode(data: &[u8]) -> String {
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

/// Löscht ein Projekt samt Versionen. War es das offene Projekt, wird der
/// Projektkontext zurückgesetzt (der Arbeitsstand bleibt zum Weiterarbeiten).
#[tauri::command]
fn project_delete(data: State<AppData>, name: String) -> Result<(), String> {
    delete_project(&projects_dir(), &name)?;
    let mut cur = data.current.lock().unwrap();
    if cur.file.as_ref().is_some_and(|f| f.name == name) {
        cur.file = None;
    }
    Ok(())
}

/// Benennt ein Projekt um (Identität/`id` bleibt). Aktualisiert den offenen
/// Projektkontext, falls es das offene Projekt war.
#[tauri::command]
fn project_rename(data: State<AppData>, old_name: String, new_name: String) -> Result<(), String> {
    let dir = projects_dir();
    rename_project(&dir, &old_name, &new_name)?;
    let mut cur = data.current.lock().unwrap();
    if let Some(f) = cur.file.as_mut() {
        if f.name == old_name {
            f.name = new_name.clone();
            remember_last_project(&new_name);
        }
    }
    Ok(())
}

/// Exportiert die Projektdatei nach `ziel` (einfacher Datei-Export der
/// `projekt.luxi`). Ordner-/ZIP-Export kann später folgen.
#[tauri::command]
fn project_export(name: String, ziel: String) -> Result<(), String> {
    let src = projects_dir().join(&name).join("projekt.luxi");
    std::fs::copy(&src, &ziel).map_err(|e| e.to_string())?;
    Ok(())
}

/// Merkt sich das zuletzt geöffnete/gespeicherte Projekt in den GUI-Settings
/// (für den Start-Toast). Fehler werden geschluckt — rein kosmetisch.
fn remember_last_project(name: &str) {
    let mut settings = UiSettings::load();
    if settings.last_project != name {
        settings.last_project = name.to_string();
        let _ = settings.save();
    }
}

#[tauri::command]
fn undo(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.undo();
    scene_with(&s, &data)
}

#[tauri::command]
fn redo(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.redo();
    scene_with(&s, &data)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppData {
                state: Mutex::new(AppState::new()),
                current: Mutex::new(CurrentProject::default()),
                lasers: Mutex::new(LaserRegistry::load()),
                active: Mutex::new(ActiveDriver::default()),
            });
            // Fenster-/Taskleisten-Icon zur Laufzeit setzen (greift auch im
            // Dev-Modus, wo das gebündelte Icon sonst nicht verwendet wird).
            if let (Some(win), Some(icon)) =
                (app.get_webview_window("main"), app.default_window_icon())
            {
                let _ = win.set_icon(icon.clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_scene,
            swatch_colors,
            add_rect,
            add_ellipse,
            add_line,
            add_polyline,
            shape_catalog,
            add_polygon,
            activate_color,
            select_at,
            select_rect,
            group_op,
            ungroup_op,
            move_selected,
            scale_selected,
            align,
            distribute,
            mirror,
            boolean_op,
            trace_image,
            list_fonts,
            import_vector_file,
            add_text,
            update_text,
            text_preview,
            pattern_fill_op,
            add_spline,
            upload_font,
            offset_op,
            fillet_op,
            nest_op,
            set_layer_params,
            toggle_layer,
            move_layer,
            job_preview,
            laser_list,
            laser_save,
            laser_delete,
            laser_set_active,
            laser_actions,
            laser_run_action,
            laser_export,
            laser_jog,
            laser_home,
            laser_position,
            laser_ping,
            clear_selection,
            delete_selected,
            get_ui_settings,
            save_ui_settings,
            reset_ui_tab,
            new_project,
            save_project,
            save_version,
            import_image_file,
            image_render,
            set_image_params,
            open_project,
            open_version,
            delete_version,
            project_list,
            project_detail,
            project_assets,
            project_thumb,
            version_thumb,
            project_delete,
            project_rename,
            project_export,
            undo,
            redo,
        ])
        .run(tauri::generate_context!())
        .expect("Fehler beim Starten der LuxiFer-App");
}
