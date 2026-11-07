//! Port menu "Quit to title" / "Give up and retry": Esc then Q / R pulses into the game's input, so its own pause menu acts (12 §3 shortcut keys).

use th08_core::gamemgr::GameFlags;
use th08_core::input::{KEY_Q, KEY_R, PAUSE};
use th08_core::types::SceneId;
use th08_core::world::World;

// Frames to wait after a pulse before sending again, and the most frames before giving up.
const SETTLE: u32 = 30;
const LIMIT: u32 = 600;

// The game's pause menu and its Q / R shortcuts can open: a stage of a played game, no replay or demo (R is not
// taken in replay), no continue menu.
pub fn available(w: &World) -> bool {
    let g = &w.game;
    g.scene == SceneId::Game && !g.replay() && !g.has(GameFlags::DEMO) && g.game_over == 0
}

// Which pause-menu shortcut the pulses end with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Exit {
    #[default]
    Title,
    Retry,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Quit {
    pub exit: Exit,
    frames: u32,
    wait: u32,
    // Q / R was sent with the pause menu open.
    keyed: bool,
}

impl Quit {
    pub fn new(exit: Exit) -> Self {
        Self {
            exit,
            ..Self::default()
        }
    }

    // Before a logic frame: Esc while the pause menu is closed, Q / R once it is open; `pending`: a pulse is still
    // queued. Some(None): wait; None: done or given up. Done = the stage scene is left, or the pause menu closed
    // after Q / R (a retry reloads the stage within one frame, 12 §3 state 10).
    pub fn next(&mut self, w: &World, pending: bool) -> Option<Option<u16>> {
        let g = &w.game;
        if self.frames >= LIMIT {
            log::warn!("{:?}: the pause menu did not answer", self.exit);
            return None;
        }
        if g.scene != SceneId::Game || (self.keyed && g.paused == 0 && !pending) {
            return None;
        }
        self.frames += 1;
        if pending || self.wait > 0 {
            self.wait = self.wait.saturating_sub(1);
            return Some(None);
        }
        self.wait = SETTLE;
        let key = match self.exit {
            Exit::Title => KEY_Q,
            Exit::Retry => KEY_R,
        };
        self.keyed |= g.paused != 0;
        Some(Some(if g.paused == 0 { PAUSE } else { key }))
    }
}
