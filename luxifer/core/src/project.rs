//! Projektdatei: Speichern/Laden als JSON.
//!
//! Angelehnt an ThorBurns `core/project.rs` (docs/referenz/01-thorburn-analyse.md
//! §3): ein Ordner pro Projekt, darin `projekt.luxi` (JSON) mit Layer- und
//! Shape-Arrays. Bilder folgen später (mit dem Raster-/Job-Teil).
//!
//! Da `Layer`, `Shape` und `Geo` bereits `Serialize`/`Deserialize` sind, ist das
//! Format eine schlanke, versionierte Hülle um den `AppState`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::datetime::{gen_id, now_iso8601};
use crate::model::{Layer, Shape};
use crate::state::AppState;

/// Dateiname der Projektdatei innerhalb des Projektordners.
pub const PROJECT_FILE: &str = "projekt.luxi";

/// Unterordner für festgehaltene Versionen (Snapshot + Thumbnail).
pub const VERSIONS_DIR: &str = "versions";

/// Aktuelle Formatversion.
pub const FORMAT_VERSION: u32 = 1;

/// Kurzinfo einer Version (ADR 0003 §1, überarbeitet 2026-07-08). Thumbnail liegt
/// als Datei `versions/<id>.png` daneben — nicht im JSON, damit es schlank bleibt.
///
/// Modell: Ein Projekt ist eine geordnete Liste von Versionen (V1, V2, …); es gibt
/// keinen separaten Arbeitsstand. Die **aktuelle Version *ist* der Canvas**
/// (`ProjectFile::current_version`). `label` ist die anzeigbare Nummer, `id` die
/// stabile interne Kennung.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionInfo {
    pub id: String,
    /// Anzeigbare Nummer, z. B. „V3". Vergeben in Reihenfolge des Anlegens.
    #[serde(default)]
    pub label: String,
    pub created_at: String,
    #[serde(default)]
    pub note: String,
}

/// Serialisierbare Projektdatei (ADR 0003).
///
/// Neue Felder tragen `#[serde(default)]`, damit ältere Dateien ohne Migration
/// laden (Format-Invariante: vorwärts-tolerant).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    /// Stabile Identität (unveränderlich über Umbenennen). Siehe [`gen_id`].
    #[serde(default = "gen_id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Erstellungszeit (ISO-8601 UTC), einmalig gesetzt.
    #[serde(default)]
    pub created_at: String,
    /// Letzte Änderung (ISO-8601 UTC), bei jedem Speichern aktualisiert.
    #[serde(default)]
    pub modified_at: String,
    /// Verweise auf Assets in der zentralen Bibliothek (nur IDs, nie Kopien).
    /// Vorerst leer — der Store kommt mit dem Import (eigene ADR).
    #[serde(default)]
    pub asset_refs: Vec<String>,
    /// Geordnete Versionsliste (V1, V2, …). Ein Projekt hat immer mindestens V1.
    #[serde(default)]
    pub versions: Vec<VersionInfo>,
    /// ID der aktuellen Version — das, was im Canvas bearbeitet wird. Die
    /// Geometrie unten (`layers`/`shapes`/`bed`) ist die dieser Version.
    #[serde(default)]
    pub current_version: String,
    pub bed_w_mm: f64,
    pub bed_h_mm: f64,
    pub layers: Vec<Layer>,
    pub shapes: Vec<Shape>,
}

impl ProjectFile {
    /// Baut eine **neue** Projektdatei aus dem aktuellen Zustand (frische ID +
    /// Zeitstempel) und legt automatisch **V1** als aktuelle Version an
    /// (ADR 0003: „Neues Projekt → bekommt automatisch V1"). Für das erste
    /// Speichern eines Projekts.
    pub fn from_state(state: &AppState, name: impl Into<String>, tags: Vec<String>) -> Self {
        let now = now_iso8601();
        let v1 = VersionInfo {
            id: gen_id(),
            label: "V1".into(),
            created_at: now.clone(),
            note: String::new(),
        };
        let current = v1.id.clone();
        let mut pf = Self {
            version: FORMAT_VERSION,
            id: gen_id(),
            name: name.into(),
            description: String::new(),
            tags,
            created_at: now.clone(),
            modified_at: now,
            asset_refs: Vec::new(),
            versions: vec![v1],
            current_version: current,
            bed_w_mm: state.bed_w_mm,
            bed_h_mm: state.bed_h_mm,
            layers: state.layers.clone(),
            shapes: state.shapes.clone(),
        };
        pf.sync_asset_refs();
        pf
    }

