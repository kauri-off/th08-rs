//! Launcher screen and game-folder flow, port infrastructure (no spec section): an egui settings screen generated
//! from `settings::ENTRIES`, drawn on the game's wgpu device and window before the game starts.

use crate::gfx::Gfx;
use crate::settings::{self, ENTRIES, Entry, Group, Kind, Origin, Settings};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use winit::event::WindowEvent;
use winit::window::Window;

// A game folder holds th08.dat (any letter case).
#[cfg(not(target_arch = "wasm32"))]
pub fn is_game_dir(dir: &Path) -> bool {
    crate::assets::find_file(dir, "th08.dat").is_file()
}

// The game folder to run; Ok(None) = dialog cancelled, Err = the folder given on the command line / TH08_DIR is invalid.
#[cfg(not(target_arch = "wasm32"))]
pub fn resolve_game_dir(s: &Settings) -> Result<Option<PathBuf>, String> {
    if let Some(d) = &s.game_dir {
        if is_game_dir(d) {
            return Ok(Some(d.clone()));
        }
        let msg = format!("{}: no th08.dat in this folder", d.display());
        if s.origin("game_dir") > Origin::PortCfg {
            return Err(msg);
        }
        log::warn!("port.cfg {msg}; asking for the folder");
    }
    Ok(pick_game_dir())
}

