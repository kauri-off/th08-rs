// Draw-log diff: `drawdiff <reference> <port-log> [from-to] [--playfield|--full] [--key scene|demo|present]
// [--worst N] [--out DIR]`. Prints per-frame differences and writes DIR/index.html (default /tmp/drawdiff) with
// renders of both logs (port renderer, textures from TH08_DIR) for the worst frames and the reference screenshots.
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use th08::drawlog::{self, DemoMerge, Frame, KeyMode, Kind, Opts, Vtx};
use th08::gfx::{Blend, Gfx, Rect, TextureId, Vertex};

struct Renderer {
    gfx: Gfx,
    ar: Option<th08::archive::Archive>,
    anm: HashMap<String, Vec<Option<TextureId>>>,
    img: HashMap<String, Option<(TextureId, u32, u32)>>,
}

fn rgba(c: u32) -> [u8; 4] {
    let [a, r, g, b] = c.to_be_bytes();
    [r, g, b, a]
}

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
    fn new(dir: Option<String>) -> anyhow::Result<Self> {
        let mut gfx = pollster::block_on(Gfx::headless())?;
        gfx.persistent = true;
        let ar = match dir {
            Some(d) => Some(th08::archive::Archive::from_bytes(std::fs::read(
                Path::new(&d).join("th08.dat"),
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
                .and_then(|b| th08::anm::parse(&b).ok())
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

    fn draw(&mut self, f: &Frame) {
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
                            self.gfx.set_viewport(th08::gfx::FULLSCREEN);
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

fn diff_image(a: &image::RgbaImage, b: &image::RgbaImage) -> (image::RgbaImage, f64) {
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

struct Entry {
    title: String,
    ref_present: i64,
    port_present: i64,
    text: String,
    shot: Option<PathBuf>,
}

// Renders `frames` in order into a persistent buffer and captures the requested presents.
fn render_pass(
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

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut pos = Vec::new();
    let (mut playfield, mut key, mut worst_n, mut out) =
        (None, None, 8usize, PathBuf::from("/tmp/drawdiff"));
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--playfield" => playfield = Some(true),
            "--full" => playfield = Some(false),
            "--key" => {
                key = Some(match it.next().map(String::as_str) {
                    Some("demo") => KeyMode::Demo,
                    Some("present") => KeyMode::Present,
                    _ => KeyMode::Scene,
                })
            }
            "--worst" => worst_n = it.next().and_then(|v| v.parse().ok()).unwrap_or(8),
            "--out" => out = it.next().map(PathBuf::from).unwrap_or(out),
            _ => pos.push(a.clone()),
        }
    }
    anyhow::ensure!(
        pos.len() >= 2,
        "usage: drawdiff <reference> <port-log> [from-to] [--playfield|--full] [--key scene|demo|present] [--worst N] [--out DIR]"
    );
    let range = pos.get(2).and_then(|r| {
        let (a, b) = r.split_once('-')?;
        Some((a.parse::<i64>().ok()?, b.parse::<i64>().ok()?))
    });
    let ref_paths = drawlog::expand(&pos[0]);
    let port_paths = drawlog::expand(&pos[1]);
    anyhow::ensure!(!ref_paths.is_empty(), "no reference log at {}", pos[0]);
    let mut probe = drawlog::open(&ref_paths)?;
    let first = probe.next();
    drop(probe);
    let mode = key.unwrap_or(if first.as_ref().is_some_and(|f| f.demo >= 0) {
        KeyMode::Demo
    } else {
        KeyMode::Scene
    });
    let opts = Opts {
        playfield: playfield.unwrap_or(mode == KeyMode::Demo),
    };
    eprintln!(
        "reference {} file(s), key {mode:?}, playfield {}",
        ref_paths.len(),
        opts.playfield
    );
    let mut mem: Option<(Vec<Frame>, Vec<Frame>)> = None;
    let rep = if mode == KeyMode::Demo {
        let mut m = DemoMerge::new(drawlog::open(&ref_paths)?, opts, worst_n);
        m.range = range;
        for f in drawlog::open(&port_paths)? {
            m.feed(&f);
        }
        m.finish()
    } else {
        let mut r: Vec<Frame> = drawlog::open(&ref_paths)?.collect();
        if let Some((a, b)) = range {
            r.retain(|f| f.present >= a && f.present <= b);
        }
        let p: Vec<Frame> = drawlog::open(&port_paths)?.collect();
        let rep = drawlog::diff_all(&r, &p, mode, &opts, worst_n);
        mem = Some((r, p));
        rep
    };
    println!("{}", rep.summary());
    println!("worst frames:");
    for w in &rep.worst {
        println!(
            "  {} (ref {}, port {}) score {}\n    {}",
            w.key,
            w.ref_present,
            w.port_present,
            w.score,
            w.text.replace('\n', "\n    ")
        );
    }

    // HTML report.
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
    let ref_dir = ref_paths[0]
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let pair_of = |rp: i64| rep.pairs.iter().find(|p| p.0 == rp).copied();
    // Menu screenshots: `<stem>.shots.txt` = `<script frame> <name>`, script frame 1 = first main-menu frame.
    let stem = ref_paths[0]
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| s.split('.').next())
        .unwrap_or("")
        .to_string();
    if let (Some((r, p)), Ok(shots)) = (
        &mem,
        std::fs::read_to_string(ref_dir.join(format!("{stem}.shots.txt"))),
    ) {
        let base = r.iter().find(|f| f.scene == 1).map_or(0, |f| f.present - 1);
        for l in shots.lines() {
            let mut t = l.split_whitespace();
            let (Some(fr), Some(name)) = (t.next().and_then(|x| x.parse::<i64>().ok()), t.next())
            else {
                continue;
            };
            let png = ref_dir.join("menu_ref").join(format!("{name}.png"));
            let rp = base + fr;
            if let Some(pr) = pair_of(rp) {
                let text = match (
                    r.iter().find(|f| f.present == pr.0),
                    p.iter().find(|f| f.present == pr.1),
                ) {
                    (Some(a), Some(b)) => drawlog::diff_frame(a, b, &opts).describe(8),
                    _ => String::new(),
                };
                entries.push(Entry {
                    title: format!("shot {name} (script frame {fr}) — score {}", pr.3),
                    ref_present: pr.0,
                    port_present: pr.1,
                    text,
                    shot: png.exists().then_some(png),
                });
            }
        }
    }
    // Demo screenshots: demos/dl_d<K>_f<game frame>.png.
    if let Some(k) = first.as_ref().filter(|f| f.demo >= 0).map(|f| f.demo) {
        for e in std::fs::read_dir(&ref_dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(rest) = name.strip_prefix(&format!("dl_d{k}_f")) else {
                continue;
            };
            let Some(gf) = rest
                .strip_suffix(".png")
                .and_then(|x| x.parse::<i64>().ok())
            else {
                continue;
            };
            if let Some(pr) = rep.pairs.iter().find(|p| p.2 == gf) {
                entries.push(Entry {
                    title: format!("demo {k} game frame {gf} — score {}", pr.3),
                    ref_present: pr.0,
                    port_present: pr.1,
                    text: String::new(),
                    shot: Some(e.path()),
                });
            }
        }
    }
    std::fs::create_dir_all(&out)?;
    let mut list = String::from("# ref_present port_present ref_game_frame score\n");
    for p in &rep.pairs {
        let _ = writeln!(list, "{} {} {} {}", p.0, p.1, p.2, p.3);
    }
    std::fs::write(out.join("frames.txt"), list)?;
    let dir = std::env::var("TH08_DIR").ok();
    if dir.is_none() {
        eprintln!("TH08_DIR not set: renders use no textures");
    }
    let mut renderer = Renderer::new(dir)?;
    let want_r: Vec<i64> = entries.iter().map(|e| e.ref_present).collect();
    let want_p: Vec<i64> = entries.iter().map(|e| e.port_present).collect();
    let (ri, pi) = match mem {
        Some((r, p)) => {
            let a = render_pass(&mut renderer, r.into_iter(), &want_r);
            let mut renderer2 = Renderer::new(std::env::var("TH08_DIR").ok())?;
            let b = render_pass(&mut renderer2, p.into_iter(), &want_p);
            (a, b)
        }
        None => {
            let a = render_pass(&mut renderer, drawlog::open(&ref_paths)?, &want_r);
            let mut renderer2 = Renderer::new(std::env::var("TH08_DIR").ok())?;
            let b = render_pass(&mut renderer2, drawlog::open(&port_paths)?, &want_p);
            (a, b)
        }
    };
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
        esc(&pos[0]),
        esc(&pos[1]),
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
