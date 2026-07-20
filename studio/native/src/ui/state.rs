//! Kurzlebiger Präsentationszustand der Dialoge (Entwürfe). Reiner UI-Zustand:
//! Er lebt nur, solange ein Dialog offen ist, und trägt keine Wahrheit — die
//! liegt im `AppState`. `App` hält davon lediglich die `Option<…>`-Felder; die
//! Dialoge bearbeiten den Entwurf und der Root übernimmt/verwirft ihn.

use studio_application::LayerParams;

/// Kurzlebiger Entwurf für Breite/Höhe der aktuellen Auswahlbox.
pub struct SelectionSizeState {
    pub width: String,
    pub height: String,
    pub proportional: bool,
    pub source: Option<studio_core::BBox>,
    pub width_dirty: bool,
    pub height_dirty: bool,
}

impl Default for SelectionSizeState {
    fn default() -> Self {
        Self {
            width: String::new(),
            height: String::new(),
            proportional: true,
            source: None,
            width_dirty: false,
            height_dirty: false,
        }
    }
}

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
    pub params: studio_core::ImageParams,
    /// Trace-Schwelle 0..=255 (Pixel darunter sind Motiv).
    pub trace_threshold: u8,
    /// Motiv/Hintergrund beim Trace tauschen (helles Motiv auf dunklem Grund).
    pub trace_invert: bool,
    /// Gecachte Live-Vorschau des aktuellen Parameterentwurfs.
    pub preview: Option<egui::TextureHandle>,
    pub preview_key: Option<u64>,
    pub preview_error: Option<String>,
    /// Reiner Ansichts-Zustand der Vorschau; verändert das Bildobjekt nicht.
    pub preview_zoom: f32,
    pub preview_pan: egui::Vec2,
    pub page: ImageDialogPage,
    /// Normalisierte Schnittkanten im Quellbild: links, oben, rechts, unten.
    pub crop_rect: [f32; 4],
    pub crop_kind: CropKind,
    /// Während der Konstruktion: drei Umfangspunkte. Danach: Mittelpunkt,
    /// rechter und unterer Halbachsenpunkt der achsenparallelen Ellipse.
    pub crop_ellipse: [[f32; 2]; 3],
    pub crop_ellipse_points: u8,
    pub crop_ellipse_error: Option<String>,
    pub crop_drag_handle: Option<usize>,
    pub crop_drag_start: Option<[f32; 2]>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CropKind {
    Rect,
    Ellipse,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ImageDialogPage {
    Settings,
    Trace,
    Crop,
}

impl ImageDialogState {
    pub fn new(index: usize, params: studio_core::ImageParams) -> Self {
        Self {
            index,
            params,
            trace_threshold: 128,
            trace_invert: false,
            preview: None,
            preview_key: None,
            preview_error: None,
            preview_zoom: 1.0,
            preview_pan: egui::Vec2::ZERO,
            page: ImageDialogPage::Settings,
            crop_rect: [0.0, 0.0, 1.0, 1.0],
            crop_kind: CropKind::Rect,
            crop_ellipse: [[0.5, 0.5], [0.85, 0.5], [0.5, 0.85]],
            crop_ellipse_points: 0,
            crop_ellipse_error: None,
            crop_drag_handle: None,
            crop_drag_start: None,
        }
    }
}

/// Entwurf des Text-Dialogs (Eingabe, Layout, gewählte Familie/Schnitt).
pub struct TextDialogState {
    pub text: String,
    pub size_mm: f64,
    pub align: studio_core::text::TextAlign,
    /// Zeilenabstand als Faktor der Em-Größe.
    pub line_spacing: f64,
    /// Zusätzlicher Zeichenabstand in mm.
    pub letter_spacing_mm: f64,
    /// Index in der Familien-Liste, oder None (kein Font gewählt).
    pub family_idx: Option<usize>,
    /// Index des Schnitts innerhalb der gewählten Familie.
    pub face_idx: usize,
    /// Suchfilter über die Familiennamen.
    pub search: String,
    /// Shape-Index des editierten Textblocks, oder None (neuer Text).
    pub edit_index: Option<usize>,
    /// Nutzer will eine Font-Datei importieren; der App-Root öffnet den
    /// Datei-Dialog (der Dialog selbst bleibt reine Zeichnung).
    pub request_font_import: bool,
    /// Live-Vorschau: Konturen (mm) zum aktuellen Entwurf, vom App-Root über
    /// den Core berechnet und gecacht (der Dialog zeichnet nur).
    pub preview: Vec<(Vec<(f64, f64)>, bool)>,
    /// Cache-Schlüssel des Vorschau-Stands (Hash über Entwurf + Fontpfad).
    pub preview_key: Option<u64>,
}

impl Default for TextDialogState {
    fn default() -> Self {
        Self {
            text: "Text".into(),
            size_mm: 20.0,
            align: studio_core::text::TextAlign::Left,
            line_spacing: studio_core::text::DEFAULT_LINE_SPACING,
            letter_spacing_mm: 0.0,
            family_idx: None,
            face_idx: 0,
            search: String::new(),
            edit_index: None,
            request_font_import: false,
            preview: Vec::new(),
            preview_key: None,
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
    pub bool_op: studio_core::BoolOp,
    /// Distanz in mm (Offset).
    pub distance: f64,
    /// Radius in mm (Fillet).
    pub radius: f64,
    /// Muster-Parameter (nur bei `PatternFill`).
    pub fill: studio_core::pattern_fill::FillParams,
}

impl GeoOpDialogState {
    pub fn new(kind: GeoOpKind) -> Self {
        Self {
            kind,
            bool_op: studio_core::BoolOp::Union,
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
    /// Empfangene Hub-Version nach Dirty-Bestätigung übernehmen.
    AcceptInbox(String),
    /// Alle offenen Hub-Versionen nach Dirty-Bestätigung übernehmen.
    AcceptAllInbox(Vec<String>),
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
    /// Lokaler Filter über Asset-Dateiname und automatisch abgeleitete Tags.
    pub asset_search: String,
    /// `true` zeigt die empfangenen Hub-Revisionen statt lokaler Projekte.
    pub show_inbox: bool,
    /// `true` zeigt den projektübergreifenden Asset-Katalog.
    pub show_assets: bool,
    /// Im Browser markiertes Projekt (unabhängig vom offenen Projekt).
    pub selected: Option<String>,
    /// `Some` = Umbenennen-Feld ist sichtbar und hält den Namensentwurf.
    pub rename_draft: Option<String>,
    /// Zweistufiges Löschen des markierten Projekts („Wirklich löschen?").
    pub confirm_delete: bool,
    /// Zweistufiges Löschen einer Version (Versions-ID der ersten Stufe).
    pub confirm_delete_version: Option<String>,
    pub confirm_delete_asset: Option<String>,
    /// Gecachte Detailsicht + Vektor-Miniatur des markierten Projekts.
    /// `cache_key` macht den Cache gegen Umbenennen/Speichern/Editieren stabil.
    pub cached: Option<CachedProjectDetail>,
}

/// Sektion des Einstellungen-Dialogs (Navigation links, wie das Tauri-Modal).
/// Die frühere Sammel-Sektion „Oberfläche" ist nach Thema aufgeteilt:
/// Arbeitsplatz (Identität), Editor (Zeichenverhalten), Darstellung
/// (Theme/Rendering), Fenster & Start (Chrome/Splash) — sonst wächst eine
/// einzelne Sektion unkontrolliert mit jeder neuen Einstellung.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsSection {
    Arbeitsplatz,
    Editor,
    Darstellung,
    FensterUndStart,
    Tastaturkuerzel,
    Hub,
    Ueber,
}

#[derive(Clone, Copy, Debug)]
pub struct ShortcutRecording {
    pub action: studio_core::ShortcutAction,
    pub replace: Option<studio_core::ShortcutTrigger>,
}

#[derive(Clone, Copy, Debug)]
pub struct ShortcutConflict {
    pub action: studio_core::ShortcutAction,
    pub previous_action: studio_core::ShortcutAction,
    pub trigger: studio_core::ShortcutTrigger,
    pub replace: Option<studio_core::ShortcutTrigger>,
}

#[derive(Clone, Debug, Default)]
pub enum HubTestStatus {
    #[default]
    Idle,
    Syncing(studio_application::HubConnection),
    Connected(studio_application::HubConnection),
    Failed(String),
}

/// Entwurf des globalen Einstellungen-Dialogs. Laserprofile werden bewusst in
/// der separaten Laser-Verwaltung bearbeitet.
pub struct SettingsDialogState {
    pub draft: studio_core::UiSettings,
    pub section: SettingsSection,
    pub hub_status: HubTestStatus,
    pub hub_sync_error: Option<String>,
    pub hub_backups: Vec<studio_application::HubWorkplaceBackup>,
    pub backup_restore_confirm: Option<BackupRestoreConfirmation>,
    pub shortcut_recording: Option<ShortcutRecording>,
    pub shortcut_conflict: Option<ShortcutConflict>,
    pub shortcut_error: Option<String>,
    pub confirm_shortcut_defaults: bool,
}

pub struct BackupRestoreConfirmation {
    pub index: usize,
    pub summary: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LaserManagerTab {
    #[default]
    Grunddaten,
    /// Reversal-/Scan-Offset-Korrektur (geschwindigkeitsabhängiger Versatz beim
    /// bidirektionalen Scannen). Nicht zu verwechseln mit `Achskalibrierung`.
    ScanOffset,
    /// Achsen-Schrittweite aus Soll/Ist kalibrieren (ADR 0022 §D).
    Achskalibrierung,
    Controller,
    /// Werkstück-Nullpunkte dieses Lasers (ADR 0020): umbenennen/löschen.
    Nullpunkte,
}

pub struct LaserManagerState {
    pub selected_id: Option<String>,
    pub draft: studio_core::LaserProfile,
    pub is_new: bool,
    pub tab: LaserManagerTab,
    pub machine_settings: Vec<studio_application::MachineSetting>,
    pub machine_dirty: std::collections::BTreeMap<u16, i64>,
    pub machine_confirm_write: bool,
    /// Ergebniskanal eines laufenden Lese- oder Schreibvorgangs. Beide liefern
    /// einen frischen Parametersatz und teilen sich deshalb einen Kanal.
    pub machine_read_rx: Option<
        std::sync::mpsc::Receiver<
            Result<Vec<studio_application::MachineSetting>, studio_application::AppError>,
        >,
    >,
    /// Gesetzt, solange der laufende Vorgang ein Schreiben ist — nur für die
    /// Rückmeldung, damit sie die geschriebenen Register nennt.
    pub machine_write_count: Option<usize>,
    /// Soll/Ist-Eingaben der Achskalibrierung (ADR 0022 §D), je Achse.
    pub axis_cal: std::collections::BTreeMap<studio_core::MachineAxis, AxisCalInput>,
    /// Einmal-Auftrag: die Textpuffer dieser Achse im nächsten Frame leeren.
    /// Nötig, weil die Eingabefelder ihren Text in `ui.data_mut` zwischenlagern
    /// und ein bloßes Nullen im State sonst überschrieben würde.
    pub axis_cal_clear_inputs: Option<studio_core::MachineAxis>,
    /// Achse, deren Schrittlänge gerade geschrieben wird. Das Schreiben blockiert
    /// den UI-Thread mehrere Sekunden — ohne Anzeige wirkt die Anwendung tot.
    pub axis_cal_pending: Option<studio_core::MachineAxis>,
    /// Ergebniskanal der laufenden Kalibrierung vom Geräte-Worker.
    pub axis_cal_rx: Option<
        std::sync::mpsc::Receiver<
            Result<studio_application::AxisCalibration, studio_application::AppError>,
        >,
    >,
}

/// Soll- und Ist-Strecke einer Achse für die Schrittweiten-Kalibrierung.
#[derive(Clone, Copy, Default)]
pub struct AxisCalInput {
    pub target_mm: f64,
    pub measured_mm: f64,
}

/// Kurzlebiger Entwurf eines lokalen, laserbezogenen Materialprofils.
pub struct MaterialManagerState {
    pub draft: studio_core::MaterialProfile,
    pub is_new: bool,
}

pub struct LayerManagerState {
    pub layers: Vec<studio_application::LayerParams>,
    pub material_id: Option<String>,
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

/// Namensdialog beim Speichern eines Werkstück-Nullpunkts (ADR 0020).
/// Umbenennen/Löschen bestehender Einträge übernimmt die Laser-Verwaltung.
#[derive(Clone)]
pub struct SavedOriginDialogState {
    pub name: String,
    /// Beim Auslösen frisch gelesene absolute Maschinenposition.
    pub position: (f64, f64),
}

/// Gecachte Browser-Detailsicht: Metadaten/Versionen aus der Application und
/// die daraus vorbereitete Vektor-Miniatur.
pub struct CachedProjectDetail {
    /// Schlüssel `name:modified_at` (bzw. `name:rev<render_rev>` beim offenen
    /// Projekt), damit Änderungen den Cache automatisch verwerfen.
    pub cache_key: String,
    pub detail: studio_application::ProjectDetail,
    pub preview: ProjectPreview,
}

/// Vorbereitete Vektor-Miniatur eines Projektzustands (Weltkoordinaten in mm;
/// das Panel skaliert sie in seinen Vorschau-Rahmen).
pub struct ProjectPreview {
    /// Bettgröße (Breite, Höhe) in mm.
    pub bed: (f32, f32),
    pub outlines: Vec<PreviewOutline>,
    pub images: Vec<PreviewImage>,
}

pub struct PreviewImage {
    pub asset_id: String,
    pub corners: [(f32, f32); 4],
}

/// Read-only Präsentationszustand des Vergleichsdialogs.
pub struct RevisionComparisonState {
    pub comparison: studio_application::InboxComparison,
    pub local_preview: Option<ProjectPreview>,
    pub remote_preview: ProjectPreview,
}

/// Eine Shape-Kontur der Miniatur in Layer-Farbe.
pub struct PreviewOutline {
    pub points: Vec<(f32, f32)>,
    pub closed: bool,
    pub color: [u8; 3],
}
