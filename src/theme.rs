//! The port UI's design system (launcher, port menu, touch-layout editor): lacquer palette, serif face, type and spacing scales, egui style, painted widgets.

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Galley, Margin, Painter, Pos2, Rect, Response,
    RichText, Shadow, Stroke, TextStyle, Ui, pos2, vec2,
};
use std::sync::Arc;

// Lacquer panel over the dimmed game, one lavender accent. `tests::palette_contrast` holds the text pairs to WCAG.
pub const PANEL: Color32 = Color32::from_rgb(0x17, 0x0c, 0x0d);
pub const EDGE: Color32 = Color32::from_rgb(0x5a, 0x40, 0x38);
pub const EDGE_IN: Color32 = Color32::from_rgb(0x33, 0x22, 0x1f);
pub const RULE: Color32 = Color32::from_rgba_premultiplied(72, 51, 45, 204);
pub const BAND: Color32 = Color32::from_rgb(0x2c, 0x18, 0x1b);
pub const PRESSED: Color32 = Color32::from_rgb(0x3a, 0x22, 0x26);
pub const CELL: Color32 = Color32::from_rgb(0x22, 0x12, 0x14);
pub const TRACK: Color32 = Color32::from_rgb(0x4a, 0x3a, 0x3f);
pub const TEXT_HI: Color32 = Color32::from_rgb(0xfc, 0xfc, 0xfc);
pub const TEXT: Color32 = Color32::from_rgb(0xc4, 0xbc, 0xcb);
pub const HELP: Color32 = Color32::from_rgb(0x9a, 0x93, 0xa6);
pub const HELP_HI: Color32 = Color32::from_rgb(0xb6, 0xae, 0xbf);
pub const NOTE: Color32 = Color32::from_rgb(0x8c, 0x86, 0x98);
pub const DIM: Color32 = Color32::from_rgb(0x6c, 0x64, 0x74);
pub const DISABLED: Color32 = Color32::from_rgb(0x5f, 0x58, 0x66);
pub const ACCENT: Color32 = Color32::from_rgb(0xca, 0xc2, 0xd1);
pub const GLOW: Color32 = Color32::from_rgb(0x3c, 0x00, 0x30);
pub const FILL: Color32 = Color32::from_rgb(0x96, 0x6c, 0xa8);
pub const DANGER: Color32 = Color32::from_rgb(0xec, 0xc0, 0xc3);
pub const QUESTION: Color32 = Color32::from_rgb(0xf6, 0xc1, 0xc1);
pub const WARN: Color32 = Color32::from_rgb(0xe8, 0xcf, 0xb4);
pub const SUCCESS: Color32 = HELP_HI;
pub const INFO: Color32 = ACCENT;
pub const KEYCAP: Color32 = Color32::from_rgb(0xec, 0xe6, 0xf0);

// Older names the launcher tabs and the layout editor use.
pub const BG: Color32 = Color32::from_rgb(0x0e, 0x08, 0x09);
pub const SURFACE: Color32 = PANEL;
pub const STRIPE: Color32 = CELL;
pub const RAISED: Color32 = CELL;
pub const RAISED_HOVER: Color32 = PRESSED;
pub const OUTLINE: Color32 = EDGE;
pub const TEXT_WEAK: Color32 = HELP;
pub const FOCUS: Color32 = ACCENT;

// Dims the game picture under the menu (16:9 and phone), and the 4:3 playfield behind the list.
pub const WASH: Color32 = Color32::from_rgba_premultiplied(3, 0, 1, 128);
pub const WASH_DEEP: Color32 = Color32::from_rgba_premultiplied(4, 1, 2, 184);
pub const SCRIM: Color32 = WASH;

// Type scale, points (= dp on touch screens) for the egui-flow screens.
pub const SMALL: f32 = 14.0;
pub const BODY: f32 = 16.0;
pub const TITLE: f32 = 20.0;
pub const HEADING: f32 = 24.0;

// Spacing scale, points (4 px base).
pub const XS: f32 = 4.0;
pub const S: f32 = 8.0;
pub const M: f32 = 12.0;
pub const L: f32 = 16.0;
pub const XL: f32 = 24.0;

pub const RADIUS: u8 = 0;
pub const RADIUS_PANEL: u8 = 0;

// The menu face: Noto Serif CJK JP SemiBold, cut to the glyphs of the UI tables (`tools/i18n_font_subset`).
pub const SERIF_FONT: &[u8] = include_bytes!("../i18n/fonts/NotoSerifJP-ui.otf");

pub fn serif_family() -> FontFamily {
    FontFamily::Name("serif".into())
}

pub fn serif(size: f32) -> FontId {
    FontId::new(size, serif_family())
}

pub fn sans(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

// Adds the serif family to `fonts`: the serif face first, then the proportional fallbacks (CJK, emoji).
pub fn add_serif(fonts: &mut egui::FontDefinitions) {
    fonts.font_data.insert(
        "serif".into(),
        Arc::new(egui::FontData::from_static(SERIF_FONT)),
    );
    let mut fam = vec!["serif".to_string()];
    fam.extend(
        fonts
            .families
            .get(&FontFamily::Proportional)
            .into_iter()
            .flatten()
            .cloned(),
    );
    fonts.families.insert(serif_family(), fam);
}

// A desktop window this many times 640×480 (in OS points) shows the UI at the OS scale; bigger windows grow it.
const DESKTOP_FIT: f32 = 1.75;
const DESKTOP_MAX_GROW: f32 = 1.5;

// Window px per point for a `sw`×`sh` px area. Touch screens: the display density (dp) or the 640×480 fit when
// bigger. Desktop: the OS scale, growing with the window up to 1.5×, shrinking only for windows under 640×480 pt.
pub fn ppp(sw: f32, sh: f32, density: f32, touch: bool) -> f32 {
    let fit = (sw / 640.0).min(sh / 480.0);
    if touch {
        return fit.max(0.5).max(density);
    }
    let grow = (fit / (DESKTOP_FIT * density)).clamp(1.0, DESKTOP_MAX_GROW);
    (density * grow).min(fit).max(0.5)
}

// Widget size and input for one screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Density {
    // Minimum target height, points (48 = 48 dp on touch screens).
    pub target: f32,
    pub touch: bool,
}

impl Density {
    pub fn new(touch: bool) -> Self {
        Self {
            target: if touch { 48.0 } else { 32.0 },
            touch,
        }
    }
}

