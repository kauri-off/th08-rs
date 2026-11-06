//! ASCII manager emit: strings, popups, darkness, gauge, boss markers, pause / continue (12 §2.9, §3–§5.10; 13 §4).

use crate::anmvm::{self, AnmVm};
use crate::ascii::{
    ANM_SLOT, ANM_SLOT_I16, AsciiManager, Darkness, G_BAR, G_GLYPH, G_HUMAN, G_MARKER, G_YOUKAI,
    Gauge, GaugeDraw, Overlays, Text, darkness_rects, gauge_glyphs, marker_shown, popup_alpha,
    set_rgb, short,
};
use crate::drawlist::{Blend, FULLSCREEN, Recorder};
use crate::game::View;
use crate::types::lo8;

pub fn node(ctx: &View, rec: &mut Recorder, tag: u8) {
    if let Some(a) = ctx.w.ascii_mgr.as_deref() {
        manager(a, ctx, rec, tag);
    }
}

// Ascii draw nodes (prio 14 / 20) emit: pause and continue menus, popups, gauge, strings, markers.
fn manager(a: &AsciiManager, ctx: &View, rec: &mut Recorder, tag: u8) {
    let high = tag == 1;
    // Pause draw 0x404750 (12 §3): playfield viewport; the capture VM (temporary copy, z-write off) while
    // the state ≠ 0; then menu VMs 0..9, all Draw2DSnap.
    if high && ctx.w.game.paused != 0 {
        rec.set_viewport(crate::drawlist::PLAYFIELD);
        if a.pause.state != 0 && a.pause.cap_vm.script.is_some() {
            let mut v = a.pause.cap_vm.clone();
            v.flags |= anmvm::F_ZWRITE_OFF;
            crate::emit::anm::emit_mode(&ctx.w.anm, rec, &v, 0);
        }
        // The selected item's (−4, −4) goes to the undrawn pos_offset: no visible shift (12 §3).
        for vm in &a.pause.vms {
            crate::emit::anm::emit_mode(&ctx.w.anm, rec, vm, 0);
        }
    }
    // ContinueMenu_Draw 0x4052B0 (12 §4): playfield viewport, capture VM, then VMs 0..3 (clock VM 3 only in a
    // normal game below Extra), all Draw2DSnap. The port's menu is past state 0 after its first calc.
    if high && ctx.w.game.game_over != 0 && a.cont.open {
        rec.set_viewport(crate::drawlist::PLAYFIELD);
        let g = &ctx.w.game;
        let clock = g.difficulty < 4 && !g.spell_practice();
        let c = &a.cont;
        for vm in [&c.cap_vm]
            .into_iter()
            .chain(c.vms.iter())
            .chain(clock.then_some(&c.clock_vm))
        {
            crate::emit::anm::emit_mode(&ctx.w.anm, rec, vm, 0);
        }
    }
    if !high {
        popups_emit(
            &ctx.w.overlays,
            &ctx.w.anm,
            rec,
            ctx.w.effects.player,
            a.popup_off,
        );
        if let Some(d) = &a.gauge {
            gauge_emit(&ctx.w.overlays.gauge, &ctx.w.anm, rec, d);
        }
    }
    emit_texts(&ctx.w.anm, rec, &a.texts[usize::from(high)]);
    if high {
        markers_emit(&ctx.w.overlays.gauge, &ctx.w.anm, rec);
    }
}

