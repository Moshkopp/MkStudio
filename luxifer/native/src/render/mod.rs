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
use crate::canvas::scene::{
    base_vertices_profiled, preview_vertices, PreviewLegend, PreviewMaterial,
};
use crate::gpu::Gpu;
use crate::image_gpu::ImageStore;

#[derive(Clone, Copy, PartialEq)]
struct PreviewKey {
    revision: u64,
    selection_only: bool,
    material: PreviewMaterial,
    show_travel: bool,
    show_laser_path: bool,
    show_scan_offset: bool,
    bed_origin: luxifer_core::BedOrigin,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UiFrameTimings {
    pub build_ms: f64,
    pub tessellate_ms: f64,
}

#[derive(Default)]
struct PerfWindow {
    started: Option<Instant>,
    frames: u64,
    scene_rebuilds: u64,
    selection_rebuilds: u64,
    ui_ms: f64,
    tessellate_ms: f64,
    fill_ms: f64,
    lines_ms: f64,
    overlay_ms: f64,
    selection_ms: f64,
    image_ms: f64,
    frame_ms: f64,
    max_frame_ms: f64,
    scene_vertices: usize,
    fill_vertices: usize,
    overlay_vertices: usize,
    selection_vertices: usize,
    fill_compounds: usize,
    estimated_draw_calls: usize,
}

impl PerfWindow {
    fn record(&mut self, sample: PerfSample) {
        self.started.get_or_insert_with(Instant::now);
        self.frames += 1;
        self.scene_rebuilds += u64::from(sample.scene_rebuilt);
        self.selection_rebuilds += u64::from(sample.selection_rebuilt);
        self.ui_ms += sample.ui.build_ms;
        self.tessellate_ms += sample.ui.tessellate_ms;
        self.fill_ms += sample.fill_ms;
        self.lines_ms += sample.lines_ms;
        self.overlay_ms += sample.overlay_ms;
        self.selection_ms += sample.selection_ms;
        self.image_ms += sample.image_ms;
        self.frame_ms += sample.frame_ms;
        self.max_frame_ms = self.max_frame_ms.max(sample.frame_ms);
        self.scene_vertices = sample.scene_vertices;
        self.fill_vertices = sample.fill_vertices;
        self.overlay_vertices = sample.overlay_vertices;
        self.selection_vertices = sample.selection_vertices;
        self.fill_compounds = sample.fill_compounds;
        self.estimated_draw_calls = sample.estimated_draw_calls;
    }

    fn log_if_due(&mut self) {
        let Some(started) = self.started else { return };
        if started.elapsed().as_secs_f32() < 1.0 || self.frames == 0 {
            return;
        }
        let n = self.frames as f64;
        let rebuilds = self.scene_rebuilds.max(1) as f64;
        let selection_rebuilds = self.selection_rebuilds.max(1) as f64;
        log::info!(
            target: "luxifer_render_perf",
            "frames={} rebuilds={} selection_rebuilds={} avg_frame_ms frame={:.2} ui={:.2} tess={:.2} overlay={:.2} images={:.2} avg_rebuild_ms fill={:.2} lines={:.2} selection={:.2} max_frame_ms={:.2} vertices scene={} fill={} selection={} dynamic_overlay={} fill_compounds={} estimated_canvas_draw_calls={}",
            self.frames,
            self.scene_rebuilds,
            self.selection_rebuilds,
            self.frame_ms / n,
            self.ui_ms / n,
            self.tessellate_ms / n,
            self.overlay_ms / n,
            self.image_ms / n,
            self.fill_ms / rebuilds,
            self.lines_ms / rebuilds,
            self.selection_ms / selection_rebuilds,
            self.max_frame_ms,
            self.scene_vertices,
            self.fill_vertices,
            self.selection_vertices,
            self.overlay_vertices,
            self.fill_compounds,
            self.estimated_draw_calls,
        );
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Default)]
struct PerfSample {
    ui: UiFrameTimings,
    scene_rebuilt: bool,
    selection_rebuilt: bool,
    fill_ms: f64,
    lines_ms: f64,
    overlay_ms: f64,
    selection_ms: f64,
    image_ms: f64,
    frame_ms: f64,
    scene_vertices: usize,
    fill_vertices: usize,
    overlay_vertices: usize,
    selection_vertices: usize,
    fill_compounds: usize,
    estimated_draw_calls: usize,
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
    pub preview_show_laser_path: bool,
    pub preview_show_scan_offset: bool,
    pub preview_trace: Option<&'a luxifer_core::ExecutionTrace>,
    /// Feinraster-Abstand des Tisch-Gitters in mm (GUI-Settings).
    pub grid_mm: f32,
    /// Maschinen-Nullpunkt des aktiven Laserprofils.
    pub bed_origin: luxifer_core::BedOrigin,
    pub selection_transform: crate::gpu::SelectionTransform,
    pub move_all_fills: bool,
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
    fill_batches: Vec<crate::scene_geo::FillBatch>,
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
    selection_revision: u64,
    selection_indices: Vec<usize>,
    selection_accent: [u8; 3],
    selection_vertex_count: usize,
    perf_enabled: bool,
    perf: PerfWindow,
}

impl Renderer {
    pub fn preload_image(&mut self, asset: &str, rgba: &[u8], width: u32, height: u32) {
        self.images
            .insert_rgba(&self.gpu, asset, rgba, (width, height));
    }

