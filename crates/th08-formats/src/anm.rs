//! ANM container (03 §2–§4) and script instruction decoder (05 §1, §5).

#[derive(Debug, thiserror::Error)]
pub enum AnmError {
    #[error("bad THTX magic")]
    BadThtxMagic,
    #[error("unknown THTX format {0}")]
    UnknownThtxFormat(u16),
    #[error("anm version mismatch")]
    VersionMismatch,
}

pub type Result<T> = std::result::Result<T, AnmError>;

// Sprite record, 20 bytes at entry_start + sprite_offset; x, y, w, h in logical texture pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprite {
    // Always the file-wide cumulative sprite index; the loader ignores it.
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

// One script of the entry's script table; scripts are addressed by cumulative index.
#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    // Ignored by the original loader.
    pub id: i32,
    // Instruction bytes up to the next script (scripts are contiguous).
    pub data: Vec<u8>,
}

// Embedded THTX texture block, decoded to RGBA8.
pub struct Thtx {
    pub format: u16,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

// One texture entry of the chain, logic data only; its pixels are `textures(..)[entry]`.
pub struct Chunk {
    // Image name at +0x1C; a leading '@' means a blank render-target texture.
    pub name: String,
    // Logical size at +0x0C/+0x10: sprite UVs are normalised by it, not by the THTX size.
    pub width: u32,
    pub height: u32,
    pub sprites: Vec<Sprite>,
    pub scripts: Vec<Script>,
    // THTX offset +0x30 is non-zero: the pixels come from `textures`, not from this logic record.
    pub has_texture: bool,
}

fn u32at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn f32at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32at(b, o))
}

fn u16at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

// THTX header: magic, u16 0, u16 format, u16 width, u16 height, u32 size, then packed rows at +0x10.
pub fn decode_thtx(b: &[u8]) -> Result<Thtx> {
    if b.len() < 16 || &b[0..4] != b"THTX" {
        return Err(AnmError::BadThtxMagic);
    }
    let format = u16at(b, 6);
    let (width, height) = (u32::from(u16at(b, 8)), u32::from(u16at(b, 10)));
    let px = &b[16..];
    let n = (width * height) as usize;
    let mut rgba = Vec::with_capacity(n * 4);
    // 5/6-bit channels widened to 8 bits by bit replication for the RGBA8 upload (03 §3 allows full precision).
    let x5 = |v: u16| ((v << 3) | (v >> 2)) as u8;
    let x6 = |v: u16| ((v << 2) | (v >> 4)) as u8;
    match format {
        // Formats: 1 A8R8G8B8, 2 A1R5G5B5, 3 R5G6B5, 4 R8G8B8, 5 A4R4G4B4 (little-endian words).
        1 => {
            for p in px[..n * 4].as_chunks::<4>().0 {
                rgba.extend([p[2], p[1], p[0], p[3]]);
            }
        }
        2 => {
            for &p in px[..n * 2].as_chunks::<2>().0 {
                let v = u16::from_le_bytes(p);
                rgba.extend([
                    x5((v >> 10) & 31),
                    x5((v >> 5) & 31),
                    x5(v & 31),
                    if v & 0x8000 != 0 { 255 } else { 0 },
                ]);
            }
        }
        3 => {
            for &p in px[..n * 2].as_chunks::<2>().0 {
                let v = u16::from_le_bytes(p);
                rgba.extend([x5(v >> 11), x6((v >> 5) & 63), x5(v & 31), 255]);
            }
        }
        4 => {
            for p in px[..n * 3].as_chunks::<3>().0 {
                rgba.extend([p[2], p[1], p[0], 255]);
            }
        }
        5 => {
            for &p in px[..n * 2].as_chunks::<2>().0 {
                let v = u16::from_le_bytes(p);
                let n4 = |s: u16| (((v >> s) & 15) * 17) as u8;
                rgba.extend([n4(8), n4(4), n4(0), n4(12)]);
            }
        }
        f => return Err(AnmError::UnknownThtxFormat(f)),
    }
    Ok(Thtx {
        format,
        width,
        height,
        rgba,
    })
}

