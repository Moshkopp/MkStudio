//! Softwareweite LuxiFer-Einstellungen. Geräteprofile und Controllerdaten
//! liegen bewusst in der separaten Laser-Verwaltung.

use crate::ui::state::{CharonTestStatus, SettingsDialogState, SettingsSection, ShortcutConflict};
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
    let shortcut_input_consumed = capture_shortcut_input(root_ui, st);
    let mut outcome = SettingsOutcome::None;
    let mut open = true;
    egui::Window::new("Einstellungen")
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .fixed_size([760.0, 520.0])
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
                                (SettingsSection::Tastaturkuerzel, "Tastenkürzel"),
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
                        SettingsSection::Tastaturkuerzel => "Tastenkürzel",
                        SettingsSection::Charon => "Charon",
                        SettingsSection::Ueber => "Über",
                    });
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt("settings_content")
                        .auto_shrink([false, false])
                        .show(ui, |ui| match st.section {
                            SettingsSection::Oberflaeche => ui_section(ui, st),
                            SettingsSection::Tastaturkuerzel => shortcuts_section(ui, st),
                            SettingsSection::Charon => charon_section(ui, st, &mut outcome),
                            SettingsSection::Ueber => about_section(ui),
                        });
                });
            });
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let accent = ui.visuals().selection.stroke.color;
                if ui
                    .add_enabled(
                        st.shortcut_recording.is_none()
                            && st.shortcut_conflict.is_none()
                            && st.shortcut_error.is_none(),
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
    shortcut_confirmation_windows(root_ui, st);
    if !shortcut_input_consumed
        && root_ui.input(|i| i.key_pressed(egui::Key::Escape))
        && root_ui.memory(|m| m.focused().is_none())
    {
        outcome = SettingsOutcome::Cancel;
    }
    outcome
}

fn capture_shortcut_input(root_ui: &egui::Ui, st: &mut SettingsDialogState) -> bool {
    let escape = root_ui.input(|input| input.key_pressed(egui::Key::Escape));
    if escape {
        if st.shortcut_recording.take().is_some() {
            st.shortcut_error = None;
            return true;
        }
        if st.shortcut_conflict.take().is_some() {
            return true;
        }
        if st.confirm_shortcut_defaults {
            st.confirm_shortcut_defaults = false;
            return true;
        }
    }
    let Some(recording) = st.shortcut_recording else {
        return false;
    };
    let event = root_ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } if *key != egui::Key::Escape => map_egui_key(*key).map(|key| {
                luxifer_core::ShortcutTrigger::Key(luxifer_core::ShortcutChord {
                    key,
                    ctrl: modifiers.ctrl,
                    shift: modifiers.shift,
                    alt: modifiers.alt,
                })
            }),
            egui::Event::PointerButton {
                button: egui::PointerButton::Secondary,
                pressed: true,
                ..
            } => Some(luxifer_core::ShortcutTrigger::Mouse(
                luxifer_core::ShortcutMouseButton::Right,
            )),
            _ => None,
        })
    });
    let Some(trigger) = event else { return false };
    if let Some(reason) = trigger.reserved_reason() {
        st.shortcut_error = Some(reason.into());
        return true;
    }
    if st
        .draft
        .shortcut_bindings
        .triggers(recording.action)
        .contains(&trigger)
        && recording.replace != Some(trigger)
    {
        st.shortcut_error = Some(format!(
            "{} ist dieser Aktion bereits zugewiesen.",
            trigger.label()
        ));
        return true;
    }
    if let Some(previous_action) = st
        .draft
        .shortcut_bindings
        .conflict(recording.action, trigger)
    {
        st.shortcut_conflict = Some(ShortcutConflict {
            action: recording.action,
            previous_action,
            trigger,
            replace: recording.replace,
        });
        st.shortcut_recording = None;
        st.shortcut_error = None;
        return true;
    }
    apply_recorded_trigger(st, recording.action, recording.replace, trigger);
    true
}

fn apply_recorded_trigger(
    st: &mut SettingsDialogState,
    action: luxifer_core::ShortcutAction,
    replace: Option<luxifer_core::ShortcutTrigger>,
    trigger: luxifer_core::ShortcutTrigger,
) {
    if let Some(previous) = replace {
        if previous != trigger {
            st.draft.shortcut_bindings.remove(action, previous);
        }
    }
    match st.draft.shortcut_bindings.reassign(action, trigger) {
        Ok(()) => {
            st.shortcut_recording = None;
            st.shortcut_error = None;
        }
        Err(message) => st.shortcut_error = Some(message),
    }
}

