//! World clone round trips: a `World` cloned mid-run (bare, menu `Ctx`, mid-stage demo) continues like the original.
use crate::bullet::BulletManager;
use crate::effect::ScreenEnv;
use crate::enemy::EnemyManager;
use crate::gamemgr::{Game, GameManager};
use crate::gui::Gui;
use crate::hud::Hud;
use crate::input::{self, Input};
use crate::msg::Dialogue;
use crate::player::Player;
use crate::replay::ReplayState;
use crate::stage::Stage;
use crate::world::World;
use th08_formats::stdfile::{Record, Std};

// A `World` run without a `Ctx` (no chain: `frame` calls the subsystems in chain order) plus the test's counters.
#[derive(Clone)]
struct Run {
    // Player, bullet manager, enemy manager, stage, HUD, g_Gui, dialogue, GM with its replay object, game state,
    // rng, effects, overlays, panel, queues and input, as the running game holds them (`Ctx.w`).
    w: World,
    // Dialogue frames run, text lines queued and g_Gui calls handled, to check the run exercised them.
    msg_frames: u32,
    msg_texts: u32,
    gui_reqs: u32,
    // The frame's SE ids with their pans, as Sound_Update drains them (13 §1.2).
    se: Vec<(i32, i32)>,
    // Stand-in for the screen effects' calc nodes (prio 3, link order): their `Effects::screen_fx` slots.
    screen: Vec<usize>,
    // Screen-effect calcs run and most effects live at once, to check the run exercised them.
    screen_runs: u32,
    fx_peak: usize,
}

// Test trace of a VM or other Debug state: a hash of its Debug text (deterministic within the test binary).
pub(crate) fn hash_dbg<T: std::fmt::Debug>(x: &T) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{x:?}").hash(&mut h);
    h.finish()
}

