//! Supervisor g_Supervisor 0x17CE758 (11 §7): scene switches, input poll, boot / loading screens; calc 0, draw 0/2/16.

use crate::anmvm::AnmVm;
use crate::chain::Node;
use crate::coreconfig::{CoreConfig, Ending};
use crate::drawlist::{Blend, PLAYFIELD, Rect};
use crate::drawlist::{Recorder, TexId};
use crate::game::{Ctx, View};
use crate::gamemgr::{self, Game, GameManager};
use crate::task::{self, Chain, Ret, Task};
use crate::types::SceneId;
use anyhow::{Context, Result};

// Owned by `Ctx.supervisor`; its chain nodes are `SupervisorTask`.
#[derive(Clone)]
pub struct Supervisor {
    // Debug `TH08_SLOT_TEST` / `TH08_INPUT`.
    debug_sprites: Option<usize>,
    script: Option<Vec<u16>>,
    frame: usize,
    // Chain node of the registered scene (`Node::Scene`, or `Node::GameManager` for the GM); the scene
    // state is `Ctx.scene` / `Ctx.gm`.
    scene_task: Option<Node>,
    // Full-screen surface slot 8 (logo, captured frame or result.jpg), drawn at prio 0 while loading (12 §1).
    logo: Option<TexId>,
    // The three nowloading.anm VMs (scripts 0–2) and their screen position (12 §1 state 1).
    boot_vms: Vec<AnmVm>,
    boot_frames: u32,
    load_pos: [f32; 2],
    // Loading state of a stage start: Loading_Wait / Loading_End follow the GM loader (12 §1).
    stage_load: bool,
    // Pending post-present capture into surface 8 (0x44886F, 12 §1).
    capture_req: bool,
    // Playback fps text buffer 0x17CF2B8 (15 §3.3) and the port's refresh counter.
    fps_text: String,
    fps_frames: u32,
    // Queue entries the prio-0 loading prompt drained this frame (12 §1), drawn by the emit.
    prompt: Vec<crate::ascii::Text>,
}

// The boot and menu-load holds end when the loader threads finish (Supervisor_Calc returns 1 until then; MainMenu
// +0x14434), host-timed (12 §1), so any fixed count is allowed: 13 boot frames, then the menu loader until
// scene frame 34 (33 when the menu is entered again).
const BOOT_FRAMES: u32 = 14;
const MENU_LOAD_BOOT: u32 = 34;
const MENU_LOAD: u32 = 33;

pub fn register(ctx: &mut Ctx) -> Result<()> {
    ctx.w.supervisor = Some(Box::new(Supervisor {
        debug_sprites: ctx.core.slot_test,
        script: ctx
            .core
            .input
            .clone()
            .and_then(|v| crate::input::load_script(&v, &mut *ctx.host)),
        frame: 0,
        scene_task: None,
        logo: None,
        boot_vms: Vec::new(),
        boot_frames: 0,
        load_pos: [500.0, 440.0],
        stage_load: false,
        capture_req: false,
        fps_text: String::new(),
        fps_frames: 0,
        prompt: Vec::new(),
    }));
    let t = Node::Supervisor;
    task::add(ctx, Chain::Calc, 0, 0, t)?;
    for prio in [0, 2, 16] {
        task::add(ctx, Chain::Draw, prio, prio as u8, t)?;
    }
    Ok(())
}

// Debug tool, not a port. Debug start (`TH08_STAGE`): difficulty `TH08_RANK` (default Normal), shot `TH08_PLAYER` (default 0).
pub fn setup_new_game(g: &mut Game, s: &CoreConfig, stage: usize) {
    let shot = s.player.unwrap_or(0);
    let default_diff = if stage % 9 == 8 { 4 } else { 1 };
    g.difficulty = s.rank.unwrap_or(default_diff);
    g.shot = shot;
    g.stage = stage;
    g.flags = 0;
    g.fresh_start = true;
    g.request(SceneId::Game);
}

