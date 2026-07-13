use std::path::Path;

use luxifer_core::Geo;

use super::App;
use crate::ui::ImageDialogState;

pub(super) struct ThumbnailRuntime {
    request_tx: std::sync::mpsc::Sender<String>,
    result_rx: std::sync::mpsc::Receiver<(String, Result<Vec<u8>, String>)>,
}

type ImportedContours = Vec<(Vec<(f64, f64)>, bool)>;
type AssetImportResult = Result<(luxifer_core::AssetMeta, Option<ImportedContours>), String>;

pub(super) struct AssetImportRuntime {
    request_tx: std::sync::mpsc::Sender<String>,
    result_rx: std::sync::mpsc::Receiver<AssetImportResult>,
}

impl AssetImportRuntime {
    pub fn new() -> Self {
        let (request_tx, request_rx) = std::sync::mpsc::channel::<String>();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("asset-import".into())
            .spawn(move || {
                let store = luxifer_core::assets_dir();
                while let Ok(id) = request_rx.recv() {
                    let result = (|| {
                        let meta = luxifer_core::asset_meta(&store, &id)
                            .map_err(|error| error.to_string())?;
                        let contours = match meta.kind {
                            luxifer_core::AssetKind::SvgSource
                            | luxifer_core::AssetKind::DxfSource => {
                                let bytes = luxifer_core::load_asset(&store, &id)
                                    .map_err(|error| error.to_string())?;
                                Some(
                                    luxifer_core::import::import_vector(&bytes, &meta.ext)
                                        .map_err(|error| error.to_string())?,
                                )
                            }
                            _ => None,
                        };
                        Ok((meta, contours))
                    })();
                    if result_tx.send(result).is_err() {
                        return;
                    }
                }
            })
            .expect("Asset-Importworker konnte nicht gestartet werden");
        Self {
            request_tx,
            result_rx,
        }
    }
}

impl ThumbnailRuntime {
    pub fn new() -> Self {
        let (request_tx, request_rx) = std::sync::mpsc::channel::<String>();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("asset-thumbnails".into())
            .spawn(move || {
                let store = luxifer_core::assets_dir();
                while let Ok(id) = request_rx.recv() {
                    let result = luxifer_core::asset_thumbnail(&store, &id)
                        .map_err(|error| error.to_string());
                    if result_tx.send((id, result)).is_err() {
                        return;
                    }
                }
            })
            .expect("Thumbnail-Hintergrundthread konnte nicht gestartet werden");
        Self {
            request_tx,
            result_rx,
        }
    }
}

pub(super) fn enrich_asset_tags_from_projects() {
    let projects = luxifer_core::projects_dir();
    let store = luxifer_core::assets_dir();
    for info in luxifer_core::list_projects(&projects) {
        let Ok(project) = luxifer_core::ProjectFile::load_by_name(&projects, &info.name) else {
            continue;
        };
        for id in &project.asset_refs {
            let _ = luxifer_core::add_asset_tags(
                &store,
                id,
                [project.name.as_str(), project.description.as_str()],
            );
        }
    }
}

impl App {
    pub(crate) fn refresh_asset_catalog(&mut self) {
        match luxifer_core::list_assets(&luxifer_core::assets_dir()) {
            Ok(assets) => {
                self.asset_thumbnails
                    .retain(|id, _| assets.iter().any(|asset| asset.id == *id));
                self.thumbnail_failed
                    .retain(|id| assets.iter().any(|asset| asset.id == *id));
                self.thumbnail_pending
                    .retain(|id| assets.iter().any(|asset| asset.id == *id));
                self.asset_catalog = assets;
            }
            Err(error) => log::error!("Asset-Katalog aktualisieren: {error}"),
        }
    }

    pub fn request_asset_thumbnail(&mut self, id: &str) {
        if self.asset_thumbnails.contains_key(id)
            || self.thumbnail_pending.contains(id)
            || self.thumbnail_failed.contains(id)
        {
            return;
        }
        if self
            .thumbnail_runtime
            .request_tx
            .send(id.to_owned())
            .is_ok()
        {
            self.thumbnail_pending.insert(id.to_owned());
        }
    }

