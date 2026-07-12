//! Eingabe-Übersetzung für den Canvas: physische Tasten → typisierte `Key` und
//! die reinen Zeiger-Events (Bewegen/Klicken/Scrollen) auf `CanvasState`-Gesten.
//!
//! Fenster-/GPU-Ereignisse (Resize) und die Tastatur-Koordination
//! (`apply_shortcut`, das auch Projekt/Dialoge berührt) bleiben im App-Root.

use luxifer_application::EditorSession;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::keyboard::KeyCode;

use crate::tools::Key;

use super::state::CanvasState;

/// Übersetzt die für Shortcuts relevanten physischen Tasten in das
/// UI-unabhängige `tools::Key`. Alles andere ignoriert die Shortcut-Ebene.
pub fn map_keycode(code: KeyCode) -> Option<Key> {
    Some(match code {
        KeyCode::KeyS => Key::S,
        KeyCode::Delete | KeyCode::Backspace => Key::Delete,
        KeyCode::Escape => Key::Escape,
        KeyCode::Enter => Key::Enter,
        KeyCode::Space => Key::Space,
        KeyCode::KeyV => Key::V,
        KeyCode::KeyR => Key::R,
        KeyCode::KeyE => Key::E,
        KeyCode::KeyP => Key::P,
        KeyCode::KeyZ => Key::Z,
        KeyCode::KeyY => Key::Y,
        _ => return None,
    })
}

impl CanvasState {
    /// Behandelt ein reines Canvas-Zeiger-Event (Bewegen/Klicken/Scrollen).
    /// Gibt true zurück, wenn dabei ein Shape entstand (→ Root frischt Accent).
    /// Für andere Event-Arten (Tastatur, Resize) false; die behandelt der Root.
    pub fn handle_pointer_event(
        &mut self,
        session: &mut EditorSession,
        event: &WindowEvent,
    ) -> bool {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let new = [position.x as f32, position.y as f32];
                self.on_cursor_move(session, new);
                false
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.on_mouse(session, *button, *state == ElementState::Pressed)
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let s = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.cam.zoom_at(1.12_f32.powf(s), self.cursor);
                false
            }
            _ => false,
        }
    }
}
