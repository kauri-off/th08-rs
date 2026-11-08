//! Touch overlay and the port HUD (replay transport, toasts, input display): egui painter-only passes onto the window target after the blit, not the frame.

use super::touch::{Ctl, Screen, TouchView};
use crate::i18n::{self, Lang};
use crate::transport::Action;
use egui::{Align2, Color32, FontId, LayerId, Order, Pos2, Rect, Stroke, pos2, vec2};
use th08_render::gfx::{Overlay, OverlayTarget};

// An egui painter-only pass: its context and renderer; the renderer is made on the first paint (it needs the target
// format).
pub(crate) struct Pass {
    lang: Lang,
    // The `font` setting, tried first for the CJK face of Japanese labels.
    font: Option<String>,
    // The CJK face is installed (Japanese UI, or game titles shown as they are).
    cjk: bool,
    ctx: egui::Context,
    renderer: Option<(wgpu::TextureFormat, egui_wgpu::Renderer)>,
    // Texture frees owed from the last paint.
    free: Vec<egui::TextureId>,
}

pub struct TouchOverlay {
    pub view: TouchView,
    pass: Pass,
}

pub(crate) fn label(lang: Lang, c: Ctl, s: Screen) -> &'static str {
    let id = match (c, s) {
        (Ctl::Shot, Screen::Menu) => "touch_ok",
        (Ctl::Bomb, Screen::Menu) => "touch_back",
        (Ctl::Shot, Screen::Dialogue) => "touch_next",
        (Ctl::Move | Ctl::Stick, _) => "touch_move",
        (Ctl::Shot, _) => "touch_shot",
        (Ctl::Bomb, _) => "touch_bomb",
        (Ctl::Focus, _) => "touch_focus",
        (Ctl::Pause, _) => "touch_pause",
        (Ctl::Skip, _) => "touch_skip",
        (Ctl::Auto, _) => "touch_auto",
        (Ctl::Menu, _) => "touch_menu",
    };
    i18n::ui(lang, id)
}

fn pos(p: [f32; 2]) -> Pos2 {
    pos2(p[0], p[1])
}

// White / accent with alpha `a` (0–1).
#[expect(clippy::cast_sign_loss, reason = "alpha is clamped to 0–1 first")]
pub(crate) fn white(a: f32) -> Color32 {
    Color32::from_white_alpha((a.clamp(0.0, 1.0) * 255.0) as u8)
}

#[expect(clippy::cast_sign_loss, reason = "alpha is clamped to 0–1 first")]
pub(crate) fn accent(a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(255, 120, 160, (a.clamp(0.0, 1.0) * 255.0) as u8)
}

// One button: a circle of radius `r` with its label (shrunk to fit inside), alpha `a`, accent fill when lit.
pub(crate) fn button(p: &egui::Painter, centre: Pos2, r: f32, text: &str, a: f32, lit: bool) {
    let fill = if lit {
        accent(a * 0.7)
    } else {
        white(a * 0.18)
    };
    p.circle(centre, r, fill, Stroke::new(2.0, white(a * 0.8)));
    let colour = white(a.max(0.3) * 1.6);
    let mut size = (r * 0.42).clamp(8.0, 28.0);
    let width = p
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), colour)
        .size()
        .x;
    if width > r * 1.7 {
        size = (size * r * 1.7 / width).max(6.0);
    }
    p.text(
        centre,
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(size),
        colour,
    );
}

impl Pass {
    pub(crate) fn new(lang: Lang, font: Option<String>) -> Self {
        let mut p = Self {
            lang,
            font,
            cjk: lang == Lang::Ja,
            ctx: egui::Context::default(),
            renderer: None,
            free: Vec::new(),
        };
        p.fresh_context();
        p
    }

    // The renderer belonged to the lost device; the next paint makes a new one and a fresh context.
    pub(crate) fn device_rebuilt(&mut self) {
        self.renderer = None;
    }

