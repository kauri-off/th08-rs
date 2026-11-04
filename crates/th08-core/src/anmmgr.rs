//! ANM manager: file slots, script start, DrawByMode, EmitQuad, tint, Draw3DWorld (03 §2.4; 05 §8–§9, §12; 06 §11.2).

use crate::anmvm::{self, AnmVm, VmFlags};
use crate::content::LogicView;
use crate::types::D3dColor;
use th08_formats::anm::{self, Script};
// Draws and texture uploads go to the core `Recorder`.
use crate::camera::{Camera, Mat4};
use crate::drawlist::{self, Blend, Recorder, TexId, Vertex};
use anyhow::{Result, ensure};
use std::sync::Arc;

// ANM file slots 0..24; index ≥ 25 is an error.
pub const SLOTS: usize = 25;

// Low word of a slot / script number, the original's i16 fields vm+0x204 / vm+0x21A.
fn lo16_index(v: usize) -> i16 {
    (v as u16).cast_signed()
}

// Runtime sprite record, reduced to texture, normalised UV rect (u0, v0, u1, v1) and logical size.
#[derive(Clone, Copy, Debug)]
pub struct SlotSprite {
    pub tex: TexId,
    pub uv: [f32; 4],
    pub w: f32,
    pub h: f32,
}

// Loaded-file record at AnmManager + 0x24 + slot × 0x1C: cumulative sprite and script arrays.
// Clone shares the parsed scripts (`Arc`, copied on the first literal-destination write, 05 §3); `textures` are
// handles only, so a clone's unload frees the renderer textures of both copies.
#[derive(Clone)]
pub struct LoadedAnm {
    pub name: String,
    pub sprites: Vec<SlotSprite>,
    pub scripts: Vec<Arc<Script>>,
    textures: Vec<TexId>,
}

#[derive(Clone)]
pub struct AnmManager {
    slots: [Option<LoadedAnm>; SLOTS],
    // Global game speed used by the VM timers, 1.0 in normal play.
    pub speed: f32,
    // Supervisor flag 0x20 (Supervisor+0x1A4): set by ECL native 29, never cleared (the Supervisor survives a
    // restart, 00 §2); every Timer_Sub reads it next to `speed`.
    pub slowdown_restore: bool,
    pub vm_counter: u32,
    // Global screen offset AnmManager +0x1C/+0x20, written by the screen-shake effect.
    pub screen_offset: [f32; 2],
    // Global tint D3DCOLOR at AnmManager+0, enabled by +4; 0x80 per channel is neutral.
    pub tint: u32,
    pub tint_on: bool,
    // Stage tint request +0x6468 (06 §10.2 step 3), applied at the next Stage_DrawLow.
    pub stage_tint: Option<u32>,
    // Stage flag +0x646C "a bomb drew this frame" (14 §1.3).
    pub bomb_drew: bool,
    // Camera used by VMs with flag bit 15.
    pub stage_camera: Option<Camera>,
    // 0x4EA3F4 = g_Stage+0x63C4: normalised camera-right vector written by Stage_SetCamera every frame.
    pub cam_right: [f32; 3],
    // The blend / texture / texture-matrix caches +0x24C0 / +0x24BC / +0x24C8 are emit state: `Recorder::anm`.
    // Port-only: texture entries loaded so far; paces the loading screen (gamemgr `loading_total`).
    pub entries_loaded: u32,
    // Pending AnmManager_QueueCapture request +0x2A252C (12 §3): the destination sprite rect at queue time.
    pub capture_req: Option<SlotSprite>,
}

// A global tint D3DCOLOR a draw-phase step drew under and turned off (or replaced) before its emit runs;
// the emit applies it to VM copies. Each channel × tint >> 7, alpha included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tint(pub u32);

impl Tint {
    // RGBA colour under the tint.
    pub fn rgba(self, mut rgba: [u8; 4]) -> [u8; 4] {
        let t = D3dColor(self.0);
        let tc = [t.r(), t.g(), t.b(), t.a()];
        for k in 0..4 {
            rgba[k] = ((u32::from(rgba[k]) * u32::from(tc[k])) >> 7).min(255) as u8;
        }
        rgba
    }

    // A lone vertex alpha (ring strip).
    pub fn alpha(self, a: u8) -> u8 {
        self.rgba([0, 0, 0, a])[3]
    }

