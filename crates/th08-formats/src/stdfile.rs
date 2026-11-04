//! .std stage background file parser (06 §1).

use crate::reader::{Reader, Truncated};

#[derive(Debug, thiserror::Error)]
pub enum StdError {
    #[error("std: {0}")]
    Truncated(#[from] Truncated),
    #[error("std: quad record size {size} at {at:#x}")]
    QuadSize { size: usize, at: usize },
    #[error("std: instance of object {0}")]
    Instance(i16),
}

pub type Result<T> = std::result::Result<T, StdError>;

pub type Vec3 = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum QuadKind {
    // Type 0, 0x1C bytes: pos relative to the object origin; w/h 0 = keep sprite size.
    Sprite { pos: Vec3, w: f32, h: f32 },
    // Type 1, 0x24 bytes: points A/B; w 0 = sprite width (06 §11.3).
    Ribbon { a: Vec3, b: Vec3, w: f32 },
}

#[derive(Clone, Copy, Debug)]
pub struct Quad {
    // +4 ANM script index in stgNbg.anm.
    pub script: i16,
    // +6 VM index, assigned in load order (06 §2 step 3).
    pub vm: usize,
    pub kind: QuadKind,
}

#[derive(Clone, Debug)]
pub struct Object {
    pub id: i16,
    // +2 layer 0..3.
    pub layer: u8,
    // +3 runtime: bit 0 script running, bit 1 drawn this frame.
    pub flags: u8,
    // +4 origin.
    pub pos: Vec3,
    // +0x10 bounding-box extent.
    pub size: Vec3,
    pub quads: Vec<Quad>,
}

// 16-byte instance record; the +2 field (always 256) is never read.
#[derive(Clone, Copy, Debug)]
pub struct Instance {
    pub object: usize,
    pub pos: Vec3,
}

// 20-byte script record: time in frames (−1 terminates), opcode, 3 args.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    pub time: i32,
    pub op: i16,
    pub args: [u32; 3],
}

impl Record {
    pub fn i(&self, k: usize) -> i32 {
        self.args[k].cast_signed()
    }
    pub fn f(&self, k: usize) -> f32 {
        f32::from_bits(self.args[k])
    }
    pub fn v(&self) -> Vec3 {
        [self.f(0), self.f(1), self.f(2)]
    }
}

// Camera vector / fov slot of the interpolation arrays (06 §4, §6): ops 5/7/9 and 6/8/10/12 by (op − 5) / 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CamSlot {
    Pos = 0,
    Facing = 1,
    Up = 2,
    Fov = 3,
}

impl CamSlot {
    pub fn index(self) -> usize {
        usize::from(self as u8)
    }

    const VEC: [Self; 3] = [Self::Pos, Self::Facing, Self::Up];
}

// One script record decoded by opcode (06 §4 table, `Stage_Calc` switch); args as the original reads them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    // 0 set_origin.
    SetOrigin(Vec3),
    // 1 fog: current fog and fog target = (colour, near, far).
    Fog { color: u32, near: f32, far: f32 },
    // 2 fog_time.
    FogTime { dur: i32 },
    // 3 halt until a section request.
    Halt,
    // 4 jump: instruction index and script clock.
    Jump { index: i32, time: i32 },
    // 5 / 7 / 9 cam_pos / cam_facing / cam_up target.
    CamTarget { slot: CamSlot, v: Vec3 },
    // 6 / 8 / 10 / 12 interpolation duration and mode (slot Fov for 12).
    CamTime { slot: CamSlot, dur: i32, mode: i32 },
    // 11 fov target.
    Fov(f32),
    // 13 clear colour (D3DCOLOR).
    ClearColor(u32),
    // 14 / 19 / 24 Hermite start vector of pos / facing / up.
    CamStart { slot: CamSlot, v: Vec3 },
    // 15 / 20 / 25 end vector.
    CamEnd { slot: CamSlot, v: Vec3 },
    // 16 / 21 / 26 start tangent.
    CamTan0 { slot: CamSlot, v: Vec3 },
    // 17 / 22 / 27 end tangent.
    CamTan1 { slot: CamSlot, v: Vec3 },
    // 18 / 23 / 28 interpolation with mode 7 (Hermite).
    CamHermite { slot: CamSlot, dur: i32 },
    // 29 / 30 / 34 bg_anm on VM A / B / C (`vm` 0 / 1 / 2; op 30's disable quirk is the interpreter's, 06 §4).
    BgAnm { vm: u8, script: i32 },
    // 31 section label (no-op when executed, 06 §4.1).
    Section(i32),
    // 32 cam_eye_offset.
    EyeOffset(Vec3),
    // 33 sway: mode (the interpreter keeps the low byte).
    Sway { mode: i32 },
    // Any other opcode: ignored, the index just advances (06 §4).
    Unknown(i16),
}