    /// Übernimmt den aktuellen Arbeitsstand (Geometrie + Bett) in eine bereits
    /// existierende Projektdatei und aktualisiert `modified_at`. Identität,
    /// Metadaten und Versionsliste bleiben erhalten. Aktualisiert **die aktuelle
    /// Version in-place** — das ist normales Speichern (Strg+S).
    pub fn update_from_state(&mut self, state: &AppState) {
        self.bed_w_mm = state.bed_w_mm;
        self.bed_h_mm = state.bed_h_mm;
        self.layers = state.layers.clone();
        self.shapes = state.shapes.clone();
        self.sync_asset_refs();
        self.modified_at = now_iso8601();
    }

    /// Leitet `asset_refs` aus den aktuellen Shapes ab (ADR 0004 §1): sammelt die
    /// Asset-IDs aller `Geo::Image`-Shapes (dedupliziert, in Reihenfolge des
    /// ersten Auftretens). So kennt das Projekt seine Assets explizit — Grundlage
    /// für Anzeige, Aufräumen verwaister Assets und späteren Charon-Sync.
    pub fn sync_asset_refs(&mut self) {
        let mut refs: Vec<String> = Vec::new();
        for s in &self.shapes {
            if let crate::geometry::Geo::Image { asset, .. } = &s.geo {
                if !refs.contains(asset) {
                    refs.push(asset.clone());
                }
            }
        }
        self.asset_refs = refs;
    }