    // A copy that draws under this tint while the global tint is off: EmitQuad's colour (colour2 with bit 17, else
    // colour), tinted, goes into colour2 + bit 17, so the alpha-0 skip still tests the untinted primary colour.
    pub fn vm(self, vm: &AnmVm) -> AnmVm {
        use anmvm::*;
        let c = if vm.has(VmFlags::COLOR2) {
            vm.color2
        } else {
            vm.color
        };
        let [r, g, b, a] = self.rgba([c[R], c[G], c[B], c[A]]);
        let mut v = vm.clone();
        (v.color2[R], v.color2[G], v.color2[B], v.color2[A]) = (r, g, b, a);
        v.insert(VmFlags::COLOR2);
        v
    }
}

// Linear vertex fog (06 §10.1): f = clamp((far − z) / (far − near), 0, 1) on eye-space z (no range fog); RGB bytes.
#[derive(Clone, Copy, Debug)]
pub struct Fog {
    pub near: f32,
    pub far: f32,
    pub color: [u8; 3],
}

impl Default for AnmManager {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            speed: 1.0,
            slowdown_restore: false,
            vm_counter: 0,
            screen_offset: [0.0; 2],
            tint: 0x8080_8080,
            tint_on: false,
            stage_tint: None,
            bomb_drew: false,
            stage_camera: None,
            cam_right: [0.0; 3],
            entries_loaded: 0,
            capture_req: None,
        }
    }
}

impl AnmManager {
    // Anm_Preload 0x465860 + Anm_LoadEntryTexture 0x465CC0; the port uploads all entries at once.
    // Sprites / scripts come from the LogicView, THTX pixels from its PresentView: core keeps no pixels,
    // the recorder gets them with the create command and `TexSource::Anm { file, entry }` as provenance.
    pub fn load(
        &mut self,
        rec: &mut Recorder,
        archive: &LogicView,
        slot: usize,
        name: &str,
    ) -> Result<()> {
        ensure!(slot < SLOTS, "anm slot {slot} out of range");
        let chunks = anm::parse(&archive.read(name)?)?;
        // Decoded before the unload so a bad THTX still fails the load with the slot untouched.
        let mut pixels = anm::textures(&archive.present().read(name)?)?;
        ensure!(
            pixels.len() == chunks.len(),
            "{name}: presentation entries differ"
        );
        self.unload(rec, slot);
        let mut entry = 0;
        let present = archive.present();
        self.insert(slot, name, chunks, |c| {
            let id = match (pixels[entry as usize].take(), present.chunk_image(&c.name)) {
                // Pack image by chunk name: THTX size kept, pixels replaced.
                (Some(t), Some(img)) => {
                    rec.create_anm_texture_over(name, entry, t.width, t.height, img)
                }
                (Some(t), None) => rec.create_anm_texture(name, entry, t.width, t.height, &t.rgba),
                // '@' entries are blank render targets (text / capture).
                (None, _) => rec.create_anm_texture(
                    name,
                    entry,
                    c.width.max(1),
                    c.height.max(1),
                    &vec![0; (c.width.max(1) * c.height.max(1) * 4) as usize],
                ),
            };
            rec.set_label(id, &format!("{name}#{entry}"), c.width, c.height);
            entry += 1;
            id
        });
        self.entries_loaded += entry;
        Ok(())
    }

    // Concatenates every entry's sprites and scripts into the slot's cumulative arrays.
    pub fn insert(
        &mut self,
        slot: usize,
        name: &str,
        chunks: Vec<anm::Chunk>,
        mut make_tex: impl FnMut(&anm::Chunk) -> TexId,
    ) {
        let (mut sprites, mut scripts, mut textures) = (Vec::new(), Vec::new(), Vec::new());
        for c in chunks {
            let tex = make_tex(&c);
            textures.push(tex);
            let (tw, th) = (c.width.max(1) as f32, c.height.max(1) as f32);
            sprites.extend(c.sprites.iter().map(|s| SlotSprite {
                tex,
                // Normalised by the header's logical size, not the THTX size (03 §2.2).
                uv: [s.x / tw, s.y / th, (s.x + s.w) / tw, (s.y + s.h) / th],
                w: s.w,
                h: s.h,
            }));
            scripts.extend(c.scripts.into_iter().map(Arc::new));
        }
        self.slots[slot] = Some(LoadedAnm {
            name: name.to_string(),
            sprites,
            scripts,
            textures,
        });
    }

    pub fn unload(&mut self, rec: &mut Recorder, slot: usize) {
        if let Some(a) = self.slots.get_mut(slot).and_then(Option::take) {
            a.textures.into_iter().for_each(|t| rec.free_texture(t));
        }
    }

    pub fn get(&self, slot: usize) -> Option<&LoadedAnm> {
        self.slots.get(slot)?.as_ref()
    }

    pub fn sprite(&self, slot: usize, index: usize) -> Option<&SlotSprite> {
        self.get(slot)?.sprites.get(index)
    }

