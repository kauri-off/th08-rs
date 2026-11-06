//! HUD emit: side panel, boss UI, dialogue (Msg_Draw) and stage-clear VMs (12 §5.3–§5.4, 10 §7).

use crate::anmmgr::AnmManager;
use crate::drawlist::{Blend, FULLSCREEN, PLAYFIELD, Recorder, Vertex};
use crate::game::View;
use crate::hud::{
    BOMBS_Y, FRONT_SLOT, GRAZE_Y, HISCORE_Y, Hud, LIVES_Y, POINT_Y, POWER_Y, SCORE_Y, TIME_Y,
    dialogue_active,
};
use crate::msg::Dialogue;
use crate::player::Stats;
use crate::types::lo8;

// Panel tile depths: score rows 0.49, other rows 0.48 (12 §5.3).
const TILE_Z_TOP: f32 = 0.49;
const TILE_Z_ROW: f32 = 0.48;

// Boss bar (12 §5.4): fill x 64 … 64 + 320·fraction, y 19..23; remaining-bars gauge x 35 … 61.
const BAR_X: f32 = 64.0;
const BAR_W: f32 = 320.0;
const BAR_Y0: f32 = 19.0;
const BAR_Y1: f32 = 23.0;
const GAUGE_X: f32 = 35.0;
const GAUGE_W: f32 = 26.0;
// Bar colours 0xRRGGBB: fill top / bottom, gauge bottom; gauge top = white − i·255/9.
const BAR_TOP: u32 = 0x00FF_FFFF;
const BAR_BOT: u32 = 0x0020_2060;
const GAUGE_BOT: u32 = 0x0020_2020;

pub fn node(ctx: &View, rec: &mut Recorder, tag: u8) {
    if let Some(h) = ctx.w.hud.as_deref() {
        hud(h, ctx, rec, tag);
    }
}

// Hud_Draw (12 §5.3) emit: Msg_Draw, Hud_DrawBoss, Hud_DrawPanel.
fn hud(h: &Hud, ctx: &View, rec: &mut Recorder, _: u8) {
    if ctx.w.game.loading != 0 {
        return;
    }
    rec.set_viewport(PLAYFIELD);
    if let Some(d) = ctx.w.msg.as_deref() {
        msg(d, &ctx.w.anm, rec);
    }
    boss(h, ctx, rec);
    if let Some(p) = ctx.w.player.as_deref() {
        panel(h, &ctx.w.anm, rec, &p.stats);
    }
}

// Msg_Draw 0x43542B (10 §7): portrait pairs back-to-front by pos.z, text box, lines, boss card (Draw2DSnap).
// Emit only: mode 0 writes no VM state (05 §9).
fn msg(d: &Dialogue, anm: &AnmManager, rec: &mut Recorder) {
    if !d.active() {
        return;
    }
    rec.set_viewport(PLAYFIELD);
    for pair in [[0usize, 1], [2, 3]] {
        let (a, b) = (pair[0], pair[1]);
        let order = if d.faces[a].pos[2] >= d.faces[b].pos[2] {
            [a, b]
        } else {
            [b, a]
        };
        for k in order {
            if d.faces[k].script.is_some() {
                crate::emit::anm::emit_mode(anm, rec, &d.faces[k], 0);
            }
        }
    }
    if d.textbox {
        let h = if d.clock.cur >= 60 {
            48.0
        } else {
            d.clock.cur as f32 * 48.0 / 60.0
        };
        // Text box (10 §7 step 2): top colour 0xD0000000, bottom 0x90000000; grows down over the first 60 clock frames.
        let (x0, x1, y0) = (PLAYFIELD.x + 16.0, PLAYFIELD.x + 368.0, 384.0);
        let (top, bot) = ([0, 0, 0, 0xD0], [0, 0, 0, 0x90]);
        bar_quad(rec, x0, x1, y0, y0 + h, top, bot);
    }
    for vm in d.lines.iter().chain(std::iter::once(&d.boss_card)) {
        if vm.script.is_some() {
            crate::emit::anm::emit_mode(anm, rec, vm, 0);
        }
    }
}

