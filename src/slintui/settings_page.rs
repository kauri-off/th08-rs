//! The port menu's settings page on Slint: shows `MenuModel` state in `ui/settings.slint` and reports what is tapped.
use super::host::Adapter;
use super::text::breakable;
use super::{Cap, Geometry, Hint, MenuRoot, Modality, PageApi, RowData, Ui};
use crate::menu_model::{Cursor, Effect, Hit, Input, MenuModel, Nav, Target};
use crate::platform::portmenu::{self, Shape};
use crate::settings_store::SettingsStore as Model;
use crate::theme::{Ctl, CtlOut};
use slint::{Global, Model as _, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use th08_render::gfx::SafeArea;

#[derive(Clone, Copy, Debug)]
enum Cmd {
    Step(usize, i32),
    Pick(usize, usize),
    Frac(usize, f32),
    Activate(usize),
    Back,
    Tab(usize),
}

// The page's fixed strings in the menu's language.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Words {
    pub heading: String,
    pub caption: String,
    pub back: String,
    pub back_key: String,
    pub prev_key: String,
    pub next_key: String,
    pub device: String,
    pub ask: String,
    pub reset: String,
    pub reset_help: String,
    pub hints: Vec<(String, String)>,
}

// The window as the page lays it out.
#[derive(Clone, Copy, Debug)]
pub struct Place {
    pub lay: portmenu::Layout,
    pub safe: SafeArea,
    pub size: (u32, u32),
}

pub struct Page {
    ui: MenuRoot,
    adapter: Rc<Adapter>,
    cmds: Rc<RefCell<Vec<Cmd>>>,
    rows: Rc<VecModel<RowData>>,
    keys: Vec<&'static str>,
    tab: usize,
    tabs: Vec<String>,
    pieces: Vec<String>,
    words: Option<Words>,
}

fn idx(i: i32) -> usize {
    usize::try_from(i).unwrap_or(usize::MAX)
}

pub(super) fn s(t: &str) -> SharedString {
    t.into()
}

pub(super) fn strings(v: Vec<String>) -> ModelRc<SharedString> {
    let v: Vec<SharedString> = v.into_iter().map(SharedString::from).collect();
    ModelRc::new(VecModel::from(v))
}

pub(super) fn hints(items: &[(String, String)]) -> ModelRc<Hint> {
    let cap = |t: &str| match t {
        "Ⓐ" => Cap {
            text: s("A"),
            disc: true,
        },
        "Ⓑ" => Cap {
            text: s("B"),
            disc: true,
        },
        _ => Cap {
            text: s(t),
            disc: false,
        },
    };
    let v: Vec<Hint> = items
        .iter()
        .map(|(k, w)| Hint {
            caps: ModelRc::new(VecModel::from(k.split(' ').map(cap).collect::<Vec<_>>())),
            word: s(w),
        })
        .collect();
    ModelRc::new(VecModel::from(v))
}

fn geometry(p: &Place) -> Geometry {
    let u = p.lay.ppp;
    let (sw, sh) = (p.size.0 as f32 / u, p.size.1 as f32 / u);
    let [fx, _, fw, _] = p.lay.frame.map(|v| v / u);
    let gs = fw / 640.0;
    let (left, top) = (p.safe.left as f32 / u, p.safe.top as f32 / u);
    let right = sw - p.safe.right as f32 / u;
    let bottom = sh - p.safe.bottom as f32 / u;
    let (panel, x, w) = match p.lay.shape {
        Shape::Split => {
            let panel = fx + 416.0 * gs;
            (
                panel,
                panel + 48.0,
                (right - 16.0).min(sw - 64.0) - panel - 48.0,
            )
        }
        Shape::Pillar => {
            let panel = fx + 421.33 * gs;
            (panel, panel + 52.0, right - 44.0 - panel - 52.0)
        }
        Shape::Column => {
            let panel = (fx + 360.0 * gs).min(right - 534.0);
            (panel, panel + 36.0, right - 28.0 - panel - 36.0)
        }
    };
    let rail_x = left + 20.0;
    Geometry {
        panel_x: panel,
        x,
        w,
        top,
        bottom,
        rail_w: fx,
        rail_x,
        rail_text_w: fx - 36.0 - rail_x,
    }
}

fn same_row(a: &RowData, b: &RowData) -> bool {
    a.kind == b.kind
        && a.frac.to_bits() == b.frac.to_bits()
        && a.sel == b.sel
        && a.rule == b.rule
        && a.number == b.number
        && a.unit == b.unit
        && a.value == b.value
        && a.label == b.label
        && a.caption == b.caption
        && a.help == b.help
        && a.words.iter().eq(b.words.iter())
        && a.pieces.iter().eq(b.pieces.iter())
}

