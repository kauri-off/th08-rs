use super::{
    Ask, Ctl, Hit, Item, Layout, MenuModel, MenuState, Modality, Shape, Words, card_help,
    hint_items, item_ctl, sentence,
};
use std::ops::Range;
use th08_render::gfx::SafeArea;
use th08_ui::scene::{Arrow, Color, Dir, Face, Rect, Ring, Round, Scene, Text};
use th08_ui::text::{Measure, wrap_lines};

#[cfg(all(feature = "ui-check", not(target_arch = "wasm32")))]
pub mod check;
pub(super) mod pointer;

const PANEL: Color = Color::rgb(23, 12, 13);
const EDGE: Color = Color::rgb(90, 64, 56);
const INNER: Color = Color::rgb(51, 34, 31);
const RULE: Color = Color::rgba(90, 64, 56, 204);
const BAND: Color = Color::rgb(44, 24, 27);
const PRESSED: Color = Color::rgb(58, 34, 38);
const CELL: Color = Color::rgb(34, 18, 20);
const TRACK: Color = Color::rgb(74, 58, 63);
const TEXT: Color = Color::rgb(196, 188, 203);
const HI: Color = Color::rgb(252, 252, 252);
const HELP: Color = Color::rgb(154, 147, 166);
const NOTE: Color = Color::rgb(182, 174, 191);
const ACCENT: Color = Color::rgb(202, 194, 209);
const FILL: Color = Color::rgb(150, 108, 168);
const DANGER: Color = Color::rgb(236, 192, 195);
const QUESTION: Color = Color::rgb(246, 193, 193);
const KEY: Color = Color::rgb(236, 230, 240);

const ROW: u32 = 0;
const LESS: u32 = 1;
const MORE: u32 = 2;
const TRACK_TAG: u32 = 3;
const PICK: u32 = 4;
const KEEP: u32 = u32::MAX - 1;
const ACCEPT: u32 = u32::MAX;

fn tag(item: Item, control: u32) -> u32 {
    ((item as u32) << 8) | control
}

fn target(tag: u32, model: &MenuModel) -> Option<Hit> {
    match (model.ask(), tag) {
        (Some(_), KEEP | ACCEPT) => Some(Hit::Yes(tag == ACCEPT)),
        (None, KEEP | ACCEPT) | (Some(_), _) => None,
        (None, _) => model
            .items()
            .into_iter()
            .find(|i| *i as u32 == tag >> 8)
            .map(Hit::Item),
    }
}

pub(super) struct Metrics<'a> {
    pub lay: Layout,
    pub safe: SafeArea,
    pub size: (u32, u32),
    pub state: MenuState<'a>,
    pub pressed: Option<u32>,
}

#[derive(Clone, Copy)]
struct Strip {
    band_x: f32,
    band_w: f32,
    x: f32,
    w: f32,
}

impl Strip {
    fn at(self, y: f32, h: f32) -> Rect {
        Rect::new(self.x, y, self.w, h)
    }

    fn band_at(self, y: f32, h: f32) -> Rect {
        Rect::new(self.band_x, y, self.band_w, h)
    }
}

struct Geometry {
    left: Strip,
    right: Strip,
    window: Rect,
    field: Rect,
    frame_left: f32,
    frame_right: f32,
    top: f32,
    bottom: f32,
}

