//! Text_RenderToVm 0x4663B0 rasteriser (10 §6): 2× work surface, 4-offset black outline, colour at the centre, exact D3DX TRIANGLE 2:1 into A4R4G4B4.
//! Renderer side (ARCH_SUMMARY decision 7): `Gfx` runs it for `ResCmd::Text`; core never rasterises or reads fonts.
//! SPEC-GAP: GDI "ＭＳ ゴシック" pixels are not reproducible; a system Japanese font (native) or the browser's canvas font (web) is used.

use th08_core::drawlist::Align;

// Port font metric, no spec section: baseline of the glyph cell as a fraction of the cell height (MS Gothic ascent ≈ 0.86 em).
const BASELINE: f32 = 0.86;

// `Default`: no font (blank text); `Raster::new` does the font search.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
pub struct Raster {
    font: Option<Face>,
    // Presentation-pack FileFont chain (ARCH_B §1.6, I18N §2.3): translated text only, first font with the glyph
    // wins, the system font last.
    pack: Vec<Face>,
}

// Port-only font search order; TH08_FONT overrides it.
#[cfg(not(target_arch = "wasm32"))]
pub const FONT_PATHS: &[&str] = &[
    "C:\\Windows\\Fonts\\msgothic.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Medium.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Medium.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/droid/DroidSansJapanese.ttf",
    "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
    "/System/Library/Fonts/ヒラギノ角ゴシック W6.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    // Android system CJK fonts (ARCH_D_MOBILE §6.2): AOSP 7+, older AOSP, some OEM builds.
    "/system/fonts/NotoSansCJK-Regular.ttc",
    "/system/fonts/NotoSansCJKjp-Regular.otf",
    "/system/fonts/NotoSansJP-Regular.otf",
    "/system/fonts/DroidSansJapanese.ttf",
    "/system/fonts/DroidSansFallback.ttf",
    "/system/fonts/DroidSansFallbackFull.ttf",
];

// A pack font file (TTF / OTF, first face of a collection); None with a warning when it does not parse.
pub fn file_font(data: Vec<u8>) -> Option<Face> {
    ab_glyph::FontVec::try_from_vec_and_index(data, 0)
        .map(Face::new)
        .map_err(|e| log::warn!("pack font: {e}"))
        .ok()
}

// A parsed face; system fonts borrow the process-wide bytes, pack fonts own theirs.
pub type Face = ab_glyph::FontArc;

// Each system font file is read once per process and kept for its lifetime: the launcher, the overlays and game
// text share the bytes.
#[cfg(not(target_arch = "wasm32"))]
static READ: std::sync::Mutex<Vec<(String, &'static [u8])>> = std::sync::Mutex::new(Vec::new());

#[cfg(not(target_arch = "wasm32"))]
fn read_once(path: &str) -> Option<&'static [u8]> {
    let mut read = READ
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, b)) = read.iter().find(|(p, _)| p == path) {
        return Some(b);
    }
    let bytes: &'static [u8] = Vec::leak(std::fs::read(path).ok()?);
    log::info!("CJK font: {path} ({} KiB)", bytes.len() / 1024);
    read.push((path.to_owned(), bytes));
    Some(bytes)
}

// The first of `font` (TH08_FONT) and FONT_PATHS that reads and parses; the file stays in memory once read.
#[cfg(not(target_arch = "wasm32"))]
pub fn system_font(font: Option<&str>) -> Option<&'static [u8]> {
    font.into_iter()
        .chain(FONT_PATHS.iter().copied())
        .find_map(|p| {
            read_once(p).filter(|b| ab_glyph::FontRef::try_from_slice_and_index(b, 0).is_ok())
        })
}

#[cfg(not(target_arch = "wasm32"))]
impl Raster {
    // `font` = TH08_FONT (`CoreConfig::font`, via `Renderer::set_font`), tried before FONT_PATHS; `pack` = the
    // mounted pack's font files in chain order.
    pub fn new(font: Option<&str>, pack: Vec<Vec<u8>>) -> Self {
        let font = system_font(font)
            .and_then(|b| ab_glyph::FontRef::try_from_slice_and_index(b, 0).ok())
            .map(Face::new);
        if font.is_none() {
            log::warn!("no Japanese font found (set TH08_FONT); dialogue text stays blank");
        }
        Self {
            font,
            pack: pack.into_iter().filter_map(file_font).collect(),
        }
    }

