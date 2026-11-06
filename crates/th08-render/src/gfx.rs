//! wgpu renderer standing in for the D3D8 device state the original sets up (05 §9, §12.4–§12.5; 06 §10).
#![allow(clippy::disallowed_methods)]

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wgpu::util::DeviceExt;

pub use th08_core::drawlist::{
    Blend, DrawCmd, FULLSCREEN, LogFog, LogRec, LogTex, PLAYFIELD, Recorder, Rect, ResCmd,
    SCREEN_H, SCREEN_W, TexId, TexInfo, Vertex, clip_in_viewport,
};

// The 640×480 frame in texels (05 §12.4).
#[expect(clippy::cast_sign_loss, reason = "640.0 / 480.0 exactly")]
const FRAME: (u32, u32) = (SCREEN_W as u32, SCREEN_H as u32);

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
                usize::from(self.blend == Blend::Add)
                    | usize::from(self.test) << 1
                    | usize::from(self.write) << 2
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

// Port-only (ARCH_D_MOBILE §5): how slot `TexId.0` was made and changed since, replayed by `Gfx::rebuild` after a
// device loss. ANM entries arrive as `ResCmd::Create` pixels, .jpg / .png as `ResCmd::CreateImage` decoded here
// once; both keep the uploaded `rgba`.
#[cfg_attr(
    target_arch = "wasm32",
    expect(dead_code, reason = "no rebuild on the web")
)]
struct Provenance {
    w: u32,
    h: u32,
    format: wgpu::TextureFormat,
    // `ResCmd::Create` / decoded `CreateImage` pixels; None = screen capture target, which comes back black until captured again.
    rgba: Option<Arc<[u8]>>,
    // Writes (text rasters included) and copies into the slot, in order; an edit inside a later one's rect is dropped.
    edits: Vec<Edit>,
}

#[cfg_attr(
    target_arch = "wasm32",
    expect(dead_code, reason = "no rebuild on the web")
)]
enum Edit {
    Write {
        at: [u32; 2],
        size: [u32; 2],
        rgba: Vec<u8>,
    },
    Copy {
        src: TexId,
        from: [u32; 2],
        to: [u32; 2],
        size: [u32; 2],
    },
}

impl Edit {
    // Destination rect `[x, y, w, h]`.
    fn rect(&self) -> [u32; 4] {
        match *self {
            Edit::Write { at, size, .. } | Edit::Copy { to: at, size, .. } => {
                [at[0], at[1], size[0], size[1]]
            }
        }
    }
}

// `ResCmd::CreateImage` (03 §1): .jpg / .png → RGBA8; a file the decoder rejects, or whose size disagrees with the
// header core read, becomes a transparent `w` × `h` texture.
fn decode_image(bytes: &[u8], w: u32, h: u32) -> Arc<[u8]> {
    match image::load_from_memory(bytes) {
        Ok(img) if (img.width(), img.height()) == (w, h) => Arc::from(img.to_rgba8().into_raw()),
        Ok(img) => {
            log::warn!("image {}x{} != header {w}x{h}", img.width(), img.height());
            Arc::from(vec![0; 4 * w as usize * h as usize])
        }
        Err(e) => {
            log::warn!("image decode failed: {e}");
            Arc::from(vec![0; 4 * w as usize * h as usize])
        }
    }
}

// A presentation-pack image override (I18N G2, ARCH_B §1.7): decoded and scaled to the vanilla `w` × `h` (HD
// overrides keep the logical size and UVs); undecodable → transparent like `decode_image`.
fn decode_fitted(bytes: &[u8], w: u32, h: u32) -> Arc<[u8]> {
    match image::load_from_memory(bytes) {
        Ok(img) if (img.width(), img.height()) == (w, h) => Arc::from(img.to_rgba8().into_raw()),
        Ok(img) => {
            let fitted = image::imageops::resize(
                &img.to_rgba8(),
                w,
                h,
                image::imageops::FilterType::Triangle,
            );
            Arc::from(fitted.into_raw())
        }
        Err(e) => {
            log::warn!("pack image decode failed: {e}");
            Arc::from(vec![0; 4 * w as usize * h as usize])
        }
    }
}

// `ResCmd::CreateAnmOver` (I18N §2.4): the pack image at the THTX size `w` × `h` (same size, or the same aspect at
// a larger size → scaled; any other size is a partial sheet placed 1:1 at the top left), then per sprite rect: a
// rect with no visible pixel in the image keeps `base`. Undecodable → `base`.
fn decode_over(bytes: &[u8], base: &Arc<[u8]>, rects: &[[u32; 4]], w: u32, h: u32) -> Arc<[u8]> {
    let img = match image::load_from_memory(bytes) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            log::warn!("pack image decode failed: {e}");
            return base.clone();
        }
    };
    let (iw, ih) = img.dimensions();
    let over: Vec<u8> = if (iw, ih) == (w, h) {
        img.into_raw()
    } else if iw >= w && u64::from(iw) * u64::from(h) == u64::from(ih) * u64::from(w) {
        image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle).into_raw()
    } else {
        log::info!("pack image {iw}x{ih} placed 1:1 on a {w}x{h} texture");
        let mut v = vec![0; 4 * w as usize * h as usize];
        let (cw, ch) = (iw.min(w) as usize * 4, ih.min(h) as usize);
        for y in 0..ch {
            let src = y * iw as usize * 4;
            let dst = y * w as usize * 4;
            v[dst..dst + cw].copy_from_slice(&img.as_raw()[src..src + cw]);
        }
        v
    };
    if base.len() != over.len() {
        return Arc::from(over);
    }
    let row = w as usize * 4;
    let span = |r: &[u32; 4], y: u32| {
        let o = (r[1] + y) as usize * row + r[0] as usize * 4;
        o..o + r[2] as usize * 4
    };
    let mut out = over.clone();
    for r in rects {
        if r[0] + r[2] > w || r[1] + r[3] > h {
            continue;
        }
        let empty = (0..r[3]).all(|y| {
            over[span(r, y)]
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[3] == 0)
        });
        if empty {
            for y in 0..r[3] {
                out[span(r, y)].copy_from_slice(&base[span(r, y)]);
            }
        }
    }
    Arc::from(out)
}

