//! Einstellungen-Dialog mit Sektionen wie das Tauri-Modal: Oberfläche
//! (Arbeitsplatz, Raster, Theme), Laser (Profil-Verwaltung inkl.
//! Scan-Offset-Kalibrierung, ADR 0007) und Über (git-abgeleitete Version).
//! Native hält nur Entwürfe; Klemmen/Persistenz machen Core bzw. LaserService.
//!
//! Layout-Lektion: Das Fenster hat eine FESTE Größe und der Inhalt scrollt
//! innen. Ein `ui.separator()` in einer horizontalen Zeile eines auto-
//! dimensionierten Fensters wächst sonst Frame für Frame mit dem Fenster mit,
//! bis der Dialog größer als der Bildschirm ist und die Fußzeile unerreichbar
//! wird. Zusätzlich schließen Esc und das Titel-✕ den Dialog immer.

use egui::RichText;
use luxifer_core::ui_settings::{GRID_SIZE_MAX, GRID_SIZE_MIN, INTENSITY_MAX, INTENSITY_MIN};
use luxifer_core::{LaserProfile, LaserRegistry, ScanOffsetPoint};

use crate::ui::state::{SettingsDialogState, SettingsSection};

/// Ergebnis des Einstellungen-Dialogs. Eigenes Enum, weil die Laser-Sektion
/// zusätzlich Profil-Aktionen kennt, die den Dialog offen lassen.
#[derive(Debug, Clone, PartialEq, Default)]
pub(in crate::ui) enum SettingsOutcome {
    #[default]
    None,
    /// GUI-Settings übernehmen und Dialog schließen.
    Commit,
    Cancel,
    /// Laser-Profil-Entwurf speichern (Dialog bleibt offen).
    LaserSave,
    /// Laser-Profil mit dieser ID löschen (Dialog bleibt offen).
    LaserDelete(String),
}

pub(in crate::ui) fn settings_dialog_window(
    ctx: &egui::Context,
    st: &mut SettingsDialogState,
    registry: &LaserRegistry,
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
            // Fußzeile (Separator + Buttons) unten fest einplanen.
            let body_height = ui.available_height() - 44.0;
            ui.horizontal(|ui| {
                // Sektions-Navigation links auf eigener, dunklerer Fläche.
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
                                (SettingsSection::Laser, "Laser"),
                                (SettingsSection::Ueber, "Über"),
                            ] {
                                let selected = st.section == section;
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 26.0],
                                        egui::SelectableLabel::new(selected, label),
                                    )
                                    .clicked()
                                {
                                    st.section = section;
                                }
                            }
                        });
                    });

                // Inhalt rechts: Überschrift + scrollender Sektionsinhalt.
                ui.vertical(|ui| {
                    ui.set_height(body_height);
                    let title = match st.section {
                        SettingsSection::Oberflaeche => "Oberfläche",
                        SettingsSection::Laser => "Laser",
                        SettingsSection::Ueber => "Über",
                    };
                    ui.add_space(2.0);
                    ui.heading(title);
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt("settings_content")
                        .auto_shrink([false, false])
                        .show(ui, |ui| match st.section {
                            SettingsSection::Oberflaeche => ui_section(ui, st),
                            SettingsSection::Laser => laser_section(ui, st, registry, &mut outcome),
                            SettingsSection::Ueber => about_section(ui),
                        });
                });
            });

            ui.separator();
            // Fußzeile: Primäraktion rechts außen, Abbrechen daneben.
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
    // Titel-✕ oder Esc (ohne fokussiertes Feld) schließen immer — auch wenn
    // das Layout je kaputt sein sollte, sperrt der modale Dialog nie wieder ein.
    if !open {
        outcome = SettingsOutcome::Cancel;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && ctx.memory(|m| m.focused().is_none()) {
        outcome = SettingsOutcome::Cancel;
    }
    outcome
}

/// Farbe + Intensitäts-Regler einer Theme-Farbe (ADR 0002 §3: Korridor).
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
        });

    ui.add_space(10.0);
    if ui.button("Theme zurücksetzen").clicked() {
        s.theme = Default::default();
    }
}

