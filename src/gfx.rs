//! wgpu renderer standing in for the D3D8 device state the original sets up (05 §9, §12.4–§12.5; 06 §10).

use std::sync::Arc;
use wgpu::util::DeviceExt;

// The original renders at 640×480 (05 §12.4); the window is a scaled copy.
pub const SCREEN_W: f32 = 640.0;
pub const SCREEN_H: f32 = 480.0;

// `pos` in wgpu clip space; `fog` is the vertex fog factor f of 06 §10.1 (1 = no fog), mixed with `fog_color`.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 4],
    pub uv: [f32; 2],
    pub color: [u8; 4],
    pub fog_color: [u8; 4],
    pub fog: f32,
}

impl Vertex {
    // Pre-transformed (D3D rhw = 1) vertex in screen pixels; no fog.
    pub fn screen(x: f32, y: f32, z: f32, uv: [f32; 2], color: [u8; 4]) -> Self {
        Self {
            pos: [x / 320.0 - 1.0, 1.0 - y / 240.0, z, 1.0],
            uv,
            color,
            fog_color: [0; 4],
            fog: 1.0,
        }
    }
}

// D3D clip coords of viewport `vp` → clip coords of the 640x480 target, with the D3D→wgpu half-pixel shift.
pub fn clip_in_viewport(c: [f32; 4], vp: Rect) -> [f32; 4] {
    let (ax, bx) = (vp.w / SCREEN_W, (vp.x + vp.w / 2.0 + 0.5) / 320.0 - 1.0);
    let (ay, by) = (vp.h / SCREEN_H, 1.0 - (vp.y + vp.h / 2.0 + 0.5) / 240.0);
    [ax * c[0] + bx * c[3], ay * c[1] + by * c[3], c[2], c[3]]
}

// SRCBLEND SRCALPHA with DESTBLEND INVSRCALPHA (Alpha) or ONE (Add) (05 §12.4, §2.1 bits 4–5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Blend {
    Alpha,
    Add,
}

// Port handle; free slots are reused.
pub type TextureId = usize;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Draw,
    ClearColor,
    ClearDepth,
}

// Render state of a batch: blend, ZFUNC LESSEQUAL (`test`) vs ALWAYS, ZWRITEENABLE (05 §12.4, §2.1 bit 13).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct PipeKey {
    blend: Blend,
    test: bool,
    write: bool,
    kind: Kind,
}

impl PipeKey {
    fn index(self) -> usize {
        match self.kind {
            Kind::ClearColor => 8,
            Kind::ClearDepth => 9,
            Kind::Draw => {
                (self.blend == Blend::Add) as usize
                    | (self.test as usize) << 1
                    | (self.write as usize) << 2
            }
        }
    }
}

// Port choice; the D3D8 depth format is not specified.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

// Viewport / rectangle in 640×480 screen pixels.
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
// Playfield viewport (32,16)–(416,464) (06 §10.2 step 5).
pub const PLAYFIELD: Rect = Rect::new(32.0, 16.0, 384.0, 448.0);

pub struct Texture {
    pub width: u32,
    pub height: u32,
    bind: wgpu::BindGroup,
    tex: wgpu::Texture,
    label: Option<LogTex>,
}

// Draw-log attribution of a texture: `<anm file>#<entry>` or an image file name, with the logical size.
#[derive(Clone, Debug, PartialEq)]
pub struct LogTex {
    pub label: Arc<str>,
    pub w: u32,
    pub h: u32,
}

// Fog colour / start / end of the draw (06 §10.1), recorded in the draw log.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogFog {
    pub color: [u8; 3],
    pub start: f32,
    pub end: f32,
}

// One recorded draw call (TH08_DRAWLOG); vertices in v0 TL, v1 TR, v2 BL, v3 BR order.
#[derive(Clone, Debug)]
pub enum LogRec {
    Quad {
        tex: Option<LogTex>,
        id: TextureId,
        textured: bool,
        blend: Blend,
        test: bool,
        write: bool,
        vp: Rect,
        fog: Option<LogFog>,
        v: [Vertex; 4],
    },
    Clear {
        vp: Rect,
        color: Option<[u8; 4]>,
    },
    Copy {
        src: String,
        dst: String,
        from: [u32; 2],
        size: [u32; 2],
        to: [u32; 2],
    },
    Fan {
        blend: Blend,
        vp: Rect,
        v: Vec<Vertex>,
    },
}

