//! Headless port run that writes a draw log: `drawrun <out[.xz]> menu <input.txt> [clr]` replays a menu script
//! (README of specs/data/drawlog; a `*mopup1*` input gets that script's score.dat and replays), `drawrun <out[.xz]> demo <0-3> [last demo frame]` plays an attract demo.

use std::io::Write;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        args.len() >= 3,
        "usage: drawrun <out> menu <input.txt> [clr] | drawrun <out> demo <n> [last]"
    );
    let dir = th08::settings::test_game_dir().ok_or_else(|| anyhow::anyhow!("set TH08_DIR"))?;
    let gfx = pollster::block_on(th08::gfx::Gfx::headless())?;
    let src = th08::assets::Source::native(dir.into());
    let mut ctx = pollster::block_on(th08::game::Ctx::load_with(src, gfx, false))?;
    let (mut xz, mut plain) = if args[0].ends_with(".xz") {
        (Some(th08::xz::create(args[0].as_ref())?), None)
    } else {
        let f = std::fs::File::create(&args[0])?;
        (None, Some(std::io::BufWriter::new(f)))
    };
    let mut put = |s: String| {
        let w: &mut dyn Write = match (&mut xz, &mut plain) {
            (Some(w), _) => w,
            (_, Some(w)) => w,
            _ => unreachable!(),
        };
        let _ = w.write_all(s.as_bytes());
    };
    match args[1].as_str() {
        "menu" => {
            let script = th08::input::load_script(&args[2])
                .ok_or_else(|| anyhow::anyhow!("bad input script {}", args[2]))?;
            let clr = args.get(3).is_some_and(|a| a == "clr");
            let name = if clr {
                "menu_clr"
            } else if args[2].contains("mopup1") {
                "menu_mopup1"
            } else {
                "menu_base"
            };
            th08::drawlog::menu_start_state(&mut ctx, name)?;
            th08::drawlog::run_menu_script(&mut ctx, &script, clr, &mut put);
        }
        "demo" => {
            let n: u8 = args[2].parse()?;
            let last = args
                .get(3)
                .and_then(|v| v.parse().ok())
                .unwrap_or(i32::MAX - 4000);
            th08::drawlog::run_demo(&mut ctx, n, last, &mut put)?;
        }
        m => anyhow::bail!("unknown mode {m}"),
    }
    if let Some(w) = xz {
        th08::xz::finish(w)?;
    }
    if let Some(mut w) = plain {
        w.flush()?;
    }
    Ok(())
}
