//! ECL file format: header, subs, timelines, instruction signatures (07 §2, §7).

#[derive(Debug, thiserror::Error)]
pub enum EclError {
    #[error("not an ECL file")]
    NotEcl,
    #[error("timeline {0}: bad offset {1:#x}")]
    BadTimeline(usize, usize),
    #[error("sub table truncated")]
    SubTableTruncated,
    #[error("sub {0}: bad offset {1:#x}")]
    BadSub(usize, u32),
}

pub type Result<T> = std::result::Result<T, EclError>;

// Header dword 0x00: checked, load fails otherwise.
pub const MAGIC: u32 = 0x800;
// Sub instruction header: time, opcode, size, unused byte, rank mask, var mask.
pub const SUB_HEADER: usize = 12;

#[derive(Clone)]
pub struct Ecl {
    pub data: Vec<u8>,
    // Sub offsets from file start (table at 0x48).
    pub subs: Vec<usize>,
    // The `ntl` timelines that are run (the extra last entry is never run).
    pub timelines: Vec<usize>,
}

// Byte offset of an instruction from the file start (the loader's relocated pointers, kept file-relative).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EclOff(pub u32);

impl EclOff {
    pub const fn idx(self) -> usize {
        self.0 as usize
    }

    // Jump target `this instruction + signed byte offset` (op 4); a negative sum wraps like the original's
    // pointer add and is never a valid offset.
    pub fn rel(self, d: i32) -> usize {
        (i64::from(self.0) + i64::from(d)).cast_unsigned() as usize
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ins {
    pub off: EclOff,
    // +0: runs when the context clock == time exactly (07 §5.3).
    pub time: i32,
    // +4: −1 = end-of-sub marker.
    pub op: i16,
    pub size: usize,
    // +9 rank mask (07 §2.3).
    pub rank: u8,
    // +10: bit k = argument k is a variable id (07 §6); bullet/laser blocks use their own bits (07 §9).
    pub mask: u16,
}

// Timeline instruction: +0 time (< 0 end), +4 op, +6 u8 size, +7 u8 rank mask.
#[derive(Clone, Copy, Debug)]
pub struct TlIns {
    pub off: EclOff,
    pub time: i32,
    pub op: i16,
    pub size: usize,
    pub rank: u8,
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

fn i16_at(d: &[u8], o: usize) -> Option<i16> {
    Some(i16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}

// i16 count clamped to `lo..=hi` (header counts).
fn count(v: i16, lo: i16, hi: i16) -> usize {
    usize::from(v.clamp(lo, hi).cast_unsigned())
}

// Loader 0x418330: offsets stay file-relative instead of being relocated to pointers.
pub fn parse(data: Vec<u8>) -> Result<Ecl> {
    if data.len() < 0x48 || u32_at(&data, 0) != Some(MAGIC) {
        return Err(EclError::NotEcl);
    }
    let nsubs = count(i16_at(&data, 4).unwrap_or(0), 0, i16::MAX);
    let ntl = count(i16_at(&data, 6).unwrap_or(0), 0, 16);
    let mut timelines = Vec::new();
    for k in 0..ntl {
        let o = u32_at(&data, 8 + 4 * k).unwrap_or(0) as usize;
        if o == 0 || o >= data.len() {
            return Err(EclError::BadTimeline(k, o));
        }
        timelines.push(o);
    }
    let mut subs = Vec::new();
    for k in 0..nsubs {
        let Some(o) = u32_at(&data, 0x48 + 4 * k) else {
            return Err(EclError::SubTableTruncated);
        };
        if o as usize >= data.len() {
            return Err(EclError::BadSub(k, o));
        }
        subs.push(o as usize);
    }
    Ok(Ecl {
        data,
        subs,
        timelines,
    })
}

impl Ecl {
    pub fn ins(&self, off: usize) -> Option<Ins> {
        let d = &self.data;
        // A negative i16 size sign-extends (huge), as the original's pointer add would run off the file.
        let size = isize::from(i16_at(d, off + 6)?).cast_unsigned();
        Some(Ins {
            off: EclOff(u32::try_from(off).ok()?),
            time: u32_at(d, off)?.cast_signed(),
            op: i16_at(d, off + 4)?,
            size: size.max(SUB_HEADER),
            rank: *d.get(off + 9)?,
            mask: u16::from_le_bytes(d.get(off + 10..off + 12)?.try_into().ok()?),
        })
    }

    pub fn arg(&self, ins: &Ins, k: usize) -> u32 {
        u32_at(&self.data, ins.off.idx() + SUB_HEADER + 4 * k).unwrap_or(0)
    }

    // Literal destinations write into the loaded file (shared by every enemy running the instruction, 07 §6).
    pub fn set_arg(&mut self, ins: &Ins, k: usize, v: u32) {
        let o = ins.off.idx() + SUB_HEADER + 4 * k;
        if o + 4 <= ins.off.idx() + ins.size
            && let Some(b) = self.data.get_mut(o..o + 4)
        {
            b.copy_from_slice(&v.to_le_bytes());
        }
    }

    pub fn bytes(&self, ins: &Ins) -> &[u8] {
        let o = ins.off.idx();
        &self.data[o..(o + ins.size).min(self.data.len())]
    }

    // The instruction's opcode with its arguments, decoded from the current file bytes.
    pub fn op(&self, ins: &Ins) -> Op {
        Op::decode(ins, &self.data)
    }

    pub fn tl_ins(&self, off: usize) -> Option<TlIns> {
        let d = &self.data;
        Some(TlIns {
            off: EclOff(u32::try_from(off).ok()?),
            time: u32_at(d, off)?.cast_signed(),
            op: i16_at(d, off + 4)?,
            size: usize::from(*d.get(off + 6)?).max(8),
            rank: *d.get(off + 7)?,
        })
    }

    pub fn tl_arg(&self, ins: &TlIns, k: usize) -> u32 {
        u32_at(&self.data, ins.off.idx() + 8 + 4 * k).unwrap_or(0)
    }

    pub fn sub_start(&self, sub: i32) -> Option<usize> {
        usize::try_from(sub)
            .ok()
            .and_then(|s| self.subs.get(s).copied())
    }
}

// --- opcodes (names are the table's mnemonics) ---

// Argument dword k with its var-mask bit: a literal, or a variable id when `var` (07 §6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Arg {
    pub raw: u32,
    pub var: bool,
}

// Destination argument (letters V/W): a writable variable, else the dword in the file at `slot` (07 §6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dst {
    pub slot: u8,
    pub arg: Arg,
}

impl Dst {
    pub fn k(self) -> usize {
        usize::from(self.slot)
    }
}

// Ops 10–29: the op number's last digit (10–14 / 20–24, 15–19 / 25–29).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arith {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

impl Arith {
    fn from_digit(d: i16) -> Self {
        match d {
            0 => Self::Add,
            1 => Self::Sub,
            2 => Self::Mul,
            3 => Self::Div,
            _ => Self::Mod,
        }
    }
}

// Ops 40–51: ije ijne ijl ijle ijg ijge, int / float pairs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

// Ops 90–94 (07 §10.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnKind {
    // 90 child: at (x, y, 0).
    Child,
    // 91 child_rel: final pos + (x, y).
    ChildRel,
    // 92 child_follow: at (x, y, 0), follows the parent.
    ChildFollow,
    // 93 spawn: at (x, y, z), not chained.
    Spawn,
    // 94 spawn_rel: pos + (x, y, z), not chained.
    SpawnRel,
}

impl SpawnKind {
    // Ops 90–92 chain the child to the spawner.
    pub fn chained(self) -> bool {
        matches!(self, Self::Child | Self::ChildRel | Self::ChildFollow)
    }
}

// Control flow, calls, threads, interrupts, natives (07 §5.2–§5.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlowOp {
    // 0, 3, 84, 85.
    Nop,
    // 1.
    Delete,
    // 2.
    Wait(Arg),
    // 4.
    Jump {
        time: Arg,
        to: Arg,
    },
    // 5.
    Loop {
        time: Arg,
        to: Arg,
        counter: Dst,
    },
    // 40–51.
    JumpIf {
        cmp: Cmp,
        float: bool,
        a: Arg,
        b: Arg,
        time: Arg,
        to: Arg,
    },
    // 52.
    Call {
        sub: Arg,
    },
    // 53.
    Return,
    // 88.
    CallBoss {
        slot: Arg,
        sub: Arg,
    },
    // 89.
    InterruptBoss {
        slot: Arg,
        k: Arg,
    },
    // 125.
    InterruptNow(Arg),
    // 126.
    SetInterrupt {
        sub: Arg,
        k: Arg,
    },
    // 135.
    Thread {
        k: Arg,
        sub: Arg,
    },
    // 136.
    Native {
        n: Arg,
        arg: Arg,
    },
    // 137.
    NativeCb {
        n: Arg,
        arg: Arg,
    },
    // 146.
    TimeAdd(Arg),
}

