//! Global engine state and the per-frame calc / draw halves of `Frame_Run` 0x441E70 (00 §2).

use crate::anmmgr::AnmManager;
use crate::archive::Archive;
use crate::assets::Source;
use crate::content::LogicView;
use crate::drawlist::{FULLSCREEN, Recorder, Renderer};
use crate::task::{self, Chain, Flow, TaskCtx, Tasks};
use anyhow::Context as _;

// The original's globals that every subsystem reaches: the deterministic state in `w` (World, ARCH_A §2.3); the
// IO / output side (device, archive, sound device, host, tool switches) here.
pub struct Ctx {
    // Deterministic game state (world.rs); subsystems borrow its fields disjointly (`ctx.w.player`, `sys!(ctx.w)`).
    pub w: crate::world::World,
    // The frame's DrawCmd / ResCmd lists and device-state mirror (drawlist.rs): draw output, outside World.
    pub rec: Recorder,
    // App / tool renderer; core calls it only at the sync points (`Renderer`, drawlist.rs).
    pub renderer: Box<dyn Renderer>,
    // Game content (th08.dat, 01): logic reads `archive.read`, presentation-only data `archive.present()` (ARCH_B D1).
    pub archive: LogicView,
    // Sound device (app / tool side, `AudioOut`); logic only queues `SoundCmd`s.
    pub audio: Box<dyn AudioOut>,
    // Test / tool: every executed `SoundCmd` with its calc frame (`session --sound-log`).
    pub sound_log: Option<Vec<(u64, crate::sound::SoundCmd)>>,
    // Debug: `TH08_SE_LOG` logs each frame's flushed SE ids.
    pub se_log: bool,
    // Port debug / start-up switches (`Settings::core`), passed in by the app or tool.
    pub core: crate::coreconfig::CoreConfig,
    // Input polls, clocks and save storage (`host::Host`), supplied by the app or tool; no other platform access in logic.
    pub host: Box<dyn crate::host::Host>,
    // Debug draw log writer (drawfmt.rs), passed in by the app / tool (`Io::drawlog`) or `log_to_memory`.
    pub drawlog: Option<crate::drawfmt::Writer>,
    // Emit switch (ARCH decision 2): off = draw-phase steps only, no draw command recorded (`set_emit`).
    emit: bool,
}

// The sound device that executes the frame's `SoundCmd` stream (13 §1, 02 §3–§5); `audio::Audio` (cpal) in the
// app and tools.
pub trait AudioOut {
    // Sound_LoadSE (13 §1.1) and the archive's MIDI files + boot slot 30 (02 §5), at start-up.
    fn load(&mut self, archive: crate::content::PresentView<'_>);
    // `sound::Bgm::midi_ready` (port): asked once at load, after `load`.
    fn midi_available(&mut self) -> bool;
    // One command, in the original's call order.
    fn exec(&mut self, c: crate::sound::SoundCmd);
    fn is_started(&self) -> bool;
    // Opens the output stream (on the web: from a user gesture).
    fn ensure_started(&mut self);
}

// The IO side of `Ctx`, built by the app or tool (ARCH decisions 2 / 6).
pub struct Io {
    pub renderer: Box<dyn Renderer>,
    pub audio: Box<dyn AudioOut>,
    pub host: Box<dyn crate::host::Host>,
    // Draw log writer (None = off); `Ctx::load_with` enables the renderer's log when present.
    pub drawlog: Option<crate::drawfmt::Writer>,
}

// Disjoint borrows of the `Ctx` state that stage subsystems without a `Ctx` read and write mid-frame: SE queue
// (13 §1.2), effect manager (13 §2), GM flags word (11 §2.1), rank (11 §2), ascii overlays (13 §4), HUD dirty fields
// (12 §5.3), bomb area queue (14 §2.13), freeze byte and frames-since-load (13 §3), g_Gui call queue (12 §5.8),
// g_Rng (11 §1), the player (09 §1; None inside the player's own calc), the bullet manager (08 §2; None inside
// the bullet calc and the player calc, which hold it), the stage (06 §3; None inside the stage calc), the
// dialogue (10 §3; None without a HUD). Built by
// `sys!(ctx.w)` from `World` fields (ARCH_A §2.3).
pub struct Sys<'a> {
    pub sound: &'a mut crate::sound::Queue,
    pub fx: &'a mut crate::effect::Effects,
    pub flags: &'a mut u32,
    pub rank: &'a mut crate::gamemgr::Rank,
    pub ov: &'a mut crate::ascii::Overlays,
    pub panel: &'a mut crate::hud::Panel,
    pub areas: &'a mut crate::bomb::AreaQueue,
    pub timer_pause: &'a mut i32,
    pub load_frames: &'a mut i32,
    pub gui: &'a mut crate::gui::Queue,
    pub rng: &'a mut crate::rng::Rng,
    pub player: Option<&'a mut crate::player::Player>,
    pub bullets: Option<&'a mut crate::bullet::BulletManager>,
    pub stage: Option<&'a mut crate::stage::Stage>,
    pub msg: Option<&'a mut crate::msg::Dialogue>,
}

