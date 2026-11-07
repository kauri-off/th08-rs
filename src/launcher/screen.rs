//! Launcher settings screen: title art, heading, Play, game folder, underline tabs, painted setting rows, help line and key hints.

use super::{Action, Folder, Model, row_ctl, row_input};
use crate::i18n::{self, Lang};
use crate::settings::{Audience, Group};
use crate::theme::{self, Ctl, CtlStyle, Edge};
use egui::{Id, Key, Painter, Rect, Sense, Ui, pos2, vec2};

// Sizes, points (the mock-up's 1280×960 px at the 1.14 OS-fit scale).
const HEAD: f32 = 31.0;
const CAP: f32 = 13.5;
const PLAY: f32 = 28.0;
const PLAY_H: f32 = 54.0;
const ROW: f32 = 19.5;
const ROW_H: f32 = 47.0;
const LABEL_W: f32 = 262.0;
const SMALL: f32 = 14.0;
const HINT: f32 = 13.0;

// The focusable targets of the screen, by identity so the focus survives a re-layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Play,
    Folder,
    Row(&'static str),
    Developer,
    Reset,
    #[cfg(not(target_os = "android"))]
    SavesExport,
    #[cfg(not(target_os = "android"))]
    SavesImport,
    #[cfg(not(target_os = "android"))]
    SavesCfg,
    // Replays / Stats tab rows: a text action, a word choice, a replay.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    Action(&'static str),
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    Choice(&'static str),
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    Replay(usize),
    // The start-failure screen's Back and Quit.
    Back,
    Exit,
}

// What the status screen shows instead of the settings.
pub(super) enum Status<'a> {
    // Starting the game; the cheats note when cheats are on.
    Waiting(Option<String>),
    Loading,
    // The start failure's own message.
    Error(&'a str),
}

// Keyboard focus, the targets of the last pass in order, the reset question, the title art.
#[derive(Default)]
pub(super) struct Nav {
    focus: Option<Target>,
    input: theme::Input<Target>,
    // Z / ← → on a Replays / Stats row, for the row to take this pass.
    #[cfg_attr(
        any(target_arch = "wasm32", target_os = "android"),
        expect(dead_code, reason = "no Replays / Stats tabs")
    )]
    pending: Option<(Target, i32)>,
    order: Vec<Target>,
    moved: bool,
    reset_ask: bool,
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    art: Art,
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
#[derive(Default)]
enum Art {
    #[default]
    NotTried,
    Loading(std::sync::mpsc::Receiver<Option<egui::ColorImage>>),
    Ready(egui::TextureHandle),
    None,
}

// title00.png (the title screen's backdrop) from the game folder, decoded off the UI thread.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn load_art(dir: std::path::PathBuf) -> std::sync::mpsc::Receiver<Option<egui::ColorImage>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let img = (|| {
            let src = crate::sources::native(dir);
            let content = pollster::block_on(th08_core::content::Content::load(&src)).ok()?;
            let view = th08_core::content::LogicView::new(content);
            let bytes = view.present().read("title00.png").ok()?;
            let rgba = image::load_from_memory(&bytes).ok()?.to_rgba8();
            let (w, h) = rgba.dimensions();
            Some(egui::ColorImage::from_rgba_unmultiplied(
                [w as usize, h as usize],
                rgba.as_raw(),
            ))
        })();
        let _ = tx.send(img);
    });
    rx
}

