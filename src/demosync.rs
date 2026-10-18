// Demo sync check (answers_phase8 §2): plays demorpyN in the port and compares each gameplay frame with the
// ground-truth trace (specs/data/demoN_trace.txt).
use crate::game::Ctx;

pub const FIELDS: [&str; 22] = [
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
];
// Columns parsed as hex (input, rng, floats, boss, flags).
const HEX: [usize; 10] = [2, 3, 5, 6, 11, 12, 13, 20, 21, 2];

pub fn parse_trace(text: &str) -> Vec<[i64; 22]> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            if t.len() < 22 {
                return None;
            }
            let mut row = [0i64; 22];
            for (i, v) in t[..22].iter().enumerate() {
                row[i] = if HEX.contains(&i) {
                    i64::from_str_radix(v, 16).ok()?
                } else {
                    v.parse().ok()?
                };
            }
            Some(row)
        })
        .collect()
}

// The port's state in trace column order.
pub fn sample(ctx: &Ctx, rng_base: u32) -> [i64; 22] {
    let g = &ctx.game;
    let mut row = [0i64; 22];
    row[0] = g.frame_counter as i64;
    row[1] = g.demo_frame as i64;
    row[2] = ctx.input.game as i64;
    row[3] = ctx.anm.rng.state as i64;
    row[4] = ctx.anm.rng.counter.wrapping_sub(rng_base) as i64;
    row[18] = crate::gamemgr::rank() as i64;
    row[21] = g.flags as i64;
    let Some(gm) = crate::gamemgr::current() else {
        return row;
    };
    let gm = gm.borrow();
    let p = gm.player.borrow();
    let st = &p.stats;
    row[5] = p.pos[0].to_bits() as i64;
    row[6] = p.pos[1].to_bits() as i64;
    row[7] = p.state as i64;
    row[8] = st.score10;
    row[9] = st.graze_total as i64;
    row[10] = st.point_items2 as i64;
    row[11] = st.lives.to_bits() as i64;
    row[12] = st.bombs.to_bits() as i64;
    row[13] = st.power.to_bits() as i64;
    row[14] = st.gauge as i64;
    row[15] = st.time as i64;
    if let Some(b) = &gm.bullets {
        row[16] = b.borrow().live as i64;
    }
    if let Some(e) = &gm.enemies {
        let e = e.borrow();
        row[17] = e
            .enemies
            .iter()
            .filter(|x| x.flags & crate::enemy::A_ALIVE != 0)
            .count() as i64;
        row[19] = e.g.spell.is_some() as i64;
        row[20] = e
            .bosses
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_some())
            .fold(0, |a, (i, _)| a | 1 << i);
    }
    row
}

pub struct Report {
    pub frames: usize,
    pub first: Option<(usize, &'static str, i64, i64)>,
    pub per_field: Vec<(&'static str, usize, i64, i64)>,
    pub rng_before: (u16, u16),
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} frames; rng before frame 1: port {:04x}, trace {:04x}",
            self.frames, self.rng_before.0, self.rng_before.1
        )?;
        match self.first {
            None => writeln!(f, "in sync for every frame")?,
            Some((fr, name, want, got)) => writeln!(
                f,
                "first divergence: frame {fr}, {name}: trace {want:#x}, port {got:#x}"
            )?,
        }
        for (name, fr, want, got) in &self.per_field {
            writeln!(
                f,
                "  {name:8} first differs at frame {fr}: trace {want:#x}, port {got:#x}"
            )?;
        }
        Ok(())
    }
}

fn header_rng(text: &str) -> u16 {
    text.lines()
        .nth(1)
        .and_then(|l| l.split("rng state ").nth(1))
        .and_then(|t| u16::from_str_radix(&t[..4], 16).ok())
        .unwrap_or(0)
}

// Fields compared; flags only in the gameplay bits the port models (0..14).
const CHECKED: [usize; 21] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
];