impl Geometry {
    fn new(m: &Metrics<'_>) -> Self {
        let u = m.lay.ppp;
        let [fx, fy, fw, _] = m.lay.frame.map(|v| v / u);
        let gs = fw / 640.0;
        let (sw, sh) = (m.size.0 as f32 / u, m.size.1 as f32 / u);
        let [left, top, right, bottom] = [
            m.safe.left as f32 / u,
            m.safe.top as f32 / u,
            sw - m.safe.right as f32 / u,
            sh - m.safe.bottom as f32 / u,
        ];
        let (fr, pf) = (
            fx + fw,
            Rect::new(fx + 32.0 * gs, fy + 16.0 * gs, 384.0 * gs, 448.0 * gs),
        );
        let strip = |l: f32, r: f32, inset: f32, end: f32| Strip {
            band_x: l,
            band_w: r - l,
            x: l + inset,
            w: (r - l - inset - end).max(1.0),
        };
        let right_strip = strip(fr + 6.0, right, 14.0, 20.0);
        let (left, top, bottom) = match m.lay.shape {
            Shape::Pillar => (strip(fr + 6.0, right, 38.0, 44.0), top, bottom),
            Shape::Split => (strip(left, fx - 6.0, 20.0, 30.0), top, bottom),
            Shape::Column => {
                let w = 396.0f32.min((pf.w - 48.0).max(pf.w * 0.75));
                let x = pf.x + (pf.w - w) / 2.0 - 7.0 * gs;
                let top = fy + (80.0f32).min(40.0 * gs);
                (
                    strip(
                        x - 24.0 * gs.min(1.0),
                        x + w + 16.0 * gs.min(1.0),
                        24.0 * gs.min(1.0),
                        16.0 * gs.min(1.0),
                    ),
                    top,
                    fy + 360.0 * gs,
                )
            }
        };
        Self {
            left,
            right: right_strip,
            window: Rect::new(0.0, 0.0, sw, sh),
            field: pf,
            frame_left: fx,
            frame_right: fr,
            top,
            bottom,
        }
    }

    fn backdrop(&self, s: &mut Scene, shape: Shape) {
        let Self {
            window,
            frame_left: l,
            frame_right: r,
            ..
        } = *self;
        let vertical = |x, w| Rect::new(x, 0.0, w, window.h);
        if shape == Shape::Column {
            s.rect(self.field, Color::rgba(6, 1, 3, 184));
            return;
        }
        s.rect(window, Color::rgba(6, 1, 3, 128));
        s.rect(vertical(r, window.w - r), PANEL);
        s.rect(vertical(r, 1.0), EDGE);
        s.rect(vertical(r + 3.0, 1.0), INNER);
        if shape == Shape::Split {
            s.rect(vertical(0.0, l), PANEL);
            s.rect(vertical(l - 1.0, 1.0), EDGE);
            s.rect(vertical(l - 4.0, 1.0), INNER);
        }
    }
}

struct Lines {
    lines: Vec<String>,
    size: f32,
    face: Face,
    ascent: f32,
    step: f32,
}

impl Lines {
    fn new(m: &dyn Measure, text: &str, face: Face, mut size: f32, width: f32) -> Self {
        let width = width.max(1.0);
        let lines = wrap_lines(text, width, face, size, m);
        for line in &lines {
            let w = m.width(line, face, size);
            if w > width {
                size *= width / w;
            }
        }
        let line = m.line(face, size);
        Self {
            lines,
            size,
            face,
            ascent: line.ascent,
            step: line.height,
        }
    }

    fn height(&self) -> f32 {
        self.step * self.lines.len() as f32
    }

    fn draw(self, s: &mut Scene, x: f32, y: f32, tint: Color, opacity: f32, ring: bool) {
        for (i, text) in self.lines.into_iter().enumerate() {
            s.text(Text {
                x,
                baseline: y + self.ascent + self.step * i as f32,
                text,
                face: self.face,
                size: self.size,
                color: tint.fade(opacity),
                ring: ring.then_some(Ring {
                    color: Color::BLACK.fade(opacity),
                    ..Ring::BLACK_1PX
                }),
            });
        }
    }
}

struct Typography {
    title: f32,
    caption: f32,
    resume: f32,
    row: f32,
    help: f32,
    note: f32,
    hint: f32,
    tap: f32,
    row_h: f32,
    resume_h: f32,
}

impl Typography {
    fn new(l: &Layout) -> Self {
        let f = |pt, dp| l.font(pt, dp);
        let (title, caption, resume, row, help, note, hint, row_h, resume_h) = match l.shape {
            Shape::Pillar => (42., 17., 34., 28., 18., 16., 15., 72., 88.),
            Shape::Column => (34., 15., 28., 22., 14., 14., 14., 42., 54.),
            Shape::Split => (48., 21., 44., 32., 24., 19., 16., 120., 140.),
        };
        Self {
            title: f(title, 18.0),
            caption: f(caption, 10.0),
            resume: f(resume, 16.0),
            row: f(row, 12.0),
            help: f(help, 9.5),
            note: f(note, 8.0),
            hint: f(hint, 10.0),
            tap: if l.touch {
                48.0 * l.density / l.ppp
            } else {
                0.0
            },
            row_h,
            resume_h,
        }
    }
}

