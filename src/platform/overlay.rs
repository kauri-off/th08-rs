//! Touch overlay drawing: an egui painter-only pass onto the window target after the blit, never into the frame.

use super::touch::{Ctl, Screen, TouchView};
use crate::i18n::{self, Lang};
use egui::{Align2, Color32, FontId, LayerId, Order, Pos2, Rect, Stroke, pos2, vec2};
use th08_render::gfx::{Overlay, OverlayTarget};

// egui context and renderer of the overlay; the renderer is made on the first paint (it needs the target format).
pub struct TouchOverlay {
    pub view: TouchView,
    lang: Lang,
    // The `font` setting, tried first for the CJK face of Japanese labels.
    font: Option<String>,
    ctx: egui::Context,
    renderer: Option<(wgpu::TextureFormat, egui_wgpu::Renderer)>,
    // Texture frees owed from the last paint.
    free: Vec<egui::TextureId>,
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

impl TouchOverlay {
    pub fn new(lang: Lang, font: Option<String>) -> Self {
        let mut o = Self {
            view: TouchView::default(),
            lang,
            font,
            ctx: egui::Context::default(),
            renderer: None,
            free: Vec::new(),
        };
        o.fresh_context();
        o
    }

    fn fresh_context(&mut self) {
        self.ctx = egui::Context::default();
        if self.lang == Lang::Ja {
            i18n::install_fonts(&self.ctx, self.font.as_deref());
        }
    }

    // The shapes of `view` in window pixels (1 point = 1 px).
    fn shapes(&self, (w, h): (u32, u32)) -> egui::FullOutput {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(w as f32, h as f32))),
            ..Default::default()
        };
        let v = &self.view;
        self.ctx.run_pass(raw, |ctx| {
            let p = ctx.layer_painter(LayerId::new(Order::Foreground, egui::Id::unique("touch")));
            for (c, lit) in &v.controls {
                let a = v.opacity * c.opacity;
                let centre = pos(c.centre);
                if c.ctl == Ctl::Move {
                    // The movement zone is invisible (it covers part of the game picture); only the finger / stick shows.
                    continue;
                }
                button(
                    &p,
                    centre,
                    c.half[0],
                    label(self.lang, c.ctl, v.screen),
                    a,
                    *lit,
                );
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
        })
    }
}

impl Overlay for TouchOverlay {
    fn paint(&mut self, t: OverlayTarget<'_>, enc: &mut wgpu::CommandEncoder) {
        if self.renderer.as_ref().is_none_or(|(f, _)| *f != t.format) {
            let r =
                egui_wgpu::Renderer::new(t.device, t.format, egui_wgpu::RendererOptions::default());
            self.renderer = Some((t.format, r));
            // A new renderer has no textures: a fresh context sends the font atlas again.
            self.fresh_context();
            self.free.clear();
        }
        let mut out = self.shapes(t.size);
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
            size_in_pixels: [t.size.0, t.size.1],
            pixels_per_point: out.pixels_per_point,
        };
        let extra = renderer.update_buffers(t.device, t.queue, enc, &jobs, &desc);
        if !extra.is_empty() {
            t.queue.submit(extra);
        }
        {
            let mut pass = enc
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("touch overlay"),
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

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    // The renderer belonged to the lost device; the next paint makes a new one and a fresh context.
    fn device_rebuilt(&mut self) {
        self.renderer = None;
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