fn map_egui_key(key: egui::Key) -> Option<luxifer_core::ShortcutKey> {
    use egui::Key as E;
    use luxifer_core::ShortcutKey as K;
    Some(match key {
        E::A => K::A,
        E::B => K::B,
        E::C => K::C,
        E::D => K::D,
        E::E => K::E,
        E::F => K::F,
        E::G => K::G,
        E::H => K::H,
        E::I => K::I,
        E::J => K::J,
        E::K => K::K,
        E::L => K::L,
        E::M => K::M,
        E::N => K::N,
        E::O => K::O,
        E::P => K::P,
        E::Q => K::Q,
        E::R => K::R,
        E::S => K::S,
        E::T => K::T,
        E::U => K::U,
        E::V => K::V,
        E::W => K::W,
        E::X => K::X,
        E::Y => K::Y,
        E::Z => K::Z,
        E::Num0 => K::Num0,
        E::Num1 => K::Num1,
        E::Num2 => K::Num2,
        E::Num3 => K::Num3,
        E::Num4 => K::Num4,
        E::Num5 => K::Num5,
        E::Num6 => K::Num6,
        E::Num7 => K::Num7,
        E::Num8 => K::Num8,
        E::Num9 => K::Num9,
        E::F1 => K::F1,
        E::F2 => K::F2,
        E::F3 => K::F3,
        E::F4 => K::F4,
        E::F5 => K::F5,
        E::F6 => K::F6,
        E::F7 => K::F7,
        E::F8 => K::F8,
        E::F9 => K::F9,
        E::F10 => K::F10,
        E::F11 => K::F11,
        E::F12 => K::F12,
        E::Delete => K::Delete,
        E::Backspace => K::Backspace,
        E::Home => K::Home,
        E::End => K::End,
        E::PageUp => K::PageUp,
        E::PageDown => K::PageDown,
        E::ArrowUp => K::ArrowUp,
        E::ArrowDown => K::ArrowDown,
        E::ArrowLeft => K::ArrowLeft,
        E::ArrowRight => K::ArrowRight,
        E::Enter => K::Enter,
        E::Escape => K::Escape,
        E::Space => K::Space,
        _ => return None,
    })
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
            ui.label("Linienglättung");
            ui.checkbox(&mut s.line_antialiasing, "Analytisches GPU-AA")
                .on_hover_text("Glättet Konturen, Raster und Overlays. Gilt nach Neustart.");
            ui.end_row();
            ui.label("Flächen-MSAA");
            egui::ComboBox::from_id_salt("settings_msaa")
                .selected_text(if s.msaa_samples == 1 {
                    "Aus".to_owned()
                } else {
                    format!("{}×", s.msaa_samples)
                })
                .show_ui(ui, |ui| {
                    for (samples, label) in
                        [(1, "Aus"), (2, "2×"), (4, "4×"), (8, "8×"), (16, "16×")]
                    {
                        ui.selectable_value(&mut s.msaa_samples, samples, label);
                    }
                });
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

fn shortcuts_section(ui: &mut egui::Ui, st: &mut SettingsDialogState) {
    ui.label("Belegungen anklicken, um sie zu ändern.");
    ui.weak("× entfernt eine Belegung. Escape bricht eine laufende Aufnahme ab.");
    ui.add_space(12.0);
    if let Some(message) = &st.shortcut_error {
        ui.colored_label(ui.visuals().error_fg_color, message);
        ui.add_space(6.0);
    }

    let modifiers = ui.input(|input| input.modifiers);
    for category in ["Allgemein", "Bearbeiten", "Werkzeuge", "Ansichten"] {
        ui.strong(category);
        ui.add_space(3.0);
        egui::Grid::new(("shortcut_table", category))
            .num_columns(3)
            .striped(true)
            .min_row_height(30.0)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.weak("Aktion");
                ui.weak("Belegung");
                ui.weak("Ändern");
                ui.end_row();

                for action in luxifer_core::ShortcutAction::ALL
                    .into_iter()
                    .filter(|action| action.category() == category)
                {
                    ui.label(action.label());
                    let triggers = st.draft.shortcut_bindings.triggers(action).to_vec();
                    ui.allocate_ui_with_layout(
                        egui::vec2(210.0, 26.0),
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
                        |ui| {
                            if triggers.is_empty() {
                                ui.weak("Nicht belegt");
                            }
                            for trigger in triggers {
                                let recording_this =
                                    st.shortcut_recording.is_some_and(|recording| {
                                        recording.action == action
                                            && recording.replace == Some(trigger)
                                    });
                                let label = if recording_this {
                                    recording_label(modifiers)
                                } else {
                                    trigger.label()
                                };
                                if ui
                                    .add(egui::Button::new(label).selected(recording_this))
                                    .on_hover_text("Diese Belegung ersetzen")
                                    .clicked()
                                {
                                    st.shortcut_recording =
                                        Some(crate::ui::state::ShortcutRecording {
                                            action,
                                            replace: Some(trigger),
                                        });
                                    st.shortcut_error = None;
                                }
                                if ui
                                    .small_button("×")
                                    .on_hover_text("Diese Belegung entfernen")
                                    .clicked()
                                {
                                    st.draft.shortcut_bindings.remove(action, trigger);
                                    st.shortcut_recording = None;
                                    st.shortcut_error = None;
                                }
                            }
                        },
                    );
                    ui.horizontal(|ui| {
                        let adding = st.shortcut_recording.is_some_and(|recording| {
                            recording.action == action && recording.replace.is_none()
                        });
                        if ui
                            .button(if adding {
                                recording_label(modifiers)
                            } else {
                                "+ Hinzufügen".into()
                            })
                            .clicked()
                        {
                            st.shortcut_recording = Some(crate::ui::state::ShortcutRecording {
                                action,
                                replace: None,
                            });
                            st.shortcut_error = None;
                        }
                        if ui
                            .button("↶ Standard")
                            .on_hover_text("Diese Aktion auf Standard zurücksetzen")
                            .clicked()
                        {
                            st.draft.shortcut_bindings.reset_action(action);
                            st.shortcut_recording = None;
                            st.shortcut_error = None;
                        }
                    });
                    ui.end_row();
                }
            });
        ui.add_space(14.0);
    }
    if ui.button("Standards wiederherstellen").clicked() {
        st.confirm_shortcut_defaults = true;
        st.shortcut_recording = None;
        st.shortcut_error = None;
    }
}