// Consecutive quads with equal texture, state and viewport, drawn in one call (05 §9 EmitQuad step 5).
struct Batch {
    tex: TextureId,
    key: PipeKey,
    clip: Rect,
    start: u32,
    end: u32,
}

// Port-only GPU state: device, pipelines, texture table, pending batches and the 640×480 frame target.
pub struct Gfx {
    pub surface: Option<wgpu::Surface<'static>>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipes: Vec<wgpu::RenderPipeline>,
    depth: Option<(u32, u32, wgpu::TextureView)>,
    depth_test: bool,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: Vec<Option<Texture>>,
    free: Vec<TextureId>,
    viewport: Rect,
    verts: Vec<Vertex>,
    indices: Vec<u32>,
    batches: Vec<Batch>,
    white: TextureId,
    frame: wgpu::Texture,
    frame_bind: wgpu::BindGroup,
    blit: wgpu::RenderPipeline,
    log: Option<Vec<LogRec>>,
    log_fog: Option<LogFog>,
    pub persistent: bool,
}

// Texture × diffuse (05 §12.4 stage 0 MODULATE), alpha test ALPHAREF 4 / GREATEREQUAL, then 06 §10.1 step 3 fog mix.
const SHADER: &str = r#"
struct VsOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f, @location(1) color: vec4f, @location(2) fogc: vec4f, @location(3) fog: f32 };
@vertex fn vs(@location(0) pos: vec4f, @location(1) uv: vec2f, @location(2) color: vec4f, @location(3) fogc: vec4f, @location(4) fog: f32) -> VsOut {
    var o: VsOut;
    o.pos = pos;
    o.uv = uv;
    o.color = color;
    o.fogc = fogc;
    o.fog = fog;
    return o;
}
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment fn fs(i: VsOut) -> @location(0) vec4f {
    let c = textureSample(t, s, i.uv) * i.color;
    if c.a * 255.0 < 3.5 { discard; }
    return vec4f(mix(i.fogc.rgb, c.rgb, clamp(i.fog, 0.0, 1.0)), c.a);
}
@fragment fn fs_clear(i: VsOut) -> @location(0) vec4f { return i.color; }
"#;

const BLIT: &str = r#"
struct VsOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f };
@vertex fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    let uv = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VsOut;
    o.pos = vec4f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    o.uv = uv;
    return o;
}
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment fn fs(i: VsOut) -> @location(0) vec4f { return textureSample(t, s, i.uv); }
"#;

