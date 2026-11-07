//! Render-scale comparison: a session or demo rendered headless at scale 1 and at scale k, compared per frame.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: hirescmp SOURCE [--scale K[,K…]] [--every N] [--frames A,B,..] [--events] [--to N] [--threshold T]
                [--worst N] [--out DIR] [--save DIR] [--ref DIR]
  SOURCE: a recorded session directory, or demo:N (attract demo N of the archive).
  Renders SOURCE headless at scale 1 and at scale K (default 2), box-downsamples each chosen K× frame to 640×480 and
  compares it with the scale-1 frame: max / mean channel difference, % of pixels whose largest channel difference
  exceeds T (default 48), and the bounding box of those pixels. Frames: every N-th Frame_Run (default 600) plus
  --frames, plus with --events the Frame_Runs 1, 10, 30, 60 and 120 after each scene or stage change (pause and
  stage-transition captures, loading screens); --to stops the run after Frame_Run N.
  DIR/Kx/index.html (default DIR /tmp/hirescmp) lists every frame, worst first; the --worst N (default 40) worst
  frames get side-by-side PNGs (scale 1 | downsampled K× | difference ×4) and the full K× frame.
  --save DIR: only render at the (first) scale K and keep the chosen frames as DIR/NNNNNN.png (no comparison).
  --ref DIR: compare the scale-K frames with the PNGs of a --save run instead of a scale-1 run of this build.
  Needs TH08_DIR; the session's own settings come from the usual port.cfg (set XDG_CONFIG_HOME for a clean one).";

struct Args {
    source: String,
    scales: Vec<u32>,
    every: u64,
    frames: Vec<u64>,
    events: bool,
    to: u64,
    threshold: u8,
    worst: usize,
    out: PathBuf,
    save: Option<PathBuf>,
    reference: Option<PathBuf>,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut a = Args {
        source: String::new(),
        scales: vec![2],
        every: 600,
        frames: Vec::new(),
        events: false,
        to: u64::MAX,
        threshold: 48,
        worst: 40,
        out: PathBuf::from("/tmp/hirescmp"),
        save: None,
        reference: None,
    };
    let mut it = std::env::args().skip(1);
    let num = |v: Option<String>, what: &str| -> anyhow::Result<u64> {
        v.and_then(|v| v.parse().ok())
            .ok_or_else(|| anyhow::anyhow!("{what} needs a number\n{USAGE}"))
    };
    while let Some(s) = it.next() {
        match s.as_str() {
            "--scale" => {
                let v = it.next().unwrap_or_default();
                a.scales = v.split(',').map(str::parse).collect::<Result<_, _>>()?;
            }
            "--every" => a.every = num(it.next(), "--every")?,
            "--to" => a.to = num(it.next(), "--to")?,
            "--threshold" => a.threshold = u8::try_from(num(it.next(), "--threshold")?)?,
            "--worst" => a.worst = usize::try_from(num(it.next(), "--worst")?)?,
            "--frames" => {
                let v = it.next().unwrap_or_default();
                for f in v.split(',').filter(|f| !f.is_empty()) {
                    a.frames.push(f.trim().parse()?);
                }
            }
            "--events" => a.events = true,
            "--out" => a.out = it.next().map(PathBuf::from).unwrap_or(a.out),
            "--save" => a.save = it.next().map(PathBuf::from),
            "--ref" => a.reference = it.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            _ if a.source.is_empty() && !s.starts_with('-') => a.source = s,
            _ => anyhow::bail!("unknown argument {s}\n{USAGE}"),
        }
    }
    anyhow::ensure!(!a.source.is_empty(), "{USAGE}");
    anyhow::ensure!(
        !a.scales.is_empty() && a.scales.iter().all(|k| (1..=8).contains(k)),
        "--scale K[,K…], each 1 … 8"
    );
    Ok(a)
}

impl Args {
    fn wanted(&self, n: u64) -> bool {
        (self.every > 0 && n.is_multiple_of(self.every)) || self.frames.contains(&n)
    }
}

