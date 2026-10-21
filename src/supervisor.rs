//! Supervisor g_Supervisor 0x17CE758 (11 §7, 12 §1): scene switches, input poll, boot / loading screens; calc prio 0,
//! draw prios 0 / 2 / 16 (00 §4).

use crate::anmvm::AnmVm;
use crate::game::Ctx;
use crate::gamemgr::{self, Game, GameManager};
use crate::gfx::{Blend, PLAYFIELD, Rect, TextureId};
use crate::settings::{Ending, Settings};
use crate::task::{self, Chain, Ret, Task, TaskRef};
use anyhow::{Context, Result};

pub struct Supervisor {
    // Debug `TH08_SLOT_TEST` / `TH08_INPUT`.
    debug_sprites: Option<usize>,
    script: Option<Vec<u16>>,
    frame: usize,
    // The registered scene object (GM, MainMenu, Result, Ending, MusicRoom).
    scene_task: Option<TaskRef<Ctx>>,
    // Full-screen surface slot 8 (logo, captured frame or result.jpg), drawn at prio 0 while loading (12 §1).
    logo: Option<TextureId>,
    // The three nowloading.anm VMs (scripts 0–2) and their screen position (12 §1 state 1).
    boot_vms: Vec<AnmVm>,
    boot_frames: u32,
    load_pos: [f32; 2],
    // Loading state of a stage start: Loading_Wait / Loading_End follow the GM loader (12 §1).
    stage_load: bool,
    // Pending post-present capture into surface 8 (0x44886F, 12 §1).
    capture_req: bool,
    presents: u32,
    // Playback fps text buffer 0x17CF2B8 (15 §3.3) and the port's refresh counter.
    fps_text: String,
    fps_frames: u32,
}

// The logo stays up while the loader threads run (12 §1: no fixed time).
// SPEC-GAP: the holds were measured on the original's draw log: 13 boot frames, then the menu loader until scene
// frame 34 (33 when the menu is entered again).
const BOOT_FRAMES: u32 = 14;
const MENU_LOAD_BOOT: u32 = 34;
const MENU_LOAD: u32 = 33;

pub fn register(ctx: &mut Ctx) -> Result<()> {
    let t = task::task(Supervisor {
        debug_sprites: ctx.settings.slot_test,
        script: ctx
            .settings
            .input
            .as_deref()
            .and_then(crate::input::load_script),
        frame: 0,
        scene_task: None,
        logo: None,
        boot_vms: Vec::new(),
        boot_frames: 0,
        load_pos: [500.0, 440.0],
        stage_load: false,
        capture_req: false,
        presents: 0,
        fps_text: String::new(),
        fps_frames: 0,
    });
    task::add(ctx, Chain::Calc, 0, 0, t.clone())?;
    for prio in [0, 2, 16] {
        task::add(ctx, Chain::Draw, prio, prio as u8, t.clone())?;
    }
    Ok(())
}

// Debug tool, not a port. Debug start (`TH08_STAGE`): difficulty `TH08_RANK` (default Normal), shot `TH08_PLAYER` (default 0).
pub fn setup_new_game(g: &mut Game, s: &Settings, stage: usize) {
    let shot = s.player.unwrap_or(0);
    let default_diff = if stage % 9 == 8 { 4 } else { 1 };
    g.difficulty = s.rank.unwrap_or(default_diff);
    g.shot = shot;
    g.stage = stage;
    g.flags = 0;
    g.fresh_start = true;
    g.request(gamemgr::SCENE_GAME);
}

// Debug tool, not a port. `TH08_ENDING=<shot 0–11><a|b|c>` (bad / good 6A / good 6B, e.g. `0b`, `7a`) or a
// `.end` file name (played as a good 6A ending of `TH08_PLAYER`); difficulty `TH08_RANK`, continues `TH08_CONTINUES`.
pub fn setup_ending(g: &mut Game, s: &Settings, e: &Ending) {
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
    g.request(gamemgr::SCENE_ENDING);
}