// The palette, scales and widget looks on every style of `ctx`.
pub fn apply(ctx: &egui::Context, d: Density) {
    ctx.all_styles_mut(|st| {
        st.text_styles = [
            (TextStyle::Heading, serif(HEADING)),
            (TextStyle::Body, sans(BODY)),
            (TextStyle::Button, serif(BODY + 1.0)),
            (TextStyle::Small, sans(SMALL)),
            (
                TextStyle::Monospace,
                FontId::new(SMALL, FontFamily::Monospace),
            ),
        ]
        .into();
        let sp = &mut st.spacing;
        sp.interact_size = vec2(40.0, d.target);
        sp.item_spacing = vec2(S, if d.touch { 6.0 } else { S });
        sp.button_padding = vec2(M, XS);
        sp.window_margin = Margin::same(L as i8);
        sp.menu_margin = Margin::same(S as i8);
        sp.indent = L;
        sp.icon_width = if d.touch { 28.0 } else { 20.0 };
        sp.icon_width_inner = sp.icon_width * 0.6;
        sp.icon_spacing = S;
        sp.slider_rail_height = 3.0;
        sp.combo_width = 180.0;
        sp.text_edit_width = 240.0;
        let v = &mut st.visuals;
        v.dark_mode = true;
        v.override_text_color = None;
        v.weak_text_color = Some(HELP);
        v.hyperlink_color = ACCENT;
        v.window_fill = PANEL;
        v.panel_fill = PANEL;
        v.faint_bg_color = CELL;
        v.extreme_bg_color = BG;
        v.text_edit_bg_color = Some(BG);
        v.code_bg_color = CELL;
        v.warn_fg_color = WARN;
        v.error_fg_color = DANGER;
        v.window_stroke = Stroke::new(1.0, EDGE);
        v.window_corner_radius = CornerRadius::ZERO;
        v.menu_corner_radius = CornerRadius::ZERO;
        v.window_shadow = Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(150),
        };
        v.popup_shadow = Shadow {
            offset: [0, 3],
            blur: 8,
            spread: 0,
            color: Color32::from_black_alpha(130),
        };
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;
        v.selection.bg_fill = BAND;
        v.selection.stroke = Stroke::new(1.5, TEXT_HI);
        v.text_cursor.stroke = Stroke::new(2.0, ACCENT);
        v.striped = false;
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = PANEL;
        w.noninteractive.weak_bg_fill = PANEL;
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        w.noninteractive.bg_stroke = Stroke::new(1.0, RULE);
        w.noninteractive.corner_radius = CornerRadius::ZERO;
        // Buttons are bare serif words (a band under the pointer, pressed fill), as the painted rows' text actions.
        for (x, fill, weak, stroke, fg) in [
            (
                &mut w.inactive,
                CELL,
                Color32::TRANSPARENT,
                Stroke::NONE,
                TEXT,
            ),
            (&mut w.hovered, PRESSED, BAND, Stroke::NONE, TEXT_HI),
            (&mut w.active, PRESSED, PRESSED, Stroke::NONE, TEXT_HI),
            (&mut w.open, PRESSED, BAND, Stroke::NONE, TEXT_HI),
        ] {
            x.bg_fill = fill;
            x.weak_bg_fill = weak;
            x.bg_stroke = stroke;
            x.fg_stroke = Stroke::new(1.5, fg);
            x.corner_radius = CornerRadius::ZERO;
            x.expansion = 0.0;
        }
    });
}

// A modal panel: lacquer fill, edge, `L` padding.
pub fn panel() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, EDGE))
        .inner_margin(Margin::same(L as i8))
}

// A bar at the edge of a full-window screen.
pub fn bar() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .inner_margin(Margin::symmetric(M as i8, S as i8))
}

// A card in a list (a replay, a translation job): a rule-coloured frame on the panel.
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(CELL)
        .stroke(Stroke::new(1.0, RULE))
        .inner_margin(Margin::symmetric(M as i8, S as i8))
}

// Button hierarchy: one primary action per screen, secondary for the rest, danger for what deletes or quits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Secondary,
    Danger,
}

pub fn button<'a>(text: impl Into<String>, kind: Kind) -> egui::Button<'a> {
    let text = RichText::new(latin_ellipsis(&text.into()).into_owned());
    egui::Button::new(match kind {
        Kind::Primary => text.color(TEXT_HI),
        Kind::Secondary => text.color(TEXT),
        Kind::Danger => text.color(DANGER),
    })
}

// An underline tab bar in the egui flow (launcher Replays / Stats screens): serif words, the selected one white
// with a lavender underline. Wraps when narrow, or scrolls sideways with `scroll`. The clicked tab and the
// selected tab's rect.
pub fn tabs(
    ui: &mut Ui,
    labels: &[&str],
    selected: usize,
    scroll: bool,
) -> (Option<usize>, Option<egui::Rect>) {
    let (mut clicked, mut rect) = (None, None);
    let mut row = |ui: &mut Ui| {
        ui.spacing_mut().item_spacing.x = L;
        for (i, text) in labels.iter().enumerate() {
            let r = tab(ui, i == selected, text);
            if r.clicked() {
                clicked = Some(i);
            }
            if i == selected {
                rect = Some(r.rect);
                if scroll && !ui.clip_rect().contains_rect(r.rect) {
                    r.scroll_to_me(None);
                }
            }
        }
    };
    if scroll {
        egui::ScrollArea::horizontal()
            .id_salt("tabs")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| ui.horizontal(|ui| row(ui)));
    } else {
        ui.horizontal_wrapped(|ui| row(ui));
    }
    (clicked, rect)
}

fn tab(ui: &mut Ui, selected: bool, text: &str) -> Response {
    let h = ui.spacing().interact_size.y;
    let size = BODY + 3.0;
    let g = ui
        .painter()
        .layout_no_wrap(text.to_string(), serif(size), TEXT);
    let (rect, r) = ui.allocate_exact_size(vec2(g.size().x, h), egui::Sense::click());
    let color = if selected || r.hovered() {
        TEXT_HI
    } else {
        HELP
    };
    let pos = pos2(rect.min.x, rect.center().y - g.size().y / 2.0);
    outlined(ui.painter(), pos, g.clone(), color, false);
    if selected {
        let y = pos.y + g.size().y + 5.0;
        ui.painter()
            .hline(rect.x_range(), y, Stroke::new(2.0, ACCENT));
    }
    r
}

// The keyboard / pad focus marker around `rect` in the egui flow: the band and the lavender tick.
pub fn ring(ui: &Ui, rect: egui::Rect, _inside: bool) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, BAND.gamma_multiply(0.85));
    tick(p, rect, 4.0);
}

// Weak small text (hints, help, version).
pub fn hint(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(HELP)
}

// ---- Painted widgets (port menu and launcher rows). Sizes in points; `px` = one window pixel in points.

