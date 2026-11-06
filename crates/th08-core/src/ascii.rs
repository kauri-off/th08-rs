//! ASCII manager 0x4CCE20 calc / draw step: string queue, popups, gauge, markers, pause / continue (12 §2.9, §3–§5; 13 §4).

use crate::anmvm::{self, AnmVm};
use crate::drawlist::Recorder;
use crate::game::Ctx;
use crate::gamemgr::{self, Game};
use crate::input::{self, Input};
use crate::task::{self, Chain, Ret, Task};
use crate::types::{SceneId, lo8};
use anyhow::Result;

// ascii.anm slot 1, capture.anm slot 3 (12 §3); calc prio 1, draw prio 14 / 20 (12 §5.7).
pub const ANM_SLOT: usize = 1;
pub const CAPTURE_SLOT: usize = 3;
pub const CALC_PRIO: u16 = 1;
pub const DRAW_LOW_PRIO: u16 = 14;
pub const DRAW_HIGH_PRIO: u16 = 20;
// `ANM_SLOT` as the VM's i16 +anm_slot field.
pub(crate) const ANM_SLOT_I16: i16 = 1;

// Index stored into an i16 field (sprite, ANM slot, card id; `mov word`): same bits as `as i16`, all < 0x8000.
#[expect(
    clippy::cast_possible_wrap,
    reason = "sprite, slot and card indices < 0x8000"
)]
pub(crate) const fn short(idx: usize) -> i16 {
    idx as i16
}

// Low-word store into a VM's pending interrupt (`mov word`): same bits as `as u16`.
pub(crate) const fn lo16(v: i32) -> u16 {
    let b = v.to_le_bytes();
    u16::from_le_bytes([b[0], b[1]])
}

// Index from a signed cursor / table value: exactly `as usize` (a negative one sign-extends and misses every table).
#[expect(
    clippy::cast_sign_loss,
    reason = "the original's signed index, `as usize` kept"
)]
pub(crate) const fn uidx(v: i32) -> usize {
    v as usize
}

// A list position / count as the menus' int fields hold it (all < 256).
#[expect(clippy::cast_possible_wrap, reason = "positions and counts < 256")]
pub(crate) const fn int(v: usize) -> i32 {
    v as i32
}

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
#[derive(Clone)]
pub struct Queue {
    items: Vec<Text>,
    // Current queue colour 0x4D5088 (RGBA here); only the callers that take "the current colour" read it (12 §1, §5.3).
    pub color: [u8; 4],
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            color: [255; 4],
        }
    }
}

impl Queue {
    pub fn push(&mut self, t: Text) {
        if self.items.len() < 256 {
            self.items.push(t);
        }
    }

    // A string in the current queue colour (12 §2.9 state copy).
    pub fn text(&self, x: f32, y: f32, s: impl Into<String>) -> Text {
        Text::new(x, y, s).color(self.color)
    }

    // Draw-phase step of Ascii_DrawStrings: the pass's entries leave the queue in queue order; `pass` None takes every
    // entry (the loading prompt's direct call, 12 §1).
    pub fn take(&mut self, pass: Option<bool>) -> Vec<Text> {
        let (out, keep) = std::mem::take(&mut self.items)
            .into_iter()
            .partition(|t| pass.is_none_or(|h| t.high == h));
        self.items = keep;
        out
    }
}

// Supervisor_DrawLoading 0x445BD4 "Press Shot Button" (12 §1): state (`Game::load_prompt`) += 1 per draw while > 1;
// prompt from c > 4, alpha written into the queue colour and left there, then the whole queue drawn and emptied.
// Draw-phase step: returns the entries to emit (`emit_texts`), empty when the prompt is not shown.
pub fn loading_prompt(q: &mut Queue, state: &mut u32) -> Vec<Text> {
    let Some(a) = prompt_step(state) else {
        return Vec::new();
    };
    q.color[anmvm::A] = a;
    let t = q.text(288.0, 454.0, "Press Shot Button").scale(0.5);
    q.push(t);
    q.take(None)
}

// The counter part of the prompt (12 §1): alpha when this draw shows it; c > 64 → state 5 after the draw (period 60).
fn prompt_step(state: &mut u32) -> Option<u8> {
    if *state <= 1 {
        return None;
    }
    *state += 1;
    let c = state.cast_signed();
    if c <= 4 {
        return None;
    }
    if c > 64 {
        *state = 5;
    }
    Some(prompt_alpha(c))
}

// 12 §1: c < 35 → 255 − trunc((c − 5)·128/30), else 255 − trunc((65 − c)·128/30), as a byte.
fn prompt_alpha(c: i32) -> u8 {
    let x = if c < 35 {
        (c - 5) * 128 / 30
    } else {
        (65 - c) * 128 / 30
    };
    lo8(255 - x)
}

// Small popups: slots 0–719 plus 3 special slots 720–722, life 60; large popups: 128 slots, life 90 (13 §4).
pub const POPUP_SLOTS: usize = 720;
const POPUP_LIFE: i32 = 60;
const LARGE_SLOTS: usize = 128;
const LARGE_LIFE: i32 = 90;

// Small score popup (13 §4): digits least significant first, glyph 10 = the "PowerUp" sign.
#[derive(Clone, Default, Debug)]
pub struct Popup {
    pub digits: Vec<u8>,
    pub pos: [f32; 2],
    // D3DCOLOR 0xAARRGGBB; only RGB is used, the alpha byte is replaced by the distance fade.
    pub colour: u32,
    // Timer ticked at game speed (13 §4); every test reads the integer cur.
    pub age: anmvm::Timer,
}

// Large popup (13 §4): glyphs LSB first, pos in screen coordinates, scale copied from the manager at spawn.
#[derive(Clone, Debug)]
pub struct LargePopup {
    pub glyphs: Vec<u8>,
    pub pos: [f32; 2],
    pub colour: u32,
    pub scale: f32,
    pub age: anmvm::Timer,
}

// Popup arrays at Ascii +0xB4DC (small) and +0x15304 (large) with their round-robin cursors (+0x8298 for large).
#[derive(Clone)]
pub struct Popups {
    pub slots: Vec<Option<Popup>>,
    next: usize,
    special: usize,
    pub large: Vec<Option<LargePopup>>,
    large_next: usize,
}

impl Default for Popups {
    fn default() -> Self {
        Popups {
            slots: vec![None; POPUP_SLOTS + 3],
            next: 0,
            special: 0,
            large: vec![None; LARGE_SLOTS],
            large_next: 0,
        }
    }
}

// Ascii manager state that stage subsystems write mid-frame: popups (13 §4), darkness mask (13 §4.1), gauge readout
// and boss markers (12 §5.9–§5.10). Reached through `Ctx::overlays` / `Sys::ov`.
#[derive(Clone, Default)]
pub struct Overlays {
    pub popups: Popups,
    pub(crate) darkness: Darkness,
    pub(crate) gauge: Gauge,
}

// At most 12 digits, least significant first; a negative value is the single glyph 10 (13 §4).
fn digits(value: i32) -> Vec<u8> {
    if value < 0 {
        return vec![10];
    }
    let mut v = value.cast_unsigned();
    let mut d = Vec::new();
    loop {
        d.push((v % 10) as u8);
        v /= 10;
        if v == 0 || d.len() == 12 {
            return d;
        }
    }
}

impl Overlays {
    // Popup_Number 0x403200: slots 0–719 round robin; pos in playfield coordinates; a negative value shows "PowerUp".
    pub fn popup(&mut self, pos: [f32; 3], value: i32, colour: u32) {
        {
            let p = &mut self.popups;
            let i = p.next;
            p.next = (p.next + 1) % POPUP_SLOTS;
            p.slots[i] = Some(Popup {
                digits: digits(value),
                pos: [pos[0] + 32.0, pos[1] + 16.0],
                colour,
                age: anmvm::Timer::default(),
            });
        }
    }

