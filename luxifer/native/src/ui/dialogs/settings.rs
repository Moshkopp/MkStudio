//! Softwareweite LuxiFer-Einstellungen. Geräteprofile und Controllerdaten
//! liegen bewusst in der separaten Laser-Verwaltung.

use crate::ui::state::{SettingsDialogState, SettingsSection};
use egui::RichText;
use luxifer_core::ui_settings::{
    GRID_SIZE_MAX, GRID_SIZE_MIN, INTENSITY_MAX, INTENSITY_MIN, SPLASH_MS_MAX, SPLASH_MS_MIN,
};

#[derive(Debug, Clone, PartialEq, Default)]
pub(in crate::ui) enum SettingsOutcome {
    #[default]
    None,
    Commit,
    Cancel,
}

pub(in crate::ui) fn settings_dialog_window(
    ctx: &egui::Context,
    st: &mut SettingsDialogState,
) -> SettingsOutcome {
    let mut outcome = SettingsOutcome::None;
    let mut open = true;
    egui::Window::new("Einstellungen")
        .collapsible(false)
        .resizable(false)
        .fixed_size([660.0, 430.0])
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let body_height = ui.available_height() - 44.0;
            ui.horizontal(|ui| {
                egui::Frame::none()
                    .fill(ui.visuals().extreme_bg_color)
                    .rounding(egui::Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(8.0))
                    .show(ui, |ui| {
                        ui.set_width(130.0);
                        ui.set_height(body_height - 16.0);
                        ui.vertical(|ui| {
                            for (section, label) in [
                                (SettingsSection::Oberflaeche, "Oberfläche"),
                                (SettingsSection::Ueber, "Über"),
                            ] {
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 26.0],
                                        egui::SelectableLabel::new(st.section == section, label),
                                    )
                                    .clicked()
                                {
                                    st.section = section;
                                }
                            }
                        });
                    });
                ui.vertical(|ui| {
                    ui.set_height(body_height);
                    ui.add_space(2.0);
                    ui.heading(match st.section {
                        SettingsSection::Oberflaeche => "Oberfläche",
                        SettingsSection::Ueber => "Über",
                    });
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt("settings_content")
                        .auto_shrink([false, false])
                        .show(ui, |ui| match st.section {
                            SettingsSection::Oberflaeche => ui_section(ui, st),
                            SettingsSection::Ueber => about_section(ui),
                        });
                });
            });
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let accent = ui.visuals().selection.stroke.color;
                if ui
                    .add(
                        egui::Button::new(RichText::new("Speichern").strong())
                            .fill(accent.gamma_multiply(0.85)),
                    )
                    .clicked()
                {
                    outcome = SettingsOutcome::Commit;
                }
                if ui.button("Abbrechen").clicked() {
                    outcome = SettingsOutcome::Cancel;
                }
            });
        });
    if !open {
        outcome = SettingsOutcome::Cancel;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && ctx.memory(|m| m.focused().is_none()) {
        outcome = SettingsOutcome::Cancel;
    }
    outcome
}

fn theme_color_row(ui: &mut egui::Ui, color: &mut luxifer_core::ThemeColor) {
    ui.horizontal(|ui| {
        ui.color_edit_button_srgb(&mut color.hue);
        ui.add(
            egui::Slider::new(&mut color.intensity, INTENSITY_MIN..=INTENSITY_MAX)
                .show_value(false)
                .text("Intensität"),
        );
    });
}
fn ui_section(ui: &mut egui::Ui, st: &mut SettingsDialogState) {
    let s = &mut st.draft;
    egui::Grid::new("settings_ui")
        .num_columns(2)
        .spacing([12.0, 10.0])
        .show(ui, |ui| {
            ui.label("Arbeitsplatz");
            ui.add(egui::TextEdit::singleline(&mut s.workplace).desired_width(220.0));
            ui.end_row();
            ui.label("Raster (mm)");
            ui.add(
                egui::DragValue::new(&mut s.grid_size_mm)
                    .range(GRID_SIZE_MIN..=GRID_SIZE_MAX)
                    .speed(1.0),
            );
            ui.end_row();
            ui.label("Akzentfarbe");
            theme_color_row(ui, &mut s.theme.accent);
            ui.end_row();
            ui.label("Buttonfarbe");
            theme_color_row(ui, &mut s.theme.button);
            ui.end_row();
            ui.label("Dialog-Hintergrund");
            ui.add(egui::Slider::new(&mut s.modal_backdrop_alpha, 0..=255).text("Abdunklung"));
            ui.end_row();
            ui.label("Splash beim Start");
            ui.checkbox(&mut s.show_splash, "");
            ui.end_row();
            ui.label("Splash-Dauer (ms)");
            ui.add_enabled(
                s.show_splash,
                egui::DragValue::new(&mut s.splash_ms)
                    .range(SPLASH_MS_MIN..=SPLASH_MS_MAX)
                    .speed(50),
            );
            ui.end_row();
        });
    ui.add_space(10.0);
    if ui.button("Theme zurücksetzen").clicked() {
        s.theme = Default::default();
    }
}
fn about_section(ui: &mut egui::Ui) {
    ui.strong("LuxiFer");
    ui.weak("Offline-first Laser-Steuerung.");
    ui.add_space(6.0);
    ui.label(format!("Version: {}", env!("LUXIFER_VERSION")));
    ui.label(format!("Commit: {}", env!("LUXIFER_COMMIT")));
}
