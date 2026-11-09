//! Turns a Scene into vertices, pipeline batches and atlas uploads, on the CPU.
use crate::atlas::{Atlas, Full, Key, SUB_STEPS, Shape, Slot};
use crate::scene::{Arrow, Cmd, Color, Rect, Ring, Scene, Text};
use crate::text::Shaper;
use bytemuck::{Pod, Zeroable};

pub const ATLAS_START: u32 = 2048;
pub const ATLAS_MAX: u32 = 4096;

// Positions are clip space; uv is in atlas units; `color` is straight alpha.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pipe {
    Solid,
    Glyph,
}

// `scissor` is x, y, w, h in px.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub pipe: Pipe,
    pub first: u32,
    pub count: u32,
    pub scissor: [u32; 4],
}

// Per-frame output of `Prepare::run`; the atlas's uploads go with it.
pub struct Prepare {
    pub shaper: Shaper,
    pub atlas: Atlas,
    pub verts: Vec<Vertex>,
    pub batches: Vec<Batch>,
    pub dropped: u32,
    limit: u32,
    frame: u64,
    target: (u32, u32),
    scale: f32,
    clips: Vec<[u32; 4]>,
    dropping: bool,
}

impl Prepare {
    // `limit`: the largest atlas side the device takes.
    pub fn new(limit: u32) -> Self {
        Self {
            shaper: Shaper::new(),
            atlas: Atlas::new(ATLAS_START.min(limit)),
            verts: Vec::new(),
            batches: Vec::new(),
            dropped: 0,
            limit: ATLAS_MAX.min(limit),
            frame: 0,
            target: (1, 1),
            scale: 1.0,
            clips: Vec::new(),
            dropping: false,
        }
    }

    // Builds the frame. A full atlas is emptied once and the frame built again; if that is not enough it grows
    // up to the limit, and at the limit the masks that do not fit are left out and counted in `dropped`.
    pub fn run(&mut self, scene: &Scene, target: (u32, u32)) {
        self.target = target;
        self.scale = scene.scale;
        self.dropping = false;
        let mut emptied = false;
        loop {
            self.frame += 1;
            self.verts.clear();
            self.batches.clear();
            self.dropped = 0;
            self.clips.clear();
            self.clips.push([0, 0, target.0, target.1]);
            if self.build(scene).is_ok() {
                return;
            }
            let size = self.atlas.size();
            if !emptied {
                emptied = true;
                self.atlas.reset(size);
            } else if size < self.limit {
                emptied = false;
                self.atlas.reset((size * 2).min(self.limit));
            } else {
                self.dropping = true;
                self.atlas.reset(size);
            }
        }
    }

    fn build(&mut self, scene: &Scene) -> Result<(), Full> {
        for cmd in &scene.cmds {
            match cmd {
                Cmd::Rect { rect, color } => self.rect(*rect, *color),
                Cmd::Text(t) => self.text(t)?,
                Cmd::Arrow(a) => self.arrow(a)?,
                Cmd::PushClip(r) => self.push_clip(*r),
                Cmd::PopClip => {
                    if self.clips.len() > 1 {
                        self.clips.pop();
                    }
                }
            }
        }
        Ok(())
    }

    fn scissor(&self) -> [u32; 4] {
        *self.clips.last().expect("the window clip stays")
    }

