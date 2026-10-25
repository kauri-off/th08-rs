//! ASCII manager 0x4CCE20: string queue, score popups, gauge readout, boss markers, pause and continue menus (12 §2.9, §3, §4, §5.9, §5.10; 13 §4).

use crate::anmvm::{self, AnmVm};
use crate::game::Ctx;
use crate::gamemgr::{self, Game};
use crate::gfx::{Blend, FULLSCREEN};
use crate::input::{self, Input};
use crate::task::{self, Chain, Ret, Task};
use anyhow::Result;

// ascii.anm slot 1, capture.anm slot 3 (12 §3); calc prio 1, draw prio 14 / 20 (12 §5.7).
pub const ANM_SLOT: usize = 1;
pub const CAPTURE_SLOT: usize = 3;
pub const CALC_PRIO: u16 = 1;
pub const DRAW_LOW_PRIO: u16 = 14;
pub const DRAW_HIGH_PRIO: u16 = 20;

// One queued ASCII string (12 §2.9, §5.3): glyph = ascii.anm sprite (char − 1), advance 13 px × scale, '\n' = +16 px.
#[derive(Clone, Debug)]
pub struct Text {
    pub x: f32,
    pub y: f32,
    pub s: String,
    pub color: [u8; 4],
    pub scale: f32,
    // Glyph scale x / y (+0x826C / +0x8270), advance per glyph +0x8280 (13 px × scale x) (12 §2.9).
    pub scale_y: f32,
    pub advance: f32,
    // Port-only: drawn in the prio-20 pass (true) or the prio-14 pass.
    pub high: bool,
    // Highlight state +0x8278 (12 §2.9).
    pub hl: bool,
    // +0x8274: 1 = drawn in the playfield viewport (12 §2.9).
    pub clip: bool,
}

impl Text {
    pub fn new(x: f32, y: f32, s: impl Into<String>) -> Self {
        Self {
            x,
            y,
            s: s.into(),
            color: [255; 4],
            scale: 1.0,
            scale_y: 1.0,
            advance: 13.0,
            high: true,
            hl: false,
            clip: false,
        }
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
        self.scale_y = s;
        self
    }

    pub fn scale_xy(mut self, x: f32, y: f32) -> Self {
        self.scale = x;
        self.scale_y = y;
        self
    }

    pub fn advance(mut self, a: f32) -> Self {
        self.advance = a;
        self
    }

    // Highlight glyph set (12 §2.9): sprite c + 0x8A (asciis.png), colour forced to white.
    pub fn highlight(mut self, on: bool) -> Self {
        self.hl = on;
        self
    }

    pub fn clip(mut self) -> Self {
        self.clip = true;
        self
    }

    pub fn low(mut self) -> Self {
        self.high = false;
        self
    }
}

// Per-frame string queue, at most 256 entries, excess ignored (12 §2.9).
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

    // Ascii_DrawStrings 0x402B20 (12 §2.9): '\n' → y += 16·scale y, ' ' → advance only, other bytes Draw2DSnap.
    fn draw(&mut self, ctx_anm: &crate::anmmgr::AnmManager, gfx: &mut crate::gfx::Gfx, high: bool) {
        let o = ctx_anm.screen_offset;
        for t in self.items.iter().filter(|t| t.high == high) {
            let vp = if t.clip {
                crate::gfx::PLAYFIELD
            } else {
                FULLSCREEN
            };
            gfx.set_viewport(vp);
            let (mut x, mut y) = (t.x + o[0], t.y + o[1]);
            // Chars U+0000-U+00FF are glyph bytes (U+0080 / U+0081 = the "SP" / "END" cells, 16 §2.2).
            for c in t.s.chars().map(|c| c as u32 as u8) {
                if c == b'\n' {
                    (x, y) = (t.x + o[0], y + 16.0 * t.scale_y);
                    continue;
                }
                let (idx, col) = if t.hl {
                    (c as usize + 0x8A, [255; 4])
                } else {
                    (c.wrapping_sub(1) as usize, t.color)
                };
                // Draw2DSnap draws nothing for alpha 0 (05 §9); the pen still advances.
                let drawn = c > b' ' && col[3] != 0;
                if let Some(sp) = ctx_anm.sprite(ANM_SLOT, idx).filter(|_| drawn) {
                    // Glyph VM: z 0, z-write on (12 §5.3 depth values).
                    let [u0, v0, u1, v1] = sp.uv;
                    let (w, h) = (sp.w * t.scale, sp.h * t.scale_y);
                    // Draw2DSnap culls quads outside the viewport (05 §9 EmitQuad step 3).
                    let (rx, ry) = (x.round_ties_even() - 0.5, y.round_ties_even() - 0.5);
                    if rx + w < vp.x || rx > vp.x + vp.w || ry + h < vp.y || ry > vp.y + vp.h {
                        x += t.advance * t.scale;
                        continue;
                    }
                    let v = |x: f32, y: f32, u: f32, vv: f32| {
                        crate::gfx::Vertex::screen(
                            x.round_ties_even(),
                            y.round_ties_even(),
                            0.0,
                            [u, vv],
                            col,
                        )
                    };
                    gfx.strip(
                        sp.tex,
                        Blend::Alpha,
                        true,
                        [
                            v(x, y, u0, v0),
                            v(x + w, y, u1, v0),
                            v(x, y + h, u0, v1),
                            v(x + w, y + h, u1, v1),
                        ],
                    );
                }
                x += t.advance * t.scale;
            }
        }
        self.items.retain(|t| t.high != high);
    }
}

// Draws one string at once (outside the queue), in the caller's draw pass (like Ascii_DrawGaugePercent, 12 §2.9).
pub fn draw_now(anm: &crate::anmmgr::AnmManager, gfx: &mut crate::gfx::Gfx, t: Text) {
    let high = t.high;
    let mut q = Queue { items: vec![t] };
    q.draw(anm, gfx, high);
}

// Small popups: slots 0–719 plus 3 special slots 720–722, life 60; large popups: 128 slots, life 90 (13 §4).
pub const POPUP_SLOTS: usize = 720;
const POPUP_LIFE: f32 = 60.0;
const LARGE_SLOTS: usize = 128;
const LARGE_LIFE: f32 = 90.0;

// Small score popup (13 §4): digits least significant first, glyph 10 = the "PowerUp" sign.
#[derive(Clone, Default, Debug)]
pub struct Popup {
    pub digits: Vec<u8>,
    pub pos: [f32; 2],
    // D3DCOLOR 0xAARRGGBB; only RGB is used, the alpha byte is replaced by the distance fade.
    pub colour: u32,
    pub age: f32,
}

