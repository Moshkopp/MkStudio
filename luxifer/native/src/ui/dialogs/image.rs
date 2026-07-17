//! Bildparameter-Dialog (Doppelklick auf ein Bild-Objekt). Bearbeitet die
//! nicht-destruktiven Verarbeitungsparameter (ADR 0004) und bietet das
//! Vektorisieren (Trace) an; Native hält nur den Entwurf, Speichern läuft
//! über `EditorSession::set_image_params`, Trace über
//! `EditorSession::trace_image`.

use luxifer_core::ImageMode;

use super::super::state::{CropKind, ImageDialogPage, ImageDialogState};

/// Ergebnis des Bild-Dialogs. `Trace` vektorisiert das Bild mit den
/// Trace-Reglern des Entwurfs (der Dialog bleibt dabei offen).
#[derive(PartialEq, Eq)]
pub(in crate::ui) enum ImageDialogOutcome {
    None,
    Save,
    Cancel,
    Trace,
    Crop,
}

pub(in crate::ui) fn image_dialog_window(
    root_ui: &mut egui::Ui,
    st: &mut ImageDialogState,
) -> ImageDialogOutcome {
    let mut outcome = ImageDialogOutcome::None;
    let title = match st.page {
        ImageDialogPage::Settings => "Bild bearbeiten",
        ImageDialogPage::Trace => "Bild vektorisieren",
        ImageDialogPage::Crop => "Bild zuschneiden",
    };
    egui::Window::new(title)
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(true)
        .default_size(egui::vec2(860.0, 430.0))
        .min_size(egui::vec2(720.0, 380.0))
        .max_height(520.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(root_ui, |ui| {
            match st.page {
                ImageDialogPage::Settings => ui.columns(2, |columns| {
                    settings_panel(&mut columns[0], st);
                    preview_panel(&mut columns[1], st, "LIVE-VORSCHAU");
                }),
                ImageDialogPage::Trace => ui.columns(2, |columns| {
                    trace_panel(&mut columns[0], st, &mut outcome);
                    preview_panel(&mut columns[1], st, "ERFASSTE BEREICHE");
                }),
                ImageDialogPage::Crop => ui.columns(2, |columns| {
                    crop_panel(&mut columns[0], st, &mut outcome);
                    preview_panel(&mut columns[1], st, "AUSSCHNITT-VORSCHAU");
                }),
            };

            ui.separator();
            ui.horizontal(|ui| match st.page {
                ImageDialogPage::Settings => {
                    if ui.button("Speichern").clicked() {
                        outcome = ImageDialogOutcome::Save;
                    }
                    if ui.button("Abbrechen").clicked() {
                        outcome = ImageDialogOutcome::Cancel;
                    }
                }
                ImageDialogPage::Trace => {
                    if ui.button("Zurück").clicked() {
                        return_to_settings(st);
                    }
                    if ui.button("Schließen").clicked() {
                        outcome = ImageDialogOutcome::Cancel;
                    }
                }
                ImageDialogPage::Crop => {
                    if ui.button("Zurück").clicked() {
                        return_to_settings(st);
                    }
                    if ui.button("Schließen").clicked() {
                        outcome = ImageDialogOutcome::Cancel;
                    }
                }
            });
        });
    outcome
}

fn settings_panel(ui: &mut egui::Ui, st: &mut ImageDialogState) {
    ui.label(egui::RichText::new("EINSTELLUNGEN").small().weak());
    let p = &mut st.params;
    egui::Grid::new("image_cfg")
        .num_columns(2)
        .spacing([8.0, 8.0])
        .show(ui, |ui| {
            ui.label("Modus");
            let mode_label = |m: ImageMode| match m {
                ImageMode::Grayscale => "Graustufe",
                ImageMode::Threshold => "Schwelle",
                ImageMode::Floyd => "Floyd–Steinberg",
                ImageMode::Jarvis => "Jarvis",
                ImageMode::Stucki => "Stucki",
                ImageMode::Atkinson => "Atkinson",
                ImageMode::Bayer => "Bayer 4×4",
                ImageMode::LaserRuns => "Laser-Runs",
            };
            egui::ComboBox::from_id_salt("image_mode")
                .selected_text(mode_label(p.mode))
                .width(220.0)
                .show_ui(ui, |ui| {
                    for m in [
                        ImageMode::Grayscale,
                        ImageMode::Threshold,
                        ImageMode::Floyd,
                        ImageMode::Jarvis,
                        ImageMode::Stucki,
                        ImageMode::Atkinson,
                        ImageMode::Bayer,
                        ImageMode::LaserRuns,
                    ] {
                        ui.selectable_value(&mut p.mode, m, mode_label(m));
                    }
                });
            ui.end_row();

            if p.mode == ImageMode::Threshold {
                ui.label("Schwelle");
                ui.add(egui::Slider::new(&mut p.threshold, 0..=255));
                ui.end_row();
            }

            ui.label("Helligkeit");
            ui.add(egui::Slider::new(&mut p.brightness, -100..=100));
            ui.end_row();

            ui.label("Kontrast");
            ui.add(egui::Slider::new(&mut p.contrast, -100..=100));
            ui.end_row();

            ui.label("Gamma");
            ui.add(egui::Slider::new(&mut p.gamma, 0.1..=3.0));
            ui.end_row();

            ui.label("Invertieren (Canvas)");
            ui.checkbox(&mut p.invert_editor, "");
            ui.end_row();

            ui.label("Invertieren (Laser)");
            ui.checkbox(&mut p.invert_laser, "");
            ui.end_row();
        });

    ui.add_space(16.0);
    ui.separator();
    if ui.button("Vektorisieren …").clicked() {
        st.page = ImageDialogPage::Trace;
        st.preview_key = None;
        st.preview_zoom = 1.0;
        st.preview_pan = egui::Vec2::ZERO;
    }
    ui.weak("Öffnet die Trace-Einstellungen mit eigener Ergebnisvorschau.");
    ui.add_space(6.0);
    if ui.button("Zuschneiden …").clicked() {
        st.page = ImageDialogPage::Crop;
        reset_preview_view(st);
    }
    ui.weak("Öffnet das Zuschneiden als eigenen Arbeitsbereich.");
}

fn crop_panel(ui: &mut egui::Ui, st: &mut ImageDialogState, outcome: &mut ImageDialogOutcome) {
    ui.label(egui::RichText::new("CROP-FORM").small().weak());
    ui.horizontal(|ui| {
        ui.selectable_value(&mut st.crop_kind, CropKind::Rect, "Rechteck");
        if ui
            .selectable_value(&mut st.crop_kind, CropKind::Ellipse, "Ellipse")
            .clicked()
        {
            st.crop_ellipse_points = 0;
            st.crop_ellipse_error = None;
            st.crop_drag_handle = None;
        }
    });
    if st.crop_kind == CropKind::Ellipse {
        ui.add_space(6.0);
        ui.label(match st.crop_ellipse_points {
            0 => "1. Punkt auf dem Kreis setzen",
            1 => "2. Punkt auf dem Kreis setzen",
            2 => "3. Punkt auf dem Kreis setzen",
            _ => "Kreis über die Bounding Box anpassen.",
        });
        ui.weak("Der Kreis läuft exakt durch die drei frei gesetzten Punkte.");
        if let Some(error) = &st.crop_ellipse_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }
    ui.add_space(10.0);
    ui.label(egui::RichText::new("SCHNITTKANTEN").small().weak());
    ui.add_space(6.0);
    let mut left = st.crop_rect[0] * 100.0;
    let mut top = st.crop_rect[1] * 100.0;
    let mut right = (1.0 - st.crop_rect[2]) * 100.0;
    let mut bottom = (1.0 - st.crop_rect[3]) * 100.0;
    egui::Grid::new("image_crop")
        .num_columns(2)
        .spacing([8.0, 10.0])
        .show(ui, |ui| {
            for (label, value) in [
                ("Links", &mut left),
                ("Oben", &mut top),
                ("Rechts", &mut right),
                ("Unten", &mut bottom),
            ] {
                ui.label(label);
                ui.add(egui::Slider::new(value, 0.0..=99.0).suffix(" %"));
                ui.end_row();
            }
        });
    let max_horizontal = 99.0;
    if left + right > max_horizontal {
        right = max_horizontal - left;
    }
    if top + bottom > 99.0 {
        bottom = 99.0 - top;
    }
    st.crop_rect = [
        left / 100.0,
        top / 100.0,
        1.0 - right / 100.0,
        1.0 - bottom / 100.0,
    ];
    ui.add_space(8.0);
    if ui.button("Vollen Bildbereich wiederherstellen").clicked() {
        st.crop_rect = [0.0, 0.0, 1.0, 1.0];
        st.crop_ellipse = [[0.5, 0.5], [0.85, 0.5], [0.5, 0.85]];
        st.crop_ellipse_points = if st.crop_kind == CropKind::Ellipse {
            3
        } else {
            0
        };
        st.crop_ellipse_error = None;
        st.preview_key = None;
    }
    ui.add_space(12.0);
    if ui.button("Ausschnitt anwenden").clicked() {
        *outcome = ImageDialogOutcome::Crop;
    }
    ui.weak(
        "Das Originalasset bleibt erhalten; Undo stellt die vorherige Bildreferenz wieder her.",
    );
}

fn reset_preview_view(st: &mut ImageDialogState) {
    st.preview_key = None;
    st.preview_zoom = 1.0;
    st.preview_pan = egui::Vec2::ZERO;
}

fn return_to_settings(st: &mut ImageDialogState) {
    st.page = ImageDialogPage::Settings;
    reset_preview_view(st);
}

fn trace_panel(ui: &mut egui::Ui, st: &mut ImageDialogState, outcome: &mut ImageDialogOutcome) {
    ui.label(egui::RichText::new("TRACE-EINSTELLUNGEN").small().weak());
    ui.add_space(6.0);
    egui::Grid::new("image_trace")
        .num_columns(2)
        .spacing([8.0, 10.0])
        .show(ui, |ui| {
            ui.label("Schwelle");
            ui.add(egui::Slider::new(&mut st.trace_threshold, 0..=255));
            ui.end_row();
            ui.label("Invertieren");
            ui.checkbox(&mut st.trace_invert, "");
            ui.end_row();
        });
    ui.add_space(12.0);
    if ui.button("Konturen erzeugen").clicked() {
        *outcome = ImageDialogOutcome::Trace;
    }
    ui.weak("Schwarz zeigt die erfassten Motivbereiche. Das Originalbild bleibt unverändert.");
}

fn preview_panel(ui: &mut egui::Ui, st: &mut ImageDialogState, label: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).small().weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Ansicht zurücksetzen").clicked() {
                st.preview_zoom = 1.0;
                st.preview_pan = egui::Vec2::ZERO;
            }
        });
    });

    let desired = egui::vec2(ui.available_width(), 340.0);
    // Crop benötigt echte Einzelklicks für die Drei-Punkt-Ellipse und Drag für
    // Rechteck/Griffe. `Sense::drag()` allein liefert die Klicks nicht auf
    // allen egui-Backends zuverlässig.
    let (response, painter) = ui.allocate_painter(desired, egui::Sense::click_and_drag());
    let rect = response.rect;
    painter.rect_filled(rect, 8.0, ui.visuals().extreme_bg_color);
    painter.rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(1.0, ui.visuals().window_stroke.color),
        egui::StrokeKind::Inside,
    );

    if response.dragged() && st.page != ImageDialogPage::Crop {
        st.preview_pan += response.drag_delta();
    }
    if response.hovered() {
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll != 0.0 {
            st.preview_zoom = (st.preview_zoom * (scroll * 0.002).exp()).clamp(0.1, 20.0);
        }
    }

    let mut painted_image_rect = None;
    if let Some(texture) = &st.preview {
        let original = texture.size_vec2();
        let viewport = rect.shrink(16.0).size();
        let fit = (viewport.x / original.x).min(viewport.y / original.y);
        let size = original * fit * st.preview_zoom;
        let image_rect = egui::Rect::from_center_size(rect.center() + st.preview_pan, size);
        painter.image(
            texture.id(),
            image_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        painted_image_rect = Some(image_rect);
    } else if let Some(error) = &st.preview_error {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            error,
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().error_fg_color,
        );
    } else {
        ui.ctx().request_repaint();
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Vorschau wird geladen …",
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().weak_text_color(),
        );
    }

    if st.page == ImageDialogPage::Crop {
        if let Some(image_rect) = painted_image_rect {
            crop_overlay(ui, st, &response, &painter, image_rect);
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }

    if response.hovered() && st.page != ImageDialogPage::Crop {
        response
            .on_hover_cursor(egui::CursorIcon::Grab)
            .on_hover_text("Ziehen: verschieben · Mausrad: zoomen");
    }
}

