//! Port menu (F1 / Android Back / resume from the background): an egui pass over the held game's dimmed last frame.

use super::touch::{TouchConfig, TouchLayout};
use super::touchedit::{EditOut, TouchEdit};
use crate::i18n::{self, Lang};
use crate::launcher::{self, Action};
use crate::settings::Settings;
use egui::{Align2, Color32, Id, LayerId, Order, RichText};
use th08_render::gfx::{Gfx, OverlayTarget, SafeArea};
use winit::event::WindowEvent;
use winit::window::Window;

pub struct PortMenu {
    ctx: egui::Context,
    state: egui_winit::State,
    // Made on the first paint (it needs the surface format).
    renderer: Option<(wgpu::TextureFormat, egui_wgpu::Renderer)>,
    // Texture updates not yet uploaded (a skipped frame keeps them) and frees owed from the last paint.
    pending: egui::TexturesDelta,
    // When egui wants the next repaint.
    pub repaint_at: Option<web_time::Instant>,
    // The `live` settings rows; the language row sets the menu's language.
    model: launcher::Model,
    // A row changed since the last `take_changed`.
    changed: bool,
    // "Quit to title" is offered (`quittitle::available`).
    pub quit_ok: bool,
    opened: (Lang, bool),
    // `Gfx::generation` of the first paint; a change means the renderer is dead (device rebuilt).
    generation: Option<u32>,
    // The active gamepad's name for the controller row (set by the app before each redraw).
    pub controller: Option<String>,
    // Active Cheats rows (`Settings::active_cheats`), shown as a no-saves note.
    pub cheats: Vec<&'static str>,
    // The last screenshot's outcome (set by the app).
    pub status: Option<String>,
    // The speed row's rate (practice slow motion or replay transport); None hides it.
    pub speed: Option<(u32, u32)>,
    // Slow motion was used: score.dat and replays are not saved.
    pub no_saves: bool,
    // The live overlay settings (None: no overlay, no layout editor), the open editor, a saved layout not yet taken.
    touch: Option<TouchConfig>,
    edit: Option<TouchEdit>,
    saved: Option<TouchLayout>,
}

// The launcher's 640×480 layout scale for a window's safe rect (launcher.rs `zoom`).
fn zoom(safe: SafeArea, (w, h): (u32, u32)) -> f32 {
    let [_, _, sw, sh] = safe.rect(w, h);
    (sw / 640.0).min(sh / 480.0).max(0.5)
}

impl PortMenu {
    // `s`: the held game's settings; `touch`: the overlay's settings when it is on.
    pub fn new(window: &Window, s: &Settings, touch: Option<TouchConfig>) -> Self {
        let ctx = new_ctx(s);
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        Self {
            ctx,
            state,
            renderer: None,
            pending: egui::TexturesDelta::default(),
            repaint_at: None,
            model: launcher::Model::port_menu(s, touch.is_some()),
            changed: false,
            quit_ok: false,
            opened: (i18n::resolve(s.language), s.now_playing),
            generation: None,
            controller: None,
            cheats: Vec::new(),
            status: None,
            speed: None,
            no_saves: false,
            touch,
            edit: None,
            saved: None,
        }
    }

    // Back / Esc: closes the layout editor without saving; false when the menu itself should close.
    pub fn back(&mut self) -> bool {
        self.edit.take().is_some()
    }

    // The live rows changed since the last call: (key, value text) of every shown row.
    pub fn take_changed(&mut self) -> Option<Vec<(&'static str, String)>> {
        std::mem::take(&mut self.changed).then(|| live_values(&self.model))
    }

    // The language and `now_playing` the menu opened with.
    pub fn opened_with(&self) -> (Lang, bool) {
        self.opened
    }

    // Writes the rows changed in the menu to port.cfg.
    pub fn save(&self) {
        self.model.save_changes();
    }

    // A layout the editor saved since the last call.
    pub fn take_layout(&mut self) -> Option<TouchLayout> {
        self.saved.take()
    }

    // Returns whether the window needs a redraw.
    pub fn on_event(&mut self, window: &Window, ev: &WindowEvent) -> bool {
        self.state.on_window_event(window, ev).repaint
    }

