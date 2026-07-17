//! Asset-Auflösung für Jobplanung und Vorschau: liefert dem Core die
//! Graustufen-Pixel eines Bild-Assets aus dem Store.
//!
//! Genau EINE Quelle für Vorschau UND echten Job/Export — die Vorschau darf
//! nichts zeigen, was der Job nicht tut (und umgekehrt). Fehlende oder
//! unlesbare Assets werden übersprungen (der Core lässt den Bild-Layer dann
//! leer); der Fehler wird auf stderr protokolliert, damit er nicht stumm
//! verschwindet.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::AppError;

pub type ImportedContours = Vec<(Vec<(f64, f64)>, bool)>;

/// Für die Darstellung vorbereitete Graustufen-Textur. Die Umwandlung ist Teil
/// des Import-Anwendungsfalls; Native muss das Assetformat nicht erneut kennen.
#[derive(Debug, Clone)]
pub struct PreparedImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Vollständig geladenes Katalog-Asset für die Übergabe an die UI-Sitzung.
#[derive(Debug, Clone)]
pub struct PreparedAsset {
    pub meta: luxifer_core::AssetMeta,
    pub contours: Option<ImportedContours>,
    pub image: Option<PreparedImage>,
}

/// UI-unabhängige Grenze für Asset-Katalog, Import und Persistenz.
pub struct AssetService;

#[derive(Clone, Copy, Debug)]
pub enum CropGeometry {
    Rect([f32; 4]),
    /// Mittelpunkt und zwei Halbachsenpunkte, normalisiert aufs Quellbild.
    Ellipse([[f32; 2]; 3]),
}

impl AssetService {
    pub fn list_visible() -> Result<Vec<luxifer_core::AssetMeta>, AppError> {
        let store = luxifer_core::assets_dir();
        let saved_refs = Self::saved_project_asset_refs();
        luxifer_core::list_assets(&store)
            .map(|assets| {
                assets
                    .into_iter()
                    .filter(|asset| {
                        !luxifer_core::asset_hidden(&store, &asset.id)
                            && (!Self::is_derived(asset) || saved_refs.contains(&asset.id))
                    })
                    .collect()
            })
            .map_err(|error| {
                AppError::wrap(
                    "asset_list",
                    "Asset-Katalog konnte nicht geladen werden.",
                    error.to_string(),
                )
            })
    }

    fn saved_project_asset_refs() -> HashSet<String> {
        let projects = luxifer_core::projects_dir();
        let mut refs = HashSet::new();
        for info in luxifer_core::list_projects(&projects) {
            let Ok(project) = luxifer_core::ProjectFile::load_by_name(&projects, &info.name) else {
                continue;
            };
            refs.extend(project.asset_refs.iter().cloned());
            for version in &project.versions {
                if let Ok(snapshot) =
                    luxifer_core::ProjectFile::load_version(&projects, &project.name, &version.id)
                {
                    refs.extend(snapshot.asset_refs);
                }
            }
        }
        refs
    }