    // 0x403330: the three extra slots 720–722, round robin.
    pub fn popup_special(&mut self, pos: [f32; 3], value: i32, colour: u32) {
        {
            let p = &mut self.popups;
            let i = POPUP_SLOTS + p.special;
            p.special = (p.special + 1) % 3;
            p.slots[i] = Some(Popup {
                digits: digits(value),
                pos: [pos[0] + 32.0, pos[1] + 16.0],
                colour,
                age: anmvm::Timer::default(),
            });
        }
    }
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

impl Overlays {
    // Popup_Large 0x403460 (life 90); `pos` in playfield coordinates.
    pub fn popup_large(&mut self, pos: [f32; 3], p3: i32, p4: i32, colour: u32, scale: f32) {
        {
            let p = &mut self.popups;
            let i = p.large_next;
            p.large_next = (i + 1) % LARGE_SLOTS;
            p.large[i] = Some(LargePopup {
                glyphs: large_glyphs(p3, p4),
                pos: [pos[0] + 32.0, pos[1] + 16.0],
                colour,
                scale,
                age: anmvm::Timer::default(),
            });
        }
    }

    // Popup_LargeStatic 0x403600: starts at age 88 (a two-frame display), x += count · 3.5 (count includes the '+').
    pub fn popup_large_static(&mut self, pos: [f32; 2], p3: i32, p4: i32, colour: u32) {
        let glyphs = large_glyphs(p3, p4);
        let x = pos[0] + 32.0 + glyphs.len() as f32 * 3.5;
        {
            let p = &mut self.popups;
            let i = p.large_next;
            p.large_next = (i + 1) % LARGE_SLOTS;
            p.large[i] = Some(LargePopup {
                glyphs,
                pos: [x, pos[1] + 16.0],
                colour,
                scale: 1.0,
                age: timer_at(88),
            });
        }
    }
}

fn timer_at(v: i32) -> anmvm::Timer {
    let mut t = anmvm::Timer::default();
    t.set(v);
    t
}

impl Popups {
    // Popup part of `Ascii_InitVms` 0x402450 (13 §4): all slots cleared, small (+0x8290) and special (+0x8294) cursors
    // := 0; the large cursor +0x8298 is kept across stages and games.
    pub fn clear(&mut self) {
        self.slots.iter_mut().for_each(|s| *s = None);
        self.large.iter_mut().for_each(|s| *s = None);
        self.next = 0;
        self.special = 0;
    }
}

// Ascii_Calc popup part (13 §4): small popups rise 0.5·gs per frame; large popups do not move.
fn popups_calc(p: &mut Popups, gs: f32) {
    {
        for s in &mut p.slots {
            if let Some(q) = s {
                q.pos[1] -= 0.5 * gs;
                q.age.tick(gs);
                // Timer_Greater(60) 0x4066C0 compares the integer cur: drawn with ages 0..60 (13 §4).
                if q.age.cur > POPUP_LIFE {
                    *s = None;
                }
            }
        }
        for s in &mut p.large {
            if let Some(q) = s {
                q.age.tick(gs);
                // Freed when cur > 90 (13 §4): a static popup issued at 88 is drawn at 89 and 90.
                if q.age.cur > LARGE_LIFE {
                    *s = None;
                }
            }
        }
    }
}

// Squared player distance d (truncated to int) → alpha 0x50 / ramp / 0xD0 (13 §4). The player is in playfield
// coordinates and the popup in screen coordinates; the origin is not subtracted (QUIRK).
pub(crate) fn popup_alpha(pos: [f32; 2], player: [f32; 3]) -> u8 {
    let (dx, dy) = (pos[0] - player[0], pos[1] - player[1]);
    let d = (dx * dx + dy * dy) as i32;
    if d <= 1024 {
        0x50
    } else if d <= 4096 {
        lo8(0x50 + (d - 1024) * 128 / 3072)
    } else {
        0xD0
    }
}

// Darkness mask (13 §4.1): alpha Ascii +0x16F08 (int, 0x4E3D28), radius +0x16F04 (f32, 0x4E3D24), hole VM +0x16F0C.
#[derive(Clone, Default)]
pub(crate) struct Darkness {
    pub(crate) alpha: i32,
    pub(crate) radius: f32,
    pub(crate) vm: AnmVm,
}

impl Overlays {
    // ECL native 0 (07 §12): alpha := int var 10000, radius := float var 10016.
    pub fn set_darkness(&mut self, alpha: i32, radius: f32) {
        (self.darkness.alpha, self.darkness.radius) = (alpha, radius);
    }

    // Alpha := 0 by `Spell_End`, `GameManager_StageStart` and `GameMgr_OnRemove` (13 §4.1); the radius is kept.
    pub fn clear_darkness(&mut self) {
        self.darkness.alpha = 0;
    }

