//! Zentraler Asset-Store (ADR 0004).
//!
//! Importierte Bilder, Fonts und originale SVG-/DXF-Quellen werden **einmalig**
//! unter `<data_root>/Assets/` abgelegt,
//! per **Content-Hash** benannt und projektübergreifend geteilt (nie pro Projekt
//! kopiert — das war ThorBurns Import-Fehler). Farbige Quellen werden beim Import
//! zu **Graustufe in voller Auflösung** konvertiert (Luminanz) und *diese* als
//! Asset gespeichert; die Farbe wird verworfen (der Laser braucht ohnehin Grau,
//! das Canvas zeigt farblos). Die Quelldatei auf der Platte bleibt unangetastet.
//!
//! Fachlogik (UI-frei, testbar): Dekodierung, Graustufe, Hash, Ablage. Das
//! Frontend liefert nur den Pfad und zeichnet später die Vorschau.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::datetime::now_iso8601;
use crate::geometry::{ImageMode, ImageParams};
use crate::project::data_root;

/// Unterordner des Datenverzeichnisses für den Asset-Store.
pub const ASSETS_DIR: &str = "Assets";

/// Asset-Store-Verzeichnis (`<data_root>/Assets`).
pub fn assets_dir() -> PathBuf {
    data_root().join(ASSETS_DIR)
}

/// Stabile Asset-Identität = Content-Hash der abgelegten Bytes. Gleicher Inhalt
/// zweimal importiert ⇒ dieselbe ID ⇒ ein Asset.
pub type AssetId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    #[default]
    Image,
    Font,
    SvgSource,
    DxfSource,
}

/// Metadaten eines Assets, liegen als `<hash>.meta.json` neben den Bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetMeta {
    /// Content-Hash (= Dateiname ohne Endung), zugleich `AssetId`.
    pub id: AssetId,
    /// Dateiendung der abgelegten Bytes (aktuell immer `png`, da Graustufe).
    pub ext: String,
    #[serde(default)]
    pub kind: AssetKind,
    /// Ursprünglicher Dateiname der Quelldatei (nur zur Anzeige).
    #[serde(default)]
    pub original_name: String,
    /// Format der Quelldatei vor der Konvertierung (z. B. `jpeg`).
    #[serde(default)]
    pub source_format: String,
    /// Pixelmaße des abgelegten Graustufenbildes.
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    /// Importzeitpunkt (ISO-8601 UTC).
    #[serde(default)]
    pub import_at: String,
    /// Suchbegriffe aus Dateiname und den Projekten, in denen das Asset
    /// verwendet wurde. Metadaten dürfen wachsen, die Content-ID bleibt stabil.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Fehler beim Import/Store.
#[derive(Debug)]
pub struct AssetError(pub String);

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AssetError {}

impl From<std::io::Error> for AssetError {
    fn from(e: std::io::Error) -> Self {
        AssetError(e.to_string())
    }
}

/// FNV-1a-64-Hash der Bytes, hex-kodiert. Kein Fremd-Crate, ausreichend
/// kollisionsarm für lokale Content-Adressierung (analog `project::gen_id`).
pub fn content_hash(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    format!("{h:016x}")
}

/// Wandelt beliebige Bild-Bytes in **Graustufe (volle Auflösung, Luminanz)** um
/// und gibt die als **PNG** kodierten Bytes samt Pixelmaßen und erkanntem
/// Quellformat zurück. Bereits graue Bilder bleiben inhaltlich unverändert.
///
/// Luminanz gamma-korrekt (sRGB-linearisiert) über `image::to_luma8` —
/// fotografisch korrekt statt naiver Rec.601-Gewichtung. PNG als verlustfreies
/// Ablageformat, damit die Graustufe exakt bleibt (kein erneuter JPEG-Verlust).
pub fn to_grayscale_png(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32, String), AssetError> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| AssetError(e.to_string()))?;
    let source_format = reader
        .format()
        .map(|f| format!("{f:?}").to_lowercase())
        .unwrap_or_default();
    let img = reader.decode().map_err(|e| AssetError(e.to_string()))?;
    let luma = img.to_luma8();
    let (w, h) = (luma.width(), luma.height());

    let mut out = Vec::new();
    image::DynamicImage::ImageLuma8(luma)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| AssetError(e.to_string()))?;
    Ok((out, w, h, source_format))
}

