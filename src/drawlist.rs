//! Core-side draw output: `DrawCmd` / `ResCmd` lists, core-owned `TexId`s and the device-state mirror (ARCH_A §2.2,
//! REWRITE_PLAN step 8; graphics 12 §1, §3). No wgpu type; the app's `Renderer` runs them.

use std::sync::Arc;

use th08_formats::sjis::SjisString;

// The original renders at 640×480 (05 §12.4); the window is a scaled copy.
pub const SCREEN_W: f32 = 640.0;
pub const SCREEN_H: f32 = 480.0;

// `pos` in wgpu clip space; `fog` is the vertex fog factor f of 06 §10.1 (1 = no fog), mixed with `fog_color`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
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
        id: TexId,
        textured: bool,
        blend: Blend,
        test: bool,
        write: bool,
        vp: Rect,
        fog: Option<LogFog>,
        v: [Vertex; 4],
    },
    // `arg`: the D3DCOLOR argument of a z-only clear, ignored by D3D but logged (11 §3.1, 06 §3 step 5).
    Clear {
        vp: Rect,
        color: Option<[u8; 4]>,
        arg: u32,
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

// Core-owned texture handle (port); slot 0 is the 1×1 white texture, freed slots are reused last-freed first.
// The numbers appear in the draw log (`?<id hex>` for unlabelled textures), so the allocation order is frozen.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct TexId(pub u32);

impl TexId {
    pub const WHITE: TexId = TexId(0);

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl std::fmt::LowerHex for TexId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::LowerHex::fmt(&self.0, f)
    }
}

// Where a texture's pixels came from, kept for a device-loss rebuild (ARCH_D §5, §6.1 item 4).
#[derive(Clone, Debug)]
pub enum TexSource {
    White,
    // ANM file entry `file#entry` (03 §2); blank '@' render targets too.
    Anm { file: Arc<str>, entry: u32 },
    // Standalone .jpg / .png (03 §1).
    Image { name: Arc<str> },
    // Pixels handed in by the caller (shared with the `ResCmd::Create` that uploaded them).
    Bytes(Arc<[u8]>),
    // Post-present screen capture (12 §1); comes back black after a rebuild until captured again.
    Capture,
}

#[derive(Clone, Debug)]
pub struct TexEntry {
    pub width: u32,
    pub height: u32,
    pub label: Option<LogTex>,
    pub src: TexSource,
}

// Logical texture size, answered by the core table (no device read).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TexInfo {
    pub width: u32,
    pub height: u32,
}

// Slot allocator for `TexId`s: same order as the device table it replaces (push, or pop the last freed slot).
#[derive(Debug, Default)]
pub struct ResTable {
    slots: Vec<Option<TexEntry>>,
    free: Vec<TexId>,
}

impl ResTable {
    fn alloc(&mut self, e: TexEntry) -> TexId {
        if let Some(id) = self.free.pop() {
            self.slots[id.index()] = Some(e);
            id
        } else {
            self.slots.push(Some(e));
            TexId(self.slots.len() as u32 - 1)
        }
    }

    // Live slot; `None` for freed or never allocated ids.
    pub fn get(&self, id: TexId) -> Option<&TexEntry> {
        self.slots.get(id.index())?.as_ref()
    }

    pub fn iter(&self) -> impl Iterator<Item = (TexId, &TexEntry)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, e)| Some((TexId(i as u32), e.as_ref()?)))
    }
}

// The device state core reads back today (viewport, ZFUNC, draw-log fog); reset by every Present like the device's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceState {
    pub viewport: Rect,
    pub depth_test: bool,
    pub log_fog: Option<LogFog>,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            viewport: FULLSCREEN,
            depth_test: false,
            log_fog: None,
        }
    }
}