fn crop_overlay(
    ui: &egui::Ui,
    st: &mut ImageDialogState,
    response: &egui::Response,
    painter: &egui::Painter,
    image_rect: egui::Rect,
) {
    if image_rect.width() <= 1.0 || image_rect.height() <= 1.0 {
        return;
    }
    let to_pos = |point: [f32; 2]| {
        egui::pos2(
            image_rect.left() + point[0] * image_rect.width(),
            image_rect.top() + point[1] * image_rect.height(),
        )
    };
    let to_norm = |pos: egui::Pos2| {
        [
            ((pos.x - image_rect.left()) / image_rect.width()).clamp(0.0, 1.0),
            ((pos.y - image_rect.top()) / image_rect.height()).clamp(0.0, 1.0),
        ]
    };
    let to_norm_free = |pos: egui::Pos2| {
        [
            (pos.x - image_rect.left()) / image_rect.width(),
            (pos.y - image_rect.top()) / image_rect.height(),
        ]
    };
    let pointer = response.interact_pointer_pos();
    let stroke = egui::Stroke::new(2.0, ui.visuals().selection.stroke.color);

    match st.crop_kind {
        CropKind::Rect => {
            let min = to_pos([st.crop_rect[0], st.crop_rect[1]]);
            let max = to_pos([st.crop_rect[2], st.crop_rect[3]]);
            let selection = egui::Rect::from_min_max(min, max);
            painter.rect_stroke(selection, 0.0, stroke, egui::StrokeKind::Inside);
            let handles = [
                selection.left_top(),
                selection.right_top(),
                selection.right_bottom(),
                selection.left_bottom(),
            ];
            for point in handles {
                painter.circle_filled(point, 5.0, stroke.color);
            }
            if response.drag_started() {
                if let Some(pos) = pointer {
                    st.crop_drag_handle = handles.iter().position(|p| p.distance(pos) <= 10.0);
                    st.crop_drag_start = Some(to_norm(pos));
                    if st.crop_drag_handle.is_none() {
                        let p = to_norm(pos);
                        st.crop_rect = [p[0], p[1], p[0], p[1]];
                    }
                }
            }
            if response.dragged() {
                if let Some(pos) = pointer {
                    let p = to_norm(pos);
                    if let Some(handle) = st.crop_drag_handle {
                        match handle {
                            0 => {
                                st.crop_rect[0] = p[0];
                                st.crop_rect[1] = p[1];
                            }
                            1 => {
                                st.crop_rect[2] = p[0];
                                st.crop_rect[1] = p[1];
                            }
                            2 => {
                                st.crop_rect[2] = p[0];
                                st.crop_rect[3] = p[1];
                            }
                            _ => {
                                st.crop_rect[0] = p[0];
                                st.crop_rect[3] = p[1];
                            }
                        }
                    } else if let Some(start) = st.crop_drag_start {
                        st.crop_rect = [
                            start[0].min(p[0]),
                            start[1].min(p[1]),
                            start[0].max(p[0]),
                            start[1].max(p[1]),
                        ];
                    }
                }
            }
        }
        CropKind::Ellipse => {
            if st.crop_ellipse_points == 3 {
                let center = to_pos(st.crop_ellipse[0]);
                let axis_a = to_pos(st.crop_ellipse[1]);
                let axis_b = to_pos(st.crop_ellipse[2]);
                let a = axis_a - center;
                let b = axis_b - center;
                let points: Vec<_> = (0..=64)
                    .map(|i| {
                        let t = i as f32 * std::f32::consts::TAU / 64.0;
                        center + a * t.cos() + b * t.sin()
                    })
                    .collect();
                painter.add(egui::Shape::line(points, stroke));
                let bbox = egui::Rect::from_center_size(
                    center,
                    egui::vec2(a.x.abs() * 2.0, b.y.abs() * 2.0),
                );
                painter.rect_stroke(
                    bbox,
                    0.0,
                    egui::Stroke::new(1.0, stroke.color),
                    egui::StrokeKind::Inside,
                );
                for handle in ellipse_bbox_handles(bbox) {
                    painter.rect_filled(
                        egui::Rect::from_center_size(handle, egui::vec2(9.0, 9.0)),
                        1.0,
                        egui::Color32::WHITE,
                    );
                    painter.rect_stroke(
                        egui::Rect::from_center_size(handle, egui::vec2(9.0, 9.0)),
                        1.0,
                        egui::Stroke::new(1.5, stroke.color),
                        egui::StrokeKind::Inside,
                    );
                }
            } else {
                for point in st
                    .crop_ellipse
                    .iter()
                    .take(st.crop_ellipse_points as usize)
                    .copied()
                {
                    let pos = to_pos(point);
                    painter.circle_filled(pos, 7.0, egui::Color32::WHITE);
                    painter.circle_stroke(pos, 7.0, egui::Stroke::new(2.0, stroke.color));
                }
                if st.crop_ellipse_points == 2 {
                    painter.line_segment(
                        [to_pos(st.crop_ellipse[0]), to_pos(st.crop_ellipse[1])],
                        egui::Stroke::new(1.0, stroke.color),
                    );
                }
            }
            if response.clicked() && st.crop_ellipse_points < 3 {
                if let Some(pos) = pointer {
                    let p = to_norm(pos);
                    match st.crop_ellipse_points {
                        0 => {
                            st.crop_ellipse[0] = p;
                            st.crop_ellipse_points = 1;
                            st.crop_ellipse_error = None;
                        }
                        1 => {
                            st.crop_ellipse[1] = p;
                            st.crop_ellipse_points = 2;
                        }
                        2 => {
                            let first = to_pos(st.crop_ellipse[0]);
                            let second = to_pos(st.crop_ellipse[1]);
                            if let Some((center, radius)) = circumcircle(first, second, pos) {
                                st.crop_ellipse = [
                                    to_norm_free(center),
                                    to_norm_free(center + egui::vec2(radius, 0.0)),
                                    to_norm_free(center + egui::vec2(0.0, radius)),
                                ];
                                st.crop_ellipse_points = 3;
                                st.crop_ellipse_error = None;
                            } else {
                                st.crop_ellipse_error =
                                    Some("Die drei Punkte liegen fast auf einer Linie.".into());
                            }
                        }
                        _ => {}
                    }
                }
            }
            if st.crop_ellipse_points == 3 && response.drag_started() {
                if let Some(pos) = pointer {
                    let center = to_pos(st.crop_ellipse[0]);
                    let bbox = egui::Rect::from_min_max(
                        egui::pos2(
                            2.0 * center.x - to_pos(st.crop_ellipse[1]).x,
                            2.0 * center.y - to_pos(st.crop_ellipse[2]).y,
                        ),
                        egui::pos2(to_pos(st.crop_ellipse[1]).x, to_pos(st.crop_ellipse[2]).y),
                    );
                    st.crop_drag_handle = ellipse_bbox_handles(bbox)
                        .iter()
                        .position(|point| point.distance(pos) <= 10.0);
                }
            }
            if response.dragged() {
                if let (Some(handle), Some(pos)) = (st.crop_drag_handle, pointer) {
                    let center = to_pos(st.crop_ellipse[0]);
                    let mut min = egui::pos2(
                        2.0 * center.x - to_pos(st.crop_ellipse[1]).x,
                        2.0 * center.y - to_pos(st.crop_ellipse[2]).y,
                    );
                    let mut max =
                        egui::pos2(to_pos(st.crop_ellipse[1]).x, to_pos(st.crop_ellipse[2]).y);
                    match handle {
                        0 => min = pos,
                        1 => min.y = pos.y,
                        2 => {
                            max.x = pos.x;
                            min.y = pos.y;
                        }
                        3 => max.x = pos.x,
                        4 => max = pos,
                        5 => max.y = pos.y,
                        6 => {
                            min.x = pos.x;
                            max.y = pos.y;
                        }
                        _ => min.x = pos.x,
                    }
                    min.x = min.x.clamp(image_rect.left(), max.x - 4.0);
                    min.y = min.y.clamp(image_rect.top(), max.y - 4.0);
                    max.x = max.x.clamp(min.x + 4.0, image_rect.right());
                    max.y = max.y.clamp(min.y + 4.0, image_rect.bottom());
                    let center = min + (max - min) * 0.5;
                    st.crop_ellipse = [
                        to_norm(center),
                        to_norm(egui::pos2(max.x, center.y)),
                        to_norm(egui::pos2(center.x, max.y)),
                    ];
                }
            }
        }
    }
    if response.drag_stopped() {
        st.crop_drag_handle = None;
        st.crop_drag_start = None;
    }
}