enum Control {
    None,
    Choices {
        words: Vec<String>,
        selected: Option<usize>,
        widths: Option<Vec<f32>>,
    },
    Slider {
        frac: f32,
        text: String,
        stacked: bool,
    },
}

impl Control {
    fn new(c: Ctl, w: f32, cell: f32, size: f32, m: &dyn Measure, quick: bool) -> Self {
        match c {
            Ctl::Choices { words, sel } => {
                let min = if quick { cell.max(150.0) } else { cell };
                let widths: Vec<_> = words
                    .iter()
                    .map(|word| (m.width(word, Face::Serif, size) + 12.0).max(min))
                    .collect();
                let widths = (widths.iter().sum::<f32>() <= w).then_some(widths);
                Self::Choices {
                    words,
                    selected: sel,
                    widths,
                }
            }
            Ctl::Slider { frac, text, unit } => Self::Slider {
                frac,
                text: format!("{text}{unit}"),
                stacked: quick,
            },
            _ => Self::None,
        }
    }

    fn least(c: &Ctl, cell: f32, size: f32, m: &dyn Measure) -> f32 {
        let center = match c {
            Ctl::Choices { words, .. } => {
                words
                    .iter()
                    .map(|w| m.width(w, Face::Serif, size))
                    .fold(0.0, f32::max)
                    + 8.0
            }
            Ctl::Slider { text, unit, .. } => {
                65.0 + m.width(&format!("{text}{unit}"), Face::Sans, size * 0.85)
            }
            _ => return 0.0,
        };
        cell * 2.0 + center
    }
}

struct Row {
    item: Item,
    label: Lines,
    caption: Option<Lines>,
    value: Value,
    value_x: f32,
    stacked: bool,
    base: f32,
    min: f32,
}

struct Value {
    control: Control,
    cell: f32,
    size: f32,
}

impl Row {
    fn new(card: &Card<'_, '_>, it: Item, w: f32, factor: f32, quick: bool) -> Self {
        let m = card.measure;
        let t = &card.ty;
        let primary = it == Item::Resume;
        let size = if primary { t.resume } else { t.row } * factor;
        let control_size = size * 0.91;
        let cell = t.tap.max(27.0 * factor).min(w / 3.0);
        let ctl = item_ctl(it, &card.metrics.state, &card.model.store, card.words.lang);
        let has_ctl = !matches!(ctl, Ctl::None);
        let label = card.words.item(it);
        let inline_x = (m.width(label, Face::Serif, size) + 12.0).min(w * 0.47);
        let stacked =
            has_ctl && (quick || w - inline_x < Control::least(&ctl, cell, control_size, m));
        let value_x = if stacked { 0.0 } else { inline_x };
        let link = matches!(it, Item::Settings | Item::TouchLayout);
        let label_w = if has_ctl && !stacked {
            (value_x - 6.0).max(1.0)
        } else if link {
            w - size
        } else {
            w
        };
        let label = Lines::new(m, label, Face::Serif, size, label_w.max(1.0));
        let caption = if factor >= 0.9 && (card.shape != Shape::Column || primary) {
            card.words
                .latin(it)
                .map(|cap| Lines::new(m, cap, Face::Sans, t.caption * 0.8, label_w.max(1.0)))
        } else {
            None
        };
        let control = Control::new(ctl, w - value_x, cell, control_size, m, quick);
        let label_height = label.height() + caption.as_ref().map_or(0.0, Lines::height);
        let control_height = match &control {
            Control::None => 0.0,
            Control::Slider { stacked: true, .. } => t.tap + 72.0,
            _ => t.tap.max(m.line(Face::Serif, control_size).height + 6.0),
        };
        let body = if stacked {
            label_height + control_height + 4.0
        } else {
            label_height.max(control_height)
        };
        let min = t.tap.max(body + 4.0);
        let base = min.max(if primary { t.resume_h } else { t.row_h } * factor);
        Self {
            item: it,
            label,
            caption,
            value: Value {
                control,
                cell,
                size: control_size,
            },
            value_x,
            stacked,
            base,
            min,
        }
    }

