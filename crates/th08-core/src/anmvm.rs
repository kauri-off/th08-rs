//! ANM script VM `AnmVm_Run` 0x45EA00: timers, variables, opcodes, per-frame POST (05 §1–§8).

use crate::anmmgr::AnmManager;
use crate::types::D3dColor;
use th08_formats::anm::{
    AnmOff, Arg, Arith, Cmp, FlowOp, Ins, InterpOp, MathFn, Op, SpriteOp, VarOp, signature,
};

bitflags::bitflags! {
    // AnmVm flags at vm+0x1F8 (05 §2.1), the original's bit positions.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct VmFlags: u32 {
        const VISIBLE = 1 << 0;
        // Set at init, never changed by scripts; also required for drawing (05 §2.1, §12.2).
        const ENABLED = 1 << 1;
        // Bits 2/3: mode-5 world matrix must be rebuilt (05 §9).
        const ROT_DIRTY = 1 << 2;
        const SCALE_DIRTY = 1 << 3;
        // Bits 4–5: 0 alpha, 1 additive, 2/3 leave the previous dest-blend (05 §2.1, §12.5).
        const BLEND = 3 << 4;
        // Op 6 and the pos slot write pos_offset (vm+0x288) instead of pos (05 §2.1).
        const POS_OFFSET = 1 << 8;
        const FLIP_X = 1 << 9;
        const FLIP_Y = 1 << 10;
        // Bits 11/12: x / y anchored at the left / top edge instead of the centre (05 §9).
        const ANCHOR_X = 1 << 11;
        const ANCHOR_Y = 1 << 12;
        const ZWRITE_OFF = 1 << 13;
        // Set by ops 20/23 while waiting for an interrupt (05 §5).
        const WAIT_INT = 1 << 14;
        // Draw with the stage 3D camera instead of the default 2D camera (05 §9).
        const STAGE_CAM = 1 << 15;
        // Do not rebuild the mode-5 world matrix (05 §9).
        const FREEZE_MATRIX = 1 << 16;
        // Draw with colour2; the skip test still uses the primary alpha (05 §12.2).
        const COLOR2 = 1 << 17;
        // AnmVm_Run returns 0 at once (05 §4).
        const PAUSED = 1 << 19;
    }
}

// Raw bit values of `VmFlags` for the owners that still use the `flags: u32` word directly.
pub const F_VISIBLE: u32 = VmFlags::VISIBLE.bits();
pub const F_ENABLED: u32 = VmFlags::ENABLED.bits();
pub const F_ROT_DIRTY: u32 = VmFlags::ROT_DIRTY.bits();
pub const F_SCALE_DIRTY: u32 = VmFlags::SCALE_DIRTY.bits();
pub const F_BLEND_SHIFT: u32 = VmFlags::BLEND.bits().trailing_zeros();
pub const F_POS_OFFSET: u32 = VmFlags::POS_OFFSET.bits();
pub const F_FLIP_X: u32 = VmFlags::FLIP_X.bits();
pub const F_FLIP_Y: u32 = VmFlags::FLIP_Y.bits();
pub const F_ANCHOR_X: u32 = VmFlags::ANCHOR_X.bits();
pub const F_ANCHOR_Y: u32 = VmFlags::ANCHOR_Y.bits();
pub const F_ZWRITE_OFF: u32 = VmFlags::ZWRITE_OFF.bits();
pub const F_WAIT_INT: u32 = VmFlags::WAIT_INT.bits();
pub const F_STAGE_CAM: u32 = VmFlags::STAGE_CAM.bits();
pub const F_FREEZE_MATRIX: u32 = VmFlags::FREEZE_MATRIX.bits();
pub const F_COLOR2: u32 = VmFlags::COLOR2.bits();
pub const F_PAUSED: u32 = VmFlags::PAUSED.bits();

// Interpolation slot order of vm+0x050/0x0A4/0x0F8 (05 §2).
const SLOT_POS: usize = 0;
const SLOT_COLOR: usize = 1;
const SLOT_ALPHA: usize = 2;
const SLOT_ROT: usize = 3;
const SLOT_SCALE: usize = 4;
const SLOT_COLOR2: usize = 5;
const SLOT_ALPHA2: usize = 6;

// D3DCOLOR byte order of colour / colour2 (05 §2: +0x1F0 B, +0x1F1 G, +0x1F2 R, +0x1F3 A).
pub const B: usize = 0;
pub const G: usize = 1;
pub const R: usize = 2;
pub const A: usize = 3;

// 12-byte Timer (05 §2): previous value (−999 after a Set), sub-frame fraction, current value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timer {
    pub prev: i32,
    pub frac: f32,
    pub cur: i32,
}

impl Default for Timer {
    fn default() -> Self {
        Self {
            prev: -999,
            frac: 0.0,
            cur: 0,
        }
    }
}

impl Timer {
    pub fn set(&mut self, v: i32) {
        *self = Self {
            prev: -999,
            frac: 0.0,
            cur: v,
        };
    }

    // Timer_GetF (13 §3): cur + frac, computed on the x87 (f64 here).
    pub fn get_f(&self) -> f64 {
        f64::from(self.cur) + f64::from(self.frac)
    }

    // Timer_Tick (05 §2): one step per frame at full speed, else accumulates the fraction; not affected by flag 0x20.
    pub fn tick(&mut self, speed: f32) {
        self.prev = self.cur;
        if speed > 0.99 {
            self.cur += 1;
        } else {
            self.frac += speed;
            if self.frac >= 1.0 {
                self.cur += 1;
                self.frac -= 1.0;
            }
        }
    }

    // Timer_Sub(1) (05 §2), with the extra step of Supervisor flag 0x20 (`restore`, AnmManager field) first (05 §12.6).
    pub fn dec(&mut self, speed: f32, restore: bool) {
        if restore {
            self.cur -= 1;
            self.frac = 0.0;
            self.prev = -999;
        }
        // The fast path leaves prev alone (−999 after a Set stays); only the slow path copies it (05 §2, 0x44735B).
        if speed > 0.99 {
            self.cur -= 1;
        } else {
            self.prev = self.cur;
            self.frac -= speed;
            while self.frac < 0.0 {
                self.cur -= 1;
                self.frac += 1.0;
            }
        }
    }

    // Timer_GetF = current + fraction (05 §2).
    pub fn getf(&self) -> f32 {
        self.cur as f32 + self.frac
    }
}

// Angle_AddNormalize 0x43EDB0 (05 §6.3), moved to types.
pub use crate::types::wrap_add;

// UV offset wrap (05 §6 step 4, ops 26/27): a single step, not a modulo.
fn wrap_uv(u: f32) -> f32 {
    if u >= 1.0 {
        u - 1.0
    } else if u < 0.0 {
        u + 1.0
    } else {
        u
    }
}

