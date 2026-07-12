mod actions;
mod drawing;
mod layers;
mod selection;

pub use layers::LayerParams;

use std::ops::{Deref, DerefMut};

use luxifer_core::AppState;

use crate::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxShape {
    Rect,
    Ellipse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointPath {
    Polyline,
    Spline,
    Bezier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerToggle {
    Visible,
    Enabled,
    Locked,
    AirAssist,
}

/// Laufende, UI-unabhängige Editor-Sitzung.
///
/// `Deref`/`DerefMut` sind eine bewusst vorübergehende Migrationsbrücke für
/// noch nicht extrahierte Native-Abläufe. Neue Anwendungsfälle erhalten
/// benannte Methoden in den verantwortlichen Session-Modulen.
#[derive(Debug, Default)]
pub struct EditorSession {
    state: AppState,
    edit_start: Option<AppState>,
}

impl EditorSession {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            edit_start: None,
        }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn state_mut_for_migration(&mut self) -> &mut AppState {
        &mut self.state
    }

    pub fn replace_state(&mut self, state: AppState) -> AppState {
        self.edit_start = None;
        std::mem::replace(&mut self.state, state)
    }

    /// Ob ungespeicherte Änderungen vorliegen (für den Dirty-Guard).
    pub fn is_dirty(&self) -> bool {
        self.state.dirty
    }

    /// Nach erfolgreichem Speichern: der Zustand gilt als gesichert.
    pub fn mark_saved(&mut self) {
        self.state.mark_saved();
    }

    pub(super) fn require_selection(&self, action: &str) -> Result<(), AppError> {
        if self.state.selected.is_empty() {
            Err(AppError::new(
                "selection_required",
                format!("Für „{action}“ muss mindestens ein Objekt ausgewählt sein."),
            ))
        } else {
            Ok(())
        }
    }

    pub fn delete_selected(&mut self) -> Result<(), AppError> {
        if self.state.selected.is_empty() {
            return Err(AppError::new(
                "selection_required",
                "Zum Löschen muss mindestens ein Objekt ausgewählt sein.",
            ));
        }
        self.state.delete_selected();
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        self.state.undo()
    }

    pub fn redo(&mut self) -> bool {
        self.state.redo()
    }
}

impl Deref for EditorSession {
    type Target = AppState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for EditorSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

#[cfg(test)]
mod tests;