// True when rect `inner` lies inside `outer` (both `[x, y, w, h]`).
fn rect_within(inner: [u32; 4], outer: [u32; 4]) -> bool {
    inner[0] >= outer[0]
        && inner[1] >= outer[1]
        && inner[0] + inner[2] <= outer[0] + outer[2]
        && inner[1] + inner[3] <= outer[1] + outer[3]
}

// Consecutive quads with equal texture, state and viewport, drawn in one call (05 §9 EmitQuad step 5).
struct Batch {
    tex: TexId,
    key: PipeKey,
    clip: Rect,
    start: u32,
    end: u32,
}

// Port-only UI drawn onto the window target after the blit, never into the 640×480 frame (ARCH_D §3.5): the
// frame texture is what `capture_screen` (12 §1) copies, so overlay pixels must stay out of it.
pub trait Overlay {
    fn paint(&mut self, gfx: OverlayTarget<'_>, enc: &mut wgpu::CommandEncoder);
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
    // After `Gfx::rebuild`: GPU objects made on the old device are dead and must be made again.
    fn device_rebuilt(&mut self) {}
}

// What an overlay draws with: the device and the window-sized target (surface or the scaled `--shot` texture).
#[derive(Clone, Copy)]
pub struct OverlayTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    pub size: (u32, u32),
    // Where `blit` puts the 640×480 frame in the target: `[x, y, w, h]` px (`Gfx::frame_rect`).
    pub frame: [f32; 4],
}

// Port-only: physical px of the window kept clear on each edge (Android display cutout, rounded corners, system
// bars, the `screen_margin` setting); zero on desktop / web. The frame is letterboxed inside the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SafeArea {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl SafeArea {
    // The safe rect `[x, y, w, h]` of a w×h target, at least 1×1 px.
    pub fn rect(&self, w: u32, h: u32) -> [f32; 4] {
        let x = self.left.min(w.saturating_sub(1));
        let y = self.top.min(h.saturating_sub(1));
        let sw = w.saturating_sub(x + self.right).max(1);
        let sh = h.saturating_sub(y + self.bottom).max(1);
        [x as f32, y as f32, sw as f32, sh as f32]
    }
}

// Window blit sampling: plain bilinear, sharp-bilinear (crisp texels, smooth seams) or nearest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScaleFilter {
    #[default]
    Smooth,
    Sharp,
    Nearest,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorFilter {
    #[default]
    Off,
    Protan,
    Deutan,
    Tritan,
    HighContrast,
}

impl ColorFilter {
    // Row-major colour matrix and offset: out = M·rgb + o.
    fn matrix(self) -> ([[f32; 3]; 3], [f32; 3]) {
        const I: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        // Machado et al. 2009 dichromat simulation (severity 1).
        let sim = match self {
            ColorFilter::Off => return (I, [0.0; 3]),
            ColorFilter::HighContrast => {
                // Contrast 1.5 about mid grey with saturation 1.3.
                let (k, sat) = (1.5f32, 1.3f32);
                let l = [0.2126f32, 0.7152, 0.0722];
                let mut m = [[0.0; 3]; 3];
                for (r, row) in m.iter_mut().enumerate() {
                    for (c, v) in row.iter_mut().enumerate() {
                        let id = if r == c { 1.0 } else { 0.0 };
                        *v = k * ((1.0 - sat) * l[c] + sat * id);
                    }
                }
                return (m, [0.5 * (1.0 - k); 3]);
            }
            ColorFilter::Protan => [
                [0.152_286, 1.052_583, -0.204_868],
                [0.114_503, 0.786_281, 0.099_216],
                [-0.003_882, -0.048_116, 1.051_998],
            ],
            ColorFilter::Deutan => [
                [0.367_322, 0.860_646, -0.227_968],
                [0.280_085, 0.672_501, 0.047_413],
                [-0.011_820, 0.042_940, 0.968_881],
            ],
            ColorFilter::Tritan => [
                [1.255_528, -0.076_749, -0.178_779],
                [-0.078_411, 0.930_809, 0.147_602],
                [0.004_733, 0.691_367, 0.303_900],
            ],
        };
        // Daltonisation: the colour error the viewer misses is moved into channels they see,
        // out = c + E·(c − S·c).
        let e = [[0.0, 0.0, 0.0], [0.7, 1.0, 0.0], [0.7, 0.0, 1.0]];
        let mut m = I;
        for (r, row) in m.iter_mut().enumerate() {
            for (c, v) in row.iter_mut().enumerate() {
                *v += (0..3).map(|k| e[r][k] * (I[k][c] - sim[k][c])).sum::<f32>();
            }
        }
        (m, [0.0; 3])
    }
}

// Port display options of the final window blit only; the frame texture, captures and the draw log never see them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlitOptions {
    pub filter: ScaleFilter,
    // Largest whole multiple of 640×480 that fits (when at least 1×).
    pub integer: bool,
    pub color: ColorFilter,
    // Output gamma: out = in^(1/gamma); 1 = neutral, above 1 brightens the darks.
    pub gamma: f32,
}

impl Default for BlitOptions {
    fn default() -> Self {
        Self {
            filter: ScaleFilter::Smooth,
            integer: false,
            color: ColorFilter::Off,
            gamma: 1.0,
        }
    }
}

impl BlitOptions {
    // Plain texture copy (today's blit shader) with only the sampler chosen.
    fn plain(&self) -> bool {
        self.filter != ScaleFilter::Sharp && self.color == ColorFilter::Off && self.gamma == 1.0
    }

    // `BLIT_FX` uniform: matrix rows, offset, (sharp, scale x, scale y, 1/gamma).
    fn uniform(&self, scale: [f32; 2]) -> [f32; 20] {
        let (m, o) = self.color.matrix();
        let sharp = if self.filter == ScaleFilter::Sharp {
            1.0
        } else {
            0.0
        };
        [
            m[0][0],
            m[0][1],
            m[0][2],
            0.0,
            m[1][0],
            m[1][1],
            m[1][2],
            0.0,
            m[2][0],
            m[2][1],
            m[2][2],
            0.0,
            o[0],
            o[1],
            o[2],
            0.0,
            sharp,
            scale[0],
            scale[1],
            1.0 / self.gamma,
        ]
    }
}