    // One pass: the game frame, a dim layer and the menu box in the safe area.
    pub fn redraw(&mut self, window: &Window, gfx: &mut Gfx) -> Option<Action> {
        let size = window.inner_size();
        gfx.resize(size.width, size.height);
        let (w, h) = gfx.surface_size();
        let ppp = zoom(gfx.safe, (w, h));
        let z = ppp / window.scale_factor() as f32;
        if self.ctx.zoom_factor() != z {
            window.request_redraw();
        }
        self.ctx.set_zoom_factor(z);
        let mut raw = self.state.take_egui_input(window);
        let s = gfx.safe;
        raw.safe_area_insets = Some(egui::SafeAreaInsets(egui::epaint::MarginF32 {
            left: s.left as f32 / ppp,
            right: s.right as f32 / ppp,
            top: s.top as f32 / ppp,
            bottom: s.bottom as f32 / ppp,
        }));
        let mut act = None;
        let lang = self.model.lang();
        if let Some(e) = &mut self.edit {
            e.set_window((w, h), window.scale_factor(), s);
        }
        let pad = self.controller.as_deref();
        let cheats = &self.cheats;
        let status = self.status.as_deref();
        let (speed, no_saves, quit) = (self.speed, self.no_saves, self.quit_ok);
        let (edit, touch) = (&mut self.edit, self.touch.is_some());
        let model = &mut self.model;
        let mut changed = false;
        let mut open_edit = false;
        let mut done = None;
        let hitboxes = super::hitbox::mode(gfx);
        let mut cycle = false;
        let mut out = self.ctx.run_pass(raw, |ctx| {
            if let Some(e) = edit {
                done = e.ui(ctx, lang, ppp);
            } else {
                let m = MenuState {
                    pad,
                    touch,
                    cheats,
                    hitboxes,
                    status,
                    speed,
                    no_saves,
                    quit,
                };
                (act, open_edit, cycle, changed) = menu_ui(ctx, lang, &m, model);
            }
        });
        if changed {
            self.changed = true;
            window.request_redraw();
        }
        if cycle {
            super::hitbox::cycle(gfx);
            window.request_redraw();
        }
        match done {
            Some(EditOut::Save(l)) => {
                if let Some(t) = &mut self.touch {
                    t.layout = l.clone();
                }
                self.saved = Some(l);
                self.edit = None;
                window.request_redraw();
            }
            Some(EditOut::Cancel) => {
                self.edit = None;
                window.request_redraw();
            }
            None => {}
        }
        if open_edit && let Some(t) = &self.touch {
            self.edit = Some(TouchEdit::new(t.clone()));
            window.request_redraw();
        }
        self.state
            .handle_platform_output(window, std::mem::take(&mut out.platform_output));
        let delay = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        self.repaint_at =
            (delay < std::time::Duration::from_secs(5)).then(|| web_time::Instant::now() + delay);
        self.pending.append(std::mem::take(&mut out.textures_delta));
        let jobs = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        let ppp = out.pixels_per_point;
        gfx.recover_if_lost();
        if self.generation.is_some_and(|g| g != gfx.generation()) {
            self.renderer = None;
            launcher::resend_atlas(&self.ctx, &mut self.pending);
        }
        self.generation = Some(gfx.generation());
        let (renderer, pending) = (&mut self.renderer, &mut self.pending);
        let shown = gfx.present_last(|t, enc| paint(renderer, pending, &jobs, ppp, &t, enc));
        if !shown {
            window.request_redraw();
        }
        act
    }
}

// What the menu box shows; `touch`: the overlay is on; `quit`: "Quit to title" is offered.
struct MenuState<'a> {
    pad: Option<&'a str>,
    touch: bool,
    cheats: &'a [&'static str],
    hitboxes: super::hitbox::Hitboxes,
    status: Option<&'a str>,
    speed: Option<(u32, u32)>,
    no_saves: bool,
    quit: bool,
}

// The egui context of a menu: dark, with the CJK font (the language row names 日本語 in every language).
fn new_ctx(s: &Settings) -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_visuals(egui::Visuals::dark());
    i18n::install_fonts(&ctx, s.font.as_deref());
    ctx
}

fn live_values(m: &launcher::Model) -> Vec<(&'static str, String)> {
    m.live_keys()
        .into_iter()
        .filter_map(|k| Some((k, m.value_text(k)?)))
        .collect()
}

