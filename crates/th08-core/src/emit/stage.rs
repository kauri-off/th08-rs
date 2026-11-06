//! Stage draw emit: replays the DrawLow / DrawHigh items recorded by the stage draw step (06 §10, 07 §12.1).

use crate::anmvm;
use crate::drawlist::{PLAYFIELD, Recorder, Vertex};
use crate::game::View;
use crate::stage::{Item, Stage};
use th08_formats::stdfile::Vec3;

pub fn node(ctx: &View, rec: &mut Recorder, tag: u8) {
    if let Some(st) = ctx.w.stage.as_deref() {
        stage(st, ctx, rec, tag);
    }
}

fn stage(st: &Stage, ctx: &View, rec: &mut Recorder, tag: u8) {
    let anm = &ctx.w.anm;
    for it in st.drawn.get(tag as usize).into_iter().flatten() {
        match it {
            Item::Viewport => rec.set_viewport(PLAYFIELD),
            Item::AnmReset => rec.anm.reset(),
            Item::DepthTest(on) => rec.set_depth_test(*on),
            Item::ClearDepth(c) => rec.clear_depth(Some(*c)),
            Item::ClearViewport(c) => rec.clear_viewport(*c),
            Item::ClearZ(arg) => rec.clear_z(*arg),
            Item::Fade(c) => rec.rect(32.0, 16.0, 384.0, 448.0, *c),
            Item::Vm { vm, mode, z } => crate::emit::anm::emit_mode_z(anm, rec, vm, *mode, *z),
            Item::World { vm, cam, fog } => {
                crate::emit::anm::emit_3d_world(anm, rec, vm, cam, Some(*fog));
            }
            Item::Ribbon { vm, tex, v } => {
                let blend = crate::emit::anm::blend_for(rec, vm);
                rec.anm.bound_tex = Some(*tex);
                rec.strip(*tex, blend, vm.flags & anmvm::F_ZWRITE_OFF == 0, *v);
            }
            Item::Ring(i) => crate::emit::effect::ring_at(&ctx.w.effects, anm, rec, *i),
            Item::Background(d) => crate::emit::effect::background(&ctx.w.effects, anm, rec, d),
            Item::MaskFrame { a, b, c } => mask_frame(rec, *a, *b, *c),
        }
    }
}

// Spell mask frame (07 §12.1): four black strips between half sizes a and b around the ring centre, z 0.8.
fn mask_frame(rec: &mut Recorder, a: f32, b: f32, c: Vec3) {
    // +0.5: port D3D → wgpu pixel-centre offset.
    let (cx, cy) = (c[0] + 32.0 + 0.5, c[1] + 16.0 + 0.5);
    let ring = [
        (-1.0, -1.0),
        (1.0, -1.0),
        (1.0, 1.0),
        (-1.0, 1.0),
        (-1.0, -1.0),
    ];
    let white = rec.white();
    rec.set_depth_test(false);
    for k in 0..4 {
        let v = [
            (ring[k], a),
            (ring[k], b),
            (ring[k + 1], a),
            (ring[k + 1], b),
        ]
        .map(|((sx, sy), r)| {
            Vertex::screen(cx + sx * r, cy + sy * r, 0.8, [0.0, 0.0], [0, 0, 0, 255])
        });
        rec.strip(white, crate::drawlist::Blend::Alpha, true, v);
    }
}
