//! Text_RenderToVm 0x4663B0 rasteriser (10 §6): 2× work surface, 4-offset black outline, colour at the centre, exact D3DX TRIANGLE 2:1 into A4R4G4B4.
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
        let font = crate::settings::get()
            .font
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
        self.render_coloured(s, color, [0; 3], w, size, x, outline)
    }

    // As `render_outline` with the outline drawn in `edge` instead of black (music room titles, 16 §5.2).
    #[allow(clippy::too_many_arguments)]
    pub fn render_coloured(
        &mut self,
        s: &str,
        color: [u8; 3],
        edge: [u8; 3],
        w: usize,
        size: usize,
        x: f32,
        outline: i32,
    ) -> Vec<u8> {
        let o = if outline == -1 { 1 } else { 2 };
        // Fresh 1024-wide A1R5G5B5 DIB per call; the copied source rect is (0, 0, min(2w, 1024), 2·size) (10 §6).
        let (w2, h2) = ((2 * w).min(DIB_W), 2 * size + 6);
        let mask = self.mask(s, (2 * size - 2) as f32, 2.0 * x, 0.0, w2, h2);
        let mut dib = vec![Px::default(); w2 * h2];
        // Outline colour (black for Text_RenderToVm) at (2x+2±o, 2), (2x+2, 2±o), then the colour at the centre (2x+2, 2) (10 §6).
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
        let a = a as u32;
        for (d, c) in self.rgb.iter_mut().zip(c) {
            let d8 = *d << 3 | *d >> 2;
            *d = (c as u32 * a + d8 * (255 - a)) / 255 >> 3;
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
                    for k in 0..3 {
                        acc[k] += wt * (p.rgb[k] as f32 * k31);
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
