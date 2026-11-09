//! The port menu's state and input rules for the pause card and the settings page; views only send `Input` and read.

use crate::settings::{Group, Hitboxes};
use crate::settings_store::{CtlOut, SettingsStore};

pub const VOLUME: &str = "volume_master";

// Coasting after a release, points per second.
const FLING_MIN: f32 = 200.0;
const FLING_MAX: f32 = 3000.0;

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

// The last kind of input: the focus bar shows only after a key or pad press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modality {
    Pointer,
    Keys,
    Touch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
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
    pub fn danger(self) -> bool {
        matches!(self, Self::Retry | Self::Quit)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Card,
    Settings,
}

// A run action waiting for its in-place confirm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    Retry,
    Quit,
}

// The settings page's focus: the tab bar or a list row; rows count from 0 and "Reset this page" comes after the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Tabs,
    Row(usize),
}

// A row the pointer can be over: a card item, a settings row, the question's Keep playing (false) / action (true).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Item(Item),
    Row(usize),
    Yes(bool),
}

// What a tap lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Item(Item),
    Row(usize),
    Answer(bool),
    Tab(usize),
    Back,
}

// Top of each list row plus the end of the last one, and the view's height, in points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListGeometry {
    pub tops: Vec<f32>,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Nav(Nav),
    Tap(Target),
    Control(Target, CtlOut),
    // Pointer input was seen; `touch`: a finger, or a mouse on a touch screen.
    Pointer { touch: bool },
    Over(Hit),
    // End of a view pass: what was `Over` is now the hovered row.
    HoverEnd,
    Geometry(ListGeometry),
    Wheel { dy: f32 },
    // A finger went down on the list.
    Press,
    Drag { dy: f32, dt: f32 },
    Release,
    Frame { dt: f32 },
    // Keeps the key-focused row in view and the scroll in range.
    Settle,
    // Where the toolkit's own list scrolled to.
    Scrolled(f32),
    ScrollTo(f32),
}

// What the app or a view must do after an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Resume,
    Quit,
    Retry,
    Screenshot,
    Speed((u32, u32)),
    Hitboxes(Hitboxes),
    EditLayout,
    SettingsChanged,
    // The model moved the list; a view with its own scroll position follows.
    ScrollMoved,
    // The hovered row changed: the help line needs another pass.
    HoverChanged,
}

// The game state the card depends on, set by the app before each pass.
#[derive(Clone, Copy, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts about the game"
)]
pub struct Context {
    // The touch overlay is on.
    pub overlay: bool,
    // A touch screen is the main input.
    pub touch_screen: bool,
    pub hitboxes: Hitboxes,
    pub speed: Option<(u32, u32)>,
    pub rates: &'static [(u32, u32)],
    pub quit: bool,
    pub split: bool,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            overlay: false,
            touch_screen: false,
            hitboxes: Hitboxes::Off,
            speed: None,
            rates: &[],
            quit: false,
            split: false,
        }
    }
}

#[expect(clippy::struct_excessive_bools, reason = "independent menu flags")]
pub struct MenuModel {
    pub store: SettingsStore,
    ctx: Context,
    page: Page,
    item: Item,
    ask: Option<Ask>,
    // The question's action row is focused (else "Keep playing").
    yes: bool,
    tab: usize,
    cursor: Cursor,
    armed: bool,
    moved: bool,
    scroll: f32,
    scroll_moved: bool,
    dragging: bool,
    drag_speed: f32,
    fling: f32,
    list: ListGeometry,
    modality: Modality,
    hover: Option<Hit>,
    next: Option<Hit>,
}

fn hitbox_mode(i: usize) -> Hitboxes {
    match i {
        0 => Hitboxes::Off,
        1 => Hitboxes::Player,
        _ => Hitboxes::All,
    }
}

impl MenuModel {
    pub fn new(store: SettingsStore, modality: Modality) -> Self {
        Self {
            store,
            ctx: Context::default(),
            page: Page::Card,
            item: Item::Resume,
            ask: None,
            yes: false,
            tab: 0,
            cursor: Cursor::Row(0),
            armed: false,
            moved: false,
            scroll: 0.0,
            scroll_moved: false,
            dragging: false,
            drag_speed: 0.0,
            fling: 0.0,
            list: ListGeometry::default(),
            modality,
            hover: None,
            next: None,
        }
    }

