//! Port menu "Quit to title": Esc and Q pulses into the game's input, so its own pause menu quits.

use th08_core::gamemgr::GameFlags;
use th08_core::input::{KEY_Q, PAUSE};
use th08_core::types::SceneId;
use th08_core::world::World;

// Frames to wait after a pulse before sending again, and the most frames before giving up.
const SETTLE: u32 = 30;
const LIMIT: u32 = 600;

// The game's pause menu (and its Q shortcut) can open: a stage of a played game, no replay or demo, no continue menu.
pub fn available(w: &World) -> bool {
    let g = &w.game;
    g.scene == SceneId::Game && !g.replay() && !g.has(GameFlags::DEMO) && g.game_over == 0
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Quit {
    frames: u32,
    wait: u32,
}

impl Quit {
    // Before a logic frame: Esc while the pause menu is closed, Q once it is open; `pending`: a pulse is still
    // queued. Some(None): wait; None: done or given up.
    pub fn next(&mut self, w: &World, pending: bool) -> Option<Option<u16>> {
        let g = &w.game;
        if self.frames >= LIMIT {
            log::warn!("quit to title: the pause menu did not answer");
            return None;
        }
        if g.scene != SceneId::Game {
            return None;
        }
        self.frames += 1;
        if pending || self.wait > 0 {
            self.wait = self.wait.saturating_sub(1);
            return Some(None);
        }
        self.wait = SETTLE;
        Some(Some(if g.paused == 0 { PAUSE } else { KEY_Q }))
    }
}
