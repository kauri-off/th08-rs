//! World clone round trip (ARCH_A §2.3 `World: Clone`, REWRITE_PLAN "New tests"): a state cloned mid-run continues
//! exactly like the original. Step 9a covers the player and the `Sys` value parts, 9b the bullet manager with its
//! items, 9c the enemy manager with a synthetic ECL, 9d the stage with a synthetic .std (`TestSys::stage`, released
//! by the ECL's section requests), 9f the effect manager and the screen-effect pool (`TestSys::effects`), 9e the
//! HUD, g_Gui and the dialogue (`TestSys::msg`, started by an ECL timeline op 6 with a synthetic .msg file), 9g the
//! game manager with its replay object recording the run, then playing the recording back; each later step-9 run
//! adds its subsystem to `World` here (docs/arch/STEP9_WORLD.md).
use crate::anmmgr::AnmManager;
use crate::bullet::BulletManager;
use crate::effect::ScreenEnv;
use crate::enemy::EnemyManager;
use crate::game::TestSys;
use crate::gamemgr::{Game, GameManager};
use crate::gui::Gui;
use crate::hud::Hud;
use crate::input::{self, Input};
use crate::msg::Dialogue;
use crate::player::Player;
use crate::replay::ReplayState;
use crate::stage::Stage;
use crate::stdfile::{Record, Std};

struct World {
    // The player, the bullet manager and the stage (`TestSys::player`, `::bullets`, `::stage`) next to rng, effects, overlays, panel,
    // queues, flags and rank.
    sys: TestSys,
    // The enemy manager (`Ctx.enemies`), its ECL shared with the clone until either writes into it.
    enemies: Box<EnemyManager>,
    // The HUD (`Ctx.hud`) and g_Gui (`Ctx.gui`); the dialogue is `TestSys::msg`, its .msg file shared by the clone.
    hud: Box<Hud>,
    gui: Box<Gui>,
    // The game manager (`Ctx.gm`) with its replay object recording into `game.rpy` (`Ctx.game`).
    gm: GameManager,
    game: Game,
    // Dialogue frames run, text lines queued and g_Gui calls handled, to check the run exercised them.
    msg_frames: u32,
    msg_texts: u32,
    gui_reqs: u32,
    anm: AnmManager,
    inp: Input,
    // The frame's SE ids with their pans, as Sound_Update drains them (13 §1.2).
    se: Vec<(i32, i32)>,
    // Stand-in for the screen effects' calc nodes (prio 3, link order): their `Effects::screen_fx` slots.
    screen: Vec<usize>,
    // Screen-effect calcs run and most effects live at once, to check the run exercised them.
    screen_runs: u32,
    fx_peak: usize,
}