// Text with the game's lettering treatment: a 1 px black ring and a soft drop shadow, plus a plum glow when
// `glow` (the focused item).
pub fn outlined(p: &Painter, pos: Pos2, g: Arc<Galley>, color: Color32, glow: bool) {
    let px = 1.0 / p.pixels_per_point();
    if glow {
        for r in [3.0, 5.5] {
            for (dx, dy) in RING {
                p.galley_with_override_text_color(
                    pos + vec2(dx, dy) * r * px,
                    g.clone(),
                    GLOW.gamma_multiply(0.35),
                );
            }
        }
    }
    p.galley_with_override_text_color(
        pos + vec2(0.0, 2.0 * px),
        g.clone(),
        Color32::from_black_alpha(150),
    );
    for (dx, dy) in RING {
        p.galley_with_override_text_color(pos + vec2(dx, dy) * px, g.clone(), Color32::BLACK);
    }
    p.galley_with_override_text_color(pos, g, color);
}

const RING: [(f32, f32); 8] = [
    (-1.0, 0.0),
    (1.0, 0.0),
    (0.0, -1.0),
    (0.0, 1.0),
    (-1.0, -1.0),
    (1.0, 1.0),
    (-1.0, 1.0),
    (1.0, -1.0),
];

// Plain text with only the drop shadow (help, status, hints).
pub fn shadowed(p: &Painter, pos: Pos2, g: Arc<Galley>, color: Color32) {
    let px = 1.0 / p.pixels_per_point();
    p.galley_with_override_text_color(
        pos + vec2(0.0, px),
        g.clone(),
        Color32::from_black_alpha(170),
    );
    p.galley_with_override_text_color(pos, g, color);
}

// `text` wrapped at most `w` wide with its lines evened out: the same line count at the narrowest width that
// keeps it, so a last line never holds a lone word.
pub fn balanced(p: &Painter, text: &str, font: FontId, color: Color32, w: f32) -> Arc<Galley> {
    let w = w.max(10.0);
    let full = wrap(p, text.to_string(), font.clone(), color, w);
    let n = full.rows.len();
    if n < 2 {
        return full;
    }
    let (mut lo, mut hi) = (w * 0.4, w);
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        if wrap(p, text.to_string(), font.clone(), color, mid)
            .rows
            .len()
            > n
        {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    wrap(p, text.to_string(), font, color, hi.ceil())
}

// Kana, CJK ideographs and full-width forms: the characters Japanese line breaking applies to.
pub fn is_ja(c: char) -> bool {
    matches!(u32::from(c), 0x3000..=0x30ff | 0x3400..=0x9fff | 0xf900..=0xfaff | 0xff00..=0xffef)
}

// Characters a line never starts with, and never ends with (kinsoku shori).
fn no_start(c: char) -> bool {
    "、。，．・：；？！ー）」』】〕〉》｝’”…‥ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶ々%％)]},.!?:;".contains(c)
}

fn no_end(c: char) -> bool {
    "（「『【〔〈《｛‘“([{".contains(c)
}

// The pieces a line never breaks inside, in order, each with its trailing spaces: Japanese phrases (BudouX, then
// kinsoku), and every other word whole.
pub fn pieces(text: &str) -> Vec<&str> {
    thread_local! {
        static JA: budoux_phf_rs::Parser = budoux_phf_rs::Parser::japanese_parser();
    }
    let mut cuts = vec![0usize];
    let mut i = 0;
    for (at, c) in text.char_indices() {
        if at < i {
            continue;
        }
        if c.is_whitespace() {
            let end = text[at..]
                .find(|c: char| !c.is_whitespace())
                .map_or(text.len(), |e| at + e);
            if end < text.len() {
                cuts.push(end);
            }
            i = end;
            continue;
        }
        let end = text[at..]
            .find(char::is_whitespace)
            .map_or(text.len(), |e| at + e);
        let word = &text[at..end];
        if word.chars().any(is_ja) {
            let mut off = at;
            JA.with(|j| {
                j.parse_with(word, |chunk| {
                    if off > at {
                        cuts.push(off);
                    }
                    off += chunk.len();
                });
            });
        }
        i = end;
    }
    cuts.push(text.len());
    cuts.dedup();
    // Kinsoku: no cut before a closing mark or small kana, none after an opening bracket.
    cuts.retain(|&c| {
        c == 0
            || c == text.len()
            || !(text[c..].chars().next().is_some_and(no_start)
                || text[..c].chars().next_back().is_some_and(no_end))
    });
    cuts.windows(2).map(|w| &text[w[0]..w[1]]).collect()
}

// Text wrapped at `w` as `Painter::layout` does, except that text with Japanese in it breaks only between
// `pieces` (a piece wider than `w` still wraps inside).
pub fn wrap(p: &Painter, text: String, font: FontId, color: Color32, w: f32) -> Arc<Galley> {
    if !text.chars().any(is_ja) {
        return p.layout(text, font, color, w);
    }
    let width = |s: &str| {
        p.layout_no_wrap(s.to_string(), font.clone(), color)
            .size()
            .x
    };
    let mut out = String::with_capacity(text.len() + 8);
    for (n, para) in text.split('\n').enumerate() {
        if n > 0 {
            out.push('\n');
        }
        let mut line = String::new();
        for piece in pieces(para) {
            let next = format!("{line}{piece}");
            if line.is_empty() || width(next.trim_end()) <= w {
                line = next;
            } else {
                out.push_str(line.trim_end());
                out.push('\n');
                line = piece.to_string();
            }
        }
        out.push_str(&line);
    }
    p.layout(out, font, color, w)
}

pub fn layout(p: &Painter, text: &str, font: FontId, wrap: f32) -> Arc<Galley> {
    self::wrap(p, text.to_string(), font, TEXT, wrap)
}

// The heading token: the serif word, its lavender caption below at the same left edge. Returns the block height.
pub fn heading(p: &Painter, pos: Pos2, word: &str, caption: &str, size: f32, cap: f32) -> f32 {
    let g = p.layout_no_wrap(word.to_string(), serif(size), TEXT_HI);
    let h = g.size().y;
    outlined(p, pos, g.clone(), TEXT_HI, false);
    if caption.is_empty() {
        return h;
    }
    let c = p.layout_no_wrap(caption.to_string(), serif(cap), ACCENT);
    let ch = c.size().y;
    // Under the word's descenders: at least 3 points between the inks.
    let dy = (size * 1.18).max(g.mesh_bounds.max.y + 3.0 - c.mesh_bounds.min.y);
    caption_text(p, pos + vec2(0.0, dy), c);
    (dy + ch).max(h)
}

// A small lavender serif caption with the plum glow.
pub fn caption_text(p: &Painter, pos: Pos2, g: Arc<Galley>) {
    let px = 1.0 / p.pixels_per_point();
    for (dx, dy) in RING {
        p.galley_with_override_text_color(
            pos + vec2(dx, dy) * 2.0 * px,
            g.clone(),
            GLOW.gamma_multiply(0.5),
        );
    }
    p.galley_with_override_text_color(
        pos + vec2(0.0, px),
        g.clone(),
        Color32::from_black_alpha(200),
    );
    p.galley_with_override_text_color(pos, g, ACCENT);
}

// The lacquer panel: opaque fill, a 1 px edge and an inner line 3 px in on the side facing the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
}

