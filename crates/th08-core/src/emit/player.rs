//! Player draw emit: shots, options, the sprite and the bomb draw functions (09 §1, 14 §2–§5).

use crate::anmmgr::{AnmManager, Tint};
use crate::anmvm::{self, AnmVm};
use crate::bomb::{OBJS, ObjState, RGB_RED, RGB_WHITE};
use crate::drawlist::{PLAYFIELD, Recorder};
use crate::game::View;
use crate::player::Player;
use crate::types::{D3dColor, lo8};

pub fn node(ctx: &View, rec: &mut Recorder, tag: u8) {
    let anm = &ctx.w.anm;
    let p = ctx.w.player.as_deref().expect("player");
    rec.set_viewport(PLAYFIELD);
    if tag == 0 {
        bullets(p, anm, rec, 1, p.bullet_tint);
        bomb(p, anm, rec);
        sprite(p, anm, rec, ctx.w.game.game_over != 0);
        options(p, anm, rec);
    } else {
        bullets(p, anm, rec, 2, None);
    }
}

fn sprite(p: &Player, anm: &AnmManager, rec: &mut Recorder, game_over: bool) {
    if p.vm.script.is_some() && !game_over {
        crate::emit::anm::emit_mode_z(anm, rec, &p.vm, 0, 0.1);
    }
}

fn options(p: &Player, anm: &AnmManager, rec: &mut Recorder) {
    for o in p
        .options
        .iter()
        .filter(|o| o.state != 0 && o.vm.script.is_some())
    {
        crate::emit::anm::emit_mode_z(anm, rec, &o.vm, 2, 0.49);
    }
}

// Emit of `step_bullets`. `tint`: the global tint the flying bullets were drawn under when a later write of the
// same draw node (the bomb's tint off, 14 §1.3) turned it off (05 §12.1).
fn bullets(p: &Player, anm: &AnmManager, rec: &mut Recorder, state: u8, tint: Option<Tint>) {
    let youkai_max = p.stats.youkai_max();
    let emit = |rec: &mut Recorder, vm: &AnmVm, mode: i32, z: f32| {
        crate::emit::anm::emit_tinted(anm, rec, vm, mode, z, tint);
    };
    for b in p
        .bullets
        .iter()
        .filter(|b| b.state == state && b.vm.script.is_some())
    {
        if state == 2 {
            emit(rec, &b.vm, b.vm.draw_mode, 0.2);
            continue;
        }
        if b.draw != 1 {
            emit(rec, &b.vm, 2, 0.4);
            continue;
        }
        // The step left the trail's last pos / red in the VM: rebuild the main draw's state on a copy.
        let mut vm = b.vm.clone();
        vm.pos = [b.pos[0] + 32.0, b.pos[1] + 16.0, 0.0];
        (vm.color[anmvm::R], vm.color[anmvm::G], vm.color[anmvm::B]) =
            (b.main_rgb[0], b.main_rgb[1], b.main_rgb[2]);
        emit(rec, &vm, 2, 0.4);
        Player::trail_points(&mut vm, b, youkai_max, |vm, z| emit(rec, vm, 2, z));
    }
}

fn objs(p: &Player, anm: &AnmManager, rec: &mut Recorder, n: usize, mode: i32) {
    for o in p.bomb.objs[..n]
        .iter()
        .filter(|o| o.state != ObjState::Free)
    {
        crate::emit::anm::emit_mode_z(anm, rec, &o.vm, mode, 0.0);
    }
}

// Alpha-blended playfield rectangle (Alice LS, Youmu draw fns).
fn playfield_rect(rec: &mut Recorder, rgb: D3dColor, a: i32) {
    let r = PLAYFIELD;
    rec.rect(
        r.x,
        r.y,
        r.w,
        r.h,
        [rgb.r(), rgb.g(), rgb.b(), lo8(a.clamp(0, 255))],
    );
}

// Emit of `step_bomb`. Quirk kept: Sakuya LS draws only knives 0–95 (96–127 are invisible but damage).
fn bomb(p: &Player, anm: &AnmManager, rec: &mut Recorder) {
    if !p.bombing || p.bomb.index == 4 {
        return;
    }
    let t = p.bomb.timer.cur;
    let ls = p.bomb.index & 2 != 0;
    match p.bomb.ch {
        0 => objs(p, anm, rec, if ls { OBJS } else { 16 }, 0),
        1 if ls => {
            for (vm, z) in p.bomb.vms[..2].iter().zip([0.01, 0.0]) {
                crate::emit::anm::emit_mode_z(anm, rec, vm, 2, z);
            }
        }
        2 => p
            .bomb
            .vms
            .iter()
            .for_each(|vm| crate::emit::anm::emit_mode_z(anm, rec, vm, 2, 0.0)),
        3 if ls && (90..=120).contains(&t) => {
            playfield_rect(rec, RGB_WHITE, (t - 90) * 255 / 30);
        }
        3 if ls && (121..=220).contains(&t) => playfield_rect(rec, RGB_WHITE, 0x70),
        4 => objs(p, anm, rec, 96, 2),
        6 if (70..160).contains(&t) => {
            let a = if t <= 100 {
                255
            } else {
                255 - (t - 100) * 255 / 60
            };
            playfield_rect(rec, if ls { RGB_RED } else { RGB_WHITE }, a);
        }
        7.. => objs(p, anm, rec, if ls { OBJS } else { 96 }, 2),
        _ => {}
    }
}