fn gfx(ctx: &mut th08_core::game::Ctx) -> &mut th08_render::gfx::Gfx {
    ctx.renderer
        .as_any_mut()
        .downcast_mut::<th08_render::gfx::Gfx>()
        .expect("renderer is Gfx")
}

// Plays the source at render scale `k`; `shot(n, frame)` gets the frame after each wanted Frame_Run n.
fn render(a: &Args, k: u32, mut shot: impl FnMut(u64, image::RgbaImage)) -> anyhow::Result<()> {
    let dir = th08::settings::test_game_dir().ok_or_else(|| anyhow::anyhow!("set TH08_DIR"))?;
    let mut g = pollster::block_on(th08_render::gfx::Gfx::headless())?;
    g.offscreen = true;
    g.render_scale = th08_render::gfx::RenderScale::Fixed(k);
    let src = th08::sources::native(dir);
    let mut ctx = pollster::block_on(th08_core::game::Ctx::load_with(
        src.clone(),
        th08::settings::get().tool_io(g, &src),
        th08::settings::get().core(),
        false,
    ))?;
    let mut n = 0u64;
    let mut seen = None;
    let mut extra = std::collections::BTreeSet::new();
    let mut after = |ctx: &mut th08_core::game::Ctx| -> bool {
        n += 1;
        let now = (ctx.w.game.scene, ctx.w.game.stage);
        if a.events && seen.replace(now).is_some_and(|s| s != now) {
            extra.extend([1, 10, 30, 60, 120].map(|d| n + d));
        }
        if a.wanted(n) || extra.remove(&n) {
            let img = gfx(ctx).read_frame();
            shot(n, img);
        }
        n < a.to
    };
    if let Some(d) = a.source.strip_prefix("demo:") {
        let d: u8 = d.parse()?;
        let r = th08_core::replay::parse(
            &ctx.archive.read(&format!("demorpy{d}.rpy"))?,
            ctx.w.game.ver_list.get(),
        )?;
        ctx.w.game.replay_ff = false;
        anyhow::ensure!(
            th08_core::replay::start_playback(&mut ctx.w.game, r, None, Some(d)),
            "demo has no stage"
        );
        let mut started = false;
        for _ in 0..200_000 {
            let flow = ctx.run_frame();
            let demo = ctx.w.game.flags & th08_core::gamemgr::F_DEMO != 0;
            started |= demo;
            if !after(&mut ctx) || flow != th08_core::task::Flow::Continue || (started && !demo) {
                break;
            }
        }
        return Ok(());
    }
    let s = th08_tools::session::Session::load(Path::new(&a.source))?;
    s.start(&mut ctx)?;
    let mut stop = false;
    s.rec.run(
        &mut ctx,
        usize::MAX,
        |ctx, poll| {
            if stop {
                return None;
            }
            th08::platform::tool_host(ctx).cursor = poll;
            let flow = ctx.run_frame();
            stop = !after(ctx);
            Some(flow)
        },
        |_, _| true,
    );
    Ok(())
}

// Mean of each k×k block (rounded), back to 640×480.
#[expect(clippy::cast_possible_truncation, reason = "mean of bytes")]
fn downsample(img: &image::RgbaImage, k: u32) -> image::RgbaImage {
    if k == 1 {
        return img.clone();
    }
    let (w, h) = (img.width() / k, img.height() / k);
    let kk = k * k;
    image::RgbaImage::from_fn(w, h, |x, y| {
        let mut acc = [0u32; 4];
        for dy in 0..k {
            for dx in 0..k {
                let p = img.get_pixel(x * k + dx, y * k + dy).0;
                for (a, v) in acc.iter_mut().zip(p) {
                    *a += u32::from(v);
                }
            }
        }
        image::Rgba(acc.map(|a| ((a + kk / 2) / kk) as u8))
    })
}