impl Clone for World {
    fn clone(&self) -> Self {
        World {
            sys: self.sys.clone(),
            enemies: self.enemies.clone(),
            hud: self.hud.clone(),
            gui: self.gui.clone(),
            gm: self.gm.clone(),
            game: self.game.clone(),
            msg_frames: self.msg_frames,
            msg_texts: self.msg_texts,
            gui_reqs: self.gui_reqs,
            anm: anm_copy(&self.anm),
            inp: self.inp,
            se: self.se.clone(),
            screen: self.screen.clone(),
            screen_runs: self.screen_runs,
            fx_peak: self.fx_peak,
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
        sys.stage = Some(Box::new(Stage::new(0, stage_std())));
        let mut d = Box::new(Dialogue::default());
        d.file = Some(std::sync::Arc::new(msg_file()));
        sys.msg = Some(d);
        let mut enemies = Box::new(EnemyManager::new(ecl(), crate::pl!(sys), 1));
        enemies.stage_start(&mut sys.rng, crate::pl!(sys), 0, 0);
        // As StageStart leaves the game for a recorded stage 1 (11 §3.1, 15 §3.2): flags bit 2, a new recording,
        // Replay_Register's frame hook.
        let mut game = Game::default();
        (game.shot, game.difficulty) = (2, 1);
        game.flags |= crate::gamemgr::F_RUNNING;
        crate::replay::record_stage_start(&mut game, &crate::pl!(sys).stats);
        let mut rp = ReplayState::new(false, 0, false);
        rp.frame_hook(&mut sys.rng);
        World {
            sys,
            enemies,
            hud: Box::new(Hud::new(true)),
            gui: Box::new(Gui::new(2)),
            gm: GameManager::for_test(rp),
            game,
            msg_frames: 0,
            msg_texts: 0,
            gui_reqs: 0,
            anm,
            inp: Input::default(),
            se: Vec::new(),
            screen: Vec::new(),
            screen_runs: 0,
            fx_peak: 0,
        }
    }

    // One game frame of the parts present so far, in chain order: Stage_Calc (prio 8) with the stage lent out of its
    // `Sys`, as `StageTask::calc`; Player_Calc (prio 9) with the player and the
    // bullet manager lent out of its `Sys` (items passed in), as `PlayerTask::calc`; EnemyMgr_Calc (prio 11) as
    // `EnemyTask::calc` with a spark burst every 40 frames; EffectMgr_Calc (prio 13); BulletMgr_Calc (prio 14) with the
    // time-orb spell bonus, as `BulletTask::calc`. 9f: the screen-effect calcs first (prio 3, `ScreenTask::calc`) and
    // a type 7 + type 1 shake queued every 100 frames, linked after the calc pass (`register_pending`). 9e: Gui_Calc
    // (prio 12) handles the queued g_Gui calls and reads the bonus, as `GuiTask::calc` (no score.dat record, no
    // card steps); Hud_Calc (prio 15) boss bar outside dialogue, popups, Msg_Run with its side effects and item
    // attraction, as `HudTask::calc` (the queued text renders and events are only counted). 9g: GameMgr_Calc's
    // per-frame part first (prio 2), Replay_FrameHook (prio 7) and Replay_RecordCalc (prio 17), as `ReplayTask::calc`.
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
        self.gm
            .test_frame(&mut self.game, &mut crate::pl!(self.sys).stats);
        let env = ScreenEnv {
            flags: self.sys.game.flags,
            timer_pause: self.sys.timer_pause,
            load_frames: self.sys.load_frames,
            paused: false,
            frame_counter: f + 2,
        };
        let fx = &mut self.sys.effects;
        self.screen.retain(|&slot| {
            self.screen_runs += 1;
            let r = fx.screen_calc(slot, &mut self.anm, &mut self.sys.rng, env);
            if r == crate::task::Ret::Remove {
                fx.screen_fx[slot] = None;
            }
            r != crate::task::Ret::Remove
        });
        let rp = self.gm.replay.as_mut().unwrap();
        rp.frame_hook(&mut self.sys.rng);
        let (anm, inp) = (&mut self.anm, &self.inp);
        let ts = &mut self.sys;
        let st = ts.stage.as_deref_mut().unwrap();
        let (p, b) = (ts.player.as_deref_mut(), ts.bullets.as_deref_mut());
        st.step(anm, &mut crate::sys!(ts; p; b; None));
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
        for r in std::mem::take(sys.gui) {
            self.gui_reqs += 1;
            self.gui
                .handle(anm, sys.rng, r, sys.sound, sys.panel, sys.msg.as_deref());
        }
        self.gui.read_bonus(&self.enemies);
        if f % 40 == 0 {
            sys.fx
                .spawn(anm, sys.rng, 4, [192.0, 100.0, 0.0], 6, 0xFFFF_FFFF);
        }
        let flags = *sys.flags;
        sys.fx.calc(anm, sys.rng, flags, sys.areas);
        sys.with_bullets(|b, sys| b.step(anm, sys));
        let orbs = std::mem::take(&mut crate::pl!(sys).stats.spell_bonus_orbs);
        for _ in 0..orbs {
            self.enemies.spell_add_bonus(8000);
        }
        if f % 100 == 90 {
            sys.fx.screen(7, 4, 10, 20, 10, 0x15);
            sys.fx.screen(1, 30, 2, 6, 0, 0x10);
        }
        if !sys.msg.as_deref().unwrap().active() {
            self.hud.boss_calc(&self.enemies);
        }
        let gs = anm.speed;
        self.hud
            .popup_step(&mut crate::pl!(sys).stats, sys.panel, gs);
        let (clock, dying) = {
            let p = crate::pl!(sys);
            (p.stats.clock, p.state == 2)
        };
        let d = sys.msg.as_deref_mut().unwrap();
        self.msg_frames += d.active() as u32;
        d.info.clock = clock;
        let collect = d.run(anm, sys.rng, inp, dying);
        self.msg_texts += d.texts.len() as u32;
        d.texts.clear();
        d.events.clear();
        if std::mem::take(&mut d.side_effects) {
            self.enemies.msg_side_effects(anm, sys);
        }
        if collect {
            crate::bm!(sys).items.attract_all();
        }
        // Stage_DrawLow / DrawHigh / DrawHigh tail steps (draw prio 6 / 7): sparkle reset and flag clear.
        let st = self.sys.stage.as_deref_mut().unwrap();
        for tag in 0..3 {
            st.draw_step(&mut self.anm, &mut self.sys.effects, tag);
        }
        let rp = self.gm.replay.as_mut().unwrap();
        rp.record_game(&mut self.game, &mut self.inp);
        self.fx_peak = self.fx_peak.max(self.sys.effects.live);
        self.screen.extend(self.sys.effects.alloc_pending());
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
        let fx = &self.sys.effects;
        t.extend([
            fx.live as u64,
            self.anm.screen_offset[0].to_bits() as u64,
            self.anm.screen_offset[1].to_bits() as u64,
        ]);
        t.extend(
            fx.fx
                .iter()
                .filter(|e| e.active)
                .map(|e| (e.kind as u64) << 32 | e.pos[0].to_bits() as u64),
        );
        t.extend(
            self.screen
                .iter()
                .flat_map(|&s| fx.screen_fx[s].map_or([0; 3], |e| e.trace())),
        );
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
        t.extend(self.sys.stage.as_deref().unwrap().trace());
        t.extend(self.sys.msg.as_deref().unwrap().trace());
        t.extend(self.hud.trace());
        t.extend(self.gui.trace());
        t.extend([
            self.msg_frames as u64,
            self.msg_texts as u64,
            self.gui_reqs as u64,
        ]);
        t.extend(self.gm.trace());
        let g = &self.game;
        t.extend([g.frame_counter as u64, g.replay_fps as u64]);
        let rec = g.rpy.as_ref().unwrap().stages[0].as_ref().unwrap();
        t.extend(rec.inputs.iter().map(|&w| w as u64));
        t.extend(rec.fps.iter().map(|&b| b as u64));
        t
    }
}

