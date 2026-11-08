//! Layout check of an egui frame (launcher, port menu): clipped or overlapping text, stock egui visuals, old colours.

use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Rect};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Clipped,
    Overlap,
    MidWord,
    Glued,
    StockVisuals,
    OldColour,
    RawId,
    FewRows,
    NoFocus,
    TwoFocus,
    Unreachable,
}

impl Kind {
    pub const ALL: [Kind; 11] = [
        Kind::Clipped,
        Kind::Overlap,
        Kind::MidWord,
        Kind::Glued,
        Kind::StockVisuals,
        Kind::OldColour,
        Kind::RawId,
        Kind::FewRows,
        Kind::NoFocus,
        Kind::TwoFocus,
        Kind::Unreachable,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Clipped => "clipped",
            Kind::Overlap => "overlap",
            Kind::MidWord => "mid-word-break",
            Kind::Glued => "glued-words",
            Kind::StockVisuals => "stock-visuals",
            Kind::OldColour => "old-colour",
            Kind::RawId => "raw-identifier",
            Kind::FewRows => "few-rows",
            Kind::NoFocus => "no-focus-at-rest",
            Kind::TwoFocus => "two-focus-markers",
            Kind::Unreachable => "unreachable-row",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Issue {
    pub kind: Kind,
    pub what: String,
    pub rect: Rect,
}

// Style slots whose stock egui look the port never shows; a shape painted in one of them is a stock widget.
// Slots the theme leaves transparent paint nothing and stay unmarked.
const SLOTS: [&str; 13] = [
    "inactive.bg_fill",
    "inactive.weak_bg_fill",
    "inactive.bg_stroke",
    "hovered.bg_fill",
    "hovered.weak_bg_fill",
    "hovered.bg_stroke",
    "active.bg_fill",
    "active.weak_bg_fill",
    "active.bg_stroke",
    "open.bg_fill",
    "open.weak_bg_fill",
    "selection.bg_fill",
    "faint_bg_color",
];

fn sentinel(i: usize) -> Color32 {
    Color32::from_rgb(0xff, 0x01, 0x10 + u8::try_from(i).unwrap_or(0) * 7)
}

fn slot_of(c: Color32) -> Option<&'static str> {
    (0..SLOTS.len())
        .find(|&i| sentinel(i) == c)
        .map(|i| SLOTS[i])
}

// The style of `ctx` with a marker colour in every visible stock slot (only for the check's own context).
pub fn mark_stock_slots(ctx: &egui::Context) {
    ctx.all_styles_mut(|st| {
        let v = &mut st.visuals;
        let mark = |c: &mut Color32, i: usize| {
            if *c != Color32::TRANSPARENT {
                *c = sentinel(i);
            }
        };
        let w = &mut v.widgets;
        for (k, x) in [&mut w.inactive, &mut w.hovered, &mut w.active]
            .into_iter()
            .enumerate()
        {
            mark(&mut x.bg_fill, 3 * k);
            mark(&mut x.weak_bg_fill, 3 * k + 1);
            if x.bg_stroke.width > 0.0 {
                mark(&mut x.bg_stroke.color, 3 * k + 2);
            }
        }
        mark(&mut w.open.bg_fill, 9);
        mark(&mut w.open.weak_bg_fill, 10);
        mark(&mut v.selection.bg_fill, 11);
        mark(&mut v.faint_bg_color, 12);
    });
}

// The pre-lacquer night-sky palette and egui's own dark-theme colours.
const OLD: [(Color32, &str); 18] = [
    (Color32::from_rgb(10, 11, 30), "night BG"),
    (Color32::from_rgb(20, 20, 48), "night SURFACE"),
    (Color32::from_rgb(26, 26, 58), "night STRIPE"),
    (Color32::from_rgb(34, 34, 76), "night RAISED"),
    (Color32::from_rgb(46, 46, 98), "night RAISED_HOVER"),
    (Color32::from_rgb(112, 112, 170), "night OUTLINE"),
    (Color32::from_rgb(238, 236, 248), "night TEXT"),
    (Color32::from_rgb(176, 174, 206), "night TEXT_WEAK"),
    (Color32::from_rgb(100, 82, 220), "night ACCENT"),
    (Color32::from_rgb(245, 214, 128), "night FOCUS"),
    (Color32::from_rgb(255, 138, 128), "night DANGER"),
    (Color32::from_rgb(150, 176, 255), "night INFO"),
    (Color32::from_rgb(0, 92, 128), "egui selection"),
    (Color32::from_rgb(60, 60, 60), "egui widget"),
    (Color32::from_rgb(70, 70, 70), "egui hovered"),
    (Color32::from_rgb(27, 27, 27), "egui window"),
    (Color32::from_rgb(90, 170, 255), "egui hyperlink"),
    (Color32::from_rgb(140, 140, 140), "egui text"),
];

fn old_of(c: Color32) -> Option<&'static str> {
    let rgb = |c: Color32| [c.r(), c.g(), c.b()];
    (c.a() == 255)
        .then(|| OLD.iter().find(|(o, _)| rgb(*o) == rgb(c)).map(|(_, n)| *n))
        .flatten()
}

