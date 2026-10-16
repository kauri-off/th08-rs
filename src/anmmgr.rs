use crate::anm::{self, Script};
use crate::anmvm::{self, AnmVm};
use crate::archive::Archive;
use crate::gfx::{Blend, Camera, Gfx, Mat4, TextureId};
use crate::rng::Rng;
use anyhow::{Result, ensure};

pub const SLOTS: usize = 25;

#[derive(Clone, Copy, Debug)]
pub struct SlotSprite {
    pub tex: TextureId,
    pub uv: [f32; 4],
    pub w: f32,
    pub h: f32,
}

pub struct LoadedAnm {
    pub name: String,
    pub sprites: Vec<SlotSprite>,
    pub scripts: Vec<Script>,
    textures: Vec<TextureId>,
}

pub struct AnmManager {
    slots: [Option<LoadedAnm>; SLOTS],
    pub speed: f32,
    pub rng: Rng,
    pub vm_counter: u32,
    pub screen_offset: [f32; 2],
    pub tint: u32,
    pub stage_camera: Option<Camera>,
    last_blend: Blend,
}

impl Default for AnmManager {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            speed: 1.0,
            rng: Rng::default(),
            vm_counter: 0,
            screen_offset: [0.0; 2],
            tint: 0,
            stage_camera: None,
            last_blend: Blend::Alpha,
        }
    }
}

impl AnmManager {
    pub fn load(&mut self, gfx: &mut Gfx, archive: &Archive, slot: usize, name: &str) -> Result<()> {
        ensure!(slot < SLOTS, "anm slot {slot} out of range");
        let chunks = anm::parse(&archive.read(name)?)?;
        self.unload(gfx, slot);
        self.insert(slot, name, chunks, |c| match &c.texture {
            Some(t) => gfx.create_texture(t.width, t.height, &t.rgba),
            // '@' entries are blank render targets (text / capture).
            None => gfx.create_texture(c.width.max(1), c.height.max(1), &vec![0; (c.width.max(1) * c.height.max(1) * 4) as usize]),
        });
        Ok(())
    }

