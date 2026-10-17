use crate::anmmgr::AnmManager;
use std::f32::consts::PI;

pub const F_VISIBLE: u32 = 1 << 0;
pub const F_ENABLED: u32 = 1 << 1;
pub const F_ROT_DIRTY: u32 = 1 << 2;
pub const F_SCALE_DIRTY: u32 = 1 << 3;
pub const F_BLEND_SHIFT: u32 = 4;
pub const F_POS_OFFSET: u32 = 1 << 8;
pub const F_FLIP_X: u32 = 1 << 9;
pub const F_FLIP_Y: u32 = 1 << 10;
pub const F_ANCHOR_X: u32 = 1 << 11;
pub const F_ANCHOR_Y: u32 = 1 << 12;
pub const F_ZWRITE_OFF: u32 = 1 << 13;
pub const F_WAIT_INT: u32 = 1 << 14;
pub const F_STAGE_CAM: u32 = 1 << 15;
pub const F_FREEZE_MATRIX: u32 = 1 << 16;
pub const F_COLOR2: u32 = 1 << 17;
pub const F_PAUSED: u32 = 1 << 19;

const SLOT_POS: usize = 0;
const SLOT_COLOR: usize = 1;
const SLOT_ALPHA: usize = 2;
const SLOT_ROT: usize = 3;
const SLOT_SCALE: usize = 4;
const SLOT_COLOR2: usize = 5;
const SLOT_ALPHA2: usize = 6;

// D3DCOLOR byte order.
pub const B: usize = 0;
pub const G: usize = 1;
pub const R: usize = 2;
pub const A: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timer {
    pub prev: i32,
    pub frac: f32,
    pub cur: i32,
}

impl Default for Timer {
    fn default() -> Self {
        Self { prev: -999, frac: 0.0, cur: 0 }
    }
}

impl Timer {
    pub fn set(&mut self, v: i32) {
        *self = Self { prev: -999, frac: 0.0, cur: v };
    }

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

    pub fn dec(&mut self, speed: f32) {
        if slowdown_restore() {
            self.cur -= 1;
            self.frac = 0.0;
            self.prev = -999;
        }
        if speed > 0.99 {
            self.cur -= 1;
        } else {
            self.frac -= speed;
            while self.frac < 0.0 {
                self.cur -= 1;
                self.frac += 1.0;
            }
        }
    }

    pub fn getf(&self) -> f32 {
        self.cur as f32 + self.frac
    }
}

thread_local! {
    static SLOWDOWN_RESTORE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// Supervisor flag 0x20 (05 §12.6): set by ECL native 29, never cleared; Timer_Sub steps once more.
pub fn set_slowdown_restore(on: bool) {
    SLOWDOWN_RESTORE.with(|f| f.set(on));
}

pub fn slowdown_restore() -> bool {
    SLOWDOWN_RESTORE.with(|f| f.get())
}

pub fn wrap_add(a: f32, d: f32) -> f32 {
    const TWO_PI: f32 = 6.2831855;
    let mut a = a + d;
    for _ in 0..18 {
        if a <= PI {
            break;
        }
        a -= TWO_PI;
    }
    for _ in 0..18 {
        if a >= -PI {
            break;
        }
        a += TWO_PI;
    }
    a
}

fn wrap_uv(u: f32) -> f32 {
    if u >= 1.0 {
        u - 1.0
    } else if u < 0.0 {
        u + 1.0
    } else {
        u
    }
}

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

// Argument kinds: i int, f float, V int dest, W float dest, O offset, T time.
pub fn signature(op: i16) -> Option<&'static str> {
    Some(match op {
        -1 | 0 | 1 | 2 | 10 | 11 | 20 | 22 | 23 | 89 => "",
        3 | 8 | 16 | 21 | 24 | 25 | 28 | 30 | 31 | 79 | 82 | 83 | 85 | 88 => "i",
        4 => "OT",
        5 => "VOT",
        6 | 12 | 13 => "fff",
        7 | 14 => "ff",
        9 | 84 => "iii",
        15 => "ii",
        17..=19 => "fffi",
        26 | 27 | 80 | 81 => "f",
        29 => "ffi",
        32 | 35 => "iifff",
        33 | 86 => "iiiii",
        34 | 87 => "iii",
        36 => "iiff",
        37 | 59 => "Vi",
        38 | 60..=65 => "Wf",
        39..=48 if op % 2 == 1 => "Vi",
        39..=48 => "Wf",
        49..=58 if op % 2 == 1 => "Vii",
        49..=58 => "Wff",
        66 => "W",
        67..=78 if op % 2 == 1 => "iiOT",
        67..=78 => "ffOT",
        _ => return None,
    })
}

