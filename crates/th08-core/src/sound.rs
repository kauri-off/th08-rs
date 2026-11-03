//! Sound manager (13 §1.1–§1.3): SE queue (`Sound_Play` 0x45D550, `Sound_PlayAt` 0x45D660, flushed by `Sound_Update`
//! 0x45D790) and the BGM state of `Music_*` / `Bgm_*` (02 §4–§5), which emit `SoundCmd`s for the audio back end.

pub use th08_formats::thbgm::Track;

// Sound effect ids (13 §1.1).
pub const PLST: i32 = 0;
pub const ENEP0: i32 = 2;
pub const ENEP1: i32 = 3;
pub const PLDEAD: i32 = 4;
pub const TAN00: i32 = 7;
pub const OK: i32 = 10;
pub const CANCEL: i32 = 11;
pub const SELECT: i32 = 12;
pub const CAT: i32 = 14;
pub const TAN00_LOUD: i32 = 15;
pub const DAMAGE0: i32 = 20;
pub const ITEM0: i32 = 21;
pub const KIRA0: i32 = 25;
pub const EXTEND: i32 = 28;
pub const TIMEOUT: i32 = 29;
pub const GRAZE: i32 = 30;
pub const POWERUP: i32 = 31;
pub const PAUSE: i32 = 34;
pub const CARDGET: i32 = 35;
pub const OPTION: i32 = 36;
pub const DAMAGE1: i32 = 37;
pub const TIMEOUT2: i32 = 38;
pub const OPSHOW: i32 = 39;
pub const OPHIDE: i32 = 40;
pub const INVALID: i32 = 41;
pub const ITEM1: i32 = 44;
// se_ok00 at −100: ECL op 181 (clock +1, 13 §1.4).
pub const OK_CLOCK: i32 = 45;

// SE table 0x4C8040 (13 §1.1): wave file, volume in 1/100 dB; the unused priority word is not kept.
pub const TABLE: [(&str, i16); 46] = [
    ("se_plst00", -1900),
    ("se_plst00", -2100),
    ("se_enep00", -1200),
    ("se_enep00", -1500),
    ("se_pldead00", -1100),
    ("se_power0", -700),
    ("se_power1", -700),
    ("se_tan00", -1900),
    ("se_tan01", -2200),
    ("se_tan02", -2400),
    ("se_ok00", -1100),
    ("se_cancel00", -1100),
    ("se_select00", -1500),
    ("se_gun00", -1500),
    ("se_cat00", -1000),
    ("se_tan00", -1100),
    ("se_lazer00", -1300),
    ("se_lazer01", -1400),
    ("se_enep01", -900),
    ("se_nep00", -400),
    ("se_damage00", -880),
    ("se_item00", -1500),
    ("se_tan00", -300),
    ("se_tan01", -1800),
    ("se_tan02", -1800),
    ("se_kira00", -1100),
    ("se_kira01", -1300),
    ("se_kira02", -1500),
    ("se_extend", -500),
    ("se_timeout", -500),
    ("se_graze", -1100),
    ("se_powerup", -800),
    ("se_graze", -1200),
    ("se_kira00", -500),
    ("se_pause", -800),
    ("se_cardget", -800),
    ("se_option", -800),
    ("se_damage01", -700),
    ("se_timeout2", -300),
    ("se_opshow", -800),
    ("se_ophide", -800),
    ("se_invalid", -200),
    ("se_slash", 0),
    ("se_slash", -600),
    ("se_item01", -800),
    ("se_ok00", -100),
];

const MAX_IDS: usize = 12;
const MAX_PANS: usize = 128;

// Per-frame queue (13 §1.2): up to 12 distinct ids (manager +0x620), each with up to 128 pan values (+0x680);
// a 13th distinct id in one frame is dropped, so each id sounds at most once per frame.
#[derive(Clone, Default, Debug)]
pub struct Queue {
    slots: Vec<(i32, Vec<i32>)>,
}

impl Queue {
    pub fn push(&mut self, id: i32, pan: i32) {
        // No range check in the original, but no caller passes an invalid id (13 §1.2); this guard is never taken.
        if !(0..TABLE.len() as i32).contains(&id) {
            return;
        }
        if let Some((_, pans)) = self.slots.iter_mut().find(|(i, _)| *i == id) {
            if pans.len() < MAX_PANS {
                pans.push(pan);
            }
        } else if self.slots.len() < MAX_IDS {
            self.slots.push((id, vec![pan]));
        }
    }

