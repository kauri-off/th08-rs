//! score.dat (15 §2): container, chunks, ExeCheck and the in-memory records the game updates.

use crate::ByteSource;
use crate::archive::{block_decrypt, block_encrypt, lzss_compress, lzss_decompress};
use crate::reader::{Reader, Truncated, put_u32, u16_at, u32_at};

#[derive(Debug, thiserror::Error)]
pub enum ScoreError {
    #[error("score.dat too small")]
    TooSmall,
    #[error("score.dat checksum")]
    Checksum,
    #[error("score.dat header")]
    Header,
    #[error("score.dat exesumcheck error")]
    ExeSum,
    #[error("score.dat: empty chunk")]
    EmptyChunk,
    #[error("score.dat: no TH8K chunk")]
    NoTh8k,
    #[error("score.dat: {0}")]
    Truncated(#[from] Truncated),
}

pub type Result<T> = std::result::Result<T, ScoreError>;

pub const CARDS: usize = 222;
// Outer layer: archive block cipher (key, step, block, limit) (01 §2).
const CRYPT: (u8, u8, usize, usize) = (0x59, 0x79, 0x100, 0xC00);
// CATK counter cap and PLST counter cap.
const CAP: u32 = 9999;
const PLST_CAP: u32 = 999_999;
// VRSM values of the shipped th08.exe.
pub const EXE_SIZE: u32 = 840_704;
pub const EXE_SUM: u32 = 0xA268_61B9;
// Version list file, read from th08.dat only at boot (`th08_%.4x%c.ver` with 0x100, 'd').
pub const VER_LIST_NAME: &str = "th08_0100d.ver";

// C `strncmp(a, b, n) == 0` on byte strings (missing bytes read as NUL).
fn strneq(a: &[u8], b: &[u8], n: usize) -> bool {
    for i in 0..n {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return false;
        }
        if x == 0 {
            return true;
        }
    }
    true
}

// CRT `%d`: leading white space, optional sign, digits accumulated with 32-bit wrap ("2724749753" → 0xA26861B9).
fn scan_d(b: &[u8]) -> Option<(u32, &[u8])> {
    let mut i = b.iter().take_while(|c| c.is_ascii_whitespace()).count();
    let neg = match b.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let n = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    if n == 0 {
        return None;
    }
    let v = b[i..i + n].iter().fold(0u32, |a, &c| {
        a.wrapping_mul(10).wrapping_add(u32::from(c - b'0'))
    });
    Some((if neg { v.wrapping_neg() } else { v }, &b[i + n..]))
}

// ExeCheck steps 1–4 over the `tag size sum\n` lines; `tag` is the 8-byte field (NUL-terminated).
// QUIRK: a tag-matching line with other numbers makes the original walk past the buffer end unless a later line
// matches; that over-run is treated as a fail, as the spec allows.
pub fn exe_check_in(list: Option<&[u8]>, tag: &[u8], size: u32, sum: u32) -> bool {
    let Some(list) = list else { return true };
    if strneq(tag, b"debug", 5) {
        return true;
    }
    let mut line = list;
    while !line.is_empty() {
        if line.len() >= 5 && strneq(&line[..5], tag, 5) {
            let nums = line.get(6..).unwrap_or(&[]);
            if let Some((a, rest)) = scan_d(nums)
                && let Some((b, _)) = scan_d(rest)
                && a == size
                && b == sum
            {
                return true;
            }
        }
        line = match line.iter().position(|&c| c == b'\n') {
            Some(p) => &line[p + 1..],
            None => &[],
        };
    }
    false
}

// C `strcpy` into a fixed field: the bytes up to the source's NUL (at most len − 1) plus a NUL; the rest is kept.
fn strcpy(dst: &mut [u8], src: &[u8]) {
    let n = src
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(src.len())
        .min(dst.len() - 1);
    dst[..n].copy_from_slice(&src[..n]);
    dst[n] = 0;
}

fn put_u32s(b: &mut [u8], o: usize, v: &[u32]) {
    for (i, x) in v.iter().enumerate() {
        put_u32(b, o + 4 * i, *x);
    }
}

// Chunk header: magic, u16 size twice, u8 version, 3 bytes padding.
fn chunk(magic: &[u8; 4], size: usize, version: u8) -> Vec<u8> {
    let mut c = vec![0; size];
    c[..4].copy_from_slice(magic);
    c[4..6].copy_from_slice(&(size as u16).to_le_bytes());
    c[6..8].copy_from_slice(&(size as u16).to_le_bytes());
    c[8] = version;
    c
}

// HSCR chunk v4, 0x168 bytes, kept raw: +0x0C score ÷10, +0x10 slowdown f32, +0x14 shot, +0x15
// difficulty, +0x16 stage (99 = cleared), +0x17 name, +0x20 date, +0x26 continues, +0x166 default flag.
#[derive(Clone)]
pub struct Hscr(pub Vec<u8>);