/// Laser-Sektion: Profil-Karten + Formular des Entwurfs (inkl. Scan-Offset).
fn laser_section(
    ui: &mut egui::Ui,
    st: &mut SettingsDialogState,
    registry: &LaserRegistry,
    outcome: &mut SettingsOutcome,
) {
    if registry.profiles.is_empty() {
        ui.weak("Noch kein Laser angelegt.");
        ui.add_space(4.0);
    }
    for profile in &registry.profiles {
        let is_active = registry.active_id.as_deref() == Some(profile.id.as_str());
        egui::Frame::none()
            .fill(ui.visuals().faint_bg_color)
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&profile.name).strong());
                    ui.weak(format!("· {:?}", profile.kind));
                    if is_active {
                        let accent = ui.visuals().selection.stroke.color;
                        ui.label(RichText::new("aktiv").small().color(accent));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Löschen").clicked() {
                            *outcome = SettingsOutcome::LaserDelete(profile.id.clone());
                        }
                        if ui.button("Bearbeiten").clicked() {
                            st.laser_draft = Some(profile.clone());
                        }
                    });
                });
            });
        ui.add_space(4.0);
    }
    if ui.button("+ Neuer Laser").clicked() {
        st.laser_draft = Some(LaserProfile::default());
    }

    let Some(profile) = st.laser_draft.as_mut() else {
        return;
    };
    ui.add_space(8.0);
    // Verwerfen erst nach dem Closure ausführen — `profile` leiht den Entwurf.
    let mut discard = false;
    egui::Frame::group(ui.style())
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::same(10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let title = if profile.id.is_empty() {
                "Neuer Laser"
            } else {
                "Profil bearbeiten"
            };
            ui.strong(title);
            ui.add_space(6.0);
            laser_profile_form(ui, profile);

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Profil speichern").clicked() {
                    *outcome = SettingsOutcome::LaserSave;
                }
                if ui.button("Verwerfen").clicked() {
                    discard = true;
                }
            });
        });
    if discard {
        st.laser_draft = None;
    }
}

/// Formular eines Laser-Profils: Name, Treiber, Verbindung, Bett und die
/// Scan-Offset-Kalibrierung.
fn laser_profile_form(ui: &mut egui::Ui, profile: &mut LaserProfile) {
    use luxifer_core::{Connection, DriverKind};
    egui::Grid::new("laser_cfg")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut profile.name);
            ui.end_row();

            ui.label("Typ");
            egui::ComboBox::from_id_salt("kind")
                .selected_text(format!("{:?}", profile.kind))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut profile.kind, DriverKind::Ruida, "Ruida");
                    ui.selectable_value(&mut profile.kind, DriverKind::Grbl, "GRBL");
                    ui.selectable_value(&mut profile.kind, DriverKind::MiniGrbl, "miniGRBL");
                });
            ui.end_row();

            // Verbindung: je nach Treiber Netz (IP) oder Seriell (Port).
            match &mut profile.connection {
                Connection::Netz { ip, .. } => {
                    ui.label("IP-Adresse");
                    ui.text_edit_singleline(ip);
                    ui.end_row();
                }
                Connection::Seriell { port, baud } => {
                    ui.label("Port");
                    ui.text_edit_singleline(port);
                    ui.end_row();
                    ui.label("Baud");
                    ui.add(egui::DragValue::new(baud));
                    ui.end_row();
                }
            }

            ui.label("Bett B×H (mm)");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut profile.bed_mm.0).speed(1.0));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut profile.bed_mm.1).speed(1.0));
            });
            ui.end_row();
        });

    // Scan-Offset (Reversal-Korrektur, ADR 0006 §6): Tabelle speed → offset;
    // der Treiber interpoliert linear und extrapoliert über die Ränder.
    ui.add_space(8.0);
    ui.checkbox(
        &mut profile.scan_offset.enabled,
        "Scan-Offset (Reversal-Korrektur) aktiv",
    );
    if profile.scan_offset.enabled {
        ui.weak("Zeilenversatz je Geschwindigkeit — Kanten fransen beim bidirektionalen Rastern sonst aus.");
        ui.add_space(4.0);
        let mut remove: Option<usize> = None;
        egui::Grid::new("scan_offset")
            .num_columns(3)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.weak("Geschwindigkeit");
                ui.weak("Offset");
                ui.label("");
                ui.end_row();
                for (i, pt) in profile.scan_offset.points.iter_mut().enumerate() {
                    ui.add(
                        egui::DragValue::new(&mut pt.speed_mm_s)
                            .range(1.0..=10_000.0)
                            .speed(1.0)
                            .suffix(" mm/s"),
                    );
                    ui.add(
                        egui::DragValue::new(&mut pt.offset_mm)
                            .range(-5.0..=5.0)
                            .speed(0.01)
                            .suffix(" mm"),
                    );
                    if ui.small_button("✕").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        if let Some(i) = remove {
            profile.scan_offset.points.remove(i);
        }
        if ui.button("+ Stützpunkt").clicked() {
            profile.scan_offset.points.push(ScanOffsetPoint {
                speed_mm_s: 100.0,
                offset_mm: 0.1,
            });
        }
    }
}

fn about_section(ui: &mut egui::Ui) {
    ui.strong("LuxiFer");
    ui.weak("Offline-first Laser-Steuerung.");
    ui.add_space(6.0);
    // Version wächst mit jedem Commit (git describe, siehe build.rs).
    ui.label(format!("Version: {}", env!("LUXIFER_VERSION")));
    ui.label(format!("Commit: {}", env!("LUXIFER_COMMIT")));
}