    fn push_clip(&mut self, r: Rect) {
        let [cx, cy, cw, ch] = self.scissor();
        let s = self.scale;
        let x0 = px((r.x * s).round()).max(cx);
        let y0 = px((r.y * s).round()).max(cy);
        let x1 = px(((r.x + r.w) * s).round()).min(cx + cw);
        let y1 = px(((r.y + r.h) * s).round()).min(cy + ch);
        self.clips
            .push([x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)]);
    }

    // Edges land on whole pixels: a rule keeps its width wherever it sits, and at least 1 px.
    fn rect(&mut self, r: Rect, color: Color) {
        if r.w <= 0.0 || r.h <= 0.0 {
            return;
        }
        let s = self.scale;
        let (x0, y0) = ((r.x * s).round(), (r.y * s).round());
        let x1 = ((r.x + r.w) * s).round().max(x0 + 1.0);
        let y1 = ((r.y + r.h) * s).round().max(y0 + 1.0);
        self.quad(Pipe::Solid, [x0, y0, x1, y1], [0.0; 4], color);
    }

    fn quad(&mut self, pipe: Pipe, [x0, y0, x1, y1]: [f32; 4], uv: [f32; 4], color: Color) {
        let scissor = self.scissor();
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }
        let (w, h) = (self.target.0 as f32, self.target.1 as f32);
        let v = |x: f32, y: f32, u: f32, t: f32| Vertex {
            pos: [x / w * 2.0 - 1.0, 1.0 - y / h * 2.0],
            uv: [u, t],
            color: color.0,
        };
        let first = self.verts.len() as u32;
        let [u0, t0, u1, t1] = uv;
        self.verts.extend_from_slice(&[
            v(x0, y0, u0, t0),
            v(x1, y0, u1, t0),
            v(x0, y1, u0, t1),
            v(x0, y1, u0, t1),
            v(x1, y0, u1, t0),
            v(x1, y1, u1, t1),
        ]);
        match self.batches.last_mut() {
            Some(b) if b.pipe == pipe && b.scissor == scissor => b.count += 6,
            _ => self.batches.push(Batch {
                pipe,
                first,
                count: 6,
                scissor,
            }),
        }
    }

    fn slot(&mut self, key: Key) -> Result<Option<Slot>, Full> {
        match self.atlas.get(key, self.frame) {
            Err(Full) if self.dropping => {
                self.dropped += 1;
                Ok(None)
            }
            r => r,
        }
    }

    fn mask(&mut self, s: Slot, x: f32, y: f32, color: Color) {
        let a = self.atlas.size() as f32;
        let (sx, sy, sw, sh) = (s.x as f32, s.y as f32, s.w as f32, s.h as f32);
        let (x0, y0) = (x + s.left as f32, y - s.top as f32);
        self.quad(
            Pipe::Glyph,
            [x0, y0, x0 + sw, y0 + sh],
            [sx / a, sy / a, (sx + sw) / a, (sy + sh) / a],
            color,
        );
    }

    fn text(&mut self, t: &Text) -> Result<(), Full> {
        if let Some(r) = t.ring {
            self.glyphs(t, r.color, ring_q(r))?;
        }
        self.glyphs(t, t.color, 0)
    }

    fn glyphs(&mut self, t: &Text, color: Color, ring: u32) -> Result<(), Full> {
        let shaped = self.shaper.shape(&t.text, t.face);
        let size_px = t.size * self.scale;
        let size = q64(size_px);
        let (ox, oy) = (t.x * self.scale, (t.baseline * self.scale).round());
        for g in &shaped.glyphs {
            let x = ox + g.x * size_px;
            let whole = x.floor();
            let sub = px((x - whole) * SUB_STEPS).min(px(SUB_STEPS) - 1) as u8;
            let key = Key {
                shape: Shape::Glyph {
                    face: t.face,
                    id: g.id,
                    sub,
                },
                size,
                ring,
            };
            if let Some(s) = self.slot(key)? {
                self.mask(s, whole, oy - (g.y * size_px).round(), color);
            }
        }
        Ok(())
    }

    fn arrow(&mut self, a: &Arrow) -> Result<(), Full> {
        let size = q64(a.size * self.scale);
        let (x, y) = ((a.x * self.scale).round(), (a.y * self.scale).round());
        let shape = Shape::Arrow { dir: a.dir };
        if let Some(r) = a.ring
            && let Some(s) = self.slot(Key {
                shape,
                size,
                ring: ring_q(r),
            })?
        {
            self.mask(s, x, y, r.color);
        }
        if let Some(s) = self.slot(Key {
            shape,
            size,
            ring: 0,
        })? {
            self.mask(s, x, y, a.color);
        }
        Ok(())
    }
}

#[expect(clippy::cast_sign_loss, reason = "negative coordinates saturate to 0")]
fn px(v: f32) -> u32 {
    v as u32
}

fn q64(v: f32) -> u32 {
    px((v * 64.0).round())
}