// Easing modes of the interpolation slots (05 §6 step 2); unknown modes are linear.
pub fn ease(mode: u8, t: f32) -> f32 {
    match mode {
        1 => t * t,
        2 => t * t * t,
        3 => t * t * t * t,
        4 => 1.0 - (1.0 - t) * (1.0 - t),
        5 => 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t),
        6 => 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t) * (1.0 - t),
        _ => t,
    }
}

// Low byte of an int, the original's byte stores (alpha / colour bytes, the signed-char easing mode, 05 §5 notes).
fn lo8(v: i32) -> u8 {
    v.to_le_bytes()[0]
}

// Low word of an int, the op-25 tag store into the u16 at vm+0x1FC (05 §5).
fn lo16(v: i32) -> u16 {
    let b = v.to_le_bytes();
    u16::from_le_bytes([b[0], b[1]])
}

// A signed slot / sprite / script number as a table index: sign-extended like the original's indexing, so a
// negative number is far out of range and the lookup fails (05 §7: no change when the sprite is missing).
#[expect(
    clippy::cast_sign_loss,
    reason = "sign-extended index; negatives must miss the table"
)]
pub(crate) fn table_index(v: i32) -> usize {
    v as usize
}

// Debug listing in the style of 05 §5 ("$" = variable argument, 05 §3).
pub fn disasm(data: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let mut ip = AnmOff(0);
    while let Some(ins) = Ins::read(data, ip) {
        let sig = signature(ins.op).unwrap_or("?");
        let args: Vec<String> = (0..ins.nargs())
            .map(|k| {
                let a = ins.arg(data, k);
                let v = if a.var { "$" } else { "" };
                match sig.as_bytes().get(k) {
                    Some(b'f' | b'W') => format!("{v}{}", f32::from_bits(a.raw)),
                    _ => format!("{v}{}", a.raw.cast_signed()),
                }
            })
            .collect();
        let _ = writeln!(
            s,
            "  @{:<5} t={:<4} op{:<3} {}",
            ip.0,
            ins.time,
            ins.op,
            args.join(", ")
        );
        if ins.op == -1 || ins.size == 0 {
            break;
        }
        ip = ip.add(usize::from(ins.size));
    }
    s
}

// Variable ids of 05 §3: 10000–10003 I0–I3, 10004–10007 F0–F3, 10008/10009 I4/I5 (ivars[4], ivars[5]).
#[derive(Clone, Copy)]
enum Var {
    I(usize),
    F(usize),
}

fn var(id: i32) -> Option<Var> {
    Some(match id {
        10000 => Var::I(0),
        10001 => Var::I(1),
        10002 => Var::I(2),
        10003 => Var::I(3),
        10004 => Var::F(0),
        10005 => Var::F(1),
        10006 => Var::F(2),
        10007 => Var::F(3),
        10008 => Var::I(4),
        10009 => Var::I(5),
        _ => return None,
    })
}

// Destination of a V/W argument (05 §3): a VM variable or arg 0's own bytes in the script (`Lit`, its raw bits).
#[derive(Clone, Copy)]
enum Dest {
    I(usize),
    F(usize),
    Lit(u32),
}

// Result of one instruction (05 §4 LOOP): next instruction, jump, yield to POST, script finished, or dispatch the
// pending interrupt (ops 20 / 23 with a request already pending).
enum Step {
    Next,
    Jump(AnmOff),
    Post,
    Done,
    Interrupt,
}

// Game speed and Supervisor flag 0x20, read once at the start of AnmVm_Run (05 §2, §12.6).
#[derive(Clone, Copy)]
struct Clock {
    speed: f32,
    restore: bool,
}

// AnmVm, 0x2A4 bytes in the original (05 §2); base/tex matrices are derived at draw time by the port.
#[derive(Clone, Debug)]
pub struct AnmVm {
    // +0x000 radians; 2D draws use z only.
    pub rotation: [f32; 3],
    // +0x00C, added each frame × game speed (05 §6 step 1).
    pub angular_vel: [f32; 3],
    pub scale: [f32; 2],
    // +0x020, added each frame × game speed (05 §6 step 3).
    pub scale_growth: [f32; 2],
    // +0x028/+0x02C: current sprite size in logical px (05 §7).
    pub sprite_w: f32,
    pub sprite_h: f32,
    // +0x030/+0x034: added to the normalised sprite UVs, wrapped into [0,1) (05 §9).
    pub uv_off: [f32; 2],
    // +0x038 script clock; +0x044 op-79 wait timer.
    pub time: Timer,
    pub wait_timer: Timer,
    // +0x050 / +0x0A4 / +0x0F8: per-slot elapsed, duration (≤ 0 = inactive) and easing mode (05 §2).
    pub interp_elapsed: [Timer; 7],
    pub interp_duration: [Timer; 7],
    pub interp_mode: [u8; 7],
    // I0–I3 at +0x100, I4/I5 at +0x120 (vars 10000–10003, 10008–10009); F0–F3 at +0x110 (10004–10007) (05 §3).
    pub ivars: [i32; 6],
    pub fvars: [f32; 4],
    // +0x128, added to uv_off every frame, not scaled by game speed (05 §6 step 4).
    pub uv_scroll: [f32; 2],
    // +0x170 cached mode-5 model matrix (05 §9).
    pub world_matrix: crate::camera::Mat4,
    // +0x1F0 / +0x1F4 D3DCOLOR bytes B, G, R, A.
    pub color: [u8; 4],
    pub color2: [u8; 4],
    // +0x1F8 raw flag word, bits = `VmFlags` (05 §2.1).
    pub flags: u32,
    // +0x1FC op 25 value, read only by owners (player bullets: tag ≠ 0 → rotation z = angle, 05 §2).
    pub tag: u16,
    // +0x1FE, written by owners (AnmVm_Interrupt 0x407120), handled at the next run (05 §4).
    pub pending_interrupt: u16,
    // +0x200, 0..5 for AnmManager_DrawByMode (05 §9).
    pub draw_mode: i32,
    pub anm_slot: i16,
    // +0x208, outside the block AnmVm_Init clears (05 §8).
    pub pos: [f32; 3],
    pub sprite_index: i16,
    // Port-only: the original tests the sprite pointer at +0x224.
    pub has_sprite: bool,
    // +0x21A; < 0 = no script, skipped by the range runner (05 §8).
    pub script_index: i16,
    // Port form of script_start / ip (+0x21C / +0x220): (ANM slot, script index) and a byte offset; ip None = finished.
    pub script: Option<(usize, usize)>,
    pub ip: Option<AnmOff>,
    // +0x228 / +0x234: clock and ip saved on interrupt entry, restored by op 89 (05 §4).
    pub saved_time: Timer,
    pub saved_ip: Option<AnmOff>,
    // +0x238..+0x287 interpolation start / end values (05 §2).
    pub pos_start: [f32; 3],
    pub pos_end: [f32; 3],
    pub rot_start: [f32; 3],
    pub rot_end: [f32; 3],
    pub scale_start: [f32; 2],
    pub scale_end: [f32; 2],
    pub color_start: [u8; 4],
    pub color_end: [u8; 4],
    pub color2_start: [u8; 4],
    pub color2_end: [u8; 4],
    // +0x288, written instead of pos when flag bit 8 is set (05 §2).
    pub pos_offset: [f32; 3],
    // +0x294, script time when op 3 last ran.
    pub sprite_set_time: i32,
}

