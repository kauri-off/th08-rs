//! ANM manager: 25 file slots, script start, draw modes 0–5, EmitQuad, tint, Draw3DWorld (03 §2.4, §5; 05 §8–§9, §12; 06 §11.2).

use crate::anm::{self, Script};
use crate::anmvm::{self, AnmVm};
use crate::archive::Archive;
use crate::gfx::{self, Blend, Camera, Gfx, Mat4, TextureId, Vertex};
use crate::rng::Rng;
use anyhow::{Result, ensure};

// ANM file slots 0..24; index ≥ 25 is an error (03 §2.4).
pub const SLOTS: usize = 25;

// Runtime sprite record (03 §5, structs.md), reduced to texture, normalised UV rect (u0, v0, u1, v1) and logical size.
#[derive(Clone, Copy, Debug)]
pub struct SlotSprite {
    pub tex: TextureId,
    pub uv: [f32; 4],
    pub w: f32,
    pub h: f32,
}

// Loaded-file record at AnmManager + 0x24 + slot × 0x1C (03 §5): cumulative sprite and script arrays (03 §2.4).
pub struct LoadedAnm {
    pub name: String,
    pub sprites: Vec<SlotSprite>,
    pub scripts: Vec<Script>,
    textures: Vec<TextureId>,
}

pub struct AnmManager {
    slots: [Option<LoadedAnm>; SLOTS],
    // Global game speed used by the VM timers (05 §2), 1.0 in normal play.
    pub speed: f32,
    // The global game RNG g_Rng, used by ANM ops 59/60 (05 §5 notes).
    pub rng: Rng,
    pub vm_counter: u32,
    // Global screen offset AnmManager +0x1C/+0x20, written by the screen-shake effect (05 §9 EmitQuad step 1).
    pub screen_offset: [f32; 2],
    // Global tint D3DCOLOR at AnmManager+0, enabled by +4 (05 §12.1); 0x80 per channel is neutral.
    pub tint: u32,
    pub tint_on: bool,
    // Stage tint request +0x6468 (06 §10.2 step 3), applied at the next Stage_DrawLow.
    pub stage_tint: Option<u32>,
    // Stage flag +0x646C "a bomb drew this frame" (06 §10.2, 14 §1.3).
    pub bomb_drew: bool,
    // Camera used by VMs with flag bit 15 (05 §9).
    pub stage_camera: Option<Camera>,
    // 0x4EA3F4 = g_Stage+0x63C4: normalised camera-right vector written by Stage_SetCamera every frame (05 §9 mode 4).
    pub cam_right: [f32; 3],
    // Blend state cache +0x24C0: blend bits 2/3 keep whatever the previous sprite set (05 §12.5).
    last_blend: Blend,
    // +0x24C8: last sprite whose texture matrix Draw3DWorld uploaded, with its UV offset (05 §12.8).
    last_tex_matrix: Option<((i16, i16), [f32; 2])>,
    // Texture cache +0x24BC, shared by the 2D paths and Draw3DWorld (05 §12.8).
    bound_tex: Option<TextureId>,
    // Port-only: texture entries loaded so far; paces the loading screen (gamemgr `loading_total`).
    pub entries_loaded: u32,
    // Pending AnmManager_QueueCapture request +0x2A252C (12 §3): the destination sprite rect at queue time.
    pub capture_req: Option<SlotSprite>,
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
            rng: Rng::default(),
            vm_counter: 0,
            screen_offset: [0.0; 2],
            tint: 0x8080_8080,
            tint_on: false,
            stage_tint: None,
            bomb_drew: false,
            stage_camera: None,
            cam_right: [0.0; 3],
            last_blend: Blend::Alpha,
            last_tex_matrix: None,
            bound_tex: None,
            entries_loaded: 0,
            capture_req: None,
        }
    }
}