impl Run {
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
        let mut w = World::new(crate::sound::Bgm::default());
        w.player = Some(Box::new(p));
        w.bullets = Some(Box::new(BulletManager::new(&mut w.anm, &mut w.rng)));
        w.stage = Some(Box::new(Stage::new(0, stage_std())));
        let mut d = Box::new(Dialogue::default());
        d.file = Some(std::sync::Arc::new(msg_file()));
        w.msg = Some(d);
        let mut enemies = Box::new(EnemyManager::new(ecl(), crate::pl!(w), 1));
        enemies.stage_start(&mut w.rng, crate::pl!(w), 0, 0);
        w.enemies = Some(enemies);
        // As StageStart leaves the game for a recorded stage 1 (11 §3.1, 15 §3.2): flags bit 2, a new recording,
        // Replay_Register's frame hook.
        (w.game.shot, w.game.difficulty) = (2, 1);
        w.game.flags |= crate::gamemgr::F_RUNNING;
        crate::replay::record_stage_start(&mut w.game, &crate::pl!(w).stats);
        let mut rp = ReplayState::new(false, 0, false);
        rp.frame_hook(&mut w.rng);
        w.hud = Some(Box::new(Hud::new(true)));
        w.gui = Some(Box::new(Gui::new(2)));
        w.gm = Some(Box::new(GameManager::for_test(rp)));
        Run {
            w,
            msg_frames: 0,
            msg_texts: 0,
            gui_reqs: 0,
            se: Vec::new(),
            screen: Vec::new(),
            screen_runs: 0,
            fx_peak: 0,
        }
    }

    // One game frame in chain order, each part as its `*Task::calc` with its `Sys` lent out: GameMgr (prio 2),
    // screen effects (3, a type 7 + type 1 shake every 100 frames), replay hook (7), Stage (8), Player (9), EnemyMgr
    // (11, a spark burst every 40 frames), Gui (12), EffectMgr (13), BulletMgr (14), Hud (15), replay record (17).
    fn frame(&mut self, f: u32) {
        let mut bits = input::SHOT;
        bits |= if (f / 45).is_multiple_of(2) {
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
        let w = &mut self.w;
        w.input.raw = bits;
        w.input.tick();
        w.gm.as_deref_mut()
            .unwrap()
            .test_frame(&mut w.game, &mut crate::pl!(w).stats);
        let env = ScreenEnv {
            flags: w.game.flags,
            timer_pause: w.timer_pause,
            load_frames: w.load_frames,
            paused: false,
            frame_counter: f + 2,
        };
        let fx = &mut w.effects;
        self.screen.retain(|&slot| {
            self.screen_runs += 1;
            let r = fx.screen_calc(slot, &mut w.anm, &mut w.rng, env);
            if r == crate::task::Ret::Remove {
                fx.screen_fx[slot] = None;
            }
            r != crate::task::Ret::Remove
        });
        let rp = w.gm.as_deref_mut().unwrap().replay.as_mut().unwrap();
        rp.frame_hook(&mut w.rng);
        let (anm, inp) = (&mut w.anm, &w.input);
        let st = w.stage.as_deref_mut().unwrap();
        st.step(
            anm,
            &mut crate::sys!(w; w.player.as_deref_mut(); w.bullets.as_deref_mut(); None),
        );
        let (enemies, gui, hud) = (
            w.enemies.as_deref_mut().unwrap(),
            w.gui.as_deref_mut().unwrap(),
            w.hud.as_deref_mut().unwrap(),
        );
        let sys = &mut crate::sys!(w);
        sys.with_bullets(|b, sys| {
            sys.with_player(|p, sys| {
                p.frame(anm, inp, Some(&mut b.items), sys);
                if std::mem::take(&mut p.bomb.attract) {
                    b.items.attract_all();
                }
            });
        });
        enemies.step(anm, sys);
        for r in std::mem::take(sys.gui) {
            self.gui_reqs += 1;
            gui.handle(anm, sys.rng, r, sys.sound, sys.panel, sys.msg.as_deref());
        }
        gui.read_bonus(enemies);
        if f.is_multiple_of(40) {
            sys.fx
                .spawn(anm, sys.rng, 4, [192.0, 100.0, 0.0], 6, 0xFFFF_FFFF);
        }
        let flags = *sys.flags;
        sys.fx.calc(anm, sys.rng, flags, sys.areas);
        sys.with_bullets(|b, sys| b.step(anm, sys));
        let orbs = std::mem::take(&mut crate::pl!(sys).stats.spell_bonus_orbs);
        for _ in 0..orbs {
            enemies.spell_add_bonus(8000);
        }
        if f % 100 == 90 {
            sys.fx.screen(7, 4, 10, 20, 10, 0x15);
            sys.fx.screen(1, 30, 2, 6, 0, 0x10);
        }
        if !sys.msg.as_deref().unwrap().active() {
            hud.boss_calc(enemies);
        }
        let gs = anm.speed;
        hud.popup_step(&mut crate::pl!(sys).stats, sys.panel, gs);
        let (clock, dying) = {
            let p = crate::pl!(sys);
            (p.stats.clock, p.state == 2)
        };
        let d = sys.msg.as_deref_mut().unwrap();
        self.msg_frames += u32::from(d.active());
        d.info.clock = clock;
        let collect = d.run(anm, sys.rng, inp, dying);
        self.msg_texts += d.texts.len() as u32;
        d.texts.clear();
        d.events.clear();
        if std::mem::take(&mut d.side_effects) {
            enemies.msg_side_effects(anm, sys);
        }
        if collect {
            crate::bm!(sys).items.attract_all();
        }
        // Stage_DrawLow / DrawHigh / DrawHigh tail steps (draw prio 6 / 7): sparkle reset and flag clear.
        let st = w.stage.as_deref_mut().unwrap();
        for tag in 0..3 {
            st.draw_step(&mut w.anm, &mut w.effects, tag);
        }
        let rp = w.gm.as_deref_mut().unwrap().replay.as_mut().unwrap();
        rp.record_game(&mut w.game, &mut w.input);
        self.fx_peak = self.fx_peak.max(w.effects.live);
        self.screen.extend(w.effects.alloc_pending());
        self.se = w.sound.drain();
    }

    // Everything the clone must reproduce, as raw bits: the World's state plus the run's SE drain, screen-effect
    // list, ECL file bytes, counters and recording.
    fn trace(&self) -> Vec<u64> {
        let w = &self.w;
        let mut t = world_trace(w);
        t.extend(self.se.iter().map(|&(id, pan)| {
            i64::from(id).cast_unsigned() << 32 | u64::from(pan.cast_unsigned())
        }));
        t.extend(
            self.screen
                .iter()
                .flat_map(|&s| w.effects.screen_fx[s].map_or([0; 3], |e| e.trace())),
        );
        t.extend(
            w.enemies
                .as_deref()
                .unwrap()
                .ecl
                .data
                .iter()
                .map(|&b| u64::from(b)),
        );
        t.extend([
            u64::from(self.msg_frames),
            u64::from(self.msg_texts),
            u64::from(self.gui_reqs),
        ]);
        let rec = w.game.rpy.as_ref().unwrap().stages[0].as_ref().unwrap();
        t.extend(rec.inputs.iter().map(|&w| u64::from(w)));
        t.extend(rec.fps.iter().map(|&b| u64::from(b)));
        t
    }
}

