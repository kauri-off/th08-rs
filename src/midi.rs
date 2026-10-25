//! MIDI music mode: the original's own SMF sequencer (02 §5, `Midi_*` 0x443C60–0x445440) and the port's synth output.

use anyhow::{Context, Result, ensure};
use std::sync::Arc;

// Midi_Open's tempo := 1 000 000 µs per quarter note (02 §5).
const OPEN_TEMPO: u32 = 1_000_000;

// Where the sequencer's channel messages and SysEx go (winmm `midiOutShortMsg` / `midiOutLongMsg` in the original).
pub trait Out {
    fn short(&mut self, msg: [u8; 3]);
    // SysEx body after the 0xF0 byte, as stored in the file.
    fn sysex(&mut self, data: &[u8]);
}

// Midi_Open 0x4442E0: header fields and every `MTrk` body copied into its own buffer.
#[derive(Clone, Debug)]
pub struct Song {
    pub format: u16,
    pub division: u16,
    pub tracks: Vec<Arc<[u8]>>,
}

pub fn parse(file: &[u8]) -> Result<Song> {
    let (header, tracks) = midly::parse(file).context("not a MIDI file")?;
    let midly::Timing::Metrical(division) = header.timing else {
        anyhow::bail!("SMPTE timing is not used by th08");
    };
    let format = match header.format {
        midly::Format::SingleTrack => 0,
        midly::Format::Parallel => 1,
        midly::Format::Sequential => 2,
    };
    let tracks = tracks
        .map(|t| t.map(|e| Arc::<[u8]>::from(e.unread())))
        .collect::<Result<Vec<_>, _>>()?;
    ensure!(division.as_int() > 0, "division 0");
    Ok(Song {
        format,
        division: division.as_int(),
        tracks,
    })
}

// Track record (0x20 bytes at +0x138): +0 active, +4 next event time, +0xC running status, +0x14 read pointer (at
// the next event's status byte, its delta already added to `next`), +0x18/+0x1C CC 2 copies of +0x14/+4.
#[derive(Clone, Debug, Default)]
struct Track {
    active: bool,
    next: u64,
    running: u8,
    pos: usize,
    loop_pos: usize,
    loop_next: u64,
}

// The MIDI object (02 §5 "Object"), one per run: +0x2CC and the raw CC 7 bytes carry over between songs. The
// per-channel bookkeeping other than CC 7 (note bitmap, program, bank, pan, CC 91/93) has no reader in 02 §5.
pub struct Sequencer {
    // Parsed tracks of the open file; none after `Midi_Open` of a null slot (+0x138 = 0).
    song: Song,
    tracks: Vec<Track>,
    // +0x124 µs per quarter note, +0x128 elapsed timer ticks T, +0x130 tick base B.
    tempo: u32,
    t: u64,
    b: u64,
    // +0x2C8 volume factor and +0x2CC last trunc(factor·128).
    factor: f32,
    last_q: i32,
    // +0x2E0 fade on, +0x2E4 length / +0x2E8 counter in timer ticks (ms).
    fade: bool,
    fade_len: u32,
    fade_counter: u32,
    // Per-channel raw CC 7 (+0x15).
    cc7: [u8; 16],
    // CC 2 copies of tempo/T/B (+0x2EC…+0x2FC); None = no CC 2 since boot (heap content in the original).
    loop_timing: Option<(u32, u64, u64)>,
}

// Byte reader over one track body; reading past the end yields None (a truncated track then ends).
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    // SMF variable-length quantity (7 bits per byte, high bit = more).
    fn vlq(&mut self) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..4 {
            let b = self.byte()?;
            v = (v << 7) | (b & 0x7F) as u32;
            if b < 0x80 {
                return Some(v);
            }
        }
        Some(v)
    }

    fn bytes(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.data.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(s)
    }
}

impl Default for Sequencer {
    fn default() -> Self {
        Self::new()
    }
}

impl Sequencer {
    // Midi_Ctor 0x443FA0: no tracks, tempo 0, factor 0.0, +0x2CC = 0. The raw CC 7 bytes are heap content in the
    // original (02 §5), 0 here.
    pub fn new() -> Self {
        Self {
            song: Song {
                format: 0,
                division: 0,
                tracks: Vec::new(),
            },
            tracks: Vec::new(),
            tempo: 0,
            t: 0,
            b: 0,
            factor: 0.0,
            last_q: 0,
            fade: false,
            fade_len: 0,
            fade_counter: 0,
            cc7: [0; 16],
            loop_timing: None,
        }
    }