impl Gfx {
    pub async fn new(window: Arc<winit::window::Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::util::new_instance_with_webgpu_detection(
            wgpu::InstanceDescriptor::new_without_display_handle(),
        )
        .await;
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;
        let info = adapter.get_info();
        log::info!(
            "adapter: {} ({:?}, {:?})",
            info.name,
            info.backend,
            info.device_type
        );
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await?;
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow::anyhow!("surface unsupported"))?;
        let caps = surface.get_capabilities(&adapter);
        let want = [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8Unorm,
        ];
        if let Some(f) = caps.formats.iter().find(|f| want.contains(f)) {
            config.format = *f;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);
        Ok(Self::build(Some(surface), device, queue, config))
    }

    // Offscreen device for tests and tools: renders only into the 640x480 frame.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn headless() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: SCREEN_W as u32,
            height: SCREEN_H as u32,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: Vec::new(),
            color_space: Default::default(),
        };
        Ok(Self::build(None, device, queue, config))
    }

    fn build(
        surface: Option<wgpu::Surface<'static>>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
    ) -> Self {
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
        let add = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let make = |blend: Option<wgpu::BlendState>,
                    mask: wgpu::ColorWrites,
                    test: bool,
                    write: bool,
                    fs: &str| {
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
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Float32],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: config.format, blend, write_mask: mask })],
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(write),
                    depth_compare: Some(if test { wgpu::CompareFunction::LessEqual } else { wgpu::CompareFunction::Always }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let mut pipes: Vec<_> = (0..8)
            .map(|i| {
                let blend = if i & 1 != 0 {
                    add
                } else {
                    wgpu::BlendState::ALPHA_BLENDING
                };
                make(
                    Some(blend),
                    wgpu::ColorWrites::ALL,
                    i & 2 != 0,
                    i & 4 != 0,
                    "fs",
                )
            })
            .collect();
        pipes.push(make(None, wgpu::ColorWrites::ALL, false, true, "fs_clear"));
        pipes.push(make(
            None,
            wgpu::ColorWrites::empty(),
            false,
            true,
            "fs_clear",
        ));
        // ADDRESSU/V = WRAP, MAG/MIN = LINEAR, no mip (05 §12.4).
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(BLIT.into()),
        });
        let blit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        // The original renders at 640x480; the window shows that image scaled (05 §12.4).
        let frame = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: SCREEN_W as u32,
                height: SCREEN_H as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let frame_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &tex_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &frame.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&blit_sampler),
                },
            ],
        });
        let mut g = Self {
            surface,
            device,
            queue,
            config,
            pipes,
            depth: None,
            depth_test: false,
            tex_layout,
            sampler,
            textures: Vec::new(),
            free: Vec::new(),
            viewport: FULLSCREEN,
            verts: Vec::new(),
            indices: Vec::new(),
            batches: Vec::new(),
            white: 0,
            frame,
            frame_bind,
            blit,
            log: None,
            log_fog: None,
            persistent: false,
        };
        g.white = g.create_texture(1, 1, &[255; 4]);
        g
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w > 0 && h > 0 && (w, h) != (self.config.width, self.config.height) {
            self.config.width = w;
            self.config.height = h;
            if let Some(s) = &self.surface {
                s.configure(&self.device, &self.config);
            }
        }
    }

    // Full-precision RGBA8 upload (03 §3: the reduced-colour option can be ignored).
    pub fn create_texture(&mut self, width: u32, height: u32, rgba: &[u8]) -> TextureId {
        self.create_texture_fmt(width, height, rgba, wgpu::TextureFormat::Rgba8Unorm)
    }

    // Post-present capture (12 §1, 0x44886F): copies the last presented 640x480 frame into a new surface texture.
    pub fn capture_screen(&mut self, label: &str) -> TextureId {
        let (w, h) = (SCREEN_W as u32, SCREEN_H as u32);
        let fmt = self.config.format;
        let blank = vec![0u8; (w * h * 4) as usize];
        let id = self.create_texture_fmt(w, h, &blank, fmt);
        if self.surface.is_some() {
            let mut enc = self.device.create_command_encoder(&Default::default());
            let dst = &self.textures[id].as_ref().unwrap().tex;
            enc.copy_texture_to_texture(
                self.frame.as_image_copy(),
                dst.as_image_copy(),
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit([enc.finish()]);
        }
        self.set_label(id, label, w, h);
        if let Some(log) = self.log.as_mut() {
            log.push(LogRec::Copy {
                src: "screen".into(),
                dst: label.into(),
                from: [0, 0],
                size: [w, h],
                to: [0, 0],
            });
        }
        id
    }

    fn create_texture_fmt(
        &mut self,
        width: u32,
        height: u32,
        rgba: &[u8],
        format: wgpu::TextureFormat,
    ) -> TextureId {
        let tex = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
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
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let t = Some(Texture {
            width,
            height,
            bind,
            tex,
            label: None,
        });
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

    // `name` = the image file, used as its draw-log label.
    // Standalone .jpg / .png images (03 §1).
    pub fn load_image(&mut self, name: &str, bytes: &[u8]) -> anyhow::Result<TextureId> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        let id = self.create_texture(img.width(), img.height(), &img);
        self.set_label(id, name, img.width(), img.height());
        Ok(id)
    }

    // Replaces a w×h RGBA region at (x, y); applied before the next submit.
    pub fn write_texture(&mut self, id: TextureId, x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) {
        let Some(t) = self.textures.get(id).and_then(Option::as_ref) else {
            return;
        };
        let (w, h) = (
            w.min(t.width.saturating_sub(x)),
            h.min(t.height.saturating_sub(y)),
        );
        if w == 0 || h == 0 {
            return;
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &t.tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    // Surface-to-surface copy of a pixel rectangle (submitted at once, before this frame's draws).
    pub fn copy_texture(
        &mut self,
        src: TextureId,
        from: [u32; 2],
        dst: TextureId,
        to: [u32; 2],
        size: [u32; 2],
    ) {
        let (Some(Some(a)), Some(Some(b))) = (self.textures.get(src), self.textures.get(dst))
        else {
            return;
        };
        let w = size[0]
            .min(a.width.saturating_sub(from[0]))
            .min(b.width.saturating_sub(to[0]));
        let h = size[1]
            .min(a.height.saturating_sub(from[1]))
            .min(b.height.saturating_sub(to[1]));
        if w == 0 || h == 0 {
            return;
        }
        if self.log.is_some() {
            let rec = LogRec::Copy {
                src: self.label_name(src),
                dst: self.label_name(dst),
                from,
                size: [w, h],
                to,
            };
            self.log.as_mut().unwrap().push(rec);
        }
        let mut enc = self.device.create_command_encoder(&Default::default());
        fn info(t: &wgpu::Texture, p: [u32; 2]) -> wgpu::TexelCopyTextureInfo<'_> {
            wgpu::TexelCopyTextureInfo {
                texture: t,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: p[0],
                    y: p[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            }
        }
        enc.copy_texture_to_texture(
            info(&a.tex, from),
            info(&b.tex, to),
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([enc.finish()]);
    }

    pub fn set_label(&mut self, id: TextureId, label: &str, w: u32, h: u32) {
        if let Some(Some(t)) = self.textures.get_mut(id) {
            t.label = Some(LogTex {
                label: label.into(),
                w,
                h,
            });
        }
    }

    fn label_name(&self, id: TextureId) -> String {
        match self.textures.get(id).and_then(Option::as_ref) {
            Some(Texture { label: Some(l), .. }) => l.label.to_string(),
            _ => format!("?{id:x}"),
        }
    }

    pub fn enable_log(&mut self) {
        self.log.get_or_insert_with(Vec::new);
    }

    pub fn logging(&self) -> bool {
        self.log.is_some()
    }

    pub fn take_log(&mut self) -> Vec<LogRec> {
        self.log.as_mut().map(std::mem::take).unwrap_or_default()
    }

    // Vertex fog parameters reported for the following draws (the factor itself is per vertex).
    pub fn set_log_fog(&mut self, fog: Option<LogFog>) {
        self.log_fog = fog;
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

    // Axis-aligned textured rectangle at `dst`, no depth write.
    pub fn sprite(
        &mut self,
        tex: TextureId,
        blend: Blend,
        dst: Rect,
        uv: [f32; 4],
        color: [u8; 4],
    ) {
        let Rect { x, y, w, h } = dst;
        let [u0, v0, u1, v1] = uv;
        self.quad(
            tex,
            blend,
            [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
            [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
            color,
        );
    }

    pub fn quad(
        &mut self,
        tex: TextureId,
        blend: Blend,
        corners: [[f32; 2]; 4],
        uv: [[f32; 2]; 4],
        color: [u8; 4],
    ) {
        let v = std::array::from_fn(|i| {
            Vertex::screen(corners[i][0], corners[i][1], 0.0, uv[i], color)
        });
        self.push(
            tex,
            self.key(blend, false, Kind::Draw),
            v,
            [0, 1, 2, 0, 2, 3],
        );
    }

    // D3D vertex order: v0 top-left, v1 top-right, v2 bottom-left, v3 bottom-right; triangles (v0 v1 v2), (v1 v2 v3).
    pub fn quad_strip(
        &mut self,
        tex: TextureId,
        blend: Blend,
        pos: [[f32; 2]; 4],
        uv: [[f32; 2]; 4],
        color: [u8; 4],
    ) {
        let v = std::array::from_fn(|i| Vertex::screen(pos[i][0], pos[i][1], 0.0, uv[i], color));
        self.strip(tex, blend, false, v);
    }

    pub fn strip(&mut self, tex: TextureId, blend: Blend, zwrite: bool, v: [Vertex; 4]) {
        self.push(
            tex,
            self.key(blend, zwrite, Kind::Draw),
            v,
            [0, 1, 2, 1, 2, 3],
        );
    }

    // ZFUNC: LESSEQUAL when on, ALWAYS when off (the D3D default outside the stage draw).
    pub fn set_depth_test(&mut self, on: bool) {
        self.depth_test = on;
    }

    // D3D Clear on the current viewport: colour (if given) and z = 1.
    pub fn clear_depth(&mut self, color: Option<[u8; 4]>) {
        let Rect { x, y, w, h } = self.viewport;
        let c = color.unwrap_or([0; 4]);
        let uv = [0.0, 0.0];
        let v = [[x, y], [x + w, y], [x, y + h], [x + w, y + h]]
            .map(|[px, py]| Vertex::screen(px, py, 1.0, uv, c));
        let kind = if color.is_some() {
            Kind::ClearColor
        } else {
            Kind::ClearDepth
        };
        if let Some(log) = &mut self.log {
            log.push(LogRec::Clear {
                vp: self.viewport,
                color,
            });
        }
        self.push(
            self.white,
            PipeKey {
                blend: Blend::Alpha,
                test: false,
                write: true,
                kind,
            },
            v,
            [0, 1, 2, 1, 2, 3],
        );
    }

    fn key(&self, blend: Blend, write: bool, kind: Kind) -> PipeKey {
        PipeKey {
            blend,
            test: self.depth_test,
            write,
            kind,
        }
    }

    // Appends to the last batch while texture, state and viewport match; any change starts a new batch (05 §9 step 5).
    fn push(&mut self, tex: TextureId, key: PipeKey, v: [Vertex; 4], idx: [u32; 6]) {
        if key.kind == Kind::Draw && self.log.is_some() {
            let v = if idx[3] == 0 {
                [v[0], v[1], v[3], v[2]]
            } else {
                v
            };
            let t = self.textures.get(tex).and_then(Option::as_ref);
            let rec = LogRec::Quad {
                tex: t.and_then(|t| t.label.clone()),
                id: tex,
                textured: tex != self.white,
                blend: key.blend,
                test: key.test,
                write: key.write,
                vp: self.viewport,
                fog: self.log_fog,
                v,
            };
            self.log.as_mut().unwrap().push(rec);
        }
        let base = self.verts.len() as u32;
        self.verts.extend(v);
        let start = self.indices.len() as u32;
        self.indices.extend(idx.map(|i| base + i));
        match self.batches.last_mut() {
            Some(b) if b.tex == tex && b.key == key && b.clip == self.viewport => b.end += 6,
            _ => self.batches.push(Batch {
                tex,
                key,
                clip: self.viewport,
                start,
                end: start + 6,
            }),
        }
    }

    pub fn white(&self) -> TextureId {
        self.white
    }

    // Untextured rectangle in D3D screen coordinates (pixel centres on integers).
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
        let (x, y) = (x + 0.5, y + 0.5);
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        self.quad(
            self.white,
            Blend::Alpha,
            [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
            uv,
            color,
        );
    }

    pub fn solid_quad(&mut self, blend: Blend, corners: [[f32; 2]; 4], color: [u8; 4]) {
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        self.quad(self.white, blend, corners, uv, color);
    }

    // Full-surface copy to the screen (logged as a K record).
    pub fn image(&mut self, tex: TextureId, x: f32, y: f32) {
        let t = self.texture(tex);
        let (w, h) = (t.width as f32, t.height as f32);
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let log = self.log.take();
        if let Some(mut log) = log {
            log.push(LogRec::Copy {
                src: self.label_name(tex),
                dst: "screen".into(),
                from: [0, 0],
                size: [w as u32, h as u32],
                to: [x as u32, y as u32],
            });
            self.quad(
                tex,
                Blend::Alpha,
                [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
                uv,
                [255; 4],
            );
            self.log = Some(log);
            return;
        }
        self.quad(
            tex,
            Blend::Alpha,
            [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
            uv,
            [255; 4],
        );
    }

    fn depth_view(&mut self, w: u32, h: u32) -> wgpu::TextureView {
        if let Some((dw, dh, v)) = &self.depth
            && (*dw, *dh) == (w, h)
        {
            return v.clone();
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let v = tex.create_view(&Default::default());
        self.depth = Some((w, h, v.clone()));
        v
    }

    fn encode(
        &self,
        enc: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) {
        let buffers = (!self.verts.is_empty()).then(|| {
            let vb = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&self.verts),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let ib = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
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
                ops: wgpu::Operations {
                    load: if self.persistent {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(if self.persistent {
                    wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }
                } else {
                    wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        if let Some((vb, ib)) = &buffers {
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            for b in &self.batches {
                let x0 = b.clip.x.round().clamp(0.0, SCREEN_W) as u32;
                let y0 = b.clip.y.round().clamp(0.0, SCREEN_H) as u32;
                let x1 = (b.clip.x + b.clip.w).round().clamp(0.0, SCREEN_W) as u32;
                let y1 = (b.clip.y + b.clip.h).round().clamp(0.0, SCREEN_H) as u32;
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
                pass.set_pipeline(&self.pipes[b.key.index()]);
                pass.set_bind_group(0, &self.texture(b.tex).bind, &[]);
                pass.draw_indexed(b.start..b.end, 0, 0..1);
            }
        }
    }

    pub fn present(&mut self) {
        let Some(surface) = &self.surface else {
            self.clear_batches();
            return;
        };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => {
                surface.configure(&self.device, &self.config);
                self.clear_batches();
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        self.render_frame(&mut enc);
        self.blit(&mut enc, &view, self.config.width, self.config.height);
        self.queue.submit([enc.finish()]);
        self.queue.present(frame);
        self.clear_batches();
    }

    // Letterboxed, linearly scaled copy of the 640x480 frame into a w×h target.
    fn blit(&self, enc: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, w: u32, h: u32) {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        let (w, h) = (w as f32, h as f32);
        let scale = (w / SCREEN_W).min(h / SCREEN_H);
        let (vw, vh) = ((SCREEN_W * scale).round(), (SCREEN_H * scale).round());
        pass.set_viewport(
            ((w - vw) / 2.0).floor(),
            ((h - vh) / 2.0).floor(),
            vw,
            vh,
            0.0,
            1.0,
        );
        pass.set_pipeline(&self.blit);
        pass.set_bind_group(0, &self.frame_bind, &[]);
        pass.draw(0..3, 0..1);
    }

    // Renders the pending batches into the 640x480 frame without presenting (tools).
    pub fn flush(&mut self) {
        let mut enc = self.device.create_command_encoder(&Default::default());
        self.render_frame(&mut enc);
        self.queue.submit([enc.finish()]);
        self.clear_batches();
    }

    // Untextured triangle fan (vertex 0 = centre), z-write off; logged as one `P fan` record.
    pub fn fan(&mut self, blend: Blend, v: &[Vertex]) {
        if v.len() < 3 {
            return;
        }
        let log = self.log.take();
        let key = self.key(blend, false, Kind::Draw);
        for i in 1..v.len() - 1 {
            self.push(
                self.white,
                key,
                [v[0], v[i], v[i + 1], v[i + 1]],
                [0, 1, 2, 0, 1, 2],
            );
        }
        self.log = log;
        if let Some(log) = self.log.as_mut() {
            log.push(LogRec::Fan {
                blend,
                vp: self.viewport,
                v: v.to_vec(),
            });
        }
    }

    // Raw draw of a pre-built quad (v0 TL, v1 TR, v2 BL, v3 BR) with explicit depth state (draw-log renderer).
    pub fn draw_raw(
        &mut self,
        tex: Option<TextureId>,
        blend: Blend,
        test: bool,
        write: bool,
        v: [Vertex; 4],
    ) {
        let tex = tex.unwrap_or(self.white);
        let key = PipeKey {
            blend,
            test,
            write,
            kind: Kind::Draw,
        };
        self.push(tex, key, v, [0, 1, 2, 1, 2, 3]);
    }

    fn render_frame(&mut self, enc: &mut wgpu::CommandEncoder) {
        let (w, h) = (SCREEN_W as u32, SCREEN_H as u32);
        let depth = self.depth_view(w, h);
        let view = self.frame.create_view(&Default::default());
        self.encode(enc, &view, &depth);
    }

    // Renders the pending batches at 640x480 into memory, optionally scaled to `size` like the window (RGBA).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture(&mut self, size: Option<(u32, u32)>) -> image::RgbaImage {
        let mut enc = self.device.create_command_encoder(&Default::default());
        self.render_frame(&mut enc);
        let scaled = size.map(|(w, h)| {
            let t = self.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            self.blit(&mut enc, &t.create_view(&Default::default()), w, h);
            t
        });
        let src = scaled.as_ref().unwrap_or(&self.frame);
        let (w, h) = (src.width(), src.height());
        let row = (w * 4).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_texture_to_buffer(
            src.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let data = buf.slice(..).get_mapped_range().unwrap().to_vec();
        let mut px: Vec<u8> = data
            .chunks_exact(row as usize)
            .flat_map(|r| &r[..(w * 4) as usize])
            .copied()
            .collect();
        if matches!(
            self.config.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            px.chunks_exact_mut(4).for_each(|p| p.swap(0, 2));
        }
        self.clear_batches();
        image::RgbaImage::from_raw(w, h, px).unwrap()
    }

    fn clear_batches(&mut self) {
        if let Some(l) = &mut self.log {
            l.clear();
        }
        self.log_fog = None;
        self.viewport = FULLSCREEN;
        self.depth_test = false;
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
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(a: Vec3) -> Vec3 {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    pub fn scale(x: f32, y: f32, z: f32) -> Self {
        Mat4([
            [x, 0.0, 0.0, 0.0],
            [0.0, y, 0.0, 0.0],
            [0.0, 0.0, z, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    pub fn rotation_x(a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Mat4([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, c, s, 0.0],
            [0.0, -s, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    pub fn rotation_y(a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Mat4([
            [c, 0.0, -s, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [s, 0.0, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    pub fn rotation_z(a: f32) -> Self {
        let (s, c) = a.sin_cos();
        Mat4([
            [c, s, 0.0, 0.0],
            [-s, c, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    // D3DXMatrixLookAtLH / D3DXMatrixPerspectiveFovLH equivalents (left-handed, row vectors).
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
        Mat4([
            [xs, 0.0, 0.0, 0.0],
            [0.0, ys, 0.0, 0.0],
            [0.0, 0.0, q, 1.0],
            [0.0, 0.0, -near * q, 0.0],
        ])
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

// View and projection of one camera: Camera_Set2D (05 §9) or the stage camera (06 §10.1).
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub view: Mat4,
    pub proj: Mat4,
}

impl Camera {
    // SPEC-GAP: FOV π/6 / near 100 of `default_2d` have no spec section; only the gfx tests use it.
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

    // Camera_Set2D 0x40B5A0 (05 §9): fovY π/10, near 1, far 10000, up (0, −1, 0); world (x, y, 0) maps to viewport pixel (x, y).
    pub fn set_2d(vp: Rect) -> Self {
        let fov = std::f32::consts::PI / 10.0;
        let (cx, cy) = (vp.w / 2.0, vp.h / 2.0);
        Self {
            view: Mat4::look_at_lh(
                [cx, cy, cy / (fov / 2.0).tan()],
                [cx, cy, 0.0],
                [0.0, -1.0, 0.0],
            ),
            proj: Mat4::perspective_fov_lh(fov, vp.w / vp.h, 1.0, 10000.0),
        }
    }

    pub fn view_proj(&self) -> Mat4 {
        self.view.mul(&self.proj)
    }

    // World point → (screen x, screen y, depth 0..1) inside viewport `vp`.
    // D3DXVec3Project with world = identity (05 §9 modes 4/5).
    pub fn project(&self, p: Vec3, vp: Rect) -> Vec3 {
        let c = self.view_proj().transform(p);
        let (x, y, z) = (c[0] / c[3], c[1] / c[3], c[2] / c[3]);
        [
            vp.x + (x + 1.0) * vp.w / 2.0,
            vp.y + (1.0 - y) * vp.h / 2.0,
            z,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_camera_maps_z0_to_pixels() {
        let cam = Camera::default_2d();
        for (x, y) in [
            (0.0, 0.0),
            (320.0, 240.0),
            (640.0, 480.0),
            (32.0, 16.0),
            (416.0, 464.0),
            (100.5, 377.25),
        ] {
            let s = cam.project([x, -y, 0.0], FULLSCREEN);
            assert!(
                (s[0] - x).abs() < 1e-3 && (s[1] - y).abs() < 1e-3,
                "({x},{y}) -> {s:?}"
            );
            assert!(s[2] > 0.0 && s[2] < 1.0);
        }
    }

    #[test]
    fn set_2d_maps_to_viewport_pixels() {
        let cam = Camera::set_2d(PLAYFIELD);
        for (x, y) in [(0.0, 0.0), (192.0, 224.0), (384.0, 448.0), (10.0, 300.0)] {
            let s = cam.project([x, y, 0.0], PLAYFIELD);
            assert!(
                (s[0] - x - 32.0).abs() < 1e-2 && (s[1] - y - 16.0).abs() < 1e-2,
                "({x},{y}) -> {s:?}"
            );
        }
    }
}