impl Sys<'_> {
    pub fn re(&mut self) -> Sys<'_> {
        Sys {
            sound: &mut *self.sound,
            fx: &mut *self.fx,
            flags: &mut *self.flags,
            rank: &mut *self.rank,
            ov: &mut *self.ov,
            panel: &mut *self.panel,
            areas: &mut *self.areas,
            timer_pause: &mut *self.timer_pause,
            load_frames: &mut *self.load_frames,
            gui: &mut *self.gui,
            rng: &mut *self.rng,
            player: self.player.as_deref_mut(),
            bullets: self.bullets.as_deref_mut(),
            stage: self.stage.as_deref_mut(),
            msg: self.msg.as_deref_mut(),
        }
    }

    // The player lent out of `sys` for a call that takes both (the original passes the player and reads globals).
    pub fn with_player<R>(
        &mut self,
        f: impl FnOnce(&mut crate::player::Player, &mut Self) -> R,
    ) -> R {
        let p = self.player.take().expect("no player in this Sys");
        let r = f(p, self);
        self.player = Some(p);
        r
    }

    // The bullet manager lent out of `sys` for a call on it that also takes `sys`.
    pub fn with_bullets<R>(
        &mut self,
        f: impl FnOnce(&mut crate::bullet::BulletManager, &mut Self) -> R,
    ) -> R {
        let b = self.bullets.take().expect("no bullet manager in this Sys");
        let r = f(b, self);
        self.bullets = Some(b);
        r
    }
}

// The player of a `Sys` or `Ctx` inside a running game (a field borrow, disjoint from the other fields).
#[macro_export]
macro_rules! pl {
    ($s:expr) => {
        $s.player.as_deref_mut().expect("no player in this Sys")
    };
}

// The bullet manager of a `Sys` or `Ctx` inside a running stage (a field borrow, like `pl!`).
#[macro_export]
macro_rules! bm {
    ($s:expr) => {
        $s.bullets
            .as_deref_mut()
            .expect("no bullet manager in this Sys")
    };
}

// The enemy manager of a `Ctx` inside a running stage (a field borrow, like `bm!`).
#[macro_export]
macro_rules! em {
    ($s:expr) => {
        $s.enemies.as_deref_mut().expect("no enemy manager")
    };
}

// The stage of a `Ctx` inside a running stage (a field borrow, like `em!`).
#[macro_export]
macro_rules! stg {
    ($s:expr) => {
        $s.stage.as_deref_mut().expect("no stage")
    };
}

#[macro_export]
macro_rules! sys {
    ($c:expr) => {
        $crate::sys!($c; $c.player.as_deref_mut(); $c.bullets.as_deref_mut())
    };
    ($c:expr; $p:expr; $b:expr) => {
        $crate::sys!($c; $p; $b; $c.stage.as_deref_mut())
    };
    ($c:expr; $p:expr; $b:expr; $s:expr) => {
        $crate::game::Sys {
            sound: &mut $c.sound,
            fx: &mut $c.effects,
            flags: &mut $c.game.flags,
            rank: &mut $c.game.rank,
            ov: &mut $c.overlays,
            panel: &mut $c.panel,
            areas: &mut $c.bomb_areas,
            timer_pause: &mut $c.timer_pause,
            load_frames: &mut $c.load_frames,
            gui: &mut $c.gui_queue,
            rng: &mut $c.rng,
            player: $p,
            bullets: $b,
            stage: $s,
            msg: $c.msg.as_deref_mut(),
        }
    };
}

