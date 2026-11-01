//! .rpy replay files (15 §3.4–§3.7, 11 §6): header, info block, per-stage records; load and save codec.

use crate::ByteSource;
use crate::archive::{lzss_compress, lzss_decompress};
use crate::scoredat::{EXE_SIZE, EXE_SUM, exe_check_in};
use crate::sjis::SjisStr;

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("not a th08 replay")]
    NotReplay,
    #[error("replay size")]
    Size,
    #[error("replay checksum")]
    Checksum,
    #[error("replay too short")]
    TooShort,
    #[error("replay recorded in slow mode")]
    SlowMode,
    #[error("replay exe check")]
    ExeCheck,
    #[error("replay stage {0} record")]
    StageRecord(usize),
}

pub type Result<T> = std::result::Result<T, ReplayError>;

// File header size, end of the info block, stage record header size and checksum base (15 §3.5, 11 §6).
pub const HEADER: usize = 0x68;
pub const INFO: usize = 0x134;
pub const REC: usize = 0x24;
const CHECK_BASE: u32 = 0x3F000318;

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn put_u32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

// Stage record, 0x24-byte header (11 §6) + input words + fps bytes (15 §3.4).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StageRecord {
    // +0x00 score ÷10 at the end of this stage (filled when the next stage starts or at finish).
    pub score: u32,
    pub point_items: i32,
    pub graze: u32,
    pub extends: i32,
    pub next_extend: i32,
    pub max_point: i32,
    pub gauge: i16,
    // +0x1A stage_seed (11 §1.3).
    pub seed: u16,
    pub power: u8,
    pub lives: u8,
    pub bombs: u8,
    // +0x1F rank byte 0x164D334; +0x20 the byte 0x164D0B0.
    pub rank: u8,
    pub b20: u8,
    pub spells: u8,
    pub clock: i8,
    // Input words from the dummy word on (15 §3.2); fps bytes as stored (one per 30 frames, bit 7 = flag).
    pub inputs: Vec<u16>,
    pub fps: Vec<u8>,
    // Image bytes after the input words to the end of the decompressed image; playback reads on into them (15 §3.3).
    pub tail: Vec<u8>,
}

impl StageRecord {
    fn parse(b: &[u8]) -> Self {
        Self {
            score: u32_at(b, 0),
            point_items: u32_at(b, 4) as i32,
            graze: u32_at(b, 8),
            extends: u32_at(b, 0x0C) as i32,
            next_extend: u32_at(b, 0x10) as i32,
            max_point: u32_at(b, 0x14) as i32,
            gauge: u16_at(b, 0x18) as i16,
            seed: u16_at(b, 0x1A),
            power: b[0x1C],
            lives: b[0x1D],
            bombs: b[0x1E],
            rank: b[0x1F],
            b20: b[0x20],
            spells: b[0x21],
            clock: b[0x22] as i8,
            inputs: b[REC..].chunks_exact(2).map(|c| u16_at(c, 0)).collect(),
            fps: Vec::new(),
            tail: Vec::new(),
        }
    }

