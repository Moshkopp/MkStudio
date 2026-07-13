//! Kurzlebiger Präsentationszustand der Dialoge (Entwürfe). Reiner UI-Zustand:
//! Er lebt nur, solange ein Dialog offen ist, und trägt keine Wahrheit — die
//! liegt im `AppState`. `App` hält davon lediglich die `Option<…>`-Felder; die
//! Dialoge bearbeiten den Entwurf und der Root übernimmt/verwirft ihn.

use luxifer_application::LayerParams;

/// Entwurf des Layer-Parameter-Dialogs (Doppelklick auf eine Ebene).
pub struct LayerDialogState {
    pub index: usize,
    pub params: LayerParams,
}

/// Entwurf des Bildparameter-Dialogs (Doppelklick auf ein Bild-Objekt).
/// Enthält auch die Trace-Regler (Vektorisieren): reine Dialog-Drafts,
/// erst „Vektorisieren" führt über die Session aus.
pub struct ImageDialogState {
    /// Shape-Index des bearbeiteten Bildes.
    pub index: usize,
    pub params: luxifer_core::ImageParams,
    /// Trace-Schwelle 0..=255 (Pixel darunter sind Motiv).
    pub trace_threshold: u8,
    /// Motiv/Hintergrund beim Trace tauschen (helles Motiv auf dunklem Grund).
    pub trace_invert: bool,
}

impl ImageDialogState {
    pub fn new(index: usize, params: luxifer_core::ImageParams) -> Self {
        Self {
            index,
            params,
            trace_threshold: 128,
            trace_invert: false,
        }
    }
}

/// Entwurf des Text-Dialogs (Eingabe, Größe, gewählter Font-Index).
pub struct TextDialogState {
    pub text: String,
    pub size_mm: f64,
    /// Index in der Font-Liste, oder None (kein Font gewählt).
    pub font_idx: Option<usize>,
    /// Shape-Index des editierten Textblocks, oder None (neuer Text).
    pub edit_index: Option<usize>,
}

impl Default for TextDialogState {
    fn default() -> Self {
        Self {
            text: "Text".into(),
            size_mm: 20.0,
            font_idx: None,
            edit_index: None,
        }
    }
}

/// Welche parametrierte Geometrieoperation der Dialog bearbeitet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GeoOpKind {
    Boolean,
    Offset,
    Fillet,
    PatternFill,
}

/// Entwurf des Geometrie-Parameterdialogs (Boolean-Variante, Offset-Distanz,
/// Fillet-Radius, Muster-Füllung). Reiner UI-Zustand; die Ausführung läuft
/// über die Session.
pub struct GeoOpDialogState {
    pub kind: GeoOpKind,
    /// Boolean-Variante (nur bei `Boolean`).
    pub bool_op: luxifer_core::BoolOp,
    /// Distanz in mm (Offset).
    pub distance: f64,
    /// Radius in mm (Fillet).
    pub radius: f64,
    /// Muster-Parameter (nur bei `PatternFill`).
    pub fill: luxifer_core::pattern_fill::FillParams,
}

impl GeoOpDialogState {
    pub fn new(kind: GeoOpKind) -> Self {
        Self {
            kind,
            bool_op: luxifer_core::BoolOp::Union,
            distance: 2.0,
            radius: 2.0,
            fill: Default::default(),
        }
    }
}

/// Eine Projektaktion, die den aktuellen Editorzustand ersetzen würde und
/// deshalb bei ungespeicherten Änderungen erst bestätigt werden muss
/// (Dirty-Guard). Wird ausgeführt, sobald der Nutzer „Verwerfen" wählt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingProjectAction {
    /// Aktuelles Projekt schließen und den Editor leeren.
    Blank,
    /// Neues Projekt aus dem Maskenentwurf anlegen.
    New { name: String, description: String },
    /// Projekt mit diesem Namen öffnen.
    Open(String),
    /// Eine Version des offenen Projekts laden (Versions-ID).
    OpenVersion(String),
    /// Die AKTUELLE Version löschen (Versions-ID) — der Core befördert dann
    /// die vorherige in den Canvas, ersetzt also den Editorzustand.
    DeleteVersion(String),
}

