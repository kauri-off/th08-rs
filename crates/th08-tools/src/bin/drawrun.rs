//! Headless port run that writes a draw log (or `--draw-hash` hashes) of a menu input script or an attract demo.

use std::io::Write;

fn main() -> anyhow::Result<()> {
    // RUST_LOG only (the pack mount line for arch_check); stderr is never digested.
    let _ = env_logger::try_init();
    let (mut args, mut hash_on, mut window) = (Vec::new(), false, (i64::MIN, i64::MAX));
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--draw-hash" => hash_on = true,
            "--window" => {
                let w = it.next();
                let w = w.as_deref().and_then(|v| v.split_once('-'));
                let w = w.and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)));
                window = w.ok_or_else(|| anyhow::anyhow!("--window A-B"))?;
            }
            _ => args.push(a),
        }
    }
    anyhow::ensure!(
        args.len() >= 3,
        "usage: drawrun [--draw-hash] [--window A-B] <out> menu <input.txt> [clr] | drawrun [--draw-hash] [--window A-B] <out> demo <n> [last]"
    );
    let dir = th08::settings::test_game_dir().ok_or_else(|| anyhow::anyhow!("set TH08_DIR"))?;
    let gfx = pollster::block_on(th08_render::gfx::Gfx::headless())?;
    let src = th08::sources::native(dir);
    let mut ctx = pollster::block_on(th08_core::game::Ctx::load_with(
        src.clone(),
        th08::settings::get().tool_io(gfx, &src),
        th08::settings::get().core(),
        false,
    ))?;
    let mut hash = if hash_on {
        std::fs::create_dir_all(&args[0])?;
        Some(th08_tools::drawlog::HashSink::create(
            &std::path::Path::new(&args[0]).join("draw.hash"),
        )?)
    } else {
        None
    };
    let (mut xz, mut plain) = if hash_on {
        (None, None)
    } else if args[0].ends_with(".xz") {
        (Some(th08_tools::xz::create(args[0].as_ref())?), None)
    } else {
        let f = std::fs::File::create(&args[0])?;
        (None, Some(std::io::BufWriter::new(f)))
    };
    let mut put = |s: String| {
        let present = s.split_ascii_whitespace().nth(1);
        let present = present.and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        if !(window.0..=window.1).contains(&present) {
            return;
        }
        let w: &mut dyn Write = match (&mut xz, &mut plain, &mut hash) {
            (_, _, Some(h)) => return h.frame(&s),
            (Some(w), _, _) => w,
            (_, Some(w), _) => w,
            _ => unreachable!(),
        };
        let _ = w.write_all(s.as_bytes());
    };
    match args[1].as_str() {
        "menu" => {
            let script = th08_core::input::load_script(&args[2], &mut *ctx.host)
                .ok_or_else(|| anyhow::anyhow!("bad input script {}", args[2]))?;
            let clr = args.get(3).is_some_and(|a| a == "clr");
            let name = if clr {
                "menu_clr"
            } else if args[2].contains("mopup1") {
                "menu_mopup1"
            } else {
                "menu_base"
            };
            th08_tools::drawlog::menu_start_state(&mut ctx, name)?;
            th08_tools::drawlog::run_menu_script(&mut ctx, &script, clr, &mut put);
        }
        "demo" => {
            let n: u8 = args[2].parse()?;
            let last = args
                .get(3)
                .and_then(|v| v.parse().ok())
                .unwrap_or(i32::MAX - 4000);
            th08_tools::drawlog::run_demo(&mut ctx, n, last, &mut put)?;
        }
        m => anyhow::bail!("unknown mode {m}"),
    }
    if let Some(w) = xz {
        th08_tools::xz::finish(w)?;
    }
    if let Some(h) = hash {
        h.finish()?;
    }
    if let Some(mut w) = plain {
        w.flush()?;
    }
    Ok(())
}