impl Model {
    // The title art in `rect`: the kanji column of title00.png, darkened, fading into the panel; without game data
    // the title in large serif glyphs.
    #[cfg_attr(
        any(target_arch = "wasm32", target_os = "android"),
        expect(unused_variables, reason = "no game folder to read the art from")
    )]
    fn art_ui(&mut self, ctx: &egui::Context, p: &Painter, rect: Rect) {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        if matches!(self.nav.art, Art::NotTried) && self.folder_ok() {
            self.nav.art = match &self.folder {
                Folder::Native => Art::Loading(load_art(self.game_dir().into())),
                _ => Art::None,
            };
        }
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        if let Art::Loading(rx) = &self.nav.art {
            match rx.try_recv() {
                Ok(Some(img)) => {
                    self.nav.art = Art::Ready(ctx.load_texture(
                        "title art",
                        img,
                        egui::TextureOptions::LINEAR,
                    ));
                }
                Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.nav.art = Art::None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint(),
            }
        }
        p.rect_filled(rect, 0.0, theme::BG);
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        let art = false;
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        let art = matches!(self.nav.art, Art::Ready(_));
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        if let Art::Ready(t) = &self.nav.art {
            // A column of the backdrop around the shrine maiden (x ≈ 365, y 0..440 of 640×480), at the rect's height.
            let [iw, ih] = t.size().map(|v| v as f32);
            let crop_h = ih * 440.0 / 480.0;
            let crop_w = (rect.width() / rect.height() * crop_h).min(iw);
            let cx = (iw * 365.0 / 640.0).clamp(crop_w / 2.0, iw - crop_w / 2.0);
            let uv = Rect::from_min_max(
                pos2((cx - crop_w / 2.0) / iw, 0.0),
                pos2((cx + crop_w / 2.0) / iw, crop_h / ih),
            );
            p.image(t.id(), rect, uv, egui::Color32::WHITE);
            p.rect_filled(
                rect,
                0.0,
                egui::Color32::from_rgba_premultiplied(3, 0, 1, 115),
            );
        }
        // The title down the column in the menu face, as the title screen's logo.
        let size = (rect.height() / 6.6).min(rect.width() * 0.42);
        // Over the art the column stands where the title screen's logo does; alone it is centred.
        let x = if art {
            rect.max.x - size * 1.45
        } else {
            rect.center().x - size / 2.0
        };
        let color = if art {
            egui::Color32::from_rgb(0xd2, 0xc8, 0xd8)
        } else {
            theme::DIM
        };
        for (i, c) in "東方永夜抄".chars().enumerate() {
            let g = p.layout_no_wrap(c.to_string(), theme::serif(size), color);
            theme::outlined(
                p,
                pos2(x, rect.min.y + size * 0.45 + i as f32 * size * 1.17),
                g,
                color,
                true,
            );
        }
        let fade = Rect::from_x_y_ranges(rect.max.x - 80.0..=rect.max.x, rect.y_range());
        let mut m = egui::Mesh::default();
        let clear = egui::Color32::TRANSPARENT;
        m.colored_vertex(fade.left_top(), clear);
        m.colored_vertex(fade.right_top(), theme::PANEL);
        m.colored_vertex(fade.right_bottom(), theme::PANEL);
        m.colored_vertex(fade.left_bottom(), clear);
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        p.add(m);
    }

    // Arrow keys / Z / Enter / Q / E / Esc while no text field has the keyboard.
    fn keys(&mut self, ui: &Ui) -> Option<Action> {
        if ui.ctx().memory(|m| m.focused().is_some()) || self.capture.is_some() {
            return None;
        }
        let pressed = |k: Key| ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, k));
        let mut act = None;
        let nav = [
            Key::ArrowDown,
            Key::ArrowUp,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::Z,
            Key::Enter,
        ];
        // The first key press after pointer input only shows the focus bar (Z / Enter also act on its row).
        if ui.input(|i| nav.iter().any(|&k| i.key_pressed(k)))
            && let Some(hover) = self.nav.input.key()
        {
            if let Some(h) = hover {
                self.nav.focus = Some(h);
            }
            self.nav.moved = true;
            for k in &nav[..4] {
                pressed(*k);
            }
        }
        let order = self.nav.order.clone();
        let at = self
            .nav
            .focus
            .and_then(|f| order.iter().position(|&t| t == f));
        if pressed(Key::ArrowDown) {
            self.nav.focus = order
                .get(at.map_or(0, |a| (a + 1).min(order.len() - 1)))
                .copied();
            self.nav.moved = true;
        }
        if pressed(Key::ArrowUp) {
            self.nav.focus = order.get(at.map_or(0, |a| a.saturating_sub(1))).copied();
            self.nav.moved = true;
        }
        for (k, dir) in [(Key::ArrowLeft, -1), (Key::ArrowRight, 1)] {
            if pressed(k) {
                match self.nav.focus {
                    Some(Target::Row(key)) => {
                        self.row_mut(key).apply(dir, false);
                    }
                    Some(Target::Developer) => self.dev = dir > 0,
                    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
                    Some(t @ Target::Choice(_)) => self.nav.pending = Some((t, dir)),
                    _ => {}
                }
            }
        }
        if pressed(Key::Z) || pressed(Key::Enter) {
            act = self.activate_target();
        }
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        for (k, dir) in [(Key::Q, 2usize), (Key::E, 1)] {
            if pressed(k) {
                self.nav.input.key();
                let all = [super::Tab::Settings, super::Tab::Replays, super::Tab::Stats];
                let at = all.iter().position(|&t| t == self.tab).unwrap_or(0);
                self.set_tab(all[(at + dir) % 3]);
            }
        }
        if cfg!(not(target_arch = "wasm32")) && pressed(Key::Escape) {
            act = Some(Action::Quit);
        }
        act
    }

    fn activate_target(&mut self) -> Option<Action> {
        match self.nav.focus? {
            Target::Play => self.can_play().then_some(Action::Play),
            Target::Back => Some(Action::Back),
            Target::Exit => Some(Action::Quit),
            Target::Folder => {
                self.change_folder();
                None
            }
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            Target::Row(key) if is_path(self.row(key).e) => {
                self.change_path(key);
                None
            }
            Target::Row(key) => {
                self.row_mut(key).apply(1, true);
                None
            }
            #[cfg(not(target_os = "android"))]
            Target::SavesExport => {
                self.saves.status = self.export_saves(self.lang());
                None
            }
            #[cfg(not(target_os = "android"))]
            Target::SavesImport => {
                self.saves.status = self.pick_import(self.lang());
                None
            }
            #[cfg(not(target_os = "android"))]
            Target::SavesCfg => {
                self.saves.port_cfg = !self.saves.port_cfg;
                None
            }
            Target::Developer => {
                self.dev = !self.dev;
                None
            }
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            t @ (Target::Action(_) | Target::Choice(_) | Target::Replay(_)) => {
                self.nav.pending = Some((t, 0));
                None
            }
            Target::Reset => {
                if std::mem::take(&mut self.nav.reset_ask) {
                    self.reset();
                } else {
                    self.nav.reset_ask = true;
                }
                None
            }
        }
    }

    // "Change…" on the game folder: the folder dialog (desktop).
    fn change_folder(&mut self) {
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        if matches!(self.folder, Folder::Native)
            && let Some(d) = super::folder_dialog()
        {
            self.row_mut("game_dir")
                .set(super::Value::Path(d.to_string_lossy().into_owned()));
            self.nav.art = Art::NotTried;
        }
    }

    // "Change…" on a path / file row: its dialog.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn change_path(&mut self, key: &'static str) {
        if let Some(v) = super::pick_path(self.row(key).e, self.lang()) {
            self.row_mut(key).set(super::Value::Path(v));
        }
    }

    fn help_of(&self, t: Target, lang: Lang) -> String {
        let text = match t {
            Target::Play if !self.folder_ok() => i18n::ui(lang, "select_folder_first"),
            Target::Play => i18n::ui(lang, "help_play"),
            Target::Folder if !self.game_dir().is_empty() => {
                return tilde(self.game_dir().trim_end_matches(['/', '\\']));
            }
            Target::Folder => crate::settings::find("game_dir").map_or("", |e| i18n::help(lang, e)),
            Target::Row(k) => i18n::help(lang, self.row(k).e),
            Target::Developer => i18n::ui(lang, "developer_hint"),
            Target::Reset => i18n::ui(lang, "reset"),
            Target::Back => i18n::ui(lang, "help_error_back"),
            Target::Exit => i18n::ui(lang, "help_error_quit"),
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            Target::Choice("rp_sort") => i18n::ui(lang, "rp_hint"),
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            Target::Action(k) | Target::Choice(k) => i18n::ui(lang, k),
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            Target::Replay(_) => i18n::ui(lang, "rp_hint"),
            #[cfg(not(target_os = "android"))]
            Target::SavesExport | Target::SavesImport => i18n::ui(lang, "saves_hint"),
            #[cfg(not(target_os = "android"))]
            Target::SavesCfg => i18n::ui(lang, "saves_port_cfg_hint"),
        };
        let mut c = text.chars();
        c.next()
            .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
    }

    // The row's mark: the focus bar under key / pad input, a hover band under the pointer, pressed on touch.
    fn target(&mut self, ui: &Ui, p: &Painter, band: Rect, t: Target) -> egui::Response {
        let id = Id::unique(("launcher target", format!("{t:?}")));
        let resp = ui.interact(band, id, Sense::click());
        if resp.contains_pointer() {
            self.nav.input.over(t);
        }
        self.nav.order.push(t);
        let bar = self.nav.input.bar() && self.nav.focus == Some(t);
        theme::mark(
            p,
            band,
            4.0,
            bar,
            self.nav.input.lit(t),
            resp.is_pointer_button_down_on(),
        );
        if bar && self.nav.moved {
            resp.scroll_to_me(None);
        }
        if resp.clicked() {
            self.nav.focus = Some(t);
        }
        resp
    }

    // Touch input: no key hints or key letters, a touch device line.
    pub(super) fn touch_ui(&self) -> bool {
        self.touch || self.nav.input.modality == theme::Modality::Touch
    }

    // `t` is drawn bright: it has the focus bar or the pointer.
    pub(super) fn shows(&self, t: Target) -> bool {
        (self.nav.input.bar() && self.nav.focus == Some(t)) || self.nav.input.lit(t)
    }

    // The title art column (on windows 760 pt and wider) and the lacquer panel: art width, panel, text x and right.
    fn frame(&mut self, ui: &Ui, p: &Painter, full: Rect, screen: Rect) -> (f32, Rect, f32, f32) {
        // A phone-sized window (under 420 pt tall) keeps a narrow strip of it.
        let art_w = if full.height() < 420.0 && full.width() >= 600.0 {
            (full.width() * 0.12).clamp(80.0, 130.0).round()
        } else if full.width() >= 760.0 {
            (full.width() * 0.344).round()
        } else {
            0.0
        };
        if art_w > 0.0 {
            let r = Rect::from_x_y_ranges(screen.min.x..=full.min.x + art_w, screen.y_range());
            self.art_ui(ui.ctx(), p, r);
        }
        let panel = Rect::from_x_y_ranges(full.min.x + art_w..=screen.max.x, screen.y_range());
        theme::lacquer(p, panel, Edge::Left);
        let x = panel.min.x + if art_w > 0.0 { 42.0 } else { 24.0 };
        let right = full.max.x - if art_w > 0.0 { 38.0 } else { 24.0 };
        (art_w, panel, x, right)
    }

    #[expect(clippy::too_many_lines, reason = "one painted screen, top to bottom")]
    pub(super) fn settings_screen(&mut self, ui: &mut Ui) -> Option<Action> {
        let lang = self.lang();
        self.nav.input.begin(ui.ctx(), cfg!(target_os = "android"));
        let mut act = self.keys(ui);
        let full = ui.max_rect();
        let screen = ui.ctx().viewport_rect();
        let p = ui.painter().with_clip_rect(screen);
        let (art_w, panel, x, right) = self.frame(ui, &p, full, screen);
        let w = right - x;
        let band_x = panel.min.x + 6.0..=screen.max.x;
        if self.nav.focus.is_none() {
            self.nav.focus = Some(Target::Play);
        }
        self.nav.order.clear();
        let ver = i18n::ui_fmt(lang, "version", &[("version", &super::version_line())]);
        let caption = i18n::ui_fmt(lang, "launcher_caption", &[("version", &ver)]);
        // Short windows (under 600 pt) pack the head closer so the rows keep most of the height; under 500 pt with
        // the title art beside it the heading (the art's title) is left out.
        let compact = full.height() < 600.0;
        // A phone-sized window: the heading on one line, Play beside the game folder.
        let phone = full.height() < 420.0;
        let bare = full.height() < 500.0 && art_w > 0.0 && !phone;
        let (head_y, play_y, play_h) = if phone {
            (8.0, 58.0, 56.0)
        } else if bare {
            (0.0, 8.0, PLAY_H - 12.0)
        } else if compact {
            (12.0, 78.0, PLAY_H - 10.0)
        } else {
            (26.0, 112.0, PLAY_H)
        };
        // Phone: the title on one line with the tabs beside it (the caption instead where there are no tabs);
        // without room for both the tabs keep the line and the art strip carries the title.
        let tabs_in_head = phone && cfg!(not(any(target_arch = "wasm32", target_os = "android")));
        if phone {
            let g = p.layout_no_wrap(
                i18n::ui(lang, "launcher_title").to_string(),
                theme::serif(HEAD * 0.72),
                theme::TEXT_HI,
            );
            let (gw, gh) = (g.size().x, g.size().y);
            let ty = full.min.y + head_y;
            #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
            {
                let tw = self.tabs_width(&p, lang);
                let tx = if gw + 36.0 + tw <= w {
                    theme::outlined(&p, pos2(x, ty), g.clone(), theme::TEXT_HI, false);
                    x + gw + 36.0
                } else {
                    x
                };
                self.tab_bar(ui, &p, pos2(tx, ty + 2.0), lang);
            }
            if !tabs_in_head {
                theme::outlined(&p, pos2(x, ty), g, theme::TEXT_HI, false);
                let c = p.layout(
                    caption.clone(),
                    theme::serif(CAP),
                    theme::ACCENT,
                    (w - gw - 16.0).max(40.0),
                );
                let ch = c.size().y;
                theme::caption_text(&p, pos2(x + gw + 16.0, ty + gh - ch - 4.0), c);
            }
            theme::rule(&p, egui::Rangef::new(x, right), ty + 44.0);
        } else if !bare {
            theme::heading(
                &p,
                pos2(x, full.min.y + head_y),
                i18n::ui(lang, "launcher_title"),
                &caption,
                HEAD,
                CAP,
            );
        }
        let mut y = full.min.y + play_y;
        // Play.
        let play_w = if phone {
            let pw = p
                .layout_no_wrap(
                    i18n::ui(lang, "play").to_string(),
                    theme::serif(PLAY),
                    theme::TEXT,
                )
                .size()
                .x;
            (pw + 40.0).max(w * 0.22)
        } else {
            w
        };
        let r = Rect::from_min_size(pos2(x, y), vec2(play_w, play_h));
        let play_band = if phone {
            Rect::from_x_y_ranges(panel.min.x + 6.0..=r.max.x, r.y_range())
        } else {
            Rect::from_x_y_ranges(band_x.clone(), r.y_range())
        };
        let resp = self.target(ui, &p, play_band, Target::Play);
        let ok = self.can_play();
        let focused = self.shows(Target::Play);
        let color = match (ok, focused) {
            (false, _) => theme::DISABLED,
            (true, true) => theme::TEXT_HI,
            (true, false) => theme::TEXT,
        };
        text_in(
            &p,
            r,
            i18n::ui(lang, "play"),
            theme::serif(PLAY),
            color,
            focused && ok,
        );
        if resp.clicked() && ok {
            act = Some(Action::Play);
        }
        // Game folder: under Play, or beside it on a phone.
        let fr = if phone {
            Rect::from_x_y_ranges(r.max.x + 24.0..=x + w, r.y_range())
        } else {
            y += play_h + if compact { 2.0 } else { 6.0 };
            Rect::from_min_size(pos2(x, y), vec2(w, if compact { 52.0 } else { 58.0 }))
        };
        let folder_band = if phone {
            Rect::from_x_y_ranges(fr.min.x - 12.0..=screen.max.x, fr.y_range())
        } else {
            Rect::from_x_y_ranges(band_x.clone(), fr.y_range())
        };
        let resp = self.target(ui, &p, folder_band, Target::Folder);
        if phone {
            y = fr.min.y;
        }
        self.folder_row(ui, &p, fr, lang, resp.clicked());
        y += fr.height() + 6.0;
        theme::rule(&p, egui::Rangef::new(x, right), y);
        // Tabs.
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        if !tabs_in_head {
            self.tab_bar(ui, &p, pos2(x, y + if compact { 10.0 } else { 16.0 }), lang);
            y += if compact { 46.0 } else { 58.0 };
            theme::rule(&p, egui::Rangef::new(x, right), y);
        }
        y += if compact { 8.0 } else { 10.0 };
        // Bottom: help line (as tall as its text), hints, device.
        let shown = if self.nav.input.bar() || self.touch_ui() {
            self.nav.focus
        } else {
            self.nav.input.hovered()
        };
        let help = shown.map(|t| self.help_of(t, lang)).unwrap_or_default();
        let help_g = p.layout(help, theme::sans(SMALL), theme::HELP_HI, w);
        let help_h = help_g.size().y.max(SMALL * 1.3);
        // Touch has no key hints: the help line takes the bottom.
        let hint_y = full.max.y - if compact { 28.0 } else { 34.0 };
        let help_y = if self.touch_ui() {
            full.max.y - help_h - 10.0
        } else {
            hint_y - help_h - if compact { 10.0 } else { 20.0 }
        };
        let bottom = help_y - if compact { 8.0 } else { 14.0 };
        theme::rule(&p, egui::Rangef::new(x, right), bottom);
        let content = Rect::from_x_y_ranges(panel.min.x + 4.0..=screen.max.x, y..=bottom - 2.0);
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(content);
        if let Some(a) = self.content_ui(&mut child, x, w, lang) {
            act = Some(a);
        }
        theme::shadowed(&p, pos2(x, help_y), help_g, theme::HELP_HI);
        let touch = self.touch_ui();
        let mut hints = vec![
            ("↑↓", i18n::ui(lang, "hint_move")),
            ("←→", i18n::ui(lang, "hint_change")),
        ];
        if cfg!(not(any(target_arch = "wasm32", target_os = "android"))) {
            hints.push(("Q E", i18n::ui(lang, "hint_tabs")));
        }
        hints.push(("Z", i18n::ui(lang, "hint_select")));
        if cfg!(not(target_arch = "wasm32")) {
            hints.push(("Esc", i18n::ui(lang, "hint_quit")));
        }
        if touch {
            hints.clear();
        }
        let hw = theme::key_hints(&p, pos2(x, hint_y), &hints, HINT);
        let dev = p.layout_no_wrap(
            i18n::ui(
                lang,
                if touch {
                    "device_touch"
                } else {
                    "device_keyboard"
                },
            )
            .to_string(),
            theme::sans(HINT),
            theme::HELP,
        );
        let dw = dev.size().x;
        // The input device at the right end of the hints line, when there is room for it.
        if !touch && hw + 24.0 + dw <= w {
            theme::shadowed(&p, pos2(right - dw, hint_y), dev, theme::HELP);
        }
        self.nav.moved = false;
        if self.nav.input.end() {
            ui.ctx().request_repaint();
        }
        act
    }

    // Starting / loading / start failure in the launcher's frame: title art, heading, the status, and on a failure
    // a short cause, the message as helper text and Choose folder / Back / Quit as rows.
    #[expect(clippy::too_many_lines, reason = "one painted screen, top to bottom")]
    pub(super) fn status_screen(&mut self, ui: &mut Ui, status: &Status<'_>) -> Option<Action> {
        let lang = self.lang();
        let error = matches!(status, Status::Error(_));
        if error {
            self.nav.input.begin(ui.ctx(), cfg!(target_os = "android"));
        }
        let dir = self.game_dir().to_string();
        let mut act = if error { self.keys(ui) } else { None };
        if self.game_dir() != dir {
            act = Some(Action::Back);
        }
        let full = ui.max_rect();
        let screen = ui.ctx().viewport_rect();
        let p = ui.painter().with_clip_rect(screen);
        let (_, panel, x, right) = self.frame(ui, &p, full, screen);
        let w = right - x;
        let compact = full.height() < 600.0;
        let ver = i18n::ui_fmt(lang, "version", &[("version", &super::version_line())]);
        let caption = i18n::ui_fmt(lang, "launcher_caption", &[("version", &ver)]);
        let head = if error {
            i18n::ui(lang, "start_failed")
        } else {
            i18n::ui(lang, "launcher_title")
        };
        let hh = theme::heading(
            &p,
            pos2(x, full.min.y + if compact { 12.0 } else { 26.0 }),
            head,
            if error { "" } else { &caption },
            if full.height() < 420.0 {
                HEAD * 0.8
            } else {
                HEAD
            },
            CAP,
        );
        let mut y =
            full.min.y + if compact { 12.0 } else { 26.0 } + hh + if compact { 16.0 } else { 28.0 };
        // Body text at most about 70 characters wide.
        let measure = w.min(SMALL * 38.0);
        let Status::Error(msg) = status else {
            self.progress_body(ui, &p, status, x, w, measure, full);
            return None;
        };
        // A phone-sized window sets the cause smaller so the message keeps room above the rows.
        let cause_size = if full.height() < 420.0 {
            SMALL + 2.0
        } else {
            ROW
        };
        let cause = p.layout(
            i18n::ui(lang, "start_failed_body").to_string(),
            theme::serif(cause_size),
            theme::TEXT,
            measure,
        );
        let ch = cause.size().y;
        theme::outlined(&p, pos2(x, y), cause, theme::TEXT, false);
        y += ch + 14.0;
        let band_x = panel.min.x + 6.0..=screen.max.x;
        let native = matches!(self.folder, Folder::Native)
            && cfg!(not(any(target_arch = "wasm32", target_os = "android")));
        let mut rows: Vec<(Target, &str)> = Vec::new();
        if native {
            rows.push((Target::Folder, i18n::ui(lang, "choose_folder")));
        }
        rows.push((Target::Back, i18n::ui(lang, "back")));
        if cfg!(not(target_arch = "wasm32")) {
            rows.push((Target::Exit, i18n::ui(lang, "quit")));
        }
        if !self
            .nav
            .focus
            .is_some_and(|f| rows.iter().any(|r| r.0 == f))
        {
            self.nav.focus = Some(rows[0].0);
        }
        // Bottom up: hints, help line, the rows, a rule; the message scrolls in what is left.
        let hint_y = full.max.y - if compact { 28.0 } else { 34.0 };
        let shown = if self.nav.input.bar() || self.touch_ui() {
            self.nav.focus
        } else {
            self.nav.input.hovered()
        };
        let help = match shown {
            Some(Target::Folder) => i18n::ui(lang, "help_choose_folder").to_string(),
            Some(t) => self.help_of(t, lang),
            None => String::new(),
        };
        let help_g = p.layout(help, theme::sans(SMALL), theme::HELP_HI, w);
        let help_h = help_g.size().y.max(SMALL * 1.3);
        let help_y = hint_y - help_h - if compact { 10.0 } else { 20.0 };
        // The rows follow the message; a long message scrolls above rows pinned over the help line.
        let msg_h = p
            .layout(
                (*msg).to_string(),
                theme::sans(SMALL),
                theme::HELP,
                measure - 16.0,
            )
            .size()
            .y;
        let rows_top = (help_y - 14.0 - rows.len() as f32 * ROW_H - 8.0)
            .min(y + msg_h + if compact { 16.0 } else { 28.0 });
        theme::rule(&p, egui::Rangef::new(x, right), rows_top);
        self.nav.order.clear();
        let mut ry = rows_top + 8.0;
        for (t, text) in &rows {
            let r = Rect::from_min_size(pos2(x, ry), vec2(w, ROW_H));
            let resp = self.target(
                ui,
                &p,
                Rect::from_x_y_ranges(band_x.clone(), r.y_range()),
                *t,
            );
            let lit = self.shows(*t);
            let color = if lit { theme::TEXT_HI } else { theme::TEXT };
            text_in(&p, r, text, theme::serif(ROW + 1.0), color, lit);
            if resp.clicked() {
                act = match t {
                    Target::Folder => {
                        self.change_folder();
                        (self.game_dir() != dir).then_some(Action::Back)
                    }
                    Target::Back => Some(Action::Back),
                    _ => Some(Action::Quit),
                };
            }
            ry += ROW_H;
        }
        theme::shadowed(&p, pos2(x, help_y), help_g, theme::HELP_HI);
        let mut hints = vec![
            ("↑↓", i18n::ui(lang, "hint_move")),
            ("Z", i18n::ui(lang, "hint_select")),
        ];
        if cfg!(not(target_arch = "wasm32")) {
            hints.push(("Esc", i18n::ui(lang, "hint_quit")));
        }
        if !self.touch_ui() {
            theme::key_hints(&p, pos2(x, hint_y), &hints, HINT);
        }
        // The message box holds whole lines only; the rest scrolls.
        let line = p
            .layout_no_wrap("Ag".into(), theme::sans(SMALL), theme::HELP)
            .size()
            .y;
        let lines = ((rows_top - 12.0 - y) / line).floor().max(1.0);
        let body = Rect::from_x_y_ranges(x..=x + measure, y..=y + lines * line);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        child.set_clip_rect(body);
        let out = egui::ScrollArea::vertical()
            .min_scrolled_height(body.height())
            .max_height(body.height())
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                let g = ui.painter().layout(
                    (*msg).to_string(),
                    theme::sans(SMALL),
                    theme::HELP,
                    ui.available_width().min(measure - 16.0),
                );
                let (rr, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                theme::shadowed(ui.painter(), rr.min, g, theme::HELP);
            });
        crate::uicheck::note_scroll(ui.ctx(), out.inner_rect, body);
        theme::scroll_edges(ui, &out);
        self.nav.moved = false;
        if self.nav.input.end() {
            ui.ctx().request_repaint();
        }
        act
    }

    // The loading / starting line, a sweeping hairline, the Shift hint and the cheats note.
    #[expect(clippy::too_many_arguments, reason = "one painted block")]
    fn progress_body(
        &self,
        ui: &Ui,
        p: &Painter,
        status: &Status<'_>,
        x: f32,
        w: f32,
        measure: f32,
        full: Rect,
    ) {
        let lang = self.lang();
        let (line, hint) = match status {
            Status::Loading => (i18n::ui(lang, "loading"), None),
            Status::Waiting(_) => (
                i18n::ui(lang, "starting"),
                Some(i18n::ui(lang, "waiting_hint")),
            ),
            Status::Error(_) => unreachable!(),
        };
        let mid = full.min.y + full.height() * 0.42;
        let g = p.layout(
            theme::latin_ellipsis(line).into_owned(),
            theme::serif(PLAY),
            theme::TEXT,
            w,
        );
        let gh = g.size().y;
        theme::outlined(p, pos2(x, mid - gh), g, theme::TEXT, false);
        // A lavender segment sweeping along a hairline: the game is still working.
        let ly = mid + 18.0;
        let lw = measure.min(w);
        p.hline(x..=x + lw, ly, egui::Stroke::new(1.0, theme::TRACK));
        let t = ui.input(|i| i.time) as f32;
        let seg = lw * 0.18;
        let sx = x + (t * 0.6).fract() * (lw + seg) - seg;
        p.hline(
            sx.max(x)..=(sx + seg).min(x + lw),
            ly,
            egui::Stroke::new(2.0, theme::FILL),
        );
        ui.ctx().request_repaint();
        let mut hy = ly + 22.0;
        if let Some(h) = hint
            && !self.touch_ui()
            && cfg!(not(any(target_arch = "wasm32", target_os = "android")))
        {
            theme::key_hints(p, pos2(x, hy), &[("Shift", h)], SMALL);
            hy += SMALL * 2.2;
        }
        if let Status::Waiting(Some(note)) = status {
            let g = p.layout(note.clone(), theme::sans(SMALL), theme::WARN, measure);
            theme::shadowed(p, pos2(x, hy), g, theme::WARN);
        }
    }

    // "Game folder", the path and its state, "Change…".
    fn folder_row(&mut self, ui: &Ui, p: &Painter, r: Rect, lang: Lang, clicked: bool) {
        let focused = self.shows(Target::Folder);
        let color = if focused { theme::TEXT_HI } else { theme::TEXT };
        // A narrow row (beside Play on a phone) leaves out the label: the path and its state say what it is.
        let label_w = if r.width() < 600.0 {
            0.0
        } else {
            LABEL_W.min(r.width() * 0.3)
        };
        if label_w > 0.0 {
            text_in(
                p,
                Rect::from_min_size(r.min, vec2(label_w, r.height())),
                i18n::ui(lang, "game_folder").trim_end_matches([':', '：']),
                theme::serif(ROW + 1.0),
                color,
                focused,
            );
        }
        let ok = self.folder_ok();
        let path = match &self.folder {
            Folder::Native => {
                let t = self.game_dir();
                if t.is_empty() {
                    i18n::ui(lang, "folder_hint").to_string()
                } else {
                    tilde(&t)
                }
            }
            Folder::Web(n) | Folder::Bundled(n) if n.is_empty() => {
                i18n::ui(lang, "no_files").into()
            }
            Folder::Web(n) | Folder::Bundled(n) => n.join(", "),
        };
        let native = matches!(self.folder, Folder::Native)
            && cfg!(not(any(target_arch = "wasm32", target_os = "android")));
        let change = p.layout_no_wrap(
            theme::latin_ellipsis(i18n::ui(lang, "change")).into_owned(),
            theme::serif(ROW),
            color,
        );
        let cw = if native { change.size().x + 16.0 } else { 0.0 };
        let px = r.min.x + label_w;
        let pw = r.max.x - px - cw;
        let pg = p.layout_no_wrap(fit_path(p, &path, pw), theme::sans(SMALL), theme::HELP_HI);
        let ph = pg.size().y;
        let state = if ok {
            i18n::ui(lang, "dat_found")
        } else {
            i18n::ui(lang, "dat_missing")
        };
        let mut st = state.chars();
        let state: String = st
            .next()
            .map_or_else(String::new, |f| f.to_uppercase().chain(st).collect());
        let sc = if ok { theme::NOTE } else { theme::DANGER };
        // The state in at most two lines, so the block stays inside the row.
        let mut job =
            egui::text::LayoutJob::simple(state, theme::sans(SMALL - 1.0), sc, pw.max(20.0));
        job.wrap.max_rows = 2;
        let sg = p.fonts_mut(|f| f.layout_job(job));
        let top = r.center().y - (ph + 2.0 + sg.size().y) / 2.0;
        theme::shadowed(p, pos2(px, top), pg, theme::HELP_HI);
        theme::shadowed(p, pos2(px, top + ph + 2.0), sg, sc);
        if native {
            let cr = Rect::from_min_size(
                pos2(
                    r.max.x - change.size().x,
                    r.center().y - change.size().y / 2.0,
                ),
                change.size(),
            );
            theme::outlined(p, cr.min, change, color, false);
            let hit = ui.interact(cr.expand(8.0), Id::unique("change folder"), Sense::click());
            if hit.clicked() || clicked {
                self.change_folder();
            }
        }
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    // The width `tab_bar` takes.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn tabs_width(&self, p: &Painter, lang: Lang) -> f32 {
        let words: f32 = ["tab_settings", "tab_replays", "tab_stats"]
            .iter()
            .map(|id| {
                p.layout_no_wrap(
                    i18n::ui(lang, id).to_string(),
                    theme::serif(ROW),
                    theme::TEXT,
                )
                .size()
                .x + 28.0
            })
            .sum();
        let keys = if self.touch_ui() {
            0.0
        } else {
            2.0 * HINT + 24.0
        };
        words - 28.0 + keys
    }

    fn tab_bar(&mut self, ui: &Ui, p: &Painter, at: egui::Pos2, lang: Lang) {
        let all = [
            (super::Tab::Settings, "tab_settings"),
            (super::Tab::Replays, "tab_replays"),
            (super::Tab::Stats, "tab_stats"),
        ];
        let size = ROW;
        let key = |p: &Painter, x: f32, k: &str| -> f32 {
            let g = p.layout_no_wrap(k.to_string(), theme::sans(HINT), theme::KEYCAP);
            let w = g.size().x;
            theme::shadowed(p, pos2(x, at.y + ROW * 0.5), g, theme::KEYCAP);
            w
        };
        let keys = !self.touch_ui();
        let mut x = at.x;
        if keys {
            x += key(p, x, "Q") + 18.0;
        }
        for (t, id) in all {
            let sel = self.tab == t;
            let col = if sel { theme::TEXT_HI } else { theme::HELP };
            let g = p.layout_no_wrap(i18n::ui(lang, id).to_string(), theme::serif(size), col);
            let (gw, gh) = (g.size().x, g.size().y);
            let r = Rect::from_min_size(pos2(x - 6.0, at.y - 8.0), vec2(gw + 12.0, gh + 18.0));
            theme::outlined(p, pos2(x, at.y), g, col, false);
            if sel {
                p.hline(
                    egui::Rangef::new(x, x + gw),
                    at.y + gh + 8.0,
                    egui::Stroke::new(2.0, theme::ACCENT),
                );
            }
            if ui
                .interact(r, Id::unique(("launcher tab", id)), Sense::click())
                .clicked()
                && !sel
            {
                self.set_tab(t);
            }
            x += gw + 28.0;
        }
        if keys {
            key(p, x - 6.0, "E");
        }
    }

    // The tab's content in the scrolling area under the tabs.
    fn content_ui(&mut self, ui: &mut Ui, x: f32, w: f32, lang: Lang) -> Option<Action> {
        #[cfg_attr(
            any(target_arch = "wasm32", target_os = "android"),
            expect(unused_mut, reason = "only the update check sets it")
        )]
        let mut act = None;
        let left = x - ui.max_rect().min.x;
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        match self.tab {
            super::Tab::Replays => {
                return Self::tab_scroll(ui, |ui| self.replays_ui(ui, x, w, lang));
            }
            super::Tab::Stats => {
                Self::tab_scroll(ui, |ui| self.stats_ui(ui, x, w, lang));
                return None;
            }
            super::Tab::Settings => {}
        }
        let out = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(4.0);
                padded(ui, left, w, |ui| {
                    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
                    if let Some(a) = self.update_ui(ui, lang) {
                        act = Some(a);
                    }
                    self.last_error_ui(ui, lang);
                    if !self.dev {
                        let active: Vec<&str> = self
                            .rows
                            .iter()
                            .filter(|r| {
                                r.e.audience == Audience::Dev && r.value != r.default_value()
                            })
                            .map(|r| i18n::label(lang, r.e))
                            .collect();
                        if !active.is_empty() {
                            ui.colored_label(
                                theme::DANGER,
                                i18n::ui_fmt(lang, "dev_active", &[("list", &active.join(", "))]),
                            );
                        }
                    }
                    if let Some(note) = super::cheats_note(lang, &self.active_cheats()) {
                        ui.colored_label(theme::DANGER, note);
                    }
                    if self.bundled {
                        ui.label(theme::hint(i18n::ui(lang, "bundled_note")));
                    }
                });
                for g in Group::ALL {
                    self.group_rows(ui, g, x, w, lang);
                    #[cfg(not(target_os = "android"))]
                    if g == Group::GameData {
                        self.saves_rows(ui, x, w, lang);
                    }
                }
                self.more_rows(ui, x, w, lang);
                ui.add_space(12.0);
            });
        crate::uicheck::note_scroll(ui.ctx(), out.inner_rect, ui.max_rect());
        theme::scroll_edges(ui, &out);
        act
    }

    // One group: its title as a lavender caption, then its rows.
    fn group_rows(&mut self, ui: &mut Ui, g: Group, x: f32, w: f32, lang: Lang) {
        self.refresh_installed();
        let rows: Vec<usize> = (0..self.rows.len())
            .filter(|&i| self.rows[i].e.group == g && self.shown(i))
            .collect();
        if rows.is_empty() {
            return;
        }
        caption(ui, x, w, i18n::group_title(lang, g));
        let left = x - ui.max_rect().min.x;
        match g {
            Group::Gamepad => padded(ui, left, w, |ui| self.pad_section_ui(ui, &rows, lang)),
            Group::Keyboard => padded(ui, left, w, |ui| {
                egui::Grid::new(g.id())
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| self.keys_rows_ui(ui, &rows, lang));
                ui.add_space(6.0);
                self.keys_presets_ui(ui, lang);
            }),
            _ => {
                let extra = self.pack_choices(lang);
                for i in rows {
                    let x_extra: Vec<(String, String)> = if self.rows[i].e.key == "game_text" {
                        extra.clone()
                    } else {
                        Vec::new()
                    };
                    self.setting_row(ui, i, x, w, lang, &x_extra);
                }
                if g == Group::Language && self.live.is_none() {
                    padded(ui, left, w, |ui| {
                        ui.add_space(6.0);
                        self.translations_ui(ui, lang);
                    });
                }
            }
        }
    }

    // A Bool / Int / Enum row painted with its control; a text or path row with its field.
    fn setting_row(
        &mut self,
        ui: &mut Ui,
        i: usize,
        x: f32,
        w: f32,
        lang: Lang,
        extra: &[(String, String)],
    ) {
        let key = self.rows[i].e.key;
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        if is_path(self.rows[i].e) {
            self.path_row(ui, i, x, w, lang);
            return;
        }
        let ctl = row_ctl(&self.rows[i], lang, extra);
        let full = ui.max_rect();
        let (r, _) = ui.allocate_exact_size(vec2(full.width(), ROW_H), Sense::hover());
        let row = Rect::from_x_y_ranges(x..=x + w, r.y_range());
        let band = Rect::from_x_y_ranges(full.min.x + 2.0..=full.max.x, r.y_range());
        let p = ui.painter().clone();
        let t = Target::Row(key);
        let resp = if matches!(ctl, Ctl::None) {
            ui.interact(band, Id::unique(("text row", key)), Sense::hover())
        } else {
            self.target(ui, &p, band, t)
        };
        let focused = self.shows(t);
        let color = if focused { theme::TEXT_HI } else { theme::TEXT };
        let label_w = LABEL_W.min(w * 0.42);
        let lr = Rect::from_min_size(row.min, vec2(label_w - 12.0, ROW_H));
        text_in(
            &p,
            lr,
            i18n::label(lang, self.rows[i].e),
            theme::serif(ROW),
            color,
            focused,
        );
        let cr = Rect::from_min_max(pos2(row.min.x + label_w, row.min.y), row.max);
        if matches!(ctl, Ctl::None) {
            let dev = self.dev;
            let r = &mut self.rows[i];
            let mut c = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(cr.shrink2(vec2(0.0, 8.0)))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            super::value_ui(&mut c, r, lang, extra);
            if dev {
                super::origin_label(&mut c, r, lang);
            }
            if let Some(e) = r.error() {
                c.colored_label(theme::DANGER, e);
            }
            return;
        }
        let st = CtlStyle {
            size: ROW,
            num: ROW * 0.8,
            cells: false,
            right: false,
            indent: 0.0,
            track: (cr.width() - 150.0).clamp(80.0, 230.0),
            numw: 70.0,
            gap: ROW * 1.1,
            cell_w: 0.0,
            cell_h: 48.0,
            one_line: false,
        };
        if let Some(o) = theme::value_ctl(
            ui,
            &p,
            Id::unique(("launcher ctl", key)),
            cr,
            &ctl,
            focused,
            &st,
        ) {
            self.nav.focus = Some(t);
            row_input(&mut self.rows[i], o, extra);
        } else if resp.clicked() {
            self.rows[i].apply(1, true);
        }
        if self.dev {
            let mut c = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_x_y_ranges(
                        row.max.x - 140.0..=row.max.x,
                        row.y_range(),
                    ))
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            super::origin_label(&mut c, &self.rows[i], lang);
        }
    }

    // A folder / file setting: its label, the path (or what empty means) and "Change…"; "Default" clears a set path.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    fn path_row(&mut self, ui: &mut Ui, i: usize, x: f32, w: f32, lang: Lang) {
        let e = self.rows[i].e;
        let t = Target::Row(e.key);
        let full = ui.max_rect();
        let (r, _) = ui.allocate_exact_size(vec2(full.width(), ROW_H), Sense::hover());
        let row = Rect::from_x_y_ranges(x..=x + w, r.y_range());
        let band = Rect::from_x_y_ranges(full.min.x + 2.0..=full.max.x, r.y_range());
        let p = ui.painter().clone();
        let resp = self.target(ui, &p, band, t);
        let focused = self.shows(t);
        let color = if focused { theme::TEXT_HI } else { theme::TEXT };
        let label_w = LABEL_W.min(w * 0.42);
        text_in(
            &p,
            Rect::from_min_size(row.min, vec2(label_w - 12.0, ROW_H)),
            i18n::label(lang, e),
            theme::serif(ROW),
            color,
            focused,
        );
        let path = self.rows[i].value.text();
        let set = !path.is_empty();
        let word = |p: &Painter, text: &str, c| {
            p.layout_no_wrap(
                theme::latin_ellipsis(text).into_owned(),
                theme::serif(ROW),
                c,
            )
        };
        let change = word(&p, i18n::ui(lang, "change"), color);
        let cr = Rect::from_min_size(
            pos2(
                row.max.x - change.size().x,
                row.center().y - change.size().y / 2.0,
            ),
            change.size(),
        );
        theme::outlined(&p, cr.min, change, color, false);
        let mut right = cr.min.x - 16.0;
        let mut clear = None;
        if set {
            let g = word(&p, i18n::ui(lang, "use_default"), theme::DIM);
            let dr = Rect::from_min_size(pos2(right - g.size().x, cr.min.y), g.size());
            theme::outlined(&p, dr.min, g, theme::DIM, false);
            right = dr.min.x - 16.0;
            clear = Some(dr);
        }
        let px = row.min.x + label_w;
        let shown = if set {
            tilde(&path)
        } else {
            i18n::word(lang, e, "")
                .unwrap_or_else(|| i18n::ui(lang, "not_set"))
                .to_string()
        };
        // A path is shortened in its middle; a sentence (the default's name) wraps, never cut.
        let room = (right - px).max(20.0);
        let pg = if set {
            p.layout_no_wrap(
                fit_path(&p, &shown, room),
                theme::sans(SMALL),
                theme::HELP_HI,
            )
        } else {
            theme::balanced(&p, &shown, theme::sans(SMALL), theme::HELP_HI, room)
        };
        theme::shadowed(
            &p,
            pos2(px, row.center().y - pg.size().y / 2.0),
            pg,
            theme::HELP_HI,
        );
        if let Some(err) = self.rows[i].error() {
            let g = p.layout_no_wrap(err, theme::sans(SMALL - 1.0), theme::DANGER);
            theme::shadowed(&p, pos2(px, row.max.y - g.size().y), g, theme::DANGER);
        }
        if let Some(dr) = clear
            && ui
                .interact(
                    dr.expand(8.0),
                    Id::unique(("path default", e.key)),
                    Sense::click(),
                )
                .clicked()
        {
            self.nav.focus = Some(t);
            self.rows[i].set(super::Value::Path(String::new()));
        } else if resp.clicked() {
            self.nav.focus = Some(t);
            self.change_path(e.key);
        }
    }

    // Saves: export / import and whether an import takes the settings too, as rows under their caption.
    #[cfg(not(target_os = "android"))]
    fn saves_rows(&mut self, ui: &mut Ui, x: f32, w: f32, lang: Lang) {
        self.poll_saves(ui, lang);
        caption(ui, x, w, i18n::ui(lang, "saves_title"));
        let full = ui.max_rect();
        let label_w = LABEL_W.min(w * 0.42);
        for t in [Target::SavesExport, Target::SavesImport, Target::SavesCfg] {
            let (r, _) = ui.allocate_exact_size(vec2(full.width(), ROW_H), Sense::hover());
            let row = Rect::from_x_y_ranges(x..=x + w, r.y_range());
            let band = Rect::from_x_y_ranges(full.min.x + 2.0..=full.max.x, r.y_range());
            let p = ui.painter().clone();
            let resp = self.target(ui, &p, band, t);
            let focused = self.shows(t);
            let color = if focused { theme::TEXT_HI } else { theme::TEXT };
            let (text, lw) = match t {
                Target::SavesExport => (i18n::ui(lang, "saves_export"), w),
                Target::SavesImport => (i18n::ui(lang, "saves_import"), w),
                _ => (i18n::ui(lang, "saves_port_cfg"), label_w - 12.0),
            };
            text_in(
                &p,
                Rect::from_min_size(row.min, vec2(lw, ROW_H)),
                text,
                theme::serif(ROW),
                color,
                focused,
            );
            if t == Target::SavesCfg {
                let ctl = Ctl::Choices {
                    words: vec![
                        i18n::ui(lang, "word_off").into(),
                        i18n::ui(lang, "word_on").into(),
                    ],
                    sel: Some(usize::from(self.saves.port_cfg)),
                };
                let cr = Rect::from_min_max(pos2(row.min.x + label_w, row.min.y), row.max);
                let st = CtlStyle {
                    size: ROW,
                    num: ROW,
                    cells: false,
                    right: false,
                    indent: 0.0,
                    track: 0.0,
                    numw: 0.0,
                    gap: ROW * 1.1,
                    cell_w: 0.0,
                    cell_h: 48.0,
                    one_line: false,
                };
                if let Some(theme::CtlOut::Pick(i)) =
                    theme::value_ctl(ui, &p, Id::unique("saves cfg"), cr, &ctl, focused, &st)
                {
                    self.nav.focus = Some(t);
                    self.saves.port_cfg = i == 1;
                    continue;
                }
            }
            if resp.clicked() {
                self.nav.focus = Some(t);
                self.activate_target();
            }
        }
        let status = match &self.saves.status {
            Some(Ok(m)) => Some((m.clone(), theme::NOTE)),
            Some(Err(e)) => Some((e.clone(), theme::DANGER)),
            None => None,
        };
        if let Some((m, c)) = status {
            let left = x - ui.max_rect().min.x;
            padded(ui, left, w, |ui| {
                ui.colored_label(c, m);
            });
        }
    }

    // Developer view and "Reset to defaults…".
    fn more_rows(&mut self, ui: &mut Ui, x: f32, w: f32, lang: Lang) {
        caption(ui, x, w, i18n::ui(lang, "more_title"));
        let full = ui.max_rect();
        let label_w = LABEL_W.min(w * 0.42);
        for t in [Target::Developer, Target::Reset] {
            let (r, _) = ui.allocate_exact_size(vec2(full.width(), ROW_H), Sense::hover());
            let row = Rect::from_x_y_ranges(x..=x + w, r.y_range());
            let band = Rect::from_x_y_ranges(full.min.x + 2.0..=full.max.x, r.y_range());
            let p = ui.painter().clone();
            let resp = self.target(ui, &p, band, t);
            let focused = self.shows(t);
            if t == Target::Developer {
                let color = if focused { theme::TEXT_HI } else { theme::TEXT };
                text_in(
                    &p,
                    Rect::from_min_size(row.min, vec2(label_w, ROW_H)),
                    i18n::ui(lang, "developer"),
                    theme::serif(ROW),
                    color,
                    focused,
                );
                let ctl = Ctl::Choices {
                    words: vec![
                        i18n::ui(lang, "word_off").into(),
                        i18n::ui(lang, "word_on").into(),
                    ],
                    sel: Some(usize::from(self.dev)),
                };
                let cr = Rect::from_min_max(pos2(row.min.x + label_w, row.min.y), row.max);
                let st = CtlStyle {
                    size: ROW,
                    num: ROW,
                    cells: false,
                    right: false,
                    indent: 0.0,
                    track: 0.0,
                    numw: 0.0,
                    gap: ROW * 1.1,
                    cell_w: 0.0,
                    cell_h: 48.0,
                    one_line: false,
                };
                if let Some(theme::CtlOut::Pick(i)) =
                    theme::value_ctl(ui, &p, Id::unique("dev ctl"), cr, &ctl, focused, &st)
                {
                    self.dev = i == 1;
                } else if resp.clicked() {
                    self.dev = !self.dev;
                }
            } else {
                let text = if self.nav.reset_ask {
                    format!("{}?", i18n::ui(lang, "reset"))
                } else {
                    format!("{}…", i18n::ui(lang, "reset"))
                };
                let color = if self.nav.reset_ask {
                    theme::QUESTION
                } else if focused {
                    theme::TEXT_HI
                } else {
                    theme::DANGER
                };
                text_in(&p, row, &text, theme::serif(ROW), color, focused);
                if resp.clicked() {
                    self.nav.focus = Some(t);
                    self.activate_target();
                }
                if !focused {
                    self.nav.reset_ask = false;
                }
            }
        }
        let errors = self.rows.iter().any(|r| r.error().is_some());
        if errors {
            let left = x - ui.max_rect().min.x;
            padded(ui, left, w, |ui| {
                ui.colored_label(theme::DANGER, i18n::ui(lang, "fix_values"));
            });
        }
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
impl Model {
    // A row's band and mark for target `t` over `r` (the text column's rows); clicked or activated by Z.
    pub(super) fn row_target(&mut self, ui: &Ui, r: Rect, t: Target) -> (egui::Response, bool) {
        let full = ui.max_rect();
        let band = Rect::from_x_y_ranges(full.min.x + 2.0..=full.max.x, r.y_range());
        let p = ui.painter().clone();
        let resp = self.target(ui, &p, band, t);
        let key = self
            .nav
            .pending
            .take_if(|(p, d)| *p == t && *d == 0)
            .is_some();
        (resp.clone(), resp.clicked() || key)
    }

    // The band and mark of `t` without taking its Z.
    pub(super) fn mark_target(&mut self, ui: &Ui, band: Rect, t: Target) {
        let p = ui.painter().clone();
        self.target(ui, &p, band, t);
    }

    // Z (0) or ← / → (-1 / 1) pressed on `t` this pass.
    pub(super) fn take_pending(&mut self, t: Target) -> Option<(Target, i32)> {
        self.nav.pending.take_if(|(p, _)| *p == t)
    }

    // A text action as a menu row (Refresh, Import replays…): its serif word; true when chosen.
    pub(super) fn action_row(
        &mut self,
        ui: &mut Ui,
        x: f32,
        w: f32,
        key: &'static str,
        lang: Lang,
    ) -> bool {
        let (r, _) = ui.allocate_exact_size(vec2(ui.max_rect().width(), ROW_H), Sense::hover());
        let row = Rect::from_x_y_ranges(x..=x + w, r.y_range());
        let t = Target::Action(key);
        let (_, go) = self.row_target(ui, row, t);
        let color = if self.shows(t) {
            theme::TEXT_HI
        } else {
            theme::TEXT
        };
        text_in(
            ui.painter(),
            row,
            i18n::ui(lang, key),
            theme::serif(ROW),
            color,
            self.shows(t),
        );
        go
    }

    // A labelled word choice (sort order, filters): the picked index when it changes.
    #[expect(clippy::too_many_arguments, reason = "one painted row")]
    pub(super) fn choice_row(
        &mut self,
        ui: &mut Ui,
        x: f32,
        w: f32,
        key: &'static str,
        lang: Lang,
        words: Vec<String>,
        sel: usize,
    ) -> Option<usize> {
        let label_w = theme::label_width(
            ui.painter(),
            &[i18n::ui(lang, key)],
            &theme::serif(ROW),
            LABEL_W.min(w * 0.36),
            w * 0.5,
        );
        let st = CtlStyle {
            size: ROW,
            num: ROW * 0.8,
            cells: false,
            right: false,
            indent: 0.0,
            track: 0.0,
            numw: 70.0,
            gap: ROW * 1.1,
            cell_w: 0.0,
            cell_h: 48.0,
            one_line: false,
        };
        let n = words.len();
        let ctl = Ctl::Choices {
            words,
            sel: Some(sel),
        };
        let h = ROW_H.max(theme::ctl_height(ui.painter(), &ctl, &st, w - label_w) + 16.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.max_rect().width(), h), Sense::hover());
        let row = Rect::from_x_y_ranges(x..=x + w, r.y_range());
        let t = Target::Choice(key);
        let (resp, go) = self.row_target(ui, row, t);
        let focused = self.shows(t);
        let color = if focused { theme::TEXT_HI } else { theme::TEXT };
        let p = ui.painter().clone();
        text_in(
            &p,
            Rect::from_min_size(row.min, vec2(label_w - 12.0, h)),
            i18n::ui(lang, key),
            theme::serif(ROW),
            color,
            focused,
        );
        let cr = Rect::from_min_max(pos2(row.min.x + label_w, row.min.y), row.max);
        let mut out =
            match theme::value_ctl(ui, &p, Id::unique(("choice", key)), cr, &ctl, focused, &st) {
                Some(theme::CtlOut::Pick(i)) => Some(i),
                Some(theme::CtlOut::Step(d)) => {
                    Some(sel.saturating_add_signed(d as isize).min(n - 1))
                }
                _ => None,
            };
        if let Some((_, d)) = self.nav.pending.take_if(|(p, _)| *p == t) {
            out = Some(if d == 0 {
                (sel + 1) % n
            } else {
                sel.saturating_add_signed(d as isize).min(n - 1)
            });
        } else if go || (resp.clicked() && out.is_none()) {
            out = Some((sel + 1) % n);
        }
        out.filter(|&i| i != sel)
    }

    // The scroll area of a tab: the rest of the content rect, scrolling only when the rows need it.
    pub(super) fn tab_scroll<R>(ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> R {
        let space = ui.max_rect();
        let out = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(4.0);
                let r = f(ui);
                ui.add_space(12.0);
                r
            });
        crate::uicheck::note_scroll(ui.ctx(), out.inner_rect, space);
        theme::scroll_edges(ui, &out);
        out.inner
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
// Sans text wrapped to `w` in the text column, as a flow item: notes, status lines, the tab hint.
pub(super) fn note(ui: &mut Ui, x: f32, w: f32, text: &str, size: f32, color: egui::Color32) {
    if text.is_empty() {
        return;
    }
    let g = ui
        .painter()
        .layout(text.to_string(), theme::sans(size), color, w.max(10.0));
    let (r, _) = ui.allocate_exact_size(
        vec2(ui.max_rect().width(), g.size().y + 8.0),
        Sense::hover(),
    );
    theme::shadowed(ui.painter(), pos2(x, r.min.y + 4.0), g, color);
}

// A folder / file setting (painted with "Change…" on desktop).
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn is_path(e: &crate::settings::Entry) -> bool {
    matches!(
        e.kind,
        crate::settings::Kind::Path | crate::settings::Kind::File
    )
}

// Serif text in `r`, left and vertically centred; "…" after Latin / Cyrillic as three periods.
pub(super) fn text_in(
    p: &Painter,
    r: Rect,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    glow: bool,
) {
    let g = p.layout(
        theme::latin_ellipsis(text).into_owned(),
        font,
        color,
        r.width().max(10.0),
    );
    let h = g.size().y;
    theme::outlined(p, pos2(r.min.x, r.center().y - h / 2.0), g, color, glow);
}

// A group title: small lavender serif caption over a rule.
pub(super) fn caption(ui: &mut Ui, x: f32, w: f32, text: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.max_rect().width(), 34.0), Sense::hover());
    let p = ui.painter();
    let g = p.layout_no_wrap(text.to_string(), theme::serif(CAP), theme::ACCENT);
    theme::caption_text(p, pos2(x, r.min.y + 12.0), g);
    theme::rule(p, egui::Rangef::new(x, x + w), r.max.y - 1.0);
}

