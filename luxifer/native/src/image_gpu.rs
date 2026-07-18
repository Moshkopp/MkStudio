//! Bild-Rendering im Canvas: importierte Assets (Graustufe) als GPU-Texturen an
//! ihrer mm-Box. Eigene Pipeline (texturiertes Quad), dieselben Kamera-Uniforms
//! wie der Linien-Renderer. Der Core liefert die Pixel (`load_asset_luma`).

use std::collections::HashMap;

use luxifer_core::state::AppState;
use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::gpu::Gpu;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ImgVertex {
    pos: [f32; 2],
    uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    center: [f32; 2],
    scale: f32,
    _pad: f32,
    viewport: [f32; 2],
    _pad2: [f32; 2],
}

/// Eine geladene Bild-Textur samt Bind-Group.
struct Tex {
    bind: wgpu::BindGroup,
}

/// Eine verarbeitete Job-Rastertextur (Preview) samt mm-Platzierung.
struct RasterQuad {
    tex: Tex,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Hält Pipeline, Sampler, Uniform-Buffer und die geladenen Texturen je Asset-ID
/// sowie die Rastertexturen der Laser-Vorschau.
#[derive(Default)]
pub struct ImageStore {
    pipeline: Option<wgpu::RenderPipeline>,
    sampler: Option<wgpu::Sampler>,
    tex_layout: Option<wgpu::BindGroupLayout>,
    uniform_buf: Option<wgpu::Buffer>,
    uni_bind: Option<wgpu::BindGroup>,
    textures: HashMap<String, Tex>,
    design_vbuf: Option<wgpu::Buffer>,
    design_ranges: Vec<(String, u32, u32)>,
    /// Verarbeitete Rasterungen für den Preview-Reiter (ersetzen dort die
    /// Design-Texturen). Werden beim Preview-Aufbau gesetzt.
    rasters: Vec<RasterQuad>,
}

impl ImageStore {
    pub fn insert_rgba(&mut self, gpu: &Gpu, asset: &str, rgba: &[u8], size: (u32, u32)) {
        let (width, height) = size;
        self.ensure_pipeline(&gpu.device, gpu.config.format, gpu.sample_count);
        let texture = self.upload_rgba(&gpu.device, &gpu.queue, rgba, width, height);
        self.textures.insert(asset.to_owned(), texture);
    }

