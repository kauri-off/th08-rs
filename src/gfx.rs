use std::sync::Arc;
use wgpu::util::DeviceExt;

pub const SCREEN_W: f32 = 640.0;
pub const SCREEN_H: f32 = 480.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Blend {
    Alpha,
    Add,
}

pub type TextureId = usize;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}

pub const FULLSCREEN: Rect = Rect::new(0.0, 0.0, SCREEN_W, SCREEN_H);
pub const PLAYFIELD: Rect = Rect::new(32.0, 16.0, 384.0, 448.0);

pub struct Texture {
    pub width: u32,
    pub height: u32,
    bind: wgpu::BindGroup,
}

struct Batch {
    tex: TextureId,
    blend: Blend,
    clip: Rect,
    start: u32,
    end: u32,
}

pub struct Gfx {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipe_alpha: wgpu::RenderPipeline,
    pipe_add: wgpu::RenderPipeline,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: Vec<Option<Texture>>,
    free: Vec<TextureId>,
    viewport: Rect,
    verts: Vec<Vertex>,
    indices: Vec<u32>,
    batches: Vec<Batch>,
    white: TextureId,
}

const SHADER: &str = r#"
struct VsOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f, @location(1) color: vec4f };
@vertex fn vs(@location(0) pos: vec2f, @location(1) uv: vec2f, @location(2) color: vec4f) -> VsOut {
    var o: VsOut;
    o.pos = vec4f(pos.x / 320.0 - 1.0, 1.0 - pos.y / 240.0, 0.0, 1.0);
    o.uv = uv;
    o.color = color;
    return o;
}
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment fn fs(i: VsOut) -> @location(0) vec4f { return textureSample(t, s, i.uv) * i.color; }
"#;