// Large popup (13 §4): glyphs LSB first, pos in screen coordinates, scale copied from the manager at spawn.
#[derive(Clone, Debug)]
pub struct LargePopup {
    pub glyphs: Vec<u8>,
    pub pos: [f32; 2],
    pub colour: u32,
    pub scale: f32,
    pub age: f32,
}

// Popup arrays at Ascii +0xB4DC (small) and +0x15304 (large) with their round-robin cursors (+0x8298 for large).
#[derive(Default)]
pub struct Popups {
    pub slots: Vec<Option<Popup>>,
    next: usize,
    special: usize,
    pub large: Vec<Option<LargePopup>>,
    large_next: usize,
}

thread_local! {
    static POPUPS: std::cell::RefCell<Popups> = std::cell::RefCell::new(Popups { slots: vec![None; POPUP_SLOTS + 3], next: 0, special: 0, large: vec![None; LARGE_SLOTS], large_next: 0 });
}

// At most 12 digits, least significant first; a negative value is the single glyph 10 (13 §4).
fn digits(value: i32) -> Vec<u8> {
    if value < 0 {
        return vec![10];
    }
    let mut v = value as u32;
    let mut d = Vec::new();
    loop {
        d.push((v % 10) as u8);
        v /= 10;
        if v == 0 || d.len() == 12 {
            return d;
        }
    }
}

// Popup_Number 0x403200: slots 0–719 round robin; pos in playfield coordinates; a negative value shows "PowerUp".
pub fn popup(pos: [f32; 3], value: i32, colour: u32) {
    POPUPS.with_borrow_mut(|p| {
        let i = p.next;
        p.next = (p.next + 1) % POPUP_SLOTS;
        p.slots[i] = Some(Popup {
            digits: digits(value),
            pos: [pos[0] + 32.0, pos[1] + 16.0],
            colour,
            age: 0.0,
        });
    });
}

// 0x403330: the three extra slots 720–722, round robin.
pub fn popup_special(pos: [f32; 3], value: i32, colour: u32) {
    POPUPS.with_borrow_mut(|p| {
        let i = POPUP_SLOTS + p.special;
        p.special = (p.special + 1) % 3;
        p.slots[i] = Some(Popup {
            digits: digits(value),
            pos: [pos[0] + 32.0, pos[1] + 16.0],
            colour,
            age: 0.0,
        });
    });
}

// Large popup glyphs, least significant first: [0xF, p4 digits, 0xE,] p3 digits, 0xD → "+p3" / "+p3(p4)".
// The "(p4)" part only when p4 > 0; p3 < 1 is a single '0' (13 §4).
fn large_glyphs(p3: i32, p4: i32) -> Vec<u8> {
    let mut g = Vec::new();
    if p4 > 0 {
        g.push(0xF);
        g.extend(digits(p4));
        g.push(0xE);
    }
    g.extend(digits(p3.max(0)));
    g.push(0xD);
    g
}

// Popup_Large 0x403460 (life 90); `pos` in playfield coordinates.
pub fn popup_large(pos: [f32; 3], p3: i32, p4: i32, colour: u32, scale: f32) {
    POPUPS.with_borrow_mut(|p| {
        let i = p.large_next;
        p.large_next = (i + 1) % LARGE_SLOTS;
        p.large[i] = Some(LargePopup {
            glyphs: large_glyphs(p3, p4),
            pos: [pos[0] + 32.0, pos[1] + 16.0],
            colour,
            scale,
            age: 0.0,
        });
    });
}

// Popup_LargeStatic 0x403600: starts at age 88 (a two-frame display), x += count · 3.5 (count includes the '+').
pub fn popup_large_static(pos: [f32; 2], p3: i32, p4: i32, colour: u32) {
    let glyphs = large_glyphs(p3, p4);
    let x = pos[0] + 32.0 + glyphs.len() as f32 * 3.5;
    POPUPS.with_borrow_mut(|p| {
        let i = p.large_next;
        p.large_next = (i + 1) % LARGE_SLOTS;
        p.large[i] = Some(LargePopup {
            glyphs,
            pos: [x, pos[1] + 16.0],
            colour,
            scale: 1.0,
            age: 88.0,
        });
    });
}

pub fn clear_popups() {
    POPUPS.with_borrow_mut(|p| {
        p.slots.iter_mut().for_each(|s| *s = None);
        p.large.iter_mut().for_each(|s| *s = None);
    });
}

// Ascii_Calc popup part (13 §4): small popups rise 0.5·gs per frame; large popups do not move.
fn popups_calc(gs: f32) {
    POPUPS.with_borrow_mut(|p| {
        for s in p.slots.iter_mut() {
            if let Some(q) = s {
                q.pos[1] -= 0.5 * gs;
                q.age += gs;
                // Timer_Greater(60): drawn with ages 0..60 (13 §4).
                if q.age > POPUP_LIFE {
                    *s = None;
                }
            }
        }
        for s in p.large.iter_mut() {
            if let Some(q) = s {
                q.age += gs;
                // Freed when age > 90 (13 §4): a static popup issued at 88 is drawn at 89 and 90.
                if q.age > LARGE_LIFE {
                    *s = None;
                }
            }
        }
    });
}

// Squared player distance d (truncated to int) → alpha 0x50 / ramp / 0xD0 (13 §4). The player is in playfield
// coordinates and the popup in screen coordinates; the origin is not subtracted (QUIRK).
fn popup_alpha(pos: [f32; 2], player: [f32; 3]) -> u8 {
    let (dx, dy) = (pos[0] - player[0], pos[1] - player[1]);
    let d = (dx * dx + dy * dy) as i32;
    if d <= 1024 {
        0x50
    } else if d <= 4096 {
        (0x50 + (d - 1024) * 128 / 3072) as u8
    } else {
        0xD0
    }
}

// Popup VMs Ascii +0x2A4 (sprite 0, 8×8) / +0x548 (sprite 0x88, 8×12) after Ascii_InitVms 0x402450; AnmVm_Init
// resets the flags to 7, so the 0x1800 anchor bits ORed in before are lost and the glyphs are centred (QUIRK, 13 §4).
fn popup_vm(anm: &crate::anmmgr::AnmManager, sprite: i32, z: f32) -> AnmVm {
    let mut vm = AnmVm {
        anm_slot: ANM_SLOT as _,
        ..AnmVm::default()
    };
    vm.init();
    vm.set_sprite(anm, sprite);
    vm.pos[2] = z;
    vm
}

// Per glyph only the sprite width is copied; the height stays the init sprite's (13 §4).
fn popup_glyph(anm: &crate::anmmgr::AnmManager, vm: &mut AnmVm, idx: usize) -> bool {
    let Some(sp) = anm.sprite(ANM_SLOT, idx) else {
        return false;
    };
    vm.sprite_index = idx as i16;
    vm.sprite_w = sp.w;
    true
}

