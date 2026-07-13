//! wgpu-Canvas: rendert die Linien-Vertices (Welt-mm) mit Kamera-Projektion.
//! Ein Pipeline, LineList, Vertex trägt Farbe. egui rendert danach obendrauf
//! (siehe `App::render`).

use crate::camera::Camera;
use crate::scene_geo::Vertex;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    center: [f32; 2],
    scale: f32,
    _pad: f32,
    viewport: [f32; 2],
    _pad2: [f32; 2],
}

/// Kapselt Device/Queue/Surface + die Canvas-Pipeline. egui-State liegt separat.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    // Dynamischer Vertex-Buffer (gecachte Szene); wächst bei Bedarf.
    vbuf: wgpu::Buffer,
    vbuf_cap: u64,
    // Overlay-Buffer (Transform-Handles, Marquee): jeden Frame neu, klein,
    // kamera-abhängig — bleibt aus dem Szene-Cache heraus.
    obuf: wgpu::Buffer,
    obuf_cap: u64,
    ocount: u32,
    // Grid-Buffer (viewportfüllendes Gitter): kamera-abhängig, wird nur bei
    // Kamera-/Rasteränderung neu hochgeladen.
    gbuf: wgpu::Buffer,
    gbuf_cap: u64,
    gcount: u32,
}

impl Gpu {
    pub async fn new(window: std::sync::Arc<winit::window::Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("kein GPU-Adapter");
        log::info!("GPU: {}", adapter.get_info().name);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .unwrap();

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });
        let pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lines"),
            layout: Some(&pl_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    // pos, dir, side, color
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Float32x4],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let vbuf_cap = 1 << 16;
        let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vertices"),
            size: vbuf_cap * std::mem::size_of::<Vertex>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let obuf_cap = 1024;
        let obuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay"),
            size: obuf_cap * std::mem::size_of::<Vertex>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let gbuf_cap = 1 << 13;
        let gbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid"),
            size: gbuf_cap * std::mem::size_of::<Vertex>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device,
            queue,
            surface,
            config,
            pipeline,
            uniform_buf,
            bind,
            vbuf,
            vbuf_cap,
            obuf,
            obuf_cap,
            ocount: 0,
            gbuf,
            gbuf_cap,
            gcount: 0,
        }
    }

    /// Overlay-Vertices (Handles/Marquee) hochladen — jeden Frame, klein.
    pub fn upload_overlay(&mut self, verts: &[Vertex]) {
        let need = verts.len() as u64;
        if need > self.obuf_cap {
            self.obuf_cap = need.next_power_of_two().max(1);
            self.obuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("overlay"),
                size: self.obuf_cap * std::mem::size_of::<Vertex>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !verts.is_empty() {
            self.queue
                .write_buffer(&self.obuf, 0, bytemuck::cast_slice(verts));
        }
        self.ocount = verts.len() as u32;
    }

    /// Zeichnet das Overlay (nach der Szene, gleiche Pipeline/Kamera).
    pub fn draw_overlay<'a>(&'a self, rp: &mut wgpu::RenderPass<'a>) {
        if self.ocount == 0 {
            return;
        }
        rp.set_pipeline(&self.pipeline);
        rp.set_bind_group(0, &self.bind, &[]);
        rp.set_vertex_buffer(0, self.obuf.slice(..));
        rp.draw(0..self.ocount, 0..1);
    }

    /// Grid-Vertices hochladen — nur bei Kamera-/Rasteränderung, klein.
    pub fn upload_grid(&mut self, verts: &[Vertex]) {
        let need = verts.len() as u64;
        if need > self.gbuf_cap {
            self.gbuf_cap = need.next_power_of_two().max(1);
            self.gbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("grid"),
                size: self.gbuf_cap * std::mem::size_of::<Vertex>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !verts.is_empty() {
            self.queue
                .write_buffer(&self.gbuf, 0, bytemuck::cast_slice(verts));
        }
        self.gcount = verts.len() as u32;
    }

    /// Zeichnet das Gitter (nach der Bett-Fläche, vor den Inhalten).
    pub fn draw_grid<'a>(&'a self, rp: &mut wgpu::RenderPass<'a>) {
        if self.gcount == 0 {
            return;
        }
        rp.set_pipeline(&self.pipeline);
        rp.set_bind_group(0, &self.bind, &[]);
        rp.set_vertex_buffer(0, self.gbuf.slice(..));
        rp.draw(0..self.gcount, 0..1);
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w.max(1);
        self.config.height = h.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    /// Schreibt die Kamera-Uniforms (jeden Frame — billig, ein kleiner Buffer).
    pub fn upload_camera(&mut self, cam: &Camera) {
        let uni = Uniforms {
            center: cam.center,
            scale: cam.scale,
            _pad: 0.0,
            viewport: cam.viewport,
            _pad2: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uni));
    }

    /// Lädt die Vertices in den (ggf. vergrößerten) Buffer. Nur bei
    /// Szenen-Änderung aufrufen, nicht pro Frame.
    pub fn upload_verts(&mut self, verts: &[Vertex]) {
        let need = verts.len() as u64;
        if need > self.vbuf_cap {
            self.vbuf_cap = need.next_power_of_two().max(1);
            self.vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vertices"),
                size: self.vbuf_cap * std::mem::size_of::<Vertex>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !verts.is_empty() {
            self.queue
                .write_buffer(&self.vbuf, 0, bytemuck::cast_slice(verts));
        }
    }

    /// Zeichnet die hochgeladenen Linien in den Render-Pass (Clear + Canvas).
    /// egui zeichnet danach in denselben Encoder (siehe App).
    pub fn draw_canvas_range<'a>(
        &'a self,
        rp: &mut wgpu::RenderPass<'a>,
        range: std::ops::Range<u32>,
    ) {
        if range.is_empty() {
            return;
        }
        rp.set_pipeline(&self.pipeline);
        rp.set_bind_group(0, &self.bind, &[]);
        rp.set_vertex_buffer(0, self.vbuf.slice(..));
        rp.draw(range, 0..1);
    }
}

const SHADER: &str = r#"
struct U { center: vec2<f32>, scale: f32, _p: f32, viewport: vec2<f32>, _p2: vec2<f32> };
@group(0) @binding(0) var<uniform> u: U;
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) col: vec4<f32> };

// Halbe Linienbreite in Pixeln (bildschirm-konstant).
const HALF_W: f32 = 0.9;

@vertex
fn vs(@location(0) p: vec2<f32>, @location(1) dir: vec2<f32>,
      @location(2) side: f32, @location(3) c: vec4<f32>) -> VOut {
    // Welt → Pixel (relativ zum Zentrum).
    var px = (p - u.center) * u.scale;
    // Senkrechte zur Segmentrichtung, um HALF_W Pixel zur Seite versetzen.
    let n = vec2<f32>(-dir.y, dir.x);
    px = px + n * side * HALF_W;
    let ndc = vec2<f32>(px.x / (u.viewport.x * 0.5), -px.y / (u.viewport.y * 0.5));
    var o: VOut;
    o.pos = vec4<f32>(ndc, 0.0, 1.0);
    o.col = c;
    return o;
}

@fragment
fn fs(v: VOut) -> @location(0) vec4<f32> { return v.col; }
"#;