impl Gfx {
    pub async fn new(window: Arc<winit::window::Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::util::new_instance_with_webgpu_detection(wgpu::InstanceDescriptor::new_without_display_handle()).await;
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await?;
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow::anyhow!("surface unsupported"))?;
        let caps = surface.get_capabilities(&adapter);
        let want = [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm];
        if let Some(f) = caps.formats.iter().find(|f| want.contains(f)) {
            config.format = *f;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&tex_layout)],
            immediate_size: 0,
        });
        let make = |blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let add = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let pipe_alpha = make(wgpu::BlendState::ALPHA_BLENDING);
        let pipe_add = make(add);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut g = Self {
            surface,
            device,
            queue,
            config,
            pipe_alpha,
            pipe_add,
            tex_layout,
            sampler,
            textures: Vec::new(),
            free: Vec::new(),
            viewport: FULLSCREEN,
            verts: Vec::new(),
            indices: Vec::new(),
            batches: Vec::new(),
            white: 0,
        };
        g.white = g.create_texture(1, 1, &[255; 4]);
        Ok(g)
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w > 0 && h > 0 && (w, h) != (self.config.width, self.config.height) {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn create_texture(&mut self, width: u32, height: u32, rgba: &[u8]) -> TextureId {
        let tex = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
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
        let view = tex.create_view(&Default::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.tex_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        let t = Some(Texture { width, height, bind });
        if let Some(id) = self.free.pop() {
            self.textures[id] = t;
            id
        } else {
            self.textures.push(t);
            self.textures.len() - 1
        }
    }

    pub fn free_texture(&mut self, id: TextureId) {
        if id != self.white && self.textures.get_mut(id).and_then(Option::take).is_some() {
            self.free.push(id);
        }
    }

    pub fn load_image(&mut self, bytes: &[u8]) -> anyhow::Result<TextureId> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        Ok(self.create_texture(img.width(), img.height(), &img))
    }

    pub fn texture(&self, id: TextureId) -> &Texture {
        self.textures[id].as_ref().expect("freed texture")
    }

    // Subsequent draws are clipped to `r` (screen coords); 2D vertices stay in screen space like D3D pretransformed ones.
    pub fn set_viewport(&mut self, r: Rect) {
        self.viewport = r;
    }

    pub fn viewport(&self) -> Rect {
        self.viewport
    }

    pub fn clear_viewport(&mut self, color: [u8; 4]) {
        let r = self.viewport;
        self.rect(r.x, r.y, r.w, r.h, color);
    }

    pub fn sprite(&mut self, tex: TextureId, blend: Blend, dst: Rect, uv: [f32; 4], color: [u8; 4]) {
        let Rect { x, y, w, h } = dst;
        let [u0, v0, u1, v1] = uv;
        self.quad(tex, blend, [[x, y], [x + w, y], [x + w, y + h], [x, y + h]], [[u0, v0], [u1, v0], [u1, v1], [u0, v1]], color);
    }

    pub fn quad(&mut self, tex: TextureId, blend: Blend, corners: [[f32; 2]; 4], uv: [[f32; 2]; 4], color: [u8; 4]) {
        let base = self.verts.len() as u32;
        for i in 0..4 {
            self.verts.push(Vertex { pos: corners[i], uv: uv[i], color });
        }
        let start = self.indices.len() as u32;
        self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        match self.batches.last_mut() {
            Some(b) if b.tex == tex && b.blend == blend && b.clip == self.viewport => b.end += 6,
            _ => self.batches.push(Batch { tex, blend, clip: self.viewport, start, end: start + 6 }),
        }
    }

    // D3D vertex order: v0 top-left, v1 top-right, v2 bottom-left, v3 bottom-right; triangles (v0 v1 v2), (v1 v2 v3).
    pub fn quad_strip(&mut self, tex: TextureId, blend: Blend, pos: [[f32; 2]; 4], uv: [[f32; 2]; 4], color: [u8; 4]) {
        let base = self.verts.len() as u32;
        for i in 0..4 {
            self.verts.push(Vertex { pos: pos[i], uv: uv[i], color });
        }
        let start = self.indices.len() as u32;
        self.indices.extend([base, base + 1, base + 2, base + 1, base + 2, base + 3]);
        match self.batches.last_mut() {
            Some(b) if b.tex == tex && b.blend == blend && b.clip == self.viewport => b.end += 6,
            _ => self.batches.push(Batch { tex, blend, clip: self.viewport, start, end: start + 6 }),
        }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        self.quad(self.white, Blend::Alpha, [[x, y], [x + w, y], [x + w, y + h], [x, y + h]], uv, color);
    }

    pub fn image(&mut self, tex: TextureId, x: f32, y: f32) {
        let t = self.texture(tex);
        let (w, h) = (t.width as f32, t.height as f32);
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        self.quad(tex, Blend::Alpha, [[x, y], [x + w, y], [x + w, y + h], [x, y + h]], uv, [255; 4]);
    }

    fn encode(&self, enc: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, w: f32, h: f32) {
        let buffers = (!self.verts.is_empty()).then(|| {
            let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&self.verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&self.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            (vb, ib)
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        let scale = (w / SCREEN_W).min(h / SCREEN_H);
        let (vw, vh) = (SCREEN_W * scale, SCREEN_H * scale);
        let (ox, oy) = ((w - vw) / 2.0, (h - vh) / 2.0);
        pass.set_viewport(ox, oy, vw, vh, 0.0, 1.0);
        if let Some((vb, ib)) = &buffers {
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            for b in &self.batches {
                let x0 = (ox + b.clip.x * scale).round().clamp(0.0, w) as u32;
                let y0 = (oy + b.clip.y * scale).round().clamp(0.0, h) as u32;
                let x1 = (ox + (b.clip.x + b.clip.w) * scale).round().clamp(0.0, w) as u32;
                let y1 = (oy + (b.clip.y + b.clip.h) * scale).round().clamp(0.0, h) as u32;
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
                pass.set_pipeline(if b.blend == Blend::Add { &self.pipe_add } else { &self.pipe_alpha });
                pass.set_bind_group(0, &self.texture(b.tex).bind, &[]);
                pass.draw_indexed(b.start..b.end, 0, 0..1);
            }
        }
    }

    pub fn present(&mut self) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => {
                self.surface.configure(&self.device, &self.config);
                self.clear_batches();
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        self.encode(&mut enc, &view, self.config.width as f32, self.config.height as f32);
        self.queue.submit([enc.finish()]);
        self.queue.present(frame);
        self.clear_batches();
    }

    // Renders the pending batches at 640x480 into memory (BGRA/RGBA per surface format → RGBA).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture(&mut self) -> image::RgbaImage {
        let (w, h) = (SCREEN_W as u32, SCREEN_H as u32);
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (w * h * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        self.encode(&mut enc, &tex.create_view(&Default::default()), w as f32, h as f32);
        enc.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let mut px = buf.slice(..).get_mapped_range().unwrap().to_vec();
        if matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
            px.chunks_exact_mut(4).for_each(|p| p.swap(0, 2));
        }
        self.clear_batches();
        image::RgbaImage::from_raw(w, h, px).unwrap()
    }

    fn clear_batches(&mut self) {
        self.viewport = FULLSCREEN;
        self.verts.clear();
        self.indices.clear();
        self.batches.clear();
    }
}

pub type Vec3 = [f32; 3];

// Row-vector convention (p' = p * M, as D3D); the same memory read as WGSL column-major gives M^T, so `m * v` works.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Mat4(pub [[f32; 4]; 4]);

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(a: Vec3) -> Vec3 {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]);

    pub fn scale(x: f32, y: f32, z: f32) -> Self {
        Mat4([[x, 0.0, 0.0, 0.0], [0.0, y, 0.0, 0.0], [0.0, 0.0, z, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }

    pub fn rotation_x(a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Mat4([[1.0, 0.0, 0.0, 0.0], [0.0, c, s, 0.0], [0.0, -s, c, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }

    pub fn rotation_y(a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Mat4([[c, 0.0, -s, 0.0], [0.0, 1.0, 0.0, 0.0], [s, 0.0, c, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }

    pub fn rotation_z(a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Mat4([[c, s, 0.0, 0.0], [-s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]])
    }

    pub fn look_at_lh(eye: Vec3, at: Vec3, up: Vec3) -> Self {
        let z = normalize(sub(at, eye));
        let x = normalize(cross(up, z));
        let y = cross(z, x);
        Mat4([
            [x[0], y[0], z[0], 0.0],
            [x[1], y[1], z[1], 0.0],
            [x[2], y[2], z[2], 0.0],
            [-dot(x, eye), -dot(y, eye), -dot(z, eye), 1.0],
        ])
    }

    pub fn perspective_fov_lh(fovy: f32, aspect: f32, near: f32, far: f32) -> Self {
        let ys = 1.0 / (fovy / 2.0).tan();
        let xs = ys / aspect;
        let q = far / (far - near);
        Mat4([[xs, 0.0, 0.0, 0.0], [0.0, ys, 0.0, 0.0], [0.0, 0.0, q, 1.0], [0.0, 0.0, -near * q, 0.0]])
    }

    pub fn mul(&self, o: &Mat4) -> Mat4 {
        let mut r = [[0.0; 4]; 4];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..4).map(|k| self.0[i][k] * o.0[k][j]).sum();
            }
        }
        Mat4(r)
    }

    pub fn transform(&self, p: Vec3) -> [f32; 4] {
        let m = &self.0;
        let v = [p[0], p[1], p[2], 1.0];
        std::array::from_fn(|j| (0..4).map(|k| v[k] * m[k][j]).sum())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub view: Mat4,
    pub proj: Mat4,
}

impl Camera {
    pub const FOV: f32 = std::f32::consts::PI / 6.0;
    pub const NEAR: f32 = 100.0;
    pub const FAR: f32 = 10000.0;

    pub fn default_2d() -> Self {
        let d = (SCREEN_H / 2.0) / (Self::FOV / 2.0).tan();
        let (cx, cy) = (SCREEN_W / 2.0, -SCREEN_H / 2.0);
        Self {
            view: Mat4::look_at_lh([cx, cy, -d], [cx, cy, 0.0], [0.0, 1.0, 0.0]),
            proj: Mat4::perspective_fov_lh(Self::FOV, SCREEN_W / SCREEN_H, Self::NEAR, Self::FAR),
        }
    }

    // Camera_Set2D: world (x, y, 0) maps to viewport pixel (x, y).
    pub fn set_2d(vp: Rect) -> Self {
        let fov = std::f32::consts::PI / 10.0;
        let (cx, cy) = (vp.w / 2.0, vp.h / 2.0);
        Self {
            view: Mat4::look_at_lh([cx, cy, cy / (fov / 2.0).tan()], [cx, cy, 0.0], [0.0, -1.0, 0.0]),
            proj: Mat4::perspective_fov_lh(fov, vp.w / vp.h, 1.0, 10000.0),
        }
    }

    pub fn view_proj(&self) -> Mat4 {
        self.view.mul(&self.proj)
    }

    // World point → (screen x, screen y, depth 0..1) inside viewport `vp`.
    pub fn project(&self, p: Vec3, vp: Rect) -> Vec3 {
        let c = self.view_proj().transform(p);
        let (x, y, z) = (c[0] / c[3], c[1] / c[3], c[2] / c[3]);
        [vp.x + (x + 1.0) * vp.w / 2.0, vp.y + (1.0 - y) * vp.h / 2.0, z]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_camera_maps_z0_to_pixels() {
        let cam = Camera::default_2d();
        for (x, y) in [(0.0, 0.0), (320.0, 240.0), (640.0, 480.0), (32.0, 16.0), (416.0, 464.0), (100.5, 377.25)] {
            let s = cam.project([x, -y, 0.0], FULLSCREEN);
            assert!((s[0] - x).abs() < 1e-3 && (s[1] - y).abs() < 1e-3, "({x},{y}) -> {s:?}");
            assert!(s[2] > 0.0 && s[2] < 1.0);
        }
    }

    #[test]
    fn set_2d_maps_to_viewport_pixels() {
        let cam = Camera::set_2d(PLAYFIELD);
        for (x, y) in [(0.0, 0.0), (192.0, 224.0), (384.0, 448.0), (10.0, 300.0)] {
            let s = cam.project([x, y, 0.0], PLAYFIELD);
            assert!((s[0] - x - 32.0).abs() < 1e-2 && (s[1] - y - 16.0).abs() < 1e-2, "({x},{y}) -> {s:?}");
        }
    }
}