impl Default for AnmVm {
    fn default() -> Self {
        Self {
            rotation: [0.0; 3],
            angular_vel: [0.0; 3],
            scale: [0.0; 2],
            scale_growth: [0.0; 2],
            sprite_w: 0.0,
            sprite_h: 0.0,
            uv_off: [0.0; 2],
            time: Timer::default(),
            wait_timer: Timer::default(),
            interp_elapsed: [Timer::default(); 7],
            interp_duration: [Timer::default(); 7],
            interp_mode: [0; 7],
            ivars: [0; 6],
            fvars: [0.0; 4],
            uv_scroll: [0.0; 2],
            world_matrix: crate::camera::Mat4::IDENTITY,
            color: [0; 4],
            color2: [0; 4],
            flags: 0,
            tag: 0,
            pending_interrupt: 0,
            draw_mode: 0,
            anm_slot: 0,
            pos: [0.0; 3],
            sprite_index: 0,
            has_sprite: false,
            script_index: -1,
            script: None,
            ip: None,
            saved_time: Timer::default(),
            saved_ip: None,
            pos_start: [0.0; 3],
            pos_end: [0.0; 3],
            rot_start: [0.0; 3],
            rot_end: [0.0; 3],
            scale_start: [0.0; 2],
            scale_end: [0.0; 2],
            color_start: [0; 4],
            color_end: [0; 4],
            color2_start: [0; 4],
            color2_end: [0; 4],
            pos_offset: [0.0; 3],
            sprite_set_time: 0,
        }
    }
}

impl AnmVm {
    // AnmVm_Init (05 §8): zero the first 0x208 bytes (fields before `pos`), scale 1, colour 0xFFFFFFFF, flags 7, time 0.
    pub fn init(&mut self) {
        let keep = std::mem::take(self);
        *self = AnmVm {
            pos: keep.pos,
            sprite_index: keep.sprite_index,
            has_sprite: keep.has_sprite,
            anm_slot: keep.anm_slot,
            script_index: keep.script_index,
            script: keep.script,
            ip: keep.ip,
            saved_time: keep.saved_time,
            saved_ip: keep.saved_ip,
            pos_start: keep.pos_start,
            pos_end: keep.pos_end,
            rot_start: keep.rot_start,
            rot_end: keep.rot_end,
            scale_start: keep.scale_start,
            scale_end: keep.scale_end,
            color_start: keep.color_start,
            color_end: keep.color_end,
            color2_start: keep.color2_start,
            color2_end: keep.color2_end,
            pos_offset: keep.pos_offset,
            sprite_set_time: keep.sprite_set_time,
            ..AnmVm::default()
        };
        self.scale = [1.0, 1.0];
        self.color = D3dColor(0xFFFF_FFFF).to_bgra();
        self.flags = (VmFlags::VISIBLE | VmFlags::ENABLED | VmFlags::ROT_DIRTY).bits();
        self.time.set(0);
    }

    // The flag word as `VmFlags` (05 §2.1); bits without a name are kept.
    pub fn vm_flags(&self) -> VmFlags {
        VmFlags::from_bits_retain(self.flags)
    }

    // Any of the bits of `f` set.
    pub fn has(&self, f: VmFlags) -> bool {
        self.vm_flags().intersects(f)
    }

    pub fn insert(&mut self, f: VmFlags) {
        self.flags |= f.bits();
    }

    pub fn remove(&mut self, f: VmFlags) {
        self.flags &= !f.bits();
    }

    fn toggle(&mut self, f: VmFlags) {
        self.flags ^= f.bits();
    }

    fn set(&mut self, f: VmFlags, on: bool) {
        if on {
            self.insert(f);
        } else {
            self.remove(f);
        }
    }

    pub fn visible(&self) -> bool {
        self.has(VmFlags::VISIBLE)
    }

    pub fn set_flag(&mut self, f: u32, on: bool) {
        self.set(VmFlags::from_bits_retain(f), on);
    }

    pub fn blend(&self) -> u32 {
        (self.flags & VmFlags::BLEND.bits()) >> F_BLEND_SHIFT
    }

    // Blend bits 4–5 = `b` (ops 16 / 82; `b` ≤ 3).
    fn set_blend(&mut self, b: u32) {
        self.flags = (self.flags & !VmFlags::BLEND.bits()) | (b << F_BLEND_SHIFT);
    }

    // AnmVm_SetSprite 0x45E430 (05 §7): no change when the file or sprite is missing.
    pub fn set_sprite(&mut self, anm: &AnmManager, idx: i32) {
        let slot = table_index(i32::from(self.anm_slot));
        let Some(s) = anm.sprite(slot, table_index(idx)) else {
            return;
        };
        self.sprite_index = idx as i16;
        self.has_sprite = true;
        self.sprite_w = s.w;
        self.sprite_h = s.h;
    }

    // Int argument read 0x45E650 (05 §3): float vars truncate; an unknown id returns the literal.
    fn ival(&self, a: Arg) -> i32 {
        let lit = a.raw.cast_signed();
        if !a.var {
            return lit;
        }
        match var(lit) {
            Some(Var::I(k)) => self.ivars[k],
            Some(Var::F(k)) => self.fvars[k] as i32,
            None => lit,
        }
    }

    // Float argument read (05 §3): the id is stored as a float and truncated before the lookup.
    fn fval(&self, a: Arg) -> f32 {
        let lit = f32::from_bits(a.raw);
        if !a.var {
            return lit;
        }
        match var(lit as i32) {
            Some(Var::I(k)) => self.ivars[k] as f32,
            Some(Var::F(k)) => self.fvars[k],
            None => lit,
        }
    }

    fn ivals<const N: usize>(&self, a: [Arg; N]) -> [i32; N] {
        a.map(|a| self.ival(a))
    }

    fn fvals<const N: usize>(&self, a: [Arg; N]) -> [f32; N] {
        a.map(|a| self.fval(a))
    }

    // Destinations redirect only with mask bit 0 and a variable of the right kind (05 §3).
    fn idest(a: Arg) -> Dest {
        match var(a.raw.cast_signed()) {
            Some(Var::I(k)) if a.var => Dest::I(k),
            _ => Dest::Lit(a.raw),
        }
    }

    fn fdest(a: Arg) -> Dest {
        match var(f32::from_bits(a.raw) as i32) {
            Some(Var::F(k)) if a.var => Dest::F(k),
            _ => Dest::Lit(a.raw),
        }
    }

    fn read_i(&self, d: Dest) -> i32 {
        match d {
            Dest::I(i) => self.ivars[i],
            Dest::F(i) => self.fvars[i] as i32,
            Dest::Lit(raw) => raw.cast_signed(),
        }
    }