    pub fn set_context(&mut self, ctx: Context) {
        self.ctx = ctx;
        if !self.items().contains(&self.item) {
            self.item = Item::Resume;
        }
    }

    pub fn context(&self) -> &Context {
        &self.ctx
    }

    pub fn page(&self) -> Page {
        self.page
    }

    pub fn item(&self) -> Item {
        self.item
    }

    pub fn ask(&self) -> Option<Ask> {
        self.ask
    }

    pub fn yes(&self) -> bool {
        self.yes
    }

    pub fn tab(&self) -> usize {
        self.tab
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn armed(&self) -> bool {
        self.armed
    }

    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    pub fn modality(&self) -> Modality {
        self.modality
    }

    // A key moved the focus in this view pass.
    pub fn moved(&self) -> bool {
        self.moved
    }

    pub fn set_moved(&mut self, moved: bool) {
        self.moved = moved;
    }

    pub fn animating(&self) -> bool {
        self.fling != 0.0
    }

    pub fn bar(&self) -> bool {
        self.modality == Modality::Keys
    }

    // The transient pointer highlight of `t` in this pass.
    pub fn lit(&self, t: Hit) -> bool {
        self.modality == Modality::Pointer && self.next == Some(t)
    }

    // The row the help line describes under pointer input.
    pub fn hovered(&self) -> Option<Hit> {
        match self.modality {
            Modality::Pointer => self.hover,
            _ => None,
        }
    }

    // The settings row the help line describes.
    pub fn help_row(&self) -> Option<usize> {
        if self.bar() {
            return match self.cursor {
                Cursor::Tabs => None,
                Cursor::Row(r) => Some(r),
            };
        }
        if self.ctx.touch_screen {
            return Some(match self.cursor {
                Cursor::Tabs => 0,
                Cursor::Row(r) => r,
            });
        }
        match self.hovered() {
            Some(Hit::Row(r)) => Some(r),
            _ => None,
        }
    }

    // The card's targets in focus order: the column / pillar top to bottom; the phone's left bar, then its quick settings.
    pub fn items(&self) -> Vec<Item> {
        let c = &self.ctx;
        let quick = {
            let mut v = vec![Item::Volume, Item::Hitboxes];
            if c.speed.is_some() {
                v.push(Item::Speed);
            }
            v.push(Item::Screenshot);
            if c.overlay {
                v.push(Item::TouchLayout);
            }
            v
        };
        let run: &[Item] = if c.quit {
            &[Item::Retry, Item::Quit]
        } else {
            &[]
        };
        if c.split {
            [vec![Item::Resume, Item::Settings], run.to_vec(), quick].concat()
        } else {
            let mut mid = vec![Item::Hitboxes, Item::Volume];
            if c.speed.is_some() {
                mid.push(Item::Speed);
            }
            mid.push(Item::Screenshot);
            if c.overlay {
                mid.push(Item::TouchLayout);
            }
            mid.push(Item::Settings);
            [vec![Item::Resume], mid, run.to_vec()].concat()
        }
    }

    pub fn has_volume(&self) -> bool {
        self.store.live_keys().contains(&VOLUME)
    }

    pub fn tabs(&self) -> Vec<Group> {
        self.store.live_tabs()
    }

    // The keys of the tab shown, in row order.
    pub fn keys(&self) -> Vec<&'static str> {
        self.tabs()
            .get(self.tab)
            .map_or_else(Vec::new, |&g| self.store.live_rows(g))
    }

    pub fn live_values(&self) -> Vec<(&'static str, String)> {
        self.store
            .live_keys()
            .into_iter()
            .filter_map(|k| Some((k, self.store.value_text(k)?)))
            .collect()
    }