    pub fn poll_asset_thumbnails(&mut self) -> bool {
        let results: Vec<_> = self.thumbnail_runtime.result_rx.try_iter().collect();
        if results.is_empty() {
            return false;
        }
        for (id, result) in results {
            self.thumbnail_pending.remove(&id);
            if !self.asset_catalog.iter().any(|asset| asset.id == id) {
                continue;
            }
            match result {
                Ok(png) => match image::load_from_memory(&png) {
                    Ok(image) => {
                        let rgba = image.to_rgba8();
                        let size = [rgba.width() as usize, rgba.height() as usize];
                        let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                        let texture = self.egui_ctx.load_texture(
                            format!("asset-thumbnail-{id}"),
                            color,
                            egui::TextureOptions::LINEAR,
                        );
                        self.asset_thumbnails.insert(id, texture);
                    }
                    Err(error) => {
                        log::error!("Thumbnail dekodieren: {error}");
                        self.thumbnail_failed.insert(id);
                    }
                },
                Err(error) => {
                    log::error!("Thumbnail erzeugen: {error}");
                    self.thumbnail_failed.insert(id);
                }
            }
        }
        true
    }

    /// Fügt ein bereits im globalen Katalog vorhandenes Asset erneut ein.
    pub fn import_catalog_asset(&mut self, id: &str) {
        if self.asset_import_pending {
            return;
        }
        if self
            .asset_import_runtime
            .request_tx
            .send(id.to_owned())
            .is_ok()
        {
            self.asset_import_pending = true;
            self.toasts
                .success("Asset wird im Hintergrund vorbereitet …");
        }
    }

    pub fn poll_asset_import(&mut self) -> bool {
        let Ok(result) = self.asset_import_runtime.result_rx.try_recv() else {
            return false;
        };
        self.asset_import_pending = false;
        let (meta, contours) = match result {
            Ok(result) => result,
            Err(error) => {
                self.toasts
                    .error(format!("Asset-Import fehlgeschlagen: {error}"));
                return true;
            }
        };
        match meta.kind {
            luxifer_core::AssetKind::Image => {
                let index = self.session.add_image(
                    meta.id.clone(),
                    20.0,
                    20.0,
                    meta.width as f64 / 10.0,
                    meta.height as f64 / 10.0,
                );
                self.session.selected = vec![index];
                self.image_dirty = true;
                self.fit_all();
            }
            luxifer_core::AssetKind::SvgSource | luxifer_core::AssetKind::DxfSource => {
                if let Some(contours) = contours {
                    self.session.add_polylines(contours);
                    self.refresh_accent();
                    self.fit_all();
                }
            }
            luxifer_core::AssetKind::Font => return true,
        }
        self.session_asset_context.insert(meta.id.clone());
        self.tag_asset_for_current_project(&meta.id);
        self.refresh_asset_catalog();
        true
    }

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
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("quelle");
                let kind = if ext == "svg" {
                    luxifer_core::AssetKind::SvgSource
                } else {
                    luxifer_core::AssetKind::DxfSource
                };
                match luxifer_core::import_source(
                    &luxifer_core::assets_dir(),
                    &bytes,
                    name,
                    &ext,
                    kind,
                ) {
                    Ok(meta) => {
                        self.session_asset_context.insert(meta.id.clone());
                        self.tag_asset_for_current_project(&meta.id);
                        self.refresh_asset_catalog();
                    }
                    Err(error) => log::error!("Quelldatei katalogisieren: {error}"),
                }
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
                self.session_asset_context.insert(meta.id.clone());
                self.tag_asset_for_current_project(&meta.id);
                self.refresh_asset_catalog();
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

    pub(super) fn tag_asset_for_current_project(&self, id: &str) {
        let Some(name) = self.project.open_name() else {
            return;
        };
        let description = self
            .project
            .detail(name)
            .map(|detail| detail.description)
            .unwrap_or_default();
        if let Err(error) = luxifer_core::add_asset_tags(
            &luxifer_core::assets_dir(),
            &id.to_string(),
            [name, description.as_str()],
        ) {
            log::error!("Asset-Tags ergänzen: {error}");
        }
    }

    pub(super) fn tag_current_project_assets(&mut self) {
        let mut ids: Vec<String> = self
            .session
            .shapes
            .iter()
            .filter_map(|shape| match &shape.geo {
                Geo::Image { asset, .. } => Some(asset.clone()),
                _ => None,
            })
            .collect();
        ids.extend(self.session_asset_context.iter().cloned());
        ids.sort();
        ids.dedup();
        for id in ids {
            self.tag_asset_for_current_project(&id);
        }
        if self.project.has_open() {
            self.session_asset_context.clear();
        }
        self.refresh_asset_catalog();
    }
}
