//! Deterministic game state of the original's globals (ARCH_A §2.3 `World`, STEP9_WORLD.md 9h): everything a frame's
//! calc and draw chains read and write; `Ctx` adds only the IO / output side (renderer, host, recorder, audio, tools).

use crate::anmmgr::AnmManager;
use crate::input::Input;
use crate::task::Tasks;

// WinMain 0x441A70: skip counter byte := 0xFC at the first start and every restart (00 §2), first draw in frame 4.
pub(crate) const SKIP_START: i8 = -4;

#[derive(Clone)]
pub struct World {
    pub input: Input,
    // ANM manager (03 §2.4, 05 §8); its clone shares the parsed scripts (`Arc`, copy on write) and texture handles.
    pub anm: AnmManager,
    // The game RNG g_Rng 0x164D520 (11 §1); every draw site keeps the original's order.
    pub rng: crate::rng::Rng,
    // funcChain (00 §2, 9h part 1: `Node` payload).
    pub tasks: Tasks<crate::chain::Node>,
    // Port count of executed calc steps (debug tools, TH08_SHOT, sound log stamps); not an original counter.
    pub frame: u64,
    pub game: crate::gamemgr::Game,
    pub ascii: crate::ascii::Queue,
    // Sound manager SE queue (13 §1.2), flushed by `sound_update`.
    pub sound: crate::sound::Queue,
    // Sound manager BGM state + the frame's `SoundCmd` output (02 §4–§5, 13 §1.3), executed by `Ctx.audio`.
    pub bgm: crate::sound::Bgm,
    // Effect manager g_EffectMgr 0x4ECE60 (13 §2).
    pub effects: crate::effect::Effects,
    // Ascii manager popups, darkness mask, gauge readout and boss markers (13 §4, 12 §5.9–§5.10).
    pub overlays: crate::ascii::Overlays,
    // HUD dirty fields + full-repaint counter 0x17CE8CC (12 §5.3).
    pub panel: crate::hud::Panel,
    // HUD impl block kept between Hud_OnRemove and the next Hud_Load (12 §5.1a).
    pub hud_kept: Option<crate::hud::Kept>,
    // Player 0x17D5EF8 (09 §1): created by GameManager_Register, dropped with the game manager by the scene switch
    // (11 §7). A `World` field next to the GM so `sys!` can lend it with the other fields (STEP9_WORLD.md 9a).
    pub player: Option<Box<crate::player::Player>>,
    // Bullet manager 0xF54E90 (08 §2), with the item manager it holds: created by GameMgr's stage start (11 §3.1),
    // replaced by the next one, dropped with the game manager (11 §7).
    pub bullets: Option<Box<crate::bullet::BulletManager>>,
    // Enemy manager 0x577F20 (07 §4): created by GameMgr's stage start (11 §3.1), dropped with the game manager (11 §7).
    pub enemies: Option<Box<crate::enemy::EnemyManager>>,
    // Stage g_Stage 0x4E4030 (06 §3): created by GameMgr's stage start (11 §3.1), dropped with the game manager (11 §7).
    pub stage: Option<Box<crate::stage::Stage>>,
    // HUD g_Hud 0x160F428 (12 §5) and spell manager g_Gui 0x4EA670 (12 §5.8): registered by GameMgr's stage start
    // (11 §3.1), dropped with the game manager (11 §7).
    pub hud: Option<Box<crate::hud::Hud>>,
    pub gui: Option<Box<crate::gui::Gui>>,
    // Dialogue state at HudImpl+0x21814 (10 §3), created with the HUD; read by the ECL (`Sys.msg`) and GameMgr_Calc.
    pub msg: Option<Box<crate::msg::Dialogue>>,
    // Game manager 0x164D0B0 (11 §2–§4) with its replay object: created by GameManager_Register, dropped by the
    // scene switch (11 §7).
    pub gm: Option<Box<crate::gamemgr::GameManager>>,
    // Supervisor g_Supervisor 0x17CE758 (11 §7, 12 §1), registered at start-up and by a restart (00 §2); its chain
    // nodes are `SupervisorTask`.
    pub supervisor: Option<Box<crate::supervisor::Supervisor>>,
    // The registered menu-side scene object (MainMenu, Result, Ending, MusicRoom; 11 §7), switched by the
    // Supervisor; its chain nodes are `SceneTask`. The game scene's GM is `gm`.
    pub scene: crate::supervisor::Scene,
    // ASCII manager (13 §4, 12 §3–§4), registered by Supervisor on-register; chain nodes `AsciiTask`.
    pub ascii_mgr: Option<Box<crate::ascii::AsciiManager>>,
    // Debug ANM viewer (TH08_ANM), not a port; chain nodes `AnmDebugTask`.
    pub anm_debug: Option<Box<crate::anmdebug::AnmDebug>>,
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
    // Frame_Run frame-skip counter (i8, 00 §2), compared with cfg +0x23; SKIP_START after every (re)start.
    pub skip_counter: i8,
    // Port: Window_InitD3D's two clear + Present (12 §1 boot presents 1–2) run as the first two port frames, outside
    // Frame_Run, so they neither run the draw chain nor step the skip counter (presents 3–5 are its −3…−1).
    pub init_presents: u8,
    // Unlock notices (16 §4), shown in place of the frame's chains.
    pub info: crate::menu::InfoNotices,
}

impl World {
    pub(crate) fn new(bgm: crate::sound::Bgm) -> Self {
        World {
            input: Input::default(),
            anm: AnmManager::default(),
            rng: Default::default(),
            tasks: Tasks::default(),
            frame: 0,
            game: Default::default(),
            ascii: Default::default(),
            sound: Default::default(),
            bgm,
            effects: Default::default(),
            overlays: Default::default(),
            panel: Default::default(),
            hud_kept: None,
            player: None,
            bullets: None,
            enemies: None,
            stage: None,
            hud: None,
            gui: None,
            msg: None,
            gm: None,
            supervisor: None,
            scene: Default::default(),
            ascii_mgr: None,
            anm_debug: None,
            bomb_areas: Vec::new(),
            timer_pause: 0,
            load_frames: 0,
            gui_queue: Vec::new(),
            skip_counter: SKIP_START,
            init_presents: 2,
            info: Default::default(),
        }
    }
}

impl World {
    // WinMain exit's file part (15 §2.2): ScoreDat_Save with Result mode 2 (2 RNG draws, 11 §1.5) and the th08.cfg
    // write; the score.dat bytes.
    pub(crate) fn save_files(&mut self, host: &mut dyn crate::host::Host) -> Vec<u8> {
        let bytes = self.game.save_score(&mut self.rng, true, host);
        if self.game.persist {
            host.write("th08.cfg", &self.game.cfg.to_bytes());
        }
        bytes
    }
}