impl Hscr {
    pub fn new() -> Self {
        Self(chunk(b"HSCR", 0x168, 4))
    }
    pub fn score(&self) -> u32 {
        u32_at(&self.0, 0x0C)
    }
    pub fn shot(&self) -> usize {
        self.0[0x14] as usize
    }
    pub fn difficulty(&self) -> usize {
        self.0[0x15] as usize
    }
    // Default table entry (Result_OnRegister 0x45964D): magic "DMYS", name "--------", date "--/--",
    // stage byte 0 (shown as stage 1, 16 §2.3), +0x166 = 1. The loop never writes the shot byte +0x14, so it stays 0.
    pub fn placeholder(score: u32, diff: usize) -> Self {
        let mut h = Self(chunk(b"DMYS", 0x168, 4));
        h.set(score, 0, diff as u8, 0, b"--------", "--/--");
        h.0[0x166] = 1;
        h
    }
    // ScoreDat_Save skips every node whose magic is not "HSCR".
    pub fn is_placeholder(&self) -> bool {
        &self.0[..4] != b"HSCR"
    }
    pub fn stage(&self) -> u8 {
        self.0[0x16]
    }
    pub fn continues(&self) -> u8 {
        self.0[0x26]
    }
    pub fn name(&self) -> String {
        let n = &self.0[0x17..0x20];
        let end = n.iter().position(|&c| c == 0).unwrap_or(n.len());
        n[..end].iter().map(|&c| c as char).collect()
    }
    pub fn date(&self) -> String {
        let n = &self.0[0x20..0x26];
        let end = n.iter().position(|&c| c == 0).unwrap_or(n.len());
        n[..end].iter().map(|&c| c as char).collect()
    }
    pub fn slowdown(&self) -> f32 {
        f32::from_bits(u32_at(&self.0, 0x10))
    }
    pub fn put_u32(&mut self, o: usize, v: u32) {
        put_u32(&mut self.0, o, v);
    }
    pub fn set(&mut self, score: u32, shot: u8, diff: u8, stage: u8, name: &[u8], date: &str) {
        put_u32(&mut self.0, 0x0C, score);
        self.0[0x14] = shot;
        self.0[0x15] = diff;
        self.0[0x16] = stage;
        let n = &mut self.0[0x17..0x20];
        n.fill(0);
        n[..name.len().min(8)].copy_from_slice(&name[..name.len().min(8)]);
        let d = &mut self.0[0x20..0x26];
        d.fill(0);
        d[..date.len().min(5)].copy_from_slice(&date.as_bytes()[..date.len().min(5)]);
    }
}

impl Default for Hscr {
    fn default() -> Self {
        Self::new()
    }
}

// CLRD chunk v4: +0x0C no-continue masks, +0x16 any-clear masks (E, N, H, L, Ex; bit 1 << stage, 0x4000
// Final B, 0x8000 Final A), +0x20 "bad ending seen" flag. Entry 12 is the union over all shots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clrd {
    pub nc: [u16; 5],
    pub any: [u16; 5],
    pub bad_ending: u8,
}

impl Default for Clrd {
    fn default() -> Self {
        Self {
            // Every mask defaults to 1 (the stage-1 bit).
            nc: [1; 5],
            any: [1; 5],
            bad_ending: 0,
        }
    }
}

// CATK chunk v3, 0x22C bytes; in memory at GM+0x40.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catk {
    // `CATK` magic set (loaded or stamped at game start); the chunk is saved only when set.
    pub present: bool,
    pub difficulty: u8,
    pub checksum: u8,
    pub name: [u8; 48],
    pub owner: [u8; 48],
    // +0x10 name, +0x40 owner, +0x70 / +0xB0 comment lines; stored decoded (XOR 0xAA / 0xBB / 0xDD / 0xEE come from ECL).
    pub comment: [[u8; 64]; 2],
    // +0xF0.. six u32[13] rows: [story bonus, story attempts, story captures, practice bonus, practice attempts, practice captures][shot or 12]
    pub counters: [[u32; 13]; 6],
}

pub const C_BONUS: usize = 0;
pub const C_ATTEMPTS: usize = 1;
pub const C_CAPTURES: usize = 2;
pub const C_PRACTICE: usize = 3;

impl Default for Catk {
    fn default() -> Self {
        Self {
            present: false,
            difficulty: 0,
            checksum: 0,
            name: [0; 48],
            owner: [0; 48],
            comment: [[0; 64]; 2],
            counters: [[0; 13]; 6],
        }
    }
}

impl Catk {
    // +0x0E = (Σ signed name bytes up to the NUL + Σ all 78 u32 counters) & 0xFF.
    pub fn compute_checksum(&self) -> u8 {
        let n: i32 = self
            .name
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| i32::from(c.cast_signed()))
            .sum();
        let c: u32 = self
            .counters
            .iter()
            .flatten()
            .fold(0, |a, &v| a.wrapping_add(v));
        n.cast_unsigned().wrapping_add(c) as u8
    }

    // "Attempted" for the FLSP unlocks: any attempt counter ≠ 0, story or practice.
    pub fn attempted(&self) -> bool {
        self.counters[C_ATTEMPTS][12] != 0 || self.counters[C_PRACTICE + C_ATTEMPTS][12] != 0
    }
}