    fn read_f(&self, d: Dest) -> f32 {
        match d {
            Dest::I(i) => self.ivars[i] as f32,
            Dest::F(i) => self.fvars[i],
            Dest::Lit(raw) => f32::from_bits(raw),
        }
    }

    // Literal destinations write into the shared script bytes, so the write persists for every user (05 §3).
    fn write(&mut self, anm: &mut AnmManager, ip: AnmOff, d: Dest, bits: u32, is_float: bool) {
        match d {
            Dest::I(i) => {
                self.ivars[i] = if is_float {
                    f32::from_bits(bits) as i32
                } else {
                    bits.cast_signed()
                };
            }
            Dest::F(i) => {
                self.fvars[i] = if is_float {
                    f32::from_bits(bits)
                } else {
                    bits.cast_signed() as f32
                };
            }
            Dest::Lit(_) => {
                let o = ip.at() + 8;
                if let Some((slot, idx)) = self.script
                    && let Some(s) = anm.script_mut(slot, idx)
                    && let Some(b) = s.data.get_mut(o..o + 4)
                {
                    b.copy_from_slice(&bits.to_le_bytes());
                }
            }
        }
    }

    // Only the low byte (signed char) of the mode is kept (05 §5 notes).
    fn start_slot(&mut self, slot: usize, dur: i32, mode: i32) {
        self.interp_mode[slot] = lo8(mode);
        self.interp_elapsed[slot].set(0);
        self.interp_duration[slot].set(dur);
    }

    fn pos_target(&mut self) -> &mut [f32; 3] {
        if self.has(VmFlags::POS_OFFSET) {
            &mut self.pos_offset
        } else {
            &mut self.pos
        }
    }

    // Alpha interpolation started by an owner, from the current alpha (as op 34).
    pub fn interp_alpha(&mut self, dur: i32, mode: i32, end: u8) {
        self.color_start[A] = self.color[A];
        self.color_end[A] = end;
        self.start_slot(SLOT_ALPHA, dur, mode);
    }

    // Position interpolation started by an owner (as ops 17/18/32).
    pub fn interp_pos(&mut self, dur: i32, mode: i32, end: [f32; 3]) {
        self.start_pos(dur, mode, end);
    }

    // Bomb helpers 0x40EC30 / 0x40EDA0 / 0x40ED50 / 0x40ECA0 (14 §1.4): explicit start and end values.
    pub fn interp_pos_from(&mut self, dur: i32, mode: i32, start: [f32; 3], end: [f32; 3]) {
        self.pos_start = start;
        self.pos_end = end;
        self.start_slot(SLOT_POS, dur, mode);
    }

    pub fn interp_scale_from(&mut self, dur: i32, mode: i32, start: [f32; 2], end: [f32; 2]) {
        self.scale_start = start;
        self.scale_end = end;
        self.start_slot(SLOT_SCALE, dur, mode);
        self.insert(VmFlags::SCALE_DIRTY);
    }

    pub fn interp_alpha_from(&mut self, dur: i32, mode: i32, start: u8, end: u8) {
        self.color_start[A] = start;
        self.color_end[A] = end;
        self.start_slot(SLOT_ALPHA, dur, mode);
    }

    // `start` / `end` are D3DCOLORs; only their R, G, B bytes are used.
    pub fn interp_color_from(&mut self, dur: i32, mode: i32, start: u32, end: u32) {
        let (s, e) = (D3dColor(start).to_bgra(), D3dColor(end).to_bgra());
        for c in [R, G, B] {
            self.color_start[c] = s[c];
            self.color_end[c] = e[c];
        }
        self.start_slot(SLOT_COLOR, dur, mode);
    }

    fn start_pos(&mut self, dur: i32, mode: i32, end: [f32; 3]) {
        self.pos_start = *self.pos_target();
        self.pos_end = end;
        self.start_slot(SLOT_POS, dur, mode);
    }

    // AnmVm_Run 0x45EA00 (05 §4): runs every instruction with time ≤ clock, then POST; returns true when finished.
    pub fn run(&mut self, anm: &mut AnmManager, rng: &mut crate::rng::Rng) -> bool {
        let Some(mut ip) = self.ip else { return true };
        if self.has(VmFlags::PAUSED) {
            return false;
        }
        let clk = Clock {
            speed: anm.speed,
            restore: anm.slowdown_restore,
        };
        let mut interrupt = self.pending_interrupt != 0;
        loop {
            if interrupt {
                interrupt = false;
                // DISPATCH_INTERRUPT (05 §4): with no label the clock is decremented, so POST leaves it unchanged.
                let Some(target) = self.find_label(anm) else {
                    self.time.dec(clk.speed, clk.restore);
                    break;
                };
                self.saved_time = self.time;
                self.saved_ip = Some(ip);
                ip = target;
                let t = self.ins_at(anm, ip).map_or(0, |i| i.time);
                self.time.set(i32::from(t));
                self.insert(VmFlags::VISIBLE);
                continue;
            }
            let Some(d) = self.script_data(anm) else {
                self.ip = None;
                return true;
            };
            let Some(ins) = Ins::read(d, ip) else {
                self.ip = None;
                return true;
            };
            if self.time.cur < i32::from(ins.time) {
                break;
            }
            let op = Op::decode(&ins, d);
            match self.exec_op(anm, rng, ip, op, clk) {
                Step::Next => ip = ip.add(usize::from(ins.size).max(8)),
                Step::Jump(to) => ip = to,
                Step::Post => break,
                Step::Interrupt => interrupt = true,
                Step::Done => {
                    self.ip = None;
                    return true;
                }
            }
        }
        self.ip = Some(ip);
        self.post(clk.speed);
        // Statistics only (05 §4).
        anm.vm_counter += 1;
        false
    }

    // One instruction of the 05 §5 table at `ip`.
    fn exec_op(
        &mut self,
        anm: &mut AnmManager,
        rng: &mut crate::rng::Rng,
        ip: AnmOff,
        op: Op,
        clk: Clock,
    ) -> Step {
        match op {
            Op::Flow(f) => return self.ops_flow(anm, ip, f, clk),
            Op::Sprite(s) => self.ops_sprite(anm, s),
            Op::Interp(i) => self.ops_interp(i),
            Op::Var(v) => self.ops_var(anm, rng, ip, v),
            // Port-only: all data opcodes are known (05 §11), unknown ones are skipped.
            Op::Unknown(op) => log::warn!("anm: unknown opcode {op}"),
        }
        Step::Next
    }

    // Jump of ops 4 / 5 / 67–78: clock = T, ip = script start + O, continue in the same frame (05 §4).
    fn jump(&mut self, to: Arg, time: Arg) -> Step {
        self.time.set(self.ival(time));
        Step::Jump(AnmOff(self.ival(to).cast_unsigned()))
    }