/// Importiert eine Bilddatei in den Store: liest die Quelle, konvertiert zu
/// Graustufe (PNG), legt Bytes + Metadaten unter dem Content-Hash ab und gibt die
/// Metadaten zurück. Idempotent: existiert der Hash schon, wird nichts neu
/// geschrieben (gleiches Bild = ein Asset).
///
/// `source_bytes` sind die rohen Bytes der Quelldatei (das Frontend/Backend liest
/// sie; der Core fasst die Platte nur im Store an). `original_name` dient nur der
/// Anzeige.
pub fn import_image(
    store_dir: &Path,
    source_bytes: &[u8],
    original_name: &str,
) -> Result<AssetMeta, AssetError> {
    let (png, width, height, source_format) = to_grayscale_png(source_bytes)?;
    let id = content_hash(&png);
    std::fs::create_dir_all(store_dir)?;

    let bytes_path = store_dir.join(format!("{id}.png"));
    // Nur schreiben, wenn noch nicht vorhanden (idempotent, spart I/O).
    if !bytes_path.exists() {
        std::fs::write(&bytes_path, &png)?;
    }

    let meta = AssetMeta {
        id: id.clone(),
        ext: "png".into(),
        kind: AssetKind::Image,
        original_name: original_name.to_string(),
        source_format,
        width,
        height,
        import_at: now_iso8601(),
        tags: derive_tags([original_name]),
    };
    let meta_path = store_dir.join(format!("{id}.meta.json"));
    if !meta_path.exists() {
        let json = serde_json::to_string_pretty(&meta).map_err(|e| AssetError(e.to_string()))?;
        std::fs::write(&meta_path, json)?;
    }
    add_asset_tags(store_dir, &id, [original_name])
}

/// Legt eine unveränderte Quelldatei content-adressiert im Katalog ab.
pub fn import_source(
    store_dir: &Path,
    source_bytes: &[u8],
    original_name: &str,
    ext: &str,
    kind: AssetKind,
) -> Result<AssetMeta, AssetError> {
    if kind == AssetKind::Image {
        return Err(AssetError(
            "Bildquellen müssen über import_image laufen".into(),
        ));
    }
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    if ext.is_empty() || !ext.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(AssetError("Ungültige Asset-Dateiendung".into()));
    }
    let id = content_hash(source_bytes);
    std::fs::create_dir_all(store_dir)?;
    let bytes_path = store_dir.join(format!("{id}.{ext}"));
    if !bytes_path.exists() {
        std::fs::write(&bytes_path, source_bytes)?;
    }
    let meta = AssetMeta {
        id: id.clone(),
        ext,
        kind,
        original_name: original_name.into(),
        source_format: String::new(),
        width: 0,
        height: 0,
        import_at: now_iso8601(),
        tags: derive_tags([original_name]),
    };
    let meta_path = store_dir.join(format!("{id}.meta.json"));
    if !meta_path.exists() {
        let json = serde_json::to_vec_pretty(&meta).map_err(|e| AssetError(e.to_string()))?;
        std::fs::write(meta_path, json)?;
    }
    add_asset_tags(store_dir, &id, [original_name])
}

pub fn list_assets(store_dir: &Path) -> Result<Vec<AssetMeta>, AssetError> {
    let entries = match std::fs::read_dir(store_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut assets: Vec<AssetMeta> = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json")
            || !path.to_string_lossy().ends_with(".meta.json")
        {
            continue;
        }
        let bytes = std::fs::read(path)?;
        assets.push(serde_json::from_slice(&bytes).map_err(|e| AssetError(e.to_string()))?);
    }
    assets.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(assets)
}