pub fn disasm(data: &[u8]) -> String {
    let mut s = String::new();
    let mut ip = 0;
    while let Some(ins) = Ins::read(data, ip) {
        let sig = signature(ins.op).unwrap_or("?");
        let args: Vec<String> = (0..ins.nargs())
            .map(|k| {
                let raw = ins.args[k];
                let v = if ins.mask & (1 << k) != 0 { "$" } else { "" };
                match sig.as_bytes().get(k) {
                    Some(b'f' | b'W') => format!("{v}{}", f32::from_bits(raw)),
                    _ => format!("{v}{}", raw as i32),
                }
            })
            .collect();
        s += &format!("  @{ip:<5} t={:<4} op{:<3} {}\n", ins.time, ins.op, args.join(", "));
        if ins.op == -1 || ins.size == 0 {
            break;
        }
        ip += ins.size;
    }
    s
}

#[derive(Clone, Copy)]
struct Ins {
    op: i16,
    size: usize,
    time: i16,
    mask: u16,
    args: [u32; 8],
}

impl Ins {
    fn read(d: &[u8], ip: usize) -> Option<Ins> {
        let h = d.get(ip..ip + 8)?;
        let size = u16::from_le_bytes([h[2], h[3]]) as usize;
        let mut args = [0u32; 8];
        for (k, a) in args.iter_mut().enumerate() {
            let o = ip + 8 + 4 * k;
            if o + 4 > ip + size.max(8) {
                break;
            }
            *a = u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?);
        }
        Some(Ins {
            op: i16::from_le_bytes([h[0], h[1]]),
            size,
            time: i16::from_le_bytes([h[4], h[5]]),
            mask: u16::from_le_bytes([h[6], h[7]]),
            args,
        })
    }

    fn nargs(&self) -> usize {
        (self.size.saturating_sub(8) / 4).min(8)
    }
}

#[derive(Clone, Copy)]
enum Dest {
    I(usize),
    F(usize),
    Arg(usize),
}

#[derive(Clone, Debug)]
pub struct AnmVm {
    pub rotation: [f32; 3],
    pub angular_vel: [f32; 3],
    pub scale: [f32; 2],
    pub scale_growth: [f32; 2],
    pub sprite_w: f32,
    pub sprite_h: f32,
    pub uv_off: [f32; 2],
    pub time: Timer,
    pub wait_timer: Timer,
    pub interp_elapsed: [Timer; 7],
    pub interp_duration: [Timer; 7],
    pub interp_mode: [u8; 7],
    pub ivars: [i32; 6],
    pub fvars: [f32; 4],
    pub uv_scroll: [f32; 2],
    pub world_matrix: crate::gfx::Mat4,
    pub color: [u8; 4],
    pub color2: [u8; 4],
    pub flags: u32,
    pub tag: u16,
    pub pending_interrupt: u16,
    pub draw_mode: i32,
    pub anm_slot: i16,
    pub pos: [f32; 3],
    pub sprite_index: i16,
    pub has_sprite: bool,
    pub script_index: i16,
    pub script: Option<(usize, usize)>,
    pub ip: Option<usize>,
    pub saved_time: Timer,
    pub saved_ip: Option<usize>,
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
    pub pos_offset: [f32; 3],
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
            world_matrix: crate::gfx::Mat4::IDENTITY,
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

enum Step {
    Next,
    Jump,
    Post,
    Done,
}

impl AnmVm {
    // AnmVm_Init: clears the leading state block (everything up to `pos`), then sets the defaults.
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
        self.color = [255; 4];
        self.flags = F_VISIBLE | F_ENABLED | F_ROT_DIRTY;
        self.time.set(0);
    }

    pub fn visible(&self) -> bool {
        self.flags & F_VISIBLE != 0
    }

    pub fn set_flag(&mut self, f: u32, on: bool) {
        if on {
            self.flags |= f;
        } else {
            self.flags &= !f;
        }
    }