// Runs demo `n` from a freshly loaded context and compares it with `trace`.
pub fn run(ctx: &mut Ctx, n: u8, trace: &str) -> anyhow::Result<Report> {
    let rows = parse_trace(trace);
    let r = crate::replay::Replay::parse(&ctx.archive.read(&format!("demorpy{n}.rpy"))?)?;
    ctx.game.replay_ff = false;
    anyhow::ensure!(
        crate::replay::start_playback(&mut ctx.game, r, None, Some(n)),
        "demo has no stage"
    );
    let mut rng_base = 0;
    let mut rng_before = 0;
    let mut out = Vec::with_capacity(rows.len());
    let mut guard = 0;
    while out.len() < rows.len() && guard < rows.len() + 2000 {
        guard += 1;
        let before = ctx.game.demo_frame;
        if out.is_empty() {
            rng_base = ctx.anm.rng.counter;
            rng_before = ctx.anm.rng.state;
        }
        if ctx.calc() != crate::task::Flow::Continue {
            break;
        }
        if ctx.game.demo_frame > before {
            out.push(sample(ctx, rng_base));
        }
        if ctx.game.flags & crate::gamemgr::F_DEMO == 0 {
            break;
        }
    }
    let mut first = None;
    let mut per_field: Vec<(&'static str, usize, i64, i64)> = Vec::new();
    for (i, (want, got)) in rows.iter().zip(&out).enumerate() {
        for &c in &CHECKED {
            if want[c] != got[c] && !per_field.iter().any(|p| p.0 == FIELDS[c]) {
                per_field.push((FIELDS[c], i + 1, want[c], got[c]));
                if first.is_none() {
                    first = Some((i + 1, FIELDS[c], want[c], got[c]));
                }
            }
        }
    }
    if out.len() < rows.len() && first.is_none() {
        first = Some((out.len() + 1, "frame", rows.len() as i64, out.len() as i64));
    }
    Ok(Report {
        frames: out.len(),
        first,
        per_field,
        rng_before: (rng_before, header_rng(trace)),
    })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    // Frames each demo must stay in sync for (raise as divergences get fixed).
    const MIN_SYNC: [usize; 4] = [260, 559, 449, 794];

    fn state(ctx: &crate::game::Ctx) -> [i64; 22] {
        let mut s = super::sample(ctx, 0);
        s[1] = 0;
        s[4] = 0;
        s
    }

    // Records a scripted Lunatic stage-1 run, saves/loads the .rpy and plays it back: every frame must match.
    #[test]
    fn record_playback() {
        let Some(dir) = std::env::var("TH08_DIR").ok() else {
            return;
        };
        let Ok(gfx) = pollster::block_on(crate::gfx::Gfx::headless()) else {
            return;
        };
        let src = crate::assets::Source::native(dir.clone().into());
        let mut ctx = pollster::block_on(crate::game::Ctx::load_with(src, gfx, false)).unwrap();
        let g = &mut ctx.game;
        (g.difficulty, g.shot, g.stage, g.flags, g.fresh_start) = (3, 1, 0, 0, true);
        g.request(crate::gamemgr::SCENE_GAME);
        let mut want = Vec::new();
        let mut k = 0u32;
        for _ in 0..4000 {
            if ctx.game.game_over != 0 {
                break;
            }
            k = k.wrapping_mul(1103515245).wrapping_add(12345);
            let dirs = [0, 0x10, 0x20, 0x40, 0x80, 0x50, 0x90];
            ctx.input.raw = 0x1 | (k >> 20 & 4) as u16 | dirs[(k >> 24) as usize % 7];
            let f = ctx.game.frame_counter;
            ctx.calc();
            if ctx.game.frame_counter > f {
                want.push(state(&ctx));
            }
        }
        let mut r = ctx.game.rpy.clone().unwrap();
        let mut rng = crate::rng::Rng::seeded(1);
        let r = crate::replay::Replay::parse(&r.encode(&mut rng)).unwrap();
        let gfx = pollster::block_on(crate::gfx::Gfx::headless()).unwrap();
        let src = crate::assets::Source::native(dir.into());
        let mut ctx = pollster::block_on(crate::game::Ctx::load_with(src, gfx, false)).unwrap();
        ctx.game.replay_ff = false;
        assert!(crate::replay::start_playback(&mut ctx.game, r, None, None));
        let mut got = 0;
        for _ in 0..want.len() + 200 {
            if got == want.len() {
                break;
            }
            let f = ctx.game.frame_counter;
            ctx.calc();
            if ctx.game.frame_counter > f {
                let s = state(&ctx);
                let w = want[got];
                if let Some(c) = (0..21).find(|&c| c != 2 && s[c] != w[c]) {
                    panic!(
                        "playback differs at frame {}: {} recorded {:#x}, played {:#x}",
                        got + 1,
                        super::FIELDS[c],
                        w[c],
                        s[c]
                    );
                }
                got += 1;
            }
        }
        assert!(
            want.len() > 1000 && got == want.len(),
            "{} recorded, {got} played",
            want.len()
        );
    }

    #[test]
    fn demo_sync() {
        let Some(dir) = std::env::var("TH08_DIR").ok() else {
            return;
        };
        let Ok(gfx) = pollster::block_on(crate::gfx::Gfx::headless()) else {
            eprintln!("no GPU adapter; demo sync skipped");
            return;
        };
        let mut gfx = Some(gfx);
        for n in 0..4u8 {
            let g = match gfx.take() {
                Some(g) => g,
                None => pollster::block_on(crate::gfx::Gfx::headless()).unwrap(),
            };
            let src = crate::assets::Source::native(dir.clone().into());
            let mut ctx = pollster::block_on(crate::game::Ctx::load_with(src, g, false)).unwrap();
            let trace = std::fs::read_to_string(format!(
                "{}/specs/data/demo{n}_trace.txt",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap();
            let rep = super::run(&mut ctx, n, &trace).unwrap();
            eprintln!("demo {n}: {rep}");
            let synced = rep.first.map_or(rep.frames, |f| f.0 - 1);
            assert!(
                synced >= MIN_SYNC[n as usize],
                "demo {n} lost sync early: {rep}"
            );
        }
    }
}
