//! Replays (15 §3, 11 §6): .rpy load/save, the per-stage records, playback and recording.

use crate::archive::{lzss_compress, lzss_decompress};
use anyhow::{Result, bail};

// File header size, end of the info block, stage record header size and checksum base (15 §3.5, 11 §6).
const HEADER: usize = 0x68;
const INFO: usize = 0x134;
const REC: usize = 0x24;
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
    // Raw "USER" section after the file size (15 §3.5), kept as read.
    pub user: Vec<u8>,
}

impl Default for Replay {
    // Info constants of 15 §3.5: +0x69 = 100, +0x120 = 30 (fps interval), exe size/sum and tag "0100d" (15 §2.10).
    fn default() -> Self {
        let mut info = vec![0; INFO - HEADER];
        info[1] = 100;
        put_u32(&mut info, 0x120 - HEADER, 30);
        put_u32(&mut info, 0x124 - HEADER, crate::scoredat::EXE_SIZE);
        put_u32(&mut info, 0x128 - HEADER, crate::scoredat::EXE_SUM);
        info[0x12C - HEADER..0x131 - HEADER].copy_from_slice(b"0100d");
        Self {
            debug: 0,
            info,
            stages: Default::default(),
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
    pub fn parse(file: &[u8]) -> Result<Self> {
        if file.len() < HEADER || &file[..4] != b"T8RP" || u16_at(file, 4) != 6 {
            bail!("not a th08 replay");
        }
        let size = (u32_at(file, 0x0C) as usize).min(file.len());
        if size < HEADER {
            bail!("replay size");
        }
        let mut b = file[..size].to_vec();
        crypt(&mut b[0x18..], file[0x15], true);
        let sum = b[0x15..]
            .iter()
            .fold(CHECK_BASE, |a, &c| a.wrapping_add(c as u32));
        if sum != u32_at(&b, 0x10) {
            bail!("replay checksum");
        }
        let csize = (u32_at(&b, 0x18) as usize).min(b.len() - HEADER);
        let dsize = u32_at(&b, 0x1C) as usize;
        let mut img = b[..HEADER].to_vec();
        img.extend(lzss_decompress(&b[HEADER..HEADER + csize], dsize));
        if img.len() < INFO {
            bail!("replay too short");
        }
        let info = img[HEADER..INFO].to_vec();
        if info[0xD9 - HEADER] != 0 {
            bail!("replay recorded in slow mode");
        }
        if !crate::scoredat::exe_check(
            &info[0x12C - HEADER..0x134 - HEADER],
            u32_at(&info, 0x124 - HEADER),
            u32_at(&info, 0x128 - HEADER),
        ) {
            bail!("replay exe check");
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
                bail!("replay stage {i} record");
            }
            let mut r = StageRecord::parse(&img[o..f]);
            r.fps = img[f..next].to_vec();
            stages[i] = Some(r);
        }
        Ok(Self {
            debug: file[6],
            info,
            stages,
            user: file[size..].to_vec(),
        })
    }

    pub fn clear_status(&self) -> u8 {
        self.info[0x11C - HEADER]
    }
    pub fn card_name(&self) -> &[u8] {
        let c = &self.info[0x7E - HEADER..0xAE - HEADER];
        &c[..c.iter().position(|&b| b == 0).unwrap_or(c.len())]
    }

    // Replay_Finish image/compress/checksum/encrypt (15 §3.5, §3.6 steps 2–5). 3 RNG draws in this order:
    // key byte, info +0x68, header +0x14 (Rng_Range16(256) each). `user` is appended as is.
    pub fn encode(&mut self, rng: &mut crate::rng::Rng) -> Vec<u8> {
        let key = rng.range16(256) as u8;
        self.info[0] = rng.range16(256) as u8;
        let r14 = rng.range16(256) as u8;
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

// Replay_PlayCalc 0x452550 prio 6, Replay_FrameHook 0x4522A0 prio 7, Replay_RecordCalc 0x452310 prio 17,
// Replay_FastForward 0x452490 prio 18 (15 §3.1, 11 §1.4).
pub const PLAY_PRIO: u16 = 6;
pub const HOOK_PRIO: u16 = 7;
pub const RECORD_PRIO: u16 = 17;
pub const FF_PRIO: u16 = 18;

// Calc tags of the replay task's nodes.
const TAG_MAIN: u8 = 0;
const TAG_FF: u8 = 1;
const TAG_HOOK: u8 = 2;

// Runtime replay object (15 §3.1): mode 0 records, mode 1 plays back. The data lives in `Game::rpy`.
pub struct ReplayTask {
    play: bool,
    stage: usize,
    // +0x50 input cursor (in words), +0 frame counter, +4 clear-bonus frame counter, fps cursor q (+0xA0).
    cursor: usize,
    frame: u32,
    bonus_frames: u32,
    fps_q: usize,
    // Playback's key-repeat emulation counter (0x164D53C, 15 §3.3).
    repeat_ctr: u32,
    // Header +6 ≠ 0: Replay_PlayCalcDebug 0x4526C0 with 6-byte frames (15 §3.1).
    debug: bool,
    // +0xD8 RNG state stored by Replay_FrameHook (11 §1.4).
    rng_snap: u16,
}

pub fn register(
    ctx: &mut crate::game::Ctx,
    play: bool,
    st: &crate::player::Stats,
) -> anyhow::Result<crate::task::TaskRef<crate::game::Ctx>> {
    use crate::task::{self, Chain};
    // Replay_Register 0x451F90 sets g_Input and g_InputPrev to 0 (15 §3.1).
    ctx.input.game = 0;
    ctx.input.game_prev = 0;
    let debug = play && ctx.game.rpy.as_ref().is_some_and(|r| r.debug != 0);
    let mut rt = ReplayTask {
        play,
        stage: ctx.game.stage % 9,
        cursor: 0,
        frame: 0,
        bonus_frames: 0,
        fps_q: 0,
        repeat_ctr: 0,
        debug,
        rng_snap: 0,
    };
    if !play || debug {
        rt.frame_hook(&mut ctx.anm.rng);
    }
    let t = task::task(rt);
    if play {
        task::add(ctx, Chain::Calc, PLAY_PRIO, TAG_MAIN, t.clone())?;
        task::add(ctx, Chain::Calc, FF_PRIO, TAG_FF, t.clone())?;
    } else {
        record_stage_start(&mut ctx.game, st);
        task::add(ctx, Chain::Calc, RECORD_PRIO, TAG_MAIN, t.clone())?;
    }
    // Replay_FrameHook: always while recording, during playback only with the debug flag; Replay_Register also
    // calls it once on linking (above, 11 §1.4).
    if !play || debug {
        task::add(ctx, Chain::Calc, HOOK_PRIO, TAG_HOOK, t.clone())?;
    }
    Ok(t)
}

// Replay_RecordStageStart 0x452830 (15 §3.2): the previous record gets the current score, then a new record with the
// dummy word (the debug words at +0x26 / +0x28 are overwritten by frames 1 and 2, so they are not kept).
fn record_stage_start(g: &mut crate::gamemgr::Game, st: &crate::player::Stats) {
    let stage = g.stage % 9;
    if g.rpy.is_none() {
        let mut r = new_recording(g.shot, g.difficulty, g.flags.get(), g.card, &g.cfg_bytes());
        r.info[0x11C - HEADER] = clear_status(g);
        g.rpy = Some(r);
    }
    let r = g.rpy.as_mut().unwrap();
    // Only the record of the highest stage below this one gets +0 := score (scan 0 .. stage − 1, last non-null).
    if let Some(s) = r.stages[..stage].iter_mut().flatten().last() {
        s.score = st.score10 as u32;
    }
    r.stages[stage] = Some(StageRecord {
        score: 0,
        point_items: st.point_items2,
        graze: st.graze_total as u32,
        extends: st.extends,
        next_extend: st.next_extend,
        max_point: st.max_point,
        gauge: st.gauge,
        seed: g.stage_seed,
        power: st.power as u8,
        lives: st.lives as u8,
        bombs: st.bombs as u8,
        rank: crate::gamemgr::rank() as u8,
        b20: st.d0b0,
        spells: st.spells as u8,
        clock: st.clock as i8,
        inputs: vec![0],
        fps: Vec::new(),
    });
}

// Info +0x11C at record start (15 §3.5): 2 if the shot passes 0x43C15F (a no-continue Final B clear on E–L, or shot ≥ 4),
// else 1 if it passes 0x43C23B (a Final A clear on E–L), else 0 (15 §2.4). Read back as the route byte (10 §4 step 3).
fn clear_status(g: &crate::gamemgr::Game) -> u8 {
    let t = g.shot as usize % 12;
    if t >= 4 || g.clear_nc[t][..4].iter().any(|m| m & 0x4000 != 0) {
        2
    } else if g.clear_any[t][..4].iter().any(|m| m & 0x8000 != 0) {
        1
    } else {
        0
    }
}

// Info block at record start (15 §3.5): shot, difficulty, "NO NAME", practice flag +0x7B, spell id +0x7C (−1 outside
// spell practice), +0xAE = 0x100, config copy +0xB4.
fn new_recording(shot: u8, diff: i32, flags: u32, card: i16, cfg: &[u8; 60]) -> Replay {
    let mut r = Replay::default();
    let i = &mut r.info;
    i[0x6A - HEADER] = shot;
    i[0x6B - HEADER] = diff as u8;
    i[0x72 - HEADER..0x79 - HEADER].copy_from_slice(b"NO NAME");
    i[0x7B - HEADER] = (flags & crate::gamemgr::F_PRACTICE != 0) as u8;
    let id = if flags & crate::gamemgr::F_SPELL_PRACTICE != 0 {
        card
    } else {
        -1
    };
    i[0x7C - HEADER..0x7E - HEADER].copy_from_slice(&id.to_le_bytes());
    i[0xAE - HEADER..0xB0 - HEADER].copy_from_slice(&0x100u16.to_le_bytes());
    i[0xB4 - HEADER..0xF0 - HEADER].copy_from_slice(cfg);
    r
}

// Replay start from the menu (15 §3.8) or the title demo (12 §2.2): header values, flags, scene 2.
// Flags become replay (| demo), keeping only the unlock bits 15–17 (15 §2.2: playback keeps them).
pub fn start_playback(
    g: &mut crate::gamemgr::Game,
    r: Replay,
    stage: Option<usize>,
    demo: Option<u8>,
) -> bool {
    use crate::gamemgr::*;
    let Some(stage) = stage.or_else(|| r.first_stage()) else {
        return false;
    };
    g.difficulty = r.difficulty().min(4) as i32;
    g.shot = r.shot() % 12;
    g.stage = stage;
    g.flags.set(g.flags & (7 << 15) | F_REPLAY);
    let id = r.spell_id();
    if id >= 0 {
        // Only bit 14: bit 0 stays clear in a spell-practice replay (15 §3.8).
        g.flags |= F_SPELL_PRACTICE;
        g.card = id;
    }
    if let Some(n) = demo {
        g.flags |= F_DEMO;
        g.demo = n;
        g.demo_frame = 0;
        // Byte 0x164D0B2 = shot % 2 for attract demos (12 §2.2).
        g.demo_shot_bias = r.shot() % 2;
    } else {
        g.demo_shot_bias = 0;
    }
    g.rpy = Some(r);
    g.fresh_start = true;
    g.request(SCENE_GAME);
    true
}

// Back to the menu after playback: replay state is dropped (port bookkeeping).
pub fn end_playback(g: &mut crate::gamemgr::Game) {
    if g.replay() {
        g.rpy = None;
        g.watched_replay = g.flags & crate::gamemgr::F_DEMO == 0;
    }
    g.flags &= !(crate::gamemgr::F_REPLAY
        | crate::gamemgr::F_DEMO
        | crate::gamemgr::F_PRACTICE
        | crate::gamemgr::F_SPELL_PRACTICE);
}

// Replay_PlayStageLoad 0x452D60 (11 §3.4, 15 §3.4): stats, rank and seed from the record of the stage being started;
// the score comes from the last present record of a lower stage (kept as is when there is none).
// SetLives and SetPower each consume RNG before Rng_SetState(seed) (11 §1.3), hence the order below.
pub fn play_stage_load(
    g: &mut crate::gamemgr::Game,
    st: &mut crate::player::Stats,
    rng: &mut crate::rng::Rng,
) -> Result<()> {
    let stage = g.stage % 9;
    let Some(r) = g.rpy.as_ref() else {
        bail!("no replay loaded")
    };
    // 11 §3.4: all 60 bytes of info +0xB4 over the gameplay copy *(GM+4), as stored (no field selection, no clamp).
    g.cfg_game.copy_from_slice(r.config());
    let Some(rec) = r.stages[stage].as_ref() else {
        bail!("replay has no record for stage {stage}")
    };
    if let Some(p) = r.stages[..stage].iter().flatten().last() {
        st.score10 = p.score as i64;
        st.score_disp = p.score as i64;
    }
    st.score_step = 0;
    let mut rs = crate::gamemgr::rank_state();
    rs[0] = rec.rank as i32;
    crate::gamemgr::set_rank_state(rs);
    st.lives = rec.lives as f32;
    crate::player::Stats::checksum(rng);
    st.bombs = rec.bombs as f32;
    st.set_power(rec.power as f32, rng);
    st.graze_total = rec.graze as i32;
    st.point_items2 = rec.point_items;
    st.max_point = rec.max_point;
    st.spells = rec.spells as i32;
    st.extends = rec.extends;
    st.next_extend = rec.next_extend;
    st.gauge = rec.gauge;
    st.clock = rec.clock as i32;
    st.d0b0 = rec.b20;
    rng.state = rec.seed;
    Ok(())
}

// Scene change out of the game or the result (port): a recording the result dialogue did not save is dropped.
// Tool switch `TH08_REPLAY_SAVE=<1–15>` saves it to that slot with the LSNM name instead.
pub fn finish(ctx: &mut crate::game::Ctx) {
    match ctx.settings.replay_save {
        Some(slot) => {
            let mut name = [0; 8];
            name.copy_from_slice(&ctx.game.score.lsnm[..8]);
            save(ctx, slot.clamp(1, 15) as usize, &name);
        }
        None => drop(take_recording(&mut ctx.game)),
    }
}

// Replay_Finish("./replay/th8_%.2d.rpy", slot) 0x4531F0 (15 §3.6), called by Result state 13 (16 §2.6).
pub fn save(ctx: &mut crate::game::Ctx, slot: usize, name: &[u8; 8]) -> bool {
    let Some(bytes) = build_save(&mut ctx.game, name, &mut ctx.anm.rng) else {
        return false;
    };
    let path = format!("replay/th8_{slot:02}.rpy");
    crate::scoredat::storage_write(&path, &bytes);
    log::info!("replay saved to {path} ({} bytes)", bytes.len());
    true
}

// 15 §3.6: skip tests, then step 1 (date, name, score, slowdown, card name), steps 2–5 (`Replay::encode`) and the
// USER section (15 §3.5). None = nothing to write.
pub fn build_save(
    g: &mut crate::gamemgr::Game,
    name: &[u8; 8],
    rng: &mut crate::rng::Rng,
) -> Option<Vec<u8>> {
    let mut r = take_recording(g)?;
    // Skipped for a normal game on E–L whose config changed since record start, and for a recorded slow mode (+0xD9).
    if !g.practice() && g.difficulty < 4 && r.config() != g.cfg_bytes() {
        return None;
    }
    if r.info[0xD9 - HEADER] != 0 {
        return None;
    }
    r.info[0x72 - HEADER..0x7B - HEADER].fill(0);
    r.info[0x72 - HEADER..0x7A - HEADER].copy_from_slice(name);
    // Slowdown % (15 §2.11): the port never drops frames, so x = 1 and the value is 0; +0x114 = % + 1.12 (0x4B7088).
    let slow = 0f32;
    r.info[0x114 - HEADER..0x118 - HEADER].copy_from_slice(&(slow + 1.12).to_le_bytes());
    r.info[0x118 - HEADER..0x11C - HEADER].copy_from_slice(&slow.to_le_bytes());
    let id = r.spell_id();
    if id >= 0 {
        // 48 bytes of the in-memory CATK[id] +0x10 (15 §3.6 step 1).
        let c = &g.score.catk[id as usize % crate::scoredat::CARDS].name;
        r.info[0x7E - HEADER..0xAE - HEADER].copy_from_slice(c);
    }
    r.user = user_section(&user_lines(g, &r, slow));
    Some(r.encode(rng))
}

// USER lines `key\tvalue` (15 §3.5): キャラ名 table 0x4C7F4C with its U+3000 padding, 難易度 table 0x4C7F30,
// 最終ステージ table 0x4C7FAC (0x453CD1, "Clear" from stage 9 on or with flags bit 4).
fn user_lines(g: &crate::gamemgr::Game, r: &Replay, slow: f32) -> Vec<(&'static str, String)> {
    const DIFF: [&str; 5] = ["Easy", "Normal", "Hard", "Lunatic", "Extra"];
    const STAGE: [&str; 9] = [
        "Stage 1",
        "Stage 2",
        "Stage 3",
        "Stage 4-uncanny",
        "Stage 4-powerful",
        "Stage 5",
        "Stage 6-Eirin",
        "Stage 6-Kaguya",
        "Extra Stage",
    ];
    let st = &g.stats;
    let id = r.spell_id();
    let last = if id >= 0 {
        (
            "カード名",
            format!(
                "No.{:3} {}",
                id as i32 + 1,
                crate::text::decode_sjis(r.card_name())
            ),
        )
    } else if g.flags & crate::gamemgr::F_ENDING_OK != 0 {
        ("最終ステージ", "Clear".into())
    } else {
        (
            "最終ステージ",
            STAGE.get(g.stage).copied().unwrap_or("Clear").into(),
        )
    };
    // HSCR +0x84 = human·10000/total (16 §2.2), printed ÷ 100.
    let human = (st.rec.human as u64 * 10000)
        .checked_div(st.rec.total as u64)
        .unwrap_or(0);
    vec![
        ("プレイヤー名", r.name()),
        ("プレイ時刻", crate::supervisor::datetime_now()),
        (
            "キャラ名",
            crate::menulist::SHOT_NAMES[r.shot() as usize % 12].into(),
        ),
        ("スコア", format!("{}0", r.score())),
        ("難易度", DIFF[r.difficulty().min(4) as usize].into()),
        last,
        ("ミス回数", st.deaths.to_string()),
        ("ボム回数", st.bombs_used.to_string()),
        ("処理落ち率", format!("{slow:.6}%")),
        ("人間率", format!("{:3.2}％", human as f32 / 100.0)),
        ("ゲームのバージョン", "1.00d".into()),
    ]
}

// "USER", u32 total length (with this 12-byte head), u8 type 0, 3 garbage bytes (written 0); then the Shift-JIS lines
// with CR LF, NUL-terminated and padded to an even length (15 §3.5).
pub fn user_section(lines: &[(&str, String)]) -> Vec<u8> {
    let mut text = String::new();
    for (k, v) in lines {
        text += &format!("{k}\t{v}\r\n");
    }
    let mut body = encoding_rs::SHIFT_JIS.encode(&text).0.into_owned();
    body.push(0);
    if body.len() % 2 != 0 {
        body.push(0);
    }
    let mut out = b"USER".to_vec();
    out.extend(((12 + body.len()) as u32).to_le_bytes());
    out.extend([0; 4]);
    out.extend(body);
    out
}

pub fn take_recording(g: &mut crate::gamemgr::Game) -> Option<Replay> {
    if g.replay() {
        return None;
    }
    let mut r = g.rpy.take()?;
    let score = g.stats.score10 as u32;
    for s in r.stages.iter_mut().flatten() {
        if s.score == 0 {
            s.score = score;
        }
    }
    if let Some(s) = r.stages.iter_mut().flatten().last() {
        // 0 terminator + 4 uninitialised bytes, written as the demo files' 0xCDCD (15 §3.2).
        s.inputs.extend([0, 0xCDCD, 0xCDCD]);
    }
    put_u32(&mut r.info, 0xB0 - HEADER, score);
    let date = crate::supervisor::date_md();
    let d = &mut r.info[0x6C - HEADER..0x72 - HEADER];
    d.fill(0);
    d[..date.len().min(5)].copy_from_slice(&date.as_bytes()[..date.len().min(5)]);
    Some(r)
}

// The boss test 0x42F1F0 used by fast playback (15 §3.3).
fn boss_active() -> bool {
    crate::gamemgr::current()
        .and_then(|g| g.borrow().enemies.clone())
        .is_some_and(|e| e.borrow().bosses.iter().any(Option::is_some))
}

// Replay_FastForward 0x452490 (15 §3.3): true = restart the calc chain (6). The boss test 0x42F1F0 is made last.
fn fast_forward(
    flags: u32,
    dialogue: bool,
    speed: u8,
    frame: u32,
    boss: impl FnOnce() -> bool,
) -> bool {
    use crate::gamemgr::{F_CLEAR_PANEL, F_RUNNING};
    if flags & F_RUNNING == 0 || flags & F_CLEAR_PANEL != 0 {
        return false;
    }
    if dialogue && frame % 3 != 2 {
        return true;
    }
    speed == 2 && frame % 5 != 4 && !boss()
}

// One playback frame from the input words at `cursor`: (input, next cursor). PlayCalc steps 1 word; PlayCalcDebug
// (header +6 ≠ 0) steps 3 words and never reads the other two (15 §3.1). Past the end the input is 0.
fn next_frame(words: &[u16], cursor: usize, debug: bool) -> (u16, usize) {
    let w = words.get(cursor).copied().unwrap_or(0);
    (w, cursor + if debug { 3 } else { 1 })
}

impl ReplayTask {
    // Replay_RecordCalc 0x452310 (15 §3.2): runs after the game objects, so frame n's input is the raw word sampled at
    // the end of frame n − 1.
    fn record(&mut self, ctx: &mut crate::game::Ctx) {
        self.record_game(&mut ctx.game, &mut ctx.input);
    }

    fn record_game(&mut self, g: &mut crate::gamemgr::Game, i: &mut crate::input::Input) {
        if g.flags & crate::gamemgr::F_RUNNING == 0 {
            return;
        }
        i.game_prev = i.game;
        i.game = i.held;
        // Step 2: slow mode records nothing (Supervisor+0x1A4 bit 3 is never set in retail code).
        // The slow byte is +0x25 of the gameplay config copy `*(GM+4)` (15 §3.2 step 2).
        if g.game_slow() != 0 {
            return;
        }
        // Step 3: at most 3 frames are recorded during the stage-clear bonus (flags bit 9).
        if g.flags & crate::gamemgr::F_CLEAR_PANEL != 0 {
            if self.bonus_frames > 2 {
                return;
            }
            self.bonus_frames += 1;
        }
        let raw = i.held;
        let Some(s) = g.rpy.as_mut().and_then(|r| r.stages[self.stage].as_mut()) else {
            return;
        };
        s.inputs.push(raw);
        if self.frame % 30 == 0 {
            let q = self.fps_q;
            // Overlapping two-byte write: fps[q] = fps_now | flag (always 0), fps[q+1] = fps_now (15 §3.2 step 5).
            // No fps meter: sample 0 is the carried fps_now, later samples 60, which a meter window at 60 fps
            // has stored into fps_now by then (15 §3.2).
            if q > 0 {
                g.replay_fps = 60;
            }
            s.fps.resize(q + 2, 0);
            s.fps[q] = g.replay_fps;
            s.fps[q + 1] = g.replay_fps;
            self.fps_q += 1;
        }
        self.frame += 1;
    }

    // Replay_FrameHook 0x4522A0 (11 §1.4): +0xD8 := RNG state, RNG call counter := 0. Nothing reads either; the
    // statistics word +0xDA and GM+0x3DB98 it also clears are never read either and not kept.
    fn frame_hook(&mut self, rng: &mut crate::rng::Rng) {
        self.rng_snap = rng.state;
        rng.counter = 0;
    }

    // Replay_PlayCalc 0x452550 (15 §3.3): input substitution and the key-repeat emulation; not during the clear bonus.
    // The first word read is the dummy 0, matching the one-frame latency of recording.
    // Replay_PlayCalcDebug 0x4526C0 (15 §3.1) is the same with the input read through cursor +0x78 in 6-byte steps.
    fn play(&mut self, ctx: &mut crate::game::Ctx) {
        let g = &ctx.game;
        // The slow byte is +0x25 of the gameplay config copy `*(GM+4)` (15 §3.3).
        if g.flags & crate::gamemgr::F_RUNNING == 0
            || g.game_slow() != 0
            || g.flags & crate::gamemgr::F_CLEAR_PANEL != 0
        {
            return;
        }
        let words = g
            .rpy
            .as_ref()
            .and_then(|r| r.stages[self.stage].as_ref())
            .map_or(&[][..], |s| &s.inputs[..]);
        let (w, next) = next_frame(words, self.cursor, self.debug);
        self.cursor = next;
        let i = &mut ctx.input;
        i.game_prev = i.game;
        i.game = w;
        i.repeat = false;
        if i.game == i.game_prev {
            if self.repeat_ctr > 29 {
                i.repeat = self.repeat_ctr & 7 == 0;
                if self.repeat_ctr > 37 {
                    self.repeat_ctr = 30;
                }
            }
            self.repeat_ctr += 1;
        } else {
            self.repeat_ctr = 0;
        }
        if self.frame % 30 == 0 {
            // QUIRK: playback reads byte q + 1 (one sample ahead).
            let b = ctx
                .game
                .rpy
                .as_ref()
                .and_then(|r| r.stages[self.stage].as_ref())
                .and_then(|s| s.fps.get(self.fps_q + 1).copied())
                .unwrap_or(60);
            ctx.game.replay_fps = b & 0x7F;
            ctx.game.replay_fps_red = b >> 7 != 0;
            self.fps_q += 1;
        }
        self.frame += 1;
    }
}

impl crate::task::Task<crate::game::Ctx> for ReplayTask {
    fn calc(&mut self, ctx: &mut crate::game::Ctx, tag: u8) -> crate::task::Ret {
        use crate::task::Ret;
        match (self.play, tag) {
            (_, TAG_HOOK) => self.frame_hook(&mut ctx.anm.rng),
            (false, _) => self.record(ctx),
            (true, TAG_MAIN) => self.play(ctx),
            // Replay_FastForward 0x452490 (15 §3.3); `replay_ff` = false is a tool switch that disables both restarts.
            // Only with flags bit 2 set and bit 9 clear; both tests use the replay frame counter +0: skippable dialogue
            // first, the fast test (speed byte 2) only when it does not restart.
            (true, _) => {
                let g = &ctx.game;
                if g.replay_ff
                    && fast_forward(
                        g.flags.get(),
                        g.dialogue_skippable,
                        g.replay_speed,
                        self.frame,
                        boss_active,
                    )
                {
                    return Ret::Restart;
                }
            }
        }
        Ret::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 11 §1.4: the hook stores the RNG state at +0xD8 and resets the call counter, leaving the state.
    #[test]
    fn frame_hook_snapshots_rng() {
        let mut t = ReplayTask {
            play: false,
            stage: 0,
            cursor: 0,
            frame: 0,
            bonus_frames: 0,
            fps_q: 0,
            repeat_ctr: 0,
            debug: false,
            rng_snap: 0,
        };
        let mut rng = crate::rng::Rng::seeded(0x1234);
        rng.next_u32();
        let s = rng.state;
        t.frame_hook(&mut rng);
        // The trace count (`total`) is not reset by the hook.
        assert_eq!(
            (t.rng_snap, rng.state, rng.counter, rng.total),
            (s, s, 0, 2)
        );
    }

    // 15 §3.1: PlayCalc reads every word; PlayCalcDebug reads only the first word of each 6-byte step.
    #[test]
    fn playback_frame_words() {
        let w = [0, 0, 0x55, 0x11, 9, 0x66];
        assert_eq!(next_frame(&w, 0, false), (0, 1));
        assert_eq!(next_frame(&w, 3, false), (0x11, 4));
        assert_eq!(next_frame(&w, 0, true), (0, 3));
        assert_eq!(next_frame(&w, 3, true), (0x11, 6));
        assert_eq!(next_frame(&w, 6, true), (0, 9));
        assert_eq!(next_frame(&w, 6, false), (0, 7));
    }

    // 15 §3.2 (audit A.48): a stage start sets +0 := score only on the highest recorded stage below it.
    #[test]
    fn stage_start_scores_previous_record() {
        let mut g = crate::gamemgr::Game::default();
        let mut st = crate::player::Stats::default();
        let mut r = Replay::default();
        r.stages[0] = Some(StageRecord {
            score: 11,
            ..Default::default()
        });
        r.stages[2] = Some(StageRecord {
            score: 22,
            ..Default::default()
        });
        r.stages[5] = Some(StageRecord {
            score: 55,
            ..Default::default()
        });
        g.rpy = Some(r);
        g.stage = 4;
        st.score10 = 777;
        record_stage_start(&mut g, &st);
        let s: Vec<_> = g
            .rpy
            .unwrap()
            .stages
            .iter()
            .map(|s| s.as_ref().map(|s| s.score))
            .collect();
        assert_eq!(
            s,
            [
                Some(11),
                None,
                Some(777),
                None,
                Some(0),
                Some(55),
                None,
                None,
                None
            ]
        );
    }

    // 11 §3.4 (audit A.47): all 60 config bytes go over the gameplay copy unclamped; the Supervisor config is untouched.
    #[test]
    fn stage_load_copies_config() {
        let mut g = crate::gamemgr::Game::default();
        let mut st = crate::player::Stats::default();
        let mut rng = crate::rng::Rng::default();
        let mut r = Replay::default();
        r.stages[0] = Some(StageRecord::default());
        let cfg: Vec<u8> = (0..60u8).map(|i| i.wrapping_mul(37) ^ 0xA5).collect();
        r.info[0xB4 - HEADER..0xF0 - HEADER].copy_from_slice(&cfg);
        g.rpy = Some(r);
        g.stage = 0;
        let live = g.cfg;
        play_stage_load(&mut g, &mut st, &mut rng).unwrap();
        assert_eq!(g.cfg_game[..], cfg[..]);
        assert_eq!(g.cfg, live);
    }

    // 15 §3.8 (audit A.49): a spell-practice replay starts with bit 14 and bit 3, bit 0 clear.
    #[test]
    fn spell_practice_playback_flags() {
        use crate::gamemgr::*;
        let mut g = Game::default();
        g.flags.set(F_PRACTICE);
        let mut r = Replay::default();
        r.stages[0] = Some(StageRecord::default());
        r.info[0x7C - HEADER..0x7E - HEADER].copy_from_slice(&5i16.to_le_bytes());
        assert_eq!(r.spell_id(), 5);
        assert!(start_playback(&mut g, r, None, None));
        assert_eq!(
            g.flags & (F_PRACTICE | F_SPELL_PRACTICE | F_REPLAY),
            F_SPELL_PRACTICE | F_REPLAY
        );
        assert_eq!(g.card, 5);
    }

    // 15 §3.5 / §3.6 (impl69): a saved replay parses back with the same header fields, USER section appended.
    #[test]
    fn save_parses_back() {
        let mut g = crate::gamemgr::Game::default();
        g.shot = 3;
        g.difficulty = 2;
        g.clear_nc[3][1] |= 0x4000;
        let st = crate::player::Stats {
            d0b0: 7,
            ..Default::default()
        };
        record_stage_start(&mut g, &st);
        let rec = g.rpy.as_mut().unwrap().stages[0].as_mut().unwrap();
        rec.inputs.extend([1, 2, 3]);
        g.stats.score10 = 123_456;
        let f = build_save(&mut g, b"TESTER  ", &mut crate::rng::Rng::seeded(5)).unwrap();
        assert!(g.rpy.is_none());
        let r = Replay::parse(&f).unwrap();
        assert_eq!((r.shot(), r.difficulty()), (3, 2));
        assert_eq!(r.name(), "TESTER  ");
        assert_eq!(r.date(), crate::supervisor::date_md());
        assert_eq!(r.score(), 123_456);
        assert!(!r.practice());
        assert_eq!(r.spell_id(), -1);
        assert_eq!(r.clear_status(), 2);
        assert_eq!(r.config(), g.cfg_bytes());
        assert_eq!(r.slowdown(), 0.0);
        assert_eq!(
            r.info[0x114 - HEADER..0x118 - HEADER],
            1.12f32.to_le_bytes()
        );
        assert_eq!(r.info[0x120 - HEADER], 30);
        assert_eq!(&r.info[0x12C - HEADER..0x131 - HEADER], b"0100d");
        let s = r.stages[0].as_ref().unwrap();
        assert_eq!(s.b20, 7);
        assert_eq!(s.score, 123_456);
        assert_eq!(s.inputs, [0, 1, 2, 3, 0, 0xCDCD, 0xCDCD]);
        // USER: "USER", total length, type 0; Shift-JIS lines, NUL-terminated, even length.
        let u = &r.user;
        assert_eq!(&u[..4], b"USER");
        assert_eq!(u32_at(u, 4) as usize, u.len());
        assert_eq!((u[8], u.len() % 2, *u.last().unwrap()), (0, 0, 0));
        let text = crate::text::decode_sjis(&u[12..]);
        assert!(text.starts_with("プレイヤー名\tTESTER  \r\n"));
        assert!(text.contains("スコア\t1234560\r\n"));
        assert!(text.contains("ゲームのバージョン\t1.00d\r\n"));
    }

    // 15 §3.6: spell practice fills the card name +0x7E; a changed config (normal game, E–L) or a recorded slow mode
    // skips the save.
    #[test]
    fn save_card_name_and_skips() {
        use crate::gamemgr::*;
        let mut g = Game::default();
        g.flags.set(F_PRACTICE | F_SPELL_PRACTICE);
        g.card = 5;
        g.score.catk[5].name[..4].copy_from_slice(b"Card");
        record_stage_start(&mut g, &Default::default());
        let r = Replay::parse(&build_save(&mut g, b"A       ", &mut Default::default()).unwrap())
            .unwrap();
        assert_eq!((r.spell_id(), r.card_name()), (5, &b"Card"[..]));
        assert!(crate::text::decode_sjis(&r.user).contains("カード名\tNo.  6 Card\r\n"));

        let mut g = Game::default();
        record_stage_start(&mut g, &Default::default());
        g.cfg.slow ^= 1;
        assert!(build_save(&mut g, b"A       ", &mut Default::default()).is_none());

        let mut g = Game::default();
        g.difficulty = 4;
        record_stage_start(&mut g, &Default::default());
        g.rpy.as_mut().unwrap().info[0xD9 - HEADER] = 1;
        assert!(build_save(&mut g, b"A       ", &mut Default::default()).is_none());
    }

    // 15 §3.3: Replay_FastForward gate (bit 2 set, bit 9 clear), dialogue test first, fast test only otherwise.
    #[test]
    fn fast_forward_order() {
        use crate::gamemgr::{F_CLEAR_PANEL, F_RUNNING};
        let no_boss = || false;
        assert!(!fast_forward(0, true, 2, 0, no_boss));
        assert!(!fast_forward(
            F_RUNNING | F_CLEAR_PANEL,
            true,
            2,
            0,
            no_boss
        ));
        // Dialogue: restart unless frame % 3 == 2; the boss test is not made then.
        assert!(fast_forward(F_RUNNING, true, 0, 0, || panic!(
            "boss tested"
        )));
        assert!(!fast_forward(F_RUNNING, true, 0, 2, no_boss));
        // frame 2: dialogue lets it through (2 % 3 == 2), fast restarts (2 % 5 ≠ 4); frame 29: neither.
        assert!(fast_forward(F_RUNNING, true, 2, 2, no_boss));
        assert!(!fast_forward(F_RUNNING, true, 2, 29, no_boss));
        assert!(fast_forward(F_RUNNING, false, 2, 3, no_boss));
        assert!(!fast_forward(F_RUNNING, false, 2, 4, no_boss));
        assert!(!fast_forward(F_RUNNING, false, 2, 3, || true));
        assert!(!fast_forward(F_RUNNING, false, 1, 3, no_boss));
    }

    // 15 §3.5: キャラ名 keeps its U+3000 padding, as in demo 0's USER text; stage ≥ 9 prints "Clear".
    #[test]
    fn user_chara_padding() {
        let mut g = crate::gamemgr::Game::default();
        g.stage = 9;
        record_stage_start(&mut g, &Default::default());
        let r = g.rpy.clone().unwrap();
        let l = user_lines(&g, &r, 0.0);
        assert_eq!(l[2], ("キャラ名", "霊夢＆紫　　　　".into()));
        assert_eq!(l[5], ("最終ステージ", "Clear".into()));
        let Some(ar) = crate::archive::test_archive() else {
            return;
        };
        crate::scoredat::set_ver_list(ar.read(crate::scoredat::VER_LIST_NAME).unwrap());
        let d = Replay::parse(&ar.read("demorpy0.rpy").unwrap()).unwrap();
        let text = crate::text::decode_sjis(&d.user);
        let want = crate::menulist::SHOT_NAMES[d.shot() as usize % 12];
        assert!(text.contains(&format!("キャラ名\t{want}\r\n")), "{text}");
    }

    // 15 §3.2: fps sample 0 is the carried fps_now (0 after boot), later samples 60; the next stage carries 60.
    #[test]
    fn fps_sample0_carried() {
        let mut g = crate::gamemgr::Game::default();
        assert_eq!(g.replay_fps, 0);
        record_stage_start(&mut g, &Default::default());
        let mut t = ReplayTask {
            play: false,
            stage: 0,
            cursor: 0,
            frame: 0,
            bonus_frames: 0,
            fps_q: 0,
            repeat_ctr: 0,
            debug: false,
            rng_snap: 0,
        };
        g.flags.set(crate::gamemgr::F_RUNNING);
        let mut input = crate::input::Input::default();
        for _ in 0..61 {
            t.record_game(&mut g, &mut input);
        }
        let s = g.rpy.as_ref().unwrap().stages[0].as_ref().unwrap();
        assert_eq!(s.fps, [0, 60, 60, 60]);
        assert_eq!(g.replay_fps, 60);
    }

    #[test]
    fn demos() {
        let Some(ar) = crate::archive::test_archive() else {
            return;
        };
        // Demo 0 has the listed 1.00a tag/size/sum, demos 1–3 the tag "debug": all pass ExeCheck (15 §2.10).
        crate::scoredat::set_ver_list(ar.read(crate::scoredat::VER_LIST_NAME).unwrap());
        let frames = [11083, 0, 0, 0];
        for n in 0..4 {
            let f = ar.read(&format!("demorpy{n}.rpy")).unwrap();
            let r = Replay::parse(&f).unwrap();
            let st = r.first_stage().unwrap();
            let s = r.stages[st].as_ref().unwrap();
            assert_eq!(s.inputs[0], 0, "dummy word");
            assert!(s.inputs.iter().all(|w| w & !0x1FFF == 0 || *w == 0xCDCD));
            if frames[n] != 0 {
                assert!(s.inputs.len() >= frames[n]);
            }
            let mut c = r.clone();
            let mut rng = crate::rng::Rng::seeded(3);
            let enc = c.encode(&mut rng);
            assert_eq!(Replay::parse(&enc).unwrap().stages, r.stages);
        }
    }
}
#[cfg(test)]
#[test]
fn dump_demos() {
    let Some(ar) = crate::archive::test_archive() else {
        return;
    };
    for n in 0..4 {
        let r = Replay::parse(&ar.read(&format!("demorpy{n}.rpy")).unwrap()).unwrap();
        let st = r.first_stage().unwrap();
        let s = r.stages[st].as_ref().unwrap();
        let mut s2 = s.clone();
        s2.inputs.truncate(8);
        s2.fps.truncate(4);
        eprintln!(
            "demo{n} shot {} diff {} stage {st} name {:?} date {:?} score {} n_in {} n_fps {} cfg {:02x?} {:?}",
            r.shot(),
            r.difficulty(),
            r.name(),
            r.date(),
            r.score(),
            s.inputs.len(),
            s.fps.len(),
            r.config(),
            s2
        );
        eprintln!("tail {:x?}", &s.inputs[s.inputs.len() - 4..]);
    }
}
