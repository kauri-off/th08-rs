//! Player bullets, shot functions, options and the player laser; their draw steps (09 §4–§5, 14 §3–§5).
use crate::anmmgr::AnmManager;
use crate::anmvm::{self, AnmVm, Timer};
use crate::fmath;
use crate::game::Sys;
use crate::gamemgr::F_13;
use crate::input;
use crate::item::ItemManager;
use crate::player::{ANM_SLOT, Player};
use crate::types::{Angle, ShotType, lo8};
use std::f32::consts::{FRAC_PI_2, PI};
use th08_formats::sht::ShotEntry;

// 128 × 0x484 at player +0xBE838.
pub const MAX_BULLETS: usize = 128;

// Index from an i16 already range-checked or clamped to ≥ 0: the value of `as usize`.
fn uidx(v: i16) -> usize {
    usize::from(v.cast_unsigned())
}

// Laser slot of an entry phase / bullet +0x466, clamped to 0..=LASER_SLOTS − 1: (stored i16, index).
fn laser_slot(v: i16) -> (i16, usize) {
    let s = v.clamp(0, 2);
    (s, uidx(s))
}

// Vec3_FromPolar (05 §10).
fn polar(a: f32, r: f32) -> [f32; 2] {
    let v = crate::fmath::polar(a, r);
    [v[0], v[1]]
}

// Option calc fn: Yukari 0x44E3A0, Alice 0x44EA40, Remilia 0x44EB70, Yuyuko 0x44F930,
// Youmu's half 0x44F2D0, Remilia solo 0x44EE70, Youmu solo 0x44F5E0.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OptKind {
    #[default]
    None,
    Yukari,
    Alice,
    Remilia,
    Yuyuko,
    YoumuHalf,
    RemiliaSolo,
    YoumuSolo,
}

// Option 0x2F4 at player +0x40C + k·0x2F4.
#[derive(Clone, Debug, Default)]
pub struct Opt {
    pub vm: AnmVm,
    // +0x2A4 playfield px (shot origin), +0x2B0 anchor, +0x2BC velocity.
    pub pos: [f32; 3],
    pub anchor: [f32; 3],
    pub vel: [f32; 2],
    // +0x2C8: 0 free, 1 init, 2 active, 3 leaving; +0x2CC sub-state; +0x2D0 index.
    pub state: u8,
    pub sub: u8,
    pub index: usize,
    // +0x2D8 orbit angle, +0x2DC facing, radians.
    pub angle: Angle,
    pub facing: Angle,
    // +0x2E0.
    pub timer: Timer,
    // +0x2EC calc fn (the draw fn is always 0x44E9E0).
    pub kind: OptKind,
}

// Player bullet 0x484.
#[derive(Clone, Debug, Default)]
pub struct PBullet {
    pub vm: AnmVm,
    // +0x2A4 playfield px.
    pub pos: [f32; 3],
    // +0x430 hitbox w, h.
    pub size: [f32; 2],
    // +0x43C px/frame, +0x44C speed, +0x450 angle (radians).
    pub vel: [f32; 2],
    pub speed: f32,
    pub angle: Angle,
    // +0x454.
    pub timer: Timer,
    // +0x460.
    pub damage: i16,
    // +0x462: 0 free, 1 flying, 2 hit animation.
    pub state: u8,
    // +0x464 entry type (3 piercing, 4/5 laser, 6 no hit animation).
    pub kind: i16,
    // +0x466 laser slot.
    pub slot: i16,
    // +0x46C focus at fire.
    pub focus: u8,
    // +0x46E entry script (−10 base).
    pub script: i16,
    pub side: i16,
    // +0x468 origin.
    pub origin: i16,
    // +0x470 red tint.
    pub red: bool,
    // +0x474 update / +0x478 extra draw / +0x47C hit filter function indexes.
    pub update: i32,
    pub draw: i32,
    pub hit: i32,
    // Port-only hit counter.
    pub hits: u32,
    // +0x2B0 laser trail, 32 × f32[3] (x −999 = unused), and +0x46A its length n.
    pub trail: [[f32; 3]; 32],
    pub trail_n: i32,
    // Port-only draw-phase scratch: VM R, G, B at the main draw, before the trail's youkai-max red.
    pub main_rgb: [u8; 3],
}

// 3 slots (14 §6).
pub const LASER_SLOTS: usize = 3;

// Player laser slot (+0xE2A38 Timer + bullet, entry at +0xE2A80).
#[derive(Clone, Copy, Debug, Default)]
pub struct LaserSlot {
    pub owner: Option<usize>,
    pub timer: i32,
    pub entry: Option<EntryId>,
}

// Stands in for the entry pointer at +0xE2A80: the human and youkai sht are separate buffers (09 §2), so
// the same entry of a solo's shared file differs by copy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntryId {
    pub youkai: bool,
    pub level: usize,
    pub index: usize,
}

// Effect 0x2F colour of Youmu's half / solo. Solo, unfocused, direction 0: the original returns before both
// spawns (jump table 0x44F90A[0]), so no effect and no RNG.
fn youmu_sparkle(solo: bool, unfocused: bool, dir: u8) -> Option<u32> {
    match (solo, unfocused) {
        (true, true) if dir == 0 => None,
        (true, false) => Some(0xFFF0_5080),
        _ => Some(0x8040_5080),
    }
}

// Remilia anchor offsets, orbit speeds (rad/frame) and start angles by index.
const REMILIA_ANCHOR: [[f32; 2]; 4] =
    [[-30.0, -16.0], [-10.0, -32.0], [10.0, -32.0], [30.0, -16.0]];
const REMILIA_SPIN: [f32; 4] = [PI / 120.0, -PI / 90.0, PI / 90.0, -PI / 120.0];
const REMILIA_ORBIT: [Angle; 4] = [Angle(0.0), Angle(PI), Angle(0.0), Angle(PI)];
// Youmu facing target by direction code 1–8, pointing away from the motion.
const FACING: [f32; 8] = [
    FRAC_PI_2,
    -FRAC_PI_2,
    0.0,
    PI,
    PI / 4.0,
    3.0 * PI / 4.0,
    -PI / 4.0,
    -3.0 * PI / 4.0,
];

// Player state the option calc fns read, copied before the option is borrowed.
#[derive(Clone, Copy)]
struct OptEnv {
    p: [f32; 2],
    shooting: bool,
    shot_held: bool,
    unfocused: bool,
    dir: u8,
    trail: [f32; 3],
    aimed_pos: [f32; 3],
}

// Yukari's smoothed follow step: velocity eases toward (target − pos) / 16 by 0.2; |vx| < 0.05 snaps to 0.
fn yukari_smooth(o: &mut Opt, target: [f32; 2]) {
    let want = [(target[0] - o.pos[0]) / 16.0, (target[1] - o.pos[1]) / 16.0];
    o.vel = [
        o.vel[0] + (want[0] - o.vel[0]) * 0.2,
        o.vel[1] + (want[1] - o.vel[1]) * 0.2,
    ];
    o.pos[0] += o.vel[0];
    o.pos[1] += o.vel[1];
    if o.vel[0].abs() < 0.05 {
        o.vel[0] = 0.0;
    }
}