// One device draw / state call, 1:1 with the `Recorder` method of the same name (05 §9, §12; 06 §10; 12 §1).
#[derive(Clone, Debug)]
pub enum DrawCmd {
    SetViewport(Rect),
    ClearViewport([u8; 4]),
    Sprite {
        tex: TexId,
        blend: Blend,
        dst: Rect,
        uv: [f32; 4],
        color: [u8; 4],
    },
    Quad {
        tex: TexId,
        blend: Blend,
        corners: [[f32; 2]; 4],
        uv: [[f32; 2]; 4],
        color: [u8; 4],
    },
    QuadStrip {
        tex: TexId,
        blend: Blend,
        pos: [[f32; 2]; 4],
        uv: [[f32; 2]; 4],
        color: [u8; 4],
    },
    Strip {
        tex: TexId,
        blend: Blend,
        zwrite: bool,
        v: [Vertex; 4],
    },
    SetDepthTest(bool),
    ClearDepth(Option<[u8; 4]>),
    ClearZ(u32),
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [u8; 4],
    },
    SolidQuad {
        blend: Blend,
        corners: [[f32; 2]; 4],
        color: [u8; 4],
    },
    Image {
        tex: TexId,
        x: f32,
        y: f32,
    },
    ImageRect {
        tex: TexId,
        sx: u32,
        sy: u32,
        w: u32,
        h: u32,
        dx: f32,
        dy: f32,
    },
    Fan {
        blend: Blend,
        v: Vec<Vertex>,
    },
    Raw {
        tex: Option<TexId>,
        blend: Blend,
        test: bool,
        write: bool,
        v: [Vertex; 4],
    },
    SetLogFog(Option<LogFog>),
    // Device Present (12 §1); in-calc presents exist, so a frame's list may hold several.
    Present,
}

// Texture resource / capture command; ids are already allocated by the `ResTable`.
#[derive(Clone, Debug)]
pub enum ResCmd {
    // RGBA8 texture (03 §3: full precision).
    Create {
        id: TexId,
        w: u32,
        h: u32,
        rgba: Arc<[u8]>,
    },
    // Post-present capture into a new 640×480 surface-format texture (12 §1, 0x44886F).
    CaptureScreen {
        id: TexId,
        label: Arc<str>,
    },
    // Anm_CaptureToTexture 0x466F20 (12 §3).
    CaptureRect {
        dst: TexId,
        from: [u32; 2],
        size: [u32; 2],
        to: [u32; 2],
        to_size: [u32; 2],
    },
    Free(TexId),
    Write {
        id: TexId,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        rgba: Vec<u8>,
    },
    Copy {
        src: TexId,
        from: [u32; 2],
        dst: TexId,
        to: [u32; 2],
        size: [u32; 2],
    },
    SetLabel {
        id: TexId,
        label: Arc<str>,
        w: u32,
        h: u32,
    },
    Text(TextCmd),
}

// Text_RenderToVm 0x4663B0 family (10 §6) as an output command (ARCH_SUMMARY decision 7): the renderer decodes and
// rasterises `text` and replaces rows `y .. y+size` of `dst`, `w` texels wide (x = 0), at this point in call order.
#[derive(Clone, Debug)]
pub struct TextCmd {
    pub dst: TexId,
    pub y: u32,
    // Texture-row width W and font size (strip height) in texels.
    pub w: u32,
    pub size: u32,
    pub text: SjisString,
    pub color: [u8; 3],
    // Pen start x in strip pixels.
    pub x: f32,
    // −1 → ±1 px outline offsets (2× space), anything else ±2 px.
    pub outline: i32,
}

// AnmManager's D3D render-state mirrors: blend +0x24C0 (05 §12.5), bound texture +0x24BC and texture-matrix sprite
// +0x24C8 (05 §12.8). Only draw paths read them, so they are emit state (ARCH decision 2): with emit off they keep
// their last emitted values, as on a frame-skipped frame of the original (00 §2), and are never part of the World.
#[derive(Clone, Copy, Debug)]
pub struct AnmCache {
    pub last_blend: Blend,
    pub bound_tex: Option<TexId>,
    // (anm slot, sprite index) and the uploaded UV offset.
    pub last_tex_matrix: Option<((i16, i16), [f32; 2])>,
}

impl Default for AnmCache {
    fn default() -> Self {
        Self {
            last_blend: Blend::Alpha,
            bound_tex: None,
            last_tex_matrix: None,
        }
    }
}

impl AnmCache {
    // Texture-matrix and texture caches reset (05 §12.8); the blend state carries over between frames (§12.5).
    pub fn reset(&mut self) {
        self.last_tex_matrix = None;
        self.bound_tex = None;
    }
}

