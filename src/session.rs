//! Session replay (specs/data/drawlog/README.md "Sessions"): recorded input, boot seed and loader timing of the
//! original, the port's 36-column trace and its comparison with the recorded one.

use std::collections::VecDeque;

// Loader steps of the original, by poll index (0 = first main-menu frame).
#[derive(Clone, Debug, Default)]
pub struct Sched {
    // Port calc (1-based, = present) of the first main-menu frame: `release boot calc C` + the 2 Window_InitD3D presents.
    pub boot_calc: u32,
    pub menu: VecDeque<i64>,
    // (gauge setup, Loading_Wait, release) per stage start.
    pub stages: VecDeque<[i64; 3]>,
    // Poll index of the calc pass being run (negative during boot); advances on every chain restart (00 §3).
    pub poll: i64,
    // Recorded Input_Poll return per poll index (polls.txt `S`/`I` rows).
    pub input: std::rc::Rc<[u16]>,
    // Rows written inside this frame's calc (restarted passes, in-calc presents with ff −1), taken by `Session::run`.
    pub pass_rows: Vec<Row>,
}

impl Sched {
    // Raw input of poll `poll`; boot frames before the first menu frame never poll (0).
    pub fn raw(&self, poll: i64) -> u16 {
        usize::try_from(poll)
            .ok()
            .and_then(|p| self.input.get(p).copied())
            .unwrap_or(0)
    }
}

// FuncChain_RunCalc restart (00 §3, README "Trace"): the probe writes a row per pass, and the next pass polls again.
pub fn on_restart(ctx: &mut crate::game::Ctx) {
    let Some(poll) = ctx.game.sched.as_ref().map(|s| s.poll) else {
        return;
    };
    let r = sample(ctx, 0, poll, 0);
    if let Some(s) = &mut ctx.game.sched {
        s.pass_rows.push(r);
        s.poll += 1;
    }
}

// Device_ClearPresentTwice (16 §1.2): a present inside a calc gets its own trace row, ff = −1 (README "Trace").
pub fn on_extra_present(ctx: &mut crate::game::Ctx) {
    let Some(poll) = ctx.game.sched.as_ref().map(|s| s.poll) else {
        return;
    };
    let mut r = sample(ctx, 0, poll, 0);
    r[22] = -1;
    if let Some(s) = &mut ctx.game.sched {
        s.pass_rows.push(r);
    }
}

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

pub type Row = [i64; 36];

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

