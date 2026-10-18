use crate::anmvm::AnmVm;
use crate::game::Ctx;
use crate::gamemgr::{self, Game, GameManager};
use crate::gfx::{Blend, PLAYFIELD, Rect, TextureId};
use crate::task::{self, Chain, Ret, Task, TaskRef};
use anyhow::{Context, Result};

pub struct Supervisor {
    debug_sprites: Option<usize>,
    script: Option<Vec<u16>>,
    frame: usize,
    scene_task: Option<TaskRef<Ctx>>,
    logo: Option<TextureId>,
    boot_vms: Vec<AnmVm>,
    boot_frames: u32,
}

// The logo stays up while the loader runs (answers_impl9 §18: no fixed time); loading is instant here, so a short hold.
const BOOT_FRAMES: u32 = 60;

pub fn register(ctx: &mut Ctx) -> Result<()> {
    let t = task::task(Supervisor {
        debug_sprites: crate::env::var("TH08_SLOT_TEST").and_then(|v| v.parse().ok()),
        script: crate::env::var("TH08_INPUT").and_then(|v| crate::input::load_script(&v)),
        frame: 0,
        scene_task: None,
        logo: None,
        boot_vms: Vec::new(),
        boot_frames: 0,
    });
    task::add(ctx, Chain::Calc, 0, 0, t.clone())?;
    for prio in [0, 2, 16] {
        task::add(ctx, Chain::Draw, prio, prio as u8, t.clone())?;
    }
    Ok(())
}

// Debug start (`TH08_STAGE`): difficulty `TH08_RANK` (default Normal), shot `TH08_PLAYER` (default 0).
pub fn setup_new_game(g: &mut Game, stage: usize) -> Result<()> {
    let shot = match crate::env::var("TH08_PLAYER") {
        Some(v) => crate::player::parse_shot(&v)
            .with_context(|| format!("TH08_PLAYER={v}: unknown shot type"))?,
        None => 0,
    };
    let default_diff = if stage % 9 == 8 { 4 } else { 1 };
    g.difficulty = crate::env::var("TH08_RANK")
        .and_then(|s| crate::enemy::parse_difficulty(&s))
        .unwrap_or(default_diff);
    g.shot = shot;
    g.stage = stage;
    g.flags = 0;
    g.fresh_start = true;
    g.request(gamemgr::SCENE_GAME);
    Ok(())
}

impl Supervisor {
    fn drop_scene(&mut self, ctx: &mut Ctx) {
        if let Some(t) = self.scene_task.take() {
            task::remove_task(ctx, &t);
        }
    }