    /// Entfernt nur intern erzeugte Assets, die in keiner gespeicherten
    /// Projektversion vorkommen. Beim Programmstart existiert noch keine
    /// Undo-Historie, daher können temporäre Crop-Dateien sicher weg.
    pub fn cleanup_orphan_derived() -> Result<usize, AppError> {
        let store = luxifer_core::assets_dir();
        let saved_refs = Self::saved_project_asset_refs();
        let assets = luxifer_core::list_assets(&store).map_err(|error| {
            AppError::wrap(
                "asset_cleanup_list",
                "Asset-Bereinigung konnte nicht gestartet werden.",
                error.to_string(),
            )
        })?;
        let mut removed = 0;
        for asset in assets {
            if Self::is_derived(&asset) && !saved_refs.contains(&asset.id) {
                luxifer_core::delete_asset(&store, &asset.id).map_err(|error| {
                    AppError::wrap(
                        "asset_cleanup_delete",
                        "Temporäres Crop-Asset konnte nicht entfernt werden.",
                        error.to_string(),
                    )
                })?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn is_derived(asset: &luxifer_core::AssetMeta) -> bool {
        asset.derived || asset.original_name == "Bildausschnitt.png"
    }

    pub fn prepare_catalog(id: &str) -> Result<PreparedAsset, AppError> {
        let store = luxifer_core::assets_dir();
        let meta = luxifer_core::asset_meta(&store, &id.to_owned()).map_err(|error| {
            AppError::wrap(
                "asset_read",
                "Asset konnte nicht geladen werden.",
                error.to_string(),
            )
        })?;
        Self::prepare_meta(meta)
    }

    pub fn import_path(path: &Path) -> Result<PreparedAsset, AppError> {
        let bytes = std::fs::read(path).map_err(|error| {
            AppError::wrap(
                "asset_file_read",
                "Datei konnte nicht gelesen werden.",
                error.to_string(),
            )
        })?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("quelle");
        let ext = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let store = luxifer_core::assets_dir();
        let meta = if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "bmp" | "webp") {
            luxifer_core::import_image(&store, &bytes, name)
        } else {
            let kind = match ext.as_str() {
                "svg" => luxifer_core::AssetKind::SvgSource,
                "dxf" => luxifer_core::AssetKind::DxfSource,
                _ => {
                    return Err(AppError::new(
                        "asset_format",
                        "Dieses Dateiformat wird nicht unterstützt.",
                    ))
                }
            };
            luxifer_core::import_source(&store, &bytes, name, &ext, kind)
        }
        .map_err(|error| {
            AppError::wrap(
                "asset_import",
                "Asset konnte nicht importiert werden.",
                error.to_string(),
            )
        })?;
        Self::prepare_meta(meta)
    }

    pub fn import_font(path: &Path) -> Result<luxifer_core::AssetMeta, AppError> {
        let bytes = std::fs::read(path).map_err(|error| {
            AppError::wrap(
                "font_read",
                "Font konnte nicht gelesen werden.",
                error.to_string(),
            )
        })?;
        luxifer_core::text::text_to_contours(&bytes, "Ag", 20.0).map_err(|error| {
            AppError::wrap(
                "font_invalid",
                "Die Font-Datei ist nicht verwendbar.",
                error.to_string(),
            )
        })?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("font.ttf");
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("ttf");
        luxifer_core::import_source(
            &luxifer_core::assets_dir(),
            &bytes,
            name,
            ext,
            luxifer_core::AssetKind::Font,
        )
        .map_err(|error| {
            AppError::wrap(
                "font_import",
                "Font konnte nicht importiert werden.",
                error.to_string(),
            )
        })
    }

    pub fn catalog_font(path: &Path, bytes: &[u8]) -> Result<luxifer_core::AssetMeta, AppError> {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("font.ttf");
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("ttf");
        luxifer_core::import_source(
            &luxifer_core::assets_dir(),
            bytes,
            name,
            ext,
            luxifer_core::AssetKind::Font,
        )
        .map_err(|error| {
            AppError::wrap(
                "font_catalog",
                "Font konnte nicht katalogisiert werden.",
                error.to_string(),
            )
        })
    }

    pub fn asset_path(id: &str) -> Option<PathBuf> {
        luxifer_core::asset_path(&luxifer_core::assets_dir(), &id.to_owned())
    }

    pub fn thumbnail(id: &str) -> Result<Vec<u8>, AppError> {
        luxifer_core::asset_thumbnail(&luxifer_core::assets_dir(), &id.to_owned()).map_err(
            |error| {
                AppError::wrap(
                    "asset_thumbnail",
                    "Asset-Vorschau konnte nicht erzeugt werden.",
                    error.to_string(),
                )
            },
        )
    }

    /// Erzeugt die nicht-destruktive Editor-Vorschau mit derselben Core-
    /// Verarbeitung wie die Canvas-Darstellung, einschließlich Dithering.
    pub fn image_preview(
        id: &str,
        params: &luxifer_core::ImageParams,
    ) -> Result<PreparedImage, AppError> {
        let png = luxifer_core::rendered_png(
            &luxifer_core::assets_dir(),
            &id.to_owned(),
            params,
            params.invert_editor,
        )
        .map_err(|error| {
            AppError::wrap(
                "image_preview",
                "Bildvorschau konnte nicht erzeugt werden.",
                error.to_string(),
            )
        })?;
        let image = image::load_from_memory(&png).map_err(|error| {
            AppError::wrap(
                "image_preview_decode",
                "Bildvorschau konnte nicht gelesen werden.",
                error.to_string(),
            )
        })?;
        let rgba = image.to_rgba8();
        Ok(PreparedImage {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        })
    }

    /// Binäre Vorschau der Pixel, die der Trace als Vordergrund erfasst.
    /// Nutzt wie `EditorSession::trace_image` nur die Tonwert-LUT vor der
    /// separaten Trace-Schwelle.
    pub fn trace_preview(
        id: &str,
        params: &luxifer_core::ImageParams,
        threshold: u8,
        invert: bool,
    ) -> Result<PreparedImage, AppError> {
        let (pixels, width, height) =
            luxifer_core::load_asset_luma(&luxifer_core::assets_dir(), &id.to_owned()).map_err(
                |error| {
                    AppError::wrap(
                        "trace_preview",
                        "Trace-Vorschau konnte nicht erzeugt werden.",
                        error.to_string(),
                    )
                },
            )?;
        let lut = luxifer_core::ImageParams {
            mode: luxifer_core::ImageMode::Grayscale,
            ..*params
        };
        let gray = luxifer_core::apply_params(&pixels, &lut, false);
        let mut rgba = Vec::with_capacity(gray.len() * 4);
        for pixel in gray {
            let captured = (pixel < threshold) != invert;
            let color = if captured { 20 } else { 245 };
            rgba.extend_from_slice(&[color, color, color, 255]);
        }
        Ok(PreparedImage {
            rgba,
            width,
            height,
        })
    }

    /// Rendert einen Quellbild-Ausschnitt mit den aktuellen Bildparametern.
    pub fn crop_preview(
        id: &str,
        params: &luxifer_core::ImageParams,
        crop: [f32; 4],
    ) -> Result<PreparedImage, AppError> {
        let cropped = Self::cropped_luma(id, crop)?;
        let (width, height) = cropped.dimensions();
        let mut processed =
            luxifer_core::apply_params(cropped.as_raw(), params, params.invert_editor);
        if luxifer_core::dither::is_dither(params.mode) {
            processed = luxifer_core::dither::dither(
                &processed,
                width as usize,
                height as usize,
                params.mode,
            );
        }
        let mut rgba = Vec::with_capacity(processed.len() * 4);
        for pixel in processed {
            rgba.extend_from_slice(&[pixel, pixel, pixel, 255]);
        }
        Ok(PreparedImage {
            rgba,
            width,
            height,
        })
    }

    /// Legt den Ausschnitt als abgeleitetes Asset ab. Das Quellasset bleibt
    /// unverändert und kann von Undo oder anderen Bildern weiter genutzt werden.
    pub fn crop_image(id: &str, crop: [f32; 4]) -> Result<luxifer_core::AssetMeta, AppError> {
        let cropped = Self::cropped_luma(id, crop)?;
        Self::store_crop(cropped)
    }

    pub fn crop_image_geometry(
        id: &str,
        geometry: CropGeometry,
    ) -> Result<(luxifer_core::AssetMeta, [f32; 4]), AppError> {
        match geometry {
            CropGeometry::Rect(crop) => Ok((Self::crop_image(id, crop)?, crop)),
            CropGeometry::Ellipse(points) => {
                let c = points[0];
                let a = [points[1][0] - c[0], points[1][1] - c[1]];
                let b = [points[2][0] - c[0], points[2][1] - c[1]];
                let det = a[0] * b[1] - a[1] * b[0];
                if !det.is_finite() || det.abs() < 0.0001 {
                    return Err(AppError::new(
                        "image_crop_ellipse",
                        "Die Ellipse ist zu klein oder ungültig.",
                    ));
                }
                let ex = (a[0] * a[0] + b[0] * b[0]).sqrt();
                let ey = (a[1] * a[1] + b[1] * b[1]).sqrt();
                let crop = [
                    (c[0] - ex).max(0.0),
                    (c[1] - ey).max(0.0),
                    (c[0] + ex).min(1.0),
                    (c[1] + ey).min(1.0),
                ];
                let luma = Self::cropped_luma(id, crop)?;
                let mut image =
                    image::GrayAlphaImage::from_fn(luma.width(), luma.height(), |x, y| {
                        image::LumaA([luma.get_pixel(x, y).0[0], 255])
                    });
                let (width, height) = image.dimensions();
                for y in 0..height {
                    for x in 0..width {
                        let source = [
                            crop[0] + (x as f32 + 0.5) / width as f32 * (crop[2] - crop[0]),
                            crop[1] + (y as f32 + 0.5) / height as f32 * (crop[3] - crop[1]),
                        ];
                        let d = [source[0] - c[0], source[1] - c[1]];
                        let u = (d[0] * b[1] - d[1] * b[0]) / det;
                        let v = (a[0] * d[1] - a[1] * d[0]) / det;
                        if u * u + v * v > 1.0 {
                            image.put_pixel(x, y, image::LumaA([255, 0]));
                        }
                    }
                }
                Ok((Self::store_crop_alpha(image)?, crop))
            }
        }
    }

    fn store_crop(cropped: image::GrayImage) -> Result<luxifer_core::AssetMeta, AppError> {
        let mut png = Vec::new();
        image::DynamicImage::ImageLuma8(cropped)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|error| {
                AppError::wrap(
                    "image_crop_encode",
                    "Bildausschnitt konnte nicht gespeichert werden.",
                    error.to_string(),
                )
            })?;
        luxifer_core::import_image(&luxifer_core::assets_dir(), &png, "Bildausschnitt.png").map_err(
            |error| {
                AppError::wrap(
                    "image_crop_store",
                    "Bildausschnitt konnte nicht abgelegt werden.",
                    error.to_string(),
                )
            },
        )
    }

    fn store_crop_alpha(
        cropped: image::GrayAlphaImage,
    ) -> Result<luxifer_core::AssetMeta, AppError> {
        let mut png = Vec::new();
        image::DynamicImage::ImageLumaA8(cropped)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|error| {
                AppError::wrap(
                    "image_crop_encode",
                    "Bildausschnitt konnte nicht gespeichert werden.",
                    error.to_string(),
                )
            })?;
        luxifer_core::import_image_preserve_alpha(
            &luxifer_core::assets_dir(),
            &png,
            "Bildausschnitt.png",
        )
        .map_err(|error| {
            AppError::wrap(
                "image_crop_store",
                "Bildausschnitt konnte nicht abgelegt werden.",
                error.to_string(),
            )
        })
    }