// Screen_FillRect3 0x45B490 (12 §5.4, 10 §7): untextured quad at exact D3D coordinates (+0.5 for wgpu).
fn bar_quad(rec: &mut Recorder, x0: f32, x1: f32, y0: f32, y1: f32, top: [u8; 4], bot: [u8; 4]) {
    let uv = [0.0, 0.0];
    let (x0, x1, y0, y1) = (x0 + 0.5, x1 + 0.5, y0 + 0.5, y1 + 0.5);
    let v = [
        Vertex::screen(x0, y0, 0.0, uv, top),
        Vertex::screen(x1, y0, 0.0, uv, top),
        Vertex::screen(x0, y1, 0.0, uv, bot),
        Vertex::screen(x1, y1, 0.0, uv, bot),
    ];
    rec.strip(rec.white(), Blend::Alpha, false, v);
}

// Hud_DrawBoss emit: unguarded VMs, the guarded bar part, the stage-clear VM last.
fn boss(h: &Hud, ctx: &View, rec: &mut Recorder) {
    let anm = &ctx.w.anm;
    rec.set_viewport(PLAYFIELD);
    // Stage title VMs, the stage-start clock, then the night clock with Draw2DRot (12 §5.1, §5.7).
    for vm in h.title.iter().chain(std::iter::once(&h.start_clock_vm)) {
        if vm.script.is_some() {
            crate::emit::anm::emit_mode(anm, rec, vm, 0);
        }
    }
    if h.night_vm.script.is_some() {
        crate::emit::anm::emit_mode(anm, rec, &h.night_vm, 2);
    }
    // Op 9 VMs: loading picture Draw2DSnap, capture and ghosts Draw3DQuad; impl+0x3A1C is never started (10 §9).
    if h.load_vm.script_index >= 0 {
        crate::emit::anm::emit_mode(anm, rec, &h.load_vm, 0);
        for vm in std::iter::once(&h.cap_vm).chain(h.ghosts.iter()) {
            crate::emit::anm::emit_mode(anm, rec, vm, 5);
        }
    }
    if h.tiles_live != 0 {
        for vm in &h.tiles {
            crate::emit::anm::emit_mode(anm, rec, vm, 5);
        }
    }
    if !dialogue_active(ctx.w) {
        bar(h, ctx, rec);
    }
    if h.clear_vm.script.is_some() {
        crate::emit::anm::emit_mode(anm, rec, &h.clear_vm, 0);
    }
}

// Bar part, emit: fill, ECL markers, remaining-bars gauge, the timer frame VM.
#[expect(clippy::disallowed_methods, reason = "draw only")]
fn bar(h: &Hud, ctx: &View, rec: &mut Recorder) {
    let Some(e) = ctx.w.enemies.as_deref() else {
        return;
    };
    if !h.bar_shown(e) {
        return;
    }
    let a = lo8(h.bar_alpha);
    rec.set_viewport(PLAYFIELD);
    let disp = h.bar_disp;
    bar_quad(
        rec,
        BAR_X,
        BAR_X + BAR_W * disp,
        BAR_Y0,
        BAR_Y1,
        Hud::rgb(BAR_TOP, a),
        Hud::rgb(BAR_BOT, a),
    );
    // ECL op 158 markers: drawn when end ≠ 0 and start < displayed; bottom colour (c >> 2) & 0x3F3F3F.
    for (end, start, c) in e.g.hud_markers {
        if end != 0.0 && start < disp {
            bar_quad(
                rec,
                BAR_X + BAR_W * start,
                BAR_X + BAR_W * end.min(disp),
                BAR_Y0,
                BAR_Y1,
                Hud::rgb(c, a),
                Hud::rgb((c >> 2) & 0x003F_3F3F, a),
            );
        }
    }
    // Remaining-bars gauge (+0x20, ECL op 148) between x 35 and 61.
    let n = e.g.hud_bars.max(0);
    let gap = if n < 6 { 2.0 } else { 1.0 };
    for i in 0..n {
        let x0 = GAUGE_X + GAUGE_W * i as f32 / n as f32;
        let x1 = GAUGE_X + GAUGE_W * (i + 1) as f32 / n as f32 - gap;
        let c = Hud::rgb(BAR_TOP - (i.cast_unsigned() * 255 / 9), a);
        bar_quad(rec, x0, x1, BAR_Y0, BAR_Y1, c, Hud::rgb(GAUGE_BOT, a));
    }
    crate::emit::anm::emit_mode(&ctx.w.anm, rec, &h.front[12], 0);
}