// PSCR chunk v2, 0x178 bytes per shot: +0x0C plays [stage][diff], +0xC0 hiscore ÷10, +0x175 used.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pscr {
    pub plays: [[u32; 5]; 9],
    pub hiscore: [[u32; 5]; 9],
    pub used: u8,
}

// PLST chunk v2, 0x228 bytes: run / play time h, m, s, ms; counters [block][17] with blocks 0..4 =
// difficulty, 5 unused, 6 total; +0x208 music-room heard flags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plst {
    pub run_time: [u32; 4],
    pub play_time: [u32; 4],
    pub counters: [[u32; 17]; 7],
    pub music: [u8; 32],
}

// Counter indices inside a 17-word block; [1 + shot] are per-shot plays.
pub const PL_PLAYS: usize = 0;
pub const PL_RETRIES: usize = 13;
pub const PL_CLEARS: usize = 14;
pub const PL_CONTINUES: usize = 15;
pub const PL_PRACTICE: usize = 16;

impl Plst {
    pub fn total_plays(&self) -> u32 {
        self.counters[6][PL_PLAYS]
    }

    // Counter `idx` of difficulty block `d` and of the total block, capped at 999 999.
    pub fn bump(&mut self, d: usize, idx: usize) {
        for b in [d.min(4), 6] {
            let c = &mut self.counters[b][idx];
            *c = (*c + 1).min(PLST_CAP);
        }
    }

    // Adds `ms` to an h/m/s/ms clock with carries (Supervisor_UpdatePlayTime 0x448418).
    pub fn add_time(t: &mut [u32; 4], ms: u64) {
        let total = ((u64::from(t[0]) * 60 + u64::from(t[1])) * 60 + u64::from(t[2])) * 1000
            + u64::from(t[3])
            + ms;
        *t = [
            (total / 3_600_000) as u32,
            (total / 60_000 % 60) as u32,
            (total / 1000 % 60) as u32,
            (total % 1000) as u32,
        ];
    }
}

// All chunks of one score.dat; `lsnm` +0x0C char[12], `flsp` +0x0C u8[17], `vrsm` +0x0C..+0x1C tag, exe size,
// exe sum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoreDat {
    pub hscr: Vec<Hscr>,
    pub clrd: [Clrd; 13],
    pub catk: Vec<Catk>,
    pub pscr: [Pscr; 12],
    pub lsnm: [u8; 12],
    pub flsp: [u8; 17],
    pub plst: Plst,
    pub vrsm: [u8; 0x10],
}

impl PartialEq for Hscr {
    fn eq(&self, o: &Self) -> bool {
        self.0 == o.0
    }
}
impl Eq for Hscr {}
impl std::fmt::Debug for Hscr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Hscr({} s{} d{})",
            self.score(),
            self.shot(),
            self.difficulty()
        )
    }
}

impl Default for ScoreDat {
    fn default() -> Self {
        let mut vrsm = [0; 0x10];
        vrsm[..5].copy_from_slice(b"0100d");
        vrsm[8..12].copy_from_slice(&EXE_SIZE.to_le_bytes());
        vrsm[12..16].copy_from_slice(&EXE_SUM.to_le_bytes());
        Self {
            hscr: Vec::new(),
            clrd: [Clrd::default(); 13],
            catk: vec![Catk::default(); CARDS],
            pscr: Default::default(),
            // LSNM default: 8 spaces.
            lsnm: *b"        \0\0\0\0",
            flsp: [0; 17],
            plst: Plst::default(),
            vrsm,
        }
    }
}

// ScoreDat_Load 0x45A5E0 container: removes both layers (archive cipher, then the rolling XOR from byte 1),
// checks the u16 checksum of bytes 4.., header size 0x1C and version 1, then LZSS-decompresses the body.
pub fn decode_container(file: &[u8]) -> Result<Vec<u8>> {
    if file.len() < 0x1C {
        return Err(ScoreError::TooSmall);
    }
    let mut b = block_decrypt(file, CRYPT.0, CRYPT.1, CRYPT.2, CRYPT.3);
    let mut acc = 0u8;
    for i in 1..b.len() - 1 {
        acc = acc.wrapping_add(b[i]).rotate_left(3);
        b[i + 1] ^= acc;
    }
    let sum = b[4..]
        .iter()
        .fold(0u16, |a, &c| a.wrapping_add(u16::from(c)));
    if sum != u16_at(&b, 2) {
        return Err(ScoreError::Checksum);
    }
    if u32_at(&b, 8) != 0x1C || u16_at(&b, 4) != 1 {
        return Err(ScoreError::Header);
    }
    let size = u32_at(&b, 0x14) as usize;
    let mut out = b[..0x1C].to_vec();
    out.extend(lzss_decompress(&b[0x1C..], size));
    Ok(out)
}

