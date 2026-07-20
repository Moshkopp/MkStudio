//! Softwareweite Studio-Einstellungen. Geräteprofile und Controllerdaten
//! liegen bewusst in der separaten Laser-Verwaltung.

use crate::ui::state::{HubTestStatus, SettingsDialogState, SettingsSection, ShortcutConflict};
use egui::RichText;
use studio_core::ui_settings::{
    GRID_SIZE_MAX, GRID_SIZE_MIN, INTENSITY_MAX, INTENSITY_MIN, SPLASH_MS_MAX, SPLASH_MS_MIN,
    UI_SCALE_MAX, UI_SCALE_MIN,
};

#[derive(Debug, Clone, PartialEq, Default)]
pub(in crate::ui) enum SettingsOutcome {
    #[default]
    None,
    Commit,
    Cancel,
    HubTest,
    HubBackups,
    PrepareRestore(usize),
    ConfirmRestore(usize),
    CancelRestore,
}

/// Einzige Quelle der Wahrheit für Reihenfolge und Beschriftung der Sektionen.
/// Sidebar-Navigation und Überschrift lesen daraus, damit sie nie auseinander
/// laufen.
const SECTIONS: &[(SettingsSection, &str)] = &[
    (SettingsSection::Arbeitsplatz, "Arbeitsplatz"),
    (SettingsSection::Editor, "Editor"),
    (SettingsSection::Darstellung, "Darstellung"),
    (SettingsSection::FensterUndStart, "Fenster & Start"),
    (SettingsSection::Tastaturkuerzel, "Tastenkürzel"),
    (SettingsSection::Hub, "Hub"),
    (SettingsSection::Ueber, "Über"),
];