// Owned stand-in for the `Ctx` fields behind `Sys`, for unit tests that run a subsystem without a `Ctx`.
#[cfg(test)]
#[derive(Clone, Default)]
pub struct TestSys {
    pub sound: crate::sound::Queue,
    pub effects: crate::effect::Effects,
    pub game: TestGame,
    pub overlays: crate::ascii::Overlays,
    pub panel: crate::hud::Panel,
    pub bomb_areas: crate::bomb::AreaQueue,
    pub timer_pause: i32,
    pub load_frames: i32,
    pub gui_queue: crate::gui::Queue,
    pub rng: crate::rng::Rng,
    pub player: Option<Box<crate::player::Player>>,
    pub bullets: Option<Box<crate::bullet::BulletManager>>,
    pub stage: Option<Box<crate::stage::Stage>>,
    pub msg: Option<Box<crate::msg::Dialogue>>,
}

#[cfg(test)]
#[derive(Clone, Default)]
pub struct TestGame {
    pub flags: u32,
    pub rank: crate::gamemgr::Rank,
}

#[cfg(test)]
impl TestSys {
    pub fn sys(&mut self) -> Sys<'_> {
        crate::sys!(self)
    }

    // With a default player whose difficulty the test's enemy manager wrote (EnemyMgr_Ctor, `EnemyManager::new`).
    pub fn with_player(difficulty: i32) -> Self {
        let mut p = crate::player::Player::default();
        p.stats.difficulty = difficulty;
        // An empty bullet manager (no etama.anm), built with its own RNG so the test's RNG is untouched.
        let b = crate::bullet::BulletManager::new(
            &mut crate::anmmgr::AnmManager::default(),
            &mut crate::rng::Rng::default(),
        );
        TestSys {
            player: Some(Box::new(p)),
            bullets: Some(Box::new(b)),
            ..Default::default()
        }
    }
}

impl TaskCtx for Ctx {
    type Node = crate::chain::Node;
    fn tasks(&mut self) -> &mut Tasks<crate::chain::Node> {
        &mut self.w.tasks
    }
    fn recorder(&mut self) -> Option<&mut Recorder> {
        self.emit.then_some(&mut self.rec)
    }
    fn on_restart(&mut self) {
        // ScreenEffect_Register links at once: the restarted pass already runs effects queued in this one (13 §3).
        crate::effect::register_pending(self);
        crate::trace::on_restart(self);
    }
}

impl Ctx {
    pub async fn load(
        source: Source,
        io: Io,
        core: crate::coreconfig::CoreConfig,
    ) -> anyhow::Result<Self> {
        Self::load_with(source, io, core, true).await
    }