// Folder dialog until a valid folder is picked (saved as port.cfg `game_dir`) or the dialog is cancelled.
#[cfg(not(target_arch = "wasm32"))]
fn pick_game_dir() -> Option<PathBuf> {
    loop {
        let Some(dir) = folder_dialog() else {
            log::info!("folder dialog cancelled or unavailable");
            eprintln!("no game folder selected (usage: th08 <game dir>, see --help)");
            return None;
        };
        if is_game_dir(&dir) {
            settings::save_port_cfg("game_dir", &dir.to_string_lossy());
            return Some(dir);
        }
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("th08")
            .set_description(format!(
                "{} does not contain th08.dat.\nSelect the Imperishable Night folder.",
                dir.display()
            ))
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn folder_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select the Touhou 8 (Imperishable Night) game folder")
        .pick_folder()
}

// Where the game files come from: the `game_dir` row (native) or the files picked in the web page.
pub enum Folder {
    Native,
    Web(Vec<String>),
}

pub enum Action {
    Play,
    Quit,
}

// One table entry as edited on screen. Value text "" = unset; booleans are "on"/"off".
struct Row {
    e: &'static Entry,
    value: String,
    initial: String,
    origin: Origin,
}

impl Row {
    fn default_value(&self) -> String {
        norm(self.e, self.e.default)
    }

    fn changed(&self) -> bool {
        self.value != self.initial
    }

    fn error(&self) -> Option<String> {
        (!self.value.is_empty()).then(|| self.e.check(&self.value).err())?
    }
}

fn norm(e: &Entry, v: &str) -> String {
    match e.kind {
        Kind::Bool => match settings::parse_bool(v) {
            Ok(true) => "on".into(),
            Ok(false) => "off".into(),
            Err(_) => v.into(),
        },
        _ => v.trim().into(),
    }
}

// The edited settings: one row per `ENTRIES` entry.
pub struct Model {
    rows: Vec<Row>,
    folder: Folder,
}

impl Model {
    pub fn new(s: &Settings, folder: Folder) -> Self {
        let rows = ENTRIES
            .iter()
            .map(|e| {
                let v = norm(e, s.value(e.key).unwrap_or(""));
                Row {
                    e,
                    value: v.clone(),
                    initial: v,
                    origin: s.origin(e.key),
                }
            })
            .collect();
        Self { rows, folder }
    }

    // F11 / Alt+Enter on the launcher: flips the fullscreen box.
    pub fn toggle_fullscreen(&mut self) {
        let on = self.settings().fullscreen;
        let r = self
            .rows
            .iter_mut()
            .find(|r| r.e.key == "fullscreen")
            .unwrap();
        r.value = if on { "off" } else { "on" }.into();
    }

    fn row(&self, key: &str) -> &Row {
        self.rows.iter().find(|r| r.e.key == key).unwrap()
    }

    fn folder_ok(&self) -> bool {
        match &self.folder {
            #[cfg(not(target_arch = "wasm32"))]
            Folder::Native => {
                let v = &self.row("game_dir").value;
                !v.is_empty() && is_game_dir(Path::new(v))
            }
            #[cfg(target_arch = "wasm32")]
            Folder::Native => false,
            Folder::Web(names) => names.iter().any(|n| n.eq_ignore_ascii_case("th08.dat")),
        }
    }

    fn can_play(&self) -> bool {
        self.folder_ok() && self.rows.iter().all(|r| r.error().is_none())
    }

    // The final settings: unchanged rows keep their origin, edited ones count as set on the command line.
    pub fn settings(&self) -> Settings {
        Settings::from_values(self.rows.iter().filter(|r| !r.value.is_empty()).map(|r| {
            let o = if r.changed() { Origin::Cli } else { r.origin };
            (r.e, r.value.as_str(), o)
        }))
    }

    // port.cfg / localStorage: every row edited on screen, removed when back at its default.
    pub fn save(&self) {
        let changes: Vec<(&str, Option<&str>)> = self
            .rows
            .iter()
            .filter(|r| r.changed())
            .map(|r| {
                let keep = !r.value.is_empty() && r.value != r.default_value();
                (r.e.key, keep.then_some(r.value.as_str()))
            })
            .collect();
        if !changes.is_empty() {
            settings::write_port_cfg(&changes);
        }
    }

    fn reset(&mut self) {
        for r in self.rows.iter_mut().filter(|r| r.e.key != "game_dir") {
            r.value = r.default_value();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut act = None;
        egui::Panel::top("folder").show(ui, |ui| {
            ui.add_space(4.0);
            ui.heading("th08 — Touhou 8 port");
            self.folder_ui(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("buttons").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let play = ui.add_enabled(
                    self.can_play(),
                    egui::Button::new(egui::RichText::new("  Play  ").strong()),
                );
                if play.clicked() {
                    act = Some(Action::Play);
                }
                if ui.button("Reset to defaults").clicked() {
                    self.reset();
                }
                if cfg!(not(target_arch = "wasm32")) && ui.button("Quit").clicked() {
                    act = Some(Action::Quit);
                }
                if !self.folder_ok() {
                    ui.colored_label(ui.visuals().warn_fg_color, "select the game folder first");
                } else if self.rows.iter().any(|r| r.error().is_some()) {
                    ui.colored_label(ui.visuals().error_fg_color, "fix the marked values");
                }
            });
            ui.add_space(4.0);
        });
        egui::CentralPanel::default_margins().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for g in Group::ALL {
                    self.group_ui(ui, g);
                }
            });
        });
        act
    }

    fn folder_ui(&mut self, ui: &mut egui::Ui) {
        let ok = self.folder_ok();
        ui.horizontal(|ui| {
            ui.label("Game folder:");
            match &self.folder {
                Folder::Native => {
                    let r = self
                        .rows
                        .iter_mut()
                        .find(|r| r.e.key == "game_dir")
                        .unwrap();
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button("Browse…").clicked()
                        && let Some(d) = folder_dialog()
                    {
                        r.value = d.to_string_lossy().into_owned();
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut r.value)
                            .hint_text("folder with th08.dat")
                            .desired_width(f32::INFINITY),
                    )
                    .on_hover_text(r.e.help);
                }
                Folder::Web(names) => {
                    ui.label(if names.is_empty() {
                        "no files".to_string()
                    } else {
                        names.join(", ")
                    });
                }
            }
        });
        ui.horizontal(|ui| {
            if ok {
                ui.colored_label(egui::Color32::from_rgb(110, 200, 110), "th08.dat found");
            } else {
                ui.colored_label(ui.visuals().error_fg_color, "no th08.dat in this folder");
            }
            if let Folder::Native = self.folder {
                origin_label(ui, self.row("game_dir"));
            }
        });
    }

    fn group_ui(&mut self, ui: &mut egui::Ui, g: Group) {
        let web = matches!(self.folder, Folder::Web(_));
        // Folder rows are at the top; on the web the files come from the page.
        let rows: Vec<usize> = (0..self.rows.len())
            .filter(|&i| self.rows[i].e.group == g && self.rows[i].e.key != "game_dir")
            .filter(|&i| !(web && matches!(self.rows[i].e.kind, Kind::Path | Kind::File)))
            .collect();
        if rows.is_empty() {
            return;
        }
        let set_here = rows.iter().any(|&i| self.rows[i].origin > Origin::PortCfg);
        egui::CollapsingHeader::new(egui::RichText::new(g.title()).strong())
            .default_open(matches!(g, Group::GameData | Group::Display) || set_here)
            .show(ui, |ui| {
                egui::Grid::new(g.title())
                    .num_columns(2)
                    .striped(true)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        for i in rows {
                            row_ui(ui, &mut self.rows[i]);
                            ui.end_row();
                        }
                    });
            });
    }
}

