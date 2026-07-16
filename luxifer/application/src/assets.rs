//! Asset-Auflösung für Jobplanung und Vorschau: liefert dem Core die
//! Graustufen-Pixel eines Bild-Assets aus dem Store.
//!
//! Genau EINE Quelle für Vorschau UND echten Job/Export — die Vorschau darf
//! nichts zeigen, was der Job nicht tut (und umgekehrt). Fehlende oder
//! unlesbare Assets werden übersprungen (der Core lässt den Bild-Layer dann
//! leer); der Fehler wird auf stderr protokolliert, damit er nicht stumm
//! verschwindet.

use std::borrow::Cow;
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

impl AssetService {
    pub fn list_visible() -> Result<Vec<luxifer_core::AssetMeta>, AppError> {
        let store = luxifer_core::assets_dir();
        luxifer_core::list_assets(&store)
            .map(|assets| {
                assets
                    .into_iter()
                    .filter(|asset| !luxifer_core::asset_hidden(&store, &asset.id))
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
pub(crate) fn resolve_luma(id: &str) -> Option<(Cow<'static, [u8]>, usize, usize)> {
    let dir = luxifer_core::assets_dir();
    match luxifer_core::load_asset_luma(&dir, &id.to_string()) {
        Ok((pixels, w, h)) => Some((Cow::Owned(pixels), w as usize, h as usize)),
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
}
