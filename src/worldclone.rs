//! World clone round trip (ARCH_A §2.3 `World: Clone`, REWRITE_PLAN "New tests"): a state cloned mid-run continues
//! exactly like the original. Step 9a covers the player and the `Sys` value parts; each later step-9 run adds its
//! subsystem to `World` here (docs/arch/STEP9_WORLD.md).
use crate::anmmgr::AnmManager;
use crate::game::TestSys;
use crate::input::{self, Input};
use crate::player::Player;

struct World {
    // The player (`TestSys::player`) next to rng, effects, overlays, panel, queues, flags and rank.
    sys: TestSys,
    anm: AnmManager,
    inp: Input,
    // The frame's SE ids with their pans, as Sound_Update drains them (13 §1.2).
    se: Vec<(i32, i32)>,
}

impl Clone for World {
    fn clone(&self) -> Self {
        World {
            sys: self.sys.clone(),
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
        World {
            sys: TestSys {
                player: Some(Box::new(p)),
                ..TestSys::default()
            },
            anm: AnmManager::default(),
            inp: Input::default(),
            se: Vec::new(),
        }
    }

    // One game frame of the parts present so far, in chain order: Player_Calc (prio 9) with the player lent out
    // of its own `Sys`, as `PlayerTask::calc`.
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
        self.sys.sys().with_player(|p, sys| p.step(anm, inp, sys));
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
        t
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
    assert_eq!(a.trace(), b.trace());
    for f in 200..900 {
        a.frame(f);
        b.frame(f);
        assert_eq!(a.trace(), b.trace(), "frame {f}");
    }
    // The run did something worth cloning: the player moved and bombed, the RNG was drawn.
    let (t, p) = (a.trace(), a.sys.player.as_deref().unwrap());
    assert_ne!(t[..2], start[..2]);
    assert!(p.bombs_started > 0 && a.sys.rng.total > 0, "{:?}", &t[..18]);
}
