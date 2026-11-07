//! Port menu (F1 / Android Back / pad Select / resume from the background): a pause card, docked in the letterbox when it fits, and a tabbed settings page, drawn with egui over the held game's last frame.

use super::touch::{TouchConfig, TouchLayout};
use super::touchedit::{EditOut, TouchEdit};
use crate::i18n::{self, Lang};
use crate::launcher::{self, Action};
use crate::settings::Settings;
use egui::{Align2, Color32, Id, LayerId, Order, RichText, Stroke, StrokeKind, vec2};
use th08_render::gfx::{Gfx, OverlayTarget, SafeArea};
use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{KeyCode, NativeKeyCode, PhysicalKey};
use winit::window::Window;

// Menu input from keys and pad buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    Activate,
    Back,
    PrevTab,
    NextTab,
}

impl Nav {
    // Keyboard; Esc, F1 and Android Back go through the app (`PortMenu::back`).
    pub fn of_key(k: KeyCode, shift: bool) -> Option<Self> {
        Some(match k {
            KeyCode::ArrowUp => Self::Up,
            KeyCode::Tab if shift => Self::Up,
            KeyCode::ArrowDown | KeyCode::Tab => Self::Down,
            KeyCode::ArrowLeft => Self::Left,
            KeyCode::ArrowRight => Self::Right,
            KeyCode::KeyZ | KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                Self::Activate
            }
            KeyCode::KeyX | KeyCode::Backspace => Self::Back,
            KeyCode::KeyQ | KeyCode::PageUp => Self::PrevTab,
            KeyCode::KeyE | KeyCode::PageDown => Self::NextTab,
            _ => return None,
        })
    }

    // A gamepad button in `joystick` order: A, B, LB, RB and the d-pad.
    pub fn of_button(i: usize) -> Option<Self> {
        Some(match i {
            0 => Self::Activate,
            1 => Self::Back,
            4 => Self::PrevTab,
            5 => Self::NextTab,
            12 => Self::Up,
            13 => Self::Down,
            14 => Self::Left,
            15 => Self::Right,
            _ => return None,
        })
    }

    // Stick / hat edges (axes of ±1000, y down-positive) from `prev` to `now`.
    fn of_axes(prev: [i32; 2], now: [i32; 2]) -> Vec<Self> {
        let mut out = Vec::new();
        for (i, (neg, pos)) in [(Self::Left, Self::Right), (Self::Up, Self::Down)]
            .into_iter()
            .enumerate()
        {
            if now[i] > STICK_NAV && prev[i] <= STICK_NAV {
                out.push(pos);
            } else if now[i] < -STICK_NAV && prev[i] >= -STICK_NAV {
                out.push(neg);
            }
        }
        out
    }
}

// Stick / hat deflection that moves the focus once.
const STICK_NAV: i32 = 600;

// Theme colours; `theme_contrast` checks text ≥ 4.5:1 and outlines ≥ 3:1 on the panel.
const PANEL: Color32 = Color32::from_rgb(24, 20, 34);
const TEXT: Color32 = Color32::from_rgb(236, 232, 244);
const TEXT_DIM: Color32 = Color32::from_rgb(184, 176, 200);
const ACCENT: Color32 = Color32::from_rgb(104, 62, 168);
const BUTTON: Color32 = Color32::from_rgb(50, 42, 68);
const BUTTON_HOVER: Color32 = Color32::from_rgb(70, 60, 96);
const BORDER: Color32 = Color32::from_rgb(134, 122, 164);
const FOCUS: Color32 = Color32::from_rgb(255, 214, 102);
const WARN: Color32 = Color32::from_rgb(255, 140, 128);

// Text sizes, points (= dp on touch screens, where ppp ≥ the display density).
const BODY: f32 = 16.0;
const SMALL: f32 = 14.0;
const HEADING: f32 = 22.0;

// Where and how big the menu is for one window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    // Window px per egui point.
    pub ppp: f32,
    // Minimum target height, points (48 = 48 dp on touch screens).
    pub target: f32,
    // Card width, points.
    pub card_w: f32,
    // The card is docked as a sheet at the right edge, beside the picture.
    pub sheet: bool,
    // The safe area the held frame is drawn in: narrowed by the sheet.
    pub picture: SafeArea,
    pub touch: bool,
}

// The picture may shrink to this share of its full size to make room for the sheet.
const SHEET_MIN_PICTURE: f32 = 0.8;

#[expect(
    clippy::cast_sign_loss,
    reason = "the sheet width is between the card width and the safe width"
)]
// `density`: the window's scale factor (Android: dp → px); `touch`: a touch screen is the main input.
pub fn layout(safe: SafeArea, (w, h): (u32, u32), density: f32, touch: bool) -> Layout {
    let [_, _, sw, sh] = safe.rect(w, h);
    let fit = |width: f32| (width / 640.0).min(sh / 480.0);
    let ppp = fit(sw).max(0.5).max(density);
    let target = if touch { 48.0 } else { 36.0 };
    let min_card = if touch { 300.0 } else { 280.0 };
    let rest = sw - min_card * ppp;
    let sheet = rest > 0.0 && fit(rest) >= SHEET_MIN_PICTURE * fit(sw) && sh / ppp >= 320.0;
    if !sheet {
        return Layout {
            ppp,
            target,
            card_w: (min_card + 40.0).min(sw / ppp - 24.0),
            sheet,
            picture: safe,
            touch,
        };
    }
    let free = (sw - (640.0 * fit(rest)).floor()).min(400.0 * ppp).floor();
    Layout {
        ppp,
        target,
        card_w: free / ppp,
        sheet,
        picture: SafeArea {
            right: safe.right + free as u32,
            ..safe
        },
        touch,
    }
}

// The card's targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Resume,
    Quit,
    Volume,
    Speed,
    Hitboxes,
    Screenshot,
    TouchLayout,
    Settings,
}

