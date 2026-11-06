//! Spell manager emit: name plates, cut-ins, streak strips and the bonus line (12 §5.8).

use crate::anmmgr::Tint;
use crate::anmvm::AnmVm;
use crate::drawlist::{PLAYFIELD, Recorder};
use crate::game::View;
use crate::gui::Gui;

pub fn node(ctx: &View, rec: &mut Recorder, tag: u8) {
    if let Some(g) = ctx.w.gui.as_deref() {
        gui(g, ctx, rec, tag);
    }
}

// Spell_Draw 0x4178C0 emit (draw steps 1–4).
fn gui(g: &Gui, ctx: &View, rec: &mut Recorder, _: u8) {
    let anm = &ctx.w.anm;
    rec.set_viewport(PLAYFIELD);
    // No 0xFFF01010 freeze tint here: the only tint of Spell_Draw is +0x2620 of step 4.
    let v = &g.vms;
    // VMs 0–6 are drawn before the VM-7 block, under the global tint still on then.
    let pre = g.pre_tint;
    let draw = |rec: &mut Recorder, vm: &AnmVm, mode| {
        crate::emit::anm::emit_tinted(anm, rec, vm, mode, vm.pos[2], pre);
    };
    // Boss strips (VM 2/4) are drawn only while the player cut-in VM 0 is visible: original quirk (step 1).
    if Gui::shown(&v[0]) {
        draw(rec, &v[0], 0);
        draw(rec, &v[2], 0);
        draw(rec, &v[4], 2);
    }
    if Gui::shown(&v[1]) {
        // Boss cut-in drawn at pos + pos_offset, restored afterwards (step 2).
        let mut c = v[1].clone();
        let (p, o) = (c.pos, c.pos_offset);
        c.pos = [p[0] + o[0], p[1] + o[1], p[2] + o[2]];
        draw(rec, &c, 0);
    }
    if Gui::shown(&v[6]) {
        draw(rec, &v[10], 0);
        draw(rec, &v[6], 2);
    }
    if Gui::shown(&v[7]) {
        // Tint +0x2620 (step 4), turned off by the step; the alpha-0 skip tests the untinted colour.
        let t = Some(Tint(g.tint));
        let draw = |rec: &mut Recorder, vm: &AnmVm, mode| {
            crate::emit::anm::emit_tinted(anm, rec, vm, mode, vm.pos[2], t);
        };
        draw(rec, &v[11], 0);
        for k in [7, 8, 9] {
            if v[k].script.is_some() {
                draw(rec, &v[k], 2);
            }
        }
        draw(rec, &v[13], 0);
        if !g.name_only {
            let mut vm = v[12].clone();
            for st in g.bonus_stamps() {
                Gui::stamp(&mut vm, anm, st);
                draw(rec, &vm, 0);
            }
        }
    }
}