// Device-bound objects that do not depend on core state: made by `build` and again by `rebuild`.
struct Gpu {
    pipes: Vec<wgpu::RenderPipeline>,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    // The original renders at 640x480; the window shows that image scaled (05 §12.4).
    frame: wgpu::Texture,
    frame_bind: wgpu::BindGroup,
    blit: wgpu::RenderPipeline,
    // Window blit with `BlitOptions`: the frame through a nearest sampler, the filter / colour pipeline and its uniform.
    frame_near: wgpu::BindGroup,
    blit_fx: wgpu::RenderPipeline,
    fx_buf: wgpu::Buffer,
    fx_bind: wgpu::BindGroup,
    // Filtered sub-rect copy of the frame into an Rgba8Unorm texture (texture capture, 12 §3).
    crop: wgpu::RenderPipeline,
}

// Port-only GPU state: device, pipelines, texture table, pending batches and the 640×480 frame target.
pub struct Gfx {
    pub surface: Option<wgpu::Surface<'static>>,
    // Touch overlay (app), painted after `blit`.
    pub overlay: Option<Box<dyn Overlay>>,
    // Port labels and strips (app), painted after `overlay`.
    pub hud: Option<Box<dyn Overlay>>,
    // Read-only game-state overlay (hitbox viewer), painted after `overlay`.
    pub overlay_top: Option<Box<dyn Overlay>>,
    // Set by the app on every resize / resume; `blit` and the launcher keep out of it.
    pub safe: SafeArea,
    pub blit_options: BlitOptions,
    // Makes the surface again after a lifecycle detach and the device after a loss (ARCH_D §5).
    instance: wgpu::Instance,
    // No window: `attach` fails, `rebuild` asks for any adapter.
    headless: bool,
    // The native window `surface` was made for (None while detached): `rebuild` makes a new surface on it.
    window: Option<Arc<winit::window::Window>>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    gpu: Gpu,
    depth: Option<(u32, u32, wgpu::TextureView)>,
    depth_test: bool,
    // Executor side of the core `ResTable`: slot `TexId.0` holds that id's texture.
    textures: Vec<Option<Texture>>,
    // Same slots: what `rebuild` uploads again.
    prov: Vec<Option<Provenance>>,
    // Set by the device-lost callback of the current device; checked before each submit.
    lost: Arc<AtomicBool>,
    // Bumped by every `rebuild`; GPU objects made outside `Gfx` on an older generation are dead.
    generation: u32,
    // Debug-build hook TH08_DEVICE_LOSS=<n>: the device is destroyed at the n-th Present.
    loss_at: Option<u32>,
    presents: u32,
    viewport: Rect,
    verts: Vec<Vertex>,
    indices: Vec<u32>,
    batches: Vec<Batch>,
    white: TexId,
    log: Option<Vec<LogRec>>,
    log_fog: Option<LogFog>,
    pub persistent: bool,
    // `ResCmd::Text` rasteriser (10 §6), made with `font` at the first text command.
    text: Option<crate::text::Raster>,
    // Translations for `TextCmd` keys, pack font, overflow per site; empty = vanilla text (I18N §2.3).
    pack: crate::textdb::PackText,
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

// `BLIT` plus sharp-bilinear (texel interiors flat, a bilinear seam one output pixel wide), colour matrix and gamma.
const BLIT_FX: &str = r#"
struct VsOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f };
@vertex fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    let uv = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VsOut;
    o.pos = vec4f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    o.uv = uv;
    return o;
}
struct Fx { m0: vec4f, m1: vec4f, m2: vec4f, off: vec4f, p: vec4f };
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@group(1) @binding(0) var<uniform> fx: Fx;
@fragment fn fs(i: VsOut) -> @location(0) vec4f {
    var uv = i.uv;
    if fx.p.x > 0.5 {
        let size = vec2f(textureDimensions(t));
        let texel = uv * size;
        let sc = max(fx.p.yz, vec2f(1.0));
        let rr = 0.5 - 0.5 / sc;
        let d = fract(texel) - 0.5;
        uv = (floor(texel) + (d - clamp(d, -rr, rr)) * sc + 0.5) / size;
    }
    let c = textureSample(t, s, uv);
    let rgb = vec3f(dot(fx.m0.xyz, c.rgb), dot(fx.m1.xyz, c.rgb), dot(fx.m2.xyz, c.rgb)) + fx.off.xyz;
    return vec4f(pow(clamp(rgb, vec3f(0.0), vec3f(1.0)), vec3f(fx.p.w)), c.a);
}
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

fn shader(device: &wgpu::Device, src: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(src.into()),
    })
}

fn view(t: &wgpu::Texture) -> wgpu::TextureView {
    t.create_view(&wgpu::TextureViewDescriptor::default())
}

fn encoder(device: &wgpu::Device) -> wgpu::CommandEncoder {
    device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default())
}

fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

// Texture + sampler bind group of the draw / blit / crop shaders (`@group(0)`).
fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

// Same limits for the first device and a rebuilt one.
async fn request_device(adapter: &wgpu::Adapter) -> anyhow::Result<(wgpu::Device, wgpu::Queue)> {
    Ok(adapter
        .request_device(&wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                .using_resolution(adapter.limits()),
            ..Default::default()
        })
        .await?)
}

// Web: a lost WebGL context (GPU process restart, too many contexts) marks the device lost and wakes the event loop;
// WebGPU reports it through the device-lost callback.
#[cfg(target_arch = "wasm32")]
fn watch_context_loss(window: &Arc<winit::window::Window>, flag: Arc<AtomicBool>) {
    use wasm_bindgen::JsCast;
    use winit::platform::web::WindowExtWebSys;
    let Some(canvas) = window.canvas() else {
        return;
    };
    let w = window.clone();
    let cb = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        log::warn!("webglcontextlost");
        flag.store(true, Ordering::Release);
        w.request_redraw();
    });
    let _ =
        canvas.add_event_listener_with_callback("webglcontextlost", cb.as_ref().unchecked_ref());
    cb.forget();
}