// The card's rows in focus order; the tiles share one row (← / → between them).
fn card_rows(m: &MenuState<'_>) -> Vec<Vec<Item>> {
    let mut rows = vec![vec![Item::Resume]];
    if m.quit {
        rows.push(vec![Item::Quit]);
    }
    rows.push(vec![Item::Volume]);
    if m.speed.is_some() {
        rows.push(vec![Item::Speed]);
    }
    let mut tiles = vec![Item::Hitboxes, Item::Screenshot];
    if m.touch {
        tiles.push(Item::TouchLayout);
    }
    rows.push(tiles);
    rows.push(vec![Item::Settings]);
    rows
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Card,
    Settings,
}

// What a press or click asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Do {
    Nothing,
    Resume,
    Quit,
    Screenshot,
    Speed((u32, u32)),
    Hitboxes,
    EditLayout,
}

const VOLUME: &str = "volume_master";

// Focus: the card's (row, column); on the settings page the tab and the row (0 = the tab bar).
#[derive(Debug)]
struct Focus {
    page: Page,
    card: (usize, usize),
    tab: usize,
    row: usize,
    // "Quit to title" was pressed once; the next press quits.
    confirm: bool,
    // Moved by a key this pass: the focused row is scrolled into view.
    moved: bool,
}

impl Default for Focus {
    fn default() -> Self {
        Self {
            page: Page::Card,
            card: (0, 0),
            tab: 0,
            row: 1,
            confirm: false,
            moved: false,
        }
    }
}

impl Focus {
    fn clamp(&mut self, rows: &[Vec<Item>]) -> Item {
        let r = self.card.0.min(rows.len() - 1);
        let c = self.card.1.min(rows[r].len() - 1);
        self.card = (r, c);
        rows[r][c]
    }

    fn focus_item(&mut self, rows: &[Vec<Item>], it: Item) {
        for (r, row) in rows.iter().enumerate() {
            if let Some(c) = row.iter().position(|&x| x == it) {
                self.card = (r, c);
            }
        }
    }

    fn nav(&mut self, n: Nav, m: &MenuState<'_>, model: &mut launcher::Model) -> Do {
        self.moved = true;
        match self.page {
            Page::Card => self.card_nav(n, m, model),
            Page::Settings => {
                self.settings_nav(n, m, model);
                Do::Nothing
            }
        }
    }

    fn card_nav(&mut self, n: Nav, m: &MenuState<'_>, model: &mut launcher::Model) -> Do {
        let rows = card_rows(m);
        let it = self.clamp(&rows);
        let (r, c) = self.card;
        if n != Nav::Activate && n != Nav::Back {
            self.confirm = false;
        }
        match n {
            Nav::Up => self.card = (r.saturating_sub(1), c),
            Nav::Down => self.card = ((r + 1).min(rows.len() - 1), c),
            Nav::Left | Nav::Right => {
                let dir = if n == Nav::Left { -1 } else { 1 };
                if rows[r].len() > 1 {
                    self.card.1 = c.saturating_add_signed(dir).min(rows[r].len() - 1);
                } else if it == Item::Volume && has_volume(model) {
                    model.step(VOLUME, dir as i32);
                } else if it == Item::Speed
                    && let Some(now) = m.speed
                {
                    let rates = crate::transport::SLOW;
                    let at = rates.iter().position(|&x| x == now).unwrap_or(0);
                    let to = at.saturating_add_signed(dir).min(rates.len() - 1);
                    if to != at {
                        return Do::Speed(rates[to]);
                    }
                }
            }
            Nav::Activate => return self.activate(it, &rows),
            Nav::Back => {
                if !self.back(&rows) {
                    return Do::Resume;
                }
            }
            Nav::PrevTab | Nav::NextTab => {}
        }
        if self.card.0 != r {
            self.card = (self.card.0, self.card.1.min(rows[self.card.0].len() - 1));
        }
        Do::Nothing
    }

    fn settings_nav(&mut self, n: Nav, m: &MenuState<'_>, model: &mut launcher::Model) {
        let tabs = model.live_tabs();
        if tabs.is_empty() {
            if n == Nav::Back {
                self.back(&card_rows(m));
            }
            return;
        }
        self.tab = self.tab.min(tabs.len() - 1);
        let keys = model.live_rows(tabs[self.tab]);
        self.row = self.row.min(keys.len());
        let last = tabs.len() - 1;
        match n {
            Nav::Up => self.row = self.row.saturating_sub(1),
            Nav::Down => self.row = (self.row + 1).min(keys.len()),
            Nav::Left if self.row == 0 => self.tab = self.tab.saturating_sub(1),
            Nav::Right if self.row == 0 => self.tab = (self.tab + 1).min(last),
            Nav::Left => {
                model.step(keys[self.row - 1], -1);
            }
            Nav::Right => {
                model.step(keys[self.row - 1], 1);
            }
            Nav::PrevTab | Nav::NextTab => {
                self.tab = if n == Nav::PrevTab {
                    self.tab.checked_sub(1).unwrap_or(last)
                } else if self.tab == last {
                    0
                } else {
                    self.tab + 1
                };
                self.row = self.row.min(1);
            }
            Nav::Activate if self.row > 0 => {
                model.activate(keys[self.row - 1]);
            }
            Nav::Activate => {}
            Nav::Back => {
                self.back(&card_rows(m));
            }
        }
    }

    // A press or click of a card target.
    fn activate(&mut self, it: Item, rows: &[Vec<Item>]) -> Do {
        self.focus_item(rows, it);
        let confirm = std::mem::take(&mut self.confirm);
        match it {
            Item::Resume => Do::Resume,
            Item::Quit if confirm => Do::Quit,
            Item::Quit => {
                self.confirm = true;
                Do::Nothing
            }
            Item::Volume | Item::Speed => Do::Nothing,
            Item::Hitboxes => Do::Hitboxes,
            Item::Screenshot => Do::Screenshot,
            Item::TouchLayout => Do::EditLayout,
            Item::Settings => {
                self.page = Page::Settings;
                self.tab = 0;
                self.row = 1;
                Do::Nothing
            }
        }
    }