    pub fn script(&self, slot: usize, index: usize) -> Option<&Script> {
        self.get(slot)?.scripts.get(index).map(|s| &**s)
    }

    pub fn script_mut(&mut self, slot: usize, index: usize) -> Option<&mut Script> {
        self.slots
            .get_mut(slot)?
            .as_mut()?
            .scripts
            .get_mut(index)
            .map(Arc::make_mut)
    }

    // Statistics counter (05 §4).
    pub fn begin_frame(&mut self) {
        self.vm_counter = 0;
    }

    // AnmManager_SetScript 0x40B910: pos and pos_offset are zeroed before the frame-0 run.
    pub fn set_script(
        &mut self,
        rng: &mut crate::rng::Rng,
        vm: &mut AnmVm,
        slot: usize,
        index: usize,
    ) {
        vm.script_index = lo16_index(index);
        vm.pos = [0.0; 3];
        vm.pos_offset = [0.0; 3];
        self.start_script(rng, vm, slot, index);
    }

    // AnmFile_SetScript 0x4069F0 (09 §4.5): like `set_script` but keeps the VM pos / pos offset.
    pub fn file_set_script(
        &mut self,
        rng: &mut crate::rng::Rng,
        vm: &mut AnmVm,
        slot: usize,
        index: usize,
    ) {
        vm.script_index = lo16_index(index);
        self.start_script(rng, vm, slot, index);
    }

    // AnmFile_StartScript 0x45E580: a missing script zeroes the whole VM; otherwise init and run once (frame 0).
    pub fn start_script(
        &mut self,
        rng: &mut crate::rng::Rng,
        vm: &mut AnmVm,
        slot: usize,
        index: usize,
    ) {
        if self.script(slot, index).is_none() {
            *vm = AnmVm {
                script_index: 0,
                ..AnmVm::default()
            };
            return;
        }
        vm.init();
        vm.anm_slot = lo16_index(slot);
        vm.remove(VmFlags::FLIP_X | VmFlags::FLIP_Y | VmFlags::VISIBLE);
        vm.script = Some((slot, index));
        vm.ip = Some(anm::AnmOff(0));
        vm.time.set(0);
        vm.run(self, rng);
    }

    // Draw = `prepare` (draw-phase step) + `emit_mode_z`.
    pub fn draw(&self, rec: &mut Recorder, vm: &mut AnmVm) {
        self.draw_mode(rec, vm, vm.draw_mode);
    }

    // 2D draws carry the VM's z as the vertex depth.
    pub fn draw_mode(&self, rec: &mut Recorder, vm: &mut AnmVm, mode: i32) {
        let z = vm.pos[2];
        self.draw_mode_z(rec, vm, mode, z);
    }

    pub fn emit(&self, rec: &mut Recorder, vm: &AnmVm) {
        self.emit_mode_z(rec, vm, vm.draw_mode, vm.pos[2]);
    }

    pub fn emit_mode(&self, rec: &mut Recorder, vm: &AnmVm, mode: i32) {
        self.emit_mode_z(rec, vm, mode, vm.pos[2]);
    }

    // The VM write of a draw (draw-phase step): DrawByMode mode 5 and Draw3DWorld rebuild the cached world matrix
    // behind the same visibility / sprite guards; every other mode writes nothing.
    pub fn prepare(&self, vm: &mut AnmVm, mode: i32) {
        if mode == 5 && self.drawable(vm).is_some() {
            Self::update_world_matrix(vm);
        }
    }

    // Visible, enabled, primary alpha ≠ 0, has a sprite, and the sprite exists.
    fn drawable(&self, vm: &AnmVm) -> Option<SlotSprite> {
        use anmvm::*;
        if !vm.vm_flags().contains(VmFlags::VISIBLE | VmFlags::ENABLED)
            || vm.color[A] == 0
            || !vm.has_sprite
        {
            return None;
        }
        self.sprite(
            table_index(i32::from(vm.anm_slot)),
            table_index(i32::from(vm.sprite_index)),
        )
        .copied()
    }

