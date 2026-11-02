//! World clone round trip (ARCH_A §2.3 `World: Clone`, REWRITE_PLAN "New tests"): a state cloned mid-run continues
//! exactly like the original. Step 9a covers the player and the `Sys` value parts, 9b the bullet manager with its
//! items, 9c the enemy manager with a synthetic ECL; each later step-9 run adds its subsystem to `World` here
//! (docs/arch/STEP9_WORLD.md).
use crate::anmmgr::AnmManager;
use crate::bullet::BulletManager;
use crate::enemy::EnemyManager;
use crate::game::TestSys;
use crate::input::{self, Input};
use crate::player::Player;

struct World {
    // The player and the bullet manager (`TestSys::player`, `::bullets`) next to rng, effects, overlays, panel,
    // queues, flags and rank.
    sys: TestSys,
    // The enemy manager (`Ctx.enemies`), its ECL shared with the clone until either writes into it.
    enemies: Box<EnemyManager>,
    anm: AnmManager,
    inp: Input,
    // The frame's SE ids with their pans, as Sound_Update drains them (13 §1.2).
    se: Vec<(i32, i32)>,
}

impl Clone for World {
    fn clone(&self) -> Self {
        World {
            sys: self.sys.clone(),
            enemies: self.enemies.clone(),
            anm: anm_copy(&self.anm),
            inp: self.inp,
            se: self.se.clone(),
        }
    }
}

// AnmManager is not Clone yet (its slots hold script data that becomes `Arc`-shared, ARCH_A §2.3); with no slot
// loaded its whole state is the public fields copied here.
fn anm_copy(a: &AnmManager) -> AnmManager {
    assert!((0..crate::anmmgr::SLOTS).all(|s| a.get(s).is_none()));
    let mut c = AnmManager::default();
    c.speed = a.speed;
    c.slowdown_restore = a.slowdown_restore;
    c.vm_counter = a.vm_counter;
    c.screen_offset = a.screen_offset;
    c.tint = a.tint;
    c.tint_on = a.tint_on;
    c.stage_tint = a.stage_tint;
    c.bomb_drew = a.bomb_drew;
    c.stage_camera = a.stage_camera;
    c.cam_right = a.cam_right;
    c.entries_loaded = a.entries_loaded;
    c.capture_req = a.capture_req;
    c
}

impl World {
    fn new() -> Self {
        // As Player_Load leaves it for shot type 2 (09 §2, §6.4), without the sht / anm files.
        let mut p = Player {
            state: 0,
            focus: 0,
            shot_type: 2,
            deathbomb: 18,
            ..Player::default()
        };
        p.stats.limits = crate::player::GaugeLimits::default();
        p.stats.power = 128.0;
        p.stats.bombs = 3.0;
        p.stats.lives = 2.0;
        p.state_timer.set(0);
        let mut anm = AnmManager::default();
        let mut sys = TestSys {
            player: Some(Box::new(p)),
            ..TestSys::default()
        };
        sys.bullets = Some(Box::new(BulletManager::new(&mut anm, &mut sys.rng)));
        let mut enemies = Box::new(EnemyManager::new(ecl(), crate::pl!(sys), 1));
        enemies.stage_start(&mut sys.rng, crate::pl!(sys), 0, 0);
        World {
            sys,
            enemies,
            anm,
            inp: Input::default(),
            se: Vec::new(),
        }
    }

    // One game frame of the parts present so far, in chain order: Player_Calc (prio 9) with the player and the
    // bullet manager lent out of its `Sys` (items passed in), as `PlayerTask::calc`; EnemyMgr_Calc (prio 11) as
    // `EnemyTask::calc`; BulletMgr_Calc (prio 14) with the time-orb spell bonus, as `BulletTask::calc`.
    fn frame(&mut self, f: u32) {
        let mut bits = input::SHOT;
        bits |= if (f / 45) % 2 == 0 {
            input::LEFT
        } else {
            input::RIGHT
        };
        if (f / 120) % 2 == 1 {
            bits |= input::FOCUS;
        }
        if f % 30 < 8 {
            bits |= input::UP;
        }
        if f % 300 == 150 {
            bits |= input::BOMB;
        }
        self.inp.raw = bits;
        self.inp.tick();
        let (anm, inp) = (&mut self.anm, &self.inp);
        let sys = &mut self.sys.sys();
        sys.with_bullets(|b, sys| {
            sys.with_player(|p, sys| {
                p.frame(anm, inp, Some(&mut b.items), sys);
                if std::mem::take(&mut p.bomb.attract) {
                    b.items.attract_all();
                }
            })
        });
        self.enemies.step(anm, sys);
        sys.with_bullets(|b, sys| b.step(anm, sys));
        let orbs = std::mem::take(&mut crate::pl!(sys).stats.spell_bonus_orbs);
        for _ in 0..orbs {
            self.enemies.spell_add_bonus(8000);
        }
        self.se = self.sys.sound.drain();
    }

