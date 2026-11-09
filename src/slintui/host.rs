//! Slint platform on the game's wgpu device: the overlay is drawn into its own texture and composited premultiplied.
#[cfg(target_os = "android")]
use i_slint_renderer_femtovg::FemtoVGWGPURenderer;
#[cfg(not(target_os = "android"))]
use slint::platform::femtovg_renderer::FemtoVGWGPURenderer;
use slint::platform::{Platform, Renderer, WindowAdapter, WindowEvent};
use slint::{LogicalSize, PhysicalSize, PlatformError, Window};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;
use th08_render::gfx::OverlayTarget;
use web_time::Instant;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const COMPOSITE: &str = "
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
struct V { @builtin(position) p: vec4f, @location(0) uv: vec2f }
@vertex fn vs(@builtin(vertex_index) i: u32) -> V {
  let uv = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
  return V(vec4f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0), uv);
}
@fragment fn fs(v: V) -> @location(0) vec4f { return textureSample(t, s, v.uv); }
";

#[derive(Clone)]
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

pub struct Adapter {
    window: Window,
    renderer: FemtoVGWGPURenderer,
    size: Cell<PhysicalSize>,
    dirty: Cell<bool>,
}

impl WindowAdapter for Adapter {
    fn window(&self) -> &Window {
        &self.window
    }
    fn size(&self) -> PhysicalSize {
        self.size.get()
    }
    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }
    fn request_redraw(&self) {
        self.dirty.set(true);
    }
}

impl Adapter {
    pub fn dispatch(&self, ev: WindowEvent) {
        self.window.dispatch_event(ev);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.get()
    }

    pub fn animating(&self) -> bool {
        self.window.has_active_animations()
    }
}

struct Shared {
    gpu: RefCell<Option<Gpu>>,
    made: RefCell<Option<Rc<Adapter>>>,
    mock_now: Cell<Option<Duration>>,
    start: Instant,
}

thread_local! {
    static SHARED: Rc<Shared> = Rc::new(Shared {
        gpu: RefCell::new(None),
        made: RefCell::new(None),
        mock_now: Cell::new(None),
        start: Instant::now(),
    });
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}

struct Host(Rc<Shared>);

impl Platform for Host {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let gpu = self
            .0
            .gpu
            .borrow()
            .clone()
            .ok_or(PlatformError::NoPlatform)?;
        let renderer = FemtoVGWGPURenderer::new(gpu.instance, gpu.device, gpu.queue)?;
        let a = Rc::new_cyclic(|w: &std::rc::Weak<Adapter>| Adapter {
            window: Window::new(w.clone()),
            renderer,
            size: Cell::new(PhysicalSize::new(1, 1)),
            dirty: Cell::new(true),
        });
        *self.0.made.borrow_mut() = Some(a.clone());
        Ok(a)
    }

    fn duration_since_start(&self) -> Duration {
        self.0
            .mock_now
            .get()
            .unwrap_or_else(|| self.0.start.elapsed())
    }
}

// Sets the GPU that the next created Slint window draws with; the platform is installed on first use, so a run that
// never opens a Slint page does no Slint work.
pub fn use_gpu(gpu: Gpu) -> Result<(), PlatformError> {
    SHARED.with(|s| *s.gpu.borrow_mut() = Some(gpu));
    if !INSTALLED.replace(true) {
        slint::platform::set_platform(Box::new(Host(SHARED.with(Rc::clone))))
            .map_err(|e| PlatformError::Other(e.to_string()))?;
    }
    Ok(())
}

// The adapter of the window created last.
pub fn take_adapter() -> Option<Rc<Adapter>> {
    SHARED.with(|s| s.made.borrow_mut().take())
}

// Tests: moves Slint's clock on by `by` and keeps it there; Slint requires it to never go back.
pub fn advance_mock_time(by: Duration) {
    SHARED.with(|s| {
        let now = s.mock_now.get().unwrap_or_else(|| s.start.elapsed());
        s.mock_now.set(Some(now + by));
    });
}

// Runs Slint's timers and animations; true while something still moves.
pub fn tick(a: &Adapter) -> bool {
    slint::platform::update_timers_and_animations();
    a.animating()
}

// How long Slint can sleep, None when no timer is pending.
pub fn next_timer() -> Option<Duration> {
    slint::platform::duration_until_next_timer_update()
}

thread_local! {
    static LAST_PASS: Cell<Option<Instant>> = const { Cell::new(None) };
}

pub fn pass_begin() -> Instant {
    Instant::now()
}