    fn header(&self) -> [u8; REC] {
        let mut h = [0; REC];
        put_u32(&mut h, 0, self.score);
        put_u32(&mut h, 4, self.point_items as u32);
        put_u32(&mut h, 8, self.graze);
        put_u32(&mut h, 0x0C, self.extends as u32);
        put_u32(&mut h, 0x10, self.next_extend as u32);
        put_u32(&mut h, 0x14, self.max_point as u32);
        h[0x18..0x1A].copy_from_slice(&self.gauge.to_le_bytes());
        h[0x1A..0x1C].copy_from_slice(&self.seed.to_le_bytes());
        h[0x1C] = self.power;
        h[0x1D] = self.lives;
        h[0x1E] = self.bombs;
        h[0x1F] = self.rank;
        h[0x20] = self.b20;
        h[0x21] = self.spells;
        h[0x22] = self.clock as u8;
        h
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Replay {
    // File header +6, debug-frame flag (15 §3.1); retail files have 0.
    pub debug: u8,
    // Info block 0x68..0x134 of the image (date, name, card, config copy, slowdown, …).
    pub info: Vec<u8>,
    pub stages: [Option<StageRecord>; 9],
    // Header +0x44, the stage-0 fps-stream offset; also read by the replay menu's stage-skip quirk (15 §3.8).
    pub fps0: u32,
    // Raw "USER" section after the file size (15 §3.5), kept as read.
    pub user: Vec<u8>,
}

impl Default for Replay {
    // Info constants of 15 §3.5: +0x69 = 100, +0x120 = 30 (fps interval), exe size/sum and tag "0100d" (15 §2.10).
    fn default() -> Self {
        let mut info = vec![0; INFO - HEADER];
        info[1] = 100;
        put_u32(&mut info, 0x120 - HEADER, 30);
        put_u32(&mut info, 0x124 - HEADER, EXE_SIZE);
        put_u32(&mut info, 0x128 - HEADER, EXE_SUM);
        info[0x12C - HEADER..0x131 - HEADER].copy_from_slice(b"0100d");
        Self {
            debug: 0,
            info,
            stages: Default::default(),
            fps0: 0,
            user: Vec::new(),
        }
    }
}

// 15 §3.5: from header +0x18 on, b += key, key += 7 (u8); decrypt subtracts the same sequence.
fn crypt(b: &mut [u8], mut key: u8, decrypt: bool) {
    for c in b {
        *c = if decrypt {
            c.wrapping_sub(key)
        } else {
            c.wrapping_add(key)
        };
        key = key.wrapping_add(7);
    }
}

impl Replay {
    pub fn shot(&self) -> u8 {
        self.info[0x6A - HEADER]
    }
    pub fn difficulty(&self) -> u8 {
        self.info[0x6B - HEADER]
    }
    pub fn date(&self) -> String {
        cstr(&self.info[0x6C - HEADER..0x72 - HEADER])
    }
    pub fn name(&self) -> String {
        cstr(&self.info[0x72 - HEADER..0x7B - HEADER])
    }
    pub fn practice(&self) -> bool {
        self.info[0x7B - HEADER] != 0
    }
    pub fn spell_id(&self) -> i16 {
        u16_at(&self.info, 0x7C - HEADER) as i16
    }
    pub fn score(&self) -> u32 {
        u32_at(&self.info, 0xB0 - HEADER)
    }
    pub fn config(&self) -> &[u8] {
        &self.info[0xB4 - HEADER..0xF0 - HEADER]
    }
    pub fn slowdown(&self) -> f32 {
        f32::from_le_bytes(
            self.info[0x118 - HEADER..0x11C - HEADER]
                .try_into()
                .unwrap(),
        )
    }
    pub fn first_stage(&self) -> Option<usize> {
        self.stages.iter().position(Option::is_some)
    }

    // Replay_Load 0x451D90 (15 §3.7): magic + version 6, decrypt from 0x18 with key +0x15, checksum, LZSS, then the
    // slow-mode and exe-check rejects (step 5: ExeCheck of info +0x12C tag, +0x124 size, +0x128 sum, 15 §2.10).
    // `ver_list` is the boot-loaded version list, None = not loaded (pass).
    pub fn parse(file: &[u8], ver_list: Option<&[u8]>) -> Result<Self> {
        if file.len() < HEADER || &file[..4] != b"T8RP" || u16_at(file, 4) != 6 {
            return Err(ReplayError::NotReplay);
        }
        let size = (u32_at(file, 0x0C) as usize).min(file.len());
        if size < HEADER {
            return Err(ReplayError::Size);
        }
        let mut b = file[..size].to_vec();
        crypt(&mut b[0x18..], file[0x15], true);
        let sum = b[0x15..]
            .iter()
            .fold(CHECK_BASE, |a, &c| a.wrapping_add(c as u32));
        if sum != u32_at(&b, 0x10) {
            return Err(ReplayError::Checksum);
        }
        let csize = (u32_at(&b, 0x18) as usize).min(b.len() - HEADER);
        let dsize = u32_at(&b, 0x1C) as usize;
        let mut img = b[..HEADER].to_vec();
        img.extend(lzss_decompress(&b[HEADER..HEADER + csize], dsize));
        if img.len() < INFO {
            return Err(ReplayError::TooShort);
        }
        let info = img[HEADER..INFO].to_vec();
        if info[0xD9 - HEADER] != 0 {
            return Err(ReplayError::SlowMode);
        }
        if !exe_check_in(
            ver_list,
            &info[0x12C - HEADER..0x134 - HEADER],
            u32_at(&info, 0x124 - HEADER),
            u32_at(&info, 0x128 - HEADER),
        ) {
            return Err(ReplayError::ExeCheck);
        }
        // Header +0x20[9] record offsets and +0x44[9] fps offsets, in image coordinates (0 = stage not played).
        let offs: Vec<usize> = (0..9)
            .map(|i| u32_at(&img, 0x20 + 4 * i) as usize)
            .collect();
        let fps: Vec<usize> = (0..9)
            .map(|i| u32_at(&img, 0x44 + 4 * i) as usize)
            .collect();
        let mut stages: [Option<StageRecord>; 9] = Default::default();
        for i in 0..9 {
            let o = offs[i];
            if o == 0 {
                continue;
            }
            // A record runs to its fps stream, the fps stream to the next record (15 §3.5 image order).
            let next = offs[i + 1..]
                .iter()
                .copied()
                .find(|&o| o != 0)
                .unwrap_or(img.len())
                .min(img.len());
            let f = if fps[i] > o { fps[i].min(next) } else { next };
            if o + REC > f {
                return Err(ReplayError::StageRecord(i));
            }
            let mut r = StageRecord::parse(&img[o..f]);
            r.fps = img[f..next].to_vec();
            r.tail = img[o + REC + 2 * r.inputs.len()..].to_vec();
            stages[i] = Some(r);
        }
        Ok(Self {
            debug: file[6],
            info,
            stages,
            fps0: fps[0] as u32,
            user: file[size..].to_vec(),
        })
    }

    pub fn clear_status(&self) -> u8 {
        self.info[0x11C - HEADER]
    }
    pub fn card_name(&self) -> SjisStr<'_> {
        let c = &self.info[0x7E - HEADER..0xAE - HEADER];
        SjisStr(&c[..c.iter().position(|&b| b == 0).unwrap_or(c.len())])
    }

    // Replay_Finish image/compress/checksum/encrypt (15 §3.5, §3.6 steps 2–5). 3 RNG draws in this order:
    // key byte, info +0x68, header +0x14 (Rng_Range16(256) each). `user` is appended as is.
    pub fn encode(&mut self, rng: &mut impl ByteSource) -> Vec<u8> {
        let key = rng.next_byte();
        self.info[0] = rng.next_byte();
        let r14 = rng.next_byte();
        let mut img = vec![0u8; HEADER];
        img.extend(&self.info);
        for (i, s) in self.stages.iter().enumerate() {
            let Some(s) = s else { continue };
            let at = img.len() as u32;
            put_u32(&mut img, 0x20 + 4 * i, at);
            let h = s.header();
            img.extend(h);
            for w in &s.inputs {
                img.extend(w.to_le_bytes());
            }
            let at = img.len() as u32;
            put_u32(&mut img, 0x44 + 4 * i, at);
            img.extend(&s.fps);
        }
        let comp = lzss_compress(&img[HEADER..]);
        let mut f = img[..HEADER].to_vec();
        f[..4].copy_from_slice(b"T8RP");
        f[4..6].copy_from_slice(&6u16.to_le_bytes());
        f[6] = self.debug;
        put_u32(&mut f, 0x0C, (HEADER + comp.len()) as u32);
        f[0x14] = r14;
        f[0x15] = key.wrapping_add(0x40);
        put_u32(&mut f, 0x18, comp.len() as u32);
        put_u32(&mut f, 0x1C, (img.len() - HEADER) as u32);
        f.extend(comp);
        // Checksum over [0x15, 0x68) + the compressed body, before encryption.
        let sum = f[0x15..]
            .iter()
            .fold(CHECK_BASE, |a, &c| a.wrapping_add(c as u32));
        put_u32(&mut f, 0x10, sum);
        let k = f[0x15];
        crypt(&mut f[0x18..], k, false);
        f.extend(&self.user);
        f
    }
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoredat::VER_LIST_NAME;

    // 15 §3.5–§3.7: a built replay round-trips; a broken checksum and a slow-mode record are rejected.
    #[test]
    fn encode_round_trip() {
        let mut r = Replay::default();
        r.info[0x6A - HEADER] = 3;
        r.info[0x72 - HEADER..0x7A - HEADER].copy_from_slice(b"TESTER  ");
        r.info[0x7E - HEADER..0x82 - HEADER].copy_from_slice(b"Card");
        r.stages[1] = Some(StageRecord {
            score: 1234,
            seed: 77,
            inputs: vec![0, 1, 2, 3],
            fps: vec![60, 60],
            ..Default::default()
        });
        r.user = b"USER\x0c\0\0\0\0\0\0\0".to_vec();
        let mut rng = crate::testdata::Counter(9, 0);
        let f = r.clone().encode(&mut rng);
        assert_eq!(rng.1, 3);
        let p = Replay::parse(&f, None).unwrap();
        assert_eq!(
            (p.shot(), p.name().as_str(), p.card_name().as_bytes()),
            (3, "TESTER  ", &b"Card"[..])
        );
        assert_eq!(p.first_stage(), Some(1));
        let s = p.stages[1].as_ref().unwrap();
        assert_eq!(
            (s.score, s.seed, &s.inputs[..], &s.fps[..]),
            (1234, 77, &[0, 1, 2, 3][..], &[60, 60][..])
        );
        assert_eq!(p.user, r.user);
        let mut bad = f.clone();
        bad[0x70] ^= 1;
        assert!(matches!(
            Replay::parse(&bad, None),
            Err(ReplayError::Checksum)
        ));
        r.info[0xD9 - HEADER] = 1;
        let f = r.encode(&mut rng);
        assert!(matches!(
            Replay::parse(&f, None),
            Err(ReplayError::SlowMode)
        ));
    }

    #[test]
    fn demos() {
        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        // Demo 0 has the listed 1.00a tag/size/sum, demos 1–3 the tag "debug": all pass ExeCheck (15 §2.10).
        let ver = ar.read(VER_LIST_NAME).unwrap();
        let frames = [11083, 0, 0, 0];
        for n in 0..4 {
            let f = ar.read(&format!("demorpy{n}.rpy")).unwrap();
            let r = Replay::parse(&f, Some(&ver)).unwrap();
            let st = r.first_stage().unwrap();
            let s = r.stages[st].as_ref().unwrap();
            assert_eq!(s.inputs[0], 0, "dummy word");
            assert!(s.inputs.iter().all(|w| w & !0x1FFF == 0 || *w == 0xCDCD));
            if frames[n] != 0 {
                assert!(s.inputs.len() >= frames[n]);
            }
            let mut c = r.clone();
            let enc = c.encode(&mut crate::testdata::Counter(3, 0));
            assert_eq!(Replay::parse(&enc, Some(&ver)).unwrap().stages, r.stages);
        }
    }
}
