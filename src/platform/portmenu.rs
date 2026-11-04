//! Port menu (F1 / Android Back / resume from the background): an egui pass over the held game's dimmed last frame.

use crate::i18n::{self, Lang};
use crate::launcher::{self, Action};
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
    lang: Lang,
    // `Gfx::generation` of the first paint; a change means the renderer is dead (device rebuilt).
    generation: Option<u32>,
    // The active gamepad's name for the controller row (set by the app before each redraw).
    pub controller: Option<String>,
}

// The launcher's 640×480 layout scale for a window's safe rect (launcher.rs `zoom`).
fn zoom(safe: SafeArea, (w, h): (u32, u32)) -> f32 {
    let [_, _, sw, sh] = safe.rect(w, h);
    (sw / 640.0).min(sh / 480.0).max(0.5)
}

impl PortMenu {
    // `lang`: the game's resolved `language` setting; `font`: its `font` setting (CJK fallback lookup).
    pub fn new(window: &Window, lang: Lang, font: Option<&str>) -> Self {
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        if lang == Lang::Ja {
            i18n::install_fonts(&ctx, font);
        }
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
            lang,
            generation: None,
            controller: None,
        }
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
        let lang = self.lang;
        let pad = self.controller.as_deref();
        let mut out = self.ctx.run_pass(raw, |ctx| act = menu_ui(ctx, lang, pad));
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

fn menu_ui(ctx: &egui::Context, lang: Lang, pad: Option<&str>) -> Option<Action> {
    let mut act = None;
    let screen = ctx.content_rect();
    ctx.layer_painter(LayerId::new(Order::Background, Id::unique("dim")))
        .rect_filled(screen, 0.0, Color32::from_black_alpha(150));
    egui::Area::new(Id::unique("port menu"))
        .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading(i18n::ui(lang, "port_menu"));
                        ui.add_space(4.0);
                        ui.label(RichText::new(i18n::ui(lang, "paused")).weak());
                        ui.add_space(12.0);
                        let resume = format!("  {}  ", i18n::ui(lang, "resume"));
                        if ui.button(RichText::new(resume).strong()).clicked() {
                            act = Some(Action::Resume);
                        }
                        ui.add_space(4.0);
                        let keys = if cfg!(target_os = "android") {
                            i18n::ui(lang, "key_back")
                        } else {
                            "F1 / Esc"
                        };
                        ui.label(RichText::new(keys).small().weak());
                        ui.add_space(8.0);
                        let name = pad.unwrap_or_else(|| i18n::ui(lang, "no_controller"));
                        ui.label(i18n::ui_fmt(lang, "controller", &[("name", name)]));
                    });
                });
        });
    act
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