/// Übernimmt ein bereits normalisiertes Katalog-Asset (z. B. von Charon).
pub fn store_asset(store_dir: &Path, meta: &AssetMeta, bytes: &[u8]) -> Result<(), AssetError> {
    if content_hash(bytes) != meta.id
        || meta.ext.is_empty()
        || !meta.ext.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err(AssetError(
            "Asset-Hash oder Dateiendung ist ungültig".into(),
        ));
    }
    std::fs::create_dir_all(store_dir)?;
    let data_path = store_dir.join(format!("{}.{}", meta.id, meta.ext));
    let meta_path = store_dir.join(format!("{}.meta.json", meta.id));
    if !data_path.exists() {
        let temp = store_dir.join(format!(".{}.data.tmp", meta.id));
        std::fs::write(&temp, bytes)?;
        std::fs::rename(temp, data_path)?;
    }
    if !meta_path.exists() {
        let temp = store_dir.join(format!(".{}.meta.tmp", meta.id));
        let json = serde_json::to_vec_pretty(meta).map_err(|e| AssetError(e.to_string()))?;
        std::fs::write(&temp, json)?;
        std::fs::rename(temp, meta_path)?;
    } else {
        let mut stored: AssetMeta = serde_json::from_slice(&std::fs::read(&meta_path)?)
            .map_err(|error| AssetError(error.to_string()))?;
        let mut changed = false;
        for tag in &meta.tags {
            if !stored.tags.contains(tag) {
                stored.tags.push(tag.clone());
                changed = true;
            }
        }
        if changed {
            stored.tags.sort();
            let temp = store_dir.join(format!(".{}.meta.tmp", meta.id));
            let json = serde_json::to_vec_pretty(&stored)
                .map_err(|error| AssetError(error.to_string()))?;
            std::fs::write(&temp, json)?;
            std::fs::rename(temp, meta_path)?;
        }
    }
    Ok(())
}

/// Ergänzt lokale Suchbegriffe, ohne die content-adressierten Asset-Bytes oder
/// ihre ID zu verändern. Bestehende Tags bleiben erhalten.
pub fn add_asset_tags<'a>(
    store_dir: &Path,
    id: &AssetId,
    sources: impl IntoIterator<Item = &'a str>,
) -> Result<AssetMeta, AssetError> {
    let mut meta = asset_meta(store_dir, id)?;
    let mut changed = false;
    for tag in derive_tags(sources) {
        if !meta.tags.contains(&tag) {
            meta.tags.push(tag);
            changed = true;
        }
    }
    if changed {
        meta.tags.sort();
        let path = store_dir.join(format!("{id}.meta.json"));
        let temp = store_dir.join(format!(".{id}.meta.tmp"));
        let bytes = serde_json::to_vec_pretty(&meta).map_err(|e| AssetError(e.to_string()))?;
        std::fs::write(&temp, bytes)?;
        std::fs::rename(temp, path)?;
    }
    Ok(meta)
}

/// Zerlegt Namen und Beschreibung in robuste, kleingeschriebene Such-Tags.
/// Sehr kurze Fragmente und reine Satzzeichen werden ausgelassen.
pub fn derive_tags<'a>(sources: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut tags = Vec::new();
    for source in sources {
        for word in source
            .split(|ch: char| !ch.is_alphanumeric())
            .map(str::to_lowercase)
            .filter(|word| word.chars().count() >= 2)
            .filter(|word| {
                !matches!(
                    word.as_str(),
                    "der"
                        | "die"
                        | "das"
                        | "den"
                        | "dem"
                        | "des"
                        | "ein"
                        | "eine"
                        | "einer"
                        | "einem"
                        | "einen"
                        | "und"
                        | "oder"
                        | "für"
                        | "von"
                        | "mit"
                        | "sein"
                        | "seine"
                        | "seinen"
                )
            })
        {
            if !tags.contains(&word) {
                tags.push(word);
            }
        }
    }
    tags.sort();
    tags
}