// A fresh flag per device, so the old device's late callback cannot trigger a second rebuild.
fn watch_loss(device: &wgpu::Device) -> Arc<AtomicBool> {
    let lost = Arc::new(AtomicBool::new(false));
    let flag = lost.clone();
    device.set_device_lost_callback(move |reason, msg| {
        log::warn!("device lost ({reason:?}): {msg}");
        flag.store(true, Ordering::Release);
    });
    // wgpu's default handler panics; log instead, and rebuild the device after an out-of-memory error.
    let flag = lost.clone();
    device.on_uncaptured_error(Arc::new(move |e| {
        log::error!("gpu error: {e}");
        if matches!(e, wgpu::Error::OutOfMemory { .. }) {
            flag.store(true, Ordering::Release);
        }
    }));
    lost
}

impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let tex_layout = Self::tex_layout(device);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&tex_layout)],
            immediate_size: 0,
        });
        let pipes = Self::draw_pipes(device, &layout, format);
        // ADDRESSU/V = WRAP, MAG/MIN = LINEAR, no mip (05 §12.4).
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let blit = Self::blit_pipe(device, &layout, format, BLIT);
        let crop = Self::crop_pipe(device, &layout);
        let frame = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent(FRAME.0, FRAME.1),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
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
        let frame_bind = bind_group(device, &tex_layout, &view(&frame), &blit_sampler);
        let near_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let frame_near = bind_group(device, &tex_layout, &view(&frame), &near_sampler);
        let fx_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let fx_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let fx_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &fx_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: fx_buf.as_entire_binding(),
            }],
        });
        let fx_pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&tex_layout), Some(&fx_layout)],
            immediate_size: 0,
        });
        let blit_fx = Self::blit_pipe(device, &fx_pipe_layout, format, BLIT_FX);
        Self {
            pipes,
            tex_layout,
            sampler,
            frame,
            frame_bind,
            blit,
            frame_near,
            blit_fx,
            fx_buf,
            fx_bind,
            crop,
        }
    }

    fn tex_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        })
    }

    // Pipelines indexed by `PipeKey::index`: 0–7 draw (bit 0 Add blend, bit 1 z test, bit 2 z write), 8 colour + z
    // clear, 9 z-only clear.
    fn draw_pipes(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        format: wgpu::TextureFormat,
    ) -> Vec<wgpu::RenderPipeline> {
        let shader = shader(device, SHADER);
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
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2, 2 => Unorm8x4, 3 => Unorm8x4, 4 => Float32],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: mask })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(write),
                    depth_compare: Some(if test { wgpu::CompareFunction::LessEqual } else { wgpu::CompareFunction::Always }),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
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
        pipes
    }

    fn blit_pipe(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        format: wgpu::TextureFormat,
        src: &str,
    ) -> wgpu::RenderPipeline {
        let blit_shader = shader(device, src);
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    fn crop_pipe(device: &wgpu::Device, layout: &wgpu::PipelineLayout) -> wgpu::RenderPipeline {
        let crop_shader = shader(device, CROP);
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &crop_shader,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &crop_shader,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
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
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    // Full-precision RGBA8 upload (03 §3: the reduced-colour option can be ignored); `rgba` None = a blank (black,
    // alpha 0) capture target.
    fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        rgba: Option<&[u8]>,
        format: wgpu::TextureFormat,
    ) -> Texture {
        let [width, height] = size;
        let rgba = rgba.map_or_else(
            || Cow::Owned(vec![0u8; (width * height * 4) as usize]),
            Cow::Borrowed,
        );
        let tex = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: None,
                size: extent(width, height),
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
            &rgba,
        );
        let bind = bind_group(device, &self.tex_layout, &view(&tex), &self.sampler);
        Texture {
            width,
            height,
            bind,
            tex,
            label: None,
        }
    }
}