// Debug tool, not a port. `TH08_ENDING=<shot 0–11><a|b|c>` (bad / good 6A / good 6B, e.g. `0b`, `7a`) or a
// `.end` file name (played as a good 6A ending of `TH08_PLAYER`); difficulty `TH08_RANK`, continues `TH08_CONTINUES`.
pub fn setup_ending(g: &mut Game, s: &CoreConfig, e: &Ending) {
    let (shot, kind) = match e {
        Ending::File(_) => (s.player.unwrap_or(0), 'b'),
        Ending::Kind { shot, kind } => (*shot, *kind),
    };
    g.difficulty = s.rank.unwrap_or(1).min(3);
    g.shot = shot % 12;
    g.stats.continues = s.continues;
    match kind {
        'a' => {
            g.flags = 0;
            g.stage = 4;
            g.stats.clock = 12;
        }
        'c' => {
            g.flags = gamemgr::F_ENDING_OK;
            g.stage = 7;
        }
        _ => {
            g.flags = gamemgr::F_ENDING_OK;
            g.stage = 6;
        }
    }
    g.request(SceneId::Ending);
}

// The scene object registered by the Supervisor's scene switch, except the game scene, whose GM is `Ctx.gm`.
#[derive(Clone, Default)]
pub enum Scene {
    #[default]
    None,
    Menu(Box<crate::menu::MainMenu>),
    Result(Box<crate::result::ResultScreen>),
    Ending(Box<crate::ending::Ending>),
    MusicRoom(Box<crate::musicroom::MusicRoom>),
}

// One call on whichever scene object is set; `$none` without one.
macro_rules! on_scene {
    ($s:expr, $x:ident => $e:expr, $none:expr) => {
        match $s {
            Scene::Menu($x) => $e,
            Scene::Result($x) => $e,
            Scene::Ending($x) => $e,
            Scene::MusicRoom($x) => $e,
            Scene::None => $none,
        }
    };
}

impl Scene {
    // Test: the menu's screen, cursor and VM states; the other scenes by kind.
    #[cfg(test)]
    pub(crate) fn trace(&self) -> Vec<u64> {
        let h = crate::worldclone::hash_dbg;
        match self {
            Scene::Menu(m) => {
                let mut t = vec![
                    1,
                    i64::from(m.screen).cast_unsigned(),
                    i64::from(m.prev_screen).cast_unsigned(),
                    u64::from(m.frames),
                    u64::from(m.cursor.cast_unsigned()),
                    u64::from(m.idle),
                    m.pending.as_ref().map_or(0, |p| 1 + u64::from(p.frames)),
                    m.desc_cur.map_or(0, |d| d as u64 + 1),
                    h(&m.notice),
                ];
                t.extend(m.vms.iter().chain(&m.desc).map(h));
                t
            }
            Scene::Result(_) => vec![2],
            Scene::Ending(_) => vec![3],
            Scene::MusicRoom(_) => vec![4],
            Scene::None => vec![0],
        }
    }

    // The scene lent out of `ctx.w.scene` for one of its chain callbacks (only the Supervisor switches scenes).
    fn with<R>(ctx: &mut Ctx, f: impl FnOnce(&mut Scene, &mut Ctx) -> R) -> R {
        let mut s = std::mem::take(&mut ctx.w.scene);
        let r = f(&mut s, ctx);
        ctx.w.scene = s;
        r
    }
}

// Chain nodes of `ctx.w.scene` (the scene's calc / draw prios, the menu's surface node): stateless forwarder.
pub struct SceneTask;

impl Task<Ctx> for SceneTask {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, tag: u8) -> Result<()> {
        Scene::with(
            ctx,
            |s, ctx| on_scene!(s, x => x.on_register(ctx, chain, tag), Ok(())),
        )
    }

    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, tag: u8) {
        Scene::with(
            ctx,
            |s, ctx| on_scene!(s, x => x.on_remove(ctx, chain, tag), ()),
        );
    }

    fn calc(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        Scene::with(ctx, |s, ctx| on_scene!(s, x => x.calc(ctx, tag), Ret::Next))
    }

    fn draw_step(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        Scene::with(
            ctx,
            |s, ctx| on_scene!(s, x => x.draw_step(ctx, tag), Ret::Next),
        )
    }
}