    // Sound_Update: (id, integer average pan) in queue order; the queue is cleared.
    pub fn drain(&mut self) -> Vec<(i32, i32)> {
        self.slots
            .drain(..)
            .map(|(id, p)| (id, p.iter().sum::<i32>() / p.len() as i32))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

impl Queue {
    // Sound_Play(id, pan); every call site passes pan 0 (13 §1.2).
    pub fn play(&mut self, id: i32) {
        self.push(id, 0);
    }

    // Sound_PlayAt(id, x): pan = trunc((x − 192)·1000/192), −1000 at x = 0 … +1000 at x = 384 (13 §1.2).
    pub fn play_at(&mut self, id: i32, x: f32) {
        self.push(id, ((x - 192.0) * 1000.0 / 192.0) as i32);
    }
}

// One device call of the frame, executed in order by `audio::Audio::exec` (ARCH_A §2.2 `Outputs.sound`). Track
// lookup, slots, music-mode routing and the BGM fade state machine are resolved in `Bgm` before a command is made.
#[derive(Clone, Debug)]
pub enum SoundCmd {
    // Sound_Update per flushed id (13 §1.3): pan and volume in 1/100 dB.
    Se { id: usize, pan: i32, vol: i32 },
    Wav(WavCmd),
    Midi(MidiCmd),
}

// Music mode 1 streaming object (02 §3–§4).
#[derive(Clone, Debug)]
pub enum WavCmd {
    // Play command: rewind, fill the ring from the stored track and play (02 §3).
    Play(Track),
    // `Bgm_StreamStop` 0x4728E0: stop, position 0; the object and ring stay.
    Stop,
    // `Stop` with the position kept (true: command 6, mode-1 fade end) / `Play` at the position (false: command 7).
    Pause(bool),
    // Sound manager release on restart (00 §2): the streaming object is gone.
    Release,
    // `Bgm_SetVolume` 0x472840 after the BGM-volume curve (1/100 dB).
    Volume(i32),
}

// Music mode 2 MIDI object (02 §5); slot data, parsing and the open slot live in the back end.
#[derive(Clone, Debug)]
pub enum MidiCmd {
    // `Midi_LoadSlot(slot, name)`.
    Load { slot: usize, name: String },
    // `Midi_Stop`, `Midi_Open(slot)`, `Midi_Play`.
    PlaySlot(usize),
    // `Midi_Stop`, `Midi_PlayFileOnce(name)` (slot 31), `Midi_Play`.
    PlayFile(String),
    Stop,
    // init.mid reset: `Midi_Stop`, `Midi_Open(30)`, `Midi_Play`.
    Reset,
    // `Midi_Fade(ms)`.
    Fade(u32),
}

// Streaming-object fade (02 §4): `mode` 1 `Music_Fade`, 2 `Bgm_StartFadeIn`, 3 `Bgm_StartFadeInFromDip`,
// 4 `Bgm_StartDip`; `n` counts down from N = trunc(secs·60) once per Supervisor calc.
#[derive(Clone, Debug)]
pub(crate) struct Fade {
    mode: u8,
    n: i32,
    total: i32,
}

// One fade tick: a new attenuation (1/100 dB), the end (mode 0, last value stays), or stop the buffer (mode 1).
#[derive(Debug, PartialEq)]
pub(crate) enum FadeTick {
    Volume(i32),
    End,
    Stop,
}

impl Fade {
    pub(crate) fn new(mode: u8, secs: f32) -> Self {
        let total = (secs * 60.0) as i32;
        Self {
            mode,
            n: total,
            total,
        }
    }

    // `Sound_TickBgmFades` 0x447764: n −= 1, n < 1 ends; p is linear in dB (integer division).
    pub(crate) fn tick(&mut self) -> FadeTick {
        self.n -= 1;
        let (n, t) = (self.n, self.total);
        match self.mode {
            _ if n >= 1 => FadeTick::Volume(match self.mode {
                1 => n * 5000 / t - 5000,
                2 => -(n * 5000 / t),
                3 => -(n * 1000 / t),
                _ => n * 1000 / t - 1000,
            }),
            1 => FadeTick::Stop,
            _ => FadeTick::End,
        }
    }
}

// Effective music path: mode 2 without a usable synth falls back to the WAV track (port).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Path {
    Off,
    Wav,
    Midi,
}

// Music_Play's .mid → .wav rename plus Bgm_FindTrack 0x45C6B0 (02 §4): basename, case-sensitive compare,
// no match → track 0 (th08_01.wav). So the std typos th08_02 / th08_13_b play the title theme.
fn find_track<'a>(tracks: &'a [Track], name: &str) -> Option<&'a Track> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let wav = match base.rfind('.') {
        Some(i) => format!("{}.wav", &base[..i]),
        None => base.to_string(),
    };
    tracks.iter().find(|t| t.name == wav).or(tracks.first())
}

