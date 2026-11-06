//! Ending emit (16 §1.5).

use crate::drawlist::Recorder;
use crate::ending::Ending;
use crate::game::View;

// Draw 0x4298F0: picture, 15 VMs, fade overlay.
pub fn ending(e: &Ending, ctx: &View, rec: &mut Recorder, _: u8) {
    if e.done {
        return;
    }
    rec.set_viewport(crate::drawlist::FULLSCREEN);
    if let Some(t) = e.picture {
        // The float source offset is truncated (ftol) to whole pixels (16 §1.5).
        #[expect(
            clippy::cast_sign_loss,
            reason = "ftol of a value clamped to ≥ 0 (16 §1.5)"
        )]
        let (sx, sy) = (e.pic_x.max(0.0) as u32, e.pic_y.max(0.0) as u32);
        rec.image_rect(t, sx, sy, 640, 480, 0.0, 0.0);
    }
    for vm in &e.vms {
        crate::emit::anm::emit(&ctx.w.anm, rec, vm);
    }
    let c = e.fade_shown;
    if c >> 24 != 0 {
        let rgba = [(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8];
        rec.rect(0.0, 0.0, 640.0, 480.0, rgba);
    }
}