    pub fn darkness_alpha(&self) -> i32 {
        self.darkness.alpha
    }
}

// The four black rects (x0, y0, x1, y1) around the player P (playfield) with radius r and shake offset s (13 §4.1).
pub(crate) fn darkness_rects(p: [f32; 2], r: f32, s: [f32; 2]) -> Vec<[f32; 4]> {
    let (left, right) = (p[0] + 32.0 - r + s[0], p[0] + 32.0 + r + s[0]);
    let (top, bottom) = (p[1] + 16.0 - r + s[1], p[1] + 16.0 + r + s[1]);
    let mut out = Vec::with_capacity(4);
    if left > 32.0 {
        out.push([32.0, 16.0, left, 464.0]);
    }
    if right < 416.0 {
        out.push([right, 16.0, 416.0, 464.0]);
    }
    let (x0, x1) = (
        crate::fmath::fmax(left, 32.0),
        crate::fmath::fmin(right, 416.0),
    );
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
// Step: the script set (runs the VM, RNG) and the VM writes; no draw precedes them that reads state.
fn darkness_step(
    d: &mut Darkness,
    anm: &mut crate::anmmgr::AnmManager,
    rng: &mut crate::rng::Rng,
    player: [f32; 3],
) {
    if d.alpha <= 0 {
        return;
    }
    anm.file_set_script(rng, &mut d.vm, crate::effect::SCRIPT_SLOT, 105);
    d.vm.scale = [d.radius / 63.0; 2];
    d.vm.pos = [player[0] + 32.0, player[1] + 16.0, player[2]];
    d.vm.color[anmvm::A] = lo8(d.alpha);
}

// Human/youkai gauge readout VMs (12 §5.9): bar, 人, 妖, marker, glyph stamp.
// VMs G0–G4 at ASCII manager +0x7EC.. and the stored interrupt +0x827C (12 §5.9).
#[derive(Clone, Default)]
pub(crate) struct Gauge {
    pub(crate) vms: Vec<AnmVm>,
    id: i32,
    // Boss "ENEMY" markers M_k at +0x17C4 + k·0x2A4, k = boss slot (12 §5.10).
    pub(crate) markers: Vec<AnmVm>,
    // +0x2254[slot]: 0 plain, 1 hit flash, 2–4 low-life blink (warning level + 1), written by EnemyMgr_Calc.
    marker_mode: [u8; 4],
    // +0x8284: incremented at the end of every Ascii_Calc, never reset, starts at 0 (12 §5.10).
    frame: u32,
}

impl Overlays {
    // Session tool (12 §5.10 slow boots): starts +0x8284 at the recording's boot offset; a live port keeps 0.
    pub fn set_frame_counter(&mut self, v: u32) {
        self.gauge.frame = v;
    }
}

pub(crate) const G_BAR: usize = 0;
pub(crate) const G_HUMAN: usize = 1;
pub(crate) const G_YOUKAI: usize = 2;
pub(crate) const G_MARKER: usize = 3;
pub(crate) const G_GLYPH: usize = 4;

impl Overlays {
    // Ascii_SetupGaugeVms 0x402600: restarts the scripts (keeping pos), places 人/妖 from the gauge limits, re-sends the stored interrupt.
    // AnmFile_StartScript does not zero pos (05 §8), so G0/G3 keep theirs; min/max = gauge limits (09 §9.3).
    // The restart of +0x1520 (script 9) is skipped: that VM is run but never drawn (12 §5.9 step 1).
    pub fn gauge_setup(
        &mut self,
        anm: &mut crate::anmmgr::AnmManager,
        rng: &mut crate::rng::Rng,
        min: i32,
        max: i32,
    ) {
        {
            let g = &mut self.gauge;
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
                anm.start_script(rng, &mut g.vms[k], ANM_SLOT, script);
            }
            for m in &mut g.markers {
                anm.start_script(rng, m, ANM_SLOT, MARKER_SCRIPT);
            }
            g.vms[G_HUMAN].pos[0] -= 56.0 * min as f32 / -10000.0;
            g.vms[G_YOUKAI].pos[0] += 56.0 * max as f32 / 10000.0;
            let id = g.id;
            g.vms[..4]
                .iter_mut()
                .for_each(|v| v.pending_interrupt = lo16(id));
        }
    }

    // Ascii_GaugeInterrupt 0x4070B0 (12 §5.9): 1 show, 2/3 fade out/in, 99 hide (catch-all label); sent to G0–G3.
    pub fn gauge_interrupt(&mut self, id: i32) {
        {
            let g = &mut self.gauge;
            g.id = id;
            g.vms
                .iter_mut()
                .take(4)
                .for_each(|v| v.pending_interrupt = lo16(id));
        }
    }

    // Player_UpdateGaugeFade 0x44D420 (12 §5.9), player pos in playfield coordinates.
    // 12 §5.9 interrupts 2/3; the player-side writes to +0xE2AA4 / +0xE2AB0 / +0xE2AC0 are not modelled here.
    pub fn gauge_fade(&mut self, pos: [f32; 3]) {
        let cur = self.gauge.id;
        if pos[1] < 400.0 {
            if cur == 2 {
                self.gauge_interrupt(3);
            }
        } else if cur != 2 && pos[0] < 160.0 {
            self.gauge_interrupt(2);
        } else if cur == 2 && pos[0] > 160.0 {
            self.gauge_interrupt(3);
        }
    }
}

// Run every frame by Ascii_Calc, also while paused (12 §5.9, §5.10).
fn gauge_calc(g: &mut Gauge, anm: &mut crate::anmmgr::AnmManager, rng: &mut crate::rng::Rng) {
    {
        g.vms
            .iter_mut()
            .chain(g.markers.iter_mut())
            .filter(|v| v.script.is_some())
            .for_each(|v| {
                v.run(anm, rng);
            });
    }
}

// ascii.anm script 10: sprite 157 plate, label 1 fade in over 30, label 2 fade out over 30 then hidden, −1 hidden.
const MARKER_SCRIPT: usize = 10;
const MARKER_SPRITE: i16 = 157;
const MARKER_BLINK_SPRITE: i16 = 158;

impl Overlays {
    // 0x422BB0: interrupt to marker k (12 §5.10).
    pub fn marker_interrupt(&mut self, k: usize, id: u16) {
        if let Some(m) = self.gauge.markers.get_mut(k) {
            m.pending_interrupt = id;
        }
    }

    // Op 127 −1 / Enemy_Free of a boss: interrupt 2 and pos := (−999, −999, 0) (12 §5.10).
    pub fn marker_release(&mut self, k: usize) {
        self.marker_interrupt(k, 2);
        if let Some(m) = self.gauge.markers.get_mut(k) {
            m.pos = [-999.0, -999.0, 0.0];
        }
    }

    // Per boss-flag enemy in EnemyMgr_Calc after the hit flash (12 §5.10): pos (x, 472, 0) in screen coordinates.
    pub fn marker_track(&mut self, k: usize, x: f32, mode: u8) {
        let g = &mut self.gauge;
        if let Some(m) = g.markers.get_mut(k) {
            m.pos = [x, 472.0, 0.0];
            g.marker_mode[k] = mode;
        }
    }
}

// Tail of Ascii_DrawStrings (prio 20, full-screen viewport, 12 §5.10); the colour is rewritten every frame.
// Step: sprite and colour of every marker in range (persistent VM writes).
// `player`: the running game's player (`ctx.w.player`), none outside a game.
fn markers_step(g: &mut Gauge, player: Option<&crate::player::Player>) {
    let px = player.map_or(0.0, |p| p.pos[0]);
    let frame = g.frame;
    for k in 0..g.markers.len() {
        let m = &mut g.markers[k];
        if !marker_shown(m) {
            continue;
        }
        let d = (m.pos[0] - 32.0 - px).abs();
        // Set directly, without SetSprite (both plates are 48×16).
        m.sprite_index = MARKER_SPRITE;
        let plain = 0x00FF_FFFF | marker_alpha(d) << 24;
        let argb = match g.marker_mode[k] {
            1 => 0x80FF_4040,
            n @ 2..=4 if frame.is_multiple_of(32 >> n) => {
                m.sprite_index = MARKER_BLINK_SPRITE;
                0xFFFF_FFFF
            }
            _ => plain,
        };
        set_rgb(m, argb);
        m.color[anmvm::A] = (argb >> 24) as u8;
    }
}

// Plain marker alpha (12 §5.10): 96 + the distance (ftol) below 64 px, else 160.
#[expect(clippy::cast_sign_loss, reason = "ftol of d + 96 ≥ 96")]
fn marker_alpha(d: f32) -> u32 {
    if d < 64.0 { (d + 96.0) as u32 } else { 160 }
}

pub(crate) fn marker_shown(m: &AnmVm) -> bool {
    (56.0..=392.0).contains(&m.pos[0])
}

// End of Ascii_DrawPopups 0x405420 (draw prio 14): bar, ends, marker, percent text and max point value.
// Nothing is drawn unless G0 is visible; g = gauge stats+0x22 (12 §5.9 draw steps 1–4).
// What the emit needs from the step: gauge value, percent position, zone colour, max point stamps (glyph, x, y).
#[derive(Clone)]
pub(crate) struct GaugeDraw {
    pub(crate) gv: i32,
    pub(crate) at: [f32; 2],
    pub(crate) colour: [u8; 4],
    pub(crate) stamps: Vec<(i32, f32, f32)>,
}

// Step: G3 x, G0 colour, and G4's sprite / pos / scale / colour as the last glyph left them.
fn gauge_step(
    g: &mut Gauge,
    anm: &crate::anmmgr::AnmManager,
    player: Option<&crate::player::Player>,
    fallback: &crate::player::Stats,
) -> Option<GaugeDraw> {
    if g.vms.len() < 5 || !g.vms[G_BAR].visible() {
        return None;
    }
    let st = player.map_or(fallback, |p| &p.stats);
    let gv = i32::from(st.gauge);
    let (bx, by) = (g.vms[G_BAR].pos[0], g.vms[G_BAR].pos[1]);
    g.vms[G_MARKER].pos[0] = bx + 64.0 + gv as f32 * 56.0 / 10000.0;
    let rgb: u32 = if st.human_max() {
        0x0070_70FF
    } else if st.human() {
        0x00B0_B0FF
    } else if st.youkai_max() {
        0x00FF_7070
    } else if st.youkai() {
        0x00FF_B0B0
    } else {
        0x00FF_FFFF
    };
    // The zone colour becomes G0's colour (alpha kept) and G4's colour (12 §5.9 step 2).
    set_rgb(&mut g.vms[G_BAR], rgb);
    let colour = g.vms[G_BAR].color;
    let at = [
        bx + 64.0 + gv as f32 * 40.0 / 10000.0,
        g.vms[G_MARKER].pos[1] - 7.0,
    ];
    // Max point value stats+0x24 (12 §5.9 step 4): a digit is drawn once the running digit sum is ≠ 0.
    let mut stamps = Vec::new();
    let (mut x, y) = (bx + 62.0 - 14.0, by + 3.0 + 8.0);
    let (mut div, mut sum) = (10_000_000, 0);
    while div > 0 {
        let d = st.max_point / div % 10;
        sum += d;
        if sum != 0 {
            stamps.push((d, x, y));
            x += 7.0;
        }
        div /= 10;
    }
    let draw = GaugeDraw {
        gv,
        at,
        colour,
        stamps,
    };
    gauge_glyphs(anm, &mut g.vms[G_GLYPH], &draw, |_| {});
    Some(draw)
}

// G4's glyph sequence: percent text, then the stamps; `each` sees each glyph (a no-op in the step, a draw in the emit).
pub(crate) fn gauge_glyphs(
    anm: &crate::anmmgr::AnmManager,
    vm: &mut AnmVm,
    d: &GaugeDraw,
    mut each: impl FnMut(&AnmVm),
) {
    percent_glyphs(anm, vm, d.gv, [d.at[0], d.at[1], 0.0], d.colour, &mut each);
    vm.color = d.colour;
    for &(glyph, x, y) in &d.stamps {
        vm.set_sprite(anm, 136 + glyph);
        vm.pos = [x, y, 0.0];
        vm.scale = [1.0, 1.0];
        each(vm);
    }
    vm.scale = [1.0, 1.0];
}

// Ascii_DrawGaugePercent 0x405E10 (12 §5.9): "%d.%02d%%" of value/100 with glyph VM sprites 136 + (0–9, 10 '%',
// 11 '.', 12 '-'), decimals at 0.8×, the integer part centred around x = at[0]. `each` sees the VM after each
// glyph's writes: a draw-phase step passes a no-op, an emit draws (on a copy: every field read is rewritten per glyph).
pub fn percent_glyphs(
    anm: &crate::anmmgr::AnmManager,
    vm: &mut AnmVm,
    value: i32,
    at: [f32; 3],
    colour: [u8; 4],
    mut each: impl FnMut(&AnmVm),
) {
    let [cx, cy, z] = at;
    vm.color = colour;
    let a = value.abs();
    let n = 4
        + i32::from(value < 0)
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
        each(vm);
        x += adv;
    }
    vm.scale = [1.0, 1.0];
}

