//! Session replay: runs a recorded session of the original headless and compares trace and draw log with it.

use std::io::Write;
use std::path::{Path, PathBuf};
use th08_tools::drawlog::{self, DemoMerge, KeyMode, Opts};
use th08_tools::int;
use th08_tools::session::{self, Session};

// N = main-menu frame; `--port-drawlog` logs without a reference, `--draw-hash` writes per-frame hashes instead,
// `--draw-window A-B` limits both to presents A..B, `--sound-log` writes the `SoundCmd` stream, `--no-emit` records nothing.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent command-line switches"
)]
struct Args {
    dir: PathBuf,
    out: PathBuf,
    drawlog_on: bool,
    from: usize,
    to: usize,
    port_only: bool,
    sound_log: bool,
    emit: bool,
    hash_on: bool,
    window: (i64, i64),
}

fn parse_args() -> anyhow::Result<Args> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut dir, mut out, mut drawlog_on, mut from, mut to) = (
        None,
        PathBuf::from("/tmp/th08_session"),
        true,
        1usize,
        usize::MAX,
    );
    let (mut port_only, mut sound_log, mut emit) = (false, false, true);
    let (mut hash_on, mut window) = (false, (i64::MIN, i64::MAX));
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out = it.next().map(PathBuf::from).unwrap_or(out),
            "--no-drawlog" => drawlog_on = false,
            "--port-drawlog" => port_only = true,
            "--sound-log" => sound_log = true,
            "--no-emit" => emit = false,
            "--draw-hash" => hash_on = true,
            "--draw-window" => {
                let w = it.next().and_then(|v| v.split_once('-'));
                let w = w.and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)));
                window = w.ok_or_else(|| anyhow::anyhow!("--draw-window A-B"))?;
            }
            "--from" => from = it.next().and_then(|v| v.parse().ok()).unwrap_or(1).max(1),
            "--to" => to = it.next().and_then(|v| v.parse().ok()).unwrap_or(to),
            _ => dir = Some(PathBuf::from(a)),
        }
    }
    let dir = dir.ok_or_else(|| {
        anyhow::anyhow!(
            "usage: session <session dir> [--out DIR] [--no-drawlog] [--port-drawlog] [--draw-hash] [--draw-window A-B] [--sound-log] [--no-emit] [--from N] [--to N]"
        )
    })?;
    Ok(Args {
        dir,
        out,
        drawlog_on,
        from,
        to,
        port_only,
        sound_log,
        emit,
        hash_on,
        window,
    })
}

// Headless game context with the session's start state; run metadata beside trace.txt.
fn load_ctx(s: &Session, a: &Args) -> anyhow::Result<th08_core::game::Ctx> {
    let tdir = th08::settings::test_game_dir().ok_or_else(|| anyhow::anyhow!("set TH08_DIR"))?;
    let gfx = pollster::block_on(th08_render::gfx::Gfx::headless())?;
    let src = th08::sources::native(tdir);
    let mut ctx = pollster::block_on(th08_core::game::Ctx::load_with(
        src.clone(),
        th08::settings::get().tool_io(gfx, &src),
        th08::settings::get().core(),
        false,
    ))?;
    s.start(&mut ctx)?;
    // Run metadata beside trace.txt (the trace bytes stay as they are): logic-content fingerprint, empty = vanilla.
    std::fs::write(
        a.out.join("meta.txt"),
        format!("fingerprint: {}\n", ctx.archive.fingerprint()),
    )?;
    ctx.set_emit(a.emit);
    if a.sound_log {
        ctx.w.bgm.midi_ready = false;
        ctx.sound_log = Some(Vec::new());
    }
    Ok(ctx)
}