// Anm_Preload 0x465860: entry k+1 at entry_k + next_offset (+0x38), 0 ends the chain; yields each entry's
// file offset. Version must be 3, otherwise the load fails.
fn entries(b: &[u8]) -> Result<Vec<usize>> {
    let mut v = Vec::new();
    let mut base = 0;
    loop {
        let c = &b[base..];
        if u32at(c, 0x28) != 3 {
            return Err(AnmError::VersionMismatch);
        }
        v.push(base);
        match u32at(c, 0x38) as usize {
            0 => return Ok(v),
            next => base += next,
        }
    }
}

// Logic half of Anm_Preload 0x465860: entries, sprites, scripts; no pixels are touched.
pub fn parse(b: &[u8]) -> Result<Vec<Chunk>> {
    entries(b)?.into_iter().map(|base| entry(b, base)).collect()
}

fn entry(b: &[u8], base: usize) -> Result<Chunk> {
    let c = &b[base..];
    let (n_spr, n_scr) = (u32at(c, 0) as usize, u32at(c, 4) as usize);
    let (width, height) = (u32at(c, 0x0c), u32at(c, 0x10));
    let name_off = u32at(c, 0x1c) as usize;
    let thtx_off = u32at(c, 0x30) as usize;
    let next = u32at(c, 0x38) as usize;
    let name_end = c[name_off..].iter().position(|&x| x == 0).unwrap_or(0);
    let name = String::from_utf8_lossy(&c[name_off..name_off + name_end]).into_owned();
    let sprites = (0..n_spr)
        .map(|i| {
            let o = u32at(c, 0x40 + 4 * i) as usize;
            Sprite {
                id: u32at(c, o),
                x: f32at(c, o + 4),
                y: f32at(c, o + 8),
                w: f32at(c, o + 12),
                h: f32at(c, o + 16),
            }
        })
        .collect();
    // Script table {i32 id, u32 offset}[script_count] follows the sprite offsets.
    let tbl = 0x40 + 4 * n_spr;
    let offs: Vec<(i32, usize)> = (0..n_scr)
        .map(|i| {
            (
                u32at(c, tbl + 8 * i).cast_signed(),
                u32at(c, tbl + 8 * i + 4) as usize,
            )
        })
        .collect();
    // Script length is not stored: each script ends at the next script start, the THTX block or the next entry.
    let limit = if thtx_off != 0 {
        thtx_off
    } else if next != 0 {
        next
    } else {
        c.len()
    };
    let mut sorted: Vec<usize> = offs.iter().map(|o| o.1).collect();
    sorted.sort();
    let scripts = offs
        .into_iter()
        .map(|(id, o)| {
            let end = sorted
                .iter()
                .copied()
                .find(|&e| e > o)
                .unwrap_or(limit)
                .min(limit);
            Script {
                id,
                data: c[o..end].to_vec(),
            }
        })
        .collect();
    Ok(Chunk {
        name,
        width,
        height,
        sprites,
        scripts,
        has_texture: thtx_off != 0,
    })
}

// Pixel half of Anm_Preload / Anm_LoadEntryTexture 0x465CC0: one decoded THTX per entry, `None` for
// entries without a THTX block ('@' render targets). Same entry order as `parse`.
pub fn textures(b: &[u8]) -> Result<Vec<Option<Thtx>>> {
    entries(b)?
        .into_iter()
        .map(|base| match u32at(b, base + 0x30) as usize {
            0 => Ok(None),
            off => decode_thtx(&b[base + off..]).map(Some),
        })
        .collect()
}

// Byte offset from the script start (jump / branch targets and the VM ip are script-relative).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnmOff(pub u32);

impl AnmOff {
    pub const fn at(self) -> usize {
        self.0 as usize
    }

