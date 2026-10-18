use crate::anmmgr::AnmManager;
use crate::archive::Archive;
use crate::assets::Source;
use crate::audio::Audio;
use crate::gfx::Gfx;
use crate::input::Input;
use crate::task::{self, Chain, Flow, TaskCtx, Tasks};

pub struct Ctx {
    pub gfx: Gfx,
    pub archive: Archive,
    pub audio: Audio,
    pub input: Input,
    pub anm: AnmManager,
    pub tasks: Tasks<Ctx>,
    pub frame: u64,
    pub game: crate::gamemgr::Game,
    pub ascii: crate::ascii::Queue,
    pub se_log: bool,
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

    // `persist` = read/write the port's save files (off for tests).
    pub async fn load_with(source: Source, gfx: Gfx, persist: bool) -> anyhow::Result<Self> {
        let archive = Archive::from_bytes(source.read("th08.dat").await?)?;
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
            se_log: crate::env::var("TH08_SE_LOG").is_some(),
            drawlog: None,
        };
        if persist {
            ctx.drawlog = crate::drawlog::Writer::from_env();
        }
        if ctx.drawlog.is_some() {
            ctx.gfx.enable_log();
        }
        ctx.anm.rng = crate::rng::Rng::boot();
        if persist {
            // The port's own save first; else import the original's score.dat (read-only).
            let file = match crate::scoredat::storage_read("score.dat") {
                Some(f) => Some(f),
                None => source.read("score.dat").await.ok(),
            };
            ctx.game.load_score(file.as_deref());
            ctx.game.persist = true;
        } else {
            ctx.game.load_score(None);
        }
        ctx.game.debug_clear_masks();
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

    pub fn calc(&mut self) -> Flow {
        self.anm.begin_frame();
        let f = task::run(self, Chain::Calc);
        self.sound_update();
        crate::effect::register_pending(self);
        self.frame += 1;
        f
    }

    // Sound_Update (13 §1.3): with SE off the queue is left as is.
    fn sound_update(&mut self) {
        let cfg = self.game.cfg;
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

    // Frame_Run only checks the calc runner's result.
    pub fn draw(&mut self) {
        task::run(self, Chain::Draw);
        if self.drawlog.is_some() {
            self.log_frame();
        }
    }

    // TH08_DRAWLOG: one F record per presented frame, sampled after the draw.
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