// Width of a one-line string: bytes × advance × scale (12 §2.9).
pub fn text_width(s: &str, advance: f32, scale: f32) -> f32 {
    s.len() as f32 * advance * scale
}

// Sets R, G, B of the VM colour from 0x..RRGGBB; alpha is untouched.
pub(crate) fn set_rgb(vm: &mut AnmVm, argb: u32) {
    vm.color[anmvm::R] = (argb >> 16) as u8;
    vm.color[anmvm::G] = (argb >> 8) as u8;
    vm.color[anmvm::B] = argb as u8;
}

// Whole-dword store into VM +0x1F0, alpha included (12 §3, §4).
fn set_argb(vm: &mut AnmVm, argb: u32) {
    set_rgb(vm, argb);
    vm.color[anmvm::A] = (argb >> 24) as u8;
}

// Pause menu (12 §3); VM i runs ascii.anm script 12 + i.
// PauseMenu_Update 0x4037B0 state machine (12 §3 table); `shown` = interrupt 1 (show) / 2 (hide) last sent per VM.
#[derive(Default, Clone)]
pub(crate) struct PauseMenu {
    pub(crate) state: u8,
    frame: u32,
    pub(crate) vms: Vec<AnmVm>,
    shown: [bool; 10],
    // Capture VM +0x1A70 (capture.anm script 0, 12 §3).
    pub(crate) cap_vm: AnmVm,
}