pub fn lacquer(p: &Painter, rect: Rect, edge: Edge) {
    let px = 1.0 / p.pixels_per_point();
    p.rect_filled(rect, 0.0, PANEL);
    let (x0, x1) = match edge {
        Edge::Left => (rect.min.x + 0.5 * px, rect.min.x + 3.5 * px),
        Edge::Right => (rect.max.x - 0.5 * px, rect.max.x - 3.5 * px),
    };
    p.vline(x0, rect.y_range(), Stroke::new(px, EDGE));
    p.vline(x1, rect.y_range(), Stroke::new(px, EDGE_IN));
}

pub fn rule(p: &Painter, x: egui::Rangef, y: f32) {
    let px = 1.0 / p.pixels_per_point();
    p.hline(x, y.round_to_px(px), Stroke::new(px, RULE));
}

trait RoundPx {
    fn round_to_px(self, px: f32) -> f32;
}

impl RoundPx for f32 {
    fn round_to_px(self, px: f32) -> f32 {
        (self / px).floor() * px + px / 2.0
    }
}

// The last kind of input: the keyboard focus bar shows only after a key or pad press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modality {
    Pointer,
    Keys,
    Touch,
}

// Hover and modality of a screen's rows, kept apart from its keyboard focus (focus-visible).
#[derive(Clone, Debug)]
pub struct Input<T> {
    pub modality: Modality,
    // The row under the pointer in the last finished pass, and in this one.
    hover: Option<T>,
    next: Option<T>,
}

impl<T> Default for Input<T> {
    fn default() -> Self {
        Self {
            modality: Modality::Pointer,
            hover: None,
            next: None,
        }
    }
}

impl<T: Copy + PartialEq> Input<T> {
    pub fn new(modality: Modality) -> Self {
        Self {
            modality,
            hover: None,
            next: None,
        }
    }

    // Start of a pass: a moving or pressing pointer makes it pointer input (touch on a touch screen).
    pub fn begin(&mut self, ctx: &egui::Context, touch_screen: bool) {
        let (touch, pointer) = ctx.input(|i| {
            (
                i.any_touches()
                    || i.raw
                        .events
                        .iter()
                        .any(|e| matches!(e, egui::Event::Touch { .. })),
                i.pointer.is_moving() || i.pointer.any_pressed(),
            )
        });
        if touch || (pointer && touch_screen) {
            self.pointer(true);
        } else if pointer {
            self.pointer(false);
        }
        self.next = None;
    }

    pub fn pointer(&mut self, touch: bool) {
        self.modality = if touch {
            Modality::Touch
        } else {
            Modality::Pointer
        };
    }

    // `t` is under the pointer this pass.
    pub fn over(&mut self, t: T) {
        if self.modality != Modality::Touch {
            self.next = Some(t);
        }
    }

    // End of a pass; true when the hovered row changed (the help line needs another pass).
    pub fn end(&mut self) -> bool {
        let changed = self.hover != self.next;
        self.hover = self.next;
        changed
    }

    // A key or pad press. None: the focus bar was showing and moves as usual; Some(h): the bar appears now, on the
    // hovered row `h` if any (else on the last focused row), and the press does nothing else.
    pub fn key(&mut self) -> Option<Option<T>> {
        if self.modality == Modality::Keys {
            return None;
        }
        self.modality = Modality::Keys;
        Some(self.hover)
    }

    pub fn bar(&self) -> bool {
        self.modality == Modality::Keys
    }

    // The transient pointer highlight of `t` (only while the pointer is over it).
    pub fn lit(&self, t: T) -> bool {
        self.modality == Modality::Pointer && self.next == Some(t)
    }

    // The row the help line describes under pointer input.
    pub fn hovered(&self) -> Option<T> {
        match self.modality {
            Modality::Pointer => self.hover,
            _ => None,
        }
    }
}

// How a row is marked: the focus bar (keys / pad), a hover band (pointer), pressed (touch), or not at all.
pub fn mark(p: &Painter, band: Rect, tick_w: f32, bar: bool, lit: bool, pressed: bool) {
    if bar {
        focus(p, band, tick_w, pressed);
    } else if pressed {
        p.rect_filled(band, 0.0, PRESSED);
    } else if lit {
        p.rect_filled(band, 0.0, BAND.gamma_multiply(0.55));
    }
}

// The focus marker: the band and a lavender tick at its left edge, 60 % of its height.
pub fn focus(p: &Painter, band: Rect, tick_w: f32, pressed: bool) {
    p.rect_filled(band, 0.0, if pressed { PRESSED } else { BAND });
    tick(p, band, tick_w);
}

// The same over the 4:3 playfield: lavender 16 → 4 % left to right instead of the band.
pub fn focus_over_game(p: &Painter, band: Rect, tick_w: f32) {
    let a = ACCENT.gamma_multiply(0.16);
    let b = ACCENT.gamma_multiply(0.04);
    let mut m = egui::Mesh::default();
    m.colored_vertex(band.left_top(), a);
    m.colored_vertex(band.right_top(), b);
    m.colored_vertex(band.right_bottom(), b);
    m.colored_vertex(band.left_bottom(), a);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(m);
    tick(p, band, tick_w);
}

// The lavender focus tick at the band's left edge, 60 % of its height.
pub fn tick(p: &Painter, band: Rect, w: f32) {
    let h = band.height() * 0.6;
    let r = Rect::from_min_size(pos2(band.min.x, band.center().y - h / 2.0), vec2(w, h));
    p.rect_filled(r, 0.0, ACCENT);
}

// "…" after a Latin / Cyrillic letter as "...": the serif face's own ellipsis is the centred CJK form.
pub fn latin_ellipsis(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains('…') {
        return s.into();
    }
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev = ' ';
    for c in s.chars() {
        if c == '…' && (prev.is_ascii() || matches!(prev, 'А'..='я' | 'ё' | 'Ё')) {
            out.push_str("...");
        } else {
            out.push(c);
        }
        prev = c;
    }
    out.into()
}

// A ◀ / ▶ triangle of height `h` centred on `c`.
pub fn arrow(p: &Painter, c: Pos2, h: f32, right: bool, color: Color32) {
    let w = h * 0.86;
    let (tip, back) = if right {
        (c.x + w / 2.0, c.x - w / 2.0)
    } else {
        (c.x - w / 2.0, c.x + w / 2.0)
    };
    p.add(egui::Shape::convex_polygon(
        vec![
            pos2(tip, c.y),
            pos2(back, c.y - h / 2.0),
            pos2(back, c.y + h / 2.0),
        ],
        color,
        Stroke::NONE,
    ));
}

