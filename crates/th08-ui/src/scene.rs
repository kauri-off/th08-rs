//! One frame of port UI as plain data, in points (`Scene::scale` physical px each).

#[derive(Clone, Copy, Debug, PartialEq)]
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

// Straight (not premultiplied) RGBA.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color(pub [u8; 4]);

impl Color {
    pub const BLACK: Self = Self([0, 0, 0, 255]);
    pub const WHITE: Self = Self([255, 255, 255, 255]);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self([r, g, b, 255])
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self([r, g, b, a])
    }

    // The same colour at `k` times its opacity.
    #[must_use]
    #[expect(clippy::cast_sign_loss, reason = "k is clamped to 0..=1")]
    pub fn fade(self, k: f32) -> Self {
        let [r, g, b, a] = self.0;
        Self([r, g, b, (f32::from(a) * k.clamp(0.0, 1.0)).round() as u8])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    Serif,
    Sans,
}

// An outline around text or an arrow; `width` is in physical px on every screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub color: Color,
    pub width: f32,
}

impl Ring {
    pub const BLACK_1PX: Self = Self {
        color: Color::BLACK,
        width: 1.0,
    };
}

// One line of text; `baseline` is its y, wrapping belongs to the layout (`text::wrap_lines`).
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub x: f32,
    pub baseline: f32,
    pub text: String,
    pub face: Face,
    pub size: f32,
    pub color: Color,
    pub ring: Option<Ring>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

// A filled triangle in a `size` × `size` box with its corner at (x, y).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arrow {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub dir: Dir,
    pub color: Color,
    pub ring: Option<Ring>,
}

// A rounded rect, filled, or outlined `stroke` physical px wide inside its edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Round {
    pub rect: Rect,
    pub radius: f32,
    pub color: Color,
    pub stroke: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Rect { rect: Rect, color: Color },
    Round(Round),
    Text(Text),
    Arrow(Arrow),
    PushClip(Rect),
    PopClip,
}

// An area that takes pointer input; `tag` is the layout's own meaning of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub rect: Rect,
    pub tag: u32,
}

impl Hit {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let r = self.rect;
        x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
    }
}

// Commands in painter's order; clips nest and intersect. Hits are not drawn; a later one lies above an earlier one.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub scale: f32,
    pub cmds: Vec<Cmd>,
    pub hits: Vec<Hit>,
}

impl Scene {
    pub fn new(scale: f32) -> Self {
        Self {
            scale,
            cmds: Vec::new(),
            hits: Vec::new(),
        }
    }

    pub fn round(&mut self, round: Round) {
        self.cmds.push(Cmd::Round(round));
    }

    pub fn hit(&mut self, rect: Rect, tag: u32) {
        self.hits.push(Hit { rect, tag });
    }

    // The topmost hit area under (x, y), in points.
    pub fn hit_at(&self, x: f32, y: f32) -> Option<Hit> {
        self.hits.iter().rev().find(|h| h.contains(x, y)).copied()
    }

    pub fn rect(&mut self, rect: Rect, color: Color) {
        self.cmds.push(Cmd::Rect { rect, color });
    }

    pub fn text(&mut self, text: Text) {
        self.cmds.push(Cmd::Text(text));
    }

    pub fn arrow(&mut self, arrow: Arrow) {
        self.cmds.push(Cmd::Arrow(arrow));
    }

    // Everything `body` adds is clipped to `rect`.
    pub fn clipped(&mut self, rect: Rect, body: impl FnOnce(&mut Self)) {
        self.cmds.push(Cmd::PushClip(rect));
        body(self);
        self.cmds.push(Cmd::PopClip);
    }
}