    // Vanilla text: the system font only, exactly as before packs existed.
    fn mask(&mut self, s: &str, cell: f32, x: f32, y: f32, w: usize, h: usize) -> Vec<u8> {
        let mut out = vec![0u8; w * h];
        let fonts: Vec<&Face> = self.font.iter().collect();
        glyph_mask(&fonts, &Run::plain(s, cell, x, y), &mut out, w, h);
        out
    }

    // Translated text: pack font first, the system font for the glyphs it lacks.
    fn mask_runs(&mut self, runs: &[Run], w: usize, h: usize) -> Vec<u8> {
        let mut out = vec![0u8; w * h];
        let fonts: Vec<&Face> = self.pack.iter().chain(self.font.iter()).collect();
        for r in runs {
            glyph_mask(&fonts, r, &mut out, w, h);
        }
        out
    }

    fn measure(&mut self, s: &str, cell: f32) -> f32 {
        let fonts: Vec<&Face> = self.pack.iter().chain(self.font.iter()).collect();
        glyph_width(&fonts, s, cell)
    }

    // pack-check (I18N G3): the first pack-chain font with a glyph for `ch` (index), and whether the system font has
    // one (None = no system font).
    pub fn has_glyph(&self, ch: char) -> (Option<usize>, Option<bool>) {
        use ab_glyph::Font;
        let has = |f: &Face| f.glyph_id(ch).0 != 0;
        (self.pack.iter().position(has), self.font.as_ref().map(has))
    }

    pub fn pack_fonts(&self) -> usize {
        self.pack.len()
    }

    // pack-check: measured width of translated text over the room `render_layout` gives it in a strip whose sprite
    // ends at `edge`, font size `size` (> 1 = the overflow mode applies; > 1 / MIN_FIT = still too wide after fitting).
    pub fn fit_ratio(&mut self, s: &str, size: usize, (align, x, edge): (Align, f32, f32)) -> f32 {
        let cell = (2 * size - 2) as f32;
        self.measure(s, cell) / (2.0 * avail(align, x, edge))
    }
}

// Smallest fitted scale, for pack-check (`fit_ratio` above 1 / this is cut even after shrink / squeeze).
pub const MIN_FIT_SCALE: f32 = MIN_FIT;

// One line of translated text in 2× work-surface pixels: glyph cell height, horizontal scale, pen start, cell top.
struct Run<'a> {
    s: &'a str,
    cell: f32,
    sx: f32,
    x: f32,
    y: f32,
}

impl<'a> Run<'a> {
    fn plain(s: &'a str, cell: f32, x: f32, y: f32) -> Self {
        Self {
            s,
            cell,
            sx: 1.0,
            x,
            y,
        }
    }
}

// The first font of the stack that has a glyph for `ch` (else the first font's .notdef).
fn pick<'f>(fonts: &[&'f Face], ch: char) -> Option<&'f Face> {
    use ab_glyph::Font;
    fonts
        .iter()
        .find(|f| f.glyph_id(ch).0 != 0)
        .or(fonts.first())
        .copied()
}

// Glyph scale of `font` for a cell `cell` px high (the em box fills the cell like GDI's character height).
fn font_scale(font: &Face, cell: f32, sx: f32) -> ab_glyph::PxScale {
    use ab_glyph::Font;
    let em = font.units_per_em().unwrap_or(1000.0);
    let v = cell * font.height_unscaled() / em;
    ab_glyph::PxScale { x: v * sx, y: v }
}

// Advance width of `s` at cell height `cell`, in the same pixels.
fn glyph_width(fonts: &[&Face], s: &str, cell: f32) -> f32 {
    use ab_glyph::{Font, ScaleFont};
    s.chars()
        .filter_map(|ch| {
            let f = pick(fonts, ch)?;
            Some(
                f.as_scaled(font_scale(f, cell, 1.0))
                    .h_advance(f.glyph_id(ch)),
            )
        })
        .sum()
}