/// Erzeugt bzw. lädt ein lokales 160×120-Thumbnail. Es ist ein vollständig
/// abgeleiteter Cache und wird deshalb nicht als eigenes Asset synchronisiert.
pub fn asset_thumbnail(store_dir: &Path, id: &AssetId) -> Result<Vec<u8>, AssetError> {
    let cache = store_dir.join(format!("{id}.thumb.png"));
    if let Ok(bytes) = std::fs::read(&cache) {
        return Ok(bytes);
    }
    let meta = asset_meta(store_dir, id)?;
    let bytes = load_asset(store_dir, id)?;
    let image = match meta.kind {
        AssetKind::Image => image::load_from_memory(&bytes)
            .map_err(|error| AssetError(error.to_string()))?
            .resize(160, 120, image::imageops::FilterType::Triangle)
            .to_luma8(),
        AssetKind::SvgSource | AssetKind::DxfSource => vector_thumbnail(&bytes, &meta.ext)?,
        AssetKind::Font => return Err(AssetError("Fonts haben kein Katalog-Thumbnail".into())),
    };
    let mut png = Vec::new();
    image::DynamicImage::ImageLuma8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|error| AssetError(error.to_string()))?;
    let temp = store_dir.join(format!(".{id}.thumb.tmp"));
    std::fs::write(&temp, &png)?;
    std::fs::rename(temp, cache)?;
    Ok(png)
}

fn vector_thumbnail(bytes: &[u8], ext: &str) -> Result<image::GrayImage, AssetError> {
    let contours =
        crate::import::import_vector(bytes, ext).map_err(|error| AssetError(error.to_string()))?;
    let points: Vec<_> = contours
        .iter()
        .flat_map(|(points, _)| points.iter())
        .collect();
    let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let span_x = (max_x - min_x).max(1e-9);
    let span_y = (max_y - min_y).max(1e-9);
    let scale = (144.0 / span_x).min(104.0 / span_y);
    let ox = (160.0 - span_x * scale) * 0.5;
    let oy = (120.0 - span_y * scale) * 0.5;
    let map = |p: &crate::geometry::Pt| {
        (
            (ox + (p.0 - min_x) * scale) as i32,
            (oy + (p.1 - min_y) * scale) as i32,
        )
    };
    let mut out = image::GrayImage::from_pixel(160, 120, image::Luma([245]));
    for (points, closed) in contours {
        for pair in points.windows(2) {
            draw_line(&mut out, map(&pair[0]), map(&pair[1]));
        }
        if closed && points.len() > 2 {
            draw_line(&mut out, map(points.last().unwrap()), map(&points[0]));
        }
    }
    Ok(out)
}

fn draw_line(image: &mut image::GrayImage, from: (i32, i32), to: (i32, i32)) {
    let (mut x0, mut y0) = from;
    let (x1, y1) = to;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        if x0 >= 0 && y0 >= 0 && x0 < image.width() as i32 && y0 < image.height() as i32 {
            image.put_pixel(x0 as u32, y0 as u32, image::Luma([32]));
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let twice = 2 * error;
        if twice >= dy {
            error += dy;
            x0 += sx;
        }
        if twice <= dx {
            error += dx;
            y0 += sy;
        }
    }
}

/// Lädt die (Graustufen-)Bytes eines Assets.
pub fn load_asset(store_dir: &Path, id: &AssetId) -> Result<Vec<u8>, AssetError> {
    let meta = asset_meta(store_dir, id)?;
    let path = store_dir.join(format!("{id}.{}", meta.ext));
    std::fs::read(&path).map_err(|e| AssetError(format!("Asset {id} nicht lesbar: {e}")))
}

/// Lädt ein Asset und dekodiert es zu **Graustufen-Pixeln** (row-major `u8`)
/// samt Pixelmaßen `(pixels, width, height)`. Für den Job-Rasterpfad: der Core
/// hält so die Bilddekodierung, das Tauri-Backend liefert nur den Store-Pfad
/// (CLAUDE.md Regel 2). Fehlt das Asset oder ist es nicht dekodierbar ⇒ Fehler.
pub fn load_asset_luma(store_dir: &Path, id: &AssetId) -> Result<(Vec<u8>, u32, u32), AssetError> {
    let bytes = load_asset(store_dir, id)?;
    let luma = image::load_from_memory(&bytes)
        .map_err(|e| AssetError(e.to_string()))?
        .to_luma8();
    let (w, h) = (luma.width(), luma.height());
    Ok((luma.into_raw(), w, h))
}

