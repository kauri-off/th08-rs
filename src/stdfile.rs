//! .std stage background file parser (06 §1).

use anyhow::{Context, Result, ensure};

pub type Vec3 = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum QuadKind {
    // Type 0, 0x1C bytes: pos relative to the object origin; w/h 0 = keep sprite size (06 §1.2).
    Sprite { pos: Vec3, w: f32, h: f32 },
    // Type 1, 0x24 bytes: points A/B; w 0 = sprite width (06 §1.2, §11.3).
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
// Object record (06 §1.1).
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

// 16-byte instance record; the +2 field (always 256) is never read (06 §1.3).
#[derive(Clone, Copy, Debug)]
pub struct Instance {
    pub object: usize,
    pub pos: Vec3,
}

// 20-byte script record: time in frames (−1 terminates), opcode, 3 args (06 §1.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    pub time: i32,
    pub op: i16,
    pub args: [u32; 3],
}

impl Record {
    pub fn i(&self, k: usize) -> i32 {
        self.args[k] as i32
    }
    pub fn f(&self, k: usize) -> f32 {
        f32::from_bits(self.args[k])
    }
    pub fn v(&self) -> Vec3 {
        [self.f(0), self.f(1), self.f(2)]
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

fn rd<const N: usize>(b: &[u8], o: usize) -> Result<[u8; N]> {
    b.get(o..o + N)
        .and_then(|s| s.try_into().ok())
        .with_context(|| format!("std: read past end at {o:#x}"))
}
fn i16at(b: &[u8], o: usize) -> Result<i16> {
    Ok(i16::from_le_bytes(rd(b, o)?))
}
fn u32at(b: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(rd(b, o)?))
}
fn f32at(b: &[u8], o: usize) -> Result<f32> {
    Ok(f32::from_bits(u32at(b, o)?))
}
fn v3at(b: &[u8], o: usize) -> Result<Vec3> {
    Ok([f32at(b, o)?, f32at(b, o + 4)?, f32at(b, o + 8)?])
}

pub fn parse(b: &[u8]) -> Result<Std> {
    let nobj = i16at(b, 0)?.max(0) as usize;
    let quad_count = i16at(b, 2)?.max(0) as usize;
    let inst_off = u32at(b, 4)? as usize;
    let script_off = u32at(b, 8)? as usize;
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
        let o = u32at(b, 0x490 + k * 4)? as usize;
        let mut obj = Object {
            id: i16at(b, o)?,
            layer: rd::<1>(b, o + 2)?[0],
            flags: 0,
            pos: v3at(b, o + 4)?,
            size: v3at(b, o + 0x10)?,
            quads: Vec::new(),
        };
        let mut q = o + 0x1C;
        loop {
            let ty = i16at(b, q)?;
            if ty < 0 {
                break;
            }
            let size = i16at(b, q + 2)? as usize;
            ensure!(size >= 4, "std: quad record size {size} at {q:#x}");
            let kind = match ty {
                0 => QuadKind::Sprite {
                    pos: v3at(b, q + 8)?,
                    w: f32at(b, q + 0x14)?,
                    h: f32at(b, q + 0x18)?,
                },
                1 => QuadKind::Ribbon {
                    a: v3at(b, q + 8)?,
                    b: v3at(b, q + 0x14)?,
                    w: f32at(b, q + 0x20)?,
                },
                // SPEC-GAP: other non-negative quad types do not occur (06 §1.2); the port skips them.
                _ => {
                    q += size;
                    continue;
                }
            };
            obj.quads.push(Quad {
                script: i16at(b, q + 4)?,
                vm,
                kind,
            });
            vm += 1;
            q += size;
        }
        objects.push(obj);
    }
    let mut instances = Vec::new();
    let mut o = inst_off;
    loop {
        let idx = i16at(b, o)?;
        if idx < 0 {
            break;
        }
        ensure!(
            (idx as usize) < objects.len(),
            "std: instance of object {idx}"
        );
        instances.push(Instance {
            object: idx as usize,
            pos: v3at(b, o + 4)?,
        });
        o += 16;
    }
    let mut script = Vec::new();
    let mut o = script_off;
    loop {
        let time = u32at(b, o)? as i32;
        let op = i16at(b, o + 4)?;
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
            args: [u32at(b, o + 8)?, u32at(b, o + 12)?, u32at(b, o + 16)?],
        });
        o += 20;
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
        let Some(ar) = crate::archive::test_archive() else {
            return;
        };
        let mut files = 0;
        let (mut sprites, mut ribbons) = (0, 0);
        for n in ar.names().iter().filter(|n| n.ends_with(".std")) {
            let b = ar.read(n).unwrap();
            let s = parse(&b).unwrap_or_else(|e| panic!("{n}: {e}"));
            assert_eq!(
                s.quad_count,
                i16::from_le_bytes([b[2], b[3]]) as usize,
                "{n}"
            );
            for q in s.objects.iter().flat_map(|o| &o.quads) {
                match q.kind {
                    QuadKind::Sprite { .. } => sprites += 1,
                    QuadKind::Ribbon { .. } => ribbons += 1,
                }
            }
            for r in &s.script {
                if r.op == 4 {
                    assert!((r.i(0) as usize) < s.script.len(), "{n}: jump");
                }
            }
            files += 1;
        }
        assert_eq!(files, 18);
        assert_eq!((sprites, ribbons), (384, 168));
    }
}