fn main() -> anyhow::Result<()> {
    // RUST_LOG only (the pack mount line for arch_check); stderr is never digested.
    let _ = env_logger::try_init();
    let a = parse_args()?;
    let (dir, out, from, to, window) = (&a.dir, &a.out, a.from, a.to, a.window);
    let s = Session::load(dir)?;
    if s.noclip {
        eprintln!("noclip: player invulnerable (meta.txt)");
    }
    std::fs::create_dir_all(out)?;
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mut ref_paths = drawlog::expand(&dir.join("draw").to_string_lossy());
    if ref_paths.is_empty() {
        let alt = th08_core::testdata::repo_root().join("sessions").join(name);
        ref_paths = drawlog::expand(&alt.join("draw").to_string_lossy());
    }
    let port_only = a.port_only && a.emit;
    let hash_on = a.hash_on && a.emit;
    let drawlog_on = a.drawlog_on && a.emit && !port_only && !hash_on && !ref_paths.is_empty();
    if !drawlog_on && !port_only && !hash_on {
        eprintln!("draw log: off (no reference draw_*.drawlog.xz or --no-drawlog)");
    }
    let mut ctx = load_ctx(&s, &a)?;

    let boot = i64::from(s.sched.boot_calc);
    let range = (int(from) - 1 + boot, int(to.min(1 << 40)) + boot - 1);
    let mut merge = drawlog_on
        .then(|| -> anyhow::Result<_> {
            let mut m = DemoMerge::new(drawlog::open(&ref_paths)?, Opts { playfield: false }, 8);
            m.key = KeyMode::Present;
            m.range = Some(range);
            Ok(m)
        })
        .transpose()?;
    let port_log = out.join("draw.drawlog.xz");
    // `--port-drawlog`: the port's own log of rows from..to, with or without a reference log.
    let mut xz = if drawlog_on || port_only {
        Some(th08_tools::xz::create(&port_log)?)
    } else {
        None
    };
    let mut hash = if hash_on {
        Some(drawlog::HashSink::create(&out.join("draw.hash"))?)
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
            let keep = (range.0..=range.1 + 1).contains(&present)
                && (window.0..=window.1).contains(&present);
            if let Some(h) = &mut hash
                && keep
            {
                h.frame(&text);
            }
            if port_only && !keep {
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
        let draw: Option<&mut dyn FnMut(String)> = if drawlog_on || port_only || hash_on {
            Some(&mut on_draw)
        } else {
            None
        };
        s.run(
            &mut ctx,
            to,
            |r, ctx| {
                if dump.is_some_and(|(a, b)| r[25] >= a && r[25] <= b) {
                    dump_items(ctx, r[25]);
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
        th08_tools::xz::finish(w)?;
    }
    if let Some(h) = hash.take() {
        h.finish()?;
    }
    if let Some(log) = &ctx.sound_log {
        std::fs::write(out.join("sound.txt"), th08_core::sound::format_log(log))?;
    }
    eprintln!(
        "ran {} menu-relative rows in {:.1} s",
        port_rows.len(),
        t0.elapsed().as_secs_f64()
    );

    report_trace(&s, from, &port_rows)?;
    if let Some(m) = merge {
        report_draw(m, out, &ref_paths, &port_log, range)?;
    }
    Ok(())
}

// Recorded vs port trace from menu frame `from`, with the rows around the first difference.
fn report_trace(
    s: &Session,
    from: usize,
    port_rows: &[th08_core::trace::Row],
) -> anyhow::Result<()> {
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
    Ok(())
}

// Draw-log summary and DIR/index.html for the presents in `range`.
fn report_draw(
    m: DemoMerge<impl Iterator<Item = drawlog::Frame>>,
    out: &Path,
    ref_paths: &[PathBuf],
    port_log: &Path,
    range: (i64, i64),
) -> anyhow::Result<()> {
    let rep = m.finish();
    println!("\ndraw log (key present):\n{}", rep.summary());
    let entries = th08_tools::drawreport::entries(&rep);
    let in_range = |f: &drawlog::Frame| f.present >= range.0 && f.present <= range.1;
    th08_tools::drawreport::write_html(
        out,
        (&ref_paths[0].to_string_lossy(), &port_log.to_string_lossy()),
        &rep,
        &entries,
        drawlog::open(ref_paths)?.filter(in_range),
        drawlog::open(&[port_log.to_path_buf()])?.filter(in_range),
    )?;
    Ok(())
}

fn dump_items(ctx: &th08_core::game::Ctx, row: i64) {
    let Some(p) = ctx.w.player.as_deref() else {
        return;
    };
    if ctx.w.gm.is_none() {
        return;
    }
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
    if let Some(em) = ctx.w.enemies.as_deref() {
        for (i, x) in em.enemies.iter().enumerate() {
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
                if x.flags & th08_core::enemy::A_BOSS != 0 {
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
    if let Some(bm) = ctx.w.bullets.as_deref() {
        for (i, x) in bm.items.items.iter().enumerate() {
            if x.alive {
                println!(
                    "  i{i} kind {} st {} pos {:.3},{:.3} vel {:.3},{:.3} t {}",
                    x.kind, x.state, x.pos[0], x.pos[1], x.vel[0], x.vel[1], x.timer.cur
                );
            }
        }
    }
}
