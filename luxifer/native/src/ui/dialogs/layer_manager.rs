//! Gemeinsamer Layer-Entwurf für den Design-first-Materialworkflow (ADR 0019).

use crate::ui::LayerManagerState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::ui) enum LayerManagerOutcome {
    #[default]
    None,
    Save,
    Cancel,
    LoadMaterial,
    NewMaterial,
    EditMaterial,
}

pub(in crate::ui) fn layer_manager_window(
    root_ui: &egui::Ui,
    state: &mut LayerManagerState,
    laser: Option<&luxifer_core::LaserProfile>,
    materials: &luxifer_core::MaterialLibrary,
    colors: &[[u8; 3]],
) -> LayerManagerOutcome {
    let mut outcome = LayerManagerOutcome::None;
    let screen = root_ui.max_rect().size();
    let max_window = egui::vec2((screen.x - 32.0).max(320.0), (screen.y - 64.0).max(300.0));
    egui::Window::new("Layer verwalten")
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(true)
        .default_size([1050.0_f32.min(max_window.x), 520.0_f32.min(max_window.y)])
        .min_size([720.0_f32.min(max_window.x), 360.0_f32.min(max_window.y)])
        .max_size(max_window)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(root_ui, |ui| {
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(12, 9))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("LASER").small().weak());
                ui.strong(laser.map(|profile| profile.name.as_str()).unwrap_or("Kein Laser"));
                ui.add_space(10.0);
                ui.label(egui::RichText::new("MATERIAL").small().weak());

                let selected = state.material_id.as_deref().and_then(|id| {
                    materials.profiles.iter().find(|profile| profile.id == id)
                });
                egui::ComboBox::from_id_salt("layer_manager_material")
                    .selected_text(
                        selected
                            .map(|profile| profile.display_name())
                            .unwrap_or_else(|| "Kein Material".into()),
                    )
                    .width(190.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(state.material_id.is_none(), "Kein Material")
                            .clicked()
                        {
                            state.material_id = None;
                        }
                        if let Some(laser) = laser {
                            for profile in materials.for_laser(&laser.id) {
                                if ui
                                    .selectable_label(
                                        state.material_id.as_deref() == Some(profile.id.as_str()),
                                        profile.display_name(),
                                    )
                                    .clicked()
                                {
                                    state.material_id = Some(profile.id.clone());
                                }
                            }
                        }
                    });

                if ui
                    .add_enabled(selected.is_some(), egui::Button::new("Materialwerte laden"))
                    .clicked()
                {
                    outcome = LayerManagerOutcome::LoadMaterial;
                }
                if ui
                    .add_enabled(laser.is_some(), egui::Button::new("+ Material"))
                    .clicked()
                {
                    outcome = LayerManagerOutcome::NewMaterial;
                }
                if ui
                    .add_enabled(selected.is_some(), egui::Button::new("Material bearbeiten"))
                    .clicked()
                {
                    outcome = LayerManagerOutcome::EditMaterial;
                }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Geladene Materialwerte bleiben ein Entwurf, bis du alle Layer speicherst.",
                        )
                        .small()
                        .weak(),
                    );
                });
            ui.add_space(10.0);

            // Kopf und Footer bleiben fest sichtbar; nur die Layertabelle
            // nutzt den restlichen Platz und scrollt auf kleineren Displays.
            let table_height = (ui.available_height() - 54.0).max(180.0);
            ui.label(egui::RichText::new("LAYER UND PROZESSWERTE").small().weak());
            ui.add_space(4.0);
            egui::ScrollArea::both()
                .id_salt("layer_manager_table_scroll")
                .auto_shrink([false, false])
                .max_height(table_height)
                .show(ui, |ui| {
                    let table_width = 1_112.0;
                    ui.set_min_width(table_width);
                    table_header(ui);
                    ui.separator();
                    for (index, layer) in state.layers.iter_mut().enumerate() {
                        let fill = if index % 2 == 0 {
                            ui.visuals().faint_bg_color
                        } else {
                            ui.visuals().panel_fill
                        };
                        egui::Frame::new()
                            .fill(fill)
                            .corner_radius(5.0)
                            .inner_margin(egui::Margin::symmetric(8, 7))
                            .show(ui, |ui| {
                                layer_table_row(
                                    ui,
                                    index,
                                    layer,
                                    colors.get(index).copied().unwrap_or_default(),
                                );
                            });
                        ui.add_space(3.0);
                    }
                });

            ui.add_space(6.0);
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Abbrechen").clicked() {
                    outcome = LayerManagerOutcome::Cancel;
                }
                if ui.button("Alle Layer speichern").clicked() {
                    outcome = LayerManagerOutcome::Save;
                }
            });
        });
    outcome
}