// A World's state as raw bits: RNG (state + counter), game and ANM globals, player with its shots, bullets, effects,
// items, enemies, stage, dialogue, HUD, g_Gui, GM, Supervisor, ascii manager and scene (present ones only).
pub(crate) fn world_trace(w: &World) -> Vec<u64> {
    let g = &w.game;
    let mut t = vec![
        u64::from(w.rng.state),
        u64::from(w.rng.total),
        u64::from(g.flags),
        u64::from(g.frame_counter),
        u64::from(g.replay_fps),
        g.scene as u64,
        w.frame,
        u64::from(w.anm.speed.to_bits()),
        u64::from(w.anm.vm_counter),
        u64::from(w.anm.screen_offset[0].to_bits()),
        u64::from(w.anm.screen_offset[1].to_bits()),
        i64::from(w.timer_pause).cast_unsigned(),
        i64::from(w.load_frames).cast_unsigned(),
        i64::from(w.skip_counter).cast_unsigned(),
    ];
    if let Some(p) = w.player.as_deref() {
        let st = &p.stats;
        t.extend([
            u64::from(p.pos[0].to_bits()),
            u64::from(p.pos[1].to_bits()),
            u64::from(p.state),
            u64::from(p.focus),
            u64::from(p.mode),
            u64::from(p.bombing),
            u64::from(p.bombs_started),
            i64::from(p.frames).cast_unsigned(),
            u64::from(st.power.to_bits()),
            u64::from(st.bombs.to_bits()),
            i64::from(st.gauge).cast_unsigned(),
            st.score10.cast_unsigned(),
        ]);
        t.extend(
            p.bullets
                .iter()
                .map(|b| u64::from(b.state) << 32 | u64::from(b.pos[0].to_bits())),
        );
    }
    if let Some(bm) = w.bullets.as_deref() {
        t.extend([
            bm.live as u64,
            i64::from(bm.frame).cast_unsigned(),
            bm.patterns,
        ]);
        t.extend(bm.bullets.iter().filter(|b| !b.free()).flat_map(|b| {
            [
                i64::from(b.state).cast_unsigned() << 32 | u64::from(b.pos[0].to_bits()),
                u64::from(b.pos[1].to_bits()) << 32 | u64::from(b.angle.to_bits()),
            ]
        }));
        t.extend(
            bm.items
                .items
                .iter()
                .filter(|i| i.alive)
                .map(|i| u64::from(i.kind) << 32 | u64::from(i.pos[1].to_bits())),
        );
    }
    let fx = &w.effects;
    t.push(fx.live as u64);
    t.extend(
        fx.fx
            .iter()
            .filter(|e| e.active)
            .map(|e| u64::from(e.kind) << 32 | u64::from(e.pos[0].to_bits())),
    );
    t.extend(
        fx.screen_fx
            .iter()
            .flatten()
            .flat_map(crate::effect::ScreenEffect::trace),
    );
    if let Some(m) = w.enemies.as_deref() {
        t.extend([
            i64::from(m.clock.cur).cast_unsigned(),
            u64::from(m.full),
            u64::from(m.g.drop_counter),
        ]);
        t.extend(m.enemies.iter().filter(|e| e.alive()).flat_map(|e| {
            [
                i64::from(e.life).cast_unsigned() << 32
                    | u64::from(e.main.ctx.clock.cur.cast_unsigned()),
                u64::from(e.final_pos[0].to_bits()) << 32 | u64::from(e.final_pos[1].to_bits()),
            ]
        }));
    }
    let opt = |v: Option<Vec<u64>>| v.unwrap_or_else(|| vec![u64::MAX]);
    t.extend(opt(w.stage.as_deref().map(Stage::trace)));
    t.extend(opt(w.msg.as_deref().map(Dialogue::trace)));
    t.extend(opt(w.hud.as_deref().map(Hud::trace)));
    t.extend(opt(w.gui.as_deref().map(Gui::trace)));
    t.extend(opt(w.gm.as_deref().map(GameManager::trace)));
    t.extend(opt(w
        .supervisor
        .as_deref()
        .map(crate::supervisor::Supervisor::trace)));
    t.extend(opt(w
        .ascii_mgr
        .as_deref()
        .map(crate::ascii::AsciiManager::trace)));
    t.extend(w.scene.trace());
    t
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
            u64::from(i.game[0]),
            u64::from(i.game_prev[0]),
            u64::from(i.repeat),
            u64::from(self.game.replay_fps),
        ]);
        t
    }
}

