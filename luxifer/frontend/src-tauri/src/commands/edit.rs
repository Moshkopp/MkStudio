//! Auswahl & Bearbeitung: Farbe aktivieren (Farbe=Layer), Auswahl (Punkt/Marquee),
//! Gruppieren, Verschieben/Skalieren/Spiegeln, Ausrichten/Verteilen, Löschen und
//! die Layer-Verwaltung (Parameter, Sichtbarkeit, Reihenfolge).

use tauri::State;

use crate::shared::{scene_with, AppData, Scene};

#[tauri::command]
pub fn activate_color(data: State<AppData>, color: [u8; 3]) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.activate_color(color);
    scene_with(&s, &data)
}

#[tauri::command]
pub fn select_at(data: State<AppData>, x: f64, y: f64, tol: f64, additive: bool) -> Scene {
    let mut s = data.state.lock().unwrap();
    match s.hit_test(x, y, tol) {
        Some(idx) => {
            if additive {
                // Toggle: enthalten → raus, sonst rein.
                if let Some(pos) = s.selected.iter().position(|&i| i == idx) {
                    s.selected.remove(pos);
                } else {
                    s.selected.push(idx);
                }
            } else if !s.selected.contains(&idx) {
                s.selected = vec![idx];
            }
        }
        None => {
            if !additive {
                s.selected.clear();
            }
        }
    }
    // Gruppen sind eine Einheit: Auswahl auf ganze Gruppen erweitern.
    s.expand_selection_to_groups();
    scene_with(&s, &data)
}

/// Marquee-Auswahl: alle Shapes, deren BBox vollständig im Rechteck liegt.
#[tauri::command]
pub fn select_rect(data: State<AppData>, x1: f64, y1: f64, x2: f64, y2: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.select_in_rect(x1, y1, x2, y2);
    s.expand_selection_to_groups();
    scene_with(&s, &data)
}

/// Gruppiert die Auswahl (Shapes verhalten sich danach als Einheit).
#[tauri::command]
pub fn group_op(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.group_selected();
    scene_with(&s, &data)
}

/// Löst die Gruppierung der Auswahl.
#[tauri::command]
pub fn ungroup_op(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.ungroup_selected();
    scene_with(&s, &data)
}

/// Verschiebt die Auswahl um ein Gesamt-Delta (ein Undo-Punkt pro Geste).
#[tauri::command]
pub fn move_selected(data: State<AppData>, dx: f64, dy: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    if dx != 0.0 || dy != 0.0 {
        s.push_undo();
        s.translate_selected(dx, dy);
    }
    scene_with(&s, &data)
}

/// Skaliert die Auswahl von der Start-Gruppenbox auf die Zielbox (ein Undo-Punkt).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn scale_selected(
    data: State<AppData>,
    sx: f64,
    sy: f64,
    sw: f64,
    sh: f64,
    tx: f64,
    ty: f64,
    tw: f64,
    th: f64,
) -> Scene {
    use luxifer_core::BBox;
    let mut s = data.state.lock().unwrap();
    s.push_undo();
    s.scale_selection_to(BBox::new(sx, sy, sw, sh), BBox::new(tx, ty, tw, th));
    scene_with(&s, &data)
}

#[tauri::command]
pub fn align(data: State<AppData>, kind: String) -> Scene {
    use luxifer_core::Align;
    let mut s = data.state.lock().unwrap();
    let k = match kind.as_str() {
        "left" => Align::Left,
        "hcenter" => Align::HCenter,
        "right" => Align::Right,
        "top" => Align::Top,
        "vcenter" => Align::VCenter,
        "bottom" => Align::Bottom,
        "center" => Align::Center,
        _ => return scene_with(&s, &data),
    };
    s.align_selection(k);
    scene_with(&s, &data)
}

