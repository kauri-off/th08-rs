//! Renders the in-game port menu over a headless game frame to a PNG (TH08_DIR: the game folder).

use th08::platform::portmenu::{Nav, Shot, picture_safe, render_image};

// Switches of this tool, removed before the th08 switches are parsed.
const OWN: [&str; 9] = [
    "--frames=",
    "--safe=",
    "--touch",
    "--density=",
    "--pad=",
    "--speed=",
    "--no-saves",
    "--status=",
    "--nav=",
];

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        !args.is_empty(),
        "usage: portmenushot OUT.png [WxH] [--frames=N] [--safe=L,T,R,B] [--touch] [--density=F] [--pad=NAME] \
         [--speed=N/D] [--no-saves] [--status=TEXT] [--settings=TAB] [--nav=down,z,…] [th08 switches]..."
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
    let opt = |name: &str| {
        args.iter()
            .find_map(|a| a.strip_prefix(name))
            .map(str::to_string)
    };
    let flag = |name: &str| args.iter().any(|a| a == name);
    let mut navs = Vec::new();
    if let Some(tab) = opt("--settings=") {
        navs.extend([Nav::Down; 8]);
        navs.push(Nav::Activate);
        navs.extend(std::iter::repeat_n(Nav::NextTab, tab.parse()?));
    }
    for n in opt("--nav=").iter().flat_map(|v| v.split(',')) {
        navs.push(match n {
            "up" => Nav::Up,
            "down" => Nav::Down,
            "left" => Nav::Left,
            "right" => Nav::Right,
            "z" => Nav::Activate,
            "x" => Nav::Back,
            "q" => Nav::PrevTab,
            "e" => Nav::NextTab,
            _ => anyhow::bail!("--nav={n}: expected up, down, left, right, z, x, q, e"),
        });
    }
    let speed = match opt("--speed=") {
        None => None,
        Some(v) => {
            let (a, b) = v
                .split_once('/')
                .ok_or_else(|| anyhow::anyhow!("--speed={v}: N/D"))?;
            Some((a.parse()?, b.parse()?))
        }
    };
    let mut shot = Shot {
        touch: flag("--touch"),
        density: opt("--density=").map_or(Ok(1.0), |v| v.parse())?,
        pad: opt("--pad="),
        speed,
        no_saves: flag("--no-saves"),
        status: opt("--status="),
        navs,
        ..Shot::default()
    };
    args.retain(|a| !OWN.iter().any(|o| a.starts_with(o)) && !a.starts_with("--settings="));
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
    shot.quit = th08::platform::quittitle::available(&ctx.w);
    let quit = shot.quit;
    let g: &mut th08_render::gfx::Gfx = ctx
        .renderer
        .as_any_mut()
        .downcast_mut()
        .expect("headless Gfx");
    g.safe = picture_safe(safe, (w, h), &shot);
    let frame = g.capture(&mut ctx.rec, Some((w, h)));
    g.safe = safe;
    render_image(g, &s, &frame, safe, &shot)
        .save(&out)
        .map_err(|e| anyhow::anyhow!("{out}: {e}"))?;
    eprintln!(
        "scene {:?}, paused {}, quit to title {}",
        ctx.w.game.scene, ctx.w.game.paused, quit
    );
    Ok(())
}