impl SceneTask {
    pub(crate) fn emit(&self, ctx: &View, rec: &mut Recorder, tag: u8) {
        on_scene!(&ctx.w.scene, x => x.emit(ctx, rec, tag), ());
    }
}

// Chain nodes of `ctx.w.supervisor` (calc 0, draw 0 / 2 / 16): stateless forwarder.
pub struct SupervisorTask;

impl Task<Ctx> for SupervisorTask {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, tag: u8) -> Result<()> {
        Supervisor::with(ctx, |s, ctx| s.on_register(ctx, chain, tag))
    }

    fn calc(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        Supervisor::with(ctx, |s, ctx| s.calc(ctx, tag))
    }

    fn draw_step(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        Supervisor::with(ctx, |s, ctx| s.draw_step(ctx, tag))
    }
}

impl SupervisorTask {
    pub(crate) fn emit(&self, ctx: &View, rec: &mut Recorder, tag: u8) {
        if let Some(s) = ctx.w.supervisor.as_deref() {
            s.emit(ctx, rec, tag);
        }
    }
}

impl Supervisor {
    #[cfg(test)]
    pub(crate) fn trace(&self) -> Vec<u64> {
        let mut t = vec![
            self.frame as u64,
            u64::from(self.boot_frames),
            u64::from(self.stage_load),
            u64::from(self.capture_req),
            u64::from(self.logo.is_some()),
            self.prompt.len() as u64,
            u64::from(self.load_pos[0].to_bits()),
        ];
        t.extend(self.boot_vms.iter().map(crate::worldclone::hash_dbg));
        t
    }

    // The Supervisor lent out of `ctx.w.supervisor` for one of its chain callbacks.
    fn with<R>(ctx: &mut Ctx, f: impl FnOnce(&mut Supervisor, &mut Ctx) -> R) -> R {
        let mut s = ctx.w.supervisor.take().expect("no supervisor");
        let r = f(&mut s, ctx);
        ctx.w.supervisor = Some(s);
        r
    }

    // Fps_Update 0x446F53 playback text refresh (15 §3.3). The original refreshes on a 500 ms meter window continuous
    // since boot, whose phase cannot be reproduced (15 §3.3); the port refreshes every 30 gameplay calcs, on the 3rd.
    fn replay_fps(&mut self, ctx: &mut Ctx) {
        let g = &ctx.w.game;
        if g.loading != 0 || g.scene != SceneId::Game {
            self.fps_frames = 0;
            return;
        }
        if !g.replay() || g.flags & gamemgr::F_RUNNING == 0 {
            return;
        }
        self.fps_frames += 1;
        if self.fps_frames % 30 == 3 {
            self.fps_text = format!("{:2}", g.replay_fps);
        }
    }

    // Draw-time Fps_Update (Supervisor draw prio 16, 0x445BC0; 15 §3.3): "%2d" at (384, 448), yellow or red with the
    // flag, queued once per presented frame, then the queue colour reset to white.
    fn replay_fps_draw(&mut self, ctx: &mut Ctx) {
        let g = &ctx.w.game;
        if g.loading != 0
            || g.scene != SceneId::Game
            || !g.replay()
            || g.flags & gamemgr::F_RUNNING == 0
        {
            return;
        }
        let c = if g.replay_fps_red {
            0xFFFF_4040
        } else {
            0xFFFF_FFD0
        };
        ctx.w
            .ascii
            .push(crate::ascii::Text::new(384.0, 448.0, self.fps_text.clone()).argb(c));
        ctx.w.ascii.color = [255; 4];
    }

    // Loading_Begin 0x448790 (12 §1): surface 8 (the logo) full screen + nowloading.anm scripts 0–2 at (500, 440).
    fn begin_loading(&mut self, ctx: &mut Ctx) -> Result<()> {
        if self.logo.is_none() {
            let (b, over) = ctx.archive.present().image("th08logo.jpg")?;
            self.logo = Some(ctx.rec.load_image_over("th08logo.jpg", &b, over)?);
        }
        self.start_vms(ctx, [500.0, 440.0]);
        Ok(())
    }