    // Midi_Open 0x4442E0: the parsed tracks are freed first; a null slot leaves none. Else the track array is zeroed
    // (+0x1C = 0) and tempo := 1 000 000. +0x2CC, the channel state and the tempo/T/B copies are kept.
    pub fn open(&mut self, song: Option<Song>) {
        self.tracks.clear();
        self.song.tracks.clear();
        let Some(song) = song else { return };
        self.tracks = vec![Track::default(); song.tracks.len()];
        self.song = song;
        self.tempo = OPEN_TEMPO;
    }

    // +0x138 ≠ 0: `Midi_Play` / `Midi_Stop` return −1 at once without it.
    pub fn has_tracks(&self) -> bool {
        !self.tracks.is_empty()
    }

    // Midi_Rewind 0x444550: factor 1, fade off, T = B = 0, every track active, read pointer := start, next time :=
    // first delta, +0x18 := start (at the first delta); +0x1C and tempo are not reset.
    pub fn rewind(&mut self) {
        self.factor = 1.0;
        self.fade = false;
        self.t = 0;
        self.b = 0;
        for i in 0..self.tracks.len() {
            let mut r = Reader {
                data: &self.song.tracks[i],
                pos: 0,
            };
            let first = r.vlq();
            self.tracks[i] = Track {
                active: first.is_some(),
                next: first.unwrap_or(0) as u64,
                running: 0,
                pos: r.pos,
                loop_pos: 0,
                loop_next: self.tracks[i].loop_next,
            };
        }
    }

    // Midi_Fade(N) 0x4447D0: N in timer ticks (ms).
    pub fn fade(&mut self, len: u32) {
        self.factor = 0.0;
        self.fade_len = len;
        self.fade_counter = 0;
        self.fade = true;
    }

    pub fn factor(&self) -> f32 {
        self.factor
    }

    #[cfg(test)]
    fn t_for_test(&self) -> u64 {
        self.t
    }

    // Step 1: pos = B + trunc(T·division·1000 / tempo), unsigned 64-bit.
    fn pos(&self) -> u64 {
        self.b + self.t * self.song.division as u64 * 1000 / self.tempo.max(1) as u64
    }

    // Midi_Tick 0x444820, one 1 ms timer callback.
    pub fn tick(&mut self, out: &mut impl Out) {
        if self.fade {
            if self.fade_counter >= self.fade_len {
                // Fade end freezes the song: no events, notes that are on keep sounding.
                self.factor = 0.0;
                return;
            }
            self.factor = 1.0 - self.fade_counter as f32 / self.fade_len as f32;
            let q = (self.factor * 128.0) as i32;
            if q != self.last_q {
                for ch in 0..16u8 {
                    out.short([0xB0 | ch, 7, self.scaled(self.cc7[ch as usize])]);
                }
            }
            self.last_q = q;
            self.fade_counter += 1;
        }
        // Step 4's "active" flag is set when step 3 reaches an active track, before its events run (02 §5).
        let mut any = false;
        let mut pos = self.pos();
        for i in 0..self.tracks.len() {
            any |= self.tracks[i].active;
            while self.tracks[i].active && self.tracks[i].next <= pos {
                if self.event(i, out).is_none() {
                    self.tracks[i].active = false;
                }
                pos = self.pos();
            }
        }
        self.t += 1;
        if !any {
            self.rewind();
        }
    }

    // clamp(trunc(raw · factor), 0, 127).
    fn scaled(&self, raw: u8) -> u8 {
        ((raw as f32 * self.factor) as i32).clamp(0, 127) as u8
    }

