//! Session replay: `session <session dir> [--out DIR] [--no-drawlog] [--from N] [--to N]` runs a recorded session
//! of the original in the port (headless), writes DIR/trace.txt (+ DIR/draw.drawlog.xz), compares the trace with the
//! recorded one and the draw log with sessions/<name>/draw_*.drawlog.xz (DIR/index.html). N = main-menu frame.

use std::io::Write;
use std::path::{Path, PathBuf};
use th08::drawlog::{self, DemoMerge, KeyMode, Opts};
use th08::session::{self, Session};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut dir, mut out, mut drawlog_on, mut from, mut to) = (
        None,
        PathBuf::from("/tmp/th08_session"),
        true,
        1usize,
        usize::MAX,
    );
    let mut port_only = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out = it.next().map(PathBuf::from).unwrap_or(out),
            "--no-drawlog" => drawlog_on = false,
            "--port-drawlog" => port_only = true,
            "--from" => from = it.next().and_then(|v| v.parse().ok()).unwrap_or(1).max(1),
            "--to" => to = it.next().and_then(|v| v.parse().ok()).unwrap_or(to),
            _ => dir = Some(PathBuf::from(a)),
        }
    }
    let dir = dir.ok_or_else(|| {
        anyhow::anyhow!(
            "usage: session <session dir> [--out DIR] [--no-drawlog] [--port-drawlog] [--from N] [--to N]"
        )
    })?;
    let s = Session::load(&dir)?;
    if s.noclip {
        eprintln!("noclip: player invulnerable (meta.txt)");
    }
    std::fs::create_dir_all(&out)?;
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mut ref_paths = drawlog::expand(&dir.join("draw").to_string_lossy());
    if ref_paths.is_empty() {
        let alt = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("sessions")
            .join(name);
        ref_paths = drawlog::expand(&alt.join("draw").to_string_lossy());
    }
    let drawlog_on = drawlog_on && !ref_paths.is_empty();
    if !drawlog_on {
        eprintln!("draw log: off (no reference draw_*.drawlog.xz or --no-drawlog)");
    }

    let tdir = th08::settings::test_game_dir().ok_or_else(|| anyhow::anyhow!("set TH08_DIR"))?;
    let gfx = pollster::block_on(th08::gfx::Gfx::headless())?;
    let src = th08::assets::Source::native(tdir.into());
    let mut ctx = pollster::block_on(th08::game::Ctx::load_with(src, gfx, false))?;
    s.start(&mut ctx)?;

    let boot = s.sched.boot_calc as i64;
    let range = (from as i64 - 1 + boot, (to.min(1 << 40) as i64) + boot - 1);
    let mut merge = drawlog_on.then(|| -> anyhow::Result<_> {
        let mut m = DemoMerge::new(drawlog::open(&ref_paths)?, Opts { playfield: false }, 8);
        m.key = KeyMode::Present;
        m.range = Some(range);
        Ok(m)
    });
    let mut merge = match merge.take() {
        Some(m) => Some(m?),
        None => None,
    };
    let port_log = out.join("draw.drawlog.xz");
    // `--port-drawlog`: the port's own log of rows from..to, also for sessions without a reference log.
    let port_only = port_only && !drawlog_on;
    let mut xz = if drawlog_on || port_only {
        Some(th08::xz::create(&port_log)?)
    } else {
        None
    };
    let mut trace = std::io::BufWriter::new(std::fs::File::create(out.join("trace.txt"))?);
    trace.write_all(session::trace_header().as_bytes())?;
    let mut port_rows = Vec::new();
    // Debug `TH08_SESSION_DUMP=a-b`: player and items after each calc of menu frames a..b.
    let dump = th08::settings::get().session_dump;
    let t0 = std::time::Instant::now();
    {
        let mut on_draw = |text: String| {
            let present = text
                .split_whitespace()
                .nth(1)
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
            if port_only && !(range.0..=range.1 + 1).contains(&present) {
                return;
            }
            if let Some(w) = &mut xz {
                let _ = w.write_all(text.as_bytes());
            }
            if let Some(m) = &mut merge {
                for f in drawlog::parse_str(&text) {
                    m.feed(&f);
                }
            }
        };
        let draw: Option<&mut dyn FnMut(String)> = if drawlog_on || port_only {
            Some(&mut on_draw)
        } else {
            None
        };
        s.run(
            &mut ctx,
            to,
            |r| {
                if dump.is_some_and(|(a, b)| r[25] >= a && r[25] <= b) {
                    dump_items(r[25]);
                }
                let _ = writeln!(trace, "{}", session::format_row(r));
                if r[26] == 1 || !port_rows.is_empty() {
                    port_rows.push(*r);
                }
                true
            },
            draw,
        );
    }
    trace.flush()?;
    drop(trace);
    if let Some(w) = xz.take() {
        th08::xz::finish(w)?;
    }
    eprintln!(
        "ran {} menu-relative rows in {:.1} s",
        port_rows.len(),
        t0.elapsed().as_secs_f64()
    );

    let rec = s.recorded()?;
    let k = from - 1;
    let c = session::compare(
        rec.get(k..).unwrap_or(&[]),
        port_rows.get(k..).unwrap_or(&[]),
    );
    if k > 0 {
        println!("trace (row numbers counted from menu frame {from}):");
    } else {
        println!("trace:");
    }
    print!("{c}");
    if let Some((row, _)) = &c.first {
        let i = k + row - 1;
        println!("rows around it (recorded / port):");
        for j in i.saturating_sub(2)..(i + 3).min(rec.len()).min(port_rows.len()) {
            println!("  r {:6} {}", j + 1, session::format_row(&rec[j]));
            println!("  p {:6} {}", j + 1, session::format_row(&port_rows[j]));
        }
    }

    if let Some(m) = merge {
        let rep = m.finish();
        println!("\ndraw log (key present):\n{}", rep.summary());
        let entries = th08::drawreport::entries(&rep);
        let in_range = |f: &drawlog::Frame| f.present >= range.0 && f.present <= range.1;
        th08::drawreport::write_html(
            &out,
            (&ref_paths[0].to_string_lossy(), &port_log.to_string_lossy()),
            &rep,
            &entries,
            drawlog::open(&ref_paths)?.filter(in_range),
            drawlog::open(&[port_log.clone()])?.filter(in_range),
        )?;
    }
    Ok(())
}