    // `self + n` bytes; script offsets stay far below 2^32 (scripts are a few KiB).
    #[must_use]
    pub const fn add(self, n: usize) -> Self {
        Self(self.0.wrapping_add(n as u32))
    }
}

// Instruction header at `off`: i16 op, u16 size (header + args), i16 time, u16 var mask; args at off + 8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ins {
    pub off: AnmOff,
    pub op: i16,
    pub size: u16,
    pub time: i16,
    pub mask: u16,
}

// One 4-byte argument (05 §3): the raw bits and its var-mask bit (set = the bits are a variable id).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arg {
    pub raw: u32,
    pub var: bool,
}

impl Ins {
    // Header at `off`; `None` when the header or one of the (at most 8) args its size covers lies past the end.
    pub fn read(d: &[u8], off: AnmOff) -> Option<Ins> {
        let ip = off.at();
        let h = d.get(ip..ip.checked_add(8)?)?;
        let ins = Ins {
            off,
            op: i16::from_le_bytes([h[0], h[1]]),
            size: u16::from_le_bytes([h[2], h[3]]),
            time: i16::from_le_bytes([h[4], h[5]]),
            mask: u16::from_le_bytes([h[6], h[7]]),
        };
        d.get(ip + 8..ip + 8 + 4 * ins.nargs())?;
        Some(ins)
    }

    // Args the size covers, capped at 8 (the VM's argument buffer).
    pub fn nargs(&self) -> usize {
        (usize::from(self.size).saturating_sub(8) / 4).min(8)
    }

    // Raw bits of arg k; 0 for an arg the size does not cover.
    pub fn raw(&self, d: &[u8], k: usize) -> u32 {
        if k >= self.nargs() {
            return 0;
        }
        let o = self.off.at() + 8 + 4 * k;
        u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
    }

    pub fn arg(&self, d: &[u8], k: usize) -> Arg {
        Arg {
            raw: self.raw(d, k),
            var: self.mask & (1 << k) != 0,
        }
    }
}

// Arg letters of the 05 §5 opcode table: i int, f float, V int var dest, W float var dest, O offset, T time.
pub fn signature(op: i16) -> Option<&'static str> {
    Some(match op {
        -1 | 0 | 1 | 2 | 10 | 11 | 20 | 22 | 23 | 89 => "",
        3 | 8 | 16 | 21 | 24 | 25 | 28 | 30 | 31 | 79 | 82 | 83 | 85 | 88 => "i",
        4 => "OT",
        5 => "VOT",
        6 | 12 | 13 => "fff",
        7 | 14 => "ff",
        9 | 34 | 84 | 87 => "iii",
        15 => "ii",
        17..=19 => "fffi",
        26 | 27 | 80 | 81 => "f",
        29 => "ffi",
        32 | 35 => "iifff",
        33 | 86 => "iiiii",
        36 => "iiff",
        // Ops 37–58: odd int, even float; 49–58 take two sources.
        37..=58 => match (op % 2 == 1, op >= 49) {
            (true, false) => "Vi",
            (false, false) => "Wf",
            (true, true) => "Vii",
            (false, true) => "Wff",
        },
        59 => "Vi",
        60..=65 => "Wf",
        66 => "W",
        67..=78 => {
            if op % 2 == 1 {
                "iiOT"
            } else {
                "ffOT"
            }
        }
        _ => return None,
    })
}

// Control flow, waits and interrupts (05 §4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlowOp {
    // Op −1 end, op 1 hide_stop: visible = 0, script finished.
    End,
    // Op 2 stop: finished, visible kept.
    Stop,
    // Op 0 nop; op 21 interrupt label (a no-op when executed).
    Nop,
    Jump {
        to: Arg,
        time: Arg,
    },
    LoopDec {
        var: Arg,
        to: Arg,
        time: Arg,
    },
    // Ops 20 / 23 (`hide` = 23).
    WaitInterrupt {
        hide: bool,
    },
    // Ops 67–78: odd int, even float compare.
    Branch {
        cmp: Cmp,
        float: bool,
        a: Arg,
        b: Arg,
        to: Arg,
        time: Arg,
    },
    Wait(Arg),
    ReturnInterrupt,
}