fn recording_label(modifiers: egui::Modifiers) -> String {
    let mut parts = Vec::new();
    if modifiers.ctrl {
        parts.push("Ctrl");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if parts.is_empty() {
        "Tastenkombination drücken …".into()
    } else {
        format!("{} + …", parts.join(" + "))
    }
}

fn shortcut_confirmation_windows(root_ui: &egui::Ui, st: &mut SettingsDialogState) {
    if let Some(conflict) = st.shortcut_conflict {
        egui::Window::new("Tastenkürzel umbelegen?")
            .order(egui::Order::Tooltip)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(root_ui, |ui| {
                ui.label(format!(
                    "{} ist bereits „{}“ zugewiesen. Für „{}“ umbelegen?",
                    conflict.trigger.label(),
                    conflict.previous_action.label(),
                    conflict.action.label(),
                ));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Umbelegen").clicked() {
                        apply_recorded_trigger(
                            st,
                            conflict.action,
                            conflict.replace,
                            conflict.trigger,
                        );
                        st.shortcut_conflict = None;
                    }
                    if ui.button("Abbrechen").clicked() {
                        st.shortcut_conflict = None;
                    }
                });
            });
    }
    if st.confirm_shortcut_defaults {
        egui::Window::new("Shortcut-Standards wiederherstellen?")
            .order(egui::Order::Tooltip)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(root_ui, |ui| {
                ui.label("Alle benutzerdefinierten Tastenkürzel im Entwurf zurücksetzen?");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Standards wiederherstellen").clicked() {
                        st.draft.shortcut_bindings = Default::default();
                        st.confirm_shortcut_defaults = false;
                    }
                    if ui.button("Abbrechen").clicked() {
                        st.confirm_shortcut_defaults = false;
                    }
                });
            });
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
