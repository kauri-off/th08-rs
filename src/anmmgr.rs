use crate::anm::{self, Script};
use crate::archive::Archive;
use crate::gfx::{Gfx, TextureId};
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
}

impl Default for AnmManager {
    fn default() -> Self {
        Self { slots: std::array::from_fn(|_| None) }
    }
}

impl AnmManager {
    pub fn load(&mut self, gfx: &mut Gfx, archive: &Archive, slot: usize, name: &str) -> Result<()> {
        ensure!(slot < SLOTS, "anm slot {slot} out of range");
        let chunks = anm::parse(&archive.read(name)?)?;
        self.unload(gfx, slot);
        let (mut sprites, mut scripts, mut textures) = (Vec::new(), Vec::new(), Vec::new());
        for c in chunks {
            let tex = match &c.texture {
                Some(t) => gfx.create_texture(t.width, t.height, &t.rgba),
                // '@' entries are blank render targets (text / capture).
                None => gfx.create_texture(c.width.max(1), c.height.max(1), &vec![0; (c.width.max(1) * c.height.max(1) * 4) as usize]),
            };
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
        Ok(())
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
}