// What the pause and continue menus touch each calc (12 §3, §4): ANM, RNG, game state, this frame's input, music,
// sound queue and the running game's HUD (Hud_StageTransition on the retry paths).
pub(crate) struct MenuEnv<'a> {
    pub anm: &'a mut crate::anmmgr::AnmManager,
    pub rng: &'a mut crate::rng::Rng,
    pub g: &'a mut Game,
    pub inp: &'a Input,
    pub bgm: &'a mut crate::sound::Bgm,
    pub sound: &'a mut crate::sound::Queue,
    pub hud: Option<&'a mut crate::hud::Hud>,
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

    // Close interrupts sent in the branch that changes the state (12 §3): interrupt 2 to `ks`, and interrupt 1 to
    // the capture VM (label 1 = alpha_fade 0, 30) on the resume paths.
    fn close(&mut self, ks: impl IntoIterator<Item = usize>, capture: bool) {
        for k in ks {
            self.shown[k] = false;
            self.vms[k].pending_interrupt = 2;
        }
        if capture {
            self.cap_vm.pending_interrupt = 1;
        }
    }

    // Shortcut targets: interrupt 2 to every VM with flag bit 0 (visible) set (12 §3).
    fn visible(&self) -> Vec<usize> {
        (0..10)
            .filter(|&k| self.vms[k].flags & anmvm::F_VISIBLE != 0)
            .collect()
    }

    // Input is read from frame 4 of each state; confirms sfx 10, cursor moves sfx 0 (12 §3).
    fn update(&mut self, e: MenuEnv, host: &mut dyn crate::host::Host) {
        let MenuEnv {
            anm: ctx_anm,
            rng,
            g,
            inp,
            bgm,
            sound,
            hud,
        } = e;
        let replay = g.replay();
        if self.state == 0 {
            // All ten scripts start; then VM 8 (practice label) is hidden unless practice and VM 9 (slow label)
            // unless the gameplay copy's slow byte ≠ 0 (12 §3, 0x406700 clears flag bit 0).
            self.vms = (12..22)
                .map(|s| {
                    let mut vm = AnmVm::default();
                    ctx_anm.set_script(rng, &mut vm, ANM_SLOT, s);
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
            start_capture(ctx_anm, rng, &mut self.cap_vm);
            // Only queued: copied after this frame's Present (12 §3 capture timing). It cannot fail, since every
            // Present applies the pending request, so the "close at once" path is not modelled.
            queue_capture(ctx_anm, &self.cap_vm);
            let diff_vm = &mut self.vms[P_DIFF];
            diff_vm.set_sprite(ctx_anm, difficulty_sprite(g));
            // Interrupt 1 to VMs 0–3 only, VM 3 also in replay (12 §3).
            for k in [P_TITLE, 1, 2, 3] {
                self.show(k, true);
            }
            // VMs 7–9 get no interrupt; their scripts start the fade-in on the first run (12 §3).
            self.shown[P_DIFF] = true;
            self.shown[P_PRACTICE] = g.practice();
            self.shown[P_SLOW] = g.game_slow() != 0;
            self.goto(1);
        }
        self.write_colours();
        let pressed = |b: u16| self.frame >= 4 && inp.pressed(b);
        let confirm = pressed(input::SHOT | input::ENTER);
        let (up, down) = (pressed(input::UP), pressed(input::DOWN));
        if matches!(self.state, 1..=3 | 5..=8) {
            if up || down {
                sound.play(crate::sound::PLST);
            } else if confirm {
                sound.play(crate::sound::OK);
            }
            // Shortcut keys ESC / Q / R (R not in replay) (12 §3).
            if inp.pressed(input::PAUSE) {
                self.close(self.visible(), true);
                self.goto(4);
            } else if inp.pressed(input::KEY_Q) {
                self.close(self.visible(), false);
                self.goto(9);
            } else if inp.pressed(input::KEY_R) && !replay {
                self.close(self.visible(), false);
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
                    self.close(0..4, true);
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
                        // "Yes": interrupt 2 to VMs 4–6 only; the difficulty / practice / slow labels stay (12 §3).
                        self.close([P_REALLY, P_YES, P_NO], false);
                        self.goto(if base == 5 { 9 } else { 10 });
                    } else {
                        self.confirm_screen(false);
                        self.goto(if base == 5 { 2 } else { 3 });
                    }
                }
            }
            // States 4 / 9 / 10 send no interrupts themselves (12 §3).
            4 | 9 | 10 if self.frame >= 20 => {
                let e = MenuEnv {
                    anm: ctx_anm,
                    rng,
                    g,
                    inp,
                    bgm,
                    sound,
                    hud,
                };
                self.leave(e, host);
                return;
            }
            _ => {}
        }
        for vm in self
            .vms
            .iter_mut()
            .chain(std::iter::once(&mut self.cap_vm))
            .filter(|v| v.script.is_some())
        {
            vm.run(ctx_anm, rng);
        }
        self.frame += 1;
    }

    // Frame 20 of states 4 (resume), 9 (quit), 10 (retry): the exit action, then paused := 0 and the menu freed (12 §3).
    fn leave(&mut self, e: MenuEnv, host: &mut dyn crate::host::Host) {
        let MenuEnv {
            anm: ctx_anm,
            rng,
            g,
            bgm,
            hud,
            ..
        } = e;
        match self.state {
            4 => bgm.pause_bgm(false),
            9 => {
                // State 9: Result_Register(2) (save) and scene 1 (12 §3).
                g.save_score(rng, true, host);
                g.request(SceneId::Menu);
            }
            _ => {
                // State 10: spell practice / practice / Extra reload the stage (scene 11) after
                // Hud_StageTransition, else scene 10 (12 §3).
                if g.practice() || g.spell_practice() || g.difficulty >= 4 {
                    // Music_Stop outside spell practice or for a card whose record flag is 1; else
                    // "UnPause" and the music keeps playing (12 §3 state 10).
                    if !g.spell_practice() || gamemgr::spell_bgm_restarts(g.card) {
                        bgm.stop_bgm();
                    } else {
                        // Command 7 "UnPause", then `Bgm_StartFadeIn(2.0)`: 120 frames from −10000.
                        bgm.pause_bgm(false);
                        bgm.start_fade_in(2.0);
                    }
                    stage_transition(hud, ctx_anm, rng);
                    g.request(SceneId::Reload);
                } else {
                    g.request(SceneId::Retry);
                }
            }
        }
        g.paused = 0;
        g.mark_play_time(host.ticks_ms());
        self.state = 0;
        self.vms.clear();
        self.cap_vm = AnmVm::default();
    }

    // Whole-dword colour stores at the top of the cursor states, before the input test (12 §3): selected
    // 0xFFFFFFFF, others 0xFF505050; confirm screen selected 0xFFFF8080.
    fn write_colours(&mut self) {
        match self.state {
            1..=3 => {
                for (i, &k) in P_ITEMS.iter().enumerate() {
                    let sel = self.state as usize == i + 1;
                    set_argb(
                        &mut self.vms[k],
                        if sel { 0xFFFF_FFFF } else { 0xFF50_5050 },
                    );
                }
            }
            5..=8 => {
                let yes_sel = matches!(self.state, 5 | 7);
                let (on, off) = (0xFFFF_8080, 0xFF50_5050);
                set_argb(&mut self.vms[P_YES], if yes_sel { on } else { off });
                set_argb(&mut self.vms[P_NO], if yes_sel { off } else { on });
            }
            _ => {}
        }
    }

    fn confirm_screen(&mut self, on: bool) {
        for k in [P_TITLE, 1, 2, 3] {
            self.show(k, !on);
        }
        for k in [P_REALLY, P_YES, P_NO] {
            self.show(k, on);
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
fn start_capture(anm: &mut crate::anmmgr::AnmManager, rng: &mut crate::rng::Rng, vm: &mut AnmVm) {
    *vm = AnmVm::default();
    anm.set_script(rng, vm, CAPTURE_SLOT, 0);
    vm.pos = [32.0, 16.0, 0.0];
}

// AnmManager_QueueCapture 0x406A30 (12 §3): playfield (32, 16, 384, 448) into slot 3 entry 0 at the VM's sprite rect
// (u0, v0 in texture pixels, width, height, truncated). The port copies at once from the last presented frame, so no
// request is ever pending and the "close at once" failure path cannot happen.
pub fn apply_capture(anm: &crate::anmmgr::AnmManager, rec: &mut Recorder, vm: &AnmVm) {
    if let Some(sp) = anm.sprite(
        CAPTURE_SLOT,
        usize::from(vm.sprite_index.max(0).cast_unsigned()),
    ) {
        capture_into(rec, sp);
    }
}

#[expect(
    clippy::cast_sign_loss,
    reason = "ftol of the sprite rect (12 §3), as the original"
)]
fn capture_into(rec: &mut Recorder, sp: &crate::anmmgr::SlotSprite) {
    let t = rec.texture(sp.tex);
    let to = [
        (sp.uv[0] * t.width as f32) as u32,
        (sp.uv[1] * t.height as f32) as u32,
    ];
    let tex = sp.tex;
    rec.capture_rect(tex, [32, 16], [384, 448], to, [sp.w as u32, sp.h as u32]);
}

// AnmManager_QueueCapture 0x406A30 (12 §3): stores the request only if none is pending (else −1); the destination is
// the VM's sprite rect at queue time. Applied by `apply_pending_captures` after the next Present.
pub fn queue_capture(anm: &mut crate::anmmgr::AnmManager, vm: &AnmVm) -> i32 {
    if anm.capture_req.is_some() {
        return -1;
    }
    let Some(&sp) = anm.sprite(
        CAPTURE_SLOT,
        usize::from(vm.sprite_index.max(0).cast_unsigned()),
    ) else {
        return -1;
    };
    anm.capture_req = Some(sp);
    0
}

// Hud_StageTransition on the registered game's HUD (pause / continue retry paths, 12 §3).
fn stage_transition(
    hud: Option<&mut crate::hud::Hud>,
    anm: &mut crate::anmmgr::AnmManager,
    rng: &mut crate::rng::Rng,
) {
    if let Some(h) = hud {
        h.stage_transition(anm, rng);
    }
}

// AnmManager_ApplyPendingCaptures 0x443B60 (12 §3): right after Present, the playfield (32, 16, 384, 448) is copied.
pub fn apply_pending_captures(anm: &mut crate::anmmgr::AnmManager, rec: &mut Recorder) {
    if let Some(sp) = anm.capture_req.take() {
        capture_into(rec, &sp);
    }
}

// Continue / game-over menu (11 §4.2, 12 §4): ascii scripts 22–24 and times.anm script 1 (night clock).
// ContinueMenu_Update 0x404890 (12 §4); phase 1 = no, 3 = continue (11 §4.2).
#[derive(Default, Clone)]
pub(crate) struct ContinueMenu {
    pub(crate) open: bool,
    yes: bool,
    frame: u32,
    phase: u8,
    phase_frame: u32,
    pub(crate) vms: Vec<AnmVm>,
    pub(crate) clock_vm: AnmVm,
    // Capture VM menu +0xFE0 (12 §4).
    pub(crate) cap_vm: AnmVm,
}

// 0x404890 early exits (11 §4.2): game_over := 0 and displayed score := score, no sound / VMs / capture /
// play time; GameMgr_Calc later in the same pass then runs one more gameplay frame. `update` then returns true and
// the caller syncs the player's displayed score (nothing in between reads it).
fn early_exit(g: &mut Game, scene: SceneId) {
    g.game_over = 0;
    g.request(scene);
}

