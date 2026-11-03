//! wgpu renderer standing in for the D3D8 device state the original sets up (05 §9, §12.4–§12.5; 06 §10).
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;
use wgpu::util::DeviceExt;

pub use th08_core::drawlist::{
    Blend, DrawCmd, FULLSCREEN, LogFog, LogRec, LogTex, PLAYFIELD, Recorder, Rect, ResCmd,
    SCREEN_H, SCREEN_W, TexId, TexInfo, Vertex, clip_in_viewport,
};

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

pub struct Texture {
    pub width: u32,
    pub height: u32,
    bind: wgpu::BindGroup,
    tex: wgpu::Texture,
    label: Option<LogTex>,
}

// Consecutive quads with equal texture, state and viewport, drawn in one call (05 §9 EmitQuad step 5).
struct Batch {
    tex: TexId,
    key: PipeKey,
    clip: Rect,
    start: u32,
    end: u32,
}

// Port-only GPU state: device, pipelines, texture table, pending batches and the 640×480 frame target.
pub struct Gfx {
    pub surface: Option<wgpu::Surface<'static>>,
    // Window devices: makes the surface again after a lifecycle detach (ARCH_D §5); None = headless.
    instance: Option<wgpu::Instance>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipes: Vec<wgpu::RenderPipeline>,
    depth: Option<(u32, u32, wgpu::TextureView)>,
    depth_test: bool,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    // Executor side of the core `ResTable`: slot `TexId.0` holds that id's texture.
    textures: Vec<Option<Texture>>,
    viewport: Rect,
    verts: Vec<Vertex>,
    indices: Vec<u32>,
    batches: Vec<Batch>,
    white: TexId,
    frame: wgpu::Texture,
    frame_bind: wgpu::BindGroup,
    blit: wgpu::RenderPipeline,
    // Filtered sub-rect copy of the frame into an Rgba8Unorm texture (texture capture, 12 §3).
    crop: wgpu::RenderPipeline,
    log: Option<Vec<LogRec>>,
    log_fog: Option<LogFog>,
    pub persistent: bool,
    // `ResCmd::Text` rasteriser (10 §6), made with `font` at the first text command.
    text: Option<crate::text::Raster>,
    font: Option<String>,
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

const CROP: &str = r#"
struct VsOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f };
@vertex fn vs(@location(0) pos: vec2f, @location(1) uv: vec2f) -> VsOut {
    var o: VsOut;
    o.pos = vec4f(pos, 0.0, 1.0);
    o.uv = uv;
    return o;
}
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment fn fs(i: VsOut) -> @location(0) vec4f { return textureSample(t, s, i.uv); }
"#;

