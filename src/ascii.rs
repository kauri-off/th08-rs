use crate::anmvm::{self, AnmVm};
use crate::game::Ctx;
use crate::gamemgr::{self, Game};
use crate::gfx::{Blend, FULLSCREEN, Rect};
use crate::input::{self, Input};
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;

pub const ANM_SLOT: usize = 1;
pub const CAPTURE_SLOT: usize = 3;
pub const CALC_PRIO: u16 = 1;
pub const DRAW_LOW_PRIO: u16 = 14;
pub const DRAW_HIGH_PRIO: u16 = 20;

// One queued ASCII string (12 §5.3): glyph = ascii.anm sprite (char − 1), advance 13 px × scale, '\n' = +16 px.
#[derive(Clone, Debug)]
pub struct Text {
    pub x: f32,
    pub y: f32,
    pub s: String,
    pub color: [u8; 4],
    pub scale: f32,
    pub advance: f32,
    pub high: bool,
}

impl Text {
    pub fn new(x: f32, y: f32, s: impl Into<String>) -> Self {
        Self { x, y, s: s.into(), color: [255; 4], scale: 1.0, advance: 13.0, high: true }
    }

    pub fn color(mut self, c: [u8; 4]) -> Self {
        self.color = c;
        self
    }

    // D3DCOLOR 0xAARRGGBB.
    pub fn argb(self, c: u32) -> Self {
        self.color([(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8])
    }

    pub fn scale(mut self, s: f32) -> Self {
        self.scale = s;
        self
    }

    pub fn advance(mut self, a: f32) -> Self {
        self.advance = a;
        self
    }

    pub fn low(mut self) -> Self {
        self.high = false;
        self
    }
}

#[derive(Default)]
pub struct Queue {
    items: Vec<Text>,
}

impl Queue {
    pub fn push(&mut self, t: Text) {
        if self.items.len() < 256 {
            self.items.push(t);
        }
    }

    fn draw(&mut self, ctx_anm: &crate::anmmgr::AnmManager, gfx: &mut crate::gfx::Gfx, high: bool) {
        gfx.set_viewport(FULLSCREEN);
        for t in self.items.iter().filter(|t| t.high == high) {
            let (mut x, mut y) = (t.x, t.y);
            for c in t.s.bytes() {
                if c == b'\n' {
                    (x, y) = (t.x, y + 16.0);
                    continue;
                }
                if let Some(sp) = ctx_anm.sprite(ANM_SLOT, c.wrapping_sub(1) as usize).filter(|_| c > b' ') {
                    gfx.sprite(sp.tex, Blend::Alpha, Rect::new(x, y, sp.w * t.scale, sp.h * t.scale), sp.uv, t.color);
                }
                x += t.advance * t.scale;
            }
        }
        self.items.retain(|t| t.high != high);
    }
}

pub fn text_width(s: &str, advance: f32, scale: f32) -> f32 {
    s.len() as f32 * advance * scale
}

fn set_rgb(vm: &mut AnmVm, argb: u32) {
    vm.color[anmvm::R] = (argb >> 16) as u8;
    vm.color[anmvm::G] = (argb >> 8) as u8;
    vm.color[anmvm::B] = argb as u8;
}

// Pause menu (12 §3); VM i runs ascii.anm script 12 + i.
#[derive(Default)]
struct PauseMenu {
    state: u8,
    frame: u32,
    vms: Vec<AnmVm>,
    shown: [bool; 10],
}

const P_TITLE: usize = 0;
const P_ITEMS: [usize; 3] = [1, 2, 3];
const P_REALLY: usize = 4;
const P_YES: usize = 5;
const P_NO: usize = 6;
const P_DIFF: usize = 7;
const P_PRACTICE: usize = 8;

impl PauseMenu {
    fn show(&mut self, k: usize, on: bool) {
        if self.shown[k] != on {
            self.shown[k] = on;
            self.vms[k].pending_interrupt = if on { 1 } else { 2 };
        }
    }

    fn goto(&mut self, s: u8) {
        self.state = s;
        self.frame = 0;
    }

