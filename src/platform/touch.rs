//! Touch overlay input (ARCH_D_MOBILE §3.1–§3.3): fingers on the overlay controls → the same input word bits as the
//! keyboard (15 §1.1). Port-only; core never sees touches, a replay records only the word.

use th08_core::input::*;
use winit::event::TouchPhase;

// Finger id of the desktop mouse when `touch_mouse` is on.
pub const MOUSE_ID: u64 = u64::MAX;

// Android dp: 160 per inch; winit's scale factor is px per dp there (1.0 on a plain desktop).
const DP_PER_MM: f32 = 160.0 / 25.4;
// Menu swipe length per direction pulse, and the most a tap may move (mm).
const SWIPE_MM: f32 = 6.0;
const TAP_MM: f32 = 3.0;
// Stick: dead zone as a fraction of its radius.
const STICK_DEAD: f32 = 0.3;
const SCREEN: [f32; 2] = [640.0, 480.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ctl {
    // Movement zone (relative drag or floating stick).
    Move,
    Shot,
    Bomb,
    Focus,
    Pause,
    Skip,
    // Auto-fire on/off.
    Auto,
}

impl Ctl {
    pub const ALL: [Ctl; 7] = [
        Ctl::Move,
        Ctl::Shot,
        Ctl::Bomb,
        Ctl::Focus,
        Ctl::Pause,
        Ctl::Skip,
        Ctl::Auto,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Ctl::Move => "move",
            Ctl::Shot => "shot",
            Ctl::Bomb => "bomb",
            Ctl::Focus => "focus",
            Ctl::Pause => "pause",
            Ctl::Skip => "skip",
            Ctl::Auto => "auto",
        }
    }

    // Word bit of a held button (Focus in hold mode; Auto has none).
    fn bit(self) -> u16 {
        match self {
            Ctl::Shot => SHOT,
            Ctl::Bomb => BOMB,
            Ctl::Focus => FOCUS,
            Ctl::Pause => PAUSE,
            Ctl::Skip => SKIP,
            Ctl::Move | Ctl::Auto => 0,
        }
    }
}

// One overlay control. `x`, `y`: centre as a fraction of the window. `w`, `h`: the Move zone's size as a fraction of
// the window; buttons are circles of diameter `w` mm (`h` unused, kept equal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Control {
    pub ctl: Ctl,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub opacity: f32,
    pub visible: bool,
}

const fn c(ctl: Ctl, x: f32, y: f32, w: f32, h: f32) -> Control {
    Control {
        ctl,
        x,
        y,
        w,
        h,
        opacity: 1.0,
        visible: true,
    }
}

// The overlay layout, one control list per orientation (the later layout editor edits these); port.cfg
// `touch_layout` holds `to_cfg()`, "" = the defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct TouchLayout {
    pub landscape: Vec<Control>,
    pub portrait: Vec<Control>,
}

impl Default for TouchLayout {
    // Laid out for a 20:9 phone: landscape = movement on the left part, buttons in the right letterbox bar;
    // portrait = both under the game picture.
    fn default() -> Self {
        Self {
            landscape: vec![
                c(Ctl::Move, 0.30, 0.50, 0.60, 1.00),
                c(Ctl::Shot, 0.92, 0.78, 15.0, 15.0),
                c(Ctl::Bomb, 0.94, 0.53, 11.0, 11.0),
                c(Ctl::Focus, 0.84, 0.89, 12.0, 12.0),
                c(Ctl::Auto, 0.86, 0.60, 8.0, 8.0),
                c(Ctl::Skip, 0.875, 0.09, 8.0, 8.0),
                c(Ctl::Pause, 0.95, 0.09, 8.0, 8.0),
            ],
            portrait: vec![
                c(Ctl::Move, 0.275, 0.835, 0.55, 0.33),
                c(Ctl::Shot, 0.83, 0.86, 15.0, 15.0),
                c(Ctl::Bomb, 0.83, 0.74, 11.0, 11.0),
                c(Ctl::Focus, 0.64, 0.93, 12.0, 12.0),
                c(Ctl::Auto, 0.64, 0.78, 8.0, 8.0),
                c(Ctl::Skip, 0.75, 0.06, 8.0, 8.0),
                c(Ctl::Pause, 0.90, 0.06, 8.0, 8.0),
            ],
        }
    }
}

impl TouchLayout {
    pub fn controls(&self, landscape: bool) -> &[Control] {
        if landscape {
            &self.landscape
        } else {
            &self.portrait
        }
    }

