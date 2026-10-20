//! Global engine state and the per-frame calc / draw halves of `Frame_Run` 0x441E70 (00 §2).

use crate::anmmgr::AnmManager;
use crate::archive::Archive;
use crate::assets::Source;
use crate::audio::Audio;
use crate::gfx::Gfx;
use crate::input::Input;
use crate::task::{self, Chain, Flow, TaskCtx, Tasks};

// The original's globals that every subsystem reaches (device, archive, sound manager, input words, SprtCtrlInf,
// funcChain); `game` holds GM / Supervisor fields (11 §2, §7).
pub struct Ctx {
    pub gfx: Gfx,
    pub archive: Archive,
    pub audio: Audio,
    pub input: Input,
    pub anm: AnmManager,
    pub tasks: Tasks<Ctx>,
    // Port count of executed calc steps (debug tools, TH08_SHOT); not an original counter.
    pub frame: u64,
    pub game: crate::gamemgr::Game,
    pub ascii: crate::ascii::Queue,
    // Debug: `TH08_SE_LOG` logs each frame's flushed SE ids.
    pub se_log: bool,
    pub settings: &'static crate::settings::Settings,
    pub drawlog: Option<crate::drawlog::Writer>,
}

impl TaskCtx for Ctx {
    fn tasks(&mut self) -> &mut Tasks<Self> {
        &mut self.tasks
    }
}

impl Ctx {
    pub async fn load(source: Source, gfx: Gfx) -> anyhow::Result<Self> {
        Self::load_with(source, gfx, true).await
    }

    // WinMain start-up (00 §2): archive, sound (thbgm.fmt 02 §2, SE 13 §1.1), RNG boot seed (11 §1.3), score data, then the
    // Supervisor task. `persist` = read/write the port's save files (off for tests).
    // SPEC-GAP: th08.cfg is not read at boot (15 §1.2); the config keeps its defaults.
    pub async fn load_with(source: Source, gfx: Gfx, persist: bool) -> anyhow::Result<Self> {
        let archive = Archive::from_bytes(source.read("th08.dat").await?)?;
        let settings = crate::settings::get();
        let mut audio = Audio::new(source.clone(), &archive.read("thbgm.fmt")?);
        audio.load_se(&archive);
        let mut ctx = Self {
            gfx,
            archive,
            audio,
            input: Input::default(),
            anm: AnmManager::default(),
            tasks: Tasks::default(),
            frame: 0,
            game: Default::default(),
            ascii: Default::default(),
            se_log: settings.se_log,
            settings,
            drawlog: None,
        };
        if persist {
            ctx.drawlog = settings
                .drawlog
                .as_deref()
                .and_then(crate::drawlog::Writer::from_spec);
        }
        if ctx.drawlog.is_some() {
            ctx.gfx.enable_log();
        }
        ctx.anm.rng = crate::rng::Rng::boot();
        if persist {
            // The port's own save first; else import the original's score.dat (read-only).
            // SPEC-GAP: the whole file is loaded here; at boot the original takes only PLST (15 §2.2).
            let file = match crate::scoredat::storage_read("score.dat") {
                Some(f) => Some(f),
                None => source.read("score.dat").await.ok(),
            };
            ctx.game.load_score(file.as_deref());
            ctx.game.persist = true;
        } else {
            ctx.game.load_score(None);
        }
        // Debug: `TH08_CLRD` overrides clear masks.
        ctx.game.debug_clear_masks(settings.clrd.as_deref());
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
        let f = task::run(self, Chain::Calc);
        self.sound_update();
        // Port: ScreenEffect_Register (13 §3) links its nodes at once; effects queued by calc nodes after prio 3 are linked
        // here (their first calc is the next frame either way); callers below prio 3 use `effect::screen_now`.
        crate::effect::register_pending(self);
        self.frame += 1;
        f
    }

    // Sound_Update 0x45D790 (13 §1.3), right after the calc chain (also while paused): with SE off (cfg +0x20) the
    // queue is left as is. Volume = the curve applied to the table volume and cfg +0x28.
    fn sound_update(&mut self) {
        let cfg = self.game.cfg;
        // SPEC-GAP: whether "SE off" also skips the BGM command queue (02 §4 command 5) is unclear; it does not here.
        self.audio.start_fade();
        if cfg.se_on == 0 || !crate::sound::pending() {
            return;
        }
        let played = crate::sound::take();
        if self.se_log {
            log::info!("frame {} se {:?}", self.frame, played);
        }
        for (id, pan) in played {
            let vol = crate::config::volume_curve(
                crate::sound::TABLE[id as usize].1 as i32,
                cfg.se_volume,
            );
            self.audio.play_se(id as usize, pan, vol);
        }
    }

    // Frame_Run second half (00 §2); Frame_Run only checks the calc runner's result.
    // SPEC-GAP: frame skip (cfg +0x23) is not kept, so the draw chain runs every frame (the default).
    pub fn draw(&mut self) {
        task::run(self, Chain::Draw);
        if self.drawlog.is_some() {
            self.log_frame();
        }
    }

    // Device_ClearPresentTwice 0x45B020 (16 §1.2 step 1): clear colour (`color`) and z 1.0, Present, twice; called from
    // inside a calc node, so these presents carry no primitives and shift the present numbering by 2.
    pub fn clear_present_twice(&mut self, color: [u8; 4]) {
        for _ in 0..2 {
            self.gfx.set_viewport(crate::gfx::FULLSCREEN);
            self.gfx.clear_depth(Some(color));
            if self.drawlog.is_some() {
                self.log_frame();
            }
            self.gfx.present();
        }
    }

    // WinMain exit 0x441B95 (15 §2.2): once the main loop has left (after the last frame's calc, draw and Present),
    // Result_Register(2) and the chain teardown flush score.dat (2 RNG draws, 11 §1.5).
    pub fn exit_save(&mut self) {
        self.game.save_score(&mut self.anm.rng, true);
    }

    // Debug tool. TH08_DRAWLOG: one F record per presented frame, sampled after the draw.
    fn log_frame(&mut self) {
        let recs = self.gfx.take_log();
        let g = &self.game;
        let game_frame = if g.scene == crate::gamemgr::SCENE_GAME {
            g.frame_counter
        } else {
            0
        };
        let demo = (g.flags & crate::gamemgr::F_DEMO != 0).then_some((g.demo, g.demo_frame));
        let input = if g.scene == crate::gamemgr::SCENE_GAME {
            self.input.game
        } else {
            self.input.held
        };
        let scene = g.scene;
        if let Some(w) = &mut self.drawlog {
            w.frame(scene, game_frame, demo, input, &recs);
        }
    }

    // Starts an in-memory draw log (tests and tools).
    pub fn log_to_memory(&mut self) {
        self.drawlog = Some(crate::drawlog::Writer::memory());
        self.gfx.enable_log();
    }
}