// The sound manager's BGM side (02 §4–§5): music mode, BGM volume, fade, WAV slots; every device effect is pushed
// to `out` (SE flushes too, 13 §1.3) and drained by `Ctx::flush_sound` in call order.
#[derive(Clone, Debug)]
pub struct Bgm {
    // thbgm.fmt tracks (02 §2).
    tracks: Vec<Track>,
    // BGM volume cfg +0x27 (0..100) and the current fade attenuation p (1/100 dB, 0 = none), 02 §4.
    pub(crate) bgm_volume: u8,
    pub(crate) fade_p: i32,
    // Sound command 5 queued by Music_Fade (whole seconds), started by Sound_Update.
    pub(crate) fade_req: Option<i32>,
    pub(crate) fade: Option<Fade>,
    // Music slots 0..15 (02 §3 preload): the name queued by sound command 1 (`Music_Play`), started by `play_slot`.
    pub(crate) slots: [Option<String>; 16],
    // Music mode cfg +0x1F (02 §4): 0 off, 1 WAV, 2 MIDI.
    music: u8,
    // Host input: the back end has a MIDI synth (SoundFont found; none on the web) — else mode 2 takes the WAV path.
    pub midi_ready: bool,
    pub out: Vec<SoundCmd>,
}

impl Default for Bgm {
    fn default() -> Self {
        Self::new(&[])
    }
}

impl Bgm {
    pub fn new(fmt: &[u8]) -> Self {
        Self {
            tracks: th08_formats::thbgm::parse_fmt(fmt),
            bgm_volume: 100,
            fade_p: 0,
            fade_req: None,
            fade: None,
            slots: Default::default(),
            music: 1,
            midi_ready: false,
            out: Vec::new(),
        }
    }

    pub(crate) fn path(&self) -> Path {
        match self.music {
            0 => Path::Off,
            2 if self.midi_ready => Path::Midi,
            _ => Path::Wav,
        }
    }

    fn wav(&mut self, c: WavCmd) {
        self.out.push(SoundCmd::Wav(c));
    }

    fn midi(&mut self, c: MidiCmd) {
        self.out.push(SoundCmd::Midi(c));
    }

    // Sound_Update per flushed id (13 §1.3).
    pub fn play_se(&mut self, id: usize, pan: i32, vol: i32) {
        self.out.push(SoundCmd::Se { id, pan, vol });
    }

    // init.mid reset (02 §5): `Midi_Stop`, `Midi_Open(30)`, `Midi_Play` in mode 2 at main-menu Quit, GameMgr_OnRemove
    // after its Music_Stop, the Supervisor teardown and the option BGM item's Left from mode 2 (15 §1.3).
    pub fn midi_reset(&mut self) {
        if self.path() == Path::Midi {
            self.midi(MidiCmd::Reset);
        }
    }

    // Music_Play + Music_PlaySlot / Music_PlayFile (02 §4) in one step; music mode 0 plays nothing. The music-room
    // heard flags are set by callers. MIDI: `Midi_Stop`, `Midi_PlayFileOnce(name)` (slot 31), `Midi_Play` (02 §5).
    // WAV: loads the stored part (never past loop_end) and plays; script names like "bgm/th08_00.mid" are renamed and
    // unknown names fall back to track 0.
    pub fn play_bgm(&mut self, name: &str) {
        match self.path() {
            Path::Off => {}
            Path::Wav => self.play_wav(name),
            // MIDI calls only; a WAV buffer is not touched (02 §4/§5).
            Path::Midi => self.midi(MidiCmd::PlayFile(name.to_owned())),
        }
    }

