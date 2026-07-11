//! Formen anlegen und Pfade bearbeiten: primitive/parametrische Formen, Spline,
//! Bézier & Knoten-Editor, Text→Pfad (+ Fonts), Vektor-Import, Muster-Füllung,
//! Boolean/Offset/Fillet/Haltesteg, Trace und Nesting/Untersetzer.

use luxifer_core::{assets_dir, Geo, PolyShape, ShapeInfo};
use serde::Serialize;
use tauri::State;

use crate::shared::{scene_with, AppData, Scene};

/// Importiert eine Vektordatei (SVG/DXF): Konturen als Polylinien auf dem
/// aktiven Layer (ein Undo-Punkt). Die Endung des Dateinamens entscheidet.
#[tauri::command]
pub fn import_vector_file(
    data: State<AppData>,
    bytes: Vec<u8>,
    name: String,
) -> Result<Scene, String> {
    let ext = name.rsplit('.').next().unwrap_or("");
    let contours = luxifer_core::import::import_vector(&bytes, ext).map_err(|e| e.to_string())?;
    let mut s = data.state();
    s.add_polylines(contours);
    Ok(scene_with(&s, &data))
}

/// Füllt die Auswahl mit einem Muster (Pattern-Fill, wie v1: Linien, Kreise,
/// Slots, Waben). Alle selektierten geschlossenen Konturen wirken gemeinsam
/// als Ringe (innere = Löcher). Ein Undo-Punkt; die Konturen bleiben.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn pattern_fill_op(
    data: State<AppData>,
    pattern: String,
    gap_x: f64,
    gap_y: f64,
    angle: f64,
    size: f64,
) -> Result<Scene, String> {
    use luxifer_core::pattern_fill::{FillParams, Pattern};
    let Some(pat) = Pattern::from_key(&pattern) else {
        return Err(format!("Unbekanntes Muster: {pattern}"));
    };
    let mut s = data.state();
    s.pattern_fill_selected(&FillParams {
        pattern: pat,
        gap_x,
        gap_y,
        angle_deg: angle,
        size,
    });
    Ok(scene_with(&s, &data))
}

/// Fügt eine Spline hinzu: Catmull-Rom-Kurve durch die geklickten Punkte
/// (Zeichenfluss wie die Polylinie; die Glättung passiert im Core).
#[tauri::command]
pub fn add_spline(data: State<AppData>, pts: Vec<(f64, f64)>, closed: bool) -> Scene {
    use luxifer_core::geometry::catmull_rom;
    let mut s = data.state();
    let smooth = catmull_rom(&pts, closed, 12);
    s.add_shape(Geo::Polyline {
        pts: smooth,
        closed,
    });
    scene_with(&s, &data)
}

/// Eigener Fonts-Ordner der App (<data_root>/Fonts) — wie v3s Fonts-Ablage.
pub fn fonts_dir() -> std::path::PathBuf {
    luxifer_core::data_root().join("Fonts")
}