    // Flow ops (05 §4, §5): −1, 0–2, 4, 5, 20, 21, 23, 67–79, 89.
    fn ops_flow(&mut self, anm: &mut AnmManager, ip: AnmOff, op: FlowOp, clk: Clock) -> Step {
        match op {
            // 05 §5 ops −1 / 1.
            FlowOp::End => {
                self.remove(VmFlags::VISIBLE);
                Step::Done
            }
            // Op 2 stop keeps the visible bit.
            FlowOp::Stop => Step::Done,
            // Ops 0 / 21.
            FlowOp::Nop => Step::Next,
            // Op 4.
            FlowOp::Jump { to, time } => self.jump(to, time),
            // Op 5.
            FlowOp::LoopDec { var, to, time } => {
                let d = Self::idest(var);
                // Op 5: a literal counter is written back into the shared script bytes (05 §3).
                let v = self.read_i(d).wrapping_sub(1);
                self.write(anm, ip, d, v.cast_unsigned(), false);
                if v > 0 {
                    self.jump(to, time)
                } else {
                    Step::Next
                }
            }
            // Ops 20 / 23.
            FlowOp::WaitInterrupt { hide } => {
                if hide {
                    self.remove(VmFlags::VISIBLE);
                }
                // Yield (05 §4): decrement now, POST ticks again, so the same instruction re-runs next frame.
                if self.pending_interrupt == 0 {
                    self.insert(VmFlags::WAIT_INT);
                    self.time.dec(clk.speed, clk.restore);
                    Step::Post
                } else {
                    Step::Interrupt
                }
            }
            // Ops 67–78.
            FlowOp::Branch {
                cmp,
                float,
                a,
                b,
                to,
                time,
            } => {
                let take = if float {
                    compare(cmp, self.fval(a), self.fval(b))
                } else {
                    compare(cmp, self.ival(a), self.ival(b))
                };
                if take {
                    self.jump(to, time)
                } else {
                    Step::Next
                }
            }
            // Op 79 wait: the first run sets the timer, later runs decrement it; the clock is held meanwhile (05 §5).
            FlowOp::Wait(n) => {
                if self.wait_timer.cur == 0 {
                    self.wait_timer.set(self.ival(n));
                } else {
                    self.wait_timer.dec(clk.speed, clk.restore);
                }
                if self.wait_timer.cur > 0 {
                    self.time.dec(clk.speed, clk.restore);
                    Step::Post
                } else {
                    self.wait_timer.set(0);
                    Step::Next
                }
            }
            // Op 89: clock and ip as saved on interrupt entry, so the interrupted instruction runs again (05 §5).
            FlowOp::ReturnInterrupt => {
                self.time = self.saved_time;
                // Port-only guard: no saved ip (op 89 without an interrupt) ends the script.
                self.saved_ip.map_or(Step::Done, Step::Jump)
            }
        }
    }

    // Direct state ops (05 §5): 3, 6–14, 16, 22, 24–28, 30, 31, 80–85, 88.
    fn ops_sprite(&mut self, anm: &AnmManager, op: SpriteOp) {
        match op {
            // Op 3.
            SpriteOp::Sprite(a) => {
                self.insert(VmFlags::VISIBLE);
                let idx = self.ival(a);
                self.set_sprite(anm, idx);
                // Op 3 sets visible even when the sprite lookup fails (05 §5, §7).
                self.sprite_set_time = self.time.cur;
            }
            // Op 6.
            SpriteOp::Pos(p) => {
                let p = self.fvals(p);
                *self.pos_target() = p;
            }
            // Op 7.
            SpriteOp::Scale(s) => {
                self.scale = self.fvals(s);
                self.insert(VmFlags::SCALE_DIRTY);
            }
            // Ops 8 / 85.
            SpriteOp::Alpha { second, a } => {
                let v = lo8(self.ival(a));
                if second {
                    self.color2[A] = v;
                } else {
                    self.color[A] = v;
                }
            }
            // Ops 9 / 84.
            SpriteOp::Color { second, rgb } => {
                let [r, g, b] = self.ivals(rgb).map(lo8);
                let c = if second {
                    &mut self.color2
                } else {
                    &mut self.color
                };
                (c[R], c[G], c[B]) = (r, g, b);
            }
            // Op 10.
            SpriteOp::FlipX => {
                self.toggle(VmFlags::FLIP_X);
                self.scale[0] = -self.scale[0];
                self.insert(VmFlags::SCALE_DIRTY);
            }
            // Op 11.
            SpriteOp::FlipY => {
                self.toggle(VmFlags::FLIP_Y);
                self.scale[1] = -self.scale[1];
                self.insert(VmFlags::SCALE_DIRTY);
            }
            // Ops 12/13 both set bit 2 (05 §5).
            SpriteOp::Rotation(v) => {
                self.rotation = self.fvals(v);
                self.insert(VmFlags::ROT_DIRTY);
            }
            SpriteOp::AngularVel(v) => {
                self.angular_vel = self.fvals(v);
                self.insert(VmFlags::ROT_DIRTY);
            }
            // Op 14.
            SpriteOp::ScaleGrowth(v) => self.scale_growth = self.fvals(v),
            // Op 16.
            SpriteOp::BlendAdditive(b) => {
                let b = u32::from(self.ival(b) != 0);
                self.set_blend(b);
            }
            // Op 22.
            SpriteOp::AnchorTopLeft => self.insert(VmFlags::ANCHOR_X | VmFlags::ANCHOR_Y),
            // Op 24.
            SpriteOp::PosTargetOffset(b) => {
                let b = self.ival(b);
                self.set(VmFlags::POS_OFFSET, b & 1 != 0);
            }
            // Op 25.
            SpriteOp::SetTag(v) => self.tag = lo16(self.ival(v)),
            // Ops 26 / 27.
            SpriteOp::UvAdd { axis, d } => {
                self.uv_off[axis] = wrap_uv(self.uv_off[axis] + self.fval(d));
            }
            // Op 28.
            SpriteOp::Visible(b) => {
                let b = self.ival(b);
                self.set(VmFlags::VISIBLE, b & 1 != 0);
            }
            // Op 30.
            SpriteOp::ZwriteOff(b) => {
                let b = self.ival(b);
                self.set(VmFlags::ZWRITE_OFF, b & 1 != 0);
            }
            // Op 31.
            SpriteOp::StageCamera(b) => {
                let b = self.ival(b);
                self.set(VmFlags::STAGE_CAM, b & 1 != 0);
            }
            // Ops 80 / 81.
            SpriteOp::UvScroll { axis, s } => self.uv_scroll[axis] = self.fval(s),
            // Op 82.
            SpriteOp::BlendMode(m) => {
                let b = (self.ival(m) & 3).cast_unsigned();
                self.set_blend(b);
            }
            // Op 83 reads its argument raw, without variable support (05 §5).
            SpriteOp::DrawMode(raw) => self.draw_mode = raw.cast_signed(),
            // Op 88 takes bit 8 of the argument, not bit 0: the original reads the arg's second byte (05 §5).
            SpriteOp::UseColor2(b) => {
                let b = self.ival(b);
                self.set(VmFlags::COLOR2, (b >> 8) & 1 != 0);
            }
        }
    }