// Ascii_DrawPopups 0x405420 popup part (13 §4): small popups, the darkness mask (§4.1), then large ones, all Draw2DSnap in the playfield viewport.
fn popups_draw(anm: &mut crate::anmmgr::AnmManager, gfx: &mut crate::gfx::Gfx) {
    gfx.set_viewport(crate::gfx::PLAYFIELD);
    let pl = crate::effect::with(|e| e.player);
    let mut small = popup_vm(anm, 0, 0.1);
    let mut large = popup_vm(anm, 0x88, 0.0);
    POPUPS.with_borrow(|p| {
        for q in p.slots.iter().flatten() {
            set_rgb(&mut small, q.colour);
            small.color[anmvm::A] = popup_alpha(q.pos, pl);
            // Digit glyph g → sprite g (age < 52), g + 11 (52–55), g + 21 (≥ 56).
            let shift = if q.age < 52.0 {
                0
            } else if q.age < 56.0 {
                11
            } else {
                21
            };
            // x starts at pos.x − 4·digits, advance 8, most significant (last stored) first.
            let x0 = q.pos[0] - 4.0 * q.digits.len() as f32;
            for (k, &g) in q.digits.iter().rev().enumerate() {
                if popup_glyph(anm, &mut small, (g + shift) as usize) {
                    small.pos[0] = x0 + 8.0 * k as f32;
                    small.pos[1] = q.pos[1];
                    anm.draw_mode(gfx, &mut small, 0);
                }
            }
        }
        darkness_draw(anm, gfx, pl);
        for q in p.large.iter().flatten() {
            set_rgb(&mut large, q.colour);
            large.color[anmvm::A] = popup_alpha(q.pos, pl);
            large.scale = [q.scale; 2];
            // x0 = pos.x − count·3.5 (count = stored glyphs incl. '+', unscaled), advance 7·scale.x, sprite 0x88 + glyph.
            let x0 = q.pos[0] - q.glyphs.len() as f32 * 3.5;
            for (k, &g) in q.glyphs.iter().rev().enumerate() {
                if popup_glyph(anm, &mut large, 0x88 + g as usize) {
                    large.pos[0] = x0 + 7.0 * q.scale * k as f32;
                    large.pos[1] = q.pos[1];
                    anm.draw_mode(gfx, &mut large, 0);
                }
            }
        }
    });
}

// Darkness mask (13 §4.1): alpha Ascii +0x16F08 (int, 0x4E3D28), radius +0x16F04 (f32, 0x4E3D24), hole VM +0x16F0C.
#[derive(Default)]
struct Darkness {
    alpha: i32,
    radius: f32,
    vm: AnmVm,
}

thread_local! {
    static DARKNESS: std::cell::RefCell<Darkness> = std::cell::RefCell::new(Darkness::default());
}

// ECL native 0 (07 §12): alpha := int var 10000, radius := float var 10016.
pub fn set_darkness(alpha: i32, radius: f32) {
    DARKNESS.with_borrow_mut(|d| (d.alpha, d.radius) = (alpha, radius));
}

// Alpha := 0 by `Spell_End`, `GameManager_StageStart` and `GameMgr_OnRemove` (13 §4.1); the radius is kept.
pub fn clear_darkness() {
    DARKNESS.with_borrow_mut(|d| d.alpha = 0);
}

pub fn darkness_alpha() -> i32 {
    DARKNESS.with_borrow(|d| d.alpha)
}

// The four black rects (x0, y0, x1, y1) around the player P (playfield) with radius r and shake offset s (13 §4.1).
fn darkness_rects(p: [f32; 2], r: f32, s: [f32; 2]) -> Vec<[f32; 4]> {
    let (left, right) = (p[0] + 32.0 - r + s[0], p[0] + 32.0 + r + s[0]);
    let (top, bottom) = (p[1] + 16.0 - r + s[1], p[1] + 16.0 + r + s[1]);
    let mut out = Vec::with_capacity(4);
    if left > 32.0 {
        out.push([32.0, 16.0, left, 464.0]);
    }
    if right < 416.0 {
        out.push([right, 16.0, 416.0, 464.0]);
    }
    let (x0, x1) = (left.max(32.0), right.min(416.0));
    if top > 16.0 {
        out.push([x0, 16.0, x1, top]);
    }
    if bottom < 464.0 {
        out.push([x0, bottom, x1, 464.0]);
    }
    out
}

// Drawn by Ascii_DrawPopups between the small and large popups while alpha > 0: Screen_FillRect rects (untextured,
// z 0, ZFUNC unchanged), then etama.anm script 105 (sprite 229, 128×128) set every frame, scale r / 63, Draw2DSnap.
fn darkness_draw(anm: &mut crate::anmmgr::AnmManager, gfx: &mut crate::gfx::Gfx, player: [f32; 3]) {
    DARKNESS.with_borrow_mut(|d| {
        if d.alpha <= 0 {
            return;
        }
        let a = d.alpha as u8;
        for [x0, y0, x1, y1] in darkness_rects([player[0], player[1]], d.radius, anm.screen_offset)
        {
            gfx.rect(x0, y0, x1 - x0, y1 - y0, [0, 0, 0, a]);
        }
        anm.file_set_script(&mut d.vm, crate::effect::SCRIPT_SLOT, 105);
        d.vm.scale = [d.radius / 63.0; 2];
        d.vm.pos = [player[0] + 32.0, player[1] + 16.0, player[2]];
        d.vm.color[anmvm::A] = a;
        anm.draw_mode(gfx, &mut d.vm, 0);
    });
}

// Human/youkai gauge readout VMs (12 §5.9): bar, 人, 妖, marker, glyph stamp.
// VMs G0–G4 at ASCII manager +0x7EC.. and the stored interrupt +0x827C (12 §5.9).
#[derive(Default)]
struct Gauge {
    vms: Vec<AnmVm>,
    id: i32,
    player: std::rc::Weak<std::cell::RefCell<crate::player::Player>>,
    // Boss "ENEMY" markers M_k at +0x17C4 + k·0x2A4, k = boss slot (12 §5.10).
    markers: Vec<AnmVm>,
    // +0x2254[slot]: 0 plain, 1 hit flash, 2–4 low-life blink (warning level + 1), written by EnemyMgr_Calc.
    marker_mode: [u8; 4],
    // +0x8284: incremented at the end of every Ascii_Calc, never reset, starts at 0 (12 §5.10).
    frame: u32,
}

thread_local! {
    static GAUGE: std::cell::RefCell<Gauge> = std::cell::RefCell::new(Gauge::default());
}

