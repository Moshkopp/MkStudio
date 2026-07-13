//! Projekt-Browser (Reiter „Projekt"): Master-Detail-Ansicht mit Projektliste
//! links und Detailbereich (Metadaten, Vektor-Miniatur, Aktionen, Versionen)
//! rechts.
//!
//! Über die `UiAction`-Grenze (ADR 0011): Das Panel erhält den „Neu"-Namens-
//! entwurf und den Browser-Präsentationszustand als `&mut` (Auswahl, Umbenennen-
//! Entwurf, Lösch-Bestätigungen sind kurzlebige Drafts), die Projektliste und
//! die gecachte Detailsicht als `&`-Sicht, und liefert Absichten zurück. Den
//! Detail-/Vorschau-Cache füllt der Root (`ui/mod.rs`), weil nur er den
//! `ProjectService` kennt.

use egui::RichText;
use luxifer_application::{InboxEntry, InboxStatus};
use luxifer_core::project::ProjectInfo;
use luxifer_core::state::AppState;

use super::action::UiAction;
use super::state::{PreviewImage, PreviewOutline, ProjectBrowserState, ProjectPreview};

/// Baut die Vektor-Miniatur aus einem Projektzustand: sichtbare Konturen in
/// Layer-Farbe, Weltkoordinaten in mm. Reine Präsentationsaufbereitung — die
/// Outline-Ableitung ist dieselbe wie im Canvas (`scene_geo::world_outline`).
pub(crate) fn preview_from_state(state: &AppState) -> ProjectPreview {
    let mut outlines = Vec::new();
    let mut images = Vec::new();
    for shape in &state.shapes {
        let layer = state.layers.get(shape.layer_id);
        if !layer.map(|l| l.visible).unwrap_or(true) {
            continue;
        }
        let color = layer.map(|l| l.color).unwrap_or([200, 200, 200]);
        let (pts, closed) = crate::scene_geo::world_outline(shape);
        if let luxifer_core::Geo::Image { asset, .. } = &shape.geo {
            if let Ok(corners) = pts
                .iter()
                .take(4)
                .map(|&(x, y)| (x as f32, y as f32))
                .collect::<Vec<_>>()
                .try_into()
            {
                images.push(PreviewImage {
                    asset_id: asset.clone(),
                    corners,
                });
            }
        }
        if pts.len() < 2 {
            continue;
        }
        outlines.push(PreviewOutline {
            points: pts.iter().map(|&(x, y)| (x as f32, y as f32)).collect(),
            closed,
            color,
        });
    }
    ProjectPreview {
        bed: (state.bed_w_mm as f32, state.bed_h_mm as f32),
        outlines,
        images,
    }
}

/// Zeichnet die Miniatur in einen festen Rahmen (Bett eingepasst, Y wie im
/// Design-Canvas nach unten).
pub(super) fn draw_preview(
    ui: &mut egui::Ui,
    preview: &ProjectPreview,
    textures: &std::collections::BTreeMap<String, egui::TextureHandle>,
) {
    let height = 180.0_f32.min(ui.available_width() * 0.6);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

    let (bw, bh) = preview.bed;
    if bw <= 0.0 || bh <= 0.0 {
        return;
    }
    let pad = 10.0;
    let s = ((rect.width() - 2.0 * pad) / bw).min((rect.height() - 2.0 * pad) / bh);
    if !s.is_finite() || s <= 0.0 {
        return;
    }
    let origin = rect.center() - egui::vec2(bw, bh) * s * 0.5;
    let to_screen = |x: f32, y: f32| origin + egui::vec2(x * s, y * s);

    // Bettrahmen.
    painter.rect_stroke(
        egui::Rect::from_min_size(origin, egui::vec2(bw * s, bh * s)),
        0.0,
        egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
        egui::StrokeKind::Inside,
    );

    for image in &preview.images {
        let Some(texture) = textures.get(&image.asset_id) else {
            continue;
        };
        let mut mesh = egui::Mesh::with_texture(texture.id());
        let base = mesh.vertices.len() as u32;
        let uvs = [
            egui::pos2(0.0, 0.0),
            egui::pos2(1.0, 0.0),
            egui::pos2(1.0, 1.0),
            egui::pos2(0.0, 1.0),
        ];
        for (corner, uv) in image.corners.iter().zip(uvs) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: to_screen(corner.0, corner.1),
                uv,
                color: egui::Color32::WHITE,
            });
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        painter.add(egui::Shape::mesh(mesh));
    }

    for o in &preview.outlines {
        let mut pts: Vec<egui::Pos2> = o.points.iter().map(|&(x, y)| to_screen(x, y)).collect();
        if o.closed {
            if let Some(&first) = pts.first() {
                pts.push(first);
            }
        }
        let color = egui::Color32::from_rgb(o.color[0], o.color[1], o.color[2]);
        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.0, color)));
    }
}

