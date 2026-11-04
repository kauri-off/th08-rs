//! Draw log text format (specs/data/drawlog/FORMAT.md): per-frame text of the recorded `LogRec`s and the writer
//! `Ctx` feeds; pure (the file / xz opening is `drawlog::open_spec`, tools side).

use crate::drawlist::{Blend, LogRec, Vertex};
use std::fmt::Write as _;

// Decimal with trailing zeros removed ("436.5", "0", "0.00781").
pub fn num(v: f32, dec: usize) -> String {
    if let Some(s) = num_fast(v, dec) {
        return s;
    }
    num_fmt(v, dec)
}

fn num_fmt(v: f32, dec: usize) -> String {
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

// `num` by integer arithmetic (port tooling speed-up): v·10^dec rounded half to even on the exact binary value, as
// `{:.dec$}` does; None (huge, NaN/inf, dec > 9) falls back to the formatter.
fn num_fast(v: f32, dec: usize) -> Option<String> {
    if dec > 9 || !v.is_finite() {
        return None;
    }
    let bits = v.to_bits();
    let (ex, fr) = (
        ((bits >> 23) & 0xff).cast_signed(),
        u64::from(bits & 0x7f_ffff),
    );
    let (m, e) = if ex == 0 {
        (fr, -149)
    } else {
        (fr | 0x80_0000, ex - 150)
    };
    let p = 10u64.pow(dec as u32);
    let x = m * p; // < 2^54
    let n = if e >= 0 {
        if e > 9 {
            return None;
        }
        x << e
    } else if e <= -63 {
        0 // x < 2^54 < half
    } else {
        let k = (-e).cast_unsigned();
        let (q, r, half) = (x >> k, x & ((1 << k) - 1), 1u64 << (k - 1));
        q + u64::from(r > half || (r == half && q & 1 == 1))
    };
    if n == 0 {
        return Some("0".into());
    }
    // Digits right to left; trailing fraction zeros (and a bare '.') never written.
    let mut buf = [0u8; 32];
    let (mut i, mut n, mut frac) = (buf.len(), n, true);
    for _ in 0..dec {
        let d = (n % 10) as u8;
        n /= 10;
        if d != 0 || !frac {
            frac = false;
            i -= 1;
            buf[i] = b'0' + d;
        }
    }
    if !frac {
        i -= 1;
        buf[i] = b'.';
    }
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if bits >> 31 != 0 {
        i -= 1;
        buf[i] = b'-';
    }
    Some(std::str::from_utf8(&buf[i..]).ok()?.to_string())
}

// C `printf("%g")` (6 significant digits, trailing zeros removed), as the probe prints fog start / end (FORMAT.md §2).
pub fn num_g(v: f32) -> String {
    let v = f64::from(v);
    if !v.is_finite() {
        return if v.is_nan() {
            "nan".into()
        } else if v > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let sci = format!("{v:.5e}");
    let (mant, exp) = sci.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let strip = |s: &str| {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    };
    if (-4..6).contains(&exp) {
        strip(&format!("{v:.*}", (5 - exp).cast_unsigned() as usize))
    } else {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", strip(mant), exp.abs())
    }
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
                    u8::from(*write)
                );
                if let Some(f) = fog {
                    let c = argb([f.color[0], f.color[1], f.color[2], 255]);
                    let _ = write!(sl, " fog={c:08x},{},{}", num_g(f.start), num_g(f.end));
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
            LogRec::Clear { vp, color, arg } => {
                set(&mut body, &mut v, vp_line(vp));
                let _ = match color {
                    Some(c) => writeln!(body, "C tz {:08x}", argb(*c)),
                    None => writeln!(body, "C z {arg:08x}"),
                };
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

fn vp_line(r: &crate::drawlist::Rect) -> String {
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
    Stream(Box<dyn std::io::Write>),
}

// Frame filter: present numbers, or demo frames with a `d` prefix (`d1000-1100`).
#[derive(Clone, Copy)]
pub struct Range {
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

    // Text log into `w` (a file or an xz stream opened by the app / tool, `drawlog::open_spec`), only the frames
    // in `range`.
    pub fn stream(w: Box<dyn std::io::Write>, range: Option<Range>) -> Self {
        let mut w = Self::new(Sink::Stream(w));
        w.range = range;
        w
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
            demo: demo.map_or(-1, |d| i32::from(d.0)),
            demo_frame: demo.map_or(0, |d| d.1),
            input,
        };
        if let Some(r) = self.range {
            let k = if r.demo {
                i64::from(h.demo_frame)
            } else {
                h.present.cast_signed()
            };

            if k < r.from || k > r.to || (r.demo && h.demo < 0) {
                return;
            }
        }
        let text = format_frame(&h, recs);
        match &mut self.sink {
            Sink::Memory(v) => v.push(text),
            Sink::Stream(w) => {
                let _ = w.write_all(text.as_bytes());
            }
        }
    }

    pub fn take_frames(&mut self) -> Vec<String> {
        match &mut self.sink {
            Sink::Memory(v) => std::mem::take(v),
            Sink::Stream(_) => Vec::new(),
        }
    }
}

// A `TH08_DRAWLOG` spec `<path>[,from-to | ,dfrom-to]` → (path, range).
pub fn parse_spec(v: &str) -> (&str, Option<Range>) {
    match v.rsplit_once(',') {
        Some((p, r)) => (p, parse_range(r)),
        None => (v, None),
    }
}

// The `,from-to | ,dfrom-to` part of a `TH08_DRAWLOG` spec.
pub fn parse_range(r: &str) -> Option<Range> {
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

#[cfg(test)]
mod tests {
    use super::*;

    // `num_fast` = the formatter path on every f32 bit pattern, for the decimals the log uses (2, 4, 5). ~2 min.
    #[test]
    #[ignore]
    fn num_fast_exhaustive() {
        let t = 32u64;
        std::thread::scope(|sc| {
            for i in 0..t {
                sc.spawn(move || {
                    let mut b = i;
                    while b < 1 << 32 {
                        let v = f32::from_bits(b as u32);
                        for d in [2, 4, 5] {
                            if let Some(s) = num_fast(v, d) {
                                assert_eq!(s, num_fmt(v, d), "{b:#x} {d}");
                            }
                        }
                        b += t;
                    }
                });
            }
        });
    }

    #[test]
    fn num_fast_ties_and_edges() {
        let mut vals = vec![
            0.25f32, 0.75, 0.125, 0.375, 2.5, 3.5, -0.0, 1e-30, 5e-3, 1e12, 0.005, 0.00005,
        ];
        for n in -2000i32..2000 {
            for k in 0..20 {
                vals.push(n as f32 / (1u32 << k) as f32);
            }
        }
        for v in vals {
            for d in [0, 1, 2, 4, 5] {
                assert_eq!(num(v, d), num_fmt(v, d), "{v} {d}");
            }
        }
    }
}