impl Supervisor {
    // Fps_Update 0x446F53 playback text (15 §3.3): "%2d" of fps_now at (384, 448), yellow or red with the flag.
    // The original refreshes on a host-timed 500 ms window, which 15 §3.3 says cannot be reproduced.
    // SPEC-GAP: fixed rule here, every 30 gameplay calcs on the 3rd of each (demo 0 / 2 phase, from traces).
    fn replay_fps(&mut self, ctx: &mut Ctx) {
        let g = &ctx.game;
        if g.loading != 0 || g.scene != gamemgr::SCENE_GAME {
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
        let c = if g.replay_fps_red {
            0xFFFF4040
        } else {
            0xFFFFFFD0
        };
        ctx.ascii
            .push(crate::ascii::Text::new(384.0, 448.0, self.fps_text.clone()).argb(c));
    }

    // Loading_Begin 0x448790 (12 §1): surface 8 (the logo) full screen + nowloading.anm scripts 0–2 at (500, 440).
    fn begin_loading(&mut self, ctx: &mut Ctx) -> Result<()> {
        if self.logo.is_none() {
            let b = ctx.archive.read("th08logo.jpg")?;
            self.logo = Some(ctx.gfx.load_image("th08logo.jpg", &b)?);
        }
        self.start_vms(ctx, [500.0, 440.0]);
        Ok(())
    }

    fn start_vms(&mut self, ctx: &mut Ctx, pos: [f32; 2]) {
        self.load_pos = pos;
        self.boot_vms = (0..3)
            .map(|i| {
                let mut vm = AnmVm::default();
                ctx.anm.set_script(&mut vm, 2, i);
                vm
            })
            .collect();
    }

    // Surface 8 := the last presented frame (post-present capture of 0x44886F).
    fn capture(&mut self, ctx: &mut Ctx) {
        if let Some(t) = self.logo.take() {
            ctx.gfx.free_texture(t);
        }
        self.logo = Some(ctx.gfx.capture_screen("th08logo.jpg"));
    }

    // Menu loader done: title fade-from-white (ScreenEffect type 0, 70 frames, 13 §3), Loading_End 0x44881A (interrupt 1
    // fades the VMs out, surface released, 12 §1).
    fn end_loading(&mut self, ctx: &mut Ctx) {
        if let Some(t) = self.logo.take() {
            ctx.gfx.free_texture(t);
        }
        crate::effect::screen(0, 70, 0xFFFFFF, 0, 0, 0x15);
        for vm in &mut self.boot_vms {
            vm.pending_interrupt = 1;
        }
    }

    // Main-menu loading screen by previous scene (12 §1); returns the port's hold length (see BOOT_FRAMES).
    fn menu_loading(&mut self, ctx: &mut Ctx, from: i32) -> Result<()> {
        ctx.game.menu_loading = if from == gamemgr::SCENE_BOOT {
            MENU_LOAD_BOOT
        } else if matches!(from, gamemgr::SCENE_RESULT | gamemgr::SCENE_RESULT_MENU) {
            // Loading_Begin already ran at result state 2 frame 1; Result_OnRemove moved result.jpg into surface 8,
            // so the menu loader shows it without a capture (16 §2.4 exit).
            if let Some(t) = self.logo.take() {
                ctx.gfx.free_texture(t);
            }
            // Labelled by its surface slot's file name, as the draw log names surface 8.
            let b = ctx.archive.read("result.jpg")?;
            self.logo = Some(ctx.gfx.load_image("th08logo.jpg", &b)?);
            MENU_LOAD
        } else if from == gamemgr::SCENE_MUSIC_ROOM {
            // The music room calls 0x44886F itself on the exit press, one frame before the scene switch.
            self.begin_loading(ctx)?;
            for vm in &mut self.boot_vms {
                vm.run(&mut ctx.anm);
            }
            MENU_LOAD - 1
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
            task::add(ctx, Chain::Draw, p, p as u8, t.clone())?;
        }
        Ok(())
    }

    fn drop_scene(&mut self, ctx: &mut Ctx) {
        if let Some(t) = self.scene_task.take() {
            task::remove_task(ctx, &t);
        }
    }

    // GameMgr_OnRegister 0x43AAF4: 0x44886F at (500, 440) + darken overlay (ScreenEffect type 5) after the main menu,
    // else at (280, 430) (12 §1).
    fn start_game(&mut self, ctx: &mut Ctx) -> Result<()> {
        let gm = GameManager::register(ctx)?;
        self.scene_task = Some(gm);
        // Attract demos always start from the main menu.
        let from_menu =
            ctx.game.previous == gamemgr::SCENE_MENU || ctx.game.flags & gamemgr::F_DEMO != 0;
        if let Some(t) = self.logo.take() {
            ctx.gfx.free_texture(t);
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

    fn set_scene<T: Task<Ctx> + 'static>(
        &mut self,
        ctx: &mut Ctx,
        t: T,
        calc: u16,
        draw: u16,
    ) -> Result<()> {
        let t = task::task(t);
        task::add(ctx, Chain::Calc, calc, 0, t.clone())?;
        task::add(ctx, Chain::Draw, draw, 0, t.clone())?;
        self.scene_task = Some(t);
        Ok(())
    }

    // Supervisor_Calc 0x445453 scene switch (11 §7 table): `previous` (+0x160) := current, then by (current, requested).
    fn switch(&mut self, ctx: &mut Ctx) -> Result<Option<Ret>> {
        let g = &mut ctx.game;
        let (cur, req) = (g.scene, g.requested);
        if cur == req {
            return Ok(None);
        }
        g.previous = cur;
        if req < 0 {
            // The score.dat flush comes after this frame's draw and Present (`Ctx::exit_save`, 15 §2.2).
            // SPEC-GAP: 6 → −1 also finishes the replay in 11 §7; the port does not.
            return Ok(Some(Ret::Quit));
        }
        log::info!("scene {cur} -> {req} (calc {})", ctx.frame);
        match (cur, req) {
            // Debug start (TH08_STAGE / TH08_REPLAY) from boot; the original always goes 0 → 1.
            (gamemgr::SCENE_BOOT, gamemgr::SCENE_GAME) => self.start_game(ctx)?,
            // 1 / 2 → 9 (0 → 9: debug `TH08_ENDING`): unregister GM, Ending_Register 0x4296A0 (16 §1.1–§1.2).
            (_, gamemgr::SCENE_ENDING) => {
                self.drop_scene(ctx);
                self.set_scene(
                    ctx,
                    crate::ending::Ending::new(),
                    crate::ending::CALC_PRIO,
                    crate::ending::DRAW_PRIO,
                )?;
            }
            (gamemgr::SCENE_BOOT, _) | (_, gamemgr::SCENE_MENU) => {
                if cur == gamemgr::SCENE_MUSIC_ROOM {
                    self.capture(ctx);
                }
                self.drop_scene(ctx);
                if matches!(cur, gamemgr::SCENE_GAME | gamemgr::SCENE_RESULT) {
                    crate::replay::finish(ctx);
                }
                crate::replay::end_playback(&mut ctx.game);
                ctx.game.requested = gamemgr::SCENE_MENU;
                self.set_scene(
                    ctx,
                    crate::menu::MainMenu::new(),
                    crate::menu::CALC_PRIO,
                    crate::menu::DRAW_PRIO,
                )?;
                self.menu_surface(ctx)?;
                self.menu_loading(ctx, cur)?;
            }
            (gamemgr::SCENE_MENU, gamemgr::SCENE_GAME) => {
                self.drop_scene(ctx);
                self.start_game(ctx)?;
            }
            (gamemgr::SCENE_GAME, gamemgr::SCENE_NEXT_STAGE) => {
                self.drop_scene(ctx);
                ctx.game.fresh_start = false;
                self.start_game(ctx)?;
                ctx.game.requested = gamemgr::SCENE_GAME;
            }
            // 2 → 10: a full exit and a fresh start (11 §7).
            (gamemgr::SCENE_GAME, gamemgr::SCENE_RETRY) => {
                self.drop_scene(ctx);
                crate::replay::finish(ctx);
                let g = &mut ctx.game;
                if !g.practice() && g.difficulty < 4 {
                    g.stage = 0;
                }
                g.fresh_start = true;
                g.retrying = true;
                g.stats = Default::default();
                self.start_game(ctx)?;
                ctx.game.requested = gamemgr::SCENE_GAME;
            }
            // 2 → 11: requested = 3 is written before the unregister, so the GM sees "next stage", not a fresh start (11 §7).
            (gamemgr::SCENE_GAME, gamemgr::SCENE_RELOAD) => {
                ctx.game.reload_same = true;
                self.drop_scene(ctx);
                ctx.game.fresh_start = false;
                self.start_game(ctx)?;
                ctx.game.requested = gamemgr::SCENE_GAME;
            }
            // 1 → 5 Result_Register(0) viewer; 2 / 9 → 6 Result_Register(1) after a game.
            (_, gamemgr::SCENE_RESULT | gamemgr::SCENE_RESULT_MENU) => {
                self.drop_scene(ctx);
                let mode = (req == gamemgr::SCENE_RESULT) as u8;
                self.set_scene(
                    ctx,
                    crate::result::ResultScreen::new(mode),
                    crate::result::CALC_PRIO,
                    crate::result::DRAW_PRIO,
                )?;
            }
            (_, gamemgr::SCENE_MUSIC_ROOM) => {
                self.drop_scene(ctx);
                self.set_scene(
                    ctx,
                    crate::musicroom::MusicRoom::new(),
                    crate::musicroom::CALC_PRIO,
                    crate::musicroom::DRAW_PRIO,
                )?;
            }
            // 2 → 7: like 2 → 1 (also releases managed textures in the original).
            (_, gamemgr::SCENE_REPLAY_END) => {
                self.drop_scene(ctx);
                crate::replay::finish(ctx);
                crate::replay::end_playback(&mut ctx.game);
                ctx.game.requested = gamemgr::SCENE_MENU;
                self.set_scene(
                    ctx,
                    crate::menu::MainMenu::new(),
                    crate::menu::CALC_PRIO,
                    crate::menu::DRAW_PRIO,
                )?;
                self.menu_surface(ctx)?;
                self.menu_loading(ctx, cur)?;
            }
            // 2 → 12 is never requested (11 §7).
            _ => log::warn!("unhandled scene switch {cur} -> {req}"),
        }
        // The raw input words are cleared so the new scene sees no held keys (11 §7).
        // SPEC-GAP: the +0x154 frame counter (config re-check every 4000 frames) is not kept.
        ctx.input.held = 0;
        ctx.input.prev = 0;
        ctx.game.scene = ctx.game.requested;
        Ok(None)
    }
}

impl Task<Ctx> for Supervisor {
    // Supervisor on-register 0x445FA3 (12 §1); score.dat and the unlock flags are loaded in Ctx::load_with.
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            // ANM slots 0 text.anm and 2 nowloading.anm (00 §4).
            ctx.anm.load(&mut ctx.gfx, &ctx.archive, 0, "text.anm")?;
            ctx.anm
                .load(&mut ctx.gfx, &ctx.archive, 2, "nowloading.anm")?;
            // Boot loading screen (12 §1): th08logo.jpg full screen + nowloading.anm scripts 0–2 at (500, 440).
            self.begin_loading(ctx)?;
            let st = ctx.settings;
            if let Some(spec) = &st.anm {
                crate::anmdebug::register(ctx, spec)?;
                return Ok(());
            }
            crate::ascii::AsciiManager::register(ctx)?;
            // Debug `TH08_REPLAY=<file | demoN>`: starts that replay; `TH08_REPLAY_SPEED=<0–2>`.
            if let Some(v) = &st.replay {
                let bytes = match v.strip_prefix("demo") {
                    Some(n) => ctx.archive.read(&format!("demorpy{n}.rpy"))?,
                    None => load_file(v).with_context(|| format!("TH08_REPLAY={v}"))?,
                };
                let r = crate::replay::Replay::parse(&bytes)?;
                ctx.game.replay_speed = st.replay_speed;
                let demo = v.strip_prefix("demo").and_then(|n| n.parse().ok());
                crate::replay::start_playback(&mut ctx.game, r, None, demo);
            } else if let Some(e) = &st.ending {
                setup_ending(&mut ctx.game, st, e);
            } else if let Some(index) = st.stage {
                setup_new_game(&mut ctx.game, st, index);
            } else {
                ctx.game.request(gamemgr::SCENE_MENU);
            }
        }
        Ok(())
    }

