//! Touch layout editor in the port menu: move, resize, opacity, show / hide and reset of the overlay controls.

use super::overlay::{button, label, white};
use super::touch::{Control, Ctl, Screen, TouchConfig, TouchLayout, TouchSource};
use crate::i18n::{self, Lang};
use crate::theme;
use egui::{
    Align2, Color32, FontId, Id, Order, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2,
};
use th08_render::gfx::SafeArea;

pub enum EditOut {
    Save(TouchLayout),
    Cancel,
}

pub struct TouchEdit {
    src: TouchSource,
    selected: Option<Ctl>,
}

impl TouchEdit {
    pub fn new(cfg: TouchConfig) -> Self {
        Self {
            src: TouchSource::new(cfg),
            selected: None,
        }
    }

    pub fn layout(&self) -> &TouchLayout {
        &self.src.cfg.layout
    }

    // Window size (px), winit scale factor and safe area: the same placement as the game's overlay.
    pub fn set_window(&mut self, (w, h): (u32, u32), scale_factor: f64, safe: SafeArea) {
        self.src.set_window(w, h, scale_factor, safe);
    }

    fn control(&self, ctl: Ctl) -> Option<Control> {
        let l = self.src.landscape();
        self.src
            .cfg
            .layout
            .controls(l)
            .iter()
            .copied()
            .find(|c| c.ctl == ctl)
    }

    // The controls of this movement mode, the Move zone first (buttons over it win the hit test).
    fn list(&self, land: bool) -> Vec<Control> {
        let mut list: Vec<Control> = self
            .src
            .cfg
            .layout
            .controls(land)
            .iter()
            .filter(|c| self.src.uses(c.ctl))
            .copied()
            .collect();
        list.sort_by_key(|c| c.ctl != Ctl::Move);
        list
    }

