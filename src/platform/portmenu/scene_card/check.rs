use super::super::{Effect, Input, Shot, ShotAsk, layout as window_layout, shot_texture};
use super::*;
use crate::settings::Settings;
use crate::settings_store::SettingsStore;
use crate::uicheck::{Issue, Kind};
use th08_render::gfx::Gfx;
use th08_ui::scene::Cmd;
use th08_ui::text::Shaper;
use winit::dpi::PhysicalPosition;
use winit::event::{DeviceId, Touch, TouchPhase, WindowEvent};

pub struct Rig {
    shot: Shot,
    model: MenuModel,
    lay: Layout,
    size: (u32, u32),
    safe: SafeArea,
    shaper: Shaper,
    hitboxes: super::super::Hitboxes,
    pointer: pointer::Pointer,
}

impl Rig {
    pub fn new(s: &Settings, size: (u32, u32), safe: SafeArea, shot: &Shot) -> Self {
        let lay = window_layout(
            safe,
            size,
            shot.density.max(1.0),
            shot.touch,
            s.blit.integer,
        );
        let mut rig = Self {
            model: MenuModel::new(
                SettingsStore::port_menu(s, shot.touch),
                if shot.touch {
                    Modality::Touch
                } else {
                    Modality::Keys
                },
            ),
            shot: shot.clone(),
            lay,
            size,
            safe,
            hitboxes: s.hitboxes,
            shaper: Shaper::new(),
            pointer: pointer::Pointer::default(),
        };
        rig.model.set_context(rig.state().context(&lay));
        if let Some(ask) = shot.ask {
            rig.model.open_ask(match ask {
                ShotAsk::Retry => Ask::Retry,
                ShotAsk::Quit => Ask::Quit,
            });
        }
        for &nav in &shot.navs {
            rig.model.apply(Input::Nav(nav));
        }
        rig
    }

