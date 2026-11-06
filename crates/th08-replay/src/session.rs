//! Recorded sessions of the original: polls, events, meta and start files, the trace row format and the frame loop.

use std::path::{Path, PathBuf};
use th08_core::game::Ctx;
use th08_core::task::Flow;
use th08_core::trace::{Row, Sched, sample};

// `polls.txt`: `poll present scene src mask` per Input_Poll call; the mask by poll index.
pub fn parse_polls(text: &str) -> Option<Vec<u16>> {
    let mut v = Vec::new();
    for l in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let t: Vec<&str> = l.split_whitespace().collect();
        let (poll, mask) = (t.first()?.parse::<usize>().ok()?, t.get(4)?);
        if poll != v.len() {
            return None;
        }
        v.push(u16::from_str_radix(mask, 16).ok()?);
    }
    Some(v)
}

pub const COLUMNS: [&str; 36] = [
    "frame",
    "demo_frame",
    "input",
    "rng",
    "rng_ctr",
    "px",
    "py",
    "pstate",
    "score",
    "graze",
    "points",
    "lives",
    "bombs",
    "power",
    "gauge",
    "time",
    "bullets",
    "enemies",
    "rank",
    "spell",
    "boss",
    "flags",
    "ff",
    "present",
    "calc",
    "poll",
    "scene",
    "scene_frame",
    "menu",
    "cursor",
    "result",
    "pause",
    "rawinput",
    "stage",
    "loading",
    "demo",
];

// Hex columns and their print width.
const HEX: [(usize, usize); 10] = [
    (2, 4),
    (3, 4),
    (5, 8),
    (6, 8),
    (11, 8),
    (12, 8),
    (13, 8),
    (20, 0),
    (21, 0),
    (32, 4),
];

pub fn parse_trace(text: &str) -> Vec<Row> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            if t.len() < 36 {
                return None;
            }
            let mut row = [0i64; 36];
            for (i, v) in t[..36].iter().enumerate() {
                row[i] = if HEX.iter().any(|h| h.0 == i) {
                    i64::from_str_radix(v, 16).ok()?
                } else {
                    v.parse().ok()?
                };
            }
            Some(row)
        })
        .collect()
}

// Low 32 bits of a trace value (hex columns, schedule numbers): exactly `as u32`.
#[expect(
    clippy::cast_sign_loss,
    reason = "two's-complement low word, `as u32` kept"
)]
const fn lo32(v: i64) -> u32 {
    v as u32
}

pub fn format_row(r: &Row) -> String {
    let mut s = String::new();
    for (i, v) in r.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        match HEX.iter().find(|h| h.0 == i) {
            Some(&(_, w)) => s.push_str(&format!("{:0w$x}", lo32(*v))),
            None => s.push_str(&v.to_string()),
        }
    }
    s
}

pub fn trace_header() -> String {
    format!("# {}\n", COLUMNS.join(" "))
}

// `events.txt`: (boot seed, schedule).
pub fn parse_events(text: &str) -> anyhow::Result<(u32, Sched)> {
    let mut seed = None;
    let mut s = Sched {
        poll: -1,
        ..Default::default()
    };
    let (mut gauge, mut wait) = (None, None);
    let mut first_gauge = true;
    for l in text.lines() {
        let t: Vec<&str> = l.split_whitespace().collect();
        let num = |i: usize| t.get(i).and_then(|v| v.parse::<i64>().ok());
        match t.as_slice() {
            ["seed", h, ..] => seed = u32::from_str_radix(h, 16).ok(),
            ["release", "boot", "calc", ..] => s.boot_calc = lo32(num(3).unwrap_or(12)) + 2,
            ["release", "menu", "poll", ..] => s.menu.extend(num(3)),
            ["loader", "gauge", "poll", ..] => {
                gauge = num(3);
                let calc = t
                    .iter()
                    .position(|w| *w == "asciicalc")
                    .and_then(|i| num(i + 1));
                if std::mem::take(&mut first_gauge)
                    && let (Some(b), Some(c)) = (gauge, calc)
                    && c > b
                {
                    s.ascii_offset = lo32(c - b);
                }
            }
            ["loader", "lwait", "poll", ..] => wait = num(3),
            ["release", "stage", "poll", ..] => {
                let p = num(3).unwrap_or(0);
                s.stages
                    .push_back([gauge.take().unwrap_or(p), wait.take().unwrap_or(p), p]);
            }
            _ => {}
        }
    }
    let seed = seed.ok_or_else(|| anyhow::anyhow!("events.txt: no seed line"))?;
    anyhow::ensure!(s.boot_calc > 0, "events.txt: no `release boot calc` line");
    Ok((seed, s))
}