    fn fresh_context(&mut self) {
        self.ctx = egui::Context::default();
        if self.cjk {
            i18n::install_fonts(&self.ctx, self.font.as_deref());
        }
    }

    // `shapes` on a foreground painter over the whole target (1 point = 1 px), then rendered onto it.
    pub(crate) fn paint(
        &mut self,
        t: OverlayTarget<'_>,
        enc: &mut wgpu::CommandEncoder,
        label: &str,
        shapes: impl FnOnce(&egui::Painter),
    ) {
        if self.renderer.as_ref().is_none_or(|(f, _)| *f != t.format) {
            let r =
                egui_wgpu::Renderer::new(t.device, t.format, egui_wgpu::RendererOptions::default());
            self.renderer = Some((t.format, r));
            // A new renderer has no textures: a fresh context sends the font atlas again.
            self.fresh_context();
            self.free.clear();
        }
        let (w, h) = t.size;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(w as f32, h as f32))),
            ..Default::default()
        };
        let mut shapes = Some(shapes);
        let mut out = self.ctx.run_pass(raw, |ctx| {
            let p = ctx.layer_painter(LayerId::new(Order::Foreground, egui::Id::unique(label)));
            if let Some(f) = shapes.take() {
                f(&p);
            }
        });
        let jobs = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        let (_, renderer) = self.renderer.as_mut().expect("renderer");
        for id in self.free.drain(..) {
            renderer.free_texture(&id);
        }
        for (id, deltas) in out.textures_delta.set.drain() {
            for d in &deltas {
                renderer.update_texture(t.device, t.queue, id, d);
            }
        }
        let desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [w, h],
            pixels_per_point: out.pixels_per_point,
        };
        let extra = renderer.update_buffers(t.device, t.queue, enc, &jobs, &desc);
        if !extra.is_empty() {
            t.queue.submit(extra);
        }
        {
            let mut pass = enc
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(label),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: t.view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, &jobs, &desc);
        }
        self.free.extend(out.textures_delta.free.drain());
    }
}

impl TouchOverlay {
    pub fn new(lang: Lang, font: Option<String>) -> Self {
        Self {
            view: TouchView::default(),
            pass: Pass::new(lang, font),
        }
    }
}

// The shapes of `v` (window px).
fn touch_shapes(p: &egui::Painter, v: &TouchView, lang: Lang) {
    for (c, lit) in &v.controls {
        let a = v.opacity * c.opacity;
        let centre = pos(c.centre);
        if c.ctl == Ctl::Move {
            // The movement zone is invisible (it covers part of the game picture); only the finger / stick shows.
            continue;
        }
        if c.ctl == Ctl::Stick {
            // The ring always shows; the knob comes with the finger.
            p.circle(
                centre,
                c.half[0],
                white(a * 0.12),
                Stroke::new(2.0, white(a * 0.6)),
            );
            p.circle_filled(centre, c.half[0] * 0.3, white(a * 0.25));
            continue;
        }
        button(p, centre, c.half[0], label(lang, c.ctl, v.screen), a, *lit);
    }
    if let Some((centre, finger, r)) = v.finger {
        let a = v.opacity;
        if r > 0.0 {
            p.circle(
                pos(centre),
                r,
                white(a * 0.12),
                Stroke::new(2.0, white(a * 0.6)),
            );
            p.circle_filled(pos(finger), r * 0.35, accent(a * 0.7));
        } else {
            p.line_segment([pos(centre), pos(finger)], Stroke::new(2.0, white(a * 0.4)));
            p.circle_stroke(pos(finger), 18.0, Stroke::new(2.0, accent(a)));
        }
    }
}