// Yukari 0x44E3A0; `aimed` is the player's aimed-enemy pointer, cleared here as the original does.
fn yukari_calc(o: &mut Opt, env: &OptEnv, aimed: &mut Option<usize>) {
    let t = o.timer.cur;
    let [px, py] = env.p;
    if o.sub < 3 {
        // `Option_YukariFollow` 0x44E770: follow step, then the attack test.
        let entered = o.sub;
        yukari_smooth(o, [px, fmath::fmax(py - 96.0, 32.0)]);
        if env.shooting && aimed.is_some() && t >= 10 {
            o.vm.pending_interrupt = 3;
            o.sub = 3;
        } else {
            *aimed = None;
        }
        // Animation test, always run on the sub-state the frame entered with: it can overwrite
        // the sub 3 / interrupt 3 just set while the aimed pointer stays, so the attack starts next frame.
        // scale.x negative = mirrored while moving right.
        let vx = o.vel[0];
        let mirror = |o: &mut Opt, neg: bool| {
            o.vm.scale[0] = if neg {
                -o.vm.scale[0].abs()
            } else {
                o.vm.scale[0].abs()
            }
        };
        match entered {
            0 if vx != 0.0 => {
                o.vm.pending_interrupt = 2;
                o.sub = if vx > 0.0 { 2 } else { 1 };
                mirror(o, vx > 0.0);
            }
            1 | 2 if vx == 0.0 => {
                o.vm.pending_interrupt = 1;
                o.sub = 0;
                mirror(o, false);
            }
            1 if vx > 0.0 => {
                o.vm.pending_interrupt = 2;
                o.sub = 2;
                mirror(o, true);
            }
            2 if vx < 0.0 => {
                o.vm.pending_interrupt = 2;
                o.sub = 1;
                mirror(o, false);
            }
            _ => {}
        }
        o.vm.flags |= anmvm::F_SCALE_DIRTY;
    } else {
        // Sub 3 attack: chase the aimed enemy's final pos + (0, 32), y ≥ 32, whenever the
        // pointer is set; then the exit test, so the option still moves on the exit frame.
        let ap = env.aimed_pos;
        if aimed.is_some() {
            yukari_smooth(o, [ap[0], fmath::fmax(ap[1] + 32.0, 32.0)]);
        }
        if aimed.is_none() || !(env.shooting || env.shot_held) {
            o.vm.pending_interrupt = 1;
            o.sub = 0;
            *aimed = None;
        }
    }
}

// Remilia 0x44EB70 / Remilia solo 0x44EE70.
fn remilia_calc(o: &mut Opt, k: usize, env: &OptEnv) -> Option<u32> {
    let [px, py] = env.p;
    let mut sparkle = None;
    // `Timer_Greater`(t, 12): strictly > 12, first turn at t = 13.
    if o.timer.cur > 12 {
        // Orbit steps wrap to [−π, π] (`Angle_AddNormalize`, 14 §3 "Orbit wrap").
        o.angle = o.angle.wrap_add(REMILIA_SPIN[k]);
    }
    // Solo: colour by focus, anchor follows only while unfocused, sparkle 0x2F.
    if o.kind == OptKind::RemiliaSolo {
        let c = if env.unfocused {
            0xFF80_FFFF_u32
        } else {
            0xFFFF_8080
        };
        o.vm.color = c.to_le_bytes();
        if env.unfocused {
            let a = [px + REMILIA_ANCHOR[k][0], py + REMILIA_ANCHOR[k][1]];
            o.anchor[0] += (a[0] - o.anchor[0]) * 0.2;
            o.anchor[1] += (a[1] - o.anchor[1]) * 0.2;
        }
        sparkle = Some(0x8060_2050);
    }
    let d = polar(o.angle.0, 8.0);
    o.pos = [o.anchor[0] + d[0], o.anchor[1] + d[1], 0.0];
    sparkle
}

// Yuyuko 0x44F930.
fn yuyuko_calc(o: &mut Opt, k: usize, env: &OptEnv) -> Option<u32> {
    let [px, py] = env.p;
    let tx = if k == 0 { px - 32.0 } else { px + 32.0 };
    o.anchor[0] += (tx - o.anchor[0]) * 0.09;
    o.anchor[1] += (py - o.anchor[1]) * 0.09;
    o.angle = o
        .angle
        .wrap_add(if k == 0 { PI / 60.0 } else { -PI / 60.0 });
    let d = polar(o.angle.0, 6.0);
    o.pos = [o.anchor[0] + d[0], o.anchor[1] + d[1], 0.0];
    Some(0x8060_2050)
}

// Youmu's half 0x44F2D0 / Youmu solo 0x44F5E0.
fn youmu_calc(o: &mut Opt, env: &OptEnv) -> Option<u32> {
    let (trail, dir) = (env.trail, env.dir);
    o.angle = o.angle.wrap_add(PI / 60.0);
    o.anchor[0] += (trail[0] - o.anchor[0]) * 0.05;
    o.anchor[1] += (trail[1] - o.anchor[1]) * 0.05;
    let d = polar(o.angle.0, 8.0);
    o.pos = [o.anchor[0] + d[0], o.anchor[1] + d[1], 0.0];
    // Team half always tracks; solo tracks only unfocused.
    let tracks = o.kind == OptKind::YoumuHalf || env.unfocused;
    if o.kind == OptKind::YoumuSolo {
        o.vm.color = if env.unfocused {
            0xFFFF_FFFF_u32
        } else {
            0xFFFF_8080
        }
        .to_le_bytes();
    }
    if tracks && (1..=8).contains(&dir) {
        let facing = o.facing.0;
        let mut target = FACING[usize::from(dir) - 1];
        // Shift the target by 2π toward facing; ease by 0.07 within π/2, else snap.
        if (facing - target).abs() > PI {
            target += if target < facing { 2.0 * PI } else { -2.0 * PI };
        }
        o.facing = if (target - facing).abs() <= FRAC_PI_2 {
            Angle(facing + (target - facing) * 0.07).wrap_add(0.0)
        } else {
            Angle(target)
        };
    }
    youmu_sparkle(o.kind == OptKind::YoumuSolo, env.unfocused, dir)
}

impl Player {
    // Youkai option set of a team and its slot count.
    fn youkai_set(&self) -> (OptKind, usize) {
        match self.shot().team() {
            0 => (OptKind::Yukari, 1),
            1 => (OptKind::Alice, 1),
            2 => (OptKind::Remilia, 4),
            _ => (OptKind::Yuyuko, 2),
        }
    }

    // A new option starts in state 1; its calc fn initialises it on the next update.
    fn set_option(&mut self, k: usize, kind: OptKind) {
        let state = u8::from(kind != OptKind::None);
        self.options[k] = Opt {
            kind,
            index: k,
            state,
            ..Opt::default()
        };
    }

    // Unfocus: state 3 with timer Set(0).
    fn remove_option(&mut self, k: usize) {
        let o = &mut self.options[k];
        if o.state != 0 && o.state != 3 {
            o.state = 3;
            o.timer.set(0);
        }
    }

