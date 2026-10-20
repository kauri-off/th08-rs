//! Debug tool: live ANM script viewer (TH08_ANM), not a port of the original.

use crate::anmvm::AnmVm;
use crate::game::Ctx;
use crate::gfx::{FULLSCREEN, PLAYFIELD};
use crate::task::{self, Chain, Ret, Task};
use anyhow::{Context, Result};

// TH08_ANM=<file>:<script>[,<script>...][:pf] runs scripts live in slot 24 (`pf` = playfield viewport);
// TH08_ANM_INT=<frame>:<id>[,<frame>:<id>...] sends interrupts; TH08_ANM_POS=x,y[,dx] places VM i at (x + i·dx, y);
// TH08_ANM_MODE=<n> forces a draw mode (owners choose it; scripts rarely set one).
pub struct AnmDebug {
    vms: Vec<AnmVm>,
    playfield: bool,
    interrupts: Vec<(u64, u16)>,
    frame: u64,
    mode: Option<i32>,
}

const SLOT: usize = 24;

pub fn register(ctx: &mut Ctx, spec: &str) -> Result<()> {
    let mut parts = spec.split(':');
    let file = parts.next().context("TH08_ANM: file")?;
    let scripts = parts.next().unwrap_or("0");
    let playfield = parts.next() == Some("pf");
    ctx.anm.load(&mut ctx.gfx, &ctx.archive, SLOT, file)?;
    let mut vms = Vec::new();
    for s in scripts.split(',') {
        let (a, b) = s.split_once('-').unwrap_or((s, s));
        for i in a.parse::<usize>()?..=b.parse::<usize>()? {
            let mut vm = AnmVm::default();
            ctx.anm.set_script(&mut vm, SLOT, i);
            vms.push(vm);
        }
    }
    let pos = &crate::settings::get().anm_pos;
    if pos.len() >= 2 {
        for (i, vm) in vms.iter_mut().enumerate() {
            vm.pos = [
                pos[0] + i as f32 * pos.get(2).copied().unwrap_or(0.0),
                pos[1],
                0.0,
            ];
        }
    }
    let interrupts = ctx.settings.anm_int.clone();
    let t = task::task(AnmDebug {
        vms,
        playfield,
        interrupts,
        frame: 0,
        mode: ctx.settings.anm_mode,
    });
    task::add(ctx, Chain::Calc, 4, 0, t.clone())?;
    task::add(ctx, Chain::Draw, 3, 0, t)?;
    Ok(())
}

impl Task<Ctx> for AnmDebug {
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        self.frame += 1;
        for &(f, id) in &self.interrupts {
            if f == self.frame {
                self.vms.iter_mut().for_each(|v| v.pending_interrupt = id);
            }
        }
        for vm in &mut self.vms {
            vm.run(&mut ctx.anm);
        }
        Ret::Next
    }

    fn draw(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        ctx.gfx.set_viewport(if self.playfield {
            PLAYFIELD
        } else {
            FULLSCREEN
        });
        ctx.gfx.clear_viewport([32, 32, 64, 255]);
        for vm in &mut self.vms {
            let mode = self.mode.unwrap_or(vm.draw_mode);
            ctx.anm.draw_mode(&mut ctx.gfx, vm, mode);
        }
        ctx.gfx.set_viewport(FULLSCREEN);
        Ret::Next
    }
}
