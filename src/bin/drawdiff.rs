//! Draw-log diff: `drawdiff <reference> <port-log> [from-to] [--playfield|--full] [--key scene|demo|present]
//! [--worst N] [--out DIR]`. Prints per-frame differences and writes DIR/index.html (default /tmp/drawdiff) with
//! renders of both logs (port renderer, textures from TH08_DIR) for the worst frames and the reference screenshots.

use std::path::{Path, PathBuf};
use th08::drawlog::{self, DemoMerge, Frame, KeyMode, Opts};
use th08::drawreport::{self, Entry};

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
    let rep = if mode != KeyMode::Scene {
        let mut m = DemoMerge::new(drawlog::open(&ref_paths)?, opts, worst_n);
        m.range = range;
        m.key = mode;
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

    let mut entries = drawreport::entries(&rep);
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
    let names = (pos[0].as_str(), pos[1].as_str());
    match mem {
        Some((r, p)) => {
            drawreport::write_html(&out, names, &rep, &entries, r.into_iter(), p.into_iter())
        }
        None => drawreport::write_html(
            &out,
            names,
            &rep,
            &entries,
            drawlog::open(&ref_paths)?,
            drawlog::open(&port_paths)?,
        ),
    }
}