fn section_label(section: SettingsSection) -> &'static str {
    SECTIONS
        .iter()
        .find(|(candidate, _)| *candidate == section)
        .map(|(_, label)| *label)
        .unwrap_or("")
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
                            for (section, label) in SECTIONS {
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 26.0],
                                        egui::Button::selectable(st.section == *section, *label),
                                    )
                                    .clicked()
                                {
                                    st.section = *section;
                                }
                            }
                        });
                    });
                ui.vertical(|ui| {
                    ui.set_height(body_height);
                    ui.add_space(2.0);
                    ui.heading(section_label(st.section));
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt("settings_content")
                        .auto_shrink([false, false])
                        .show(ui, |ui| match st.section {
                            SettingsSection::Arbeitsplatz => workplace_section(ui, st),
                            SettingsSection::Editor => editor_section(ui, st),
                            SettingsSection::Darstellung => appearance_section(ui, st),
                            SettingsSection::FensterUndStart => window_section(ui, st),
                            SettingsSection::Tastaturkuerzel => shortcuts_section(ui, st),
                            SettingsSection::Hub => hub_section(ui, st, &mut outcome),
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
                studio_core::ShortcutTrigger::Key(studio_core::ShortcutChord {
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
            } => Some(studio_core::ShortcutTrigger::Mouse(
                studio_core::ShortcutMouseButton::Right,
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
    action: studio_core::ShortcutAction,
    replace: Option<studio_core::ShortcutTrigger>,
    trigger: studio_core::ShortcutTrigger,
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

fn map_egui_key(key: egui::Key) -> Option<studio_core::ShortcutKey> {
    use egui::Key as E;
    use studio_core::ShortcutKey as K;
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

fn theme_color_row(ui: &mut egui::Ui, color: &mut studio_core::ThemeColor) {
    ui.horizontal(|ui| {
        ui.color_edit_button_srgb(&mut color.hue);
        ui.add(
            egui::Slider::new(&mut color.intensity, INTENSITY_MIN..=INTENSITY_MAX)
                .show_value(false)
                .text("Intensität"),
        );
    });
}
/// Arbeitsplatz-Identität: Name (und alles Weitere, das den Arbeitsplatz als
/// solchen kennzeichnet). Bewusst schlank, damit die Sektion nicht wieder zur
/// Resterampe wird.
fn workplace_section(ui: &mut egui::Ui, st: &mut SettingsDialogState) {
    let s = &mut st.draft;
    egui::Grid::new("settings_workplace")
        .num_columns(2)
        .spacing([12.0, 10.0])
        .show(ui, |ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut s.workplace).desired_width(220.0))
                .on_hover_text("Sichtbarer Name dieses Arbeitsplatzes, z. B. im Hub.");
            ui.end_row();
        });
}

/// Editor-Verhalten beim Zeichnen und Auswählen.
fn editor_section(ui: &mut egui::Ui, st: &mut SettingsDialogState) {
    let s = &mut st.draft;
    egui::Grid::new("settings_editor")
        .num_columns(2)
        .spacing([12.0, 10.0])
        .show(ui, |ui| {
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
            ui.label("Nach dem Zeichnen");
            ui.checkbox(&mut s.select_after_drawing, "Zum Auswahlwerkzeug wechseln")
                .on_hover_text(
                    "Aus: Zeichenwerkzeug bleibt aktiv, die neue Form wird aber abgewählt.",
                );
            ui.end_row();
        });
}

/// Darstellung: Skalierung, Theme-Farben, Dialog-Abdunklung und die
/// GPU-Bildqualität. Alles, was das Aussehen bestimmt.
fn appearance_section(ui: &mut egui::Ui, st: &mut SettingsDialogState) {
    let s = &mut st.draft;
    egui::Grid::new("settings_appearance")
        .num_columns(2)
        .spacing([12.0, 10.0])
        .show(ui, |ui| {
            ui.label("UI-Größe");
            ui.add(
                egui::Slider::new(&mut s.ui_scale, UI_SCALE_MIN..=UI_SCALE_MAX)
                    .custom_formatter(|v, _| format!("{:.0} %", v * 100.0))
                    .custom_parser(|text| {
                        text.trim()
                            .trim_end_matches('%')
                            .trim()
                            .parse::<f64>()
                            .ok()
                            .map(|percent| percent / 100.0)
                    }),
            )
            .on_hover_text(
                "Skalierung der Oberfläche für diesen Arbeitsplatz — \
                 z. B. kleiner auf Full HD, größer auf 4K. Gilt beim Speichern.",
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
        });
    ui.add_space(10.0);
    if ui.button("Theme zurücksetzen").clicked() {
        s.theme = Default::default();
    }
}

/// Fenster-Chrome und Start-Verhalten (Maximieren, Splash).
fn window_section(ui: &mut egui::Ui, st: &mut SettingsDialogState) {
    let s = &mut st.draft;
    egui::Grid::new("settings_window")
        .num_columns(2)
        .spacing([12.0, 10.0])
        .show(ui, |ui| {
            ui.label("Fensterstart");
            ui.checkbox(&mut s.open_maximized, "Maximiert öffnen")
                .on_hover_text("Wird beim nächsten Programmstart angewendet.");
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
}

/// Feste Anzahl Belegungs-Slots pro Aktion. Zwei genügen (kein Standard hat
/// mehr), und die feste Zahl trägt die saubere Tabellenspalten-Optik.
const SHORTCUT_SLOTS: usize = 2;

/// Breite einer Slot-Spalte (Pill). Zwei Slots + Optionen ergeben die Tabelle.
const SLOT_WIDTH: f32 = 130.0;

fn shortcuts_section(ui: &mut egui::Ui, st: &mut SettingsDialogState) {
    ui.label("Belegung anklicken, um sie neu aufzunehmen. Leerer Slot nimmt eine neue auf.");
    ui.weak("× an der Pille entfernt sie. Escape bricht eine laufende Aufnahme ab.");
    ui.add_space(12.0);
    if let Some(message) = &st.shortcut_error {
        ui.colored_label(ui.visuals().error_fg_color, message);
        ui.add_space(6.0);
    }

    let modifiers = ui.input(|input| input.modifiers);
    for category in ["Allgemein", "Bearbeiten", "Werkzeuge", "Ansichten"] {
        ui.add_space(2.0);
        ui.strong(category);
        ui.add_space(4.0);
        egui::Grid::new(("shortcut_table", category))
            .num_columns(4)
            .striped(true)
            .min_row_height(32.0)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                for action in studio_core::ShortcutAction::ALL
                    .into_iter()
                    .filter(|action| action.category() == category)
                {
                    ui.add_sized(
                        [150.0, 24.0],
                        egui::Label::new(action.label()).halign(egui::Align::LEFT),
                    );
                    let triggers = st.draft.shortcut_bindings.triggers(action).to_vec();
                    for slot in 0..SHORTCUT_SLOTS {
                        shortcut_slot(ui, st, action, triggers.get(slot).copied(), modifiers);
                    }
                    // Optionen: nur ein dezenter Reset (das Löschen sitzt jetzt
                    // an der Pill selbst).
                    let differs = st.draft.shortcut_bindings.triggers(action)
                        != studio_core::ShortcutBindings::default().triggers(action);
                    if ui
                        .add_enabled(
                            differs,
                            egui::Button::new(
                                RichText::new("Standard").color(ui.visuals().weak_text_color()),
                            )
                            .small(),
                        )
                        .on_hover_text("Diese Aktion auf ihre Standardbelegung zurücksetzen")
                        .clicked()
                    {
                        st.draft.shortcut_bindings.reset_action(action);
                        st.shortcut_recording = None;
                        st.shortcut_error = None;
                    }
                    ui.end_row();
                }
            });
        ui.add_space(14.0);
    }
    if ui.button("Alle Standards wiederherstellen").clicked() {
        st.confirm_shortcut_defaults = true;
        st.shortcut_recording = None;
        st.shortcut_error = None;
    }
}

/// Ein Belegungs-Slot als Pill. Belegt: klickbare Badge mit Label, beim
/// Überfahren ein × zum Entfernen. Leer: dezente „+ Belegen"-Pille, die eine
/// neue Aufnahme in genau diesen Slot startet. Während der Aufnahme zeigt die
/// Pille live die gedrückten Modifier.
fn shortcut_slot(
    ui: &mut egui::Ui,
    st: &mut SettingsDialogState,
    action: studio_core::ShortcutAction,
    trigger: Option<studio_core::ShortcutTrigger>,
    modifiers: egui::Modifiers,
) {
    let recording_this = st
        .shortcut_recording
        .is_some_and(|recording| recording.action == action && recording.replace == trigger);

    ui.allocate_ui_with_layout(
        egui::vec2(SLOT_WIDTH, 26.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            if recording_this {
                // Aktive Aufnahme: hervorgehobene Pille mit Live-Modifiern.
                let accent = ui.visuals().selection.stroke.color;
                ui.add(
                    egui::Button::new(RichText::new(recording_label(modifiers)).strong())
                        .fill(accent.gamma_multiply(0.85))
                        .corner_radius(egui::CornerRadius::same(11)),
                );
                return;
            }
            match trigger {
                Some(trigger) => {
                    let pill = ui
                        .add(
                            egui::Button::new(RichText::new(trigger.label()).strong())
                                .fill(ui.visuals().widgets.inactive.bg_fill)
                                .corner_radius(egui::CornerRadius::same(11)),
                        )
                        .on_hover_text("Klicken, um diese Belegung neu aufzunehmen");
                    if pill.clicked() {
                        st.shortcut_recording = Some(crate::ui::state::ShortcutRecording {
                            action,
                            replace: Some(trigger),
                        });
                        st.shortcut_error = None;
                    }
                    // × immer im Slot vorhanden (kein Layout-Zappeln), aber nur
                    // beim Überfahren deutlich sichtbar.
                    let over = pill.hovered() || ui.rect_contains_pointer(ui.min_rect());
                    let x_color = if over {
                        ui.visuals().text_color()
                    } else {
                        ui.visuals().weak_text_color().gamma_multiply(0.5)
                    };
                    if ui
                        .add(egui::Button::new(RichText::new("x").color(x_color)).frame(false))
                        .on_hover_text("Diese Belegung entfernen")
                        .clicked()
                    {
                        st.draft.shortcut_bindings.remove(action, trigger);
                        st.shortcut_recording = None;
                        st.shortcut_error = None;
                    }
                }
                None => {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("+ Belegen").color(ui.visuals().weak_text_color()),
                            )
                            .fill(ui.visuals().faint_bg_color)
                            .corner_radius(egui::CornerRadius::same(11)),
                        )
                        .on_hover_text("Neue Belegung für diese Aktion aufnehmen")
                        .clicked()
                    {
                        st.shortcut_recording = Some(crate::ui::state::ShortcutRecording {
                            action,
                            replace: None,
                        });
                        st.shortcut_error = None;
                    }
                }
            }
        },
    );
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
    ui.strong(studio_core::branding::STUDIO_NAME);
    ui.weak("Offline-first Laser-Steuerung.");
    ui.add_space(6.0);
    ui.label(format!("Version: {}", env!("STUDIO_VERSION")));
    ui.label(format!("Commit: {}", env!("STUDIO_COMMIT")));
}

fn hub_section(ui: &mut egui::Ui, state: &mut SettingsDialogState, outcome: &mut SettingsOutcome) {
    ui.checkbox(&mut state.draft.hub_enabled, "Hub-Koordination verwenden");
    ui.add_space(10.0);
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.strong("Verbindung");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(state.draft.hub_enabled, egui::Button::new("Testen"))
                    .clicked()
                {
                    *outcome = SettingsOutcome::HubTest;
                }
            });
        });
        ui.add_space(4.0);
        egui::Grid::new("hub_connection")
            .num_columns(2)
            .spacing([12.0, 5.0])
            .show(ui, |ui| {
                ui.weak("Server");
                ui.add_enabled(
                    state.draft.hub_enabled,
                    egui::TextEdit::singleline(&mut state.draft.hub_url)
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();
                ui.weak("Status");
                hub_connection_status(ui, state);
                ui.end_row();
            });
        ui.add_space(2.0);
        ui.weak("Nur lokales HTTP im vertrauenswürdigen internen Netzwerk.");
    });
    if let Some(message) = &state.hub_sync_error {
        ui.add_space(6.0);
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("Projekt-Synchronisierung: {message}"),
        );
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.strong("Sicherungen");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    state.draft.hub_enabled,
                    egui::Button::new("Backups anzeigen"),
                )
                .clicked()
            {
                *outcome = SettingsOutcome::HubBackups;
            }
        });
    });
    ui.weak("Automatische Stände dieses und anderer Arbeitsplätze.");
    ui.add_space(5.0);
    let mut groups: std::collections::BTreeMap<_, std::collections::BTreeMap<_, Vec<_>>> =
        std::collections::BTreeMap::new();
    for (index, backup) in state.hub_backups.iter().enumerate() {
        groups
            .entry(backup.workplace_name.clone())
            .or_default()
            .entry(backup.kind)
            .or_default()
            .push(index);
    }
    if groups.is_empty() {
        ui.weak("Noch keine Sicherungen geladen.");
    }
    for (workplace, kinds) in groups {
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.strong(&workplace);
            ui.add_space(2.0);
            for (kind, indices) in kinds {
                backup_row(ui, state, outcome, &workplace, kind, &indices);
            }
        });
    }
    if let Some(confirm) = &state.backup_restore_confirm {
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Diesen Sicherungsstand wirklich wiederherstellen?",
            );
            ui.weak("Der aktuelle lokale Stand wird vorher automatisch gesichert.");
            for line in &confirm.summary {
                ui.label(line);
            }
            ui.horizontal(|ui| {
                if ui.button("Wiederherstellen").clicked() {
                    *outcome = SettingsOutcome::ConfirmRestore(confirm.index);
                }
                if ui.button("Abbrechen").clicked() {
                    *outcome = SettingsOutcome::CancelRestore;
                }
            });
        });
    }
}

