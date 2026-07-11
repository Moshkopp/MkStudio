//! LuxiFer Tauri-Backend. Hält den `AppState` des Cores und stellt Commands
//! bereit. Das Frontend zeichnet nur — die gesamte Fachlogik bleibt im Core.

use std::sync::Mutex;

use luxifer_core::preview::JobPreview;
use luxifer_core::{AppState, LaserRegistry, UiSettings};
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

mod commands;
mod shared;
use commands::edit::*;
use commands::image::*;
use commands::laser::*;
use commands::project::*;
use commands::shapes::*;
use shared::{
    plan_with_assets, scene, scene_with, ActiveDriver, AppData, CurrentProject, PreviewDto, Scene,
};

/// Versionsdaten für Splash/About (aus build.rs, git-abgeleitet).
#[derive(serde::Serialize)]
struct AppVersion {
    version: String,
    commit: String,
}

#[tauri::command]
fn get_scene(data: State<AppData>) -> Scene {
    scene(&data)
}

#[tauri::command]
fn swatch_colors() -> Vec<[u8; 3]> {
    luxifer_core::SWATCH_COLORS.to_vec()
}

/// App-Version (Splash/About). Beide Werte werden zur Build-Zeit in build.rs aus
/// git abgeleitet: `version` tag-relativ (z. B. v0.8-12-gbc59d67), `commit` der
/// kurze Hash. Steigt automatisch mit jedem Commit.
#[tauri::command]
fn app_version() -> AppVersion {
    AppVersion {
        version: env!("LUXIFER_VERSION").to_string(),
        commit: env!("LUXIFER_COMMIT").to_string(),
    }
}

/// Leitet aus dem aktuellen Zustand die Laser-Vorschau ab (ADR 0005): die zu
/// fahrenden Segmente in Ausführungsreihenfolge inkl. Verfahrwege. Reine
/// Ableitung des `JobPlan` — kein Undo, keine Mutation.
#[tauri::command]
fn job_preview(data: State<AppData>) -> PreviewDto {
    let s = data.state();
    let plan = plan_with_assets(&s.shapes, &s.layers);
    let preview = JobPreview::from_plan(&plan);
    PreviewDto::from_preview(&preview)
}

/// Lädt die GUI-Settings (Theming, Arbeitsplatz) — ADR 0002.
/// Fehlt die Datei, kommt der Default zurück; die GUI startet immer.
#[tauri::command]
fn get_ui_settings() -> UiSettings {
    UiSettings::load()
}

/// Speichert die vom Frontend gelieferten GUI-Settings lokal als JSON.
/// Werte werden vor dem Schreiben geklemmt/aufgeräumt (sanitize).
#[tauri::command]
fn save_ui_settings(mut settings: UiSettings) -> Result<UiSettings, String> {
    settings.sanitize();
    settings.save()?;
    Ok(settings)
}

#[tauri::command]
fn undo(data: State<AppData>) -> Scene {
    let mut s = data.state();
    s.undo();
    scene_with(&s, &data)
}

#[tauri::command]
fn redo(data: State<AppData>) -> Scene {
    let mut s = data.state();
    s.redo();
    scene_with(&s, &data)
}

/// Das Frontend meldet, dass die GUI fertig geladen ist: Hauptfenster zeigen und
/// den Splash (falls vorhanden) schließen. Idempotent — mehrfacher Aufruf schadet
/// nicht (z. B. bei HMR-Reloads im Dev).
#[tauri::command]
fn frontend_ready(app: tauri::AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.close();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppData {
                state: Mutex::new(AppState::new()),
                current: Mutex::new(CurrentProject::default()),
                lasers: Mutex::new(LaserRegistry::load()),
                active: Mutex::new(ActiveDriver::default()),
            });
            // Fenster-/Taskleisten-Icon zur Laufzeit setzen (greift auch im
            // Dev-Modus, wo das gebündelte Icon sonst nicht verwendet wird).
            if let (Some(win), Some(icon)) =
                (app.get_webview_window("main"), app.default_window_icon())
            {
                let _ = win.set_icon(icon.clone());
            }

            // Splash: das Hauptfenster startet unsichtbar (tauri.conf). Ist der
            // Splash aktiviert, ein kleines, randloses Splash-Fenster ZUERST
            // zeigen (klassische Reihenfolge: Splash → dann GUI). Das Frontend
            // meldet sich per `frontend_ready`, sobald es fertig geladen ist —
            // dann wird `main` gezeigt und der Splash geschlossen. Ist der Splash
            // deaktiviert, kommt `main` sofort.
            if UiSettings::load().show_splash {
                let _ =
                    WebviewWindowBuilder::new(app, "splash", WebviewUrl::App("splash.html".into()))
                        .title("LuxiFer")
                        .inner_size(560.0, 300.0)
                        .decorations(false)
                        .transparent(true)
                        .resizable(false)
                        .center()
                        .always_on_top(true)
                        .skip_taskbar(true)
                        .build();
            } else if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_scene,
            swatch_colors,
            app_version,
            frontend_ready,
            add_rect,
            add_ellipse,
            add_line,
            add_polyline,
            shape_catalog,
            add_polygon,
            activate_color,
            select_at,
            select_rect,
            group_op,
            ungroup_op,
            move_selected,
            scale_selected,
            rotate_selected,
            align,
            distribute,
            mirror,
            boolean_op,
            trace_image,
            list_fonts,
            import_vector_file,
            add_text,
            update_text,
            text_preview,
            pattern_fill_op,
            add_spline,
            add_bezier,
            add_bezier_nodes,
            drag_node,
            hit_bezier_segment,
            split_node,
            delete_node,
            toggle_node_smooth,
            upload_font,
            offset_op,
            fillet_op,
            fillet_corners_op,
            bridge_op,
            nest_op,
            nest_fill_op,
            insert_coasters,
            set_layer_params,
            toggle_layer,
            move_layer,
            job_preview,
            laser_list,
            laser_save,
            laser_delete,
            laser_set_active,
            laser_actions,
            laser_run_action,
            laser_export,
            laser_jog,
            laser_home,
            laser_position,
            laser_ping,
            clear_selection,
            delete_selected,
            get_ui_settings,
            save_ui_settings,
            new_project,
            save_project,
            save_version,
            import_image_file,
            image_render,
            set_image_params,
            open_project,
            open_version,
            delete_version,
            project_list,
            project_detail,
            project_assets,
            project_thumb,
            version_thumb,
            project_delete,
            project_rename,
            project_export,
            undo,
            redo,
        ])
        .run(tauri::generate_context!())
        .expect("Fehler beim Starten der LuxiFer-App");
}