    // One level back; false on the plain card (the menu closes).
    fn back(&mut self, rows: &[Vec<Item>]) -> bool {
        if self.page == Page::Settings {
            self.page = Page::Card;
            self.focus_item(rows, Item::Settings);
            return true;
        }
        std::mem::take(&mut self.confirm)
    }
}

fn has_volume(model: &launcher::Model) -> bool {
    model.live_keys().contains(&VOLUME)
}

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
    // The active gamepad's name for the controller line (set by the app before each redraw).
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
    focus: Focus,
    // Key / pad presses since the last pass.
    navs: Vec<Nav>,
    mods: winit::keyboard::ModifiersState,
    axes: [i32; 2],
    // The style was set for this (target, touch).
    styled: Option<(u32, bool)>,
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
            focus: Focus::default(),
            navs: Vec::new(),
            mods: winit::keyboard::ModifiersState::empty(),
            axes: [0; 2],
            styled: None,
        }
    }

    // Esc / Android Back: closes the layout editor, an open list or the settings page, or cancels the quit
    // question; false when the menu itself should close.
    pub fn back(&mut self) -> bool {
        if self.edit.take().is_some() {
            return true;
        }
        if egui::Popup::is_any_open(&self.ctx) {
            egui::Popup::close_all(&self.ctx);
            return true;
        }
        let rows = card_rows(&self.state_now());
        self.focus.back(&rows)
    }

    // The layout editor is open (F1 / Select then close it first).
    pub fn editing(&self) -> bool {
        self.edit.is_some()
    }

    // A gamepad button press (`joystick` order); true when the menu needs a redraw.
    pub fn pad_button(&mut self, i: usize) -> bool {
        let n = Nav::of_button(i).filter(|_| self.edit.is_none());
        self.navs.extend(n);
        n.is_some()
    }

    // The event-fed pad's stick / hat (Android); true when the menu needs a redraw.
    pub fn pad_axes(&mut self, axes: [i32; 2]) -> bool {
        let n = Nav::of_axes(std::mem::replace(&mut self.axes, axes), axes);
        let any = !n.is_empty() && self.edit.is_none();
        if any {
            self.navs.extend(n);
        }
        any
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

    // Returns whether the window needs a redraw. Menu keys move the focus and never reach egui (no hidden egui
    // focus that Enter could toggle).
    pub fn on_event(&mut self, window: &Window, ev: &WindowEvent) -> bool {
        if let WindowEvent::ModifiersChanged(m) = ev {
            self.mods = m.state();
        }
        if self.edit.is_none()
            && let WindowEvent::KeyboardInput { event, .. } = ev
        {
            let n = match event.physical_key {
                PhysicalKey::Code(k) => Nav::of_key(k, self.mods.shift_key()),
                PhysicalKey::Unidentified(NativeKeyCode::Android(c)) => {
                    crate::joystick::android_button(c).and_then(Nav::of_button)
                }
                _ => None,
            };
            if let Some(n) = n {
                let once = matches!(n, Nav::Activate | Nav::Back);
                if event.state == ElementState::Pressed && !(once && event.repeat) {
                    self.navs.push(n);
                    return true;
                }
                return false;
            }
        }
        self.state.on_window_event(window, ev).repaint
    }

    fn state_now(&self) -> MenuState<'_> {
        MenuState {
            pad: self.controller.as_deref(),
            touch: self.touch.is_some(),
            cheats: &self.cheats,
            hitboxes: super::hitbox::Hitboxes::Off,
            status: self.status.as_deref(),
            speed: self.speed,
            no_saves: self.no_saves,
            quit: self.quit_ok,
        }
    }

    // One pass: the game frame (left of the sheet when docked), the card or settings page in the safe area.
    pub fn redraw(&mut self, window: &Window, gfx: &mut Gfx) -> Option<Action> {
        let size = window.inner_size();
        gfx.resize(size.width, size.height);
        let (w, h) = gfx.surface_size();
        let s = gfx.safe;
        let touch_ui = cfg!(target_os = "android") || self.touch.is_some();
        let lay = layout(s, (w, h), window.scale_factor() as f32, touch_ui);
        let ppp = lay.ppp;
        let z = ppp / window.scale_factor() as f32;
        if self.ctx.zoom_factor() != z {
            window.request_redraw();
        }
        self.ctx.set_zoom_factor(z);
        let key = (lay.target.to_bits(), lay.touch);
        if self.styled != Some(key) {
            style(&self.ctx, &lay);
            self.styled = Some(key);
        }
        let mut raw = self.state.take_egui_input(window);
        raw.safe_area_insets = Some(insets(s, ppp));
        if let Some(e) = &mut self.edit {
            e.set_window((w, h), window.scale_factor(), s);
        }
        let hitboxes = super::hitbox::mode(gfx);
        let m = MenuState {
            pad: self.controller.as_deref(),
            touch: self.touch.is_some(),
            cheats: &self.cheats,
            hitboxes,
            status: self.status.as_deref(),
            speed: self.speed,
            no_saves: self.no_saves,
            quit: self.quit_ok,
        };
        let (model, focus, edit) = (&mut self.model, &mut self.focus, &mut self.edit);
        let before = live_values(model);
        let mut todo: Vec<Do> = self
            .navs
            .drain(..)
            .map(|n| focus.nav(n, &m, model))
            .collect();
        let mut done = None;
        let mut out = self.ctx.run_pass(raw, |ctx| {
            if let Some(e) = edit {
                done = e.ui(ctx, model.lang(), ppp);
            } else {
                todo.extend(menu_ui(ctx, &m, model, focus, &lay));
            }
        });
        focus.moved = false;
        if live_values(model) != before {
            self.changed = true;
            window.request_redraw();
        }
        let mut act = None;
        for d in todo {
            match d {
                Do::Nothing => {}
                Do::Resume => act = Some(Action::Resume),
                Do::Quit => act = Some(Action::QuitToTitle),
                Do::Screenshot => act = Some(Action::Screenshot),
                Do::Speed(r) => act = Some(Action::Speed(r)),
                Do::Hitboxes => {
                    super::hitbox::cycle(gfx);
                }
                Do::EditLayout => {
                    if let Some(t) = &self.touch {
                        self.edit = Some(TouchEdit::new(t.clone()));
                    }
                }
            }
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
        // The sheet narrows the frame's safe area for this present only; the game keeps its own.
        if self.edit.is_none() {
            gfx.safe = lay.picture;
        }
        let shown = gfx.present_last(|t, enc| paint(renderer, pending, &jobs, ppp, &t, enc));
        gfx.safe = s;
        if !shown {
            window.request_redraw();
        }
        act
    }
}

