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
    r[26] = i64::from(g2.scene as i32);
    r[27] = scene_frame;
    r[28] = i64::from(menu);
    r[29] = i64::from(cursor);
    r[30] = i64::from(result);
    r[31] = i64::from(g2.paused);
    r[32] = i64::from(ctx.w.input.held);
    r[33] = col(g2.stage);
    r[34] = i64::from(g2.loading);
    r[35] = if g2.has(crate::gamemgr::GameFlags::DEMO) {
        i64::from(g2.demo)
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
    row[0] = i64::from(g.frame_counter);
    row[1] = i64::from(g.demo_frame);
    row[2] = i64::from(ctx.w.input.game[0]);
    row[3] = i64::from(ctx.w.rng.state);
    row[4] = i64::from(ctx.w.rng.total.wrapping_sub(rng_base));
    row[18] = i64::from(g.rank.rank);
    row[21] = i64::from(g.flags);
    let Some(p) = ctx.w.player.as_deref() else {
        return row;
    };
    if ctx.w.gm.is_none() {
        return row;
    }
    let st = &p.stats;
    row[5] = i64::from(p.pos[0].to_bits());
    row[6] = i64::from(p.pos[1].to_bits());
    row[7] = i64::from(p.state);
    row[8] = st.score10;
    row[9] = i64::from(st.graze_total);
    row[10] = i64::from(st.point_items2);
    row[11] = i64::from(st.lives.to_bits());
    row[12] = i64::from(st.bombs.to_bits());
    row[13] = i64::from(st.power.to_bits());
    row[14] = i64::from(st.gauge);
    row[15] = i64::from(st.time);
    if let Some(b) = ctx.w.bullets.as_deref() {
        row[16] = col(b.live);
    }
    if let Some(e) = ctx.w.enemies.as_deref() {
        row[17] = col(e
            .enemies
            .iter()
            .filter(|x| x.flags & crate::enemy::A_ALIVE != 0)
            .count());
        row[19] = i64::from(e.g.spell.is_some());
        row[20] = e
            .bosses
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_some())
            .fold(0, |a, (i, _)| a | 1 << i);
    }
    row
}

// A slot index or count as a trace column (all < 2^31).
#[expect(clippy::cast_possible_wrap, reason = "trace column of a small count")]
fn col(n: usize) -> i64 {
    n as i64
}