// ScoreDat_Save 0x453D0D container: `r` are the random header bytes +0x01 and +0x06; the rolling XOR runs on the
// plain bytes so that the decoder above recovers them.
pub fn encode_container(body: &[u8], r: [u8; 2]) -> Vec<u8> {
    let comp = lzss_compress(body);
    let mut b = vec![0u8; 0x1C];
    b[1] = r[0];
    b[4..6].copy_from_slice(&1u16.to_le_bytes());
    b[6] = r[1];
    put_u32(&mut b, 8, 0x1C);
    put_u32(&mut b, 0x10, (body.len() + 0x1C) as u32);
    put_u32(&mut b, 0x14, body.len() as u32);
    put_u32(&mut b, 0x18, comp.len() as u32);
    b.extend(comp);
    let sum = b[4..]
        .iter()
        .fold(0u16, |a, &c| a.wrapping_add(u16::from(c)));
    b[2..4].copy_from_slice(&sum.to_le_bytes());
    let plain = b.clone();
    let mut acc = 0u8;
    for i in 1..b.len() - 1 {
        acc = acc.wrapping_add(plain[i]).rotate_left(3);
        b[i + 1] ^= acc;
    }
    block_encrypt(&b, CRYPT.0, CRYPT.1, CRYPT.2, CRYPT.3)
}

impl ScoreDat {
    // Chunk walk of ScoreDat_Get*: only chunks with matching magic and version are taken; a size-0 chunk or no
    // TH8K v1 rejects the file. `ver_list` is the boot-loaded version list, None = not loaded (pass).
    pub fn parse(file: &[u8], ver_list: Option<&[u8]>) -> Result<Self> {
        let d = decode_container(file)?;
        let mut s = Self::default();
        let mut th8k = false;
        let mut p = 0x1C;
        while p + 12 <= d.len() {
            let size = u16_at(&d, p + 4) as usize;
            // Every VRSM v1 chunk goes through ExeCheck (tag +0x0C, size +0x14, sum +0x18) before the size-0 test.
            if &d[p..p + 4] == b"VRSM" && d[p + 8] == 1 {
                let ok = d
                    .get(p + 0x0C..p + 0x1C)
                    .is_some_and(|v| exe_check_in(ver_list, &v[..8], u32_at(v, 8), u32_at(v, 12)));
                if !ok {
                    return Err(ScoreError::ExeSum);
                }
            }
            if size == 0 {
                return Err(ScoreError::EmptyChunk);
            }
            let Some(c) = d.get(p..p + size) else { break };
            let ver = c[8];
            match (&c[..4], ver) {
                (b"TH8K", 1) => th8k = true,
                (b"HSCR", 4) if size >= 0x168 => s.hscr.push(Hscr(c[..0x168].to_vec())),
                (b"CLRD", 4) if size >= 0x24 => {
                    let mut r = Reader::at(c, 0x0C);
                    let (nc, any, bad_ending) = (r.u16s()?, r.u16s()?, r.u8()?);
                    if let Some(e) = s.clrd.get_mut(usize::from(r.u8()?)) {
                        *e = Clrd {
                            nc,
                            any,
                            bad_ending,
                        };
                    }
                }
                (b"CATK", 3) if size >= 0x22C => {
                    let mut r = Reader::at(c, 0x0C);
                    let id = usize::from(r.u16()?);
                    if let Some(e) = s.catk.get_mut(id) {
                        *e = Catk {
                            present: true,
                            checksum: r.u8()?,
                            difficulty: r.u8()?,
                            name: r.array()?,
                            owner: r.array()?,
                            comment: [r.array()?, r.array()?],
                            counters: [
                                r.u32s()?,
                                r.u32s()?,
                                r.u32s()?,
                                r.u32s()?,
                                r.u32s()?,
                                r.u32s()?,
                            ],
                        };
                    }
                }
                (b"PSCR", 2) if size >= 0x178 => {
                    let shot = usize::from(c[0x174]);
                    if let Some(e) = s.pscr.get_mut(shot) {
                        let mut r = Reader::at(c, 0x0C);
                        for row in &mut e.plays {
                            *row = r.u32s()?;
                        }
                        for row in &mut e.hiscore {
                            *row = r.u32s()?;
                        }
                        e.used = c[0x175];
                    }
                }
                (b"LSNM", 1) if size >= 0x18 => s.lsnm.copy_from_slice(&c[0x0C..0x18]),
                (b"FLSP", 1) if size >= 0x20 => s.flsp.copy_from_slice(&c[0x0C..0x1D]),
                (b"PLST", 2) if size >= 0x228 => {
                    let mut r = Reader::at(c, 0x0C);
                    s.plst.run_time = r.u32s()?;
                    s.plst.play_time = r.u32s()?;
                    for row in &mut s.plst.counters {
                        *row = r.u32s()?;
                    }
                    s.plst.music = r.array()?;
                }
                (b"VRSM", 1) if size >= 0x1C => s.vrsm.copy_from_slice(&c[0x0C..0x1C]),
                _ => {}
            }
            p += size;
        }
        if !th8k {
            return Err(ScoreError::NoTh8k);
        }
        Ok(s)
    }