// Variable ops (07 §5.6, §6).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VarOp {
    // 6 / 7.
    ISet {
        dst: Dst,
        v: Arg,
    },
    FSet {
        dst: Dst,
        v: Arg,
    },
    // 8 / 9.
    IRandSign {
        dst: Dst,
        v: Arg,
    },
    FRandSign {
        dst: Dst,
        v: Arg,
    },
    // 10–14 / 15–19: dst op= rhs.
    IArith {
        kind: Arith,
        dst: Dst,
        rhs: Arg,
    },
    FArith {
        kind: Arith,
        dst: Dst,
        rhs: Arg,
    },
    // 20–24 / 25–29: dst = a op b.
    IArith3 {
        kind: Arith,
        dst: Dst,
        a: Arg,
        b: Arg,
    },
    FArith3 {
        kind: Arith,
        dst: Dst,
        a: Arg,
        b: Arg,
    },
    // 30 / 31.
    Inc(Dst),
    Dec(Dst),
    // 32 / 33.
    FSin {
        dst: Dst,
        a: Arg,
    },
    FCos {
        dst: Dst,
        a: Arg,
    },
    // 34 / 39.
    FAtan2 {
        dst: Dst,
        x1: Arg,
        y1: Arg,
        x2: Arg,
        y2: Arg,
    },
    FDist {
        dst: Dst,
        x1: Arg,
        y1: Arg,
        x2: Arg,
        y2: Arg,
    },
    // 35.
    FLerp {
        dst: Dst,
        b: Arg,
        c: Arg,
        d: Arg,
    },
    // 36: the target id is the raw f32 word 0 (07 §5.6).
    Interp {
        target_bits: u32,
        dur: Arg,
        func: Arg,
        mode: Arg,
        abcd: [Arg; 4],
    },
    // 37.
    NormAngle(Dst),
    // 38 / 166.
    Polar {
        x: Dst,
        y: Dst,
        ang: Arg,
        r: Arg,
    },
    PolarYs {
        x: Dst,
        y: Dst,
        ang: Arg,
        r: Arg,
    },
    // 86 / 87.
    IGetBoss {
        dst: Dst,
        var: Arg,
        slot: Arg,
    },
    FGetBoss {
        dst: Dst,
        var: Arg,
        slot: Arg,
    },
    // 169.
    RandAwayAngle(Dst),
}