    fn update(&mut self, ctx_anm: &mut crate::anmmgr::AnmManager, g: &mut Game, inp: &Input, audio: &mut crate::audio::Audio) {
        let replay = g.replay();
        if self.state == 0 {
            // The practice / slow-mode labels (scripts 20, 21) fade in on start, so they only run when they apply.
            // SPEC-GAP: slow mode is not configurable yet, so script 21 never runs.
            let practice = g.practice();
            self.vms = (12..22)
                .map(|s| {
                    let mut vm = AnmVm::default();
                    if s < 20 || (s == 20 && practice) {
                        ctx_anm.set_script(&mut vm, ANM_SLOT, s);
                    }
                    vm
                })
                .collect();
            self.shown = [false; 10];
            let d = if g.spell_practice() { 0x120 - 0x11B } else { g.difficulty.clamp(0, 4) };
            let diff_vm = &mut self.vms[P_DIFF];
            diff_vm.set_sprite(ctx_anm, 0x11B + d);
            for k in [P_TITLE, 1, 2, P_DIFF] {
                self.show(k, true);
            }
            if !replay {
                self.show(3, true);
            }
            if g.practice() {
                self.show(P_PRACTICE, true);
            }
            self.goto(1);
        }
        let pressed = |b: u16| self.frame >= 4 && inp.pressed(b);
        let confirm = pressed(input::SHOT | input::ENTER);
        let (up, down) = (pressed(input::UP), pressed(input::DOWN));
        if matches!(self.state, 1..=3 | 5..=8) {
            if up || down {
                crate::sound::play(crate::sound::PLST);
            } else if confirm {
                crate::sound::play(crate::sound::OK);
            }
            if inp.pressed(input::PAUSE) {
                self.goto(4);
            } else if inp.pressed(input::KEY_Q) {
                self.goto(9);
            } else if inp.pressed(input::KEY_R) && !replay {
                self.goto(10);
            }
        }
        match self.state {
            1 => {
                if up {
                    self.goto(if replay { 2 } else { 3 });
                } else if down {
                    self.goto(2);
                } else if confirm {
                    self.goto(4);
                }
            }
            2 => {
                if up {
                    self.goto(1);
                } else if down {
                    self.goto(if replay { 1 } else { 3 });
                } else if confirm {
                    self.confirm_screen(true);
                    self.goto(6);
                }
            }
            3 => {
                if up {
                    self.goto(2);
                } else if down {
                    self.goto(1);
                } else if confirm {
                    self.confirm_screen(true);
                    self.goto(8);
                }
            }
            5..=8 => {
                let base = if self.state <= 6 { 5 } else { 7 };
                if up || down {
                    let s = if self.state == base { base + 1 } else { base };
                    self.goto(s);
                } else if confirm {
                    if self.state == base {
                        self.goto(if base == 5 { 9 } else { 10 });
                    } else {
                        self.confirm_screen(false);
                        self.goto(if base == 5 { 2 } else { 3 });
                    }
                }
            }
            4 | 9 | 10 => {
                if self.frame == 0 {
                    for k in 0..10 {
                        self.show(k, false);
                    }
                }
                if self.frame >= 20 {
                    match self.state {
                        4 => audio.pause_bgm(false),
                        9 => g.request(gamemgr::SCENE_MENU),
                        _ => g.request(if g.practice() || g.spell_practice() || g.difficulty >= 4 { gamemgr::SCENE_RELOAD } else { gamemgr::SCENE_RETRY }),
                    }
                    g.paused = 0;
                    self.state = 0;
                    self.vms.clear();
                    return;
                }
            }
            _ => {}
        }
        for (i, &k) in P_ITEMS.iter().enumerate() {
            set_rgb(&mut self.vms[k], if self.state as usize == i + 1 { 0xFFFFFFFF } else { 0xFF505050 });
        }
        let yes_sel = matches!(self.state, 5 | 7);
        set_rgb(&mut self.vms[P_YES], if yes_sel { 0xFFFF8080 } else { 0xFF505050 });
        set_rgb(&mut self.vms[P_NO], if !yes_sel { 0xFFFF8080 } else { 0xFF505050 });
        for vm in self.vms.iter_mut().filter(|v| v.script.is_some()) {
            vm.run(ctx_anm);
        }
        self.frame += 1;
    }

    fn confirm_screen(&mut self, on: bool) {
        for k in [P_TITLE, 1, 2, 3] {
            self.show(k, !on);
        }
        for k in [P_REALLY, P_YES, P_NO] {
            self.show(k, on);
        }
    }

