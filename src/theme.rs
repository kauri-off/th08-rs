//! The port UI's design system (launcher, port menu, touch-layout editor): palette, type and spacing scales, scale per window, egui style, shared widgets.

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Margin, Response, RichText, Shadow, Stroke,
    StrokeKind, TextStyle, Ui, vec2,
};

// Night sky of the app icon: indigo surfaces, a violet accent, moon-gold focus. `tests::contrast` holds the pairs
// to WCAG (text 4.5:1, outlines / fills / rings 3:1).
pub const BG: Color32 = Color32::from_rgb(10, 11, 30);
pub const SURFACE: Color32 = Color32::from_rgb(20, 20, 48);
pub const STRIPE: Color32 = Color32::from_rgb(26, 26, 58);
pub const RAISED: Color32 = Color32::from_rgb(34, 34, 76);
pub const RAISED_HOVER: Color32 = Color32::from_rgb(46, 46, 98);
pub const OUTLINE: Color32 = Color32::from_rgb(112, 112, 170);
pub const TEXT: Color32 = Color32::from_rgb(238, 236, 248);
pub const TEXT_WEAK: Color32 = Color32::from_rgb(176, 174, 206);
pub const ACCENT: Color32 = Color32::from_rgb(100, 82, 220);
pub const FOCUS: Color32 = Color32::from_rgb(245, 214, 128);
pub const DANGER: Color32 = Color32::from_rgb(255, 138, 128);
pub const WARN: Color32 = Color32::from_rgb(240, 192, 96);
pub const SUCCESS: Color32 = Color32::from_rgb(124, 214, 150);
pub const INFO: Color32 = Color32::from_rgb(150, 176, 255);

// Dims the game picture under a modal page.
pub const SCRIM: Color32 = Color32::from_black_alpha(170);

// Type scale, points (= dp on touch screens).
pub const SMALL: f32 = 14.0;
pub const BODY: f32 = 16.0;
pub const TITLE: f32 = 20.0;
pub const HEADING: f32 = 24.0;

// Spacing scale, points.
pub const XS: f32 = 4.0;
pub const S: f32 = 8.0;
pub const M: f32 = 12.0;
pub const L: f32 = 16.0;
pub const XL: f32 = 24.0;

pub const RADIUS: u8 = 6;
pub const RADIUS_PANEL: u8 = 10;

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
        sp.interact_size = vec2(40.0, d.target);
        sp.item_spacing = vec2(S, if d.touch { 6.0 } else { S });
        sp.button_padding = vec2(M, XS);
        sp.window_margin = Margin::same(L as i8);
        sp.menu_margin = Margin::same(S as i8);
        sp.indent = L;
        sp.icon_width = if d.touch { 28.0 } else { 20.0 };
        sp.icon_width_inner = sp.icon_width * 0.6;
        sp.icon_spacing = S;
        sp.slider_rail_height = 6.0;
        sp.combo_width = 180.0;
        sp.text_edit_width = 240.0;
        let v = &mut st.visuals;
        v.dark_mode = true;
        v.override_text_color = None;
        v.weak_text_color = Some(TEXT_WEAK);
        v.hyperlink_color = INFO;
        v.window_fill = SURFACE;
        v.panel_fill = BG;
        v.faint_bg_color = STRIPE;
        v.extreme_bg_color = BG;
        v.text_edit_bg_color = Some(BG);
        v.code_bg_color = RAISED;
        v.warn_fg_color = WARN;
        v.error_fg_color = DANGER;
        v.window_stroke = Stroke::new(1.0, OUTLINE);
        v.window_corner_radius = CornerRadius::same(RADIUS_PANEL);
        v.menu_corner_radius = CornerRadius::same(RADIUS);
        v.window_shadow = Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha(140),
        };
        v.popup_shadow = Shadow {
            offset: [0, 4],
            blur: 10,
            spread: 0,
            color: Color32::from_black_alpha(120),
        };
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;
        v.selection.bg_fill = ACCENT;
        v.selection.stroke = Stroke::new(1.5, TEXT);
        v.text_cursor.stroke = Stroke::new(2.0, FOCUS);
        v.striped = false;
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = SURFACE;
        w.noninteractive.weak_bg_fill = SURFACE;
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        w.noninteractive.bg_stroke = Stroke::new(1.0, OUTLINE.gamma_multiply(0.6));
        w.noninteractive.corner_radius = CornerRadius::same(RADIUS);
        for (x, fill, stroke) in [
            (&mut w.inactive, RAISED, Stroke::new(1.0, OUTLINE)),
            (&mut w.hovered, RAISED_HOVER, Stroke::new(1.5, TEXT)),
            (&mut w.active, RAISED_HOVER, Stroke::new(2.0, TEXT)),
            (&mut w.open, RAISED_HOVER, Stroke::new(1.0, OUTLINE)),
        ] {
            x.bg_fill = fill;
            x.weak_bg_fill = fill;
            x.bg_stroke = stroke;
            x.fg_stroke = Stroke::new(1.5, TEXT);
            x.corner_radius = CornerRadius::same(RADIUS);
            x.expansion = 0.0;
        }
    });
}

