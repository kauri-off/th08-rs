//! msg*.dat dialogue file container and instruction headers (10 §2); the interpreter is th08 msg.rs.

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

// File layout (10 §2): u32 count, u32 offset[count] (entries may share an offset), instructions.
pub struct MsgFile {
    data: Vec<u8>,
    pub entries: Vec<usize>,
}

// Instruction header (10 §2): u16 time, u8 opcode, u8 arg_size; next = at + 4 + arg_size.
#[derive(Clone, Copy, Debug)]
pub struct Ins {
    pub time: u16,
    pub op: u8,
    pub size: usize,
    pub at: usize,
}

pub fn parse(data: Vec<u8>) -> Result<MsgFile, MsgError> {
    if data.len() < 4 {
        return Err(MsgError::TooShort);
    }
    let n = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    if data.len() < 4 + 4 * n {
        return Err(MsgError::TableTruncated);
    }
    let entries = (0..n)
        .map(|i| u32::from_le_bytes(data[4 + 4 * i..8 + 4 * i].try_into().unwrap()) as usize)
        .collect::<Vec<_>>();
    if !entries.iter().all(|&o| o + 4 <= data.len()) {
        return Err(MsgError::OffsetOutOfRange);
    }
    Ok(MsgFile { data, entries })
}

impl MsgFile {
    pub fn ins(&self, at: usize) -> Option<Ins> {
        let h = self.data.get(at..at + 4)?;
        let size = h[3] as usize;
        (at + 4 + size <= self.data.len()).then_some(Ins {
            time: u16::from_le_bytes([h[0], h[1]]),
            op: h[2],
            size,
            at,
        })
    }

    pub fn args(&self, i: &Ins) -> &[u8] {
        &self.data[i.at + 4..i.at + 4 + i.size]
    }

    pub fn i16(&self, i: &Ins, k: usize) -> i16 {
        let a = self.args(i);
        a.get(2 * k..2 * k + 2)
            .map_or(0, |b| i16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i32(&self, i: &Ins, k: usize) -> i32 {
        let a = self.args(i);
        a.get(4 * k..4 * k + 4)
            .map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn u8(&self, i: &Ins, k: usize) -> u8 {
        self.args(i).get(k).copied().unwrap_or(0)
    }

    // XOR-0x77 Shift-JIS string starting at byte `from` of the arguments.
    pub fn string(&self, i: &Ins, from: usize) -> SjisString {
        let raw: Vec<u8> = self
            .args(i)
            .iter()
            .skip(from)
            .map(|b| b ^ 0x77)
            .take_while(|&b| b != 0)
            .collect();
        SjisString(raw)
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
    }
}