    pub fn new(gpu: Gpu, egui_state: egui_winit::State) -> Self {
        let egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.config.format,
            egui_wgpu::RendererOptions {
                msaa_samples: gpu.sample_count,
                ..Default::default()
            },
        );
        Self {
            gpu,
            egui_state,
            egui_renderer,
            images: ImageStore::default(),
            verts: Vec::new(),
            fill_batches: Vec::new(),
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
            selection_revision: u64::MAX,
            selection_indices: Vec::new(),
            selection_accent: [0; 3],
            selection_vertex_count: 0,
            perf_enabled: std::env::var_os("LUXIFER_RENDER_PROFILE").is_some(),
            perf: PerfWindow::default(),
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
        self.selection_revision = u64::MAX;
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
        ui_timings: UiFrameTimings,
    ) {
        let frame_started = Instant::now();
        let mut perf = PerfSample {
            ui: ui_timings,
            ..Default::default()
        };
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
        let selection_changed =
            self.selection_indices.as_slice() != scene.session.state().selected.as_slice();
        let mut scene_changed = rev != self.last_render_rev || selection_changed;
        if scene.preview {
            let key = PreviewKey {
                revision: rev,
                selection_only: scene.selection_only,
                material: scene.preview_material,
                show_travel: scene.preview_show_travel,
                show_laser_path: scene.preview_show_laser_path,
                show_scan_offset: scene.preview_show_scan_offset,
                bed_origin: scene.bed_origin,
            };
            if self.preview_key != Some(key)
                && self.preview_pending.as_ref().map(|(k, _)| *k) != Some(key)
            {
                let state = scene.session.state().clone();
                let trace = scene.preview_trace.cloned().unwrap_or_default();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let session = EditorSession::new(state);
                    let geometry = preview_vertices(
                        &session,
                        &trace,
                        key.material,
                        key.show_travel,
                        key.show_laser_path,
                        key.show_scan_offset,
                        key.bed_origin,
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
                    self.fill_batches.clear();
                    self.gpu.upload_fill_verts(&[]);
                    self.images.set_rasters(
                        &self.gpu.device,
                        &self.gpu.queue,
                        self.gpu.config.format,
                        self.gpu.sample_count,
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
                self.fill_batches.clear();
                self.gpu.upload_fill_verts(&[]);
                self.images.set_rasters(
                    &self.gpu.device,
                    &self.gpu.queue,
                    self.gpu.config.format,
                    self.gpu.sample_count,
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
            let (geometry, build_timings) = base_vertices_profiled(scene.session, scene.bed_origin);
            perf.scene_rebuilt = true;
            perf.fill_ms = build_timings.fill_ms;
            perf.lines_ms = build_timings.lines_ms;
            self.background_end = geometry.background_end;
            self.verts = geometry.vertices;
            self.fill_batches = geometry.fill_batches;
            self.gpu.upload_fill_verts(&geometry.fill_vertices);
            self.preview_legend = None;
            let verts = std::mem::take(&mut self.verts);
            self.gpu.upload_verts(&verts);
            self.verts = verts;
        }
        self.gpu.upload_camera(scene.cam, scene.selection_transform);
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
                self.gpu.sample_count,
                scene.session,
            );
        }
        if scene.preview {
            if self.selection_vertex_count != 0 {
                self.gpu.upload_selection(&[]);
                self.selection_vertex_count = 0;
            }
            self.selection_revision = u64::MAX;
        } else {
            let selected = &scene.session.state().selected;
            if self.selection_revision != rev
                || self.selection_indices.as_slice() != selected.as_slice()
                || self.selection_accent != scene.overlay.accent
            {
                let selection_started = Instant::now();
                let selection = crate::scene_geo::selected_outlines(
                    scene.session.state(),
                    scene.overlay.accent,
                );
                self.selection_vertex_count = selection.len();
                perf.selection_rebuilt = true;
                perf.selection_ms = selection_started.elapsed().as_secs_f64() * 1_000.0;
                self.gpu.upload_selection(&selection);
                self.selection_revision = rev;
                self.selection_indices.clone_from(selected);
                self.selection_accent = scene.overlay.accent;
            }
        }
        let count = self.verts.len() as u32;
        let overlay_started = Instant::now();
        let overlay = if scene.preview {
            Vec::new()
        } else {
            overlay_vertices(&scene.overlay)
        };
        perf.overlay_ms = overlay_started.elapsed().as_secs_f64() * 1_000.0;
        perf.overlay_vertices = overlay.len();
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
        let color_view = self.gpu.color_view(&view);
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
        let image_started = Instant::now();
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("background-images"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            // Dark-Workshop-Canvas (#111318 in etwa, linear).
                            r: 0.006,
                            g: 0.007,
                            b: 0.009,
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
            // Untergrund, Gitter und Bilder liegen unter den Vektorflächen.
            self.gpu.draw_canvas_range(&mut rp, 0..self.background_end);
            self.gpu.draw_grid(&mut rp);
            if scene.preview {
                self.images
                    .draw_rasters(&mut rp, &self.gpu, scene.cam, &mut img_scratch);
            } else {
                self.images.draw(&mut rp, &self.gpu, scene.cam);
            }
        }
        perf.image_ms = image_started.elapsed().as_secs_f64() * 1_000.0;
        if !scene.preview && !self.fill_batches.is_empty() {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("solid-fills"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.gpu.stencil_view(),
                    depth_ops: None,
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Discard,
                    }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.gpu
                .draw_solid_fills(&mut rp, &self.fill_batches, scene.move_all_fills);
        }
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("outlines-overlay-ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: (self.gpu.sample_count > 1).then_some(&view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.gpu
                .draw_canvas_range(&mut rp, self.background_end..count);
            self.gpu.draw_selection(&mut rp);
            self.gpu.draw_overlay(&mut rp);
            // egui obendrauf (eigener Lebenszeit-Scope via forget_lifetime).
            let mut rp = rp.forget_lifetime();
            self.egui_renderer.render(&mut rp, &tris, &screen);
        }
        self.gpu.queue.submit(Some(enc.finish()));
        frame.present();

        if self.perf_enabled {
            perf.frame_ms = frame_started.elapsed().as_secs_f64() * 1_000.0;
            perf.scene_vertices = self.verts.len();
            perf.selection_vertices = self.selection_vertex_count;
            perf.fill_vertices = self.fill_batches.last().map_or(0, |_| {
                self.fill_batches
                    .iter()
                    .map(|batch| batch.cover.end as usize)
                    .max()
                    .unwrap_or(0)
            });
            perf.fill_compounds = self
                .fill_batches
                .iter()
                .map(|batch| batch.compounds.len())
                .sum();
            // Canvas/Grid/Overlay sind je ein Draw; jeder Compound benötigt drei
            // Stencil-Draws, jeder Fill-Layer zwei Abschluss-Draws.
            perf.estimated_draw_calls = 3
                + usize::from(self.selection_vertex_count > 0)
                + perf.fill_compounds * 3
                + self.fill_batches.len() * 2;
            self.perf.record(perf);
            self.perf.log_if_due();
        }

        for id in &full.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }
}
