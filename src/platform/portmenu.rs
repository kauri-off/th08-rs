//! Port menu (F1 / Android Back / pad Select / resume from the background): the pause card in the right pillar (16:9), over the playfield (4:3) or in the phone's side bars, and the tabbed settings page, painted with egui over the held game's last frame.

use super::hitbox::Hitboxes;
use super::touch::{TouchConfig, TouchLayout};
use super::touchedit::{EditOut, TouchEdit};
use crate::i18n::{self, Lang};
use crate::launcher::{self, Action};
use crate::settings::Settings;
use crate::theme::{self, Ctl, CtlOut, CtlStyle, Edge};
use egui::{Id, LayerId, Order, Painter, Pos2, Rect, Sense, Ui, pos2, vec2};
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

// Where the card goes: the right pillar beside the moved picture, a column over the 4:3 playfield, or the side
// bars of a wide touch screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Pillar,
    Column,
    Split,
}

// Where and how big the menu is for one window. A point is one pixel of the mock-up at its reference size
// (1920×1080 pillar and phone, 1280×960 column), so the menu scales with the game picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    // Window px per point.
    pub ppp: f32,
    pub shape: Shape,
    // The safe area the held frame is drawn in: narrowed by the pillar.
    pub picture: SafeArea,
    // The game frame in window px: x, y, w, h.
    pub frame: [f32; 4],
    // The window's scale factor (Android: dp → px).
    pub density: f32,
    pub touch: bool,
    // The layout editor's px per point (`theme::ppp`).
    pub edit_ppp: f32,
}

// Pillar width at the reference size, points.
const PILLAR: f32 = 480.0;
// The picture may shrink to this share of its full size to make room for the pillar.
const SHEET_MIN_PICTURE: f32 = 0.8;
// Touch: 120 points are at least 48 dp.
const TOUCH_PX_PER_DP: f32 = 2.5;

// `Gfx::frame_rect` for a safe area.
fn frame_in(safe: SafeArea, (w, h): (u32, u32), integer: bool) -> [f32; 4] {
    let [x, y, sw, sh] = safe.rect(w, h);
    let mut s = (sw / 640.0).min(sh / 480.0);
    if integer && s >= 1.0 {
        s = s.floor();
    }
    let (vw, vh) = ((640.0 * s).round(), (480.0 * s).round());
    [
        x + ((sw - vw) / 2.0).floor(),
        y + ((sh - vh) / 2.0).floor(),
        vw,
        vh,
    ]
}

#[expect(
    clippy::cast_sign_loss,
    reason = "the pillar is narrower than the safe width"
)]
// `density`: the window's scale factor; `touch`: a touch screen is the main input; `integer`: integer scaling.
pub fn layout(
    safe: SafeArea,
    (w, h): (u32, u32),
    density: f32,
    touch: bool,
    integer: bool,
) -> Layout {
    let [_, _, sw, sh] = safe.rect(w, h);
    let fit = |width: f32| (width / 640.0).min(sh / 480.0);
    let f = fit(sw);
    let floor = if touch {
        density / TOUCH_PX_PER_DP
    } else {
        density * 0.6
    };
    let edit_ppp = theme::ppp(sw, sh, density, touch);
    let mk = |shape, ppp: f32, picture| Layout {
        ppp,
        shape,
        picture,
        frame: frame_in(picture, (w, h), integer),
        density,
        touch,
        edit_ppp,
    };
    if sw > sh {
        let u = (f / 2.25).max(floor);
        if touch && (sw - 640.0 * f) / 2.0 >= 360.0 * u {
            return mk(Shape::Split, u, safe);
        }
        let rest = sw - PILLAR * u;
        if rest > 0.0 && fit(rest) >= SHEET_MIN_PICTURE * f && sh / u >= 600.0 {
            let free = (sw - (640.0 * fit(rest)).floor()).max(PILLAR * u).floor();
            let picture = SafeArea {
                right: safe.right + free as u32,
                ..safe
            };
            return mk(Shape::Pillar, (fit(rest) / 2.25).max(floor), picture);
        }
    }
    mk(Shape::Column, (f / 2.0).max(floor), safe)
}

impl Layout {
    // A font size of `mock` points, at least `min_dp` dp (touch) / OS points (desktop).
    fn font(&self, mock: f32, min_dp: f32) -> f32 {
        mock.max(min_dp * self.density / self.ppp)
    }
}

// The card's targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Resume,
    Hitboxes,
    Volume,
    Speed,
    Screenshot,
    TouchLayout,
    Settings,
    Retry,
    Quit,
}

impl Item {
    fn danger(self) -> bool {
        matches!(self, Self::Retry | Self::Quit)
    }
}

// The card's targets in focus order: the column / pillar top to bottom; the phone's left bar, then its quick settings.
fn card_items(m: &MenuState<'_>, split: bool) -> Vec<Item> {
    let quick = {
        let mut v = vec![Item::Volume, Item::Hitboxes];
        if m.speed.is_some() {
            v.push(Item::Speed);
        }
        v.push(Item::Screenshot);
        if m.touch {
            v.push(Item::TouchLayout);
        }
        v
    };
    let run: &[Item] = if m.quit {
        &[Item::Retry, Item::Quit]
    } else {
        &[]
    };
    if split {
        [vec![Item::Resume, Item::Settings], run.to_vec(), quick].concat()
    } else {
        let mut mid = vec![Item::Hitboxes, Item::Volume];
        if m.speed.is_some() {
            mid.push(Item::Speed);
        }
        mid.push(Item::Screenshot);
        if m.touch {
            mid.push(Item::TouchLayout);
        }
        mid.push(Item::Settings);
        [vec![Item::Resume], mid, run.to_vec()].concat()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Card,
    Settings,
}

// A run action waiting for its in-place confirm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ask {
    Retry,
    Quit,
}

// What a press or click asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Do {
    Nothing,
    Resume,
    Quit,
    Retry,
    Screenshot,
    Speed((u32, u32)),
    Hitboxes(Hitboxes),
    EditLayout,
}

const VOLUME: &str = "volume_master";

// Focus: the card item; on the settings page the tab and the row (0 = the tab bar, keys + 1 = "Reset this page").
#[derive(Debug)]
#[expect(clippy::struct_excessive_bools, reason = "independent focus flags")]
struct Focus {
    page: Page,
    item: Item,
    // Retry / Quit asked; `yes` = the confirm's action row is focused (else "Keep playing").
    ask: Option<Ask>,
    yes: bool,
    tab: usize,
    row: usize,
    // "Reset this page" was pressed once; the next press resets.
    reset: bool,
    // Moved by a key this pass: the focused row is scrolled into view.
    moved: bool,
    // Settings rows scrolled by this many points.
    scroll: f32,
    split: bool,
}

impl Default for Focus {
    fn default() -> Self {
        Self {
            page: Page::Card,
            item: Item::Resume,
            ask: None,
            yes: false,
            tab: 0,
            row: 1,
            reset: false,
            moved: false,
            scroll: 0.0,
            split: false,
        }
    }
}

impl Focus {
    fn items(&self, m: &MenuState<'_>) -> Vec<Item> {
        card_items(m, self.split)
    }

    // The focused item, moved to Resume when it is not on the card.
    fn clamp(&mut self, items: &[Item]) -> Item {
        if !items.contains(&self.item) {
            self.item = Item::Resume;
        }
        self.item
    }

    fn nav(&mut self, n: Nav, m: &MenuState<'_>, model: &mut launcher::Model) -> Do {
        self.moved = true;
        match self.page {
            Page::Card => self.card_nav(n, m, model),
            Page::Settings => {
                self.settings_nav(n, model);
                Do::Nothing
            }
        }
    }

    fn card_nav(&mut self, n: Nav, m: &MenuState<'_>, model: &mut launcher::Model) -> Do {
        if self.ask.is_some() {
            match n {
                Nav::Up | Nav::Down => self.yes = !self.yes,
                Nav::Activate => return self.confirm(),
                Nav::Back => self.cancel(),
                _ => {}
            }
            return Do::Nothing;
        }
        let items = self.items(m);
        let it = self.clamp(&items);
        let at = items.iter().position(|&x| x == it).unwrap_or(0);
        match n {
            Nav::Up => self.item = items[at.saturating_sub(1)],
            Nav::Down => self.item = items[(at + 1).min(items.len() - 1)],
            Nav::Left | Nav::Right => {
                let dir = if n == Nav::Left { -1 } else { 1 };
                return self.step(it, dir, m, model);
            }
            Nav::Activate => return self.activate(it, m),
            Nav::Back => return Do::Resume,
            Nav::PrevTab | Nav::NextTab => {}
        }
        Do::Nothing
    }

    // ← / → on a value item.
    fn step(&self, it: Item, dir: i32, m: &MenuState<'_>, model: &mut launcher::Model) -> Do {
        match it {
            Item::Volume if has_volume(model) => {
                model.step(VOLUME, dir);
            }
            Item::Hitboxes => {
                let at = m.hitboxes as usize;
                let to = at.saturating_add_signed(dir as isize).min(2);
                if to != at {
                    return Do::Hitboxes(hitbox_mode(to));
                }
            }
            Item::Speed => {
                if let Some(now) = m.speed {
                    let rates = crate::transport::SLOW;
                    let at = rates.iter().position(|&x| x == now).unwrap_or(0);
                    let to = at.saturating_add_signed(dir as isize).min(rates.len() - 1);
                    if to != at {
                        return Do::Speed(rates[to]);
                    }
                }
            }
            _ => {}
        }
        Do::Nothing
    }

    // Z on the question's focused row: the action, or "Keep playing" back to the item.
    fn confirm(&mut self) -> Do {
        match self.ask {
            Some(a) if self.yes => {
                self.ask = None;
                self.yes = false;
                match a {
                    Ask::Retry => Do::Retry,
                    Ask::Quit => Do::Quit,
                }
            }
            _ => {
                self.cancel();
                Do::Nothing
            }
        }
    }

    fn cancel(&mut self) {
        if let Some(a) = self.ask.take() {
            self.item = match a {
                Ask::Retry => Item::Retry,
                Ask::Quit => Item::Quit,
            };
        }
        self.yes = false;
    }