fn hub_connection_status(ui: &mut egui::Ui, state: &SettingsDialogState) {
    match &state.hub_status {
        HubTestStatus::Idle => {
            ui.weak("Noch nicht getestet");
        }
        HubTestStatus::Syncing(connection) | HubTestStatus::Connected(connection) => {
            let syncing = matches!(state.hub_status, HubTestStatus::Syncing(_));
            let color = if syncing {
                egui::Color32::from_rgb(0xfb, 0x92, 0x3c)
            } else {
                egui::Color32::from_rgb(0x34, 0xd3, 0x99)
            };
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(color, "●");
                ui.colored_label(
                    color,
                    format!(
                        "Hub {} · Protokoll {}",
                        connection.handshake.server_version, connection.handshake.protocol_version
                    ),
                );
                ui.weak(format!("· {}", connection.handshake.instance_id));
                for workplace in &connection.workplaces {
                    ui.weak(format!(
                        "· {} {}",
                        workplace.name,
                        if workplace.online {
                            "online"
                        } else {
                            "offline"
                        }
                    ));
                }
            });
        }
        HubTestStatus::Failed(message) => {
            ui.colored_label(ui.visuals().error_fg_color, message);
        }
    }
}

fn backup_row(
    ui: &mut egui::Ui,
    state: &SettingsDialogState,
    outcome: &mut SettingsOutcome,
    workplace: &str,
    kind: studio_application::HubBackupKind,
    indices: &[usize],
) {
    let latest = &state.hub_backups[indices[0]];
    ui.horizontal(|ui| {
        ui.add_sized([145.0, 24.0], egui::Label::new(backup_kind_label(kind)));
        ui.add_sized(
            [95.0, 24.0],
            egui::Label::new(
                egui::RichText::new(format_backup_age(latest.saved_at_unix))
                    .color(ui.visuals().weak_text_color()),
            ),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Wiederherstellen").clicked() {
                *outcome = SettingsOutcome::PrepareRestore(indices[0]);
            }
        });
    });
    if indices.len() > 1 {
        ui.indent(("backup_indent", workplace, kind), |ui| {
            egui::CollapsingHeader::new(format!("{} ältere Stände", indices.len() - 1))
                .id_salt(("backup_history", workplace, kind))
                .show(ui, |ui| {
                    for &index in indices.iter().skip(1) {
                        let backup = &state.hub_backups[index];
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [120.0, 22.0],
                                egui::Label::new(
                                    egui::RichText::new(format_backup_age(backup.saved_at_unix))
                                        .color(ui.visuals().weak_text_color()),
                                ),
                            );
                            if ui.small_button("Wiederherstellen").clicked() {
                                *outcome = SettingsOutcome::PrepareRestore(index);
                            }
                        });
                    }
                });
        });
    }
}

