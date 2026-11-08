//! The port menu's settings page on Slint: binds `launcher::Model` rows to `ui/settings.slint`.
use super::host::{self, Gpu, Surface};
use super::input::{self, Pointer, Pointing};
use super::text::breakable;
use super::{Cap, Geometry, Hint, Modality, RowData, SettingsPage, Ui};
use crate::launcher::Model;
use crate::platform::portmenu::{self, Nav, Shape};
use crate::theme::{Ctl, CtlOut};
use slint::{ComponentHandle, Global, Model as _, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use th08_render::gfx::{OverlayTarget, SafeArea};
use winit::event::WindowEvent;

#[derive(Clone, Copy, Debug)]
enum Cmd {
    Step(usize, i32),
    Pick(usize, usize),
    Frac(usize, f32),
    Activate(usize),
    Reset,
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
    pub ui: SettingsPage,
    surface: Surface,
    cmds: Rc<RefCell<Vec<Cmd>>>,
    rows: Rc<VecModel<RowData>>,
    keys: Vec<&'static str>,
    tab: usize,
    pointer: Pointer,
    place: Option<Place>,
    words: Words,
    generation: u32,
}

fn idx(i: i32) -> usize {
    usize::try_from(i).unwrap_or(usize::MAX)
}

fn s(t: &str) -> SharedString {
    t.into()
}

fn strings(v: Vec<String>) -> ModelRc<SharedString> {
    let v: Vec<SharedString> = v.into_iter().map(SharedString::from).collect();
    ModelRc::new(VecModel::from(v))
}

fn hints(items: &[(String, String)]) -> ModelRc<Hint> {
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
            (panel, panel + 48.0, right - 64.0 - panel - 48.0)
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

impl Page {
    pub fn new(
        gpu: Gpu,
        generation: u32,
        words: Words,
        touch: bool,
    ) -> Result<Self, slint::PlatformError> {
        host::use_gpu(gpu.clone())?;
        let ui = SettingsPage::new()?;
        ui.show()?;
        let adapter = host::take_adapter().ok_or(slint::PlatformError::NoPlatform)?;
        let cmds = Rc::new(RefCell::new(Vec::new()));
        let rows = Rc::new(VecModel::<RowData>::default());
        ui.set_rows(ModelRc::from(rows.clone()));
        let c = cmds.clone();
        ui.on_step(move |i, d| c.borrow_mut().push(Cmd::Step(idx(i), d)));
        let c = cmds.clone();
        ui.on_pick(move |i, w| c.borrow_mut().push(Cmd::Pick(idx(i), idx(w))));
        let c = cmds.clone();
        ui.on_set_frac(move |i, f| c.borrow_mut().push(Cmd::Frac(idx(i), f)));
        let c = cmds.clone();
        ui.on_activate(move |i| c.borrow_mut().push(Cmd::Activate(idx(i))));
        let c = cmds.clone();
        ui.on_reset_page(move || c.borrow_mut().push(Cmd::Reset));
        let c = cmds.clone();
        ui.on_back(move || c.borrow_mut().push(Cmd::Back));
        let c = cmds.clone();
        ui.on_tab_changed(move |i| c.borrow_mut().push(Cmd::Tab(idx(i))));
        ui.set_heading(s(&words.heading));
        ui.set_caption(s(&words.caption));
        ui.set_back_word(s(&words.back));
        ui.set_back_key(s(&words.back_key));
        ui.set_prev_key(s(&words.prev_key));
        ui.set_next_key(s(&words.next_key));
        ui.set_device(s(&words.device));
        ui.set_ask(s(&breakable(&words.ask)));
        ui.set_hints(hints(&words.hints));
        let g = Ui::get(&ui);
        g.set_touch(touch);
        g.set_modality(if touch {
            Modality::Touch
        } else {
            Modality::Keys
        });
        Ok(Self {
            surface: Surface::new(adapter, gpu),
            ui,
            cmds,
            rows,
            keys: Vec::new(),
            tab: 0,
            pointer: Pointer::default(),
            place: None,
            words,
            generation,
        })
    }

    pub fn words(&self) -> &Words {
        &self.words
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn place(&mut self, p: Place) {
        let u = p.lay.ppp;
        let g = Ui::get(&self.ui);
        g.set_split(p.lay.shape == Shape::Split);
        g.set_pt_per_dp(p.lay.density / u);
        self.ui.set_geo(geometry(&p));
        self.surface.resize(p.size.0, p.size.1, u);
        self.place = Some(p);
    }

    pub fn tab(&self) -> usize {
        self.tab
    }

    pub fn adapter(&self) -> &Rc<host::Adapter> {
        self.surface.adapter()
    }

    // Opens tab `tab`, the rows from `model`.
    pub fn show_tab(&mut self, tab: usize, model: &Model, lang: crate::i18n::Lang) {
        let tabs = model.live_tabs();
        self.tab = tab.min(tabs.len().saturating_sub(1));
        self.ui.set_tab(i32::try_from(self.tab).unwrap_or(0));
        self.ui.set_tabs(strings(
            tabs.iter()
                .map(|&g| breakable(crate::i18n::group_title(lang, g)))
                .collect(),
        ));
        self.keys = tabs
            .get(self.tab)
            .map_or_else(Vec::new, |&g| model.live_rows(g));
        self.rows.set_vec(self.row_data(model, lang));
        let pieces = self
            .keys
            .iter()
            .flat_map(|k| crate::theme::unbreakable(model.row_text(k, lang).0))
            .map(str::to_string)
            .collect();
        self.ui.set_label_pieces(strings(pieces));
        self.ui.invoke_scroll_top();
    }

    fn row_data(&self, model: &Model, lang: crate::i18n::Lang) -> Vec<RowData> {
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
            label: s(&breakable(&self.words.reset)),
            help: s(&breakable(&self.words.reset_help)),
            rule: true,
            sel: -1,
            ..RowData::default()
        });
        v
    }

    // Writes changed rows in place so the list keeps its scroll position and press state.
    fn refresh(&self, model: &Model, lang: crate::i18n::Lang) {
        let new = self.row_data(model, lang);
        for (i, r) in new.into_iter().enumerate() {
            let same = self.rows.row_data(i).is_some_and(|o| {
                o.frac.to_bits() == r.frac.to_bits()
                    && o.sel == r.sel
                    && o.number == r.number
                    && o.value == r.value
                    && o.label == r.label
                    && o.help == r.help
                    && o.words.iter().eq(r.words.iter())
            });
            if !same {
                self.rows.set_row_data(i, r);
            }
        }
    }

    // Applies what the page asked for since the last call; true when the page asks to go back.
    pub fn apply(&mut self, model: &mut Model, lang: crate::i18n::Lang) -> bool {
        let cmds: Vec<Cmd> = self.cmds.borrow_mut().drain(..).collect();
        let mut back = false;
        for c in cmds {
            let key = |i: usize| self.keys.get(i).copied();
            match c {
                Cmd::Step(i, d) => {
                    if let Some(k) = key(i) {
                        model.step(k, d);
                    }
                }
                Cmd::Pick(i, w) => {
                    if let Some(k) = key(i) {
                        model.ctl_input(k, CtlOut::Pick(w));
                    }
                }
                Cmd::Frac(i, f) => {
                    if let Some(k) = key(i) {
                        model.ctl_input(k, CtlOut::Frac(f));
                    }
                }
                Cmd::Activate(i) => {
                    if let Some(k) = key(i) {
                        model.activate(k);
                    }
                }
                Cmd::Reset => model.reset_rows(&self.keys),
                Cmd::Back => back = true,
                Cmd::Tab(t) => self.show_tab(t, model, lang),
            }
        }
        self.refresh(model, lang);
        back
    }

    pub fn nav(&self, n: Nav) {
        input::send_nav(self.surface.adapter(), n);
    }

    // Pointer input; true when the event was pointer input.
    pub fn on_event(&mut self, ev: &WindowEvent) -> bool {
        let Some(p) = self.place else {
            return false;
        };
        let Some(kind) = self.pointer.event(self.surface.adapter(), ev, p.lay.ppp) else {
            return false;
        };
        Ui::get(&self.ui).set_modality(match kind {
            Pointing::Mouse => Modality::Mouse,
            Pointing::Touch => Modality::Touch,
        });
        true
    }

    // Runs timers; returns how soon the next repaint is due.
    pub fn tick(&self) -> Option<std::time::Duration> {
        if host::tick(self.surface.adapter()) {
            return Some(std::time::Duration::from_millis(16));
        }
        host::next_timer()
    }

    pub fn paint(&mut self, t: &OverlayTarget<'_>, enc: &mut wgpu::CommandEncoder) {
        self.surface.paint(t, enc);
    }
}