    // Interpolation starts (05 §5): 15, 17–19, 29, 32–36, 86, 87.
    fn ops_interp(&mut self, op: InterpOp) {
        match op {
            // Op 15.
            InterpOp::AlphaFade { a, dur } => {
                self.color_start[A] = self.color[A];
                self.color_end[A] = lo8(self.ival(a));
                let dur = self.ival(dur);
                self.start_slot(SLOT_ALPHA, dur, 0);
            }
            // Ops 17/18/19 = easing modes 0/4/6 (05 §5).
            InterpOp::PosFixed { mode, end, dur } => {
                let end = self.fvals(end);
                let dur = self.ival(dur);
                self.start_pos(dur, mode, end);
            }
            // Op 29.
            InterpOp::ScaleLinear { end, dur } => {
                self.scale_start = self.scale;
                self.scale_end = self.fvals(end);
                let dur = self.ival(dur);
                self.start_slot(SLOT_SCALE, dur, 0);
            }
            // Op 32.
            InterpOp::Pos { dur, mode, end } => {
                let (dur, mode) = (self.ival(dur), self.ival(mode));
                let end = self.fvals(end);
                self.start_pos(dur, mode, end);
            }
            // Ops 33 / 86.
            InterpOp::Color {
                second,
                dur,
                mode,
                rgb,
            } => {
                let (dur, mode) = (self.ival(dur), self.ival(mode));
                let rgb = self.ivals(rgb).map(lo8);
                let (cur, start, end, slot) = if second {
                    (
                        self.color2,
                        &mut self.color2_start,
                        &mut self.color2_end,
                        SLOT_COLOR2,
                    )
                } else {
                    (
                        self.color,
                        &mut self.color_start,
                        &mut self.color_end,
                        SLOT_COLOR,
                    )
                };
                (start[R], start[G], start[B]) = (cur[R], cur[G], cur[B]);
                (end[R], end[G], end[B]) = (rgb[0], rgb[1], rgb[2]);
                self.start_slot(slot, dur, mode);
            }
            // Ops 34 / 87.
            InterpOp::Alpha {
                second,
                dur,
                mode,
                a,
            } => {
                let (dur, mode, v) = (self.ival(dur), self.ival(mode), lo8(self.ival(a)));
                if second {
                    self.color2_start[A] = self.color2[A];
                    self.color2_end[A] = v;
                    self.start_slot(SLOT_ALPHA2, dur, mode);
                } else {
                    self.color_start[A] = self.color[A];
                    self.color_end[A] = v;
                    self.start_slot(SLOT_ALPHA, dur, mode);
                }
            }
            // Op 35.
            InterpOp::Rotation { dur, mode, end } => {
                let (dur, mode) = (self.ival(dur), self.ival(mode));
                self.rot_start = self.rotation;
                self.rot_end = self.fvals(end);
                self.start_slot(SLOT_ROT, dur, mode);
                self.insert(VmFlags::ROT_DIRTY);
            }
            // Op 36.
            InterpOp::Scale { dur, mode, end } => {
                let (dur, mode) = (self.ival(dur), self.ival(mode));
                self.scale_start = self.scale;
                self.scale_end = self.fvals(end);
                self.start_slot(SLOT_SCALE, dur, mode);
                self.insert(VmFlags::SCALE_DIRTY);
            }
        }
    }

    // Variable ops 37–66 (05 §3, §5); the destination is arg 0 of the instruction at `ip`.
    fn ops_var(&mut self, anm: &mut AnmManager, rng: &mut crate::rng::Rng, ip: AnmOff, op: VarOp) {
        match op {
            // Odd ops 37–57.
            VarOp::Arith {
                f,
                float: false,
                dst,
                b,
                c,
            } => {
                let d = Self::idest(dst);
                let x = self.read_i(d);
                let (b, c) = (self.ival(b), self.ival(c));
                self.write(anm, ip, d, int_arith(f, x, b, c).cast_unsigned(), false);
            }
            // Even ops 38–58.
            VarOp::Arith {
                f,
                float: true,
                dst,
                b,
                c,
            } => {
                let d = Self::fdest(dst);
                let x = self.read_f(d);
                let (b, c) = (self.fval(b), self.fval(c));
                self.write(anm, ip, d, float_arith(f, x, b, c).to_bits(), true);
            }
            // Ops 59/60 draw from the global game RNG g_Rng 0x164D520 (05 §5 notes); Rng_Range(0) draws nothing (05 §12.3).
            VarOp::IRand { dst, n } => {
                let d = Self::idest(dst);
                let n = self.ival(n);
                let v = rng.range(n);
                self.write(anm, ip, d, v.cast_unsigned(), false);
            }
            VarOp::FRand { dst, x } => {
                let d = Self::fdest(dst);
                let x = self.fval(x);
                let v = rng.next_f32() * x;
                self.write(anm, ip, d, v.to_bits(), true);
            }
            // Ops 61–65, rounded once from the extended x87 result (05 §10).
            VarOp::Math { f, dst, x } => {
                let d = Self::fdest(dst);
                let x = self.fval(x);
                let v = match f {
                    MathFn::Sin => crate::fmath::sin_f(x),
                    MathFn::Cos => crate::fmath::cos_f(x),
                    MathFn::Tan => crate::fmath::tan_f(x),
                    MathFn::Acos => crate::fmath::acos_f(x),
                    MathFn::Atan => crate::fmath::atan_f(x),
                };
                self.write(anm, ip, d, v.to_bits(), true);
            }
            // Op 66.
            VarOp::NormAngle { dst } => {
                let d = Self::fdest(dst);
                let v = wrap_add(self.read_f(d), 0.0);
                self.write(anm, ip, d, v.to_bits(), true);
            }
        }
    }

