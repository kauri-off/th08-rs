//! Global engine state and the per-frame calc / draw halves of `Frame_Run` 0x441E70 (00 §2).

use crate::anmmgr::AnmManager;
use crate::archive::Archive;
use crate::assets::Source;
use crate::audio::Audio;
use crate::drawlist::{FULLSCREEN, Recorder, Renderer};
use crate::input::Input;
use crate::task::{self, Chain, Flow, TaskCtx, Tasks};
use anyhow::Context as _;

// WinMain 0x441A70: skip counter byte := 0xFC at the first start and every restart (00 §2), first draw in frame 4.
const SKIP_START: i8 = -4;

// The original's globals that every subsystem reaches (device, archive, sound manager, input words, SprtCtrlInf,
// funcChain); `game` holds GM / Supervisor fields (11 §2, §7).
pub struct Ctx {
    // The frame's DrawCmd / ResCmd lists and device-state mirror (drawlist.rs, future `World` draw output).
    pub rec: Recorder,
    // App / tool renderer; core calls it only at the sync points (`Renderer`, drawlist.rs).
    pub renderer: Box<dyn Renderer>,
    // TODO(step 8b): core reads content through a LogicView, not the archive.
    pub archive: Archive,
    // TODO(step 10): the cpal device leaves core with the crate cut; logic only queues `SoundCmd`s.
    pub audio: Audio,
    pub input: Input,
    pub anm: AnmManager,
    // The game RNG g_Rng 0x164D520 (11 §1); every draw site keeps the original's order (future `World.rng`).
    pub rng: crate::rng::Rng,
    pub tasks: Tasks<Ctx>,
    // Port count of executed calc steps (debug tools, TH08_SHOT); not an original counter.
    pub frame: u64,
    pub game: crate::gamemgr::Game,
    pub ascii: crate::ascii::Queue,
    // Sound manager SE queue (13 §1.2), flushed by `sound_update`.
    pub sound: crate::sound::Queue,
    // Sound manager BGM state + the frame's `SoundCmd` output (02 §4–§5, 13 §1.3), executed by `audio`.
    pub bgm: crate::sound::Bgm,
    // Test / tool: every executed `SoundCmd` with its calc frame (`session --sound-log`).
    pub sound_log: Option<Vec<(u64, crate::sound::SoundCmd)>>,
    // Effect manager g_EffectMgr 0x4ECE60 (13 §2).
    pub effects: crate::effect::Effects,
    // Ascii manager popups, darkness mask, gauge readout and boss markers (13 §4, 12 §5.9–§5.10).
    pub overlays: crate::ascii::Overlays,
    // HUD dirty fields + full-repaint counter 0x17CE8CC (12 §5.3).
    pub panel: crate::hud::Panel,
    // HUD impl block kept between Hud_OnRemove and the next Hud_Load (12 §5.1a).
    pub hud_kept: Option<crate::hud::Kept>,
    // Port queue of bomb areas created by effects (14 §2.13).
    pub bomb_areas: crate::bomb::AreaQueue,
    // Global freeze byte 0x160F534: native 26 sets it, Player_Die clears it, enemy timers and bullets read it live.
    pub timer_pause: i32,
    // Global 0x18B8A2C "frames since load" (13 §3): ECL native 30 (= arg), 2 at the end of StageStart, 1 by
    // GameMgr_OnRemove, 0 by Result / MainMenu register; Supervisor_Calc decrements it once per frame while ≠ 0.
    // Read by screen effects 1 / 2 / 4, which are removed while it is ≠ 0.
    pub load_frames: i32,
    // g_Gui calls queued for Gui_Calc (12 §5.8); cleared by Gui_Register.
    pub gui_queue: crate::gui::Queue,
    // Debug: `TH08_SE_LOG` logs each frame's flushed SE ids.
    pub se_log: bool,
    // Port debug / start-up switches (`Settings::core`), passed in by the app or tool.
    pub core: crate::coreconfig::CoreConfig,
    // Input polls, clocks and save storage (`host::Host`), supplied by the app or tool; no other platform access in logic.
    pub host: Box<dyn crate::host::Host>,
    // TODO(step 8): debug draw log file writer; becomes a serializer of the recorded DrawList in the tools.
    pub drawlog: Option<crate::drawlog::Writer>,
    // Frame_Run frame-skip counter (i8, 00 §2), compared with cfg +0x23; SKIP_START after every (re)start.
    pub skip_counter: i8,
    // Port: Window_InitD3D's two clear + Present (12 §1 boot presents 1–2) run as the first two port frames, outside
    // Frame_Run, so they neither run the draw chain nor step the skip counter (presents 3–5 are its −3…−1).
    init_presents: u8,
    // Unlock notices (16 §4), shown in place of the frame's chains.
    pub info: crate::menu::InfoNotices,
    // Emit switch (ARCH decision 2): off = draw-phase steps only, no draw command recorded (`set_emit`).
    emit: bool,
}