    // Mode-5 world matrix: base (w/256, h/256) × scale, then × RotX × RotY × RotZ (each only if ≠ 0).
    fn update_world_matrix(vm: &mut AnmVm) {
        use anmvm::*;
        if vm.has(VmFlags::ROT_DIRTY | VmFlags::SCALE_DIRTY) && !vm.has(VmFlags::FREEZE_MATRIX) {
            let mut m = Mat4::scale(
                vm.sprite_w / 256.0 * vm.scale[0],
                vm.sprite_h / 256.0 * vm.scale[1],
                1.0,
            );
            let [rx, ry, rz] = vm.rotation;
            if rx != 0.0 {
                m = m.mul(&Mat4::rotation_x(rx));
            }
            if ry != 0.0 {
                m = m.mul(&Mat4::rotation_y(ry));
            }
            if rz != 0.0 {
                m = m.mul(&Mat4::rotation_z(rz));
            }
            vm.world_matrix = m;
            vm.remove(VmFlags::ROT_DIRTY | VmFlags::SCALE_DIRTY);
        }
    }

    // Colour or colour2 (bit 17), then the global tint; returns RGBA.
    fn vm_color(&self, vm: &AnmVm) -> [u8; 4] {
        use anmvm::*;
        let c = if vm.has(VmFlags::COLOR2) {
            vm.color2
        } else {
            vm.color
        };
        self.tinted([c[R], c[G], c[B], c[A]])
    }

    // AnmMgr_TintOff 0x40BAB0 (06 §10.2): +4 := 0, colour 0x80808080.
    pub fn tint_off(&mut self) {
        (self.tint, self.tint_on) = (0x8080_8080, false);
    }

    // Stage_RequestTint 0x409160 (06 §10.2 step 3): a second request in the same frame averages each channel.
    pub fn request_tint(&mut self, c: u32) {
        self.stage_tint = Some(match self.stage_tint {
            Some(o) => {
                let (a, b) = (o.to_le_bytes(), c.to_le_bytes());
                u32::from_le_bytes(std::array::from_fn(|k| {
                    ((u32::from(a[k]) + u32::from(b[k])) / 2) as u8
                }))
            }
            None => c,
        });
    }

    // Global tint: D3DCOLOR at AnmManager+0, enabled by +4; every channel including alpha.
    pub fn tinted(&self, rgba: [u8; 4]) -> [u8; 4] {
        match self.tint_state() {
            Some(t) => t.rgba(rgba),
            None => rgba,
        }
    }

    // The global tint +0 while +4 is on, else None.
    pub fn tint_state(&self) -> Option<Tint> {
        self.tint_on.then_some(Tint(self.tint))
    }

    // Emit of `vm` under the tint a draw-phase step saw before it turned the global tint off (None: as is).
    pub fn emit_tinted(&self, rec: &mut Recorder, vm: &AnmVm, mode: i32, z: f32, t: Option<Tint>) {
        match t {
            Some(t) => {
                debug_assert!(!self.tint_on, "a saved tint applied under the global tint");
                self.emit_mode_z(rec, &t.vm(vm), mode, z);
            }
            None => self.emit_mode_z(rec, vm, mode, z),
        }
    }

    // Blend bits 4–5: 0 alpha, 1 additive, 2/3 set nothing.
    pub fn blend_for(rec: &mut Recorder, vm: &AnmVm) -> Blend {
        let blend = match vm.blend() {
            0 => Blend::Alpha,
            1 => Blend::Add,
            _ => rec.anm.last_blend,
        };
        rec.anm.last_blend = blend;
        blend
    }

    // AnmManager_Draw3DWorld 0x464470: unit quad ±128 through `cam` with vertex fog; the screen-shake
    // offset is added to the translation.
    pub fn draw_3d_world(
        &self,
        rec: &mut Recorder,
        vm: &mut AnmVm,
        cam: &Camera,
        fog: Option<Fog>,
    ) {
        self.prepare(vm, 5);
        self.emit_3d_world(rec, vm, cam, fog);
    }