    fn state(&self) -> MenuState<'_> {
        MenuState {
            pad: self.shot.pad.as_deref(),
            touch: self.shot.touch,
            cheats: &[],
            hitboxes: self.hitboxes,
            status: self.shot.status.as_deref(),
            speed: self.shot.speed,
            no_saves: self.shot.no_saves,
            quit: self.shot.quit,
            stage: self.shot.stage.as_ref(),
        }
    }

    pub fn scene(&self) -> Scene {
        layout(
            &self.model,
            &Metrics {
                lay: self.lay,
                size: self.size,
                safe: self.safe,
                state: self.state(),
                pressed: self.pointer.pressed(),
            },
            &self.shaper,
        )
    }

    fn event(&mut self, ev: &WindowEvent, scene: &Scene) -> Vec<Effect> {
        self.pointer.event(ev, scene, &mut self.model)
    }

    pub fn tap_item(&mut self, item: Item) -> Vec<Effect> {
        self.tap(tag(item, ROW))
    }

    pub fn tap_answer(&mut self, yes: bool) -> Vec<Effect> {
        self.tap(if yes { ACCEPT } else { KEEP })
    }

    pub fn asking(&self) -> bool {
        self.model.ask().is_some()
    }

    fn tap(&mut self, tag: u32) -> Vec<Effect> {
        let scene = self.scene();
        let at = center(&scene, tag);
        self.event(&touch(TouchPhase::Started, at), &scene);
        self.event(&touch(TouchPhase::Ended, at), &scene)
    }

    pub fn issues(&self, scene: &Scene) -> Vec<Issue> {
        let metrics = Metrics {
            lay: self.lay,
            size: self.size,
            safe: self.safe,
            state: self.state(),
            pressed: None,
        };
        let g = Geometry::new(&metrics);
        let column = self.lay.shape == Shape::Column;
        let safe = if column {
            Rect::new(g.field.x, g.field.y, g.field.w, g.bottom - g.field.y)
        } else {
            Rect::new(
                self.safe.left as f32 / scene.scale,
                g.top,
                g.window.w - (self.safe.left + self.safe.right) as f32 / scene.scale,
                g.bottom - g.top,
            )
        };
        let mut issues = Vec::new();
        let mut note = |kind, what, rect: Rect| {
            issues.push(Issue {
                kind,
                what,
                rect: egui::Rect::from_min_size(
                    egui::pos2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                ),
            });
        };
        let mut text_bounds = Vec::new();
        for cmd in &scene.cmds {
            if let Cmd::Text(t) = cmd {
                if t.text.is_empty() {
                    continue;
                }
                let line = self.shaper.line(t.face, t.size);
                let rect = Rect::new(
                    t.x,
                    t.baseline - line.ascent,
                    self.shaper.width(&t.text, t.face, t.size),
                    line.height,
                );
                if !inside(rect, safe) {
                    note(Kind::Clipped, t.text.clone(), rect);
                }
                for (other, text) in &text_bounds {
                    if overlap(rect, *other) {
                        note(Kind::Overlap, format!("{} / {text}", t.text), rect);
                    }
                }
                text_bounds.push((rect, &t.text));
            }
        }
        let min = if self.lay.touch {
            48.0 * self.lay.density / scene.scale
        } else {
            0.0
        };
        for hit in &scene.hits {
            let r = hit.rect;
            let reachable = [0.02, 0.5, 0.98]
                .into_iter()
                .any(|x| scene.hit_at(r.x + x * r.w, r.y + 0.5 * r.h) == Some(*hit));
            if !inside(r, safe)
                || r.h + 0.02 < min
                || !reachable
                || target(hit.tag, &self.model).is_none()
            {
                note(
                    Kind::Unreachable,
                    format!("hit {:#x} outside bounds, too short or covered", hit.tag),
                    r,
                );
            }
            if !matches!(hit.tag, KEEP | ACCEPT)
                && !matches!(hit.tag & 255, ROW | TRACK_TAG)
                && hit.rect.w + 0.02 < min
            {
                note(
                    Kind::Unreachable,
                    format!("control {:#x} too narrow", hit.tag),
                    hit.rect,
                );
            }
        }
        let expected: Vec<_> = if self.asking() {
            vec![KEEP, ACCEPT]
        } else {
            self.model
                .items()
                .into_iter()
                .map(|it| tag(it, ROW))
                .collect()
        };
        for code in expected {
            let reachable = scene.hits.iter().filter(|h| h.tag == code).count() == 1;
            if !reachable {
                note(
                    Kind::Unreachable,
                    format!("missing or duplicate target {code:#x}"),
                    safe,
                );
            }
        }
        let focus_count = scene
            .cmds
            .iter()
            .filter(|cmd| matches!(cmd, Cmd::Rect { color: ACCENT, rect } if rect.w == 4.0))
            .count();
        let expected_focus = usize::from(self.model.bar());
        if focus_count != expected_focus {
            note(
                if focus_count == 0 {
                    Kind::NoFocus
                } else {
                    Kind::TwoFocus
                },
                format!("{focus_count} focus bars, expected {expected_focus}"),
                safe,
            );
        }
        issues
    }
}

fn inside(r: Rect, clip: Rect) -> bool {
    r.x >= clip.x - 0.02
        && r.y >= clip.y - 0.02
        && r.x + r.w <= clip.x + clip.w + 0.02
        && r.y + r.h <= clip.y + clip.h + 0.02
}

fn overlap(a: Rect, b: Rect) -> bool {
    a.x.max(b.x) + 0.5 < (a.x + a.w).min(b.x + b.w)
        && a.y.max(b.y) + 0.5 < (a.y + a.h).min(b.y + b.h)
}

fn center(scene: &Scene, tag: u32) -> PhysicalPosition<f64> {
    let r = scene
        .hits
        .iter()
        .find(|h| h.tag == tag)
        .unwrap_or_else(|| panic!("missing Scene tag {tag:#x}"))
        .rect;
    PhysicalPosition::new(
        f64::from((r.x + r.w * 0.5) * scene.scale),
        f64::from((r.y + r.h * 0.5) * scene.scale),
    )
}

fn touch(phase: TouchPhase, location: PhysicalPosition<f64>) -> WindowEvent {
    WindowEvent::Touch(Touch {
        device_id: DeviceId::dummy(),
        phase,
        location,
        force: None,
        id: 0,
    })
}

pub fn image(gfx: &mut Gfx, frame: &image::RgbaImage, scene: &Scene) -> image::RgbaImage {
    let tex = shot_texture(gfx, frame);
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let mut enc = gfx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    gfx.draw_scene(scene, &view, frame.dimensions(), &mut enc);
    gfx.read_texture(enc, &tex)
}

#[cfg(test)]
mod tests;
