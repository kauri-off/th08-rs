//! Port audio back end (cpal): thbgm.dat BGM streaming (02 §1–§4), MIDI music mode (02 §5, `midi.rs`) and the SE
//! voices DirectSound plays (13 §1.1, §1.3).
#![allow(clippy::disallowed_methods)]

use crate::assets::Source;
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// thbgm.fmt WAVEFORMATEX: 44100 Hz, stereo, 16-bit for every track (02 §2).
const BGM_RATE: f64 = 44100.0;

use crate::sound::{MidiCmd, SoundCmd, Track, WavCmd};

// Streaming ring (02 §3): 16 notification chunks of 44100 bytes = 11025 stereo frames (0.25 s), 4.0 s in all.
const CHUNK: usize = 11025;
const RING: usize = 16 * CHUNK;

// The playing BGM: the whole stored track in memory, heard through a model of the DirectSound streaming ring
// (02 §3) so that `Bgm_StreamStop` + UnPause replays what the ring still holds (02 §4).
struct Music {
    samples: Vec<[f32; 2]>,
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
    // Play command fill stage (02 §3): rewind, fill the whole ring with the first 4.0 s, +0x64 := 0, play from 0.
    fn new(samples: Vec<[f32; 2]>, loop_start: usize) -> Self {
        Self {
            samples,
            loop_start,
            chunks: std::array::from_fn(|k| (k * CHUNK) as u64),
            refill: 0,
            file: RING as u64,
            cursor: 0.0,
        }
    }

    // Stream frame → track frame: `loop_end` frames first, then loop_start..loop_end forever (02 §3).
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
        self.samples[self.track_frame(self.chunks[r / CHUNK] + (r % CHUNK) as u64)]
    }

    // Notification as the play cursor leaves a chunk (handler 0x472D30, 02 §3). The write cursor then lies in the
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

// One se_*.wav PCM wave file (RIFF, 8/16-bit, mono/stereo; 04 file index, 13 §1.1).
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
        .map(|v| v.map(|v| v as f32 / scale))
        .collect::<Result<Vec<f32>, _>>()?;
    let samples = pcm
        .chunks_exact(channels)
        .map(|f| [f[0], f[channels - 1]])
        .collect();
    Ok(Wave {
        samples,
        rate: spec.sample_rate as f64,
    })
}

// One SE id's DirectSound buffer (13 §1.1): a non-looping copy of its wave; gain holds volume and pan per channel.
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

#[derive(Default)]
struct Mixer {
    voices: Vec<Voice>,
    music: Option<Music>,
    generation: u32,
    out_rate: f64,
    volume: f32,
    // Streaming object +0x30 playing flag cleared by `Stop` with the position kept (command 6, mode-1 fade end, 02 §4).
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

