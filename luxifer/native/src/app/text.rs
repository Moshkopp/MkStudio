//! Nativer Text-Workflow: Dialog-Draft, Font-Auflösung (Familie/Schnitt),
//! Live-Vorschau und Übergabe der erzeugten Konturen an die Application-Session.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use super::App;
use crate::ui::TextDialogState;
use luxifer_application::AssetService;
use luxifer_core::text::{layout_text, TextOptions};

impl App {
    pub fn open_text_dialog(&mut self) {
        self.ensure_fonts();
        let mut state = TextDialogState::default();
        if let Some(fam) = self.fonts.first() {
            state.family_idx = Some(0);
            state.face_idx = fam.default_face();
        }
        self.text_dialog = Some(state);
    }

    pub fn open_text_editor(&mut self, index: usize) {
        self.ensure_fonts();
        let Some(meta) = self
            .session
            .state()
            .shapes
            .get(index)
            .and_then(|shape| shape.text_meta.clone())
        else {
            return;
        };
        let (family_idx, face_idx) = self
            .find_font_by_path(&meta.font_path)
            .or_else(|| self.fonts.first().map(|fam| (0, fam.default_face())))
            .map(|(fam, face)| (Some(fam), face))
            .unwrap_or((None, 0));
        self.text_dialog = Some(TextDialogState {
            text: meta.text,
            size_mm: meta.size_mm,
            align: meta.align,
            line_spacing: meta.line_spacing,
            letter_spacing_mm: meta.letter_spacing_mm,
            family_idx,
            face_idx,
            edit_index: Some(index),
            ..TextDialogState::default()
        });
    }

    /// Font-Liste lazy laden (einmaliger Verzeichnis-Scan, danach gecacht).
    fn ensure_fonts(&mut self) {
        if self.fonts.is_empty() {
            self.fonts = crate::fonts::list_font_families();
        }
    }

    /// (Familien-, Schnitt-Index) des Fonts mit diesem Pfad.
    fn find_font_by_path(&self, path: &str) -> Option<(usize, usize)> {
        self.fonts.iter().enumerate().find_map(|(fi, fam)| {
            fam.faces
                .iter()
                .position(|face| face.path.to_string_lossy() == path)
                .map(|si| (fi, si))
        })
    }

    /// Pfad des im Dialog gewählten Schnitts.
    fn selected_font_path(&self) -> Option<PathBuf> {
        let state = self.text_dialog.as_ref()?;
        let fam = self.fonts.get(state.family_idx?)?;
        let face = fam
            .faces
            .get(state.face_idx)
            .or_else(|| fam.faces.first())?;
        Some(face.path.clone())
    }

    /// Font-Bytes mit Cache — verhindert, dass Vorschau und Commit dieselbe
    /// Datei wiederholt von Platte lesen.
    fn font_bytes(&mut self, path: &PathBuf) -> Result<Arc<Vec<u8>>, String> {
        if let Some(bytes) = self.font_cache.get(path) {
            return Ok(bytes.clone());
        }
        let bytes = Arc::new(std::fs::read(path).map_err(|e| format!("Font lesen: {e}"))?);
        self.font_cache.insert(path.clone(), bytes.clone());
        Ok(bytes)
    }

