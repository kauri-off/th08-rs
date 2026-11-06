//! Touch overlay input: fingers on the overlay controls → the keyboard's input word bits; core never sees touches.

use th08_core::input::*;
use th08_render::gfx::SafeArea;
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
// Button diameter range of the layout editor (mm).
pub const BUTTON_MM: [f32; 2] = [6.0, 30.0];
// Gesture exclusion: the most Android honours per vertical edge, and how near an edge a control must reach (dp).
const EXCLUSION_DP: f32 = 200.0;
const EDGE_DP: f32 = 48.0;

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
    // Opens the port menu; never in the input word.
    Menu,
}

impl Ctl {
    pub const ALL: [Ctl; 8] = [
        Ctl::Move,
        Ctl::Shot,
        Ctl::Bomb,
        Ctl::Focus,
        Ctl::Pause,
        Ctl::Skip,
        Ctl::Auto,
        Ctl::Menu,
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
            Ctl::Menu => "menu",
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
            Ctl::Move | Ctl::Auto | Ctl::Menu => 0,
        }
    }

    // Controls that do nothing in menus (focus, auto-fire, skip) are hidden there.
    pub fn shown(self, s: Screen) -> bool {
        s != Screen::Menu || !matches!(self, Ctl::Focus | Ctl::Auto | Ctl::Skip)
    }

    // Order in which controls near an edge get the limited gesture exclusion height.
    fn edge_rank(self) -> u8 {
        match self {
            Ctl::Move => 0,
            Ctl::Shot => 1,
            Ctl::Bomb => 2,
            Ctl::Focus => 3,
            Ctl::Skip => 4,
            Ctl::Auto => 5,
            Ctl::Pause => 6,
            Ctl::Menu => 7,
        }
    }
}

// What the overlay labels and shows: menus (SHOT = confirm, BOMB = cancel), play, or dialogue (SHOT = next).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Menu,
    Play,
    Dialogue,
}

// One overlay control. `x`, `y`: centre as a fraction of the safe rect (the window minus the safe-area insets).
// `w`, `h`: the Move zone's size as a fraction of the safe rect; buttons are circles of diameter `w` mm (`h` unused, kept equal).
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

