//! thbgm.fmt track table and the thbgm.dat header probe (02 §1, §2).

use crate::reader::{Reader, u32_at};

// thbgm.fmt record (0x34 bytes); positions are in stereo frames (bytes / 4).
#[derive(Clone, Debug)]
pub struct Track {
    pub name: String,
    pub offset: u64,
    pub loop_start: usize,
    // Loop end +0x1C = frames stored in thbgm.dat; +0x14 (full length) is larger and never read.
    pub length: usize,
}

// thbgm.dat magic "ZWAV" as a little-endian u32.
const ZWAV: u32 = u32::from_le_bytes(*b"ZWAV");

// 52-byte records up to the one whose name starts with NUL.
pub fn parse_fmt(data: &[u8]) -> Vec<Track> {
    data.as_chunks::<52>()
        .0
        .iter()
        .take_while(|e| e[0] != 0)
        .map(|e| {
            let u = |o: usize| u32_at(e, o);
            Track {
                name: String::from_utf8_lossy(&e[..16])
                    .trim_end_matches('\0')
                    .to_string(),
                offset: u64::from(u(0x10)),
                loop_start: u(0x18) as usize / 4,
                length: u(0x1c) as usize / 4,
            }
        })
        .collect()
}

// thbgm.dat header probe made by `Config_Load`: "ZWAV", version 1, 0x800.
pub fn bgm_header_valid(h: &[u8]) -> bool {
    let mut r = Reader::new(h);
    r.u32() == Ok(ZWAV) && r.u32() == Ok(1) && r.u32() == Ok(0x800)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_records() {
        let mut d = vec![0u8; 52 * 2];
        d[..11].copy_from_slice(b"th08_01.wav");
        d[0x10] = 0x10;
        d[0x18] = 8;
        d[0x1c] = 0x40;
        let t = parse_fmt(&d);
        assert_eq!(t.len(), 1);
        assert_eq!(
            (
                t[0].name.as_str(),
                t[0].offset,
                t[0].loop_start,
                t[0].length
            ),
            ("th08_01.wav", 16, 2, 16)
        );
        let mut h = b"ZWAV".to_vec();
        h.extend(1u32.to_le_bytes());
        h.extend(0x800u32.to_le_bytes());
        assert!(bgm_header_valid(&h));
        assert!(!bgm_header_valid(&h[..8]));
    }
}
