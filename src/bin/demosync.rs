//! Demo sync check: `demosync <0-3> [from-to] [rng-frame]` prints the report, trace/port rows around a range and,
//! with rng-frame, the call sites of every RNG draw made in that gameplay frame.

use th08::demosync::{FIELDS, parse_trace, sample};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let n: u8 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
    let range = args.get(2).and_then(|r| {
        let (a, b) = r.split_once('-')?;
        Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?))
    });
    let rng_frame: Option<i32> = args.get(3).and_then(|v| v.parse().ok());
    let dir = th08::settings::test_game_dir().ok_or_else(|| anyhow::anyhow!("set TH08_DIR"))?;
    let gfx = pollster::block_on(th08::gfx::Gfx::headless())?;
    let src = th08::sources::native(dir.into());
    let mut ctx = pollster::block_on(th08::game::Ctx::load_with(
        src.clone(),
        th08::settings::get().tool_io(gfx, &src),
        th08::settings::get().core(),
        false,
    ))?;
    let trace = std::fs::read_to_string(format!(
        "{}/specs/data/demo{n}_trace.txt",
        env!("CARGO_MANIFEST_DIR")
    ))?;
    if range.is_none() && rng_frame.is_none() {
        println!("{}", th08::demosync::run(&mut ctx, n, &trace)?);
        return Ok(());
    }
    let rows = parse_trace(&trace);
    let r = th08::replay::parse(
        &ctx.archive.read(&format!("demorpy{n}.rpy"))?,
        ctx.w.game.ver_list.get(),
    )?;
    ctx.w.game.replay_ff = false;
    th08::replay::start_playback(&mut ctx.w.game, r, None, Some(n));
    let (a, b) = range.unwrap_or((0, 0));
    let mut base = 0;
    let mut started = false;
    // Kept across calcs that do not advance the demo frame, like the former thread-local list.
    let mut calls = Vec::new();
    loop {
        let before = ctx.w.game.demo_frame;
        if !started {
            base = ctx.w.rng.total;
        }
        let tr = rng_frame == Some(before + 1);
        ctx.w.rng.trace = tr.then(Vec::new);
        eprintln!("== calc for frame {}", before + 1);
        ctx.calc();
        calls.extend(ctx.w.rng.trace.take().unwrap_or_default());
        if ctx.w.game.demo_frame == before {
            continue;
        }
        started = true;
        let f = ctx.w.game.demo_frame as usize;
        if tr {
            let mut groups: Vec<(String, usize)> = Vec::new();
            for c in std::mem::take(&mut calls) {
                let frames: Vec<String> = c
                    .lines()
                    .filter(|l| {
                        l.contains("th08::")
                            && !l.contains("th08::rng::")
                            && !l.contains("LocalKey")
                            && !l.contains("closure")
                    })
                    .map(|l| {
                        let l = l.trim();
                        let l = l.split_once(": ").map_or(l, |x| x.1);
                        l.replace("th08::", "")
                            .replace(" as task::Task<game::Ctx>>", ">")
                    })
                    .take(6)
                    .collect();
                let k = frames.join(" <- ");
                match groups.last_mut() {
                    Some((g, n)) if *g == k => *n += 1,
                    _ => groups.push((k, 1)),
                }
            }
            for (g, n) in groups {
                println!("  {n:4} x {g}");
            }
        }
        if f >= a && f <= b && th08::settings::get().bdump {
            if ctx.w.gm.is_some() {
                if let Some(em) = ctx.w.enemies.as_deref() {
                    for (i, x) in em.enemies.iter().enumerate() {
                        if x.alive() {
                            println!(
                                "  e{i} sub {} t {} pos {:.1},{:.1} fl {:x} life {}",
                                x.main.ctx.sub,
                                x.main.ctx.clock.cur,
                                x.final_pos[0],
                                x.final_pos[1],
                                x.flags,
                                x.life
                            );
                        }
                    }
                }
                if th08::settings::get().idump
                    && let Some(p) = ctx.w.player.as_deref()
                {
                    println!(
                        "  player {:.1},{:.1} state {} poc {} {}",
                        p.pos[0], p.pos[1], p.state, p.sht.poc_line, p.sht_youkai.poc_line
                    );
                }
                if let Some(bm) = ctx
                    .w
                    .bullets
                    .as_deref()
                    .filter(|_| th08::settings::get().idump)
                {
                    for (i, x) in bm.items.items.iter().enumerate() {
                        if x.alive {
                            println!(
                                "  i{i} kind {} st {} pos {:.1},{:.1} t {}",
                                x.kind, x.state, x.pos[0], x.pos[1], x.timer.cur
                            );
                        }
                    }
                }
                if let Some(bm) = ctx
                    .w
                    .bullets
                    .as_deref()
                    .filter(|_| !th08::settings::get().idump)
                {
                    for (i, x) in bm.bullets.iter().enumerate() {
                        if x.state != 0 {
                            println!(
                                "  b{i} st {} pos {:.1},{:.1} vel {:.2},{:.2} ang {:.2} off {} act {:x}",
                                x.state,
                                x.pos[0],
                                x.pos[1],
                                x.vel[0],
                                x.vel[1],
                                x.angle,
                                x.off_count,
                                x.active
                            );
                        }
                    }
                }
            }
        }
        if f >= a
            && f <= b
            && let Some(k) = th08::settings::get().edump
        {
            print!("frame {f} {}", ctx.w.effects.dump(k));
        }
        if f >= a && f <= b {
            let got = sample(&ctx, base);
            let want = rows[f - 1];
            let diff: Vec<String> = (0..22)
                .filter(|&c| c != 18 && want[c] != got[c])
                .map(|c| format!("{}={:x}/{:x}", FIELDS[c], want[c], got[c]))
                .collect();
            println!(
                "{f}: in {:04x} rng {:04x}/{:04x} ctr {}/{} bul {}/{} en {}/{} {}",
                want[2],
                want[3],
                got[3],
                want[4],
                got[4],
                want[16],
                got[16],
                want[17],
                got[17],
                diff.join(" ")
            );
        }
        if f >= b.max(rng_frame.unwrap_or(0) as usize) || f >= rows.len() {
            break;
        }
    }
    Ok(())
}
