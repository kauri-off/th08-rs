//! Draw-log report: renders reference and port logs with the port's renderer and writes an HTML page with the
//! chosen frames (drawdiff, session).

use crate::drawlog::{Frame, Kind, Report, Vtx};
use crate::gfx::{Blend, Gfx, Rect, TextureId, Vertex};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub struct Renderer {
    pub gfx: Gfx,
    ar: Option<crate::archive::Archive>,
    anm: HashMap<String, Vec<Option<TextureId>>>,
    img: HashMap<String, Option<(TextureId, u32, u32)>>,
}

fn rgba(c: u32) -> [u8; 4] {
    let [a, r, g, b] = c.to_be_bytes();
    [r, g, b, a]
}

// `fog=AARRGGBB,start,end` from a primitive's state field (drawlog FORMAT.md, linear fog 06 §8).
fn parse_fog(state: &str) -> Option<(u32, f32, f32)> {
    let f = state.split(' ').find_map(|k| k.strip_prefix("fog="))?;
    let mut t = f.split(',');
    Some((
        u32::from_str_radix(t.next()?, 16).ok()?,
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
    ))
}

impl Renderer {
    pub fn new(dir: Option<std::path::PathBuf>) -> anyhow::Result<Self> {
        let mut gfx = pollster::block_on(Gfx::headless())?;
        gfx.persistent = true;
        let ar = match dir {
            Some(d) => Some(crate::archive::Archive::from_bytes(std::fs::read(
                crate::assets::find_file(&d, "th08.dat"),
            )?)?),
            None => None,
        };
        Ok(Self {
            gfx,
            ar,
            anm: HashMap::new(),
            img: HashMap::new(),
        })
    }

    fn anm_tex(&mut self, label: &str) -> Option<TextureId> {
        let (file, idx) = label.split_once('#')?;
        let idx: usize = idx.parse().ok()?;
        if !self.anm.contains_key(file) {
            let mut v = Vec::new();
            if let Some(chunks) = self
                .ar
                .as_ref()
                .and_then(|a| a.read(file).ok())
                .and_then(|b| crate::anm::parse(&b).ok())
            {
                for c in chunks {
                    v.push(
                        c.texture
                            .map(|t| self.gfx.create_texture(t.width, t.height, &t.rgba)),
                    );
                }
            }
            self.anm.insert(file.to_string(), v);
        }
        self.anm[file].get(idx).copied().flatten()
    }

    fn image(&mut self, name: &str) -> Option<(TextureId, u32, u32)> {
        if !self.img.contains_key(name) {
            let t = self
                .ar
                .as_ref()
                .and_then(|a| a.read(name).ok())
                .and_then(|b| self.gfx.load_image(name, &b).ok())
                .map(|t| {
                    let x = self.gfx.texture(t);
                    (t, x.width, x.height)
                });
            self.img.insert(name.to_string(), t);
        }
        self.img[name]
    }

    fn vertex(v: &Vtx, fog: Option<(u32, f32, f32)>) -> Vertex {
        // Stage quads: eye depth from z (rhw is too coarse in the log), FORMAT.md §2.
        let w = match fog {
            Some(_) => 30.0 * 1800.0 / (1800.0 - 1770.0 * v.z),
            None if v.rhw.abs() > 1e-6 => 1.0 / v.rhw,
            None => 1.0,
        };
        let nx = (v.x + 0.5) / 320.0 - 1.0;
        let ny = 1.0 - (v.y + 0.5) / 240.0;
        let (fog_color, f) = match fog {
            Some((c, a, b)) => {
                let c = rgba(c);
                (c, ((b - w) / (b - a)).clamp(0.0, 1.0))
            }
            None => ([0; 4], 1.0),
        };
        let uv = |x: f32| if x.is_nan() { 0.0 } else { x };
        Vertex {
            pos: [nx * w, ny * w, v.z * w, w],
            uv: [uv(v.u), uv(v.v)],
            color: rgba(v.col),
            fog_color,
            fog: f,
        }
    }