    fn cropped_luma(id: &str, crop: [f32; 4]) -> Result<image::GrayImage, AppError> {
        let valid = crop.iter().all(|value| value.is_finite())
            && crop[0] >= 0.0
            && crop[1] >= 0.0
            && crop[2] <= 1.0
            && crop[3] <= 1.0
            && crop[2] - crop[0] >= 0.01
            && crop[3] - crop[1] >= 0.01;
        if !valid {
            return Err(AppError::new(
                "image_crop_bounds",
                "Der Bildausschnitt ist zu klein oder ungültig.",
            ));
        }
        let bytes = luxifer_core::load_asset(&luxifer_core::assets_dir(), &id.to_owned()).map_err(
            |error| {
                AppError::wrap(
                    "image_crop_read",
                    "Quellbild konnte nicht gelesen werden.",
                    error.to_string(),
                )
            },
        )?;
        let image = image::load_from_memory(&bytes)
            .map_err(|error| {
                AppError::wrap(
                    "image_crop_decode",
                    "Quellbild konnte nicht dekodiert werden.",
                    error.to_string(),
                )
            })?
            .to_luma8();
        let (width, height) = image.dimensions();
        let left = (crop[0] * width as f32).floor() as u32;
        let top = (crop[1] * height as f32).floor() as u32;
        let right = ((crop[2] * width as f32).ceil() as u32).clamp(left + 1, width);
        let bottom = ((crop[3] * height as f32).ceil() as u32).clamp(top + 1, height);
        Ok(image::imageops::crop_imm(&image, left, top, right - left, bottom - top).to_image())
    }

