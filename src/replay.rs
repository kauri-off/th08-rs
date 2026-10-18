// Replays (15 §3, 11 §6): .rpy load/save, the per-stage records, playback and recording.
use crate::archive::{lzss_compress, lzss_decompress};
use anyhow::{Result, bail};

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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StageRecord {
    pub score: u32,
    pub point_items: i32,
    pub graze: u32,
    pub extends: i32,
    pub next_extend: i32,
    pub max_point: i32,
    pub gauge: i16,
    pub seed: u16,
    pub power: u8,
    pub lives: u8,
    pub bombs: u8,
    pub rank: u8,
    pub b20: u8,
    pub spells: u8,
    pub clock: i8,
    // Input words from the dummy word on; fps bytes as stored.
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
    pub debug: u8,
    // Info block 0x68..0x134 of the image (date, name, card, config copy, slowdown, …).
    pub info: Vec<u8>,
    pub stages: [Option<StageRecord>; 9],
    pub user: Vec<u8>,
}

impl Default for Replay {
    fn default() -> Self {
        let mut info = vec![0; INFO - HEADER];
        info[1] = 100;
        put_u32(&mut info, 0x120 - HEADER, 30);
        info[0x12C - HEADER..0x131 - HEADER].copy_from_slice(b"0100d");
        Self {
            debug: 0,
            info,
            stages: Default::default(),
            user: Vec::new(),
        }
    }
}

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

    // Replay_Load (15 §3.7).
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

    // Replay_Finish image/compress/checksum/encrypt (15 §3.5); 3 RNG draws.
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

pub const PLAY_PRIO: u16 = 6;
pub const RECORD_PRIO: u16 = 17;
pub const FF_PRIO: u16 = 18;

// Runtime replay object (15 §3.1): mode 0 records, mode 1 plays back. The data lives in `Game::rpy`.
pub struct ReplayTask {
    play: bool,
    stage: usize,
    cursor: usize,
    frame: u32,
    bonus_frames: u32,
    fps_q: usize,
    repeat_ctr: u32,
    ff: u32,
}

pub fn register(
    ctx: &mut crate::game::Ctx,
    play: bool,
    st: &crate::player::Stats,
) -> anyhow::Result<crate::task::TaskRef<crate::game::Ctx>> {
    use crate::task::{self, Chain};
    ctx.input.game = 0;
    ctx.input.game_prev = 0;
    let t = task::task(ReplayTask {
        play,
        stage: ctx.game.stage % 9,
        cursor: 0,
        frame: 0,
        bonus_frames: 0,
        fps_q: 0,
        repeat_ctr: 0,
        ff: 0,
    });
    if play {
        task::add(ctx, Chain::Calc, PLAY_PRIO, 0, t.clone())?;
        task::add(ctx, Chain::Calc, FF_PRIO, 1, t.clone())?;
    } else {
        record_stage_start(&mut ctx.game, st);
        task::add(ctx, Chain::Calc, RECORD_PRIO, 0, t.clone())?;
    }
    Ok(t)
}

// Replay_RecordStageStart: the previous record gets the current score, then a new record with the dummy word.
fn record_stage_start(g: &mut crate::gamemgr::Game, st: &crate::player::Stats) {
    let stage = g.stage % 9;
    if g.rpy.is_none() {
        g.rpy = Some(new_recording(
            g.shot,
            g.difficulty,
            g.flags,
            g.card,
            &g.cfg_bytes(),
        ));
    }
    let r = g.rpy.as_mut().unwrap();
    for s in r.stages.iter_mut().flatten() {
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
        b20: 0,
        spells: st.spells as u8,
        clock: st.clock as i8,
        inputs: vec![0],
        fps: Vec::new(),
    });
}

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
    g.flags = g.flags & (7 << 15) | F_REPLAY;
    let id = r.spell_id();
    if id >= 0 {
        g.flags |= F_SPELL_PRACTICE | F_PRACTICE;
        g.card = id;
    }
    if let Some(n) = demo {
        g.flags |= F_DEMO;
        g.demo = n;
        g.demo_frame = 0;
        g.demo_shot_bias = r.shot() % 2;
    } else {
        g.demo_shot_bias = 0;
    }
    if g.cfg_live.is_none() {
        g.cfg_live = Some(g.cfg);
    }
    g.cfg.apply_gameplay(r.config());
    g.rpy = Some(r);
    g.fresh_start = true;
    g.request(SCENE_GAME);
    true
}

// Back to the menu after playback: the live config returns, replay state is dropped.
pub fn end_playback(g: &mut crate::gamemgr::Game) {
    if let Some(c) = g.cfg_live.take() {
        g.cfg = c;
    }
    if g.replay() {
        g.rpy = None;
        g.watched_replay = g.flags & crate::gamemgr::F_DEMO == 0;
    }
    g.flags &= !(crate::gamemgr::F_REPLAY
        | crate::gamemgr::F_DEMO
        | crate::gamemgr::F_PRACTICE
        | crate::gamemgr::F_SPELL_PRACTICE);
}