    /// Nächstes freies Versions-Label („V1", „V2", …). Zählt anhand der bereits
    /// vergebenen numerischen Labels hoch, damit auch nach Löschungen keine Nummer
    /// doppelt vergeben wird.
    fn next_label(&self) -> String {
        let max = self
            .versions
            .iter()
            .filter_map(|v| v.label.strip_prefix('V'))
            .filter_map(|n| n.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        format!("V{}", max + 1)
    }

    /// `VersionInfo` der aktuellen Version (falls vorhanden).
    pub fn current(&self) -> Option<&VersionInfo> {
        self.versions.iter().find(|v| v.id == self.current_version)
    }

    /// Erzeugt einen frischen `AppState` aus der Projektdatei (leerer Undo-Verlauf,
    /// `dirty = false`).
    pub fn into_state(self) -> AppState {
        let mut state = AppState::new();
        state.active_layer = self.layers.len().saturating_sub(1);
        state.layers = self.layers;
        state.shapes = self.shapes;
        state.bed_w_mm = self.bed_w_mm;
        state.bed_h_mm = self.bed_h_mm;
        state.dirty = false;
        state
    }

    /// JSON-Text (hübsch formatiert).
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    /// Aus JSON-Text.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    /// Schreibt die Projektdatei nach `<dir>/<name>/projekt.luxi`.
    pub fn save_to_dir(&self, projects_dir: &Path) -> Result<PathBuf, String> {
        let proj_dir = projects_dir.join(&self.name);
        std::fs::create_dir_all(&proj_dir).map_err(|e| e.to_string())?;
        let path = proj_dir.join(PROJECT_FILE);
        std::fs::write(&path, self.to_json()?).map_err(|e| e.to_string())?;
        Ok(path)
    }

    /// Lädt eine Projektdatei aus einem Pfad und **migriert** bei Bedarf ins neue
    /// Versions-Modell (ADR 0003, 2026-07-08): Dateien ohne `versions`/
    /// `current_version` (altes Format: Geometrie in `projekt.luxi`, Historie ohne
    /// Zeiger) bekommen aus ihrem vorhandenen Stand automatisch eine aktuelle
    /// Version. Die Migration wirkt nur im Speicher; geschrieben wird sie erst beim
    /// nächsten regulären Speichern.
    pub fn load(path: &Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut pf = Self::from_json(&json)?;
        pf.migrate();
        Ok(pf)
    }

    /// Bringt eine geladene Datei ins neue Modell, falls sie aus der Zeit davor
    /// stammt. Idempotent: bei bereits gültigem `current_version` passiert nichts.
    fn migrate(&mut self) {
        // Labels für Alt-Versionen nachtragen (V1, V2, … in Listenreihenfolge).
        for (i, v) in self.versions.iter_mut().enumerate() {
            if v.label.is_empty() {
                v.label = format!("V{}", i + 1);
            }
        }
        // Zeigt current_version bereits gültig? Dann fertig.
        if self.versions.iter().any(|v| v.id == self.current_version) {
            return;
        }
        // Kein gültiger Zeiger: Der Stand in projekt.luxi wird zur aktuellen
        // Version. Ist die Liste leer, entsteht V1; sonst zeigt current auf die
        // letzte vorhandene Version (der Stand entspricht dem jüngsten Speichern).
        if let Some(last) = self.versions.last() {
            self.current_version = last.id.clone();
        } else {
            let v1 = VersionInfo {
                id: gen_id(),
                label: "V1".into(),
                created_at: self.created_at.clone(),
                note: String::new(),
            };
            self.current_version = v1.id.clone();
            self.versions.push(v1);
        }
    }

    /// Lädt ein Projekt über seinen Ordnernamen aus `projects_dir`.
    pub fn load_by_name(projects_dir: &Path, name: &str) -> Result<Self, String> {
        Self::load(&projects_dir.join(name).join(PROJECT_FILE))
    }

    /// Schreibt den Geometrie-Snapshot **einer** Version auf die Platte
    /// (`versions/<id>.luxi` + optional `versions/<id>.png`). Interner Helfer;
    /// der Snapshot enthält nur die Geometrie dieser Version, keine Historie.
    fn write_version_snapshot(
        &self,
        projects_dir: &Path,
        version_id: &str,
        thumb_png: &[u8],
    ) -> Result<(), String> {
        let vdir = projects_dir.join(&self.name).join(VERSIONS_DIR);
        std::fs::create_dir_all(&vdir).map_err(|e| e.to_string())?;
        let snap = vdir.join(format!("{version_id}.luxi"));
        std::fs::write(&snap, self.to_json()?).map_err(|e| e.to_string())?;
        if !thumb_png.is_empty() {
            let png = vdir.join(format!("{version_id}.png"));
            std::fs::write(&png, thumb_png).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Ist die aktuelle Version die **letzte** der Liste? Dann updatet Strg+S
    /// in-place; sonst (eine ältere Version wurde geladen) verzweigt der erste
    /// Strg+S in eine neue Version.
    fn current_is_last(&self) -> bool {
        self.versions
            .last()
            .map(|v| v.id == self.current_version)
            .unwrap_or(true)
    }

    /// **Strg+S** — normales Speichern. Der Core entscheidet selbst nach dem
    /// Modell (ADR 0003):
    /// - aktuelle Version ist die **letzte** → **in-place** aktualisieren;
    /// - eine **ältere** Version wurde geladen → einmalig in eine **neue**
    ///   Version **verzweigen** (danach ist sie die letzte, weitere Strg+S sind
    ///   wieder in-place).
    ///
    /// Schreibt `projekt.luxi` und Snapshot + Thumbnail der betroffenen Version
    /// (kein Drift: genau ein Thumbnail pro Version). `thumb_png` sind fertige
    /// PNG-Bytes aus dem Frontend. Gibt die aktuelle [`VersionInfo`] zurück
    /// (bei Verzweigung die neu angelegte).
    pub fn save_current(
        &mut self,
        projects_dir: &Path,
        thumb_png: &[u8],
    ) -> Result<VersionInfo, String> {
        if !self.current_is_last() {
            // Ältere Version geladen → verzweigen statt überschreiben.
            return self.branch_from_current(projects_dir, thumb_png);
        }
        self.modified_at = now_iso8601();
        let cur = self.current_version.clone();
        self.write_version_snapshot(projects_dir, &cur, thumb_png)?;
        self.save_to_dir(projects_dir)?;
        self.current()
            .cloned()
            .ok_or_else(|| "Aktuelle Version fehlt.".into())
    }

    /// **Shift+Strg+S** — nächste Version anlegen: friert den jetzigen Stand als
    /// neue Version (V2, V3, …) ein und macht sie zur aktuellen. Der Canvas
    /// arbeitet danach auf ihr weiter. `note` ist eine optionale Kurznotiz.
    pub fn add_version(
        &mut self,
        projects_dir: &Path,
        note: impl Into<String>,
        thumb_png: &[u8],
    ) -> Result<VersionInfo, String> {
        let info = VersionInfo {
            id: gen_id(),
            label: self.next_label(),
            created_at: now_iso8601(),
            note: note.into(),
        };
        self.versions.push(info.clone());
        self.current_version = info.id.clone();
        self.modified_at = now_iso8601();
        self.write_version_snapshot(projects_dir, &info.id, thumb_png)?;
        self.save_to_dir(projects_dir)?;
        Ok(info)
    }

    /// **Alte Version laden → ändern → Strg+S** — Verzweigen: legt eine **neue**
    /// Version aus dem jetzigen Stand an (statt die geladene alte zu
    /// überschreiben) und macht sie zur aktuellen. Verhält sich wie
    /// [`add_version`], ist aber semantisch das erste Speichern nach dem Laden
    /// einer nicht-aktuellen Version. Danach greift wieder [`save_current`].
    pub fn branch_from_current(
        &mut self,
        projects_dir: &Path,
        thumb_png: &[u8],
    ) -> Result<VersionInfo, String> {
        self.add_version(projects_dir, String::new(), thumb_png)
    }

    /// Löscht eine Version (`versions/<id>.luxi` + `.png`) und entfernt sie aus
    /// der Liste. **V1 / die letzte verbliebene Version ist nicht löschbar.**
    /// War es die aktuelle Version, wird die vorherige (in Listenreihenfolge)
    /// zur neuen aktuellen; ihr Snapshot wird in `projekt.luxi` geladen und
    /// zurückgegeben, damit der Aufrufer den Canvas darauf setzen kann.
    pub fn delete_version(
        &mut self,
        projects_dir: &Path,
        version_id: &str,
    ) -> Result<Option<Self>, String> {
        if self.versions.len() <= 1 {
            return Err("Die letzte Version kann nicht gelöscht werden.".into());
        }
        let idx = self
            .versions
            .iter()
            .position(|v| v.id == version_id)
            .ok_or("Version nicht gefunden.")?;

        let was_current = self.current_version == version_id;
        self.versions.remove(idx);
        self.modified_at = now_iso8601();

        // Snapshot-Dateien der gelöschten Version entfernen.
        let vdir = projects_dir.join(&self.name).join(VERSIONS_DIR);
        let _ = std::fs::remove_file(vdir.join(format!("{version_id}.luxi")));
        let _ = std::fs::remove_file(vdir.join(format!("{version_id}.png")));

        let mut promoted = None;
        if was_current {
            // Vorherige Version zur aktuellen machen (idx-1, sonst die erste).
            let new_idx = idx.saturating_sub(1).min(self.versions.len() - 1);
            let new_id = self.versions[new_idx].id.clone();
            self.current_version = new_id.clone();
            // Geometrie der neuen aktuellen Version in projekt.luxi übernehmen.
            let snap = Self::load_version(projects_dir, &self.name, &new_id)?;
            self.bed_w_mm = snap.bed_w_mm;
            self.bed_h_mm = snap.bed_h_mm;
            self.layers = snap.layers.clone();
            self.shapes = snap.shapes.clone();
            promoted = Some(snap);
        }
        self.save_to_dir(projects_dir)?;
        Ok(promoted)
    }

    /// Lädt den Geometrie-Snapshot einer Version.
    pub fn load_version(projects_dir: &Path, name: &str, version_id: &str) -> Result<Self, String> {
        let path = projects_dir
            .join(name)
            .join(VERSIONS_DIR)
            .join(format!("{version_id}.luxi"));
        Self::load(&path)
    }
}

/// Pfad zum Thumbnail einer Version (`versions/<id>.png`) oder `None`, wenn es
/// keins gibt. Für die Anzeige der Versionsliste im Frontend.
pub fn version_thumb_path(projects_dir: &Path, name: &str, version_id: &str) -> Option<PathBuf> {
    let p = projects_dir
        .join(name)
        .join(VERSIONS_DIR)
        .join(format!("{version_id}.png"));
    p.exists().then_some(p)
}

/// Benennt einen Projektordner um. Die Projekt-`id` bleibt unberührt (Identität
/// hängt an der ID, nicht am Namen — ADR 0003 Invariante 1). Aktualisiert das
/// `name`-Feld in der Projektdatei mit.
pub fn rename_project(projects_dir: &Path, old_name: &str, new_name: &str) -> Result<(), String> {
    if new_name.trim().is_empty() {
        return Err("Neuer Name darf nicht leer sein.".into());
    }
    let old_dir = projects_dir.join(old_name);
    let new_dir = projects_dir.join(new_name);
    if new_dir.exists() {
        return Err(format!("Projekt „{new_name}“ existiert bereits."));
    }
    std::fs::rename(&old_dir, &new_dir).map_err(|e| e.to_string())?;
    // name-Feld in der Datei nachziehen.
    let mut pf = ProjectFile::load_by_name(projects_dir, new_name)?;
    pf.name = new_name.to_string();
    pf.modified_at = now_iso8601();
    pf.save_to_dir(projects_dir)?;
    Ok(())
}

/// Löscht einen Projektordner samt Versionen.
pub fn delete_project(projects_dir: &Path, name: &str) -> Result<(), String> {
    let dir = projects_dir.join(name);
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// Basis-Datenverzeichnis. Reihenfolge: `LUXIFER_DATA_DIR` → `$XDG_DATA_HOME/luxifer`
/// → `$HOME/.local/share/luxifer` → `.` (Notnagel). Plattformneutral.
pub fn data_root() -> PathBuf {
    if let Ok(dir) = std::env::var("LUXIFER_DATA_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("luxifer");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("luxifer");
        }
    }
    PathBuf::from(".")
}

/// Projektordner (`<data_root>/Projekte`).
pub fn projects_dir() -> PathBuf {
    data_root().join("Projekte")
}

/// Kurzinfo eines Projekts für die Listenansicht (links im Browser).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectInfo {
    pub name: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub modified_at: String,
}

/// Listet alle Projekte unter `projects_dir()`. Sortiert nach zuletzt geändert
/// (neueste zuerst), damit das aktivste Projekt oben steht.
pub fn list_projects(projects_dir: &Path) -> Vec<ProjectInfo> {
    let Ok(entries) = std::fs::read_dir(projects_dir) else {
        return vec![];
    };
    let mut infos: Vec<ProjectInfo> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let file = e.path().join(PROJECT_FILE);
            if !file.exists() {
                return None;
            }
            let name = e.file_name().into_string().ok()?;
            let pf = ProjectFile::load(&file).ok()?;
            Some(ProjectInfo {
                name,
                tags: pf.tags,
                description: pf.description,
                modified_at: pf.modified_at,
            })
        })
        .collect();
    // Neueste zuerst; bei gleichem/leerem Datum alphabetisch als Fallback.
    infos.sort_by(|a, b| {
        b.modified_at
            .cmp(&a.modified_at)
            .then_with(|| a.name.cmp(&b.name))
    });
    infos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datetime::format_iso8601;
    use crate::geometry::Geo;