    pub fn insert(&mut self, slot: usize, name: &str, chunks: Vec<anm::Chunk>, mut make_tex: impl FnMut(&anm::Chunk) -> TextureId) {
        let (mut sprites, mut scripts, mut textures) = (Vec::new(), Vec::new(), Vec::new());
        for c in chunks {
            let tex = make_tex(&c);
            textures.push(tex);
            let (tw, th) = (c.width.max(1) as f32, c.height.max(1) as f32);
            sprites.extend(c.sprites.iter().map(|s| SlotSprite {
                tex,
                uv: [s.x / tw, s.y / th, (s.x + s.w) / tw, (s.y + s.h) / th],
                w: s.w,
                h: s.h,
            }));
            scripts.extend(c.scripts);
        }
        self.slots[slot] = Some(LoadedAnm { name: name.to_string(), sprites, scripts, textures });
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

    pub fn begin_frame(&mut self) {
        self.vm_counter = 0;
    }

    // SPEC-GAP: initial blend state of a frame is not specified; alpha assumed.
    pub fn reset_render_state(&mut self) {
        self.last_blend = Blend::Alpha;
    }

    pub fn set_script(&mut self, vm: &mut AnmVm, slot: usize, index: usize) {
        vm.script_index = index as i16;
        vm.pos = [0.0; 3];
        vm.pos_offset = [0.0; 3];
        self.start_script(vm, slot, index);
    }

    pub fn start_script(&mut self, vm: &mut AnmVm, slot: usize, index: usize) {
        if self.script(slot, index).is_none() {
            *vm = AnmVm { script_index: 0, ..AnmVm::default() };
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

    pub fn draw_mode(&mut self, gfx: &mut Gfx, vm: &mut AnmVm, mode: i32) {
        use anmvm::*;
        let alpha = vm.color[A];
        if vm.flags & (F_VISIBLE | F_ENABLED) != (F_VISIBLE | F_ENABLED) || alpha == 0 || !vm.has_sprite {
            return;
        }
        let Some(&spr) = self.sprite(vm.anm_slot as usize, vm.sprite_index as usize) else { return };
        let (hw, hh) = (vm.sprite_w * vm.scale[0] / 2.0, vm.sprite_h * vm.scale[1] / 2.0);
        let [px, py, pz] = vm.pos;
        let ax = if vm.flags & F_ANCHOR_X != 0 { hw } else { 0.0 };
        let ay = if vm.flags & F_ANCHOR_Y != 0 { hh } else { 0.0 };
        let axis_aligned = || {
            let (x0, y0) = (px - hw + ax, py - hh + ay);
            let (x1, y1) = (px + hw + ax, py + hh + ay);
            [[x0, y0], [x1, y0], [x0, y1], [x1, y1]]
        };
        let rotated = || {
            let (s, c) = vm.rotation[2].sin_cos();
            [(-hw, -hh), (hw, -hh), (-hw, hh), (hw, hh)].map(|(dx, dy)| [dx * c - dy * s + px + ax, dx * s + dy * c + py + ay])
        };
        let (corners, snap) = match mode {
            0 => (axis_aligned(), true),
            1 => (axis_aligned(), false),
            2 | 3 if vm.rotation[2] == 0.0 => (axis_aligned(), mode == 2),
            2 | 3 => (rotated(), false),
            // Billboard size uses a reference vector that is always zero, so nothing visible is drawn.
            4 => return,
            5 => {
                if vm.flags & (F_ROT_DIRTY | F_SCALE_DIRTY) != 0 && vm.flags & F_FREEZE_MATRIX == 0 {
                    let mut m = Mat4::scale(vm.sprite_w / 256.0 * vm.scale[0], vm.sprite_h / 256.0 * vm.scale[1], 1.0);
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
                let mut w = vm.world_matrix;
                w.0[3] = [px + ax, py + ay, pz, 1.0];
                let vp = gfx.viewport();
                let cam = match (vm.flags & F_STAGE_CAM != 0, self.stage_camera) {
                    (true, Some(c)) => c,
                    _ => Camera::set_2d(vp),
                };
                let corners = [[-128.0, -128.0], [128.0, -128.0], [-128.0, 128.0], [128.0, 128.0]].map(|[x, y]| {
                    let p = w.transform([x, y, 0.0]);
                    let s = cam.project([p[0], p[1], p[2]], vp);
                    [s[0], s[1]]
                });
                (corners, false)
            }
            _ => return,
        };
        // EmitQuad: global offset, optional snapping, then D3D pixel-centre convention (+0.5) for wgpu.
        let corners = corners.map(|[x, y]| {
            let (mut x, mut y) = (x + self.screen_offset[0], y + self.screen_offset[1]);
            if snap {
                x = x.round_ties_even() - 0.5;
                y = y.round_ties_even() - 0.5;
            }
            [x + 0.5, y + 0.5]
        });
        let vp = gfx.viewport();
        let (minx, maxx) = corners.iter().fold((f32::MAX, f32::MIN), |(a, b), c| (a.min(c[0]), b.max(c[0])));
        let (miny, maxy) = corners.iter().fold((f32::MAX, f32::MIN), |(a, b), c| (a.min(c[1]), b.max(c[1])));
        if maxx < vp.x || minx > vp.x + vp.w || maxy < vp.y || miny > vp.y + vp.h {
            return;
        }
        let (u0, v0) = (spr.uv[0] + vm.uv_off[0], spr.uv[1] + vm.uv_off[1]);
        let (u1, v1) = (spr.uv[2] + vm.uv_off[0], spr.uv[3] + vm.uv_off[1]);
        let c = if vm.flags & F_COLOR2 != 0 { vm.color2 } else { vm.color };
        let mut rgba = [c[R], c[G], c[B], c[A]];
        // SPEC-GAP: the global tint is read as a D3DCOLOR applied per channel (including alpha).
        if self.tint != 0 {
            let t = self.tint.to_le_bytes();
            let tc = [t[R], t[G], t[B], t[A]];
            for k in 0..4 {
                rgba[k] = ((rgba[k] as u32 * tc[k] as u32) >> 7).min(255) as u8;
            }
        }
        let blend = match vm.blend() {
            0 => Blend::Alpha,
            1 => Blend::Add,
            _ => self.last_blend,
        };
        self.last_blend = blend;
        gfx.quad_strip(spr.tex, blend, corners, [[u0, v0], [u1, v0], [u0, v1], [u1, v1]], rgba);
    }
}
