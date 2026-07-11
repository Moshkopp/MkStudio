//! Projektverwaltung (ADR 0003): Speichern/Öffnen, Versionen, Projektliste und
//! -details, Assets, Thumbnails, Umbenennen/Löschen/Export. Nur Versionen, keine
//! separaten Arbeitsstände — die aktuelle Version IST der Canvas.

use luxifer_core::{
    asset_meta, assets_dir, delete_project, list_projects, projects_dir, rename_project,
    rendered_png, AppState, ImageParams, ProjectFile, ProjectInfo, VersionInfo,
};
use serde::Serialize;
use tauri::State;

use crate::shared::{
    base64_encode, read_png_data_url, remember_last_project, scene, AppData, Scene,
};

/// Volle Detailansicht eines Projekts (rechte Seite im Browser).
#[derive(Serialize)]
pub struct ProjectDetail {
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
pub fn new_project(data: State<AppData>) -> Scene {
    {
        let mut s = data.state();
        *s = AppState::new();
    }
    {
        let mut cur = data.current();
        cur.file = None;
    }
    scene(&data)
}

/// Speichert das Projekt. Ist noch keins offen (namenlos), wird mit den
/// gelieferten Metadaten ein neues angelegt; sonst wird der Arbeitsstand des
/// offenen Projekts überschrieben. `thumb_png` sind fertige PNG-Bytes (Frontend).
#[tauri::command]
pub fn save_project(
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
    let mut s = data.state();
    let mut cur = data.current();

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
pub fn save_version(
    data: State<AppData>,
    note: String,
    thumb_png: Vec<u8>,
) -> Result<Scene, String> {
    let dir = projects_dir();
    let mut s = data.state();
    let mut cur = data.current();
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
pub fn open_project(data: State<AppData>, name: String) -> Result<Scene, String> {
    let dir = projects_dir();
    let pf = ProjectFile::load_by_name(&dir, &name)?;
    {
        let mut s = data.state();
        *s = pf.clone().into_state();
    }
    {
        let mut cur = data.current();
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
pub fn open_version(
    data: State<AppData>,
    name: String,
    version_id: String,
) -> Result<Scene, String> {
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
        let mut s = data.state();
        *s = snap.into_state();
    }
    {
        let mut cur = data.current();
        cur.file = Some(current);
    }
    remember_last_project(&name);
    Ok(scene(&data))
}

/// Löscht eine einzelne Version (ADR 0003). Die letzte verbliebene Version ist
/// geschützt (Core lehnt ab). War es die aktuelle Version, wird die vorherige
/// zur aktuellen und ihr Stand in den Canvas geladen.
#[tauri::command]
pub fn delete_version(
    data: State<AppData>,
    name: String,
    version_id: String,
) -> Result<Scene, String> {
    let dir = projects_dir();
    let mut cur = data.current();
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
        let mut s = data.state();
        *s = snap.into_state();
    }
    cur.file = Some(pf);
    drop(cur);
    Ok(scene(&data))
}

/// Liste aller Projekte (linke Seite im Browser).
#[tauri::command]
pub fn project_list() -> Vec<ProjectInfo> {
    list_projects(&projects_dir())
}

/// Volle Details eines Projekts (rechte Seite im Browser).
#[tauri::command]
pub fn project_detail(name: String) -> Result<ProjectDetail, String> {
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
pub struct ProjectAsset {
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
pub fn project_assets(name: String) -> Result<Vec<ProjectAsset>, String> {
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
pub fn project_thumb(name: String) -> Option<String> {
    let dir = projects_dir();
    let pf = ProjectFile::load_by_name(&dir, &name).ok()?;
    let p = luxifer_core::version_thumb_path(&dir, &name, &pf.current_version)?;
    read_png_data_url(&p)
}

/// Liefert das Thumbnail einer bestimmten Version als Data-URL (oder `None`).
#[tauri::command]
pub fn version_thumb(name: String, version_id: String) -> Option<String> {
    let p = luxifer_core::version_thumb_path(&projects_dir(), &name, &version_id)?;
    read_png_data_url(&p)
}

/// Löscht ein Projekt samt Versionen. War es das offene Projekt, wird der
/// Projektkontext zurückgesetzt (der Arbeitsstand bleibt zum Weiterarbeiten).
#[tauri::command]
pub fn project_delete(data: State<AppData>, name: String) -> Result<(), String> {
    delete_project(&projects_dir(), &name)?;
    let mut cur = data.current();
    if cur.file.as_ref().is_some_and(|f| f.name == name) {
        cur.file = None;
    }
    Ok(())
}

/// Benennt ein Projekt um (Identität/`id` bleibt). Aktualisiert den offenen
/// Projektkontext, falls es das offene Projekt war.
#[tauri::command]
pub fn project_rename(
    data: State<AppData>,
    old_name: String,
    new_name: String,
) -> Result<(), String> {
    let dir = projects_dir();
    rename_project(&dir, &old_name, &new_name)?;
    let mut cur = data.current();
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
pub fn project_export(name: String, ziel: String) -> Result<(), String> {
    let src = projects_dir().join(&name).join("projekt.luxi");
    std::fs::copy(&src, &ziel).map_err(|e| e.to_string())?;
    Ok(())
}