// A scroll area of the current pass: its viewport and the space it was given. Text cut at the viewport's edge is
// scrolled content, not a clipped label, as long as the viewport fills its space.
#[derive(Clone, Copy, Debug)]
pub struct Scroll {
    pub viewport: Rect,
    pub space: Rect,
    // The content is taller than the viewport.
    pub more: bool,
    // Scrolled to its last line.
    pub at_end: bool,
}

pub fn note_scroll<R>(
    ctx: &egui::Context,
    out: &egui::scroll_area::ScrollAreaOutput<R>,
    space: Rect,
) {
    let viewport = out.inner_rect;
    let content_h = out.content_size.y;
    let more = content_h > viewport.height() + 0.5;
    let at_end = out.state.clamped_offset().y + viewport.height() >= content_h - 1.0;
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let v = d.get_temp_mut_or_default::<(u64, Vec<Scroll>)>(egui::Id::unique("uicheck scroll"));
        if v.0 != pass {
            *v = (pass, Vec::new());
        }
        v.1.push(Scroll {
            viewport,
            space,
            more,
            at_end,
        });
    });
}

// The scroll areas noted in the last pass that ran.
pub fn scrolls(ctx: &egui::Context) -> Vec<Scroll> {
    let last = ctx.cumulative_pass_nr().saturating_sub(1);
    ctx.data(|d| d.get_temp::<(u64, Vec<Scroll>)>(egui::Id::unique("uicheck scroll")))
        .filter(|v| v.0 == last)
        .map(|v| v.1)
        .unwrap_or_default()
}

// The rows of a paged list shown this pass (index, rect) and how many it has.
pub fn note_rows(ctx: &egui::Context, shown: Vec<(usize, Rect)>, total: usize) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("uicheck rows"), (shown, total)));
}

pub fn rows(ctx: &egui::Context) -> (Vec<(usize, Rect)>, usize) {
    ctx.data(|d| d.get_temp(egui::Id::unique("uicheck rows")))
        .unwrap_or_default()
}

// What a finger sends, as egui-winit reports it: the touch event and the pointer events it emulates.
pub fn finger(phase: egui::TouchPhase, pos: egui::Pos2) -> Vec<egui::Event> {
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let touch = egui::Event::Touch {
        device_id: egui::TouchDeviceId(0),
        id: egui::TouchId(0),
        phase,
        pos,
        force: None,
    };
    match phase {
        egui::TouchPhase::Start => vec![touch, egui::Event::PointerMoved(pos), button(true)],
        egui::TouchPhase::Move => vec![touch, egui::Event::PointerMoved(pos)],
        _ => vec![touch, button(false), egui::Event::PointerGone],
    }
}

// One swipe of `dist` points up from `from`, as the events of each pass from `k`: `None` when it is over.
pub fn swipe(k: u32, from: egui::Pos2, dist: f32) -> Option<Vec<egui::Event>> {
    const MOVES: u32 = 8;
    let at = |i: u32| from - egui::vec2(0.0, dist * i as f32 / MOVES as f32);
    Some(match k {
        0 => finger(egui::TouchPhase::Start, from),
        1..=MOVES => finger(egui::TouchPhase::Move, at(k)),
        9 => finger(egui::TouchPhase::End, at(MOVES)),
        10..=40 => Vec::new(),
        _ => return None,
    })
}