    pub fn delete_or_hide(id: &str, referenced_in_session: bool) -> Result<bool, AppError> {
        let projects = luxifer_core::projects_dir();
        let referenced = referenced_in_session
            || luxifer_core::list_projects(&projects)
                .into_iter()
                .any(|info| {
                    luxifer_core::ProjectFile::load_by_name(&projects, &info.name)
                        .map(|project| project.asset_refs.iter().any(|asset| asset == id))
                        .unwrap_or(false)
                });
        let store = luxifer_core::assets_dir();
        let result = if referenced {
            luxifer_core::hide_asset(&store, &id.to_owned())
        } else {
            luxifer_core::delete_asset(&store, &id.to_owned())
        };
        result.map_err(|error| {
            AppError::wrap(
                "asset_delete",
                "Asset konnte nicht entfernt werden.",
                error.to_string(),
            )
        })?;
        Ok(referenced)
    }

    pub fn add_tags<'a>(id: &str, tags: impl IntoIterator<Item = &'a str>) -> Result<(), AppError> {
        luxifer_core::add_asset_tags(&luxifer_core::assets_dir(), &id.to_owned(), tags)
            .map(|_| ())
            .map_err(|error| {
                AppError::wrap(
                    "asset_tags",
                    "Asset-Tags konnten nicht gespeichert werden.",
                    error.to_string(),
                )
            })
    }

    pub fn enrich_tags_from_projects() {
        let projects = luxifer_core::projects_dir();
        for info in luxifer_core::list_projects(&projects) {
            let Ok(project) = luxifer_core::ProjectFile::load_by_name(&projects, &info.name) else {
                continue;
            };
            for id in &project.asset_refs {
                let _ = Self::add_tags(id, [project.name.as_str(), project.description.as_str()]);
            }
        }
    }

    fn prepare_meta(meta: luxifer_core::AssetMeta) -> Result<PreparedAsset, AppError> {
        let store = luxifer_core::assets_dir();
        let contours = match meta.kind {
            luxifer_core::AssetKind::SvgSource | luxifer_core::AssetKind::DxfSource => {
                let bytes = luxifer_core::load_asset(&store, &meta.id).map_err(|error| {
                    AppError::wrap(
                        "asset_read",
                        "Asset konnte nicht geladen werden.",
                        error.to_string(),
                    )
                })?;
                Some(
                    luxifer_core::import::import_vector(&bytes, &meta.ext).map_err(|error| {
                        AppError::wrap(
                            "vector_import",
                            "Vektordatei konnte nicht verarbeitet werden.",
                            error.to_string(),
                        )
                    })?,
                )
            }
            _ => None,
        };
        let image = if meta.kind == luxifer_core::AssetKind::Image {
            let (luma, width, height) =
                luxifer_core::load_asset_luma(&store, &meta.id).map_err(|error| {
                    AppError::wrap(
                        "image_read",
                        "Bild-Asset konnte nicht geladen werden.",
                        error.to_string(),
                    )
                })?;
            let mut rgba = Vec::with_capacity(luma.len() * 4);
            for value in luma {
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
            Some(PreparedImage {
                rgba,
                width,
                height,
            })
        } else {
            None
        };
        Ok(PreparedAsset {
            meta,
            contours,
            image,
        })
    }
}

