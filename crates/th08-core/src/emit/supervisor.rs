//! Supervisor and scene emits: loading screen, loading VMs, slot test; scene draw nodes forwarded per scene.

use crate::drawlist::{Blend, PLAYFIELD, Recorder, Rect};
use crate::game::View;
use crate::supervisor::{Scene, Supervisor};

pub fn node(ctx: &View, rec: &mut Recorder, tag: u8) {
    if let Some(s) = ctx.w.supervisor.as_deref() {
        supervisor(s, ctx, rec, tag);
    }
}

pub fn scene_node(ctx: &View, rec: &mut Recorder, tag: u8) {
    match &ctx.w.scene {
        Scene::Menu(x) => crate::emit::menu::menu(x, ctx, rec, tag),
        Scene::Result(x) => crate::emit::result::result(x, ctx, rec, tag),
        Scene::Ending(x) => crate::emit::ending::ending(x, ctx, rec, tag),
        Scene::MusicRoom(x) => crate::emit::musicroom::music_room(x, ctx, rec, tag),
        Scene::None => (),
    }
}

// Tag 0 the loading surface and prompt, tag 2 the loading VMs; TH08_SLOT_TEST=<slot> draws that slot's sprites in a
// grid, clipped to the playfield (tag 16).
fn supervisor(s: &Supervisor, ctx: &View, rec: &mut Recorder, tag: u8) {
    match tag {
        0 => {
            if let Some(t) = s.logo {
                rec.set_viewport(crate::drawlist::FULLSCREEN);
                rec.image(t, 0.0, 0.0);
            }
            // Then the "Press Shot Button" prompt with the whole queue (12 §1).
            crate::emit::ascii::emit_texts(&ctx.w.anm, rec, &s.prompt);
        }
        2 => {
            // The scripts' own positions (slide-in, z) are relative to the loading position.
            // 0x445D3E: pos += pos_offset, Draw2DRot, pos −= pos_offset (12 §1): a shifted copy, no net write.
            for vm in &s.boot_vms {
                let (p, o) = (vm.pos, vm.pos_offset);
                let mut v = vm.clone();
                v.pos = [
                    p[0] + o[0] + s.load_pos[0],
                    p[1] + o[1] + s.load_pos[1],
                    p[2] + o[2],
                ];
                crate::emit::anm::emit_mode(&ctx.w.anm, rec, &v, 2);
            }
        }
        _ => {}
    }
    let (Some(slot), 16) = (s.debug_sprites, tag) else {
        return;
    };
    let Some(anm) = ctx.w.anm.get(slot) else {
        return;
    };
    rec.set_viewport(PLAYFIELD);
    rec.clear_viewport([0, 0, 64, 255]);
    let (mut x, mut y, mut row) = (0.0, 0.0, 0.0f32);
    for s in &anm.sprites {
        if x + s.w > 640.0 {
            (x, y, row) = (0.0, y + row, 0.0);
        }
        rec.sprite(
            s.tex,
            Blend::Alpha,
            Rect::new(x, y, s.w, s.h),
            s.uv,
            [255; 4],
        );
        x += s.w;
        row = row.max(s.h);
    }
    rec.set_viewport(crate::drawlist::FULLSCREEN);
}