// Logs a menu pass that took long, or a late one during continuous redraws.
pub fn pass_end(what: &str, begin: Instant) {
    let took = begin.elapsed().as_micros();
    let gap = LAST_PASS
        .replace(Some(begin))
        .map_or(0, |last| begin.duration_since(last).as_micros());
    if took > 12_000 || (25_000..1_000_000).contains(&gap) {
        log::info!("slintui pass: {what} took {took} us, gap {gap} us");
    }
}

// CPU time of the texture redraws and composites since the last log line.
#[derive(Default)]
struct FrameLog {
    renders: u32,
    composites: u32,
    render_us: u64,
    max_render_us: u64,
    composite_us: u64,
}

impl FrameLog {
    fn maybe_print(&mut self) {
        if self.renders >= 60 {
            log::info!(
                "slintui frame: {} renders avg {} us max {} us, {} composites avg {} us",
                self.renders,
                self.render_us / u64::from(self.renders),
                self.max_render_us,
                self.composites,
                self.composite_us / u64::from(self.composites.max(1)),
            );
            *self = Self::default();
        }
    }
}

struct Target {
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
}

pub struct Surface {
    adapter: Rc<Adapter>,
    gpu: Gpu,
    pipe: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    sampler: wgpu::Sampler,
    target: Option<Target>,
    scale: f32,
    log: FrameLog,
}

impl Surface {
    pub fn new(adapter: Rc<Adapter>, gpu: Gpu) -> Self {
        let sampler = gpu
            .device
            .create_sampler(&wgpu::SamplerDescriptor::default());
        Self {
            adapter,
            gpu,
            pipe: None,
            sampler,
            target: None,
            scale: 0.0,
            log: FrameLog::default(),
        }
    }

    pub fn adapter(&self) -> &Rc<Adapter> {
        &self.adapter
    }

    // The overlay's pixel size and px per logical point.
    #[expect(clippy::float_cmp, reason = "only a changed value resends the event")]
    pub fn resize(&mut self, w: u32, h: u32, scale: f32) {
        let a = &self.adapter;
        let same = a.size.get() == PhysicalSize::new(w, h) && self.scale == scale;
        if same {
            return;
        }
        self.scale = scale;
        a.size.set(PhysicalSize::new(w, h));
        a.dispatch(WindowEvent::ScaleFactorChanged {
            scale_factor: scale,
        });
        a.dispatch(WindowEvent::Resized {
            size: LogicalSize::new(w as f32 / scale, h as f32 / scale),
        });
        a.dirty.set(true);
    }

    fn pipeline(&mut self, format: wgpu::TextureFormat) {
        if self.pipe.as_ref().is_some_and(|(f, _)| *f == format) {
            return;
        }
        let d = &self.gpu.device;
        let m = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("slint composite"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE.into()),
        });
        let p = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("slint composite"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &m,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &m,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        self.pipe = Some((format, p));
        self.target = None;
    }

    fn make_target(&self, w: u32, h: u32) -> Option<Target> {
        let d = &self.gpu.device;
        let texture = d.create_texture(&wgpu::TextureDescriptor {
            label: Some("slint overlay"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("slint overlay"),
            layout: &self.pipe.as_ref()?.1.get_bind_group_layout(0),
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
        Some(Target { texture, bind })
    }

    // Redraws the texture when something changed and composites it over `t`.
    pub fn paint(&mut self, t: &OverlayTarget<'_>, enc: &mut wgpu::CommandEncoder) {
        self.pipeline(t.format);
        let (w, h) = t.size;
        if self
            .target
            .as_ref()
            .is_none_or(|x| x.texture.width() != w || x.texture.height() != h)
        {
            self.target = self.make_target(w, h);
            self.adapter.dirty.set(true);
        }
        let Some(target) = &self.target else {
            return;
        };
        let start = Instant::now();
        if self.adapter.dirty.replace(false) {
            if let Err(e) = self.adapter.renderer.render_to_texture(&target.texture) {
                log::error!("slintui: render failed: {e}");
            }
            let us = start.elapsed().as_micros() as u64;
            if us > 8_000 {
                log::info!("slintui render: {us} us");
            }
            self.log.renders += 1;
            self.log.render_us += us;
            self.log.max_render_us = self.log.max_render_us.max(us);
        }
        let composite = Instant::now();
        let Some((_, pipe)) = &self.pipe else {
            return;
        };
        {
            let mut p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("slint composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: t.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            p.set_pipeline(pipe);
            p.set_bind_group(0, &target.bind, &[]);
            p.draw(0..3, 0..1);
        }
        self.log.composites += 1;
        self.log.composite_us += composite.elapsed().as_micros() as u64;
        self.log.maybe_print();
    }
}