    fn start_game(&mut self, ctx: &mut Ctx) -> Result<()> {
        let gm = GameManager::register(ctx)?;
        self.scene_task = Some(gm);
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

    // Supervisor_Calc scene switch (11 §7).
    fn switch(&mut self, ctx: &mut Ctx) -> Result<Option<Ret>> {
        let g = &mut ctx.game;
        let (cur, req) = (g.scene, g.requested);
        if cur == req {
            return Ok(None);
        }
        g.previous = cur;
        if req < 0 {
            // WinMain exit flushes score.dat (Result mode 2).
            ctx.game.save_score(&mut ctx.anm.rng, true);
            return Ok(Some(Ret::Quit));
        }
        log::info!("scene {cur} -> {req}");
        match (cur, req) {
            (gamemgr::SCENE_BOOT, gamemgr::SCENE_GAME) => self.start_game(ctx)?,
            (gamemgr::SCENE_BOOT, _) | (_, gamemgr::SCENE_MENU) => {
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
            (gamemgr::SCENE_GAME, gamemgr::SCENE_RELOAD) => {
                ctx.game.reload_same = true;
                self.drop_scene(ctx);
                ctx.game.fresh_start = false;
                self.start_game(ctx)?;
                ctx.game.requested = gamemgr::SCENE_GAME;
            }
            (
                _,
                gamemgr::SCENE_RESULT
                | gamemgr::SCENE_ENDING
                | gamemgr::SCENE_RESULT_MENU
                | gamemgr::SCENE_MUSIC_ROOM,
            ) => {
                self.drop_scene(ctx);
                self.set_scene(
                    ctx,
                    Stub {
                        kind: req,
                        frame: 0,
                    },
                    4,
                    3,
                )?;
            }
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
            }
            _ => log::warn!("unhandled scene switch {cur} -> {req}"),
        }
        ctx.input.held = 0;
        ctx.input.prev = 0;
        ctx.game.scene = ctx.game.requested;
        Ok(None)
    }
}

impl Task<Ctx> for Supervisor {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            ctx.anm.load(&mut ctx.gfx, &ctx.archive, 0, "text.anm")?;
            ctx.anm
                .load(&mut ctx.gfx, &ctx.archive, 2, "nowloading.anm")?;
            // Boot loading screen (12 §1): th08logo.jpg full screen + nowloading.anm scripts 0–2 at (500, 440).
            self.logo = Some(ctx.gfx.load_image(&ctx.archive.read("th08logo.jpg")?)?);
            self.boot_vms = (0..3)
                .map(|i| {
                    let mut vm = AnmVm::default();
                    ctx.anm.set_script(&mut vm, 2, i);
                    vm.pos = [500.0, 440.0, 0.0];
                    vm
                })
                .collect();
            if let Some(spec) = crate::env::var("TH08_ANM") {
                crate::anmdebug::register(ctx, &spec)?;
                return Ok(());
            }
            crate::ascii::AsciiManager::register(ctx)?;
            // Debug `TH08_REPLAY=<file | demoN>`: starts that replay; `TH08_REPLAY_SPEED=<0–2>`.
            if let Some(v) = crate::env::var("TH08_REPLAY") {
                let bytes = match v.strip_prefix("demo") {
                    Some(n) => ctx.archive.read(&format!("demorpy{n}.rpy"))?,
                    None => load_file(&v).with_context(|| format!("TH08_REPLAY={v}"))?,
                };
                let r = crate::replay::Replay::parse(&bytes)?;
                ctx.game.replay_speed = crate::env::var("TH08_REPLAY_SPEED")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let demo = v.strip_prefix("demo").and_then(|n| n.parse().ok());
                crate::replay::start_playback(&mut ctx.game, r, None, demo);
            } else if let Some(arg) = crate::env::var("TH08_STAGE") {
                let index = crate::stage::parse_stage_arg(&arg)
                    .with_context(|| format!("TH08_STAGE={arg}: unknown stage"))?;
                setup_new_game(&mut ctx.game, index)?;
            } else {
                ctx.game.request(gamemgr::SCENE_MENU);
            }
        }
        Ok(())
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        if let Some(s) = &self.script {
            ctx.input.raw = s.get(self.frame).copied().unwrap_or(0);
        }
        self.frame += 1;
        ctx.input.tick();
        if self.logo.is_some() {
            self.boot_frames += 1;
            for vm in &mut self.boot_vms {
                vm.run(&mut ctx.anm);
            }
            if ctx.game.requested == gamemgr::SCENE_MENU && self.boot_frames < BOOT_FRAMES {
                return Ret::Next;
            }
            if let Some(t) = self.logo.take() {
                ctx.gfx.free_texture(t);
                crate::effect::screen(0, 70, 0xFFFFFF, 0, 0, 0x15);
            }
            self.boot_vms.clear();
        }
        match self.switch(ctx) {
            Ok(Some(r)) => r,
            Ok(None) => Ret::Next,
            Err(e) => {
                log::error!("scene switch failed: {e:#}");
                Ret::ErrorQuit
            }
        }
    }

    // TH08_SLOT_TEST=<slot>: draws that slot's sprites in a grid, clipped to the playfield.
    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        match (tag, self.logo) {
            (0, Some(t)) => {
                ctx.gfx.set_viewport(crate::gfx::FULLSCREEN);
                ctx.gfx.image(t, 0.0, 0.0);
            }
            (2, Some(_)) => {
                for vm in &mut self.boot_vms {
                    ctx.anm.draw(&mut ctx.gfx, vm);
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

// SPEC-GAP: result (scenes 5/6), music room (8) and ending (9) are later tasks; this stub shows the outcome and
// returns on Shot/Enter (ending → result → menu, like the scene table).
pub struct Stub {
    kind: i32,
    frame: u32,
}

// A file from disk natively, or from the save storage (web, and `replay/…` names).
fn load_file(path: &str) -> Option<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(b) = std::fs::read(path) {
        return Some(b);
    }
    crate::scoredat::storage_read(path)
}

// Today's "%m/%d" (UTC; the original uses local time).
pub fn date_md() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    #[cfg(target_arch = "wasm32")]
    let secs = (js_sys::Date::now() / 1000.0) as i64;
    let z = secs.div_euclid(86400) + 719468;
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{m:02}/{d:02}")
}

// Result records after a game. SPEC-GAP: no name entry yet (16 §2.2); the HSCR entry takes the LSNM name.
fn record_result(g: &mut Game) {
    if g.replay() || g.spell_practice() {
        return;
    }
    let st = &g.stats;
    let (shot, d) = (g.shot as usize % 12, g.difficulty.clamp(0, 4) as usize);
    let score = st.score10.clamp(0, 999_999_999) as u32;
    if g.practice() {
        let h = &mut g.score.pscr[shot + g.demo_shot_bias as usize].hiscore[g.stage % 9][d];
        *h = (*h).max(score);
        return;
    }
    let mut h = crate::scoredat::Hscr::new();
    let stage = if g.flags & gamemgr::F_ENDING_OK != 0 {
        99
    } else {
        (g.stage % 9) as u8
    };
    let name: Vec<u8> = g.score.lsnm[..8].to_vec();
    h.set(score, shot as u8, d as u8, stage, &name, &date_md());
    h.0[0x26] = st.continues;
    if let Some(rank) = g.score.insert_hscr(h) {
        log::info!("high score rank {}", rank + 1);
    }
}

impl Task<Ctx> for Stub {
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc
            && matches!(
                self.kind,
                gamemgr::SCENE_RESULT | gamemgr::SCENE_RESULT_MENU
            )
        {
            ctx.game.save_score(&mut ctx.anm.rng, false);
        }
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let g = &mut ctx.game;
        if self.frame == 0
            && self.kind == gamemgr::SCENE_RESULT
            && matches!(g.previous, gamemgr::SCENE_GAME | gamemgr::SCENE_ENDING)
        {
            record_result(g);
        }
        if self.frame == 0
            && self.kind == gamemgr::SCENE_ENDING
            && g.flags & gamemgr::F_ENDING_OK != 0
            && !g.replay()
        {
            // Ending_Init step 3 (16 §1): Final A / B bits of the clear masks.
            let bit = if g.stage % 9 == 7 { 0x4000 } else { 0x8000 };
            g.set_clear(g.stats.continues == 0, bit);
            g.play_counter(crate::scoredat::PL_CLEARS);
            g.score.plst.music[18] = 1;
            g.score.plst.music[19] = 1;
        } else if self.frame == 0 && self.kind == gamemgr::SCENE_ENDING && !g.replay() {
            let shot = g.shot as usize % 12;
            g.score.clrd[shot].bad_ending = 0;
            g.score.plst.music[18] = 1;
        }
        self.frame += 1;
        if self.frame >= 30 && ctx.input.pressed(crate::input::SHOT | crate::input::ENTER) {
            let next = if self.kind == gamemgr::SCENE_ENDING {
                gamemgr::SCENE_RESULT
            } else {
                gamemgr::SCENE_MENU
            };
            ctx.game.request(next);
        }
        Ret::Next
    }

    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        use crate::ascii::Text;
        ctx.gfx.set_viewport(crate::gfx::FULLSCREEN);
        ctx.gfx.rect(0.0, 0.0, 640.0, 480.0, [0, 0, 0, 255]);
        let g = &ctx.game;
        let st = &g.stats;
        let title = match self.kind {
            gamemgr::SCENE_ENDING if g.flags & gamemgr::F_ENDING_OK != 0 => format!(
                "Ending (stub) route {}",
                if g.stage % 9 == 7 {
                    "Final B"
                } else {
                    "Final A"
                }
            ),
            gamemgr::SCENE_ENDING => "Bad ending (stub)".to_string(),
            gamemgr::SCENE_MUSIC_ROOM => "Music Room (stub)".to_string(),
            _ => "Result (stub)".to_string(),
        };
        let lines = [
            title,
            String::new(),
            format!(
                "Score    {:09}{}",
                st.score10.min(999_999_999),
                st.continues.min(9)
            ),
            format!("Stage    {}", g.stage),
            format!("Clock    {}", st.clock),
            format!("Continue {}", st.continues),
            format!("Spells   {}", st.spells),
            String::new(),
            "Press Shot".to_string(),
        ];
        for (i, l) in lines.into_iter().enumerate() {
            ctx.ascii.push(Text::new(160.0, 160.0 + 16.0 * i as f32, l));
        }
        Ret::Next
    }
}
