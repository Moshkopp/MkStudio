//! Bild-Import und -Darstellung (ADR 0004): Import in den Asset-Store + Platzierung,
//! neutrales Rendern als Data-URL fürs Canvas/Editor-Vorschau, Editor-Parameter.

use luxifer_core::{assets_dir, import_image, rendered_png, Geo, ImageParams};
use tauri::State;

use crate::shared::{base64_encode, scene_with, AppData, Scene};

/// Importiert ein Bild (ADR 0004): legt die (Graustufen-)Kopie im Asset-Store ab
/// und fügt ein Bild-Objekt auf einem eigenen Image-Layer ein. `bytes` sind die
/// rohen Bytes der vom Nutzer gewählten Datei (das Frontend liest sie über einen
/// `<input type=file>` — kein Tauri-Dialog nötig), `name` der Anzeigename.
///
/// Die Zielgröße in mm ergibt sich aus den Pixelmaßen bei 96 DPI, begrenzt auf
/// 80 % der Bettgröße (ein 4K-Bild soll nicht riesig platziert werden), und wird
/// mittig aufs Bett gesetzt. Seitenverhältnis bleibt erhalten.
#[tauri::command]
pub fn import_image_file(
    data: State<AppData>,
    bytes: Vec<u8>,
    name: String,
) -> Result<Scene, String> {
    let meta = import_image(&assets_dir(), &bytes, &name).map_err(|e| e.to_string())?;

    let mut s = data.state();
    // px → mm bei 96 DPI.
    const PX_TO_MM: f64 = 25.4 / 96.0;
    let mut w = meta.width as f64 * PX_TO_MM;
    let mut h = meta.height as f64 * PX_TO_MM;
    // Auf 80 % der Bettgröße begrenzen, Seitenverhältnis wahren.
    let max_w = s.bed_w_mm * 0.8;
    let max_h = s.bed_h_mm * 0.8;
    if w > max_w || h > max_h {
        let scale = (max_w / w).min(max_h / h);
        w *= scale;
        h *= scale;
    }
    // Mittig aufs Bett.
    let x = (s.bed_w_mm - w) / 2.0;
    let y = (s.bed_h_mm - h) / 2.0;
    s.add_image(meta.id, x, y, w, h);
    Ok(scene_with(&s, &data))
}

/// Rendert ein Asset mit den gegebenen Parametern und gibt es als PNG-Data-URL
/// zurück (Canvas-Darstellung bzw. Editor-Vorschau). `invert` = Editor- oder
/// Laser-Invert (der Aufrufer wählt); für die Canvas-Anzeige `invert_editor`.
#[tauri::command]
pub fn image_render(asset: String, params: ImageParams, invert: bool) -> Option<String> {
    let png = rendered_png(&assets_dir(), &asset, &params, invert).ok()?;
    Some(format!("data:image/png;base64,{}", base64_encode(&png)))
}

/// Setzt die Bild-Parameter eines Bild-Shapes (Editor). `index` ist der
/// Shape-Index; nicht-Bild-Shapes werden ignoriert.
#[tauri::command]
pub fn set_image_params(data: State<AppData>, index: usize, params: ImageParams) -> Scene {
    let mut s = data.state();
    if let Some(shape) = s.shapes.get_mut(index) {
        if let Geo::Image { params: p, .. } = &mut shape.geo {
            *p = params;
        }
    }
    scene_with(&s, &data)
}