    // Midi_Event 0x444A90; None = the track data ran out.
    fn event(&mut self, i: usize, out: &mut impl Out) -> Option<()> {
        let song = self.song.tracks[i].clone();
        let mut r = Reader {
            data: &song,
            pos: self.tracks[i].pos,
        };
        let mut status = *r.data.get(r.pos)?;
        if status < 0x80 {
            status = self.tracks[i].running;
        } else {
            r.pos += 1;
        }
        match status {
            0x80..=0xEF => {
                let d1 = r.byte()?;
                let d2 = if matches!(status & 0xF0, 0xC0 | 0xD0) {
                    0
                } else {
                    r.byte()?
                };
                let mut send = [status, d1, d2];
                if status & 0xF0 == 0xB0 {
                    match d1 {
                        7 => {
                            self.cc7[(status & 0x0F) as usize] = d2;
                            send[2] = self.scaled(d2);
                        }
                        2 => self.save_loop(i, r.pos),
                        4 => self.restore_loop(i, &mut r.pos),
                        _ => {}
                    }
                }
                out.short(send);
            }
            0xF0 => {
                let n = r.vlq()? as usize;
                out.sysex(r.bytes(n)?);
            }
            0xFF => {
                let kind = r.byte()?;
                let n = r.vlq()? as usize;
                let data = r.bytes(n)?;
                match kind {
                    0x2F => {
                        self.tracks[i].active = false;
                        return Some(());
                    }
                    0x51 => {
                        self.b = self.pos();
                        self.t = 0;
                        // Original bug kept (02 §5): tempo := tempo·256 + b + tempo per byte, i.e. ·257, ≈ 0.75 % slow.
                        self.tempo = data.iter().fold(0u32, |v, &b| {
                            (v << 8).wrapping_add(b as u32).wrapping_add(v)
                        });
                    }
                    _ => {}
                }
            }
            // 0xF1–0xFE (0xF7 included) read no data bytes and send nothing (02 §5 Event).
            _ => {}
        }
        // Running status := status, next time += the following delta (after a CC 4: the delta after the CC 2).
        let tr = &mut self.tracks[i];
        tr.running = status;
        tr.next += r.vlq()? as u64;
        tr.pos = r.pos;
        Some(())
    }

    // CC 2: every track's read pointer and next time (the current one: past this event, at its time), and tempo, T, B.
    fn save_loop(&mut self, cur: usize, end: usize) {
        for (k, t) in self.tracks.iter_mut().enumerate() {
            t.loop_pos = if k == cur { end } else { t.pos };
            t.loop_next = t.next;
        }
        self.loop_timing = Some((self.tempo, self.t, self.b));
    }

    // CC 4: restores the copies; `pos` is the current track's read position. Without a CC 2 since `Midi_Play` the
    // tracks go back to their start (+0x18) with next time +0x1C, and tempo/T/B to the last CC 2 since boot (02 §5);
    // before the first CC 2 since boot they are heap content in the original and are kept here.
    fn restore_loop(&mut self, cur: usize, pos: &mut usize) {
        for t in &mut self.tracks {
            (t.pos, t.next) = (t.loop_pos, t.loop_next);
        }
        *pos = self.tracks[cur].pos;
        if let Some((tempo, t, b)) = self.loop_timing {
            (self.tempo, self.t, self.b) = (tempo, t, b);
        }
    }
}

// The port's MIDI device: a rustysynth General MIDI synth (winmm MIDI mapper in the original).
#[cfg(not(target_arch = "wasm32"))]
pub struct Synth {
    synth: rustysynth::Synthesizer,
}

#[cfg(not(target_arch = "wasm32"))]
impl Synth {
    pub fn new(font: &Arc<rustysynth::SoundFont>, rate: u32) -> Result<Self> {
        let settings = rustysynth::SynthesizerSettings::new(rate as i32);
        Ok(Self {
            synth: rustysynth::Synthesizer::new(font, &settings)
                .map_err(|e| anyhow::anyhow!("{e:?}"))?,
        })
    }