// Disjoint borrows of the `Ctx` state that stage subsystems without a `Ctx` read and write mid-frame: SE queue
// (13 §1.2), effect manager (13 §2), GM flags word (11 §2.1), rank (11 §2), ascii overlays (13 §4), HUD dirty fields
// (12 §5.3), bomb area queue (14 §2.13), freeze byte and frames-since-load (13 §3), g_Gui call queue (12 §5.8),
// g_Rng (11 §1). Built by `sys!(ctx)`; these become `World` fields in step 9 (ARCH_A §2.3).
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
        }
    }
}

#[macro_export]
macro_rules! sys {
    ($c:expr) => {
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
        }
    };
}

// Owned stand-in for the `Ctx` fields behind `Sys`, for unit tests that run a subsystem without a `Ctx`.
#[cfg(test)]
#[derive(Default)]
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
}

#[cfg(test)]
#[derive(Default)]
pub struct TestGame {
    pub flags: u32,
    pub rank: crate::gamemgr::Rank,
}

#[cfg(test)]
impl TestSys {
    pub fn sys(&mut self) -> Sys<'_> {
        crate::sys!(self)
    }
}

impl TaskCtx for Ctx {
    fn tasks(&mut self) -> &mut Tasks<Self> {
        &mut self.tasks
    }
    fn recorder(&mut self) -> Option<&mut Recorder> {
        self.emit.then_some(&mut self.rec)
    }
    fn on_restart(&mut self) {
        // ScreenEffect_Register links at once: the restarted pass already runs effects queued in this one (13 §3).
        crate::effect::register_pending(self);
        crate::session::on_restart(self);
    }
}

impl Ctx {
    pub async fn load(
        source: Source,
        renderer: impl Renderer + 'static,
        core: crate::coreconfig::CoreConfig,
        host: Box<dyn crate::host::Host>,
    ) -> anyhow::Result<Self> {
        Self::load_with(source, renderer, true, core, host).await
    }