    // WinMain start-up (00 §2): archive, sound (thbgm.fmt 02 §2, SE 13 §1.1), RNG boot seed (11 §1.3), score data, then the
    // Supervisor task. `persist` = read/write the port's save files (off for tests).
    pub async fn load_with(
        source: Source,
        io: Io,
        core: crate::coreconfig::CoreConfig,
        persist: bool,
    ) -> anyhow::Result<Self> {
        let Io {
            renderer,
            mut audio,
            mut host,
            drawlog,
        } = io;
        let archive = LogicView::new(Archive::from_bytes(source.read("th08.dat").await?)?);
        // Supervisor_OnRegister 0x4461A0: the exe-check version list, archive only; missing fails start-up (15 §2.10).
        let ver = archive
            .read(crate::scoredat::VER_LIST_NAME)
            .context("error : データのバージョンが違います")?;
        let mut bgm = crate::sound::Bgm::new(&archive.read("thbgm.fmt")?);
        audio.load(archive.present());
        bgm.midi_ready = audio.midi_available();
        // Config_Load (15 §1.2): th08.cfg and the thbgm.dat header probe (02 §1), then the file is written back.
        // The port's own save first; else import the original's th08.cfg (read-only).
        // Game-directory reads (thbgm.dat header, the original's th08.cfg / score.dat imports) go through the
        // app's `DataSource` (assets.rs); saves through `host`.
        let bgm_header = source.read_range("thbgm.dat", 0, 16).await.ok();
        let mut cfg_file = None;
        if persist {
            cfg_file = match host.read("th08.cfg") {
                Some(f) => Some(f),
                None => source.read("th08.cfg").await.ok(),
            };
        }
        let cfg = crate::config::load(cfg_file.as_deref(), bgm_header.as_deref())?;
        if persist {
            host.write("th08.cfg", &cfg.to_bytes());
        }
        let mut ctx = Self {
            w: crate::world::World::new(bgm),
            rec: Recorder::new(),
            renderer,
            archive,
            audio,
            sound_log: None,
            se_log: core.se_log,
            core,
            host,
            drawlog: None,
            emit: true,
        };
        ctx.w.game.cfg = cfg;
        ctx.w.game.ver_list.set(ver);
        // Music mode cfg +0x1F and BGM volume +0x27 (02 §4) from the loaded config.
        ctx.w.bgm.set_volumes(&ctx.w.game.cfg);
        ctx.flush_sound();
        ctx.drawlog = drawlog;
        ctx.renderer.set_font(ctx.core.font.as_deref());
        if ctx.drawlog.is_some() {
            ctx.renderer.enable_log();
        }
        ctx.w.rng = crate::rng::Rng::boot(ctx.core.seed, ctx.host.boot_clock_ms());
        if persist {
            // The port's own save first; else import the original's score.dat (read-only).
            // The whole file is loaded once; equivalent to the original's boot PLST take (15 §2.2, INFERRED).
            let file = match ctx.host.read("score.dat") {
                Some(f) => Some(f),
                None => source.read("score.dat").await.ok(),
            };
            ctx.w.game.load_score(file.as_deref(), ctx.host.ticks_ms());
            ctx.w.game.persist = true;
        } else {
            ctx.w.game.load_score(None, ctx.host.ticks_ms());
        }
        // Debug: `TH08_CLRD` overrides clear masks.
        ctx.w.game.debug_clear_masks(ctx.core.clrd.as_deref());
        crate::supervisor::register(&mut ctx)?;
        Ok(ctx)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn start_once(&mut self) {
        if !self.audio.is_started() {
            self.start();
        }
    }

    pub fn start(&mut self) {
        self.audio.ensure_started();
    }

    // Frame_Run first half (00 §2): AnmManager begin-frame, calc chain, Sound_Update.
    pub fn calc(&mut self) -> Flow {
        self.w.anm.begin_frame();
        if self.w.info.active() && crate::menu::InfoNotices::calc(self) {
            // Info_Show's own loop: input, Sound_Update, no chains.
            self.sound_update();
            self.w.frame += 1;
            return Flow::Continue;
        }
        let f = task::run(self, Chain::Calc);
        self.sound_update();
        // Port: ScreenEffect_Register (13 §3) links its nodes at once; effects queued by calc nodes after prio 3 are linked
        // here (their first calc is the next frame either way); callers below prio 3 use `effect::screen_now`.
        crate::effect::register_pending(self);
        self.w.frame += 1;
        f
    }

    // Sound_Update 0x45D790 (13 §1.3), right after the calc chain (also while paused): with SE off (cfg +0x20) the
    // queue is left as is. Volume = the curve applied to the table volume and cfg +0x28. Menus also call it in place
    // ("sound flush", 12 §2.2, 16 §6.2).
    pub fn sound_update(&mut self) {
        let cfg = self.w.game.cfg;
        // The BGM command queue runs before the SE flush, also with "SE off" (13 §1.3).
        self.w.bgm.start_fade();
        if cfg.se_on == 0 || self.w.sound.is_empty() {
            self.flush_sound();
            return;
        }
        let played = self.w.sound.drain();
        if self.se_log {
            log::info!("frame {} se {:?}", self.w.frame, played);
        }
        for (id, pan) in played {
            let vol = crate::config::volume_curve(
                crate::sound::TABLE[id as usize].1 as i32,
                cfg.se_volume,
            );
            self.w.bgm.play_se(id as usize, pan, vol);
        }
        self.flush_sound();
    }

    // Port: the device executes the `SoundCmd`s queued so far, in order, at each Sound_Update and after the
    // start-up / restart / exit calls; nothing is read back into logic.
    pub fn flush_sound(&mut self) {
        for c in self.w.bgm.out.drain(..) {
            if let Some(log) = &mut self.sound_log {
                log.push((self.w.frame, c.clone()));
            }
            self.audio.exec(c);
        }
    }

    // One Frame_Run (00 §2): calc, then draw and Present only if the calc chain did not return quit/error (no
    // draw and no Present on the quitting frame, 15 §2.2).
    pub fn run_frame(&mut self) -> Flow {
        let f = self.calc();
        if f == Flow::Continue {
            self.draw();
            self.present();
            crate::ascii::apply_pending_captures(&mut self.w.anm, &mut self.rec);
        }
        f
    }

    // Frame_Run second half (00 §2); Frame_Run only checks the calc runner's result. The draw chain runs when the
    // counter reaches cfg +0x23 (so 0 and 1 both draw every frame); Present follows either way.
    pub fn draw(&mut self) {
        if self.w.init_presents > 0 && self.w.game.scene == crate::gamemgr::SCENE_BOOT {
            self.w.init_presents -= 1;
            self.rec.set_viewport(FULLSCREEN);
            self.rec.clear_depth(Some([0, 0, 0, 255]));
            if self.drawlog.is_some() {
                self.log_frame();
            }
            return;
        }
        if self.w.info.showing() {
            crate::menu::InfoNotices::draw(self);
            if self.drawlog.is_some() {
                self.log_frame();
            }
            return;
        }
        self.w.skip_counter = self.w.skip_counter.wrapping_add(1);
        if self.w.skip_counter >= self.w.game.cfg.frame_skip as i8 {
            task::run(self, Chain::Draw);
            self.w.skip_counter = 0;
        }
        if self.drawlog.is_some() {
            self.log_frame();
        }
    }

    // Device_ClearPresentTwice 0x45B020 (16 §1.2 step 1): clear colour (`color`) and z 1.0, Present, twice; called from
    // inside a calc node, so these presents carry no primitives and shift the present numbering by 2.
    pub fn clear_present_twice(&mut self, color: [u8; 4]) {
        for _ in 0..2 {
            self.rec.set_viewport(FULLSCREEN);
            self.rec.clear_depth(Some(color));
            if self.drawlog.is_some() {
                self.log_frame();
            }
            self.present();
            crate::trace::on_extra_present(self);
        }
    }

    // WinMain exit 0x441B95 (15 §2.2): once the main loop has left (after the last frame's calc; no draw, no Present),
    // Result_Register(2) and the chain teardown flush score.dat (2 RNG draws, 11 §1.5); th08.cfg is written again at
    // WinMain exit (15 §1.2).
    pub fn exit_save(&mut self) {
        // Supervisor teardown 0x446D06, mode 2: the init.mid reset (02 §5).
        self.w.bgm.midi_reset();
        self.flush_sound();
        self.w.save_files(&mut *self.host);
    }

    // Port (ARCH_D_MOBILE: saves outside the original's save points): the files `exit_save` would write from this
    // state, written from a clone of the World that is then dropped, so the live RNG (2 draws, 11 §1.5), the score
    // data and the run-time mark are untouched; the score.dat bytes.
    pub fn save_from_clone(&mut self) -> Vec<u8> {
        self.w.clone().save_files(&mut *self.host)
    }

    // WinMain on Frame_Run result 2 (00 §2 "What the restart re-initialises"): Result(2) score.dat flush, chain
    // teardown, Sound_Update, sound / ANM manager released, then Supervisor_Register as at start-up, so the game boots
    // again through logo and loading. The live config, input words and GM fields are kept; no th08.cfg write. Port:
    // the window and device stay (the app applies the window mode), the 1 s sleep and 60 PeekMessage calls are skipped.
    pub fn restart(&mut self) -> anyhow::Result<()> {
        // Step 1: Result_Register(2) + teardown save score.dat; step 5 reloads it (the written bytes = the file).
        let score = self
            .w
            .game
            .save_score(&mut self.w.rng, true, &mut *self.host);
        task::clear(self);
        // The chain teardown released every task object (00 §2).
        self.w.scene = Default::default();
        self.w.ascii_mgr = None;
        self.w.anm_debug = None;
        self.sound_update();
        self.w.bgm.stop_bgm();
        self.w.bgm.release_bgm();
        // Supervisor teardown 0x446D06, mode 2: the init.mid reset (02 §5).
        self.w.bgm.midi_reset();
        self.flush_sound();
        for slot in 0..crate::anmmgr::SLOTS {
            self.w.anm.unload(&mut self.rec, slot);
        }
        // New ANM manager; g_GameSpeed := 1.0 (step 5; the port keeps it in the manager). Step 6: g_Rng state :=
        // (u16) timeGetTime(), the call counter +4 is kept (11 §1); AsciiManager_Load zeroes the ASCII object.
        self.w.rng.state = crate::rng::Rng::boot(self.core.seed, self.host.boot_clock_ms()).state;
        // Supervisor flag 0x20 survives: the Supervisor object is not re-constructed (00 §2).
        let restore = self.w.anm.slowdown_restore;
        self.w.anm = AnmManager::default();
        self.w.anm.speed = 1.0;
        self.w.anm.slowdown_restore = restore;
        self.w.info = Default::default();
        self.w.ascii = Default::default();
        self.w.skip_counter = crate::world::SKIP_START;
        self.w.init_presents = 2;
        let now = self.host.ticks_ms();
        self.w.game.load_score(Some(&score), now);
        // Step 3: scene +0x158 := 0; the original's request −1 (switch to the menu on the first calc) is the port's
        // boot request of Supervisor_Init; +0x160 stays until the boot → menu switch writes it.
        self.w.game.scene = crate::gamemgr::SCENE_BOOT;
        self.w.game.requested = crate::gamemgr::SCENE_BOOT;
        crate::supervisor::register(self)
    }

    // Debug tool. TH08_DRAWLOG: one F record per presented frame, sampled after the draw.
    fn log_frame(&mut self) {
        let recs = self.renderer.take_log(&mut self.rec);
        let g = &self.w.game;
        let game_frame = if g.scene == crate::gamemgr::SCENE_GAME {
            g.frame_counter
        } else {
            0
        };
        let demo = (g.flags & crate::gamemgr::F_DEMO != 0).then_some((g.demo, g.demo_frame));
        let input = if g.scene == crate::gamemgr::SCENE_GAME {
            self.w.input.game[0]
        } else {
            self.w.input.held
        };
        let scene = g.scene;
        if let Some(w) = &mut self.drawlog {
            w.frame(scene, game_frame, demo, input, &recs);
        }
    }

    // Tools and tests: their own host type (`platform::ToolHost`, `host::MemHost`), None for another host.
    pub fn host_as<T: 'static>(&mut self) -> Option<&mut T> {
        self.host.as_any_mut().downcast_mut()
    }

