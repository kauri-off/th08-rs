// Headless port run that writes a draw log: `drawrun <out[.xz]> menu <input.txt> [clr]` replays a menu script
// (README of specs/data/drawlog), `drawrun <out[.xz]> demo <0-3> [last demo frame]` plays an attract demo.
use std::io::Write;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        args.len() >= 3,
        "usage: drawrun <out> menu <input.txt> [clr] | drawrun <out> demo <n> [last]"
    );
    let dir = std::env::var("TH08_DIR")?;
    let gfx = pollster::block_on(th08::gfx::Gfx::headless())?;
    let src = th08::assets::Source::native(dir.into());
    let mut ctx = pollster::block_on(th08::game::Ctx::load_with(src, gfx, false))?;
    let file = std::fs::File::create(&args[0])?;
    let mut child = None;
    let mut w: Box<dyn Write> = if args[0].ends_with(".xz") {
        let mut c = std::process::Command::new("xz")
            .args(["-1", "-T0", "-c"])
            .stdin(std::process::Stdio::piped())
            .stdout(file)
            .spawn()?;
        let w = std::io::BufWriter::new(c.stdin.take().unwrap());
        child = Some(c);
        Box::new(w)
    } else {
        Box::new(std::io::BufWriter::new(file))
    };
    let mut put = |s: String| {
        let _ = w.write_all(s.as_bytes());
    };
    match args[1].as_str() {
        "menu" => {
            let script = th08::input::load_script(&args[2])
                .ok_or_else(|| anyhow::anyhow!("bad input script {}", args[2]))?;
            let clr = args.get(3).is_some_and(|a| a == "clr");
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
    w.flush()?;
    drop(w);
    if let Some(mut c) = child {
        c.wait()?;
    }
    Ok(())
}