/// `browser` = Auswahl/Drafts samt vom Root gefülltem Detail-Cache;
/// `projects` = vorhandene Projekte; `open_name` = Name des offenen Projekts;
/// `dirty` = ungespeicherte Änderungen (nur Anzeige).
pub(super) fn project_browser(
    ui: &mut egui::Ui,
    browser: &mut ProjectBrowserState,
    projects: &[ProjectInfo],
    inbox: &[InboxEntry],
    asset_library: (
        &[luxifer_core::AssetMeta],
        &std::collections::BTreeMap<String, egui::TextureHandle>,
    ),
    open_project: (Option<&str>, bool),
    pending: (bool, bool),
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let (assets, asset_thumbnails) = asset_library;
    let (open_name, dirty) = open_project;
    let (integration_pending, asset_import_pending) = pending;

    // Kopfzeile: „Neues Projekt…" öffnet die Maske (Name + Beschreibung);
    // ein „Speichern"-Button ist bewusst weggelassen — Speichern läuft über
    // Strg+S (bzw. Shift+Strg+S für eine neue Version).
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.heading("Projekte");
        if ui
            .selectable_label(
                !browser.show_inbox && !browser.show_assets,
                "Meine Projekte",
            )
            .clicked()
        {
            browser.show_inbox = false;
            browser.show_assets = false;
        }
        let pending = inbox
            .iter()
            .filter(|entry| entry.status == InboxStatus::PendingReview)
            .count();
        let inbox_label = if pending > 0 {
            format!("Von Charon ({pending})")
        } else {
            "Von Charon".into()
        };
        if ui
            .selectable_label(browser.show_inbox, inbox_label)
            .clicked()
        {
            browser.show_inbox = true;
            browser.show_assets = false;
        }
        if ui.selectable_label(browser.show_assets, "Assets").clicked() {
            browser.show_assets = true;
            browser.show_inbox = false;
        }
        if ui.button("Neues Projekt…").clicked() {
            actions.push(UiAction::OpenProjectSaveDialog);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let has_open = open_name.is_some();
            if ui
                .add_enabled(has_open, egui::Button::new("Neue Version"))
                .clicked()
            {
                actions.push(UiAction::SaveProjectVersion);
            }
            if dirty {
                ui.colored_label(ui.visuals().warn_fg_color, "● ungespeichert");
            }
        });
    });
    ui.add_space(8.0);
    ui.separator();

    if browser.show_inbox {
        inbox_pane(ui, inbox, integration_pending, &mut actions);
        return actions;
    }
    if browser.show_assets {
        assets_pane(
            ui,
            browser,
            assets,
            asset_thumbnails,
            asset_import_pending,
            &mut actions,
        );
        return actions;
    }

    // Master-Detail: Liste links, Detailbereich rechts.
    egui::Panel::left("project_list")
        .resizable(true)
        .default_size(260.0)
        .size_range(200.0..=380.0)
        .show(ui, |ui| {
            project_list(ui, browser, projects, open_name, &mut actions);
        });
    egui::CentralPanel::default().show(ui, |ui| {
        detail_pane(ui, browser, asset_thumbnails, open_name, &mut actions);
    });

    actions
}

