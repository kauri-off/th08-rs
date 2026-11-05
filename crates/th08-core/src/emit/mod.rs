//! Emit dispatcher: records each draw node's draw commands from a read-only `View`.

use crate::chain::Node;
use crate::drawlist::{FULLSCREEN, Recorder};
use crate::game::View;

pub struct Emit;

impl Emit {
    // One draw node, right after its draw-phase step.
    pub fn node(&self, v: &View<'_>, node: Node, tag: u8, rec: &mut Recorder) {
        match node {
            Node::Supervisor => crate::supervisor::SupervisorTask.emit(v, rec, tag),
            Node::Scene => crate::supervisor::SceneTask.emit(v, rec, tag),
            Node::Ascii => crate::ascii::AsciiTask.emit(v, rec, tag),
            Node::AnmDebug => crate::anmdebug::AnmDebugTask.emit(v, rec, tag),
            Node::GameManager | Node::Replay => {}
            Node::Player => crate::player::PlayerTask.emit(v, rec, tag),
            Node::Bullets => crate::bullet::BulletTask.emit(v, rec, tag),
            Node::Enemies => crate::enemy::EnemyTask.emit(v, rec, tag),
            Node::Stage => crate::stage::StageTask.emit(v, rec, tag),
            Node::Hud => crate::hud::HudTask.emit(v, rec, tag),
            Node::Gui => crate::gui::GuiTask.emit(v, rec, tag),
            Node::Effects => crate::effect::EffectTask.emit(v, rec, tag),
            Node::Screen { slot } => crate::effect::ScreenTask { slot }.emit(v, rec, tag),
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