    /// Baut die Pipeline lazy (beim ersten Bild). Trennt die einmalige
    /// GPU-Objekt-Erzeugung von der Textur-Verwaltung.
    fn ensure_pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) {
        if self.pipeline.is_some() {
            return;
        }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("img_uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uni_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let uni_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &uni_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&uni_layout), Some(&tex_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image"),
            layout: Some(&pl_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<ImgVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: sample_count,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        self.pipeline = Some(pipeline);
        self.sampler = Some(sampler);
        self.tex_layout = Some(tex_layout);
        self.uniform_buf = Some(uniform_buf);
        self.uni_bind = Some(uni_bind);
    }

    /// Lädt fehlende Texturen für alle Image-Shapes aus dem Asset-Store.
    pub fn sync(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        sample_count: u32,
        state: &AppState,
    ) {
        let dir = luxifer_core::assets_dir();
        for s in &state.shapes {
            if let luxifer_core::Geo::Image { asset, .. } = &s.geo {
                if self.textures.contains_key(asset) {
                    continue;
                }
                self.ensure_pipeline(device, format, sample_count);
                match luxifer_core::load_asset_rgba(&dir, asset) {
                    Ok((rgba, w, h)) => {
                        let tex = self.upload_rgba(device, queue, &rgba, w, h);
                        self.textures.insert(asset.clone(), tex);
                    }
                    Err(e) => log::error!("Asset {asset} laden: {e}"),
                }
            }
        }
        self.rebuild_design_geometry(device, state);
    }

    /// Baut die sechs Quad-Vertices je Image-Shape ausschließlich bei einer
    /// Szenenänderung. Der Frame-Pfad bindet danach nur noch diesen Buffer und
    /// die bereits gecachten Texturen.
    fn rebuild_design_geometry(&mut self, device: &wgpu::Device, state: &AppState) {
        let mut verts = Vec::new();
        self.design_ranges.clear();
        for shape in &state.shapes {
            if let luxifer_core::Geo::Image {
                asset, x, y, w, h, ..
            } = &shape.geo
            {
                if !self.textures.contains_key(asset) {
                    continue;
                }
                let start = verts.len() as u32;
                Self::push_rotated_quad(
                    &mut verts,
                    *x as f32,
                    *y as f32,
                    (*x + *w) as f32,
                    (*y + *h) as f32,
                    shape.rotation,
                );
                self.design_ranges.push((asset.clone(), start, start + 6));
            }
        }
        self.design_vbuf = (!verts.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("img_quads_cached"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
    }

    /// Übersetzt die Job-Rastertexturen (Pixel 255 = gebrannt) in GPU-Texturen
    /// für den Preview-Reiter. Gebrannte Pixel erscheinen in der Brennfarbe
    /// des gewählten Materials, nicht gebrannte bleiben transparent (das
    /// Werkstück scheint durch).
    pub fn set_rasters(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        sample_count: u32,
        rasters: &[luxifer_core::RasterTexture],
        burn_color: [f32; 4],
    ) {
        self.rasters.clear();
        for r in rasters {
            if r.width == 0 || r.height == 0 {
                continue;
            }
            self.ensure_pipeline(device, format, sample_count);
            let burn = [
                (burn_color[0] * 255.0) as u8,
                (burn_color[1] * 255.0) as u8,
                (burn_color[2] * 255.0) as u8,
                235,
            ];
            let mut rgba = Vec::with_capacity(r.pixels.len() * 4);
            for &v in &r.pixels {
                if v >= 128 {
                    rgba.extend_from_slice(&burn);
                } else {
                    rgba.extend_from_slice(&[0, 0, 0, 0]);
                }
            }
            let tex = self.upload_rgba(device, queue, &rgba, r.width, r.height);
            self.rasters.push(RasterQuad {
                tex,
                x: r.x as f32,
                y: r.y as f32,
                w: r.w as f32,
                h: r.h as f32,
            });
        }
    }

    fn upload_rgba(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        w: u32,
        h: u32,
    ) -> Tex {
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("asset"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        );
        let view = texture.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: self.tex_layout.as_ref().unwrap(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(self.sampler.as_ref().unwrap()),
                },
            ],
        });
        Tex { bind }
    }

    /// Schreibt die Kamera-Uniforms für die Bild-Pipeline.
    fn write_camera(&self, gpu: &Gpu, cam: &Camera, uni_buf: &wgpu::Buffer) {
        let uni = Uniforms {
            center: cam.center,
            scale: cam.scale,
            _pad: 0.0,
            viewport: cam.viewport,
            _pad2: [0.0, 0.0],
        };
        gpu.queue.write_buffer(uni_buf, 0, bytemuck::bytes_of(&uni));
    }

    /// Zwei Dreiecke einer mm-Box, UV oben-links = (0,0).
    fn push_quad(verts: &mut Vec<ImgVertex>, x0: f32, y0: f32, x1: f32, y1: f32) {
        let quad = [
            ([x0, y0], [0.0, 0.0]),
            ([x1, y0], [1.0, 0.0]),
            ([x1, y1], [1.0, 1.0]),
            ([x0, y0], [0.0, 0.0]),
            ([x1, y1], [1.0, 1.0]),
            ([x0, y1], [0.0, 1.0]),
        ];
        for (pos, uv) in quad {
            verts.push(ImgVertex { pos, uv });
        }
    }

    fn push_rotated_quad(
        verts: &mut Vec<ImgVertex>,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        degrees: f64,
    ) {
        if degrees.abs() <= f64::EPSILON {
            Self::push_quad(verts, x0, y0, x1, y1);
            return;
        }
        let center = [((x0 + x1) * 0.5) as f64, ((y0 + y1) * 0.5) as f64];
        let rotate = |p: [f32; 2]| {
            let (x, y) = luxifer_core::geometry::rotate_point(
                p[0] as f64,
                p[1] as f64,
                center[0],
                center[1],
                degrees,
            );
            [x as f32, y as f32]
        };
        let p = [
            rotate([x0, y0]),
            rotate([x1, y0]),
            rotate([x1, y1]),
            rotate([x0, y1]),
        ];
        for (index, uv) in [
            (0, [0.0, 0.0]),
            (1, [1.0, 0.0]),
            (2, [1.0, 1.0]),
            (0, [0.0, 0.0]),
            (2, [1.0, 1.0]),
            (3, [0.0, 1.0]),
        ] {
            verts.push(ImgVertex { pos: p[index], uv });
        }
    }