    // Supervisor_Calc 0x445453 (11 §7): input poll, loading VMs, then the scene switch.
    // SPEC-GAP: the slow-mode OR-accumulation of input (gm+0x2D) is not done.
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        ctx.audio.tick_fade();
        if let Some(s) = &self.script {
            ctx.input.raw = s.get(self.frame).copied().unwrap_or(0);
        }
        self.frame += 1;
        ctx.input.tick();
        self.replay_fps(ctx);
        // Loading_Begin from the result screen (16 §2.1 state 2): requested in its calc (prio 16), so the VMs
        // catch up the frame they missed.
        // Surface 8 stays empty until the result object's removal moves result.jpg into it.
        if std::mem::take(&mut ctx.game.loading_req) {
            if let Some(t) = self.logo.take() {
                ctx.gfx.free_texture(t);
            }
            self.start_vms(ctx, [500.0, 440.0]);
            for vm in &mut self.boot_vms {
                vm.run(&mut ctx.anm);
            }
        }
        if std::mem::take(&mut self.capture_req) {
            self.capture(ctx);
        }
        if self.stage_load {
            if ctx.game.loading == 0 {
                // Loading_End: state 0, surface 8 released.
                self.stage_load = false;
                self.boot_vms.clear();
                if let Some(t) = self.logo.take() {
                    ctx.gfx.free_texture(t);
                }
            } else if ctx.game.loading_wait == 1 {
                for vm in &mut self.boot_vms {
                    vm.pending_interrupt = 2;
                }
            }
        }
        // Session tool hook: the session trace supplies when the async menu loader finishes (12 §1 has no fixed time).
        // Its tail ran before this calc, so its interrupt reaches this frame's VM run.
        if ctx.game.menu_loading > 0
            && let Some(s) = &mut ctx.game.sched
            && s.menu.front().is_some_and(|&p| s.poll >= p)
        {
            s.menu.pop_front();
            ctx.game.menu_loading = 0;
            self.end_loading(ctx);
        }
        // Presents 1–2 come from Window_InitD3D, before the first Frame_Run (12 §1 boot presents).
        if self.frame > 2 || ctx.game.requested != gamemgr::SCENE_MENU {
            for vm in &mut self.boot_vms {
                vm.run(&mut ctx.anm);
            }
        }
        if self.logo.is_some() && ctx.game.scene == gamemgr::SCENE_BOOT {
            self.boot_frames += 1;
            let boot = ctx.game.sched.as_ref().map_or(BOOT_FRAMES, |s| s.boot_calc);
            if ctx.game.requested == gamemgr::SCENE_MENU && self.boot_frames < boot {
                return Ret::Next;
            }
            if ctx.game.requested != gamemgr::SCENE_MENU {
                self.end_loading(ctx);
                self.boot_vms.clear();
            }
        }
        let r = self.switch(ctx);
        // 0x18B8A2C decrements once per frame while ≠ 0 (13 §3, 11 §7).
        let n = crate::enemy::load_frames();
        if n != 0 {
            crate::enemy::set_load_frames(n - 1);
        }
        if ctx.game.menu_loading > 0 {
            let g = &mut ctx.game;
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
    // TH08_SLOT_TEST=<slot>: draws that slot's sprites in a grid, clipped to the playfield.
    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        // Boot: presents 1–2 clear to black, presents 3–5 draw nothing (loader thread start-up, 12 §1).
        if tag == 0 && self.presents < 5 && ctx.game.scene == gamemgr::SCENE_BOOT {
            self.presents += 1;
            if self.presents <= 2 {
                ctx.gfx.set_viewport(crate::gfx::FULLSCREEN);
                ctx.gfx.clear_depth(Some([0, 0, 0, 255]));
            }
            return Ret::StopFrame;
        }
        match (tag, self.logo) {
            (0, Some(t)) => {
                ctx.gfx.set_viewport(crate::gfx::FULLSCREEN);
                ctx.gfx.image(t, 0.0, 0.0);
            }
            (2, _) => {
                // The scripts' own positions (slide-in, z) are relative to the loading position.
                for vm in &mut self.boot_vms {
                    let p = vm.pos;
                    let o = vm.pos_offset;
                    vm.pos = [
                        p[0] + o[0] + self.load_pos[0],
                        p[1] + o[1] + self.load_pos[1],
                        p[2] + o[2],
                    ];
                    // 0x445D3E: pos += pos_offset, Draw2DRot, pos −= pos_offset (12 §1).
                    ctx.anm.draw_mode(&mut ctx.gfx, vm, 2);
                    vm.pos = p;
                }
            }
            _ => {}
        }
        let (Some(slot), 16) = (self.debug_sprites, tag) else {
            return Ret::Next;
        };
        let Some(anm) = ctx.anm.get(slot) else {
            return Ret::Next;
        };
        let sprites = anm.sprites.clone();
        ctx.gfx.set_viewport(PLAYFIELD);
        ctx.gfx.clear_viewport([0, 0, 64, 255]);
        let (mut x, mut y, mut row) = (0.0, 0.0, 0.0f32);
        for s in sprites {
            if x + s.w > 640.0 {
                (x, y, row) = (0.0, y + row, 0.0);
            }
            ctx.gfx.sprite(
                s.tex,
                Blend::Alpha,
                Rect::new(x, y, s.w, s.h),
                s.uv,
                [255; 4],
            );
            x += s.w;
            row = row.max(s.h);
        }
        ctx.gfx.set_viewport(crate::gfx::FULLSCREEN);
        Ret::Next
    }
}

// Debug: a file from disk natively, or from the save storage (web, and `replay/…` names).
fn load_file(path: &str) -> Option<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(b) = std::fs::read(path) {
        return Some(b);
    }
    crate::scoredat::storage_read(path)
}

// Today's "%m/%d" in local time for HSCR +0x20 / replay info +0x6C (15 §2.3, §3.5; 16 §2.2: C `localtime`).
pub fn date_md() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    return chrono::Local::now().format("%m/%d").to_string();
    #[cfg(target_arch = "wasm32")]
    {
        // The JS Date getters are local time.
        let d = js_sys::Date::new_0();
        format!("{:02}/{:02}", d.get_month() + 1, d.get_date())
    }
}