/// Installiert einen Font (TTF/OTF-Bytes) in den App-Fonts-Ordner.
#[tauri::command]
pub fn upload_font(bytes: Vec<u8>, name: String) -> Result<String, String> {
    // Vorab prüfen, dass der Font lesbar ist (sonst Datenmüll im Ordner).
    luxifer_core::text::text_to_contours(&bytes, "Ag", 10.0).map_err(|e| e.to_string())?;
    let dir = fonts_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe = name.replace(['/', '\\'], "_");
    let path = dir.join(&safe);
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Ein installierter Font (fürs Text-Werkzeug).
#[derive(Serialize)]
pub struct FontInfo {
    name: String,
    path: String,
}

/// Listet die System-Fonts (TTF/OTF) aus den üblichen Verzeichnissen.
#[tauri::command]
pub fn list_fonts() -> Vec<FontInfo> {
    let home = std::env::var("HOME").unwrap_or_default();
    // Eigene Fonts der App zuerst (erscheinen oben in der Liste).
    let dirs = [
        fonts_dir().to_string_lossy().to_string(),
        "/usr/share/fonts".to_string(),
        "/usr/local/share/fonts".to_string(),
        format!("{home}/.fonts"),
        format!("{home}/.local/share/fonts"),
    ];
    let mut out: Vec<FontInfo> = Vec::new();
    for dir in dirs {
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "ttf" || x == "otf") {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        out.push(FontInfo {
                            name: stem.to_string(),
                            path: p.to_string_lossy().to_string(),
                        });
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// Fügt Text als Vektorpfade ein (Text→Pfad, ein Undo-Punkt). Der Text landet
/// bei 10 % der Bettmaße; verschieben/skalieren wie jede andere Form.
#[tauri::command]
pub fn add_text(
    data: State<AppData>,
    text: String,
    font_path: String,
    size_mm: f64,
) -> Result<Scene, String> {
    if text.trim().is_empty() {
        return Err("Kein Text eingegeben.".into());
    }
    let bytes = std::fs::read(&font_path).map_err(|e| format!("Font nicht lesbar: {e}"))?;
    let contours = luxifer_core::text::text_to_contours(&bytes, &text, size_mm.clamp(1.0, 500.0))
        .map_err(|e| e.to_string())?;
    if contours.is_empty() {
        return Err("Der Font liefert für diesen Text keine Konturen.".into());
    }
    let mut s = data.state();
    let (ox, oy) = (s.bed_w_mm * 0.1, s.bed_h_mm * 0.1);
    let placed: Vec<(Vec<(f64, f64)>, bool)> = contours
        .into_iter()
        .map(|(c, closed)| {
            (
                c.into_iter().map(|(x, y)| (x + ox, y + oy)).collect(),
                closed,
            )
        })
        .collect();
    // Als Text-Block: eine Gruppe + Quelldaten fürs spätere Editieren.
    s.add_text_block(
        placed,
        luxifer_core::TextMeta {
            text,
            font_path,
            size_mm,
        },
    );
    Ok(scene_with(&s, &data))
}

/// Vorschau-Konturen für den Text-Dialog (mm, Ursprung oben links). Reine
/// Anzeige — die Wahrheit erzeugt add_text/update_text im Core.
#[tauri::command]
pub fn text_preview(
    text: String,
    font_path: String,
    size_mm: f64,
) -> Result<Vec<(Vec<(f64, f64)>, bool)>, String> {
    let bytes = std::fs::read(&font_path).map_err(|e| e.to_string())?;
    luxifer_core::text::text_to_contours(&bytes, &text, size_mm.clamp(1.0, 500.0))
        .map_err(|e| e.to_string())
}

/// Ersetzt einen bestehenden Text-Block (Doppelklick-Edit): neuer Text/Font/
/// Größe, gleiche Position, gleicher Layer.
#[tauri::command]
pub fn update_text(
    data: State<AppData>,
    shape_index: usize,
    text: String,
    font_path: String,
    size_mm: f64,
) -> Result<Scene, String> {
    if text.trim().is_empty() {
        return Err("Kein Text eingegeben.".into());
    }
    let bytes = std::fs::read(&font_path).map_err(|e| format!("Font nicht lesbar: {e}"))?;
    let contours = luxifer_core::text::text_to_contours(&bytes, &text, size_mm.clamp(1.0, 500.0))
        .map_err(|e| e.to_string())?;
    if contours.is_empty() {
        return Err("Der Font liefert für diesen Text keine Konturen.".into());
    }
    let mut s = data.state();
    s.replace_text_block(
        shape_index,
        contours,
        luxifer_core::TextMeta {
            text,
            font_path,
            size_mm,
        },
    );
    Ok(scene_with(&s, &data))
}

/// Fügt eine Bézier-Feder aus den geklickten Punkten ein (glatte Kurve durch
/// alle Punkte, editierbare Knoten).
#[tauri::command]
pub fn add_bezier(data: State<AppData>, pts: Vec<(f64, f64)>, closed: bool) -> Scene {
    let mut s = data.state();
    s.add_bezier(pts, closed);
    scene_with(&s, &data)
}

/// Fügt eine Bézier-Feder aus fertigen Knoten ein (Inkscape-Feder-Stil).
#[tauri::command]
pub fn add_bezier_nodes(
    data: State<AppData>,
    nodes: Vec<luxifer_core::bezier::BezierNode>,
    closed: bool,
) -> Scene {
    let mut s = data.state();
    s.add_bezier_nodes(nodes, closed);
    scene_with(&s, &data)
}

/// Node-Editor: Anker/Handle eines Bézier-Knotens ziehen. `part` = "anchor" |
/// "in" | "out". `begin` = true beim Drag-Start (setzt den Undo-Punkt).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn drag_node(
    data: State<AppData>,
    shape_index: usize,
    node: usize,
    part: String,
    x: f64,
    y: f64,
    begin: bool,
) -> Scene {
    use luxifer_core::bezier::NodePart;
    let np = match part.as_str() {
        "in" => NodePart::HandleIn,
        "out" => NodePart::HandleOut,
        _ => NodePart::Anchor,
    };
    let mut s = data.state();
    if begin {
        s.push_undo();
    }
    s.drag_node(shape_index, node, np, (x, y));
    scene_with(&s, &data)
}

/// Teilt das Segment ab Knoten `seg_start` (fügt einen Mittelknoten ein).
#[tauri::command]
pub fn split_node(data: State<AppData>, shape_index: usize, seg_start: usize, t: f64) -> Scene {
    let mut s = data.state();
    s.push_undo();
    s.split_node_segment(shape_index, seg_start, t);
    scene_with(&s, &data)
}

#[tauri::command]
pub fn hit_bezier_segment(
    data: State<AppData>,
    x: f64,
    y: f64,
    tolerance: f64,
) -> Option<luxifer_core::bezier::BezierSegmentHit> {
    let s = data.state();
    s.hit_bezier_segment((x, y), tolerance.max(0.001))
}

#[tauri::command]
pub fn toggle_node_smooth(data: State<AppData>, shape_index: usize, node: usize) -> Scene {
    let mut s = data.state();
    s.push_undo();
    s.toggle_node_smooth(shape_index, node);
    scene_with(&s, &data)
}

/// Löscht einen Bézier-Knoten.
#[tauri::command]
pub fn delete_node(data: State<AppData>, shape_index: usize, node: usize) -> Scene {
    let mut s = data.state();
    s.push_undo();
    s.delete_node(shape_index, node);
    scene_with(&s, &data)
}

/// Vektorisiert ein Bild-Shape (Trace): Konturen des Motivs als geschlossene
/// Polylinien in mm, auf dem aktiven Zeichen-Layer (ein Undo-Punkt). Die
/// Tonwert-LUT des Bildes (Helligkeit/Kontrast/Gamma) wirkt vor der Schwelle.
#[tauri::command]
pub fn trace_image(
    data: State<AppData>,
    shape_index: usize,
    threshold: u8,
    invert: bool,
) -> Result<Scene, String> {
    use luxifer_core::geometry::{Geo, ImageMode, ImageParams};
    use luxifer_core::trace::{trace, TraceParams};

    let mut s = data.state();
    let (asset, bx, by, bw, bh, params) = match s.shapes.get(shape_index).map(|sh| &sh.geo) {
        Some(Geo::Image {
            asset,
            x,
            y,
            w,
            h,
            params,
        }) => (asset.clone(), *x, *y, *w, *h, *params),
        _ => return Err("Kein Bild ausgewählt.".into()),
    };
    let (px, w, h) =
        luxifer_core::load_asset_luma(&assets_dir(), &asset).map_err(|e| e.to_string())?;
    // Tonwerte anwenden (nur LUT), dann tracen.
    let lut_params = ImageParams {
        mode: ImageMode::Grayscale,
        ..params
    };
    let gray = luxifer_core::apply_params(&px, &lut_params, false);
    let contours = trace(
        &gray,
        w as usize,
        h as usize,
        &TraceParams {
            threshold,
            invert,
            ..Default::default()
        },
    );
    if contours.is_empty() {
        return Err("Keine Konturen gefunden — Schwelle anpassen?".into());
    }
    // Pixel → mm über die Bildbox.
    let (sx, sy) = (bw / w as f64, bh / h as f64);
    let mm: Vec<(Vec<(f64, f64)>, bool)> = contours
        .into_iter()
        .map(|c| {
            (
                c.into_iter()
                    .map(|(x, y)| (bx + x * sx, by + y * sy))
                    .collect(),
                true,
            )
        })
        .collect();
    s.add_polylines(mm);
    Ok(scene_with(&s, &data))
}

/// Boolesche Operation auf der Auswahl: "union" | "intersect" | "diff".
/// Subjekt = zuerst selektierte Shape; die Eingaben werden ersetzt.
#[tauri::command]
pub fn boolean_op(data: State<AppData>, op: String) -> Scene {
    use luxifer_core::BoolOp;
    let mut s = data.state();
    let o = match op.as_str() {
        "union" => BoolOp::Union,
        "intersect" => BoolOp::Intersect,
        "diff" => BoolOp::Difference,
        _ => return scene_with(&s, &data),
    };
    s.boolean_selected(o);
    scene_with(&s, &data)
}

/// Parallele Kontur (mm) zu jeder selektierten Shape hinzufügen.
/// Positiv = außen, negativ = innen; das Original bleibt.
#[tauri::command]
pub fn offset_op(data: State<AppData>, dist: f64) -> Scene {
    let mut s = data.state();
    s.offset_selected(dist);
    scene_with(&s, &data)
}

/// Haltesteg: Steg-Linie (x0,y0)→(x1,y1) in mm der Breite `width` über die
/// Konturen ziehen — wo sie kreuzt, wird aufgeschnitten (Materialbrücke).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn bridge_op(data: State<AppData>, x0: f64, y0: f64, x1: f64, y1: f64, width: f64) -> Scene {
    let mut s = data.state();
    s.bridge_stroke((x0, y0), (x1, y1), width);
    scene_with(&s, &data)
}

/// Verrundet NUR die angeklickten Ecken einer Shape (Punkt-Indizes).
#[tauri::command]
pub fn fillet_corners_op(
    data: State<AppData>,
    shape_index: usize,
    corners: Vec<usize>,
    radius: f64,
) -> Scene {
    let mut s = data.state();
    s.fillet_shape_corners(shape_index, &corners, radius);
    scene_with(&s, &data)
}

/// Ecken der selektierten Shapes mit Radius (mm) verrunden.
#[tauri::command]
pub fn fillet_op(data: State<AppData>, radius: f64) -> Scene {
    let mut s = data.state();
    s.fillet_selected(radius);
    scene_with(&s, &data)
}

/// Packt die Auswahl platzsparend aufs Bett (Nesting, `gap` mm Abstand).
#[tauri::command]
pub fn nest_op(data: State<AppData>, gap: f64) -> Scene {
    let mut s = data.state();
    s.nest_selected(gap);
    scene_with(&s, &data)
}

/// Füllt das Bett mit Kopien der zuerst selektierten Form (Nesting v3-Modus).
#[tauri::command]
pub fn nest_fill_op(data: State<AppData>, gap: f64) -> Scene {
    let mut s = data.state();
    s.nest_fill_selected(gap);
    scene_with(&s, &data)
}

/// Fügt die 4×2-Untersetzer-Vorlage ein (100 mm, 20 mm Lücke, zentriert).
#[tauri::command]
pub fn insert_coasters(data: State<AppData>, round: bool) -> Scene {
    let mut s = data.state();
    s.insert_coasters(round);
    scene_with(&s, &data)
}

// ---- Formen anlegen (primitiv + parametrisch) ----------------------------

#[tauri::command]
pub fn add_rect(data: State<AppData>, x: f64, y: f64, w: f64, h: f64) -> Scene {
    let mut s = data.state();
    s.add_shape(Geo::Rect { x, y, w, h });
    scene_with(&s, &data)
}

#[tauri::command]
pub fn add_ellipse(data: State<AppData>, cx: f64, cy: f64, rx: f64, ry: f64) -> Scene {
    let mut s = data.state();
    s.add_shape(Geo::Ellipse { cx, cy, rx, ry });
    scene_with(&s, &data)
}

/// Fügt eine offene 2-Punkt-Linie als Polyline hinzu.
#[tauri::command]
pub fn add_line(data: State<AppData>, x1: f64, y1: f64, x2: f64, y2: f64) -> Scene {
    let mut s = data.state();
    s.add_shape(Geo::Polyline {
        pts: vec![(x1, y1), (x2, y2)],
        closed: false,
    });
    scene_with(&s, &data)
}

/// Fügt eine Polylinie aus den gelieferten Punkten hinzu. `closed` schließt die
/// Kontur (letzter → erster Punkt). Wird ignoriert, wenn < 2 Punkte kommen.
#[tauri::command]
pub fn add_polyline(data: State<AppData>, pts: Vec<(f64, f64)>, closed: bool) -> Scene {
    let mut s = data.state();
    if pts.len() >= 2 {
        s.add_shape(Geo::Polyline { pts, closed });
    }
    scene_with(&s, &data)
}

/// Katalog der parametrischen Formen für die Galerie im Werkzeug-Panel.
/// Datengetrieben: eine neue Form im Core erscheint hier automatisch.
#[tauri::command]
pub fn shape_catalog() -> Vec<ShapeInfo> {
    PolyShape::catalog()
}

/// Fügt eine parametrische Form als geschlossene Polylinie hinzu.
/// `shape` = stabiler Bezeichner aus dem Katalog (z. B. "hex"); unbekannte
/// Bezeichner werden ignoriert (Zustand bleibt unverändert).
#[tauri::command]
pub fn add_polygon(
    data: State<AppData>,
    shape: String,
    cx: f64,
    cy: f64,
    r: f64,
    rot: f64,
) -> Scene {
    let mut s = data.state();
    if let Some(kind) = PolyShape::from_id(&shape) {
        let pts = kind.points(cx, cy, r, rot);
        if pts.len() >= 3 {
            s.add_shape(Geo::Polyline { pts, closed: true });
        }
    }
    scene_with(&s, &data)
}