    pub fn emit_3d_world(&self, rec: &mut Recorder, vm: &AnmVm, cam: &Camera, fog: Option<Fog>) {
        use anmvm::*;
        let Some(spr) = self.drawable(vm) else {
            return;
        };
        let (hw, hh) = (
            (vm.sprite_w * vm.scale[0] / 2.0).abs(),
            (vm.sprite_h * vm.scale[1] / 2.0).abs(),
        );
        let ax = if vm.has(VmFlags::ANCHOR_X) { hw } else { 0.0 };
        let ay = if vm.has(VmFlags::ANCHOR_Y) { hh } else { 0.0 };
        let mut w = vm.world_matrix;
        w.0[3] = [
            vm.pos[0] + ax + self.screen_offset[0],
            vm.pos[1] + ay + self.screen_offset[1],
            vm.pos[2],
            1.0,
        ];
        // Texture matrix offset is re-uploaded only for a new sprite or a non-zero u offset (v-only scroll quirk).
        let key = (vm.anm_slot, vm.sprite_index);
        // The texture is re-bound only on the same cache miss, so a 2D draw in between can leave another bound.
        let (off, tex) = match (rec.anm.last_tex_matrix, rec.anm.bound_tex) {
            (Some((k, o)), Some(t)) if k == key && vm.uv_off[0] == 0.0 => (o, t),
            _ => (
                [spr.uv[0] + vm.uv_off[0], spr.uv[1] + vm.uv_off[1]],
                spr.tex,
            ),
        };
        rec.anm.bound_tex = Some(tex);
        rec.anm.last_tex_matrix = Some((key, off));
        let (su, sv) = (spr.uv[2] - spr.uv[0], spr.uv[3] - spr.uv[1]);
        let color = self.vm_color(vm);
        let vp = rec.viewport();
        let vproj = cam.view_proj();
        let v = [
            [-128.0, -128.0, 0.0, 0.0],
            [128.0, -128.0, 1.0, 0.0],
            [-128.0, 128.0, 0.0, 1.0],
            [128.0, 128.0, 1.0, 1.0],
        ]
        .map(|[x, y, u, t]| {
            let p = w.transform([x, y, 0.0]);
            let p = [p[0], p[1], p[2]];
            let (fog_color, f) = match fog {
                Some(fg) => {
                    let z = cam.view.transform(p)[2];
                    let [r, g, b] = fg.color;
                    (
                        [r, g, b, 255],
                        ((fg.far - z) / (fg.far - fg.near)).clamp(0.0, 1.0),
                    )
                }
                None => ([0; 4], 1.0),
            };
            Vertex {
                pos: drawlist::clip_in_viewport(vproj.transform(p), vp),
                uv: [off[0] + u * su, off[1] + t * sv],
                color,
                fog_color,
                fog: f,
            }
        });
        let blend = Self::blend_for(rec, vm);
        rec.set_log_fog(fog.map(|f| drawlist::LogFog {
            color: f.color,
            start: f.near,
            end: f.far,
        }));
        rec.strip(tex, blend, !vm.has(VmFlags::ZWRITE_OFF), v);
        rec.set_log_fog(None);
    }

    // Anm_DrawRingStrip (07 §10.6 trail, flag 8): one triangle strip of (pos, uv, alpha) vertices with the VM's
    // texture, blend and colour, emitted as consecutive 4-vertex strips.
    pub fn draw_ring_strip(&self, rec: &mut Recorder, vm: &AnmVm, v: &[([f32; 2], [f32; 2], u8)]) {
        use anmvm::*;
        let Some(&spr) = self.sprite(
            table_index(i32::from(vm.anm_slot)),
            table_index(i32::from(vm.sprite_index)),
        ) else {
            return;
        };
        let c = if vm.has(VmFlags::COLOR2) {
            vm.color2
        } else {
            vm.color
        };
        let blend = Self::blend_for(rec, vm);
        let off = self.screen_offset;
        let vert = |&([x, y], uv, a): &([f32; 2], [f32; 2], u8)| {
            let rgba = self.tinted([c[R], c[G], c[B], a]);
            Vertex::screen(x + off[0] + 0.5, y + off[1] + 0.5, 0.0, uv, rgba)
        };
        let v: Vec<Vertex> = v.iter().map(vert).collect();
        for q in (0..v.len().saturating_sub(2)).step_by(2) {
            if q + 3 < v.len() {
                rec.strip(
                    spr.tex,
                    blend,
                    !vm.has(VmFlags::ZWRITE_OFF),
                    [v[q], v[q + 1], v[q + 2], v[q + 3]],
                );
            }
        }
        rec.anm.bound_tex = Some(spr.tex);
    }

    // `z` is the vertex depth (projected depth for stage billboards, 0 otherwise).
    // AnmManager_DrawByMode 0x45E960; skip unless visible, enabled and primary alpha ≠ 0.
    pub fn draw_mode_z(&self, rec: &mut Recorder, vm: &mut AnmVm, mode: i32, z: f32) {
        self.prepare(vm, mode);
        self.emit_mode_z(rec, vm, mode, z);
    }

    // Emit half of DrawByMode: reads only; mode 5 uses the world matrix `prepare` left.
    pub fn emit_mode_z(&self, rec: &mut Recorder, vm: &AnmVm, mode: i32, z: f32) {
        self.emit_mode_z_at(rec, vm, mode, z, self.screen_offset);
    }