    pub fn apply(&mut self, input: Input) -> Vec<Effect> {
        let edits = matches!(input, Input::Nav(_) | Input::Tap(_) | Input::Control(..));
        let before = edits.then(|| self.live_values());
        let mut out = Vec::new();
        self.clamp_page();
        match input {
            Input::Nav(n) => out.extend(self.nav(n)),
            Input::Tap(t) => out.extend(self.tap(t)),
            Input::Control(t, o) => out.extend(self.control(t, o)),
            Input::Pointer { touch } => {
                self.modality = if touch {
                    Modality::Touch
                } else {
                    Modality::Pointer
                };
            }
            Input::Over(h) => {
                if self.modality != Modality::Touch {
                    self.next = Some(h);
                }
            }
            Input::HoverEnd => {
                if self.hover != self.next {
                    out.push(Effect::HoverChanged);
                }
                self.hover = self.next.take();
            }
            Input::Geometry(g) => self.list = g,
            Input::Wheel { dy } => self.scroll -= dy,
            Input::Press => self.fling = 0.0,
            Input::Drag { dy, dt } => {
                self.fling = 0.0;
                self.dragging = true;
                self.scroll -= dy;
                self.drag_speed = self.drag_speed * 0.7 + (-dy / dt) * 0.3;
            }
            Input::Release => {
                if std::mem::take(&mut self.dragging) {
                    if self.drag_speed.abs() > FLING_MIN {
                        self.fling = self.drag_speed.clamp(-FLING_MAX, FLING_MAX);
                    } else {
                        self.scroll = self.nearest(self.scroll);
                    }
                    self.drag_speed = 0.0;
                }
            }
            Input::Frame { dt } => {
                if self.fling != 0.0 {
                    self.scroll += self.fling * dt;
                    self.fling *= 1.0 - (6.0 * dt).min(0.9);
                    if self.fling.abs() < FLING_MIN {
                        self.fling = 0.0;
                        self.scroll = self.nearest(self.scroll);
                    }
                }
            }
            Input::Settle => self.settle(),
            Input::Scrolled(v) => self.scroll = v,
            Input::ScrollTo(v) => {
                self.scroll = v;
                self.fling = 0.0;
            }
        }
        if std::mem::take(&mut self.scroll_moved) {
            out.push(Effect::ScrollMoved);
        }
        if before.is_some_and(|b| b != self.live_values()) {
            out.push(Effect::SettingsChanged);
        }
        out
    }

    // Esc / Android Back: closes the settings page or the question; false on the plain card.
    pub fn back(&mut self) -> bool {
        if self.page == Page::Settings {
            self.page = Page::Card;
            self.item = Item::Settings;
            self.armed = false;
            return true;
        }
        if self.ask.is_some() {
            self.cancel();
            return true;
        }
        false
    }

    // A menu opened straight on a page or a question (tools).
    pub fn open_settings(&mut self, tab: usize) {
        self.activate(Item::Settings);
        self.tab = tab;
    }

    pub fn open_ask(&mut self, ask: Ask) {
        self.ask = Some(ask);
        self.yes = false;
    }

