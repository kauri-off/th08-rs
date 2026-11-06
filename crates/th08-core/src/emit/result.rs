//! Result screen emit (16 §2.4).

use crate::anmvm::{self, AnmVm, table_index};
use crate::drawlist::{FULLSCREEN, Recorder};
use crate::game::View;
use crate::result::{ResultScreen, State, VM_ANCHOR};

// Result_Draw 0x4586B4: surface, VMs at pos + pos_offset, card list, play lines, save dialogue VMs.
pub fn result(r: &ResultScreen, ctx: &View, rec: &mut Recorder, _: u8) {
    if r.removed {
        return;
    }
    rec.set_viewport(FULLSCREEN);
    if let Some(t) = r.surface {
        rec.image(t, 0.0, 0.0);
    }
    for vm in &r.vms {
        let mut vm = vm.clone();
        for k in 0..3 {
            vm.pos[k] += vm.pos_offset[k];
        }
        crate::emit::anm::emit(&ctx.w.anm, rec, &vm);
    }
    if r.state == State::Cards {
        cards(r, ctx, rec);
    }
    if r.state.is_play() {
        for vm in &r.text_vms[..20] {
            crate::emit::anm::emit_mode(&ctx.w.anm, rec, vm, 0);
        }
    }
    if r.state.is_save() {
        for vm in &r.vms[49..55] {
            crate::emit::anm::emit(&ctx.w.anm, rec, vm);
        }
    }
}

// Emit of the card list: header text VM 10, then per row the bar (×2.375) and the row text VM, all copies.
fn cards(r: &ResultScreen, ctx: &View, rec: &mut Recorder) {
    let a = r.vms[VM_ANCHOR].pos;
    if a[0] >= 640.0 || r.card_page < 0 {
        return;
    }
    emit_at(ctx, rec, &r.text_vms[10], a[0] + 320.0, a[1] - 16.0);
    for (i, _, x, y) in r.card_layout() {
        let mut bar = r.bar.clone();
        bar.scale = [2.375, 1.0];
        bar.flags |= anmvm::F_SCALE_DIRTY;
        emit_at(ctx, rec, &bar, x + 320.0, y + 16.0);
        emit_at(ctx, rec, &r.text_vms[table_index(i)], x + 78.0, y);
    }
}

// Mode-0 draw of a copy at (x, y): the original's draws of these VMs leave no persistent write.
fn emit_at(ctx: &View, rec: &mut Recorder, vm: &AnmVm, x: f32, y: f32) {
    let mut vm = vm.clone();
    vm.pos = [x, y, 0.0];
    crate::emit::anm::emit_mode(&ctx.w.anm, rec, &vm, 0);
}
