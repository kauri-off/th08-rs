//! ANM manager: file slots, script start, tint, DrawByMode / Draw3DWorld world-matrix prepare (03 §2.4; 05 §8–§9, §12; 06 §11.2).

use crate::anmvm::{self, AnmVm, VmFlags};
use crate::content::LogicView;
use crate::types::D3dColor;
use th08_formats::anm::{self, Script};
// Draws and texture uploads go to the core `Recorder`.
use crate::camera::{Camera, Mat4};
use crate::drawlist::{Recorder, TexId};
use anyhow::{Result, ensure};
use std::sync::Arc;

// Linear vertex fog (06 §10.1): f = clamp((far − z) / (far − near), 0, 1) on eye-space z (no range fog); RGB bytes.
#[derive(Clone, Copy, Debug)]
pub struct Fog {
    pub near: f32,
    pub far: f32,
    pub color: [u8; 3],
}

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

// The chunk's sprite rects in THTX pixels (sprites are in logical-size units, 03 §2.2), rounded outward.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "sprite rects lie inside the texture; clamped to it"
)]
#[expect(clippy::disallowed_methods, reason = "texture atlas rects, not logic")]
fn thtx_rects(c: &anm::Chunk, tw: u32, th: u32) -> Vec<[u32; 4]> {
    let (kx, ky) = (
        tw as f32 / c.width.max(1) as f32,
        th as f32 / c.height.max(1) as f32,
    );
    c.sprites
        .iter()
        .filter_map(|s| {
            let x0 = (s.x * kx).floor().clamp(0.0, tw as f32) as u32;
            let y0 = (s.y * ky).floor().clamp(0.0, th as f32) as u32;
            let x1 = ((s.x + s.w) * kx).ceil().clamp(0.0, tw as f32) as u32;
            let y1 = ((s.y + s.h) * ky).ceil().clamp(0.0, th as f32) as u32;
            (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
        })
        .collect()
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
                // Pack image by chunk name: THTX size kept, merged per sprite rect.
                (Some(t), Some(img)) => {
                    let rects = thtx_rects(c, t.width, t.height);
                    rec.create_anm_texture_over(name, entry, t.width, t.height, &t.rgba, rects, img)
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

    // The VM write of a draw (draw-phase step): DrawByMode mode 5 and Draw3DWorld rebuild the cached world matrix
    // behind the same visibility / sprite guards; every other mode writes nothing.
    pub fn prepare(&self, vm: &mut AnmVm, mode: i32) {
        if mode == 5 && self.drawable(vm).is_some() {
            Self::update_world_matrix(vm);
        }
    }

    // Visible, enabled, primary alpha ≠ 0, has a sprite, and the sprite exists.
    pub(crate) fn drawable(&self, vm: &AnmVm) -> Option<SlotSprite> {
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
}

// Vec3_Length: the projected difference includes the depth (13 §2.6c).
pub fn length3(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    anm.prepare(vm, vm.draw_mode);
                    crate::emit::anm::emit(anm, &mut rec, vm);
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
}
