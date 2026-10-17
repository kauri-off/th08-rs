use crate::anmvm::AnmVm;
use crate::game::Ctx;
use crate::gfx::{Blend, PLAYFIELD, Rect, TextureId};
use crate::task::{self, Chain, Ret, Task};
use anyhow::{Context, Result};

pub struct Supervisor {
    debug_sprites: Option<usize>,
    script: Option<Vec<u16>>,
    frame: usize,
}

pub fn register(ctx: &mut Ctx) -> Result<()> {
    let t = task::task(Supervisor {
        debug_sprites: crate::env::var("TH08_SLOT_TEST").and_then(|v| v.parse().ok()),
        script: crate::env::var("TH08_INPUT").and_then(|v| crate::input::load_script(&v)),
        frame: 0,
    });
    task::add(ctx, Chain::Calc, 0, 0, t.clone())?;
    for prio in [0, 2, 16] {
        task::add(ctx, Chain::Draw, prio, prio as u8, t.clone())?;
    }
    Ok(())
}

impl Task<Ctx> for Supervisor {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            ctx.anm.load(&mut ctx.gfx, &ctx.archive, 0, "text.anm")?;
            ctx.anm.load(&mut ctx.gfx, &ctx.archive, 2, "nowloading.anm")?;
            if let Some(spec) = crate::env::var("TH08_ANM") {
                crate::anmdebug::register(ctx, &spec)?;
            } else if let Some(arg) = crate::env::var("TH08_STAGE") {
                let index = crate::stage::parse_stage_arg(&arg).with_context(|| format!("TH08_STAGE={arg}: unknown stage"))?;
                let shot = match crate::env::var("TH08_PLAYER") {
                    Some(v) => crate::player::parse_shot(&v).with_context(|| format!("TH08_PLAYER={v}: unknown shot type"))?,
                    None => 0,
                };
                let player = crate::player::Player::new_ref();
                crate::player::Player::register(ctx, player.clone(), shot)?;
                if let Some(pw) = crate::env::var("TH08_POWER").and_then(|v| v.parse::<f32>().ok()) {
                    player.borrow_mut().stats.power = pw.clamp(0.0, 128.0);
                }
                let t = crate::stage::Stage::register(ctx, index, Some(player.clone()))?;
                let bgm = t.borrow().std.bgm[0].clone();
                ctx.audio.play_bgm(&bgm);
                let bullets = crate::bullet::BulletManager::register(ctx, player)?;
                crate::enemy::EnemyManager::register(ctx, index, Some(t), bullets)?;
            } else {
                Title::register(ctx)?;
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
        Ret::Next
    }

    // TH08_SLOT_TEST=<slot>: draws that slot's sprites in a grid, clipped to the playfield.
    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
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

// SPEC-GAP: the main menu's script set is unspecified; title01.anm scripts 0..=9 (side picture + the nine
// main-menu items, by their positions in the data) are started, drawn in order with their own draw mode.
const TITLE_SCRIPTS: std::ops::RangeInclusive<usize> = 0..=9;

pub struct Title {
    image: Option<TextureId>,
    vms: Vec<AnmVm>,
}

impl Title {
    pub fn register(ctx: &mut Ctx) -> Result<()> {
        let t = task::task(Title { image: None, vms: Vec::new() });
        task::add(ctx, Chain::Calc, 4, 0, t.clone())?;
        task::add(ctx, Chain::Draw, 3, 0, t)?;
        Ok(())
    }
}

impl Task<Ctx> for Title {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            self.image = Some(ctx.gfx.load_image(&ctx.archive.read("title00.png")?)?);
            // SPEC-GAP: the "previous scene was the result screen" exception waits for scene tracking.
            ctx.audio.play_bgm("bgm/th08_01.mid");
            ctx.anm.load(&mut ctx.gfx, &ctx.archive, 20, "title01.anm")?;
            self.vms = TITLE_SCRIPTS
                .map(|i| {
                    let mut vm = AnmVm::default();
                    ctx.anm.set_script(&mut vm, 20, i);
                    vm
                })
                .collect();
        }
        Ok(())
    }

    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) {
        if chain == Chain::Calc {
            if let Some(t) = self.image.take() {
                ctx.gfx.free_texture(t);
            }
            ctx.anm.unload(&mut ctx.gfx, 20);
        }
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        for vm in self.vms.iter_mut().filter(|v| v.script_index >= 0) {
            vm.run(&mut ctx.anm);
        }
        Ret::Next
    }

    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        if let Some(t) = self.image {
            ctx.gfx.image(t, 0.0, 0.0);
        }
        for vm in &mut self.vms {
            ctx.anm.draw(&mut ctx.gfx, vm);
        }
        Ret::Next
    }
}