// "YYYY-MM-DDTHH:MM:SS[+zone]" (meta.txt `recorded:`): the recorder's local wall-clock fields.
pub fn parse_local_time(s: &str) -> Option<th08_core::host::LocalTime> {
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse().ok();
    Some(th08_core::host::LocalTime {
        year: u32::cast_signed(n(0..4)?),
        month: n(5..7)?,
        day: n(8..10)?,
        hour: n(11..13)?,
        minute: n(14..16)?,
        second: n(17..19)?,
    })
}

// Rows from the first main-menu row (scene 1) on.
pub fn menu_rows(mut rows: Vec<Row>) -> Vec<Row> {
    let k = rows.iter().position(|r| r[26] == 1).unwrap_or(rows.len());
    rows.split_off(k)
}

// A count or index as a trace column; exactly `as i64`.
#[expect(clippy::cast_possible_wrap, reason = "counts and indices < 2^63")]
const fn int(n: usize) -> i64 {
    n as i64
}

pub struct Recording {
    pub dir: PathBuf,
    pub input: Vec<u16>,
    pub seed: u32,
    pub sched: Sched,
    pub noclip: bool,
    // meta.txt `recorded:` local date and time, the session's `localtime` answer (replay / score dates).
    pub recorded_at: Option<th08_core::host::LocalTime>,
    // meta.txt `fingerprint:` logic-content fingerprint; absent = vanilla (empty).
    pub fingerprint: String,
}