    pub fn blend(&self) -> u32 {
        (self.flags >> F_BLEND_SHIFT) & 3
    }

    pub fn set_sprite(&mut self, anm: &AnmManager, idx: i32) {
        let Some(s) = anm.sprite(self.anm_slot as usize, idx as usize) else { return };
        self.sprite_index = idx as i16;
        self.has_sprite = true;
        self.sprite_w = s.w;
        self.sprite_h = s.h;
    }

    fn ival(&self, raw: u32, var: bool) -> i32 {
        if !var {
            return raw as i32;
        }
        match raw as i32 {
            id @ 10000..=10003 => self.ivars[(id - 10000) as usize],
            id @ 10004..=10007 => self.fvars[(id - 10004) as usize] as i32,
            id @ 10008..=10009 => self.ivars[(id - 10004) as usize],
            v => v,
        }
    }

    fn fval(&self, raw: u32, var: bool) -> f32 {
        let lit = f32::from_bits(raw);
        if !var {
            return lit;
        }
        match lit as i32 {
            id @ 10000..=10003 => self.ivars[(id - 10000) as usize] as f32,
            id @ 10004..=10007 => self.fvars[(id - 10004) as usize],
            id @ 10008..=10009 => self.ivars[(id - 10004) as usize] as f32,
            _ => lit,
        }
    }

    fn idest(&self, ins: &Ins) -> Dest {
        match ins.args[0] as i32 {
            id @ 10000..=10003 if ins.mask & 1 != 0 => Dest::I((id - 10000) as usize),
            id @ 10008..=10009 if ins.mask & 1 != 0 => Dest::I((id - 10004) as usize),
            _ => Dest::Arg(0),
        }
    }

    fn fdest(&self, ins: &Ins) -> Dest {
        match f32::from_bits(ins.args[0]) as i32 {
            id @ 10004..=10007 if ins.mask & 1 != 0 => Dest::F((id - 10004) as usize),
            _ => Dest::Arg(0),
        }
    }

    fn read_i(&self, d: Dest, ins: &Ins) -> i32 {
        match d {
            Dest::I(i) => self.ivars[i],
            Dest::F(i) => self.fvars[i] as i32,
            Dest::Arg(k) => ins.args[k] as i32,
        }
    }

    fn read_f(&self, d: Dest, ins: &Ins) -> f32 {
        match d {
            Dest::I(i) => self.ivars[i] as f32,
            Dest::F(i) => self.fvars[i],
            Dest::Arg(k) => f32::from_bits(ins.args[k]),
        }
    }

    // Literal destinations write into the shared script bytes (the instruction's own argument).
    fn write(&mut self, anm: &mut AnmManager, ip: usize, d: Dest, bits: u32, is_float: bool) {
        match d {
            Dest::I(i) => self.ivars[i] = if is_float { f32::from_bits(bits) as i32 } else { bits as i32 },
            Dest::F(i) => self.fvars[i] = if is_float { f32::from_bits(bits) } else { bits as i32 as f32 },
            Dest::Arg(k) => {
                if let Some((slot, idx)) = self.script
                    && let Some(s) = anm.script_mut(slot, idx)
                    && let Some(b) = s.data.get_mut(ip + 8 + 4 * k..ip + 12 + 4 * k)
                {
                    b.copy_from_slice(&bits.to_le_bytes());
                }
            }
        }
    }

    fn start_slot(&mut self, slot: usize, dur: i32, mode: i32) {
        self.interp_mode[slot] = mode as i8 as u8;
        self.interp_elapsed[slot].set(0);
        self.interp_duration[slot].set(dur);
    }

    fn pos_target(&mut self) -> &mut [f32; 3] {
        if self.flags & F_POS_OFFSET != 0 { &mut self.pos_offset } else { &mut self.pos }
    }

    // Position interpolation started by an owner (as ops 17/18/32).
    pub fn interp_alpha(&mut self, dur: i32, mode: i32, end: u8) {
        self.color_start[A] = self.color[A];
        self.color_end[A] = end;
        self.start_slot(SLOT_ALPHA, dur, mode);
    }

    pub fn interp_pos(&mut self, dur: i32, mode: i32, end: [f32; 3]) {
        self.start_pos(dur, mode, end);
    }