// Compare of ops 67–78 in table order (op − 67) / 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

// Direct state writes: sprite, transform, colour, flag bits, UV, blend, draw mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpriteOp {
    Sprite(Arg),
    Pos([Arg; 3]),
    Scale([Arg; 2]),
    // Op 8 alpha / op 85 alpha2 (`second`).
    Alpha { second: bool, a: Arg },
    // Op 9 colour / op 84 colour2.
    Color { second: bool, rgb: [Arg; 3] },
    FlipX,
    FlipY,
    Rotation([Arg; 3]),
    AngularVel([Arg; 3]),
    ScaleGrowth([Arg; 2]),
    BlendAdditive(Arg),
    AnchorTopLeft,
    PosTargetOffset(Arg),
    SetTag(Arg),
    // Ops 26 / 27: axis 0 u, 1 v.
    UvAdd { axis: usize, d: Arg },
    Visible(Arg),
    ZwriteOff(Arg),
    StageCamera(Arg),
    // Ops 80 / 81.
    UvScroll { axis: usize, s: Arg },
    BlendMode(Arg),
    // Op 83 reads its arg raw, no variable support.
    DrawMode(u32),
    UseColor2(Arg),
}

// Interpolation slot starts (ops 15, 17–19, 29, 32–36, 86, 87).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InterpOp {
    AlphaFade {
        a: Arg,
        dur: Arg,
    },
    // Ops 17 / 18 / 19 = easing modes 0 / 4 / 6.
    PosFixed {
        mode: i32,
        end: [Arg; 3],
        dur: Arg,
    },
    ScaleLinear {
        end: [Arg; 2],
        dur: Arg,
    },
    Pos {
        dur: Arg,
        mode: Arg,
        end: [Arg; 3],
    },
    // Op 33 colour / op 86 colour2.
    Color {
        second: bool,
        dur: Arg,
        mode: Arg,
        rgb: [Arg; 3],
    },
    // Op 34 alpha / op 87 alpha2.
    Alpha {
        second: bool,
        dur: Arg,
        mode: Arg,
        a: Arg,
    },
    Rotation {
        dur: Arg,
        mode: Arg,
        end: [Arg; 3],
    },
    Scale {
        dur: Arg,
        mode: Arg,
        end: [Arg; 2],
    },
}

// Variable arithmetic of ops 37–58 in table order (op − 37) / 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arith {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Add3,
    Sub3,
    Mul3,
    Div3,
    Rem3,
}

// Ops 61–65.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathFn {
    Sin,
    Cos,
    Tan,
    Acos,
    Atan,
}

// Variable ops 37–66 (05 §3); `dst` is always arg 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VarOp {
    // Odd ops 37–57 int (`float` false), even 38–58 float.
    Arith {
        f: Arith,
        float: bool,
        dst: Arg,
        b: Arg,
        c: Arg,
    },
    IRand {
        dst: Arg,
        n: Arg,
    },
    FRand {
        dst: Arg,
        x: Arg,
    },
    Math {
        f: MathFn,
        dst: Arg,
        x: Arg,
    },
    NormAngle {
        dst: Arg,
    },
}

// One decoded instruction of the opcode table, grouped as the VM executes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Flow(FlowOp),
    Sprite(SpriteOp),
    Interp(InterpOp),
    Var(VarOp),
    // Not in the table; every data opcode is known (05 §11).
    Unknown(i16),
}

const CMPS: [Cmp; 6] = [Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge];
const ARITH: [Arith; 11] = [
    Arith::Set,
    Arith::Add,
    Arith::Sub,
    Arith::Mul,
    Arith::Div,
    Arith::Rem,
    Arith::Add3,
    Arith::Sub3,
    Arith::Mul3,
    Arith::Div3,
    Arith::Rem3,
];
const MATH: [MathFn; 5] = [
    MathFn::Sin,
    MathFn::Cos,
    MathFn::Tan,
    MathFn::Acos,
    MathFn::Atan,
];