/// Lädt die Metadaten eines Assets.
pub fn asset_meta(store_dir: &Path, id: &AssetId) -> Result<AssetMeta, AssetError> {
    let path = store_dir.join(format!("{id}.meta.json"));
    let json = std::fs::read_to_string(&path)
        .map_err(|e| AssetError(format!("Metadaten {id} nicht lesbar: {e}")))?;
    serde_json::from_str(&json).map_err(|e| AssetError(e.to_string()))
}

/// Pfad zu den Asset-Bytes (`<store>/<id>.png`) oder `None`, wenn nicht vorhanden.
pub fn asset_path(store_dir: &Path, id: &AssetId) -> Option<PathBuf> {
    let meta = asset_meta(store_dir, id).ok()?;
    let p = store_dir.join(format!("{id}.{}", meta.ext));
    p.exists().then_some(p)
}

// ── Nicht-destruktive Bildverarbeitung (ADR 0004 §3) ─────────────────────────
//
// Neu implementiert statt aus ThorBurn kopiert (CLAUDE.md Regel 6). Der brauchbare
// Kern dort war die Tonwert-LUT (Helligkeit → Kontrast → Gamma → Invert); die
// bilden wir hier sauber ab und ergänzen den Schwellwert. Alles arbeitet auf
// Graustufen-`u8`-Pixeln (das Store-Asset ist bereits grau).

/// Baut eine 256-Einträge-LUT aus den Tonwert-Parametern: Helligkeit (additiv),
/// Kontrast (um Mittelpunkt 128), Gamma (Potenz), dann optional Invert.
/// `invert` wird explizit übergeben, weil Editor- und Laser-Vorschau
/// unterschiedliche Invert-Flags nutzen (ADR 0004 §3).
fn build_lut(p: &ImageParams, invert: bool) -> [u8; 256] {
    let mut lut = [0u8; 256];
    let gamma = p.gamma.clamp(0.01, 10.0);
    let factor = (p.contrast as f32 + 100.0) / 100.0;
    for (i, out) in lut.iter_mut().enumerate() {
        // 1. Helligkeit: additiver Offset (−100..+100 → ±255).
        let v = i as f32 + p.brightness as f32 * 2.55;
        // 2. Kontrast: Skalierung um Mittelpunkt 128.
        let v = (v - 128.0) * factor + 128.0;
        // 3. Gamma: Potenz auf normiertem Wert.
        let v = (v.clamp(0.0, 255.0) / 255.0).powf(1.0 / gamma as f32) * 255.0;
        // 4. Invert.
        let v = if invert { 255.0 - v } else { v };
        *out = v.clamp(0.0, 255.0) as u8;
    }
    lut
}

/// Lädt ein Asset, wendet die Parameter an und gibt das Ergebnis als **PNG**
/// zurück (für Canvas-/Editor-Vorschau im Frontend). `invert` wählt Editor- oder
/// Laser-Invert. Das Store-Asset bleibt unverändert (nur gelesen).
pub fn rendered_png(
    store_dir: &Path,
    id: &AssetId,
    p: &ImageParams,
    invert: bool,
) -> Result<Vec<u8>, AssetError> {
    let bytes = load_asset(store_dir, id)?;
    let luma = image::load_from_memory(&bytes)
        .map_err(|e| AssetError(e.to_string()))?
        .to_luma8();
    let (w, h) = (luma.width(), luma.height());
    let mut processed = apply_params(luma.as_raw(), p, invert);
    // Dither-Modi: die Vorschau zeigt das Punktmuster (auf nativer Auflösung;
    // die Job-Auflösung zeigt der Laser-Preview-Reiter).
    if crate::dither::is_dither(p.mode) {
        processed = crate::dither::dither(&processed, w as usize, h as usize, p.mode);
    }
    let out_img = image::GrayImage::from_raw(w, h, processed)
        .ok_or_else(|| AssetError("Pixelanzahl passt nicht zur Größe".into()))?;
    let mut out = Vec::new();
    image::DynamicImage::ImageLuma8(out_img)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| AssetError(e.to_string()))?;
    Ok(out)
}