const COL_GAP: f32 = 10.0;
const COLOR_W: f32 = 44.0;
const NAME_W: f32 = 160.0;
const PROCESS_W: f32 = 132.0;
const SPEED_W: f32 = 112.0;
const POWER_W: f32 = 94.0;
const PASSES_W: f32 = 90.0;
const AIR_W: f32 = 54.0;
const DETAIL_W: f32 = 210.0;

fn table_header(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = COL_GAP;
        header_cell(ui, "Farbe", COLOR_W);
        header_cell(ui, "Layer", NAME_W);
        header_cell(ui, "Prozess", PROCESS_W);
        header_cell(ui, "Geschwindigkeit", SPEED_W);
        header_cell(ui, "Min. Power", POWER_W);
        header_cell(ui, "Max. Power", POWER_W);
        header_cell(ui, "Durchläufe", PASSES_W);
        header_cell(ui, "Luft", AIR_W);
        header_cell(ui, "Prozessdetail", DETAIL_W);
    });
}

fn header_cell(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.add_sized(
        [width, 24.0],
        egui::Label::new(egui::RichText::new(text).small().strong()),
    );
}

fn layer_table_row(
    ui: &mut egui::Ui,
    index: usize,
    layer: &mut luxifer_application::LayerParams,
    color: [u8; 3],
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = COL_GAP;
        ui.allocate_ui(egui::vec2(COLOR_W, 28.0), |ui| color_swatch(ui, color));
        ui.add_sized([NAME_W, 28.0], egui::TextEdit::singleline(&mut layer.name));
        ui.allocate_ui(egui::vec2(PROCESS_W, 28.0), |ui| {
            mode_combo(ui, index, &mut layer.mode);
        });
        ui.add_sized(
            [SPEED_W, 28.0],
            egui::DragValue::new(&mut layer.speed_mm_s)
                .range(1.0..=10000.0)
                .suffix(" mm/s"),
        );
        ui.add_sized(
            [POWER_W, 28.0],
            egui::DragValue::new(&mut layer.min_power_pct)
                .range(0.0..=100.0)
                .suffix(" %"),
        );
        ui.add_sized(
            [POWER_W, 28.0],
            egui::DragValue::new(&mut layer.power_pct)
                .range(0.0..=100.0)
                .suffix(" %"),
        );
        ui.add_sized(
            [PASSES_W, 28.0],
            egui::DragValue::new(&mut layer.passes).range(1..=100),
        );
        ui.allocate_ui_with_layout(
            egui::vec2(AIR_W, 28.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.checkbox(&mut layer.air_assist, "");
            },
        );
        ui.allocate_ui(egui::vec2(DETAIL_W, 28.0), |ui| {
            process_detail(ui, layer);
        });
    });
}

fn color_swatch(ui: &mut egui::Ui, color: [u8; 3]) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(26.0, 22.0), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        6.0,
        egui::Color32::from_rgb(color[0], color[1], color[2]),
    );
    ui.painter().rect_stroke(
        rect,
        6.0,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
}

fn mode_combo(ui: &mut egui::Ui, index: usize, mode: &mut luxifer_core::LayerMode) {
    if *mode == luxifer_core::LayerMode::Image {
        ui.label("Bildgravur");
        return;
    }
    let label = |mode| match mode {
        luxifer_core::LayerMode::Cut => "Schneiden",
        luxifer_core::LayerMode::Fill => "Gravieren",
        luxifer_core::LayerMode::Raster => "Raster",
        luxifer_core::LayerMode::Image => "Bildgravur",
    };
    egui::ComboBox::from_id_salt(("layer_manager_mode", index))
        .selected_text(label(*mode))
        .width(PROCESS_W - 8.0)
        .show_ui(ui, |ui| {
            for candidate in [
                luxifer_core::LayerMode::Cut,
                luxifer_core::LayerMode::Fill,
                luxifer_core::LayerMode::Raster,
            ] {
                ui.selectable_value(mode, candidate, label(candidate));
            }
        });
}

fn process_detail(ui: &mut egui::Ui, layer: &mut luxifer_application::LayerParams) {
    match layer.mode {
        luxifer_core::LayerMode::Cut => {
            ui.weak("—");
        }
        luxifer_core::LayerMode::Fill => {
            ui.add_sized(
                [DETAIL_W - 8.0, 28.0],
                egui::DragValue::new(&mut layer.line_step_mm)
                    .range(0.01..=10.0)
                    .suffix(" mm Abstand"),
            );
        }
        luxifer_core::LayerMode::Raster | luxifer_core::LayerMode::Image => {
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut layer.dpi)
                        .range(1.0..=2540.0)
                        .suffix(" DPI"),
                );
                ui.checkbox(&mut layer.bidirectional, "Bidirektional");
            });
        }
    }
}