impl Recording {
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let events = std::fs::read_to_string(dir.join("events.txt"))?;
        let (seed, sched) = parse_events(&events)?;
        // Raw input per poll (polls.txt, not the per-present input.txt, which loses restart polls).
        let input = if let Ok(t) = std::fs::read_to_string(dir.join("polls.txt")) {
            parse_polls(&t).ok_or_else(|| anyhow::anyhow!("bad polls.txt"))?
        } else {
            let t = std::fs::read_to_string(dir.join("input.txt"))?;
            th08_core::input::parse_script(&t).ok_or_else(|| anyhow::anyhow!("bad input.txt"))?
        };
        let meta = std::fs::read_to_string(dir.join("meta.txt")).unwrap_or_default();
        let noclip = meta.lines().any(|l| l.trim() == "noclip: 1");
        let recorded_at = meta
            .lines()
            .find_map(|l| l.strip_prefix("recorded:"))
            .and_then(|v| parse_local_time(v.trim()));
        let fingerprint = meta
            .lines()
            .find_map(|l| l.strip_prefix("fingerprint:"))
            .map_or_else(String::new, |v| v.trim().to_string());
        Ok(Self {
            dir: dir.to_path_buf(),
            input,
            seed,
            sched,
            noclip,
            recorded_at,
            fingerprint,
        })
    }

    // Start state of `ctx`: start/score.dat and th08.cfg, boot seed, loader schedule. Returns the save files
    // (score.dat, replay/…) the caller puts into its host storage, together with the recorded polls and time.
    pub fn start(&self, ctx: &mut Ctx) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
        // A session replays only on the logic content it was recorded with.
        let fp = ctx.archive.fingerprint().to_string();
        anyhow::ensure!(
            fp == self.fingerprint,
            "session fingerprint '{}' ≠ content fingerprint '{fp}'",
            self.fingerprint
        );
        let start = self.dir.join("start");
        let mut files = Vec::new();
        let score = std::fs::read(start.join("score.dat")).ok();
        let now = ctx.host.ticks_ms();
        ctx.w.game.load_score(score.as_deref(), now);
        if let Some(b) = score {
            files.push(("score.dat".to_string(), b));
        }
        if let Some(c) = std::fs::read(start.join("th08.cfg"))
            .ok()
            .and_then(|b| th08_core::config::Config::from_bytes(&b))
        {
            ctx.w.game.cfg = c;
        }
        let mut replays: Vec<_> = std::fs::read_dir(start.join("replay"))
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        replays.sort_by_key(std::fs::DirEntry::file_name);
        for e in replays {
            let name = e.file_name().to_string_lossy().into_owned();
            files.push((format!("replay/{name}"), std::fs::read(e.path())?));
        }
        // The session's saves (replays the menus list again).
        ctx.w.game.persist = true;
        ctx.w.rng = th08_core::rng::Rng::seeded(self.seed as u16);
        ctx.w.overlays.set_frame_counter(self.sched.ascii_offset);
        ctx.w.game.sched = Some(self.sched.clone());
        ctx.w.game.noclip = self.noclip;
        Ok(files)
    }

    // Runs the session up to poll row `to` (1-based). `frame(ctx, poll)` runs one Frame_Run whose first Input_Poll
    // reads recorded poll `poll` (None stops); `row` gets every trace row (boot rows too, one per calc pass) and
    // returns false to stop.
    pub fn run(
        &self,
        ctx: &mut Ctx,
        to: usize,
        mut frame: impl FnMut(&mut Ctx, i64) -> Option<Flow>,
        mut row: impl FnMut(&Row, &Ctx) -> bool,
    ) {
        let boot = i64::from(self.sched.boot_calc);
        let (mut scene, mut scene_frame) = (i32::MIN, 0i64);
        let to = int(to.min(self.input.len()));
        // Poll index of the next frame's first pass (restarted passes take one each).
        let mut next_poll = 0i64;
        // Presents made inside a calc (Device_ClearPresentTwice) so far; `n` counts Frame_Runs.
        let mut extra = 0i64;
        for n in 1i64.. {
            let poll = if n < boot { n - boot } else { next_poll };
            if poll >= to {
                break;
            }
            if let Some(s) = &mut ctx.w.game.sched {
                s.poll = poll;
                s.pass_rows.clear();
            }
            let Some(flow) = frame(ctx, poll) else {
                break;
            };
            let (last, rows) = match &mut ctx.w.game.sched {
                Some(s) => (s.poll, std::mem::take(&mut s.pass_rows)),
                None => (poll, Vec::new()),
            };
            if n >= boot {
                next_poll = last + 1;
            }
            // Rows written inside this calc: pass rows ff = 1…k before the final row ff = k,
            // ff = −1 rows for in-calc presents; `calc` (Frame_Runs) increments only on the final row; present and
            // scene_frame count presents, the in-calc ones included.
            let base = n + extra;
            let sf_base = scene_frame + 1;
            let k_in = int(rows.iter().filter(|r| r[22] < 0).count());
            let present = base + k_in;
            let (mut passes, mut ins, mut stop) = (0, 0, false);
            for mut r in rows {
                if r[22] < 0 {
                    ins += 1;
                    r[23] = base + ins;
                    r[27] = sf_base + ins;
                } else {
                    passes += 1;
                    r[22] = passes;
                    r[23] = present;
                    r[27] = sf_base + k_in;
                }
                r[24] = n - 3;
                if !row(&r, ctx) {
                    stop = true;
                    break;
                }
            }
            extra += k_in;
            if ctx.w.game.scene as i32 == scene {
                scene_frame = sf_base + k_in;
            } else {
                (scene, scene_frame) = (ctx.w.game.scene as i32, 1);
            }
            let mut r = sample(ctx, present, last.max(-1), scene_frame);
            r[22] = passes;
            r[24] = n - 2;
            // The exit save follows the quitting frame's sample.
            if flow == Flow::Quit {
                ctx.exit_save();
            }
            if stop || !row(&r, ctx) || flow != Flow::Continue {
                break;
            }
        }
    }
}