fn ellipse_bbox_handles(rect: egui::Rect) -> [egui::Pos2; 8] {
    [
        rect.left_top(),
        egui::pos2(rect.center().x, rect.top()),
        rect.right_top(),
        egui::pos2(rect.right(), rect.center().y),
        rect.right_bottom(),
        egui::pos2(rect.center().x, rect.bottom()),
        rect.left_bottom(),
        egui::pos2(rect.left(), rect.center().y),
    ]
}

fn circumcircle(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> Option<(egui::Pos2, f32)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 0.01 {
        return None;
    }
    let aa = a.x * a.x + a.y * a.y;
    let bb = b.x * b.x + b.y * b.y;
    let cc = c.x * c.x + c.y * c.y;
    let center = egui::pos2(
        (aa * (b.y - c.y) + bb * (c.y - a.y) + cc * (a.y - b.y)) / d,
        (aa * (c.x - b.x) + bb * (a.x - c.x) + cc * (b.x - a.x)) / d,
    );
    let radius = center.distance(a);
    (center.x.is_finite() && center.y.is_finite() && radius.is_finite()).then_some((center, radius))
}

#[cfg(test)]
mod tests {
    use super::circumcircle;

    #[test]
    fn umkreis_laeuft_durch_alle_drei_punkte() {
        let a = egui::pos2(10.0, 20.0);
        let b = egui::pos2(50.0, 20.0);
        let c = egui::pos2(30.0, 40.0);
        let (center, radius) = circumcircle(a, b, c).expect("Umkreis");
        for point in [a, b, c] {
            assert!((center.distance(point) - radius).abs() < 0.001);
        }
    }

    #[test]
    fn kollineare_punkte_haben_keinen_umkreis() {
        assert!(circumcircle(
            egui::pos2(0.0, 0.0),
            egui::pos2(10.0, 10.0),
            egui::pos2(20.0, 20.0),
        )
        .is_none());
    }
}