    // Solos get their set at load.
    pub fn spawn_options(&mut self, _anm: &mut AnmManager, _solo: bool) {
        match self.shot() {
            ShotType::Yukari => self.set_option(0, OptKind::Yukari),
            ShotType::Alice => self.set_option(0, OptKind::Alice),
            ShotType::Remilia => (0..4).for_each(|k| self.set_option(k, OptKind::RemiliaSolo)),
            ShotType::Yuyuko => (0..2).for_each(|k| self.set_option(k, OptKind::Yuyuko)),
            ShotType::Youmu => self.set_option(2, OptKind::YoumuSolo),
            _ => {}
        }
    }

    // Teams: focus rewrites all 4 slots from the youkai table; unfocus sends the youkai options away and
    // gives team 3 Youmu's half in slot 2 (trail reset to the player).
    pub fn swap_options(&mut self, _anm: &mut AnmManager, f: u8) {
        let (kind, n) = self.youkai_set();
        let team3 = self.shot().team() == 3;
        if f == 1 {
            (0..4).for_each(|k| self.set_option(k, if k < n { kind } else { OptKind::None }));
        } else {
            (0..if team3 { 2 } else { 4 }).for_each(|k| self.remove_option(k));
            if team3 {
                self.trail = [self.pos; 16];
                self.set_option(2, OptKind::YoumuHalf);
            }
        }
    }

    // Calc-fn init per kind: script, start position / anchor, orbit, facing.
    fn init_option(&mut self, anm: &mut AnmManager, rng: &mut crate::rng::Rng, k: usize) {
        let p = self.pos;
        let trail = self.trail[15];
        let bombing = self.bombing;
        let o = &mut self.options[k];
        let script = match o.kind {
            OptKind::Yukari => 0x12,
            OptKind::Alice => 0x1D,
            OptKind::Remilia | OptKind::RemiliaSolo => 0x18,
            OptKind::Yuyuko | OptKind::YoumuHalf | OptKind::YoumuSolo => 0x15,
            OptKind::None => return,
        };
        anm.set_script(rng, &mut o.vm, ANM_SLOT, script);
        o.timer.set(0);
        o.sub = 0;
        o.vel = [0.0; 2];
        match o.kind {
            OptKind::Yukari => {
                o.pos = [p[0], fmath::fmax(p[1] - 96.0, 32.0), 0.0];
                self.aimed = None;
            }
            // Created during a bomb she keeps the zeroed slot pos (0, 0, 0).
            OptKind::Alice => {
                if !bombing {
                    o.pos = [p[0], p[1] - 32.0, 0.0];
                }
            }
            OptKind::Remilia | OptKind::RemiliaSolo => {
                o.anchor = [
                    p[0] + REMILIA_ANCHOR[k][0],
                    p[1] + REMILIA_ANCHOR[k][1],
                    0.0,
                ];
                o.angle = REMILIA_ORBIT[k];
            }
            OptKind::Yuyuko => {
                o.anchor = p;
                o.angle = Angle(if k == 0 { 0.0 } else { -PI });
            }
            OptKind::YoumuHalf | OptKind::YoumuSolo => {
                o.anchor = trail;
                o.angle = Angle(0.0);
                o.facing = Angle(-FRAC_PI_2);
            }
            OptKind::None => {}
        }
        o.state = 2;
    }

    // Option calc functions; positions in playfield space. One fn per calc fn of the 14 §3 table;
    // each returns the colour of its effect 0x2F sparkle, spawned after the calc.
    fn place_option(&mut self, anm: &mut AnmManager, k: usize, shot_held: bool, sys: &mut Sys) {
        let env = OptEnv {
            p: [self.pos[0], self.pos[1]],
            shooting: self.shot_timer.cur >= 0,
            shot_held,
            unfocused: self.focus == 0,
            dir: self.direction,
            trail: self.trail[15],
            aimed_pos: self.aimed_pos,
        };
        let bombing = self.bombing;
        let mut aimed = self.aimed;
        let o = &mut self.options[k];
        let sparkle = match o.kind {
            OptKind::Yukari => {
                yukari_calc(o, &env, &mut aimed);
                None
            }
            // While bombing the bomb moves option 0 (14 §2.3).
            OptKind::Alice => {
                // Leaving = state 3.
                if !bombing || o.state == 3 {
                    o.pos = [env.p[0], env.p[1] - 32.0, 0.0];
                }
                None
            }
            OptKind::Remilia | OptKind::RemiliaSolo => remilia_calc(o, k, &env),
            OptKind::Yuyuko => yuyuko_calc(o, k, &env),
            OptKind::YoumuHalf | OptKind::YoumuSolo => youmu_calc(o, &env),
            OptKind::None => None,
        };
        let pos = o.pos;
        self.aimed = aimed;
        if let Some(c) = sparkle {
            sys.fx.spawn(anm, sys.rng, 0x2F, pos, 1, c);
        }
    }

    // Generic option loop in Player_Move: calc fn, then VM run and timer tick every frame.
    pub fn update_options(&mut self, anm: &mut AnmManager, shot_held: bool, sys: &mut Sys) {
        let gs = anm.speed;
        for k in 0..4 {
            match self.options[k].state {
                0 => continue,
                1 => self.init_option(anm, sys.rng, k),
                // Leaving: no motion or sparkle; Alice keeps following the player.
                3 => {
                    let p = self.pos;
                    let o = &mut self.options[k];
                    if o.timer.cur == 0 {
                        o.vm.pending_interrupt = 5;
                    }
                    if o.kind == OptKind::Alice {
                        o.pos = [p[0], p[1] - 32.0, 0.0];
                    }
                    if o.timer.cur > 16 {
                        o.state = 0;
                        o.kind = OptKind::None;
                    }
                }
                _ => {}
            }
            if !matches!(self.options[k].state, 0 | 3) {
                self.place_option(anm, k, shot_held, sys);
            }
            let o = &mut self.options[k];
            if o.vm.script.is_some() {
                o.vm.run(anm, sys.rng);
            }
            o.timer.tick(gs);
        }
    }

    // Option draw fn 0x44E9E0: VM at pos + playfield origin, z 0.49, Draw2DRot. Step: the VM pos write.
    pub fn step_options(&mut self, anm: &AnmManager) {
        for o in self
            .options
            .iter_mut()
            .filter(|o| o.state != 0 && o.vm.script.is_some())
        {
            o.vm.pos = [o.pos[0] + 32.0, o.pos[1] + 16.0, 0.0];
            anm.prepare(&mut o.vm, 2);
        }
    }

    // Shot origin: 0 player, k = option k−1 (09 §2.1 +0x20). The option pos is read whatever its state,
    // so a free option gives its last stored pos.
    fn origin_pos(&self, origin: i16) -> [f32; 3] {
        match origin {
            1..=4 => self.options[uidx(origin) - 1].pos,
            _ => self.pos,
        }
    }