impl Gfx {
    // `vsync` = port display setting; logic pacing is separate (app.rs). The display handle lets GL present (EGL on
    // X11 / Wayland); `WGPU_BACKEND` and the other wgpu variables are honoured.
    pub async fn new(
        window: Arc<winit::window::Window>,
        display: winit::event_loop::OwnedDisplayHandle,
        vsync: bool,
    ) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::util::new_instance_with_webgpu_detection(
            wgpu::InstanceDescriptor::new_with_display_handle(Box::new(display)).with_env(),
        )
        .await;
        let surface = instance.create_surface(window.clone())?;
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
        let (device, queue) = request_device(&adapter).await?;
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
        // One queued frame: a full FIFO queue would add up to two refreshes of latency.
        config.desired_maximum_frame_latency = 1;
        config.present_mode = if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        surface.configure(&device, &config);
        let mut g = Self::build(instance, Some(surface), device, queue, config);
        #[cfg(target_arch = "wasm32")]
        watch_context_loss(&window, g.lost.clone());
        g.window = Some(window);
        Ok(g)
    }

    // Offscreen device for tests and tools: renders only into the 640x480 frame.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn headless() -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = request_device(&adapter).await?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: FRAME.0,
            height: FRAME.1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: Vec::new(),
            color_space: wgpu::SurfaceColorSpace::default(),
        };
        Ok(Self::build(instance, None, device, queue, config))
    }

    // `surface` None = headless.
    fn build(
        instance: wgpu::Instance,
        surface: Option<wgpu::Surface<'static>>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
    ) -> Self {
        let gpu = Gpu::new(&device, config.format);
        let lost = watch_loss(&device);
        #[cfg(debug_assertions)]
        let loss_at = std::env::var("TH08_DEVICE_LOSS")
            .ok()
            .and_then(|v| v.parse().ok());
        #[cfg(not(debug_assertions))]
        let loss_at = None;
        let mut g = Self {
            headless: surface.is_none(),
            window: None,
            surface,
            overlay: None,
            hud: None,
            overlay_top: None,
            safe: SafeArea::default(),
            blit_options: BlitOptions::default(),
            instance,
            device,
            queue,
            config,
            gpu,
            depth: None,
            depth_test: false,
            textures: Vec::new(),
            prov: Vec::new(),
            lost,
            generation: 0,
            loss_at,
            presents: 0,
            viewport: FULLSCREEN,
            verts: Vec::new(),
            indices: Vec::new(),
            batches: Vec::new(),
            white: TexId::WHITE,
            log: None,
            log_fog: None,
            persistent: false,
            text: None,
            pack: crate::textdb::PackText::default(),
            font: None,
        };
        // A fresh recorder's white texture (slot 0), so a Gfx without a core recorder (launcher, tools) has it too.
        g.submit(&mut Recorder::new());
        g
    }

    // Number of `rebuild`s so far: a caller holding its own GPU objects (egui renderer) remakes them when it changes.
    pub fn generation(&self) -> u32 {
        self.generation
    }

    // Port debug: destroys the device as a driver reset would; the next submit rebuilds (ARCH_D §5 manual check).
    pub fn force_device_loss(&mut self) {
        log::warn!("forcing a device loss");
        self.device.destroy();
        self.lost.store(true, Ordering::Release);
    }

    // Before each submit: a lost device is replaced (native); the web keeps the dead one and the app reloads the
    // page (`is_lost`). Callers presenting on their own (launcher) call it first, then compare `generation`.
    pub fn recover_if_lost(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.lost.load(Ordering::Acquire) {
            match self.rebuild() {
                Ok(()) => log::warn!("device rebuilt (generation {})", self.generation),
                Err(e) => log::error!("device rebuild failed: {e:#}"),
            }
        }
    }

    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Acquire)
    }

    // Device loss (ARCH_D_MOBILE §5): a new device and surface on the same instance and window, pipelines and frame
    // made again, every live slot uploaded from its provenance with its edits replayed. Captures come back black.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn rebuild(&mut self) -> anyhow::Result<()> {
        // The old swapchain goes while its device still exists; reconfiguring it onto a new device crashes (Vulkan).
        self.surface = None;
        if let Some(w) = &self.window {
            self.surface = Some(self.instance.create_surface(w.clone())?);
        }
        let adapter =
            pollster::block_on(self.instance.request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: self.surface.as_ref(),
                ..Default::default()
            }))?;
        let (device, queue) = pollster::block_on(request_device(&adapter))?;
        self.lost = watch_loss(&device);
        self.gpu = Gpu::new(&device, self.config.format);
        self.device = device;
        self.queue = queue;
        self.depth = None;
        self.reconfigure();
        self.reupload();
        self.generation += 1;
        for o in [&mut self.overlay, &mut self.hud, &mut self.overlay_top]
            .into_iter()
            .flatten()
        {
            o.device_rebuilt();
        }
        Ok(())
    }

    // Base pixels of every slot first, then each slot's edits in order (copies read the rebuilt sources).
    #[cfg(not(target_arch = "wasm32"))]
    fn reupload(&mut self) {
        for (i, p) in self.prov.iter().enumerate() {
            let Some(p) = p else {
                self.textures[i] = None;
                continue;
            };
            let rgba = p.rgba.as_deref();
            let mut t = self
                .gpu
                .upload(&self.device, &self.queue, [p.w, p.h], rgba, p.format);
            t.label = self.textures[i].take().and_then(|old| old.label);
            self.textures[i] = Some(t);
        }
        let mut enc = encoder(&self.device);
        for (i, p) in self.prov.iter().enumerate() {
            let (Some(p), Some(dst)) = (p, &self.textures[i]) else {
                continue;
            };
            for e in &p.edits {
                match *e {
                    Edit::Write { at, size, ref rgba } => {
                        write_region(&self.queue, &dst.tex, at, size, rgba);
                    }
                    Edit::Copy {
                        src,
                        from,
                        to,
                        size,
                    } => {
                        if let Some(Some(a)) = self.textures.get(src.index()) {
                            copy_region(&self.device, &mut enc, &a.tex, from, &dst.tex, to, size);
                        }
                    }
                }
            }
        }
        self.queue.submit([enc.finish()]);
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
        self.window = None;
    }

    // App `Resumed` again: a surface for the (new) native window on the same device, same format and present mode.
    pub fn attach(&mut self, window: Arc<winit::window::Window>) -> anyhow::Result<()> {
        if self.headless {
            anyhow::bail!("headless device has no window surface");
        }
        let size = window.inner_size();
        let surface = self.instance.create_surface(window.clone())?;
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        surface.configure(&self.device, &self.config);
        self.surface = Some(surface);
        self.window = Some(window);
        Ok(())
    }

    // The window's next texture, None to skip this frame. A lost surface (its native window went away under it) is
    // made again on the current window; an outdated one is reconfigured.
    pub fn acquire(&mut self) -> Option<wgpu::SurfaceTexture> {
        use wgpu::CurrentSurfaceTexture as T;
        match self.surface.as_ref()?.get_current_texture() {
            T::Success(f) | T::Suboptimal(f) => Some(f),
            T::Outdated => {
                self.reconfigure();
                None
            }
            T::Lost => {
                log::warn!("surface lost: made again");
                self.surface = None;
                if let Some(w) = self.window.clone()
                    && let Err(e) = self.attach(w)
                {
                    log::error!("surface: {e:#}");
                }
                None
            }
            T::Timeout | T::Occluded | T::Validation => None,
        }
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
        let (w, h) = FRAME;
        let fmt = self.config.format;
        self.create_texture_fmt(id, w, h, None, fmt);
        if self.surface.is_some() {
            let mut enc = encoder(&self.device);
            let dst = &self.tex(id).tex;
            enc.copy_texture_to_texture(
                self.gpu.frame.as_image_copy(),
                dst.as_image_copy(),
                extent(w, h),
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
        if size == to_size
            && let Some(mut log) = self.log.take()
        {
            log.push(LogRec::Copy {
                src: "screen".into(),
                dst: self.label_name(dst),
                from,
                size,
                to,
            });
            self.log = Some(log);
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
        let target = view(&t.tex);
        let mut enc = encoder(&self.device);
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
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
            pass.set_pipeline(&self.gpu.crop);
            pass.set_bind_group(0, &self.gpu.frame_bind, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.draw(0..4, 0..1);
        }
        self.queue.submit([enc.finish()]);
    }

    // Upload at core id `id`: `rgba` from `ResCmd::Create` / `CreateImage`, None = blank capture target; records its provenance.
    fn create_texture_fmt(
        &mut self,
        id: TexId,
        width: u32,
        height: u32,
        rgba: Option<&Arc<[u8]>>,
        format: wgpu::TextureFormat,
    ) {
        let t = self.gpu.upload(
            &self.device,
            &self.queue,
            [width, height],
            rgba.map(|b| &b[..]),
            format,
        );
        let p = Provenance {
            w: width,
            h: height,
            format,
            // The web never rebuilds (`recover_if_lost`): no second copy of the pixels there.
            rgba: rgba.filter(|_| cfg!(not(target_arch = "wasm32"))).cloned(),
            edits: Vec::new(),
        };
        if self.textures.len() <= id.index() {
            self.textures.resize_with(id.index() + 1, || None);
            self.prov.resize_with(id.index() + 1, || None);
        }
        self.textures[id.index()] = Some(t);
        self.prov[id.index()] = Some(p);
    }

    // Appends an edit to slot `id`'s provenance, dropping earlier ones it fully covers (bounded by distinct rects).
    fn push_edit(&mut self, id: TexId, e: Edit) {
        if cfg!(target_arch = "wasm32") {
            return;
        }
        if let Some(Some(p)) = self.prov.get_mut(id.index()) {
            let r = e.rect();
            p.edits.retain(|o| !rect_within(o.rect(), r));
            p.edits.push(e);
        }
    }

    // The core table already skipped white and double frees.
    fn do_free_texture(&mut self, id: TexId) {
        if let Some(t) = self.textures.get_mut(id.index()) {
            *t = None;
        }
        if let Some(p) = self.prov.get_mut(id.index()) {
            *p = None;
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
        write_region(&self.queue, &t.tex, [x, y], [w, h], rgba);
        self.push_edit(
            id,
            Edit::Write {
                at: [x, y],
                size: [w, h],
                rgba: rgba.to_vec(),
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
        let mut enc = encoder(&self.device);
        copy_region(&self.device, &mut enc, &a.tex, from, &b.tex, to, [w, h]);
        self.queue.submit([enc.finish()]);
        self.push_edit(
            dst,
            Edit::Copy {
                src,
                from,
                to,
                size: [w, h],
            },
        );
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
            if let Some(log) = self.log.as_mut() {
                log.push(rec);
            }
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
    #[expect(
        clippy::cast_sign_loss,
        reason = "draw-log K fields: ftol of the screen position"
    )]
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
    #[expect(
        clippy::cast_sign_loss,
        reason = "draw-log K fields: ftol of the screen position"
    )]
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
            size: extent(w, h),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let v = view(&tex);
        self.depth = Some((w, h, v.clone()));
        v
    }

    // Scissor = batch viewport rounded and clamped to the frame (D3D viewport clip).
    #[expect(clippy::cast_sign_loss, reason = "clamped to 0 … 640 / 480 first")]
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
                pass.set_pipeline(&self.gpu.pipes[b.key.index()]);
                pass.set_bind_group(0, &self.tex(b.tex).bind, &[]);
                pass.draw_indexed(b.start..b.end, 0, 0..1);
            }
        }
    }

    fn do_present(&mut self) {
        self.present_frame();
        self.presented();
    }

    // Counts a Present for the TH08_DEVICE_LOSS hook; the launcher, which presents outside `Gfx`, calls it too.
    pub fn presented(&mut self) {
        self.presents += 1;
        if self.loss_at == Some(self.presents) {
            self.force_device_loss();
        }
    }

    fn present_frame(&mut self) {
        let Some(frame) = self.acquire() else {
            self.clear_batches();
            return;
        };
        let target = view(&frame.texture);
        let mut enc = encoder(&self.device);
        self.render_frame(&mut enc);
        self.blit(&mut enc, &target, self.config.width, self.config.height);
        self.paint_overlay(&mut enc, &target, (self.config.width, self.config.height));
        self.queue.submit([enc.finish()]);
        self.queue.present(frame);
        self.clear_batches();
    }

    // Port UI over a held game (port menu, ARCH_D_MOBILE §5): the last rendered 640×480 frame blitted again, then
    // `paint` on the window target; no batches run, the frame texture is untouched. False: no frame was presented.
    pub fn present_last(
        &mut self,
        paint: impl FnOnce(OverlayTarget<'_>, &mut wgpu::CommandEncoder),
    ) -> bool {
        self.recover_if_lost();
        let Some(frame) = self.acquire() else {
            return false;
        };
        let target = view(&frame.texture);
        let mut enc = encoder(&self.device);
        let size = (self.config.width, self.config.height);
        self.blit(&mut enc, &target, size.0, size.1);
        let t = OverlayTarget {
            device: &self.device,
            queue: &self.queue,
            view: &target,
            format: self.config.format,
            size,
            frame: self.frame_rect(size.0, size.1),
        };
        paint(t, &mut enc);
        self.queue.submit([enc.finish()]);
        self.queue.present(frame);
        self.presented();
        true
    }

    // `present_last` with this device's own overlays (held replay frame).
    pub fn present_held(&mut self) -> bool {
        let (mut a, mut b) = (self.overlay.take(), self.hud.take());
        let shown = self.present_last(|t, enc| {
            for o in [&mut a, &mut b].into_iter().flatten() {
                o.paint(t, enc);
            }
        });
        (self.overlay, self.hud) = (a, b);
        shown
    }

    fn paint_overlay(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        let frame = self.frame_rect(size.0, size.1);
        for o in [&mut self.overlay, &mut self.hud, &mut self.overlay_top]
            .into_iter()
            .flatten()
        {
            let t = OverlayTarget {
                device: &self.device,
                queue: &self.queue,
                view,
                format: self.config.format,
                size,
                frame,
            };
            o.paint(t, enc);
        }
    }

    // The letterboxed frame rect `[x, y, w, h]` of a w×h target: the safe rect, scaled (whole steps with
    // `integer_scale`), centred.
    pub fn frame_rect(&self, w: u32, h: u32) -> [f32; 4] {
        let [x, y, w, h] = self.safe.rect(w, h);
        let mut scale = (w / SCREEN_W).min(h / SCREEN_H);
        if self.blit_options.integer && scale >= 1.0 {
            scale = scale.floor();
        }
        let (vw, vh) = ((SCREEN_W * scale).round(), (SCREEN_H * scale).round());
        [
            x + ((w - vw) / 2.0).floor(),
            y + ((h - vh) / 2.0).floor(),
            vw,
            vh,
        ]
    }

    // Letterboxed, scaled copy of the 640x480 frame into the safe rect of a w×h target.
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
        let o = self.blit_options;
        let [x, y, vw, vh] = self.frame_rect(w, h);
        pass.set_viewport(x, y, vw, vh, 0.0, 1.0);
        let frame = if o.filter == ScaleFilter::Nearest {
            &self.gpu.frame_near
        } else {
            &self.gpu.frame_bind
        };
        if o.plain() {
            pass.set_pipeline(&self.gpu.blit);
        } else {
            let u = o.uniform([vw / SCREEN_W, vh / SCREEN_H]);
            self.queue
                .write_buffer(&self.gpu.fx_buf, 0, bytemuck::cast_slice(&u));
            pass.set_pipeline(&self.gpu.blit_fx);
            pass.set_bind_group(1, &self.gpu.fx_bind, &[]);
        }
        pass.set_bind_group(0, frame, &[]);
        pass.draw(0..3, 0..1);
    }

    // Renders the pending batches into the 640x480 frame without presenting (tools).
    pub fn flush(&mut self, rec: &mut Recorder) {
        self.submit(rec);
        let mut enc = encoder(&self.device);
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
        let (w, h) = FRAME;
        let depth = self.depth_view(w, h);
        let target = view(&self.gpu.frame);
        self.encode(enc, &target, &depth);
    }

    // Renders the pending batches at 640x480 into memory, optionally scaled to `size` like the window (RGBA).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture(&mut self, rec: &mut Recorder, size: Option<(u32, u32)>) -> image::RgbaImage {
        self.submit(rec);
        let mut enc = encoder(&self.device);
        self.render_frame(&mut enc);
        let scaled = size.map(|(w, h)| {
            let t = self.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: extent(w, h),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let target = view(&t);
            self.blit(&mut enc, &target, w, h);
            self.paint_overlay(&mut enc, &target, (w, h));
            t
        });
        let src = scaled.as_ref().unwrap_or(&self.gpu.frame);
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
            size: u64::from(row * h),
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
            extent(w, h),
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
            px.as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|p| p.swap(0, 2));
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