// Movement (07 §8).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MoveOp {
    SetPos {
        x: Arg,
        y: Arg,
    },
    MoveTo {
        dur: Arg,
        ease: Arg,
        x: Arg,
        y: Arg,
    },
    SetDir {
        ang: Arg,
        spd: Arg,
    },
    MoveDir {
        dur: Arg,
        ease: Arg,
        ang: Arg,
        spd: Arg,
    },
    // 67 / 178.
    MoveRand {
        dur: Arg,
        ease: Arg,
        spd: Arg,
    },
    MoveRand2 {
        dur: Arg,
        ease: Arg,
        spd: Arg,
    },
    SetDirAim {
        off: Arg,
        spd: Arg,
    },
    MoveDirAim {
        dur: Arg,
        ease: Arg,
        off: Arg,
        spd: Arg,
    },
    SetAngVel(Arg),
    SetAccel(Arg),
    Orbit {
        dur: Arg,
        cx: Arg,
        cy: Arg,
        a: Arg,
        av: Arg,
        r: Arg,
        rv: Arg,
    },
    OrbitHere {
        dur: Arg,
        a: Arg,
        av: Arg,
        rv: Arg,
    },
    OrbitVel {
        dur: Arg,
        av: Arg,
        rv: Arg,
    },
    ClampRect([Arg; 4]),
    ClampOff,
    Hitbox {
        w: Arg,
        h: Arg,
    },
    Shotbox {
        w: Arg,
        h: Arg,
    },
    NofireRadius(Arg),
}

// Enemy state: ANM scripts, flags, children, items, effects, HUD requests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EnemyOp {
    // 54 / 58 (`stg`: stgNenm.anm, flags B bit 2).
    Anm {
        script: Arg,
        stg: bool,
    },
    // 55 / 59.
    AnmDir {
        base: Arg,
        stg: bool,
    },
    // 56 / 60.
    AnmDir6 {
        scripts: [Arg; 6],
        stg: bool,
    },
    // 57 / 61.
    SubAnm {
        vm: Arg,
        script: Arg,
        stg: bool,
    },
    // 62.
    AnmReplay,
    // 79 / 80 / 81.
    FlagsSet(Arg),
    FlagsOff(Arg),
    FlagsOn(Arg),
    // 83.
    FlagB1(Arg),
    // 90–94.
    Spawn {
        kind: SpawnKind,
        sub: Arg,
        x: Arg,
        y: Arg,
        z: Option<Arg>,
        life: Arg,
        item: Arg,
        score: Arg,
    },
    // 124.
    Sfx(Arg),
    // 128: words 1–4 raw (mask ignored, word 0 unused).
    Effect13 {
        axis: [f32; 3],
        radius: f32,
    },
    // 138: raw word.
    SetBytes(u32),
    // 139 / 140.
    Effect {
        id: Arg,
        n: Arg,
        colour: Arg,
    },
    EffectDir {
        id: Arg,
        n: Arg,
        colour: Arg,
        param: [Arg; 3],
    },
    // 141 / 142 / 168 / 143 / 144.
    Item(Arg),
    ItemBurst(Arg),
    ItemBurst1(Arg),
    SetItem(Arg),
    ExtraItems {
        point: Arg,
        power: Arg,
    },
    // 145 / 151 / 156: raw words, low byte tested.
    FlagA25(u32),
    NoStack(u32),
    FlagA7(u32),
    // 147 / 148 / 149.
    StdSection(Arg),
    Hud148(Arg),
    AnmInterrupt(Arg),
    // 150: raw words.
    SubAnmInterrupt {
        vm: u32,
        n: u32,
    },
    // 157: word 0 raw.
    Trail {
        flags: u32,
        n: Arg,
        a: Arg,
        b: Arg,
    },
    // 159 / 160 / 163 / 165.
    DrawLayer(Arg),
    DmgTimer(Arg),
    GF54cec(Arg),
    AnmRotZ(Arg),
    // 173 / 182 / 183.
    FlagA30(Arg),
    FlagB8(Arg),
    FlagA31(Arg),
    // 174 / 175 / 177.
    Aura(Arg),
    SpawnDisable(Arg),
    SetHudLife(Arg),
    // 179 / 180 / 181.
    NightClockShow,
    NightClockHide,
    Extend181,
}

// Bullets (07 §9.1–§9.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BulletOp {
    // 96–104: the block is the instruction itself (`BulletBlock`).
    Bullets,
    // 105 / 106.
    Autofire {
        n: Arg,
        rand: bool,
    },
    // 107 / 108.
    AutofireStore(bool),
    // 109.
    FireStored,
    // 110.
    ShotOffset {
        x: Arg,
        y: Arg,
    },
    // 111.
    BulletEx {
        k: Arg,
        a: Arg,
        b: Arg,
        c: Arg,
        d: Arg,
        e: Arg,
        f: Arg,
    },
    // 112 / 162.
    CancelBullets,
    CancelAll,
    // 113.
    BulletSfx {
        id: Arg,
        x: Arg,
    },
    // 152.
    RankParams {
        speed: [Arg; 2],
        counts: [Arg; 4],
    },
    // 161.
    CancelRadius(Arg),
}

// Lasers (07 §9.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LaserOp {
    // 114 / 115: the block is the instruction itself (`LaserBlock`); 114 = fixed.
    Laser { fixed: bool },
    LaserSlot(Arg),
    // 117 / 118 / 167.
    LaserRotate { k: Arg, a: Arg },
    LaserAim { k: Arg, a: Arg },
    LaserSetAngle { k: Arg, a: Arg },
    LaserOffset { k: Arg, off: [Arg; 3] },
    LaserAlive(Arg),
    LaserCancel(Arg),
    // 154.
    LaserClear,
    // 170: word 1 is read as a float and its raw low byte stored.
    LaserHideEmitter { k: Arg, a: Arg },
    // 171 / 172.
    LaserMaxLen { k: Arg, a: Arg },
    LaserLen { k: Arg, tail: Arg, head: Arg },
}

