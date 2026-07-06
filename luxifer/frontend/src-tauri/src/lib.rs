//! LuxiFer Tauri-Backend. Hält den `AppState` des Cores und stellt Commands
//! bereit. Das Frontend zeichnet nur — die gesamte Fachlogik bleibt im Core.

use std::sync::Mutex;

use luxifer_core::{AppState, Geo, Layer, Shape};
use serde::Serialize;
use tauri::{Manager, State};

/// Geteilter Zustand über alle Commands.
struct AppData {
    state: Mutex<AppState>,
}

/// Schlanke Sicht auf den Zustand fürs Frontend (ohne Undo-Stacks).
#[derive(Serialize)]
struct Scene {
    layers: Vec<Layer>,
    shapes: Vec<Shape>,
    selected: Vec<usize>,
    bed_w_mm: f64,
    bed_h_mm: f64,
}

impl Scene {
    fn from_state(s: &AppState) -> Self {
        Scene {
            layers: s.layers.clone(),
            shapes: s.shapes.clone(),
            selected: s.selected.clone(),
            bed_w_mm: s.bed_w_mm,
            bed_h_mm: s.bed_h_mm,
        }
    }
}

// Hilfsmakro-Ersatz: Zustand sperren und Scene zurückgeben.
fn scene(data: &State<AppData>) -> Scene {
    let s = data.state.lock().unwrap();
    Scene::from_state(&s)
}

#[tauri::command]
fn get_scene(data: State<AppData>) -> Scene {
    scene(&data)
}

#[tauri::command]
fn swatch_colors() -> Vec<[u8; 3]> {
    luxifer_core::SWATCH_COLORS.to_vec()
}

#[tauri::command]
fn add_rect(data: State<AppData>, x: f64, y: f64, w: f64, h: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.add_shape(Geo::Rect { x, y, w, h });
    Scene::from_state(&s)
}

#[tauri::command]
fn add_ellipse(data: State<AppData>, cx: f64, cy: f64, rx: f64, ry: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.add_shape(Geo::Ellipse { cx, cy, rx, ry });
    Scene::from_state(&s)
}

#[tauri::command]
fn activate_color(data: State<AppData>, color: [u8; 3]) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.activate_color(color);
    Scene::from_state(&s)
}

#[tauri::command]
fn select_at(data: State<AppData>, x: f64, y: f64, tol: f64, additive: bool) -> Scene {
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
    Scene::from_state(&s)
}

/// Marquee-Auswahl: alle Shapes, deren BBox vollständig im Rechteck liegt.
#[tauri::command]
fn select_rect(data: State<AppData>, x1: f64, y1: f64, x2: f64, y2: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.select_in_rect(x1, y1, x2, y2);
    Scene::from_state(&s)
}

/// Verschiebt die Auswahl um ein Gesamt-Delta (ein Undo-Punkt pro Geste).
#[tauri::command]
fn move_selected(data: State<AppData>, dx: f64, dy: f64) -> Scene {
    let mut s = data.state.lock().unwrap();
    if dx != 0.0 || dy != 0.0 {
        s.push_undo();
        s.translate_selected(dx, dy);
    }
    Scene::from_state(&s)
}

/// Skaliert die Auswahl von der Start-Gruppenbox auf die Zielbox (ein Undo-Punkt).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn scale_selected(
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
    Scene::from_state(&s)
}

#[tauri::command]
fn align(data: State<AppData>, kind: String) -> Scene {
    use luxifer_core::Align;
    let mut s = data.state.lock().unwrap();
    let k = match kind.as_str() {
        "left" => Align::Left,
        "hcenter" => Align::HCenter,
        "right" => Align::Right,
        "top" => Align::Top,
        "vcenter" => Align::VCenter,
        "bottom" => Align::Bottom,
        _ => return Scene::from_state(&s),
    };
    s.align_selection(k);
    Scene::from_state(&s)
}

#[tauri::command]
fn distribute(data: State<AppData>, kind: String) -> Scene {
    use luxifer_core::Distribute;
    let mut s = data.state.lock().unwrap();
    let k = match kind.as_str() {
        "h" => Distribute::Horizontal,
        "v" => Distribute::Vertical,
        _ => return Scene::from_state(&s),
    };
    s.distribute_selection(k);
    Scene::from_state(&s)
}

#[tauri::command]
fn clear_selection(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.selected.clear();
    Scene::from_state(&s)
}

#[tauri::command]
fn delete_selected(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.delete_selected();
    Scene::from_state(&s)
}

#[tauri::command]
fn undo(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.undo();
    Scene::from_state(&s)
}

#[tauri::command]
fn redo(data: State<AppData>) -> Scene {
    let mut s = data.state.lock().unwrap();
    s.redo();
    Scene::from_state(&s)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppData {
                state: Mutex::new(AppState::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_scene,
            swatch_colors,
            add_rect,
            add_ellipse,
            activate_color,
            select_at,
            select_rect,
            move_selected,
            scale_selected,
            align,
            distribute,
            clear_selection,
            delete_selected,
            undo,
            redo,
        ])
        .run(tauri::generate_context!())
        .expect("Fehler beim Starten der LuxiFer-App");
}
