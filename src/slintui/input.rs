//! winit and gamepad input into a Slint window; records the last input modality.
use super::host::Adapter;
use crate::platform::portmenu::Nav;
use slint::LogicalPosition;
use slint::platform::{Key, PointerEventButton, WindowEvent as S};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};

// Logical px a wheel line scrolls.
const WHEEL_LINE: f32 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointing {
    Mouse,
    Touch,
}

#[derive(Default)]
pub struct Pointer {
    pos: LogicalPosition,
    finger: Option<u64>,
}

impl Pointer {
    fn at(&mut self, p: PhysicalPosition<f64>, scale: f32) -> LogicalPosition {
        self.pos = LogicalPosition::new(p.x as f32 / scale, p.y as f32 / scale);
        self.pos
    }

    // Feeds one window event in; returns the modality it implies, None when the event is not pointer input.
    pub fn event(&mut self, a: &Adapter, ev: &WindowEvent, scale: f32) -> Option<Pointing> {
        let left = PointerEventButton::Left;
        match ev {
            WindowEvent::CursorMoved { position, .. } => {
                let position = self.at(*position, scale);
                a.dispatch(S::PointerMoved { position });
                Some(Pointing::Mouse)
            }
            WindowEvent::CursorLeft { .. } => {
                a.dispatch(S::PointerExited);
                Some(Pointing::Mouse)
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => left,
                    MouseButton::Right => PointerEventButton::Right,
                    MouseButton::Middle => PointerEventButton::Middle,
                    _ => PointerEventButton::Other,
                };
                let position = self.pos;
                a.dispatch(match state {
                    ElementState::Pressed => S::PointerPressed { position, button },
                    ElementState::Released => S::PointerReleased { position, button },
                });
                Some(Pointing::Mouse)
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (delta_x, delta_y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * WHEEL_LINE, y * WHEEL_LINE),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32 / scale, p.y as f32 / scale),
                };
                a.dispatch(S::PointerScrolled {
                    position: self.pos,
                    delta_x,
                    delta_y,
                });
                Some(Pointing::Mouse)
            }
            WindowEvent::Touch(t) => {
                if self.finger.is_some_and(|f| f != t.id) {
                    return None;
                }
                let position = self.at(t.location, scale);
                match t.phase {
                    TouchPhase::Started => {
                        self.finger = Some(t.id);
                        a.dispatch(S::PointerMoved { position });
                        a.dispatch(S::PointerPressed {
                            position,
                            button: left,
                        });
                    }
                    TouchPhase::Moved => a.dispatch(S::PointerMoved { position }),
                    TouchPhase::Ended => {
                        self.finger = None;
                        a.dispatch(S::PointerReleased {
                            position,
                            button: left,
                        });
                        a.dispatch(S::PointerExited);
                    }
                    TouchPhase::Cancelled => {
                        self.finger = None;
                        a.dispatch(S::PointerExited);
                    }
                }
                Some(Pointing::Touch)
            }
            _ => None,
        }
    }
}

// Keys, pad buttons and stick edges all reach the page as these keys.
pub fn send_nav(a: &Adapter, n: Nav) {
    let k = match n {
        Nav::Up => Key::UpArrow,
        Nav::Down => Key::DownArrow,
        Nav::Left => Key::LeftArrow,
        Nav::Right => Key::RightArrow,
        Nav::Activate => Key::Return,
        Nav::Back => Key::Escape,
        Nav::PrevTab => Key::PageUp,
        Nav::NextTab => Key::PageDown,
    };
    let text: slint::SharedString = k.into();
    a.dispatch(S::KeyPressed { text: text.clone() });
    a.dispatch(S::KeyReleased { text });
}
