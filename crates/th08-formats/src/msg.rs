//! msg*.dat dialogue file container and instruction headers (10 §2); the interpreter is th08 msg.rs.

use crate::reader::{Reader, Truncated};
use crate::sjis::SjisString;

#[derive(Debug, thiserror::Error)]
pub enum MsgError {
    #[error("msg file too short")]
    TooShort,
    #[error("msg entry table truncated")]
    TableTruncated,
    #[error("msg entry offset out of range")]
    OffsetOutOfRange,
}

// Byte offset of an instruction from the start of the msg file (10 §2 entry offsets, instruction chain).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MsgOff(pub u32);

impl MsgOff {
    pub fn idx(self) -> usize {
        self.0 as usize
    }

    // Interpreter ip (a `usize` offset that came from a `MsgOff`) back to the offset type.
    pub fn from_idx(at: usize) -> Self {
        Self(at as u32)
    }
}

impl std::fmt::LowerHex for MsgOff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::LowerHex::fmt(&self.0, f)
    }
}

// File layout (10 §2): u32 count, u32 offset[count] (entries may share an offset), instructions.
pub struct MsgFile {
    data: Vec<u8>,
    pub entries: Vec<MsgOff>,
}

// Instruction header (10 §2): u16 time, u8 opcode, u8 arg_size; next = at + 4 + arg_size.
#[derive(Clone, Copy, Debug)]
pub struct Ins {
    pub time: u16,
    pub op: u8,
    pub size: usize,
    pub at: MsgOff,
}

impl Ins {
    pub fn next(&self) -> MsgOff {
        MsgOff::from_idx(self.at.idx() + 4 + self.size)
    }
}

pub fn parse(data: Vec<u8>) -> Result<MsgFile, MsgError> {
    let mut r = Reader::new(&data);
    let n = r.u32().map_err(|_| MsgError::TooShort)? as usize;
    if data.len() < 4 + 4 * n {
        return Err(MsgError::TableTruncated);
    }
    let entries = (0..n)
        .map(|_| r.u32().map(MsgOff))
        .collect::<Result<Vec<_>, Truncated>>()
        .map_err(|_| MsgError::TableTruncated)?;
    if !entries.iter().all(|&o| o.idx() + 4 <= data.len()) {
        return Err(MsgError::OffsetOutOfRange);
    }
    Ok(MsgFile { data, entries })
}

// Argument readers over an instruction's argument bytes; bytes past the argument size read as 0.
fn arg_i16(a: &[u8], k: usize) -> i16 {
    Reader::at(a, 2 * k).i16().unwrap_or(0)
}

fn arg_i32(a: &[u8], k: usize) -> i32 {
    Reader::at(a, 4 * k).i32().unwrap_or(0)
}

fn arg_u8(a: &[u8], k: usize) -> u8 {
    a.get(k).copied().unwrap_or(0)
}

// XOR-0x77 Shift-JIS string starting at byte `from` of the arguments, up to the byte that decodes to 0 (10 §2).
fn arg_string(a: &[u8], from: usize) -> SjisString {
    SjisString(
        a.iter()
            .skip(from)
            .map(|b| b ^ 0x77)
            .take_while(|&b| b != 0)
            .collect(),
    )
}

impl MsgFile {
    pub fn ins(&self, at: MsgOff) -> Option<Ins> {
        let at_i = at.idx();
        let h = self.data.get(at_i..at_i + 4)?;
        let size = usize::from(h[3]);
        (at_i + 4 + size <= self.data.len()).then_some(Ins {
            time: u16::from_le_bytes([h[0], h[1]]),
            op: h[2],
            size,
            at,
        })
    }

    pub fn args(&self, i: &Ins) -> &[u8] {
        let at = i.at.idx() + 4;
        &self.data[at..at + i.size]
    }

    pub fn i16(&self, i: &Ins, k: usize) -> i16 {
        arg_i16(self.args(i), k)
    }

    pub fn i32(&self, i: &Ins, k: usize) -> i32 {
        arg_i32(self.args(i), k)
    }

    pub fn u8(&self, i: &Ins, k: usize) -> u8 {
        arg_u8(self.args(i), k)
    }

    pub fn string(&self, i: &Ins, from: usize) -> SjisString {
        arg_string(self.args(i), from)
    }

    pub fn op(&self, i: &Ins) -> Op {
        Op::decode(i, self.args(i))
    }

    // Text ops (3, 16, 19, 20) from `start` up to, not including, `at` in file order: the `nth` of a text key
    // (I18N §2.3; the instruction chain of 10 §2).
    pub fn text_ops_before(&self, start: MsgOff, at: MsgOff) -> u16 {
        let (mut n, mut p) = (0u16, start);
        while p < at {
            let Some(i) = self.ins(p) else { break };
            if matches!(i.op, 3 | 16 | 19 | 20) {
                n = n.saturating_add(1);
            }
            p = i.next();
        }
        n
    }
}

