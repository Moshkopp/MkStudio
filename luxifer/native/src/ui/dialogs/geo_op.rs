//! Parameterdialog für die geometrischen Operationen Boolean, Offset, Fillet
//! und Muster-Füllung. Native hält nur den Entwurf; die Ausführung läuft über
//! die Session (mit Auswahlvoraussetzung und Undo).

use luxifer_core::pattern_fill::Pattern;
use luxifer_core::BoolOp;

use super::super::state::{GeoOpDialogState, GeoOpKind};
use super::DialogOutcome;

pub(in crate::ui) fn geo_op_dialog_window(
    ctx: &egui::Context,
    st: &mut GeoOpDialogState,
) -> DialogOutcome {
    let mut outcome = DialogOutcome::None;
    let title = match st.kind {
        GeoOpKind::Boolean => "Boolesche Operation",
        GeoOpKind::Offset => "Offset",
        GeoOpKind::Fillet => "Ecken verrunden",
        GeoOpKind::PatternFill => "Muster-Füllung",
    };
    egui::Window::new(title)
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(300.0);
            match st.kind {
                GeoOpKind::Boolean => {
                    let label = |op: BoolOp| match op {
                        BoolOp::Union => "Vereinigen (A ∪ B)",
                        BoolOp::Intersect => "Schneiden (A ∩ B)",
                        BoolOp::Difference => "Abziehen (A − B)",
                    };
                    egui::ComboBox::from_label("Variante")
                        .selected_text(label(st.bool_op))
                        .show_ui(ui, |ui| {
                            for op in [BoolOp::Union, BoolOp::Intersect, BoolOp::Difference] {
                                ui.selectable_value(&mut st.bool_op, op, label(op));
                            }
                        });
                }
                GeoOpKind::Offset => {
                    ui.horizontal(|ui| {
                        ui.label("Distanz (mm)");
                        ui.add(
                            egui::DragValue::new(&mut st.distance)
                                .range(-100.0..=100.0)
                                .speed(0.1),
                        );
                    });
                }
                GeoOpKind::Fillet => {
                    ui.horizontal(|ui| {
                        ui.label("Radius (mm)");
                        ui.add(
                            egui::DragValue::new(&mut st.radius)
                                .range(0.1..=100.0)
                                .speed(0.1),
                        );
                    });
                }
                GeoOpKind::PatternFill => {
                    let label = |p: Pattern| match p {
                        Pattern::Lines => "Linien",
                        Pattern::Circles => "Kreise",
                        Pattern::Slots => "Langlöcher",
                        Pattern::Hex => "Waben",
                    };
                    egui::ComboBox::from_label("Muster")
                        .selected_text(label(st.fill.pattern))
                        .show_ui(ui, |ui| {
                            for p in [
                                Pattern::Lines,
                                Pattern::Circles,
                                Pattern::Slots,
                                Pattern::Hex,
                            ] {
                                ui.selectable_value(&mut st.fill.pattern, p, label(p));
                            }
                        });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Abstand X (mm)");
                        ui.add(
                            egui::DragValue::new(&mut st.fill.gap_x)
                                .range(0.1..=100.0)
                                .speed(0.1),
                        );
                        ui.label("Y (mm)");
                        ui.add(
                            egui::DragValue::new(&mut st.fill.gap_y)
                                .range(0.1..=100.0)
                                .speed(0.1),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Winkel (°)");
                        ui.add(
                            egui::DragValue::new(&mut st.fill.angle_deg)
                                .range(-90.0..=90.0)
                                .speed(1.0),
                        );
                    });
                    // Elementgröße nur bei Formen-Mustern (Linien haben keine).
                    let has_size = st.fill.pattern != Pattern::Lines;
                    ui.horizontal(|ui| {
                        ui.label("Elementgröße (mm)");
                        ui.add_enabled(
                            has_size,
                            egui::DragValue::new(&mut st.fill.size)
                                .range(0.1..=100.0)
                                .speed(0.1),
                        );
                    });
                    ui.add_space(2.0);
                    ui.weak("Die Füllung landet auf einem eigenen Layer.");
                }
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Anwenden").clicked() {
                    outcome = DialogOutcome::Commit;
                }
                if ui.button("Abbrechen").clicked() {
                    outcome = DialogOutcome::Cancel;
                }
            });
        });
    outcome
}
