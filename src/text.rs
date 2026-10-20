//! Text_RenderToVm 0x4663B0 rasteriser (10 §6): 2× work surface, 4-offset black outline, colour at the centre, 2:1 box downsample.
//! SPEC-GAP: GDI "ＭＳ ゴシック" pixels are not reproducible; a system Japanese font (native) or the browser's canvas font (web) is used.

// Msg strings and spell names are Shift-JIS (cp932) (10 §2).
pub fn decode_sjis(bytes: &[u8]) -> String {
    encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned()
}

// Port font metric, no spec section: baseline of the glyph cell as a fraction of the cell height (MS Gothic ascent ≈ 0.86 em).
const BASELINE: f32 = 0.86;

#[cfg(not(target_arch = "wasm32"))]
pub struct Raster {
    font: Option<ab_glyph::FontVec>,
}

// Port-only font search order; TH08_FONT overrides it.
#[cfg(not(target_arch = "wasm32"))]
const FONT_PATHS: &[&str] = &[
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
];

#[cfg(not(target_arch = "wasm32"))]
impl Default for Raster {
    fn default() -> Self {
        let env = crate::env::var("TH08_FONT");
        let font = env
            .iter()
            .map(String::as_str)
            .chain(FONT_PATHS.iter().copied())
            .find_map(|p| {
                let data = std::fs::read(p).ok()?;
                ab_glyph::FontVec::try_from_vec_and_index(data, 0).ok()
            });
        if font.is_none() {
            log::warn!("no Japanese font found (set TH08_FONT); dialogue text stays blank");
        }
        Self { font }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Raster {
    // Coverage of `s` in a w×h buffer; the glyph cell (height `cell`) has its top-left at (x, y).
    fn mask(&mut self, s: &str, cell: f32, x: f32, y: f32, w: usize, h: usize) -> Vec<u8> {
        use ab_glyph::{Font, ScaleFont, point};
        let mut out = vec![0u8; w * h];
        let Some(font) = &self.font else { return out };
        let em = font.units_per_em().unwrap_or(1000.0);
        let scale = ab_glyph::PxScale::from(cell * font.height_unscaled() / em);
        let sf = font.as_scaled(scale);
        let mut pen = x;
        for ch in s.chars() {
            let id = font.glyph_id(ch);
            let g = id.with_scale_and_position(scale, point(pen, y + cell * BASELINE));
            pen += sf.h_advance(id);
            let Some(o) = font.outline_glyph(g) else {
                continue;
            };
            let b = o.px_bounds();
            o.draw(|gx, gy, c| {
                let (px, py) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                    let v = &mut out[py as usize * w + px as usize];
                    *v = (*v).max((c.clamp(0.0, 1.0) * 255.0) as u8);
                }
            });
        }
        out
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub struct Raster {
    canvas: Option<(
        web_sys::HtmlCanvasElement,
        web_sys::CanvasRenderingContext2d,
    )>,
}

#[cfg(target_arch = "wasm32")]
impl Raster {
    fn mask(&mut self, s: &str, cell: f32, x: f32, y: f32, w: usize, h: usize) -> Vec<u8> {
        use wasm_bindgen::JsCast;
        let mut out = vec![0u8; w * h];
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
        let Some((c, ctx)) = &self.canvas else {
            return out;
        };
        if c.width() as usize != w || c.height() as usize != h {
            c.set_width(w as u32);
            c.set_height(h as u32);
        }
        ctx.clear_rect(0.0, 0.0, w as f64, h as f64);
        ctx.set_font(&format!(
            "600 {cell}px \"MS Gothic\", \"Noto Sans CJK JP\", \"Hiragino Sans\", monospace"
        ));
        ctx.set_fill_style_str("#fff");
        let _ = ctx.fill_text(s, x as f64, (y + cell * BASELINE) as f64);
        if let Ok(img) = ctx.get_image_data(0.0, 0.0, w as f64, h as f64) {
            for (o, px) in out.iter_mut().zip(img.data().chunks_exact(4)) {
                *o = px[3];
            }
        }
        out
    }
}

impl Raster {
    // One text row, RGBA w×size (10 §6): GDI height 2·size − 2 at 2×, outline ±2 px (2× space), then 2:1 downsample.
    pub fn render(&mut self, s: &str, color: [u8; 3], w: usize, size: usize) -> Vec<u8> {
        self.render_at(s, color, w, size, 0.0)
    }

    // As `render`, with the text starting at x (strip pixels).
    pub fn render_at(&mut self, s: &str, color: [u8; 3], w: usize, size: usize, x: f32) -> Vec<u8> {
        // 1024×64 work surface, cleared over (0, 0, 2w, 2·size + 6) (10 §6).
        let (w2, h2) = ((2 * w).min(1024), 2 * size + 6);
        let mask = self.mask(s, (2 * size - 2) as f32, 2.0 * x, 0.0, w2, h2);
        let mut work = vec![[0f32; 4]; w2 * h2];
        let black = [0.0, 0.0, 0.0];
        let col = color.map(|c| c as f32 / 255.0);
        // Black at (2x+4, 2), (2x, 2), (2x+2, 0), (2x+2, 4), then the colour at the centre (2x+2, 2) (10 §6, outline 0).
        for (dx, dy, c) in [
            (4, 2, black),
            (0, 2, black),
            (2, 0, black),
            (2, 4, black),
            (2, 2, col),
        ] {
            for y in 0..h2 - dy {
                for x in 0..w2 - dx {
                    let a = mask[y * w2 + x] as f32 / 255.0;
                    if a == 0.0 {
                        continue;
                    }
                    let d = &mut work[(y + dy) * w2 + x + dx];
                    for k in 0..3 {
                        d[k] = c[k] * a + d[k] * (1.0 - a);
                    }
                    d[3] = a + d[3] * (1.0 - a);
                }
            }
        }
        // SPEC-GAP: D3DX filter TRIANGLE is approximated by an alpha-weighted 2×2 box.
        let mut out = vec![0u8; w * size * 4];
        for y in 0..size {
            for x in 0..w.min(w2 / 2) {
                let mut acc = [0f32; 4];
                for (sx, sy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = work[(2 * y + sy) * w2 + 2 * x + sx];
                    for k in 0..3 {
                        acc[k] += p[k] * p[3];
                    }
                    acc[3] += p[3];
                }
                if acc[3] > 0.0 {
                    let o = &mut out[(y * w + x) * 4..][..4];
                    for k in 0..3 {
                        o[k] = (acc[k] / acc[3] * 255.0).round() as u8;
                    }
                    o[3] = (acc[3] / 4.0 * 255.0).round() as u8;
                }
            }
        }
        out
    }
}