const G_BAR: usize = 0;
const G_HUMAN: usize = 1;
const G_YOUKAI: usize = 2;
const G_MARKER: usize = 3;
const G_GLYPH: usize = 4;

// Ascii_SetupGaugeVms 0x402600: restarts the scripts (keeping pos), places 人/妖 from the gauge limits, re-sends the stored interrupt.
// AnmFile_StartScript does not zero pos (05 §8), so G0/G3 keep theirs; min/max = gauge limits (09 §9.3).
// The restart of +0x1520 (script 9) is skipped: that VM is run but never drawn (12 §5.9 step 1).
pub fn gauge_setup(anm: &mut crate::anmmgr::AnmManager, min: i32, max: i32) {
    GAUGE.with_borrow_mut(|g| {
        if g.vms.is_empty() {
            g.vms = vec![AnmVm::default(); 5];
            g.markers = vec![AnmVm::default(); 4];
        }
        for (k, script) in [
            (G_BAR, 5),
            (G_YOUKAI, 7),
            (G_HUMAN, 6),
            (G_MARKER, 8),
            (G_GLYPH, 4),
        ] {
            anm.start_script(&mut g.vms[k], ANM_SLOT, script);
        }
        for m in g.markers.iter_mut() {
            anm.start_script(m, ANM_SLOT, MARKER_SCRIPT);
        }
        g.vms[G_HUMAN].pos[0] -= 56.0 * min as f32 / -10000.0;
        g.vms[G_YOUKAI].pos[0] += 56.0 * max as f32 / 10000.0;
        let id = g.id;
        g.vms[..4]
            .iter_mut()
            .for_each(|v| v.pending_interrupt = id as _);
    });
}

pub fn gauge_player(p: &std::rc::Rc<std::cell::RefCell<crate::player::Player>>) {
    GAUGE.with_borrow_mut(|g| g.player = std::rc::Rc::downgrade(p));
}

// Ascii_GaugeInterrupt 0x4070B0 (12 §5.9): 1 show, 2/3 fade out/in, 99 hide (catch-all label); sent to G0–G3.
pub fn gauge_interrupt(id: i32) {
    GAUGE.with_borrow_mut(|g| {
        g.id = id;
        g.vms
            .iter_mut()
            .take(4)
            .for_each(|v| v.pending_interrupt = id as _);
    });
}

// Player_UpdateGaugeFade 0x44D420 (12 §5.9), player pos in playfield coordinates.
// 12 §5.9 interrupts 2/3; the player-side writes to +0xE2AA4 / +0xE2AB0 / +0xE2AC0 are not modelled here.
pub fn gauge_fade(pos: [f32; 3]) {
    let cur = GAUGE.with_borrow(|g| g.id);
    if pos[1] < 400.0 {
        if cur == 2 {
            gauge_interrupt(3);
        }
    } else if cur != 2 && pos[0] < 160.0 {
        gauge_interrupt(2);
    } else if cur == 2 && pos[0] > 160.0 {
        gauge_interrupt(3);
    }
}

// Run every frame by Ascii_Calc, also while paused (12 §5.9, §5.10).
fn gauge_calc(anm: &mut crate::anmmgr::AnmManager) {
    GAUGE.with_borrow_mut(|g| {
        g.vms
            .iter_mut()
            .chain(g.markers.iter_mut())
            .filter(|v| v.script.is_some())
            .for_each(|v| {
                v.run(anm);
            })
    });
}

// ascii.anm script 10: sprite 157 plate, label 1 fade in over 30, label 2 fade out over 30 then hidden, −1 hidden.
const MARKER_SCRIPT: usize = 10;
const MARKER_SPRITE: i16 = 157;
const MARKER_BLINK_SPRITE: i16 = 158;

// 0x422BB0: interrupt to marker k (12 §5.10).
pub fn marker_interrupt(k: usize, id: u16) {
    GAUGE.with_borrow_mut(|g| {
        if let Some(m) = g.markers.get_mut(k) {
            m.pending_interrupt = id;
        }
    });
}

// Op 127 −1 / Enemy_Free of a boss: interrupt 2 and pos := (−999, −999, 0) (12 §5.10).
pub fn marker_release(k: usize) {
    marker_interrupt(k, 2);
    GAUGE.with_borrow_mut(|g| {
        if let Some(m) = g.markers.get_mut(k) {
            m.pos = [-999.0, -999.0, 0.0];
        }
    });
}

// Per boss-flag enemy in EnemyMgr_Calc after the hit flash (12 §5.10): pos (x, 472, 0) in screen coordinates.
pub fn marker_track(k: usize, x: f32, mode: u8) {
    GAUGE.with_borrow_mut(|g| {
        if let Some(m) = g.markers.get_mut(k) {
            m.pos = [x, 472.0, 0.0];
            g.marker_mode[k] = mode;
        }
    });
}

// Tail of Ascii_DrawStrings (prio 20, full-screen viewport, 12 §5.10); the colour is rewritten every frame.
fn markers_draw(anm: &mut crate::anmmgr::AnmManager, gfx: &mut crate::gfx::Gfx) {
    GAUGE.with_borrow_mut(|g| {
        let px = g
            .player
            .upgrade()
            .and_then(|p| p.try_borrow().ok().map(|p| p.pos[0]));
        let px = px.unwrap_or(0.0);
        let frame = g.frame;
        gfx.set_viewport(FULLSCREEN);
        for k in 0..g.markers.len() {
            let m = &mut g.markers[k];
            if !(56.0..=392.0).contains(&m.pos[0]) {
                continue;
            }
            let d = (m.pos[0] - 32.0 - px).abs();
            // Set directly, without SetSprite (both plates are 48×16).
            m.sprite_index = MARKER_SPRITE;
            let plain = 0xFFFFFF | (if d < 64.0 { (d + 96.0) as u32 } else { 160 }) << 24;
            let argb = match g.marker_mode[k] {
                1 => 0x80FF4040,
                n @ 2..=4 if frame % (32 >> n) == 0 => {
                    m.sprite_index = MARKER_BLINK_SPRITE;
                    0xFFFFFFFF
                }
                _ => plain,
            };
            set_rgb(m, argb);
            m.color[anmvm::A] = (argb >> 24) as u8;
            anm.draw_mode(gfx, m, 0);
        }
    });
}

