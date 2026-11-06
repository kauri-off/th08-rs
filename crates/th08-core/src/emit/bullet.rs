//! Bullet manager emit: items, lasers, bullets by size class and effect list D (08 §1, §6.3, §8.6).

use crate::anmmgr::{AnmManager, Tint};
use crate::bullet::{BulletManager, FROZEN_TINT};
use crate::drawlist::{PLAYFIELD, Recorder};
use crate::game::View;
use crate::item::ItemManager;
use crate::player::Player;

pub fn node(ctx: &View, rec: &mut Recorder, _: u8) {
    let p = ctx.w.player.as_deref().expect("player");
    let b = ctx.w.bullets.as_deref().expect("bullet manager");
    manager(b, &ctx.w.anm, rec, &ctx.w.effects, p, ctx.w.game.flags);
}

fn manager(
    m: &BulletManager,
    anm: &AnmManager,
    rec: &mut Recorder,
    fx: &crate::effect::Effects,
    p: &Player,
    flags: u32,
) {
    rec.set_viewport(PLAYFIELD);
    // The frozen tint is off again when the emit runs: the emit applies it to VM copies.
    let tint = p.freeze_bullets(flags).then_some(Tint(FROZEN_TINT));
    items(&m.items, anm, rec, tint);
    lasers(m, anm, rec, tint);
    for c in 0..6 {
        for &i in m.lists[c].iter().rev() {
            let b = &m.bullets[i.idx()];
            if let Some(v) = BulletManager::draw_vm(b) {
                crate::emit::anm::emit_tinted(anm, rec, &b.vms[v], 2, 0.05, tint);
            }
        }
    }
    match tint {
        Some(t) => crate::emit::effect::over_bullets_with(fx, |vm| t.vm(vm), anm, rec),
        None => crate::emit::effect::over_bullets(fx, anm, rec),
    }
}

// Laser draw emit: body z 0.06, emitter z 0.05.
fn lasers(m: &BulletManager, anm: &AnmManager, rec: &mut Recorder, tint: Option<Tint>) {
    for l in m.lasers.iter().filter(|l| l.alive) {
        crate::emit::anm::emit_tinted(anm, rec, &l.body, 2, 0.06, tint);
        if l.emitter_drawn() {
            crate::emit::anm::emit_tinted(anm, rec, &l.emitter, 2, 0.05, tint);
        }
    }
}

// Item draw emit: Draw2DRot at z 0.15, in list order.
fn items(m: &ItemManager, anm: &AnmManager, rec: &mut Recorder, tint: Option<Tint>) {
    for &i in &m.order {
        let it = &m.items[i.idx()];
        if it.alive {
            crate::emit::anm::emit_tinted(anm, rec, &it.vm, 2, 0.15, tint);
        }
    }
}
