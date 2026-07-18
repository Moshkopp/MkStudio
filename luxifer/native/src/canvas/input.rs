//! Eingabe-Übersetzung für den Canvas: logische Tasten → typisierte `Key` und
//! die reinen Zeiger-Events (Bewegen/Klicken/Scrollen) auf `CanvasState`-Gesten.
//!
//! Fenster-/GPU-Ereignisse (Resize) und die Tastatur-Koordination
//! (`apply_shortcut`, das auch Projekt/Dialoge berührt) bleiben im App-Root.

use luxifer_application::EditorSession;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key as WinitKey, NamedKey};

use crate::tools::Drag;
use crate::tools::Key;

use super::state::CanvasState;

/// Übersetzt die für Shortcuts relevanten Tasten in das UI-unabhängige
/// `tools::Key`. Bewusst die **logische** Taste (layout-abhängig), nicht die
/// physische Position: Auf QWERTZ sind Z/Y gegenüber US vertauscht — mit
/// `physical_key` wäre Strg+Z sonst Redo. Buchstaben case-insensitiv, weil
/// Shift (Strg+Shift+Z) den Großbuchstaben liefert.
pub fn map_key(key: &WinitKey) -> Option<Key> {
    Some(match key {
        WinitKey::Named(NamedKey::Delete) | WinitKey::Named(NamedKey::Backspace) => Key::Delete,
        WinitKey::Named(NamedKey::Escape) => Key::Escape,
        WinitKey::Named(NamedKey::Enter) => Key::Enter,
        WinitKey::Named(NamedKey::Space) => Key::Space,
        WinitKey::Named(NamedKey::F1) => Key::F1,
        WinitKey::Named(NamedKey::F2) => Key::F2,
        WinitKey::Named(NamedKey::F3) => Key::F3,
        WinitKey::Named(NamedKey::F4) => Key::F4,
        WinitKey::Named(NamedKey::F5) => Key::F5,
        WinitKey::Named(NamedKey::F6) => Key::F6,
        WinitKey::Named(NamedKey::F7) => Key::F7,
        WinitKey::Named(NamedKey::F8) => Key::F8,
        WinitKey::Named(NamedKey::F9) => Key::F9,
        WinitKey::Named(NamedKey::F10) => Key::F10,
        WinitKey::Named(NamedKey::F11) => Key::F11,
        WinitKey::Named(NamedKey::F12) => Key::F12,
        WinitKey::Named(NamedKey::Home) => Key::Home,
        WinitKey::Named(NamedKey::End) => Key::End,
        WinitKey::Named(NamedKey::PageUp) => Key::PageUp,
        WinitKey::Named(NamedKey::PageDown) => Key::PageDown,
        WinitKey::Named(NamedKey::ArrowUp) => Key::ArrowUp,
        WinitKey::Named(NamedKey::ArrowDown) => Key::ArrowDown,
        WinitKey::Named(NamedKey::ArrowLeft) => Key::ArrowLeft,
        WinitKey::Named(NamedKey::ArrowRight) => Key::ArrowRight,
        WinitKey::Character(c) => match c.to_ascii_lowercase().as_str() {
            " " => Key::Space,
            "a" => Key::A,
            "b" => Key::B,
            "c" => Key::C,
            "d" => Key::D,
            "e" => Key::E,
            "f" => Key::F,
            "g" => Key::G,
            "h" => Key::H,
            "i" => Key::I,
            "j" => Key::J,
            "k" => Key::K,
            "l" => Key::L,
            "m" => Key::M,
            "n" => Key::N,
            "o" => Key::O,
            "p" => Key::P,
            "q" => Key::Q,
            "r" => Key::R,
            "s" => Key::S,
            "t" => Key::T,
            "u" => Key::U,
            "v" => Key::V,
            "w" => Key::W,
            "x" => Key::X,
            "y" => Key::Y,
            "z" => Key::Z,
            "0" => Key::Num0,
            "1" => Key::Num1,
            "2" => Key::Num2,
            "3" => Key::Num3,
            "4" => Key::Num4,
            "5" => Key::Num5,
            "6" => Key::Num6,
            "7" => Key::Num7,
            "8" => Key::Num8,
            "9" => Key::Num9,
            _ => return None,
        },
        _ => return None,
    })
}

impl CanvasState {
    /// Read-only Navigation der Preview: Mittelmaus-Pan und Mausrad-Zoom,
    /// keinerlei Auswahl- oder Zeichenmutation.
    pub fn handle_preview_pointer_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let new = [position.x as f32, position.y as f32];
                if matches!(self.drag, Drag::Pan) {
                    self.cam
                        .pan_pixels(new[0] - self.cursor[0], new[1] - self.cursor[1]);
                }
                self.cursor = new;
            }
            WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Middle,
                ..
            } => {
                self.drag = if *state == ElementState::Pressed {
                    Drag::Pan
                } else {
                    Drag::None
                };
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.cam.zoom_at(1.12_f32.powf(steps), self.cursor);
            }
            _ => {}
        }
    }

    /// Behandelt ein reines Canvas-Zeiger-Event (Bewegen/Klicken/Scrollen) und
    /// meldet dessen Ergebnis (Shape entstanden, Doppelklick auf Shape). Für
    /// andere Event-Arten (Tastatur, Resize) ein leeres Ergebnis; die behandelt
    /// der Root.
    pub fn handle_pointer_event(
        &mut self,
        session: &mut EditorSession,
        event: &WindowEvent,
    ) -> super::gestures::PointerOutcome {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let new = [position.x as f32, position.y as f32];
                self.on_cursor_move(session, new);
                Default::default()
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
                Default::default()
            }
            _ => Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// QWERTZ-Regression: Die logische Taste „z" muss `Key::Z` liefern —
    /// unabhängig davon, auf welcher physischen Position sie liegt. Mit Shift
    /// (Strg+Shift+Z = Redo) kommt der Großbuchstabe an.
    #[test]
    fn buchstaben_folgen_dem_layout_nicht_der_position() {
        assert_eq!(map_key(&WinitKey::Character("z".into())), Some(Key::Z));
        assert_eq!(map_key(&WinitKey::Character("Z".into())), Some(Key::Z));
        assert_eq!(map_key(&WinitKey::Character("y".into())), Some(Key::Y));
        assert_eq!(map_key(&WinitKey::Character("x".into())), Some(Key::X));
    }

    #[test]
    fn benannte_tasten_werden_erkannt() {
        assert_eq!(
            map_key(&WinitKey::Named(NamedKey::Escape)),
            Some(Key::Escape)
        );
        assert_eq!(
            map_key(&WinitKey::Named(NamedKey::Backspace)),
            Some(Key::Delete)
        );
        assert_eq!(map_key(&WinitKey::Named(NamedKey::Space)), Some(Key::Space));
        assert_eq!(map_key(&WinitKey::Named(NamedKey::F5)), Some(Key::F5));
        assert_eq!(map_key(&WinitKey::Named(NamedKey::Tab)), None);
    }
}
