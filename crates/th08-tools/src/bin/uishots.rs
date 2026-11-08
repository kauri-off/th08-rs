//! Launcher and port-menu screenshot matrix over a fixture save / game folder, with the `uicheck` layout check.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use th08::launcher::{Probe, ShotOpts, Tab, View};
use th08::platform::portmenu::{Nav, Shot, ShotAsk};
use th08::uicheck::{Issue, Kind};
use th08_render::gfx::SafeArea;

const USAGE: &str = "usage: uishots [OUT_DIR] [--only=launcher|portmenu] [--lang=en,ja,ru] [--size=SUBSTR] [--all]
Renders every launcher view and port-menu page × window size × OS scale × language over a fixture (temp config /
save dir with stats and replays, nested game folder) and runs the layout check; exit 1 when it finds issues.
Without OUT_DIR only the check runs (no GPU); with it: PNGs, index.html and sheet_*.png contact sheets.
--size keeps sizes whose name contains SUBSTR (phone, 1674x738, @2x); --all prints every issue.
--fixture=DIR only copies the fixture's save dir (score.dat, stats.jsonl, replay/) to DIR (for the Android check).";

#[derive(Default)]
struct Args {
    out: Option<PathBuf>,
    only: Option<String>,
    lang: Vec<String>,
    size: Option<String>,
    all: bool,
    fixture: Option<PathBuf>,
}

impl Args {
    fn parse() -> Self {
        let mut a = Args::default();
        for x in std::env::args().skip(1) {
            if let Some(v) = x.strip_prefix("--only=") {
                a.only = Some(v.into());
            } else if let Some(v) = x.strip_prefix("--lang=") {
                a.lang = v.split(',').map(String::from).collect();
            } else if let Some(v) = x.strip_prefix("--size=") {
                a.size = Some(v.into());
            } else if x == "--help" || x == "-h" {
                println!("{USAGE}");
                std::process::exit(0);
            } else if let Some(v) = x.strip_prefix("--fixture=") {
                a.fixture = Some(v.into());
            } else if x == "--all" {
                a.all = true;
            } else if x.starts_with('-') || a.out.is_some() {
                eprintln!("{USAGE}");
                std::process::exit(2);
            } else {
                a.out = Some(x.into());
            }
        }
        a
    }
}

#[derive(Clone, Copy)]
struct Size {
    name: &'static str,
    px: (u32, u32),
    density: f32,
    safe: SafeArea,
    phone: bool,
}

fn sizes() -> Vec<Size> {
    let mut v = Vec::new();
    for px in [(1280, 960), (1674, 738), (1920, 1080), (2560, 1440)] {
        for (d, dn) in [(1.0, "1x"), (1.5, "1.5x"), (2.0, "2x")] {
            v.push(Size {
                name: Box::leak(format!("{}x{}@{dn}", px.0, px.1).into_boxed_str()),
                px,
                density: d,
                safe: SafeArea::default(),
                phone: false,
            });
        }
    }
    let cut = |left, right| SafeArea {
        left,
        top: 0,
        right,
        bottom: 0,
    };
    v.push(Size {
        name: "phone2400x1080",
        px: (2400, 1080),
        density: 2.75,
        safe: cut(110, 48),
        phone: true,
    });
    v.push(Size {
        name: "phone1600x720",
        px: (1600, 720),
        density: 2.0,
        safe: cut(72, 32),
        phone: true,
    });
    v
}

// Temp config / save dir and nested game folder; removed on drop.
struct Fixture {
    root: PathBuf,
    save: PathBuf,
    game: PathBuf,
}

