//! Glyph and arrow masks in one single-channel atlas: shelf packing, least-recently-used shelf eviction, CPU uploads.
use crate::scene::{Dir, Face};
use crate::text::font;
use std::collections::HashMap;
use swash::scale::image::Image;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Cap, Command, Format, Join, Mask, Placement, Stroke, Vector};

// Sub-pixel x positions a glyph is rasterised at.
pub const SUB_STEPS: f32 = 4.0;
const PAD: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    // `sub` in steps of 1 / `SUB_STEPS` px.
    Glyph { face: Face, id: u16, sub: u8 },
    Arrow { dir: Dir },
}

// `size` and `ring` in 1/64 px; ring 0 is the fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub shape: Shape,
    pub size: u32,
    pub ring: u32,
}

// Where a mask is, and its offset from the pen (x right, `top` above the baseline).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub left: i32,
    pub top: i32,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Upload {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

struct Shelf {
    y: u32,
    h: u32,
    pen: u32,
    used: u64,
    keys: Vec<Key>,
}

// The atlas has no room for a mask in this frame without dropping one the frame already uses.
#[derive(Debug, PartialEq, Eq)]
pub struct Full;

pub struct Atlas {
    size: u32,
    shelves: Vec<Shelf>,
    next_y: u32,
    map: HashMap<Key, Option<Slot>>,
    scale: ScaleContext,
    pub uploads: Vec<Upload>,
    pub evicted_shelves: u64,
}

impl Atlas {
    pub fn new(size: u32) -> Self {
        Self {
            size,
            shelves: Vec::new(),
            next_y: 0,
            map: HashMap::new(),
            scale: ScaleContext::new(),
            uploads: Vec::new(),
            evicted_shelves: 0,
        }
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn cached(&self) -> usize {
        self.map.len()
    }

    // Forgets every mask, and the uploads not yet sent, at `size`.
    pub fn reset(&mut self, size: u32) {
        self.size = size;
        self.shelves.clear();
        self.next_y = 0;
        self.map.clear();
        self.uploads.clear();
    }

    // The mask of `key` (None: nothing to draw), rasterised and placed on first use.
    pub fn get(&mut self, key: Key, frame: u64) -> Result<Option<Slot>, Full> {
        if let Some(&slot) = self.map.get(&key) {
            if let Some(s) = slot {
                self.touch(s.y, frame);
            }
            return Ok(slot);
        }
        let Some((data, p)) = self.raster(key) else {
            self.map.insert(key, None);
            return Ok(None);
        };
        let (x, y) = self.place(key, p.width, p.height, frame)?;
        let slot = Slot {
            x,
            y,
            w: p.width,
            h: p.height,
            left: p.left,
            top: p.top,
        };
        self.uploads.push(Upload {
            x,
            y,
            w: p.width,
            h: p.height,
            data,
        });
        self.map.insert(key, Some(slot));
        Ok(Some(slot))
    }

    fn touch(&mut self, y: u32, frame: u64) {
        if let Ok(i) = self.shelves.binary_search_by_key(&y, |s| s.y) {
            self.shelves[i].used = frame;
        }
    }

    fn place(&mut self, key: Key, w: u32, h: u32, frame: u64) -> Result<(u32, u32), Full> {
        let fits =
            |s: &Shelf, size: u32| s.h >= h && s.h <= h + h / 4 + 2 && s.pen + w + PAD <= size;
        let size = self.size;
        let mut at = self.shelves.iter().position(|s| fits(s, size));
        if at.is_none() && w + PAD <= size && self.next_y + h + PAD <= size {
            let shelf_h = h + h / 8;
            let y = self.next_y;
            self.next_y += shelf_h + PAD;
            self.shelves.push(Shelf {
                y,
                h: shelf_h,
                pen: 0,
                used: frame,
                keys: Vec::new(),
            });
            at = Some(self.shelves.len() - 1);
        }
        if at.is_none() {
            at = self.evict(h, frame);
        }
        let s = &mut self.shelves[at.ok_or(Full)?];
        if s.pen + w + PAD > size {
            return Err(Full);
        }
        let x = s.pen;
        s.pen += w + PAD;
        s.used = frame;
        s.keys.push(key);
        Ok((x, s.y))
    }

    // Empties the shelf tall enough for `h` that was used longest ago, never one this frame uses.
    fn evict(&mut self, h: u32, frame: u64) -> Option<usize> {
        let i = self
            .shelves
            .iter()
            .enumerate()
            .filter(|(_, s)| s.used < frame && s.h >= h)
            .min_by_key(|(_, s)| (s.used, s.h))?
            .0;
        for k in std::mem::take(&mut self.shelves[i].keys) {
            self.map.remove(&k);
        }
        self.shelves[i].pen = 0;
        self.evicted_shelves += 1;
        Some(i)
    }

    fn raster(&mut self, key: Key) -> Option<(Vec<u8>, Placement)> {
        let size = key.size as f32 / 64.0;
        let ring = key.ring as f32 / 64.0;
        match key.shape {
            Shape::Glyph { face, id, sub } => self.glyph(face, id, sub, size, ring),
            Shape::Arrow { dir } => arrow(dir, size, ring),
        }
    }

    fn glyph(
        &mut self,
        face: Face,
        id: u16,
        sub: u8,
        size: f32,
        ring: f32,
    ) -> Option<(Vec<u8>, Placement)> {
        let mut scaler = self
            .scale
            .builder(font(face))
            .size(size)
            .hint(false)
            .build();
        let offset = Vector::new(f32::from(sub) / SUB_STEPS, 0.0);
        let mut render = Render::new(&[Source::Outline]);
        render.format(Format::Alpha).offset(offset);
        let fill: Image = render.render(&mut scaler, id)?;
        if fill.placement.width == 0 || fill.placement.height == 0 {
            return None;
        }
        if ring <= 0.0 {
            return Some((fill.data, fill.placement));
        }
        let mut stroke = Stroke::new(ring * 2.0);
        stroke.join(Join::Round).cap(Cap::Round);
        let mut render = Render::new(&[Source::Outline]);
        render.format(Format::Alpha).offset(offset).style(stroke);
        let outer: Image = render.render(&mut scaler, id)?;
        Some(union(outer, &fill))
    }
}

// `outer` with `inner` added: the ring is solid under the glyph, so no background shows through its soft edge.
fn union(mut outer: Image, inner: &Image) -> (Vec<u8>, Placement) {
    let (o, i) = (outer.placement, inner.placement);
    let (dx, dy) = (i.left - o.left, o.top - i.top);
    for y in 0..i.height {
        for x in 0..i.width {
            let (Some(ox), Some(oy)) = (x.checked_add_signed(dx), y.checked_add_signed(dy)) else {
                continue;
            };
            if ox >= o.width || oy >= o.height {
                continue;
            }
            let a = u32::from(inner.data[(y * i.width + x) as usize]);
            let p = &mut outer.data[(oy * o.width + ox) as usize];
            *p = (255 - (255 - a) * (255 - u32::from(*p)) / 255) as u8;
        }
    }
    (outer.data, o)
}

fn arrow(dir: Dir, size: f32, ring: f32) -> Option<(Vec<u8>, Placement)> {
    let s = size / 10.0;
    let pts: [(f32, f32); 3] = match dir {
        Dir::Left => [(9.0, 0.0), (9.0, 10.0), (1.0, 5.0)],
        Dir::Right => [(1.0, 0.0), (1.0, 10.0), (9.0, 5.0)],
        Dir::Up => [(0.0, 9.0), (10.0, 9.0), (5.0, 1.0)],
        Dir::Down => [(0.0, 1.0), (10.0, 1.0), (5.0, 9.0)],
    };
    let v = |(x, y): (f32, f32)| Vector::new(x * s, y * s);
    let path = [
        Command::MoveTo(v(pts[0])),
        Command::LineTo(v(pts[1])),
        Command::LineTo(v(pts[2])),
        Command::Close,
    ];
    let (mut fill, p) = Mask::new(&path[..]).format(Format::Alpha).render();
    if ring <= 0.0 {
        return (p.width > 0).then_some((fill, p));
    }
    let mut stroke = Stroke::new(ring * 2.0);
    stroke.join(Join::Round);
    let (outer, op) = Mask::new(&path[..])
        .format(Format::Alpha)
        .style(stroke)
        .render();
    let mut a = Image::new();
    a.placement = op;
    a.data = outer;
    let mut b = Image::new();
    b.placement = p;
    b.data = std::mem::take(&mut fill);
    Some(union(a, &b))
}