impl ContinueMenu {
    // `now`: `Host::ticks_ms` for the play-time marks (15 §2.9); `enemies`: the running stage's enemy manager (spell
    // flag bit 9, captured); `e.hud`: Hud_StageTransition on "Yes" in spell practice / Extra.
    fn update(
        &mut self,
        e: MenuEnv,
        panel: &mut crate::hud::Panel,
        now: u64,
        enemies: Option<&crate::enemy::EnemyManager>,
    ) -> bool {
        let MenuEnv {
            anm: ctx_anm,
            rng,
            g,
            inp,
            bgm,
            sound,
            hud,
        } = e;
        if !self.open {
            // First call, on the calc after game_over was written (12 §4): state 0 here, then the state-2/1 body
            // below in the same call.
            *self = ContinueMenu {
                open: true,
                ..ContinueMenu::default()
            };
            log::info!("continue menu opened");
            if g.practice() && !g.spell_practice() {
                early_exit(g, SceneId::Result);
                return true;
            }
            if g.replay() {
                early_exit(g, SceneId::ReplayEnd);
                return true;
            }
            if g.difficulty < 4 && !g.spell_practice() && (g.clock > 10 || g.stage % 9 == 7) {
                g.flags &= !gamemgr::F_ENDING_OK;
                early_exit(g, SceneId::Ending);
                return true;
            }
            // Play time is added only when the prompt opens, not on the early exits (15 §2.9).
            g.add_play_time(now);
            // Music on opening (12 §4): spell practice with a non-restarting card dips to −10 dB and keeps
            // playing; otherwise command 6 "Pause".
            if g.spell_practice() && !gamemgr::spell_bgm_restarts(g.card) {
                bgm.start_dip(1.0);
            } else {
                bgm.pause_bgm(true);
            }
            self.vms = (22..25)
                .map(|s| {
                    let mut vm = AnmVm::default();
                    ctx_anm.set_script(rng, &mut vm, ANM_SLOT, s);
                    vm.pending_interrupt = 1;
                    vm
                })
                .collect();
            ctx_anm.set_script(rng, &mut self.clock_vm, crate::hud::TIMES_SLOT, 1);
            self.clock_vm.set_sprite(ctx_anm, g.clock.clamp(0, 15));
            start_capture(ctx_anm, rng, &mut self.cap_vm);
            // Queued, copied after this frame's Present (12 §3 capture timing, §4).
            queue_capture(ctx_anm, &self.cap_vm);
            // Default "No"; "Yes" only in spell practice with spell flag bit 9 (captured) clear (12 §4).
            let captured = enemies.is_some_and(|e| e.g.spell_captured);
            self.yes = g.spell_practice() && !captured;
        }
        // Input from frame 30 (cursor on "No") or frame 4 (cursor on "Yes") (12 §4).
        let ready = self.frame
            >= if self.yes && g.spell_practice() {
                4
            } else {
                30
            };
        // States 1 / 2: whole ARGB writes at the top, before the input test and the VM run (12 §4).
        if self.phase == 0
            && let [_, y, n] = &mut self.vms[..]
        {
            let (on, off) = (0xFFFF_8080, 0xFF50_5050);
            set_argb(y, if self.yes { on } else { off });
            set_argb(n, if self.yes { off } else { on });
        }
        match self.phase {
            0 if ready => {
                if inp.pressed(input::UP) || inp.pressed(input::DOWN) {
                    self.yes = !self.yes;
                    sound.play(crate::sound::PLST);
                }
                if inp.pressed(input::SHOT | input::ENTER) {
                    sound.play(crate::sound::OK);
                    if self.yes && (g.difficulty >= 4 || g.spell_practice()) {
                        // "Yes" in spell practice / Extra acts in the press call (12 §4): `Music_Stop`, or for a
                        // non-restarting card "UnPause" + `Bgm_StartFadeInFromDip(1.0)`; scene 11, Hud_StageTransition,
                        // game_over := 0, play-time timestamp; no VM run and no frame += 1.
                        if !g.spell_practice() || gamemgr::spell_bgm_restarts(g.card) {
                            bgm.stop_bgm();
                        } else {
                            bgm.pause_bgm(false);
                            bgm.start_fade_in_from_dip(1.0);
                        }
                        g.request(SceneId::Reload);
                        stage_transition(hud, ctx_anm, rng);
                        g.game_over = 0;
                        g.mark_play_time(now);
                        self.open = false;
                        return false;
                    }
                    self.phase = if self.yes { 3 } else { 1 };
                    self.phase_frame = 0;
                    if self.phase == 1 {
                        self.vms.iter_mut().for_each(|v| v.pending_interrupt = 2);
                        self.clock_vm.pending_interrupt = 2;
                    }
                }
            }
            1 if self.phase_frame >= 20 => {
                // "No" at P+20 (12 §4 "Confirm timing"): game_over := 0, scene 6, the 4 VMs hidden, displayed
                // score := score; GameMgr_Calc then runs one more gameplay frame (11 §4.2).
                for vm in self.vms.iter_mut().chain([&mut self.clock_vm]) {
                    vm.flags &= !crate::anmvm::F_VISIBLE;
                }
                early_exit(g, SceneId::Result);
                self.open = false;
                return true;
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
                    // Resume (counter > 0x59): the only 0x17CE8CC := 8 of 0x404890 (0x40512A, 12 §5.3).
                    panel.set_repaint(8);
                    g.continue_apply = true;
                    g.game_over = 0;
                    g.mark_play_time(now);
                    self.open = false;
                    return false;
                }
            }
            _ => {}
        }
        for vm in self
            .vms
            .iter_mut()
            .chain([&mut self.clock_vm, &mut self.cap_vm])
            .filter(|v| v.script.is_some())
        {
            vm.run(ctx_anm, rng);
        }
        self.frame += 1;
        self.phase_frame += 1;
        false
    }
}

// ASCII manager (calc 1, draw 14/20; slots 1 ascii.anm, 3 capture.anm): text queue, pause and continue menus.
#[derive(Default, Clone)]
pub struct AsciiManager {
    pub(crate) pause: PauseMenu,
    pub(crate) cont: ContinueMenu,
    // Draw-phase results the emit reads: shake offset before the prio-14 zeroing, gauge readout, the drained strings
    // per pass (0 = prio 14, 1 = prio 20).
    pub(crate) popup_off: [f32; 2],
    pub(crate) gauge: Option<GaugeDraw>,
    pub(crate) texts: [Vec<Text>; 2],
}

impl AsciiManager {
    pub fn register(ctx: &mut Ctx) -> Result<()> {
        ctx.w.ascii_mgr = Some(Box::default());
        let t = crate::chain::Node::Ascii;
        task::add(ctx, Chain::Calc, CALC_PRIO, 0, t)?;
        task::add(ctx, Chain::Draw, DRAW_LOW_PRIO, 0, t)?;
        task::add(ctx, Chain::Draw, DRAW_HIGH_PRIO, 1, t)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn trace(&self) -> Vec<u64> {
        let (p, c) = (&self.pause, &self.cont);
        let mut t = vec![
            u64::from(p.state),
            u64::from(p.frame),
            u64::from(c.open),
            u64::from(c.frame),
            u64::from(self.popup_off[0].to_bits()),
            self.texts[0].len() as u64,
            self.texts[1].len() as u64,
        ];
        t.extend(p.vms.iter().chain(&c.vms).map(crate::worldclone::hash_dbg));
        t
    }

    // The manager lent out of `ctx.w.ascii_mgr` for one of its chain callbacks.
    fn with<R>(ctx: &mut Ctx, f: impl FnOnce(&mut AsciiManager, &mut Ctx) -> R) -> R {
        let mut a = ctx.w.ascii_mgr.take().expect("no ascii manager");
        let r = f(&mut a, ctx);
        ctx.w.ascii_mgr = Some(a);
        r
    }
}

// Chain nodes of `ctx.w.ascii_mgr` (calc 1, draw 14 / 20): stateless forwarder.
pub struct AsciiTask;

impl Task<Ctx> for AsciiTask {
    fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, tag: u8) -> Result<()> {
        AsciiManager::with(ctx, |a, ctx| a.on_register(ctx, chain, tag))
    }

    fn calc(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        AsciiManager::with(ctx, |a, ctx| a.calc(ctx, tag))
    }

    fn draw_step(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        AsciiManager::with(ctx, |a, ctx| a.draw_step(ctx, tag))
    }
}