    fn nearest(&self, v: f32) -> f32 {
        let n = self.list.tops.len().saturating_sub(1);
        self.list.tops[..n]
            .iter()
            .copied()
            .min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()))
            .unwrap_or(0.0)
    }

    // The largest scroll: the first row whose top makes the rest fit.
    fn max_scroll(&self) -> f32 {
        let tops = &self.list.tops;
        let Some(&content) = tops.last() else {
            return 0.0;
        };
        let n = tops.len() - 1;
        if n == 0 {
            return 0.0;
        }
        tops[..n]
            .iter()
            .copied()
            .find(|&t| content - t <= self.list.height)
            .unwrap_or(tops[n - 1])
    }

    fn settle(&mut self) {
        if let (true, Cursor::Row(r)) = (self.moved, self.cursor) {
            let tops = &self.list.tops;
            if r + 1 < tops.len() {
                let (ft, fb) = (tops[r], tops[r + 1]);
                let n = tops.len() - 1;
                if ft < self.scroll {
                    self.scroll = ft;
                } else if fb > self.scroll + self.list.height {
                    self.scroll = tops[..n]
                        .iter()
                        .copied()
                        .find(|&t| fb - t <= self.list.height)
                        .unwrap_or(ft);
                }
            }
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    fn clamp_page(&mut self) {
        if self.page != Page::Settings {
            return;
        }
        self.tab = self.tab.min(self.tabs().len().saturating_sub(1));
        if let Cursor::Row(r) = self.cursor {
            self.cursor = Cursor::Row(r.min(self.keys().len()));
        }
    }

    fn set_scroll(&mut self, v: f32) {
        self.scroll = v;
        self.fling = 0.0;
        self.scroll_moved = true;
    }

    fn nav(&mut self, n: Nav) -> Option<Effect> {
        self.moved = true;
        // The first key / pad press after pointer or touch input shows the bar (on the hovered row) and moves nothing.
        if self.modality != Modality::Keys {
            self.modality = Modality::Keys;
            match self.hover {
                Some(Hit::Item(it)) if self.page == Page::Card && self.ask.is_none() => {
                    self.item = it;
                }
                Some(Hit::Row(r)) if self.page == Page::Settings => self.cursor = Cursor::Row(r),
                Some(Hit::Yes(y)) if self.ask.is_some() => self.yes = y,
                _ => {}
            }
            if matches!(n, Nav::Up | Nav::Down | Nav::Left | Nav::Right) {
                return None;
            }
        }
        match self.page {
            Page::Card => self.card_nav(n),
            Page::Settings => {
                self.settings_nav(n);
                None
            }
        }
    }

    fn card_nav(&mut self, n: Nav) -> Option<Effect> {
        if self.ask.is_some() {
            match n {
                Nav::Up | Nav::Down => self.yes = !self.yes,
                Nav::Activate => return self.confirm(),
                Nav::Back => self.cancel(),
                _ => {}
            }
            return None;
        }
        let items = self.items();
        let at = items.iter().position(|&x| x == self.item).unwrap_or(0);
        match n {
            Nav::Up => self.item = items[at.saturating_sub(1)],
            Nav::Down => self.item = items[(at + 1).min(items.len() - 1)],
            Nav::Left | Nav::Right => {
                return self.step(self.item, if n == Nav::Left { -1 } else { 1 });
            }
            Nav::Activate => return self.activate(self.item),
            Nav::Back => return Some(Effect::Resume),
            Nav::PrevTab | Nav::NextTab => {}
        }
        None
    }

    // ← / → on a value item.
    fn step(&mut self, it: Item, dir: i32) -> Option<Effect> {
        match it {
            Item::Volume if self.has_volume() => {
                self.store.step(VOLUME, dir);
            }
            Item::Hitboxes => {
                let at = self.ctx.hitboxes as usize;
                let to = at.saturating_add_signed(dir as isize).min(2);
                if to != at {
                    return Some(Effect::Hitboxes(hitbox_mode(to)));
                }
            }
            Item::Speed => {
                if let Some(now) = self.ctx.speed {
                    let rates = self.ctx.rates;
                    let at = rates.iter().position(|&x| x == now).unwrap_or(0);
                    let to = at.saturating_add_signed(dir as isize).min(rates.len() - 1);
                    if to != at {
                        return Some(Effect::Speed(rates[to]));
                    }
                }
            }
            _ => {}
        }
        None
    }

    // Z on the question's focused row: the action, or "Keep playing" back to the item.
    fn confirm(&mut self) -> Option<Effect> {
        match self.ask {
            Some(a) if self.yes => {
                self.ask = None;
                self.yes = false;
                Some(match a {
                    Ask::Retry => Effect::Retry,
                    Ask::Quit => Effect::Quit,
                })
            }
            _ => {
                self.cancel();
                None
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

    fn go_tab(&mut self, tab: usize) {
        self.tab = tab;
        if let Cursor::Row(_) = self.cursor {
            self.cursor = Cursor::Row(0);
        }
        self.armed = false;
        self.set_scroll(0.0);
    }

    fn settings_nav(&mut self, n: Nav) {
        let tabs = self.tabs();
        if tabs.is_empty() {
            if n == Nav::Back {
                self.back();
            }
            return;
        }
        let keys = self.keys();
        let reset_row = keys.len();
        let last = tabs.len() - 1;
        if n != Nav::Activate {
            self.armed = false;
        }
        match (n, self.cursor) {
            (Nav::Up, Cursor::Row(0)) => {
                self.cursor = Cursor::Tabs;
                self.set_scroll(0.0);
            }
            (Nav::Up, Cursor::Row(r)) => self.cursor = Cursor::Row(r - 1),
            (Nav::Down, Cursor::Tabs) => self.cursor = Cursor::Row(0),
            (Nav::Down, Cursor::Row(r)) => self.cursor = Cursor::Row((r + 1).min(reset_row)),
            (Nav::Left, Cursor::Tabs) if self.tab > 0 => self.go_tab(self.tab - 1),
            (Nav::Right, Cursor::Tabs) if self.tab < last => self.go_tab(self.tab + 1),
            (Nav::Left | Nav::Right, Cursor::Row(r)) if r < reset_row => {
                self.store
                    .step(keys[r], if n == Nav::Left { -1 } else { 1 });
            }
            (Nav::PrevTab, _) => self.go_tab(self.tab.checked_sub(1).unwrap_or(last)),
            (Nav::NextTab, _) => self.go_tab(if self.tab == last { 0 } else { self.tab + 1 }),
            (Nav::Activate, Cursor::Row(r)) => self.press_row(r),
            (Nav::Back, _) => {
                self.back();
            }
            _ => {}
        }
    }

    // A press of a settings row: the reset row asks once and then resets; any other row cancels the question.
    fn press_row(&mut self, r: usize) {
        let keys = self.keys();
        self.cursor = Cursor::Row(r);
        if r == keys.len() {
            if std::mem::take(&mut self.armed) {
                self.store.reset_rows(&keys);
            } else {
                self.armed = true;
            }
        } else if let Some(k) = keys.get(r) {
            self.armed = false;
            self.store.activate(k);
        }
    }

    fn tap(&mut self, t: Target) -> Option<Effect> {
        match t {
            Target::Item(it) if self.ask.is_none() => return self.activate(it),
            Target::Answer(y) if self.ask.is_some() => {
                self.yes = y;
                return self.confirm();
            }
            Target::Row(r) if self.page == Page::Settings => self.press_row(r),
            Target::Tab(i) if self.page == Page::Settings => self.go_tab(i),
            Target::Back => {
                self.back();
            }
            _ => {}
        }
        None
    }

    fn control(&mut self, t: Target, o: CtlOut) -> Option<Effect> {
        match t {
            Target::Row(r) if self.page == Page::Settings => {
                self.cursor = Cursor::Row(r);
                self.armed = false;
                if let Some(k) = self.keys().get(r) {
                    self.store.ctl_input(k, o);
                }
                None
            }
            Target::Item(it) if self.ask.is_none() => {
                self.item = it;
                self.item_input(it, o)
            }
            _ => None,
        }
    }

    fn item_input(&mut self, it: Item, o: CtlOut) -> Option<Effect> {
        let rates = self.ctx.rates;
        match (it, o) {
            (Item::Volume, o) => {
                self.store.ctl_input(VOLUME, o);
                None
            }
            (Item::Hitboxes, CtlOut::Pick(i)) => Some(Effect::Hitboxes(hitbox_mode(i))),
            (Item::Hitboxes, CtlOut::Step(d)) => Some(Effect::Hitboxes(hitbox_mode(
                (self.ctx.hitboxes as usize)
                    .saturating_add_signed(d as isize)
                    .min(2),
            ))),
            (Item::Speed, CtlOut::Pick(i)) => rates.get(i).map(|&r| Effect::Speed(r)),
            (Item::Speed, CtlOut::Step(d)) => {
                let at = rates
                    .iter()
                    .position(|&x| Some(x) == self.ctx.speed)
                    .unwrap_or(0);
                Some(Effect::Speed(
                    rates[at.saturating_add_signed(d as isize).min(rates.len() - 1)],
                ))
            }
            _ => None,
        }
    }

    // A press or click of a card item.
    fn activate(&mut self, it: Item) -> Option<Effect> {
        self.item = it;
        match it {
            Item::Resume => Some(Effect::Resume),
            Item::Retry => {
                self.open_ask(Ask::Retry);
                None
            }
            Item::Quit => {
                self.open_ask(Ask::Quit);
                None
            }
            Item::Volume | Item::Speed => None,
            Item::Hitboxes => Some(Effect::Hitboxes(self.ctx.hitboxes.next())),
            Item::Screenshot => Some(Effect::Screenshot),
            Item::TouchLayout => Some(Effect::EditLayout),
            Item::Settings => {
                self.page = Page::Settings;
                self.tab = 0;
                self.cursor = Cursor::Row(0);
                self.armed = false;
                self.set_scroll(0.0);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    const RATES: [(u32, u32); 3] = [(1, 1), (3, 4), (1, 2)];

    fn menu_with(ctx: Context, modality: Modality) -> MenuModel {
        let store = SettingsStore::port_menu(&Settings::default(), ctx.overlay);
        let mut m = MenuModel::new(store, modality);
        m.set_context(ctx);
        m
    }

    fn full() -> Context {
        Context {
            overlay: true,
            touch_screen: true,
            speed: Some((1, 1)),
            rates: &RATES,
            quit: true,
            ..Context::default()
        }
    }

    fn menu() -> MenuModel {
        menu_with(full(), Modality::Keys)
    }

    fn settings() -> MenuModel {
        let mut m = menu();
        m.apply(Input::Tap(Target::Item(Item::Settings)));
        m
    }

    fn nav(m: &mut MenuModel, n: Nav) -> Vec<Effect> {
        m.apply(Input::Nav(n))
    }

    fn volume(m: &MenuModel) -> Option<String> {
        m.store.value_text(VOLUME)
    }

    fn list() -> ListGeometry {
        ListGeometry {
            tops: vec![0.0, 100.0, 200.0, 300.0, 400.0],
            height: 200.0,
        }
    }

    #[test]
    fn up_down_move_the_row_and_up_from_the_first_row_is_the_tab_bar() {
        let mut m = settings();
        assert_eq!(m.cursor(), Cursor::Row(0));
        nav(&mut m, Nav::Down);
        assert_eq!(m.cursor(), Cursor::Row(1));
        nav(&mut m, Nav::Up);
        nav(&mut m, Nav::Up);
        assert_eq!(m.cursor(), Cursor::Tabs);
        nav(&mut m, Nav::Up);
        assert_eq!(m.cursor(), Cursor::Tabs);
        nav(&mut m, Nav::Down);
        assert_eq!(m.cursor(), Cursor::Row(0));
        let last = m.keys().len();
        for _ in 0..last + 4 {
            nav(&mut m, Nav::Down);
        }
        assert_eq!(m.cursor(), Cursor::Row(last));
    }

    #[test]
    fn left_right_change_the_value_and_on_the_tab_bar_the_tab() {
        let mut m = settings();
        let v = volume(&m);
        assert_eq!(nav(&mut m, Nav::Left), [Effect::SettingsChanged]);
        assert_ne!(volume(&m), v);
        nav(&mut m, Nav::Up);
        nav(&mut m, Nav::Left);
        assert_eq!(m.tab(), 0);
        nav(&mut m, Nav::Right);
        assert_eq!((m.tab(), m.cursor()), (1, Cursor::Tabs));
        nav(&mut m, Nav::Left);
        assert_eq!(m.tab(), 0);
    }

    #[test]
    fn activate_runs_a_row_and_reset_asks_first() {
        let mut m = settings();
        let keys = m.keys();
        let b = keys.iter().position(|&k| k == "mute_unfocused").unwrap();
        let was = m.store.value_text("mute_unfocused");
        for _ in 0..b {
            nav(&mut m, Nav::Down);
        }
        assert_eq!(nav(&mut m, Nav::Activate), [Effect::SettingsChanged]);
        assert_ne!(m.store.value_text("mute_unfocused"), was);
        for _ in 0..keys.len() {
            nav(&mut m, Nav::Down);
        }
        assert_eq!(m.cursor(), Cursor::Row(keys.len()));
        assert!(nav(&mut m, Nav::Activate).is_empty());
        assert!(m.armed());
        nav(&mut m, Nav::Up);
        assert!(!m.armed(), "moving away cancels");
        nav(&mut m, Nav::Down);
        nav(&mut m, Nav::Activate);
        assert_eq!(nav(&mut m, Nav::Activate), [Effect::SettingsChanged]);
        assert!(!m.armed());
        assert_eq!(m.store.value_text("mute_unfocused"), was);
    }

    #[test]
    fn back_leaves_the_page_then_closes_the_menu() {
        let mut m = settings();
        nav(&mut m, Nav::Back);
        assert_eq!((m.page(), m.item()), (Page::Card, Item::Settings));
        assert_eq!(nav(&mut m, Nav::Back), [Effect::Resume]);
    }

    #[test]
    fn tab_keys_wrap_and_reset_the_list() {
        let mut m = settings();
        let last = m.tabs().len() - 1;
        m.apply(Input::Geometry(list()));
        m.apply(Input::ScrollTo(100.0));
        assert_eq!(
            nav(&mut m, Nav::PrevTab),
            [Effect::ScrollMoved],
            "the view's list follows"
        );
        assert_eq!((m.tab(), m.scroll()), (last, 0.0));
        nav(&mut m, Nav::NextTab);
        assert_eq!(m.tab(), 0);
    }

    #[test]
    fn wheel_scrolls_within_the_list_and_changes_nothing() {
        let mut m = settings();
        let values = m.live_values();
        m.apply(Input::Geometry(list()));
        m.apply(Input::Wheel { dy: -150.0 });
        m.apply(Input::Settle);
        assert_eq!(m.scroll(), 150.0);
        m.apply(Input::Wheel { dy: -900.0 });
        m.apply(Input::Settle);
        assert_eq!(m.scroll(), 200.0, "stops at the last whole page");
        assert_eq!(m.live_values(), values);
    }

    #[test]
    fn tap_activates_what_is_under_the_finger() {
        let mut m = menu_with(full(), Modality::Touch);
        assert_eq!(
            m.apply(Input::Tap(Target::Item(Item::Resume))),
            [Effect::Resume]
        );
        m.apply(Input::Tap(Target::Item(Item::Quit)));
        assert_eq!(m.ask(), Some(Ask::Quit));
        assert_eq!(m.apply(Input::Tap(Target::Answer(true))), [Effect::Quit]);
        m.apply(Input::Tap(Target::Item(Item::Settings)));
        m.apply(Input::Tap(Target::Tab(1)));
        assert_eq!(m.tab(), 1);
        m.apply(Input::Tap(Target::Back));
        assert_eq!(m.page(), Page::Card);
    }

    #[test]
    fn a_tap_elsewhere_cancels_the_armed_reset() {
        let mut m = settings();
        let reset = m.keys().len();
        m.apply(Input::Tap(Target::Row(reset)));
        assert!(m.armed());
        m.apply(Input::Control(Target::Row(0), CtlOut::Step(-1)));
        assert!(!m.armed());
    }

    #[test]
    fn drag_scrolls_coasts_and_rests_on_a_row_without_changing_a_value() {
        let mut m = settings();
        let values = m.live_values();
        m.apply(Input::Geometry(list()));
        m.apply(Input::Press);
        m.apply(Input::Drag { dy: -30.0, dt: 0.1 });
        assert_eq!(m.scroll(), 30.0);
        m.apply(Input::Release);
        assert_eq!(m.scroll(), 0.0, "slow: rests on the nearest row");
        assert!(!m.animating());
        for _ in 0..8 {
            m.apply(Input::Drag {
                dy: -40.0,
                dt: 1.0 / 60.0,
            });
        }
        m.apply(Input::Release);
        assert!(m.animating(), "fast: coasts");
        let from = m.scroll();
        m.apply(Input::Frame { dt: 1.0 / 60.0 });
        assert!(m.scroll() > from);
        for _ in 0..600 {
            m.apply(Input::Frame { dt: 1.0 / 60.0 });
        }
        assert!(!m.animating());
        m.apply(Input::Settle);
        assert!(list().tops.contains(&m.scroll()));
        assert_eq!(m.live_values(), values);
    }

    #[test]
    fn a_sideways_drag_on_a_track_sets_it() {
        let mut m = settings();
        m.apply(Input::Control(Target::Row(0), CtlOut::Frac(0.5)));
        assert_eq!(volume(&m).as_deref(), Some("50"));
    }

    #[test]
    fn the_first_key_after_pointer_only_shows_the_bar_on_the_hovered_row() {
        let mut m = menu_with(
            Context {
                touch_screen: false,
                ..full()
            },
            Modality::Keys,
        );
        m.apply(Input::Tap(Target::Item(Item::Settings)));
        m.apply(Input::Pointer { touch: false });
        m.apply(Input::Over(Hit::Row(2)));
        assert_eq!(m.apply(Input::HoverEnd), [Effect::HoverChanged]);
        assert!(!m.bar());
        assert_eq!(m.help_row(), Some(2), "the hovered row's help");
        assert!(nav(&mut m, Nav::Down).is_empty());
        assert!(m.bar());
        assert_eq!(m.cursor(), Cursor::Row(2));
        nav(&mut m, Nav::Down);
        assert_eq!(m.cursor(), Cursor::Row(3));
    }

    #[test]
    fn touch_keeps_no_hover_and_help_follows_the_last_row() {
        let mut m = menu_with(full(), Modality::Touch);
        m.apply(Input::Tap(Target::Item(Item::Settings)));
        m.apply(Input::Over(Hit::Row(2)));
        m.apply(Input::HoverEnd);
        assert_eq!(m.help_row(), Some(0));
        nav(&mut m, Nav::Down);
        assert_eq!(m.cursor(), Cursor::Row(0), "the first key moves nothing");
    }

    #[test]
    fn a_key_keeps_its_row_in_view() {
        let mut m = settings();
        m.apply(Input::Geometry(list()));
        for _ in 0..3 {
            nav(&mut m, Nav::Down);
        }
        m.apply(Input::Settle);
        assert_eq!(m.scroll(), 200.0);
        m.set_moved(false);
        m.apply(Input::ScrollTo(0.0));
        m.apply(Input::Settle);
        assert_eq!(
            m.scroll(),
            0.0,
            "scrolling away is not undone without a key"
        );
    }

    #[test]
    fn resume_and_the_card_order() {
        let mut m = menu();
        assert_eq!(m.item(), Item::Resume);
        assert_eq!(nav(&mut m, Nav::Activate), [Effect::Resume]);
        assert_eq!(
            m.items(),
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
        let items = m.items();
        for want in &items[1..] {
            nav(&mut m, Nav::Down);
            assert_eq!(m.item(), *want);
        }
        nav(&mut m, Nav::Down);
        assert_eq!(m.item(), Item::Quit);
        let mut split = menu_with(
            Context {
                split: true,
                ..full()
            },
            Modality::Keys,
        );
        assert_eq!(
            split.items()[..4],
            [Item::Resume, Item::Settings, Item::Retry, Item::Quit]
        );
        nav(&mut split, Nav::Down);
        assert_eq!(split.item(), Item::Settings);
    }

    #[test]
    fn an_item_gone_from_the_card_falls_back_to_resume() {
        let mut m = menu();
        m.apply(Input::Tap(Target::Item(Item::Quit)));
        m.apply(Input::Tap(Target::Answer(false)));
        assert_eq!(m.item(), Item::Quit);
        m.set_context(Context {
            quit: false,
            ..full()
        });
        assert_eq!(m.item(), Item::Resume);
    }

    #[test]
    fn card_values_step() {
        let mut m = menu();
        for _ in 0..1 {
            nav(&mut m, Nav::Down);
        }
        assert_eq!(
            nav(&mut m, Nav::Right),
            [Effect::Hitboxes(Hitboxes::Player)]
        );
        assert!(nav(&mut m, Nav::Left).is_empty(), "already at the end");
        assert_eq!(
            nav(&mut m, Nav::Activate),
            [Effect::Hitboxes(Hitboxes::Player)]
        );
        nav(&mut m, Nav::Down);
        let v = volume(&m);
        assert_eq!(nav(&mut m, Nav::Left), [Effect::SettingsChanged]);
        assert_ne!(volume(&m), v);
        nav(&mut m, Nav::Down);
        assert_eq!(nav(&mut m, Nav::Right), [Effect::Speed((3, 4))]);
    }

    #[test]
    fn a_question_asks_in_place_with_the_safe_answer_focused() {
        for (item, ask, act) in [
            (Item::Retry, Ask::Retry, Effect::Retry),
            (Item::Quit, Ask::Quit, Effect::Quit),
        ] {
            let mut m = menu();
            m.apply(Input::Tap(Target::Item(item)));
            assert_eq!((m.ask(), m.yes()), (Some(ask), false));
            assert!(nav(&mut m, Nav::Activate).is_empty());
            assert_eq!((m.ask(), m.item()), (None, item));
            m.apply(Input::Tap(Target::Item(item)));
            assert!(m.back());
            assert_eq!((m.ask(), m.item()), (None, item));
            m.apply(Input::Tap(Target::Item(item)));
            nav(&mut m, Nav::Down);
            assert_eq!(nav(&mut m, Nav::Activate), [act]);
            assert_eq!(m.ask(), None);
        }
    }

    // The model's sources name no toolkit and no module that uses one.
    #[test]
    fn the_model_imports_no_toolkit() {
        for src in [
            include_str!("menu_model.rs"),
            include_str!("settings_store.rs"),
        ] {
            let code = src.split("#[cfg(test)]").next().unwrap_or(src);
            for word in [
                "egui",
                "slint",
                "wgpu",
                "winit",
                "crate::theme",
                "crate::launcher",
                "crate::platform",
                "crate::slintui",
                "crate::transport",
            ] {
                assert!(!code.contains(word), "{word}");
            }
        }
    }

    // Focus and navigation live here, not in the Slint views.
    #[test]
    fn no_key_handling_in_slint() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/ui");
        for e in std::fs::read_dir(dir).unwrap() {
            let path = e.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            for word in ["Key.", "key-pressed", "FocusScope", "wake"] {
                assert!(!text.contains(word), "{word} in {}", path.display());
            }
        }
    }
}
