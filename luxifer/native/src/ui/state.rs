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

/// Entwurf des Text-Dialogs (Eingabe, Größe, gewählter Font-Index).
pub struct TextDialogState {
    pub text: String,
    pub size_mm: f64,
    /// Index in der Font-Liste, oder None (kein Font gewählt).
    pub font_idx: Option<usize>,
}

impl Default for TextDialogState {
    fn default() -> Self {
        Self {
            text: "Text".into(),
            size_mm: 20.0,
            font_idx: None,
        }
    }
}

/// Eine Projektaktion, die den aktuellen Editorzustand ersetzen würde und
/// deshalb bei ungespeicherten Änderungen erst bestätigt werden muss
/// (Dirty-Guard). Wird ausgeführt, sobald der Nutzer „Verwerfen" wählt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingProjectAction {
    /// Neues Projekt mit diesem Namen anlegen.
    New(String),
    /// Projekt mit diesem Namen öffnen.
    Open(String),
}