// Egui-flow content (lists, buttons) in the text column.
// An empty block takes no height (the row's least height is lifted while it is opened).
fn padded<R>(ui: &mut Ui, left: f32, w: f32, f: impl FnOnce(&mut Ui) -> R) -> R {
    let least = ui.spacing().interact_size.y;
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 0.0;
        ui.horizontal(|ui| {
            ui.add_space(left);
            ui.vertical(|ui| {
                ui.spacing_mut().interact_size.y = least;
                ui.set_max_width(w);
                f(ui)
            })
            .inner
        })
        .inner
    })
    .inner
}

// A path for a row: the root, "…" and the folder itself ("~/Games/…/TH08 - Imperishable Night"), shorter still
// when `w` points do not hold it; the whole path is in the help line.
fn fit_path(p: &Painter, path: &str, w: f32) -> String {
    let width = |s: &str| {
        p.layout_no_wrap(s.to_string(), theme::sans(SMALL), theme::TEXT)
            .size()
            .x
    };
    let sep = if path.contains('\\') && !path.contains('/') {
        '\\'
    } else {
        '/'
    };
    let trimmed = path.trim_end_matches(['/', '\\']);
    let parts: Vec<&str> = trimmed.split(['/', '\\']).collect();
    let n = parts.len();
    let mut tries = Vec::new();
    if n <= 3 {
        tries.push(trimmed.to_string());
    }
    let last = parts[n - 1];
    if n > 3 {
        tries.push(format!("{}{sep}{}{sep}…{sep}{last}", parts[0], parts[1]));
    }
    if n > 2 {
        tries.push(format!("{}{sep}…{sep}{last}", parts[0]));
    }
    if n > 1 {
        tries.push(format!("…{sep}{last}"));
    }
    if let Some(t) = tries.iter().find(|t| width(t) <= w) {
        return t.clone();
    }
    let mut s: String = last.to_string();
    while !s.is_empty() && width(&format!("…{sep}{s}…")) > w {
        s.pop();
    }
    format!("…{sep}{s}…")
}