    fn start_vms(&mut self, ctx: &mut Ctx, pos: [f32; 2]) {
        self.load_pos = pos;
        self.boot_vms = (0..3)
            .map(|i| {
                let mut vm = AnmVm::default();
                ctx.w.anm.set_script(&mut ctx.w.rng, &mut vm, 2, i);
                vm
            })
            .collect();
    }

    // Surface 8 := the last presented frame (post-present capture of 0x44886F).
    fn capture(&mut self, ctx: &mut Ctx) {
        if let Some(t) = self.logo.take() {
            ctx.rec.free_texture(t);
        }
        self.logo = Some(ctx.rec.capture_screen("th08logo.jpg"));
    }

    // Menu loader done: title fade-from-white (ScreenEffect type 0, 70 frames, 13 §3), Loading_End 0x44881A (interrupt 1
    // fades the VMs out, surface released, 12 §1).
    fn end_loading(&mut self, ctx: &mut Ctx) {
        if let Some(t) = self.logo.take() {
            ctx.rec.free_texture(t);
        }
        // Title fade (13 §3 "Title fade"): linked at once from the Supervisor calc (prio 0), so its first calc precedes
        // its first draw (alphas 255, 251, …).
        crate::effect::screen_now(ctx, 0, 70, 0x00FF_FFFF, 0, 0, 0x15);
        // Loading_End releases a loading darken overlay (13 §3 type 5); only the menu load after a game has one.
        ctx.w.effects.release_loading_overlay();
        for vm in &mut self.boot_vms {
            vm.pending_interrupt = 1;
        }
    }

    // Main-menu loading screen by previous scene (12 §1); returns the port's hold length (see BOOT_FRAMES).
    fn menu_loading(&mut self, ctx: &mut Ctx, from: SceneId) -> Result<()> {
        ctx.w.game.menu_loading = if from == SceneId::Boot {
            MENU_LOAD_BOOT
        } else if matches!(from, SceneId::Result | SceneId::ResultMenu) {
            // Loading_Begin already ran at result state 2 frame 1; Result_OnRemove moved result.jpg into surface 8,
            // so the menu loader shows it without a capture (16 §2.4 exit).
            if let Some(t) = self.logo.take() {
                ctx.rec.free_texture(t);
            }
            // Labelled by its surface slot's file name, as the draw log names surface 8.
            let (b, over) = ctx.archive.present().image("result.jpg")?;
            self.logo = Some(ctx.rec.load_image_over("th08logo.jpg", &b, over)?);
            MENU_LOAD
        } else if from == SceneId::MusicRoom {
            // The music room calls 0x44886F itself on the exit press, one frame before the scene switch.
            self.begin_loading(ctx)?;
            for vm in &mut self.boot_vms {
                vm.run(&mut ctx.w.anm, &mut ctx.w.rng);
            }
            MENU_LOAD - 1
        } else if from == SceneId::Game {
            // MainMenu_OnRegister, previous scene 2: Loading_BeginCapture 0x44886F (capture after the next Present, no
            // logo) + Loading_StartDarken(0); the first frame draws no surface 8 but the darken quad (12 §1).
            if let Some(t) = self.logo.take() {
                ctx.rec.free_texture(t);
            }
            self.start_vms(ctx, [500.0, 440.0]);
            self.capture_req = true;
            crate::effect::loading_overlay(ctx);
            MENU_LOAD
        } else {
            self.begin_loading(ctx)?;
            MENU_LOAD
        };
        Ok(())
    }

    // The menu's extra draw node for its title surface (menu::SURFACE_PRIO).
    fn menu_surface(&mut self, ctx: &mut Ctx) -> Result<()> {
        if let Some(t) = &self.scene_task {
            let p = crate::menu::SURFACE_PRIO;
            task::add(ctx, Chain::Draw, p, p as u8, *t)?;
        }
        Ok(())
    }