impl Fixture {
    fn new() -> anyhow::Result<Self> {
        let root = std::env::temp_dir().join(format!("th08-uishots-{}", std::process::id()));
        let save = root.join("config");
        let game = root.join("Games/Touhou Project/TH08 - Imperishable Night");
        std::fs::create_dir_all(save.join("replay"))?;
        std::fs::create_dir_all(&game)?;
        // The real data (title art) when TH08_DIR has it, linked read-only; else an empty archive.
        match th08::settings::test_game_dir() {
            Some(d) => {
                for e in std::fs::read_dir(&d)? {
                    let p = e?.path();
                    if p.is_file() {
                        link(&p, &game.join(p.file_name().unwrap_or_default()))?;
                    }
                }
            }
            None => std::fs::write(game.join("TH08.DAT"), b"")?,
        }
        let specs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../specs/data/sessions");
        std::fs::copy(
            specs.join("hiscore_entry/end/score.dat"),
            save.join("score.dat"),
        )?;
        let rp = save.join("replay");
        std::fs::copy(
            specs.join("hiscore_entry/replays/th8_01.rpy"),
            rp.join("th8_01.rpy"),
        )?;
        std::fs::copy(
            specs.join("gameover_save/replays/th8_02.rpy"),
            rp.join("th8_tournament_final_round_2026_extra_long_replay_file_name.rpy"),
        )?;
        std::fs::write(rp.join("damaged.rpy"), b"T8RP not really a replay")?;
        stats(&save)?;
        Ok(Self { root, save, game })
    }

    fn settings(&self, lang: &str) -> th08::settings::Settings {
        let dirs = th08::settings::PlatformDirs {
            config: Some(self.save.clone()),
            save: Some(self.save.clone()),
            soundfonts: Vec::new(),
        };
        let mut s = th08::settings::Settings::from_args(dirs, Vec::<String>::new())
            .unwrap_or_else(|e| e.exit());
        let o = th08::settings::Origin::Cli;
        s.set_from(o, "game_dir", &format!("{}/", self.game.display()))
            .expect("game_dir");
        s.set_from(o, "language", lang).expect("language");
        s
    }
}