fn assets_pane(
    ui: &mut egui::Ui,
    browser: &mut ProjectBrowserState,
    assets: &[luxifer_core::AssetMeta],
    thumbnails: &std::collections::BTreeMap<String, egui::TextureHandle>,
    import_pending: bool,
    actions: &mut Vec<UiAction>,
) {
    ui.add_space(10.0);
    ui.heading("Assets");
    ui.weak("Importierte Bilder und originale SVG-/DXF-Dateien stehen projektübergreifend bereit.");
    ui.add_space(8.0);
    ui.add(
        egui::TextEdit::singleline(&mut browser.asset_search)
            .hint_text("Nach Name oder Tag suchen …")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(8.0);

    let query: Vec<String> = browser
        .asset_search
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let reusable: Vec<_> = assets
        .iter()
        .filter(|asset| {
            matches!(
                asset.kind,
                luxifer_core::AssetKind::Image
                    | luxifer_core::AssetKind::SvgSource
                    | luxifer_core::AssetKind::DxfSource
            )
        })
        .filter(|asset| {
            let name = asset.original_name.to_lowercase();
            query
                .iter()
                .all(|term| name.contains(term) || asset.tags.iter().any(|tag| tag.contains(term)))
        })
        .collect();
    if reusable.is_empty() {
        ui.weak("Noch keine wiederverwendbaren Assets importiert.");
        return;
    }

    let card_width = 190.0;
    let columns = ((ui.available_width() + 8.0) / (card_width + 8.0))
        .floor()
        .max(1.0) as usize;
    let row_count = reusable.len().div_ceil(columns);
    egui::ScrollArea::vertical()
        .id_salt("asset_catalog_cards")
        .show_rows(ui, 220.0, row_count, |ui, rows| {
            for row in rows {
                ui.horizontal(|ui| {
                    for column in 0..columns {
                        let index = row * columns + column;
                        let Some(asset) = reusable.get(index) else {
                            break;
                        };
                        let response = egui::Frame::group(ui.style())
                            .show(ui, |ui| {
                                ui.set_width(card_width);
                                ui.set_height(202.0);
                                if let Some(texture) = thumbnails.get(&asset.id) {
                                    ui.add(
                                        egui::Image::new(texture)
                                            .fit_to_exact_size(egui::vec2(174.0, 112.0)),
                                    );
                                } else {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(174.0, 112.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(
                                        rect,
                                        6.0,
                                        ui.visuals().faint_bg_color,
                                    );
                                    actions.push(UiAction::RequestAssetThumbnail(asset.id.clone()));
                                }
                                ui.strong(if asset.original_name.is_empty() {
                                    &asset.id
                                } else {
                                    &asset.original_name
                                });
                                ui.weak(match asset.kind {
                                    luxifer_core::AssetKind::Image => "Bild",
                                    luxifer_core::AssetKind::SvgSource => "SVG",
                                    luxifer_core::AssetKind::DxfSource => "DXF",
                                    luxifer_core::AssetKind::Font => "Font",
                                });
                                if !asset.tags.is_empty() {
                                    ui.weak(egui::RichText::new(asset.tags.join(" · ")).small());
                                }
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(!import_pending, egui::Button::new("Einfügen"))
                                        .clicked()
                                    {
                                        actions
                                            .push(UiAction::ImportCatalogAsset(asset.id.clone()));
                                    }
                                    if browser.confirm_delete_asset.as_deref()
                                        == Some(asset.id.as_str())
                                    {
                                        if ui.button("Ja").clicked() {
                                            browser.confirm_delete_asset = None;
                                            actions.push(UiAction::DeleteCatalogAsset(
                                                asset.id.clone(),
                                            ));
                                        }
                                        if ui.button("Nein").clicked() {
                                            browser.confirm_delete_asset = None;
                                        }
                                    } else if ui
                                        .add_enabled(!import_pending, egui::Button::new("Löschen"))
                                        .clicked()
                                    {
                                        browser.confirm_delete_asset = Some(asset.id.clone());
                                    }
                                });
                            })
                            .response
                            .interact(egui::Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text("Doppelklick zum Einfügen");
                        if response.double_clicked() && !import_pending {
                            actions.push(UiAction::ImportCatalogAsset(asset.id.clone()));
                        }
                    }
                });
            }
        });
}

fn inbox_pane(
    ui: &mut egui::Ui,
    inbox: &[InboxEntry],
    integration_pending: bool,
    actions: &mut Vec<UiAction>,
) {
    ui.add_space(10.0);
    ui.heading("Von Charon");
    ui.weak("Empfangene Revisionen werden erst nach deiner Entscheidung lokal übernommen.");
    ui.add_space(8.0);

    let visible: Vec<_> = inbox
        .iter()
        .filter(|entry| {
            matches!(
                entry.status,
                InboxStatus::PendingReview | InboxStatus::Deferred
            )
        })
        .collect();
    if visible.is_empty() {
        ui.weak("Keine offenen Projektrevisionen.");
        return;
    }

    ui.horizontal(|ui| {
        ui.weak(format!("{} offene Revision(en)", visible.len()));
        if ui
            .add_enabled(
                !integration_pending,
                egui::Button::new(if integration_pending {
                    "Übernehme …"
                } else {
                    "Alle übernehmen"
                }),
            )
            .clicked()
        {
            actions.push(UiAction::ApplyAllInboxRevisions);
        }
    });
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .id_salt("charon_inbox")
        .show(ui, |ui| {
            for entry in visible {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.strong(&entry.project_name);
                        if entry.status == InboxStatus::PendingReview {
                            ui.colored_label(ui.visuals().warn_fg_color, "● neu");
                        } else {
                            ui.weak("später");
                        }
                    });
                    ui.weak(format!(
                        "Von Arbeitsplatz {} · empfangen {}",
                        entry.source_workplace_id, entry.received_at
                    ));
                    ui.weak(format!(
                        "Projektversion {} · Revision {}",
                        short_id(&entry.project_version_id),
                        short_id(&entry.revision_id)
                    ));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!integration_pending, egui::Button::new("Übernehmen"))
                            .clicked()
                        {
                            actions.push(UiAction::ApplyInboxRevision(entry.revision_id.clone()));
                        }
                        if ui.button("Änderungen anzeigen").clicked() {
                            actions.push(UiAction::ShowInboxComparison(entry.revision_id.clone()));
                        }
                        if entry.status == InboxStatus::PendingReview {
                            if ui.button("Später").clicked() {
                                actions
                                    .push(UiAction::DeferInboxRevision(entry.revision_id.clone()));
                            }
                        } else if ui.button("Erneut prüfen").clicked() {
                            actions
                                .push(UiAction::ReconsiderInboxRevision(entry.revision_id.clone()));
                        }
                    });
                });
                ui.add_space(8.0);
            }
        });
}