    // WAV paths never call a MIDI function: after option Reset from mode 2 the MIDI song keeps playing (15 §1.3).
    fn play_wav(&mut self, name: &str) {
        let Some(t) = find_track(&self.tracks, name).cloned() else {
            log::warn!("no bgm {name}");
            return;
        };
        // Sound_Update play stages 0 (stop: fade mode 0) and 4 (`Bgm_SetVolume(0)`), 02 §4 (RE-Q30); done at once here.
        (self.fade_req, self.fade, self.fade_p) = (None, None, 0);
        self.apply_volume();
        self.wav(WavCmd::Play(t));
    }

    // Music_Play(slot, name) 0x447D56 (02 §4): WAV sound command 1 only prepares the slot, nothing is heard; MIDI
    // `Midi_LoadSlot(slot, name)`.
    pub fn load_bgm(&mut self, slot: usize, name: &str) {
        match self.path() {
            Path::Off => {}
            Path::Wav => {
                if let Some(s) = self.slots.get_mut(slot) {
                    *s = Some(name.to_owned());
                }
            }
            Path::Midi => self.midi(MidiCmd::Load {
                slot,
                name: name.to_owned(),
            }),
        }
    }

    // Music_PlaySlot 0x447E47 (02 §4): WAV command 2 starts a loaded slot; MIDI `Midi_Stop`, `Midi_Open(slot)`,
    // `Midi_Play`. The music-room heard flags are set by callers.
    pub fn play_slot(&mut self, slot: usize) {
        match self.path() {
            Path::Off => {}
            Path::Wav => match self.slots.get(slot).cloned().flatten() {
                Some(name) => self.play_wav(&name),
                None => log::warn!("bgm slot {slot} not loaded"),
            },
            Path::Midi => self.midi(MidiCmd::PlaySlot(slot)),
        }
    }

    // Sound commands 6 "Pause" (`Stop`, position kept) / 7 "UnPause" (`Play` at the position, no volume write) act in
    // music mode 1 only; the MIDI song keeps playing (02 §4). The fade is not touched.
    pub fn pause_bgm(&mut self, on: bool) {
        if self.path() == Path::Wav {
            self.wav(WavCmd::Pause(on));
        }
    }

    // Music_Fade 0x4480F8 (10 §5 op 12, 11 §4, 16 §1.3 `@M`), 02 §4: MIDI `Midi_Fade(trunc(1000·secs))` ms, no
    // game-speed division. WAV: secs / `speed` (Supervisor+0x188) when 0 < speed ≤ 1, then command 5 with trunc(secs):
    // linear in dB from 0 to −50 dB over trunc(secs)·60 frames, then stop. Music mode 0 queues nothing.
    pub fn fade_bgm(&mut self, secs: f32, speed: f32) {
        match self.path() {
            Path::Off => {}
            Path::Wav => {
                let secs = if speed != 0.0 && speed <= 1.0 {
                    secs / speed
                } else {
                    secs
                };
                self.fade_req = Some(secs as i32);
            }
            Path::Midi => self.midi(MidiCmd::Fade((1000.0 * secs) as u32)),
        }
    }

    // Sound_Update (13 §1.3) handling command 5: mode 1, N = n = trunc(secs·60); no volume write yet.
    pub fn start_fade(&mut self) {
        if let Some(secs) = self.fade_req.take() {
            self.fade = Some(Fade::new(1, secs as f32));
        }
    }

    // `Bgm_StartFadeIn` 0x406AC0 (02 §4): immediate, mode 2, volume −10000 at once (−50 dB → ≈ 0).
    pub fn start_fade_in(&mut self, secs: f32) {
        self.fade = Some(Fade::new(2, secs));
        self.set_fade_p(-10000);
    }

    // `Bgm_StartDip` 0x406B40 (02 §4): mode 4, no initial write (0 → −10 dB); the music keeps playing.
    pub fn start_dip(&mut self, secs: f32) {
        self.fade = Some(Fade::new(4, secs));
    }

    // `Bgm_StartFadeInFromDip` 0x406BB0 (02 §4): mode 3, volume −1000 first (−10 dB → ≈ 0).
    pub fn start_fade_in_from_dip(&mut self, secs: f32) {
        self.fade = Some(Fade::new(3, secs));
        self.set_fade_p(-1000);
    }

    // 0x447764, every Supervisor calc before the input poll, also while paused (02 §4): fade tick; mode 1 `Stop`s the
    // buffer at the end with its position and last volume kept, so a later UnPause resumes it near-silent.
    pub fn tick_fade(&mut self) {
        let Some(f) = &mut self.fade else { return };
        match f.tick() {
            FadeTick::Volume(p) => self.set_fade_p(p),
            FadeTick::End => self.fade = None,
            FadeTick::Stop => {
                self.fade = None;
                self.wav(WavCmd::Pause(true));
            }
        }
    }

