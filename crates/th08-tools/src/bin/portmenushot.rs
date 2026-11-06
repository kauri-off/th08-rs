//! Renders the in-game port menu over a headless game frame to a PNG (TH08_DIR: the game folder).

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        !args.is_empty(),
        "usage: portmenushot OUT.png [WxH] [--frames=N] [--safe=L,T,R,B] [--touch] [th08 switches]..."
    );
    let out = args.remove(0);
    let size = args
        .first()
        .and_then(|a| a.split_once('x'))
        .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)));
    if size.is_some() {
        args.remove(0);
    }
    let (w, h) = size.unwrap_or((640, 480));
    let frames: u32 = args
        .iter()
        .find_map(|a| a.strip_prefix("--frames="))
        .map_or(Ok(300), str::parse)?;
    let safe = match args.iter().find_map(|a| a.strip_prefix("--safe=")) {
        None => th08_render::gfx::SafeArea::default(),
        Some(v) => {
            let n: Vec<u32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            anyhow::ensure!(n.len() == 4, "--safe={v}: expected L,T,R,B");
            th08_render::gfx::SafeArea {
                left: n[0],
                top: n[1],
                right: n[2],
                bottom: n[3],
            }
        }
    };
    let touch = args.iter().any(|a| a == "--touch");
    args.retain(|a| !a.starts_with("--frames=") && !a.starts_with("--safe=") && a != "--touch");
    let s = th08::settings::Settings::from_args(th08::launcher::desktop_dirs().tool(), args)
        .unwrap_or_else(|e| e.exit());
    let dir = th08::settings::test_game_dir().ok_or_else(|| anyhow::anyhow!("set TH08_DIR"))?;
    let mut gfx = pollster::block_on(th08_render::gfx::Gfx::headless())?;
    gfx.safe = safe;
    gfx.blit_options = s.blit;
    let src = th08::sources::native(dir);
    let mut ctx = pollster::block_on(th08_core::game::Ctx::load_with(
        src.clone(),
        s.tool_io(gfx, &src),
        s.core(),
        false,
    ))?;
    for _ in 0..frames {
        ctx.calc();
        ctx.draw();
        ctx.present();
    }
    ctx.calc();
    ctx.draw();
    let quit = th08::platform::quittitle::available(&ctx.w);
    let g: &mut th08_render::gfx::Gfx = ctx
        .renderer
        .as_any_mut()
        .downcast_mut()
        .expect("headless Gfx");
    let frame = g.capture(&mut ctx.rec, Some((w, h)));
    th08::platform::portmenu::render_image(g, &s, &frame, safe, quit, touch)
        .save(&out)
        .map_err(|e| anyhow::anyhow!("{out}: {e}"))?;
    eprintln!(
        "scene {:?}, paused {}, quit to title {}",
        ctx.w.game.scene, ctx.w.game.paused, quit
    );
    Ok(())
}