struct Metrics {
    frame: u64,
    max: u8,
    mean: f64,
    over: f64,
    // Bounding box `[x0, y0, x1, y1]` of the pixels over the threshold.
    bbox: Option<[u32; 4]>,
}

// RGB only: the frame's alpha is blend residue the window never shows.
fn compare(frame: u64, a: &image::RgbaImage, b: &image::RgbaImage, t: u8) -> Metrics {
    let (mut max, mut sum, mut over) = (0u8, 0u64, 0u64);
    let mut bbox: Option<[u32; 4]> = None;
    for (x, y, p) in a.enumerate_pixels() {
        let q = b.get_pixel(x, y);
        let d = (0..3).map(|c| p.0[c].abs_diff(q.0[c])).max().unwrap_or(0);
        sum += (0..3)
            .map(|c| u64::from(p.0[c].abs_diff(q.0[c])))
            .sum::<u64>();
        max = max.max(d);
        if d > t {
            over += 1;
            let r = bbox.get_or_insert([x, y, x, y]);
            *r = [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)];
        }
    }
    let px = f64::from(a.width() * a.height());
    Metrics {
        frame,
        max,
        mean: sum as f64 / (px * 3.0),
        over: over as f64 * 100.0 / px,
        bbox,
    }
}

fn diff_image(a: &image::RgbaImage, b: &image::RgbaImage) -> image::RgbaImage {
    image::RgbaImage::from_fn(a.width(), a.height(), |x, y| {
        let (p, q) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
        let d = |c: usize| p[c].abs_diff(q[c]).saturating_mul(4);
        image::Rgba([d(0), d(1), d(2), 255])
    })
}

fn opaque(img: &image::RgbaImage) -> image::RgbaImage {
    let mut o = img.clone();
    o.pixels_mut().for_each(|p| p.0[3] = 255);
    o
}

fn side_by_side(a: &image::RgbaImage, b: &image::RgbaImage) -> image::RgbaImage {
    let (w, h) = (a.width(), a.height());
    let mut out = image::RgbaImage::from_pixel(w * 3 + 8, h, image::Rgba([255, 0, 255, 255]));
    image::imageops::replace(&mut out, &opaque(a), 0, 0);
    image::imageops::replace(&mut out, &opaque(b), i64::from(w + 4), 0);
    image::imageops::replace(&mut out, &diff_image(a, b), i64::from(2 * w + 8), 0);
    out
}

type Frames = std::collections::BTreeMap<u64, image::RgbaImage>;

fn main() -> anyhow::Result<()> {
    let _ = env_logger::try_init();
    let a = parse_args()?;
    if let Some(dir) = &a.save {
        std::fs::create_dir_all(dir)?;
        let mut err = None;
        render(&a, a.scales[0], |n, img| {
            if let Err(e) = img.save(dir.join(format!("{n:06}.png"))) {
                err.get_or_insert(e);
            }
        })?;
        return err.map_or(Ok(()), |e| Err(e.into()));
    }
    let mut base = Frames::new();
    if let Some(dir) = &a.reference {
        for e in std::fs::read_dir(dir)? {
            let p = e?.path();
            let n = p
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<u64>().ok());
            if let Some(n) = n {
                base.insert(n, image::open(&p)?.to_rgba8());
            }
        }
        eprintln!("reference: {} frames from {}", base.len(), dir.display());
    } else {
        render(&a, 1, |n, img| {
            base.insert(n, img);
        })?;
        eprintln!("scale 1: {} frames", base.len());
    }
    let mut summary = String::new();
    for &k in &a.scales {
        let line = report(&a, k, &base, &a.out.join(format!("{k}x")))?;
        let _ = writeln!(summary, "<li><a href={k}x/index.html>{k}×</a>: {line}");
    }
    std::fs::write(
        a.out.join("index.html"),
        format!(
            "<!doctype html><meta charset=utf-8><title>hirescmp</title><h1>{}</h1><ul>{summary}</ul>",
            a.source
        ),
    )?;
    Ok(())
}