    pub fn draw(&mut self, f: &Frame) {
        for p in &f.prims {
            self.gfx
                .set_viewport(Rect::new(p.vp[0], p.vp[1], p.vp[2], p.vp[3]));
            let st = &*p.state;
            let blend = if st.contains("blend=add") {
                Blend::Add
            } else {
                Blend::Alpha
            };
            let test = !st.contains("ztest=always") && !st.contains("ztest=off");
            let write = st.contains("zwrite=1");
            let fog = parse_fog(st);
            let tex = if p.textured() {
                match self.anm_tex(&p.tex) {
                    Some(t) => Some(t),
                    None => continue,
                }
            } else {
                None
            };
            let quads: Vec<[Vtx; 4]> = match &p.kind {
                Kind::Quad(q) => vec![*q],
                Kind::Poly(k, v) if &**k == "strip" && v.len() >= 4 => (0..v.len() - 3)
                    .step_by(2)
                    .map(|i| [v[i], v[i + 1], v[i + 2], v[i + 3]])
                    .collect(),
                Kind::Clear(what, c) => {
                    let c = rgba(*c);
                    if what.contains('t') {
                        self.gfx.clear_depth(Some(c));
                    } else {
                        self.gfx.clear_depth(None);
                    }
                    continue;
                }
                Kind::Copy(k) => {
                    let t: Vec<&str> = k.split(' ').collect();
                    if t.len() == 8 && t[1] == "screen" {
                        let n: Vec<f32> = t[2..].iter().map(|x| x.parse().unwrap_or(0.0)).collect();
                        if let Some((id, iw, ih)) = self.image(t[0]) {
                            let (iw, ih) = (iw as f32, ih as f32);
                            let (u0, v0) = (n[0] / iw, n[1] / ih);
                            let (u1, v1) = ((n[0] + n[2]) / iw, (n[1] + n[3]) / ih);
                            let (x0, y0, x1, y1) = (n[4], n[5], n[4] + n[2], n[5] + n[3]);
                            self.gfx.set_viewport(crate::gfx::FULLSCREEN);
                            let q = [
                                (x0, y0, u0, v0),
                                (x1, y0, u1, v0),
                                (x0, y1, u0, v1),
                                (x1, y1, u1, v1),
                            ]
                            .map(|(x, y, u, v)| Vertex::screen(x, y, 0.0, [u, v], [255; 4]));
                            self.gfx.draw_raw(Some(id), Blend::Alpha, false, false, q);
                        }
                    }
                    continue;
                }
                Kind::Poly(..) => continue,
            };
            for q in quads {
                let v = q.map(|v| Self::vertex(&v, fog));
                self.gfx.draw_raw(tex, blend, test, write, v);
            }
        }
    }
}

pub fn diff_image(a: &image::RgbaImage, b: &image::RgbaImage) -> (image::RgbaImage, f64) {
    let mut out = image::RgbaImage::new(a.width(), a.height());
    let mut bad = 0usize;
    for (x, y, p) in out.enumerate_pixels_mut() {
        let (pa, pb) = (a.get_pixel(x, y), b.get_pixel(x, y));
        let d = (0..3).map(|k| pa[k].abs_diff(pb[k])).max().unwrap_or(0);
        if d > 24 {
            bad += 1;
            *p = image::Rgba([255, 40, 40, 255]);
        } else {
            let g = (pa[0] as u32 + pa[1] as u32 + pa[2] as u32) / 9;
            *p = image::Rgba([g as u8, g as u8, g as u8, 255]);
        }
    }
    (out, 100.0 * bad as f64 / (a.width() * a.height()) as f64)
}

pub struct Entry {
    pub title: String,
    pub ref_present: i64,
    pub port_present: i64,
    pub text: String,
    pub shot: Option<PathBuf>,
}

// Renders `frames` in order into a persistent buffer and captures the requested presents.
pub fn render_pass(
    r: &mut Renderer,
    frames: impl Iterator<Item = Frame>,
    want: &[i64],
) -> HashMap<i64, image::RgbaImage> {
    let mut out = HashMap::new();
    let last = want.iter().copied().max().unwrap_or(-1);
    for f in frames {
        if f.present > last {
            break;
        }
        r.draw(&f);
        if want.contains(&f.present) {
            out.insert(f.present, r.gfx.capture(None));
        } else {
            r.gfx.flush();
        }
    }
    out
}

