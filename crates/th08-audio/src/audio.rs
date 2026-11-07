//! Audio back end: thbgm.dat BGM streaming (02 §1–§4), MIDI music mode and the SE voices (13 §1).
#![allow(clippy::disallowed_methods)]

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use th08_core::assets::Source;

use crate::bgm::{BgmSource, Pcm, PcmWriter};

// thbgm.fmt WAVEFORMATEX: 44100 Hz, stereo, 16-bit for every track.
const BGM_RATE: f64 = 44100.0;

use th08_core::sound::{MidiCmd, SoundCmd, Track, WavCmd};

// Streaming ring: 16 notification chunks of 44100 bytes = 11025 stereo frames (0.25 s), 4.0 s in all.
const CHUNK: usize = crate::bgm::BLOCK;
const RING: usize = crate::bgm::RING_FRAMES;

// The playing BGM: the stored track in memory (decoded progressively), heard through a model of the DirectSound
// streaming ring so that `Bgm_StreamStop` + UnPause replays what the ring still holds.
struct Music {
    // s16 frames as stored in thbgm.dat (or decoded from thbgm.pak), half the RAM of f32;
    // the loader thread still appends while the first rings play.
    samples: Arc<Pcm>,
    loop_start: usize,
    // Per ring chunk: the stream frame its data starts at (frames read since the play command's rewind).
    chunks: [u64; 16],
    // Refill offset +0x64 as a chunk index, and the file position as a stream frame.
    refill: usize,
    file: u64,
    // Play cursor in ring frames (fractional for resampling).
    cursor: f64,
}

impl Music {
    // Play command fill stage: rewind, fill the whole ring with the first 4.0 s, +0x64 := 0, play from 0.
    fn new(samples: Arc<Pcm>, loop_start: usize) -> Self {
        Self {
            samples,
            loop_start,
            chunks: std::array::from_fn(|k| (k * CHUNK) as u64),
            refill: 0,
            file: RING as u64,
            cursor: 0.0,
        }
    }

    // Stream frame → track frame: `loop_end` frames first, then loop_start..loop_end forever.
    fn track_frame(&self, s: u64) -> usize {
        let n = self.samples.len() as u64;
        if s < n {
            return s as usize;
        }
        let ls = (self.loop_start as u64).min(n - 1);
        (ls + (s - n) % (n - ls)) as usize
    }

    fn ring_sample(&self, r: usize) -> [f32; 2] {
        let r = r % RING;
        let s = self
            .samples
            .get(self.track_frame(self.chunks[r / CHUNK] + (r % CHUNK) as u64));
        [f32::from(s[0]) / 32768.0, f32::from(s[1]) / 32768.0]
    }

    // Notification as the play cursor leaves a chunk (handler 0x472D30). The write cursor then lies in the
    // chunk the play cursor entered; if that is the refill chunk → "Stream Skip", else refill it and advance.
    fn notify(&mut self, entered: usize) {
        if entered == self.refill {
            return;
        }
        self.chunks[self.refill] = self.file;
        self.file += CHUNK as u64;
        self.refill = (self.refill + 1) % 16;
    }

    // One output frame at `step` ring frames per output frame (port: linear resampling to the device rate).
    #[expect(clippy::cast_sign_loss, reason = "the ring cursor is ≥ 0")]
    fn next(&mut self, step: f64) -> (f32, f32) {
        let i = self.cursor as usize;
        let t = (self.cursor - i as f64) as f32;
        let (a, b) = (self.ring_sample(i), self.ring_sample(i + 1));
        self.cursor += step;
        let k = self.cursor as usize / CHUNK;
        if k != i / CHUNK {
            self.notify(k % 16);
        }
        if self.cursor >= RING as f64 {
            self.cursor -= RING as f64;
        }
        (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)
    }
}

// One se_*.wav PCM wave file (RIFF, 8/16-bit, mono/stereo; 04 file index).
pub struct Wave {
    samples: Vec<[f32; 2]>,
    rate: f64,
}

// PCM WAV (8/16-bit, mono/stereo) via hound; 8-bit is unsigned on disk, hound yields it centred on 0.
pub fn parse_wav(data: &[u8]) -> Result<Wave> {
    let r = hound::WavReader::new(std::io::Cursor::new(data))?;
    let spec = r.spec();
    let (channels, bits) = (spec.channels as usize, spec.bits_per_sample);
    anyhow::ensure!(
        spec.sample_format == hound::SampleFormat::Int
            && (1..=2).contains(&channels)
            && (bits == 8 || bits == 16),
        "unsupported wav format {:?}/{channels}/{bits}",
        spec.sample_format
    );
    let scale = if bits == 8 { 128.0 } else { 32768.0 };
    let pcm = r
        .into_samples::<i16>()
        .map(|v| v.map(|v| f32::from(v) / scale))
        .collect::<Result<Vec<f32>, _>>()?;
    let samples = pcm
        .chunks_exact(channels)
        .map(|f| [f[0], f[channels - 1]])
        .collect();
    Ok(Wave {
        samples,
        rate: f64::from(spec.sample_rate),
    })
}

// One SE id's DirectSound buffer: a non-looping copy of its wave; gain holds volume and pan per channel.
#[derive(Default, Clone)]
struct Voice {
    wave: Option<Arc<Wave>>,
    pos: f64,
    gain: [f32; 2],
    playing: bool,
}

// DirectSound attenuation (1/100 dB) → amplitude; −10000 is silence.
fn db_gain(db: i32) -> f32 {
    if db <= -10000 {
        0.0
    } else {
        10f32.powf(db as f32 / 2000.0)
    }
}

// The mixer is plain data: a panic elsewhere must not take the game thread's sound commands down with it.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

// The output callback: a panic inside the mixer gives silence for this buffer and drops the synth.
fn render_guarded(mixer: &Mutex<Mixer>, out: &mut [f32], channels: usize) {
    let mut m = lock(mixer);
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| m.render(out, channels))).is_err() {
        m.drop_synth();
        out.fill(0.0);
    }
}

#[derive(Default)]
struct Mixer {
    voices: Vec<Voice>,
    music: Option<Music>,
    generation: u32,
    out_rate: f64,
    volume: f32,
    // Port volume rows (0–1) applied after the game's own gains; `muted` silences the output, positions still advance.
    bgm_gain: f32,
    se_gain: f32,
    muted: bool,
    // Streaming object +0x30 playing flag cleared by `Stop` with the position kept (command 6, mode-1 fade end).
    stopped: bool,
    // MIDI mode (02 §5): the sequencer + synth, driven by the output sample clock; not scaled by the BGM volume.
    #[cfg(not(target_arch = "wasm32"))]
    midi: Option<crate::midi::Player>,
    #[cfg(not(target_arch = "wasm32"))]
    font: Option<Arc<rustysynth::SoundFont>>,
}