fn dump_items(row: i64) {
    let Some(gm) = th08::gamemgr::current() else {
        return;
    };
    let gm = gm.borrow();
    let p = gm.player.borrow();
    println!(
        "row {row}: player {:.3},{:.3} state {} power {} target {:?}",
        p.pos[0], p.pos[1], p.state, p.stats.power, p.targets[0]
    );
    if th08::settings::get().session_pb {
        for (i, b) in p.bullets.iter().enumerate().filter(|(_, b)| b.state != 0) {
            println!(
                "  pb{i} st {} kind {} pos {:.2},{:.2} size {:.1},{:.1} vel {:.2},{:.2} dmg {} t {}",
                b.state,
                b.kind,
                b.pos[0],
                b.pos[1],
                b.size[0],
                b.size[1],
                b.vel[0],
                b.vel[1],
                b.damage,
                b.timer.cur
            );
        }
    }
    for (k, o) in p.options.iter().enumerate().filter(|(_, o)| o.state != 0) {
        println!(
            "  opt{k} st {} sub {} pos {:.2},{:.2} vel {:.3},{:.3} t {} aimed {:?}",
            o.state, o.sub, o.pos[0], o.pos[1], o.vel[0], o.vel[1], o.timer.cur, p.aimed
        );
    }
    if let Some(em) = &gm.enemies {
        for (i, x) in em.borrow().enemies.iter().enumerate() {
            if x.alive() {
                println!(
                    "  e{i} sub {} t {} pos {:.2},{:.2} fl {:x} life {} acc {}",
                    x.main.ctx.sub,
                    x.main.ctx.clock.cur,
                    x.final_pos[0],
                    x.final_pos[1],
                    x.flags,
                    x.life,
                    x.orb_acc
                );
                if x.flags & th08::enemy::A_BOSS != 0 {
                    println!(
                        "    boss dep {} max {} hud {} thr {:?} tsub {:?} tmo {} tmosub {} dsub {} tim {} dmgt {} flb {:x} bon {}",
                        x.main.depth,
                        x.max_life,
                        x.hud_life,
                        x.thresholds,
                        x.threshold_subs,
                        x.timeout,
                        x.timeout_sub,
                        x.death_sub,
                        x.boss_timer.cur,
                        x.dmg_timer.cur,
                        x.flags_b,
                        x.bonus_seconds
                    );
                }
                for t in x.threads.iter().flatten() {
                    println!(
                        "    thread sub {} t {} w {} ip {:#x}",
                        t.ctx.sub, t.ctx.clock.cur, t.ctx.wait.cur, t.ctx.ip
                    );
                }
            }
        }
    }
    if let Some(bm) = &gm.bullets {
        for (i, x) in bm.borrow().items.items.iter().enumerate() {
            if x.alive {
                println!(
                    "  i{i} kind {} st {} pos {:.3},{:.3} vel {:.3},{:.3} t {}",
                    x.kind, x.state, x.pos[0], x.pos[1], x.vel[0], x.vel[1], x.timer.cur
                );
            }
        }
    }
}