// What the card shows; `touch`: the overlay is on; `quit`: "Quit to title" is offered.
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

// The egui context of a menu, with the CJK font (the language row names 日本語 in every language).
fn new_ctx(s: &Settings) -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_visuals(egui::Visuals::dark());
    i18n::install_fonts(&ctx, s.font.as_deref());
    ctx
}

// The menu theme: the game's purple accent, outlined buttons, high-contrast text, targets of `lay.target` points.
fn style(ctx: &egui::Context, lay: &Layout) {
    use egui::{FontFamily, FontId, TextStyle};
    ctx.all_styles_mut(|st| {
        st.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(HEADING, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(BODY, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(BODY, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(SMALL, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(SMALL, FontFamily::Monospace),
            ),
        ]
        .into();
        let sp = &mut st.spacing;
        sp.interact_size = vec2(40.0, lay.target);
        sp.item_spacing = vec2(8.0, 6.0);
        sp.button_padding = vec2(12.0, 6.0);
        sp.icon_width = if lay.touch { 28.0 } else { 20.0 };
        sp.icon_width_inner = sp.icon_width * 0.6;
        sp.icon_spacing = 8.0;
        sp.slider_rail_height = 8.0;
        sp.combo_width = 160.0;
        let v = &mut st.visuals;
        v.dark_mode = true;
        v.override_text_color = None;
        v.weak_text_color = Some(TEXT_DIM);
        v.window_fill = PANEL;
        v.panel_fill = PANEL;
        v.extreme_bg_color = Color32::from_rgb(12, 10, 18);
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;
        v.selection.bg_fill = ACCENT;
        v.selection.stroke = Stroke::new(1.5, TEXT);
        v.error_fg_color = WARN;
        let w = &mut v.widgets;
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        for (x, fill) in [
            (&mut w.inactive, BUTTON),
            (&mut w.hovered, BUTTON_HOVER),
            (&mut w.active, BUTTON_HOVER),
            (&mut w.open, BUTTON_HOVER),
        ] {
            x.bg_fill = fill;
            x.weak_bg_fill = fill;
            x.bg_stroke = Stroke::new(1.5, BORDER);
            x.fg_stroke = Stroke::new(1.5, TEXT);
            x.corner_radius = egui::CornerRadius::same(6);
        }
        w.hovered.bg_stroke = Stroke::new(2.0, TEXT);
        w.active.bg_stroke = Stroke::new(2.0, TEXT);
    });
}

fn insets(s: SafeArea, ppp: f32) -> egui::SafeAreaInsets {
    egui::SafeAreaInsets(egui::epaint::MarginF32 {
        left: s.left as f32 / ppp,
        right: s.right as f32 / ppp,
        top: s.top as f32 / ppp,
        bottom: s.bottom as f32 / ppp,
    })
}

fn live_values(m: &launcher::Model) -> Vec<(&'static str, String)> {
    m.live_keys()
        .into_iter()
        .filter_map(|k| Some((k, m.value_text(k)?)))
        .collect()
}

// The focus ring inside `rect` (rows in a clipped scroll area).
fn ring_inside(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter()
        .rect_stroke(rect, 8.0, Stroke::new(3.0, FOCUS), StrokeKind::Inside);
}

// The visible focus ring.
fn ring(ui: &egui::Ui, rect: egui::Rect) {
    ui.painter().rect_stroke(
        rect.expand(2.0),
        8.0,
        Stroke::new(3.0, FOCUS),
        StrokeKind::Outside,
    );
}

// The card or the settings page; what was clicked.
fn menu_ui(
    ctx: &egui::Context,
    m: &MenuState<'_>,
    model: &mut launcher::Model,
    focus: &mut Focus,
    lay: &Layout,
) -> Vec<Do> {
    let screen = ctx.viewport_rect();
    let safe = ctx.content_rect();
    let mut out = Vec::new();
    let bg = LayerId::new(Order::Background, Id::unique("port menu bg"));
    if focus.page == Page::Settings {
        ctx.layer_painter(bg)
            .rect_filled(screen, 0.0, Color32::from_black_alpha(170));
        let w = (safe.width() - 24.0).min(560.0);
        let h = safe.height() - 24.0;
        egui::Area::new(Id::unique("port menu settings"))
            .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                panel_frame(ui).show(ui, |ui| {
                    ui.set_width(w - 32.0);
                    ui.set_height(h - 32.0);
                    settings_ui(ui, m, model, focus, lay);
                });
            });
        return out;
    }
    if lay.sheet {
        let x = safe.max.x - lay.card_w;
        ctx.layer_painter(bg).rect_filled(
            egui::Rect::from_min_max(egui::pos2(x, screen.min.y), screen.max),
            0.0,
            PANEL,
        );
        egui::Area::new(Id::unique("port menu sheet"))
            .fixed_pos(egui::pos2(x + 16.0, safe.min.y + 12.0))
            .show(ctx, |ui| {
                ui.set_max_height(safe.height() - 24.0);
                egui::ScrollArea::vertical()
                    .max_height(safe.height() - 24.0)
                    .show(ui, |ui| {
                        out = card_ui(ui, m, model, focus, lay, lay.card_w - 32.0);
                    });
            });
    } else {
        ctx.layer_painter(bg)
            .rect_filled(screen, 0.0, Color32::from_black_alpha(110));
        egui::Area::new(Id::unique("port menu card"))
            .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                panel_frame(ui).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(safe.height() - 56.0)
                        .show(ui, |ui| {
                            out = card_ui(ui, m, model, focus, lay, lay.card_w - 32.0);
                        });
                });
            });
    }
    out
}

