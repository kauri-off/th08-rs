//! Core-side draw output: `DrawCmd` / `ResCmd` lists, core-owned `TexId`s and the device-state mirror (ARCH_A §2.2,
//! REWRITE_PLAN step 8; graphics 12 §1, §3). No wgpu type here; `gfx::Gfx` executes the lists.

use std::sync::Arc;

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

// One device draw / state call, 1:1 with the `Gfx` method of the same name (05 §9, §12; 06 §10; 12 §1).
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
}

// Per-frame command recorder with `Gfx`'s method names. `res` entries carry the `draws` length at record time, so
// the executor replays both lists in exact call order (log order of K records; GPU order is unchanged either way).
#[derive(Debug)]
pub struct Recorder {
    pub draws: Vec<DrawCmd>,
    pub res: Vec<(usize, ResCmd)>,
    pub state: DeviceState,
    pub table: ResTable,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl AsMut<Recorder> for Recorder {
    fn as_mut(&mut self) -> &mut Recorder {
        self
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
        self.draws.push(c);
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
