//! Joystick device for `Input_PollJoystick` 0x43D120 (15 §1.1): the first connected gilrs gamepad, or the gamepad events the window delivers (Android).

use th08_core::input::PadState;

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

// Android KEYCODE_* of the same 16 buttons, in `BUTTONS` order (A B X Y L1 R1 L2 R2 SELECT START THUMBL THUMBR,
// DPAD_UP/DOWN/LEFT/RIGHT).
pub const ANDROID_KEYS: [u32; 16] = [
    96, 97, 99, 100, 102, 103, 104, 105, 109, 108, 106, 107, 19, 20, 21, 22,
];

// Button index of Select / Back / View: opens the port menu, like Android Back.
pub const MENU_BUTTON: usize = 8;

// Stick deflection (of ±1000) that counts as using the pad (hides the touch overlay).
const IN_USE: i32 = 300;

// Hat axis value (−1..1) that counts as a d-pad press.
const HAT_DEAD: f32 = 0.5;

// Gamepad button index of an Android keycode.
pub fn android_button(code: u32) -> Option<usize> {
    ANDROID_KEYS.iter().position(|&k| k == code)
}

// DIPROP_RANGE ±1000 on every axis (EnumObjects callback 0x445E3D); `v` is a −1..1 deflection.
fn axis_value(v: f32) -> i32 {
    (v.clamp(-1.0, 1.0) * 1000.0) as i32
}

// A gamepad fed by window events (Android key and joystick motion events).
#[derive(Clone, Debug, Default)]
pub struct EventPad {
    // Buttons, and lX/lY: the stick, with a pressed hat direction at full deflection over it.
    pub state: PadState,
    stick: [i32; 2],
    // Hat direction per axis: −1, 0 or 1.
    hat: [i32; 2],
    // Connected gamepad names (Android: InputDevice enumeration).
    pub devices: Vec<String>,
    // Any button or stick input seen since the last release.
    seen: bool,
}

impl EventPad {
    pub fn button(&mut self, i: usize, down: bool) {
        if down {
            self.state.buttons |= 1 << i;
        } else {
            self.state.buttons &= !(1 << i);
        }
        self.seen = true;
    }

    // Stick deflection, −1..1 per axis, y down-positive (Android AXIS_Y, like DirectInput lY).
    pub fn stick(&mut self, x: f32, y: f32) {
        self.stick = [axis_value(x), axis_value(y)];
        self.seen |= self.stick.iter().any(|v| v.abs() > IN_USE);
        self.mix();
    }

    // D-pad sent as AXIS_HAT_X / AXIS_HAT_Y, −1..1 per axis, y down-positive; centred releases it.
    pub fn hat(&mut self, x: f32, y: f32) {
        let dir = |v: f32| {
            if v > HAT_DEAD {
                1
            } else if v < -HAT_DEAD {
                -1
            } else {
                0
            }
        };
        self.hat = [dir(x), dir(y)];
        self.seen |= self.hat != [0, 0];
        self.mix();
    }

    fn mix(&mut self) {
        for i in 0..2 {
            self.state.axis[i] = if self.hat[i] == 0 {
                self.stick[i]
            } else {
                self.hat[i] * 1000
            };
        }
    }

    // A device went away: everything held is released.
    pub fn release(&mut self) {
        self.state = PadState::default();
        self.stick = [0, 0];
        self.hat = [0, 0];
    }
}

// Splits winit touch events on Android: joystick motion arrives as `Touch` moves of a device with no pointer down (a
// screen always sends a press first; a stick never does), with the raw AXIS_X / AXIS_Y as the location.
#[derive(Clone, Debug)]
pub struct StickFilter<D> {
    down: std::collections::HashMap<D, u32>,
}

impl<D> Default for StickFilter<D> {
    fn default() -> Self {
        Self {
            down: std::collections::HashMap::new(),
        }
    }
}

impl<D: Eq + std::hash::Hash + Copy> StickFilter<D> {
    // Some((x, y)) when the event is stick motion, None for a touch.
    pub fn classify(
        &mut self,
        dev: D,
        phase: winit::event::TouchPhase,
        x: f32,
        y: f32,
    ) -> Option<(f32, f32)> {
        use winit::event::TouchPhase::*;
        let n = self.down.entry(dev).or_insert(0);
        match phase {
            Started => *n += 1,
            Ended | Cancelled => *n = n.saturating_sub(1),
            Moved if *n == 0 => return Some((x, y)),
            Moved => {}
        }
        None
    }
}

#[derive(Default)]
pub struct Joystick {
    #[cfg(any(not(target_os = "linux"), feature = "gamepad-linux"))]
    gilrs: Option<gilrs::Gilrs>,
    pub events: EventPad,
    // Select pressed on a gilrs pad since the last `take_menu`.
    menu: bool,
    // Pad input since the last `take_used`.
    used: bool,
}