// Renders at scale `k`, compares with `base`, writes `out`/index.html, metrics.csv and the worst PNGs.
fn report(a: &Args, k: u32, base: &Frames, out: &Path) -> anyhow::Result<String> {
    std::fs::create_dir_all(out.join("png"))?;
    let mut rows = Vec::new();
    let mut big = std::collections::BTreeMap::new();
    render(a, k, |n, img| {
        let Some(one) = base.get(&n) else {
            return;
        };
        let small = downsample(&img, img.width() / one.width().max(1));
        let m = compare(n, one, &small, a.threshold);
        if m.max > 0 {
            big.insert(n, (img, small));
        }
        rows.push(m);
        // Keep only what the worst list can still use.
        if big.len() > a.worst * 2 {
            let mut keep: Vec<&Metrics> = rows.iter().collect();
            keep.sort_by(|x, y| y.over.total_cmp(&x.over).then(y.mean.total_cmp(&x.mean)));
            let keep: Vec<u64> = keep.iter().take(a.worst).map(|m| m.frame).collect();
            big.retain(|f, _| keep.contains(f));
        }
    })?;
    rows.sort_by(|x, y| y.over.total_cmp(&x.over).then(y.mean.total_cmp(&x.mean)));
    let mut csv = String::from("frame,max,mean,pct_over,x0,y0,x1,y1\n");
    let mut html = format!(
        "<!doctype html><meta charset=utf-8><title>hirescmp {k}x</title>\
         <style>body{{font:13px sans-serif;background:#222;color:#ddd}}td{{padding:2px 8px}}img{{max-width:100%}}\
         a{{color:#8cf}}</style><h1>{} — scale 1 vs {k}× (box-downsampled), threshold {}</h1>\
         <p>side-by-side: scale 1 | {k}× downsampled | |difference| × 4</p><table>\
         <tr><th>frame<th>max<th>mean<th>% over<th>bbox<th>images",
        a.source, a.threshold
    );
    let (mut zero, mut worst_max) = (0usize, 0u8);
    for (i, m) in rows.iter().enumerate() {
        let b = m
            .bbox
            .map(|b| b.map(|v| v.to_string()).join(","))
            .unwrap_or_else(|| ",,,".into());
        let _ = writeln!(csv, "{},{},{:.4},{:.4},{b}", m.frame, m.max, m.mean, m.over);
        zero += usize::from(m.max == 0);
        worst_max = worst_max.max(m.max);
        let mut imgs = String::new();
        if i < a.worst
            && let Some((img, small)) = big.get(&m.frame)
            && let Some(one) = base.get(&m.frame)
        {
            let sbs = format!("png/{:06}_sbs.png", m.frame);
            let full = format!("png/{:06}_{k}x.png", m.frame);
            side_by_side(one, small).save(out.join(&sbs))?;
            opaque(img).save(out.join(&full))?;
            imgs = format!("<a href={full}>{k}× frame</a><br><a href={sbs}><img src={sbs}></a>");
        }
        let _ = write!(
            html,
            "<tr><td>{}<td>{}<td>{:.3}<td>{:.3}<td>{b}<td>{imgs}",
            m.frame, m.max, m.mean, m.over
        );
    }
    html.push_str("</table>");
    std::fs::write(out.join("metrics.csv"), csv)?;
    std::fs::write(out.join("index.html"), html)?;
    let n = rows.len().max(1) as f64;
    let line = format!(
        "{} frames, {zero} identical, max channel difference {worst_max}, mean {:.3}, over {:.4}%",
        rows.len(),
        rows.iter().map(|m| m.mean).sum::<f64>() / n,
        rows.iter().map(|m| m.over).sum::<f64>() / n,
    );
    println!("scale {k}: {line}; {}", out.join("index.html").display());
    for m in rows.iter().take(5) {
        println!(
            "  frame {:6} max {:3} mean {:.3} over {:.3}% bbox {:?}",
            m.frame, m.max, m.mean, m.over, m.bbox
        );
    }
    Ok(line)
}