fn ring_q(r: Ring) -> u32 {
    q64(r.width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Dir, Face};

    fn label(text: &str, y: f32, size: f32) -> Text {
        Text {
            x: 10.25,
            baseline: y,
            text: text.into(),
            face: Face::Serif,
            size,
            color: Color::WHITE,
            ring: Some(Ring::BLACK_1PX),
        }
    }

    fn page(texts: &[String], size: f32) -> Scene {
        let mut s = Scene::new(1.0);
        for (i, t) in texts.iter().enumerate() {
            s.text(label(t, 30.0 + 40.0 * i as f32, size));
        }
        s
    }

    #[test]
    fn ring_is_drawn_under_the_fill_and_order_makes_batches() {
        let mut s = Scene::new(1.0);
        s.rect(Rect::new(0.0, 0.0, 100.0, 50.0), Color::BLACK);
        s.text(label("Sound", 30.0, 28.0));
        s.rect(Rect::new(0.0, 60.0, 100.0, 1.0), Color::WHITE);
        let mut p = Prepare::new(4096);
        p.run(&s, (200, 100));
        let pipes: Vec<Pipe> = p.batches.iter().map(|b| b.pipe).collect();
        assert_eq!(pipes, [Pipe::Solid, Pipe::Glyph, Pipe::Solid]);
        let g = &p.batches[1];
        let first = p.verts[g.first as usize].color;
        let last = p.verts[(g.first + g.count - 1) as usize].color;
        assert_eq!((first, last), (Color::BLACK.0, Color::WHITE.0));
    }

    #[test]
    fn clips_nest_intersect_and_scale() {
        let mut s = Scene::new(2.0);
        s.clipped(Rect::new(10.0, 10.0, 100.0, 100.0), |s| {
            s.clipped(Rect::new(50.0, 0.0, 200.0, 40.0), |s| {
                s.rect(Rect::new(0.0, 0.0, 500.0, 500.0), Color::WHITE);
            });
            s.rect(Rect::new(0.0, 0.0, 5.0, 5.0), Color::WHITE);
        });
        s.clipped(Rect::new(500.0, 500.0, 10.0, 10.0), |s| {
            s.rect(Rect::new(0.0, 0.0, 5.0, 5.0), Color::WHITE);
        });
        let mut p = Prepare::new(4096);
        p.run(&s, (400, 400));
        let got: Vec<[u32; 4]> = p.batches.iter().map(|b| b.scissor).collect();
        assert_eq!(got, [[100, 20, 120, 60], [20, 20, 200, 200]]);
    }

    #[test]
    fn rules_land_on_whole_pixels_at_least_one_wide() {
        let mut s = Scene::new(0.67);
        s.rect(Rect::new(10.1, 20.3, 50.0, 1.0), Color::WHITE);
        let mut p = Prepare::new(4096);
        p.run(&s, (100, 100));
        let px: Vec<f32> = p.verts.iter().map(|v| (1.0 - v.pos[1]) * 50.0).collect();
        let (top, bottom) = (px[0], px[2]);
        assert!((bottom - top - 1.0).abs() < 1e-3, "{top} {bottom}");
        assert!((top - top.round()).abs() < 1e-3);
    }

    #[test]
    fn arrows_draw_with_ring_and_blank_glyphs_draw_nothing() {
        let mut s = Scene::new(1.0);
        s.arrow(Arrow {
            x: 5.0,
            y: 5.0,
            size: 14.0,
            dir: Dir::Left,
            color: Color::WHITE,
            ring: Some(Ring::BLACK_1PX),
        });
        let mut p = Prepare::new(4096);
        p.run(&s, (64, 64));
        assert_eq!(p.verts.len(), 12);
        p.run(&page(&["   ".to_string()], 28.0), (64, 64));
        assert!(p.verts.is_empty());
    }

    fn ja_chars(from: u32, n: u32) -> String {
        (0..n)
            .filter_map(|i| char::from_u32(0x4e00 + from + i))
            .collect()
    }

    // Text with every glyph the UI font has, cut in lines.
    fn all_glyphs(start: usize) -> Vec<String> {
        let f = crate::text::font(Face::Serif);
        let map = f.charmap();
        let mut chars: Vec<char> = (0x21..0x9fffu32)
            .filter_map(char::from_u32)
            .filter(|&c| map.map(c) != 0)
            .collect();
        let n = chars.len();
        chars.rotate_left(start % n);
        chars.chunks(30).map(|c| c.iter().collect()).collect()
    }

    #[test]
    fn eviction_keeps_the_atlas_size_and_every_glyph_of_the_frame() {
        let mut p = Prepare::new(4096);
        let page_of = |n: usize| {
            page(
                &all_glyphs(n * 400).into_iter().take(12).collect::<Vec<_>>(),
                48.0,
            )
        };
        for n in 0..40 {
            p.run(&page_of(n), (1920, 1080));
            assert_eq!(p.dropped, 0);
            assert_eq!(p.atlas.size(), ATLAS_START, "page {n}");
        }
        assert!(p.atlas.evicted_shelves > 0, "pages never filled the atlas");
    }

    #[test]
    fn one_huge_frame_grows_once_then_drops_and_counts() {
        let every = all_glyphs(0);
        let mut p = Prepare::new(4096);
        p.run(&page(&every, 400.0), (1920, 1080));
        assert_eq!(p.atlas.size(), ATLAS_MAX);
        assert!(p.dropped > 0, "the frame should not fit 4096");
        let mut small = Prepare::new(2048);
        small.run(&page(&every, 400.0), (1920, 1080));
        assert_eq!(small.atlas.size(), 2048, "device limit");
        assert!(small.dropped > 0);
    }

    #[test]
    fn a_repeated_frame_uploads_nothing() {
        let s = page(&[ja_chars(0, 5), "Звук".to_string()], 28.0);
        let mut p = Prepare::new(4096);
        p.run(&s, (640, 480));
        assert!(!p.atlas.uploads.is_empty());
        p.atlas.uploads.clear();
        p.run(&s, (640, 480));
        assert!(p.atlas.uploads.is_empty());
    }
}