// Coverage of run `r` max-blended into the w×h buffer `out`.
fn glyph_mask(fonts: &[&Face], r: &Run, out: &mut [u8], w: usize, h: usize) {
    use ab_glyph::{Font, ScaleFont, point};
    let mut pen = r.x;
    for ch in r.s.chars() {
        let Some(font) = pick(fonts, ch) else { return };
        let scale = font_scale(font, r.cell, r.sx);
        let id = font.glyph_id(ch);
        let g = id.with_scale_and_position(scale, point(pen, r.y + r.cell * BASELINE));
        pen += font.as_scaled(scale).h_advance(id);
        let Some(o) = font.outline_glyph(g) else {
            continue;
        };
        let b = o.px_bounds();
        o.draw(|gx, gy, c| {
            let (px, py) = (
                b.min.x as i32 + gx.cast_signed(),
                b.min.y as i32 + gy.cast_signed(),
            );
            if let (Ok(px), Ok(py)) = (usize::try_from(px), usize::try_from(py))
                && px < w
                && py < h
            {
                let v = &mut out[py * w + px];
                *v = (*v).max(coverage_u8(c));
            }
        });
    }
}

// Glyph coverage 0 … 1 as a byte (port raster, truncating like the original's ftol).
#[expect(clippy::cast_sign_loss, reason = "clamped to 0 … 255 first")]
fn coverage_u8(c: f32) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0) as u8
}

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub struct Raster {
    canvas: Option<(
        web_sys::HtmlCanvasElement,
        web_sys::CanvasRenderingContext2d,
    )>,
    // Presentation-pack FileFont chain: the same pixels as native for translated text (ARCH_B §1.6).
    pack: Vec<Face>,
}

#[cfg(target_arch = "wasm32")]
impl Raster {
    // The browser's canvas font; TH08_FONT is native only.
    pub fn new(_font: Option<&str>, pack: Vec<Vec<u8>>) -> Self {
        Self {
            canvas: None,
            pack: pack.into_iter().filter_map(file_font).collect(),
        }
    }

    fn ctx(&mut self, w: usize, h: usize) -> Option<&web_sys::CanvasRenderingContext2d> {
        use wasm_bindgen::JsCast;
        if self.canvas.is_none() {
            let doc = web_sys::window().and_then(|w| w.document());
            let c = doc
                .and_then(|d| d.create_element("canvas").ok())
                .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());
            let ctx = c
                .as_ref()
                .and_then(|c| c.get_context("2d").ok().flatten())
                .and_then(|x| x.dyn_into::<web_sys::CanvasRenderingContext2d>().ok());
            if let (Some(c), Some(ctx)) = (c, ctx) {
                self.canvas = Some((c, ctx));
            }
        }
        let (c, ctx) = self.canvas.as_ref()?;
        if c.width() as usize != w || c.height() as usize != h {
            c.set_width(w as u32);
            c.set_height(h as u32);
        }
        Some(ctx)
    }

    fn set_font(ctx: &web_sys::CanvasRenderingContext2d, cell: f32) {
        ctx.set_font(&format!(
            "600 {cell}px \"MS Gothic\", \"Noto Sans CJK JP\", \"Hiragino Sans\", monospace"
        ));
    }

    fn mask(&mut self, s: &str, cell: f32, x: f32, y: f32, w: usize, h: usize) -> Vec<u8> {
        self.canvas_runs(&[Run::plain(s, cell, x, y)], w, h)
    }

    fn canvas_runs(&mut self, runs: &[Run], w: usize, h: usize) -> Vec<u8> {
        let mut out = vec![0u8; w * h];
        let Some(ctx) = self.ctx(w, h) else {
            return out;
        };
        ctx.clear_rect(0.0, 0.0, w as f64, h as f64);
        ctx.set_fill_style_str("#fff");
        for r in runs {
            Self::set_font(ctx, r.cell);
            let _ = ctx.set_transform(f64::from(r.sx), 0.0, 0.0, 1.0, f64::from(r.x), 0.0);
            let _ = ctx.fill_text(r.s, 0.0, f64::from(r.y + r.cell * BASELINE));
        }
        let _ = ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        if let Ok(img) = ctx.get_image_data(0.0, 0.0, w as f64, h as f64) {
            for (o, px) in out.iter_mut().zip(img.data().chunks_exact(4)) {
                *o = px[3];
            }
        }
        out
    }

    fn mask_runs(&mut self, runs: &[Run], w: usize, h: usize) -> Vec<u8> {
        if self.pack.is_empty() {
            return self.canvas_runs(runs, w, h);
        }
        let fonts: Vec<&Face> = self.pack.iter().collect();
        let mut out = vec![0u8; w * h];
        for r in runs {
            glyph_mask(&fonts, r, &mut out, w, h);
        }
        out
    }

    fn measure(&mut self, s: &str, cell: f32) -> f32 {
        if !self.pack.is_empty() {
            let fonts: Vec<&Face> = self.pack.iter().collect();
            return glyph_width(&fonts, s, cell);
        }
        let Some(ctx) = self.ctx(1, 1) else {
            return 0.0;
        };
        Self::set_font(ctx, cell);
        ctx.measure_text(s).map_or(0.0, |m| m.width() as f32)
    }
}

