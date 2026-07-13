use std::path::Path;

use luxifer_core::Geo;

use super::App;
use crate::ui::ImageDialogState;

impl App {
    /// Öffnet den Bildparameter-Dialog mit den aktuellen Werten des Bild-Shapes.
    pub fn open_image_dialog(&mut self, index: usize) {
        if let Some(Geo::Image { params, .. }) =
            self.session.state().shapes.get(index).map(|s| &s.geo)
        {
            self.image_dialog = Some(ImageDialogState::new(index, *params));
        }
    }

    /// Übernimmt den Bildparameter-Entwurf über die Session (validiert, ein
    /// Undo-Schritt). Erfolg → Dialog schließen; Fehler → offen + Fehlerkanal.
    pub fn commit_image_dialog(&mut self) -> bool {
        let Some(st) = self.image_dialog.as_ref() else {
            return false;
        };
        let (index, params) = (st.index, st.params);
        match self.session.set_image_params(index, params) {
            Ok(()) => {
                self.image_dirty = true;
                true
            }
            Err(error) => {
                self.app_error = Some(error);
                false
            }
        }
    }

    /// Vektorisiert das Bild des offenen Dialogs (Trace) über die Session.
    /// Der Dialog bleibt offen; jeder Lauf ist ein eigener Undo-Schritt.
    pub fn trace_image_dialog(&mut self) {
        let Some(st) = self.image_dialog.as_ref() else {
            return;
        };
        let (index, threshold, invert) = (st.index, st.trace_threshold, st.trace_invert);
        match self.session.trace_image(index, threshold, invert) {
            Ok(indices) => {
                self.refresh_accent();
                self.toasts
                    .success(format!("{} Konturen erzeugt.", indices.len()));
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    /// Öffnet einen nativen Datei-Dialog für Vektor (SVG/DXF) und Bild;
    /// `import_path` verzweigt nach Endung.
    pub fn import_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Importierbar (Vektor + Bild)",
                &["svg", "dxf", "png", "jpg", "jpeg", "bmp", "gif", "webp"],
            )
            .add_filter("Vektor", &["svg", "dxf"])
            .add_filter("Bild", &["png", "jpg", "jpeg", "bmp", "gif", "webp"])
            .pick_file()
        {
            self.import_path(&path);
        }
    }

    /// Importiert eine Datei direkt nach Endung — Vektor (SVG/DXF) oder Bild.
    /// Nutzen der Import-Dialog und das CLI-Argument.
    pub fn import_path(&mut self, path: &Path) {
        let ext = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp"
        ) {
            self.import_image_path(path);
            return;
        }

        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                log::error!("Datei lesen: {error}");
                return;
            }
        };
        match luxifer_core::import::import_vector(&bytes, &ext) {
            Ok(contours) => {
                let started = std::time::Instant::now();
                self.session.add_polylines(contours);
                self.refresh_accent();
                self.fit_all();
                log::info!(
                    "Import {}: {} Shapes in {:?}",
                    path.display(),
                    self.session.shapes.len(),
                    started.elapsed()
                );
            }
            Err(error) => log::error!("Import fehlgeschlagen: {error}"),
        }
    }

    /// Bilddatei in den Asset-Store importieren und als Image-Shape platzieren.
    fn import_image_path(&mut self, path: &Path) {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                log::error!("Bild lesen: {error}");
                return;
            }
        };
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("bild")
            .to_string();
        match luxifer_core::import_image(&luxifer_core::assets_dir(), &bytes, &name) {
            Ok(meta) => {
                // Pixel → mm bei 254 DPI (10 px/mm), wie der Core-Default.
                let width_mm = meta.width as f64 / 10.0;
                let height_mm = meta.height as f64 / 10.0;
                let index =
                    self.session
                        .add_image(meta.id.clone(), 20.0, 20.0, width_mm, height_mm);
                self.session.selected = vec![index];
                self.image_dirty = true;
                self.fit_all();
                log::info!(
                    "Bild importiert: {} ({}×{})",
                    meta.id,
                    meta.width,
                    meta.height
                );
            }
            Err(error) => log::error!("Bild-Import fehlgeschlagen: {error}"),
        }
    }
}