#[tauri::command]
pub fn distribute(data: State<AppData>, kind: String) -> Scene {
    use luxifer_core::Distribute;
    let mut s = data.state.lock().unwrap();
    let k = match kind.as_str() {
        "h" => Distribute::Horizontal,
        "v" => Distribute::Vertical,
        "space-h" => Distribute::SpaceHorizontal,
        "space-v" => Distribute::SpaceVertical,
        _ => return scene_with(&s, &data),
    };
    s.distribute_selection(k);
    scene_with(&s, &data)
}


/// Spiegelt die Auswahl an der Mittelachse ihrer gemeinsamen BBox.
/// `axis`: "h" = horizontal spiegeln (links↔rechts, vertikale Achse),
/// "v" = vertikal spiegeln (oben↔unten, horizontale Achse).
#[tauri::command]
pub fn mirror(data: State<AppData>, axis: String) -> Scene {
    use luxifer_core::Axis;
    let mut s = data.state.lock().unwrap();
    let a = match axis.as_str() {
        "h" => Axis::Vertical,
        "v" => Axis::Horizontal,
        _ => return scene_with(&s, &data),
    };
    s.mirror_selection(a);
    scene_with(&s, &data)
}

#[tauri::command]
pub fn clear_selection(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.selected.clear();
    scene_with(&s, &data)
}

#[tauri::command]
pub fn delete_selected(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.delete_selected();
    scene_with(&s, &data)
}

/// Vom Frontend gelieferte Layer-Parameter (Doppelklick-Dialog).
#[derive(serde::Deserialize)]
pub struct LayerParams {
    name: String,
    mode: String,
    speed_mm_s: f64,
    power_pct: f64,
    min_power_pct: f64,
    passes: u32,
    air_assist: bool,
    line_step_mm: f64,
    dpi: f64,
    #[serde(default = "default_bidirectional")]
    bidirectional: bool,
}

pub fn default_bidirectional() -> bool {
    true
}

/// Setzt die Parameter eines Layers (ein Undo-Punkt).
#[tauri::command]
pub fn set_layer_params(data: State<AppData>, index: usize, p: LayerParams) -> Scene {
    use luxifer_core::LayerMode;
    let mut s = data.state.lock().unwrap();
    if index < s.layers.len() {
        s.push_undo();
        let l = &mut s.layers[index];
        l.name = p.name;
        l.mode = match p.mode.as_str() {
            "Fill" => LayerMode::Fill,
            "Raster" => LayerMode::Raster,
            "Image" => LayerMode::Image,
            _ => LayerMode::Cut,
        };
        l.speed_mm_s = p.speed_mm_s;
        l.power_pct = p.power_pct;
        l.min_power_pct = p.min_power_pct;
        l.passes = p.passes;
        l.air_assist = p.air_assist;
        l.line_step_mm = p.line_step_mm;
        l.dpi = p.dpi;
        l.bidirectional = p.bidirectional;
    }
    scene_with(&s, &data)
}

/// Schalter eines Layers umschalten (Anzeige, Brennen, Luft, Sperre).
#[tauri::command]
pub fn toggle_layer(data: State<AppData>, index: usize, field: String) -> Scene {
    let mut s = data.state.lock().unwrap();
    if let Some(l) = s.layers.get_mut(index) {
        match field.as_str() {
            "visible" => l.visible = !l.visible,          // Objekte anzeigen
            "enabled" => l.enabled = !l.enabled,          // im Job mitbrennen
            "air_assist" => l.air_assist = !l.air_assist, // Luftunterstützung
            "locked" => l.locked = !l.locked,             // Editiersperre
            _ => {}
        }
    }
    scene_with(&s, &data)
}

/// Verschiebt einen Layer in der Brenn-Reihenfolge (ADR 0005 §0). `from`/`to`
/// sind Layer-Indizes; der Core remappt dabei alle `shape.layer_id`. Ein
/// Undo-Punkt entsteht nur bei tatsächlicher Bewegung.
#[tauri::command]
pub fn move_layer(data: State<AppData>, from: usize, to: usize) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.move_layer(from, to);
    scene_with(&s, &data)
}
