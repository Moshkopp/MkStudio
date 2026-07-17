//! Softwareweite LuxiFer-Einstellungen. Geräteprofile und Controllerdaten
//! liegen bewusst in der separaten Laser-Verwaltung.

use crate::ui::state::{CharonTestStatus, SettingsDialogState, SettingsSection};
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
    CharonTest,
    CharonBackups,
    RestoreBackup(usize),
}

pub(in crate::ui) fn settings_dialog_window(
    root_ui: &mut egui::Ui,
    st: &mut SettingsDialogState,
) -> SettingsOutcome {
    let mut outcome = SettingsOutcome::None;
    let mut open = true;
    egui::Window::new("Einstellungen")
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .fixed_size([660.0, 430.0])
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(root_ui, |ui| {
            let body_height = ui.available_height() - 44.0;
            ui.horizontal(|ui| {
                egui::Frame::new()
                    .fill(ui.visuals().extreme_bg_color)
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(130.0);
                        ui.set_height(body_height - 16.0);
                        ui.vertical(|ui| {
                            for (section, label) in [
                                (SettingsSection::Oberflaeche, "Oberfläche"),
                                (SettingsSection::Charon, "Charon"),
                                (SettingsSection::Ueber, "Über"),
                            ] {
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 26.0],
                                        egui::Button::selectable(st.section == section, label),
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
                        SettingsSection::Charon => "Charon",
                        SettingsSection::Ueber => "Über",
                    });
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt("settings_content")
                        .auto_shrink([false, false])
                        .show(ui, |ui| match st.section {
                            SettingsSection::Oberflaeche => ui_section(ui, st),
                            SettingsSection::Charon => charon_section(ui, st, &mut outcome),
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
    if root_ui.input(|i| i.key_pressed(egui::Key::Escape))
        && root_ui.memory(|m| m.focused().is_none())
    {
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
            ui.label("Auswahlrichtung");
            ui.checkbox(
                &mut s.invert_marquee_direction,
                "Fenster/Kreuz-Auswahl umkehren",
            )
            .on_hover_text("Vertauscht rechts→links und links→rechts samt grüner/roter Anzeige.");
            ui.end_row();
            ui.label("Fensterstart");
            ui.checkbox(&mut s.open_maximized, "Maximiert öffnen")
                .on_hover_text("Wird beim nächsten Programmstart angewendet.");
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

fn charon_section(
    ui: &mut egui::Ui,
    state: &mut SettingsDialogState,
    outcome: &mut SettingsOutcome,
) {
    ui.checkbox(
        &mut state.draft.charon_enabled,
        "Charon-Koordination verwenden",
    );
    ui.add_space(8.0);
    ui.label("Serveradresse");
    ui.add_enabled(
        state.draft.charon_enabled,
        egui::TextEdit::singleline(&mut state.draft.charon_url).desired_width(320.0),
    );
    ui.weak("Der erste Meilenstein unterstützt ausschließlich lokales HTTP.");
    ui.add_space(10.0);
    if ui
        .add_enabled(
            state.draft.charon_enabled,
            egui::Button::new("Verbindung testen"),
        )
        .clicked()
    {
        *outcome = SettingsOutcome::CharonTest;
    }
    ui.add_space(8.0);
    match &state.charon_status {
        CharonTestStatus::Idle => {
            ui.weak("Noch nicht getestet.");
        }
        CharonTestStatus::Syncing(connection) | CharonTestStatus::Connected(connection) => {
            let syncing = matches!(state.charon_status, CharonTestStatus::Syncing(_));
            ui.colored_label(
                if syncing {
                    egui::Color32::from_rgb(0xfb, 0x92, 0x3c)
                } else {
                    egui::Color32::from_rgb(0x34, 0xd3, 0x99)
                },
                format!(
                    "{}: Charon {} · Protokoll {}",
                    if syncing {
                        "Synchronisiert"
                    } else {
                        "Verbunden"
                    },
                    connection.handshake.server_version,
                    connection.handshake.protocol_version
                ),
            );
            ui.weak(format!("Instanz: {}", connection.handshake.instance_id));
            ui.add_space(8.0);
            ui.label("Arbeitsplätze");
            for workplace in &connection.workplaces {
                let (color, status) = if workplace.online {
                    (egui::Color32::from_rgb(0x34, 0xd3, 0x99), "online")
                } else {
                    (ui.visuals().weak_text_color(), "offline")
                };
                ui.horizontal(|ui| {
                    ui.colored_label(color, "⏺");
                    ui.label(&workplace.name);
                    ui.weak(status);
                });
            }
        }
        CharonTestStatus::Failed(message) => {
            ui.colored_label(ui.visuals().error_fg_color, message);
        }
    }
    if let Some(message) = &state.charon_sync_error {
        ui.add_space(6.0);
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("Projekt-Synchronisierung: {message}"),
        );
    }
    ui.add_space(10.0);
    if ui
        .add_enabled(
            state.draft.charon_enabled,
            egui::Button::new("Sicherungen laden"),
        )
        .clicked()
    {
        *outcome = SettingsOutcome::CharonBackups;
    }
    for (index, backup) in state.charon_backups.iter().enumerate() {
        ui.horizontal(|ui| {
            let kind = match backup.kind {
                luxifer_application::CharonBackupKind::UiSettings => "Einstellungen",
                luxifer_application::CharonBackupKind::LaserProfiles => "Laserprofile",
            };
            ui.label(format!("{} · {}", backup.workplace_name, kind));
            if ui.button("Wiederherstellen").clicked() {
                *outcome = SettingsOutcome::RestoreBackup(index);
            }
        });
    }
}