// The overlay layout, one control list per orientation (edited in the port menu); port.cfg `touch_layout` holds
// `to_cfg()`, "" = the defaults.
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
                c(Ctl::Shot, 0.935, 0.75, 15.0, 15.0),
                c(Ctl::Bomb, 0.94, 0.47, 11.0, 11.0),
                c(Ctl::Focus, 0.835, 0.90, 12.0, 12.0),
                c(Ctl::Auto, 0.85, 0.57, 8.0, 8.0),
                c(Ctl::Skip, 0.85, 0.09, 8.0, 8.0),
                c(Ctl::Pause, 0.95, 0.09, 8.0, 8.0),
                c(Ctl::Menu, 0.04, 0.09, 8.0, 8.0),
            ],
            portrait: vec![
                c(Ctl::Move, 0.275, 0.835, 0.55, 0.33),
                c(Ctl::Shot, 0.84, 0.85, 15.0, 15.0),
                c(Ctl::Bomb, 0.84, 0.70, 11.0, 11.0),
                c(Ctl::Focus, 0.62, 0.93, 12.0, 12.0),
                c(Ctl::Auto, 0.62, 0.78, 8.0, 8.0),
                c(Ctl::Skip, 0.72, 0.06, 8.0, 8.0),
                c(Ctl::Pause, 0.90, 0.06, 8.0, 8.0),
                c(Ctl::Menu, 0.08, 0.06, 8.0, 8.0),
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

    pub fn controls_mut(&mut self, landscape: bool) -> &mut Vec<Control> {
        if landscape {
            &mut self.landscape
        } else {
            &mut self.portrait
        }
    }

    // `to_cfg()`, or "" for the defaults.
    pub fn cfg_value(&self) -> String {
        if *self == Self::default() {
            String::new()
        } else {
            self.to_cfg()
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
        // A layout saved before a control existed gets that control at its default place.
        let def = Self::default();
        for (dst, d) in [
            (&mut out.landscape, &def.landscape),
            (&mut out.portrait, &def.portrait),
        ] {
            for c in d {
                if !dst.iter().any(|m| m.ctl == c.ctl) {
                    dst.push(*c);
                }
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
    // Relative drag: the player follows the finger's movement.
    Drag,
    // Floating 8-way stick centred where the finger went down.
    #[default]
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
            movement: Movement::Stick,
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
// resulting word is recorded, so determinism is unaffected).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TouchScene {
    // Menus, pause, game over: directions as pulses, a tap = SHOT, no auto-fire.
    #[default]
    Menu,
    Play {
        // Player position, game pixels.
        pos: [f32; 2],
        // Ortho speed per frame unfocused / focused (sht +0x24 / +0x28) and the x / y multipliers.
        speed: [f32; 2],
        mult: [f32; 2],
        // Auto-fire allowed: not during dialogue.
        fire: bool,
    },
}

impl TouchScene {
    pub fn screen(self) -> Screen {
        match self {
            TouchScene::Menu => Screen::Menu,
            TouchScene::Play { fire: true, .. } => Screen::Play,
            TouchScene::Play { fire: false, .. } => Screen::Dialogue,
        }
    }

    // The keyboard key Back stands for: X (cancel, back one level) in menus, Esc (the game's pause) in play.
    pub fn back_bit(self) -> u16 {
        match self {
            TouchScene::Menu => BOMB,
            TouchScene::Play { .. } => PAUSE,
        }
    }

    // Back for the live game: the pause menu only answers Esc, so Back there resumes instead of sending X.
    pub fn back_bit_of(w: &th08_core::world::World) -> u16 {
        let g = &w.game;
        if g.scene == th08_core::types::SceneId::Game && g.paused != 0 && g.game_over == 0 {
            PAUSE
        } else {
            Self::of(w).back_bit()
        }
    }

    pub fn of(w: &th08_core::world::World) -> Self {
        use th08_core::types::SceneId;
        let g = &w.game;
        let Some(p) = w.player.as_deref() else {
            return Self::Menu;
        };
        if g.scene != SceneId::Game || g.paused != 0 || g.game_over != 0 {
            return Self::Menu;
        }
        Self::Play {
            pos: [p.pos[0], p.pos[1]],
            speed: [p.sht.speed, p.sht_youkai.speed_focus],
            mult: p.speed_mult,
            fire: !w
                .msg
                .as_deref()
                .is_some_and(th08_core::msg::Dialogue::active),
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
    // Some poll has seen this finger down.
    polled: bool,
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

// Overlay draw state: placed controls, which are lit, and the movement finger's marks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TouchView {
    pub controls: Vec<(Placed, bool)>,
    pub screen: Screen,
    pub autofire: bool,
    pub opacity: f32,
    // Movement finger: (stick centre or drag start, finger), and the stick radius (0 = drag).
    pub finger: Option<([f32; 2], [f32; 2], f32)>,
}

// The touch device: fingers on controls, polled once per Input_Poll like the keyboard.
#[derive(Clone, Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent toggles and latches, not one mode"
)]
pub struct TouchSource {
    pub cfg: TouchConfig,
    // Window size and its safe rect (px): top-left and size; controls are placed in it. Px per mm.
    window: [f32; 2],
    origin: [f32; 2],
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
    // The Menu control was tapped; the app opens the port menu.
    menu_req: bool,
    // As `Keyboard::failed`: focus loss releases every finger.
    failed: bool,
}

impl TouchSource {
    pub fn new(cfg: TouchConfig) -> Self {
        Self {
            autofire: cfg.autofire,
            cfg,
            window: SCREEN,
            size: SCREEN,
            ppm: DP_PER_MM,
            ..Default::default()
        }
    }

    // Window size in px, winit's scale factor and the window's safe area (the blit letterboxes into the same rect).
    pub fn set_window(&mut self, w: u32, h: u32, scale_factor: f64, safe: SafeArea) {
        let [x, y, sw, sh] = safe.rect(w.max(1), h.max(1));
        self.window = [w.max(1) as f32, h.max(1) as f32];
        self.origin = [x, y];
        self.size = [sw, sh];
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
            self.menu_req = false;
        }
    }

    pub fn landscape(&self) -> bool {
        self.size[0] >= self.size[1]
    }

    // The safe rect (px): x, y, w, h.
    pub fn rect(&self) -> [f32; 4] {
        [self.origin[0], self.origin[1], self.size[0], self.size[1]]
    }

    // Visible controls of the current screen in window px.
    pub fn placed(&self) -> Vec<Placed> {
        let s = self.scene.screen();
        self.cfg
            .layout
            .controls(self.landscape())
            .iter()
            .filter(|c| c.visible && c.ctl.shown(s))
            .map(|c| self.place(c))
            .collect()
    }

    // A control in window px, kept inside the safe rect: a button's circle is moved in, the Move zone is cut to it.
    pub fn place(&self, c: &Control) -> Placed {
        let ([x0, y0], [w, h]) = (self.origin, self.size);
        let (centre, half) = if c.ctl == Ctl::Move {
            let lo = [(c.x - c.w / 2.0).max(0.0), (c.y - c.h / 2.0).max(0.0)];
            let hi = [(c.x + c.w / 2.0).min(1.0), (c.y + c.h / 2.0).min(1.0)];
            (
                [
                    x0 + (lo[0] + hi[0]) / 2.0 * w,
                    y0 + (lo[1] + hi[1]) / 2.0 * h,
                ],
                [
                    (hi[0] - lo[0]).max(0.0) * w / 2.0,
                    (hi[1] - lo[1]).max(0.0) * h / 2.0,
                ],
            )
        } else {
            let r = c.w * self.ppm / 2.0;
            let inside = |v: f32, lo: f32, len: f32| {
                if 2.0 * r >= len {
                    lo + len / 2.0
                } else {
                    v.clamp(lo + r, lo + len - r)
                }
            };
            (
                [inside(x0 + c.x * w, x0, w), inside(y0 + c.y * h, y0, h)],
                [r; 2],
            )
        };
        Placed {
            ctl: c.ctl,
            centre,
            half,
            opacity: c.opacity,
        }
    }

    fn control_mut(&mut self, ctl: Ctl) -> Option<&mut Control> {
        let l = self.landscape();
        self.cfg
            .layout
            .controls_mut(l)
            .iter_mut()
            .find(|c| c.ctl == ctl)
    }

    // Layout editor: moves `ctl` of the current orientation by `d` window px, kept inside the safe rect.
    pub fn edit_move(&mut self, ctl: Ctl, d: [f32; 2]) {
        let [_, _, w, h] = self.rect();
        let Some(c) = self
            .cfg
            .layout
            .controls(self.landscape())
            .iter()
            .find(|c| c.ctl == ctl)
        else {
            return;
        };
        let p = self.place(c);
        let x = (p.centre[0] + d[0] - self.origin[0]) / w;
        let y = (p.centre[1] + d[1] - self.origin[1]) / h;
        self.edit(ctl, |c| {
            c.x = x;
            c.y = y;
        });
    }

    // Layout editor: scales `ctl` by `k` (buttons 6–30 mm, the Move zone 10–100 % of the safe rect).
    pub fn edit_scale(&mut self, ctl: Ctl, k: f32) {
        self.edit(ctl, |c| {
            if c.ctl == Ctl::Move {
                c.w = (c.w * k).clamp(0.1, 1.0);
                c.h = (c.h * k).clamp(0.1, 1.0);
            } else {
                c.w = (c.w * k).clamp(BUTTON_MM[0], BUTTON_MM[1]);
                c.h = c.w;
            }
        });
    }

    // Layout editor: changes `ctl` of the current orientation, then keeps it inside the safe rect.
    pub fn edit(&mut self, ctl: Ctl, f: impl FnOnce(&mut Control)) {
        if let Some(c) = self.control_mut(ctl) {
            f(c);
        }
        self.fit(ctl);
    }

    // Stores the placed (inside the safe rect) position back into the fractions.
    pub fn fit(&mut self, ctl: Ctl) {
        let [x0, y0, w, h] = self.rect();
        let Some(c) = self
            .cfg
            .layout
            .controls(self.landscape())
            .iter()
            .find(|c| c.ctl == ctl)
        else {
            return;
        };
        let mut c = *c;
        if ctl == Ctl::Move {
            c.w = c.w.clamp(0.1, 1.0);
            c.h = c.h.clamp(0.1, 1.0);
            c.x = c.x.clamp(c.w / 2.0, 1.0 - c.w / 2.0);
            c.y = c.y.clamp(c.h / 2.0, 1.0 - c.h / 2.0);
        } else {
            let p = self.place(&c);
            c.x = (p.centre[0] - x0) / w;
            c.y = (p.centre[1] - y0) / h;
        }
        // Four decimals keep port.cfg readable (0.1 px on a 2400 px screen).
        let round = |v: f32| (v * 1e4).round() / 1e4;
        (c.x, c.y, c.w, c.h) = (round(c.x), round(c.y), round(c.w), round(c.h));
        if let Some(m) = self.control_mut(ctl) {
            *m = c;
        }
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
            On::Button(Ctl::Menu) => self.menu_req = true,
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
            polled: false,
        });
    }

    fn up(&mut self, id: u64, ended: bool) {
        let Some(i) = self.fingers.iter().position(|f| f.id == id) else {
            return;
        };
        let f = self.fingers.remove(i);
        // A tap shorter than one poll still presses its button once.
        if let On::Button(c) = f.on
            && !f.polled
            && c.bit() != 0
            && !(c == Ctl::Focus && self.cfg.focus_toggle)
        {
            self.pulse(c.bit());
        }
        if f.on == On::Move {
            self.target = None;
            // Menu tap = confirm.
            if ended && self.scene == TouchScene::Menu && f.travel < TAP_MM * self.ppm {
                self.pulse(SHOT);
            }
        }
    }

    // Android Back (button, gesture or a pad's Back key): one pulse of the key it stands for in `scene`.
    pub fn back(&mut self, scene: TouchScene) {
        self.back_key(scene.back_bit());
    }

    pub fn back_key(&mut self, bit: u16) {
        if !self.failed {
            self.pulse(bit);
        }
    }

    // A pulse is queued or its up poll is still owed.
    pub fn pulse_pending(&self) -> bool {
        self.gap || !self.pulses.is_empty()
    }

    pub fn take_menu(&mut self) -> bool {
        std::mem::take(&mut self.menu_req)
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
        for f in &mut self.fingers {
            f.polled = true;
        }
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
        // A pulse is one poll down, then one poll up, so the edge detection sees each.
        if !std::mem::take(&mut self.gap)
            && let Some(b) = self.pulses.pop_front()
        {
            self.gap = true;
            w |= b;
        }
        self.last_word = w;
        w
    }

    fn move_finger(&mut self) -> Option<&mut Finger> {
        self.fingers.iter_mut().find(|f| f.on == On::Move)
    }

    // Window px → game px (the blit's letterbox scale in the safe rect).
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
    // held keys themselves).
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
                    );
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
        held
    }

    // Android system gesture exclusion rects `[left, top, right, bottom]` (window px) in play: the controls that
    // reach within `EDGE_DP` of the safe rect's left or right side, by `Ctl::edge_rank`, cut to `EXCLUSION_DP` of
    // height per edge around each control's centre and stretched out to the window edge (the gesture starts in the
    // cutout / bar inset). None in menus.
    #[expect(clippy::cast_possible_truncation, reason = "window px fit i32")]
    pub fn exclusion(&self) -> Vec<[i32; 4]> {
        if self.scene == TouchScene::Menu {
            return Vec::new();
        }
        let dp = self.ppm / DP_PER_MM;
        let edge = EDGE_DP * dp;
        let mut placed = self.placed();
        placed.sort_by_key(|p| p.ctl.edge_rank());
        let mut out = Vec::new();
        for left in [true, false] {
            let mut budget = (EXCLUSION_DP * dp).floor();
            for p in &placed {
                let half = if p.ctl == Ctl::Move {
                    p.half
                } else {
                    [p.half[0] * 1.2; 2]
                };
                let (x0, x1) = (p.centre[0] - half[0], p.centre[0] + half[0]);
                let near = if left {
                    x0 <= self.origin[0] + edge
                } else {
                    x1 >= self.origin[0] + self.size[0] - edge
                };
                if !near || budget < EDGE_DP * dp / 2.0 {
                    continue;
                }
                let h = (2.0 * half[1]).min(budget).floor();
                budget -= h;
                let y0 = p.centre[1] - h / 2.0;
                let (x0, x1) = if left {
                    (0.0, x1)
                } else {
                    (x0, self.window[0])
                };
                out.push([
                    x0.floor() as i32,
                    y0.ceil() as i32,
                    x1.ceil() as i32,
                    (y0.ceil() + h) as i32,
                ]);
            }
        }
        out
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
            screen: self.scene.screen(),
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
#[expect(clippy::cast_sign_loss, reason = "the shifted angle is in [0, 360)")]
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
        t.set_window(640, 480, 1.0, SafeArea::default());
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

    // The phone case: controls inside the safe rect (cutout left, corners, gesture bar), hit testing on the same
    // placed rects, the letterbox scale of the safe rect.
    #[test]
    fn safe_area_placement() {
        let mut t = TouchSource::new(TouchConfig::default());
        let safe = SafeArea {
            left: 106,
            top: 32,
            right: 32,
            bottom: 72,
        };
        t.set_window(2712, 1220, 3.0, safe);
        let [x0, y0, w, h] = safe.rect(2712, 1220);
        for p in t.placed() {
            let [cx, cy] = p.centre;
            let [hx, hy] = p.half;
            assert!(cx - hx >= x0 - 0.5 && cx + hx <= x0 + w + 0.5, "{p:?}");
            assert!(cy - hy >= y0 - 0.5 && cy + hy <= y0 + h + 0.5, "{p:?}");
        }
        let pause = t
            .placed()
            .into_iter()
            .find(|p| p.ctl == Ctl::Pause)
            .unwrap();
        t.touch(1, TouchPhase::Started, pause.centre[0], pause.centre[1]);
        assert_eq!(t.poll() & PAUSE, PAUSE);
        assert_eq!(t.game_scale(), (1116.0 / 480.0f32).min(2574.0 / 640.0));
    }

    // A tap that starts and ends between two polls still presses once, then releases.
    #[test]
    fn sub_poll_tap_pulses() {
        let mut t = TouchSource::new(TouchConfig::default());
        t.set_window(2712, 1220, 3.0, SafeArea::default());
        let pause = t
            .placed()
            .into_iter()
            .find(|p| p.ctl == Ctl::Pause)
            .unwrap();
        t.touch(1, TouchPhase::Started, pause.centre[0], pause.centre[1]);
        t.touch(1, TouchPhase::Ended, pause.centre[0], pause.centre[1]);
        assert_eq!(t.poll() & PAUSE, PAUSE);
        assert_eq!(t.poll() & PAUSE, 0);
        t.touch(2, TouchPhase::Started, pause.centre[0], pause.centre[1]);
        assert_eq!(t.poll() & PAUSE, PAUSE);
        t.touch(2, TouchPhase::Ended, pause.centre[0], pause.centre[1]);
        assert_eq!(t.poll() & PAUSE, 0);
        assert_eq!(t.poll() & PAUSE, 0);
    }

    // Phones (landscape and portrait, with and without insets) and desktop windows: every default control inside
    // the safe rect, buttons at least 1 mm apart.
    #[test]
    fn default_layout_fits() {
        let cut = SafeArea {
            left: 106,
            top: 32,
            right: 32,
            bottom: 72,
        };
        let windows = [
            (2712, 1220, 3.0, cut),
            (2400, 1080, 2.625, SafeArea::default()),
            (2340, 1080, 2.75, cut),
            (1920, 1080, 2.0, SafeArea::default()),
            (2560, 1600, 2.0, SafeArea::default()),
            (640, 480, 1.0, SafeArea::default()),
            (1280, 960, 1.0, SafeArea::default()),
        ];
        for (w, h, k, safe) in windows {
            let rot = SafeArea {
                left: safe.bottom,
                top: safe.left,
                right: safe.top,
                bottom: safe.right,
            };
            for (w, h, safe) in [(w, h, safe), (h, w, rot)] {
                let mut t = TouchSource::new(TouchConfig::default());
                t.set_window(w, h, k, safe);
                t.set_scene(play([0.0; 2]));
                let [x0, y0, sw, sh] = t.rect();
                let placed = t.placed();
                for p in &placed {
                    let [cx, cy] = p.centre;
                    let [hx, hy] = p.half;
                    assert!(
                        cx - hx >= x0 - 0.5 && cx + hx <= x0 + sw + 0.5,
                        "{w}x{h} {p:?}"
                    );
                    assert!(
                        cy - hy >= y0 - 0.5 && cy + hy <= y0 + sh + 0.5,
                        "{w}x{h} {p:?}"
                    );
                }
                let buttons: Vec<_> = placed.iter().filter(|p| p.ctl != Ctl::Move).collect();
                for (i, a) in buttons.iter().enumerate() {
                    for b in &buttons[i + 1..] {
                        let d = len(a.centre[0] - b.centre[0], a.centre[1] - b.centre[1]);
                        let gap = (d - a.half[0] - b.half[0]) / t.ppm;
                        assert!(
                            gap >= 1.0,
                            "{w}x{h}: {:?} / {:?} gap {gap} mm",
                            a.ctl,
                            b.ctl
                        );
                    }
                }
            }
        }
    }

    // Editor moves and resizes stay inside the safe rect and round-trip through port.cfg.
    #[test]
    fn edit_keeps_inside() {
        let mut t = TouchSource::new(TouchConfig::default());
        t.set_window(2400, 1080, 2.625, SafeArea::default());
        let [x0, y0, w, h] = t.rect();
        t.edit_move(Ctl::Shot, [5000.0, 5000.0]);
        let s = t.placed().into_iter().find(|p| p.ctl == Ctl::Shot).unwrap();
        assert!((s.centre[0] - (x0 + w - s.half[0])).abs() < 0.5);
        assert!((s.centre[1] - (y0 + h - s.half[1])).abs() < 0.5);
        t.edit_scale(Ctl::Shot, 100.0);
        let c = t.cfg.layout.landscape[1];
        assert_eq!(c.w, BUTTON_MM[1]);
        let s = t.place(&c);
        assert!(s.centre[0] + s.half[0] <= x0 + w + 0.01);
        t.edit_move(Ctl::Move, [-9000.0, 0.0]);
        let m = t.cfg.layout.landscape[0];
        assert!((m.x - m.w / 2.0).abs() < 1e-6);
        t.edit_scale(Ctl::Move, 0.01);
        assert_eq!(t.cfg.layout.landscape[0].w, 0.1);
        assert_eq!(t.cfg.layout.portrait, TouchLayout::default().portrait);
        let l = &t.cfg.layout;
        assert_eq!(&TouchLayout::parse(&l.cfg_value()).unwrap(), l);
        assert_eq!(TouchLayout::default().cfg_value(), "");
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

    fn drag_cfg() -> TouchConfig {
        TouchConfig {
            movement: Movement::Drag,
            ..TouchConfig::default()
        }
    }

    // Relative drag: the player follows the finger's movement and stops within one step, no oscillation.
    #[test]
    fn drag_follows_finger() {
        let mut t = src(drag_cfg());
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
            movement: Movement::Drag,
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
        let mut t = src(drag_cfg());
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

    // Recorded-input check: touch-made input gives the same word stream as the keyboard, in play
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
            touch: src(drag_cfg()),
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

    // Back = one pulse of the key it stands for: X in menus, Esc in play and dialogue; the same words as tapping
    // those keys, one poll each.
    #[test]
    fn back_pulses_per_scene() {
        let mut t = src(TouchConfig::default());
        t.back(TouchScene::Menu);
        assert_eq!([t.poll(), t.poll()], [BOMB, 0]);
        t.set_scene(play([0.0; 2]));
        t.back(play([0.0; 2]));
        assert_eq!([t.poll(), t.poll()], [PAUSE, 0]);
        let dialogue = TouchScene::Play {
            pos: [0.0; 2],
            speed: [4.0, 2.0],
            mult: [1.0; 2],
            fire: false,
        };
        t.set_scene(dialogue);
        t.back(dialogue);
        assert_eq!([t.poll(), t.poll()], [PAUSE, 0]);
        // Two quick Backs stay two edges.
        t.set_scene(TouchScene::Menu);
        t.back(TouchScene::Menu);
        t.back(TouchScene::Menu);
        assert_eq!([t.poll(), t.poll(), t.poll(), t.poll()], [BOMB, 0, BOMB, 0]);
        // Unfocused: nothing.
        t.set_focus(false);
        t.back(TouchScene::Menu);
        t.set_focus(true);
        assert_eq!(t.poll(), 0);

        let mut kb = InputMux::default();
        let mut keys = Vec::new();
        for k in [KeyCode::KeyX, KeyCode::Escape] {
            kb.keyboard.key(k, true);
            keys.push(kb.poll()[0]);
            kb.keyboard.key(k, false);
            keys.push(kb.poll()[0]);
        }
        let mut m = InputMux {
            touch: src(TouchConfig::default()),
            ..Default::default()
        };
        let mut words = Vec::new();
        m.touch.back(TouchScene::Menu);
        words.extend([m.poll()[0], m.poll()[0]]);
        m.touch.set_scene(play([0.0; 2]));
        m.touch.back(play([0.0; 2]));
        words.extend([m.poll()[0], m.poll()[0]]);
        assert_eq!(words, keys);
    }

    // The MENU control asks for the port menu and never reaches the word; it is shown in menus and in play.
    #[test]
    fn menu_control() {
        let mut t = src(TouchConfig::default());
        for scene in [TouchScene::Menu, play([0.0; 2])] {
            t.set_scene(scene);
            let [x, y] = centre(&t, Ctl::Menu);
            t.touch(1, TouchPhase::Started, x, y);
            assert_eq!(t.poll(), 0);
            t.touch(1, TouchPhase::Ended, x, y);
            assert_eq!(t.poll(), 0);
            assert!(t.take_menu());
            assert!(!t.take_menu());
        }
    }

    // Menus hide FOCUS / AUTO / SKIP (not hit either); play and dialogue show every control.
    #[test]
    fn controls_per_screen() {
        let mut t = src(TouchConfig::default());
        let shown = |t: &TouchSource| t.placed().iter().map(|p| p.ctl).collect::<Vec<_>>();
        assert_eq!(t.view().screen, Screen::Menu);
        assert_eq!(
            shown(&t),
            [Ctl::Move, Ctl::Shot, Ctl::Bomb, Ctl::Pause, Ctl::Menu]
        );
        let focus = TouchSource::new(TouchConfig::default())
            .cfg
            .layout
            .landscape
            .iter()
            .find(|c| c.ctl == Ctl::Focus)
            .map(|c| t.place(c).centre)
            .unwrap();
        t.touch(1, TouchPhase::Started, focus[0], focus[1]);
        assert_eq!(t.poll(), 0);
        t.touch(1, TouchPhase::Ended, focus[0], focus[1]);
        t.set_scene(play([0.0; 2]));
        assert_eq!(t.view().screen, Screen::Play);
        assert_eq!(shown(&t).len(), Ctl::ALL.len());
        assert_eq!(
            TouchScene::Play {
                pos: [0.0; 2],
                speed: [1.0; 2],
                mult: [1.0; 2],
                fire: false
            }
            .screen(),
            Screen::Dialogue
        );
    }

    // A layout saved before MENU existed gets it at its default place.
    #[test]
    fn old_layout_gets_menu() {
        let mut old = TouchLayout::default();
        old.landscape.retain(|c| c.ctl != Ctl::Menu);
        old.portrait.retain(|c| c.ctl != Ctl::Menu);
        old.landscape[1].x = 0.5;
        let l = TouchLayout::parse(&old.to_cfg()).unwrap();
        assert_eq!(l.landscape[1].x, 0.5);
        assert_eq!(l.landscape.last().unwrap().ctl, Ctl::Menu);
        assert_eq!(l.portrait, TouchLayout::default().portrait);
    }

    // Exclusion rects: none in menus; in play the movement zone on the left and the near-edge buttons on the right,
    // at most 200 dp of height per edge.
    #[test]
    fn gesture_exclusion() {
        let mut t = TouchSource::new(TouchConfig::default());
        t.set_window(2400, 1080, 2.625, SafeArea::default());
        assert!(t.exclusion().is_empty());
        t.set_scene(play([0.0; 2]));
        let ex = t.exclusion();
        let cap = 200.0 * 2.625;
        let left: Vec<_> = ex.iter().filter(|r| r[0] <= 0).collect();
        let right: Vec<_> = ex.iter().filter(|r| r[2] >= 2400).collect();
        assert_eq!(left.len() + right.len(), ex.len(), "{ex:?}");
        for side in [&left, &right] {
            assert!(!side.is_empty());
            let h: i32 = side.iter().map(|r| r[3] - r[1]).sum();
            assert!(f64::from(h) <= cap, "{ex:?}");
        }
        // Left: the middle of the movement zone; right: SHOT first.
        let m = t.placed()[0];
        assert_eq!(m.ctl, Ctl::Move);
        let r = left[0];
        assert!((f64::from(r[1] + r[3]) / 2.0 - f64::from(m.centre[1])).abs() <= 1.0);
        let shot = centre(&t, Ctl::Shot);
        let r = right[0];
        assert!(r[1] as f32 <= shot[1] && shot[1] <= r[3] as f32, "{ex:?}");
        // Portrait too; a cutout inset on the left still covers the movement zone, out to the window edge.
        t.set_window(1080, 2400, 2.625, SafeArea::default());
        assert!(!t.exclusion().is_empty());
        let cut = SafeArea {
            left: 130,
            ..SafeArea::default()
        };
        t.set_window(2400, 1080, 2.625, cut);
        assert!(t.exclusion().iter().any(|r| r[0] == 0));
    }
}