    fn state_with_two_layers() -> AppState {
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        s.selected.clear();
        s.activate_color([0x3B, 0x82, 0xF6]); // pending blau
        s.add_shape(Geo::Ellipse {
            cx: 50.0,
            cy: 50.0,
            rx: 20.0,
            ry: 10.0,
        });
        s
    }

    #[test]
    fn roundtrip_json_erhaelt_layer_und_shapes() {
        let s = state_with_two_layers();
        let pf = ProjectFile::from_state(&s, "Test", vec!["deko".into()]);
        let json = pf.to_json().unwrap();
        let back = ProjectFile::from_json(&json).unwrap();
        assert_eq!(pf, back);
        assert_eq!(back.layers.len(), 2);
        assert_eq!(back.shapes.len(), 2);
        assert_eq!(back.tags, vec!["deko".to_string()]);
    }

    #[test]
    fn into_state_setzt_aktiven_layer_auf_letzten() {
        let s = state_with_two_layers();
        let pf = ProjectFile::from_state(&s, "Test", vec![]);
        let restored = pf.into_state();
        assert_eq!(restored.layers.len(), 2);
        assert_eq!(restored.active_layer, 1);
        assert!(!restored.can_undo(), "frischer Undo-Verlauf");
    }

    #[test]
    fn save_und_load_ueber_tempdir() {
        let dir = std::env::temp_dir().join(format!("luxifer_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = state_with_two_layers();
        let pf = ProjectFile::from_state(&s, "MeinProjekt", vec!["a".into()]);
        let path = pf.save_to_dir(&dir).unwrap();
        assert!(path.exists());

        let loaded = ProjectFile::load(&path).unwrap();
        assert_eq!(loaded.name, "MeinProjekt");
        assert_eq!(loaded.shapes.len(), 2);

        let infos = list_projects(&dir);
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].name, "MeinProjekt");
        assert_eq!(infos[0].tags, vec!["a".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn from_state_hat_id_und_automatisch_v1() {
        let s = state_with_two_layers();
        let pf = ProjectFile::from_state(&s, "T", vec![]);
        assert!(pf.id.starts_with("lx-"));
        assert!(pf.created_at.ends_with('Z'));
        assert_eq!(pf.created_at, pf.modified_at);
        // Neues Projekt bekommt automatisch V1, das auch die aktuelle ist.
        assert_eq!(pf.versions.len(), 1);
        assert_eq!(pf.versions[0].label, "V1");
        assert_eq!(pf.current_version, pf.versions[0].id);
        assert_eq!(pf.current().map(|v| v.label.as_str()), Some("V1"));
        assert!(pf.asset_refs.is_empty());
    }

    #[test]
    fn gen_id_ist_eindeutig() {
        let a = gen_id();
        let b = gen_id();
        assert_ne!(a, b);
    }

    #[test]
    fn iso8601_formatiert_bekannten_zeitpunkt() {
        // 2021-01-01T00:00:00Z = 1609459200 Sekunden seit Epoche.
        assert_eq!(format_iso8601(1_609_459_200), "2021-01-01T00:00:00Z");
        // Epoche selbst.
        assert_eq!(format_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn alte_json_ohne_neue_felder_laedt() {
        // Minimal-JSON wie aus der Zeit vor ADR 0003 (ohne id/versions/…).
        let json = r#"{
            "version": 1, "name": "Alt", "tags": ["x"],
            "bed_w_mm": 300.0, "bed_h_mm": 200.0,
            "layers": [], "shapes": []
        }"#;
        let pf = ProjectFile::from_json(json).unwrap();
        assert_eq!(pf.name, "Alt");
        assert!(pf.id.starts_with("lx-"), "id per serde-default erzeugt");
        assert!(pf.description.is_empty());
    }

    #[test]
    fn migrate_altes_format_erzeugt_v1() {
        // Alte Datei ohne versions/current_version → Migration legt V1 an und
        // zeigt current_version darauf; die vorhandene Geometrie bleibt.
        let json = r#"{
            "version": 1, "name": "Alt", "tags": [],
            "created_at": "2021-01-01T00:00:00Z",
            "bed_w_mm": 300.0, "bed_h_mm": 200.0,
            "layers": [], "shapes": []
        }"#;
        let dir = std::env::temp_dir().join(format!("luxifer_mig_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Alt")).unwrap();
        std::fs::write(dir.join("Alt").join(PROJECT_FILE), json).unwrap();

        let pf = ProjectFile::load_by_name(&dir, "Alt").unwrap();
        assert_eq!(pf.versions.len(), 1, "V1 durch Migration angelegt");
        assert_eq!(pf.versions[0].label, "V1");
        assert_eq!(pf.current_version, pf.versions[0].id);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strg_s_aktualisiert_aktuelle_version_in_place() {
        // save_current darf KEINE neue Version anlegen, nur die aktuelle updaten.
        let dir = std::env::temp_dir().join(format!("luxifer_scur_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = state_with_two_layers();
        let mut pf = ProjectFile::from_state(&s, "Proj", vec![]);
        pf.save_to_dir(&dir).unwrap();
        let v1_id = pf.current_version.clone();

        pf.save_current(&dir, b"\x89PNG-fake").unwrap();
        assert_eq!(pf.versions.len(), 1, "keine neue Version bei Strg+S");
        assert_eq!(pf.current_version, v1_id, "aktuelle Version unverändert");
        // Snapshot + Thumbnail der aktuellen Version liegen jetzt auf der Platte.
        assert!(version_thumb_path(&dir, "Proj", &v1_id).is_some());
        let snap = ProjectFile::load_version(&dir, "Proj", &v1_id).unwrap();
        assert_eq!(snap.shapes.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shift_strg_s_legt_naechste_version_an_und_wird_aktuell() {
        let dir = std::env::temp_dir().join(format!("luxifer_ver_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = state_with_two_layers();
        let mut pf = ProjectFile::from_state(&s, "Proj", vec![]);
        pf.save_to_dir(&dir).unwrap();
        let v1_id = pf.current_version.clone();

        let info = pf
            .add_version(&dir, "zweiter Stand", b"\x89PNG-fake")
            .unwrap();
        assert_eq!(pf.versions.len(), 2);
        assert_eq!(info.label, "V2");
        assert_eq!(pf.current_version, info.id, "neue Version ist aktuell");
        assert_ne!(pf.current_version, v1_id);
        // Snapshot + Thumbnail der neuen Version liegen auf der Platte.
        assert!(version_thumb_path(&dir, "Proj", &info.id).is_some());
        let snap = ProjectFile::load_version(&dir, "Proj", &info.id).unwrap();
        assert_eq!(snap.shapes.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strg_s_auf_alter_version_verzweigt_dann_in_place() {
        let dir = std::env::temp_dir().join(format!("luxifer_br_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = state_with_two_layers();
        let mut pf = ProjectFile::from_state(&s, "Proj", vec![]); // V1
        pf.save_current(&dir, b"").unwrap();
        let v2 = pf.add_version(&dir, "", b"").unwrap(); // V2 (aktuell, letzte)
        let v1_id = pf.versions[0].id.clone();

        // Ältere Version V1 laden (current_version zeigt jetzt auf V1, nicht letzte).
        pf.current_version = v1_id.clone();
        // Erster Strg+S → verzweigt zu V3.
        let branched = pf.save_current(&dir, b"").unwrap();
        assert_eq!(branched.label, "V3", "verzweigt in neue Version");
        assert_eq!(pf.versions.len(), 3);
        assert_eq!(pf.current_version, branched.id);
        assert_ne!(pf.current_version, v1_id, "V1 bleibt unberührt");

        // Zweiter Strg+S → jetzt in-place (keine V4).
        let again = pf.save_current(&dir, b"").unwrap();
        assert_eq!(again.id, branched.id, "in-place, keine neue Version");
        assert_eq!(pf.versions.len(), 3);
        // V2 unangetastet.
        assert!(pf.versions.iter().any(|v| v.id == v2.id));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn labels_zaehlen_auch_nach_loeschen_korrekt_hoch() {
        let dir = std::env::temp_dir().join(format!("luxifer_lbl_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = state_with_two_layers();
        let mut pf = ProjectFile::from_state(&s, "Proj", vec![]); // V1
        pf.save_to_dir(&dir).unwrap();
        pf.save_current(&dir, b"").unwrap();
        let v2 = pf.add_version(&dir, "", b"").unwrap(); // V2
        let v3 = pf.add_version(&dir, "", b"").unwrap(); // V3
        assert_eq!(v2.label, "V2");
        assert_eq!(v3.label, "V3");

        // V2 löschen, dann neue Version → muss V4 heißen (nicht wieder V3).
        pf.delete_version(&dir, &v2.id).unwrap();
        let v4 = pf.add_version(&dir, "", b"").unwrap();
        assert_eq!(v4.label, "V4", "Labels werden nie doppelt vergeben");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn version_loeschen_letzte_ist_geschuetzt() {
        let dir = std::env::temp_dir().join(format!("luxifer_del1_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = state_with_two_layers();
        let mut pf = ProjectFile::from_state(&s, "Proj", vec![]);
        pf.save_to_dir(&dir).unwrap();
        // Nur V1 vorhanden → Löschen muss scheitern.
        let err = pf.delete_version(&dir, &pf.current_version.clone());
        assert!(err.is_err(), "letzte Version nicht löschbar");
        assert_eq!(pf.versions.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn aktuelle_version_loeschen_befoerdert_vorherige() {
        let dir = std::env::temp_dir().join(format!("luxifer_del2_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = AppState::new();
        s.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        });
        let mut pf = ProjectFile::from_state(&s, "Proj", vec![]); // V1: 1 shape
        pf.save_current(&dir, b"").unwrap();

        // V2 mit zwei Shapes anlegen (aktueller Stand).
        s.add_shape(Geo::Rect {
            x: 1.0,
            y: 1.0,
            w: 2.0,
            h: 2.0,
        });
        pf.update_from_state(&s);
        let v2 = pf.add_version(&dir, "", b"").unwrap();
        assert_eq!(pf.current_version, v2.id);

        // V2 (aktuell) löschen → V1 wird aktuell, Geometrie = V1 (1 Shape).
        let promoted = pf.delete_version(&dir, &v2.id).unwrap();
        assert_eq!(pf.versions.len(), 1);
        assert_eq!(pf.current().unwrap().label, "V1");
        assert_eq!(pf.shapes.len(), 1, "Geometrie auf V1 zurück");
        assert_eq!(promoted.unwrap().shapes.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_erhaelt_id_und_verschiebt_ordner() {
        let dir = std::env::temp_dir().join(format!("luxifer_ren_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = state_with_two_layers();
        let pf = ProjectFile::from_state(&s, "Alt", vec![]);
        pf.save_to_dir(&dir).unwrap();
        let id_vorher = pf.id.clone();

        rename_project(&dir, "Alt", "Neu").unwrap();
        assert!(!dir.join("Alt").exists());
        let geladen = ProjectFile::load_by_name(&dir, "Neu").unwrap();
        assert_eq!(geladen.name, "Neu");
        assert_eq!(geladen.id, id_vorher, "id bleibt stabil (Invariante 1)");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_entfernt_projekt() {
        let dir = std::env::temp_dir().join(format!("luxifer_del_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = state_with_two_layers();
        ProjectFile::from_state(&s, "Weg", vec![])
            .save_to_dir(&dir)
            .unwrap();
        assert_eq!(list_projects(&dir).len(), 1);
        delete_project(&dir, "Weg").unwrap();
        assert_eq!(list_projects(&dir).len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn asset_refs_werden_aus_bild_shapes_abgeleitet() {
        use crate::geometry::{Geo, ImageParams};
        let mut s = AppState::new();
        s.add_image("asset-1".into(), 0.0, 0.0, 10.0, 10.0);
        s.add_image("asset-2".into(), 0.0, 0.0, 10.0, 10.0);
        // Dasselbe Asset ein zweites Mal → darf nicht doppelt in asset_refs.
        s.shapes.push(crate::model::Shape::new(
            0,
            Geo::Image {
                asset: "asset-1".into(),
                x: 5.0,
                y: 5.0,
                w: 4.0,
                h: 4.0,
                params: ImageParams::default(),
            },
        ));
        let pf = ProjectFile::from_state(&s, "P", vec![]);
        assert_eq!(
            pf.asset_refs,
            vec!["asset-1".to_string(), "asset-2".to_string()]
        );
    }

    #[test]
    fn update_from_state_erhaelt_id_und_versionen() {
        let s = state_with_two_layers();
        let mut pf = ProjectFile::from_state(&s, "P", vec![]); // hat bereits V1
        let id = pf.id.clone();
        let cur = pf.current_version.clone();
        // Neuer Arbeitsstand mit nur einem Shape.
        let mut s2 = AppState::new();
        s2.add_shape(Geo::Rect {
            x: 0.0,
            y: 0.0,
            w: 5.0,
            h: 5.0,
        });
        pf.update_from_state(&s2);
        assert_eq!(pf.id, id, "Identitaet bleibt");
        assert_eq!(pf.versions.len(), 1, "Versionsliste unverändert (nur V1)");
        assert_eq!(pf.current_version, cur, "aktuelle Version unverändert");
        assert_eq!(pf.shapes.len(), 1, "Arbeitsstand ersetzt");
    }
}