    // Everything the clone must reproduce, as raw bits.
    fn trace(&self) -> Vec<u64> {
        let p = self.sys.player.as_deref().unwrap();
        let st = &p.stats;
        let mut t = vec![
            p.pos[0].to_bits() as u64,
            p.pos[1].to_bits() as u64,
            p.state as u64,
            p.focus as u64,
            p.mode as u64,
            p.bombing as u64,
            p.bombs_started as u64,
            p.frames as u64,
            st.power.to_bits() as u64,
            st.bombs.to_bits() as u64,
            st.gauge as u64,
            st.score10 as u64,
            self.sys.rng.state as u64,
            self.sys.rng.total as u64,
            self.sys.game.flags as u64,
            self.anm.speed.to_bits() as u64,
            self.anm.vm_counter as u64,
        ];
        t.extend(
            self.se
                .iter()
                .map(|&(id, pan)| (id as u64) << 32 | pan as u32 as u64),
        );
        t.extend(
            p.bullets
                .iter()
                .map(|b| (b.state as u64) << 32 | b.pos[0].to_bits() as u64),
        );
        let bm = self.sys.bullets.as_deref().unwrap();
        t.extend([bm.live as u64, bm.frame as u64, bm.patterns]);
        t.extend(bm.bullets.iter().filter(|b| !b.free()).flat_map(|b| {
            [
                (b.state as u64) << 32 | b.pos[0].to_bits() as u64,
                (b.pos[1].to_bits() as u64) << 32 | b.angle.to_bits() as u64,
            ]
        }));
        t.extend(
            bm.items
                .items
                .iter()
                .filter(|i| i.alive)
                .map(|i| (i.kind as u64) << 32 | i.pos[1].to_bits() as u64),
        );
        let m = &*self.enemies;
        t.extend([m.clock.cur as u64, m.full as u64, m.g.drop_counter as u64]);
        t.extend(m.enemies.iter().filter(|e| e.alive()).flat_map(|e| {
            [
                (e.life as u64) << 32 | e.main.ctx.clock.cur as u32 as u64,
                (e.final_pos[0].to_bits() as u64) << 32 | e.final_pos[1].to_bits() as u64,
            ]
        }));
        t.extend(m.ecl.data.iter().map(|&b| b as u64));
        t
    }
}

// ECL instruction (07 §2.1): time, op, size, rank mask 0xFF, i32 args.
fn ins(time: i32, op: i16, args: &[i32]) -> Vec<u8> {
    let mut v = time.to_le_bytes().to_vec();
    v.extend(op.to_le_bytes());
    v.extend(((12 + 4 * args.len()) as i16).to_le_bytes());
    v.extend([0, 0xFF, 0, 0]);
    v.extend(args.iter().flat_map(|a| a.to_le_bytes()));
    v
}

// A stage ECL with one timeline of five `spawn_randx` (07 §3 op 2, an RNG draw each) and one enemy sub: a random
// move (op 67, RNG), a write into the file itself (op 6 to a raw destination, 07 §6: a write, so the clone copies
// the shared file), an aimed two-row ring (op 98, 07 §9), then a jump back to time 0 (op 4).
fn ecl() -> crate::ecl::Ecl {
    let f = |x: f32| x.to_bits() as i32;
    let mut data = vec![0u8; 0x48];
    let sub = data.len();
    let body = [
        ins(0, 67, &[50, 0, f(1.5)]),
        ins(0, 6, &[0, 7]),
        ins(30, 98, &[0, 6, 2, f(2.0), f(1.0), 0, f(0.2), 0]),
    ];
    let len: usize = body.iter().map(Vec::len).sum();
    for i in body {
        data.extend(i);
    }
    data.extend(ins(60, 4, &[0, -(len as i32)]));
    data.extend(ins(0, -1, &[]));
    let tl = data.len();
    for time in [10i32, 100, 250, 400, 600] {
        let mut v = time.to_le_bytes().to_vec();
        v.extend(2i16.to_le_bytes());
        v.extend([8 + 4 * 7, 0xFF]);
        for a in [0, f(64.0), f(320.0), f(80.0), 600, 0, 100] {
            v.extend(a.to_le_bytes());
        }
        data.extend(v);
    }
    data.extend((-1i32).to_le_bytes());
    data.extend([0u8; 4]);
    crate::ecl::Ecl {
        data,
        subs: vec![sub],
        timelines: vec![tl],
    }
}

#[test]
fn clone_continues_identically() {
    let mut a = World::new();
    let start = a.trace();
    for f in 0..200 {
        a.frame(f);
    }
    let mut b = a.clone();
    assert!(
        b.sys.bullets.as_deref().unwrap().live > 0,
        "no bullet in flight at the clone"
    );
    assert_eq!(a.trace(), b.trace());
    for f in 200..900 {
        a.frame(f);
        b.frame(f);
        assert_eq!(a.trace(), b.trace(), "frame {f}");
    }
    // The run did something worth cloning: the player moved and bombed, the RNG was drawn, bullets flew.
    let (t, p) = (a.trace(), a.sys.player.as_deref().unwrap());
    assert_ne!(t[..2], start[..2]);
    assert!(p.bombs_started > 0 && a.sys.rng.total > 0, "{:?}", &t[..18]);
    let bm = a.sys.bullets.as_deref().unwrap();
    assert!(bm.patterns > 20 && bm.frame == 900, "{}", bm.patterns);
    // All five enemies spawned and fired (the patterns above); each run wrote into its own copy of the ECL file.
    let (ma, mb) = (&a.enemies, &b.enemies);
    assert_eq!(ma.enemies.iter().filter(|e| e.alive()).count(), 5);
    assert!(!std::sync::Arc::ptr_eq(&ma.ecl, &mb.ecl));
}