impl Page {
    pub fn new(ui: MenuRoot, adapter: Rc<Adapter>) -> Self {
        let cmds = Rc::new(RefCell::new(Vec::new()));
        let rows = Rc::new(VecModel::<RowData>::default());
        let api = PageApi::get(&ui);
        api.set_rows(ModelRc::from(rows.clone()));
        let c = cmds.clone();
        api.on_step(move |i, d| c.borrow_mut().push(Cmd::Step(idx(i), d)));
        let c = cmds.clone();
        api.on_pick(move |i, w| c.borrow_mut().push(Cmd::Pick(idx(i), idx(w))));
        let c = cmds.clone();
        api.on_set_frac(move |i, f| c.borrow_mut().push(Cmd::Frac(idx(i), f)));
        let c = cmds.clone();
        api.on_activate(move |i| c.borrow_mut().push(Cmd::Activate(idx(i))));
        let c = cmds.clone();
        api.on_back(move || c.borrow_mut().push(Cmd::Back));
        let c = cmds.clone();
        api.on_tab_changed(move |i| c.borrow_mut().push(Cmd::Tab(idx(i))));
        drop(api);
        Self {
            ui,
            adapter,
            cmds,
            rows,
            keys: Vec::new(),
            tab: 0,
            tabs: Vec::new(),
            pieces: Vec::new(),
            words: None,
        }
    }