impl Mixer {
    // The MIDI player at the current output rate, (re)created from the SoundFont when the rate changed.
    #[cfg(not(target_arch = "wasm32"))]
    #[expect(clippy::cast_sign_loss, reason = "device rates are positive")]
    fn midi(&mut self) -> Option<&mut crate::midi::Player> {
        let rate = self.out_rate as u32;
        if self.midi.as_ref().is_none_or(|p| p.rate() != rate) {
            let font = self.font.as_ref()?;
            let mut p = match crate::midi::Player::new(font, rate) {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("midi synth: {e:#}");
                    self.font = None;
                    return None;
                }
            };
            if let Some(old) = self.midi.take() {
                (p.seq, p.playing) = (old.seq, old.playing);
            }
            self.midi = Some(p);
        }
        self.midi.as_mut()
    }

    // After a panic inside `render`: MIDI music stops until a new SoundFont load, WAV and SE go on.
    fn drop_synth(&mut self) {
        log::error!("audio: the mixer panicked; MIDI synth dropped, this callback is silent");
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.midi = None;
            self.font = None;
        }
    }

    #[expect(clippy::cast_sign_loss, reason = "voice positions are ≥ 0")]
    fn render(&mut self, out: &mut [f32], channels: usize) {
        let step = BGM_RATE / self.out_rate;
        #[cfg(not(target_arch = "wasm32"))]
        let n = out.len() / channels;
        #[cfg(not(target_arch = "wasm32"))]
        if self.midi.is_some() {
            self.midi();
        }
        #[cfg(not(target_arch = "wasm32"))]
        let midi = self.midi.as_mut().map(|p| p.render(n));
        let (bg, sg) = (self.bgm_gain, self.se_gain);
        for (k, fr) in out.chunks_exact_mut(channels).enumerate() {
            let (l, r) = match &mut self.music {
                Some(m) if !m.samples.is_empty() && !self.stopped => m.next(step),
                _ => (0.0, 0.0),
            };
            let (mut l, mut r) = (l * self.volume, r * self.volume);
            #[cfg(not(target_arch = "wasm32"))]
            if let Some((ml, mr)) = midi {
                l += ml[k];
                r += mr[k];
            }
            #[cfg(target_arch = "wasm32")]
            let _ = k;
            (l, r) = (l * bg, r * bg);
            for vc in self.voices.iter_mut().filter(|v| v.playing) {
                let Some(w) = &vc.wave else { continue };
                let i = vc.pos as usize;
                if i >= w.samples.len() {
                    vc.playing = false;
                    continue;
                }
                let s = w.samples[i];
                l += s[0] * (vc.gain[0] * sg);
                r += s[1] * (vc.gain[1] * sg);
                vc.pos += w.rate / self.out_rate;
            }
            if self.muted {
                (l, r) = (0.0, 0.0);
            }
            if channels > 1 {
                fr[0] = l.clamp(-1.0, 1.0);
                fr[1] = r.clamp(-1.0, 1.0);
            } else {
                fr[0] = ((l + r) * 0.5).clamp(-1.0, 1.0);
            }
        }
    }
}

// SoundFont lookup for MIDI music mode, from the app's / tool's settings (`soundfont`, the platform's
// soundfont dirs); read once at load by `midi_available`.
#[derive(Clone, Debug, Default)]
pub struct AudioConfig {
    pub soundfont: Option<std::path::PathBuf>,
    pub soundfont_dirs: Vec<std::path::PathBuf>,
    pub volume: PortVolume,
    // Desktop / web output callback size in frames; None = the device default.
    pub buffer: Option<u32>,
}

// Port volume rows as gains 0–1: `master` scales both the BGM (WAV and MIDI) and the SE mix.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortVolume {
    pub master: f32,
    pub bgm: f32,
    pub se: f32,
}

impl Default for PortVolume {
    fn default() -> Self {
        Self {
            master: 1.0,
            bgm: 1.0,
            se: 1.0,
        }
    }
}

// Port volume rows of a running `Audio` (port menu changes).
#[derive(Clone)]
pub struct VolumeHandle(Arc<Mutex<Mixer>>);

impl VolumeHandle {
    pub fn set(&self, v: PortVolume) {
        let mut m = lock(&self.0);
        m.bgm_gain = v.master * v.bgm;
        m.se_gain = v.master * v.se;
    }
}

// Background loader, standing in for the original's loader threads (00 §3.2, §6).
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn(fut: impl std::future::Future<Output = ()> + Send + 'static) {
    std::thread::spawn(move || pollster::block_on(fut));
}

#[cfg(target_arch = "wasm32")]
pub fn spawn(fut: impl std::future::Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(fut);
}

// Device side of the sound manager: SE voices, the BGM stream and the MIDI object; driven only by `exec`.
pub struct Audio {
    source: Source,
    // thbgm.dat or thbgm.pak, chosen on the first play command (needs async reads of the source).
    bgm: Arc<Mutex<Option<Arc<BgmSource>>>>,
    mixer: Arc<Mutex<Mixer>>,
    stream: Option<cpal::Stream>,
    // Port: output held by `pause`; `lost` is set by the error callback when the device or stream
    // is gone, so `resume` builds a new stream (the mixer keeps the BGM / voice positions).
    paused: bool,
    lost: Arc<AtomicBool>,
    // A stream was opened once (`is_started`; `resume` reopens only then, so the web still waits for its gesture).
    started: bool,
    stats: Arc<Stats>,
    // Port: a lost stream is reopened at most this often while playing (route change: headphones, Bluetooth).
    #[cfg(not(target_arch = "wasm32"))]
    retry_at: Option<std::time::Instant>,
    open_stream: OpenStream,
    // MIDI object: the archive's .mid members by lower-case basename, the 32 file slots +0x98 and the open
    // slot +0x10 (−1 none).
    midi_files: HashMap<String, Arc<[u8]>>,
    midi_slots: [Option<Arc<[u8]>>; 32],
    midi_open: i32,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    cfg: AudioConfig,
    // No device: `OfflineMix` pulls the samples, BGM tracks decode before the play command returns.
    offline: bool,
}

impl Audio {
    pub fn new(source: Source, cfg: AudioConfig) -> Self {
        let v = cfg.volume;
        let mixer = Arc::new(Mutex::new(Mixer {
            volume: 1.0,
            bgm_gain: v.master * v.bgm,
            se_gain: v.master * v.se,
            out_rate: BGM_RATE,
            ..Default::default()
        }));
        Self {
            source,
            bgm: Arc::default(),
            mixer,
            stream: None,
            paused: false,
            lost: Arc::new(AtomicBool::new(false)),
            started: false,
            stats: Arc::default(),
            #[cfg(not(target_arch = "wasm32"))]
            retry_at: None,
            open_stream,
            midi_files: HashMap::new(),
            midi_slots: Default::default(),
            midi_open: -1,
            cfg,
            offline: false,
        }
    }