// The menu box, scrolled when it does not fit; also whether the layout editor was asked for, whether the hitbox
// mode is to be cycled and whether a settings row changed.
fn menu_ui(
    ctx: &egui::Context,
    lang: Lang,
    m: &MenuState<'_>,
    model: &mut launcher::Model,
) -> (Option<Action>, bool, bool, bool) {
    let MenuState {
        pad,
        touch,
        cheats,
        hitboxes,
        status,
        speed,
        no_saves,
        quit,
    } = *m;
    let mut act = None;
    let mut edit = false;
    let mut cycle = false;
    let mut changed = false;
    let screen = ctx.content_rect();
    ctx.layer_painter(LayerId::new(Order::Background, Id::unique("dim")))
        .rect_filled(screen, 0.0, Color32::from_black_alpha(150));
    let width = (screen.width() - 48.0).clamp(240.0, 600.0);
    egui::Area::new(Id::unique("port menu"))
        .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(ui.visuals().window_fill.to_opaque())
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(width);
                    egui::ScrollArea::vertical()
                        .max_height(screen.height() - 56.0)
                        .min_scrolled_height(screen.height() - 56.0)
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.heading(i18n::ui(lang, "port_menu"));
                                ui.add_space(4.0);
                                ui.label(RichText::new(i18n::ui(lang, "paused")).weak());
                                if let Some(note) = launcher::cheats_note(lang, cheats) {
                                    ui.add_space(4.0);
                                    ui.colored_label(ui.visuals().error_fg_color, note);
                                }
                                if no_saves {
                                    ui.add_space(4.0);
                                    ui.colored_label(
                                        ui.visuals().error_fg_color,
                                        i18n::ui(lang, "slow_nosave"),
                                    );
                                }
                                ui.add_space(12.0);
                                let resume = format!("  {}  ", i18n::ui(lang, "resume"));
                                if ui.button(RichText::new(resume).strong()).clicked() {
                                    act = Some(Action::Resume);
                                }
                                if quit {
                                    ui.add_space(4.0);
                                    if ui.button(i18n::ui(lang, "quit_title")).clicked() {
                                        act = Some(Action::QuitToTitle);
                                    }
                                }
                                if touch {
                                    ui.add_space(4.0);
                                    edit = ui.button(i18n::ui(lang, "touch_edit")).clicked();
                                }
                                if let Some(now) = speed {
                                    ui.add_space(4.0);
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(i18n::ui(lang, "slow_motion"));
                                        for rate in crate::transport::SLOW {
                                            let text = crate::transport::rate_label(lang, rate);
                                            if ui.selectable_label(now == rate, text).clicked()
                                                && now != rate
                                            {
                                                act = Some(Action::Speed(rate));
                                            }
                                        }
                                    });
                                }
                                ui.add_space(4.0);
                                cycle = ui.button(hitboxes_label(lang, hitboxes)).clicked();
                                ui.add_space(4.0);
                                let shot =
                                    if cfg!(any(target_os = "android", target_arch = "wasm32")) {
                                        i18n::ui(lang, "screenshot").to_string()
                                    } else {
                                        format!("{} (F12)", i18n::ui(lang, "screenshot"))
                                    };
                                if ui.button(shot).clicked() {
                                    act = Some(Action::Screenshot);
                                }
                                if let Some(s) = status {
                                    ui.label(RichText::new(s).small());
                                }
                            });
                            ui.add_space(8.0);
                            ui.separator();
                            changed = model.live_ui(ui, lang);
                            ui.separator();
                            ui.vertical_centered(|ui| {
                                let keys = if cfg!(target_os = "android") {
                                    i18n::ui(lang, "key_back")
                                } else {
                                    "F1 / Esc"
                                };
                                ui.label(RichText::new(keys).small().weak());
                                ui.add_space(4.0);
                                let name = pad.unwrap_or_else(|| i18n::ui(lang, "no_controller"));
                                ui.label(i18n::ui_fmt(lang, "controller", &[("name", name)]));
                                ui.add_space(4.0);
                                ui.label(RichText::new(launcher::version_line()).small().weak());
                            });
                        });
                });
        });
    (act, edit, cycle, changed)
}