fn panel_frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::popup(ui.style())
        .fill(PANEL)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(16))
}

// A full-width or tile button; `primary`: accent filled.
fn wide(ui: &mut egui::Ui, text: &str, w: f32, h: f32, primary: bool) -> egui::Response {
    button(ui, RichText::new(text).color(TEXT), w, h, primary)
}

// A tile: small text in up to two lines, all tiles of a row the same size.
fn tile(ui: &mut egui::Ui, text: &str, w: f32, h: f32) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding.x = 4.0;
        button(ui, RichText::new(text).color(TEXT).small(), w, h, false)
    })
    .inner
}

// A button of exactly w×h points at least.
fn button(ui: &mut egui::Ui, text: RichText, w: f32, h: f32, primary: bool) -> egui::Response {
    let mut b = egui::Button::new(text).wrap().min_size(vec2(w, h));
    if primary {
        b = b.fill(ACCENT).stroke(Stroke::new(2.0, TEXT));
    }
    ui.allocate_ui(vec2(w, h), |ui| ui.add(b)).inner
}

fn card_ui(
    ui: &mut egui::Ui,
    m: &MenuState<'_>,
    model: &mut launcher::Model,
    focus: &mut Focus,
    lay: &Layout,
    w: f32,
) -> Vec<Do> {
    let lang = model.lang();
    let rows = card_rows(m);
    focus.clamp(&rows);
    let mut out = Vec::new();
    let t = lay.target;
    let tile_h = t.max(50.0);
    ui.set_width(w);
    ui.spacing_mut().item_spacing.y = if lay.touch { 6.0 } else { 8.0 };
    ui.vertical_centered(|ui| {
        ui.heading(
            RichText::new(i18n::ui(lang, "paused_title"))
                .color(TEXT)
                .strong(),
        );
    });
    let mut notes: Vec<String> = launcher::cheats_note(lang, m.cheats).into_iter().collect();
    if m.no_saves {
        notes.push(i18n::ui(lang, "slow_nosave").into());
    }
    for n in notes {
        ui.label(RichText::new(format!("⚠ {n}")).color(WARN).small());
    }
    for (r, row) in rows.iter().enumerate() {
        let n = row.len() as f32;
        let tile_w = (w - 8.0 * (n - 1.0)) / n;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for (c, &it) in row.iter().enumerate() {
                let resp = match it {
                    Item::Resume => wide(ui, i18n::ui(lang, "resume"), w, t, true),
                    Item::Quit => {
                        let text = if focus.confirm {
                            i18n::ui(lang, "quit_confirm")
                        } else {
                            i18n::ui(lang, "quit_title")
                        };
                        let r = wide(ui, text, w, t, false);
                        if focus.confirm {
                            ui.painter().rect_stroke(
                                r.rect,
                                6.0,
                                Stroke::new(2.0, WARN),
                                StrokeKind::Inside,
                            );
                        }
                        r
                    }
                    Item::Volume => {
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(
                                vec2(w * 0.34, t),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_size(vec2(w * 0.34, t));
                                    ui.add(egui::Label::new(i18n::ui(lang, "volume")).wrap());
                                },
                            );
                            ui.spacing_mut().slider_width = (w * 0.66 - 72.0).max(60.0);
                            if has_volume(model) {
                                model.value_widget_ui(ui, VOLUME, lang);
                            }
                        })
                        .response
                    }
                    Item::Speed => {
                        let now = m.speed.unwrap_or((1, 1));
                        ui.horizontal_wrapped(|ui| {
                            ui.set_min_height(t);
                            ui.label(i18n::ui(lang, "slow_motion"));
                            for rate in crate::transport::SLOW {
                                let text = crate::transport::rate_label(lang, rate);
                                let b = egui::Button::selectable(now == rate, text)
                                    .min_size(vec2(t * 1.2, t));
                                if ui.add(b).clicked() && now != rate {
                                    out.push(Do::Speed(rate));
                                }
                            }
                        })
                        .response
                    }
                    Item::Hitboxes => tile(ui, &hitboxes_label(lang, m.hitboxes), tile_w, tile_h),
                    Item::Screenshot => {
                        let text = if cfg!(any(target_os = "android", target_arch = "wasm32")) {
                            i18n::ui(lang, "screenshot").to_string()
                        } else {
                            format!("{} (F12)", i18n::ui(lang, "screenshot"))
                        };
                        tile(ui, &text, tile_w, tile_h)
                    }
                    Item::TouchLayout => {
                        tile(ui, i18n::ui(lang, "touch_layout_short"), tile_w, tile_h)
                    }
                    Item::Settings => wide(ui, i18n::ui(lang, "settings_page"), w, t, false),
                };
                if resp.clicked() {
                    focus.card = (r, c);
                    out.push(focus.activate(it, &rows));
                } else if resp.contains_pointer() && ui.input(|i| i.pointer.any_pressed()) {
                    focus.card = (r, c);
                    focus.confirm &= it == Item::Quit;
                }
                if focus.card == (r, c) {
                    ring(ui, resp.rect);
                    if focus.moved {
                        resp.scroll_to_me(None);
                    }
                }
            }
        });
    }
    if let Some(s) = m.status {
        ui.label(RichText::new(s).small().color(TEXT_DIM));
    }
    let name = m.pad.unwrap_or_else(|| i18n::ui(lang, "no_controller"));
    let mut foot = i18n::ui_fmt(lang, "controller", &[("name", name)]);
    let keys = if cfg!(target_os = "android") {
        i18n::ui(lang, "key_back")
    } else {
        "F1 / Esc"
    };
    foot.push_str(&format!(" · {keys}"));
    ui.label(RichText::new(foot).small().color(TEXT_DIM));
    if !lay.touch || m.pad.is_some() {
        ui.label(
            RichText::new(i18n::ui(lang, "nav_hint"))
                .small()
                .color(TEXT_DIM),
        );
    }
    out
}

