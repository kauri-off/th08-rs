// Draw log (specs/data/drawlog/FORMAT.md): the port's writer, a reader and a per-frame diff against a reference log.
use crate::gfx::{Blend, LogRec, Vertex};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::BufRead;
use std::rc::Rc;

// Decimal with trailing zeros removed ("436.5", "0", "0.00781").
pub fn num(v: f32, dec: usize) -> String {
    let mut s = format!("{v:.dec$}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" { "0".into() } else { s }
}

fn argb(c: [u8; 4]) -> u32 {
    u32::from_be_bytes([c[3], c[0], c[1], c[2]])
}

// Back from the port's clip coordinates to D3D screen space (pixel centres on integers).
fn screen(v: &Vertex) -> [f32; 4] {
    let w = v.pos[3];
    let (nx, ny) = (v.pos[0] / w, v.pos[1] / w);
    [
        (nx + 1.0) * 320.0 - 0.5,
        (1.0 - ny) * 240.0 - 0.5,
        v.pos[2] / w,
        1.0 / w,
    ]
}

pub struct Header {
    pub present: u64,
    pub scene: i32,
    pub scene_frame: u32,
    pub game_frame: u32,
    pub demo: i32,
    pub demo_frame: i32,
    pub input: u16,
}

pub fn format_frame(h: &Header, recs: &[LogRec]) -> String {
    let mut body = String::new();
    let (mut t, mut s, mut v) = (String::new(), String::new(), String::new());
    let mut nquads = 0;
    let set = |body: &mut String, last: &mut String, line: String| {
        if *last != line {
            body.push_str(&line);
            body.push('\n');
            *last = line;
        }
    };
    for r in recs {
        match r {
            LogRec::Quad {
                tex,
                id,
                textured,
                blend,
                test,
                write,
                vp,
                fog,
                v: verts,
            } => {
                let tl = match (textured, tex) {
                    (false, _) => "T -".to_string(),
                    (true, Some(l)) => format!("T {} {} {}", l.label, l.w, l.h),
                    (true, None) => format!("T ?{id:x} 0 0"),
                };
                let comb = if *textured { "tex*col" } else { "col" };
                let mut sl = format!(
                    "S blend={} atest=ge4 ztest={} zwrite={} c={comb} a={comb}",
                    if *blend == Blend::Add { "add" } else { "alpha" },
                    if *test { "le" } else { "always" },
                    *write as u8
                );
                if let Some(f) = fog {
                    let c = argb([f.color[0], f.color[1], f.color[2], 255]);
                    let _ = write!(sl, " fog={c:08x},{},{}", num(f.start, 2), num(f.end, 2));
                }
                set(&mut body, &mut t, tl);
                set(&mut body, &mut s, sl);
                set(&mut body, &mut v, vp_line(vp));
                body.push_str(&quad_line(verts, *textured));
                body.push('\n');
                nquads += 1;
            }
            LogRec::Fan {
                blend,
                vp,
                v: verts,
            } => {
                set(&mut body, &mut t, "T -".to_string());
                set(
                    &mut body,
                    &mut s,
                    format!(
                        "S blend={} atest=ge4 ztest=always zwrite=0 c=col a=col",
                        if *blend == Blend::Add { "add" } else { "alpha" }
                    ),
                );
                set(&mut body, &mut v, vp_line(vp));
                let _ = write!(body, "P fan {}", verts.len());
                for vx in verts {
                    let [x, y, z, rhw] = screen(vx);
                    let _ = write!(
                        body,
                        " {} {} {} {} - - {:08x}",
                        num(x, 2),
                        num(y, 2),
                        num(z, 4),
                        num(rhw, 4),
                        argb(vx.color)
                    );
                }
                body.push('\n');
            }
            LogRec::Clear { vp, color } => {
                set(&mut body, &mut v, vp_line(vp));
                match color {
                    Some(c) => {
                        let _ = writeln!(body, "C tz {:08x}", argb(*c));
                    }
                    None => body.push_str("C z ff000000\n"),
                }
            }
            LogRec::Copy {
                src,
                dst,
                from,
                size,
                to,
            } => {
                let _ = writeln!(
                    body,
                    "K {src} {dst} {} {} {} {} {} {}",
                    from[0], from[1], size[0], size[1], to[0], to[1]
                );
            }
        }
    }
    format!(
        "F {} {} {} {} {} {} {:04x} {nquads}\n{body}",
        h.present, h.scene, h.scene_frame, h.game_frame, h.demo, h.demo_frame, h.input
    )
}

fn vp_line(r: &crate::gfx::Rect) -> String {
    format!(
        "V {} {} {} {}",
        num(r.x, 2),
        num(r.y, 2),
        num(r.w, 2),
        num(r.h, 2)
    )
}

fn quad_line(verts: &[Vertex; 4], textured: bool) -> String {
    let f: Vec<[String; 7]> = verts
        .iter()
        .map(|vx| {
            let [x, y, z, rhw] = screen(vx);
            let (u, v) = if textured {
                (num(vx.uv[0], 5), num(vx.uv[1], 5))
            } else {
                ("-".into(), "-".into())
            };
            [
                num(x, 2),
                num(y, 2),
                num(z, 4),
                num(rhw, 4),
                u,
                v,
                format!("{:08x}", argb(vx.color)),
            ]
        })
        .collect();
    let rect = textured
        && f[0][1] == f[1][1]
        && f[2][1] == f[3][1]
        && f[0][0] == f[2][0]
        && f[1][0] == f[3][0]
        && f.iter()
            .all(|a| a[3] == "1" && a[2] == f[0][2] && a[6] == f[0][6])
        && f[0][4] == f[2][4]
        && f[1][4] == f[3][4]
        && f[0][5] == f[1][5]
        && f[2][5] == f[3][5];
    if rect {
        let mut l = format!(
            "R {} {} {} {} {} {} {} {} {}",
            f[0][0], f[0][1], f[3][0], f[3][1], f[0][4], f[0][5], f[3][4], f[3][5], f[0][6]
        );
        if f[0][2] != "0" {
            l.push(' ');
            l.push_str(&f[0][2]);
        }
        l
    } else {
        let mut l = String::from("Q");
        for a in &f {
            for x in a {
                l.push(' ');
                l.push_str(x);
            }
        }
        l
    }
}

enum Sink {
    Memory(Vec<String>),
    #[cfg(not(target_arch = "wasm32"))]
    File(Box<dyn std::io::Write>, Option<std::process::Child>),
}

// Frame filter: present numbers, or demo frames with a `d` prefix (`d1000-1100`).
#[derive(Clone, Copy)]
struct Range {
    demo: bool,
    from: i64,
    to: i64,
}

pub struct Writer {
    sink: Sink,
    range: Option<Range>,
    present: u64,
    scene: i32,
    scene_frame: u32,
}

impl Writer {
    fn new(sink: Sink) -> Self {
        Self {
            sink,
            range: None,
            present: 0,
            scene: i32::MIN,
            scene_frame: 0,
        }
    }

    pub fn memory() -> Self {
        Self::new(Sink::Memory(Vec::new()))
    }

    // `TH08_DRAWLOG=<path>[,from-to | ,dfrom-to]`; a `.xz` path is compressed through the xz tool.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_env() -> Option<Self> {
        let v = crate::env::var("TH08_DRAWLOG")?;
        let (path, range) = match v.rsplit_once(',') {
            Some((p, r)) => (p.to_string(), parse_range(r)),
            None => (v.clone(), None),
        };
        let file = match std::fs::File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                log::error!("TH08_DRAWLOG={path}: {e}");
                return None;
            }
        };
        let sink = if path.ends_with(".xz") {
            let child = std::process::Command::new("xz")
                .args(["-1", "-T0", "-c"])
                .stdin(std::process::Stdio::piped())
                .stdout(file)
                .spawn();
            match child {
                Ok(mut c) => {
                    let w = std::io::BufWriter::new(c.stdin.take().unwrap());
                    Sink::File(Box::new(w), Some(c))
                }
                Err(e) => {
                    log::error!("xz: {e}");
                    return None;
                }
            }
        } else {
            Sink::File(Box::new(std::io::BufWriter::new(file)), None)
        };
        let mut w = Self::new(sink);
        w.range = range;
        Some(w)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn from_env() -> Option<Self> {
        None
    }

    pub fn frame(
        &mut self,
        scene: i32,
        game_frame: u32,
        demo: Option<(u8, i32)>,
        input: u16,
        recs: &[LogRec],
    ) {
        self.present += 1;
        if scene != self.scene {
            (self.scene, self.scene_frame) = (scene, 0);
        }
        self.scene_frame += 1;
        let h = Header {
            present: self.present,
            scene,
            scene_frame: self.scene_frame,
            game_frame,
            demo: demo.map_or(-1, |d| d.0 as i32),
            demo_frame: demo.map_or(0, |d| d.1),
            input,
        };
        if let Some(r) = self.range {
            let k = if r.demo {
                h.demo_frame as i64
            } else {
                h.present as i64
            };
            if k < r.from || k > r.to || (r.demo && h.demo < 0) {
                return;
            }
        }
        let text = format_frame(&h, recs);
        match &mut self.sink {
            Sink::Memory(v) => v.push(text),
            #[cfg(not(target_arch = "wasm32"))]
            Sink::File(w, _) => {
                let _ = w.write_all(text.as_bytes());
            }
        }
    }

    pub fn take_frames(&mut self) -> Vec<String> {
        match &mut self.sink {
            Sink::Memory(v) => std::mem::take(v),
            #[cfg(not(target_arch = "wasm32"))]
            _ => Vec::new(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Writer {
    fn drop(&mut self) {
        if let Sink::File(w, child) = &mut self.sink {
            let _ = w.flush();
            *w = Box::new(std::io::sink());
            if let Some(c) = child {
                let _ = c.wait();
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_range(r: &str) -> Option<Range> {
    let (demo, r) = match r.strip_prefix('d') {
        Some(x) => (true, x),
        None => (false, r),
    };
    let (a, b) = r.split_once('-')?;
    Some(Range {
        demo,
        from: a.parse().ok()?,
        to: b.parse().ok()?,
    })
}

// ---- reader ----

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vtx {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rhw: f32,
    pub u: f32,
    pub v: f32,
    pub col: u32,
}

#[derive(Clone, Debug)]
pub enum Kind {
    Quad([Vtx; 4]),
    Poly(Rc<str>, Vec<Vtx>),
    Clear(Rc<str>, u32),
    Copy(Rc<str>),
}

#[derive(Clone, Debug)]
pub struct Prim {
    pub tex: Rc<str>,
    pub tw: u32,
    pub th: u32,
    pub state: Rc<str>,
    pub vp: [f32; 4],
    pub kind: Kind,
}

impl Prim {
    pub fn textured(&self) -> bool {
        &*self.tex != "-"
    }
}

#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub present: i64,
    pub scene: i32,
    pub scene_frame: i64,
    pub game_frame: i64,
    pub demo: i32,
    pub demo_frame: i64,
    pub input: u32,
    pub prims: Vec<Prim>,
}

#[derive(Default)]
struct Interner(HashMap<String, Rc<str>>);

impl Interner {
    fn get(&mut self, s: &str) -> Rc<str> {
        if let Some(r) = self.0.get(s) {
            return r.clone();
        }
        let r: Rc<str> = s.into();
        self.0.insert(s.to_string(), r.clone());
        r
    }
}

pub struct Reader<R: BufRead> {
    src: R,
    line: String,
    next: Option<Frame>,
    intern: Interner,
    tex: (Rc<str>, u32, u32),
    state: Rc<str>,
    vp: [f32; 4],
    done: bool,
    none: Rc<str>,
}

fn f(t: Option<&str>) -> f32 {
    match t {
        Some("-") | None => f32::NAN,
        Some(s) => s.parse().unwrap_or(f32::NAN),
    }
}

fn hex(t: Option<&str>) -> u32 {
    t.and_then(|s| u32::from_str_radix(s, 16).ok()).unwrap_or(0)
}

impl<R: BufRead> Reader<R> {
    pub fn new(src: R) -> Self {
        let mut intern = Interner::default();
        let none = intern.get("-");
        Self {
            src,
            line: String::new(),
            next: None,
            intern,
            tex: (none.clone(), 0, 0),
            state: none.clone(),
            none,
            vp: [0.0, 0.0, 640.0, 480.0],
            done: false,
        }
    }

    fn vertex(t: &mut std::str::SplitAsciiWhitespace) -> Vtx {
        Vtx {
            x: f(t.next()),
            y: f(t.next()),
            z: f(t.next()),
            rhw: f(t.next()),
            u: f(t.next()),
            v: f(t.next()),
            col: hex(t.next()),
        }
    }

    // Untextured primitives ("-" UVs) are grouped under "-" whatever texture is still set.
    fn prim(&self, kind: Kind) -> Prim {
        let untextured = match &kind {
            Kind::Quad(q) => q[0].u.is_nan(),
            Kind::Poly(_, v) => v.first().is_some_and(|x| x.u.is_nan()),
            _ => false,
        };
        Prim {
            tex: if untextured {
                self.none.clone()
            } else {
                self.tex.0.clone()
            },
            tw: self.tex.1,
            th: self.tex.2,
            state: self.state.clone(),
            vp: self.vp,
            kind,
        }
    }
}

impl<R: BufRead> Iterator for Reader<R> {
    type Item = Frame;

    fn next(&mut self) -> Option<Frame> {
        loop {
            if self.done {
                return self.next.take();
            }
            self.line.clear();
            if self.src.read_line(&mut self.line).unwrap_or(0) == 0 {
                self.done = true;
                continue;
            }
            let line = std::mem::take(&mut self.line);
            let mut t = line.split_ascii_whitespace();
            let rec = t.next().unwrap_or("");
            match rec {
                "F" => {
                    let n: Vec<i64> = (0..6)
                        .map(|_| t.next().and_then(|s| s.parse().ok()).unwrap_or(0))
                        .collect();
                    let fr = Frame {
                        present: n[0],
                        scene: n[1] as i32,
                        scene_frame: n[2],
                        game_frame: n[3],
                        demo: n[4] as i32,
                        demo_frame: n[5],
                        input: hex(t.next()),
                        prims: Vec::new(),
                    };
                    self.tex = (self.intern.get("-"), 0, 0);
                    self.state = self.intern.get("-");
                    self.vp = [0.0, 0.0, 640.0, 480.0];
                    if let Some(prev) = self.next.replace(fr) {
                        self.line = line;
                        return Some(prev);
                    }
                }
                "T" => {
                    let l = t.next().unwrap_or("-");
                    let w = t.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let h = t.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    self.tex = (self.intern.get(l), w, h);
                }
                "S" => self.state = self.intern.get(line[1..].trim()),
                "V" => {
                    let v: Vec<f32> = t.map(|s| s.parse().unwrap_or(0.0)).collect();
                    if v.len() >= 4 {
                        self.vp = [v[0], v[1], v[2], v[3]];
                    }
                }
                "R" => {
                    let n: Vec<&str> = t.collect();
                    let g = |i: usize| f(n.get(i).copied());
                    let col = hex(n.get(8).copied());
                    let z = n.get(9).map_or(0.0, |s| s.parse().unwrap_or(0.0));
                    let (x0, y0, x1, y1, u0, v0, u1, v1) =
                        (g(0), g(1), g(2), g(3), g(4), g(5), g(6), g(7));
                    let vt = |x, y, u, v| Vtx {
                        x,
                        y,
                        z,
                        rhw: 1.0,
                        u,
                        v,
                        col,
                    };
                    let q = [
                        vt(x0, y0, u0, v0),
                        vt(x1, y0, u1, v0),
                        vt(x0, y1, u0, v1),
                        vt(x1, y1, u1, v1),
                    ];
                    let p = self.prim(Kind::Quad(q));
                    if let Some(fr) = &mut self.next {
                        fr.prims.push(p);
                    }
                }
                "Q" => {
                    let q = std::array::from_fn(|_| Self::vertex(&mut t));
                    let p = self.prim(Kind::Quad(q));
                    if let Some(fr) = &mut self.next {
                        fr.prims.push(p);
                    }
                }
                "P" => {
                    let kind = self.intern.get(t.next().unwrap_or("?"));
                    let n: usize = t.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let v = (0..n).map(|_| Self::vertex(&mut t)).collect();
                    let p = self.prim(Kind::Poly(kind, v));
                    if let Some(fr) = &mut self.next {
                        fr.prims.push(p);
                    }
                }
                "C" => {
                    let what = self.intern.get(t.next().unwrap_or("?"));
                    let col = hex(t.next());
                    let p = self.prim(Kind::Clear(what, col));
                    if let Some(fr) = &mut self.next {
                        fr.prims.push(p);
                    }
                }
                "K" => {
                    let rest = self.intern.get(line[1..].trim());
                    let p = self.prim(Kind::Copy(rest));
                    if let Some(fr) = &mut self.next {
                        fr.prims.push(p);
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn parse_str(text: &str) -> Vec<Frame> {
    Reader::new(std::io::Cursor::new(text.as_bytes())).collect()
}

// Opens one or more logs (plain or .xz, concatenated in order); `.xz` goes through the xz tool.
#[cfg(not(target_arch = "wasm32"))]
pub fn open(paths: &[std::path::PathBuf]) -> anyhow::Result<Reader<Box<dyn BufRead>>> {
    use std::io::Read;
    anyhow::ensure!(!paths.is_empty(), "no draw log given");
    let mut src: Box<dyn Read> = Box::new(std::io::empty());
    for p in paths {
        let r: Box<dyn Read> = if p.extension().is_some_and(|e| e == "xz") {
            let mut c = std::process::Command::new("xz")
                .arg("-dc")
                .arg(p)
                .stdout(std::process::Stdio::piped())
                .spawn()?;
            Box::new(c.stdout.take().unwrap())
        } else {
            Box::new(std::fs::File::open(p)?)
        };
        src = Box::new(src.chain(r));
    }
    Ok(Reader::new(Box::new(std::io::BufReader::with_capacity(
        1 << 20,
        src,
    ))))
}

// A reference given as `<dir>/demo0` expands to its chunks `<dir>/demo0_*.drawlog.xz`.
#[cfg(not(target_arch = "wasm32"))]
pub fn expand(path: &str) -> Vec<std::path::PathBuf> {
    let p = std::path::Path::new(path);
    if p.is_file() {
        return vec![p.to_path_buf()];
    }
    let (dir, stem) = (
        p.parent().unwrap_or(std::path::Path::new(".")),
        p.file_name().and_then(|s| s.to_str()).unwrap_or(""),
    );
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|q| {
            q.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|n| n.starts_with(&format!("{stem}_")) && n.contains(".drawlog"))
        })
        .collect();
    v.sort();
    v
}

// ---- diff ----

pub const PLAYFIELD: [f32; 4] = [32.0, 16.0, 416.0, 464.0];

#[derive(Clone, Copy, Debug)]
pub struct Opts {
    pub playfield: bool,
}

pub const D_POS: u8 = 1;
pub const D_UV: u8 = 2;
pub const D_COL: u8 = 4;
pub const D_Z: u8 = 8;
pub const D_STATE: u8 = 16;
// z / zwrite of quads drawn with ztest=always on both sides: not visible, reported but not scored.
pub const D_HIDDEN: u8 = 32;
pub const D_NAMES: [&str; 6] = ["pos", "uv", "colour", "z/rhw", "state", "2D z/zwrite"];

#[derive(Clone, Debug, Default)]
pub struct LabelDiff {
    pub same: usize,
    pub missing: usize,
    pub extra: usize,
    pub changed: usize,
    pub why: [usize; 6],
}

impl LabelDiff {
    fn add(&mut self, o: &LabelDiff) {
        self.same += o.same;
        self.missing += o.missing;
        self.extra += o.extra;
        self.changed += o.changed;
        for k in 0..6 {
            self.why[k] += o.why[k];
        }
    }
}

pub type SpriteKey = (Rc<str>, [i32; 4]);

#[derive(Clone, Debug, Default)]
pub struct FrameDiff {
    pub total: LabelDiff,
    pub reordered: usize,
    pub ref_quads: usize,
    pub port_quads: usize,
    pub by_label: BTreeMap<Rc<str>, LabelDiff>,
    pub sprites_missing: Vec<SpriteKey>,
    pub sprites_extra: Vec<SpriteKey>,
    pub other_missing: Vec<String>,
    pub other_extra: Vec<String>,
}

impl FrameDiff {
    pub fn score(&self) -> usize {
        self.total.missing
            + self.total.extra
            + self.total.changed
            + self.other_missing.len()
            + self.other_extra.len()
    }

    // One-line summary plus the labels with the most differences.
    pub fn describe(&self, max_labels: usize) -> String {
        let t = &self.total;
        let mut s = format!(
            "quads ref {} port {}: same {} changed {} missing {} extra {} reordered {}",
            self.ref_quads, self.port_quads, t.same, t.changed, t.missing, t.extra, self.reordered
        );
        if t.why.iter().any(|&x| x > 0) {
            s.push_str(" (");
            s.push_str(&why_str(&t.why));
            s.push(')');
        }
        let mut labels: Vec<_> = self
            .by_label
            .iter()
            .filter(|(_, d)| d.missing + d.extra + d.changed > 0)
            .collect();
        labels.sort_by_key(|(_, d)| std::cmp::Reverse(d.missing + d.extra + d.changed));
        for (l, d) in labels.into_iter().take(max_labels) {
            let _ = write!(
                s,
                "\n  {l}: missing {} extra {} changed {}",
                d.missing, d.extra, d.changed
            );
            if d.why.iter().any(|&x| x > 0) {
                let _ = write!(s, " ({})", why_str(&d.why));
            }
        }
        for (name, v) in [
            ("missing", &self.other_missing),
            ("extra", &self.other_extra),
        ] {
            for o in v.iter().take(4) {
                let _ = write!(s, "\n  {name}: {o}");
            }
        }
        let sk = |k: &SpriteKey| format!("{}[{},{},{},{}]", k.0, k.1[0], k.1[1], k.1[2], k.1[3]);
        if !self.sprites_missing.is_empty() {
            let v: Vec<_> = self.sprites_missing.iter().take(8).map(sk).collect();
            let _ = write!(s, "\n  sprites only in ref: {}", v.join(" "));
        }
        if !self.sprites_extra.is_empty() {
            let v: Vec<_> = self.sprites_extra.iter().take(8).map(sk).collect();
            let _ = write!(s, "\n  sprites only in port: {}", v.join(" "));
        }
        s
    }
}

pub fn why_str(w: &[usize; 6]) -> String {
    (0..6)
        .filter(|&k| w[k] > 0)
        .map(|k| format!("{} {}", D_NAMES[k], w[k]))
        .collect::<Vec<_>>()
        .join(", ")
}

struct Item<'a> {
    order: usize,
    prim: &'a Prim,
    q: [Vtx; 4],
    key: [i32; 4],
    c: [f32; 2],
}

fn sprite_rect(p: &Prim, q: &[Vtx; 4]) -> [i32; 4] {
    if !p.textured() {
        return [0; 4];
    }
    let (mut u0, mut v0, mut u1, mut v1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for v in q {
        u0 = u0.min(v.u);
        u1 = u1.max(v.u);
        v0 = v0.min(v.v);
        v1 = v1.max(v.v);
    }
    let (w, h) = (p.tw as f32, p.th as f32);
    [
        (u0 * w).round() as i32,
        (v0 * h).round() as i32,
        (u1 * w).round() as i32,
        (v1 * h).round() as i32,
    ]
}

fn is_fps(p: &Prim, q: &[Vtx; 4]) -> bool {
    p.tex.starts_with("ascii.anm") && q.iter().all(|v| v.x >= 496.0 && v.y >= 456.0)
}

fn in_playfield(q: &[Vtx; 4]) -> bool {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for v in q {
        x0 = x0.min(v.x);
        x1 = x1.max(v.x);
        y0 = y0.min(v.y);
        y1 = y1.max(v.y);
    }
    x1 > PLAYFIELD[0] && x0 < PLAYFIELD[2] && y1 > PLAYFIELD[1] && y0 < PLAYFIELD[3]
}

fn items<'a>(fr: &'a Frame, o: &Opts, others: &mut Vec<String>) -> Vec<Item<'a>> {
    let mut out = Vec::new();
    let push = |p: &'a Prim, q: [Vtx; 4], out: &mut Vec<Item<'a>>| {
        if is_fps(p, &q) || (o.playfield && !in_playfield(&q)) {
            return;
        }
        let c = [
            (q[0].x + q[1].x + q[2].x + q[3].x) / 4.0,
            (q[0].y + q[1].y + q[2].y + q[3].y) / 4.0,
        ];
        out.push(Item {
            order: out.len(),
            prim: p,
            key: sprite_rect(p, &q),
            q,
            c,
        });
    };
    for p in &fr.prims {
        match &p.kind {
            Kind::Quad(q) => push(p, *q, &mut out),
            Kind::Poly(k, v) if &**k == "strip" && v.len() >= 4 => {
                for i in (0..v.len() - 3).step_by(2) {
                    push(p, [v[i], v[i + 1], v[i + 2], v[i + 3]], &mut out);
                }
            }
            Kind::Poly(k, v) => others.push(format!("P {k} {} ({})", v.len(), p.tex)),
            Kind::Clear(w, c) => {
                let vp = format!("{} {} {} {}", p.vp[0], p.vp[1], p.vp[2], p.vp[3]);
                if !o.playfield || p.vp == [32.0, 16.0, 384.0, 448.0] {
                    if w.contains('t') {
                        others.push(format!("C {w} {c:08x} @ {vp}"));
                    } else {
                        others.push(format!("C {w} @ {vp}"));
                    }
                }
            }
            Kind::Copy(k) => {
                if !o.playfield {
                    others.push(format!("K {k}"));
                }
            }
        }
    }
    out
}

fn tol(v: f32) -> f32 {
    0.03 + 1e-4 * v.abs()
}

fn close(a: f32, b: f32, t: f32) -> bool {
    (a.is_nan() && b.is_nan()) || (a - b).abs() <= t
}

fn col_close(a: u32, b: u32) -> bool {
    (0..4).all(|k| ((a >> (k * 8)) as u8).abs_diff((b >> (k * 8)) as u8) <= 1)
}

fn compare(a: &Item, b: &Item) -> u8 {
    let mut d = 0;
    for (x, y) in a.q.iter().zip(&b.q) {
        if !close(x.x, y.x, tol(x.x)) || !close(x.y, y.y, tol(x.y)) {
            d |= D_POS;
        }
        if !close(x.u, y.u, 1e-4) || !close(x.v, y.v, 1e-4) {
            d |= D_UV;
        }
        if !col_close(x.col, y.col) {
            d |= D_COL;
        }
        let rt = 2e-3 + 0.02 * x.rhw.abs();
        if !close(x.z, y.z, 2e-3) || !close(x.rhw, y.rhw, rt) {
            d |= D_Z;
        }
    }
    let flat = |st: &str| st.contains("ztest=always");
    let strip = |st: &str| {
        st.split(' ')
            .filter(|k| !k.starts_with("zwrite="))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (sa, sb) = (&*a.prim.state, &*b.prim.state);
    let both_2d = flat(sa) && flat(sb);
    if sa != sb {
        d |= if both_2d && strip(sa) == strip(sb) {
            D_HIDDEN
        } else {
            D_STATE
        };
    }
    if a.prim.vp != b.prim.vp {
        d |= D_STATE;
    }
    if both_2d && d & D_Z != 0 {
        d = d & !D_Z | D_HIDDEN;
    }
    d
}

// Longest increasing subsequence length.
fn lis(v: &[usize]) -> usize {
    let mut tails: Vec<usize> = Vec::new();
    for &x in v {
        match tails.binary_search(&x) {
            Ok(_) => {}
            Err(i) if i == tails.len() => tails.push(x),
            Err(i) => tails[i] = x,
        }
    }
    tails.len()
}

pub fn diff_frame(r: &Frame, p: &Frame, o: &Opts) -> FrameDiff {
    let (mut ro, mut po) = (Vec::new(), Vec::new());
    let ri = items(r, o, &mut ro);
    let pi = items(p, o, &mut po);
    let mut d = FrameDiff {
        ref_quads: ri.len(),
        port_quads: pi.len(),
        ..Default::default()
    };
    let mut rmatch: Vec<Option<usize>> = vec![None; ri.len()];
    let mut pmatch: Vec<Option<(usize, u8)>> = vec![None; pi.len()];
    // Pass 1: identical quads, looked up by sprite and position.
    let cell = |it: &Item| {
        (
            (it.c[0] * 2.0).round() as i64,
            (it.c[1] * 2.0).round() as i64,
        )
    };
    let mut grid: HashMap<(&str, [i32; 4], i64, i64), Vec<usize>> = HashMap::new();
    for (i, it) in ri.iter().enumerate() {
        let (x, y) = cell(it);
        grid.entry((&it.prim.tex, it.key, x, y))
            .or_default()
            .push(i);
    }
    for (j, it) in pi.iter().enumerate() {
        let (x, y) = cell(it);
        'found: for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(c) = grid.get(&(&*it.prim.tex, it.key, x + dx, y + dy)) {
                    for &i in c {
                        let w = compare(&ri[i], it);
                        if rmatch[i].is_none() && w & !D_HIDDEN == 0 {
                            rmatch[i] = Some(j);
                            pmatch[j] = Some((i, w));
                            break 'found;
                        }
                    }
                }
            }
        }
    }
    // Pass 2: nearest remaining quad of the same texture within 64 px → changed.
    let mut near: HashMap<(&str, i64, i64), Vec<usize>> = HashMap::new();
    let gc = |c: f32| (c / 32.0).floor() as i64;
    for (i, it) in ri.iter().enumerate() {
        if rmatch[i].is_none() {
            near.entry((&it.prim.tex, gc(it.c[0]), gc(it.c[1])))
                .or_default()
                .push(i);
        }
    }
    for (j, it) in pi.iter().enumerate() {
        if pmatch[j].is_some() {
            continue;
        }
        let (gx, gy) = (gc(it.c[0]), gc(it.c[1]));
        let mut best: Option<(f32, usize)> = None;
        for dx in -2..=2 {
            for dy in -2..=2 {
                let Some(c) = near.get(&(&*it.prim.tex, gx + dx, gy + dy)) else {
                    continue;
                };
                for &i in c {
                    if rmatch[i].is_some() {
                        continue;
                    }
                    let a = &ri[i];
                    let dist = (a.c[0] - it.c[0]).hypot(a.c[1] - it.c[1]);
                    if dist > 64.0 {
                        continue;
                    }
                    let cost = dist + if a.key != it.key { 50.0 } else { 0.0 };
                    if best.is_none_or(|b| cost < b.0) {
                        best = Some((cost, i));
                    }
                }
            }
        }
        if let Some((_, i)) = best {
            rmatch[i] = Some(j);
            pmatch[j] = Some((i, compare(&ri[i], it)));
        }
    }
    for (i, it) in ri.iter().enumerate() {
        let l = d.by_label.entry(it.prim.tex.clone()).or_default();
        if rmatch[i].is_none() {
            l.missing += 1;
        }
    }
    for (j, it) in pi.iter().enumerate() {
        let l = d.by_label.entry(it.prim.tex.clone()).or_default();
        match pmatch[j] {
            None => l.extra += 1,
            Some((_, w)) => {
                if w & !D_HIDDEN == 0 {
                    l.same += 1;
                } else {
                    l.changed += 1;
                }
                for k in 0..6 {
                    if w & (1 << k) != 0 {
                        l.why[k] += 1;
                    }
                }
            }
        }
    }
    for l in d.by_label.values() {
        d.total.add(&l.clone());
    }
    let mut pairs: Vec<(usize, usize)> = rmatch
        .iter()
        .enumerate()
        .filter_map(|(i, m)| m.map(|j| (ri[i].order, pi[j].order)))
        .collect();
    pairs.sort();
    let seq: Vec<usize> = pairs.iter().map(|p| p.1).collect();
    d.reordered = seq.len() - lis(&seq);
    let set = |v: &[Item]| -> BTreeMap<SpriteKey, ()> {
        v.iter()
            .map(|it| ((it.prim.tex.clone(), it.key), ()))
            .collect()
    };
    let (rs, ps) = (set(&ri), set(&pi));
    d.sprites_missing = rs
        .keys()
        .filter(|k| !ps.contains_key(*k))
        .cloned()
        .collect();
    d.sprites_extra = ps
        .keys()
        .filter(|k| !rs.contains_key(*k))
        .cloned()
        .collect();
    let mut po_left = po.clone();
    for x in &ro {
        if let Some(k) = po_left.iter().position(|y| y == x) {
            po_left.remove(k);
        } else {
            d.other_missing.push(x.clone());
        }
    }
    d.other_extra = po_left;
    d
}

// ---- session over many frames ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyMode {
    Scene,
    Demo,
    Present,
}

#[derive(Clone, Debug)]
pub struct FrameNote {
    pub key: String,
    pub ref_present: i64,
    pub port_present: i64,
    pub score: usize,
    pub text: String,
    pub game_frame: i64,
    pub demo: i32,
}

#[derive(Debug, Default)]
pub struct Report {
    pub compared: usize,
    pub identical: usize,
    pub ref_only: usize,
    pub port_only: usize,
    pub first: Option<FrameNote>,
    pub worst: Vec<FrameNote>,
    pub total: LabelDiff,
    pub reordered: usize,
    pub ref_quads: usize,
    pub by_label: BTreeMap<String, LabelDiff>,
    pub others_missing: BTreeMap<String, usize>,
    pub others_extra: BTreeMap<String, usize>,
    pub ref_sprites: BTreeMap<(String, [i32; 4]), (i64, usize)>,
    pub port_sprites: BTreeMap<(String, [i32; 4]), (i64, usize)>,
    pub worst_n: usize,
    // (ref present, port present, ref game frame, score) of every compared frame.
    pub pairs: Vec<(i64, i64, i64, usize)>,
}

impl Report {
    pub fn new(worst_n: usize) -> Self {
        Self {
            worst_n,
            ..Default::default()
        }
    }

    // Exact-match percentage of the reference quads.
    pub fn match_pct(&self) -> f64 {
        if self.ref_quads == 0 {
            100.0
        } else {
            100.0 * self.total.same as f64 / self.ref_quads as f64
        }
    }

    pub fn add(&mut self, key: String, r: &Frame, p: &Frame, d: &FrameDiff) {
        self.compared += 1;
        let score = d.score();
        self.pairs.push((r.present, p.present, r.game_frame, score));
        if score == 0 {
            self.identical += 1;
        }
        self.total.add(&d.total);
        self.reordered += d.reordered;
        self.ref_quads += d.ref_quads;
        for (l, x) in &d.by_label {
            self.by_label.entry(l.to_string()).or_default().add(x);
        }
        for o in &d.other_missing {
            *self.others_missing.entry(o.clone()).or_default() += 1;
        }
        for o in &d.other_extra {
            *self.others_extra.entry(o.clone()).or_default() += 1;
        }
        let note = || FrameNote {
            key: key.clone(),
            ref_present: r.present,
            port_present: p.present,
            score,
            text: d.describe(8),
            game_frame: r.game_frame,
            demo: r.demo,
        };
        if score > 0 && self.first.is_none() {
            self.first = Some(note());
        }
        // Worst frames, at least 60 presents apart so they show different moments.
        if score > 0 && self.worst_n > 0 {
            let near = self
                .worst
                .iter()
                .position(|w| (w.ref_present - r.present).abs() < 60);
            match near {
                Some(i) if self.worst[i].score >= score => {}
                Some(i) => self.worst[i] = note(),
                None if self.worst.len() < self.worst_n => self.worst.push(note()),
                None if self.worst.last().is_some_and(|w| w.score < score) => {
                    *self.worst.last_mut().unwrap() = note();
                }
                None => {}
            }
            self.worst.sort_by_key(|w| std::cmp::Reverse(w.score));
        }
    }

    pub fn note_sprites(&mut self, fr: &Frame, port: bool, o: &Opts) {
        let mut others = Vec::new();
        let set = if port {
            &mut self.port_sprites
        } else {
            &mut self.ref_sprites
        };
        for it in items(fr, o, &mut others) {
            let e = set
                .entry((it.prim.tex.to_string(), it.key))
                .or_insert((fr.present, 0));
            e.1 += 1;
        }
    }

    pub fn summary(&self) -> String {
        let t = &self.total;
        let mut s = format!(
            "{} frames compared ({} identical), {} only in ref, {} only in port\n\
             ref quads {}: same {} ({:.2}%), changed {}, missing {}, extra {}, reordered {}\n",
            self.compared,
            self.identical,
            self.ref_only,
            self.port_only,
            self.ref_quads,
            t.same,
            self.match_pct(),
            t.changed,
            t.missing,
            t.extra,
            self.reordered
        );
        if t.why.iter().any(|&x| x > 0) {
            let _ = writeln!(s, "changed by: {}", why_str(&t.why));
        }
        match &self.first {
            Some(n) => {
                let _ = writeln!(
                    s,
                    "first divergence: {} (ref present {}, port present {})\n  {}",
                    n.key,
                    n.ref_present,
                    n.port_present,
                    n.text.replace('\n', "\n  ")
                );
            }
            None => s.push_str("no divergence\n"),
        }
        let mut labels: Vec<_> = self
            .by_label
            .iter()
            .filter(|(_, d)| d.missing + d.extra + d.changed > 0)
            .collect();
        labels.sort_by_key(|(_, d)| std::cmp::Reverse(d.missing + d.extra + d.changed));
        s.push_str("by texture (missing / extra / changed):\n");
        for (l, d) in labels.iter().take(25) {
            let _ = write!(
                s,
                "  {l:24} same {:7} missing {:7} extra {:7} changed {:7}",
                d.same, d.missing, d.extra, d.changed
            );
            if d.why.iter().any(|&x| x > 0) {
                let _ = write!(s, "  ({})", why_str(&d.why));
            }
            s.push('\n');
        }
        for (name, m) in [
            ("missing", &self.others_missing),
            ("extra", &self.others_extra),
        ] {
            let mut v: Vec<_> = m.iter().collect();
            v.sort_by_key(|x| std::cmp::Reverse(*x.1));
            for (o, n) in v.iter().take(6) {
                let _ = writeln!(s, "  {name} in {n} frames: {o}");
            }
        }
        let only = |a: &BTreeMap<(String, [i32; 4]), (i64, usize)>,
                    b: &BTreeMap<(String, [i32; 4]), (i64, usize)>| {
            a.iter()
                .filter(|(k, _)| !b.contains_key(*k))
                .map(|((l, r), (f, n))| {
                    format!("{l}[{},{},{},{}] first {f} ×{n}", r[0], r[1], r[2], r[3])
                })
                .collect::<Vec<_>>()
        };
        if !self.ref_sprites.is_empty() {
            let a = only(&self.ref_sprites, &self.port_sprites);
            let b = only(&self.port_sprites, &self.ref_sprites);
            let _ = writeln!(s, "sprites never drawn by the port: {}", a.len());
            for x in a.iter().take(30) {
                let _ = writeln!(s, "  {x}");
            }
            let _ = writeln!(s, "sprites only the port draws: {}", b.len());
            for x in b.iter().take(30) {
                let _ = writeln!(s, "  {x}");
            }
        }
        s
    }
}

// Scene key with visit numbers: (scene, n-th entry of that scene, scene_frame).
pub fn scene_keys(frames: &[Frame]) -> Vec<(i32, u32, i64)> {
    let mut visits: HashMap<i32, u32> = HashMap::new();
    let mut last = i32::MIN;
    frames
        .iter()
        .map(|f| {
            if f.scene != last || f.scene_frame == 1 {
                *visits.entry(f.scene).or_default() += 1;
                last = f.scene;
            }
            (f.scene, visits[&f.scene], f.scene_frame)
        })
        .collect()
}

// In-memory comparison keyed by scene (title intro: ±1 frame slack) or present.
pub fn diff_all(r: &[Frame], p: &[Frame], mode: KeyMode, o: &Opts, worst: usize) -> Report {
    let mut rep = Report::new(worst);
    let (rk, pk) = match mode {
        KeyMode::Scene => (scene_keys(r), scene_keys(p)),
        _ => (
            r.iter().map(|f| (0, 0, f.present)).collect(),
            p.iter().map(|f| (0, 0, f.present)).collect(),
        ),
    };
    let index: HashMap<(i32, u32, i64), usize> =
        pk.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    let mut used = vec![false; p.len()];
    for (i, k) in rk.iter().enumerate() {
        let slack = mode == KeyMode::Scene && k.0 == 1 && k.1 == 1 && k.2 <= 120;
        let cands: Vec<i64> = if slack { vec![0, -1, 1] } else { vec![0] };
        let mut best: Option<(usize, FrameDiff)> = None;
        for dk in cands {
            let Some(&j) = index.get(&(k.0, k.1, k.2 + dk)) else {
                continue;
            };
            let d = diff_frame(&r[i], &p[j], o);
            if best.as_ref().is_none_or(|b| d.score() < b.1.score()) {
                best = Some((j, d));
            }
            if best.as_ref().is_some_and(|b| b.1.score() == 0) {
                break;
            }
        }
        rep.note_sprites(&r[i], false, o);
        match best {
            Some((j, d)) => {
                used[j] = true;
                let key = match mode {
                    KeyMode::Scene => format!("scene {} visit {} frame {}", k.0, k.1, k.2),
                    _ => format!("present {}", k.2),
                };
                rep.add(key, &r[i], &p[j], &d);
            }
            None => rep.ref_only += 1,
        }
    }
    for f in p {
        rep.note_sprites(f, true, o);
    }
    rep.port_only = used.iter().filter(|u| !**u).count();
    rep
}

// Streaming comparison keyed by demo frame (frames of one demo frame number are paired in order).
pub struct DemoMerge<I: Iterator<Item = Frame>> {
    reference: I,
    held: Option<(Frame, (i64, u32))>,
    ref_occ: (i64, u32),
    port_occ: (i64, u32),
    pub opts: Opts,
    pub report: Report,
    pub range: Option<(i64, i64)>,
    // Demo: demo frame (frames outside demos skipped); Present: present number.
    pub key: KeyMode,
}

impl<I: Iterator<Item = Frame>> DemoMerge<I> {
    pub fn new(reference: I, opts: Opts, worst: usize) -> Self {
        Self {
            reference,
            held: None,
            ref_occ: (-1, 0),
            port_occ: (-1, 0),
            opts,
            report: Report::new(worst),
            range: None,
            key: KeyMode::Demo,
        }
    }

    fn frame_key(&self, f: &Frame) -> Option<i64> {
        match self.key {
            KeyMode::Present => Some(f.present),
            _ => (f.demo >= 0).then_some(f.demo_frame),
        }
    }

    fn in_range(&self, df: i64) -> bool {
        self.range.is_none_or(|(a, b)| df >= a && df <= b)
    }

    fn occ(last: &mut (i64, u32), df: i64) -> (i64, u32) {
        let n = if last.0 == df { last.1 + 1 } else { 0 };
        *last = (df, n);
        *last
    }

    fn next_ref(&mut self) -> Option<(Frame, (i64, u32))> {
        if let Some(h) = self.held.take() {
            return Some(h);
        }
        loop {
            let f = self.reference.next()?;
            let Some(fk) = self.frame_key(&f) else {
                continue;
            };
            let k = Self::occ(&mut self.ref_occ, fk);
            if !self.in_range(fk) {
                continue;
            }
            self.report.note_sprites(&f, false, &self.opts);
            return Some((f, k));
        }
    }

    pub fn feed(&mut self, p: &Frame) {
        let Some(pk) = self.frame_key(p) else {
            return;
        };
        let key = Self::occ(&mut self.port_occ, pk);
        if !self.in_range(pk) {
            return;
        }
        self.report.note_sprites(p, true, &self.opts);
        loop {
            let Some((r, rk)) = self.next_ref() else {
                self.report.port_only += 1;
                return;
            };
            match rk.cmp(&key) {
                std::cmp::Ordering::Less => self.report.ref_only += 1,
                std::cmp::Ordering::Equal => {
                    let d = diff_frame(&r, p, &self.opts);
                    let k = match self.key {
                        KeyMode::Present => format!(
                            "present {} (scene {} frame {})",
                            key.0, r.scene, r.scene_frame
                        ),
                        _ => format!("demo frame {} #{}", key.0, key.1),
                    };
                    self.report.add(k, &r, p, &d);
                    return;
                }
                std::cmp::Ordering::Greater => {
                    self.held = Some((r, rk));
                    self.report.port_only += 1;
                    return;
                }
            }
        }
    }

    // Counts the reference frames left after the port stopped.
    pub fn finish(mut self) -> Report {
        while self.next_ref().is_some() {
            self.report.ref_only += 1;
        }
        self.report
    }
}

// ---- headless runs of the port (tests, drawrun) ----

// One calc + draw + present; returns the logged frame text.
pub fn step(ctx: &mut crate::game::Ctx) -> Option<(crate::task::Flow, Vec<String>)> {
    let f = ctx.calc();
    ctx.draw();
    ctx.gfx.present();
    let frames = ctx.drawlog.as_mut().map(|w| w.take_frames())?;
    Some((f, frames))
}

// Menu CLRD pokes of menu_clr (README): any-clear / no-continue masks of teams 0–2 on E–L.
pub fn clr_pokes(g: &mut crate::gamemgr::Game) {
    for (t, any, nc) in [(0, 0x41, 0x01), (1, 0x81, 0x81), (2, 0xC1, 0xC1)] {
        g.clear_any[t][..4].fill(any);
        g.clear_nc[t][..4].fill(nc);
    }
}

// Start state of a menu script (README of specs/data/drawlog): a private save directory, empty for menu_base /
// menu_clr; menu_mopup1 adds its score.dat and the four attract demos as replay/th8_01..04.rpy.
#[cfg(not(target_arch = "wasm32"))]
pub fn menu_start_state(ctx: &mut crate::game::Ctx, name: &str) -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!(
        "th08_{name}_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("replay"))?;
    if name == "menu_mopup1" {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("specs/data/drawlog");
        let score = std::fs::read(data.join("menu_mopup1.score.dat"))?;
        ctx.game.load_score(Some(&score));
        for n in 0..4 {
            let r = ctx.archive.read(&format!("demorpy{n}.rpy"))?;
            std::fs::write(dir.join(format!("replay/th8_{:02}.rpy", n + 1)), r)?;
        }
    }
    crate::scoredat::set_save_dir(Some(dir));
    Ok(())
}

// Replays a menu input script: script frame 1 = the port's first main-menu frame (boot frames get input 0).
pub fn run_menu_script(
    ctx: &mut crate::game::Ctx,
    script: &[u16],
    clr: bool,
    mut out: impl FnMut(String),
) {
    ctx.log_to_memory();
    let mut k = 0usize;
    for _ in 0..script.len() + 600 {
        if k >= script.len() {
            break;
        }
        ctx.input.raw = if k > 0 { script[k] } else { 0 };
        if clr && (k + 1 == 100 || k + 1 == 280) {
            clr_pokes(&mut ctx.game);
        }
        let Some((flow, frames)) = step(ctx) else {
            return;
        };
        frames.into_iter().for_each(&mut out);
        if k > 0 || ctx.game.scene == crate::gamemgr::SCENE_MENU {
            k += 1;
        }
        if flow != crate::task::Flow::Continue {
            break;
        }
    }
}

// Plays attract demo `n` from boot (no fast-forward) up to demo frame `last`.
pub fn run_demo(
    ctx: &mut crate::game::Ctx,
    n: u8,
    last: i32,
    mut out: impl FnMut(String),
) -> anyhow::Result<()> {
    let r = crate::replay::Replay::parse(&ctx.archive.read(&format!("demorpy{n}.rpy"))?)?;
    ctx.game.replay_ff = false;
    anyhow::ensure!(
        crate::replay::start_playback(&mut ctx.game, r, None, Some(n)),
        "demo has no stage"
    );
    ctx.log_to_memory();
    let mut started = false;
    for _ in 0..last as usize + 2000 {
        let Some((flow, frames)) = step(ctx) else {
            break;
        };
        frames.into_iter().for_each(&mut out);
        let demo = ctx.game.flags & crate::gamemgr::F_DEMO != 0;
        started |= demo;
        if flow != crate::task::Flow::Continue || (started && !demo) || ctx.game.demo_frame > last {
            break;
        }
    }
    Ok(())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::gfx::{LogTex, Rect};

    // Current match level per reference (raise as differences get fixed): (identical frames, % same quads).
    const MENU_BASE: (usize, f64) = (1930, 73.9);
    const MENU_CLR: (usize, f64) = (900, 97.7);
    const MENU_MOPUP1: (usize, f64) = (2700, 99.7);
    // Demos 0–3, first chunk: % same quads (playfield).
    const DEMO: [f64; 4] = [90.6, 71.6, 83.3, 86.9];

    fn data(p: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("specs/data/drawlog")
            .join(p)
    }

    fn ctx() -> Option<crate::game::Ctx> {
        let dir = std::env::var("TH08_DIR").ok()?;
        let gfx = pollster::block_on(crate::gfx::Gfx::headless()).ok()?;
        let src = crate::assets::Source::native(dir.into());
        Some(pollster::block_on(crate::game::Ctx::load_with(src, gfx, false)).unwrap())
    }

    #[test]
    fn format_round_trip() {
        let tex = Some(LogTex {
            label: "etama.anm#1".into(),
            w: 256,
            h: 256,
        });
        let v = |x: f32, y: f32, u: f32, v: f32| {
            Vertex::screen(x + 0.5, y + 0.5, 0.0, [u, v], [255, 255, 255, 128])
        };
        let rect = [
            v(10.0, 20.0, 0.0, 0.0),
            v(26.0, 20.0, 0.0625, 0.0),
            v(10.0, 36.0, 0.0, 0.0625),
            v(26.0, 36.0, 0.0625, 0.0625),
        ];
        let mut rot = rect;
        rot[1].pos[1] += 0.01;
        let quad = |v| LogRec::Quad {
            tex: tex.clone(),
            id: 3,
            textured: true,
            blend: Blend::Add,
            test: false,
            write: true,
            vp: Rect::new(32.0, 16.0, 384.0, 448.0),
            fog: None,
            v,
        };
        let recs = [quad(rect), quad(rot)];
        let h = Header {
            present: 7,
            scene: 2,
            scene_frame: 3,
            game_frame: 1,
            demo: 0,
            demo_frame: 1,
            input: 0x11,
        };
        let text = format_frame(&h, &recs);
        assert!(text.starts_with("F 7 2 3 1 0 1 0011 2\n"), "{text}");
        assert!(
            text.contains("\nR 10 20 26 36 0 0 0.0625 0.0625 80ffffff\n"),
            "{text}"
        );
        assert!(text.contains("\nQ "), "{text}");
        assert!(text.contains("S blend=add atest=ge4 ztest=always zwrite=1"));
        let f = parse_str(&text);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].prims.len(), 2);
        let d = diff_frame(&f[0], &f[0], &Opts { playfield: false });
        assert_eq!((d.score(), d.total.same), (0, 2));
    }

    fn menu(name: &str, clr: bool, min: (usize, f64)) {
        let reference = data(&format!("{name}.drawlog.xz"));
        let Some(mut ctx) = ctx() else {
            eprintln!("no TH08_DIR / GPU: {name} skipped");
            return;
        };
        let Ok(r) = open(&[reference]) else {
            eprintln!("cannot read {name}: skipped");
            return;
        };
        let r: Vec<Frame> = r.collect();
        if r.is_empty() {
            eprintln!("xz unavailable: {name} skipped");
            return;
        }
        menu_start_state(&mut ctx, name).unwrap();
        let input = std::fs::read_to_string(data(&format!("{name}.input.txt"))).unwrap();
        let script = crate::input::parse_script(&input).unwrap();
        let mut text = String::new();
        crate::drawlog::run_menu_script(&mut ctx, &script, clr, |s| text.push_str(&s));
        let p = parse_str(&text);
        let rep = diff_all(&r, &p, KeyMode::Scene, &Opts { playfield: false }, 4);
        eprintln!("{name}:\n{}", rep.summary());
        assert!(
            rep.identical >= min.0 && rep.match_pct() >= min.1,
            "{name} regressed: {} identical frames, {:.2}% same quads (min {min:?})",
            rep.identical,
            rep.match_pct()
        );
    }

    #[test]
    fn menu_base() {
        menu("menu_base", false, MENU_BASE);
    }

    #[test]
    fn menu_clr() {
        menu("menu_clr", true, MENU_CLR);
    }

    #[test]
    fn menu_mopup1() {
        menu("menu_mopup1", false, MENU_MOPUP1);
    }

    #[test]
    fn demos() {
        for n in 0..4u8 {
            let chunk = data(&format!("demos/demo{n}_00000-01500.drawlog.xz"));
            if !chunk.exists() {
                eprintln!("demo {n}: reference chunk missing, skipped");
                continue;
            }
            let Some(mut ctx) = ctx() else {
                eprintln!("no TH08_DIR / GPU: demos skipped");
                return;
            };
            let mut m = DemoMerge::new(open(&[chunk]).unwrap(), Opts { playfield: true }, 4);
            m.range = Some((0, 1500));
            run_demo(&mut ctx, n, 1500, |s| {
                for f in parse_str(&s) {
                    m.feed(&f);
                }
            })
            .unwrap();
            let rep = m.finish();
            eprintln!("demo {n}:\n{}", rep.summary());
            assert!(
                rep.match_pct() >= DEMO[n as usize],
                "demo {n} regressed: {:.2}% same quads (min {})",
                rep.match_pct(),
                DEMO[n as usize]
            );
        }
    }
}