    // Starts an in-memory draw log (tests and tools).
    pub fn log_to_memory(&mut self) {
        self.drawlog = Some(crate::drawfmt::Writer::memory());
        self.renderer.enable_log();
    }

    // Sync point: records the Present and has the renderer execute the lists (frame end, Device_ClearPresentTwice).
    pub fn present(&mut self) {
        self.rec.present();
        self.renderer.submit(&mut self.rec);
    }

    // Emit off: the draw chain runs only the draw-phase steps (rollback re-sim, `session --no-emit`); logic is
    // unchanged. Draws outside the chain are dropped by the recorder's mute.
    pub fn set_emit(&mut self, on: bool) {
        self.emit = on;
        self.rec.mute = !on;
    }

    // App and tools: the concrete renderer (`gfx::Gfx`), None for another type.
    pub fn renderer_as<T: 'static>(&mut self) -> Option<&mut T> {
        self.renderer.as_any_mut().downcast_mut()
    }
}

#[cfg(test)]
mod tests {
    use crate::gamemgr::{SCENE_BOOT, SCENE_MENU};

    fn ctx() -> Option<super::Ctx> {
        crate::testdata::ctx(Default::default())
    }

    // 00 §2 restart: back to the boot scene, then logo and loading reach the menu again with the live config kept.
    #[test]
    fn restart_boots_again() {
        let Some(mut c) = ctx() else {
            return;
        };
        let to_menu = |c: &mut super::Ctx| {
            (0..2000)
                .any(|_| c.run_frame().eq(&super::Flow::Continue) && c.w.game.scene == SCENE_MENU)
        };
        assert!(to_menu(&mut c));
        c.w.game.cfg.windowed ^= 1;
        let mode = c.w.game.cfg.windowed;
        c.restart().unwrap();
        assert_eq!(c.w.game.scene, SCENE_BOOT);
        assert!(to_menu(&mut c));
        assert_eq!(c.w.game.cfg.windowed, mode);
    }

    // ARCH decision 2: with emit off the frames run (boot, logo, menu) and no draw command is recorded.
    #[test]
    fn emit_off_records_nothing() {
        let Some(mut c) = ctx() else {
            return;
        };
        c.set_emit(false);
        for _ in 0..600 {
            if c.calc() != super::Flow::Continue {
                break;
            }
            c.draw();
            assert!(c.rec.draws.is_empty());
            c.present();
        }
        assert_eq!(c.w.game.scene, SCENE_MENU);
    }

    // 13 §3: a screen effect queued in a fast-forward pass is linked before the restarted pass.
    #[test]
    fn restart_links_pending_screen_effects() {
        use crate::task::TaskCtx;
        let Some(mut c) = ctx() else {
            return;
        };
        c.w.effects.screen(1, 10, 2, 0, 0, 0x10);
        c.on_restart();
        assert!(c.w.effects.take_screen().is_empty());
    }
}