// The settings page: Back, the tabs, the rows of one tab, then help for the hovered or focused row.
fn settings_ui(
    ui: &mut egui::Ui,
    m: &MenuState<'_>,
    model: &mut launcher::Model,
    focus: &mut Focus,
    lay: &Layout,
) {
    let lang = model.lang();
    let t = lay.target;
    let tabs = model.live_tabs();
    let rows = card_rows(m);
    focus.tab = focus.tab.min(tabs.len().saturating_sub(1));
    ui.horizontal(|ui| {
        let back = format!("‹ {}", i18n::ui(lang, "back"));
        if ui
            .add(egui::Button::new(back).min_size(vec2(t * 1.5, t)))
            .clicked()
        {
            focus.back(&rows);
        }
        ui.add_space(8.0);
        egui::ScrollArea::horizontal()
            .id_salt("tabs")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (i, &g) in tabs.iter().enumerate() {
                        let b =
                            egui::Button::selectable(i == focus.tab, i18n::group_title(lang, g))
                                .min_size(vec2(t * 1.5, t));
                        let r = ui.add(b);
                        if r.clicked() {
                            focus.tab = i;
                            focus.row = focus.row.min(1);
                        }
                        if i == focus.tab && focus.moved {
                            r.scroll_to_me(None);
                        }
                        if focus.row == 0 && i == focus.tab {
                            ring_inside(ui, r.rect);
                        }
                    }
                });
            });
    });
    ui.separator();
    let Some(&g) = tabs.get(focus.tab) else {
        return;
    };
    let keys = model.live_rows(g);
    focus.row = focus.row.min(keys.len());
    // The footer's height as measured last pass (help lines vary).
    let footer_id = egui::Id::unique("port menu footer");
    let footer = ui
        .data(|d| d.get_temp::<f32>(footer_id))
        .unwrap_or(SMALL * 1.4 * 3.0 + 16.0);
    let label_w = (ui.available_width() * 0.42).min(240.0);
    ui.spacing_mut().slider_width = (ui.available_width() - label_w - 96.0).clamp(80.0, 260.0);
    let mut hovered = None;
    egui::ScrollArea::vertical()
        .id_salt(g.id())
        .max_height((ui.available_height() - footer).max(t))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (k, key) in keys.iter().enumerate() {
                let r = model.menu_row_ui(ui, key, lang, label_w);
                let rect = egui::Rect::from_min_max(
                    r.rect.min,
                    egui::pos2(ui.max_rect().max.x, r.rect.max.y),
                );
                if r.contains_pointer() {
                    hovered = Some(*key);
                    if ui.input(|i| i.pointer.any_pressed()) {
                        focus.row = k + 1;
                    }
                }
                if focus.row == k + 1 {
                    ring_inside(ui, rect);
                    if focus.moved {
                        r.scroll_to_me(None);
                    }
                }
            }
        });
    let top = ui.cursor().min.y;
    ui.separator();
    let help = hovered.or(focus.row.checked_sub(1).and_then(|i| keys.get(i).copied()));
    if let Some(e) = help.and_then(crate::settings::find) {
        ui.add(egui::Label::new(RichText::new(i18n::help(lang, e)).small().color(TEXT_DIM)).wrap());
    }
    let mut foot = launcher::version_line();
    if !lay.touch || m.pad.is_some() {
        foot = format!(
            "{} · {} · {foot}",
            i18n::ui(lang, "nav_hint"),
            i18n::ui(lang, "tabs_hint")
        );
    }
    ui.label(RichText::new(foot).small().color(TEXT_DIM));
    let used = ui.cursor().min.y - top;
    ui.data_mut(|d| d.insert_temp(footer_id, used));
}