    // Shot timer (0x451500): fires on frames where the timer value changed; t > 19 → −1.
    pub fn update_shot_timer(&mut self, anm: &mut AnmManager, inp: &input::Input, sys: &mut Sys) {
        let special = self.bombing && self.bomb.index == 4;
        if self.frames >= 20 && self.shot_timer.cur >= 0 && !special {
            let t = self.shot_timer.cur;
            // Shot types 1, 6, 7 do not fire while bombing.
            let quiet = matches!(
                self.shot(),
                ShotType::MarisaAlice | ShotType::Marisa | ShotType::Alice
            );
            if t != self.shot_timer.prev && !(self.bombing && quiet) {
                self.fire(anm, t, sys);
            }
            self.shot_timer.tick(anm.speed);
            if self.shot_timer.cur > 19 {
                self.shot_timer.set(-1);
            }
        }
        if inp.down(input::SHOT) && self.shot_timer.cur < 0 && !self.env.dialogue {
            self.shot_timer.set(0);
        }
        if matches!(self.state, 1 | 2) {
            self.shot_timer.set(-1);
        }
    }

    // Bomb levels 6 / 7 for shot 2 (youkai bomb) and 9 once the bomb timer ≥ 60 (09 §2.1).
    fn forced_level(&self) -> Option<usize> {
        let s = self.shot();
        let typed =
            (s == ShotType::SakuyaRemilia && self.bomb.index & 1 == 1) || s == ShotType::Remilia;
        (typed && self.bombing && self.bomb.timer.cur >= 60)
            .then_some(if self.bomb.index & 2 != 0 { 7 } else { 6 })
    }

    // Firing (0x450F60): free slots in index order paired with the entries; a used slot advances.
    pub fn fire(&mut self, anm: &mut AnmManager, t: i32, sys: &mut Sys) {
        let sht = if self.focus == 1 {
            &self.sht_youkai
        } else {
            &self.sht
        };
        let Some(level) = sht.level_index(self.stats.power, self.forced_level()) else {
            return;
        };
        let entries = sht.levels[level].entries.clone();
        let youkai = self.focus == 1;
        let mut slot = 0;
        for (index, e) in entries.iter().enumerate() {
            let id = EntryId {
                youkai,
                level,
                index,
            };
            while slot < MAX_BULLETS && self.bullets[slot].state != 0 {
                slot += 1;
            }
            if slot >= MAX_BULLETS {
                break;
            }
            if self.init_bullet(anm, slot, t, e, id, sys) {
                slot += 1;
            }
        }
    }

    // Default spawn 0x44FB70: z 0.495, VM script = entry script + 10, VM flag 0x2000.
    fn spawn_bullet(&mut self, anm: &mut AnmManager, slot: usize, e: &ShotEntry, sys: &mut Sys) {
        if e.sound >= 0 {
            sys.sound.play_at(i32::from(e.sound), self.pos[0]);
        }
        let o = self.origin_pos(e.origin);
        let mut b = PBullet {
            pos: [o[0] + e.offset[0], o[1] + e.offset[1], 0.495],
            size: e.size,
            angle: Angle(e.angle),
            speed: e.speed,
            vel: polar(e.angle, e.speed),
            damage: e.damage,
            state: 1,
            kind: e.kind,
            slot: e.phase,
            focus: self.focus,
            script: e.script,
            side: e.side,
            origin: e.origin,
            red: e.side > 0 && self.stats.youkai_max(),
            update: e.update,
            draw: e.draw,
            hit: e.hit,
            ..PBullet::default()
        };
        b.timer.set(0);
        anm.set_script(sys.rng, &mut b.vm, ANM_SLOT, uidx((e.script + 10).max(0)));
        b.vm.flags |= anmvm::F_ZWRITE_OFF;
        self.bullets[slot] = b;
    }

    // Init 0 0x44FD80: fire on phase match.
    fn default_init(
        &mut self,
        anm: &mut AnmManager,
        slot: usize,
        t: i32,
        e: &ShotEntry,
        sys: &mut Sys,
    ) -> bool {
        if e.interval <= 0 || t % i32::from(e.interval) != i32::from(e.phase) {
            return false;
        }
        self.spawn_bullet(anm, slot, e, sys);
        true
    }

    fn set_dir(&mut self, slot: usize, angle: f32, speed: f32) {
        let b = &mut self.bullets[slot];
        b.angle = Angle(angle).wrap_add(0.0);
        b.vel = polar(b.angle.0, speed);
    }

    // Init functions (table 0x4C7EE0, 09 §2.1); true when the slot was used.
    fn init_bullet(
        &mut self,
        anm: &mut AnmManager,
        slot: usize,
        t: i32,
        e: &ShotEntry,
        id: EntryId,
        sys: &mut Sys,
    ) -> bool {
        match e.init {
            1 => {
                if !self.default_init(anm, slot, t, e, sys) {
                    return false;
                }
                // 0x450240: aims at e+0x2D34 (not the final pos); entry angle + π/2 is summed first (09 §2.1).
                if self.aimed.is_some() {
                    let a = self.aimed_base;
                    let p = self.bullets[slot].pos;
                    let ang = anmvm::wrap_add(
                        fmath::atan2_f(a[1] - p[1], a[0] - p[0]),
                        e.angle + FRAC_PI_2,
                    );
                    self.set_dir(slot, ang, e.speed * 1.5);
                }
                true
            }
            2 | 3 => !self.bombing && self.default_init(anm, slot, t, e, sys),
            // Laser 0x44FE20: refused while bombing or GameFlags bit 13; one live laser per slot; a different
            // entry (level or focus change, compared by identity) ends the old one.
            4 => {
                let (slot16, s) = laser_slot(e.phase);
                if self.bombing || *sys.flags & F_13 != 0 {
                    return false;
                }
                let ls = self.laser_slots[s];
                if let Some(old) = ls.owner {
                    if ls.entry != Some(id) {
                        self.bullets[old].vm.pending_interrupt = 1;
                        self.laser_slots[s] = LaserSlot::default();
                    }
                    return false;
                }
                self.spawn_bullet(anm, slot, e, sys);
                let b = &mut self.bullets[slot];
                b.slot = slot16;
                b.trail_n = i32::from(e.interval);
                b.trail = [[-999.0, 0.0, 0.0]; 32];
                b.pos[0] = -999.0;
                self.laser_slots[s] = LaserSlot {
                    owner: Some(slot),
                    timer: 999,
                    entry: Some(id),
                };
                true
            }
            5 => {
                if !self.default_init(anm, slot, t, e, sys) {
                    return false;
                }
                let tg = self.targets[1];
                if tg[0] > -100.0 {
                    let p = self.bullets[slot].pos;
                    self.set_dir(
                        slot,
                        fmath::atan2_f(tg[1] - p[1], tg[0] - p[0]),
                        e.speed * 1.5,
                    );
                }
                true
            }
            6 => {
                if !self.default_init(anm, slot, t, e, sys) {
                    return false;
                }
                self.set_dir(slot, self.option_angle.0 + e.angle + FRAC_PI_2, e.speed);
                true
            }
            // The random angle is drawn only on a firing frame, after Player_InitShot (09 §2.1 init 7).
            7 => {
                if !self.default_init(anm, slot, t, e, sys) {
                    return false;
                }
                let a = sys.rng.next_signed() * PI / 48.0 - FRAC_PI_2;
                self.set_dir(slot, a, e.speed);
                true
            }
            8 => {
                if self.bombing || !self.default_init(anm, slot, t, e, sys) {
                    return false;
                }
                self.set_dir(slot, self.options[2].facing.0 + e.angle, e.speed);
                true
            }
            _ => self.default_init(anm, slot, t, e, sys),
        }
    }