    fn settings_nav(&mut self, n: Nav, model: &mut launcher::Model) {
        let tabs = model.live_tabs();
        if tabs.is_empty() {
            if n == Nav::Back {
                self.back();
            }
            return;
        }
        self.tab = self.tab.min(tabs.len() - 1);
        let keys = model.live_rows(tabs[self.tab]);
        let last_row = keys.len() + 1;
        self.row = self.row.min(last_row);
        let last = tabs.len() - 1;
        if !matches!(n, Nav::Activate) {
            self.reset = false;
        }
        match n {
            Nav::Up => self.row = self.row.saturating_sub(1),
            Nav::Down => self.row = (self.row + 1).min(last_row),
            Nav::Left if self.row == 0 => self.tab = self.tab.saturating_sub(1),
            Nav::Right if self.row == 0 => self.tab = (self.tab + 1).min(last),
            Nav::Left | Nav::Right if self.row <= keys.len() => {
                model.step(keys[self.row - 1], if n == Nav::Left { -1 } else { 1 });
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
                self.scroll = 0.0;
            }
            Nav::Activate if self.row == last_row => {
                if std::mem::take(&mut self.reset) {
                    model.reset_rows(&keys);
                } else {
                    self.reset = true;
                }
            }
            Nav::Activate if self.row > 0 => {
                model.activate(keys[self.row - 1]);
            }
            Nav::Left | Nav::Right | Nav::Activate => {}
            Nav::Back => {
                self.back();
            }
        }
    }

    // A press or click of a card item.
    fn activate(&mut self, it: Item, m: &MenuState<'_>) -> Do {
        self.item = it;
        match it {
            Item::Resume => Do::Resume,
            Item::Retry => {
                self.ask = Some(Ask::Retry);
                self.yes = false;
                Do::Nothing
            }
            Item::Quit => {
                self.ask = Some(Ask::Quit);
                self.yes = false;
                Do::Nothing
            }
            Item::Volume | Item::Speed => Do::Nothing,
            Item::Hitboxes => Do::Hitboxes(m.hitboxes.next()),
            Item::Screenshot => Do::Screenshot,
            Item::TouchLayout => Do::EditLayout,
            Item::Settings => {
                self.page = Page::Settings;
                self.tab = 0;
                self.row = 1;
                self.scroll = 0.0;
                self.reset = false;
                Do::Nothing
            }
        }
    }

    // One level back; false on the plain card (the menu closes).
    fn back(&mut self) -> bool {
        if self.page == Page::Settings {
            self.page = Page::Card;
            self.item = Item::Settings;
            self.reset = false;
            return true;
        }
        if self.ask.is_some() {
            self.cancel();
            return true;
        }
        false
    }
}

fn hitbox_mode(i: usize) -> Hitboxes {
    match i {
        0 => Hitboxes::Off,
        1 => Hitboxes::Player,
        _ => Hitboxes::All,
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
    // "Give up and retry" / "Quit to title" are offered (`quittitle::available`).
    pub quit_ok: bool,
    opened: (Lang, bool),
    // `Gfx::generation` of the first paint; a change means the renderer is dead (device rebuilt).
    generation: Option<u32>,
    // The active gamepad's name for the device line (set by the app before each redraw).
    pub controller: Option<String>,
    // Active Cheats rows (`Settings::active_cheats`), shown as a no-saves note.
    pub cheats: Vec<&'static str>,
    // The last screenshot's outcome (set by the app).
    pub status: Option<String>,
    // The speed row's rate (practice slow motion or replay transport); None hides it.
    pub speed: Option<(u32, u32)>,
    // Slow motion was used: score.dat and replays are not saved.
    pub no_saves: bool,
    // The paused stage and difficulty (`stage_line`), the heading's caption.
    pub stage: Option<(String, String)>,
    // The live overlay settings (None: no overlay, no layout editor), the open editor, a saved layout not yet taken.
    touch: Option<TouchConfig>,
    edit: Option<TouchEdit>,
    saved: Option<TouchLayout>,
    focus: Focus,
    // Key / pad presses since the last pass.
    navs: Vec<Nav>,
    mods: winit::keyboard::ModifiersState,
    axes: [i32; 2],
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
            stage: None,
            touch,
            edit: None,
            saved: None,
            focus: Focus::default(),
            navs: Vec::new(),
            mods: winit::keyboard::ModifiersState::empty(),
            axes: [0; 2],
        }
    }

    // Esc / Android Back: closes the layout editor, the settings page or the Retry / Quit question; false when
    // the menu itself should close.
    pub fn back(&mut self) -> bool {
        if self.edit.take().is_some() {
            return true;
        }
        self.focus.back()
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

    // Returns whether the window needs a redraw. Menu keys move the focus and never reach egui.
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

    // One pass: the game frame (left of the pillar when docked), the card or settings page.
    pub fn redraw(&mut self, window: &Window, gfx: &mut Gfx) -> Option<Action> {
        let size = window.inner_size();
        gfx.resize(size.width, size.height);
        let (w, h) = gfx.surface_size();
        let s = gfx.safe;
        let touch_ui = cfg!(target_os = "android") || self.touch.is_some();
        let lay = layout(
            s,
            (w, h),
            window.scale_factor() as f32,
            touch_ui,
            gfx.blit_options.integer,
        );
        let ppp = if self.edit.is_some() {
            lay.edit_ppp
        } else {
            lay.ppp
        };
        let z = ppp / window.scale_factor() as f32;
        if self.ctx.zoom_factor() != z {
            window.request_redraw();
        }
        self.ctx.set_zoom_factor(z);
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
            stage: self.stage.as_ref(),
        };
        let (model, focus, edit) = (&mut self.model, &mut self.focus, &mut self.edit);
        focus.split = lay.shape == Shape::Split;
        let before = live_values(model);
        let mut todo: Vec<Do> = self
            .navs
            .drain(..)
            .map(|n| focus.nav(n, &m, model))
            .collect();
        let mut done = None;
        let mut out = self.ctx.run_pass(raw, |ctx| {
            if let Some(e) = edit {
                done = e.ui(ctx, model.lang(), lay.edit_ppp);
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
                Do::Nothing => continue,
                Do::Resume => act = Some(Action::Resume),
                Do::Quit => act = Some(Action::QuitToTitle),
                Do::Retry => act = Some(Action::Retry),
                Do::Screenshot => act = Some(Action::Screenshot),
                Do::Speed(r) => act = Some(Action::Speed(r)),
                Do::Hitboxes(to) => super::hitbox::set_mode(gfx, to),
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
        // The pillar narrows the frame's safe area for this present only; the game keeps its own.
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

// What the card shows; `touch`: the overlay is on; `quit`: Retry and Quit are offered.
struct MenuState<'a> {
    pad: Option<&'a str>,
    touch: bool,
    cheats: &'a [&'static str],
    hitboxes: Hitboxes,
    status: Option<&'a str>,
    speed: Option<(u32, u32)>,
    no_saves: bool,
    quit: bool,
    stage: Option<&'a (String, String)>,
}

// The paused stage ("1" … "6B", "Extra") and difficulty word, while a stage is played.
pub fn stage_line(w: &th08_core::world::World) -> Option<(String, String)> {
    const STAGES: [&str; 9] = ["1", "2", "3", "4A", "4B", "5", "6A", "6B", "Extra"];
    const DIFF: [&str; 5] = ["Easy", "Normal", "Hard", "Lunatic", "Extra"];
    let g = &w.game;
    if g.scene != th08_core::types::SceneId::Game {
        return None;
    }
    let stage = STAGES.get(g.stage)?;
    let diff = DIFF.get(usize::try_from(g.difficulty).ok()?)?;
    Some(((*stage).to_string(), (*diff).to_string()))
}

// The egui context of a menu, with the serif and CJK faces.
fn new_ctx(s: &Settings) -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_visuals(egui::Visuals::dark());
    i18n::install_fonts(&ctx, s.font.as_deref());
    theme::apply(&ctx, theme::Density::new(false));
    ctx
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

// ---- Painting. Every rect below is in points of `Layout` (one point = one mock-up px).

// The window, the safe area, the game frame and the playfield, in points.
struct Geo {
    screen: Rect,
    safe: Rect,
    frame: Rect,
    // Game px → points.
    gs: f32,
}

impl Geo {
    fn new(ctx: &egui::Context, lay: &Layout) -> Self {
        let [x, y, w, h] = lay.frame;
        let u = lay.ppp;
        let frame = Rect::from_min_size(pos2(x / u, y / u), vec2(w / u, h / u));
        Self {
            screen: ctx.viewport_rect(),
            safe: ctx.content_rect(),
            frame,
            gs: frame.width() / 640.0,
        }
    }

    // A rect in game px (640×480 frame coordinates).
    fn game(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(self.frame.min + vec2(x, y) * self.gs, vec2(w, h) * self.gs)
    }
}

// The texts of the menu in its language.
struct Words {
    lang: Lang,
}

impl Words {
    fn ui(&self, id: &str) -> &'static str {
        i18n::ui(self.lang, id)
    }

    fn item(&self, it: Item) -> &'static str {
        match it {
            Item::Resume => self.ui("resume"),
            Item::Hitboxes => {
                crate::settings::find("hitboxes").map_or("Hitboxes", |e| i18n::label(self.lang, e))
            }
            Item::Volume => self.ui("volume"),
            Item::Speed => self.ui("slow_motion"),
            Item::Screenshot => self.ui("screenshot"),
            Item::TouchLayout => self.ui("touch_layout"),
            Item::Settings => self.ui("settings_heading"),
            Item::Retry => self.ui("retry_title"),
            Item::Quit => self.ui("quit_title"),
        }
    }

    // The original's Latin item captions under the Japanese words (ja only).
    fn latin(&self, it: Item) -> Option<&'static str> {
        (self.lang == Lang::Ja).then_some(())?;
        match it {
            Item::Resume => Some("Return to Game"),
            Item::Retry => Some("Give up and Retry"),
            Item::Quit => Some("Quit and Return to Title"),
            _ => None,
        }
    }

    fn help(&self, it: Item) -> &'static str {
        match it {
            Item::Resume => self.ui("help_resume"),
            Item::Hitboxes => {
                crate::settings::find("hitboxes").map_or("", |e| i18n::summary(self.lang, e))
            }
            Item::Volume => {
                crate::settings::find(VOLUME).map_or("", |e| i18n::summary(self.lang, e))
            }
            Item::Speed => self.ui("help_speed"),
            Item::Screenshot => self.ui("help_screenshot"),
            Item::TouchLayout => self.ui("help_touch_layout"),
            Item::Settings => self.ui("help_settings"),
            Item::Retry => self.ui("help_retry"),
            Item::Quit => self.ui("help_quit"),
        }
    }

    fn heading_caption(&self, m: &MenuState<'_>) -> String {
        if self.lang == Lang::Ja {
            return "Pause".into();
        }
        m.stage.map_or_else(String::new, |(s, d)| {
            if s == "Extra" {
                "Extra".into()
            } else {
                i18n::ui_fmt(self.lang, "stage_line", &[("stage", s), ("difficulty", d)])
            }
        })
    }

    fn device(&self, m: &MenuState<'_>, lay: &Layout) -> String {
        let dev = match m.pad {
            Some(p) => p.to_string(),
            None if lay.touch => self.ui("device_touch").into(),
            None => self.ui("device_keyboard").into(),
        };
        format!("{dev} · {}", version(self.lang))
    }

    fn notes(&self, m: &MenuState<'_>) -> Vec<String> {
        let mut v: Vec<String> = launcher::cheats_note(self.lang, m.cheats)
            .into_iter()
            .collect();
        if m.no_saves {
            v.push(self.ui("slow_nosave").into());
        }
        v.extend(m.status.map(str::to_string));
        v
    }
}

