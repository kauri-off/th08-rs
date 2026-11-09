//! Measuring and shaping UI text with the fonts the UI ships; advances are linear, so points measure the same at any scale.
use crate::scene::Face;
use crate::wrap::{latin_dots, pieces};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use swash::FontRef;
use swash::shape::ShapeContext;
use swash::text::{Codepoint, Script};

// The menu face: Noto Serif CJK JP SemiBold, cut to the glyphs of the UI tables (`tools/i18n_font_subset`).
pub const SERIF_FONT: &[u8] = include_bytes!("../../../i18n/fonts/NotoSerifJP-ui.otf");
pub const SANS_FONT: &[u8] = include_bytes!("../../../i18n/fonts/NotoSansJP-launcher.otf");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub height: f32,
}

// Text sizes in points for a layout; the renderer's shaper and test doubles implement it.
pub trait Measure {
    fn width(&self, text: &str, face: Face, size: f32) -> f32;
    fn line(&self, face: Face, size: f32) -> LineMetrics;
}

// The lines of `text` in at most `max` points: breaks at newlines and between `wrap::pieces`; a piece wider than
// `max` stays whole on its own line.
pub fn wrap_lines(text: &str, max: f32, face: Face, size: f32, m: &dyn Measure) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for piece in pieces(para) {
            let next = format!("{line}{piece}");
            if line.is_empty() || m.width(next.trim_end(), face, size) <= max {
                line = next;
            } else {
                out.push(line.trim_end().to_string());
                line = piece.to_string();
            }
        }
        out.push(line.trim_end().to_string());
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u16,
    // Pen position in ems from the start of the run.
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, PartialEq)]
pub struct Shaped {
    pub glyphs: Vec<Glyph>,
    // Ems.
    pub width: f32,
}

const SHAPE_CACHE: usize = 4096;

pub fn font(face: Face) -> FontRef<'static> {
    let data = match face {
        Face::Serif => SERIF_FONT,
        Face::Sans => SANS_FONT,
    };
    FontRef::from_index(data, 0).expect("the shipped UI font parses")
}

#[derive(Default)]
struct Cache {
    ctx: ShapeContext,
    runs: [HashMap<String, Rc<Shaped>>; 2],
}

// Shapes and measures on the CPU; the results are cached per string.
#[derive(Default)]
pub struct Shaper {
    cache: RefCell<Cache>,
}

// Maximal pieces of `text` in one script; neutral characters join the run before them.
fn runs(text: &str) -> Vec<(Script, &str)> {
    let mut out: Vec<(Script, usize)> = Vec::new();
    for (at, c) in text.char_indices() {
        let s = c.script();
        let neutral = matches!(s, Script::Common | Script::Inherited | Script::Unknown);
        match out.last_mut() {
            Some((ls, _)) if neutral || *ls == s => {}
            Some((ls, _)) if matches!(ls, Script::Common) => *ls = s,
            _ => out.push((s, at)),
        }
    }
    let ends: Vec<usize> = out
        .iter()
        .skip(1)
        .map(|r| r.1)
        .chain([text.len()])
        .collect();
    out.iter()
        .zip(ends)
        .map(|(&(s, at), end)| (s, &text[at..end]))
        .collect()
}

impl Shaper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shape(&self, text: &str, face: Face) -> Rc<Shaped> {
        let c = &mut *self.cache.borrow_mut();
        let map = &mut c.runs[face as usize];
        if let Some(s) = map.get(text) {
            return s.clone();
        }
        if map.len() >= SHAPE_CACHE {
            map.clear();
        }
        let shaped = Rc::new(shape_uncached(&mut c.ctx, text, face));
        map.insert(text.to_string(), shaped.clone());
        shaped
    }
}

fn shape_uncached(ctx: &mut ShapeContext, text: &str, face: Face) -> Shaped {
    let text = latin_dots(text);
    let mut glyphs = Vec::new();
    let mut pen = 0.0;
    for (script, run) in runs(&text) {
        let mut shaper = ctx.builder(font(face)).script(script).size(1.0).build();
        shaper.add_str(run);
        shaper.shape_with(|cluster| {
            for g in cluster.glyphs {
                glyphs.push(Glyph {
                    id: g.id,
                    x: pen + g.x,
                    y: g.y,
                });
                pen += g.advance;
            }
        });
    }
    Shaped { glyphs, width: pen }
}

impl Measure for Shaper {
    fn width(&self, text: &str, face: Face, size: f32) -> f32 {
        self.shape(text, face).width * size
    }

    fn line(&self, face: Face, size: f32) -> LineMetrics {
        let m = font(face).metrics(&[]).scale(size);
        LineMetrics {
            ascent: m.ascent,
            descent: m.descent,
            height: m.ascent + m.descent + m.leading,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every char is 10 points wide.
    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, _: Face, _: f32) -> f32 {
            10.0 * text.chars().count() as f32
        }
        fn line(&self, _: Face, size: f32) -> LineMetrics {
            LineMetrics {
                ascent: size,
                descent: 0.0,
                height: size,
            }
        }
    }

    #[test]
    fn wrap_keeps_words_and_newlines() {
        let w = |t, max| wrap_lines(t, max, Face::Serif, 20.0, &Fixed);
        assert_eq!(w("Master volume level", 130.0), ["Master volume", "level"]);
        assert_eq!(w("a\nb c", 500.0), ["a", "b c"]);
        assert_eq!(
            w("Extraordinarily short", 50.0),
            ["Extraordinarily", "short"]
        );
    }

    #[test]
    fn wrap_japanese_only_between_phrases_with_kinsoku() {
        let t = "ゲーム本来のBGM音量はそのままです。";
        let lines = wrap_lines(t, 90.0, Face::Serif, 20.0, &Fixed);
        assert!(lines.len() > 1, "{lines:?}");
        assert_eq!(lines.concat(), t);
        let cuts = pieces(t);
        for l in &lines {
            assert!(
                cuts.iter().any(|p| l.ends_with(p.trim_end())),
                "{l} {cuts:?}"
            );
            assert!(!l.starts_with('。'), "{lines:?}");
        }
    }

    #[test]
    fn real_fonts_cover_en_ru_ja_and_measure_linearly() {
        let s = Shaper::new();
        for t in ["Master volume", "Громкость музыки", "音量の設定"] {
            let one = s.width(t, Face::Serif, 10.0);
            assert!(one > 0.0);
            assert!((s.width(t, Face::Serif, 30.0) - one * 3.0).abs() < 1e-3);
            assert!(
                s.shape(t, Face::Serif).glyphs.iter().all(|g| g.id != 0),
                "{t}"
            );
        }
    }

    #[test]
    fn ellipsis_follows_the_script() {
        let s = Shaper::new();
        assert_eq!(
            s.shape("Quit…", Face::Serif).glyphs.len(),
            s.shape("Quit...", Face::Serif).glyphs.len()
        );
    }

    #[test]
    fn the_crate_has_no_gpu_window_or_toolkit_dependency() {
        let manifest = include_str!("../Cargo.toml");
        for dep in ["wgpu", "winit", "egui", "slint"] {
            assert!(!manifest.contains(dep), "{dep}");
        }
    }
}