impl Overlay for TouchOverlay {
    fn paint(&mut self, t: OverlayTarget<'_>, enc: &mut wgpu::CommandEncoder) {
        let (v, lang) = (&self.view, self.pass.lang);
        self.pass
            .paint(t, enc, "touch overlay", |p| touch_shapes(p, v, lang));
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    // The renderer belonged to the lost device; the next paint makes a new one and a fresh context.
    fn device_rebuilt(&mut self) {
        self.pass.renderer = None;
    }
}

// Window corner of the input display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub const IDS: [&'static str; 5] = [
        "off",
        "top-left",
        "top-right",
        "bottom-left",
        "bottom-right",
    ];

    pub fn parse(v: &str) -> Option<Self> {
        Some(match v {
            "top-left" => Self::TopLeft,
            "top-right" => Self::TopRight,
            "bottom-left" => Self::BottomLeft,
            "bottom-right" => Self::BottomRight,
            _ => return None,
        })
    }
}

// The input display: the game word of the last logic frame, where and how visible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputView {
    pub word: u16,
    pub corner: Corner,
    pub opacity: f32,
}

// What the port HUD shows: the transport speed label, the touch strip (window px) while a replay plays, toasts and
// the input display.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TransportView {
    pub label: Option<String>,
    pub strip: Option<[(Action, [f32; 4]); 4]>,
    pub paused: bool,
    // A save write failure or screenshot result, shown at the bottom for a few seconds.
    pub toast: Option<String>,
    // The BGM title that just started, shown top left for a few seconds.
    pub now_playing: Option<String>,
    pub inputs: Option<InputView>,
}

impl TransportView {
    fn is_empty(&self) -> bool {
        self.label.is_none()
            && self.strip.is_none()
            && self.toast.is_none()
            && self.now_playing.is_none()
            && self.inputs.is_none()
    }
}

pub struct TransportOverlay {
    pub view: TransportView,
    // The safe rect `[x, y, w, h]` (window px) the corners are taken from.
    pub safe: [f32; 4],
    pass: Pass,
}

impl TransportOverlay {
    // `cjk`: install the CJK face whatever the UI language (music titles are Japanese without a pack).
    pub fn new(lang: Lang, font: Option<String>, cjk: bool) -> Self {
        let mut pass = Pass::new(lang, font);
        if cjk && !pass.cjk {
            pass.cjk = true;
            pass.fresh_context();
        }
        Self {
            view: TransportView::default(),
            safe: [0.0; 4],
            pass,
        }
    }
}

// The input display: a d-pad of four arrows and the shot / bomb / focus / pause keys, lit while held.
fn input_shapes(p: &egui::Painter, v: &InputView, safe: [f32; 4], lang: Lang) {
    use th08_core::input::{BOMB, DOWN, FOCUS, LEFT, PAUSE, RIGHT, SHOT, UP};
    let a = v.opacity;
    let u = (safe[3] / 32.0).clamp(10.0, 36.0);
    let (w, h) = (u * 9.6, u * 3.4);
    let m = u * 0.5;
    let x = match v.corner {
        Corner::TopLeft | Corner::BottomLeft => safe[0] + m,
        Corner::TopRight | Corner::BottomRight => safe[0] + safe[2] - w - m,
    };
    let y = match v.corner {
        Corner::TopLeft | Corner::TopRight => safe[1] + m,
        Corner::BottomLeft | Corner::BottomRight => safe[1] + safe[3] - h - m,
    };
    p.rect_filled(
        Rect::from_min_size(pos2(x, y), vec2(w, h)),
        u * 0.4,
        Color32::from_black_alpha(white(a * 0.55).a()),
    );
    let on = |bit: u16| v.word & bit != 0;
    let fill = |lit: bool| {
        if lit {
            accent(a * 0.9)
        } else {
            white(a * 0.18)
        }
    };
    let stroke = Stroke::new(1.5, white(a * 0.8));
    let c = pos2(x + u * 1.7, y + h / 2.0);
    let s = u * 0.55;
    for (bit, dx, dy) in [
        (UP, 0.0, -1.0),
        (DOWN, 0.0, 1.0),
        (LEFT, -1.0, 0.0),
        (RIGHT, 1.0, 0.0),
    ] {
        let tip = c + vec2(dx, dy) * (s * 2.6);
        let base = c + vec2(dx, dy) * (s * 0.9);
        let side = vec2(-dy, dx) * s;
        p.add(egui::Shape::convex_polygon(
            vec![tip, base + side, base - side],
            fill(on(bit)),
            stroke,
        ));
    }
    let keys = [
        (SHOT, "touch_shot"),
        (BOMB, "touch_bomb"),
        (FOCUS, "touch_focus"),
        (PAUSE, "touch_pause"),
    ];
    let (kw, kh) = (u * 2.6, u * 1.3);
    for (i, (bit, id)) in keys.into_iter().enumerate() {
        let col = (i % 2) as f32;
        let row = (i / 2) as f32;
        let r = Rect::from_min_size(
            pos2(
                x + u * 3.8 + col * (kw + u * 0.3),
                y + u * 0.3 + row * (kh + u * 0.2),
            ),
            vec2(kw, kh),
        );
        p.rect(r, u * 0.3, fill(on(bit)), stroke, egui::StrokeKind::Inside);
        let text = i18n::ui(lang, id);
        let mut size = u * 0.7;
        let tw = p
            .layout_no_wrap(text.to_owned(), FontId::proportional(size), white(1.0))
            .size()
            .x;
        if tw > kw * 0.9 {
            size *= kw * 0.9 / tw;
        }
        p.text(
            r.center(),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(size),
            white(a.max(0.4)),
        );
    }
}

