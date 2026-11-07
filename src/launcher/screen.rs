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
}

// Keyboard focus, the targets of the last pass in order, the reset question, the title art.
#[derive(Default)]
pub(super) struct Nav {
    focus: Option<Target>,
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
        let x = rect.max.x - size * 1.45;
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
        let order = &self.nav.order;
        let at = self
            .nav
            .focus
            .and_then(|f| order.iter().position(|&t| t == f));
        let mut act = None;
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
            Target::Folder => {
                self.change_folder();
                None
            }
            Target::Row(key) => {
                self.row_mut(key).apply(1, true);
                None
            }
            Target::Developer => {
                self.dev = !self.dev;
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

    fn help_of(&self, t: Target, lang: Lang) -> String {
        let text = match t {
            Target::Play if !self.folder_ok() => i18n::ui(lang, "select_folder_first"),
            Target::Play => i18n::ui(lang, "help_play"),
            Target::Folder => crate::settings::find("game_dir").map_or("", |e| i18n::help(lang, e)),
            Target::Row(k) => i18n::help(lang, self.row(k).e),
            Target::Developer => i18n::ui(lang, "developer_hint"),
            Target::Reset => i18n::ui(lang, "reset"),
        };
        let mut c = text.chars();
        c.next()
            .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
    }

    // The focus band when `t` has the focus; the pointer moving over `band` takes it.
    fn target(&mut self, ui: &Ui, p: &Painter, band: Rect, t: Target) -> egui::Response {
        let id = Id::unique(("launcher target", format!("{t:?}")));
        let resp = ui.interact(band, id, Sense::click());
        if resp.contains_pointer() && ui.input(|i| i.pointer.is_moving()) {
            self.nav.focus = Some(t);
        }
        self.nav.order.push(t);
        if self.nav.focus == Some(t) {
            theme::focus(p, band, 4.0, resp.is_pointer_button_down_on());
            if self.nav.moved {
                resp.scroll_to_me(None);
            }
        }
        resp
    }

    pub(super) fn settings_screen(&mut self, ui: &mut Ui) -> Option<Action> {
        let lang = self.lang();
        let mut act = self.keys(ui);
        let full = ui.max_rect();
        let screen = ui.ctx().viewport_rect();
        let p = ui.painter().with_clip_rect(screen);
        let art_w = if full.width() >= 760.0 {
            (full.width() * 0.344).round()
        } else {
            0.0
        };
        if art_w > 0.0 {
            let r = Rect::from_x_y_ranges(screen.min.x..=full.min.x + art_w, screen.y_range());
            self.art_ui(ui.ctx(), &p, r);
        }
        let panel = Rect::from_x_y_ranges(full.min.x + art_w..=screen.max.x, screen.y_range());
        theme::lacquer(&p, panel, Edge::Left);
        let x = panel.min.x + if art_w > 0.0 { 42.0 } else { 24.0 };
        let right = full.max.x - if art_w > 0.0 { 38.0 } else { 24.0 };
        let w = right - x;
        let band_x = panel.min.x + 6.0..=screen.max.x;
        if self.nav.focus.is_none() {
            self.nav.focus = Some(Target::Play);
        }
        self.nav.order.clear();
        let ver = i18n::ui_fmt(lang, "version", &[("version", &super::version_line())]);
        let caption = i18n::ui_fmt(lang, "launcher_caption", &[("version", &ver)]);
        theme::heading(
            &p,
            pos2(x, full.min.y + 26.0),
            i18n::ui(lang, "launcher_title"),
            &caption,
            HEAD,
            CAP,
        );
        let mut y = full.min.y + 112.0;
        // Play.
        let r = Rect::from_min_size(pos2(x, y), vec2(w, PLAY_H));
        let resp = self.target(
            ui,
            &p,
            Rect::from_x_y_ranges(band_x.clone(), r.y_range()),
            Target::Play,
        );
        let ok = self.can_play();
        let focused = self.nav.focus == Some(Target::Play);
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
        y += PLAY_H + 6.0;
        // Game folder.
        let fr = Rect::from_min_size(pos2(x, y), vec2(w, 58.0));
        let resp = self.target(
            ui,
            &p,
            Rect::from_x_y_ranges(band_x.clone(), fr.y_range()),
            Target::Folder,
        );
        self.folder_row(ui, &p, fr, lang, resp.clicked());
        y += 64.0;
        theme::rule(&p, egui::Rangef::new(x, right), y);
        // Tabs.
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            self.tab_bar(ui, &p, pos2(x, y + 16.0), lang);
            y += 58.0;
            theme::rule(&p, egui::Rangef::new(x, right), y);
        }
        y += 6.0;
        // Bottom: help line, hints, device.
        let hint_y = full.max.y - 34.0;
        let help_y = hint_y - 40.0;
        let bottom = help_y - 14.0;
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
        let help = self
            .nav
            .focus
            .map(|t| self.help_of(t, lang))
            .unwrap_or_default();
        let g = p.layout(help, theme::sans(SMALL), theme::HELP_HI, w);
        theme::shadowed(&p, pos2(x, help_y), g, theme::HELP_HI);
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
        theme::key_hints(&p, pos2(x, hint_y), &hints, HINT);
        let dev = p.layout_no_wrap(
            i18n::ui(lang, "device_keyboard").to_string(),
            theme::sans(HINT),
            theme::HELP,
        );
        let dw = dev.size().x;
        theme::shadowed(&p, pos2(right - dw, hint_y), dev, theme::HELP);
        self.nav.moved = false;
        act
    }

