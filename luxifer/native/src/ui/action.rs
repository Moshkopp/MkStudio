//! Typisierte UI-Absichten: Ein Panel zeichnet und liefert `UiAction`s zurück,
//! statt den `App`-Zustand direkt zu mutieren. Der Root führt sie über
//! `App::dispatch` aus. So bleibt die UI von der Anwendungslogik entkoppelt
//! (ADR 0011: „UI erzeugt Absicht, App koordiniert").
//!
//! Das Enum wächst schnittweise: Es deckt zunächst nur die Aktionen der bereits
//! migrierten Panels ab. Panels, die noch `&mut App` erhalten, tragen hier noch
//! nichts bei.

use std::path::PathBuf;

use luxifer_application::LayerToggle;
use luxifer_core::{Align, Distribute, JobAction, PolyShape};

use crate::tools::{Tool, ToolAction, View};

/// Eine vom UI ausgelöste Absicht. Rein beschreibend — kein Verhalten.
/// Nicht `Copy`, weil einzelne Varianten Eigentum tragen (z. B. Projektname).
#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    /// Auswahl ausrichten.
    Align(Align),
    /// Auswahl verteilen/Abstände angleichen.
    Distribute(Distribute),
    /// Auswahl gruppieren.
    Group,
    /// Gruppierung der Auswahl lösen.
    Ungroup,
    /// Auswahl mit festem Abstand packen (mm).
    Nest(f64),
    /// Bett mit der Auswahl füllen (Abstand mm).
    NestFill(f64),
    /// Farbe aktivieren (Farbe = Layer).
    PickColor([u8; 3]),
    /// Aktive Polygon-Form wählen (Präsentationszustand).
    SelectShape(PolyShape),
    /// Aktives Werkzeug wählen (Präsentationszustand).
    SelectTool(Tool),
    /// Sofort-Operation auf der Auswahl (Boolean/Fillet/Offset/…).
    ToolAction(ToolAction),
    /// Text-Dialog öffnen.
    OpenTextDialog,
    /// Auswahl horizontal spiegeln.
    MirrorH,
    /// Auswahl vertikal spiegeln.
    MirrorV,
    /// Untersetzer einfügen (`round` = rund statt eckig).
    InsertCoasters(bool),
    /// Einen Layer-Schalter umlegen (Index in Layer-Reihenfolge).
    ToggleLayer(usize, LayerToggle),
    /// Layer-Parameter-Dialog für diesen Index öffnen.
    OpenLayerDialog(usize),
    /// Einen Layer in der Brenn-Reihenfolge verschieben.
    MoveLayer { from: usize, to: usize },
    /// Neues Projekt aus dem aktuellen Namensentwurf anlegen.
    NewProject,
    /// Aktuelles Projekt in-place speichern.
    SaveProject,
    /// Aktuelles Projekt als neue Version speichern.
    SaveProjectVersion,
    /// Projekt mit diesem Namen öffnen.
    OpenProject(String),
    /// Projekt mit diesem Namen löschen.
    DeleteProject(String),
    /// Projekt mit diesem Namen exportieren (Zieldialog im Root).
    ExportProject(String),
    /// Haupt-Ansicht (Reiter) wechseln.
    SelectView(View),
    /// Einen Layer im Laser-Tab vorübergehend für Transformationen freigeben.
    ToggleLaserEditLayer(usize),
    /// Rückgängig.
    Undo,
    /// Wiederholen.
    Redo,
    /// Vektor-Import-Dialog (SVG/DXF) öffnen.
    ImportVector,
    /// Bild-Import-Dialog öffnen.
    ImportImage,
    /// Datei von einem bekannten Pfad importieren (Entwickler-Shortcut).
    ImportPath(PathBuf),
    /// Die aktuelle Fehleranzeige schließen.
    DismissError,
    /// Laser-Profil aktivieren.
    LaserSelect(String),
    /// Laser-Job-Aktion ausführen (Start/Pause/Stop/…).
    LaserRun(JobAction),
    /// Aktuellen Job als Datei exportieren.
    LaserExport,
    /// Laserkopf um (dx, dy) mm bewegen.
    LaserJog(f64, f64),
    /// Laserkopf homen.
    LaserHome,
    /// Laser-Einstellungen öffnen (`edit_active` = bestehendes bearbeiten).
    OpenLaserSettings { edit_active: bool },
}