// A slider track: 2 px (3 px focused), plum fill to the knob, the knob lavender with a black ring when focused.
pub fn track(p: &Painter, x: egui::Rangef, cy: f32, frac: f32, focused: bool) {
    let px = 1.0 / p.pixels_per_point();
    let t = if focused { 3.0 } else { 2.0 } * px;
    let line = Rect::from_x_y_ranges(x, cy - t / 2.0..=cy + t / 2.0);
    p.rect_filled(line, 0.0, TRACK);
    let kx = x.min + x.span() * frac.clamp(0.0, 1.0);
    p.rect_filled(
        Rect::from_min_max(line.min, pos2(kx, line.max.y)),
        0.0,
        FILL,
    );
    let d = if focused { 14.0 } else { 10.0 } * px;
    let (fill, ring) = if focused {
        (ACCENT, 2.0)
    } else {
        (Color32::from_rgb(0x8c, 0x86, 0x98), 1.0)
    };
    p.circle_filled(pos2(kx, cy), d / 2.0 + ring * px, Color32::BLACK);
    p.circle_filled(pos2(kx, cy), d / 2.0, fill);
}

// A › chevron at the right end of a row.
pub fn chevron(p: &Painter, right: Pos2, size: f32, color: Color32) {
    let g = p.layout_no_wrap("›".into(), serif(size), color);
    let pos = pos2(right.x - g.size().x, right.y - g.size().y / 2.0);
    shadowed(p, pos, g, color);
}

// A keycap: the key's glyph(s) in a 1 px hairline box, centred on a line of `size` text at `pos`. Returns its width.
pub fn keycap(p: &Painter, pos: Pos2, keys: &str, size: f32) -> f32 {
    let line = p.layout_no_wrap("Ag".into(), sans(size), HELP).size().y;
    keycap_at(p, pos.x, pos.y + line / 2.0, keys, size)
}

// A keycap whose box is centred on `cy`.
pub fn keycap_at(p: &Painter, x: f32, cy: f32, keys: &str, size: f32) -> f32 {
    let k = p.layout_no_wrap(keys.to_string(), sans(size * 0.86), KEYCAP);
    let w = keycap_w(p, keys, size);
    let r = Rect::from_center_size(pos2(x + w / 2.0, cy), vec2(w, size * 1.3));
    let px = 1.0 / p.pixels_per_point();
    p.rect_stroke(
        r.shrink(px / 2.0),
        3.0,
        Stroke::new(px, DIM),
        egui::StrokeKind::Inside,
    );
    shadowed(p, r.center() - k.size() / 2.0, k, KEYCAP);
    w
}

pub fn keycap_w(p: &Painter, keys: &str, size: f32) -> f32 {
    let k = p.layout_no_wrap(keys.to_string(), sans(size * 0.86), KEYCAP);
    (k.size().x + size * 0.7).max(size * 1.3)
}

// One key hint: the keycap, then the word in helper grey. Returns the width used.
pub fn key_hint(p: &Painter, pos: Pos2, keys: &str, word: &str, size: f32) -> f32 {
    let kw = keycap(p, pos, keys, size);
    let w = p.layout_no_wrap(word.to_string(), sans(size), HELP);
    let ww = w.size().x;
    let gap = size * 0.5;
    shadowed(p, pos + vec2(kw + gap, 0.0), w, HELP);
    kw + gap + ww
}

// The width `key_hint` takes.
pub fn key_hint_w(p: &Painter, keys: &str, word: &str, size: f32) -> f32 {
    let ww = p
        .layout_no_wrap(word.to_string(), sans(size), HELP)
        .size()
        .x;
    keycap_w(p, keys, size) + size * 0.5 + ww
}

// A line of key hints; returns its width.
pub fn key_hints(p: &Painter, pos: Pos2, items: &[(&str, &str)], size: f32) -> f32 {
    let mut x = pos.x;
    for (i, (k, w)) in items.iter().enumerate() {
        if i > 0 {
            x += size * 1.3;
        }
        x += key_hint(p, pos2(x, pos.y), k, w, size);
    }
    x - pos.x
}

// Key hints in `max_w`: the hints that would run past it are left out, last first.
pub fn key_hints_fit(p: &Painter, pos: Pos2, items: &[(&str, &str)], size: f32, max_w: f32) -> f32 {
    let mut used = 0.0;
    let mut n = 0;
    for (i, (k, w)) in items.iter().enumerate() {
        let iw = key_hint_w(p, k, w, size) + if i > 0 { size * 1.3 } else { 0.0 };
        if used + iw > max_w {
            break;
        }
        used += iw;
        n += 1;
    }
    key_hints(p, pos, &items[..n], size)
}

// A filled pad disc with its button letter (A / B / ≡).
pub fn pad_disc(p: &Painter, c: Pos2, size: f32, label: &str) {
    p.circle_filled(c, size * 0.7, Color32::from_rgb(0xe6, 0xe0, 0xea));
    p.text(
        c,
        egui::Align2::CENTER_CENTER,
        label,
        sans(size * 0.8),
        PANEL,
    );
}

// A settings row's value control as the menus paint it.
#[derive(Clone, Debug, PartialEq)]
pub enum Ctl {
    // Inline words, the selected one white (a value without a word in the list: none selected).
    Choices {
        words: Vec<String>,
        sel: Option<usize>,
    },
    // ◀ track ▶ and the number; `frac` 0..1 along the track.
    Slider {
        frac: f32,
        text: String,
        unit: String,
    },
    // Read-only value text with a › (activating the row opens or cycles it).
    Value(String),
    None,
}

// What a click / tap / drag on a control asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CtlOut {
    Pick(usize),
    Step(i32),
    Frac(f32),
}

// How a control is laid out: `size` the serif word size, `num` the number size; `cells`: phone stepper cells and
// 48 dp choice cells; `right`: aligned to the rect's right edge, else from `indent`; `track` the slider length.
#[derive(Clone, Copy, Debug)]
pub struct CtlStyle {
    pub size: f32,
    pub num: f32,
    pub cells: bool,
    pub right: bool,
    pub indent: f32,
    pub track: f32,
    pub numw: f32,
    pub gap: f32,
    pub cell_w: f32,
    // Phone choice cells: the least cell height.
    pub cell_h: f32,
    // Choice words stay on one line: when they do not fit, the row shows a ◀ value ▶ stepper instead.
    pub one_line: bool,
}

fn hit(ui: &Ui, id: egui::Id, r: Rect, sense: egui::Sense) -> Response {
    ui.interact(r, id, sense)
}