    fn drop_scene(&mut self, ctx: &mut Ctx) {
        if let Some(t) = self.scene_task.take() {
            task::remove_task(ctx, t);
        }
        // The game manager with its replay object, and the player, the bullet and enemy managers, the stage, HUD,
        // dialogue and g_Gui that go with it (dropped here, after GameMgr_OnRemove).
        ctx.w.player = None;
        ctx.w.bullets = None;
        ctx.w.enemies = None;
        ctx.w.stage = None;
        ctx.w.hud = None;
        ctx.w.gui = None;
        ctx.w.msg = None;
        ctx.w.gm = None;
        ctx.w.scene = Scene::None;
    }

    // GameMgr_OnRegister 0x43AAF4: 0x44886F at (500, 440) + darken overlay (ScreenEffect type 5) after the main menu,
    // else at (280, 430) (12 §1).
    fn start_game(&mut self, ctx: &mut Ctx) -> Result<()> {
        let gm = GameManager::register(ctx)?;
        self.scene_task = Some(gm);
        // Attract demos always start from the main menu.
        let from_menu =
            ctx.w.game.previous == SceneId::Menu || ctx.w.game.flags & gamemgr::F_DEMO != 0;
        if let Some(t) = self.logo.take() {
            ctx.rec.free_texture(t);
        }
        self.start_vms(
            ctx,
            if from_menu {
                [500.0, 440.0]
            } else {
                [280.0, 430.0]
            },
        );
        self.stage_load = true;
        self.capture_req = true;
        if from_menu {
            crate::effect::loading_overlay(ctx);
        }
        Ok(())
    }

    fn set_scene(&mut self, ctx: &mut Ctx, scene: Scene, calc: u16, draw: u16) -> Result<()> {
        ctx.w.scene = scene;
        let t = Node::Scene;
        task::add(ctx, Chain::Calc, calc, 0, t)?;
        task::add(ctx, Chain::Draw, draw, 0, t)?;
        self.scene_task = Some(t);
        Ok(())
    }

    // Supervisor_Calc 0x445453 scene switch (11 §7 table): `previous` (+0x160) := current, then by (current, requested).
    fn switch(&mut self, ctx: &mut Ctx) -> Result<Option<Ret>> {
        let g = &mut ctx.w.game;
        let (cur, req) = (g.scene, g.requested);
        if cur == req {
            return Ok(None);
        }
        g.previous = cur;
        if req == SceneId::Quit {
            // The quitting frame is not drawn or presented; the score.dat flush follows it (`Ctx::exit_save`, 15 §2.2).
            // 6 → −1 (Replay_Finish(0), then quit) is unreachable: no −1 request can happen in scene 6.
            return Ok(Some(Ret::Quit));
        }
        log::info!(
            "scene {} -> {} (calc {})",
            cur as i32,
            req as i32,
            ctx.w.frame
        );
        match (cur, req) {
            // Debug start (TH08_STAGE / TH08_REPLAY) from boot; the original always goes 0 → 1.
            (SceneId::Boot, SceneId::Game) => self.start_game(ctx)?,
            // 1 / 2 → 9 (0 → 9: debug `TH08_ENDING`): unregister GM, Ending_Register 0x4296A0 (16 §1.1–§1.2).
            (_, SceneId::Ending) => {
                self.drop_scene(ctx);
                self.set_scene(
                    ctx,
                    Scene::Ending(Box::default()),
                    crate::ending::CALC_PRIO,
                    crate::ending::DRAW_PRIO,
                )?;
            }
            (SceneId::Boot, _) | (_, SceneId::Menu) => {
                if cur == SceneId::MusicRoom {
                    self.capture(ctx);
                }
                self.drop_scene(ctx);
                if matches!(cur, SceneId::Game | SceneId::Result) {
                    crate::replay::finish(ctx);
                }
                crate::replay::end_playback(&mut ctx.w.game);
                self.open_menu(ctx, cur)?;
            }
            (SceneId::Menu, SceneId::Game) => {
                self.drop_scene(ctx);
                self.start_game(ctx)?;
            }
            (SceneId::Game, SceneId::NextStage) => {
                self.drop_scene(ctx);
                ctx.w.game.fresh_start = false;
                self.start_game(ctx)?;
                ctx.w.game.requested = SceneId::Game;
            }
            (SceneId::Game, SceneId::Retry) => self.retry(ctx)?,
            // 2 → 11: requested = 3 is written before the unregister, so the GM sees "next stage", not a fresh start.
            (SceneId::Game, SceneId::Reload) => {
                ctx.w.game.reload_same = true;
                self.drop_scene(ctx);
                ctx.w.game.fresh_start = false;
                self.start_game(ctx)?;
                ctx.w.game.requested = SceneId::Game;
            }
            // 1 → 5 Result_Register(0) viewer; 2 / 9 → 6 Result_Register(1) after a game.
            (_, SceneId::Result | SceneId::ResultMenu) => {
                self.drop_scene(ctx);
                let mode = u8::from(req == SceneId::Result);
                self.set_scene(
                    ctx,
                    Scene::Result(Box::new(crate::result::ResultScreen::new(mode))),
                    crate::result::CALC_PRIO,
                    crate::result::DRAW_PRIO,
                )?;
            }
            (_, SceneId::MusicRoom) => {
                self.drop_scene(ctx);
                self.set_scene(
                    ctx,
                    Scene::MusicRoom(Box::default()),
                    crate::musicroom::CALC_PRIO,
                    crate::musicroom::DRAW_PRIO,
                )?;
            }
            // 2 → 7: like 2 → 1 (also releases managed textures in the original).
            (_, SceneId::ReplayEnd) => {
                self.drop_scene(ctx);
                crate::replay::finish(ctx);
                crate::replay::end_playback(&mut ctx.w.game);
                self.open_menu(ctx, cur)?;
            }
            // 2 → 12 is never requested.
            _ => log::warn!("unhandled scene switch {} -> {}", cur as i32, req as i32),
        }
        // The raw input words are cleared so the new scene sees no held keys.
        // +0x154: ExeCheck of the running exe every 4000 frames; with the 1.00d constants it always passes
        // (15 §2.10), so the port does not count it.
        ctx.w.input.held = 0;
        ctx.w.input.prev = 0;
        ctx.w.game.scene = ctx.w.game.requested;
        Ok(None)
    }