fn short_id(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

fn project_list(
    ui: &mut egui::Ui,
    browser: &mut ProjectBrowserState,
    projects: &[ProjectInfo],
    open_name: Option<&str>,
    actions: &mut Vec<UiAction>,
) {
    ui.add_space(6.0);
    if projects.is_empty() {
        ui.weak("Noch keine Projekte gespeichert.");
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for p in projects {
            let is_open = open_name == Some(p.name.as_str());
            let is_selected = browser.selected.as_deref() == Some(p.name.as_str());
            let title = if is_open {
                format!("{}  (geöffnet)", p.name)
            } else {
                p.name.clone()
            };
            let label = egui::Button::selectable(is_selected, RichText::new(title).strong());
            let resp = ui.add_sized([ui.available_width(), 20.0], label);
            if !p.modified_at.is_empty() {
                ui.weak(RichText::new(&p.modified_at).small());
            }
            if resp.clicked() && !is_selected {
                browser.selected = Some(p.name.clone());
                // Drafts der vorherigen Auswahl verwerfen.
                browser.rename_draft = None;
                browser.confirm_delete = false;
                browser.confirm_delete_version = None;
            }
            if resp.double_clicked() && !is_open {
                actions.push(UiAction::OpenProject(p.name.clone()));
            }
            ui.add_space(2.0);
            ui.separator();
        }
    });
}