// Paints `ctl` in `rect` (the control column of one row) and returns what the pointer asked for.
#[expect(clippy::too_many_lines, reason = "one painted control per kind")]
pub fn value_ctl(
    ui: &Ui,
    p: &Painter,
    id: egui::Id,
    rect: Rect,
    ctl: &Ctl,
    focused: bool,
    st: &CtlStyle,
) -> Option<CtlOut> {
    let cy = rect.center().y;
    let arrow_c = if focused { TEXT_HI } else { DIM };
    let mut out = None;
    match ctl {
        Ctl::None => {}
        Ctl::Choices { words, sel } if steps(p, words, st, rect.width()) => {
            let word = sel.and_then(|i| words.get(i)).cloned().unwrap_or_default();
            out = stepper(ui, p, id, rect, st, focused, |p, r| {
                // A single word wider than the middle is set smaller (to 60 %) rather than overflowing.
                let wide = p
                    .layout_no_wrap(word.clone(), serif(st.size), TEXT)
                    .size()
                    .x;
                let size = st.size * ((r.width() - 8.0) / wide.max(1.0)).clamp(0.6, 1.0);
                let g = wrap(p, word.clone(), serif(size), TEXT, r.width().max(10.0));
                let pos = r.center() - g.size() / 2.0;
                outlined(p, pos, g, if focused { TEXT_HI } else { TEXT }, false);
            });
        }
        Ctl::Choices { words, sel } => {
            let (items, lines) = choice_layout(p, words, st, rect.width());
            let lh = line_h(st);
            let top = cy - lines as f32 * lh / 2.0;
            for (i, (g, x, line, w)) in items.into_iter().enumerate() {
                let r =
                    Rect::from_min_size(pos2(rect.min.x + x, top + line as f32 * lh), vec2(w, lh));
                let hr = if st.cells {
                    r
                } else {
                    r.expand2(vec2(st.gap / 2.0, 0.0))
                };
                let resp = hit(ui, id.with(("pick", i)), hr, egui::Sense::click());
                if st.cells && resp.is_pointer_button_down_on() {
                    p.rect_filled(r.shrink(4.0), 0.0, PRESSED);
                }
                if resp.clicked() {
                    out = Some(CtlOut::Pick(i));
                }
                let pos = if st.cells {
                    r.center() - g.size() / 2.0
                } else {
                    pos2(r.min.x, r.center().y - g.size().y / 2.0)
                };
                let on = *sel == Some(i);
                outlined(p, pos, g, if on { TEXT_HI } else { DIM }, false);
            }
        }
        Ctl::Slider { frac, text, unit } => {
            let num = |p: &Painter, right: f32| {
                let u = p.layout_no_wrap(unit.clone(), sans(st.num * 0.78), HELP);
                let n = p.layout_no_wrap(text.clone(), sans(st.num), TEXT);
                let uw = if unit.is_empty() {
                    0.0
                } else {
                    u.size().x + 3.0
                };
                let ny = cy - n.size().y / 2.0;
                shadowed(
                    p,
                    pos2(right - uw - n.size().x, ny),
                    n,
                    if focused { TEXT_HI } else { TEXT },
                );
                if !unit.is_empty() {
                    shadowed(
                        p,
                        pos2(right - u.size().x, ny + (st.num - st.num * 0.78) * 0.8),
                        u,
                        HELP,
                    );
                }
            };
            // The number column fits its text (a word such as "Normal" is wider than a number).
            let numw = {
                let n = p.layout_no_wrap(text.clone(), sans(st.num), TEXT).size().x;
                let u = if unit.is_empty() {
                    0.0
                } else {
                    p.layout_no_wrap(unit.clone(), sans(st.num * 0.78), HELP)
                        .size()
                        .x
                        + 3.0
                };
                st.numw.max(n + u + 12.0)
            };
            if st.cells {
                out = stepper(ui, p, id, rect, st, focused, |p, r| {
                    let tr = egui::Rangef::new(r.min.x + 14.0, r.max.x - numw - 12.0);
                    // A track shorter than this reads as a stray line: the stepper then shows the value alone.
                    if tr.span() >= MIN_CELL_TRACK {
                        track(p, tr, cy, *frac, focused);
                        num(p, r.max.x - 8.0);
                    } else {
                        let n = p.layout_no_wrap(format!("{text}{unit}"), sans(st.num), TEXT);
                        shadowed(
                            p,
                            r.center() - n.size() / 2.0,
                            n,
                            if focused { TEXT_HI } else { TEXT },
                        );
                    }
                });
                let tr = Rect::from_x_y_ranges(
                    rect.min.x + st.cell_w + 14.0..=rect.max.x - st.cell_w - numw - 12.0,
                    rect.y_range(),
                );
                if tr.width() >= MIN_CELL_TRACK
                    && let Some(f) = drag(ui, id, tr)
                {
                    out = Some(CtlOut::Frac(f));
                }
            } else {
                let ah = st.size * 0.5;
                // The track gives way so the slider stays inside its rect.
                let fixed = ah + 16.0 + 16.0 + ah + numw;
                let track_len = st.track.min(rect.width() - fixed).max(24.0);
                let w = fixed + track_len;
                let x0 = if st.right { rect.max.x - w } else { rect.min.x };
                let left =
                    Rect::from_center_size(pos2(x0 + ah / 2.0, cy), vec2(ah + 16.0, rect.height()));
                let tx = x0 + ah + 16.0;
                let right = Rect::from_center_size(
                    pos2(tx + track_len + 16.0 + ah / 2.0, cy),
                    vec2(ah + 16.0, rect.height()),
                );
                arrow(p, left.center(), ah, false, arrow_c);
                arrow(p, right.center(), ah, true, arrow_c);
                if hit(ui, id.with("dec"), left, egui::Sense::click()).clicked() {
                    out = Some(CtlOut::Step(-1));
                }
                if hit(ui, id.with("inc"), right, egui::Sense::click()).clicked() {
                    out = Some(CtlOut::Step(1));
                }
                let tr = egui::Rangef::new(tx, tx + track_len);
                track(p, tr, cy, *frac, focused);
                if let Some(f) = drag(ui, id, Rect::from_x_y_ranges(tr, rect.y_range())) {
                    out = Some(CtlOut::Frac(f));
                }
                num(p, x0 + w);
            }
        }
        Ctl::Value(text) => {
            let x = rect.min.x + if st.right { 0.0 } else { st.indent };
            let g = wrap(
                p,
                text.clone(),
                sans(st.num * 0.8),
                TEXT,
                (rect.max.x - x - st.size).max(10.0),
            );
            shadowed(
                p,
                pos2(x, cy - g.size().y / 2.0),
                g,
                if focused { TEXT_HI } else { TEXT },
            );
            chevron(p, pos2(rect.max.x, cy), st.size, HELP);
        }
    }
    out
}

const MIN_CELL_TRACK: f32 = 60.0;

// Enums with more values than this get a stepper; fewer show all their words (the menus' one rule).
pub const MAX_WORDS: usize = 5;

// The choice words of a row `room` points wide show as a stepper: too many of them, or more than one line of them
// in a one-line style.
fn steps(p: &Painter, words: &[String], st: &CtlStyle, room: f32) -> bool {
    words.len() > MAX_WORDS || (st.one_line && choice_layout(p, words, st, room).1 > 1)
}