    // `L:<control>;…|P:<control>;…`, control = `name,x,y,w,h,opacity,visible`.
    pub fn to_cfg(&self) -> String {
        let side = |v: &[Control]| {
            v.iter()
                .map(|c| {
                    format!(
                        "{},{},{},{},{},{},{}",
                        c.ctl.name(),
                        c.x,
                        c.y,
                        c.w,
                        c.h,
                        c.opacity,
                        u8::from(c.visible)
                    )
                })
                .collect::<Vec<_>>()
                .join(";")
        };
        format!("L:{}|P:{}", side(&self.landscape), side(&self.portrait))
    }

    pub fn parse(v: &str) -> Result<Self, String> {
        let v = v.trim();
        if v.is_empty() {
            return Ok(Self::default());
        }
        let mut out = Self {
            landscape: Vec::new(),
            portrait: Vec::new(),
        };
        for part in v.split('|') {
            let (o, list) = part
                .split_once(':')
                .ok_or("expected L:<controls>|P:<controls>")?;
            let dst = match o.trim() {
                "L" => &mut out.landscape,
                "P" => &mut out.portrait,
                _ => return Err(format!("{o}: expected L or P")),
            };
            for item in list.split(';').filter(|s| !s.trim().is_empty()) {
                dst.push(parse_control(item)?);
            }
        }
        Ok(out)
    }
}