// Boss, spell card, death and timeout (07 §10–§11).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BossOp {
    KillAll,
    // 122: the block is the instruction itself (`SpellBlock`).
    Spellcard,
    SpellcardEnd,
    Boss(Arg),
    // 129 / 130: raw words.
    DeathMode(u32),
    DeathSub(u32),
    SetLife(Arg),
    SetTimer(Arg),
    LifeThreshold { k: Arg, life: Arg, sub: Arg },
    Timeout { frames: Arg, sub: Arg },
    TimeoutSubFromDeath,
    // 155: raw word.
    FixedBonus(u32),
    HpMarker { k: Arg, a: Arg, b: Arg, c: Arg },
    SpellMode(Arg),
    LastSpell,
    SpellEx(Arg),
}

// One decoded sub instruction. `Unknown` = an op outside the table: ignored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Flow(FlowOp),
    Var(VarOp),
    Move(MoveOp),
    Enemy(EnemyOp),
    Bullet(BulletOp),
    Laser(LaserOp),
    Boss(BossOp),
    Unknown(i16),
}

// Argument reader over the whole file (`Ecl::arg`: reads are not bounded by the instruction size).
struct Rd<'a> {
    d: &'a [u8],
    ins: &'a Ins,
}

impl Rd<'_> {
    fn raw(&self, k: usize) -> u32 {
        u32_at(self.d, self.ins.off.idx() + SUB_HEADER + 4 * k).unwrap_or(0)
    }

    fn a(&self, k: usize) -> Arg {
        Arg {
            raw: self.raw(k),
            var: self.ins.mask & (1 << k) != 0,
        }
    }

    fn dst(&self, k: u8) -> Dst {
        Dst {
            slot: k,
            arg: self.a(usize::from(k)),
        }
    }

    fn arr<const N: usize>(&self, from: usize) -> [Arg; N] {
        std::array::from_fn(|k| self.a(from + k))
    }
}

impl Op {
    // Decoded from the current bytes each time the instruction runs: the file stays writable (07 §6 literal
    // destinations).
    pub fn decode(ins: &Ins, data: &[u8]) -> Self {
        let r = Rd { d: data, ins };
        let op = ins.op;
        // One switch on the opcode picks the group, then the group's own switch.
        match op {
            0..=5 | 40..=53 | 84 | 85 | 88 | 89 | 125 | 126 | 135..=137 | 146 => {
                FlowOp::decode(op, &r).map(Op::Flow)
            }
            6..=39 | 86 | 87 | 166 | 169 => VarOp::decode(op, &r).map(Op::Var),
            63..=78 | 82 | 178 => MoveOp::decode(op, &r).map(Op::Move),
            96..=113 | 152 | 161 | 162 => BulletOp::decode(op, &r).map(Op::Bullet),
            114..=121 | 154 | 167 | 170..=172 => LaserOp::decode(op, &r).map(Op::Laser),
            95 | 122 | 123 | 127 | 129..=134 | 153 | 155 | 158 | 164 | 176 | 184 => {
                BossOp::decode(op, &r).map(Op::Boss)
            }
            _ => EnemyOp::decode(op, &r).map(Op::Enemy),
        }
        .unwrap_or(Op::Unknown(op))
    }
}

impl FlowOp {
    fn decode(op: i16, r: &Rd) -> Option<Self> {
        Some(match op {
            0 | 3 | 84 | 85 => Self::Nop,
            1 => Self::Delete,
            2 => Self::Wait(r.a(0)),
            4 => Self::Jump {
                time: r.a(0),
                to: r.a(1),
            },
            5 => Self::Loop {
                time: r.a(0),
                to: r.a(1),
                counter: r.dst(2),
            },
            40..=51 => Self::JumpIf {
                cmp: match (op - 40) / 2 {
                    0 => Cmp::Eq,
                    1 => Cmp::Ne,
                    2 => Cmp::Lt,
                    3 => Cmp::Le,
                    4 => Cmp::Gt,
                    _ => Cmp::Ge,
                },
                float: op % 2 != 0,
                a: r.a(0),
                b: r.a(1),
                time: r.a(2),
                to: r.a(3),
            },
            52 => Self::Call { sub: r.a(0) },
            53 => Self::Return,
            88 => Self::CallBoss {
                slot: r.a(0),
                sub: r.a(1),
            },
            89 => Self::InterruptBoss {
                slot: r.a(0),
                k: r.a(1),
            },
            125 => Self::InterruptNow(r.a(0)),
            126 => Self::SetInterrupt {
                sub: r.a(0),
                k: r.a(1),
            },
            135 => Self::Thread {
                k: r.a(0),
                sub: r.a(1),
            },
            136 => Self::Native {
                n: r.a(0),
                arg: r.a(1),
            },
            137 => Self::NativeCb {
                n: r.a(0),
                arg: r.a(1),
            },
            146 => Self::TimeAdd(r.a(0)),
            _ => return None,
        })
    }
}