// Overflow mode of translated text that is wider than its strip (I18N §2.3, ARCH_B §1.5); port-only presentation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    // Cut at the strip edge.
    Clip,
    // Smaller glyphs (down to half size), centred in the strip rows.
    #[default]
    Shrink,
    // Narrower glyphs (horizontal scale down to one half).
    Squeeze,
    // Two lines of half height in the same strip rows, each squeezed if still too wide.
    Wrap,
}

impl Overflow {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "clip" => Self::Clip,
            "shrink" => Self::Shrink,
            "squeeze" => Self::Squeeze,
            "wrap" => Self::Wrap,
            _ => return None,
        })
    }
}

// Smallest scale a fitted line gets (shrink, squeeze, wrap).
const MIN_FIT: f32 = 0.5;

// Free width in strip pixels for a placement in a sprite whose right edge is `edge`.
fn avail(align: Align, x: f32, edge: f32) -> f32 {
    match align {
        Align::Left => edge - x,
        Align::Centre(c) => 2.0 * c.min(edge - c),
        Align::Right(e) => e,
    }
    .max(1.0)
}

// Pen start in strip pixels for a line `width` wide (measured; truncated like the vanilla formulas).
fn place(align: Align, x: f32, width: f32) -> f32 {
    match align {
        Align::Left => x,
        Align::Centre(c) => (c - width / 2.0).trunc().max(0.0),
        Align::Right(e) => (e - width).trunc().max(0.0),
    }
}

// Split for `Wrap`: the space nearest the middle, else the middle character.
fn split_two(s: &str) -> (&str, &str) {
    let mid = s.chars().count() / 2;
    let idx: Vec<usize> = s.char_indices().map(|(i, _)| i).collect();
    let best = s
        .char_indices()
        .filter(|(_, c)| c.is_whitespace())
        .min_by_key(|(i, _)| idx.iter().position(|j| j == i).unwrap_or(0).abs_diff(mid))
        .map(|(i, _)| i);
    match best {
        Some(i) => (s[..i].trim_end(), s[i..].trim_start()),
        None => s.split_at(idx.get(mid).copied().unwrap_or(s.len())),
    }
}

impl Raster {
    // One text row, RGBA w×size (10 §6): GDI height 2·size − 2 at 2×, outline ±2 px (2× space), then 2:1 downsample.
    pub fn render(&mut self, s: &str, color: [u8; 3], w: usize, size: usize) -> Vec<u8> {
        self.render_at(s, color, w, size, 0.0)
    }

    // As `render`, with the text starting at x (strip pixels).
    pub fn render_at(&mut self, s: &str, color: [u8; 3], w: usize, size: usize, x: f32) -> Vec<u8> {
        self.render_outline(s, color, w, size, x, 0)
    }

    // As `render_at` with the outline argument: −1 → ±1 px offsets (2× space), else ±2 px (10 §6).
    pub fn render_outline(
        &mut self,
        s: &str,
        color: [u8; 3],
        w: usize,
        size: usize,
        x: f32,
        outline: i32,
    ) -> Vec<u8> {
        let (w2, h2) = work_size(w, size);
        let mask = self.mask(s, (2 * size - 2) as f32, 2.0 * x, 0.0, w2, h2);
        finish(&mask, color, w, size, outline)
    }