fn parse_control(item: &str) -> Result<Control, String> {
    let f: Vec<&str> = item.split(',').map(str::trim).collect();
    if f.len() != 7 {
        return Err(format!("{item}: expected name,x,y,w,h,opacity,visible"));
    }
    let ctl = Ctl::ALL
        .into_iter()
        .find(|c| c.name() == f[0])
        .ok_or_else(|| format!("{}: unknown control", f[0]))?;
    let n = |s: &str| {
        s.parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("{s}: expected a number"))
    };
    Ok(Control {
        ctl,
        x: n(f[1])?,
        y: n(f[2])?,
        w: n(f[3])?,
        h: n(f[4])?,
        opacity: n(f[5])?.clamp(0.0, 1.0),
        visible: crate::settings::parse_bool(f[6])?,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TouchMode {
    // On touch platforms (Android).
    #[default]
    Auto,
    On,
    Off,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Movement {
    // Relative drag (ARCH_D §3.2, recommended): the player follows the finger's movement.
    #[default]
    Drag,
    // Floating 8-way stick centred where the finger went down.
    Stick,
}

// Port settings of the overlay (settings.rs `touch*` rows).
#[derive(Clone, Debug, PartialEq)]
pub struct TouchConfig {
    pub mode: TouchMode,
    // Dev: the desktop mouse acts as one finger (and shows the overlay).
    pub mouse: bool,
    pub movement: Movement,
    // Drag: game pixels per window pixel scaled to the game picture (1.0 = the player follows the finger 1:1).
    pub sens: f32,
    // FOCUS button toggles instead of being held.
    pub focus_toggle: bool,
    // Auto-fire on at start (the Auto button toggles it).
    pub autofire: bool,
    // Overlay opacity 0–1, times each control's own.
    pub opacity: f32,
    pub layout: TouchLayout,
}

impl Default for TouchConfig {
    fn default() -> Self {
        Self {
            mode: TouchMode::Auto,
            mouse: false,
            movement: Movement::Drag,
            sens: 1.0,
            focus_toggle: false,
            autofire: true,
            opacity: 0.5,
            layout: TouchLayout::default(),
        }
    }
}

impl TouchConfig {
    // Whether the overlay is drawn and touches are read.
    pub fn active(&self) -> bool {
        self.mouse
            || match self.mode {
                TouchMode::On => true,
                TouchMode::Off => false,
                TouchMode::Auto => cfg!(target_os = "android"),
            }
    }
}

// What the game is doing, set by the app before each logic frame (a read of the World between frames; only the
// resulting word is recorded, so determinism is unaffected, ARCH_D §3.2).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TouchScene {
    // Menus, pause, game over: directions as pulses, a tap = SHOT, no auto-fire (§3.3).
    #[default]
    Menu,
    Play {
        // Player position (09 §1 +pos), game pixels.
        pos: [f32; 2],
        // Ortho speed per frame unfocused / focused (09 §2.1 sht +0x24 / youkai +0x28) and the x / y multipliers.
        speed: [f32; 2],
        mult: [f32; 2],
        // Auto-fire allowed: not during dialogue (§3.3).
        fire: bool,
    },
}

impl TouchScene {
    pub fn of(w: &th08_core::world::World) -> Self {
        use th08_core::gamemgr::SCENE_GAME;
        let g = &w.game;
        let Some(p) = w.player.as_deref() else {
            return Self::Menu;
        };
        if g.scene != SCENE_GAME || g.paused != 0 || g.game_over != 0 {
            return Self::Menu;
        }
        Self::Play {
            pos: [p.pos[0], p.pos[1]],
            speed: [p.sht.speed, p.sht_youkai.speed_focus],
            mult: p.speed_mult,
            fire: !w.msg.as_deref().is_some_and(|m| m.active()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum On {
    Button(Ctl),
    Move,
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    id: u64,
    on: On,
    start: [f32; 2],
    last: [f32; 2],
    // Position already turned into target / swipe movement.
    seen: [f32; 2],
    // Stick centre (floating: dragged along past the rim).
    centre: [f32; 2],
    // Farthest distance from `start` (tap detection).
    travel: f32,
}

// A placed control in window pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub ctl: Ctl,
    pub centre: [f32; 2],
    // Buttons: radius; Move: half size.
    pub half: [f32; 2],
    pub opacity: f32,
}

impl Placed {
    fn hit(&self, p: [f32; 2]) -> bool {
        let d = [p[0] - self.centre[0], p[1] - self.centre[1]];
        match self.ctl {
            Ctl::Move => d[0].abs() <= self.half[0] && d[1].abs() <= self.half[1],
            // A little larger than drawn: fingers are imprecise.
            _ => len(d[0], d[1]) <= self.half[0] * 1.2,
        }
    }
}

// Overlay draw state (§3.5): placed controls, which are lit, and the movement finger's marks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TouchView {
    pub controls: Vec<(Placed, bool)>,
    pub autofire: bool,
    pub opacity: f32,
    // Movement finger: (stick centre or drag start, finger), and the stick radius (0 = drag).
    pub finger: Option<([f32; 2], [f32; 2], f32)>,
}

// The touch device (ARCH_D §3.2–§3.3): fingers on controls, polled once per Input_Poll like the keyboard.
#[derive(Clone, Debug, Default)]
pub struct TouchSource {
    pub cfg: TouchConfig,
    // Window size (px) and px per mm.
    size: [f32; 2],
    ppm: f32,
    fingers: Vec<Finger>,
    autofire: bool,
    focus_on: bool,
    scene: TouchScene,
    // Drag target (game px) and the previous poll's request / position (wall re-anchoring).
    target: Option<[f32; 2]>,
    last_req: u16,
    last_pos: [f32; 2],
    // Menu swipe accumulator (px), queued direction / tap pulses and the up poll owed after a pulse.
    acc: [f32; 2],
    pulses: std::collections::VecDeque<u16>,
    gap: bool,
    last_word: u16,
    mouse_pos: [f32; 2],
    mouse_down: bool,
    // As `Keyboard::failed`: focus loss releases every finger.
    failed: bool,
}

impl TouchSource {
    pub fn new(cfg: TouchConfig) -> Self {
        Self {
            autofire: cfg.autofire,
            cfg,
            size: SCREEN,
            ppm: DP_PER_MM,
            ..Default::default()
        }
    }

    // Window size in px and winit's scale factor.
    pub fn set_window(&mut self, w: u32, h: u32, scale_factor: f64) {
        self.size = [w.max(1) as f32, h.max(1) as f32];
        self.ppm = DP_PER_MM * scale_factor as f32;
    }

    pub fn set_scene(&mut self, s: TouchScene) {
        if matches!(s, TouchScene::Menu) != matches!(self.scene, TouchScene::Menu) {
            self.target = None;
            self.acc = [0.0; 2];
            self.pulses.clear();
            for f in &mut self.fingers {
                f.seen = f.last;
            }
        }
        self.scene = s;
    }

    pub fn set_focus(&mut self, focused: bool) {
        self.failed = !focused;
        if !focused {
            self.fingers.clear();
            self.pulses.clear();
            self.gap = false;
            self.target = None;
            self.acc = [0.0; 2];
            self.mouse_down = false;
        }
    }

    fn landscape(&self) -> bool {
        self.size[0] >= self.size[1]
    }

    pub fn placed(&self) -> Vec<Placed> {
        let [w, h] = self.size;
        self.cfg
            .layout
            .controls(self.landscape())
            .iter()
            .filter(|c| c.visible)
            .map(|c| Placed {
                ctl: c.ctl,
                centre: [c.x * w, c.y * h],
                half: match c.ctl {
                    Ctl::Move => [c.w * w / 2.0, c.h * h / 2.0],
                    _ => [c.w * self.ppm / 2.0; 2],
                },
                opacity: c.opacity,
            })
            .collect()
    }

    // A winit `Touch` (window px).
    pub fn touch(&mut self, id: u64, phase: TouchPhase, x: f32, y: f32) {
        let p = [x, y];
        match phase {
            TouchPhase::Started => self.down(id, p),
            TouchPhase::Moved => {
                if let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) {
                    f.last = p;
                    f.travel = f.travel.max(len(p[0] - f.start[0], p[1] - f.start[1]));
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => self.up(id, phase == TouchPhase::Ended),
        }
    }

    // Dev `touch_mouse`: the left button is a finger.
    pub fn mouse_move(&mut self, x: f32, y: f32) {
        self.mouse_pos = [x, y];
        if self.mouse_down {
            self.touch(MOUSE_ID, TouchPhase::Moved, x, y);
        }
    }

    pub fn mouse_button(&mut self, down: bool) {
        if down == self.mouse_down {
            return;
        }
        self.mouse_down = down;
        let [x, y] = self.mouse_pos;
        let phase = if down {
            TouchPhase::Started
        } else {
            TouchPhase::Ended
        };
        self.touch(MOUSE_ID, phase, x, y);
    }

    fn down(&mut self, id: u64, p: [f32; 2]) {
        if self.failed || self.fingers.iter().any(|f| f.id == id) {
            return;
        }
        let placed = self.placed();
        let on = if let Some(b) = placed.iter().find(|c| c.ctl != Ctl::Move && c.hit(p)) {
            On::Button(b.ctl)
        } else if placed.iter().any(|c| c.ctl == Ctl::Move && c.hit(p))
            && !self.fingers.iter().any(|f| f.on == On::Move)
        {
            On::Move
        } else {
            return;
        };
        match on {
            On::Button(Ctl::Auto) => self.autofire = !self.autofire,
            On::Button(Ctl::Focus) if self.cfg.focus_toggle => self.focus_on = !self.focus_on,
            On::Move => {
                self.target = None;
                self.acc = [0.0; 2];
            }
            _ => {}
        }
        self.fingers.push(Finger {
            id,
            on,
            start: p,
            last: p,
            seen: p,
            centre: p,
            travel: 0.0,
        });
    }

    fn up(&mut self, id: u64, ended: bool) {
        let Some(i) = self.fingers.iter().position(|f| f.id == id) else {
            return;
        };
        let f = self.fingers.remove(i);
        if f.on == On::Move {
            self.target = None;
            // Menu tap = confirm (§3.3).
            if ended && self.scene == TouchScene::Menu && f.travel < TAP_MM * self.ppm {
                self.pulse(SHOT);
            }
        }
    }

    fn pulse(&mut self, bit: u16) {
        if self.pulses.len() < 4 {
            self.pulses.push_back(bit);
        }
    }

    // One Input_Poll: the word of this device.
    pub fn poll(&mut self) -> u16 {
        if self.failed {
            self.last_word = 0;
            return 0;
        }
        let toggle = self.cfg.focus_toggle;
        let mut w = self
            .fingers
            .iter()
            .filter_map(|f| match f.on {
                On::Button(Ctl::Focus) if toggle => None,
                On::Button(c) => Some(c.bit()),
                On::Move => None,
            })
            .fold(0, |w, b| w | b);
        match self.scene {
            TouchScene::Play {
                pos,
                speed,
                mult,
                fire,
            } => {
                if fire && self.autofire {
                    w |= SHOT;
                }
                if toggle && self.focus_on {
                    w |= FOCUS;
                }
                let v = if w & FOCUS != 0 { speed[1] } else { speed[0] };
                w |= self.steer(pos, [v * mult[0], v * mult[1]]);
            }
            TouchScene::Menu => w |= self.menu(),
        }
        self.last_word = w;
        w
    }

    fn move_finger(&mut self) -> Option<&mut Finger> {
        self.fingers.iter_mut().find(|f| f.on == On::Move)
    }

    // Window px → game px (the blit's letterbox scale).
    fn game_scale(&self) -> f32 {
        (self.size[0] / SCREEN[0]).min(self.size[1] / SCREEN[1])
    }

    fn steer(&mut self, pos: [f32; 2], step: [f32; 2]) -> u16 {
        let k = self.cfg.sens / self.game_scale();
        let movement = self.cfg.movement;
        let r = self.stick_radius();
        let (last_req, last_pos) = (self.last_req, self.last_pos);
        let had_target = self.target.is_some();
        let Some(f) = self.move_finger() else {
            self.target = None;
            self.last_req = 0;
            return 0;
        };
        let bits = match movement {
            Movement::Stick => {
                let mut d = [f.last[0] - f.centre[0], f.last[1] - f.centre[1]];
                let n = len(d[0], d[1]);
                if n > r {
                    let s = (n - r) / n;
                    f.centre = [f.centre[0] + d[0] * s, f.centre[1] + d[1] * s];
                    d = [d[0] * r / n, d[1] * r / n];
                }
                eight_way(d, r * STICK_DEAD)
            }
            Movement::Drag => {
                let delta = [(f.last[0] - f.seen[0]) * k, (f.last[1] - f.seen[1]) * k];
                f.seen = f.last;
                let mut t = if had_target {
                    self.target.unwrap()
                } else {
                    pos
                };
                // Blocked at a wall (or not movable): the target restarts at the player on that axis.
                if had_target {
                    if last_req & (LEFT | RIGHT) != 0 && pos[0] == last_pos[0] {
                        t[0] = pos[0];
                    }
                    if last_req & (UP | DOWN) != 0 && pos[1] == last_pos[1] {
                        t[1] = pos[1];
                    }
                }
                t = [t[0] + delta[0], t[1] + delta[1]];
                self.target = Some(t);
                // Dead zone of one frame's step per axis: never overshoots, so it cannot oscillate.
                let axis = |d: f32, s: f32, neg: u16, posb: u16| {
                    let s = s.abs().max(0.25);
                    if d >= s {
                        posb
                    } else if d <= -s {
                        neg
                    } else {
                        0
                    }
                };
                axis(t[0] - pos[0], step[0], LEFT, RIGHT) | axis(t[1] - pos[1], step[1], UP, DOWN)
            }
        };
        self.last_req = bits;
        self.last_pos = pos;
        bits
    }

    fn stick_radius(&self) -> f32 {
        12.0 * self.ppm
    }

    // Menus: a drag swipes one direction pulse per `SWIPE_MM`; a stick holds the direction (the menus repeat
    // held keys themselves). A pulse is one poll down, then one poll up, so the edge detection sees each (§3.3).
    fn menu(&mut self) -> u16 {
        let swipe = SWIPE_MM * self.ppm;
        let movement = self.cfg.movement;
        let r = self.stick_radius();
        let mut held = 0;
        if let Some(f) = self.move_finger() {
            match movement {
                Movement::Stick => {
                    held = eight_way(
                        [f.last[0] - f.centre[0], f.last[1] - f.centre[1]],
                        r * STICK_DEAD,
                    )
                }
                Movement::Drag => {
                    let d = [f.last[0] - f.seen[0], f.last[1] - f.seen[1]];
                    f.seen = f.last;
                    self.acc = [self.acc[0] + d[0], self.acc[1] + d[1]];
                    loop {
                        let [ax, ay] = self.acc;
                        let bit = if ax.abs() >= swipe && ax.abs() >= ay.abs() {
                            self.acc[0] -= swipe * ax.signum();
                            if ax > 0.0 { RIGHT } else { LEFT }
                        } else if ay.abs() >= swipe {
                            self.acc[1] -= swipe * ay.signum();
                            if ay > 0.0 { DOWN } else { UP }
                        } else {
                            break;
                        };
                        self.pulse(bit);
                    }
                }
            }
        }
        if std::mem::take(&mut self.gap) {
            return held;
        }
        match self.pulses.pop_front() {
            Some(b) => {
                self.gap = true;
                held | b
            }
            None => held,
        }
    }

    // Overlay state for the frame being drawn.
    pub fn view(&self) -> TouchView {
        let controls =
            self.placed()
                .into_iter()
                .map(|p| {
                    let lit = self.fingers.iter().any(|f| {
                        f.on == On::Button(p.ctl) || (p.ctl == Ctl::Move && f.on == On::Move)
                    }) || (p.ctl.bit() & self.last_word != 0)
                        || (p.ctl == Ctl::Auto && self.autofire);
                    (p, lit)
                })
                .collect();
        let finger =
            self.fingers
                .iter()
                .find(|f| f.on == On::Move)
                .map(|f| match self.cfg.movement {
                    Movement::Stick => (f.centre, f.last, self.stick_radius()),
                    Movement::Drag => (f.start, f.last, 0.0),
                });
        TouchView {
            controls,
            autofire: self.autofire,
            opacity: self.cfg.opacity,
            finger,
        }
    }
}

// Length of (x, y); sqrt is exact in IEEE, unlike `hypot`.
fn len(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

// 8-way direction of `d` (window px, y down), none inside `dead`: 45° sectors.
fn eight_way(d: [f32; 2], dead: f32) -> u16 {
    if len(d[0], d[1]) < dead {
        return 0;
    }
    let a = th08_core::fmath::atan2_f(d[1], d[0]).to_degrees();
    let sector = (((a + 360.0 + 22.5) % 360.0) / 45.0) as u32;
    [
        RIGHT,
        DOWN | RIGHT,
        DOWN,
        DOWN | LEFT,
        LEFT,
        UP | LEFT,
        UP,
        UP | RIGHT,
    ][sector as usize % 8]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::InputMux;
    use winit::keyboard::KeyCode;

    const PLAY_SPEED: f32 = 4.0;

    // A 640×480 window at scale 1 (1 window px = 1 game px), default layout, auto-fire off unless asked.
    fn src(cfg: TouchConfig) -> TouchSource {
        let mut t = TouchSource::new(TouchConfig {
            autofire: false,
            ..cfg
        });
        t.set_window(640, 480, 1.0);
        t
    }

    fn play(pos: [f32; 2]) -> TouchScene {
        TouchScene::Play {
            pos,
            speed: [PLAY_SPEED, 2.0],
            mult: [1.0; 2],
            fire: true,
        }
    }

    fn centre(t: &TouchSource, ctl: Ctl) -> [f32; 2] {
        t.placed().iter().find(|p| p.ctl == ctl).unwrap().centre
    }

    // Moves a simulated player by one step per poll along the word's direction bits.
    fn advance(pos: &mut [f32; 2], w: u16) {
        if w & LEFT != 0 {
            pos[0] -= PLAY_SPEED;
        }
        if w & RIGHT != 0 {
            pos[0] += PLAY_SPEED;
        }
        if w & UP != 0 {
            pos[1] -= PLAY_SPEED;
        }
        if w & DOWN != 0 {
            pos[1] += PLAY_SPEED;
        }
    }

    #[test]
    fn layout_cfg_round_trip() {
        let l = TouchLayout::default();
        assert_eq!(TouchLayout::parse(&l.to_cfg()).unwrap(), l);
        assert_eq!(TouchLayout::parse("").unwrap(), l);
        let mut m = l.clone();
        m.portrait[1].visible = false;
        m.landscape[2].opacity = 0.25;
        assert_eq!(TouchLayout::parse(&m.to_cfg()).unwrap(), m);
        assert!(TouchLayout::parse("L:laser,0,0,1,1,1,1").is_err());
        assert!(TouchLayout::parse("X:").is_err());
    }

    // Relative drag: the player follows the finger's movement and stops within one step, no oscillation.
    #[test]
    fn drag_follows_finger() {
        let mut t = src(TouchConfig::default());
        let mut pos = [192.0, 400.0];
        t.set_scene(play(pos));
        t.touch(1, TouchPhase::Started, 100.0, 300.0);
        assert_eq!(t.poll(), 0);
        t.touch(1, TouchPhase::Moved, 140.0, 280.0);
        let mut words = Vec::new();
        for _ in 0..16 {
            t.set_scene(play(pos));
            let w = t.poll();
            advance(&mut pos, w);
            words.push(w);
        }
        assert_eq!(pos, [232.0, 380.0]);
        assert_eq!(words[0], UP | RIGHT);
        assert!(words[10..].iter().all(|&w| w == 0));
        t.touch(1, TouchPhase::Ended, 140.0, 280.0);
        assert_eq!(t.poll(), 0);
    }

    // Sensitivity 2 doubles the distance; a wall re-anchors the target so reversing acts at once.
    #[test]
    fn drag_sensitivity_and_wall() {
        let mut t = src(TouchConfig {
            sens: 2.0,
            ..Default::default()
        });
        let mut pos = [100.0, 300.0];
        t.set_scene(play(pos));
        t.touch(1, TouchPhase::Started, 100.0, 300.0);
        t.touch(1, TouchPhase::Moved, 110.0, 300.0);
        for _ in 0..10 {
            t.set_scene(play(pos));
            let w = t.poll();
            advance(&mut pos, w);
        }
        assert_eq!(pos, [120.0, 300.0]);
        // Wall at x = 120: the finger goes on right, the player stays; reversing moves left right away.
        t.touch(1, TouchPhase::Moved, 200.0, 300.0);
        t.set_scene(play(pos));
        assert_eq!(t.poll(), RIGHT);
        for _ in 0..4 {
            t.set_scene(play(pos));
            assert_eq!(t.poll(), 0);
        }
        t.touch(1, TouchPhase::Moved, 196.0, 300.0);
        t.set_scene(play(pos));
        assert_eq!(t.poll(), LEFT);
    }

    #[test]
    fn stick_eight_way() {
        let mut t = src(TouchConfig {
            movement: Movement::Stick,
            ..Default::default()
        });
        t.set_scene(play([0.0; 2]));
        let r = t.stick_radius();
        t.touch(3, TouchPhase::Started, 150.0, 240.0);
        assert_eq!(t.poll(), 0);
        t.touch(3, TouchPhase::Moved, 150.0 + r * 0.2, 240.0);
        assert_eq!(t.poll(), 0, "dead zone");
        t.touch(3, TouchPhase::Moved, 150.0 + r, 240.0 - r);
        assert_eq!(t.poll(), UP | RIGHT);
        t.touch(3, TouchPhase::Moved, 150.0 - r, 240.0 + r * 0.1);
        assert_eq!(t.poll(), LEFT);
        // Floating: past the rim the centre follows, so a small move back reverses.
        t.touch(3, TouchPhase::Moved, 150.0 - 4.0 * r, 240.0);
        assert_eq!(t.poll(), LEFT);
        t.touch(3, TouchPhase::Moved, 150.0 - 2.0 * r, 240.0);
        assert_eq!(t.poll(), RIGHT);
    }

    // Two fingers: movement + SHOT + BOMB together; each release clears only its own bits.
    #[test]
    fn multi_touch() {
        let mut t = src(TouchConfig::default());
        let mut pos = [192.0, 400.0];
        t.set_scene(play(pos));
        t.touch(1, TouchPhase::Started, 100.0, 300.0);
        let [sx, sy] = centre(&t, Ctl::Shot);
        let [bx, by] = centre(&t, Ctl::Bomb);
        t.touch(2, TouchPhase::Started, sx, sy);
        t.touch(5, TouchPhase::Started, bx, by);
        t.touch(1, TouchPhase::Moved, 80.0, 300.0);
        assert_eq!(t.poll(), SHOT | BOMB | LEFT);
        advance(&mut pos, LEFT);
        t.touch(5, TouchPhase::Ended, bx, by);
        t.set_scene(play(pos));
        assert_eq!(t.poll(), SHOT | LEFT);
        t.touch(2, TouchPhase::Ended, sx, sy);
        t.touch(1, TouchPhase::Cancelled, 80.0, 300.0);
        assert_eq!(t.poll(), 0);
        // A second finger in the movement zone is ignored while one owns it.
        t.touch(1, TouchPhase::Started, 100.0, 300.0);
        t.touch(7, TouchPhase::Started, 50.0, 100.0);
        t.touch(7, TouchPhase::Moved, 10.0, 100.0);
        assert_eq!(t.poll(), 0);
    }

    // Auto-fire: SHOT in play, not in menus or dialogue; the Auto button toggles it. Focus toggle mode.
    #[test]
    fn autofire_and_focus_toggle() {
        let mut t = src(TouchConfig {
            autofire: true,
            focus_toggle: true,
            ..Default::default()
        });
        t.autofire = true;
        t.set_scene(play([0.0; 2]));
        assert_eq!(t.poll(), SHOT);
        t.set_scene(TouchScene::Menu);
        assert_eq!(t.poll(), 0);
        t.set_scene(TouchScene::Play {
            pos: [0.0; 2],
            speed: [4.0, 2.0],
            mult: [1.0; 2],
            fire: false,
        });
        assert_eq!(t.poll(), 0, "dialogue");
        t.set_scene(play([0.0; 2]));
        let [ax, ay] = centre(&t, Ctl::Auto);
        t.touch(1, TouchPhase::Started, ax, ay);
        t.touch(1, TouchPhase::Ended, ax, ay);
        assert_eq!(t.poll(), 0);
        let [fx, fy] = centre(&t, Ctl::Focus);
        t.touch(2, TouchPhase::Started, fx, fy);
        t.touch(2, TouchPhase::Ended, fx, fy);
        assert_eq!(t.poll(), FOCUS);
        assert_eq!(t.poll(), FOCUS);
        t.set_scene(TouchScene::Menu);
        assert_eq!(t.poll(), 0);
    }

    // Focus loss releases every finger and drops queued pulses; later fingers work again.
    #[test]
    fn focus_loss_releases() {
        let mut t = src(TouchConfig::default());
        let [sx, sy] = centre(&t, Ctl::Shot);
        t.touch(1, TouchPhase::Started, sx, sy);
        t.touch(2, TouchPhase::Started, 100.0, 300.0);
        t.touch(2, TouchPhase::Moved, 100.0, 400.0);
        assert_eq!(t.poll(), SHOT | DOWN);
        t.set_focus(false);
        assert_eq!(t.poll(), 0);
        t.touch(1, TouchPhase::Started, sx, sy);
        assert_eq!(t.poll(), 0);
        t.set_focus(true);
        assert_eq!(t.poll(), 0);
        t.touch(1, TouchPhase::Ended, sx, sy);
        t.touch(1, TouchPhase::Started, sx, sy);
        assert_eq!(t.poll(), SHOT);
    }

    // Mouse-as-touch (dev): the left button is a finger.
    #[test]
    fn mouse_finger() {
        let mut t = src(TouchConfig {
            mouse: true,
            ..Default::default()
        });
        assert!(t.cfg.active());
        let [bx, by] = centre(&t, Ctl::Bomb);
        t.mouse_move(bx, by);
        assert_eq!(t.poll(), 0);
        t.mouse_button(true);
        assert_eq!(t.poll(), BOMB);
        t.mouse_button(false);
        assert_eq!(t.poll(), 0);
    }

    // Recorded-input check (ARCH_D §6.4): touch-made input gives the same word stream as the keyboard, in play
    // (drag + held SHOT) and in a menu (two swipes down + a tap = Down, Down, Z taps).
    #[test]
    fn same_words_as_keyboard() {
        let kb_script: [&[(KeyCode, bool)]; 14] = [
            &[(KeyCode::KeyZ, true), (KeyCode::ArrowLeft, true)],
            &[],
            &[],
            &[(KeyCode::ArrowLeft, false)],
            &[(KeyCode::KeyZ, false)],
            // Menu.
            &[(KeyCode::ArrowDown, true)],
            &[(KeyCode::ArrowDown, false)],
            &[(KeyCode::ArrowDown, true)],
            &[(KeyCode::ArrowDown, false)],
            &[(KeyCode::KeyZ, true)],
            &[(KeyCode::KeyZ, false)],
            &[],
            &[],
            &[],
        ];
        let mut kb = InputMux::default();
        let keyboard: Vec<u16> = kb_script
            .iter()
            .map(|ev| {
                for &(k, d) in *ev {
                    kb.keyboard.key(k, d);
                }
                kb.poll()[0]
            })
            .collect();

        let mut m = InputMux {
            touch: src(TouchConfig::default()),
            ..Default::default()
        };
        let t = &mut m.touch;
        let [sx, sy] = centre(t, Ctl::Shot);
        let mut pos = [192.0, 400.0];
        let swipe = SWIPE_MM * t.ppm;
        let mut touch = Vec::new();
        for i in 0..14 {
            let t = &mut m.touch;
            match i {
                0 => {
                    t.set_scene(play(pos));
                    t.touch(2, TouchPhase::Started, sx, sy);
                    t.touch(1, TouchPhase::Started, 100.0, 300.0);
                    t.touch(1, TouchPhase::Moved, 100.0 - 3.0 * PLAY_SPEED, 300.0);
                }
                4 => t.touch(2, TouchPhase::Ended, sx, sy),
                5 => {
                    t.touch(1, TouchPhase::Ended, 88.0, 300.0);
                    t.set_scene(TouchScene::Menu);
                    t.touch(1, TouchPhase::Started, 100.0, 200.0);
                    t.touch(1, TouchPhase::Moved, 100.0, 200.0 + 2.0 * swipe + 1.0);
                }
                9 => {
                    t.touch(1, TouchPhase::Ended, 100.0, 200.0 + 2.0 * swipe + 1.0);
                    t.touch(3, TouchPhase::Started, 100.0, 300.0);
                    t.touch(3, TouchPhase::Ended, 101.0, 300.0);
                }
                _ => {}
            }
            if i < 5 {
                m.touch.set_scene(play(pos));
            }
            let w = m.poll()[0];
            if i < 5 {
                advance(&mut pos, w);
            }
            touch.push(w);
        }
        assert_eq!(touch, keyboard);
        assert_eq!(pos, [180.0, 400.0]);
    }
}