    // midiOutReset: all notes off on every channel.
    pub fn reset_notes(&mut self) {
        self.synth.note_off_all(false);
    }

    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.synth.render(left, right);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Out for Synth {
    fn short(&mut self, m: [u8; 3]) {
        self.synth.process_midi_message(
            (m[0] & 0x0F) as i32,
            (m[0] & 0xF0) as i32,
            m[1] as i32,
            m[2] as i32,
        );
    }

    // Port: rustysynth has no SysEx input; the GM / GS / XG system resets map to a synth reset, the rest is ignored.
    fn sysex(&mut self, data: &[u8]) {
        const RESETS: [&[u8]; 3] = [
            &[0x7E, 0x7F, 0x09, 0x01, 0xF7],
            &[0x41, 0x10, 0x42, 0x12, 0x40, 0x00, 0x7F, 0x00, 0x41, 0xF7],
            &[0x43, 0x10, 0x4C, 0x00, 0x00, 0x7E, 0x00, 0xF7],
        ];
        if RESETS.iter().any(|r| data == *r) {
            self.synth.reset();
        }
    }
}

// The 1 ms timer (`timeSetEvent`) driven by the output sample clock: one `Midi_Tick` per elapsed millisecond of
// rendered audio, the synth rendered in between.
#[cfg(not(target_arch = "wasm32"))]
pub struct Player {
    pub synth: Synth,
    pub seq: Sequencer,
    // Timer running (between Midi_Play and Midi_Stop).
    pub playing: bool,
    rate: u32,
    // Output frames rendered since the last tick, in 1/1000 frame units.
    acc: u64,
    left: Vec<f32>,
    right: Vec<f32>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Player {
    pub fn new(font: &Arc<rustysynth::SoundFont>, rate: u32) -> Result<Self> {
        Ok(Self {
            synth: Synth::new(font, rate)?,
            seq: Sequencer::new(),
            playing: false,
            rate,
            acc: 0,
            left: Vec::new(),
            right: Vec::new(),
        })
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    // Renders `n` stereo frames; ticks the sequencer whenever another 1/1000 s of output has passed.
    pub fn render(&mut self, n: usize) -> (&[f32], &[f32]) {
        self.left.resize(n, 0.0);
        self.right.resize(n, 0.0);
        let mut done = 0;
        while done < n {
            let left_ms = (self.rate as u64 - self.acc).div_ceil(1000) as usize;
            let k = left_ms.clamp(1, n - done);
            self.synth.render(
                &mut self.left[done..done + k],
                &mut self.right[done..done + k],
            );
            self.acc += k as u64 * 1000;
            done += k;
            // The first callback comes 1 ms after timeSetEvent.
            if self.acc >= self.rate as u64 {
                self.acc -= self.rate as u64;
                if self.playing {
                    self.seq.tick(&mut self.synth);
                }
            }
        }
        (&self.left, &self.right)
    }
}

// SoundFont lookup: the `soundfont` setting, then common system locations; the first readable .sf2 is used.
#[cfg(not(target_arch = "wasm32"))]
pub fn find_soundfont(configured: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    if let Some(p) = configured {
        return Some(p.to_path_buf());
    }
    let mut dirs: Vec<std::path::PathBuf> = [
        "/usr/share/soundfonts",
        "/usr/share/sounds/sf2",
        "/usr/share/sf2",
        "/usr/local/share/soundfonts",
        "/usr/local/share/sounds/sf2",
        "/opt/homebrew/share/soundfonts",
        "C:\\soundfonts",
    ]
    .iter()
    .map(Into::into)
    .collect();
    if let Some(d) = directories::BaseDirs::new() {
        dirs.push(d.data_dir().join("soundfonts"));
        dirs.push(d.home_dir().join("Library/Audio/Sounds/Banks"));
    }
    const PREFERRED: [&str; 3] = ["default.sf2", "FluidR3_GM.sf2", "GeneralUser.sf2"];
    for d in &dirs {
        if let Some(p) = PREFERRED.iter().map(|f| d.join(f)).find(|p| p.is_file()) {
            return Some(p);
        }
        let Ok(rd) = std::fs::read_dir(d) else {
            continue;
        };
        let mut sf2: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sf2")))
            .collect();
        sf2.sort();
        if let Some(p) = sf2.into_iter().next() {
            return Some(p);
        }
    }
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_soundfont(path: &std::path::Path) -> Result<Arc<rustysynth::SoundFont>> {
    let mut f = std::io::BufReader::new(
        std::fs::File::open(path).with_context(|| path.display().to_string())?,
    );
    let sf = rustysynth::SoundFont::new(&mut f)
        .map_err(|e| anyhow::anyhow!("{}: {e:?}", path.display()))?;
    Ok(Arc::new(sf))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[derive(Default)]
    struct Log {
        short: Vec<(u64, [u8; 3])>,
        sysex: usize,
        now: u64,
    }

    impl Out for Log {
        fn short(&mut self, m: [u8; 3]) {
            self.short.push((self.now, m));
        }
        fn sysex(&mut self, _: &[u8]) {
            self.sysex += 1;
        }
    }

    // A format-0 file from (delta, raw event bytes) pairs.
    fn smf(division: u16, events: &[(u32, &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        for (d, e) in events {
            let mut v = vec![(*d & 0x7F) as u8];
            let mut d = *d >> 7;
            while d > 0 {
                v.insert(0, 0x80 | (d & 0x7F) as u8);
                d >>= 7;
            }
            body.extend(v);
            body.extend_from_slice(e);
        }
        let mut f = b"MThd\0\0\0\x06\0\0\0\x01".to_vec();
        f.extend(division.to_be_bytes());
        f.extend(b"MTrk");
        f.extend((body.len() as u32).to_be_bytes());
        f.extend(body);
        f
    }

    // Midi_Open + Midi_Play on a fresh object.
    fn played(song: Song) -> Sequencer {
        let mut s = Sequencer::new();
        s.open(Some(song));
        s.rewind();
        s
    }

    fn run(seq: &mut Sequencer, log: &mut Log, ms: u64) {
        for _ in 0..ms {
            seq.tick(log);
            log.now += 1;
        }
    }

    // Division 480, tempo bytes 07 C9 00 → 7·66049 + 201·257 = 514 000 µs (02 §5 ·257 quirk): tick 480 is due at
    // T = 514.
    #[test]
    fn timing_tempo_and_loop() {
        let f = smf(
            480,
            &[
                (0, &[0xFF, 0x51, 3, 0x07, 0xC9, 0x00]),
                (0, &[0xB0, 7, 100]),
                (480, &[0xB0, 2, 0]),
                (480, &[0x90, 60, 64]),
                (480, &[0xB0, 4, 0]),
                (0, &[0xFF, 0x2F, 0]),
            ],
        );
        let song = parse(&f).unwrap();
        assert_eq!((song.format, song.division, song.tracks.len()), (0, 480, 1));
        let mut seq = played(song);
        let mut log = Log::default();
        run(&mut seq, &mut log, 2100);
        assert_eq!(seq.tempo, 514_000);
        let at: Vec<_> = log.short.iter().map(|(t, m)| (*t, m[0], m[1])).collect();
        // CC 2 at 514 ms, note at 1028, CC 4 at 1542 → back to the CC 2 state: note again 514 ms later (2056).
        assert_eq!(
            &at[..5],
            &[
                (0, 0xB0, 7),
                (514, 0xB0, 2),
                (1028, 0x90, 60),
                (1542, 0xB0, 4),
                (2056, 0x90, 60)
            ]
        );
    }

    // 02 §5 example: th08_01's file tempo 0x069627 = 431 655 µs plays as 434 883.
    #[test]
    fn tempo_times_257() {
        let f = smf(
            480,
            &[
                (0, &[0xFF, 0x51, 3, 0x06, 0x96, 0x27]),
                (0, &[0xFF, 0x2F, 0]),
            ],
        );
        let mut seq = played(parse(&f).unwrap());
        run(&mut seq, &mut Log::default(), 1);
        assert_eq!(seq.tempo, 434_883);
    }

    // 0xF7 reads no length (02 §5 Event): its next byte is the following delta. CC 4 with no CC 2 since boot puts the
    // track at its start with next time +0x1C = 0; the CC 4's delta read then lands on the first event.
    #[test]
    fn f7_and_cc4_without_cc2() {
        let f = smf(
            480,
            &[
                (0, &[0x90, 60, 64]),
                (0, &[0xF7]),
                (480, &[0xB0, 4, 0]),
                (0, &[0xFF, 0x2F, 0]),
            ],
        );
        let mut seq = played(parse(&f).unwrap());
        let mut log = Log::default();
        // Tempo 1 000 000: the CC 4 (tick 480) is due at T = 1000. (Running it in a tick would loop forever there, as
        // in the original: the replayed events are all due again.)
        run(&mut seq, &mut log, 1000);
        assert_eq!((log.short.len(), seq.tracks[0].next), (1, 480));
        seq.event(0, &mut log).unwrap();
        assert_eq!((seq.tracks[0].pos, seq.tracks[0].next), (1, 0));
        assert_eq!(seq.tempo, OPEN_TEMPO);
    }

    // Midi_Ctor +0x2CC = 0 and the raw CC 7 bytes carry over Midi_Open / Midi_Play (02 §5 Object); a null slot leaves no
    // tracks.
    #[test]
    fn object_state_carries_across_songs() {
        let mut seq = Sequencer::new();
        assert_eq!((seq.last_q, seq.has_tracks()), (0, false));
        let a = smf(480, &[(0, &[0xB3, 7, 90]), (4800, &[0xFF, 0x2F, 0])]);
        seq.open(Some(parse(&a).unwrap()));
        seq.rewind();
        let mut log = Log::default();
        // First fade tick after boot: trunc(1·128) = 128 ≠ 0 → CC 7 resent (factor 1) on all channels.
        seq.fade(10);
        run(&mut seq, &mut log, 1);
        assert_eq!((log.short.len(), log.short[16].1), (17, [0xB3, 7, 90]));
        run(&mut seq, &mut log, 20);
        let q = seq.last_q;
        seq.open(None);
        assert!(!seq.has_tracks());
        let b = smf(480, &[(0, &[0x90, 60, 64]), (4800, &[0xFF, 0x2F, 0])]);
        seq.open(Some(parse(&b).unwrap()));
        seq.rewind();
        assert_eq!((seq.last_q, seq.cc7[3], seq.tempo), (q, 90, OPEN_TEMPO));
        log.short.clear();
        seq.fade(10);
        run(&mut seq, &mut log, 1);
        // Channel 3 is rescaled from the previous song's raw CC 7.
        assert!(log.short.contains(&(log.now - 1, [0xB3, 7, 90])));
    }

    #[test]
    fn fade_scales_cc7_and_freezes() {
        let f = smf(
            480,
            &[
                (0, &[0xB0, 7, 100]),
                (0, &[0xB1, 7, 127]),
                (960, &[0x90, 60, 64]),
                (0, &[0xFF, 0x2F, 0]),
            ],
        );
        let mut seq = played(parse(&f).unwrap());
        let mut log = Log::default();
        run(&mut seq, &mut log, 1);
        seq.fade(100);
        run(&mut seq, &mut log, 51);
        // Tick 51 runs with counter 50 → factor 0.5: trunc(100·0.5) = 50, trunc(127·0.5) = 63.
        let last: Vec<_> = log.short.iter().rev().take(16).collect();
        assert!(last.contains(&&(51, [0xB0, 7, 50])) && last.contains(&&(51, [0xB1, 7, 63])));
        run(&mut seq, &mut log, 2000);
        // Frozen after 100 ticks: the note at 1 s (tempo 1 000 000: 480 ticks per s) never plays.
        assert!(log.short.iter().all(|(_, m)| m[0] != 0x90));
        assert_eq!(seq.factor(), 0.0);
        // +0x2CC starts at 0, so the first fade tick after boot (counter 0, factor 1) resends CC 7 unscaled.
        let first: Vec<_> = log
            .short
            .iter()
            .filter(|(t, _)| *t == 1)
            .map(|m| m.1)
            .collect();
        assert_eq!(
            (first.len(), first[0], first[1]),
            (16, [0xB0, 7, 100], [0xB1, 7, 127])
        );
    }

    #[test]
    fn end_of_track_rewinds() {
        let f = smf(480, &[(0, &[0x90, 60, 64]), (480, &[0xFF, 0x2F, 0])]);
        let mut seq = played(parse(&f).unwrap());
        let mut log = Log::default();
        run(&mut seq, &mut log, 1003);
        // Tempo 1 000 000: end of track at 1000 ms; tick 1001 finds no active track and rewinds; note again at 1002.
        let notes: Vec<_> = log.short.iter().map(|(t, _)| *t).collect();
        assert_eq!(notes, [0, 1002]);
    }

    // The shipped files (02 §5): format 0, one track, division 480; every song reaches its loop end repeatedly.
    #[test]
    fn shipped_files() {
        let Some(a) = crate::archive::test_archive() else {
            return;
        };
        let names: Vec<_> = a
            .names()
            .iter()
            .filter(|n| n.ends_with(".mid"))
            .cloned()
            .collect();
        assert_eq!(names.len(), 22);
        for n in &names {
            let song = parse(&a.read(n).unwrap()).unwrap();
            assert_eq!(
                (song.format, song.division, song.tracks.len()),
                (0, 480, 1),
                "{n}"
            );
            let mut seq = played(song);
            let mut log = Log::default();
            run(&mut seq, &mut log, 400_000);
            let cc = |c: u8| {
                log.short
                    .iter()
                    .filter(|(_, m)| m[0] & 0xF0 == 0xB0 && m[1] == c)
                    .map(|(t, m)| (*t, m[0]))
                    .collect::<Vec<_>>()
            };
            let (c2, c4) = (cc(2), cc(4));
            assert!(!c2.is_empty() && c4.len() >= 2, "{n}: loop not reached");
            // After a loop end no CC 2 runs again: playback resumes after the last one (th08_07 has two).
            assert!(c2.iter().all(|c| c.0 < c4[0].0), "{n} {c2:?}");
        }
    }

    #[test]
    fn th08_01_loop_period() {
        let Some(a) = crate::archive::test_archive() else {
            return;
        };
        let song = parse(&a.read("th08_01.mid").unwrap()).unwrap();
        let mut seq = played(song);
        let mut log = Log::default();
        run(&mut seq, &mut log, 400_000);
        let c4: Vec<_> = log
            .short
            .iter()
            .filter(|(_, m)| m[0] & 0xF0 == 0xB0 && m[1] == 4)
            .map(|(t, _)| *t)
            .collect();
        // Every loop has the same length (the CC 2 timing state is restored with the read pointer).
        let d: Vec<_> = c4.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(
            d.len() >= 2 && d.iter().all(|x| x.abs_diff(d[0]) <= 1),
            "{d:?}"
        );
    }

    // Static walk of one track: absolute tick of every CC 2 / CC 4 and of the end of track; channels with notes but no CC 7.
    fn scan(body: &[u8]) -> (Vec<(u8, u64)>, u64, Vec<u8>) {
        let mut r = Reader { data: body, pos: 0 };
        let (mut at, mut running, mut marks) = (0u64, 0u8, Vec::new());
        let (mut notes, mut cc7) = ([false; 16], [false; 16]);
        loop {
            at += r.vlq().unwrap() as u64;
            let mut st = body[r.pos];
            if st < 0x80 {
                st = running;
            } else {
                r.pos += 1;
            }
            match st {
                0x80..=0xEF => {
                    let d1 = r.byte().unwrap();
                    let d2 = if matches!(st & 0xF0, 0xC0 | 0xD0) {
                        0
                    } else {
                        r.byte().unwrap()
                    };
                    let ch = (st & 0x0F) as usize;
                    match (st & 0xF0, d1) {
                        (0x90, _) if d2 != 0 => notes[ch] = true,
                        (0xB0, 7) => cc7[ch] = true,
                        (0xB0, 2 | 4) => marks.push((d1, at)),
                        _ => {}
                    }
                }
                0xF0 => {
                    let n = r.vlq().unwrap() as usize;
                    r.bytes(n).unwrap();
                }
                0xFF => {
                    let k = r.byte().unwrap();
                    let n = r.vlq().unwrap() as usize;
                    r.bytes(n).unwrap();
                    if k == 0x2F {
                        let no_cc7 = (0..16u8)
                            .filter(|&c| notes[c as usize] && !cc7[c as usize])
                            .collect();
                        return (marks, at, no_cc7);
                    }
                }
                _ => panic!("status {st:02x}"),
            }
            running = st;
        }
    }

    // 02 §5 scan example: th08_01 CC 2 at tick 3840, CC 4 at 77762, end of track 81610; every played channel sets CC 7.
    #[test]
    fn shipped_scan() {
        let Some(a) = crate::archive::test_archive() else {
            return;
        };
        let song = parse(&a.read("th08_01.mid").unwrap()).unwrap();
        let (marks, end, _) = scan(&song.tracks[0]);
        assert_eq!((marks, end), (vec![(2, 3840), (4, 77762)], 81610));
        for n in a
            .names()
            .iter()
            .filter(|n| n.ends_with(".mid") && *n != "init.mid")
        {
            let song = parse(&a.read(n).unwrap()).unwrap();
            assert_eq!(scan(&song.tracks[0]).2, Vec::<u8>::new(), "{n}");
        }
    }

    // A one-sample, one-instrument SoundFont (looped sine, every key), built in memory so the render test needs no
    // SoundFont on disk.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn tiny_sf2() -> Vec<u8> {
        fn chunk(id: &[u8], body: &[u8]) -> Vec<u8> {
            let mut v = id.to_vec();
            v.extend((body.len() as u32).to_le_bytes());
            v.extend(body);
            v
        }
        fn list(kind: &[u8], parts: &[Vec<u8>]) -> Vec<u8> {
            let mut b = kind.to_vec();
            parts.iter().for_each(|p| b.extend(p));
            chunk(b"LIST", &b)
        }
        fn name(n: &str) -> Vec<u8> {
            let mut v = n.as_bytes().to_vec();
            v.resize(20, 0);
            v
        }
        let le16 = |v: &[u16]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let len = 4410u32;
        let mut smpl: Vec<u8> = (0..len)
            .flat_map(|k| {
                let x = (k as f32 * 2.0 * std::f32::consts::PI * 100.0 / 44100.0).sin();
                ((x * 16000.0) as i16).to_le_bytes()
            })
            .collect();
        smpl.extend([0u8; 92]);
        let phdr = |n: &str, bag: u16| [name(n), le16(&[0, 0, bag]), vec![0; 12]].concat();
        let inst = |n: &str, bag: u16| [name(n), le16(&[bag])].concat();
        let shdr = |n: &str, a: [u32; 5], kind: u16| {
            let mut v = name(n);
            a.iter().for_each(|x| v.extend(x.to_le_bytes()));
            v.extend([60, 0]);
            v.extend(le16(&[0, kind]));
            v
        };
        let info = list(
            b"INFO",
            &[
                chunk(b"ifil", &le16(&[2, 1])),
                chunk(b"isng", b"EMU8000\0"),
                chunk(b"INAM", b"t\0"),
            ],
        );
        let sdta = list(b"sdta", &[chunk(b"smpl", &smpl)]);
        let pdta = list(
            b"pdta",
            &[
                chunk(b"phdr", &[phdr("p", 0), phdr("EOP", 1)].concat()),
                chunk(b"pbag", &le16(&[0, 0, 1, 0])),
                chunk(b"pmod", &[0; 10]),
                chunk(b"pgen", &le16(&[41, 0, 0, 0])),
                chunk(b"inst", &[inst("i", 0), inst("EOI", 1)].concat()),
                chunk(b"ibag", &le16(&[0, 0, 2, 0])),
                chunk(b"imod", &[0; 10]),
                chunk(b"igen", &le16(&[54, 1, 53, 0, 0, 0])),
                chunk(
                    b"shdr",
                    &[
                        shdr("s", [0, len, 0, len, 44100], 1),
                        shdr("EOS", [0; 5], 0),
                    ]
                    .concat(),
                ),
            ],
        );
        let mut body = b"sfbk".to_vec();
        body.extend([info, sdta, pdta].concat());
        chunk(b"RIFF", &body)
    }

    // Player end to end into a buffer (never to a device): th08_01 sounds, and after Midi_Fade the frozen song keeps
    // only its held notes, scaled down by the CC 7 resend.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn player_renders_to_buffer() {
        let Some(a) = crate::archive::test_archive() else {
            return;
        };
        let font = Arc::new(rustysynth::SoundFont::new(&mut &tiny_sf2()[..]).unwrap());
        let mut p = Player::new(&font, 44100).unwrap();
        p.seq
            .open(Some(parse(&a.read("th08_01.mid").unwrap()).unwrap()));
        p.seq.rewind();
        p.playing = true;
        let rms = |p: &mut Player, frames: usize| {
            let (l, r) = p.render(frames);
            (l.iter().chain(r).map(|x| x * x).sum::<f32>() / (2 * frames) as f32).sqrt()
        };
        let mut loud = 0.0f32;
        for _ in 0..10 {
            loud = loud.max(rms(&mut p, 44100));
        }
        assert!(loud > 1e-3, "{loud}");
        // 1 s = 1000 timer ticks per 44100 frames.
        assert_eq!(p.seq.t_for_test(), 10_000);
        p.seq.fade(500);
        let _ = rms(&mut p, 44100);
        assert_eq!(p.seq.factor(), 0.0);
        p.playing = false;
        p.synth.reset_notes();
        let _ = rms(&mut p, 44100);
        let quiet = rms(&mut p, 44100);
        assert!(quiet < 1e-4, "{quiet}");
    }
}