// Playback of the World's recording (15 §3.3): its own game state and replay object in mode 1, as `ReplayTask::calc`
// with tag 0 (Replay_PlayCalc).
#[derive(Clone)]
struct Playback {
    game: Game,
    rp: ReplayState,
    inp: Input,
}

impl Playback {
    fn frame(&mut self) -> u16 {
        self.rp.play(&mut self.game, &mut self.inp);
        self.inp.game[0]
    }

    fn trace(&self) -> Vec<u64> {
        let i = &self.inp;
        let mut t = self.rp.trace().to_vec();
        t.extend([
            i.game[0] as u64,
            i.game_prev[0] as u64,
            i.repeat as u64,
            self.game.replay_fps as u64,
        ]);
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
// the shared file), an aimed two-row ring (op 98, 07 §9), a stage section request (op 147, 06 §4.1), then a jump
// back to time 0 (op 4).
fn ecl() -> crate::ecl::Ecl {
    let f = |x: f32| x.to_bits() as i32;
    let mut data = vec![0u8; 0x48];
    let sub = data.len();
    let body = [
        ins(0, 67, &[50, 0, f(1.5)]),
        ins(0, 6, &[0, 7]),
        ins(30, 98, &[0, 6, 2, f(2.0), f(1.0), 0, f(0.2), 0]),
        ins(45, 147, &[1]),
    ];
    let len: usize = body.iter().map(Vec::len).sum();
    for i in body {
        data.extend(i);
    }
    data.extend(ins(60, 4, &[0, -(len as i32)]));
    data.extend(ins(0, -1, &[]));
    let tl = data.len();
    let tl_ins = |time: i32, op: i16, args: &[i32]| {
        let mut v = time.to_le_bytes().to_vec();
        v.extend(op.to_le_bytes());
        v.extend([(8 + 4 * args.len()) as u8, 0xFF]);
        v.extend(args.iter().flat_map(|a| a.to_le_bytes()));
        v
    };
    for time in [10i32, 100, 250, 400, 600] {
        if time == 600 {
            // Dialogue entry 0 at time 450 (timeline op 6), then op 7 holds the timeline while it blocks (07 §3).
            data.extend(tl_ins(450, 6, &[0]));
            data.extend(tl_ins(450, 7, &[]));
        }
        let spawn = [0, f(64.0), f(320.0), f(80.0), 600, 0, 100];
        data.extend(tl_ins(time, 2, &spawn));
    }
    data.extend((-1i32).to_le_bytes());
    data.extend([0u8; 4]);
    crate::ecl::Ecl {
        data,
        subs: vec![sub],
        timelines: vec![tl],
    }
}

// msg instruction (10 §2): u16 time, u8 op, u8 arg size, args.
fn msg_ins(time: u16, op: u8, args: &[u8]) -> Vec<u8> {
    let mut v = time.to_le_bytes().to_vec();
    v.extend([op, args.len() as u8]);
    v.extend(args);
    v
}

// A .msg file (10 §2) with entry 0: skippable on (op 13), a speaker line (op 3) and a 40-frame wait (op 4), a release
// (op 6), a text line (op 16) and a 30-frame wait, then the end (op 0) at clock 20.
fn msg_file() -> crate::msg::MsgFile {
    let text = |s: &[u8]| s.iter().chain(&[0]).map(|b| b ^ 0x77).collect::<Vec<u8>>();
    let mut data = 1u32.to_le_bytes().to_vec();
    data.extend(8u32.to_le_bytes());
    let mut line = vec![0, 0, 0, 0];
    line.extend(text(b"ab"));
    for i in [
        msg_ins(0, 13, &[1, 0, 0, 0]),
        msg_ins(0, 3, &line),
        msg_ins(0, 4, &40i32.to_le_bytes()),
        msg_ins(0, 6, &[]),
        msg_ins(0, 16, &text(b"cd")),
        msg_ins(0, 4, &30i32.to_le_bytes()),
        msg_ins(20, 0, &[]),
    ] {
        data.extend(i);
    }
    crate::msg::parse(data).unwrap()
}

// A .std script (06 §1.4, §4) without objects: fog fade (ops 2, 1), a 90-frame camera move (ops 6, 5), a halt
// (op 3, the clock stops on it) that the ECL's section requests release, at time 100 a clear colour (op 13), then a
// jump to the start (op 4).
fn stage_std() -> Std {
    let f = |x: f32| x.to_bits();
    let rec = |time, op, args| Record { time, op, args };
    Std {
        quad_count: 0,
        bgm: Default::default(),
        objects: Vec::new(),
        instances: Vec::new(),
        script: vec![
            rec(0, 2, [40, 0, 0]),
            rec(0, 1, [0xFF40_8000, f(100.0), f(300.0)]),
            rec(0, 6, [90, 1, 0]),
            rec(0, 5, [f(10.0), f(-20.0), f(900.0)]),
            rec(0, 3, [0; 3]),
            rec(100, 13, [0x8010_2030, 0, 0]),
            rec(100, 4, [0, 0, 0]),
            rec(-1, 0, [0; 3]),
        ],
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
    assert!(
        !b.screen.is_empty() && b.sys.effects.live > 0,
        "no screen effect or effect live at the clone"
    );
    assert_eq!(a.trace(), b.trace());
    // A second clone in the middle of the dialogue (9e).
    let mut c: Option<World> = None;
    for f in 200..900 {
        a.frame(f);
        b.frame(f);
        assert_eq!(a.trace(), b.trace(), "frame {f}");
        if let Some(c) = &mut c {
            c.frame(f);
            assert_eq!(a.trace(), c.trace(), "frame {f} (dialogue clone)");
        } else if a.sys.msg.as_deref().unwrap().active() && a.msg_frames == 30 {
            c = Some(a.clone());
        }
    }
    assert!(c.is_some(), "no dialogue running for the second clone");
    // The run did something worth cloning: the player moved and bombed, the RNG was drawn, bullets flew.
    let (t, p) = (a.trace(), a.sys.player.as_deref().unwrap());
    assert_ne!(t[..2], start[..2]);
    assert!(p.bombs_started > 0 && a.sys.rng.total > 0, "{:?}", &t[..18]);
    let bm = a.sys.bullets.as_deref().unwrap();
    assert!(bm.patterns > 15 && bm.frame == 900, "{}", bm.patterns);
    // All five enemies spawned and fired (the patterns above); Msg_Start's KillAll at time 450 took the first four,
    // the time-600 spawn is still alive; each run wrote into its own copy of the ECL file.
    let (ma, mb) = (&a.enemies, &b.enemies);
    assert_eq!(ma.enemies.iter().filter(|e| e.alive()).count(), 1);
    assert!(!std::sync::Arc::ptr_eq(&ma.ecl, &mb.ecl));
    // The stage passed its halt (released by an ECL section request) and looped; sparkles spawned from frame 700.
    let st = a.sys.stage.as_deref().unwrap().trace();
    assert_eq!((st[0], st[4]), (900, 0x8010_2030), "{st:?}");
    // The dialogue ran and ended, rendered its lines; g_Gui handled the bomb cut-in calls.
    let d = a.sys.msg.as_deref().unwrap();
    assert!(
        !d.active() && a.msg_frames > 60 && a.msg_texts >= 2 && a.gui_reqs >= 2,
        "{} {} {}",
        a.msg_frames,
        a.msg_texts,
        a.gui_reqs
    );
    // Effects lived and the screen shakes ran (each type 7 / 1 calc draws the RNG twice).
    assert!(
        a.fx_peak > 0 && a.screen_runs > 100,
        "{} {}",
        a.fx_peak,
        a.screen_runs
    );
    // The GM counted every frame and the replay object recorded each one after the dummy word, with fps samples.
    let rec = a.game.rpy.as_ref().unwrap().stages[0].clone().unwrap();
    assert_eq!((a.game.frame_counter, rec.inputs.len()), (900, 901));
    assert!(rec.fps.len() == 31 && rec.inputs.iter().any(|&w| w & input::BOMB != 0));
    // Playback reproduces the recorded words one frame late (the dummy first), also from a clone mid-playback.
    let mut game = a.game.clone();
    game.flags |= crate::gamemgr::F_REPLAY;
    let mut pa = Playback {
        game,
        rp: ReplayState::new(true, 0, false),
        inp: Input::default(),
    };
    let mut pb: Option<Playback> = None;
    for (k, &w) in rec.inputs.iter().enumerate() {
        assert_eq!(pa.frame(), w, "playback frame {k}");
        if let Some(pb) = &mut pb {
            pb.frame();
            assert_eq!(pa.trace(), pb.trace(), "playback frame {k} (clone)");
        } else if k == 450 {
            pb = Some(pa.clone());
        }
    }
}