    // The script bytes of this VM's (slot, script), if loaded.
    fn script_data<'a>(&self, anm: &'a AnmManager) -> Option<&'a [u8]> {
        let (slot, idx) = self.script?;
        Some(&anm.script(slot, idx)?.data)
    }

    fn ins_at(&self, anm: &AnmManager, ip: AnmOff) -> Option<Ins> {
        Ins::read(self.script_data(anm)?, ip)
    }

    // Clears the request; returns the ip after the matching label or after the last catch-all seen.
    // Scan from the script start for op 21 with the id, stopping at op −1; a −1 label is the catch-all (05 §4).
    fn find_label(&mut self, anm: &AnmManager) -> Option<AnmOff> {
        let want = i32::from(self.pending_interrupt.cast_signed());
        self.pending_interrupt = 0;
        self.remove(VmFlags::WAIT_INT);
        let d = self.script_data(anm)?;
        let mut ip = AnmOff(0);
        let mut catch_all = None;
        while let Some(ins) = Ins::read(d, ip) {
            if ins.op == -1 || ins.size == 0 {
                break;
            }
            let next = ip.add(usize::from(ins.size));
            if ins.op == 21 {
                let id = ins.raw(d, 0).cast_signed();
                if id == want {
                    return Some(next);
                }
                if id == -1 {
                    catch_all = Some(next);
                }
            }
            ip = next;
        }
        catch_all
    }

    // POST (05 §6), in the original order: angular velocity, slots 0..6, scale growth, UV scroll, clock tick.
    fn post(&mut self, speed: f32) {
        for k in 0..3 {
            if self.angular_vel[k] != 0.0 {
                self.rotation[k] = wrap_add(self.rotation[k], speed * self.angular_vel[k]);
                self.insert(VmFlags::ROT_DIRTY);
            }
        }
        for slot in 0..7 {
            if self.interp_duration[slot].cur <= 0 {
                continue;
            }
            self.interp_elapsed[slot].tick(speed);
            // The slot started this frame already updates with elapsed = 1, so the end value lands `dur` frames later.
            let t = if self.interp_elapsed[slot].cur >= self.interp_duration[slot].cur {
                self.interp_duration[slot].set(0);
                1.0
            } else {
                self.interp_elapsed[slot].getf() / self.interp_duration[slot].getf()
            };
            let t = ease(self.interp_mode[slot], t);
            let lerp = |s: f32, e: f32| (e - s) * t + s;
            // Colour bytes: float lerp, then truncation (05 §6).
            #[expect(
                clippy::cast_sign_loss,
                reason = "05 §6: ftol of a lerp between two bytes"
            )]
            let lerp8 = |s: u8, e: u8| lerp(f32::from(s), f32::from(e)) as u8;
            match slot {
                SLOT_POS => {
                    let p = std::array::from_fn(|k| lerp(self.pos_start[k], self.pos_end[k]));
                    *self.pos_target() = p;
                }
                SLOT_COLOR => {
                    for c in [B, G, R] {
                        self.color[c] = lerp8(self.color_start[c], self.color_end[c]);
                    }
                }
                SLOT_ALPHA => self.color[A] = lerp8(self.color_start[A], self.color_end[A]),
                SLOT_ROT => {
                    for k in 0..3 {
                        self.rotation[k] = wrap_add(lerp(self.rot_start[k], self.rot_end[k]), 0.0);
                    }
                    self.insert(VmFlags::ROT_DIRTY);
                }
                SLOT_SCALE => {
                    self.scale = [
                        lerp(self.scale_start[0], self.scale_end[0]),
                        lerp(self.scale_start[1], self.scale_end[1]),
                    ];
                    self.insert(VmFlags::SCALE_DIRTY);
                }
                SLOT_COLOR2 => {
                    for c in [B, G, R] {
                        self.color2[c] = lerp8(self.color2_start[c], self.color2_end[c]);
                    }
                }
                _ => self.color2[A] = lerp8(self.color2_start[A], self.color2_end[A]),
            }
        }
        // Quirk kept (05 §6 step 3): y grows before x, and the x branch also sets the rotation-dirty bit.
        if self.scale_growth[1] != 0.0 {
            self.scale[1] += speed * self.scale_growth[1];
            self.insert(VmFlags::SCALE_DIRTY);
        }
        if self.scale_growth[0] != 0.0 {
            self.scale[0] += speed * self.scale_growth[0];
            self.insert(VmFlags::SCALE_DIRTY | VmFlags::ROT_DIRTY);
        }
        for k in 0..2 {
            self.uv_off[k] = wrap_uv(self.uv_off[k] + self.uv_scroll[k]);
        }
        self.time.tick(speed);
    }
}

// Branch compare of ops 67–78 (05 §5).
fn compare<T: PartialOrd + Copy>(cmp: Cmp, x: T, y: T) -> bool {
    match cmp {
        Cmp::Eq => x == y,
        Cmp::Ne => x != y,
        Cmp::Lt => x < y,
        Cmp::Le => x <= y,
        Cmp::Gt => x > y,
        Cmp::Ge => x >= y,
    }
}

// Int ops 37–57 (05 §5): `x` the destination's value, `b` / `c` args 1 / 2; wrapping like x86 32-bit ALU ops.
fn int_arith(f: Arith, x: i32, b: i32, c: i32) -> i32 {
    match f {
        Arith::Set => b,
        Arith::Add => x.wrapping_add(b),
        Arith::Sub => x.wrapping_sub(b),
        Arith::Mul => x.wrapping_mul(b),
        // Divide by zero crashes the original; unreachable in the data (05 §5 ops 45–58), yields 0 here.
        Arith::Div => x.checked_div(b).unwrap_or(0),
        Arith::Rem => x.checked_rem(b).unwrap_or(0),
        Arith::Add3 => b.wrapping_add(c),
        Arith::Sub3 => b.wrapping_sub(c),
        Arith::Mul3 => b.wrapping_mul(c),
        Arith::Div3 => b.checked_div(c).unwrap_or(0),
        Arith::Rem3 => b.checked_rem(c).unwrap_or(0),
    }
}