// Hud_DrawPanel emit; flushes and sets the full-screen viewport first.
fn panel(h: &Hud, anm: &AnmManager, rec: &mut Recorder, st: &Stats) {
    rec.set_viewport(FULLSCREEN);
    // Tiles (scripts 13–15) keep z-write on; depths 0.49 / 0.48 / 0.46 (12 §5.3).
    let spr = |rec: &mut Recorder, i: usize, x: f32, y: f32, z: f32| {
        if let Some(s) = anm.sprite(FRONT_SLOT, i) {
            let [u0, v0, u1, v1] = s.uv;
            // Draw2DSnap → EmitQuad: the global tint applies (12 §5.3, 05 §12.1); the gauge quad stays raw.
            let c = anm.tinted([255; 4]);
            let v = |x: f32, y: f32, u: f32, vv: f32| Vertex::screen(x, y, z, [u, vv], c);
            rec.strip(
                s.tex,
                Blend::Alpha,
                true,
                [
                    v(x, y, u0, v0),
                    v(x + s.w, y, u1, v0),
                    v(x, y + s.h, u0, v1),
                    v(x + s.w, y + s.h, u1, v1),
                ],
            );
        }
    };
    for y in [HISCORE_Y, SCORE_Y] {
        spr(rec, 15, 480.0, y, TILE_Z_TOP);
    }
    for y in [LIVES_Y, BOMBS_Y, POWER_Y, GRAZE_Y, POINT_Y, TIME_Y] {
        spr(rec, 15, 480.0, y, TILE_Z_ROW);
    }
    // Always: the corner under the fps text.
    spr(rec, 15, 512.0, 464.0, TILE_Z_ROW);
    // Full repaint (cfg bit 12, tile VM 13 still running = first 200 frames, or counter 0x17CE8CC ≠ 0) and the
    // dirty rows are not tracked: the port clears the screen every frame, so it repaints in full every frame.
    for y in (0..480).step_by(32) {
        spr(rec, 13, 0.0, y as f32, TILE_Z_TOP);
    }
    for x in (416..640).step_by(32) {
        for y in (16..480).step_by(32) {
            spr(rec, 13, x as f32, y as f32, TILE_Z_TOP);
        }
    }
    for x in (0..640).step_by(128) {
        spr(rec, 14, x as f32, 0.0, TILE_Z_TOP);
        spr(rec, 14, x as f32, 464.0, TILE_Z_TOP);
    }
    for vm in h.front.iter().take(10) {
        crate::emit::anm::emit_mode(anm, rec, vm, if vm.script_index == 1 { 2 } else { 0 });
    }
    crate::emit::anm::emit_mode(anm, rec, &h.diff_vm, 0);
    for (row, vm, n) in Hud::stock_rows(st) {
        for i in 0..n {
            let mut v = h.front[vm].clone();
            v.pos = Hud::stock_pos(row, i);
            crate::emit::anm::emit_mode(anm, rec, &v, 0);
        }
    }
    if h.label_latch {
        crate::emit::anm::emit_mode(anm, rec, &h.label_vm, 0);
    }
    // Strip 14 along y = 464 for x 32..368 every frame (12 §5.3).
    for x in [32.0, 160.0, 288.0] {
        spr(rec, 14, x, 464.0, TILE_Z_TOP);
    }
    // Power gauge x 488 … 488 + power, y 136..152, left 0xE0E0E0FF / right 0x80E0E0FF (12 §5.3).
    let pw = st.power.trunc();
    let uv = [0.0, 0.0];
    let (l, r) = ([0xE0, 0xE0, 0xFF, 0xE0], [0xE0, 0xE0, 0xFF, 0x80]);
    rec.strip(
        rec.white(),
        Blend::Alpha,
        false,
        [
            // D3D coordinates (+0.5 for wgpu) and z 0.1, as in the original's draw log.
            Vertex::screen(488.5, 136.5, 0.1, uv, l),
            Vertex::screen(488.5 + pw, 136.5, 0.1, uv, r),
            Vertex::screen(488.5, 152.5, 0.1, uv, l),
            Vertex::screen(488.5 + pw, 152.5, 0.1, uv, r),
        ],
    );
}
