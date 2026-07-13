//! Lineale (H2): mm-Skala oben und links am Canvas-Rand, Tick-Intervall
//! zoomabhängig in der 1/2/5er-Reihe, Cursor-Marker in Akzentfarbe.
//!
//! Gezeichnet wird direkt über einen Layer-Painter (kein Panel/Area): So
//! bleibt der Canvas-Bereich für egui „leer" und die Maus-Events erreichen
//! weiterhin die Canvas-Gesten statt von egui verschluckt zu werden.

use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke};

use crate::camera::Camera;

/// Lineal-Dicke in egui-Punkten.
const THICKNESS: f32 = 22.0;
/// Mindestabstand beschrifteter Haupt-Ticks in Punkten.
const MIN_LABEL_PT: f32 = 60.0;

/// Wählt das Haupt-Tick-Intervall (mm) in der 1/2/5er-Reihe: das kleinste,
/// das auf dem Bildschirm mindestens [`MIN_LABEL_PT`] Abstand hat.
fn tick_interval_mm(pt_per_mm: f32) -> f64 {
    let mut decade = 1.0_f64;
    loop {
        for mantissa in [1.0, 2.0, 5.0] {
            let interval = mantissa * decade;
            if interval as f32 * pt_per_mm >= MIN_LABEL_PT {
                return interval;
            }
        }
        decade *= 10.0;
        if decade > 1e7 {
            return decade;
        }
    }
}