    // `Bgm_SetVolume` 0x472840: every write goes through the BGM-volume scaling (02 §4).
    fn set_fade_p(&mut self, p: i32) {
        self.fade_p = p;
        self.apply_volume();
    }

    // Music mode cfg +0x1F, and the BGM volume through the square curve of 0x472840 (02 §4) with cfg +0x27; SE
    // volumes are applied per play.
    pub fn set_volumes(&mut self, cfg: &crate::config::Config) {
        self.music = cfg.music;
        self.bgm_volume = cfg.bgm_volume;
        self.apply_volume();
    }

    fn apply_volume(&mut self) {
        let db = crate::config::bgm_volume_curve(self.fade_p, self.bgm_volume);
        self.wav(WavCmd::Volume(db));
    }

    // Music_Stop 0x448081 (02 §4): mode 2 `Midi_Stop`; mode 1 command 3 `Bgm_StreamStop` (buffer stopped, fade mode 0,
    // so a queued command 5 ahead of it is cancelled too); mode 0 nothing.
    pub fn stop_bgm(&mut self) {
        match self.path() {
            Path::Off => {}
            Path::Wav => {
                (self.fade_req, self.fade) = (None, None);
                self.wav(WavCmd::Stop);
            }
            Path::Midi => self.midi(MidiCmd::Stop),
        }
    }

    // Sound manager release on restart (00 §2): the streaming object is gone; nothing is left to UnPause.
    pub fn release_bgm(&mut self) {
        self.wav(WavCmd::Release);
    }
}

