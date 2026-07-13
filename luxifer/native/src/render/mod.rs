//! Frame-Koordination und GPU-Ressourcen des nativen Renderers. Besitzt die
//! GPU, den egui-Wgpu-Renderer/-State, den Bild-Store und den Vertex-Cache.
//!
//! Die Trennung zu `App`: Der App-Root baut den egui-Frame (dazu braucht die
//! `ui::build`-Closure `&mut App`) und übergibt das Ergebnis samt Szenenzustand
//! an [`Renderer::draw_frame`], das den eigentlichen GPU-Frame erzeugt. So
//! liegen GPU-Ressourcen und Frame-Ablauf gebündelt hier, nicht im Monolithen.

use std::time::Instant;

use egui_wgpu::ScreenDescriptor;
use luxifer_application::EditorSession;
use winit::window::Window;

use crate::camera::Camera;
use crate::canvas::overlay::{overlay_vertices, OverlayInput};
use crate::canvas::scene::{base_vertices, preview_vertices, PreviewLegend, PreviewMaterial};
use crate::gpu::Gpu;
use crate::image_gpu::ImageStore;

#[derive(Clone, Copy, PartialEq)]
struct PreviewKey {
    revision: u64,
    selection_only: bool,
    material: PreviewMaterial,
    show_travel: bool,
}

/// Nur-lesender Szenenzustand, den der Root pro Frame an den Renderer übergibt.
pub struct FrameScene<'a> {
    pub session: &'a EditorSession,
    pub cam: &'a Camera,
    pub overlay: OverlayInput<'a>,
    /// Ob externe Ereignisse (Import) neue Bild-Texturen nötig machen.
    pub image_dirty: bool,
    pub preview: bool,
    pub selection_only: bool,
    /// Material-Vorlage der Vorschau (bestimmt Untergrund-/Brennfarbe).
    pub preview_material: PreviewMaterial,
    /// Leerfahrten in der Vorschau zeichnen (Kennzahlen zählen immer).
    pub preview_show_travel: bool,
    /// Feinraster-Abstand des Tisch-Gitters in mm (GUI-Settings).
    pub grid_mm: f32,
    /// Maschinen-Nullpunkt des aktiven Laserprofils.
    pub bed_origin: luxifer_core::BedOrigin,
}

pub struct Renderer {
    gpu: Gpu,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    images: ImageStore,
    // Vertex-Cache: die (teure) Scanline-Füllung wird NUR neu gebaut, wenn sich
    // der Zustand ändert — nicht pro Frame. Pan/Zoom lassen die Vertices
    // unberührt (die Projektion macht der Shader), daher bleiben sie gecacht.
    verts: Vec<crate::scene_geo::Vertex>,
    background_end: u32,
    /// Render-Revision (aus dem Core) beim letzten Vertex-Aufbau.
    last_render_rev: u64,
    /// Legende des letzten Preview-Aufbaus (None außerhalb der Vorschau bzw.
    /// vor dem ersten Preview-Frame). Die UI liest sie einen Frame versetzt.
    preview_legend: Option<PreviewLegend>,
    preview_key: Option<PreviewKey>,
    preview_pending: Option<(
        PreviewKey,
        std::sync::mpsc::Receiver<crate::canvas::scene::PreviewGeometry>,
    )>,
    last_frame: Instant,
    fps: f32,
    /// Ob egui im letzten Frame sofort weiter zeichnen wollte.
    wants_repaint: bool,
    /// Frühester von egui angeforderter verzögerter Repaint (z. B. Tooltip).
    next_repaint: Option<Instant>,
    /// Kamera-/Rasterstand des letzten Grid-Aufbaus (Center, Scale, Viewport,
    /// grid_mm) — das viewportfüllende Gitter wird nur bei Änderung neu gebaut.
    grid_key: Option<([f32; 2], f32, [f32; 2], f32)>,
}

impl Renderer {
    pub fn preload_image(&mut self, asset: &str, rgba: &[u8], width: u32, height: u32) {
        self.images.insert_rgba(
            &self.gpu.device,
            &self.gpu.queue,
            self.gpu.config.format,
            asset,
            rgba,
            (width, height),
        );
    }

