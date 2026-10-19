// score.dat (15 §2): container, chunks and the in-memory records the game updates.
use crate::archive::{block_decrypt, block_encrypt, lzss_compress, lzss_decompress};
use anyhow::{Result, bail};

pub const CARDS: usize = 222;
const CRYPT: (u8, u8, usize, usize) = (0x59, 0x79, 0x100, 0xC00);
const CAP: u32 = 9999;
const PLST_CAP: u32 = 999_999;
// VRSM values of the shipped th08.exe (15 §2.10).
pub const EXE_SIZE: u32 = 840_704;
pub const EXE_SUM: u32 = 0xA268_61B9;

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn put_u32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

fn u32s<const N: usize>(b: &[u8], o: usize) -> [u32; N] {
    std::array::from_fn(|i| u32_at(b, o + 4 * i))
}

fn put_u32s(b: &mut [u8], o: usize, v: &[u32]) {
    for (i, x) in v.iter().enumerate() {
        put_u32(b, o + 4 * i, *x);
    }
}

fn chunk(magic: &[u8; 4], size: usize, version: u8) -> Vec<u8> {
    let mut c = vec![0; size];
    c[..4].copy_from_slice(magic);
    c[4..6].copy_from_slice(&(size as u16).to_le_bytes());
    c[6..8].copy_from_slice(&(size as u16).to_le_bytes());
    c[8] = version;
    c
}

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
    // Default table entry (Result_OnRegister, 15 §2.3): name "--------", date "--/--", +0x166 = 1.
    // SPEC-GAP: stage byte 1, not 0: the reference ranking shows "(1)" for default rows and "(5)" for stage index 5.
    pub fn placeholder(score: u32, shot: usize, diff: usize) -> Self {
        let mut h = Self::new();
        h.set(score, shot as u8, diff as u8, 1, b"--------", "--/--");
        h.0[0x166] = 1;
        h
    }
    pub fn is_placeholder(&self) -> bool {
        self.0[0x166] != 0
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
        f32::from_le_bytes(self.0[0x10..0x14].try_into().unwrap())
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clrd {
    pub nc: [u16; 5],
    pub any: [u16; 5],
    pub bad_ending: u8,
}

impl Default for Clrd {
    fn default() -> Self {
        Self {
            nc: [1; 5],
            any: [1; 5],
            bad_ending: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catk {
    pub present: bool,
    pub difficulty: u8,
    pub checksum: u8,
    pub name: [u8; 48],
    pub owner: [u8; 48],
    pub comment: [[u8; 64]; 2],
    // [story bonus, story attempts, story captures, practice bonus, practice attempts, practice captures][shot or 12]
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
    pub fn compute_checksum(&self) -> u8 {
        let n: i32 = self.name.iter().map(|&c| c as i8 as i32).sum();
        let c: u32 = self
            .counters
            .iter()
            .flatten()
            .fold(0, |a, &v| a.wrapping_add(v));
        (n as u32).wrapping_add(c) as u8
    }

    pub fn attempted(&self) -> bool {
        self.counters[C_ATTEMPTS][12] != 0 || self.counters[C_PRACTICE + C_ATTEMPTS][12] != 0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pscr {
    pub plays: [[u32; 5]; 9],
    pub hiscore: [[u32; 5]; 9],
    pub used: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plst {
    pub run_time: [u32; 4],
    pub play_time: [u32; 4],
    pub counters: [[u32; 17]; 7],
    pub music: [u8; 32],
}

impl Default for Plst {
    fn default() -> Self {
        Self {
            run_time: [0; 4],
            play_time: [0; 4],
            counters: [[0; 17]; 7],
            music: [0; 32],
        }
    }
}

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

    // Adds `ms` to an h/m/s/ms clock.
    pub fn add_time(t: &mut [u32; 4], ms: u64) {
        let total = ((t[0] as u64 * 60 + t[1] as u64) * 60 + t[2] as u64) * 1000 + t[3] as u64 + ms;
        *t = [
            (total / 3_600_000) as u32,
            (total / 60_000 % 60) as u32,
            (total / 1000 % 60) as u32,
            (total % 1000) as u32,
        ];
    }
}

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
            lsnm: *b"        \0\0\0\0",
            flsp: [0; 17],
            plst: Plst::default(),
            vrsm,
        }
    }
}

// Removes both layers (archive cipher, then the rolling XOR) and checks the header.
pub fn decode_container(file: &[u8]) -> Result<Vec<u8>> {
    if file.len() < 0x1C {
        bail!("score.dat too small");
    }
    let mut b = block_decrypt(file, CRYPT.0, CRYPT.1, CRYPT.2, CRYPT.3);
    let mut acc = 0u8;
    for i in 1..b.len() - 1 {
        acc = acc.wrapping_add(b[i]).rotate_left(3);
        b[i + 1] ^= acc;
    }
    let sum = b[4..].iter().fold(0u16, |a, &c| a.wrapping_add(c as u16));
    if sum != u16_at(&b, 2) {
        bail!("score.dat checksum");
    }
    if u32_at(&b, 8) != 0x1C || u16_at(&b, 4) != 1 {
        bail!("score.dat header");
    }
    let size = u32_at(&b, 0x14) as usize;
    let mut out = b[..0x1C].to_vec();
    out.extend(lzss_decompress(&b[0x1C..], size));
    Ok(out)
}

// Builds the container around a decompressed body; `r` are the two random header bytes.
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
    let sum = b[4..].iter().fold(0u16, |a, &c| a.wrapping_add(c as u16));
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
    pub fn parse(file: &[u8]) -> Result<Self> {
        let d = decode_container(file)?;
        let mut s = Self::default();
        let mut th8k = false;
        let mut p = 0x1C;
        while p + 12 <= d.len() {
            let size = u16_at(&d, p + 4) as usize;
            if size == 0 {
                bail!("score.dat: empty chunk");
            }
            let Some(c) = d.get(p..p + size) else { break };
            let ver = c[8];
            match (&c[..4], ver) {
                (b"TH8K", 1) => th8k = true,
                (b"HSCR", 4) if size >= 0x168 => s.hscr.push(Hscr(c[..0x168].to_vec())),
                (b"CLRD", 4) if size >= 0x24 => {
                    let i = c[0x21] as usize;
                    if let Some(e) = s.clrd.get_mut(i) {
                        e.nc = std::array::from_fn(|k| u16_at(c, 0x0C + 2 * k));
                        e.any = std::array::from_fn(|k| u16_at(c, 0x16 + 2 * k));
                        e.bad_ending = c[0x20];
                    }
                }
                (b"CATK", 3) if size >= 0x22C => {
                    let id = u16_at(c, 0x0C) as usize;
                    if let Some(e) = s.catk.get_mut(id) {
                        e.present = true;
                        e.checksum = c[0x0E];
                        e.difficulty = c[0x0F];
                        e.name.copy_from_slice(&c[0x10..0x40]);
                        e.owner.copy_from_slice(&c[0x40..0x70]);
                        e.comment[0].copy_from_slice(&c[0x70..0xB0]);
                        e.comment[1].copy_from_slice(&c[0xB0..0xF0]);
                        for (k, row) in e.counters.iter_mut().enumerate() {
                            *row = u32s(c, 0xF0 + 0x34 * k);
                        }
                    }
                }
                (b"PSCR", 2) if size >= 0x178 => {
                    let shot = c[0x174] as usize;
                    if let Some(e) = s.pscr.get_mut(shot) {
                        for st in 0..9 {
                            e.plays[st] = u32s(c, 0x0C + 20 * st);
                            e.hiscore[st] = u32s(c, 0xC0 + 20 * st);
                        }
                        e.used = c[0x175];
                    }
                }
                (b"LSNM", 1) if size >= 0x18 => s.lsnm.copy_from_slice(&c[0x0C..0x18]),
                (b"FLSP", 1) if size >= 0x20 => s.flsp.copy_from_slice(&c[0x0C..0x1D]),
                (b"PLST", 2) if size >= 0x228 => {
                    s.plst.run_time = u32s(c, 0x0C);
                    s.plst.play_time = u32s(c, 0x1C);
                    for b in 0..7 {
                        s.plst.counters[b] = u32s(c, 0x2C + 0x44 * b);
                    }
                    s.plst.music.copy_from_slice(&c[0x208..0x228]);
                }
                (b"VRSM", 1) if size >= 0x1C => s.vrsm.copy_from_slice(&c[0x0C..0x1C]),
                _ => {}
            }
            p += size;
        }
        if !th8k {
            bail!("score.dat: no TH8K chunk");
        }
        Ok(s)
    }

    // Chunk list in the saver's order (15 §2.1).
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

    // Saving draws two Rng_Range16(256) values (15 §2.1).
    pub fn encode(&self, rng: &mut crate::rng::Rng) -> Vec<u8> {
        let r = [rng.range16(256) as u8, rng.range16(256) as u8];
        encode_container(&self.body(), r)
    }

    // ScoreDat_GetHiscore: max(top entry of (shot, difficulty), 100000), ÷10 units.
    pub fn hiscore(&self, shot: usize, diff: usize) -> u32 {
        self.hscr
            .iter()
            .filter(|h| h.shot() == shot && h.difficulty() == diff)
            .map(Hscr::score)
            .max()
            .unwrap_or(0)
            .max(100_000)
    }

    // One ranking list [diff][shot] as Result_OnRegister builds it (15 §2.3): the default table, then the file's
    // entries inserted after equal scores. Default entries carry +0x166 = 1.
    pub fn ranking(&self, shot: usize, diff: usize) -> Vec<Hscr> {
        let mut list: Vec<Hscr> = (0..10u32)
            .map(|i| Hscr::placeholder(100_000 - 10_000 * i, shot, diff))
            .collect();
        for e in self
            .hscr
            .iter()
            .filter(|e| e.shot() == shot && e.difficulty() == diff)
        {
            let at = list.iter().filter(|h| h.score() >= e.score()).count();
            list.insert(at, e.clone());
        }
        list.truncate(10);
        list
    }

    // ScoreList_Insert (new entry after equal scores; INFERRED); only the top 10 survive and default entries are
    // never stored. Returns the 0-based rank.
    pub fn insert_hscr(&mut self, h: Hscr) -> Option<usize> {
        let (shot, diff) = (h.shot(), h.difficulty());
        let mut list = self.ranking(shot, diff);
        let rank = list.iter().filter(|e| e.score() >= h.score()).count();
        list.insert(rank, h);
        list.truncate(10);
        self.hscr
            .retain(|e| e.shot() != shot || e.difficulty() != diff);
        self.hscr
            .extend(list.into_iter().filter(|e| !e.is_placeholder()));
        // The saver walks the lists by difficulty, then shot (15 §2.1).
        self.hscr.sort_by_key(|e| (e.difficulty(), e.shot()));
        (rank < 10).then_some(rank)
    }

    // The entry being named (Result list node byte +9 ≠ 0, 16 §2.3).
    pub fn new_entry_mut(&mut self) -> Option<&mut Hscr> {
        self.hscr.iter_mut().find(|h| h.0[9] != 0)
    }

    // SPEC-GAP: the "new entry" byte +9 is 0 in every saved file read so far; cleared before saving.
    pub fn clear_new_flags(&mut self) {
        for h in &mut self.hscr {
            h.0[9] = 0;
        }
    }

    // Spell_Start attempt (15 §2.5): checksum check, then attempts of this shot and of all shots.
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
        if c.present && c.compute_checksum() != c.checksum {
            c.counters = [[0; 13]; 6];
        }
        c.present = true;
        c.difficulty = diff;
        c.name = [0; 48];
        c.name[..name.len().min(47)].copy_from_slice(&name[..name.len().min(47)]);
        c.owner = [0; 48];
        c.owner[..owner.len().min(47)].copy_from_slice(&owner[..owner.len().min(47)]);
        let row = &mut c.counters[if practice { C_PRACTICE } else { 0 } + C_ATTEMPTS];
        for k in [shot % 12, 12] {
            row[k] = (row[k] + 1).min(CAP);
        }
        c.checksum = c.compute_checksum();
    }

    // Spell_End capture: captures and max bonus; comments only in spell practice.
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

// Save storage: never the game directory. Native: `TH08_SAVE_DIR`, else $XDG_CONFIG_HOME/th08 or ~/.config/th08,
// else next to the binary. Web: localStorage (hex).
#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static SAVE_DIR: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

// Tools/tests: a private save directory for this thread (overrides `TH08_SAVE_DIR`).
#[cfg(not(target_arch = "wasm32"))]
pub fn set_save_dir(dir: Option<std::path::PathBuf>) {
    SAVE_DIR.with_borrow_mut(|d| *d = dir);
}

#[cfg(not(target_arch = "wasm32"))]
fn save_dir() -> Option<std::path::PathBuf> {
    if let Some(d) = SAVE_DIR.with_borrow(|d| d.clone()) {
        return Some(d);
    }
    if let Some(d) = crate::env::var("TH08_SAVE_DIR") {
        return Some(d.into());
    }
    let cfg = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")));
    cfg.map(|c| c.join("th08")).or_else(|| {
        std::env::current_exe()
            .ok()?
            .parent()
            .map(|p| p.to_path_buf())
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn storage_read(name: &str) -> Option<Vec<u8>> {
    std::fs::read(save_dir()?.join(name)).ok()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn storage_write(name: &str, data: &[u8]) {
    let Some(dir) = save_dir() else { return };
    let path = dir.join(name);
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    if let Err(e) = std::fs::write(&path, data) {
        log::warn!("{}: {e}", path.display());
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn storage_list(prefix: &str) -> Vec<String> {
    let Some(dir) = save_dir() else {
        return Vec::new();
    };
    let (sub, file) = prefix.rsplit_once('/').unwrap_or(("", prefix));
    let Ok(rd) = std::fs::read_dir(dir.join(sub)) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with(file))
        .map(|n| {
            if sub.is_empty() {
                n
            } else {
                format!("{sub}/{n}")
            }
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn storage_read(name: &str) -> Option<Vec<u8>> {
    let s = local_storage()?.get_item(&format!("th08:{name}")).ok()??;
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}

#[cfg(target_arch = "wasm32")]
pub fn storage_write(name: &str, data: &[u8]) {
    use std::fmt::Write;
    let mut s = String::with_capacity(data.len() * 2);
    for b in data {
        let _ = write!(s, "{b:02x}");
    }
    if let Some(ls) = local_storage() {
        let _ = ls.set_item(&format!("th08:{name}"), &s);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn storage_list(prefix: &str) -> Vec<String> {
    let Some(ls) = local_storage() else {
        return Vec::new();
    };
    let n = ls.length().unwrap_or(0);
    (0..n)
        .filter_map(|i| ls.key(i).ok()?)
        .filter_map(|k| k.strip_prefix("th08:").map(str::to_string))
        .filter(|k| k.starts_with(prefix))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut rng = crate::rng::Rng::seeded(7);
        let f = s.encode(&mut rng);
        assert_eq!(rng.counter, 2);
        let t = ScoreDat::parse(&f).unwrap();
        assert_eq!(t, s);
        assert_eq!(t.hiscore(0, 1), 5_000_000);
        assert_eq!(t.card_history(54, 0, false), (1, 1));
    }

    #[test]
    fn real_file() {
        let Some(dir) = std::env::var("TH08_DIR").ok() else {
            return;
        };
        let Ok(f) = std::fs::read(format!("{dir}/score.dat")) else {
            return;
        };
        let s = ScoreDat::parse(&f).unwrap();
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
        let mut rng = crate::rng::Rng::seeded(1);
        assert_eq!(ScoreDat::parse(&s.encode(&mut rng)).unwrap(), s);
    }
}