    // Chunk list in the saver's order.
    pub fn body(&self) -> Vec<u8> {
        let mut out = chunk(b"TH8K", 0x0C, 1);
        for h in &self.hscr {
            out.extend(&h.0);
        }
        for (i, e) in self.clrd.iter().enumerate() {
            let mut c = chunk(b"CLRD", 0x24, 4);
            for k in 0..5 {
                c[0x0C + 2 * k..0x0E + 2 * k].copy_from_slice(&e.nc[k].to_le_bytes());
                c[0x16 + 2 * k..0x18 + 2 * k].copy_from_slice(&e.any[k].to_le_bytes());
            }
            c[0x20] = e.bad_ending;
            c[0x21] = i as u8;
            out.extend(c);
        }
        for (id, e) in self.catk.iter().enumerate().filter(|(_, e)| e.present) {
            let mut c = chunk(b"CATK", 0x22C, 3);
            c[0x0C..0x0E].copy_from_slice(&(id as u16).to_le_bytes());
            c[0x0E] = e.checksum;
            c[0x0F] = e.difficulty;
            c[0x10..0x40].copy_from_slice(&e.name);
            c[0x40..0x70].copy_from_slice(&e.owner);
            c[0x70..0xB0].copy_from_slice(&e.comment[0]);
            c[0xB0..0xF0].copy_from_slice(&e.comment[1]);
            for (k, row) in e.counters.iter().enumerate() {
                put_u32s(&mut c, 0xF0 + 0x34 * k, row);
            }
            out.extend(c);
        }
        for (shot, e) in self.pscr.iter().enumerate().filter(|(_, e)| e.used != 0) {
            let mut c = chunk(b"PSCR", 0x178, 2);
            for st in 0..9 {
                put_u32s(&mut c, 0x0C + 20 * st, &e.plays[st]);
                put_u32s(&mut c, 0xC0 + 20 * st, &e.hiscore[st]);
            }
            c[0x174] = shot as u8;
            c[0x175] = e.used;
            out.extend(c);
        }
        let mut c = chunk(b"LSNM", 0x18, 1);
        c[0x0C..0x18].copy_from_slice(&self.lsnm);
        out.extend(c);
        let mut c = chunk(b"FLSP", 0x20, 1);
        c[0x0C..0x1D].copy_from_slice(&self.flsp);
        out.extend(c);
        let mut c = chunk(b"PLST", 0x228, 2);
        put_u32s(&mut c, 0x0C, &self.plst.run_time);
        put_u32s(&mut c, 0x1C, &self.plst.play_time);
        for b in 0..7 {
            put_u32s(&mut c, 0x2C + 0x44 * b, &self.plst.counters[b]);
        }
        c[0x208..0x228].copy_from_slice(&self.plst.music);
        out.extend(c);
        let mut c = chunk(b"VRSM", 0x1C, 1);
        c[0x0C..0x1C].copy_from_slice(&self.vrsm);
        out.extend(c);
        out
    }

    // Saving draws two Rng_Range16(256) values; the RNG state carries into the next game (11 §1.5).
    pub fn encode(&self, rng: &mut impl ByteSource) -> Vec<u8> {
        let r = [rng.next_byte(), rng.next_byte()];
        encode_container(&self.body(), r)
    }

    // ScoreDat_GetHiscore 0x45A950: max(top entry of (shot, difficulty), 100000), ÷10 units.
    pub fn hiscore(&self, shot: usize, diff: usize) -> u32 {
        self.hscr
            .iter()
            .filter(|h| h.shot() == shot && h.difficulty() == diff)
            .map(Hscr::score)
            .max()
            .unwrap_or(0)
            .max(100_000)
    }

    // ScoreDat_GetHiscore 4th argument: continues byte +0x26 of the top entry of (shot, difficulty), 0 when there is
    // none; of equal scores the later file entry is on top.
    pub fn hiscore_continues(&self, shot: usize, diff: usize) -> u8 {
        self.hscr
            .iter()
            .filter(|h| h.shot() == shot && h.difficulty() == diff)
            .fold(None, |top: Option<&Hscr>, h| match top {
                Some(t) if t.score() > h.score() => Some(t),
                _ => Some(h),
            })
            .map_or(0, Hscr::continues)
    }

    // One ranking list [diff][shot] as it stands in memory: `hscr` holds the list's stored entries in list
    // order, each one above the default rows of equal score.
    pub fn ranking(&self, shot: usize, diff: usize) -> Vec<Hscr> {
        let mut list: Vec<Hscr> = (0..10u32)
            .map(|i| Hscr::placeholder(100_000 - 10_000 * i, diff))
            .collect();
        for e in self
            .hscr
            .iter()
            .filter(|e| e.shot() == shot && e.difficulty() == diff)
        {
            let at = list
                .iter()
                .take_while(|h| h.score() > e.score() || !h.is_placeholder())
                .count();
            list.insert(at, e.clone());
        }
        list.truncate(10);
        list
    }