// Ascii_DrawStrings 0x402B20 (12 §2.9) emit: '\n' → y += 16·scale y, ' ' → advance only, other bytes Draw2DSnap.
pub fn emit_texts(ctx_anm: &crate::anmmgr::AnmManager, rec: &mut Recorder, items: &[Text]) {
    let o = ctx_anm.screen_offset;
    for t in items {
        let vp = if t.clip {
            crate::drawlist::PLAYFIELD
        } else {
            FULLSCREEN
        };
        rec.set_viewport(vp);
        let (mut x, mut y) = (t.x + o[0], t.y + o[1]);
        // Chars U+0000-U+00FF are glyph bytes (U+0080 / U+0081 = the "SP" / "END" cells, 16 §2.2).
        for c in t.s.chars().map(|c| c as u32 as u8) {
            if c == b'\n' {
                (x, y) = (t.x + o[0], y + 16.0 * t.scale_y);
                continue;
            }
            let (idx, col) = if t.hl {
                (c as usize + 0x8A, [255; 4])
            } else {
                (c.wrapping_sub(1) as usize, t.color)
            };
            // Draw2DSnap draws nothing for alpha 0 (05 §9); the pen still advances.
            let drawn = c > b' ' && col[3] != 0;
            // Glyphs are EmitQuad quads: the global ANM tint applies (12 §2.9, 05 §12.1).
            let col = ctx_anm.tinted(col);
            if let Some(sp) = ctx_anm.sprite(ANM_SLOT, idx).filter(|_| drawn) {
                // Glyph VM: z 0, z-write on (12 §5.3 depth values).
                let [u0, v0, u1, v1] = sp.uv;
                let (w, h) = (sp.w * t.scale, sp.h * t.scale_y);
                // Draw2DSnap culls quads outside the viewport (05 §9 EmitQuad step 3).
                let (rx, ry) = (x.round_ties_even() - 0.5, y.round_ties_even() - 0.5);
                if rx + w < vp.x || rx > vp.x + vp.w || ry + h < vp.y || ry > vp.y + vp.h {
                    x += t.advance * t.scale;
                    continue;
                }
                let v = |x: f32, y: f32, u: f32, vv: f32| {
                    crate::drawlist::Vertex::screen(
                        x.round_ties_even(),
                        y.round_ties_even(),
                        0.0,
                        [u, vv],
                        col,
                    )
                };
                rec.strip(
                    sp.tex,
                    Blend::Alpha,
                    true,
                    [
                        v(x, y, u0, v0),
                        v(x + w, y, u1, v0),
                        v(x, y + h, u0, v1),
                        v(x + w, y + h, u1, v1),
                    ],
                );
            }
            x += t.advance * t.scale;
        }
    }
}

// Ascii_DrawPopups 0x405420 popup part (13 §4) emit: small popups, the darkness mask (§4.1), then large ones, all
// Draw2DSnap in the playfield viewport. `pl`: the effect manager's player-position mirror; `off`: the shake offset
// before the draw zeroes it. The glyph VMs are rebuilt per draw (locals of the original), so the emit owns them.
fn popups_emit(
    ov: &Overlays,
    anm: &crate::anmmgr::AnmManager,
    rec: &mut Recorder,
    pl: [f32; 3],
    off: [f32; 2],
) {
    rec.set_viewport(crate::drawlist::PLAYFIELD);
    let mut small = popup_vm(anm, 0, 0.1);
    let mut large = popup_vm(anm, 0x88, 0.0);
    let draw = |rec: &mut Recorder, vm: &AnmVm| {
        crate::emit::anm::emit_mode_z_at(anm, rec, vm, 0, vm.pos[2], off);
    };
    let p = &ov.popups;
    for q in p.slots.iter().flatten() {
        set_rgb(&mut small, q.colour);
        small.color[anmvm::A] = popup_alpha(q.pos, pl);
        // Digit glyph g → sprite g (age < 52), g + 11 (52–55), g + 21 (≥ 56).
        let shift = if q.age.cur < 52 {
            0
        } else if q.age.cur < 56 {
            11
        } else {
            21
        };
        // x starts at pos.x − 4·digits, advance 8, most significant (last stored) first.
        let x0 = q.pos[0] - 4.0 * q.digits.len() as f32;
        for (k, &g) in q.digits.iter().rev().enumerate() {
            if popup_glyph(anm, &mut small, (g + shift) as usize) {
                small.pos[0] = x0 + 8.0 * k as f32;
                small.pos[1] = q.pos[1];
                draw(rec, &small);
            }
        }
    }
    darkness_emit(&ov.darkness, anm, rec, pl, off);
    for q in p.large.iter().flatten() {
        set_rgb(&mut large, q.colour);
        large.color[anmvm::A] = popup_alpha(q.pos, pl);
        large.scale = [q.scale; 2];
        // x0 = pos.x − count·3.5 (count = stored glyphs incl. '+', unscaled), advance 7·scale.x, sprite 0x88 + glyph.
        let x0 = q.pos[0] - q.glyphs.len() as f32 * 3.5;
        for (k, &g) in q.glyphs.iter().rev().enumerate() {
            if popup_glyph(anm, &mut large, 0x88 + g as usize) {
                large.pos[0] = x0 + 7.0 * q.scale * k as f32;
                large.pos[1] = q.pos[1];
                draw(rec, &large);
            }
        }
    }
}