// Blocks until the title art has loaded (offscreen shots).
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
impl Model {
    // Offscreen shots: the settings list scrolled to its last row under key focus.
    pub fn shot_scroll_end(&mut self) {
        self.nav.focus = Some(Target::Reset);
        self.nav.input.modality = theme::Modality::Keys;
        self.nav.moved = true;
    }

    // Offscreen shots without game data: a stand-in for title00.png, shown as the real art would be.
    pub fn shot_art(&mut self, img: &image::RgbaImage) {
        let (w, h) = img.dimensions();
        let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = tx.send(Some(img));
        self.nav.art = Art::Loading(rx);
    }

    pub(super) fn settle_art(&mut self) {
        if let Art::Loading(rx) = &self.nav.art {
            let img = rx.recv().ok().flatten();
            self.nav.art = match img {
                Some(_) => Art::Loading({
                    let (tx, rx) = std::sync::mpsc::channel();
                    let _ = tx.send(img);
                    rx
                }),
                None => Art::None,
            };
        }
    }
}

// $HOME shortened to ~ in a shown path.
pub(super) fn tilde(path: &str) -> String {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    if let Some(h) = std::env::var("HOME").ok().filter(|h| !h.is_empty())
        && let Some(rest) = path.strip_prefix(&h)
    {
        return format!("~{rest}");
    }
    path.to_string()
}