impl AsciiManager {
    pub(crate) fn on_register(&mut self, ctx: &mut Ctx, chain: Chain, _: u8) -> Result<()> {
        if chain == Chain::Calc {
            ctx.w
                .anm
                .load(&mut ctx.rec, &ctx.archive, ANM_SLOT, "ascii.anm")?;
            ctx.w
                .anm
                .load(&mut ctx.rec, &ctx.archive, CAPTURE_SLOT, "capture.anm")?;
            let l = crate::player::GaugeLimits::default();
            ctx.w
                .overlays
                .gauge_setup(&mut ctx.w.anm, &mut ctx.w.rng, l.min, l.max);
        }
        Ok(())
    }

    pub(crate) fn calc(&mut self, ctx: &mut Ctx, _: u8) -> Ret {
        // Supervisor_Init registers the manager on the boot loader thread, so the boot calc passes never reach
        // Ascii_Calc and +0x8284 equals the main-menu frame number (12 §5.10).
        if ctx.w.game.scene == SceneId::Boot {
            return Ret::Next;
        }
        let inp = ctx.w.input;
        gauge_calc(&mut ctx.w.overlays.gauge, &mut ctx.w.anm, &mut ctx.w.rng);
        // The pause menu runs while paused, the continue menu while game_over; popups only otherwise (12 §3, 13 §4).
        if ctx.w.game.paused != 0 {
            // 11 §2: paused = 2 once the menu was drawn (GameMgr_Calc opened it after this calc).
            ctx.w.game.paused = 2;
            let e = MenuEnv {
                anm: &mut ctx.w.anm,
                rng: &mut ctx.w.rng,
                g: &mut ctx.w.game,
                inp: &inp,
                bgm: &mut ctx.w.bgm,
                sound: &mut ctx.w.sound,
                hud: ctx.w.hud.as_deref_mut(),
            };
            self.pause.update(e, &mut *ctx.host);
        } else if ctx.w.game.game_over != 0 {
            let e = MenuEnv {
                anm: &mut ctx.w.anm,
                rng: &mut ctx.w.rng,
                g: &mut ctx.w.game,
                inp: &inp,
                bgm: &mut ctx.w.bgm,
                sound: &mut ctx.w.sound,
                hud: ctx.w.hud.as_deref_mut(),
            };
            let exited = self.cont.update(
                e,
                &mut ctx.w.panel,
                ctx.host.ticks_ms(),
                ctx.w.enemies.as_deref(),
            );
            if exited && let Some(p) = ctx.w.player.as_deref_mut() {
                p.stats.score_disp = p.stats.score10;
            }
        } else {
            self.cont.open = false;
            self.pause.state = 0;
            // Ascii_Calc 0x402200 (calc prio 1) reads GameFlags bit 10 live, not the end-of-Player_Calc mirror (13 §4).
            if ctx.w.game.flags & crate::gamemgr::F_DEATH == 0 {
                popups_calc(&mut ctx.w.overlays.popups, ctx.w.anm.speed);
            }
        }
        // +0x8284 is incremented at the very end of Ascii_Calc (12 §5.10).
        let g = &mut ctx.w.overlays.gauge;
        g.frame = g.frame.wrapping_add(1);
        Ret::Next
    }