// Entries for the first divergence and the worst frames of a report.
pub fn entries(rep: &Report) -> Vec<Entry> {
    let mut entries: Vec<Entry> = rep
        .worst
        .iter()
        .map(|w| Entry {
            title: format!("{} — score {}", w.key, w.score),
            ref_present: w.ref_present,
            port_present: w.port_present,
            text: w.text.clone(),
            shot: None,
        })
        .collect();
    if let Some(n) = &rep.first {
        entries.insert(
            0,
            Entry {
                title: format!("first divergence: {}", n.key),
                ref_present: n.ref_present,
                port_present: n.port_present,
                text: n.text.clone(),
                shot: None,
            },
        );
    }
    entries
}

// Writes `out/index.html` (+ frames.txt and PNGs); the frame iterators are read once each.
pub fn write_html<R, P>(
    out: &Path,
    names: (&str, &str),
    rep: &Report,
    entries: &[Entry],
    reference: R,
    port: P,
) -> anyhow::Result<()>
where
    R: Iterator<Item = Frame>,
    P: Iterator<Item = Frame>,
{
    std::fs::create_dir_all(out)?;
    let mut list = String::from("# ref_present port_present ref_game_frame score\n");
    for p in &rep.pairs {
        let _ = writeln!(list, "{} {} {} {}", p.0, p.1, p.2, p.3);
    }
    std::fs::write(out.join("frames.txt"), list)?;
    let dir = crate::settings::test_game_dir();
    if dir.is_none() {
        eprintln!("TH08_DIR not set: renders use no textures");
    }
    let mut renderer = Renderer::new(dir)?;
    let want_r: Vec<i64> = entries.iter().map(|e| e.ref_present).collect();
    let want_p: Vec<i64> = entries.iter().map(|e| e.port_present).collect();
    let ri = render_pass(&mut renderer, reference, &want_r);
    let mut renderer2 = Renderer::new(crate::settings::test_game_dir())?;
    let pi = render_pass(&mut renderer2, port, &want_p);
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><title>drawdiff</title><style>body{font:13px monospace;background:#222;color:#ddd}\
         img{width:480px;image-rendering:pixelated;margin:2px}pre{white-space:pre-wrap}h2{color:#fc6}</style>",
    );
    let _ = write!(
        html,
        "<h1>drawdiff</h1><p>{} vs {}</p><pre>{}</pre>",
        esc(names.0),
        esc(names.1),
        esc(&rep.summary())
    );
    for (i, e) in entries.iter().enumerate() {
        let _ = write!(
            html,
            "<h2>{}</h2><p>ref present {}, port present {}</p><div>",
            esc(&e.title),
            e.ref_present,
            e.port_present
        );
        let (a, b) = (ri.get(&e.ref_present), pi.get(&e.port_present));
        if let Some(a) = a {
            a.save(out.join(format!("{i}_ref.png")))?;
            let _ = write!(html, "<img src='{i}_ref.png' title='reference log'>");
        }
        if let Some(b) = b {
            b.save(out.join(format!("{i}_port.png")))?;
            let _ = write!(html, "<img src='{i}_port.png' title='port log'>");
        }
        if let (Some(a), Some(b)) = (a, b) {
            let (d, pct) = diff_image(a, b);
            d.save(out.join(format!("{i}_diff.png")))?;
            let _ = write!(
                html,
                "<img src='{i}_diff.png' title='differing pixels: {pct:.2}%'>"
            );
        }
        if let Some(s) = &e.shot {
            std::fs::copy(s, out.join(format!("{i}_shot.png")))?;
            let _ = write!(
                html,
                "<img src='{i}_shot.png' title='reference screenshot'>"
            );
        }
        let _ = write!(
            html,
            "</div><p>reference log · port log · differing pixels (red) · reference screenshot</p><pre>{}</pre>",
            esc(&e.text)
        );
    }
    std::fs::write(out.join("index.html"), html)?;
    eprintln!("report: {}", out.join("index.html").display());
    Ok(())
}
