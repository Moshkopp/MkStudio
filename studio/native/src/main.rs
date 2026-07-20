//! Studio nativ (ADR 0010/0011): produktive winit/wgpu/egui-Anwendung ohne
//! WebView oder IPC. Fachliche Anwendungsfälle laufen über studio-application.

mod app;
mod camera;
mod canvas;
mod fonts;
mod gpu;
mod icons;
mod image_gpu;
mod laserpanel;
mod render;
mod scene_geo;
mod tools;
mod ui;

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::app::App;
use crate::gpu::Gpu;

/// App-Icon einbetten und dekodieren — greift unter X11/Windows direkt; unter
/// Wayland kommt das Icon stattdessen über app_id + .desktop-Datei.
fn load_window_icon() -> Option<winit::window::Icon> {
    let bytes = include_bytes!("../assets/icon/studio-512.png");
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    winit::window::Icon::from_rgba(img.into_raw(), w, h).ok()
}

fn load_trim_cursor(el: &ActiveEventLoop) -> Option<winit::window::CustomCursor> {
    let bytes = include_bytes!("../assets/cursors/trim-scissors.png");
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    let source = winit::window::CustomCursor::from_rgba(
        image.into_raw(),
        width.try_into().ok()?,
        height.try_into().ok()?,
        35,
        3,
    )
    .ok()?;
    Some(el.create_custom_cursor(source))
}

#[derive(Default)]
struct Runner {
    app: Option<App>,
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.app.is_some() {
            return;
        }
        let startup_settings = studio_core::UiSettings::load();
        let open_maximized = startup_settings.open_maximized;
        let mut attrs = Window::default_attributes()
            .with_title(studio_core::branding::STUDIO_NAME)
            .with_inner_size(winit::dpi::LogicalSize::new(1400, 880))
            .with_maximized(open_maximized)
            .with_active(true)
            .with_window_icon(load_window_icon());
        #[cfg(target_os = "linux")]
        {
            // Wayland zeigt Fenster-Icons nur über die app_id → .desktop-Datei
            // (studio.desktop mit Icon=studio); ohne app_id gibt es das
            // generische Compositor-Icon. Unter X11 entspricht das WM_CLASS.
            use winit::platform::{
                wayland::WindowAttributesExtWayland, x11::WindowAttributesExtX11,
            };
            attrs = WindowAttributesExtWayland::with_name(
                attrs,
                studio_core::branding::APP_ID,
                studio_core::branding::APP_ID,
            );
            attrs = WindowAttributesExtX11::with_name(
                attrs,
                studio_core::branding::APP_ID,
                studio_core::branding::APP_ID,
            );
        }
        let window = match el.create_window(attrs) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                log::error!("Anwendungsfenster konnte nicht erstellt werden: {error}");
                el.exit();
                return;
            }
        };
        let gpu = match pollster::block_on(Gpu::new(
            window.clone(),
            startup_settings.msaa_samples,
            startup_settings.line_antialiasing,
        )) {
            Ok(gpu) => gpu,
            Err(error) => {
                log::error!("{error}");
                el.exit();
                return;
            }
        };
        let trim_cursor = load_trim_cursor(el);
        let mut app = match App::new(window, gpu, trim_cursor) {
            Ok(app) => app,
            Err(error) => {
                log::error!("Anwendung konnte nicht initialisiert werden: {error:?}");
                el.exit();
                return;
            }
        };
        // Ersten Frame sofort präsentieren und Redraw anfordern — sonst bleibt
        // das Fenster unter manchen Wayland-Compositoren leer/unsichtbar, bis
        // ein Event kommt.
        app.render();
        app.window.request_redraw();
        self.app = Some(app);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(app) = self.app.as_mut() else { return };
        if matches!(event, WindowEvent::CloseRequested) {
            // Dirty-Guard: bei ungespeicherten Änderungen erst bestätigen lassen.
            if app.request_close() {
                el.exit();
            }
            return;
        }
        if matches!(event, WindowEvent::RedrawRequested) {
            app.render();
            // KEIN blindes request_redraw hier — sonst rennt die Schleife auf
            // Vollgas. Neu gezeichnet wird nur bei Events (siehe unten) oder
            // wenn egui selbst einen Repaint anfordert.
            if app.egui_wants_repaint() {
                app.window.request_redraw();
            }
            if app.should_exit() {
                el.exit();
            }
            return;
        }
        // Jedes eingehende Fenster-Event kann etwas ändern → einmal neu zeichnen.
        let changed = app.window_event(&event);
        if changed {
            app.window.request_redraw();
        }
        if app.should_exit() {
            el.exit();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(app) = self.app.as_mut() else { return };
        let now = std::time::Instant::now();
        if app
            .egui_next_repaint()
            .is_some_and(|deadline| deadline <= now)
        {
            app.window.request_redraw();
        }
        if app.poll_hub() {
            app.window.request_redraw();
        }
        if app.poll_asset_thumbnails() {
            app.window.request_redraw();
        }
        if app.poll_asset_import() {
            app.window.request_redraw();
        }
        if app.poll_project_integration() {
            app.window.request_redraw();
        }
        if app.poll_laser_lease() {
            app.window.request_redraw();
        }
        if app.view == crate::tools::View::Laser && app.poll_laser_status() {
            app.window.request_redraw();
        }
        // Ohne View-Bedingung: die Kalibrierung läuft im Verwaltungsdialog,
        // der aus jeder Ansicht heraus offen sein kann.
        if app.poll_axis_calibration() {
            app.window.request_redraw();
        }
        if app.poll_machine_read() {
            app.window.request_redraw();
        }
        // Der Netzwerkthread arbeitet unabhängig. Dieses kurze Aufwachen dient
        // nur dazu, dessen Ergebnis zeitnah in die UI zu übernehmen.
        let regular_wake = now + std::time::Duration::from_millis(500);
        let next_wake = app
            .egui_next_repaint()
            .map_or(regular_wake, |deadline| deadline.min(regular_wake));
        el.set_control_flow(ControlFlow::WaitUntil(next_wake));
    }
}

fn main() -> Result<(), String> {
    // Nur Warnungen/Fehler loggen — das INFO-Log von wgpu würde sonst das
    // Terminal fluten (Device::maintain je Frame).
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let el = EventLoop::new()
        .map_err(|error| format!("Eventloop konnte nicht gestartet werden: {error}"))?;
    // Warten statt pollen: der Editor zeichnet nur bei Bedarf neu, nicht in einer
    // Endlosschleife. Spart CPU/GPU und beruhigt das Terminal.
    el.set_control_flow(winit::event_loop::ControlFlow::Wait);
    let mut runner = Runner::default();
    el.run_app(&mut runner)
        .map_err(|error| format!("Eventloop wurde mit einem Fehler beendet: {error}"))
}