pub fn format_row(r: &Row) -> String {
    let mut s = String::new();
    for (i, v) in r.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        match HEX.iter().find(|h| h.0 == i) {
            Some(&(_, w)) => s.push_str(&format!("{:0w$x}", *v as u32)),
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
    for l in text.lines() {
        let t: Vec<&str> = l.split_whitespace().collect();
        let num = |i: usize| t.get(i).and_then(|v| v.parse::<i64>().ok());
        match t.as_slice() {
            ["seed", h, ..] => seed = u32::from_str_radix(h, 16).ok(),
            ["release", "boot", "calc", ..] => s.boot_calc = num(3).unwrap_or(12) as u32 + 2,
            ["release", "menu", "poll", ..] => s.menu.extend(num(3)),
            ["loader", "gauge", "poll", ..] => gauge = num(3),
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

// The port's state in trace column order, after the calc of present `present`; `calc` (col 24) is the caller's.
pub fn sample(ctx: &mut crate::game::Ctx, present: i64, poll: i64, scene_frame: i64) -> Row {
    let mut r = [0i64; 36];
    let d = crate::demosync::sample(ctx, 0);
    r[..22].copy_from_slice(&d);
    let (menu, cursor) = ctx.game.trace_menu.take().unwrap_or((-1, -1));
    let result = ctx.game.trace_result.take().unwrap_or(-1);
    let g2 = &ctx.game;
    r[22] = 0;
    r[23] = present;
    r[25] = poll + 1;
    r[26] = g2.scene as i64;
    r[27] = scene_frame;
    r[28] = menu as i64;
    r[29] = cursor as i64;
    r[30] = result as i64;
    r[31] = g2.paused as i64;
    r[32] = ctx.input.held as i64;
    r[33] = g2.stage as i64;
    r[34] = g2.loading as i64;
    r[35] = if g2.flags & crate::gamemgr::F_DEMO != 0 {
        g2.demo as i64
    } else {
        -1
    };
    r
}

// Columns not compared.
pub const SKIP: [usize; 0] = [];
// Gameplay columns compared only in scene 2 (the original keeps stale values of freed objects elsewhere).
pub const GAMEPLAY: std::ops::RangeInclusive<usize> = 5..=20;

// Column 26 is the recorded scene.
pub fn compared(c: usize, rec: &Row) -> bool {
    !SKIP.contains(&c) && (rec[26] == 2 || !GAMEPLAY.contains(&c))
}

#[derive(Default, Debug)]
pub struct Comparison {
    pub rows: usize,
    pub compared: usize,
    pub skipped_loading: usize,
    pub leading: usize,
    // (row, columns (name, recorded, port)) of the first differing row.
    pub first: Option<(usize, Vec<(&'static str, i64, i64)>)>,
    // (scene, visit, first row, rows, leading identical rows).
    pub visits: Vec<(i64, usize, usize, usize, usize)>,
    pub per_column: Vec<usize>,
}

// Recorded and port rows from their first main-menu row; rows with recorded loading ≠ 0 are skipped.
pub fn compare(rec: &[Row], port: &[Row]) -> Comparison {
    let mut c = Comparison {
        rows: rec.len().min(port.len()),
        per_column: vec![0; 36],
        ..Default::default()
    };
    let mut leading_open = true;
    let mut visit_open = false;
    let mut counts = std::collections::HashMap::new();
    for (i, (r, p)) in rec.iter().zip(port).enumerate() {
        if i == 0 || r[26] != rec[i - 1][26] {
            let n = counts.entry(r[26]).or_insert(0);
            *n += 1;
            c.visits.push((r[26], *n, i + 1, 0, 0));
            visit_open = true;
        }
        let v = c.visits.last_mut().unwrap();
        v.3 += 1;
        if r[34] != 0 {
            c.skipped_loading += 1;
            if visit_open {
                v.4 += 1;
            }
            if leading_open {
                c.leading += 1;
            }
            continue;
        }
        c.compared += 1;
        let diff: Vec<usize> = (0..36)
            .filter(|&k| compared(k, r) && r[k] != p[k])
            .collect();
        for &k in &diff {
            c.per_column[k] += 1;
        }
        if diff.is_empty() {
            if visit_open {
                v.4 += 1;
            }
            if leading_open {
                c.leading += 1;
            }
        } else {
            visit_open = false;
            if leading_open {
                leading_open = false;
                c.first = Some((
                    i + 1,
                    diff.iter().map(|&k| (COLUMNS[k], r[k], p[k])).collect(),
                ));
            }
        }
    }
    c
}

impl std::fmt::Display for Comparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} rows (from the first main-menu row), {} compared, {} loading rows skipped",
            self.rows, self.compared, self.skipped_loading
        )?;
        writeln!(f, "leading identical rows: {}", self.leading)?;
        match &self.first {
            None => writeln!(f, "no differing row")?,
            Some((row, cols)) => {
                write!(f, "first differing row {row} (menu frame {row}):")?;
                for (n, a, b) in cols {
                    write!(f, " {n} {a:#x}/{b:#x}")?;
                }
                writeln!(f, "  (recorded/port)")?;
            }
        }
        writeln!(
            f,
            "per scene visit (scene #visit: first row, rows, leading identical):"
        )?;
        for (s, n, first, rows, lead) in &self.visits {
            writeln!(
                f,
                "  scene {s} #{n}: row {first}, {rows} rows, {lead} identical"
            )?;
        }
        write!(f, "differing rows per column:")?;
        for (k, n) in self.per_column.iter().enumerate() {
            if *n > 0 {
                write!(f, " {}={n}", COLUMNS[k])?;
            }
        }
        writeln!(
            f,
            "\nnot compared: gameplay columns {}–{} outside scene 2",
            COLUMNS[*GAMEPLAY.start()],
            COLUMNS[*GAMEPLAY.end()]
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::path::{Path, PathBuf};

    pub struct Session {
        pub dir: PathBuf,
        pub input: Vec<u16>,
        pub seed: u32,
        pub sched: Sched,
        pub noclip: bool,
    }

    impl Session {
        pub fn load(dir: &Path) -> anyhow::Result<Self> {
            let events = std::fs::read_to_string(dir.join("events.txt"))?;
            let (seed, sched) = parse_events(&events)?;
            // Raw input per poll (README: polls.txt, not the per-present input.txt, which loses restart polls).
            let input = match std::fs::read_to_string(dir.join("polls.txt")) {
                Ok(t) => parse_polls(&t).ok_or_else(|| anyhow::anyhow!("bad polls.txt"))?,
                Err(_) => {
                    let t = std::fs::read_to_string(dir.join("input.txt"))?;
                    crate::input::parse_script(&t)
                        .ok_or_else(|| anyhow::anyhow!("bad input.txt"))?
                }
            };
            let noclip = std::fs::read_to_string(dir.join("meta.txt"))
                .is_ok_and(|m| m.lines().any(|l| l.trim() == "noclip: 1"));
            Ok(Self {
                dir: dir.to_path_buf(),
                input,
                seed,
                sched,
                noclip,
            })
        }

        // Recorded trace rows from the first main-menu row on.
        pub fn recorded(&self) -> anyhow::Result<Vec<Row>> {
            let text = crate::xz::read_to_string(&self.dir.join("trace.txt.xz"))?;
            let rows = parse_trace(&text);
            Ok(menu_rows(rows))
        }

        // Start state: private save dir with start/score.dat, th08.cfg, replays; boot seed; loader schedule.
        pub fn start(&self, ctx: &mut crate::game::Ctx) -> anyhow::Result<()> {
            let save = crate::settings::temp_dir().join(format!(
                "th08_session_{}_{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&save);
            std::fs::create_dir_all(save.join("replay"))?;
            let start = self.dir.join("start");
            let score = std::fs::read(start.join("score.dat")).ok();
            ctx.game.load_score(score.as_deref());
            if let Some(b) = &score {
                std::fs::write(save.join("score.dat"), b)?;
            }
            if let Some(c) = std::fs::read(start.join("th08.cfg"))
                .ok()
                .and_then(|b| crate::config::Config::from_bytes(&b))
            {
                ctx.game.cfg = c;
            }
            for e in std::fs::read_dir(start.join("replay"))
                .into_iter()
                .flatten()
                .flatten()
            {
                std::fs::copy(e.path(), save.join("replay").join(e.file_name()))?;
            }
            crate::scoredat::set_save_dir(Some(save));
            ctx.anm.rng = crate::rng::Rng::seeded(self.seed as u16);
            ctx.game.sched = Some(Sched {
                input: self.input.as_slice().into(),
                ..self.sched.clone()
            });
            ctx.game.noclip = self.noclip;
            Ok(())
        }

        // Runs the session up to poll row `to` (1-based); `row` gets every trace row (boot rows too, one per calc
        // pass) and returns false to stop; `draw` gets the draw-log text of every present.
        pub fn run(
            &self,
            ctx: &mut crate::game::Ctx,
            to: usize,
            mut row: impl FnMut(&Row) -> bool,
            mut draw: Option<&mut dyn FnMut(String)>,
        ) {
            if draw.is_some() {
                ctx.log_to_memory();
            }
            // Debug `TH08_SESSION_RNG=<row>`: RNG call sites of that menu frame.
            let rng_row = ctx.settings.session_rng;
            let boot = self.sched.boot_calc as i64;
            let (mut scene, mut scene_frame) = (i32::MIN, 0i64);
            let to = to.min(self.input.len()) as i64;
            // Poll index of the next frame's first pass (restarted passes take one each, 00 §3).
            let mut next_poll = 0i64;
            // Presents made inside a calc (Device_ClearPresentTwice) so far; `n` counts Frame_Runs.
            let mut extra = 0i64;
            for n in 1i64.. {
                let poll = if n < boot { n - boot } else { next_poll };
                if poll >= to {
                    break;
                }
                if let Some(s) = &mut ctx.game.sched {
                    s.poll = poll;
                    s.pass_rows.clear();
                }
                // Info_Show polls outside the chain; Supervisor_Calc takes its own word from the schedule.
                ctx.input.raw = ctx.game.sched.as_ref().map_or(0, |s| s.raw(poll));
                let tr = rng_row == Some(poll + 1);
                crate::rng::TRACE.set(tr);
                let flow = match &mut draw {
                    Some(out) => {
                        let Some((flow, frames)) = crate::drawlog::step(ctx) else {
                            break;
                        };
                        frames.into_iter().for_each(|f| out(f));
                        flow
                    }
                    None => ctx.run_frame(),
                };
                crate::rng::TRACE.set(false);
                if tr {
                    print_rng_calls(poll + 1);
                }
                let (last, rows) = match &mut ctx.game.sched {
                    Some(s) => (s.poll, std::mem::take(&mut s.pass_rows)),
                    None => (poll, Vec::new()),
                };
                if n >= boot {
                    next_poll = last + 1;
                }
                // Rows written inside this calc (README "Trace"): pass rows ff = 1…k before the final row ff = k,
                // ff = −1 rows for in-calc presents; `calc` (Frame_Runs) increments only on the final row; present and
                // scene_frame count presents, the in-calc ones included.
                let base = n + extra;
                let sf_base = scene_frame + 1;
                let k_in = rows.iter().filter(|r| r[22] < 0).count() as i64;
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
                    if !row(&r) {
                        stop = true;
                        break;
                    }
                }
                extra += k_in;
                if ctx.game.scene != scene {
                    (scene, scene_frame) = (ctx.game.scene, 1);
                } else {
                    scene_frame = sf_base + k_in;
                }
                let mut r = sample(ctx, present, last.max(-1), scene_frame);
                r[22] = passes;
                r[24] = n - 2;
                // The exit save follows the quitting frame's sample (15 §2.2).
                if flow == crate::task::Flow::Quit {
                    ctx.exit_save();
                }
                if stop || !row(&r) || flow != crate::task::Flow::Continue {
                    break;
                }
            }
        }
    }

    fn print_rng_calls(row: i64) {
        println!("rng calls of row {row}:");
        let mut groups: Vec<(String, usize)> = Vec::new();
        for c in crate::rng::CALLS.with_borrow_mut(std::mem::take) {
            let k = c
                .lines()
                .filter(|l| {
                    l.contains("th08::") && !l.contains("th08::rng::") && !l.contains("closure")
                })
                .map(|l| {
                    let l = l.trim();
                    l.split_once(": ").map_or(l, |x| x.1).replace("th08::", "")
                })
                .take(5)
                .collect::<Vec<_>>()
                .join(" <- ");
            match groups.last_mut() {
                Some((g, n)) if *g == k => *n += 1,
                _ => groups.push((k, 1)),
            }
        }
        for (g, n) in groups {
            println!("  {n:4} x {g}");
        }
    }

    // Rows from the first main-menu row (scene 1) on.
    pub fn menu_rows(rows: Vec<Row>) -> Vec<Row> {
        let k = rows.iter().position(|r| r[26] == 1).unwrap_or(rows.len());
        rows[k..].to_vec()
    }

    pub fn data_dir(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("specs/data/sessions")
            .join(name)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    // Leading identical trace rows per session (raise as divergences get fixed).
    const MIN_LEADING: [(&str, usize); 3] = [
        ("selftest1", 1922),
        ("golden", 25634),
        ("fullrun_noclip", 124707),
    ];

    #[test]
    fn events() {
        let (seed, s) = parse_events(
            "seed 001afaed present 3 poll 0\nrelease boot calc 12 present 14\nrelease menu poll 32 present 46\n\
             loader gauge poll 10918 present 1\nloader lwait poll 10935 present 1\nrelease stage poll 10968 present 1\n",
        )
        .unwrap();
        assert_eq!((seed, s.boot_calc), (0x1afaed, 14));
        assert_eq!(s.menu, [32]);
        assert_eq!(s.stages, [[10918, 10935, 10968]]);
        let polls = parse_polls(
            "# poll present scene src mask\n0 17 0 S 0000\n1 18 1 S 0020\n2 18 2 S 0001\n",
        );
        assert_eq!(polls.as_deref(), Some(&[0, 0x20, 1][..]));
        assert!(parse_polls("0 17 0 S 0000\n2 18 1 S 0020\n").is_none());
        let r = parse_trace(
            "0 0 0001 4037 0 3f800000 00000000 0 0 0 0 00000000 00000000 00000000 0 0 0 0 0 0 0 10000 0 15 13 1 1 1 0 0 -1 0 0001 0 0 -1\n",
        );
        assert_eq!(
            format_row(&r[0]),
            "0 0 0001 4037 0 3f800000 00000000 0 0 0 0 00000000 00000000 00000000 0 0 0 0 0 0 0 10000 0 15 13 1 1 1 0 0 -1 0 0001 0 0 -1"
        );
    }

    #[test]
    fn session_sync() {
        let Some(dir) = crate::settings::test_game_dir() else {
            return;
        };
        for (name, min) in MIN_LEADING {
            let Ok(gfx) = pollster::block_on(crate::gfx::Gfx::headless()) else {
                eprintln!("no GPU adapter; session sync skipped");
                return;
            };
            let src = crate::assets::Source::native(dir.clone().into());
            let mut ctx = pollster::block_on(crate::game::Ctx::load_with(src, gfx, false)).unwrap();
            let s = Session::load(&data_dir(name)).unwrap();
            let Ok(rec) = s.recorded() else {
                eprintln!("{name}: reference unreadable, skipped");
                continue;
            };
            s.start(&mut ctx).unwrap();
            let mut port = Vec::new();
            // Stops a little after the first differing row.
            let mut stop = usize::MAX;
            s.run(
                &mut ctx,
                rec.len(),
                |r| {
                    if r[26] == 1 || !port.is_empty() {
                        port.push(*r);
                    }
                    let k = port.len();
                    if k > 0 && k <= rec.len() {
                        let a = &rec[k - 1];
                        if stop == usize::MAX
                            && a[34] == 0
                            && (0..36).any(|c| compared(c, a) && a[c] != r[c])
                        {
                            stop = k;
                        }
                    }
                    k < stop.saturating_add(1)
                },
                None,
            );
            let c = compare(&rec, &port);
            eprintln!("{name}: {c}");
            assert!(
                c.leading >= min,
                "{name} regressed: {} leading identical rows (min {min})",
                c.leading
            );
        }
    }

    // Smoke test: the whole 124 705-frame noclip run (stages 1–6, ending) without panics or hangs (~12 s release).
    #[test]
    fn fullrun_smoke() {
        let Some(dir) = crate::settings::test_game_dir() else {
            return;
        };
        let Ok(gfx) = pollster::block_on(crate::gfx::Gfx::headless()) else {
            eprintln!("no GPU adapter; fullrun smoke skipped");
            return;
        };
        let src = crate::assets::Source::native(dir.into());
        let mut ctx = pollster::block_on(crate::game::Ctx::load_with(src, gfx, false)).unwrap();
        let s = Session::load(&data_dir("fullrun_noclip")).unwrap();
        let Ok(rec) = s.recorded() else {
            eprintln!("fullrun trace unreadable: smoke skipped");
            return;
        };
        let events = std::fs::read_to_string(s.dir.join("events.txt")).unwrap();
        let hits: Vec<(i64, u32)> = events
            .lines()
            .filter_map(|l| {
                let w: Vec<&str> = l.split_whitespace().collect();
                (w.first() == Some(&"noclip_hit"))
                    .then(|| (w[3].parse().unwrap(), w[1].parse().unwrap()))
            })
            .collect();
        s.start(&mut ctx).unwrap();
        let t0 = std::time::Instant::now();
        let (mut port, mut port_hits) = (Vec::new(), Vec::new());
        // The recorded count runs over the whole game; `Player::hits` restarts with each stage's GameMgr.
        let (mut hit_base, mut hit_prev) = (0, 0);
        s.run(
            &mut ctx,
            rec.len(),
            |r| {
                if r[26] == 1 || !port.is_empty() {
                    port.push(*r);
                    let h =
                        crate::gamemgr::current().map_or(0, |g| g.borrow().player.borrow().hits);
                    if h < hit_prev {
                        hit_base += hit_prev;
                    }
                    hit_prev = h;
                    port_hits.push((r[24], hit_base + h));
                }
                true
            },
            None,
        );
        let starts = |rows: &[Row]| {
            let mut v: Vec<(i64, usize)> = Vec::new();
            for (k, r) in rows.iter().enumerate() {
                if r[26] == 2 && r[34] == 0 && v.last().is_none_or(|l| l.0 != r[33]) {
                    v.push((r[33], k + 1));
                }
            }
            v
        };
        let (rs, ps) = (starts(&rec), starts(&port));
        eprintln!(
            "fullrun: {} rows in {:.1} s; stage starts recorded {rs:?} port {ps:?}",
            port.len(),
            t0.elapsed().as_secs_f64()
        );
        assert!(
            port.len() + 1 >= rec.len(),
            "the port stopped at row {}",
            port.len()
        );
        // Past the leading rows the recorded input drives a desynced game, so how far it gets is luck (impl29: 5 stages).
        assert!(ps.len() >= 5, "the port reached only stages {ps:?}");
        let c = compare(&rec, &port);
        let synced_calc = port[c.leading.saturating_sub(1)][24];
        for &(calc, n) in hits.iter().filter(|h| h.0 <= synced_calc) {
            let got = port_hits.iter().find(|h| h.0 == calc).map(|h| h.1);
            assert_eq!(got, Some(n), "noclip hit {n} at calc {calc}");
        }
    }
}
