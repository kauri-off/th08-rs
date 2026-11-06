//! Effect manager and screen-effect emit: lists A / C / D / E, list B background draws, rings, overlays (13 §2–§3).

use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm};
use crate::drawlist::{FULLSCREEN, PLAYFIELD, Recorder, Vertex};
use crate::effect::{BgDraw, Effect, Effects, ScreenEffect, ftol_u8, is_ring};
use crate::fmath::{cos_f, sin_cos_f, sin_f};
use crate::game::View;
use crate::types::{D3dColor, EffectIdx};
use std::f32::consts::PI;

// EffectMgr_Draw writes nothing: emit only.
pub fn node(ctx: &View, rec: &mut Recorder, _: u8) {
    main(&ctx.w.effects, &ctx.w.anm, rec);
}

pub fn screen_node(slot: usize, ctx: &View, rec: &mut Recorder, _: u8) {
    if let Some(s) = ctx.w.effects.screen_fx[slot].filter(|s| !s.done) {
        screen(&s, rec);
    }
}

// Lists A/D/E: 2D rotated sprite at pos + playfield origin + pos offset, or the ring renderer.
// The original sets pos / pos_offset around the draw and restores them; the emit draws a copy instead.
fn one(fx: &Effects, anm: &AnmManager, rec: &mut Recorder, i: EffectIdx, z: f32) {
    let e = &fx[i];
    if is_ring(e.kind) {
        draw_ring(anm, rec, e);
        return;
    }
    let mut vm = e.vm.clone();
    vm.pos = [
        e.pos[0] + 32.0 + e.vm.pos_offset[0],
        e.pos[1] + 16.0 + e.vm.pos_offset[1],
        0.0,
    ];
    vm.pos_offset = [0.0; 3];
    crate::emit::anm::emit_mode_z(anm, rec, &vm, 2, z);
}

// EffectMgr_Draw (prio 12): list A, then C, then E; A/E at z 0.07.
pub fn main(fx: &Effects, anm: &AnmManager, rec: &mut Recorder) {
    rec.set_viewport(PLAYFIELD);
    for list in [0, 2, 4] {
        for &i in &fx.lists[list] {
            if list == 2 {
                billboard_c(fx, anm, rec, i);
            } else {
                one(fx, anm, rec, i, 0.07);
            }
        }
    }
}

// List C: `AnmManager_DrawBillboard` (05 §9 mode 4) at the effect pos as-is (no playfield origin, no
// pos offset), projected through `Camera_Set2D` on the playfield viewport left by `Stage_DrawHigh`, whatever
// the VM's camera bit; sized by the stage camera's right vector, vertex z = projected z.
fn billboard_c(fx: &Effects, anm: &AnmManager, rec: &mut Recorder, i: EffectIdx) {
    let e = &fx[i];
    let mut vm = e.vm.clone();
    vm.pos = e.pos;
    vm.flags &= !anmvm::F_STAGE_CAM;
    crate::emit::anm::emit_mode_z(anm, rec, &vm, 4, 0.0);
}

// List D, drawn at the end of BulletMgr_Draw (prio 13), z 0.04.
pub fn over_bullets(fx: &Effects, anm: &AnmManager, rec: &mut Recorder) {
    for &i in &fx.lists[3] {
        one(fx, anm, rec, i, 0.04);
    }
}

// List D under BulletMgr_Draw's frozen tint (06 §10.2), already off at emit time: `tint` maps each sprite VM
// copy; rings ignore the global tint anyway.
pub fn over_bullets_with(
    fx: &Effects,
    tint: impl Fn(&AnmVm) -> AnmVm,
    anm: &AnmManager,
    rec: &mut Recorder,
) {
    for &i in &fx.lists[3] {
        let e = &fx[i];
        if is_ring(e.kind) {
            draw_ring(anm, rec, e);
            continue;
        }
        let mut vm = tint(&e.vm);
        vm.pos = [
            e.pos[0] + 32.0 + e.vm.pos_offset[0],
            e.pos[1] + 16.0 + e.vm.pos_offset[1],
            0.0,
        ];
        vm.pos_offset = [0.0; 3];
        crate::emit::anm::emit_mode_z(anm, rec, &vm, 2, 0.04);
    }
}

// Emit of one list-B draw. Layer 4: the original sets VM pos := effect pos around the draw; a copy here.
pub fn background(fx: &Effects, anm: &AnmManager, rec: &mut Recorder, d: &BgDraw) {
    match *d {
        BgDraw::Flat(i) => {
            let e = &fx[i];
            let mut vm = e.vm.clone();
            vm.pos = e.pos;
            crate::emit::anm::emit_mode_z(anm, rec, &vm, 2, 0.0);
        }
        BgDraw::Billboard { i, s, len } => {
            crate::emit::anm::draw_billboard(anm, rec, &fx[i].vm, s, len);
        }
    }
}