// Executor: runs the recorded command lists on the device.
impl Gfx {
    // Runs everything recorded so far in call order: each `ResCmd` before the draw that followed it.
    pub fn submit(&mut self, rec: &mut Recorder) {
        self.recover_if_lost();
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
                self.create_texture_fmt(*id, *w, *h, Some(rgba), wgpu::TextureFormat::Rgba8Unorm);
            }
            ResCmd::CreateImage {
                id,
                bytes,
                w,
                h,
                fit,
            } => {
                let rgba = if *fit {
                    decode_fitted(bytes, *w, *h)
                } else {
                    decode_image(bytes, *w, *h)
                };
                self.create_texture_fmt(*id, *w, *h, Some(&rgba), wgpu::TextureFormat::Rgba8Unorm);
            }
            ResCmd::CreateAnmOver {
                id,
                w,
                h,
                base,
                rects,
                bytes,
            } => {
                let rgba = decode_over(bytes, base, rects, *w, *h);
                self.create_texture_fmt(*id, *w, *h, Some(&rgba), wgpu::TextureFormat::Rgba8Unorm);
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

    // A mounted presentation pack's text side (I18N G2): texts, FileFont, overflow modes.
    pub fn set_pack(&mut self, pack: crate::textdb::PackText) {
        self.pack = pack;
        self.text = None;
    }

    // Text_RenderToVm 0x4663B0 (10 §6): translation or Shift-JIS decode, CPU raster, upload over the strip.
    // Text equal to the vanilla decode takes the vanilla path (system font, byte-formula `x`), so untranslated
    // strings keep their pixels with a pack mounted.
    fn do_text(&mut self, t: &th08_core::drawlist::TextCmd) {
        let s = self.pack.db.resolve(&t.key, &t.text);
        let translated = !self.pack.db.is_empty() && s != t.text.decode();
        let (font, pack_font) = (self.font.as_deref(), &self.pack.fonts);
        let raster = self
            .text
            .get_or_insert_with(|| crate::text::Raster::new(font, pack_font.clone()));
        let (w, size) = (t.w as usize, t.size as usize);
        let rgba = if translated {
            let overflow = self.pack.overflow(t.site);
            raster.render_layout(
                &s,
                t.color,
                w,
                size,
                t.outline,
                (t.align, t.x, t.edge),
                overflow,
            )
        } else {
            raster.render_outline(&s, t.color, w, size, t.x, t.outline)
        };
        self.do_write_texture(t.dst, 0, t.y, t.w, t.size, &rgba);
    }
}

impl th08_core::drawlist::Renderer for Gfx {
    fn submit(&mut self, rec: &mut Recorder) {
        Gfx::submit(self, rec);
    }