fn row_ui(ui: &mut egui::Ui, r: &mut Row) {
    ui.label(r.e.key.replace('_', " ")).on_hover_text(r.e.help);
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            value_ui(ui, r);
            origin_label(ui, r);
        });
        if let Some(err) = r.error() {
            ui.colored_label(ui.visuals().error_fg_color, err);
        }
        ui.label(egui::RichText::new(r.e.help).small().weak());
    });
}

fn value_ui(ui: &mut egui::Ui, r: &mut Row) {
    let unsettable = r.e.default.is_empty();
    match r.e.kind {
        Kind::Bool => {
            let mut on = r.value == "on";
            if ui.checkbox(&mut on, "").changed() {
                r.value = if on { "on" } else { "off" }.into();
            }
        }
        Kind::Int { min, max } => {
            let mut set = !r.value.is_empty();
            if unsettable && ui.checkbox(&mut set, "").changed() {
                r.value = if set {
                    min.max(0).min(max).to_string()
                } else {
                    String::new()
                };
            }
            if set {
                let mut n: f64 = r.value.parse().unwrap_or(min as f64);
                let range = min as f64..=max as f64;
                let changed = if max - min <= 16 {
                    ui.add(
                        egui::Slider::new(&mut n, range)
                            .step_by(1.0)
                            .fixed_decimals(0),
                    )
                    .changed()
                } else {
                    ui.add(
                        egui::DragValue::new(&mut n)
                            .range(range)
                            .speed(1.0)
                            .fixed_decimals(0),
                    )
                    .changed()
                };
                if changed {
                    r.value = (n.round() as i64).to_string();
                }
            }
        }
        Kind::Enum(choices) => {
            let shown = if r.value.is_empty() {
                "(not set)"
            } else {
                r.value.as_str()
            };
            egui::ComboBox::from_id_salt(r.e.key)
                .selected_text(shown.to_string())
                .show_ui(ui, |ui| {
                    if unsettable {
                        ui.selectable_value(&mut r.value, String::new(), "(not set)");
                    }
                    for c in choices {
                        ui.selectable_value(&mut r.value, c.to_string(), *c);
                    }
                });
        }
        Kind::Text(hint) => {
            ui.add(
                egui::TextEdit::singleline(&mut r.value)
                    .hint_text(hint)
                    .desired_width(220.0),
            );
        }
        Kind::Path | Kind::File => {
            ui.add(
                egui::TextEdit::singleline(&mut r.value)
                    .hint_text(r.e.hint())
                    .desired_width(220.0),
            );
            #[cfg(not(target_arch = "wasm32"))]
            if ui.button("Browse…").clicked() {
                let d = rfd::FileDialog::new().set_title(r.e.help);
                let picked = if r.e.key == "soundfont" {
                    d.add_filter("SoundFont", &["sf2"]).pick_file()
                } else if r.e.kind == Kind::File {
                    d.add_filter("fonts", &["ttf", "otf", "ttc"]).pick_file()
                } else {
                    d.pick_folder()
                };
                if let Some(p) = picked {
                    r.value = p.to_string_lossy().into_owned();
                }
            }
        }
    }
}

// "set by TH08_STAGE" / "set by --stage" for values from the environment or command line, "changed" after an edit.
fn origin_label(ui: &mut egui::Ui, r: &Row) {
    let text = if r.changed() {
        "changed".to_string()
    } else {
        match r.origin {
            Origin::Cli => format!("set by --{}", r.e.key.replace('_', "-")),
            Origin::Env if cfg!(target_arch = "wasm32") => format!("set by URL ?{}=", r.e.url()),
            Origin::Env => format!("set by {}", r.e.env),
            Origin::PortCfg if cfg!(target_arch = "wasm32") => "saved".into(),
            Origin::PortCfg => "port.cfg".into(),
            Origin::Default => return,
        }
    };
    let c = match (r.changed(), r.origin) {
        (true, _) => egui::Color32::from_rgb(120, 170, 255),
        (_, Origin::Cli | Origin::Env) => egui::Color32::from_rgb(240, 190, 80),
        _ => ui.visuals().weak_text_color(),
    };
    ui.colored_label(c, text);
}

// The 640×480 layout scaled to the window.
fn zoom(size_px: (u32, u32)) -> f32 {
    (size_px.0 as f32 / 640.0)
        .min(size_px.1 as f32 / 480.0)
        .max(0.5)
}

// egui context and renderer on the game's device, drawing the model.
struct Painter {
    model: Model,
    ctx: egui::Context,
    renderer: egui_wgpu::Renderer,
    // Texture updates not yet uploaded (a pass whose frame was skipped keeps them for the next paint).
    pending: egui::TexturesDelta,
}