    // A device-less sound manager whose mix is pulled per game frame through the returned `OfflineMix` at `rate` Hz.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn offline(source: Source, cfg: AudioConfig, rate: u32) -> (Self, OfflineMix) {
        let mut a = Self::new(source, cfg);
        a.offline = true;
        a.started = true;
        lock(&a.mixer).out_rate = f64::from(rate);
        let mix = OfflineMix {
            mixer: a.mixer.clone(),
            rate,
            frames: 0,
        };
        (a, mix)
    }

    // Host input for `sound::Bgm::midi_ready`: a SoundFont (`AudioConfig::soundfont`, else its dirs) loads, so
    // music mode 2 plays MIDI; else it takes the WAV path (port).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn midi_available(&mut self) -> bool {
        let wav = "music mode MIDI plays the WAV tracks";
        let found =
            crate::midi::find_soundfont(self.cfg.soundfont.as_deref(), &self.cfg.soundfont_dirs);
        let Some(p) = found.filter(|p| p.is_file()) else {
            if let Some(p) = &self.cfg.soundfont {
                log::warn!("midi: soundfont {} not found; {wav}", p.display());
            } else {
                log::info!("midi: no SoundFont found (set soundfont / TH08_SOUNDFONT); {wav}");
            }
            return false;
        };
        match crate::midi::load_soundfont(&p) {
            Ok(f) => {
                log::info!("midi: soundfont {}", p.display());
                lock(&self.mixer).font = Some(f);
                true
            }
            Err(e) => {
                log::warn!("midi: {e:#}; {wav}");
                false
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn midi_available(&mut self) -> bool {
        false
    }

    pub fn exec(&mut self, c: SoundCmd) {
        #[cfg(not(target_arch = "wasm32"))]
        self.reopen_if_lost(std::time::Instant::now());
        match c {
            SoundCmd::Se { id, pan, vol } => self.play_se(id, pan, vol),
            SoundCmd::Wav(c) => match c {
                WavCmd::Play(t) => self.play_wav(t),
                WavCmd::Stop => self.stop_wav(),
                WavCmd::Pause(on) => lock(&self.mixer).stopped = on,
                WavCmd::Release => self.release_bgm(),
                WavCmd::Volume(db) => lock(&self.mixer).volume = db_gain(db),
            },
            SoundCmd::Midi(c) => match c {
                MidiCmd::Load { slot, name } => self.midi_load_slot(slot, &name),
                MidiCmd::PlaySlot(slot) => {
                    self.midi_stop();
                    self.midi_open(slot);
                    self.midi_play();
                }
                MidiCmd::PlayFile(name) => {
                    self.midi_stop();
                    self.midi_load_slot(31, &name);
                    self.midi_open(31);
                    self.midi_slots[31] = None;
                    self.midi_play();
                }
                MidiCmd::Stop => self.midi_stop(),
                MidiCmd::Reset => {
                    self.midi_stop();
                    self.midi_open(30);
                    self.midi_play();
                }
                MidiCmd::Fade(ms) => {
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(p) = lock(&self.mixer).midi() {
                        p.seq.fade(ms);
                    }
                    #[cfg(target_arch = "wasm32")]
                    let _ = ms;
                }
            },
        }
    }

    // The archive's `bgm/*.mid` members, then the boot `Midi_LoadSlot(30, "bgm/init.mid")`.
    pub fn load_midi(&mut self, archive: th08_core::content::PresentView<'_>) {
        for n in archive.names().iter().filter(|n| n.ends_with(".mid")) {
            match archive.read(n) {
                Ok(d) => {
                    self.midi_files.insert(n.to_ascii_lowercase(), d.into());
                }
                Err(e) => log::warn!("{n}: {e:#}"),
            }
        }
        self.midi_load_slot(30, "bgm/init.mid");
    }

    // Midi_LoadSlot 0x4441A0: the open slot is stopped first; the name's basename is the archive lookup (01 §5); not
    // found → null slot.
    fn midi_load_slot(&mut self, slot: usize, name: &str) {
        if slot_id(slot) == self.midi_open {
            self.midi_stop();
        }
        let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let data = self.midi_files.get(&base.to_ascii_lowercase()).cloned();
        if data.is_none() {
            log::warn!("midi: {name} not in archive");
        }
        if let Some(s) = self.midi_slots.get_mut(slot) {
            *s = data;
        }
    }

    // Midi_Open 0x4442E0: parses the slot's file (tempo 1 000 000), open slot := slot; a null slot frees the tracks
    // and leaves the open slot unchanged.
    fn midi_open(&mut self, slot: usize) {
        let song = self.midi_slots.get(slot).cloned().flatten().and_then(|d| {
            crate::midi::parse(&d)
                .map_err(|e| log::warn!("midi slot {slot}: {e:#}"))
                .ok()
        });
        if song.is_some() {
            self.midi_open = slot_id(slot);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = lock(&self.mixer).midi() {
            p.seq.open(song);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = song;
    }

    // Midi_Play 0x444620: Midi_Rewind, then the 1 ms timer starts; nothing without parsed tracks.
    fn midi_play(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = lock(&self.mixer).midi()
            && p.seq.has_tracks()
        {
            p.seq.rewind();
            p.playing = true;
        }
    }

    // Midi_Stop 0x444680: timer killed, midiOutReset (all notes off), open slot := −1; nothing without parsed tracks.
    fn midi_stop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = lock(&self.mixer).midi.as_mut() {
            if !p.seq.has_tracks() {
                return;
            }
            p.playing = false;
            p.synth.reset_notes();
        }
        self.midi_open = -1;
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    // Port: silence the output without holding it (window unfocused); BGM and SE keep playing underneath.
    pub fn set_muted(&mut self, on: bool) {
        lock(&self.mixer).muted = on;
    }

    // A handle that sets the port volume rows while the game holds this device behind `AudioOut`.
    pub fn volume_handle(&self) -> VolumeHandle {
        VolumeHandle(self.mixer.clone())
    }

    // Must be called from a user gesture on the web (AudioContext policy).
    pub fn ensure_started(&mut self) {
        if !self.started && !self.paused {
            self.open();
        }
        // Web: a context opened outside a gesture the browser accepts stays suspended; each later gesture resumes it.
        #[cfg(target_arch = "wasm32")]
        if !self.paused
            && let Some(s) = &self.stream
        {
            let _ = s.play();
        }
    }

    // Port: AAudio / ALSA drop the stream on a route change; while not paused a new one is opened on the next command.
    #[cfg(not(target_arch = "wasm32"))]
    fn reopen_if_lost(&mut self, now: std::time::Instant) {
        if !self.started
            || self.paused
            || (self.stream.is_some() && !self.lost.load(Ordering::Relaxed))
            || self.retry_at.is_some_and(|t| now < t)
        {
            return;
        }
        log::warn!("audio: stream lost while playing; reopening");
        self.retry_at = Some(now + std::time::Duration::from_secs(1));
        self.stream = None;
        self.open();
    }

    fn open(&mut self) {
        if self.offline {
            return;
        }
        self.lost.store(false, Ordering::Relaxed);
        match (self.open_stream)(
            self.mixer.clone(),
            self.lost.clone(),
            self.stats.clone(),
            self.cfg.buffer,
        ) {
            Ok(s) => {
                self.stream = Some(s);
                self.started = true;
            }
            Err(e) => log::warn!("audio disabled: {e:#}"),
        }
    }

    // Port: holds the output (app background, port menu) within one device callback. Native: the stream is closed,
    // which discards the samples the device (AAudio / ALSA) still queues instead of letting them drain; web: the
    // AudioContext is suspended (reopening it would need a user gesture). The mixer keeps every position. Idempotent.
    pub fn pause(&mut self) {
        if std::mem::replace(&mut self.paused, true) {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.stream = None;
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(s) = &self.stream
            && let Err(e) = s.pause()
        {
            log::warn!("audio pause: {e}; the stream is closed");
            self.stream = None;
        }
        log::info!("audio: paused ({})", self.stats.summary());
    }

    // Port: output again; native (and a lost device / failed play) gets a new stream. Idempotent.
    pub fn resume(&mut self) {
        if !std::mem::replace(&mut self.paused, false) {
            return;
        }
        let lost = self.lost.load(Ordering::Relaxed);
        if let Some(s) = &self.stream
            && (lost || s.play().is_err())
        {
            log::warn!("audio resume: stream lost or not playing; reopening");
            self.stream = None;
        }
        if self.stream.is_none() && self.started {
            self.open();
        }
        log::info!(
            "audio: resumed (stream {})",
            if self.stream.is_some() {
                "open"
            } else {
                "none"
            }
        );
    }

    // Play command: the stored part of the track (never past loop_end), read in the background.
    fn play_wav(&mut self, t: Track) {
        let generation = {
            let mut m = lock(&self.mixer);
            m.generation += 1;
            m.music = None;
            m.stopped = false;
            m.generation
        };
        let (source, mixer, cell) = (self.source.clone(), self.mixer.clone(), self.bgm.clone());
        let job = async move {
            let t0 = web_time::Instant::now();
            let known = lock(&cell).clone();
            let bgm = if let Some(b) = known {
                b
            } else {
                let b = Arc::new(BgmSource::resolve(&source).await);
                lock(&cell).get_or_insert(b).clone()
            };
            // Port: the fill stage needs only the first ring, so the Music starts as soon as 4.0 s (or the whole shorter
            // track) are decoded; the rest is appended while it plays. Decoding stops when the track is replaced or released.
            let pcm = Pcm::new(t.length);
            let (name, loop_start, first) = (t.name.clone(), t.loop_start, RING.min(t.length));
            let mut started = false;
            let m2 = mixer.clone();
            let mut w = PcmWriter::with(pcm.clone(), move |p, ready| {
                if !started {
                    if ready < first {
                        return lock(&m2).generation == generation;
                    }
                    let mut m = lock(&m2);
                    if m.generation != generation {
                        return false;
                    }
                    m.music = Some(Music::new(p.clone(), loop_start));
                    started = true;
                    log::info!(
                        "bgm {name}: first {ready} frames after {:.1} ms, playing",
                        t0.elapsed().as_secs_f64() * 1000.0
                    );
                    return true;
                }
                // The writer's own Arc plus the Music's (kept across Music_Stop, dropped on a new play / release).
                Arc::strong_count(p) > 1
            });
            match bgm.stream(&source, &t, &mut w).await {
                Ok(()) if pcm.ready() == pcm.len() => {
                    if pcm.is_empty() {
                        let mut m = lock(&mixer);
                        if m.generation == generation {
                            m.music = Some(Music::new(pcm.clone(), t.loop_start));
                        }
                    }
                    log::info!(
                        "bgm {}: all {} frames after {:.1} ms{}",
                        t.name,
                        pcm.len(),
                        t0.elapsed().as_secs_f64() * 1000.0,
                        if pcm.underrun() {
                            " (underrun: played before decoded)"
                        } else {
                            ""
                        }
                    );
                }
                Ok(()) => log::info!(
                    "bgm {}: decode stopped at {} / {} frames (replaced)",
                    t.name,
                    pcm.ready(),
                    pcm.len()
                ),
                Err(e) => log::warn!("bgm {}: {e:#}", t.name),
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        if self.offline {
            pollster::block_on(job);
            return;
        }
        spawn(job);
    }

    // Sound_LoadSE 0x45D3F0: one buffer per SE id, each a copy of its wave file; the table volume is not
    // stored because Sound_Update overwrites it on every play.
    pub fn load_se(&mut self, archive: th08_core::content::PresentView<'_>) {
        let mut waves: Vec<(&str, Option<Arc<Wave>>)> = Vec::new();
        let mut voices = Vec::new();
        for (name, _) in th08_core::sound::TABLE {
            let w = if let Some((_, w)) = waves.iter().find(|(n, _)| *n == name) {
                w.clone()
            } else {
                let w = archive
                    .read(&format!("{name}.wav"))
                    .map_err(anyhow::Error::from)
                    .and_then(|d| parse_wav(&d))
                    .map(Arc::new);
                if let Err(e) = &w {
                    log::warn!("{name}.wav: {e:#}");
                }
                waves.push((name, w.ok()));
                waves.last().unwrap().1.clone()
            };
            voices.push(Voice {
                wave: w,
                ..Voice::default()
            });
        }
        lock(&self.mixer).voices = voices;
    }

    // Sound_Update per queued id: stop, rewind, set pan/volume (1/100 dB) and play once.
    // Pan uses the DirectSound rule: the opposite channel is attenuated by |pan| (1/100 dB).
    fn play_se(&mut self, id: usize, pan: i32, volume: i32) {
        let mut m = lock(&self.mixer);
        let Some(v) = m.voices.get_mut(id) else {
            return;
        };
        let g = db_gain(volume);
        let pan = pan.clamp(-10000, 10000);
        v.gain = [g * db_gain(-pan.max(0)), g * db_gain(pan.min(0))];
        v.pos = 0.0;
        v.playing = v.wave.is_some();
    }

    // `Bgm_StreamStop` 0x4728E0: `Stop` + position 0, playing flag 0; the object, ring, +0x64 and file
    // position stay, so a later UnPause plays the ring on from position 0.
    fn stop_wav(&mut self) {
        let mut m = lock(&self.mixer);
        m.stopped = true;
        if let Some(music) = &mut m.music {
            music.cursor = 0.0;
        }
    }

    // Sound manager release on restart (00 §2): the streaming object is gone; nothing is left to UnPause.
    fn release_bgm(&mut self) {
        let mut m = lock(&self.mixer);
        m.generation += 1;
        m.music = None;
    }
}

impl th08_core::game::AudioOut for Audio {
    fn load(&mut self, archive: th08_core::content::PresentView<'_>) {
        self.load_se(archive);
        self.load_midi(archive);
    }
    fn midi_available(&mut self) -> bool {
        Audio::midi_available(self)
    }
    fn exec(&mut self, c: SoundCmd) {
        Audio::exec(self, c);
    }
    fn is_started(&self) -> bool {
        Audio::is_started(self)
    }
    fn ensure_started(&mut self) {
        Audio::ensure_started(self);
    }
    fn pause(&mut self) {
        Audio::pause(self);
    }
    fn resume(&mut self) {
        Audio::resume(self);
    }
    fn set_muted(&mut self, on: bool) {
        Audio::set_muted(self, on);
    }
}

// The mix of an offline `Audio`, pulled one game frame (1/60 s) at a time; no output device is ever opened.
#[cfg(not(target_arch = "wasm32"))]
pub struct OfflineMix {
    mixer: Arc<Mutex<Mixer>>,
    rate: u32,
    frames: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl OfflineMix {
    pub fn rate(&self) -> u32 {
        self.rate
    }

    // Sample frames of game frame `n` at `rate`: the running total after n frames is exactly ⌊n × rate / 60⌋.
    #[expect(clippy::cast_possible_truncation, reason = "at most rate / 60 + 1")]
    pub fn samples_in_frame(rate: u32, n: u64) -> usize {
        let at = |n: u64| n * u64::from(rate) / 60;
        (at(n + 1) - at(n)) as usize
    }

    // Appends the next game frame's interleaved stereo samples (f32, −1…1) to `out`.
    pub fn frame(&mut self, out: &mut Vec<f32>) {
        let n = Self::samples_in_frame(self.rate, self.frames);
        self.frames += 1;
        let at = out.len();
        out.resize(at + n * 2, 0.0);
        render_guarded(&self.mixer, &mut out[at..], 2);
    }

    // Game frames mixed so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }
}

// Port: output device setup (prefers 44.1 kHz stereo so the BGM needs no resampling).
// Android output callback in frames (10 ms at 48 kHz); the AAudio capacity is twice that.
const ANDROID_FRAMES: u32 = 480;

// Port: device callback counts for the latency log (frames per callback = the mixer → device granularity).
#[derive(Default)]
struct Stats {
    callbacks: AtomicU64,
    frames: AtomicU64,
    max_frames: AtomicU64,
    rate: AtomicU64,
    // Underruns reported by the backend (AAudio xrun count).
    xruns: AtomicU64,
}

impl Stats {
    fn on_callback(&self, frames: u64) {
        self.callbacks.fetch_add(1, Ordering::Relaxed);
        self.frames.fetch_add(frames, Ordering::Relaxed);
        self.max_frames.fetch_max(frames, Ordering::Relaxed);
    }

    fn summary(&self) -> String {
        let n = self.callbacks.load(Ordering::Relaxed);
        let rate = self.rate.load(Ordering::Relaxed).max(1) as f64;
        let max = self.max_frames.load(Ordering::Relaxed);
        let avg = self.frames.load(Ordering::Relaxed) as f64 / n.max(1) as f64;
        format!(
            "{n} callbacks, avg {avg:.0} / max {max} frames = {:.1} / {:.1} ms, {} xruns",
            avg * 1000.0 / rate,
            max as f64 * 1000.0 / rate,
            self.xruns.load(Ordering::Relaxed)
        )
    }
}

type OpenStream =
    fn(Arc<Mutex<Mixer>>, Arc<AtomicBool>, Arc<Stats>, Option<u32>) -> Result<cpal::Stream>;

// Android: with cpal's `realtime` feature the AAudio stream asks for the LowLatency performance mode (fast mixer /
// MMAP path, buffer ≈ 2 bursts, tuned up by cpal on underruns) instead of the default deep-buffer path.
#[expect(
    clippy::needless_pass_by_value,
    reason = "`lost` moves into the error callback"
)]
fn open_stream(
    mixer: Arc<Mutex<Mixer>>,
    lost: Arc<AtomicBool>,
    stats: Arc<Stats>,
    buffer: Option<u32>,
) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let pick = |want: &str| {
        host.output_devices().ok()?.find(|d| {
            d.id()
                .map(|i| format!("{i:?}").contains(want))
                .unwrap_or(false)
        })
    };
    let dev = pick("pipewire")
        .or_else(|| pick("pulse"))
        .or_else(|| host.default_output_device())
        .context("no output device")?;
    let def = dev.default_output_config()?;
    let channels = def.channels() as usize;
    let mut cfg: cpal::StreamConfig = def.config();
    // Android: the device's native rate (48 kHz on most phones), else AudioFlinger resamples and the LowLatency
    // fast path is refused; the mixer resamples BGM / SE itself.
    let want = if cfg!(target_os = "android") {
        cfg.sample_rate
    } else {
        44100
    };
    if dev
        .supported_output_configs()?
        .any(|c| c.channels() == 2 && c.min_sample_rate() <= want && c.max_sample_rate() >= want)
    {
        cfg.sample_rate = want;
        cfg.channels = 2;
    }
    let channels = if cfg.channels == 2 { 2 } else { channels };
    // Android: a fixed small callback (AAudio capacity = 2 callbacks), else the legacy path keeps AudioTrack's
    // ~90 ms default buffer.
    if cfg!(target_os = "android") {
        cfg.buffer_size = cpal::BufferSize::Fixed(ANDROID_FRAMES);
    } else if let Some(n) = buffer {
        cfg.buffer_size = cpal::BufferSize::Fixed(n);
    }
    lock(&mixer).out_rate = f64::from(cfg.sample_rate);
    stats
        .rate
        .store(u64::from(cfg.sample_rate), Ordering::Relaxed);
    let cb_stats = stats.clone();
    let err_stats = stats.clone();
    let stream = dev.build_output_stream(
        cfg,
        move |out: &mut [f32], _| {
            cb_stats.on_callback((out.len() / channels) as u64);
            render_guarded(&mixer, out, channels);
        },
        move |e| {
            match e.kind() {
                cpal::ErrorKind::Xrun => {
                    err_stats.xruns.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                // No real-time thread priority (emulator, most non-system apps): informational.
                cpal::ErrorKind::RealtimeDenied => return log::info!("audio: {e}"),
                _ => {}
            }
            log::warn!("audio error: {e}");
            if matches!(
                e.kind(),
                cpal::ErrorKind::DeviceNotAvailable
                    | cpal::ErrorKind::StreamInvalidated
                    | cpal::ErrorKind::HostUnavailable
            ) {
                lost.store(true, Ordering::Relaxed);
            }
        },
        None,
    )?;
    stream.play()?;
    let burst = stream.buffer_size().unwrap_or(0);
    log::info!(
        "audio: stream {} Hz × {channels}, buffer {:?}, callback / burst {burst} frames = {:.1} ms",
        cfg.sample_rate,
        cfg.buffer_size,
        f64::from(burst) * 1000.0 / f64::from(cfg.sample_rate)
    );
    Ok(stream)
}

// th08.dat from TH08_DIR for the audio / MIDI tests (core's testdata.rs is test-only in core).
#[cfg(test)]
pub(crate) fn test_archive() -> Option<th08_formats::archive::Archive> {
    let Some(dir) = std::env::var_os("TH08_DIR").map(std::path::PathBuf::from) else {
        let t = std::thread::current();
        eprintln!(
            "skipped: no game data (TH08_DIR unset): {}",
            t.name().unwrap_or("?")
        );
        return None;
    };
    let path = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .find(|e| e.file_name().eq_ignore_ascii_case("th08.dat"))
        .map_or(dir.join("th08.dat"), |e| e.path());
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Some(th08_formats::archive::Archive::from_bytes(bytes).expect("th08.dat"))
}

// A MIDI slot index (0..=31) as the open-slot field +0x10 holds it.
#[expect(clippy::cast_possible_wrap, reason = "slots 0..=31")]
const fn slot_id(slot: usize) -> i32 {
    slot as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use th08_core::sound::Bgm;

    // Test frame halves (< 0x8000): same bits as `as i16`.
    #[expect(clippy::cast_possible_wrap, reason = "values < 0x8000")]
    fn lo16(v: usize) -> i16 {
        v as i16
    }

    #[test]
    fn se_mix_pan_and_volume() {
        let wave = Arc::new(Wave {
            samples: vec![[0.5, 0.5]; 100],
            rate: 44100.0,
        });
        let mut m = Mixer {
            volume: 1.0,
            bgm_gain: 1.0,
            se_gain: 1.0,
            out_rate: 44100.0,
            voices: vec![Voice {
                wave: Some(wave),
                ..Voice::default()
            }],
            ..Default::default()
        };
        let g = db_gain(-600);
        let v = &mut m.voices[0];
        (v.gain, v.playing) = ([g * db_gain(-1000), g], true);
        let mut out = vec![0.0; 2 * 120];
        m.render(&mut out, 2);
        assert!(
            (out[1] - 0.5 * g).abs() < 1e-6 && (out[0] - 0.5 * g * db_gain(-1000)).abs() < 1e-6
        );
        assert_eq!(out[2 * 110], 0.0);
        assert!(!m.voices[0].playing);
        assert!((db_gain(-2000) - 0.1).abs() < 1e-6 && db_gain(-10000) == 0.0);
    }

    // Port SE gain scales the voice; mute writes silence while the voice still advances.
    #[test]
    fn port_gain_and_mute() {
        let wave = Arc::new(Wave {
            samples: vec![[0.5, 0.5]; 100],
            rate: 44100.0,
        });
        let mut m = Mixer {
            volume: 1.0,
            bgm_gain: 1.0,
            se_gain: 0.5,
            out_rate: 44100.0,
            voices: vec![Voice {
                wave: Some(wave),
                gain: [1.0, 1.0],
                playing: true,
                ..Voice::default()
            }],
            ..Default::default()
        };
        let mut out = vec![0.0; 2 * 10];
        m.render(&mut out, 2);
        assert!((out[0] - 0.25).abs() < 1e-6);
        m.muted = true;
        m.render(&mut out, 2);
        assert!(out.iter().all(|&x| x == 0.0));
        assert!((m.voices[0].pos - 20.0).abs() < 1e-9);
    }

    // No game files: BGM reads fail as with an empty directory.
    struct NoFiles;
    impl th08_core::assets::DataSource for NoFiles {
        fn names(&self) -> Vec<String> {
            Vec::new()
        }
        fn fetch<'a>(
            &'a self,
            name: &'a str,
            _: Option<(u64, usize)>,
        ) -> th08_core::assets::BoxFuture<'a, Result<Vec<u8>>> {
            Box::pin(async move { anyhow::bail!("{name}: no files") })
        }
    }

    // Core BGM state plus the device; each call's commands are executed at once (as `Ctx::flush_sound`).
    struct Dev {
        b: Bgm,
        a: Audio,
    }

    impl Dev {
        fn new() -> Self {
            Self {
                b: Bgm::default(),
                a: Audio::new(Source::new(NoFiles), AudioConfig::default()),
            }
        }

        fn run(&mut self, f: impl FnOnce(&mut Bgm)) {
            f(&mut self.b);
            for c in self.b.out.drain(..) {
                self.a.exec(c);
            }
        }

        fn stopped(&self) -> bool {
            lock(&self.a.mixer).stopped
        }
    }

    // A poisoned mixer keeps taking commands; a panicking render yields silence and leaves the lock usable.
    #[test]
    fn mixer_survives_panics() {
        let mut a = Audio::new(Source::new(NoFiles), AudioConfig::default());
        let m = a.mixer.clone();
        let _ = std::thread::spawn(move || {
            let _g = m.lock().unwrap();
            panic!("poison");
        })
        .join();
        assert!(a.mixer.is_poisoned());
        a.set_muted(true);
        assert!(lock(&a.mixer).muted);
        let mut out = [1.0f32; 4];
        render_guarded(&a.mixer, &mut out, 0);
        assert_eq!(out, [0.0; 4]);
        render_guarded(&a.mixer, &mut out, 2);
    }

    // A lost stream is reopened while playing, at most once per second; never while paused or before the first start.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn lost_stream_reopens_while_playing() {
        use std::time::{Duration, Instant};
        thread_local!(static OPENS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) });
        fn fake(
            _: Arc<Mutex<Mixer>>,
            _: Arc<AtomicBool>,
            _: Arc<Stats>,
            _: Option<u32>,
        ) -> Result<cpal::Stream> {
            OPENS.with(|n| n.set(n.get() + 1));
            anyhow::bail!("no device")
        }
        let opens = || OPENS.with(std::cell::Cell::get);
        let mut a = Audio::new(Source::new(NoFiles), AudioConfig::default());
        a.open_stream = fake;
        let t = Instant::now();
        a.lost.store(true, Ordering::Relaxed);
        a.reopen_if_lost(t);
        assert_eq!(opens(), 0);
        a.started = true;
        a.paused = true;
        a.reopen_if_lost(t);
        assert_eq!(opens(), 0);
        a.paused = false;
        a.reopen_if_lost(t);
        assert_eq!(opens(), 1);
        assert!(!a.lost.load(Ordering::Relaxed));
        a.reopen_if_lost(t + Duration::from_millis(500));
        assert_eq!(opens(), 1);
        a.reopen_if_lost(t + Duration::from_secs(1));
        assert_eq!(opens(), 2);
    }

    // The offline mix yields exactly ⌊n × rate / 60⌋ sample frames after n game frames (735 per frame at 44.1 kHz),
    // carries a playing SE across frames and never opens a stream.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn offline_mix_sample_count() {
        for rate in [44100u32, 48000, 22050, 32000, 44101] {
            let (mut a, mut m) = Audio::offline(Source::new(NoFiles), AudioConfig::default(), rate);
            a.ensure_started();
            assert!(a.is_started() && a.stream.is_none());
            let mut out = Vec::new();
            let n = 10_007u64;
            for k in 0..n {
                let before = out.len();
                m.frame(&mut out);
                if rate == 44100 {
                    assert_eq!(out.len() - before, 2 * 735, "frame {k}");
                }
            }
            assert_eq!(m.frames(), n);
            assert_eq!(
                out.len() as u64,
                2 * (n * u64::from(rate) / 60),
                "rate {rate}"
            );
        }
        let (mut a, mut m) = Audio::offline(Source::new(NoFiles), AudioConfig::default(), 44100);
        lock(&a.mixer).voices = vec![Voice {
            wave: Some(Arc::new(Wave {
                samples: vec![[0.5, 0.5]; 1000],
                rate: 44100.0,
            })),
            ..Voice::default()
        }];
        a.exec(SoundCmd::Se {
            id: 0,
            pan: 0,
            vol: 0,
        });
        let mut out = Vec::new();
        m.frame(&mut out);
        m.frame(&mut out);
        let heard = out
            .as_chunks::<2>()
            .0
            .iter()
            .filter(|f| f[0] != 0.0)
            .count();
        assert_eq!(heard, 1000);
    }

    // A configured SoundFont that is missing or does not parse leaves MIDI not ready; a valid one makes it ready.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn midi_available_needs_a_loadable_soundfont() {
        let dir = std::env::temp_dir().join(format!("th08-sf2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (bad, good) = (dir.join("bad.sf2"), dir.join("good.sf2"));
        std::fs::write(&bad, b"not a soundfont").unwrap();
        std::fs::write(&good, crate::midi::tests::tiny_sf2()).unwrap();
        let ready = |p: &std::path::Path| {
            let cfg = AudioConfig {
                soundfont: Some(p.to_path_buf()),
                ..AudioConfig::default()
            };
            Audio::new(Source::new(NoFiles), cfg).midi_available()
        };
        let (missing, bad, good) = (ready(&dir.join("gone.sf2")), ready(&bad), ready(&good));
        std::fs::remove_dir_all(&dir).ok();
        assert!(!missing && !bad && good);
    }

    // Mode-2 calls (02 §5): Midi_LoadSlot, PlaySlot (stop, open, play), Midi_Fade in ms without speed division,
    // Music_Stop, and the init.mid reset from slot 30.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn midi_mode_calls() {
        let Some(arc) = test_archive() else {
            return;
        };
        let mut d = Dev::new();
        d.a.load_midi(th08_core::content::LogicView::new(arc).present());
        assert!(d.a.midi_slots[30].is_some());
        let sf = crate::midi::tests::tiny_sf2();
        lock(&d.a.mixer).font = Some(Arc::new(rustysynth::SoundFont::new(&mut &sf[..]).unwrap()));
        d.b.midi_ready = true;
        let mut cfg = th08_core::config::Config {
            music: 2,
            ..th08_core::config::Config::default()
        };
        d.run(|b| b.set_volumes(&cfg));
        d.run(|b| b.load_bgm(1, "bgm/th08_03.mid"));
        d.run(|b| b.load_bgm(2, "bgm/th08_02.mid"));
        // The MIDI path leaves the WAV slots empty (sound.rs `midi_mode_bgm_side`).
        assert!(d.a.midi_slots[1].is_some() && d.a.midi_slots[2].is_none());
        d.run(|b| b.play_slot(1));
        assert_eq!(d.a.midi_open, 1);
        let playing = |a: &Audio| lock(&a.mixer).midi.as_ref().unwrap().playing;
        assert!(playing(&d.a));
        d.b.fade_bgm(3.0, 0.5);
        assert!(matches!(d.b.out[..], [SoundCmd::Midi(MidiCmd::Fade(3000))]));
        d.run(|_| {});
        // Loading the open slot stops it first.
        d.run(|b| b.load_bgm(1, "bgm/th08_04.mid"));
        assert!(!playing(&d.a) && d.a.midi_open == -1);
        d.run(|b| b.play_bgm("bgm/th08_01.mid"));
        assert!(playing(&d.a) && d.a.midi_open == 31 && d.a.midi_slots[31].is_none());
        d.run(Bgm::stop_bgm);
        assert!(!playing(&d.a));
        d.run(Bgm::midi_reset);
        assert!(playing(&d.a) && d.a.midi_open == 30);
        // A null slot (th08_02.mid missing): Midi_Open frees the tracks, open slot unchanged; Play / Stop do nothing.
        d.run(|b| b.play_slot(2));
        assert!(!playing(&d.a) && d.a.midi_open == -1);
        d.a.midi_open = 5;
        d.run(Bgm::stop_bgm);
        assert_eq!(d.a.midi_open, 5);
        // Option Reset from mode 2 (15 §1.3 row 7): mode 1 with no music call; WAV plays never stop the MIDI song.
        d.run(|b| b.play_slot(1));
        cfg.music = 1;
        d.run(|b| b.set_volumes(&cfg));
        d.run(|b| b.play_bgm("bgm/th08_01.mid"));
        d.run(Bgm::stop_bgm);
        assert!(playing(&d.a) && d.a.midi_open == 1);
    }

    // Command 6 keeps the fade ticking, a mode-1 fade end stops the buffer (position and last volume kept) and
    // command 7 resumes it there; Music_Stop rewinds. Mode 2 ignores 6/7. Bgm side: sound.rs `wav_pause_bgm_side`.
    #[test]
    fn wav_pause_and_fade() {
        let mut d = Dev::new();
        let mut music = Music::new(Pcm::from_frames(&[[16384, 16384]; 10]), 0);
        music.cursor = 3.0;
        lock(&d.a.mixer).music = Some(music);
        d.run(|b| {
            b.fade_bgm(4.0, 1.0);
            b.start_fade();
            b.pause_bgm(true);
        });
        (0..240).for_each(|_| d.run(Bgm::tick_fade));
        d.run(|b| b.pause_bgm(false));
        {
            let m = lock(&d.a.mixer);
            assert!(!m.stopped && m.music.as_ref().unwrap().cursor == 3.0);
        }
        d.run(|b| {
            b.fade_bgm(4.0, 1.0);
            b.start_fade();
            b.stop_bgm();
        });
        {
            let m = lock(&d.a.mixer);
            assert!(m.stopped && m.music.as_ref().unwrap().cursor == 0.0);
        }
        let cfg = th08_core::config::Config {
            music: 0,
            ..th08_core::config::Config::default()
        };
        d.run(|b| {
            b.set_volumes(&cfg);
            b.pause_bgm(false);
        });
        assert!(d.stopped());
    }
    // Renders `frames` at 44.1 kHz (step 1, no interpolation) and decodes the track frame each sample encodes.
    #[expect(clippy::cast_sign_loss, reason = "encoded frame halves are ≥ 0")]
    fn heard(m: &mut Mixer, frames: usize) -> Vec<usize> {
        let mut out = vec![0.0; 2 * frames];
        m.render(&mut out, 2);
        let s = |v: f32| (v * 32768.0).round() as usize;
        out.as_chunks::<2>()
            .0
            .iter()
            .map(|f| s(f[0]) | s(f[1]) << 15)
            .collect()
    }

    fn ring_mixer(len: usize, loop_start: usize) -> Mixer {
        // Frame index i encoded as (low 15 bits, high bits) in the two s16 channels.
        let samples: Vec<_> = (0..len)
            .map(|i| [lo16(i & 0x7FFF), lo16(i >> 15)])
            .collect();
        let samples = Pcm::from_frames(&samples);
        Mixer {
            volume: 1.0,
            bgm_gain: 1.0,
            se_gain: 1.0,
            out_rate: BGM_RATE,
            music: Some(Music::new(samples, loop_start)),
            ..Default::default()
        }
    }

    // Ring + UnPause after `Music_Stop`: stop in chunk P at song time t (start of chunk P), UnPause plays
    // from ring position 0 and the song continues seamlessly from t + ((16 − P) mod 16)·0.25 s; P = 1 plays 0.25 s of
    // t + 3.75 s, then t on.
    #[test]
    fn unpause_after_stop_resumes_ring() {
        for (played, p) in [
            (37 * CHUNK + 500, 5),
            (32 * CHUNK + 9, 0),
            (17 * CHUNK + 3000, 1),
        ] {
            let mut m = ring_mixer(300_000, 50_000);
            let before = heard(&mut m, played);
            assert!(
                before
                    .windows(2)
                    .all(|w| w[1] == w[0] + 1 || w[1] == 50_000)
            );
            assert_eq!(m.music.as_ref().unwrap().refill, p);
            let t = m
                .music
                .as_ref()
                .unwrap()
                .track_frame((played - played % CHUNK) as u64);
            // Command 3 then command 7.
            m.stopped = true;
            m.music.as_mut().unwrap().cursor = 0.0;
            assert!(heard(&mut m, 100).iter().all(|&v| v == 0));
            m.stopped = false;
            let after = heard(&mut m, 3 * RING);
            let mm = m.music.as_ref().unwrap();
            let song = |s: usize| mm.track_frame((played - played % CHUNK + s) as u64);
            if p == 1 {
                assert_eq!(after[0], song(15 * CHUNK));
                assert_eq!(after[CHUNK - 1], song(16 * CHUNK - 1));
                assert!((CHUNK..3 * RING).all(|i| after[i] == song(i - CHUNK)));
            } else {
                let s0 = (16 - p) % 16 * CHUNK;
                assert_eq!(after[0], song(s0));
                assert!((0..3 * RING).all(|i| after[i] == song(s0 + i)));
            }
            assert_eq!(
                after[0],
                mm.track_frame(t as u64 + ((16 - p) % 16 * CHUNK) as u64)
            );
        }
    }

    // The ring keeps streaming through the loop point and `Music_Stop` keeps the stream object.
    #[test]
    fn ring_loops_and_stop_keeps_object() {
        let mut m = ring_mixer(200_000, 150_000);
        let v = heard(&mut m, 260_000);
        assert_eq!(
            (v[199_999], v[200_000], v[249_999], v[250_000]),
            (199_999, 150_000, 199_999, 150_000)
        );
        let mut d = Dev::new();
        lock(&d.a.mixer).music = m.music.take();
        d.run(|b| {
            b.stop_bgm();
            b.pause_bgm(false);
        });
        assert!(!d.stopped());
        d.run(Bgm::release_bgm);
        assert!(lock(&d.a.mixer).music.is_none());
    }

    #[test]
    fn se_waves_parse() {
        let Some(a) = test_archive() else {
            return;
        };
        for (name, _) in th08_core::sound::TABLE {
            let data = a.read(&format!("{name}.wav")).unwrap();
            let w = super::parse_wav(&data).unwrap();
            assert!(!w.samples.is_empty() && w.rate > 8000.0, "{name}");
            let r = parse_wav_reference(&data).unwrap();
            assert_eq!(w.rate, r.rate, "{name}");
            assert!(
                w.samples.len() == r.samples.len()
                    && w.samples.iter().zip(&r.samples).all(|(a, b)| {
                        a[0].to_bits() == b[0].to_bits() && a[1].to_bits() == b[1].to_bits()
                    }),
                "{name}: hound samples differ from the RIFF parser"
            );
        }
    }

    // Reference RIFF/WAVE parser for the hound-based `se_waves_parse`.
    fn parse_wav_reference(data: &[u8]) -> Result<Wave> {
        anyhow::ensure!(
            data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WAVE",
            "not a RIFF/WAVE file"
        );
        let (mut fmt, mut pcm) = (None, None);
        let mut p = 12;
        while p + 8 <= data.len() {
            let len = u32::from_le_bytes(data[p + 4..p + 8].try_into().unwrap()) as usize;
            let body = &data[p + 8..(p + 8 + len).min(data.len())];
            match &data[p..p + 4] {
                b"fmt " if body.len() >= 16 => fmt = Some(body),
                b"data" => pcm = Some(body),
                _ => {}
            }
            p += 8 + len + (len & 1);
        }
        let (fmt, pcm) = (
            anyhow::Context::context(fmt, "no fmt chunk")?,
            anyhow::Context::context(pcm, "no data chunk")?,
        );
        let u16_at = |o: usize| u16::from_le_bytes([fmt[o], fmt[o + 1]]);
        let (tag, channels, rate, bits) = (
            u16_at(0),
            u16_at(2) as usize,
            u32::from_le_bytes(fmt[4..8].try_into().unwrap()),
            u16_at(14),
        );
        anyhow::ensure!(
            tag == 1 && (1..=2).contains(&channels) && (bits == 8 || bits == 16),
            "unsupported wav format {tag}/{channels}/{bits}"
        );
        let bytes = bits as usize / 8;
        let sample = |c: &[u8]| {
            if bytes == 1 {
                (f32::from(c[0]) - 128.0) / 128.0
            } else {
                f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0
            }
        };
        let samples = pcm
            .chunks_exact(bytes * channels)
            .map(|f| {
                let l = sample(f);
                [
                    l,
                    if channels == 2 {
                        sample(&f[bytes..])
                    } else {
                        l
                    },
                ]
            })
            .collect();
        Ok(Wave {
            samples,
            rate: f64::from(rate),
        })
    }
}