    // Update functions (table 0x4C7F04, 09 §2.1); true frees the bullet.
    fn bullet_update_fn(&mut self, i: usize, sys: &mut Sys) -> bool {
        let tg = self.targets[0];
        let b = &mut self.bullets[i];
        match b.update {
            // Homing 0x450320; the 1e-6 floors are port-only.
            1 => {
                if b.state == 1
                    && tg[0] > -100.0
                    && b.timer.cur <= 39
                    && b.timer.cur != b.timer.prev
                {
                    let d = [tg[0] - b.pos[0], tg[1] - b.pos[1]];
                    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
                    let k = fmath::fmax(len / (b.speed / 4.0), 1.0);
                    let v = [b.vel[0] + d[0] / k, b.vel[1] + d[1] / k];
                    let vl = fmath::fmax((v[0] * v[0] + v[1] * v[1]).sqrt(), 1e-6);
                    b.speed = fmath::fclamp(vl, 1.0, 10.0);
                    b.vel = [v[0] * b.speed / vl, v[1] * b.speed / vl];
                } else if b.state == 1 && b.speed < 10.0 {
                    // Rescale by the current |v|, not the old speed field.
                    b.speed += 1.0 / 3.0;
                    let vl = fmath::fmax((b.vel[0] * b.vel[0] + b.vel[1] * b.vel[1]).sqrt(), 1e-6);
                    b.vel = [b.vel[0] * b.speed / vl, b.vel[1] * b.speed / vl];
                }
                b.angle = Angle(fmath::atan2_f(b.vel[1], b.vel[0]));
                false
            }
            // 0x450580: vy −= Rng_Float × 0.1 + 0.27 (RNG), only while flying (state 1, 09 §2.1).
            3 => {
                if b.state == 1 {
                    b.vel[1] -= sys.rng.next_f32() * 0.1 + 0.27;
                }
                false
            }
            5 => self.laser_update(i, *sys.flags),
            // Update 4 is used by no shipped sht entry (09 §2.1): unreachable, not implemented.
            _ => false,
        }
    }

    // Laser update 5 (0x450840): follows option 0, leaves a 32-point trail of one-frame damage rects.
    fn laser_update(&mut self, i: usize, flags: u32) -> bool {
        let s = laser_slot(self.bullets[i].slot).1;
        let owned = self.laser_slots[s].owner == Some(i);
        if !owned
            || self.env.dialogue
            || self.shot_timer.cur < 0
            || self.state == 2
            || self.bombing
            || flags & F_13 != 0
        {
            let b = &mut self.bullets[i];
            b.vm.pending_interrupt = 1;
            b.update = 0;
            if owned {
                self.laser_slots[s] = LaserSlot::default();
            }
            // QUIRK: no return; the last frame still follows option 0 and adds its trail rects.
        }
        if self.options[0].state == 0 {
            self.laser_slots[s] = LaserSlot::default();
            return true;
        }
        // The clamp is a port-only bound for the 32-point trail.
        let n = self.bullets[i].trail_n.clamp(0, 16).cast_unsigned() as usize;
        for k in 0..n {
            let p = self.bullets[i].trail[2 * k];
            if p[0] >= -900.0 {
                let a = self.damage_rect([p[0], p[1]], [16.0, 448.0], 1, 0);
                self.damage_areas[a].no_spark = true;
            }
        }
        let opt = self.options[0].pos;
        let white = !self.stats.youkai_max();
        let b = &mut self.bullets[i];
        for k in (1..32).rev() {
            b.trail[k] = b.trail[k - 1];
            b.trail[k][1] -= 1.0;
        }
        b.trail[0] = b.pos;
        b.pos = [opt[0], opt[1] - 208.0, 0.44];
        b.size[1] = 448.0;
        let c = if white { [0xFF; 3] } else { [0xFF, 0xD0, 0xB0] };
        (
            b.vm.color[anmvm::R],
            b.vm.color[anmvm::G],
            b.vm.color[anmvm::B],
        ) = (c[0], c[1], c[2]);
        false
    }

    // Player bullets update (0x451150): update fn, move, off-screen (08 §4.3) unless types 4/5, VM, timer.
    pub fn update_bullets(&mut self, anm: &mut AnmManager, sys: &mut Sys) {
        let gs = anm.speed;
        for i in 0..MAX_BULLETS {
            if self.bullets[i].state == 0 {
                continue;
            }
            if self.bullet_update_fn(i, sys) {
                self.bullets[i].state = 0;
                continue;
            }
            let b = &mut self.bullets[i];
            b.pos[0] += b.vel[0] * gs;
            b.pos[1] += b.vel[1] * gs;
            if !matches!(b.kind, 4 | 5) {
                let (hw, hh) = (b.vm.sprite_w / 2.0, b.vm.sprite_h / 2.0);
                let on = b.pos[0] + hw >= 0.0
                    && b.pos[0] - hw <= 384.0
                    && b.pos[1] + hh >= 0.0
                    && b.pos[1] - hh <= 448.0;
                if !on {
                    b.state = 0;
                    continue;
                }
            }
            if b.vm.script.is_some() && b.vm.run(anm, sys.rng) {
                b.state = 0;
                continue;
            }
            // Port-only: a bullet without a script is freed.
            if b.vm.script.is_none() {
                b.state = 0;
                continue;
            }
            b.timer.tick(gs);
        }
    }

    // Hit filters (table 0x4C7F24, 09 §2.1); true skips this hit.
    fn hit_filter(
        &mut self,
        anm: &mut AnmManager,
        i: usize,
        enemy: [f32; 3],
        sys: &mut Sys,
    ) -> bool {
        let spell = self.env.spell;
        let b = &mut self.bullets[i];
        match b.hit {
            // 0x450C50 explosion: first hit draws the RNG even when the sprite has no size entry.
            1 => {
                if b.state == 1 {
                    let a = sys.rng.next_f32() * FRAC_PI_2 - 3.0 * PI / 4.0;
                    let size = match b.vm.script_index {
                        0xC => Some(48.0),
                        0xE => Some(64.0),
                        0x10 => Some(80.0),
                        0x12 => Some(96.0),
                        0x14 => Some(128.0),
                        _ => None,
                    };
                    if let Some(s) = size {
                        b.size = [s, s];
                        b.vel = polar(a, 6.0);
                    }
                } else {
                    if b.timer.cur % 2 != 0 || (spell && b.timer.cur % 4 != 0) {
                        return true;
                    }
                    b.damage = (b.damage / 3).max(1);
                    b.vel = [b.vel[0] * 0.88, b.vel[1] * 0.88];
                }
                if self.bullets[i].timer.cur % 6 == 0 {
                    sys.fx.spawn(anm, sys.rng, 5, enemy, 1, 0xFFFF_FFFF);
                }
                false
            }
            // 0x450EE0: hit effect every 8th hit, never filters.
            // 09 §2.1: u8 counter +0xE2A94 (shared with the area hit effect); type 5 at (bullet x, enemy y, z).
            2 => {
                b.hits += 1;
                let x = b.pos[0];
                self.spark_counter = self.spark_counter.wrapping_add(1);
                if self.spark_counter & 7 == 0 {
                    sys.fx
                        .spawn(anm, sys.rng, 5, [x, enemy[1], enemy[2]], 1, 0xFFFF_FFFF);
                }
                false
            }
            _ => false,
        }
    }