// "Hitboxes: Off", from the settings row's label and words.
fn hitboxes_label(lang: Lang, m: super::hitbox::Hitboxes) -> String {
    let e = crate::settings::find("hitboxes").expect("hitboxes row");
    let word = i18n::word(lang, e, m.id()).unwrap_or(m.id());
    format!("{}: {word}", i18n::label(lang, e))
}

fn paint(
    renderer: &mut Option<(wgpu::TextureFormat, egui_wgpu::Renderer)>,
    pending: &mut egui::TexturesDelta,
    jobs: &[egui::ClippedPrimitive],
    ppp: f32,
    t: &OverlayTarget<'_>,
    enc: &mut wgpu::CommandEncoder,
) {
    if renderer.as_ref().is_none_or(|(f, _)| *f != t.format) {
        let r = egui_wgpu::Renderer::new(t.device, t.format, egui_wgpu::RendererOptions::default());
        *renderer = Some((t.format, r));
    }
    let (_, r) = renderer.as_mut().expect("renderer");
    for (id, deltas) in pending.set.drain() {
        for d in &deltas {
            r.update_texture(t.device, t.queue, id, d);
        }
    }
    let desc = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [t.size.0, t.size.1],
        pixels_per_point: ppp,
    };
    let extra = r.update_buffers(t.device, t.queue, enc, jobs, &desc);
    if !extra.is_empty() {
        t.queue.submit(extra);
    }
    {
        let mut pass = enc
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("port menu"),
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
        r.render(&mut pass, jobs, &desc);
    }
    for id in pending.free.drain() {
        r.free_texture(&id);
    }
}

// Passes of `render_image`, 0.1 s apart: the menu box's fade-in has ended by the last.
#[cfg(not(target_arch = "wasm32"))]
const PASSES: u32 = 8;

// Tools: the menu over `frame` (a w×h picture of the game as the window shows it) on a headless device, after a few
// passes so the layout settles; `quit` / `touch` as the app would set them.
#[cfg(not(target_arch = "wasm32"))]
pub fn render_image(
    gfx: &Gfx,
    s: &Settings,
    frame: &image::RgbaImage,
    safe: SafeArea,
    quit: bool,
    touch: bool,
) -> image::RgbaImage {
    let (w, h) = frame.dimensions();
    let tex = gfx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("port menu shot"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gfx.surface_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut px = frame.clone().into_raw();
    if matches!(
        gfx.surface_format(),
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        px.as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|p| p.swap(0, 2));
    }
    gfx.queue.write_texture(
        tex.as_image_copy(),
        &px,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        tex.size(),
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let ctx = new_ctx(s);
    let mut model = launcher::Model::port_menu(s, touch);
    let lang = model.lang();
    let ppp = zoom(safe, (w, h));
    let (mut renderer, mut pending) = (None, egui::TexturesDelta::default());
    let cheats = s.active_cheats();
    let m = MenuState {
        pad: None,
        touch,
        cheats: &cheats,
        hitboxes: s.hitboxes,
        status: None,
        speed: None,
        no_saves: false,
        quit,
    };
    let target = OverlayTarget {
        device: &gfx.device,
        queue: &gfx.queue,
        view: &view,
        format: gfx.surface_format(),
        size: (w, h),
        frame: gfx.frame_rect(w, h),
    };
    for i in 0..PASSES {
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(w as f32 / ppp, h as f32 / ppp),
            )),
            time: Some(f64::from(i) / 10.0),
            safe_area_insets: Some(egui::SafeAreaInsets(egui::epaint::MarginF32 {
                left: safe.left as f32 / ppp,
                right: safe.right as f32 / ppp,
                top: safe.top as f32 / ppp,
                bottom: safe.bottom as f32 / ppp,
            })),
            ..Default::default()
        };
        raw.viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);
        let out = ctx.run_pass(raw, |ctx| {
            menu_ui(ctx, lang, &m, &mut model);
        });
        pending.append(out.textures_delta);
        if i + 1 < PASSES {
            continue;
        }
        let jobs = ctx.tessellate(out.shapes, out.pixels_per_point);
        let mut enc = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        paint(
            &mut renderer,
            &mut pending,
            &jobs,
            out.pixels_per_point,
            &target,
            &mut enc,
        );
        return gfx.read_texture(enc, &tex);
    }
    unreachable!()
}