    // Result_OnRegister 0x45964D list build: the default table, then the file's HSCR entries in file order,
    // each with ScoreList_Insert (before every equal score); top 10 kept. Equal scores of one list end up in reverse
    // file order, so each load/save cycle flips them. Port: `hscr` stands for the file (it is what the last save wrote).
    pub fn rebuild_lists(&mut self) {
        let file = std::mem::take(&mut self.hscr);
        let mut lists: Vec<((usize, usize), Vec<Hscr>)> = Vec::new();
        for e in file {
            let key = (e.difficulty(), e.shot());
            let i = if let Some(i) = lists.iter().position(|(k, _)| *k == key) {
                i
            } else {
                let d = (0..10u32)
                    .map(|i| Hscr::placeholder(100_000 - 10_000 * i, key.0))
                    .collect();
                lists.push((key, d));
                lists.len() - 1
            };
            let list = &mut lists[i].1;
            let at = list.iter().filter(|h| h.score() > e.score()).count();
            list.insert(at, e);
            list.truncate(10);
        }
        lists.sort_by_key(|(k, _)| *k);
        self.hscr = lists
            .into_iter()
            .flat_map(|(_, l)| l.into_iter().filter(|e| !e.is_placeholder()))
            .collect();
    }

    // ScoreList_Insert 0x45A500: the new entry goes before every equal score (the walk passes a node only while
    // the new score is strictly less); only the top 10 survive and default entries are never stored. Returns the rank.
    pub fn insert_hscr(&mut self, h: Hscr) -> Option<usize> {
        let (shot, diff) = (h.shot(), h.difficulty());
        let mut list = self.ranking(shot, diff);
        let rank = list.iter().filter(|e| e.score() > h.score()).count();
        list.insert(rank, h);
        list.truncate(10);
        self.hscr
            .retain(|e| e.shot() != shot || e.difficulty() != diff);
        self.hscr
            .extend(list.into_iter().filter(|e| !e.is_placeholder()));
        // The saver walks the lists by difficulty, then shot.
        self.hscr.sort_by_key(|e| (e.difficulty(), e.shot()));
        (rank < 10).then_some(rank)
    }

    // The entry being named (Result list node byte +9 ≠ 0, 16 §2.3).
    pub fn new_entry_mut(&mut self) -> Option<&mut Hscr> {
        self.hscr.iter_mut().find(|h| h.0[9] != 0)
    }

    // ScoreDat_Save sets the in-memory entry's +9 := 0 before copying it (16 §2.3).
    pub fn clear_new_flags(&mut self) {
        for h in &mut self.hscr {
            h.0[9] = 0;
        }
    }

    // GameMgr_LoadScoreData 0x43BBE1: the CATK block is zeroed and all 222 records stamped (magic, size,
    // version 3, id) before the file is loaded. With the blocks kept in memory, a record that is in the file
    // (`present`) already equals its file copy; the others become zero records with the magic.
    pub fn stamp_catk(&mut self) {
        for c in self.catk.iter_mut().filter(|c| !c.present) {
            *c = Catk {
                present: true,
                ..Default::default()
            };
        }
    }

    // Spell_Start 0x4152A0 attempt (steps 1–6): name and owner are written first (`strcpy`, bytes after the
    // NUL keep their old value), then the checksum of the new name is tested with no magic test; a mismatch zeroes all
    // counters. Attempts of this shot and of all shots (index 12) are capped at 9999.
    pub fn card_attempt(
        &mut self,
        id: usize,
        shot: usize,
        diff: u8,
        practice: bool,
        name: &[u8],
        owner: &[u8],
    ) {
        let Some(c) = self.catk.get_mut(id) else {
            return;
        };
        strcpy(&mut c.name, name);
        strcpy(&mut c.owner, owner);
        if c.compute_checksum() != c.checksum {
            c.counters = [[0; 13]; 6];
        }
        c.present = true;
        let row = &mut c.counters[if practice { C_PRACTICE } else { 0 } + C_ATTEMPTS];
        for k in [shot % 12, 12] {
            if row[k] < CAP {
                row[k] += 1;
            }
        }
        c.difficulty = diff;
        c.checksum = c.compute_checksum();
    }

    // Spell_End 0x4161B0 capture: captures and max bonus; comments only in spell practice.
    pub fn card_capture(
        &mut self,
        id: usize,
        shot: usize,
        practice: bool,
        bonus: u32,
        comment: &[Vec<u8>; 2],
    ) {
        let Some(c) = self.catk.get_mut(id) else {
            return;
        };
        let base = if practice { C_PRACTICE } else { 0 };
        for k in [shot % 12, 12] {
            let cap = &mut c.counters[base + C_CAPTURES][k];
            *cap = (*cap + 1).min(CAP);
            let b = &mut c.counters[base + C_BONUS][k];
            *b = (*b).max(bonus);
        }
        if practice {
            for (dst, src) in c.comment.iter_mut().zip(comment) {
                *dst = [0; 64];
                dst[..src.len().min(63)].copy_from_slice(&src[..src.len().min(63)]);
            }
        }
        c.checksum = c.compute_checksum();
    }