// ECL instruction (07 §2.1): time, op, size, rank mask 0xFF, i32 args.
fn ins(time: i32, op: i16, args: &[i32]) -> Vec<u8> {
    let mut v = time.to_le_bytes().to_vec();
    v.extend(op.to_le_bytes());
    v.extend(i16::try_from(12 + 4 * args.len()).unwrap().to_le_bytes());
    v.extend([0, 0xFF, 0, 0]);
    v.extend(args.iter().flat_map(|a| a.to_le_bytes()));
    v
}

// A stage ECL with one timeline of five `spawn_randx` (op 2, an RNG draw each) and one enemy sub: a random
// move (op 67, RNG), a write into the file itself (op 6 to a raw destination: the clone copies the shared file),
// an aimed two-row ring (op 98), a stage section request (op 147), then a jump back to time 0 (op 4).
fn ecl() -> th08_formats::ecl::Ecl {
    let f = |x: f32| x.to_bits().cast_signed();
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
    data.extend(ins(60, 4, &[0, -i32::try_from(len).unwrap()]));
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
    th08_formats::ecl::Ecl {
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
    let mut a = Run::new();
    let start = a.trace();
    for f in 0..200 {
        a.frame(f);
    }
    let mut b = a.clone();
    assert!(
        b.w.bullets.as_deref().unwrap().live > 0,
        "no bullet in flight at the clone"
    );
    assert!(
        !b.screen.is_empty() && b.w.effects.live > 0,
        "no screen effect or effect live at the clone"
    );
    assert_eq!(a.trace(), b.trace());
    // A second clone in the middle of the dialogue.
    let mut c: Option<Run> = None;
    for f in 200..900 {
        a.frame(f);
        b.frame(f);
        assert_eq!(a.trace(), b.trace(), "frame {f}");
        if let Some(c) = &mut c {
            c.frame(f);
            assert_eq!(a.trace(), c.trace(), "frame {f} (dialogue clone)");
        } else if a.w.msg.as_deref().unwrap().active() && a.msg_frames == 30 {
            c = Some(a.clone());
        }
    }
    assert!(c.is_some(), "no dialogue running for the second clone");
    // The run did something worth cloning: the player moved and bombed, the RNG was drawn, bullets flew.
    let (t, p) = (a.trace(), a.w.player.as_deref().unwrap());
    assert_ne!(t[..2], start[..2]);
    assert!(p.bombs_started > 0 && a.w.rng.total > 0, "{:?}", &t[..18]);
    let bm = a.w.bullets.as_deref().unwrap();
    assert!(bm.patterns > 15 && bm.frame == 900, "{}", bm.patterns);
    // All five enemies spawned and fired (the patterns above); Msg_Start's KillAll at time 450 took the first four,
    // the time-600 spawn is still alive; each run wrote into its own copy of the ECL file.
    let (ma, mb) = (
        a.w.enemies.as_deref().unwrap(),
        b.w.enemies.as_deref().unwrap(),
    );
    assert_eq!(ma.enemies.iter().filter(|e| e.alive()).count(), 1);
    assert!(!std::sync::Arc::ptr_eq(&ma.ecl, &mb.ecl));
    // The stage passed its halt (released by an ECL section request) and looped; sparkles spawned from frame 700.
    let st = a.w.stage.as_deref().unwrap().trace();
    assert_eq!((st[0], st[4]), (900, 0x8010_2030), "{st:?}");
    // The dialogue ran and ended, rendered its lines; g_Gui handled the bomb cut-in calls.
    let d = a.w.msg.as_deref().unwrap();
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
    let rec = a.w.game.rpy.as_ref().unwrap().stages[0].clone().unwrap();
    assert_eq!((a.w.game.frame_counter, rec.inputs.len()), (900, 901));
    assert!(rec.fps.len() == 31 && rec.inputs.iter().any(|&w| w & input::BOMB != 0));
    // Playback reproduces the recorded words one frame late (the dummy first), also from a clone mid-playback.
    let mut game = a.w.game.clone();
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

// On a loaded `Ctx` (renderer side: `NullRenderer`; the test compares recorded commands).
#[cfg(test)]
mod scene {
    use super::*;

    // A loaded `Ctx`'s `World` (the whole deterministic state, chain and ANM manager included) plus the recorder's
    // device-state mirror and ANM texture cache, which shape the recorded draw commands.
    struct Snapshot {
        w: World,
        rec_state: crate::drawlist::DeviceState,
        rec_anm: crate::drawlist::AnmCache,
    }

    impl Snapshot {
        fn take(c: &crate::game::Ctx) -> Self {
            Snapshot {
                w: c.w.clone(),
                rec_state: c.rec.state,
                rec_anm: c.rec.anm,
            }
        }

        // The live World is dropped (its chain without on-remove) and the snapshot continues in its place.
        fn put(self, c: &mut crate::game::Ctx) {
            c.w = self.w;
            c.rec.state = self.rec_state;
            c.rec.anm = self.rec_anm;
        }
    }

    // Scripted main-menu input by frame: Up twice (top cursor to Option), Shot (option screen after its wait), Down,
    // Right / Left on the option rows, then Bomb twice (cursor to Quit, then leave) back to the top menu.
    fn menu_keys(f: u32) -> u16 {
        let press = |at: u32| (at..at + 2).contains(&f);
        if press(120) || press(140) {
            input::UP
        } else if press(160) {
            input::SHOT
        } else if press(230) || press(250) {
            input::DOWN
        } else if press(270) || press(290) {
            input::RIGHT
        } else if press(310) {
            input::LEFT
        } else if press(340) || press(360) {
            input::BOMB
        } else {
            0
        }
    }

    // One Frame_Run with the scripted keys; the trace of the scene, Supervisor and ascii manager, the RNG and the
    // frame's draw commands.
    fn ctx_frame(c: &mut crate::game::Ctx, f: u32) -> Vec<u64> {
        crate::testdata::mem_host(c).keys[0] = menu_keys(f);
        assert_eq!(c.calc(), crate::task::Flow::Continue);
        c.draw();
        let mut t = c.w.scene.trace();
        t.extend(c.w.supervisor.as_deref().unwrap().trace());
        t.extend(c.w.ascii_mgr.as_deref().unwrap().trace());
        t.extend([
            u64::from(c.w.rng.state),
            u64::from(c.w.rng.total),
            c.w.game.scene as u64,
            u64::from(c.w.anm.vm_counter),
            c.rec.draws.len() as u64,
            hash_dbg(&c.rec.draws),
        ]);
        c.present();
        crate::ascii::apply_pending_captures(&mut c.w.anm, &mut c.rec);
        t
    }

    // The menu scene, the Supervisor and the ascii manager cloned mid-run (in the option screen's entry wait), chain
    // included, continue exactly like the original: same state, RNG and draw commands every frame.
    #[test]
    fn scene_clone_continues_identically() {
        const CLONE_AT: u32 = 175;
        let core = crate::coreconfig::CoreConfig {
            seed: Some(0x1234),
            ..Default::default()
        };
        let Some(mut c) = crate::testdata::ctx(core) else {
            return;
        };
        for f in 0..CLONE_AT {
            ctx_frame(&mut c, f);
        }
        assert!(matches!(c.w.scene, crate::supervisor::Scene::Menu(_)));
        let nodes = |c: &crate::game::Ctx| {
            (
                c.w.tasks.nodes(crate::task::Chain::Calc),
                c.w.tasks.nodes(crate::task::Chain::Draw),
            )
        };
        let chain = nodes(&c);
        let snap = Snapshot::take(&c);
        let a: Vec<_> = (CLONE_AT..450).map(|f| ctx_frame(&mut c, f)).collect();
        // The continuation runs on the cloned World, chain included.
        snap.put(&mut c);
        assert_eq!(chain, nodes(&c));
        for (k, f) in (CLONE_AT..450).enumerate() {
            assert_eq!(ctx_frame(&mut c, f), a[k], "frame {f}");
        }
        // The chain is the same at both ends (no scene switch, no task left behind).
        assert_eq!(chain, nodes(&c));
        assert!(chain.1.iter().any(|n| n.2 == crate::chain::Node::Scene));
        // The run did something worth cloning: the option screen was entered and left, the cursor moved.
        let screens: Vec<u64> = a.iter().map(|t| t[1]).collect();
        assert!(
            screens.contains(&2) && screens.last() == Some(&0),
            "{screens:?}"
        );
        assert!(
            a.iter()
                .map(|t| t[4])
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                > 2
        );
    }
}

// Mid-stage: demo 0 (stage play with bullets, enemies and items) from demo frame `AT`; renderer side:
// `NullRenderer`.
#[cfg(test)]
mod stage {
    use super::*;
    use crate::game::Ctx;

    const DEMO: u8 = 0;
    // Demo frame of the clone / save and the frames run on after it, all inside the demo's stage.
    const AT: i32 = 1800;
    const FRAMES: usize = 600;

    // A loaded `Ctx` (saves on, into its `MemHost`; fixed ticks, so the PLST run time of a score.dat written at the
    // same frame is the same in both runs, 15 §2.9) playing demo `DEMO` up to demo frame `AT`; None without the
    // game data.
    fn demo_at() -> Option<Ctx> {
        let mut c = crate::testdata::ctx(crate::coreconfig::CoreConfig::default())?;
        let rpy = c.archive.read(&format!("demorpy{DEMO}.rpy")).unwrap();
        let r = crate::replay::parse(&rpy, c.w.game.ver_list.get()).unwrap();
        c.w.game.replay_ff = false;
        assert!(crate::replay::start_playback(
            &mut c.w.game,
            r,
            None,
            Some(DEMO)
        ));
        c.w.game.persist = true;
        let mut guard = 0;
        while c.w.game.demo_frame < AT {
            step(&mut c);
            guard += 1;
            assert!(guard < AT + 2000, "demo {DEMO} never reached frame {AT}");
        }
        Some(c)
    }

    // One Frame_Run; the World's trace, the session trace row, and the frame's draw and resource commands.
    fn step(c: &mut Ctx) -> Vec<u64> {
        assert_eq!(c.calc(), crate::task::Flow::Continue);
        c.draw();
        let mut t = world_trace(&c.w);
        t.extend(crate::trace::sample(c, 0, 0, 0).map(i64::cast_unsigned));
        t.extend([
            c.rec.draws.len() as u64,
            c.rec.res.len() as u64,
            hash_dbg(&c.rec.draws),
            hash_dbg(&c.rec.res),
        ]);
        c.present();
        crate::ascii::apply_pending_captures(&mut c.w.anm, &mut c.rec);
        t
    }

    // Live enemy bullets, enemies and items of a World.
    fn alive(w: &World) -> [usize; 3] {
        let bm = w.bullets.as_deref().unwrap();
        [
            bm.bullets.iter().filter(|b| !b.free()).count(),
            w.enemies
                .as_deref()
                .unwrap()
                .enemies
                .iter()
                .filter(|e| e.alive())
                .count(),
            bm.items.items.iter().filter(|i| i.alive).count(),
        ]
    }

    // Both runs on for `FRAMES` frames: identical state, RNG, trace row and draw / resource commands every frame;
    // the peak of bullets, enemies and items alive.
    fn lockstep(a: &mut Ctx, b: &mut Ctx) -> [usize; 3] {
        let mut peak = [0; 3];
        for k in 0..FRAMES {
            let (ta, tb) = (step(a), step(b));
            assert_eq!(ta, tb, "frame {k} after the clone");
            assert_eq!(a.w.game.scene, crate::types::SceneId::Game, "frame {k}");
            let n = alive(&a.w);
            peak = std::array::from_fn(|i| peak[i].max(n[i]));
        }
        peak
    }

    // A mid-stage World cloned into a second `Ctx` (same demo, same frame, so its recorder mirrors the
    // first's) continues exactly like the original.
    #[test]
    fn stage_clone_continues_identically() {
        let Some(mut a) = demo_at() else { return };
        let mut b = demo_at().unwrap();
        assert_eq!(a.w.game.scene, crate::types::SceneId::Game);
        assert!(alive(&a.w).iter().all(|&n| n > 0), "{:?}", alive(&a.w));
        b.w = a.w.clone();
        let peak = lockstep(&mut a, &mut b);
        assert!(peak.iter().all(|&n| n > 0), "{peak:?}");
    }

    // A save from a clone at demo frame `AT` (run A) leaves the run as one without it (run B), and writes the
    // score.dat `exit_save` writes from that state.
    #[test]
    fn save_from_clone_keeps_the_run() {
        let Some(mut a) = demo_at() else { return };
        let rng = a.w.rng.clone();
        let bytes = a.save_from_clone();
        assert_eq!((a.w.rng.state, a.w.rng.total), (rng.state, rng.total));
        assert_eq!(a.host.read("score.dat").as_deref(), Some(&bytes[..]));
        let mut b = demo_at().unwrap();
        // `exit_save` itself on a copy of B's World, then B's own World back.
        let mut w = b.w.clone();
        std::mem::swap(&mut b.w, &mut w);
        b.exit_save();
        std::mem::swap(&mut b.w, &mut w);
        assert_eq!(
            w.rng.total,
            b.w.rng.total + 2,
            "ScoreDat_Save draws twice (11 §1.5)"
        );
        assert_eq!(b.host.read("score.dat"), Some(bytes));
        lockstep(&mut a, &mut b);
    }
}