    /// Zeichnet alle Image-Shapes als texturierte Quads in den Render-Pass
    /// (Design-Ansicht: die Original-Graustufen an ihrer mm-Box).
    pub fn draw<'a>(&'a self, rp: &mut wgpu::RenderPass<'a>, gpu: &Gpu, cam: &Camera) {
        let (Some(pipeline), Some(uni_buf), Some(uni_bind)) = (
            self.pipeline.as_ref(),
            self.uniform_buf.as_ref(),
            self.uni_bind.as_ref(),
        ) else {
            return;
        };
        self.write_camera(gpu, cam, uni_buf);
        let Some(buf) = self.design_vbuf.as_ref() else {
            return;
        };

        rp.set_pipeline(pipeline);
        rp.set_bind_group(0, uni_bind, &[]);
        rp.set_vertex_buffer(0, buf.slice(..));
        for (asset, start, end) in &self.design_ranges {
            if let Some(tex) = self.textures.get(asset) {
                rp.set_bind_group(1, &tex.bind, &[]);
                rp.draw(*start..*end, 0..1);
            }
        }
    }

    /// Zeichnet die verarbeiteten Preview-Rasterungen (Laser-Vorschau) als
    /// texturierte Quads — statt der Design-Texturen.
    pub fn draw_rasters<'a>(
        &'a self,
        rp: &mut wgpu::RenderPass<'a>,
        gpu: &Gpu,
        cam: &Camera,
        scratch: &'a mut Option<wgpu::Buffer>,
    ) {
        if self.rasters.is_empty() {
            return;
        }
        let (Some(pipeline), Some(uni_buf), Some(uni_bind)) = (
            self.pipeline.as_ref(),
            self.uniform_buf.as_ref(),
            self.uni_bind.as_ref(),
        ) else {
            return;
        };
        self.write_camera(gpu, cam, uni_buf);

        let mut verts: Vec<ImgVertex> = Vec::new();
        for r in &self.rasters {
            Self::push_quad(&mut verts, r.x, r.y, r.x + r.w, r.y + r.h);
        }
        let buf = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("raster_quads"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
        *scratch = Some(buf);
        let buf = scratch.as_ref().unwrap();

        rp.set_pipeline(pipeline);
        rp.set_bind_group(0, uni_bind, &[]);
        rp.set_vertex_buffer(0, buf.slice(..));
        for (i, r) in self.rasters.iter().enumerate() {
            let start = (i as u32) * 6;
            rp.set_bind_group(1, &r.tex.bind, &[]);
            rp.draw(start..start + 6, 0..1);
        }
    }
}

const SHADER: &str = r#"
struct U { center: vec2<f32>, scale: f32, _p: f32, viewport: vec2<f32>, _p2: vec2<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@location(0) p: vec2<f32>, @location(1) uv: vec2<f32>) -> VOut {
    let px = (p - u.center) * u.scale;
    let ndc = vec2<f32>(px.x / (u.viewport.x * 0.5), -px.y / (u.viewport.y * 0.5));
    var o: VOut;
    o.pos = vec4<f32>(ndc, 0.0, 1.0);
    o.uv = uv;
    return o;
}

@fragment
fn fs(v: VOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, v.uv);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bild_quad_rotiert_um_seinen_mittelpunkt() {
        let mut vertices = Vec::new();
        ImageStore::push_rotated_quad(&mut vertices, 0.0, 0.0, 100.0, 50.0, 90.0);

        assert_eq!(vertices.len(), 6);
        // Oben links (0,0) rotiert um (50,25) nach (75,-25); UV bleibt am
        // Bildpunkt hängen und darf nicht achsenparallel zurückbleiben.
        assert!((vertices[0].pos[0] - 75.0).abs() < 1e-4);
        assert!((vertices[0].pos[1] + 25.0).abs() < 1e-4);
        assert_eq!(vertices[0].uv, [0.0, 0.0]);
        assert!((vertices[2].pos[0] - 25.0).abs() < 1e-4);
        assert!((vertices[2].pos[1] - 75.0).abs() < 1e-4);
        assert_eq!(vertices[2].uv, [1.0, 1.0]);
    }
}
