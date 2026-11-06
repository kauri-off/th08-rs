//! ECL variables: ids 10000–10100, getters 0x41F420 / 0x420120 and references 0x41FE10 / 0x420950 (07 §6).

use crate::enemy::{
    Context, Enemy, EnemyManager, MAX_ENEMIES, V_ARGF, V_ARGI, V_FLT0, V_FLT1, V_INT0, V_INT1,
};
use crate::types::angle_from;
use std::f32::consts::PI;

// Offset of `id` in a variable range starting at `base` (callers match the range first, so it is ≥ 0).
fn off(id: i32, base: i32) -> usize {
    (id - base).cast_unsigned() as usize
}

// Writable int variables.
#[derive(Clone, Copy)]
pub(super) enum IntVar {
    Ctx(usize),
    EInt(usize),
    Difficulty,
    Rank,
    F2e1c,
    Life,
    GArg(usize),
    Item,
    Score,
}

// Writable float variables; 10045–47 (player pos) are writable (!).
#[derive(Clone, Copy)]
pub(super) enum FltVar {
    Ctx(usize),
    EFlt(usize),
    Pos(usize),
    Player(usize),
    GArg(usize),
    Angle,
    AngVel,
    Speed,
    Accel,
    Radius,
    Start(usize),
    OrbitAngle,
    OrbitAv,
    Delta(usize),
}

impl IntVar {
    pub(super) fn from_id(id: i32) -> Option<Self> {
        Some(match id {
            10000..=10007 => Self::Ctx(V_INT0 + off(id, 10000)),
            10008..=10015 => Self::EInt(off(id, 10008)),
            10036..=10039 => Self::Ctx(V_INT1 + off(id, 10036)),
            10040 => Self::Difficulty,
            10041 => Self::Rank,
            10049 => Self::F2e1c,
            10051 => Self::Life,
            10053..=10056 => Self::Ctx(V_ARGI + off(id, 10053)),
            10061..=10064 => Self::GArg(off(id, 10061)),
            10092 => Self::Item,
            10093 => Self::Score,
            _ => return None,
        })
    }
}

impl FltVar {
    pub(super) fn from_id(id: i32) -> Option<Self> {
        Some(match id {
            10016..=10023 => Self::Ctx(V_FLT0 + off(id, 10016)),
            10024..=10031 => Self::EFlt(off(id, 10024)),
            10042..=10044 => Self::Pos(off(id, 10042)),
            10045..=10047 => Self::Player(off(id, 10045)),
            10057..=10060 => Self::Ctx(V_ARGF + off(id, 10057)),
            10065..=10068 => Self::GArg(4 + off(id, 10065)),
            10069 => Self::Angle,
            10070 => Self::AngVel,
            10071 => Self::Speed,
            10072 => Self::Accel,
            10073 => Self::Radius,
            10074..=10076 => Self::Start(off(id, 10074)),
            10077 => Self::OrbitAngle,
            10078 => Self::OrbitAv,
            10079..=10081 => Self::Delta(off(id, 10079)),
            10094 | 10095 => Self::Ctx(V_FLT1 + off(id, 10094)),
            _ => return None,
        })
    }
}

// Variable getters evaluated on any enemy/context (also used for boss queries, ops 86/87).
pub(crate) fn var_f(
    m: &mut EnemyManager,
    rng: &mut crate::rng::Rng,
    e: &Enemy,
    c: &Context,
    lit: f32,
    rank: i32,
) -> f32 {
    // Float variable ids are float literals (e.g. 10016.0), truncated before lookup.
    let id = lit as i32;
    // The getter reads the final pos e+0x2D88; only the destination (FltVar::Pos) is the pos e+0x2D34.
    if let 10042..=10044 = id {
        return e.final_pos[off(id, 10042)];
    }
    if let Some(r) = FltVar::from_id(id) {
        return read_f(m, e, c, &r);
    }
    if let Some(r) = IntVar::from_id(id) {
        return read_i(m, e, c, &r, rank) as f32;
    }
    match id {
        10032 => (rng.next_u32() & 0x7FFF_FFFF) as f32,
        10033 => rng.next_f32(),
        10034 => rng.next_u32().cast_signed() as f32,
        10035 => rng.next_signed(),
        10048 => angle_from(e.final_pos, m.g.player),
        10050 => dist(e.final_pos, m.g.player),
        10052 => m.g.player_char as f32,
        10082 => rng.next_f32() * 2.0 * PI - PI,
        10083 => e.last_damage as f32,
        10084 => e.boss_slot as f32,
        10085..=10087 => e.displacement[off(id, 10085)],
        10088..=10091 => e.thresholds[off(id, 10088)] as f32,
        10096 => chain_count(m, e) as f32,
        10097 => m.g.player_mode as f32,
        // Card active (bit 0) ? bit 2 capture possible : bit 9 last card captured.
        10099 => i32::from(m.g.spell.as_ref().map_or(m.g.spell_captured, |s| s.capture)) as f32,
        _ => lit,
    }
}

