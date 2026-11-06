//! ANM viewer emit (TH08_ANM): background clear and the live VMs.

use crate::anmdebug::AnmDebug;
use crate::drawlist::{FULLSCREEN, PLAYFIELD, Recorder};
use crate::game::View;

pub fn node(ctx: &View, rec: &mut Recorder, tag: u8) {
    if let Some(a) = ctx.w.anm_debug.as_deref() {
        anm_debug(a, ctx, rec, tag);
    }
}

fn anm_debug(a: &AnmDebug, ctx: &View, rec: &mut Recorder, _: u8) {
    rec.set_viewport(if a.playfield { PLAYFIELD } else { FULLSCREEN });
    rec.clear_viewport([32, 32, 64, 255]);
    for vm in &a.vms {
        crate::emit::anm::emit_mode(&ctx.w.anm, rec, vm, a.mode.unwrap_or(vm.draw_mode));
    }
    rec.set_viewport(FULLSCREEN);
}