// Per-frame command recorder (the former `Gfx` draw API). `res` entries carry the `draws` length at record time, so
// the executor replays both lists in exact call order (log order of K records; GPU order is unchanged either way).
#[derive(Debug)]
pub struct Recorder {
    pub draws: Vec<DrawCmd>,
    pub res: Vec<(usize, ResCmd)>,
    pub state: DeviceState,
    pub table: ResTable,
    pub anm: AnmCache,
    // Emit off (`Ctx::set_emit`): draw / state commands are dropped, mirrors still update; resource commands kept.
    // Also covers draws outside the draw chain (in-calc loading presents).
    pub mute: bool,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    // Slot 0 = the 1×1 white texture used by untextured draws and clears.
    pub fn new() -> Self {
        let mut r = Self {
            draws: Vec::new(),
            res: Vec::new(),
            state: DeviceState::default(),
            table: ResTable::default(),
            anm: AnmCache::default(),
            mute: false,
        };
        let id = r.create_entry(1, 1, TexSource::White, Arc::from([255u8; 4].as_slice()));
        debug_assert_eq!(id, TexId::WHITE);
        r
    }

    // Drains this frame's lists (the executor's input).
    pub fn take(&mut self) -> (Vec<DrawCmd>, Vec<(usize, ResCmd)>) {
        (
            std::mem::take(&mut self.draws),
            std::mem::take(&mut self.res),
        )
    }

    fn draw(&mut self, c: DrawCmd) {
        if !self.mute {
            self.draws.push(c);
        }
    }

    // Stand-in left in `Ctx` while a task's emit holds the real recorder (`task::run`); no white entry, no allocation.
    pub fn detached() -> Self {
        Self {
            draws: Vec::new(),
            res: Vec::new(),
            state: DeviceState::default(),
            table: ResTable::default(),
            anm: AnmCache::default(),
            mute: true,
        }
    }

    fn res_cmd(&mut self, c: ResCmd) {
        self.res.push((self.draws.len(), c));
    }

    // --- device-state reads ---

    pub fn viewport(&self) -> Rect {
        self.state.viewport
    }

    pub fn white(&self) -> TexId {
        TexId::WHITE
    }

    // Logical size of a live texture; panics on a freed id like the device table did.
    pub fn texture(&self, id: TexId) -> TexInfo {
        let e = self.table.get(id).expect("freed texture");
        TexInfo {
            width: e.width,
            height: e.height,
        }
    }

    // --- draw / state commands ---

    pub fn set_viewport(&mut self, r: Rect) {
        self.state.viewport = r;
        self.draw(DrawCmd::SetViewport(r));
    }

    pub fn clear_viewport(&mut self, color: [u8; 4]) {
        self.draw(DrawCmd::ClearViewport(color));
    }

    pub fn sprite(&mut self, tex: TexId, blend: Blend, dst: Rect, uv: [f32; 4], color: [u8; 4]) {
        self.draw(DrawCmd::Sprite {
            tex,
            blend,
            dst,
            uv,
            color,
        });
    }

    pub fn quad(
        &mut self,
        tex: TexId,
        blend: Blend,
        corners: [[f32; 2]; 4],
        uv: [[f32; 2]; 4],
        color: [u8; 4],
    ) {
        self.draw(DrawCmd::Quad {
            tex,
            blend,
            corners,
            uv,
            color,
        });
    }

    pub fn quad_strip(
        &mut self,
        tex: TexId,
        blend: Blend,
        pos: [[f32; 2]; 4],
        uv: [[f32; 2]; 4],
        color: [u8; 4],
    ) {
        self.draw(DrawCmd::QuadStrip {
            tex,
            blend,
            pos,
            uv,
            color,
        });
    }

    pub fn strip(&mut self, tex: TexId, blend: Blend, zwrite: bool, v: [Vertex; 4]) {
        self.draw(DrawCmd::Strip {
            tex,
            blend,
            zwrite,
            v,
        });
    }

    pub fn set_depth_test(&mut self, on: bool) {
        self.state.depth_test = on;
        self.draw(DrawCmd::SetDepthTest(on));
    }

    pub fn clear_depth(&mut self, color: Option<[u8; 4]>) {
        self.draw(DrawCmd::ClearDepth(color));
    }

    pub fn clear_z(&mut self, arg: u32) {
        self.draw(DrawCmd::ClearZ(arg));
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
        self.draw(DrawCmd::Rect { x, y, w, h, color });
    }

    pub fn solid_quad(&mut self, blend: Blend, corners: [[f32; 2]; 4], color: [u8; 4]) {
        self.draw(DrawCmd::SolidQuad {
            blend,
            corners,
            color,
        });
    }

