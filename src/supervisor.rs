use crate::anmvm::AnmVm;
use crate::game::Ctx;
use crate::gfx::{Blend, PLAYFIELD, Rect, TextureId};
use crate::gamemgr::{self, Game, GameManager};
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

// SPEC-GAP: the original shows the boot logo for as long as its loader thread runs; loading here is instant,
// so the logo is held for this many frames.
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
        Some(v) => crate::player::parse_shot(&v).with_context(|| format!("TH08_PLAYER={v}: unknown shot type"))?,
        None => 0,
    };
    let default_diff = if stage % 9 == 8 { 4 } else { 1 };
    g.difficulty = crate::env::var("TH08_RANK").and_then(|s| crate::enemy::parse_difficulty(&s)).unwrap_or(default_diff);
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

    fn set_scene<T: Task<Ctx> + 'static>(&mut self, ctx: &mut Ctx, t: T, calc: u16, draw: u16) -> Result<()> {
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
            return Ok(Some(Ret::Quit));
        }
        log::info!("scene {cur} -> {req}");
        match (cur, req) {
            (gamemgr::SCENE_BOOT, gamemgr::SCENE_GAME) => self.start_game(ctx)?,
            (gamemgr::SCENE_BOOT, _) | (_, gamemgr::SCENE_MENU) => {
                self.drop_scene(ctx);
                ctx.game.requested = gamemgr::SCENE_MENU;
                self.set_scene(ctx, crate::menu::MainMenu::new(), crate::menu::CALC_PRIO, crate::menu::DRAW_PRIO)?;
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
                let g = &mut ctx.game;
                if !g.practice() && g.difficulty < 4 {
                    g.stage = 0;
                }
                g.fresh_start = true;
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
            (_, gamemgr::SCENE_RESULT | gamemgr::SCENE_ENDING | gamemgr::SCENE_RESULT_MENU | gamemgr::SCENE_MUSIC_ROOM) => {
                self.drop_scene(ctx);
                self.set_scene(ctx, Stub { kind: req, frame: 0 }, 4, 3)?;
            }
            (_, gamemgr::SCENE_REPLAY_END) => {
                self.drop_scene(ctx);
                ctx.game.requested = gamemgr::SCENE_MENU;
                self.set_scene(ctx, crate::menu::MainMenu::new(), crate::menu::CALC_PRIO, crate::menu::DRAW_PRIO)?;
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
            ctx.anm.load(&mut ctx.gfx, &ctx.archive, 2, "nowloading.anm")?;
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
            if let Some(arg) = crate::env::var("TH08_STAGE") {
                let index = crate::stage::parse_stage_arg(&arg).with_context(|| format!("TH08_STAGE={arg}: unknown stage"))?;
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
        let (Some(slot), 16) = (self.debug_sprites, tag) else { return Ret::Next };
        let Some(anm) = ctx.anm.get(slot) else { return Ret::Next };
        let sprites = anm.sprites.clone();
        ctx.gfx.set_viewport(PLAYFIELD);
        ctx.gfx.clear_viewport([0, 0, 64, 255]);
        let (mut x, mut y, mut row) = (0.0, 0.0, 0.0f32);
        for s in sprites {
            if x + s.w > 640.0 {
                (x, y, row) = (0.0, y + row, 0.0);
            }
            ctx.gfx.sprite(s.tex, Blend::Alpha, Rect::new(x, y, s.w, s.h), s.uv, [255; 4]);
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

impl Task<Ctx> for Stub {
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        self.frame += 1;
        if self.frame >= 30 && ctx.input.pressed(crate::input::SHOT | crate::input::ENTER) {
            let next = if self.kind == gamemgr::SCENE_ENDING { gamemgr::SCENE_RESULT } else { gamemgr::SCENE_MENU };
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
            gamemgr::SCENE_ENDING if g.flags & gamemgr::F_ENDING_OK != 0 => format!("Ending (stub) route {}", if g.stage % 9 == 7 { "Final B" } else { "Final A" }),
            gamemgr::SCENE_ENDING => "Bad ending (stub)".to_string(),
            gamemgr::SCENE_MUSIC_ROOM => "Music Room (stub)".to_string(),
            _ => "Result (stub)".to_string(),
        };
        let lines = [
            title,
            String::new(),
            format!("Score    {:09}{}", st.score10.min(999_999_999), st.continues.min(9)),
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