    // Player_DamageAgainst (0x451670); `acc` is the enemy's time-orb accumulator.
    #[expect(
        clippy::too_many_arguments,
        reason = "callers live in enemy.rs (run 11e); the signature is kept until their env struct exists"
    )]
    pub fn damage_against(
        &mut self,
        anm: &mut AnmManager,
        mut items: Option<&mut ItemManager>,
        pos: [f32; 3],
        size: [f32; 2],
        acc: &mut i32,
        by_bomb: &mut bool,
        sys: &mut Sys,
    ) -> i32 {
        if self.state_timer.cur == self.state_timer.prev {
            return 0;
        }
        *by_bomb = false;
        let (emin, emax) = box_edges(pos, size);
        let mut shot = 0;
        for i in 0..MAX_BULLETS {
            let b = &self.bullets[i];
            if b.state == 0 || !(b.state == 1 || b.kind == 3) {
                continue;
            }
            // Inclusive f32 edge tests (y first), not |d| ≤ sum of halves.
            let (bmin, bmax) = box_edges(b.pos, b.size);
            if !(bmin[1] <= emax[1]
                && bmin[0] <= emax[0]
                && emin[1] <= bmax[1]
                && emin[0] <= bmax[0])
            {
                continue;
            }
            if matches!(b.kind, 4 | 5) && b.timer.cur % 2 != 0 {
                continue;
            }
            if self.hit_filter(anm, i, pos, sys) {
                continue;
            }
            let d = i32::from(self.bullets[i].damage);
            shot += if self.bombing { (d / 5).max(1) } else { d };
            while *acc >= self.orb_step {
                *acc -= self.orb_step;
                if self.stats.human_max()
                    && self.bullets[i].side < 0
                    && let Some(items) = items.as_deref_mut()
                {
                    let p = self.bullets[i].pos;
                    items.spawn(anm, sys.rng, self, p, i32::from(crate::item::TIME_ORB), 0);
                }
            }
            let b = &mut self.bullets[i];
            // Order: rotation z saved → AnmFile_SetScript (VM pos kept, flag 0x2000 only if the
            // hit script sets it) → rotation z restored → effect 5 at the bullet → pos z 0.1 → state 2.
            if !(4..=6).contains(&b.kind) && b.state == 1 {
                let rot = b.vm.rotation[2];
                anm.file_set_script(sys.rng, &mut b.vm, ANM_SLOT, uidx((b.script + 11).max(0)));
                b.vm.rotation[2] = rot;
                let pos = b.pos;
                sys.fx.spawn(anm, sys.rng, 5, pos, 1, 0xFFFF_FFFF);
                let b = &mut self.bullets[i];
                b.pos[2] = 0.1;
                b.state = 2;
                if b.kind != 3 {
                    b.vel = [b.vel[0] / 8.0, b.vel[1] / 8.0];
                }
            }
        }
        // Only the accumulator add is capped at 50.
        *acc += shot.min(50);
        let mut dmg = shot;
        // Damage areas: life % period == 0 (period > 0 test port-only); dealt total capped by max.
        for a in self.damage_areas.iter_mut().filter(|a| a.active) {
            if a.period > 0 && a.life % a.period != 0 || !a.hits_enemy([pos[0], pos[1]], size) {
                continue;
            }
            dmg += a.damage;
            a.total += a.damage;
            if a.max > 0 && a.total >= a.max {
                dmg -= a.total - a.max;
                a.total = a.max;
                a.damage = 0;
            }
            if self.bombing {
                *by_bomb = true;
            }
            // Effect 3 every 4th area hit, areas with +0x3D = 0 only (13 §2.6).
            if !a.no_spark {
                self.spark_counter = self.spark_counter.wrapping_add(1);
                if self.spark_counter & 3 == 0 {
                    sys.fx.spawn(anm, sys.rng, 3, pos, 1, 0xFFFF_FFFF);
                }
            }
        }
        if self.stats.youkai_max() {
            dmg = dmg * 106 / 100;
        }
        dmg
    }

    // Player draw: flying bullets (state 1) at z 0.4 with their extra draw fn, or hit bullets (state 2) at z 0.2.
    // Step: the VM writes of the draw (rotation, red, pos), then the trail's lasting writes.
    pub fn step_bullets(&mut self, anm: &AnmManager, state: u8) {
        let youkai_max = self.stats.youkai_max();
        for b in self
            .bullets
            .iter_mut()
            .filter(|b| b.state == state && b.vm.script.is_some())
        {
            if b.vm.tag != 0 && b.vm.rotation[2] != b.angle.0 {
                b.vm.rotation[2] = b.angle.0;
                b.vm.flags |= anmvm::F_ROT_DIRTY;
            }
            if b.red {
                (
                    b.vm.color[anmvm::R],
                    b.vm.color[anmvm::G],
                    b.vm.color[anmvm::B],
                ) = (0xFF, 0x40, 0x40);
            }
            b.vm.pos = [b.pos[0] + 32.0, b.pos[1] + 16.0, 0.0];
            if state == 2 {
                let mode = b.vm.draw_mode;
                anm.prepare(&mut b.vm, mode);
                continue;
            }
            anm.prepare(&mut b.vm, 2);
            b.main_rgb = [
                b.vm.color[anmvm::R],
                b.vm.color[anmvm::G],
                b.vm.color[anmvm::B],
            ];
            if b.draw == 1 {
                let mut vm = std::mem::take(&mut b.vm);
                Self::trail_points(&mut vm, b, youkai_max, |_, _| {});
                b.vm = vm;
            }
        }
    }

    // Trail draw 0x450AD0: every second trail point, alpha fading from 3/4 of the VM alpha; `draw` per point.
    pub(crate) fn trail_points(
        vm: &mut AnmVm,
        b: &PBullet,
        youkai_max: bool,
        mut draw: impl FnMut(&AnmVm, f32),
    ) {
        let n = b.trail_n.max(1);
        let alpha = vm.color[anmvm::A];
        let a0 = i32::from(alpha) * 3 / 4;
        let mut c = 0;
        while c < 2 * n && c < 32 && b.trail[c.cast_unsigned() as usize][0] != -999.0 {
            let p = b.trail[c.cast_unsigned() as usize];
            vm.color[anmvm::A] = if c == 0 {
                alpha
            } else {
                lo8((a0 - (a0 / 2) * c / n).clamp(0, 255))
            };
            if youkai_max {
                (vm.color[anmvm::R], vm.color[anmvm::G], vm.color[anmvm::B]) = (0xFF, 0x40, 0x40);
            }
            vm.pos = [p[0] + 32.0, p[1] + 16.0, p[2]];
            // VM pos z := trail[c].z, not the 0.4 of the main draw.
            draw(vm, p[2]);
            c += 2;
        }
        // Only the alpha byte is restored: the youkai-max red stays in the VM colour.
        vm.color[anmvm::A] = alpha;
    }
}

