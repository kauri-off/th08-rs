use crate::game::Ctx;
use crate::gfx::{Blend, PLAYFIELD, Rect, TextureId};
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;

pub struct Supervisor {
    debug_sprites: Option<usize>,
}

pub fn register(ctx: &mut Ctx) -> Result<()> {
    let t = task::task(Supervisor { debug_sprites: std::env::var("TH08_SLOT_TEST").ok().and_then(|v| v.parse().ok()) });
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
            Title::register(ctx)?;
        }
        Ok(())
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
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

pub struct Title {
    image: Option<TextureId>,
}

impl Title {
    pub fn register(ctx: &mut Ctx) -> Result<()> {
        let t = task::task(Title { image: None });
        task::add(ctx, Chain::Calc, 4, 0, t.clone())?;
        task::add(ctx, Chain::Draw, 3, 0, t)?;
        Ok(())
    }
}

impl Task<Ctx> for Title {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            self.image = Some(ctx.gfx.load_image(&ctx.archive.read("title00.png")?)?);
            ctx.anm.load(&mut ctx.gfx, &ctx.archive, 20, "title01.anm")?;
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

    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        if let Some(t) = self.image {
            ctx.gfx.image(t, 0.0, 0.0);
        }
        Ret::Next
    }
}