// "Hitboxes" over "Off", the word from the settings row.
fn hitboxes_label(lang: Lang, m: super::hitbox::Hitboxes) -> String {
    let e = crate::settings::find("hitboxes").expect("hitboxes row");
    let word = i18n::word(lang, e, m.id()).unwrap_or(m.id());
    format!("{}\n{word}", i18n::ui(lang, "hitboxes_short"))
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

// Passes of `render_image`, 0.1 s apart: the layout and fade-ins have settled by the last.
#[cfg(not(target_arch = "wasm32"))]
const PASSES: u32 = 8;

// What a `render_image` shows besides the settings.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, Default)]
pub struct Shot {
    pub quit: bool,
    pub touch: bool,
    // The window's scale factor (dp → px).
    pub density: f32,
    pub pad: Option<String>,
    pub speed: Option<(u32, u32)>,
    pub no_saves: bool,
    pub status: Option<String>,
    // Key presses before the shot (e.g. into the settings page).
    pub navs: Vec<Nav>,
}

// Tools: the safe area to capture the game frame in, so it sits where the menu's sheet leaves it.
#[cfg(not(target_arch = "wasm32"))]
pub fn picture_safe(safe: SafeArea, size: (u32, u32), shot: &Shot) -> SafeArea {
    layout(safe, size, shot.density.max(1.0), shot.touch).picture
}

// Tools: the menu over `frame` (a w×h picture of the game as the menu shows it, see `picture_safe`) on a headless
// device, after a few passes so the layout settles.
#[cfg(not(target_arch = "wasm32"))]
pub fn render_image(
    gfx: &Gfx,
    s: &Settings,
    frame: &image::RgbaImage,
    safe: SafeArea,
    shot: &Shot,
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
    let mut model = launcher::Model::port_menu(s, shot.touch);
    let lay = layout(safe, (w, h), shot.density.max(1.0), shot.touch);
    style(&ctx, &lay);
    let ppp = lay.ppp;
    let (mut renderer, mut pending) = (None, egui::TexturesDelta::default());
    let cheats = s.active_cheats();
    let m = MenuState {
        pad: shot.pad.as_deref(),
        touch: shot.touch,
        cheats: &cheats,
        hitboxes: s.hitboxes,
        status: shot.status.as_deref(),
        speed: shot.speed,
        no_saves: shot.no_saves,
        quit: shot.quit,
    };
    let mut focus = Focus::default();
    for &n in &shot.navs {
        focus.nav(n, &m, &mut model);
    }
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
            safe_area_insets: Some(insets(safe, ppp)),
            ..Default::default()
        };
        raw.viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);
        let out = ctx.run_pass(raw, |ctx| {
            menu_ui(ctx, &m, &mut model, &mut focus, &lay);
        });
        focus.moved = false;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn state(quit: bool, touch: bool, speed: Option<(u32, u32)>) -> MenuState<'static> {
        MenuState {
            pad: None,
            touch,
            cheats: &[],
            hitboxes: super::super::hitbox::Hitboxes::Off,
            status: None,
            speed,
            no_saves: false,
            quit,
        }
    }

    fn model(touch: bool) -> launcher::Model {
        launcher::Model::port_menu(&Settings::default(), touch)
    }

    fn run(f: &mut Focus, m: &MenuState<'_>, model: &mut launcher::Model, navs: &[Nav]) -> Vec<Do> {
        navs.iter().map(|&n| f.nav(n, m, model)).collect()
    }

    #[test]
    fn keys_and_buttons() {
        assert_eq!(Nav::of_key(KeyCode::KeyZ, false), Some(Nav::Activate));
        assert_eq!(Nav::of_key(KeyCode::Enter, false), Some(Nav::Activate));
        assert_eq!(Nav::of_key(KeyCode::KeyX, false), Some(Nav::Back));
        assert_eq!(Nav::of_key(KeyCode::Tab, true), Some(Nav::Up));
        assert_eq!(Nav::of_key(KeyCode::KeyE, false), Some(Nav::NextTab));
        assert_eq!(Nav::of_key(KeyCode::Escape, false), None);
        assert_eq!(Nav::of_key(KeyCode::F1, false), None);
        assert_eq!(Nav::of_button(0), Some(Nav::Activate));
        assert_eq!(Nav::of_button(1), Some(Nav::Back));
        assert_eq!(Nav::of_button(13), Some(Nav::Down));
        assert_eq!(Nav::of_button(crate::joystick::MENU_BUTTON), None);
        assert_eq!(Nav::of_axes([0, 0], [1000, 0]), [Nav::Right]);
        assert_eq!(Nav::of_axes([1000, 0], [1000, 0]), []);
        assert_eq!(Nav::of_axes([0, 0], [0, -800]), [Nav::Up]);
    }

    // Resume has the default focus; Z on it resumes, and X on the plain card closes the menu.
    #[test]
    fn resume() {
        let (m, mut model) = (state(true, false, None), model(false));
        let mut f = Focus::default();
        assert_eq!(f.clamp(&card_rows(&m)), Item::Resume);
        assert_eq!(run(&mut f, &m, &mut model, &[Nav::Activate]), [Do::Resume]);
        let mut f = Focus::default();
        assert_eq!(run(&mut f, &m, &mut model, &[Nav::Back]), [Do::Resume]);
    }

    // Quit to title asks first: the second press quits; moving away or Back cancels the question.
    #[test]
    fn quit_confirm() {
        let (m, mut model) = (state(true, false, None), model(false));
        let mut f = Focus::default();
        let d = run(&mut f, &m, &mut model, &[Nav::Down, Nav::Activate]);
        assert_eq!(d, [Do::Nothing, Do::Nothing]);
        assert!(f.confirm);
        assert_eq!(run(&mut f, &m, &mut model, &[Nav::Activate]), [Do::Quit]);
        let mut f = Focus::default();
        run(
            &mut f,
            &m,
            &mut model,
            &[Nav::Down, Nav::Activate, Nav::Up, Nav::Down],
        );
        assert!(!f.confirm);
        assert_eq!(run(&mut f, &m, &mut model, &[Nav::Activate]), [Do::Nothing]);
        assert_eq!(run(&mut f, &m, &mut model, &[Nav::Back]), [Do::Nothing]);
        assert!(!f.confirm);
        assert_eq!(f.page, Page::Card);
        let rows = card_rows(&m);
        let mut f = Focus::default();
        assert_eq!(f.activate(Item::Quit, &rows), Do::Nothing);
        assert_eq!(f.activate(Item::Quit, &rows), Do::Quit);
        assert!(
            !card_rows(&state(false, false, None))
                .concat()
                .contains(&Item::Quit)
        );
    }

    // The card's focus order, the tiles sharing a row, and Settings › / Back.
    #[test]
    fn focus_order_and_settings() {
        let m = state(true, true, Some((1, 1)));
        let mut model = model(true);
        let rows = card_rows(&m);
        assert_eq!(
            rows,
            [
                vec![Item::Resume],
                vec![Item::Quit],
                vec![Item::Volume],
                vec![Item::Speed],
                vec![Item::Hitboxes, Item::Screenshot, Item::TouchLayout],
                vec![Item::Settings],
            ]
        );
        let mut f = Focus::default();
        let mut seen = vec![f.clamp(&rows)];
        for _ in 0..6 {
            f.nav(Nav::Down, &m, &mut model);
            seen.push(f.clamp(&rows));
        }
        assert_eq!(
            seen,
            [
                Item::Resume,
                Item::Quit,
                Item::Volume,
                Item::Speed,
                Item::Hitboxes,
                Item::Settings,
                Item::Settings
            ]
        );
        run(
            &mut f,
            &m,
            &mut model,
            &[Nav::Up, Nav::Right, Nav::Right, Nav::Right],
        );
        assert_eq!(f.clamp(&rows), Item::TouchLayout);
        assert_eq!(
            run(&mut f, &m, &mut model, &[Nav::Activate]),
            [Do::EditLayout]
        );
        run(&mut f, &m, &mut model, &[Nav::Up]);
        assert_eq!(f.clamp(&rows), Item::Speed);
        assert_eq!(
            run(&mut f, &m, &mut model, &[Nav::Right]),
            [Do::Speed((3, 4))]
        );
        let v = model.value_text(VOLUME);
        run(&mut f, &m, &mut model, &[Nav::Up, Nav::Left]);
        assert_eq!(f.clamp(&rows), Item::Volume);
        assert_ne!(model.value_text(VOLUME), v);
        run(&mut f, &m, &mut model, &[Nav::Down, Nav::Down, Nav::Down]);
        assert_eq!(f.clamp(&rows), Item::Settings);
        run(&mut f, &m, &mut model, &[Nav::Activate]);
        assert_eq!((f.page, f.tab, f.row), (Page::Settings, 0, 1));
        let tabs = model.live_tabs();
        assert_eq!(tabs[0], crate::settings::Group::Sound);
        assert_eq!(model.live_rows(tabs[0])[0], VOLUME);
        // Tabs wrap; the tab bar takes ← / →; a row takes ← / → and Z.
        run(&mut f, &m, &mut model, &[Nav::PrevTab]);
        assert_eq!(f.tab, tabs.len() - 1);
        run(&mut f, &m, &mut model, &[Nav::NextTab, Nav::Up, Nav::Right]);
        assert_eq!((f.tab, f.row), (1, 0));
        run(&mut f, &m, &mut model, &[Nav::Left, Nav::Down]);
        assert_eq!((f.tab, f.row), (0, 1));
        let before = model.value_text(VOLUME);
        run(&mut f, &m, &mut model, &[Nav::Left]);
        assert_ne!(model.value_text(VOLUME), before);
        let keys = model.live_rows(tabs[0]);
        let b = keys.iter().position(|&k| k == "mute_unfocused").unwrap();
        let mut g = Focus {
            page: Page::Settings,
            row: b + 1,
            ..Focus::default()
        };
        let was = model.value_text("mute_unfocused");
        run(&mut g, &m, &mut model, &[Nav::Activate]);
        assert_ne!(model.value_text("mute_unfocused"), was);
        // Back returns to the card with Settings › focused; X there closes the menu.
        assert_eq!(run(&mut f, &m, &mut model, &[Nav::Back]), [Do::Nothing]);
        assert_eq!((f.page, f.clamp(&rows)), (Page::Card, Item::Settings));
        assert_eq!(run(&mut f, &m, &mut model, &[Nav::Back]), [Do::Resume]);
    }

    // Phones: ppp from the density, so 48 pt targets are ≥ 48 dp and text ≥ 14 sp; the sheet docks where the
    // letterbox leaves room, the centred card elsewhere.
    #[test]
    fn density_and_sheet() {
        let phone_safe = SafeArea {
            left: 118,
            ..SafeArea::default()
        };
        let phone = layout(phone_safe, (2400, 1080), 2.625, true);
        assert!(phone.target * phone.ppp >= 48.0 * 2.625);
        assert!(SMALL * phone.ppp >= 14.0 * 2.625);
        assert!(phone.sheet);
        assert!(phone.card_w >= 300.0);
        let [_, _, pw, ph] = phone.picture.rect(2400, 1080);
        assert!((pw / 640.0).min(ph / 480.0) >= SHEET_MIN_PICTURE * 2.25);
        let flat = layout(SafeArea::default(), (2400, 1080), 2.625, true);
        assert_eq!(flat.picture.right, 960);
        assert!((flat.card_w - 960.0 / 2.625).abs() < 1.0);
        let portrait = layout(SafeArea::default(), (1080, 2400), 2.625, true);
        assert!(!portrait.sheet && portrait.picture == SafeArea::default());
        assert!(portrait.target * portrait.ppp >= 48.0 * 2.625);
        let tablet = layout(SafeArea::default(), (2560, 1600), 2.0, true);
        assert!(tablet.target * tablet.ppp >= 48.0 * 2.0);
        for (size, sheet) in [
            ((1280, 960), false),
            ((800, 600), false),
            ((640, 480), false),
            ((1920, 1080), true),
            ((2560, 1440), true),
        ] {
            let l = layout(SafeArea::default(), size, 1.0, false);
            assert_eq!(l.sheet, sheet, "{size:?}");
            assert_eq!(l.ppp, (size.0 as f32 / 640.0).min(size.1 as f32 / 480.0));
            assert!(l.card_w * l.ppp <= size.0 as f32);
        }
        assert_eq!(
            layout(SafeArea::default(), (1280, 960), 2.0, false).ppp,
            2.0
        );
    }

    #[expect(clippy::disallowed_methods, reason = "test-only sRGB curve")]
    fn luminance(c: Color32) -> f32 {
        let f = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    // WCAG: text 4.5:1 on the panel and buttons (the red note sits on the panel only), outlines and the focus ring 3:1 against the panel.
    #[test]
    fn theme_contrast() {
        for bg in [PANEL, BUTTON, BUTTON_HOVER] {
            for fg in [TEXT, TEXT_DIM] {
                assert!(contrast(fg, bg) >= 4.5, "{fg:?} on {bg:?}");
            }
        }
        assert!(contrast(WARN, PANEL) >= 4.5);
        assert!(contrast(TEXT, ACCENT) >= 4.5);
        for c in [BORDER, FOCUS] {
            assert!(contrast(c, PANEL) >= 3.0, "{c:?}");
        }
        assert!(contrast(FOCUS, ACCENT) >= 3.0 && contrast(FOCUS, BUTTON) >= 3.0);
    }
}