    // As `emit_mode_z` with the EmitQuad global offset `off` the draw saw (a step zeroed `screen_offset` since).
    pub fn emit_mode_z_at(&self, rec: &mut Recorder, vm: &AnmVm, mode: i32, z: f32, off: [f32; 2]) {
        use anmvm::*;
        let Some(spr) = self.drawable(vm) else {
            return;
        };
        let (hw, hh) = (
            vm.sprite_w * vm.scale[0] / 2.0,
            vm.sprite_h * vm.scale[1] / 2.0,
        );
        let [px, py, pz] = vm.pos;
        let ax = if vm.has(VmFlags::ANCHOR_X) { hw } else { 0.0 };
        let ay = if vm.has(VmFlags::ANCHOR_Y) { hh } else { 0.0 };
        let axis_aligned = || {
            let (x0, y0) = (px - hw + ax, py - hh + ay);
            let (x1, y1) = (px + hw + ax, py + hh + ay);
            [[x0, y0], [x1, y0], [x0, y1], [x1, y1]]
        };
        // Modes 2/3 with rotation z ≠ 0: corners rotated, the anchor shift added afterwards unrotated.
        let rotated = || {
            let (s, c) = crate::fmath::sin_cos_f(vm.rotation[2]);
            [(-hw, -hh), (hw, -hh), (-hw, hh), (hw, hh)]
                .map(|(dx, dy)| [dx * c - dy * s + px + ax, dx * s + dy * c + py + ay])
        };
        // Mode 4 sizes the quad from the projection; both 3D modes carry the projected z as vertex depth.
        let mut depth = [z; 4];
        let stage_cam = vm.has(VmFlags::STAGE_CAM);
        let cam = |anm: &Self, vp| match (stage_cam, anm.stage_camera) {
            (true, Some(c)) => c,
            _ => Camera::set_2d(vp),
        };
        let (corners, snap) = match mode {
            // 0 Draw2DSnap 0x462FF0, 1 Draw2D 0x463800, 2 Draw2DRot 0x463210 (snaps only when unrotated),
            // 3 Draw2DRotNoSnap 0x463470.
            0 => (axis_aligned(), true),
            1 => (axis_aligned(), false),
            2 | 3 if vm.rotation[2] == 0.0 => (axis_aligned(), mode == 2),
            2 | 3 => (rotated(), false),
            // 4 DrawBillboard 0x463CF0: never reached through the dispatch (05 §9 reachability), kept for completeness.
            4 => {
                let vp = rec.viewport();
                let cam = cam(self, vp);
                let s = cam.project([px, py, pz], vp);
                if !(0.0..=1.0).contains(&s[2]) {
                    return;
                }
                let r = self.cam_right;
                let len = length3(sub3(cam.project([px + r[0], py + r[1], pz + r[2]], vp), s));
                depth = [s[2]; 4];
                (billboard_corners(vm, s, len), false)
            }
            // 5 Draw3DQuad 0x464070; the anchor shift uses the absolute half size.
            5 => {
                let mut w = vm.world_matrix;
                w.0[3] = [px + ax.abs(), py + ay.abs(), pz, 1.0];
                let vp = rec.viewport();
                let cam = cam(self, vp);
                let projected = [
                    [-128.0, -128.0],
                    [128.0, -128.0],
                    [-128.0, 128.0],
                    [128.0, 128.0],
                ]
                .map(|[x, y]| {
                    let p = w.transform([x, y, 0.0]);
                    cam.project([p[0], p[1], p[2]], vp)
                });
                // EmitQuad never rewrites z: the projected z is the vertex depth (≈ 0.9994 for z = 0, 2D camera).
                depth = projected.map(|s| s[2]);
                (projected.map(|s| [s[0], s[1]]), false)
            }
            _ => return,
        };
        self.emit_quad_at(rec, vm, spr, corners, depth, snap, off);
    }

    // DrawBillboard 0x463CF0 (13 §2.6c) from an already projected point `s` and the 3D length `len` of
    // project(pos + right) − s; list B moves `s` in screen space (callback 0x428310) between the two.
    pub fn draw_billboard(&self, rec: &mut Recorder, vm: &AnmVm, s: [f32; 3], len: f32) {
        let Some(spr) = self.drawable(vm) else {
            return;
        };
        self.emit_quad(
            rec,
            vm,
            spr,
            billboard_corners(vm, s, len),
            [s[2]; 4],
            false,
        );
    }