fn detail_pane(
    ui: &mut egui::Ui,
    browser: &mut ProjectBrowserState,
    asset_thumbnails: &std::collections::BTreeMap<String, egui::TextureHandle>,
    open_name: Option<&str>,
    actions: &mut Vec<UiAction>,
) {
    let Some(selected) = browser.selected.clone() else {
        ui.add_space(20.0);
        ui.weak("Links ein Projekt auswählen.");
        return;
    };
    // Cache kurz herausnehmen, damit die Drafts am `browser` frei mutierbar
    // bleiben; am Ende zurücklegen, sofern die Auswahl noch dieselbe ist.
    let Some(cached) = browser.cached.take() else {
        ui.add_space(20.0);
        ui.weak("Lade Projektdetails …");
        return;
    };
    let detail = &cached.detail;
    let is_open = open_name == Some(selected.as_str());

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.heading(&detail.name);
        if is_open {
            ui.label(RichText::new("geöffnet").small().weak());
        }
    });
    if !detail.description.is_empty() {
        ui.label(&detail.description);
    }
    if !detail.tags.is_empty() {
        ui.weak(format!("Tags: {}", detail.tags.join(", ")));
    }
    ui.horizontal(|ui| {
        if !detail.created_at.is_empty() {
            ui.weak(format!("Angelegt: {}", detail.created_at));
        }
        if !detail.modified_at.is_empty() {
            ui.weak(format!("Geändert: {}", detail.modified_at));
        }
    });

    ui.add_space(8.0);
    for image in &cached.preview.images {
        if !asset_thumbnails.contains_key(&image.asset_id) {
            actions.push(UiAction::RequestAssetThumbnail(image.asset_id.clone()));
        }
    }
    draw_preview(ui, &cached.preview, asset_thumbnails);
    ui.add_space(8.0);

    // Aktionszeile.
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!is_open, egui::Button::new("Öffnen"))
            .clicked()
        {
            actions.push(UiAction::OpenProject(selected.clone()));
        }
        if ui.button("Umbenennen…").clicked() {
            browser.rename_draft = Some(detail.name.clone());
        }
        if ui.button("Exportieren…").clicked() {
            actions.push(UiAction::ExportProject(selected.clone()));
        }
        ui.separator();
        // Zweistufiges Löschen, damit ein Fehlklick nichts zerstört.
        if browser.confirm_delete {
            ui.label(RichText::new("Wirklich löschen?").color(ui.visuals().warn_fg_color));
            if ui.button("Ja, löschen").clicked() {
                browser.confirm_delete = false;
                browser.selected = None;
                actions.push(UiAction::DeleteProject(selected.clone()));
            }
            if ui.button("Abbrechen").clicked() {
                browser.confirm_delete = false;
            }
        } else if ui.button("Löschen…").clicked() {
            browser.confirm_delete = true;
        }
    });

    // Umbenennen-Entwurf.
    let mut close_rename = false;
    if let Some(draft) = browser.rename_draft.as_mut() {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Neuer Name:");
            ui.add(egui::TextEdit::singleline(draft).desired_width(200.0));
            if ui.button("Übernehmen").clicked() {
                actions.push(UiAction::RenameProject {
                    from: selected.clone(),
                    to: draft.clone(),
                });
                close_rename = true;
            }
            if ui.button("Abbrechen").clicked() {
                close_rename = true;
            }
        });
    }
    if close_rename {
        browser.rename_draft = None;
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);
    ui.strong(format!("Versionen ({})", detail.versions.len()));
    if !is_open {
        ui.weak("Zum Laden oder Löschen von Versionen das Projekt öffnen.");
    }
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .id_salt("versions")
        .show(ui, |ui| {
            // Neueste zuerst.
            for v in detail.versions.iter().rev() {
                let is_current = v.id == detail.current_version;
                ui.horizontal(|ui| {
                    let label = if is_current {
                        RichText::new(format!("{}  (aktuell)", v.label)).strong()
                    } else {
                        RichText::new(&v.label)
                    };
                    ui.label(label);
                    ui.weak(RichText::new(&v.created_at).small());
                    if !v.note.is_empty() {
                        ui.weak(&v.note);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let can_delete = is_open && detail.versions.len() > 1;
                        if browser.confirm_delete_version.as_deref() == Some(v.id.as_str()) {
                            if ui.button("Ja, löschen").clicked() {
                                browser.confirm_delete_version = None;
                                actions.push(UiAction::DeleteProjectVersion(v.id.clone()));
                            }
                            if ui.button("Abbrechen").clicked() {
                                browser.confirm_delete_version = None;
                            }
                        } else {
                            if ui
                                .add_enabled(can_delete, egui::Button::new("Löschen…"))
                                .clicked()
                            {
                                browser.confirm_delete_version = Some(v.id.clone());
                            }
                            if ui
                                .add_enabled(is_open && !is_current, egui::Button::new("Laden"))
                                .clicked()
                            {
                                actions.push(UiAction::OpenProjectVersion(v.id.clone()));
                            }
                        }
                    });
                });
                ui.separator();
            }
        });

    // Cache zurücklegen, sofern die Auswahl noch dieselbe ist (das Löschen des
    // Projekts hebt sie auf; dann verfällt der Cache bewusst).
    if browser.selected.as_deref() == Some(selected.as_str()) {
        browser.cached = Some(cached);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projektvorschau_behaelt_bild_asset_und_platzierung() {
        let mut state = AppState::default();
        state.add_image("asset-1".into(), 12.0, 18.0, 40.0, 30.0);

        let preview = preview_from_state(&state);

        assert_eq!(preview.images.len(), 1);
        assert_eq!(preview.images[0].asset_id, "asset-1");
        assert_eq!(preview.images[0].corners[0], (12.0, 18.0));
        assert_eq!(preview.images[0].corners[2], (52.0, 48.0));
    }
}
