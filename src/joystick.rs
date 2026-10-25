//! Joystick device for `Input_PollJoystick` 0x43D120 (15 §1.1): the first connected gamepad via gilrs (the browser
//! Gamepad API on wasm); the mapping itself is `input::joystick_bits`.

use crate::input::PadState;

// Port: gilrs reports standard buttons, not DirectInput indices; this order stands in for rgbButtons[0..16]
// (defaults Shot 0 = south, Bomb 1 = east, Focus 2 = west, Skip 3 = north, Pause 4 = left shoulder).
#[cfg(any(not(target_os = "linux"), feature = "gamepad-linux"))]
const BUTTONS: [gilrs::Button; 16] = {
    use gilrs::Button::*;
    [
        South,
        East,
        West,
        North,
        LeftTrigger,
        RightTrigger,
        LeftTrigger2,
        RightTrigger2,
        Select,
        Start,
        LeftThumb,
        RightThumb,
        DPadUp,
        DPadDown,
        DPadLeft,
        DPadRight,
    ]
};

#[derive(Default)]
pub struct Joystick {
    #[cfg(any(not(target_os = "linux"), feature = "gamepad-linux"))]
    gilrs: Option<gilrs::Gilrs>,
}

impl Joystick {
    pub fn new() -> Self {
        Self {
            #[cfg(any(not(target_os = "linux"), feature = "gamepad-linux"))]
            gilrs: gilrs::Gilrs::new()
                .map_err(|e| log::warn!("joystick: {e}"))
                .ok(),
        }
    }

    // One poll: buttons and lX/lY of the first connected gamepad; all released when there is none.
    pub fn poll(&mut self) -> PadState {
        #[cfg(any(not(target_os = "linux"), feature = "gamepad-linux"))]
        if let Some(g) = &mut self.gilrs {
            while g.next_event().is_some() {}
            if let Some((_, pad)) = g.gamepads().next() {
                let buttons = BUTTONS
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| pad.is_pressed(**b))
                    .fold(0u32, |m, (i, _)| m | 1 << i);
                // DIPROP_RANGE ±1000 on every axis (15 §1.1, EnumObjects callback 0x445E3D).
                // gilrs Y is up-positive, DirectInput lY down-positive.
                let axis = [
                    (pad.value(gilrs::Axis::LeftStickX) * 1000.0) as i32,
                    (-pad.value(gilrs::Axis::LeftStickY) * 1000.0) as i32,
                ];
                return PadState { buttons, axis };
            }
        }
        PadState::default()
    }
}