    // (captures, attempts) of a card for a shot type, story or spell practice (bonus line, 12 §5.8).
    pub fn card_history(&self, id: usize, shot: usize, practice: bool) -> (u32, u32) {
        let base = if practice { C_PRACTICE } else { 0 };
        self.catk.get(id).map_or((0, 0), |c| {
            (
                c.counters[base + C_CAPTURES][shot % 12],
                c.counters[base + C_ATTEMPTS][shot % 12],
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The continues digit comes from the top entry; equal scores → the later file entry.
    #[test]
    fn hiscore_continues_top_entry() {
        let mut s = ScoreDat::default();
        assert_eq!(s.hiscore_continues(0, 1), 0);
        for (score, cont) in [(300, 1), (500, 2), (500, 3), (400, 4)] {
            let mut h = Hscr::new();
            h.set(score, 0, 1, 1, b"A", "10/24");
            h.0[0x26] = cont;
            s.hscr.push(h);
        }
        assert_eq!(s.hiscore_continues(0, 1), 3);
        assert_eq!(s.hiscore_continues(1, 1), 0);
    }

    #[test]
    fn round_trip() {
        let mut s = ScoreDat::default();
        s.clrd[3].nc[2] |= 0x4000;
        s.card_attempt(54, 0, 0, false, b"name", b"owner");
        s.card_capture(54, 0, false, 1234, &[b"a".to_vec(), b"b".to_vec()]);
        s.pscr[2].used = 1;
        s.pscr[2].hiscore[1][3] = 777;
        s.plst.bump(1, PL_PLAYS);
        let mut h = Hscr::new();
        h.set(5_000_000, 0, 1, 99, b"TEST", "10/18");
        assert_eq!(s.insert_hscr(h), Some(0));
        let mut rng = crate::testdata::Counter(7, 0);
        let f = s.encode(&mut rng);
        assert_eq!(rng.1, 2);
        let t = ScoreDat::parse(&f, None).unwrap();
        assert_eq!(t, s);
        assert_eq!(t.hiscore(0, 1), 5_000_000);
        assert_eq!(t.card_history(54, 0, false), (1, 1));
    }

    // A new entry goes before equal scores, including a default entry of the same score.
    #[test]
    fn insert_before_equal() {
        let mut s = ScoreDat::default();
        let mut a = Hscr::new();
        a.set(3_000_000, 0, 1, 2, b"FIRST", "10/18");
        let mut b = Hscr::new();
        b.set(3_000_000, 0, 1, 3, b"SECOND", "10/19");
        assert_eq!(s.insert_hscr(a.clone()), Some(0));
        assert_eq!(s.insert_hscr(b.clone()), Some(0));
        let r = s.ranking(0, 1);
        assert_eq!((&r[0], &r[1]), (&b, &a));
        let mut c = Hscr::new();
        c.set(90_000, 0, 1, 1, b"TIE", "10/20");
        // Default rows: 100000, 90000, …; with FIRST/SECOND on top the 90000 default is rank 3.
        assert_eq!(s.insert_hscr(c), Some(3));
    }

    // Each Result_OnRegister rebuild re-inserts the stored entries, flipping equal scores.
    #[test]
    fn rebuild_flips_ties() {
        let mut s = ScoreDat::default();
        let mut a = Hscr::new();
        a.set(3_000_000, 0, 1, 2, b"A", "10/18");
        let mut b = Hscr::new();
        b.set(3_000_000, 0, 1, 3, b"B", "10/19");
        let mut c = Hscr::new();
        c.set(5_000_000, 0, 1, 3, b"C", "10/19");
        let mut d = Hscr::new();
        d.set(100_000, 2, 0, 1, b"D", "10/20");
        s.hscr = vec![d.clone(), a.clone(), c.clone(), b.clone()];
        s.rebuild_lists();
        assert_eq!(s.hscr, [d.clone(), c.clone(), b.clone(), a.clone()]);
        assert_eq!(s.ranking(0, 1)[..3], [c.clone(), b.clone(), a.clone()]);
        s.rebuild_lists();
        assert_eq!(s.hscr, [d.clone(), c.clone(), a.clone(), b.clone()]);
        // The 100000 entry sits above the equal default row.
        let r = s.ranking(2, 0);
        assert_eq!((&r[0], r[1].is_placeholder()), (&d, true));
    }

    // Default rows are "DMYS" entries with stage byte 0 and are never saved.
    #[test]
    fn default_rows() {
        let mut s = ScoreDat::default();
        let r = s.ranking(3, 2);
        assert_eq!(r.len(), 10);
        for (i, e) in r.iter().enumerate() {
            assert_eq!(&e.0[..4], b"DMYS");
            assert_eq!((e.stage(), e.shot(), e.difficulty()), (0, 0, 2));
            assert_eq!(e.score(), 100_000 - 10_000 * i as u32);
            assert!(e.is_placeholder());
        }
        let mut h = Hscr::new();
        h.set(95_000, 3, 2, 4, b"X", "10/20");
        assert_eq!(s.insert_hscr(h), Some(1));
        assert_eq!(s.hscr.len(), 1);
    }

    // ExeCheck over the shipped list; `%d` wraps, the duplicate "0100c" tag passes on its second line.
    #[test]
    fn exe_check_list() {
        let list = b"0100c 840704 1605176696\n0100c 840704 1588396223\n0100d 840704 2724749753\n";
        let chk = |tag: &[u8], size, sum| exe_check_in(Some(list), tag, size, sum);
        assert!(chk(b"0100d\0\xCC\xCC", EXE_SIZE, EXE_SUM));
        assert!(chk(b"0100c\0\0\0", 840_704, 1_588_396_223));
        assert!(!chk(b"0100d\0\0\0", EXE_SIZE, EXE_SUM ^ 1));
        assert!(!chk(b"0100e\0\0\0", EXE_SIZE, EXE_SUM));
        assert!(!chk(b"0100\0\0\0\0", EXE_SIZE, EXE_SUM));
        assert!(chk(b"debug\0\0\0", 0, 0));
        assert!(exe_check_in(None, b"xxxxx\0\0\0", 0, 0));
        assert_eq!(
            scan_d(b" -5 x"),
            Some(((-5i32).cast_unsigned(), &b" x"[..]))
        );

        let Some(ar) = crate::testdata::archive() else {
            return;
        };
        let ver = ar.read(VER_LIST_NAME).unwrap();
        assert_eq!(ver.iter().filter(|&&c| c == b'\n').count(), 15);
        assert!(exe_check_in(Some(&ver), b"0100d\0\0\0", EXE_SIZE, EXE_SUM));
        assert!(exe_check_in(
            Some(&ver),
            b"0100a\0\0\0",
            841_216,
            3_000_360_128
        ));
        // A VRSM v1 chunk failing the check rejects the whole file.
        let mut s = ScoreDat::default();
        let mut rng = crate::testdata::Counter(1, 0);
        assert!(ScoreDat::parse(&s.encode(&mut rng), Some(&ver)).is_ok());
        s.vrsm[12] ^= 1;
        assert!(ScoreDat::parse(&s.encode(&mut rng), Some(&ver)).is_err());
        assert!(ScoreDat::parse(&s.encode(&mut rng), None).is_ok());
    }

    // Game start stamps all 222 records; Spell_Start writes the name, then tests the checksum
    // against it, with no magic test.
    #[test]
    fn stamp_and_attempt() {
        let mut s = ScoreDat::default();
        // Hidden code +1 on a record without the magic is dropped by the stamp: first attempt = 1.
        s.catk[7].counters[C_ATTEMPTS] = [1; 13];
        s.catk[8].present = true;
        s.catk[8].counters[C_ATTEMPTS][0] = 5;
        s.catk[8].name[..2].copy_from_slice(b"ab");
        s.catk[8].checksum = s.catk[8].compute_checksum();
        s.stamp_catk();
        assert!(s.catk.iter().all(|c| c.present));
        assert_eq!(
            s.catk[7],
            Catk {
                present: true,
                ..Default::default()
            }
        );
        s.card_attempt(7, 0, 1, false, b"card", b"owner");
        assert_eq!(s.catk[7].counters[C_ATTEMPTS][0], 1);
        assert_eq!(s.catk[7].counters[C_ATTEMPTS][12], 1);
        // Same name: checksum holds, counters kept.
        s.card_attempt(8, 0, 1, false, b"ab", b"o");
        assert_eq!(s.catk[8].counters[C_ATTEMPTS][0], 6);
        // A new name changes the sum: counters zeroed first; bytes after the NUL keep their old value and are not summed.
        s.card_attempt(8, 0, 2, false, b"x", b"o");
        assert_eq!(s.catk[8].counters[C_ATTEMPTS][0], 1);
        assert_eq!(&s.catk[8].name[..3], b"x\0\0");
        s.card_attempt(8, 0, 2, false, b"abc", b"o");
        s.card_attempt(8, 0, 2, false, b"a", b"o");
        assert_eq!(&s.catk[8].name[..3], b"a\0c");
        assert_eq!(s.catk[8].difficulty, 2);
        assert_eq!(s.catk[8].checksum, s.catk[8].compute_checksum());
        // All 222 records are saved after a stamp.
        let n = s.body().windows(4).filter(|w| w == b"CATK").count();
        assert_eq!(n, CARDS);
    }

    #[test]
    fn real_file() {
        let Some(f) = crate::testdata::game_file("score.dat") else {
            return;
        };
        let ver = crate::testdata::archive().map(|a| a.read(VER_LIST_NAME).unwrap());
        let s = ScoreDat::parse(&f, ver.as_deref()).unwrap();
        let raw = decode_container(&f).unwrap();
        assert_eq!(
            &s.body()[..],
            &raw[0x1C..],
            "re-encoded body differs from the file"
        );
        for c in s.catk.iter().filter(|c| c.present) {
            assert_eq!(c.checksum, c.compute_checksum());
        }
        for k in 0..17 {
            let sum: u32 = (0..6).map(|b| s.plst.counters[b][k]).sum();
            assert_eq!(s.plst.counters[6][k], sum);
        }
        let mut rng = crate::testdata::Counter(1, 0);
        assert_eq!(
            ScoreDat::parse(&s.encode(&mut rng), ver.as_deref()).unwrap(),
            s
        );
    }
}