// Table index of an op inside its range (`op - first`, step `step`); only called with op in that range.
fn rank(op: i16, first: i16, step: i16) -> usize {
    usize::from((op - first).cast_unsigned()) / usize::from(step.cast_unsigned())
}

impl Op {
    // Decode `ins` from the script bytes it was read from; args past the size read as 0.
    pub fn decode(ins: &Ins, d: &[u8]) -> Op {
        let a = |k: usize| ins.arg(d, k);
        let a2 = |k: usize| [a(k), a(k + 1)];
        let a3 = |k: usize| [a(k), a(k + 1), a(k + 2)];
        let op = ins.op;
        let flow = |f| Op::Flow(f);
        let spr = |s| Op::Sprite(s);
        let interp = |i| Op::Interp(i);
        match op {
            -1 | 1 => flow(FlowOp::End),
            2 => flow(FlowOp::Stop),
            0 | 21 => flow(FlowOp::Nop),
            4 => flow(FlowOp::Jump {
                to: a(0),
                time: a(1),
            }),
            5 => flow(FlowOp::LoopDec {
                var: a(0),
                to: a(1),
                time: a(2),
            }),
            20 | 23 => flow(FlowOp::WaitInterrupt { hide: op == 23 }),
            67..=78 => flow(FlowOp::Branch {
                cmp: CMPS[rank(op, 67, 2)],
                float: op % 2 == 0,
                a: a(0),
                b: a(1),
                to: a(2),
                time: a(3),
            }),
            79 => flow(FlowOp::Wait(a(0))),
            89 => flow(FlowOp::ReturnInterrupt),
            3 => spr(SpriteOp::Sprite(a(0))),
            6 => spr(SpriteOp::Pos(a3(0))),
            7 => spr(SpriteOp::Scale(a2(0))),
            8 | 85 => spr(SpriteOp::Alpha {
                second: op == 85,
                a: a(0),
            }),
            9 | 84 => spr(SpriteOp::Color {
                second: op == 84,
                rgb: a3(0),
            }),
            10 => spr(SpriteOp::FlipX),
            11 => spr(SpriteOp::FlipY),
            12 => spr(SpriteOp::Rotation(a3(0))),
            13 => spr(SpriteOp::AngularVel(a3(0))),
            14 => spr(SpriteOp::ScaleGrowth(a2(0))),
            16 => spr(SpriteOp::BlendAdditive(a(0))),
            22 => spr(SpriteOp::AnchorTopLeft),
            24 => spr(SpriteOp::PosTargetOffset(a(0))),
            25 => spr(SpriteOp::SetTag(a(0))),
            26 | 27 => spr(SpriteOp::UvAdd {
                axis: rank(op, 26, 1),
                d: a(0),
            }),
            28 => spr(SpriteOp::Visible(a(0))),
            30 => spr(SpriteOp::ZwriteOff(a(0))),
            31 => spr(SpriteOp::StageCamera(a(0))),
            80 | 81 => spr(SpriteOp::UvScroll {
                axis: rank(op, 80, 1),
                s: a(0),
            }),
            82 => spr(SpriteOp::BlendMode(a(0))),
            83 => spr(SpriteOp::DrawMode(a(0).raw)),
            88 => spr(SpriteOp::UseColor2(a(0))),
            15 => interp(InterpOp::AlphaFade { a: a(0), dur: a(1) }),
            17..=19 => interp(InterpOp::PosFixed {
                mode: [0, 4, 6][rank(op, 17, 1)],
                end: a3(0),
                dur: a(3),
            }),
            29 => interp(InterpOp::ScaleLinear {
                end: a2(0),
                dur: a(2),
            }),
            32 => interp(InterpOp::Pos {
                dur: a(0),
                mode: a(1),
                end: a3(2),
            }),
            33 | 86 => interp(InterpOp::Color {
                second: op == 86,
                dur: a(0),
                mode: a(1),
                rgb: a3(2),
            }),
            34 | 87 => interp(InterpOp::Alpha {
                second: op == 87,
                dur: a(0),
                mode: a(1),
                a: a(2),
            }),
            35 => interp(InterpOp::Rotation {
                dur: a(0),
                mode: a(1),
                end: a3(2),
            }),
            36 => interp(InterpOp::Scale {
                dur: a(0),
                mode: a(1),
                end: a2(2),
            }),
            37..=58 => Op::Var(VarOp::Arith {
                f: ARITH[rank(op, 37, 2)],
                float: op % 2 == 0,
                dst: a(0),
                b: a(1),
                c: a(2),
            }),
            59 => Op::Var(VarOp::IRand { dst: a(0), n: a(1) }),
            60 => Op::Var(VarOp::FRand { dst: a(0), x: a(1) }),
            61..=65 => Op::Var(VarOp::Math {
                f: MATH[rank(op, 61, 1)],
                dst: a(0),
                x: a(1),
            }),
            66 => Op::Var(VarOp::NormAngle { dst: a(0) }),
            _ => Op::Unknown(op),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type LegacyEntry = (String, u32, u32, Vec<Sprite>, Vec<Script>, Option<Thtx>);

    // Single-pass reference parser (sprites, scripts and decoded THTX together) for the split test.
    fn legacy(b: &[u8]) -> Vec<LegacyEntry> {
        let mut out = Vec::new();
        let mut base = 0;
        loop {
            let c = &b[base..];
            let (n_spr, n_scr) = (u32at(c, 0) as usize, u32at(c, 4) as usize);
            assert_eq!(u32at(c, 0x28), 3);
            let (thtx_off, next) = (u32at(c, 0x30) as usize, u32at(c, 0x38) as usize);
            let name_off = u32at(c, 0x1c) as usize;
            let name_end = c[name_off..].iter().position(|&x| x == 0).unwrap_or(0);
            let name = String::from_utf8_lossy(&c[name_off..name_off + name_end]).into_owned();
            let sprites = (0..n_spr)
                .map(|i| {
                    let o = u32at(c, 0x40 + 4 * i) as usize;
                    let f = |k| f32at(c, o + k);
                    Sprite {
                        id: u32at(c, o),
                        x: f(4),
                        y: f(8),
                        w: f(12),
                        h: f(16),
                    }
                })
                .collect();
            let tbl = 0x40 + 4 * n_spr;
            let offs: Vec<(i32, usize)> = (0..n_scr)
                .map(|i| {
                    (
                        u32at(c, tbl + 8 * i).cast_signed(),
                        u32at(c, tbl + 8 * i + 4) as usize,
                    )
                })
                .collect();
            let limit = [thtx_off, next, c.len()]
                .into_iter()
                .find(|&x| x != 0)
                .unwrap();
            let mut sorted: Vec<usize> = offs.iter().map(|o| o.1).collect();
            sorted.sort();
            let scripts = offs
                .into_iter()
                .map(|(id, o)| {
                    let end = sorted
                        .iter()
                        .copied()
                        .find(|&e| e > o)
                        .unwrap_or(limit)
                        .min(limit);
                    Script {
                        id,
                        data: c[o..end].to_vec(),
                    }
                })
                .collect();
            let tex = (thtx_off != 0).then(|| decode_thtx(&c[thtx_off..]).unwrap());
            out.push((name, u32at(c, 0x0c), u32at(c, 0x10), sprites, scripts, tex));
            if next == 0 {
                return out;
            }
            base += next;
        }
    }

    // The logic / pixel split yields the same sprites, scripts and pixel bytes as the single-pass parser, every file.
    #[test]
    fn split_matches_legacy() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let (mut files, mut tex) = (0, 0);
        for n in ar.names().iter().filter(|n| n.ends_with(".anm")) {
            let b = ar.read(n).unwrap();
            let chunks = parse(&b).unwrap_or_else(|e| panic!("{n}: {e}"));
            let pixels = textures(&b).unwrap_or_else(|e| panic!("{n}: {e}"));
            let old = legacy(&b);
            assert_eq!((chunks.len(), pixels.len()), (old.len(), old.len()), "{n}");
            for ((c, p), (name, w, h, spr, scr, t)) in chunks.iter().zip(&pixels).zip(&old) {
                assert_eq!((&c.name, c.width, c.height), (name, *w, *h), "{n}");
                let bits = |v: &[Sprite]| -> Vec<[u32; 5]> {
                    v.iter()
                        .map(|s| {
                            [
                                s.id,
                                s.x.to_bits(),
                                s.y.to_bits(),
                                s.w.to_bits(),
                                s.h.to_bits(),
                            ]
                        })
                        .collect()
                };
                assert_eq!(bits(&c.sprites), bits(spr), "{n}");
                assert_eq!(&c.scripts, scr, "{n}");
                assert_eq!(c.has_texture, t.is_some(), "{n}");
                let key = |t: &Option<Thtx>| {
                    t.as_ref()
                        .map(|t| (t.format, t.width, t.height, t.rgba.clone()))
                };
                assert!(key(p) == key(t), "{n}: pixels differ");
            }
            tex += pixels.iter().filter(|t| t.is_some()).count();
            files += 1;
        }
        assert_eq!(files, 113);
        assert_eq!(tex, 306);
    }

    // Every op of the opcode table decodes to a known `Op`, every other number to `Unknown`.
    #[test]
    fn decode_covers_signature_table() {
        for op in i16::MIN..=i16::MAX {
            let mut b = Vec::from(op.to_le_bytes());
            b.extend(40u16.to_le_bytes());
            b.extend([0; 4 + 32]);
            let ins = Ins::read(&b, AnmOff(0)).unwrap();
            let known = !matches!(Op::decode(&ins, &b), Op::Unknown(_));
            assert_eq!(known, signature(op).is_some(), "op {op}");
        }
    }

    // Decoding every instruction of every vanilla script: no `Unknown`, sizes match the signature, args by mask.
    #[test]
    fn decode_vanilla_scripts() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let mut n_ins = 0;
        for n in ar.names().iter().filter(|n| n.ends_with(".anm")) {
            for c in parse(&ar.read(n).unwrap()).unwrap() {
                for s in &c.scripts {
                    let mut ip = AnmOff(0);
                    while let Some(i) = Ins::read(&s.data, ip) {
                        let op = Op::decode(&i, &s.data);
                        assert!(!matches!(op, Op::Unknown(_)), "{n}: op {}", i.op);
                        let sig = signature(i.op).unwrap();
                        assert_eq!(i.nargs(), sig.len(), "{n}: op {}", i.op);
                        n_ins += 1;
                        if i.op == -1 {
                            break;
                        }
                        ip = ip.add(usize::from(i.size));
                    }
                }
            }
        }
        assert_eq!(n_ins, 17_117);
    }

    // Truncated args: the header alone is not enough when the size covers args past the end.
    #[test]
    fn read_needs_covered_args() {
        let mut b = Vec::from(3i16.to_le_bytes());
        b.extend(12u16.to_le_bytes());
        b.extend([0; 4]);
        assert!(Ins::read(&b, AnmOff(0)).is_none());
        b.extend(7u32.to_le_bytes());
        let i = Ins::read(&b, AnmOff(0)).unwrap();
        assert_eq!((i.nargs(), i.raw(&b, 0), i.raw(&b, 1)), (1, 7, 0));
        assert_eq!(
            Op::decode(&i, &b),
            Op::Sprite(SpriteOp::Sprite(Arg { raw: 7, var: false }))
        );
    }
}