impl Joystick {
    pub fn new() -> Self {
        Self {
            #[cfg(any(not(target_os = "linux"), feature = "gamepad-linux"))]
            // gilrs has no Android backend; pads there arrive as window events.
            gilrs: (!cfg!(target_os = "android"))
                .then(|| gilrs::Gilrs::new().map_err(|e| log::warn!("joystick: {e}")).ok())
                .flatten(),
            ..Default::default()
        }
    }

    // Drains the gilrs events: hot-plug, the menu button edge, pad use.
    pub fn pump(&mut self) {
        #[cfg(any(not(target_os = "linux"), feature = "gamepad-linux"))]
        if let Some(g) = &mut self.gilrs {
            while let Some(ev) = g.next_event() {
                match ev.event {
                    gilrs::EventType::Connected => {
                        log::info!("gamepad connected: {}", g.gamepad(ev.id).name());
                    }
                    gilrs::EventType::Disconnected => {
                        log::info!("gamepad disconnected: {}", g.gamepad(ev.id).name());
                    }
                    gilrs::EventType::ButtonPressed(b, _) => {
                        self.used = true;
                        if b == BUTTONS[MENU_BUTTON] {
                            self.menu = true;
                        }
                    }
                    gilrs::EventType::AxisChanged(_, v, _) if axis_value(v).abs() > IN_USE => {
                        self.used = true;
                    }
                    _ => {}
                }
            }
        }
        if std::mem::take(&mut self.events.seen) {
            self.used = true;
        }
    }

    // A gilrs Select press since the last call (Android Select arrives as a window key event instead).
    pub fn take_menu(&mut self) -> bool {
        std::mem::take(&mut self.menu)
    }

    // Pad input since the last call.
    pub fn take_used(&mut self) -> bool {
        self.pump();
        std::mem::take(&mut self.used)
    }

    // Whether the pump needs a periodic call while the game does not poll (port menu): gilrs has no wake-up.
    pub fn needs_pump(&self) -> bool {
        #[cfg(any(not(target_os = "linux"), feature = "gamepad-linux"))]
        return self
            .gilrs
            .as_ref()
            .is_some_and(|g| g.gamepads().next().is_some());
        #[cfg(not(any(not(target_os = "linux"), feature = "gamepad-linux")))]
        false
    }

    // Name of the active device: the first connected gilrs gamepad, else the first event-fed one.
    pub fn active_name(&self) -> Option<String> {
        #[cfg(any(not(target_os = "linux"), feature = "gamepad-linux"))]
        if let Some((_, pad)) = self.gilrs.as_ref().and_then(|g| g.gamepads().next()) {
            return Some(pad.name().to_string());
        }
        self.events.devices.first().cloned()
    }