// The table's help texts start lower-case (they are also `--help` lines); the menu shows them as a sentence.
fn sentence(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

fn version(lang: Lang) -> String {
    i18n::ui_fmt(lang, "version", &[("version", &launcher::version_line())])
}

// Key hints: plain glyphs on keyboard, A / B discs on a pad, none on touch.
enum Hints {
    Card,
    Confirm,
    Settings,
}

fn hint_items(
    w: &Words,
    m: &MenuState<'_>,
    lay: &Layout,
    which: &Hints,
) -> Vec<(String, &'static str)> {
    if m.pad.is_none() && lay.touch {
        return Vec::new();
    }
    let pad = m.pad.is_some();
    let k = |key: &str, pad_key: &str| -> String { if pad { pad_key } else { key }.to_string() };
    let mut v = vec![(k("↑↓", "↑↓"), w.ui("hint_move"))];
    if !matches!(which, Hints::Confirm) {
        v.push((k("←→", "←→"), w.ui("hint_change")));
    }
    v.push((k("Z", "Ⓐ"), w.ui("hint_select")));
    match which {
        Hints::Card => v.push((k("Esc", "Ⓑ"), w.ui("hint_resume"))),
        Hints::Confirm => v.push((k("X", "Ⓑ"), w.ui("hint_back"))),
        Hints::Settings => {
            v.push((k("X", "Ⓑ"), w.ui("hint_back")));
            v.push((k("Q E", "LB RB"), w.ui("hint_tabs")));
        }
    }
    v
}

// A hint line; pad buttons Ⓐ / Ⓑ as filled discs.
fn draw_hints(p: &Painter, pos: Pos2, items: &[(String, &str)], size: f32) -> f32 {
    let mut x = pos.x;
    for (i, (k, word)) in items.iter().enumerate() {
        if i > 0 {
            x += size * 1.3;
        }
        let disc = match k.as_str() {
            "Ⓐ" => Some("A"),
            "Ⓑ" => Some("B"),
            _ => None,
        };
        if let Some(d) = disc {
            let r = size * 0.7;
            theme::pad_disc(p, pos2(x + r, pos.y + size * 0.62), size, d);
            let g = p.layout_no_wrap((*word).to_string(), theme::sans(size), theme::HELP);
            let gw = g.size().x;
            theme::shadowed(p, pos2(x + 2.0 * r + size * 0.45, pos.y), g, theme::HELP);
            x += 2.0 * r + size * 0.45 + gw;
        } else {
            x += theme::key_hint(p, pos2(x, pos.y), k, word, size);
        }
    }
    x - pos.x
}

// The pointer moved this pass: a row under it takes the focus (a resting pointer leaves key / pad focus alone).
fn pointer_moved(ui: &Ui) -> bool {
    ui.input(|i| i.pointer.is_moving() || i.pointer.any_pressed())
}

// Text in `rect`, left, vertically centred: shrunk to 85 % to stay on one line, else wrapped. The serif face's
// "…" is the centred CJK one; after Latin or Cyrillic letters it is drawn as three periods.
fn label(
    p: &Painter,
    rect: Rect,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    glow: bool,
) -> f32 {
    let text = theme::latin_ellipsis(text);
    let mut font = font;
    let one = p
        .layout_no_wrap(text.to_string(), font.clone(), color)
        .size()
        .x;
    if one > rect.width() && one * 0.85 <= rect.width() {
        font.size *= rect.width() / one;
    }
    let g = p.layout(text.into_owned(), font, color, rect.width().max(10.0));
    let h = g.size().y;
    theme::outlined(
        p,
        pos2(rect.min.x, rect.center().y - h / 2.0),
        g,
        color,
        glow,
    );
    h
}

fn sans_text(
    p: &Painter,
    pos: Pos2,
    text: &str,
    size: f32,
    color: egui::Color32,
    wrap: f32,
) -> f32 {
    let g = p.layout(text.to_string(), theme::sans(size), color, wrap.max(10.0));
    let h = g.size().y;
    theme::shadowed(p, pos, g, color);
    h
}

// The card or the settings page; what was clicked.
fn menu_ui(
    ctx: &egui::Context,
    m: &MenuState<'_>,
    model: &mut launcher::Model,
    focus: &mut Focus,
    lay: &Layout,
) -> Vec<Do> {
    let geo = Geo::new(ctx, lay);
    let mut out = Vec::new();
    let wash = ctx.layer_painter(LayerId::new(
        Order::Background,
        Id::unique("port menu wash"),
    ));
    match lay.shape {
        Shape::Column => {
            wash.rect_filled(geo.game(32.0, 16.0, 384.0, 448.0), 0.0, theme::WASH_DEEP);
        }
        _ => {
            wash.rect_filled(geo.frame, 0.0, theme::WASH);
        }
    }
    egui::Area::new(Id::unique("port menu"))
        .fixed_pos(geo.screen.min)
        .order(Order::Middle)
        .show(ctx, |ui| {
            ui.set_min_size(geo.screen.size());
            ui.set_clip_rect(geo.screen);
            let words = Words { lang: model.lang() };
            if focus.page == Page::Settings {
                settings_ui(ui, m, model, focus, lay, &geo, &words);
            } else {
                out = card_ui(ui, m, model, focus, lay, &geo, &words);
            }
        });
    out
}

// Row geometry of one card shape.
struct CardMetrics {
    head: f32,
    cap: f32,
    resume_h: f32,
    resume: f32,
    row_h: f32,
    row: f32,
    status: f32,
    hint: f32,
    help: f32,
    track: f32,
    numw: f32,
}

fn card_metrics(lay: &Layout) -> CardMetrics {
    let f = |mock, dp| lay.font(mock, dp);
    match lay.shape {
        Shape::Pillar => CardMetrics {
            head: f(42.0, 18.0),
            cap: f(17.0, 11.0),
            resume_h: 88.0,
            resume: f(34.0, 16.0),
            row_h: 72.0,
            row: f(28.0, 14.0),
            status: f(16.0, 11.0),
            hint: f(15.0, 11.0),
            help: f(18.0, 12.0),
            track: 134.0,
            numw: 72.0,
        },
        Shape::Column => CardMetrics {
            head: f(34.0, 18.0),
            cap: f(15.0, 11.0),
            resume_h: 54.0,
            resume: f(28.0, 16.0),
            row_h: 42.0,
            row: f(22.0, 14.0),
            status: f(14.0, 11.0),
            hint: f(14.0, 11.0),
            help: f(14.0, 12.0),
            track: 128.0,
            numw: 58.0,
        },
        Shape::Split => CardMetrics {
            head: f(48.0, 20.0),
            cap: f(21.0, 12.0),
            resume_h: 140.0,
            resume: f(44.0, 18.0),
            row_h: 120.0,
            row: f(32.0, 14.0),
            status: f(19.0, 12.0),
            hint: f(16.0, 12.0),
            help: f(24.0, 12.0),
            track: 0.0,
            numw: 100.0,
        },
    }
}

// The control a card item shows on its row.
fn item_ctl(it: Item, m: &MenuState<'_>, model: &launcher::Model, lang: Lang) -> Ctl {
    match it {
        Item::Hitboxes => Ctl::Choices {
            words: Hitboxes::IDS
                .iter()
                .map(|id| {
                    crate::settings::find("hitboxes")
                        .and_then(|e| i18n::word(lang, e, id))
                        .unwrap_or(id)
                        .to_string()
                })
                .collect(),
            sel: Some(m.hitboxes as usize),
        },
        Item::Volume if has_volume(model) => model.ctl(VOLUME, lang),
        Item::Speed => {
            let now = m.speed.unwrap_or((1, 1));
            let rates = crate::transport::SLOW;
            Ctl::Choices {
                words: rates
                    .iter()
                    .map(|&r| crate::transport::rate_label(lang, r))
                    .collect(),
                sel: rates.iter().position(|&r| r == now),
            }
        }
        _ => Ctl::None,
    }
}

// A click / tap on an item's control.
fn item_input(it: Item, o: CtlOut, m: &MenuState<'_>, model: &mut launcher::Model) -> Do {
    match (it, o) {
        (Item::Volume, o) => {
            model.ctl_input(VOLUME, o);
            Do::Nothing
        }
        (Item::Hitboxes, CtlOut::Pick(i)) => Do::Hitboxes(hitbox_mode(i)),
        (Item::Hitboxes, CtlOut::Step(d)) => Do::Hitboxes(hitbox_mode(
            (m.hitboxes as usize)
                .saturating_add_signed(d as isize)
                .min(2),
        )),
        (Item::Speed, CtlOut::Pick(i)) => crate::transport::SLOW
            .get(i)
            .map_or(Do::Nothing, |&r| Do::Speed(r)),
        (Item::Speed, CtlOut::Step(d)) => {
            let rates = crate::transport::SLOW;
            let at = rates.iter().position(|&x| Some(x) == m.speed).unwrap_or(0);
            Do::Speed(rates[at.saturating_add_signed(d as isize).min(rates.len() - 1)])
        }
        _ => Do::Nothing,
    }
}

// One item row: focus band (or the 4:3 gradient), label, its control, the › on Settings / Touch layout.
#[expect(clippy::too_many_arguments, reason = "one painted row")]
fn item_row(
    ui: &Ui,
    p: &Painter,
    rect: Rect,
    band: Rect,
    it: Item,
    size: f32,
    cm: &CardMetrics,
    m: &MenuState<'_>,
    model: &mut launcher::Model,
    focus: &mut Focus,
    lay: &Layout,
    words: &Words,
    dimmed: bool,
    out: &mut Vec<Do>,
) {
    let id = Id::unique(("card item", it as u8));
    let resp = ui.interact(
        band,
        id,
        if dimmed {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    let focused = !dimmed && focus.ask.is_none() && focus.item == it;
    if !dimmed && resp.contains_pointer() && pointer_moved(ui) && focus.ask.is_none() {
        focus.item = it;
    }
    if focused {
        if lay.shape == Shape::Column {
            theme::focus_over_game(p, band, 4.0);
        } else {
            theme::focus(
                p,
                band,
                if lay.shape == Shape::Split { 5.0 } else { 4.0 },
                resp.is_pointer_button_down_on(),
            );
        }
    }
    let color = if focused {
        theme::TEXT_HI
    } else if it.danger() {
        theme::DANGER
    } else {
        theme::TEXT
    };
    let ctl = item_ctl(it, m, model, words.lang);
    let label_w = if matches!(ctl, Ctl::None) {
        rect.width() - size
    } else {
        rect.width() * 0.42
    };
    let caption = words.latin(it);
    let mut lrect = Rect::from_min_size(rect.min, vec2(label_w, rect.height()));
    if caption.is_some() {
        lrect.max.y -= rect.height() * 0.32;
    }
    label(p, lrect, words.item(it), theme::serif(size), color, focused);
    if let Some(c) = caption {
        let g = p.layout_no_wrap(c.to_string(), theme::serif(cm.cap * 0.85), theme::ACCENT);
        theme::caption_text(p, pos2(rect.min.x + size * 1.2, lrect.max.y - 2.0), g);
    }
    if matches!(it, Item::Settings | Item::TouchLayout) {
        theme::chevron(
            p,
            pos2(rect.max.x, rect.center().y),
            size * 0.85,
            theme::HELP,
        );
    }
    let st = CtlStyle {
        size: size * 0.93,
        num: size * 0.8,
        cells: false,
        right: true,
        indent: 0.0,
        track: cm.track,
        numw: cm.numw,
        gap: size * 0.93,
        cell_w: 0.0,
    };
    let crect = Rect::from_min_max(pos2(rect.min.x + label_w, rect.min.y), rect.max);
    if !dimmed && let Some(o) = theme::value_ctl(ui, p, id, crect, &ctl, focused, &st) {
        focus.item = it;
        out.push(item_input(it, o, m, model));
    } else if resp.clicked() {
        focus.item = it;
        out.push(focus.activate(it, m));
    }
    if focused && focus.moved {
        resp.scroll_to_me(None);
    }
}

// The in-place question for Retry / Quit: question, note, "Keep playing" (focused first) and the action row.
// Returns the height used.
#[expect(clippy::too_many_arguments, reason = "one painted block")]
fn confirm_block(
    ui: &Ui,
    p: &Painter,
    x: f32,
    y: f32,
    w: f32,
    band: (f32, f32),
    ask: Ask,
    cm: &CardMetrics,
    focus: &mut Focus,
    lay: &Layout,
    words: &Words,
    out: &mut Vec<Do>,
) -> f32 {
    let (q, note, act) = match ask {
        Ask::Retry => ("retry_ask", "retry_note", "retry_do"),
        Ask::Quit => ("quit_ask", "quit_note", "quit_do"),
    };
    let mut cy = y;
    let qs = cm.row * 1.14;
    let g = p.layout(
        words.ui(q).to_string(),
        theme::serif(qs),
        theme::QUESTION,
        w,
    );
    let gh = g.size().y;
    theme::outlined(p, pos2(x, cy), g, theme::QUESTION, false);
    cy += gh + qs * 0.3;
    cy += sans_text(p, pos2(x, cy), words.ui(note), cm.help, theme::HELP_HI, w);
    cy += cm.help * 0.9;
    let rh = cm.row_h;
    for (i, (id, yes)) in [("keep_playing", false), (act, true)]
        .into_iter()
        .enumerate()
    {
        let rect = Rect::from_min_size(pos2(x, cy), vec2(w, rh));
        let b = Rect::from_x_y_ranges(band.0..=band.1, rect.y_range());
        let resp = ui.interact(b, Id::unique(("confirm", i)), Sense::click());
        if resp.contains_pointer() && pointer_moved(ui) {
            focus.yes = yes;
        }
        let f = focus.yes == yes;
        if f {
            if lay.shape == Shape::Column {
                theme::focus_over_game(p, b, 4.0);
            } else {
                theme::focus(p, b, 4.0, resp.is_pointer_button_down_on());
            }
        }
        let color = if f {
            theme::TEXT_HI
        } else if yes {
            theme::DANGER
        } else {
            theme::TEXT
        };
        label(p, rect, words.ui(id), theme::serif(cm.row * 1.07), color, f);
        if resp.clicked() {
            focus.yes = yes;
            out.push(focus.confirm());
        }
        cy += rh;
    }
    cy - y
}

fn card_ui(
    ui: &mut Ui,
    m: &MenuState<'_>,
    model: &mut launcher::Model,
    focus: &mut Focus,
    lay: &Layout,
    geo: &Geo,
    words: &Words,
) -> Vec<Do> {
    let items = focus.items(m);
    focus.clamp(&items);
    let cm = card_metrics(lay);
    let mut out = Vec::new();
    let p = ui.painter().clone();
    match lay.shape {
        Shape::Split => split_card(
            ui, &p, &items, &cm, m, model, focus, lay, geo, words, &mut out,
        ),
        _ => list_card(
            ui, &p, &items, &cm, m, model, focus, lay, geo, words, &mut out,
        ),
    }
    out
}

// Pillar and 4:3 column: heading, Resume, rule, the shortcuts, rule, Retry… / Quit…, then status and hints.
#[expect(clippy::too_many_arguments, reason = "one painted card")]
#[expect(clippy::too_many_lines, reason = "one painted screen, top to bottom")]
fn list_card(
    ui: &Ui,
    p: &Painter,
    items: &[Item],
    cm: &CardMetrics,
    m: &MenuState<'_>,
    model: &mut launcher::Model,
    focus: &mut Focus,
    lay: &Layout,
    geo: &Geo,
    words: &Words,
    out: &mut Vec<Do>,
) {
    let pillar = lay.shape == Shape::Pillar;
    let (x, w, band, top) = if pillar {
        let px = geo.frame.max.x;
        theme::lacquer(
            p,
            Rect::from_x_y_ranges(px..=geo.screen.max.x, geo.screen.y_range()),
            Edge::Left,
        );
        let x = px + 44.0;
        (
            x,
            geo.safe.max.x - 44.0 - x,
            (px + 6.0, geo.screen.max.x),
            geo.safe.min.y,
        )
    } else {
        let pf = geo.game(32.0, 16.0, 384.0, 448.0);
        let w = 396.0f32.min(pf.width() - 48.0);
        let x = pf.center().x - 14.0 * geo.gs / 2.0 - w / 2.0;
        let soft = Rect::from_min_max(
            pos2(x - 64.0, pf.min.y + 40.0),
            pos2(
                x + w + 56.0,
                pf.min.y + 40.0 + 590.0f32.min(pf.height() - 40.0),
            ),
        );
        theme::soft_rect(p, soft, 60.0, egui::Color32::from_black_alpha(90));
        (x, w, (x - 24.0, x + w + 16.0), geo.frame.min.y + 80.0)
    };
    let mut y = top + if pillar { 44.0 } else { 32.0 };
    theme::heading(
        p,
        pos2(x, y),
        words.ui("paused_title"),
        &words.heading_caption(m),
        cm.head,
        cm.cap,
    );
    y = top + if pillar { 160.0 } else { 116.0 };
    // The bottom block of the pillar is pinned; rows shrink when the list would reach it.
    let hints_y = geo.safe.max.y - 60.0;
    let notes = words.notes(m);
    let dev_y = hints_y - 58.0;
    let notes_y = dev_y - 26.0 * notes.len() as f32;
    let help_text = match focus.ask {
        Some(_) if focus.yes => words.help(match focus.ask {
            Some(Ask::Retry) => Item::Retry,
            _ => Item::Quit,
        }),
        Some(_) => words.ui("help_keep"),
        None => words.help(focus.item),
    };
    let help_g = p.layout(
        help_text.to_string(),
        theme::sans(cm.help),
        theme::HELP_HI,
        w,
    );
    let rule_y = notes_y - 18.0 - help_g.size().y - 14.0;
    let mids = items
        .iter()
        .filter(|i| !matches!(i, Item::Resume) && !i.danger())
        .count() as f32;
    let dangers = items.iter().filter(|i| i.danger()).count() as f32;
    let gaps = if pillar { 24.0 + 32.0 } else { 16.0 + 20.0 };
    let mut rh = cm.row_h;
    if pillar {
        let room = rule_y - 12.0 - y - cm.resume_h - gaps;
        if (mids + dangers) * rh > room {
            rh = (room / (mids + dangers)).max(48.0);
        }
    }
    let dimmed = focus.ask.is_some();
    let mut op = p.clone();
    if dimmed {
        op.set_opacity(0.3);
    }
    let row = |y: f32, h: f32| Rect::from_min_size(pos2(x, y), vec2(w, h));
    let bandr = |r: Rect| Rect::from_x_y_ranges(band.0..=band.1, r.y_range());
    // Japanese items with the original's Latin caption take a taller row.
    let tall = |it: Item, h: f32| {
        if words.latin(it).is_some() {
            h * 1.33
        } else {
            h
        }
    };
    let r = row(y, tall(Item::Resume, cm.resume_h));
    let resume_h = r.height();
    item_row(
        ui,
        &op,
        r,
        bandr(r),
        Item::Resume,
        cm.resume,
        cm,
        m,
        model,
        focus,
        lay,
        words,
        dimmed,
        out,
    );
    y += resume_h + if pillar { 14.0 } else { 10.0 };
    theme::rule(&op, egui::Rangef::new(x, x + w), y);
    y += if pillar { 10.0 } else { 6.0 };
    for &it in items
        .iter()
        .filter(|i| !matches!(i, Item::Resume) && !i.danger())
    {
        let r = row(y, rh);
        item_row(
            ui,
            &op,
            r,
            bandr(r),
            it,
            cm.row,
            cm,
            m,
            model,
            focus,
            lay,
            words,
            dimmed,
            out,
        );
        y += rh;
    }
    if dangers > 0.0 {
        y += if pillar { 22.0 } else { 14.0 };
        theme::rule(p, egui::Rangef::new(x, x + w), y);
        y += if pillar { 10.0 } else { 6.0 };
        if let Some(ask) = focus.ask {
            y += 6.0;
            let cmr = CardMetrics { row_h: rh, ..*cm };
            y += confirm_block(ui, p, x, y, w, band, ask, &cmr, focus, lay, words, out);
        } else {
            for &it in items.iter().filter(|i| i.danger()) {
                let r = row(y, tall(it, rh));
                item_row(
                    ui,
                    p,
                    r,
                    bandr(r),
                    it,
                    cm.row,
                    cm,
                    m,
                    model,
                    focus,
                    lay,
                    words,
                    false,
                    out,
                );
                y += r.height();
            }
        }
    }
    let span = egui::Rangef::new(x, x + w);
    if pillar {
        theme::rule(p, span, rule_y);
        theme::shadowed(p, pos2(x, rule_y + 14.0), help_g, theme::HELP_HI);
        for (i, n) in notes.iter().enumerate() {
            sans_text(
                p,
                pos2(x, notes_y + 26.0 * i as f32),
                n,
                cm.status,
                theme::HELP,
                w,
            );
        }
        let dev = match m.pad {
            Some(pn) => pn.to_string(),
            None if lay.touch => words.ui("device_touch").into(),
            None => words.ui("device_keyboard").into(),
        };
        sans_text(p, pos2(x, dev_y), &dev, cm.status, theme::HELP, w);
        let v = p.layout_no_wrap(version(words.lang), theme::sans(cm.status), theme::HELP);
        theme::shadowed(p, pos2(x + w - v.size().x, dev_y), v, theme::HELP);
        let which = if focus.ask.is_some() {
            Hints::Confirm
        } else {
            Hints::Card
        };
        draw_hints(
            p,
            pos2(x, hints_y),
            &hint_items(words, m, lay, &which),
            cm.hint,
        );
    } else {
        y += if dangers > 0.0 { 16.0 } else { 14.0 };
        theme::rule(p, span, y);
        y += 10.0;
        for n in &notes {
            y += sans_text(p, pos2(x, y), n, cm.status, theme::HELP_HI, w) + 6.0;
        }
        y += sans_text(
            p,
            pos2(x, y),
            &words.device(m, lay),
            cm.status,
            theme::HELP,
            w,
        ) + 14.0;
        let which = if focus.ask.is_some() {
            Hints::Confirm
        } else {
            Hints::Card
        };
        draw_hints(p, pos2(x, y), &hint_items(words, m, lay, &which), cm.hint);
    }
}

// Phone: the left bar holds the heading, status and the run items low in thumb reach; the right bar the quick
// settings, bottom-anchored.
#[expect(clippy::too_many_arguments, reason = "one painted card")]
#[expect(clippy::too_many_lines, reason = "one painted screen, top to bottom")]
fn split_card(
    ui: &Ui,
    p: &Painter,
    items: &[Item],
    cm: &CardMetrics,
    m: &MenuState<'_>,
    model: &mut launcher::Model,
    focus: &mut Focus,
    lay: &Layout,
    geo: &Geo,
    words: &Words,
    out: &mut Vec<Do>,
) {
    let left = Rect::from_x_y_ranges(geo.screen.min.x..=geo.frame.min.x, geo.screen.y_range());
    let right = Rect::from_x_y_ranges(geo.frame.max.x..=geo.screen.max.x, geo.screen.y_range());
    theme::lacquer(p, left, Edge::Right);
    theme::lacquer(p, right, Edge::Left);
    let x = geo.safe.min.x + 20.0;
    let w = left.max.x - 36.0 - x;
    let top = geo.safe.min.y;
    let bottom = geo.safe.max.y - 40.0;
    let hh = theme::heading(
        p,
        pos2(x, top + 44.0),
        words.ui("paused_title"),
        &words.heading_caption(m),
        cm.head,
        cm.cap,
    );
    let mut y = top + 44.0 + hh + 24.0;
    for n in words.notes(m) {
        y += sans_text(p, pos2(x, y), &n, cm.status, theme::HELP_HI, w) + 4.0;
    }
    sans_text(
        p,
        pos2(x, y),
        &words.device(m, lay),
        cm.status,
        theme::HELP,
        w,
    );
    let dimmed = focus.ask.is_some();
    let mut op = p.clone();
    if dimmed {
        op.set_opacity(0.3);
    }
    let band = (geo.safe.min.x, left.max.x - 6.0);
    let bandr = |r: Rect| Rect::from_x_y_ranges(band.0..=band.1, r.y_range());
    // Bottom-up: Quit, Retry (or the question), rule, Settings, Resume.
    let mut yb = bottom;
    let dangers: Vec<Item> = items.iter().copied().filter(|i| i.danger()).collect();
    if let Some(ask) = focus.ask {
        let need = cm.row * 1.14 * 1.6 + cm.help * 3.4 + 2.0 * cm.row_h;
        yb -= need;
        let cmr = CardMetrics {
            row_h: cm.row_h,
            ..*cm
        };
        confirm_block(ui, p, x, yb, w, band, ask, &cmr, focus, lay, words, out);
        yb -= 24.0;
        theme::rule(p, egui::Rangef::new(x, x + w), yb + 8.0);
    } else if !dangers.is_empty() {
        for &it in dangers.iter().rev() {
            yb -= cm.row_h;
            let r = Rect::from_min_size(pos2(x, yb), vec2(w, cm.row_h));
            item_row(
                ui,
                p,
                r,
                bandr(r),
                it,
                cm.row * 0.94,
                cm,
                m,
                model,
                focus,
                lay,
                words,
                false,
                out,
            );
        }
        yb -= 24.0;
        theme::rule(p, egui::Rangef::new(x, x + w), yb + 8.0);
    }
    yb -= cm.row_h;
    let r = Rect::from_min_size(pos2(x, yb), vec2(w, cm.row_h));
    item_row(
        ui,
        &op,
        r,
        bandr(r),
        Item::Settings,
        cm.row,
        cm,
        m,
        model,
        focus,
        lay,
        words,
        dimmed,
        out,
    );
    yb -= cm.resume_h;
    let r = Rect::from_min_size(pos2(x, yb), vec2(w, cm.resume_h));
    item_row(
        ui,
        &op,
        r,
        bandr(r),
        Item::Resume,
        cm.resume,
        cm,
        m,
        model,
        focus,
        lay,
        words,
        dimmed,
        out,
    );
    // Quick settings.
    let rx = right.min.x + 44.0;
    let rw = geo.safe.max.x - 64.0 - rx;
    let rband = (right.min.x + 6.0, geo.screen.max.x);
    let quick: Vec<Item> = items
        .iter()
        .copied()
        .filter(|i| {
            matches!(
                i,
                Item::Volume | Item::Hitboxes | Item::Speed | Item::Screenshot | Item::TouchLayout
            )
        })
        .collect();
    let lab = cm.row;
    let lab_h = lab * 1.75;
    let height = |it: Item| match it {
        Item::Volume => lab_h + 120.0 + 40.0 + 16.0,
        Item::Hitboxes | Item::Speed => lab_h + 120.0 + 16.0,
        _ => 120.0,
    };
    let total: f32 = quick.iter().map(|&i| height(i)).sum();
    let mut y = bottom - total;
    let cap = p.layout_no_wrap(
        words.ui("quick_settings").to_string(),
        theme::serif(cm.cap * 0.95),
        theme::ACCENT,
    );
    let ch = cap.size().y;
    theme::caption_text(p, pos2(rx, y - ch - 24.0), cap);
    theme::rule(&op, egui::Rangef::new(rx, rx + rw), y - 12.0);
    for it in quick {
        let h = height(it);
        let r = Rect::from_min_size(pos2(rx, y), vec2(rw, h));
        let b = Rect::from_x_y_ranges(rband.0..=rband.1, r.y_range());
        let id = Id::unique(("quick", it as u8));
        let resp = ui.interact(
            b,
            id,
            if dimmed {
                Sense::hover()
            } else {
                Sense::click()
            },
        );
        if !dimmed && resp.contains_pointer() && pointer_moved(ui) {
            focus.item = it;
        }
        let focused = !dimmed && focus.item == it;
        if focused {
            theme::focus(&op, b, 5.0, resp.is_pointer_button_down_on());
        }
        let color = if focused { theme::TEXT_HI } else { theme::TEXT };
        let ctl = item_ctl(it, m, model, words.lang);
        if matches!(ctl, Ctl::None) {
            label(&op, r, words.item(it), theme::serif(lab), color, focused);
            if it == Item::TouchLayout {
                theme::chevron(&op, pos2(r.max.x + 20.0, r.center().y), lab, theme::HELP);
            }
            if resp.clicked() {
                out.push(focus.activate(it, m));
            }
        } else {
            label(
                &op,
                Rect::from_min_size(r.min, vec2(rw, lab_h)),
                words.item(it),
                theme::serif(lab),
                color,
                focused,
            );
            let cells = Rect::from_min_size(pos2(rx, y + lab_h), vec2(rw, 120.0));
            let st = CtlStyle {
                size: lab,
                num: lab,
                cells: true,
                right: true,
                indent: 0.0,
                track: 0.0,
                numw: 0.0,
                gap: lab,
                cell_w: 120.0,
            };
            let res = if let Ctl::Slider { frac, text, unit } = &ctl {
                phone_volume(ui, &op, id, cells, *frac, text, unit, focused, lab)
            } else {
                theme::value_ctl(ui, &op, id, cells, &ctl, focused, &st)
            };
            if let Some(o) = res.filter(|_| !dimmed) {
                focus.item = it;
                out.push(item_input(it, o, m, model));
            } else if resp.clicked() {
                out.push(focus.activate(it, m));
            }
            if it != *items.last().unwrap_or(&it) {
                theme::rule(&op, egui::Rangef::new(rx, rx + rw), y + h - 8.0);
            }
        }
        y += h;
    }
}

// The phone's volume: ◀ number ▶ cells, the track below.
#[expect(clippy::too_many_arguments, reason = "one painted control")]
fn phone_volume(
    ui: &Ui,
    p: &Painter,
    id: Id,
    cells: Rect,
    frac: f32,
    text: &str,
    unit: &str,
    focused: bool,
    size: f32,
) -> Option<CtlOut> {
    let mut out = None;
    for (dir, r) in [
        (-1, Rect::from_min_size(cells.min, vec2(120.0, 120.0))),
        (
            1,
            Rect::from_min_size(pos2(cells.max.x - 120.0, cells.min.y), vec2(120.0, 120.0)),
        ),
    ] {
        let resp = ui.interact(r, id.with(("vstep", dir)), Sense::click());
        let fill = if resp.is_pointer_button_down_on() || focused {
            theme::PRESSED
        } else {
            theme::CELL
        };
        p.rect_filled(r, 0.0, fill);
        theme::arrow(
            p,
            r.center(),
            20.0,
            dir > 0,
            if focused { theme::TEXT_HI } else { theme::TEXT },
        );
        if resp.clicked() {
            out = Some(CtlOut::Step(dir));
        }
    }
    let n = p.layout_no_wrap(text.to_string(), theme::sans(size), theme::TEXT_HI);
    let u = p.layout_no_wrap(unit.to_string(), theme::sans(size * 0.7), theme::HELP);
    let tw = n.size().x
        + if unit.is_empty() {
            0.0
        } else {
            u.size().x + 4.0
        };
    let c = cells.center();
    let ny = c.y - n.size().y / 2.0;
    let nx = c.x - tw / 2.0;
    let nw = n.size().x;
    theme::shadowed(p, pos2(nx, ny), n, theme::TEXT_HI);
    theme::shadowed(p, pos2(nx + nw + 4.0, ny + size * 0.25), u, theme::HELP);
    let ty = cells.max.y + 20.0;
    let span = egui::Rangef::new(cells.min.x, cells.max.x);
    theme::track(p, span, ty, frac, focused);
    let tr = Rect::from_x_y_ranges(span, ty - 16.0..=ty + 16.0);
    let resp = ui.interact(tr, id.with("vtrack"), Sense::click_and_drag());
    if (resp.clicked() || resp.dragged())
        && let Some(pos) = resp.interact_pointer_pos()
    {
        out = Some(CtlOut::Frac(
            ((pos.x - tr.min.x) / tr.width()).clamp(0.0, 1.0),
        ));
    }
    out
}

// Settings page metrics of one shape.
struct PageMetrics {
    head: f32,
    cap: f32,
    tab: f32,
    row_h: f32,
    row: f32,
    help: f32,
    hint: f32,
    label_w: f32,
}

// The settings page's frame: the rows panel, its text column and where the rows start; the back / tab clicks.
struct Head {
    panel: Rect,
    x: f32,
    w: f32,
    list_top: f32,
    back: bool,
    tab: Option<usize>,
}

// Phone: ‹ back and the vertical tab rail in the left bar, the rows panel over the HUD.
#[expect(clippy::too_many_arguments, reason = "one painted block")]
fn rail_head(
    ui: &Ui,
    p: &Painter,
    m: &MenuState<'_>,
    tabs: &[crate::settings::Group],
    focus: &Focus,
    lay: &Layout,
    geo: &Geo,
    words: &Words,
    pm: &PageMetrics,
) -> Head {
    let top = geo.safe.min.y;
    let f = |mock, dp| lay.font(mock, dp);
    let p = p.clone();
    let left = Rect::from_x_y_ranges(geo.screen.min.x..=geo.frame.min.x, geo.screen.y_range());
    theme::lacquer(&p, left, Edge::Right);
    let lx = geo.safe.min.x + 20.0;
    let lw = left.max.x - 36.0 - lx;
    let back = Rect::from_min_size(pos2(lx, top + 24.0), vec2(lw, 120.0));
    let resp = ui.interact(back, Id::unique("settings back"), Sense::click());
    let chev = p.layout_no_wrap("‹".into(), theme::serif(pm.row * 1.14), theme::HELP);
    let cw = chev.size().x;
    theme::shadowed(
        &p,
        pos2(lx, back.center().y - chev.size().y / 2.0),
        chev,
        theme::HELP,
    );
    label(
        &p,
        Rect::from_min_max(pos2(lx + cw + 14.0, back.min.y), back.max),
        words.ui("paused_title"),
        theme::serif(pm.row * 1.14),
        theme::TEXT,
        false,
    );
    let back_clicked = resp.clicked();
    theme::rule(&p, egui::Rangef::new(lx, lx + lw), top + 156.0);
    let dev_y = geo.safe.max.y - 80.0;
    let n = tabs.len().max(1) as f32;
    let th = 120.0f32.min((dev_y - 16.0 - (top + 168.0)) / n);
    let mut clicked = None;
    for (i, &g) in tabs.iter().enumerate() {
        let r = Rect::from_min_size(pos2(lx, top + 168.0 + th * i as f32), vec2(lw, th));
        let sel = i == focus.tab;
        let resp = ui.interact(r, Id::unique(("tab", i)), Sense::click());
        if focus.row == 0 && sel {
            theme::focus(
                &p,
                Rect::from_x_y_ranges(geo.safe.min.x..=left.max.x - 6.0, r.y_range()),
                5.0,
                false,
            );
        }
        let col = if sel { theme::TEXT_HI } else { theme::HELP };
        let g = p.layout(
            i18n::group_title(words.lang, g).to_string(),
            theme::serif(pm.tab),
            col,
            lw,
        );
        let gp = pos2(lx, r.center().y - g.size().y / 2.0);
        let (gw, gh) = (
            g.rows.last().map_or(g.size().x, |row| row.rect().width()),
            g.size().y,
        );
        theme::outlined(&p, gp, g, col, false);
        if sel {
            p.hline(
                egui::Rangef::new(lx, lx + gw),
                gp.y + gh + 8.0,
                egui::Stroke::new(3.0, theme::ACCENT),
            );
        }
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    sans_text(
        &p,
        pos2(lx, dev_y),
        &words.device(m, lay),
        f(19.0, 12.0),
        theme::HELP,
        lw,
    );
    let tab_clicked = clicked;
    let px = geo.frame.min.x + 416.0 * geo.gs;
    let panel = Rect::from_x_y_ranges(px..=geo.screen.max.x, geo.screen.y_range());
    theme::lacquer(&p, panel, Edge::Left);
    let x = px + 48.0;
    let w = geo.safe.max.x - 64.0 - x;
    let list_top = top + 24.0;
    Head {
        panel,
        x,
        w,
        list_top,
        back: back_clicked,
        tab: tab_clicked,
    }
}

// Desktop: heading, "X ‹ Pause", the underline tabs with Q / E at their ends.
#[expect(clippy::too_many_arguments, reason = "one painted block")]
fn wide_head(
    ui: &Ui,
    p: &Painter,
    m: &MenuState<'_>,
    tabs: &[crate::settings::Group],
    focus: &Focus,
    lay: &Layout,
    geo: &Geo,
    words: &Words,
    pm: &PageMetrics,
) -> Head {
    let top = geo.safe.min.y;
    let f = |mock, dp| lay.font(mock, dp);
    let p = p.clone();
    let px = if lay.shape == Shape::Pillar {
        geo.frame.min.x + 421.33 * geo.gs
    } else {
        let pw = 972.0f32.min(geo.safe.width() - 32.0);
        geo.safe.center().x - pw / 2.0
    };
    let panel = Rect::from_x_y_ranges(px..=geo.screen.max.x, geo.screen.y_range());
    theme::lacquer(&p, panel, Edge::Left);
    let x = px + 52.0;
    let w = geo.safe.max.x - 44.0 - x;
    theme::heading(
        &p,
        pos2(x, top + 30.0),
        words.ui("settings_heading"),
        words.ui("settings_caption"),
        pm.head,
        pm.cap,
    );
    // Back target: "X ‹ Pause" at the top right.
    let bt = format!("‹ {}", words.ui("paused_title"));
    let g = p.layout_no_wrap(bt, theme::sans(f(17.0, 11.0)), theme::HELP);
    let keyed = m.pad.is_some() || !lay.touch;
    let k = p.layout_no_wrap(
        if m.pad.is_some() { "B" } else { "X" }.into(),
        theme::sans(f(17.0, 11.0)),
        theme::KEYCAP,
    );
    let gw = g.size().x;
    let kw = if keyed { k.size().x + 10.0 } else { 0.0 };
    let bx = x + w - gw;
    let by = top + 48.0;
    let br = Rect::from_min_size(pos2(bx - kw, by - 8.0), vec2(gw + kw, g.size().y + 16.0));
    let back_clicked = ui
        .interact(br, Id::unique("settings back"), Sense::click())
        .clicked();
    if keyed {
        theme::shadowed(&p, pos2(bx - kw, by), k, theme::KEYCAP);
    }
    theme::shadowed(&p, pos2(bx, by), g, theme::HELP);
    // Tabs with Q / E at the ends (keyboard), LB / RB (pad), shrunk to fit.
    let ty = top + 126.0;
    let labels: Vec<&str> = tabs
        .iter()
        .map(|&g| i18n::group_title(words.lang, g))
        .collect();
    let ends = if m.pad.is_some() {
        ("LB", "RB")
    } else if lay.touch {
        ("", "")
    } else {
        ("Q", "E")
    };
    let mut size = pm.tab;
    let gap = |s: f32| s * 1.4;
    let width = |s: f32| -> f32 {
        let words_w: f32 = labels
            .iter()
            .map(|l| {
                p.layout_no_wrap((*l).to_string(), theme::serif(s), theme::TEXT)
                    .size()
                    .x
            })
            .sum();
        words_w
            + gap(s) * (labels.len() as f32 + 1.0)
            + if ends.0.is_empty() {
                0.0
            } else {
                2.0 * s * 1.3
            }
    };
    while size > 13.0 && width(size) > w {
        size -= 1.0;
    }
    let mut tx = x;
    let hint_size = (size * 0.8).max(11.0);
    if !ends.0.is_empty() {
        let g = p.layout_no_wrap(ends.0.to_string(), theme::sans(hint_size), theme::KEYCAP);
        let gw = g.size().x;
        theme::shadowed(
            &p,
            pos2(tx, ty + (size - hint_size) * 0.75),
            g,
            theme::KEYCAP,
        );
        tx += gw + gap(size) * 0.7;
    }
    let mut clicked = None;
    for (i, l) in labels.iter().enumerate() {
        let sel = i == focus.tab;
        let col = if sel { theme::TEXT_HI } else { theme::HELP };
        let g = p.layout_no_wrap((*l).to_string(), theme::serif(size), col);
        let (gw, gh) = (g.size().x, g.size().y);
        let r = Rect::from_min_size(pos2(tx - 8.0, ty - 10.0), vec2(gw + 16.0, gh + 24.0));
        if focus.row == 0 && sel {
            theme::focus(&p, r, 3.0, false);
        }
        theme::outlined(&p, pos2(tx, ty), g, col, false);
        if sel {
            p.hline(
                egui::Rangef::new(tx, tx + gw),
                ty + gh + 9.0,
                egui::Stroke::new(2.0, theme::ACCENT),
            );
        }
        if ui
            .interact(r, Id::unique(("tab", i)), Sense::click())
            .clicked()
        {
            clicked = Some(i);
        }
        tx += gw + gap(size);
    }
    if !ends.1.is_empty() {
        let g = p.layout_no_wrap(ends.1.to_string(), theme::sans(hint_size), theme::KEYCAP);
        theme::shadowed(
            &p,
            pos2(tx - gap(size) * 0.3, ty + (size - hint_size) * 0.75),
            g,
            theme::KEYCAP,
        );
    }
    let tab_clicked = clicked;
    theme::rule(&p, egui::Rangef::new(x, x + w), ty + 50.0);
    let list_top = ty + 64.0;
    Head {
        panel,
        x,
        w,
        list_top,
        back: back_clicked,
        tab: tab_clicked,
    }
}

// The settings page: back, heading, tabs, the rows of one tab, "Reset this page…", the help line following the
// focus, the key hints.
#[expect(clippy::too_many_lines, reason = "one painted screen, top to bottom")]
fn settings_ui(
    ui: &mut Ui,
    m: &MenuState<'_>,
    model: &mut launcher::Model,
    focus: &mut Focus,
    lay: &Layout,
    geo: &Geo,
    words: &Words,
) {
    let p = ui.painter().clone();
    let tabs = model.live_tabs();
    focus.tab = focus.tab.min(tabs.len().saturating_sub(1));
    let split = lay.shape == Shape::Split;
    let f = |mock, dp| lay.font(mock, dp);
    let ru = words.lang == Lang::Ru;
    let pm = if split {
        PageMetrics {
            head: 0.0,
            cap: 0.0,
            tab: f(if ru { 28.0 } else { 30.0 }, 14.0),
            row_h: 120.0,
            row: f(28.0, 14.0),
            help: f(24.0, 12.0),
            hint: f(16.0, 12.0),
            label_w: 330.0,
        }
    } else {
        PageMetrics {
            head: f(40.0, 18.0),
            cap: f(16.0, 11.0),
            tab: f(if ru { 20.0 } else { 22.0 }, 13.0),
            row_h: 70.0,
            row: f(25.0, 14.0),
            help: f(19.0, 12.0),
            hint: f(16.0, 11.0),
            label_w: 440.0,
        }
    };
    let head = if split {
        rail_head(ui, &p, m, &tabs, focus, lay, geo, words, &pm)
    } else {
        wide_head(ui, &p, m, &tabs, focus, lay, geo, words, &pm)
    };
    let Head {
        panel,
        x,
        w,
        list_top,
        back: back_clicked,
        tab: tab_clicked,
    } = head;
    if back_clicked {
        focus.back();
        return;
    }
    if let Some(i) = tab_clicked {
        focus.tab = i;
        focus.row = focus.row.min(1);
        focus.scroll = 0.0;
        focus.reset = false;
    }
    let Some(&g) = tabs.get(focus.tab) else {
        return;
    };
    let keys = model.live_rows(g);
    focus.row = focus.row.min(keys.len() + 1);
    // Bottom block: help line (and on desktop the hints and device line).
    let help = match focus.row {
        0 => "",
        r if r <= keys.len() => model.row_text(keys[r - 1], words.lang).1,
        _ => words.ui("help_reset_page"),
    };
    let help_g = p.layout(sentence(help), theme::sans(pm.help), theme::HELP_HI, w);
    let help_h = help_g.size().y.max(pm.help * 3.0);
    let bottom = if split {
        geo.safe.max.y - 24.0 - help_h - 30.0
    } else {
        geo.safe.max.y - 80.0 - help_h - 28.0
    };
    // Rows: label column, control column; scrolled to keep the focused row in view.
    let n = keys.len() + 1;
    let rh = pm.row_h;
    let list = Rect::from_x_y_ranges(panel.min.x..=panel.max.x, list_top..=bottom);
    let content = rh * n as f32 + 12.0;
    let max_scroll = (content - list.height()).max(0.0);
    if ui.rect_contains_pointer(list) {
        let d = ui.input(|i| i.smooth_scroll_delta.y);
        focus.scroll -= d;
    }
    if focus.moved && focus.row > 0 {
        let fy = rh * (focus.row - 1) as f32 + if focus.row > keys.len() { 12.0 } else { 0.0 };
        if fy < focus.scroll {
            focus.scroll = fy;
        } else if fy + rh > focus.scroll + list.height() {
            focus.scroll = fy + rh - list.height();
        }
    }
    focus.scroll = focus.scroll.clamp(0.0, max_scroll);
    let lp = p.with_clip_rect(list);
    let moved = pointer_moved(ui);
    let label_w = pm.label_w.min(w * 0.48);
    let st = CtlStyle {
        size: pm.row * 0.96,
        num: if split { pm.row } else { pm.row * 0.8 },
        cells: split,
        right: false,
        indent: if split { 0.0 } else { 32.0 },
        track: if split {
            0.0
        } else {
            (w - label_w - 166.0).clamp(80.0, 270.0)
        },
        numw: 100.0,
        gap: pm.row * 1.2,
        cell_w: if split { 120.0 } else { 0.0 },
    };
    let ui_clip = ui.clip_rect();
    ui.set_clip_rect(list);
    let mut y = list.min.y - focus.scroll;
    for (k, key) in keys.iter().enumerate() {
        let r = Rect::from_min_size(pos2(x, y), vec2(w, rh));
        let b = Rect::from_x_y_ranges(panel.min.x + 6.0..=panel.max.x, r.y_range());
        let id = Id::unique(("row", *key));
        let resp = ui.interact(b.intersect(list), id, Sense::click());
        if resp.contains_pointer() && moved {
            focus.row = k + 1;
            focus.reset = false;
        }
        let focused = focus.row == k + 1;
        if focused {
            theme::focus(
                &lp,
                b,
                if split { 5.0 } else { 4.0 },
                resp.is_pointer_button_down_on(),
            );
        }
        let (lab, _) = model.row_text(key, words.lang);
        let col = if focused { theme::TEXT_HI } else { theme::TEXT };
        label(
            &lp,
            Rect::from_min_size(r.min, vec2(label_w - 16.0, rh)),
            lab,
            theme::serif(pm.row),
            col,
            focused,
        );
        let ctl = model.ctl(key, words.lang);
        let cr = Rect::from_min_max(pos2(x + label_w, r.min.y), r.max);
        if let Some(o) = theme::value_ctl(ui, &lp, id, cr, &ctl, focused, &st) {
            focus.row = k + 1;
            model.ctl_input(key, o);
        } else if resp.clicked() {
            focus.row = k + 1;
            model.activate(key);
        }
        y += rh;
    }
    y += 4.0;
    theme::rule(&lp, egui::Rangef::new(x, x + w), y);
    y += 8.0;
    {
        let r = Rect::from_min_size(pos2(x, y), vec2(w, rh));
        let b = Rect::from_x_y_ranges(panel.min.x + 6.0..=panel.max.x, r.y_range());
        let resp = ui.interact(b.intersect(list), Id::unique("reset page"), Sense::click());
        if resp.contains_pointer() && moved {
            focus.row = keys.len() + 1;
        }
        let focused = focus.row == keys.len() + 1;
        if focused {
            theme::focus(
                &lp,
                b,
                if split { 5.0 } else { 4.0 },
                resp.is_pointer_button_down_on(),
            );
        }
        let text = if focus.reset {
            words.ui("reset_page_ask")
        } else {
            words.ui("reset_page")
        };
        let col = if focus.reset {
            theme::QUESTION
        } else if focused {
            theme::TEXT_HI
        } else {
            theme::DANGER
        };
        label(&lp, r, text, theme::serif(pm.row), col, focused);
        if resp.clicked() {
            focus.row = keys.len() + 1;
            if std::mem::take(&mut focus.reset) {
                model.reset_rows(&keys);
            } else {
                focus.reset = true;
            }
        }
    }
    ui.set_clip_rect(ui_clip);
    let by = bottom + 14.0;
    theme::rule(&p, egui::Rangef::new(x, x + w), by);
    theme::shadowed(&p, pos2(x, by + 14.0), help_g, theme::HELP_HI);
    if !split {
        let hy = geo.safe.max.y - 60.0;
        draw_hints(
            &p,
            pos2(x, hy),
            &hint_items(words, m, lay, &Hints::Settings),
            pm.hint,
        );
        let d = p.layout_no_wrap(words.device(m, lay), theme::sans(pm.hint), theme::HELP);
        let dw = d.size().x;
        theme::shadowed(&p, pos2(x + w - dw, hy + 2.0), d, theme::HELP);
    }
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
    pub stage: Option<(String, String)>,
    // The settings page open at this tab.
    pub settings: Option<usize>,
    // The Retry / Quit question open.
    pub ask: Option<ShotAsk>,
    // Key presses before the shot.
    pub navs: Vec<Nav>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotAsk {
    Retry,
    Quit,
}

// Tools: the safe area to capture the game frame in, so it sits where the menu's pillar leaves it.
#[cfg(not(target_arch = "wasm32"))]
pub fn picture_safe(safe: SafeArea, size: (u32, u32), shot: &Shot, integer: bool) -> SafeArea {
    layout(safe, size, shot.density.max(1.0), shot.touch, integer).picture
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
    let lay = layout(
        safe,
        (w, h),
        shot.density.max(1.0),
        shot.touch,
        s.blit.integer,
    );
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
        stage: shot.stage.as_ref(),
    };
    let mut focus = Focus {
        split: lay.shape == Shape::Split,
        ..Focus::default()
    };
    if let Some(t) = shot.settings {
        focus.activate(Item::Settings, &m);
        focus.tab = t;
    }
    if let Some(a) = shot.ask {
        focus.activate(
            match a {
                ShotAsk::Retry => Item::Retry,
                ShotAsk::Quit => Item::Quit,
            },
            &m,
        );
    }
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
        focus.moved = i == 0;
        let out = ctx.run_pass(raw, |ctx| {
            menu_ui(ctx, &m, &mut model, &mut focus, &lay);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn state(quit: bool, touch: bool, speed: Option<(u32, u32)>) -> MenuState<'static> {
        MenuState {
            pad: None,
            touch,
            cheats: &[],
            hitboxes: Hitboxes::Off,
            status: None,
            speed,
            no_saves: false,
            quit,
            stage: None,
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
        assert_eq!(f.clamp(&f.items(&m)), Item::Resume);
        assert_eq!(run(&mut f, &m, &mut model, &[Nav::Activate]), [Do::Resume]);
        let mut f = Focus::default();
        assert_eq!(run(&mut f, &m, &mut model, &[Nav::Back]), [Do::Resume]);
    }

    // Retry… and Quit… ask in place with "Keep playing" focused: Z there or X cancels back to the item; ↓ Z acts.
    #[test]
    fn run_confirms() {
        let (m, mut model) = (state(true, false, None), model(false));
        for (item, ask, act) in [
            (Item::Retry, Ask::Retry, Do::Retry),
            (Item::Quit, Ask::Quit, Do::Quit),
        ] {
            let mut f = Focus::default();
            assert_eq!(f.activate(item, &m), Do::Nothing);
            assert_eq!((f.ask, f.yes), (Some(ask), false));
            assert_eq!(run(&mut f, &m, &mut model, &[Nav::Activate]), [Do::Nothing]);
            assert_eq!((f.ask, f.item), (None, item));
            f.activate(item, &m);
            assert!(f.back());
            assert_eq!((f.ask, f.item), (None, item));
            f.activate(item, &m);
            assert_eq!(
                run(&mut f, &m, &mut model, &[Nav::Down, Nav::Activate]),
                [Do::Nothing, act]
            );
            assert_eq!(f.ask, None);
        }
        let none = card_items(&state(false, false, None), false);
        assert!(!none.contains(&Item::Quit) && !none.contains(&Item::Retry));
    }

    // The card's focus order on the column / pillar and on the phone; ← / → change values; Settings › / Back.
    #[test]
    fn focus_order_and_settings() {
        let m = state(true, true, Some((1, 1)));
        let mut model = model(true);
        let items = card_items(&m, false);
        assert_eq!(
            items,
            [
                Item::Resume,
                Item::Hitboxes,
                Item::Volume,
                Item::Speed,
                Item::Screenshot,
                Item::TouchLayout,
                Item::Settings,
                Item::Retry,
                Item::Quit,
            ]
        );
        assert_eq!(
            card_items(&m, true),
            [
                Item::Resume,
                Item::Settings,
                Item::Retry,
                Item::Quit,
                Item::Volume,
                Item::Hitboxes,
                Item::Speed,
                Item::Screenshot,
                Item::TouchLayout,
            ]
        );
        let mut f = Focus::default();
        let mut seen = vec![f.clamp(&items)];
        for _ in 0..9 {
            f.nav(Nav::Down, &m, &mut model);
            seen.push(f.item);
        }
        assert_eq!(&seen[..9], &items[..]);
        assert_eq!(seen[9], Item::Quit);
        f.item = Item::Hitboxes;
        assert_eq!(
            run(&mut f, &m, &mut model, &[Nav::Right]),
            [Do::Hitboxes(Hitboxes::Player)]
        );
        assert_eq!(run(&mut f, &m, &mut model, &[Nav::Left]), [Do::Nothing]);
        assert_eq!(
            run(&mut f, &m, &mut model, &[Nav::Activate]),
            [Do::Hitboxes(Hitboxes::Player)]
        );
        f.item = Item::Speed;
        assert_eq!(
            run(&mut f, &m, &mut model, &[Nav::Right]),
            [Do::Speed((3, 4))]
        );
        let v = model.value_text(VOLUME);
        f.item = Item::Volume;
        run(&mut f, &m, &mut model, &[Nav::Left]);
        assert_ne!(model.value_text(VOLUME), v);
        f.item = Item::TouchLayout;
        assert_eq!(
            run(&mut f, &m, &mut model, &[Nav::Activate]),
            [Do::EditLayout]
        );
        f.item = Item::Settings;
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
        // "Reset this page" asks once, then resets the tab's rows.
        let mut r = Focus {
            page: Page::Settings,
            row: keys.len() + 1,
            ..Focus::default()
        };
        run(&mut r, &m, &mut model, &[Nav::Activate]);
        assert!(r.reset);
        run(&mut r, &m, &mut model, &[Nav::Activate]);
        assert!(!r.reset);
        assert_eq!(model.value_text(VOLUME).as_deref(), Some("100"));
        assert_eq!(model.value_text("mute_unfocused"), was);
        // Back returns to the card with Settings focused; X there closes the menu.
        assert_eq!(run(&mut f, &m, &mut model, &[Nav::Back]), [Do::Nothing]);
        assert_eq!((f.page, f.item), (Page::Card, Item::Settings));
        assert_eq!(run(&mut f, &m, &mut model, &[Nav::Back]), [Do::Resume]);
    }

    // The value controls a row paints: a slider with its number and unit, Off / On words, the choice words.
    #[test]
    fn row_controls() {
        let model = model(false);
        let Ctl::Slider { text, unit, frac } = model.ctl(VOLUME, Lang::En) else {
            panic!("volume is a slider");
        };
        assert_eq!((text.as_str(), unit.as_str(), frac), ("100", "%", 1.0));
        assert_eq!(
            model.ctl("mute_unfocused", Lang::En),
            Ctl::Choices {
                words: vec!["Off".into(), "On".into()],
                sel: Some(1)
            }
        );
        let Ctl::Choices { words, sel } = model.ctl("scale_filter", Lang::Ja) else {
            panic!("scale_filter is a choice");
        };
        assert_eq!(words.len(), 3);
        assert!(sel.is_some());
    }

    // Phones split the card into the side bars with 48 dp targets; 16:9 desktops dock the pillar and move the
    // picture left; 4:3 windows put the column over the playfield.
    #[test]
    fn shapes_and_scale() {
        let phone_safe = SafeArea {
            left: 112,
            ..SafeArea::default()
        };
        let phone = layout(phone_safe, (2400, 1080), 2.625, true, false);
        assert_eq!(phone.shape, Shape::Split);
        assert!(120.0 * phone.ppp >= 48.0 * 2.625 - 0.01);
        assert!(phone.font(24.0, 12.0) * phone.ppp >= 12.0 * 2.625 - 0.01);
        assert_eq!(phone.picture, phone_safe);
        let portrait = layout(SafeArea::default(), (1080, 2400), 2.625, true, false);
        assert_eq!(portrait.shape, Shape::Column);
        assert!(120.0 * portrait.ppp >= 48.0 * 2.625 - 0.01);
        let hd = layout(SafeArea::default(), (1920, 1080), 1.0, false, false);
        assert_eq!(hd.shape, Shape::Pillar);
        assert!((hd.ppp - 1.0).abs() < 1e-4);
        assert_eq!(hd.picture.right, 480);
        assert_eq!(hd.frame, [0.0, 0.0, 1440.0, 1080.0]);
        let qhd = layout(SafeArea::default(), (2560, 1440), 1.0, false, false);
        assert_eq!(qhd.shape, Shape::Pillar);
        assert_eq!(qhd.picture.right, 640);
        for size in [(1280, 960), (800, 600), (640, 480)] {
            let l = layout(SafeArea::default(), size, 1.0, false, false);
            assert_eq!(l.shape, Shape::Column, "{size:?}");
            assert_eq!(l.picture, SafeArea::default());
        }
        assert!(
            (layout(SafeArea::default(), (1280, 960), 1.0, false, false).ppp - 1.0).abs() < 1e-4
        );
    }
}