    fn enable_log(&mut self) {
        Gfx::enable_log(self);
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

// `w`×`h` RGBA texels at `at`, rows `w * 4` bytes apart.
fn write_region(
    queue: &wgpu::Queue,
    tex: &wgpu::Texture,
    at: [u32; 2],
    size: [u32; 2],
    rgba: &[u8],
) {
    let [w, h] = size;
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: at[0],
                y: at[1],
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        extent(w, h),
    );
}

// Texel rect copy `src`@`from` → `dst`@`to`, already clipped to both textures. wgpu rejects a copy within one
// texture, so `src == dst` (e.g. two sprites of one ANM texture) goes through a scratch texture: the whole source
// rect is read before any destination texel is written.
fn copy_region(
    device: &wgpu::Device,
    enc: &mut wgpu::CommandEncoder,
    src: &wgpu::Texture,
    from: [u32; 2],
    dst: &wgpu::Texture,
    to: [u32; 2],
    size: [u32; 2],
) {
    let info = |t, p: [u32; 2]| wgpu::TexelCopyTextureInfo {
        texture: t,
        mip_level: 0,
        origin: wgpu::Origin3d {
            x: p[0],
            y: p[1],
            z: 0,
        },
        aspect: wgpu::TextureAspect::All,
    };
    let ext = extent(size[0], size[1]);
    if src != dst {
        enc.copy_texture_to_texture(info(src, from), info(dst, to), ext);
        return;
    }
    let scratch = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("self-copy scratch"),
        size: ext,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: src.format(),
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    enc.copy_texture_to_texture(info(src, from), info(&scratch, [0, 0]), ext);
    enc.copy_texture_to_texture(info(&scratch, [0, 0]), info(dst, to), ext);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba(f(x, y)));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    // Empty sprite rects keep the vanilla pixels; drawn ones are replaced; a partial sheet is placed 1:1.
    #[test]
    fn pack_image_merges_per_sprite() {
        let base: Arc<[u8]> = Arc::from(vec![9u8; 4 * 4 * 4]);
        let rects = [[0, 0, 2, 2], [2, 0, 2, 2], [0, 2, 4, 2]];
        let over = png(4, 4, |x, y| if x >= 2 && y < 2 { [1; 4] } else { [0; 4] });
        let out = decode_over(&over, &base, &rects, 4, 4);
        let px = |x: usize, y: usize| out[(y * 4 + x) * 4];
        assert_eq!((px(0, 0), px(3, 1), px(1, 3)), (9, 1, 9));
        let strip = png(4, 1, |_, _| [5; 4]);
        let out = decode_over(&strip, &base, &rects, 4, 4);
        assert_eq!((out[0], out[4 * 4 * 3]), (5, 9));
    }