/// Zeichnet beide Lineale in den Canvas-Bereich `rect` (Punkte).
/// `cursor_px` = Mausposition in physischen Pixeln (Canvas-Koordinaten).
pub(super) fn rulers(ctx: &egui::Context, cam: &Camera, cursor_px: [f32; 2], accent: [u8; 3]) {
    let rect = ctx.available_rect();
    if rect.width() < 2.0 * THICKNESS || rect.height() < 2.0 * THICKNESS {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("rulers"),
    ));
    let ppp = ctx.pixels_per_point();
    let pt_per_mm = cam.scale / ppp;

    let bg = Color32::from_rgb(0x17, 0x1a, 0x20);
    let border = Color32::from_rgb(0x2a, 0x2e, 0x36);
    let tick_col = Color32::from_rgb(0x6b, 0x71, 0x7b);
    let text_col = Color32::from_rgb(0x9a, 0xa0, 0xa9);
    let accent = Color32::from_rgb(accent[0], accent[1], accent[2]);
    let font = FontId::monospace(9.0);

    let top = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.min.y + THICKNESS));
    let left = Rect::from_min_max(rect.min, Pos2::new(rect.min.x + THICKNESS, rect.max.y));
    painter.rect_filled(top, 0.0, bg);
    painter.rect_filled(left, 0.0, bg);
    painter.line_segment(
        [top.left_bottom(), top.right_bottom()],
        Stroke::new(1.0, border),
    );
    painter.line_segment(
        [left.right_top(), left.right_bottom()],
        Stroke::new(1.0, border),
    );

    let interval = tick_interval_mm(pt_per_mm);
    // Zwischen-Ticks: 2er-Intervalle halbieren, 1er/5er fünfteln — so fallen
    // die Minor-Ticks immer auf runde mm-Werte.
    let minor = if (interval / 10f64.powf(interval.log10().floor()) - 2.0).abs() < 0.01 {
        interval / 2.0
    } else {
        interval / 5.0
    };

    // Ein Durchlauf pro Achse: Welt-mm → Bildschirm-Punkt, Ticks + Labels.
    let axis = |horizontal: bool| {
        let (p0, p1) = if horizontal {
            (rect.min.x + THICKNESS, rect.max.x)
        } else {
            (rect.min.y + THICKNESS, rect.max.y)
        };
        let world_at = |p: f32| {
            let px = p * ppp;
            if horizontal {
                cam.screen_to_world([px, 0.0])[0]
            } else {
                cam.screen_to_world([0.0, px])[1]
            }
        };
        let to_pt = |w: f64| {
            if horizontal {
                cam.world_to_screen([w, 0.0])[0] / ppp
            } else {
                cam.world_to_screen([0.0, w])[1] / ppp
            }
        };
        let w0 = world_at(p0);
        let w1 = world_at(p1);
        let i0 = (w0 / minor).floor() as i64;
        let i1 = (w1 / minor).ceil() as i64;
        let per_major = (interval / minor).round() as i64;
        for i in i0..=i1 {
            let w = i as f64 * minor;
            let p = to_pt(w);
            if p < p0 - 1.0 || p > p1 + 1.0 {
                continue;
            }
            let major = i.rem_euclid(per_major) == 0;
            let len = if major { 9.0 } else { 4.0 };
            if horizontal {
                let y1 = top.max.y;
                painter.line_segment(
                    [Pos2::new(p, y1 - len), Pos2::new(p, y1)],
                    Stroke::new(1.0, tick_col),
                );
                if major {
                    painter.text(
                        Pos2::new(p + 3.0, top.min.y + 1.0),
                        Align2::LEFT_TOP,
                        format!("{}", w.round() as i64),
                        font.clone(),
                        text_col,
                    );
                }
            } else {
                let x1 = left.max.x;
                painter.line_segment(
                    [Pos2::new(x1 - len, p), Pos2::new(x1, p)],
                    Stroke::new(1.0, tick_col),
                );
                if major {
                    // Ziffern einzeln untereinander — lesbar ohne Rotation
                    // (egui-Painter kann Text nicht drehen).
                    let label = format!("{}", w.round() as i64);
                    let mut y = p + 2.0;
                    for ch in label.chars() {
                        painter.text(
                            Pos2::new(left.min.x + 2.0, y),
                            Align2::LEFT_TOP,
                            ch,
                            font.clone(),
                            text_col,
                        );
                        y += 9.0;
                    }
                }
            }
        }
        // Cursor-Marker.
        let c = if horizontal {
            cursor_px[0] / ppp
        } else {
            cursor_px[1] / ppp
        };
        if c >= p0 && c <= p1 {
            if horizontal {
                painter.line_segment(
                    [Pos2::new(c, top.min.y), Pos2::new(c, top.max.y)],
                    Stroke::new(1.0, accent),
                );
            } else {
                painter.line_segment(
                    [Pos2::new(left.min.x, c), Pos2::new(left.max.x, c)],
                    Stroke::new(1.0, accent),
                );
            }
        }
    };
    axis(true);
    axis(false);

    // Eckfeld mit Einheit.
    let corner = Rect::from_min_size(rect.min, egui::vec2(THICKNESS, THICKNESS));
    painter.rect_filled(corner, 0.0, bg);
    painter.line_segment(
        [corner.right_top(), corner.right_bottom()],
        Stroke::new(1.0, border),
    );
    painter.line_segment(
        [corner.left_bottom(), corner.right_bottom()],
        Stroke::new(1.0, border),
    );
    painter.text(corner.center(), Align2::CENTER_CENTER, "mm", font, text_col);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_intervall_folgt_der_1_2_5_reihe() {
        // 1 mm = 100 pt → 1er-Intervall reicht (100 ≥ 60).
        assert_eq!(tick_interval_mm(100.0), 1.0);
        // 1 mm = 2 pt → 50 mm-Intervall (50·2 = 100 ≥ 60, 20·2 = 40 < 60).
        assert_eq!(tick_interval_mm(2.0), 50.0);
        // Grenzfall exakt: 30 pt/mm → 2 mm (2·30 = 60).
        assert_eq!(tick_interval_mm(30.0), 2.0);
        // Sehr weit rausgezoomt: Reihe setzt sich in Dekaden fort.
        assert_eq!(tick_interval_mm(0.001), 100_000.0);
    }
}