impl Gfx {
    // `vsync` = port display setting; logic pacing is separate (app.rs).
    pub async fn new(window: Arc<winit::window::Window>, vsync: bool) -> anyhow::Result<Self> {
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
        config.present_mode = if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        surface.configure(&device, &config);
        let mut g = Self::build(Some(surface), device, queue, config);
        g.instance = Some(instance);
        Ok(g)
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
        let crop_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(CROP.into()),
        });
        let crop = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &crop_shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &crop_shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
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
            instance: None,
            device,
            queue,
            config,
            pipes,
            depth: None,
            depth_test: false,
            tex_layout,
            sampler,
            textures: Vec::new(),
            viewport: FULLSCREEN,
            verts: Vec::new(),
            indices: Vec::new(),
            batches: Vec::new(),
            white: TexId::WHITE,
            frame,
            frame_bind,
            blit,
            crop,
            log: None,
            log_fog: None,
            persistent: false,
            text: None,
            font: None,
        };
        // A fresh recorder's white texture (slot 0), so a Gfx without a core recorder (launcher, tools) has it too.
        g.submit(&mut Recorder::new());
        g
    }

    // Port display setting `vsync`, changed by the launcher screen after the surface exists.
    pub fn set_vsync(&mut self, vsync: bool) {
        self.config.present_mode = if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        self.reconfigure();
    }

    // App `Suspended` (ARCH_D §5): the native window goes away; the device and textures stay.
    pub fn detach(&mut self) {
        self.surface = None;
    }

    // App `Resumed` again: a surface for the (new) native window on the same device, same format and present mode.
    pub fn attach(&mut self, window: Arc<winit::window::Window>) -> anyhow::Result<()> {
        let Some(instance) = &self.instance else {
            anyhow::bail!("headless device has no window surface");
        };
        let size = window.inner_size();
        let surface = instance.create_surface(window)?;
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        surface.configure(&self.device, &self.config);
        self.surface = Some(surface);
        Ok(())
    }

    pub fn reconfigure(&self) {
        if let Some(s) = &self.surface {
            s.configure(&self.device, &self.config);
        }
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn surface_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
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

    // Post-present capture (12 §1, 0x44886F): copies the last presented 640x480 frame into a new surface texture.
    fn do_capture_screen(&mut self, id: TexId, label: &str) {
        let (w, h) = (SCREEN_W as u32, SCREEN_H as u32);
        let fmt = self.config.format;
        let blank = vec![0u8; (w * h * 4) as usize];
        self.create_texture_fmt(id, w, h, &blank, fmt);
        if self.surface.is_some() {
            let mut enc = self.device.create_command_encoder(&Default::default());
            let dst = &self.tex(id).tex;
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
        self.do_set_label(id, label, w, h);
        if let Some(log) = self.log.as_mut() {
            log.push(LogRec::Copy {
                src: "screen".into(),
                dst: label.into(),
                from: [0, 0],
                size: [w, h],
                to: [0, 0],
            });
        }
    }

    // Anm_CaptureToTexture 0x466F20 (12 §3): the last presented frame's rect `from`/`size` is filtered into the
    // texture rect `to`/`to_size` (D3DX surface load, default filter; the port samples linearly).
    fn do_capture_rect(
        &mut self,
        dst: TexId,
        from: [u32; 2],
        size: [u32; 2],
        to: [u32; 2],
        to_size: [u32; 2],
    ) {
        let Some(Some(t)) = self.textures.get(dst.index()) else {
            return;
        };
        let (tw, th) = (t.width, t.height);
        let w = to_size[0].min(tw.saturating_sub(to[0]));
        let h = to_size[1].min(th.saturating_sub(to[1]));
        if w == 0 || h == 0 || size[0] == 0 || size[1] == 0 {
            return;
        }
        // Draw log: no K for a size-mismatched (filtered) capture (data/drawlog/FORMAT.md K).
        if self.log.is_some() && size == to_size {
            let rec = LogRec::Copy {
                src: "screen".into(),
                dst: self.label_name(dst),
                from,
                size,
                to,
            };
            self.log.as_mut().unwrap().push(rec);
        }
        if self.surface.is_none() || t.tex.format() != wgpu::TextureFormat::Rgba8Unorm {
            return;
        }
        let (u0, v0) = (from[0] as f32 / SCREEN_W, from[1] as f32 / SCREEN_H);
        let (u1, v1) = (
            (from[0] + size[0]) as f32 / SCREEN_W,
            (from[1] + size[1]) as f32 / SCREEN_H,
        );
        let quad: [f32; 16] = [
            -1.0, 1.0, u0, v0, 1.0, 1.0, u1, v0, -1.0, -1.0, u0, v1, 1.0, -1.0, u1, v1,
        ];
        let vb = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&quad),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let view = t.tex.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_viewport(to[0] as f32, to[1] as f32, w as f32, h as f32, 0.0, 1.0);
            pass.set_pipeline(&self.crop);
            pass.set_bind_group(0, &self.frame_bind, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.draw(0..4, 0..1);
        }
        self.queue.submit([enc.finish()]);
    }

    // Full-precision RGBA8 upload (03 §3: the reduced-colour option can be ignored) or a capture target, at core id `id`.
    fn create_texture_fmt(
        &mut self,
        id: TexId,
        width: u32,
        height: u32,
        rgba: &[u8],
        format: wgpu::TextureFormat,
    ) {
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
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
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
        if self.textures.len() <= id.index() {
            self.textures.resize_with(id.index() + 1, || None);
        }
        self.textures[id.index()] = t;
    }

    // The core table already skipped white and double frees.
    fn do_free_texture(&mut self, id: TexId) {
        if let Some(t) = self.textures.get_mut(id.index()) {
            *t = None;
        }
    }

    // Replaces a w×h RGBA region at (x, y); applied before the next submit.
    fn do_write_texture(&mut self, id: TexId, x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) {
        let Some(t) = self.textures.get(id.index()).and_then(Option::as_ref) else {
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
    fn do_copy_texture(
        &mut self,
        src: TexId,
        from: [u32; 2],
        dst: TexId,
        to: [u32; 2],
        size: [u32; 2],
    ) {
        let (Some(Some(a)), Some(Some(b))) = (
            self.textures.get(src.index()),
            self.textures.get(dst.index()),
        ) else {
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
        // No K record: the original's texture-to-texture copy (D3DXLoadSurfaceFromSurface) makes no device call
        // (specs/data/drawlog/FORMAT.md §3 K).
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

    fn do_set_label(&mut self, id: TexId, label: &str, w: u32, h: u32) {
        if let Some(Some(t)) = self.textures.get_mut(id.index()) {
            t.label = Some(LogTex {
                label: label.into(),
                w,
                h,
            });
        }
    }

    fn label_name(&self, id: TexId) -> String {
        match self.textures.get(id.index()).and_then(Option::as_ref) {
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

    pub fn take_log(&mut self, rec: &mut Recorder) -> Vec<LogRec> {
        self.submit(rec);
        self.log.as_mut().map(std::mem::take).unwrap_or_default()
    }

    // Vertex fog parameters reported for the following draws (the factor itself is per vertex).
    fn do_set_log_fog(&mut self, fog: Option<LogFog>) {
        self.log_fog = fog;
    }

    fn tex(&self, id: TexId) -> &Texture {
        self.textures[id.index()].as_ref().expect("freed texture")
    }

    // Subsequent draws are clipped to `r` (screen coords); 2D vertices stay in screen space like D3D pretransformed ones.
    fn do_set_viewport(&mut self, r: Rect) {
        self.viewport = r;
    }

    fn do_clear_viewport(&mut self, color: [u8; 4]) {
        let r = self.viewport;
        self.do_rect(r.x, r.y, r.w, r.h, color);
    }

    // Axis-aligned textured rectangle at `dst`, no depth write.
    fn do_sprite(&mut self, tex: TexId, blend: Blend, dst: Rect, uv: [f32; 4], color: [u8; 4]) {
        let Rect { x, y, w, h } = dst;
        let [u0, v0, u1, v1] = uv;
        self.do_quad(
            tex,
            blend,
            [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
            [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
            color,
        );
    }

    fn do_quad(
        &mut self,
        tex: TexId,
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
    fn do_quad_strip(
        &mut self,
        tex: TexId,
        blend: Blend,
        pos: [[f32; 2]; 4],
        uv: [[f32; 2]; 4],
        color: [u8; 4],
    ) {
        let v = std::array::from_fn(|i| Vertex::screen(pos[i][0], pos[i][1], 0.0, uv[i], color));
        self.do_strip(tex, blend, false, v);
    }

    fn do_strip(&mut self, tex: TexId, blend: Blend, zwrite: bool, v: [Vertex; 4]) {
        self.push(
            tex,
            self.key(blend, zwrite, Kind::Draw),
            v,
            [0, 1, 2, 1, 2, 3],
        );
    }

    // ZFUNC: LESSEQUAL when on, ALWAYS when off (the D3D default outside the stage draw).
    fn do_set_depth_test(&mut self, on: bool) {
        self.depth_test = on;
    }

    // D3D Clear on the current viewport: colour (if given) and z = 1.
    fn do_clear_depth(&mut self, color: Option<[u8; 4]>) {
        self.clear(color, 0xff00_0000);
    }

    // Z-only D3D Clear passing colour `arg` (D3DCOLOR), which has no pixel effect (11 §3.1, 06 §3 step 5).
    fn do_clear_z(&mut self, arg: u32) {
        self.clear(None, arg);
    }

    fn clear(&mut self, color: Option<[u8; 4]>, arg: u32) {
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
                arg,
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
    fn push(&mut self, tex: TexId, key: PipeKey, v: [Vertex; 4], idx: [u32; 6]) {
        if key.kind == Kind::Draw && self.log.is_some() {
            let v = if idx[3] == 0 {
                [v[0], v[1], v[3], v[2]]
            } else {
                v
            };
            let t = self.textures.get(tex.index()).and_then(Option::as_ref);
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

    // Untextured rectangle in D3D screen coordinates (pixel centres on integers).
    fn do_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
        let (x, y) = (x + 0.5, y + 0.5);
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        self.do_quad(
            self.white,
            Blend::Alpha,
            [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
            uv,
            color,
        );
    }

    fn do_solid_quad(&mut self, blend: Blend, corners: [[f32; 2]; 4], color: [u8; 4]) {
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        self.do_quad(self.white, blend, corners, uv, color);
    }

    // Full-surface copy to the screen (logged as a K record).
    fn do_image(&mut self, tex: TexId, x: f32, y: f32) {
        let t = self.tex(tex);
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
            self.do_quad(
                tex,
                Blend::Alpha,
                [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
                uv,
                [255; 4],
            );
            self.log = Some(log);
            return;
        }
        self.do_quad(
            tex,
            Blend::Alpha,
            [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
            uv,
            [255; 4],
        );
    }

    // Copy of the (sx, sy, w, h) source rectangle to the screen at (dx, dy) (logged as a K record).
    #[allow(clippy::too_many_arguments)]
    fn do_image_rect(&mut self, tex: TexId, sx: u32, sy: u32, w: u32, h: u32, dx: f32, dy: f32) {
        let t = self.tex(tex);
        let (tw, th) = (t.width as f32, t.height as f32);
        let (u0, v0) = (sx as f32 / tw, sy as f32 / th);
        let (u1, v1) = ((sx + w) as f32 / tw, (sy + h) as f32 / th);
        let uv = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        if let Some(mut log) = self.log.take() {
            log.push(LogRec::Copy {
                src: self.label_name(tex),
                dst: "screen".into(),
                from: [sx, sy],
                size: [w, h],
                to: [dx as u32, dy as u32],
            });
            self.log = Some(log);
        }
        let log = self.log.take();
        let (w, h) = (w as f32, h as f32);
        self.do_quad(
            tex,
            Blend::Alpha,
            [[dx, dy], [dx + w, dy], [dx + w, dy + h], [dx, dy + h]],
            uv,
            [255; 4],
        );
        self.log = log;
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
                pass.set_bind_group(0, &self.tex(b.tex).bind, &[]);
                pass.draw_indexed(b.start..b.end, 0, 0..1);
            }
        }
    }

    fn do_present(&mut self) {
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
    pub fn flush(&mut self, rec: &mut Recorder) {
        self.submit(rec);
        let mut enc = self.device.create_command_encoder(&Default::default());
        self.render_frame(&mut enc);
        self.queue.submit([enc.finish()]);
        self.clear_batches();
    }

    // Untextured triangle fan (vertex 0 = centre), z-write off; logged as one `P fan` record.
    fn do_fan(&mut self, blend: Blend, v: &[Vertex]) {
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
    fn do_draw_raw(
        &mut self,
        tex: Option<TexId>,
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
    pub fn capture(&mut self, rec: &mut Recorder, size: Option<(u32, u32)>) -> image::RgbaImage {
        self.submit(rec);
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
        let img = self.read_texture(enc, src);
        self.clear_batches();
        img
    }

    // Submits `enc` plus a copy of `src` (surface format) and reads it back as RGBA.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read_texture(
        &self,
        mut enc: wgpu::CommandEncoder,
        src: &wgpu::Texture,
    ) -> image::RgbaImage {
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

// Executor: today's device code run from the recorded lists (REWRITE_PLAN step 8).
impl Gfx {
    // Runs everything recorded so far in call order: each `ResCmd` before the draw that followed it.
    pub fn submit(&mut self, rec: &mut Recorder) {
        let (draws, res) = rec.take();
        let mut res = res.into_iter().peekable();
        for (i, d) in draws.iter().enumerate() {
            while let Some((_, r)) = res.next_if(|(at, _)| *at <= i) {
                self.execute_res(&r);
            }
            self.execute_one(d);
        }
        for (_, r) in res {
            self.execute_res(&r);
        }
    }

    // Records the Present and executes the frame's lists (tools that own a `Recorder`).
    pub fn present(&mut self, rec: &mut Recorder) {
        rec.present();
        self.submit(rec);
    }

    pub fn execute(&mut self, cmds: &[DrawCmd]) {
        cmds.iter().for_each(|c| self.execute_one(c));
    }

    fn execute_one(&mut self, c: &DrawCmd) {
        match *c {
            DrawCmd::SetViewport(r) => self.do_set_viewport(r),
            DrawCmd::ClearViewport(color) => self.do_clear_viewport(color),
            DrawCmd::Sprite {
                tex,
                blend,
                dst,
                uv,
                color,
            } => self.do_sprite(tex, blend, dst, uv, color),
            DrawCmd::Quad {
                tex,
                blend,
                corners,
                uv,
                color,
            } => self.do_quad(tex, blend, corners, uv, color),
            DrawCmd::QuadStrip {
                tex,
                blend,
                pos,
                uv,
                color,
            } => self.do_quad_strip(tex, blend, pos, uv, color),
            DrawCmd::Strip {
                tex,
                blend,
                zwrite,
                v,
            } => self.do_strip(tex, blend, zwrite, v),
            DrawCmd::SetDepthTest(on) => self.do_set_depth_test(on),
            DrawCmd::ClearDepth(color) => self.do_clear_depth(color),
            DrawCmd::ClearZ(arg) => self.do_clear_z(arg),
            DrawCmd::Rect { x, y, w, h, color } => self.do_rect(x, y, w, h, color),
            DrawCmd::SolidQuad {
                blend,
                corners,
                color,
            } => self.do_solid_quad(blend, corners, color),
            DrawCmd::Image { tex, x, y } => self.do_image(tex, x, y),
            DrawCmd::ImageRect {
                tex,
                sx,
                sy,
                w,
                h,
                dx,
                dy,
            } => self.do_image_rect(tex, sx, sy, w, h, dx, dy),
            DrawCmd::Fan { blend, ref v } => self.do_fan(blend, v),
            DrawCmd::Raw {
                tex,
                blend,
                test,
                write,
                v,
            } => self.do_draw_raw(tex, blend, test, write, v),
            DrawCmd::SetLogFog(fog) => self.do_set_log_fog(fog),
            DrawCmd::Present => self.do_present(),
        }
    }

    pub fn execute_res(&mut self, c: &ResCmd) {
        match c {
            ResCmd::Create { id, w, h, rgba } => {
                self.create_texture_fmt(*id, *w, *h, rgba, wgpu::TextureFormat::Rgba8Unorm)
            }
            ResCmd::CaptureScreen { id, label } => self.do_capture_screen(*id, label),
            ResCmd::CaptureRect {
                dst,
                from,
                size,
                to,
                to_size,
            } => self.do_capture_rect(*dst, *from, *size, *to, *to_size),
            ResCmd::Free(id) => self.do_free_texture(*id),
            ResCmd::Write {
                id,
                x,
                y,
                w,
                h,
                rgba,
            } => self.do_write_texture(*id, *x, *y, *w, *h, rgba),
            ResCmd::Copy {
                src,
                from,
                dst,
                to,
                size,
            } => self.do_copy_texture(*src, *from, *dst, *to, *size),
            ResCmd::SetLabel { id, label, w, h } => self.do_set_label(*id, label, *w, *h),
            ResCmd::Text(t) => self.do_text(t),
        }
    }

    // Text_RenderToVm 0x4663B0 (10 §6): Shift-JIS decode, CPU raster, upload over the strip.
    fn do_text(&mut self, t: &th08_core::drawlist::TextCmd) {
        let font = self.font.as_deref();
        let raster = self
            .text
            .get_or_insert_with(|| crate::text::Raster::new(font));
        let rgba = raster.render_outline(
            &t.text.decode(),
            t.color,
            t.w as usize,
            t.size as usize,
            t.x,
            t.outline,
        );
        self.do_write_texture(t.dst, 0, t.y, t.w, t.size, &rgba);
    }
}

impl th08_core::drawlist::Renderer for Gfx {
    fn submit(&mut self, rec: &mut Recorder) {
        Gfx::submit(self, rec)
    }

    fn enable_log(&mut self) {
        Gfx::enable_log(self)
    }

    fn take_log(&mut self, rec: &mut Recorder) -> Vec<LogRec> {
        Gfx::take_log(self, rec)
    }

    fn set_font(&mut self, font: Option<&str>) {
        self.font = font.map(str::to_owned);
        self.text = None;
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