    fn draw(self, card: &Card<'_, '_>, s: &mut Scene, strip: Strip, y: f32, h: f32, active: bool) {
        let band = strip.band_at(y, h);
        let it = self.item;
        let keyed = card.model.bar() && card.model.item() == it && active;
        let primary = it == Item::Resume && card.metrics.lay.touch && !card.model.bar() && active;
        let hover = card.model.hovered() == Some(Hit::Item(it)) && active;
        let pressed = card.pressed(tag(it, ROW)) && active;
        marker(s, band, keyed, primary, hover, pressed, card.shape);
        if active {
            s.hit(band, tag(it, ROW));
        }
        let tint = if keyed || primary || hover {
            HI
        } else if it.danger() {
            DANGER
        } else {
            TEXT
        };
        let opacity = if active { 1.0 } else { 0.3 };
        let label_h = self.label.height() + self.caption.as_ref().map_or(0.0, Lines::height);
        let ly = if self.stacked {
            y + 2.0
        } else {
            y + (h - label_h) / 2.0
        };
        let cy = ly + self.label.height();
        let size = self.label.size;
        self.label.draw(s, strip.x, ly, tint, opacity, true);
        if let Some(caption) = self.caption {
            caption.draw(s, strip.x, cy, ACCENT, opacity, true);
        }
        if matches!(it, Item::Settings | Item::TouchLayout) {
            card.text(
                s,
                "›",
                strip.at(y, h),
                Face::Serif,
                size * 0.85,
                HELP.fade(opacity),
                Align::Right,
            );
        }
        let x = strip.x + self.value_x;
        let top = if self.stacked { ly + label_h + 2.0 } else { y };
        let rect = Rect::new(x, top, strip.w - self.value_x, y + h - top);
        card.control(s, it, self.value, rect, active);
    }
}