    fn emit_quad(
        &self,
        rec: &mut Recorder,
        vm: &AnmVm,
        spr: SlotSprite,
        corners: [[f32; 2]; 4],
        depth: [f32; 4],
        snap: bool,
    ) {
        self.emit_quad_at(rec, vm, spr, corners, depth, snap, self.screen_offset);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_quad_at(
        &self,
        rec: &mut Recorder,
        vm: &AnmVm,
        spr: SlotSprite,
        corners: [[f32; 2]; 4],
        depth: [f32; 4],
        snap: bool,
        off: [f32; 2],
    ) {
        use anmvm::*;
        // AnmManager_EmitQuad 0x4628B0: global offset, optional snapping, then the D3D pixel centre (+0.5) for wgpu.
        let corners = corners.map(|[x, y]| {
            let (mut x, mut y) = (x + off[0], y + off[1]);
            // Snap: x87 round-to-nearest-even, minus 0.5.
            if snap {
                x = x.round_ties_even() - 0.5;
                y = y.round_ties_even() - 0.5;
            }
            [x + 0.5, y + 0.5]
        });
        let vp = rec.viewport();
        let (minx, maxx) = corners
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), c| (a.min(c[0]), b.max(c[0])));
        let (miny, maxy) = corners
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), c| (a.min(c[1]), b.max(c[1])));
        // Cull unless the bounding box meets the viewport, in D3D coordinates (undo the +0.5).
        let (minx, maxx, miny, maxy) = (minx - 0.5, maxx - 0.5, miny - 0.5, maxy - 0.5);
        if maxx < vp.x || minx > vp.x + vp.w || maxy < vp.y || miny > vp.y + vp.h {
            return;
        }
        // EmitQuad step 2: v0 (u0,v0), v1 (u1,v0), v2 (u0,v1), v3 (u1,v1).
        let (u0, v0) = (spr.uv[0] + vm.uv_off[0], spr.uv[1] + vm.uv_off[1]);
        let (u1, v1) = (spr.uv[2] + vm.uv_off[0], spr.uv[3] + vm.uv_off[1]);
        let rgba = self.vm_color(vm);
        let blend = Self::blend_for(rec, vm);
        let uv = [[u0, v0], [u1, v0], [u0, v1], [u1, v1]];
        let v = std::array::from_fn(|i| {
            Vertex::screen(corners[i][0], corners[i][1], depth[i], uv[i], rgba)
        });
        rec.anm.bound_tex = Some(spr.tex);
        rec.strip(spr.tex, blend, !vm.has(VmFlags::ZWRITE_OFF), v);
    }
}