impl VarOp {
    fn decode(op: i16, r: &Rd) -> Option<Self> {
        let d0 = r.dst(0);
        Some(match op {
            6 => Self::ISet { dst: d0, v: r.a(1) },
            7 => Self::FSet { dst: d0, v: r.a(1) },
            8 => Self::IRandSign { dst: d0, v: r.a(1) },
            9 => Self::FRandSign { dst: d0, v: r.a(1) },
            10..=14 => Self::IArith {
                kind: Arith::from_digit(op % 10),
                dst: d0,
                rhs: r.a(1),
            },
            15..=19 => Self::FArith {
                kind: Arith::from_digit((op - 15) % 10),
                dst: d0,
                rhs: r.a(1),
            },
            20..=24 => Self::IArith3 {
                kind: Arith::from_digit(op % 10),
                dst: d0,
                a: r.a(1),
                b: r.a(2),
            },
            25..=29 => Self::FArith3 {
                kind: Arith::from_digit((op - 15) % 10),
                dst: d0,
                a: r.a(1),
                b: r.a(2),
            },
            30 => Self::Inc(d0),
            31 => Self::Dec(d0),
            32 => Self::FSin { dst: d0, a: r.a(1) },
            33 => Self::FCos { dst: d0, a: r.a(1) },
            34 | 39 => {
                let [x1, y1, x2, y2] = r.arr(1);
                if op == 34 {
                    Self::FAtan2 {
                        dst: d0,
                        x1,
                        y1,
                        x2,
                        y2,
                    }
                } else {
                    Self::FDist {
                        dst: d0,
                        x1,
                        y1,
                        x2,
                        y2,
                    }
                }
            }
            35 => Self::FLerp {
                dst: d0,
                b: r.a(1),
                c: r.a(2),
                d: r.a(3),
            },
            36 => Self::Interp {
                target_bits: r.raw(0),
                dur: r.a(1),
                func: r.a(2),
                mode: r.a(3),
                abcd: r.arr(4),
            },
            37 => Self::NormAngle(d0),
            38 => Self::Polar {
                x: d0,
                y: r.dst(1),
                ang: r.a(2),
                r: r.a(3),
            },
            166 => Self::PolarYs {
                x: d0,
                y: r.dst(1),
                ang: r.a(2),
                r: r.a(3),
            },
            86 => Self::IGetBoss {
                dst: d0,
                var: r.a(1),
                slot: r.a(2),
            },
            87 => Self::FGetBoss {
                dst: d0,
                var: r.a(1),
                slot: r.a(2),
            },
            169 => Self::RandAwayAngle(d0),
            _ => return None,
        })
    }
}

impl MoveOp {
    fn decode(op: i16, r: &Rd) -> Option<Self> {
        Some(match op {
            63 => Self::SetPos {
                x: r.a(0),
                y: r.a(1),
            },
            64 => Self::MoveTo {
                dur: r.a(0),
                ease: r.a(1),
                x: r.a(2),
                y: r.a(3),
            },
            65 => Self::SetDir {
                ang: r.a(0),
                spd: r.a(1),
            },
            66 => Self::MoveDir {
                dur: r.a(0),
                ease: r.a(1),
                ang: r.a(2),
                spd: r.a(3),
            },
            67 => Self::MoveRand {
                dur: r.a(0),
                ease: r.a(1),
                spd: r.a(2),
            },
            178 => Self::MoveRand2 {
                dur: r.a(0),
                ease: r.a(1),
                spd: r.a(2),
            },
            68 => Self::SetDirAim {
                off: r.a(0),
                spd: r.a(1),
            },
            69 => Self::MoveDirAim {
                dur: r.a(0),
                ease: r.a(1),
                off: r.a(2),
                spd: r.a(3),
            },
            70 => Self::SetAngVel(r.a(0)),
            71 => Self::SetAccel(r.a(0)),
            72 => Self::Orbit {
                dur: r.a(0),
                cx: r.a(1),
                cy: r.a(2),
                a: r.a(3),
                av: r.a(4),
                r: r.a(5),
                rv: r.a(6),
            },
            73 => Self::OrbitHere {
                dur: r.a(0),
                a: r.a(1),
                av: r.a(2),
                rv: r.a(3),
            },
            74 => Self::OrbitVel {
                dur: r.a(0),
                av: r.a(1),
                rv: r.a(2),
            },
            75 => Self::ClampRect(r.arr(0)),
            76 => Self::ClampOff,
            77 => Self::Hitbox {
                w: r.a(0),
                h: r.a(1),
            },
            78 => Self::Shotbox {
                w: r.a(0),
                h: r.a(1),
            },
            82 => Self::NofireRadius(r.a(0)),
            _ => return None,
        })
    }
}

impl EnemyOp {
    fn decode(op: i16, r: &Rd) -> Option<Self> {
        let a0 = r.a(0);
        Some(match op {
            54 | 58 => Self::Anm {
                script: a0,
                stg: op == 58,
            },
            55 | 59 => Self::AnmDir {
                base: a0,
                stg: op == 59,
            },
            56 | 60 => Self::AnmDir6 {
                scripts: r.arr(0),
                stg: op == 60,
            },
            57 | 61 => Self::SubAnm {
                vm: a0,
                script: r.a(1),
                stg: op == 61,
            },
            62 => Self::AnmReplay,
            79 => Self::FlagsSet(a0),
            80 => Self::FlagsOff(a0),
            81 => Self::FlagsOn(a0),
            83 => Self::FlagB1(a0),
            90..=94 => {
                let (kind, z) = match op {
                    90 => (SpawnKind::Child, None),
                    91 => (SpawnKind::ChildRel, None),
                    92 => (SpawnKind::ChildFollow, None),
                    93 => (SpawnKind::Spawn, Some(r.a(3))),
                    _ => (SpawnKind::SpawnRel, Some(r.a(3))),
                };
                let base = if z.is_some() { 4 } else { 3 };
                Self::Spawn {
                    kind,
                    sub: a0,
                    x: r.a(1),
                    y: r.a(2),
                    z,
                    life: r.a(base),
                    item: r.a(base + 1),
                    score: r.a(base + 2),
                }
            }
            124 => Self::Sfx(a0),
            128 => Self::Effect13 {
                axis: [1, 2, 3].map(|k| f32::from_bits(r.raw(k))),
                radius: f32::from_bits(r.raw(4)),
            },
            138 => Self::SetBytes(a0.raw),
            139 => Self::Effect {
                id: a0,
                n: r.a(1),
                colour: r.a(2),
            },
            140 => Self::EffectDir {
                id: a0,
                n: r.a(1),
                colour: r.a(2),
                param: r.arr(3),
            },
            141 => Self::Item(a0),
            142 => Self::ItemBurst(a0),
            168 => Self::ItemBurst1(a0),
            143 => Self::SetItem(a0),
            144 => Self::ExtraItems {
                point: a0,
                power: r.a(1),
            },
            145 => Self::FlagA25(a0.raw),
            151 => Self::NoStack(a0.raw),
            156 => Self::FlagA7(a0.raw),
            147 => Self::StdSection(a0),
            148 => Self::Hud148(a0),
            149 => Self::AnmInterrupt(a0),
            150 => Self::SubAnmInterrupt {
                vm: a0.raw,
                n: r.raw(1),
            },
            157 => Self::Trail {
                flags: a0.raw,
                n: r.a(1),
                a: r.a(2),
                b: r.a(3),
            },
            159 => Self::DrawLayer(a0),
            160 => Self::DmgTimer(a0),
            163 => Self::GF54cec(a0),
            165 => Self::AnmRotZ(a0),
            173 => Self::FlagA30(a0),
            182 => Self::FlagB8(a0),
            183 => Self::FlagA31(a0),
            174 => Self::Aura(a0),
            175 => Self::SpawnDisable(a0),
            177 => Self::SetHudLife(a0),
            179 => Self::NightClockShow,
            180 => Self::NightClockHide,
            181 => Self::Extend181,
            _ => return None,
        })
    }
}