    // Translated text (I18N §2.3): placed by its measured width (`align`, `x` for Left) and fitted by `overflow` to
    // the visible sprite (`edge`, not the texture row); the 2× surface, outline, gradient and 2:1 filter are the vanilla ones (10 §6).
    #[expect(
        clippy::too_many_arguments,
        reason = "the vanilla text arguments plus placement and overflow"
    )]
    pub fn render_layout(
        &mut self,
        s: &str,
        color: [u8; 3],
        w: usize,
        size: usize,
        outline: i32,
        (align, x, edge): (Align, f32, f32),
        overflow: Overflow,
    ) -> Vec<u8> {
        let (w2, h2) = work_size(w, size);
        let cell = (2 * size - 2) as f32;
        let room = 2.0 * avail(align, x, edge);
        let fit = |width: f32| (room / width.max(1.0)).clamp(MIN_FIT, 1.0);
        let mut runs = Vec::new();
        let width = self.measure(s, cell);
        let lines: Vec<&str> = if overflow == Overflow::Wrap && width > room {
            let (a, b) = split_two(s);
            vec![a, b]
        } else {
            vec![s]
        };
        let n = lines.len() as f32;
        for (i, line) in lines.into_iter().enumerate() {
            let (c, top) = if n > 1.0 {
                (cell / n, cell / n * i as f32)
            } else {
                (cell, 0.0)
            };
            let lw = self.measure(line, c);
            let (c, sx, top) = match overflow {
                Overflow::Shrink if lw > room => {
                    let k = fit(lw);
                    (c * k, 1.0, top + c * (1.0 - k) / 2.0)
                }
                Overflow::Squeeze | Overflow::Wrap if lw > room => (c, fit(lw), top),
                _ => (c, 1.0, top),
            };
            let drawn = self.measure(line, c) * sx;
            runs.push(Run {
                s: line,
                cell: c,
                sx,
                x: 2.0 * place(align, x, drawn / 2.0),
                y: top,
            });
        }
        let mask = self.mask_runs(&runs, w2, h2);
        finish(&mask, color, w, size, outline)
    }
}

// The 2× work rect: a fresh 1024-wide A1R5G5B5 DIB per call, copied as (0, 0, min(2w, 1024), 2·size) (10 §6).
fn work_size(w: usize, size: usize) -> (usize, usize) {
    ((2 * w).min(DIB_W), 2 * size + 6)
}

// 10 §6 steps 2–5 over a coverage mask of the work rect: outline, colour, gradient, TRIANGLE 2:1, RGBA w×size.
fn finish(mask: &[u8], color: [u8; 3], w: usize, size: usize, outline: i32) -> Vec<u8> {
    let o = if outline == -1 { 1 } else { 2 };
    let (w2, h2) = work_size(w, size);
    let mut dib = vec![Px::default(); w2 * h2];
    // Outline always black at (2x+2±o, 2), (2x+2, 2±o), then the colour at the centre (2x+2, 2) (10 §6).
    let edge = [0; 3];
    for (dx, dy, c) in [
        (2 + o, 2, edge),
        (2 - o, 2, edge),
        (2, 2 - o, edge),
        (2, 2 + o, edge),
        (2, 2, color),
    ] {
        for y in 0..h2 - dy {
            for x in 0..w2 - dx {
                let a = mask[y * w2 + x];
                if a != 0 {
                    dib[(y + dy) * w2 + x + dx].text_out(c, a);
                }
            }
        }
    }
    gradient(&mut dib, w2, h2, outline == -1);
    let out = triangle_2to1(&dib, w2, 2 * size);
    let mut rgba = vec![0u8; w * size * 4];
    for y in 0..size {
        for x in 0..w2 / 2 {
            let p = out[y * (w2 / 2) + x];
            let d = &mut rgba[(y * w + x) * 4..][..4];
            // A4R4G4B4 texel as sampled: 4-bit channels widened by ×17.
            for (k, v) in [p >> 8 & 15, p >> 4 & 15, p & 15, p >> 12]
                .into_iter()
                .enumerate()
            {
                d[k] = (v * 17) as u8;
            }
        }
    }
    rgba
}

// Rasteriser DIB row width in pixels (10 §6 step 1).
const DIB_W: usize = 1024;

// One A1R5G5B5 DIB pixel: 5-bit R, G, B and the alpha bit (10 §6 steps 1–4).
#[derive(Clone, Copy, Default)]
struct Px {
    rgb: [u32; 3],
    alpha: bool,
}

impl Px {
    // A GDI antialiased TextOut sample: blend over the pixel, 15-bit result. The two alpha-bit inversions (steps 2, 4)
    // leave exactly the GDI-touched pixels opaque (1-bit alpha). Coverage and blend are platform, like the glyphs.
    fn text_out(&mut self, c: [u8; 3], a: u8) {
        let a = u32::from(a);
        for (d, c) in self.rgb.iter_mut().zip(c) {
            let d8 = *d << 3 | *d >> 2;
            *d = ((u32::from(c) * a + d8 * (255 - a)) / 255) >> 3;
        }
        self.alpha = true;
    }
}