// Stage 6B ring draw from `Stage_DrawLow` step 4 (06 §5.1): calls the slot's draw fn through the stored pointer,
// with no active check.
pub fn ring_at(fx: &Effects, anm: &AnmManager, rec: &mut Recorder, idx: usize) {
    draw_ring(anm, rec, &fx.fx[idx]);
}

// Ring renderer 0x427450: strip of 2(n+1) vertices, v along the ring, u across the band, WRAP sampling.
fn draw_ring(anm: &AnmManager, rec: &mut Recorder, e: &Effect) {
    let vm = &e.vm;
    if vm.flags & (anmvm::F_VISIBLE | anmvm::F_ENABLED) != (anmvm::F_VISIBLE | anmvm::F_ENABLED)
        || vm.color[anmvm::A] == 0
        || !vm.has_sprite
    {
        return;
    }
    let Some(&spr) = anm.sprite(
        anmvm::table_index(i32::from(vm.anm_slot)),
        anmvm::table_index(i32::from(vm.sprite_index)),
    ) else {
        return;
    };
    let r = e.ring;
    let n = r.segments.max(3);
    let step = 2.0 * PI / n as f32;
    let w = r.width / sin_f((PI - step) / 2.0);
    // Half width w = width / sin((π − 2π/n)/2); +0.5 is the port's D3D → wgpu pixel-centre offset.
    let (cx, cy) = (e.centre[0] + 32.0 + 0.5, e.centre[1] + 16.0 + 0.5);
    // Raw VM colour: `Anm_RingSetUV` 0x464B00 skips the global ANM tint.
    let c = vm.color;
    let rgba = [c[anmvm::R], c[anmvm::G], c[anmvm::B], c[anmvm::A]];
    let wrap = |a: f32| if a >= PI { a - 2.0 * PI } else { a };
    let point = |k: i32, outer: bool| -> [f32; 2] {
        let th = wrap(r.start + k as f32 * step);
        let sgn = if outer { 1.0 } else { -1.0 };
        // Plain ring / rotated ellipse (θ from 0) / wavy ring.
        if r.amp == 0.0 {
            let rr = r.radius + sgn * w;
            [cx + cos_f(th) * rr, cy + sin_f(th) * rr]
        } else if r.waves == 0.0 {
            let th = wrap(k as f32 * step);
            let (x, y) = (
                cos_f(th) * (r.radius + sgn * w),
                sin_f(th) * (r.amp + sgn * w),
            );
            let (s, co) = sin_cos_f(r.tilt);
            [cx + x * co - y * s, cy + x * s + y * co]
        } else {
            let phi = r.tilt + k as f32 * 2.0 * PI * r.waves / n as f32;
            let rr = r.radius + cos_f(phi) * r.amp + sgn * w;
            [cx + cos_f(th) * rr, cy + sin_f(th) * rr]
        }
    };
    let blend = crate::emit::anm::blend_for(rec, vm);
    let (u0, v0, u1, v1) = (
        spr.uv[0] + vm.uv_off[0],
        spr.uv[1],
        spr.uv[2] + vm.uv_off[0],
        spr.uv[3],
    );
    let vk = |k: i32| v1 + vm.uv_off[1] - k as f32 * (v1 - v0) / n as f32;
    for k in 0..n {
        let p = [
            point(k, true),
            point(k, false),
            point(k + 1, true),
            point(k + 1, false),
        ];
        let uv = [[u0, vk(k)], [u1, vk(k)], [u0, vk(k + 1)], [u1, vk(k + 1)]];
        // Vertex z: plain ring = 0 + centre z (the spawn position z); ellipse / wavy := 0 after the add.
        let z = if r.amp == 0.0 { e.centre[2] } else { 0.0 };
        let v = std::array::from_fn(|i| Vertex::screen(p[i][0], p[i][1], z, uv[i], rgba));
        // z-write on unless VM flag bit 13 (`AnmManager_SetRenderState2D`).
        rec.strip(spr.tex, blend, vm.flags & anmvm::F_ZWRITE_OFF == 0, v);
    }
}

// Types 0/4/5 full screen, 2/3/6 playfield; colour p1 (p2 for the flash).
#[expect(clippy::disallowed_methods, reason = "draw only")]
fn screen(s: &ScreenEffect, rec: &mut Recorder) {
    let colour = D3dColor(match s.kind {
        0 | 2 | 4 | 5 | 6 => s.p[1].cast_unsigned(),
        3 => s.p[2].cast_unsigned(),
        _ => return,
    });
    let rgba = [
        colour.r(),
        colour.g(),
        colour.b(),
        ftol_u8(s.alpha.clamp(0.0, 255.0)),
    ];
    let full = matches!(s.kind, 0 | 4 | 5);
    let r = if full { FULLSCREEN } else { PLAYFIELD };
    rec.set_viewport(r);
    rec.rect(r.x, r.y, r.w, r.h, rgba);
}
