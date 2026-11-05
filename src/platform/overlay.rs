//! Touch overlay and replay transport HUD: egui painter-only passes onto the window target after the blit, not the frame.

use super::touch::{Ctl, Screen, TouchView};
use crate::i18n::{self, Lang};
use crate::transport::Action;
use egui::{Align2, Color32, FontId, LayerId, Order, Pos2, Rect, Stroke, pos2, vec2};
use th08_render::gfx::{Overlay, OverlayTarget};

// An egui painter-only pass: its context and renderer; the renderer is made on the first paint (it needs the target
// format).
struct Pass {
    lang: Lang,
    // The `font` setting, tried first for the CJK face of Japanese labels.
    font: Option<String>,
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
        (Ctl::Move, _) => "touch_move",
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
    fn new(lang: Lang, font: Option<String>) -> Self {
        let mut p = Self {
            lang,
            font,
            ctx: egui::Context::default(),
            renderer: None,
            free: Vec::new(),
        };
        p.fresh_context();
        p
    }

    fn fresh_context(&mut self) {
        self.ctx = egui::Context::default();
        if self.lang == Lang::Ja {
            i18n::install_fonts(&self.ctx, self.font.as_deref());
        }
    }

    // `shapes` on a foreground painter over the whole target (1 point = 1 px), then rendered onto it.
    fn paint(
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

// What the replay transport HUD shows: the speed label, and the touch strip (window px) while a replay plays.
#[derive(Clone, Debug, Default)]
pub struct TransportView {
    pub label: Option<String>,
    pub strip: Option<[(Action, [f32; 4]); 4]>,
    pub paused: bool,
}

pub struct TransportOverlay {
    pub view: TransportView,
    pass: Pass,
}

impl TransportOverlay {
    pub fn new(lang: Lang, font: Option<String>) -> Self {
        Self {
            view: TransportView::default(),
            pass: Pass::new(lang, font),
        }
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

fn transport_shapes(p: &egui::Painter, v: &TransportView, (w, h): (u32, u32)) {
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
}

impl Overlay for TransportOverlay {
    fn paint(&mut self, t: OverlayTarget<'_>, enc: &mut wgpu::CommandEncoder) {
        if self.view.label.is_none() && self.view.strip.is_none() {
            return;
        }
        let (v, size) = (&self.view, t.size);
        self.pass
            .paint(t, enc, "replay transport", |p| transport_shapes(p, v, size));
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