// Post-pass 0x43F31C (10 §6 step 4): alpha 0 → colour 0; alpha 1 → vertical gradient in 5-bit integer arithmetic.
// c / N are byte offsets in the 1024-pixel-row DIB over the region rows 0 … h−1 (exact for the 512-wide text
// textures; a narrower strip gets the left part of that region).
fn gradient(dib: &mut [Px], w: usize, h: usize, flag: bool) {
    let n = (2 * DIB_W * h) as u32;
    for y in 0..h {
        for x in 0..w {
            let p = &mut dib[y * w + x];
            if !p.alpha {
                p.rgb = [0; 3];
                continue;
            }
            let c = (2 * (y * DIB_W + x)) as u32;
            let [r, g, b] = &mut p.rgb;
            if *b > *r {
                let d = if flag { 4 } else { 2 };
                *b -= *b * c / n / d;
                *g -= *g * c / n / d;
            } else if flag {
                *r -= *r * c / n / 4;
                *g -= *g * c / n / 4;
            } else {
                *r -= *r * c * 2 / n / 3;
                *g -= *g * c * 2 / n / 3;
            }
            p.rgb = p.rgb.map(|v| v.min(31));
        }
    }
}

// D3DX filter TRIANGLE at exactly 2:1 (10 §6 "TRIANGLE 2:1"): source rows 0 … sh−1 of the w-wide DIB into A4R4G4B4
// (returned as u16 0xARGB). Unweighted channels, taps 2j−1 … 2j+2 with weights 1/16, 7/16, 7/16, 1/16 wrapping in the
// source rect, f32 sums in source (row, column) order, trunc(acc·15 + 0.5).
#[expect(clippy::cast_sign_loss, reason = "trunc of a non-negative sum (10 §6)")]
fn triangle_2to1(src: &[Px], w: usize, sh: usize) -> Vec<u16> {
    const W: [f32; 4] = [1.0 / 16.0, 7.0 / 16.0, 7.0 / 16.0, 1.0 / 16.0];
    let taps = |j: usize, n: usize| {
        let mut t: [(usize, f32); 4] = std::array::from_fn(|k| ((2 * j + n + k - 1) % n, W[k]));
        t.sort_by_key(|&(i, _)| i);
        t
    };
    let k31 = 1.0f32 / 31.0;
    let (dw, dh) = (w / 2, sh / 2);
    let mut out = vec![0u16; dw * dh];
    for j in 0..dh {
        let ty = taps(j, sh);
        for i in 0..dw {
            let tx = taps(i, w);
            let mut acc = [0f32; 4];
            for &(sy, wy) in &ty {
                for &(sx, wx) in &tx {
                    let p = src[sy * w + sx];
                    let wt = wy * wx;
                    for (a, &c) in acc.iter_mut().zip(&p.rgb) {
                        *a += wt * (c as f32 * k31);
                    }
                    acc[3] += wt * if p.alpha { 1.0 } else { 0.0 };
                }
            }
            let q = acc.map(|a| (a * 15.0 + 0.5) as u16);
            out[j * dw + i] = q[3] << 12 | q[0] << 8 | q[1] << 4 | q[2];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white() -> Px {
        Px {
            rgb: [31; 3],
            alpha: true,
        }
    }

    #[test]
    fn placement() {
        assert_eq!(avail(Align::Left, 20.0, 320.0), 300.0);
        assert_eq!(avail(Align::Centre(256.0), 0.0, 512.0), 512.0);
        assert_eq!(avail(Align::Right(320.0), 0.0, 512.0), 320.0);
        assert_eq!(place(Align::Left, 7.0, 99.0), 7.0);
        assert_eq!(place(Align::Centre(256.0), 0.0, 101.0), 205.0);
        assert_eq!(place(Align::Right(320.0), 0.0, 100.5), 219.0);
        assert_eq!(place(Align::Right(320.0), 0.0, 400.0), 0.0);
        assert_eq!(split_two("Hello there, world!"), ("Hello there,", "world!"));
        assert_eq!(split_two("abcd"), ("ab", "cd"));
        assert_eq!(Overflow::from_name("wrap"), Some(Overflow::Wrap));
        assert_eq!(Overflow::from_name("x"), None);
    }

    // With no translation the layout path is not taken; with one, measured placement moves the ink (needs a
    // system font, skipped without one).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn layout_right_aligns_by_measured_width() {
        let mut r = Raster::new(None, Vec::new());
        if r.font.is_none() {
            return;
        }
        let ink_x = |rgba: &[u8], w: usize| {
            (0..w)
                .filter(|x| rgba.chunks_exact(4 * w).any(|row| row[x * 4 + 3] != 0))
                .collect::<Vec<_>>()
        };
        let w = 512;
        let right = r.render_layout(
            "Abc",
            [255; 3],
            w,
            15,
            0,
            (Align::Right(320.0), 0.0, 512.0),
            Overflow::Clip,
        );
        let xs = ink_x(&right, w);
        assert!(
            *xs.last().unwrap() <= 322 && *xs.last().unwrap() >= 300,
            "{xs:?}"
        );
        let long = "Long translated text that does not fit into the strip at all, really";
        for mode in [Overflow::Shrink, Overflow::Squeeze, Overflow::Wrap] {
            let fit = r.render_layout(
                long,
                [255; 3],
                w,
                15,
                0,
                (Align::Right(320.0), 0.0, 512.0),
                mode,
            );
            let xs = ink_x(&fit, w);
            assert!(*xs.last().unwrap() <= 322, "{mode:?} {xs:?}");
            let left = r.render_layout(long, [255; 3], w, 15, 0, (Align::Left, 0.0, 320.0), mode);
            // Column 0 also feeds column w − 1 (TRIANGLE taps wrap, 10 §6).
            let xs = ink_x(&left, w);
            assert!(
                xs.iter().all(|&x| x <= 322 || x == w - 1),
                "{mode:?} {xs:?}"
            );
        }
        let clip = r.render_layout(
            long,
            [255; 3],
            w,
            15,
            0,
            (Align::Left, 0.0, 512.0),
            Overflow::Clip,
        );
        assert!(*ink_x(&clip, w).last().unwrap() > 400);
    }

    // 10 §6: one white opaque source pixel → 0x3333 in its destination pixel, nothing elsewhere.
    #[test]
    fn triangle_impulse() {
        let (w, h) = (16, 8);
        let mut src = vec![Px::default(); w * h];
        src[2 * w + 3] = white();
        let out = triangle_2to1(&src, w, h);
        for (k, &v) in out.iter().enumerate() {
            assert_eq!(v, if k == w / 2 + 1 { 0x3333 } else { 0 }, "{k}");
        }
    }

    // Unweighted channels: an alpha-0 white pixel still adds colour; taps wrap at the rect edges.
    #[test]
    fn triangle_unweighted_wrap() {
        let (w, h) = (8, 4);
        let src = vec![
            Px {
                rgb: [31; 3],
                alpha: false
            };
            w * h
        ];
        assert!(triangle_2to1(&src, w, h).iter().all(|&v| v == 0x0FFF));
        // Source column 0 feeds destination column dw−1 with 1/16 (wrap) and column 0 with 7/16.
        let mut src = vec![Px::default(); w * h];
        for y in 0..h {
            src[y * w] = Px {
                rgb: [0; 3],
                alpha: true,
            };
        }
        let out = triangle_2to1(&src, w, h);
        for row in out.chunks_exact(w / 2) {
            assert_eq!(row, [0x7000, 0, 0, 0x1000]);
        }
    }

    // 10 §6 step 4: gradient on opaque pixels only; B > R loses up to ½ of B and G at the region bottom.
    #[test]
    fn gradient_rows() {
        let (w, h) = (1024, 36);
        let mut dib = vec![Px::default(); w * h];
        dib[0] = white();
        dib[(h - 1) * w] = Px {
            rgb: [0, 31, 31],
            alpha: true,
        };
        dib[(h - 1) * w + 1] = Px {
            rgb: [31, 31, 0],
            alpha: true,
        };
        dib[1].rgb = [5; 3];
        gradient(&mut dib, w, h, false);
        assert_eq!(dib[0].rgb, [31; 3]);
        assert_eq!(dib[1].rgb, [0; 3]);
        assert_eq!(dib[(h - 1) * w].rgb, [0, 16, 16]);
        assert_eq!(dib[(h - 1) * w + 1].rgb, [11, 11, 0]);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod font_tests {
    // The second lookup returns the same bytes, so the file was read once.
    #[test]
    fn system_font_read_once() {
        if let Some(a) = super::system_font(None) {
            assert!(std::ptr::eq(a, super::system_font(None).unwrap()));
        }
    }
}
