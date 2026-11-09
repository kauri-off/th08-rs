//! Slint platform on the game's wgpu device: the overlay is drawn into its own texture and composited premultiplied.
#[cfg(target_os = "android")]
use i_slint_renderer_femtovg::FemtoVGWGPURenderer;
#[cfg(not(target_os = "android"))]
use slint::platform::femtovg_renderer::FemtoVGWGPURenderer;
use slint::platform::{Platform, Renderer, WindowAdapter, WindowEvent};
use slint::{LogicalSize, PhysicalSize, PlatformError, Window};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};
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
    scale: Cell<f32>,
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

    pub fn scale(&self) -> f32 {
        self.scale.get()
    }

    // The overlay's pixel size and px per logical point.
    #[expect(clippy::float_cmp, reason = "only a changed value resends the event")]
    pub fn resize(&self, w: u32, h: u32, scale: f32) {
        if self.size.get() == PhysicalSize::new(w, h) && self.scale.get() == scale {
            return;
        }
        self.scale.set(scale);
        self.size.set(PhysicalSize::new(w, h));
        self.dispatch(WindowEvent::ScaleFactorChanged {
            scale_factor: scale,
        });
        self.dispatch(WindowEvent::Resized {
            size: LogicalSize::new(w as f32 / scale, h as f32 / scale),
        });
        self.dirty.set(true);
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
        let began = Instant::now();
        let renderer = FemtoVGWGPURenderer::new(gpu.instance, gpu.device, gpu.queue)?;
        log::info!("slintui renderer: new {} us", began.elapsed().as_micros());
        let a = Rc::new_cyclic(|w: &std::rc::Weak<Adapter>| Adapter {
            window: Window::new(w.clone()),
            renderer,
            size: Cell::new(PhysicalSize::new(1, 1)),
            scale: Cell::new(0.0),
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

// Samples of one menu session, microseconds.
#[derive(Default)]
struct Session {
    renders: Vec<u32>,
    passes: Vec<u32>,
    presents: Vec<u32>,
    frames: Vec<u32>,
    gpu: Vec<u32>,
    gpu_note: Option<&'static str>,
}

const MAX_SAMPLES: usize = 100_000;
// Passes further apart than this are waits for input, not frames.
const FRAME_GAP_US: u128 = 250_000;

fn sample(v: &mut Vec<u32>, us: u128) {
    if v.len() < MAX_SAMPLES {
        v.push(u32::try_from(us).unwrap_or(u32::MAX));
    }
}

thread_local! {
    static LAST_PASS: Cell<Option<Instant>> = const { Cell::new(None) };
    static SESSION: RefCell<Session> = RefCell::default();
    static PASS_PARTS: Cell<(u128, u128)> = const { Cell::new((0, 0)) };
}

pub fn pass_begin() -> Instant {
    PASS_PARTS.set((0, 0));
    Instant::now()
}

// How long the pass's present took, render included: the rest of it is composite, surface acquire and submit.
pub fn note_present(begin: Instant) {
    let us = begin.elapsed().as_micros();
    PASS_PARTS.set((us, PASS_PARTS.get().1));
    SESSION.with_borrow_mut(|s| sample(&mut s.presents, us));
}

// Logs a menu pass that took long, or a late one during continuous redraws.
pub fn pass_end(what: &str, begin: Instant) {
    let took = begin.elapsed().as_micros();
    let gap = LAST_PASS
        .replace(Some(begin))
        .map_or(0, |last| begin.duration_since(last).as_micros());
    SESSION.with_borrow_mut(|s| {
        sample(&mut s.passes, took);
        if gap > 0 && gap < FRAME_GAP_US {
            sample(&mut s.frames, gap);
        }
    });
    if took > 12_000 || (25_000..1_000_000).contains(&gap) {
        let (present, render) = PASS_PARTS.get();
        log::info!(
            "slintui pass: {what} took {took} us (present {present} us, render {render} us), gap {gap} us"
        );
    }
}

fn percentiles(name: &str, v: &mut [u32]) {
    if v.is_empty() {
        log::info!("slintui session: {name} n=0");
        return;
    }
    v.sort_unstable();
    let at = |p: usize| v[(v.len() - 1) * p / 100];
    log::info!(
        "slintui session: {name} n={} p50={} p95={} p99={} max={} us",
        v.len(),
        at(50),
        at(95),
        at(99),
        v[v.len() - 1],
    );
}

// Logs the percentiles of the menu session that just ended and starts a new one.
pub fn session_end() {
    LAST_PASS.set(None);
    let mut s = SESSION.take();
    if s.passes.is_empty() {
        return;
    }
    percentiles("render", &mut s.renders);
    percentiles("pass", &mut s.passes);
    percentiles("present", &mut s.presents);
    percentiles("frame", &mut s.frames);
    match s.gpu_note {
        Some(why) => log::info!("slintui session: gpu unavailable ({why})"),
        None => percentiles("gpu", &mut s.gpu),
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

// Live wgpu objects: render pipelines, shader modules, textures; zero where wgpu keeps no report.
#[derive(Clone, Copy)]
struct Objects([usize; 3]);

impl Objects {
    fn since(self, before: Self) -> [isize; 3] {
        std::array::from_fn(|i| self.0[i].cast_signed() - before.0[i].cast_signed())
    }
}

fn objects(instance: &wgpu::Instance) -> Objects {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(r) = instance.generate_report() {
        return Objects([
            r.hub.render_pipelines.num_allocated,
            r.hub.shader_modules.num_allocated,
            r.hub.textures.num_allocated,
        ]);
    }
    let _ = instance;
    Objects([0; 3])
}

const NO_CLOCK: &str = "the adapter has no TIMESTAMP_QUERY";

// GPU time of a render: two empty timestamped passes submitted around Slint's own submit. One sample at a time; a
// render that finds the last one unread goes unmeasured.
struct GpuClock {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    view: wgpu::TextureView,
    period: f32,
    state: Arc<AtomicU8>,
}

const IDLE: u8 = 0;
const MAPPING: u8 = 1;
const MAPPED: u8 = 2;

impl GpuClock {
    fn new(gpu: &Gpu) -> Option<Self> {
        if !gpu
            .device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            return None;
        }
        let d = &gpu.device;
        let set = d.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("slint gpu clock"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let buffer = |usage| {
            d.create_buffer(&wgpu::BufferDescriptor {
                label: Some("slint gpu clock"),
                size: 16,
                usage,
                mapped_at_creation: false,
            })
        };
        let resolve = buffer(wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC);
        let read = buffer(wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ);
        let view = d
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("slint gpu clock"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        Some(Self {
            set,
            resolve,
            read,
            view,
            period: gpu.queue.get_timestamp_period(),
            state: Arc::new(AtomicU8::new(IDLE)),
        })
    }

    fn stamp(&self, enc: &mut wgpu::CommandEncoder, index: u32) {
        enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("slint gpu clock"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            timestamp_writes: Some(wgpu::RenderPassTimestampWrites {
                query_set: &self.set,
                beginning_of_pass_write_index: Some(index),
                end_of_pass_write_index: None,
            }),
            ..Default::default()
        });
    }

    fn begin(&mut self, gpu: &Gpu) -> bool {
        if self.state.load(Relaxed) != IDLE {
            return false;
        }
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.stamp(&mut enc, 0);
        gpu.queue.submit([enc.finish()]);
        true
    }

    fn end(&mut self, gpu: &Gpu) {
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.stamp(&mut enc, 1);
        enc.resolve_query_set(&self.set, 0..2, &self.resolve, 0);
        enc.copy_buffer_to_buffer(&self.resolve, 0, &self.read, 0, 16);
        gpu.queue.submit([enc.finish()]);
        self.state.store(MAPPING, Relaxed);
        let state = self.state.clone();
        self.read
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                state.store(if r.is_ok() { MAPPED } else { IDLE }, Relaxed);
            });
    }

    // Takes a finished sample without waiting for the GPU.
    fn collect(&mut self, device: &wgpu::Device) {
        if self.state.load(Relaxed) == IDLE {
            return;
        }
        let _ = device.poll(wgpu::PollType::Poll);
        if self.state.load(Relaxed) != MAPPED {
            return;
        }
        let ticks: Option<[u64; 2]> =
            self.read.slice(..).get_mapped_range().ok().map(|m| {
                [0, 8].map(|i| u64::from_le_bytes(m[i..i + 8].try_into().unwrap_or([0; 8])))
            });
        self.read.unmap();
        self.state.store(IDLE, Relaxed);
        if let Some([a, b]) = ticks
            && b > a
        {
            #[expect(clippy::cast_sign_loss, reason = "ticks and period are positive")]
            let us = ((b - a) as f64 * f64::from(self.period) / 1000.0) as u128;
            SESSION.with_borrow_mut(|s| sample(&mut s.gpu, us));
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
    log: FrameLog,
    screen: &'static str,
    tab: Option<usize>,
    shown: (&'static str, Option<usize>),
    renders: u32,
    clock: Option<GpuClock>,
}

impl Surface {
    pub fn new(adapter: Rc<Adapter>, gpu: Gpu) -> Self {
        let sampler = gpu
            .device
            .create_sampler(&wgpu::SamplerDescriptor::default());
        let clock = GpuClock::new(&gpu);
        if clock.is_none() {
            SESSION.with_borrow_mut(|s| s.gpu_note = Some(NO_CLOCK));
        }
        Self {
            adapter,
            gpu,
            pipe: None,
            sampler,
            target: None,
            log: FrameLog::default(),
            screen: "",
            tab: None,
            shown: ("", None),
            renders: 0,
            clock,
        }
    }

    // What the next render shows, for the log.
    pub fn label(&mut self, screen: &'static str, tab: Option<usize>) {
        self.screen = screen;
        self.tab = tab;
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
        if let Some(c) = &mut self.clock {
            c.collect(&self.gpu.device);
        }
        if self.adapter.dirty.replace(false) {
            let before = objects(&self.gpu.instance);
            let timed = self.clock.as_mut().is_some_and(|c| c.begin(&self.gpu));
            let start = Instant::now();
            if let Err(e) = self.adapter.renderer.render_to_texture(&target.texture) {
                log::error!("slintui: render failed: {e}");
            }
            let us = start.elapsed().as_micros() as u64;
            if timed && let Some(c) = &mut self.clock {
                c.end(&self.gpu);
            }
            self.renders += 1;
            let label = (self.screen, self.tab);
            let after = if self.shown.0 != label.0 {
                "view"
            } else if self.shown.1 != label.1 {
                "tab"
            } else {
                "change"
            };
            self.shown = label;
            if us > 8_000 || self.renders == 1 {
                let [p, s, t] = objects(&self.gpu.instance).since(before);
                log::info!(
                    "slintui render: {us} us screen={} tab={} after={after} nth={} pipelines=+{p} shaders=+{s} textures=+{t}",
                    label.0,
                    label.1.map_or(-1, usize::cast_signed),
                    self.renders,
                );
            }
            PASS_PARTS.set((PASS_PARTS.get().0, u128::from(us)));
            SESSION.with_borrow_mut(|s| sample(&mut s.renders, u128::from(us)));
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