// A modal panel: surface fill, outline, rounded, `L` padding.
pub fn panel() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, OUTLINE))
        .corner_radius(CornerRadius::same(RADIUS_PANEL))
        .inner_margin(Margin::same(L as i8))
}

// A bar at the edge of a full-window screen (the launcher's header and button bar).
pub fn bar() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .inner_margin(Margin::symmetric(M as i8, S as i8))
}

// A card in a list (a replay, a translation job).
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, OUTLINE.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(RADIUS))
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
    let text = RichText::new(text.into());
    match kind {
        Kind::Primary => egui::Button::new(text.color(TEXT).strong())
            .fill(ACCENT)
            .stroke(Stroke::new(1.0, ACCENT)),
        Kind::Secondary => egui::Button::new(text.color(TEXT)),
        Kind::Danger => egui::Button::new(text.color(DANGER)).stroke(Stroke::new(1.5, DANGER)),
    }
}

// A tab bar: unframed labels, the selected one bright with an accent underline. Wraps onto more lines when
// narrow, or scrolls sideways in one line with `scroll` (short touch screens). Returns the clicked tab and the
// selected tab's rect.
pub fn tabs(
    ui: &mut Ui,
    labels: &[&str],
    selected: usize,
    scroll: bool,
) -> (Option<usize>, Option<egui::Rect>) {
    let (mut clicked, mut rect) = (None, None);
    let mut row = |ui: &mut Ui| {
        ui.spacing_mut().item_spacing.x = XS;
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
    let color = if selected { TEXT } else { TEXT_WEAK };
    let mut t = RichText::new(text).color(color);
    if selected {
        t = t.strong();
    }
    let galley = egui::WidgetText::from(t).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Button,
    );
    let size = vec2(galley.size().x + 2.0 * M, h.max(galley.size().y + 2.0 * S));
    let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
    if r.hovered() && !selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(RADIUS), STRIPE);
    }
    ui.painter().galley(
        egui::Align2::CENTER_CENTER
            .align_size_within_rect(galley.size(), rect)
            .min,
        galley,
        TEXT,
    );
    let line = if selected {
        Some(Stroke::new(3.0, ACCENT))
    } else if r.hovered() {
        Some(Stroke::new(2.0, OUTLINE))
    } else {
        None
    };
    if let Some(s) = line {
        let y = rect.max.y - s.width / 2.0;
        ui.painter().hline(rect.x_range().shrink(XS), y, s);
    }
    r
}

// The keyboard / pad focus ring around `rect`; `inside` for rows clipped by a scroll area.
pub fn ring(ui: &Ui, rect: egui::Rect, inside: bool) {
    let (rect, kind) = if inside {
        (rect, StrokeKind::Inside)
    } else {
        (rect.expand(2.0), StrokeKind::Outside)
    };
    ui.painter()
        .rect_stroke(rect, f32::from(RADIUS) + 2.0, Stroke::new(3.0, FOCUS), kind);
}

// Weak small text (hints, help, version).
pub fn hint(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).small().color(TEXT_WEAK)
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

    // WCAG: every text colour 4.5:1 on every background it is drawn on; outlines, the accent fill and the focus
    // ring 3:1 against the surfaces they sit on.
    #[test]
    fn palette_contrast() {
        for bg in [BG, SURFACE, STRIPE, RAISED, RAISED_HOVER] {
            for fg in [TEXT, TEXT_WEAK, DANGER, WARN, SUCCESS, INFO] {
                assert!(contrast(fg, bg) >= 4.5, "{fg:?} on {bg:?}");
            }
            assert!(contrast(FOCUS, bg) >= 3.0, "focus on {bg:?}");
        }
        assert!(contrast(TEXT, ACCENT) >= 4.5);
        assert!(contrast(FOCUS, ACCENT) >= 3.0);
        for bg in [BG, SURFACE] {
            assert!(contrast(OUTLINE, bg) >= 3.0, "outline on {bg:?}");
            assert!(contrast(ACCENT, bg) >= 3.0, "accent on {bg:?}");
        }
        assert!(contrast(OUTLINE, RAISED) >= 3.0);
        assert!(contrast(TEXT, RAISED_HOVER) >= 3.0);
    }

    // Desktop windows: the OS scale up to 1.75× 640×480, growing to 1.5× at most; touch: the density.
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