    pub fn api(&self) -> PageApi<'_> {
        PageApi::get(&self.ui)
    }

    // The view was just switched in: nothing from its last time on screen counts.
    pub fn open(&self, touch: bool) {
        self.cmds.borrow_mut().clear();
        self.api().set_content_y(0.0);
        self.api().set_hovered(-1);
        Ui::get(&self.ui).set_touch(touch);
    }

    // The fixed strings; true when they changed.
    pub fn set_words(&mut self, words: &Words) -> bool {
        if self.words.as_ref() == Some(words) {
            return false;
        }
        let api = self.api();
        api.set_heading(s(&words.heading));
        api.set_caption(s(&words.caption));
        api.set_back_word(s(&words.back));
        api.set_back_key(s(&words.back_key));
        api.set_prev_key(s(&words.prev_key));
        api.set_next_key(s(&words.next_key));
        api.set_device(s(&words.device));
        api.set_ask(s(&breakable(&words.ask)));
        let cut = words.hints.len().min(3);
        api.set_hints_first(hints(&words.hints[..cut]));
        api.set_hints_rest(hints(&words.hints[cut..]));
        self.words = Some(words.clone());
        true
    }

    pub fn place(&self, p: Place) {
        let u = p.lay.ppp;
        let g = Ui::get(&self.ui);
        g.set_split(p.lay.shape == Shape::Split);
        g.set_pt_per_dp(p.lay.density / u);
        self.api().set_geo(geometry(&p));
        self.adapter.resize(p.size.0, p.size.1, u);
    }

    pub fn tab(&self) -> usize {
        self.tab
    }

    // Opens tab `tab`: the strip and label measures only when they differ, rows in place so unchanged ones are kept.
    pub fn show_tab(&mut self, tab: usize, model: &Model, lang: crate::i18n::Lang) {
        Ui::get(&self.ui).set_ru(lang == crate::i18n::Lang::Ru);
        let tabs = model.live_tabs();
        self.tab = tab.min(tabs.len().saturating_sub(1));
        self.api().set_tab(i32::try_from(self.tab).unwrap_or(0));
        let titles: Vec<String> = tabs
            .iter()
            .map(|&g| breakable(crate::i18n::group_title(lang, g)))
            .collect();
        if titles != self.tabs {
            self.api().set_tabs(strings(titles.clone()));
            self.tabs = titles;
        }
        self.keys = tabs
            .get(self.tab)
            .map_or_else(Vec::new, |&g| model.live_rows(g));
        self.sync_rows(model, lang);
        let pieces: Vec<String> = self
            .keys
            .iter()
            .flat_map(|k| crate::theme::unbreakable(model.row_text(k, lang).0))
            .map(str::to_string)
            .collect();
        if pieces != self.pieces {
            self.api().set_label_pieces(strings(pieces.clone()));
            self.pieces = pieces;
        }
    }

    fn row_data(&self, model: &Model, lang: crate::i18n::Lang) -> Vec<RowData> {
        let (reset, reset_help) = self
            .words
            .as_ref()
            .map_or(("", ""), |w| (w.reset.as_str(), w.reset_help.as_str()));
        let mut v: Vec<RowData> = self
            .keys
            .iter()
            .map(|k| {
                let (label, help) = model.row_text(k, lang);
                let mut r = RowData {
                    label: s(&breakable(label)),
                    help: s(&breakable(&portmenu::sentence(help))),
                    pieces: strings(
                        crate::theme::unbreakable(label)
                            .iter()
                            .map(ToString::to_string)
                            .collect(),
                    ),
                    sel: -1,
                    words: ModelRc::default(),
                    ..RowData::default()
                };
                match model.ctl(k, lang) {
                    Ctl::Choices { words, sel } => {
                        r.kind = 0;
                        r.sel = sel.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1);
                        r.words = strings(words.iter().map(|w| breakable(w)).collect());
                    }
                    Ctl::Slider { frac, text, unit } => {
                        r.kind = 1;
                        r.frac = frac;
                        r.number = s(&text);
                        r.unit = s(&unit);
                    }
                    Ctl::Value(t) => {
                        r.kind = 2;
                        r.value = s(&breakable(&t));
                    }
                    Ctl::None => {
                        r.kind = 2;
                        r.value = s(&model.value_text(k).unwrap_or_default());
                    }
                }
                r
            })
            .collect();
        v.push(RowData {
            kind: 3,
            label: s(&breakable(reset)),
            help: s(&breakable(reset_help)),
            rule: true,
            sel: -1,
            ..RowData::default()
        });
        v
    }

    // Writes changed rows in place; the list keeps its other rows, its scroll position and press state.
    fn sync_rows(&self, model: &Model, lang: crate::i18n::Lang) {
        let new = self.row_data(model, lang);
        let keep = new.len();
        for (i, r) in new.into_iter().enumerate() {
            match self.rows.row_data(i) {
                Some(o) if same_row(&o, &r) => {}
                Some(_) => self.rows.set_row_data(i, r),
                None => self.rows.push(r),
            }
        }
        while self.rows.row_count() > keep {
            self.rows.remove(self.rows.row_count() - 1);
        }
    }

    // Hover, the list's position, keys and then taps go to the model; the page then shows what the model says.
    pub fn apply(
        &mut self,
        menu: &mut MenuModel,
        navs: Vec<Nav>,
        lang: crate::i18n::Lang,
    ) -> Vec<Effect> {
        let api = self.api();
        menu.apply(Input::Scrolled(api.get_scroll()));
        if let Ok(r) = usize::try_from(api.get_hovered()) {
            menu.apply(Input::Over(Hit::Row(r)));
        }
        drop(api);
        let mut out = menu.apply(Input::HoverEnd);
        for n in navs {
            out.extend(menu.apply(Input::Nav(n)));
        }
        let cmds: Vec<Cmd> = self.cmds.borrow_mut().drain(..).collect();
        for c in cmds {
            let input = match c {
                Cmd::Step(i, d) => Input::Control(Target::Row(i), CtlOut::Step(d)),
                Cmd::Pick(i, w) => Input::Control(Target::Row(i), CtlOut::Pick(w)),
                Cmd::Frac(i, f) => Input::Control(Target::Row(i), CtlOut::Frac(f)),
                Cmd::Activate(i) => Input::Tap(Target::Row(i)),
                Cmd::Back => Input::Tap(Target::Back),
                Cmd::Tab(t) => Input::Tap(Target::Tab(t)),
            };
            out.extend(menu.apply(input));
        }
        if menu.tab() != self.tab {
            self.show_tab(menu.tab(), &menu.store, lang);
        }
        self.sync_rows(&menu.store, lang);
        self.show(menu);
        if out.contains(&Effect::ScrollMoved) {
            self.api().set_content_y(-menu.scroll());
        }
        menu.set_moved(false);
        out
    }

    // Focus state as the page's properties; -1 is the tab bar or no row.
    fn show(&self, menu: &MenuModel) {
        let api = self.api();
        api.set_cursor(match menu.cursor() {
            Cursor::Tabs => -1,
            Cursor::Row(r) => i32::try_from(r).unwrap_or(0),
        });
        api.set_armed(menu.armed());
        Ui::get(&self.ui).set_modality(match menu.modality() {
            crate::menu_model::Modality::Keys => Modality::Keys,
            crate::menu_model::Modality::Pointer => Modality::Mouse,
            crate::menu_model::Modality::Touch => Modality::Touch,
        });
        api.set_help_row(
            menu.help_row()
                .and_then(|r| i32::try_from(r).ok())
                .unwrap_or(-1),
        );
    }
}
