//! LuxiFer nativ (ADR 0010): winit + wgpu (Canvas) + egui (Panels), direkt an
//! luxifer-core. Kein WebView, kein IPC. Startpunkt des Umbaus neben der noch
//! lauffähigen Tauri-App.

mod app;
mod camera;
mod gpu;
mod laser;
mod laserpanel;
mod scene_geo;
mod tools;
mod ui;

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::app::App;
use crate::gpu::Gpu;

#[derive(Default)]
struct Runner {
    app: Option<App>,
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.app.is_some() {
            return;
        }
        let window = Arc::new(
            el.create_window(
                Window::default_attributes()
                    .with_title("LuxiFer — nativ (wgpu)")
                    .with_inner_size(winit::dpi::LogicalSize::new(1400, 880))
                    .with_active(true),
            )
            .unwrap(),
        );
        let gpu = pollster::block_on(Gpu::new(window.clone()));
        let mut app = App::new(window, gpu);
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
            el.exit();
            return;
        }
        if matches!(event, WindowEvent::RedrawRequested) {
            app.render();
            app.window.request_redraw();
            return;
        }
        app.window_event(&event);
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let el = EventLoop::new().unwrap();
    el.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut runner = Runner::default();
    el.run_app(&mut runner).unwrap();
}
