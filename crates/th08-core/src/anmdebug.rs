//! Debug tool: live ANM script viewer (TH08_ANM), not a port of the original.

use crate::anmvm::AnmVm;
use crate::drawlist::{FULLSCREEN, PLAYFIELD, Recorder};
use crate::game::{Ctx, View};
use crate::task::{self, Chain, Ret, Task};
use anyhow::{Context, Result};

// TH08_ANM=<file>:<script>[,<script>...][:pf] runs scripts live in slot 24 (`pf` = playfield viewport);
// TH08_ANM_INT=<frame>:<id>[,<frame>:<id>...] sends interrupts; TH08_ANM_POS=x,y[,dx] places VM i at (x + i·dx, y);
// TH08_ANM_MODE=<n> forces a draw mode (owners choose it; scripts rarely set one).
#[derive(Clone)]
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
    ctx.w.anm.load(&mut ctx.rec, &ctx.archive, SLOT, file)?;
    let mut vms = Vec::new();
    for s in scripts.split(',') {
        let (a, b) = s.split_once('-').unwrap_or((s, s));
        for i in a.parse::<usize>()?..=b.parse::<usize>()? {
            let mut vm = AnmVm::default();
            ctx.w.anm.set_script(&mut ctx.w.rng, &mut vm, SLOT, i);
            vms.push(vm);
        }
    }
    let pos = ctx.core.anm_pos.clone();
    if pos.len() >= 2 {
        for (i, vm) in vms.iter_mut().enumerate() {
            vm.pos = [
                pos[0] + i as f32 * pos.get(2).copied().unwrap_or(0.0),
                pos[1],
                0.0,
            ];
        }
    }
    let interrupts = ctx.core.anm_int.clone();
    ctx.w.anm_debug = Some(Box::new(AnmDebug {
        vms,
        playfield,
        interrupts,
        frame: 0,
        mode: ctx.core.anm_mode,
    }));
    let t = crate::chain::Node::AnmDebug;
    task::add(ctx, Chain::Calc, 4, 0, t)?;
    task::add(ctx, Chain::Draw, 3, 0, t)?;
    Ok(())
}

// The viewer lent out of `ctx.w.anm_debug` for one of its chain callbacks.
fn with<R>(ctx: &mut Ctx, f: impl FnOnce(&mut AnmDebug, &mut Ctx) -> R) -> R {
    let mut a = ctx.w.anm_debug.take().expect("no anm debug viewer");
    let r = f(&mut a, ctx);
    ctx.w.anm_debug = Some(a);
    r
}

// Chain nodes of `ctx.w.anm_debug` (calc 4, draw 3): stateless forwarder.
pub struct AnmDebugTask;

impl Task<Ctx> for AnmDebugTask {
    fn calc(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        with(ctx, |a, ctx| a.calc(ctx, tag))
    }

    fn draw_step(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        with(ctx, |a, ctx| a.draw_step(ctx, tag))
    }
}

impl AnmDebugTask {
    pub(crate) fn emit(&self, ctx: &View, rec: &mut Recorder, tag: u8) {
        if let Some(a) = ctx.w.anm_debug.as_deref() {
            a.emit(ctx, rec, tag);
        }
    }
}

impl AnmDebug {
    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        self.frame += 1;
        for &(f, id) in &self.interrupts {
            if f == self.frame {
                self.vms.iter_mut().for_each(|v| v.pending_interrupt = id);
            }
        }
        for vm in &mut self.vms {
            vm.run(&mut ctx.w.anm, &mut ctx.w.rng);
        }
        Ret::Next
    }

    fn draw_step(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        for vm in &mut self.vms {
            let mode = self.mode.unwrap_or(vm.draw_mode);
            ctx.w.anm.prepare(vm, mode);
        }
        Ret::Next
    }

    fn emit(&self, ctx: &View, rec: &mut Recorder, _: u8) {
        rec.set_viewport(if self.playfield {
            PLAYFIELD
        } else {
            FULLSCREEN
        });
        rec.clear_viewport([32, 32, 64, 255]);
        for vm in &self.vms {
            ctx.w
                .anm
                .emit_mode(rec, vm, self.mode.unwrap_or(vm.draw_mode));
        }
        rec.set_viewport(FULLSCREEN);
    }
}