// The launcher in the game window. Dropped before the game loads, freeing its GPU objects.
pub struct Screen {
    p: Painter,
    state: egui_winit::State,
    // When egui wants the next repaint (tooltips, animations).
    pub repaint_at: Option<web_time::Instant>,
}

impl Screen {
    pub fn new(window: &Window, gfx: &Gfx, model: Model) -> Self {
        let p = Painter::new(gfx, model);
        let state = egui_winit::State::new(
            p.ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        Self {
            p,
            state,
            repaint_at: None,
        }
    }

    pub fn model(&self) -> &Model {
        &self.p.model
    }

    pub fn model_mut(&mut self) -> &mut Model {
        &mut self.p.model
    }

    // Returns whether the window needs a redraw.
    pub fn on_event(&mut self, window: &Window, ev: &WindowEvent) -> bool {
        self.state.on_window_event(window, ev).repaint
    }

    // One egui pass drawn to the window surface.
    pub fn redraw(&mut self, window: &Window, gfx: &mut Gfx) -> Option<Action> {
        let size = window.inner_size();
        gfx.resize(size.width, size.height);
        let (w, h) = gfx.surface_size();
        self.p
            .ctx
            .set_zoom_factor(zoom((w, h)) / window.scale_factor() as f32);
        let raw = self.state.take_egui_input(window);
        let (out, act) = self.p.pass(raw);
        self.state
            .handle_platform_output(window, out.platform_output.clone());
        let delay = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        self.repaint_at =
            (delay < std::time::Duration::from_secs(5)).then(|| web_time::Instant::now() + delay);
        let Some(surface) = &gfx.surface else {
            return act;
        };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            _ => {
                gfx.reconfigure();
                window.request_redraw();
                return act;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let enc = self.p.paint(gfx, out, &view, (w, h));
        gfx.queue.submit([enc.finish()]);
        gfx.queue.present(frame);
        act
    }
}

impl Painter {
    fn new(gfx: &Gfx, model: Model) -> Self {
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        let renderer = egui_wgpu::Renderer::new(
            &gfx.device,
            gfx.surface_format(),
            egui_wgpu::RendererOptions::default(),
        );
        Self {
            model,
            ctx,
            renderer,
            pending: egui::TexturesDelta::default(),
        }
    }

    fn pass(&mut self, raw: egui::RawInput) -> (egui::FullOutput, Option<Action>) {
        let mut act = None;
        let model = &mut self.model;
        let mut out = self.ctx.run_ui(raw, |ui| {
            if let Some(a) = model.ui(ui) {
                act = Some(a);
            }
        });
        self.pending.append(std::mem::take(&mut out.textures_delta));
        (out, act)
    }

    fn paint(
        &mut self,
        gfx: &Gfx,
        out: egui::FullOutput,
        view: &wgpu::TextureView,
        (w, h): (u32, u32),
    ) -> wgpu::CommandEncoder {
        let jobs = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        let desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [w, h],
            pixels_per_point: out.pixels_per_point,
        };
        for (id, deltas) in self.pending.set.drain() {
            for d in &deltas {
                self.renderer.update_texture(&gfx.device, &gfx.queue, id, d);
            }
        }
        let mut enc = gfx.device.create_command_encoder(&Default::default());
        let extra = self
            .renderer
            .update_buffers(&gfx.device, &gfx.queue, &mut enc, &jobs, &desc);
        if !extra.is_empty() {
            gfx.queue.submit(extra);
        }
        {
            let mut pass = enc
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("launcher"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &jobs, &desc);
        }
        for id in self.pending.free.drain() {
            self.renderer.free_texture(&id);
        }
        enc
    }
}

// Offscreen render of the launcher (tests, `launchershot`): a few passes so the layout settles, then read back.
#[cfg(not(target_arch = "wasm32"))]
pub fn render_image(gfx: &Gfx, model: Model, (w, h): (u32, u32)) -> image::RgbaImage {
    let mut s = Painter::new(gfx, model);
    let tex = gfx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("launcher shot"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gfx.surface_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    let ppp = zoom((w, h));
    let mut enc = None;
    for i in 0..4 {
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(w as f32 / ppp, h as f32 / ppp),
            )),
            time: Some(i as f64 / 60.0),
            ..Default::default()
        };
        raw.viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);
        let (out, _) = s.pass(raw);
        let e = s.paint(gfx, out, &view, (w, h));
        if i < 3 {
            gfx.queue.submit([e.finish()]);
        } else {
            enc = Some(e);
        }
    }
    gfx.read_texture(enc.unwrap(), &tex)
}
