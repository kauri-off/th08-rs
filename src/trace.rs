//! Trace sampling of the port's state (specs/data/drawlog/README.md "Sessions", "Trace"): the 36-column session row,
//! the 22-column demo row, the recorded loader schedule and the chain hooks that write per-pass rows. Pure reads of
//! `ctx.w`; the session / demosync runners, parsers and comparisons are tools-side.

use crate::game::Ctx;
use std::collections::VecDeque;

pub type Row = [i64; 36];

// Loader steps of the original, by poll index (0 = first main-menu frame).
#[derive(Clone, Debug, Default)]
pub struct Sched {
    // Port calc (1-based, = present) of the first main-menu frame: `release boot calc C` + the 2 Window_InitD3D presents.
    pub boot_calc: u32,
    pub menu: VecDeque<i64>,
    // (gauge setup, Loading_Wait, release) per stage start.
    pub stages: VecDeque<[i64; 3]>,
    // Poll index of the calc pass being run (negative during boot); advances on every chain restart (00 §3).
    pub poll: i64,
    // Rows written inside this frame's calc (restarted passes, in-calc presents with ff −1), taken by `Session::run`.
    pub pass_rows: Vec<Row>,
    // Start value of the ASCII frame counter +0x8284 (12 §5.10 slow boots): `asciicalc − poll` of the first
    // `loader gauge` line when positive, else 0.
    pub ascii_offset: u32,
}

// FuncChain_RunCalc restart (00 §3, README "Trace"): the probe writes a row per pass, and the next pass polls again.
pub fn on_restart(ctx: &mut Ctx) {
    let Some(poll) = ctx.w.game.sched.as_ref().map(|s| s.poll) else {
        return;
    };
    let r = sample(ctx, 0, poll, 0);
    if let Some(s) = &mut ctx.w.game.sched {
        s.pass_rows.push(r);
        s.poll += 1;
    }
}

// Device_ClearPresentTwice (16 §1.2): a present inside a calc gets its own trace row, ff = −1 (README "Trace").
pub fn on_extra_present(ctx: &mut Ctx) {
    let Some(poll) = ctx.w.game.sched.as_ref().map(|s| s.poll) else {
        return;
    };
    let mut r = sample(ctx, 0, poll, 0);
    r[22] = -1;
    if let Some(s) = &mut ctx.w.game.sched {
        s.pass_rows.push(r);
    }
}

// The port's state in trace column order, after the calc of present `present`; `calc` (col 24) is the caller's.
pub fn sample(ctx: &mut Ctx, present: i64, poll: i64, scene_frame: i64) -> Row {
    let mut r = [0i64; 36];
    let d = demo_sample(ctx, 0);
    r[..22].copy_from_slice(&d);
    let (menu, cursor) = ctx.w.game.trace_menu.take().unwrap_or((-1, -1));
    let result = ctx.w.game.trace_result.take().unwrap_or(-1);
    let g2 = &ctx.w.game;
    r[22] = 0;
    r[23] = present;
    r[25] = poll + 1;
    r[26] = g2.scene as i64;
    r[27] = scene_frame;
    r[28] = menu as i64;
    r[29] = cursor as i64;
    r[30] = result as i64;
    r[31] = g2.paused as i64;
    r[32] = ctx.w.input.held as i64;
    r[33] = g2.stage as i64;
    r[34] = g2.loading as i64;
    r[35] = if g2.flags & crate::gamemgr::F_DEMO != 0 {
        g2.demo as i64
    } else {
        -1
    };
    r
}

// The port's state in demo-trace column order (specs/data/demoN_trace.txt, session columns 0–21); `rng_ctr` counts
// from `rng_base`.
pub fn demo_sample(ctx: &Ctx, rng_base: u32) -> [i64; 22] {
    let g = &ctx.w.game;
    let mut row = [0i64; 22];
    row[0] = g.frame_counter as i64;
    row[1] = g.demo_frame as i64;
    row[2] = ctx.w.input.game[0] as i64;
    row[3] = ctx.w.rng.state as i64;
    row[4] = ctx.w.rng.total.wrapping_sub(rng_base) as i64;
    row[18] = g.rank.rank as i64;
    row[21] = g.flags as i64;
    let Some(p) = ctx.w.player.as_deref() else {
        return row;
    };
    if ctx.w.gm.is_none() {
        return row;
    }
    let st = &p.stats;
    row[5] = p.pos[0].to_bits() as i64;
    row[6] = p.pos[1].to_bits() as i64;
    row[7] = p.state as i64;
    row[8] = st.score10;
    row[9] = st.graze_total as i64;
    row[10] = st.point_items2 as i64;
    row[11] = st.lives.to_bits() as i64;
    row[12] = st.bombs.to_bits() as i64;
    row[13] = st.power.to_bits() as i64;
    row[14] = st.gauge as i64;
    row[15] = st.time as i64;
    if let Some(b) = ctx.w.bullets.as_deref() {
        row[16] = b.live as i64;
    }
    if let Some(e) = ctx.w.enemies.as_deref() {
        row[17] = e
            .enemies
            .iter()
            .filter(|x| x.flags & crate::enemy::A_ALIVE != 0)
            .count() as i64;
        row[19] = e.g.spell.is_some() as i64;
        row[20] = e
            .bosses
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_some())
            .fold(0, |a, (i, _)| a | 1 << i);
    }
    row
}
