//! funcChain payload of the game (00 §3): one `Node` value per task object, dispatched to its stateless forwarder.

use crate::drawlist::Recorder;
use crate::game::Ctx;
use crate::task::{Chain, Ret, Task};
use anyhow::Result;

// One task object on the chain (00 §3 node +0x10 object pointer); a forwarder with several nodes tells them apart by
// the chain node's tag. The state is the matching `Ctx` field (docs/arch/STEP9_WORLD.md §3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Node {
    Supervisor,
    Scene,
    Ascii,
    AnmDebug,
    GameManager,
    Replay,
    Player,
    Bullets,
    Enemies,
    Stage,
    Hud,
    Gui,
    Effects,
    // ScreenEffect in pool slot `ctx.effects.screen_fx[slot]` (13 §3).
    Screen { slot: usize },
}

impl Node {
    // The single dispatch: the forwarder of this node.
    fn with<R>(self, f: impl FnOnce(&mut dyn Task<Ctx>) -> R) -> R {
        match self {
            Node::Supervisor => f(&mut crate::supervisor::SupervisorTask),
            Node::Scene => f(&mut crate::supervisor::SceneTask),
            Node::Ascii => f(&mut crate::ascii::AsciiTask),
            Node::AnmDebug => f(&mut crate::anmdebug::AnmDebugTask),
            Node::GameManager => f(&mut crate::gamemgr::GameManagerTask),
            Node::Replay => f(&mut crate::replay::ReplayTask),
            Node::Player => f(&mut crate::player::PlayerTask),
            Node::Bullets => f(&mut crate::bullet::BulletTask),
            Node::Enemies => f(&mut crate::enemy::EnemyTask),
            Node::Stage => f(&mut crate::stage::StageTask),
            Node::Hud => f(&mut crate::hud::HudTask),
            Node::Gui => f(&mut crate::gui::GuiTask),
            Node::Effects => f(&mut crate::effect::EffectTask),
            Node::Screen { slot } => f(&mut crate::effect::ScreenTask { slot }),
        }
    }
}

impl Task<Ctx> for Node {
    fn calc(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        self.with(|t| t.calc(ctx, tag))
    }
    fn draw_step(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        self.with(|t| t.draw_step(ctx, tag))
    }
    fn emit(&self, ctx: &Ctx, rec: &mut Recorder, tag: u8) {
        self.with(|t| t.emit(ctx, rec, tag))
    }
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, tag: u8) -> Result<()> {
        self.with(|t| t.on_register(ctx, chain, tag))
    }
    fn on_remove(&mut self, ctx: &mut Ctx, chain: Chain, tag: u8) {
        self.with(|t| t.on_remove(ctx, chain, tag))
    }
}