// A read-only stand-in for a game file: a symlink (a copy where there are none).
fn link(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    return std::os::unix::fs::symlink(from, to);
    #[cfg(not(unix))]
    return std::fs::copy(from, to).map(drop);
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// Seven runs: story / practice / spell, deaths, bombs, captured and failed cards with long names.
fn stats(save: &Path) -> anyhow::Result<()> {
    use th08::stats::{EndReason, Event, Mode};
    let path = th08::stats::path(save);
    let names = [
        "Hidden Bug \"Firefly Phenomenon of the Late Autumn Night\"",
        "Spirit Sign \"Fantasy Seal\"",
        "Lantern Sign \"Firefly Phenomenon\"",
    ];
    for run in 0u8..7 {
        let mode = [Mode::Story, Mode::Practice, Mode::Spell][usize::from(run % 3)];
        let mut ev = vec![Event::Start {
            mode,
            shot: run % 12,
            diff: run % 5,
            stage: 0,
        }];
        for stage in 0..=run.min(5) {
            ev.push(Event::Stage { stage });
            ev.push(Event::Death { stage });
            ev.push(Event::Bomb { stage });
            let card = u16::from(stage) * 13 + u16::from(run);
            ev.push(Event::Spell {
                card,
                stage,
                name: names[usize::from(card) % names.len()].to_string(),
            });
            ev.push(Event::SpellEnd {
                card,
                captured: run % 2 == 0,
            });
        }
        ev.push(Event::End {
            score: 123_456_780 * u64::from(run + 1),
            stage: run.min(5),
            frames: 60 * 60 * 7 * u64::from(run + 1),
            reason: [
                EndReason::Menu,
                EndReason::Ending,
                EndReason::Retry,
                EndReason::Quit,
            ][usize::from(run % 4)],
        });
        th08::stats::append(&path, &ev, 1_790_000_000 + u64::from(run) * 86_400)?;
    }
    Ok(())
}

#[derive(Clone)]
enum Page {
    Launcher(&'static str),
    Menu(&'static str, Shot),
}

impl Page {
    fn name(&self) -> String {
        match self {
            Page::Launcher(n) => format!("launcher_{n}"),
            Page::Menu(n, _) => format!("portmenu_{n}"),
        }
    }
}

const LAUNCHER: [&str; 9] = [
    "settings",
    "settings_end",
    "replays",
    "stats",
    "error",
    "waiting",
    "loading",
    "pointer_left",
    "keys_focus",
];

fn menu_pages(fx: &Fixture, phone: bool) -> Vec<Page> {
    let base = Shot {
        quit: true,
        touch: phone,
        density: 1.0,
        ..Shot::default()
    };
    let mut v = vec![
        Page::Menu("card", base.clone()),
        Page::Menu(
            "ask_retry",
            Shot {
                ask: Some(ShotAsk::Retry),
                ..base.clone()
            },
        ),
        Page::Menu(
            "ask_quit",
            Shot {
                ask: Some(ShotAsk::Quit),
                ..base.clone()
            },
        ),
        Page::Menu(
            "pointer_left",
            Shot {
                pointer_left: true,
                ..base.clone()
            },
        ),
    ];
    let n = th08::platform::portmenu::settings_tabs(&fx.settings("en"), phone);
    for t in 0..n {
        let top: &'static str = Box::leak(format!("settings{t}").into_boxed_str());
        let end: &'static str = Box::leak(format!("settings{t}_end").into_boxed_str());
        v.push(Page::Menu(
            top,
            Shot {
                settings: Some(t),
                ..base.clone()
            },
        ));
        v.push(Page::Menu(
            end,
            Shot {
                settings: Some(t),
                navs: vec![Nav::Down; 60],
                ..base.clone()
            },
        ));
    }
    v
}

fn launcher_model(
    fx: &Fixture,
    lang: &str,
    view: &str,
    art: Option<&image::RgbaImage>,
) -> (th08::launcher::Model, Probe) {
    let s = fx.settings(lang);
    let mut m = th08::launcher::Model::new(&s, th08::launcher::Folder::Native);
    if let Some(a) = art {
        m.shot_art(a);
    }
    let mut probe = Probe::None;
    match view {
        "settings_end" => m.shot_scroll_end(),
        "replays" => m.set_tab(Tab::Replays),
        "stats" => m.set_tab(Tab::Stats),
        "error" => {
            m.view = View::Error(
                "/home/someone/Games/Touhou Project/TH08 - Imperishable Night/th08.dat: \
                 No such file or directory (os error 2)"
                    .into(),
            );
        }
        "waiting" => m.view = View::Waiting,
        "loading" => m.view = View::Loading,
        "pointer_left" => probe = Probe::PointerLeft,
        "keys_focus" => probe = Probe::Keys,
        _ => {}
    }
    (m, probe)
}

#[expect(clippy::cast_sign_loss, reason = "the frame rect is positive")]
// The game picture under the port menu: a stage screenshot from the design references (never game data), scaled
// into the frame rect the menu leaves it; a plain gradient when the references are not checked out.
fn backdrop(stage: Option<&image::RgbaImage>, sz: &Size, shot: &Shot) -> image::RgbaImage {
    let (w, h) = sz.px;
    let Some(stage) = stage else {
        return image::RgbaImage::from_fn(w, h, |x, y| {
            let t = u64::from(x + y) * 255 / u64::from(w + h).max(1);
            let t = u8::try_from(t).unwrap_or(255);
            image::Rgba([40 + t / 4, 30, 70 - t / 8, 255])
        });
    };
    let shot = Shot {
        density: sz.density,
        ..shot.clone()
    };
    let [x, y, fw, fh] = th08::platform::portmenu::picture_frame(sz.safe, sz.px, &shot, false);
    let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 255]));
    let scaled = image::imageops::resize(
        stage,
        fw as u32,
        fh as u32,
        image::imageops::FilterType::Triangle,
    );
    image::imageops::overlay(&mut img, &scaled, x as i64, y as i64);
    img
}

// A design reference image `rel` in this checkout or, from a worktree, in the main checkout.
fn reference(rel: &str) -> Option<image::RgbaImage> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let main = std::process::Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(&repo)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|d| Path::new(d.trim()).parent().map(Path::to_path_buf));
    [Some(repo), main]
        .into_iter()
        .flatten()
        .find_map(|r| image::open(r.join(rel)).ok())
        .map(|i| i.to_rgba8())
}