    fn render(&mut self, out: &mut [f32], channels: usize) {
        let step = BGM_RATE / self.out_rate;
        #[cfg(not(target_arch = "wasm32"))]
        let n = out.len() / channels;
        #[cfg(not(target_arch = "wasm32"))]
        if self.midi.is_some() {
            self.midi();
        }
        #[cfg(not(target_arch = "wasm32"))]
        let midi = match &mut self.midi {
            Some(p) => Some(p.render(n)),
            None => None,
        };
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
            for vc in self.voices.iter_mut().filter(|v| v.playing) {
                let Some(w) = &vc.wave else { continue };
                let i = vc.pos as usize;
                if i >= w.samples.len() {
                    vc.playing = false;
                    continue;
                }
                let s = w.samples[i];
                l += s[0] * vc.gain[0];
                r += s[1] * vc.gain[1];
                vc.pos += w.rate / self.out_rate;
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

// SoundFont lookup for MIDI music mode (02 §5), from the app's / tool's settings (`soundfont`, the platform's
// soundfont dirs); read once at load by `midi_available`.
#[derive(Clone, Debug, Default)]
pub struct AudioConfig {
    pub soundfont: Option<std::path::PathBuf>,
    pub soundfont_dirs: Vec<std::path::PathBuf>,
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
    mixer: Arc<Mutex<Mixer>>,
    stream: Option<cpal::Stream>,
    // MIDI object (02 §5): the archive's .mid members by lower-case basename, the 32 file slots +0x98 and the open
    // slot +0x10 (−1 none).
    midi_files: HashMap<String, Arc<[u8]>>,
    midi_slots: [Option<Arc<[u8]>>; 32],
    midi_open: i32,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    cfg: AudioConfig,
    // Port: SoundFont found by `midi_available`, loaded on the first MIDI command.
    #[cfg(not(target_arch = "wasm32"))]
    font_path: Option<std::path::PathBuf>,
}

impl Audio {
    pub fn new(source: Source, cfg: AudioConfig) -> Self {
        let mixer = Arc::new(Mutex::new(Mixer {
            volume: 1.0,
            out_rate: BGM_RATE,
            ..Default::default()
        }));
        Self {
            source,
            mixer,
            stream: None,
            midi_files: HashMap::new(),
            midi_slots: Default::default(),
            midi_open: -1,
            cfg,
            #[cfg(not(target_arch = "wasm32"))]
            font_path: None,
        }
    }

    // Host input for `sound::Bgm::midi_ready`: a SoundFont (`AudioConfig::soundfont`, else its dirs) exists, so
    // music mode 2 plays MIDI; else it takes the WAV path (port).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn midi_available(&mut self) -> bool {
        self.font_path =
            crate::midi::find_soundfont(self.cfg.soundfont.as_deref(), &self.cfg.soundfont_dirs)
                .filter(|p| p.is_file());
        match &self.font_path {
            Some(p) => log::info!("midi: soundfont {}", p.display()),
            None => log::info!(
                "midi: no SoundFont found (set soundfont / TH08_SOUNDFONT); music mode MIDI plays the WAV tracks"
            ),
        }
        self.font_path.is_some()
    }

    #[cfg(target_arch = "wasm32")]
    pub fn midi_available(&mut self) -> bool {
        false
    }

    // Port: the SoundFont is loaded on the first MIDI command; a file that fails to load leaves MIDI silent.
    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_font(&mut self) {
        let Some(p) = self.font_path.take() else {
            return;
        };
        match crate::midi::load_soundfont(&p) {
            Ok(f) => self.mixer.lock().unwrap().font = Some(f),
            Err(e) => log::warn!("midi: {e:#}; MIDI music is silent"),
        }
    }

    // Executes one command of the frame's `SoundCmd` stream (call order = the original's).
    pub fn exec(&mut self, c: SoundCmd) {
        match c {
            SoundCmd::Se { id, pan, vol } => self.play_se(id, pan, vol),
            SoundCmd::Wav(c) => match c {
                WavCmd::Play(t) => self.play_wav(t),
                WavCmd::Stop => self.stop_wav(),
                WavCmd::Pause(on) => self.mixer.lock().unwrap().stopped = on,
                WavCmd::Release => self.release_bgm(),
                WavCmd::Volume(db) => self.mixer.lock().unwrap().volume = db_gain(db),
            },
            SoundCmd::Midi(c) => {
                #[cfg(not(target_arch = "wasm32"))]
                self.ensure_font();
                match c {
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
                        if let Some(p) = self.mixer.lock().unwrap().midi() {
                            p.seq.fade(ms);
                        }
                        #[cfg(target_arch = "wasm32")]
                        let _ = ms;
                    }
                }
            }
        }
    }

    // The archive's `bgm/*.mid` members (02 §5), then the boot `Midi_LoadSlot(30, "bgm/init.mid")`.
    pub fn load_midi(&mut self, archive: crate::content::PresentView<'_>) {
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
    // found → null slot (02 §5).
    fn midi_load_slot(&mut self, slot: usize, name: &str) {
        if slot as i32 == self.midi_open {
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
    // and leaves the open slot unchanged (02 §5).
    fn midi_open(&mut self, slot: usize) {
        let song = self.midi_slots.get(slot).cloned().flatten().and_then(|d| {
            crate::midi::parse(&d)
                .map_err(|e| log::warn!("midi slot {slot}: {e:#}"))
                .ok()
        });
        if song.is_some() {
            self.midi_open = slot as i32;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = self.mixer.lock().unwrap().midi() {
            p.seq.open(song);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = song;
    }

    // Midi_Play 0x444620: Midi_Rewind, then the 1 ms timer starts; nothing without parsed tracks.
    fn midi_play(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = self.mixer.lock().unwrap().midi()
            && p.seq.has_tracks()
        {
            p.seq.rewind();
            p.playing = true;
        }
    }

    // Midi_Stop 0x444680: timer killed, midiOutReset (all notes off), open slot := −1; nothing without parsed tracks.
    fn midi_stop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = self.mixer.lock().unwrap().midi.as_mut() {
            if !p.seq.has_tracks() {
                return;
            }
            p.playing = false;
            p.synth.reset_notes();
        }
        self.midi_open = -1;
    }

    pub fn is_started(&self) -> bool {
        self.stream.is_some()
    }

    // Must be called from a user gesture on the web (AudioContext policy).
    pub fn ensure_started(&mut self) {
        if self.stream.is_none() {
            match open_stream(self.mixer.clone()) {
                Ok(s) => self.stream = Some(s),
                Err(e) => log::warn!("audio disabled: {e:#}"),
            }
        }
    }

    // Play command (02 §3): the stored part of the track (never past loop_end), read in the background.
    fn play_wav(&mut self, t: Track) {
        let generation = {
            let mut m = self.mixer.lock().unwrap();
            m.generation += 1;
            m.music = None;
            m.stopped = false;
            m.generation
        };
        let (source, mixer) = (self.source.clone(), self.mixer.clone());
        spawn(async move {
            match source.read_range("thbgm.dat", t.offset, t.length * 4).await {
                Ok(buf) => {
                    let samples = buf
                        .chunks_exact(4)
                        .map(|c| {
                            let s =
                                |o: usize| i16::from_le_bytes([c[o], c[o + 1]]) as f32 / 32768.0;
                            [s(0), s(2)]
                        })
                        .collect();
                    let mut m = mixer.lock().unwrap();
                    if m.generation == generation {
                        m.music = Some(Music::new(samples, t.loop_start));
                    }
                }
                Err(e) => log::warn!("bgm {}: {e:#}", t.name),
            }
        });
    }

    // Sound_LoadSE 0x45D3F0 (13 §1.1): one buffer per SE id, each a copy of its wave file; the table volume is not
    // stored because Sound_Update overwrites it on every play (13 §1.3).
    pub fn load_se(&mut self, archive: crate::content::PresentView<'_>) {
        let mut waves: Vec<(&str, Option<Arc<Wave>>)> = Vec::new();
        let mut voices = Vec::new();
        for (name, _) in crate::sound::TABLE {
            let w = match waves.iter().find(|(n, _)| *n == name) {
                Some((_, w)) => w.clone(),
                None => {
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
                }
            };
            voices.push(Voice {
                wave: w,
                ..Voice::default()
            });
        }
        self.mixer.lock().unwrap().voices = voices;
    }

    // Sound_Update per queued id (13 §1.3): stop, rewind, set pan/volume (1/100 dB) and play once.
    // Pan uses the DirectSound rule: the opposite channel is attenuated by |pan| (1/100 dB).
    fn play_se(&mut self, id: usize, pan: i32, volume: i32) {
        let mut m = self.mixer.lock().unwrap();
        let Some(v) = m.voices.get_mut(id) else {
            return;
        };
        let g = db_gain(volume);
        let pan = pan.clamp(-10000, 10000);
        v.gain = [g * db_gain(-pan.max(0)), g * db_gain(pan.min(0))];
        v.pos = 0.0;
        v.playing = v.wave.is_some();
    }

    // `Bgm_StreamStop` 0x4728E0 (02 §4): `Stop` + position 0, playing flag 0; the object, ring, +0x64 and file
    // position stay, so a later UnPause plays the ring on from position 0 (02 §4 "UnPause after `Music_Stop`").
    fn stop_wav(&mut self) {
        let mut m = self.mixer.lock().unwrap();
        m.stopped = true;
        if let Some(music) = &mut m.music {
            music.cursor = 0.0;
        }
    }

    // Sound manager release on restart (00 §2): the streaming object is gone; nothing is left to UnPause.
    fn release_bgm(&mut self) {
        let mut m = self.mixer.lock().unwrap();
        m.generation += 1;
        m.music = None;
    }
}

impl crate::game::AudioOut for Audio {
    fn load(&mut self, archive: crate::content::PresentView<'_>) {
        self.load_se(archive);
        self.load_midi(archive);
    }
    fn midi_available(&mut self) -> bool {
        Audio::midi_available(self)
    }
    fn exec(&mut self, c: SoundCmd) {
        Audio::exec(self, c)
    }
    fn is_started(&self) -> bool {
        Audio::is_started(self)
    }
    fn ensure_started(&mut self) {
        Audio::ensure_started(self)
    }
}

// Port: output device setup (prefers 44.1 kHz stereo so the BGM needs no resampling).
fn open_stream(mixer: Arc<Mutex<Mixer>>) -> Result<cpal::Stream> {
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
    if dev
        .supported_output_configs()?
        .any(|c| c.channels() == 2 && c.min_sample_rate() <= 44100 && c.max_sample_rate() >= 44100)
    {
        cfg.sample_rate = 44100;
        cfg.channels = 2;
    }
    let channels = if cfg.channels == 2 { 2 } else { channels };
    mixer.lock().unwrap().out_rate = cfg.sample_rate as f64;
    let stream = dev.build_output_stream(
        cfg,
        move |out: &mut [f32], _| mixer.lock().unwrap().render(out, channels),
        |e| log::warn!("audio error: {e}"),
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

// th08.dat from TH08_DIR for the audio / MIDI tests (core's testdata.rs is test-only in core).
#[cfg(test)]
pub(crate) fn test_archive() -> Option<crate::archive::Archive> {
    let dir = std::path::PathBuf::from(std::env::var_os("TH08_DIR")?);
    let path = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .find(|e| e.file_name().eq_ignore_ascii_case("th08.dat"))
        .map_or(dir.join("th08.dat"), |e| e.path());
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Some(crate::archive::Archive::from_bytes(bytes).expect("th08.dat"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound::Bgm;

    #[test]
    fn se_mix_pan_and_volume() {
        let wave = Arc::new(Wave {
            samples: vec![[0.5, 0.5]; 100],
            rate: 44100.0,
        });
        let mut m = Mixer {
            volume: 1.0,
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
    // No game files: BGM reads fail as with an empty directory.
    struct NoFiles;
    impl crate::assets::DataSource for NoFiles {
        fn names(&self) -> Vec<String> {
            Vec::new()
        }
        fn fetch<'a>(
            &'a self,
            name: &'a str,
            _: Option<(u64, usize)>,
        ) -> crate::assets::BoxFuture<'a, Result<Vec<u8>>> {
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
            self.a.mixer.lock().unwrap().stopped
        }
    }

    // 02 §5 calls in mode 2: Midi_LoadSlot, PlaySlot (stop, open, play), Midi_Fade in ms without speed division,
    // Music_Stop, and the init.mid reset from slot 30.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn midi_mode_calls() {
        let Some(arc) = test_archive() else {
            return;
        };
        let mut d = Dev::new();
        d.a.load_midi(crate::content::LogicView::new(arc).present());
        assert!(d.a.midi_slots[30].is_some());
        let sf = crate::midi::tests::tiny_sf2();
        d.a.mixer.lock().unwrap().font =
            Some(Arc::new(rustysynth::SoundFont::new(&mut &sf[..]).unwrap()));
        d.b.midi_ready = true;
        let mut cfg = crate::config::Config::default();
        cfg.music = 2;
        d.run(|b| b.set_volumes(&cfg));
        d.run(|b| b.load_bgm(1, "bgm/th08_03.mid"));
        d.run(|b| b.load_bgm(2, "bgm/th08_02.mid"));
        // The MIDI path leaves the WAV slots empty (sound.rs `midi_mode_bgm_side`).
        assert!(d.a.midi_slots[1].is_some() && d.a.midi_slots[2].is_none());
        d.run(|b| b.play_slot(1));
        assert_eq!(d.a.midi_open, 1);
        let playing = |a: &Audio| a.mixer.lock().unwrap().midi.as_ref().unwrap().playing;
        assert!(playing(&d.a));
        d.b.fade_bgm(3.0, 0.5);
        assert!(matches!(d.b.out[..], [SoundCmd::Midi(MidiCmd::Fade(3000))]));
        d.run(|_| {});
        // Loading the open slot stops it first.
        d.run(|b| b.load_bgm(1, "bgm/th08_04.mid"));
        assert!(!playing(&d.a) && d.a.midi_open == -1);
        d.run(|b| b.play_bgm("bgm/th08_01.mid"));
        assert!(playing(&d.a) && d.a.midi_open == 31 && d.a.midi_slots[31].is_none());
        d.run(|b| b.stop_bgm());
        assert!(!playing(&d.a));
        d.run(|b| b.midi_reset());
        assert!(playing(&d.a) && d.a.midi_open == 30);
        // A null slot (th08_02.mid missing): Midi_Open frees the tracks, open slot unchanged; Play / Stop do nothing.
        d.run(|b| b.play_slot(2));
        assert!(!playing(&d.a) && d.a.midi_open == -1);
        d.a.midi_open = 5;
        d.run(|b| b.stop_bgm());
        assert_eq!(d.a.midi_open, 5);
        // Option Reset from mode 2 (15 §1.3 row 7): mode 1 with no music call; WAV plays never stop the MIDI song.
        d.run(|b| b.play_slot(1));
        cfg.music = 1;
        d.run(|b| b.set_volumes(&cfg));
        d.run(|b| b.play_bgm("bgm/th08_01.mid"));
        d.run(|b| b.stop_bgm());
        assert!(playing(&d.a) && d.a.midi_open == 1);
    }

    // 02 §4 command 6 keeps the fade ticking, a mode-1 fade end stops the buffer (position and last volume kept) and
    // command 7 resumes it there; Music_Stop rewinds. Mode 2 ignores 6/7. Bgm side: sound.rs `wav_pause_bgm_side`.
    #[test]
    fn wav_pause_and_fade() {
        let mut d = Dev::new();
        let mut music = Music::new(vec![[0.5, 0.5]; 10], 0);
        music.cursor = 3.0;
        d.a.mixer.lock().unwrap().music = Some(music);
        d.run(|b| {
            b.fade_bgm(4.0, 1.0);
            b.start_fade();
            b.pause_bgm(true);
        });
        (0..240).for_each(|_| d.run(|b| b.tick_fade()));
        d.run(|b| b.pause_bgm(false));
        {
            let m = d.a.mixer.lock().unwrap();
            assert!(!m.stopped && m.music.as_ref().unwrap().cursor == 3.0);
        }
        d.run(|b| {
            b.fade_bgm(4.0, 1.0);
            b.start_fade();
            b.stop_bgm();
        });
        {
            let m = d.a.mixer.lock().unwrap();
            assert!(m.stopped && m.music.as_ref().unwrap().cursor == 0.0);
        }
        let mut cfg = crate::config::Config::default();
        cfg.music = 0;
        d.run(|b| {
            b.set_volumes(&cfg);
            b.pause_bgm(false);
        });
        assert!(d.stopped());
    }
    // Renders `frames` at 44.1 kHz (step 1, no interpolation) and decodes the track frame each sample encodes.
    fn heard(m: &mut Mixer, frames: usize) -> Vec<usize> {
        let mut out = vec![0.0; 2 * frames];
        m.render(&mut out, 2);
        out.chunks_exact(2)
            .map(|f| (f[0] * 1e6).round() as usize)
            .collect()
    }

    fn ring_mixer(len: usize, loop_start: usize) -> Mixer {
        let samples = (0..len).map(|i| [i as f32 / 1e6; 2]).collect();
        Mixer {
            volume: 1.0,
            out_rate: BGM_RATE,
            music: Some(Music::new(samples, loop_start)),
            ..Default::default()
        }
    }

    // 02 §3 ring + 02 §4 "UnPause after `Music_Stop`": stop in chunk P at song time t (start of chunk P), UnPause plays
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

    // The ring keeps streaming through the loop point (02 §3) and `Audio::stop_bgm` keeps the stream object.
    // The ring keeps streaming through the loop point (02 §3) and `Music_Stop` keeps the stream object.
    #[test]
    fn ring_loops_and_stop_keeps_object() {
        let mut m = ring_mixer(200_000, 150_000);
        let v = heard(&mut m, 260_000);
        assert_eq!(
            (v[199_999], v[200_000], v[249_999], v[250_000]),
            (199_999, 150_000, 199_999, 150_000)
        );
        let mut d = Dev::new();
        d.a.mixer.lock().unwrap().music = m.music.take();
        d.run(|b| {
            b.stop_bgm();
            b.pause_bgm(false);
        });
        assert!(!d.stopped());
        d.run(|b| b.release_bgm());
        assert!(d.a.mixer.lock().unwrap().music.is_none());
    }

    #[test]
    fn se_waves_parse() {
        let Some(a) = test_archive() else {
            return;
        };
        for (name, _) in crate::sound::TABLE {
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

    // The pre-hound RIFF/WAVE parser, kept as the reference for se_waves_parse.
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
                (c[0] as f32 - 128.0) / 128.0
            } else {
                i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0
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
            rate: rate as f64,
        })
    }
}