// The shapes of a pass whose textures nobody uploads.
pub fn shapes(mut out: egui::FullOutput) -> Vec<ClippedShape> {
    out.textures_delta.clear();
    out.shapes
}

struct Text {
    ink: Rect,
    clip: Rect,
    text: String,
    galley: *const egui::Galley,
    // A wrapped row ends inside a word: the two letters around the break.
    split: Option<(char, char)>,
}

// Scripts that wrap between any two characters (CJK ideographs, kana, full-width forms).
fn breaks_anywhere(c: char) -> bool {
    matches!(u32::from(c), 0x2e80..=0x9fff | 0xf900..=0xfaff | 0xff00..=0xffef | 0x20000..=0x3ffff)
}

fn mid_word(g: &egui::Galley) -> Option<(char, char)> {
    let latin = g.rows.windows(2).find_map(|w| {
        let a = w[0].row.glyphs.last()?.chr;
        let b = w[1].row.glyphs.first()?.chr;
        (!w[0].ends_with_newline
            && a.is_alphanumeric()
            && b.is_alphanumeric()
            && !breaks_anywhere(a)
            && !breaks_anywhere(b))
        .then_some((a, b))
    });
    latin.or_else(|| ja_mid_phrase(g))
}

// A wrapped row of Japanese that ends inside a phrase (`theme::pieces`) or against kinsoku.
fn ja_mid_phrase(g: &egui::Galley) -> Option<(char, char)> {
    let text = &g.job.text;
    if !text.chars().any(crate::theme::is_ja) {
        return None;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut cuts = std::collections::HashSet::new();
    let mut at = 0;
    for piece in crate::theme::pieces(text) {
        at += piece.chars().count();
        cuts.insert(at);
    }
    let mut idx = 0;
    for w in g.rows.windows(2) {
        idx += w[0].row.glyphs.len() + usize::from(w[0].ends_with_newline);
        if w[0].ends_with_newline {
            continue;
        }
        let (Some(a), Some(b)) = (w[0].row.glyphs.last(), w[1].row.glyphs.first()) else {
            continue;
        };
        let (a, b) = (a.chr, b.chr);
        if chars.get(idx) != Some(&b) || a.is_whitespace() || b.is_whitespace() {
            continue;
        }
        if (crate::theme::is_ja(a) || crate::theme::is_ja(b)) && !cuts.contains(&idx) {
            return Some((a, b));
        }
    }
    None
}

// A code identifier in visible text: snake_case or a --flag; file names and paths are real text.
fn raw_id(text: &str) -> Option<&str> {
    text.split_whitespace().find_map(|w| {
        let t = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
        let snake = t.contains('_')
            && t.split('_').all(|p| {
                !p.is_empty()
                    && p.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            });
        let flag = t.starts_with("--") && t.len() > 3;
        (!w.contains(['/', '\\']) && (snake || flag)).then_some(t)
    })
}

fn short(s: &str) -> String {
    let s = s.replace('\n', "⏎");
    if s.chars().count() > 40 {
        format!("{}…", s.chars().take(40).collect::<String>())
    } else {
        s
    }
}

// Painted marks that must not sit on text: arrows, chevrons, knobs, slider tracks.
fn mark_rect(s: &Shape) -> Option<Rect> {
    match s {
        Shape::Path(p) if p.fill.a() > 0 || p.stroke.width > 0.0 => Some(p.visual_bounding_rect()),
        Shape::Circle(c) => Some(c.visual_bounding_rect()),
        Shape::LineSegment { points, stroke }
            if stroke.width > 0.0 && points[0].y == points[1].y =>
        {
            Some(
                Rect::from_two_pos(points[0], points[1])
                    .expand2(egui::vec2(0.0, stroke.width / 2.0)),
            )
        }
        _ => None,
    }
}

fn walk(
    s: &Shape,
    clip: Rect,
    texts: &mut Vec<Text>,
    colour: &mut dyn FnMut(Color32, Rect, &str),
    marks: &mut Vec<Rect>,
) {
    if let Some(r) = mark_rect(s).filter(|r| r.intersects(clip)) {
        marks.push(r.intersect(clip));
    }
    match s {
        Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, texts, colour, marks)),
        Shape::Text(t) => {
            let r = t.galley.mesh_bounds.translate(t.pos.to_vec2());
            if let Some(c) = t.override_text_color {
                colour(c, r, &t.galley.job.text);
            } else {
                for sec in &t.galley.job.sections {
                    colour(sec.format.color, r, &t.galley.job.text);
                    colour(sec.format.background, r, &t.galley.job.text);
                }
            }
            if !t.galley.job.text.trim().is_empty() && r.is_positive() {
                texts.push(Text {
                    ink: r,
                    clip,
                    text: t.galley.job.text.clone(),
                    galley: Arc::as_ptr(&t.galley),
                    split: mid_word(&t.galley),
                });
            }
        }
        Shape::Rect(r) => {
            colour(r.fill, r.rect, "rect fill");
            if r.stroke.width > 0.0 {
                colour(r.stroke.color, r.rect, "rect stroke");
            }
        }
        Shape::Circle(c) => {
            colour(c.fill, c.visual_bounding_rect(), "circle");
            colour(c.stroke.color, c.visual_bounding_rect(), "circle");
        }
        Shape::LineSegment { points, stroke } => {
            colour(
                stroke.color,
                Rect::from_two_pos(points[0], points[1]),
                "line",
            );
        }
        Shape::Path(p) => {
            colour(p.fill, p.visual_bounding_rect(), "path");
            if let egui::epaint::ColorMode::Solid(c) = p.stroke.color {
                colour(c, p.visual_bounding_rect(), "path");
            }
        }
        _ => {}
    }
}

