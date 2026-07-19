//! Toast-Meldungen oben mittig: fahren von oben herein, stehen kurz und blenden
//! aus. Reiner Präsentationszustand für Erfolg (grün) und Fehler (rot), ohne
//! Panels oder Canvas zu verschieben.

use std::time::Instant;

use egui::{Align2, Color32, CornerRadius, RichText, Stroke};

/// Phasen der Lebensdauer in Sekunden.
const SLIDE_IN: f32 = 0.25;
const HOLD: f32 = 3.5;
const FADE_OUT: f32 = 0.45;
const WIDTH: f32 = 520.0;
/// Abstand zum Fensterrand und zwischen gestapelten Toasts.
const MARGIN: f32 = 12.0;
/// Textgröße — bewusst größer als der Panel-Standard, Toasts sind flüchtig.
const TEXT_SIZE: f32 = 16.0;

#[derive(Clone, Copy)]
enum ToastKind {
    Success,
    Error,
}

impl ToastKind {
    /// Signalfarbe (Punkt + Randton). Grün/Rot passend zum dunklen Theme;
    /// das Rot entspricht dem Fehler-Banner.
    fn color(self) -> Color32 {
        match self {
            ToastKind::Success => Color32::from_rgb(0x4a, 0xde, 0x80),
            ToastKind::Error => Color32::from_rgb(0xf8, 0x71, 0x71),
        }
    }
}

struct Toast {
    text: String,
    kind: ToastKind,
    born: Instant,
    /// Stabile egui-Id, damit ein Toast beim Ablauf seiner Vorgänger nicht
    /// die Identität wechselt.
    id: u64,
}

#[derive(Default)]
pub struct Toasts {
    items: Vec<Toast>,
    next_id: u64,
}

/// Weiche S-Kurve (smoothstep) für Ein-/Ausfahren.
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Vertikaler Eintrittsversatz und Deckkraft für ein Toast-Alter.
fn motion(age: f32) -> (f32, f32) {
    let enter = ease(age / SLIDE_IN);
    let y_offset = -96.0 * (1.0 - enter);
    let opacity = if age > SLIDE_IN + HOLD {
        1.0 - ease((age - SLIDE_IN - HOLD) / FADE_OUT)
    } else {
        1.0
    };
    (y_offset, opacity.clamp(0.0, 1.0))
}

impl Toasts {
    /// Grüner Erfolgs-/Statustoast.
    pub fn success(&mut self, text: impl Into<String>) {
        self.push(text, ToastKind::Success);
    }

    /// Roter Fehler-/Warntoast (für leichte Fehler, die keinen Banner brauchen).
    pub fn error(&mut self, text: impl Into<String>) {
        self.push(text, ToastKind::Error);
    }

    fn push(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.items.push(Toast {
            text: text.into(),
            kind,
            born: Instant::now(),
            id: self.next_id,
        });
        self.next_id = self.next_id.wrapping_add(1);
    }

    /// Zeichnet alle aktiven Toasts (Aufruf am Ende von `ui::build`, damit sie
    /// über den Panels liegen) und entfernt abgelaufene.
    pub fn show(&mut self, root_ui: &mut egui::Ui) {
        let now = Instant::now();
        self.items
            .retain(|t| now.duration_since(t.born).as_secs_f32() < SLIDE_IN + HOLD + FADE_OUT);
        if self.items.is_empty() {
            return;
        }

        let top = root_ui.max_rect().top() + MARGIN;
        let center_x = root_ui.max_rect().center().x;

        let mut y = top;
        for t in &self.items {
            let age = now.duration_since(t.born).as_secs_f32();
            let (y_offset, opacity) = motion(age);
            let base_color = t.kind.color();
            let color = base_color.linear_multiply(opacity);
            let fill = Color32::from_rgba_unmultiplied(
                0x1c,
                0x1f,
                0x26,
                (0xf0 as f32 * opacity).round() as u8,
            );
            let text = Color32::WHITE.linear_multiply(opacity);

            let response = egui::Area::new(egui::Id::new(("toast", t.id)))
                .order(egui::Order::Foreground)
                .interactable(false)
                .pivot(Align2::CENTER_TOP)
                .fixed_pos(egui::pos2(center_x, y + y_offset))
                .show(root_ui, |ui| {
                    egui::Frame::new()
                        .fill(fill)
                        .stroke(Stroke::new(1.5, color.gamma_multiply(0.6)))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(16, 12))
                        .show(ui, |ui| {
                            ui.set_max_width(WIDTH);
                            ui.horizontal(|ui| {
                                // Signalpunkt in der Statusfarbe statt Icon.
                                let (dot, _) = ui.allocate_exact_size(
                                    egui::vec2(12.0, 12.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().circle_filled(dot.center(), 6.0, color);
                                ui.label(RichText::new(&t.text).size(TEXT_SIZE).color(text));
                            });
                        });
                })
                .response;
            y += response.rect.height() + 8.0;
        }
        // Animation läuft — bis alle Toasts weg sind weiterzeichnen.
        root_ui.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_faehrt_von_oben_ein_und_blendet_am_ende_aus() {
        let start = motion(0.0);
        let visible = motion(SLIDE_IN);
        let fading = motion(SLIDE_IN + HOLD + FADE_OUT * 0.5);
        let end = motion(SLIDE_IN + HOLD + FADE_OUT);

        assert!(start.0 < visible.0);
        assert_eq!(visible, (0.0, 1.0));
        assert!(fading.1 > 0.0 && fading.1 < 1.0);
        assert_eq!(end.1, 0.0);
    }
}