// Test / tool text of a recorded `SoundCmd` stream: one "calc-frame command" line per executed command.
pub fn format_log(log: &[(u64, SoundCmd)]) -> String {
    log.iter().map(|(f, c)| format!("{f} {c:?}\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_dedup_and_pan() {
        let mut q = Queue::default();
        q.push(30, 100);
        q.push(30, -300);
        q.push(2, 0);
        for id in 3..20 {
            q.push(id, 0);
        }
        q.push(30, 0);
        q.push(99, 0);
        let d = q.drain();
        assert_eq!(d.len(), 12);
        assert_eq!(d[0], (30, (100 - 300) / 3));
        assert_eq!(d[1], (2, 0));
        assert_eq!(d[11].0, 12);
        assert!(q.is_empty());
        for _ in 0..200 {
            q.push(5, 1000);
        }
        q.push(5, -1000);
        assert_eq!(q.drain(), vec![(5, 1000)]);
    }

    #[test]
    fn pan_from_x() {
        let mut q = Queue::default();
        q.play_at(4, 384.0);
        q.play_at(5, 0.0);
        q.play_at(6, 192.0);
        assert_eq!(q.drain(), vec![(4, 1000), (5, -1000), (6, 0)]);
    }

    #[test]
    fn music_off_plays_nothing() {
        // cfg +0x1F = 0 (02 §4): no slot is loaded and no fade is queued; mode 1 loads.
        let mut a = Bgm::default();
        let mut cfg = crate::config::Config::default();
        cfg.music = 0;
        a.set_volumes(&cfg);
        a.load_bgm(0, "bgm/th08_00.mid");
        a.fade_bgm(4.0, 1.0);
        assert!(a.slots[0].is_none() && a.fade_req.is_none());
        cfg.music = 1;
        a.set_volumes(&cfg);
        a.load_bgm(0, "bgm/th08_00.mid");
        a.fade_bgm(4.0, 1.0);
        assert!(a.slots[0].is_some() && a.fade_req == Some(4));
    }

    // 02 §4: WAV fades divide by a game speed in (0, 1]; MIDI mode without a SoundFont takes the WAV path.
    #[test]
    fn wav_fade_speed_and_midi_fallback() {
        let mut a = Bgm::default();
        a.fade_bgm(4.0, 0.5);
        assert_eq!(a.fade_req, Some(8));
        a.fade_bgm(4.0, 2.0);
        assert_eq!(a.fade_req, Some(4));
        let mut cfg = crate::config::Config::default();
        cfg.music = 2;
        a.set_volumes(&cfg);
        a.out.clear();
        a.load_bgm(0, "bgm/th08_00.mid");
        assert!(a.slots[0].is_some() && a.out.is_empty());
    }

    #[test]
    fn fade_linear_in_db() {
        // Music_Fade(4): N = 240; first tick n = 239, middle n = 120, last audible n = 1, then stop.
        let mut f = Fade::new(1, 4.0);
        let p = ticks(&mut f);
        assert_eq!(p.len(), 239);
        assert_eq!([p[0], p[119], p[238]], [-21, -2500, -4980]);
        let bgm = |p, v| crate::config::bgm_volume_curve(p, v);
        assert_eq!(
            [bgm(p[0], 100), bgm(p[119], 100), bgm(p[238], 100)],
            [-21, -2500, -4980]
        );
        // v = 50 (k = 0.75): trunc((p + 5000)·0.75) − 5000.
        assert_eq!(
            [bgm(p[0], 50), bgm(p[119], 50), bgm(p[238], 50)],
            [-1266, -3125, -4985]
        );
        assert_eq!(Fade::new(1, 0.0).tick(), FadeTick::Stop);
    }

    fn ticks(f: &mut Fade) -> Vec<i32> {
        std::iter::from_fn(|| match f.tick() {
            FadeTick::Volume(p) => Some(p),
            _ => None,
        })
        .collect()
    }

    // 02 §4 modes 2/3/4: last values −41 (N 120), −16 and −984 (N 60), then mode 0 with no write.
    #[test]
    fn fade_in_and_dip() {
        let mut f = Fade::new(2, 2.0);
        let p = ticks(&mut f);
        assert_eq!((p.len(), p[0], p[118]), (119, -4958, -41));
        assert_eq!(f.tick(), FadeTick::End);
        let p = ticks(&mut Fade::new(3, 1.0));
        assert_eq!((p.len(), p[0], p[58]), (59, -983, -16));
        let p = ticks(&mut Fade::new(4, 1.0));
        assert_eq!((p.len(), p[0], p[58]), (59, -17, -984));

        let mut a = Bgm::default();
        a.start_fade_in(2.0);
        assert_eq!(a.fade_p, -10000);
        (0..119).for_each(|_| a.tick_fade());
        assert_eq!(a.fade_p, -41);
        a.tick_fade();
        assert!(a.fade.is_none() && a.fade_p == -41);
        a.start_dip(1.0);
        assert_eq!(a.fade_p, -41);
        a.tick_fade();
        assert_eq!(a.fade_p, -17);
        a.start_fade_in_from_dip(1.0);
        assert_eq!(a.fade_p, -1000);
        // Every attenuation write is one Volume command; the mode-2 end writes nothing.
        let vols = a
            .out
            .iter()
            .filter(|c| matches!(c, SoundCmd::Wav(WavCmd::Volume(_))))
            .count();
        assert_eq!((vols, a.out.len()), (1 + 119 + 1 + 1, 122));
    }

    // Bgm side of audio.rs `midi_mode_calls` (02 §5): the MIDI path fills no WAV slot and queues no WAV fade.
    #[test]
    fn midi_mode_bgm_side() {
        let mut b = Bgm::default();
        b.midi_ready = true;
        let mut cfg = crate::config::Config::default();
        cfg.music = 2;
        b.set_volumes(&cfg);
        b.load_bgm(1, "bgm/th08_03.mid");
        assert!(b.slots[1].is_none());
        b.out.clear();
        b.fade_bgm(3.0, 0.5);
        assert!(b.fade_req.is_none());
        assert!(matches!(b.out[..], [SoundCmd::Midi(MidiCmd::Fade(3000))]));
    }

    // Bgm side of audio.rs `wav_pause_and_fade` (02 §4): command 6 keeps the fade ticking to its end; Music_Stop
    // clears a running fade and a queued one.
    #[test]
    fn wav_pause_bgm_side() {
        let mut b = Bgm::default();
        b.fade_bgm(4.0, 1.0);
        b.start_fade();
        b.pause_bgm(true);
        (0..240).for_each(|_| b.tick_fade());
        assert!(b.fade.is_none() && b.fade_p == -4980);
        b.pause_bgm(false);
        b.fade_bgm(4.0, 1.0);
        b.start_fade();
        b.stop_bgm();
        assert!(b.fade.is_none());
        b.fade_bgm(4.0, 1.0);
        b.stop_bgm();
        assert!(b.fade_req.is_none());
    }
}