    fn start_pos(&mut self, dur: i32, mode: i32, end: [f32; 3]) {
        self.pos_start = *self.pos_target();
        self.pos_end = end;
        self.start_slot(SLOT_POS, dur, mode);
    }

    // Returns true when the script has finished (ip == NULL).
    pub fn run(&mut self, anm: &mut AnmManager) -> bool {
        let Some(mut ip) = self.ip else { return true };
        if self.flags & F_PAUSED != 0 {
            return false;
        }
        let speed = anm.speed;
        let mut interrupt = self.pending_interrupt != 0;
        loop {
            if interrupt {
                interrupt = false;
                match self.find_label(anm) {
                    Some(target) => {
                        self.saved_time = self.time;
                        self.saved_ip = Some(ip);
                        ip = target;
                        let t = self.ins_at(anm, ip).map_or(0, |i| i.time);
                        self.time.set(t as i32);
                        self.flags |= F_VISIBLE;
                        continue;
                    }
                    None => {
                        self.time.dec(speed);
                        break;
                    }
                }
            }
            let Some(ins) = self.ins_at(anm, ip) else {
                self.ip = None;
                return true;
            };
            if self.time.cur < ins.time as i32 {
                break;
            }
            let a = &ins.args;
            let m = |k: usize| ins.mask & (1 << k) != 0;
            let step = match ins.op {
                -1 | 1 => {
                    self.flags &= !F_VISIBLE;
                    Step::Done
                }
                2 => Step::Done,
                3 => {
                    self.flags |= F_VISIBLE;
                    let idx = self.ival(a[0], m(0));
                    self.set_sprite(anm, idx);
                    self.sprite_set_time = self.time.cur;
                    Step::Next
                }
                4 => {
                    self.time.set(self.ival(a[1], m(1)));
                    ip = self.ival(a[0], m(0)) as usize;
                    Step::Jump
                }
                5 => {
                    let d = self.idest(&ins);
                    let v = self.read_i(d, &ins).wrapping_sub(1);
                    self.write(anm, ip, d, v as u32, false);
                    if v > 0 {
                        self.time.set(self.ival(a[2], m(2)));
                        ip = self.ival(a[1], m(1)) as usize;
                        Step::Jump
                    } else {
                        Step::Next
                    }
                }
                6 => {
                    let p = [self.fval(a[0], m(0)), self.fval(a[1], m(1)), self.fval(a[2], m(2))];
                    *self.pos_target() = p;
                    Step::Next
                }
                7 => {
                    self.scale = [self.fval(a[0], m(0)), self.fval(a[1], m(1))];
                    self.flags |= F_SCALE_DIRTY;
                    Step::Next
                }
                8 => {
                    self.color[A] = self.ival(a[0], m(0)) as u8;
                    Step::Next
                }
                9 | 84 => {
                    let (r, g, b) = (self.ival(a[0], m(0)) as u8, self.ival(a[1], m(1)) as u8, self.ival(a[2], m(2)) as u8);
                    let c = if ins.op == 9 { &mut self.color } else { &mut self.color2 };
                    (c[R], c[G], c[B]) = (r, g, b);
                    Step::Next
                }
                10 => {
                    self.flags ^= F_FLIP_X;
                    self.scale[0] = -self.scale[0];
                    self.flags |= F_SCALE_DIRTY;
                    Step::Next
                }
                11 => {
                    self.flags ^= F_FLIP_Y;
                    self.scale[1] = -self.scale[1];
                    self.flags |= F_SCALE_DIRTY;
                    Step::Next
                }
                12 | 13 => {
                    let v = [self.fval(a[0], m(0)), self.fval(a[1], m(1)), self.fval(a[2], m(2))];
                    if ins.op == 12 { self.rotation = v } else { self.angular_vel = v }
                    self.flags |= F_ROT_DIRTY;
                    Step::Next
                }
                14 => {
                    self.scale_growth = [self.fval(a[0], m(0)), self.fval(a[1], m(1))];
                    Step::Next
                }
                15 => {
                    self.color_start[A] = self.color[A];
                    self.color_end[A] = self.ival(a[0], m(0)) as u8;
                    let dur = self.ival(a[1], m(1));
                    self.start_slot(SLOT_ALPHA, dur, 0);
                    Step::Next
                }
                16 => {
                    let b = (self.ival(a[0], m(0)) != 0) as u32;
                    self.flags = (self.flags & !(3 << F_BLEND_SHIFT)) | (b << F_BLEND_SHIFT);
                    Step::Next
                }
                17..=19 => {
                    let end = [self.fval(a[0], m(0)), self.fval(a[1], m(1)), self.fval(a[2], m(2))];
                    let dur = self.ival(a[3], m(3));
                    self.start_pos(dur, [0, 4, 6][(ins.op - 17) as usize], end);
                    Step::Next
                }
                20 | 23 => {
                    if ins.op == 23 {
                        self.flags &= !F_VISIBLE;
                    }
                    if self.pending_interrupt == 0 {
                        self.flags |= F_WAIT_INT;
                        self.time.dec(speed);
                        Step::Post
                    } else {
                        interrupt = true;
                        continue;
                    }
                }
                0 | 21 => Step::Next,
                22 => {
                    self.flags |= F_ANCHOR_X | F_ANCHOR_Y;
                    Step::Next
                }
                24 => {
                    let b = self.ival(a[0], m(0));
                    self.set_flag(F_POS_OFFSET, b & 1 != 0);
                    Step::Next
                }
                25 => {
                    self.tag = self.ival(a[0], m(0)) as u16;
                    Step::Next
                }
                26 | 27 => {
                    let i = (ins.op - 26) as usize;
                    self.uv_off[i] = wrap_uv(self.uv_off[i] + self.fval(a[0], m(0)));
                    Step::Next
                }
                28 => {
                    let b = self.ival(a[0], m(0));
                    self.set_flag(F_VISIBLE, b & 1 != 0);
                    Step::Next
                }
                29 => {
                    self.scale_start = self.scale;
                    self.scale_end = [self.fval(a[0], m(0)), self.fval(a[1], m(1))];
                    let dur = self.ival(a[2], m(2));
                    self.start_slot(SLOT_SCALE, dur, 0);
                    Step::Next
                }
                30 => {
                    let b = self.ival(a[0], m(0));
                    self.set_flag(F_ZWRITE_OFF, b & 1 != 0);
                    Step::Next
                }
                31 => {
                    let b = self.ival(a[0], m(0));
                    self.set_flag(F_STAGE_CAM, b & 1 != 0);
                    Step::Next
                }
                32 => {
                    let (dur, mode) = (self.ival(a[0], m(0)), self.ival(a[1], m(1)));
                    let end = [self.fval(a[2], m(2)), self.fval(a[3], m(3)), self.fval(a[4], m(4))];
                    self.start_pos(dur, mode, end);
                    Step::Next
                }
                33 | 86 => {
                    let (dur, mode) = (self.ival(a[0], m(0)), self.ival(a[1], m(1)));
                    let rgb = [self.ival(a[2], m(2)) as u8, self.ival(a[3], m(3)) as u8, self.ival(a[4], m(4)) as u8];
                    let (cur, start, end, slot) = if ins.op == 33 {
                        (self.color, &mut self.color_start, &mut self.color_end, SLOT_COLOR)
                    } else {
                        (self.color2, &mut self.color2_start, &mut self.color2_end, SLOT_COLOR2)
                    };
                    (start[R], start[G], start[B]) = (cur[R], cur[G], cur[B]);
                    (end[R], end[G], end[B]) = (rgb[0], rgb[1], rgb[2]);
                    self.start_slot(slot, dur, mode);
                    Step::Next
                }
                34 | 87 => {
                    let (dur, mode, v) = (self.ival(a[0], m(0)), self.ival(a[1], m(1)), self.ival(a[2], m(2)) as u8);
                    if ins.op == 34 {
                        self.color_start[A] = self.color[A];
                        self.color_end[A] = v;
                        self.start_slot(SLOT_ALPHA, dur, mode);
                    } else {
                        self.color2_start[A] = self.color2[A];
                        self.color2_end[A] = v;
                        self.start_slot(SLOT_ALPHA2, dur, mode);
                    }
                    Step::Next
                }
                35 => {
                    let (dur, mode) = (self.ival(a[0], m(0)), self.ival(a[1], m(1)));
                    self.rot_start = self.rotation;
                    self.rot_end = [self.fval(a[2], m(2)), self.fval(a[3], m(3)), self.fval(a[4], m(4))];
                    self.start_slot(SLOT_ROT, dur, mode);
                    self.flags |= F_ROT_DIRTY;
                    Step::Next
                }
                36 => {
                    let (dur, mode) = (self.ival(a[0], m(0)), self.ival(a[1], m(1)));
                    self.scale_start = self.scale;
                    self.scale_end = [self.fval(a[2], m(2)), self.fval(a[3], m(3))];
                    self.start_slot(SLOT_SCALE, dur, mode);
                    self.flags |= F_SCALE_DIRTY;
                    Step::Next
                }
                37..=58 if ins.op % 2 == 1 => {
                    let d = self.idest(&ins);
                    let x = self.read_i(d, &ins);
                    let b = self.ival(a[1], m(1));
                    let c = self.ival(a[2], m(2));
                    let v = match ins.op {
                        37 => b,
                        39 => x.wrapping_add(b),
                        41 => x.wrapping_sub(b),
                        43 => x.wrapping_mul(b),
                        // SPEC-GAP: integer division/remainder by zero is undefined (crash in the original); yields 0.
                        45 => x.checked_div(b).unwrap_or(0),
                        47 => x.checked_rem(b).unwrap_or(0),
                        49 => b.wrapping_add(c),
                        51 => b.wrapping_sub(c),
                        53 => b.wrapping_mul(c),
                        55 => b.checked_div(c).unwrap_or(0),
                        _ => b.checked_rem(c).unwrap_or(0),
                    };
                    self.write(anm, ip, d, v as u32, false);
                    Step::Next
                }
                38..=58 => {
                    let d = self.fdest(&ins);
                    let x = self.read_f(d, &ins);
                    let b = self.fval(a[1], m(1));
                    let c = self.fval(a[2], m(2));
                    let v = match ins.op {
                        38 => b,
                        40 => x + b,
                        42 => x - b,
                        44 => x * b,
                        46 => x / b,
                        48 => x % b,
                        50 => b + c,
                        52 => b - c,
                        54 => b * c,
                        56 => b / c,
                        _ => b % c,
                    };
                    self.write(anm, ip, d, v.to_bits(), true);
                    Step::Next
                }
                59 => {
                    let d = self.idest(&ins);
                    let n = self.ival(a[1], m(1));
                    let v = anm.rng.range(n);
                    self.write(anm, ip, d, v as u32, false);
                    Step::Next
                }
                60..=66 => {
                    let d = self.fdest(&ins);
                    let x = if ins.op == 66 { self.read_f(d, &ins) } else { self.fval(a[1], m(1)) };
                    let v = match ins.op {
                        60 => anm.rng.next_f32() * x,
                        61 => x.sin(),
                        62 => x.cos(),
                        63 => x.tan(),
                        64 => x.acos(),
                        65 => x.atan(),
                        _ => wrap_add(x, 0.0),
                    };
                    self.write(anm, ip, d, v.to_bits(), true);
                    Step::Next
                }
                67..=78 => {
                    let take = if ins.op % 2 == 1 {
                        let (x, y) = (self.ival(a[0], m(0)), self.ival(a[1], m(1)));
                        match ins.op {
                            67 => x == y,
                            69 => x != y,
                            71 => x < y,
                            73 => x <= y,
                            75 => x > y,
                            _ => x >= y,
                        }
                    } else {
                        let (x, y) = (self.fval(a[0], m(0)), self.fval(a[1], m(1)));
                        match ins.op {
                            68 => x == y,
                            70 => x != y,
                            72 => x < y,
                            74 => x <= y,
                            76 => x > y,
                            _ => x >= y,
                        }
                    };
                    if take {
                        self.time.set(self.ival(a[3], m(3)));
                        ip = self.ival(a[2], m(2)) as usize;
                        Step::Jump
                    } else {
                        Step::Next
                    }
                }
                79 => {
                    if self.wait_timer.cur == 0 {
                        self.wait_timer.set(self.ival(a[0], m(0)));
                    } else {
                        self.wait_timer.dec(speed);
                    }
                    if self.wait_timer.cur > 0 {
                        self.time.dec(speed);
                        Step::Post
                    } else {
                        self.wait_timer.set(0);
                        Step::Next
                    }
                }
                80 | 81 => {
                    self.uv_scroll[(ins.op - 80) as usize] = self.fval(a[0], m(0));
                    Step::Next
                }
                82 => {
                    let b = (self.ival(a[0], m(0)) & 3) as u32;
                    self.flags = (self.flags & !(3 << F_BLEND_SHIFT)) | (b << F_BLEND_SHIFT);
                    Step::Next
                }
                83 => {
                    self.draw_mode = a[0] as i32;
                    Step::Next
                }
                85 => {
                    self.color2[A] = self.ival(a[0], m(0)) as u8;
                    Step::Next
                }
                88 => {
                    let b = self.ival(a[0], m(0));
                    self.set_flag(F_COLOR2, (b >> 8) & 1 != 0);
                    Step::Next
                }
                89 => {
                    self.time = self.saved_time;
                    match self.saved_ip {
                        Some(s) => ip = s,
                        None => {
                            self.ip = None;
                            return true;
                        }
                    }
                    Step::Jump
                }
                op => {
                    log::warn!("anm: unknown opcode {op}");
                    Step::Next
                }
            };
            match step {
                Step::Next => ip += ins.size.max(8),
                Step::Jump => {}
                Step::Post => break,
                Step::Done => {
                    self.ip = None;
                    return true;
                }
            }
        }
        self.ip = Some(ip);
        self.post(speed);
        anm.vm_counter += 1;
        false
    }