fn marker(
    s: &mut Scene,
    r: Rect,
    keyed: bool,
    primary: bool,
    hover: bool,
    pressed: bool,
    shape: Shape,
) {
    if keyed || primary || hover || pressed {
        let tint = if pressed { PRESSED } else { BAND };
        s.rect(
            r,
            if shape == Shape::Column {
                tint.fade(0.8)
            } else {
                tint
            },
        );
    }
    if keyed || primary {
        s.rect(
            Rect::new(
                r.x,
                r.y + r.h * 0.2,
                if keyed { 4.0 } else { 5.0 },
                r.h * 0.6,
            ),
            ACCENT,
        );
    }
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

struct Card<'a, 'b> {
    model: &'a MenuModel,
    metrics: &'a Metrics<'b>,
    measure: &'a dyn Measure,
    ty: Typography,
    words: Words,
    shape: Shape,
}

impl Card<'_, '_> {
    fn pressed(&self, row: u32) -> bool {
        self.model.modality() == Modality::Touch
            && self
                .metrics
                .pressed
                .is_some_and(|p| p == row || (p < KEEP && row < KEEP && p >> 8 == row >> 8))
    }

    #[expect(clippy::too_many_arguments, reason = "one positioned text run")]
    fn text(
        &self,
        s: &mut Scene,
        text: &str,
        r: Rect,
        face: Face,
        size: f32,
        tint: Color,
        align: Align,
    ) {
        let size = size.min(size * r.w / self.measure.width(text, face, size).max(1.0));
        let w = self.measure.width(text, face, size);
        let line = self.measure.line(face, size);
        let x = r.x
            + match align {
                Align::Left => 0.0,
                Align::Center => (r.w - w) / 2.0,
                Align::Right => r.w - w,
            };
        s.text(Text {
            x,
            baseline: r.y + (r.h - line.height) / 2.0 + line.ascent,
            text: text.into(),
            face,
            size,
            color: tint,
            ring: (face == Face::Serif).then_some(Ring {
                color: Color::BLACK.fade(f32::from(tint.0[3]) / 255.0),
                ..Ring::BLACK_1PX
            }),
        });
    }

    fn control(&self, s: &mut Scene, it: Item, value: Value, r: Rect, active: bool) {
        let Value {
            control,
            cell,
            size,
        } = value;
        let button_h = if matches!(control, Control::Slider { stacked: true, .. }) {
            self.ty.tap.max(r.h - 72.0)
        } else {
            r.h
        };
        let opacity = if active { 1.0 } else { 0.3 };
        let ink = |c: Color| c.fade(opacity);
        let label = |s: &mut Scene, word: &str, rect, selected| {
            self.text(
                s,
                word,
                rect,
                Face::Serif,
                size,
                ink(if selected { HI } else { HELP }),
                Align::Center,
            );
        };
        let arrow = |s: &mut Scene, rect: Rect, dir| {
            if self.metrics.lay.touch {
                s.rect(rect, ink(CELL));
            }
            let a = 16.0f32.min(size * 0.55);
            s.arrow(Arrow {
                x: rect.x + (rect.w - a) / 2.0,
                y: rect.y + (rect.h - a) / 2.0,
                size: a,
                dir,
                color: ink(ACCENT),
                ring: Some(Ring {
                    color: Color::BLACK.fade(opacity),
                    ..Ring::BLACK_1PX
                }),
            });
        };
        let left = Rect::new(r.x, r.y, cell, button_h);
        let right = Rect::new(r.x + r.w - cell, r.y, cell, button_h);
        let mid = Rect::new(r.x + cell, r.y, (r.w - cell * 2.0).max(1.0), button_h);
        match control {
            Control::None => return,
            Control::Choices {
                words,
                selected,
                widths: Some(widths),
            } => {
                let mut x = r.x + r.w - widths.iter().sum::<f32>();
                for (i, (word, w)) in words.iter().zip(widths).enumerate() {
                    let rect = Rect::new(x, r.y, w, r.h);
                    label(s, word, rect, Some(i) == selected);
                    if active {
                        s.hit(rect, tag(it, PICK + u32::try_from(i).unwrap_or_default()));
                    }
                    x += w;
                }
                return;
            }
            Control::Choices {
                words,
                selected,
                widths: None,
            } => {
                if let Some(word) = selected.and_then(|i| words.get(i)) {
                    label(s, word, mid, true);
                }
            }
            Control::Slider {
                frac,
                text,
                stacked,
            } => {
                let (track, number) = if stacked {
                    (Rect::new(r.x, r.y + button_h, r.w, r.h - button_h), mid)
                } else {
                    let num_w = (self.measure.width(&text, Face::Sans, size * 0.85) + 8.0)
                        .min(mid.w * 0.42);
                    (
                        Rect {
                            w: mid.w - num_w,
                            ..mid
                        },
                        Rect::new(mid.x + mid.w - num_w, mid.y, num_w, mid.h),
                    )
                };
                self.track(s, track, frac, opacity);
                self.text(
                    s,
                    &text,
                    number,
                    Face::Sans,
                    size * 0.85,
                    ink(HI),
                    Align::Center,
                );
                if active {
                    let height = track.h.max(self.ty.tap);
                    s.hit(
                        Rect {
                            y: track.y + track.h - height,
                            h: height,
                            ..track
                        },
                        tag(it, TRACK_TAG),
                    );
                }
            }
        }
        arrow(s, left, Dir::Left);
        arrow(s, right, Dir::Right);
        if active {
            s.hit(left, tag(it, LESS));
            s.hit(right, tag(it, MORE));
        }
    }

    fn track(&self, s: &mut Scene, r: Rect, frac: f32, opacity: f32) {
        let line = Rect::new(r.x + 4.0, r.y + r.h * 0.5 - 1.0, (r.w - 8.0).max(1.0), 2.0);
        let filled = line.w * frac.clamp(0.0, 1.0);
        s.rect(line, TRACK.fade(opacity));
        s.rect(Rect { w: filled, ..line }, FILL.fade(opacity));
        s.round(Round {
            rect: Rect::new(line.x + filled - 4.0, line.y - 3.0, 8.0, 8.0),
            radius: 4.0,
            color: ACCENT.fade(opacity),
            stroke: None,
        });
    }

    fn heading(&self, s: &mut Scene, strip: Strip, y: f32) -> f32 {
        let opacity = if self.model.ask().is_some() { 0.3 } else { 1.0 };
        let word = Lines::new(
            self.measure,
            self.words.ui("paused_title"),
            Face::Serif,
            self.ty.title,
            strip.w,
        );
        let cap = self
            .words
            .heading_caption(&self.metrics.state, self.shape != Shape::Split);
        let cap = Lines::new(
            self.measure,
            &cap,
            Face::Sans,
            self.ty.caption,
            strip.w - 2.0,
        );
        let next = y + word.height();
        let bottom = next + cap.height();
        word.draw(s, strip.x, y, HI, opacity, true);
        cap.draw(s, strip.x + 2.0, next, ACCENT, opacity, true);
        bottom
    }

    fn rows(
        &self,
        s: &mut Scene,
        strip: Strip,
        items: &[Item],
        vertical: Range<f32>,
        quick: bool,
    ) -> (f32, f32) {
        let (top, bottom) = (vertical.start, vertical.end);
        let anchored = self.shape == Shape::Split;
        let make = |k| {
            items
                .iter()
                .map(|&i| Row::new(self, i, strip.w, k, quick))
                .collect::<Vec<_>>()
        };
        let gaps = |rows: &[Row]| -> f32 {
            rows.windows(2)
                .filter(|pair| {
                    pair[0].item == Item::Resume && !anchored || pair[1].item == Item::Retry
                })
                .count() as f32
                * 13.0
        };
        let room = (bottom - top).max(0.0);
        let mut rows = make(1.0);
        for k in [0.92, 0.84, 0.76, 0.68] {
            if rows.iter().map(|r| r.min).sum::<f32>() + gaps(&rows) <= room {
                break;
            }
            rows = make(k);
        }
        let min = rows.iter().map(|r| r.min).sum::<f32>() + gaps(&rows);
        if min > room && self.model.ask().is_some() && !quick {
            return (top, top);
        }
        let base = rows.iter().map(|r| r.base).sum::<f32>() + gaps(&rows);
        let stretch = ((room - min) / (base - min).max(1.0)).clamp(0.0, 1.0);
        let mut y = if anchored {
            (bottom - min - (base - min) * stretch).max(top)
        } else {
            top
        };
        let start = y;
        let len = rows.len();
        for (index, row) in rows.into_iter().enumerate() {
            if (row.item == Item::Retry || quick) && index > 0 {
                if quick {
                    s.rect(strip.at(y, 1.0), RULE);
                } else {
                    s.rect(strip.at(y + 6.0, 1.0), RULE);
                    y += 13.0;
                }
            }
            let divider = row.item == Item::Resume && !anchored && index + 1 < len;
            let h = row.min + (row.base - row.min) * stretch;
            row.draw(self, s, strip, y, h, self.model.ask().is_none());
            y += h;
            if divider {
                s.rect(strip.at(y + 6.0, 1.0), RULE);
                y += 13.0;
            }
        }
        (start, y)
    }

    fn confirm(&self, s: &mut Scene, strip: Strip, ask: Ask, top: f32, bottom: f32) -> (f32, f32) {
        let (question, note, action) = match ask {
            Ask::Retry => ("retry_ask", "retry_note", "retry_do"),
            Ask::Quit => ("quit_ask", "quit_note", "quit_do"),
        };
        let t = &self.ty;
        let line =
            |id, face, size| Lines::new(self.measure, self.words.ui(id), face, size, strip.w);
        let (mut q, mut n, mut keep, mut act) = (
            line(question, Face::Serif, t.row * 1.14),
            line(note, Face::Sans, t.help),
            line("keep_playing", Face::Serif, t.row * 1.07),
            line(action, Face::Serif, t.row * 1.07),
        );
        let want = |l: &Lines| (l.height() + 8.0).max(t.tap).max(t.row_h);
        if q.height() + n.height() + want(&keep) + want(&act) + 14.0 > bottom - top {
            q = line(question, Face::Serif, t.row);
            n = line(note, Face::Sans, t.help * 0.93);
            keep = line("keep_playing", Face::Serif, t.row);
            act = line(action, Face::Serif, t.row);
        }
        let min = |l: &Lines| (l.height() + 8.0).max(t.tap);
        let heads = q.height() + n.height() + 14.0;
        let row_h = t.row_h.min((bottom - top - heads) * 0.5);
        let kh = min(&keep).max(row_h);
        let ah = min(&act).max(row_h);
        let mut y = if self.shape == Shape::Split {
            (bottom - heads - kh - ah).max(top)
        } else {
            let plain: Vec<_> = self
                .model
                .items()
                .into_iter()
                .take_while(|i| !i.danger())
                .collect();
            let row_end = bottom - heads - kh - ah - 14.0;
            let (_, end) = self.rows(s, strip, &plain, top..row_end, false);
            if end > top {
                s.rect(strip.at(end + 5.0, 1.0), RULE);
                end + 14.0
            } else {
                top
            }
        };
        let start = y;
        let ny = y + q.height() + 5.0;
        q.draw(s, strip.x, y, QUESTION, 1.0, true);
        n.draw(s, strip.x, ny, NOTE, 1.0, false);
        y += heads;
        for (yes, text, h, tag) in [(false, keep, kh, KEEP), (true, act, ah, ACCEPT)] {
            let keyed = self.model.bar() && self.model.yes() == yes;
            let lit = self.model.hovered() == Some(Hit::Yes(yes));
            let safe = !yes && self.model.modality() == Modality::Touch;
            let r = strip.band_at(y, h);
            marker(
                s,
                r,
                keyed,
                false,
                lit || safe,
                self.pressed(tag),
                self.shape,
            );
            s.hit(r, tag);
            let ty = y + (h - text.height()) / 2.0;
            text.draw(
                s,
                strip.x,
                ty,
                if keyed || lit || !yes { HI } else { DANGER },
                1.0,
                true,
            );
            y += h;
        }
        (start, y)
    }

    fn footer(&self, s: &mut Scene, strip: Strip, top: f32, bottom: f32) {
        let t = &self.ty;
        let m = &self.metrics.state;
        let make = |text: &str, size| Lines::new(self.measure, text, Face::Sans, size, strip.w);
        let help = make(&sentence(card_help(self.model, &self.words)), t.help);
        let device = make(&self.words.device(m, &self.metrics.lay), t.note);
        let notes: Vec<_> = self
            .words
            .notes(m)
            .iter()
            .map(|n| make(n, t.note))
            .collect();
        let choices = if self.model.ask().is_some() {
            super::Hints::Confirm
        } else {
            super::Hints::Card
        };
        let hints = if self.model.modality() == Modality::Touch {
            Vec::new()
        } else {
            hint_items(&self.words, m, &self.metrics.lay, &choices)
        };
        let cap_h = self.measure.line(Face::Sans, t.hint).height + 4.0;
        let cap_w = |key: &str| match key {
            "Ⓐ" | "Ⓑ" => cap_h,
            "↑↓" | "←→" => t.hint * 2.2,
            _ => self.measure.width(key, Face::Sans, t.hint) + 10.0,
        };
        let mut hx = 0.0;
        let mut hy = 0.0;
        let hints: Vec<_> = hints
            .into_iter()
            .map(|(key, label)| {
                let cw = cap_w(&key);
                let width =
                    (cw + 8.0 + self.measure.width(label, Face::Sans, t.hint) + 16.0).min(strip.w);
                if hx + width > strip.w {
                    hx = 0.0;
                    hy += cap_h + 4.0;
                }
                let at = (key, label, hx, hy, cw, width);
                hx += width;
                at
            })
            .collect();
        let mut heights = [
            if self.shape == Shape::Pillar && !help.lines[0].is_empty() {
                help.height() + 6.0
            } else {
                0.0
            },
            device.height() + 4.0,
            if hints.is_empty() {
                0.0
            } else {
                hy + cap_h + 7.0
            },
            notes.iter().map(|n| n.height() + 4.0).sum(),
        ];
        let mut height = heights.iter().sum::<f32>();
        for h in &mut heights {
            if height <= bottom - top {
                break;
            }
            height -= *h;
            *h = 0.0;
        }
        let mut y = if self.shape == Shape::Pillar {
            bottom - heights.iter().sum::<f32>()
        } else {
            top
        };
        if heights[3] > 0.0 {
            for n in notes {
                let h = n.height();
                n.draw(s, strip.x, y, NOTE, 1.0, false);
                y += h + 4.0;
            }
        }
        if heights[0] > 0.0 {
            help.draw(s, strip.x, y, NOTE, 1.0, false);
            y += heights[0];
        }
        if heights[2] > 0.0 {
            for (key, label, x, dy, cw, width) in hints {
                let x = strip.x + x;
                let r = Rect::new(x, y + dy, cw, cap_h);
                self.keycap(s, &key, r);
                self.text(
                    s,
                    label,
                    Rect::new(x + cw + 8.0, y + dy, (width - cw - 8.0).max(1.0), cap_h),
                    Face::Sans,
                    t.hint,
                    HELP,
                    Align::Left,
                );
            }
            y += heights[2];
        }
        if heights[1] > 0.0 {
            device.draw(s, strip.x, y, HELP, 1.0, false);
        }
    }
    fn keycap(&self, s: &mut Scene, key: &str, r: Rect) {
        let circle = matches!(key, "Ⓐ" | "Ⓑ");
        s.round(Round {
            rect: r,
            radius: if circle { r.h * 0.5 } else { 3.0 },
            color: if circle { KEY } else { HELP },
            stroke: (!circle).then_some(1.0),
        });
        let pair = match key {
            "↑↓" => [Dir::Up, Dir::Down],
            "←→" => [Dir::Left, Dir::Right],
            _ => {
                let inside = match key {
                    "Ⓐ" => "A",
                    "Ⓑ" => "B",
                    k => k,
                };
                self.text(
                    s,
                    inside,
                    r,
                    Face::Sans,
                    self.ty.hint,
                    if circle { PANEL } else { KEY },
                    Align::Center,
                );
                return;
            }
        };
        let size = self.ty.hint * 0.65;
        for (i, dir) in pair.into_iter().enumerate() {
            s.arrow(Arrow {
                x: r.x + (r.w - 3.0 * size) * 0.5 + 2.0 * i as f32 * size,
                y: r.y + (r.h - size) * 0.5,
                size,
                dir,
                color: KEY,
                ring: None,
            });
        }
    }
}

pub(super) fn layout(model: &MenuModel, metrics: &Metrics<'_>, measure: &dyn Measure) -> Scene {
    let g = Geometry::new(metrics);
    let shape = metrics.lay.shape;
    let mut s = Scene::new(metrics.lay.ppp);
    g.backdrop(&mut s, shape);
    let card = Card {
        model,
        metrics,
        measure,
        ty: Typography::new(&metrics.lay),
        words: Words {
            lang: model.store.lang(),
        },
        shape,
    };
    let (above, head_gap, below) = match shape {
        Shape::Pillar if g.bottom - g.top < 680.0 => (20.0, 12.0, 12.0),
        Shape::Pillar => (44.0, 28.0, 24.0),
        Shape::Column => (24.0, 10.0, 1.0),
        Shape::Split => (32.0, 20.0, 40.0),
    };
    let y = card.heading(&mut s, g.left, g.top + above) + head_gap;
    let bottom = g.bottom - below;
    let items = model.items();
    if shape == Shape::Split {
        let (run, quick) = items.split_at(
            items
                .iter()
                .take_while(|i| matches!(i, Item::Resume | Item::Settings) || i.danger())
                .count(),
        );
        let quick_cap = Lines::new(
            measure,
            card.words.ui("quick_settings"),
            Face::Serif,
            card.ty.caption,
            g.right.w,
        );
        let cap_h = quick_cap.height();
        let quick_min: f32 = quick
            .iter()
            .map(|&i| Row::new(&card, i, g.right.w, 1.0, true).base)
            .sum();
        let quick_y = (bottom - quick_min - cap_h - 18.0).max(g.top + 20.0);
        quick_cap.draw(
            &mut s,
            g.right.x,
            quick_y,
            ACCENT,
            if model.ask().is_some() { 0.3 } else { 1.0 },
            true,
        );
        s.rect(g.right.at(quick_y + cap_h + 7.0, 1.0), RULE);
        card.rows(&mut s, g.right, quick, quick_y + cap_h + 18.0..bottom, true);
        let (left_space, _) = match model.ask() {
            Some(ask) => card.confirm(&mut s, g.left, ask, y, bottom),
            None => card.rows(&mut s, g.left, run, y..bottom, false),
        };
        card.footer(&mut s, g.left, y, left_space - 12.0);
    } else {
        let (_, end) = if let Some(ask) = model.ask() {
            card.confirm(&mut s, g.left, ask, y + 12.0, bottom)
        } else {
            card.rows(&mut s, g.left, &items, y..bottom, false)
        };
        card.footer(&mut s, g.left, end + 10.0, bottom);
    }
    s
}
