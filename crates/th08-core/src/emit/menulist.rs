//! Main-menu list screen emits: spell practice pies and percent glyphs, card list, replay card name (16 §6.4).

use crate::anmvm::AnmVm;
use crate::ascii::{percent_glyphs, uidx};
use crate::drawlist::{Blend, Recorder, Vertex};
use crate::fmath;
use crate::game::View;
use crate::menu::MainMenu;
use crate::menulist::{PERCENTS, pct};

// Emit of 0x46F5CB: scratch 0, scratch 1 / 2, then pies and percent glyphs interleaved (16 §6.4 steps 1, 4, 6).
pub fn spell_char(m: &MainMenu, ctx: &View, rec: &mut Recorder) {
    if m.spell_char_own() {
        emit0(ctx, rec, &m.lists.scratch[0]);
        emit0(ctx, rec, &m.lists.scratch[1]);
        emit0(ctx, rec, &m.lists.scratch[2]);
    }
    let v = m.lists.pies;
    let mut glyph = m.lists.glyph.clone();
    let mut percent = |rec: &mut Recorder, (k, z, c): (usize, [f32; 2], [u8; 4])| {
        percent_glyphs(
            &ctx.w.anm,
            &mut glyph,
            pct(v[k]),
            [z[0], z[1], 0.01],
            c,
            |vm| emit0(ctx, rec, vm),
        );
    };
    pie(rec, [530.0, 420.0], 24.0, 0.02, 0x60C0_C0F0, v[0]);
    pie(rec, [530.0, 420.0], 12.0, 0.01, 0x80C0_C0F0, v[1]);
    percent(rec, PERCENTS[0]);
    percent(rec, PERCENTS[1]);
    pie(rec, [562.0, 438.0], 16.0, 0.02, 0x6080_C0C0, v[2]);
    pie(rec, [562.0, 438.0], 8.0, 0.01, 0x8080_C0C0, v[3]);
    percent(rec, PERCENTS[2]);
    percent(rec, PERCENTS[3]);
}

pub fn spell_card(m: &MainMenu, ctx: &View, rec: &mut Recorder) {
    if !m.spell_card_own() {
        return;
    }
    let (a, b) = m.card_ids();
    emit0(ctx, rec, &m.lists.header);
    for j in 0..15 {
        if a + uidx(m.lists.page) * 15 + j >= b {
            break;
        }
        emit0(ctx, rec, &m.lists.scratch[j]);
    }
    for vm in &m.lists.info {
        emit0(ctx, rec, vm);
    }
}

pub fn replay(m: &MainMenu, ctx: &View, rec: &mut Recorder) {
    if let (2 | 3, Some(i)) = (m.lists.rpy_sub, m.lists.rpy_loaded)
        && m.replays[i].1.spell_id() >= 0
    {
        emit0(ctx, rec, &m.notice);
    }
}

fn emit0(ctx: &View, rec: &mut Recorder, vm: &AnmVm) {
    crate::emit::anm::emit_mode(&ctx.w.anm, rec, vm, 0);
}

// Menu_DrawPie 0x46FDC2 → AnmManager_DrawFan: 64-vertex fan from 12 o'clock, clockwise over v·360°.
fn pie(rec: &mut Recorder, c: [f32; 2], r: f32, z: f32, argb: u32, v: f32) {
    let col = [
        (argb >> 16) as u8,
        (argb >> 8) as u8,
        argb as u8,
        (argb >> 24) as u8,
    ];
    let p = |x: f32, y: f32| Vertex::screen(x + 0.5, y + 0.5, z, [0.0, 0.0], col);
    let mut verts = vec![p(c[0], c[1])];
    let step = 0.101_342 * v;
    for k in 0..63 {
        let a = -std::f32::consts::FRAC_PI_2 + k as f32 * step;
        verts.push(p(c[0] + r * fmath::cos_f(a), c[1] + r * fmath::sin_f(a)));
    }
    rec.fan(Blend::Alpha, &verts);
}
