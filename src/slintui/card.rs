//! The port menu's pause card on Slint: shows what `portmenu` decides and reports taps back to it.
use super::host::{self, Gpu, Surface};
use super::input::{Pointer, Pointing};
use super::settings_page::{Place, hints, s, strings};
use super::text::breakable;
use super::{CardGeo, Modality, PauseCard, RowData, Ui};
use crate::platform::portmenu::Shape;
use slint::{ComponentHandle, Global, Model as _, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use th08_render::gfx::OverlayTarget;
use winit::event::WindowEvent;

// What a tap on the card asks for; an index is the item's place in the focus order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    Activate(usize),
    Step(usize, i32),
    Pick(usize, usize),
    Frac(usize, f32),
    Answer(bool),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardItem {
    pub kind: i32,
    pub label: String,
    pub caption: String,
    pub words: Vec<String>,
    pub sel: i32,
    pub frac: f32,
    pub number: String,
    pub unit: String,
}

impl CardItem {
    fn row(&self) -> RowData {
        RowData {
            kind: self.kind,
            label: s(&breakable(&self.label)),
            caption: s(&self.caption),
            words: strings(self.words.iter().map(|w| breakable(w)).collect()),
            sel: self.sel,
            frac: self.frac,
            number: s(&self.number),
            unit: s(&self.unit),
            ..RowData::default()
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
#[expect(clippy::struct_excessive_bools, reason = "independent view flags")]
pub struct CardView {
    pub resume: CardItem,
    pub settings: CardItem,
    pub mids: Vec<CardItem>,
    pub dangers: Vec<CardItem>,
    pub quick: Vec<CardItem>,
    pub focused: i32,
    pub ask: i32,
    pub yes: bool,
    pub heading: String,
    pub caption: String,
    pub quick_caption: String,
    pub help: String,
    pub notes: Vec<String>,
    pub device: String,
    pub version: String,
    pub device_line: String,
    pub question: String,
    pub note: String,
    pub keep: String,
    pub act: String,
    pub hints: Vec<(String, String)>,
    pub ru: bool,
    pub keys: bool,
    pub mouse: bool,
}

fn idx(i: i32) -> usize {
    usize::try_from(i).unwrap_or(usize::MAX)
}

fn geometry(p: &Place) -> CardGeo {
    let u = p.lay.ppp;
    let (sw, sh) = (p.size.0 as f32 / u, p.size.1 as f32 / u);
    let [fx, fy, fw, fh] = p.lay.frame.map(|v| v / u);
    let gs = fw / 640.0;
    let frame_r = fx + fw;
    let (left, top) = (p.safe.left as f32 / u, p.safe.top as f32 / u);
    let right = sw - p.safe.right as f32 / u;
    let bottom = sh - p.safe.bottom as f32 / u;
    let game_y = |y: f32| fy + y * gs;
    let mut g = CardGeo {
        shape: 0,
        top,
        bottom,
        frame_l: fx,
        frame_r,
        frame_t: fy,
        frame_h: fh,
        ..CardGeo::default()
    };
    match p.lay.shape {
        Shape::Pillar => {
            g.x = frame_r + 44.0;
            g.w = right - 44.0 - g.x;
            g.band_l = frame_r + 6.0;
            g.band_r = sw;
        }
        Shape::Column => {
            let (pf_x, pf_w) = (fx + 32.0 * gs, 384.0 * gs);
            g.shape = 1;
            g.pf_x = pf_x;
            g.pf_y = game_y(16.0);
            g.pf_w = pf_w;
            g.pf_h = 448.0 * gs;
            g.w = 396f32.min(pf_w - 48.0);
            g.x = pf_x + pf_w / 2.0 - 7.0 * gs - g.w / 2.0;
            g.band_l = g.x - 24.0;
            g.band_r = g.x + g.w + 16.0;
            g.top = fy + 80.0;
            g.soft_x = g.x - 64.0;
            g.soft_y = g.pf_y + 40.0;
            g.soft_w = g.w + 120.0;
            g.soft_h = game_y(372.0) - g.soft_y;
            g.list_limit = game_y(336.0) + 12.0;
            g.foot_limit = game_y(360.0);
        }
        Shape::Split => {
            g.shape = 2;
            g.x = left + 20.0;
            g.w = fx - 36.0 - g.x;
            g.band_l = left;
            g.band_r = fx - 6.0;
            g.rx = frame_r + 20.0;
            g.rw = right - 20.0 - g.rx;
            g.rband_l = frame_r + 6.0;
            g.rband_r = sw;
        }
    }
    g
}

fn write_rows(model: &VecModel<RowData>, old: &[CardItem], new: &[CardItem]) {
    if old.len() != new.len() {
        model.set_vec(new.iter().map(CardItem::row).collect::<Vec<_>>());
        return;
    }
    for (i, (o, n)) in old.iter().zip(new).enumerate() {
        if o != n && i < model.row_count() {
            model.set_row_data(i, n.row());
        }
    }
}

pub struct Card {
    pub ui: PauseCard,
    surface: Surface,
    cmds: Rc<RefCell<Vec<Cmd>>>,
    mids: Rc<VecModel<RowData>>,
    dangers: Rc<VecModel<RowData>>,
    quick: Rc<VecModel<RowData>>,
    pointer: Pointer,
    place: Option<Place>,
    view: Option<CardView>,
    generation: u32,
}

impl Card {
    pub fn new(gpu: Gpu, generation: u32) -> Result<Self, slint::PlatformError> {
        host::use_gpu(gpu.clone())?;
        log::info!("slintui: pause card opened");
        let ui = PauseCard::new()?;
        ui.show()?;
        let adapter = host::take_adapter().ok_or(slint::PlatformError::NoPlatform)?;
        let cmds = Rc::new(RefCell::new(Vec::new()));
        let model = || Rc::new(VecModel::<RowData>::default());
        let (mids, dangers, quick) = (model(), model(), model());
        ui.set_mids(ModelRc::from(mids.clone()));
        ui.set_dangers(ModelRc::from(dangers.clone()));
        ui.set_quick(ModelRc::from(quick.clone()));
        let c = cmds.clone();
        ui.on_activate(move |i| c.borrow_mut().push(Cmd::Activate(idx(i))));
        let c = cmds.clone();
        ui.on_step(move |i, d| c.borrow_mut().push(Cmd::Step(idx(i), d)));
        let c = cmds.clone();
        ui.on_pick(move |i, w| c.borrow_mut().push(Cmd::Pick(idx(i), idx(w))));
        let c = cmds.clone();
        ui.on_set_frac(move |i, f| c.borrow_mut().push(Cmd::Frac(idx(i), f)));
        let c = cmds.clone();
        ui.on_answer(move |y| c.borrow_mut().push(Cmd::Answer(y)));
        Ok(Self {
            surface: Surface::new(adapter, gpu),
            ui,
            cmds,
            mids,
            dangers,
            quick,
            pointer: Pointer::default(),
            place: None,
            view: None,
            generation,
        })
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn take_cmds(&self) -> Vec<Cmd> {
        self.cmds.borrow_mut().drain(..).collect()
    }

    pub fn hovered_item(&self) -> Option<usize> {
        usize::try_from(self.ui.get_hovered()).ok()
    }

    pub fn hovered_answer(&self) -> Option<bool> {
        match self.ui.get_hovered_answer() {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    // Writes what changed, so rows keep their press state between passes.
    pub fn show(&mut self, v: CardView, p: Place) {
        let u = p.lay.ppp;
        let g = Ui::get(&self.ui);
        g.set_touch(p.lay.touch);
        g.set_split(p.lay.shape == Shape::Split);
        g.set_pt_per_dp(p.lay.density / u);
        g.set_ru(v.ru);
        g.set_modality(if v.keys {
            Modality::Keys
        } else if v.mouse {
            Modality::Mouse
        } else {
            Modality::Touch
        });
        self.ui.set_geo(geometry(&p));
        self.surface.resize(p.size.0, p.size.1, u);
        self.place = Some(p);
        if self.view.as_ref() == Some(&v) {
            return;
        }
        let empty = CardView::default();
        let old = self.view.as_ref().unwrap_or(&empty);
        write_rows(&self.mids, &old.mids, &v.mids);
        write_rows(&self.dangers, &old.dangers, &v.dangers);
        write_rows(&self.quick, &old.quick, &v.quick);
        if old.resume != v.resume {
            self.ui.set_resume(v.resume.row());
        }
        if old.settings != v.settings {
            self.ui.set_settings(v.settings.row());
        }
        if old.notes != v.notes {
            self.ui
                .set_notes(strings(v.notes.iter().map(|n| breakable(n)).collect()));
        }
        if old.hints != v.hints {
            let cut = v.hints.len().min(3);
            self.ui.set_hints_first(hints(&v.hints[..cut]));
            self.ui.set_hints_rest(hints(&v.hints[cut..]));
        }
        self.ui.set_focused(v.focused);
        self.ui.set_ask(v.ask);
        self.ui.set_yes(v.yes);
        self.ui.set_heading(s(&v.heading));
        self.ui.set_caption(s(&v.caption));
        self.ui.set_quick_caption(s(&v.quick_caption));
        self.ui.set_help(s(&breakable(&v.help)));
        self.ui.set_device(s(&v.device));
        self.ui.set_version(s(&v.version));
        self.ui.set_device_line(s(&v.device_line));
        self.ui.set_question(s(&breakable(&v.question)));
        self.ui.set_note(s(&breakable(&v.note)));
        self.ui.set_keep(s(&v.keep));
        self.ui.set_act(s(&v.act));
        self.view = Some(v);
    }

    pub fn adapter(&self) -> &Rc<host::Adapter> {
        self.surface.adapter()
    }

    // Pointer input; its kind, None when the event is not pointer input or the card has no place yet.
    pub fn on_event(&mut self, ev: &WindowEvent) -> Option<Pointing> {
        let p = self.place?;
        self.pointer.event(self.surface.adapter(), ev, p.lay.ppp)
    }

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