    // "Game folder", the path and its state, "Change…".
    fn folder_row(&mut self, ui: &Ui, p: &Painter, r: Rect, lang: Lang, clicked: bool) {
        let focused = self.nav.focus == Some(Target::Folder);
        let color = if focused { theme::TEXT_HI } else { theme::TEXT };
        text_in(
            p,
            Rect::from_min_size(r.min, vec2(LABEL_W, r.height())),
            i18n::ui(lang, "game_folder").trim_end_matches([':', '：']),
            theme::serif(ROW + 1.0),
            color,
            focused,
        );
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
        let px = r.min.x + LABEL_W;
        let pw = r.max.x - px - cw;
        let pg = p.layout_no_wrap(fit_path(p, &path, pw), theme::sans(SMALL), theme::HELP_HI);
        let ph = pg.size().y;
        theme::shadowed(p, pos2(px, r.min.y + 6.0), pg, theme::HELP_HI);
        let state = if ok {
            i18n::ui(lang, "dat_found")
        } else {
            i18n::ui(lang, "dat_missing")
        };
        let mut st = state.chars();
        let state: String = st
            .next()
            .map_or_else(String::new, |f| f.to_uppercase().chain(st).collect());
        let sg = p.layout(
            state,
            theme::sans(SMALL - 1.0),
            if ok { theme::NOTE } else { theme::DANGER },
            pw,
        );
        theme::shadowed(
            p,
            pos2(px, r.min.y + 8.0 + ph),
            sg,
            if ok { theme::NOTE } else { theme::DANGER },
        );
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
        let mut x = at.x;
        x += key(p, x, "Q") + 18.0;
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
        key(p, x - 6.0, "E");
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
                return padded(ui, left, w, |ui| {
                    egui::ScrollArea::both()
                        .show(ui, |ui| self.replays_ui(ui, lang))
                        .inner
                });
            }
            super::Tab::Stats => {
                padded(ui, left, w, |ui| {
                    egui::ScrollArea::both().show(ui, |ui| self.stats_ui(ui, lang));
                });
                return None;
            }
            super::Tab::Settings => {}
        }
        egui::ScrollArea::vertical()
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
                        padded(ui, left, w, |ui| self.saves_ui(ui, lang));
                    }
                }
                self.more_rows(ui, x, w, lang);
                ui.add_space(12.0);
            });
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
        let ctl = row_ctl(&self.rows[i], lang, extra);
        let full = ui.max_rect();
        let (r, _) = ui.allocate_exact_size(vec2(full.width(), ROW_H), Sense::hover());
        let row = Rect::from_x_y_ranges(x..=x + w, r.y_range());
        let band = Rect::from_x_y_ranges(full.min.x + 2.0..=full.max.x, r.y_range());
        let p = ui.painter().clone();
        let t = Target::Row(key);
        let focused_before = self.nav.focus == Some(t);
        let resp = if matches!(ctl, Ctl::None) {
            ui.interact(band, Id::unique(("text row", key)), Sense::hover())
        } else {
            self.target(ui, &p, band, t)
        };
        let focused = focused_before || self.nav.focus == Some(t);
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
        if resp.hovered() {
            self.nav.focus = Some(t);
        }
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
            let focused = self.nav.focus == Some(t);
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

// Serif text in `r`, left and vertically centred; "…" after Latin / Cyrillic as three periods.
fn text_in(p: &Painter, r: Rect, text: &str, font: egui::FontId, color: egui::Color32, glow: bool) {
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
fn caption(ui: &mut Ui, x: f32, w: f32, text: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.max_rect().width(), 34.0), Sense::hover());
    let p = ui.painter();
    let g = p.layout_no_wrap(text.to_string(), theme::serif(CAP), theme::ACCENT);
    theme::caption_text(p, pos2(x, r.min.y + 12.0), g);
    theme::rule(p, egui::Rangef::new(x, x + w), r.max.y - 1.0);
}

// Egui-flow content (lists, buttons) in the text column.
fn padded<R>(ui: &mut Ui, left: f32, w: f32, f: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.add_space(left);
        ui.vertical(|ui| {
            ui.set_max_width(w);
            f(ui)
        })
        .inner
    })
    .inner
}

// A path cut from the left to `w` points, folder by folder ("…/TH08 - Imperishable Night").
fn fit_path(p: &Painter, path: &str, w: f32) -> String {
    let width = |s: &str| {
        p.layout_no_wrap(s.to_string(), theme::sans(SMALL), theme::TEXT)
            .size()
            .x
    };
    let mut s = path.to_string();
    while width(&s) > w {
        let body = s.trim_start_matches("…/");
        match body.find(['/', '\\']) {
            Some(i) if i + 1 < body.len() => s = format!("…/{}", &body[i + 1..]),
            _ => break,
        }
    }
    s
}

// Blocks until the title art has loaded (offscreen shots).
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
impl Model {
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
fn tilde(path: &str) -> String {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    if let Some(h) = std::env::var("HOME").ok().filter(|h| !h.is_empty())
        && let Some(rest) = path.strip_prefix(&h)
    {
        return format!("~{rest}");
    }
    path.to_string()
}