// The title screen shot without what the game draws over title00.png on the right: the logo sprite (kanji column
// and the diagonal "Imperishable Night"), filled in from around it and blurred.
#[expect(
    clippy::cast_sign_loss,
    clippy::type_complexity,
    reason = "coordinates filtered to the image, colours in 0..=255"
)]
fn without_logo(img: &image::RgbaImage) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    let logo = |x: u32, y: u32| {
        let band = 330.0 - (x as f32 - 420.0) * 0.5;
        x >= 505 || (x >= 418 && (y as f32 - band).abs() < 34.0)
    };
    let mut out = img.clone();
    let mut known: Vec<bool> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| !logo(x, y))
        .collect();
    let at = |x: u32, y: u32| (y * w + x) as usize;
    let around = |x: u32, y: u32, r: i64| {
        let (x, y) = (i64::from(x), i64::from(y));
        (y - r..=y + r)
            .flat_map(move |b| (x - r..=x + r).map(move |a| (a, b)))
            .filter(|&(a, b)| a >= 0 && b >= 0 && a < i64::from(w) && b < i64::from(h))
            .map(|(a, b)| (a as u32, b as u32))
    };
    let average = |src: &image::RgbaImage, cells: Vec<(u32, u32)>| {
        let n = cells.len().max(1) as u32;
        let sum = cells.iter().fold([0u32; 3], |s, &(a, b)| {
            let p = src.get_pixel(a, b).0;
            [
                s[0] + u32::from(p[0]),
                s[1] + u32::from(p[1]),
                s[2] + u32::from(p[2]),
            ]
        });
        image::Rgba([
            (sum[0] / n) as u8,
            (sum[1] / n) as u8,
            (sum[2] / n) as u8,
            255,
        ])
    };
    let hole: Vec<(u32, u32)> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| logo(x, y))
        .collect();
    while known.iter().any(|k| !k) {
        let edge: Vec<(u32, u32, Vec<(u32, u32)>)> = hole
            .iter()
            .filter(|&&(x, y)| !known[at(x, y)])
            .filter_map(|&(x, y)| {
                let near: Vec<_> = around(x, y, 1).filter(|&(a, b)| known[at(a, b)]).collect();
                (!near.is_empty()).then_some((x, y, near))
            })
            .collect();
        for (x, y, near) in edge {
            let c = average(&out, near);
            out.put_pixel(x, y, c);
            known[at(x, y)] = true;
        }
    }
    for _ in 0..3 {
        let src = out.clone();
        for &(x, y) in &hole {
            out.put_pixel(x, y, average(&src, around(x, y, 10).collect()));
        }
    }
    // The fill's edge fades into the picture around it over 14 px.
    let src = out.clone();
    for y in 0..h {
        for x in 0..w {
            if logo(x, y) {
                continue;
            }
            let near = around(x, y, 14)
                .filter(|&(a, b)| logo(a, b))
                .map(|(a, b)| {
                    (i64::from(a) - i64::from(x)).pow(2) + (i64::from(b) - i64::from(y)).pow(2)
                })
                .min();
            let Some(d) = near else { continue };
            let t = ((d as f32).sqrt() / 14.0).min(1.0);
            let soft = average(&src, around(x, y, 8).collect()).0;
            let old = src.get_pixel(x, y).0;
            let mix = |i: usize| (f32::from(soft[i]) * (1.0 - t) + f32::from(old[i]) * t) as u8;
            out.put_pixel(x, y, image::Rgba([mix(0), mix(1), mix(2), 255]));
        }
    }
    out
}