/// Graustufen-Pixel (row-major `u8`) samt Pixelmaßen zu einer Asset-ID, im
/// Format des `JobPlan::from_shapes_with_assets`-Resolvers.
type ResolvedRaster = (Cow<'static, [u8]>, Cow<'static, [u8]>, usize, usize);

pub(crate) fn resolve_luma(id: &str) -> Option<ResolvedRaster> {
    let dir = luxifer_core::assets_dir();
    match luxifer_core::load_asset_luma_alpha(&dir, &id.to_string()) {
        Ok((pixels, alpha, w, h)) => Some((
            Cow::Owned(pixels),
            Cow::Owned(alpha),
            w as usize,
            h as usize,
        )),
        Err(e) => {
            eprintln!("Bild-Asset {id} für den Job nicht ladbar: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elliptischer_crop_maskiert_ecken_und_behaelt_mitte() {
        let _guard = crate::test_env::with_temp_dir("ellipse_crop");
        let source = image::GrayImage::from_pixel(20, 20, image::Luma([0]));
        let mut png = Vec::new();
        image::DynamicImage::ImageLuma8(source)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("PNG");
        let meta = luxifer_core::import_image(&luxifer_core::assets_dir(), &png, "Quelle.png")
            .expect("Quellasset");

        let (cropped, bounds) = AssetService::crop_image_geometry(
            &meta.id,
            CropGeometry::Ellipse([[0.5, 0.5], [0.9, 0.5], [0.5, 0.8]]),
        )
        .expect("elliptischer Crop");
        let (pixels, width, height) =
            luxifer_core::load_asset_luma(&luxifer_core::assets_dir(), &cropped.id)
                .expect("Crop laden");
        let (rgba, _, _) = luxifer_core::load_asset_rgba(&luxifer_core::assets_dir(), &cropped.id)
            .expect("Crop mit Alpha laden");

        for (actual, expected) in bounds.into_iter().zip([0.1, 0.2, 0.9, 0.8]) {
            assert!((actual - expected).abs() < 0.00001);
        }
        assert_eq!(pixels[0], 255, "Ecke liegt außerhalb der Ellipse");
        assert_eq!(rgba[3], 0, "Ecke muss transparent sein");
        assert_eq!(pixels[(height / 2 * width + width / 2) as usize], 0);
        assert_eq!(
            rgba[((height / 2 * width + width / 2) * 4 + 3) as usize],
            255
        );
    }

    #[test]
    fn crop_assets_erscheinen_nur_mit_gespeicherter_projektreferenz() {
        let _guard = crate::test_env::with_temp_dir("crop_lifecycle");
        let source = image::GrayImage::from_pixel(8, 8, image::Luma([40]));
        let mut png = Vec::new();
        image::DynamicImage::ImageLuma8(source)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let original =
            luxifer_core::import_image(&luxifer_core::assets_dir(), &png, "Original.png").unwrap();
        let (crop, _) = AssetService::crop_image_geometry(
            &original.id,
            CropGeometry::Ellipse([[0.5, 0.5], [0.9, 0.5], [0.5, 0.9]]),
        )
        .unwrap();

        let visible = AssetService::list_visible().unwrap();
        assert!(visible.iter().any(|asset| asset.id == original.id));
        assert!(!visible.iter().any(|asset| asset.id == crop.id));

        let mut state = luxifer_core::AppState::new();
        state.add_image(crop.id.clone(), 0.0, 0.0, 10.0, 10.0);
        let project = luxifer_core::ProjectFile::from_state(&state, "Crop-Projekt", Vec::new());
        project.save_to_dir(&luxifer_core::projects_dir()).unwrap();

        assert!(AssetService::list_visible()
            .unwrap()
            .iter()
            .any(|asset| asset.id == crop.id));
        assert_eq!(AssetService::cleanup_orphan_derived().unwrap(), 0);
        assert!(luxifer_core::asset_meta(&luxifer_core::assets_dir(), &crop.id).is_ok());
    }

    #[test]
    fn unbekanntes_format_wird_stabil_abgewiesen() {
        let _guard = crate::test_env::with_temp_dir("asset_format");
        let path = luxifer_core::assets_dir().join("quelle.txt");
        std::fs::create_dir_all(path.parent().expect("Datenverzeichnis"))
            .expect("Datenverzeichnis anlegen");
        std::fs::write(&path, b"kein asset").expect("Testdatei schreiben");

        let error = AssetService::import_path(&path).expect_err("Format muss scheitern");

        assert_eq!(error.code(), "asset_format");
    }

    #[test]
    fn unbrauchbarer_font_wird_nicht_katalogisiert() {
        let _guard = crate::test_env::with_temp_dir("font_invalid");
        let path = luxifer_core::assets_dir().join("kaputt.ttf");
        std::fs::create_dir_all(path.parent().expect("Datenverzeichnis"))
            .expect("Datenverzeichnis anlegen");
        std::fs::write(&path, b"kein font").expect("Testdatei schreiben");

        let error = AssetService::import_font(&path).expect_err("Font muss scheitern");

        assert_eq!(error.code(), "font_invalid");
        assert!(AssetService::list_visible().expect("Katalog").is_empty());
    }

    #[test]
    fn bildvorschau_nutzt_die_core_parameterpipeline() {
        let _guard = crate::test_env::with_temp_dir("image_preview");
        let mut source = image::GrayImage::new(2, 1);
        source.put_pixel(0, 0, image::Luma([40]));
        source.put_pixel(1, 0, image::Luma([220]));
        let mut png = Vec::new();
        image::DynamicImage::ImageLuma8(source)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("Test-PNG");
        let meta = luxifer_core::import_image(&luxifer_core::assets_dir(), &png, "preview.png")
            .expect("Bild importieren");
        let params = luxifer_core::ImageParams {
            mode: luxifer_core::ImageMode::Threshold,
            threshold: 128,
            ..Default::default()
        };

        let preview = AssetService::image_preview(&meta.id, &params).expect("Vorschau");

        assert_eq!((preview.width, preview.height), (2, 1));
        assert_eq!(&preview.rgba[0..4], &[0, 0, 0, 255]);
        assert_eq!(&preview.rgba[4..8], &[255, 255, 255, 255]);
    }
}
