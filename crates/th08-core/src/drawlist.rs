//! Core-side draw output (05 §12): `DrawCmd` / `ResCmd` lists, core-owned `TexId`s and the device-state mirror.

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

// Where a texture's pixels came from, kept for a device-loss rebuild.
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

// Slot allocator for `TexId`s: push, or pop the last freed slot.
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

// One device draw / state call, 1:1 with the `Recorder` method of the same name.
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
    // Standalone .jpg / .png file bytes (03 §1); the renderer decodes them to RGBA8 of the header's `w` × `h`.
    // `fit`: a presentation-pack override, scaled to `w` × `h` when its size differs (the logical size and
    // every UV stay the vanilla ones).
    CreateImage {
        id: TexId,
        bytes: Arc<[u8]>,
        w: u32,
        h: u32,
        fit: bool,
    },
    // ANM entry texture with a pack image over its vanilla THTX pixels `base` (I18N §2.4): per sprite rect (THTX
    // pixels), an override rect without one non-transparent pixel keeps the vanilla rect, as thcrap's patcher does.
    CreateAnmOver {
        id: TexId,
        w: u32,
        h: u32,
        base: Arc<[u8]>,
        rects: Arc<[[u32; 4]]>,
        bytes: Arc<[u8]>,
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

// Text_RenderToVm 0x4663B0 family (10 §6) as an output command: the renderer decodes and
// rasterises `text` and replaces rows `y .. y+size` of `dst`, `w` texels wide (x = 0), at this point in call order.
#[derive(Clone, Debug)]
pub struct TextCmd {
    pub dst: TexId,
    pub y: u32,
    // Texture-row width W and font size (strip height) in texels.
    pub w: u32,
    pub size: u32,
    // Vanilla Shift-JIS bytes, rendered as is when no translation of `key` is mounted.
    pub text: SjisString,
    pub key: crate::textref::TextRef,
    pub color: [u8; 3],
    // Pen start x in strip pixels.
    pub x: f32,
    // Right edge of the VM's sprite in strip pixels (u0·W + sprite width): what is visible of the strip.
    pub edge: f32,
    // −1 → ±1 px outline offsets (2× space), anything else ±2 px.
    pub outline: i32,
    // How the original placed the text (10 §6 entry points). The vanilla path always draws at `x` (the original's
    // byte-length formula); a translation is placed by its measured width.
    pub align: Align,
    // Which recorder made it: the presentation pack picks the overflow mode per site.
    pub site: TextSite,
}

// Placement of a `TextCmd` in strip pixels: Text_RenderToVm at `x`, Text_RenderCentered around a centre,
// Text_RenderToVmRight against a right edge (10 §6).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Left,
    Centre(f32),
    Right(f32),
}

// The six text recorders; the key of a pack's `[targets]` overflow table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextSite {
    // Dialogue strips (10 §6, hud.rs).
    Dialogue,
    // Spell / player-spell plate (gui.rs).
    Spell,
    // Ending text (16 §1.3).
    Ending,
    // Music room title and comments (16 §5.1).
    MusicRoom,
    // Spell practice / result / replay lists (16 §6.3, §6.5).
    MenuList,
    // Menu descriptions (12 §2.2).
    MenuDesc,
}

impl TextSite {
    pub const ALL: [TextSite; 6] = [
        TextSite::Dialogue,
        TextSite::Spell,
        TextSite::Ending,
        TextSite::MusicRoom,
        TextSite::MenuList,
        TextSite::MenuDesc,
    ];

    // Name in a pack's `[targets]` table.
    pub fn name(self) -> &'static str {
        match self {
            TextSite::Dialogue => "dialogue",
            TextSite::Spell => "spell",
            TextSite::Ending => "ending",
            TextSite::MusicRoom => "music_room",
            TextSite::MenuList => "menu_list",
            TextSite::MenuDesc => "menu_desc",
        }
    }
}

// AnmManager's D3D render-state mirrors: blend +0x24C0 (05 §12.5), bound texture +0x24BC and texture-matrix sprite
// +0x24C8 (05 §12.8). Only draw paths read them, so they are emit state: with emit off they keep
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

// Per-frame command recorder. `res` entries carry the `draws` length at record time, so
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

    // Logical size of a live texture; panics on a freed id.
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

    // ANM entry texture with a pack image: same provenance and THTX size, pixels decoded, placed and merged with
    // `base` per sprite rect by the renderer.
    #[expect(
        clippy::too_many_arguments,
        reason = "provenance, size, vanilla pixels, rects, image"
    )]
    pub fn create_anm_texture_over(
        &mut self,
        file: &str,
        entry: u32,
        width: u32,
        height: u32,
        base: &[u8],
        rects: Vec<[u32; 4]>,
        image: Arc<[u8]>,
    ) -> TexId {
        let id = self.table.alloc(TexEntry {
            width,
            height,
            label: None,
            src: TexSource::Anm {
                file: file.into(),
                entry,
            },
        });
        self.res_cmd(ResCmd::CreateAnmOver {
            id,
            w: width,
            h: height,
            base: Arc::from(base),
            rects: Arc::from(rects),
            bytes: image,
        });
        id
    }

    #[expect(clippy::cast_sign_loss, reason = "640.0 × 480.0 screen constants")]
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

    // Standalone .jpg / .png images (03 §1): size from the header, pixels decoded by the renderer; `name` is the
    // draw-log label.
    pub fn load_image(&mut self, name: &str, bytes: &[u8]) -> anyhow::Result<TexId> {
        self.load_image_over(name, bytes, None)
    }

    // `load_image` with a pack override (`PresentView::image`): the size still comes from the vanilla `bytes`.
    pub fn load_image_over(
        &mut self,
        name: &str,
        bytes: &[u8],
        over: Option<Arc<[u8]>>,
    ) -> anyhow::Result<TexId> {
        let (w, h) = th08_formats::img::dims(bytes)?;
        let id = self.table.alloc(TexEntry {
            width: w,
            height: h,
            label: None,
            src: TexSource::Image { name: name.into() },
        });
        let fit = over.is_some();
        self.res_cmd(ResCmd::CreateImage {
            id,
            bytes: over.unwrap_or_else(|| Arc::from(bytes)),
            w,
            h,
            fit,
        });
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

// Core's only access to the renderer (like `host::Host`), used at the sync points: Present (frame end,
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
// reads the renderer).
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
