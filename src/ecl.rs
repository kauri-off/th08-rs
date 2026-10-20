//! ECL file format: header, subs, timelines, instruction signatures (07 §2, §2.1, §2.2, §7).

use anyhow::{Result, bail, ensure};

// Header dword 0x00 (07 §2): checked, load fails otherwise.
pub const MAGIC: u32 = 0x800;
// Sub instruction header (07 §2.1): time, opcode, size, unused byte, rank mask, var mask.
pub const SUB_HEADER: usize = 12;

pub struct Ecl {
    pub data: Vec<u8>,
    // Sub offsets from file start (07 §2, table at 0x48).
    pub subs: Vec<usize>,
    // The `ntl` timelines that are run (07 §2: the extra last entry is never run).
    pub timelines: Vec<usize>,
}

// Sub instruction (07 §2.1).
#[derive(Clone, Copy, Debug)]
pub struct Ins {
    pub off: usize,
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

// Timeline instruction (07 §2.2): +0 time (< 0 end), +4 op, +6 u8 size, +7 u8 rank mask.
#[derive(Clone, Copy, Debug)]
pub struct TlIns {
    pub off: usize,
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

// Loader 0x418330 (07 §2): offsets stay file-relative instead of being relocated to pointers.
pub fn parse(data: Vec<u8>) -> Result<Ecl> {
    ensure!(
        data.len() >= 0x48 && u32_at(&data, 0) == Some(MAGIC),
        "not an ECL file"
    );
    let nsubs = i16_at(&data, 4).unwrap_or(0).max(0) as usize;
    let ntl = i16_at(&data, 6).unwrap_or(0).clamp(0, 16) as usize;
    let mut timelines = Vec::new();
    for k in 0..ntl {
        let o = u32_at(&data, 8 + 4 * k).unwrap_or(0) as usize;
        ensure!(o != 0 && o < data.len(), "timeline {k}: bad offset {o:#x}");
        timelines.push(o);
    }
    let mut subs = Vec::new();
    for k in 0..nsubs {
        let Some(o) = u32_at(&data, 0x48 + 4 * k) else {
            bail!("sub table truncated")
        };
        ensure!((o as usize) < data.len(), "sub {k}: bad offset {o:#x}");
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
        let size = i16_at(d, off + 6)? as usize;
        Some(Ins {
            off,
            time: u32_at(d, off)? as i32,
            op: i16_at(d, off + 4)?,
            size: size.max(SUB_HEADER),
            rank: *d.get(off + 9)?,
            mask: u16::from_le_bytes(d.get(off + 10..off + 12)?.try_into().ok()?),
        })
    }

    pub fn arg(&self, ins: &Ins, k: usize) -> u32 {
        u32_at(&self.data, ins.off + SUB_HEADER + 4 * k).unwrap_or(0)
    }

    // 07 §6: literal destinations write into the loaded file (shared by every enemy running the instruction).
    pub fn set_arg(&mut self, ins: &Ins, k: usize, v: u32) {
        let o = ins.off + SUB_HEADER + 4 * k;
        if o + 4 <= ins.off + ins.size
            && let Some(b) = self.data.get_mut(o..o + 4)
        {
            b.copy_from_slice(&v.to_le_bytes());
        }
    }

    pub fn bytes(&self, ins: &Ins) -> &[u8] {
        &self.data[ins.off..(ins.off + ins.size).min(self.data.len())]
    }

    pub fn tl_ins(&self, off: usize) -> Option<TlIns> {
        let d = &self.data;
        Some(TlIns {
            off,
            time: u32_at(d, off)? as i32,
            op: i16_at(d, off + 4)?,
            size: (*d.get(off + 6)? as usize).max(8),
            rank: *d.get(off + 7)?,
        })
    }

    pub fn tl_arg(&self, ins: &TlIns, k: usize) -> u32 {
        u32_at(&self.data, ins.off + 8 + 4 * k).unwrap_or(0)
    }

    pub fn sub_start(&self, sub: i32) -> Option<usize> {
        usize::try_from(sub)
            .ok()
            .and_then(|s| self.subs.get(s).copied())
    }
}

// Argument letters (07 §7): i int, f float, V/W int/float destination, T time, O offset, S sub, r raw;
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
        | 146..=149 => "i",
        159 | 160 | 163 | 168 | 173..=177 | 182..=184 => "i",
        4 => "TO",
        5 => "TOV",
        6 | 8 | 10..=14 => "Vi",
        7 | 9 | 15..=19 | 32 | 33 => "Wf",
        20..=24 => "Vii",
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
        73 => "ifff",
        74 => "iff",
        75 => "ffff",
        86 => "Vii",
        87 => "Wii",
        88 => "iS",
        90..=92 => "Sffiii",
        93 | 94 => "Sfffiii",
        96..=104 => "B",
        111 => "iiiiiff",
        114 | 115 => "L",
        117 | 118 | 167 | 171 => "if",
        119 => "ifff",
        122 => "C",
        126 => "Si",
        128 => "rfffi",
        129 | 130 | 138 | 145 | 151 | 155 | 156 => "r",
        133 => "iiS",
        134 | 135 => "iS",
        139 => "iii",
        140 => "iiifff",
        150 => "rr",
        152 => "ffiiii",
        157 => "riii",
        158 => "iiii",
        164 => "ifff",
        172 => "iff",
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

// Timeline instruction sizes by op (07 §3 table args).
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
        let Some(ar) = crate::archive::test_archive() else {
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
                    if matches!(ins.op, 4 | 5 | 40..=51) {
                        let k = if ins.op == 4 || ins.op == 5 { 1 } else { 3 };
                        let target = (off as i64 + ecl.arg(&ins, k) as i32 as i64) as usize;
                        assert!(ecl.ins(target).is_some(), "{f} sub {s}: bad jump");
                    }
                    if matches!(ins.op, 52 | 88 | 135) {
                        let k = if ins.op == 52 { 0 } else { 1 };
                        let sub = ecl.arg(&ins, k) as i32;
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
}