    pub fn image(&mut self, tex: TexId, x: f32, y: f32) {
        self.draw(DrawCmd::Image { tex, x, y });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn image_rect(&mut self, tex: TexId, sx: u32, sy: u32, w: u32, h: u32, dx: f32, dy: f32) {
        self.draw(DrawCmd::ImageRect {
            tex,
            sx,
            sy,
            w,
            h,
            dx,
            dy,
        });
    }

    pub fn fan(&mut self, blend: Blend, v: &[Vertex]) {
        self.draw(DrawCmd::Fan {
            blend,
            v: v.to_vec(),
        });
    }

    pub fn draw_raw(
        &mut self,
        tex: Option<TexId>,
        blend: Blend,
        test: bool,
        write: bool,
        v: [Vertex; 4],
    ) {
        self.draw(DrawCmd::Raw {
            tex,
            blend,
            test,
            write,
            v,
        });
    }

    pub fn set_log_fog(&mut self, fog: Option<LogFog>) {
        self.state.log_fog = fog;
        self.draw(DrawCmd::SetLogFog(fog));
    }

    // The device resets viewport, ZFUNC and the log fog after each Present (port executor `clear_batches`).
    pub fn present(&mut self) {
        self.state = DeviceState::default();
        self.draw(DrawCmd::Present);
    }

    // --- resources ---

    fn create_entry(&mut self, w: u32, h: u32, src: TexSource, rgba: Arc<[u8]>) -> TexId {
        let id = self.table.alloc(TexEntry {
            width: w,
            height: h,
            label: None,
            src,
        });
        self.res_cmd(ResCmd::Create { id, w, h, rgba });
        id
    }

    pub fn create_texture(&mut self, width: u32, height: u32, rgba: &[u8]) -> TexId {
        let rgba: Arc<[u8]> = Arc::from(rgba);
        self.create_entry(width, height, TexSource::Bytes(rgba.clone()), rgba)
    }

    // ANM entry texture (03 §2): provenance `file#entry` instead of a pixel copy.
    pub fn create_anm_texture(
        &mut self,
        file: &str,
        entry: u32,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> TexId {
        let src = TexSource::Anm {
            file: file.into(),
            entry,
        };
        self.create_entry(width, height, src, Arc::from(rgba))
    }

    pub fn capture_screen(&mut self, label: &str) -> TexId {
        let (w, h) = (SCREEN_W as u32, SCREEN_H as u32);
        let id = self.table.alloc(TexEntry {
            width: w,
            height: h,
            label: None,
            src: TexSource::Capture,
        });
        self.label_entry(id, label, w, h);
        self.res_cmd(ResCmd::CaptureScreen {
            id,
            label: label.into(),
        });
        id
    }

    pub fn capture_rect(
        &mut self,
        dst: TexId,
        from: [u32; 2],
        size: [u32; 2],
        to: [u32; 2],
        to_size: [u32; 2],
    ) {
        self.res_cmd(ResCmd::CaptureRect {
            dst,
            from,
            size,
            to,
            to_size,
        });
    }

    // The white slot is never freed; a second free of the same id is ignored.
    pub fn free_texture(&mut self, id: TexId) {
        if id != TexId::WHITE
            && self
                .table
                .slots
                .get_mut(id.index())
                .and_then(Option::take)
                .is_some()
        {
            self.table.free.push(id);
            self.res_cmd(ResCmd::Free(id));
        }
    }

    // Standalone .jpg / .png images (03 §1), decoded here so the size is known; `name` is the draw-log label.
    pub fn load_image(&mut self, name: &str, bytes: &[u8]) -> anyhow::Result<TexId> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        let (w, h) = (img.width(), img.height());
        let src = TexSource::Image { name: name.into() };
        let id = self.create_entry(w, h, src, Arc::from(img.into_raw()));
        self.set_label(id, name, w, h);
        Ok(id)
    }

    pub fn write_texture(&mut self, id: TexId, x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) {
        self.res_cmd(ResCmd::Write {
            id,
            x,
            y,
            w,
            h,
            rgba: rgba.to_vec(),
        });
    }

    pub fn text(&mut self, t: TextCmd) {
        self.res_cmd(ResCmd::Text(t));
    }

    pub fn copy_texture(
        &mut self,
        src: TexId,
        from: [u32; 2],
        dst: TexId,
        to: [u32; 2],
        size: [u32; 2],
    ) {
        self.res_cmd(ResCmd::Copy {
            src,
            from,
            dst,
            to,
            size,
        });
    }

    fn label_entry(&mut self, id: TexId, label: &str, w: u32, h: u32) {
        if let Some(Some(e)) = self.table.slots.get_mut(id.index()) {
            e.label = Some(LogTex {
                label: label.into(),
                w,
                h,
            });
        }
    }

    pub fn set_label(&mut self, id: TexId, label: &str, w: u32, h: u32) {
        self.label_entry(id, label, w, h);
        self.res_cmd(ResCmd::SetLabel {
            id,
            label: label.into(),
            w,
            h,
        });
    }
}

// Core's only access to the renderer (ARCH_A §2.2; like `host::Host`), used at the sync points: Present (frame end,
// Device_ClearPresentTwice in calc), the draw-log sample. Implemented by `gfx::Gfx` (app, tools) and `NullRenderer`
// (core tests).
pub trait Renderer {
    // Executes everything recorded so far in call order (a recorded `DrawCmd::Present` presents) and drains `rec`.
    fn submit(&mut self, rec: &mut Recorder);
    // Debug draw log (TH08_DRAWLOG): the executor records `LogRec`s from here on.
    fn enable_log(&mut self);
    // Submits `rec`, then hands over the records executed since the last call.
    fn take_log(&mut self, rec: &mut Recorder) -> Vec<LogRec>;
    // Port-only: Japanese font file for `ResCmd::Text` (`CoreConfig::font`, TH08_FONT), tried before the search list.
    fn set_font(&mut self, font: Option<&str>);
    // App / tools: back to the concrete renderer (resize, surface attach, screenshot readback).
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

// Renderer that executes nothing: drains the lists, logs no records. Core tests and headless logic runs (logic never
// reads the renderer, ARCH decision 2).
#[derive(Clone, Copy, Debug, Default)]
pub struct NullRenderer;

impl Renderer for NullRenderer {
    fn submit(&mut self, rec: &mut Recorder) {
        rec.take();
    }
    fn enable_log(&mut self) {}
    fn take_log(&mut self, rec: &mut Recorder) -> Vec<LogRec> {
        rec.take();
        Vec::new()
    }
    fn set_font(&mut self, _: Option<&str>) {}
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // STEP10_CRATES §2 per-file table: the files that become th08-core (10c) and the rest of src/ (app, render,
    // audio, tools). Every src/*.rs is in exactly one list.
    const CORE: &[&str] = &[
        "anm",
        "anmdebug",
        "anmmgr",
        "anmvm",
        "archive",
        "ascii",
        "assets",
        "bomb",
        "bullet",
        "camera",
        "chain",
        "config",
        "content",
        "coreconfig",
        "d3dx",
        "drawfmt",
        "drawlist",
        "ecl",
        "eclvm",
        "effect",
        "ending",
        "enemy",
        "fmath",
        "game",
        "gamemgr",
        "gui",
        "host",
        "hud",
        "input",
        "item",
        "menu",
        "menulist",
        "msg",
        "musicroom",
        "player",
        "pshot",
        "replay",
        "result",
        "rng",
        "scoredat",
        "sht",
        "sound",
        "stage",
        "stdfile",
        "supervisor",
        "task",
        "testdata",
        "trace",
        "world",
        "worldclone",
    ];
    const NOT_CORE: &[&str] = &[
        "app",
        "audio",
        "demosync",
        "drawlog",
        "drawreport",
        "gfx",
        "joystick",
        "launcher",
        "lib",
        "main",
        "midi",
        "pacer",
        "platform",
        "session",
        "settings",
        "sources",
        "text",
        "xz",
    ];
    // Never named by a core file, tests included (core's dev-deps stay empty, §1).
    const NOT_CORE_MODULES: &[&str] = &[
        "gfx",
        "text",
        "audio",
        "midi",
        "app",
        "launcher",
        "settings",
        "platform",
        "sources",
        "joystick",
        "pacer",
        "session",
        "demosync",
        "drawlog",
        "drawreport",
        "xz",
    ];
    const NOT_CORE_CRATES: &[&str] = &[
        "wgpu",
        "cpal",
        "winit",
        "egui",
        "egui_wgpu",
        "egui_winit",
        "rfd",
        "clap",
        "pollster",
        "web_time",
        "web_sys",
        "js_sys",
        "wasm_bindgen",
        "wasm_bindgen_futures",
        "liblzma",
        "sha2",
        "rustysynth",
        "hound",
        "gilrs",
        "directories",
        "chrono",
        "ab_glyph",
        "env_logger",
    ];
    // Not outside `#[cfg(test)]` items: file system, environment, threads, clocks (ARCH decisions 3 / 6: `Host`).
    const NOT_OUTSIDE_TESTS: &[&str] = &[
        "std::fs",
        "std::env",
        "std::thread",
        "std::time",
        "Instant",
        "SystemTime",
        "std::process",
    ];