// Int getter 0x41F420: float variables read through the float getter and truncate.
#[allow(clippy::too_many_arguments)]
pub(crate) fn var_i(
    m: &mut EnemyManager,
    pl: &crate::player::Player,
    bm: Option<&crate::bullet::BulletManager>,
    rng: &mut crate::rng::Rng,
    e: &Enemy,
    c: &Context,
    id: i32,
    rank: i32,
) -> i32 {
    if let Some(r) = IntVar::from_id(id) {
        return read_i(m, e, c, &r, rank);
    }
    match id {
        // 0x41F5E7: Rng_Float truncated, one Next32; 1 only when it rounds up to 1.0.
        10033 => rng.next_f32() as i32,
        10032 => (rng.next_u32() & 0x7FFF_FFFF).cast_signed(),
        10034 => rng.next_u32().cast_signed(),
        // 2 if stage time + pending capture time + live time orbs ≥ TIME_THR (11 §3.1).
        10098 => {
            let pending = m.g.payout.map_or(0, |c| c.time);
            let orbs = bm
                .expect("bullet manager")
                .items
                .count(crate::item::TIME_ORB);
            let st = &pl.stats;
            if st.time + pending + orbs >= st.time_threshold {
                2
            } else {
                0
            }
        }
        // Remaining card time Timer_Get(g_Gui +0x108), also after the card and without one.
        10100 => m.g.spell_remaining.cur,
        // Int read of 10079–10081 gives the literal; 10082 is float only.
        10079..=10082 => id,
        _ if FltVar::from_id(id).is_some()
            || matches!(
                id,
                10035 | 10048 | 10050 | 10052 | 10083..=10091 | 10096 | 10097 | 10099
            ) =>
        {
            var_f(m, rng, e, c, id as f32, rank) as i32
        }
        _ => id,
    }
}

pub(super) fn read_i(m: &EnemyManager, e: &Enemy, c: &Context, r: &IntVar, rank: i32) -> i32 {
    match *r {
        IntVar::Ctx(k) => c.int(k),
        IntVar::EInt(k) => e.ivars[k],
        IntVar::Difficulty => m.g.difficulty,
        IntVar::Rank => rank,
        IntVar::F2e1c => e.f2e1c,
        IntVar::Life => e.life,
        IntVar::GArg(k) => m.g.args[k].cast_signed(),
        IntVar::Item => e.item,
        IntVar::Score => e.score,
    }
}

pub(super) fn read_f(m: &EnemyManager, e: &Enemy, c: &Context, r: &FltVar) -> f32 {
    match *r {
        FltVar::Ctx(k) => c.flt(k),
        FltVar::EFlt(k) => e.fvars[k],
        FltVar::Pos(k) => e.pos[k],
        FltVar::Player(k) => m.g.player[k],
        FltVar::GArg(k) => f32::from_bits(m.g.args[k]),
        FltVar::Angle => e.angle,
        FltVar::AngVel => e.angvel,
        FltVar::Speed => e.speed,
        FltVar::Accel => e.accel,
        FltVar::Radius => e.radius,
        FltVar::Start(k) => e.interp_start[k],
        FltVar::OrbitAngle => e.orbit_angle,
        FltVar::OrbitAv => e.orbit_av,
        FltVar::Delta(k) => e.interp_delta[k],
    }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    (dx * dx + dy * dy).sqrt()
}

// Var 10096, Enemy_ChainChildCount 0x41FD40: a root (no parent, +8 set) counts the enemies reached by +8;
// a child is evaluated on its parent (0 unless the parent is a root); no parent and no +8 → 0. No alive test.
fn chain_count(m: &EnemyManager, e: &Enemy) -> i32 {
    let root = match e.root {
        Some(p) => &*m.enemies[p],
        None => e,
    };
    if root.root.is_some() {
        return 0;
    }
    let mut d = 0;
    let mut p = root.chain_next;
    while let Some(i) = p {
        d += 1;
        if d > MAX_ENEMIES {
            break;
        }
        p = m.enemies[i].chain_next;
    }
    // d ≤ MAX_ENEMIES + 1 = 481: always fits.
    i32::try_from(d).unwrap_or(i32::MAX)
}
