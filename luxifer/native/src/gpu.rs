//! wgpu-Canvas: rendert die Linien-Vertices (Welt-mm) mit Kamera-Projektion.
//! Ein Pipeline, LineList, Vertex trägt Farbe. egui rendert danach obendrauf
//! (siehe `App::render`).

use crate::camera::Camera;
use crate::scene_geo::{FillBatch, Vertex};

const STENCIL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionTransform {
    pub matrix: [f32; 4],
    pub pivot: [f32; 2],
    pub offset: [f32; 2],
}

impl Default for SelectionTransform {
    fn default() -> Self {
        Self {
            matrix: [1.0, 0.0, 0.0, 1.0],
            pivot: [0.0; 2],
            offset: [0.0; 2],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    center: [f32; 2],
    scale: f32,
    _pad: f32,
    viewport: [f32; 2],
    line_aa: f32,
    _pad2: f32,
    offset: [f32; 2],
    _pad3: [f32; 2],
    matrix: [f32; 4],
    pivot: [f32; 2],
    _pad4: [f32; 2],
}

/// Kapselt Device/Queue/Surface + die Canvas-Pipeline. egui-State liegt separat.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub sample_count: u32,
    pipeline: wgpu::RenderPipeline,
    fill_stencil_pipeline: wgpu::RenderPipeline,
    fill_union_pipeline: wgpu::RenderPipeline,
    fill_temp_clear_pipeline: wgpu::RenderPipeline,
    fill_color_pipeline: wgpu::RenderPipeline,
    fill_clear_pipeline: wgpu::RenderPipeline,
    stencil_view: wgpu::TextureView,
    msaa_view: Option<wgpu::TextureView>,
    line_antialiasing: bool,
    uniform_buf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    selection_uniform_buf: wgpu::Buffer,
    selection_bind: wgpu::BindGroup,
    // Dynamischer Vertex-Buffer (gecachte Szene); wächst bei Bedarf.
    vbuf: wgpu::Buffer,
    vbuf_cap: u64,
    fill_vbuf: wgpu::Buffer,
    fill_vbuf_cap: u64,
    // Overlay-Buffer (Transform-Handles, Marquee): jeden Frame neu, klein,
    // kamera-abhängig — bleibt aus dem Szene-Cache heraus.
    obuf: wgpu::Buffer,
    obuf_cap: u64,
    ocount: u32,
    // Auswahlkonturen: szenengroß, deshalb persistent und getrennt vom kleinen
    // dynamischen Overlay. Wird nur bei Auswahl-/Geometrieänderung hochgeladen.
    sbuf: wgpu::Buffer,
    sbuf_cap: u64,
    scount: u32,
    // Grid-Buffer (viewportfüllendes Gitter): kamera-abhängig, wird nur bei
    // Kamera-/Rasteränderung neu hochgeladen.
    gbuf: wgpu::Buffer,
    gbuf_cap: u64,
    gcount: u32,
}

impl Gpu {
    pub async fn new(
        window: std::sync::Arc<winit::window::Window>,
        requested_samples: u32,
        line_antialiasing: bool,
    ) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| format!("GPU-Oberfläche konnte nicht erstellt werden: {error}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|error| format!("Kein geeigneter GPU-Adapter verfügbar: {error}"))?;
        log::info!("GPU: {}", adapter.get_info().name);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|error| format!("GPU-Gerät konnte nicht geöffnet werden: {error}"))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| "GPU-Oberfläche bietet kein Texturformat an.".to_owned())?;
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .ok_or_else(|| "GPU-Oberfläche bietet keinen Alpha-Modus an.".to_owned())?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let color_features = adapter.get_texture_format_features(format);
        let stencil_features = adapter.get_texture_format_features(STENCIL_FORMAT);
        let sample_count = [16, 8, 4, 2, 1]
            .into_iter()
            .find(|&samples| {
                samples <= requested_samples
                    && color_features.flags.sample_count_supported(samples)
                    && stencil_features.flags.sample_count_supported(samples)
                    && (samples == 1
                        || color_features
                            .flags
                            .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE))
            })
            .unwrap_or(1);
        if sample_count != requested_samples {
            log::warn!("MSAA {requested_samples}x nicht unterstützt; verwende {sample_count}x");
        }

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
        let selection_uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("selection-uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
        let selection_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("selection-bind"),
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: selection_uniform_buf.as_entire_binding(),
            }],
        });
        let pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lines"),
            layout: Some(&pl_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
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
                entry_point: Some("fs"),
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
            multisample: wgpu::MultisampleState {
                count: sample_count,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
        let stencil_face = |compare, pass_op| wgpu::StencilFaceState {
            compare,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op,
        };
        let make_fill_pipeline = |label, color_write, compare, pass_op, read_mask, write_mask| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pl_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32, 3 => Float32x4],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: color_write,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: STENCIL_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState {
                        front: stencil_face(compare, pass_op),
                        back: stencil_face(compare, pass_op),
                        read_mask,
                        write_mask,
                    },
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: sample_count,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        };
        let fill_stencil_pipeline = make_fill_pipeline(
            "fill-stencil",
            wgpu::ColorWrites::empty(),
            wgpu::CompareFunction::Always,
            wgpu::StencilOperation::Invert,
            0x01,
            0x01,
        );
        let fill_union_pipeline = make_fill_pipeline(
            "fill-union",
            wgpu::ColorWrites::empty(),
            wgpu::CompareFunction::NotEqual,
            wgpu::StencilOperation::Replace,
            0x01,
            0x02,
        );
        let fill_temp_clear_pipeline = make_fill_pipeline(
            "fill-temp-clear",
            wgpu::ColorWrites::empty(),
            wgpu::CompareFunction::Always,
            wgpu::StencilOperation::Replace,
            0x01,
            0x01,
        );
        let fill_color_pipeline = make_fill_pipeline(
            "fill-color",
            wgpu::ColorWrites::ALL,
            wgpu::CompareFunction::NotEqual,
            wgpu::StencilOperation::Keep,
            0x02,
            0,
        );
        let fill_clear_pipeline = make_fill_pipeline(
            "fill-clear",
            wgpu::ColorWrites::empty(),
            wgpu::CompareFunction::Always,
            wgpu::StencilOperation::Zero,
            0x02,
            0x02,
        );
        let stencil_view =
            stencil_view(&device, size.width.max(1), size.height.max(1), sample_count);
        let msaa_view = msaa_view(
            &device,
            format,
            size.width.max(1),
            size.height.max(1),
            sample_count,
        );

        let vbuf_cap = 1 << 16;
        let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vertices"),
            size: vbuf_cap * std::mem::size_of::<Vertex>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let fill_vbuf_cap = 1 << 16;
        let fill_vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fill-vertices"),
            size: fill_vbuf_cap * std::mem::size_of::<Vertex>() as u64,
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
        let sbuf_cap = 1 << 16;
        let sbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("selection-outlines"),
            size: sbuf_cap * std::mem::size_of::<Vertex>() as u64,
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

        Ok(Self {
            device,
            queue,
            surface,
            config,
            sample_count,
            pipeline,
            fill_stencil_pipeline,
            fill_union_pipeline,
            fill_temp_clear_pipeline,
            fill_color_pipeline,
            fill_clear_pipeline,
            stencil_view,
            msaa_view,
            line_antialiasing,
            uniform_buf,
            bind,
            selection_uniform_buf,
            selection_bind,
            vbuf,
            vbuf_cap,
            fill_vbuf,
            fill_vbuf_cap,
            obuf,
            obuf_cap,
            ocount: 0,
            sbuf,
            sbuf_cap,
            scount: 0,
            gbuf,
            gbuf_cap,
            gcount: 0,
        })
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

    pub fn upload_selection(&mut self, verts: &[Vertex]) {
        let need = verts.len() as u64;
        if need > self.sbuf_cap {
            self.sbuf_cap = need.next_power_of_two().max(1);
            self.sbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("selection-outlines"),
                size: self.sbuf_cap * std::mem::size_of::<Vertex>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !verts.is_empty() {
            self.queue
                .write_buffer(&self.sbuf, 0, bytemuck::cast_slice(verts));
        }
        self.scount = verts.len() as u32;
    }

    pub fn draw_selection<'a>(&'a self, rp: &mut wgpu::RenderPass<'a>) {
        if self.scount == 0 {
            return;
        }
        rp.set_pipeline(&self.pipeline);
        rp.set_bind_group(0, &self.selection_bind, &[]);
        rp.set_vertex_buffer(0, self.sbuf.slice(..));
        rp.draw(0..self.scount, 0..1);
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

    /// Zeichnet das Gitter (nach dem Bettrahmen, vor den Inhalten).
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
        self.stencil_view = stencil_view(
            &self.device,
            self.config.width,
            self.config.height,
            self.sample_count,
        );
        self.msaa_view = msaa_view(
            &self.device,
            self.config.format,
            self.config.width,
            self.config.height,
            self.sample_count,
        );
    }

    /// Schreibt die Kamera-Uniforms (jeden Frame — billig, ein kleiner Buffer).
    pub fn upload_camera(&mut self, cam: &Camera, transform: SelectionTransform) {
        let make = |transform: SelectionTransform| Uniforms {
            center: cam.center,
            scale: cam.scale,
            _pad: 0.0,
            viewport: cam.viewport,
            line_aa: f32::from(self.line_antialiasing),
            _pad2: 0.0,
            offset: transform.offset,
            _pad3: [0.0; 2],
            matrix: transform.matrix,
            pivot: transform.pivot,
            _pad4: [0.0; 2],
        };
        self.queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::bytes_of(&make(Default::default())),
        );
        self.queue.write_buffer(
            &self.selection_uniform_buf,
            0,
            bytemuck::bytes_of(&make(transform)),
        );
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

    pub fn upload_fill_verts(&mut self, verts: &[Vertex]) {
        let need = verts.len() as u64;
        if need > self.fill_vbuf_cap {
            self.fill_vbuf_cap = need.next_power_of_two().max(1);
            self.fill_vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("fill-vertices"),
                size: self.fill_vbuf_cap * std::mem::size_of::<Vertex>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !verts.is_empty() {
            self.queue
                .write_buffer(&self.fill_vbuf, 0, bytemuck::cast_slice(verts));
        }
    }

    pub fn stencil_view(&self) -> &wgpu::TextureView {
        &self.stencil_view
    }

    pub fn color_view<'a>(&'a self, surface: &'a wgpu::TextureView) -> &'a wgpu::TextureView {
        self.msaa_view.as_ref().unwrap_or(surface)
    }

    pub fn draw_solid_fills<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        batches: &[FillBatch],
        use_selection_transform: bool,
    ) {
        if batches.is_empty() {
            return;
        }
        pass.set_bind_group(
            0,
            if use_selection_transform {
                &self.selection_bind
            } else {
                &self.bind
            },
            &[],
        );
        pass.set_vertex_buffer(0, self.fill_vbuf.slice(..));
        for batch in batches {
            for compound in &batch.compounds {
                pass.set_stencil_reference(0);
                pass.set_pipeline(&self.fill_stencil_pipeline);
                pass.draw(compound.stencil.clone(), 0..1);
                // Temp-Parität (Bit 0) in die Layer-Union (Bit 1) übernehmen.
                pass.set_stencil_reference(0x02);
                pass.set_pipeline(&self.fill_union_pipeline);
                pass.draw(compound.cover.clone(), 0..1);
                pass.set_stencil_reference(0);
                pass.set_pipeline(&self.fill_temp_clear_pipeline);
                pass.draw(compound.cover.clone(), 0..1);
            }
            pass.set_stencil_reference(0);
            pass.set_pipeline(&self.fill_color_pipeline);
            pass.draw(batch.cover.clone(), 0..1);
            pass.set_pipeline(&self.fill_clear_pipeline);
            pass.draw(batch.cover.clone(), 0..1);
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

fn stencil_view(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    sample_count: u32,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("canvas-stencil"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: STENCIL_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn msaa_view(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    sample_count: u32,
) -> Option<wgpu::TextureView> {
    (sample_count > 1).then(|| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("canvas-msaa"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    })
}

const SHADER: &str = r#"
struct U { center: vec2<f32>, scale: f32, _p: f32, viewport: vec2<f32>, line_aa: f32, _p2: f32, offset: vec2<f32>, _p3: vec2<f32>, matrix: vec4<f32>, pivot: vec2<f32>, _p4: vec2<f32> };
@group(0) @binding(0) var<uniform> u: U;
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) col: vec4<f32>, @location(1) edge: f32 };

// Halbe Linienbreite in Pixeln (bildschirm-konstant).
const HALF_W: f32 = 1.1;

@vertex
fn vs(@location(0) p: vec2<f32>, @location(1) dir: vec2<f32>,
      @location(2) side: f32, @location(3) c: vec4<f32>) -> VOut {
    // Welt → Pixel (relativ zum Zentrum).
    let q = p - u.pivot;
    let transformed = vec2<f32>(u.matrix.x * q.x + u.matrix.y * q.y,
                                u.matrix.z * q.x + u.matrix.w * q.y)
                      + u.pivot + u.offset;
    var px = (transformed - u.center) * u.scale;
    // Senkrechte zur Segmentrichtung, um HALF_W Pixel zur Seite versetzen.
    let transformed_dir = normalize(vec2<f32>(
        u.matrix.x * dir.x + u.matrix.y * dir.y,
        u.matrix.z * dir.x + u.matrix.w * dir.y,
    ));
    let n = vec2<f32>(-transformed_dir.y, transformed_dir.x);
    let outer = HALF_W + u.line_aa;
    px = px + n * side * outer;
    let ndc = vec2<f32>(px.x / (u.viewport.x * 0.5), -px.y / (u.viewport.y * 0.5));
    var o: VOut;
    o.pos = vec4<f32>(ndc, 0.0, 1.0);
    o.col = c;
    o.edge = side * outer;
    return o;
}

@fragment
fn fs(v: VOut) -> @location(0) vec4<f32> {
    if u.line_aa < 0.5 {
        return v.col;
    }
    let coverage = 1.0 - smoothstep(HALF_W - u.line_aa, HALF_W + u.line_aa, abs(v.edge));
    return vec4<f32>(v.col.rgb, v.col.a * coverage);
}
"#;