// A shape of `r` cut by `clip` (smaller than the shape's own extent on neither axis): a control cut by a
// fixed edge such as a footer.
const TOL: f32 = 1.0;

fn cut(r: Rect, clip: Rect, screen: Rect, scrolls: &[Scroll]) -> bool {
    if !r.intersects(clip)
        || clip.expand(TOL).contains_rect(r)
        || clip.expand(TOL).contains_rect(screen)
    {
        return false;
    }
    if scrolls
        .iter()
        .any(|v| v.viewport.expand(TOL).contains_rect(clip))
    {
        return false;
    }
    let y =
        (r.min.y < clip.min.y - TOL || r.max.y > clip.max.y + TOL) && r.height() < clip.height();
    let x = (r.min.x < clip.min.x - TOL || r.max.x > clip.max.x + TOL) && r.width() < clip.width();
    y || x
}

// All issues of one frame. `screen` is the window in points; `scrolls` the scroll viewports of the pass.
#[expect(
    clippy::too_many_lines,
    reason = "one pass over the frame, one rule per block"
)]
pub fn check(shapes: &[ClippedShape], screen: Rect, scrolls: &[Scroll]) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut texts = Vec::new();
    let mut marks = Vec::new();
    let mut ticks = Vec::new();
    for cs in shapes {
        let clip = cs.clip_rect.intersect(screen);
        let mut colour = |c: Color32, r: Rect, what: &str| {
            if c.a() == 0 || !r.intersects(clip) {
                return;
            }
            if let Some(slot) = slot_of(c) {
                out.push(Issue {
                    kind: Kind::StockVisuals,
                    what: format!("{slot} on {}", short(what)),
                    rect: r,
                });
            } else if let Some(name) = old_of(c) {
                out.push(Issue {
                    kind: Kind::OldColour,
                    what: format!("{name} on {}", short(what)),
                    rect: r,
                });
            }
        };
        walk(&cs.shape, clip, &mut texts, &mut colour, &mut marks);
        // The focus tick: a thin upright lavender bar (`theme::tick`).
        if let Shape::Rect(r) = &cs.shape
            && r.fill == crate::theme::ACCENT
            && r.rect.width() <= 8.0
            && r.rect.height() >= 2.0 * r.rect.width()
            && r.rect.intersect(clip).is_positive()
        {
            ticks.push(r.rect);
        }
        if let Shape::Rect(r) = &cs.shape
            && r.fill.a() > 0
            && cut(r.rect, cs.clip_rect, screen, scrolls)
        {
            out.push(Issue {
                kind: Kind::Clipped,
                what: format!("box {:?} cut by {:?}", r.rect, cs.clip_rect),
                rect: r.rect,
            });
        }
    }
    if ticks.len() > 1 {
        out.push(Issue {
            kind: Kind::TwoFocus,
            what: format!("{} focus markers: {ticks:?}", ticks.len()),
            rect: ticks[1],
        });
    }
    // Outline / shadow / glow copies (the same galley, or the same text within half its height): the last copy
    // (the coloured one) stands for all.
    let mut group: Vec<usize> = Vec::with_capacity(texts.len());
    let mut seen = std::collections::HashMap::new();
    for (i, t) in texts.iter().enumerate() {
        let twin = texts[..i].iter().position(|o| {
            o.galley == t.galley
                || (o.text == t.text
                    && (o.ink.min - t.ink.min).abs().max_elem() <= 3.0f32.max(0.5 * t.ink.height()))
        });
        let g = twin.map_or(i, |j| group[j]);
        group.push(g);
        seen.insert(g, i);
    }
    let mut keep: Vec<usize> = seen.into_values().collect();
    keep.sort_unstable();
    let texts: Vec<&Text> = keep.iter().map(|&i| &texts[i]).collect();
    let mut shown = Vec::new();
    for t in &texts {
        let vis = t.ink.intersect(t.clip);
        if !vis.is_positive() {
            continue;
        }
        shown.push((*t, vis));
        if let Some(id) = raw_id(&t.text) {
            out.push(Issue {
                kind: Kind::RawId,
                what: format!("{id} in “{}”", short(&t.text)),
                rect: t.ink,
            });
        }
        if let Some((a, b)) = t.split {
            out.push(Issue {
                kind: Kind::MidWord,
                what: format!("“{}” breaks between {a} and {b}", short(&t.text)),
                rect: t.ink,
            });
        }
        if t.clip.expand(TOL).contains_rect(t.ink) {
            continue;
        }
        // Cut by a scroll viewport's edge while inside it horizontally: scrolled content.
        // Cut only at the top / bottom edge of a scroll viewport (inside it across): scrolled content. Cut by
        // any other clip (a parent that is shorter than the viewport, a fixed footer) is a clipped label.
        let scrolled = scrolls.iter().any(|s| {
            let v = &s.viewport;
            // A viewport ending short of its space cuts rows it had room to show.
            let fills = v.max.y >= s.space.max.y - 4.0;
            let top_ok = t.ink.min.y >= t.clip.min.y - TOL || (t.clip.min.y - v.min.y).abs() <= TOL;
            let bottom_ok = t.ink.max.y <= t.clip.max.y + TOL
                || ((t.clip.max.y - v.max.y).abs() <= TOL && fills);
            v.expand(TOL).contains_rect(t.clip)
                && top_ok
                && bottom_ok
                && v.expand(TOL).x_range().contains(t.ink.min.x)
                && t.ink.max.x <= v.max.x + TOL
                && screen.expand(TOL).contains_rect(*v)
        });
        if !scrolled {
            out.push(Issue {
                kind: Kind::Clipped,
                what: format!("“{}” cut by {:?}", short(&t.text), t.clip),
                rect: t.ink,
            });
        }
    }
    // A scrolling list must show at least two whole lines of its content.
    for s in scrolls.iter().filter(|s| s.more) {
        let mut ys: Vec<f32> = shown
            .iter()
            .filter(|(t, _)| s.viewport.expand(TOL).contains_rect(t.ink))
            .flat_map(|(t, _)| {
                let n = t.text.lines().count().max(1);
                let h = t.ink.height() / n as f32;
                (0..n).map(move |i| t.ink.min.y + h * (i as f32 + 0.5))
            })
            .collect();
        ys.sort_by(f32::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 8.0);
        if ys.len() < 2 {
            out.push(Issue {
                kind: Kind::FewRows,
                what: format!("{} whole line(s) in {:?}", ys.len(), s.viewport),
                rect: s.viewport,
            });
        }
    }
    for (t, r) in &shown {
        // A mark crossing the middle band of the text's ink (underlines and rules sit outside it).
        let core = r.shrink2(egui::vec2(0.0, r.height() * 0.2));
        if let Some(m) = marks
            .iter()
            .find(|m| m.intersect(core).width() > 1.0 && m.intersect(core).height() > 0.5)
        {
            out.push(Issue {
                kind: Kind::Overlap,
                what: format!("a mark at {:?} over “{}”", m.min, short(&t.text)),
                rect: m.intersect(*r),
            });
        }
    }
    for (i, (a, ra)) in shown.iter().enumerate() {
        for (b, rb) in &shown[i + 1..] {
            let x = ra.intersect(*rb);
            let h = ra.height().min(rb.height());
            if x.width() > 1.5 && x.height() > 1.0 {
                out.push(Issue {
                    kind: Kind::Overlap,
                    what: format!("“{}” over “{}”", short(&a.text), short(&b.text)),
                    rect: x,
                });
                continue;
            }
            // Two words on one line, nearly touching: a gap under half the line height.
            let same_line = ra.y_range().intersection(rb.y_range()).span() > 0.6 * h;
            let gap = (rb.min.x - ra.max.x).max(ra.min.x - rb.max.x);
            let wordy = |t: &str| t.chars().filter(|c| c.is_alphanumeric()).count() >= 2;
            if same_line
                && (-1.5..0.5 * h).contains(&gap)
                && ra.height().max(rb.height()) < 1.25 * h
                && wordy(&a.text)
                && wordy(&b.text)
            {
                out.push(Issue {
                    kind: Kind::Glued,
                    what: format!("“{}” against “{}”", short(&a.text), short(&b.text)),
                    rect: ra.union(*rb),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, pos2, vec2};

    fn frame(f: impl FnMut(&egui::Context)) -> Vec<Issue> {
        let ctx = egui::Context::default();
        mark_stock_slots(&ctx);
        let raw = |t: f64| egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
            time: Some(t),
            ..Default::default()
        };
        let mut f = f;
        for i in 0..5 {
            shapes(ctx.run_pass(raw(f64::from(i) / 5.0), &mut f));
        }
        let out = shapes(ctx.run_pass(raw(1.0), &mut f));
        check(
            &out,
            Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0)),
            &[],
        )
    }

    fn kinds(v: &[Issue]) -> Vec<Kind> {
        let mut k: Vec<Kind> = v.iter().map(|i| i.kind).collect();
        k.dedup();
        k
    }

    #[test]
    fn finds_each_kind() {
        let font = egui::FontId::proportional(16.0);
        let v = frame(|ctx| {
            let p = ctx.layer_painter(egui::LayerId::background());
            p.with_clip_rect(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 10.0)))
                .text(
                    pos2(10.0, 0.0),
                    egui::Align2::LEFT_TOP,
                    "cut",
                    font.clone(),
                    Color32::WHITE,
                );
        });
        assert_eq!(kinds(&v), [Kind::Clipped]);
        let v = frame(|ctx| {
            let p = ctx.layer_painter(egui::LayerId::background());
            p.text(
                pos2(10.0, 50.0),
                egui::Align2::LEFT_TOP,
                "one",
                font.clone(),
                Color32::WHITE,
            );
            p.text(
                pos2(14.0, 52.0),
                egui::Align2::LEFT_TOP,
                "two",
                font.clone(),
                Color32::WHITE,
            );
        });
        assert_eq!(kinds(&v), [Kind::Overlap]);
        let v = frame(|ctx| {
            egui::Area::new(egui::Id::unique("a")).show(ctx, |ui| {
                let _ = ui.button("stock");
            });
        });
        assert!(kinds(&v).contains(&Kind::StockVisuals), "{v:?}");
        let v = frame(|ctx| {
            ctx.layer_painter(egui::LayerId::background()).rect_filled(
                Rect::from_min_size(pos2(10.0, 10.0), vec2(20.0, 20.0)),
                0.0,
                Color32::from_rgb(100, 82, 220),
            );
        });
        assert_eq!(kinds(&v), [Kind::OldColour]);
        let v = frame(|ctx| {
            let p = ctx.layer_painter(egui::LayerId::background());
            let g = p.layout("масштабирования".into(), font.clone(), Color32::WHITE, 60.0);
            p.galley(pos2(10.0, 50.0), g, Color32::WHITE);
        });
        assert_eq!(kinds(&v), [Kind::MidWord]);
        let v = frame(|ctx| {
            let p = ctx.layer_painter(egui::LayerId::background());
            let r = p.text(
                pos2(10.0, 50.0),
                egui::Align2::LEFT_TOP,
                "Сглаженный",
                font.clone(),
                Color32::WHITE,
            );
            p.text(
                pos2(r.max.x + 1.0, 50.0),
                egui::Align2::LEFT_TOP,
                "Чёткий",
                font.clone(),
                Color32::WHITE,
            );
        });
        assert_eq!(kinds(&v), [Kind::Glued]);
        let v = frame(|ctx| {
            ctx.layer_painter(egui::LayerId::background())
                .with_clip_rect(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 100.0)))
                .rect_filled(
                    Rect::from_min_size(pos2(10.0, 80.0), vec2(40.0, 40.0)),
                    0.0,
                    Color32::from_rgb(40, 20, 20),
                );
        });
        assert_eq!(kinds(&v), [Kind::Clipped]);
        let v = frame(|ctx| {
            let p = ctx.layer_painter(egui::LayerId::background());
            p.text(
                pos2(10.0, 50.0),
                egui::Align2::LEFT_TOP,
                "fine",
                font.clone(),
                Color32::WHITE,
            );
        });
        assert!(v.is_empty(), "{v:?}");
        let tick = |ctx: &egui::Context, y: f32| {
            ctx.layer_painter(egui::LayerId::background()).rect_filled(
                Rect::from_min_size(pos2(10.0, y), vec2(4.0, 30.0)),
                0.0,
                crate::theme::ACCENT,
            );
        };
        assert!(frame(|ctx| tick(ctx, 10.0)).is_empty());
        let v = frame(|ctx| {
            tick(ctx, 10.0);
            tick(ctx, 80.0);
        });
        assert_eq!(kinds(&v), [Kind::TwoFocus]);
    }

    #[test]
    fn raw_identifiers() {
        assert_eq!(raw_id("off or high_contrast."), Some("high_contrast"));
        assert_eq!(raw_id("see --stage first"), Some("--stage"));
        assert_eq!(raw_id("th8_long_name.rpy is damaged"), None);
        assert_eq!(raw_id("/tmp/some_dir and text"), None);
    }

    #[test]
    fn japanese_breaks_between_phrases() {
        let p = crate::theme::pieces("設定はそのままです");
        assert!(p.iter().all(|c| !c.ends_with("そ")), "{p:?}");
        let p = crate::theme::pieces("「はい」を選ぶ。次へ");
        assert!(
            p.iter().all(|c| !c.ends_with('「') && !c.starts_with('」')),
            "{p:?}"
        );
        assert_eq!(crate::theme::pieces("plain words"), ["plain ", "words"]);
    }

    #[test]
    fn scrolled_text_is_not_clipped() {
        let font = egui::FontId::proportional(16.0);
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
            ..Default::default()
        };
        let view = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 10.0));
        let out = shapes(ctx.run_pass(raw, |ctx| {
            ctx.layer_painter(egui::LayerId::background())
                .with_clip_rect(view)
                .text(
                    pos2(10.0, 0.0),
                    egui::Align2::LEFT_TOP,
                    "cut",
                    font.clone(),
                    Color32::WHITE,
                );
        }));
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
        let s = Scroll {
            viewport: view,
            space: view,
            more: false,
            at_end: true,
        };
        assert!(check(&out, screen, &[s]).is_empty());
        // The same viewport given twice the height it uses: the cut row had room.
        let short = Scroll {
            viewport: view,
            space: view.with_max_y(40.0),
            more: false,
            at_end: true,
        };
        assert_eq!(kinds(&check(&out, screen, &[short])), [Kind::Clipped]);
    }
}