// Box edges helper 0x451CE0: min = c − s·0.5, max = s·0.5 + c, in f32 (operand order kept).
fn box_edges(c: [f32; 3], s: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    (
        [c[0] - s[0] * 0.5, c[1] - s[1] * 0.5],
        [s[0] * 0.5 + c[0], s[1] * 0.5 + c[1]],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yukari(p: &mut Player) {
        p.pos = [192.0, 400.0, 0.0];
        let o = &mut p.options[0];
        o.kind = OptKind::Yukari;
        o.state = 2;
        o.sub = 0;
        o.pos = [100.0, 304.0, 0.0];
        o.timer.set(10);
    }

    // 14 §3.1: the animation test runs after the attack test on the entry sub-state and overwrites sub 3.
    #[test]
    fn yukari_animation_test_overrides_attack() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut anm = AnmManager::default();
        let mut p = Player::default();
        yukari(&mut p);
        p.shot_timer.set(0);
        p.aimed = Some(3);
        p.aimed_pos = [192.0, 100.0, 0.0];
        p.place_option(&mut anm, 0, true, sys);
        let o = &p.options[0];
        assert_eq!((o.sub, o.vm.pending_interrupt), (2, 2));
        assert_eq!(p.aimed, Some(3));
        // Next frame: entered with sub 2, still moving right → no animation change, the attack sticks.
        p.options[0].vm.pending_interrupt = 0;
        p.place_option(&mut anm, 0, true, sys);
        let o = &p.options[0];
        assert_eq!((o.sub, o.vm.pending_interrupt), (3, 3));
    }

    // 14 §3.1: sub 3 chases whenever the pointer is set, then exits; the exit frame still moves.
    #[test]
    fn yukari_sub3_moves_on_exit_frame() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut anm = AnmManager::default();
        let mut p = Player::default();
        yukari(&mut p);
        p.options[0].sub = 3;
        p.shot_timer.set(-1);
        p.aimed = Some(3);
        p.aimed_pos = [100.0, 200.0, 0.0];
        p.place_option(&mut anm, 0, false, sys);
        let o = &p.options[0];
        assert_eq!((o.sub, o.vm.pending_interrupt), (0, 1));
        assert_eq!(p.aimed, None);
        assert_ne!(o.pos[1], 304.0);
        // No pointer: no move.
        p.options[0].sub = 3;
        let before = p.options[0].pos;
        p.place_option(&mut anm, 0, true, sys);
        assert_eq!(p.options[0].pos, before);
        assert_eq!(p.options[0].sub, 0);
    }

    // 14 §3.3: unfocused solo Youmu on direction 0 spawns no effect 0x2F (no RNG); the team half always does.
    #[test]
    fn youmu_solo_idle_no_sparkle() {
        assert_eq!(youmu_sparkle(true, true, 0), None);
        assert_eq!(youmu_sparkle(true, true, 3), Some(0x8040_5080));
        assert_eq!(youmu_sparkle(true, false, 0), Some(0xFFF0_5080));
        assert_eq!(youmu_sparkle(false, true, 0), Some(0x8040_5080));
        assert_eq!(youmu_sparkle(false, false, 0), Some(0x8040_5080));
    }

    // 09 §4.4: state 2 only sets the angle; the speed-up divides by the current |v|.
    #[test]
    fn homing_speed_up_only_in_state1_by_current_v() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut p = Player::default();
        p.targets[0] = [-999.0; 3];
        let b = &mut p.bullets[0];
        (b.update, b.state, b.speed, b.vel) = (1, 2, 4.0, [0.0, -0.5]);
        p.bullet_update_fn(0, sys);
        let b = &p.bullets[0];
        assert_eq!((b.speed, b.vel), (4.0, [0.0, -0.5]));
        assert_eq!(b.angle.0, fmath::atan2_f(-0.5, 0.0));
        p.bullets[0].state = 1;
        p.bullet_update_fn(0, sys);
        let b = &p.bullets[0];
        assert_eq!(b.speed, 4.0 + 1.0 / 3.0);
        assert_eq!(b.vel, [0.0, -0.5 * b.speed / 0.5]);
    }

    // 14 §3 0x44EA40: Alice created during a bomb keeps the zeroed pos.
    #[test]
    fn alice_init_while_bombing_stays_at_origin() {
        let mut rng = crate::rng::Rng::default();
        let mut anm = AnmManager::default();
        let mut p = Player {
            pos: [192.0, 400.0, 0.0],
            bombing: true,
            ..Player::default()
        };
        p.set_option(0, OptKind::Alice);
        p.init_option(&mut anm, &mut rng, 0);
        assert_eq!((p.options[0].state, p.options[0].pos), (2, [0.0; 3]));
        p.bombing = false;
        p.set_option(0, OptKind::Alice);
        p.init_option(&mut anm, &mut rng, 0);
        assert_eq!(p.options[0].pos, [192.0, 368.0, 0.0]);
    }

    // 09 §4.3: the origin is the option's stored pos whatever its state.
    #[test]
    fn free_option_origin_is_its_last_pos() {
        let mut p = Player {
            pos: [192.0, 400.0, 0.0],
            ..Player::default()
        };
        p.options[1].state = 0;
        p.options[1].pos = [50.0, 60.0, 0.0];
        assert_eq!(p.origin_pos(2), [50.0, 60.0, 0.0]);
        assert_eq!(p.origin_pos(0), p.pos);
    }

    fn laser_entry() -> ShotEntry {
        ShotEntry {
            interval: 2,
            phase: 0,
            origin: 1,
            init: 4,
            update: 5,
            sound: -1,
            ..ShotEntry::default()
        }
    }

    // 14 §4: laser init refuses while GameFlags bit 13 is set.
    #[test]
    fn laser_init_refused_with_bit13() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut anm = AnmManager::default();
        let mut p = Player::default();
        let e = laser_entry();
        *sys.flags = F_13;
        assert!(!p.init_bullet(&mut anm, 0, 0, &e, EntryId::default(), sys));
        assert!(p.laser_slots[0].owner.is_none());
        *sys.flags = 0;
        assert!(p.init_bullet(&mut anm, 0, 0, &e, EntryId::default(), sys));
        assert_eq!(p.laser_slots[0].owner, Some(0));
    }

    // 14 §4: the same entry from the other sht copy (solo focus change) ends the laser; no new laser that frame.
    #[test]
    fn laser_focus_change_restarts() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut anm = AnmManager::default();
        let mut p = Player::default();
        let e = laser_entry();
        let human = EntryId::default();
        let youkai = EntryId {
            youkai: true,
            ..human
        };
        assert!(p.init_bullet(&mut anm, 0, 0, &e, human, sys));
        assert!(!p.init_bullet(&mut anm, 1, 2, &e, human, sys));
        assert_eq!(p.laser_slots[0].owner, Some(0));
        assert!(!p.init_bullet(&mut anm, 1, 2, &e, youkai, sys));
        assert_eq!(p.bullets[0].vm.pending_interrupt, 1);
        assert!(p.laser_slots[0].owner.is_none());
        assert!(p.init_bullet(&mut anm, 1, 4, &e, youkai, sys));
        assert_eq!(p.laser_slots[0].entry, Some(youkai));
    }

    // 14 §4: laser update 5 ends the laser (interrupt 1, slot cleared, update removed) on bit 13.
    #[test]
    fn laser_update_ends_on_bit13() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut anm = AnmManager::default();
        let mut p = Player::default();
        let e = laser_entry();
        p.options[0].state = 2;
        assert!(p.init_bullet(&mut anm, 0, 0, &e, EntryId::default(), sys));
        p.shot_timer.set(0);
        *sys.flags = F_13;
        p.options[0].pos = [80.0, 300.0, 0.0];
        assert!(!p.laser_update(0, *sys.flags));
        let b = &p.bullets[0];
        assert_eq!((b.vm.pending_interrupt, b.update), (1, 0));
        assert!(p.laser_slots[0].owner.is_none());
        // The end branch falls through: this frame the laser still moves to option 0 (14 §4).
        assert_eq!(b.pos, [80.0, 300.0 - 208.0, 0.44]);
        p.options[0].state = 0;
        assert!(p.laser_update(0, *sys.flags));
    }

    // Player anm slot with 12 scripts: 10 (flight) sets zwrite_off; 11 (hit) sets it only if asked.
    fn hit_scripts(anm: &mut AnmManager, hit_zwrite: bool) {
        let ins = |op: i16, args: &[u32]| {
            let mut v = Vec::new();
            v.extend(op.to_le_bytes());
            v.extend(((8 + 4 * args.len()) as u16).to_le_bytes());
            v.extend(0i16.to_le_bytes());
            v.extend(0u16.to_le_bytes());
            args.iter().for_each(|a| v.extend(a.to_le_bytes()));
            v
        };
        let end = ins(-1, &[]);
        let scripts = (0..12)
            .map(|k| {
                let mut data = Vec::new();
                if k == 10 || k == 11 && hit_zwrite {
                    data.extend(ins(30, &[1]));
                }
                data.extend(&end);
                th08_formats::anm::Script { id: k, data }
            })
            .collect();
        let chunk = th08_formats::anm::Chunk {
            name: "p".into(),
            width: 256,
            height: 256,
            sprites: Vec::new(),
            scripts,
            has_texture: false,
        };
        anm.insert(ANM_SLOT, "p", vec![chunk], |_| crate::drawlist::TexId(0));
    }

    // 09 §4.5: entering state 2 keeps the VM pos and rotation z, z 0.1, flag 0x2000 only from the hit script.
    #[test]
    fn hit_restart_keeps_pos_and_does_not_reset_zwrite() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        for hit_zwrite in [false, true] {
            let mut anm = AnmManager::default();
            hit_scripts(&mut anm, hit_zwrite);
            let mut p = Player {
                pos: [192.0, 400.0, 0.0],
                ..Player::default()
            };
            let e = ShotEntry {
                size: [8.0, 8.0],
                speed: 8.0,
                angle: -std::f32::consts::FRAC_PI_2,
                damage: 10,
                kind: 1,
                ..ShotEntry::default()
            };
            p.spawn_bullet(&mut anm, 0, &e, sys);
            let b = &mut p.bullets[0];
            assert_ne!(b.vm.flags & anmvm::F_ZWRITE_OFF, 0);
            b.vm.pos = [11.0, 22.0, 0.0];
            b.vm.rotation[2] = 1.25;
            let (mut acc, mut by_bomb) = (0, false);
            let dmg = p.damage_against(
                &mut anm,
                None,
                [192.0, 400.0, 0.0],
                [32.0, 32.0],
                &mut acc,
                &mut by_bomb,
                sys,
            );
            assert_eq!(dmg, 10);
            let b = &p.bullets[0];
            assert_eq!((b.state, b.pos[2]), (2, 0.1));
            assert_eq!(b.vm.script_index, 11);
            assert_eq!(b.vm.pos, [11.0, 22.0, 0.0]);
            assert_eq!(b.vm.rotation[2], 1.25);
            assert_eq!(b.vm.flags & anmvm::F_ZWRITE_OFF != 0, hit_zwrite);
            assert_eq!(
                b.vel,
                [polar(e.angle, 8.0)[0] / 8.0, polar(e.angle, 8.0)[1] / 8.0]
            );
        }
    }

    // 09 §4.5: f32 edges, inclusive. Bullet x 3.688634 w 24 vs enemy x 23.688635 w 16: the edges
    // miss by rounding although |d| ≤ sum of halves; exactly touching edges hit.
    #[test]
    fn damage_uses_f32_box_edges() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let bx = 3.688_634_f32;
        for (ex, hit) in [(23.688_635_f32, false), (bx + 20.0, true)] {
            let mut anm = AnmManager::default();
            hit_scripts(&mut anm, false);
            let mut p = Player::default();
            let e = ShotEntry {
                size: [24.0, 8.0],
                damage: 10,
                kind: 1,
                ..ShotEntry::default()
            };
            p.spawn_bullet(&mut anm, 0, &e, sys);
            p.bullets[0].pos = [bx, 100.0, 0.0];
            p.bullets[0].size = [24.0, 8.0];
            let (mut acc, mut by_bomb) = (0, false);
            let dmg = p.damage_against(
                &mut anm,
                None,
                [ex, 100.0, 0.0],
                [16.0, 16.0],
                &mut acc,
                &mut by_bomb,
                sys,
            );
            assert_eq!(dmg, if hit { 10 } else { 0 }, "enemy x {ex}");
        }
    }

    // 09 §2.1 init 1: Yukari's aimed shot targets e+0x2D34 (`aimed_base`), not the final pos.
    #[test]
    fn yukari_aim_uses_base_pos() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut anm = AnmManager::default();
        let mut p = Player {
            pos: [192.0, 400.0, 0.0],
            aimed: Some(6),
            aimed_pos: [300.0, 400.0, 0.0],
            aimed_base: [-6.0, 400.0, 0.0],
            ..Player::default()
        };
        let e = ShotEntry {
            interval: 1,
            init: 1,
            speed: 10.0,
            angle: -FRAC_PI_2,
            sound: -1,
            ..ShotEntry::default()
        };
        assert!(p.init_bullet(&mut anm, 0, 0, &e, EntryId::default(), sys));
        let b = &p.bullets[0];
        assert!(b.vel[0] < 0.0, "aims left at the base pos: {:?}", b.vel);
        assert!((-PI..=PI).contains(&b.angle.0));
    }

    // 14 §3 "Orbit wrap": every orbit step wraps, so the angle stays in [−π, π].
    #[test]
    fn orbit_angle_wraps() {
        let mut ts = crate::game::TestSys::default();
        let sys = &mut ts.sys();
        let mut anm = AnmManager::default();
        for kind in [OptKind::Yuyuko, OptKind::RemiliaSolo, OptKind::YoumuSolo] {
            let mut p = Player::default();
            p.options[0].kind = kind;
            p.options[0].state = 2;
            p.options[0].timer.set(20);
            for _ in 0..1000 {
                p.place_option(&mut anm, 0, false, sys);
                let a = p.options[0].angle.0;
                assert!((-PI..=PI).contains(&a), "{kind:?} {a}");
            }
        }
    }
}