// One strip button: a rounded square with the action's icon drawn as shapes (no font glyphs needed).
fn strip_button(p: &egui::Painter, a: Action, r: [f32; 4], paused: bool) {
    let rect = Rect::from_min_size(pos2(r[0], r[1]), vec2(r[2], r[3]));
    p.rect(
        rect,
        r[2] * 0.2,
        white(0.18),
        Stroke::new(2.0, white(0.7)),
        egui::StrokeKind::Inside,
    );
    let c = rect.center();
    let s = r[2] * 0.18;
    let fill = white(0.9);
    let tri = |x: f32, right: bool| {
        let (tip, back) = if right {
            (x + s, x - s)
        } else {
            (x - s, x + s)
        };
        egui::Shape::convex_polygon(
            vec![pos2(back, c.y - s), pos2(tip, c.y), pos2(back, c.y + s)],
            fill,
            Stroke::NONE,
        )
    };
    let bar = |x: f32| {
        egui::Shape::rect_filled(
            Rect::from_center_size(pos2(x, c.y), vec2(s * 0.5, s * 2.0)),
            0.0,
            fill,
        )
    };
    match a {
        Action::Pause if paused => {
            p.add(tri(c.x, true));
        }
        Action::Pause => {
            p.add(bar(c.x - s * 0.5));
            p.add(bar(c.x + s * 0.5));
        }
        Action::Step => {
            p.add(tri(c.x - s * 0.4, true));
            p.add(bar(c.x + s * 0.9));
        }
        Action::Slower => {
            p.add(tri(c.x - s * 0.5, false));
            p.add(tri(c.x + s * 0.5, false));
        }
        Action::Faster => {
            p.add(tri(c.x - s * 0.5, true));
            p.add(tri(c.x + s * 0.5, true));
        }
    }
}