fn backup_kind_label(kind: studio_application::HubBackupKind) -> &'static str {
    match kind {
        studio_application::HubBackupKind::UiSettings => "Einstellungen",
        studio_application::HubBackupKind::LaserProfiles => "Laserprofile",
        studio_application::HubBackupKind::MaterialProfiles => "Materialprofile",
    }
}

fn format_backup_age(saved_at_unix: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let age = now.saturating_sub(saved_at_unix);
    match age {
        0..=59 => "gerade eben".into(),
        60..=3_599 => format!("vor {} min", age / 60),
        3_600..=86_399 => format!("vor {} h", age / 3_600),
        _ => format!("vor {} Tagen", age / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::state::SettingsSection;

    fn dialog_state() -> SettingsDialogState {
        SettingsDialogState {
            draft: studio_core::UiSettings::default(),
            section: SettingsSection::Tastaturkuerzel,
            hub_status: HubTestStatus::Idle,
            hub_sync_error: None,
            hub_backups: Vec::new(),
            backup_restore_confirm: None,
            shortcut_recording: None,
            shortcut_conflict: None,
            shortcut_error: None,
            confirm_shortcut_defaults: false,
        }
    }

    /// Alle Texte eines Frames rekursiv einsammeln.
    fn frame_texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(t) => out.push(t.galley.job.text.clone()),
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for c in shapes {
            walk(&c.shape, &mut out);
        }
        out
    }

    fn render(st: &mut SettingsDialogState) -> Vec<String> {
        let ctx = egui::Context::default();
        let style = crate::ui::theme_style(&studio_core::Theme::default());
        ctx.all_styles_mut(|s| *s = style.clone());
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(760.0, 900.0),
            )),
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                shortcuts_section(ui, st);
            });
        });
        frame_texts(&out.shapes)
    }

    /// Die Standardbelegungen erscheinen als Pills, die leeren zweiten Slots
    /// als „+ Belegen", und pro Aktion gibt es einen „Standard"-Reset. Sichert
    /// zugleich, dass keine der Beschriftungen leer ist (kaputte Glyphe).
    #[test]
    fn tastenkuerzel_zeigt_pills_und_leere_slots() {
        let mut st = dialog_state();
        let texts = render(&mut st);

        // Aktionslabels und Standardbelegungen sind sichtbar.
        assert!(texts.iter().any(|t| t == "Rückgängig"));
        assert!(texts.iter().any(|t| t == "Ctrl+Z"));
        assert!(texts.iter().any(|t| t == "Ctrl+Shift+Z"));
        assert!(texts.iter().any(|t| t == "Ctrl+Y"));

        // Aktionen mit nur einer Belegung zeigen im zweiten Slot „+ Belegen".
        assert!(texts.iter().any(|t| t == "+ Belegen"));

        // Reset-Option je Zeile.
        assert!(texts.iter().any(|t| t == "Standard"));

        // Keine leeren Beschriftungen (Indikator für nicht renderbare Glyphen).
        assert!(texts.iter().all(|t| !t.is_empty()));
    }

    /// Ein Klick auf einen leeren Slot startet die Aufnahme genau für diesen
    /// Slot (replace = None), damit der nächste Tastendruck dort landet.
    #[test]
    fn klick_auf_leeren_slot_startet_aufnahme() {
        let mut st = dialog_state();
        // „Gruppieren" hat nur eine Belegung (G) → zweiter Slot ist „+ Belegen".
        let ctx = egui::Context::default();
        let style = crate::ui::theme_style(&studio_core::Theme::default());
        ctx.all_styles_mut(|s| *s = style.clone());

        // Position des ersten „+ Belegen" bestimmen.
        let find_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(760.0, 900.0),
            )),
            ..Default::default()
        };
        let out = ctx.run_ui(find_input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                shortcuts_section(ui, &mut st);
            });
        });
        let pos = out
            .shapes
            .iter()
            .find_map(|c| match &c.shape {
                egui::epaint::Shape::Text(t) if t.galley.job.text == "+ Belegen" => Some(t.pos),
                egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| match s {
                    egui::epaint::Shape::Text(t) if t.galley.job.text == "+ Belegen" => Some(t.pos),
                    _ => None,
                }),
                _ => None,
            })
            .expect("+ Belegen nicht gefunden");

        let click = |ctx: &egui::Context, st: &mut SettingsDialogState, events| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(760.0, 900.0),
                )),
                events,
                ..Default::default()
            };
            // Der FullOutput interessiert hier nicht: geprüft wird allein die
            // Zustandsänderung in `st`, nicht das Rendering-Ergebnis.
            let _ = ctx.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    shortcuts_section(ui, st);
                });
            });
        };
        let at = pos + egui::vec2(4.0, 4.0);
        click(
            &ctx,
            &mut st,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
        );

        let recording = st.shortcut_recording.expect("Aufnahme nicht gestartet");
        assert_eq!(
            recording.replace, None,
            "leerer Slot fügt hinzu, ersetzt nicht"
        );
    }
}