    // WinMain start-up (00 §2): archive, sound (thbgm.fmt 02 §2, SE 13 §1.1), RNG boot seed (11 §1.3), score data, then the
    // Supervisor task. `persist` = read/write the port's save files (off for tests).
    pub async fn load_with(
        source: Source,
        renderer: impl Renderer + 'static,
        persist: bool,
        core: crate::coreconfig::CoreConfig,
        mut host: Box<dyn crate::host::Host>,
    ) -> anyhow::Result<Self> {
        let archive = Archive::from_bytes(source.read("th08.dat").await?)?;
        // Supervisor_OnRegister 0x4461A0: the exe-check version list, archive only; missing fails start-up (15 §2.10).
        let ver = archive
            .read(crate::scoredat::VER_LIST_NAME)
            .context("error : データのバージョンが違います")?;
        let mut bgm = crate::sound::Bgm::new(&archive.read("thbgm.fmt")?);
        let mut audio = Audio::new(source.clone());
        audio.load_se(&archive);
        audio.load_midi(&archive);
        bgm.midi_ready = audio.midi_available();
        // Config_Load (15 §1.2): th08.cfg and the thbgm.dat header probe (02 §1), then the file is written back.
        // The port's own save first; else import the original's th08.cfg (read-only).
        // TODO(step 8b): game-directory reads (assets, the original's th08.cfg / score.dat imports) via `Source`.
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
            rec: Recorder::new(),
            renderer: Box::new(renderer),
            archive,
            audio,
            input: Input::default(),
            anm: AnmManager::default(),
            rng: Default::default(),
            tasks: Tasks::default(),
            frame: 0,
            game: Default::default(),
            ascii: Default::default(),
            sound: Default::default(),
            bgm,
            sound_log: None,
            effects: Default::default(),
            overlays: Default::default(),
            panel: Default::default(),
            hud_kept: None,
            bomb_areas: Vec::new(),
            timer_pause: 0,
            load_frames: 0,
            gui_queue: Vec::new(),
            se_log: core.se_log,
            core,
            host,
            drawlog: None,
            skip_counter: SKIP_START,
            init_presents: 2,
            info: Default::default(),
            emit: true,
        };
        ctx.game.cfg = cfg;
        ctx.game.ver_list.set(ver);
        // Music mode cfg +0x1F and BGM volume +0x27 (02 §4) from the loaded config.
        ctx.bgm.set_volumes(&ctx.game.cfg);
        ctx.flush_sound();
        if persist {
            ctx.drawlog = ctx
                .core
                .drawlog
                .as_deref()
                .and_then(crate::drawlog::Writer::from_spec);
        }
        ctx.renderer.set_font(ctx.core.font.as_deref());
        if ctx.drawlog.is_some() {
            ctx.renderer.enable_log();
        }
        ctx.rng = crate::rng::Rng::boot(ctx.core.seed, ctx.host.boot_clock_ms());
        if persist {
            // The port's own save first; else import the original's score.dat (read-only).
            // The whole file is loaded once; equivalent to the original's boot PLST take (15 §2.2, INFERRED).
            let file = match ctx.host.read("score.dat") {
                Some(f) => Some(f),
                None => source.read("score.dat").await.ok(),
            };
            ctx.game.load_score(file.as_deref(), ctx.host.ticks_ms());
            ctx.game.persist = true;
        } else {
            ctx.game.load_score(None, ctx.host.ticks_ms());
        }
        // Debug: `TH08_CLRD` overrides clear masks.
        ctx.game.debug_clear_masks(ctx.core.clrd.as_deref());
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
        self.anm.begin_frame();
        if self.info.active() && crate::menu::InfoNotices::calc(self) {
            // Info_Show's own loop: input, Sound_Update, no chains.
            self.sound_update();
            self.frame += 1;
            return Flow::Continue;
        }
        let f = task::run(self, Chain::Calc);
        self.sound_update();
        // Port: ScreenEffect_Register (13 §3) links its nodes at once; effects queued by calc nodes after prio 3 are linked
        // here (their first calc is the next frame either way); callers below prio 3 use `effect::screen_now`.
        crate::effect::register_pending(self);
        self.frame += 1;
        f
    }

    // Sound_Update 0x45D790 (13 §1.3), right after the calc chain (also while paused): with SE off (cfg +0x20) the
    // queue is left as is. Volume = the curve applied to the table volume and cfg +0x28. Menus also call it in place
    // ("sound flush", 12 §2.2, 16 §6.2).
    pub fn sound_update(&mut self) {
        let cfg = self.game.cfg;
        // The BGM command queue runs before the SE flush, also with "SE off" (13 §1.3).
        self.bgm.start_fade();
        if cfg.se_on == 0 || self.sound.is_empty() {
            self.flush_sound();
            return;
        }
        let played = self.sound.drain();
        if self.se_log {
            log::info!("frame {} se {:?}", self.frame, played);
        }
        for (id, pan) in played {
            let vol = crate::config::volume_curve(
                crate::sound::TABLE[id as usize].1 as i32,
                cfg.se_volume,
            );
            self.bgm.play_se(id as usize, pan, vol);
        }
        self.flush_sound();
    }

    // Port: the device executes the `SoundCmd`s queued so far, in order, at each Sound_Update and after the
    // start-up / restart / exit calls; nothing is read back into logic.
    pub fn flush_sound(&mut self) {
        for c in self.bgm.out.drain(..) {
            if let Some(log) = &mut self.sound_log {
                log.push((self.frame, c.clone()));
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
            crate::ascii::apply_pending_captures(&mut self.anm, &mut self.rec);
        }
        f
    }

    // Frame_Run second half (00 §2); Frame_Run only checks the calc runner's result. The draw chain runs when the
    // counter reaches cfg +0x23 (so 0 and 1 both draw every frame); Present follows either way.
    pub fn draw(&mut self) {
        if self.init_presents > 0 && self.game.scene == crate::gamemgr::SCENE_BOOT {
            self.init_presents -= 1;
            self.rec.set_viewport(FULLSCREEN);
            self.rec.clear_depth(Some([0, 0, 0, 255]));
            if self.drawlog.is_some() {
                self.log_frame();
            }
            return;
        }
        if self.info.showing() {
            crate::menu::InfoNotices::draw(self);
            if self.drawlog.is_some() {
                self.log_frame();
            }
            return;
        }
        self.skip_counter = self.skip_counter.wrapping_add(1);
        if self.skip_counter >= self.game.cfg.frame_skip as i8 {
            task::run(self, Chain::Draw);
            self.skip_counter = 0;
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
            crate::session::on_extra_present(self);
        }
    }

    // WinMain exit 0x441B95 (15 §2.2): once the main loop has left (after the last frame's calc; no draw, no Present),
    // Result_Register(2) and the chain teardown flush score.dat (2 RNG draws, 11 §1.5); th08.cfg is written again at
    // WinMain exit (15 §1.2).
    pub fn exit_save(&mut self) {
        // Supervisor teardown 0x446D06, mode 2: the init.mid reset (02 §5).
        self.bgm.midi_reset();
        self.flush_sound();
        self.game.save_score(&mut self.rng, true, &mut *self.host);
        if self.game.persist {
            self.host.write("th08.cfg", &self.game.cfg.to_bytes());
        }
    }

    // WinMain on Frame_Run result 2 (00 §2 "What the restart re-initialises"): Result(2) score.dat flush, chain
    // teardown, Sound_Update, sound / ANM manager released, then Supervisor_Register as at start-up, so the game boots
    // again through logo and loading. The live config, input words and GM fields are kept; no th08.cfg write. Port:
    // the window and device stay (the app applies the window mode), the 1 s sleep and 60 PeekMessage calls are skipped.
    pub fn restart(&mut self) -> anyhow::Result<()> {
        // Step 1: Result_Register(2) + teardown save score.dat; step 5 reloads it (the written bytes = the file).
        let score = self.game.save_score(&mut self.rng, true, &mut *self.host);
        task::clear(self);
        self.sound_update();
        self.bgm.stop_bgm();
        self.bgm.release_bgm();
        // Supervisor teardown 0x446D06, mode 2: the init.mid reset (02 §5).
        self.bgm.midi_reset();
        self.flush_sound();
        for slot in 0..crate::anmmgr::SLOTS {
            self.anm.unload(&mut self.rec, slot);
        }
        // New ANM manager; g_GameSpeed := 1.0 (step 5; the port keeps it in the manager). Step 6: g_Rng state :=
        // (u16) timeGetTime(), the call counter +4 is kept (11 §1); AsciiManager_Load zeroes the ASCII object.
        self.rng.state = crate::rng::Rng::boot(self.core.seed, self.host.boot_clock_ms()).state;
        // Supervisor flag 0x20 survives: the Supervisor object is not re-constructed (00 §2).
        let restore = self.anm.slowdown_restore;
        self.anm = AnmManager::default();
        self.anm.speed = 1.0;
        self.anm.slowdown_restore = restore;
        self.info = Default::default();
        self.ascii = Default::default();
        self.skip_counter = SKIP_START;
        self.init_presents = 2;
        let now = self.host.ticks_ms();
        self.game.load_score(Some(&score), now);
        // Step 3: scene +0x158 := 0; the original's request −1 (switch to the menu on the first calc) is the port's
        // boot request of Supervisor_Init; +0x160 stays until the boot → menu switch writes it.
        self.game.scene = crate::gamemgr::SCENE_BOOT;
        self.game.requested = crate::gamemgr::SCENE_BOOT;
        crate::supervisor::register(self)
    }

    // Debug tool. TH08_DRAWLOG: one F record per presented frame, sampled after the draw.
    fn log_frame(&mut self) {
        let recs = self.renderer.take_log(&mut self.rec);
        let g = &self.game;
        let game_frame = if g.scene == crate::gamemgr::SCENE_GAME {
            g.frame_counter
        } else {
            0
        };
        let demo = (g.flags & crate::gamemgr::F_DEMO != 0).then_some((g.demo, g.demo_frame));
        let input = if g.scene == crate::gamemgr::SCENE_GAME {
            self.input.game[0]
        } else {
            self.input.held
        };
        let scene = g.scene;
        if let Some(w) = &mut self.drawlog {
            w.frame(scene, game_frame, demo, input, &recs);
        }
    }

    // Tools and tests: their own host type (`platform::ToolHost`), None for another host.
    pub fn host_as<T: 'static>(&mut self) -> Option<&mut T> {
        self.host.as_any_mut().downcast_mut()
    }

    // Starts an in-memory draw log (tests and tools).
    pub fn log_to_memory(&mut self) {
        self.drawlog = Some(crate::drawlog::Writer::memory());
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
        let dir = crate::settings::test_game_dir()?;
        let gfx = pollster::block_on(crate::gfx::Gfx::headless()).ok()?;
        let src = crate::assets::Source::native(dir.into());
        Some(
            pollster::block_on(super::Ctx::load_with(
                src,
                gfx,
                false,
                crate::settings::get().core(),
                crate::settings::get().tool_host(),
            ))
            .unwrap(),
        )
    }

    // 00 §2 restart: back to the boot scene, then logo and loading reach the menu again with the live config kept.
    #[test]
    fn restart_boots_again() {
        let Some(mut c) = ctx() else {
            return;
        };
        let to_menu = |c: &mut super::Ctx| {
            (0..2000)
                .any(|_| c.run_frame().eq(&super::Flow::Continue) && c.game.scene == SCENE_MENU)
        };
        assert!(to_menu(&mut c));
        c.game.cfg.windowed ^= 1;
        let mode = c.game.cfg.windowed;
        c.restart().unwrap();
        assert_eq!(c.game.scene, SCENE_BOOT);
        assert!(to_menu(&mut c));
        assert_eq!(c.game.cfg.windowed, mode);
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
        assert_eq!(c.game.scene, SCENE_MENU);
    }

    // 13 §3: a screen effect queued in a fast-forward pass is linked before the restarted pass.
    #[test]
    fn restart_links_pending_screen_effects() {
        use crate::task::TaskCtx;
        let Some(mut c) = ctx() else {
            return;
        };
        c.effects.screen(1, 10, 2, 0, 0, 0x10);
        c.on_restart();
        assert!(c.effects.take_screen().is_empty());
    }
}