impl Op {
    pub fn decode(r: &Record) -> Self {
        let v = r.v();
        match r.op {
            0 => Self::SetOrigin(v),
            1 => Self::Fog {
                color: r.args[0],
                near: r.f(1),
                far: r.f(2),
            },
            2 => Self::FogTime { dur: r.i(0) },
            3 => Self::Halt,
            4 => Self::Jump {
                index: r.i(0),
                time: r.i(1),
            },
            5 | 7 | 9 => Self::CamTarget {
                slot: CamSlot::VEC[usize::from((r.op - 5).cast_unsigned()) / 2],
                v,
            },
            6 | 8 | 10 | 12 => Self::CamTime {
                slot: [CamSlot::Pos, CamSlot::Facing, CamSlot::Up, CamSlot::Fov]
                    [usize::from((r.op - 6).cast_unsigned()) / 2],
                dur: r.i(0),
                mode: r.i(1),
            },
            11 => Self::Fov(r.f(0)),
            13 => Self::ClearColor(r.args[0]),
            14..=28 => {
                let k = usize::from((r.op - 14).cast_unsigned());
                let slot = CamSlot::VEC[k / 5];
                match k % 5 {
                    0 => Self::CamStart { slot, v },
                    1 => Self::CamEnd { slot, v },
                    2 => Self::CamTan0 { slot, v },
                    3 => Self::CamTan1 { slot, v },
                    _ => Self::CamHermite { slot, dur: r.i(0) },
                }
            }
            29 => Self::BgAnm {
                vm: 0,
                script: r.i(0),
            },
            30 => Self::BgAnm {
                vm: 1,
                script: r.i(0),
            },
            34 => Self::BgAnm {
                vm: 2,
                script: r.i(0),
            },
            31 => Self::Section(r.i(0)),
            32 => Self::EyeOffset(v),
            33 => Self::Sway { mode: r.i(0) },
            op => Self::Unknown(op),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Std {
    // +2 total quads = VMs allocated.
    pub quad_count: usize,
    // +0x290 BGM names for music slots 0..3 (" " = none).
    pub bgm: [String; 4],
    pub objects: Vec<Object>,
    pub instances: Vec<Instance>,
    pub script: Vec<Record>,
}

// Header, objects with their quads, instance list and script; the object offsets stay file offsets.
pub fn parse(b: &[u8]) -> Result<Std> {
    let mut h = Reader::new(b);
    let nobj = usize::from(h.i16()?.max(0).cast_unsigned());
    let quad_count = usize::from(h.i16()?.max(0).cast_unsigned());
    let inst_off = h.u32()? as usize;
    let script_off = h.u32()? as usize;
    let bgm = std::array::from_fn(|k| {
        let s = b
            .get(0x290 + k * 0x80..0x290 + (k + 1) * 0x80)
            .unwrap_or_default();
        let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        String::from_utf8_lossy(&s[..end]).into_owned()
    });
    let mut objects = Vec::with_capacity(nobj);
    let mut vm = 0;
    for k in 0..nobj {
        let o = Reader::at(b, 0x490 + k * 4).u32()? as usize;
        let mut r = Reader::at(b, o);
        let mut obj = Object {
            id: r.i16()?,
            layer: r.u8()?,
            flags: 0,
            pos: r.skip(1).vec3()?,
            size: r.vec3()?,
            quads: Vec::new(),
        };
        let mut q = o + 0x1C;
        loop {
            let mut r = Reader::at(b, q);
            let ty = r.i16()?;
            if ty < 0 {
                break;
            }
            let size = r.i16()?;
            if size < 4 {
                return Err(StdError::QuadSize {
                    size: usize::from(size.cast_unsigned()),
                    at: q,
                });
            }
            let size = usize::from(size.cast_unsigned());
            // 06 §2: the original does not filter, but shipped files have only types 0 and 1.
            if !matches!(ty, 0 | 1) {
                q += size;
                continue;
            }
            let script = r.i16()?;
            r.skip(2);
            let kind = match ty {
                0 => QuadKind::Sprite {
                    pos: r.vec3()?,
                    w: r.f32()?,
                    h: r.f32()?,
                },
                _ => QuadKind::Ribbon {
                    a: r.vec3()?,
                    b: r.vec3()?,
                    w: r.f32()?,
                },
            };
            obj.quads.push(Quad { script, vm, kind });
            vm += 1;
            q += size;
        }
        objects.push(obj);
    }
    let mut instances = Vec::new();
    let mut r = Reader::at(b, inst_off);
    loop {
        let idx = r.i16()?;
        if idx < 0 {
            break;
        }
        let object = usize::from(idx.cast_unsigned());
        if object >= objects.len() {
            return Err(StdError::Instance(idx));
        }
        instances.push(Instance {
            object,
            pos: r.skip(2).vec3()?,
        });
    }
    let mut script = Vec::new();
    let mut r = Reader::at(b, script_off);
    loop {
        let time = r.i32()?;
        let op = r.i16()?;
        if time == -1 {
            script.push(Record {
                time,
                op,
                args: [0; 3],
            });
            break;
        }
        script.push(Record {
            time,
            op,
            args: [r.skip(2).u32()?, r.u32()?, r.u32()?],
        });
    }
    Ok(Std {
        // Port-only guard against a header count below the quads found.
        quad_count: quad_count.max(vm),
        bgm,
        objects,
        instances,
        script,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_std_parse() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let mut files = 0;
        let (mut sprites, mut ribbons) = (0, 0);
        for n in ar.names().iter().filter(|n| n.ends_with(".std")) {
            let b = ar.read(n).unwrap();
            let s = parse(&b).unwrap_or_else(|e| panic!("{n}: {e}"));
            assert_eq!(
                s.quad_count,
                usize::from(u16::from_le_bytes([b[2], b[3]])),
                "{n}"
            );
            for q in s.objects.iter().flat_map(|o| &o.quads) {
                match q.kind {
                    QuadKind::Sprite { .. } => sprites += 1,
                    QuadKind::Ribbon { .. } => ribbons += 1,
                }
            }
            for r in &s.script {
                if let Op::Jump { index, .. } = Op::decode(r) {
                    assert!(
                        usize::try_from(index).unwrap() < s.script.len(),
                        "{n}: jump"
                    );
                }
            }
            files += 1;
        }
        assert_eq!(files, 18);
        assert_eq!((sprites, ribbons), (384, 168));
        // 06 §4 "count" column: uses of each opcode in the 18 files, terminators excluded.
        let want = [
            (0, 18),
            (1, 52),
            (2, 25),
            (4, 35),
            (5, 213),
            (6, 95),
            (7, 53),
            (8, 19),
            (9, 26),
            (11, 18),
            (13, 11),
            (29, 24),
            (31, 17),
            (32, 10),
            (33, 18),
            (34, 7),
        ];
        let mut got = std::collections::BTreeMap::new();
        for n in ar.names().iter().filter(|n| n.ends_with(".std")) {
            for r in parse(&ar.read(n).unwrap()).unwrap().script {
                if r.time == -1 {
                    continue;
                }
                assert!(!matches!(Op::decode(&r), Op::Unknown(_)), "{n}: {r:?}");
                *got.entry(r.op).or_insert(0) += 1;
            }
        }
        assert_eq!(got.into_iter().collect::<Vec<_>>(), want);
    }
}