// Height of one line of choice words.
fn line_h(st: &CtlStyle) -> f32 {
    if st.cells {
        (st.size * 2.3).max(st.cell_h)
    } else {
        st.size * 1.8
    }
}

// The shortest gap between two choice words (cells: the room around the word inside its cell).
fn word_gap(st: &CtlStyle) -> f32 {
    st.gap
}

// Choice words laid out in lines of `room` points: (galley, x, line, item width) and the line count. Words keep
// their size and wrap to the next line; `right` cells share one line evenly when every word fits its share.
// A laid-out choice word: galley, x, line, item width.
type Placed = (Arc<Galley>, f32, usize, f32);

fn choice_layout(p: &Painter, words: &[String], st: &CtlStyle, room: f32) -> (Vec<Placed>, usize) {
    let indent = if st.right || st.cells { 0.0 } else { st.indent };
    let gs: Vec<Arc<Galley>> = words
        .iter()
        .map(|w| p.layout_no_wrap(w.clone(), serif(st.size), TEXT))
        .collect();
    let pad = word_gap(st);
    if st.cells && st.right {
        let share = room / gs.len().max(1) as f32;
        if gs.iter().all(|g| g.size().x + pad <= share) {
            let n = gs.len();
            let v = gs
                .into_iter()
                .enumerate()
                .map(|(i, g)| (g, share * i as f32, 0, share))
                .collect();
            return (v, usize::from(n > 0));
        }
    }
    let width = |g: &Galley| {
        if st.cells {
            st.cell_w.max(g.size().x + pad)
        } else {
            g.size().x
        }
    };
    let gap = if st.cells { 0.0 } else { pad };
    let (mut x, mut line, mut v) = (indent, 0, Vec::new());
    for g in gs {
        let w = width(&g);
        if x > indent && x + w > room + 0.5 {
            x = indent;
            line += 1;
        }
        v.push((g, x, line, w));
        x += w + gap;
    }
    let lines = if v.is_empty() { 0 } else { line + 1 };
    // Right-aligned words end at the rect's right edge when they fit one line; wrapped words stay left-aligned.
    if st.right && !st.cells && lines == 1 {
        for l in 0..lines {
            let end = v
                .iter()
                .filter(|c| c.2 == l)
                .map(|c| c.1 + c.3)
                .fold(0.0, f32::max);
            let shift = (room - end).max(0.0);
            v.iter_mut().filter(|c| c.2 == l).for_each(|c| c.1 += shift);
        }
    }
    (v, lines)
}

// The widest single choice word of `ctl` (0 for other controls): a column narrower than this cannot hold it.
pub fn widest_word(p: &Painter, ctl: &Ctl, st: &CtlStyle) -> f32 {
    match ctl {
        Ctl::Choices { words, .. } if words.len() <= MAX_WORDS && !st.one_line => words
            .iter()
            .map(|w| {
                let x = p.layout_no_wrap(w.clone(), serif(st.size), TEXT).size().x;
                if st.cells {
                    st.cell_w.max(x + word_gap(st))
                } else {
                    x
                }
            })
            .fold(0.0, f32::max),
        _ => 0.0,
    }
}

// The height a control needs in a row `w` points wide.
pub fn ctl_height(p: &Painter, ctl: &Ctl, st: &CtlStyle, w: f32) -> f32 {
    match ctl {
        Ctl::Choices { words, .. } if !steps(p, words, st, w) => {
            choice_layout(p, words, st, w).1 as f32 * line_h(st)
        }
        Ctl::None => 0.0,
        _ => line_h(st),
    }
}

// The pieces a label never breaks inside, without their spaces.
pub fn unbreakable(text: &str) -> Vec<&str> {
    pieces(text).into_iter().map(str::trim_end).collect()
}

// The label column (16 points of it the gap to the control): at least `base`, wide enough for the widest single
// word of `labels` (labels wrap only between words), at most `max`.
pub fn label_width(p: &Painter, labels: &[&str], font: &FontId, base: f32, max: f32) -> f32 {
    let widest = labels
        .iter()
        .flat_map(|l| unbreakable(l))
        .map(|w| p.layout_no_wrap(w.to_string(), font.clone(), TEXT).size().x)
        .fold(0.0, f32::max);
    (widest + 24.0).max(base).min(max)
}

// ◀ [middle] ▶: phone cells at both ends (`cell_w` × `cell_h`), or small arrows around the middle on desktop.
pub fn stepper(
    ui: &Ui,
    p: &Painter,
    id: egui::Id,
    rect: Rect,
    st: &CtlStyle,
    focused: bool,
    middle: impl FnOnce(&Painter, Rect),
) -> Option<CtlOut> {
    let cy = rect.center().y;
    let mut out = None;
    let (left, right, mid) = if st.cells {
        let h = st.cell_h;
        let l = Rect::from_min_size(pos2(rect.min.x, cy - h / 2.0), vec2(st.cell_w, h));
        let r = Rect::from_min_size(
            pos2(rect.max.x - st.cell_w, cy - h / 2.0),
            vec2(st.cell_w, h),
        );
        (
            l,
            r,
            Rect::from_x_y_ranges(l.max.x..=r.min.x, rect.y_range()),
        )
    } else {
        let ah = st.size * 0.5;
        let x = rect.min.x + if st.right { 0.0 } else { st.indent - ah - 16.0 }.max(0.0);
        let l = Rect::from_center_size(pos2(x + ah / 2.0, cy), vec2(ah + 16.0, rect.height()));
        let r = Rect::from_center_size(
            pos2(rect.max.x - ah / 2.0, cy),
            vec2(ah + 16.0, rect.height()),
        );
        (
            l,
            r,
            Rect::from_x_y_ranges(l.max.x..=r.min.x, rect.y_range()),
        )
    };
    for (r, dir) in [(left, -1), (right, 1)] {
        let resp = hit(ui, id.with(("step", dir)), r, egui::Sense::click());
        let c = if focused {
            TEXT_HI
        } else if st.cells {
            TEXT
        } else {
            DIM
        };
        if st.cells {
            let fill = if resp.is_pointer_button_down_on() || focused {
                PRESSED
            } else {
                CELL
            };
            p.rect_filled(r, 0.0, fill);
        }
        arrow(
            p,
            r.center(),
            if st.cells { 20.0 } else { st.size * 0.5 },
            dir > 0,
            c,
        );
        if resp.clicked() {
            out = Some(CtlOut::Step(dir));
        }
    }
    middle(p, mid);
    out
}

// A drag or click on a slider track: the new position 0..1.
fn drag(ui: &Ui, id: egui::Id, r: Rect) -> Option<f32> {
    let resp = ui.interact(r, id.with("track"), egui::Sense::click_and_drag());
    if resp.clicked() || resp.dragged() {
        let x = resp.interact_pointer_pos()?.x;
        return Some(((x - r.min.x) / r.width()).clamp(0.0, 1.0));
    }
    None
}