    // One pass; `ppp`: window px per egui point.
    pub fn ui(&mut self, ctx: &egui::Context, lang: Lang, ppp: f32) -> Option<EditOut> {
        let pt = |p: [f32; 2]| pos2(p[0] / ppp, p[1] / ppp);
        let land = self.src.landscape();
        let list = self.list(land);
        let screen = ctx.content_rect();
        egui::Area::new(Id::unique("touch edit area"))
            .order(Order::Background)
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                ui.allocate_rect(screen, Sense::hover());
                ui.painter()
                    .rect_filled(screen, 0.0, Color32::from_black_alpha(90));
                for c in &list {
                    let p = self.src.place(c);
                    let rect = Rect::from_center_size(
                        pt(p.centre),
                        vec2(2.0 * p.half[0] / ppp, 2.0 * p.half[1] / ppp),
                    );
                    let r = ui.interact(
                        rect,
                        Id::unique(("touch edit", c.ctl.name())),
                        Sense::click_and_drag(),
                    );
                    if r.clicked() || r.drag_started() {
                        self.selected = Some(c.ctl);
                    }
                    if r.dragged() {
                        let d = r.drag_delta() * ppp;
                        self.src.edit_move(c.ctl, [d.x, d.y]);
                    }
                }
                if let Some(s) = self.selected {
                    let z = ui.input(egui::InputState::zoom_delta);
                    if z != 1.0 {
                        self.src.edit_scale(s, z);
                    }
                }
                self.draw(ui.painter(), lang, ppp);
            });
        self.panel(ctx, lang, land)
    }

    fn draw(&self, p: &egui::Painter, lang: Lang, ppp: f32) {
        let pt = |v: [f32; 2]| pos2(v[0] / ppp, v[1] / ppp);
        let [x0, y0, w, h] = self.src.rect();
        p.rect_stroke(
            Rect::from_min_size(pt([x0, y0]), vec2(w / ppp, h / ppp)),
            0.0,
            Stroke::new(1.0, white(0.35)),
            StrokeKind::Inside,
        );
        let list = self.list(self.src.landscape());
        for c in &list {
            let pl = self.src.place(c);
            let centre = pt(pl.centre);
            let half = vec2(pl.half[0] / ppp, pl.half[1] / ppp);
            let shown = if c.visible { 1.0 } else { 0.35 };
            let a = (self.src.cfg.opacity * c.opacity).max(0.25) * shown;
            let sel = self.selected == Some(c.ctl);
            if c.ctl == Ctl::Move {
                let r = Rect::from_center_size(centre, half * 2.0);
                p.rect(
                    r,
                    6.0,
                    white(a * 0.1),
                    Stroke::new(2.0, white(a * 0.6)),
                    StrokeKind::Inside,
                );
                p.text(
                    centre,
                    Align2::CENTER_CENTER,
                    label(lang, c.ctl, Screen::Play),
                    FontId::proportional(18.0),
                    white(a.max(0.3) * 1.6),
                );
                if sel {
                    p.rect_stroke(r, 6.0, Stroke::new(3.0, theme::FOCUS), StrokeKind::Inside);
                }
            } else {
                button(
                    p,
                    centre,
                    half.x,
                    label(lang, c.ctl, Screen::Play),
                    a,
                    false,
                );
                if sel {
                    p.circle_stroke(centre, half.x + 3.0, Stroke::new(3.0, theme::FOCUS));
                }
            }
            if !c.visible {
                p.line_segment(
                    [centre - half * 0.6, centre + half * 0.6],
                    Stroke::new(2.0, white(0.6)),
                );
            }
        }
    }

    fn panel(&mut self, ctx: &egui::Context, lang: Lang, land: bool) -> Option<EditOut> {
        let mut out = None;
        let screen = ctx.content_rect();
        egui::Window::new(i18n::ui(lang, "touch_edit"))
            .id(Id::unique("touch edit panel"))
            .collapsible(true)
            .resizable(false)
            .default_pos(pos2(screen.center().x, screen.top() + 8.0))
            .pivot(Align2::CENTER_TOP)
            .show(ctx, |ui| {
                let o = if land {
                    "touch_landscape"
                } else {
                    "touch_portrait"
                };
                ui.label(RichText::new(i18n::ui(lang, o)).weak());
                ui.add_space(4.0);
                match self.selected.and_then(|s| self.control(s)) {
                    Some(c) => self.control_ui(ui, lang, c),
                    None => {
                        ui.label(i18n::ui(lang, "touch_hint"));
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(i18n::ui(lang, "touch_reset")).clicked() {
                        let d = TouchLayout::preset(self.src.cfg.preset);
                        *self.src.cfg.layout.controls_mut(land) = d.controls(land).to_vec();
                    }
                    if ui.button(i18n::ui(lang, "touch_mirror")).clicked() {
                        self.src.cfg.layout.mirror(land);
                    }
                    if ui.button(i18n::ui(lang, "cancel")).clicked() {
                        out = Some(EditOut::Cancel);
                    }
                    if ui
                        .add(theme::button(i18n::ui(lang, "save"), theme::Kind::Primary))
                        .clicked()
                    {
                        out = Some(EditOut::Save(self.src.cfg.layout.clone()));
                    }
                });
            });
        out
    }

    fn control_ui(&mut self, ui: &mut egui::Ui, lang: Lang, c: Control) {
        let ctl = c.ctl;
        ui.label(RichText::new(label(lang, ctl, Screen::Play)).strong());
        egui::Grid::new("touch edit grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label(i18n::ui(lang, "touch_size"));
                if ctl == Ctl::Move {
                    let (mut w, mut h) = (c.w * 100.0, c.h * 100.0);
                    let a = ui.add(
                        egui::Slider::new(&mut w, 10.0..=100.0)
                            .suffix(" %")
                            .prefix("↔ "),
                    );
                    ui.end_row();
                    ui.label("");
                    let b = ui.add(
                        egui::Slider::new(&mut h, 10.0..=100.0)
                            .suffix(" %")
                            .prefix("↕ "),
                    );
                    if a.changed() || b.changed() {
                        self.src.edit(ctl, |c| {
                            c.w = w / 100.0;
                            c.h = h / 100.0;
                        });
                    }
                } else {
                    let mut mm = c.w;
                    let [lo, hi] = ctl.mm_range();
                    let r = ui.add(egui::Slider::new(&mut mm, lo..=hi).suffix(" mm"));
                    if r.changed() {
                        self.src.edit(ctl, |c| {
                            c.w = mm;
                            c.h = mm;
                        });
                    }
                }
                ui.end_row();
                ui.label(i18n::ui(lang, "touch_opacity"));
                let mut op = c.opacity * 100.0;
                if ui
                    .add(egui::Slider::new(&mut op, 0.0..=100.0).suffix(" %"))
                    .changed()
                {
                    self.src.edit(ctl, |c| c.opacity = op / 100.0);
                }
                ui.end_row();
                let mut vis = c.visible;
                if ui
                    .checkbox(&mut vis, i18n::ui(lang, "touch_shown"))
                    .changed()
                {
                    self.src.edit(ctl, |c| c.visible = vis);
                }
                ui.end_row();
            });
    }
}
