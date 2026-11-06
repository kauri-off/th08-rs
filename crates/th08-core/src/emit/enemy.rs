//! Enemy draw emit: trail copies / ring strip and the main / sub VMs per layer (07 §10.6).

use crate::anmmgr::{AnmManager, Tint};
use crate::anmvm::{self, table_index};
use crate::bullet::FROZEN_TINT;
use crate::drawlist::{PLAYFIELD, Recorder};
use crate::enemy::{Enemy, EnemyManager, trail_mid};
use crate::game::View;
use crate::player::Player;
use crate::types::lo8;

pub fn node(ctx: &View, rec: &mut Recorder, tag: u8) {
    let p = ctx.w.player.as_deref().expect("player");
    let m = ctx.w.enemies.as_deref().expect("enemy manager");
    layer(m, &ctx.w.anm, rec, p, tag == 1, ctx.w.game.flags);
}

// Enemy drawing emit: Draw2DRot with the VM z (0.25 main / 0.3 sub) as the vertex depth (05 §9).
fn layer(
    m: &EnemyManager,
    anm: &AnmManager,
    rec: &mut Recorder,
    p: &Player,
    high: bool,
    flags: u32,
) {
    rec.set_viewport(PLAYFIELD);
    // The frozen tint is off again when the emit runs: the emit applies it to VM copies.
    let tint = (high && p.freeze_bullets(flags)).then_some(Tint(FROZEN_TINT));
    for i in m.draw_order(high) {
        let e = &m.enemies[i];
        for k in [0usize, 2, 1] {
            let main = k == 2;
            let trail = main && e.trail[0] != 0;
            if trail {
                draw_trail(anm, rec, e, tint);
            }
            if EnemyManager::vm_drawn(e, k) {
                let drew =
                    trail && e.trail[0] & 8 == 0 && EnemyManager::trail_copies(e).next().is_some();
                let vm = if main { &e.vm } else { &e.sub_vms[k] };
                crate::emit::anm::emit_tinted(anm, rec, vm, 2, EnemyManager::vm_z(k, drew), tint);
            }
        }
    }
}

// Trail emit: the sprite copies replayed on a copy of the (restored) main VM, or the ring strip.
#[expect(clippy::disallowed_methods, reason = "draw only")]
fn draw_trail(anm: &AnmManager, rec: &mut Recorder, e: &Enemy, tint: Option<Tint>) {
    let (f, n) = (e.trail[0], e.trail_len());
    let b = e.trail[3].max(1);
    let ring = e.trail_ring;
    let used = |i: i32| ring[table_index(i)].pos[0] >= -990.0;
    let a = i32::from(e.vm.color[anmvm::A]);
    if f & 8 == 0 {
        let mut vm = e.vm.clone();
        for c in EnemyManager::trail_copies(e) {
            EnemyManager::trail_copy(&mut vm, e.a_flags(), f, c, n, e.vm.scale, a);
            crate::emit::anm::emit_tinted(anm, rec, &vm, 2, 0.3, tint);
        }
    } else if let Some(&spr) = anm.sprite(
        table_index(i32::from(e.vm.anm_slot)),
        table_index(i32::from(e.vm.sprite_index)),
    ) {
        // One triangle strip (Anm_DrawRingStrip); unreachable with retail data.
        let mut idx = Vec::new();
        let mut i = 0;
        while i < n && used(i) {
            idx.push(i);
            i += b;
        }
        let vm = &e.vm;
        let [u0, _, u1, _] = spr.uv;
        let step = (u1 - u0) / (idx.len() as f32 - 1.0).max(1.0);
        let (va, vb) = (spr.uv[1] + vm.uv_off[1], spr.uv[3] + vm.uv_off[1]);
        let theta = |i: i32| {
            let at = |k: i32| ring[table_index(k)].angle;
            if i == 0 {
                at(0)
            } else {
                trail_mid(at(i - 1), at(i))
            }
        };
        let mut v = Vec::new();
        let mut prev = None;
        for (j, &i) in idx.iter().enumerate() {
            let th = theta(i);
            // Flag 2: an interior entry on a straight run is skipped (QUIRK: m uses the fixed index b).
            if f & 2 != 0 && i > 0 && i + b < n {
                let m = trail_mid(
                    ring[table_index(i + b - 1)].angle,
                    ring[table_index(b)].angle,
                );
                if prev.is_some_and(|p: f32| (p - th).abs() < 1e-5) && (th - m).abs() < 1e-5 {
                    continue;
                }
            }
            prev = Some(th);
            // Coordinate A steps per walked entry, skipped ones included.
            let ua = u1 + vm.uv_off[0] - step * j as f32;
            let mut h = vm.scale[1] * vm.sprite_h / 2.0;
            if f & 2 != 0 {
                h *= 1.0 - i as f32 / n as f32;
            }
            let al = lo8(if f & 4 != 0 { a - a * i / n } else { a });
            let al = tint.map_or(al, |t| t.alpha(al));
            let p = ring[table_index(i)].pos;
            let (sn, cs) = crate::fmath::sin_cos_f(th);
            let (x, y) = (p[0] + 32.0, p[1] + 16.0);
            v.push(([x - sn * h, y + cs * h], [ua, va], al));
            v.push(([x + sn * h, y - cs * h], [ua, vb], al));
        }
        if v.len() > 2 {
            match tint {
                Some(t) => crate::emit::anm::draw_ring_strip(anm, rec, &t.vm(vm), &v),
                None => crate::emit::anm::draw_ring_strip(anm, rec, vm, &v),
            }
        }
    }
}