    // One frame drawing an 8×8 texture made of bytes, a write and a copy, scaled over the frame.
    fn frame(gfx: &mut Gfx, rec: &mut Recorder, tex: TexId) -> image::RgbaImage {
        rec.image_rect(tex, 0, 0, 8, 8, 0.0, 0.0);
        rec.sprite(
            tex,
            Blend::Alpha,
            FULLSCREEN,
            [0.0, 0.0, 1.0, 1.0],
            [255; 4],
        );
        gfx.capture(rec, None)
    }

    #[test]
    fn rebuild_restores_bytes_writes_and_copies() {
        let Ok(mut gfx) = pollster::block_on(Gfx::headless()) else {
            eprintln!("no GPU adapter; skipped");
            return;
        };
        let mut rec = Recorder::new();
        let base: Vec<u8> = (0..8 * 8 * 4).map(|i: u32| (i * 7 % 251) as u8).collect();
        let tex = rec.create_texture(8, 8, &base);
        rec.write_texture(tex, 2, 2, 2, 2, &[200u8; 16]);
        rec.write_texture(tex, 1, 1, 4, 4, &[90u8; 64]);
        let src = rec.create_texture(2, 2, &[30u8; 16]);
        rec.copy_texture(src, [0, 0], tex, [6, 6], [2, 2]);
        let before = frame(&mut gfx, &mut rec, tex);
        let p = gfx.prov[tex.index()].as_ref().expect("provenance");
        assert_eq!(p.edits.len(), 2, "the 2×2 write lies inside the 4×4 one");
        gfx.force_device_loss();
        let after = frame(&mut gfx, &mut rec, tex);
        assert_eq!(gfx.generation(), 1);
        assert!(before == after, "rebuilt frame differs");
    }

    #[test]
    fn copy_within_one_texture() {
        let Ok(mut gfx) = pollster::block_on(Gfx::headless()) else {
            eprintln!("no GPU adapter; skipped");
            return;
        };
        let mut rec = Recorder::new();
        let base: Vec<u8> = (0..8 * 8 * 4).map(|i: u32| (i * 13 % 247) as u8).collect();
        let mut want = base.clone();
        for y in 0..3 {
            for x in 0..4 {
                let s = ((y + 1) * 8 + x) * 4;
                let d = ((y + 5) * 8 + x + 3) * 4;
                want.copy_within(s..s + 4, d);
            }
        }
        let tex = rec.create_texture(8, 8, &base);
        rec.copy_texture(tex, [0, 1], tex, [3, 5], [4, 3]);
        let reference = rec.create_texture(8, 8, &want);
        let got = frame(&mut gfx, &mut rec, tex);
        assert!(
            got == frame(&mut gfx, &mut rec, reference),
            "self-copy differs"
        );
        gfx.force_device_loss();
        assert!(
            got == frame(&mut gfx, &mut rec, tex),
            "rebuilt self-copy differs"
        );
    }

    // Blit options touch only the scaled window copy; at 2× sharp-bilinear hits texel centres like nearest.
    #[test]
    fn blit_options_window_only() {
        let Ok(mut gfx) = pollster::block_on(Gfx::headless()) else {
            eprintln!("no GPU adapter; skipped");
            return;
        };
        let mut rec = Recorder::new();
        let base: Vec<u8> = (0..8 * 8 * 4).map(|i: u32| (i * 11 % 253) as u8).collect();
        let tex = rec.create_texture(8, 8, &base);
        let mut shot = |gfx: &mut Gfx, o: BlitOptions, size| {
            gfx.blit_options = o;
            rec.sprite(
                tex,
                Blend::Alpha,
                FULLSCREEN,
                [0.0, 0.0, 1.0, 1.0],
                [255; 4],
            );
            gfx.capture(&mut rec, size)
        };
        let plain = BlitOptions::default();
        let with = |filter, color| BlitOptions {
            filter,
            color,
            ..plain
        };
        let fx = with(ScaleFilter::Sharp, ColorFilter::Deutan);
        assert!(shot(&mut gfx, plain, None) == shot(&mut gfx, fx, None));
        let two = Some((1280, 960));
        let near = shot(&mut gfx, with(ScaleFilter::Nearest, ColorFilter::Off), two);
        let sharp = shot(&mut gfx, with(ScaleFilter::Sharp, ColorFilter::Off), two);
        let max = near
            .pixels()
            .zip(sharp.pixels())
            .flat_map(|(a, b)| (0..4).map(move |i| a[i].abs_diff(b[i])))
            .max();
        assert!(
            max <= Some(1),
            "sharp at 2× differs from nearest by {max:?}"
        );
        assert!(shot(&mut gfx, plain, two) != near);
    }

    #[test]
    fn colour_filters_keep_grey() {
        for f in [
            ColorFilter::Off,
            ColorFilter::Protan,
            ColorFilter::Deutan,
            ColorFilter::Tritan,
        ] {
            let (m, o) = f.matrix();
            for (row, off) in m.iter().zip(o) {
                assert!((row.iter().sum::<f32>() + off - 1.0).abs() < 0.01, "{f:?}");
            }
        }
    }

    #[test]
    fn rect_within_edges() {
        assert!(rect_within([1, 1, 2, 2], [1, 1, 2, 2]));
        assert!(rect_within([2, 2, 1, 1], [0, 0, 4, 4]));
        assert!(!rect_within([3, 3, 2, 2], [0, 0, 4, 4]));
        assert!(!rect_within([0, 0, 1, 1], [1, 1, 4, 4]));
    }
}