// Replay_PlayStageLoad (11 §3.4): stats, rank and seed from the record of the stage being started.
// SPEC-GAP: record +0 is the score at the end of a stage, so the start score is taken from the previous record (0 if none).
pub fn play_stage_load(
    g: &mut crate::gamemgr::Game,
    st: &mut crate::player::Stats,
    rng: &mut crate::rng::Rng,
) -> Result<()> {
    let stage = g.stage % 9;
    let Some(r) = g.rpy.as_ref() else {
        bail!("no replay loaded")
    };
    let Some(rec) = r.stages[stage].as_ref() else {
        bail!("replay has no record for stage {stage}")
    };
    let score = r.stages[..stage]
        .iter()
        .flatten()
        .last()
        .map_or(0, |p| p.score);
    st.score10 = score as i64;
    st.score_disp = score as i64;
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
    rng.state = rec.seed;
    Ok(())
}

// Replay_AppendEnd + Replay_Finish (scene changes out of the game): last score, terminator, date; then the save.
// SPEC-GAP: the save dialogue (16 / 15 §3.6 states 10–14) is not implemented; `TH08_REPLAY_SAVE=<1–15>` saves there.
pub fn finish(ctx: &mut crate::game::Ctx) {
    let Some(mut r) = take_recording(&mut ctx.game) else {
        return;
    };
    let Some(slot) = crate::env::var("TH08_REPLAY_SAVE").and_then(|v| v.parse::<u32>().ok()) else {
        return;
    };
    let name = ctx.game.score.lsnm;
    r.info[0x72 - HEADER..0x7B - HEADER].fill(0);
    r.info[0x72 - HEADER..0x7A - HEADER].copy_from_slice(&name[..8]);
    let bytes = r.encode(&mut ctx.anm.rng);
    let path = format!("replay/th8_{:02}.rpy", slot.clamp(1, 15));
    crate::scoredat::storage_write(&path, &bytes);
    log::info!("replay saved to {path} ({} bytes)", bytes.len());
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
        s.inputs.extend([0, 0xCDCD, 0xCDCD]);
    }
    put_u32(&mut r.info, 0xB0 - HEADER, score);
    let date = crate::supervisor::date_md();
    let d = &mut r.info[0x6C - HEADER..0x72 - HEADER];
    d.fill(0);
    d[..date.len().min(5)].copy_from_slice(&date.as_bytes()[..date.len().min(5)]);
    Some(r)
}

fn boss_active() -> bool {
    crate::gamemgr::current()
        .and_then(|g| g.borrow().enemies.clone())
        .is_some_and(|e| e.borrow().bosses.iter().any(Option::is_some))
}

impl ReplayTask {
    // Replay_RecordCalc (15 §3.2).
    fn record(&mut self, ctx: &mut crate::game::Ctx) {
        let g = &mut ctx.game;
        if g.flags & crate::gamemgr::F_RUNNING == 0 {
            return;
        }
        let i = &mut ctx.input;
        i.game_prev = i.game;
        i.game = i.held;
        if g.cfg.slow != 0 {
            return;
        }
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
            s.fps.resize(q + 2, 0);
            s.fps[q] = 60;
            s.fps[q + 1] = 60;
            self.fps_q += 1;
        }
        self.frame += 1;
    }

    // Replay_PlayCalc (15 §3.3): input substitution and the key-repeat emulation.
    fn play(&mut self, ctx: &mut crate::game::Ctx) {
        let g = &ctx.game;
        if g.flags & crate::gamemgr::F_RUNNING == 0
            || g.cfg.slow != 0
            || g.flags & crate::gamemgr::F_CLEAR_PANEL != 0
        {
            return;
        }
        let w = g
            .rpy
            .as_ref()
            .and_then(|r| r.stages[self.stage].as_ref())
            .and_then(|s| s.inputs.get(self.cursor).copied())
            .unwrap_or(0);
        self.cursor += 1;
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
            self.fps_q += 1;
        }
        self.frame += 1;
    }
}

impl crate::task::Task<crate::game::Ctx> for ReplayTask {
    fn calc(&mut self, ctx: &mut crate::game::Ctx, tag: u8) -> crate::task::Ret {
        use crate::task::Ret;
        match (self.play, tag) {
            (false, _) => self.record(ctx),
            (true, 0) => self.play(ctx),
            // While skippable dialogue is shown the chain is restarted on 2 of every 3 frames (15 §3.3).
            (true, _) => {
                // Fast playback: restart on every frame except frame % 5 == 4 while no boss is active.
                if ctx.game.replay_ff
                    && ctx.game.replay_speed == 2
                    && self.frame % 5 != 4
                    && !boss_active()
                {
                    return Ret::Restart;
                }
                if ctx.game.replay_ff && ctx.game.dialogue_skippable {
                    self.ff += 1;
                    if self.ff % 3 != 0 {
                        return Ret::Restart;
                    }
                } else {
                    self.ff = 0;
                }
            }
        }
        Ret::Next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demos() {
        let Some(ar) = crate::archive::test_archive() else {
            return;
        };
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