struct Frame {
    file: String,
    lang: &'static str,
    surface: &'static str,
    issues: Vec<Issue>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();
    let fx = Fixture::new()?;
    if let Some(dir) = &args.fixture {
        copy_dir(&fx.save, dir)?;
        return Ok(());
    }
    let langs: Vec<&'static str> = ["en", "ja", "ru"]
        .into_iter()
        .filter(|l| args.lang.is_empty() || args.lang.iter().any(|x| x == l))
        .collect();
    let sizes: Vec<Size> = sizes()
        .into_iter()
        .filter(|s| {
            args.size
                .as_ref()
                .is_none_or(|f| s.name.contains(f.as_str()))
        })
        .collect();
    let gfx = match &args.out {
        Some(o) => {
            std::fs::create_dir_all(o)?;
            Some(pollster::block_on(th08_render::gfx::Gfx::headless())?)
        }
        None => None,
    };
    let stage = gfx
        .as_ref()
        .and_then(|_| reference("docs/briefs/uiref/05_th08_stage.png"));
    // Without game data the launcher's title art (title00.png) is stood in for by the title screen reference at the
    // game's 640×480, its logo sprite painted out.
    let title = th08::settings::test_game_dir()
        .is_none()
        .then(|| reference("docs/briefs/uiref/03_th08_title.png"))
        .flatten()
        .map(|i| image::imageops::resize(&i, 640, 480, image::imageops::FilterType::Triangle))
        .map(|i| without_logo(&i));
    if gfx.is_some() && stage.is_none() {
        eprintln!("uishots: no stage reference image, the port menu renders over a gradient");
    }
    let mut frames = Vec::new();
    for &lang in &langs {
        for sz in &sizes {
            let mut pages: Vec<Page> = Vec::new();
            if args.only.as_deref() != Some("portmenu") {
                pages.extend(LAUNCHER.iter().map(|v| Page::Launcher(v)));
            }
            if args.only.as_deref() != Some("launcher") {
                pages.extend(menu_pages(&fx, sz.phone));
            }
            for page in pages {
                let file = format!("{}_{}_{lang}.png", page.name(), sz.name);
                let s = fx.settings(lang);
                let (issues, surface) = match &page {
                    Page::Launcher(view) => {
                        let o = |probe| ShotOpts {
                            density: sz.density,
                            probe,
                            touch: sz.phone,
                        };
                        let (m, probe) = launcher_model(&fx, lang, view, title.as_ref());
                        let mut issues = th08::launcher::check_frame(m, sz.px, sz.safe, &o(probe));
                        if matches!(*view, "settings" | "replays" | "stats" | "error") {
                            issues.extend(th08::launcher::check_reach(
                                || launcher_model(&fx, lang, view, None).0,
                                sz.px,
                                sz.safe,
                                &o(Probe::None),
                            ));
                        }
                        if let (Some(g), Some(dir)) = (&gfx, &args.out) {
                            let (m, probe) = launcher_model(&fx, lang, view, title.as_ref());
                            th08::launcher::render_image(g, m, sz.px, sz.safe, &o(probe))
                                .save(dir.join(&file))?;
                        }
                        (issues, "launcher")
                    }
                    Page::Menu(_, shot) => {
                        let shot = Shot {
                            density: sz.density,
                            ..shot.clone()
                        };
                        let mut issues =
                            th08::platform::portmenu::check_frame(&s, sz.px, sz.safe, &shot);
                        if shot.settings.is_some() && shot.navs.is_empty() {
                            issues.extend(th08::platform::portmenu::check_reach(
                                &s, sz.px, sz.safe, &shot,
                            ));
                        }
                        if let (Some(g), Some(dir)) = (&gfx, &args.out) {
                            th08::platform::portmenu::render_image(
                                g,
                                &s,
                                &backdrop(stage.as_ref(), sz, &shot),
                                sz.safe,
                                &shot,
                            )
                            .save(dir.join(&file))?;
                        }
                        (issues, "portmenu")
                    }
                };
                frames.push(Frame {
                    file,
                    lang,
                    surface,
                    issues,
                });
            }
        }
    }
    let mut total = std::collections::BTreeMap::new();
    for f in &frames {
        for (i, is) in f.issues.iter().enumerate() {
            *total.entry(is.kind).or_insert(0usize) += 1;
            if args.all || i < 3 {
                println!("{}: {}: {}", f.file, is.kind.name(), is.what);
            }
        }
    }
    let bad = frames.iter().filter(|f| !f.issues.is_empty()).count();
    let counts: Vec<String> = Kind::ALL
        .iter()
        .map(|k| format!("{} {}", k.name(), total.get(k).copied().unwrap_or(0)))
        .collect();
    println!(
        "uishots: {} frames, {bad} with issues; {}",
        frames.len(),
        counts.join(", ")
    );
    if let Some(dir) = &args.out {
        index(dir, &frames)?;
        sheets(dir, &frames)?;
    }
    std::process::exit(i32::from(bad > 0));
}

