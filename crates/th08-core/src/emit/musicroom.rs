//! Music room emit (16 §5.2).

use crate::anmvm;
use crate::drawlist::{FULLSCREEN, Recorder};
use crate::game::View;
use crate::musicroom::MusicRoom;

// MusicRoom_Draw 0x449487: surface, background, rows, comments, then the title at its forced pos / colour.
pub fn music_room(m: &MusicRoom, ctx: &View, rec: &mut Recorder, _: u8) {
    if m.removed {
        return;
    }
    rec.set_viewport(FULLSCREEN);
    if let Some(t) = m.surface {
        rec.image(t, 0.0, 0.0);
    }
    crate::emit::anm::emit(&ctx.w.anm, rec, &m.bg);
    for i in m.top..m.row_end() {
        crate::emit::anm::emit(&ctx.w.anm, rec, &m.rows[anmvm::table_index(i)]);
    }
    for vm in &m.comments {
        crate::emit::anm::emit(&ctx.w.anm, rec, vm);
    }
    crate::emit::anm::emit(&ctx.w.anm, rec, &m.shown_title());
}