/// Präsentationszustand des Projektbrowsers: Auswahl, Umbenennen-Entwurf,
/// Lösch-Bestätigungen und die gecachte Detail-/Vorschausicht. Reiner
/// UI-Zustand — die Wahrheit liegt im `ProjectService`/Core.
#[derive(Default)]
pub struct ProjectBrowserState {
    /// `true` zeigt die empfangenen Charon-Revisionen statt lokaler Projekte.
    pub show_inbox: bool,
    /// Im Browser markiertes Projekt (unabhängig vom offenen Projekt).
    pub selected: Option<String>,
    /// `Some` = Umbenennen-Feld ist sichtbar und hält den Namensentwurf.
    pub rename_draft: Option<String>,
    /// Zweistufiges Löschen des markierten Projekts („Wirklich löschen?").
    pub confirm_delete: bool,
    /// Zweistufiges Löschen einer Version (Versions-ID der ersten Stufe).
    pub confirm_delete_version: Option<String>,
    /// Gecachte Detailsicht + Vektor-Miniatur des markierten Projekts.
    /// `cache_key` macht den Cache gegen Umbenennen/Speichern/Editieren stabil.
    pub cached: Option<CachedProjectDetail>,
}

/// Sektion des Einstellungen-Dialogs (Navigation links, wie das Tauri-Modal).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsSection {
    Oberflaeche,
    Charon,
    Ueber,
}

#[derive(Clone, Debug, Default)]
pub enum CharonTestStatus {
    #[default]
    Idle,
    Connected(luxifer_application::CharonConnection),
    Failed(String),
}

/// Entwurf des globalen Einstellungen-Dialogs. Laserprofile werden bewusst in
/// der separaten Laser-Verwaltung bearbeitet.
pub struct SettingsDialogState {
    pub draft: luxifer_core::UiSettings,
    pub section: SettingsSection,
    pub charon_status: CharonTestStatus,
    pub charon_sync_error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LaserManagerTab {
    #[default]
    Grunddaten,
    Kalibrierung,
    Controller,
}

pub struct LaserManagerState {
    pub selected_id: Option<String>,
    pub draft: luxifer_core::LaserProfile,
    pub is_new: bool,
    pub tab: LaserManagerTab,
    pub machine_settings: Vec<luxifer_application::RuidaMachineSetting>,
    pub machine_dirty: std::collections::BTreeMap<u16, i64>,
    pub machine_confirm_write: bool,
}

/// Entwurf der „Neues Projekt"-Maske (Strg+S ohne offenes Projekt bzw.
/// „Neues Projekt…" im Projekt-Reiter): Name + Beschreibung. Kurzlebig —
/// Anlegen läuft über den validierenden `ProjectService`.
#[derive(Default)]
pub struct ProjectSaveDialogState {
    pub name: String,
    pub description: String,
    /// Einmal-Flag: das Namensfeld beim ersten Frame fokussieren.
    pub focus_name: bool,
}

/// Gecachte Browser-Detailsicht: Metadaten/Versionen aus der Application und
/// die daraus vorbereitete Vektor-Miniatur.
pub struct CachedProjectDetail {
    /// Schlüssel `name:modified_at` (bzw. `name:rev<render_rev>` beim offenen
    /// Projekt), damit Änderungen den Cache automatisch verwerfen.
    pub cache_key: String,
    pub detail: luxifer_application::ProjectDetail,
    pub preview: ProjectPreview,
}

/// Vorbereitete Vektor-Miniatur eines Projektzustands (Weltkoordinaten in mm;
/// das Panel skaliert sie in seinen Vorschau-Rahmen).
pub struct ProjectPreview {
    /// Bettgröße (Breite, Höhe) in mm.
    pub bed: (f32, f32),
    pub outlines: Vec<PreviewOutline>,
}

/// Eine Shape-Kontur der Miniatur in Layer-Farbe.
pub struct PreviewOutline {
    pub points: Vec<(f32, f32)>,
    pub closed: bool,
    pub color: [u8; 3],
}