impl BulletOp {
    fn decode(op: i16, r: &Rd) -> Option<Self> {
        Some(match op {
            96..=104 => Self::Bullets,
            105 | 106 => Self::Autofire {
                n: r.a(0),
                rand: op == 106,
            },
            107 => Self::AutofireStore(true),
            108 => Self::AutofireStore(false),
            109 => Self::FireStored,
            110 => Self::ShotOffset {
                x: r.a(0),
                y: r.a(1),
            },
            111 => {
                let [k, a, b, c, d, e, f] = r.arr(0);
                Self::BulletEx {
                    k,
                    a,
                    b,
                    c,
                    d,
                    e,
                    f,
                }
            }
            112 => Self::CancelBullets,
            162 => Self::CancelAll,
            113 => Self::BulletSfx {
                id: r.a(0),
                x: r.a(1),
            },
            152 => Self::RankParams {
                speed: r.arr(0),
                counts: r.arr(2),
            },
            161 => Self::CancelRadius(r.a(0)),
            _ => return None,
        })
    }
}

impl LaserOp {
    fn decode(op: i16, r: &Rd) -> Option<Self> {
        let (k, a) = (r.a(0), r.a(1));
        Some(match op {
            114 | 115 => Self::Laser { fixed: op == 114 },
            116 => Self::LaserSlot(k),
            117 => Self::LaserRotate { k, a },
            118 => Self::LaserAim { k, a },
            167 => Self::LaserSetAngle { k, a },
            119 => Self::LaserOffset { k, off: r.arr(1) },
            120 => Self::LaserAlive(k),
            121 => Self::LaserCancel(k),
            154 => Self::LaserClear,
            170 => Self::LaserHideEmitter { k, a },
            171 => Self::LaserMaxLen { k, a },
            172 => Self::LaserLen {
                k,
                tail: a,
                head: r.a(2),
            },
            _ => return None,
        })
    }
}

impl BossOp {
    fn decode(op: i16, r: &Rd) -> Option<Self> {
        let a0 = r.a(0);
        Some(match op {
            95 => Self::KillAll,
            122 => Self::Spellcard,
            123 => Self::SpellcardEnd,
            127 => Self::Boss(a0),
            129 => Self::DeathMode(a0.raw),
            130 => Self::DeathSub(a0.raw),
            131 => Self::SetLife(a0),
            132 => Self::SetTimer(a0),
            133 => Self::LifeThreshold {
                k: a0,
                life: r.a(1),
                sub: r.a(2),
            },
            134 => Self::Timeout {
                frames: a0,
                sub: r.a(1),
            },
            153 => Self::TimeoutSubFromDeath,
            155 => Self::FixedBonus(a0.raw),
            158 => Self::HpMarker {
                k: a0,
                a: r.a(1),
                b: r.a(2),
                c: r.a(3),
            },
            164 => Self::SpellMode(a0),
            176 => Self::LastSpell,
            184 => Self::SpellEx(a0),
            _ => return None,
        })
    }
}

// --- instruction blocks (bounded by the instruction's own bytes, `Ecl::bytes`) ---

fn blk_u32(b: &[u8], o: usize) -> u32 {
    u32_at(b, o).unwrap_or(0)
}

fn blk_i16(b: &[u8], o: usize) -> i16 {
    i16_at(b, o).unwrap_or(0)
}

fn blk_f32(b: &[u8], o: usize) -> f32 {
    f32::from_bits(blk_u32(b, o))
}

fn blk_mask(b: &[u8]) -> u16 {
    i16_at(b, 10).unwrap_or(0).cast_unsigned()
}

// Bullet block, ops 96–104 (07 §9.1, 44 bytes): raw fields; mask bit k = field k is a variable id.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BulletBlock {
    // +0x04 opcode (aim mode = op − 96).
    pub op: i16,
    pub mask: u16,
    // +0x0C / +0x0E (mask bits 0 / 1).
    pub kind: i16,
    pub colour: i16,
    // +0x10 / +0x14 (bits 2 / 3).
    pub count1: i32,
    pub count2: i32,
    // +0x18 / +0x1C (bits 4 / 5).
    pub speed1: f32,
    pub speed2: f32,
    // +0x20 / +0x24 (bits 6 / 7).
    pub angle1: f32,
    pub angle2: f32,
    // +0x28, never a variable.
    pub flags: u32,
}