// End of Ascii_DrawPopups 0x405420 (draw prio 14): bar, ends, marker, percent text and max point value.
// Nothing is drawn unless G0 is visible; g = gauge stats+0x22 (12 §5.9 draw steps 1–4).
fn gauge_draw(
    anm: &mut crate::anmmgr::AnmManager,
    gfx: &mut crate::gfx::Gfx,
    fallback: &crate::player::Stats,
) {
    GAUGE.with_borrow_mut(|g| {
        if g.vms.len() < 5 || !g.vms[G_BAR].visible() {
            return;
        }
        let player = g.player.upgrade();
        let borrowed = player.as_ref().and_then(|p| p.try_borrow().ok());
        let st = borrowed.as_ref().map_or(fallback, |p| &p.stats);
        let gv = st.gauge as i32;
        let (bx, by) = (g.vms[G_BAR].pos[0], g.vms[G_BAR].pos[1]);
        let m = &mut g.vms[G_MARKER];
        m.pos[0] = bx + 64.0 + gv as f32 * 56.0 / 10000.0;
        // Marker G3 with Draw2DRotNoSnap (12 §5.9 step 1).
        anm.draw_mode(gfx, m, 3);
        let rgb: u32 = if st.human_max() {
            0x7070FF
        } else if st.human() {
            0xB0B0FF
        } else if st.youkai_max() {
            0xFF7070
        } else if st.youkai() {
            0xFFB0B0
        } else {
            0xFFFFFF
        };
        // The zone colour becomes G0's colour (alpha kept) and G4's colour (12 §5.9 step 2).
        set_rgb(&mut g.vms[G_BAR], rgb);
        let colour = g.vms[G_BAR].color;
        for k in [G_BAR, G_HUMAN, G_YOUKAI] {
            anm.draw_mode(gfx, &mut g.vms[k], 0);
        }
        let my = g.vms[G_MARKER].pos[1];
        let vm = &mut g.vms[G_GLYPH];
        draw_percent(
            anm,
            gfx,
            vm,
            gv,
            bx + 64.0 + gv as f32 * 40.0 / 10000.0,
            my - 7.0,
            0.0,
            colour,
        );
        vm.color = colour;
        let mut stamp =
            |anm: &mut crate::anmmgr::AnmManager, glyph: i32, x: f32, y: f32, scale: f32| {
                vm.set_sprite(anm, 136 + glyph);
                vm.pos = [x, y, 0.0];
                vm.scale = [scale, scale];
                anm.draw_mode(gfx, vm, 0);
            };
        // Max point value stats+0x24 (12 §5.9 step 4): a digit is drawn once the running digit sum is ≠ 0.
        let (mut x, y) = (bx + 62.0 - 14.0, by + 3.0 + 8.0);
        let (mut div, mut sum) = (10_000_000, 0);
        while div > 0 {
            let d = st.max_point / div % 10;
            sum += d;
            if sum != 0 {
                stamp(anm, d, x, y, 1.0);
                x += 7.0;
            }
            div /= 10;
        }
        vm.scale = [1.0, 1.0];
    });
}

// Ascii_DrawGaugePercent 0x405E10 (12 §5.9): "%d.%02d%%" of value/100 with glyph VM sprites 136 + (0–9, 10 '%',
// 11 '.', 12 '-'), decimals at 0.8×, the integer part centred around x = cx.
#[allow(clippy::too_many_arguments)]
pub fn draw_percent(
    anm: &mut crate::anmmgr::AnmManager,
    gfx: &mut crate::gfx::Gfx,
    vm: &mut AnmVm,
    value: i32,
    cx: f32,
    cy: f32,
    z: f32,
    colour: [u8; 4],
) {
    vm.color = colour;
    let a = value.abs();
    let n = 4
        + (value < 0) as i32
        + if a < 1000 {
            1
        } else if a < 10000 {
            2
        } else {
            3
        };
    let mut x = cx - (3.5 * n as f32 - 3.5 - 4.0);
    let mut seq: Vec<(i32, f32, bool)> = Vec::new();
    if value < 0 {
        seq.push((12, 7.0, false));
    }
    let (int, frac): (Vec<i32>, [i32; 2]) = if a < 1000 {
        (vec![a / 100], [a % 100 / 10, a % 10])
    } else if a < 10000 {
        (vec![a / 1000, a % 1000 / 100], [a % 100 / 10, a % 10])
    } else {
        (vec![1, 0, 0], [0, 0])
    };
    seq.extend(int.into_iter().map(|d| (d, 7.0, false)));
    seq.push((11, 5.0, false));
    seq.push((frac[0], 5.0, true));
    seq.push((frac[1], 7.0, true));
    seq.push((10, 0.0, false));
    for (glyph, adv, small) in seq {
        let (yy, sc) = if small { (cy + 2.0, 0.8) } else { (cy, 1.0) };
        vm.set_sprite(anm, 136 + glyph);
        vm.pos = [x, yy, z];
        vm.scale = [sc, sc];
        anm.draw_mode(gfx, vm, 0);
        x += adv;
    }
    vm.scale = [1.0, 1.0];
}

// Width of a one-line string: bytes × advance × scale (12 §2.9).
pub fn text_width(s: &str, advance: f32, scale: f32) -> f32 {
    s.len() as f32 * advance * scale
}

// Sets R, G, B of the VM colour from 0x..RRGGBB; alpha is untouched.
fn set_rgb(vm: &mut AnmVm, argb: u32) {
    vm.color[anmvm::R] = (argb >> 16) as u8;
    vm.color[anmvm::G] = (argb >> 8) as u8;
    vm.color[anmvm::B] = argb as u8;
}

// Pause menu (12 §3); VM i runs ascii.anm script 12 + i.
// PauseMenu_Update 0x4037B0 state machine (12 §3 table); `shown` = interrupt 1 (show) / 2 (hide) last sent per VM.
#[derive(Default)]
struct PauseMenu {
    state: u8,
    frame: u32,
    vms: Vec<AnmVm>,
    shown: [bool; 10],
    // Capture VM +0x1A70 (capture.anm) and the pending AnmManager_QueueCapture request (12 §3).
    cap_vm: AnmVm,
    capture: bool,
}