// Float ops 38–58 (05 §5).
fn float_arith(f: Arith, x: f32, b: f32, c: f32) -> f32 {
    match f {
        Arith::Set => b,
        Arith::Add => x + b,
        Arith::Sub => x - b,
        Arith::Mul => x * b,
        // Float `%` is fmod like x87 (05 §5); division by 0 gives ±inf/NaN, as on x87 with exceptions masked (05 §10).
        Arith::Div => x / b,
        Arith::Rem => x % b,
        Arith::Add3 => b + c,
        Arith::Sub3 => b - c,
        Arith::Mul3 => b * c,
        Arith::Div3 => b / c,
        Arith::Rem3 => b % c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_dec_prev_only_on_slow_path() {
        let mut t = Timer::default();
        t.set(10);
        t.dec(1.0, false);
        assert_eq!((t.prev, t.cur), (-999, 9));
        t.set(10);
        t.dec(0.5, false);
        assert_eq!((t.prev, t.cur, t.frac), (10, 9, 0.5));
        t.dec(0.5, false);
        assert_eq!((t.prev, t.cur, t.frac), (9, 9, 0.0));
    }

    fn vm_with(script: Vec<u8>) -> (AnmManager, AnmVm) {
        let mut rng = crate::rng::Rng::default();
        let mut anm = AnmManager::default();
        let chunk = th08_formats::anm::Chunk {
            name: "t".into(),
            width: 256,
            height: 256,
            sprites: vec![th08_formats::anm::Sprite {
                id: 0,
                x: 0.0,
                y: 0.0,
                w: 32.0,
                h: 16.0,
            }],
            scripts: vec![th08_formats::anm::Script {
                id: 0,
                data: script,
            }],
            has_texture: false,
        };
        anm.insert(0, "t", vec![chunk], |_| crate::drawlist::TexId(0));
        let mut vm = AnmVm::default();
        anm.set_script(&mut rng, &mut vm, 0, 0);
        (anm, vm)
    }

    fn ins(op: i16, time: i16, mask: u16, args: &[u32]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend(op.to_le_bytes());
        v.extend(((8 + 4 * args.len()) as u16).to_le_bytes());
        v.extend(time.to_le_bytes());
        v.extend(mask.to_le_bytes());
        args.iter().for_each(|a| v.extend(a.to_le_bytes()));
        v
    }

    fn f(x: f32) -> u32 {
        x.to_bits()
    }

    #[test]
    fn timing_interp_and_interrupts() {
        let mut rng = crate::rng::Rng::default();
        let mut s = Vec::new();
        s.extend(ins(3, 0, 0, &[0]));
        s.extend(ins(34, 0, 0, &[4, 0, 100]));
        s.extend(ins(8, 0, 0, &[0]));
        s.extend(ins(6, 2, 0, &[f(10.0), f(20.0), f(0.0)]));
        s.extend(ins(20, 3, 0, &[]));
        s.extend(ins(21, 3, 0, &[7]));
        s.extend(ins(6, 3, 0, &[f(1.0), f(2.0), f(3.0)]));
        s.extend(ins(89, 3, 0, &[]));
        s.extend(ins(-1, 0, 0, &[]));
        let (mut anm, mut vm) = vm_with(s);
        assert_eq!(vm.time.cur, 1);
        assert!(vm.visible() && vm.sprite_w == 32.0);
        // The alpha slot captured 255 before op 8 ran; the first POST already applies t = 1/4.
        #[expect(
            clippy::cast_sign_loss,
            reason = "expected byte as the 05 §6 lerp computes it"
        )]
        let expected = (255.0 - 155.0 * 0.25) as u8;
        assert_eq!(vm.color[A], expected);
        vm.run(&mut anm, &mut rng);
        assert_eq!(vm.pos, [0.0; 3]);
        vm.run(&mut anm, &mut rng);
        assert_eq!(vm.pos, [10.0, 20.0, 0.0]);
        for _ in 0..5 {
            vm.run(&mut anm, &mut rng);
        }
        assert_eq!(vm.time.cur, 3);
        assert!(vm.flags & F_WAIT_INT != 0);
        assert_eq!(vm.color[A], 100);
        vm.pending_interrupt = 7;
        vm.run(&mut anm, &mut rng);
        assert_eq!(vm.pos, [1.0, 2.0, 3.0]);
        assert_eq!(vm.time.cur, 3);
        assert!(vm.ip.is_some());
    }

    #[test]
    fn loop_and_wait() {
        let mut rng = crate::rng::Rng::default();
        let mut s = Vec::new();
        s.extend(ins(37, 0, 1, &[10008, 3]));
        let body = s.len() as u32;
        s.extend(ins(40, 0, 1, &[f(10004.0), f(1.5)]));
        s.extend(ins(79, 0, 0, &[2]));
        s.extend(ins(5, 0, 1, &[10008, body, 0]));
        s.extend(ins(2, 0, 0, &[]));
        let (mut anm, mut vm) = vm_with(s);
        let mut frames = 1;
        while !vm.run(&mut anm, &mut rng) {
            frames += 1;
            assert!(frames < 100);
        }
        assert_eq!(vm.fvars[0], 4.5);
        assert_eq!(vm.ivars[4], 0);
        assert_eq!(frames, 6);
    }

    #[test]
    fn ease_and_wrap() {
        assert_eq!(ease(4, 0.5), 0.75);
        assert_eq!(ease(9, 0.3), 0.3);
        assert!((wrap_add(3.0, 1.0) - (4.0 - 6.283_185_5)).abs() < 1e-6);
        assert!((wrap_add(-3.0, -1.0) - (-4.0 + 6.283_185_5)).abs() < 1e-6);
    }

    #[test]
    fn all_game_opcodes_handled() {
        let mut rng = crate::rng::Rng::default();
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut scripts = 0;
        for n in ar.names().iter().filter(|n| n.ends_with(".anm")) {
            let chunks = th08_formats::anm::parse(&ar.read(n).unwrap()).unwrap();
            for c in &chunks {
                for s in &c.scripts {
                    scripts += 1;
                    let mut ip = AnmOff(0);
                    while let Some(i) = Ins::read(&s.data, ip) {
                        seen.insert(i.op);
                        if i.op == -1 {
                            break;
                        }
                        let sig = signature(i.op)
                            .unwrap_or_else(|| panic!("{n}: unknown opcode {}", i.op));
                        assert_eq!(
                            usize::from(i.size),
                            8 + 4 * sig.len(),
                            "{n}: op {} size",
                            i.op
                        );
                        if sig.contains('O') {
                            let off = AnmOff(i.raw(&s.data, sig.find('O').unwrap()));
                            assert!(
                                Ins::read(&s.data, off).is_some(),
                                "{n}: jump target {off:?}"
                            );
                        }
                        ip = ip.add(usize::from(i.size));
                    }
                }
            }
            let mut anm = AnmManager::default();
            let total = chunks.iter().map(|c| c.scripts.len()).sum::<usize>();
            anm.insert(0, n, chunks, |_| crate::drawlist::TexId(0));
            for k in 0..total {
                let mut vm = AnmVm::default();
                anm.set_script(&mut rng, &mut vm, 0, k);
                for frame in 0..600 {
                    if frame % 97 == 50 {
                        vm.pending_interrupt = [1, 2, 3, 4, 5, 7][frame / 97 % 6];
                    }
                    if vm.run(&mut anm, &mut rng) {
                        break;
                    }
                }
            }
        }
        assert_eq!(scripts, 1151);
        let missing: Vec<i16> = seen
            .iter()
            .copied()
            .filter(|&o| signature(o).is_none())
            .collect();
        assert!(missing.is_empty(), "{missing:?}");
        assert_eq!(seen.len(), 57, "{seen:?}");
    }

    // `VmFlags` keeps the 05 §2.1 bit positions and any unnamed bits of the word.
    #[test]
    fn vm_flag_bits() {
        let bits = [
            (F_VISIBLE, 1),
            (F_ENABLED, 2),
            (F_ROT_DIRTY, 4),
            (F_SCALE_DIRTY, 8),
            (F_POS_OFFSET, 0x100),
            (F_FLIP_X, 0x200),
            (F_FLIP_Y, 0x400),
            (F_ANCHOR_X, 0x800),
            (F_ANCHOR_Y, 0x1000),
            (F_ZWRITE_OFF, 0x2000),
            (F_WAIT_INT, 0x4000),
            (F_STAGE_CAM, 0x8000),
            (F_FREEZE_MATRIX, 0x1_0000),
            (F_COLOR2, 0x2_0000),
            (F_PAUSED, 0x8_0000),
        ];
        for (f, v) in bits {
            assert_eq!(f, v);
        }
        assert_eq!((F_BLEND_SHIFT, VmFlags::BLEND.bits()), (4, 0x30));
        for x in [0, 7, 0x4_00C0, u32::MAX] {
            assert_eq!(VmFlags::from_bits_retain(x).bits(), x);
        }
    }
}