// One instruction decoded by opcode (10 §5.1 table, `Msg_Run` switch 0x433DB3); args in file order, raw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    // 0 end: entry = −1.
    End,
    // 1 face_script: ANM `script` of face file `face` on face[face].
    FaceScript {
        face: i16,
        script: i16,
    },
    // 2 face_sprite.
    FaceSprite {
        face: i16,
        sprite: i16,
    },
    // 3 text_line: start text.anm script `line` on line[line], render `text`.
    TextLine {
        speaker: i16,
        line: i16,
        text: SjisString,
    },
    // 4 wait up to n frames (stall, 10 §5.2).
    Wait(i32),
    // 5 face_interrupt.
    FaceInterrupt {
        face: i16,
        interrupt: u8,
    },
    // 6 release_timeline.
    ReleaseTimeline,
    // 7 bgm: n < 0 stops the music, else plays slot n.
    Bgm(i32),
    // 8 boss_card; its arguments are ignored (10 §5.1).
    BossCard,
    // 9 stage_end; the i32 argument (always 0) is ignored.
    StageEnd,
    // 10 halt: stall forever.
    Halt,
    // 11 halt_final: game-clear trigger in stage idx 6–8, else as 10.
    HaltFinal,
    // 12 bgm_fade.
    BgmFade,
    // 13 skippable.
    Skippable(u8),
    // 14 flash.
    Flash,
    // 15 speaker4: highlight `who`; sprites s_k ≥ 0 set face[k] (negative = keep).
    Speaker4 {
        who: u32,
        sprites: [i32; 4],
    },
    // 16 text: next line of the box.
    Text(SjisString),
    // 17 speaker: like 15 for face[who] only.
    Speaker {
        who: u32,
        sprite: i32,
    },
    // 18 textbox.
    Textbox(u8),
    // 19 / 20 text on line 0 / 1 with text_colour[0].
    Text0(SjisString),
    Text1(SjisString),
    // 21 choice with timeout n (stall, 10 §5.4).
    Choice(i32),
    // 22 choice_jump.
    ChoiceJump,
    // Opcode outside 0..=22 (none in the data).
    Unknown(u8),
}

impl Op {
    pub fn decode(i: &Ins, a: &[u8]) -> Self {
        match i.op {
            0 => Self::End,
            1 => Self::FaceScript {
                face: arg_i16(a, 0),
                script: arg_i16(a, 1),
            },
            2 => Self::FaceSprite {
                face: arg_i16(a, 0),
                sprite: arg_i16(a, 1),
            },
            3 => Self::TextLine {
                speaker: arg_i16(a, 0),
                line: arg_i16(a, 1),
                text: arg_string(a, 4),
            },
            4 => Self::Wait(arg_i32(a, 0)),
            5 => Self::FaceInterrupt {
                face: arg_i16(a, 0),
                interrupt: arg_u8(a, 2),
            },
            6 => Self::ReleaseTimeline,
            7 => Self::Bgm(arg_i32(a, 0)),
            8 => Self::BossCard,
            9 => Self::StageEnd,
            10 => Self::Halt,
            11 => Self::HaltFinal,
            12 => Self::BgmFade,
            13 => Self::Skippable(arg_u8(a, 0)),
            14 => Self::Flash,
            15 => Self::Speaker4 {
                who: arg_i32(a, 0).cast_unsigned(),
                sprites: std::array::from_fn(|k| arg_i32(a, 1 + k)),
            },
            16 => Self::Text(arg_string(a, 0)),
            17 => Self::Speaker {
                who: arg_i32(a, 0).cast_unsigned(),
                sprite: arg_i32(a, 1),
            },
            18 => Self::Textbox(arg_u8(a, 0)),
            19 => Self::Text0(arg_string(a, 0)),
            20 => Self::Text1(arg_string(a, 0)),
            21 => Self::Choice(arg_i32(a, 0)),
            22 => Self::ChoiceJump,
            op => Self::Unknown(op),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_string() {
        assert!(parse(vec![1, 0, 0]).is_err());
        assert!(parse(vec![2, 0, 0, 0, 12, 0, 0, 0]).is_err());
        // Entry 0 at 8: time 3, op 3, 6 arg bytes: i16 1, i16 0, XOR-0x77 "A".
        let f = parse(vec![
            1,
            0,
            0,
            0,
            8,
            0,
            0,
            0,
            3,
            0,
            3,
            6,
            1,
            0,
            0,
            0,
            b'A' ^ 0x77,
            0x77,
        ])
        .unwrap();
        let i = f.ins(f.entries[0]).unwrap();
        assert_eq!((i.time, i.op, i.size), (3, 3, 6));
        assert_eq!(f.i16(&i, 0), 1);
        assert_eq!(f.string(&i, 4).as_bytes(), b"A");
        assert_eq!(i.next(), MsgOff(18));
        assert_eq!(
            f.op(&i),
            Op::TextLine {
                speaker: 1,
                line: 0,
                text: SjisString(b"A".to_vec())
            }
        );
    }

    // 10 §5.1 "count" column: every instruction of every msg file decodes to a known op, with the listed counts
    // (each entry walked to its op 0, past halts to the unreachable `end`, shared tails counted once).
    #[test]
    fn all_msg_ops_decode() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let mut ops = [0usize; 23];
        let mut files = 0;
        for n in ar
            .names()
            .iter()
            .filter(|n| n.starts_with("msg") && n.ends_with(".dat"))
        {
            let f = parse(ar.read(n).unwrap()).unwrap();
            files += 1;
            let mut seen = std::collections::BTreeSet::new();
            for &e in &f.entries {
                let mut at = e;
                while seen.insert(at) {
                    let Some(i) = f.ins(at) else { break };
                    let op = f.op(&i);
                    if let Op::Unknown(op) = op {
                        panic!("{n}: op {op} at {at:?}");
                    }
                    ops[usize::from(i.op)] += 1;
                    if op == Op::End {
                        break;
                    }
                    at = i.next();
                }
            }
        }
        assert!(files > 0);
        assert_eq!(
            ops,
            [
                136, 256, 1, 18, 1565, 177, 40, 49, 35, 37, 37, 36, 37, 204, 12, 216, 2214, 1336,
                4, 4, 4, 4, 4
            ]
        );
    }
}