impl BulletBlock {
    pub fn parse(b: &[u8]) -> Self {
        Self {
            op: blk_i16(b, 4),
            mask: blk_mask(b),
            kind: blk_i16(b, 0x0C),
            colour: blk_i16(b, 0x0E),
            count1: blk_u32(b, 0x10).cast_signed(),
            count2: blk_u32(b, 0x14).cast_signed(),
            speed1: blk_f32(b, 0x18),
            speed2: blk_f32(b, 0x1C),
            angle1: blk_f32(b, 0x20),
            angle2: blk_f32(b, 0x24),
            flags: blk_u32(b, 0x28),
        }
    }

    pub fn var(&self, k: u32) -> bool {
        self.mask & (1 << k) != 0
    }
}

// Laser block, ops 114/115 (07 §9.4, 64 bytes); mask bits 0–10 cover +0x0C…+0x30, +0x34…+0x3C are raw.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LaserBlock {
    pub mask: u16,
    pub kind: i16,
    pub colour: i16,
    pub angle: f32,
    pub speed: f32,
    pub tail: f32,
    pub head: f32,
    pub max_len: f32,
    pub width: f32,
    pub startup: i32,
    pub duration: i32,
    pub fade: i32,
    pub hit_start: i32,
    pub fade_hit: i32,
    pub flags: u32,
}

impl LaserBlock {
    pub fn parse(b: &[u8]) -> Self {
        Self {
            mask: blk_mask(b),
            kind: blk_i16(b, 0x0C),
            colour: blk_i16(b, 0x0E),
            angle: blk_f32(b, 0x10),
            speed: blk_f32(b, 0x14),
            tail: blk_f32(b, 0x18),
            head: blk_f32(b, 0x1C),
            max_len: blk_f32(b, 0x20),
            width: blk_f32(b, 0x24),
            startup: blk_u32(b, 0x28).cast_signed(),
            duration: blk_u32(b, 0x2C).cast_signed(),
            fade: blk_u32(b, 0x30).cast_signed(),
            hit_start: blk_u32(b, 0x34).cast_signed(),
            fade_hit: blk_u32(b, 0x38).cast_signed(),
            flags: blk_u32(b, 0x3C),
        }
    }

    pub fn var(&self, k: u32) -> bool {
        self.mask & (1 << k) != 0
    }
}

// Spell block, op 122 (07 §11, 244 bytes): strings XOR-decoded and cut at the first NUL.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpellBlock {
    pub face: i16,
    pub id: i16,
    pub bonus: u32,
    pub name: Vec<u8>,
    pub owner: Vec<u8>,
    pub comment: [Vec<u8>; 2],
}

impl SpellBlock {
    pub fn parse(b: &[u8]) -> Self {
        let dec = |o: usize, n: usize, key: u8| -> Vec<u8> {
            let s: Vec<u8> = b
                .get(o..o + n)
                .unwrap_or(&[])
                .iter()
                .map(|c| c ^ key)
                .collect();
            let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
            s[..end].to_vec()
        };
        Self {
            face: blk_i16(b, 0x0C),
            id: blk_i16(b, 0x0E),
            bonus: blk_u32(b, 0x10),
            name: dec(0x14, 48, 0xAA),
            owner: dec(0x44, 48, 0xBB),
            comment: [dec(0x74, 64, 0xDD), dec(0xB4, 64, 0xEE)],
        }
    }
}

// Argument letters: i int, f float, V/W int/float destination, T time, O offset, S sub, r raw;
// B bullet block (8 dwords, 07 §9.1), L laser block (13, §9.4), C spell block (58, §11).
pub fn signature(op: i16) -> Option<&'static str> {
    Some(match op {
        0 | 1 | 53 | 62 | 76 | 84 | 85 | 95 | 107 | 108 | 109 | 112 | 123 | 153 | 154 | 162
        | 179 | 180 | 181 => "",
        2
        | 3
        | 54
        | 55
        | 58
        | 59
        | 79..=81
        | 83
        | 105
        | 106
        | 116
        | 120
        | 121
        | 124
        | 125
        | 127
        | 131
        | 132
        | 141..=143
        | 146..=149
        | 159
        | 160
        | 163
        | 168
        | 173..=177
        | 182..=184 => "i",
        4 => "TO",
        5 => "TOV",
        6 | 8 | 10..=14 => "Vi",
        7 | 9 | 15..=19 | 32 | 33 => "Wf",
        20..=24 | 86 => "Vii",
        25..=29 => "Wff",
        30 | 31 => "V",
        34 | 39 => "Wffff",
        35 => "Wfff",
        36 => "riiiffff",
        37 | 169 => "W",
        38 | 166 => "WWff",
        40..=51 if op % 2 == 0 => "iiTO",
        40..=51 => "ffTO",
        52 => "S",
        56 | 60 => "iiiiii",
        57 | 61 | 89 | 113 | 136 | 137 | 144 | 170 => "ii",
        63 | 65 | 68 | 77 | 78 | 110 => "ff",
        64 | 66 | 69 => "iiff",
        67 | 178 => "iif",
        70 | 71 | 82 | 161 | 165 => "f",
        72 => "iffffff",
        73 | 119 | 164 => "ifff",
        74 | 172 => "iff",
        75 => "ffff",
        87 => "Wii",
        90..=92 => "Sffiii",
        93 | 94 => "Sfffiii",
        96..=104 => "B",
        111 => "iiiiiff",
        114 | 115 => "L",
        117 | 118 | 167 | 171 => "if",
        122 => "C",
        126 => "Si",
        128 => "rfffi",
        129 | 130 | 138 | 145 | 151 | 155 | 156 => "r",
        133 => "iiS",
        88 | 134 | 135 => "iS",
        139 => "iii",
        140 => "iiifff",
        150 => "rr",
        152 => "ffiiii",
        157 => "riii",
        158 => "iiii",
        _ => return None,
    })
}