    // Step (prio 14): darkness VM (script set, RNG), shake offset := 0 (12 §5.9 draw), gauge VM writes, low strings
    // drained; (prio 20): high strings drained, marker sprite / colour. Pause and continue draws write nothing.
    pub(crate) fn draw_step(&mut self, ctx: &mut Ctx, tag: u8) -> Ret {
        let high = tag == 1;
        if !high {
            let pl = ctx.w.effects.player;
            darkness_step(
                &mut ctx.w.overlays.darkness,
                &mut ctx.w.anm,
                &mut ctx.w.rng,
                pl,
            );
            self.popup_off = ctx.w.anm.screen_offset;
            // The gauge readout is drawn after the shake offset is zeroed (12 §5.9 draw).
            ctx.w.anm.screen_offset = [0.0; 2];
            self.gauge = gauge_step(
                &mut ctx.w.overlays.gauge,
                &ctx.w.anm,
                ctx.w.player.as_deref(),
                &ctx.w.game.stats,
            );
        }
        self.texts[usize::from(high)] = ctx.w.ascii.take(Some(high));
        if high {
            markers_step(&mut ctx.w.overlays.gauge, ctx.w.player.as_deref());
        }
        Ret::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 12 §1: Loading_Wait sets 2; the third draw (c = 5) is the first prompt frame; alpha 255 … 127 (c 35) … 251
    // (c 64), 255 (c 65); then c restarts at 6, a 60-frame period.
    #[test]
    fn press_shot_prompt_counter() {
        let mut st = 0;
        assert_eq!(prompt_step(&mut st), None);
        assert_eq!(st, 0);
        st = 2;
        assert_eq!(prompt_step(&mut st), None);
        assert_eq!(prompt_step(&mut st), None);
        let a: Vec<u8> = (0..62).map(|_| prompt_step(&mut st).unwrap()).collect();
        assert_eq!(&a[..3], &[255, 251, 247]);
        assert_eq!(a[30], 127);
        assert_eq!(a[31], 132);
        assert_eq!((a[59], a[60]), (251, 255));
        assert_eq!(a[61], 251);
        assert_eq!(st, 6);
    }

    // 12 §4: whole-dword colours (alpha 0xFF) written before the input test, so a cursor move shows a frame later.
    #[test]
    fn continue_menu_colours_before_input() {
        let mut rng = crate::rng::Rng::default();
        let mut anm = crate::anmmgr::AnmManager::default();
        let mut bgm = crate::sound::Bgm::default();
        let mut g = Game::default();
        g.game_over = 1;
        let mut m = ContinueMenu::default();
        for _ in 0..30 {
            cont(
                &mut m,
                &mut anm,
                &mut rng,
                &mut g,
                &mut bgm,
                &Input::default(),
            );
        }
        assert_eq!(m.vms[2].color, [0x80, 0x80, 0xFF, 0xFF]);
        assert_eq!(m.vms[1].color, [0x50, 0x50, 0x50, 0xFF]);
        cont(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &press(input::UP),
        );
        assert!(m.yes);
        assert_eq!(m.vms[2].color, [0x80, 0x80, 0xFF, 0xFF]);
        cont(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &Input::default(),
        );
        assert_eq!(m.vms[1].color, [0x80, 0x80, 0xFF, 0xFF]);
    }

    // 12 §4: game_over is written in the player calc (prio 9); the next ASCII calc (prio 1) opens the menu, does
    // state 0 and the state-2 body in that one call, so after it the menu counts frame 1 with the cursor on "No".
    #[test]
    fn continue_menu_opens_next_calc() {
        let mut rng = crate::rng::Rng::default();
        let mut anm = crate::anmmgr::AnmManager::default();
        let mut g = Game::default();
        let mut p = crate::player::Player {
            game_over: true,
            ..crate::player::Player::default()
        };
        p.store_game_over(&mut g);
        assert!(g.game_over == 1 && !p.game_over);
        let mut m = ContinueMenu::default();
        let mut bgm = crate::sound::Bgm::default();
        cont(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &Input::default(),
        );
        assert!(m.open && !m.yes);
        assert_eq!((m.phase, m.frame, m.phase_frame), (0, 1, 1));
        assert_eq!(g.requested, SceneId::Boot);
    }

    // 11 §4.2 / 15 §2.9: the replay early exit clears game_over, requests scene 7, opens nothing, adds no play time.
    #[test]
    fn continue_menu_replay_early_exit() {
        let mut rng = crate::rng::Rng::default();
        let mut anm = crate::anmmgr::AnmManager::default();
        let mut g = Game::default();
        g.flags |= gamemgr::F_REPLAY;
        g.game_over = 1;
        g.mark_play_time(0);
        let mut m = ContinueMenu::default();
        let mut bgm = crate::sound::Bgm::default();
        cont(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &Input::default(),
        );
        assert_eq!((g.game_over, g.requested), (0, SceneId::ReplayEnd));
        assert!(g.play_clock.is_some() && m.vms.is_empty());
    }

    // One ContinueMenu calc with a throwaway sound queue and panel, no HUD / enemies, time 0.
    fn cont(
        m: &mut ContinueMenu,
        anm: &mut crate::anmmgr::AnmManager,
        rng: &mut crate::rng::Rng,
        g: &mut Game,
        bgm: &mut crate::sound::Bgm,
        inp: &Input,
    ) -> bool {
        let e = MenuEnv {
            anm,
            rng,
            g,
            inp,
            bgm,
            sound: &mut crate::sound::Queue::default(),
            hud: None,
        };
        m.update(e, &mut crate::hud::Panel::default(), 0, None)
    }

    // One PauseMenu calc with a throwaway sound queue and host, no HUD.
    fn pause(
        m: &mut PauseMenu,
        anm: &mut crate::anmmgr::AnmManager,
        rng: &mut crate::rng::Rng,
        g: &mut Game,
        bgm: &mut crate::sound::Bgm,
        inp: &Input,
    ) {
        let e = MenuEnv {
            anm,
            rng,
            g,
            inp,
            bgm,
            sound: &mut crate::sound::Queue::default(),
            hud: None,
        };
        m.update(e, &mut crate::host::MemHost::default());
    }

    fn press(b: u16) -> Input {
        Input {
            held: b,
            ..Input::default()
        }
    }

    // 12 §4 "Confirm timing": Extra "Yes" acts in the press call (scene 11, game_over := 0, no frame += 1); "No"
    // clears game_over and sets displayed score := score in the call 20 frames after the press.
    #[test]
    fn continue_menu_confirm_timing() {
        let mut rng = crate::rng::Rng::default();
        let mut anm = crate::anmmgr::AnmManager::default();
        let mut bgm = crate::sound::Bgm::default();
        let mut g = Game::default();
        g.difficulty = 4;
        g.game_over = 1;
        let mut m = ContinueMenu::default();
        for _ in 0..30 {
            cont(
                &mut m,
                &mut anm,
                &mut rng,
                &mut g,
                &mut bgm,
                &Input::default(),
            );
        }
        cont(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &press(input::UP),
        );
        assert!(m.yes);
        let frame = m.frame;
        cont(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &press(input::SHOT),
        );
        assert_eq!((g.game_over, g.requested), (0, SceneId::Reload));
        assert!(!m.open && m.frame == frame);

        let mut g = Game::default();
        g.difficulty = 4;
        g.game_over = 1;
        let mut m = ContinueMenu::default();
        for _ in 0..30 {
            cont(
                &mut m,
                &mut anm,
                &mut rng,
                &mut g,
                &mut bgm,
                &Input::default(),
            );
        }
        cont(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &press(input::SHOT),
        );
        for _ in 0..19 {
            cont(
                &mut m,
                &mut anm,
                &mut rng,
                &mut g,
                &mut bgm,
                &Input::default(),
            );
            assert_eq!(g.game_over, 1);
        }
        cont(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &Input::default(),
        );
        assert_eq!((g.game_over, g.requested), (0, SceneId::Result));
        assert!(!m.open && m.vms.iter().all(|v| !v.visible()));
    }

    // 12 §3: state 0 interrupts VMs 0–3 only (VM 3 also in replay); confirm on Resume sends interrupt 2 to
    // VMs 0–3 and 1 to the capture VM in that call; "yes" sends interrupt 2 to VMs 4–6 only; states 4/9/10 send none.
    #[test]
    fn pause_menu_interrupts() {
        let mut rng = crate::rng::Rng::default();
        let mut anm = crate::anmmgr::AnmManager::default();
        let mut bgm = crate::sound::Bgm::default();
        let mut g = Game::default();
        g.flags |= gamemgr::F_REPLAY;
        g.paused = 1;
        let mut m = PauseMenu::default();
        let ints = |m: &PauseMenu| {
            m.vms
                .iter()
                .map(|v| v.pending_interrupt)
                .collect::<Vec<_>>()
        };
        pause(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &Input::default(),
        );
        assert_eq!(ints(&m), [1, 1, 1, 1, 0, 0, 0, 0, 0, 0]);
        for _ in 0..4 {
            pause(
                &mut m,
                &mut anm,
                &mut rng,
                &mut g,
                &mut bgm,
                &Input::default(),
            );
        }
        pause(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &press(input::SHOT),
        );
        assert_eq!(m.state, 4);
        assert_eq!(ints(&m), [2, 2, 2, 2, 0, 0, 0, 0, 0, 0]);
        assert_eq!(m.cap_vm.pending_interrupt, 1);

        let mut g = Game::default();
        g.paused = 1;
        let mut m = PauseMenu::default();
        pause(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &Input::default(),
        );
        m.state = 5;
        m.frame = 4;
        for v in &mut m.vms {
            v.pending_interrupt = 0;
        }
        pause(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &press(input::SHOT),
        );
        assert_eq!(m.state, 9);
        assert_eq!(ints(&m), [0, 0, 0, 0, 2, 2, 2, 0, 0, 0]);
        pause(
            &mut m,
            &mut anm,
            &mut rng,
            &mut g,
            &mut bgm,
            &Input::default(),
        );
        assert_eq!(ints(&m), [0, 0, 0, 0, 2, 2, 2, 0, 0, 0]);
    }

    #[test]
    fn popup_digits_and_life() {
        assert_eq!(digits(1230), vec![0, 3, 2, 1]);
        assert_eq!(digits(0), vec![0]);
        assert_eq!(digits(-1), vec![10]);
        let mut ov = Overlays::default();
        ov.popup([100.0, 200.0, 0.0], 500, 0xFFFF_FFFF);
        for _ in 0..60 {
            popups_calc(&mut ov.popups, 1.0);
        }
        let y = ov.popups.slots[0].as_ref().map(|q| q.pos[1]);
        assert_eq!(y, Some(216.0 - 30.0));
        popups_calc(&mut ov.popups, 1.0);
        assert!(ov.popups.slots[0].is_none());
    }

    // 13 §4: Ascii_InitVms resets the small and special cursors, not the large one.
    #[test]
    fn clear_popups_resets_small_cursors() {
        let mut ov = Overlays::default();
        ov.popup([0.0; 3], 1, 0xFFFF_FFFF);
        ov.popup_special([0.0; 3], 1, 0xFFFF_FFFF);
        ov.popup_large_static([0.0, 0.0], 1, 0, 0xFFFF_FFFF);
        let large0 = ov.popups.large_next;
        ov.popups.clear();
        let p = &ov.popups;
        assert_eq!((p.next, p.special, p.large_next), (0, 0, large0));
        assert!(p.slots.iter().all(Option::is_none));
        assert!(p.large.iter().all(Option::is_none));
    }

    // 13 §4: Timer_Greater on the integer cur, so at speed 0.5 a small popup lives 122 calcs, a static
    // large popup (age 88) 6.
    #[test]
    fn popup_life_on_integer_cur() {
        let mut ov = Overlays::default();
        ov.popup([100.0, 200.0, 0.0], 500, 0xFFFF_FFFF);
        ov.popup_large_static([100.0, 200.0], 1, 0, 0xFFFF_FFFF);
        let live = |p: &Popups| (p.slots[0].is_some(), p.large[0].is_some());
        for n in 1..=122 {
            popups_calc(&mut ov.popups, 0.5);
            let (small, large) = live(&ov.popups);
            assert_eq!(small, n < 122, "small {n}");
            assert_eq!(large, n < 6, "large {n}");
        }
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
        let mut ov = Overlays::default();
        ov.set_darkness(255, 288.0);
        ov.clear_darkness();
        assert_eq!(ov.darkness_alpha(), 0);
    }
}