fn transport_shapes(
    p: &egui::Painter,
    v: &TransportView,
    (w, h): (u32, u32),
    safe: [f32; 4],
    lang: Lang,
) {
    if let Some(i) = &v.inputs {
        input_shapes(p, i, safe, lang);
    }
    if let Some(text) = &v.now_playing {
        let size = (h as f32 / 30.0).clamp(12.0, 26.0);
        let g = crate::theme::wrap(
            p,
            text.clone(),
            FontId::proportional(size),
            white(1.0),
            w as f32 * 0.6,
        );
        let top = v
            .inputs
            .filter(|i| i.corner == Corner::TopLeft)
            .map_or(0.0, |_| (safe[3] / 32.0).clamp(10.0, 36.0) * 4.2);
        let at = pos2(safe[0] + size, safe[1] + size * 0.8 + top);
        let bg = Rect::from_min_size(at, g.size()).expand2(vec2(size * 0.6, size * 0.3));
        p.rect_filled(bg, size * 0.3, Color32::from_black_alpha(170));
        p.galley(at, g, white(1.0));
    }
    if let Some(strip) = &v.strip {
        for (a, r) in strip {
            strip_button(p, *a, *r, v.paused);
        }
    }
    if let Some(text) = &v.label {
        let size = (h as f32 / 18.0).clamp(14.0, 40.0);
        let y = v
            .strip
            .as_ref()
            .map_or(size * 0.8, |s| s[0].1[1] + s[0].1[3] + size * 0.4);
        let at = pos2(w as f32 / 2.0, y);
        let font = FontId::proportional(size);
        let g = p.layout_no_wrap(text.clone(), font.clone(), white(1.0));
        let bg = Rect::from_center_size(
            at + vec2(0.0, g.size().y / 2.0),
            g.size() + vec2(size, size * 0.4),
        );
        p.rect_filled(bg, size * 0.3, Color32::from_black_alpha(150));
        p.text(at, Align2::CENTER_TOP, text, font, white(1.0));
    }
    if let Some(text) = &v.toast {
        let size = (h as f32 / 26.0).clamp(12.0, 28.0);
        let g = crate::theme::wrap(
            p,
            text.clone(),
            FontId::proportional(size),
            white(1.0),
            w as f32 * 0.8,
        );
        let at = pos2(
            (w as f32 - g.size().x) / 2.0,
            h as f32 - g.size().y - size * 1.5,
        );
        let bg = Rect::from_min_size(at, g.size()).expand2(vec2(size * 0.6, size * 0.3));
        p.rect_filled(
            bg,
            size * 0.3,
            Color32::from_rgba_unmultiplied(120, 20, 20, 210),
        );
        p.galley(at, g, white(1.0));
    }
}

impl Overlay for TransportOverlay {
    fn paint(&mut self, t: OverlayTarget<'_>, enc: &mut wgpu::CommandEncoder) {
        if self.view.is_empty() {
            return;
        }
        let (v, size, safe, lang) = (&self.view, t.size, self.safe, self.pass.lang);
        self.pass.paint(t, enc, "port hud", |p| {
            transport_shapes(p, v, size, safe, lang);
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn device_rebuilt(&mut self) {
        self.pass.renderer = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Labels follow the screen; the layout editor's play labels; every language has its own text.
    #[test]
    fn labels_per_screen() {
        let en = |c, s| label(Lang::En, c, s);
        assert_eq!(en(Ctl::Shot, Screen::Menu), "OK");
        assert_eq!(en(Ctl::Bomb, Screen::Menu), "BACK");
        assert_eq!(en(Ctl::Shot, Screen::Play), "SHOT");
        assert_eq!(en(Ctl::Bomb, Screen::Play), "BOMB");
        assert_eq!(en(Ctl::Shot, Screen::Dialogue), "NEXT");
        assert_eq!(en(Ctl::Bomb, Screen::Dialogue), "BOMB");
        assert_eq!(en(Ctl::Skip, Screen::Dialogue), "SKIP");
        assert_eq!(en(Ctl::Menu, Screen::Menu), "MENU");
        assert_eq!(label(Lang::Ja, Ctl::Bomb, Screen::Menu), "戻る");
        assert_eq!(label(Lang::Ru, Ctl::Shot, Screen::Dialogue), "ДАЛЕЕ");
        for s in [Screen::Menu, Screen::Play, Screen::Dialogue] {
            assert!(Ctl::Menu.shown(s) && Ctl::Shot.shown(s) && Ctl::Bomb.shown(s));
            for c in Ctl::ALL {
                for lang in Lang::ALL {
                    assert!(!label(lang, c, s).is_empty());
                }
            }
        }
        assert!(Ctl::Skip.shown(Screen::Dialogue) && !Ctl::Skip.shown(Screen::Menu));
        assert!(!Ctl::Focus.shown(Screen::Menu) && !Ctl::Auto.shown(Screen::Menu));
    }
}