// Popup VMs Ascii +0x2A4 (sprite 0, 8×8) / +0x548 (sprite 0x88, 8×12) after Ascii_InitVms 0x402450; AnmVm_Init
// resets the flags to 7, so the 0x1800 anchor bits ORed in before are lost and the glyphs are centred (QUIRK, 13 §4).
fn popup_vm(anm: &crate::anmmgr::AnmManager, sprite: i32, z: f32) -> AnmVm {
    let mut vm = AnmVm {
        anm_slot: ANM_SLOT_I16,
        ..AnmVm::default()
    };
    vm.init();
    vm.set_sprite(anm, sprite);
    vm.pos[2] = z;
    vm
}

// Per glyph only the sprite width is copied; the height stays the init sprite's (13 §4).
fn popup_glyph(anm: &crate::anmmgr::AnmManager, vm: &mut AnmVm, idx: usize) -> bool {
    let Some(sp) = anm.sprite(ANM_SLOT, idx) else {
        return false;
    };
    vm.sprite_index = short(idx);
    vm.sprite_w = sp.w;
    true
}

fn darkness_emit(
    d: &Darkness,
    anm: &crate::anmmgr::AnmManager,
    rec: &mut Recorder,
    player: [f32; 3],
    off: [f32; 2],
) {
    if d.alpha <= 0 {
        return;
    }
    let a = lo8(d.alpha);
    for [x0, y0, x1, y1] in darkness_rects([player[0], player[1]], d.radius, off) {
        rec.rect(x0, y0, x1 - x0, y1 - y0, [0, 0, 0, a]);
    }
    crate::emit::anm::emit_mode_z_at(anm, rec, &d.vm, 0, d.vm.pos[2], off);
}

// Marker G3 (Draw2DRotNoSnap, 12 §5.9 step 1), G0–G2, then the G4 glyphs.
fn gauge_emit(g: &Gauge, anm: &crate::anmmgr::AnmManager, rec: &mut Recorder, d: &GaugeDraw) {
    crate::emit::anm::emit_mode(anm, rec, &g.vms[G_MARKER], 3);
    for k in [G_BAR, G_HUMAN, G_YOUKAI] {
        crate::emit::anm::emit_mode(anm, rec, &g.vms[k], 0);
    }
    // Every glyph rewrites sprite, pos and scale, and the colour is fixed: replaying on a copy draws the same quads.
    let mut vm = g.vms[G_GLYPH].clone();
    gauge_glyphs(anm, &mut vm, d, Some(rec));
}

fn markers_emit(g: &Gauge, anm: &crate::anmmgr::AnmManager, rec: &mut Recorder) {
    rec.set_viewport(FULLSCREEN);
    for m in g.markers.iter().filter(|m| marker_shown(m)) {
        crate::emit::anm::emit_mode(anm, rec, m, 0);
    }
}