fn copy_dir(from: &Path, to: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let p = e?.path();
        let dest = to.join(p.file_name().unwrap_or_default());
        if p.is_dir() {
            copy_dir(&p, &dest)?;
        } else {
            std::fs::copy(&p, &dest)?;
        }
    }
    Ok(())
}

fn index(dir: &Path, frames: &[Frame]) -> anyhow::Result<()> {
    let mut h = String::from(
        "<!doctype html><meta charset=utf-8><title>uishots</title><style>body{background:#111;color:#ddd;\
         font:13px sans-serif}figure{display:inline-block;margin:6px;vertical-align:top;width:330px}\
         img{width:320px;border:1px solid #444}.bad img{border:2px solid #e44}figcaption{word-break:break-all}\
         li{color:#f99}</style>",
    );
    for f in frames {
        let class = if f.issues.is_empty() {
            ""
        } else {
            " class=bad"
        };
        let _ = write!(
            h,
            "<figure{class}><a href=\"{0}\"><img loading=lazy src=\"{0}\"></a><figcaption>{0}<ul>",
            f.file
        );
        for i in &f.issues {
            let _ = write!(h, "<li>{}: {}</li>", i.kind.name(), html(&i.what));
        }
        h.push_str("</ul></figcaption></figure>\n");
    }
    std::fs::write(dir.join("index.html"), h)?;
    Ok(())
}

fn html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}

// One sheet per surface × language: thumbnails 320 px wide in rows of 8, a red frame on frames with issues.
fn sheets(dir: &Path, frames: &[Frame]) -> anyhow::Result<()> {
    const TW: u32 = 320;
    const COLS: u32 = 8;
    for surface in ["launcher", "portmenu"] {
        for lang in ["en", "ja", "ru"] {
            let set: Vec<&Frame> = frames
                .iter()
                .filter(|f| f.surface == surface && f.lang == lang)
                .collect();
            if set.is_empty() {
                continue;
            }
            let thumbs: Vec<(image::RgbaImage, bool)> = set
                .iter()
                .filter_map(|f| {
                    let img = image::open(dir.join(&f.file)).ok()?.to_rgba8();
                    let th = TW * img.height() / img.width().max(1);
                    let t = image::imageops::resize(
                        &img,
                        TW,
                        th,
                        image::imageops::FilterType::Triangle,
                    );
                    Some((t, !f.issues.is_empty()))
                })
                .collect();
            let rows = (thumbs.len() as u32).div_ceil(COLS);
            let cell_h = thumbs.iter().map(|(t, _)| t.height()).max().unwrap_or(0) + 8;
            let mut sheet = image::RgbaImage::from_pixel(
                COLS * (TW + 8),
                rows * cell_h,
                image::Rgba([17, 17, 17, 255]),
            );
            for (i, (t, bad)) in thumbs.iter().enumerate() {
                let (x, y) = (
                    (i as u32 % COLS) * (TW + 8) + 4,
                    (i as u32 / COLS) * cell_h + 4,
                );
                image::imageops::overlay(&mut sheet, t, i64::from(x), i64::from(y));
                if *bad {
                    for dx in 0..t.width() {
                        for d in 0..3 {
                            sheet.put_pixel(x + dx, y + d, image::Rgba([230, 60, 60, 255]));
                        }
                    }
                }
            }
            sheet.save(dir.join(format!("sheet_{surface}_{lang}.png")))?;
        }
    }
    Ok(())
}
