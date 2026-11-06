//! Main menu emit (12 §2.2, §2.4).

use crate::drawlist::{FULLSCREEN, Recorder};
use crate::emit::menulist;
use crate::game::View;
use crate::menu::{MainMenu, REPLAY, SPELL_CARD, SPELL_PRACTICE, SURFACE_PRIO};

// MainMenu_Draw 0x47087F: surface 0 at its own prio, else the VMs, the current description VM, the list screens and
// the Final-route notice.
pub fn menu(m: &MainMenu, ctx: &View, rec: &mut Recorder, tag: u8) {
    if !m.drawn(ctx.w) {
        return;
    }
    rec.set_viewport(FULLSCREEN);
    if tag == SURFACE_PRIO as u8 {
        if let Some((t, _)) = m.surface {
            rec.image(t, 0.0, 0.0);
        }
        return;
    }
    for vm in &m.vms {
        crate::emit::anm::emit(&ctx.w.anm, rec, vm);
    }
    if let Some(i) = m.desc_cur {
        crate::emit::anm::emit(&ctx.w.anm, rec, &m.desc[i]);
    }
    match m.screen {
        REPLAY => menulist::replay(m, ctx, rec),
        SPELL_PRACTICE => menulist::spell_char(m, ctx, rec),
        SPELL_CARD => menulist::spell_card(m, ctx, rec),
        _ => {}
    }
    if m.notice_sprite(ctx.w).is_some() {
        crate::emit::anm::emit_mode(&ctx.w.anm, rec, &m.notice, 0);
    }
}
