//! Read-only Vergleich einer lokalen Projektversion mit einer Charon-Revision.

use crate::ui::RevisionComparisonState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum RevisionComparisonOutcome {
    #[default]
    None,
    Close,
    KeepLocal,
    AcceptRemote,
}

pub(in crate::ui) fn revision_comparison_window(
    ctx: &egui::Context,
    state: &RevisionComparisonState,
) -> RevisionComparisonOutcome {
    let mut open = true;
    let mut outcome = RevisionComparisonOutcome::None;
    let screen = ctx.screen_rect().size();
    egui::Window::new("Projektänderungen")
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(true)
        .default_size([screen.x * 0.78, screen.y * 0.72])
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let comparison = &state.comparison;
            ui.heading(&comparison.entry.project_name);
            ui.weak(format!(
                "Revision von Arbeitsplatz {} · empfangen {}",
                comparison.entry.source_workplace_id, comparison.entry.received_at
            ));
            ui.add_space(8.0);

            ui.horizontal_wrapped(|ui| {
                change_badge(ui, "Arbeitsbereich", comparison.bed_changed);
                change_badge(ui, "Ebenen", comparison.layers_changed);
                change_badge(ui, "Objekte", comparison.shapes_changed);
                change_badge(ui, "Metadaten", comparison.metadata_changed);
            });
            ui.separator();

            ui.columns(2, |columns| {
                columns[0].heading("Lokal");
                if let Some(preview) = state.local_preview.as_ref() {
                    columns[0].weak(format!(
                        "{} · geändert {}",
                        comparison
                            .local_project_name
                            .as_deref()
                            .unwrap_or("Projekt"),
                        comparison
                            .local_modified_at
                            .as_deref()
                            .unwrap_or("unbekannt")
                    ));
                    columns[0].label(format!(
                        "{} Ebenen · {} Objekte · {:.0} × {:.0} mm",
                        comparison
                            .local_state
                            .as_ref()
                            .map_or(0, |s| s.layers.len()),
                        comparison
                            .local_state
                            .as_ref()
                            .map_or(0, |s| s.shapes.len()),
                        preview.bed.0,
                        preview.bed.1
                    ));
                    crate::ui::project::draw_preview(&mut columns[0], preview);
                } else {
                    columns[0].weak("Dieses Projekt ist lokal noch nicht vorhanden.");
                }

                columns[1].heading("Von Charon");
                columns[1].weak(format!("geändert {}", comparison.remote_modified_at));
                columns[1].label(format!(
                    "{} Ebenen · {} Objekte · {:.0} × {:.0} mm",
                    comparison.remote_state.layers.len(),
                    comparison.remote_state.shapes.len(),
                    state.remote_preview.bed.0,
                    state.remote_preview.bed.1
                ));
                crate::ui::project::draw_preview(&mut columns[1], &state.remote_preview);
            });

            ui.separator();
            ui.weak("Die Auswahl wird erst nach Betätigung einer Entscheidung angewendet.");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Schließen").clicked() {
                    outcome = RevisionComparisonOutcome::Close;
                }
                if comparison.local_project_name.is_some() {
                    if ui.button("Charon-Version übernehmen").clicked() {
                        outcome = RevisionComparisonOutcome::AcceptRemote;
                    }
                    if ui.button("Lokale Version behalten").clicked() {
                        outcome = RevisionComparisonOutcome::KeepLocal;
                    }
                }
            });
        });
    if !open || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        RevisionComparisonOutcome::Close
    } else {
        outcome
    }
}

fn change_badge(ui: &mut egui::Ui, label: &str, changed: bool) {
    if changed {
        ui.colored_label(ui.visuals().warn_fg_color, format!("● {label} geändert"));
    } else {
        ui.weak(format!("✓ {label} gleich"));
    }
}