impl AnmManager {
    // Anm_Preload 0x465860 + Anm_LoadEntryTexture 0x465CC0 (03 §2); the port uploads all entries at once.
    pub fn load(
        &mut self,
        gfx: &mut Gfx,
        archive: &Archive,
        slot: usize,
        name: &str,
    ) -> Result<()> {
        ensure!(slot < SLOTS, "anm slot {slot} out of range");
        let chunks = anm::parse(&archive.read(name)?)?;
        self.unload(gfx, slot);
        let mut entry = 0;
        self.insert(slot, name, chunks, |c| {
            let id = match &c.texture {
                Some(t) => gfx.create_texture(t.width, t.height, &t.rgba),
                // '@' entries are blank render targets (text / capture).
                None => gfx.create_texture(
                    c.width.max(1),
                    c.height.max(1),
                    &vec![0; (c.width.max(1) * c.height.max(1) * 4) as usize],
                ),
            };
            gfx.set_label(id, &format!("{name}#{entry}"), c.width, c.height);
            entry += 1;
            id
        });
        self.entries_loaded += entry;
        Ok(())
    }

    // Concatenates every entry's sprites and scripts into the slot's cumulative arrays (03 §2.4).
    pub fn insert(
        &mut self,
        slot: usize,
        name: &str,
        chunks: Vec<anm::Chunk>,
        mut make_tex: impl FnMut(&anm::Chunk) -> TextureId,
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
            scripts.extend(c.scripts);
        }
        self.slots[slot] = Some(LoadedAnm {
            name: name.to_string(),
            sprites,
            scripts,
            textures,
        });
    }

    pub fn unload(&mut self, gfx: &mut Gfx, slot: usize) {
        if let Some(a) = self.slots.get_mut(slot).and_then(Option::take) {
            a.textures.into_iter().for_each(|t| gfx.free_texture(t));
        }
    }

    pub fn get(&self, slot: usize) -> Option<&LoadedAnm> {
        self.slots.get(slot)?.as_ref()
    }

    pub fn sprite(&self, slot: usize, index: usize) -> Option<&SlotSprite> {
        self.get(slot)?.sprites.get(index)
    }

    pub fn script(&self, slot: usize, index: usize) -> Option<&Script> {
        self.get(slot)?.scripts.get(index)
    }

    pub fn script_mut(&mut self, slot: usize, index: usize) -> Option<&mut Script> {
        self.slots.get_mut(slot)?.as_mut()?.scripts.get_mut(index)
    }

    // Statistics counter (05 §4).
    pub fn begin_frame(&mut self) {
        self.vm_counter = 0;
    }

    // Texture-matrix and texture caches reset (05 §12.8); the blend state carries over between frames (§12.5).
    pub fn reset_render_state(&mut self) {
        self.last_tex_matrix = None;
        self.bound_tex = None;
    }

    // 2D paths (EmitQuad / DrawCustomQuad) update the shared texture cache +0x24BC (05 §12.8).
    pub fn bind(&mut self, tex: TextureId) {
        self.bound_tex = Some(tex);
    }

    // AnmManager_SetScript 0x40B910 (05 §8): pos and pos_offset are zeroed before the frame-0 run.
    pub fn set_script(&mut self, vm: &mut AnmVm, slot: usize, index: usize) {
        vm.script_index = index as i16;
        vm.pos = [0.0; 3];
        vm.pos_offset = [0.0; 3];
        self.start_script(vm, slot, index);
    }

    // AnmFile_SetScript 0x4069F0 (09 §4.5): like `set_script` but keeps the VM pos / pos offset.
    pub fn file_set_script(&mut self, vm: &mut AnmVm, slot: usize, index: usize) {
        vm.script_index = index as i16;
        self.start_script(vm, slot, index);
    }

    // AnmFile_StartScript 0x45E580 (05 §8): a missing script zeroes the whole VM; otherwise init and run once (frame 0).
    pub fn start_script(&mut self, vm: &mut AnmVm, slot: usize, index: usize) {
        if self.script(slot, index).is_none() {
            *vm = AnmVm {
                script_index: 0,
                ..AnmVm::default()
            };
            return;
        }
        vm.init();
        vm.anm_slot = slot as i16;
        vm.flags &= !(anmvm::F_FLIP_X | anmvm::F_FLIP_Y | anmvm::F_VISIBLE);
        vm.script = Some((slot, index));
        vm.ip = Some(0);
        vm.time.set(0);
        vm.run(self);
    }

    pub fn draw(&mut self, gfx: &mut Gfx, vm: &mut AnmVm) {
        self.draw_mode(gfx, vm, vm.draw_mode);
    }

    // 2D draws carry the VM's z as the vertex depth.
    pub fn draw_mode(&mut self, gfx: &mut Gfx, vm: &mut AnmVm, mode: i32) {
        let z = vm.pos[2];
        self.draw_mode_z(gfx, vm, mode, z);
    }

    // Mode-5 world matrix (05 §9): base (w/256, h/256) × scale, then × RotX × RotY × RotZ (each only if ≠ 0).
    fn update_world_matrix(vm: &mut AnmVm) {
        use anmvm::*;
        if vm.flags & (F_ROT_DIRTY | F_SCALE_DIRTY) != 0 && vm.flags & F_FREEZE_MATRIX == 0 {
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
            vm.flags &= !(F_ROT_DIRTY | F_SCALE_DIRTY);
        }
    }

    // Colour or colour2 (bit 17), then the global tint (05 §9 EmitQuad step 4); returns RGBA.
    fn vm_color(&self, vm: &AnmVm) -> [u8; 4] {
        use anmvm::*;
        let c = if vm.flags & F_COLOR2 != 0 {
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
                    ((a[k] as u32 + b[k] as u32) / 2) as u8
                }))
            }
            None => c,
        });
    }

    // Global tint: D3DCOLOR at AnmManager+0, enabled by +4; every channel including alpha (05 §12.1).
    pub fn tinted(&self, mut rgba: [u8; 4]) -> [u8; 4] {
        use anmvm::*;
        if self.tint_on {
            let t = self.tint.to_le_bytes();
            let tc = [t[R], t[G], t[B], t[A]];
            for k in 0..4 {
                rgba[k] = ((rgba[k] as u32 * tc[k] as u32) >> 7).min(255) as u8;
            }
        }
        rgba
    }

    // Blend bits 4–5: 0 alpha, 1 additive, 2/3 set nothing (05 §12.5).
    pub fn blend_for(&mut self, vm: &AnmVm) -> Blend {
        let blend = match vm.blend() {
            0 => Blend::Alpha,
            1 => Blend::Add,
            _ => self.last_blend,
        };
        self.last_blend = blend;
        blend
    }

    // AnmManager_Draw3DWorld 0x464470 (06 §11.2): unit quad ±128 through `cam` with vertex fog; the screen-shake
    // offset is added to the translation.
    pub fn draw_3d_world(&mut self, gfx: &mut Gfx, vm: &mut AnmVm, cam: &Camera, fog: Option<Fog>) {
        use anmvm::*;
        if vm.flags & (F_VISIBLE | F_ENABLED) != (F_VISIBLE | F_ENABLED)
            || vm.color[A] == 0
            || !vm.has_sprite
        {
            return;
        }
        let Some(&spr) = self.sprite(vm.anm_slot as usize, vm.sprite_index as usize) else {
            return;
        };
        Self::update_world_matrix(vm);
        let (hw, hh) = (
            (vm.sprite_w * vm.scale[0] / 2.0).abs(),
            (vm.sprite_h * vm.scale[1] / 2.0).abs(),
        );
        let ax = if vm.flags & F_ANCHOR_X != 0 { hw } else { 0.0 };
        let ay = if vm.flags & F_ANCHOR_Y != 0 { hh } else { 0.0 };
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
        let (off, tex) = match (self.last_tex_matrix, self.bound_tex) {
            (Some((k, o)), Some(t)) if k == key && vm.uv_off[0] == 0.0 => (o, t),
            _ => (
                [spr.uv[0] + vm.uv_off[0], spr.uv[1] + vm.uv_off[1]],
                spr.tex,
            ),
        };
        self.bound_tex = Some(tex);
        self.last_tex_matrix = Some((key, off));
        let (su, sv) = (spr.uv[2] - spr.uv[0], spr.uv[3] - spr.uv[1]);
        let color = self.vm_color(vm);
        let vp = gfx.viewport();
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
                pos: gfx::clip_in_viewport(vproj.transform(p), vp),
                uv: [off[0] + u * su, off[1] + t * sv],
                color,
                fog_color,
                fog: f,
            }
        });
        let blend = self.blend_for(vm);
        gfx.set_log_fog(fog.map(|f| gfx::LogFog {
            color: f.color,
            start: f.near,
            end: f.far,
        }));
        gfx.strip(tex, blend, vm.flags & F_ZWRITE_OFF == 0, v);
        gfx.set_log_fog(None);
    }

    // Anm_DrawRingStrip (07 §10.6 trail, flag 8): one triangle strip of (pos, uv, alpha) vertices with the VM's
    // texture, blend and colour, emitted as consecutive 4-vertex strips.
    pub fn draw_ring_strip(&mut self, gfx: &mut Gfx, vm: &AnmVm, v: &[([f32; 2], [f32; 2], u8)]) {
        use anmvm::*;
        let Some(&spr) = self.sprite(vm.anm_slot as usize, vm.sprite_index as usize) else {
            return;
        };
        let c = if vm.flags & F_COLOR2 != 0 {
            vm.color2
        } else {
            vm.color
        };
        let blend = self.blend_for(vm);
        let off = self.screen_offset;
        let vert = |&([x, y], uv, a): &([f32; 2], [f32; 2], u8)| {
            let rgba = self.tinted([c[R], c[G], c[B], a]);
            Vertex::screen(x + off[0] + 0.5, y + off[1] + 0.5, 0.0, uv, rgba)
        };
        let v: Vec<Vertex> = v.iter().map(vert).collect();
        for q in (0..v.len().saturating_sub(2)).step_by(2) {
            if q + 3 < v.len() {
                gfx.strip(
                    spr.tex,
                    blend,
                    vm.flags & F_ZWRITE_OFF == 0,
                    [v[q], v[q + 1], v[q + 2], v[q + 3]],
                );
            }
        }
        self.bound_tex = Some(spr.tex);
    }

    // `z` is the vertex depth (projected depth for stage billboards, 0 otherwise).
    // AnmManager_DrawByMode 0x45E960 (05 §9); skip unless visible, enabled and primary alpha ≠ 0 (05 §12.2).
    pub fn draw_mode_z(&mut self, gfx: &mut Gfx, vm: &mut AnmVm, mode: i32, z: f32) {
        use anmvm::*;
        let alpha = vm.color[A];
        if vm.flags & (F_VISIBLE | F_ENABLED) != (F_VISIBLE | F_ENABLED)
            || alpha == 0
            || !vm.has_sprite
        {
            return;
        }
        let Some(&spr) = self.sprite(vm.anm_slot as usize, vm.sprite_index as usize) else {
            return;
        };
        let (hw, hh) = (
            vm.sprite_w * vm.scale[0] / 2.0,
            vm.sprite_h * vm.scale[1] / 2.0,
        );
        let [px, py, pz] = vm.pos;
        let ax = if vm.flags & F_ANCHOR_X != 0 { hw } else { 0.0 };
        let ay = if vm.flags & F_ANCHOR_Y != 0 { hh } else { 0.0 };
        let axis_aligned = || {
            let (x0, y0) = (px - hw + ax, py - hh + ay);
            let (x1, y1) = (px + hw + ax, py + hh + ay);
            [[x0, y0], [x1, y0], [x0, y1], [x1, y1]]
        };
        // Modes 2/3 with rotation z ≠ 0: corners rotated, the anchor shift added afterwards unrotated (05 §9).
        let rotated = || {
            let (s, c) = vm.rotation[2].sin_cos();
            [(-hw, -hh), (hw, -hh), (-hw, hh), (hw, hh)]
                .map(|(dx, dy)| [dx * c - dy * s + px + ax, dx * s + dy * c + py + ay])
        };
        // Mode 4 sizes the quad from the projection; both 3D modes carry the projected z as vertex depth (05 §9).
        let mut depth = [z; 4];
        let stage_cam = vm.flags & F_STAGE_CAM != 0;
        let cam = |anm: &Self, vp| match (stage_cam, anm.stage_camera) {
            (true, Some(c)) => c,
            _ => Camera::set_2d(vp),
        };
        let (corners, snap) = match mode {
            // 0 Draw2DSnap 0x462FF0, 1 Draw2D 0x463800, 2 Draw2DRot 0x463210 (snaps only when unrotated), 3 Draw2DRotNoSnap 0x463470.
            0 => (axis_aligned(), true),
            1 => (axis_aligned(), false),
            2 | 3 if vm.rotation[2] == 0.0 => (axis_aligned(), mode == 2),
            2 | 3 => (rotated(), false),
            // 4 DrawBillboard 0x463CF0: never reached through the dispatch (05 §9 reachability), kept for completeness.
            4 => {
                let vp = gfx.viewport();
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
            // 5 Draw3DQuad 0x464070; the anchor shift uses the absolute half size (05 §12.7).
            5 => {
                Self::update_world_matrix(vm);
                let mut w = vm.world_matrix;
                w.0[3] = [px + ax.abs(), py + ay.abs(), pz, 1.0];
                let vp = gfx.viewport();
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
                // EmitQuad never rewrites z: the projected z is the vertex depth (≈ 0.9994 for z = 0 under the 2D camera).
                depth = projected.map(|s| s[2]);
                (projected.map(|s| [s[0], s[1]]), false)
            }
            _ => return,
        };
        self.emit_quad(gfx, vm, spr, corners, depth, snap);
    }

    // DrawBillboard 0x463CF0 (05 §9 mode 4, 13 §2.6c) from an already projected point `s` and the 3D length `len` of
    // project(pos + right) − s; list B moves `s` in screen space (callback 0x428310) between the two.
    pub fn draw_billboard(&mut self, gfx: &mut Gfx, vm: &mut AnmVm, s: [f32; 3], len: f32) {
        use anmvm::*;
        if vm.flags & (F_VISIBLE | F_ENABLED) != (F_VISIBLE | F_ENABLED)
            || vm.color[A] == 0
            || !vm.has_sprite
        {
            return;
        }
        let Some(&spr) = self.sprite(vm.anm_slot as usize, vm.sprite_index as usize) else {
            return;
        };
        self.emit_quad(
            gfx,
            vm,
            spr,
            billboard_corners(vm, s, len),
            [s[2]; 4],
            false,
        );
    }

    fn emit_quad(
        &mut self,
        gfx: &mut Gfx,
        vm: &AnmVm,
        spr: SlotSprite,
        corners: [[f32; 2]; 4],
        depth: [f32; 4],
        snap: bool,
    ) {
        use anmvm::*;
        // AnmManager_EmitQuad 0x4628B0 (05 §9): global offset, optional snapping, then D3D pixel-centre convention (+0.5) for wgpu.
        let corners = corners.map(|[x, y]| {
            let (mut x, mut y) = (x + self.screen_offset[0], y + self.screen_offset[1]);
            // Snap: x87 round-to-nearest-even, minus 0.5 (05 §9).
            if snap {
                x = x.round_ties_even() - 0.5;
                y = y.round_ties_even() - 0.5;
            }
            [x + 0.5, y + 0.5]
        });
        let vp = gfx.viewport();
        let (minx, maxx) = corners
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), c| (a.min(c[0]), b.max(c[0])));
        let (miny, maxy) = corners
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), c| (a.min(c[1]), b.max(c[1])));
        // Cull unless the bounding box meets the viewport (05 §9 step 3), in D3D coordinates (undo the +0.5).
        let (minx, maxx, miny, maxy) = (minx - 0.5, maxx - 0.5, miny - 0.5, maxy - 0.5);
        if maxx < vp.x || minx > vp.x + vp.w || maxy < vp.y || miny > vp.y + vp.h {
            return;
        }
        // EmitQuad step 2 (05 §9): v0 (u0,v0), v1 (u1,v0), v2 (u0,v1), v3 (u1,v1).
        let (u0, v0) = (spr.uv[0] + vm.uv_off[0], spr.uv[1] + vm.uv_off[1]);
        let (u1, v1) = (spr.uv[2] + vm.uv_off[0], spr.uv[3] + vm.uv_off[1]);
        let rgba = self.vm_color(vm);
        let blend = self.blend_for(vm);
        let uv = [[u0, v0], [u1, v0], [u0, v1], [u1, v1]];
        let v = std::array::from_fn(|i| {
            Vertex::screen(corners[i][0], corners[i][1], depth[i], uv[i], rgba)
        });
        self.bound_tex = Some(spr.tex);
        gfx.strip(spr.tex, blend, vm.flags & F_ZWRITE_OFF == 0, v);
    }
}

// Billboard corners (05 §9 mode 4, 13 §2.6c): half size ((len·0.5)·sprite_w)·scale.x, rotated by rotation.z about s.
fn billboard_corners(vm: &AnmVm, s: [f32; 3], len: f32) -> [[f32; 2]; 4] {
    let hw = ((len * 0.5) * vm.sprite_w) * vm.scale[0];
    let hh = ((len * 0.5) * vm.sprite_h) * vm.scale[1];
    let (sn, cs) = vm.rotation[2].sin_cos();
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
