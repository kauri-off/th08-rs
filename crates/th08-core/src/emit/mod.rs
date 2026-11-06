//! Emit dispatcher: records each draw node's draw commands from a read-only `View`.

use crate::chain::Node;
use crate::drawlist::{FULLSCREEN, Recorder};
use crate::game::View;

pub mod anm;
pub mod anmdebug;
pub mod ascii;
pub mod bullet;
pub mod effect;
pub mod ending;
pub mod enemy;
pub mod gui;
pub mod hud;
pub mod menu;
pub mod menulist;
pub mod musicroom;
pub mod player;
pub mod result;
pub mod stage;
pub mod supervisor;

pub struct Emit;

impl Emit {
    // One draw node, right after its draw-phase step.
    pub fn node(&self, v: &View<'_>, node: Node, tag: u8, rec: &mut Recorder) {
        match node {
            Node::Supervisor => supervisor::node(v, rec, tag),
            Node::Scene => supervisor::scene_node(v, rec, tag),
            Node::Ascii => ascii::node(v, rec, tag),
            Node::AnmDebug => anmdebug::node(v, rec, tag),
            Node::GameManager | Node::Replay => {}
            Node::Player => player::node(v, rec, tag),
            Node::Bullets => bullet::node(v, rec, tag),
            Node::Enemies => enemy::node(v, rec, tag),
            Node::Stage => stage::node(v, rec, tag),
            Node::Hud => hud::node(v, rec, tag),
            Node::Gui => gui::node(v, rec, tag),
            Node::Effects => effect::node(v, rec, tag),
            Node::Screen { slot } => effect::screen_node(slot, v, rec, tag),
        }
    }

    // Unlock-notice iteration (16 §4), drawn in place of the chains: the picture full-screen and the black quad
    // (0, 0)–(639, 479).
    pub fn info(&self, v: &View<'_>, rec: &mut Recorder) {
        let Some((tex, _)) = v.w.info.cur else {
            return;
        };
        rec.set_viewport(FULLSCREEN);
        rec.image(tex, 0.0, 0.0);
        if let Some(a) = crate::menu::info_fade_alpha(v.w.info.drawn) {
            rec.rect(0.0, 0.0, 639.0, 479.0, [0, 0, 0, a]);
        }
    }
}