const P_TITLE: usize = 0;
const P_ITEMS: [usize; 3] = [1, 2, 3];
const P_REALLY: usize = 4;
const P_YES: usize = 5;
const P_NO: usize = 6;
const P_DIFF: usize = 7;
const P_PRACTICE: usize = 8;
const P_SLOW: usize = 9;

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

    // Input is read from frame 4 of each state; confirms sfx 10, cursor moves sfx 0 (12 §3).
    fn update(
        &mut self,
        ctx_anm: &mut crate::anmmgr::AnmManager,
        g: &mut Game,
        inp: &Input,
        audio: &mut crate::audio::Audio,
    ) {
        let replay = g.replay();
        if self.state == 0 {
            // All ten scripts start; then VM 8 (practice label) is hidden unless practice and VM 9 (slow label)
            // unless the gameplay copy's slow byte ≠ 0 (12 §3, 0x406700 clears flag bit 0).
            self.vms = (12..22)
                .map(|s| {
                    let mut vm = AnmVm::default();
                    ctx_anm.set_script(&mut vm, ANM_SLOT, s);
                    vm
                })
                .collect();
            if !g.practice() {
                self.vms[P_PRACTICE].flags &= !anmvm::F_VISIBLE;
            }
            if g.game_slow() == 0 {
                self.vms[P_SLOW].flags &= !anmvm::F_VISIBLE;
            }
            self.shown = [false; 10];
            start_capture(ctx_anm, &mut self.cap_vm);
            self.capture = true;
            let diff_vm = &mut self.vms[P_DIFF];
            diff_vm.set_sprite(ctx_anm, difficulty_sprite(g));
            for k in [P_TITLE, 1, 2, P_DIFF] {
                self.show(k, true);
            }
            if !replay {
                self.show(3, true);
            }
            if g.practice() {
                self.show(P_PRACTICE, true);
            }
            // Visible without an interrupt (dispatch would set it visible); the close interrupts reach it (12 §3).
            self.shown[P_SLOW] = g.game_slow() != 0;
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
            // Shortcut keys ESC / Q / R (R not in replay) (12 §3).
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
                        9 => {
                            // State 9: Result_Register(2) (save) and scene 1 (12 §3).
                            g.save_score(&mut ctx_anm.rng, true);
                            g.request(gamemgr::SCENE_MENU)
                        }
                        _ => {
                            // State 10: spell practice / practice / Extra reload the stage (scene 11) after
                            // Hud_StageTransition, else scene 10 (12 §3).
                            if g.practice() || g.spell_practice() || g.difficulty >= 4 {
                                // Music_Stop outside spell practice or for a card whose record flag is 1; else
                                // "UnPause" and the music keeps playing (12 §3 state 10).
                                if !g.spell_practice() || gamemgr::spell_bgm_restarts(g.card) {
                                    audio.stop_bgm();
                                } else {
                                    // Command 7 "UnPause", then `Bgm_StartFadeIn(2.0)`: 120 frames from −10000.
                                    audio.pause_bgm(false);
                                    audio.start_fade_in(2.0);
                                }
                                stage_transition(ctx_anm);
                                g.request(gamemgr::SCENE_RELOAD)
                            } else {
                                g.request(gamemgr::SCENE_RETRY)
                            }
                        }
                    }
                    g.paused = 0;
                    g.mark_play_time();
                    self.state = 0;
                    self.vms.clear();
                    self.cap_vm = AnmVm::default();
                    return;
                }
            }
            _ => {}
        }
        // Selected 0xFFFFFFFF, others 0xFF505050; confirm screen selected 0xFFFF8080 (12 §3).
        for (i, &k) in P_ITEMS.iter().enumerate() {
            set_rgb(
                &mut self.vms[k],
                if self.state as usize == i + 1 {
                    0xFFFFFFFF
                } else {
                    0xFF505050
                },
            );
        }
        let yes_sel = matches!(self.state, 5 | 7);
        set_rgb(
            &mut self.vms[P_YES],
            if yes_sel { 0xFFFF8080 } else { 0xFF505050 },
        );
        set_rgb(
            &mut self.vms[P_NO],
            if !yes_sel { 0xFFFF8080 } else { 0xFF505050 },
        );
        for vm in self
            .vms
            .iter_mut()
            .chain(std::iter::once(&mut self.cap_vm))
            .filter(|v| v.script.is_some())
        {
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

// Difficulty sprite of the pause menu VM +0x1284 and the HUD (12 §3, §5.1): 0x120 only for spell-practice ids
// ≥ 205, else 0x11B + difficulty (in spell practice the card's).
pub(crate) fn difficulty_sprite(g: &Game) -> i32 {
    if g.spell_practice() && g.card >= 205 {
        0x120
    } else {
        0x11B + g.difficulty.clamp(0, 4)
    }
}

// Capture VM: capture.anm script 0 (sprite 0, scale 3 × 3.5 = 384×448), then pos := (32, 16, 0) (12 §3).
fn start_capture(anm: &mut crate::anmmgr::AnmManager, vm: &mut AnmVm) {
    *vm = AnmVm::default();
    anm.set_script(vm, CAPTURE_SLOT, 0);
    vm.pos = [32.0, 16.0, 0.0];
}

// AnmManager_QueueCapture 0x406A30 (12 §3): playfield (32, 16, 384, 448) into slot 3 entry 0 at the VM's sprite rect
// (u0, v0 in texture pixels, width, height, truncated). The port copies at once from the last presented frame, so no
// request is ever pending and the "close at once" failure path cannot happen.
pub fn apply_capture(anm: &crate::anmmgr::AnmManager, gfx: &mut crate::gfx::Gfx, vm: &AnmVm) {
    if let Some(sp) = anm.sprite(CAPTURE_SLOT, vm.sprite_index.max(0) as usize) {
        capture_into(gfx, sp);
    }
}

fn capture_into(gfx: &mut crate::gfx::Gfx, sp: &crate::anmmgr::SlotSprite) {
    let t = gfx.texture(sp.tex);
    let to = [
        (sp.uv[0] * t.width as f32) as u32,
        (sp.uv[1] * t.height as f32) as u32,
    ];
    let tex = sp.tex;
    gfx.capture_rect(tex, [32, 16], [384, 448], to, [sp.w as u32, sp.h as u32]);
}

// AnmManager_QueueCapture 0x406A30 (12 §3): stores the request only if none is pending (else −1); the destination is
// the VM's sprite rect at queue time. Applied by `apply_pending_captures` after the next Present.
pub fn queue_capture(anm: &mut crate::anmmgr::AnmManager, vm: &AnmVm) -> i32 {
    if anm.capture_req.is_some() {
        return -1;
    }
    let Some(&sp) = anm.sprite(CAPTURE_SLOT, vm.sprite_index.max(0) as usize) else {
        return -1;
    };
    anm.capture_req = Some(sp);
    0
}

// Hud_StageTransition on the registered game's HUD (pause / continue retry paths, 12 §3).
fn stage_transition(anm: &mut crate::anmmgr::AnmManager) {
    if let Some(h) = gamemgr::current().and_then(|m| m.borrow().hud.clone()) {
        h.borrow_mut().stage_transition(anm);
    }
}

// AnmManager_ApplyPendingCaptures 0x443B60 (12 §3): right after Present, the playfield (32, 16, 384, 448) is copied.
pub fn apply_pending_captures(anm: &mut crate::anmmgr::AnmManager, gfx: &mut crate::gfx::Gfx) {
    if let Some(sp) = anm.capture_req.take() {
        capture_into(gfx, &sp);
    }
}

// Continue / game-over menu (11 §4.2, 12 §4): ascii scripts 22–24 and times.anm script 1 (night clock).
// ContinueMenu_Update 0x404890 (12 §4); phase 1 = no, 2 = yes in Extra / spell practice, 3 = continue (11 §4.2).
#[derive(Default)]
struct ContinueMenu {
    open: bool,
    yes: bool,
    frame: u32,
    phase: u8,
    phase_frame: u32,
    vms: Vec<AnmVm>,
    clock_vm: AnmVm,
    // Capture VM menu +0xFE0 and the pending capture request (12 §4).
    cap_vm: AnmVm,
    capture: bool,
}

// 0x404890 early exits (11 §4.2, RE-Q51): game_over := 0 and displayed score := score, no sound / VMs / capture /
// play time; GameMgr_Calc later in the same pass then runs one more gameplay frame.
fn early_exit(g: &mut Game, scene: i32) {
    g.game_over = 0;
    if let Some(gm) = gamemgr::current() {
        let gm = gm.borrow();
        let st = &mut gm.player.borrow_mut().stats;
        st.score_disp = st.score10;
    }
    g.request(scene);
}

impl ContinueMenu {
    fn update(
        &mut self,
        ctx_anm: &mut crate::anmmgr::AnmManager,
        g: &mut Game,
        inp: &Input,
        audio: &mut crate::audio::Audio,
    ) {
        if !self.open {
            // First call, on the calc after game_over was written (12 §4): state 0 here, then the state-2/1 body
            // below in the same call.
            *self = ContinueMenu {
                open: true,
                ..ContinueMenu::default()
            };
            log::info!("continue menu opened");
            if g.practice() && !g.spell_practice() {
                early_exit(g, gamemgr::SCENE_RESULT);
                return;
            }
            if g.replay() {
                early_exit(g, gamemgr::SCENE_REPLAY_END);
                return;
            }
            if g.difficulty < 4 && !g.spell_practice() && (g.clock > 10 || g.stage % 9 == 7) {
                g.flags &= !gamemgr::F_ENDING_OK;
                early_exit(g, gamemgr::SCENE_ENDING);
                return;
            }
            // Play time is added only when the prompt opens, not on the early exits (15 §2.9).
            g.add_play_time();
            // Music on opening (12 §4): spell practice with a non-restarting card dips to −10 dB and keeps
            // playing; otherwise command 6 "Pause".
            if g.spell_practice() && !gamemgr::spell_bgm_restarts(g.card) {
                audio.start_dip(1.0);
            } else {
                audio.pause_bgm(true);
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
            start_capture(ctx_anm, &mut self.cap_vm);
            self.capture = true;
            // Default "No"; "Yes" only in spell practice with spell flag bit 9 (captured) clear (12 §4).
            let captured = gamemgr::current()
                .and_then(|gm| gm.borrow().enemies.clone())
                .is_some_and(|e| e.borrow().g.spell_captured);
            self.yes = g.spell_practice() && !captured;
        }
        // Input from frame 30 (cursor on "No") or frame 4 (cursor on "Yes") (12 §4).
        let ready = self.frame
            >= if self.yes && g.spell_practice() {
                4
            } else {
                30
            };
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
                if self.phase == 1 {
                    g.request(gamemgr::SCENE_RESULT);
                } else {
                    // "Yes" in spell practice / Extra (12 §4 "Music"): `Music_Stop`, or for a non-restarting
                    // spell-practice card game_over := 0, "UnPause" and `Bgm_StartFadeInFromDip(1.0)`; then
                    // Hud_StageTransition and scene 11.
                    if !g.spell_practice() || gamemgr::spell_bgm_restarts(g.card) {
                        audio.stop_bgm();
                    } else {
                        g.game_over = 0;
                        audio.pause_bgm(false);
                        audio.start_fade_in_from_dip(1.0);
                    }
                    stage_transition(ctx_anm);
                    g.request(gamemgr::SCENE_RELOAD);
                }
                self.open = false;
                return;
            }
            3 => {
                // The night clock advances by one at frame 15 of a continue (12 §4).
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
                    g.mark_play_time();
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
        for vm in self
            .vms
            .iter_mut()
            .chain([&mut self.clock_vm, &mut self.cap_vm])
            .filter(|v| v.script.is_some())
        {
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
            ctx.anm
                .load(&mut ctx.gfx, &ctx.archive, ANM_SLOT, "ascii.anm")?;
            ctx.anm
                .load(&mut ctx.gfx, &ctx.archive, CAPTURE_SLOT, "capture.anm")?;
            let l = crate::player::GaugeLimits::default();
            gauge_setup(&mut ctx.anm, l.min, l.max);
        }
        Ok(())
    }

    fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        // Supervisor_Init registers the manager on the boot loader thread, so the boot calc passes never reach
        // Ascii_Calc and +0x8284 equals the main-menu frame number (12 §5.10).
        if ctx.game.scene == crate::gamemgr::SCENE_BOOT {
            return Ret::Next;
        }
        let inp = ctx.input;
        gauge_calc(&mut ctx.anm);
        // The pause menu runs while paused, the continue menu while game_over; popups only otherwise (12 §3, 13 §4).
        if ctx.game.paused != 0 {
            // 11 §2: paused = 2 once the menu was drawn (GameMgr_Calc opened it after this calc).
            ctx.game.paused = 2;
            self.pause
                .update(&mut ctx.anm, &mut ctx.game, &inp, &mut ctx.audio);
            if std::mem::take(&mut self.pause.capture) {
                apply_capture(&ctx.anm, &mut ctx.gfx, &self.pause.cap_vm);
            }
        } else if ctx.game.game_over != 0 {
            self.cont
                .update(&mut ctx.anm, &mut ctx.game, &inp, &mut ctx.audio);
            if std::mem::take(&mut self.cont.capture) {
                apply_capture(&ctx.anm, &mut ctx.gfx, &self.cont.cap_vm);
            }
        } else {
            self.cont.open = false;
            self.pause.state = 0;
            if !crate::effect::with(|e| e.frozen) {
                popups_calc(ctx.anm.speed);
            }
        }
        // +0x8284 is incremented at the very end of Ascii_Calc (12 §5.10).
        GAUGE.with_borrow_mut(|g| g.frame = g.frame.wrapping_add(1));
        Ret::Next
    }

    fn draw(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        let high = tag == 1;
        // Pause draw 0x404750 (12 §3): playfield viewport; the capture VM (temporary copy, z-write off) while
        // the state ≠ 0; then menu VMs 0..9, all Draw2DSnap.
        if high && ctx.game.paused != 0 {
            ctx.gfx.set_viewport(crate::gfx::PLAYFIELD);
            if self.pause.state != 0 && self.pause.cap_vm.script.is_some() {
                let mut v = self.pause.cap_vm.clone();
                v.flags |= anmvm::F_ZWRITE_OFF;
                ctx.anm.draw_mode(&mut ctx.gfx, &mut v, 0);
            }
            let sel: Vec<bool> = (0..self.pause.vms.len())
                .map(|k| self.pause.selected(k))
                .collect();
            for (vm, sel) in self.pause.vms.iter_mut().zip(sel) {
                let pos = vm.pos;
                if sel {
                    vm.pos = [pos[0] - 4.0, pos[1] - 4.0, pos[2]];
                }
                ctx.anm.draw_mode(&mut ctx.gfx, vm, 0);
                vm.pos = pos;
            }
        }
        // ContinueMenu_Draw 0x4052B0 (12 §4): playfield viewport, capture VM, then VMs 0..3 (clock VM 3 only in a
        // normal game below Extra), all Draw2DSnap. The port's menu is past state 0 after its first calc.
        if high && ctx.game.game_over != 0 && self.cont.open {
            ctx.gfx.set_viewport(crate::gfx::PLAYFIELD);
            let g = &ctx.game;
            let clock = g.difficulty < 4 && !g.spell_practice();
            let c = &mut self.cont;
            for vm in [&mut c.cap_vm]
                .into_iter()
                .chain(c.vms.iter_mut())
                .chain(clock.then_some(&mut c.clock_vm))
            {
                ctx.anm.draw_mode(&mut ctx.gfx, vm, 0);
            }
        }
        if !high {
            popups_draw(&mut ctx.anm, &mut ctx.gfx);
            // The gauge readout is drawn after the shake offset is zeroed (12 §5.9 draw).
            ctx.anm.screen_offset = [0.0; 2];
            gauge_draw(&mut ctx.anm, &mut ctx.gfx, &ctx.game.stats);
        }
        let mut q = std::mem::take(&mut ctx.ascii);
        q.draw(&ctx.anm, &mut ctx.gfx, high);
        ctx.ascii = q;
        if high {
            markers_draw(&mut ctx.anm, &mut ctx.gfx);
        }
        Ret::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 12 §4: game_over is written in the player calc (prio 9); the next ASCII calc (prio 1) opens the menu, does
    // state 0 and the state-2 body in that one call, so after it the menu counts frame 1 with the cursor on "No".
    #[test]
    fn continue_menu_opens_next_calc() {
        let mut anm = crate::anmmgr::AnmManager::default();
        let mut g = Game::default();
        let mut p = crate::player::Player::default();
        p.game_over = true;
        p.store_game_over(&mut g);
        assert!(g.game_over == 1 && !p.game_over);
        let mut m = ContinueMenu::default();
        let mut audio =
            crate::audio::Audio::new(crate::assets::Source::native(std::env::temp_dir()), &[]);
        m.update(&mut anm, &mut g, &Input::default(), &mut audio);
        assert!(m.open && !m.yes && m.capture);
        assert_eq!((m.phase, m.frame, m.phase_frame), (0, 1, 1));
        assert_eq!(g.requested, gamemgr::SCENE_BOOT);
    }

    // 11 §4.2 / 15 §2.9: the replay early exit clears game_over, requests scene 7, opens nothing, adds no play time.
    #[test]
    fn continue_menu_replay_early_exit() {
        let mut anm = crate::anmmgr::AnmManager::default();
        let mut g = Game::default();
        g.flags |= gamemgr::F_REPLAY;
        g.game_over = 1;
        g.mark_play_time();
        let mut m = ContinueMenu::default();
        let mut audio =
            crate::audio::Audio::new(crate::assets::Source::native(std::env::temp_dir()), &[]);
        m.update(&mut anm, &mut g, &Input::default(), &mut audio);
        assert_eq!((g.game_over, g.requested), (0, gamemgr::SCENE_REPLAY_END));
        assert!(g.play_clock.is_some() && !m.capture && m.vms.is_empty());
    }

    #[test]
    fn popup_digits_and_life() {
        assert_eq!(digits(1230), vec![0, 3, 2, 1]);
        assert_eq!(digits(0), vec![0]);
        assert_eq!(digits(-1), vec![10]);
        clear_popups();
        popup([100.0, 200.0, 0.0], 500, 0xFFFFFFFF);
        for _ in 0..60 {
            popups_calc(1.0);
        }
        let y = POPUPS.with_borrow(|p| p.slots[0].as_ref().map(|q| q.pos[1]));
        assert_eq!(y, Some(216.0 - 30.0));
        popups_calc(1.0);
        assert!(POPUPS.with_borrow(|p| p.slots[0].is_none()));
    }

    // 13 §4: fade by squared distance, screen pos vs playfield player (no origin correction).
    #[test]
    fn popup_fade_and_large_count() {
        assert_eq!(popup_alpha([132.0, 216.0], [100.0, 216.0, 0.0]), 0x50);
        assert_eq!(popup_alpha([164.0, 216.0], [100.0, 216.0, 0.0]), 0xD0);
        assert_eq!(
            popup_alpha([148.0, 216.0], [100.0, 216.0, 0.0]),
            (0x50 + 1280 * 128 / 3072) as u8
        );
        assert_eq!(large_glyphs(1, 0).len(), 2);
        assert_eq!(large_glyphs(12, 3), vec![0xF, 3, 0xE, 2, 1, 0xD]);
    }

    // 13 §4.1: left/right rects only when inside the playfield; top/bottom between the clamped x0/x1.
    #[test]
    fn darkness_mask_rects() {
        let r = darkness_rects([192.0, 300.0], 50.0, [0.0, 0.0]);
        assert_eq!(
            r,
            vec![
                [32.0, 16.0, 174.0, 464.0],
                [274.0, 16.0, 416.0, 464.0],
                [174.0, 16.0, 274.0, 266.0],
                [174.0, 366.0, 274.0, 464.0],
            ]
        );
        // Golden stage-2 card radius 288: only the top rect (clamped) and nothing that is out of range.
        let r = darkness_rects([258.0, 400.0], 288.0, [0.0, 0.0]);
        assert_eq!(r, vec![[32.0, 16.0, 416.0, 128.0]]);
        // The shake offset moves the hole.
        let r = darkness_rects([192.0, 300.0], 50.0, [2.0, -3.0]);
        assert_eq!(r[0], [32.0, 16.0, 176.0, 464.0]);
        assert_eq!(r[2], [176.0, 16.0, 276.0, 263.0]);
        set_darkness(255, 288.0);
        clear_darkness();
        assert_eq!(darkness_alpha(), 0);
    }
}
