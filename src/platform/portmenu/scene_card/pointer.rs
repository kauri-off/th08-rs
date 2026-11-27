use super::{ACCEPT, KEEP, LESS, MORE, PICK, ROW, TRACK_TAG, target};
use crate::menu_model::{Effect, Hit, Input, MenuModel, Target};
use crate::settings_store::CtlOut;
use th08_ui::scene::{Hit as Region, Scene};
use winit::event::{ElementState, MouseButton, TouchPhase, WindowEvent};

#[derive(Default)]
pub(in crate::platform::portmenu) struct Pointer {
    mouse: Option<[f32; 2]>,
    finger: Option<u64>,
    grab: Option<Grab>,
}

struct Grab {
    hit: Region,
    from: [f32; 2],
    motion: Motion,
}

#[derive(Clone, Copy)]
enum Motion {
    Still,
    Track,
    Cancelled,
}

#[derive(Clone, Copy)]
enum Event {
    Down([f32; 2]),
    Move([f32; 2]),
    Up([f32; 2]),
    Leave,
}

impl Event {
    fn point(self) -> Option<[f32; 2]> {
        match self {
            Self::Down(p) | Self::Move(p) | Self::Up(p) => Some(p),
            Self::Leave => None,
        }
    }
}

impl Pointer {
    pub(in crate::platform::portmenu) fn pressed(&self) -> Option<u32> {
        self.grab
            .as_ref()
            .filter(|g| !matches!(g.motion, Motion::Cancelled))
            .map(|g| g.hit.tag)
    }

    pub(in crate::platform::portmenu) fn cancel(&mut self) {
        self.grab = None;
    }

    pub(in crate::platform::portmenu) fn event(
        &mut self,
        event: &WindowEvent,
        scene: &Scene,
        model: &mut MenuModel,
    ) -> Vec<Effect> {
        let (ev, touch) = match event {
            WindowEvent::Touch(t) => {
                let at = [t.location.x as f32, t.location.y as f32];
                let ev = match t.phase {
                    TouchPhase::Started if self.finger.is_none() => {
                        self.finger = Some(t.id);
                        Event::Down(at)
                    }
                    TouchPhase::Moved if self.finger == Some(t.id) => Event::Move(at),
                    TouchPhase::Ended if self.finger == Some(t.id) => {
                        self.finger = None;
                        Event::Up(at)
                    }
                    TouchPhase::Cancelled if self.finger == Some(t.id) => {
                        self.finger = None;
                        Event::Leave
                    }
                    _ => return Vec::new(),
                };
                (ev, true)
            }
            WindowEvent::Focused(false) => {
                *self = Self::default();
                return model.apply(Input::HoverEnd);
            }
            _ if self.finger.is_some() => return Vec::new(),
            WindowEvent::CursorMoved { position, .. } => {
                let at = [position.x as f32, position.y as f32];
                self.mouse = Some(at);
                (Event::Move(at), model.context().overlay)
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let Some(at) = self.mouse else {
                    return Vec::new();
                };
                (
                    match state {
                        ElementState::Pressed => Event::Down(at),
                        ElementState::Released => Event::Up(at),
                    },
                    model.context().overlay,
                )
            }
            WindowEvent::CursorLeft { .. } => {
                self.mouse = None;
                (Event::Leave, model.context().overlay)
            }
            _ => return Vec::new(),
        };
        self.apply(ev, touch, scene, model)
    }

    fn apply(
        &mut self,
        ev: Event,
        touch: bool,
        scene: &Scene,
        model: &mut MenuModel,
    ) -> Vec<Effect> {
        let mut effects = model.apply(Input::Pointer { touch });
        let at = ev.point().map(|[x, y]| [x / scene.scale, y / scene.scale]);
        let hit = at
            .and_then(|[x, y]| scene.hit_at(x, y))
            .and_then(|h| target(h.tag, model).map(|t| (h, t)));
        if let Some((_, t)) = hit {
            effects.extend(model.apply(Input::Over(t)));
        }
        let hit = hit.map(|(h, _)| h);
        effects.extend(model.apply(Input::HoverEnd));
        match ev {
            Event::Down(_) => {
                self.grab = hit.zip(at).map(|(hit, from)| Grab {
                    hit,
                    from,
                    motion: Motion::Still,
                });
            }
            Event::Move(_) | Event::Up(_) => {
                if let Some(g) = &mut self.grab
                    && let Some([x, y]) = at
                {
                    let dx = (x - g.from[0]).abs();
                    let dy = (y - g.from[1]).abs();
                    if matches!(g.motion, Motion::Still) && dx.max(dy) > 8.0 {
                        g.motion = if g.hit.tag & 255 == TRACK_TAG && (dx >= dy || !touch) {
                            Motion::Track
                        } else {
                            Motion::Cancelled
                        };
                    }
                    let was_up = matches!(ev, Event::Up(_));
                    let fire = match g.motion {
                        Motion::Track => true,
                        Motion::Still => was_up && hit.is_some_and(|h| h.tag == g.hit.tag),
                        Motion::Cancelled => false,
                    };
                    if fire && let Some(input) = activation(g.hit, model, x) {
                        effects.extend(model.apply(input));
                    }
                }
                if matches!(ev, Event::Up(_)) {
                    self.grab = None;
                }
            }
            Event::Leave => self.grab = None,
        }
        effects
    }
}

fn activation(hit: Region, m: &MenuModel, x: f32) -> Option<Input> {
    let code = hit.tag;
    let target = match target(code, m)? {
        Hit::Item(i) => Target::Item(i),
        Hit::Yes(yes) => Target::Answer(yes),
        Hit::Row(_) => return None,
    };
    if matches!(code, KEEP | ACCEPT) {
        return Some(Input::Tap(target));
    }
    let control = match code & 255 {
        ROW => return Some(Input::Tap(target)),
        LESS => CtlOut::Step(-1),
        MORE => CtlOut::Step(1),
        TRACK_TAG => CtlOut::Frac(((x - hit.rect.x) / hit.rect.w).clamp(0.0, 1.0)),
        pick => CtlOut::Pick((pick - PICK) as usize),
    };
    Some(Input::Control(target, control))
}