// Billboard corners (13 §2.6c): half size ((len·0.5)·sprite_w)·scale.x, rotated by rotation.z about s.
fn billboard_corners(vm: &AnmVm, s: [f32; 3], len: f32) -> [[f32; 2]; 4] {
    let hw = ((len * 0.5) * vm.sprite_w) * vm.scale[0];
    let hh = ((len * 0.5) * vm.sprite_h) * vm.scale[1];
    let (sn, cs) = crate::fmath::sin_cos_f(vm.rotation[2]);
    [(-hw, -hh), (hw, -hh), (-hw, hh), (hw, hh)]
        .map(|(dx, dy)| [dx * cs - dy * sn + s[0], dx * sn + dy * cs + s[1]])
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

// Vec3_Length: the projected difference includes the depth (13 §2.6c).
pub fn length3(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    // A saved tint applied by the emit (global tint off) records what the draw under the global tint records, also
    // with colour2 and a tint alpha that zeroes a small primary alpha (the skip tests the untinted colour).
    #[test]
    fn saved_tint_equals_global_tint() {
        let mut anm = AnmManager::default();
        let chunk = th08_formats::anm::Chunk {
            name: "t".into(),
            width: 256,
            height: 256,
            sprites: vec![th08_formats::anm::Sprite {
                id: 0,
                x: 0.0,
                y: 0.0,
                w: 32.0,
                h: 16.0,
            }],
            scripts: Vec::new(),
            has_texture: false,
        };
        anm.insert(0, "t", vec![chunk], |_| crate::drawlist::TexId(0));
        let mut vm = AnmVm::default();
        vm.init();
        vm.set_sprite(&anm, 0);
        vm.pos = [100.0, 100.0, 0.0];
        for (tint, color, color2) in [
            (0xFFAB_ABAB, [255, 255, 255, 255], None),
            (0x20F0_1010, [10, 200, 30, 3], None),
            (0x8060_6060, [1, 2, 3, 4], Some([200, 100, 50, 255])),
        ] {
            vm.color = color;
            vm.flags &= !anmvm::F_COLOR2;
            if let Some(c) = color2 {
                (vm.color2, vm.flags) = (c, vm.flags | anmvm::F_COLOR2);
            }
            let mut global = Recorder::new();
            (anm.tint, anm.tint_on) = (tint, true);
            anm.emit_mode_z(&mut global, &vm, 0, 0.0);
            anm.tint_off();
            let mut saved = Recorder::new();
            anm.emit_tinted(&mut saved, &vm, 0, 0.0, Some(Tint(tint)));
            assert_eq!(global.draws.len(), 1);
            assert_eq!(format!("{:?}", global.draws), format!("{:?}", saved.draws));
        }
    }

    // AnmManager::clone mid-animation: both copies step to the same VM state, RNG and draws; a
    // literal-destination write (05 §3) copies the clone's script instead of writing into the shared one.
    #[test]
    fn clone_mid_animation_steps_identically() {
        fn ins(op: i16, time: i16, mask: u16, args: &[u32]) -> Vec<u8> {
            let mut v = Vec::new();
            v.extend(op.to_le_bytes());
            v.extend(((8 + 4 * args.len()) as u16).to_le_bytes());
            v.extend(time.to_le_bytes());
            v.extend(mask.to_le_bytes());
            args.iter().for_each(|a| v.extend(a.to_le_bytes()));
            v
        }
        fn step(
            anm: &mut AnmManager,
            vms: &mut [AnmVm],
            rng: &mut crate::rng::Rng,
            frames: usize,
        ) -> String {
            let mut rec = Recorder::new();
            for _ in 0..frames {
                anm.begin_frame();
                for vm in vms.iter_mut() {
                    vm.run(anm, rng);
                    anm.draw(&mut rec, vm);
                }
            }
            format!("{:?}\n{vms:?}\n{rng:?}", rec.draws)
        }
        let mut s = ins(3, 0, 0, &[0]);
        let body = s.len() as u32;
        s.extend(ins(39, 1, 0, &[0, 1]));
        s.extend(ins(59, 1, 1, &[10000, 100]));
        s.extend(ins(4, 2, 0, &[body, 1]));
        let synthetic = th08_formats::anm::Chunk {
            name: "t".into(),
            width: 256,
            height: 256,
            sprites: vec![th08_formats::anm::Sprite {
                id: 0,
                x: 0.0,
                y: 0.0,
                w: 32.0,
                h: 16.0,
            }],
            scripts: vec![th08_formats::anm::Script { id: 0, data: s }],
            has_texture: false,
        };
        let mut files = vec![("t".to_string(), vec![synthetic])];
        if let Some(ar) = crate::testdata::archive() {
            for n in ar.names().iter().filter(|n| n.ends_with(".anm")).take(6) {
                files.push((
                    n.clone(),
                    th08_formats::anm::parse(&ar.read(n).unwrap()).unwrap(),
                ));
            }
        }
        let mut anm = AnmManager::default();
        let mut rng = crate::rng::Rng::seeded(0x1234);
        let mut vms = Vec::new();
        for (slot, (name, chunks)) in files.into_iter().enumerate() {
            let n = chunks.iter().map(|c| c.scripts.len()).sum::<usize>();
            anm.insert(slot, &name, chunks, |_| crate::drawlist::TexId(slot as u32));
            for k in 0..n {
                let mut vm = AnmVm::default();
                anm.set_script(&mut rng, &mut vm, slot, k);
                vms.push(vm);
            }
        }
        let start = anm.script(0, 0).unwrap().data.clone();
        step(&mut anm, &mut vms, &mut rng, 30);
        assert_ne!(anm.script(0, 0).unwrap().data, start);
        let (mut anm2, mut vms2, mut rng2) = (anm.clone(), vms.clone(), rng.clone());
        assert!(Arc::ptr_eq(
            &anm.get(0).unwrap().scripts[0],
            &anm2.get(0).unwrap().scripts[0]
        ));
        let a = step(&mut anm, &mut vms, &mut rng, 120);
        let b = step(&mut anm2, &mut vms2, &mut rng2, 120);
        assert!(rng.total > 30);
        assert_eq!(a, b);
        let (s1, s2) = (anm.get(0).unwrap(), anm2.get(0).unwrap());
        assert!(!Arc::ptr_eq(&s1.scripts[0], &s2.scripts[0]));
        assert_eq!(s1.scripts[0].data, s2.scripts[0].data);
    }

    // 13 §2.6c: half size ((len·0.5)·sprite_w)·scale.x around the projected point, unrotated.
    #[test]
    fn billboard_half_size() {
        let mut vm = AnmVm::default();
        (vm.sprite_w, vm.sprite_h, vm.scale) = (32.0, 16.0, [2.0, 0.5]);
        let c = billboard_corners(&vm, [100.0, 50.0, 0.5], 1.5);
        assert_eq!(c[0], [100.0 - 48.0, 50.0 - 6.0]);
        assert_eq!(c[3], [100.0 + 48.0, 50.0 + 6.0]);
        assert_eq!(length3([3.0, 0.0, 4.0]), 5.0);
    }
}