    fn selected(&self, k: usize) -> bool {
        match k {
            1..=3 => self.state as usize == k,
            P_YES => matches!(self.state, 5 | 7),
            P_NO => matches!(self.state, 6 | 8),
            _ => false,
        }
    }
}

// Continue / game-over menu (11 §4.2, 12 §4): ascii scripts 22–24 and times.anm script 1 (night clock).
#[derive(Default)]
struct ContinueMenu {
    open: bool,
    yes: bool,
    frame: u32,
    phase: u8,
    phase_frame: u32,
    vms: Vec<AnmVm>,
    clock_vm: AnmVm,
}

impl ContinueMenu {
    fn update(&mut self, ctx_anm: &mut crate::anmmgr::AnmManager, g: &mut Game, inp: &Input) {
        if !self.open {
            *self = ContinueMenu { open: true, ..ContinueMenu::default() };
            if g.practice() && !g.spell_practice() {
                g.request(gamemgr::SCENE_RESULT);
                return;
            }
            if g.replay() {
                g.request(gamemgr::SCENE_REPLAY_END);
                return;
            }
            if g.difficulty < 4 && !g.spell_practice() && (g.clock > 10 || g.stage % 9 == 7) {
                g.flags &= !gamemgr::F_ENDING_OK;
                g.request(gamemgr::SCENE_ENDING);
                return;
            }
            self.vms = (22..25)
                .map(|s| {
                    let mut vm = AnmVm::default();
                    ctx_anm.set_script(&mut vm, ANM_SLOT, s);
                    vm.pending_interrupt = 1;
                    vm
                })
                .collect();
            ctx_anm.set_script(&mut self.clock_vm, crate::hud::TIMES_SLOT, 1);
            self.clock_vm.set_sprite(ctx_anm, g.clock.clamp(0, 15));
            self.yes = g.difficulty >= 4 || g.spell_practice();
        }
        let ready = self.frame >= 30;
        match self.phase {
            0 if ready => {
                if inp.pressed(input::UP) || inp.pressed(input::DOWN) {
                    self.yes = !self.yes;
                    crate::sound::play(crate::sound::PLST);
                }
                if inp.pressed(input::SHOT | input::ENTER) {
                    crate::sound::play(crate::sound::OK);
                    self.phase = if !self.yes {
                        1
                    } else if g.difficulty >= 4 || g.spell_practice() {
                        2
                    } else {
                        3
                    };
                    self.phase_frame = 0;
                    if self.phase != 3 {
                        self.vms.iter_mut().for_each(|v| v.pending_interrupt = 2);
                        self.clock_vm.pending_interrupt = 2;
                    }
                }
            }
            1 | 2 if self.phase_frame >= 20 => {
                g.request(if self.phase == 1 { gamemgr::SCENE_RESULT } else { gamemgr::SCENE_RELOAD });
                self.open = false;
                return;
            }
            3 => {
                if self.phase_frame == 15 {
                    g.continue_clock = true;
                    g.clock += 1;
                    self.clock_vm.set_sprite(ctx_anm, g.clock.clamp(0, 15));
                    self.clock_vm.pending_interrupt = 1;
                } else if self.phase_frame == 60 {
                    self.vms.iter_mut().for_each(|v| v.pending_interrupt = 2);
                    self.clock_vm.pending_interrupt = 2;
                } else if self.phase_frame >= 90 {
                    g.continue_apply = true;
                    g.game_over = 0;
                    self.open = false;
                    return;
                }
            }
            _ => {}
        }
        let (on, off) = (0xFFFF8080, 0xFF505050);
        if let [_, y, n] = &mut self.vms[..] {
            set_rgb(y, if self.yes { on } else { off });
            set_rgb(n, if self.yes { off } else { on });
        }
        for vm in self.vms.iter_mut().chain(std::iter::once(&mut self.clock_vm)).filter(|v| v.script.is_some()) {
            vm.run(ctx_anm);
        }
        self.frame += 1;
        self.phase_frame += 1;
    }
}

// ASCII manager (calc 1, draw 14/20; slots 1 ascii.anm, 3 capture.anm): text queue, pause and continue menus.
#[derive(Default)]
pub struct AsciiManager {
    pause: PauseMenu,
    cont: ContinueMenu,
}

impl AsciiManager {
    pub fn register(ctx: &mut Ctx) -> Result<()> {
        let t = task::task(AsciiManager::default());
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_LOW_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Draw, DRAW_HIGH_PRIO, 1, t)?;
        Ok(())
    }
}

impl Task<Ctx> for AsciiManager {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            ctx.anm.load(&mut ctx.gfx, &ctx.archive, ANM_SLOT, "ascii.anm")?;
            ctx.anm.load(&mut ctx.gfx, &ctx.archive, CAPTURE_SLOT, "capture.anm")?;
        }
        Ok(())
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        let inp = ctx.input;
        if ctx.game.paused != 0 {
            self.pause.update(&mut ctx.anm, &mut ctx.game, &inp, &mut ctx.audio);
        } else if ctx.game.game_over != 0 {
            self.cont.update(&mut ctx.anm, &mut ctx.game, &inp);
        } else {
            self.cont.open = false;
            self.pause.state = 0;
        }
        Ret::Next
    }

    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        let high = tag == 1;
        if high && ctx.game.paused != 0 {
            ctx.gfx.set_viewport(FULLSCREEN);
            let sel: Vec<bool> = (0..self.pause.vms.len()).map(|k| self.pause.selected(k)).collect();
            for (vm, sel) in self.pause.vms.iter_mut().zip(sel) {
                let pos = vm.pos;
                if sel {
                    vm.pos = [pos[0] - 4.0, pos[1] - 4.0, pos[2]];
                }
                ctx.anm.draw(&mut ctx.gfx, vm);
                vm.pos = pos;
            }
        }
        if high && ctx.game.game_over != 0 && self.cont.open {
            ctx.gfx.set_viewport(FULLSCREEN);
            for vm in self.cont.vms.iter_mut().chain(std::iter::once(&mut self.cont.clock_vm)) {
                ctx.anm.draw(&mut ctx.gfx, vm);
            }
        }
        let mut q = std::mem::take(&mut ctx.ascii);
        q.draw(&ctx.anm, &mut ctx.gfx, high);
        ctx.ascii = q;
        Ret::Next
    }
}