    // `pat` in `code` as a path segment start (`wgpu::…`, not `my_wgpu::`).
    fn names(code: &str, pat: &str) -> bool {
        code.match_indices(pat).any(|(i, _)| {
            let before = code[..i].chars().next_back();
            let after = code[i + pat.len()..].chars().next();
            before.is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == ':'))
                && after.is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
        })
    }

    // `text` with comments removed and string / char literal contents blanked (quotes and newlines kept), so the
    // checks and the brace depth see code only.
    fn strip(text: &str) -> String {
        let c: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        let ident = |k: usize| k > 0 && (c[k - 1].is_alphanumeric() || c[k - 1] == '_');
        while i < c.len() {
            let at = |k: usize| c.get(k).copied().unwrap_or('\0');
            if c[i] == '/' && at(i + 1) == '/' {
                while i < c.len() && c[i] != '\n' {
                    i += 1;
                }
            } else if c[i] == '/' && at(i + 1) == '*' {
                i += 2;
                while i < c.len() && !(c[i] == '*' && at(i + 1) == '/') {
                    if c[i] == '\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
                i += 2;
            } else if c[i] == 'r' && !ident(i) && (at(i + 1) == '"' || at(i + 1) == '#') {
                // Raw string r#"…"#.
                let mut k = i + 1;
                while at(k) == '#' {
                    k += 1;
                }
                if at(k) != '"' {
                    out.push(c[i]);
                    i += 1;
                    continue;
                }
                let hashes = k - i - 1;
                out.push_str("r\"");
                i = k + 1;
                while i < c.len() && !(c[i] == '"' && (1..=hashes).all(|h| at(i + h) == '#')) {
                    out.push(if c[i] == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
                out.push('"');
                i += 1 + hashes;
            } else if c[i] == '"' {
                out.push('"');
                i += 1;
                while i < c.len() && c[i] != '"' {
                    if c[i] == '\\' {
                        out.push(' ');
                        i += 1;
                    }
                    out.push(if at(i) == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
                out.push('"');
                i += 1;
            } else if c[i] == '\'' && (at(i + 2) == '\'' || at(i + 1) == '\\') {
                // Char literal ('x', '\n', '\''); a lifetime has no closing quote.
                let end = (i + 2..c.len().min(i + 12)).find(|&k| c[k] == '\'' && k > i + 1);
                match end {
                    Some(e) => {
                        out.push_str("' '");
                        i = e + 1;
                    }
                    None => {
                        out.push(c[i]);
                        i += 1;
                    }
                }
            } else {
                out.push(c[i]);
                i += 1;
            }
        }
        out
    }

    fn depth(code: &str) -> i32 {
        code.chars()
            .map(|c| match c {
                '{' => 1,
                '}' => -1,
                _ => 0,
            })
            .sum()
    }

    // Boundary violations of one core file: (line, text, rule). Items under `#[cfg(test)]` / `#[cfg(all(test, …))]`
    // may use the file system etc.; nothing may name the non-core modules or crates or use `unsafe`.
    fn violations(text: &str) -> Vec<(usize, String, &'static str)> {
        let mut bad = Vec::new();
        let (mut test_attr, mut test_depth) = (false, None::<i32>);
        let code = strip(text);
        for (i, (c, line)) in code.lines().zip(text.lines()).enumerate() {
            let t = c.trim();
            let in_test = test_depth.is_some() || test_attr;
            if test_depth.is_none()
                && t.starts_with("#[cfg(")
                && names(t, "test")
                && !t.contains("not(test")
            {
                test_attr = true;
            } else if test_attr && !t.starts_with("#[") && !t.is_empty() {
                // The item after the attribute: a block runs to its closing brace, else one line.
                test_attr = false;
                let d = depth(c);
                if d > 0 {
                    test_depth = Some(d);
                }
            } else if let Some(d) = &mut test_depth {
                *d += depth(c);
                if *d <= 0 {
                    test_depth = None;
                }
            }
            let mut rule = |r: &'static str| bad.push((i + 1, line.trim().to_string(), r));
            for m in NOT_CORE_MODULES {
                let group = c.split("crate::{").skip(1).any(|g| {
                    g.split('}')
                        .next()
                        .unwrap_or("")
                        .split(',')
                        .any(|n| n.trim().split("::").next() == Some(m))
                });
                if names(c, &format!("crate::{m}")) || group {
                    rule("non-core module");
                }
            }
            if NOT_CORE_CRATES
                .iter()
                .any(|k| names(c, &format!("{k}::")) || (names(c, k) && t.starts_with("use ")))
            {
                rule("non-core crate");
            }
            if names(c, "unsafe") {
                rule("unsafe");
            }
            if !in_test && NOT_OUTSIDE_TESTS.iter().any(|p| names(c, p)) {
                rule("platform access outside tests");
            }
        }
        assert!(test_depth.is_none(), "unbalanced braces in a test item");
        bad
    }

    // STEP10_CRATES 10b gate (replaces step 8c's renderer check; deleted in 10c, where the crate graph enforces
    // it): every core-bound file is free of non-core modules, crates, platform access and `unsafe`.
    #[test]
    fn core_boundary() {
        let probe = "use crate::{host, settings};\nfn f() { std::fs::read(\"x//\"); }\n\
                     #[cfg(test)]\nmod t {\n    fn g() { std::fs::read(\"{\"); let _ = '}'; }\n}\n\
                     fn h() { unsafe {} }\n// crate::gfx in a comment\n";
        let rules: Vec<_> = violations(probe).into_iter().map(|v| (v.0, v.2)).collect();
        assert_eq!(
            rules,
            [
                (1, "non-core module"),
                (2, "platform access outside tests"),
                (7, "unsafe")
            ]
        );
        let dir = crate::testdata::repo_root().join("src");
        let mut bad = Vec::new();
        let mut files = Vec::new();
        for e in std::fs::read_dir(&dir).unwrap() {
            let path = e.unwrap().path();
            if path.extension().is_some_and(|x| x == "rs") {
                files.push(path.file_stem().unwrap().to_string_lossy().into_owned());
            }
        }
        files.sort();
        for f in &files {
            let (c, n) = (CORE.contains(&f.as_str()), NOT_CORE.contains(&f.as_str()));
            assert!(
                c != n,
                "src/{f}.rs must be in exactly one of CORE / NOT_CORE"
            );
            if !c {
                continue;
            }
            let text = std::fs::read_to_string(dir.join(format!("{f}.rs"))).unwrap();
            for (l, t, r) in violations(&text) {
                bad.push(format!("{f}.rs:{l}: {r}: {t}"));
            }
        }
        assert!(
            CORE.iter()
                .chain(NOT_CORE)
                .all(|f| files.iter().any(|x| x == f)),
            "a listed file is missing from src/"
        );
        assert!(bad.is_empty(), "core boundary:\n{}", bad.join("\n"));
    }

    // Same id order as the device table: white = 0, then push, freed slots reused last-freed first.
    #[test]
    fn tex_ids_reuse_last_freed() {
        let mut r = Recorder::new();
        let a = r.create_texture(2, 2, &[0; 16]);
        let b = r.create_texture(2, 2, &[0; 16]);
        assert_eq!((a, b), (TexId(1), TexId(2)));
        r.free_texture(a);
        r.free_texture(b);
        r.free_texture(b);
        r.free_texture(TexId::WHITE);
        assert_eq!(r.create_texture(1, 1, &[0; 4]), TexId(2));
        assert_eq!(r.capture_screen("x"), TexId(1));
        assert_eq!(r.texture(TexId(1)).width, 640);
        assert_eq!(r.create_texture(1, 1, &[0; 4]), TexId(3));
    }

    // Resource commands remember their position among the draws.
    #[test]
    fn res_sequence_points() {
        let mut r = Recorder::new();
        r.rect(0.0, 0.0, 1.0, 1.0, [0; 4]);
        let t = r.create_texture(1, 1, &[0; 4]);
        r.image(t, 0.0, 0.0);
        let (d, res) = r.take();
        assert_eq!(d.len(), 2);
        assert_eq!(
            res.iter().map(|(at, _)| *at).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }
}