    pub fn new(gpu: Gpu, egui_state: egui_winit::State) -> Self {
        let egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.config.format,
            egui_wgpu::RendererOptions::default(),
        );
        Self {
            gpu,
            egui_state,
            egui_renderer,
            images: ImageStore::default(),
            verts: Vec::new(),
            background_end: 0,
            // MAX erzwingt den Aufbau im ersten Frame (Core startet bei 0).
            last_render_rev: u64::MAX,
            preview_legend: None,
            preview_key: None,
            preview_pending: None,
            last_frame: Instant::now(),
            fps: 0.0,
            wants_repaint: false,
            next_repaint: None,
            grid_key: None,
        }
    }

    pub fn fps(&self) -> f32 {
        self.fps
    }

    pub fn wants_repaint(&self) -> bool {
        self.wants_repaint
    }

    pub fn next_repaint(&self) -> Option<Instant> {
        self.next_repaint
    }

    /// Erzwingt den Vertex-Neuaufbau im nächsten Frame (z. B. nach Projektwechsel,
    /// weil der geladene Zustand einen eigenen Revisionszähler mitbringt). Die
    /// Preview-Legende verfällt mit, damit die UI keine veralteten Kennzahlen
    /// zeigt.
    pub fn invalidate_scene(&mut self) {
        self.last_render_rev = u64::MAX;
        self.preview_legend = None;
        self.preview_key = None;
        self.preview_pending = None;
    }

    /// Legende des letzten Preview-Aufbaus (None, solange keiner lief).
    pub fn preview_legend(&self) -> Option<&PreviewLegend> {
        self.preview_legend.as_ref()
    }

    /// Nimmt egui-Roheingaben entgegen (für den Frame-Aufbau im Root).
    pub fn take_egui_input(&mut self, window: &Window) -> egui::RawInput {
        self.egui_state.take_egui_input(window)
    }

    /// Leitet ein Fensterereignis an egui weiter (Fokus, Hover, Tastatur).
    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &winit::event::WindowEvent,
    ) -> egui_winit::EventResponse {
        self.egui_state.on_window_event(window, event)
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.gpu.resize(w, h);
    }

    /// Zeichnet einen Frame: aktualisiert Vertex-/Bild-Caches aus `scene`, lädt
    /// egui-Daten hoch und rendert Canvas + Overlay + egui. `full`/`tris` liefert
    /// der Root (er besitzt den egui-`Context` und baut damit den Frame).
    pub fn draw_frame(
        &mut self,
        window: &Window,
        scene: FrameScene,
        full: egui::FullOutput,
        tris: Vec<egui::ClippedPrimitive>,
    ) {
        // FPS.
        let dt = self.last_frame.elapsed().as_secs_f32();
        self.last_frame = Instant::now();
        if dt > 0.0 {
            self.fps = 0.9 * self.fps + 0.1 * (1.0 / dt);
        }

        self.egui_state
            .handle_platform_output(window, full.platform_output);
        let repaint_delay = full
            .viewport_output
            .values()
            .map(|output| output.repaint_delay)
            .min();
        self.wants_repaint = repaint_delay.is_some_and(|delay| delay.is_zero());
        self.next_repaint = repaint_delay
            .filter(|delay| !delay.is_zero())
            .and_then(|delay| Instant::now().checked_add(delay));

        // Canvas-Vertices nur neu bauen+hochladen, wenn sich die Szene änderte.
        let rev = scene.session.render_rev();
        let mut scene_changed = rev != self.last_render_rev;
        if scene.preview {
            let key = PreviewKey {
                revision: rev,
                selection_only: scene.selection_only,
                material: scene.preview_material,
                show_travel: scene.preview_show_travel,
            };
            if self.preview_key != Some(key)
                && self.preview_pending.as_ref().map(|(k, _)| *k) != Some(key)
            {
                let state = scene.session.state().clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let session = EditorSession::new(state);
                    let geometry = preview_vertices(
                        &session,
                        key.selection_only,
                        key.material,
                        key.show_travel,
                    );
                    let _ = tx.send(geometry);
                });
                self.preview_pending = Some((key, rx));
                self.preview_legend = None;
                if self.preview_key.is_none() {
                    // Beim ersten Eintritt keine alte Design-Geometrie als
                    // vermeintliche Preview zeigen: nur die Materialbühne.
                    self.verts = crate::scene_geo::bed_material(
                        scene.session.bed_w_mm as f32,
                        scene.session.bed_h_mm as f32,
                        crate::canvas::scene::srgb_to_linear(key.material.bed()),
                    );
                    self.background_end = self.verts.len() as u32;
                    self.images.set_rasters(
                        &self.gpu.device,
                        &self.gpu.queue,
                        self.gpu.config.format,
                        &[],
                        crate::canvas::scene::srgb_to_linear(key.material.burn()),
                    );
                    self.gpu.upload_verts(&self.verts);
                }
            }
            if self.preview_pending.is_some() {
                self.wants_repaint = true;
                window.request_redraw();
            }
            let ready = self
                .preview_pending
                .as_ref()
                .and_then(|(_, rx)| rx.try_recv().ok());
            if let Some(geometry) = ready {
                let (key, _) = self.preview_pending.take().unwrap();
                self.preview_key = Some(key);
                self.background_end = geometry.background_end;
                self.verts = geometry.vertices;
                self.images.set_rasters(
                    &self.gpu.device,
                    &self.gpu.queue,
                    self.gpu.config.format,
                    &geometry.rasters,
                    crate::canvas::scene::srgb_to_linear(key.material.burn()),
                );
                self.preview_legend = Some(geometry.legend);
                let verts = std::mem::take(&mut self.verts);
                self.gpu.upload_verts(&verts);
                self.verts = verts;
            }
            // Preview-Geometrie wird ausschließlich vom fertigen Workergebnis
            // aktualisiert; eine Core-Revision startet nur den Worker.
            scene_changed = false;
            self.last_render_rev = rev;
        }
        if scene_changed {
            self.last_render_rev = rev;
            let geometry = base_vertices(scene.session, scene.bed_origin);
            self.background_end = geometry.background_end;
            self.verts = geometry.vertices;
            self.preview_legend = None;
            let verts = std::mem::take(&mut self.verts);
            self.gpu.upload_verts(&verts);
            self.verts = verts;
        }
        self.gpu.upload_camera(scene.cam);
        // Viewportfüllendes Gitter (kamera-abhängig): nur bei Änderung neu.
        // Die Vorschau ist die Material-Bühne und bleibt gitterfrei.
        let grid_key = if scene.preview {
            None
        } else {
            Some((
                scene.cam.center,
                scene.cam.scale,
                scene.cam.viewport,
                scene.grid_mm,
            ))
        };
        if grid_key != self.grid_key {
            self.grid_key = grid_key;
            let grid = match grid_key {
                Some(_) => crate::scene_geo::viewport_grid(scene.cam, scene.grid_mm),
                None => Vec::new(),
            };
            self.gpu.upload_grid(&grid);
        }
        if scene.image_dirty || scene_changed {
            self.images.sync(
                &self.gpu.device,
                &self.gpu.queue,
                self.gpu.config.format,
                scene.session,
            );
        }
        let count = self.verts.len() as u32;
        let overlay = if scene.preview {
            Vec::new()
        } else {
            overlay_vertices(&scene.overlay)
        };
        self.gpu.upload_overlay(&overlay);

        let frame = match self.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.gpu
                    .surface
                    .configure(&self.gpu.device, &self.gpu.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());

        // egui-Texturen/Buffer aktualisieren.
        let screen = ScreenDescriptor {
            size_in_pixels: [self.gpu.config.width, self.gpu.config.height],
            pixels_per_point: full.pixels_per_point,
        };
        for (id, delta) in &full.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.gpu.device, &self.gpu.queue, *id, delta);
        }
        self.egui_renderer.update_buffers(
            &self.gpu.device,
            &self.gpu.queue,
            &mut enc,
            &tris,
            &screen,
        );

        // Scratch-Buffer für die Bild-Quads (muss den Render-Pass überleben).
        let mut img_scratch: Option<wgpu::Buffer> = None;
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.025,
                            g: 0.03,
                            b: 0.04,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // Bettrahmen zuerst, darüber das viewportfüllende Gitter. Danach
            // Bildtexturen — im Preview die verarbeiteten
            // Job-Rasterungen, sonst die Design-Originale —, anschließend
            // Vektor-Fills und Konturen; Handles bleiben ganz oben.
            self.gpu.draw_canvas_range(&mut rp, 0..self.background_end);
            self.gpu.draw_grid(&mut rp);
            if scene.preview {
                self.images
                    .draw_rasters(&mut rp, &self.gpu, scene.cam, &mut img_scratch);
            } else {
                self.images.draw(
                    &mut rp,
                    &self.gpu,
                    scene.cam,
                    scene.session,
                    &mut img_scratch,
                );
            }
            self.gpu
                .draw_canvas_range(&mut rp, self.background_end..count);
            self.gpu.draw_overlay(&mut rp);
            // egui obendrauf (eigener Lebenszeit-Scope via forget_lifetime).
            let mut rp = rp.forget_lifetime();
            self.egui_renderer.render(&mut rp, &tris, &screen);
        }
        self.gpu.queue.submit(Some(enc.finish()));
        frame.present();

        for id in &full.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }
}