/// Wendet die Bildverarbeitung auf Graustufen-Pixel an (row-major `u8`):
/// Tonwert-LUT und — bei `ImageMode::Threshold` — die harte Schwelle. `invert`
/// bestimmt, ob invertiert wird (Aufrufer wählt Editor- oder Laser-Flag).
/// Gibt ein neues Pixel-Array gleicher Länge zurück; das Original bleibt.
///
/// Dither-Modi liefern hier nur die **LUT-Graustufe** — das Dithern selbst
/// braucht die Bildmaße und passiert in `dither::dither` (auf Zielauflösung,
/// raster.rs) bzw. für die Editor-Vorschau in `rendered_png`.
pub fn apply_params(pixels: &[u8], p: &ImageParams, invert: bool) -> Vec<u8> {
    let lut = build_lut(p, invert);
    match p.mode {
        ImageMode::Threshold => pixels
            .iter()
            .map(|&v| {
                if lut[v as usize] >= p.threshold {
                    255
                } else {
                    0
                }
            })
            .collect(),
        // Grayscale + alle Dither-Modi: nur die Tonwert-LUT.
        _ => pixels.iter().map(|&v| lut[v as usize]).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Baut ein winziges farbiges 2×1-PNG (rot, grün) in Bytes.
    fn tiny_color_png() -> Vec<u8> {
        let mut img = image::RgbImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        img.put_pixel(1, 0, image::Rgb([0, 255, 0]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn hash_ist_deterministisch_und_inhaltsabhaengig() {
        let a = content_hash(b"hallo");
        let b = content_hash(b"hallo");
        let c = content_hash(b"welt");
        assert_eq!(a, b, "gleiche Bytes ⇒ gleicher Hash");
        assert_ne!(a, c, "andere Bytes ⇒ anderer Hash");
        assert_eq!(a.len(), 16, "FNV-64 hex = 16 Zeichen");
    }

    #[test]
    fn grayscale_konvertiert_farbe_zu_luma() {
        let (png, w, h, fmt) = to_grayscale_png(&tiny_color_png()).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(fmt, "png");
        // Erneut dekodieren: muss ein Luma-Bild sein. `image` nutzt die
        // gamma-korrekte (sRGB-linearisierte) Luminanz, nicht die naive
        // Rec.601-Formel — deshalb ist Grün deutlich heller als Rot.
        let img = image::load_from_memory(&png).unwrap();
        let luma = img.to_luma8();
        let (rot, gruen) = (luma.get_pixel(0, 0).0[0], luma.get_pixel(1, 0).0[0]);
        assert!(rot < gruen, "Grün heller als Rot (Luminanzgewichtung)");
        assert!(rot > 0 && gruen < 255, "keine reinen Extremwerte");
    }

    #[test]
    fn apply_params_identitaet_laesst_grau_unveraendert() {
        let p = ImageParams::default(); // Grayscale, keine Tonwertänderung, kein Invert
        let input: Vec<u8> = (0..=255).collect();
        assert_eq!(apply_params(&input, &p, false), input);
    }

    #[test]
    fn apply_params_invert() {
        let p = ImageParams::default();
        let out = apply_params(&[0, 128, 255], &p, true);
        assert_eq!(out[0], 255);
        assert_eq!(out[2], 0);
    }

    #[test]
    fn apply_params_threshold_macht_schwarz_weiss() {
        let p = ImageParams {
            mode: ImageMode::Threshold,
            threshold: 128,
            ..Default::default()
        };
        let out = apply_params(&[0, 100, 130, 200, 255], &p, false);
        // Nur Werte ≥ 128 werden weiß (255), Rest schwarz (0).
        assert_eq!(out, vec![0, 0, 255, 255, 255]);
    }

    #[test]
    fn apply_params_brightness_hebt_an() {
        let p = ImageParams {
            brightness: 50,
            ..Default::default()
        };
        assert!(apply_params(&[0], &p, false)[0] > 0);
    }

    #[test]
    fn import_legt_asset_und_meta_ab_und_ist_idempotent() {
        let dir = std::env::temp_dir().join(format!("luxifer_assets_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let src = tiny_color_png();
        let meta = import_image(&dir, &src, "rot_gruen.png").unwrap();
        assert_eq!(meta.width, 2);
        assert_eq!(meta.original_name, "rot_gruen.png");
        assert!(asset_path(&dir, &meta.id).is_some());

        // Geladene Bytes = Graustufe, wieder dekodierbar.
        let bytes = load_asset(&dir, &meta.id).unwrap();
        assert!(image::load_from_memory(&bytes).is_ok());
        let m2 = asset_meta(&dir, &meta.id).unwrap();
        assert_eq!(m2.id, meta.id);

        // Zweiter Import derselben Quelle ⇒ selbe ID, kein zweites Asset.
        let meta_again = import_image(&dir, &src, "kopie.png").unwrap();
        assert_eq!(meta_again.id, meta.id, "gleiches Bild = ein Asset");
        let count = std::fs::read_dir(&dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "png")
            })
            .count();
        assert_eq!(count, 1, "nur ein PNG im Store");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn quell_asset_bleibt_unveraendert_und_wird_gelistet() {
        let dir = std::env::temp_dir().join(format!("luxifer_sources_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let source = br#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0L1 1"/></svg>"#;

        let meta = import_source(&dir, source, "kontur.svg", "svg", AssetKind::SvgSource)
            .expect("SVG katalogisieren");
        assert_eq!(load_asset(&dir, &meta.id).unwrap(), source);
        assert_eq!(list_assets(&dir).unwrap(), vec![meta.clone()]);
        assert_eq!(
            asset_path(&dir, &meta.id).unwrap().extension().unwrap(),
            "svg"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empfangenes_asset_wird_nur_mit_passendem_hash_gespeichert() {
        let dir = std::env::temp_dir().join(format!("luxifer_received_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let bytes = b"font-data";
        let meta = AssetMeta {
            id: content_hash(bytes),
            ext: "ttf".into(),
            kind: AssetKind::Font,
            original_name: "Werkstatt.ttf".into(),
            source_format: String::new(),
            width: 0,
            height: 0,
            import_at: String::new(),
            tags: vec!["werkstatt".into()],
        };

        store_asset(&dir, &meta, bytes).expect("gültiges Asset speichern");
        assert_eq!(load_asset(&dir, &meta.id).unwrap(), bytes);
        assert!(store_asset(&dir, &meta, b"manipuliert").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tags_kommen_aus_dateiname_projekt_und_beschreibung() {
        assert_eq!(
            derive_tags([
                "Heinz-Untersetzer_v2.svg",
                "Untersetzer für Heinz seinen Geburtstag"
            ]),
            vec!["geburtstag", "heinz", "svg", "untersetzer", "v2"]
        );
    }

    #[test]
    fn asset_tags_werden_idempotent_ergaenzt() {
        let dir = std::env::temp_dir().join(format!("luxifer_tags_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let bytes = b"vector-source";
        let meta = import_source(&dir, bytes, "eiche.svg", "svg", AssetKind::SvgSource).unwrap();
        add_asset_tags(&dir, &meta.id, ["Heinz Untersetzer", "Eiche rund"]).unwrap();
        add_asset_tags(&dir, &meta.id, ["Heinz Untersetzer"]).unwrap();
        let stored = asset_meta(&dir, &meta.id).unwrap();
        assert!(stored.tags.contains(&"heinz".into()));
        assert!(stored.tags.contains(&"untersetzer".into()));
        assert_eq!(stored.tags.iter().filter(|tag| *tag == "heinz").count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn vektor_thumbnail_wird_lokal_gecacht() {
        let dir = std::env::temp_dir().join(format!("luxifer_thumb_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect x="2" y="2" width="96" height="46"/></svg>"#;
        let meta = import_source(&dir, svg, "rahmen.svg", "svg", AssetKind::SvgSource).unwrap();
        let first = asset_thumbnail(&dir, &meta.id).unwrap();
        let second = asset_thumbnail(&dir, &meta.id).unwrap();
        assert_eq!(first, second);
        let image = image::load_from_memory(&first).unwrap();
        assert_eq!((image.width(), image.height()), (160, 120));
        assert!(dir.join(format!("{}.thumb.png", meta.id)).exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