    fn ins_at(&self, anm: &AnmManager, ip: usize) -> Option<Ins> {
        let (slot, idx) = self.script?;
        Ins::read(&anm.script(slot, idx)?.data, ip)
    }

    // Returns the ip after the matching label (or the last catch-all seen before it); clears the request.
    fn find_label(&mut self, anm: &AnmManager) -> Option<usize> {
        let want = self.pending_interrupt as i16 as i32;
        self.pending_interrupt = 0;
        self.flags &= !F_WAIT_INT;
        let mut ip = 0;
        let mut catch_all = None;
        while let Some(ins) = self.ins_at(anm, ip) {
            if ins.op == -1 || ins.size == 0 {
                break;
            }
            if ins.op == 21 {
                let id = ins.args[0] as i32;
                if id == want {
                    return Some(ip + ins.size);
                }
                if id == -1 {
                    catch_all = Some(ip + ins.size);
                }
            }
            ip += ins.size;
        }
        catch_all
    }

    fn post(&mut self, speed: f32) {
        for k in 0..3 {
            if self.angular_vel[k] != 0.0 {
                self.rotation[k] = wrap_add(self.rotation[k], speed * self.angular_vel[k]);
                self.flags |= F_ROT_DIRTY;
            }
        }
        for slot in 0..7 {
            if self.interp_duration[slot].cur <= 0 {
                continue;
            }
            self.interp_elapsed[slot].tick(speed);
            let t = if self.interp_elapsed[slot].cur >= self.interp_duration[slot].cur {
                self.interp_duration[slot].set(0);
                1.0
            } else {
                self.interp_elapsed[slot].getf() / self.interp_duration[slot].getf()
            };
            let t = ease(self.interp_mode[slot], t);
            let lerp = |s: f32, e: f32| (e - s) * t + s;
            let lerp8 = |s: u8, e: u8| lerp(s as f32, e as f32) as u8;
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
                    self.flags |= F_ROT_DIRTY;
                }
                SLOT_SCALE => {
                    self.scale = [lerp(self.scale_start[0], self.scale_end[0]), lerp(self.scale_start[1], self.scale_end[1])];
                    self.flags |= F_SCALE_DIRTY;
                }
                SLOT_COLOR2 => {
                    for c in [B, G, R] {
                        self.color2[c] = lerp8(self.color2_start[c], self.color2_end[c]);
                    }
                }
                _ => self.color2[A] = lerp8(self.color2_start[A], self.color2_end[A]),
            }
        }
        if self.scale_growth[1] != 0.0 {
            self.scale[1] += speed * self.scale_growth[1];
            self.flags |= F_SCALE_DIRTY;
        }
        if self.scale_growth[0] != 0.0 {
            self.scale[0] += speed * self.scale_growth[0];
            self.flags |= F_SCALE_DIRTY | F_ROT_DIRTY;
        }
        for k in 0..2 {
            self.uv_off[k] = wrap_uv(self.uv_off[k] + self.uv_scroll[k]);
        }
        self.time.tick(speed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm_with(script: Vec<u8>) -> (AnmManager, AnmVm) {
        let mut anm = AnmManager::default();
        let chunk = crate::anm::Chunk {
            name: "t".into(),
            width: 256,
            height: 256,
            sprites: vec![crate::anm::Sprite { id: 0, x: 0.0, y: 0.0, w: 32.0, h: 16.0 }],
            scripts: vec![crate::anm::Script { id: 0, data: script }],
            texture: None,
        };
        anm.insert(0, "t", vec![chunk], |_| 0);
        let mut vm = AnmVm::default();
        anm.set_script(&mut vm, 0, 0);
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
        assert_eq!(vm.color[A], (255.0 - 155.0 * 0.25) as u8);
        vm.run(&mut anm);
        assert_eq!(vm.pos, [0.0; 3]);
        vm.run(&mut anm);
        assert_eq!(vm.pos, [10.0, 20.0, 0.0]);
        for _ in 0..5 {
            vm.run(&mut anm);
        }
        assert_eq!(vm.time.cur, 3);
        assert!(vm.flags & F_WAIT_INT != 0);
        assert_eq!(vm.color[A], 100);
        vm.pending_interrupt = 7;
        vm.run(&mut anm);
        assert_eq!(vm.pos, [1.0, 2.0, 3.0]);
        assert_eq!(vm.time.cur, 3);
        assert!(vm.ip.is_some());
    }

    #[test]
    fn loop_and_wait() {
        let mut s = Vec::new();
        s.extend(ins(37, 0, 1, &[10008, 3]));
        let body = s.len() as u32;
        s.extend(ins(40, 0, 1, &[f(10004.0), f(1.5)]));
        s.extend(ins(79, 0, 0, &[2]));
        s.extend(ins(5, 0, 1, &[10008, body, 0]));
        s.extend(ins(2, 0, 0, &[]));
        let (mut anm, mut vm) = vm_with(s);
        let mut frames = 1;
        while !vm.run(&mut anm) {
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
        assert!((wrap_add(3.0, 1.0) - (4.0 - 6.2831855)).abs() < 1e-6);
        assert!((wrap_add(-3.0, -1.0) - (-4.0 + 6.2831855)).abs() < 1e-6);
    }

    #[test]
    fn all_game_opcodes_handled() {
        let Some(dir) = std::env::var("TH08_DIR").ok() else { return };
        let ar = crate::archive::Archive::from_bytes(std::fs::read(format!("{dir}/th08.dat")).unwrap()).unwrap();
        let mut seen = std::collections::BTreeSet::new();
        let mut scripts = 0;
        for n in ar.names().iter().filter(|n| n.ends_with(".anm")) {
            let chunks = crate::anm::parse(&ar.read(n).unwrap()).unwrap();
            for c in &chunks {
                for s in &c.scripts {
                    scripts += 1;
                    let mut ip = 0;
                    while let Some(i) = Ins::read(&s.data, ip) {
                        seen.insert(i.op);
                        if i.op == -1 {
                            break;
                        }
                        let sig = signature(i.op).unwrap_or_else(|| panic!("{n}: unknown opcode {}", i.op));
                        assert_eq!(i.size, 8 + 4 * sig.len(), "{n}: op {} size", i.op);
                        if sig.contains('O') {
                            let off = i.args[sig.find('O').unwrap()] as usize;
                            assert!(Ins::read(&s.data, off).is_some(), "{n}: jump target {off}");
                        }
                        ip += i.size;
                    }
                }
            }
            let mut anm = AnmManager::default();
            let total = chunks.iter().map(|c| c.scripts.len()).sum::<usize>();
            anm.insert(0, n, chunks, |_| 0);
            for k in 0..total {
                let mut vm = AnmVm::default();
                anm.set_script(&mut vm, 0, k);
                for frame in 0..600 {
                    if frame % 97 == 50 {
                        vm.pending_interrupt = [1, 2, 3, 4, 5, 7][frame / 97 % 6];
                    }
                    if vm.run(&mut anm) {
                        break;
                    }
                }
            }
        }
        assert_eq!(scripts, 1151);
        let missing: Vec<i16> = seen.iter().copied().filter(|&o| signature(o).is_none()).collect();
        assert!(missing.is_empty(), "{missing:?}");
        assert_eq!(seen.len(), 57, "{seen:?}");
    }
}