pub fn ins_size(op: i16) -> Option<usize> {
    let n = match signature(op)? {
        "B" => 8,
        "L" => 13,
        "C" => 58,
        s => s.len(),
    };
    Some(SUB_HEADER + 4 * n)
}

// Timeline instruction sizes by op (07 §3).
pub fn tl_size(op: i16) -> Option<usize> {
    Some(
        8 + 4 * match op {
            0 | 1 | 15 => 6,
            2 | 4 | 11 | 12 => 7,
            3 | 5 => 5,
            6 | 10 | 13 | 14 | 9 => 1,
            8 => 2,
            7 | 16 => 0,
            _ => return None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const FILES: [&str; 24] = [
        "ecldata1",
        "ecldata2",
        "ecldata3",
        "ecldata4a",
        "ecldata4b",
        "ecldata5",
        "ecldata6",
        "ecldata7",
        "ecldata8",
        "ecldata1sp",
        "ecldata2sp",
        "ecldata3sp",
        "ecldata4asp",
        "ecldata4bsp",
        "ecldata5sp",
        "ecldata6sp",
        "ecldata7sp",
        "ecldata8sp",
        "ecldata_al",
        "ecldata_rm",
        "ecldata_sk",
        "ecldata_yk",
        "ecldata_ym",
        "ecldata_yy",
    ];

    #[test]
    fn all_ecl_parse_and_ops_known() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let mut used = std::collections::BTreeSet::new();
        for f in FILES {
            let ecl = parse(ar.read(&format!("{f}.ecl")).unwrap()).unwrap();
            for (s, &start) in ecl.subs.iter().enumerate() {
                let mut off = start;
                loop {
                    let ins = ecl
                        .ins(off)
                        .unwrap_or_else(|| panic!("{f} sub {s}: truncated at {off:#x}"));
                    if ins.op == -1 {
                        break;
                    }
                    assert_eq!(
                        ins_size(ins.op),
                        Some(ins.size),
                        "{f} sub {s} @{off:#x}: op {} size {}",
                        ins.op,
                        ins.size
                    );
                    used.insert(ins.op);
                    let op = ecl.op(&ins);
                    assert!(
                        !matches!(op, Op::Unknown(_)),
                        "{f} sub {s} @{off:#x}: op {} not decoded",
                        ins.op
                    );
                    if let Op::Flow(
                        FlowOp::Jump { to, .. }
                        | FlowOp::Loop { to, .. }
                        | FlowOp::JumpIf { to, .. },
                    ) = op
                    {
                        let target = ins.off.rel(to.raw.cast_signed());
                        assert!(ecl.ins(target).is_some(), "{f} sub {s}: bad jump");
                    }
                    if matches!(ins.op, 52 | 88 | 135) {
                        let k = usize::from(ins.op != 52);
                        let sub = ecl.arg(&ins, k).cast_signed();
                        assert!(
                            ins.mask != 0 || sub < 0 || ecl.sub_start(sub).is_some(),
                            "{f} sub {s}: bad sub id {sub}"
                        );
                    }
                    off += ins.size;
                }
            }
            for (t, &start) in ecl.timelines.iter().enumerate() {
                let mut off = start;
                while let Some(ins) = ecl.tl_ins(off) {
                    if ins.time < 0 {
                        break;
                    }
                    assert_eq!(
                        tl_size(ins.op),
                        Some(ins.size),
                        "{f} timeline {t}: op {} size {}",
                        ins.op,
                        ins.size
                    );
                    off += ins.size;
                }
            }
        }
        assert!(used.len() > 140, "only {} distinct ops", used.len());
    }

    // Every op of the table decodes to a known `Op`, each argument from its own dword and mask bit.
    #[test]
    fn decode_every_table_op() {
        for op in 0..=184i16 {
            let Some(size) = ins_size(op) else {
                continue;
            };
            let mut data = vec![0u8; size];
            data[4..6].copy_from_slice(&op.to_le_bytes());
            data[6..8].copy_from_slice(&i16::try_from(size).unwrap().to_le_bytes());
            data[10..12].copy_from_slice(&0b10u16.to_le_bytes());
            for k in 0..(size - SUB_HEADER) / 4 {
                let v = 100 + u32::try_from(k).unwrap();
                data[SUB_HEADER + 4 * k..SUB_HEADER + 4 * k + 4].copy_from_slice(&v.to_le_bytes());
            }
            let ecl = Ecl {
                data,
                subs: vec![0],
                timelines: Vec::new(),
            };
            let ins = ecl.ins(0).unwrap();
            assert!(!matches!(ecl.op(&ins), Op::Unknown(_)), "op {op}");
        }
        let ins = Ins {
            off: EclOff(0),
            time: 0,
            op: 5,
            size: 24,
            rank: 0xFF,
            mask: 0b110,
        };
        let mut d = vec![0u8; 24];
        for (k, v) in [7u32, 0xFFFF_FFF4, 10_000].into_iter().enumerate() {
            d[12 + 4 * k..16 + 4 * k].copy_from_slice(&v.to_le_bytes());
        }
        let Op::Flow(FlowOp::Loop { time, to, counter }) = Op::decode(&ins, &d) else {
            panic!("op 5");
        };
        assert_eq!((time.raw, time.var), (7, false));
        assert_eq!((to.raw, to.var), (0xFFFF_FFF4, true));
        assert_eq!(
            (counter.k(), counter.arg.raw, counter.arg.var),
            (2, 10_000, true)
        );
        assert_eq!(EclOff(0x20).rel(-12), 0x14);
    }
}