    // Shared tail of x → 1 and 2 → 7: requested := 1, MainMenu_Register, then the loading screen by `cur` (12 §1).
    fn open_menu(&mut self, ctx: &mut Ctx, cur: SceneId) -> Result<()> {
        ctx.w.game.requested = SceneId::Menu;
        self.set_scene(
            ctx,
            Scene::Menu(Box::default()),
            crate::menu::CALC_PRIO,
            crate::menu::DRAW_PRIO,
        )?;
        self.menu_surface(ctx)?;
        self.menu_loading(ctx, cur)
    }

    // 2 → 10: a full exit and a fresh start.
    fn retry(&mut self, ctx: &mut Ctx) -> Result<()> {
        self.drop_scene(ctx);
        crate::replay::finish(ctx);
        let g = &mut ctx.w.game;
        if !g.practice() && g.difficulty < 4 {
            g.stage = 0;
        }
        g.fresh_start = true;
        g.retrying = true;
        g.stats = crate::player::Stats::default();
        self.start_game(ctx)?;
        ctx.w.game.requested = SceneId::Game;
        Ok(())
    }
}

impl Supervisor {
    // Supervisor on-register 0x445FA3 (12 §1); score.dat and the unlock flags are loaded in Ctx::load_with.
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            // ANM slots 0 text.anm and 2 nowloading.anm (00 §4).
            ctx.w.anm.load(&mut ctx.rec, &ctx.archive, 0, "text.anm")?;
            ctx.w
                .anm
                .load(&mut ctx.rec, &ctx.archive, 2, "nowloading.anm")?;
            // Boot loading screen (12 §1): th08logo.jpg full screen + nowloading.anm scripts 0–2 at (500, 440).
            self.begin_loading(ctx)?;
            let st = ctx.core.clone();
            if let Some(spec) = &st.anm {
                crate::anmdebug::register(ctx, spec)?;
                return Ok(());
            }
            crate::ascii::AsciiManager::register(ctx)?;
            // Debug `TH08_REPLAY=<file | demoN>`: starts that replay; `TH08_REPLAY_SPEED=<0–2>`.
            if let Some(v) = &st.replay {
                let bytes = match v.strip_prefix("demo") {
                    Some(n) => ctx.archive.read(&format!("demorpy{n}.rpy"))?,
                    None => ctx
                        .host
                        .read_path(v)
                        .with_context(|| format!("TH08_REPLAY={v}"))?,
                };
                let r = crate::replay::parse(&bytes, ctx.w.game.ver_list.get())?;
                ctx.w.game.replay_speed = st.replay_speed;
                let demo = v.strip_prefix("demo").and_then(|n| n.parse().ok());
                crate::replay::start_playback(&mut ctx.w.game, r, None, demo);
            } else if let Some(e) = &st.ending {
                setup_ending(&mut ctx.w.game, &st, e);
            } else if let Some(index) = st.stage {
                setup_new_game(&mut ctx.w.game, &st, index);
            } else {
                ctx.w.game.request(SceneId::Menu);
            }
        }
        Ok(())
    }

    // Supervisor_Calc 0x445453: input poll (OR-accumulated on a slow-mode skipped frame), loading VMs, then
    // the scene switch.
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        // AnmManager reset 0x44548A on every pass, also each restarted fast-forward pass: tint off
        // (06 §10.2), shake offset 0 (13 §3), texture caches (05 §12.5); the blend cache models D3D state, kept.
        ctx.w.anm.tint_off();
        ctx.w.anm.screen_offset = [0.0; 2];
        ctx.rec.anm.reset();
        ctx.w.bgm.tick_fade();
        if let Some(s) = &self.script {
            ctx.w.input.raw = s.get(self.frame).copied().unwrap_or(0);
        } else {
            // Input_Poll: one host poll per calc pass, also per restarted pass (00 §3).
            let p = ctx.host.poll_input();
            ctx.w.input.poll(&ctx.w.game.cfg, &p);
        }
        self.frame += 1;
        if ctx.w.game.slow_skip != 0 {
            ctx.w.input.accumulate();
        } else {
            ctx.w.input.tick();
        }
        self.replay_fps(ctx);
        // Loading_Begin from the result screen (16 §2.1 state 2): requested in its calc (prio 16), so the VMs
        // catch up the frame they missed.
        // Surface 8 stays empty until the result object's removal moves result.jpg into it.
        if std::mem::take(&mut ctx.w.game.loading_req) {
            if let Some(t) = self.logo.take() {
                ctx.rec.free_texture(t);
            }
            self.start_vms(ctx, [500.0, 440.0]);
            for vm in &mut self.boot_vms {
                vm.run(&mut ctx.w.anm, &mut ctx.w.rng);
            }
        }
        if std::mem::take(&mut self.capture_req) {
            self.capture(ctx);
        }
        if self.stage_load {
            if ctx.w.game.loading == 0 {
                // Loading_End: state 0, surface 8 released.
                self.stage_load = false;
                self.boot_vms.clear();
                if let Some(t) = self.logo.take() {
                    ctx.rec.free_texture(t);
                }
            } else if ctx.w.game.loading_wait == 1 {
                for vm in &mut self.boot_vms {
                    vm.pending_interrupt = 2;
                }
            }
        }
        // Session tool hook: the session trace supplies when the async menu loader finishes (12 §1 has no fixed time).
        // Its tail ran before this calc, so its interrupt reaches this frame's VM run.
        if ctx.w.game.menu_loading > 0
            && let Some(s) = &mut ctx.w.game.sched
            && s.menu.front().is_some_and(|&p| s.poll >= p)
        {
            s.menu.pop_front();
            ctx.w.game.menu_loading = 0;
            self.end_loading(ctx);
        }
        // Presents 1–2 come from Window_InitD3D, before the first Frame_Run (12 §1 boot presents).
        if self.frame > 2 || ctx.w.game.requested != SceneId::Menu {
            for vm in &mut self.boot_vms {
                vm.run(&mut ctx.w.anm, &mut ctx.w.rng);
            }
        }
        if self.logo.is_some() && ctx.w.game.scene == SceneId::Boot {
            self.boot_frames += 1;
            let boot = ctx
                .w
                .game
                .sched
                .as_ref()
                .map_or(BOOT_FRAMES, |s| s.boot_calc);
            if ctx.w.game.requested == SceneId::Menu && self.boot_frames < boot {
                return Ret::Next;
            }
            if ctx.w.game.requested != SceneId::Menu {
                self.end_loading(ctx);
                self.boot_vms.clear();
            }
        }
        let r = self.switch(ctx);
        // 0x18B8A2C decrements once per frame while ≠ 0 (13 §3).
        let n = ctx.w.load_frames;
        if n != 0 {
            ctx.w.load_frames = n - 1;
        }
        if ctx.w.game.menu_loading > 0 {
            let g = &mut ctx.w.game;
            match &mut g.sched {
                Some(s) if s.menu.front().is_some_and(|&p| s.poll >= p) => {
                    s.menu.pop_front();
                    g.menu_loading = 0;
                }
                Some(s) if !s.menu.is_empty() => {}
                _ => g.menu_loading -= 1,
            }
            if g.menu_loading == 0 {
                self.end_loading(ctx);
            }
        }
        match r {
            Ok(Some(r)) => r,
            Ok(None) => Ret::Next,
            Err(e) => {
                log::error!("scene switch failed: {e:#}");
                Ret::ErrorQuit
            }
        }
    }

    // Draw prio 0 `Supervisor_DrawLoading` 0x445BD4 (surface 8), prio 2 loading VMs 0x445D3E (12 §1); prio 16 debug only.
    // Step: tag 0 the "Press Shot Button" counter, queue colour and drain (12 §1); tag 16 the replay fps string
    // (15 §3.3). Tag 2 and the slot test write nothing.
    fn draw_step(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        match tag {
            0 => {
                self.prompt =
                    crate::ascii::loading_prompt(&mut ctx.w.ascii, &mut ctx.w.game.load_prompt);
            }
            16 => self.replay_fps_draw(ctx),
            _ => {}
        }
        Ret::Next
    }

    // TH08_SLOT_TEST=<slot>: draws that slot's sprites in a grid, clipped to the playfield.
    fn emit(&self, ctx: &View, rec: &mut Recorder, tag: u8) {
        match tag {
            0 => {
                if let Some(t) = self.logo {
                    rec.set_viewport(crate::drawlist::FULLSCREEN);
                    rec.image(t, 0.0, 0.0);
                }
                // Then the "Press Shot Button" prompt with the whole queue (12 §1).
                crate::ascii::emit_texts(&ctx.w.anm, rec, &self.prompt);
            }
            2 => {
                // The scripts' own positions (slide-in, z) are relative to the loading position.
                // 0x445D3E: pos += pos_offset, Draw2DRot, pos −= pos_offset (12 §1): a shifted copy, no net write.
                for vm in &self.boot_vms {
                    let (p, o) = (vm.pos, vm.pos_offset);
                    let mut v = vm.clone();
                    v.pos = [
                        p[0] + o[0] + self.load_pos[0],
                        p[1] + o[1] + self.load_pos[1],
                        p[2] + o[2],
                    ];
                    ctx.w.anm.emit_mode(rec, &v, 2);
                }
            }
            _ => {}
        }
        let (Some(slot), 16) = (self.debug_sprites, tag) else {
            return;
        };
        let Some(anm) = ctx.w.anm.get(slot) else {
            return;
        };
        rec.set_viewport(PLAYFIELD);
        rec.clear_viewport([0, 0, 64, 255]);
        let (mut x, mut y, mut row) = (0.0, 0.0, 0.0f32);
        for s in &anm.sprites {
            if x + s.w > 640.0 {
                (x, y, row) = (0.0, y + row, 0.0);
            }
            rec.sprite(
                s.tex,
                Blend::Alpha,
                Rect::new(x, y, s.w, s.h),
                s.uv,
                [255; 4],
            );
            x += s.w;
            row = row.max(s.h);
        }
        rec.set_viewport(crate::drawlist::FULLSCREEN);
    }
}