// A rectangle of `color` whose edges fade out over `feather` (the soft wash behind the 4:3 list).
pub fn soft_rect(p: &Painter, rect: Rect, feather: f32, color: Color32) {
    let inner = rect.shrink(feather);
    let clear = Color32::TRANSPARENT;
    let xs = [rect.min.x, inner.min.x, inner.max.x, rect.max.x];
    let ys = [rect.min.y, inner.min.y, inner.max.y, rect.max.y];
    let mut m = egui::Mesh::default();
    for (j, &y) in ys.iter().enumerate() {
        for (i, &x) in xs.iter().enumerate() {
            let solid = (1..=2).contains(&i) && (1..=2).contains(&j);
            m.colored_vertex(pos2(x, y), if solid { color } else { clear });
        }
    }
    for j in 0..3u32 {
        for i in 0..3u32 {
            let a = j * 4 + i;
            m.add_triangle(a, a + 1, a + 5);
            m.add_triangle(a, a + 5, a + 4);
        }
    }
    p.add(m);
}

// A scroll area's edges: the rows fade into the panel over 38 points where more content lies beyond the edge, with
// a small ▴ / ▾ mark, so no glyph ends sliced at the edge.
pub fn scroll_edges<R>(ui: &Ui, out: &egui::scroll_area::ScrollAreaOutput<R>) {
    const FADE: f32 = 28.0;
    // A solid strip at the edge, then the fade, so a cut line of text is fully hidden.
    const SOLID: f32 = 10.0;
    let r = out.inner_rect;
    let off = out.state.clamped_offset().y;
    let more_below = off + r.height() < out.content_size.y - 0.5;
    let p = ui.painter().with_clip_rect(r);
    let fade = |y0: f32, y1: f32, top_solid: bool| {
        let (a, b) = if top_solid {
            (PANEL, Color32::TRANSPARENT)
        } else {
            (Color32::TRANSPARENT, PANEL)
        };
        let mut m = egui::Mesh::default();
        m.colored_vertex(pos2(r.min.x, y0), a);
        m.colored_vertex(pos2(r.max.x, y0), a);
        m.colored_vertex(pos2(r.max.x, y1), b);
        m.colored_vertex(pos2(r.min.x, y1), b);
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        p.add(m);
    };
    let mark = |y: f32, down: bool| {
        let (cx, h) = (r.max.x - 11.0, 6.0);
        let (tip, base) = if down { (y + h, y) } else { (y - h, y) };
        p.add(egui::Shape::convex_polygon(
            vec![pos2(cx - h, base), pos2(cx + h, base), pos2(cx, tip)],
            HELP,
            egui::Stroke::NONE,
        ));
    };
    if off > 0.5 {
        p.rect_filled(
            Rect::from_x_y_ranges(r.x_range(), r.min.y..=r.min.y + SOLID),
            0.0,
            PANEL,
        );
        fade(r.min.y + SOLID, r.min.y + SOLID + FADE, true);
        mark(r.min.y + 10.0, false);
    }
    if more_below {
        p.rect_filled(
            Rect::from_x_y_ranges(r.x_range(), r.max.y - SOLID..=r.max.y),
            0.0,
            PANEL,
        );
        fade(r.max.y - SOLID - FADE, r.max.y - SOLID, false);
        mark(r.max.y - 14.0, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // WCAG: text 4.5:1 on every surface it is drawn on (panel, focus band, pressed band, stepper cell); the focus
    // tick, the knob and the inactive choices 3:1 (non-text UI, WCAG 1.4.11); disabled text is exempt.
    #[test]
    fn palette_contrast() {
        for bg in [PANEL, BAND, PRESSED, CELL, BG] {
            for fg in [TEXT_HI, TEXT, HELP, HELP_HI, ACCENT, DANGER, QUESTION, WARN] {
                assert!(contrast(fg, bg) >= 4.5, "{fg:?} on {bg:?}");
            }
            assert!(contrast(ACCENT, bg) >= 3.0, "tick on {bg:?}");
        }
        for bg in [PANEL, CELL] {
            assert!(contrast(DIM, bg) >= 3.0, "inactive choice on {bg:?}");
            assert!(contrast(FILL, bg) >= 3.0, "slider fill on {bg:?}");
        }
        assert!(contrast(NOTE, PANEL) >= 4.5);
        assert!(contrast(KEYCAP, PANEL) >= 4.5);
    }

    // Desktop windows: the OS scale up to 1.75× 640×480, growing to 1.5× at most; touch: the density.
    #[test]
    fn modality() {
        let mut i: Input<u8> = Input::new(Modality::Pointer);
        i.over(3);
        assert!(i.lit(3) && !i.bar());
        assert!(i.end());
        assert_eq!(i.hovered(), Some(3));
        // The pointer leaves: nothing stays lit, the help line falls back.
        assert!(!i.end());
        i.next = None;
        assert!(i.end());
        assert!(!i.lit(3));
        assert_eq!(i.hovered(), None);
        // A key press over a row: the bar appears on that row; the next press moves it.
        i.over(5);
        i.end();
        assert_eq!(i.key(), Some(Some(5)));
        assert!(i.bar() && !i.lit(5));
        assert_eq!(i.hovered(), None);
        assert_eq!(i.key(), None);
        // Pointer again: bar gone; a key press with no row hovered starts from the last focus.
        i.pointer(false);
        i.next = None;
        i.end();
        assert!(!i.bar());
        assert_eq!(i.key(), Some(None));
        // Touch: no hover band, no bar.
        i.pointer(true);
        i.over(2);
        assert!(!i.lit(2) && !i.bar());
        i.end();
        assert_eq!(i.hovered(), None);
    }

    #[test]
    fn scale() {
        assert_eq!(ppp(1120.0, 840.0, 1.0, false), 1.0);
        assert!((ppp(1280.0, 960.0, 1.0, false) - 2.0 / 1.75).abs() < 1e-4);
        assert!((ppp(2000.0, 1200.0, 1.0, false) - 2.5 / 1.75).abs() < 1e-4);
        assert_eq!(ppp(3840.0, 2160.0, 1.0, false), 1.5);
        assert!((ppp(3840.0, 2160.0, 2.0, false) - 2.0 * 4.5 / 3.5).abs() < 1e-4);
        assert_eq!(ppp(640.0, 480.0, 1.0, false), 1.0);
        assert_eq!(ppp(400.0, 300.0, 1.0, false), 0.625);
        assert_eq!(ppp(2400.0, 1080.0, 2.625, true), 2.625);
        assert_eq!(ppp(2560.0, 1600.0, 2.0, true), 1600.0 / 480.0);
    }
}