    // One poll: buttons and lX/lY of the first connected gilrs gamepad, else the event-fed pad.
    pub fn poll(&mut self) -> PadState {
        self.pump();
        #[cfg(any(not(target_os = "linux"), feature = "gamepad-linux"))]
        if let Some(g) = &mut self.gilrs
            && let Some((_, pad)) = g.gamepads().next()
        {
            let buttons = BUTTONS
                .iter()
                .enumerate()
                .filter(|(_, b)| pad.is_pressed(**b))
                .fold(0u32, |m, (i, _)| m | 1 << i);
            // gilrs Y is up-positive, DirectInput lY down-positive.
            let axis = [
                axis_value(pad.value(gilrs::Axis::LeftStickX)),
                axis_value(-pad.value(gilrs::Axis::LeftStickY)),
            ];
            return PadState { buttons, axis };
        }
        self.events.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::event::TouchPhase::*;

    // Android keycodes take the gilrs button order, so the game's joystick config means the same buttons.
    #[test]
    fn android_mapping() {
        assert_eq!(android_button(96), Some(0));
        assert_eq!(android_button(97), Some(1));
        assert_eq!(android_button(99), Some(2));
        assert_eq!(android_button(100), Some(3));
        assert_eq!(android_button(102), Some(4));
        assert_eq!(android_button(109), Some(MENU_BUTTON));
        assert_eq!(android_button(108), Some(9));
        assert_eq!(android_button(22), Some(15));
        assert_eq!(android_button(98), None);
        let mut p = EventPad::default();
        p.button(0, true);
        p.button(4, true);
        p.button(0, false);
        p.stick(1.5, -0.7);
        assert_eq!((p.state.buttons, p.state.axis), (1 << 4, [1000, -700]));
        p.release();
        assert_eq!((p.state.buttons, p.state.axis), (0, [0, 0]));
    }

    // The default thresholds (600) see a full deflection as Right / Up.
    #[test]
    fn stick_through_config() {
        let cfg = th08_core::config::Config::default();
        let mut p = EventPad::default();
        p.stick(1.0, -1.0);
        p.button(0, true);
        let mut c = 0;
        let bits = th08_core::input::joystick_bits(&cfg, &p.state, &mut c);
        assert_eq!(
            bits & (th08_core::input::RIGHT | th08_core::input::UP | th08_core::input::SHOT),
            th08_core::input::RIGHT | th08_core::input::UP | th08_core::input::SHOT
        );
    }

    // Moves with nothing pressed on that device are stick motion; presses and the moves between them are touches
    // (adb injects both under one device id).
    #[test]
    fn stick_filter() {
        let mut f = StickFilter::default();
        assert_eq!(f.classify(1u32, Started, 100.0, 50.0), None);
        assert_eq!(f.classify(1, Moved, 110.0, 50.0), None);
        assert_eq!(f.classify(1, Ended, 110.0, 50.0), None);
        assert_eq!(f.classify(2, Moved, 0.5, -1.0), Some((0.5, -1.0)));
        assert_eq!(f.classify(2, Moved, 0.0, 0.0), Some((0.0, 0.0)));
        assert_eq!(f.classify(2, Started, 300.0, 200.0), None);
        assert_eq!(f.classify(2, Moved, 310.0, 200.0), None);
        assert_eq!(f.classify(2, Ended, 310.0, 200.0), None);
        assert_eq!(f.classify(2, Moved, -1.0, 0.0), Some((-1.0, 0.0)));
    }

    fn dirs(p: &EventPad) -> u16 {
        use th08_core::input::{DOWN, LEFT, RIGHT, UP};
        let cfg = th08_core::config::Config::default();
        th08_core::input::joystick_bits(&cfg, &p.state, &mut 0) & (UP | DOWN | LEFT | RIGHT)
    }

    // Hat values past the dead zone are full d-pad presses, y down-positive; inside it nothing is pressed.
    #[test]
    fn hat_bits() {
        use th08_core::input::{DOWN, LEFT, RIGHT, UP};
        let mut p = EventPad::default();
        for (x, y, bits) in [
            (0.0, -1.0, UP),
            (0.0, 1.0, DOWN),
            (-1.0, 0.0, LEFT),
            (1.0, 0.0, RIGHT),
            (1.0, -1.0, RIGHT | UP),
            (-1.0, 1.0, LEFT | DOWN),
            (0.4, -0.4, 0),
            (0.0, 0.0, 0),
        ] {
            p.hat(x, y);
            assert_eq!(dirs(&p), bits, "hat ({x}, {y})");
        }
    }

    // A centred hat releases its direction; the stick shows again, a device going away clears the hat.
    #[test]
    fn hat_release() {
        use th08_core::input::{LEFT, UP};
        let mut p = EventPad::default();
        p.hat(0.0, -1.0);
        assert_eq!(dirs(&p), UP);
        p.hat(0.0, 0.0);
        assert_eq!((dirs(&p), p.state.axis), (0, [0, 0]));
        p.stick(-1.0, 0.1);
        p.hat(0.0, -1.0);
        assert_eq!(dirs(&p), LEFT | UP);
        p.hat(0.0, 0.0);
        assert_eq!((dirs(&p), p.state.axis), (LEFT, [-1000, 100]));
        p.release();
        p.stick(0.0, 0.0);
        assert_eq!((dirs(&p), p.state.axis), (0, [0, 0]));
    }

    // Hat, stick and buttons (d-pad keys arrive as arrow keys) together: the hat wins on its axis, buttons stay.
    #[test]
    fn hat_with_stick_and_buttons() {
        use th08_core::input::{DOWN, RIGHT, SHOT};
        let mut p = EventPad::default();
        p.button(0, true);
        p.stick(1.0, -1.0);
        p.hat(0.0, 1.0);
        assert_eq!(p.state.axis, [1000, 1000]);
        assert_eq!(dirs(&p), RIGHT | DOWN);
        let cfg = th08_core::config::Config::default();
        assert_ne!(
            th08_core::input::joystick_bits(&cfg, &p.state, &mut 0) & SHOT,
            0
        );
        p.stick(0.2, 0.0);
        assert_eq!(dirs(&p), DOWN);
        let mut j = Joystick::default();
        j.events.hat(1.0, 0.0);
        assert!(j.take_used());
        assert_eq!(j.poll().axis, [1000, 0]);
    }

    // The event pad's input marks the pad as used once.
    #[test]
    fn used_flag() {
        let mut j = Joystick::default();
        assert!(!j.take_used());
        j.events.button(1, true);
        assert!(j.take_used());
        assert!(!j.take_used());
    }
}