    /// Importiert eine Font-Datei (TTF/OTF) in den Asset-Katalog und wählt sie
    /// im offenen Text-Dialog aus. Der Katalog liegt vor den System-Fonts,
    /// damit importierte Fonts auch ohne Systeminstallation verfügbar bleiben.
    pub fn import_font_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Font (TTF/OTF)", &["ttf", "otf"])
            .pick_file()
        else {
            return;
        };
        let meta = match AssetService::import_font(&path) {
            Ok(meta) => meta,
            Err(error) => {
                self.app_error = Some(error);
                return;
            }
        };
        self.refresh_asset_catalog();
        // Liste neu aufbauen und den frisch importierten Schnitt (über seinen
        // Katalog-Pfad) direkt auswählen.
        self.fonts = crate::fonts::list_font_families();
        let asset_path = AssetService::asset_path(&meta.id);
        let found = asset_path
            .as_ref()
            .and_then(|p| self.find_font_by_path(&p.to_string_lossy()));
        let family_name = found
            .map(|(fi, _)| self.fonts[fi].name.clone())
            .unwrap_or_else(|| meta.original_name.clone());
        if let (Some(state), Some((fi, si))) = (self.text_dialog.as_mut(), found) {
            state.family_idx = Some(fi);
            state.face_idx = si;
            state.search.clear();
        }
        self.toasts
            .success(format!("Font „{family_name}“ importiert"));
    }

    /// Layout-Parameter aus dem Dialog-Entwurf.
    fn dialog_options(state: &TextDialogState) -> TextOptions {
        TextOptions {
            size_mm: state.size_mm,
            align: state.align,
            line_spacing: state.line_spacing,
            letter_spacing_mm: state.letter_spacing_mm,
        }
    }

    /// Berechnet die Live-Vorschau des Text-Dialogs neu, wenn sich der Entwurf
    /// (Text, Font, Layout) geändert hat. Läuft vor dem Zeichnen des Dialogs;
    /// der Dialog selbst zeichnet nur die gecachten Konturen.
    pub fn update_text_preview(&mut self) {
        let Some(path) = self.selected_font_path() else {
            if let Some(state) = self.text_dialog.as_mut() {
                state.preview.clear();
                state.preview_key = None;
            }
            return;
        };
        let Some(state) = self.text_dialog.as_ref() else {
            return;
        };
        let mut hasher = DefaultHasher::new();
        state.text.hash(&mut hasher);
        path.hash(&mut hasher);
        state.size_mm.to_bits().hash(&mut hasher);
        state.line_spacing.to_bits().hash(&mut hasher);
        state.letter_spacing_mm.to_bits().hash(&mut hasher);
        (state.align as u8).hash(&mut hasher);
        let key = hasher.finish();
        if state.preview_key == Some(key) {
            return;
        }
        let opts = Self::dialog_options(state);
        let text = state.text.clone();
        let contours = self
            .font_bytes(&path)
            .ok()
            .and_then(|bytes| layout_text(&bytes, &text, &opts).ok())
            .unwrap_or_default();
        if let Some(state) = self.text_dialog.as_mut() {
            state.preview = contours;
            state.preview_key = Some(key);
        }
    }

    pub fn commit_text(&mut self) -> bool {
        let Some(font_path) = self.selected_font_path() else {
            self.toasts.error("Kein Font gewählt");
            return false;
        };
        let Some(state) = self.text_dialog.as_ref() else {
            return false;
        };
        let opts = Self::dialog_options(state);
        let (text, edit_index) = (state.text.clone(), state.edit_index);
        let font_data = match self.font_bytes(&font_path) {
            Ok(data) => data,
            Err(error) => {
                self.toasts.error(error);
                return false;
            }
        };
        let font_asset = match AssetService::catalog_font(&font_path, &font_data) {
            Ok(meta) => Some(meta.id),
            Err(error) => {
                self.app_error = Some(error);
                return false;
            }
        };
        self.refresh_asset_catalog();
        match layout_text(&font_data, &text, &opts) {
            Ok(contours) if !contours.is_empty() => {
                let meta = luxifer_core::TextMeta {
                    text,
                    font_path: font_path.to_string_lossy().to_string(),
                    font_asset,
                    size_mm: opts.size_mm,
                    align: opts.align,
                    line_spacing: opts.line_spacing,
                    letter_spacing_mm: opts.letter_spacing_mm,
                };
                if let Some(index) = edit_index {
                    // Edit behält den alten Anker (replace_text_block).
                    if let Err(error) = self.session.replace_text_block(index, contours, meta) {
                        self.app_error = Some(error);
                        return false;
                    }
                } else {
                    // Neuer Text landet mittig in der aktuellen Ansicht —
                    // ohne fit_all, damit Zoom und Ausschnitt stehen bleiben.
                    let contours = center_contours_at(contours, self.canvas.cam.center);
                    self.session.selected = self.session.add_text_block(contours, meta);
                }
                self.refresh_accent();
                true
            }
            Ok(_) => {
                self.toasts.error("Text ergab keine Konturen");
                false
            }
            Err(error) => {
                self.toasts.error(format!("Text-Fehler: {error}"));
                false
            }
        }
    }
}

/// Verschiebt die Konturen so, dass ihr BBox-Zentrum auf `center` (Welt-mm)
/// liegt.
fn center_contours_at(
    contours: Vec<(Vec<luxifer_core::Pt>, bool)>,
    center: [f32; 2],
) -> Vec<(Vec<luxifer_core::Pt>, bool)> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (pts, _) in &contours {
        for &(x, y) in pts {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x0 > x1 {
        return contours;
    }
    let dx = center[0] as f64 - (x0 + x1) / 2.0;
    let dy = center[1] as f64 - (y0 + y1) / 2.0;
    contours
        .into_iter()
        .map(|(pts, closed)| {
            (
                pts.into_iter().map(|(x, y)| (x + dx, y + dy)).collect(),
                closed,
            )
        })
        .collect()
}
